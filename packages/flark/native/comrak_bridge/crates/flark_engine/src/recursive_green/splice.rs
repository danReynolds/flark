//! Exact-lineage local mutation for packed recursive Green storage.

use std::ops::Range;

use crate::document::{DocumentRuntime, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness};
use crate::measured_sequence::{
    begin_measured_sequence_seal, splice_measured_sequence_atomic, MeasuredSequenceBuildRoot,
    ResumableMeasuredSequenceBuilder, ResumableSequenceProgress, SequenceInspectionReceipt,
    SequenceMutationReceipt,
};
use crate::source::SourceSnapshotLease;
use crate::storage::{ArenaBuildSession, PageArena, ARENA_PAGE_BYTES};

use super::build::{
    allocate_recursive_green_identity, M11RecursiveGreenBuildReceipt, M11RecursiveGreenRoot,
};
use super::codec::{
    decode_leaf, decode_packed_event, encode_leaf_header, encode_packed_event, packed_event_len,
    packed_event_summary, LogicalAtom, M11RecursiveGreenCoveragePart, M11RecursiveGreenError,
    M11RecursiveGreenLogicalAction, M11RecursiveGreenSourceMetric, PackedGreenEvent,
    RecursiveGreenSpec, RecursiveGreenSummary, GREEN_EVENTS_PER_PAGE_MAX, GREEN_LEAF_HEADER_BYTES,
};

type GreenSequenceBuilder = ResumableMeasuredSequenceBuilder<RecursiveGreenSpec>;
pub(super) type GreenSequenceBuildRoot = MeasuredSequenceBuildRoot<RecursiveGreenSpec>;

/// Exact work performed by one coverage-only recursive-Green splice.
///
/// This fast path cannot alter tree shape: it replaces one complete persisted
/// coverage atom while retaining every Enter, Property, Retype, and Exit event.
/// Structural edits deliberately use a wider parser restart/convergence cut.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11RecursiveGreenCoverageSpliceReceipt {
    base_events: u64,
    replacement_coverage_events: u64,
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
    branch_payload_bytes: usize,
    maximum_atomic_height: u16,
    seal_transitions: usize,
    lineage_transitions: usize,
}

impl M11RecursiveGreenCoverageSpliceReceipt {
    #[must_use]
    pub const fn base_events(self) -> u64 {
        self.base_events
    }
    #[must_use]
    pub const fn replacement_coverage_events(self) -> u64 {
        self.replacement_coverage_events
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
    pub const fn branch_payload_bytes(self) -> usize {
        self.branch_payload_bytes
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
}

struct CoverageSplicePlan {
    storage_page_ordinal: u64,
    events: Vec<PackedGreenEvent>,
    replacement_index: usize,
    owner_depth: u32,
    part: M11RecursiveGreenCoveragePart,
    boundary_events_decoded: u64,
    inspection: SequenceInspectionReceipt,
}

/// Path-copies one exact, source-authenticated coverage replacement.
///
/// The unchanged prefix and suffix proofs bind the retained base root to the
/// current target revision without rescanning either side of the edit. The
/// selected base range must equal one complete persisted Coverage event. The
/// event's owner depth and semantic part are retained; `logical` is rederived
/// from the exact target source bytes before persistence.
///
/// This operation decodes/re-encodes one bounded storage page and performs one
/// AVL splice. Work outside the replacement bytes is bounded by tree height.
/// Prefix/suffix witnesses may be absent only at document start/end.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn splice_m11_recursive_green_coverage_atomic(
    runtime: &mut DocumentRuntime,
    base: &M11RecursiveGreenRoot,
    target_lease: SourceSnapshotLease,
    prefix: Option<ExactUnchangedPrefixWitness>,
    suffix: Option<ExactUnchangedSuffixWitness>,
    base_byte_range: Range<usize>,
    target_byte_range: Range<usize>,
    logical: M11RecursiveGreenLogicalAction,
) -> Result<
    (
        M11RecursiveGreenRoot,
        M11RecursiveGreenCoverageSpliceReceipt,
    ),
    M11RecursiveGreenError,
> {
    base.ensure_storage_live(runtime)?;
    let target_source = target_lease.version();
    if runtime.current_source_version() != Some(target_source) {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    let base_source = base.source();
    if base_byte_range.start >= base_byte_range.end
        || base_byte_range.end > base_source.byte_len()
        || target_byte_range.start >= target_byte_range.end
        || target_byte_range.end > target_source.byte_len()
    {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    let base_lease = base.lease()?;
    let base_utf16_start = base_lease.utf16_offset_for_byte(base_byte_range.start)?;
    let base_utf16_end = base_lease.utf16_offset_for_byte(base_byte_range.end)?;
    let target_utf16_start = target_lease.utf16_offset_for_byte(target_byte_range.start)?;
    let target_utf16_end = target_lease.utf16_offset_for_byte(target_byte_range.end)?;
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
    let mut plan = plan_coverage_splice(runtime.producer_arena(), tree, &base_byte_range)?;
    let replacement = derive_coverage_atoms(
        &target_lease,
        target_byte_range,
        plan.owner_depth,
        plan.part,
        logical,
    )?;
    let replacement_coverage_events =
        u64::try_from(replacement.len()).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let boundary_events_reencoded = plan
        .events
        .len()
        .checked_sub(1)
        .and_then(|events| events.checked_add(replacement.len()))
        .and_then(|events| u64::try_from(events).ok())
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    plan.events
        .splice(plan.replacement_index..=plan.replacement_index, replacement);

    let mut mutation = SequenceMutationReceipt::default();
    add_inspection(&mut mutation.inspection, plan.inspection)?;
    let mut session = runtime.producer_arena_mut().begin_build()?;
    let replacement_root = build_replacement_pages(&mut session, &plan.events, &mut mutation)?;
    let replacement_storage_pages = u64::try_from(mutation.leaves_adopted)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let storage_range = plan.storage_page_ordinal
        ..plan
            .storage_page_ordinal
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let root = splice_measured_sequence_atomic::<RecursiveGreenSpec>(
        &mut session,
        tree,
        storage_range,
        Some(replacement_root),
        &mut mutation,
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "nonempty recursive-green splice produced an empty root",
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
                "recursive-green splice sealed an empty root",
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
    let target_bytes = u64::try_from(target_source.byte_len())
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let target_utf16 = u64::try_from(target_source.utf16_len())
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let expected_events = base
        .event_count()
        .checked_sub(1)
        .and_then(|events| events.checked_add(replacement_coverage_events))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if summary.physical_bytes != target_bytes
        || summary.physical_utf16 != target_utf16
        || summary.events != expected_events
        || summary.balance != 0
        || summary.minimum_prefix != 0
        || summary.oldest_open.is_some()
    {
        release_tree_after_failure(runtime, target_tree);
        return Err(M11RecursiveGreenError::IncompleteCoverage);
    }

    let receipt = match make_receipt(
        base,
        replacement_coverage_events,
        plan.boundary_events_decoded,
        boundary_events_reencoded,
        replacement_storage_pages,
        mutation,
        seal_transitions,
        lineage_transitions,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            release_tree_after_failure(runtime, target_tree);
            return Err(error);
        }
    };
    let build_receipt =
        M11RecursiveGreenBuildReceipt::from_mutation(0, summary, mutation, seal_transitions);
    Ok((
        M11RecursiveGreenRoot::from_splice(
            runtime.producer_identity(),
            allocate_recursive_green_identity()?,
            target_lease,
            summary,
            measure.leaves(),
            measure.height(),
            target_tree,
            build_receipt,
        ),
        receipt,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_lineage(
    runtime: &DocumentRuntime,
    base_source: crate::SourceVersion,
    target_source: crate::SourceVersion,
    prefix: Option<ExactUnchangedPrefixWitness>,
    suffix: Option<ExactUnchangedSuffixWitness>,
    base_bytes: &Range<usize>,
    target_bytes: &Range<usize>,
    base_utf16_start: usize,
    base_utf16_end: usize,
    target_utf16_start: usize,
    target_utf16_end: usize,
) -> Result<usize, M11RecursiveGreenError> {
    let prefix_transitions = match prefix {
        Some(witness) => {
            let witness = runtime.take_exact_unchanged_prefix_witness(witness)?;
            if base_bytes.start == 0
                || witness.base() != base_source
                || witness.target() != target_source
                || witness.byte_end() != base_bytes.start
                || witness.byte_end() != target_bytes.start
                || witness.utf16_end() != base_utf16_start
                || witness.utf16_end() != target_utf16_start
            {
                return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
            }
            witness.lineage_transitions()
        }
        None if base_bytes.start == 0 && target_bytes.start == 0 => 0,
        None => return Err(M11RecursiveGreenError::SourceAuthorityMismatch),
    };
    let suffix_transitions = match suffix {
        Some(witness) => {
            let witness = runtime.take_exact_unchanged_suffix_witness(witness)?;
            if base_bytes.end == base_source.byte_len()
                || witness.base() != base_source
                || witness.target() != target_source
                || witness.base_byte_start() != base_bytes.end
                || witness.target_byte_start() != target_bytes.end
                || witness.base_utf16_start() != base_utf16_end
                || witness.target_utf16_start() != target_utf16_end
            {
                return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
            }
            witness.lineage_transitions()
        }
        None if base_bytes.end == base_source.byte_len()
            && target_bytes.end == target_source.byte_len() =>
        {
            0
        }
        None => return Err(M11RecursiveGreenError::SourceAuthorityMismatch),
    };
    if prefix_transitions != 0
        && suffix_transitions != 0
        && prefix_transitions != suffix_transitions
    {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    Ok(prefix_transitions.max(suffix_transitions))
}

fn plan_coverage_splice(
    arena: &PageArena,
    tree: &super::build::GreenSequenceTree,
    base_byte_range: &Range<usize>,
) -> Result<CoverageSplicePlan, M11RecursiveGreenError> {
    let position = u64::try_from(base_byte_range.start)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut inspection = SequenceInspectionReceipt::default();
    let located = tree
        .as_ref()
        .locate_leaf_containing_metric(
            arena,
            position,
            |summary| summary.physical_bytes,
            &mut inspection,
        )?
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let payload = arena.payload(located.id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(
        M11RecursiveGreenError::Corrupt("coverage splice selected a branch payload"),
    )?;
    let mut cursor = 0_usize;
    let mut events = Vec::with_capacity(usize::from(leaf.events));
    for _ in 0..leaf.events {
        events.push(decode_packed_event(leaf.event_bytes, &mut cursor)?);
    }
    if cursor != leaf.event_bytes.len() {
        return Err(M11RecursiveGreenError::Corrupt(
            "coverage splice did not consume its boundary page",
        ));
    }

    let mut physical_cursor = located.prefix.map_or(0, |prefix| prefix.physical_bytes);
    let wanted_start = u64::try_from(base_byte_range.start)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let wanted_end =
        u64::try_from(base_byte_range.end).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut selected = None;
    for (index, event) in events.iter().copied().enumerate() {
        let PackedGreenEvent::Coverage {
            physical,
            owner_depth,
            part,
            ..
        } = event
        else {
            continue;
        };
        let event_start = physical_cursor;
        let event_end = event_start
            .checked_add(physical.bytes())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let overlaps = wanted_start < event_end && wanted_end > event_start;
        if overlaps {
            if event_start != wanted_start || event_end != wanted_end || selected.is_some() {
                return Err(M11RecursiveGreenError::InvalidPoint);
            }
            selected = Some((index, owner_depth, part));
        }
        physical_cursor = event_end;
    }
    let (replacement_index, owner_depth, part) =
        selected.ok_or(M11RecursiveGreenError::InvalidPoint)?;
    Ok(CoverageSplicePlan {
        storage_page_ordinal: located.ordinal,
        events,
        replacement_index,
        owner_depth,
        part,
        boundary_events_decoded: u64::from(leaf.events),
        inspection,
    })
}

pub(super) fn derive_coverage_atoms(
    lease: &SourceSnapshotLease,
    range: Range<usize>,
    owner_depth: u32,
    part: M11RecursiveGreenCoveragePart,
    logical: M11RecursiveGreenLogicalAction,
) -> Result<Vec<PackedGreenEvent>, M11RecursiveGreenError> {
    logical.validate(owner_depth)?;
    let physical = metric_between(lease, range.start, range.end)?;
    let one = |atom| PackedGreenEvent::Coverage {
        physical,
        owner_depth,
        part,
        atom,
    };
    match logical {
        M11RecursiveGreenLogicalAction::None => Ok(vec![one(LogicalAtom::None)]),
        M11RecursiveGreenLogicalAction::Identity => Ok(vec![one(LogicalAtom::Identity)]),
        M11RecursiveGreenLogicalAction::HiddenUpstream => {
            Ok(vec![one(LogicalAtom::HiddenUpstream)])
        }
        M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth,
            remaining_spaces,
        } => {
            if physical != M11RecursiveGreenSourceMetric::from_validated(1, 1)
                || read_small(lease, range.start, range.end)? != ([b'\t', 0], 1)
            {
                return Err(M11RecursiveGreenError::InvalidEvent);
            }
            Ok(vec![one(LogicalAtom::TabToSpaces {
                target_owner_depth,
                spaces: remaining_spaces,
            })])
        }
        M11RecursiveGreenLogicalAction::CanonicalNewline => {
            let (bytes, len) = read_small(lease, range.start, range.end)?;
            let atom = match &bytes[..len] {
                b"\n" => LogicalAtom::LfToLf,
                b"\r" => LogicalAtom::LoneCrToLf,
                b"\r\n" => LogicalAtom::CrLfToLf,
                _ => return Err(M11RecursiveGreenError::InvalidEvent),
            };
            Ok(vec![one(atom)])
        }
        M11RecursiveGreenLogicalAction::CanonicalText => {
            derive_canonical_text_atoms(lease, range, owner_depth, part)
        }
    }
}

pub(super) fn derive_canonical_text_atoms(
    lease: &SourceSnapshotLease,
    range: Range<usize>,
    owner_depth: u32,
    part: M11RecursiveGreenCoveragePart,
) -> Result<Vec<PackedGreenEvent>, M11RecursiveGreenError> {
    let mut cursor = lease.duplicate().cursor_in(range.clone())?;
    let mut atoms = Vec::new();
    let mut run_start = range.start;
    while cursor.position() < range.end {
        let position = cursor.position();
        let byte = cursor
            .next_byte()
            .ok_or(M11RecursiveGreenError::IncompleteCoverage)?;
        if byte != 0 {
            continue;
        }
        if position > run_start {
            atoms.push(PackedGreenEvent::Coverage {
                physical: metric_between(lease, run_start, position)?,
                owner_depth,
                part,
                atom: LogicalAtom::Identity,
            });
        }
        atoms.push(PackedGreenEvent::Coverage {
            physical: M11RecursiveGreenSourceMetric::from_validated(1, 1),
            owner_depth,
            part,
            atom: LogicalAtom::NulToReplacement,
        });
        run_start = position
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    }
    drop(cursor.finish()?);
    if run_start < range.end {
        atoms.push(PackedGreenEvent::Coverage {
            physical: metric_between(lease, run_start, range.end)?,
            owner_depth,
            part,
            atom: LogicalAtom::Identity,
        });
    }
    if atoms.is_empty() {
        return Err(M11RecursiveGreenError::InvalidEvent);
    }
    Ok(atoms)
}

pub(super) fn build_replacement_pages(
    session: &mut ArenaBuildSession<'_>,
    events: &[PackedGreenEvent],
    mutation: &mut SequenceMutationReceipt,
) -> Result<GreenSequenceBuildRoot, M11RecursiveGreenError> {
    let mut builder = GreenSequenceBuilder::try_new(session, mutation)?;
    let mut page = [0_u8; ARENA_PAGE_BYTES];
    let mut page_len = GREEN_LEAF_HEADER_BYTES;
    let mut page_events = 0_u16;
    let mut page_summary = RecursiveGreenSummary::empty();
    for event in events.iter().copied() {
        let event_len = packed_event_len(event);
        if page_events > 0
            && (usize::from(page_events) >= GREEN_EVENTS_PER_PAGE_MAX
                || page_len
                    .checked_add(event_len)
                    .is_none_or(|next| next > ARENA_PAGE_BYTES))
        {
            push_replacement_page(
                session,
                &mut builder,
                &mut page,
                page_len,
                page_events,
                page_summary,
                mutation,
            )?;
            page.fill(0);
            page_len = GREEN_LEAF_HEADER_BYTES;
            page_events = 0;
            page_summary = RecursiveGreenSummary::empty();
        }
        if page_len
            .checked_add(event_len)
            .is_none_or(|next| next > ARENA_PAGE_BYTES)
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green replacement event exceeds one storage page",
            ));
        }
        page_summary = page_summary.checked_followed_by(packed_event_summary(event)?)?;
        encode_packed_event(event, &mut page, &mut page_len)?;
        page_events = page_events
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    }
    if page_events == 0 {
        return Err(M11RecursiveGreenError::InvalidEvent);
    }
    push_replacement_page(
        session,
        &mut builder,
        &mut page,
        page_len,
        page_events,
        page_summary,
        mutation,
    )?;
    builder.begin_finish(session, mutation)?;
    while builder.poll_finish(session, mutation)? != ResumableSequenceProgress::Complete {}
    builder.take_root(session)
}

#[allow(clippy::too_many_arguments)]
fn push_replacement_page(
    session: &mut ArenaBuildSession<'_>,
    builder: &mut GreenSequenceBuilder,
    page: &mut [u8; ARENA_PAGE_BYTES],
    page_len: usize,
    page_events: u16,
    page_summary: RecursiveGreenSummary,
    mutation: &mut SequenceMutationReceipt,
) -> Result<(), M11RecursiveGreenError> {
    encode_leaf_header(
        page,
        page_events,
        page_len - GREEN_LEAF_HEADER_BYTES,
        page_summary,
    )?;
    let leaf = session.allocate(&page[..page_len], &[])?;
    builder.begin_push(session, leaf, mutation)?;
    while builder.poll_push(session, mutation)? != ResumableSequenceProgress::Complete {}
    Ok(())
}

pub(super) fn metric_between(
    lease: &SourceSnapshotLease,
    start: usize,
    end: usize,
) -> Result<M11RecursiveGreenSourceMetric, M11RecursiveGreenError> {
    let start_utf16 = lease.utf16_offset_for_byte(start)?;
    let end_utf16 = lease.utf16_offset_for_byte(end)?;
    M11RecursiveGreenSourceMetric::new(
        u64::try_from(
            end.checked_sub(start)
                .ok_or(M11RecursiveGreenError::InvalidPoint)?,
        )
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        u64::try_from(
            end_utf16
                .checked_sub(start_utf16)
                .ok_or(M11RecursiveGreenError::InvalidPoint)?,
        )
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )
    .ok_or(M11RecursiveGreenError::InvalidPoint)
}

pub(super) fn read_small(
    lease: &SourceSnapshotLease,
    start: usize,
    end: usize,
) -> Result<([u8; 2], usize), M11RecursiveGreenError> {
    if end.checked_sub(start).is_none_or(|length| length > 2) {
        return Err(M11RecursiveGreenError::InvalidEvent);
    }
    let mut cursor = lease.duplicate().cursor_in(start..end)?;
    let mut output = [0_u8; 2];
    let read = cursor.read(&mut output);
    drop(cursor.finish()?);
    Ok((output, read))
}

#[allow(clippy::too_many_arguments)]
fn make_receipt(
    base: &M11RecursiveGreenRoot,
    replacement_coverage_events: u64,
    boundary_events_decoded: u64,
    boundary_events_reencoded: u64,
    replacement_storage_pages: u64,
    mutation: SequenceMutationReceipt,
    seal_transitions: usize,
    lineage_transitions: usize,
) -> Result<M11RecursiveGreenCoverageSpliceReceipt, M11RecursiveGreenError> {
    let base_storage_pages = base.storage_page_count();
    let reused_storage_pages = base_storage_pages
        .checked_sub(1)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if mutation.leaves_deleted != 1
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
            "recursive-green splice receipt differs from measured mutation work",
        ));
    }
    Ok(M11RecursiveGreenCoverageSpliceReceipt {
        base_events: base.event_count(),
        replacement_coverage_events,
        unchanged_events_preserved: base
            .event_count()
            .checked_sub(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        boundary_events_decoded,
        boundary_events_reencoded,
        base_storage_pages,
        deleted_storage_pages: 1,
        replacement_storage_pages,
        reused_storage_pages,
        node_headers_decoded: mutation.inspection.node_headers_decoded,
        summary_combinations: mutation.inspection.summary_combinations,
        payload_bytes_inspected: mutation.inspection.spec.payload_bytes_inspected,
        events_authenticated: mutation.inspection.spec.spec_items_hashed,
        tree_nodes_visited: mutation.nodes_visited,
        branches_allocated: mutation.branches_allocated,
        branch_payload_bytes: mutation.branch_payload_bytes,
        maximum_atomic_height: mutation.maximum_atomic_height,
        seal_transitions,
        lineage_transitions,
    })
}

pub(super) fn add_inspection(
    total: &mut SequenceInspectionReceipt,
    added: SequenceInspectionReceipt,
) -> Result<(), M11RecursiveGreenError> {
    total.node_headers_decoded = total
        .node_headers_decoded
        .checked_add(added.node_headers_decoded)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.summary_combinations = total
        .summary_combinations
        .checked_add(added.summary_combinations)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.spec.payload_bytes_inspected = total
        .spec
        .payload_bytes_inspected
        .checked_add(added.spec.payload_bytes_inspected)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.spec.spec_items_hashed = total
        .spec
        .spec_items_hashed
        .checked_add(added.spec.spec_items_hashed)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    Ok(())
}

pub(super) fn abort_seal_after_failure(
    runtime: &mut DocumentRuntime,
    seal: crate::measured_sequence::MeasuredSequenceSeal<RecursiveGreenSpec>,
) {
    if let Err(failure) = seal.abort(runtime.producer_arena_mut()) {
        let error = failure.error;
        let _seal = failure.seal;
        panic!("recursive-green splice seal cleanup failed: {error}");
    }
}

pub(super) fn release_tree_after_failure(
    runtime: &mut DocumentRuntime,
    tree: super::build::GreenSequenceTree,
) {
    if let Err(failure) = tree.release(runtime.producer_arena_mut()) {
        panic!(
            "recursive-green splice tree cleanup failed in its creating arena: {}",
            failure.error
        );
    }
}

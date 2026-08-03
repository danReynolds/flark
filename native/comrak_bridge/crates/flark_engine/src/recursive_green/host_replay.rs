//! Typed exact-base transport primitives for persistent recursive Green.
//!
//! The producer names a contiguous semantic event range, maps it to complete
//! packed `RGL1` leaves, and transfers only those leaves. An independent host
//! derives the base cut again, rejects every non-leaf payload, path-copies the
//! retained `RGB1` tree, and authenticates the result against the target's
//! shape-independent canonical event commitment.

use std::ops::Range;

use crate::measured_sequence::{
    splice_measured_sequence_build_root_atomic, validate_measured_sequence_build_owner,
    validate_measured_sequence_node, MeasuredSequenceBuildRoot, MeasuredSequenceRef,
    ResumableSequenceProgress, SequenceInspectionReceipt, SequenceMutationReceipt,
    SequenceSpecInspection,
};
use crate::storage::{ArenaBuildOwner, ArenaBuildSession, ArenaError, PageArena};
use crate::ArenaId;

use super::build::GreenSequenceBuilder;
use super::codec::{
    decode_leaf, M11RecursiveGreenError, RecursiveGreenSpec, RecursiveGreenSummary,
};
use super::publication::{
    descriptor_for, validate_descriptor, validate_persistent_m11_recursive_green_root,
    PersistentM11RecursiveGreenRoleDescriptor,
};

struct RecursiveGreenSemanticSplicePlan {
    storage_page_range: Range<u64>,
    prefix_events: u64,
    suffix_events: u64,
    boundary_events_decoded: u64,
    inspection: SequenceInspectionReceipt,
}

/// Packed-leaf cut independently derived from one semantic event range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11RecursiveGreenSemanticSplicePlan {
    pub(crate) storage_page_range: Range<u64>,
    pub(crate) prefix_events: u64,
    pub(crate) suffix_events: u64,
    pub(crate) boundary_events_decoded: u64,
    inspection: SequenceInspectionReceipt,
}

/// Maps a semantic event range to the complete `RGL1` pages which contain it.
///
/// Boundary survivors are deliberately included. Therefore replay never
/// transports or edits an `RGB1` branch and measured-sequence path copying can
/// remain the sole tree-shape mechanism.
pub(crate) fn plan_persistent_m11_recursive_green_semantic_splice(
    arena: &PageArena,
    root: Option<ArenaId>,
    descriptor: PersistentM11RecursiveGreenRoleDescriptor,
    event_range: Range<u64>,
) -> Result<M11RecursiveGreenSemanticSplicePlan, M11RecursiveGreenError> {
    let claim = validate_persistent_m11_recursive_green_root(arena, root, descriptor)?;
    let plan = plan_recursive_green_semantic_splice(
        arena,
        MeasuredSequenceRef::from_imported_root(root),
        claim.summary(),
        claim.storage_page_count(),
        event_range,
    )?;
    Ok(M11RecursiveGreenSemanticSplicePlan {
        storage_page_range: plan.storage_page_range,
        prefix_events: plan.prefix_events,
        suffix_events: plan.suffix_events,
        boundary_events_decoded: plan.boundary_events_decoded,
        inspection: plan.inspection,
    })
}

/// Returns one authenticated canonical `RGL1` leaf by storage-page ordinal.
pub(crate) fn persistent_m11_recursive_green_storage_page_at(
    arena: &PageArena,
    root: Option<ArenaId>,
    ordinal: u64,
) -> Result<Option<&[u8]>, M11RecursiveGreenError> {
    let Some(root) = root else {
        return Ok(None);
    };
    let mut inspection = SequenceInspectionReceipt::default();
    let sequence = MeasuredSequenceRef::<RecursiveGreenSpec>::from_imported_root(Some(root));
    let Some(located) = sequence.locate_leaf_with_prefix(arena, ordinal, &mut inspection)? else {
        return Ok(None);
    };
    let payload = arena.payload(located.id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(
        M11RecursiveGreenError::Corrupt("located recursive Green storage page is not a leaf"),
    )?;
    if leaf.summary != located.summary || arena.child_count(located.id)? != 0 {
        return Err(M11RecursiveGreenError::Corrupt(
            "located recursive Green storage page changed shape",
        ));
    }
    Ok(Some(payload))
}

/// One exact-base semantic splice segment admitted by an independent host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11RecursiveGreenHostSpliceSegmentClaim {
    pub(crate) base_event_range: Range<u64>,
    pub(crate) target_event_range: Range<u64>,
    pub(crate) base_storage_range: Range<u64>,
    pub(crate) target_storage_range: Range<u64>,
}

/// Exact-base semantic splice batch admitted by an independent host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11RecursiveGreenHostSpliceClaim {
    pub(crate) segments: Box<[M11RecursiveGreenHostSpliceSegmentClaim]>,
    pub(crate) base_descriptor: PersistentM11RecursiveGreenRoleDescriptor,
    pub(crate) target_descriptor: PersistentM11RecursiveGreenRoleDescriptor,
}

impl M11RecursiveGreenHostSpliceClaim {
    pub(crate) fn segments(&self) -> &[M11RecursiveGreenHostSpliceSegmentClaim] {
        &self.segments
    }
}

/// Bounded work receipt for one independently authenticated Green replay.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11RecursiveGreenHostSpliceWork {
    base_events: u64,
    deleted_events: u64,
    replacement_events: u64,
    base_storage_pages: u64,
    transferred_storage_pages: u64,
    reused_storage_pages: u64,
    transferred_payload_bytes: u64,
    boundary_events_decoded: u64,
    node_headers_decoded: u64,
    events_authenticated: u64,
    tree_nodes_visited: usize,
    branches_allocated: usize,
    branch_payload_bytes: usize,
    maximum_atomic_height: u16,
}

#[allow(dead_code)] // Endpoint receipts are wired by the exact-base transport layer.
impl M11RecursiveGreenHostSpliceWork {
    pub(crate) const fn base_events(self) -> u64 {
        self.base_events
    }

    pub(crate) const fn deleted_events(self) -> u64 {
        self.deleted_events
    }

    pub(crate) const fn replacement_events(self) -> u64 {
        self.replacement_events
    }

    pub(crate) const fn base_storage_pages(self) -> u64 {
        self.base_storage_pages
    }

    pub(crate) const fn transferred_storage_pages(self) -> u64 {
        self.transferred_storage_pages
    }

    pub(crate) const fn reused_storage_pages(self) -> u64 {
        self.reused_storage_pages
    }

    pub(crate) const fn transferred_payload_bytes(self) -> u64 {
        self.transferred_payload_bytes
    }

    pub(crate) const fn boundary_events_decoded(self) -> u64 {
        self.boundary_events_decoded
    }

    pub(crate) const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    pub(crate) const fn events_authenticated(self) -> u64 {
        self.events_authenticated
    }

    pub(crate) const fn tree_nodes_visited(self) -> usize {
        self.tree_nodes_visited
    }

    pub(crate) const fn branches_allocated(self) -> usize {
        self.branches_allocated
    }

    pub(crate) const fn branch_payload_bytes(self) -> usize {
        self.branch_payload_bytes
    }

    pub(crate) const fn maximum_atomic_height(self) -> u16 {
        self.maximum_atomic_height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11RecursiveGreenHostReplayPoll {
    Pending,
    Complete,
}

pub(crate) struct M11RecursiveGreenHostReplayOutput {
    root: Option<ArenaBuildOwner>,
    work: M11RecursiveGreenHostSpliceWork,
}

impl M11RecursiveGreenHostReplayOutput {
    pub(crate) fn into_parts(self) -> (Option<ArenaBuildOwner>, M11RecursiveGreenHostSpliceWork) {
        (self.root, self.work)
    }
}

enum M11RecursiveGreenHostReplayPhase {
    Accepting {
        segment_index: usize,
        builder: Option<GreenSequenceBuilder>,
        push_active: bool,
    },
    Finishing {
        segment_index: usize,
        builder: GreenSequenceBuilder,
    },
    Ready,
    Complete,
    Poisoned,
}

/// Abort-journal-safe replay of exact base-relative recursive-Green cuts.
pub(crate) struct M11RecursiveGreenHostReplay {
    base: Option<MeasuredSequenceBuildRoot<RecursiveGreenSpec>>,
    claim: M11RecursiveGreenHostSpliceClaim,
    plans: Box<[M11RecursiveGreenSemanticSplicePlan]>,
    replacements: Vec<Option<MeasuredSequenceBuildRoot<RecursiveGreenSpec>>>,
    current_replacement_page_count: u64,
    current_replacement_summary: RecursiveGreenSummary,
    total_replacement_page_count: u64,
    replacement_payload_bytes: u64,
    phase: M11RecursiveGreenHostReplayPhase,
    base_validation_receipt: SequenceMutationReceipt,
    replacement_receipt: SequenceMutationReceipt,
    splice_receipt: SequenceMutationReceipt,
    target_validation_receipt: SequenceMutationReceipt,
}

impl M11RecursiveGreenHostReplay {
    pub(crate) fn new(
        session: &ArenaBuildSession<'_>,
        base_owner: Option<ArenaBuildOwner>,
        claim: M11RecursiveGreenHostSpliceClaim,
    ) -> Result<Self, M11RecursiveGreenError> {
        validate_descriptor(claim.base_descriptor)?;
        validate_descriptor(claim.target_descriptor)?;
        let base_root = base_owner.as_ref().map(ArenaBuildOwner::id);
        let base_claim = validate_persistent_m11_recursive_green_root(
            session.arena(),
            base_root,
            claim.base_descriptor,
        )?;

        let mut plans = Vec::new();
        plans
            .try_reserve_exact(claim.segments.len())
            .map_err(|_| M11RecursiveGreenError::Arena(ArenaError::AllocationFailed))?;
        let mut previous_base_event_end = 0_u64;
        let mut previous_target_event_end = 0_u64;
        let mut previous_base_storage_end = 0_u64;
        let mut previous_target_storage_end = 0_u64;
        let mut deleted_events = 0_u64;
        let mut replacement_events = 0_u64;
        let mut deleted_pages = 0_u64;
        let mut replacement_pages = 0_u64;
        for segment in claim.segments.iter() {
            if segment.base_event_range.start > segment.base_event_range.end
                || segment.target_event_range.start > segment.target_event_range.end
                || segment.base_storage_range.start > segment.base_storage_range.end
                || segment.target_storage_range.start > segment.target_storage_range.end
                || segment.base_event_range.end > claim.base_descriptor.event_count()
                || segment.target_event_range.end > claim.target_descriptor.event_count()
                || segment.base_storage_range.end > claim.base_descriptor.storage_page_count()
                || segment.target_storage_range.end > claim.target_descriptor.storage_page_count()
            {
                return Err(M11RecursiveGreenError::InvalidPoint);
            }
            if segment.base_event_range.start < previous_base_event_end
                || segment.target_event_range.start < previous_target_event_end
                || segment.base_storage_range.start < previous_base_storage_end
                || segment.target_storage_range.start < previous_target_storage_end
            {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive Green replay segments are not sorted and nonoverlapping",
                ));
            }
            let base_event_gap = segment.base_event_range.start - previous_base_event_end;
            let target_event_gap = segment.target_event_range.start - previous_target_event_end;
            let base_storage_gap = segment.base_storage_range.start - previous_base_storage_end;
            let target_storage_gap =
                segment.target_storage_range.start - previous_target_storage_end;
            if base_event_gap != target_event_gap || base_storage_gap != target_storage_gap {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive Green replay segment ordinal mapping changed",
                ));
            }

            let plan = plan_recursive_green_semantic_splice(
                session.arena(),
                MeasuredSequenceRef::from_imported_root(base_root),
                base_claim.summary(),
                base_claim.storage_page_count(),
                segment.base_event_range.clone(),
            )?;
            if plan.storage_page_range != segment.base_storage_range {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive Green replay storage cut differs from its semantic range",
                ));
            }
            plans.push(M11RecursiveGreenSemanticSplicePlan {
                storage_page_range: plan.storage_page_range,
                prefix_events: plan.prefix_events,
                suffix_events: plan.suffix_events,
                boundary_events_decoded: plan.boundary_events_decoded,
                inspection: plan.inspection,
            });

            deleted_events = deleted_events
                .checked_add(segment.base_event_range.end - segment.base_event_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            replacement_events = replacement_events
                .checked_add(segment.target_event_range.end - segment.target_event_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            deleted_pages = deleted_pages
                .checked_add(segment.base_storage_range.end - segment.base_storage_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            replacement_pages = replacement_pages
                .checked_add(segment.target_storage_range.end - segment.target_storage_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            previous_base_event_end = segment.base_event_range.end;
            previous_target_event_end = segment.target_event_range.end;
            previous_base_storage_end = segment.base_storage_range.end;
            previous_target_storage_end = segment.target_storage_range.end;
        }

        let expected_target_events = claim
            .base_descriptor
            .event_count()
            .checked_sub(deleted_events)
            .and_then(|events| events.checked_add(replacement_events))
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let expected_target_pages = claim
            .base_descriptor
            .storage_page_count()
            .checked_sub(deleted_pages)
            .and_then(|pages| pages.checked_add(replacement_pages))
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let base_event_suffix = claim.base_descriptor.event_count() - previous_base_event_end;
        let target_event_suffix = claim.target_descriptor.event_count() - previous_target_event_end;
        let base_storage_suffix =
            claim.base_descriptor.storage_page_count() - previous_base_storage_end;
        let target_storage_suffix =
            claim.target_descriptor.storage_page_count() - previous_target_storage_end;
        if claim.target_descriptor.event_count() != expected_target_events
            || base_event_suffix != target_event_suffix
        {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        if claim.target_descriptor.storage_page_count() != expected_target_pages
            || base_storage_suffix != target_storage_suffix
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive Green replay storage-page arithmetic changed",
            ));
        }

        let mut base_validation_receipt = SequenceMutationReceipt::default();
        let base = match base_owner {
            Some(owner) => Some(
                validate_measured_sequence_build_owner::<RecursiveGreenSpec>(
                    session,
                    owner,
                    &mut base_validation_receipt,
                )?,
            ),
            None => {
                if base_claim.event_count() != 0 {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "nonempty recursive Green replay base lost its owner",
                    ));
                }
                None
            }
        };
        let mut replacements = Vec::new();
        replacements
            .try_reserve_exact(claim.segments.len())
            .map_err(|_| M11RecursiveGreenError::Arena(ArenaError::AllocationFailed))?;
        let mut replay = Self {
            base,
            claim,
            plans: plans.into_boxed_slice(),
            replacements,
            current_replacement_page_count: 0,
            current_replacement_summary: RecursiveGreenSummary::empty(),
            total_replacement_page_count: 0,
            replacement_payload_bytes: 0,
            phase: M11RecursiveGreenHostReplayPhase::Poisoned,
            base_validation_receipt,
            replacement_receipt: SequenceMutationReceipt::default(),
            splice_receipt: SequenceMutationReceipt::default(),
            target_validation_receipt: SequenceMutationReceipt::default(),
        };
        let segment_index = replay.advance_empty_segments(0)?;
        replay.phase = M11RecursiveGreenHostReplayPhase::Accepting {
            segment_index,
            builder: None,
            push_active: false,
        };
        Ok(replay)
    }

    fn expected_segment_events(&self, segment_index: usize) -> Result<u64, M11RecursiveGreenError> {
        let segment = self
            .claim
            .segments
            .get(segment_index)
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let plan = self
            .plans
            .get(segment_index)
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        plan.prefix_events
            .checked_add(segment.target_event_range.end - segment.target_event_range.start)
            .and_then(|events| events.checked_add(plan.suffix_events))
            .ok_or(M11RecursiveGreenError::CounterOverflow)
    }

    fn expected_segment_pages(&self, segment_index: usize) -> Result<u64, M11RecursiveGreenError> {
        let segment = self
            .claim
            .segments
            .get(segment_index)
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        Ok(segment.target_storage_range.end - segment.target_storage_range.start)
    }

    fn record_current_segment(
        &mut self,
        segment_index: usize,
        replacement: Option<MeasuredSequenceBuildRoot<RecursiveGreenSpec>>,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.replacements.len() != segment_index {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let expected_pages = self.expected_segment_pages(segment_index)?;
        let expected_events = self.expected_segment_events(segment_index)?;
        if self.current_replacement_page_count != expected_pages
            || self.current_replacement_summary.events != expected_events
            || replacement.is_some() != (expected_pages != 0)
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive Green replacement pages differ from their semantic segment",
            ));
        }
        self.replacements.push(replacement);
        self.current_replacement_page_count = 0;
        self.current_replacement_summary = RecursiveGreenSummary::empty();
        Ok(())
    }

    fn advance_empty_segments(
        &mut self,
        mut segment_index: usize,
    ) -> Result<usize, M11RecursiveGreenError> {
        while segment_index < self.claim.segments.len()
            && self.expected_segment_pages(segment_index)? == 0
        {
            self.record_current_segment(segment_index, None)?;
            segment_index += 1;
        }
        Ok(segment_index)
    }

    /// Admits one canonical `RGL1` replacement leaf. `RGB1` and arbitrary
    /// arena nodes are rejected before they can enter the typed builder.
    pub(crate) fn offer_replacement_leaf(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        leaf: ArenaBuildOwner,
    ) -> Result<(), M11RecursiveGreenError> {
        let phase = std::mem::replace(&mut self.phase, M11RecursiveGreenHostReplayPhase::Poisoned);
        let M11RecursiveGreenHostReplayPhase::Accepting {
            segment_index,
            builder,
            push_active: false,
        } = phase
        else {
            return Err(M11RecursiveGreenError::InvalidState);
        };
        if segment_index >= self.claim.segments.len()
            || self.current_replacement_page_count >= self.expected_segment_pages(segment_index)?
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "too many recursive Green replacement pages",
            ));
        }
        session.validate_owner(&leaf)?;
        if session.arena().child_count(leaf.id())? != 0 {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive Green replacement page owns children",
            ));
        }
        let payload = session.arena().payload(leaf.id())?;
        let mut inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(payload, &mut inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("recursive Green replacement page is not RGL1"),
        )?;
        self.current_replacement_summary = self
            .current_replacement_summary
            .checked_followed_by(decoded.summary)?;
        self.replacement_payload_bytes = self
            .replacement_payload_bytes
            .checked_add(
                u64::try_from(payload.len())
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            )
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;

        let mut builder = match builder {
            Some(builder) => builder,
            None => GreenSequenceBuilder::try_new(session, &mut self.replacement_receipt)?,
        };
        builder.begin_push(session, leaf, &mut self.replacement_receipt)?;
        self.current_replacement_page_count = self
            .current_replacement_page_count
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        self.total_replacement_page_count = self
            .total_replacement_page_count
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        self.phase = M11RecursiveGreenHostReplayPhase::Accepting {
            segment_index,
            builder: Some(builder),
            push_active: true,
        };
        Ok(())
    }

    pub(crate) fn poll_replacement(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<M11RecursiveGreenHostReplayPoll, M11RecursiveGreenError> {
        let phase = std::mem::replace(&mut self.phase, M11RecursiveGreenHostReplayPhase::Poisoned);
        match phase {
            M11RecursiveGreenHostReplayPhase::Accepting {
                segment_index,
                builder: Some(mut builder),
                push_active: true,
            } => {
                let progress = builder.poll_push(session, &mut self.replacement_receipt)?;
                if progress != ResumableSequenceProgress::Complete {
                    self.phase = M11RecursiveGreenHostReplayPhase::Accepting {
                        segment_index,
                        builder: Some(builder),
                        push_active: true,
                    };
                    Ok(M11RecursiveGreenHostReplayPoll::Pending)
                } else if self.current_replacement_page_count
                    == self.expected_segment_pages(segment_index)?
                {
                    builder.begin_finish(session, &mut self.replacement_receipt)?;
                    self.phase = M11RecursiveGreenHostReplayPhase::Finishing {
                        segment_index,
                        builder,
                    };
                    Ok(M11RecursiveGreenHostReplayPoll::Pending)
                } else {
                    self.phase = M11RecursiveGreenHostReplayPhase::Accepting {
                        segment_index,
                        builder: Some(builder),
                        push_active: false,
                    };
                    Ok(M11RecursiveGreenHostReplayPoll::Complete)
                }
            }
            M11RecursiveGreenHostReplayPhase::Finishing {
                segment_index,
                mut builder,
            } => {
                let progress = builder.poll_finish(session, &mut self.replacement_receipt)?;
                if progress == ResumableSequenceProgress::Complete {
                    let replacement = builder.take_root(session)?;
                    self.record_current_segment(segment_index, Some(replacement))?;
                    let next_segment = self.advance_empty_segments(segment_index + 1)?;
                    self.phase = M11RecursiveGreenHostReplayPhase::Accepting {
                        segment_index: next_segment,
                        builder: None,
                        push_active: false,
                    };
                    Ok(M11RecursiveGreenHostReplayPoll::Complete)
                } else {
                    self.phase = M11RecursiveGreenHostReplayPhase::Finishing {
                        segment_index,
                        builder,
                    };
                    Ok(M11RecursiveGreenHostReplayPoll::Pending)
                }
            }
            _ => Err(M11RecursiveGreenError::InvalidState),
        }
    }

    pub(crate) fn finish_replacement(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<M11RecursiveGreenHostReplayPoll, M11RecursiveGreenError> {
        let phase = std::mem::replace(&mut self.phase, M11RecursiveGreenHostReplayPhase::Poisoned);
        let M11RecursiveGreenHostReplayPhase::Accepting {
            segment_index,
            builder,
            push_active: false,
        } = phase
        else {
            return Err(M11RecursiveGreenError::InvalidState);
        };
        if segment_index != self.claim.segments.len()
            || builder.is_some()
            || self.current_replacement_page_count != 0
            || self.current_replacement_summary != RecursiveGreenSummary::empty()
            || self.replacements.len() != self.claim.segments.len()
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive Green replacement segment input is incomplete",
            ));
        }
        let _ = session;
        self.phase = M11RecursiveGreenHostReplayPhase::Ready;
        Ok(M11RecursiveGreenHostReplayPoll::Complete)
    }

    pub(crate) fn complete(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<M11RecursiveGreenHostReplayOutput, M11RecursiveGreenError> {
        let phase = std::mem::replace(&mut self.phase, M11RecursiveGreenHostReplayPhase::Poisoned);
        let M11RecursiveGreenHostReplayPhase::Ready = phase else {
            return Err(M11RecursiveGreenError::InvalidState);
        };
        if self.replacements.len() != self.claim.segments.len() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let mut target = self.base.take();
        for segment_index in (0..self.claim.segments.len()).rev() {
            let replacement = self
                .replacements
                .pop()
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            let base_storage_range = self.claim.segments[segment_index]
                .base_storage_range
                .clone();
            target = match target {
                Some(base) => splice_measured_sequence_build_root_atomic::<RecursiveGreenSpec>(
                    session,
                    base,
                    base_storage_range,
                    replacement,
                    &mut self.splice_receipt,
                )?,
                None => {
                    if base_storage_range != (0..0) {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "empty recursive Green replay base changed its cut",
                        ));
                    }
                    replacement
                }
            };
        }
        let target_owner = target.map(MeasuredSequenceBuildRoot::into_owner);
        let target_root = target_owner.as_ref().map(ArenaBuildOwner::id);
        let target_summary = match target_root {
            Some(root) => {
                let measure = validate_measured_sequence_node::<RecursiveGreenSpec>(
                    session.arena(),
                    root,
                    &mut self.target_validation_receipt.inspection,
                )?;
                if descriptor_for(measure.summary(), measure.leaves(), measure.height())
                    != self.claim.target_descriptor
                {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "recursive Green replay target descriptor changed after splice",
                    ));
                }
                measure.summary()
            }
            None if self.claim.target_descriptor.event_count() == 0 => {
                RecursiveGreenSummary::empty()
            }
            None => {
                return Err(M11RecursiveGreenError::Corrupt(
                    "nonempty recursive Green replay target lost its root",
                ));
            }
        };
        if self.claim.target_descriptor.canonical_commitment256()
            != target_summary.canonical_commitment.checksum()
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive Green replay target commitment changed after splice",
            ));
        }

        let mut deleted_events = 0_u64;
        let mut replacement_events = 0_u64;
        let mut deleted_pages = 0_u64;
        let mut boundary_events_decoded = 0_u64;
        for (segment, plan) in self.claim.segments.iter().zip(self.plans.iter()) {
            deleted_events = deleted_events
                .checked_add(segment.base_event_range.end - segment.base_event_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            replacement_events = replacement_events
                .checked_add(segment.target_event_range.end - segment.target_event_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            deleted_pages = deleted_pages
                .checked_add(segment.base_storage_range.end - segment.base_storage_range.start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            boundary_events_decoded = boundary_events_decoded
                .checked_add(plan.boundary_events_decoded)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }
        let reused_storage_pages = self
            .claim
            .base_descriptor
            .storage_page_count()
            .checked_sub(deleted_pages)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let mut node_headers_decoded = 0_u64;
        let mut events_authenticated = 0_u64;
        for receipt in [
            &self.base_validation_receipt.inspection,
            &self.replacement_receipt.inspection,
            &self.splice_receipt.inspection,
            &self.target_validation_receipt.inspection,
        ] {
            node_headers_decoded = node_headers_decoded
                .checked_add(receipt.node_headers_decoded)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            events_authenticated = events_authenticated
                .checked_add(receipt.spec.spec_items_hashed)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }
        for plan in self.plans.iter() {
            node_headers_decoded = node_headers_decoded
                .checked_add(plan.inspection.node_headers_decoded)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            events_authenticated = events_authenticated
                .checked_add(plan.inspection.spec.spec_items_hashed)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }
        let work = M11RecursiveGreenHostSpliceWork {
            base_events: self.claim.base_descriptor.event_count(),
            deleted_events,
            replacement_events,
            base_storage_pages: self.claim.base_descriptor.storage_page_count(),
            transferred_storage_pages: self.total_replacement_page_count,
            reused_storage_pages,
            transferred_payload_bytes: self.replacement_payload_bytes,
            boundary_events_decoded,
            node_headers_decoded,
            events_authenticated,
            tree_nodes_visited: self.splice_receipt.nodes_visited,
            branches_allocated: self
                .replacement_receipt
                .branches_allocated
                .checked_add(self.splice_receipt.branches_allocated)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            branch_payload_bytes: self
                .replacement_receipt
                .branch_payload_bytes
                .checked_add(self.splice_receipt.branch_payload_bytes)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            maximum_atomic_height: self
                .replacement_receipt
                .maximum_atomic_height
                .max(self.splice_receipt.maximum_atomic_height),
        };
        self.phase = M11RecursiveGreenHostReplayPhase::Complete;
        Ok(M11RecursiveGreenHostReplayOutput {
            root: target_owner,
            work,
        })
    }
}

fn plan_recursive_green_semantic_splice(
    arena: &PageArena,
    sequence: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    page_count: u64,
    event_range: Range<u64>,
) -> Result<RecursiveGreenSemanticSplicePlan, M11RecursiveGreenError> {
    if event_range.start > event_range.end || event_range.end > summary.events {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    if summary.events == 0 {
        if sequence.root_id().is_some() || page_count != 0 || !event_range.is_empty() {
            return Err(M11RecursiveGreenError::Corrupt(
                "empty recursive Green splice base changed shape",
            ));
        }
        return Ok(RecursiveGreenSemanticSplicePlan {
            storage_page_range: 0..0,
            prefix_events: 0,
            suffix_events: 0,
            boundary_events_decoded: 0,
            inspection: SequenceInspectionReceipt::default(),
        });
    }
    if sequence.root_id().is_none() {
        return Err(M11RecursiveGreenError::Corrupt(
            "nonempty recursive Green splice base lost its tree",
        ));
    }
    let mut inspection = SequenceInspectionReceipt::default();

    if event_range.is_empty() && event_range.start == summary.events {
        return Ok(RecursiveGreenSemanticSplicePlan {
            storage_page_range: page_count..page_count,
            prefix_events: 0,
            suffix_events: 0,
            boundary_events_decoded: 0,
            inspection,
        });
    }

    let first = sequence
        .locate_leaf_containing_metric(
            arena,
            event_range.start,
            |value| value.events,
            &mut inspection,
        )?
        .ok_or(M11RecursiveGreenError::Corrupt(
            "recursive Green splice start is absent from complete coverage",
        ))?;
    let first_prefix_events = first.prefix.map_or(0, |prefix| prefix.events);
    let first_local_start = event_range
        .start
        .checked_sub(first_prefix_events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if first_local_start >= first.summary.events {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green splice start escaped its packed page",
        ));
    }
    let first_payload = arena.payload(first.id)?;
    let first_leaf = decode_leaf(first_payload, &mut inspection.spec)?.ok_or(
        M11RecursiveGreenError::Corrupt("recursive Green splice boundary uses RGB1"),
    )?;
    if first_leaf.summary != first.summary || arena.child_count(first.id)? != 0 {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green splice boundary changed shape",
        ));
    }

    if event_range.is_empty() {
        return Ok(RecursiveGreenSemanticSplicePlan {
            storage_page_range: first.ordinal
                ..first
                    .ordinal
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            prefix_events: first_local_start,
            suffix_events: first
                .summary
                .events
                .checked_sub(first_local_start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            boundary_events_decoded: first.summary.events,
            inspection,
        });
    }

    let first_page_end = first_prefix_events
        .checked_add(first.summary.events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if event_range.end <= first_page_end {
        let local_end = event_range
            .end
            .checked_sub(first_prefix_events)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        return Ok(RecursiveGreenSemanticSplicePlan {
            storage_page_range: first.ordinal
                ..first
                    .ordinal
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            prefix_events: first_local_start,
            suffix_events: first
                .summary
                .events
                .checked_sub(local_end)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            boundary_events_decoded: first.summary.events,
            inspection,
        });
    }

    let last_probe = event_range
        .end
        .checked_sub(1)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let last = sequence
        .locate_leaf_containing_metric(arena, last_probe, |value| value.events, &mut inspection)?
        .ok_or(M11RecursiveGreenError::Corrupt(
            "recursive Green splice end is absent from complete coverage",
        ))?;
    let last_prefix_events = last.prefix.map_or(0, |prefix| prefix.events);
    let last_local_end = event_range
        .end
        .checked_sub(last_prefix_events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let last_payload = arena.payload(last.id)?;
    let last_leaf = decode_leaf(last_payload, &mut inspection.spec)?.ok_or(
        M11RecursiveGreenError::Corrupt("recursive Green splice boundary uses RGB1"),
    )?;
    if last_leaf.summary != last.summary
        || arena.child_count(last.id)? != 0
        || last_local_end == 0
        || last_local_end > last.summary.events
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive Green splice end changed shape",
        ));
    }
    Ok(RecursiveGreenSemanticSplicePlan {
        storage_page_range: first.ordinal
            ..last
                .ordinal
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        prefix_events: first_local_start,
        suffix_events: last
            .summary
            .events
            .checked_sub(last_local_end)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        boundary_events_decoded: first
            .summary
            .events
            .checked_add(last.summary.events)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        inspection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recursive_green::build::{
        M11RecursiveGreenBuild, M11RecursiveGreenBuildStatus, M11RecursiveGreenRoot,
        M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS,
    };
    use crate::recursive_green::codec::{
        M11RecursiveGreenClosedChild, M11RecursiveGreenCoveragePart, M11RecursiveGreenEvent,
        M11RecursiveGreenFrameId, M11RecursiveGreenKind, M11RecursiveGreenLogicalAction,
        M11RecursiveGreenSourceMetric,
    };
    use crate::recursive_green::publication::descriptor_for;
    use crate::storage::{ArenaLimits, ArenaMetrics};
    use crate::{DocumentRuntime, DocumentRuntimeConfig};

    fn frame(value: u64) -> M11RecursiveGreenFrameId {
        M11RecursiveGreenFrameId::new(value).expect("nonzero frame")
    }

    fn kind(value: u16) -> M11RecursiveGreenKind {
        M11RecursiveGreenKind::new(value).expect("nonzero kind")
    }

    fn offer(
        build: &mut M11RecursiveGreenBuild,
        runtime: &mut DocumentRuntime,
        event: M11RecursiveGreenEvent,
    ) {
        build
            .offer_event(event)
            .expect("offer recursive Green event");
        loop {
            let poll = build
                .poll(runtime, M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS)
                .expect("poll recursive Green event");
            if poll.status() == M11RecursiveGreenBuildStatus::NeedsInput {
                break;
            }
            assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
        }
    }

    fn build_root(
        runtime: &mut DocumentRuntime,
        items: usize,
        changed_items: &[usize],
    ) -> M11RecursiveGreenRoot {
        let lease = runtime.snapshot_current_source().expect("source lease");
        let mut build = M11RecursiveGreenBuild::new(runtime, lease).expect("Green build");
        offer(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Enter {
                frame: frame(1),
                kind: kind(1),
            },
        );
        for index in 0..items {
            let frame = frame(u64::try_from(index + 2).expect("frame fits u64"));
            let item_kind = if changed_items.contains(&index) {
                kind(9)
            } else {
                kind(2)
            };
            offer(
                &mut build,
                runtime,
                M11RecursiveGreenEvent::Enter {
                    frame,
                    kind: item_kind,
                },
            );
            offer(
                &mut build,
                runtime,
                M11RecursiveGreenEvent::Coverage {
                    physical: M11RecursiveGreenSourceMetric::new(1, 1).expect("metric"),
                    owner_depth: 0,
                    part: M11RecursiveGreenCoveragePart::Content,
                    logical: M11RecursiveGreenLogicalAction::Identity,
                },
            );
            offer(
                &mut build,
                runtime,
                M11RecursiveGreenEvent::Exit {
                    frame,
                    final_kind: item_kind,
                    close: None,
                    last_line_blank: false,
                    child: M11RecursiveGreenClosedChild::default(),
                },
            );
        }
        offer(
            &mut build,
            runtime,
            M11RecursiveGreenEvent::Exit {
                frame: frame(1),
                final_kind: kind(1),
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            },
        );
        build.finish_input().expect("finish Green input");
        loop {
            let poll = build
                .poll(runtime, M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS)
                .expect("finish Green build");
            if poll.status() == M11RecursiveGreenBuildStatus::Complete {
                break;
            }
            assert_eq!(poll.status(), M11RecursiveGreenBuildStatus::Pending);
        }
        build.take_root().expect("Green root")
    }

    fn copy_closure_into_build(
        source: &PageArena,
        id: ArenaId,
        session: &mut ArenaBuildSession<'_>,
    ) -> ArenaBuildOwner {
        let child_count = source.child_count(id).expect("source child count");
        let mut children = Vec::with_capacity(child_count);
        for index in 0..child_count {
            let child = source.child_at(id, index).expect("source child");
            children.push(copy_closure_into_build(source, child, session));
        }
        let child_ids: Vec<_> = children.iter().map(ArenaBuildOwner::id).collect();
        let owner = session
            .allocate(source.payload(id).expect("source payload"), &child_ids)
            .expect("copy source node");
        for child in children {
            session.release(child).expect("release copied child owner");
        }
        owner
    }

    fn drain_host_abort(host: &mut PageArena) {
        loop {
            let ArenaMetrics {
                live_builds,
                pending_build_aborts,
                pending_reclaims,
                ..
            } = host.metrics();
            if live_builds == 0 && pending_build_aborts == 0 && pending_reclaims == 0 {
                break;
            }
            host.poll_reclaim(256);
        }
    }

    fn host_replay_claim_is_rejected(
        producer: &PageArena,
        base_root: ArenaId,
        claim: M11RecursiveGreenHostSpliceClaim,
    ) -> bool {
        let mut host = PageArena::new(ArenaLimits::default()).expect("rejecting host arena");
        let rejected = {
            let mut session = host.begin_build().expect("rejecting host build");
            let base_owner = copy_closure_into_build(producer, base_root, &mut session);
            M11RecursiveGreenHostReplay::new(&session, Some(base_owner), claim).is_err()
        };
        drain_host_abort(&mut host);
        rejected
    }

    fn replay_case(
        items: usize,
        changed_items: &[usize],
    ) -> (M11RecursiveGreenHostSpliceWork, u64) {
        let mut runtime =
            DocumentRuntime::new(&"x".repeat(items), DocumentRuntimeConfig::default())
                .expect("runtime");
        assert!(!changed_items.is_empty());
        assert!(changed_items.windows(2).all(|pair| pair[0] < pair[1]));
        let mut base = build_root(&mut runtime, items, &[]);
        let mut target = build_root(&mut runtime, items, changed_items);
        assert_ne!(
            base.canonical_event_commitment256(),
            target.canonical_event_commitment256(),
            "the local retype must change canonical Green authority"
        );
        let base_descriptor = descriptor_for(base.summary, base.page_count, base.tree_height);
        let target_descriptor =
            descriptor_for(target.summary, target.page_count, target.tree_height);
        let base_root = base.tree_root_id_for_test().expect("base tree root");
        let target_root = target.tree_root_id_for_test().expect("target tree root");
        let mut segment_claims = Vec::new();
        let mut target_storage_ranges = Vec::new();
        for &changed_item in changed_items {
            let item_start = 1 + u64::try_from(changed_item).expect("item fits u64") * 3;
            let event_range = item_start..item_start + 3;
            let base_plan = plan_persistent_m11_recursive_green_semantic_splice(
                runtime.producer_arena(),
                Some(base_root),
                base_descriptor,
                event_range.clone(),
            )
            .expect("base semantic cut");
            let target_plan = plan_persistent_m11_recursive_green_semantic_splice(
                runtime.producer_arena(),
                Some(target_root),
                target_descriptor,
                event_range.clone(),
            )
            .expect("target semantic cut");
            assert_eq!(
                base_plan.storage_page_range.start, target_plan.storage_page_range.start,
                "unchanged semantic prefix must retain the packed-page boundary"
            );
            segment_claims.push(M11RecursiveGreenHostSpliceSegmentClaim {
                base_event_range: event_range.clone(),
                target_event_range: event_range,
                base_storage_range: base_plan.storage_page_range,
                target_storage_range: target_plan.storage_page_range.clone(),
            });
            target_storage_ranges.push(target_plan.storage_page_range);
        }
        let claim = M11RecursiveGreenHostSpliceClaim {
            segments: segment_claims.into_boxed_slice(),
            base_descriptor,
            target_descriptor,
        };

        let mut host = PageArena::new(ArenaLimits::default()).expect("host arena");
        let (work, output_commitment) = {
            let mut session = host.begin_build().expect("host build");
            let base_owner =
                copy_closure_into_build(runtime.producer_arena(), base_root, &mut session);
            let mut replay =
                M11RecursiveGreenHostReplay::new(&session, Some(base_owner), claim.clone())
                    .expect("begin Green host replay");
            for target_storage_range in target_storage_ranges.iter().cloned() {
                for ordinal in target_storage_range {
                    let payload = persistent_m11_recursive_green_storage_page_at(
                        runtime.producer_arena(),
                        Some(target_root),
                        ordinal,
                    )
                    .expect("read target Green page")
                    .expect("target Green page exists");
                    let leaf = session.allocate(payload, &[]).expect("import target RGL1");
                    replay
                        .offer_replacement_leaf(&mut session, leaf)
                        .expect("offer target RGL1");
                    while replay
                        .poll_replacement(&mut session)
                        .expect("poll target RGL1")
                        == M11RecursiveGreenHostReplayPoll::Pending
                    {}
                }
            }
            if replay
                .finish_replacement(&session)
                .expect("finish RGL1 input")
                == M11RecursiveGreenHostReplayPoll::Pending
            {
                while replay
                    .poll_replacement(&mut session)
                    .expect("finish RGL1 builder")
                    == M11RecursiveGreenHostReplayPoll::Pending
                {}
            }
            let output = replay
                .complete(&mut session)
                .expect("complete Green replay");
            let (owner, work) = output.into_parts();
            let owner = owner.expect("nonempty replay root");
            let output_claim = validate_persistent_m11_recursive_green_root(
                session.arena(),
                Some(owner.id()),
                target_descriptor,
            )
            .expect("independently validate replay target");
            let commitment = output_claim.summary().canonical_commitment.checksum();
            assert_eq!(commitment, target.canonical_event_commitment256());
            (work, commitment)
        };
        drain_host_abort(&mut host);

        // A branch-shaped RGB1 payload is never admissible as replacement
        // material, even if it came from the authentic target closure.
        assert_eq!(
            runtime
                .producer_arena()
                .payload(target_root)
                .expect("target root payload")
                .get(..4),
            Some(b"RGB1".as_slice())
        );
        let mut rejecting_host = PageArena::new(ArenaLimits::default()).expect("rejecting host");
        {
            let mut session = rejecting_host.begin_build().expect("rejecting build");
            let base_owner =
                copy_closure_into_build(runtime.producer_arena(), base_root, &mut session);
            let mut replay = M11RecursiveGreenHostReplay::new(&session, Some(base_owner), claim)
                .expect("begin rejecting replay");
            let rgb = session
                .allocate(
                    runtime
                        .producer_arena()
                        .payload(target_root)
                        .expect("RGB1 payload"),
                    &[],
                )
                .expect("import raw RGB1 payload");
            assert!(replay.offer_replacement_leaf(&mut session, rgb).is_err());
        }
        drain_host_abort(&mut rejecting_host);

        base.begin_release(&mut runtime).expect("release base");
        target.begin_release(&mut runtime).expect("release target");
        while !base
            .poll_release(&mut runtime, 256)
            .expect("poll Green release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        (
            work,
            u64::from_le_bytes(output_commitment[..8].try_into().unwrap()),
        )
    }

    #[test]
    fn local_semantic_replay_is_size_independent_leaf_only_and_commitment_exact() {
        let (small, small_commitment_lane) = replay_case(256, &[128]);
        let (large, large_commitment_lane) = replay_case(8_192, &[4_096]);

        assert!(small.base_storage_pages() >= 2);
        assert!(large.base_storage_pages() > small.base_storage_pages() * 16);
        assert!(small.transferred_storage_pages() <= 2);
        assert!(large.transferred_storage_pages() <= 2);
        assert_eq!(small.deleted_events(), 3);
        assert_eq!(large.deleted_events(), 3);
        assert_eq!(small.replacement_events(), 3);
        assert_eq!(large.replacement_events(), 3);
        assert!(large.reused_storage_pages() > small.reused_storage_pages() * 16);
        assert_ne!(small_commitment_lane, 0);
        assert_ne!(large_commitment_lane, 0);
    }

    #[test]
    fn sparse_semantic_replay_applies_all_segments_in_one_host_build() {
        let (work, commitment_lane) = replay_case(1_024, &[128, 896]);

        assert_eq!(work.deleted_events(), 6);
        assert_eq!(work.replacement_events(), 6);
        assert!(work.transferred_storage_pages() <= 4);
        assert!(work.reused_storage_pages() > work.transferred_storage_pages() * 8);
        assert_ne!(commitment_lane, 0);
    }

    #[test]
    fn splice_batch_rejects_unsorted_segments_and_broken_cumulative_mapping() {
        let mut runtime = DocumentRuntime::new(&"x".repeat(512), DocumentRuntimeConfig::default())
            .expect("runtime");
        let mut base = build_root(&mut runtime, 512, &[]);
        let descriptor = descriptor_for(base.summary, base.page_count, base.tree_height);
        let base_root = base.tree_root_id_for_test().expect("base tree root");
        let first_events = 1..4;
        let later_events = 901..904;
        let first_plan = plan_persistent_m11_recursive_green_semantic_splice(
            runtime.producer_arena(),
            Some(base_root),
            descriptor,
            first_events.clone(),
        )
        .expect("first semantic cut");
        let later_plan = plan_persistent_m11_recursive_green_semantic_splice(
            runtime.producer_arena(),
            Some(base_root),
            descriptor,
            later_events.clone(),
        )
        .expect("later semantic cut");
        assert!(first_plan.storage_page_range.end < later_plan.storage_page_range.start);

        let first = M11RecursiveGreenHostSpliceSegmentClaim {
            base_event_range: first_events.clone(),
            target_event_range: first_events.clone(),
            base_storage_range: first_plan.storage_page_range.clone(),
            target_storage_range: first_plan.storage_page_range.clone(),
        };
        let later = M11RecursiveGreenHostSpliceSegmentClaim {
            base_event_range: later_events.clone(),
            target_event_range: later_events,
            base_storage_range: later_plan.storage_page_range.clone(),
            target_storage_range: later_plan.storage_page_range,
        };
        assert!(host_replay_claim_is_rejected(
            runtime.producer_arena(),
            base_root,
            M11RecursiveGreenHostSpliceClaim {
                segments: vec![later, first.clone()].into_boxed_slice(),
                base_descriptor: descriptor,
                target_descriptor: descriptor,
            },
        ));

        let mut shifted_target = first.clone();
        shifted_target.target_event_range = 4..7;
        assert!(host_replay_claim_is_rejected(
            runtime.producer_arena(),
            base_root,
            M11RecursiveGreenHostSpliceClaim {
                segments: vec![shifted_target].into_boxed_slice(),
                base_descriptor: descriptor,
                target_descriptor: descriptor,
            },
        ));

        let mut wrong_event_arithmetic = first.clone();
        wrong_event_arithmetic.target_event_range = 1..5;
        assert!(host_replay_claim_is_rejected(
            runtime.producer_arena(),
            base_root,
            M11RecursiveGreenHostSpliceClaim {
                segments: vec![wrong_event_arithmetic].into_boxed_slice(),
                base_descriptor: descriptor,
                target_descriptor: descriptor,
            },
        ));

        let mut wrong_page_arithmetic = first;
        wrong_page_arithmetic.target_storage_range.end += 1;
        assert!(wrong_page_arithmetic.target_storage_range.end <= descriptor.storage_page_count());
        assert!(host_replay_claim_is_rejected(
            runtime.producer_arena(),
            base_root,
            M11RecursiveGreenHostSpliceClaim {
                segments: vec![wrong_page_arithmetic].into_boxed_slice(),
                base_descriptor: descriptor,
                target_descriptor: descriptor,
            },
        ));

        base.begin_release(&mut runtime).expect("release base");
        while !base
            .poll_release(&mut runtime, 256)
            .expect("poll Green release")
            .complete()
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
    }
}

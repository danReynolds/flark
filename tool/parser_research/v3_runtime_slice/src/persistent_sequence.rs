//! One grammar-free, transactional persistent sequence primitive.
//!
//! The parser output contenders supply only leaf and aggregate semantics. This
//! module owns balancing, suffix sharing, and ownership-token rollback so the
//! event tape and record forest cannot grow subtly different tree machinery.

use std::cmp::Ordering;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;

use crate::arena::{
    ArenaBuildError, ArenaBuildOwner, ArenaBuildSession, ArenaBuildTransaction, ArenaOwnerHandle,
};
use crate::{ArenaError, ArenaId, OwnedArenaRef, PageArena};

const MAX_RESUMABLE_SEQUENCE_BIN_SLOTS: usize = u64::BITS as usize;

pub(crate) trait SequenceSpec {
    type Summary: Copy;
    type Error: From<ArenaError>;
    type BranchPayload: AsRef<[u8]>;

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error>;
    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error>;
    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload;
    /// Optional third ownership edge authenticated by a branch summary.
    ///
    /// Most sequences need only their left and right children and therefore
    /// retain the original two-child representation byte-for-byte. Formats
    /// whose routing summary names an out-of-line witness can opt into one
    /// packed third edge so a scalar arena ID in the payload never becomes
    /// unauthenticated authority.
    fn branch_witness(_summary: Self::Summary) -> Option<crate::ArenaId> {
        None
    }
    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error>;
    fn leaves(summary: Self::Summary) -> u64;
    fn height(summary: Self::Summary) -> u16;
    fn invalid(message: &'static str) -> Self::Error;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequenceNodeKind {
    Leaf,
    Branch { left: ArenaId, right: ArenaId },
}

pub(crate) fn sequence_node<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(Spec::Summary, SequenceNodeKind), Spec::Error> {
    let payload = arena.payload(id)?;
    if let Some(summary) = Spec::branch_summary(payload)? {
        let witness = Spec::branch_witness(summary);
        let expected_children = 2 + usize::from(witness.is_some());
        if arena.packed_child_count(id)? != expected_children {
            return Err(Spec::invalid(
                "sequence branch has the wrong ownership-edge count",
            ));
        }
        let left = arena.packed_child_at(id, 0)?;
        let right = arena.packed_child_at(id, 1)?;
        if let Some(expected_witness) = witness
            && arena.packed_child_at(id, 2)? != expected_witness
        {
            return Err(Spec::invalid(
                "sequence branch ownership witness does not match its summary",
            ));
        }
        return Ok((summary, SequenceNodeKind::Branch { left, right }));
    }
    Spec::leaf_summary(payload)?
        .map(|summary| (summary, SequenceNodeKind::Leaf))
        .ok_or_else(|| Spec::invalid("unknown sequence node"))
}

#[derive(Debug)]
pub(crate) struct SealedSequenceLeaf {
    owner: OwnedArenaRef,
}

impl SealedSequenceLeaf {
    pub(crate) const fn new(owner: OwnedArenaRef) -> Self {
        Self { owner }
    }

    #[cfg(test)]
    pub(crate) const fn id(&self) -> ArenaId {
        self.owner.id()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SequenceMutationReceipt {
    pub leaves_adopted: usize,
    pub branches_allocated: usize,
    pub branch_payload_bytes_copied: usize,
    pub nodes_visited: usize,
    pub child_references_added: usize,
    pub leaves_reused: usize,
    pub maximum_streaming_roots: usize,
    pub maximum_streaming_bin_slots: usize,
    pub maximum_streaming_bin_bytes: usize,
    pub maximum_resumable_bin_logical_slots: usize,
    pub maximum_resumable_bin_requested_bytes: usize,
    pub maximum_resumable_join_tasks: usize,
    pub maximum_resumable_join_task_requested_bytes: usize,
    pub maximum_resumable_join_task_bytes: usize,
    pub maximum_resumable_join_values: usize,
    pub maximum_resumable_join_value_requested_bytes: usize,
    pub maximum_resumable_join_value_bytes: usize,
    pub resumable_join_scratch_reservations: usize,
    pub resumable_split_polls: usize,
    pub resumable_split_frame_reservations: usize,
    pub maximum_resumable_split_frames: usize,
    pub maximum_resumable_split_frame_requested_bytes: usize,
    pub maximum_resumable_split_frame_bytes: usize,
    pub maximum_resumable_split_total_requested_bytes: usize,
    pub maximum_resumable_split_total_scratch_bytes: usize,
    pub resumable_splice_polls: usize,
    pub resumable_splice_deleted_roots_released: usize,
    pub maximum_resumable_splice_total_requested_bytes: usize,
    pub maximum_resumable_splice_total_scratch_bytes: usize,
}

trait SequenceBuildBackend {
    type Owner;
    type Error;

    fn arena(&self) -> &PageArena;
    fn owner_id(&self, owner: &Self::Owner) -> Result<ArenaId, Self::Error>;
    fn retain_sequence_node(&mut self, id: ArenaId) -> Result<Self::Owner, Self::Error>;
    fn allocate_sequence_node(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(Self::Owner, crate::AllocationReceipt), Self::Error>;
    fn allocate_packed_sequence_node(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(Self::Owner, crate::AllocationReceipt), Self::Error>;
    fn release_sequence_owner(&mut self, owner: Self::Owner) -> Result<(), Self::Error>;
}

impl SequenceBuildBackend for ArenaBuildTransaction<'_> {
    type Owner = ArenaOwnerHandle;
    type Error = ArenaError;

    fn arena(&self) -> &PageArena {
        self.arena()
    }

    fn owner_id(&self, owner: &Self::Owner) -> Result<ArenaId, Self::Error> {
        Ok(self.id(owner))
    }

    fn retain_sequence_node(&mut self, id: ArenaId) -> Result<Self::Owner, Self::Error> {
        self.retain(id)
    }

    fn allocate_sequence_node(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(Self::Owner, crate::AllocationReceipt), Self::Error> {
        self.allocate(payload, children)
    }

    fn allocate_packed_sequence_node(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(Self::Owner, crate::AllocationReceipt), Self::Error> {
        self.allocate_packed(payload, children)
    }

    fn release_sequence_owner(&mut self, owner: Self::Owner) -> Result<(), Self::Error> {
        self.release(owner)
    }
}

impl SequenceBuildBackend for ArenaBuildSession<'_> {
    type Owner = ArenaBuildOwner;
    type Error = ArenaBuildError;

    fn arena(&self) -> &PageArena {
        self.arena()
    }

    fn owner_id(&self, owner: &Self::Owner) -> Result<ArenaId, Self::Error> {
        self.owner_id(owner)
    }

    fn retain_sequence_node(&mut self, id: ArenaId) -> Result<Self::Owner, Self::Error> {
        self.retain(id)
    }

    fn allocate_sequence_node(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(Self::Owner, crate::AllocationReceipt), Self::Error> {
        self.allocate(payload, children)
    }

    fn allocate_packed_sequence_node(
        &mut self,
        payload: &[u8],
        children: &[ArenaId],
    ) -> Result<(Self::Owner, crate::AllocationReceipt), Self::Error> {
        self.allocate_packed(payload, children)
    }

    fn release_sequence_owner(&mut self, owner: Self::Owner) -> Result<(), Self::Error> {
        self.release(owner)
    }
}

fn make_branch<Spec, Backend>(
    backend: &mut Backend,
    left: Backend::Owner,
    right: Backend::Owner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Backend::Owner, Spec::Error>
where
    Spec: SequenceSpec,
    Backend: SequenceBuildBackend,
    Spec::Error: From<Backend::Error>,
{
    let left_id = backend.owner_id(&left)?;
    let right_id = backend.owner_id(&right)?;
    let left_summary = sequence_node::<Spec>(backend.arena(), left_id)?.0;
    let right_summary = sequence_node::<Spec>(backend.arena(), right_id)?.0;
    let summary = Spec::combine(left_summary, right_summary)?;
    let payload = Spec::encode_branch(summary);
    let (owner, allocation) = if let Some(witness) = Spec::branch_witness(summary) {
        let children = [left_id, right_id, witness];
        backend.allocate_packed_sequence_node(payload.as_ref(), &children)?
    } else {
        let children = [left_id, right_id];
        backend.allocate_sequence_node(payload.as_ref(), &children)?
    };
    backend.release_sequence_owner(left)?;
    backend.release_sequence_owner(right)?;
    receipt.branches_allocated += 1;
    receipt.branch_payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.child_references_added += allocation.child_references_added;
    Ok(owner)
}

/// Binomial-carry builder for a persistent sequence.
///
/// At most one completed subtree per power-of-two leaf count is live in the
/// builder. The arena transaction owns every handle, so cancellation at any
/// push or join rolls back without a second cleanup path.
#[derive(Debug)]
pub(crate) struct StreamingSequenceBuilder<Spec> {
    bins: Vec<Option<ArenaOwnerHandle>>,
    marker: PhantomData<Spec>,
}

impl<Spec> Default for StreamingSequenceBuilder<Spec> {
    fn default() -> Self {
        Self {
            bins: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<Spec: SequenceSpec> StreamingSequenceBuilder<Spec> {
    pub(crate) fn push_handle(
        &mut self,
        transaction: &mut ArenaBuildTransaction<'_>,
        mut carry: ArenaOwnerHandle,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        // Validate a caller-provided leaf before it is combined. Its owner is
        // already journaled, so malformed payloads roll back transactionally.
        let (_, kind) = sequence_node::<Spec>(transaction.arena(), transaction.id(&carry))?;
        if kind != SequenceNodeKind::Leaf {
            return Err(Spec::invalid("streaming sequence input is not a leaf"));
        }
        receipt.leaves_adopted += 1;
        let mut level = 0;
        loop {
            if level == self.bins.len() {
                self.bins.push(Some(carry));
                break;
            }
            if let Some(left) = self.bins[level].take() {
                carry = make_branch::<Spec, _>(transaction, left, carry, receipt)?;
                level += 1;
            } else {
                self.bins[level] = Some(carry);
                break;
            }
        }
        receipt.maximum_streaming_roots = receipt
            .maximum_streaming_roots
            .max(self.bins.iter().filter(|root| root.is_some()).count());
        receipt.maximum_streaming_bin_slots = receipt
            .maximum_streaming_bin_slots
            .max(self.bins.capacity());
        receipt.maximum_streaming_bin_bytes = receipt
            .maximum_streaming_bin_bytes
            .max(self.bins.capacity() * std::mem::size_of::<Option<ArenaOwnerHandle>>());
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        transaction: &mut ArenaBuildTransaction<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
        let mut root = None;
        for subtree in self.bins.iter_mut().rev().filter_map(Option::take) {
            root = concat_nodes::<Spec>(transaction, root, Some(subtree), receipt)?;
        }
        Ok(root)
    }
}

/// Allocation-granular binomial builder for an arena-owned resumable build.
///
/// A push carries at most one root through the bins, and one poll allocates at
/// most one branch. Final reduction folds the logarithmic bin roots through an
/// explicit AVL join/rotation interpreter; one poll executes one bounded task
/// and allocates at most one branch.
#[derive(Debug)]
pub(crate) struct ResumableStreamingSequenceBuilder<Spec> {
    bins: Vec<Option<ArenaBuildOwner>>,
    bin_slot_limit: usize,
    bin_capacity: usize,
    bin_requested_bytes: usize,
    carry: Option<(ArenaBuildOwner, usize)>,
    reduction: Option<SequenceReduction<Spec>>,
    join: ResumableSequenceJoin<Spec>,
    marker: PhantomData<Spec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResumableSequenceProgress {
    Complete,
    Pending,
}

#[derive(Debug)]
struct SequenceReduction<Spec> {
    next_bin: usize,
    root: Option<ArenaBuildOwner>,
    joining: bool,
    marker: PhantomData<Spec>,
}

#[derive(Debug)]
struct ResumableSequenceJoin<Spec> {
    tasks: Vec<ResumableJoinTask>,
    values: Vec<ArenaBuildOwner>,
    task_requested_bytes: usize,
    task_slot_limit: usize,
    task_capacity: usize,
    value_requested_bytes: usize,
    value_slot_limit: usize,
    value_capacity: usize,
    marker: PhantomData<Spec>,
}

/// Progress of one persistent-sequence boundary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ResumableSequenceSplitProgress {
    Pending,
    Complete,
}

#[derive(Debug)]
enum ResumableSplitFrame {
    /// The split descended through the right child. This untouched left
    /// sibling precedes the prefix returned by the descendant.
    Prefix(ArenaBuildOwner),
    /// The split descended through the left child. This untouched right
    /// sibling follows the suffix returned by the descendant.
    Suffix(ArenaBuildOwner),
}

#[derive(Debug)]
enum ResumableSplitJoinSide {
    Prefix { suffix: Option<ArenaBuildOwner> },
    Suffix { prefix: Option<ArenaBuildOwner> },
}

#[derive(Debug)]
enum ResumableSplitPhase {
    RetainBase {
        root: ArenaId,
        leaf_index: u64,
    },
    Descend {
        node: ArenaBuildOwner,
        leaf_index: u64,
    },
    Unwind {
        prefix: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    },
    Join {
        side: ResumableSplitJoinSide,
    },
    Complete {
        prefix: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    },
    Taken,
    Failed,
}

/// Allocation-granular split of one immutable persistent-sequence root.
///
/// The job retains the base root into the build journal, descends only the
/// logarithmic boundary path, and retains one untouched sibling per level.
/// Unwinding delegates all balancing and path-copy allocations to the same
/// explicit [`ResumableSequenceJoin`] interpreter used by fresh builds. One
/// poll performs at most one join task and therefore allocates at most one
/// branch. Every live owner stays in the caller's [`ArenaBuildSession`]
/// journal, so dropping a failed or cancelled job needs no bespoke cleanup.
/// A poll error terminally poisons this job; the caller must abort the whole
/// arena build so journal-owned handles no longer reachable here are released.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResumableSequenceSplit<Spec> {
    build: crate::ArenaBuildId,
    frames: Vec<ResumableSplitFrame>,
    frame_slot_limit: usize,
    frame_capacity: usize,
    frame_requested_bytes: usize,
    join: ResumableSequenceJoin<Spec>,
    phase: ResumableSplitPhase,
}

fn maximum_avl_sequence_height(leaves: u64) -> u16 {
    if leaves == 0 {
        return 0;
    }
    let mut height = 1_u16;
    let mut minimum_at_height = 1_u64;
    let mut minimum_at_next_height = 2_u64;
    while minimum_at_next_height <= leaves {
        height = height.saturating_add(1);
        let Some(next) = minimum_at_height.checked_add(minimum_at_next_height) else {
            break;
        };
        minimum_at_height = minimum_at_next_height;
        minimum_at_next_height = next;
    }
    height
}

/// Validates the summary fields that control split scratch and path choice.
/// Full semantic-summary validation remains the responsibility of the typed
/// sequence format, but corrupt height/leaves fields cannot induce an
/// attacker-sized reservation or an invalid AVL reconstruction here.
fn validated_split_node<Spec: SequenceSpec>(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(Spec::Summary, SequenceNodeKind), Spec::Error> {
    let (summary, kind) = sequence_node::<Spec>(arena, id)?;
    let leaves = Spec::leaves(summary);
    let height = Spec::height(summary);
    if leaves == 0 || height == 0 || height > maximum_avl_sequence_height(leaves) {
        return Err(Spec::invalid("sequence split root has invalid AVL summary"));
    }
    match kind {
        SequenceNodeKind::Leaf => {
            if leaves != 1 || height != 1 {
                return Err(Spec::invalid("sequence leaf has invalid split summary"));
            }
        }
        SequenceNodeKind::Branch { left, right } => {
            let left_summary = sequence_node::<Spec>(arena, left)?.0;
            let right_summary = sequence_node::<Spec>(arena, right)?.0;
            let left_leaves = Spec::leaves(left_summary);
            let right_leaves = Spec::leaves(right_summary);
            let left_height = Spec::height(left_summary);
            let right_height = Spec::height(right_summary);
            let combined_leaves = left_leaves
                .checked_add(right_leaves)
                .ok_or_else(|| Spec::invalid("sequence split leaf count overflow"))?;
            let combined_height = left_height
                .max(right_height)
                .checked_add(1)
                .ok_or_else(|| Spec::invalid("sequence split height overflow"))?;
            if left_leaves == 0
                || right_leaves == 0
                || left_height == 0
                || right_height == 0
                || combined_leaves != leaves
                || combined_height != height
                || left_height.abs_diff(right_height) > 1
            {
                return Err(Spec::invalid("sequence branch has invalid split summary"));
            }
        }
    }
    Ok((summary, kind))
}

#[cfg_attr(not(test), allow(dead_code))]
impl<Spec> ResumableSequenceSplit<Spec>
where
    Spec: SequenceSpec,
    Spec::Error: From<ArenaBuildError>,
{
    /// Binds a split to the exact suspended build generation named by
    /// `ticket`. The immutable `base_root` remains caller-owned; the first
    /// poll retains a working reference into this build's journal.
    ///
    /// `base_root` must be a storage-private root minted by the matching
    /// sequence builder or validated manifest. The bounded local checks here
    /// reject summary-controlled scratch bombs and corrupt nodes on the split
    /// path; recursively authenticating an arbitrary raw arena root would be
    /// whole-tree work and is intentionally not this O(log n) primitive.
    pub(crate) fn try_new(
        ticket: &crate::ArenaBuildTicket,
        arena: &PageArena,
        base_root: ArenaId,
        leaf_index: u64,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        // This also rejects a ticket from another arena before a colliding
        // local ArenaId could be interpreted in the wrong PageArena.
        arena.build_lifecycle(ticket.id())?;
        let summary = validated_split_node::<Spec>(arena, base_root)?.0;
        if leaf_index > Spec::leaves(summary) {
            return Err(Spec::invalid("resumable sequence split is out of range"));
        }
        Self::with_phase(
            ticket.id(),
            summary,
            ResumableSplitPhase::RetainBase {
                root: base_root,
                leaf_index,
            },
            receipt,
        )
    }

    fn restart_from_owned(
        &mut self,
        session: &ArenaBuildSession<'_>,
        root: ArenaBuildOwner,
        leaf_index: u64,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        if session.id() != self.build {
            return Err(Spec::invalid(
                "resumable sequence job belongs to another build generation",
            ));
        }
        if !matches!(self.phase, ResumableSplitPhase::Taken)
            || !self.frames.is_empty()
            || !self.join.tasks.is_empty()
            || !self.join.values.is_empty()
        {
            return Err(Spec::invalid(
                "resumable sequence split cannot restart with live state",
            ));
        }
        let summary = validated_split_node::<Spec>(session.arena(), session.owner_id(&root)?)?.0;
        if leaf_index > Spec::leaves(summary) {
            return Err(Spec::invalid("resumable sequence split is out of range"));
        }
        let height = Spec::height(summary);
        if usize::from(height.saturating_sub(1)) > self.frame_slot_limit
            || usize::from(height).saturating_add(4) > self.join.task_slot_limit
        {
            return Err(Spec::invalid(
                "retained sequence tail exceeds preflighted split scratch",
            ));
        }
        self.phase = ResumableSplitPhase::Descend {
            node: root,
            leaf_index,
        };
        self.record_scratch(receipt);
        Ok(())
    }

    fn with_phase(
        build: crate::ArenaBuildId,
        summary: Spec::Summary,
        phase: ResumableSplitPhase,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        Self::with_phase_and_join_height(build, summary, Spec::height(summary), phase, receipt)
    }

    fn with_phase_and_join_height(
        build: crate::ArenaBuildId,
        summary: Spec::Summary,
        join_maximum_height: u16,
        phase: ResumableSplitPhase,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        let height = Spec::height(summary);
        if height == 0 || Spec::leaves(summary) == 0 {
            return Err(Spec::invalid("resumable sequence root has no leaves"));
        }
        let mut split = Self::try_preallocated(build, height, join_maximum_height, receipt)?;
        split.phase = phase;
        split.record_scratch(receipt);
        Ok(split)
    }

    fn try_preallocated(
        build: crate::ArenaBuildId,
        maximum_height: u16,
        join_maximum_height: u16,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        if maximum_height == 0 {
            return Err(Spec::invalid("resumable split height bound is zero"));
        }
        if join_maximum_height < maximum_height {
            return Err(Spec::invalid(
                "resumable split join preflight is below root height",
            ));
        }
        let frame_slot_limit = usize::from(maximum_height.saturating_sub(1));
        let frame_requested_bytes = frame_slot_limit
            .checked_mul(std::mem::size_of::<ResumableSplitFrame>())
            .ok_or_else(|| Spec::invalid("resumable split frame bytes overflow"))?;
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(frame_slot_limit)
            .map_err(|_| Spec::invalid("resumable split frame reservation failed"))?;
        receipt.resumable_split_frame_reservations = receipt
            .resumable_split_frame_reservations
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("resumable split reservation count overflow"))?;
        let frame_capacity = frames.capacity();
        // Reserve one interpreter for the root's maximum possible join depth
        // and reuse it at every unwind level. No split-local scratch reserve
        // occurs after this constructor succeeds (the arena journal may still
        // admit one of its separately bounded metadata segments).
        let join = ResumableSequenceJoin::<Spec>::try_preallocated(join_maximum_height, receipt)?;
        let split = Self {
            build,
            frames,
            frame_slot_limit,
            frame_capacity,
            frame_requested_bytes,
            join,
            phase: ResumableSplitPhase::Taken,
        };
        split.record_scratch(receipt);
        Ok(split)
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> crate::ArenaBuildId {
        self.build
    }

    /// Executes one bounded state-machine step. A step may retain/release the
    /// constant number of owners at one tree level, reserve one height-bounded
    /// join interpreter, or execute exactly one join task. It never allocates
    /// more than one arena branch.
    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceSplitProgress, Spec::Error> {
        self.ensure_session(session)?;
        self.require_fixed_frame_capacity()?;
        receipt.resumable_split_polls = receipt
            .resumable_split_polls
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("resumable split poll count overflow"))?;
        let phase = std::mem::replace(&mut self.phase, ResumableSplitPhase::Failed);
        let result = self.poll_phase(session, receipt, phase);
        match result {
            Ok((phase, progress)) => {
                self.phase = phase;
                self.require_fixed_frame_capacity()?;
                self.record_scratch(receipt);
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    fn poll_phase(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        phase: ResumableSplitPhase,
    ) -> Result<(ResumableSplitPhase, ResumableSequenceSplitProgress), Spec::Error> {
        match phase {
            ResumableSplitPhase::RetainBase { root, leaf_index } => {
                let node = session.retain(root)?;
                Ok((
                    ResumableSplitPhase::Descend { node, leaf_index },
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
            ResumableSplitPhase::Descend { node, leaf_index } => {
                self.poll_descend(session, receipt, node, leaf_index)
            }
            ResumableSplitPhase::Unwind { prefix, suffix } => {
                self.poll_unwind(session, receipt, prefix, suffix)
            }
            ResumableSplitPhase::Join { side } => self.poll_join(session, receipt, side),
            complete @ ResumableSplitPhase::Complete { .. } => {
                Ok((complete, ResumableSequenceSplitProgress::Complete))
            }
            ResumableSplitPhase::Taken => Err(Spec::invalid(
                "resumable sequence split output was already taken",
            )),
            ResumableSplitPhase::Failed => {
                Err(Spec::invalid("resumable sequence split is poisoned"))
            }
        }
    }

    fn poll_descend(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        node: ArenaBuildOwner,
        leaf_index: u64,
    ) -> Result<(ResumableSplitPhase, ResumableSequenceSplitProgress), Spec::Error> {
        let node_id = session.owner_id(&node)?;
        let (summary, kind) = validated_split_node::<Spec>(session.arena(), node_id)?;
        receipt.nodes_visited = receipt
            .nodes_visited
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("sequence visit count overflow"))?;
        let leaves = Spec::leaves(summary);
        if leaf_index > leaves {
            return Err(Spec::invalid(
                "resumable sequence split became out of range",
            ));
        }
        if leaf_index == 0 || leaf_index == leaves {
            let (prefix, suffix) = if leaf_index == 0 {
                (None, Some(node))
            } else {
                (Some(node), None)
            };
            return Ok((
                ResumableSplitPhase::Unwind { prefix, suffix },
                ResumableSequenceSplitProgress::Pending,
            ));
        }
        let SequenceNodeKind::Branch { left, right } = kind else {
            return Err(Spec::invalid("interior sequence split reached a leaf"));
        };
        let left_leaves = Spec::leaves(sequence_node::<Spec>(session.arena(), left)?.0);
        let left_owner = session.retain(left)?;
        let right_owner = session.retain(right)?;
        session.release(node)?;
        match leaf_index.cmp(&left_leaves) {
            Ordering::Less => {
                self.push_frame(ResumableSplitFrame::Suffix(right_owner))?;
                Ok((
                    ResumableSplitPhase::Descend {
                        node: left_owner,
                        leaf_index,
                    },
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
            Ordering::Equal => Ok((
                ResumableSplitPhase::Unwind {
                    prefix: Some(left_owner),
                    suffix: Some(right_owner),
                },
                ResumableSequenceSplitProgress::Pending,
            )),
            Ordering::Greater => {
                self.push_frame(ResumableSplitFrame::Prefix(left_owner))?;
                Ok((
                    ResumableSplitPhase::Descend {
                        node: right_owner,
                        leaf_index: leaf_index - left_leaves,
                    },
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
        }
    }

    fn poll_unwind(
        &mut self,
        session: &ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        prefix: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    ) -> Result<(ResumableSplitPhase, ResumableSequenceSplitProgress), Spec::Error> {
        let Some(frame) = self.frames.pop() else {
            return Ok((
                ResumableSplitPhase::Complete { prefix, suffix },
                ResumableSequenceSplitProgress::Complete,
            ));
        };
        let (left, right, side) = match frame {
            ResumableSplitFrame::Prefix(left) => {
                let Some(right) = prefix else {
                    return Ok((
                        ResumableSplitPhase::Unwind {
                            prefix: Some(left),
                            suffix,
                        },
                        ResumableSequenceSplitProgress::Pending,
                    ));
                };
                (left, right, ResumableSplitJoinSide::Prefix { suffix })
            }
            ResumableSplitFrame::Suffix(right) => {
                let Some(left) = suffix else {
                    return Ok((
                        ResumableSplitPhase::Unwind {
                            prefix,
                            suffix: Some(right),
                        },
                        ResumableSequenceSplitProgress::Pending,
                    ));
                };
                (left, right, ResumableSplitJoinSide::Suffix { prefix })
            }
        };
        self.join.begin(session, left, right, receipt)?;
        Ok((
            ResumableSplitPhase::Join { side },
            ResumableSequenceSplitProgress::Pending,
        ))
    }

    fn poll_join(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        side: ResumableSplitJoinSide,
    ) -> Result<(ResumableSplitPhase, ResumableSequenceSplitProgress), Spec::Error> {
        if self.join.poll(session, receipt)? == ResumableSequenceProgress::Pending {
            return Ok((
                ResumableSplitPhase::Join { side },
                ResumableSequenceSplitProgress::Pending,
            ));
        }
        let joined = self.join.take_root()?;
        let phase = match side {
            ResumableSplitJoinSide::Prefix { suffix } => ResumableSplitPhase::Unwind {
                prefix: Some(joined),
                suffix,
            },
            ResumableSplitJoinSide::Suffix { prefix } => ResumableSplitPhase::Unwind {
                prefix,
                suffix: Some(joined),
            },
        };
        Ok((phase, ResumableSequenceSplitProgress::Pending))
    }

    /// Transfers the two build-owned roots after completion. Empty prefix or
    /// suffix boundaries are represented by `None`; this call is linear.
    pub(crate) fn take_parts(
        &mut self,
    ) -> Result<(Option<ArenaBuildOwner>, Option<ArenaBuildOwner>), Spec::Error> {
        let phase = std::mem::replace(&mut self.phase, ResumableSplitPhase::Taken);
        match phase {
            ResumableSplitPhase::Complete { prefix, suffix } => Ok((prefix, suffix)),
            other => {
                self.phase = other;
                Err(Spec::invalid("resumable sequence split is incomplete"))
            }
        }
    }

    fn ensure_session(&self, session: &ArenaBuildSession<'_>) -> Result<(), Spec::Error> {
        if session.id() != self.build {
            return Err(Spec::invalid(
                "resumable sequence job belongs to another build generation",
            ));
        }
        session.live_owners()?;
        Ok(())
    }

    fn push_frame(&mut self, frame: ResumableSplitFrame) -> Result<(), Spec::Error> {
        if self.frames.len() >= self.frame_slot_limit {
            return Err(Spec::invalid(
                "resumable sequence split exceeded logical path bound",
            ));
        }
        self.frames.push(frame);
        Ok(())
    }

    fn require_fixed_frame_capacity(&self) -> Result<(), Spec::Error> {
        if self.frames.capacity() != self.frame_capacity {
            return Err(Spec::invalid("resumable split frame capacity changed"));
        }
        Ok(())
    }

    fn record_scratch(&self, receipt: &mut SequenceMutationReceipt) {
        let frame_bytes = self.frames.capacity() * std::mem::size_of::<ResumableSplitFrame>();
        receipt.maximum_resumable_split_frames = receipt
            .maximum_resumable_split_frames
            .max(self.frames.len());
        receipt.maximum_resumable_split_frame_requested_bytes = receipt
            .maximum_resumable_split_frame_requested_bytes
            .max(self.frame_requested_bytes);
        receipt.maximum_resumable_split_frame_bytes =
            receipt.maximum_resumable_split_frame_bytes.max(frame_bytes);
        let (total_requested, total_bytes) = self.scratch_totals();
        receipt.maximum_resumable_split_total_requested_bytes = receipt
            .maximum_resumable_split_total_requested_bytes
            .max(total_requested);
        receipt.maximum_resumable_split_total_scratch_bytes = receipt
            .maximum_resumable_split_total_scratch_bytes
            .max(total_bytes);
    }

    fn scratch_totals(&self) -> (usize, usize) {
        let frame_bytes = self.frames.capacity() * std::mem::size_of::<ResumableSplitFrame>();
        let join_requested = self.join.task_requested_bytes + self.join.value_requested_bytes;
        let join_bytes = self.join.tasks.capacity() * std::mem::size_of::<ResumableJoinTask>()
            + self.join.values.capacity() * std::mem::size_of::<ArenaBuildOwner>();
        (
            self.frame_requested_bytes + join_requested,
            frame_bytes + join_bytes,
        )
    }
}

#[derive(Debug)]
enum ResumableRetainedRangePhase<Spec> {
    Empty,
    First(ResumableSequenceSplit<Spec>),
    FirstReady(ResumableSequenceSplit<Spec>),
    Second(ResumableSequenceSplit<Spec>),
    SecondReady(ResumableSequenceSplit<Spec>),
    Complete(Option<ArenaBuildOwner>),
    Taken,
    Failed,
}

/// Retains one immutable base range as a build-owned persistent-sequence root.
///
/// This is the minimal composition needed by a later checkpoint splice: split
/// at `range.start`, split the remaining tail at `range.len()`, and release the
/// discarded sides through the same build journal. It never enumerates the
/// retained leaves and preserves an aligned subtree's exact arena identity.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResumableSequenceRetainedRange<Spec> {
    build: crate::ArenaBuildId,
    range_len: u64,
    phase: ResumableRetainedRangePhase<Spec>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<Spec> ResumableSequenceRetainedRange<Spec>
where
    Spec: SequenceSpec,
    Spec::Error: From<ArenaBuildError>,
{
    pub(crate) fn try_new(
        ticket: &crate::ArenaBuildTicket,
        arena: &PageArena,
        base_root: ArenaId,
        range: Range<u64>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        arena.build_lifecycle(ticket.id())?;
        let leaves = Spec::leaves(validated_split_node::<Spec>(arena, base_root)?.0);
        if range.start > range.end || range.end > leaves {
            return Err(Spec::invalid("retained sequence range is out of bounds"));
        }
        let range_len = range.end - range.start;
        let phase = if range_len == 0 {
            ResumableRetainedRangePhase::Empty
        } else {
            ResumableRetainedRangePhase::First(ResumableSequenceSplit::<Spec>::try_new(
                ticket,
                arena,
                base_root,
                range.start,
                receipt,
            )?)
        };
        Ok(Self {
            build: ticket.id(),
            range_len,
            phase,
        })
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> crate::ArenaBuildId {
        self.build
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceSplitProgress, Spec::Error> {
        if session.id() != self.build {
            return Err(Spec::invalid(
                "resumable sequence job belongs to another build generation",
            ));
        }
        session.live_owners()?;
        let phase = std::mem::replace(&mut self.phase, ResumableRetainedRangePhase::Failed);
        let result = self.poll_phase(session, receipt, phase);
        match result {
            Ok((phase, progress)) => {
                self.phase = phase;
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    fn poll_phase(
        &self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        phase: ResumableRetainedRangePhase<Spec>,
    ) -> Result<
        (
            ResumableRetainedRangePhase<Spec>,
            ResumableSequenceSplitProgress,
        ),
        Spec::Error,
    > {
        match phase {
            ResumableRetainedRangePhase::Empty => Ok((
                ResumableRetainedRangePhase::Complete(None),
                ResumableSequenceSplitProgress::Complete,
            )),
            ResumableRetainedRangePhase::First(mut split) => {
                let progress = split.poll(session, receipt)?;
                let phase = match progress {
                    ResumableSequenceSplitProgress::Pending => {
                        ResumableRetainedRangePhase::First(split)
                    }
                    ResumableSequenceSplitProgress::Complete => {
                        ResumableRetainedRangePhase::FirstReady(split)
                    }
                };
                Ok((phase, ResumableSequenceSplitProgress::Pending))
            }
            ResumableRetainedRangePhase::FirstReady(mut split) => {
                let (prefix, tail) = split.take_parts()?;
                if let Some(prefix) = prefix {
                    session.release(prefix)?;
                }
                let tail = tail.ok_or_else(|| {
                    Spec::invalid("nonempty retained range has no first-split tail")
                })?;
                // The tail cannot exceed the original root's height. Reuse
                // both preflighted path and join buffers rather than briefly
                // overlapping two splits or reserving inside a poll.
                split.restart_from_owned(session, tail, self.range_len, receipt)?;
                Ok((
                    ResumableRetainedRangePhase::Second(split),
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
            ResumableRetainedRangePhase::Second(mut split) => {
                let progress = split.poll(session, receipt)?;
                let phase = match progress {
                    ResumableSequenceSplitProgress::Pending => {
                        ResumableRetainedRangePhase::Second(split)
                    }
                    ResumableSequenceSplitProgress::Complete => {
                        ResumableRetainedRangePhase::SecondReady(split)
                    }
                };
                Ok((phase, ResumableSequenceSplitProgress::Pending))
            }
            ResumableRetainedRangePhase::SecondReady(mut split) => {
                let (middle, suffix) = split.take_parts()?;
                drop(split);
                if let Some(suffix) = suffix {
                    session.release(suffix)?;
                }
                if middle.is_none() {
                    return Err(Spec::invalid("nonempty retained range has no root"));
                }
                Ok((
                    ResumableRetainedRangePhase::Complete(middle),
                    ResumableSequenceSplitProgress::Complete,
                ))
            }
            complete @ ResumableRetainedRangePhase::Complete(_) => {
                Ok((complete, ResumableSequenceSplitProgress::Complete))
            }
            ResumableRetainedRangePhase::Taken => Err(Spec::invalid(
                "retained sequence range output was already taken",
            )),
            ResumableRetainedRangePhase::Failed => {
                Err(Spec::invalid("retained sequence range is poisoned"))
            }
        }
    }

    pub(crate) fn take_root(&mut self) -> Result<Option<ArenaBuildOwner>, Spec::Error> {
        let phase = std::mem::replace(&mut self.phase, ResumableRetainedRangePhase::Taken);
        match phase {
            ResumableRetainedRangePhase::Complete(root) => Ok(root),
            other => {
                self.phase = other;
                Err(Spec::invalid("retained sequence range is incomplete"))
            }
        }
    }
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
enum ResumableSplicePhase {
    Direct {
        output: Option<ArenaBuildOwner>,
    },
    DirectReplace {
        deleted: ArenaBuildOwner,
        output: Option<ArenaBuildOwner>,
    },
    First,
    FirstReady,
    Second {
        prefix: Option<ArenaBuildOwner>,
    },
    SecondReady {
        prefix: Option<ArenaBuildOwner>,
    },
    Assemble {
        prefix: Option<ArenaBuildOwner>,
        deleted: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    },
    JoinPrefix {
        suffix: Option<ArenaBuildOwner>,
    },
    PrefixReady {
        working: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    },
    JoinSuffix,
    Complete(Option<ArenaBuildOwner>),
    Taken,
    Failed,
}

#[derive(Debug)]
enum ResumableOwnedSpliceStart {
    Direct {
        output: Option<ArenaBuildOwner>,
    },
    DirectReplace {
        deleted: ArenaBuildOwner,
        output: Option<ArenaBuildOwner>,
    },
    Split {
        working_root: ArenaBuildOwner,
        leaf_index: u64,
        replacement: Option<ArenaBuildOwner>,
        split_height: u16,
        join_maximum_height: u16,
    },
}

#[derive(Debug)]
struct ResumableOwnedSplicePlan {
    deleted_count: u64,
    replacement_leaves: usize,
    reused_leaves: usize,
    start: ResumableOwnedSpliceStart,
}

/// One journal-owned persistent-sequence splice.
///
/// The constructor consumes a build-owned working root and optional
/// build-owned replacement root. The job cuts the deleted range with the
/// reusable split machine above, transfers the deleted subtree into the
/// arena's iterative release queue without walking it, and reassembles at
/// most `prefix + replacement + suffix` through the same preflighted join
/// interpreter. One poll allocates at most one branch and performs no
/// split-local heap reservation.
///
/// This raw mechanism must remain behind a typed green working-root
/// capability: its bounded path checks do not recursively authenticate an
/// arbitrary [`ArenaBuildOwner`]. Any constructor or poll error consumes
/// linear handles into a poisoned build; callers must abort that whole build.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResumableSequenceSplice<Spec: SequenceSpec> {
    build: crate::ArenaBuildId,
    deleted_count: u64,
    deleted_summary_guard: Option<SequenceDeletedSummaryGuard<Spec::Summary>>,
    replacement: Option<ArenaBuildOwner>,
    replacement_leaves: usize,
    reused_leaves: usize,
    completion_recorded: bool,
    split: Option<ResumableSequenceSplit<Spec>>,
    phase: ResumableSplicePhase,
}

/// Optional caller-owned semantic guard for the subtree removed by a splice.
///
/// The persistent sequence knows how to isolate and release the deleted
/// subtree, but only its typed caller knows which summary fields constitute an
/// identity proof. The guard is therefore a value plus a non-capturing typed
/// comparator. It is checked exactly once, after the actual deleted subtree is
/// isolated and before that subtree is transferred to the release queue.
/// Existing mechanism callers remain unguarded unless they opt in explicitly.
#[derive(Clone, Copy)]
pub(crate) struct SequenceDeletedSummaryGuard<Summary> {
    expected: Option<Summary>,
    equivalent: fn(Option<Summary>, Option<Summary>) -> bool,
}

impl<Summary> fmt::Debug for SequenceDeletedSummaryGuard<Summary> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceDeletedSummaryGuard")
            .field("expected", &self.expected.is_some())
            .finish_non_exhaustive()
    }
}

impl<Summary> SequenceDeletedSummaryGuard<Summary> {
    pub(crate) const fn new(
        expected: Option<Summary>,
        equivalent: fn(Option<Summary>, Option<Summary>) -> bool,
    ) -> Self {
        Self {
            expected,
            equivalent,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl<Spec> ResumableSequenceSplice<Spec>
where
    Spec: SequenceSpec,
    Spec::Error: From<ArenaBuildError>,
{
    pub(crate) fn try_from_owned(
        session: &ArenaBuildSession<'_>,
        working_root: Option<ArenaBuildOwner>,
        range: Range<u64>,
        replacement: Option<ArenaBuildOwner>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        let plan = Self::plan_owned(session, working_root, range, replacement)?;
        let split = match &plan.start {
            ResumableOwnedSpliceStart::Split {
                split_height,
                join_maximum_height,
                ..
            } => Some(ResumableSequenceSplit::<Spec>::try_preallocated(
                session.id(),
                *split_height,
                *join_maximum_height,
                receipt,
            )?),
            ResumableOwnedSpliceStart::Direct { .. }
            | ResumableOwnedSpliceStart::DirectReplace { .. } => None,
        };
        let mut splice = Self {
            build: session.id(),
            deleted_count: 0,
            deleted_summary_guard: None,
            replacement: None,
            replacement_leaves: 0,
            reused_leaves: 0,
            completion_recorded: false,
            split,
            phase: ResumableSplicePhase::Taken,
        };
        splice.install_plan(session, plan, receipt)?;
        Ok(splice)
    }

    /// Fallibly reserves one maximum-domain split/join interpreter before a
    /// repeated-edit loop begins. [`Self::begin_from_owned`] then installs
    /// each build-local splice without reserving heap storage from a poll.
    pub(crate) fn try_preallocated_for_build(
        build: crate::ArenaBuildId,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        let maximum_height = maximum_avl_sequence_height(u64::MAX);
        let join_maximum_height = maximum_height
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("splice join height overflow"))?;
        Ok(Self {
            build,
            deleted_count: 0,
            deleted_summary_guard: None,
            replacement: None,
            replacement_leaves: 0,
            reused_leaves: 0,
            completion_recorded: false,
            split: Some(ResumableSequenceSplit::<Spec>::try_preallocated(
                build,
                maximum_height,
                join_maximum_height,
                receipt,
            )?),
            phase: ResumableSplicePhase::Taken,
        })
    }

    pub(crate) fn begin_from_owned(
        &mut self,
        session: &ArenaBuildSession<'_>,
        working_root: Option<ArenaBuildOwner>,
        range: Range<u64>,
        replacement: Option<ArenaBuildOwner>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.ensure_session(session)?;
        if !matches!(self.phase, ResumableSplicePhase::Taken) || self.replacement.is_some() {
            return Err(Spec::invalid("resumable sequence splice is still active"));
        }
        let plan = Self::plan_owned(session, working_root, range, replacement)?;
        self.deleted_summary_guard = None;
        self.install_plan(session, plan, receipt)
    }

    pub(crate) fn begin_from_owned_validating_deleted(
        &mut self,
        session: &ArenaBuildSession<'_>,
        working_root: Option<ArenaBuildOwner>,
        range: Range<u64>,
        replacement: Option<ArenaBuildOwner>,
        deleted_summary_guard: SequenceDeletedSummaryGuard<Spec::Summary>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.ensure_session(session)?;
        if !matches!(self.phase, ResumableSplicePhase::Taken) || self.replacement.is_some() {
            return Err(Spec::invalid("resumable sequence splice is still active"));
        }
        let plan = Self::plan_owned(session, working_root, range, replacement)?;
        self.deleted_summary_guard = Some(deleted_summary_guard);
        self.install_plan(session, plan, receipt)
    }

    fn plan_owned(
        session: &ArenaBuildSession<'_>,
        working_root: Option<ArenaBuildOwner>,
        range: Range<u64>,
        replacement: Option<ArenaBuildOwner>,
    ) -> Result<ResumableOwnedSplicePlan, Spec::Error> {
        session.live_owners()?;
        let replacement_summary = replacement
            .as_ref()
            .map(|root| {
                validated_split_node::<Spec>(session.arena(), session.owner_id(root)?)
                    .map(|value| value.0)
            })
            .transpose()?;
        let replacement_leaves = replacement_summary
            .map(Spec::leaves)
            .map(|leaves| {
                usize::try_from(leaves)
                    .map_err(|_| Spec::invalid("replacement leaf count exceeds usize"))
            })
            .transpose()?
            .unwrap_or(0);
        let replacement_height = replacement_summary.map_or(0, Spec::height);

        let Some(working_root) = working_root else {
            if range.start != 0 || range.end != 0 {
                return Err(Spec::invalid("empty sequence splice is out of range"));
            }
            return Ok(ResumableOwnedSplicePlan {
                deleted_count: 0,
                replacement_leaves,
                reused_leaves: 0,
                start: ResumableOwnedSpliceStart::Direct {
                    output: replacement,
                },
            });
        };

        let root_summary =
            validated_split_node::<Spec>(session.arena(), session.owner_id(&working_root)?)?.0;
        let old_leaves = Spec::leaves(root_summary);
        if range.start > range.end || range.end > old_leaves {
            return Err(Spec::invalid("owned sequence splice is out of range"));
        }
        let deleted_count = range.end - range.start;
        let reused_leaves = usize::try_from(old_leaves - deleted_count)
            .map_err(|_| Spec::invalid("reused leaf count exceeds usize"))?;
        let start = if range.is_empty() && replacement.is_none() {
            ResumableOwnedSpliceStart::Direct {
                output: Some(working_root),
            }
        } else if range.start == 0 && range.end == old_leaves {
            ResumableOwnedSpliceStart::DirectReplace {
                deleted: working_root,
                output: replacement,
            }
        } else {
            let join_maximum_height = Spec::height(root_summary)
                .max(replacement_height)
                .checked_add(1)
                .ok_or_else(|| Spec::invalid("splice join height overflow"))?;
            ResumableOwnedSpliceStart::Split {
                working_root,
                leaf_index: range.start,
                replacement,
                split_height: Spec::height(root_summary),
                join_maximum_height,
            }
        };
        Ok(ResumableOwnedSplicePlan {
            deleted_count,
            replacement_leaves,
            reused_leaves,
            start,
        })
    }

    fn install_plan(
        &mut self,
        session: &ArenaBuildSession<'_>,
        plan: ResumableOwnedSplicePlan,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.deleted_count = plan.deleted_count;
        self.replacement_leaves = plan.replacement_leaves;
        self.reused_leaves = plan.reused_leaves;
        self.completion_recorded = false;
        match plan.start {
            ResumableOwnedSpliceStart::Direct { output } => {
                self.replacement = None;
                self.phase = ResumableSplicePhase::Direct { output };
            }
            ResumableOwnedSpliceStart::DirectReplace { deleted, output } => {
                self.replacement = None;
                self.phase = ResumableSplicePhase::DirectReplace { deleted, output };
            }
            ResumableOwnedSpliceStart::Split {
                working_root,
                leaf_index,
                replacement,
                split_height,
                join_maximum_height,
            } => {
                let split = self
                    .split
                    .as_mut()
                    .ok_or_else(|| Spec::invalid("resumable splice has no preflighted split"))?;
                if usize::from(split_height.saturating_sub(1)) > split.frame_slot_limit
                    || usize::from(join_maximum_height).saturating_add(4)
                        > split.join.task_slot_limit
                {
                    return Err(Spec::invalid(
                        "resumable splice exceeds preflighted scratch",
                    ));
                }
                split.restart_from_owned(session, working_root, leaf_index, receipt)?;
                self.replacement = replacement;
                self.phase = ResumableSplicePhase::First;
            }
        }
        self.record_scratch(receipt);
        Ok(())
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> crate::ArenaBuildId {
        self.build
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceSplitProgress, Spec::Error> {
        self.ensure_session(session)?;
        receipt.resumable_splice_polls = receipt
            .resumable_splice_polls
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("resumable splice poll count overflow"))?;
        let phase = std::mem::replace(&mut self.phase, ResumableSplicePhase::Failed);
        let result = self.poll_phase(session, receipt, phase);
        match result {
            Ok((phase, progress)) => {
                self.phase = phase;
                self.record_scratch(receipt);
                if progress == ResumableSequenceSplitProgress::Complete {
                    self.record_completion(receipt)?;
                }
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    fn poll_phase(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        phase: ResumableSplicePhase,
    ) -> Result<(ResumableSplicePhase, ResumableSequenceSplitProgress), Spec::Error> {
        match phase {
            ResumableSplicePhase::Direct { output } => {
                self.validate_deleted_summary(session, None)?;
                Ok((
                    ResumableSplicePhase::Complete(output),
                    ResumableSequenceSplitProgress::Complete,
                ))
            }
            ResumableSplicePhase::DirectReplace { deleted, output } => {
                self.validate_deleted_summary(session, Some(&deleted))?;
                session.release(deleted)?;
                Self::record_deleted_root(receipt)?;
                Ok((
                    ResumableSplicePhase::Complete(output),
                    ResumableSequenceSplitProgress::Complete,
                ))
            }
            ResumableSplicePhase::First => self.poll_first(session, receipt),
            ResumableSplicePhase::FirstReady => self.finish_first_split(session, receipt),
            ResumableSplicePhase::Second { prefix } => self.poll_second(session, receipt, prefix),
            ResumableSplicePhase::SecondReady { prefix } => {
                let (deleted, suffix) = self.split_mut()?.take_parts()?;
                Ok((
                    ResumableSplicePhase::Assemble {
                        prefix,
                        deleted,
                        suffix,
                    },
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
            ResumableSplicePhase::Assemble {
                prefix,
                deleted,
                suffix,
            } => self.begin_assembly(session, receipt, prefix, deleted, suffix),
            ResumableSplicePhase::JoinPrefix { suffix } => {
                if self.split_mut()?.join.poll(session, receipt)?
                    == ResumableSequenceProgress::Pending
                {
                    Ok((
                        ResumableSplicePhase::JoinPrefix { suffix },
                        ResumableSequenceSplitProgress::Pending,
                    ))
                } else {
                    let working = self.split_mut()?.join.take_root()?;
                    Ok((
                        ResumableSplicePhase::PrefixReady {
                            working: Some(working),
                            suffix,
                        },
                        ResumableSequenceSplitProgress::Pending,
                    ))
                }
            }
            ResumableSplicePhase::PrefixReady { working, suffix } => {
                self.begin_suffix_join(session, receipt, working, suffix)
            }
            ResumableSplicePhase::JoinSuffix => {
                if self.split_mut()?.join.poll(session, receipt)?
                    == ResumableSequenceProgress::Pending
                {
                    Ok((
                        ResumableSplicePhase::JoinSuffix,
                        ResumableSequenceSplitProgress::Pending,
                    ))
                } else {
                    let root = self.split_mut()?.join.take_root()?;
                    Ok((
                        ResumableSplicePhase::Complete(Some(root)),
                        ResumableSequenceSplitProgress::Complete,
                    ))
                }
            }
            complete @ ResumableSplicePhase::Complete(_) => {
                Ok((complete, ResumableSequenceSplitProgress::Complete))
            }
            ResumableSplicePhase::Taken => {
                Err(Spec::invalid("resumable sequence splice output was taken"))
            }
            ResumableSplicePhase::Failed => {
                Err(Spec::invalid("resumable sequence splice is poisoned"))
            }
        }
    }

    fn poll_first(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(ResumableSplicePhase, ResumableSequenceSplitProgress), Spec::Error> {
        let phase = match self.split_mut()?.poll(session, receipt)? {
            ResumableSequenceSplitProgress::Pending => ResumableSplicePhase::First,
            ResumableSequenceSplitProgress::Complete => ResumableSplicePhase::FirstReady,
        };
        Ok((phase, ResumableSequenceSplitProgress::Pending))
    }

    fn finish_first_split(
        &mut self,
        session: &ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(ResumableSplicePhase, ResumableSequenceSplitProgress), Spec::Error> {
        let (prefix, tail) = self.split_mut()?.take_parts()?;
        let Some(tail) = tail else {
            if self.deleted_count != 0 {
                return Err(Spec::invalid("nonempty deleted range has no split tail"));
            }
            return Ok((
                ResumableSplicePhase::Assemble {
                    prefix,
                    deleted: None,
                    suffix: None,
                },
                ResumableSequenceSplitProgress::Pending,
            ));
        };
        let deleted_count = self.deleted_count;
        self.split_mut()?
            .restart_from_owned(session, tail, deleted_count, receipt)?;
        Ok((
            ResumableSplicePhase::Second { prefix },
            ResumableSequenceSplitProgress::Pending,
        ))
    }

    fn poll_second(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        prefix: Option<ArenaBuildOwner>,
    ) -> Result<(ResumableSplicePhase, ResumableSequenceSplitProgress), Spec::Error> {
        let phase = match self.split_mut()?.poll(session, receipt)? {
            ResumableSequenceSplitProgress::Pending => ResumableSplicePhase::Second { prefix },
            ResumableSequenceSplitProgress::Complete => {
                ResumableSplicePhase::SecondReady { prefix }
            }
        };
        Ok((phase, ResumableSequenceSplitProgress::Pending))
    }

    fn begin_assembly(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        prefix: Option<ArenaBuildOwner>,
        deleted: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    ) -> Result<(ResumableSplicePhase, ResumableSequenceSplitProgress), Spec::Error> {
        self.validate_deleted_summary(session, deleted.as_ref())?;
        if let Some(deleted) = deleted {
            session.release(deleted)?;
            Self::record_deleted_root(receipt)?;
        }
        let replacement = self.replacement.take();
        match (prefix, replacement) {
            (Some(left), Some(right)) => {
                self.split_mut()?
                    .join
                    .begin(session, left, right, receipt)?;
                Ok((
                    ResumableSplicePhase::JoinPrefix { suffix },
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
            (working, None) | (None, working) => Ok((
                ResumableSplicePhase::PrefixReady { working, suffix },
                ResumableSequenceSplitProgress::Pending,
            )),
        }
    }

    fn begin_suffix_join(
        &mut self,
        session: &ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        working: Option<ArenaBuildOwner>,
        suffix: Option<ArenaBuildOwner>,
    ) -> Result<(ResumableSplicePhase, ResumableSequenceSplitProgress), Spec::Error> {
        match (working, suffix) {
            (Some(left), Some(right)) => {
                self.split_mut()?
                    .join
                    .begin(session, left, right, receipt)?;
                Ok((
                    ResumableSplicePhase::JoinSuffix,
                    ResumableSequenceSplitProgress::Pending,
                ))
            }
            (output, None) | (None, output) => Ok((
                ResumableSplicePhase::Complete(output),
                ResumableSequenceSplitProgress::Complete,
            )),
        }
    }

    pub(crate) fn take_root(&mut self) -> Result<Option<ArenaBuildOwner>, Spec::Error> {
        let phase = std::mem::replace(&mut self.phase, ResumableSplicePhase::Taken);
        match phase {
            ResumableSplicePhase::Complete(root) => Ok(root),
            other => {
                self.phase = other;
                Err(Spec::invalid("resumable sequence splice is incomplete"))
            }
        }
    }

    fn split_mut(&mut self) -> Result<&mut ResumableSequenceSplit<Spec>, Spec::Error> {
        self.split
            .as_mut()
            .ok_or_else(|| Spec::invalid("resumable sequence splice has no split machine"))
    }

    fn ensure_session(&self, session: &ArenaBuildSession<'_>) -> Result<(), Spec::Error> {
        if session.id() != self.build {
            return Err(Spec::invalid(
                "resumable sequence splice belongs to another build generation",
            ));
        }
        session.live_owners()?;
        Ok(())
    }

    fn validate_deleted_summary(
        &mut self,
        session: &ArenaBuildSession<'_>,
        deleted: Option<&ArenaBuildOwner>,
    ) -> Result<(), Spec::Error> {
        let Some(guard) = self.deleted_summary_guard.take() else {
            return Ok(());
        };
        let actual = deleted
            .map(|root| {
                validated_split_node::<Spec>(session.arena(), session.owner_id(root)?)
                    .map(|value| value.0)
            })
            .transpose()?;
        if !(guard.equivalent)(actual, guard.expected) {
            return Err(Spec::invalid("deleted sequence summary mismatch"));
        }
        Ok(())
    }

    fn record_scratch(&self, receipt: &mut SequenceMutationReceipt) {
        if let Some(split) = &self.split {
            let (total_requested, total_bytes) = split.scratch_totals();
            receipt.maximum_resumable_splice_total_requested_bytes = receipt
                .maximum_resumable_splice_total_requested_bytes
                .max(total_requested);
            receipt.maximum_resumable_splice_total_scratch_bytes = receipt
                .maximum_resumable_splice_total_scratch_bytes
                .max(total_bytes);
        }
    }

    fn record_completion(
        &mut self,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        if self.completion_recorded {
            return Ok(());
        }
        receipt.leaves_adopted = receipt
            .leaves_adopted
            .checked_add(self.replacement_leaves)
            .ok_or_else(|| Spec::invalid("splice adopted leaf count overflow"))?;
        receipt.leaves_reused = receipt
            .leaves_reused
            .checked_add(self.reused_leaves)
            .ok_or_else(|| Spec::invalid("splice reused leaf count overflow"))?;
        self.completion_recorded = true;
        Ok(())
    }

    fn record_deleted_root(receipt: &mut SequenceMutationReceipt) -> Result<(), Spec::Error> {
        receipt.resumable_splice_deleted_roots_released = receipt
            .resumable_splice_deleted_roots_released
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("splice deleted-root count overflow"))?;
        Ok(())
    }
}

#[derive(Debug)]
enum ResumableJoinTask {
    Join {
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    },
    Balance {
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    },
    BalanceWithLeft {
        left: ArenaBuildOwner,
    },
    BalanceWithRight {
        right: ArenaBuildOwner,
    },
    MakeBranch {
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    },
    MakeBranchWithLeft {
        left: ArenaBuildOwner,
    },
    MakeBranchWithRight {
        right: ArenaBuildOwner,
    },
    MakeBranchFromValues,
}

impl<Spec> ResumableStreamingSequenceBuilder<Spec>
where
    Spec: SequenceSpec,
    Spec::Error: From<ArenaBuildError>,
{
    pub(crate) fn try_new(receipt: &mut SequenceMutationReceipt) -> Result<Self, Spec::Error> {
        let bin_slot_limit = MAX_RESUMABLE_SEQUENCE_BIN_SLOTS;
        let bin_requested_bytes = bin_slot_limit
            .checked_mul(std::mem::size_of::<Option<ArenaBuildOwner>>())
            .ok_or_else(|| Spec::invalid("resumable bin bytes overflow"))?;
        let mut bins = Vec::new();
        bins.try_reserve_exact(bin_slot_limit)
            .map_err(|_| Spec::invalid("resumable bin reservation failed"))?;
        let bin_capacity = bins.capacity();
        // Preflight one maximum-domain join interpreter and reuse it for
        // every tail reduction. Repeated checkpoint reductions therefore do
        // not reserve task/value vectors from a poll.
        let join = ResumableSequenceJoin::<Spec>::try_preallocated(
            maximum_avl_sequence_height(u64::MAX),
            receipt,
        )?;
        let builder = Self {
            bins,
            bin_slot_limit,
            bin_capacity,
            bin_requested_bytes,
            carry: None,
            reduction: None,
            join,
            marker: PhantomData,
        };
        builder.record_scratch(receipt);
        Ok(builder)
    }

    pub(crate) fn begin_push(
        &mut self,
        session: &ArenaBuildSession<'_>,
        leaf: ArenaBuildOwner,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.require_fixed_bin_capacity()?;
        if self.carry.is_some() || self.reduction.is_some() {
            return Err(Spec::invalid(
                "resumable sequence operation is already active",
            ));
        }
        let leaf_id = session.owner_id(&leaf)?;
        if sequence_node::<Spec>(session.arena(), leaf_id)?.1 != SequenceNodeKind::Leaf {
            return Err(Spec::invalid("streaming sequence input is not a leaf"));
        }
        receipt.leaves_adopted = receipt
            .leaves_adopted
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("streaming leaf count overflow"))?;
        self.carry = Some((leaf, 0));
        self.record_scratch(receipt);
        Ok(())
    }

    pub(crate) fn poll_push(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceProgress, Spec::Error> {
        self.require_fixed_bin_capacity()?;
        if self.reduction.is_some() {
            return Err(Spec::invalid("sequence finalization already started"));
        }
        let Some((carry, level)) = self.carry.take() else {
            return Ok(ResumableSequenceProgress::Complete);
        };
        if level == self.bins.len() {
            self.push_bin(carry)?;
            self.require_fixed_bin_capacity()?;
            self.record_scratch(receipt);
            return Ok(ResumableSequenceProgress::Complete);
        }
        let Some(left) = self.bins[level].take() else {
            self.bins[level] = Some(carry);
            self.record_scratch(receipt);
            return Ok(ResumableSequenceProgress::Complete);
        };
        let branch = make_branch::<Spec, _>(session, left, carry, receipt)?;
        self.carry = Some((branch, level + 1));
        self.require_fixed_bin_capacity()?;
        self.record_scratch(receipt);
        Ok(ResumableSequenceProgress::Pending)
    }

    pub(crate) fn begin_finish(
        &mut self,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.require_fixed_bin_capacity()?;
        if self.carry.is_some() {
            return Err(Spec::invalid("sequence push is still pending"));
        }
        if self.reduction.is_some() {
            return Err(Spec::invalid("sequence finalization already started"));
        }
        // There are at most one bin per bit in the u64 leaf count, so this
        // reverse/compaction is an explicitly bounded O(log leaves) kernel.
        // Bin index is subtree size: reversing yields exact source order, the
        // largest/oldest complete prefix first and the newest tail last.
        self.bins.reverse();
        self.bins.retain(Option::is_some);
        if self.bins.is_empty() {
            return Err(Spec::invalid("empty resumable sequence"));
        }
        let root = self.bins[0]
            .take()
            .ok_or_else(|| Spec::invalid("missing first reduction root"))?;
        self.reduction = Some(SequenceReduction {
            next_bin: 1,
            root: Some(root),
            joining: false,
            marker: PhantomData,
        });
        self.require_fixed_bin_capacity()?;
        self.record_scratch(receipt);
        Ok(())
    }

    pub(crate) fn poll_finish(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceProgress, Spec::Error> {
        self.require_fixed_bin_capacity()?;
        let joining = self
            .reduction
            .as_ref()
            .ok_or_else(|| Spec::invalid("sequence finalization has not started"))?
            .joining;
        if joining {
            match self.join.poll(session, receipt)? {
                ResumableSequenceProgress::Pending => {}
                ResumableSequenceProgress::Complete => {
                    let root = self.join.take_root()?;
                    let reduction = self
                        .reduction
                        .as_mut()
                        .ok_or_else(|| Spec::invalid("sequence reduction disappeared"))?;
                    reduction.root = Some(root);
                    reduction.joining = false;
                }
            }
            self.record_scratch(receipt);
            return Ok(ResumableSequenceProgress::Pending);
        }
        let reduction = self
            .reduction
            .as_mut()
            .ok_or_else(|| Spec::invalid("sequence finalization has not started"))?;
        if reduction.next_bin == self.bins.len() {
            return Ok(ResumableSequenceProgress::Complete);
        }
        let left = reduction
            .root
            .take()
            .ok_or_else(|| Spec::invalid("sequence reduction lost its working root"))?;
        let right = self.bins[reduction.next_bin]
            .take()
            .ok_or_else(|| Spec::invalid("sequence reduction lost its next root"))?;
        reduction.next_bin += 1;
        self.join.begin(session, left, right, receipt)?;
        reduction.joining = true;
        self.record_scratch(receipt);
        Ok(ResumableSequenceProgress::Pending)
    }

    pub(crate) fn take_root(&mut self) -> Result<ArenaBuildOwner, Spec::Error> {
        let reduction = self
            .reduction
            .as_ref()
            .ok_or_else(|| Spec::invalid("sequence finalization has not started"))?;
        if reduction.next_bin != self.bins.len() || reduction.joining {
            return Err(Spec::invalid("sequence reduction is incomplete"));
        }
        let root = self
            .reduction
            .take()
            .and_then(|reduction| reduction.root)
            .ok_or_else(|| Spec::invalid("sequence reduction lost its root"))?;
        // `begin_finish` reverses and compacts the bins. Clearing their
        // logical length after ownership transfers makes this same
        // preflighted builder an unambiguously empty source-ordered tail.
        self.bins.clear();
        self.require_fixed_bin_capacity()?;
        Ok(root)
    }

    fn record_scratch(&self, receipt: &mut SequenceMutationReceipt) {
        let live_bins = self.bins.iter().filter(|root| root.is_some()).count()
            + usize::from(self.carry.is_some());
        receipt.maximum_streaming_roots = receipt.maximum_streaming_roots.max(live_bins);
        receipt.maximum_streaming_bin_slots = receipt
            .maximum_streaming_bin_slots
            .max(self.bins.capacity() + usize::from(self.carry.is_some()));
        receipt.maximum_resumable_bin_logical_slots = receipt
            .maximum_resumable_bin_logical_slots
            .max(self.bin_slot_limit);
        receipt.maximum_resumable_bin_requested_bytes = receipt
            .maximum_resumable_bin_requested_bytes
            .max(self.bin_requested_bytes);
        receipt.maximum_streaming_bin_bytes = receipt.maximum_streaming_bin_bytes.max(
            self.bins.capacity() * std::mem::size_of::<Option<ArenaBuildOwner>>()
                + usize::from(self.carry.is_some())
                    * std::mem::size_of::<(ArenaBuildOwner, usize)>(),
        );
        self.join.record_scratch(receipt);
    }

    fn push_bin(&mut self, root: ArenaBuildOwner) -> Result<(), Spec::Error> {
        if self.bins.len() >= self.bin_slot_limit {
            return Err(Spec::invalid(
                "resumable sequence exceeded logical bin bound",
            ));
        }
        self.bins.push(Some(root));
        Ok(())
    }

    fn require_fixed_bin_capacity(&self) -> Result<(), Spec::Error> {
        if self.bins.capacity() != self.bin_capacity {
            return Err(Spec::invalid("resumable bin capacity changed"));
        }
        Ok(())
    }
}

impl<Spec> ResumableSequenceJoin<Spec>
where
    Spec: SequenceSpec,
    Spec::Error: From<ArenaBuildError>,
{
    /// Preflights one reusable explicit-interpreter scratch allocation. The
    /// split job reserves this once from the validated base-root height and
    /// calls [`Self::begin`] for every unwind join without further heap work.
    fn try_preallocated(
        maximum_height: u16,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        // A recursive Join would retain at most one Balance continuation per
        // level. One AVL rotation can temporarily replace its top frame with
        // three MakeBranch tasks, hence height + 4 is conservative.
        let task_slots = usize::from(maximum_height)
            .checked_add(4)
            .ok_or_else(|| Spec::invalid("resumable join task bound overflow"))?;
        let value_slots = 2_usize;
        let task_requested_bytes = task_slots
            .checked_mul(std::mem::size_of::<ResumableJoinTask>())
            .ok_or_else(|| Spec::invalid("resumable join task bytes overflow"))?;
        let value_requested_bytes = value_slots
            .checked_mul(std::mem::size_of::<ArenaBuildOwner>())
            .ok_or_else(|| Spec::invalid("resumable join value bytes overflow"))?;
        let mut tasks = Vec::new();
        tasks
            .try_reserve_exact(task_slots)
            .map_err(|_| Spec::invalid("resumable join task reservation failed"))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(value_slots)
            .map_err(|_| Spec::invalid("resumable join value reservation failed"))?;
        receipt.resumable_join_scratch_reservations = receipt
            .resumable_join_scratch_reservations
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("resumable join reservation count overflow"))?;
        let task_capacity = tasks.capacity();
        let value_capacity = values.capacity();
        let join = Self {
            tasks,
            values,
            task_requested_bytes,
            task_slot_limit: task_slots,
            task_capacity,
            value_requested_bytes,
            value_slot_limit: value_slots,
            value_capacity,
            marker: PhantomData,
        };
        join.record_scratch(receipt);
        Ok(join)
    }

    fn begin(
        &mut self,
        session: &ArenaBuildSession<'_>,
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<(), Spec::Error> {
        self.require_fixed_capacity()?;
        if !self.tasks.is_empty() || !self.values.is_empty() {
            return Err(Spec::invalid("resumable join is already active"));
        }
        let required_height =
            owner_height::<Spec>(session, &left)?.max(owner_height::<Spec>(session, &right)?);
        let required_slots = usize::from(required_height)
            .checked_add(4)
            .ok_or_else(|| Spec::invalid("resumable join task bound overflow"))?;
        if required_slots > self.task_slot_limit {
            return Err(Spec::invalid(
                "resumable join exceeds preflighted task bound",
            ));
        }
        self.push_task(ResumableJoinTask::Join { left, right })?;
        self.record_scratch(receipt);
        Ok(())
    }

    /// Executes exactly one explicit join task. A task performs only bounded
    /// node inspection/owner transfers or one arena allocation, so callers can
    /// suspend after every poll without a recursive or allocation burst.
    fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<ResumableSequenceProgress, Spec::Error> {
        self.require_fixed_capacity()?;
        let Some(task) = self.tasks.pop() else {
            return if self.values.len() == 1 {
                Ok(ResumableSequenceProgress::Complete)
            } else {
                Err(Spec::invalid("resumable join has no complete value"))
            };
        };
        self.execute_task(session, receipt, task)?;
        self.require_fixed_capacity()?;
        self.record_scratch(receipt);
        if self.tasks.is_empty() && self.values.len() == 1 {
            Ok(ResumableSequenceProgress::Complete)
        } else {
            Ok(ResumableSequenceProgress::Pending)
        }
    }

    fn execute_task(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        task: ResumableJoinTask,
    ) -> Result<(), Spec::Error> {
        match task {
            ResumableJoinTask::Join { left, right } => {
                self.schedule_join(session, receipt, left, right)?;
            }
            ResumableJoinTask::Balance { left, right } => {
                self.schedule_balance(session, receipt, left, right)?;
            }
            ResumableJoinTask::BalanceWithLeft { left } => {
                let right = self.pop_value()?;
                self.push_task(ResumableJoinTask::Balance { left, right })?;
            }
            ResumableJoinTask::BalanceWithRight { right } => {
                let left = self.pop_value()?;
                self.push_task(ResumableJoinTask::Balance { left, right })?;
            }
            ResumableJoinTask::MakeBranch { left, right } => {
                let branch = make_branch::<Spec, _>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
            ResumableJoinTask::MakeBranchWithLeft { left } => {
                let right = self.pop_value()?;
                let branch = make_branch::<Spec, _>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
            ResumableJoinTask::MakeBranchWithRight { right } => {
                let left = self.pop_value()?;
                let branch = make_branch::<Spec, _>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
            ResumableJoinTask::MakeBranchFromValues => {
                let right = self.pop_value()?;
                let left = self.pop_value()?;
                let branch = make_branch::<Spec, _>(session, left, right, receipt)?;
                self.push_value(branch)?;
            }
        }
        Ok(())
    }

    fn schedule_join(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    ) -> Result<(), Spec::Error> {
        receipt.nodes_visited = receipt
            .nodes_visited
            .checked_add(1)
            .ok_or_else(|| Spec::invalid("sequence visit count overflow"))?;
        let left_height = owner_height::<Spec>(session, &left)?;
        let right_height = owner_height::<Spec>(session, &right)?;
        if left_height > right_height.saturating_add(1) {
            let (outer, inner) = decompose_branch::<Spec, _>(session, left, receipt)?;
            self.push_task(ResumableJoinTask::BalanceWithLeft { left: outer })?;
            self.push_task(ResumableJoinTask::Join { left: inner, right })?;
        } else if right_height > left_height.saturating_add(1) {
            let (inner, outer) = decompose_branch::<Spec, _>(session, right, receipt)?;
            self.push_task(ResumableJoinTask::BalanceWithRight { right: outer })?;
            self.push_task(ResumableJoinTask::Join { left, right: inner })?;
        } else {
            self.push_task(ResumableJoinTask::MakeBranch { left, right })?;
        }
        Ok(())
    }

    fn schedule_balance(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        receipt: &mut SequenceMutationReceipt,
        left: ArenaBuildOwner,
        right: ArenaBuildOwner,
    ) -> Result<(), Spec::Error> {
        let left_height = owner_height::<Spec>(session, &left)?;
        let right_height = owner_height::<Spec>(session, &right)?;
        if left_height > right_height.saturating_add(1) {
            let (left_left, left_right) = decompose_branch::<Spec, _>(session, left, receipt)?;
            if owner_height::<Spec>(session, &left_right)?
                > owner_height::<Spec>(session, &left_left)?
            {
                let (pivot_left, pivot_right) =
                    decompose_branch::<Spec, _>(session, left_right, receipt)?;
                self.push_task(ResumableJoinTask::MakeBranchFromValues)?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: pivot_right,
                    right,
                })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: left_left,
                    right: pivot_left,
                })?;
            } else {
                self.push_task(ResumableJoinTask::MakeBranchWithLeft { left: left_left })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: left_right,
                    right,
                })?;
            }
        } else if right_height > left_height.saturating_add(1) {
            let (right_left, right_right) = decompose_branch::<Spec, _>(session, right, receipt)?;
            if owner_height::<Spec>(session, &right_left)?
                > owner_height::<Spec>(session, &right_right)?
            {
                let (pivot_left, pivot_right) =
                    decompose_branch::<Spec, _>(session, right_left, receipt)?;
                self.push_task(ResumableJoinTask::MakeBranchFromValues)?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left: pivot_right,
                    right: right_right,
                })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left,
                    right: pivot_left,
                })?;
            } else {
                self.push_task(ResumableJoinTask::MakeBranchWithRight { right: right_right })?;
                self.push_task(ResumableJoinTask::MakeBranch {
                    left,
                    right: right_left,
                })?;
            }
        } else {
            self.push_task(ResumableJoinTask::MakeBranch { left, right })?;
        }
        Ok(())
    }

    fn take_root(&mut self) -> Result<ArenaBuildOwner, Spec::Error> {
        if !self.tasks.is_empty() || self.values.len() != 1 {
            return Err(Spec::invalid("resumable join is incomplete"));
        }
        self.values
            .pop()
            .ok_or_else(|| Spec::invalid("resumable join lost its root"))
    }

    fn pop_value(&mut self) -> Result<ArenaBuildOwner, Spec::Error> {
        self.values
            .pop()
            .ok_or_else(|| Spec::invalid("resumable join continuation lost its value"))
    }

    fn push_task(&mut self, task: ResumableJoinTask) -> Result<(), Spec::Error> {
        if self.tasks.len() >= self.task_slot_limit {
            return Err(Spec::invalid("resumable join exceeded logical task bound"));
        }
        self.tasks.push(task);
        Ok(())
    }

    fn push_value(&mut self, value: ArenaBuildOwner) -> Result<(), Spec::Error> {
        if self.values.len() >= self.value_slot_limit {
            return Err(Spec::invalid("resumable join exceeded logical value bound"));
        }
        self.values.push(value);
        Ok(())
    }

    fn require_fixed_capacity(&self) -> Result<(), Spec::Error> {
        if self.tasks.capacity() != self.task_capacity
            || self.values.capacity() != self.value_capacity
        {
            return Err(Spec::invalid("resumable join scratch capacity changed"));
        }
        Ok(())
    }

    fn record_scratch(&self, receipt: &mut SequenceMutationReceipt) {
        receipt.maximum_resumable_join_tasks =
            receipt.maximum_resumable_join_tasks.max(self.tasks.len());
        receipt.maximum_resumable_join_task_requested_bytes = receipt
            .maximum_resumable_join_task_requested_bytes
            .max(self.task_requested_bytes);
        receipt.maximum_resumable_join_task_bytes = receipt
            .maximum_resumable_join_task_bytes
            .max(self.tasks.capacity() * std::mem::size_of::<ResumableJoinTask>());
        receipt.maximum_resumable_join_values =
            receipt.maximum_resumable_join_values.max(self.values.len());
        receipt.maximum_resumable_join_value_requested_bytes = receipt
            .maximum_resumable_join_value_requested_bytes
            .max(self.value_requested_bytes);
        receipt.maximum_resumable_join_value_bytes = receipt
            .maximum_resumable_join_value_bytes
            .max(self.values.capacity() * std::mem::size_of::<ArenaBuildOwner>());
    }
}

fn owner_height<Spec: SequenceSpec>(
    session: &ArenaBuildSession<'_>,
    owner: &ArenaBuildOwner,
) -> Result<u16, Spec::Error>
where
    Spec::Error: From<ArenaBuildError>,
{
    Ok(Spec::height(
        sequence_node::<Spec>(session.arena(), session.owner_id(owner)?)?.0,
    ))
}

fn build_balanced<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    nodes: &mut [Option<ArenaOwnerHandle>],
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    match nodes.len() {
        0 => Ok(None),
        1 => Ok(nodes[0].take()),
        length => {
            let middle = length / 2;
            let (left, right) = nodes.split_at_mut(middle);
            let left = build_balanced::<Spec>(transaction, left, receipt)?
                .ok_or_else(|| Spec::invalid("missing balanced left root"))?;
            let right = build_balanced::<Spec>(transaction, right, receipt)?
                .ok_or_else(|| Spec::invalid("missing balanced right root"))?;
            Ok(Some(make_branch::<Spec, _>(
                transaction,
                left,
                right,
                receipt,
            )?))
        }
    }
}

fn decompose_branch<Spec, Backend>(
    backend: &mut Backend,
    node: Backend::Owner,
    receipt: &mut SequenceMutationReceipt,
) -> Result<(Backend::Owner, Backend::Owner), Spec::Error>
where
    Spec: SequenceSpec,
    Backend: SequenceBuildBackend,
    Spec::Error: From<Backend::Error>,
{
    receipt.nodes_visited += 1;
    let node_id = backend.owner_id(&node)?;
    let (_, SequenceNodeKind::Branch { left, right }) =
        sequence_node::<Spec>(backend.arena(), node_id)?
    else {
        return Err(Spec::invalid("expected sequence branch"));
    };
    let left = backend.retain_sequence_node(left)?;
    let right = backend.retain_sequence_node(right)?;
    backend.release_sequence_owner(node)?;
    Ok((left, right))
}

fn join_nodes<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    left: ArenaOwnerHandle,
    right: ArenaOwnerHandle,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaOwnerHandle, Spec::Error> {
    receipt.nodes_visited += 1;
    let left_height =
        Spec::height(sequence_node::<Spec>(transaction.arena(), transaction.id(&left))?.0);
    let right_height =
        Spec::height(sequence_node::<Spec>(transaction.arena(), transaction.id(&right))?.0);
    if left_height > right_height.saturating_add(1) {
        let (outer, inner) = decompose_branch::<Spec, _>(transaction, left, receipt)?;
        let joined = join_nodes::<Spec>(transaction, inner, right, receipt)?;
        return balance_nodes::<Spec>(transaction, outer, joined, receipt);
    }
    if right_height > left_height.saturating_add(1) {
        let (inner, outer) = decompose_branch::<Spec, _>(transaction, right, receipt)?;
        let joined = join_nodes::<Spec>(transaction, left, inner, receipt)?;
        return balance_nodes::<Spec>(transaction, joined, outer, receipt);
    }
    make_branch::<Spec, _>(transaction, left, right, receipt)
}

fn balance_nodes<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    left: ArenaOwnerHandle,
    right: ArenaOwnerHandle,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaOwnerHandle, Spec::Error> {
    let left_height =
        Spec::height(sequence_node::<Spec>(transaction.arena(), transaction.id(&left))?.0);
    let right_height =
        Spec::height(sequence_node::<Spec>(transaction.arena(), transaction.id(&right))?.0);
    if left_height > right_height.saturating_add(1) {
        let (left_left, left_right) = decompose_branch::<Spec, _>(transaction, left, receipt)?;
        let left_left_height =
            Spec::height(sequence_node::<Spec>(transaction.arena(), transaction.id(&left_left))?.0);
        let left_right_height = Spec::height(
            sequence_node::<Spec>(transaction.arena(), transaction.id(&left_right))?.0,
        );
        if left_right_height > left_left_height {
            let (pivot_left, pivot_right) =
                decompose_branch::<Spec, _>(transaction, left_right, receipt)?;
            let next_left = make_branch::<Spec, _>(transaction, left_left, pivot_left, receipt)?;
            let next_right = make_branch::<Spec, _>(transaction, pivot_right, right, receipt)?;
            return make_branch::<Spec, _>(transaction, next_left, next_right, receipt);
        }
        let next_right = make_branch::<Spec, _>(transaction, left_right, right, receipt)?;
        return make_branch::<Spec, _>(transaction, left_left, next_right, receipt);
    }
    if right_height > left_height.saturating_add(1) {
        let (right_left, right_right) = decompose_branch::<Spec, _>(transaction, right, receipt)?;
        let right_left_height = Spec::height(
            sequence_node::<Spec>(transaction.arena(), transaction.id(&right_left))?.0,
        );
        let right_right_height = Spec::height(
            sequence_node::<Spec>(transaction.arena(), transaction.id(&right_right))?.0,
        );
        if right_left_height > right_right_height {
            let (pivot_left, pivot_right) =
                decompose_branch::<Spec, _>(transaction, right_left, receipt)?;
            let next_left = make_branch::<Spec, _>(transaction, left, pivot_left, receipt)?;
            let next_right =
                make_branch::<Spec, _>(transaction, pivot_right, right_right, receipt)?;
            return make_branch::<Spec, _>(transaction, next_left, next_right, receipt);
        }
        let next_left = make_branch::<Spec, _>(transaction, left, right_left, receipt)?;
        return make_branch::<Spec, _>(transaction, next_left, right_right, receipt);
    }
    make_branch::<Spec, _>(transaction, left, right, receipt)
}

fn concat_nodes<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    left: Option<ArenaOwnerHandle>,
    right: Option<ArenaOwnerHandle>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(left), Some(right)) => {
            Ok(Some(join_nodes::<Spec>(transaction, left, right, receipt)?))
        }
    }
}

fn split_owned<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    node: ArenaOwnerHandle,
    leaf_index: u64,
    receipt: &mut SequenceMutationReceipt,
) -> Result<(Option<ArenaOwnerHandle>, Option<ArenaOwnerHandle>), Spec::Error> {
    receipt.nodes_visited += 1;
    let summary = sequence_node::<Spec>(transaction.arena(), transaction.id(&node))?.0;
    let leaves = Spec::leaves(summary);
    if leaf_index == 0 {
        return Ok((None, Some(node)));
    }
    if leaf_index == leaves {
        return Ok((Some(node), None));
    }
    if leaf_index > leaves {
        return Err(Spec::invalid("sequence split is out of range"));
    }
    let (left, right) = decompose_branch::<Spec, _>(transaction, node, receipt)?;
    let left_leaves =
        Spec::leaves(sequence_node::<Spec>(transaction.arena(), transaction.id(&left))?.0);
    match leaf_index.cmp(&left_leaves) {
        Ordering::Less => {
            let (prefix, middle) = split_owned::<Spec>(transaction, left, leaf_index, receipt)?;
            let suffix = concat_nodes::<Spec>(transaction, middle, Some(right), receipt)?;
            Ok((prefix, suffix))
        }
        Ordering::Equal => Ok((Some(left), Some(right))),
        Ordering::Greater => {
            let (middle, suffix) =
                split_owned::<Spec>(transaction, right, leaf_index - left_leaves, receipt)?;
            let prefix = concat_nodes::<Spec>(transaction, Some(left), middle, receipt)?;
            Ok((prefix, suffix))
        }
    }
}

/// Splices one persistent sequence inside a caller-owned top-level arena
/// transaction. Every replacement handle must already belong to
/// `transaction`; on error the caller must abort that transaction.
pub(crate) fn splice_root_in_transaction<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    old_root: Option<ArenaId>,
    range: Range<u64>,
    replacements: Vec<ArenaOwnerHandle>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    receipt.leaves_adopted += replacements.len();
    let old_leaves = old_root
        .map(|root| {
            sequence_node::<Spec>(transaction.arena(), root).map(|value| Spec::leaves(value.0))
        })
        .transpose()?
        .unwrap_or(0);
    if range.start > range.end || range.end > old_leaves {
        return Err(Spec::invalid("sequence splice is out of range"));
    }
    let working = old_root.map(|id| transaction.retain(id)).transpose()?;
    let (prefix, tail) = if let Some(working) = working {
        split_owned::<Spec>(transaction, working, range.start, receipt)?
    } else {
        (None, None)
    };
    let deleted_count = range.end - range.start;
    let (deleted, suffix) = if let Some(tail) = tail {
        split_owned::<Spec>(transaction, tail, deleted_count, receipt)?
    } else {
        (None, None)
    };
    if let Some(deleted) = deleted {
        transaction.release(deleted)?;
    }
    let mut replacement_nodes = replacements.into_iter().map(Some).collect::<Vec<_>>();
    let replacement = build_balanced::<Spec>(transaction, &mut replacement_nodes, receipt)?;
    let prefix = concat_nodes::<Spec>(transaction, prefix, replacement, receipt)?;
    let root = concat_nodes::<Spec>(transaction, prefix, suffix, receipt)?;
    receipt.leaves_reused += usize::try_from(old_leaves - deleted_count)
        .map_err(|_| Spec::invalid("reused leaf count exceeds usize"))?;
    Ok(root)
}

#[derive(Debug)]
struct PendingLeafReplacement {
    leaf_index: u64,
    expected_leaf: ArenaId,
    replacements: Vec<Option<ArenaOwnerHandle>>,
}

#[derive(Debug)]
pub(crate) struct BaseLeafReplacement {
    pub(crate) leaf_index: u64,
    pub(crate) expected_leaf: ArenaId,
    pub(crate) replacements: Vec<ArenaOwnerHandle>,
}

fn rewrite_leaf_batch<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    node: ArenaId,
    base_leaf_index: u64,
    replacements: &mut [PendingLeafReplacement],
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    receipt.nodes_visited += 1;
    if replacements.is_empty() {
        return Ok(Some(transaction.retain(node)?));
    }

    let (_, kind) = sequence_node::<Spec>(transaction.arena(), node)?;
    match kind {
        SequenceNodeKind::Leaf => {
            if replacements.len() != 1
                || replacements[0].leaf_index != base_leaf_index
                || replacements[0].expected_leaf != node
            {
                return Err(Spec::invalid("batch replacement does not match base leaf"));
            }
            build_balanced::<Spec>(transaction, &mut replacements[0].replacements, receipt)
        }
        SequenceNodeKind::Branch { left, right } => {
            let left_leaves = Spec::leaves(sequence_node::<Spec>(transaction.arena(), left)?.0);
            let pivot = base_leaf_index
                .checked_add(left_leaves)
                .ok_or_else(|| Spec::invalid("batch leaf index overflow"))?;
            let split = replacements.partition_point(|replacement| replacement.leaf_index < pivot);
            let (left_replacements, right_replacements) = replacements.split_at_mut(split);
            let left = rewrite_leaf_batch::<Spec>(
                transaction,
                left,
                base_leaf_index,
                left_replacements,
                receipt,
            )?;
            let right =
                rewrite_leaf_batch::<Spec>(transaction, right, pivot, right_replacements, receipt)?;
            concat_nodes::<Spec>(transaction, left, right, receipt)
        }
    }
}

/// Replaces multiple leaves addressed against one immutable base root.
///
/// The addresses are transient, current-root leaf indices rather than stored
/// ranks. Every replacement is interpreted against `old_root`, so path-copying
/// an earlier edit cannot stale the later edits. Untouched subtrees are retained
/// directly and all replacement ownership remains in the caller's transaction
/// until the final manifest is committed.
pub(crate) fn replace_leaf_batch_in_transaction<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    old_root: Option<ArenaId>,
    replacements: Vec<BaseLeafReplacement>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    let Some(old_root) = old_root else {
        if replacements.is_empty() {
            return Ok(None);
        }
        return Err(Spec::invalid("batch replacement has no base root"));
    };
    let old_leaves = Spec::leaves(sequence_node::<Spec>(transaction.arena(), old_root)?.0);
    let mut previous = None;
    let mut adopted = 0_usize;
    let mut pending = Vec::with_capacity(replacements.len());
    for replacement in replacements {
        let BaseLeafReplacement {
            leaf_index,
            expected_leaf,
            replacements: handles,
        } = replacement;
        if leaf_index >= old_leaves {
            return Err(Spec::invalid("batch replacement is out of range"));
        }
        if previous.is_some_and(|previous| leaf_index <= previous) {
            return Err(Spec::invalid(
                "batch replacement indices are not strictly increasing",
            ));
        }
        previous = Some(leaf_index);
        for handle in &handles {
            if sequence_node::<Spec>(transaction.arena(), transaction.id(handle))?.1
                != SequenceNodeKind::Leaf
            {
                return Err(Spec::invalid("batch replacement input is not a leaf"));
            }
        }
        adopted = adopted
            .checked_add(handles.len())
            .ok_or_else(|| Spec::invalid("batch replacement count overflow"))?;
        pending.push(PendingLeafReplacement {
            leaf_index,
            expected_leaf,
            replacements: handles.into_iter().map(Some).collect(),
        });
    }
    receipt.leaves_adopted += adopted;
    let replaced = u64::try_from(pending.len())
        .map_err(|_| Spec::invalid("batch replacement count exceeds u64"))?;
    receipt.leaves_reused += usize::try_from(old_leaves - replaced)
        .map_err(|_| Spec::invalid("reused leaf count exceeds usize"))?;
    rewrite_leaf_batch::<Spec>(transaction, old_root, 0, &mut pending, receipt)
}

/// Consumes one transaction-owned working root and replaces a leaf range with
/// an already-built transaction-owned subtree. This is the range primitive
/// used by parser fragments; it never collects the replacement's leaves.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn splice_owned_root_in_transaction<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    working_root: Option<ArenaOwnerHandle>,
    range: Range<u64>,
    replacement_root: Option<ArenaOwnerHandle>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    let old_leaves = working_root
        .as_ref()
        .map(|root| {
            sequence_node::<Spec>(transaction.arena(), transaction.id(root))
                .map(|value| Spec::leaves(value.0))
        })
        .transpose()?
        .unwrap_or(0);
    if range.start > range.end || range.end > old_leaves {
        return Err(Spec::invalid("owned sequence splice is out of range"));
    }
    if let Some(replacement) = replacement_root.as_ref() {
        let replacement_summary =
            sequence_node::<Spec>(transaction.arena(), transaction.id(replacement))?.0;
        receipt.leaves_adopted += usize::try_from(Spec::leaves(replacement_summary))
            .map_err(|_| Spec::invalid("replacement leaf count exceeds usize"))?;
    }
    let (prefix, tail) = if let Some(root) = working_root {
        split_owned::<Spec>(transaction, root, range.start, receipt)?
    } else {
        (None, None)
    };
    let removed = range.end - range.start;
    let (deleted, suffix) = if let Some(tail) = tail {
        split_owned::<Spec>(transaction, tail, removed, receipt)?
    } else {
        (None, None)
    };
    if let Some(deleted) = deleted {
        transaction.release(deleted)?;
    }
    let prefix = concat_nodes::<Spec>(transaction, prefix, replacement_root, receipt)?;
    concat_nodes::<Spec>(transaction, prefix, suffix, receipt)
}

/// Retains one immutable base range as a transaction-owned sequence root.
/// Removed prefix/suffix paths are released through the same journal.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn retain_sequence_range_in_transaction<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    base_root: ArenaId,
    range: Range<u64>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    let leaves = Spec::leaves(sequence_node::<Spec>(transaction.arena(), base_root)?.0);
    if range.start > range.end || range.end > leaves {
        return Err(Spec::invalid("retained sequence range is out of bounds"));
    }
    if range.is_empty() {
        return Ok(None);
    }
    let root = transaction.retain(base_root)?;
    let (prefix, tail) = split_owned::<Spec>(transaction, root, range.start, receipt)?;
    if let Some(prefix) = prefix {
        transaction.release(prefix)?;
    }
    let tail = tail.ok_or_else(|| Spec::invalid("retained range has no tail"))?;
    let (middle, suffix) =
        split_owned::<Spec>(transaction, tail, range.end - range.start, receipt)?;
    if let Some(suffix) = suffix {
        transaction.release(suffix)?;
    }
    Ok(middle)
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct BaseRangePatch {
    pub(crate) range: Range<u64>,
    pub(crate) replacement_root: Option<ArenaOwnerHandle>,
}

/// Applies sorted, disjoint ranges addressed against one immutable base root.
/// Patches run from right to left, so an edit can never rebase a lower base
/// coordinate. All work and retained slices remain under one transaction.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn apply_disjoint_base_ranges_in_transaction<Spec: SequenceSpec>(
    transaction: &mut ArenaBuildTransaction<'_>,
    base_root: Option<ArenaId>,
    patches: Vec<BaseRangePatch>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<Option<ArenaOwnerHandle>, Spec::Error> {
    let base_leaves = base_root
        .map(|root| {
            sequence_node::<Spec>(transaction.arena(), root).map(|value| Spec::leaves(value.0))
        })
        .transpose()?
        .unwrap_or(0);
    let mut previous: Option<&Range<u64>> = None;
    let mut deleted = 0_u64;
    for patch in &patches {
        if patch.range.start > patch.range.end || patch.range.end > base_leaves {
            return Err(Spec::invalid("base range patch is out of bounds"));
        }
        if let Some(previous) = previous
            && (previous.end > patch.range.start
                || (previous.is_empty()
                    && patch.range.is_empty()
                    && previous.start == patch.range.start))
        {
            return Err(Spec::invalid(
                "base range patches overlap or duplicate a boundary",
            ));
        }
        deleted = deleted
            .checked_add(patch.range.end - patch.range.start)
            .ok_or_else(|| Spec::invalid("deleted leaf count overflow"))?;
        previous = Some(&patch.range);
    }
    let mut working = base_root.map(|root| transaction.retain(root)).transpose()?;
    for patch in patches.into_iter().rev() {
        working = splice_owned_root_in_transaction::<Spec>(
            transaction,
            working,
            patch.range,
            patch.replacement_root,
            receipt,
        )?;
    }
    receipt.leaves_reused += usize::try_from(base_leaves - deleted)
        .map_err(|_| Spec::invalid("reused leaf count exceeds usize"))?;
    Ok(working)
}

#[derive(Debug)]
pub(crate) struct PersistentSequence<Spec> {
    owner: Option<OwnedArenaRef>,
    marker: PhantomData<Spec>,
}

impl<Spec> Default for PersistentSequence<Spec> {
    fn default() -> Self {
        Self {
            owner: None,
            marker: PhantomData,
        }
    }
}

impl<Spec: SequenceSpec> PersistentSequence<Spec> {
    pub(crate) const fn from_owner(owner: OwnedArenaRef) -> Self {
        Self {
            owner: Some(owner),
            marker: PhantomData,
        }
    }

    pub(crate) fn from_leaves(
        arena: &mut PageArena,
        leaves: Vec<SealedSequenceLeaf>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        // This compatibility API receives an already-collected owner vector.
        // Register every token before the first fallible validation/build step
        // so an error cannot strand the unvisited tail. Streaming callers use
        // `StreamingSequenceBuilder` directly and never create this vector.
        receipt.leaves_adopted += leaves.len();
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut nodes = leaves
            .into_iter()
            .map(|leaf| transaction.track(leaf.owner).map(Some))
            .collect::<Result<Vec<_>, ArenaError>>()?;
        let root = build_balanced::<Spec>(&mut transaction, &mut nodes, receipt)?;
        Ok(Self {
            owner: root.map(|root| transaction.take(root)),
            marker: PhantomData,
        })
    }

    #[cfg(test)]
    pub(crate) fn as_ref(&self) -> PersistentSequenceRef<Spec> {
        PersistentSequenceRef {
            root: self.owner.as_ref().map(OwnedArenaRef::id),
            marker: PhantomData,
        }
    }

    pub(crate) fn root_id(&self) -> Option<ArenaId> {
        self.owner.as_ref().map(OwnedArenaRef::id)
    }

    pub(crate) fn splice_leaves(
        &self,
        arena: &mut PageArena,
        range: Range<u64>,
        replacements: Vec<SealedSequenceLeaf>,
        receipt: &mut SequenceMutationReceipt,
    ) -> Result<Self, Spec::Error> {
        let mut transaction = ArenaBuildTransaction::new(arena);
        // Register replacement ownership before even reading the old root. A
        // stale/corrupt root or invalid range must roll back incoming tokens.
        let mut replacement_nodes = replacements
            .into_iter()
            .map(|leaf| transaction.track(leaf.owner).map(Some))
            .collect::<Result<Vec<_>, ArenaError>>()?;
        let replacements = replacement_nodes
            .iter_mut()
            .map(|node| node.take().expect("registered replacement handle"))
            .collect();
        let root = splice_root_in_transaction::<Spec>(
            &mut transaction,
            self.root_id(),
            range,
            replacements,
            receipt,
        )?;
        Ok(Self {
            owner: root.map(|root| transaction.take(root)),
            marker: PhantomData,
        })
    }

    pub(crate) fn into_owner(mut self) -> Option<OwnedArenaRef> {
        self.owner.take()
    }

    pub(crate) fn release_later(mut self, arena: &mut PageArena) -> Result<(), Spec::Error> {
        if let Some(owner) = self.owner.take() {
            arena.release_later(owner).map_err(|failure| {
                // This shared helper still serves retained, non-selected
                // bakeoff formats whose error types cannot carry a linear
                // owner. Keep the lossy bridge here rather than weakening the
                // recoverable PageArena API.
                Spec::Error::from(failure.error)
            })?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct PersistentSequenceRef<Spec> {
    root: Option<ArenaId>,
    marker: PhantomData<Spec>,
}

impl<Spec> Clone for PersistentSequenceRef<Spec> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Spec> Copy for PersistentSequenceRef<Spec> {}

impl<Spec> Default for PersistentSequenceRef<Spec> {
    fn default() -> Self {
        Self {
            root: None,
            marker: PhantomData,
        }
    }
}

impl<Spec: SequenceSpec> PersistentSequenceRef<Spec> {
    pub(crate) const fn from_root(root: Option<ArenaId>) -> Self {
        Self {
            root,
            marker: PhantomData,
        }
    }

    #[cfg(test)]
    pub(crate) const fn root_id(self) -> Option<ArenaId> {
        self.root
    }

    pub(crate) fn summary(self, arena: &PageArena) -> Result<Option<Spec::Summary>, Spec::Error> {
        self.root
            .map(|root| sequence_node::<Spec>(arena, root).map(|value| value.0))
            .transpose()
    }

    pub(crate) fn locate_leaf(
        self,
        arena: &PageArena,
        leaf_index: u64,
    ) -> Result<Option<ArenaId>, Spec::Error> {
        let Some(mut node) = self.root else {
            return Ok(None);
        };
        let summary = sequence_node::<Spec>(arena, node)?.0;
        if leaf_index >= Spec::leaves(summary) {
            return Ok(None);
        }
        let mut index = leaf_index;
        loop {
            match sequence_node::<Spec>(arena, node)?.1 {
                SequenceNodeKind::Leaf => return Ok(Some(node)),
                SequenceNodeKind::Branch { left, right } => {
                    let left_leaves = Spec::leaves(sequence_node::<Spec>(arena, left)?.0);
                    if index < left_leaves {
                        node = left;
                    } else {
                        index -= left_leaves;
                        node = right;
                    }
                }
            }
        }
    }

    pub(crate) fn right_partition_root(
        self,
        arena: &PageArena,
    ) -> Result<Option<ArenaId>, Spec::Error> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        Ok(match sequence_node::<Spec>(arena, root)?.1 {
            SequenceNodeKind::Leaf => None,
            SequenceNodeKind::Branch { right, .. } => Some(right),
        })
    }

    /// Test-only structural witness. Hot-path receipts must use logarithmic
    /// locate/query APIs rather than this whole-tree identity walk.
    pub(crate) fn contains_node(
        self,
        arena: &PageArena,
        needle: ArenaId,
    ) -> Result<bool, Spec::Error> {
        let Some(root) = self.root else {
            return Ok(false);
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node == needle {
                return Ok(true);
            }
            if let SequenceNodeKind::Branch { left, right } = sequence_node::<Spec>(arena, node)?.1
            {
                stack.push(right);
                stack.push(left);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::{LazyLock, Mutex};

    use super::*;

    const LEAF_TAG: u8 = 0xd1;
    const BRANCH_TAG: u8 = 0xd2;
    static FAIL_AT_LEAVES: AtomicU64 = AtomicU64::new(u64::MAX);
    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestSummary {
        leaves: u64,
        height: u16,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TestError {
        Arena(ArenaError),
        ArenaBuild(ArenaBuildError),
        Invalid(&'static str),
    }

    impl From<ArenaError> for TestError {
        fn from(value: ArenaError) -> Self {
            Self::Arena(value)
        }
    }

    impl From<ArenaBuildError> for TestError {
        fn from(value: ArenaBuildError) -> Self {
            Self::ArenaBuild(value)
        }
    }

    #[derive(Debug)]
    struct TestSpec;

    impl SequenceSpec for TestSpec {
        type Summary = TestSummary;
        type Error = TestError;
        type BranchPayload = [u8; 11];

        fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
            Ok((payload == [LEAF_TAG]).then_some(TestSummary {
                leaves: 1,
                height: 1,
            }))
        }

        fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
            if payload.first().copied() != Some(BRANCH_TAG) {
                return Ok(None);
            }
            if payload.len() != 11 {
                return Err(TestError::Invalid("bad test branch"));
            }
            Ok(Some(TestSummary {
                leaves: u64::from_le_bytes(payload[1..9].try_into().expect("fixed test summary")),
                height: u16::from_le_bytes(payload[9..11].try_into().expect("fixed test height")),
            }))
        }

        fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
            let mut payload = [0_u8; 11];
            payload[0] = BRANCH_TAG;
            payload[1..9].copy_from_slice(&summary.leaves.to_le_bytes());
            payload[9..11].copy_from_slice(&summary.height.to_le_bytes());
            payload
        }

        fn combine(
            left: Self::Summary,
            right: Self::Summary,
        ) -> Result<Self::Summary, Self::Error> {
            let leaves = left.leaves + right.leaves;
            if leaves >= FAIL_AT_LEAVES.load(AtomicOrdering::Relaxed) {
                return Err(TestError::Invalid("forced test branch failure"));
            }
            Ok(TestSummary {
                leaves,
                height: left.height.max(right.height) + 1,
            })
        }

        fn leaves(summary: Self::Summary) -> u64 {
            summary.leaves
        }

        fn height(summary: Self::Summary) -> u16 {
            summary.height
        }

        fn invalid(message: &'static str) -> Self::Error {
            TestError::Invalid(message)
        }
    }

    fn leaves(arena: &mut PageArena, count: usize) -> Vec<SealedSequenceLeaf> {
        (0..count)
            .map(|_| {
                SealedSequenceLeaf::new(
                    arena
                        .allocate(&[LEAF_TAG], &[])
                        .expect("test leaf allocation")
                        .owner,
                )
            })
            .collect()
    }

    fn reclaim_all(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(10_000).expect("test reclaim");
        }
    }

    fn assert_avl_and_collect(
        arena: &PageArena,
        node: ArenaId,
        leaves: &mut Vec<ArenaId>,
    ) -> TestSummary {
        let (summary, kind) = sequence_node::<TestSpec>(arena, node).unwrap();
        match kind {
            SequenceNodeKind::Leaf => {
                leaves.push(node);
                assert_eq!(
                    summary,
                    TestSummary {
                        leaves: 1,
                        height: 1
                    }
                );
            }
            SequenceNodeKind::Branch { left, right } => {
                let left = assert_avl_and_collect(arena, left, leaves);
                let right = assert_avl_and_collect(arena, right, leaves);
                assert!(
                    left.height.abs_diff(right.height) <= 1,
                    "non-AVL node {node:?}: left={left:?}, right={right:?}"
                );
                assert_eq!(
                    summary,
                    TestSummary {
                        leaves: left.leaves + right.leaves,
                        height: left.height.max(right.height) + 1,
                    }
                );
            }
        }
        summary
    }

    fn maximum_avl_height(leaves: u64) -> u16 {
        let mut height = 1_u16;
        let mut minimum_at_height = 1_u64;
        let mut minimum_at_next_height = 2_u64;
        while minimum_at_next_height <= leaves {
            height += 1;
            let next = minimum_at_height.saturating_add(minimum_at_next_height);
            minimum_at_height = minimum_at_next_height;
            minimum_at_next_height = next;
        }
        height
    }

    fn poll_splice_with_suspend(
        arena: &mut PageArena,
        ticket: crate::ArenaBuildTicket,
        splice: &mut ResumableSequenceSplice<TestSpec>,
        receipt: &mut SequenceMutationReceipt,
    ) -> (crate::ArenaBuildTicket, ResumableSequenceSplitProgress) {
        let mut session = arena.resume_build(ticket).unwrap();
        let before = receipt.branches_allocated;
        let progress = splice.poll(&mut session, receipt).unwrap();
        assert!(
            receipt.branches_allocated - before <= 1,
            "one splice poll allocated multiple branches: {receipt:?}"
        );
        (session.suspend().unwrap(), progress)
    }

    fn finish_splice_with_suspend(
        arena: &mut PageArena,
        mut ticket: crate::ArenaBuildTicket,
        splice: &mut ResumableSequenceSplice<TestSpec>,
        receipt: &mut SequenceMutationReceipt,
    ) -> crate::ArenaBuildTicket {
        loop {
            let (next, progress) = poll_splice_with_suspend(arena, ticket, splice, receipt);
            ticket = next;
            if progress == ResumableSequenceSplitProgress::Complete {
                return ticket;
            }
        }
    }

    fn owner_leaf_ids(session: &ArenaBuildSession<'_>, owner: &ArenaBuildOwner) -> Vec<ArenaId> {
        let mut ids = Vec::new();
        assert_avl_and_collect(session.arena(), session.owner_id(owner).unwrap(), &mut ids);
        ids
    }

    fn abort_suspended_build(arena: &mut PageArena, ticket: crate::ArenaBuildTicket) {
        let abort = arena.begin_build_abort(ticket).unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        reclaim_all(arena);
    }

    #[test]
    fn resumable_finish_is_avl_for_awkward_counts_and_suspends_after_every_task() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut counts = (1_usize..=257).collect::<Vec<_>>();
        counts.extend([511, 512, 513, 1_023, 1_024, 1_025]);
        for count in counts {
            let mut arena = PageArena::new();
            let mut ticket = arena.begin_build().unwrap();
            let mut receipt = SequenceMutationReceipt::default();
            let mut builder =
                ResumableStreamingSequenceBuilder::<TestSpec>::try_new(&mut receipt).unwrap();
            let mut expected = Vec::with_capacity(count);

            for _ in 0..count {
                let mut session = arena.resume_build(ticket).unwrap();
                let (leaf, _) = session.allocate(&[LEAF_TAG], &[]).unwrap();
                expected.push(session.owner_id(&leaf).unwrap());
                builder.begin_push(&session, leaf, &mut receipt).unwrap();
                ticket = session.suspend().unwrap();
                loop {
                    let mut session = arena.resume_build(ticket).unwrap();
                    let before = receipt.branches_allocated;
                    let progress = builder.poll_push(&mut session, &mut receipt).unwrap();
                    assert!(receipt.branches_allocated - before <= 1);
                    ticket = session.suspend().unwrap();
                    if progress == ResumableSequenceProgress::Complete {
                        break;
                    }
                }
            }

            builder.begin_finish(&mut receipt).unwrap();
            loop {
                let mut session = arena.resume_build(ticket).unwrap();
                let before = receipt.branches_allocated;
                let progress = builder.poll_finish(&mut session, &mut receipt).unwrap();
                assert!(receipt.branches_allocated - before <= 1);
                ticket = session.suspend().unwrap();
                if progress == ResumableSequenceProgress::Complete {
                    break;
                }
            }

            let root = builder.take_root().unwrap();
            let session = arena.resume_build(ticket).unwrap();
            let root_id = session.owner_id(&root).unwrap();
            let mut actual = Vec::with_capacity(count);
            let summary = assert_avl_and_collect(session.arena(), root_id, &mut actual);
            assert_eq!(actual, expected, "source order changed for {count} leaves");
            assert_eq!(summary.leaves, u64::try_from(count).unwrap());
            assert!(
                summary.height <= maximum_avl_height(summary.leaves),
                "non-logarithmic height for {count} leaves: {summary:?}"
            );
            assert!(receipt.maximum_resumable_join_tasks <= 16);
            assert!(receipt.maximum_resumable_join_values <= 2);
            if count.count_ones() > 1 {
                assert!(receipt.maximum_resumable_join_task_requested_bytes > 0);
                assert!(
                    receipt.maximum_resumable_join_task_bytes
                        >= receipt.maximum_resumable_join_task_requested_bytes
                );
                assert!(
                    receipt.maximum_resumable_join_value_bytes
                        >= receipt.maximum_resumable_join_value_requested_bytes
                );
            }

            let abort = session.begin_abort().unwrap();
            while !arena.poll_build_abort(abort, 1).unwrap().complete {}
            reclaim_all(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[test]
    fn resumable_finish_can_abort_mid_rotation_with_one_owner_of_fuel() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let mut ticket = arena.begin_build().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut builder =
            ResumableStreamingSequenceBuilder::<TestSpec>::try_new(&mut receipt).unwrap();
        for _ in 0..13 {
            let mut session = arena.resume_build(ticket).unwrap();
            let (leaf, _) = session.allocate(&[LEAF_TAG], &[]).unwrap();
            builder.begin_push(&session, leaf, &mut receipt).unwrap();
            ticket = session.suspend().unwrap();
            loop {
                let mut session = arena.resume_build(ticket).unwrap();
                let progress = builder.poll_push(&mut session, &mut receipt).unwrap();
                ticket = session.suspend().unwrap();
                if progress == ResumableSequenceProgress::Complete {
                    break;
                }
            }
        }
        builder.begin_finish(&mut receipt).unwrap();
        let branches_before_finish = receipt.branches_allocated;
        loop {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = builder.poll_finish(&mut session, &mut receipt).unwrap();
            if receipt.branches_allocated > branches_before_finish {
                let owners = session.live_owners().unwrap();
                assert!(owners > 1);
                let abort = session.begin_abort().unwrap();
                assert_eq!(
                    arena.build_lifecycle(abort).unwrap(),
                    crate::ArenaBuildLifecycle::Aborting
                );
                let zero = arena.poll_build_abort(abort, 0).unwrap();
                assert_eq!(zero.owners_scheduled, 0);
                assert_eq!(zero.owners_remaining, owners);
                let mut remaining = owners;
                while remaining != 0 {
                    let step = arena.poll_build_abort(abort, 1).unwrap();
                    assert!(step.owners_scheduled <= 1);
                    assert!(step.owners_remaining < remaining);
                    remaining = step.owners_remaining;
                }
                break;
            }
            assert_eq!(progress, ResumableSequenceProgress::Pending);
            ticket = session.suspend().unwrap();
        }
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn logical_scratch_limits_do_not_expand_to_allocator_capacity() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut owner = || session.allocate(&[LEAF_TAG], &[]).unwrap().0;

        let mut bins = Vec::with_capacity(8);
        let bin_capacity = bins.capacity();
        assert!(bin_capacity > 1);
        let mut builder = ResumableStreamingSequenceBuilder::<TestSpec> {
            bins: std::mem::take(&mut bins),
            bin_slot_limit: 1,
            bin_capacity,
            bin_requested_bytes: std::mem::size_of::<Option<ArenaBuildOwner>>(),
            carry: None,
            reduction: None,
            join: ResumableSequenceJoin::try_preallocated(
                maximum_avl_sequence_height(u64::MAX),
                &mut SequenceMutationReceipt::default(),
            )
            .unwrap(),
            marker: PhantomData,
        };
        builder.push_bin(owner()).unwrap();
        assert_eq!(
            builder.push_bin(owner()),
            Err(TestError::Invalid(
                "resumable sequence exceeded logical bin bound"
            ))
        );
        assert_eq!(builder.bins.capacity(), bin_capacity);

        let mut tasks = Vec::with_capacity(8);
        let task_capacity = tasks.capacity();
        assert!(task_capacity > 1);
        tasks.push(ResumableJoinTask::Join {
            left: owner(),
            right: owner(),
        });
        let mut values = Vec::with_capacity(8);
        let value_capacity = values.capacity();
        assert!(value_capacity > 1);
        values.push(owner());
        let mut join = ResumableSequenceJoin::<TestSpec> {
            tasks,
            values,
            task_requested_bytes: std::mem::size_of::<ResumableJoinTask>(),
            task_slot_limit: 1,
            task_capacity,
            value_requested_bytes: std::mem::size_of::<ArenaBuildOwner>(),
            value_slot_limit: 1,
            value_capacity,
            marker: PhantomData,
        };
        assert_eq!(
            join.push_task(ResumableJoinTask::Join {
                left: owner(),
                right: owner(),
            }),
            Err(TestError::Invalid(
                "resumable join exceeded logical task bound"
            ))
        );
        assert_eq!(
            join.push_value(owner()),
            Err(TestError::Invalid(
                "resumable join exceeded logical value bound"
            ))
        );
        assert_eq!(join.tasks.capacity(), task_capacity);
        assert_eq!(join.values.capacity(), value_capacity);

        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn failed_mid_build_rolls_back_every_consumed_owner() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(4, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let pages = leaves(&mut arena, 4);
        let ids = pages.iter().map(SealedSequenceLeaf::id).collect::<Vec<_>>();
        let error = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            pages,
            &mut SequenceMutationReceipt::default(),
        )
        .expect_err("root branch allocation is forced to fail");
        assert_eq!(error, TestError::Invalid("forced test branch failure"));
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert!(ids.into_iter().all(|id| !arena.contains(id)));
    }

    #[test]
    fn failed_mid_splice_keeps_old_root_queryable_and_reclaims_working_copy() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 8);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .expect("initial sequence");
        let old_root = sequence.as_ref().root_id().expect("old root");
        let baseline = arena.metrics().live_nodes;

        FAIL_AT_LEAVES.store(4, AtomicOrdering::Relaxed);
        let replacement = leaves(&mut arena, 1);
        PersistentSequence::<TestSpec>::splice_leaves(
            &sequence,
            &mut arena,
            1..2,
            replacement,
            &mut SequenceMutationReceipt::default(),
        )
        .expect_err("working-copy join is forced to fail");
        reclaim_all(&mut arena);

        assert!(arena.contains(old_root));
        assert_eq!(
            sequence.as_ref().summary(&arena).unwrap().unwrap().leaves,
            8
        );
        assert!(sequence.as_ref().locate_leaf(&arena, 7).unwrap().is_some());
        assert_eq!(arena.metrics().live_nodes, baseline);

        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn invalid_range_with_nonempty_replacement_rolls_back_incoming_owners() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 2);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .expect("initial sequence");
        let baseline = arena.metrics().live_nodes;
        let replacement = leaves(&mut arena, 2);
        let error = sequence
            .splice_leaves(
                &mut arena,
                3..3,
                replacement,
                &mut SequenceMutationReceipt::default(),
            )
            .expect_err("range beyond old leaves");
        assert_eq!(error, TestError::Invalid("sequence splice is out of range"));
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);
        assert_eq!(
            sequence.as_ref().summary(&arena).unwrap().unwrap().leaves,
            2
        );
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn corrupt_old_root_with_replacement_rolls_back_incoming_owners() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let corrupt_owner = arena
            .allocate(&[0xff], &[])
            .expect("corrupt test root allocation")
            .owner;
        let sequence = PersistentSequence::<TestSpec>::from_owner(corrupt_owner);
        let replacement = leaves(&mut arena, 2);
        let error = sequence
            .splice_leaves(
                &mut arena,
                0..0,
                replacement,
                &mut SequenceMutationReceipt::default(),
            )
            .expect_err("corrupt old root must fail before mutation");
        assert_eq!(error, TestError::Invalid("unknown sequence node"));
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 1);
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn one_base_root_batch_rewrites_disjoint_leaves_without_staling_later_edits() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 16);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .expect("initial sequence");
        let old = sequence.as_ref();
        let old_root = old.root_id().expect("old root");
        let old_leaf_0 = old.locate_leaf(&arena, 0).unwrap().unwrap();
        let old_leaf_1 = old.locate_leaf(&arena, 1).unwrap().unwrap();
        let old_leaf_7 = old.locate_leaf(&arena, 7).unwrap().unwrap();
        let old_leaf_8 = old.locate_leaf(&arena, 8).unwrap().unwrap();
        let old_leaf_14 = old.locate_leaf(&arena, 14).unwrap().unwrap();
        let old_leaf_15 = old.locate_leaf(&arena, 15).unwrap().unwrap();

        let mut replacements = leaves(&mut arena, 3).into_iter();
        let first = replacements.next().unwrap().owner;
        let middle = replacements.next().unwrap().owner;
        let last = replacements.next().unwrap().owner;
        let replacement_ids = [first.id(), middle.id(), last.id()];
        let mut receipt = SequenceMutationReceipt::default();
        let next_owner = {
            let mut transaction = ArenaBuildTransaction::new(&mut arena);
            let replacements = vec![
                BaseLeafReplacement {
                    leaf_index: 1,
                    expected_leaf: old_leaf_1,
                    replacements: vec![transaction.track(first).unwrap()],
                },
                BaseLeafReplacement {
                    leaf_index: 8,
                    expected_leaf: old_leaf_8,
                    replacements: vec![transaction.track(middle).unwrap()],
                },
                BaseLeafReplacement {
                    leaf_index: 14,
                    expected_leaf: old_leaf_14,
                    replacements: vec![transaction.track(last).unwrap()],
                },
            ];
            let next = replace_leaf_batch_in_transaction::<TestSpec>(
                &mut transaction,
                Some(old_root),
                replacements,
                &mut receipt,
            )
            .unwrap()
            .expect("batch root");
            transaction.take(next)
        };
        let next = PersistentSequence::<TestSpec>::from_owner(next_owner);
        let next_ref = next.as_ref();
        assert_eq!(next_ref.summary(&arena).unwrap().unwrap().leaves, 16);
        assert_eq!(
            next_ref.locate_leaf(&arena, 1).unwrap(),
            Some(replacement_ids[0])
        );
        assert_eq!(
            next_ref.locate_leaf(&arena, 8).unwrap(),
            Some(replacement_ids[1])
        );
        assert_eq!(
            next_ref.locate_leaf(&arena, 14).unwrap(),
            Some(replacement_ids[2])
        );
        assert_eq!(next_ref.locate_leaf(&arena, 0).unwrap(), Some(old_leaf_0));
        assert_eq!(next_ref.locate_leaf(&arena, 7).unwrap(), Some(old_leaf_7));
        assert_eq!(next_ref.locate_leaf(&arena, 15).unwrap(), Some(old_leaf_15));
        assert_eq!(receipt.leaves_reused, 13);
        assert_eq!(receipt.leaves_adopted, 3);
        assert!(arena.contains(old_root));

        next.release_later(&mut arena).unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn base_root_batch_supports_delete_and_expansion_in_one_transaction() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 8);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .expect("initial sequence");
        let old = sequence.as_ref();
        let old_root = old.root_id().unwrap();
        let old_leaf_2 = old.locate_leaf(&arena, 2).unwrap().unwrap();
        let old_leaf_5 = old.locate_leaf(&arena, 5).unwrap().unwrap();
        let old_suffix = old.locate_leaf(&arena, 7).unwrap().unwrap();
        let mut added = leaves(&mut arena, 2).into_iter();
        let first = added.next().unwrap().owner;
        let second = added.next().unwrap().owner;
        let next_owner = {
            let mut transaction = ArenaBuildTransaction::new(&mut arena);
            let first = transaction.track(first).unwrap();
            let second = transaction.track(second).unwrap();
            let next = replace_leaf_batch_in_transaction::<TestSpec>(
                &mut transaction,
                Some(old_root),
                vec![
                    BaseLeafReplacement {
                        leaf_index: 2,
                        expected_leaf: old_leaf_2,
                        replacements: Vec::new(),
                    },
                    BaseLeafReplacement {
                        leaf_index: 5,
                        expected_leaf: old_leaf_5,
                        replacements: vec![first, second],
                    },
                ],
                &mut SequenceMutationReceipt::default(),
            )
            .unwrap()
            .expect("nonempty batch root");
            transaction.take(next)
        };
        let next = PersistentSequence::<TestSpec>::from_owner(next_owner);
        assert_eq!(next.as_ref().summary(&arena).unwrap().unwrap().leaves, 8);
        assert_eq!(
            next.as_ref().locate_leaf(&arena, 7).unwrap(),
            Some(old_suffix)
        );

        next.release_later(&mut arena).unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn failed_base_root_batch_rolls_back_new_leaves_and_preserves_old_root() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 8);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .expect("initial sequence");
        let old_root = sequence.as_ref().root_id().unwrap();
        let old_leaf_3 = sequence.as_ref().locate_leaf(&arena, 3).unwrap().unwrap();
        let baseline = arena.metrics().live_nodes;
        let replacement = leaves(&mut arena, 1).pop().unwrap().owner;
        FAIL_AT_LEAVES.store(4, AtomicOrdering::Relaxed);
        {
            let mut transaction = ArenaBuildTransaction::new(&mut arena);
            let replacement = transaction.track(replacement).unwrap();
            replace_leaf_batch_in_transaction::<TestSpec>(
                &mut transaction,
                Some(old_root),
                vec![BaseLeafReplacement {
                    leaf_index: 3,
                    expected_leaf: old_leaf_3,
                    replacements: vec![replacement],
                }],
                &mut SequenceMutationReceipt::default(),
            )
            .expect_err("branch allocation is forced to fail");
        }
        reclaim_all(&mut arena);
        assert!(arena.contains(old_root));
        assert_eq!(arena.metrics().live_nodes, baseline);
        assert_eq!(
            sequence.as_ref().summary(&arena).unwrap().unwrap().leaves,
            8
        );

        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn disjoint_base_ranges_apply_right_to_left_and_skip_large_deleted_interiors() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 1_024);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old = sequence.as_ref();
        let old_root = old.root_id().unwrap();
        let before = old.locate_leaf(&arena, 99).unwrap().unwrap();
        let after = old.locate_leaf(&arena, 900).unwrap().unwrap();
        let far = old.locate_leaf(&arena, 1_023).unwrap().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let next_owner = {
            let mut transaction = ArenaBuildTransaction::new(&mut arena);
            let next = apply_disjoint_base_ranges_in_transaction::<TestSpec>(
                &mut transaction,
                Some(old_root),
                vec![BaseRangePatch {
                    range: 100..900,
                    replacement_root: None,
                }],
                &mut receipt,
            )
            .unwrap()
            .unwrap();
            transaction.take(next)
        };
        let next = PersistentSequence::<TestSpec>::from_owner(next_owner);
        assert_eq!(next.as_ref().summary(&arena).unwrap().unwrap().leaves, 224);
        assert_eq!(next.as_ref().locate_leaf(&arena, 99).unwrap(), Some(before));
        assert_eq!(next.as_ref().locate_leaf(&arena, 100).unwrap(), Some(after));
        assert_eq!(next.as_ref().locate_leaf(&arena, 223).unwrap(), Some(far));
        assert_eq!(receipt.leaves_reused, 224);
        assert!(receipt.nodes_visited < 100, "{receipt:?}");
        assert!(arena.contains(old_root));

        next.release_later(&mut arena).unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn retained_base_slice_can_be_cut_and_reinserted_atomically() -> Result<(), TestError> {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 16);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old = sequence.as_ref();
        let old_root = old.root_id().unwrap();
        let old_ids = (0..16)
            .map(|index| old.locate_leaf(&arena, index).unwrap().unwrap())
            .collect::<Vec<_>>();
        let mut receipt = SequenceMutationReceipt::default();
        let next_owner = {
            let mut transaction = ArenaBuildTransaction::new(&mut arena);
            let moved = retain_sequence_range_in_transaction::<TestSpec>(
                &mut transaction,
                old_root,
                4..8,
                &mut receipt,
            )?
            .expect("retained move range");
            let next = apply_disjoint_base_ranges_in_transaction::<TestSpec>(
                &mut transaction,
                Some(old_root),
                vec![
                    BaseRangePatch {
                        range: 4..8,
                        replacement_root: None,
                    },
                    BaseRangePatch {
                        range: 12..12,
                        replacement_root: Some(moved),
                    },
                ],
                &mut receipt,
            )?
            .expect("reparented root");
            transaction.take(next)
        };
        let next = PersistentSequence::<TestSpec>::from_owner(next_owner);
        let expected = [0, 1, 2, 3, 8, 9, 10, 11, 4, 5, 6, 7, 12, 13, 14, 15];
        for (index, old_index) in expected.into_iter().enumerate() {
            assert_eq!(
                next.as_ref()
                    .locate_leaf(&arena, u64::try_from(index).unwrap())?,
                Some(old_ids[old_index])
            );
        }
        assert!(arena.contains(old_root));

        next.release_later(&mut arena)?;
        sequence.release_later(&mut arena)?;
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        Ok::<(), TestError>(())
    }

    #[test]
    fn resumable_retained_range_suspends_at_fuel_one_and_preserves_leaf_identity() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 64);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old = sequence.as_ref();
        let old_root = old.root_id().unwrap();
        let old_ids = (0..64)
            .map(|index| old.locate_leaf(&arena, index).unwrap().unwrap())
            .collect::<Vec<_>>();

        let mut ticket = arena.begin_build().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut job = ResumableSequenceRetainedRange::<TestSpec>::try_new(
            &ticket,
            &arena,
            old_root,
            3..61,
            &mut receipt,
        )
        .unwrap();
        assert_eq!(job.build_id(), ticket.id());

        let retained_owner = loop {
            let mut session = arena.resume_build(ticket).unwrap();
            let allocations_before = receipt.branches_allocated;
            let progress = job.poll(&mut session, &mut receipt).unwrap();
            assert!(
                receipt.branches_allocated - allocations_before <= 1,
                "one split poll allocated multiple branches: {receipt:?}"
            );
            if progress == ResumableSequenceSplitProgress::Complete {
                let root = job.take_root().unwrap().expect("nonempty retained range");
                assert_eq!(session.live_owners().unwrap(), 1);
                break session.commit(root).unwrap();
            }
            ticket = session.suspend().unwrap();
        };

        let retained = PersistentSequence::<TestSpec>::from_owner(retained_owner);
        let retained_ref = retained.as_ref();
        let retained_root = retained_ref.root_id().unwrap();
        let mut actual_ids = Vec::new();
        let retained_summary = assert_avl_and_collect(&arena, retained_root, &mut actual_ids);
        assert_eq!(retained_summary.leaves, 58);
        assert_eq!(actual_ids, old_ids[3..61]);
        assert!(receipt.branches_allocated > 0);
        assert!(receipt.resumable_split_polls > 0);
        assert_eq!(receipt.resumable_split_frame_reservations, 1);
        assert_eq!(receipt.resumable_join_scratch_reservations, 1);
        assert!(receipt.maximum_resumable_split_frames > 0);
        assert!(
            receipt.maximum_resumable_split_frame_bytes
                >= receipt.maximum_resumable_split_frame_requested_bytes
        );
        assert!(
            receipt.maximum_resumable_split_total_scratch_bytes
                >= receipt.maximum_resumable_split_total_requested_bytes
        );
        assert!(arena.contains(old_root));

        retained.release_later(&mut arena).unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn aligned_retained_range_reuses_the_exact_existing_subtree_root() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 32);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let (_, SequenceNodeKind::Branch { right, .. }) =
            sequence_node::<TestSpec>(&arena, old_root).unwrap()
        else {
            panic!("32-leaf fixture must have a branch root");
        };
        assert_eq!(
            sequence_node::<TestSpec>(&arena, right).unwrap().0.leaves,
            16
        );

        let mut ticket = arena.begin_build().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut job = ResumableSequenceRetainedRange::<TestSpec>::try_new(
            &ticket,
            &arena,
            old_root,
            16..32,
            &mut receipt,
        )
        .unwrap();
        let retained_owner = loop {
            let mut session = arena.resume_build(ticket).unwrap();
            match job.poll(&mut session, &mut receipt).unwrap() {
                ResumableSequenceSplitProgress::Pending => {
                    ticket = session.suspend().unwrap();
                }
                ResumableSequenceSplitProgress::Complete => {
                    let root = job.take_root().unwrap().unwrap();
                    assert_eq!(session.owner_id(&root).unwrap(), right);
                    break session.commit(root).unwrap();
                }
            }
        };
        assert_eq!(retained_owner.id(), right);
        assert_eq!(receipt.branches_allocated, 0);

        PersistentSequence::<TestSpec>::from_owner(retained_owner)
            .release_later(&mut arena)
            .unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn resumable_owned_splice_deletes_every_range_with_exact_ids_and_one_allocation_per_poll() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 17);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let base = sequence.as_ref();
        let old_root = base.root_id().unwrap();
        let expected = (0..17)
            .map(|index| base.locate_leaf(&arena, index).unwrap().unwrap())
            .collect::<Vec<_>>();
        let baseline = arena.metrics().live_nodes;

        for start in 0_u64..=17 {
            for end in start..=17 {
                let ticket = arena.begin_build().unwrap();
                let mut session = arena.resume_build(ticket).unwrap();
                let working = session.retain(old_root).unwrap();
                let mut receipt = SequenceMutationReceipt::default();
                let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
                    &session,
                    Some(working),
                    start..end,
                    None,
                    &mut receipt,
                )
                .unwrap();
                assert_eq!(splice.build_id(), session.id());
                let ticket = session.suspend().unwrap();
                let ticket =
                    finish_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
                let session = arena.resume_build(ticket).unwrap();
                let output = splice.take_root().unwrap();
                assert_eq!(
                    session.live_owners().unwrap(),
                    usize::from(output.is_some())
                );
                let mut expected_ids = expected[..usize::try_from(start).unwrap()].to_vec();
                expected_ids.extend_from_slice(&expected[usize::try_from(end).unwrap()..]);
                match output.as_ref() {
                    Some(root) => {
                        assert_eq!(owner_leaf_ids(&session, root), expected_ids);
                        if start == end {
                            assert_eq!(session.owner_id(root).unwrap(), old_root);
                        }
                    }
                    None => assert!(expected_ids.is_empty()),
                }
                if start == 0 && end == 0 {
                    assert_eq!(
                        session.owner_id(output.as_ref().unwrap()).unwrap(),
                        old_root
                    );
                }
                assert_eq!(
                    receipt.leaves_reused,
                    usize::try_from(17 - (end - start)).unwrap()
                );
                assert_eq!(receipt.leaves_adopted, 0);
                assert_eq!(
                    receipt.resumable_splice_deleted_roots_released,
                    usize::from(start != end)
                );
                if start == end || (start == 0 && end == 17) {
                    assert_eq!(receipt.branches_allocated, 0);
                    assert_eq!(receipt.resumable_split_frame_reservations, 0);
                    assert_eq!(receipt.resumable_join_scratch_reservations, 0);
                } else {
                    assert_eq!(receipt.resumable_split_frame_reservations, 1);
                    assert_eq!(receipt.resumable_join_scratch_reservations, 1);
                    assert!(
                        receipt.maximum_resumable_splice_total_scratch_bytes
                            >= receipt.maximum_resumable_splice_total_requested_bytes
                    );
                }
                let abort = session.begin_abort().unwrap();
                while !arena.poll_build_abort(abort, 1).unwrap().complete {}
                drop(splice);
                reclaim_all(&mut arena);
                assert_eq!(arena.metrics().live_nodes, baseline);
                assert!(arena.contains(old_root));
            }
        }

        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn resumable_owned_splice_inserts_and_replaces_at_every_leaf_boundary() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 17);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let base = sequence.as_ref();
        let old_root = base.root_id().unwrap();
        let expected = (0..17)
            .map(|index| base.locate_leaf(&arena, index).unwrap().unwrap())
            .collect::<Vec<_>>();
        let baseline = arena.metrics().live_nodes;

        for (range_length, boundaries) in [(0_u64, 0_u64..=17), (1_u64, 0_u64..=16)] {
            for index in boundaries {
                let ticket = arena.begin_build().unwrap();
                let mut session = arena.resume_build(ticket).unwrap();
                let working = session.retain(old_root).unwrap();
                let (replacement, _) = session.allocate(&[LEAF_TAG], &[]).unwrap();
                let replacement_id = session.owner_id(&replacement).unwrap();
                let mut receipt = SequenceMutationReceipt::default();
                let preseeded_unrelated_split_peak = range_length == 0 && index == 0;
                if preseeded_unrelated_split_peak {
                    receipt.maximum_resumable_split_total_requested_bytes = usize::MAX;
                    receipt.maximum_resumable_split_total_scratch_bytes = usize::MAX;
                }
                let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
                    &session,
                    Some(working),
                    index..index + range_length,
                    Some(replacement),
                    &mut receipt,
                )
                .unwrap();
                let ticket = session.suspend().unwrap();
                let ticket =
                    finish_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
                let session = arena.resume_build(ticket).unwrap();
                let output = splice.take_root().unwrap().unwrap();
                assert_eq!(session.live_owners().unwrap(), 1);
                let mut expected_ids = expected[..usize::try_from(index).unwrap()].to_vec();
                expected_ids.push(replacement_id);
                expected_ids
                    .extend_from_slice(&expected[usize::try_from(index + range_length).unwrap()..]);
                assert_eq!(owner_leaf_ids(&session, &output), expected_ids);
                if range_length == 0 && index == 0 {
                    assert_eq!(
                        owner_leaf_ids(&session, &output).first(),
                        Some(&replacement_id)
                    );
                }
                if range_length == 0 && index == 17 {
                    assert_eq!(
                        owner_leaf_ids(&session, &output).last(),
                        Some(&replacement_id)
                    );
                }
                if preseeded_unrelated_split_peak {
                    assert_ne!(
                        receipt.maximum_resumable_splice_total_requested_bytes,
                        usize::MAX
                    );
                    assert_ne!(
                        receipt.maximum_resumable_splice_total_scratch_bytes,
                        usize::MAX
                    );
                }
                assert_eq!(receipt.leaves_adopted, 1);
                assert_eq!(
                    receipt.leaves_reused,
                    17 - usize::try_from(range_length).unwrap()
                );
                assert_eq!(
                    receipt.resumable_splice_deleted_roots_released,
                    usize::from(range_length != 0)
                );
                assert_eq!(receipt.resumable_split_frame_reservations, 1);
                assert_eq!(receipt.resumable_join_scratch_reservations, 1);
                assert!(
                    receipt.resumable_join_scratch_reservations < receipt.resumable_splice_polls
                );
                let abort = session.begin_abort().unwrap();
                while !arena.poll_build_abort(abort, 1).unwrap().complete {}
                drop(splice);
                reclaim_all(&mut arena);
                assert_eq!(arena.metrics().live_nodes, baseline);
            }
        }

        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one shared arena fixture verifies the splice identity edge-case matrix"
    )]
    fn owned_splice_handles_empty_full_aligned_and_multileaf_replacement_without_identity_fiction()
    {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 16);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let (_, SequenceNodeKind::Branch { left, .. }) =
            sequence_node::<TestSpec>(&arena, old_root).unwrap()
        else {
            panic!("aligned fixture must have a branch root");
        };
        let replacement_initial = leaves(&mut arena, 3);
        let replacement_sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            replacement_initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let replacement_root = replacement_sequence.as_ref().root_id().unwrap();
        let replacement_ids = (0..3)
            .map(|index| {
                replacement_sequence
                    .as_ref()
                    .locate_leaf(&arena, index)
                    .unwrap()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let old_ids = (0..16)
            .map(|index| {
                sequence
                    .as_ref()
                    .locate_leaf(&arena, index)
                    .unwrap()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let baseline = arena.metrics().live_nodes;

        // An aligned suffix deletion returns the exact existing left subtree.
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let working = session.retain(old_root).unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
            &session,
            Some(working),
            8..16,
            None,
            &mut receipt,
        )
        .unwrap();
        let ticket = session.suspend().unwrap();
        let ticket = finish_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
        let session = arena.resume_build(ticket).unwrap();
        let output = splice.take_root().unwrap().unwrap();
        assert_eq!(session.owner_id(&output).unwrap(), left);
        assert_eq!(receipt.branches_allocated, 0);
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(splice);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);

        // Full replacement bypasses both split and join and preserves the
        // replacement root's exact identity.
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let working = session.retain(old_root).unwrap();
        let replacement = session.retain(replacement_root).unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
            &session,
            Some(working),
            0..16,
            Some(replacement),
            &mut receipt,
        )
        .unwrap();
        let ticket = session.suspend().unwrap();
        let ticket = finish_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
        let session = arena.resume_build(ticket).unwrap();
        let output = splice.take_root().unwrap().unwrap();
        assert_eq!(session.owner_id(&output).unwrap(), replacement_root);
        assert_eq!(receipt.branches_allocated, 0);
        assert_eq!(receipt.resumable_split_frame_reservations, 0);
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(splice);
        reclaim_all(&mut arena);

        // An empty working sequence accepts either an exact replacement root
        // or remains empty without manufacturing an arena page.
        for has_replacement in [false, true] {
            let ticket = arena.begin_build().unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            let replacement = has_replacement.then(|| session.retain(replacement_root).unwrap());
            let mut receipt = SequenceMutationReceipt::default();
            let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
                &session,
                None,
                0..0,
                replacement,
                &mut receipt,
            )
            .unwrap();
            let ticket = session.suspend().unwrap();
            let ticket = finish_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
            let session = arena.resume_build(ticket).unwrap();
            let output = splice.take_root().unwrap();
            assert_eq!(
                output
                    .as_ref()
                    .map(|owner| session.owner_id(owner).unwrap()),
                has_replacement.then_some(replacement_root)
            );
            assert_eq!(receipt.branches_allocated, 0);
            let abort = session.begin_abort().unwrap();
            while !arena.poll_build_abort(abort, 1).unwrap().complete {}
            drop(splice);
            reclaim_all(&mut arena);
        }

        // A non-aligned multileaf replacement keeps every untouched leaf ID.
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let working = session.retain(old_root).unwrap();
        let replacement = session.retain(replacement_root).unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
            &session,
            Some(working),
            5..7,
            Some(replacement),
            &mut receipt,
        )
        .unwrap();
        let ticket = session.suspend().unwrap();
        let ticket = finish_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
        let session = arena.resume_build(ticket).unwrap();
        let output = splice.take_root().unwrap().unwrap();
        let mut expected = old_ids[..5].to_vec();
        expected.extend_from_slice(&replacement_ids);
        expected.extend_from_slice(&old_ids[7..]);
        assert_eq!(owner_leaf_ids(&session, &output), expected);
        assert_eq!(receipt.resumable_split_frame_reservations, 1);
        assert_eq!(receipt.resumable_join_scratch_reservations, 1);
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(splice);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);

        replacement_sequence.release_later(&mut arena).unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn every_small_and_awkward_split_index_preserves_exact_identity_and_avl_shape() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        for count in (1_usize..=17).chain([31, 32, 33, 63, 64, 65]) {
            let mut arena = PageArena::new();
            let initial = leaves(&mut arena, count);
            let sequence = PersistentSequence::<TestSpec>::from_leaves(
                &mut arena,
                initial,
                &mut SequenceMutationReceipt::default(),
            )
            .unwrap();
            let base = sequence.as_ref();
            let old_root = base.root_id().unwrap();
            let expected = (0..u64::try_from(count).unwrap())
                .map(|index| base.locate_leaf(&arena, index).unwrap().unwrap())
                .collect::<Vec<_>>();
            let baseline = arena.metrics().live_nodes;

            for index in 0..=u64::try_from(count).unwrap() {
                let mut ticket = arena.begin_build().unwrap();
                let mut receipt = SequenceMutationReceipt::default();
                let mut split = ResumableSequenceSplit::<TestSpec>::try_new(
                    &ticket,
                    &arena,
                    old_root,
                    index,
                    &mut receipt,
                )
                .unwrap();
                let abort = loop {
                    let mut session = arena.resume_build(ticket).unwrap();
                    let allocations_before = receipt.branches_allocated;
                    match split.poll(&mut session, &mut receipt).unwrap() {
                        ResumableSequenceSplitProgress::Pending => {
                            assert!(receipt.branches_allocated - allocations_before <= 1);
                            ticket = session.suspend().unwrap();
                        }
                        ResumableSequenceSplitProgress::Complete => {
                            assert!(receipt.branches_allocated - allocations_before <= 1);
                            let (prefix, suffix) = split.take_parts().unwrap();
                            let mut prefix_ids = Vec::new();
                            if let Some(prefix) = prefix.as_ref() {
                                let prefix_id = session.owner_id(prefix).unwrap();
                                assert_avl_and_collect(session.arena(), prefix_id, &mut prefix_ids);
                                if index == u64::try_from(count).unwrap() {
                                    assert_eq!(prefix_id, old_root);
                                }
                            }
                            let mut suffix_ids = Vec::new();
                            if let Some(suffix) = suffix.as_ref() {
                                let suffix_id = session.owner_id(suffix).unwrap();
                                assert_avl_and_collect(session.arena(), suffix_id, &mut suffix_ids);
                                if index == 0 {
                                    assert_eq!(suffix_id, old_root);
                                }
                            }
                            let split_index = usize::try_from(index).unwrap();
                            assert_eq!(prefix_ids, expected[..split_index]);
                            assert_eq!(suffix_ids, expected[split_index..]);
                            if index == 0 || index == u64::try_from(count).unwrap() {
                                assert_eq!(receipt.branches_allocated, 0);
                            }
                            break session.begin_abort().unwrap();
                        }
                    }
                };
                while !arena.poll_build_abort(abort, 1).unwrap().complete {}
                drop(split);
                reclaim_all(&mut arena);
                assert_eq!(arena.metrics().live_nodes, baseline);
                assert!(arena.contains(old_root));
            }

            sequence.release_later(&mut arena).unwrap();
            reclaim_all(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Builds four distinct corrupt-shape witnesses explicitly.
    fn split_rejects_summary_controlled_scratch_and_non_avl_shapes() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let mut owners = Vec::new();
        for _ in 0..5 {
            owners.push(arena.allocate(&[LEAF_TAG], &[]).unwrap().owner);
        }
        let leaf_ids = owners.iter().map(OwnedArenaRef::id).collect::<Vec<_>>();

        let high = TestSpec::encode_branch(TestSummary {
            leaves: 2,
            height: u16::MAX,
        });
        owners.push(
            arena
                .allocate(&high, &[leaf_ids[0], leaf_ids[1]])
                .unwrap()
                .owner,
        );
        let low = TestSpec::encode_branch(TestSummary {
            leaves: 2,
            height: 1,
        });
        owners.push(
            arena
                .allocate(&low, &[leaf_ids[0], leaf_ids[1]])
                .unwrap()
                .owner,
        );
        let inconsistent = TestSpec::encode_branch(TestSummary {
            leaves: 3,
            height: 2,
        });
        owners.push(
            arena
                .allocate(&inconsistent, &[leaf_ids[0], leaf_ids[1]])
                .unwrap()
                .owner,
        );

        let inner = TestSpec::encode_branch(TestSummary {
            leaves: 2,
            height: 2,
        });
        owners.push(
            arena
                .allocate(&inner, &[leaf_ids[0], leaf_ids[1]])
                .unwrap()
                .owner,
        );
        let first_inner_id = owners.last().unwrap().id();
        owners.push(
            arena
                .allocate(&inner, &[leaf_ids[2], leaf_ids[3]])
                .unwrap()
                .owner,
        );
        let second_inner_id = owners.last().unwrap().id();
        let tall = TestSpec::encode_branch(TestSummary {
            leaves: 4,
            height: 3,
        });
        owners.push(
            arena
                .allocate(&tall, &[first_inner_id, second_inner_id])
                .unwrap()
                .owner,
        );
        let tall_id = owners.last().unwrap().id();
        let non_avl = TestSpec::encode_branch(TestSummary {
            leaves: 5,
            height: 4,
        });
        owners.push(
            arena
                .allocate(&non_avl, &[tall_id, leaf_ids[4]])
                .unwrap()
                .owner,
        );

        let corrupt_roots = [
            owners[5].id(),
            owners[6].id(),
            owners[7].id(),
            owners[11].id(),
        ];
        for (index, root) in corrupt_roots.into_iter().enumerate() {
            let ticket = arena.begin_build().unwrap();
            let error = ResumableSequenceSplit::<TestSpec>::try_new(
                &ticket,
                &arena,
                root,
                1,
                &mut SequenceMutationReceipt::default(),
            )
            .expect_err("corrupt summary must fail before scratch is trusted");
            match index {
                0 => assert_eq!(
                    error,
                    TestError::Invalid("sequence split root has invalid AVL summary")
                ),
                _ => assert_eq!(
                    error,
                    TestError::Invalid("sequence branch has invalid split summary")
                ),
            }
            let abort = arena.begin_build_abort(ticket).unwrap();
            assert!(arena.poll_build_abort(abort, 0).unwrap().complete);
        }

        for owner in owners.into_iter().rev() {
            arena.release_later(owner).unwrap();
        }
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn split_journal_saturation_is_abortable_and_preserves_the_base() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let limits = crate::ArenaLimits::new(256, 1024 * 1024, 1, 2);
        let mut arena = PageArena::try_with_limits(limits).unwrap();
        let initial = leaves(&mut arena, 16);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let baseline = arena.metrics().live_nodes;
        let mut ticket = arena.begin_build().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut split =
            ResumableSequenceSplit::<TestSpec>::try_new(&ticket, &arena, old_root, 3, &mut receipt)
                .unwrap();

        let mut session = arena.resume_build(ticket).unwrap();
        assert_eq!(
            split.poll(&mut session, &mut receipt).unwrap(),
            ResumableSequenceSplitProgress::Pending
        );
        ticket = session.suspend().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        assert_eq!(
            split.poll(&mut session, &mut receipt),
            Err(TestError::ArenaBuild(ArenaBuildError::Arena(
                ArenaError::OwnerJournalLimitReached { limit: 2 }
            )))
        );
        assert_eq!(session.live_owners().unwrap(), 2);
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(split);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);
        assert!(arena.contains(old_root));

        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn split_can_cancel_after_every_poll_boundary_including_taken_output() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 16);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let baseline = arena.metrics().live_nodes;
        let mut covered_complete = false;

        for polls_before_abort in 0..64 {
            let mut ticket = arena.begin_build().unwrap();
            let mut receipt = SequenceMutationReceipt::default();
            let mut split = ResumableSequenceSplit::<TestSpec>::try_new(
                &ticket,
                &arena,
                old_root,
                5,
                &mut receipt,
            )
            .unwrap();
            let mut complete = false;
            for _ in 0..polls_before_abort {
                let mut session = arena.resume_build(ticket).unwrap();
                let progress = split.poll(&mut session, &mut receipt).unwrap();
                if progress == ResumableSequenceSplitProgress::Complete {
                    let _parts = split.take_parts().unwrap();
                    complete = true;
                }
                ticket = session.suspend().unwrap();
                if complete {
                    break;
                }
            }
            let abort = arena.begin_build_abort(ticket).unwrap();
            while !arena.poll_build_abort(abort, 1).unwrap().complete {}
            drop(split);
            reclaim_all(&mut arena);
            assert_eq!(arena.metrics().live_nodes, baseline);
            assert!(arena.contains(old_root));
            if complete {
                covered_complete = true;
                break;
            }
        }
        assert!(
            covered_complete,
            "poll-boundary cancellation never reached output"
        );

        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn split_job_rejects_wrong_build_wrong_arena_and_reused_generation() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 8);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let root = sequence.as_ref().root_id().unwrap();

        let expected_ticket = arena.begin_build().unwrap();
        let expected_build = expected_ticket.id();
        let mut receipt = SequenceMutationReceipt::default();
        let mut split = ResumableSequenceSplit::<TestSpec>::try_new(
            &expected_ticket,
            &arena,
            root,
            3,
            &mut receipt,
        )
        .unwrap();
        assert_eq!(split.build_id(), expected_build);

        let wrong_ticket = arena.begin_build().unwrap();
        let mut wrong_session = arena.resume_build(wrong_ticket).unwrap();
        assert_eq!(
            split.poll(&mut wrong_session, &mut receipt),
            Err(TestError::Invalid(
                "resumable sequence job belongs to another build generation"
            ))
        );
        assert_eq!(wrong_session.live_owners().unwrap(), 0);
        let wrong_abort = wrong_session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(wrong_abort, 0).unwrap().complete);

        let mut other_arena = PageArena::new();
        let foreign_ticket = other_arena.begin_build().unwrap();
        assert!(matches!(
            ResumableSequenceSplit::<TestSpec>::try_new(
                &foreign_ticket,
                &arena,
                root,
                3,
                &mut SequenceMutationReceipt::default(),
            ),
            Err(TestError::ArenaBuild(ArenaBuildError::WrongArena(_)))
        ));
        let foreign_abort = other_arena.begin_build_abort(foreign_ticket).unwrap();
        assert!(
            other_arena
                .poll_build_abort(foreign_abort, 0)
                .unwrap()
                .complete
        );

        let old_abort = arena.begin_build_abort(expected_ticket).unwrap();
        assert!(arena.poll_build_abort(old_abort, 0).unwrap().complete);
        let reused_ticket = arena.begin_build().unwrap();
        assert_ne!(reused_ticket.id(), expected_build);
        let mut reused_session = arena.resume_build(reused_ticket).unwrap();
        assert_eq!(
            split.poll(&mut reused_session, &mut receipt),
            Err(TestError::Invalid(
                "resumable sequence job belongs to another build generation"
            ))
        );
        let reused_abort = reused_session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(reused_abort, 0).unwrap().complete);

        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn split_cancellation_and_join_failure_roll_back_without_touching_base_root() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 64);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let baseline = arena.metrics().live_nodes;

        let mut ticket = arena.begin_build().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut split =
            ResumableSequenceSplit::<TestSpec>::try_new(&ticket, &arena, old_root, 7, &mut receipt)
                .unwrap();
        let abort = loop {
            let mut session = arena.resume_build(ticket).unwrap();
            split.poll(&mut session, &mut receipt).unwrap();
            if receipt.branches_allocated != 0 {
                assert!(session.live_owners().unwrap() > 1);
                break session.begin_abort().unwrap();
            }
            ticket = session.suspend().unwrap();
        };
        let zero = arena.poll_build_abort(abort, 0).unwrap();
        assert_eq!(zero.owners_scheduled, 0);
        let mut remaining = zero.owners_remaining;
        while remaining != 0 {
            let step = arena.poll_build_abort(abort, 1).unwrap();
            assert!(step.owners_scheduled <= 1);
            assert!(step.owners_remaining < remaining);
            remaining = step.owners_remaining;
        }
        drop(split);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);
        assert!(arena.contains(old_root));

        let mut ticket = arena.begin_build().unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut split =
            ResumableSequenceSplit::<TestSpec>::try_new(&ticket, &arena, old_root, 7, &mut receipt)
                .unwrap();
        FAIL_AT_LEAVES.store(8, AtomicOrdering::Relaxed);
        let abort = loop {
            let mut session = arena.resume_build(ticket).unwrap();
            match split.poll(&mut session, &mut receipt) {
                Ok(ResumableSequenceSplitProgress::Pending) => {
                    ticket = session.suspend().unwrap();
                }
                Ok(ResumableSequenceSplitProgress::Complete) => {
                    panic!("forced split join failure did not fire")
                }
                Err(TestError::Invalid("forced test branch failure")) => {
                    break session.begin_abort().unwrap();
                }
                Err(error) => panic!("unexpected split failure: {error:?}"),
            }
        };
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(split);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);
        assert!(arena.contains(old_root));

        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn owned_splice_can_cancel_after_every_phase_without_touching_base_or_replacement() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 32);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let replacement_initial = leaves(&mut arena, 3);
        let replacement = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            replacement_initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let replacement_root = replacement.as_ref().root_id().unwrap();
        let baseline = arena.metrics().live_nodes;
        let mut reached_output = false;

        for polls_before_abort in 0..160 {
            let ticket = arena.begin_build().unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            let working = session.retain(old_root).unwrap();
            let replacement_owner = session.retain(replacement_root).unwrap();
            let mut receipt = SequenceMutationReceipt::default();
            let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
                &session,
                Some(working),
                5..11,
                Some(replacement_owner),
                &mut receipt,
            )
            .unwrap();
            let mut ticket = session.suspend().unwrap();
            let mut complete = false;
            for _ in 0..polls_before_abort {
                let (next, progress) =
                    poll_splice_with_suspend(&mut arena, ticket, &mut splice, &mut receipt);
                ticket = next;
                if progress == ResumableSequenceSplitProgress::Complete {
                    let session = arena.resume_build(ticket).unwrap();
                    let _output = splice.take_root().unwrap();
                    ticket = session.suspend().unwrap();
                    complete = true;
                    break;
                }
            }
            abort_suspended_build(&mut arena, ticket);
            drop(splice);
            reclaim_all(&mut arena);
            assert_eq!(arena.metrics().live_nodes, baseline);
            assert!(arena.contains(old_root));
            assert!(arena.contains(replacement_root));
            if complete {
                reached_output = true;
                break;
            }
        }
        assert!(
            reached_output,
            "phase-by-phase cancellation missed completion"
        );

        replacement.release_later(&mut arena).unwrap();
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn owned_splice_rejects_wrong_authority_and_recycled_build_generation() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 8);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();

        let first_ticket = arena.begin_build().unwrap();
        let mut first_session = arena.resume_build(first_ticket).unwrap();
        let working = first_session.retain(old_root).unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
            &first_session,
            Some(working),
            2..3,
            None,
            &mut receipt,
        )
        .unwrap();
        let first_build = splice.build_id();
        let first_ticket = first_session.suspend().unwrap();

        let wrong_ticket = arena.begin_build().unwrap();
        let mut wrong_session = arena.resume_build(wrong_ticket).unwrap();
        assert_eq!(
            splice.poll(&mut wrong_session, &mut receipt),
            Err(TestError::Invalid(
                "resumable sequence splice belongs to another build generation"
            ))
        );
        let wrong_abort = wrong_session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(wrong_abort, 0).unwrap().complete);

        let first_abort = arena.begin_build_abort(first_ticket).unwrap();
        while !arena.poll_build_abort(first_abort, 1).unwrap().complete {}
        reclaim_all(&mut arena);

        let reused_ticket = arena.begin_build().unwrap();
        assert_ne!(reused_ticket.id(), first_build);
        let mut reused_session = arena.resume_build(reused_ticket).unwrap();
        assert_eq!(
            splice.poll(&mut reused_session, &mut receipt),
            Err(TestError::Invalid(
                "resumable sequence splice belongs to another build generation"
            ))
        );
        let reused_abort = reused_session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(reused_abort, 0).unwrap().complete);
        drop(splice);

        let owner_ticket = arena.begin_build().unwrap();
        let mut owner_session = arena.resume_build(owner_ticket).unwrap();
        let foreign_owner = owner_session.retain(old_root).unwrap();
        let owner_ticket = owner_session.suspend().unwrap();
        let target_ticket = arena.begin_build().unwrap();
        let target_session = arena.resume_build(target_ticket).unwrap();
        assert!(matches!(
            ResumableSequenceSplice::<TestSpec>::try_from_owned(
                &target_session,
                Some(foreign_owner),
                0..0,
                None,
                &mut SequenceMutationReceipt::default(),
            ),
            Err(TestError::ArenaBuild(
                ArenaBuildError::CrossBuildOwner { .. }
            ))
        ));
        let target_abort = target_session.begin_abort().unwrap();
        assert!(arena.poll_build_abort(target_abort, 0).unwrap().complete);
        let owner_abort = arena.begin_build_abort(owner_ticket).unwrap();
        while !arena.poll_build_abort(owner_abort, 1).unwrap().complete {}
        reclaim_all(&mut arena);

        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn owned_splice_saturation_and_join_failure_are_fully_abortable() {
        let _guard = TEST_LOCK.lock().expect("sequence test lock");
        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        let limits = crate::ArenaLimits::new(512, 1024 * 1024, 2, 2);
        let mut arena = PageArena::try_with_limits(limits).unwrap();
        let initial = leaves(&mut arena, 32);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let baseline = arena.metrics().live_nodes;

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let working = session.retain(old_root).unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
            &session,
            Some(working),
            3..4,
            None,
            &mut receipt,
        )
        .unwrap();
        let ticket = session.suspend().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        assert_eq!(
            splice.poll(&mut session, &mut receipt),
            Err(TestError::ArenaBuild(ArenaBuildError::Arena(
                ArenaError::OwnerJournalLimitReached { limit: 2 }
            )))
        );
        assert_eq!(session.live_owners().unwrap(), 2);
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(splice);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);

        // A larger journal admits the split and then a forced summary-combine
        // failure. Recreate the arena because the configured limit is fixed.
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);

        let mut arena = PageArena::new();
        let initial = leaves(&mut arena, 32);
        let sequence = PersistentSequence::<TestSpec>::from_leaves(
            &mut arena,
            initial,
            &mut SequenceMutationReceipt::default(),
        )
        .unwrap();
        let old_root = sequence.as_ref().root_id().unwrap();
        let baseline = arena.metrics().live_nodes;
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let working = session.retain(old_root).unwrap();
        let mut receipt = SequenceMutationReceipt::default();
        let mut splice = ResumableSequenceSplice::<TestSpec>::try_from_owned(
            &session,
            Some(working),
            3..7,
            None,
            &mut receipt,
        )
        .unwrap();
        let mut ticket = session.suspend().unwrap();
        FAIL_AT_LEAVES.store(8, AtomicOrdering::Relaxed);
        let abort = loop {
            let mut session = arena.resume_build(ticket).unwrap();
            match splice.poll(&mut session, &mut receipt) {
                Ok(ResumableSequenceSplitProgress::Pending) => {
                    ticket = session.suspend().unwrap();
                }
                Ok(ResumableSequenceSplitProgress::Complete) => {
                    panic!("forced splice join failure did not fire")
                }
                Err(TestError::Invalid("forced test branch failure")) => {
                    break session.begin_abort().unwrap();
                }
                Err(error) => panic!("unexpected splice failure: {error:?}"),
            }
        };
        while !arena.poll_build_abort(abort, 1).unwrap().complete {}
        drop(splice);
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline);
        assert!(arena.contains(old_root));

        FAIL_AT_LEAVES.store(u64::MAX, AtomicOrdering::Relaxed);
        sequence.release_later(&mut arena).unwrap();
        reclaim_all(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}

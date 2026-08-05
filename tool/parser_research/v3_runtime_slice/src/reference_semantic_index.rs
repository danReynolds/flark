//! Private storage proof for reference-definition occurrence and winner state.
//!
//! This module intentionally owns both persistent roots. A parser can describe
//! a definition, but only this writer can append the occurrence, perform the
//! exact-label `insert_first`, and mint an acknowledgement after both roots
//! advance in the same unpublished arena build. The final manifest adopts both
//! roots before the arena journal can publish anything.

use std::fmt;

use crate::arena::{
    AllocationReceipt, ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession,
    ArenaError, ArenaId, OwnedArenaRef, PageArena,
};

const FORMAT_VERSION: u8 = 1;
const EMPTY_OCCURRENCE_TAG: u8 = 0xe1;
const OCCURRENCE_TAG: u8 = 0xe2;
const WINNER_VERSION_TAG: u8 = 0xe3;
const WINNER_BRANCH_TAG: u8 = 0xe4;
const WINNER_LEAF_TAG: u8 = 0xe5;
const SEMANTIC_INDEX_TAG: u8 = 0xe6;

const TRIE_LEVELS: usize = 16;
const TRIE_FANOUT: usize = 16;
const BINDING_U64S: usize = 18;
const BINDING_BYTES: usize = BINDING_U64S * 8;
const SLICE_U64S: usize = 12;
const SLICE_BYTES: usize = SLICE_U64S * 8;

const EMPTY_OCCURRENCE_BYTES: usize = 192;
const OCCURRENCE_BYTES: usize = 512;
const WINNER_VERSION_BYTES: usize = 208;
const WINNER_BRANCH_BYTES: usize = 32;
const WINNER_LEAF_BYTES: usize = 64;
const SEMANTIC_INDEX_BYTES: usize = 224;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DualMetric {
    bytes: u64,
    utf16: u64,
}

impl DualMetric {
    const fn new(bytes: u64, utf16: u64) -> Self {
        Self { bytes, utf16 }
    }

    const fn no_later_than(self, other: Self) -> bool {
        self.bytes <= other.bytes && self.utf16 <= other.utf16
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DualRange {
    start: DualMetric,
    end: DualMetric,
}

impl DualRange {
    const fn new(start: DualMetric, end: DualMetric) -> Self {
        Self { start, end }
    }

    const fn is_ordered(self) -> bool {
        self.start.no_later_than(self.end)
    }

    const fn contains(self, nested: Self) -> bool {
        self.start.no_later_than(nested.start) && nested.end.no_later_than(self.end)
    }
}

/// Immutable candidate coordinates already selected by the writer actor.
///
/// The type and every constructor remain private to this module. Production
/// wiring must move the exact writer/source admission into this same privacy
/// boundary rather than adding a scalar constructor in another crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CandidateReferenceBinding {
    authority_nonce: u64,
    candidate_generation: u64,
    semantic_epoch: u64,
    source_root: u64,
    source_revision: u64,
    source_extent: DualMetric,
    paragraph: u64,
    paragraph_source: DualRange,
    paragraph_logical_extent: DualMetric,
    projection_root: u64,
    projection_generation: u64,
    projection_runs: u64,
    interner_generation: u64,
}

impl CandidateReferenceBinding {
    fn validate(self) -> Result<(), ReferenceSemanticIndexError> {
        if self.authority_nonce == 0
            || self.candidate_generation == 0
            || self.source_root == 0
            || self.paragraph == 0
            || self.projection_root == 0
            || self.projection_generation == 0
            || self.interner_generation == 0
            || !self.paragraph_source.is_ordered()
            || !self.paragraph_source.end.no_later_than(self.source_extent)
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference candidate binding is incomplete or out of bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StoredReferenceSlice {
    source: DualRange,
    logical: DualRange,
    projection_run_start: u64,
    projection_run_end: u64,
    projection_program_root: u64,
    projection_program_generation: u64,
}

/// Non-cloneable slice capability minted by the candidate's source authority.
#[derive(Debug, PartialEq, Eq)]
struct WriterBoundReferenceSlice {
    authority_nonce: u64,
    value: StoredReferenceSlice,
}

/// Exact interner identity, not a caller-provided hash.
#[derive(Debug, PartialEq, Eq)]
struct WriterInternedReferenceLabel {
    authority_nonce: u64,
    interner_generation: u64,
    id: u64,
}

/// The only local mint for labels and source/projection slices. In production
/// this object is created by the writer after active-Paragraph admission.
#[derive(Debug)]
struct CandidateReferenceAuthority {
    binding: CandidateReferenceBinding,
}

impl CandidateReferenceAuthority {
    fn bind_label(
        &self,
        exact_interned_id: u64,
    ) -> Result<WriterInternedReferenceLabel, ReferenceSemanticIndexError> {
        if exact_interned_id == 0 {
            return Err(ReferenceSemanticIndexError::Invalid(
                "normalized reference label ID is zero",
            ));
        }
        Ok(WriterInternedReferenceLabel {
            authority_nonce: self.binding.authority_nonce,
            interner_generation: self.binding.interner_generation,
            id: exact_interned_id,
        })
    }

    fn bind_slice(
        &self,
        source: DualRange,
        logical: DualRange,
        projection_runs: (u64, u64),
    ) -> Result<WriterBoundReferenceSlice, ReferenceSemanticIndexError> {
        if !source.is_ordered()
            || !logical.is_ordered()
            || !self.binding.paragraph_source.contains(source)
            || !logical
                .end
                .no_later_than(self.binding.paragraph_logical_extent)
            || projection_runs.0 > projection_runs.1
            || projection_runs.1 > self.binding.projection_runs
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference slice escaped its admitted source/projection bounds",
            ));
        }
        Ok(WriterBoundReferenceSlice {
            authority_nonce: self.binding.authority_nonce,
            value: StoredReferenceSlice {
                source,
                logical,
                projection_run_start: projection_runs.0,
                projection_run_end: projection_runs.1,
                projection_program_root: self.binding.projection_root,
                projection_program_generation: self.binding.projection_generation,
            },
        })
    }

    #[allow(clippy::needless_pass_by_value)] // Consumes non-cloneable writer capabilities.
    fn definition(
        &self,
        label: WriterInternedReferenceLabel,
        definition: WriterBoundReferenceSlice,
        destination: WriterBoundReferenceSlice,
        title: Option<WriterBoundReferenceSlice>,
    ) -> Result<ReferenceOccurrenceDraft, ReferenceSemanticIndexError> {
        let authority = self.binding.authority_nonce;
        if label.authority_nonce != authority
            || label.interner_generation != self.binding.interner_generation
            || definition.authority_nonce != authority
            || destination.authority_nonce != authority
            || title
                .as_ref()
                .is_some_and(|slice| slice.authority_nonce != authority)
            || !definition.value.source.contains(destination.value.source)
            || !definition.value.logical.contains(destination.value.logical)
            || title.as_ref().is_some_and(|slice| {
                !definition.value.source.contains(slice.value.source)
                    || !definition.value.logical.contains(slice.value.logical)
            })
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference draft crossed writer authority or definition bounds",
            ));
        }
        Ok(ReferenceOccurrenceDraft {
            authority_nonce: authority,
            label_id: label.id,
            definition: definition.value,
            destination: destination.value,
            title: title.map(|slice| slice.value),
        })
    }
}

#[derive(Debug)]
struct ReferenceOccurrenceDraft {
    authority_nonce: u64,
    label_id: u64,
    definition: StoredReferenceSlice,
    destination: StoredReferenceSlice,
    title: Option<StoredReferenceSlice>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ReferenceSemanticIndexReceipt {
    mutations: u64,
    nodes_allocated: u64,
    owners_released: u64,
    payload_bytes_copied: u64,
    edge_bytes_copied: u64,
    maximum_page_payload_bytes: usize,
    maximum_live_owners_observed: usize,
}

impl ReferenceSemanticIndexReceipt {
    fn record_allocation(
        &mut self,
        allocation: AllocationReceipt,
    ) -> Result<(), ReferenceSemanticIndexError> {
        self.nodes_allocated = checked_add(self.nodes_allocated, 1, "index node count")?;
        self.payload_bytes_copied = checked_add(
            self.payload_bytes_copied,
            u64::try_from(allocation.payload_bytes_copied)
                .map_err(|_| ReferenceSemanticIndexError::Overflow("payload byte count"))?,
            "payload byte count",
        )?;
        self.edge_bytes_copied = checked_add(
            self.edge_bytes_copied,
            u64::try_from(allocation.edge_bytes_copied)
                .map_err(|_| ReferenceSemanticIndexError::Overflow("edge byte count"))?,
            "edge byte count",
        )?;
        self.maximum_page_payload_bytes = self
            .maximum_page_payload_bytes
            .max(allocation.payload_bytes_copied);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceSemanticIndexProgress {
    Pending,
    ReadyForItem,
    ItemAckReady,
    TerminalSealReady,
    Committable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BuildPhase {
    BootstrapOccurrence,
    BootstrapWinner,
    Ready,
    PlanWinner,
    AllocateOccurrence,
    ReleasePreviousOccurrence,
    AllocateTrieLeaf,
    AllocateTrieBranch,
    ReleaseTrieChild,
    AllocateWinnerVersion,
    ReleaseTrieRoot,
    ReleasePreviousWinner,
    ItemAckReady,
    AwaitingItemJoin,
    AllocateTerminal,
    ReleaseTerminalOccurrence,
    ReleaseTerminalWinner,
    TerminalSealReady,
    AwaitingTerminalJoin,
    Committable,
    Failed,
}

#[derive(Debug)]
struct PendingOccurrence {
    draft: ReferenceOccurrenceDraft,
    ordinal: u64,
    prior_occurrence_root: ArenaId,
    prior_winner_root: ArenaId,
    plan_depth: usize,
    plan_cursor: Option<ArenaId>,
    path_nodes: [Option<ArenaId>; TRIE_LEVELS],
    existing_winner_ordinal: Option<u64>,
    new_occurrence: Option<ArenaBuildOwner>,
    new_occurrence_id: Option<ArenaId>,
    new_trie: Option<ArenaBuildOwner>,
    new_trie_id: Option<ArenaId>,
    pending_trie_parent: Option<ArenaBuildOwner>,
    pending_trie_parent_id: Option<ArenaId>,
    trie_root_id: Option<ArenaId>,
    build_depth: usize,
    new_winner: Option<ArenaBuildOwner>,
    new_winner_id: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ItemAckInner {
    build: ArenaBuildId,
    authority_nonce: u64,
    candidate_generation: u64,
    ordinal: u64,
    count: u64,
    high_water: u64,
    occurrence_generation: u64,
    winner_generation: u64,
    previous_occurrence_root: ArenaId,
    occurrence_root: ArenaId,
    previous_winner_root: ArenaId,
    winner_root: ArenaId,
    inserted_first: bool,
    winner_ordinal: u64,
}

#[derive(Debug)]
#[must_use = "the writer-owned reference item acknowledgement must be joined"]
struct ReferenceIndexItemAck {
    inner: Option<ItemAckInner>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalSealInner {
    build: ArenaBuildId,
    authority_nonce: u64,
    candidate_generation: u64,
    count: u64,
    high_water: u64,
    occurrence_generation: u64,
    winner_generation: u64,
    winner_entries: u64,
    occurrence_root: ArenaId,
    winner_root: ArenaId,
    manifest: ArenaId,
}

#[derive(Debug)]
#[must_use = "the terminal reference-index seal must be joined before commit"]
struct ReferenceIndexTerminalSeal {
    inner: Option<TerminalSealInner>,
}

#[derive(Debug)]
struct ReferenceIndexAckJoiner {
    build: ArenaBuildId,
    authority_nonce: u64,
    candidate_generation: u64,
    next_ordinal: u64,
    joined_count: u64,
    last_occurrence_root: Option<ArenaId>,
    last_winner_root: Option<ArenaId>,
    terminal_joined: bool,
}

impl ReferenceIndexAckJoiner {
    fn new(build: ArenaBuildId, binding: CandidateReferenceBinding) -> Self {
        Self {
            build,
            authority_nonce: binding.authority_nonce,
            candidate_generation: binding.candidate_generation,
            next_ordinal: 1,
            joined_count: 0,
            last_occurrence_root: None,
            last_winner_root: None,
            terminal_joined: false,
        }
    }

    fn join_item(
        &mut self,
        builder: &mut ReferenceSemanticIndexBuilder,
        ack: &mut ReferenceIndexItemAck,
    ) -> Result<(), ReferenceSemanticIndexError> {
        let inner = ack
            .inner
            .as_ref()
            .ok_or(ReferenceSemanticIndexError::ReplayedAck)?;
        if inner.build != self.build
            || inner.authority_nonce != self.authority_nonce
            || inner.candidate_generation != self.candidate_generation
        {
            return Err(ReferenceSemanticIndexError::CrossedAck);
        }
        if inner.ordinal != self.next_ordinal {
            return Err(ReferenceSemanticIndexError::OutOfOrderAck {
                expected: self.next_ordinal,
                actual: inner.ordinal,
            });
        }
        builder.validate_pending_item_ack(inner)?;
        if inner.count != inner.ordinal
            || inner.high_water != inner.ordinal
            || inner.occurrence_generation != inner.ordinal
            || inner.winner_generation != inner.ordinal
            || inner.occurrence_root == inner.previous_occurrence_root
            || inner.winner_root == inner.previous_winner_root
            || self
                .last_occurrence_root
                .is_some_and(|root| root == inner.occurrence_root)
            || self
                .last_winner_root
                .is_some_and(|root| root == inner.winner_root)
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference acknowledgement did not advance both roots exactly once",
            ));
        }
        let joined = ack
            .inner
            .take()
            .ok_or(ReferenceSemanticIndexError::ReplayedAck)?;
        builder.finish_item_ack_join(joined)?;
        self.joined_count = joined.count;
        self.next_ordinal = joined
            .ordinal
            .checked_add(1)
            .ok_or(ReferenceSemanticIndexError::Overflow("ack ordinal"))?;
        self.last_occurrence_root = Some(joined.occurrence_root);
        self.last_winner_root = Some(joined.winner_root);
        Ok(())
    }

    fn join_terminal(
        &mut self,
        builder: &mut ReferenceSemanticIndexBuilder,
        seal: &mut ReferenceIndexTerminalSeal,
    ) -> Result<(), ReferenceSemanticIndexError> {
        let inner = seal
            .inner
            .as_ref()
            .ok_or(ReferenceSemanticIndexError::ReplayedAck)?;
        if inner.build != self.build
            || inner.authority_nonce != self.authority_nonce
            || inner.candidate_generation != self.candidate_generation
        {
            return Err(ReferenceSemanticIndexError::CrossedAck);
        }
        if self.terminal_joined
            || inner.count != self.joined_count
            || inner.high_water != self.joined_count
            || inner.occurrence_generation != self.joined_count
            || inner.winner_generation != self.joined_count
            || self.next_ordinal
                != self
                    .joined_count
                    .checked_add(1)
                    .ok_or(ReferenceSemanticIndexError::Overflow("terminal ordinal"))?
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "terminal seal disagrees with joined item high-water",
            ));
        }
        builder.validate_pending_terminal_seal(inner)?;
        let joined = seal
            .inner
            .take()
            .ok_or(ReferenceSemanticIndexError::ReplayedAck)?;
        builder.finish_terminal_join(joined)?;
        self.terminal_joined = true;
        Ok(())
    }
}

#[derive(Debug)]
struct ReferenceSemanticIndexBuilder {
    build: ArenaBuildId,
    binding: CandidateReferenceBinding,
    phase: BuildPhase,
    occurrence_root: Option<ArenaBuildOwner>,
    occurrence_root_id: Option<ArenaId>,
    winner_root: Option<ArenaBuildOwner>,
    winner_root_id: Option<ArenaId>,
    winner_trie: Option<ArenaId>,
    terminal_root: Option<ArenaBuildOwner>,
    count: u64,
    high_water: u64,
    winner_entries: u64,
    pending: Option<PendingOccurrence>,
    pending_item_ack: Option<ItemAckInner>,
    pending_terminal_seal: Option<TerminalSealInner>,
    fault_after_mutation: Option<u64>,
    receipt: ReferenceSemanticIndexReceipt,
}

impl ReferenceSemanticIndexBuilder {
    fn begin(
        build: ArenaBuildId,
        binding: CandidateReferenceBinding,
    ) -> Result<Self, ReferenceSemanticIndexError> {
        binding.validate()?;
        Ok(Self {
            build,
            binding,
            phase: BuildPhase::BootstrapOccurrence,
            occurrence_root: None,
            occurrence_root_id: None,
            winner_root: None,
            winner_root_id: None,
            winner_trie: None,
            terminal_root: None,
            count: 0,
            high_water: 0,
            winner_entries: 0,
            pending: None,
            pending_item_ack: None,
            pending_terminal_seal: None,
            fault_after_mutation: None,
            receipt: ReferenceSemanticIndexReceipt::default(),
        })
    }

    fn with_fault_after_mutation(mut self, mutation: u64) -> Self {
        self.fault_after_mutation = Some(mutation);
        self
    }

    const fn receipt(&self) -> ReferenceSemanticIndexReceipt {
        self.receipt
    }

    fn begin_item(
        &mut self,
        draft: ReferenceOccurrenceDraft,
    ) -> Result<(), ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::Ready
            || self.pending.is_some()
            || self.pending_item_ack.is_some()
            || draft.authority_nonce != self.binding.authority_nonce
            || draft.label_id == 0
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference index is not ready for this occurrence",
            ));
        }
        let ordinal = self
            .high_water
            .checked_add(1)
            .ok_or(ReferenceSemanticIndexError::Overflow("occurrence ordinal"))?;
        let prior_occurrence_root =
            self.occurrence_root_id
                .ok_or(ReferenceSemanticIndexError::Corrupt(
                    "occurrence root identity disappeared",
                ))?;
        let prior_winner_root = self
            .winner_root_id
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "winner root identity disappeared",
            ))?;
        self.pending = Some(PendingOccurrence {
            draft,
            ordinal,
            prior_occurrence_root,
            prior_winner_root,
            plan_depth: 0,
            plan_cursor: self.winner_trie,
            path_nodes: [None; TRIE_LEVELS],
            existing_winner_ordinal: None,
            new_occurrence: None,
            new_occurrence_id: None,
            new_trie: None,
            new_trie_id: None,
            pending_trie_parent: None,
            pending_trie_parent_id: None,
            trie_root_id: None,
            build_depth: TRIE_LEVELS - 1,
            new_winner: None,
            new_winner_id: None,
        });
        self.phase = BuildPhase::PlanWinner;
        Ok(())
    }

    fn begin_terminal(&mut self) -> Result<(), ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::Ready
            || self.pending.is_some()
            || self.pending_item_ack.is_some()
            || self.pending_terminal_seal.is_some()
        {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference index is not ready for terminal sealing",
            ));
        }
        self.phase = BuildPhase::AllocateTerminal;
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // One explicit fuelled mutation state machine.
    fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ReferenceSemanticIndexProgress, ReferenceSemanticIndexError> {
        self.ensure_session(session)?;
        match self.phase {
            BuildPhase::BootstrapOccurrence => {
                let payload = encode_empty_occurrence(self.binding);
                let (owner, allocation) = session.allocate_packed(&payload, &[])?;
                let owner_id = session.owner_id(&owner)?;
                self.receipt.record_allocation(allocation)?;
                self.occurrence_root = Some(owner);
                self.occurrence_root_id = Some(owner_id);
                self.phase = BuildPhase::BootstrapWinner;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::BootstrapWinner => {
                let payload = encode_winner_version(self.binding, 0, 0, 0, 0, None);
                let (owner, allocation) = session.allocate_packed(&payload, &[])?;
                let owner_id = session.owner_id(&owner)?;
                self.receipt.record_allocation(allocation)?;
                self.winner_root = Some(owner);
                self.winner_root_id = Some(owner_id);
                self.phase = BuildPhase::Ready;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::ReadyForItem)
            }
            BuildPhase::Ready => Ok(ReferenceSemanticIndexProgress::ReadyForItem),
            BuildPhase::PlanWinner => {
                self.poll_winner_plan(session.arena())?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::AllocateOccurrence => {
                let pending = self.pending_ref()?;
                let payload = encode_occurrence(self.binding, pending);
                let previous =
                    self.occurrence_root_id
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "occurrence root identity disappeared",
                        ))?;
                if previous != pending.prior_occurrence_root {
                    return Err(ReferenceSemanticIndexError::Corrupt(
                        "occurrence root changed during item planning",
                    ));
                }
                let (owner, allocation) = session.allocate_packed(&payload, &[previous])?;
                let owner_id = session.owner_id(&owner)?;
                self.receipt.record_allocation(allocation)?;
                let pending = self.pending_mut()?;
                pending.new_occurrence = Some(owner);
                pending.new_occurrence_id = Some(owner_id);
                self.phase = BuildPhase::ReleasePreviousOccurrence;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::ReleasePreviousOccurrence => {
                let previous =
                    self.occurrence_root
                        .take()
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "previous occurrence root disappeared",
                        ))?;
                session.release(previous)?;
                self.receipt.owners_released =
                    checked_add(self.receipt.owners_released, 1, "released owner count")?;
                self.occurrence_root = self.pending_mut()?.new_occurrence.take();
                self.occurrence_root_id = self.pending_mut()?.new_occurrence_id.take();
                self.count = self
                    .count
                    .checked_add(1)
                    .ok_or(ReferenceSemanticIndexError::Overflow("occurrence count"))?;
                self.high_water = self.pending_ref()?.ordinal;
                self.phase = if self.pending_ref()?.existing_winner_ordinal.is_some() {
                    BuildPhase::AllocateWinnerVersion
                } else {
                    BuildPhase::AllocateTrieLeaf
                };
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::AllocateTrieLeaf => {
                let pending = self.pending_ref()?;
                let occurrence =
                    self.occurrence_root_id
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "occurrence root identity disappeared",
                        ))?;
                let payload = encode_winner_leaf(
                    self.binding,
                    pending.draft.label_id,
                    pending.ordinal,
                    occurrence,
                );
                let (owner, allocation) = session.allocate_packed(&payload, &[occurrence])?;
                let owner_id = session.owner_id(&owner)?;
                self.receipt.record_allocation(allocation)?;
                let pending = self.pending_mut()?;
                pending.new_trie = Some(owner);
                pending.new_trie_id = Some(owner_id);
                pending.build_depth = TRIE_LEVELS - 1;
                self.phase = BuildPhase::AllocateTrieBranch;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::AllocateTrieBranch => {
                let (depth, old, label) = {
                    let pending = self.pending_ref()?;
                    (
                        pending.build_depth,
                        pending.path_nodes[pending.build_depth],
                        pending.draft.label_id,
                    )
                };
                let replacement =
                    self.pending_ref()?
                        .new_trie_id
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "trie child identity disappeared",
                        ))?;
                let (mask, children, count) = build_trie_branch_children(
                    session.arena(),
                    self.binding,
                    depth,
                    old,
                    trie_nibble(label, depth),
                    replacement,
                )?;
                let payload = encode_winner_branch(self.binding, depth, mask)?;
                let (owner, allocation) = session.allocate_packed(&payload, &children[..count])?;
                let owner_id = session.owner_id(&owner)?;
                self.receipt.record_allocation(allocation)?;
                let pending = self.pending_mut()?;
                pending.pending_trie_parent = Some(owner);
                pending.pending_trie_parent_id = Some(owner_id);
                self.phase = BuildPhase::ReleaseTrieChild;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::ReleaseTrieChild => {
                let child = self.pending_mut()?.new_trie.take().ok_or(
                    ReferenceSemanticIndexError::Corrupt("trie child owner disappeared"),
                )?;
                session.release(child)?;
                self.receipt.owners_released =
                    checked_add(self.receipt.owners_released, 1, "released owner count")?;
                let pending = self.pending_mut()?;
                pending.new_trie = pending.pending_trie_parent.take();
                pending.new_trie_id = pending.pending_trie_parent_id.take();
                if pending.build_depth == 0 {
                    pending.trie_root_id = pending.new_trie_id;
                    self.phase = BuildPhase::AllocateWinnerVersion;
                } else {
                    pending.build_depth -= 1;
                    self.phase = BuildPhase::AllocateTrieBranch;
                }
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::AllocateWinnerVersion => {
                let inserted_first = self.pending_ref()?.existing_winner_ordinal.is_none();
                let trie = if inserted_first {
                    self.pending_ref()?.trie_root_id
                } else {
                    self.winner_trie
                };
                let winner_entries = self
                    .winner_entries
                    .checked_add(u64::from(inserted_first))
                    .ok_or(ReferenceSemanticIndexError::Overflow("winner entry count"))?;
                let payload = encode_winner_version(
                    self.binding,
                    self.count,
                    self.count,
                    self.high_water,
                    winner_entries,
                    trie,
                );
                let mut children = [ArenaId::default(); 1];
                let child_count = if let Some(root) = trie {
                    children[0] = root;
                    1
                } else {
                    0
                };
                let (owner, allocation) =
                    session.allocate_packed(&payload, &children[..child_count])?;
                let owner_id = session.owner_id(&owner)?;
                self.receipt.record_allocation(allocation)?;
                let pending = self.pending_mut()?;
                pending.new_winner = Some(owner);
                pending.new_winner_id = Some(owner_id);
                self.phase = if inserted_first {
                    BuildPhase::ReleaseTrieRoot
                } else {
                    BuildPhase::ReleasePreviousWinner
                };
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::ReleaseTrieRoot => {
                let trie = self.pending_mut()?.new_trie.take().ok_or(
                    ReferenceSemanticIndexError::Corrupt("new trie root owner disappeared"),
                )?;
                session.release(trie)?;
                self.receipt.owners_released =
                    checked_add(self.receipt.owners_released, 1, "released owner count")?;
                self.phase = BuildPhase::ReleasePreviousWinner;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::ReleasePreviousWinner => {
                let previous =
                    self.winner_root
                        .take()
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "previous winner root disappeared",
                        ))?;
                session.release(previous)?;
                self.receipt.owners_released =
                    checked_add(self.receipt.owners_released, 1, "released owner count")?;
                let inserted_first = self.pending_ref()?.existing_winner_ordinal.is_none();
                let winner_ordinal = self
                    .pending_ref()?
                    .existing_winner_ordinal
                    .unwrap_or(self.pending_ref()?.ordinal);
                let trie = if inserted_first {
                    self.pending_ref()?.trie_root_id
                } else {
                    self.winner_trie
                };
                self.winner_root = self.pending_mut()?.new_winner.take();
                self.winner_root_id = self.pending_mut()?.new_winner_id.take();
                self.winner_trie = trie;
                self.winner_entries = self
                    .winner_entries
                    .checked_add(u64::from(inserted_first))
                    .ok_or(ReferenceSemanticIndexError::Overflow("winner entry count"))?;
                let pending = self.pending_ref()?;
                let occurrence_root =
                    self.occurrence_root_id
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "occurrence root identity disappeared",
                        ))?;
                let winner_root =
                    self.winner_root_id
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "winner root identity disappeared",
                        ))?;
                self.pending_item_ack = Some(ItemAckInner {
                    build: self.build,
                    authority_nonce: self.binding.authority_nonce,
                    candidate_generation: self.binding.candidate_generation,
                    ordinal: pending.ordinal,
                    count: self.count,
                    high_water: self.high_water,
                    occurrence_generation: self.count,
                    winner_generation: self.count,
                    previous_occurrence_root: pending.prior_occurrence_root,
                    occurrence_root,
                    previous_winner_root: pending.prior_winner_root,
                    winner_root,
                    inserted_first,
                    winner_ordinal,
                });
                self.pending = None;
                self.phase = BuildPhase::ItemAckReady;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::ItemAckReady)
            }
            BuildPhase::ItemAckReady => Ok(ReferenceSemanticIndexProgress::ItemAckReady),
            BuildPhase::AwaitingItemJoin => Err(ReferenceSemanticIndexError::Invalid(
                "reference item acknowledgement is awaiting writer join",
            )),
            BuildPhase::AllocateTerminal => {
                let occurrence =
                    self.occurrence_root_id
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "terminal occurrence root identity disappeared",
                        ))?;
                let winner = self
                    .winner_root_id
                    .ok_or(ReferenceSemanticIndexError::Corrupt(
                        "terminal winner root identity disappeared",
                    ))?;
                let payload = encode_semantic_index_manifest(
                    self.binding,
                    self.count,
                    self.high_water,
                    self.count,
                    self.count,
                    self.winner_entries,
                    occurrence,
                    winner,
                );
                let (owner, allocation) =
                    session.allocate_packed(&payload, &[occurrence, winner])?;
                self.receipt.record_allocation(allocation)?;
                let manifest = session.owner_id(&owner)?;
                self.terminal_root = Some(owner);
                self.pending_terminal_seal = Some(TerminalSealInner {
                    build: self.build,
                    authority_nonce: self.binding.authority_nonce,
                    candidate_generation: self.binding.candidate_generation,
                    count: self.count,
                    high_water: self.high_water,
                    occurrence_generation: self.count,
                    winner_generation: self.count,
                    winner_entries: self.winner_entries,
                    occurrence_root: occurrence,
                    winner_root: winner,
                    manifest,
                });
                self.phase = BuildPhase::ReleaseTerminalOccurrence;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::ReleaseTerminalOccurrence => {
                let owner =
                    self.occurrence_root
                        .take()
                        .ok_or(ReferenceSemanticIndexError::Corrupt(
                            "terminal occurrence root disappeared",
                        ))?;
                session.release(owner)?;
                self.receipt.owners_released =
                    checked_add(self.receipt.owners_released, 1, "released owner count")?;
                self.phase = BuildPhase::ReleaseTerminalWinner;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::Pending)
            }
            BuildPhase::ReleaseTerminalWinner => {
                let owner = self
                    .winner_root
                    .take()
                    .ok_or(ReferenceSemanticIndexError::Corrupt(
                        "terminal winner root disappeared",
                    ))?;
                session.release(owner)?;
                self.receipt.owners_released =
                    checked_add(self.receipt.owners_released, 1, "released owner count")?;
                self.phase = BuildPhase::TerminalSealReady;
                self.note_mutation(session)?;
                Ok(ReferenceSemanticIndexProgress::TerminalSealReady)
            }
            BuildPhase::TerminalSealReady => Ok(ReferenceSemanticIndexProgress::TerminalSealReady),
            BuildPhase::AwaitingTerminalJoin => Err(ReferenceSemanticIndexError::Invalid(
                "terminal reference-index seal is awaiting writer join",
            )),
            BuildPhase::Committable => Ok(ReferenceSemanticIndexProgress::Committable),
            BuildPhase::Failed => Err(ReferenceSemanticIndexError::Invalid(
                "reference-index builder has already failed",
            )),
        }
    }

    fn poll_winner_plan(&mut self, arena: &PageArena) -> Result<(), ReferenceSemanticIndexError> {
        let binding = self.binding;
        let pending = self.pending_mut()?;
        let Some(node) = pending.plan_cursor else {
            self.phase = BuildPhase::AllocateOccurrence;
            return Ok(());
        };
        if pending.plan_depth == TRIE_LEVELS {
            let leaf = decode_winner_leaf(arena, node, binding)?;
            if leaf.label_id != pending.draft.label_id {
                return Err(ReferenceSemanticIndexError::Corrupt(
                    "exact winner trie reached a different label leaf",
                ));
            }
            let DecodedOccurrenceNode::Item(winner) =
                decode_occurrence_node(arena, leaf.occurrence, binding)?
            else {
                return Err(ReferenceSemanticIndexError::Corrupt(
                    "winner trie points at the empty occurrence sentinel",
                ));
            };
            if winner.label_id != leaf.label_id || winner.ordinal != leaf.occurrence_ordinal {
                return Err(ReferenceSemanticIndexError::Corrupt(
                    "winner trie leaf and occurrence disagree",
                ));
            }
            pending.existing_winner_ordinal = Some(leaf.occurrence_ordinal);
            self.phase = BuildPhase::AllocateOccurrence;
            return Ok(());
        }
        let depth = pending.plan_depth;
        let branch = decode_winner_branch(arena, node, binding, depth)?;
        pending.path_nodes[depth] = Some(node);
        let nibble = trie_nibble(pending.draft.label_id, depth);
        pending.plan_cursor = branch_child(arena, node, branch.mask, nibble)?;
        pending.plan_depth += 1;
        if pending.plan_cursor.is_none() {
            self.phase = BuildPhase::AllocateOccurrence;
        }
        Ok(())
    }

    fn take_item_ack(&mut self) -> Result<ReferenceIndexItemAck, ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::ItemAckReady {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference item acknowledgement is not ready",
            ));
        }
        let inner = self
            .pending_item_ack
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "ready reference item acknowledgement disappeared",
            ))?;
        self.phase = BuildPhase::AwaitingItemJoin;
        Ok(ReferenceIndexItemAck { inner: Some(inner) })
    }

    fn validate_pending_item_ack(
        &self,
        inner: &ItemAckInner,
    ) -> Result<(), ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::AwaitingItemJoin
            || self.pending_item_ack.as_ref() != Some(inner)
        {
            return Err(ReferenceSemanticIndexError::CrossedAck);
        }
        Ok(())
    }

    fn finish_item_ack_join(
        &mut self,
        inner: ItemAckInner,
    ) -> Result<(), ReferenceSemanticIndexError> {
        self.validate_pending_item_ack(&inner)?;
        self.pending_item_ack = None;
        self.phase = BuildPhase::Ready;
        Ok(())
    }

    fn take_terminal_seal(
        &mut self,
    ) -> Result<ReferenceIndexTerminalSeal, ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::TerminalSealReady {
            return Err(ReferenceSemanticIndexError::Invalid(
                "terminal reference-index seal is not ready",
            ));
        }
        let inner = self
            .pending_terminal_seal
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "ready terminal reference-index seal disappeared",
            ))?;
        self.phase = BuildPhase::AwaitingTerminalJoin;
        Ok(ReferenceIndexTerminalSeal { inner: Some(inner) })
    }

    fn validate_pending_terminal_seal(
        &self,
        inner: &TerminalSealInner,
    ) -> Result<(), ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::AwaitingTerminalJoin
            || self.pending_terminal_seal.as_ref() != Some(inner)
        {
            return Err(ReferenceSemanticIndexError::CrossedAck);
        }
        Ok(())
    }

    fn finish_terminal_join(
        &mut self,
        inner: TerminalSealInner,
    ) -> Result<(), ReferenceSemanticIndexError> {
        self.validate_pending_terminal_seal(&inner)?;
        self.pending_terminal_seal = None;
        self.phase = BuildPhase::Committable;
        Ok(())
    }

    fn commit(
        mut self,
        session: ArenaBuildSession<'_>,
    ) -> Result<ReferenceSemanticIndexDocument, ReferenceSemanticIndexError> {
        if self.phase != BuildPhase::Committable || session.id() != self.build {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference semantic index is not committable by this session",
            ));
        }
        let owner = self
            .terminal_root
            .take()
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "committable reference-index manifest disappeared",
            ))?;
        if session.live_owners()? != 1 {
            return Err(ReferenceSemanticIndexError::Corrupt(
                "reference-index journal did not reduce to one atomic manifest",
            ));
        }
        let owner = session.commit(owner)?;
        Ok(ReferenceSemanticIndexDocument { owner: Some(owner) })
    }

    fn pending_ref(&self) -> Result<&PendingOccurrence, ReferenceSemanticIndexError> {
        self.pending
            .as_ref()
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "reference item state disappeared",
            ))
    }

    fn pending_mut(&mut self) -> Result<&mut PendingOccurrence, ReferenceSemanticIndexError> {
        self.pending
            .as_mut()
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "reference item state disappeared",
            ))
    }

    fn ensure_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), ReferenceSemanticIndexError> {
        if session.id() != self.build {
            return Err(ReferenceSemanticIndexError::Invalid(
                "reference-index builder and arena session differ",
            ));
        }
        Ok(())
    }

    fn note_mutation(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), ReferenceSemanticIndexError> {
        self.receipt.mutations = checked_add(self.receipt.mutations, 1, "mutation count")?;
        self.receipt.maximum_live_owners_observed = self
            .receipt
            .maximum_live_owners_observed
            .max(session.live_owners()?);
        if self.fault_after_mutation == Some(self.receipt.mutations) {
            let mutation = self.receipt.mutations;
            self.phase = BuildPhase::Failed;
            return Err(ReferenceSemanticIndexError::InjectedFault { mutation });
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ReferenceSemanticIndexDocument {
    owner: Option<OwnedArenaRef>,
}

impl ReferenceSemanticIndexDocument {
    fn root(&self) -> ArenaId {
        self.owner
            .as_ref()
            .expect("live reference semantic index owns its manifest")
            .id()
    }

    fn descriptor(
        &self,
        arena: &PageArena,
    ) -> Result<ReferenceSemanticIndexDescriptor, ReferenceSemanticIndexError> {
        decode_semantic_index_manifest(arena, self.root())
    }

    fn occurrences(
        &self,
        arena: &PageArena,
    ) -> Result<Vec<StoredReferenceOccurrence>, ReferenceSemanticIndexError> {
        let descriptor = self.descriptor(arena)?;
        let mut output = Vec::with_capacity(
            usize::try_from(descriptor.count)
                .map_err(|_| ReferenceSemanticIndexError::Overflow("occurrence query count"))?,
        );
        let mut node = descriptor.occurrence_root;
        let mut expected = descriptor.count;
        loop {
            match decode_occurrence_node(arena, node, descriptor.binding)? {
                DecodedOccurrenceNode::Empty => {
                    if expected != 0 {
                        return Err(ReferenceSemanticIndexError::Corrupt(
                            "occurrence chain ended before its declared count",
                        ));
                    }
                    break;
                }
                DecodedOccurrenceNode::Item(occurrence) => {
                    if occurrence.ordinal != expected {
                        return Err(ReferenceSemanticIndexError::Corrupt(
                            "occurrence chain is not in exact reverse document order",
                        ));
                    }
                    expected =
                        expected
                            .checked_sub(1)
                            .ok_or(ReferenceSemanticIndexError::Corrupt(
                                "occurrence chain underflowed its declared count",
                            ))?;
                    node = occurrence.previous;
                    output.push(occurrence);
                }
            }
        }
        output.reverse();
        Ok(output)
    }

    fn winner(
        &self,
        arena: &PageArena,
        label_id: u64,
    ) -> Result<Option<StoredReferenceOccurrence>, ReferenceSemanticIndexError> {
        let descriptor = self.descriptor(arena)?;
        let winner = decode_winner_version(arena, descriptor.winner_root, descriptor.binding)?;
        let Some(mut node) = winner.trie_root else {
            return Ok(None);
        };
        for depth in 0..TRIE_LEVELS {
            let branch = decode_winner_branch(arena, node, descriptor.binding, depth)?;
            let Some(child) = branch_child(arena, node, branch.mask, trie_nibble(label_id, depth))?
            else {
                return Ok(None);
            };
            node = child;
        }
        let leaf = decode_winner_leaf(arena, node, descriptor.binding)?;
        if leaf.label_id != label_id {
            return Ok(None);
        }
        let DecodedOccurrenceNode::Item(occurrence) =
            decode_occurrence_node(arena, leaf.occurrence, descriptor.binding)?
        else {
            return Err(ReferenceSemanticIndexError::Corrupt(
                "winner leaf points at the empty occurrence sentinel",
            ));
        };
        if occurrence.label_id != label_id || occurrence.ordinal != leaf.occurrence_ordinal {
            return Err(ReferenceSemanticIndexError::Corrupt(
                "winner leaf and retained occurrence disagree",
            ));
        }
        Ok(Some(occurrence))
    }

    fn release_later(mut self, arena: &mut PageArena) -> Result<(), ReferenceSemanticIndexError> {
        let owner = self
            .owner
            .take()
            .ok_or(ReferenceSemanticIndexError::Corrupt(
                "reference semantic index owner disappeared",
            ))?;
        arena
            .release_later(owner)
            .map_err(|failure| ReferenceSemanticIndexError::Arena(failure.error))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReferenceSemanticIndexDescriptor {
    binding: CandidateReferenceBinding,
    count: u64,
    high_water: u64,
    occurrence_generation: u64,
    winner_generation: u64,
    winner_entries: u64,
    occurrence_root: ArenaId,
    winner_root: ArenaId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StoredReferenceOccurrence {
    ordinal: u64,
    label_id: u64,
    definition: StoredReferenceSlice,
    destination: StoredReferenceSlice,
    title: Option<StoredReferenceSlice>,
    previous: ArenaId,
}

#[allow(clippy::large_enum_variant)] // Query scratch is bounded to one fixed-size descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodedOccurrenceNode {
    Empty,
    Item(StoredReferenceOccurrence),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedWinnerVersion {
    generation: u64,
    count: u64,
    high_water: u64,
    entries: u64,
    trie_root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceSemanticIndexError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    InjectedFault { mutation: u64 },
    CrossedAck,
    ReplayedAck,
    OutOfOrderAck { expected: u64, actual: u64 },
}

impl From<ArenaError> for ReferenceSemanticIndexError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for ReferenceSemanticIndexError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl fmt::Display for ReferenceSemanticIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Invalid(message) => {
                write!(formatter, "invalid reference semantic index: {message}")
            }
            Self::Corrupt(message) => {
                write!(formatter, "corrupt reference semantic index: {message}")
            }
            Self::Overflow(component) => {
                write!(formatter, "reference semantic index overflow: {component}")
            }
            Self::InjectedFault { mutation } => write!(
                formatter,
                "injected reference-index fault after mutation {mutation}"
            ),
            Self::CrossedAck => {
                formatter.write_str("reference-index acknowledgement crossed writer authority")
            }
            Self::ReplayedAck => {
                formatter.write_str("reference-index acknowledgement was replayed")
            }
            Self::OutOfOrderAck { expected, actual } => write!(
                formatter,
                "reference-index acknowledgement {actual} arrived before {expected}"
            ),
        }
    }
}

impl std::error::Error for ReferenceSemanticIndexError {}

fn checked_add(
    left: u64,
    right: u64,
    component: &'static str,
) -> Result<u64, ReferenceSemanticIndexError> {
    left.checked_add(right)
        .ok_or(ReferenceSemanticIndexError::Overflow(component))
}

fn trie_nibble(label: u64, depth: usize) -> usize {
    debug_assert!(depth < TRIE_LEVELS);
    usize::try_from((label >> (60 - depth * 4)) & 0x0f).expect("nibble fits usize")
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        input[offset..offset + 8]
            .try_into()
            .expect("validated fixed payload contains u64"),
    )
}

fn encode_arena_id(output: &mut [u8], offset: usize, id: ArenaId) {
    output[offset..offset + 4].copy_from_slice(&id.index.to_le_bytes());
    output[offset + 4..offset + 8].copy_from_slice(&id.generation.to_le_bytes());
}

fn decode_arena_id(input: &[u8], offset: usize) -> ArenaId {
    ArenaId {
        index: u32::from_le_bytes(
            input[offset..offset + 4]
                .try_into()
                .expect("validated fixed payload contains arena index"),
        ),
        generation: u32::from_le_bytes(
            input[offset + 4..offset + 8]
                .try_into()
                .expect("validated fixed payload contains arena generation"),
        ),
    }
}

fn encode_binding(output: &mut [u8], offset: usize, binding: CandidateReferenceBinding) {
    let values = [
        binding.authority_nonce,
        binding.candidate_generation,
        binding.semantic_epoch,
        binding.source_root,
        binding.source_revision,
        binding.source_extent.bytes,
        binding.source_extent.utf16,
        binding.paragraph,
        binding.paragraph_source.start.bytes,
        binding.paragraph_source.end.bytes,
        binding.projection_root,
        binding.projection_generation,
        binding.interner_generation,
        binding.paragraph_source.start.utf16,
        binding.paragraph_source.end.utf16,
        binding.paragraph_logical_extent.bytes,
        binding.paragraph_logical_extent.utf16,
        binding.projection_runs,
    ];
    for (index, value) in values.into_iter().enumerate() {
        put_u64(output, offset + index * 8, value);
    }
}

fn decode_binding(input: &[u8], offset: usize) -> CandidateReferenceBinding {
    CandidateReferenceBinding {
        authority_nonce: get_u64(input, offset),
        candidate_generation: get_u64(input, offset + 8),
        semantic_epoch: get_u64(input, offset + 16),
        source_root: get_u64(input, offset + 24),
        source_revision: get_u64(input, offset + 32),
        source_extent: DualMetric::new(get_u64(input, offset + 40), get_u64(input, offset + 48)),
        paragraph: get_u64(input, offset + 56),
        paragraph_source: DualRange::new(
            DualMetric::new(get_u64(input, offset + 64), get_u64(input, offset + 104)),
            DualMetric::new(get_u64(input, offset + 72), get_u64(input, offset + 112)),
        ),
        projection_root: get_u64(input, offset + 80),
        projection_generation: get_u64(input, offset + 88),
        interner_generation: get_u64(input, offset + 96),
        paragraph_logical_extent: DualMetric::new(
            get_u64(input, offset + 120),
            get_u64(input, offset + 128),
        ),
        projection_runs: get_u64(input, offset + 136),
    }
}

fn encode_slice(output: &mut [u8], offset: usize, slice: StoredReferenceSlice) {
    let values = [
        slice.source.start.bytes,
        slice.source.end.bytes,
        slice.source.start.utf16,
        slice.source.end.utf16,
        slice.logical.start.bytes,
        slice.logical.end.bytes,
        slice.logical.start.utf16,
        slice.logical.end.utf16,
        slice.projection_run_start,
        slice.projection_run_end,
        slice.projection_program_root,
        slice.projection_program_generation,
    ];
    for (index, value) in values.into_iter().enumerate() {
        put_u64(output, offset + index * 8, value);
    }
}

fn decode_slice(input: &[u8], offset: usize) -> StoredReferenceSlice {
    StoredReferenceSlice {
        source: DualRange::new(
            DualMetric::new(get_u64(input, offset), get_u64(input, offset + 16)),
            DualMetric::new(get_u64(input, offset + 8), get_u64(input, offset + 24)),
        ),
        logical: DualRange::new(
            DualMetric::new(get_u64(input, offset + 32), get_u64(input, offset + 48)),
            DualMetric::new(get_u64(input, offset + 40), get_u64(input, offset + 56)),
        ),
        projection_run_start: get_u64(input, offset + 64),
        projection_run_end: get_u64(input, offset + 72),
        projection_program_root: get_u64(input, offset + 80),
        projection_program_generation: get_u64(input, offset + 88),
    }
}

fn encode_empty_occurrence(binding: CandidateReferenceBinding) -> [u8; EMPTY_OCCURRENCE_BYTES] {
    let mut output = [0_u8; EMPTY_OCCURRENCE_BYTES];
    output[0] = EMPTY_OCCURRENCE_TAG;
    output[1] = FORMAT_VERSION;
    encode_binding(&mut output, 8, binding);
    output
}

fn encode_occurrence(
    binding: CandidateReferenceBinding,
    pending: &PendingOccurrence,
) -> [u8; OCCURRENCE_BYTES] {
    let mut output = [0_u8; OCCURRENCE_BYTES];
    output[0] = OCCURRENCE_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = u8::from(pending.draft.title.is_some());
    encode_binding(&mut output, 8, binding);
    let header = 8 + BINDING_BYTES;
    put_u64(&mut output, header, pending.ordinal);
    put_u64(&mut output, header + 8, pending.ordinal);
    put_u64(&mut output, header + 16, pending.ordinal);
    put_u64(&mut output, header + 24, pending.draft.label_id);
    encode_arena_id(&mut output, header + 32, pending.prior_occurrence_root);
    let slices = header + 48;
    encode_slice(&mut output, slices, pending.draft.definition);
    encode_slice(&mut output, slices + SLICE_BYTES, pending.draft.destination);
    if let Some(title) = pending.draft.title {
        encode_slice(&mut output, slices + 2 * SLICE_BYTES, title);
    }
    output
}

fn encode_winner_version(
    binding: CandidateReferenceBinding,
    generation: u64,
    count: u64,
    high_water: u64,
    entries: u64,
    trie: Option<ArenaId>,
) -> [u8; WINNER_VERSION_BYTES] {
    let mut output = [0_u8; WINNER_VERSION_BYTES];
    output[0] = WINNER_VERSION_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = u8::from(trie.is_some());
    encode_binding(&mut output, 8, binding);
    let header = 8 + BINDING_BYTES;
    put_u64(&mut output, header, generation);
    put_u64(&mut output, header + 8, count);
    put_u64(&mut output, header + 16, high_water);
    put_u64(&mut output, header + 24, entries);
    if let Some(root) = trie {
        encode_arena_id(&mut output, header + 32, root);
    }
    output
}

fn encode_winner_branch(
    binding: CandidateReferenceBinding,
    depth: usize,
    mask: u16,
) -> Result<[u8; WINNER_BRANCH_BYTES], ReferenceSemanticIndexError> {
    let mut output = [0_u8; WINNER_BRANCH_BYTES];
    output[0] = WINNER_BRANCH_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = u8::try_from(depth)
        .map_err(|_| ReferenceSemanticIndexError::Overflow("winner trie depth"))?;
    output[4..6].copy_from_slice(&mask.to_le_bytes());
    put_u64(&mut output, 8, binding.authority_nonce);
    put_u64(&mut output, 16, binding.candidate_generation);
    Ok(output)
}

fn encode_winner_leaf(
    binding: CandidateReferenceBinding,
    label: u64,
    occurrence_ordinal: u64,
    occurrence: ArenaId,
) -> [u8; WINNER_LEAF_BYTES] {
    let mut output = [0_u8; WINNER_LEAF_BYTES];
    output[0] = WINNER_LEAF_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = u8::try_from(TRIE_LEVELS).expect("trie depth fits u8");
    put_u64(&mut output, 8, binding.authority_nonce);
    put_u64(&mut output, 16, binding.candidate_generation);
    put_u64(&mut output, 24, label);
    put_u64(&mut output, 32, occurrence_ordinal);
    encode_arena_id(&mut output, 40, occurrence);
    output
}

#[allow(clippy::too_many_arguments)]
fn encode_semantic_index_manifest(
    binding: CandidateReferenceBinding,
    count: u64,
    high_water: u64,
    occurrence_generation: u64,
    winner_generation: u64,
    winner_entries: u64,
    occurrence: ArenaId,
    winner: ArenaId,
) -> [u8; SEMANTIC_INDEX_BYTES] {
    let mut output = [0_u8; SEMANTIC_INDEX_BYTES];
    output[0] = SEMANTIC_INDEX_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = 2;
    encode_binding(&mut output, 8, binding);
    let header = 8 + BINDING_BYTES;
    put_u64(&mut output, header, count);
    put_u64(&mut output, header + 8, high_water);
    put_u64(&mut output, header + 16, occurrence_generation);
    put_u64(&mut output, header + 24, winner_generation);
    put_u64(&mut output, header + 32, winner_entries);
    encode_arena_id(&mut output, header + 40, occurrence);
    encode_arena_id(&mut output, header + 48, winner);
    output
}

fn decode_semantic_index_manifest(
    arena: &PageArena,
    root: ArenaId,
) -> Result<ReferenceSemanticIndexDescriptor, ReferenceSemanticIndexError> {
    let payload = arena.payload(root)?;
    let header = 8 + BINDING_BYTES;
    if payload.len() != SEMANTIC_INDEX_BYTES
        || payload[0] != SEMANTIC_INDEX_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] != 2
        || payload[3..8] != [0; 5]
        || payload[header + 56..].iter().any(|byte| *byte != 0)
        || arena.packed_child_count(root)? != 2
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "reference semantic-index manifest descriptor changed",
        ));
    }
    let binding = decode_binding(payload, 8);
    binding.validate()?;
    let occurrence_root = arena.packed_child_at(root, 0)?;
    let winner_root = arena.packed_child_at(root, 1)?;
    if occurrence_root == winner_root
        || decode_arena_id(payload, header + 40) != occurrence_root
        || decode_arena_id(payload, header + 48) != winner_root
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "reference semantic-index child roles changed",
        ));
    }
    let descriptor = ReferenceSemanticIndexDescriptor {
        binding,
        count: get_u64(payload, header),
        high_water: get_u64(payload, header + 8),
        occurrence_generation: get_u64(payload, header + 16),
        winner_generation: get_u64(payload, header + 24),
        winner_entries: get_u64(payload, header + 32),
        occurrence_root,
        winner_root,
    };
    let occurrence = decode_occurrence_root_header(arena, occurrence_root, binding)?;
    let winner = decode_winner_version(arena, winner_root, binding)?;
    if descriptor.count != descriptor.high_water
        || descriptor.occurrence_generation != descriptor.count
        || descriptor.winner_generation != descriptor.count
        || descriptor.winner_entries > descriptor.count
        || occurrence != (descriptor.count, descriptor.high_water)
        || winner.generation != descriptor.winner_generation
        || winner.count != descriptor.count
        || winner.high_water != descriptor.high_water
        || winner.entries != descriptor.winner_entries
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "reference semantic-index terminal totals disagree",
        ));
    }
    Ok(descriptor)
}

fn decode_occurrence_root_header(
    arena: &PageArena,
    node: ArenaId,
    binding: CandidateReferenceBinding,
) -> Result<(u64, u64), ReferenceSemanticIndexError> {
    match decode_occurrence_node(arena, node, binding)? {
        DecodedOccurrenceNode::Empty => Ok((0, 0)),
        DecodedOccurrenceNode::Item(occurrence) => Ok((occurrence.ordinal, occurrence.ordinal)),
    }
}

fn decode_empty_occurrence(
    arena: &PageArena,
    node: ArenaId,
    binding: CandidateReferenceBinding,
) -> Result<(), ReferenceSemanticIndexError> {
    let payload = arena.payload(node)?;
    if payload.len() != EMPTY_OCCURRENCE_BYTES
        || payload[0] != EMPTY_OCCURRENCE_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2..8] != [0; 6]
        || decode_binding(payload, 8) != binding
        || payload[8 + BINDING_BYTES..] != [0; EMPTY_OCCURRENCE_BYTES - (8 + BINDING_BYTES)]
        || arena.packed_child_count(node)? != 0
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "empty occurrence sentinel descriptor changed",
        ));
    }
    Ok(())
}

fn validate_stored_slice(
    slice: StoredReferenceSlice,
    binding: CandidateReferenceBinding,
) -> Result<(), ReferenceSemanticIndexError> {
    if !slice.source.is_ordered()
        || !slice.logical.is_ordered()
        || !binding.paragraph_source.contains(slice.source)
        || !slice
            .logical
            .end
            .no_later_than(binding.paragraph_logical_extent)
        || slice.projection_run_start > slice.projection_run_end
        || slice.projection_run_end > binding.projection_runs
        || slice.projection_program_root != binding.projection_root
        || slice.projection_program_generation != binding.projection_generation
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "stored reference slice escaped its candidate projection",
        ));
    }
    Ok(())
}

fn decode_occurrence_node(
    arena: &PageArena,
    node: ArenaId,
    binding: CandidateReferenceBinding,
) -> Result<DecodedOccurrenceNode, ReferenceSemanticIndexError> {
    let payload = arena.payload(node)?;
    if payload.first() == Some(&EMPTY_OCCURRENCE_TAG) {
        decode_empty_occurrence(arena, node, binding)?;
        return Ok(DecodedOccurrenceNode::Empty);
    }
    let header = 8 + BINDING_BYTES;
    let slices = header + 48;
    if payload.len() != OCCURRENCE_BYTES
        || payload[0] != OCCURRENCE_TAG
        || payload[1] != FORMAT_VERSION
        || !matches!(payload[2], 0 | 1)
        || payload[3..8] != [0; 5]
        || decode_binding(payload, 8) != binding
        || payload[slices + 3 * SLICE_BYTES..]
            .iter()
            .any(|byte| *byte != 0)
        || arena.packed_child_count(node)? != 1
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "reference occurrence descriptor changed",
        ));
    }
    let ordinal = get_u64(payload, header);
    let count = get_u64(payload, header + 8);
    let high_water = get_u64(payload, header + 16);
    let label_id = get_u64(payload, header + 24);
    let previous = arena.packed_child_at(node, 0)?;
    if ordinal == 0
        || ordinal != count
        || ordinal != high_water
        || label_id == 0
        || decode_arena_id(payload, header + 32) != previous
        || payload[header + 40..header + 48] != [0; 8]
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "reference occurrence ordering descriptor changed",
        ));
    }
    let definition = decode_slice(payload, slices);
    let destination = decode_slice(payload, slices + SLICE_BYTES);
    let title = if payload[2] == 1 {
        Some(decode_slice(payload, slices + 2 * SLICE_BYTES))
    } else {
        if payload[slices + 2 * SLICE_BYTES..slices + 3 * SLICE_BYTES] != [0; SLICE_BYTES] {
            return Err(ReferenceSemanticIndexError::Corrupt(
                "title-free occurrence retains a title descriptor",
            ));
        }
        None
    };
    validate_stored_slice(definition, binding)?;
    validate_stored_slice(destination, binding)?;
    if !definition.source.contains(destination.source)
        || !definition.logical.contains(destination.logical)
        || title.is_some_and(|value| {
            !definition.source.contains(value.source)
                || !definition.logical.contains(value.logical)
                || validate_stored_slice(value, binding).is_err()
        })
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "stored reference values escaped their definition",
        ));
    }
    Ok(DecodedOccurrenceNode::Item(StoredReferenceOccurrence {
        ordinal,
        label_id,
        definition,
        destination,
        title,
        previous,
    }))
}

fn decode_winner_version(
    arena: &PageArena,
    node: ArenaId,
    binding: CandidateReferenceBinding,
) -> Result<DecodedWinnerVersion, ReferenceSemanticIndexError> {
    let payload = arena.payload(node)?;
    let header = 8 + BINDING_BYTES;
    if payload.len() != WINNER_VERSION_BYTES
        || payload[0] != WINNER_VERSION_TAG
        || payload[1] != FORMAT_VERSION
        || !matches!(payload[2], 0 | 1)
        || payload[3..8] != [0; 5]
        || decode_binding(payload, 8) != binding
        || payload[header + 40..].iter().any(|byte| *byte != 0)
        || arena.packed_child_count(node)? != usize::from(payload[2])
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "winner-index version descriptor changed",
        ));
    }
    let trie_root = if payload[2] == 1 {
        let child = arena.packed_child_at(node, 0)?;
        if decode_arena_id(payload, header + 32) != child {
            return Err(ReferenceSemanticIndexError::Corrupt(
                "winner-index trie edge changed",
            ));
        }
        Some(child)
    } else {
        if payload[header + 32..header + 40] != [0; 8] {
            return Err(ReferenceSemanticIndexError::Corrupt(
                "empty winner-index version encodes a trie root",
            ));
        }
        None
    };
    let decoded = DecodedWinnerVersion {
        generation: get_u64(payload, header),
        count: get_u64(payload, header + 8),
        high_water: get_u64(payload, header + 16),
        entries: get_u64(payload, header + 24),
        trie_root,
    };
    if decoded.generation != decoded.count
        || decoded.count != decoded.high_water
        || decoded.entries > decoded.count
        || (decoded.entries == 0) != decoded.trie_root.is_none()
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "winner-index version totals disagree",
        ));
    }
    Ok(decoded)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedWinnerBranch {
    mask: u16,
}

fn decode_winner_branch(
    arena: &PageArena,
    node: ArenaId,
    binding: CandidateReferenceBinding,
    expected_depth: usize,
) -> Result<DecodedWinnerBranch, ReferenceSemanticIndexError> {
    let payload = arena.payload(node)?;
    if payload.len() != WINNER_BRANCH_BYTES
        || payload[0] != WINNER_BRANCH_TAG
        || payload[1] != FORMAT_VERSION
        || usize::from(payload[2]) != expected_depth
        || payload[3] != 0
        || get_u64(payload, 8) != binding.authority_nonce
        || get_u64(payload, 16) != binding.candidate_generation
        || payload[24..] != [0; WINNER_BRANCH_BYTES - 24]
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "winner trie branch descriptor changed",
        ));
    }
    let mask = u16::from_le_bytes(
        payload[4..6]
            .try_into()
            .expect("fixed winner branch contains mask"),
    );
    if mask == 0 || arena.packed_child_count(node)? != mask.count_ones() as usize {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "winner trie branch mask disagrees with children",
        ));
    }
    Ok(DecodedWinnerBranch { mask })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedWinnerLeaf {
    label_id: u64,
    occurrence_ordinal: u64,
    occurrence: ArenaId,
}

fn decode_winner_leaf(
    arena: &PageArena,
    node: ArenaId,
    binding: CandidateReferenceBinding,
) -> Result<DecodedWinnerLeaf, ReferenceSemanticIndexError> {
    let payload = arena.payload(node)?;
    if payload.len() != WINNER_LEAF_BYTES
        || payload[0] != WINNER_LEAF_TAG
        || payload[1] != FORMAT_VERSION
        || usize::from(payload[2]) != TRIE_LEVELS
        || get_u64(payload, 8) != binding.authority_nonce
        || get_u64(payload, 16) != binding.candidate_generation
        || payload[48..] != [0; WINNER_LEAF_BYTES - 48]
        || arena.packed_child_count(node)? != 1
    {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "winner trie leaf descriptor changed",
        ));
    }
    let occurrence = arena.packed_child_at(node, 0)?;
    if decode_arena_id(payload, 40) != occurrence {
        return Err(ReferenceSemanticIndexError::Corrupt(
            "winner trie leaf occurrence edge changed",
        ));
    }
    Ok(DecodedWinnerLeaf {
        label_id: get_u64(payload, 24),
        occurrence_ordinal: get_u64(payload, 32),
        occurrence,
    })
}

fn branch_child(
    arena: &PageArena,
    node: ArenaId,
    mask: u16,
    nibble: usize,
) -> Result<Option<ArenaId>, ReferenceSemanticIndexError> {
    let bit = 1_u16 << nibble;
    if mask & bit == 0 {
        return Ok(None);
    }
    let lower = mask & bit.wrapping_sub(1);
    Ok(Some(
        arena.packed_child_at(node, lower.count_ones() as usize)?,
    ))
}

fn build_trie_branch_children(
    arena: &PageArena,
    binding: CandidateReferenceBinding,
    depth: usize,
    old: Option<ArenaId>,
    replacement_nibble: usize,
    replacement: ArenaId,
) -> Result<(u16, [ArenaId; TRIE_FANOUT], usize), ReferenceSemanticIndexError> {
    let old_mask = if let Some(node) = old {
        decode_winner_branch(arena, node, binding, depth)?.mask
    } else {
        0
    };
    let mask = old_mask | (1_u16 << replacement_nibble);
    let mut children = [ArenaId::default(); TRIE_FANOUT];
    let mut count = 0;
    for nibble in 0..TRIE_FANOUT {
        if mask & (1_u16 << nibble) == 0 {
            continue;
        }
        children[count] = if nibble == replacement_nibble {
            replacement
        } else {
            let old = old.ok_or(ReferenceSemanticIndexError::Corrupt(
                "new winner branch unexpectedly requests an old sibling",
            ))?;
            branch_child(arena, old, old_mask, nibble)?.ok_or(
                ReferenceSemanticIndexError::Corrupt("winner branch sibling disappeared"),
            )?
        };
        count += 1;
    }
    Ok((mask, children, count))
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    fn metric(value: u64) -> DualMetric {
        DualMetric::new(value, value)
    }

    fn range(start: u64, end: u64) -> DualRange {
        DualRange::new(metric(start), metric(end))
    }

    fn binding(authority_nonce: u64, extent: u64) -> CandidateReferenceBinding {
        CandidateReferenceBinding {
            authority_nonce,
            candidate_generation: 17,
            semantic_epoch: 23,
            source_root: 31,
            source_revision: 37,
            source_extent: metric(extent),
            paragraph: 41,
            paragraph_source: range(100, extent - 100),
            paragraph_logical_extent: metric(extent - 200),
            projection_root: 43,
            projection_generation: 47,
            projection_runs: extent / 2,
            interner_generation: 53,
        }
    }

    fn definition(
        authority: &CandidateReferenceAuthority,
        label_id: u64,
        source_start: u64,
        source_end: u64,
        title: bool,
    ) -> ReferenceOccurrenceDraft {
        let logical_start = source_start - authority.binding.paragraph_source.start.bytes;
        let logical_end = source_end - authority.binding.paragraph_source.start.bytes;
        let width = source_end - source_start;
        let destination_end = source_start + width / 2;
        let title_start = destination_end + 1;
        let definition = authority
            .bind_slice(
                range(source_start, source_end),
                range(logical_start, logical_end),
                (0, 3),
            )
            .unwrap();
        let destination = authority
            .bind_slice(
                range(source_start + 1, destination_end),
                range(logical_start + 1, logical_start + width / 2),
                (0, 2),
            )
            .unwrap();
        let title = title.then(|| {
            authority
                .bind_slice(
                    range(title_start, source_end - 1),
                    range(logical_start + width / 2 + 1, logical_end - 1),
                    (2, 3),
                )
                .unwrap()
        });
        authority
            .definition(
                authority.bind_label(label_id).unwrap(),
                definition,
                destination,
                title,
            )
            .unwrap()
    }

    fn bootstrap(builder: &mut ReferenceSemanticIndexBuilder, session: &mut ArenaBuildSession<'_>) {
        while builder.phase != BuildPhase::Ready {
            builder.poll(session).unwrap();
        }
    }

    fn append(
        builder: &mut ReferenceSemanticIndexBuilder,
        joiner: &mut ReferenceIndexAckJoiner,
        session: &mut ArenaBuildSession<'_>,
        draft: ReferenceOccurrenceDraft,
    ) -> ItemAckInner {
        builder.begin_item(draft).unwrap();
        while builder.phase != BuildPhase::ItemAckReady {
            builder.poll(session).unwrap();
        }
        let mut ack = builder.take_item_ack().unwrap();
        let inner = *ack.inner.as_ref().unwrap();
        joiner.join_item(builder, &mut ack).unwrap();
        inner
    }

    fn seal(
        builder: &mut ReferenceSemanticIndexBuilder,
        joiner: &mut ReferenceIndexAckJoiner,
        session: &mut ArenaBuildSession<'_>,
    ) -> TerminalSealInner {
        builder.begin_terminal().unwrap();
        while builder.phase != BuildPhase::TerminalSealReady {
            builder.poll(session).unwrap();
        }
        let mut seal = builder.take_terminal_seal().unwrap();
        let inner = *seal.inner.as_ref().unwrap();
        joiner.join_terminal(builder, &mut seal).unwrap();
        inner
    }

    fn build_document(
        arena: &mut PageArena,
        binding: CandidateReferenceBinding,
        drafts: Vec<ReferenceOccurrenceDraft>,
    ) -> (
        ReferenceSemanticIndexDocument,
        ReferenceSemanticIndexReceipt,
        Vec<ItemAckInner>,
        TerminalSealInner,
    ) {
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut builder = ReferenceSemanticIndexBuilder::begin(build, binding).unwrap();
        let mut joiner = ReferenceIndexAckJoiner::new(build, binding);
        bootstrap(&mut builder, &mut session);
        let acks = drafts
            .into_iter()
            .map(|draft| append(&mut builder, &mut joiner, &mut session, draft))
            .collect();
        let terminal = seal(&mut builder, &mut joiner, &mut session);
        let receipt = builder.receipt();
        let document = builder.commit(session).unwrap();
        (document, receipt, acks, terminal)
    }

    fn reclaim_document(arena: &mut PageArena, document: ReferenceSemanticIndexDocument) {
        document.release_later(arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    fn drain_abort(arena: &mut PageArena, build: ArenaBuildId) {
        loop {
            if arena.poll_build_abort(build, 1).unwrap().complete {
                break;
            }
        }
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn ordered_occurrences_and_exact_label_trie_preserve_first_winner() {
        let binding = binding(101, 20_000);
        let authority = CandidateReferenceAuthority { binding };
        let first = definition(&authority, 0x1234, 200, 300, true);
        let duplicate = definition(&authority, 0x1234, 400, 500, false);
        let other = definition(&authority, 0x1235, 600, 700, true);
        let mut arena = PageArena::new();
        let (document, receipt, acks, terminal) =
            build_document(&mut arena, binding, vec![first, duplicate, other]);

        assert_eq!(acks.len(), 3);
        assert!(acks[0].inserted_first);
        assert_eq!(acks[0].winner_ordinal, 1);
        assert!(!acks[1].inserted_first);
        assert_eq!(acks[1].winner_ordinal, 1);
        assert!(acks[2].inserted_first);
        assert_eq!(acks[2].winner_ordinal, 3);
        assert!(
            acks.windows(2)
                .all(|pair| pair[0].occurrence_root != pair[1].occurrence_root
                    && pair[0].winner_root != pair[1].winner_root)
        );

        let descriptor = document.descriptor(&arena).unwrap();
        assert_eq!(descriptor.count, 3);
        assert_eq!(descriptor.high_water, 3);
        assert_eq!(descriptor.occurrence_generation, 3);
        assert_eq!(descriptor.winner_generation, 3);
        assert_eq!(descriptor.winner_entries, 2);
        assert_eq!(terminal.count, 3);
        assert_eq!(terminal.high_water, 3);
        assert_eq!(terminal.winner_entries, 2);

        let occurrences = document.occurrences(&arena).unwrap();
        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| (occurrence.ordinal, occurrence.label_id))
                .collect::<Vec<_>>(),
            vec![(1, 0x1234), (2, 0x1234), (3, 0x1235)]
        );
        assert_eq!(document.winner(&arena, 0x1234).unwrap().unwrap().ordinal, 1);
        assert_eq!(document.winner(&arena, 0x1235).unwrap().unwrap().ordinal, 3);
        assert_eq!(document.winner(&arena, 0x9999).unwrap(), None);
        assert!(receipt.maximum_live_owners_observed <= 5);
        reclaim_document(&mut arena, document);
    }

    #[test]
    fn giant_destination_and_title_remain_fixed_size_slice_descriptors() {
        let small_binding = binding(201, 20_000);
        let small_authority = CandidateReferenceAuthority {
            binding: small_binding,
        };
        let small = definition(&small_authority, 7, 200, 2_000, true);
        let mut small_arena = PageArena::new();
        let (small_document, small_receipt, _, _) =
            build_document(&mut small_arena, small_binding, vec![small]);

        let giant_extent = 1_u64 << 40;
        let giant_binding = binding(202, giant_extent);
        let giant_authority = CandidateReferenceAuthority {
            binding: giant_binding,
        };
        let giant = definition(&giant_authority, 7, 200, giant_extent - 200, true);
        let mut giant_arena = PageArena::new();
        let (giant_document, giant_receipt, _, _) =
            build_document(&mut giant_arena, giant_binding, vec![giant]);

        assert_eq!(small_receipt.nodes_allocated, giant_receipt.nodes_allocated);
        assert_eq!(
            small_receipt.payload_bytes_copied,
            giant_receipt.payload_bytes_copied
        );
        assert_eq!(
            small_receipt.edge_bytes_copied,
            giant_receipt.edge_bytes_copied
        );
        assert_eq!(small_receipt.mutations, giant_receipt.mutations);
        assert_eq!(
            small_receipt.maximum_live_owners_observed,
            giant_receipt.maximum_live_owners_observed
        );
        assert!(giant_receipt.maximum_live_owners_observed <= 5);
        assert!(size_of::<ReferenceOccurrenceDraft>() < 512);
        assert_eq!(giant_receipt.maximum_page_payload_bytes, OCCURRENCE_BYTES);

        let winner = giant_document.winner(&giant_arena, 7).unwrap().unwrap();
        assert!(
            winner.destination.source.end.bytes - winner.destination.source.start.bytes > 1 << 38
        );
        assert!(
            winner
                .title
                .is_some_and(|title| title.source.end.bytes - title.source.start.bytes > 1 << 38)
        );

        reclaim_document(&mut small_arena, small_document);
        reclaim_document(&mut giant_arena, giant_document);
    }

    #[test]
    fn item_ack_rejects_crossing_reorder_and_replay_without_consuming_valid_authority() {
        let binding_a = binding(301, 20_000);
        let authority_a = CandidateReferenceAuthority { binding: binding_a };
        let mut arena_a = PageArena::new();
        let ticket_a = arena_a.begin_build().unwrap();
        let build_a = ticket_a.id();
        let mut session_a = arena_a.resume_build(ticket_a).unwrap();
        let mut builder_a = ReferenceSemanticIndexBuilder::begin(build_a, binding_a).unwrap();
        let mut joiner_a = ReferenceIndexAckJoiner::new(build_a, binding_a);
        bootstrap(&mut builder_a, &mut session_a);
        builder_a
            .begin_item(definition(&authority_a, 11, 200, 300, true))
            .unwrap();
        while builder_a.phase != BuildPhase::ItemAckReady {
            builder_a.poll(&mut session_a).unwrap();
        }
        let mut ack_a = builder_a.take_item_ack().unwrap();

        let binding_b = binding(302, 20_000);
        let authority_b = CandidateReferenceAuthority { binding: binding_b };
        let mut arena_b = PageArena::new();
        let ticket_b = arena_b.begin_build().unwrap();
        let build_b = ticket_b.id();
        let mut session_b = arena_b.resume_build(ticket_b).unwrap();
        let mut builder_b = ReferenceSemanticIndexBuilder::begin(build_b, binding_b).unwrap();
        let mut joiner_b = ReferenceIndexAckJoiner::new(build_b, binding_b);
        bootstrap(&mut builder_b, &mut session_b);
        builder_b
            .begin_item(definition(&authority_b, 11, 200, 300, true))
            .unwrap();
        while builder_b.phase != BuildPhase::ItemAckReady {
            builder_b.poll(&mut session_b).unwrap();
        }
        let mut ack_b = builder_b.take_item_ack().unwrap();

        assert_eq!(
            joiner_b.join_item(&mut builder_b, &mut ack_a),
            Err(ReferenceSemanticIndexError::CrossedAck)
        );
        ack_a.inner.as_mut().unwrap().ordinal = 2;
        assert_eq!(
            joiner_a.join_item(&mut builder_a, &mut ack_a),
            Err(ReferenceSemanticIndexError::OutOfOrderAck {
                expected: 1,
                actual: 2,
            })
        );
        ack_a.inner.as_mut().unwrap().ordinal = 1;
        joiner_a.join_item(&mut builder_a, &mut ack_a).unwrap();
        assert_eq!(
            joiner_a.join_item(&mut builder_a, &mut ack_a),
            Err(ReferenceSemanticIndexError::ReplayedAck)
        );
        joiner_b.join_item(&mut builder_b, &mut ack_b).unwrap();

        let abort_a = session_a.begin_abort().unwrap();
        let abort_b = session_b.begin_abort().unwrap();
        drain_abort(&mut arena_a, abort_a);
        drain_abort(&mut arena_b, abort_b);
    }

    #[test]
    fn terminal_seal_binds_joined_count_high_water_and_root_generations() {
        let binding = binding(401, 20_000);
        let authority = CandidateReferenceAuthority { binding };
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut builder = ReferenceSemanticIndexBuilder::begin(build, binding).unwrap();
        let mut joiner = ReferenceIndexAckJoiner::new(build, binding);
        bootstrap(&mut builder, &mut session);
        append(
            &mut builder,
            &mut joiner,
            &mut session,
            definition(&authority, 13, 200, 300, true),
        );
        builder.begin_terminal().unwrap();
        while builder.phase != BuildPhase::TerminalSealReady {
            builder.poll(&mut session).unwrap();
        }
        let mut seal = builder.take_terminal_seal().unwrap();
        seal.inner.as_mut().unwrap().winner_generation = 2;
        assert!(matches!(
            joiner.join_terminal(&mut builder, &mut seal),
            Err(ReferenceSemanticIndexError::Invalid(_))
        ));
        seal.inner.as_mut().unwrap().winner_generation = 1;
        joiner.join_terminal(&mut builder, &mut seal).unwrap();
        assert_eq!(
            joiner.join_terminal(&mut builder, &mut seal),
            Err(ReferenceSemanticIndexError::ReplayedAck)
        );
        let document = builder.commit(session).unwrap();
        reclaim_document(&mut arena, document);
    }

    fn exercise_aborted_build(fault_at: Option<u64>, cancel_after: Option<u64>) {
        let binding = binding(501, 20_000);
        let authority = CandidateReferenceAuthority { binding };
        let mut drafts = vec![
            definition(&authority, 17, 400, 500, false),
            definition(&authority, 17, 200, 300, true),
        ];
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut builder = ReferenceSemanticIndexBuilder::begin(build, binding).unwrap();
        if let Some(mutation) = fault_at {
            builder = builder.with_fault_after_mutation(mutation);
        }
        let mut joiner = ReferenceIndexAckJoiner::new(build, binding);
        let mut items_joined = 0_u64;
        let mut terminal_started = false;

        loop {
            let progress = match builder.phase {
                BuildPhase::Ready if !drafts.is_empty() => {
                    builder.begin_item(drafts.pop().unwrap()).unwrap();
                    ReferenceSemanticIndexProgress::Pending
                }
                BuildPhase::ItemAckReady => {
                    let mut ack = builder.take_item_ack().unwrap();
                    joiner.join_item(&mut builder, &mut ack).unwrap();
                    items_joined += 1;
                    ReferenceSemanticIndexProgress::Pending
                }
                BuildPhase::Ready if drafts.is_empty() && !terminal_started => {
                    assert_eq!(items_joined, 2);
                    builder.begin_terminal().unwrap();
                    terminal_started = true;
                    ReferenceSemanticIndexProgress::Pending
                }
                BuildPhase::TerminalSealReady => {
                    let mut seal = builder.take_terminal_seal().unwrap();
                    joiner.join_terminal(&mut builder, &mut seal).unwrap();
                    ReferenceSemanticIndexProgress::Committable
                }
                BuildPhase::Committable => {
                    panic!("abort exercise reached commit without a selected boundary")
                }
                _ => match builder.poll(&mut session) {
                    Ok(progress) => progress,
                    Err(error) => {
                        assert_eq!(
                            error,
                            ReferenceSemanticIndexError::InjectedFault {
                                mutation: fault_at.expect("only injected faults may fail polling"),
                            }
                        );
                        let abort = session.begin_abort().unwrap();
                        drain_abort(&mut arena, abort);
                        return;
                    }
                },
            };
            let _ = progress;
            if cancel_after == Some(builder.receipt().mutations) {
                let abort = session.begin_abort().unwrap();
                drain_abort(&mut arena, abort);
                return;
            }
        }
    }

    #[test]
    fn cancellation_and_faults_at_every_mutation_boundary_publish_nothing() {
        let binding = binding(601, 20_000);
        let authority = CandidateReferenceAuthority { binding };
        let mut arena = PageArena::new();
        let (document, receipt, _, _) = build_document(
            &mut arena,
            binding,
            vec![
                definition(&authority, 17, 200, 300, true),
                definition(&authority, 17, 400, 500, false),
            ],
        );
        let mutation_boundaries = receipt.mutations;
        reclaim_document(&mut arena, document);
        assert!(mutation_boundaries > 30);

        for mutation in 1..=mutation_boundaries {
            exercise_aborted_build(None, Some(mutation));
            exercise_aborted_build(Some(mutation), None);
        }
    }

    #[test]
    fn source_and_label_authorities_cannot_be_crossed_before_storage() {
        let binding_a = binding(701, 20_000);
        let binding_b = binding(702, 20_000);
        let authority_a = CandidateReferenceAuthority { binding: binding_a };
        let authority_b = CandidateReferenceAuthority { binding: binding_b };
        let label = authority_a.bind_label(19).unwrap();
        let definition = authority_a
            .bind_slice(range(200, 300), range(100, 200), (0, 3))
            .unwrap();
        let destination = authority_b
            .bind_slice(range(201, 250), range(101, 150), (0, 2))
            .unwrap();
        assert!(matches!(
            authority_a.definition(label, definition, destination, None),
            Err(ReferenceSemanticIndexError::Invalid(_))
        ));
    }
}

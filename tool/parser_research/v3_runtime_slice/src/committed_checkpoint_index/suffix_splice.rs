//! Persistent donor-checkpoint convergence splice.
//!
//! This is the first page-native replacement primitive for the checkpoint
//! index. It admits either one exact direct-run partition or one zero-based
//! normalization group, plus an optional terminal tail, so the proof can
//! isolate the hard storage operation: retain full old sample pages on both
//! sides, re-encode only the two boundary leaves plus fresh samples, and let
//! measured persistent-sequence summaries rebase every later five-axis
//! checkpoint.
//!
//! Persisted donor records contain grammar/control plus bounded line-local
//! state, never cumulative revision output. The current revision's C sample is
//! always freshly encoded; old paths are retained only strictly after C.
//!
//! This remains a mechanism-only, unpublishable storage proof. Grammar
//! compatibility at C does not itself authorize a semantic tail: a product
//! caller must additionally join unchanged source-suffix lineage and fresh
//! convergence authority under the same composite parent.
//!
//! The existing `StorageOnlyCheckpointIndexBuilder` remains unchanged and
//! still collects a whole `Vec` of samples.  Production construction must
//! migrate to this resumable/page-native shape; this module does not hide a
//! second vector index beside the committed format.

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::ops::Range;

use flark_comrak_value_block_core::DirectDurableGrammarCapture;

#[allow(clippy::wildcard_imports)]
// Format-private child intentionally shares the parent's sealed codecs/specs.
use super::*;
use crate::arena::{ArenaBuildLifecycle, ArenaBuildTicket};
use crate::persistent_sequence::{ResumableSequenceSplice, ResumableSequenceSplitProgress};

/// Lexical token allowing this child module to join the actor's matched-live-C
/// certificate to its writer-owned suffix sample chain. Raw coordinates never
/// cross a general scheduler API.
pub(crate) struct ParentSelectedCheckpointSpliceMint(());

/// One local edit interval expressed against the old committed index.
///
/// The final capture and its witness are minted together here.  Callers cannot
/// pair a fresh interval with an unrelated raw donor byte string.
///
/// This remains a storage-mechanism input: it does **not** make caller-chosen
/// restart/convergence cuts authoritative.  The later composite parent must
/// mint both five-axis cuts from source/green/writer convergence tokens before
/// it is allowed to invoke this primitive.
#[derive(Debug)]
pub(crate) struct DonorSuffixSpliceRequest {
    restart_cut: RelativeCheckpointMeasure,
    old_convergence_cut: RelativeCheckpointMeasure,
    route: ParentSelectedCheckpointSpliceRoute,
    changed_samples: std::vec::IntoIter<DonorCheckpointSampleDraft>,
    final_sample: DonorCheckpointSampleDraft,
    final_continuation_identity: OpaqueDonorIdentityWitness,
    fresh_total: RelativeCheckpointMeasure,
    maximum_fresh_path_depth: usize,
    admission_fresh_samples_scanned: usize,
}

/// Storage route authenticated by the retained-parent successor chain.
///
/// The adjacent variant is minted only from a `ParentBoundDonorSuccessor`
/// that crossed the typed donor-to-donor transition in the parent index.  It
/// deliberately carries the exact persisted bindings instead of exposing an
/// ordinal or source-coordinate constructor to the actor.
#[derive(Debug)]
enum ParentSelectedCheckpointSpliceRoute {
    SameDirectPartition,
    AdjacentDonorPartitions(ParentSelectedAdjacentDonorSpliceAuthority),
}

#[derive(Debug)]
struct ParentSelectedAdjacentDonorSpliceAuthority {
    parent_root: ArenaScopedId,
    restart: DonorCheckpointAuthorityBinding,
    convergence: DonorCheckpointAuthorityBinding,
}

impl DonorSuffixSpliceRequest {
    pub(crate) fn try_new(
        restart_cut: RelativeCheckpointMeasure,
        old_convergence_cut: RelativeCheckpointMeasure,
        changed_samples: Vec<DonorCheckpointSampleDraft>,
        final_interval: RelativeCheckpointMeasure,
        final_capture: DirectDurableGrammarCapture,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        let (final_sample, final_continuation_identity) =
            DonorCheckpointSampleDraft::try_new_with_identity_witness(
                final_interval,
                final_capture,
            )?;
        // Mechanism-only callers have no linear actor accumulator, so this
        // compatibility constructor derives the aggregate once. Production
        // admission receives it incrementally from CandidateWriter instead.
        let mut fresh_total = final_sample.interval();
        let mut maximum_fresh_path_depth = final_sample.path_depth();
        for sample in &changed_samples {
            fresh_total = fresh_total.checked_add(sample.interval())?;
            maximum_fresh_path_depth = maximum_fresh_path_depth.max(sample.path_depth());
        }
        let admission_fresh_samples_scanned = changed_samples.len().saturating_add(1);
        Ok(Self {
            restart_cut,
            old_convergence_cut,
            route: ParentSelectedCheckpointSpliceRoute::SameDirectPartition,
            changed_samples: changed_samples.into_iter(),
            final_sample,
            final_continuation_identity,
            fresh_total,
            maximum_fresh_path_depth,
            admission_fresh_samples_scanned,
        })
    }

    /// Production-shaped direct-lane constructor. The selected parent owns R
    /// and old C, the successful grammar join owns the matched live-C
    /// certificate, and the writer owns the exact suffix-local sample chain.
    /// No caller supplies a cut, ordinal, interval, or donor capture.
    pub(crate) fn try_from_parent_selected_writer(
        anchor: &ParentSelectedSeededRestartAnchor,
        old_convergence: &ParentBoundDonorSuccessor,
        certificate: crate::parent_selected_convergence::ParentSelectedMatchedLiveSampleCertificate,
        writer: crate::candidate_writer::ParentSelectedWriterCheckpointTail,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        if anchor.parent_root != old_convergence.parent_root
            || anchor.recipe.authority != old_convergence.restart_authority
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-selected checkpoint splice crossed R, C, or retained parent",
            ));
        }
        let route = if anchor.recipe.authority.partition_manifest
            == old_convergence.recipe.authority.partition_manifest
        {
            if anchor.recipe.role != DonorPartitionRole::DirectRun
                || old_convergence.recipe.role != DonorPartitionRole::DirectRun
                || anchor.recipe.authority.partition_ordinal
                    != old_convergence.recipe.authority.partition_ordinal
            {
                return Err(CommittedCheckpointIndexError::Invalid(
                    "same-partition parent-selected splice is not one direct donor run",
                ));
            }
            ParentSelectedCheckpointSpliceRoute::SameDirectPartition
        } else {
            if old_convergence.recipe.role != DonorPartitionRole::DirectRun
                || anchor.recipe.authority.partition_ordinal.checked_add(1)
                    != Some(old_convergence.recipe.authority.partition_ordinal)
            {
                return Err(CommittedCheckpointIndexError::Invalid(
                    "cross-partition checkpoint splice is not one authenticated adjacent donor transition",
                ));
            }
            ParentSelectedCheckpointSpliceRoute::AdjacentDonorPartitions(
                ParentSelectedAdjacentDonorSpliceAuthority {
                    parent_root: anchor.parent_root,
                    restart: anchor.recipe.authority,
                    convergence: old_convergence.recipe.authority,
                },
            )
        };
        let (epoch, cursor, mut samples, fresh_total, maximum_fresh_path_depth) =
            writer.into_checkpoint_splice_parts(ParentSelectedCheckpointSpliceMint(()));
        let (certificate_epoch, final_interval, current_cut, sample_ordinal) =
            certificate.into_checkpoint_splice_parts(ParentSelectedCheckpointSpliceMint(()));
        if epoch != certificate_epoch
            || !cursor.matches_parent_anchor(anchor)
            || cursor.epoch() != epoch
            || cursor.cumulative_cut() != current_cut
            || cursor.sample_ordinal() != sample_ordinal
            || usize::try_from(sample_ordinal).ok() != Some(samples.len())
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "matched live C and writer checkpoint chain disagree",
            ));
        }
        if anchor.recipe.checkpoint_cut.checked_add(fresh_total)? != current_cut {
            return Err(CommittedCheckpointIndexError::Invalid(
                "writer checkpoint intervals do not reach matched live C",
            ));
        }
        let final_sample = samples.pop().ok_or(CommittedCheckpointIndexError::Invalid(
            "matched live C has no writer checkpoint sample",
        ))?;
        if final_sample.interval() != final_interval {
            return Err(CommittedCheckpointIndexError::Invalid(
                "matched live C interval differs from the writer's final sample",
            ));
        }
        let final_continuation_identity = final_sample
            .donor
            .identity_witness()
            .map_err(CommittedCheckpointIndexError::Allocation)?;
        Ok(Self {
            restart_cut: anchor.recipe.checkpoint_cut,
            old_convergence_cut: old_convergence.recipe.checkpoint_cut,
            route,
            changed_samples: samples.into_iter(),
            final_sample,
            final_continuation_identity,
            fresh_total,
            maximum_fresh_path_depth,
            admission_fresh_samples_scanned: 0,
        })
    }

    /// Detaches the only potentially long heap chain before a superseded
    /// suspended candidate drops the rest of this constant-size request.
    pub(crate) fn into_heap_retirement(self) -> DonorCheckpointHeapRetirement {
        DonorCheckpointHeapRetirement::from_segmented(
            self.changed_samples,
            Some(self.final_sample),
            None,
            None,
        )
    }
}

/// One-shot proof that two independently revalidated checkpoints select the
/// same committed normalization partition, group, outcome, and bounds.
///
/// No scalar group/outcome constructor exists.  The copied binding remains
/// useful only after the splice job revalidates it against the still-live old
/// root and exact manifest child.
#[derive(Debug)]
pub(crate) struct NormalizationDonorSuffixSpliceAuthority {
    binding: CommittedNormalizationGroupBinding,
    restart_cut: RelativeCheckpointMeasure,
    convergence_cut: RelativeCheckpointMeasure,
}

impl NormalizationDonorSuffixSpliceAuthority {
    #[allow(
        clippy::needless_pass_by_value,
        reason = "consume two non-cloneable role proofs so neither can authorize another splice"
    )]
    pub(crate) fn try_new(
        restart: CommittedNormalizationOutcomeCapability<'_>,
        convergence: CommittedNormalizationOutcomeCapability<'_>,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        let restart_cut = restart.recipe.checkpoint_cut;
        let convergence_cut = convergence.recipe.checkpoint_cut;
        if restart.binding != convergence.binding {
            return Err(CommittedCheckpointIndexError::Invalid(
                "normalization splice checkpoints cross group, outcome, or partition",
            ));
        }
        if restart_cut.source_bytes() == 0
            || restart_cut.source_bytes() >= convergence_cut.source_bytes()
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "normalization splice convergence must follow restart",
            ));
        }
        Ok(Self {
            binding: restart.binding,
            restart_cut,
            convergence_cut,
        })
    }

    /// Production-shaped mint from two independently authenticated
    /// restart-parent queries. Neither input exposes group or outcome scalars;
    /// equality is established only over their copied committed bindings.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "consume two parent-bound one-shot proofs after copying only their sealed binding"
    )]
    pub(crate) fn try_new_parent_bound(
        restart: ParentBoundNormalizationCheckpoint,
        convergence: ParentBoundNormalizationCheckpoint,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        if restart.binding != convergence.binding {
            return Err(CommittedCheckpointIndexError::Invalid(
                "normalization splice checkpoints cross group, outcome, or partition",
            ));
        }
        if restart.checkpoint_cut.source_bytes() == 0
            || restart.checkpoint_cut.source_bytes() >= convergence.checkpoint_cut.source_bytes()
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "normalization splice convergence must follow restart",
            ));
        }
        Ok(Self {
            binding: restart.binding,
            restart_cut: restart.checkpoint_cut,
            convergence_cut: convergence.checkpoint_cut,
        })
    }
}

#[allow(
    clippy::large_enum_variant,
    reason = "admission-only stack value preserves fallible-allocation discipline before polling"
)]
#[derive(Debug)]
enum ExpectedPartitionRole {
    Direct,
    Normalization(NormalizationDonorSuffixSpliceAuthority),
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "orthogonal proof receipts are intentionally explicit and independently asserted"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DonorSuffixSpliceReceipt {
    pub(crate) old_samples: u64,
    pub(crate) old_sample_leaf_pages: u64,
    pub(crate) old_samples_replaced: u64,
    pub(crate) fresh_samples_inserted: usize,
    /// Production actor admission is zero: cumulative interval/path facts are
    /// folded as samples are captured, never by rescanning the matched chain.
    pub(crate) admission_fresh_samples_scanned: usize,
    /// Always zero after the segmented iterator migration: fresh drafts stay
    /// in their original allocation and are consumed one per poll.
    pub(crate) admission_fresh_samples_requeued: usize,
    pub(crate) admission_fresh_chain_capacity_allocated: usize,
    pub(crate) sample_tree_nodes_visited: usize,
    pub(crate) boundary_leaf_pages_decoded: usize,
    pub(crate) boundary_sample_records_decoded: usize,
    pub(crate) boundary_leaf_pages_reencoded: usize,
    pub(crate) replacement_leaf_pages: usize,
    pub(crate) old_leaf_pages_retained: u64,
    pub(crate) source_bytes_retained: u64,
    pub(crate) source_bytes_replaced: u64,
    pub(crate) source_bytes_inserted: u64,
    /// Maximum path depth across every freshly encoded sample, including the
    /// current revision's convergence sample C. Old paths are retained only
    /// strictly after C.
    pub(crate) maximum_fresh_path_depth: usize,
    /// The old and fresh C records have exact canonical identity over every
    /// suffix-relevant continuation field: grammar plus predecessor line-local
    /// state. Revision-cumulative child folds and display facts are absent from
    /// this codec by construction.
    pub(crate) suffix_relevant_continuation_identity: bool,
    pub(crate) normalization_role_preserved: bool,
    /// The first normalization proof admits exactly one donor group whose
    /// absolute document start is zero. Multi-partition group splicing remains
    /// unavailable and fails closed at admission.
    pub(crate) normalization_group_start_zero_storage_shape: bool,
    pub(crate) terminal_tail_retained: bool,
    /// This primitive proves page-native storage mutation only. Its result is
    /// not publishable restart authority without the later source/green/writer
    /// convergence join.
    pub(crate) mechanism_only_unpublishable: bool,
    pub(crate) retained_source_bytes: usize,
    pub(crate) sequence: SequenceMutationReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DonorSuffixSpliceProgress {
    Pending,
    Complete,
}

#[derive(Debug)]
enum ReplacementSample {
    Existing {
        interval: RelativeCheckpointMeasure,
        header: OpaqueDonorHeader,
        path_terminal: ArenaId,
    },
    Fresh(DonorCheckpointSampleDraft),
}

/// Four ordered sources for one replacement boundary. The potentially large
/// fresh chain stays in its original `Vec::IntoIter`; admission never copies
/// or requeues all `k` drafts before the first cooperative poll.
#[derive(Debug)]
struct ReplacementSamples {
    existing_prefix: VecDeque<ReplacementSample>,
    changed: std::vec::IntoIter<DonorCheckpointSampleDraft>,
    final_sample: Option<DonorCheckpointSampleDraft>,
    existing_suffix: VecDeque<ReplacementSample>,
}

impl ReplacementSamples {
    fn pop_front(&mut self) -> Option<ReplacementSample> {
        self.existing_prefix
            .pop_front()
            .or_else(|| self.changed.next().map(ReplacementSample::Fresh))
            .or_else(|| self.final_sample.take().map(ReplacementSample::Fresh))
            .or_else(|| self.existing_suffix.pop_front())
    }

    fn is_empty(&self) -> bool {
        self.existing_prefix.is_empty()
            && self.changed.len() == 0
            && self.final_sample.is_none()
            && self.existing_suffix.is_empty()
    }
}

#[derive(Debug)]
struct LocatedBoundaryLeaf {
    leaf: ArenaId,
    leaf_index: u64,
    record_index: usize,
    sample_ordinal: u64,
    sample_prefix: RelativeCheckpointMeasure,
    records: Vec<DecodedDonorSampleRecord>,
    paths: Vec<ArenaId>,
    nodes_visited: usize,
}

#[derive(Debug)]
struct LocatedOuterBoundaryLeaf {
    leaf: ArenaId,
    leaf_index: u64,
    record_index: usize,
    records: Vec<DecodedPartitionRecord>,
    children: Vec<ArenaId>,
    nodes_visited: usize,
}

impl LocatedBoundaryLeaf {
    fn selected(
        &self,
    ) -> Result<(DecodedDonorSampleRecord, ArenaId), CommittedCheckpointIndexError> {
        Ok((
            *self
                .records
                .get(self.record_index)
                .ok_or(CommittedCheckpointIndexError::Corrupt(
                    "selected donor boundary record disappeared",
                ))?,
            *self
                .paths
                .get(self.record_index)
                .ok_or(CommittedCheckpointIndexError::Corrupt(
                    "selected donor boundary path disappeared",
                ))?,
        ))
    }
}

#[derive(Debug)]
enum SplicePhase {
    EncodeReplacement,
    BuildFreshPath,
    PollReplacementPush,
    BeginReplacementFinish,
    PollReplacementFinish,
    BeginSplice,
    PollSplice,
    AllocateManifest,
    AllocateOuterReplacement,
    BeginOuterSplice,
    PollOuterSplice,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FreshPathPhase {
    CompareCommonPrefix,
    PrepareReplacement,
    AllocateUnmatchedFrame,
    InstallTerminal,
    RememberFrames,
    EmitSample,
}

/// One fresh donor sample whose path is being installed into `DonorPathCache`.
///
/// The frame vector moves directly out of the typed donor draft. Common-prefix
/// comparison, unmatched-node allocation, cache-frame replacement, and final
/// sample emission are separate poll transitions. In particular,
/// `AllocateUnmatchedFrame` allocates at most one arena node before yielding.
#[derive(Debug)]
struct ResumableFreshSample {
    interval: RelativeCheckpointMeasure,
    header: OpaqueDonorHeader,
    frames: Vec<OpaqueDonorFrame>,
    common: usize,
    compare_limit: usize,
    previous_frame_count: usize,
    next_allocation: usize,
    prior: Option<ArenaId>,
    newest_owner: Option<ArenaBuildOwner>,
    replacement_terminal: Option<ArenaBuildOwner>,
    remember_index: usize,
    phase: FreshPathPhase,
}

/// Arena-journal-owned, resumable checkpoint splice.
///
/// The borrow keeps the old committed root alive until the new sample tree has
/// retained it.  Each poll encodes at most one sample, allocates one leaf, or
/// advances one persistent-sequence state-machine step.
#[derive(Debug)]
pub(crate) struct DonorSuffixSpliceJob<'old> {
    core: DonorSuffixSpliceCore,
    _source_guard: PhantomData<&'old StorageOnlyCommittedCheckpointIndex>,
}

/// A rejected cancellation returns both linear capabilities unchanged.
///
/// In particular, an actor that accidentally presents a session from another
/// build can abort that unrelated session separately, then resume or cancel
/// this splice with its original authority. Keeping the recovery carrier
/// local to this mechanism avoids weakening the arena's fail-closed session
/// API or turning this storage-only job into publication authority.
#[derive(Debug)]
pub(crate) struct DonorSuffixSpliceCancellationFailure<'session, Job> {
    pub(crate) error: CommittedCheckpointIndexError,
    job: Job,
    session: ArenaBuildSession<'session>,
}

impl<'session, Job> DonorSuffixSpliceCancellationFailure<'session, Job> {
    pub(crate) fn into_parts(
        self,
    ) -> (
        CommittedCheckpointIndexError,
        Job,
        ArenaBuildSession<'session>,
    ) {
        (self.error, self.job, self.session)
    }
}

/// One independently retained old checkpoint root owned by the candidate
/// build journal.
///
/// Unlike `ParentRetainedCheckpointIndexLease`, this capability contains no
/// borrow into the composite adoption lease. It can therefore live in an
/// actor slot across arbitrary suspend/resume turns. The root and complete
/// descriptor are revalidated on every poll before the splice core advances.
#[derive(Debug)]
struct OwnedParentCheckpointSpliceBase {
    build: ArenaBuildId,
    parent_activation: ArenaScopedId,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
    owner: ArenaBuildOwner,
}

/// Actor-storable sibling of `DonorSuffixSpliceJob`.
///
/// Admission borrows the parent only long enough to validate it and retain
/// one additional checkpoint-root owner in the same journal. Thereafter this
/// job is lifetime-free. Completion releases that extra base after the new
/// manifest is journal-owned; cancellation aborts the entire journal and is
/// therefore safe at every intermediate state.
#[derive(Debug)]
pub(crate) struct ParentOwnedDonorSuffixSpliceJob {
    old_base: Option<OwnedParentCheckpointSpliceBase>,
    core: DonorSuffixSpliceCore,
}

/// Storage state shared by the borrowed mechanism proof and its actor-storable
/// parent-owned sibling. This type is private so a caller can never advance it
/// without one of the two old-root lifetime capabilities above.
#[derive(Debug)]
struct DonorSuffixSpliceCore {
    build: ArenaBuildId,
    old_outer_root: ArenaId,
    old_sample_root: ArenaId,
    partition_role: DonorPartitionRole,
    replaced_leaf_range: Range<u64>,
    replaced_outer_leaf_range: Range<u64>,
    outer_boundary_records: Vec<DecodedPartitionRecord>,
    outer_boundary_children: Vec<ArenaId>,
    outer_target_record: usize,
    samples: ReplacementSamples,
    pending_fresh_sample: Option<ResumableFreshSample>,
    page: DonorSampleLeafEncoder,
    page_payload_capacity: usize,
    page_path_owner_capacity: usize,
    path_cache: DonorPathCache,
    path_cache_frame_capacity: usize,
    path_cache_node_capacity: usize,
    path_ids_scratch: Vec<ArenaId>,
    path_ids_scratch_capacity: usize,
    outer_payload: Vec<u8>,
    outer_payload_capacity: usize,
    replacement_sequence: ResumableStreamingSequenceBuilder<DonorSampleSpec>,
    splice: ResumableSequenceSplice<DonorSampleSpec>,
    outer_splice: ResumableSequenceSplice<CheckpointIndexSpec>,
    replacement_root: Option<ArenaBuildOwner>,
    spliced_root: Option<ArenaBuildOwner>,
    manifest: Option<ArenaBuildOwner>,
    outer_replacement_root: Option<ArenaBuildOwner>,
    output: Option<StorageOnlyCheckpointIndexBuildManifest>,
    phase: SplicePhase,
    build_receipt: CommittedCheckpointIndexBuildReceipt,
    receipt: DonorSuffixSpliceReceipt,
}

impl<'old> DonorSuffixSpliceJob<'old> {
    #[allow(clippy::too_many_lines)] // Admission binds every old-page and convergence invariant once.
    pub(crate) fn try_new(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        old: &'old StorageOnlyCommittedCheckpointIndex,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        if arena.build_lifecycle(ticket.id())? != ArenaBuildLifecycle::Suspended {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor suffix splice requires a suspended arena build",
            ));
        }
        let old_root = old.checked_root_id(arena)?;
        let core = DonorSuffixSpliceCore::try_new_from_root(
            ticket.id(),
            arena,
            old_root,
            ExpectedPartitionRole::Direct,
            request,
        )?;
        Ok(Self {
            core,
            _source_guard: PhantomData,
        })
    }

    /// Normalization-group admission consumes an opaque authority minted from
    /// the exact parent-selected restart and convergence recipes.  Caller
    /// supplied group IDs or outcomes never cross this boundary.
    pub(crate) fn try_new_normalization(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        old: &'old StorageOnlyCommittedCheckpointIndex,
        authority: NormalizationDonorSuffixSpliceAuthority,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        if arena.build_lifecycle(ticket.id())? != ArenaBuildLifecycle::Suspended {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor suffix splice requires a suspended arena build",
            ));
        }
        let old_root = old.checked_root_id(arena)?;
        let core = DonorSuffixSpliceCore::try_new_from_root(
            ticket.id(),
            arena,
            old_root,
            ExpectedPartitionRole::Normalization(authority),
            request,
        )?;
        Ok(Self {
            core,
            _source_guard: PhantomData,
        })
    }

    /// Parent-integrated admission.  The parent lease keeps the already
    /// retained checkpoint child opaque while this child module revalidates
    /// its exact build owner and descriptor.
    pub(crate) fn try_new_from_parent<'parent>(
        session: &ArenaBuildSession<'_>,
        parent: &'old ParentRetainedCheckpointIndexLease<'parent>,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        parent.validate_session(session)?;
        let old_root = parent.validated_root(session)?;
        let core = DonorSuffixSpliceCore::try_new_from_root(
            session.id(),
            session.arena(),
            old_root,
            ExpectedPartitionRole::Direct,
            request,
        )?;
        Ok(Self {
            core,
            _source_guard: PhantomData,
        })
    }

    /// Parent-retained normalization storage admission. The retained child
    /// remains opaque and is revalidated from the two-child adoption journal
    /// before the authenticated normalization binding is checked against it.
    /// This does not establish unchanged source-suffix lineage and therefore
    /// cannot publish the output as a semantic restart tail.
    ///
    /// The first storage shape is intentionally one zero-based normalization
    /// group plus an optional terminal tail. Cross-partition cuts fail closed.
    pub(crate) fn try_new_normalization_from_parent<'parent>(
        session: &ArenaBuildSession<'_>,
        parent: &'old ParentRetainedCheckpointIndexLease<'parent>,
        authority: NormalizationDonorSuffixSpliceAuthority,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        parent.validate_session(session)?;
        let old_root = parent.validated_root(session)?;
        let core = DonorSuffixSpliceCore::try_new_from_root(
            session.id(),
            session.arena(),
            old_root,
            ExpectedPartitionRole::Normalization(authority),
            request,
        )?;
        Ok(Self {
            core,
            _source_guard: PhantomData,
        })
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.core.build_id()
    }

    #[must_use]
    pub(crate) fn receipt(&self) -> DonorSuffixSpliceReceipt {
        self.core.receipt()
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<DonorSuffixSpliceProgress, CommittedCheckpointIndexError> {
        self.core.poll(session)
    }

    pub(crate) fn take_manifest(
        &mut self,
    ) -> Result<StorageOnlyCheckpointIndexBuildManifest, CommittedCheckpointIndexError> {
        self.core.take_manifest()
    }

    pub(crate) fn cancel<'session>(
        self,
        session: ArenaBuildSession<'session>,
    ) -> Result<ArenaBuildId, DonorSuffixSpliceCancellationFailure<'session, Self>> {
        if session.id() != self.core.build_id() {
            return Err(DonorSuffixSpliceCancellationFailure {
                error: CommittedCheckpointIndexError::Invalid(
                    "arena session belongs to another donor suffix splice build",
                ),
                job: self,
                session,
            });
        }
        Ok(self.core.cancel(session))
    }
}

impl OwnedParentCheckpointSpliceBase {
    /// Retains only after every fallible read-side admission check has passed.
    /// A failed retain changes no logical journal state; after success this
    /// owner is reclaimed either by `release_after_completion` or whole-build
    /// cancellation.
    fn retain_validated(
        session: &mut ArenaBuildSession<'_>,
        parent: &ParentRetainedCheckpointIndexLease<'_>,
        root: ArenaId,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        let build = session.id();
        let parent_activation = parent.parent_activation;
        let descriptor = parent.descriptor;
        if parent.build != build || descriptor.root != root {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "validated parent checkpoint root changed before retention",
            ));
        }
        let owner = session.retain(root)?;
        Ok(Self {
            build,
            parent_activation,
            descriptor,
            owner,
        })
    }

    fn validate_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), CommittedCheckpointIndexError> {
        if session.id() != self.build {
            return Err(CommittedCheckpointIndexError::Invalid(
                "owned checkpoint splice base and arena build differ",
            ));
        }
        session.arena().local_id(self.parent_activation)?;
        let root = session.owner_id(&self.owner)?;
        if root != self.descriptor.root {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "owned checkpoint splice base root changed",
            ));
        }
        let descriptor =
            validate_committed_checkpoint_index_composite_child(session.arena(), root)?;
        if descriptor != self.descriptor {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "owned checkpoint splice base descriptor changed",
            ));
        }
        Ok(())
    }

    fn release_after_completion(
        self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), CommittedCheckpointIndexError> {
        self.validate_session(session)?;
        session.release(self.owner)?;
        Ok(())
    }
}

impl ParentOwnedDonorSuffixSpliceJob {
    /// Parent-integrated admission that turns the borrowed parent child into a
    /// lifetime-free, journal-owned base before returning to the actor.
    pub(crate) fn try_new_from_parent(
        session: &mut ArenaBuildSession<'_>,
        parent: &ParentRetainedCheckpointIndexLease<'_>,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        parent.validate_session(session)?;
        let old_root = parent.validated_root(session)?;
        let core = DonorSuffixSpliceCore::try_new_from_root(
            session.id(),
            session.arena(),
            old_root,
            ExpectedPartitionRole::Direct,
            request,
        )?;
        let old_base =
            OwnedParentCheckpointSpliceBase::retain_validated(session, parent, old_root)?;
        Ok(Self {
            old_base: Some(old_base),
            core,
        })
    }

    /// Normalization sibling with the same actor-storable ownership shape.
    pub(crate) fn try_new_normalization_from_parent(
        session: &mut ArenaBuildSession<'_>,
        parent: &ParentRetainedCheckpointIndexLease<'_>,
        authority: NormalizationDonorSuffixSpliceAuthority,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        parent.validate_session(session)?;
        let old_root = parent.validated_root(session)?;
        let core = DonorSuffixSpliceCore::try_new_from_root(
            session.id(),
            session.arena(),
            old_root,
            ExpectedPartitionRole::Normalization(authority),
            request,
        )?;
        let old_base =
            OwnedParentCheckpointSpliceBase::retain_validated(session, parent, old_root)?;
        Ok(Self {
            old_base: Some(old_base),
            core,
        })
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.core.build_id()
    }

    #[must_use]
    pub(crate) fn receipt(&self) -> DonorSuffixSpliceReceipt {
        self.core.receipt()
    }

    /// Revalidates the independently owned old base on each actor slice. The
    /// final successful poll releases that one extra owner, leaving only the
    /// parent's original children and the new index output in the journal.
    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<DonorSuffixSpliceProgress, CommittedCheckpointIndexError> {
        if let Some(base) = &self.old_base {
            base.validate_session(session)?;
        }
        let progress = self.core.poll(session)?;
        if progress == DonorSuffixSpliceProgress::Complete {
            if let Some(base) = self.old_base.take() {
                if let Err(error) = base.release_after_completion(session) {
                    self.core.phase = SplicePhase::Failed;
                    return Err(error);
                }
            }
        }
        Ok(progress)
    }

    pub(crate) fn take_manifest(
        &mut self,
    ) -> Result<StorageOnlyCheckpointIndexBuildManifest, CommittedCheckpointIndexError> {
        if self.old_base.is_some() {
            return Err(CommittedCheckpointIndexError::Invalid(
                "owned checkpoint splice base was not released at completion",
            ));
        }
        self.core.take_manifest()
    }

    pub(crate) fn cancel<'session>(
        self,
        session: ArenaBuildSession<'session>,
    ) -> Result<ArenaBuildId, DonorSuffixSpliceCancellationFailure<'session, Self>> {
        if session.id() != self.core.build_id() {
            return Err(DonorSuffixSpliceCancellationFailure {
                error: CommittedCheckpointIndexError::Invalid(
                    "arena session belongs to another donor suffix splice build",
                ),
                job: self,
                session,
            });
        }
        Ok(self.core.cancel(session))
    }

    /// Detaches unencoded fresh drafts from a suspended or already-aborting
    /// actor job. Journal-owned pages remain governed by the arena abort.
    pub(crate) fn into_heap_retirement(self) -> DonorCheckpointHeapRetirement {
        self.core.into_heap_retirement()
    }
}

impl DonorSuffixSpliceCore {
    #[allow(clippy::too_many_lines)] // Admission binds every old-page and convergence invariant once.
    fn try_new_from_root(
        build: ArenaBuildId,
        arena: &PageArena,
        old_root: ArenaId,
        expected_role: ExpectedPartitionRole,
        request: DonorSuffixSpliceRequest,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        let old_summary = sequence_node::<CheckpointIndexSpec>(arena, old_root)?.0;
        let scoped_root = arena.scoped_query_id(old_root)?;

        // The same-partition mechanism locates its donor from R.  The
        // cross-partition lane instead consumes the exact adjacent bindings
        // carried by the parent successor chain and revalidates both sides.
        let (target_partition, relative_restart_cut, relative_convergence_cut, cross_partition) =
            match &request.route {
                ParentSelectedCheckpointSpliceRoute::SameDirectPartition => {
                    let selected_byte = request.restart_cut.source_bytes().checked_sub(1).ok_or(
                        CommittedCheckpointIndexError::Invalid(
                            "donor splice restart is not a sample endpoint",
                        ),
                    )?;
                    let partition = locate_outer_partition(arena, old_root, selected_byte)?;
                    let relative_restart = request
                        .restart_cut
                        .checked_difference_from(partition.prefix)?;
                    let relative_convergence = request
                        .old_convergence_cut
                        .checked_difference_from(partition.prefix)?;
                    (partition, relative_restart, relative_convergence, false)
                }
                ParentSelectedCheckpointSpliceRoute::AdjacentDonorPartitions(authority) => {
                    if authority.restart.index_root != scoped_root
                        || authority.convergence.index_root != scoped_root
                        || authority.restart.partition_ordinal.checked_add(1)
                            != Some(authority.convergence.partition_ordinal)
                    {
                        return Err(CommittedCheckpointIndexError::Invalid(
                            "adjacent donor splice authority belongs to another index or is nonadjacent",
                        ));
                    }
                    let restart_partition = locate_outer_partition_by_ordinal(
                        arena,
                        old_root,
                        authority.restart.partition_ordinal,
                    )?;
                    let LocatedCheckpointPartitionKind::Donor(restart_donor) =
                        restart_partition.kind
                    else {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "adjacent splice restart partition is no longer a donor",
                        ));
                    };
                    if restart_partition.prefix != authority.restart.partition_prefix
                        || restart_donor.manifest != authority.restart.partition_manifest
                    {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "adjacent splice restart partition binding changed",
                        ));
                    }
                    let restart_manifest =
                        decode_donor_partition_manifest(arena, restart_donor.manifest)?;
                    let restart_sample = locate_donor_sample_by_ordinal(
                        arena,
                        restart_manifest.sample_root,
                        authority.restart.sample_ordinal,
                    )?;
                    let restart_end = restart_partition
                        .prefix
                        .checked_add(restart_sample.prefix)?
                        .checked_add(restart_sample.interval)?;
                    if authority.restart.sample_ordinal.checked_add(1)
                        != Some(restart_manifest.samples)
                        || restart_sample.header != authority.restart.sample_header
                        || restart_sample.path_terminal != authority.restart.sample_path_terminal
                        || restart_end != request.restart_cut
                        || restart_end
                            != restart_partition
                                .prefix
                                .checked_add(restart_partition.interval)?
                    {
                        return Err(CommittedCheckpointIndexError::Invalid(
                            "cross-partition splice requires R to be the authenticated final restart-partition sample",
                        ));
                    }

                    let partition = locate_outer_partition_by_ordinal(
                        arena,
                        old_root,
                        authority.convergence.partition_ordinal,
                    )?;
                    if partition.prefix != authority.convergence.partition_prefix
                        || partition.prefix != restart_end
                    {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "adjacent splice target no longer follows R",
                        ));
                    }
                    let relative_convergence = request
                        .old_convergence_cut
                        .checked_difference_from(partition.prefix)?;
                    (
                        partition,
                        RelativeCheckpointMeasure::default(),
                        relative_convergence,
                        true,
                    )
                }
            };
        let LocatedCheckpointPartitionKind::Donor(target_donor) = target_partition.kind else {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor suffix splice target is not an exact donor partition",
            ));
        };
        let old_manifest = target_donor.manifest;
        let donor_manifest = decode_donor_partition_manifest(arena, old_manifest)?;
        if donor_manifest.measure != target_partition.interval
            || donor_manifest.samples != target_donor.samples
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor suffix splice record and manifest disagree",
            ));
        }
        if let ParentSelectedCheckpointSpliceRoute::AdjacentDonorPartitions(authority) =
            &request.route
        {
            if authority.parent_root.arena() != scoped_root.arena()
                || authority.convergence.partition_manifest != old_manifest
            {
                return Err(CommittedCheckpointIndexError::Invalid(
                    "adjacent donor splice target binding changed",
                ));
            }
        }
        match expected_role {
            ExpectedPartitionRole::Direct => {
                if donor_manifest.role != DonorPartitionRole::DirectRun {
                    return Err(CommittedCheckpointIndexError::Invalid(
                        "direct donor suffix splice received a normalization partition",
                    ));
                }
            }
            ExpectedPartitionRole::Normalization(authority) => {
                let DonorPartitionRole::Normalization { group, outcome } = donor_manifest.role
                else {
                    return Err(CommittedCheckpointIndexError::Invalid(
                        "normalization donor suffix splice received a direct partition",
                    ));
                };
                let expected_end = authority
                    .binding
                    .bounds
                    .start
                    .checked_add(donor_manifest.measure)?;
                if authority.binding.index_root.local() != old_root
                    || authority.binding.index_root.arena() != arena.identity()
                    || authority.binding.partition_manifest != old_manifest
                    || authority.binding.group != group
                    || authority.binding.outcome != outcome
                    || authority.binding.bounds.start != target_partition.prefix
                    || authority.binding.bounds.interval != donor_manifest.measure
                    || authority.binding.bounds.end != expected_end
                    || authority.restart_cut != request.restart_cut
                    || authority.convergence_cut != request.old_convergence_cut
                {
                    return Err(CommittedCheckpointIndexError::Invalid(
                        "normalization splice authority does not bind this group and cuts",
                    ));
                }
            }
        }

        let restart = if cross_partition {
            None
        } else {
            Some(locate_exact_endpoint(
                arena,
                donor_manifest.sample_root,
                relative_restart_cut,
            )?)
        };
        let convergence =
            locate_exact_endpoint(arena, donor_manifest.sample_root, relative_convergence_cut)?;
        if restart
            .as_ref()
            .is_some_and(|restart| restart.sample_ordinal >= convergence.sample_ordinal)
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor convergence must follow the restart sample",
            ));
        }
        if let ParentSelectedCheckpointSpliceRoute::AdjacentDonorPartitions(authority) =
            &request.route
        {
            let (record, path) = convergence.selected()?;
            if convergence.sample_ordinal != authority.convergence.sample_ordinal
                || record.header != authority.convergence.sample_header
                || path != authority.convergence.sample_path_terminal
            {
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "adjacent donor splice convergence binding changed",
                ));
            }
        }

        let (convergence_record, convergence_path) = convergence.selected()?;
        let header = IndexedDonorCheckpointRecipe::validate_header(&convergence_record.header)
            .map_err(|_| {
                CommittedCheckpointIndexError::Corrupt(
                    "donor rejected old convergence header schema",
                )
            })?;
        let reconstructed = reconstruct_donor_path(arena, convergence_path)?;
        let convergence_recipe =
            IndexedDonorCheckpointRecipe::from_validated_storage(header, reconstructed.frames)
                .map_err(|_| {
                    CommittedCheckpointIndexError::Corrupt(
                        "donor rejected old convergence path schema",
                    )
                })?;
        // Exact identity of the canonical grammar+line-local codec is the
        // suffix-relevant convergence key. The final draft and witness were
        // minted together from one consumed capture; callers cannot pair raw
        // persisted bytes themselves. Display facts and cumulative child folds
        // never enter this comparison.
        if !convergence_recipe.matches_identity_witness(&request.final_continuation_identity) {
            return Err(CommittedCheckpointIndexError::Invalid(
                "suffix mechanism requires identical grammar and line-local continuation at C",
            ));
        }
        let changed_count = request.changed_samples.len();
        let maximum_fresh_path_depth = request.maximum_fresh_path_depth;
        let source_bytes_inserted = request.fresh_total.source_bytes();
        let admission_fresh_samples_scanned = request.admission_fresh_samples_scanned;
        let fresh_samples_inserted =
            changed_count
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "fresh sample count",
                ))?;
        let suffix_records = convergence
            .records
            .len()
            .saturating_sub(convergence.record_index + 1);
        let prefix_records = restart
            .as_ref()
            .map_or(0, |restart| restart.record_index.saturating_add(1));
        let mut existing_prefix = VecDeque::new();
        existing_prefix.try_reserve(prefix_records).map_err(|_| {
            CommittedCheckpointIndexError::Allocation("replacement sample descriptors")
        })?;
        if let Some(restart) = &restart {
            append_existing(
                &mut existing_prefix,
                &restart.records,
                &restart.paths,
                0..restart.record_index + 1,
            )?;
        }
        let mut existing_suffix = VecDeque::new();
        existing_suffix.try_reserve(suffix_records).map_err(|_| {
            CommittedCheckpointIndexError::Allocation("replacement sample descriptors")
        })?;
        if let Some(restart) = &restart
            && restart.leaf == convergence.leaf
        {
            append_existing(
                &mut existing_suffix,
                &restart.records,
                &restart.paths,
                convergence.record_index + 1..restart.records.len(),
            )?;
        } else {
            append_existing(
                &mut existing_suffix,
                &convergence.records,
                &convergence.paths,
                convergence.record_index + 1..convergence.records.len(),
            )?;
        }
        // C belongs to the fresh revision and is always encoded from its fresh
        // grammar capture. Only old samples strictly after C may be retained.
        let samples = ReplacementSamples {
            existing_prefix,
            changed: request.changed_samples,
            final_sample: Some(request.final_sample),
            existing_suffix,
        };

        let replaced_leaf_end = convergence.leaf_index.checked_add(1).ok_or(
            CommittedCheckpointIndexError::Overflow("replaced leaf range end"),
        )?;
        let replaced_leaf_range =
            restart.as_ref().map_or(0, |restart| restart.leaf_index)..replaced_leaf_end;
        let replaced_leaf_pages = replaced_leaf_range.end - replaced_leaf_range.start;
        let sample_tree_summary =
            sequence_node::<DonorSampleSpec>(arena, donor_manifest.sample_root)?.0;
        let old_leaf_pages_retained = sample_tree_summary
            .leaf_pages
            .checked_sub(replaced_leaf_pages)
            .ok_or(CommittedCheckpointIndexError::Corrupt(
                "replaced donor leaf range exceeds tree",
            ))?;
        let old_samples_replaced = if let Some(restart) = &restart {
            convergence
                .sample_ordinal
                .checked_sub(restart.sample_ordinal)
                .ok_or(CommittedCheckpointIndexError::Corrupt(
                    "donor convergence ordinal regressed",
                ))?
        } else {
            convergence.sample_ordinal.checked_add(1).ok_or(
                CommittedCheckpointIndexError::Overflow("replaced target donor samples"),
            )?
        };
        let source_bytes_replaced = request
            .old_convergence_cut
            .source_bytes()
            .checked_sub(request.restart_cut.source_bytes())
            .ok_or(CommittedCheckpointIndexError::Invalid(
                "donor convergence source cut regresses",
            ))?;
        let suffix_source_bytes = donor_manifest
            .measure
            .source_bytes()
            .checked_sub(relative_convergence_cut.source_bytes())
            .ok_or(CommittedCheckpointIndexError::Invalid(
                "old convergence exceeds donor partition",
            ))?;
        let source_bytes_retained = relative_restart_cut
            .source_bytes()
            .checked_add(suffix_source_bytes)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "retained source bytes",
            ))?;

        let mut build_receipt = CommittedCheckpointIndexBuildReceipt::default();
        let replacement_sequence =
            ResumableStreamingSequenceBuilder::try_new(&mut build_receipt.sequence)?;
        let splice = ResumableSequenceSplice::try_preallocated_for_build(
            build,
            &mut build_receipt.sequence,
        )?;
        let outer_splice = ResumableSequenceSplice::try_preallocated_for_build(
            build,
            &mut build_receipt.sequence,
        )?;
        let boundary_leaf_pages_decoded = if restart
            .as_ref()
            .is_none_or(|restart| restart.leaf == convergence.leaf)
        {
            1
        } else {
            2
        };
        let boundary_leaf_pages_reencoded = boundary_leaf_pages_decoded;
        let boundary_sample_records_decoded = if let Some(restart) = &restart {
            if restart.leaf == convergence.leaf {
                restart.records.len()
            } else {
                restart
                    .records
                    .len()
                    .checked_add(convergence.records.len())
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "decoded boundary records",
                    ))?
            }
        } else {
            convergence.records.len()
        };
        let outer_boundary = locate_outer_boundary_leaf_by_partition_ordinal(
            arena,
            old_root,
            target_partition.ordinal,
        )?;
        let outer_record = *outer_boundary
            .records
            .get(outer_boundary.record_index)
            .ok_or(CommittedCheckpointIndexError::Corrupt(
                "target outer partition record disappeared",
            ))?;
        let DecodedPartitionKind::Donor { child_ordinal } = outer_record.kind else {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "target outer boundary record is no longer a donor",
            ));
        };
        if outer_boundary
            .children
            .get(usize::from(child_ordinal))
            .copied()
            != Some(old_manifest)
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "target outer boundary child changed",
            ));
        }
        let replaced_outer_leaf_end = outer_boundary.leaf_index.checked_add(1).ok_or(
            CommittedCheckpointIndexError::Overflow("replaced outer leaf range end"),
        )?;
        let path_cache = preflight_path_cache(maximum_fresh_path_depth)?;
        let path_cache_frame_capacity = path_cache.previous_frames.capacity();
        let path_cache_node_capacity = path_cache.previous_nodes.capacity();
        let mut path_ids_scratch = Vec::new();
        path_ids_scratch
            .try_reserve_exact(MAX_PACKED_ARENA_CHILDREN)
            .map_err(|_| {
                CommittedCheckpointIndexError::Allocation("donor splice path-ID scratch")
            })?;
        let path_ids_scratch_capacity = path_ids_scratch.capacity();
        let mut outer_payload = Vec::new();
        outer_payload
            .try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| CommittedCheckpointIndexError::Allocation("donor splice outer payload"))?;
        let outer_payload_capacity = outer_payload.capacity();
        let page = DonorSampleLeafEncoder::new()?;
        let page_payload_capacity = page.payload.capacity();
        let page_path_owner_capacity = page.path_owners.capacity();
        Ok(Self {
            build,
            old_outer_root: old_root,
            old_sample_root: donor_manifest.sample_root,
            partition_role: donor_manifest.role,
            replaced_leaf_range,
            replaced_outer_leaf_range: outer_boundary.leaf_index..replaced_outer_leaf_end,
            outer_boundary_records: outer_boundary.records,
            outer_boundary_children: outer_boundary.children,
            outer_target_record: outer_boundary.record_index,
            samples,
            pending_fresh_sample: None,
            page,
            page_payload_capacity,
            page_path_owner_capacity,
            path_cache,
            path_cache_frame_capacity,
            path_cache_node_capacity,
            path_ids_scratch,
            path_ids_scratch_capacity,
            outer_payload,
            outer_payload_capacity,
            replacement_sequence,
            splice,
            outer_splice,
            replacement_root: None,
            spliced_root: None,
            manifest: None,
            outer_replacement_root: None,
            output: None,
            phase: SplicePhase::EncodeReplacement,
            build_receipt,
            receipt: DonorSuffixSpliceReceipt {
                old_samples: donor_manifest.samples,
                old_sample_leaf_pages: sample_tree_summary.leaf_pages,
                old_samples_replaced,
                fresh_samples_inserted,
                admission_fresh_samples_scanned,
                admission_fresh_samples_requeued: 0,
                admission_fresh_chain_capacity_allocated: 0,
                sample_tree_nodes_visited: restart
                    .as_ref()
                    .map_or(0, |restart| restart.nodes_visited)
                    .checked_add(convergence.nodes_visited)
                    .and_then(|visited| visited.checked_add(outer_boundary.nodes_visited))
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "sample tree visit receipt",
                    ))?,
                boundary_leaf_pages_decoded,
                boundary_sample_records_decoded,
                boundary_leaf_pages_reencoded,
                replacement_leaf_pages: 0,
                old_leaf_pages_retained,
                source_bytes_retained,
                source_bytes_replaced,
                source_bytes_inserted,
                maximum_fresh_path_depth,
                suffix_relevant_continuation_identity: true,
                normalization_role_preserved: matches!(
                    donor_manifest.role,
                    DonorPartitionRole::Normalization { .. }
                ),
                normalization_group_start_zero_storage_shape: matches!(
                    donor_manifest.role,
                    DonorPartitionRole::Normalization { .. }
                ) && target_partition.prefix
                    == RelativeCheckpointMeasure::default(),
                terminal_tail_retained: old_summary.terminal_tail,
                mechanism_only_unpublishable: true,
                retained_source_bytes: 0,
                sequence: build_receipt.sequence,
            },
        })
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub(crate) fn receipt(&self) -> DonorSuffixSpliceReceipt {
        let mut receipt = self.receipt;
        receipt.sequence = self.build_receipt.sequence;
        receipt
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<DonorSuffixSpliceProgress, CommittedCheckpointIndexError> {
        self.ensure_session(session)?;
        let result = self.poll_inner(session);
        if result.is_err() {
            self.phase = SplicePhase::Failed;
        }
        result
    }

    #[allow(clippy::too_many_lines)] // Each arm is one explicit journal/state-machine transition.
    fn poll_inner(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<DonorSuffixSpliceProgress, CommittedCheckpointIndexError> {
        match self.phase {
            SplicePhase::EncodeReplacement => {
                if !self.page.can_fit() {
                    self.begin_leaf_flush(session)?;
                    return Ok(DonorSuffixSpliceProgress::Pending);
                }
                let Some(sample) = self.samples.pop_front() else {
                    self.phase = if self.page.is_empty() {
                        SplicePhase::BeginReplacementFinish
                    } else {
                        self.begin_leaf_flush(session)?;
                        SplicePhase::PollReplacementPush
                    };
                    return Ok(DonorSuffixSpliceProgress::Pending);
                };
                self.begin_encode_one_sample(session, sample)?;
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::BuildFreshPath => {
                if self.poll_fresh_sample(session)? {
                    self.phase = SplicePhase::EncodeReplacement;
                }
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::PollReplacementPush => {
                if self
                    .replacement_sequence
                    .poll_push(session, &mut self.build_receipt.sequence)?
                    == ResumableSequenceProgress::Complete
                {
                    self.phase = if self.samples.is_empty() {
                        SplicePhase::BeginReplacementFinish
                    } else {
                        SplicePhase::EncodeReplacement
                    };
                }
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::BeginReplacementFinish => {
                self.path_cache.release_terminal_if_present(session)?;
                self.replacement_sequence
                    .begin_finish(&mut self.build_receipt.sequence)?;
                self.phase = SplicePhase::PollReplacementFinish;
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::PollReplacementFinish => {
                if self
                    .replacement_sequence
                    .poll_finish(session, &mut self.build_receipt.sequence)?
                    == ResumableSequenceProgress::Complete
                {
                    self.replacement_root = Some(self.replacement_sequence.take_root()?);
                    self.phase = SplicePhase::BeginSplice;
                }
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::BeginSplice => {
                let old_root = session.retain(self.old_sample_root)?;
                let replacement =
                    self.replacement_root
                        .take()
                        .ok_or(CommittedCheckpointIndexError::Corrupt(
                            "donor replacement sequence root disappeared",
                        ))?;
                self.splice.begin_from_owned(
                    session,
                    Some(old_root),
                    self.replaced_leaf_range.clone(),
                    Some(replacement),
                    &mut self.build_receipt.sequence,
                )?;
                self.phase = SplicePhase::PollSplice;
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::PollSplice => {
                if self
                    .splice
                    .poll(session, &mut self.build_receipt.sequence)?
                    == ResumableSequenceSplitProgress::Complete
                {
                    self.spliced_root = self.splice.take_root()?;
                    self.phase = SplicePhase::AllocateManifest;
                }
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::AllocateManifest => {
                let sample_root =
                    self.spliced_root
                        .take()
                        .ok_or(CommittedCheckpointIndexError::Corrupt(
                            "spliced donor sample root disappeared",
                        ))?;
                let sample_root_id = session.owner_id(&sample_root)?;
                let summary = sequence_node::<DonorSampleSpec>(session.arena(), sample_root_id)?.0;
                let group = match self.partition_role {
                    DonorPartitionRole::DirectRun => None,
                    DonorPartitionRole::Normalization { group, outcome } => Some((group, outcome)),
                };
                let payload =
                    encode_donor_partition_manifest(group, summary.measure, summary.samples)?;
                let (manifest, allocation) = session.allocate(&payload, &[sample_root_id])?;
                observe_allocation(&mut self.build_receipt, allocation);
                self.build_receipt.donor_partition_manifests = self
                    .build_receipt
                    .donor_partition_manifests
                    .checked_add(1)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor partition manifest count",
                    ))?;
                session.release(sample_root)?;
                self.manifest = Some(manifest);
                self.phase = SplicePhase::AllocateOuterReplacement;
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::AllocateOuterReplacement => {
                let manifest =
                    self.manifest
                        .take()
                        .ok_or(CommittedCheckpointIndexError::Corrupt(
                            "spliced donor manifest disappeared",
                        ))?;
                let manifest_id = session.owner_id(&manifest)?;
                let decoded = decode_donor_partition_manifest(session.arena(), manifest_id)?;
                self.require_fixed_poll_capacity()?;
                let target = self
                    .outer_boundary_records
                    .get_mut(self.outer_target_record)
                    .ok_or(CommittedCheckpointIndexError::Corrupt(
                        "outer splice target record disappeared",
                    ))?;
                let DecodedPartitionKind::Donor { child_ordinal } = target.kind else {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "outer splice target record changed role",
                    ));
                };
                let target_child = self
                    .outer_boundary_children
                    .get_mut(usize::from(child_ordinal))
                    .ok_or(CommittedCheckpointIndexError::Corrupt(
                        "outer splice target child disappeared",
                    ))?;
                target.interval = decoded.measure;
                target.samples = decoded.samples;
                *target_child = manifest_id;

                self.outer_payload.clear();
                self.outer_payload.resize(SUMMARY_BYTES, 0);
                let mut summary = CheckpointIndexSummary {
                    leaf_pages: 1,
                    height: 1,
                    ..CheckpointIndexSummary::default()
                };
                for record in &self.outer_boundary_records {
                    let (tag, child) = match record.kind {
                        DecodedPartitionKind::Direct => (DIRECT_PARTITION_TAG, NO_CHILD_ORDINAL),
                        DecodedPartitionKind::Normalization { child_ordinal } => {
                            (NORMALIZATION_PARTITION_TAG, child_ordinal)
                        }
                        DecodedPartitionKind::Donor { child_ordinal } => {
                            (DONOR_PARTITION_TAG, child_ordinal)
                        }
                        DecodedPartitionKind::TerminalTail => {
                            (TERMINAL_TAIL_PARTITION_TAG, NO_CHILD_ORDINAL)
                        }
                    };
                    encode_partition_record(
                        &mut self.outer_payload,
                        tag,
                        child,
                        record.interval,
                        record.samples,
                    );
                    summary = summary.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: record.samples,
                        measure: record.interval,
                        terminal_tail: matches!(record.kind, DecodedPartitionKind::TerminalTail),
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                if self
                    .outer_payload
                    .len()
                    .checked_add(self.outer_boundary_children.len().checked_mul(8).ok_or(
                        CommittedCheckpointIndexError::Overflow("outer splice child edge bytes"),
                    )?)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "outer splice packed bytes",
                    ))?
                    > ARENA_PAGE_BYTES
                {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "outer splice replacement no longer fits its boundary leaf",
                    ));
                }
                self.outer_payload[..SUMMARY_BYTES]
                    .copy_from_slice(&encode_summary(INDEX_LEAF_TAG, summary));
                let (replacement, allocation) =
                    session.allocate_packed(&self.outer_payload, &self.outer_boundary_children)?;
                observe_allocation(&mut self.build_receipt, allocation);
                self.build_receipt.outer_leaf_pages = self
                    .build_receipt
                    .outer_leaf_pages
                    .checked_add(1)
                    .ok_or(CommittedCheckpointIndexError::Overflow("outer leaf count"))?;
                let replacement_id = session.owner_id(&replacement)?;
                let validated =
                    sequence_node::<CheckpointIndexSpec>(session.arena(), replacement_id)?.0;
                if validated != summary {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "outer replacement leaf summary disagrees",
                    ));
                }
                session.release(manifest)?;
                self.outer_replacement_root = Some(replacement);
                self.require_fixed_poll_capacity()?;
                self.phase = SplicePhase::BeginOuterSplice;
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::BeginOuterSplice => {
                let old_root = session.retain(self.old_outer_root)?;
                let replacement = self.outer_replacement_root.take().ok_or(
                    CommittedCheckpointIndexError::Corrupt(
                        "outer replacement leaf disappeared before splice",
                    ),
                )?;
                self.outer_splice.begin_from_owned(
                    session,
                    Some(old_root),
                    self.replaced_outer_leaf_range.clone(),
                    Some(replacement),
                    &mut self.build_receipt.sequence,
                )?;
                self.phase = SplicePhase::PollOuterSplice;
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::PollOuterSplice => {
                if self
                    .outer_splice
                    .poll(session, &mut self.build_receipt.sequence)?
                    == ResumableSequenceSplitProgress::Complete
                {
                    let root = self.outer_splice.take_root()?.ok_or(
                        CommittedCheckpointIndexError::Corrupt(
                            "outer checkpoint splice produced an empty root",
                        ),
                    )?;
                    let root_id = session.owner_id(&root)?;
                    let summary = sequence_node::<CheckpointIndexSpec>(session.arena(), root_id)?.0;
                    if summary.partitions == 0 {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "outer checkpoint splice produced no partitions",
                        ));
                    }
                    self.receipt.sequence = self.build_receipt.sequence;
                    self.output = Some(StorageOnlyCheckpointIndexBuildManifest {
                        build: self.build,
                        owner: root,
                        receipt: self.build_receipt,
                    });
                    self.phase = SplicePhase::Complete;
                    return Ok(DonorSuffixSpliceProgress::Complete);
                }
                Ok(DonorSuffixSpliceProgress::Pending)
            }
            SplicePhase::Complete => Ok(DonorSuffixSpliceProgress::Complete),
            SplicePhase::Failed => Err(CommittedCheckpointIndexError::Invalid(
                "donor suffix splice job is poisoned",
            )),
        }
    }

    pub(crate) fn take_manifest(
        &mut self,
    ) -> Result<StorageOnlyCheckpointIndexBuildManifest, CommittedCheckpointIndexError> {
        if !matches!(self.phase, SplicePhase::Complete) {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor suffix splice is incomplete",
            ));
        }
        self.output
            .take()
            .ok_or(CommittedCheckpointIndexError::Invalid(
                "donor suffix splice manifest was already taken",
            ))
    }

    pub(crate) fn cancel(self, session: ArenaBuildSession<'_>) -> ArenaBuildId {
        // Both public cancellation entry points validate the build ID before
        // this consuming transition. An owned session is necessarily resumed,
        // so `begin_abort` cannot fail without an arena invariant violation.
        debug_assert_eq!(session.id(), self.build);
        session
            .begin_abort()
            .expect("validated donor suffix splice session must begin abort")
    }

    /// Separates unencoded heap drafts from arena-owned cancellation. The
    /// existing-prefix/suffix queues are bounded to decoded boundary leaves;
    /// only the fresh iterator can scale with the changed suffix.
    fn into_heap_retirement(self) -> DonorCheckpointHeapRetirement {
        let ReplacementSamples {
            changed,
            final_sample,
            existing_prefix: _,
            existing_suffix: _,
        } = self.samples;
        let pending_frames = self.pending_fresh_sample.map(|sample| sample.frames);
        DonorCheckpointHeapRetirement::from_segmented(
            changed,
            final_sample,
            pending_frames,
            Some(self.path_cache.previous_frames),
        )
    }

    fn ensure_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), CommittedCheckpointIndexError> {
        if session.id() != self.build {
            return Err(CommittedCheckpointIndexError::Invalid(
                "arena session belongs to another donor suffix splice build",
            ));
        }
        session.live_owners()?;
        Ok(())
    }

    fn begin_leaf_flush(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), CommittedCheckpointIndexError> {
        if self.page.is_empty() {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "cannot flush an empty donor replacement leaf",
            ));
        }
        let header = encode_summary(DONOR_SAMPLE_LEAF_TAG, self.page.summary);
        self.page.payload[..SUMMARY_BYTES].copy_from_slice(&header);
        self.require_fixed_poll_capacity()?;
        self.path_ids_scratch.clear();
        for owner in &self.page.path_owners {
            self.path_ids_scratch.push(session.owner_id(owner)?);
        }
        let (leaf, allocation) =
            session.allocate_packed(&self.page.payload, &self.path_ids_scratch)?;
        observe_allocation(&mut self.build_receipt, allocation);
        self.build_receipt.sample_leaf_pages =
            self.build_receipt.sample_leaf_pages.checked_add(1).ok_or(
                CommittedCheckpointIndexError::Overflow("donor replacement leaf count"),
            )?;
        self.receipt.replacement_leaf_pages =
            self.receipt.replacement_leaf_pages.checked_add(1).ok_or(
                CommittedCheckpointIndexError::Overflow("replacement leaf receipt"),
            )?;
        for owner in self.page.path_owners.drain(..) {
            session.release(owner)?;
        }
        self.replacement_sequence
            .begin_push(session, leaf, &mut self.build_receipt.sequence)?;
        self.page.reset();
        self.require_fixed_poll_capacity()?;
        self.phase = SplicePhase::PollReplacementPush;
        Ok(())
    }

    fn begin_encode_one_sample(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        sample: ReplacementSample,
    ) -> Result<(), CommittedCheckpointIndexError> {
        match sample {
            ReplacementSample::Existing {
                interval,
                header,
                path_terminal,
            } => {
                let path_owner = session.retain(path_terminal)?;
                self.finish_encoded_sample(interval, header, path_owner)
            }
            ReplacementSample::Fresh(sample) => {
                self.begin_fresh_sample(sample)?;
                self.phase = SplicePhase::BuildFreshPath;
                Ok(())
            }
        }
    }

    fn begin_fresh_sample(
        &mut self,
        sample: DonorCheckpointSampleDraft,
    ) -> Result<(), CommittedCheckpointIndexError> {
        if self.pending_fresh_sample.is_some() {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "fresh donor sample path is already pending",
            ));
        }
        observe_fresh_draft(&sample, &mut self.build_receipt)?;
        let DonorCheckpointSampleDraft { interval, donor } = sample;
        let header = *donor.header();
        let frames = donor.into_frames();
        if frames.is_empty() {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor sample has an empty open path",
            ));
        }
        if self.path_cache.previous_frames.len() != self.path_cache.previous_nodes.len()
            || self.path_cache.previous_frames.capacity() < frames.len()
            || self.path_cache.previous_nodes.capacity() < frames.len()
            || (self.path_cache.previous_frames.is_empty() != self.path_cache.terminal.is_none())
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "preflighted donor path cache cannot admit fresh path",
            ));
        }
        self.build_receipt.maximum_donor_path_build_scratch_bytes = self
            .build_receipt
            .maximum_donor_path_build_scratch_bytes
            .max(
                self.path_cache.previous_frames.capacity() * DONOR_FRAME_BYTES
                    + frames.capacity() * DONOR_FRAME_BYTES
                    + self.path_cache.previous_nodes.capacity() * std::mem::size_of::<ArenaId>(),
            );
        let previous_frame_count = self.path_cache.previous_frames.len();
        self.pending_fresh_sample = Some(ResumableFreshSample {
            interval,
            header,
            compare_limit: previous_frame_count.min(frames.len()),
            frames,
            common: 0,
            previous_frame_count,
            next_allocation: 0,
            prior: None,
            newest_owner: None,
            replacement_terminal: None,
            remember_index: 0,
            phase: FreshPathPhase::CompareCommonPrefix,
        });
        Ok(())
    }

    /// Advances one bounded fresh-path transition. The allocation phase emits
    /// exactly one unmatched frame node, after which control returns to the
    /// actor even when millions of additional frames remain.
    fn poll_fresh_sample(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<bool, CommittedCheckpointIndexError> {
        let mut pending =
            self.pending_fresh_sample
                .take()
                .ok_or(CommittedCheckpointIndexError::Corrupt(
                    "fresh donor path state disappeared",
                ))?;
        let complete = match pending.phase {
            FreshPathPhase::CompareCommonPrefix => {
                if pending.common < pending.compare_limit
                    && self.path_cache.previous_frames[pending.common]
                        == pending.frames[pending.common]
                {
                    pending.common += 1;
                } else {
                    pending.phase = FreshPathPhase::PrepareReplacement;
                }
                false
            }
            FreshPathPhase::PrepareReplacement => {
                self.build_receipt.donor_path_prefix_records_reused = self
                    .build_receipt
                    .donor_path_prefix_records_reused
                    .checked_add(pending.common)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor path prefix reuse receipt",
                    ))?;
                if self.path_cache.previous_nodes.len() != pending.previous_frame_count
                    || pending.common > self.path_cache.previous_nodes.len()
                {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "donor path cache changed during prefix comparison",
                    ));
                }
                self.path_cache.previous_nodes.truncate(pending.common);
                let identical = pending.common == pending.frames.len()
                    && pending.common == pending.previous_frame_count;
                if identical {
                    pending.phase = FreshPathPhase::InstallTerminal;
                } else if pending.common == pending.frames.len() {
                    let prefix = *self.path_cache.previous_nodes.last().ok_or(
                        CommittedCheckpointIndexError::Corrupt(
                            "donor prefix path lost its terminal",
                        ),
                    )?;
                    pending.replacement_terminal = Some(session.retain(prefix)?);
                    pending.phase = FreshPathPhase::InstallTerminal;
                } else {
                    pending.next_allocation = pending.common;
                    pending.prior = self.path_cache.previous_nodes.last().copied();
                    pending.phase = FreshPathPhase::AllocateUnmatchedFrame;
                }
                false
            }
            FreshPathPhase::AllocateUnmatchedFrame => {
                let index = pending.next_allocation;
                let frame =
                    *pending
                        .frames
                        .get(index)
                        .ok_or(CommittedCheckpointIndexError::Corrupt(
                            "fresh donor path allocation escaped its frame vector",
                        ))?;
                let depth = u32::try_from(index + 1).map_err(|_| {
                    CommittedCheckpointIndexError::Invalid("donor path depth exceeds u32")
                })?;
                let payload = encode_donor_path_node(depth, frame);
                let (owner, allocation) = match pending.prior {
                    Some(prior) => session.allocate_packed(&payload, &[prior])?,
                    None => session.allocate_packed(&payload, &[])?,
                };
                observe_allocation(&mut self.build_receipt, allocation);
                self.build_receipt.donor_path_nodes_allocated = self
                    .build_receipt
                    .donor_path_nodes_allocated
                    .checked_add(1)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor path node count",
                    ))?;
                self.build_receipt.donor_retained_payload_bytes = self
                    .build_receipt
                    .donor_retained_payload_bytes
                    .checked_add(DONOR_PATH_NODE_BYTES)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor retained payload bytes",
                    ))?;
                if pending.prior.is_some() {
                    self.build_receipt.donor_retained_edge_bytes = self
                        .build_receipt
                        .donor_retained_edge_bytes
                        .checked_add(8)
                        .ok_or(CommittedCheckpointIndexError::Overflow(
                            "donor retained edge bytes",
                        ))?;
                }
                let id = session.owner_id(&owner)?;
                if let Some(previous_owner) = pending.newest_owner.replace(owner) {
                    session.release(previous_owner)?;
                }
                if self.path_cache.previous_nodes.len() == self.path_cache.previous_nodes.capacity()
                {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "preflighted donor path node cache exhausted during poll",
                    ));
                }
                self.path_cache.previous_nodes.push(id);
                pending.prior = Some(id);
                pending.next_allocation = index + 1;
                if pending.next_allocation == pending.frames.len() {
                    pending.phase = FreshPathPhase::InstallTerminal;
                }
                false
            }
            FreshPathPhase::InstallTerminal => {
                let identical = pending.common == pending.frames.len()
                    && pending.common == pending.previous_frame_count;
                if identical {
                    if pending.newest_owner.is_some() || pending.replacement_terminal.is_some() {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "identical donor path unexpectedly allocated a terminal",
                        ));
                    }
                } else {
                    let replacement = pending
                        .replacement_terminal
                        .take()
                        .or_else(|| pending.newest_owner.take())
                        .ok_or(CommittedCheckpointIndexError::Corrupt(
                            "fresh donor path produced no replacement terminal",
                        ))?;
                    if let Some(old_terminal) = self.path_cache.terminal.replace(replacement) {
                        session.release(old_terminal)?;
                    }
                }
                if self.path_cache.terminal.is_none() {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "fresh donor path cache has no installed terminal",
                    ));
                }
                self.path_cache.previous_frames.clear();
                pending.remember_index = 0;
                pending.phase = FreshPathPhase::RememberFrames;
                false
            }
            FreshPathPhase::RememberFrames => {
                if pending.remember_index < pending.frames.len() {
                    if self.path_cache.previous_frames.len()
                        == self.path_cache.previous_frames.capacity()
                    {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "preflighted donor path frame cache exhausted during poll",
                        ));
                    }
                    self.path_cache
                        .previous_frames
                        .push(pending.frames[pending.remember_index]);
                    pending.remember_index += 1;
                } else {
                    if self.path_cache.previous_frames.len() != self.path_cache.previous_nodes.len()
                    {
                        return Err(CommittedCheckpointIndexError::Corrupt(
                            "fresh donor path frame and node caches diverged",
                        ));
                    }
                    pending.phase = FreshPathPhase::EmitSample;
                }
                false
            }
            FreshPathPhase::EmitSample => {
                let terminal = self.path_cache.terminal.as_ref().ok_or(
                    CommittedCheckpointIndexError::Corrupt(
                        "fresh donor path cache has no terminal owner",
                    ),
                )?;
                let terminal_id = session.owner_id(terminal)?;
                let path_owner = session.retain(terminal_id)?;
                self.finish_encoded_sample(pending.interval, pending.header, path_owner)?;
                true
            }
        };
        if !complete {
            self.pending_fresh_sample = Some(pending);
        }
        self.require_fixed_poll_capacity()?;
        Ok(complete)
    }

    fn finish_encoded_sample(
        &mut self,
        interval: RelativeCheckpointMeasure,
        header: OpaqueDonorHeader,
        path_owner: ArenaBuildOwner,
    ) -> Result<(), CommittedCheckpointIndexError> {
        observe_encoded_sample(&mut self.build_receipt)?;
        self.page.push(interval, &header, path_owner)?;
        self.require_fixed_poll_capacity()
    }

    fn require_fixed_poll_capacity(&self) -> Result<(), CommittedCheckpointIndexError> {
        if self.path_cache.previous_frames.capacity() != self.path_cache_frame_capacity
            || self.path_cache.previous_nodes.capacity() != self.path_cache_node_capacity
            || self.path_ids_scratch.capacity() != self.path_ids_scratch_capacity
            || self.outer_payload.capacity() != self.outer_payload_capacity
            || self.page.payload.capacity() != self.page_payload_capacity
            || self.page.path_owners.capacity() != self.page_path_owner_capacity
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor suffix splice poll scratch capacity changed",
            ));
        }
        Ok(())
    }
}

fn preflight_path_cache(
    maximum_depth: usize,
) -> Result<DonorPathCache, CommittedCheckpointIndexError> {
    let mut previous_frames = Vec::new();
    previous_frames
        .try_reserve_exact(maximum_depth)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("donor splice path-frame cache"))?;
    let mut previous_nodes = Vec::new();
    previous_nodes
        .try_reserve_exact(maximum_depth)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("donor splice path-node cache"))?;
    Ok(DonorPathCache {
        previous_frames,
        previous_nodes,
        terminal: None,
    })
}

fn append_existing(
    output: &mut VecDeque<ReplacementSample>,
    records: &[DecodedDonorSampleRecord],
    paths: &[ArenaId],
    range: Range<usize>,
) -> Result<(), CommittedCheckpointIndexError> {
    if records.len() != paths.len() || range.end > records.len() || range.start > range.end {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor boundary records and paths disagree",
        ));
    }
    for index in range {
        let record = records[index];
        output.push_back(ReplacementSample::Existing {
            interval: record.interval,
            header: record.header,
            path_terminal: paths[index],
        });
    }
    Ok(())
}

fn observe_encoded_sample(
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<(), CommittedCheckpointIndexError> {
    receipt.donor_sample_headers = receipt.donor_sample_headers.checked_add(1).ok_or(
        CommittedCheckpointIndexError::Overflow("donor sample header count"),
    )?;
    receipt.donor_sample_header_bytes = receipt
        .donor_sample_header_bytes
        .checked_add(DONOR_HEADER_BYTES)
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor sample header bytes",
        ))?;
    receipt.donor_retained_payload_bytes = receipt
        .donor_retained_payload_bytes
        .checked_add(DONOR_HEADER_BYTES)
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor retained payload bytes",
        ))?;
    receipt.donor_sample_path_edges = receipt.donor_sample_path_edges.checked_add(1).ok_or(
        CommittedCheckpointIndexError::Overflow("donor sample path edges"),
    )?;
    receipt.donor_retained_edge_bytes = receipt.donor_retained_edge_bytes.checked_add(8).ok_or(
        CommittedCheckpointIndexError::Overflow("donor retained edge bytes"),
    )?;
    Ok(())
}

fn observe_fresh_draft(
    sample: &DonorCheckpointSampleDraft,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<(), CommittedCheckpointIndexError> {
    receipt.donor_materialized_path_records = receipt
        .donor_materialized_path_records
        .checked_add(sample.donor.frames().len())
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor materialized path records",
        ))?;
    receipt.donor_materialized_path_bytes = receipt
        .donor_materialized_path_bytes
        .checked_add(sample.donor.donor_materialized_path_bytes())
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor materialized path bytes",
        ))?;
    receipt.maximum_donor_capture_conversion_scratch_bytes = receipt
        .maximum_donor_capture_conversion_scratch_bytes
        .max(sample.donor.conversion_scratch_bytes());
    receipt.retained_source_bytes = receipt
        .retained_source_bytes
        .checked_add(sample.donor.retained_source_bytes())
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor retained source bytes",
        ))?;
    Ok(())
}

fn locate_exact_endpoint(
    arena: &PageArena,
    root: ArenaId,
    cut: RelativeCheckpointMeasure,
) -> Result<LocatedBoundaryLeaf, CommittedCheckpointIndexError> {
    if cut.source_bytes() == 0 {
        return Err(CommittedCheckpointIndexError::Invalid(
            "donor splice cut is not a sample endpoint",
        ));
    }
    let root_summary = sequence_node::<DonorSampleSpec>(arena, root)?.0;
    if cut.source_bytes() > root_summary.measure.source_bytes() {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let selected_byte = cut.source_bytes() - 1;
    let mut node = root;
    let mut prefix = CheckpointIndexSummary::default();
    let mut leaf_index = 0_u64;
    let mut nodes_visited = 0_usize;
    loop {
        nodes_visited =
            nodes_visited
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "donor endpoint nodes visited",
                ))?;
        match sequence_node::<DonorSampleSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let records = decode_donor_sample_leaf_in_arena(arena, node)?;
                let mut paths = Vec::new();
                paths.try_reserve_exact(records.len()).map_err(|_| {
                    CommittedCheckpointIndexError::Allocation("donor boundary path IDs")
                })?;
                for index in 0..records.len() {
                    paths.push(arena.packed_child_at(node, index)?);
                }
                for (record_index, record) in records.iter().copied().enumerate() {
                    let end = prefix.measure.checked_add(record.interval)?;
                    if selected_byte < end.source_bytes() {
                        if end != cut {
                            return Err(CommittedCheckpointIndexError::Invalid(
                                "donor splice cut is not an exact five-axis sample endpoint",
                            ));
                        }
                        return Ok(LocatedBoundaryLeaf {
                            leaf: node,
                            leaf_index,
                            record_index,
                            sample_ordinal: prefix.samples,
                            sample_prefix: prefix.measure,
                            records,
                            paths,
                            nodes_visited,
                        });
                    }
                    prefix = prefix.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: 1,
                        measure: record.interval,
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "donor endpoint leaf did not cover selected byte",
                ));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<DonorSampleSpec>(arena, left)?.0;
                let left_end = prefix
                    .measure
                    .source_bytes()
                    .checked_add(left_summary.measure.source_bytes())
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor endpoint branch source bytes",
                    ))?;
                if selected_byte < left_end {
                    node = left;
                } else {
                    prefix = prefix.followed_by(left_summary)?;
                    leaf_index = leaf_index.checked_add(left_summary.leaf_pages).ok_or(
                        CommittedCheckpointIndexError::Overflow("donor endpoint leaf index"),
                    )?;
                    node = right;
                }
            }
        }
    }
}

/// Locates the one bounded outer leaf that contains `ordinal`, retaining no
/// root and copying only that leaf's canonical records and child IDs.  The
/// later persistent splice replaces this leaf and path-copies the outer tree;
/// no document-sized partition vector is materialized.
fn locate_outer_boundary_leaf_by_partition_ordinal(
    arena: &PageArena,
    root: ArenaId,
    ordinal: u64,
) -> Result<LocatedOuterBoundaryLeaf, CommittedCheckpointIndexError> {
    let root_summary = sequence_node::<CheckpointIndexSpec>(arena, root)?.0;
    if ordinal >= root_summary.partitions {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let mut node = root;
    let mut partition_prefix = 0_u64;
    let mut leaf_index = 0_u64;
    let mut nodes_visited = 0_usize;
    loop {
        nodes_visited =
            nodes_visited
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "outer boundary nodes visited",
                ))?;
        match sequence_node::<CheckpointIndexSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let records = decode_outer_leaf_records_in_arena(arena, node)?;
                let local = ordinal.checked_sub(partition_prefix).ok_or(
                    CommittedCheckpointIndexError::Corrupt(
                        "outer boundary partition prefix exceeds target ordinal",
                    ),
                )?;
                let record_index = usize::try_from(local).map_err(|_| {
                    CommittedCheckpointIndexError::Overflow("outer boundary record index")
                })?;
                if record_index >= records.len() {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "outer boundary leaf does not contain target partition",
                    ));
                }
                let child_count = arena.packed_child_count(node)?;
                let mut children = Vec::new();
                children.try_reserve_exact(child_count).map_err(|_| {
                    CommittedCheckpointIndexError::Allocation("outer boundary child IDs")
                })?;
                for child in 0..child_count {
                    children.push(arena.packed_child_at(node, child)?);
                }
                return Ok(LocatedOuterBoundaryLeaf {
                    leaf: node,
                    leaf_index,
                    record_index,
                    records,
                    children,
                    nodes_visited,
                });
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<CheckpointIndexSpec>(arena, left)?.0;
                let left_end = partition_prefix
                    .checked_add(left_summary.partitions)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "outer boundary partition prefix",
                    ))?;
                if ordinal < left_end {
                    node = left;
                } else {
                    partition_prefix = left_end;
                    leaf_index = leaf_index.checked_add(left_summary.leaf_pages).ok_or(
                        CommittedCheckpointIndexError::Overflow("outer boundary leaf index"),
                    )?;
                    node = right;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_writer::RestartCompositeChildren;
    use crate::serialized_green::{
        CoveragePart, FactsEnvelope, GreenEvent, GreenKind, LogicalContribution,
        ResumableSerializedGreenBuild, SerializedGreenBuildManifest, SerializedGreenRootSpec,
        SerializedGreenStreamProgress, SourceProjectionRun,
    };
    use crate::storage_only_composite_document::{
        RestartCompositeDocument, RestartCompositeDocumentBuilder,
    };
    use crate::{
        BlockId, ClosedChildAggregate, CoverageId, GrammarRevision, ParseGeneration,
        SourceRevision, SourceRootId,
    };
    use flark_comrak_value_block_core::{DirectPollStatus, DirectValueBlockParser, SyntaxProfile};

    fn measure(
        bytes: u64,
        utf16: u64,
        lines: u64,
        events: u64,
        runs: u64,
    ) -> RelativeCheckpointMeasure {
        RelativeCheckpointMeasure::new(bytes, utf16, lines, events, runs)
    }

    fn repeated(interval: RelativeCheckpointMeasure, count: u64) -> RelativeCheckpointMeasure {
        (0..count)
            .try_fold(RelativeCheckpointMeasure::default(), |total, _| {
                total.checked_add(interval)
            })
            .unwrap()
    }

    fn started_donor() -> DirectValueBlockParser {
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        assert!(parser.pending_command().is_some());
        parser.acknowledge_command().unwrap();
        parser
    }

    fn drive_line(parser: &mut DirectValueBlockParser, line: &str) {
        parser.begin_line(line.to_owned()).unwrap();
        for _ in 0..line.len().saturating_mul(8).saturating_add(256) {
            match parser.poll_line(1).unwrap().status {
                DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                DirectPollStatus::Pending => {}
                DirectPollStatus::ExternalWorkReady => {
                    panic!("non-reference donor fixture unexpectedly requested external work")
                }
                DirectPollStatus::Complete => return,
            }
        }
        panic!("donor line did not converge");
    }

    fn donor_after_line(line: &str) -> DirectValueBlockParser {
        donor_after_lines(&[line])
    }

    fn donor_after_lines(lines: &[&str]) -> DirectValueBlockParser {
        let mut parser = started_donor();
        for line in lines {
            drive_line(&mut parser, line);
        }
        parser
    }

    fn offer_parent_green(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        build.offer_event(session, event).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => break,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("parent green event unexpectedly finalized its manifest")
                }
            }
        }
    }

    fn finish_parent_green(
        mut build: ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        coverage_runs: u64,
        run_bytes: u64,
        run_utf16: u64,
    ) -> SerializedGreenBuildManifest {
        offer_parent_green(
            &mut build,
            session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer_parent_green(
            &mut build,
            session,
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        for ordinal in 0..coverage_runs {
            offer_parent_green(
                &mut build,
                session,
                GreenEvent::Coverage(
                    SourceProjectionRun::with_logical(
                        CoverageId(ordinal + 1),
                        run_bytes,
                        run_utf16,
                        0,
                        CoveragePart::CONTENT,
                        BlockId(2),
                        LogicalContribution::Identity,
                    )
                    .unwrap(),
                ),
            );
        }
        offer_parent_green(
            &mut build,
            session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_parent_green(
            &mut build,
            session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        build.finish_input(session).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("parent green finalization returned to input")
                }
            }
        }
        build.take_manifest().unwrap()
    }

    fn normalization_restart_parent(
        arena: &mut PageArena,
        parser: &DirectValueBlockParser,
        interval: RelativeCheckpointMeasure,
        sample_count: usize,
        group: u64,
        outcome: StorageOnlyNormalizationOutcome,
        tail: RelativeCheckpointMeasure,
    ) -> RestartCompositeDocument {
        let total_source = repeated(interval, sample_count as u64);
        assert_eq!(interval.green_events(), 1);
        assert_eq!(interval.projection_runs(), 1);
        assert_eq!(tail.green_events(), 4);
        assert_eq!(tail.projection_runs(), 0);
        let ticket = arena.begin_build().unwrap();
        let green_builder = ResumableSerializedGreenBuild::new(
            &ticket,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: SourceRevision(1),
                source_root: SourceRootId(1),
                source_bytes: total_source.source_bytes(),
                source_utf16: total_source.source_utf16(),
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(1),
                semantic_epoch: 1,
                known_bytes: 0..total_source.source_bytes(),
            },
        )
        .unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let samples = (0..sample_count)
            .map(|_| {
                DonorCheckpointSampleDraft::try_new(
                    interval,
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let mut index_builder = StorageOnlyCheckpointIndexBuilder::default();
        index_builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                group, outcome, samples,
            ))
            .unwrap();
        index_builder
            .push(StorageOnlyCheckpointPartition::terminal_tail(tail))
            .unwrap();
        let checkpoint_index = index_builder.build_in_session(&mut session).unwrap();
        let green = finish_parent_green(
            green_builder,
            &mut session,
            sample_count as u64,
            interval.source_bytes(),
            interval.source_utf16(),
        );
        let children =
            RestartCompositeChildren::from_independent_test_children(green, checkpoint_index);
        RestartCompositeDocumentBuilder::join(&mut session, children)
            .unwrap()
            .commit(session)
            .unwrap()
            .0
    }

    fn direct_index(
        arena: &mut PageArena,
        parser: &DirectValueBlockParser,
        interval: RelativeCheckpointMeasure,
        samples: usize,
        terminal_tail: Option<RelativeCheckpointMeasure>,
    ) -> StorageOnlyCommittedCheckpointIndex {
        let mut drafts = Vec::new();
        drafts.try_reserve_exact(samples).unwrap();
        for _ in 0..samples {
            drafts.push(
                DonorCheckpointSampleDraft::try_new(
                    interval,
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            );
        }
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct_samples(drafts))
            .unwrap();
        if let Some(tail) = terminal_tail {
            builder
                .push(StorageOnlyCheckpointPartition::terminal_tail(tail))
                .unwrap();
        }
        builder.commit(arena).unwrap().0
    }

    fn normalization_index(
        arena: &mut PageArena,
        parser: &DirectValueBlockParser,
        interval: RelativeCheckpointMeasure,
    ) -> StorageOnlyCommittedCheckpointIndex {
        normalization_index_with(
            arena,
            parser,
            interval,
            4,
            7,
            StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
            None,
        )
    }

    fn normalization_index_with(
        arena: &mut PageArena,
        parser: &DirectValueBlockParser,
        interval: RelativeCheckpointMeasure,
        sample_count: usize,
        group: u64,
        outcome: StorageOnlyNormalizationOutcome,
        terminal_tail: Option<RelativeCheckpointMeasure>,
    ) -> StorageOnlyCommittedCheckpointIndex {
        let samples = (0..sample_count)
            .map(|_| {
                DonorCheckpointSampleDraft::try_new(
                    interval,
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                group, outcome, samples,
            ))
            .unwrap();
        if let Some(tail) = terminal_tail {
            builder
                .push(StorageOnlyCheckpointPartition::terminal_tail(tail))
                .unwrap();
        }
        builder.commit(arena).unwrap().0
    }

    fn normalization_authority(
        arena: &PageArena,
        index: &StorageOnlyCommittedCheckpointIndex,
        restart_cut: RelativeCheckpointMeasure,
        convergence_cut: RelativeCheckpointMeasure,
    ) -> NormalizationDonorSuffixSpliceAuthority {
        let restart = index
            .locate_donor_checkpoint_at_or_before_cut(arena, restart_cut.source_bytes())
            .unwrap()
            .unwrap();
        let convergence = index
            .locate_donor_checkpoint_at_or_before_cut(arena, convergence_cut.source_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(restart.checkpoint_cut(), restart_cut);
        assert_eq!(convergence.checkpoint_cut(), convergence_cut);
        let CommittedDonorCheckpointRole::Normalization(restart) =
            restart.committed_role(index, arena).unwrap()
        else {
            panic!("restart fixture must authenticate as normalization");
        };
        let CommittedDonorCheckpointRole::Normalization(convergence) =
            convergence.committed_role(index, arena).unwrap()
        else {
            panic!("convergence fixture must authenticate as normalization");
        };
        NormalizationDonorSuffixSpliceAuthority::try_new(restart, convergence).unwrap()
    }

    fn two_normalization_groups_index(
        arena: &mut PageArena,
        parser: &DirectValueBlockParser,
        interval: RelativeCheckpointMeasure,
        first: (u64, StorageOnlyNormalizationOutcome),
        second: (u64, StorageOnlyNormalizationOutcome),
    ) -> StorageOnlyCommittedCheckpointIndex {
        let samples = || {
            (0..4)
                .map(|_| {
                    DonorCheckpointSampleDraft::try_new(
                        interval,
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap()
                })
                .collect()
        };
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                first.0,
                first.1,
                samples(),
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                second.0,
                second.1,
                samples(),
            ))
            .unwrap();
        builder.commit(arena).unwrap().0
    }

    fn donor_sample_root(
        arena: &PageArena,
        index: &StorageOnlyCommittedCheckpointIndex,
    ) -> ArenaId {
        donor_sample_root_from_index_root(arena, index.checked_root_id(arena).unwrap())
    }

    fn donor_sample_root_from_index_root(arena: &PageArena, root: ArenaId) -> ArenaId {
        let records = decode_outer_leaf_records_in_arena(arena, root).unwrap();
        let DecodedPartitionKind::Donor { child_ordinal } = records[0].kind else {
            panic!("fixture must begin with a donor partition");
        };
        let manifest = arena
            .packed_child_at(root, usize::from(child_ordinal))
            .unwrap();
        decode_donor_partition_manifest(arena, manifest)
            .unwrap()
            .sample_root
    }

    fn leaf_at_ordinal(arena: &PageArena, root: ArenaId, mut ordinal: u64) -> ArenaId {
        let mut node = root;
        loop {
            match sequence_node::<DonorSampleSpec>(arena, node).unwrap().1 {
                SequenceNodeKind::Leaf => {
                    let records = decode_donor_sample_leaf_in_arena(arena, node).unwrap();
                    assert!(ordinal < records.len() as u64);
                    return node;
                }
                SequenceNodeKind::Branch { left, right } => {
                    let left_samples = sequence_node::<DonorSampleSpec>(arena, left)
                        .unwrap()
                        .0
                        .samples;
                    if ordinal < left_samples {
                        node = left;
                    } else {
                        ordinal -= left_samples;
                        node = right;
                    }
                }
            }
        }
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(64).unwrap();
        }
    }

    fn finish_abort(arena: &mut PageArena, build: ArenaBuildId) {
        loop {
            if arena.poll_build_abort(build, 64).unwrap().complete {
                break;
            }
        }
        settle(arena);
    }

    fn abort_ticket(arena: &mut PageArena, ticket: ArenaBuildTicket) {
        let build = arena.begin_build_abort(ticket).unwrap();
        finish_abort(arena, build);
    }

    /// Test-shaped parent admission that deliberately ends the parent lease
    /// and releases its original journal owner before returning. A returned
    /// job therefore compiles and runs only if it owns an independent old
    /// checkpoint base rather than borrowing either stack value.
    fn parent_owned_direct_job_after_lease_scope(
        session: &mut ArenaBuildSession<'_>,
        old: &StorageOnlyCommittedCheckpointIndex,
        request: DonorSuffixSpliceRequest,
    ) -> ParentOwnedDonorSuffixSpliceJob {
        let old_root = old.checked_root_id(session.arena()).unwrap();
        let descriptor =
            validate_committed_checkpoint_index_composite_child(session.arena(), old_root).unwrap();
        let retained_parent_child = session.retain(old_root).unwrap();
        let job = {
            let lease = ParentRetainedCheckpointIndexLease::mechanism_only_from_retained_test_index(
                session.id(),
                old.scoped_root_id(),
                &retained_parent_child,
                descriptor,
            );
            ParentOwnedDonorSuffixSpliceJob::try_new_from_parent(session, &lease, request).unwrap()
        };
        session.release(retained_parent_child).unwrap();
        job
    }

    fn run_splice(
        arena: &mut PageArena,
        old: &StorageOnlyCommittedCheckpointIndex,
        request: DonorSuffixSpliceRequest,
    ) -> (
        StorageOnlyCommittedCheckpointIndex,
        DonorSuffixSpliceReceipt,
        CommittedCheckpointIndexBuildReceipt,
        usize,
    ) {
        let ticket = arena.begin_build().unwrap();
        let mut job = DonorSuffixSpliceJob::try_new(&ticket, arena, old, request).unwrap();
        let mut ticket = ticket;
        let mut polls = 0_usize;
        loop {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = job.poll(&mut session).unwrap();
            polls += 1;
            ticket = session.suspend().unwrap();
            if progress == DonorSuffixSpliceProgress::Complete {
                break;
            }
            assert!(polls < 10_000, "donor suffix splice did not converge");
        }
        let receipt = job.receipt();
        let manifest = job.take_manifest().unwrap();
        drop(job);
        let session = arena.resume_build(ticket).unwrap();
        let (index, build_receipt) = manifest.commit(session).unwrap();
        (index, receipt, build_receipt, polls)
    }

    fn run_normalization_splice(
        arena: &mut PageArena,
        old: &StorageOnlyCommittedCheckpointIndex,
        authority: NormalizationDonorSuffixSpliceAuthority,
        request: DonorSuffixSpliceRequest,
    ) -> (
        StorageOnlyCommittedCheckpointIndex,
        DonorSuffixSpliceReceipt,
        CommittedCheckpointIndexBuildReceipt,
        usize,
    ) {
        let ticket = arena.begin_build().unwrap();
        let mut job =
            DonorSuffixSpliceJob::try_new_normalization(&ticket, arena, old, authority, request)
                .unwrap();
        let mut ticket = ticket;
        let mut polls = 0_usize;
        loop {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = job.poll(&mut session).unwrap();
            polls += 1;
            ticket = session.suspend().unwrap();
            if progress == DonorSuffixSpliceProgress::Complete {
                break;
            }
            assert!(
                polls < 10_000,
                "normalization suffix splice did not converge"
            );
        }
        let receipt = job.receipt();
        let manifest = job.take_manifest().unwrap();
        drop(job);
        let session = arena.resume_build(ticket).unwrap();
        let (index, build_receipt) = manifest.commit(session).unwrap();
        (index, receipt, build_receipt, polls)
    }

    #[allow(clippy::too_many_lines)] // One end-to-end fixture keeps identity, measures, receipts, and lookup assertions coupled.
    #[test]
    fn ten_mib_splice_rebases_all_axes_and_retains_distant_suffix_leaf() {
        const TEN_MIB: u64 = 10 * 1024 * 1024;
        const SAMPLE_BYTES: u64 = 16 * 1024;
        const SAMPLES: usize = (TEN_MIB / SAMPLE_BYTES) as usize;
        const RESTART_SAMPLES: u64 = 200;
        const CONVERGENCE_SAMPLES: u64 = 400;
        const DISTANT_OLD_ORDINAL: u64 = 599;

        assert_eq!(SAMPLES, 640);
        let old_interval = measure(SAMPLE_BYTES, SAMPLE_BYTES, 16, 20, 18);
        let tail = measure(0, 0, 0, 3, 2);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, old_interval, SAMPLES, Some(tail));
        settle(&mut arena);
        let old_sample_root = donor_sample_root(&arena, &old);
        let old_distant_leaf = leaf_at_ordinal(&arena, old_sample_root, DISTANT_OLD_ORDINAL);
        let old_sample_summary = sequence_node::<DonorSampleSpec>(&arena, old_sample_root)
            .unwrap()
            .0;

        let restart_cut = repeated(old_interval, RESTART_SAMPLES);
        let old_convergence_cut = repeated(old_interval, CONVERGENCE_SAMPLES);
        let changed_a = measure(7_003, 5_002, 4, 8, 5);
        let changed_b = measure(8_111, 6_020, 6, 9, 6);
        let final_interval = measure(9_777, 7_000, 5, 10, 7);
        let fresh_total = changed_a
            .checked_add(changed_b)
            .unwrap()
            .checked_add(final_interval)
            .unwrap();
        let changed_samples = vec![
            DonorCheckpointSampleDraft::try_new(
                changed_a,
                parser
                    .capture_durable_grammar_line_boundary_checkpoint()
                    .unwrap(),
            )
            .unwrap(),
            DonorCheckpointSampleDraft::try_new(
                changed_b,
                parser
                    .capture_durable_grammar_line_boundary_checkpoint()
                    .unwrap(),
            )
            .unwrap(),
        ];
        let request = DonorSuffixSpliceRequest::try_new(
            restart_cut,
            old_convergence_cut,
            changed_samples,
            final_interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();

        let (fresh, receipt, build_receipt, polls) = run_splice(&mut arena, &old, request);
        settle(&mut arena);
        let new_summary = fresh.summary(&arena).unwrap();
        let old_donor_total = repeated(old_interval, SAMPLES as u64);
        let retained_suffix = old_donor_total
            .checked_difference_from(old_convergence_cut)
            .unwrap();
        let expected_donor = restart_cut
            .checked_add(fresh_total)
            .unwrap()
            .checked_add(retained_suffix)
            .unwrap();
        let expected_total = expected_donor.checked_add(tail).unwrap();

        assert_eq!(new_summary.measure, expected_total);
        assert_eq!(new_summary.partitions, 2);
        assert!(new_summary.terminal_tail);
        assert_eq!(new_summary.samples, 640 - 200 + 3);
        assert_eq!(receipt.old_samples, 640);
        assert_eq!(receipt.old_samples_replaced, 200);
        assert_eq!(receipt.fresh_samples_inserted, 3);
        assert_eq!(receipt.source_bytes_replaced, 200 * SAMPLE_BYTES);
        assert_eq!(receipt.source_bytes_inserted, fresh_total.source_bytes());
        assert_eq!(
            receipt.source_bytes_retained,
            TEN_MIB - receipt.source_bytes_replaced
        );
        assert!(receipt.suffix_relevant_continuation_identity);
        assert!(receipt.terminal_tail_retained);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(build_receipt.retained_source_bytes, 0);
        assert_eq!(receipt.boundary_leaf_pages_decoded, 2);
        assert_eq!(receipt.boundary_leaf_pages_reencoded, 2);
        assert!(receipt.boundary_sample_records_decoded < 100);
        assert_eq!(receipt.old_sample_leaf_pages, old_sample_summary.leaf_pages);
        assert_eq!(
            receipt.sequence.leaves_reused,
            usize::try_from(receipt.old_leaf_pages_retained).unwrap()
        );
        assert!(receipt.old_leaf_pages_retained > 0);
        assert!(polls > receipt.fresh_samples_inserted);
        eprintln!(
            "checkpoint_suffix_splice_10mib old_leaves={} retained={} boundary_reencoded={} replacement_leaves={} path_nodes={} boundary_records={} replaced_samples={} fresh_samples={} polls={polls}",
            receipt.old_sample_leaf_pages,
            receipt.old_leaf_pages_retained,
            receipt.boundary_leaf_pages_reencoded,
            receipt.replacement_leaf_pages,
            receipt.sample_tree_nodes_visited,
            receipt.boundary_sample_records_decoded,
            receipt.old_samples_replaced,
            receipt.fresh_samples_inserted,
        );

        let new_sample_root = donor_sample_root(&arena, &fresh);
        let distant_new_ordinal = DISTANT_OLD_ORDINAL - receipt.old_samples_replaced
            + receipt.fresh_samples_inserted as u64;
        assert_eq!(
            leaf_at_ordinal(&arena, new_sample_root, distant_new_ordinal),
            old_distant_leaf,
            "a full distant suffix leaf must survive by ArenaId"
        );

        let new_convergence_cut = restart_cut.checked_add(fresh_total).unwrap();
        let convergence = fresh
            .locate_donor_checkpoint_at_or_before_cut(&arena, new_convergence_cut.source_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(convergence.checkpoint_cut(), new_convergence_cut);
        assert_eq!(convergence.ordinal(), RESTART_SAMPLES + 2);

        let old_distant_cut = repeated(old_interval, DISTANT_OLD_ORDINAL + 1);
        let rebased_distant_cut = new_convergence_cut
            .checked_add(
                old_distant_cut
                    .checked_difference_from(old_convergence_cut)
                    .unwrap(),
            )
            .unwrap();
        let selected = fresh
            .locate_donor_checkpoint_at_or_before_cut(&arena, rebased_distant_cut.source_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(selected.ordinal(), distant_new_ordinal);
        assert_eq!(selected.checkpoint_cut(), rebased_distant_cut);

        // The old root remained independently queryable throughout the build.
        assert_eq!(
            old.summary(&arena).unwrap().measure,
            old_donor_total.checked_add(tail).unwrap()
        );
        assert_eq!(
            old.locate_donor_checkpoint_at_or_before_cut(&arena, TEN_MIB)
                .unwrap()
                .unwrap()
                .ordinal(),
            639
        );

        fresh.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[allow(clippy::too_many_lines)] // The gate couples role authentication, five-axis rebasing, and exact page-retention receipts.
    #[test]
    fn ten_mib_setext_level_one_group_splices_page_natively_and_preserves_role() {
        const TEN_MIB: u64 = 10 * 1024 * 1024;
        const SAMPLE_BYTES: u64 = 16 * 1024;
        const SAMPLES: usize = (TEN_MIB / SAMPLE_BYTES) as usize;
        const RESTART_SAMPLES: u64 = 200;
        const CONVERGENCE_SAMPLES: u64 = 400;
        const DISTANT_OLD_ORDINAL: u64 = 599;
        const GROUP: u64 = 77;

        assert_eq!(SAMPLES, 640);
        let outcome = StorageOnlyNormalizationOutcome::SetextHeading { level: 1 };
        let old_interval = measure(SAMPLE_BYTES, 12_000, 16, 20, 18);
        let tail = measure(0, 0, 0, 3, 2);
        let parser = donor_after_line("setext\n");
        let mut arena = PageArena::new();
        let old = normalization_index_with(
            &mut arena,
            &parser,
            old_interval,
            SAMPLES,
            GROUP,
            outcome,
            Some(tail),
        );
        settle(&mut arena);
        let old_sample_root = donor_sample_root(&arena, &old);
        let old_distant_leaf = leaf_at_ordinal(&arena, old_sample_root, DISTANT_OLD_ORDINAL);
        let old_sample_summary = sequence_node::<DonorSampleSpec>(&arena, old_sample_root)
            .unwrap()
            .0;

        let restart_cut = repeated(old_interval, RESTART_SAMPLES);
        let old_convergence_cut = repeated(old_interval, CONVERGENCE_SAMPLES);
        let authority = normalization_authority(&arena, &old, restart_cut, old_convergence_cut);
        let changed_a = measure(7_003, 5_002, 4, 8, 5);
        let changed_b = measure(8_111, 6_020, 6, 9, 6);
        let final_interval = measure(9_777, 7_000, 5, 10, 7);
        let fresh_total = changed_a
            .checked_add(changed_b)
            .unwrap()
            .checked_add(final_interval)
            .unwrap();
        let changed_samples = vec![
            DonorCheckpointSampleDraft::try_new(
                changed_a,
                parser
                    .capture_durable_grammar_line_boundary_checkpoint()
                    .unwrap(),
            )
            .unwrap(),
            DonorCheckpointSampleDraft::try_new(
                changed_b,
                parser
                    .capture_durable_grammar_line_boundary_checkpoint()
                    .unwrap(),
            )
            .unwrap(),
        ];
        let request = DonorSuffixSpliceRequest::try_new(
            restart_cut,
            old_convergence_cut,
            changed_samples,
            final_interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();

        let (fresh, receipt, build_receipt, polls) =
            run_normalization_splice(&mut arena, &old, authority, request);
        settle(&mut arena);
        let old_donor_total = repeated(old_interval, SAMPLES as u64);
        let retained_suffix = old_donor_total
            .checked_difference_from(old_convergence_cut)
            .unwrap();
        let expected_donor = restart_cut
            .checked_add(fresh_total)
            .unwrap()
            .checked_add(retained_suffix)
            .unwrap();
        let expected_total = expected_donor.checked_add(tail).unwrap();
        let new_summary = fresh.summary(&arena).unwrap();

        assert_eq!(new_summary.measure, expected_total);
        assert_eq!(new_summary.partitions, 2);
        assert!(new_summary.terminal_tail);
        assert_eq!(new_summary.samples, 640 - 200 + 3);
        assert_eq!(receipt.old_samples, 640);
        assert_eq!(receipt.old_samples_replaced, 200);
        assert_eq!(receipt.fresh_samples_inserted, 3);
        assert_eq!(receipt.source_bytes_replaced, 200 * SAMPLE_BYTES);
        assert_eq!(receipt.source_bytes_inserted, fresh_total.source_bytes());
        assert_eq!(
            receipt.source_bytes_retained,
            TEN_MIB - receipt.source_bytes_replaced
        );
        assert!(receipt.suffix_relevant_continuation_identity);
        assert!(receipt.normalization_role_preserved);
        assert!(receipt.normalization_group_start_zero_storage_shape);
        assert!(receipt.terminal_tail_retained);
        assert!(receipt.mechanism_only_unpublishable);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(build_receipt.retained_source_bytes, 0);
        assert_eq!(receipt.boundary_leaf_pages_decoded, 2);
        assert_eq!(receipt.boundary_leaf_pages_reencoded, 2);
        assert!(receipt.boundary_sample_records_decoded < 100);
        assert_eq!(receipt.old_sample_leaf_pages, old_sample_summary.leaf_pages);
        assert_eq!(
            receipt.sequence.leaves_reused,
            usize::try_from(receipt.old_leaf_pages_retained).unwrap()
        );
        assert!(receipt.old_leaf_pages_retained > 0);
        assert!(polls > receipt.fresh_samples_inserted);
        eprintln!(
            "checkpoint_setext_suffix_splice_10mib old_leaves={} retained={} boundary_reencoded={} replacement_leaves={} path_nodes={} boundary_records={} replaced_samples={} fresh_samples={} polls={polls}",
            receipt.old_sample_leaf_pages,
            receipt.old_leaf_pages_retained,
            receipt.boundary_leaf_pages_reencoded,
            receipt.replacement_leaf_pages,
            receipt.sample_tree_nodes_visited,
            receipt.boundary_sample_records_decoded,
            receipt.old_samples_replaced,
            receipt.fresh_samples_inserted,
        );

        let new_sample_root = donor_sample_root(&arena, &fresh);
        let distant_new_ordinal = DISTANT_OLD_ORDINAL - receipt.old_samples_replaced
            + receipt.fresh_samples_inserted as u64;
        assert_eq!(
            leaf_at_ordinal(&arena, new_sample_root, distant_new_ordinal),
            old_distant_leaf,
            "a full distant Setext suffix leaf must survive by ArenaId"
        );

        let new_convergence_cut = restart_cut.checked_add(fresh_total).unwrap();
        let convergence = fresh
            .locate_donor_checkpoint_at_or_before_cut(&arena, new_convergence_cut.source_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(convergence.checkpoint_cut(), new_convergence_cut);
        assert_eq!(convergence.ordinal(), RESTART_SAMPLES + 2);
        let CommittedDonorCheckpointRole::Normalization(role) =
            convergence.committed_role(&fresh, &arena).unwrap()
        else {
            panic!("spliced convergence must remain a normalization checkpoint");
        };
        assert_eq!(role.group(), GROUP);
        assert_eq!(role.outcome(), outcome);
        assert_eq!(role.bounds().start(), RelativeCheckpointMeasure::default());
        assert_eq!(role.bounds().interval(), expected_donor);
        assert_eq!(role.bounds().end(), expected_donor);

        let old_distant_cut = repeated(old_interval, DISTANT_OLD_ORDINAL + 1);
        let rebased_distant_cut = new_convergence_cut
            .checked_add(
                old_distant_cut
                    .checked_difference_from(old_convergence_cut)
                    .unwrap(),
            )
            .unwrap();
        let selected = fresh
            .locate_donor_checkpoint_at_or_before_cut(&arena, rebased_distant_cut.source_bytes())
            .unwrap()
            .unwrap();
        assert_eq!(selected.ordinal(), distant_new_ordinal);
        assert_eq!(selected.checkpoint_cut(), rebased_distant_cut);

        // The old committed group remains independently queryable throughout.
        assert_eq!(
            old.summary(&arena).unwrap().measure,
            old_donor_total.checked_add(tail).unwrap()
        );
        assert_eq!(
            old.locate_donor_checkpoint_at_or_before_cut(&arena, TEN_MIB)
                .unwrap()
                .unwrap()
                .ordinal(),
            639
        );

        fresh.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[allow(clippy::too_many_lines)] // This is the decisive parent-query + retained-child journal proof.
    #[test]
    fn parent_bound_setext_authority_splices_the_exact_retained_child() {
        const TEN_MIB: u64 = 10 * 1024 * 1024;
        const SAMPLE_BYTES: u64 = 16 * 1024;
        const SAMPLES: usize = (TEN_MIB / SAMPLE_BYTES) as usize;
        const RESTART_SAMPLES: u64 = 200;
        const CONVERGENCE_SAMPLES: u64 = 400;
        const DISTANT_OLD_ORDINAL: u64 = 599;
        const GROUP: u64 = 91;

        let outcome = StorageOnlyNormalizationOutcome::SetextHeading { level: 1 };
        // 640 coverage events plus Document/Paragraph enter+exit equals the
        // checkpoint index's 640 sample events plus this four-event tail.
        let old_interval = measure(SAMPLE_BYTES, SAMPLE_BYTES, 16, 1, 1);
        let tail = measure(0, 0, 0, 4, 0);
        // These list states have different cumulative child folds (loose vs
        // tight), but identical suffix-relevant grammar and predecessor
        // line-local state. The legacy full-output recipes prove the output
        // difference; canonical grammar+line-local identity must still admit C.
        let old_parser = donor_after_lines(&["- a\n", "\n", "- b\n"]);
        let fresh_parser = donor_after_lines(&["- a\n", "- b\n"]);
        let old_full = old_parser
            .capture_durable_line_boundary_checkpoint()
            .unwrap();
        let fresh_full = fresh_parser
            .capture_durable_line_boundary_checkpoint()
            .unwrap();
        assert!(
            old_full.header() != fresh_full.header()
                || !old_full.frame_records().eq(fresh_full.frame_records()),
            "fixture must differ in revision-cumulative output"
        );
        let mut arena = PageArena::new();
        let parent = normalization_restart_parent(
            &mut arena,
            &old_parser,
            old_interval,
            SAMPLES,
            GROUP,
            outcome,
            tail,
        );
        settle(&mut arena);

        let restart_cut = repeated(old_interval, RESTART_SAMPLES);
        let old_convergence_cut = repeated(old_interval, CONVERGENCE_SAMPLES);
        let restart = parent
            .locate_donor_checkpoint_at_or_before_cut(&arena, restart_cut.source_bytes())
            .unwrap()
            .unwrap()
            .into_normalization_splice_checkpoint()
            .unwrap();
        let convergence = parent
            .locate_donor_checkpoint_at_or_before_cut(&arena, old_convergence_cut.source_bytes())
            .unwrap()
            .unwrap()
            .into_normalization_splice_checkpoint()
            .unwrap();
        let authority =
            NormalizationDonorSuffixSpliceAuthority::try_new_parent_bound(restart, convergence)
                .unwrap();

        let changed_a = measure(7_003, 5_002, 4, 8, 5);
        let changed_b = measure(8_111, 6_020, 6, 9, 6);
        let final_interval = measure(9_777, 7_000, 5, 10, 7);
        let fresh_total = changed_a
            .checked_add(changed_b)
            .unwrap()
            .checked_add(final_interval)
            .unwrap();
        let request = DonorSuffixSpliceRequest::try_new(
            restart_cut,
            old_convergence_cut,
            vec![
                DonorCheckpointSampleDraft::try_new(
                    changed_a,
                    fresh_parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
                DonorCheckpointSampleDraft::try_new(
                    changed_b,
                    fresh_parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ],
            final_interval,
            fresh_parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let selection = parent
            .parent_selection_stamp_for_test(session.arena())
            .unwrap();
        let adoption = parent
            .retain_children_for_adoption(&mut session)
            .unwrap()
            .join_parent_selection(selection)
            .unwrap();
        let retained = adoption.checkpoint_index_for_splice(&session).unwrap();
        let old_root = retained.validated_root(&session).unwrap();
        let old_sample_root = donor_sample_root_from_index_root(session.arena(), old_root);
        let old_distant_leaf =
            leaf_at_ordinal(session.arena(), old_sample_root, DISTANT_OLD_ORDINAL);
        let old_convergence_path =
            locate_exact_endpoint(session.arena(), old_sample_root, old_convergence_cut)
                .unwrap()
                .selected()
                .unwrap()
                .1;
        let mut job = DonorSuffixSpliceJob::try_new_normalization_from_parent(
            &session, &retained, authority, request,
        )
        .unwrap();
        let mut polls = 0_usize;
        while job.poll(&mut session).unwrap() == DonorSuffixSpliceProgress::Pending {
            polls += 1;
            assert!(
                polls < 10_000,
                "parent-bound Setext splice did not converge"
            );
        }
        let receipt = job.receipt();
        let manifest = job.take_manifest().unwrap();
        drop(job);
        assert_eq!(session.live_owners().unwrap(), 3);
        let output_root = manifest.validate_composite_child(&session).unwrap();
        let output_summary = sequence_node::<CheckpointIndexSpec>(session.arena(), output_root)
            .unwrap()
            .0;
        let old_donor_total = repeated(old_interval, SAMPLES as u64);
        let retained_suffix = old_donor_total
            .checked_difference_from(old_convergence_cut)
            .unwrap();
        let expected_donor = restart_cut
            .checked_add(fresh_total)
            .unwrap()
            .checked_add(retained_suffix)
            .unwrap();
        assert_eq!(
            output_summary.measure,
            expected_donor.checked_add(tail).unwrap()
        );
        assert_eq!(output_summary.samples, 640 - 200 + 3);
        assert_eq!(output_summary.partitions, 2);
        assert!(output_summary.terminal_tail);

        let output_records =
            decode_outer_leaf_records_in_arena(session.arena(), output_root).unwrap();
        let DecodedPartitionKind::Donor { child_ordinal } = output_records[0].kind else {
            panic!("parent splice output must begin with a donor group");
        };
        let output_manifest = session
            .arena()
            .packed_child_at(output_root, usize::from(child_ordinal))
            .unwrap();
        let output_donor =
            decode_donor_partition_manifest(session.arena(), output_manifest).unwrap();
        assert_eq!(
            output_donor.role,
            DonorPartitionRole::Normalization {
                group: GROUP,
                outcome
            }
        );
        assert_eq!(output_donor.measure, expected_donor);
        let distant_new_ordinal = DISTANT_OLD_ORDINAL - receipt.old_samples_replaced
            + receipt.fresh_samples_inserted as u64;
        assert_eq!(
            leaf_at_ordinal(
                session.arena(),
                output_donor.sample_root,
                distant_new_ordinal,
            ),
            old_distant_leaf
        );
        let new_convergence_cut = restart_cut.checked_add(fresh_total).unwrap();
        let located = locate_exact_endpoint(
            session.arena(),
            output_donor.sample_root,
            new_convergence_cut,
        )
        .unwrap();
        assert_eq!(located.sample_ordinal, RESTART_SAMPLES + 2);
        let fresh_convergence_path = located.selected().unwrap().1;
        assert_ne!(
            fresh_convergence_path, old_convergence_path,
            "C must be encoded from the fresh draft; old sharing starts strictly after C"
        );

        assert_eq!(receipt.old_samples, 640);
        assert_eq!(receipt.old_samples_replaced, 200);
        assert_eq!(receipt.fresh_samples_inserted, 3);
        assert_eq!(receipt.boundary_leaf_pages_decoded, 2);
        assert_eq!(receipt.boundary_leaf_pages_reencoded, 2);
        assert!(receipt.boundary_sample_records_decoded < 100);
        assert_eq!(receipt.old_leaf_pages_retained, 12);
        assert_eq!(receipt.sequence.leaves_reused, 12);
        assert!(receipt.suffix_relevant_continuation_identity);
        assert!(receipt.normalization_role_preserved);
        assert!(receipt.normalization_group_start_zero_storage_shape);
        assert!(receipt.terminal_tail_retained);
        assert!(receipt.mechanism_only_unpublishable);
        assert_eq!(receipt.retained_source_bytes, 0);
        eprintln!(
            "checkpoint_parent_setext_suffix_splice_10mib old_leaves={} retained={} boundary_reencoded={} replacement_leaves={} path_nodes={} boundary_records={} replaced_samples={} fresh_samples={} polls={polls}",
            receipt.old_sample_leaf_pages,
            receipt.old_leaf_pages_retained,
            receipt.boundary_leaf_pages_reencoded,
            receipt.replacement_leaf_pages,
            receipt.sample_tree_nodes_visited,
            receipt.boundary_sample_records_decoded,
            receipt.old_samples_replaced,
            receipt.fresh_samples_inserted,
        );

        // This proof intentionally aborts the unjoined child replacement; the
        // old composite parent must remain independently queryable afterward.
        let build = session.begin_abort().unwrap();
        finish_abort(&mut arena, build);
        assert_eq!(
            parent
                .locate_donor_checkpoint_at_or_before_cut(&arena, TEN_MIB)
                .unwrap()
                .unwrap()
                .ordinal(),
            639
        );

        parent.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn cancellation_reclaims_new_pages_without_touching_old_root() {
        let interval = measure(1_024, 900, 1, 3, 2);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 128, None);
        settle(&mut arena);
        let baseline_nodes = arena.metrics().live_nodes;
        let restart_cut = repeated(interval, 20);
        let convergence_cut = repeated(interval, 100);
        let changed = (0..48)
            .map(|_| {
                DonorCheckpointSampleDraft::try_new(
                    measure(513, 400, 1, 2, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let request = DonorSuffixSpliceRequest::try_new(
            restart_cut,
            convergence_cut,
            changed,
            measure(777, 600, 1, 2, 1),
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        let mut job = DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        for _ in 0..55 {
            assert_eq!(
                job.poll(&mut session).unwrap(),
                DonorSuffixSpliceProgress::Pending
            );
        }
        assert!(session.live_owners().unwrap() > 0);
        let build = job.cancel(session).unwrap();
        finish_abort(&mut arena, build);
        assert_eq!(arena.metrics().live_nodes, baseline_nodes);
        assert_eq!(
            old.locate_donor_checkpoint_at_or_before_cut(&arena, 128 * 1_024)
                .unwrap()
                .unwrap()
                .ordinal(),
            127
        );

        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn deep_fresh_path_allocates_at_most_one_node_per_poll_and_cancels() {
        const QUOTE_DEPTH: usize = 256;
        const CANCELLATION_DEPTH: usize = 32;

        let line = format!("{}alpha\n", "> ".repeat(QUOTE_DEPTH));
        let parser = donor_after_line(&line);
        assert_eq!(
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap()
                .receipt()
                .materialized_path_records,
            QUOTE_DEPTH + 2,
            "fixture must exercise a genuinely deep donor open path"
        );
        let interval = measure(line.len() as u64, line.len() as u64, 1, 3, 2);
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 8, None);
        settle(&mut arena);
        let baseline_nodes = arena.metrics().live_nodes;
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 2),
            repeated(interval, 6),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        let mut job = DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut allocation_polls = 0_usize;
        loop {
            let before = job.core.build_receipt.donor_path_nodes_allocated;
            assert_eq!(
                job.poll(&mut session).unwrap(),
                DonorSuffixSpliceProgress::Pending
            );
            let after = job.core.build_receipt.donor_path_nodes_allocated;
            assert!(
                after - before <= 1,
                "a fresh donor path poll allocated more than one frame node"
            );
            allocation_polls += usize::from(after != before);
            if after >= CANCELLATION_DEPTH {
                break;
            }
            assert!(
                allocation_polls < 1_000,
                "deep donor path did not reach its bounded allocation phase"
            );
        }
        assert_eq!(
            job.core.build_receipt.donor_path_nodes_allocated,
            CANCELLATION_DEPTH
        );
        assert_eq!(allocation_polls, CANCELLATION_DEPTH);
        let build = job.cancel(session).unwrap();
        finish_abort(&mut arena, build);
        assert_eq!(arena.metrics().live_nodes, baseline_nodes);
        assert_eq!(
            old.locate_donor_checkpoint_at_or_before_cut(&arena, 8 * interval.source_bytes())
                .unwrap()
                .unwrap()
                .ordinal(),
            7
        );

        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn wrong_session_cancellation_returns_splice_authority_for_recovery() {
        let interval = measure(256, 220, 1, 3, 2);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 8, None);
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 2),
            repeated(interval, 6),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        let job = DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request).unwrap();
        let session = arena.resume_build(ticket).unwrap();
        let ticket = session.suspend().unwrap();

        let wrong_ticket = arena.begin_build().unwrap();
        let wrong_session = arena.resume_build(wrong_ticket).unwrap();
        let failure = job.cancel(wrong_session).unwrap_err();
        let (error, mut job, wrong_session) = failure.into_parts();
        assert!(matches!(error, CommittedCheckpointIndexError::Invalid(_)));
        assert_eq!(wrong_session.live_owners().unwrap(), 0);
        let wrong_build = wrong_session.begin_abort().unwrap();
        finish_abort(&mut arena, wrong_build);

        // The failure was localized: the original job and its suspended build
        // ticket remain usable after the unrelated session is cleaned up.
        let mut session = arena.resume_build(ticket).unwrap();
        assert_eq!(
            job.poll(&mut session).unwrap(),
            DonorSuffixSpliceProgress::Pending
        );
        let build = job.cancel(session).unwrap();
        finish_abort(&mut arena, build);
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn parent_lease_finishes_with_old_checkpoint_and_new_index_as_two_owners() {
        let interval = measure(256, 220, 1, 3, 2);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 16, None);
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 4),
            repeated(interval, 12),
            Vec::new(),
            measure(300, 240, 1, 3, 2),
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();

        let old_root = old.checked_root_id(&arena).unwrap();
        let descriptor =
            validate_committed_checkpoint_index_composite_child(&arena, old_root).unwrap();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let retained_old = session.retain(old_root).unwrap();
        let manifest = {
            let lease = ParentRetainedCheckpointIndexLease::mechanism_only_from_retained_test_index(
                session.id(),
                old.scoped_root_id(),
                &retained_old,
                descriptor,
            );
            let mut job =
                DonorSuffixSpliceJob::try_new_from_parent(&session, &lease, request).unwrap();
            let mut polls = 0;
            while job.poll(&mut session).unwrap() == DonorSuffixSpliceProgress::Pending {
                polls += 1;
                assert!(polls < 1_000);
            }
            assert!(job.receipt().maximum_fresh_path_depth > 0);
            assert_eq!(
                session.live_owners().unwrap(),
                2,
                "the parent journal must own exactly retained-old-index + new-index"
            );
            job.take_manifest().unwrap()
        };
        session.release(retained_old).unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        let fresh = manifest.commit(session).unwrap().0;
        assert_eq!(fresh.summary(&arena).unwrap().samples, 16 - 8 + 1);

        fresh.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn parent_owned_job_outlives_lease_and_resumes_one_poll_per_session() {
        let interval = measure(256, 220, 1, 3, 2);
        let fresh_interval = measure(193, 170, 1, 2, 1);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 32, None);
        let changed = (0..3)
            .map(|_| {
                DonorCheckpointSampleDraft::try_new(
                    fresh_interval,
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 5),
            repeated(interval, 25),
            changed,
            fresh_interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut job = parent_owned_direct_job_after_lease_scope(&mut session, &old, request);
        assert_eq!(job.build_id(), session.id());
        assert_eq!(
            session.live_owners().unwrap(),
            1,
            "after the borrowed parent lease ends, only the job-owned old base remains"
        );
        let mut ticket = session.suspend().unwrap();
        let mut polls = 0_usize;
        loop {
            let mut session = arena.resume_build(ticket).unwrap();
            let progress = job.poll(&mut session).unwrap();
            polls += 1;
            ticket = session.suspend().unwrap();
            assert_eq!(
                arena.build_lifecycle(ticket.id()).unwrap(),
                ArenaBuildLifecycle::Suspended
            );
            if progress == DonorSuffixSpliceProgress::Complete {
                break;
            }
            assert!(polls < 1_000, "parent-owned splice did not converge");
        }
        assert!(polls > 1, "proof must cross at least one actor suspension");
        assert_eq!(job.receipt().old_samples_replaced, 20);
        assert_eq!(
            arena
                .build_journal_metrics(ticket.id())
                .unwrap()
                .live_owners,
            1,
            "completion releases the extra old base and retains only the new index"
        );

        let manifest = job.take_manifest().unwrap();
        drop(job);
        let session = arena.resume_build(ticket).unwrap();
        let fresh = manifest.commit(session).unwrap().0;
        assert_eq!(fresh.summary(&arena).unwrap().samples, 32 - 20 + 4);
        assert_eq!(
            old.locate_donor_checkpoint_at_or_before_cut(&arena, 32 * 256)
                .unwrap()
                .unwrap()
                .ordinal(),
            31,
            "the independently published old index remains queryable"
        );

        fresh.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn parent_owned_job_cancellation_reclaims_its_base_after_resumed_polls() {
        let interval = measure(1_024, 900, 1, 3, 2);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 128, None);
        settle(&mut arena);
        let baseline_nodes = arena.metrics().live_nodes;
        let changed = (0..48)
            .map(|_| {
                DonorCheckpointSampleDraft::try_new(
                    measure(513, 400, 1, 2, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 20),
            repeated(interval, 100),
            changed,
            measure(777, 600, 1, 2, 1),
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();

        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut job = parent_owned_direct_job_after_lease_scope(&mut session, &old, request);
        let mut ticket = session.suspend().unwrap();
        for _ in 0..24 {
            let mut session = arena.resume_build(ticket).unwrap();
            assert_eq!(
                job.poll(&mut session).unwrap(),
                DonorSuffixSpliceProgress::Pending
            );
            ticket = session.suspend().unwrap();
        }
        let mut session = arena.resume_build(ticket).unwrap();
        assert!(
            session.live_owners().unwrap() > 1,
            "the partial splice should own its old base plus new working pages"
        );
        // One more resumed slice proves cancellation does not depend on the
        // parent lease or on cancelling from the original admission session.
        assert_eq!(
            job.poll(&mut session).unwrap(),
            DonorSuffixSpliceProgress::Pending
        );
        let build = job.cancel(session).unwrap();
        let zero = arena.poll_build_abort(build, 0).unwrap();
        assert!(!zero.complete);
        assert!(zero.owners_remaining > 0);
        let mut cancellation_polls = 0_usize;
        loop {
            cancellation_polls += 1;
            if arena.poll_build_abort(build, 1).unwrap().complete {
                break;
            }
            assert!(cancellation_polls < 1_000);
        }
        assert!(cancellation_polls > 1);
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, baseline_nodes);
        assert_eq!(
            old.locate_donor_checkpoint_at_or_before_cut(&arena, 128 * 1_024)
                .unwrap()
                .unwrap()
                .ordinal(),
            127
        );

        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[allow(clippy::too_many_lines)] // Negative matrix keeps binding corruptions and journal cleanup visibly coupled.
    #[test]
    fn normalization_admission_rejects_wrong_group_outcome_frontier_and_partition() {
        let interval = measure(100, 90, 1, 3, 2);
        let parser = donor_after_line("setext\n");
        let mut arena = PageArena::new();
        let old = normalization_index(&mut arena, &parser, interval);
        let restart_cut = repeated(interval, 1);
        let convergence_cut = repeated(interval, 3);

        let request = || {
            DonorSuffixSpliceRequest::try_new(
                restart_cut,
                convergence_cut,
                Vec::new(),
                interval,
                parser
                    .capture_durable_grammar_line_boundary_checkpoint()
                    .unwrap(),
            )
            .unwrap()
        };

        // Admission revalidates the opaque binding against the persisted role;
        // even an internally corrupted scalar copy cannot redirect the splice.
        let authority = normalization_authority(&arena, &old, restart_cut, convergence_cut);
        let wrong_group = NormalizationDonorSuffixSpliceAuthority {
            binding: CommittedNormalizationGroupBinding {
                group: 8,
                ..authority.binding
            },
            restart_cut: authority.restart_cut,
            convergence_cut: authority.convergence_cut,
        };
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new_normalization(
                &ticket,
                &arena,
                &old,
                wrong_group,
                request(),
            ),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        abort_ticket(&mut arena, ticket);

        let authority = normalization_authority(&arena, &old, restart_cut, convergence_cut);
        let wrong_outcome = NormalizationDonorSuffixSpliceAuthority {
            binding: CommittedNormalizationGroupBinding {
                outcome: StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
                ..authority.binding
            },
            restart_cut: authority.restart_cut,
            convergence_cut: authority.convergence_cut,
        };
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new_normalization(
                &ticket,
                &arena,
                &old,
                wrong_outcome,
                request(),
            ),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        abort_ticket(&mut arena, ticket);

        // A valid authority cannot be replayed with a caller-authored cut that
        // crosses the authenticated four-sample group frontier.
        let authority = normalization_authority(&arena, &old, restart_cut, convergence_cut);
        let crossed_frontier = DonorSuffixSpliceRequest::try_new(
            restart_cut,
            repeated(interval, 5),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new_normalization(
                &ticket,
                &arena,
                &old,
                authority,
                crossed_frontier,
            ),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        abort_ticket(&mut arena, ticket);

        // Independently authenticated checkpoints in adjacent partitions can
        // never mint one normalization-splice authority.
        let crossed = two_normalization_groups_index(
            &mut arena,
            &parser,
            interval,
            (
                11,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
            ),
            (
                12,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
            ),
        );
        let first = crossed
            .locate_donor_checkpoint_at_or_before_cut(&arena, repeated(interval, 2).source_bytes())
            .unwrap()
            .unwrap();
        let second = crossed
            .locate_donor_checkpoint_at_or_before_cut(&arena, repeated(interval, 6).source_bytes())
            .unwrap()
            .unwrap();
        let CommittedDonorCheckpointRole::Normalization(first) =
            first.committed_role(&crossed, &arena).unwrap()
        else {
            panic!("first crossed fixture must be normalization");
        };
        let CommittedDonorCheckpointRole::Normalization(second) =
            second.committed_role(&crossed, &arena).unwrap()
        else {
            panic!("second crossed fixture must be normalization");
        };
        assert!(matches!(
            NormalizationDonorSuffixSpliceAuthority::try_new(first, second),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));

        crossed.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn suffix_continuation_identity_rejects_changed_list_marker_and_fence() {
        let interval = measure(100, 90, 1, 3, 2);
        for (old_line, fresh_line) in [("- alpha\n", "+ alpha\n"), ("```rs\n", "~~~rs\n")] {
            let old_parser = donor_after_line(old_line);
            let fresh_parser = donor_after_line(fresh_line);
            let mut arena = PageArena::new();
            let old = direct_index(&mut arena, &old_parser, interval, 8, None);
            let request = DonorSuffixSpliceRequest::try_new(
                repeated(interval, 2),
                repeated(interval, 6),
                Vec::new(),
                interval,
                fresh_parser
                    .capture_durable_grammar_line_boundary_checkpoint()
                    .unwrap(),
            )
            .unwrap();
            let ticket = arena.begin_build().unwrap();
            assert!(matches!(
                DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request),
                Err(CommittedCheckpointIndexError::Invalid(_))
            ));
            abort_ticket(&mut arena, ticket);
            old.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[allow(clippy::too_many_lines)] // Direct-run rejection matrix shares one arena lifecycle fixture.
    #[test]
    fn rejects_wrong_arena_build_role_endpoint_and_convergence_state() {
        let interval = measure(100, 90, 1, 3, 2);
        let parser = donor_after_line("alpha\n");
        let mut arena = PageArena::new();
        let old = direct_index(&mut arena, &parser, interval, 8, None);

        // A job is build-generation-bound and remains usable after a wrong
        // session is rejected.
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 2),
            repeated(interval, 6),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        let job = DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request).unwrap();
        let other_ticket = arena.begin_build().unwrap();
        let mut other_session = arena.resume_build(other_ticket).unwrap();
        let mut job = job;
        assert!(matches!(
            job.poll(&mut other_session),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        let other_ticket = other_session.suspend().unwrap();
        abort_ticket(&mut arena, other_ticket);
        let session = arena.resume_build(ticket).unwrap();
        let build = job.cancel(session).unwrap();
        finish_abort(&mut arena, build);

        // The old root cannot be interpreted in another arena, even with a
        // valid ticket from that arena.
        let mut other_arena = PageArena::new();
        let other_ticket = other_arena.begin_build().unwrap();
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 2),
            repeated(interval, 6),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new(&other_ticket, &other_arena, &old, request),
            Err(CommittedCheckpointIndexError::Arena(
                ArenaError::WrongArena { .. }
            ))
        ));
        abort_ticket(&mut other_arena, other_ticket);

        // A source-byte endpoint with a different UTF-16 axis is not an exact
        // composite cut.
        let wrong_endpoint = measure(200, 181, 2, 6, 4);
        let request = DonorSuffixSpliceRequest::try_new(
            wrong_endpoint,
            repeated(interval, 6),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        abort_ticket(&mut arena, ticket);

        // A structurally different donor capture cannot claim convergence.
        let different = donor_after_line("> alpha\n");
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 2),
            repeated(interval, 6),
            Vec::new(),
            interval,
            different
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new(&ticket, &arena, &old, request),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        abort_ticket(&mut arena, ticket);

        // The first admitted semantic shape is explicitly a direct run.
        let normalization = normalization_index(&mut arena, &parser, interval);
        let request = DonorSuffixSpliceRequest::try_new(
            repeated(interval, 1),
            repeated(interval, 3),
            Vec::new(),
            interval,
            parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap(),
        )
        .unwrap();
        let ticket = arena.begin_build().unwrap();
        assert!(matches!(
            DonorSuffixSpliceJob::try_new(&ticket, &arena, &normalization, request),
            Err(CommittedCheckpointIndexError::Invalid(_))
        ));
        abort_ticket(&mut arena, ticket);

        normalization.release_later(&mut arena).unwrap();
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}

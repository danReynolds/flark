//! One candidate-owned source-to-packed-green composition boundary.
//!
//! This is the candidate-owned composition boundary. It proves that exact
//! source consumption, projection composition, structural event ordering,
//! resumable packed storage, and the first retroactive Paragraph-to-Setext
//! normalization share one poison/commit state. It does not yet prove the
//! composite durable checkpoint, retained suffix adoption, reference/table
//! normalization, or inline/reference roots.

use std::fmt;

#[cfg(feature = "exact-parser")]
mod reference_prefix;
#[cfg(feature = "exact-parser")]
use reference_prefix::*;
#[cfg(feature = "exact-parser")]
mod green_reference_composite;
#[cfg(feature = "exact-parser")]
use green_reference_composite::*;

#[cfg(feature = "exact-parser")]
use flark_comrak_value_block_core::{
    DirectLineBoundaryDeferredRole, DirectValueBlockParser, ParseError, SyntaxProfile,
};

#[cfg(feature = "exact-parser")]
use crate::committed_checkpoint_index::suffix_splice::{
    DonorSuffixSpliceProgress, DonorSuffixSpliceReceipt, DonorSuffixSpliceRequest,
    ParentOwnedDonorSuffixSpliceJob,
};
#[cfg(feature = "exact-parser")]
use crate::committed_checkpoint_index::{
    CommittedCheckpointIndexError, DonorCheckpointSampleDraft, ParentBoundDonorPartitionTransition,
    ParentBoundDonorSuccessor, ParentBoundDonorSuccessorStep, ParentSelectedRestartAnchor,
    ParentSelectedSeededRestartAnchor, ParentSelectedSuffixSampleCursor,
    ParentSelectedSuffixSampleOrigin, ParentSelectedSuffixSamplePrior,
    ParentSelectedSuffixSampleRollback, RelativeCheckpointMeasure,
    StorageOnlyCheckpointIndexBuildManifest, StorageOnlyCheckpointIndexBuilder,
    StorageOnlyCheckpointPartition,
};
#[cfg(feature = "exact-parser")]
use crate::indexed_donor_checkpoint::OpaqueDonorIdentityWitness;
use crate::live_document::DocumentIdentityAllocator;
#[cfg(feature = "exact-parser")]
use crate::parent_selected_convergence::{
    ParentSelectedConvergenceMapError, ParentSelectedConvergenceMapJob,
    ParentSelectedConvergenceMapStart, ParentSelectedConvergenceTargetRelation,
    ParentSelectedMappedConvergence,
};
#[cfg(feature = "exact-parser")]
use crate::same_build_checkpoint::{
    JoinedParserDonorSample, ParserLineBoundaryCheckpointAuthority,
};
#[cfg(feature = "exact-parser")]
use crate::serialized_green::setext_retained_restart::{
    ParentSelectedCanonicalFragmentOriginSeed, ParentSelectedDirectRetainedGreenRestart,
    ParentSelectedDirectRetainedGreenRestartError, ParentSelectedSetextRetainedGreenRestart,
    ParentSelectedSetextRetainedGreenRestartError, SealedSetextNormalizationManifest,
    SetextRetainedGreenRestartOutput, SetextRetainedGreenRestartProgress,
    SetextRetainedGreenRestartReceipt,
};
#[cfg(feature = "exact-parser")]
use crate::serialized_green::{BuilderGreenPrefixSnapshot, RetainedSetextGreenCheckpointDraft};
use crate::serialized_green::{
    CanonicalFragmentReplacement, ProvisionalParagraphEnter, SetextPromotion,
};
#[cfg(feature = "exact-parser")]
use crate::setext_cross_build_restart::{
    InMemorySetextCheckpointDraft, WitnessValidatedSetextDonorRecipe,
};
#[cfg(feature = "exact-parser")]
use crate::source_bound_ledger::{RestoredSetextSourceLedger, RetainedSetextSourceLedgerDraft};
#[cfg(feature = "exact-parser")]
use crate::storage_only_composite_document::{
    ParentSelectedRestartCompositeAdoptionLease, RestartCompositeDocument,
    RestartCompositeDocumentBuildReceipt, RestartCompositeDocumentBuilder,
    RestartCompositeDocumentError,
};
use crate::{
    ArenaBuildError, ArenaBuildId, ArenaBuildTicket, BlockId, CandidateLineReceipt,
    CandidateLogicalAction, CandidateOpenBinding, CandidateRangeReplayPlan,
    CandidateRangeReplaySourceReceipt, CandidateRecognitionBytePollError,
    CandidateRecognitionBytePollReceipt, CandidateRecognitionByteScanner,
    CandidateRecognitionByteSession, CandidateRecognitionByteSessionFinishReceipt,
    CandidateRecognitionCheckpoint, CandidateRecognitionLineReceipt, CandidateRecognitionPoll,
    CandidateRecognitionRangeKind, CandidateRecognitionRangeReceipt, CandidateRecognitionSink,
    CandidateRecognitionWindowError, CandidateRecognitionWindowReceipt, CandidateSourceAtom,
    CandidateSourceAtomKind, CandidateSourceLedger, CandidateSourcePoll,
    CandidateSourcePollReceipt, CandidateTerminatorResolution, CandidateWriterRangeRecipe,
    CanonicalFragmentProjectionOrigin, CanonicalFragmentProjectionRebase, ClosedChildAggregate,
    ComposerSealedProjectionRunCapability, CoveragePart, DeferredNormalizationIdentity,
    EntityIdentityKind, FactsEnvelope, FreshBlockPermit, GrammarRevision, GreenAffinity,
    GreenCloseFacts, GreenEvent, GreenFencedCodeCloseFacts, GreenFencedCodeOpenFacts,
    GreenHeadingOpenFacts, GreenHeadingStyle, GreenItemOpenFacts, GreenKind, GreenListOpenFacts,
    GreenRelativeLogicalSlice, GreenTableAlignment, GreenTableCellOpenFacts, GreenTableOpenFacts,
    GreenTableRowOpenFacts, LiveCandidateEpoch, LiveDocumentError, LogicalContribution, PageArena,
    ResolvedWholeNormalizationIdentity, ResumableSerializedGreenBuild,
    SerializedGreenBuildManifest, SerializedGreenBuildReceipt, SerializedGreenDocument,
    SerializedGreenError, SerializedGreenRootSpec, SerializedGreenStreamProgress, SerializedMetric,
    SourceBoundLedgerError, SourceBoundProjectionComposer, SourceLedgerMetric,
    SourcePhysicalLineDescriptor, SourceProjectionComposerCompletionSeal,
    SourceProjectionComposerError, SourceProjectionComposerProgress, SourceProjectionRun,
};

/// Lexical token allowing this module alone to open the exact driver's
/// terminal convergence carrier and the mechanism-only green result in one
/// parent-selected adoption transaction.
#[cfg(feature = "exact-parser")]
pub(crate) struct ParentSelectedAdoptionSpliceMint(());

/// Lexical capability proving that initial reference-index authority,
/// occurrences, and the final manifest all cross the concrete writer join.
/// No other module can construct this zero-data mint.
pub(crate) struct ReferenceCandidateIndexWriterMint(());
#[cfg(feature = "exact-parser")]
use crate::{
    CandidateLineBoundaryDeferredRole, CandidateSourceLineBoundaryContinuation,
    SourceBoundGreenTailAdoption, SourceProjectionComposerLineBoundaryContinuation,
    SourceProjectionComposerTailAdoptionSeal, SourceProjectionLineBoundaryStorageAck,
    SourceResumeCursorPair, SourceStore,
};

/// Maximum shared-decoder work units spent by one exact range-replay poll.
/// Large source spans remain one parser command while yielding cooperatively.
pub const CANDIDATE_RANGE_REPLAY_MAX_SOURCE_WORK_PER_POLL: usize = 256;

/// Actor-derived configuration that is not source or arena authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateWriterConfig {
    pub syntax_profile: u64,
    pub grammar_revision: GrammarRevision,
    pub semantic_epoch: u64,
}

/// Opaque, single-use source atom. The embedded transition nonce is checked
/// against the writer that emitted it; dropping it intentionally strands the
/// candidate until cancellation rather than permitting a replay or skip.
#[must_use = "a source atom must be consumed by its issuing candidate writer"]
pub struct CandidateWriterSourceAtom {
    transition: u64,
    atom: CandidateSourceAtom,
}

impl CandidateWriterSourceAtom {
    #[must_use]
    pub const fn kind(&self) -> CandidateSourceAtomKind {
        self.atom.kind()
    }
}

impl fmt::Debug for CandidateWriterSourceAtom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateWriterSourceAtom")
            .field("kind", &self.atom.kind())
            .field("range", &self.atom.absolute_range())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum CandidateWriterSourcePoll {
    NeedFuel(CandidateSourcePollReceipt),
    Atom {
        atom: CandidateWriterSourceAtom,
        receipt: CandidateSourcePollReceipt,
    },
    Eof(CandidateSourcePollReceipt),
}

/// Parser-visible binding. It deliberately exposes neither `BlockId` nor the
/// ledger stamp needed to forge an owner relationship.
#[must_use = "an open writer binding must be closed or discarded with its candidate"]
pub struct CandidateWriterBinding {
    binding: CandidateOpenBinding,
}

/// Opaque parser-side equality witness. It authorizes no source or storage
/// operation; it only lets the exact driver verify that a retroactive writer
/// transition returned the same primary semantic identity it consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CandidateWriterBindingIdentity {
    epoch: LiveCandidateEpoch,
    block: crate::BlockId,
}

impl CandidateWriterBinding {
    #[must_use]
    pub const fn kind(&self) -> GreenKind {
        self.binding.kind()
    }

    pub(crate) const fn identity(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> CandidateWriterBindingIdentity {
        CandidateWriterBindingIdentity {
            epoch,
            block: self.binding.block_id(),
        }
    }
}

impl fmt::Debug for CandidateWriterBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateWriterBinding")
            .field("kind", &self.binding.kind())
            .finish_non_exhaustive()
    }
}

/// Typed logical intent. Construction of the source-bound action happens
/// inside `CandidateWriter::start_consume`; this enum carries no reusable
/// source boundary, metric, target ID, or raw projection program.
#[derive(Debug)]
pub enum CandidateWriterLogicalAction<'a> {
    None,
    Identity {
        target: &'a CandidateWriterBinding,
    },
    Hidden {
        target: &'a CandidateWriterBinding,
        affinity: GreenAffinity,
    },
    TabToSpaces {
        target: &'a CandidateWriterBinding,
        spaces: u8,
    },
    NulToReplacement {
        target: &'a CandidateWriterBinding,
    },
    CanonicalLineEnding {
        target: &'a CandidateWriterBinding,
    },
}

/// One source-ordered input to an active streamed table-header normalization.
/// Offsets are relative to the provisional Paragraph's exact physical start.
/// The production grammar adapter will mint these from actor-bound row
/// scanner output; callers cannot submit a whole row or a cell vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateTableHeaderInput {
    BeginCell {
        source_start: SerializedMetric,
        source_end: SerializedMetric,
        alignment: GreenTableAlignment,
    },
    Coverage {
        source_end: SerializedMetric,
        part: CoveragePart,
        logical: CandidateTableHeaderLogical,
    },
    EndCell,
    Finish {
        content_end: SerializedMetric,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateTableHeaderLogical {
    None,
    Identity,
    Hidden { affinity: GreenAffinity },
}

/// One cooperative writer poll. `ActionComplete` and `Opened` are returned
/// only after the builder has explicitly acknowledged the preceding event via
/// `SerializedGreenStreamProgress::ReadyForEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRangeReplayReceipt {
    source: crate::SourceSnapshotDescriptor,
    build: ArenaBuildId,
    line_ordinal: u64,
    absolute_start: u64,
    absolute_end: u64,
    physical_metric: SourceLedgerMetric,
    writer_polls: u64,
    source_work_units: u64,
    source_bytes_read: u64,
    atoms_scanned: u64,
    source_pieces: u64,
    maximum_pending_atoms: usize,
    maximum_pending_boundaries: usize,
}

impl CandidateRangeReplayReceipt {
    #[must_use]
    pub const fn source(self) -> crate::SourceSnapshotDescriptor {
        self.source
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn absolute_range(self) -> (u64, u64) {
        (self.absolute_start, self.absolute_end)
    }

    #[must_use]
    pub const fn physical_bytes(self) -> u64 {
        self.absolute_end - self.absolute_start
    }

    #[must_use]
    pub const fn physical_metric(self) -> SourceLedgerMetric {
        self.physical_metric
    }

    #[must_use]
    pub const fn writer_polls(self) -> u64 {
        self.writer_polls
    }

    #[must_use]
    pub const fn source_work_units(self) -> u64 {
        self.source_work_units
    }

    #[must_use]
    pub const fn source_bytes_read(self) -> u64 {
        self.source_bytes_read
    }

    #[must_use]
    pub const fn atoms_scanned(self) -> u64 {
        self.atoms_scanned
    }

    #[must_use]
    pub const fn source_pieces(self) -> u64 {
        self.source_pieces
    }

    #[must_use]
    pub const fn maximum_pending_atoms(self) -> usize {
        self.maximum_pending_atoms
    }

    #[must_use]
    pub const fn maximum_pending_boundaries(self) -> usize {
        self.maximum_pending_boundaries
    }
}

#[derive(Debug)]
pub enum CandidateWriterProgress {
    Pending,
    ActionComplete,
    Opened(CandidateWriterBinding),
    /// The active provisional Paragraph was atomically normalized in packed
    /// green and then retyped in the source ledger under the same `BlockId`.
    Retyped {
        binding: CandidateWriterBinding,
        facts: GreenHeadingOpenFacts,
    },
    /// A restart-crossing Paragraph normalization installed a fresh canonical
    /// Heading identity while retaining the consumed Paragraph identity as a
    /// writer-private residual. The distinct progress kind lets the exact
    /// driver acknowledge that transition without exposing any authority to
    /// name or reopen the residual itself.
    RetypedWithDeferredResidual {
        binding: CandidateWriterBinding,
        facts: GreenHeadingOpenFacts,
    },
    /// The generic fragment splice is waiting for one bounded parser input.
    /// The action remains installed; only `supply_table_header_input` may
    /// advance it.
    TableHeaderInputReady,
    #[cfg(feature = "exact-parser")]
    ReferencePrefixSourceReady {
        identity: crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionIdentity,
    },
    #[cfg(feature = "exact-parser")]
    ReferencePrefixTerminalReady(CandidateReferencePrefixTerminal),
    /// A provisional Paragraph was replaced by an open canonical Table. The
    /// returned binding is the sole live source-ledger owner for the
    /// delimiter and later body rows.
    RetypedTable {
        binding: CandidateWriterBinding,
    },
    IdentityLineReady {
        terminator: Option<CandidateWriterSourceAtom>,
    },
    RangeReplayReady(CandidateRangeReplayReceipt),
    CompletionReady,
    #[cfg(feature = "exact-parser")]
    LineBoundaryCheckpointReady,
}

/// Result of an optional same-build checkpoint request. An unavailable
/// checkpoint is a scheduling outcome, not a parse failure: the live writer
/// remains untouched and may continue normally.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateLineBoundaryCheckpointAdmission {
    Started,
    Skipped(CandidateLineBoundaryCheckpointSkip),
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateLineBoundaryCheckpointSkip {
    SourceNotQuiescent,
    ProjectionVirtualUnsafe,
    /// A restart-crossing normalization has closed its fresh replacement but
    /// parser lookahead has not yet decided whether the retired logical
    /// identity reopens as a residual. A later line boundary can checkpoint
    /// after that linear authority has been consumed.
    DeferredNormalizationPending,
}

/// Single-use continuation of one actor-owned sparse donor-sample chain.
///
/// The cumulative cut is deliberately private. A driver can only carry this
/// value from one joined checkpoint to the next; it cannot author or revise
/// any of the five coordinate axes.
#[cfg(feature = "exact-parser")]
#[must_use = "the donor sample cursor must enter the next joined checkpoint or be discarded"]
pub(crate) struct DonorCheckpointSampleCursor {
    state: DonorCheckpointSampleCursorState,
}

#[cfg(feature = "exact-parser")]
impl fmt::Debug for DonorCheckpointSampleCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DonorCheckpointSampleCursor")
            .field("epoch", &self.state.epoch)
            .field("sample_ordinal", &self.state.sample_ordinal)
            .finish_non_exhaustive()
    }
}

#[cfg(all(feature = "exact-parser", test))]
impl DonorCheckpointSampleCursor {
    pub(crate) const fn epoch_for_test(&self) -> LiveCandidateEpoch {
        self.state.epoch
    }

    pub(crate) const fn cumulative_cut_for_test(&self) -> RelativeCheckpointMeasure {
        self.state.cumulative_cut
    }

    pub(crate) const fn sample_ordinal_for_test(&self) -> u64 {
        self.state.sample_ordinal
    }
}

/// One actor-derived interval plus the only cursor authorized to extend it.
#[cfg(feature = "exact-parser")]
#[must_use = "the sample must enter the checkpoint index and its cursor must be continued"]
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug)]
pub(crate) struct CapturedDonorCheckpointSample {
    #[cfg(test)]
    interval: RelativeCheckpointMeasure,
    next: DonorCheckpointSampleCursor,
}

/// One live post-restart sample. The actor receives no writable interval or
/// donor bytes: all coordinates come from the joined parser/writer checkpoint,
/// and the opaque identity witness is consumed only by the later convergence
/// join against a parent-selected old checkpoint.
#[cfg(feature = "exact-parser")]
#[must_use = "the parent-selected sample must enter convergence or candidate cancellation"]
#[derive(Debug)]
pub(crate) struct CapturedParentSelectedSuffixSample {
    epoch: LiveCandidateEpoch,
    interval: RelativeCheckpointMeasure,
    cumulative_cut: RelativeCheckpointMeasure,
    sample_ordinal: u64,
    donor_identity: OpaqueDonorIdentityWitness,
    rollback: CandidateParentSelectedSampleRollback,
}

/// Writer-owned metadata half of one speculative convergence sample. It can
/// restore checkpoint sampling state only while the paired advanced cursor
/// and the just-appended draft remain current; green/source/parser roots are
/// deliberately outside this rollback transaction.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct CandidateParentSelectedSampleRollback {
    chain: ParentSelectedSuffixSampleRollback,
    sample_len_before: usize,
    sample_total_before: RelativeCheckpointMeasure,
    maximum_path_depth_before: usize,
}

/// Linear rejection acknowledgement minted only by an opaque donor mismatch.
/// The paused writer consumes it to make that proof probe ephemeral.
#[cfg(feature = "exact-parser")]
#[must_use = "a rejected convergence sample must be rolled back or the candidate cancelled"]
#[derive(Debug)]
pub(crate) struct ParentSelectedRejectedSuffixSample {
    epoch: LiveCandidateEpoch,
    interval: RelativeCheckpointMeasure,
    cumulative_cut: RelativeCheckpointMeasure,
    sample_ordinal: u64,
    rollback: CandidateParentSelectedSampleRollback,
}

#[cfg(all(feature = "exact-parser", test))]
impl CapturedParentSelectedSuffixSample {
    pub(crate) const fn receipt_for_test(
        &self,
    ) -> (
        LiveCandidateEpoch,
        RelativeCheckpointMeasure,
        RelativeCheckpointMeasure,
        u64,
    ) {
        (
            self.epoch,
            self.interval,
            self.cumulative_cut,
            self.sample_ordinal,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl CapturedParentSelectedSuffixSample {
    /// Lexically restricted handoff to the convergence coordinator. The
    /// scheduler never receives the actor-observed coordinates or donor
    /// witness as independent values.
    pub(crate) fn into_convergence_parts(
        self,
        _mint: crate::parent_selected_convergence::ParentSelectedConvergenceSampleMint,
    ) -> (
        LiveCandidateEpoch,
        RelativeCheckpointMeasure,
        RelativeCheckpointMeasure,
        u64,
        OpaqueDonorIdentityWitness,
        CandidateParentSelectedSampleRollback,
    ) {
        (
            self.epoch,
            self.interval,
            self.cumulative_cut,
            self.sample_ordinal,
            self.donor_identity,
            self.rollback,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedRejectedSuffixSample {
    pub(crate) fn from_donor_mismatch(
        epoch: LiveCandidateEpoch,
        interval: RelativeCheckpointMeasure,
        cumulative_cut: RelativeCheckpointMeasure,
        sample_ordinal: u64,
        rollback: CandidateParentSelectedSampleRollback,
    ) -> Self {
        Self {
            epoch,
            interval,
            cumulative_cut,
            sample_ordinal,
            rollback,
        }
    }
}

#[cfg(feature = "exact-parser")]
#[cfg_attr(not(test), allow(dead_code))]
impl CapturedDonorCheckpointSample {
    /// Starts the only production container accepted by the restart-composite
    /// commit path. The sample and continuation are split only inside this
    /// module, after reserving storage, so callers cannot repack an
    /// independently authored list of donor drafts.
    pub(crate) fn try_start_restart_chain(
        self,
    ) -> Result<
        (RestartCheckpointSampleChain, DonorCheckpointSampleCursor),
        Box<RestartCheckpointSampleChainStartFailure>,
    > {
        RestartCheckpointSampleChain::try_start(self)
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (RelativeCheckpointMeasure, DonorCheckpointSampleCursor) {
        (self.interval, self.next)
    }
}

/// Private actor-minted sample chain accepted by the restart-authoritative
/// composite commit. There is deliberately no constructor from raw samples,
/// intervals, cursors, or arena identities.
#[cfg(feature = "exact-parser")]
#[must_use = "the actor-minted sample chain must enter restart-composite commit or be discarded"]
#[derive(Debug)]
pub(crate) struct RestartCheckpointSampleChain {
    final_state: DonorCheckpointSampleCursorState,
}

#[cfg(feature = "exact-parser")]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct RestartCheckpointSampleChainStartFailure {
    pub(crate) error: CandidateWriterError,
    #[allow(dead_code)] // Preserves linear capture authority on allocator/invariant failure.
    pub(crate) capture: CapturedDonorCheckpointSample,
}

#[cfg(feature = "exact-parser")]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct RestartCheckpointSampleChainAppendFailure {
    pub(crate) error: CandidateWriterError,
    #[allow(dead_code)] // Consumed by crossed-chain recovery once actor wiring owns both chains.
    pub(crate) capture: CapturedDonorCheckpointSample,
}

#[cfg(feature = "exact-parser")]
#[cfg_attr(not(test), allow(dead_code))]
impl RestartCheckpointSampleChain {
    fn try_start(
        capture: CapturedDonorCheckpointSample,
    ) -> Result<(Self, DonorCheckpointSampleCursor), Box<RestartCheckpointSampleChainStartFailure>>
    {
        let state = capture.next.state;
        if state.sample_ordinal != 1 {
            return Err(Box::new(RestartCheckpointSampleChainStartFailure {
                error: CandidateWriterError::Invariant(
                    "first donor sample does not bind the actor-owned origin cut",
                ),
                capture,
            }));
        }
        let CapturedDonorCheckpointSample { next, .. } = capture;
        Ok((Self { final_state: state }, next))
    }

    /// Appends exactly the capture minted from the continuation returned by
    /// this chain's prior append. On failure the chain is unchanged and the
    /// complete capture is returned for cancellation/diagnostics.
    pub(crate) fn try_append(
        &mut self,
        capture: CapturedDonorCheckpointSample,
    ) -> Result<DonorCheckpointSampleCursor, Box<RestartCheckpointSampleChainAppendFailure>> {
        let next_state = capture.next.state;
        let Some(expected_ordinal) = self.final_state.sample_ordinal.checked_add(1) else {
            return Err(Box::new(RestartCheckpointSampleChainAppendFailure {
                error: CandidateWriterError::Invariant("restart sample chain ordinal overflow"),
                capture,
            }));
        };
        if next_state.epoch != self.final_state.epoch
            || next_state.sample_ordinal != expected_ordinal
            || next_state
                .cumulative_cut
                .checked_difference_from(self.final_state.cumulative_cut)
                .is_err()
        {
            return Err(Box::new(RestartCheckpointSampleChainAppendFailure {
                error: CandidateWriterError::Invariant(
                    "restart sample chain capture is crossed or noncontiguous",
                ),
                capture,
            }));
        }
        let CapturedDonorCheckpointSample { next, .. } = capture;
        self.final_state = next_state;
        Ok(next)
    }

    fn validate_actor_completion(
        &self,
        epoch: LiveCandidateEpoch,
        accumulator: &DonorCheckpointSampleAccumulator,
    ) -> Result<(), CandidateWriterError> {
        if self.final_state.epoch != epoch
            || accumulator.document_origin_expected() != Some(self.final_state)
            || u64::try_from(accumulator.samples.len()).ok()
                != Some(self.final_state.sample_ordinal)
        {
            return Err(CandidateWriterError::Invariant(
                "restart sample chain and writer accumulator disagree",
            ));
        }
        Ok(())
    }

    fn physical_lines(&self) -> u64 {
        self.final_state.cumulative_cut.physical_lines()
    }

    #[cfg(test)]
    pub(crate) fn forge_next_ordinal_for_test(&mut self) {
        self.final_state.sample_ordinal = self
            .final_state
            .sample_ordinal
            .checked_add(1)
            .expect("test sample ordinal has room");
    }
}

/// A successive capture failure returns the unchanged linear cursor. The
/// joined parser/writer checkpoint was borrowed, so the driver may correct a
/// routing error or retry a fallible donor allocation without losing either
/// authority chain.
#[cfg(feature = "exact-parser")]
pub(crate) struct DonorCheckpointSampleCaptureFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) cursor: DonorCheckpointSampleCursor,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DonorCheckpointSampleCursorState {
    epoch: LiveCandidateEpoch,
    sample_ordinal: u64,
    cumulative_cut: RelativeCheckpointMeasure,
}

#[cfg(feature = "exact-parser")]
impl DonorCheckpointSampleCursorState {
    fn interval_to(
        self,
        current: RelativeCheckpointMeasure,
    ) -> Result<RelativeCheckpointMeasure, CandidateWriterError> {
        let interval = current.checked_difference_from(self.cumulative_cut)?;
        if interval.source_bytes() == 0 || interval.source_utf16() == 0 {
            return Err(CandidateWriterError::Invariant(
                "donor checkpoint cut did not advance",
            ));
        }
        Ok(interval)
    }
}

/// Actor-side mirror of the one external cursor. Keeping the expected state
/// with the candidate rejects a stale, replayed, or crossed cursor even when
/// it happens to name the same candidate epoch.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct DonorCheckpointSampleAccumulator {
    chain: DonorCheckpointSampleChainState,
    samples: Vec<DonorCheckpointSampleDraft>,
    sample_total: RelativeCheckpointMeasure,
    maximum_path_depth: usize,
    normalization_spans: Vec<FinalizedWriterNormalizationSpan>,
    open_paragraph: Option<OpenWriterParagraphSampleGroup>,
}

/// Heap side of candidate cancellation. Arena owners already have their own
/// fuelled journal; this companion prevents a long checkpoint draft chain
/// (and its nested frame vectors) from being destroyed in the edit-ingress
/// turn that supersedes the candidate.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct CandidateWriterHeapRetirement {
    donor: crate::committed_checkpoint_index::DonorCheckpointHeapRetirement,
    normalization_spans: std::vec::IntoIter<FinalizedWriterNormalizationSpan>,
}

#[cfg(feature = "exact-parser")]
impl CandidateWriterHeapRetirement {
    pub(crate) fn empty() -> Self {
        Self {
            donor: crate::committed_checkpoint_index::DonorCheckpointHeapRetirement::empty(),
            normalization_spans: Vec::new().into_iter(),
        }
    }

    pub(crate) fn from_donor(
        donor: crate::committed_checkpoint_index::DonorCheckpointHeapRetirement,
    ) -> Self {
        Self {
            donor,
            normalization_spans: Vec::new().into_iter(),
        }
    }

    pub(crate) fn poll(&mut self, fuel: usize) -> usize {
        let donor = self.donor.poll(fuel);
        let mut transitions = donor;
        while transitions < fuel && self.normalization_spans.next().is_some() {
            transitions += 1;
        }
        transitions
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.donor.is_complete() && self.normalization_spans.len() == 0
    }
}

/// Direct-lane suffix-local samples after one parent-selected restart.
///
/// Construction consumes the writer accumulator only after proving it has no
/// finalized or in-flight Setext normalization. Its cursor and sample vector
/// remain private and can be opened only by the checkpoint suffix module's
/// lexical mint.
#[cfg(feature = "exact-parser")]
#[must_use = "the writer checkpoint tail must enter the parent-selected splice or be aborted"]
#[derive(Debug)]
pub(crate) struct ParentSelectedWriterCheckpointTail {
    epoch: LiveCandidateEpoch,
    cursor: ParentSelectedSuffixSampleCursor,
    samples: Vec<DonorCheckpointSampleDraft>,
    sample_total: RelativeCheckpointMeasure,
    maximum_path_depth: usize,
}

/// Closed state machine for the two distinct checkpoint-chain domains. A
/// retained suffix can never masquerade as a document-origin chain, and the
/// one-shot parent origin cannot coexist with a continuation cursor.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
enum DonorCheckpointSampleChainState {
    DocumentOrigin {
        expected: Option<DonorCheckpointSampleCursorState>,
    },
    UnseededRetainedPrefix,
    ParentSelectedAwaitingFirst(ParentSelectedSuffixSampleOrigin),
    ParentSelectedContinuing(ParentSelectedSuffixSampleCursor),
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct OpenWriterParagraphSampleGroup {
    block: BlockId,
    sample_start: usize,
    final_heading: Option<GreenHeadingOpenFacts>,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct FinalizedWriterNormalizationSpan {
    block: BlockId,
    sample_start: usize,
    sample_end: usize,
    final_heading: GreenHeadingOpenFacts,
}

/// One writer-finalized contiguous checkpoint partition. Its role is derived
/// solely from the live Paragraph normalization transaction; no constructor
/// accepts a group ID, outcome tag, or caller-authored sample list.
#[cfg(feature = "exact-parser")]
#[must_use = "the finalized writer checkpoint partition must enter the committed index"]
pub(crate) struct FinalizedWriterCheckpointPartition {
    role: FinalizedWriterCheckpointPartitionRole,
    samples: Vec<DonorCheckpointSampleDraft>,
}

#[cfg(feature = "exact-parser")]
enum FinalizedWriterCheckpointPartitionRole {
    Direct,
    Setext {
        block: BlockId,
        final_heading: GreenHeadingOpenFacts,
    },
}

#[cfg(feature = "exact-parser")]
impl FinalizedWriterCheckpointPartition {
    fn direct(samples: Vec<DonorCheckpointSampleDraft>) -> Result<Self, CandidateWriterError> {
        if samples.is_empty() {
            return Err(CandidateWriterError::Invariant(
                "writer finalized an empty direct checkpoint partition",
            ));
        }
        Ok(Self {
            role: FinalizedWriterCheckpointPartitionRole::Direct,
            samples,
        })
    }

    fn setext(
        block: BlockId,
        final_heading: GreenHeadingOpenFacts,
        samples: Vec<DonorCheckpointSampleDraft>,
    ) -> Result<Self, CandidateWriterError> {
        if block.0 == 0 || final_heading.style() != GreenHeadingStyle::Setext || samples.is_empty()
        {
            return Err(CandidateWriterError::Invariant(
                "writer finalized an invalid Setext checkpoint partition",
            ));
        }
        Ok(Self {
            role: FinalizedWriterCheckpointPartitionRole::Setext {
                block,
                final_heading,
            },
            samples,
        })
    }

    pub(crate) fn into_checkpoint_index_parts(
        self,
    ) -> (
        Vec<DonorCheckpointSampleDraft>,
        Option<(BlockId, GreenHeadingOpenFacts)>,
    ) {
        let normalization = match self.role {
            FinalizedWriterCheckpointPartitionRole::Direct => None,
            FinalizedWriterCheckpointPartitionRole::Setext {
                block,
                final_heading,
            } => Some((block, final_heading)),
        };
        (self.samples, normalization)
    }
}

#[cfg(feature = "exact-parser")]
impl DonorCheckpointSampleAccumulator {
    const fn from_document_origin() -> Self {
        Self {
            chain: DonorCheckpointSampleChainState::DocumentOrigin { expected: None },
            samples: Vec::new(),
            sample_total: RelativeCheckpointMeasure::new(0, 0, 0, 0, 0),
            maximum_path_depth: 0,
            normalization_spans: Vec::new(),
            open_paragraph: None,
        }
    }

    const fn after_unseeded_retained_prefix() -> Self {
        Self {
            chain: DonorCheckpointSampleChainState::UnseededRetainedPrefix,
            samples: Vec::new(),
            sample_total: RelativeCheckpointMeasure::new(0, 0, 0, 0, 0),
            maximum_path_depth: 0,
            normalization_spans: Vec::new(),
            open_paragraph: None,
        }
    }

    fn after_parent_selected_prefix(origin: ParentSelectedSuffixSampleOrigin) -> Self {
        Self {
            chain: DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(origin),
            samples: Vec::new(),
            sample_total: RelativeCheckpointMeasure::default(),
            maximum_path_depth: 0,
            normalization_spans: Vec::new(),
            open_paragraph: None,
        }
    }

    fn validate_cursor(
        &self,
        actor_epoch: LiveCandidateEpoch,
        supplied: DonorCheckpointSampleCursorState,
    ) -> Result<(), CandidateWriterError> {
        if supplied.epoch != actor_epoch {
            return Err(CandidateWriterError::WrongCandidate);
        }
        if !matches!(
            &self.chain,
            DonorCheckpointSampleChainState::DocumentOrigin {
                expected: Some(expected)
            } if *expected == supplied
        ) {
            return Err(CandidateWriterError::Invariant(
                "crossed donor checkpoint sample cursor",
            ));
        }
        Ok(())
    }

    fn document_origin_expected(&self) -> Option<DonorCheckpointSampleCursorState> {
        match &self.chain {
            DonorCheckpointSampleChainState::DocumentOrigin { expected } => *expected,
            DonorCheckpointSampleChainState::UnseededRetainedPrefix
            | DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(_)
            | DonorCheckpointSampleChainState::ParentSelectedContinuing(_) => None,
        }
    }

    fn parent_selected_direct_tail_is_eligible(&self, epoch: LiveCandidateEpoch) -> bool {
        if !self.normalization_spans.is_empty()
            || self
                .open_paragraph
                .as_ref()
                .is_some_and(|group| group.final_heading.is_some())
        {
            return false;
        }
        matches!(
            &self.chain,
            DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor)
                if cursor.epoch() == epoch
                    && cursor.sample_ordinal() != 0
                    && usize::try_from(cursor.sample_ordinal()).ok() == Some(self.samples.len())
        )
    }

    fn push_sample(
        &mut self,
        sample: DonorCheckpointSampleDraft,
    ) -> Result<(), CandidateWriterError> {
        let sample_total = self.sample_total.checked_add(sample.interval())?;
        let maximum_path_depth = self.maximum_path_depth.max(sample.path_depth());
        self.samples.try_reserve(1).map_err(|_| {
            CandidateWriterError::CheckpointIndex(CommittedCheckpointIndexError::Allocation(
                "writer-owned restart sample chain",
            ))
        })?;
        self.samples.push(sample);
        self.sample_total = sample_total;
        self.maximum_path_depth = maximum_path_depth;
        Ok(())
    }

    fn into_heap_retirement(self) -> CandidateWriterHeapRetirement {
        CandidateWriterHeapRetirement {
            donor: crate::committed_checkpoint_index::DonorCheckpointHeapRetirement::from_samples(
                self.samples,
            ),
            normalization_spans: self.normalization_spans.into_iter(),
        }
    }

    fn begin_paragraph_group(&mut self, block: BlockId) -> Result<(), CandidateWriterError> {
        if self.open_paragraph.is_some() || block.0 == 0 {
            return Err(CandidateWriterError::Invariant(
                "writer checkpoint Paragraph groups cannot overlap",
            ));
        }
        self.open_paragraph = Some(OpenWriterParagraphSampleGroup {
            block,
            sample_start: self.samples.len(),
            final_heading: None,
        });
        Ok(())
    }

    fn promote_paragraph_group(
        &mut self,
        block: BlockId,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        let group = self
            .open_paragraph
            .as_mut()
            .ok_or(CandidateWriterError::Invariant(
                "Setext checkpoint normalization has no open Paragraph sample group",
            ))?;
        if group.block != block
            || group.final_heading.is_some()
            || facts.style() != GreenHeadingStyle::Setext
        {
            return Err(CandidateWriterError::Invariant(
                "Setext checkpoint normalization crossed its Paragraph sample group",
            ));
        }
        group.final_heading = Some(facts);
        Ok(())
    }

    fn reidentify_promoted_paragraph_group(
        &mut self,
        retired_block: BlockId,
        replacement_block: BlockId,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        let group = self
            .open_paragraph
            .as_mut()
            .ok_or(CandidateWriterError::Invariant(
                "normalization reidentity has no open Paragraph sample group",
            ))?;
        if retired_block.0 == 0
            || replacement_block.0 == 0
            || retired_block == replacement_block
            || group.block != retired_block
            || group.final_heading.is_some()
            || facts.style() != GreenHeadingStyle::Setext
        {
            return Err(CandidateWriterError::Invariant(
                "normalization reidentity crossed its Paragraph sample group",
            ));
        }
        group.block = replacement_block;
        group.final_heading = Some(facts);
        Ok(())
    }

    /// Retires the unresolved Paragraph group when a construct-neutral
    /// fragment replacement installs a nonterminal root. This first table
    /// slice deliberately admits no checkpoint sample inside that unresolved
    /// Paragraph; the next sample therefore starts a normal Direct frontier
    /// from the completed current-green path.
    fn retire_empty_paragraph_group(&mut self, block: BlockId) -> Result<(), CandidateWriterError> {
        let group = self
            .open_paragraph
            .take()
            .ok_or(CandidateWriterError::Invariant(
                "fragment replacement has no open Paragraph sample group",
            ))?;
        if group.block != block
            || group.final_heading.is_some()
            || group.sample_start != self.samples.len()
        {
            return Err(CandidateWriterError::Invariant(
                "fragment replacement crossed a sampled Paragraph group",
            ));
        }
        Ok(())
    }

    fn finish_paragraph_group(&mut self, block: BlockId) -> Result<(), CandidateWriterError> {
        let group = self
            .open_paragraph
            .take()
            .ok_or(CandidateWriterError::Invariant(
                "Paragraph close has no writer-owned checkpoint sample group",
            ))?;
        if group.block != block {
            return Err(CandidateWriterError::Invariant(
                "Paragraph close crossed its writer-owned checkpoint sample group",
            ));
        }
        if let Some(final_heading) = group.final_heading
            && group.sample_start < self.samples.len()
        {
            self.normalization_spans.try_reserve(1).map_err(|_| {
                CandidateWriterError::CheckpointIndex(CommittedCheckpointIndexError::Allocation(
                    "writer-owned normalization spans",
                ))
            })?;
            self.normalization_spans
                .push(FinalizedWriterNormalizationSpan {
                    block,
                    sample_start: group.sample_start,
                    sample_end: self.samples.len(),
                    final_heading,
                });
        }
        Ok(())
    }

    fn reidentify_finalized_whole_normalization(
        &mut self,
        identity: &ResolvedWholeNormalizationIdentity,
    ) -> Result<(), CandidateWriterError> {
        if identity.kind() != GreenKind::HEADING
            || identity.retired_block().0 == 0
            || identity.replacement_block().0 == 0
            || identity.retired_block() == identity.replacement_block()
        {
            return Err(CandidateWriterError::Invariant(
                "whole normalization supplied an invalid checkpoint identity",
            ));
        }
        if let Some(span) = self.normalization_spans.last_mut()
            && span.block == identity.replacement_block()
        {
            span.block = identity.retired_block();
        }
        Ok(())
    }

    fn into_parent_selected_direct_tail(
        self,
        epoch: LiveCandidateEpoch,
    ) -> Result<ParentSelectedWriterCheckpointTail, CandidateWriterError> {
        if !self.parent_selected_direct_tail_is_eligible(epoch) {
            return Err(CandidateWriterError::TailSpliceIneligible);
        }
        let cursor = match self.chain {
            DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor) => cursor,
            DonorCheckpointSampleChainState::DocumentOrigin { .. }
            | DonorCheckpointSampleChainState::UnseededRetainedPrefix
            | DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(_) => {
                return Err(CandidateWriterError::Invariant(
                    "parent-selected splice lacks a continued suffix sample chain",
                ));
            }
        };
        if cursor.epoch() != epoch
            || cursor.sample_ordinal() == 0
            || usize::try_from(cursor.sample_ordinal()).ok() != Some(self.samples.len())
        {
            return Err(CandidateWriterError::Invariant(
                "parent-selected writer samples disagree with their cursor",
            ));
        }
        Ok(ParentSelectedWriterCheckpointTail {
            epoch,
            cursor,
            samples: self.samples,
            sample_total: self.sample_total,
            maximum_path_depth: self.maximum_path_depth,
        })
    }

    fn into_checkpoint_index_builder(
        self,
        final_measure: RelativeCheckpointMeasure,
    ) -> Result<StorageOnlyCheckpointIndexBuilder, CandidateWriterError> {
        if self.open_paragraph.is_some() {
            return Err(CandidateWriterError::Invariant(
                "completed writer retains an unfinished Paragraph sample group",
            ));
        }
        let final_state = match self.chain {
            DonorCheckpointSampleChainState::DocumentOrigin {
                expected: Some(final_state),
            } => final_state,
            DonorCheckpointSampleChainState::DocumentOrigin { expected: None }
            | DonorCheckpointSampleChainState::UnseededRetainedPrefix
            | DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(_)
            | DonorCheckpointSampleChainState::ParentSelectedContinuing(_) => {
                return Err(CandidateWriterError::Invariant(
                    "completed writer has no document-origin donor checkpoint sample",
                ));
            }
        };
        let tail = final_measure.checked_difference_from(final_state.cumulative_cut)?;
        if tail.source_bytes() != 0 || tail.source_utf16() != 0 || tail.physical_lines() != 0 {
            return Err(CandidateWriterError::Invariant(
                "final donor sample is not at source EOF",
            ));
        }

        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        let mut samples = self.samples.into_iter();
        let mut consumed = 0_usize;
        for span in self.normalization_spans {
            if span.sample_start < consumed || span.sample_end < span.sample_start {
                return Err(CandidateWriterError::Invariant(
                    "writer-owned normalization sample spans overlap",
                ));
            }
            let direct_count = span.sample_start - consumed;
            if direct_count != 0 {
                let direct: Vec<_> = samples.by_ref().take(direct_count).collect();
                if direct.len() != direct_count {
                    return Err(CandidateWriterError::Invariant(
                        "writer-owned direct sample span is truncated",
                    ));
                }
                builder.push(
                    StorageOnlyCheckpointPartition::from_finalized_writer_partition(
                        FinalizedWriterCheckpointPartition::direct(direct)?,
                    )?,
                )?;
            }
            let normalization_count = span.sample_end - span.sample_start;
            let normalization: Vec<_> = samples.by_ref().take(normalization_count).collect();
            if normalization.len() != normalization_count {
                return Err(CandidateWriterError::Invariant(
                    "writer-owned normalization sample span is truncated",
                ));
            }
            builder.push(
                StorageOnlyCheckpointPartition::from_finalized_writer_partition(
                    FinalizedWriterCheckpointPartition::setext(
                        span.block,
                        span.final_heading,
                        normalization,
                    )?,
                )?,
            )?;
            consumed = span.sample_end;
        }
        let direct: Vec<_> = samples.collect();
        if !direct.is_empty() {
            builder.push(
                StorageOnlyCheckpointPartition::from_finalized_writer_partition(
                    FinalizedWriterCheckpointPartition::direct(direct)?,
                )?,
            )?;
        }
        if tail != RelativeCheckpointMeasure::default() {
            builder.push(StorageOnlyCheckpointPartition::terminal_tail(tail))?;
        }
        Ok(builder)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedWriterCheckpointTail {
    pub(crate) fn into_checkpoint_splice_parts(
        self,
        _mint: crate::committed_checkpoint_index::suffix_splice::ParentSelectedCheckpointSpliceMint,
    ) -> (
        LiveCandidateEpoch,
        ParentSelectedSuffixSampleCursor,
        Vec<DonorCheckpointSampleDraft>,
        RelativeCheckpointMeasure,
        usize,
    ) {
        (
            self.epoch,
            self.cursor,
            self.samples,
            self.sample_total,
            self.maximum_path_depth,
        )
    }
}

#[cfg(feature = "exact-parser")]
struct ValidatedJoinedDonorCut {
    cut: RelativeCheckpointMeasure,
    donor: flark_comrak_value_block_core::DirectDurableGrammarCapture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateFencedCodeBoundary {
    InfoEnd,
    LiteralStart,
}

#[derive(Debug)]
struct ActiveFencedCodeProjectionFold {
    build: ArenaBuildId,
    block: crate::BlockId,
    info_end: Option<SourceLedgerMetric>,
    literal_start: Option<SourceLedgerMetric>,
}

/// The one provisional Paragraph whose wrapper may still be normalized by
/// the parser. Content remains in the ordinary source-ordered green stream;
/// this group retains only storage's linear Enter capability.
#[derive(Debug)]
struct ActiveParagraphNormalizationGroup {
    build: ArenaBuildId,
    block: crate::BlockId,
    enter: Option<ProvisionalParagraphEnter>,
    projection_origin: Option<CanonicalFragmentProjectionOrigin>,
    promoted_setext: bool,
    deferred_identity: Option<DeferredNormalizationIdentity>,
    deferred_storage: Option<SetextPromotion>,
}

#[derive(Debug)]
struct PendingDeferredNormalization {
    identity: DeferredNormalizationIdentity,
    storage: SetextPromotion,
}

#[derive(Debug)]
struct PendingWholeNormalization {
    identity: ResolvedWholeNormalizationIdentity,
    storage: SetextPromotion,
}

const fn source_metric_precedes(earlier: SourceLedgerMetric, later: SourceLedgerMetric) -> bool {
    earlier.bytes() <= later.bytes() && earlier.utf16() <= later.utf16()
}

const fn fragment_metric_precedes(earlier: SerializedMetric, later: SerializedMetric) -> bool {
    earlier.bytes <= later.bytes && earlier.utf16 <= later.utf16
}

fn fragment_metric_difference(
    later: SerializedMetric,
    earlier: SerializedMetric,
) -> Result<SerializedMetric, CandidateWriterError> {
    Ok(SerializedMetric {
        bytes: later
            .bytes
            .checked_sub(earlier.bytes)
            .ok_or(CandidateWriterError::Invariant(
                "table fragment byte partition is reversed",
            ))?,
        utf16: later
            .utf16
            .checked_sub(earlier.utf16)
            .ok_or(CandidateWriterError::Invariant(
                "table fragment UTF-16 partition is reversed",
            ))?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateWriterError {
    Actor(LiveDocumentError),
    WrongCandidate,
    Busy,
    NoAction,
    SourceAtomOutstanding,
    NoSourceAtomOutstanding,
    ReplayedSourceAtom,
    NonBlankGapAtom,
    IdentityReplayRequiresTypedRecipe(CandidateSourceAtomKind),
    SourceTransitionExhausted,
    CompletionAlreadyReady,
    TailAdoptionReady,
    TailSpliceIneligible,
    WriterPoisoned,
    IdentityExhausted(EntityIdentityKind),
    SourceLedger(SourceBoundLedgerError),
    Projection(SourceProjectionComposerError),
    Green(SerializedGreenError),
    #[cfg(feature = "exact-parser")]
    ReferenceProjection(
        crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionError,
    ),
    ArenaBuild(ArenaBuildError),
    #[cfg(feature = "exact-parser")]
    CheckpointIndex(CommittedCheckpointIndexError),
    #[cfg(feature = "exact-parser")]
    RestartComposite(RestartCompositeDocumentError),
    #[cfg(feature = "exact-parser")]
    GreenReferenceComposite(GreenReferenceCompositeError),
    InjectedAfterGreenAcknowledgement,
    Invariant(&'static str),
}

impl fmt::Display for CandidateWriterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "candidate writer error: {self:?}")
    }
}

impl std::error::Error for CandidateWriterError {}

impl From<SourceBoundLedgerError> for CandidateWriterError {
    fn from(error: SourceBoundLedgerError) -> Self {
        Self::SourceLedger(error)
    }
}

impl From<SourceProjectionComposerError> for CandidateWriterError {
    fn from(error: SourceProjectionComposerError) -> Self {
        Self::Projection(error)
    }
}

impl From<SerializedGreenError> for CandidateWriterError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionError>
    for CandidateWriterError
{
    fn from(
        error: crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionError,
    ) -> Self {
        Self::ReferenceProjection(error)
    }
}

impl From<ArenaBuildError> for CandidateWriterError {
    fn from(error: ArenaBuildError) -> Self {
        Self::ArenaBuild(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<CommittedCheckpointIndexError> for CandidateWriterError {
    fn from(error: CommittedCheckpointIndexError) -> Self {
        Self::CheckpointIndex(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<RestartCompositeDocumentError> for CandidateWriterError {
    fn from(error: RestartCompositeDocumentError) -> Self {
        Self::RestartComposite(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<GreenReferenceCompositeError> for CandidateWriterError {
    fn from(error: GreenReferenceCompositeError) -> Self {
        Self::GreenReferenceComposite(error)
    }
}

#[derive(Debug)]
enum CandidateWriterBuildLease {
    Suspended(ArenaBuildTicket),
    Aborting(ArenaBuildId),
    Joined,
}

impl CandidateWriterBuildLease {
    fn build_id(&self) -> Result<ArenaBuildId, CandidateWriterError> {
        match self {
            Self::Suspended(ticket) => Ok(ticket.id()),
            Self::Aborting(build) => Ok(*build),
            Self::Joined => Err(CandidateWriterError::CompletionAlreadyReady),
        }
    }
}

#[derive(Debug)]
enum DrainState {
    NeedRunSeal,
    SealedRun(ComposerSealedProjectionRunCapability),
    AwaitGreenAcknowledgement,
    NeedComposerPoll,
    Complete,
}

#[derive(Debug)]
struct ComposerDrain {
    state: DrainState,
    document_finish: bool,
}

impl ComposerDrain {
    fn begin(
        progress: SourceProjectionComposerProgress,
        document_finish: bool,
    ) -> Result<Self, CandidateWriterError> {
        let state = match progress {
            SourceProjectionComposerProgress::RunReady => DrainState::NeedRunSeal,
            SourceProjectionComposerProgress::Idle if !document_finish => DrainState::Complete,
            SourceProjectionComposerProgress::Complete(_) if document_finish => {
                DrainState::Complete
            }
            SourceProjectionComposerProgress::Idle
            | SourceProjectionComposerProgress::Complete(_) => {
                return Err(CandidateWriterError::Invariant(
                    "composer returned the wrong terminal progress for this drain",
                ));
            }
        };
        Ok(Self {
            state,
            document_finish,
        })
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, DrainState::Complete)
    }
}

#[derive(Debug)]
// The large arm is the one bounded, inline sealed projection run. Boxing it
// would add a per-structural-action heap allocation to the live typing path.
#[allow(clippy::large_enum_variant)]
enum OpenPhase {
    BeginWholeNormalization,
    AwaitWholeNormalization,
    RequestStructuralFlush,
    Drain(ComposerDrain),
    OfferEnter,
    AwaitEnterAcknowledgement,
    OpenLedger,
}

#[derive(Debug)]
struct OpenJob {
    kind: GreenKind,
    facts: Option<FactsEnvelope>,
    permit: Option<FreshBlockPermit>,
    deferred_residual: Option<PendingDeferredNormalization>,
    whole_normalization: Option<PendingWholeNormalization>,
    phase: OpenPhase,
}

#[derive(Debug)]
struct ConsumeJob {
    drain: ComposerDrain,
}

#[derive(Debug)]
// As with the structural writer phases, this is one bounded inline run state.
// Boxing it would add heap churn to every ordinary physical line.
#[allow(clippy::large_enum_variant)]
enum RangeReplayResume {
    Scan,
    Ready,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum RangeReplayPhase {
    Scan,
    Drain {
        drain: ComposerDrain,
        resume: RangeReplayResume,
    },
    Ready,
}

/// One exact parser-command range. At most one decoder-minted boundary or one
/// typed atom is retained while the authoritative cursor advances; source
/// scanning never crosses the plan endpoint.
#[derive(Debug)]
struct RangeReplayJob {
    plan: CandidateRangeReplayPlan,
    legacy_identity_completion: bool,
    scan_high_water: u64,
    last_boundary: Option<crate::CandidateSourceBoundary>,
    pending_atom: Option<CandidateSourceAtom>,
    completion: Option<CandidateRangeReplaySourceReceipt>,
    writer_polls: u64,
    source_work_units: u64,
    source_bytes_read: u64,
    atoms_scanned: u64,
    source_pieces: u64,
    maximum_pending_atoms: usize,
    maximum_pending_boundaries: usize,
    phase: RangeReplayPhase,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum SetextPhase {
    RequestStructuralFlush,
    Drain(ComposerDrain),
    BeginGreenPromotion,
    AwaitGreenPromotion,
    RetypeLedger,
}

/// One exhaustive normalization of the active Paragraph wrapper. The old
/// parser-visible binding and storage token are consumed together; neither can
/// be replayed after the writer starts this action.
#[derive(Debug)]
struct SetextJob {
    binding: Option<CandidateWriterBinding>,
    enter: Option<ProvisionalParagraphEnter>,
    projection_origin: Option<CanonicalFragmentProjectionOrigin>,
    replacement_permit: Option<FreshBlockPermit>,
    defer_identity: bool,
    facts: GreenHeadingOpenFacts,
    storage: Option<SetextPromotion>,
    phase: SetextPhase,
}

#[derive(Debug)]
struct ActiveTableHeaderCell {
    block: BlockId,
    source_end: SerializedMetric,
}

#[derive(Debug)]
enum TableHeaderBatchCompletion {
    AwaitInput,
    InstallCell {
        block: BlockId,
        source_start: SerializedMetric,
        source_end: SerializedMetric,
    },
    AdvanceCursor(SerializedMetric),
    CloseCell,
    FinishFragment,
}

/// Fixed-size event batch for one grammar action.  Three is the exhaustive
/// maximum (`trailing marker`, `terminal`, `HeaderRow Exit`) and does not scale
/// with columns, source length, or document size.
#[derive(Debug)]
struct TableHeaderEventBatch {
    events: [Option<GreenEvent>; 3],
    next: usize,
    awaiting_ack: bool,
    completion: TableHeaderBatchCompletion,
}

impl TableHeaderEventBatch {
    fn new(
        first: Option<GreenEvent>,
        second: Option<GreenEvent>,
        third: Option<GreenEvent>,
        completion: TableHeaderBatchCompletion,
    ) -> Self {
        Self {
            events: [first, second, third],
            next: 0,
            awaiting_ack: false,
            completion,
        }
    }

    fn next_event(&mut self) -> Option<GreenEvent> {
        while self.next < self.events.len() {
            let index = self.next;
            self.next += 1;
            if let Some(event) = self.events[index].take() {
                return Some(event);
            }
        }
        None
    }
}

#[derive(Debug)]
enum TableHeaderPhase {
    RequestStructuralFlush,
    Drain(ComposerDrain),
    BeginComposerFragment,
    BeginGreenFragment,
    AwaitFragmentStart,
    AwaitFragmentInput,
    Emit(TableHeaderEventBatch),
    BeginFragmentFinish,
    AwaitFragmentCommit,
    RebaseComposer,
    RebindLedger,
}

/// Candidate-owned table normalization.  It retains only constant state and
/// one fixed event batch; each cell and each projection run is supplied and
/// acknowledged independently.
#[derive(Debug)]
struct TableHeaderJob {
    paragraph: Option<CandidateWriterBinding>,
    enter: Option<ProvisionalParagraphEnter>,
    table_permit: Option<FreshBlockPermit>,
    table_facts: GreenTableOpenFacts,
    header_block: BlockId,
    expected_physical: SerializedMetric,
    cursor: SerializedMetric,
    next_column: u32,
    active_cell: Option<ActiveTableHeaderCell>,
    storage: Option<CanonicalFragmentReplacement>,
    projection_origin: Option<CanonicalFragmentProjectionOrigin>,
    projection: Option<CanonicalFragmentProjectionRebase>,
    phase: TableHeaderPhase,
}

#[derive(Debug)]
// See `OpenPhase`: the size is codec-bounded and reused, not document-sized.
#[allow(clippy::large_enum_variant)]
enum ClosePhase {
    BeginWholeNormalization,
    AwaitWholeNormalization,
    RequestStructuralFlush,
    Drain(ComposerDrain),
    OfferExit,
    AwaitExitAcknowledgement,
    CloseLedger,
}

#[derive(Debug)]
struct CloseJob {
    binding: CandidateWriterBinding,
    closed: ClosedChildAggregate,
    last_line_blank: bool,
    facts: GreenCloseFacts,
    closes_active_paragraph: bool,
    whole_normalization: Option<PendingWholeNormalization>,
    phase: ClosePhase,
}

#[derive(Debug)]
// See `OpenPhase`: preserving inline bounded authority avoids action churn.
#[allow(clippy::large_enum_variant)]
enum FinishPhase {
    BeginWholeNormalization,
    AwaitWholeNormalization,
    Drain(ComposerDrain),
    JoinComposerCompletion,
    #[cfg(feature = "exact-parser")]
    BeginReferenceSemanticFinish,
    #[cfg(feature = "exact-parser")]
    PollReferenceInternerFinish,
    #[cfg(feature = "exact-parser")]
    PollReferenceIndexFinish,
    FinishGreenInput,
    AwaitManifest,
    JoinCompletion,
}

#[derive(Debug)]
struct FinishJob {
    composer: Option<SourceProjectionComposerCompletionSeal>,
    #[cfg(feature = "exact-parser")]
    reference: Option<crate::reference_restart_index::ReferenceCandidateIndexManifest>,
    whole_normalization: Option<PendingWholeNormalization>,
    phase: FinishPhase,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
enum LineBoundaryCheckpointPhase {
    RequestDedicatedDrain,
    Drain(ComposerDrain),
    PrepareGreenCut,
    AwaitGreenBarrier,
    Ready(Option<SourceProjectionComposerLineBoundaryContinuation>),
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct LineBoundaryCheckpointJob {
    phase: LineBoundaryCheckpointPhase,
    capture_green_prefix_snapshot: bool,
    green_prefix_snapshot: Option<BuilderGreenPrefixSnapshot>,
}

#[derive(Debug)]
enum WriterAction {
    Open(OpenJob),
    Consume(ConsumeJob),
    RangeReplay(RangeReplayJob),
    PromoteSetext(SetextJob),
    PromoteTableHeader(TableHeaderJob),
    #[cfg(feature = "exact-parser")]
    ReferencePrefix(ReferencePrefixJob),
    Close(CloseJob),
    Finish(FinishJob),
    #[cfg(feature = "exact-parser")]
    LineBoundaryCheckpoint(LineBoundaryCheckpointJob),
}

/// Selects the only legal completion route for this writer. A parent-selected
/// restart cannot enter either independent commit path because its retained
/// checkpoint-index prefix and both parent child owners still live in the
/// actor-owned adoption driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateWriterCompletionRoute {
    Independent,
    ParentSelectedAdoption,
}

impl CandidateWriterCompletionRoute {
    fn require_restart_composite_commit(self) -> Result<(), CandidateWriterError> {
        match self {
            Self::Independent => Ok(()),
            Self::ParentSelectedAdoption => Err(CandidateWriterError::Invariant(
                "parent-selected writer requires retained checkpoint-index splice and adoption",
            )),
        }
    }

    fn require_local_commit(self) -> Result<(), CandidateWriterError> {
        match self {
            Self::Independent => Ok(()),
            Self::ParentSelectedAdoption => Err(CandidateWriterError::Invariant(
                "parent-selected writer cannot enter independent green commit",
            )),
        }
    }
}

/// Joined completion is intentionally private and non-cloneable. There is no
/// accessor for its raw manifest or ticket; its sole success transition is
/// `CandidateWriter::commit_local`, which consumes all three proof domains.
#[derive(Debug)]
// Green-only local commit is executable in the composition tests, but is not
// production-wired until the composite checkpoint/reference/inline manifest
// exists. Keep the proof code compiled without pretending it is publishable.
#[cfg_attr(not(test), allow(dead_code))]
struct CandidateWriterCompletionSeal {
    epoch: LiveCandidateEpoch,
    composer: SourceProjectionComposerCompletionSeal,
    green: SerializedGreenBuildManifest,
    #[cfg(feature = "exact-parser")]
    reference: crate::reference_restart_index::ReferenceCandidateIndexManifest,
    ticket: ArenaBuildTicket,
    green_runs_acknowledged: u64,
}

/// Single-use child bundle minted only after the candidate completion proof
/// and actor-owned sample chain have jointly produced the two roots. The v2
/// parent builder accepts this seal, never two independently supplied
/// manifests.
#[cfg(feature = "exact-parser")]
#[must_use = "restart-composite children must be adopted by their parent or aborted"]
#[derive(Debug)]
pub(crate) struct RestartCompositeChildren {
    green: SerializedGreenBuildManifest,
    checkpoint_index: StorageOnlyCheckpointIndexBuildManifest,
}

#[cfg(feature = "exact-parser")]
impl RestartCompositeChildren {
    fn mint_from_completed_candidate(
        green: SerializedGreenBuildManifest,
        checkpoint_index: StorageOnlyCheckpointIndexBuildManifest,
    ) -> Self {
        Self {
            green,
            checkpoint_index,
        }
    }

    /// Parent-internal transfer. Possessing this value already proves the
    /// private candidate-writer mint; the returned manifests never cross a
    /// public or separately ownable API.
    pub(crate) fn into_parent_parts(
        self,
    ) -> (
        SerializedGreenBuildManifest,
        StorageOnlyCheckpointIndexBuildManifest,
    ) {
        (self.green, self.checkpoint_index)
    }

    /// Parent-internal validation view. The manifests remain joined inside
    /// this non-cloneable bundle until the parent allocation succeeds, so a
    /// preflight or allocation failure can return the complete candidate
    /// without reconstructing linear owner authority.
    pub(crate) const fn parent_parts(
        &self,
    ) -> (
        &SerializedGreenBuildManifest,
        &StorageOnlyCheckpointIndexBuildManifest,
    ) {
        (&self.green, &self.checkpoint_index)
    }

    #[cfg(test)]
    pub(crate) fn from_independent_test_children(
        green: SerializedGreenBuildManifest,
        checkpoint_index: StorageOnlyCheckpointIndexBuildManifest,
    ) -> Self {
        Self {
            green,
            checkpoint_index,
        }
    }
}

/// Final parent-selected child replacement. Production code deliberately has
/// no raw constructor: the future green/checkpoint adoption rendezvous in this
/// module must consume the exact retained-parent tail and the two completed
/// child jobs before it can mint this value. The storage module can validate
/// and atomically adopt it, but cannot pair unrelated roots itself.
#[cfg(feature = "exact-parser")]
#[must_use = "the parent-selected replacement must be joined or its build aborted"]
#[derive(Debug)]
pub(crate) struct ParentSelectedRestartCompositeReplacement {
    adoption: ParentSelectedRestartCompositeAdoptionLease,
    children: RestartCompositeChildren,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedRestartCompositeReplacement {
    pub(crate) const fn parent_parts(
        &self,
    ) -> (
        &ParentSelectedRestartCompositeAdoptionLease,
        &RestartCompositeChildren,
    ) {
        (&self.adoption, &self.children)
    }

    pub(crate) fn into_parent_parts(
        self,
    ) -> (
        ParentSelectedRestartCompositeAdoptionLease,
        RestartCompositeChildren,
    ) {
        (self.adoption, self.children)
    }

    /// Test-only stand-in for the not-yet-wired actor rendezvous. Keeping this
    /// constructor out of production makes the type boundary itself record
    /// the remaining integration gate instead of silently accepting arbitrary
    /// old/new child pairings.
    #[cfg(test)]
    pub(crate) fn from_independent_test_parts(
        adoption: ParentSelectedRestartCompositeAdoptionLease,
        children: RestartCompositeChildren,
    ) -> Self {
        Self { adoption, children }
    }
}

/// Grammar-free local commit result. This is not a publishable production
/// manifest: checkpoint, adoption, reference, fact, Unknown-range, and inline
/// roots remain explicit architecture-selection HOLDs.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct CandidateWriterBuiltDocument {
    #[cfg(feature = "exact-parser")]
    composite: GreenReferenceCompositeDocument,
    #[cfg(not(feature = "exact-parser"))]
    green: SerializedGreenDocument,
    composer: SourceProjectionComposerCompletionSeal,
    green_receipt: SerializedGreenBuildReceipt,
    green_runs_acknowledged: u64,
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum CandidateWriterAbortLease {
    Suspended(ArenaBuildTicket),
    AlreadyAborting(ArenaBuildId),
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct CandidateWriterLocalCommitFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) abort: CandidateWriterAbortLease,
    pub(crate) identities: DocumentIdentityAllocator,
}

/// The complete same-build writer half of one line-boundary checkpoint.
///
/// It owns no source bytes or Crop cursors. The source ledger, projection
/// composer, packed-green builder, suspended arena ticket, identity allocator,
/// and provisional normalization state move together and can be resumed only
/// under the exact same candidate epoch. The parser pause is deliberately not
/// stored here; `same_build_checkpoint` joins it only after cross-checking this
/// continuation.
#[cfg(feature = "exact-parser")]
#[must_use = "the writer continuation must be joined, resumed, or cancelled"]
#[derive(Debug)]
pub(crate) struct CandidateWriterLineBoundaryContinuation {
    epoch: LiveCandidateEpoch,
    source: CandidateSourceLineBoundaryContinuation,
    composer: SourceProjectionComposerLineBoundaryContinuation,
    builder: ResumableSerializedGreenBuild,
    ticket: Option<ArenaBuildTicket>,
    identities: DocumentIdentityAllocator,
    next_source_transition: u64,
    green_runs_acknowledged: u64,
    donor_checkpoint_samples: DonorCheckpointSampleAccumulator,
    reference_semantic: Option<CandidateReferenceSemanticTransaction>,
    green_prefix_snapshot: Option<BuilderGreenPrefixSnapshot>,
    active_paragraph: Option<ActiveParagraphNormalizationGroup>,
    active_fenced_code: Option<ActiveFencedCodeProjectionFold>,
    completion_route: CandidateWriterCompletionRoute,
    #[cfg(test)]
    fail_after_green_ack_before_ledger_close: bool,
    #[cfg(test)]
    fail_after_setext_green_ack_before_ledger_retype: bool,
}

/// Candidate authority after the real source ledger and projection composer
/// have stopped at a proven unchanged tail.
///
/// This is not a completed green document and cannot enter the ordinary commit
/// path. It keeps the suspended build, current prefix builder, old packed-tail
/// capability, identity allocator, and provisional normalization state in one
/// linear object for the future green/checkpoint journal splice. Cancellation
/// remains available without retaining a Crop cursor or source root.
#[must_use = "the tail-ready writer must enter the green/index splice or be cancelled"]
#[derive(Debug)]
#[cfg(feature = "exact-parser")]
pub(crate) struct CandidateWriterTailAdoptionReady {
    epoch: LiveCandidateEpoch,
    composer: SourceProjectionComposerTailAdoptionSeal,
    builder: ResumableSerializedGreenBuild,
    ticket: Option<ArenaBuildTicket>,
    identities: DocumentIdentityAllocator,
    next_source_transition: u64,
    green_runs_acknowledged: u64,
    donor_checkpoint_samples: DonorCheckpointSampleAccumulator,
    green_prefix_snapshot: Option<BuilderGreenPrefixSnapshot>,
    active_paragraph: Option<ActiveParagraphNormalizationGroup>,
    active_fenced_code: Option<ActiveFencedCodeProjectionFold>,
    completion_route: CandidateWriterCompletionRoute,
    #[cfg(test)]
    fail_after_green_ack_before_ledger_close: bool,
    #[cfg(test)]
    fail_after_setext_green_ack_before_ledger_retype: bool,
}

/// Linear writer half of the parent-selected adoption rendezvous.
///
/// It keeps the exact current source/composer seal, green-prefix builder and
/// snapshot, old-green tail capability, sparse checkpoint chain, suspended
/// arena ticket, and identity allocator together. Only the dedicated child
/// splice coordinator may open these private fields.
#[cfg(feature = "exact-parser")]
#[must_use = "the parent-selected writer splice bundle must be joined or aborted"]
#[derive(Debug)]
pub(crate) struct ParentSelectedCandidateTailSpliceBundle {
    epoch: LiveCandidateEpoch,
    source: crate::CandidateAdoptedSourceSeal,
    storage: SourceProjectionLineBoundaryStorageAck,
    old_green_tail: crate::GreenSourceTailAdoptionCapability,
    builder: ResumableSerializedGreenBuild,
    green_prefix_snapshot: BuilderGreenPrefixSnapshot,
    ticket: ArenaBuildTicket,
    identities: DocumentIdentityAllocator,
    checkpoints: ParentSelectedWriterCheckpointTail,
    receipt: CandidateWriterTailAdoptionReceipt,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedCandidateTailSpliceBundleFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) writer: CandidateWriterTailAdoptionReady,
}

/// Complete suspended admission result for the actor-owned convergence
/// transaction. The ticket and allocator remain outside the job so ordinary
/// latest-wins cancellation can use the candidate's existing generic abort
/// path at every later phase.
#[cfg(feature = "exact-parser")]
#[must_use = "the admitted adoption splice must be installed in its live candidate"]
pub(crate) struct ParentSelectedAdoptionSpliceStart {
    job: ParentSelectedAdoptionSpliceJob,
    ticket: ArenaBuildTicket,
    identities: DocumentIdentityAllocator,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedAdoptionSpliceStart {
    pub(crate) fn into_parts(
        self,
    ) -> (
        ParentSelectedAdoptionSpliceJob,
        ArenaBuildTicket,
        DocumentIdentityAllocator,
    ) {
        (self.job, self.ticket, self.identities)
    }
}

/// Failed suspended admission still returns the only arena ticket and
/// identity allocator. Source/parser/storage capabilities may already have
/// been consumed, so the caller must install these two values in the
/// candidate and cancel it; exact fallback is no longer legal here.
#[cfg(feature = "exact-parser")]
#[must_use = "failed adoption admission must restore ticket/identities and cancel the candidate"]
pub(crate) struct ParentSelectedAdoptionSpliceStartFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) ticket: ArenaBuildTicket,
    pub(crate) identities: DocumentIdentityAllocator,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedAdoptionSpliceProgress {
    Pending,
    ParentJoinRetryable(RestartCompositeDocumentError),
    Complete,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParentSelectedAdoptionSpliceReceipt {
    pub(crate) writer: CandidateWriterTailAdoptionReceipt,
    pub(crate) acknowledged_lines: u64,
    pub(crate) green_polls: usize,
    pub(crate) checkpoint_polls: usize,
    pub(crate) parent_join_attempts: usize,
    pub(crate) green: Option<crate::GreenJournalSuffixSpliceReceipt>,
    pub(crate) checkpoint: Option<DonorSuffixSpliceReceipt>,
}

#[cfg(feature = "exact-parser")]
enum ParentSelectedAdoptionSplicePhase {
    StartCheckpoint,
    PollGreen,
    PollCheckpoint,
    Join,
    RetryJoin(ParentSelectedRestartCompositeReplacement),
    Complete,
    Taken,
    Failed,
}

/// Lifetime-free actor job joining the exact current source, current green
/// prefix, retained old green suffix, sparse current checkpoint samples,
/// retained old checkpoint suffix, and selected parent under one journal.
/// Every poll performs one subjob transition or one constant-size parent join.
#[cfg(feature = "exact-parser")]
#[must_use = "the adoption splice must be polled, committed, or cancelled with its candidate"]
pub(crate) struct ParentSelectedAdoptionSpliceJob {
    epoch: LiveCandidateEpoch,
    source: crate::CandidateAdoptedSourceSeal,
    storage: SourceProjectionLineBoundaryStorageAck,
    adoption: Option<ParentSelectedRestartCompositeAdoptionLease>,
    green: crate::ResumableGreenJournalSuffixSplice,
    checkpoint_request: Option<DonorSuffixSpliceRequest>,
    checkpoint: Option<ParentOwnedDonorSuffixSpliceJob>,
    parent: Option<crate::storage_only_composite_document::RestartCompositeDocumentBuildManifest>,
    #[cfg(feature = "host-mirror-probe")]
    host_splice: Option<crate::host_mirror::TypedGreenLeafSplice>,
    phase: ParentSelectedAdoptionSplicePhase,
    receipt: ParentSelectedAdoptionSpliceReceipt,
}

#[cfg(feature = "exact-parser")]
impl std::fmt::Debug for ParentSelectedAdoptionSpliceJob {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phase = match self.phase {
            ParentSelectedAdoptionSplicePhase::StartCheckpoint => "StartCheckpoint",
            ParentSelectedAdoptionSplicePhase::PollGreen => "PollGreen",
            ParentSelectedAdoptionSplicePhase::PollCheckpoint => "PollCheckpoint",
            ParentSelectedAdoptionSplicePhase::Join => "Join",
            ParentSelectedAdoptionSplicePhase::RetryJoin(_) => "RetryJoin",
            ParentSelectedAdoptionSplicePhase::Complete => "Complete",
            ParentSelectedAdoptionSplicePhase::Taken => "Taken",
            ParentSelectedAdoptionSplicePhase::Failed => "Failed",
        };
        formatter
            .debug_struct("ParentSelectedAdoptionSpliceJob")
            .field("epoch", &self.epoch)
            .field("phase", &phase)
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

/// Copyable diagnostics for the completed source/composer fast-forward. The
/// receipt is not green-splice or publication authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(feature = "exact-parser")]
pub(crate) struct CandidateWriterTailAdoptionReceipt {
    pub(crate) replayed_prefix_source_pieces: u64,
    pub(crate) checkpoint_prefix_projection_runs: u64,
    pub(crate) replayed_prefix_projection_runs: u64,
    pub(crate) cumulative_prefix_projection_runs: u64,
    pub(crate) adopted_suffix_projection_runs: u64,
    pub(crate) final_projection_runs: u64,
    pub(crate) accepted_projection_prefix_metric: SourceLedgerMetric,
    pub(crate) physical_parser_prefix_metric: SourceLedgerMetric,
    pub(crate) final_source_metric: SourceLedgerMetric,
    pub(crate) storage: crate::GreenSourceTailAdoptionReceipt,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct CandidateWriterLineBoundaryCaptureFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) writer: CandidateWriter,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct CandidateWriterLineBoundaryResumeFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) continuation: CandidateWriterLineBoundaryContinuation,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct CandidateWriterTailAdoptionFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) continuation: CandidateWriterLineBoundaryContinuation,
    pub(crate) tail: SourceBoundGreenTailAdoption,
}

#[cfg(feature = "exact-parser")]
impl CandidateWriterLineBoundaryContinuation {
    pub(crate) const fn source_descriptor(&self) -> crate::SourceSnapshotDescriptor {
        self.epoch.source()
    }

    pub(crate) fn parent_selected_direct_tail_splice_is_eligible(&self) -> bool {
        let Some(snapshot) = self.green_prefix_snapshot.as_ref() else {
            return false;
        };
        let Some(cut) = self.composer.green_cut() else {
            return false;
        };
        self.completion_route == CandidateWriterCompletionRoute::ParentSelectedAdoption
            && self.ticket.is_some()
            && self.active_fenced_code.is_none()
            && self
                .donor_checkpoint_samples
                .parent_selected_direct_tail_is_eligible(self.epoch)
            && snapshot.build_id() == self.epoch.build_id()
            && snapshot.source_before() == cut.source_before()
            && self.builder.build_id() == self.epoch.build_id()
            && self.composer.build_id() == self.epoch.build_id()
    }

    /// Runs the storage half of direct-tail admission while every source and
    /// composer capability is still borrowed. A recoverable green mismatch is
    /// returned as `Ineligible`; stale/corrupt authority remains an error and
    /// therefore poisons the candidate instead of silently selecting a farther
    /// checkpoint.
    pub(crate) fn preflight_parent_selected_green_suffix(
        &self,
        arena: &PageArena,
        parent_tail: &ParentSelectedCandidateAdoptionTail,
        mapped_tail: &SourceBoundGreenTailAdoption,
    ) -> Result<crate::GreenJournalSuffixPreflight, CandidateWriterError> {
        if !self.parent_selected_direct_tail_splice_is_eligible()
            || mapped_tail.epoch() != self.epoch
            || parent_tail.build_id() != self.epoch.build_id()
        {
            return Err(CandidateWriterError::Invariant(
                "green suffix preflight authority differs from the paused convergence writer",
            ));
        }
        let ticket = self.ticket.as_ref().ok_or(CandidateWriterError::Invariant(
            "green suffix preflight lost its suspended ticket",
        ))?;
        let snapshot =
            self.green_prefix_snapshot
                .as_ref()
                .ok_or(CandidateWriterError::Invariant(
                    "green suffix preflight lost its prefix snapshot",
                ))?;
        let retained_green = parent_tail.adoption.green_for_convergence(ticket, arena)?;
        crate::ResumableGreenJournalSuffixSplice::preflight_from_parent(
            ticket,
            arena,
            &self.builder,
            snapshot,
            mapped_tail,
            &retained_green,
        )
        .map_err(Into::into)
    }

    pub(crate) fn source_identity(&self) -> crate::SourceRootId {
        self.epoch.source().root
    }

    pub(crate) fn cursor_offset(&self) -> Result<usize, CandidateWriterError> {
        usize::try_from(self.source.absolute_offset())
            .map_err(|_| CandidateWriterError::Invariant("checkpoint source offset exceeds usize"))
    }

    #[cfg(test)]
    pub(crate) const fn retained_source_bytes_for_test(&self) -> usize {
        self.source.retained_source_bytes_for_test()
    }

    #[cfg(test)]
    pub(crate) fn retained_source_heap_bytes_for_test(&self) -> usize {
        self.source.retained_heap_bytes_for_test()
    }

    #[cfg(test)]
    pub(crate) fn retained_open_depth_for_test(&self) -> usize {
        self.source.pairing_view().path_len()
    }

    pub(crate) fn build_id(&self) -> Result<ArenaBuildId, CandidateWriterError> {
        self.ticket
            .as_ref()
            .map(ArenaBuildTicket::id)
            .ok_or(CandidateWriterError::Invariant(
                "checkpoint suspended ticket is missing",
            ))
    }

    /// Performs the first old-index convergence lookup while this writer owns
    /// the exact suspended build ticket. Neither the ticket nor a standalone
    /// retained-index handle crosses the actor boundary.
    pub(crate) fn begin_parent_selected_old_convergence(
        &self,
        arena: &PageArena,
        tail: &ParentSelectedCandidateAdoptionTail,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        let ticket = self.ticket.as_ref().ok_or(CandidateWriterError::Invariant(
            "paused candidate writer lost its suspended build ticket",
        ))?;
        tail.begin_old_convergence(ticket, arena)
    }

    /// Advances the retained old-checkpoint chain after a live mismatch. This
    /// is intentionally separate from the writer-owned fresh-sample cursor.
    pub(crate) fn advance_parent_selected_old_convergence(
        &self,
        arena: &PageArena,
        tail: &ParentSelectedCandidateAdoptionTail,
        current: ParentBoundDonorSuccessor,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        let ticket = self.ticket.as_ref().ok_or(CandidateWriterError::Invariant(
            "paused candidate writer lost its suspended build ticket",
        ))?;
        tail.advance_old_convergence(ticket, arena, current)
    }

    /// Revalidates and crosses one immediate donor-to-donor outer transition.
    /// Non-donor barriers cannot construct the consumed capability.
    pub(crate) fn advance_parent_selected_old_convergence_partition(
        &self,
        arena: &PageArena,
        tail: &ParentSelectedCandidateAdoptionTail,
        transition: ParentBoundDonorPartitionTransition,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        let ticket = self.ticket.as_ref().ok_or(CandidateWriterError::Invariant(
            "paused candidate writer lost its suspended build ticket",
        ))?;
        tail.advance_old_convergence_partition(ticket, arena, transition)
    }

    /// Resolves old green semantic A and starts its one-pass lineage mapping
    /// while the exact R checkpoint remains jointly paused.
    pub(crate) fn begin_parent_selected_convergence_mapping(
        &self,
        arena: &PageArena,
        source: &SourceStore,
        tail: &ParentSelectedCandidateAdoptionTail,
        old_convergence: ParentBoundDonorSuccessor,
    ) -> Result<ParentSelectedConvergenceMapStart, ParentSelectedConvergenceMapError> {
        let ticket = self
            .ticket
            .as_ref()
            .ok_or(ParentSelectedConvergenceMapError::Invariant(
                "paused convergence mapper lost its suspended build ticket",
            ))?;
        tail.begin_convergence_mapping(ticket, arena, source, self.epoch, old_convergence)
    }

    /// Compares this already joined paused source cut to mapped C. This is
    /// needed before resuming from R (or a mismatching live C), because an edit
    /// may collapse the next old interval onto the current physical boundary.
    pub(crate) fn relation_to_parent_selected_convergence(
        &self,
        mapped: &ParentSelectedMappedConvergence,
    ) -> Result<ParentSelectedConvergenceTargetRelation, CandidateWriterError> {
        let source = self.source.pairing_view();
        let emitted = source.emitted_metric();
        let current = RelativeCheckpointMeasure::new(
            emitted.bytes(),
            emitted.utf16(),
            source.line_ordinal(),
            0,
            0,
        );
        mapped
            .relation_to_current_cut(self.epoch, current)
            .map_err(|_| {
                CandidateWriterError::Invariant(
                    "paused candidate source cut disagrees with mapped convergence target",
                )
            })
    }

    pub(crate) fn validate_parser_pairing(
        &self,
        parser: &ParserLineBoundaryCheckpointAuthority,
        bindings: &[CandidateWriterBinding],
    ) -> Result<(), CandidateWriterError> {
        let build = self.validate_parser_authority_and_path(parser, bindings)?;
        self.validate_parser_deferred_role(parser)?;
        self.validate_projection_cut()?;
        self.validate_provisional_groups(parser, bindings, build)
    }

    /// Mints the first sparse donor interval from the joined actor cut. The
    /// first interval is measured from the document origin; all five end axes
    /// come from the paused writer and none are accepted from the driver.
    pub(crate) fn capture_first_donor_checkpoint_sample(
        &mut self,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<CapturedDonorCheckpointSample, CandidateWriterError> {
        match &self.donor_checkpoint_samples.chain {
            DonorCheckpointSampleChainState::DocumentOrigin { expected: None } => {}
            DonorCheckpointSampleChainState::DocumentOrigin { expected: Some(_) } => {
                return Err(CandidateWriterError::Invariant(
                    "donor checkpoint sample chain already started",
                ));
            }
            DonorCheckpointSampleChainState::UnseededRetainedPrefix
            | DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(_)
            | DonorCheckpointSampleChainState::ParentSelectedContinuing(_) => {
                return Err(CandidateWriterError::Invariant(
                    "donor checkpoint sample chain requires a document-origin build",
                ));
            }
        }
        let current = self.validate_joined_donor_cut(bindings, joined_donor)?;
        let sample = DonorCheckpointSampleDraft::try_new(current.cut, current.donor)?;
        let state = DonorCheckpointSampleCursorState {
            epoch: self.epoch,
            sample_ordinal: 1,
            cumulative_cut: current.cut,
        };
        self.donor_checkpoint_samples.push_sample(sample)?;
        self.donor_checkpoint_samples.chain = DonorCheckpointSampleChainState::DocumentOrigin {
            expected: Some(state),
        };
        Ok(CapturedDonorCheckpointSample {
            #[cfg(test)]
            interval: current.cut,
            next: DonorCheckpointSampleCursor { state },
        })
    }

    /// Extends the one sparse donor chain at a later joined checkpoint. The
    /// input cursor is consumed, checked against the actor's own accumulator,
    /// and reminted only after every axis advances monotonically and donor
    /// conversion succeeds.
    pub(crate) fn capture_successive_donor_checkpoint_sample(
        &mut self,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
        cursor: DonorCheckpointSampleCursor,
    ) -> Result<CapturedDonorCheckpointSample, Box<DonorCheckpointSampleCaptureFailure>> {
        let prior = cursor.state;
        let result = (|| {
            self.donor_checkpoint_samples
                .validate_cursor(self.epoch, prior)?;
            let current = self.validate_joined_donor_cut(bindings, joined_donor)?;
            let interval = prior.interval_to(current.cut)?;
            let sample = DonorCheckpointSampleDraft::try_new(interval, current.donor)?;
            let sample_ordinal =
                prior
                    .sample_ordinal
                    .checked_add(1)
                    .ok_or(CandidateWriterError::Invariant(
                        "donor checkpoint sample ordinal overflow",
                    ))?;
            let state = DonorCheckpointSampleCursorState {
                epoch: self.epoch,
                sample_ordinal,
                cumulative_cut: current.cut,
            };
            Ok((sample, state))
        })();
        match result {
            Ok((sample, state)) => {
                #[cfg(test)]
                let interval = prior
                    .interval_to(state.cumulative_cut)
                    .expect("validated successive checkpoint interval remains exact");
                if let Err(error) = self.donor_checkpoint_samples.push_sample(sample) {
                    return Err(Box::new(DonorCheckpointSampleCaptureFailure {
                        error,
                        cursor,
                    }));
                }
                self.donor_checkpoint_samples.chain =
                    DonorCheckpointSampleChainState::DocumentOrigin {
                        expected: Some(state),
                    };
                Ok(CapturedDonorCheckpointSample {
                    #[cfg(test)]
                    interval,
                    next: DonorCheckpointSampleCursor { state },
                })
            }
            Err(error) => Err(Box::new(DonorCheckpointSampleCaptureFailure {
                error,
                cursor,
            })),
        }
    }

    /// Captures the next sparse sample in a retained-prefix candidate. The
    /// first interval is measured from the authenticated parent restart cut;
    /// later intervals advance the private parent-selected cursor. This path
    /// is distinct from the document-origin chain and therefore cannot be fed
    /// into an independent whole-document checkpoint-index commit.
    ///
    /// Any allocation failure after the linear origin/cursor is taken leaves
    /// the accumulator in `UnseededRetainedPrefix`, which is deliberately
    /// unrecoverable except by whole-candidate cancellation.
    pub(crate) fn capture_parent_selected_suffix_sample(
        &mut self,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<CapturedParentSelectedSuffixSample, CandidateWriterError> {
        let current = self.validate_joined_donor_cut(bindings, joined_donor)?;
        let chain = std::mem::replace(
            &mut self.donor_checkpoint_samples.chain,
            DonorCheckpointSampleChainState::UnseededRetainedPrefix,
        );
        let sample_len_before = self.donor_checkpoint_samples.samples.len();
        let sample_total_before = self.donor_checkpoint_samples.sample_total;
        let maximum_path_depth_before = self.donor_checkpoint_samples.maximum_path_depth;
        let (interval, next, chain_rollback) = match chain {
            DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(origin) => {
                origin.begin_reversible(self.epoch, current.cut)?
            }
            DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor) => {
                cursor.advance_reversible(self.epoch, current.cut)?
            }
            other @ (DonorCheckpointSampleChainState::DocumentOrigin { .. }
            | DonorCheckpointSampleChainState::UnseededRetainedPrefix) => {
                self.donor_checkpoint_samples.chain = other;
                return Err(CandidateWriterError::Invariant(
                    "parent-selected suffix sample chain is unavailable",
                ));
            }
        };
        let (sample, donor_identity) =
            match DonorCheckpointSampleDraft::try_new_with_identity_witness(interval, current.donor)
            {
                Ok(sample) => sample,
                Err(error) => {
                    let prior = chain_rollback.restore(self.epoch, next)?;
                    self.donor_checkpoint_samples.chain = match prior {
                        ParentSelectedSuffixSamplePrior::AwaitingFirst(origin) => {
                            DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(origin)
                        }
                        ParentSelectedSuffixSamplePrior::Continuing(cursor) => {
                            DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor)
                        }
                    };
                    return Err(error.into());
                }
            };
        if let Err(error) = self.donor_checkpoint_samples.push_sample(sample) {
            let prior = chain_rollback.restore(self.epoch, next)?;
            self.donor_checkpoint_samples.chain = match prior {
                ParentSelectedSuffixSamplePrior::AwaitingFirst(origin) => {
                    DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(origin)
                }
                ParentSelectedSuffixSamplePrior::Continuing(cursor) => {
                    DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor)
                }
            };
            return Err(error);
        }
        let capture = CapturedParentSelectedSuffixSample {
            epoch: next.epoch(),
            interval,
            cumulative_cut: next.cumulative_cut(),
            sample_ordinal: next.sample_ordinal(),
            donor_identity,
            rollback: CandidateParentSelectedSampleRollback {
                chain: chain_rollback,
                sample_len_before,
                sample_total_before,
                maximum_path_depth_before,
            },
        };
        self.donor_checkpoint_samples.chain =
            DonorCheckpointSampleChainState::ParentSelectedContinuing(next);
        Ok(capture)
    }

    /// Makes one donor-mismatching convergence probe ephemeral. Only the
    /// checkpoint draft/cursor fold is rewound; parser, source, projection,
    /// and packed-green state remain paused at the observed physical cut.
    pub(crate) fn reject_parent_selected_suffix_sample(
        &mut self,
        rejected: ParentSelectedRejectedSuffixSample,
    ) -> Result<(), CandidateWriterError> {
        let ParentSelectedRejectedSuffixSample {
            epoch,
            interval,
            cumulative_cut,
            sample_ordinal,
            rollback,
        } = rejected;
        if epoch != self.epoch
            || rollback.sample_len_before.checked_add(1)
                != Some(self.donor_checkpoint_samples.samples.len())
            || usize::try_from(sample_ordinal).ok()
                != Some(self.donor_checkpoint_samples.samples.len())
            || rollback
                .sample_total_before
                .checked_add(interval)
                .map_err(CandidateWriterError::CheckpointIndex)?
                != self.donor_checkpoint_samples.sample_total
        {
            return Err(CandidateWriterError::Invariant(
                "rejected convergence probe crossed its checkpoint accumulator",
            ));
        }
        let chain = std::mem::replace(
            &mut self.donor_checkpoint_samples.chain,
            DonorCheckpointSampleChainState::UnseededRetainedPrefix,
        );
        let current = match chain {
            DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor)
                if cursor.epoch() == epoch
                    && cursor.sample_ordinal() == sample_ordinal
                    && cursor.cumulative_cut() == cumulative_cut =>
            {
                cursor
            }
            _ => {
                return Err(CandidateWriterError::Invariant(
                    "rejected convergence probe is not the current suffix sample",
                ));
            }
        };
        let prior = rollback.chain.restore(epoch, current)?;
        let old_len = self.donor_checkpoint_samples.samples.len();
        self.donor_checkpoint_samples
            .samples
            .pop()
            .ok_or(CandidateWriterError::Invariant(
                "rejected convergence probe lost its checkpoint draft",
            ))?;
        let new_len = old_len - 1;
        let mut remove_last_span = false;
        if let Some(span) = self.donor_checkpoint_samples.normalization_spans.last_mut() {
            if span.sample_end > old_len {
                return Err(CandidateWriterError::Invariant(
                    "normalization span exceeds the rejected checkpoint chain",
                ));
            }
            if span.sample_end == old_len {
                if span.sample_start >= span.sample_end {
                    return Err(CandidateWriterError::Invariant(
                        "normalization span cannot contain the rejected checkpoint",
                    ));
                }
                span.sample_end = new_len;
                remove_last_span = span.sample_start == span.sample_end;
            }
        }
        if remove_last_span {
            self.donor_checkpoint_samples.normalization_spans.pop();
        }
        self.donor_checkpoint_samples.sample_total = rollback.sample_total_before;
        self.donor_checkpoint_samples.maximum_path_depth = rollback.maximum_path_depth_before;
        self.donor_checkpoint_samples.chain = match prior {
            ParentSelectedSuffixSamplePrior::AwaitingFirst(origin) => {
                DonorCheckpointSampleChainState::ParentSelectedAwaitingFirst(origin)
            }
            ParentSelectedSuffixSamplePrior::Continuing(cursor) => {
                DonorCheckpointSampleChainState::ParentSelectedContinuing(cursor)
            }
        };
        Ok(())
    }

    /// Borrows the already joined pause to mint the one transient cross-build
    /// Setext draft. Source, green, and donor coordinates all come from their
    /// existing linear capabilities; the caller supplies no `BlockId`, stamp,
    /// event ordinal, or source cut.
    pub(crate) fn capture_in_memory_setext_checkpoint(
        &self,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<InMemorySetextCheckpointDraft, CandidateWriterError> {
        let parser = joined_donor.parser();
        let build = self.validate_parser_authority_and_path(parser, bindings)?;
        self.validate_parser_deferred_role(parser)?;
        self.validate_projection_cut()?;
        self.validate_provisional_groups(parser, bindings, build)?;

        let source = self.source.seal_retained_setext_source_draft()?;
        let group = self
            .active_paragraph
            .as_ref()
            .ok_or(CandidateWriterError::Invariant(
                "retained Setext checkpoint has no active Paragraph group",
            ))?;
        let provisional = group.enter.as_ref().ok_or(CandidateWriterError::Invariant(
            "retained Setext checkpoint lost its provisional Paragraph token",
        ))?;
        if group.promoted_setext || group.build != build {
            return Err(CandidateWriterError::Invariant(
                "retained Setext checkpoint is not a provisional Paragraph",
            ));
        }
        let cut = self
            .composer
            .green_cut()
            .ok_or(CandidateWriterError::Invariant(
                "retained Setext checkpoint lacks an exact green cut",
            ))?;
        let green = self
            .builder
            .seal_retained_setext_green_checkpoint(provisional, cut)?;
        let accepted = green.accepted_source_cut();
        let (parser, donor) = joined_donor.into_parts();
        if green.old_build() != build
            || green.block() != source.terminal_block()?
            || accepted.bytes != source.accepted_bytes()
            || accepted.utf16 != source.accepted_utf16()
            || donor.receipt().materialized_path_records != bindings.len()
            || donor.receipt().retained_source_bytes != 0
        {
            return Err(CandidateWriterError::Invariant(
                "retained Setext source/green/donor drafts disagree",
            ));
        }
        // This first-sample proof has no preceding checkpoint interval. Mint
        // its absolute cut from the joined actor-owned axes before the opaque
        // donor leaves this method; no caller can relabel the capture with
        // independently authored interval scalars.
        let checkpoint_cut = self.current_joined_checkpoint_cut()?;
        if checkpoint_cut.source_bytes() != source.physical_bytes()
            || checkpoint_cut.source_utf16() != source.physical_utf16()
            || checkpoint_cut.physical_lines() != source.line_ordinal()
            || checkpoint_cut.green_events() != green.accepted_event_cut()
        {
            return Err(CandidateWriterError::Invariant(
                "retained Setext checkpoint axes disagree with joined actor cut",
            ));
        }
        let (donor, donor_identity) =
            DonorCheckpointSampleDraft::try_new_with_identity_witness(checkpoint_cut, donor)?;
        Ok(InMemorySetextCheckpointDraft::from_joined_checkpoint(
            source,
            green,
            parser,
            donor,
            donor_identity,
            checkpoint_cut,
        ))
    }

    fn validate_joined_donor_cut(
        &self,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<ValidatedJoinedDonorCut, CandidateWriterError> {
        let parser = joined_donor.parser();
        self.validate_parser_pairing(parser, bindings)?;
        let cut = self.current_joined_checkpoint_cut()?;
        let donor = joined_donor.into_donor();
        let receipt = donor.receipt();
        if receipt.materialized_path_records != bindings.len() || receipt.retained_source_bytes != 0
        {
            return Err(CandidateWriterError::Invariant(
                "joined donor sample disagrees with the actor checkpoint path",
            ));
        }
        Ok(ValidatedJoinedDonorCut { cut, donor })
    }

    fn current_joined_checkpoint_cut(
        &self,
    ) -> Result<RelativeCheckpointMeasure, CandidateWriterError> {
        let source = self.source.pairing_view();
        let emitted = source.emitted_metric();
        let green = self
            .composer
            .green_cut()
            .ok_or(CandidateWriterError::Invariant(
                "joined donor checkpoint lacks an exact green cut",
            ))?;
        let projection_runs = self
            .composer
            .cumulative_projection_runs()
            .map_err(CandidateWriterError::Projection)?;
        Ok(RelativeCheckpointMeasure::new(
            emitted.bytes(),
            emitted.utf16(),
            source.line_ordinal(),
            green.events_before(),
            projection_runs,
        ))
    }

    fn validate_parser_authority_and_path(
        &self,
        parser: &ParserLineBoundaryCheckpointAuthority,
        bindings: &[CandidateWriterBinding],
    ) -> Result<ArenaBuildId, CandidateWriterError> {
        let parser_view = parser.pairing_view();
        let source_view = self.source.pairing_view();
        let build = self.build_id()?;
        if parser.epoch() != self.epoch
            || source_view.epoch() != self.epoch
            || self.composer.epoch() != self.epoch
            || build != self.epoch.build_id()
            || self.builder.build_id() != build
            || self.composer.build_id() != build
            || parser_view.profile() != SyntaxProfile::CommonMark
        {
            return Err(CandidateWriterError::WrongCandidate);
        }

        let parser_line = u64::try_from(parser_view.line_number())
            .map_err(|_| CandidateWriterError::Invariant("parser line number exceeds u64"))?;
        let emitted = source_view.emitted_metric();
        if parser_line != source_view.line_ordinal()
            || parser_view.last_line_length() != source_view.last_line_length()
            || parser_view.open_frame_count() != source_view.path_len()
            || parser_view.open_frame_count() != bindings.len()
            || parser_view.current_frame_depth().checked_add(1)
                != Some(parser_view.open_frame_count())
            || source_view.absolute_offset() != emitted.bytes()
            || source_view.structural_state_generation() == 0
            || self.next_source_transition == 0
        {
            return Err(CandidateWriterError::Invariant(
                "parser and writer checkpoint cursor/path shapes disagree",
            ));
        }

        for (index, ((parser_kind, source_kind), binding)) in parser
            .open_green_kinds()
            .zip(source_view.path_kinds())
            .zip(bindings)
            .enumerate()
        {
            if parser_kind != source_kind
                || binding.kind() != source_kind
                || !source_view.binding_matches(index, &binding.binding)
            {
                return Err(CandidateWriterError::Invariant(
                    "parser, source ledger, and writer bindings disagree",
                ));
            }
        }

        Ok(build)
    }

    fn validate_parser_deferred_role(
        &self,
        parser: &ParserLineBoundaryCheckpointAuthority,
    ) -> Result<(), CandidateWriterError> {
        let parser_view = parser.pairing_view();
        let source_view = self.source.pairing_view();
        match (parser.deferred_role(), source_view.deferred_role()) {
            (DirectLineBoundaryDeferredRole::None, CandidateLineBoundaryDeferredRole::None) => {}
            (
                DirectLineBoundaryDeferredRole::Terminator,
                CandidateLineBoundaryDeferredRole::Terminator { owner_depth },
            ) if owner_depth == parser_view.current_frame_depth() => {}
            (
                DirectLineBoundaryDeferredRole::BlankGap { floor_depth },
                CandidateLineBoundaryDeferredRole::BlankGap,
            ) if source_view.accepts_blank_gap_floor(floor_depth) => {}
            (DirectLineBoundaryDeferredRole::Invalid, _) => {
                return Err(CandidateWriterError::Invariant(
                    "parser checkpoint has invalid deferred source state",
                ));
            }
            _ => {
                return Err(CandidateWriterError::Invariant(
                    "parser and source deferred checkpoint roles disagree",
                ));
            }
        }

        Ok(())
    }

    fn validate_projection_cut(&self) -> Result<(), CandidateWriterError> {
        let source_view = self.source.pairing_view();
        let accepted = source_view.accepted_projection_metric()?;
        let composer_source = self.composer.source_before();
        if accepted.bytes() != composer_source.bytes || accepted.utf16() != composer_source.utf16 {
            return Err(CandidateWriterError::Invariant(
                "source and projection checkpoint metrics disagree",
            ));
        }
        let cut = self
            .composer
            .green_cut()
            .ok_or(CandidateWriterError::Invariant(
                "production checkpoint lacks an exact green cut",
            ))?;
        if cut.source_before() != composer_source || !self.builder.line_boundary_cut_is_current(cut)
        {
            return Err(CandidateWriterError::Invariant(
                "projection and packed-green checkpoint cuts disagree",
            ));
        }
        if self.composer.receipt().canonical_projection_runs()? != self.green_runs_acknowledged {
            return Err(CandidateWriterError::Invariant(
                "projection and packed-green checkpoint run counts disagree",
            ));
        }

        Ok(())
    }

    fn validate_provisional_groups(
        &self,
        parser: &ParserLineBoundaryCheckpointAuthority,
        bindings: &[CandidateWriterBinding],
        build: ArenaBuildId,
    ) -> Result<(), CandidateWriterError> {
        if self.active_paragraph.is_some() && self.active_fenced_code.is_some() {
            return Err(CandidateWriterError::Invariant(
                "Paragraph and fenced-code checkpoint groups overlap",
            ));
        }
        let terminal = bindings.last().ok_or(CandidateWriterError::Invariant(
            "checkpoint open path has a terminal binding",
        ))?;
        match (terminal.kind(), self.active_paragraph.as_ref()) {
            (GreenKind::PARAGRAPH, Some(group))
                if group.build == build
                    && group.block == terminal.binding.block_id()
                    && group.enter.is_some()
                    && !group.promoted_setext => {}
            (GreenKind::HEADING, Some(group))
                if group.build == build
                    && group.block == terminal.binding.block_id()
                    && group.enter.is_none()
                    && group.promoted_setext => {}
            // A typed ATX Heading is opened directly and therefore has no
            // provisional Paragraph normalization group.
            (GreenKind::HEADING, None) => {}
            (GreenKind::PARAGRAPH | GreenKind::HEADING, _) => {
                return Err(CandidateWriterError::Invariant(
                    "checkpoint terminal disagrees with active Paragraph group",
                ));
            }
            (_, None) => {}
            (_, Some(_)) => {
                return Err(CandidateWriterError::Invariant(
                    "active Paragraph group is not terminal",
                ));
            }
        }
        match (terminal.kind(), self.active_fenced_code.as_ref()) {
            (GreenKind::FENCED_CODE, Some(fold))
                if fold.build == build
                    && fold.block == terminal.binding.block_id()
                    && match (
                        fold.info_end,
                        fold.literal_start,
                        self.source
                            .pairing_view()
                            .path_logical_metric(parser.pairing_view().current_frame_depth()),
                    ) {
                        (Some(info), Some(literal), Some(logical_end)) => {
                            source_metric_precedes(info, literal)
                                && source_metric_precedes(literal, logical_end)
                        }
                        _ => false,
                    } => {}
            (GreenKind::FENCED_CODE, _) => {
                return Err(CandidateWriterError::Invariant(
                    "checkpoint terminal disagrees with fenced-code fold",
                ));
            }
            (_, None) => {}
            (_, Some(_)) => {
                return Err(CandidateWriterError::Invariant(
                    "active fenced-code fold is not terminal",
                ));
            }
        }
        Ok(())
    }

    /// Consumes the actual source/composer line-boundary authorities into the
    /// distinct adopted-tail writer state. This does not publish or splice the
    /// old green suffix; the future grammar-convergence and green/index journal
    /// join must consume this state before commit is possible.
    pub(crate) fn seal_source_composer_adopted_tail(
        self,
        mut tail: SourceBoundGreenTailAdoption,
    ) -> Result<CandidateWriterTailAdoptionReady, Box<CandidateWriterTailAdoptionFailure>> {
        if self.reference_semantic.is_some() {
            return Err(Box::new(CandidateWriterTailAdoptionFailure {
                error: CandidateWriterError::Invariant(
                    "reference semantic restart adoption is not integrated",
                ),
                continuation: self,
                tail,
            }));
        }
        if let Err(error) = self
            .source
            .rebind_adopted_tail(&mut tail)
            .map_err(Into::into)
        {
            return Err(Box::new(CandidateWriterTailAdoptionFailure {
                error,
                continuation: self,
                tail,
            }));
        }
        if let Err(error) = self.validate_source_composer_tail_adoption(&tail) {
            return Err(Box::new(CandidateWriterTailAdoptionFailure {
                error,
                continuation: self,
                tail,
            }));
        }

        let Self {
            epoch,
            source,
            composer,
            builder,
            ticket,
            identities,
            next_source_transition,
            green_runs_acknowledged,
            donor_checkpoint_samples,
            reference_semantic,
            green_prefix_snapshot,
            active_paragraph,
            active_fenced_code,
            completion_route,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype,
        } = self;
        debug_assert!(reference_semantic.is_none());
        let (source, tail) = source
            .seal_adopted_tail(tail)
            .expect("tail adoption was validated immediately before the linear move");
        let composer = composer
            .seal_adopted_tail(source, tail)
            .expect("composer adoption was validated immediately before the linear move");
        debug_assert_eq!(
            composer.replayed_prefix_projection_runs(),
            green_runs_acknowledged
        );
        Ok(CandidateWriterTailAdoptionReady {
            epoch,
            composer,
            builder,
            ticket,
            identities,
            next_source_transition,
            green_runs_acknowledged,
            donor_checkpoint_samples,
            green_prefix_snapshot,
            active_paragraph,
            active_fenced_code,
            completion_route,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype,
        })
    }

    fn validate_source_composer_tail_adoption(
        &self,
        tail: &SourceBoundGreenTailAdoption,
    ) -> Result<(), CandidateWriterError> {
        self.validate_projection_cut()?;
        self.source.validate_adopted_tail(tail)?;
        let source = self.source.pairing_view();
        let prefix = tail.current_prefix();
        let composer_receipt = self.composer.receipt();
        let checkpoint_prefix_projection_runs = self
            .composer
            .checkpoint_prefix_projection_runs()
            .map_err(CandidateWriterError::Projection)?;
        let canonical_suffix_projection_runs = composer_receipt.canonical_projection_runs()?;
        let cumulative_prefix_projection_runs = checkpoint_prefix_projection_runs
            .checked_add(canonical_suffix_projection_runs)
            .ok_or(CandidateWriterError::Invariant(
                "tail-adoption cumulative projection count overflow",
            ))?;
        if self.composer.epoch() != self.epoch
            || self.composer.source_before() != prefix
            || composer_receipt.source_pieces_consumed != source.replayed_source_piece_count()
            || canonical_suffix_projection_runs != self.green_runs_acknowledged
        {
            return Err(CandidateWriterError::Invariant(
                "source, composer, and green prefix do not share tail-adoption authority",
            ));
        }
        // `tail.prefix_coverage_runs()` is the old document's prefix count and
        // was already authenticated against old C by the mapper. The current
        // prefix is allowed to have a different count after an edit; its
        // honest cumulative value is the selected-R base plus freshly replayed
        // runs, and the adopted old suffix is rebased onto that value.
        let _ = cumulative_prefix_projection_runs;
        Ok(())
    }

    pub(crate) fn resume_with_cursor_pair(
        self,
        pair: SourceResumeCursorPair,
    ) -> Result<CandidateWriter, Box<CandidateWriterLineBoundaryResumeFailure>> {
        if pair.descriptor() != self.epoch.source()
            || pair.offset() != usize::try_from(self.source.absolute_offset()).unwrap_or(usize::MAX)
        {
            return Err(Box::new(CandidateWriterLineBoundaryResumeFailure {
                error: CandidateWriterError::WrongCandidate,
                continuation: self,
            }));
        }
        let authoritative_root_utf16 = pair.total_utf16();
        let physical_line_start = pair.is_physical_line_start();
        let (authoritative, recognition) = pair.into_cursors();
        if let Err(error) = self.source.validate_resume_authority(
            self.epoch,
            authoritative_root_utf16,
            &authoritative,
            &recognition,
            physical_line_start,
        ) {
            return Err(Box::new(CandidateWriterLineBoundaryResumeFailure {
                error: error.into(),
                continuation: self,
            }));
        }

        let Self {
            epoch,
            source,
            composer,
            builder,
            mut ticket,
            identities,
            next_source_transition,
            green_runs_acknowledged,
            donor_checkpoint_samples,
            reference_semantic,
            green_prefix_snapshot: _,
            active_paragraph,
            active_fenced_code,
            completion_route,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype,
        } = self;
        let ledger = source.resume_with_validated_cursors(authoritative, recognition);
        let (composer, storage) =
            SourceBoundProjectionComposer::resume_line_boundary(epoch, composer)
                .expect("captured composer continuation passed the composite checkpoint join");
        let ticket = ticket
            .take()
            .expect("captured checkpoint owns one suspended arena ticket");
        Ok(CandidateWriter {
            epoch,
            ledger,
            composer: Some(composer),
            builder: Some(builder),
            lease: CandidateWriterBuildLease::Suspended(ticket),
            identities,
            action: None,
            issued_source_transition: None,
            next_source_transition,
            green_runs_acknowledged,
            donor_checkpoint_samples,
            reference_semantic,
            completion: None,
            active_paragraph,
            deferred_normalization: None,
            active_fenced_code,
            completion_route,
            last_line_boundary_storage: Some(storage),
            poisoned: false,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype,
        })
    }

    pub(crate) fn begin_abort(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<ArenaBuildId, ArenaBuildError> {
        let ticket = self.ticket.take().ok_or(ArenaBuildError::Invariant(
            "checkpoint suspended ticket is missing",
        ))?;
        match arena.begin_build_abort(ticket) {
            Ok(build) => Ok(build),
            Err(failure) => {
                self.ticket = Some(failure.ticket);
                Err(failure.error)
            }
        }
    }

    pub(crate) fn into_identities_after_abort(
        self,
    ) -> (DocumentIdentityAllocator, CandidateWriterHeapRetirement) {
        debug_assert!(self.ticket.is_none());
        let retirement = self.donor_checkpoint_samples.into_heap_retirement();
        (self.identities, retirement)
    }
}

#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Consumed by the next green/index journal integration gate.
impl CandidateWriterTailAdoptionReady {
    pub(crate) fn try_into_parent_selected_splice_bundle(
        self,
    ) -> Result<
        ParentSelectedCandidateTailSpliceBundle,
        Box<ParentSelectedCandidateTailSpliceBundleFailure>,
    > {
        let fail = |writer: CandidateWriterTailAdoptionReady, error| {
            Box::new(ParentSelectedCandidateTailSpliceBundleFailure { error, writer })
        };
        let Some(snapshot) = self.green_prefix_snapshot.as_ref() else {
            return Err(fail(self, CandidateWriterError::TailSpliceIneligible));
        };
        let Some(cut) = self.composer.green_cut() else {
            return Err(fail(
                self,
                CandidateWriterError::Invariant("tail-ready writer lost its exact green line cut"),
            ));
        };
        if self.completion_route != CandidateWriterCompletionRoute::ParentSelectedAdoption
            || self.ticket.is_none()
            || self.active_fenced_code.is_some()
            || !self
                .donor_checkpoint_samples
                .parent_selected_direct_tail_is_eligible(self.epoch)
            || snapshot.build_id() != self.epoch.build_id()
            || snapshot.source_before() != cut.source_before()
            || self.builder.build_id() != self.epoch.build_id()
            || self.composer.build_id() != self.epoch.build_id()
            || self.composer.source() != self.epoch.source()
        {
            return Err(fail(self, CandidateWriterError::TailSpliceIneligible));
        }

        let receipt = self.receipt();
        let Self {
            epoch,
            composer,
            builder,
            mut ticket,
            identities,
            next_source_transition: _,
            green_runs_acknowledged: _,
            donor_checkpoint_samples,
            green_prefix_snapshot,
            active_paragraph: _,
            active_fenced_code: None,
            completion_route: CandidateWriterCompletionRoute::ParentSelectedAdoption,
            #[cfg(test)]
                fail_after_green_ack_before_ledger_close: _,
            #[cfg(test)]
                fail_after_setext_green_ack_before_ledger_retype: _,
        } = self
        else {
            unreachable!("tail splice shape was validated immediately before its linear move")
        };
        let (source, storage, old_green_tail) = composer.into_green_storage_and_old_tail();
        let checkpoints = donor_checkpoint_samples
            .into_parent_selected_direct_tail(epoch)
            .expect("direct checkpoint tail was validated before the linear move");
        Ok(ParentSelectedCandidateTailSpliceBundle {
            epoch,
            source,
            storage,
            old_green_tail,
            builder,
            green_prefix_snapshot: green_prefix_snapshot
                .expect("green prefix snapshot was validated before the linear move"),
            ticket: ticket
                .take()
                .expect("tail splice ticket was validated before the linear move"),
            identities,
            checkpoints,
            receipt,
        })
    }

    pub(crate) const fn source_descriptor(&self) -> crate::SourceSnapshotDescriptor {
        self.epoch.source()
    }

    pub(crate) fn build_id(&self) -> Result<ArenaBuildId, CandidateWriterError> {
        self.ticket
            .as_ref()
            .map(ArenaBuildTicket::id)
            .ok_or(CandidateWriterError::Invariant(
                "tail-ready suspended ticket is missing",
            ))
    }

    pub(crate) fn cursor_offset(&self) -> Result<usize, CandidateWriterError> {
        usize::try_from(self.composer.physical_parser_prefix_metric().bytes()).map_err(|_| {
            CandidateWriterError::Invariant("tail-ready prefix source offset exceeds usize")
        })
    }

    pub(crate) const fn receipt(&self) -> CandidateWriterTailAdoptionReceipt {
        CandidateWriterTailAdoptionReceipt {
            replayed_prefix_source_pieces: self.composer.replayed_prefix_source_pieces(),
            checkpoint_prefix_projection_runs: self.composer.checkpoint_prefix_projection_runs(),
            replayed_prefix_projection_runs: self.composer.replayed_prefix_projection_runs(),
            cumulative_prefix_projection_runs: self.composer.cumulative_prefix_projection_runs(),
            adopted_suffix_projection_runs: self.composer.adopted_suffix_projection_runs(),
            final_projection_runs: self.composer.final_projection_runs(),
            accepted_projection_prefix_metric: self.composer.accepted_projection_prefix_metric(),
            physical_parser_prefix_metric: self.composer.physical_parser_prefix_metric(),
            final_source_metric: self.composer.metric(),
            storage: self.composer.tail_adoption_receipt(),
        }
    }

    pub(crate) const fn replayed_prefix_source_pieces(&self) -> u64 {
        self.composer.replayed_prefix_source_pieces()
    }

    pub(crate) const fn replayed_prefix_projection_runs(&self) -> u64 {
        self.composer.replayed_prefix_projection_runs()
    }

    pub(crate) const fn checkpoint_prefix_projection_runs(&self) -> u64 {
        self.composer.checkpoint_prefix_projection_runs()
    }

    pub(crate) const fn cumulative_prefix_projection_runs(&self) -> u64 {
        self.composer.cumulative_prefix_projection_runs()
    }

    pub(crate) const fn adopted_suffix_projection_runs(&self) -> u64 {
        self.composer.adopted_suffix_projection_runs()
    }

    pub(crate) const fn final_projection_runs(&self) -> u64 {
        self.composer.final_projection_runs()
    }

    pub(crate) const fn final_source_metric(&self) -> SourceLedgerMetric {
        self.composer.metric()
    }

    pub(crate) fn begin_abort(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<ArenaBuildId, ArenaBuildError> {
        let ticket = self.ticket.take().ok_or(ArenaBuildError::Invariant(
            "tail-ready suspended ticket is missing",
        ))?;
        match arena.begin_build_abort(ticket) {
            Ok(build) => Ok(build),
            Err(failure) => {
                self.ticket = Some(failure.ticket);
                Err(failure.error)
            }
        }
    }

    pub(crate) fn into_identities_after_abort(
        self,
    ) -> (DocumentIdentityAllocator, CandidateWriterHeapRetirement) {
        debug_assert!(self.ticket.is_none());
        let retirement = self.donor_checkpoint_samples.into_heap_retirement();
        (self.identities, retirement)
    }

    #[cfg(test)]
    pub(crate) const fn retained_source_bytes_for_test(&self) -> usize {
        self.composer.retained_source_bytes_for_test()
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedAdoptionSpliceJob {
    /// Admits the full matched-C transaction without resuming the journal.
    /// Both green and checkpoint jobs are derived from the same parent tail;
    /// the checkpoint job's one retained-base mutation is deferred to the
    /// first actor poll so admission failure always returns a usable ticket.
    pub(crate) fn try_begin_suspended(
        arena: &PageArena,
        bundle: ParentSelectedCandidateTailSpliceBundle,
        convergence: crate::exact_block_job::ParentSelectedExactConvergenceAdoption,
    ) -> Result<ParentSelectedAdoptionSpliceStart, ParentSelectedAdoptionSpliceStartFailure> {
        let ParentSelectedCandidateTailSpliceBundle {
            epoch,
            source,
            storage,
            old_green_tail,
            builder,
            green_prefix_snapshot,
            ticket,
            identities,
            checkpoints,
            receipt,
        } = bundle;
        let (
            exact_epoch,
            parent_tail,
            old_convergence,
            certificate,
            exact_writer_receipt,
            acknowledged_lines,
        ) = convergence.into_candidate_splice_parts(ParentSelectedAdoptionSpliceMint(()));
        let ParentSelectedCandidateAdoptionTail {
            adoption,
            restart_anchor,
            green_receipt: _,
            source_receipt: _,
            reconstruction_receipt: _,
        } = parent_tail;

        let admitted = (|| -> Result<Self, CandidateWriterError> {
            if epoch != exact_epoch
                || ticket.id() != epoch.build_id()
                || adoption.build_id() != epoch.build_id()
                || source.build_id() != epoch.build_id()
                || source.source() != epoch.source()
                || storage.build_id() != epoch.build_id()
                || receipt != exact_writer_receipt
            {
                return Err(CandidateWriterError::Invariant(
                    "matched-C splice halves differ in candidate, build, source, or writer receipt",
                ));
            }

            let retained_green = adoption.green_for_convergence(&ticket, arena)?;
            let green = match crate::ResumableGreenJournalSuffixSplice::begin_from_parent(
                &ticket,
                arena,
                builder,
                green_prefix_snapshot,
                old_green_tail,
                &retained_green,
            )? {
                crate::GreenJournalSuffixAdmission::Ready(job) => job,
                crate::GreenJournalSuffixAdmission::Ineligible(_) => {
                    return Err(CandidateWriterError::Invariant(
                        "green suffix changed after the same-turn borrowed preflight",
                    ));
                }
            };
            let checkpoint_request = DonorSuffixSpliceRequest::try_from_parent_selected_writer(
                &restart_anchor,
                &old_convergence,
                certificate,
                checkpoints,
            )?;

            Ok(Self {
                epoch,
                source,
                storage,
                adoption: Some(adoption),
                green,
                checkpoint_request: Some(checkpoint_request),
                checkpoint: None,
                parent: None,
                #[cfg(feature = "host-mirror-probe")]
                host_splice: None,
                phase: ParentSelectedAdoptionSplicePhase::StartCheckpoint,
                receipt: ParentSelectedAdoptionSpliceReceipt {
                    writer: receipt,
                    acknowledged_lines,
                    green_polls: 0,
                    checkpoint_polls: 0,
                    parent_join_attempts: 0,
                    green: None,
                    checkpoint: None,
                },
            })
        })();

        match admitted {
            Ok(job) => Ok(ParentSelectedAdoptionSpliceStart {
                job,
                ticket,
                identities,
            }),
            Err(error) => Err(ParentSelectedAdoptionSpliceStartFailure {
                error,
                ticket,
                identities,
            }),
        }
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.epoch.build_id()
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> ParentSelectedAdoptionSpliceReceipt {
        self.receipt
    }

    #[must_use]
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self.phase, ParentSelectedAdoptionSplicePhase::Complete)
    }

    /// Extracts the potentially long fresh checkpoint chain before the
    /// actor drops the remaining constant-size splice state. Exactly one of
    /// request/job owns that chain at every phase.
    pub(crate) fn into_heap_retirement(self) -> CandidateWriterHeapRetirement {
        let donor = if let Some(request) = self.checkpoint_request {
            request.into_heap_retirement()
        } else if let Some(checkpoint) = self.checkpoint {
            checkpoint.into_heap_retirement()
        } else {
            crate::committed_checkpoint_index::DonorCheckpointHeapRetirement::empty()
        };
        CandidateWriterHeapRetirement::from_donor(donor)
    }

    /// Advances one bounded storage transition. Parent allocation and four
    /// owner releases are one explicit constant-size transition; retryable
    /// pre-allocation failures preserve the complete linear replacement.
    pub(crate) fn poll(
        &mut self,
        session: &mut crate::ArenaBuildSession<'_>,
    ) -> Result<ParentSelectedAdoptionSpliceProgress, CandidateWriterError> {
        if session.id() != self.epoch.build_id() {
            return Err(CandidateWriterError::Invariant(
                "adoption splice and arena session build differ",
            ));
        }
        let phase = std::mem::replace(&mut self.phase, ParentSelectedAdoptionSplicePhase::Failed);
        let result = self.poll_phase(session, phase);
        match result {
            Ok((phase, progress)) => {
                self.phase = phase;
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    fn poll_phase(
        &mut self,
        session: &mut crate::ArenaBuildSession<'_>,
        phase: ParentSelectedAdoptionSplicePhase,
    ) -> Result<
        (
            ParentSelectedAdoptionSplicePhase,
            ParentSelectedAdoptionSpliceProgress,
        ),
        CandidateWriterError,
    > {
        match phase {
            ParentSelectedAdoptionSplicePhase::StartCheckpoint => {
                let request =
                    self.checkpoint_request
                        .take()
                        .ok_or(CandidateWriterError::Invariant(
                            "adoption splice lost checkpoint request",
                        ))?;
                let adoption = self
                    .adoption
                    .as_ref()
                    .ok_or(CandidateWriterError::Invariant(
                        "adoption splice lost retained parent",
                    ))?;
                let retained = adoption.checkpoint_index_for_splice(session)?;
                self.checkpoint = Some(ParentOwnedDonorSuffixSpliceJob::try_new_from_parent(
                    session, &retained, request,
                )?);
                Ok((
                    ParentSelectedAdoptionSplicePhase::PollGreen,
                    ParentSelectedAdoptionSpliceProgress::Pending,
                ))
            }
            ParentSelectedAdoptionSplicePhase::PollGreen => {
                self.receipt.green_polls = self.receipt.green_polls.checked_add(1).ok_or(
                    CandidateWriterError::Invariant("green adoption splice poll count overflowed"),
                )?;
                let progress = self.green.poll(session)?;
                Ok((
                    if progress == crate::GreenJournalSuffixSpliceProgress::Complete {
                        self.receipt.green = Some(self.green.receipt());
                        ParentSelectedAdoptionSplicePhase::PollCheckpoint
                    } else {
                        ParentSelectedAdoptionSplicePhase::PollGreen
                    },
                    ParentSelectedAdoptionSpliceProgress::Pending,
                ))
            }
            ParentSelectedAdoptionSplicePhase::PollCheckpoint => {
                self.receipt.checkpoint_polls =
                    self.receipt.checkpoint_polls.checked_add(1).ok_or(
                        CandidateWriterError::Invariant(
                            "checkpoint adoption splice poll count overflowed",
                        ),
                    )?;
                let checkpoint =
                    self.checkpoint
                        .as_mut()
                        .ok_or(CandidateWriterError::Invariant(
                            "adoption splice lost checkpoint job",
                        ))?;
                let progress = checkpoint.poll(session)?;
                Ok((
                    if progress == DonorSuffixSpliceProgress::Complete {
                        self.receipt.checkpoint = Some(checkpoint.receipt());
                        ParentSelectedAdoptionSplicePhase::Join
                    } else {
                        ParentSelectedAdoptionSplicePhase::PollCheckpoint
                    },
                    ParentSelectedAdoptionSpliceProgress::Pending,
                ))
            }
            ParentSelectedAdoptionSplicePhase::Join => {
                let replacement = self.take_validated_replacement(session)?;
                self.try_join_replacement(session, replacement)
            }
            ParentSelectedAdoptionSplicePhase::RetryJoin(replacement) => {
                self.try_join_replacement(session, replacement)
            }
            ParentSelectedAdoptionSplicePhase::Complete => Ok((
                ParentSelectedAdoptionSplicePhase::Complete,
                ParentSelectedAdoptionSpliceProgress::Complete,
            )),
            ParentSelectedAdoptionSplicePhase::Taken => Err(CandidateWriterError::Invariant(
                "adoption splice parent was already taken",
            )),
            ParentSelectedAdoptionSplicePhase::Failed => Err(CandidateWriterError::Invariant(
                "adoption splice previously failed",
            )),
        }
    }

    fn take_validated_replacement(
        &mut self,
        session: &crate::ArenaBuildSession<'_>,
    ) -> Result<ParentSelectedRestartCompositeReplacement, CandidateWriterError> {
        let green_result = self.green.take_result()?;
        let green_receipt = green_result.receipt();
        #[cfg(feature = "host-mirror-probe")]
        let (green, host_draft, host_prefix) =
            green_result.into_parent_selected_adoption_parts(ParentSelectedAdoptionSpliceMint(()));
        #[cfg(not(feature = "host-mirror-probe"))]
        let green = green_result
            .into_parent_selected_adoption_manifest(ParentSelectedAdoptionSpliceMint(()));
        let checkpoint = self
            .checkpoint
            .as_mut()
            .ok_or(CandidateWriterError::Invariant(
                "adoption splice lost completed checkpoint job",
            ))?
            .take_manifest()?;
        let green_descriptor = green.composite_descriptor(session)?;
        let checkpoint_descriptor = checkpoint.composite_descriptor(session)?;
        let source_descriptor = self.source.source();
        let source_metric = self.source.metric();
        let source_bytes = u64::try_from(source_descriptor.bytes).map_err(|_| {
            CandidateWriterError::Invariant("adopted source byte length exceeds u64")
        })?;
        let accepted = self.source.accepted_projection_prefix_metric();
        let accepted = SerializedMetric {
            bytes: accepted.bytes(),
            utf16: accepted.utf16(),
        };
        let expected_final_measure = RelativeCheckpointMeasure::new(
            source_metric.bytes(),
            source_metric.utf16(),
            self.source.line_count(),
            green_descriptor.tokens(),
            green_descriptor.coverage_count(),
        );
        if self.source.build_id() != self.epoch.build_id()
            || source_descriptor != self.epoch.source()
            || self.storage.build_id() != self.epoch.build_id()
            || self.storage.source_before() != accepted
            || self.receipt.writer.accepted_projection_prefix_metric
                != self.source.accepted_projection_prefix_metric()
            || self.receipt.writer.physical_parser_prefix_metric
                != self.source.physical_parser_prefix_metric()
            || self.receipt.writer.final_source_metric != source_metric
            || green_descriptor.source_revision() != source_descriptor.revision
            || green_descriptor.source_root() != source_descriptor.root
            || green_descriptor.source_metric().bytes != source_bytes
            || green_descriptor.source_metric().utf16 != source_metric.utf16()
            || green_descriptor.parse_generation() != self.epoch.parse_token().generation
            || checkpoint_descriptor.final_measure() != expected_final_measure
            || green_receipt.source_tail != self.receipt.writer.storage
        {
            return Err(CandidateWriterError::Invariant(
                "completed source, green suffix, checkpoint suffix, and matched-C receipt disagree",
            ));
        }

        #[cfg(feature = "host-mirror-probe")]
        {
            // Absence remains deliberate fail-closed behavior for any restart
            // path that did not carry storage-owned retained-prefix provenance
            // through normalization and this exact matched-C tail.
            self.host_splice = host_prefix
                .map(|prefix| {
                    host_draft.finalize_matched_canonical(
                        ParentSelectedAdoptionSpliceMint(()),
                        session.arena(),
                        green.build_id(),
                        prefix,
                    )
                })
                .transpose()?;
        }

        let adoption = self.adoption.take().ok_or(CandidateWriterError::Invariant(
            "adoption splice lost retained parent before final join",
        ))?;
        let children = RestartCompositeChildren::mint_from_completed_candidate(green, checkpoint);
        Ok(ParentSelectedRestartCompositeReplacement { adoption, children })
    }

    fn try_join_replacement(
        &mut self,
        session: &mut crate::ArenaBuildSession<'_>,
        replacement: ParentSelectedRestartCompositeReplacement,
    ) -> Result<
        (
            ParentSelectedAdoptionSplicePhase,
            ParentSelectedAdoptionSpliceProgress,
        ),
        CandidateWriterError,
    > {
        self.receipt.parent_join_attempts =
            self.receipt.parent_join_attempts.checked_add(1).ok_or(
                CandidateWriterError::Invariant("adoption parent join attempt count overflowed"),
            )?;
        match RestartCompositeDocumentBuilder::join_adopted_candidate(session, replacement) {
            Ok(parent) => {
                self.parent = Some(parent);
                Ok((
                    ParentSelectedAdoptionSplicePhase::Complete,
                    ParentSelectedAdoptionSpliceProgress::Complete,
                ))
            }
            Err(crate::storage_only_composite_document::RestartCompositeReplacementJoinFailure::Retryable {
                error,
                replacement,
            }) => Ok((
                ParentSelectedAdoptionSplicePhase::RetryJoin(replacement),
                ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error),
            )),
            Err(crate::storage_only_composite_document::RestartCompositeReplacementJoinFailure::AbortRequired {
                error,
                build: _,
            }) => Err(error.into()),
        }
    }

    pub(crate) fn take_parent_manifest(
        &mut self,
    ) -> Result<
        crate::storage_only_composite_document::RestartCompositeDocumentBuildManifest,
        CandidateWriterError,
    > {
        if !matches!(self.phase, ParentSelectedAdoptionSplicePhase::Complete) {
            return Err(CandidateWriterError::Invariant(
                "adoption splice parent is not complete",
            ));
        }
        let parent = self.parent.take().ok_or(CandidateWriterError::Invariant(
            "adoption splice lost its completed parent",
        ))?;
        self.phase = ParentSelectedAdoptionSplicePhase::Taken;
        Ok(parent)
    }

    /// Borrows the final actor-minted proof to prepare a host-owned delta
    /// while the completed parent and its arena pages are still jointly
    /// retained. The proof never becomes a scalar test fixture or leaves this
    /// matched-C job.
    #[cfg(feature = "host-mirror-probe")]
    pub(crate) fn prepare_completed_host_delta_bundle(
        &self,
        arena: &PageArena,
        base: crate::host_mirror::StructuralAck,
        target: crate::host_mirror::HostRevisionId,
        publication_session: crate::host_mirror::PublicationSessionId,
        source: crate::host_mirror::SourceVersion,
    ) -> Result<crate::host_mirror::StructuralBundle, crate::host_mirror::HostMirrorError> {
        if !matches!(self.phase, ParentSelectedAdoptionSplicePhase::Complete) {
            return Err(crate::host_mirror::HostMirrorError::Invalid(
                "host delta requires a completed matched-C adoption",
            ));
        }
        let proof =
            self.host_splice
                .as_ref()
                .ok_or(crate::host_mirror::HostMirrorError::Invalid(
                    "restart mode has no final host publication authority",
                ))?;
        crate::host_mirror::prepare_typed_leaf_delta_bundle(
            arena,
            proof,
            base,
            target,
            publication_session,
            source,
        )
    }

    #[cfg(all(test, feature = "host-mirror-probe"))]
    pub(crate) fn host_splice_range_counts_for_test(&self) -> Option<(u64, u64, u64, u64)> {
        if !matches!(self.phase, ParentSelectedAdoptionSplicePhase::Complete) {
            return None;
        }
        self.host_splice
            .as_ref()
            .map(crate::host_mirror::TypedGreenLeafSplice::range_counts_for_test)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl CandidateWriterBuiltDocument {
    /// Queryable packed green output for this grammar-free feasibility commit.
    /// It is not a coordinator-published composite parser manifest.
    #[must_use]
    pub(crate) const fn green_document(&self) -> &SerializedGreenDocument {
        #[cfg(feature = "exact-parser")]
        {
            self.composite.green_document()
        }
        #[cfg(not(feature = "exact-parser"))]
        {
            &self.green
        }
    }

    #[must_use]
    pub(crate) const fn source(&self) -> crate::SourceSnapshotDescriptor {
        self.composer.source()
    }

    #[must_use]
    pub(crate) const fn source_metric(&self) -> crate::SourceLedgerMetric {
        self.composer.metric()
    }

    #[must_use]
    pub(crate) const fn composer_receipt(&self) -> crate::SourceProjectionComposerReceipt {
        self.composer.receipt()
    }

    #[must_use]
    pub(crate) const fn green_receipt(&self) -> SerializedGreenBuildReceipt {
        self.green_receipt
    }

    #[must_use]
    pub(crate) const fn green_runs_acknowledged(&self) -> u64 {
        self.green_runs_acknowledged
    }

    #[cfg(all(test, feature = "exact-parser"))]
    pub(crate) const fn reference_composite_receipt_for_test(
        &self,
    ) -> (u64, u64, usize, usize) {
        let receipt = self.composite.receipt();
        (
            receipt.reference.occurrences_acknowledged(),
            receipt.reference.exact_labels(),
            receipt.child_references_added,
            receipt.live_owners_after_join,
        )
    }

    /// Finalizes the retained Setext normalization recipe only after joining
    /// the checkpoint draft back to this exact completed writer/composer
    /// build. A green document supplied independently cannot stand in for the
    /// old candidate that minted the pause.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn seal_in_memory_setext_normalization(
        &self,
        arena: &PageArena,
        source: &RetainedSetextSourceLedgerDraft,
        green: &RetainedSetextGreenCheckpointDraft,
        final_heading: GreenHeadingOpenFacts,
    ) -> Result<SealedSetextNormalizationManifest, CandidateWriterError> {
        let old_epoch = source.old_epoch();
        let accepted = green.accepted_source_cut();
        let descriptor = self.green_document().manifest_descriptor(arena)?;
        if self.composer.build_id() != old_epoch.build_id()
            || self.composer.source() != old_epoch.source()
            || self.composer.source() != source.descriptor()
            || green.old_build() != self.composer.build_id()
            || green.block() != source.terminal_block()?
            || accepted.bytes != source.accepted_bytes()
            || accepted.utf16 != source.accepted_utf16()
            || descriptor.source_revision != old_epoch.source().revision
            || descriptor.source_root != old_epoch.source().root
            || descriptor.parse_generation != old_epoch.parse_token().generation
        {
            return Err(CandidateWriterError::Invariant(
                "retained Setext draft belongs to another completed candidate",
            ));
        }
        self.green_document()
            .seal_setext_normalization_from_joined_checkpoint(arena, green, final_heading)
            .map_err(Into::into)
    }
}

/// Sole owner of the grammar-free candidate composition path.
#[derive(Debug)]
pub(crate) struct CandidateWriter {
    epoch: LiveCandidateEpoch,
    ledger: CandidateSourceLedger,
    composer: Option<SourceBoundProjectionComposer>,
    builder: Option<ResumableSerializedGreenBuild>,
    lease: CandidateWriterBuildLease,
    identities: DocumentIdentityAllocator,
    action: Option<WriterAction>,
    issued_source_transition: Option<u64>,
    next_source_transition: u64,
    green_runs_acknowledged: u64,
    #[cfg(feature = "exact-parser")]
    donor_checkpoint_samples: DonorCheckpointSampleAccumulator,
    #[cfg(feature = "exact-parser")]
    reference_semantic: Option<CandidateReferenceSemanticTransaction>,
    completion: Option<CandidateWriterCompletionSeal>,
    active_paragraph: Option<ActiveParagraphNormalizationGroup>,
    deferred_normalization: Option<PendingDeferredNormalization>,
    active_fenced_code: Option<ActiveFencedCodeProjectionFold>,
    completion_route: CandidateWriterCompletionRoute,
    #[cfg(feature = "exact-parser")]
    last_line_boundary_storage: Option<SourceProjectionLineBoundaryStorageAck>,
    poisoned: bool,
    #[cfg(test)]
    fail_after_green_ack_before_ledger_close: bool,
    #[cfg(test)]
    fail_after_setext_green_ack_before_ledger_retype: bool,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedCandidateWriterRestartProgress {
    Pending,
    Ready,
}

#[cfg(feature = "exact-parser")]
enum ParentSelectedCandidateGreenRestart {
    Direct(ParentSelectedDirectRetainedGreenRestart),
    Setext(ParentSelectedSetextRetainedGreenRestart),
}

/// Whole source-to-writer restart state after exact parent selection and donor
/// resume. Construction is possible only through the private mint held by the
/// source-ledger module; no crate-wide tuple can separate or recombine its
/// ledger, bindings, parser, cumulative composer base, or branded green role.
#[cfg(feature = "exact-parser")]
#[must_use = "the parent-selected writer restart must be polled, installed, or cancelled"]
pub(crate) struct ParentSelectedCandidateWriterRestart {
    epoch: LiveCandidateEpoch,
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateWriterBinding>,
    parser: DirectValueBlockParser,
    acknowledged_lines: u64,
    restart_anchor: ParentSelectedRestartAnchor,
    composer_coverage: crate::ParentSelectedComposerCoverage,
    green: ParentSelectedCandidateGreenRestart,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
    reconstruction_receipt: crate::PersistedSourceLedgerReconstructionReceipt,
}

/// Green prefix after one complete parent-selected output wrapper has been
/// atomically joined to the source-derived cumulative composer base. The
/// branded lease remains embedded until `Prepared::install` moves it into the
/// actor-owned driver carrier.
#[cfg(feature = "exact-parser")]
pub(crate) struct ParentSelectedCandidateWriterGreenReady {
    lease: ParentSelectedRestartCompositeAdoptionLease,
    builder: ResumableSerializedGreenBuild,
    composer: SourceBoundProjectionComposer,
    storage: SourceProjectionLineBoundaryStorageAck,
    provisional: Option<ProvisionalParagraphEnter>,
    projection_origin: Option<CanonicalFragmentProjectionOrigin>,
    receipt: SetextRetainedGreenRestartReceipt,
    terminal_block: BlockId,
    terminal_kind: GreenKind,
}

/// Last fallible retained-green/source join output. It deliberately owns no
/// ticket or identity allocator; actor suspension happens after this value is
/// produced, and the final install is then an infallible linear move.
#[cfg(feature = "exact-parser")]
#[must_use = "the prepared parent-selected writer must be installed or the candidate aborted"]
pub(crate) struct PreparedParentSelectedCandidateWriter {
    epoch: LiveCandidateEpoch,
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateWriterBinding>,
    parser: DirectValueBlockParser,
    acknowledged_lines: u64,
    restart_anchor: ParentSelectedSeededRestartAnchor,
    suffix_origin: ParentSelectedSuffixSampleOrigin,
    green: ParentSelectedCandidateWriterGreenReady,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
    reconstruction_receipt: crate::PersistedSourceLedgerReconstructionReceipt,
}

/// Actor-owned exact-driver carrier. The scheduler may receive only a copyable
/// activation handle; parser/bindings and the branded parent adoption lease
/// stay together here through checkpoint-index splice and final composite
/// adoption.
#[cfg(feature = "exact-parser")]
#[must_use = "the retained restart driver must remain actor-owned until adoption or abort"]
pub(crate) struct ParentSelectedCandidateWriterDriver {
    epoch: LiveCandidateEpoch,
    parser: DirectValueBlockParser,
    bindings: Vec<CandidateWriterBinding>,
    acknowledged_lines: u64,
    tail: ParentSelectedCandidateAdoptionTail,
}

/// Opaque actor-owned authority that must survive exact suffix driving. The
/// retained parent lease has no extractor: only the eventual splice/adoption
/// transition may consume this whole tail. Copyable methods are diagnostics,
/// never publication authority.
#[cfg(feature = "exact-parser")]
#[must_use = "the retained parent tail must reach splice/adoption or cancellation"]
#[derive(Debug)]
pub(crate) struct ParentSelectedCandidateAdoptionTail {
    adoption: ParentSelectedRestartCompositeAdoptionLease,
    restart_anchor: ParentSelectedSeededRestartAnchor,
    green_receipt: SetextRetainedGreenRestartReceipt,
    source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
    reconstruction_receipt: crate::PersistedSourceLedgerReconstructionReceipt,
}

/// Source, reminted bindings, donor cursor, and restored green prefix after a
/// single consuming activation preflight. Keeping them together prevents an
/// internal caller from substituting bindings after validation.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ValidatedRetainedSetextSourceActivation {
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateWriterBinding>,
    donor_cursor: flark_comrak_value_block_core::DirectLineBoundaryResumeCursor,
    acknowledged_lines: u64,
    accepted: crate::SerializedMetric,
    block: crate::BlockId,
}

/// Same joined activation after the exact located donor recipe has resumed
/// against the current source-derived cursor. The subsequent writer install
/// performs no allocation or recoverable validation.
#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Completed narrow proof carrier; production wiring follows the composite root.
pub(crate) struct DonorResumedRetainedSetextSourceActivation {
    ledger: CandidateSourceLedger,
    bindings: Vec<CandidateWriterBinding>,
    parser: DirectValueBlockParser,
    acknowledged_lines: u64,
    accepted: crate::SerializedMetric,
    block: crate::BlockId,
}

#[cfg(feature = "exact-parser")]
#[allow(dead_code)] // Completed narrow proof carrier; production wiring follows the composite root.
pub(crate) struct DonorResumedRetainedSetextActivation {
    source: DonorResumedRetainedSetextSourceActivation,
    green: SetextRetainedGreenRestartOutput,
}

#[cfg(feature = "exact-parser")]
pub(crate) struct RetainedSetextDriverActivation {
    parser: DirectValueBlockParser,
    bindings: Vec<CandidateWriterBinding>,
    acknowledged_lines: u64,
}

#[cfg(feature = "exact-parser")]
impl RetainedSetextDriverActivation {
    pub(crate) fn into_parts(self) -> (DirectValueBlockParser, Vec<CandidateWriterBinding>, u64) {
        (self.parser, self.bindings, self.acknowledged_lines)
    }
}

#[cfg(feature = "exact-parser")]
impl ValidatedRetainedSetextSourceActivation {
    pub(crate) fn resume_donor(
        self,
        donor: WitnessValidatedSetextDonorRecipe,
    ) -> Result<DonorResumedRetainedSetextSourceActivation, ParseError> {
        let parser = donor.resume_donor(self.donor_cursor)?;
        Ok(DonorResumedRetainedSetextSourceActivation {
            ledger: self.ledger,
            bindings: self.bindings,
            parser,
            acknowledged_lines: self.acknowledged_lines,
            accepted: self.accepted,
            block: self.block,
        })
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedCandidateWriterGreenReady {
    /// Storage-output-only constructor. The mint is constructible by the
    /// serialized-green module and its descendants, never by an arbitrary
    /// source/parser caller with copied coordinates.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_from_parent_green_mint(
        _mint: crate::serialized_green::ParentSelectedCandidateGreenReadyMint,
        epoch: LiveCandidateEpoch,
        lease: ParentSelectedRestartCompositeAdoptionLease,
        builder: ResumableSerializedGreenBuild,
        provisional: Option<ProvisionalParagraphEnter>,
        line_cut: crate::SerializedGreenLeafCut,
        receipt: SetextRetainedGreenRestartReceipt,
        coverage: crate::ParentSelectedComposerCoverage,
        fragment_origin: Option<ParentSelectedCanonicalFragmentOriginSeed>,
        terminal_block: BlockId,
        terminal_kind: GreenKind,
    ) -> Result<Self, CandidateWriterError> {
        if lease.build_id() != epoch.build_id()
            || builder.build_id() != epoch.build_id()
            || terminal_block.0 == 0
            || (terminal_kind == GreenKind::PARAGRAPH) != provisional.is_some()
            || provisional.is_some() != fragment_origin.is_some()
            || provisional.as_ref().is_some_and(|provisional| {
                !builder.retained_provisional_matches(provisional, terminal_block)
            })
        {
            return Err(CandidateWriterError::Invariant(
                "parent-selected green output disagrees with writer source path",
            ));
        }
        if let (Some(seed), Some(provisional)) = (fragment_origin.as_ref(), provisional.as_ref())
            && !seed.matches_parent_selected_join(epoch, &coverage, provisional, &line_cut)
        {
            return Err(CandidateWriterError::Invariant(
                "parent-selected fragment origin disagrees with its source/green cut",
            ));
        }
        let storage = SourceProjectionLineBoundaryStorageAck::from_green_cut(epoch, line_cut)?;
        let (mut composer, storage) =
            SourceBoundProjectionComposer::begin_parent_selected_line_boundary(
                epoch, storage, coverage,
            )?;
        let projection_origin = fragment_origin
            .map(|seed| composer.restore_parent_selected_canonical_fragment_origin(seed))
            .transpose()?;
        Ok(Self {
            lease,
            builder,
            composer,
            storage,
            provisional,
            projection_origin,
            receipt,
            terminal_block,
            terminal_kind,
        })
    }

    fn matches_source_terminal(&self, block: BlockId, kind: GreenKind) -> bool {
        block == self.terminal_block
            && kind == self.terminal_kind
            && block.0 != 0
            && (kind == GreenKind::PARAGRAPH) == self.provisional.is_some()
            && self.provisional.as_ref().is_none_or(|provisional| {
                self.builder
                    .retained_provisional_matches(provisional, block)
            })
    }
}

#[cfg(feature = "exact-parser")]
impl PreparedParentSelectedCandidateWriter {
    /// Final infallible move after every parent/source/green/composer check and
    /// after the actor has suspended the same journal to this exact ticket.
    pub(crate) fn install(
        self,
        ticket: ArenaBuildTicket,
        identities: DocumentIdentityAllocator,
    ) -> (CandidateWriter, ParentSelectedCandidateWriterDriver) {
        let Self {
            epoch,
            ledger,
            bindings,
            parser,
            acknowledged_lines,
            restart_anchor,
            suffix_origin,
            green,
            source_receipt,
            reconstruction_receipt,
        } = self;
        let ParentSelectedCandidateWriterGreenReady {
            lease,
            builder,
            composer,
            storage,
            provisional,
            projection_origin,
            receipt,
            terminal_block: restored_terminal_block,
            terminal_kind: restored_terminal_kind,
        } = green;
        assert_eq!(ticket.id(), epoch.build_id());
        let terminal = bindings
            .last()
            .expect("parent-selected restart has a validated nonempty path");
        let terminal_block = terminal.binding.block_id();
        let terminal_kind = terminal.kind();
        assert_eq!(terminal_block, restored_terminal_block);
        assert_eq!(terminal_kind, restored_terminal_kind);
        let mut donor_checkpoint_samples =
            DonorCheckpointSampleAccumulator::after_parent_selected_prefix(suffix_origin);
        let active_paragraph = provisional.map(|provisional| {
            assert_eq!(terminal_kind, GreenKind::PARAGRAPH);
            assert!(builder.retained_provisional_matches(&provisional, terminal_block));
            donor_checkpoint_samples
                .begin_paragraph_group(terminal_block)
                .expect("validated retained Paragraph starts one writer-owned sample group");
            ActiveParagraphNormalizationGroup {
                build: epoch.build_id(),
                block: terminal_block,
                enter: Some(provisional),
                projection_origin,
                promoted_setext: false,
                deferred_identity: None,
                deferred_storage: None,
            }
        });
        let writer = CandidateWriter {
            epoch,
            ledger,
            composer: Some(composer),
            builder: Some(builder),
            lease: CandidateWriterBuildLease::Suspended(ticket),
            identities,
            action: None,
            issued_source_transition: None,
            next_source_transition: 1,
            green_runs_acknowledged: 0,
            donor_checkpoint_samples,
            reference_semantic: None,
            completion: None,
            active_paragraph,
            deferred_normalization: None,
            active_fenced_code: None,
            completion_route: CandidateWriterCompletionRoute::ParentSelectedAdoption,
            last_line_boundary_storage: Some(storage),
            poisoned: false,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close: false,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype: false,
        };
        let driver = ParentSelectedCandidateWriterDriver {
            epoch,
            parser,
            bindings,
            acknowledged_lines,
            tail: ParentSelectedCandidateAdoptionTail {
                adoption: lease,
                restart_anchor,
                green_receipt: receipt,
                source_receipt,
                reconstruction_receipt,
            },
        };
        (writer, driver)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedCandidateWriterDriver {
    #[must_use]
    pub(crate) const fn acknowledged_lines(&self) -> u64 {
        self.acknowledged_lines
    }

    #[must_use]
    pub(crate) fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.tail.build_id()
    }

    #[must_use]
    pub(crate) const fn green_receipt(&self) -> SetextRetainedGreenRestartReceipt {
        self.tail.green_receipt()
    }

    #[must_use]
    pub(crate) const fn source_receipt(
        &self,
    ) -> crate::retained_restart_coordinate::PersistedRestartSourceReceipt {
        self.tail.source_receipt()
    }

    #[must_use]
    pub(crate) const fn reconstruction_receipt(
        &self,
    ) -> crate::PersistedSourceLedgerReconstructionReceipt {
        self.tail.reconstruction_receipt()
    }

    /// One-way exact-driver handoff. The raw parser and reminted bindings can
    /// cross the module boundary only when `exact_block_job` supplies its
    /// private-field mint; the branded retained-parent authority remains
    /// opaque in the returned tail.
    pub(crate) fn into_exact_driver_parts(
        self,
        _mint: crate::exact_block_job::ParentSelectedExactBlockDriverMint,
    ) -> (
        LiveCandidateEpoch,
        DirectValueBlockParser,
        Vec<CandidateWriterBinding>,
        u64,
        ParentSelectedCandidateAdoptionTail,
    ) {
        let Self {
            epoch,
            parser,
            bindings,
            acknowledged_lines,
            tail,
        } = self;
        (epoch, parser, bindings, acknowledged_lines, tail)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedCandidateAdoptionTail {
    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.adoption.build_id()
    }

    #[must_use]
    pub(crate) const fn green_receipt(&self) -> SetextRetainedGreenRestartReceipt {
        self.green_receipt
    }

    #[must_use]
    pub(crate) const fn source_receipt(
        &self,
    ) -> crate::retained_restart_coordinate::PersistedRestartSourceReceipt {
        self.source_receipt
    }

    #[must_use]
    pub(crate) const fn reconstruction_receipt(
        &self,
    ) -> crate::PersistedSourceLedgerReconstructionReceipt {
        self.reconstruction_receipt
    }

    fn begin_old_convergence(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        let retained = self
            .adoption
            .checkpoint_index_for_convergence(ticket, arena)?;
        retained
            .begin_parent_bound_donor_successor(ticket, arena, &self.restart_anchor)
            .map_err(Into::into)
    }

    fn advance_old_convergence(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        current: ParentBoundDonorSuccessor,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        let retained = self
            .adoption
            .checkpoint_index_for_convergence(ticket, arena)?;
        retained
            .advance_parent_bound_donor_successor(ticket, arena, current)
            .map_err(Into::into)
    }

    fn advance_old_convergence_partition(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        transition: ParentBoundDonorPartitionTransition,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        let retained = self
            .adoption
            .checkpoint_index_for_convergence(ticket, arena)?;
        retained
            .advance_parent_bound_donor_partition(ticket, arena, transition)
            .map_err(Into::into)
    }

    /// Starts the immutable source-lineage mapping only after old C has been
    /// rebound to this exact retained R. The old source descriptor is derived
    /// from the same composite lease; callers supply neither descriptor nor
    /// R/C coordinates.
    pub(crate) fn begin_convergence_mapping(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        source: &SourceStore,
        epoch: LiveCandidateEpoch,
        old_convergence: ParentBoundDonorSuccessor,
    ) -> Result<ParentSelectedConvergenceMapStart, ParentSelectedConvergenceMapError> {
        let bound = self.restart_anchor.bind_old_convergence(old_convergence)?;
        let retained_green = self.adoption.green_for_convergence(ticket, arena)?;
        let storage = retained_green
            .source_tail_adoption_capability_for_parent_convergence(ticket, arena, &bound)?;
        let metric = self.adoption.source_metric();
        let expected_source = crate::SourceSnapshotDescriptor {
            revision: self.adoption.source_revision(),
            root: self.adoption.source_root(),
            bytes: usize::try_from(metric.bytes).map_err(|_| {
                ParentSelectedConvergenceMapError::Overflow("retained parent source bytes")
            })?,
        };
        if storage.old_source() != expected_source {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "retained green convergence source differs from its composite parent",
            ));
        }
        ParentSelectedConvergenceMapJob::begin(source, epoch, bound, storage)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedCandidateWriterRestart {
    /// Lexically restricted source-module handoff. The zero-sized mint cannot
    /// be constructed outside `source_bound_ledger`, so these raw arguments are
    /// not a second composition surface.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_from_source_module_mint(
        _mint: crate::source_bound_ledger::ParentSelectedCandidateWriterMint,
        ledger: CandidateSourceLedger,
        open_bindings: Vec<CandidateOpenBinding>,
        parser: DirectValueBlockParser,
        acknowledged_lines: u64,
        restart_anchor: ParentSelectedRestartAnchor,
        composer_coverage: crate::ParentSelectedComposerCoverage,
        green: crate::ParentSelectedPersistedGreenActivation,
        source_receipt: crate::retained_restart_coordinate::PersistedRestartSourceReceipt,
        reconstruction_receipt: crate::PersistedSourceLedgerReconstructionReceipt,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        config: CandidateWriterConfig,
    ) -> Result<Self, CandidateWriterError> {
        let epoch = composer_coverage.epoch();
        if ticket.id() != epoch.build_id()
            || epoch.arena_identity() != arena.identity()
            || ledger.descriptor() != epoch.source()
            || green.build_id() != epoch.build_id()
            || composer_coverage.accepted_source().bytes == 0
            || composer_coverage.accepted_source().utf16 == 0
            || composer_coverage.event_cut() == 0
            || composer_coverage.projection_runs() == 0
            || acknowledged_lines == 0
            || open_bindings.is_empty()
            || open_bindings[0].kind() != GreenKind::DOCUMENT
            || open_bindings
                .iter()
                .any(|binding| binding.block_id().0 == 0)
            || reconstruction_receipt.path_frames_consumed != open_bindings.len()
        {
            return Err(CandidateWriterError::Invariant(
                "parent-selected source cannot enter CandidateWriter",
            ));
        }
        if open_bindings.last().map(CandidateOpenBinding::kind) == Some(GreenKind::FENCED_CODE) {
            return Err(CandidateWriterError::Invariant(
                "open FencedCode restart requires persisted typed projection boundaries",
            ));
        }
        let expected_spec = CandidateWriter::root_spec(epoch, &ledger, config)?;
        let green = match green {
            crate::ParentSelectedPersistedGreenActivation::Direct { lease, authority } => {
                ParentSelectedCandidateGreenRestart::Direct(
                    ParentSelectedDirectRetainedGreenRestart::try_new(
                        ticket,
                        arena,
                        lease,
                        authority,
                        expected_spec,
                    )
                    .map_err(map_parent_selected_direct_green_restart_error)?,
                )
            }
            crate::ParentSelectedPersistedGreenActivation::Setext { lease, inverse } => {
                ParentSelectedCandidateGreenRestart::Setext(
                    ParentSelectedSetextRetainedGreenRestart::try_new(
                        ticket,
                        arena,
                        lease,
                        inverse,
                        expected_spec,
                    )
                    .map_err(map_parent_selected_green_restart_error)?,
                )
            }
        };
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(open_bindings.len())
            .map_err(|_| CandidateWriterError::Invariant("restart binding reservation failed"))?;
        bindings.extend(
            open_bindings
                .into_iter()
                .map(|binding| CandidateWriterBinding { binding }),
        );
        Ok(Self {
            epoch,
            ledger,
            bindings,
            parser,
            acknowledged_lines,
            restart_anchor,
            composer_coverage,
            green,
            source_receipt,
            reconstruction_receipt,
        })
    }

    #[must_use]
    pub(crate) const fn source_descriptor(&self) -> crate::SourceSnapshotDescriptor {
        self.epoch.source()
    }

    #[must_use]
    pub(crate) fn cursor_offset(&self) -> usize {
        self.ledger.cursor_offset()
    }

    #[must_use]
    pub(crate) const fn source_receipt(
        &self,
    ) -> crate::retained_restart_coordinate::PersistedRestartSourceReceipt {
        self.source_receipt
    }

    #[must_use]
    pub(crate) const fn reconstruction_receipt(
        &self,
    ) -> crate::PersistedSourceLedgerReconstructionReceipt {
        self.reconstruction_receipt
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut crate::ArenaBuildSession<'_>,
    ) -> Result<ParentSelectedCandidateWriterRestartProgress, CandidateWriterError> {
        if session.id() != self.epoch.build_id() {
            return Err(CandidateWriterError::WrongCandidate);
        }
        let progress = match &mut self.green {
            ParentSelectedCandidateGreenRestart::Direct(restart) => restart
                .poll(session)
                .map_err(map_parent_selected_direct_green_restart_error)?,
            ParentSelectedCandidateGreenRestart::Setext(restart) => restart
                .poll(session)
                .map_err(map_parent_selected_green_restart_error)?,
        };
        Ok(match progress {
            SetextRetainedGreenRestartProgress::Pending => {
                ParentSelectedCandidateWriterRestartProgress::Pending
            }
            SetextRetainedGreenRestartProgress::Ready => {
                ParentSelectedCandidateWriterRestartProgress::Ready
            }
        })
    }

    /// Performs the last fallible source/green/composer join while the actor
    /// still owns the resumed session. Any error consumes this complete job and
    /// leaves the caller only the session-wide abort transition; no retained
    /// parent role or partially prepared writer escapes.
    pub(crate) fn take_output(
        self,
        session: &crate::ArenaBuildSession<'_>,
    ) -> Result<PreparedParentSelectedCandidateWriter, CandidateWriterError> {
        if session.id() != self.epoch.build_id() {
            return Err(CandidateWriterError::WrongCandidate);
        }
        let Self {
            epoch,
            ledger,
            bindings,
            parser,
            acknowledged_lines,
            restart_anchor,
            composer_coverage,
            green,
            source_receipt,
            reconstruction_receipt,
        } = self;
        let terminal = bindings.last().ok_or(CandidateWriterError::Invariant(
            "parent-selected restart lost its terminal source binding",
        ))?;
        let terminal_block = terminal.binding.block_id();
        let terminal_kind = terminal.kind();
        let physical = ledger.physical_metric();
        let writer_restart_cut = RelativeCheckpointMeasure::new(
            physical.bytes(),
            physical.utf16(),
            acknowledged_lines,
            composer_coverage.event_cut(),
            composer_coverage.projection_runs(),
        );
        let green = match green {
            ParentSelectedCandidateGreenRestart::Direct(restart) => restart
                .take_output(session)
                .map_err(map_parent_selected_direct_green_restart_error)?
                .into_parent_selected_candidate_ready(epoch, composer_coverage)?,
            ParentSelectedCandidateGreenRestart::Setext(restart) => restart
                .take_output(session)
                .map_err(map_parent_selected_green_restart_error)?
                .into_parent_selected_candidate_ready(epoch, composer_coverage)?,
        };
        if !green.matches_source_terminal(terminal_block, terminal_kind) {
            return Err(CandidateWriterError::Invariant(
                "parent-selected retained green and source terminal disagree",
            ));
        }
        let (restart_anchor, suffix_origin) = restart_anchor
            .try_seed_suffix_samples(epoch, writer_restart_cut)
            .map_err(CandidateWriterError::CheckpointIndex)?;
        Ok(PreparedParentSelectedCandidateWriter {
            epoch,
            ledger,
            bindings,
            parser,
            acknowledged_lines,
            restart_anchor,
            suffix_origin,
            green,
            source_receipt,
            reconstruction_receipt,
        })
    }

    pub(crate) fn cancel(
        self,
        session: crate::ArenaBuildSession<'_>,
    ) -> Result<ArenaBuildId, RestartCompositeDocumentError> {
        if session.id() != self.epoch.build_id() {
            return Err(RestartCompositeDocumentError::Invalid(
                "parent-selected writer restart and cancellation build differ",
            ));
        }
        drop(self);
        session.begin_abort().map_err(Into::into)
    }
}

#[cfg(feature = "exact-parser")]
fn map_parent_selected_green_restart_error(
    error: ParentSelectedSetextRetainedGreenRestartError,
) -> CandidateWriterError {
    match error {
        ParentSelectedSetextRetainedGreenRestartError::Parent(error) => {
            CandidateWriterError::RestartComposite(error)
        }
        ParentSelectedSetextRetainedGreenRestartError::Green(error) => {
            CandidateWriterError::Green(error)
        }
    }
}

#[cfg(feature = "exact-parser")]
fn map_parent_selected_direct_green_restart_error(
    error: ParentSelectedDirectRetainedGreenRestartError,
) -> CandidateWriterError {
    match error {
        ParentSelectedDirectRetainedGreenRestartError::Parent(error) => {
            CandidateWriterError::RestartComposite(error)
        }
        ParentSelectedDirectRetainedGreenRestartError::Green(error) => {
            CandidateWriterError::Green(error)
        }
    }
}

impl CandidateWriter {
    pub(crate) fn new(
        epoch: LiveCandidateEpoch,
        ledger: CandidateSourceLedger,
        ticket: ArenaBuildTicket,
        identities: DocumentIdentityAllocator,
        builder: ResumableSerializedGreenBuild,
    ) -> Result<Self, CandidateWriterError> {
        if ledger.descriptor() != epoch.source()
            || builder.build_id() != epoch.build_id()
            || ticket.id() != epoch.build_id()
        {
            return Err(CandidateWriterError::WrongCandidate);
        }
        Ok(Self {
            epoch,
            ledger,
            composer: Some(SourceBoundProjectionComposer::begin(epoch)),
            builder: Some(builder),
            lease: CandidateWriterBuildLease::Suspended(ticket),
            identities,
            action: None,
            issued_source_transition: None,
            next_source_transition: 1,
            green_runs_acknowledged: 0,
            #[cfg(feature = "exact-parser")]
            donor_checkpoint_samples: DonorCheckpointSampleAccumulator::from_document_origin(),
            #[cfg(feature = "exact-parser")]
            reference_semantic: None,
            completion: None,
            active_paragraph: None,
            deferred_normalization: None,
            active_fenced_code: None,
            completion_route: CandidateWriterCompletionRoute::Independent,
            #[cfg(feature = "exact-parser")]
            last_line_boundary_storage: None,
            poisoned: false,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close: false,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype: false,
        })
    }

    pub(crate) fn root_spec(
        epoch: LiveCandidateEpoch,
        ledger: &CandidateSourceLedger,
        config: CandidateWriterConfig,
    ) -> Result<SerializedGreenRootSpec, CandidateWriterError> {
        Self::root_spec_from_source(epoch, ledger.authoritative_root_utf16(), config)
    }

    pub(crate) fn root_spec_from_source(
        epoch: LiveCandidateEpoch,
        source_utf16: u64,
        config: CandidateWriterConfig,
    ) -> Result<SerializedGreenRootSpec, CandidateWriterError> {
        if config.syntax_profile == 0
            || config.grammar_revision.0 == 0
            || config.semantic_epoch == 0
        {
            return Err(CandidateWriterError::Invariant(
                "candidate writer root generations must be nonzero",
            ));
        }
        let source_bytes = u64::try_from(epoch.source().bytes)
            .map_err(|_| CandidateWriterError::Invariant("source bytes exceed u64"))?;
        Ok(SerializedGreenRootSpec {
            syntax_profile: config.syntax_profile,
            source_revision: epoch.source().revision,
            source_root: epoch.source().root,
            source_bytes,
            source_utf16,
            grammar_revision: config.grammar_revision,
            parse_generation: epoch.parse_token().generation,
            semantic_epoch: config.semantic_epoch,
            known_bytes: 0..source_bytes,
        })
    }

    /// Preflights every retained Setext writer join while the actor still owns
    /// its suspended ticket and identity allocator. The consuming constructor
    /// below is called only after donor resume also succeeds, so no fallible
    /// provenance check can orphan those linear capabilities.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn validate_retained_setext_source_activation(
        epoch: LiveCandidateEpoch,
        source: RestoredSetextSourceLedger,
        ticket: &ArenaBuildTicket,
    ) -> Result<ValidatedRetainedSetextSourceActivation, CandidateWriterError> {
        let accepted = source.accepted_projection_metric()?;
        let block = source.terminal_block()?;
        if ticket.id() != epoch.build_id()
            || source.descriptor() != epoch.source()
            || source.binding_count() != 2
            || source.terminal_kind()? != GreenKind::PARAGRAPH
            || source.completed_line_ordinal() == 0
        {
            return Err(CandidateWriterError::Invariant(
                "retained Setext source/ticket activation join failed",
            ));
        }
        // Reserve the final driver-visible wrapper vector before ticket or
        // identity ownership can leave the actor. The source wrapper is then
        // consumed exactly once, closing any ledger/binding substitution seam.
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(source.binding_count())
            .map_err(|_| {
                CandidateWriterError::Invariant("retained Setext binding reservation failed")
            })?;
        let acknowledged_lines = source.completed_line_ordinal();
        #[cfg(test)]
        {
            let (physical, lines, claims, digest, terminal_logical) =
                source.suffix_local_receipt_for_test();
            assert_eq!(physical.bytes(), accepted.bytes() + 1);
            assert_eq!(physical.utf16(), accepted.utf16() + 1);
            assert_eq!(lines, acknowledged_lines);
            assert_eq!(claims, 0);
            assert_eq!(digest, 0xcbf2_9ce4_8422_2325);
            assert_eq!(terminal_logical, accepted);
        }
        let (ledger, open_bindings, donor_cursor) = source.into_parts();
        bindings.extend(
            open_bindings
                .into_iter()
                .map(|binding| CandidateWriterBinding { binding }),
        );
        Ok(ValidatedRetainedSetextSourceActivation {
            ledger,
            bindings,
            donor_cursor,
            acknowledged_lines,
            accepted: crate::SerializedMetric {
                bytes: accepted.bytes(),
                utf16: accepted.utf16(),
            },
            block,
        })
    }

    /// Final non-allocating join after donor resume and green restart. Any
    /// failure here occurs after the fresh journal may own retained pages, so
    /// the actor must transition the candidate to cancellation/abort rather
    /// than fall back to a normal parse on the same build.
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Actor feasibility wiring is test-driven until the composite root lands.
    pub(crate) fn join_retained_setext_green_activation(
        epoch: LiveCandidateEpoch,
        source: DonorResumedRetainedSetextSourceActivation,
        green: SetextRetainedGreenRestartOutput,
        old_binding: crate::SerializedGreenManifestDescriptor,
        expected_spec: &SerializedGreenRootSpec,
    ) -> Result<DonorResumedRetainedSetextActivation, CandidateWriterError> {
        if !green.matches_activation(
            epoch,
            source.block,
            source.accepted,
            old_binding,
            expected_spec,
        ) {
            return Err(CandidateWriterError::Invariant(
                "retained Setext donor/source/green activation join failed",
            ));
        }
        Ok(DonorResumedRetainedSetextActivation { source, green })
    }

    /// Installs only parts that passed `validate_retained_setext_activation`
    /// and whose independently reconstructed donor parser has already resumed.
    /// Source position remains cumulative; all diagnostic/composer/green run
    /// counters begin at zero for the suffix build.
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Actor feasibility wiring is test-driven until the composite root lands.
    pub(crate) fn install_validated_retained_setext(
        epoch: LiveCandidateEpoch,
        activation: DonorResumedRetainedSetextActivation,
        ticket: ArenaBuildTicket,
        identities: DocumentIdentityAllocator,
    ) -> (Self, RetainedSetextDriverActivation) {
        let DonorResumedRetainedSetextActivation {
            source:
                DonorResumedRetainedSetextSourceActivation {
                    ledger,
                    bindings,
                    parser,
                    acknowledged_lines,
                    accepted: _,
                    block: _,
                },
            green,
        } = activation;
        let (builder, provisional, _old_binding, line_cut, _restart_receipt) =
            green.into_activation_parts();
        let storage = SourceProjectionLineBoundaryStorageAck::from_green_cut(epoch, line_cut)
            .expect("preflight joined the restored green cut to this exact fresh build");
        let (composer, storage) =
            SourceBoundProjectionComposer::begin_retained_line_boundary(epoch, storage)
                .expect("preflight joined the retained composer cut to this fresh build");
        let terminal = bindings
            .last()
            .expect("retained Setext preflight requires Document -> Paragraph");
        let block = terminal.binding.block_id();
        debug_assert!(builder.retained_provisional_matches(&provisional, block));
        let mut donor_checkpoint_samples =
            DonorCheckpointSampleAccumulator::after_unseeded_retained_prefix();
        donor_checkpoint_samples
            .begin_paragraph_group(block)
            .expect("retained activation starts one writer-owned Paragraph sample group");
        let writer = Self {
            epoch,
            ledger,
            composer: Some(composer),
            builder: Some(builder),
            lease: CandidateWriterBuildLease::Suspended(ticket),
            identities,
            action: None,
            issued_source_transition: None,
            next_source_transition: 1,
            green_runs_acknowledged: 0,
            donor_checkpoint_samples,
            reference_semantic: None,
            completion: None,
            active_paragraph: Some(ActiveParagraphNormalizationGroup {
                build: epoch.build_id(),
                block,
                enter: Some(provisional),
                projection_origin: None,
                promoted_setext: false,
                deferred_identity: None,
                deferred_storage: None,
            }),
            deferred_normalization: None,
            active_fenced_code: None,
            completion_route: CandidateWriterCompletionRoute::Independent,
            last_line_boundary_storage: Some(storage),
            poisoned: false,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close: false,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype: false,
        };
        (
            writer,
            RetainedSetextDriverActivation {
                parser,
                bindings,
                acknowledged_lines,
            },
        )
    }

    pub(crate) fn build_id(&self) -> Result<ArenaBuildId, CandidateWriterError> {
        self.completion.as_ref().map_or_else(
            || self.lease.build_id(),
            |completion| Ok(completion.ticket.id()),
        )
    }

    pub(crate) const fn source_descriptor(&self) -> crate::SourceSnapshotDescriptor {
        self.ledger.descriptor()
    }

    pub(crate) fn source_identity(&self) -> crate::SourceRootId {
        self.ledger.source_identity()
    }

    pub(crate) fn cursor_offset(&self) -> usize {
        self.ledger.cursor_offset()
    }

    pub(crate) const fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub(crate) fn poll_source(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateWriterSourcePoll, CandidateWriterError> {
        self.require_ready(epoch)?;
        if self.action.is_some() {
            return Err(CandidateWriterError::Busy);
        }
        if self.issued_source_transition.is_some() {
            return Err(CandidateWriterError::SourceAtomOutstanding);
        }
        let source_poll = self
            .ledger
            .poll(epoch, fuel)
            .map_err(|error| self.poison(error.into()))?;
        match source_poll {
            CandidateSourcePoll::NeedFuel(receipt) => {
                Ok(CandidateWriterSourcePoll::NeedFuel(receipt))
            }
            CandidateSourcePoll::Atom { atom, receipt } => Ok(CandidateWriterSourcePoll::Atom {
                atom: self.issue_source_atom(atom)?,
                receipt,
            }),
            CandidateSourcePoll::Eof(receipt) => Ok(CandidateWriterSourcePoll::Eof(receipt)),
        }
    }

    /// Read-only grammar recognition over the same immutable source revision.
    /// These atoms and checkpoints cannot authorize source consumption. The
    /// authoritative cursor must replay the completed line or range before it
    /// can be finished and committed.
    pub(crate) fn recognition_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionCheckpoint, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        Ok(self.ledger.recognition_checkpoint(epoch)?)
    }

    pub(crate) fn recognition_line_start_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionCheckpoint, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        Ok(self.ledger.recognition_line_start_checkpoint(epoch)?)
    }

    pub(crate) fn begin_recognition_byte_session(
        &mut self,
        epoch: LiveCandidateEpoch,
        bound_epoch: LiveCandidateEpoch,
        checkpoint: CandidateRecognitionCheckpoint,
        physical: SourcePhysicalLineDescriptor,
    ) -> Result<CandidateRecognitionByteSession, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .begin_recognition_byte_session(epoch, bound_epoch, checkpoint, physical)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn poll_recognition_byte_session<S: CandidateRecognitionByteScanner>(
        &mut self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
        fuel: usize,
        scanner: &mut S,
    ) -> Result<
        CandidateRecognitionBytePollReceipt,
        CandidateRecognitionBytePollError<CandidateWriterError, S::Error>,
    > {
        self.require_observation_slot(epoch)
            .map_err(CandidateRecognitionBytePollError::Infrastructure)?;
        match self
            .ledger
            .poll_recognition_byte_session(epoch, session, fuel, scanner)
        {
            Ok(receipt) => Ok(receipt),
            Err(CandidateRecognitionBytePollError::Infrastructure(error)) => Err(
                CandidateRecognitionBytePollError::Infrastructure(self.poison(error.into())),
            ),
            Err(CandidateRecognitionBytePollError::Scanner(error)) => {
                self.poisoned = true;
                Err(CandidateRecognitionBytePollError::Scanner(error))
            }
        }
    }

    pub(crate) fn finish_recognition_byte_session(
        &mut self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
    ) -> Result<CandidateRecognitionByteSessionFinishReceipt, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .finish_recognition_byte_session(epoch, session)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn poll_recognition(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateRecognitionPoll, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .poll_recognition(epoch, fuel)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn poll_recognition_window<S: CandidateRecognitionSink>(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
        sink: &mut S,
    ) -> Result<
        CandidateRecognitionWindowReceipt,
        CandidateRecognitionWindowError<CandidateWriterError, S::Error>,
    > {
        self.require_observation_slot(epoch)
            .map_err(CandidateRecognitionWindowError::Infrastructure)?;
        match self.ledger.poll_recognition_window(epoch, fuel, sink) {
            Ok(receipt) => Ok(receipt),
            Err(CandidateRecognitionWindowError::Infrastructure(error)) => Err(
                CandidateRecognitionWindowError::Infrastructure(self.poison(error.into())),
            ),
            Err(CandidateRecognitionWindowError::Sink(error)) => {
                // The ledger may already have advanced through the atom the
                // sink rejected. Retrying or falling back could therefore
                // fork parser state from exact source state.
                self.poisoned = true;
                Err(CandidateRecognitionWindowError::Sink(error))
            }
        }
    }

    pub(crate) fn finish_recognition_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .finish_recognition_line(epoch)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn begin_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: CandidateRecognitionRangeKind,
    ) -> Result<(), CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .begin_recognition_range(epoch, kind)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn continue_recognition_range_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .continue_recognition_range_line(epoch)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn finish_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionRangeReceipt, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .finish_recognition_range(epoch)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn finish_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineReceipt, CandidateWriterError> {
        self.require_observation_slot(epoch)?;
        self.ledger
            .finish_line(epoch)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn start_open(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: GreenKind,
        facts: FactsEnvelope,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if matches!(
            kind,
            GreenKind::LIST
                | GreenKind::ITEM
                | GreenKind::HEADING
                | GreenKind::FENCED_CODE
                | GreenKind::TABLE
                | GreenKind::TABLE_ROW
                | GreenKind::TABLE_CELL
        ) {
            return Err(
                self.poison(CandidateWriterError::Green(SerializedGreenError::Invalid(
                    "fact-bearing blocks require their typed writer open API",
                ))),
            );
        }
        self.install_open(kind, facts)
    }

    pub(crate) fn start_open_list(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenListOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.install_open(GreenKind::LIST, facts.into_envelope())
    }

    pub(crate) fn start_open_item(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenItemOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.install_open(GreenKind::ITEM, facts.into_envelope())
    }

    pub(crate) fn start_open_heading(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if facts.style() != GreenHeadingStyle::Atx {
            return Err(self.poison(CandidateWriterError::Invariant(
                "Setext Heading must come from Paragraph normalization",
            )));
        }
        self.install_open(GreenKind::HEADING, facts.into_envelope())
    }

    pub(crate) fn start_open_table(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenTableOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.install_open(GreenKind::TABLE, facts.into_envelope())
    }

    pub(crate) fn start_open_table_row(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenTableRowOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.install_open(GreenKind::TABLE_ROW, facts.into_envelope())
    }

    pub(crate) fn start_open_table_cell(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenTableCellOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.install_open(GreenKind::TABLE_CELL, facts.into_envelope())
    }

    pub(crate) fn start_open_fenced_code(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenFencedCodeOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.install_open(GreenKind::FENCED_CODE, facts.into_envelope())
    }

    /// Consumes the active provisional Paragraph binding into the one typed
    /// Setext normalization transaction. Storage is updated before the source
    /// ledger; any intervening failure poisons the unpublished candidate.
    pub(crate) fn start_promote_setext(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if binding.kind() != GreenKind::PARAGRAPH {
            return Err(self.poison(CandidateWriterError::Invariant(
                "Setext promotion requires the active Paragraph binding",
            )));
        }
        if facts.style() != GreenHeadingStyle::Setext {
            return Err(self.poison(CandidateWriterError::Invariant(
                "Setext promotion requires Setext heading facts",
            )));
        }
        let block = binding.binding.block_id();
        let mut group = self.active_paragraph.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "Setext promotion has no active Paragraph group",
            ))
        })?;
        if group.build != epoch.build_id() || group.block != block || group.promoted_setext {
            return Err(self.poison(CandidateWriterError::Invariant(
                "Setext promotion targets a different Paragraph group",
            )));
        }
        let enter = group.enter.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "Setext Paragraph Enter capability was already consumed",
            ))
        })?;
        let defer_identity = group
            .projection_origin
            .as_ref()
            .is_some_and(CanonicalFragmentProjectionOrigin::crosses_parent_selected_restart);
        let projection_origin = group.projection_origin.take();
        let replacement_permit = if defer_identity {
            Some(match self.mint_block() {
                Ok(permit) => permit,
                Err(error) => return Err(self.poison(error)),
            })
        } else {
            None
        };
        self.action = Some(WriterAction::PromoteSetext(SetextJob {
            binding: Some(binding),
            enter: Some(enter),
            projection_origin,
            replacement_permit,
            defer_identity,
            facts,
            storage: None,
            phase: SetextPhase::RequestStructuralFlush,
        }));
        Ok(())
    }

    /// Starts the streamed Paragraph-to-Table normalization after the parser
    /// has recognized a matching delimiter row. The already accepted header
    /// source remains in the same ledger/composer transaction; subsequent
    /// calls supply one scanner-derived cell or projection run at a time.
    pub(crate) fn start_promote_table_header(
        &mut self,
        epoch: LiveCandidateEpoch,
        paragraph: CandidateWriterBinding,
        table_facts: GreenTableOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if paragraph.kind() != GreenKind::PARAGRAPH {
            return Err(self.poison(CandidateWriterError::Invariant(
                "table promotion requires the active Paragraph binding",
            )));
        }
        let block = paragraph.binding.block_id();
        let source_before = {
            let group = self.active_paragraph.as_ref().ok_or_else(|| {
                CandidateWriterError::Invariant("table promotion has no active Paragraph group")
            })?;
            if group.build != epoch.build_id()
                || group.block != block
                || group.promoted_setext
                || group.enter.is_none()
                || group.projection_origin.is_none()
            {
                return Err(self.poison(CandidateWriterError::Invariant(
                    "table promotion targets a different Paragraph group",
                )));
            }
            group
                .enter
                .as_ref()
                .expect("the Paragraph Enter was checked above")
                .source_before()
        };
        let expected_physical = self
            .ledger
            .physical_metric_since(source_before)
            .map_err(CandidateWriterError::SourceLedger)?;
        if expected_physical.bytes == 0
            || expected_physical.utf16 == 0
            || (expected_physical.bytes == 0) != (expected_physical.utf16 == 0)
        {
            return Err(self.poison(CandidateWriterError::Invariant(
                "table promotion has no complete header source",
            )));
        }

        let table_permit = match self.mint_block() {
            Ok(permit) => permit,
            Err(error) => return Err(self.poison(error)),
        };
        // Header rows are wholly closed inside the replacement transaction;
        // their fresh permit never enters the source ledger, but minting here
        // is still the sole authority for the serialized BlockId.
        let header_permit = match self.mint_block() {
            Ok(permit) => permit,
            Err(error) => return Err(self.poison(error)),
        };
        let mut group = self.active_paragraph.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "table promotion lost its Paragraph group",
            ))
        })?;
        let enter = group.enter.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "table Paragraph Enter capability was already consumed",
            ))
        })?;
        let projection_origin = group.projection_origin.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "table promotion has no authenticated composer fragment origin",
            ))
        })?;
        self.action = Some(WriterAction::PromoteTableHeader(TableHeaderJob {
            paragraph: Some(paragraph),
            enter: Some(enter),
            table_permit: Some(table_permit),
            table_facts,
            header_block: header_permit.id(),
            expected_physical,
            cursor: SerializedMetric::default(),
            next_column: 0,
            active_cell: None,
            storage: None,
            projection_origin: Some(projection_origin),
            projection: None,
            phase: TableHeaderPhase::RequestStructuralFlush,
        }));
        Ok(())
    }

    /// Supplies exactly one bounded scanner-derived header action. The writer
    /// owns all identities and converts relative source cuts into exhaustive
    /// coverage; no caller can provide Green events or raw IDs.
    pub(crate) fn supply_table_header_input(
        &mut self,
        epoch: LiveCandidateEpoch,
        input: CandidateTableHeaderInput,
    ) -> Result<(), CandidateWriterError> {
        self.require_ready(epoch)?;
        let action = self.action.take().ok_or(CandidateWriterError::NoAction)?;
        let WriterAction::PromoteTableHeader(mut job) = action else {
            self.action = Some(action);
            return Err(CandidateWriterError::Busy);
        };
        if !matches!(job.phase, TableHeaderPhase::AwaitFragmentInput) {
            self.action = Some(WriterAction::PromoteTableHeader(job));
            return Err(CandidateWriterError::Busy);
        }
        let batch = match self.prepare_table_header_batch(&job, input) {
            Ok(batch) => batch,
            Err(error) => {
                self.action = Some(WriterAction::PromoteTableHeader(job));
                return Err(self.poison(error));
            }
        };
        job.phase = TableHeaderPhase::Emit(batch);
        self.action = Some(WriterAction::PromoteTableHeader(job));
        Ok(())
    }

    fn prepare_table_header_batch(
        &mut self,
        job: &TableHeaderJob,
        input: CandidateTableHeaderInput,
    ) -> Result<TableHeaderEventBatch, CandidateWriterError> {
        match input {
            CandidateTableHeaderInput::BeginCell {
                source_start,
                source_end,
                alignment,
            } => {
                if job.active_cell.is_some()
                    || job.next_column >= job.table_facts.column_count()
                    || !fragment_metric_precedes(job.cursor, source_start)
                    || !fragment_metric_precedes(source_start, source_end)
                    || !fragment_metric_precedes(source_end, job.expected_physical)
                {
                    return Err(CandidateWriterError::Invariant(
                        "table header cell range is crossed or out of bounds",
                    ));
                }
                let marker = self.table_fragment_physical_event(
                    job.cursor,
                    source_start,
                    CoveragePart::BLOCK_MARKER,
                )?;
                let cell = self.mint_block()?;
                let block = cell.id();
                Ok(TableHeaderEventBatch::new(
                    marker,
                    Some(GreenEvent::enter(
                        block,
                        GreenKind::TABLE_CELL,
                        GreenTableCellOpenFacts::header(job.next_column, alignment).into_envelope(),
                    )),
                    None,
                    TableHeaderBatchCompletion::InstallCell {
                        block,
                        source_start,
                        source_end,
                    },
                ))
            }
            CandidateTableHeaderInput::Coverage {
                source_end,
                part,
                logical,
            } => {
                let cell = job
                    .active_cell
                    .as_ref()
                    .ok_or(CandidateWriterError::Invariant(
                        "table header coverage has no active cell",
                    ))?;
                if !fragment_metric_precedes(job.cursor, source_end)
                    || job.cursor == source_end
                    || !fragment_metric_precedes(source_end, cell.source_end)
                {
                    return Err(CandidateWriterError::Invariant(
                        "table header coverage range is empty or out of bounds",
                    ));
                }
                let metric = fragment_metric_difference(source_end, job.cursor)?;
                let permit = self.mint_coverage()?;
                let run = match logical {
                    CandidateTableHeaderLogical::None => {
                        SourceProjectionRun::new(permit.id(), metric.bytes, metric.utf16, 0, part)?
                    }
                    CandidateTableHeaderLogical::Identity => SourceProjectionRun::with_logical(
                        permit.id(),
                        metric.bytes,
                        metric.utf16,
                        0,
                        part,
                        cell.block,
                        LogicalContribution::Identity,
                    )?,
                    CandidateTableHeaderLogical::Hidden { affinity } => {
                        SourceProjectionRun::with_logical(
                            permit.id(),
                            metric.bytes,
                            metric.utf16,
                            0,
                            part,
                            cell.block,
                            LogicalContribution::Hidden { affinity },
                        )?
                    }
                };
                Ok(TableHeaderEventBatch::new(
                    Some(GreenEvent::Coverage(run)),
                    None,
                    None,
                    TableHeaderBatchCompletion::AdvanceCursor(source_end),
                ))
            }
            CandidateTableHeaderInput::EndCell => {
                let cell = job
                    .active_cell
                    .as_ref()
                    .ok_or(CandidateWriterError::Invariant(
                        "table header cell close has no active cell",
                    ))?;
                if job.cursor != cell.source_end {
                    return Err(CandidateWriterError::Invariant(
                        "table header cell closed before exhaustive coverage",
                    ));
                }
                Ok(TableHeaderEventBatch::new(
                    Some(GreenEvent::exit(ClosedChildAggregate::default())),
                    None,
                    None,
                    TableHeaderBatchCompletion::CloseCell,
                ))
            }
            CandidateTableHeaderInput::Finish { content_end } => {
                if job.active_cell.is_some()
                    || job.next_column != job.table_facts.column_count()
                    || !fragment_metric_precedes(job.cursor, content_end)
                    || !fragment_metric_precedes(content_end, job.expected_physical)
                {
                    return Err(CandidateWriterError::Invariant(
                        "table header finish is incomplete or out of bounds",
                    ));
                }
                let marker = self.table_fragment_physical_event(
                    job.cursor,
                    content_end,
                    CoveragePart::BLOCK_MARKER,
                )?;
                let terminal = self.table_fragment_physical_event(
                    content_end,
                    job.expected_physical,
                    CoveragePart::TERMINAL,
                )?;
                Ok(TableHeaderEventBatch::new(
                    marker,
                    terminal,
                    Some(GreenEvent::exit(ClosedChildAggregate::default())),
                    TableHeaderBatchCompletion::FinishFragment,
                ))
            }
        }
    }

    fn table_fragment_physical_event(
        &mut self,
        start: SerializedMetric,
        end: SerializedMetric,
        part: CoveragePart,
    ) -> Result<Option<GreenEvent>, CandidateWriterError> {
        let metric = fragment_metric_difference(end, start)?;
        if metric.bytes == 0 && metric.utf16 == 0 {
            return Ok(None);
        }
        if metric.bytes == 0 || metric.utf16 == 0 {
            return Err(CandidateWriterError::Invariant(
                "table fragment physical partition splits a source scalar",
            ));
        }
        let permit = self.mint_coverage()?;
        Ok(Some(GreenEvent::Coverage(SourceProjectionRun::new(
            permit.id(),
            metric.bytes,
            metric.utf16,
            0,
            part,
        )?)))
    }

    pub(crate) fn mark_fenced_code_boundary(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: &CandidateWriterBinding,
        boundary: CandidateFencedCodeBoundary,
    ) -> Result<(), CandidateWriterError> {
        // Identity replay may have returned a still-outstanding line-ending
        // atom. Boundary marking observes the accepted logical prefix but
        // consumes no source, so it deliberately requires only an idle action
        // slot rather than the stricter no-outstanding-atom start guard.
        self.require_action_slot(epoch)?;
        let metric = self
            .ledger
            .fenced_code_logical_metric(epoch, &binding.binding)
            .map_err(|error| self.poison(error.into()))?;
        let Some(active) = self.active_fenced_code.as_mut() else {
            return Err(self.poison(CandidateWriterError::Invariant(
                "fenced-code boundary has no active fold",
            )));
        };
        if active.build != epoch.build_id() || active.block != binding.binding.block_id() {
            return Err(self.poison(CandidateWriterError::Invariant(
                "fenced-code boundary targets a different binding",
            )));
        }
        match boundary {
            CandidateFencedCodeBoundary::InfoEnd
                if active.info_end.is_none() && active.literal_start.is_none() =>
            {
                active.info_end = Some(metric);
            }
            CandidateFencedCodeBoundary::LiteralStart
                if active.info_end.is_some() && active.literal_start.is_none() =>
            {
                active.literal_start = Some(metric);
            }
            CandidateFencedCodeBoundary::InfoEnd | CandidateFencedCodeBoundary::LiteralStart => {
                return Err(self.poison(CandidateWriterError::Invariant(
                    "fenced-code boundaries are duplicated or reversed",
                )));
            }
        }
        Ok(())
    }

    fn install_open(
        &mut self,
        kind: GreenKind,
        facts: FactsEnvelope,
    ) -> Result<(), CandidateWriterError> {
        let (deferred_residual, whole_normalization) = if kind == GreenKind::PARAGRAPH {
            let residual = self.deferred_normalization.take();
            if let Some(pending) = residual.as_ref() {
                if let Err(error) = self.validate_pending_deferred_normalization(pending) {
                    return Err(self.poison(error));
                }
            }
            (residual, None)
        } else {
            (None, self.resolve_deferred_normalization_whole()?)
        };
        let phase = if whole_normalization.is_some() {
            OpenPhase::BeginWholeNormalization
        } else {
            OpenPhase::RequestStructuralFlush
        };
        self.action = Some(WriterAction::Open(OpenJob {
            kind,
            facts: Some(facts),
            permit: None,
            deferred_residual,
            whole_normalization,
            phase,
        }));
        Ok(())
    }

    fn validate_pending_deferred_normalization(
        &self,
        pending: &PendingDeferredNormalization,
    ) -> Result<(), CandidateWriterError> {
        if pending.storage.retired_block() != pending.identity.retired_block()
            || pending.storage.replacement_block() != pending.identity.replacement_block()
            || pending.identity.build_id() != self.epoch.build_id()
        {
            return Err(CandidateWriterError::Invariant(
                "deferred normalization storage and ledger identities crossed",
            ));
        }
        Ok(())
    }

    fn resolve_deferred_normalization_whole(
        &mut self,
    ) -> Result<Option<PendingWholeNormalization>, CandidateWriterError> {
        let Some(pending) = self.deferred_normalization.take() else {
            return Ok(None);
        };
        if let Err(error) = self.validate_pending_deferred_normalization(&pending) {
            return Err(self.poison(error));
        }
        let identity = match self
            .ledger
            .resolve_deferred_normalization_whole(self.epoch, pending.identity)
        {
            Ok(identity) => identity,
            Err(error) => return Err(self.poison(error.into())),
        };
        if identity.build_id() != self.epoch.build_id()
            || identity.retired_block() != pending.storage.retired_block()
            || identity.replacement_block() != pending.storage.replacement_block()
            || identity.kind() != GreenKind::HEADING
        {
            return Err(self.poison(CandidateWriterError::Invariant(
                "resolved whole normalization crossed its storage acknowledgement",
            )));
        }
        Ok(Some(PendingWholeNormalization {
            identity,
            storage: pending.storage,
        }))
    }

    // Moving both values is the parser-side single-use boundary even when the
    // ledger validates their private fields by reference during this call.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn start_consume(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
        owner: &CandidateWriterBinding,
        part: CoveragePart,
        logical: CandidateWriterLogicalAction<'_>,
    ) -> Result<(), CandidateWriterError> {
        self.require_action_slot(epoch)?;
        let atom = self.take_issued_atom(atom)?;
        let logical = (match logical {
            CandidateWriterLogicalAction::None => Ok(CandidateLogicalAction::none()),
            CandidateWriterLogicalAction::Identity { target } => {
                CandidateLogicalAction::identity(&target.binding)
            }
            CandidateWriterLogicalAction::Hidden { target, affinity } => {
                CandidateLogicalAction::hidden(&target.binding, affinity)
            }
            CandidateWriterLogicalAction::TabToSpaces { target, spaces } => {
                CandidateLogicalAction::tab_to_spaces(&target.binding, &atom, spaces)
            }
            CandidateWriterLogicalAction::NulToReplacement { target } => {
                CandidateLogicalAction::nul_to_replacement(&target.binding, &atom)
            }
            CandidateWriterLogicalAction::CanonicalLineEnding { target } => {
                CandidateLogicalAction::canonical_line_ending(&target.binding, &atom)
            }
        })
        .map_err(|error| self.poison(error.into()))?;
        let boundary = atom.boundary();
        let piece = self
            .ledger
            .consume_to(epoch, boundary, &owner.binding, part, &logical)
            .map_err(|error| self.poison(error.into()))?;
        self.start_piece(piece)
    }

    /// Defers one exact line terminator until grammar lookahead determines
    /// whether it continues a logical leaf or closes it. The atom is consumed
    /// from the authoritative cursor now, but no projection run is emitted
    /// until `start_resolve_terminator` supplies the parser's decision.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn stage_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
        terminal: &CandidateWriterBinding,
    ) -> Result<(), CandidateWriterError> {
        self.require_action_slot(epoch)?;
        let atom = self.take_issued_atom(atom)?;
        self.ledger
            .stage_consumed_terminator(epoch, &atom, &terminal.binding)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn start_resolve_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        let piece = self
            .ledger
            .resolve_consumed_terminator(epoch, resolution)
            .map_err(|error| self.poison(error.into()))?;
        self.start_piece(piece)
    }

    /// Accepts one atom from a parser-certified blank-line replay without
    /// assigning it early. `stage_blank_gap` subsequently validates the whole
    /// line as blank and moves the complete range into one pending gap.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn defer_blank_gap_atom(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
    ) -> Result<(), CandidateWriterError> {
        self.require_action_slot(epoch)?;
        let atom = self.take_issued_atom(atom)?;
        if !matches!(
            atom.kind(),
            CandidateSourceAtomKind::Scalar(' ')
                | CandidateSourceAtomKind::Tab
                | CandidateSourceAtomKind::LineEnding(_)
        ) {
            return Err(self.poison(CandidateWriterError::NonBlankGapAtom));
        }
        Ok(())
    }

    pub(crate) fn stage_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        self.ledger
            .stage_consumed_blank_gap(epoch)
            .map_err(|error| self.poison(error.into()))
    }

    pub(crate) fn start_resolve_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        surviving_owner: &CandidateWriterBinding,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        let piece = self
            .ledger
            .resolve_consumed_gap(epoch, &surviving_owner.binding)
            .map_err(|error| self.poison(error.into()))?;
        self.start_piece(piece)
    }

    fn start_piece(
        &mut self,
        piece: crate::ConsumedSourcePiece,
    ) -> Result<(), CandidateWriterError> {
        let progress = self
            .composer_mut()?
            .push_piece(piece)
            .map_err(|error| self.poison(error.into()))?;
        let drain = ComposerDrain::begin(progress, false).map_err(|error| self.poison(error))?;
        self.action = Some(WriterAction::Consume(ConsumeJob { drain }));
        Ok(())
    }

    /// Starts one exact parser-command range from a physical byte length.
    /// Source identity, line identity, start/end, owner, target, and metrics
    /// are captured by the source ledger; the caller cannot supply a boundary.
    pub(crate) fn start_range_replay(
        &mut self,
        epoch: LiveCandidateEpoch,
        physical_owner: &CandidateWriterBinding,
        part: CoveragePart,
        physical_bytes: u64,
        recipe: CandidateWriterRangeRecipe,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        let plan = self
            .ledger
            .mint_range_replay_plan(epoch, &physical_owner.binding, part, physical_bytes, recipe)
            .map_err(|error| self.poison(error.into()))?;
        self.install_range_replay(plan, false);
        Ok(())
    }

    fn install_range_replay(&mut self, plan: CandidateRangeReplayPlan, legacy: bool) {
        let scan_high_water = plan.absolute_range().0;
        self.action = Some(WriterAction::RangeReplay(RangeReplayJob {
            plan,
            legacy_identity_completion: legacy,
            scan_high_water,
            last_boundary: None,
            pending_atom: None,
            completion: None,
            writer_polls: 0,
            source_work_units: 0,
            source_bytes_read: 0,
            atoms_scanned: 0,
            source_pieces: 0,
            maximum_pending_atoms: 0,
            maximum_pending_boundaries: 0,
            phase: RangeReplayPhase::Scan,
        }));
    }

    /// Compatibility wrapper for the previous whole-content identity API.
    /// It now stops at the recognized content endpoint and never prefetches
    /// the physical terminator.
    pub(crate) fn start_identity_line_replay(
        &mut self,
        epoch: LiveCandidateEpoch,
        terminal: &CandidateWriterBinding,
        part: CoveragePart,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        let plan = self
            .ledger
            .mint_remaining_identity_replay_plan(epoch, &terminal.binding, part)
            .map_err(|error| self.poison(error.into()))?;
        self.install_range_replay(plan, true);
        Ok(())
    }

    pub(crate) fn start_close(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: ClosedChildAggregate,
        last_line_blank: bool,
    ) -> Result<(), CandidateWriterError> {
        self.start_close_with_facts(
            epoch,
            binding,
            closed,
            last_line_blank,
            GreenCloseFacts::None,
        )
    }

    pub(crate) fn start_close_with_facts(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenCloseFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if binding.kind() == GreenKind::FENCED_CODE {
            return Err(self.poison(CandidateWriterError::Invariant(
                "fenced-code close facts must be derived from the active logical fold",
            )));
        }
        facts
            .validate_for_kind(binding.kind())
            .map_err(|error| self.poison(error.into()))?;
        let closes_active_paragraph = self.validate_active_paragraph_close(&binding)?;
        let whole_normalization = self.resolve_deferred_normalization_whole()?;
        let phase = if whole_normalization.is_some() {
            ClosePhase::BeginWholeNormalization
        } else {
            ClosePhase::RequestStructuralFlush
        };
        self.action = Some(WriterAction::Close(CloseJob {
            binding,
            closed,
            last_line_blank,
            facts,
            closes_active_paragraph,
            whole_normalization,
            phase,
        }));
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn start_close_fenced_code_with_test_facts(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenFencedCodeCloseFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if binding.kind() != GreenKind::FENCED_CODE {
            return Err(self.poison(CandidateWriterError::Invariant(
                "typed fenced-code close targets a non-fenced binding",
            )));
        }
        let active = self.active_fenced_code.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "typed fenced-code close has no active projection fold",
            ))
        })?;
        if active.build != epoch.build_id() || active.block != binding.binding.block_id() {
            return Err(self.poison(CandidateWriterError::Invariant(
                "typed fenced-code close targets a different projection fold",
            )));
        }
        GreenCloseFacts::FencedCode(facts)
            .validate_for_kind(binding.kind())
            .map_err(|error| self.poison(error.into()))?;
        let whole_normalization = self.resolve_deferred_normalization_whole()?;
        let phase = if whole_normalization.is_some() {
            ClosePhase::BeginWholeNormalization
        } else {
            ClosePhase::RequestStructuralFlush
        };
        self.action = Some(WriterAction::Close(CloseJob {
            binding,
            closed,
            last_line_blank,
            facts: GreenCloseFacts::FencedCode(facts),
            closes_active_paragraph: false,
            whole_normalization,
            phase,
        }));
        Ok(())
    }

    pub(crate) fn start_close_fenced_code(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: ClosedChildAggregate,
        last_line_blank: bool,
        fence_closed: bool,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        let logical_end = self
            .ledger
            .fenced_code_logical_metric(epoch, &binding.binding)
            .map_err(|error| self.poison(error.into()))?;
        let active = self.active_fenced_code.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "fenced-code close has no active projection fold",
            ))
        })?;
        if active.build != epoch.build_id() || active.block != binding.binding.block_id() {
            return Err(self.poison(CandidateWriterError::Invariant(
                "fenced-code close targets a different projection fold",
            )));
        }
        let info_end = active.info_end.ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "fenced-code close is missing InfoEnd",
            ))
        })?;
        let literal_start = active.literal_start.ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "fenced-code close is missing LiteralStart",
            ))
        })?;
        if !source_metric_precedes(info_end, literal_start)
            || !source_metric_precedes(literal_start, logical_end)
        {
            return Err(self.poison(CandidateWriterError::Invariant(
                "fenced-code logical boundaries are out of order",
            )));
        }
        let info = GreenRelativeLogicalSlice::new(0..info_end.bytes(), 0..info_end.utf16())
            .map_err(|error| self.poison(error.into()))?;
        let literal = GreenRelativeLogicalSlice::new(
            literal_start.bytes()..logical_end.bytes(),
            literal_start.utf16()..logical_end.utf16(),
        )
        .map_err(|error| self.poison(error.into()))?;
        let facts = GreenFencedCodeCloseFacts::new(fence_closed, info, literal)
            .map_err(|error| self.poison(error.into()))?;
        let whole_normalization = self.resolve_deferred_normalization_whole()?;
        let phase = if whole_normalization.is_some() {
            ClosePhase::BeginWholeNormalization
        } else {
            ClosePhase::RequestStructuralFlush
        };
        self.action = Some(WriterAction::Close(CloseJob {
            binding,
            closed,
            last_line_blank,
            facts: GreenCloseFacts::FencedCode(facts),
            closes_active_paragraph: false,
            whole_normalization,
            phase,
        }));
        Ok(())
    }

    pub(crate) fn start_finish(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if self.active_paragraph.is_some() {
            return Err(self.poison(CandidateWriterError::Invariant(
                "candidate finish has an unclosed Paragraph normalization group",
            )));
        }
        if self.active_fenced_code.is_some() {
            return Err(self.poison(CandidateWriterError::Invariant(
                "candidate finish has an unclosed fenced-code projection fold",
            )));
        }
        #[cfg(feature = "exact-parser")]
        if self.reference_semantic.is_none() {
            self.reference_semantic = Some(
                CandidateReferenceSemanticTransaction::new(epoch)
                    .map_err(|error| self.poison(error))?,
            );
        }
        let whole_normalization = self.resolve_deferred_normalization_whole()?;
        if whole_normalization.is_some() {
            self.action = Some(WriterAction::Finish(FinishJob {
                composer: None,
                #[cfg(feature = "exact-parser")]
                reference: None,
                whole_normalization,
                phase: FinishPhase::BeginWholeNormalization,
            }));
            return Ok(());
        }
        let drain = self.begin_finish_drain(epoch)?;
        self.action = Some(WriterAction::Finish(FinishJob {
            composer: None,
            #[cfg(feature = "exact-parser")]
            reference: None,
            whole_normalization: None,
            phase: FinishPhase::Drain(drain),
        }));
        Ok(())
    }

    fn begin_finish_drain(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<ComposerDrain, CandidateWriterError> {
        let source = self
            .ledger
            .seal(epoch)
            .map_err(|error| self.poison(error.into()))?;
        let progress = self
            .composer_mut()?
            .begin_finish(source)
            .map_err(|error| self.poison(error.into()))?;
        ComposerDrain::begin(progress, true).map_err(|error| self.poison(error))
    }

    /// Compares the actor-owned quiescent source cut to opaque mapped C. This
    /// runs before checkpoint drain, so only source bytes/UTF-16/line ordinal
    /// participate; green/projection axes are captured fresh after pausing.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn relation_to_parent_selected_convergence(
        &self,
        mapped: &ParentSelectedMappedConvergence,
    ) -> Result<ParentSelectedConvergenceTargetRelation, CandidateWriterError> {
        self.ledger
            .validate_line_boundary_continuation(self.epoch)?;
        let emitted = self.ledger.physical_metric();
        let current = RelativeCheckpointMeasure::new(
            emitted.bytes(),
            emitted.utf16(),
            self.ledger.physical_line_ordinal(),
            0,
            0,
        );
        mapped
            .relation_to_current_cut(self.epoch, current)
            .map_err(|_| {
                CandidateWriterError::Invariant(
                    "candidate source cut disagrees with mapped convergence target",
                )
            })
    }

    /// Starts the writer half of a same-build physical-line checkpoint.
    /// Parser authority is joined later, after the dedicated projection drain
    /// and exact packed-green cut both exist. This admission only accepts the
    /// source ledger's already acknowledged, quiescent line boundary.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn start_line_boundary_checkpoint(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineBoundaryCheckpointAdmission, CandidateWriterError> {
        self.start_line_boundary_checkpoint_with_snapshot(epoch, false)
    }

    /// Starts the one convergence-target checkpoint that also retains an
    /// O(open-depth) green-prefix observation. Ordinary sparse checkpoints do
    /// not pay this allocation cost.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn start_convergence_line_boundary_checkpoint(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineBoundaryCheckpointAdmission, CandidateWriterError> {
        self.start_line_boundary_checkpoint_with_snapshot(epoch, true)
    }

    #[cfg(feature = "exact-parser")]
    fn start_line_boundary_checkpoint_with_snapshot(
        &mut self,
        epoch: LiveCandidateEpoch,
        capture_green_prefix_snapshot: bool,
    ) -> Result<CandidateLineBoundaryCheckpointAdmission, CandidateWriterError> {
        self.require_start(epoch)?;
        if self.deferred_normalization.is_some() {
            return Ok(CandidateLineBoundaryCheckpointAdmission::Skipped(
                CandidateLineBoundaryCheckpointSkip::DeferredNormalizationPending,
            ));
        }
        match self.ledger.validate_line_boundary_continuation(epoch) {
            Ok(()) => {}
            Err(
                SourceBoundLedgerError::RecognitionReplayPending
                | SourceBoundLedgerError::RecognitionRangeAlreadyOpen
                | SourceBoundLedgerError::LineBoundaryContinuationUnavailable,
            ) => {
                return Ok(CandidateLineBoundaryCheckpointAdmission::Skipped(
                    CandidateLineBoundaryCheckpointSkip::SourceNotQuiescent,
                ));
            }
            Err(error) => return Err(self.poison(error.into())),
        }
        let affinity_neutral = self
            .composer
            .as_ref()
            .ok_or(CandidateWriterError::Invariant(
                "line-boundary checkpoint lost projection composer",
            ))?
            .line_boundary_checkpoint_is_affinity_neutral()
            .map_err(CandidateWriterError::from)?;
        if !affinity_neutral {
            return Ok(CandidateLineBoundaryCheckpointAdmission::Skipped(
                CandidateLineBoundaryCheckpointSkip::ProjectionVirtualUnsafe,
            ));
        }
        self.action = Some(WriterAction::LineBoundaryCheckpoint(
            LineBoundaryCheckpointJob {
                phase: LineBoundaryCheckpointPhase::RequestDedicatedDrain,
                capture_green_prefix_snapshot,
                green_prefix_snapshot: None,
            },
        ));
        Ok(CandidateLineBoundaryCheckpointAdmission::Started)
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn into_line_boundary_continuation(
        mut self,
    ) -> Result<
        CandidateWriterLineBoundaryContinuation,
        Box<CandidateWriterLineBoundaryCaptureFailure>,
    > {
        let fail = |writer: CandidateWriter, error| {
            Box::new(CandidateWriterLineBoundaryCaptureFailure { error, writer })
        };
        if let Err(error) = self.validate_line_boundary_continuation_capture() {
            return Err(fail(self, error));
        }

        let action = self
            .action
            .take()
            .expect("checkpoint action was borrowed immediately before extraction");
        let WriterAction::LineBoundaryCheckpoint(LineBoundaryCheckpointJob {
            phase: LineBoundaryCheckpointPhase::Ready(Some(composer)),
            capture_green_prefix_snapshot: _,
            green_prefix_snapshot,
        }) = action
        else {
            unreachable!("checkpoint action was validated immediately before extraction")
        };
        let Self {
            epoch,
            ledger,
            composer: None,
            builder: Some(builder),
            lease,
            identities,
            action: None,
            issued_source_transition: None,
            next_source_transition,
            green_runs_acknowledged,
            donor_checkpoint_samples,
            reference_semantic,
            completion: None,
            active_paragraph,
            deferred_normalization: None,
            active_fenced_code,
            completion_route,
            last_line_boundary_storage: _,
            poisoned: false,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype,
        } = self
        else {
            unreachable!("checkpoint writer shape was validated before the infallible move")
        };
        let CandidateWriterBuildLease::Suspended(ticket) = lease else {
            unreachable!("checkpoint lease was validated before the infallible move")
        };
        let source = ledger
            .into_line_boundary_continuation(epoch)
            .expect("unchanged ledger passed line-boundary validation before extraction");
        Ok(CandidateWriterLineBoundaryContinuation {
            epoch,
            source,
            composer,
            builder,
            ticket: Some(ticket),
            identities,
            next_source_transition,
            green_runs_acknowledged,
            donor_checkpoint_samples,
            reference_semantic,
            green_prefix_snapshot,
            active_paragraph,
            active_fenced_code,
            completion_route,
            #[cfg(test)]
            fail_after_green_ack_before_ledger_close,
            #[cfg(test)]
            fail_after_setext_green_ack_before_ledger_retype,
        })
    }

    #[cfg(feature = "exact-parser")]
    fn validate_line_boundary_continuation_capture(&self) -> Result<(), CandidateWriterError> {
        if self.poisoned {
            return Err(CandidateWriterError::WriterPoisoned);
        }
        if self.issued_source_transition.is_some() {
            return Err(CandidateWriterError::SourceAtomOutstanding);
        }
        if self.completion.is_some() || self.composer.is_some() {
            return Err(CandidateWriterError::Invariant(
                "checkpoint Ready must own the paused projection composer",
            ));
        }
        self.ledger
            .validate_line_boundary_continuation(self.epoch)?;
        let Some(WriterAction::LineBoundaryCheckpoint(LineBoundaryCheckpointJob {
            phase: LineBoundaryCheckpointPhase::Ready(Some(continuation)),
            capture_green_prefix_snapshot,
            green_prefix_snapshot,
        })) = self.action.as_ref()
        else {
            return Err(CandidateWriterError::Invariant(
                "writer checkpoint continuation is not Ready",
            ));
        };
        let Some(builder) = self.builder.as_ref() else {
            return Err(CandidateWriterError::Invariant(
                "checkpoint green builder is missing",
            ));
        };
        let CandidateWriterBuildLease::Suspended(ticket) = &self.lease else {
            return Err(CandidateWriterError::Invariant(
                "checkpoint writer lacks a suspended build ticket",
            ));
        };
        let Some(cut) = continuation.green_cut() else {
            return Err(CandidateWriterError::Invariant(
                "production checkpoint continuation lacks a green cut",
            ));
        };
        if *capture_green_prefix_snapshot != green_prefix_snapshot.is_some()
            || continuation.epoch() != self.epoch
            || continuation.build_id() != self.epoch.build_id()
            || ticket.id() != self.epoch.build_id()
            || builder.build_id() != self.epoch.build_id()
            || cut.source_before() != continuation.source_before()
            || !builder.line_boundary_cut_is_current(cut)
        {
            return Err(CandidateWriterError::WrongCandidate);
        }
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        epoch: LiveCandidateEpoch,
        arena: &mut PageArena,
    ) -> Result<CandidateWriterProgress, CandidateWriterError> {
        self.require_ready(epoch)?;
        let action = self.action.take().ok_or(CandidateWriterError::NoAction)?;
        let result = match action {
            WriterAction::Open(job) => self.poll_open(job, arena),
            WriterAction::Consume(job) => self.poll_consume(job, arena),
            WriterAction::RangeReplay(job) => self.poll_range_replay(job, arena),
            WriterAction::PromoteSetext(job) => self.poll_setext(job, arena),
            WriterAction::PromoteTableHeader(job) => self.poll_table_header(job, arena),
            #[cfg(feature = "exact-parser")]
            WriterAction::ReferencePrefix(job) => self.poll_reference_prefix(job, arena),
            WriterAction::Close(job) => self.poll_close(job, arena),
            WriterAction::Finish(job) => self.poll_finish(job, arena),
            #[cfg(feature = "exact-parser")]
            WriterAction::LineBoundaryCheckpoint(job) => {
                self.poll_line_boundary_checkpoint(job, arena)
            }
        };
        match result {
            Ok((Some(action), progress)) => {
                self.action = Some(action);
                Ok(progress)
            }
            Ok((None, progress)) => Ok(progress),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    fn poll_open(
        &mut self,
        mut job: OpenJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            OpenPhase::BeginWholeNormalization => {
                let pending =
                    job.whole_normalization
                        .take()
                        .ok_or(CandidateWriterError::Invariant(
                            "whole-normalizing open lost its storage authority",
                        ))?;
                #[cfg(feature = "exact-parser")]
                self.donor_checkpoint_samples
                    .reidentify_finalized_whole_normalization(&pending.identity)?;
                self.with_short_session(arena, move |builder, session| {
                    builder.begin_whole_normalization_reidentity(
                        session,
                        pending.identity,
                        pending.storage,
                    )
                })?;
                job.phase = OpenPhase::AwaitWholeNormalization;
            }
            OpenPhase::AwaitWholeNormalization => {
                if self.poll_green_acknowledgement(arena)? {
                    let _acknowledgement = self.with_short_session(arena, |builder, session| {
                        builder.take_whole_normalization_reidentity(session)
                    })?;
                    job.phase = OpenPhase::RequestStructuralFlush;
                }
            }
            OpenPhase::RequestStructuralFlush => {
                let progress = self.composer_mut()?.flush_before_structure()?;
                job.phase = OpenPhase::Drain(ComposerDrain::begin(progress, false)?);
            }
            OpenPhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    OpenPhase::OfferEnter
                } else {
                    OpenPhase::Drain(drain)
                };
            }
            OpenPhase::OfferEnter => {
                let facts = job.facts.take().ok_or(CandidateWriterError::Invariant(
                    "open facts already consumed",
                ))?;
                if let Some(pending) = job.deferred_residual.as_ref() {
                    if job.kind != GreenKind::PARAGRAPH || !facts.fields.is_empty() {
                        return Err(CandidateWriterError::Invariant(
                            "deferred normalization residual must reopen a plain Paragraph",
                        ));
                    }
                    self.with_short_session(arena, move |builder, session| {
                        builder.offer_deferred_normalization_paragraph_enter(
                            session,
                            &pending.identity,
                            &pending.storage,
                        )
                    })?;
                } else {
                    let permit = self.mint_block()?;
                    if job.kind == GreenKind::PARAGRAPH {
                        let block = permit.id();
                        self.with_short_session(arena, move |builder, session| {
                            builder.offer_provisional_paragraph_enter(session, block, facts)
                        })?;
                    } else {
                        self.offer_green(arena, GreenEvent::enter(permit.id(), job.kind, facts))?;
                    }
                    job.permit = Some(permit);
                }
                job.phase = OpenPhase::AwaitEnterAcknowledgement;
            }
            OpenPhase::AwaitEnterAcknowledgement => {
                if self.poll_green_acknowledgement(arena)? {
                    job.phase = OpenPhase::OpenLedger;
                }
            }
            OpenPhase::OpenLedger => {
                let binding = match job.deferred_residual.take() {
                    Some(pending) => self
                        .ledger
                        .reopen_deferred_normalization_identity(self.epoch, pending.identity)?,
                    None => {
                        let permit = job
                            .permit
                            .take()
                            .ok_or(CandidateWriterError::Invariant("open permit missing"))?;
                        self.ledger.open_binding(self.epoch, permit, job.kind)?
                    }
                };
                if job.kind == GreenKind::PARAGRAPH {
                    if self.active_paragraph.is_some() {
                        return Err(CandidateWriterError::Invariant(
                            "Paragraph normalization groups cannot nest",
                        ));
                    }
                    let block = binding.block_id();
                    let enter = self.with_short_session(arena, |builder, session| {
                        builder.take_provisional_paragraph_enter(session, block)
                    })?;
                    let projection_origin =
                        self.composer_mut()?.capture_canonical_fragment_origin()?;
                    #[cfg(feature = "exact-parser")]
                    self.donor_checkpoint_samples.begin_paragraph_group(block)?;
                    self.active_paragraph = Some(ActiveParagraphNormalizationGroup {
                        build: self.epoch.build_id(),
                        block,
                        enter: Some(enter),
                        projection_origin: Some(projection_origin),
                        promoted_setext: false,
                        deferred_identity: None,
                        deferred_storage: None,
                    });
                }
                if job.kind == GreenKind::FENCED_CODE {
                    if self.active_fenced_code.is_some() {
                        return Err(CandidateWriterError::Invariant(
                            "fenced-code projection folds cannot nest",
                        ));
                    }
                    self.active_fenced_code = Some(ActiveFencedCodeProjectionFold {
                        build: self.epoch.build_id(),
                        block: binding.block_id(),
                        info_end: None,
                        literal_start: None,
                    });
                }
                return Ok((
                    None,
                    CandidateWriterProgress::Opened(CandidateWriterBinding { binding }),
                ));
            }
        }
        Ok((
            Some(WriterAction::Open(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    fn poll_consume(
        &mut self,
        mut job: ConsumeJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        self.poll_drain(&mut job.drain, arena)?;
        if job.drain.is_complete() {
            Ok((None, CandidateWriterProgress::ActionComplete))
        } else {
            Ok((
                Some(WriterAction::Consume(job)),
                CandidateWriterProgress::Pending,
            ))
        }
    }

    fn poll_range_replay(
        &mut self,
        mut job: RangeReplayJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        job.writer_polls =
            job.writer_polls
                .checked_add(1)
                .ok_or(CandidateWriterError::Invariant(
                    "range replay writer poll count overflow",
                ))?;
        match job.phase {
            RangeReplayPhase::Scan => {
                if let Some(atom) = job.pending_atom.take() {
                    let at_end = atom.boundary().absolute_offset() == job.plan.absolute_range().1;
                    let piece = self
                        .ledger
                        .consume_range_replay_canonical_atom(self.epoch, &job.plan, &atom)?;
                    job.source_pieces =
                        job.source_pieces
                            .checked_add(1)
                            .ok_or(CandidateWriterError::Invariant(
                                "range replay piece count overflow",
                            ))?;
                    if at_end {
                        job.completion =
                            Some(self.ledger.finish_range_replay(self.epoch, &job.plan)?);
                    }
                    let progress = self.composer_mut()?.push_piece(piece)?;
                    job.phase = RangeReplayPhase::Drain {
                        drain: ComposerDrain::begin(progress, false)?,
                        resume: if at_end {
                            RangeReplayResume::Ready
                        } else {
                            RangeReplayResume::Scan
                        },
                    };
                } else {
                    let exact_end = job.plan.absolute_range().1;
                    if job.scan_high_water == exact_end {
                        return Err(CandidateWriterError::SourceLedger(
                            SourceBoundLedgerError::RangeReplayIncomplete,
                        ));
                    }
                    let mut began_drain = false;
                    for _ in 0..CANDIDATE_RANGE_REPLAY_MAX_SOURCE_WORK_PER_POLL {
                        // The endpoint check precedes every decoder poll. This
                        // is what prevents a range command from prefetching a
                        // terminator or the next parser command's first byte.
                        if job.scan_high_water == exact_end {
                            break;
                        }
                        let poll = self
                            .ledger
                            .poll(self.epoch, 1)
                            .map_err(CandidateWriterError::SourceLedger)?;
                        let receipt = match &poll {
                            CandidateSourcePoll::NeedFuel(receipt)
                            | CandidateSourcePoll::Atom { receipt, .. }
                            | CandidateSourcePoll::Eof(receipt) => *receipt,
                        };
                        job.source_work_units = job
                            .source_work_units
                            .checked_add(u64::try_from(receipt.work_units).map_err(|_| {
                                CandidateWriterError::Invariant(
                                    "range replay work conversion overflow",
                                )
                            })?)
                            .ok_or(CandidateWriterError::Invariant(
                                "range replay source work overflow",
                            ))?;
                        job.source_bytes_read = job
                            .source_bytes_read
                            .checked_add(u64::try_from(receipt.source_bytes_read).map_err(
                                |_| {
                                    CandidateWriterError::Invariant(
                                        "range replay byte conversion overflow",
                                    )
                                },
                            )?)
                            .ok_or(CandidateWriterError::Invariant(
                                "range replay source bytes overflow",
                            ))?;
                        match poll {
                            CandidateSourcePoll::NeedFuel(_) => {}
                            CandidateSourcePoll::Eof(_) => {
                                return Err(CandidateWriterError::SourceLedger(
                                    SourceBoundLedgerError::RangeReplayIncomplete,
                                ));
                            }
                            CandidateSourcePoll::Atom { atom, .. } => {
                                let atom_end = atom.boundary().absolute_offset();
                                if atom_end > exact_end {
                                    return Err(CandidateWriterError::SourceLedger(
                                        SourceBoundLedgerError::RangeReplayEndpointSplitsAtom,
                                    ));
                                }
                                job.scan_high_water = atom_end;
                                job.atoms_scanned = job.atoms_scanned.checked_add(1).ok_or(
                                    CandidateWriterError::Invariant(
                                        "range replay atom count overflow",
                                    ),
                                )?;
                                let kind = atom.kind();
                                let canonical_typed = match job.plan.recipe() {
                                    CandidateWriterRangeRecipe::None
                                    | CandidateWriterRangeRecipe::Hidden { .. } => false,
                                    CandidateWriterRangeRecipe::Identity => match kind {
                                        CandidateSourceAtomKind::Scalar(_) => false,
                                        CandidateSourceAtomKind::Tab
                                        | CandidateSourceAtomKind::Nul
                                        | CandidateSourceAtomKind::LineEnding(_) => {
                                            return Err(CandidateWriterError::IdentityReplayRequiresTypedRecipe(kind));
                                        }
                                    },
                                    CandidateWriterRangeRecipe::CanonicalText => match kind {
                                        CandidateSourceAtomKind::Scalar(_) => false,
                                        CandidateSourceAtomKind::Tab
                                        | CandidateSourceAtomKind::Nul => true,
                                        CandidateSourceAtomKind::LineEnding(_) => {
                                            return Err(CandidateWriterError::SourceLedger(
                                                SourceBoundLedgerError::RangeReplayUnexpectedAtom,
                                            ));
                                        }
                                    },
                                };
                                if canonical_typed {
                                    if let Some(boundary) = job.last_boundary.take() {
                                        let piece = self.ledger.consume_range_replay_ordinary(
                                            self.epoch, &job.plan, boundary,
                                        )?;
                                        job.source_pieces = job
                                            .source_pieces
                                            .checked_add(1)
                                            .ok_or(CandidateWriterError::Invariant(
                                                "range replay piece count overflow",
                                            ))?;
                                        job.pending_atom = Some(atom);
                                        job.maximum_pending_atoms = 1;
                                        let progress = self.composer_mut()?.push_piece(piece)?;
                                        job.phase = RangeReplayPhase::Drain {
                                            drain: ComposerDrain::begin(progress, false)?,
                                            resume: RangeReplayResume::Scan,
                                        };
                                    } else {
                                        let at_end = atom_end == exact_end;
                                        let piece =
                                            self.ledger.consume_range_replay_canonical_atom(
                                                self.epoch, &job.plan, &atom,
                                            )?;
                                        job.source_pieces = job
                                            .source_pieces
                                            .checked_add(1)
                                            .ok_or(CandidateWriterError::Invariant(
                                                "range replay piece count overflow",
                                            ))?;
                                        if at_end {
                                            job.completion = Some(
                                                self.ledger
                                                    .finish_range_replay(self.epoch, &job.plan)?,
                                            );
                                        }
                                        let progress = self.composer_mut()?.push_piece(piece)?;
                                        job.phase = RangeReplayPhase::Drain {
                                            drain: ComposerDrain::begin(progress, false)?,
                                            resume: if at_end {
                                                RangeReplayResume::Ready
                                            } else {
                                                RangeReplayResume::Scan
                                            },
                                        };
                                    }
                                    began_drain = true;
                                    break;
                                }

                                job.last_boundary = Some(atom.boundary());
                                job.maximum_pending_boundaries = 1;
                                if atom_end == exact_end {
                                    let boundary = job
                                        .last_boundary
                                        .take()
                                        .expect("the endpoint atom minted a boundary");
                                    let piece = self.ledger.consume_range_replay_ordinary(
                                        self.epoch, &job.plan, boundary,
                                    )?;
                                    job.source_pieces = job.source_pieces.checked_add(1).ok_or(
                                        CandidateWriterError::Invariant(
                                            "range replay piece count overflow",
                                        ),
                                    )?;
                                    job.completion = Some(
                                        self.ledger.finish_range_replay(self.epoch, &job.plan)?,
                                    );
                                    let progress = self.composer_mut()?.push_piece(piece)?;
                                    job.phase = RangeReplayPhase::Drain {
                                        drain: ComposerDrain::begin(progress, false)?,
                                        resume: RangeReplayResume::Ready,
                                    };
                                    began_drain = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !began_drain {
                        job.phase = RangeReplayPhase::Scan;
                    }
                }
            }
            RangeReplayPhase::Drain { mut drain, resume } => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    match resume {
                        RangeReplayResume::Scan => RangeReplayPhase::Scan,
                        RangeReplayResume::Ready => RangeReplayPhase::Ready,
                    }
                } else {
                    RangeReplayPhase::Drain { drain, resume }
                };
            }
            RangeReplayPhase::Ready => {
                let completed = job
                    .completion
                    .take()
                    .ok_or(CandidateWriterError::Invariant(
                        "range replay reached Ready without a source receipt",
                    ))?;
                let receipt = CandidateRangeReplayReceipt {
                    source: completed.source(),
                    build: completed.build_id(),
                    line_ordinal: completed.line_ordinal(),
                    absolute_start: completed.absolute_range().0,
                    absolute_end: completed.absolute_range().1,
                    physical_metric: completed.metric(),
                    writer_polls: job.writer_polls,
                    source_work_units: job.source_work_units,
                    source_bytes_read: job.source_bytes_read,
                    atoms_scanned: job.atoms_scanned,
                    source_pieces: job.source_pieces,
                    maximum_pending_atoms: job.maximum_pending_atoms,
                    maximum_pending_boundaries: job.maximum_pending_boundaries,
                };
                return Ok((
                    None,
                    if job.legacy_identity_completion {
                        CandidateWriterProgress::IdentityLineReady { terminator: None }
                    } else {
                        CandidateWriterProgress::RangeReplayReady(receipt)
                    },
                ));
            }
        }
        Ok((
            Some(WriterAction::RangeReplay(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    fn poll_setext(
        &mut self,
        mut job: SetextJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            SetextPhase::RequestStructuralFlush => {
                let progress = self.composer_mut()?.flush_before_structure()?;
                job.phase = SetextPhase::Drain(ComposerDrain::begin(progress, false)?);
            }
            SetextPhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    SetextPhase::BeginGreenPromotion
                } else {
                    SetextPhase::Drain(drain)
                };
            }
            SetextPhase::BeginGreenPromotion => {
                let enter = job.enter.take().ok_or(CandidateWriterError::Invariant(
                    "Setext promotion lost its Paragraph Enter capability",
                ))?;
                let facts = job.facts;
                if job.defer_identity {
                    let replacement = job
                        .replacement_permit
                        .as_ref()
                        .ok_or(CandidateWriterError::Invariant(
                            "deferred normalization lost its fresh replacement permit",
                        ))?
                        .id();
                    self.with_short_session(arena, move |builder, session| {
                        builder.begin_reidentified_setext_promotion(
                            session,
                            enter,
                            replacement,
                            facts,
                        )
                    })?;
                } else {
                    self.with_short_session(arena, move |builder, session| {
                        builder.begin_setext_promotion(session, enter, facts)
                    })?;
                }
                job.phase = SetextPhase::AwaitGreenPromotion;
            }
            SetextPhase::AwaitGreenPromotion => {
                if self.poll_green_acknowledgement(arena)? {
                    let retired_block = job
                        .binding
                        .as_ref()
                        .ok_or(CandidateWriterError::Invariant(
                            "Setext promotion lost its source binding",
                        ))?
                        .binding
                        .block_id();
                    let block = job
                        .replacement_permit
                        .as_ref()
                        .map_or(retired_block, FreshBlockPermit::id);
                    let storage = self.with_short_session(arena, |builder, session| {
                        builder.take_setext_promotion(session, block)
                    })?;
                    job.storage = Some(storage);
                    job.phase = SetextPhase::RetypeLedger;
                }
            }
            SetextPhase::RetypeLedger => {
                #[cfg(test)]
                if self.fail_after_setext_green_ack_before_ledger_retype {
                    self.fail_after_setext_green_ack_before_ledger_retype = false;
                    return Err(CandidateWriterError::InjectedAfterGreenAcknowledgement);
                }
                let binding = job.binding.take().ok_or(CandidateWriterError::Invariant(
                    "Setext promotion lost its source binding",
                ))?;
                let block = binding.binding.block_id();
                let storage = job.storage.take().ok_or(CandidateWriterError::Invariant(
                    "Setext promotion lacks packed-green acknowledgement",
                ))?;
                if let Some(origin) = job.projection_origin.take() {
                    self.composer_mut()?
                        .retire_canonical_fragment_origin(origin)?;
                }
                let (binding, deferred_identity) = if job.defer_identity {
                    let permit =
                        job.replacement_permit
                            .take()
                            .ok_or(CandidateWriterError::Invariant(
                                "deferred normalization lost its replacement permit",
                            ))?;
                    let replacement_block = permit.id();
                    if storage.retired_block() != block
                        || storage.replacement_block() != replacement_block
                    {
                        return Err(CandidateWriterError::Invariant(
                            "deferred normalization storage crossed its identity recipe",
                        ));
                    }
                    let (binding, identity) =
                        self.ledger.replace_top_binding_with_deferred_identity(
                            self.epoch,
                            binding.binding,
                            permit,
                            GreenKind::HEADING,
                        )?;
                    (binding, Some(identity))
                } else {
                    if storage.retired_block() != block || storage.replacement_block() != block {
                        return Err(CandidateWriterError::Invariant(
                            "Setext storage changed identity during in-place promotion",
                        ));
                    }
                    (
                        self.ledger
                            .promote_top_paragraph_to_setext_heading(self.epoch, binding.binding)?,
                        None,
                    )
                };
                if binding.kind() != GreenKind::HEADING
                    || (!job.defer_identity && binding.block_id() != block)
                {
                    return Err(CandidateWriterError::Invariant(
                        "source ledger changed Setext identity during retype",
                    ));
                }
                if self.active_paragraph.is_some() {
                    return Err(CandidateWriterError::Invariant(
                        "Setext promotion would overwrite an active Paragraph group",
                    ));
                }
                if let Some(identity) = deferred_identity.as_ref() {
                    self.with_short_session(arena, |builder, session| {
                        builder.retain_deferred_normalization_target(session, &storage, identity)
                    })?;
                }
                #[cfg(feature = "exact-parser")]
                if job.defer_identity {
                    self.donor_checkpoint_samples
                        .reidentify_promoted_paragraph_group(
                            block,
                            binding.block_id(),
                            job.facts,
                        )?;
                } else {
                    self.donor_checkpoint_samples
                        .promote_paragraph_group(block, job.facts)?;
                }
                self.active_paragraph = Some(ActiveParagraphNormalizationGroup {
                    build: self.epoch.build_id(),
                    block: binding.block_id(),
                    enter: None,
                    projection_origin: None,
                    promoted_setext: true,
                    deferred_identity,
                    deferred_storage: if job.defer_identity {
                        Some(storage)
                    } else {
                        None
                    },
                });
                let binding = CandidateWriterBinding { binding };
                let progress = if job.defer_identity {
                    CandidateWriterProgress::RetypedWithDeferredResidual {
                        binding,
                        facts: job.facts,
                    }
                } else {
                    CandidateWriterProgress::Retyped {
                        binding,
                        facts: job.facts,
                    }
                };
                return Ok((None, progress));
            }
        }
        Ok((
            Some(WriterAction::PromoteSetext(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    fn poll_table_header(
        &mut self,
        mut job: TableHeaderJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            TableHeaderPhase::RequestStructuralFlush => {
                let progress = self.composer_mut()?.flush_before_structure()?;
                job.phase = TableHeaderPhase::Drain(ComposerDrain::begin(progress, false)?);
            }
            TableHeaderPhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    TableHeaderPhase::BeginComposerFragment
                } else {
                    TableHeaderPhase::Drain(drain)
                };
            }
            TableHeaderPhase::BeginComposerFragment => {
                let origin =
                    job.projection_origin
                        .take()
                        .ok_or(CandidateWriterError::Invariant(
                            "table promotion lost its composer fragment origin",
                        ))?;
                self.composer_mut()?
                    .begin_canonical_fragment_replacement(origin, job.expected_physical)?;
                job.phase = TableHeaderPhase::BeginGreenFragment;
            }
            TableHeaderPhase::BeginGreenFragment => {
                let enter = job.enter.take().ok_or(CandidateWriterError::Invariant(
                    "table promotion lost its Paragraph Enter capability",
                ))?;
                let table = job
                    .table_permit
                    .as_ref()
                    .ok_or(CandidateWriterError::Invariant(
                        "table promotion lost its fresh Table identity",
                    ))?
                    .id();
                let expected = job.expected_physical;
                self.with_short_session(arena, move |builder, session| {
                    builder.begin_canonical_fragment_replacement(
                        session,
                        enter,
                        table,
                        GreenKind::TABLE,
                        expected,
                    )
                })?;
                job.phase = TableHeaderPhase::AwaitFragmentStart;
            }
            TableHeaderPhase::AwaitFragmentStart => {
                if self.poll_green_acknowledgement(arena)? {
                    let table = job
                        .table_permit
                        .as_ref()
                        .ok_or(CandidateWriterError::Invariant(
                            "table promotion lost its fresh Table identity",
                        ))?
                        .id();
                    job.phase = TableHeaderPhase::Emit(TableHeaderEventBatch::new(
                        Some(GreenEvent::enter(
                            table,
                            GreenKind::TABLE,
                            job.table_facts.into_envelope(),
                        )),
                        Some(GreenEvent::enter(
                            job.header_block,
                            GreenKind::TABLE_ROW,
                            GreenTableRowOpenFacts::header().into_envelope(),
                        )),
                        None,
                        TableHeaderBatchCompletion::AwaitInput,
                    ));
                }
            }
            TableHeaderPhase::AwaitFragmentInput => {
                return Ok((
                    Some(WriterAction::PromoteTableHeader(job)),
                    CandidateWriterProgress::TableHeaderInputReady,
                ));
            }
            TableHeaderPhase::Emit(mut batch) => {
                if batch.awaiting_ack {
                    if self.poll_green_acknowledgement(arena)? {
                        batch.awaiting_ack = false;
                    }
                    job.phase = TableHeaderPhase::Emit(batch);
                    return Ok((
                        Some(WriterAction::PromoteTableHeader(job)),
                        CandidateWriterProgress::Pending,
                    ));
                }
                if let Some(event) = batch.next_event() {
                    self.with_short_session(arena, move |builder, session| {
                        builder.offer_canonical_fragment_event(session, event)
                    })?;
                    batch.awaiting_ack = true;
                    job.phase = TableHeaderPhase::Emit(batch);
                    return Ok((
                        Some(WriterAction::PromoteTableHeader(job)),
                        CandidateWriterProgress::Pending,
                    ));
                }
                match batch.completion {
                    TableHeaderBatchCompletion::AwaitInput => {
                        job.phase = TableHeaderPhase::AwaitFragmentInput;
                        return Ok((
                            Some(WriterAction::PromoteTableHeader(job)),
                            CandidateWriterProgress::TableHeaderInputReady,
                        ));
                    }
                    TableHeaderBatchCompletion::InstallCell {
                        block,
                        source_start,
                        source_end,
                    } => {
                        // `BeginCell` may first emit an exact physical marker
                        // between the preceding cell and this one.  Reaching
                        // the batch completion proves that marker was
                        // acknowledged, so advance the transaction cursor to
                        // the certified cell start before installing it.
                        if job.active_cell.is_some()
                            || !fragment_metric_precedes(job.cursor, source_start)
                        {
                            return Err(CandidateWriterError::Invariant(
                                "table header cell installation crossed its cursor",
                            ));
                        }
                        job.active_cell = Some(ActiveTableHeaderCell { block, source_end });
                        job.cursor = source_start;
                        job.phase = TableHeaderPhase::AwaitFragmentInput;
                        return Ok((
                            Some(WriterAction::PromoteTableHeader(job)),
                            CandidateWriterProgress::TableHeaderInputReady,
                        ));
                    }
                    TableHeaderBatchCompletion::AdvanceCursor(source_end) => {
                        job.cursor = source_end;
                        job.phase = TableHeaderPhase::AwaitFragmentInput;
                        return Ok((
                            Some(WriterAction::PromoteTableHeader(job)),
                            CandidateWriterProgress::TableHeaderInputReady,
                        ));
                    }
                    TableHeaderBatchCompletion::CloseCell => {
                        job.active_cell
                            .take()
                            .ok_or(CandidateWriterError::Invariant(
                                "table header close acknowledgement lost its cell",
                            ))?;
                        job.next_column = job.next_column.checked_add(1).ok_or(
                            CandidateWriterError::Invariant("table header column count overflow"),
                        )?;
                        job.phase = TableHeaderPhase::AwaitFragmentInput;
                        return Ok((
                            Some(WriterAction::PromoteTableHeader(job)),
                            CandidateWriterProgress::TableHeaderInputReady,
                        ));
                    }
                    TableHeaderBatchCompletion::FinishFragment => {
                        job.cursor = job.expected_physical;
                        job.phase = TableHeaderPhase::BeginFragmentFinish;
                    }
                }
            }
            TableHeaderPhase::BeginFragmentFinish => {
                self.with_short_session(
                    arena,
                    ResumableSerializedGreenBuild::finish_canonical_fragment_replacement,
                )?;
                job.phase = TableHeaderPhase::AwaitFragmentCommit;
            }
            TableHeaderPhase::AwaitFragmentCommit => {
                if self.poll_green_acknowledgement(arena)? {
                    let table = job
                        .table_permit
                        .as_ref()
                        .ok_or(CandidateWriterError::Invariant(
                            "table promotion lost its fresh Table identity",
                        ))?
                        .id();
                    let storage = self.with_short_session(arena, |builder, session| {
                        builder.take_canonical_fragment_replacement(session, table)
                    })?;
                    job.storage = Some(storage);
                    job.phase = TableHeaderPhase::RebaseComposer;
                }
            }
            TableHeaderPhase::RebaseComposer => {
                let storage = job.storage.as_ref().ok_or(CandidateWriterError::Invariant(
                    "table promotion lacks packed-green acknowledgement",
                ))?;
                let projection = self
                    .composer_mut()?
                    .finish_canonical_fragment_replacement(storage)?;
                job.projection = Some(projection);
                job.phase = TableHeaderPhase::RebindLedger;
            }
            TableHeaderPhase::RebindLedger => {
                let paragraph = job.paragraph.take().ok_or(CandidateWriterError::Invariant(
                    "table promotion lost its Paragraph source binding",
                ))?;
                let retired = paragraph.binding.block_id();
                let permit = job
                    .table_permit
                    .take()
                    .ok_or(CandidateWriterError::Invariant(
                        "table promotion lost its Table permit",
                    ))?;
                let storage = job.storage.take().ok_or(CandidateWriterError::Invariant(
                    "table promotion lacks packed-green acknowledgement",
                ))?;
                let projection = job
                    .projection
                    .take()
                    .ok_or(CandidateWriterError::Invariant(
                        "table promotion lacks composer rebase acknowledgement",
                    ))?;
                if storage.retired_block() != retired
                    || storage.replacement_block() != permit.id()
                    || storage.replacement_kind() != GreenKind::TABLE
                    || storage.physical_metric() != job.expected_physical
                    || projection.build_id() != self.epoch.build_id()
                    || projection.physical_metric() != job.expected_physical
                    || projection.retired_projection_runs() != storage.retired_coverage_runs()
                    || projection.installed_projection_runs() != storage.replacement_coverage_runs()
                    || job.cursor != job.expected_physical
                {
                    return Err(CandidateWriterError::Invariant(
                        "table storage acknowledgement crossed the writer join",
                    ));
                }
                self.green_runs_acknowledged = projection.canonical_suffix_projection_runs();
                let binding = self.ledger.replace_top_binding(
                    self.epoch,
                    paragraph.binding,
                    permit,
                    GreenKind::TABLE,
                )?;
                #[cfg(feature = "exact-parser")]
                self.donor_checkpoint_samples
                    .retire_empty_paragraph_group(retired)?;
                if self.active_paragraph.is_some()
                    || binding.kind() != GreenKind::TABLE
                    || binding.block_id() != storage.replacement_block()
                {
                    return Err(CandidateWriterError::Invariant(
                        "table source-ledger replacement changed canonical identity",
                    ));
                }
                return Ok((
                    None,
                    CandidateWriterProgress::RetypedTable {
                        binding: CandidateWriterBinding { binding },
                    },
                ));
            }
        }
        Ok((
            Some(WriterAction::PromoteTableHeader(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    fn poll_close(
        &mut self,
        mut job: CloseJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            ClosePhase::BeginWholeNormalization => {
                let pending =
                    job.whole_normalization
                        .take()
                        .ok_or(CandidateWriterError::Invariant(
                            "whole-normalizing close lost its storage authority",
                        ))?;
                #[cfg(feature = "exact-parser")]
                self.donor_checkpoint_samples
                    .reidentify_finalized_whole_normalization(&pending.identity)?;
                self.with_short_session(arena, move |builder, session| {
                    builder.begin_whole_normalization_reidentity(
                        session,
                        pending.identity,
                        pending.storage,
                    )
                })?;
                job.phase = ClosePhase::AwaitWholeNormalization;
            }
            ClosePhase::AwaitWholeNormalization => {
                if self.poll_green_acknowledgement(arena)? {
                    let _acknowledgement = self.with_short_session(arena, |builder, session| {
                        builder.take_whole_normalization_reidentity(session)
                    })?;
                    job.phase = ClosePhase::RequestStructuralFlush;
                }
            }
            ClosePhase::RequestStructuralFlush => {
                let progress = self.composer_mut()?.flush_before_structure()?;
                job.phase = ClosePhase::Drain(ComposerDrain::begin(progress, false)?);
            }
            ClosePhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    ClosePhase::OfferExit
                } else {
                    ClosePhase::Drain(drain)
                };
            }
            ClosePhase::OfferExit => {
                self.offer_green(
                    arena,
                    GreenEvent::exit_with_state(job.closed, job.last_line_blank, job.facts),
                )?;
                job.phase = ClosePhase::AwaitExitAcknowledgement;
            }
            ClosePhase::AwaitExitAcknowledgement => {
                if self.poll_green_acknowledgement(arena)? {
                    job.phase = ClosePhase::CloseLedger;
                }
            }
            ClosePhase::CloseLedger => {
                #[cfg(test)]
                if self.fail_after_green_ack_before_ledger_close {
                    self.fail_after_green_ack_before_ledger_close = false;
                    return Err(CandidateWriterError::InjectedAfterGreenAcknowledgement);
                }
                self.ledger
                    .close_binding(self.epoch, &job.binding.binding)?;
                if job.closes_active_paragraph {
                    let mut group =
                        self.active_paragraph
                            .take()
                            .ok_or(CandidateWriterError::Invariant(
                                "terminal close lost its active Paragraph group",
                            ))?;
                    if group.build != self.epoch.build_id()
                        || group.block != job.binding.binding.block_id()
                    {
                        return Err(CandidateWriterError::Invariant(
                            "terminal close consumed a different Paragraph group",
                        ));
                    }
                    if let Some(origin) = group.projection_origin.take() {
                        self.composer_mut()?
                            .retire_canonical_fragment_origin(origin)?;
                    }
                    let deferred = match (
                        group.deferred_identity.take(),
                        group.deferred_storage.take(),
                    ) {
                        (Some(identity), Some(storage)) => {
                            if identity.replacement_block() != group.block
                                || storage.replacement_block() != group.block
                                || storage.retired_block() != identity.retired_block()
                                || self.deferred_normalization.is_some()
                            {
                                return Err(CandidateWriterError::Invariant(
                                    "closed normalization group crossed its deferred identity",
                                ));
                            }
                            Some(PendingDeferredNormalization { identity, storage })
                        }
                        (None, None) => None,
                        (Some(_), None) | (None, Some(_)) => {
                            return Err(CandidateWriterError::Invariant(
                                "normalization group lost half of its deferred identity",
                            ));
                        }
                    };
                    #[cfg(feature = "exact-parser")]
                    self.donor_checkpoint_samples
                        .finish_paragraph_group(group.block)?;
                    self.deferred_normalization = deferred;
                }
                return Ok((None, CandidateWriterProgress::ActionComplete));
            }
        }
        Ok((
            Some(WriterAction::Close(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    fn poll_finish(
        &mut self,
        mut job: FinishJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            FinishPhase::BeginWholeNormalization => {
                let pending =
                    job.whole_normalization
                        .take()
                        .ok_or(CandidateWriterError::Invariant(
                            "whole-normalizing finish lost its storage authority",
                        ))?;
                #[cfg(feature = "exact-parser")]
                self.donor_checkpoint_samples
                    .reidentify_finalized_whole_normalization(&pending.identity)?;
                self.with_short_session(arena, move |builder, session| {
                    builder.begin_whole_normalization_reidentity(
                        session,
                        pending.identity,
                        pending.storage,
                    )
                })?;
                job.phase = FinishPhase::AwaitWholeNormalization;
            }
            FinishPhase::AwaitWholeNormalization => {
                if self.poll_green_acknowledgement(arena)? {
                    let _acknowledgement = self.with_short_session(arena, |builder, session| {
                        builder.take_whole_normalization_reidentity(session)
                    })?;
                    let drain = self.begin_finish_drain(self.epoch)?;
                    job.phase = FinishPhase::Drain(drain);
                }
            }
            FinishPhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    FinishPhase::JoinComposerCompletion
                } else {
                    FinishPhase::Drain(drain)
                };
            }
            FinishPhase::JoinComposerCompletion => {
                let composer = self
                    .composer
                    .take()
                    .ok_or(CandidateWriterError::Invariant("composer already consumed"))?
                    .into_completion_seal()?;
                job.composer = Some(composer);
                #[cfg(feature = "exact-parser")]
                {
                    job.phase = FinishPhase::BeginReferenceSemanticFinish;
                }
                #[cfg(not(feature = "exact-parser"))]
                {
                    job.phase = FinishPhase::FinishGreenInput;
                }
            }
            #[cfg(feature = "exact-parser")]
            FinishPhase::BeginReferenceSemanticFinish => {
                self.with_reference_semantic_session(arena, |semantic, session| {
                    semantic.index.capture_checkpoint(session).map_err(|_| {
                        CandidateWriterError::Invariant(
                            "reference checkpoint rejected candidate completion",
                        )
                    })?;
                    semantic.interner.begin_finish().map_err(|_| {
                        CandidateWriterError::Invariant(
                            "reference interner rejected candidate completion",
                        )
                    })
                })?;
                job.phase = FinishPhase::PollReferenceInternerFinish;
            }
            #[cfg(feature = "exact-parser")]
            FinishPhase::PollReferenceInternerFinish => {
                let progress = self.with_reference_semantic_session(
                    arena,
                    |semantic, session| {
                        semantic.interner.poll(session).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference interner failed during candidate completion",
                            )
                        })
                    },
                )?;
                if progress
                    == crate::reference_label_interner::ReferenceLabelInternerProgress::ManifestReady
                {
                    self.with_reference_semantic_session(arena, |semantic, session| {
                        let interner = semantic.interner.take_manifest().map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference interner manifest disappeared",
                            )
                        })?;
                        semantic.index.begin_finish(session, interner).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference index rejected interner completion",
                            )
                        })
                    })?;
                    job.phase = FinishPhase::PollReferenceIndexFinish;
                }
            }
            #[cfg(feature = "exact-parser")]
            FinishPhase::PollReferenceIndexFinish => {
                let progress = self.with_reference_semantic_session(
                    arena,
                    |semantic, session| {
                        semantic.index.poll(session).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference index failed during candidate completion",
                            )
                        })
                    },
                )?;
                if progress
                    == crate::reference_restart_index::ReferenceCandidateIndexProgress::ManifestReady
                {
                    let manifest = self.with_reference_semantic_session(
                        arena,
                        |semantic, _| {
                            semantic.index.take_manifest().map_err(|_| {
                                CandidateWriterError::Invariant(
                                    "reference index manifest disappeared",
                                )
                            })
                        },
                    )?;
                    let _consumed = self.reference_semantic.take().ok_or(
                        CandidateWriterError::Invariant(
                            "completed reference semantic state disappeared",
                        ),
                    )?;
                    job.reference = Some(manifest);
                    job.phase = FinishPhase::FinishGreenInput;
                }
            }
            FinishPhase::FinishGreenInput => {
                self.with_short_session(arena, ResumableSerializedGreenBuild::finish_input)?;
                job.phase = FinishPhase::AwaitManifest;
            }
            FinishPhase::AwaitManifest => {
                let progress = self.poll_green_builder(arena)?;
                if progress == SerializedGreenStreamProgress::ManifestReady {
                    job.phase = FinishPhase::JoinCompletion;
                } else if progress == SerializedGreenStreamProgress::ReadyForEvent {
                    return Err(CandidateWriterError::Invariant(
                        "finished green builder requested another event",
                    ));
                }
            }
            FinishPhase::JoinCompletion => {
                let builder = self.builder.take().ok_or(CandidateWriterError::Invariant(
                    "green builder already consumed",
                ))?;
                let green = builder.take_manifest()?;
                let composer = job
                    .composer
                    .as_ref()
                    .ok_or(CandidateWriterError::Invariant(
                        "composer completion missing",
                    ))?;
                let ticket_id = match &self.lease {
                    CandidateWriterBuildLease::Suspended(ticket) => ticket.id(),
                    CandidateWriterBuildLease::Aborting(build) => {
                        return Err(CandidateWriterError::ArenaBuild(
                            ArenaBuildError::BuildAborting(*build),
                        ));
                    }
                    CandidateWriterBuildLease::Joined => {
                        return Err(CandidateWriterError::CompletionAlreadyReady);
                    }
                };
                if green.build_id() != composer.build_id()
                    || green.build_id() != ticket_id
                    || composer.source() != self.epoch.source()
                {
                    return Err(CandidateWriterError::WrongCandidate);
                }
                // Every fallible check above only borrowed the ticket. From
                // here through completion installation there are only
                // infallible moves, so a suspended build cannot lose abort
                // authority on an invariant failure.
                let ticket =
                    match std::mem::replace(&mut self.lease, CandidateWriterBuildLease::Joined) {
                        CandidateWriterBuildLease::Suspended(ticket) => ticket,
                        CandidateWriterBuildLease::Aborting(_)
                        | CandidateWriterBuildLease::Joined => {
                            unreachable!("lease was validated immediately before extraction")
                        }
                    };
                let composer = job
                    .composer
                    .take()
                    .expect("composer was validated immediately before extraction");
                self.completion = Some(CandidateWriterCompletionSeal {
                    epoch: self.epoch,
                    composer,
                    green,
                    #[cfg(feature = "exact-parser")]
                    reference: job.reference.take().ok_or(CandidateWriterError::Invariant(
                        "candidate completion lost its reference manifest",
                    ))?,
                    ticket,
                    green_runs_acknowledged: self.green_runs_acknowledged,
                });
                return Ok((None, CandidateWriterProgress::CompletionReady));
            }
        }
        Ok((
            Some(WriterAction::Finish(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    #[cfg(feature = "exact-parser")]
    fn poll_line_boundary_checkpoint(
        &mut self,
        mut job: LineBoundaryCheckpointJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            LineBoundaryCheckpointPhase::RequestDedicatedDrain => {
                let progress = self.composer_mut()?.flush_for_line_boundary_checkpoint()?;
                job.phase =
                    LineBoundaryCheckpointPhase::Drain(ComposerDrain::begin(progress, false)?);
            }
            LineBoundaryCheckpointPhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    LineBoundaryCheckpointPhase::PrepareGreenCut
                } else {
                    LineBoundaryCheckpointPhase::Drain(drain)
                };
            }
            LineBoundaryCheckpointPhase::PrepareGreenCut => {
                if let Some(storage) = self.last_line_boundary_storage.take() {
                    let current = self
                        .builder
                        .as_ref()
                        .ok_or(CandidateWriterError::Invariant(
                            "line-boundary checkpoint lost green builder",
                        ))?
                        .line_boundary_cut_is_current(storage.green_cut().ok_or(
                            CandidateWriterError::Invariant(
                                "production line-boundary storage lacks green cut",
                            ),
                        )?);
                    if current {
                        job.green_prefix_snapshot = self
                            .capture_green_prefix_snapshot_for_checkpoint(
                                arena,
                                job.capture_green_prefix_snapshot,
                                &storage,
                            )?;
                        let continuation = self.pause_checkpoint_composer(storage)?;
                        job.phase = LineBoundaryCheckpointPhase::Ready(Some(continuation));
                        return Ok((
                            Some(WriterAction::LineBoundaryCheckpoint(job)),
                            CandidateWriterProgress::LineBoundaryCheckpointReady,
                        ));
                    }
                }

                let has_partial = self
                    .builder
                    .as_ref()
                    .ok_or(CandidateWriterError::Invariant(
                        "line-boundary checkpoint lost green builder",
                    ))?
                    .has_partial_line_boundary_events();
                if has_partial {
                    self.with_short_session(arena, |builder, session| {
                        builder.begin_leaf_barrier(session)
                    })?;
                    job.phase = LineBoundaryCheckpointPhase::AwaitGreenBarrier;
                } else {
                    let cut = self.with_short_session(arena, |builder, session| {
                        builder.take_natural_line_boundary_cut(session)
                    })?;
                    let storage =
                        SourceProjectionLineBoundaryStorageAck::from_green_cut(self.epoch, cut)?;
                    job.green_prefix_snapshot = self.capture_green_prefix_snapshot_for_checkpoint(
                        arena,
                        job.capture_green_prefix_snapshot,
                        &storage,
                    )?;
                    let continuation = self.pause_checkpoint_composer(storage)?;
                    job.phase = LineBoundaryCheckpointPhase::Ready(Some(continuation));
                    return Ok((
                        Some(WriterAction::LineBoundaryCheckpoint(job)),
                        CandidateWriterProgress::LineBoundaryCheckpointReady,
                    ));
                }
            }
            LineBoundaryCheckpointPhase::AwaitGreenBarrier => {
                match self.poll_green_builder(arena)? {
                    SerializedGreenStreamProgress::Pending => {}
                    SerializedGreenStreamProgress::ReadyForEvent => {
                        let cut = self.with_short_session(arena, |builder, session| {
                            builder.take_leaf_barrier_cut(session)
                        })?;
                        let storage = SourceProjectionLineBoundaryStorageAck::from_green_cut(
                            self.epoch, cut,
                        )?;
                        job.green_prefix_snapshot = self
                            .capture_green_prefix_snapshot_for_checkpoint(
                                arena,
                                job.capture_green_prefix_snapshot,
                                &storage,
                            )?;
                        let continuation = self.pause_checkpoint_composer(storage)?;
                        job.phase = LineBoundaryCheckpointPhase::Ready(Some(continuation));
                        return Ok((
                            Some(WriterAction::LineBoundaryCheckpoint(job)),
                            CandidateWriterProgress::LineBoundaryCheckpointReady,
                        ));
                    }
                    SerializedGreenStreamProgress::ManifestReady => {
                        return Err(CandidateWriterError::Invariant(
                            "line-boundary barrier reached a finished manifest",
                        ));
                    }
                }
            }
            LineBoundaryCheckpointPhase::Ready(_) => {
                return Ok((
                    Some(WriterAction::LineBoundaryCheckpoint(job)),
                    CandidateWriterProgress::LineBoundaryCheckpointReady,
                ));
            }
        }
        Ok((
            Some(WriterAction::LineBoundaryCheckpoint(job)),
            CandidateWriterProgress::Pending,
        ))
    }

    #[cfg(feature = "exact-parser")]
    fn pause_checkpoint_composer(
        &mut self,
        storage: SourceProjectionLineBoundaryStorageAck,
    ) -> Result<SourceProjectionComposerLineBoundaryContinuation, CandidateWriterError> {
        self.composer
            .take()
            .ok_or(CandidateWriterError::Invariant(
                "line-boundary checkpoint already consumed composer",
            ))?
            .pause_at_line_boundary(storage)
            .map_err(Into::into)
    }

    #[cfg(feature = "exact-parser")]
    fn capture_green_prefix_snapshot_for_checkpoint(
        &mut self,
        arena: &mut PageArena,
        requested: bool,
        storage: &SourceProjectionLineBoundaryStorageAck,
    ) -> Result<Option<BuilderGreenPrefixSnapshot>, CandidateWriterError> {
        if !requested {
            return Ok(None);
        }
        let cut = storage.green_cut().ok_or(CandidateWriterError::Invariant(
            "convergence checkpoint lacks an exact green cut",
        ))?;
        self.with_short_session(arena, |builder, session| {
            builder.capture_builder_green_prefix_snapshot(session, cut)
        })
        .map(Some)
    }

    fn poll_drain(
        &mut self,
        drain: &mut ComposerDrain,
        arena: &mut PageArena,
    ) -> Result<(), CandidateWriterError> {
        let state = std::mem::replace(&mut drain.state, DrainState::Complete);
        drain.state = match state {
            DrainState::NeedRunSeal => {
                let permit = self.mint_coverage()?;
                let run = self.composer_mut()?.seal_pending_run(permit)?;
                DrainState::SealedRun(run)
            }
            DrainState::SealedRun(run) => {
                self.offer_green(arena, GreenEvent::Coverage(run.into_run()))?;
                DrainState::AwaitGreenAcknowledgement
            }
            DrainState::AwaitGreenAcknowledgement => {
                if self.poll_green_acknowledgement(arena)? {
                    self.green_runs_acknowledged = self
                        .green_runs_acknowledged
                        .checked_add(1)
                        .ok_or(CandidateWriterError::Invariant(
                            "green run acknowledgement count overflow",
                        ))?;
                    DrainState::NeedComposerPoll
                } else {
                    DrainState::AwaitGreenAcknowledgement
                }
            }
            DrainState::NeedComposerPoll => {
                let progress = self.composer_mut()?.poll()?;
                match progress {
                    SourceProjectionComposerProgress::RunReady => DrainState::NeedRunSeal,
                    SourceProjectionComposerProgress::Idle if !drain.document_finish => {
                        DrainState::Complete
                    }
                    SourceProjectionComposerProgress::Complete(_) if drain.document_finish => {
                        DrainState::Complete
                    }
                    SourceProjectionComposerProgress::Idle
                    | SourceProjectionComposerProgress::Complete(_) => {
                        return Err(CandidateWriterError::Invariant(
                            "composer drain ended in the wrong mode",
                        ));
                    }
                }
            }
            DrainState::Complete => DrainState::Complete,
        };
        Ok(())
    }

    fn offer_green(
        &mut self,
        arena: &mut PageArena,
        event: GreenEvent,
    ) -> Result<(), CandidateWriterError> {
        self.with_short_session(arena, move |builder, session| {
            builder.offer_event(session, event)
        })
    }

    /// Returns true only on the builder's explicit sink acknowledgement.
    fn poll_green_acknowledgement(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<bool, CandidateWriterError> {
        match self.poll_green_builder(arena)? {
            SerializedGreenStreamProgress::ReadyForEvent => Ok(true),
            SerializedGreenStreamProgress::Pending => Ok(false),
            SerializedGreenStreamProgress::ManifestReady => Err(CandidateWriterError::Invariant(
                "green manifest became ready before input finish",
            )),
        }
    }

    fn poll_green_builder(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<SerializedGreenStreamProgress, CandidateWriterError> {
        self.with_short_session(arena, ResumableSerializedGreenBuild::poll)
    }

    fn with_short_session<T, E>(
        &mut self,
        arena: &mut PageArena,
        operation: impl FnOnce(
            &mut ResumableSerializedGreenBuild,
            &mut crate::ArenaBuildSession<'_>,
        ) -> Result<T, E>,
    ) -> Result<T, CandidateWriterError>
    where
        CandidateWriterError: From<E>,
    {
        let ticket = match std::mem::replace(&mut self.lease, CandidateWriterBuildLease::Joined) {
            CandidateWriterBuildLease::Suspended(ticket) => ticket,
            CandidateWriterBuildLease::Aborting(build) => {
                self.lease = CandidateWriterBuildLease::Aborting(build);
                return Err(CandidateWriterError::ArenaBuild(
                    ArenaBuildError::BuildAborting(build),
                ));
            }
            CandidateWriterBuildLease::Joined => {
                return Err(CandidateWriterError::CompletionAlreadyReady);
            }
        };
        let mut session = match arena.resume_build(ticket) {
            Ok(session) => session,
            Err(failure) => {
                self.lease = CandidateWriterBuildLease::Suspended(failure.ticket);
                return Err(failure.error.into());
            }
        };
        let build = session.id();
        let result = {
            let builder = self
                .builder
                .as_mut()
                .ok_or(CandidateWriterError::Invariant("green builder missing"))?;
            operation(builder, &mut session)
        };
        match session.suspend() {
            Ok(ticket) => self.lease = CandidateWriterBuildLease::Suspended(ticket),
            Err(error) => {
                self.lease = CandidateWriterBuildLease::Aborting(build);
                self.poisoned = true;
                return Err(error.into());
            }
        }
        result.map_err(CandidateWriterError::from)
    }

    fn mint_block(&mut self) -> Result<FreshBlockPermit, CandidateWriterError> {
        match self.identities.mint_block(self.epoch.build_id()) {
            Ok(permit) => Ok(permit),
            Err(LiveDocumentError::IdentityExhausted(kind)) => {
                Err(CandidateWriterError::IdentityExhausted(kind))
            }
            Err(_) => Err(CandidateWriterError::Invariant(
                "identity allocator returned a non-identity error",
            )),
        }
    }

    fn mint_coverage(&mut self) -> Result<crate::FreshCoveragePermit, CandidateWriterError> {
        match self.identities.mint_coverage(self.epoch.build_id()) {
            Ok(permit) => Ok(permit),
            Err(LiveDocumentError::IdentityExhausted(kind)) => {
                Err(CandidateWriterError::IdentityExhausted(kind))
            }
            Err(_) => Err(CandidateWriterError::Invariant(
                "identity allocator returned a non-identity error",
            )),
        }
    }

    fn validate_active_paragraph_close(
        &mut self,
        binding: &CandidateWriterBinding,
    ) -> Result<bool, CandidateWriterError> {
        let kind = binding.kind();
        let block = binding.binding.block_id();
        let validation = match (kind, self.active_paragraph.as_ref()) {
            (GreenKind::PARAGRAPH, Some(group))
                if group.build == self.epoch.build_id()
                    && group.block == block
                    && group.enter.is_some()
                    && !group.promoted_setext =>
            {
                Ok(true)
            }
            (GreenKind::HEADING, Some(group))
                if group.build == self.epoch.build_id()
                    && group.block == block
                    && group.enter.is_none()
                    && group.promoted_setext =>
            {
                Ok(true)
            }
            (GreenKind::HEADING, None) => Ok(false),
            (GreenKind::PARAGRAPH | GreenKind::HEADING, _) => Err(CandidateWriterError::Invariant(
                "terminal close disagrees with active Paragraph group",
            )),
            (_, _) => Ok(false),
        };
        validation.map_err(|error| self.poison(error))
    }

    fn composer_mut(&mut self) -> Result<&mut SourceBoundProjectionComposer, CandidateWriterError> {
        self.composer
            .as_mut()
            .ok_or(CandidateWriterError::CompletionAlreadyReady)
    }

    fn require_ready(&self, epoch: LiveCandidateEpoch) -> Result<(), CandidateWriterError> {
        if self.poisoned {
            return Err(CandidateWriterError::WriterPoisoned);
        }
        if epoch != self.epoch {
            return Err(CandidateWriterError::WrongCandidate);
        }
        if self.completion.is_some() {
            return Err(CandidateWriterError::CompletionAlreadyReady);
        }
        Ok(())
    }

    fn require_start(&self, epoch: LiveCandidateEpoch) -> Result<(), CandidateWriterError> {
        self.require_action_slot(epoch)?;
        if self.issued_source_transition.is_some() {
            return Err(CandidateWriterError::SourceAtomOutstanding);
        }
        Ok(())
    }

    fn require_observation_slot(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)
    }

    fn take_issued_atom(
        &mut self,
        atom: CandidateWriterSourceAtom,
    ) -> Result<CandidateSourceAtom, CandidateWriterError> {
        let expected = self
            .issued_source_transition
            .take()
            .ok_or_else(|| self.poison(CandidateWriterError::NoSourceAtomOutstanding))?;
        if expected != atom.transition {
            return Err(self.poison(CandidateWriterError::ReplayedSourceAtom));
        }
        Ok(atom.atom)
    }

    fn issue_source_atom(
        &mut self,
        atom: CandidateSourceAtom,
    ) -> Result<CandidateWriterSourceAtom, CandidateWriterError> {
        if self.issued_source_transition.is_some() {
            return Err(self.poison(CandidateWriterError::SourceAtomOutstanding));
        }
        let transition = self.next_source_transition;
        self.next_source_transition = transition
            .checked_add(1)
            .ok_or_else(|| self.poison(CandidateWriterError::SourceTransitionExhausted))?;
        self.issued_source_transition = Some(transition);
        Ok(CandidateWriterSourceAtom { transition, atom })
    }

    fn require_action_slot(&self, epoch: LiveCandidateEpoch) -> Result<(), CandidateWriterError> {
        self.require_ready(epoch)?;
        if self.action.is_some() {
            return Err(CandidateWriterError::Busy);
        }
        Ok(())
    }

    fn poison(&mut self, error: CandidateWriterError) -> CandidateWriterError {
        self.poisoned = true;
        error
    }

    /// Commits the first restart-authoritative v2 parent. The actor-derived
    /// sample chain, source/composer completion, and green manifest meet only
    /// here; the checkpoint index cannot be supplied independently.
    ///
    /// The returned allocator is the existing same-live-actor allocator.
    /// There is deliberately no count-based reconstruction or actor-loss
    /// reload path for this committed parent.
    #[cfg(feature = "exact-parser")]
    #[cfg_attr(not(test), allow(dead_code))]
    #[allow(clippy::too_many_lines)] // Keeps every linear commit/abort ownership branch together.
    pub(crate) fn commit_restart_composite(
        mut self,
        arena: &mut PageArena,
        samples: RestartCheckpointSampleChain,
    ) -> Result<
        (
            RestartCompositeDocument,
            RestartCompositeDocumentBuildReceipt,
            DocumentIdentityAllocator,
        ),
        CandidateWriterLocalCommitFailure,
    > {
        if let Err(error) = self.completion_route.require_restart_composite_commit() {
            return Err(self.into_commit_failure(error));
        }
        if self.poisoned || self.action.is_some() || self.issued_source_transition.is_some() {
            return Err(self.into_commit_failure(CandidateWriterError::WriterPoisoned));
        }
        let Some(completion) = self.completion.as_ref() else {
            return Err(self.into_commit_failure(CandidateWriterError::NoAction));
        };
        let canonical_suffix_green_runs =
            match completion.composer.receipt().canonical_projection_runs() {
                Ok(runs) => runs,
                Err(error) => {
                    return Err(self.into_commit_failure(CandidateWriterError::Projection(error)));
                }
            };
        if completion.epoch != self.epoch
            || completion.green.build_id() != completion.ticket.id()
            || completion.composer.build_id() != completion.ticket.id()
            || canonical_suffix_green_runs != completion.green_runs_acknowledged
        {
            return Err(self.into_commit_failure(CandidateWriterError::WrongCandidate));
        }
        let cumulative_green_runs = match completion.composer.cumulative_projection_runs() {
            Ok(runs) => runs,
            Err(error) => {
                return Err(self.into_commit_failure(CandidateWriterError::Projection(error)));
            }
        };
        if let Err(error) =
            samples.validate_actor_completion(self.epoch, &self.donor_checkpoint_samples)
        {
            return Err(self.into_commit_failure(error));
        }

        let completion = self
            .completion
            .take()
            .expect("completion was borrowed immediately before extraction");
        let CandidateWriterCompletionSeal {
            epoch,
            composer,
            green,
            reference: _reference,
            ticket,
            green_runs_acknowledged,
        } = completion;
        let build = ticket.id();
        let mut session = match arena.resume_build(ticket) {
            Ok(session) => session,
            Err(failure) => {
                return Err(CandidateWriterLocalCommitFailure {
                    error: failure.error.into(),
                    abort: CandidateWriterAbortLease::Suspended(failure.ticket),
                    identities: self.identities,
                });
            }
        };

        let parent = (|| -> Result<_, CandidateWriterError> {
            let green_descriptor = green.composite_descriptor(&session)?;
            let source = composer.source();
            let source_metric = composer.metric();
            let source_bytes = u64::try_from(source.bytes)
                .map_err(|_| CandidateWriterError::Invariant("source bytes exceed u64"))?;
            if epoch != self.epoch
                || source != epoch.source()
                || green_descriptor.source_revision() != source.revision
                || green_descriptor.source_root() != source.root
                || green_descriptor.source_metric().bytes != source_bytes
                || green_descriptor.source_metric().utf16 != source_metric.utf16()
                || source_metric.bytes() != source_bytes
                || green_descriptor.parse_generation() != epoch.parse_token().generation
                || composer.receipt().canonical_projection_runs()? != green_runs_acknowledged
                || green_descriptor.coverage_count() != cumulative_green_runs
            {
                return Err(CandidateWriterError::Invariant(
                    "completed source, composer, green, and actor generations disagree",
                ));
            }

            let final_measure = RelativeCheckpointMeasure::new(
                source_metric.bytes(),
                source_metric.utf16(),
                samples.physical_lines(),
                green_descriptor.tokens(),
                green_descriptor.coverage_count(),
            );
            let accumulator = std::mem::replace(
                &mut self.donor_checkpoint_samples,
                DonorCheckpointSampleAccumulator::after_unseeded_retained_prefix(),
            );
            let checkpoint_builder = accumulator.into_checkpoint_index_builder(final_measure)?;
            let checkpoint_index = checkpoint_builder.build_in_session(&mut session)?;
            if checkpoint_index
                .composite_descriptor(&session)?
                .final_measure()
                != final_measure
            {
                return Err(CandidateWriterError::Invariant(
                    "completed checkpoint index lost the actor-derived final measure",
                ));
            }
            let children =
                RestartCompositeChildren::mint_from_completed_candidate(green, checkpoint_index);
            RestartCompositeDocumentBuilder::join(&mut session, children).map_err(Into::into)
        })();

        let parent = match parent {
            Ok(parent) => parent,
            Err(error) => {
                // Dropping a resumed unfinished session transfers the journal
                // to arena-owned abort; both children and any partial parent
                // remain covered by that single cancellation identity.
                drop(session);
                return Err(CandidateWriterLocalCommitFailure {
                    error,
                    abort: CandidateWriterAbortLease::AlreadyAborting(build),
                    identities: self.identities,
                });
            }
        };
        let (document, receipt) = match parent.commit(session) {
            Ok(committed) => committed,
            Err(error) => {
                return Err(CandidateWriterLocalCommitFailure {
                    error: error.into(),
                    abort: CandidateWriterAbortLease::AlreadyAborting(build),
                    identities: self.identities,
                });
            }
        };
        Ok((document, receipt, self.identities))
    }

    /// Local mechanism commit only. No raw manifest or ticket can be extracted
    /// from the completion seal; this function consumes the joined seal and
    /// commits the sole green owner in the same transition.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn commit_local(
        mut self,
        arena: &mut PageArena,
    ) -> Result<
        (CandidateWriterBuiltDocument, DocumentIdentityAllocator),
        CandidateWriterLocalCommitFailure,
    > {
        if let Err(error) = self.completion_route.require_local_commit() {
            return Err(self.into_commit_failure(error));
        }
        if self.poisoned || self.action.is_some() || self.issued_source_transition.is_some() {
            return Err(self.into_commit_failure(CandidateWriterError::WriterPoisoned));
        }
        let Some(completion) = self.completion.as_ref() else {
            return Err(self.into_commit_failure(CandidateWriterError::NoAction));
        };
        let canonical_suffix_green_runs =
            match completion.composer.receipt().canonical_projection_runs() {
                Ok(runs) => runs,
                Err(error) => {
                    return Err(self.into_commit_failure(CandidateWriterError::Projection(error)));
                }
            };
        if completion.epoch != self.epoch
            || completion.green.build_id() != completion.ticket.id()
            || completion.composer.build_id() != completion.ticket.id()
            || canonical_suffix_green_runs != completion.green_runs_acknowledged
        {
            return Err(self.into_commit_failure(CandidateWriterError::WrongCandidate));
        }
        if let Err(error) = completion.composer.cumulative_projection_runs() {
            return Err(self.into_commit_failure(CandidateWriterError::Projection(error)));
        }
        let completion = self
            .completion
            .take()
            .expect("completion was borrowed immediately before extraction");
        let build = completion.ticket.id();
        let session = match arena.resume_build(completion.ticket) {
            Ok(session) => session,
            Err(failure) => {
                return Err(CandidateWriterLocalCommitFailure {
                    error: failure.error.into(),
                    abort: CandidateWriterAbortLease::Suspended(failure.ticket),
                    identities: self.identities,
                });
            }
        };
        #[cfg(feature = "exact-parser")]
        {
            let mut session = session;
            let mut mint = ReferenceCandidateIndexWriterMint(());
            let parent = match join_green_reference_children(
                &mut session,
                completion.green,
                completion.reference,
                &mut mint,
            ) {
                Ok(parent) => parent,
                Err(error) => {
                    drop(session);
                    return Err(CandidateWriterLocalCommitFailure {
                        error: error.into(),
                        abort: CandidateWriterAbortLease::AlreadyAborting(build),
                        identities: self.identities,
                    });
                }
            };
            let green_receipt = parent.receipt().green;
            let composite = match parent.commit(session, &mut mint) {
                Ok(document) => document,
                Err(error) => {
                    return Err(CandidateWriterLocalCommitFailure {
                        error: error.into(),
                        abort: CandidateWriterAbortLease::AlreadyAborting(build),
                        identities: self.identities,
                    });
                }
            };
            return Ok((
                CandidateWriterBuiltDocument {
                    composite,
                    composer: completion.composer,
                    green_receipt,
                    green_runs_acknowledged: completion.green_runs_acknowledged,
                },
                self.identities,
            ));
        }
        #[cfg(not(feature = "exact-parser"))]
        {
            let (green, green_receipt) = match completion.green.commit(session) {
                Ok(committed) => committed,
                Err(error) => {
                    // The failed commit consumed the resumed session. Its Drop
                    // transition has already moved the journal to arena-owned
                    // Aborting, so the failure returns abort identity, not a fake
                    // or separately committable ticket.
                    return Err(CandidateWriterLocalCommitFailure {
                        error: error.into(),
                        abort: CandidateWriterAbortLease::AlreadyAborting(build),
                        identities: self.identities,
                    });
                }
            };
            Ok((
                CandidateWriterBuiltDocument {
                    green,
                    composer: completion.composer,
                    green_receipt,
                    green_runs_acknowledged: completion.green_runs_acknowledged,
                },
                self.identities,
            ))
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn into_commit_failure(self, error: CandidateWriterError) -> CandidateWriterLocalCommitFailure {
        let (abort, identities) = self.into_abort_parts();
        CandidateWriterLocalCommitFailure {
            error,
            abort,
            identities,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn into_abort_parts(
        mut self,
    ) -> (CandidateWriterAbortLease, DocumentIdentityAllocator) {
        if let Some(completion) = self.completion.take() {
            return (
                CandidateWriterAbortLease::Suspended(completion.ticket),
                self.identities,
            );
        }
        let lease = match std::mem::replace(&mut self.lease, CandidateWriterBuildLease::Joined) {
            CandidateWriterBuildLease::Suspended(ticket) => {
                CandidateWriterAbortLease::Suspended(ticket)
            }
            CandidateWriterBuildLease::Aborting(build) => {
                CandidateWriterAbortLease::AlreadyAborting(build)
            }
            CandidateWriterBuildLease::Joined => {
                CandidateWriterAbortLease::AlreadyAborting(self.epoch.build_id())
            }
        };
        (lease, self.identities)
    }

    pub(crate) fn begin_abort(
        &mut self,
        arena: &mut PageArena,
    ) -> Result<ArenaBuildId, ArenaBuildError> {
        if let Some(mut completion) = self.completion.take() {
            return match arena.begin_build_abort(completion.ticket) {
                Ok(build) => {
                    self.lease = CandidateWriterBuildLease::Aborting(build);
                    Ok(build)
                }
                Err(failure) => {
                    completion.ticket = failure.ticket;
                    self.completion = Some(completion);
                    Err(failure.error)
                }
            };
        }
        match std::mem::replace(&mut self.lease, CandidateWriterBuildLease::Joined) {
            CandidateWriterBuildLease::Suspended(ticket) => match arena.begin_build_abort(ticket) {
                Ok(build) => {
                    self.lease = CandidateWriterBuildLease::Aborting(build);
                    Ok(build)
                }
                Err(failure) => {
                    self.lease = CandidateWriterBuildLease::Suspended(failure.ticket);
                    Err(failure.error)
                }
            },
            CandidateWriterBuildLease::Aborting(build) => {
                self.lease = CandidateWriterBuildLease::Aborting(build);
                Ok(build)
            }
            CandidateWriterBuildLease::Joined => {
                let build = self.epoch.build_id();
                self.lease = CandidateWriterBuildLease::Aborting(build);
                Ok(build)
            }
        }
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn into_identities_after_abort(
        self,
    ) -> (DocumentIdentityAllocator, CandidateWriterHeapRetirement) {
        debug_assert!(matches!(self.lease, CandidateWriterBuildLease::Aborting(_)));
        let retirement = self.donor_checkpoint_samples.into_heap_retirement();
        (self.identities, retirement)
    }

    #[cfg(not(feature = "exact-parser"))]
    pub(crate) fn into_identities_after_abort(self) -> DocumentIdentityAllocator {
        debug_assert!(matches!(self.lease, CandidateWriterBuildLease::Aborting(_)));
        self.identities
    }

    #[cfg(test)]
    pub(crate) fn inject_failure_after_green_ack_before_ledger_close(&mut self) {
        self.fail_after_green_ack_before_ledger_close = true;
    }

    #[cfg(test)]
    pub(crate) fn inject_failure_after_setext_green_ack_before_ledger_retype(&mut self) {
        self.fail_after_setext_green_ack_before_ledger_retype = true;
    }

    #[cfg(test)]
    pub(crate) fn force_active_setext_deferred_identity_for_test(
        &mut self,
    ) -> Result<(), CandidateWriterError> {
        let origin = self
            .active_paragraph
            .as_mut()
            .and_then(|group| group.projection_origin.as_mut())
            .ok_or(CandidateWriterError::Invariant(
                "forced deferred Setext requires an active Paragraph origin",
            ))?;
        origin
            .mark_crossing_parent_selected_restart_for_test()
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn cross_pending_deferred_setext_storage_for_test(
        &mut self,
    ) -> Result<(), CandidateWriterError> {
        self.deferred_normalization
            .as_mut()
            .ok_or(CandidateWriterError::Invariant(
                "crossed deferred Setext test requires a closed pending normalization",
            ))?
            .storage
            .cross_identity_for_test();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtomicProjectionKind, BlockId, GreenFenceCharacter, GreenListBullet,
        GreenRelativeLogicalSlice, SerializedGreenTestEvent, SerializedGreenTestLogical,
        serialized_green_test_close_facts, serialized_green_test_trace,
    };

    const CONFIG: CandidateWriterConfig = CandidateWriterConfig {
        syntax_profile: 1,
        grammar_revision: GrammarRevision(1),
        semantic_epoch: 1,
    };

    fn document(text: &str) -> (crate::LiveDocumentStore, LiveCandidateEpoch) {
        let mut document = crate::LiveDocumentStore::new(text, 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        document.activate_candidate_writer(epoch, CONFIG).unwrap();
        (document, epoch)
    }

    #[test]
    fn parent_selected_completion_route_rejects_both_independent_commits() {
        let route = CandidateWriterCompletionRoute::ParentSelectedAdoption;
        assert_eq!(
            route.require_restart_composite_commit(),
            Err(CandidateWriterError::Invariant(
                "parent-selected writer requires retained checkpoint-index splice and adoption"
            ))
        );
        assert_eq!(
            route.require_local_commit(),
            Err(CandidateWriterError::Invariant(
                "parent-selected writer cannot enter independent green commit"
            ))
        );

        assert_eq!(
            CandidateWriterCompletionRoute::Independent.require_restart_composite_commit(),
            Ok(())
        );
        assert_eq!(
            CandidateWriterCompletionRoute::Independent.require_local_commit(),
            Ok(())
        );
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn donor_sample_accumulator_rejects_crossed_cursor_and_backward_cut() {
        let (_document, epoch) = document("");
        let cut = RelativeCheckpointMeasure::new(20, 18, 4, 8, 6);
        let expected = DonorCheckpointSampleCursorState {
            epoch,
            sample_ordinal: 3,
            cumulative_cut: cut,
        };
        let accumulator = DonorCheckpointSampleAccumulator {
            chain: DonorCheckpointSampleChainState::DocumentOrigin {
                expected: Some(expected),
            },
            samples: Vec::new(),
            sample_total: RelativeCheckpointMeasure::default(),
            maximum_path_depth: 0,
            normalization_spans: Vec::new(),
            open_paragraph: None,
        };
        let crossed = DonorCheckpointSampleCursorState {
            sample_ordinal: 2,
            ..expected
        };
        assert_eq!(
            accumulator.validate_cursor(epoch, crossed),
            Err(CandidateWriterError::Invariant(
                "crossed donor checkpoint sample cursor"
            ))
        );

        let backward = RelativeCheckpointMeasure::new(19, 18, 5, 9, 7);
        assert_eq!(
            expected.interval_to(backward),
            Err(CandidateWriterError::CheckpointIndex(
                CommittedCheckpointIndexError::Invalid("checkpoint source-byte cut regresses")
            ))
        );
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn table_fragment_checkpoint_scope_retires_only_an_unsampled_paragraph_group() {
        use crate::committed_checkpoint_index::CommittedDonorCheckpointRole;
        use flark_comrak_value_block_core::{DirectDurableGrammarCapture, DirectPollStatus};

        fn donor_after_line(line: &str) -> DirectDurableGrammarCapture {
            let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
            parser.acknowledge_command().unwrap();
            parser.begin_line(line.to_owned()).unwrap();
            for _ in 0..512 {
                match parser.poll_line(1).unwrap().status {
                    DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                    DirectPollStatus::Pending => {}
                    DirectPollStatus::ExternalWorkReady => {
                        panic!("non-reference donor fixture unexpectedly requested external work")
                    }
                    DirectPollStatus::Complete => {
                        return parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap();
                    }
                }
            }
            panic!("donor fixture did not reach a line boundary")
        }

        let (_document, epoch) = document("");
        let interval = RelativeCheckpointMeasure::new(10, 10, 1, 3, 1);
        let mut accumulator = DonorCheckpointSampleAccumulator::from_document_origin();
        accumulator.begin_paragraph_group(BlockId(7)).unwrap();
        accumulator
            .retire_empty_paragraph_group(BlockId(7))
            .unwrap();
        assert!(accumulator.open_paragraph.is_none());
        assert!(accumulator.normalization_spans.is_empty());

        // Once the unresolved group is retired, the next fully joined sample
        // is a normal direct frontier. No synthetic Table outcome or guessed
        // provisional recipe enters the checkpoint index in this first slice.
        accumulator
            .push_sample(
                DonorCheckpointSampleDraft::try_new(interval, donor_after_line("a\n")).unwrap(),
            )
            .unwrap();
        accumulator.chain = DonorCheckpointSampleChainState::DocumentOrigin {
            expected: Some(DonorCheckpointSampleCursorState {
                epoch,
                sample_ordinal: 1,
                cumulative_cut: interval,
            }),
        };
        let builder = accumulator.into_checkpoint_index_builder(interval).unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let manifest = builder.build_in_session(&mut session).unwrap();
        let (index, receipt) = manifest.commit(session).unwrap();
        assert_eq!(receipt.donor_partition_manifests, 1);
        assert_eq!(receipt.donor_sample_headers, 1);
        let sample = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        assert!(matches!(
            sample.committed_role(&index, &arena).unwrap(),
            CommittedDonorCheckpointRole::DirectRun(_)
        ));
        index.release_later(&mut arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);

        // A sampled provisional Paragraph cannot be silently relabelled as a
        // Table. Supporting that case requires the future sealed group
        // manifest; until then the transaction fails closed.
        let mut sampled = DonorCheckpointSampleAccumulator::from_document_origin();
        sampled.begin_paragraph_group(BlockId(8)).unwrap();
        sampled
            .push_sample(
                DonorCheckpointSampleDraft::try_new(interval, donor_after_line("b\n")).unwrap(),
            )
            .unwrap();
        assert_eq!(
            sampled.retire_empty_paragraph_group(BlockId(8)),
            Err(CandidateWriterError::Invariant(
                "fragment replacement crossed a sampled Paragraph group"
            ))
        );
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn writer_owned_setext_group_persists_one_exact_normalization_partition() {
        use crate::committed_checkpoint_index::{
            CommittedDonorCheckpointRole, StorageOnlyNormalizationOutcome,
        };
        use flark_comrak_value_block_core::{DirectDurableGrammarCapture, DirectPollStatus};

        fn donor_after_line(line: &str) -> DirectDurableGrammarCapture {
            let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
            parser.acknowledge_command().unwrap();
            parser.begin_line(line.to_owned()).unwrap();
            for _ in 0..512 {
                match parser.poll_line(1).unwrap().status {
                    DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                    DirectPollStatus::Pending => {}
                    DirectPollStatus::ExternalWorkReady => {
                        panic!("non-reference donor fixture unexpectedly requested external work")
                    }
                    DirectPollStatus::Complete => {
                        return parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap();
                    }
                }
            }
            panic!("donor fixture did not reach a line boundary")
        }

        let (_document, epoch) = document("");
        let interval = RelativeCheckpointMeasure::new(10, 10, 1, 3, 1);
        let mut accumulator = DonorCheckpointSampleAccumulator::from_document_origin();
        accumulator
            .push_sample(
                DonorCheckpointSampleDraft::try_new(interval, donor_after_line("a\n")).unwrap(),
            )
            .unwrap();
        accumulator.begin_paragraph_group(BlockId(7)).unwrap();
        accumulator
            .push_sample(
                DonorCheckpointSampleDraft::try_new(interval, donor_after_line("b\n")).unwrap(),
            )
            .unwrap();
        accumulator
            .promote_paragraph_group(BlockId(7), GreenHeadingOpenFacts::setext(1).unwrap())
            .unwrap();
        accumulator
            .push_sample(
                DonorCheckpointSampleDraft::try_new(interval, donor_after_line("=\n")).unwrap(),
            )
            .unwrap();
        accumulator.finish_paragraph_group(BlockId(7)).unwrap();
        accumulator
            .push_sample(
                DonorCheckpointSampleDraft::try_new(interval, donor_after_line("c\n")).unwrap(),
            )
            .unwrap();
        let final_measure = RelativeCheckpointMeasure::new(40, 40, 4, 12, 4);
        accumulator.chain = DonorCheckpointSampleChainState::DocumentOrigin {
            expected: Some(DonorCheckpointSampleCursorState {
                epoch,
                sample_ordinal: 4,
                cumulative_cut: final_measure,
            }),
        };

        let builder = accumulator
            .into_checkpoint_index_builder(final_measure)
            .unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let manifest = builder.build_in_session(&mut session).unwrap();
        let (index, receipt) = manifest.commit(session).unwrap();
        assert_eq!(receipt.donor_partition_manifests, 3);
        assert_eq!(receipt.donor_sample_headers, 4);

        let first = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        assert!(matches!(
            first.committed_role(&index, &arena).unwrap(),
            CommittedDonorCheckpointRole::DirectRun(_)
        ));
        let normalized = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 30)
            .unwrap()
            .unwrap();
        let CommittedDonorCheckpointRole::Normalization(authority) =
            normalized.committed_role(&index, &arena).unwrap()
        else {
            panic!("Setext-owned samples were not persisted as normalization")
        };
        assert_eq!(authority.group(), 7);
        assert_eq!(
            authority.outcome(),
            StorageOnlyNormalizationOutcome::SetextHeading { level: 1 }
        );
        let last = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 40)
            .unwrap()
            .unwrap();
        assert!(matches!(
            last.committed_role(&index, &arena).unwrap(),
            CommittedDonorCheckpointRole::DirectRun(_)
        ));

        index.release_later(&mut arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    fn drive(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> CandidateWriterProgress {
        for _ in 0..100_000 {
            let progress = document.poll_candidate_writer(epoch).unwrap();
            assert_eq!(
                document
                    .candidate_writer_test_arena()
                    .build_lifecycle(epoch.build_id())
                    .unwrap(),
                crate::ArenaBuildLifecycle::Suspended,
                "every writer phase must yield the arena session"
            );
            match progress {
                CandidateWriterProgress::Pending => {}
                complete => return complete,
            }
        }
        panic!("candidate writer did not complete bounded action");
    }

    fn open(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        kind: GreenKind,
    ) -> CandidateWriterBinding {
        open_with_facts(document, epoch, kind, FactsEnvelope::empty())
    }

    fn open_with_facts(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        kind: GreenKind,
        facts: FactsEnvelope,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open(epoch, kind, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("open returned {progress:?}"),
        }
    }

    fn open_list(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenListOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_list(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("List open returned {progress:?}"),
        }
    }

    fn open_item(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenItemOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_item(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("Item open returned {progress:?}"),
        }
    }

    fn open_heading(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenHeadingOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_heading(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("Heading open returned {progress:?}"),
        }
    }

    fn open_table(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenTableOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_table(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("Table open returned {progress:?}"),
        }
    }

    fn open_table_row(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenTableRowOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_table_row(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("TableRow open returned {progress:?}"),
        }
    }

    fn open_table_cell(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenTableCellOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_table_cell(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("TableCell open returned {progress:?}"),
        }
    }

    fn open_fenced_code(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        facts: GreenFencedCodeOpenFacts,
    ) -> CandidateWriterBinding {
        document
            .candidate_writer_start_open_fenced_code(epoch, facts)
            .unwrap();
        match drive(document, epoch) {
            CandidateWriterProgress::Opened(binding) => binding,
            progress => panic!("FencedCode open returned {progress:?}"),
        }
    }

    fn close(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
    ) {
        document
            .candidate_writer_start_close(epoch, binding, ClosedChildAggregate::default(), false)
            .unwrap();
        assert!(matches!(
            drive(document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
    }

    fn close_with_facts(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        facts: GreenCloseFacts,
    ) {
        document
            .candidate_writer_start_close_with_facts(
                epoch,
                binding,
                ClosedChildAggregate::default(),
                false,
                facts,
            )
            .unwrap();
        assert!(matches!(
            drive(document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
    }

    fn atom(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> CandidateWriterSourceAtom {
        loop {
            match document.poll_candidate_writer_source(epoch, 1).unwrap() {
                CandidateWriterSourcePoll::NeedFuel(_) => {}
                CandidateWriterSourcePoll::Atom { atom, .. } => return atom,
                CandidateWriterSourcePoll::Eof(_) => panic!("expected source atom"),
            }
        }
    }

    fn close_fenced_code(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        facts: GreenFencedCodeCloseFacts,
    ) {
        document
            .candidate_writer_start_close_fenced_code_with_test_facts(
                epoch,
                binding,
                ClosedChildAggregate::default(),
                false,
                facts,
            )
            .unwrap();
        assert!(matches!(
            drive(document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
    }

    fn eof(document: &mut crate::LiveDocumentStore, epoch: LiveCandidateEpoch) {
        loop {
            match document.poll_candidate_writer_source(epoch, 1).unwrap() {
                CandidateWriterSourcePoll::NeedFuel(_) => {}
                CandidateWriterSourcePoll::Eof(_) => return,
                CandidateWriterSourcePoll::Atom { atom, .. } => {
                    panic!("unconsumed source atom: {atom:?}")
                }
            }
        }
    }

    fn recognize_line(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> (String, CandidateRecognitionLineReceipt) {
        let mut text = String::new();
        loop {
            match document
                .poll_candidate_writer_recognition(epoch, 1)
                .unwrap()
            {
                CandidateRecognitionPoll::NeedFuel(_) => {}
                CandidateRecognitionPoll::Atom { atom, .. } => match atom.kind() {
                    CandidateSourceAtomKind::Scalar(value) => text.push(value),
                    CandidateSourceAtomKind::Tab => text.push('\t'),
                    CandidateSourceAtomKind::Nul => text.push('\0'),
                    CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::Lf) => {
                        text.push('\n');
                        break;
                    }
                    CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::LoneCr) => {
                        text.push('\r');
                        break;
                    }
                    CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::CrLf) => {
                        text.push_str("\r\n");
                        break;
                    }
                },
                CandidateRecognitionPoll::Eof(_) => break,
            }
        }
        let receipt = document
            .candidate_writer_finish_recognition_line(epoch)
            .unwrap();
        (text, receipt)
    }

    fn consume(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
        owner: &CandidateWriterBinding,
        part: CoveragePart,
        logical: CandidateWriterLogicalAction<'_>,
    ) {
        document
            .candidate_writer_start_consume(epoch, atom, owner, part, logical)
            .unwrap();
        assert!(matches!(
            drive(document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
    }

    fn finish(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> CandidateWriterBuiltDocument {
        document.candidate_writer_start_finish(epoch).unwrap();
        assert!(matches!(
            drive(document, epoch),
            CandidateWriterProgress::CompletionReady
        ));
        document
            .commit_candidate_writer_local_for_test(epoch)
            .unwrap()
    }

    fn promote_forced_deferred_setext(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        paragraph: CandidateWriterBinding,
    ) -> (CandidateWriterBinding, BlockId, BlockId) {
        let retired = paragraph.binding.block_id();
        document
            .force_candidate_writer_setext_deferred_identity_for_test(epoch)
            .unwrap();
        document
            .candidate_writer_start_promote_setext(
                epoch,
                paragraph,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        let heading = match drive(document, epoch) {
            CandidateWriterProgress::RetypedWithDeferredResidual { binding, facts } => {
                assert_eq!(facts, GreenHeadingOpenFacts::setext(1).unwrap());
                binding
            }
            progress => panic!("forced deferred Setext returned {progress:?}"),
        };
        let replacement = heading.binding.block_id();
        assert_ne!(replacement, retired);
        (heading, retired, replacement)
    }

    fn entered_blocks(trace: &[SerializedGreenTestEvent]) -> Vec<(BlockId, GreenKind)> {
        trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Enter { block, kind } => Some((*block, *kind)),
                SerializedGreenTestEvent::Coverage { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect()
    }

    fn cancel_with_unit_fuel(document: &mut crate::LiveDocumentStore, epoch: LiveCandidateEpoch) {
        let abort = document.cancel_candidate(epoch).unwrap();
        for _ in 0..10_000 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                return;
            }
        }
        panic!("candidate abort did not complete with bounded unit fuel");
    }

    fn supply_table_header(
        document: &mut crate::LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        input: CandidateTableHeaderInput,
    ) -> CandidateWriterProgress {
        document
            .candidate_writer_supply_table_header_input(epoch, input)
            .unwrap();
        drive(document, epoch)
    }

    #[test]
    fn empty_document_joins_source_composer_and_green_without_raw_manifest_escape() {
        let (mut document, epoch) = document("");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        close(&mut document, epoch, root);
        eof(&mut document, epoch);

        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(matches!(
            trace.as_slice(),
            [
                SerializedGreenTestEvent::Enter {
                    kind: GreenKind::DOCUMENT,
                    ..
                },
                SerializedGreenTestEvent::Exit
            ]
        ));
        assert_eq!(
            built
                .green_document()
                .metric(document.candidate_writer_test_arena())
                .unwrap(),
            crate::SerializedMetric::default()
        );
        assert_eq!(built.source(), epoch.source());
        assert_eq!(built.green_runs_acknowledged(), 0);
    }

    #[test]
    fn one_paragraph_waits_for_ready_ack_and_preserves_exact_unicode_metrics() {
        let (mut document, epoch) = document("hello😀");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);

        for _ in 0..6 {
            let source = atom(&mut document, epoch);
            consume(
                &mut document,
                epoch,
                source,
                &paragraph,
                CoveragePart::CONTENT,
                CandidateWriterLogicalAction::Identity { target: &paragraph },
            );
        }
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert_eq!(trace.len(), 5);
        assert!(matches!(
            trace[0],
            SerializedGreenTestEvent::Enter {
                kind: GreenKind::DOCUMENT,
                ..
            }
        ));
        assert!(matches!(
            trace[1],
            SerializedGreenTestEvent::Enter {
                kind: GreenKind::PARAGRAPH,
                ..
            }
        ));
        assert!(matches!(
            trace[2],
            SerializedGreenTestEvent::Coverage {
                metric: crate::SerializedMetric { bytes: 9, utf16: 7 },
                part: CoveragePart::CONTENT,
                logical: SerializedGreenTestLogical::Identity,
                ..
            }
        ));
        assert!(matches!(trace[3], SerializedGreenTestEvent::Exit));
        assert!(matches!(trace[4], SerializedGreenTestEvent::Exit));
        assert_eq!(built.green_runs_acknowledged(), 1);
        assert_eq!(built.composer_receipt().projection_runs_sealed, 1);
    }

    #[test]
    fn recognition_is_read_only_then_authoritative_replay_matches_inside_writer() {
        let (mut document, epoch) = document("hello😀\r\n");
        assert_eq!(
            document
                .candidate_writer_recognition_checkpoint(epoch)
                .unwrap()
                .absolute_offset(),
            0
        );

        let (recognized, recognition) = recognize_line(&mut document, epoch);
        assert_eq!(recognized, "hello😀\r\n");
        assert_eq!(recognition.absolute_range(), (0, 11));

        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        loop {
            let source = atom(&mut document, epoch);
            let is_terminal = matches!(
                source.kind(),
                CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::CrLf)
            );
            if is_terminal {
                consume(
                    &mut document,
                    epoch,
                    source,
                    &paragraph,
                    CoveragePart::TERMINAL,
                    CandidateWriterLogicalAction::None,
                );
                break;
            }
            consume(
                &mut document,
                epoch,
                source,
                &paragraph,
                CoveragePart::CONTENT,
                CandidateWriterLogicalAction::Identity { target: &paragraph },
            );
        }
        let replay = document.candidate_writer_finish_line(epoch).unwrap();
        assert!(replay.recognition_replay_matched());
        assert_eq!(replay.absolute_range(), recognition.absolute_range());
        eof(&mut document, epoch);
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        assert_eq!(built.source_metric().bytes(), 11);
        assert_eq!(built.source_metric().utf16(), 9);
    }

    #[test]
    fn long_identity_line_is_one_source_piece_and_yields_during_replay() {
        let content = format!("{}😀", "a".repeat(10_000));
        let source = format!("{content}\r\n");
        let (mut document, epoch) = document(&source);
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let (recognized, _) = recognize_line(&mut document, epoch);
        assert_eq!(recognized, source);

        document
            .candidate_writer_start_identity_line_replay(epoch, &paragraph, CoveragePart::CONTENT)
            .unwrap();
        let mut pending_polls = 0;
        let compatibility_terminator = loop {
            match document.poll_candidate_writer(epoch).unwrap() {
                CandidateWriterProgress::Pending => pending_polls += 1,
                CandidateWriterProgress::IdentityLineReady { terminator } => break terminator,
                progress => panic!("identity replay returned {progress:?}"),
            }
        };
        assert!(
            compatibility_terminator.is_none(),
            "range replay must not prefetch the CRLF beyond its exact endpoint"
        );
        assert!(
            pending_polls >= 39,
            "10,001 scalars must cross many bounded 256-work polls"
        );
        let terminator = atom(&mut document, epoch);
        document
            .candidate_writer_stage_terminator(epoch, terminator, &paragraph)
            .unwrap();
        document
            .candidate_writer_start_resolve_terminator(
                epoch,
                CandidateTerminatorResolution::CloseNone,
            )
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
        let line = document.candidate_writer_finish_line(epoch).unwrap();
        assert!(line.recognition_replay_matched());
        eof(&mut document, epoch);
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        assert_eq!(
            built.composer_receipt().source_pieces_consumed,
            2,
            "one long identity span plus one deferred CRLF piece"
        );
        assert_eq!(
            built.source_metric().bytes(),
            u64::try_from(source.len()).unwrap()
        );
        assert_eq!(
            built.source_metric().utf16(),
            u64::try_from(source.encode_utf16().count()).unwrap()
        );
    }

    #[test]
    fn deferred_terminator_and_blank_gap_resolve_after_recognition_lookahead() {
        let (mut document, epoch) = document("a\n\nb");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let first = open(&mut document, epoch, GreenKind::PARAGRAPH);

        let (first_text, _) = recognize_line(&mut document, epoch);
        assert_eq!(first_text, "a\n");
        let a = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            a,
            &first,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &first },
        );
        let first_newline = atom(&mut document, epoch);
        document
            .candidate_writer_stage_terminator(epoch, first_newline, &first)
            .unwrap();
        let first_receipt = document.candidate_writer_finish_line(epoch).unwrap();
        assert!(first_receipt.recognition_replay_matched());
        assert_eq!(
            first_receipt.pending(),
            Some(crate::PendingSourceKind::Terminator)
        );

        // Grammar lookahead sees the blank successor before assigning the
        // previous newline or the blank line to a final structural owner.
        let (blank_text, _) = recognize_line(&mut document, epoch);
        assert_eq!(blank_text, "\n");
        document
            .candidate_writer_start_resolve_terminator(
                epoch,
                CandidateTerminatorResolution::CloseNone,
            )
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
        close(&mut document, epoch, first);

        let blank_newline = atom(&mut document, epoch);
        document
            .candidate_writer_defer_blank_gap_atom(epoch, blank_newline)
            .unwrap();
        document.candidate_writer_stage_blank_gap(epoch).unwrap();
        let blank_receipt = document.candidate_writer_finish_line(epoch).unwrap();
        assert!(blank_receipt.recognition_replay_matched());
        assert_eq!(blank_receipt.pending(), Some(crate::PendingSourceKind::Gap));

        let (last_text, _) = recognize_line(&mut document, epoch);
        assert_eq!(last_text, "b");
        document
            .candidate_writer_start_resolve_blank_gap(epoch, &root)
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
        let second = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let b = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            b,
            &second,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &second },
        );
        eof(&mut document, epoch);
        let last_receipt = document.candidate_writer_finish_line(epoch).unwrap();
        assert!(last_receipt.recognition_replay_matched());
        close(&mut document, epoch, second);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        assert_eq!(built.source_metric().bytes(), 4);
        assert_eq!(built.source_metric().utf16(), 4);
    }

    #[test]
    fn marked_blank_line_stages_only_the_source_certified_suffix() {
        let (mut document, epoch) = document("> \n");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let quote = open(&mut document, epoch, GreenKind::BLOCK_QUOTE);
        let (recognized, _) = recognize_line(&mut document, epoch);
        assert_eq!(recognized, "> \n");

        let marker = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            marker,
            &quote,
            CoveragePart::CONTAINER_MARKER,
            CandidateWriterLogicalAction::None,
        );
        for _ in 0..2 {
            let blank_suffix = atom(&mut document, epoch);
            document
                .candidate_writer_defer_blank_gap_atom(epoch, blank_suffix)
                .unwrap();
        }
        document.candidate_writer_stage_blank_gap(epoch).unwrap();
        let line = document.candidate_writer_finish_line(epoch).unwrap();
        assert!(line.recognition_replay_matched());
        assert_eq!(line.pending(), Some(crate::PendingSourceKind::Gap));

        document
            .candidate_writer_start_resolve_blank_gap(epoch, &quote)
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::ActionComplete
        ));
        eof(&mut document, epoch);
        close(&mut document, epoch, quote);
        close(&mut document, epoch, root);
        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let coverage = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Coverage { metric, part, .. } => Some((*metric, *part)),
                SerializedGreenTestEvent::Enter { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            coverage,
            vec![
                (
                    crate::SerializedMetric { bytes: 1, utf16: 1 },
                    CoveragePart::CONTAINER_MARKER
                ),
                (
                    crate::SerializedMetric { bytes: 2, utf16: 2 },
                    CoveragePart::GAP
                )
            ]
        );
    }

    #[test]
    fn two_paragraphs_drain_coverage_before_exit_and_enter() {
        let (mut document, epoch) = document("a\nb");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let first = open(&mut document, epoch, GreenKind::PARAGRAPH);

        let a = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            a,
            &first,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &first },
        );
        let newline = atom(&mut document, epoch);
        assert_eq!(
            newline.kind(),
            CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::Lf)
        );
        consume(
            &mut document,
            epoch,
            newline,
            &first,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, first);

        let second = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let b = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            b,
            &second,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &second },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, second);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let kinds: Vec<_> = trace
            .iter()
            .map(|event| match event {
                SerializedGreenTestEvent::Enter { kind, .. } => Some(("enter", kind.0)),
                SerializedGreenTestEvent::Coverage { .. } => Some(("coverage", 0)),
                SerializedGreenTestEvent::Exit => Some(("exit", 0)),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                Some(("enter", GreenKind::DOCUMENT.0)),
                Some(("enter", GreenKind::PARAGRAPH.0)),
                Some(("coverage", 0)),
                Some(("coverage", 0)),
                Some(("exit", 0)),
                Some(("enter", GreenKind::PARAGRAPH.0)),
                Some(("coverage", 0)),
                Some(("exit", 0)),
                Some(("exit", 0)),
            ]
        );
    }

    #[test]
    fn nested_already_final_ownership_preserves_relative_depth_without_directory() {
        let (mut document, epoch) = document(">-x");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let quote = open(&mut document, epoch, GreenKind::BLOCK_QUOTE);
        let list = open_list(
            &mut document,
            epoch,
            GreenListOpenFacts::bullet(GreenListBullet::Dash),
        );
        let item = open_item(&mut document, epoch, GreenItemOpenFacts::new(0, 2).unwrap());
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);

        let marker = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            marker,
            &quote,
            CoveragePart::CONTAINER_MARKER,
            CandidateWriterLogicalAction::None,
        );
        let item_marker = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            item_marker,
            &item,
            CoveragePart::BLOCK_MARKER,
            CandidateWriterLogicalAction::None,
        );
        let content = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            content,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, item);
        close_with_facts(
            &mut document,
            epoch,
            list,
            GreenCloseFacts::List { tight: true },
        );
        close(&mut document, epoch, quote);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let depths: Vec<_> = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Coverage {
                    owner_relative_depth,
                    part,
                    ..
                } => Some((*owner_relative_depth, *part)),
                SerializedGreenTestEvent::Enter { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect();
        assert_eq!(
            depths,
            vec![
                (3, CoveragePart::CONTAINER_MARKER),
                (1, CoveragePart::BLOCK_MARKER),
                (0, CoveragePart::CONTENT),
            ]
        );
        let close_facts = serialized_green_test_close_facts(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(close_facts.contains(&(GreenKind::LIST, GreenCloseFacts::List { tight: true })));
    }

    #[test]
    fn typed_heading_and_fenced_code_writer_entries_reach_the_packed_trace() {
        let (mut document, epoch) = document("h\nx");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let heading_facts = GreenHeadingOpenFacts::atx(1).unwrap();
        let heading = open_heading(&mut document, epoch, heading_facts);

        let h = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            h,
            &heading,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &heading },
        );
        let newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            newline,
            &heading,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, heading);

        let fenced_open =
            GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 300, 0).unwrap();
        let fenced = open_fenced_code(&mut document, epoch, fenced_open);
        let x = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            x,
            &fenced,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &fenced },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        let fenced_close = GreenFencedCodeCloseFacts::new(
            false,
            GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
            GreenRelativeLogicalSlice::new(0..1, 0..1).unwrap(),
        )
        .unwrap();
        close_fenced_code(&mut document, epoch, fenced, fenced_close);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let arena = document.candidate_writer_test_arena();
        let heading_cursor = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                0,
                GreenAffinity::Downstream,
            )
            .unwrap();
        assert_eq!(
            GreenHeadingOpenFacts::try_from_envelope(
                &heading_cursor.open_path().last().unwrap().facts
            ),
            Ok(heading_facts)
        );
        let fenced_cursor = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                2,
                GreenAffinity::Downstream,
            )
            .unwrap();
        assert_eq!(
            GreenFencedCodeOpenFacts::try_from_envelope(
                &fenced_cursor.open_path().last().unwrap().facts
            ),
            Ok(fenced_open)
        );
        assert!(
            serialized_green_test_close_facts(built.green_document(), arena)
                .unwrap()
                .contains(&(
                    GreenKind::FENCED_CODE,
                    GreenCloseFacts::FencedCode(fenced_close)
                ))
        );
    }

    #[test]
    fn streamed_paragraph_table_normalization_rebinds_composer_green_and_ledger() {
        use flark_oversized_block_line_gate::{
            CancellationToken, StreamingTableRowJob, TableRowStreamPoll,
        };

        fn metric(offset: usize) -> SerializedMetric {
            let offset = u64::try_from(offset).unwrap();
            SerializedMetric {
                bytes: offset,
                utf16: offset,
            }
        }

        fn alignment(value: u8) -> GreenTableAlignment {
            match value {
                0 => GreenTableAlignment::Unspecified,
                1 => GreenTableAlignment::Left,
                2 => GreenTableAlignment::Center,
                3 => GreenTableAlignment::Right,
                _ => panic!("scanner emitted a noncanonical alignment"),
            }
        }

        const HEADER: &[u8] = b"a|b\n";
        const DELIMITER: &[u8] = b"-|-\n";
        const SOURCE: &str = "a|b\n-|-\n1\n";
        assert_eq!(DELIMITER, &SOURCE.as_bytes()[4..8]);

        let cancellation = CancellationToken::default();
        let mut delimiter_count = StreamingTableRowJob::new(DELIMITER);
        let mut observed_columns = 0_u32;
        let delimiter_summary = loop {
            match delimiter_count.poll(DELIMITER, 1, &cancellation) {
                TableRowStreamPoll::Pending { inspected } => assert!(inspected <= 1),
                TableRowStreamPoll::Cell { inspected, .. } => {
                    assert!(inspected <= 1);
                    observed_columns = observed_columns.checked_add(1).unwrap();
                }
                TableRowStreamPoll::Complete { value, inspected } => {
                    assert!(inspected <= 1);
                    break value.expect("ordinary delimiter row is recognized");
                }
                TableRowStreamPoll::Cancelled { .. } => panic!("uncancelled scanner cancelled"),
            }
        };
        assert!(delimiter_summary.delimiter_row);
        assert_eq!(delimiter_summary.cells, observed_columns);
        assert_eq!(delimiter_count.receipt().maximum_bytes_per_poll, 1);

        let (mut document, epoch) = document(SOURCE);
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        for _ in &HEADER[..HEADER.len() - 1] {
            let source = atom(&mut document, epoch);
            consume(
                &mut document,
                epoch,
                source,
                &paragraph,
                CoveragePart::CONTENT,
                CandidateWriterLogicalAction::Identity { target: &paragraph },
            );
        }
        let header_newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            header_newline,
            &paragraph,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        document
            .candidate_writer_start_promote_table_header(
                epoch,
                paragraph,
                GreenTableOpenFacts::new(delimiter_summary.cells).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::TableHeaderInputReady
        ));

        // Second pass pairs one header cell with one certified delimiter
        // alignment. At most one cell from either scanner is retained.
        let mut header_scan = StreamingTableRowJob::new(HEADER);
        let mut delimiter_scan = StreamingTableRowJob::new(DELIMITER);
        let mut header_cell = None;
        let mut delimiter_cell = None;
        let mut header_complete = None;
        let mut delimiter_complete = None;
        while header_complete.is_none() || delimiter_complete.is_none() {
            if header_cell.is_none() && header_complete.is_none() {
                match header_scan.poll(HEADER, 1, &cancellation) {
                    TableRowStreamPoll::Pending { inspected } => assert!(inspected <= 1),
                    TableRowStreamPoll::Cell { value, inspected } => {
                        assert!(inspected <= 1);
                        header_cell = Some(value);
                    }
                    TableRowStreamPoll::Complete { value, inspected } => {
                        assert!(inspected <= 1);
                        header_complete = value;
                    }
                    TableRowStreamPoll::Cancelled { .. } => panic!("header scan cancelled"),
                }
            }
            if delimiter_cell.is_none() && delimiter_complete.is_none() {
                match delimiter_scan.poll(DELIMITER, 1, &cancellation) {
                    TableRowStreamPoll::Pending { inspected } => assert!(inspected <= 1),
                    TableRowStreamPoll::Cell { value, inspected } => {
                        assert!(inspected <= 1);
                        delimiter_cell = Some(value);
                    }
                    TableRowStreamPoll::Complete { value, inspected } => {
                        assert!(inspected <= 1);
                        delimiter_complete = value;
                    }
                    TableRowStreamPoll::Cancelled { .. } => panic!("delimiter scan cancelled"),
                }
            }
            if let (Some(header), Some(delimiter)) = (header_cell.take(), delimiter_cell.take()) {
                let scanner_alignment = delimiter
                    .delimiter_alignment
                    .expect("delimiter pass certifies every paired column");
                assert!(matches!(
                    supply_table_header(
                        &mut document,
                        epoch,
                        CandidateTableHeaderInput::BeginCell {
                            source_start: metric(header.cell.source.start),
                            source_end: metric(header.cell.source.end),
                            alignment: alignment(scanner_alignment),
                        },
                    ),
                    CandidateWriterProgress::TableHeaderInputReady
                ));
                if header.cell.source.start < header.cell.content.start {
                    assert!(matches!(
                        supply_table_header(
                            &mut document,
                            epoch,
                            CandidateTableHeaderInput::Coverage {
                                source_end: metric(header.cell.content.start),
                                part: CoveragePart::CONTENT,
                                logical: CandidateTableHeaderLogical::Hidden {
                                    affinity: GreenAffinity::Downstream,
                                },
                            },
                        ),
                        CandidateWriterProgress::TableHeaderInputReady
                    ));
                }
                if header.cell.content.start < header.cell.content.end {
                    assert!(matches!(
                        supply_table_header(
                            &mut document,
                            epoch,
                            CandidateTableHeaderInput::Coverage {
                                source_end: metric(header.cell.content.end),
                                part: CoveragePart::CONTENT,
                                logical: CandidateTableHeaderLogical::Identity,
                            },
                        ),
                        CandidateWriterProgress::TableHeaderInputReady
                    ));
                }
                if header.cell.content.end < header.cell.source.end {
                    assert!(matches!(
                        supply_table_header(
                            &mut document,
                            epoch,
                            CandidateTableHeaderInput::Coverage {
                                source_end: metric(header.cell.source.end),
                                part: CoveragePart::CONTENT,
                                logical: CandidateTableHeaderLogical::Hidden {
                                    affinity: GreenAffinity::Upstream,
                                },
                            },
                        ),
                        CandidateWriterProgress::TableHeaderInputReady
                    ));
                }
                assert!(matches!(
                    supply_table_header(&mut document, epoch, CandidateTableHeaderInput::EndCell,),
                    CandidateWriterProgress::TableHeaderInputReady
                ));
            }
        }
        let header_summary = header_complete.expect("header scan completed");
        let paired_delimiter = delimiter_complete.expect("delimiter scan completed");
        assert_eq!(header_summary.cells, delimiter_summary.cells);
        assert_eq!(paired_delimiter, delimiter_summary);
        let table = match supply_table_header(
            &mut document,
            epoch,
            CandidateTableHeaderInput::Finish {
                content_end: metric(HEADER.len() - 1),
            },
        ) {
            CandidateWriterProgress::RetypedTable { binding } => binding,
            progress => panic!("table normalization returned {progress:?}"),
        };

        // A fresh streaming pass drives the delimiter's exact physical source
        // coverage after the replacement action releases the writer slot.
        let mut delimiter_coverage_scan = StreamingTableRowJob::new(DELIMITER);
        let mut delimiter_cursor = 0_usize;
        loop {
            match delimiter_coverage_scan.poll(DELIMITER, 1, &cancellation) {
                TableRowStreamPoll::Pending { inspected } => assert!(inspected <= 1),
                TableRowStreamPoll::Cell { value, inspected } => {
                    assert!(inspected <= 1);
                    while delimiter_cursor < value.cell.source.end {
                        let marker = atom(&mut document, epoch);
                        consume(
                            &mut document,
                            epoch,
                            marker,
                            &table,
                            CoveragePart::BLOCK_MARKER,
                            CandidateWriterLogicalAction::None,
                        );
                        delimiter_cursor += 1;
                    }
                }
                TableRowStreamPoll::Complete { value, inspected } => {
                    assert!(inspected <= 1);
                    assert_eq!(value, Some(delimiter_summary));
                    break;
                }
                TableRowStreamPoll::Cancelled { .. } => panic!("delimiter coverage cancelled"),
            }
        }
        while delimiter_cursor < DELIMITER.len() - 1 {
            let marker = atom(&mut document, epoch);
            consume(
                &mut document,
                epoch,
                marker,
                &table,
                CoveragePart::BLOCK_MARKER,
                CandidateWriterLogicalAction::None,
            );
            delimiter_cursor += 1;
        }
        let delimiter_newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            delimiter_newline,
            &table,
            CoveragePart::GAP,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();

        let body = open_table_row(&mut document, epoch, GreenTableRowOpenFacts::body());
        let body_value = open_table_cell(&mut document, epoch, GreenTableCellOpenFacts::body(0));
        let one = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            one,
            &body_value,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity {
                target: &body_value,
            },
        );
        close(&mut document, epoch, body_value);
        let empty = open_table_cell(&mut document, epoch, GreenTableCellOpenFacts::body(1));
        close(&mut document, epoch, empty);
        let body_newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            body_newline,
            &body,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        eof(&mut document, epoch);
        close(&mut document, epoch, body);
        close(&mut document, epoch, table);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let receipt = built.composer_receipt();
        // The provisional Paragraph has one identity content run plus its
        // typed line-ending terminal run.
        assert_eq!(receipt.projection_runs_retired_by_normalization, 2);
        assert_eq!(receipt.projection_runs_installed_by_normalization, 4);
        assert_eq!(
            receipt.canonical_projection_runs().unwrap(),
            built.green_runs_acknowledged()
        );
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let kinds: Vec<_> = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Enter { kind, .. } => Some(*kind),
                SerializedGreenTestEvent::Coverage { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                GreenKind::DOCUMENT,
                GreenKind::TABLE,
                GreenKind::TABLE_ROW,
                GreenKind::TABLE_CELL,
                GreenKind::TABLE_CELL,
                GreenKind::TABLE_ROW,
                GreenKind::TABLE_CELL,
                GreenKind::TABLE_CELL,
            ]
        );
        let covered = trace
            .iter()
            .fold(SerializedMetric::default(), |sum, event| {
                if let SerializedGreenTestEvent::Coverage { metric, .. } = event {
                    SerializedMetric {
                        bytes: sum.bytes + metric.bytes,
                        utf16: sum.utf16 + metric.utf16,
                    }
                } else {
                    sum
                }
            });
        assert_eq!(covered, metric(SOURCE.len()));
    }

    #[test]
    fn streamed_table_column_count_crosses_legacy_u16_cap_without_truncating_writer_handoff() {
        use flark_oversized_block_line_gate::{
            CancellationToken, MAX_TABLE_CELLS, StreamingTableRowJob, TableRowStreamPoll,
        };

        let column_count = MAX_TABLE_CELLS + 2;
        let header = "x|".repeat(column_count);
        let delimiter = "-|".repeat(column_count);
        let source = format!("{header}\n{delimiter}\n");
        let cancellation = CancellationToken::default();
        let mut scanner = StreamingTableRowJob::new(delimiter.as_bytes());
        let mut emitted = 0_u32;
        let summary = loop {
            match scanner.poll(delimiter.as_bytes(), 17, &cancellation) {
                TableRowStreamPoll::Pending { inspected } => assert!(inspected <= 17),
                TableRowStreamPoll::Cell { inspected, .. } => {
                    assert!(inspected <= 17);
                    emitted = emitted.checked_add(1).unwrap();
                }
                TableRowStreamPoll::Complete { value, inspected } => {
                    assert!(inspected <= 17);
                    break value.expect("pathological delimiter row remains recognized");
                }
                TableRowStreamPoll::Cancelled { .. } => panic!("uncancelled scanner cancelled"),
            }
        };
        assert!(summary.delimiter_row);
        assert_eq!(summary.cells, emitted);
        assert_eq!(usize::try_from(summary.cells).unwrap(), column_count);
        assert!(summary.cells > u32::from(u16::MAX));

        // The scanner's u32 fact enters the real writer transaction without a
        // compatibility-collector conversion or a table-sized allocation.
        let (mut document, epoch) = document(&source);
        let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let (recognized, _) = recognize_line(&mut document, epoch);
        assert_eq!(recognized.len(), header.len() + 1);
        document
            .candidate_writer_start_range_replay(
                epoch,
                &paragraph,
                CoveragePart::CONTENT,
                u64::try_from(header.len()).unwrap(),
                CandidateWriterRangeRecipe::Identity,
            )
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::RangeReplayReady(_)
        ));
        let newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            newline,
            &paragraph,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        document
            .candidate_writer_start_promote_table_header(
                epoch,
                paragraph,
                GreenTableOpenFacts::new(summary.cells).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            drive(&mut document, epoch),
            CandidateWriterProgress::TableHeaderInputReady
        ));

        let abort = document.cancel_candidate(epoch).unwrap();
        for _ in 0..100_000 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                assert_eq!(
                    document
                        .candidate_writer_test_arena()
                        .build_lifecycle(epoch.build_id()),
                    Err(crate::ArenaBuildError::StaleBuild(epoch.build_id()))
                );
                return;
            }
        }
        panic!("pathological table handoff did not cancel under unit fuel");
    }

    #[test]
    fn table_normalization_and_scanner_cancel_at_every_cooperative_boundary() {
        use flark_oversized_block_line_gate::{
            CancellationToken, StreamingTableRowJob, TableRowStreamPoll,
        };

        const INPUTS: [CandidateTableHeaderInput; 7] = [
            CandidateTableHeaderInput::BeginCell {
                source_start: SerializedMetric { bytes: 0, utf16: 0 },
                source_end: SerializedMetric { bytes: 1, utf16: 1 },
                alignment: GreenTableAlignment::Left,
            },
            CandidateTableHeaderInput::Coverage {
                source_end: SerializedMetric { bytes: 1, utf16: 1 },
                part: CoveragePart::CONTENT,
                logical: CandidateTableHeaderLogical::Identity,
            },
            CandidateTableHeaderInput::EndCell,
            CandidateTableHeaderInput::BeginCell {
                source_start: SerializedMetric { bytes: 2, utf16: 2 },
                source_end: SerializedMetric { bytes: 3, utf16: 3 },
                alignment: GreenTableAlignment::Right,
            },
            CandidateTableHeaderInput::Coverage {
                source_end: SerializedMetric { bytes: 3, utf16: 3 },
                part: CoveragePart::CONTENT,
                logical: CandidateTableHeaderLogical::Identity,
            },
            CandidateTableHeaderInput::EndCell,
            CandidateTableHeaderInput::Finish {
                content_end: SerializedMetric { bytes: 3, utf16: 3 },
            },
        ];

        fn prepared() -> (crate::LiveDocumentStore, LiveCandidateEpoch) {
            let (mut document, epoch) = document("a|b\n");
            let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
            let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
            for _ in 0..3 {
                let source = atom(&mut document, epoch);
                consume(
                    &mut document,
                    epoch,
                    source,
                    &paragraph,
                    CoveragePart::CONTENT,
                    CandidateWriterLogicalAction::Identity { target: &paragraph },
                );
            }
            let newline = atom(&mut document, epoch);
            consume(
                &mut document,
                epoch,
                newline,
                &paragraph,
                CoveragePart::TERMINAL,
                CandidateWriterLogicalAction::None,
            );
            document.candidate_writer_finish_line(epoch).unwrap();
            document
                .candidate_writer_start_promote_table_header(
                    epoch,
                    paragraph,
                    GreenTableOpenFacts::new(2).unwrap(),
                )
                .unwrap();
            (document, epoch)
        }

        fn cancel_to_stale(document: &mut crate::LiveDocumentStore, epoch: LiveCandidateEpoch) {
            let abort = document.cancel_candidate(epoch).unwrap();
            for _ in 0..10_000 {
                if document.poll_candidate_abort(abort, 1).unwrap().complete {
                    assert_eq!(
                        document
                            .candidate_writer_test_arena()
                            .build_lifecycle(epoch.build_id()),
                        Err(crate::ArenaBuildError::StaleBuild(epoch.build_id()))
                    );
                    return;
                }
            }
            panic!("table transaction did not abort under unit fuel");
        }

        // Cancellation is observed before another byte or cell is inspected.
        let token = CancellationToken::default();
        let mut scanner = StreamingTableRowJob::new(b"a|b\n");
        assert!(matches!(
            scanner.poll(b"a|b\n", 1, &token),
            TableRowStreamPoll::Pending { inspected: 1 }
        ));
        let before_cancel = scanner.receipt();
        token.cancel();
        assert!(matches!(
            scanner.poll(b"a|b\n", 1, &token),
            TableRowStreamPoll::Cancelled { inspected: 0 }
        ));
        assert_eq!(
            scanner.receipt().bytes_inspected,
            before_cancel.bytes_inspected
        );
        assert_eq!(scanner.receipt().maximum_bytes_per_poll, 1);

        // First establish the deterministic number of cooperative boundaries.
        // A boundary is either one writer poll or one typed scanner-input
        // installation; neither operation is hidden inside the other.
        let (mut completed, completed_epoch) = prepared();
        let mut total_boundaries = 0_usize;
        let mut input_index = 0_usize;
        loop {
            let progress = completed.poll_candidate_writer(completed_epoch).unwrap();
            total_boundaries += 1;
            match progress {
                CandidateWriterProgress::Pending => {}
                CandidateWriterProgress::TableHeaderInputReady => {
                    completed
                        .candidate_writer_supply_table_header_input(
                            completed_epoch,
                            INPUTS[input_index],
                        )
                        .unwrap();
                    input_index += 1;
                    total_boundaries += 1;
                }
                CandidateWriterProgress::RetypedTable { .. } => break,
                progress => panic!("table transaction returned {progress:?}"),
            }
        }
        assert_eq!(input_index, INPUTS.len());
        assert!(total_boundaries > INPUTS.len());
        cancel_to_stale(&mut completed, completed_epoch);

        // Rebuild and cancel after every proper prefix of that boundary trace.
        // This covers pre-composer, active composer replacement, green event
        // offer/ack, storage commit, composer rebase, and ledger rebind states.
        for cancel_after in 0..total_boundaries {
            let (mut candidate, epoch) = prepared();
            let mut crossed = 0_usize;
            let mut input_index = 0_usize;
            let mut awaiting_input = false;
            while crossed < cancel_after {
                if awaiting_input {
                    candidate
                        .candidate_writer_supply_table_header_input(epoch, INPUTS[input_index])
                        .unwrap();
                    input_index += 1;
                    awaiting_input = false;
                    crossed += 1;
                    continue;
                }
                match candidate.poll_candidate_writer(epoch).unwrap() {
                    CandidateWriterProgress::Pending => {}
                    CandidateWriterProgress::TableHeaderInputReady => awaiting_input = true,
                    CandidateWriterProgress::RetypedTable { .. } => {
                        panic!("table transaction completed before boundary {cancel_after}")
                    }
                    progress => panic!("table transaction returned {progress:?}"),
                }
                crossed += 1;
            }
            cancel_to_stale(&mut candidate, epoch);
        }
    }

    #[test]
    fn typed_table_writer_builds_exact_nested_green_with_a_zero_source_cell() {
        let (mut document, epoch) = document("a|b\n-|-\n1\n");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let table = open_table(&mut document, epoch, GreenTableOpenFacts::new(2).unwrap());

        let header = open_table_row(&mut document, epoch, GreenTableRowOpenFacts::header());
        let header_a = open_table_cell(
            &mut document,
            epoch,
            GreenTableCellOpenFacts::header(0, crate::GreenTableAlignment::Left),
        );
        let a = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            a,
            &header_a,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &header_a },
        );
        close(&mut document, epoch, header_a);
        let pipe = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            pipe,
            &header,
            CoveragePart::BLOCK_MARKER,
            CandidateWriterLogicalAction::None,
        );
        let header_b = open_table_cell(
            &mut document,
            epoch,
            GreenTableCellOpenFacts::header(1, crate::GreenTableAlignment::Right),
        );
        let b = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            b,
            &header_b,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &header_b },
        );
        close(&mut document, epoch, header_b);
        let newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            newline,
            &header,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, header);

        for _ in 0..3 {
            let marker = atom(&mut document, epoch);
            consume(
                &mut document,
                epoch,
                marker,
                &table,
                CoveragePart::BLOCK_MARKER,
                CandidateWriterLogicalAction::None,
            );
        }
        let newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            newline,
            &table,
            CoveragePart::GAP,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();

        let body = open_table_row(&mut document, epoch, GreenTableRowOpenFacts::body());
        let body_value = open_table_cell(&mut document, epoch, GreenTableCellOpenFacts::body(0));
        let one = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            one,
            &body_value,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity {
                target: &body_value,
            },
        );
        close(&mut document, epoch, body_value);
        let synthesized_empty =
            open_table_cell(&mut document, epoch, GreenTableCellOpenFacts::body(1));
        close(&mut document, epoch, synthesized_empty);
        let newline = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            newline,
            &body,
            CoveragePart::TERMINAL,
            CandidateWriterLogicalAction::None,
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        eof(&mut document, epoch);
        close(&mut document, epoch, body);
        close(&mut document, epoch, table);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        assert_eq!(built.source_metric().bytes(), 10);
        assert_eq!(built.source_metric().utf16(), 10);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let kinds: Vec<_> = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Enter { kind, .. } => Some(*kind),
                SerializedGreenTestEvent::Coverage { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                GreenKind::DOCUMENT,
                GreenKind::TABLE,
                GreenKind::TABLE_ROW,
                GreenKind::TABLE_CELL,
                GreenKind::TABLE_CELL,
                GreenKind::TABLE_ROW,
                GreenKind::TABLE_CELL,
                GreenKind::TABLE_CELL,
            ]
        );
        let covered = trace
            .iter()
            .fold(SerializedMetric::default(), |sum, event| {
                if let SerializedGreenTestEvent::Coverage { metric, .. } = event {
                    SerializedMetric {
                        bytes: sum.bytes + metric.bytes,
                        utf16: sum.utf16 + metric.utf16,
                    }
                } else {
                    sum
                }
            });
        assert_eq!(
            covered,
            SerializedMetric {
                bytes: 10,
                utf16: 10
            }
        );
    }

    #[test]
    fn deferred_setext_whole_normalization_at_eof_restores_retired_identity() {
        let (mut document, epoch) = document("x");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let root_id = root.binding.block_id();
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let x = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            x,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();

        let (heading, retired, replacement) =
            promote_forced_deferred_setext(&mut document, epoch, paragraph);
        close(&mut document, epoch, heading);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert_eq!(
            entered_blocks(&trace),
            vec![
                (root_id, GreenKind::DOCUMENT),
                (retired, GreenKind::HEADING),
            ]
        );
        assert!(
            !entered_blocks(&trace)
                .iter()
                .any(|(block, _)| *block == replacement)
        );
    }

    #[test]
    fn deferred_setext_whole_normalization_precedes_non_paragraph_open() {
        let (mut document, epoch) = document("x");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let root_id = root.binding.block_id();
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let x = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            x,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();

        let (heading, retired, replacement) =
            promote_forced_deferred_setext(&mut document, epoch, paragraph);
        close(&mut document, epoch, heading);
        let atx = open_heading(&mut document, epoch, GreenHeadingOpenFacts::atx(1).unwrap());
        let atx_id = atx.binding.block_id();
        close(&mut document, epoch, atx);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert_eq!(
            entered_blocks(&trace),
            vec![
                (root_id, GreenKind::DOCUMENT),
                (retired, GreenKind::HEADING),
                (atx_id, GreenKind::HEADING),
            ]
        );
        assert!(
            !entered_blocks(&trace)
                .iter()
                .any(|(block, _)| *block == replacement)
        );
    }

    #[test]
    fn deferred_setext_whole_normalization_rejects_stale_poll_and_cancels_atomically() {
        let (mut document, epoch) = document("x");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let x = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            x,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        let (heading, _, _) = promote_forced_deferred_setext(&mut document, epoch, paragraph);
        close(&mut document, epoch, heading);
        document
            .candidate_writer_start_close(epoch, root, ClosedChildAggregate::default(), false)
            .unwrap();

        let (_foreign, foreign_epoch) = self::document("");
        assert!(matches!(
            document.poll_candidate_writer(foreign_epoch),
            Err(CandidateWriterError::Actor(
                crate::LiveDocumentError::WrongCandidateEpoch
            ))
        ));
        assert!(
            matches!(
                document.poll_candidate_writer(epoch).unwrap(),
                CandidateWriterProgress::Pending
            ),
            "the stale poll must leave the whole-normalization action intact"
        );

        cancel_with_unit_fuel(&mut document, epoch);
        let token = document.active_parse_plan().unwrap().token;
        let fresh = document.begin_candidate(token).unwrap();
        cancel_with_unit_fuel(&mut document, fresh);
    }

    #[test]
    fn deferred_setext_whole_normalization_rejects_crossed_storage_authority() {
        let (mut document, epoch) = document("x");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let x = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            x,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        let (heading, _, _) = promote_forced_deferred_setext(&mut document, epoch, paragraph);
        close(&mut document, epoch, heading);
        document
            .cross_candidate_writer_deferred_setext_storage_for_test(epoch)
            .unwrap();

        assert_eq!(
            document.candidate_writer_start_close(
                epoch,
                root,
                ClosedChildAggregate::default(),
                false,
            ),
            Err(CandidateWriterError::Invariant(
                "deferred normalization storage and ledger identities crossed"
            ))
        );
        assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
        cancel_with_unit_fuel(&mut document, epoch);
    }

    #[test]
    fn direct_setext_heading_open_is_rejected_in_favor_of_paragraph_normalization() {
        let (mut document, epoch) = document("");
        let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
        assert_eq!(
            document.candidate_writer_start_open_heading(
                epoch,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            ),
            Err(CandidateWriterError::Invariant(
                "Setext Heading must come from Paragraph normalization"
            ))
        );
        assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
    }

    #[test]
    fn fenced_code_fold_rejects_injected_facts_reversed_marks_and_missing_marks() {
        let open_facts =
            GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 3, 0).unwrap();
        let empty_close = GreenFencedCodeCloseFacts::new(
            false,
            GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
            GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
        )
        .unwrap();

        let (mut injected, injected_epoch) = document("");
        let _root = open(&mut injected, injected_epoch, GreenKind::DOCUMENT);
        let fence = open_fenced_code(&mut injected, injected_epoch, open_facts);
        assert_eq!(
            injected
                .candidate_writer_start_close_with_facts(
                    injected_epoch,
                    fence,
                    ClosedChildAggregate::default(),
                    false,
                    GreenCloseFacts::FencedCode(empty_close),
                )
                .unwrap_err(),
            CandidateWriterError::Invariant(
                "fenced-code close facts must be derived from the active logical fold"
            )
        );
        assert!(
            injected
                .candidate_writer_is_poisoned(injected_epoch)
                .unwrap()
        );

        let (mut reversed, reversed_epoch) = document("");
        let _root = open(&mut reversed, reversed_epoch, GreenKind::DOCUMENT);
        let fence = open_fenced_code(&mut reversed, reversed_epoch, open_facts);
        assert_eq!(
            reversed
                .candidate_writer_mark_fenced_code_boundary(
                    reversed_epoch,
                    &fence,
                    CandidateFencedCodeBoundary::LiteralStart,
                )
                .unwrap_err(),
            CandidateWriterError::Invariant("fenced-code boundaries are duplicated or reversed")
        );
        assert!(
            reversed
                .candidate_writer_is_poisoned(reversed_epoch)
                .unwrap()
        );

        let (mut missing, missing_epoch) = document("");
        let _root = open(&mut missing, missing_epoch, GreenKind::DOCUMENT);
        let fence = open_fenced_code(&mut missing, missing_epoch, open_facts);
        assert_eq!(
            missing
                .candidate_writer_start_close_fenced_code(
                    missing_epoch,
                    fence,
                    ClosedChildAggregate::default(),
                    false,
                    false,
                )
                .unwrap_err(),
            CandidateWriterError::Invariant("fenced-code close is missing InfoEnd")
        );
        assert!(missing.candidate_writer_is_poisoned(missing_epoch).unwrap());
    }

    #[test]
    fn candidate_finish_rejects_an_unclosed_fenced_code_fold() {
        let (mut document, epoch) = document("");
        let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let _fence = open_fenced_code(
            &mut document,
            epoch,
            GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Tilde, 3, 0).unwrap(),
        );
        assert_eq!(
            document.candidate_writer_start_finish(epoch).unwrap_err(),
            CandidateWriterError::Invariant(
                "candidate finish has an unclosed fenced-code projection fold"
            )
        );
        assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
    }

    #[test]
    fn writer_rejects_raw_list_opens_and_mismatched_close_facts() {
        let (mut raw_document, raw_epoch) = document("");
        let _root = open(&mut raw_document, raw_epoch, GreenKind::DOCUMENT);
        let error = raw_document
            .candidate_writer_start_open(
                raw_epoch,
                GreenKind::LIST,
                GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope(),
            )
            .unwrap_err();
        assert_eq!(
            error,
            CandidateWriterError::Green(SerializedGreenError::Invalid(
                "fact-bearing blocks require their typed writer open API"
            ))
        );
        assert!(
            raw_document
                .candidate_writer_is_poisoned(raw_epoch)
                .unwrap()
        );
        let abort = raw_document.cancel_candidate(raw_epoch).unwrap();
        while !raw_document
            .poll_candidate_abort(abort, 100)
            .unwrap()
            .complete
        {}

        let (mut close_document, close_epoch) = document("");
        let root = open(&mut close_document, close_epoch, GreenKind::DOCUMENT);
        let error = close_document
            .candidate_writer_start_close_with_facts(
                close_epoch,
                root,
                ClosedChildAggregate::default(),
                false,
                GreenCloseFacts::List { tight: false },
            )
            .unwrap_err();
        assert_eq!(
            error,
            CandidateWriterError::Green(SerializedGreenError::Invalid(
                "List close-time facts require a List binding"
            ))
        );
        assert!(
            close_document
                .candidate_writer_is_poisoned(close_epoch)
                .unwrap()
        );
        let abort = close_document.cancel_candidate(close_epoch).unwrap();
        while !close_document
            .poll_candidate_abort(abort, 100)
            .unwrap()
            .complete
        {}
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One readable end-to-end atom-class matrix.
    fn typed_transform_stream_covers_hidden_tab_nul_all_newlines_and_non_bmp() {
        let (mut document, epoch) = document("a\t\0😀\r\nb\rc\n");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);

        let a = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            a,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Hidden {
                target: &paragraph,
                affinity: GreenAffinity::Downstream,
            },
        );
        let tab = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            tab,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::TabToSpaces {
                target: &paragraph,
                spaces: 4,
            },
        );
        let nul = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            nul,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::NulToReplacement { target: &paragraph },
        );
        let emoji = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            emoji,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        let crlf = atom(&mut document, epoch);
        assert_eq!(
            crlf.kind(),
            CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::CrLf)
        );
        consume(
            &mut document,
            epoch,
            crlf,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::CanonicalLineEnding { target: &paragraph },
        );
        document.candidate_writer_finish_line(epoch).unwrap();

        let b = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            b,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        let lone_cr = atom(&mut document, epoch);
        assert_eq!(
            lone_cr.kind(),
            CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::LoneCr)
        );
        consume(
            &mut document,
            epoch,
            lone_cr,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::CanonicalLineEnding { target: &paragraph },
        );
        document.candidate_writer_finish_line(epoch).unwrap();

        let c = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            c,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        let lf = atom(&mut document, epoch);
        assert_eq!(
            lf.kind(),
            CandidateSourceAtomKind::LineEnding(crate::CandidateLineEnding::Lf)
        );
        consume(
            &mut document,
            epoch,
            lf,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::CanonicalLineEnding { target: &paragraph },
        );
        document.candidate_writer_finish_line(epoch).unwrap();
        eof(&mut document, epoch);
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);

        let built = finish(&mut document, epoch);
        assert_eq!(built.source_metric().bytes(), 13);
        assert_eq!(built.source_metric().utf16(), 11);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let coverage: Vec<_> = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Coverage {
                    metric, logical, ..
                } => Some((*metric, logical)),
                SerializedGreenTestEvent::Enter { .. } | SerializedGreenTestEvent::Exit => None,
            })
            .collect();
        assert_eq!(
            coverage
                .iter()
                .fold(crate::SerializedMetric::default(), |sum, (metric, _)| {
                    crate::SerializedMetric {
                        bytes: sum.bytes + metric.bytes,
                        utf16: sum.utf16 + metric.utf16,
                    }
                }),
            crate::SerializedMetric {
                bytes: 13,
                utf16: 11
            }
        );
        let program_pieces: u64 = coverage
            .iter()
            .filter_map(|(_, logical)| match logical {
                SerializedGreenTestLogical::Program { piece_count } => {
                    Some(u64::from(*piece_count))
                }
                SerializedGreenTestLogical::None
                | SerializedGreenTestLogical::Identity
                | SerializedGreenTestLogical::Hidden(_)
                | SerializedGreenTestLogical::Atomic(_) => None,
            })
            .sum();
        assert_eq!(
            built.composer_receipt().source_pieces_consumed,
            9,
            "all certified source atoms must reach the composer"
        );
        assert_eq!(
            program_pieces, 8,
            "typed pieces must survive Program packing: {coverage:?}"
        );
    }

    #[test]
    fn dense_transform_stream_packs_thousands_of_single_use_atoms_into_bounded_runs() {
        let text = "\t\0".repeat(2_500);
        let (mut document, epoch) = document(&text);
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);

        for index in 0..5_000 {
            let source = atom(&mut document, epoch);
            let logical = if index % 2 == 0 {
                CandidateWriterLogicalAction::TabToSpaces {
                    target: &paragraph,
                    spaces: 4,
                }
            } else {
                CandidateWriterLogicalAction::NulToReplacement { target: &paragraph }
            };
            consume(
                &mut document,
                epoch,
                source,
                &paragraph,
                CoveragePart::CONTENT,
                logical,
            );
        }
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);
        let built = finish(&mut document, epoch);

        let receipt = built.composer_receipt();
        assert_eq!(receipt.source_pieces_consumed, 5_000);
        assert!(receipt.projection_runs_sealed > 1);
        assert!(receipt.projection_runs_sealed < 100);
        assert_eq!(
            receipt.projection_runs_sealed,
            built.green_runs_acknowledged()
        );
        assert!(built.green_receipt().projection_program_pages_allocated > 1);
        assert!(receipt.maximum_buffered_projection_bytes <= crate::PROJECTION_PROGRAM_PAGE_BYTES);
    }

    #[test]
    fn dense_nul_range_replay_keeps_constant_job_state_and_bounded_program_pages() {
        const NULS: usize = 8 * 1024;
        let text = "\0".repeat(NULS);
        let (mut document, epoch) = document(&text);
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let (recognized, _) = recognize_line(&mut document, epoch);
        assert_eq!(recognized, text);

        document
            .candidate_writer_start_range_replay(
                epoch,
                &paragraph,
                CoveragePart::CONTENT,
                u64::try_from(NULS).unwrap(),
                CandidateWriterRangeRecipe::CanonicalText,
            )
            .unwrap();
        let range = match drive(&mut document, epoch) {
            CandidateWriterProgress::RangeReplayReady(receipt) => receipt,
            progress => panic!("dense NUL range returned {progress:?}"),
        };
        assert_eq!(range.physical_bytes(), u64::try_from(NULS).unwrap());
        assert_eq!(range.source_work_units(), u64::try_from(NULS).unwrap());
        assert_eq!(range.source_bytes_read(), u64::try_from(NULS).unwrap());
        assert_eq!(range.atoms_scanned(), u64::try_from(NULS).unwrap());
        assert_eq!(range.source_pieces(), u64::try_from(NULS).unwrap());
        assert_eq!(range.maximum_pending_atoms(), 0);
        assert_eq!(range.maximum_pending_boundaries(), 0);

        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);
        let built = finish(&mut document, epoch);
        let composer = built.composer_receipt();
        assert_eq!(
            composer.source_pieces_consumed,
            u64::try_from(NULS).unwrap()
        );
        assert!(composer.projection_runs_sealed > 1);
        assert!(composer.projection_runs_sealed < 100);
        assert_eq!(
            composer.projection_runs_sealed,
            built.green_runs_acknowledged()
        );
        assert!(built.green_receipt().projection_program_pages_allocated > 1);
        assert!(composer.maximum_buffered_projection_bytes <= crate::PROJECTION_PROGRAM_PAGE_BYTES);
        assert!(composer.maximum_pending_source_pieces <= 1);
        assert!(composer.maximum_pending_runs <= 1);
    }

    #[test]
    fn source_atom_is_writer_validated_and_an_outstanding_atom_blocks_repoll() {
        let (mut document, epoch) = document("x");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let source = atom(&mut document, epoch);
        assert!(matches!(
            document.poll_candidate_writer_source(epoch, 1),
            Err(CandidateWriterError::SourceAtomOutstanding)
        ));
        consume(
            &mut document,
            epoch,
            source,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        close(&mut document, epoch, paragraph);
        close(&mut document, epoch, root);
        let _ = finish(&mut document, epoch);
    }

    #[test]
    fn same_scalar_transition_nonce_from_another_candidate_is_rejected_by_source_certificate() {
        let (mut left, left_epoch) = document("x");
        let _left_root = open(&mut left, left_epoch, GreenKind::DOCUMENT);
        let _left_paragraph = open(&mut left, left_epoch, GreenKind::PARAGRAPH);
        let foreign = atom(&mut left, left_epoch);

        let (mut right, right_epoch) = document("y");
        let _right_root = open(&mut right, right_epoch, GreenKind::DOCUMENT);
        let right_paragraph = open(&mut right, right_epoch, GreenKind::PARAGRAPH);
        let right_atom_with_same_nonce = atom(&mut right, right_epoch);
        drop(right_atom_with_same_nonce);

        let error = right
            .candidate_writer_start_consume(
                right_epoch,
                foreign,
                &right_paragraph,
                CoveragePart::CONTENT,
                CandidateWriterLogicalAction::Identity {
                    target: &right_paragraph,
                },
            )
            .unwrap_err();
        assert_eq!(
            error,
            CandidateWriterError::SourceLedger(SourceBoundLedgerError::WrongBoundary)
        );
        assert!(right.candidate_writer_is_poisoned(right_epoch).unwrap());

        // Both candidates remain explicitly cancellable even though A's token
        // was moved into B and B consumed its own scalar transition nonce.
        let candidates =
            std::iter::once((left, left_epoch)).chain(std::iter::once((right, right_epoch)));
        for (mut document, epoch) in candidates {
            let abort = document.cancel_candidate(epoch).unwrap();
            for _ in 0..100 {
                if document.poll_candidate_abort(abort, 1).unwrap().complete {
                    break;
                }
            }
        }
    }

    #[test]
    fn injected_failure_after_exit_ack_is_poison_only_and_cancellable() {
        let (mut document, epoch) = document("");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        document
            .candidate_writer_start_close(epoch, root, ClosedChildAggregate::default(), false)
            .unwrap();
        document.inject_candidate_writer_close_failure_for_test(epoch);

        let error = loop {
            match document.poll_candidate_writer(epoch) {
                Ok(CandidateWriterProgress::Pending) => {}
                Ok(progress) => panic!("close unexpectedly completed: {progress:?}"),
                Err(error) => break error,
            }
        };
        assert_eq!(
            error,
            CandidateWriterError::InjectedAfterGreenAcknowledgement
        );
        assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
        assert_eq!(
            document.candidate_writer_start_finish(epoch),
            Err(CandidateWriterError::WriterPoisoned)
        );

        let abort = document.cancel_candidate(epoch).unwrap();
        let mut complete = false;
        for _ in 0..100 {
            let receipt = document.poll_candidate_abort(abort, 1).unwrap();
            if receipt.complete {
                complete = true;
                break;
            }
        }
        assert!(complete);
    }

    #[test]
    fn injected_failure_after_setext_storage_ack_never_retypes_or_commits() {
        let (mut document, epoch) = document("x");
        let _root = open(&mut document, epoch, GreenKind::DOCUMENT);
        let paragraph = open(&mut document, epoch, GreenKind::PARAGRAPH);
        let x = atom(&mut document, epoch);
        consume(
            &mut document,
            epoch,
            x,
            &paragraph,
            CoveragePart::CONTENT,
            CandidateWriterLogicalAction::Identity { target: &paragraph },
        );
        eof(&mut document, epoch);
        document.candidate_writer_finish_line(epoch).unwrap();
        document
            .candidate_writer_start_promote_setext(
                epoch,
                paragraph,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        document.inject_candidate_writer_setext_failure_for_test(epoch);

        let error = loop {
            match document.poll_candidate_writer(epoch) {
                Ok(CandidateWriterProgress::Pending) => {}
                Ok(progress) => panic!("Setext unexpectedly completed: {progress:?}"),
                Err(error) => break error,
            }
        };
        assert_eq!(
            error,
            CandidateWriterError::InjectedAfterGreenAcknowledgement
        );
        assert!(document.candidate_writer_is_poisoned(epoch).unwrap());
        assert_eq!(
            document.candidate_writer_start_finish(epoch),
            Err(CandidateWriterError::WriterPoisoned)
        );

        let abort = document.cancel_candidate(epoch).unwrap();
        let mut complete = false;
        for _ in 0..100 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                complete = true;
                break;
            }
        }
        assert!(complete);
    }

    #[test]
    fn premature_local_commit_returns_abort_authority_instead_of_losing_ticket() {
        let (mut document, epoch) = document("");
        let failure = document
            .commit_candidate_writer_local_for_test(epoch)
            .unwrap_err();
        let CandidateWriterLocalCommitFailure {
            error,
            abort,
            identities,
        } = failure;
        assert_eq!(error, CandidateWriterError::NoAction);
        let ticket = match abort {
            CandidateWriterAbortLease::Suspended(ticket) => ticket,
            CandidateWriterAbortLease::AlreadyAborting(build) => {
                panic!("precommit failure unexpectedly began abort for {build:?}")
            }
        };
        let build = document
            .candidate_writer_test_arena_mut()
            .begin_build_abort(ticket)
            .unwrap();
        assert!(
            document
                .candidate_writer_test_arena_mut()
                .poll_build_abort(build, 0)
                .unwrap()
                .complete
        );
        // The failed local commit returns the document identity authority as
        // well as the suspended build ticket; neither may be silently lost.
        std::hint::black_box(identities);
    }

    #[test]
    fn actor_mechanism_commit_failure_registers_abort_and_restores_identities() {
        let (mut document, epoch) = document("");
        let root = open(&mut document, epoch, GreenKind::DOCUMENT);
        assert_eq!(root.binding.block_id(), BlockId(1));

        let failure = document
            .commit_candidate_writer_mechanism(epoch)
            .unwrap_err();
        assert_eq!(failure.error, CandidateWriterError::NoAction);
        let abort = failure
            .abort
            .expect("actor-derived suspended ticket enters fuelled abort");
        assert_eq!(document.candidate_epoch(), None);

        for _ in 0..100 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                let token = document.active_parse_plan().unwrap().token;
                let next = document.begin_candidate(token).unwrap();
                let next_block = document.mint_block_permit(next).unwrap();
                assert_eq!(
                    next_block.id(),
                    BlockId(2),
                    "the failed writer's identity allocator remains document-owned"
                );
                let next_abort = document.cancel_candidate(next).unwrap();
                assert!(
                    document
                        .poll_candidate_abort(next_abort, 0)
                        .unwrap()
                        .complete
                );
                return;
            }
        }
        panic!("failed mechanism commit did not complete fuelled abort");
    }

    #[test]
    fn atomic_kinds_are_present_in_test_decoder_for_regression_clarity() {
        // Keeps the trace assertion vocabulary anchored to all atomic variants
        // even when the dense stream packs them into Program pages.
        let kinds = [
            AtomicProjectionKind::TabToSpaces { spaces: 4 },
            AtomicProjectionKind::CrLfToLf,
            AtomicProjectionKind::LoneCrToLf,
            AtomicProjectionKind::NulToReplacement,
        ];
        assert_eq!(kinds.len(), 4);
    }
}

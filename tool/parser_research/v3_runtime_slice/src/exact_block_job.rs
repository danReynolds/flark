//! First real grammar-to-candidate composition slice.
//!
//! The parser owns grammar decisions; the writer owns source, projection, and
//! packed-green authority. This job is the only seam between them. The current
//! executable slice supports the donor core's direct `Document`, `BlockQuote`,
//! `List`, `Item`, `Paragraph`, `FencedCode`, and no-reference Setext protocol.
//! Broader reference-prefix and table normalization remain explicit
//! fail-closed boundaries.

use flark_comrak_value_block_core::{
    DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES, DirectBlockKind, DirectClosedChild,
    DirectCommand, DirectCoveragePart, DirectExternalWorkKind, DirectFenceCharacter,
    DirectFencedCodeBoundary, DirectFencedCodeFacts, DirectFinalFacts, DirectItemFacts,
    DirectLineEnding, DirectListFacts, DirectLogicalAction, DirectOwner, DirectParagraphOutcome,
    DirectPollStatus, DirectReferencePrefixCommitStatus, DirectReferencePrefixRequest,
    DirectSourceLinePollError, DirectSourceLinePollReceipt,
    DirectSourceLinePollStatus, DirectSourceLineSource,
    DirectSourceLineWork, DirectTerminatorResolution, DirectUnsupported, DirectValueBlockParser,
    ListDelimiter as DirectListDelimiter, ListType as DirectListType, ParseError, SyntaxProfile,
};

#[cfg(test)]
use crate::committed_checkpoint_index::RelativeCheckpointMeasure;
use crate::committed_checkpoint_index::{
    ParentBoundDonorPartitionTransition, ParentBoundDonorSuccessor, ParentBoundDonorSuccessorStep,
};
use crate::parent_selected_convergence::{
    ParentSelectedConvergenceMapError, ParentSelectedConvergenceMapJob,
    ParentSelectedConvergenceMapProgress, ParentSelectedConvergenceMapStart,
    ParentSelectedConvergenceTargetRelation, ParentSelectedLiveDonorJoin,
    ParentSelectedMappedConvergence,
};
use crate::same_build_checkpoint::{
    ParserLineBoundaryCheckpointAuthority, SameBuildLineBoundaryCheckpoint,
};
use crate::serialized_green::setext_retained_restart::SetextRetainedGreenRestartReceipt;
use crate::setext_cross_build_restart::InMemorySetextCheckpointDraft;
use crate::{
    CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES, CandidateAbort, CandidateFencedCodeBoundary,
    CandidateLineBoundaryCheckpointAdmission, CandidateLineBoundaryCheckpointSkip,
    CandidateLineEnding, CandidateRecognitionByteAccessError, CandidateRecognitionBytePollError,
    CandidateRecognitionByteSession, CandidateRecognitionByteSessionFinishReceipt,
    CandidateRecognitionByteSource, CandidateRecognitionLineReceipt, CandidateRecognitionPoll,
    CandidateSourceAtomKind, CandidateTerminatorResolution, CandidateWriterBinding,
    CandidateWriterBindingIdentity, CandidateWriterBuiltDocument, CandidateWriterError,
    CandidateWriterLogicalAction, CandidateWriterProgress, CandidateWriterRangeRecipe,
    CandidateWriterSourcePoll, CapturedDonorCheckpointSample, CapturedParentSelectedSuffixSample,
    ClosedChildAggregate, CoveragePart, DonorCheckpointSampleCursor, FactsEnvelope, GreenAffinity,
    GreenCloseFacts, GreenFenceCharacter, GreenFencedCodeOpenFacts, GreenHeadingOpenFacts,
    GreenItemOpenFacts, GreenKind, GreenListBullet, GreenListDelimiter, GreenListOpenFacts,
    LiveCandidateEpoch, LiveDocumentError, LiveDocumentStore, ParentSelectedCandidateAdoptionTail,
    ParentSelectedCandidateWriterDriver, RestartCheckpointSampleChain,
    RetainedSetextDriverActivation, SerializedGreenError, SourcePhysicalLineEnding,
};

/// Private-field mint proving that only this exact-block module may split the
/// actor-owned parent-selected writer driver into parser state and its opaque
/// retained-parent tail.
pub(crate) struct ParentSelectedExactBlockDriverMint(());

/// Copyable progress visible outside the actor. It intentionally carries no
/// donor parser, binding, checkpoint, or retained-parent authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedExactBlockDriverProgress {
    Rejoining,
    Mapping,
    Running,
    ConvergenceCapture,
    ConvergenceJoinRequired,
    FullSuffixReplacementRequired,
}

/// The phase whose failure made ordinary replay on this candidate illegal.
/// Every variant requires whole-candidate cancellation; none is a fallback
/// admission result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedExactBlockAbortStage {
    Admission,
    RejoinCapture,
    RejoinResume,
    Mapping,
    Running,
    ConvergenceCapture,
    ConvergenceJoin,
    FullSuffix,
    PreviouslyFailed,
}

/// Typed poison result for a parent-selected candidate. Retained parent pages
/// may already be owned by its arena journal, so the only legal actor action
/// after this error is cancellation of the whole candidate.
#[derive(Debug)]
#[must_use = "a parent-selected exact-block failure requires whole-candidate cancellation"]
pub(crate) struct ParentSelectedExactBlockAbortRequired {
    stage: ParentSelectedExactBlockAbortStage,
    error: ExactBlockJobError,
}

impl ParentSelectedExactBlockAbortRequired {
    #[must_use]
    pub(crate) const fn stage(&self) -> ParentSelectedExactBlockAbortStage {
        self.stage
    }

    #[must_use]
    pub(crate) const fn error(&self) -> &ExactBlockJobError {
        &self.error
    }
}

#[derive(Debug)]
pub(crate) enum ExactBlockJobError {
    Parser(ParseError),
    Writer(CandidateWriterError),
    SourceLineDonor(DirectSourceLinePollError<CandidateRecognitionByteAccessError>),
    /// The parser has reached the production-shaped reference seam.  The
    /// actor must next adapt the completed open Paragraph projection into the
    /// parser-minted DFA, then join rejection or publication back here.
    ReferenceExternalWork(DirectExternalWorkKind),
    Convergence(ParentSelectedConvergenceMapError),
    Commit(CandidateWriterError),
    Failed,
    Invariant(&'static str),
}

impl From<ParseError> for ExactBlockJobError {
    fn from(error: ParseError) -> Self {
        Self::Parser(error)
    }
}

impl From<CandidateWriterError> for ExactBlockJobError {
    fn from(error: CandidateWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<ParentSelectedConvergenceMapError> for ExactBlockJobError {
    fn from(error: ParentSelectedConvergenceMapError) -> Self {
        Self::Convergence(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExactBlockJobProgress {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParentSelectedExactBlockRunningProgress {
    Pending,
    CheckpointIndexSpliceRequired,
}

enum ParentSelectedTargetPoll {
    Pending(Box<ExactBlockJob>),
    Capturing(Box<ExactBlockCheckpointCapture>),
    FullSuffix(Box<ExactBlockJob>),
}

#[derive(Debug)]
pub(crate) enum ExactBlockCheckpointAdmission {
    Started(Box<ExactBlockCheckpointCapture>),
    Skipped {
        job: Box<ExactBlockJob>,
        reason: ExactBlockCheckpointSkip,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExactBlockCheckpointSkip {
    ParserNotAtBoundary,
    Writer(CandidateLineBoundaryCheckpointSkip),
}

#[derive(Debug)]
pub(crate) enum ExactBlockCheckpointCapturePoll {
    Pending(ExactBlockCheckpointCapture),
    Ready(ExactBlockCheckpoint),
}

#[derive(Debug)]
pub(crate) struct ExactBlockCheckpointStartFailure {
    pub(crate) error: ExactBlockJobError,
    pub(crate) job: ExactBlockJob,
}

#[derive(Debug)]
pub(crate) struct ExactBlockCheckpointCaptureFailure {
    pub(crate) error: ExactBlockJobError,
    pub(crate) capture: ExactBlockCheckpointCapture,
}

#[derive(Debug)]
pub(crate) struct ExactBlockCheckpointResumeFailure {
    pub(crate) error: ExactBlockJobError,
    pub(crate) checkpoint: ExactBlockCheckpoint,
}

#[derive(Debug)]
pub(crate) struct ExactBlockDonorCheckpointSampleCaptureFailure {
    pub(crate) error: ExactBlockJobError,
    pub(crate) cursor: DonorCheckpointSampleCursor,
}

/// Driver-owned capture state. Arena, source, builder, identity, and abort
/// authority remain in `LiveDocumentStore` while its writer action is polled.
#[derive(Debug)]
pub(crate) struct ExactBlockCheckpointCapture {
    epoch: LiveCandidateEpoch,
    parser: Option<ParserLineBoundaryCheckpointAuthority>,
    bindings: Vec<CandidateWriterBinding>,
    line_input: ExactLineInput,
    source_line_metrics: ExactSourceLineMetrics,
    maximum_line_bytes: usize,
    acknowledged_lines: u64,
}

/// One fully joined same-build line boundary. This owns only parser/driver
/// state; the paused writer continuation remains actor-owned and cancellable.
#[derive(Debug)]
pub(crate) struct ExactBlockCheckpoint {
    epoch: LiveCandidateEpoch,
    joined: SameBuildLineBoundaryCheckpoint,
    bindings: Vec<CandidateWriterBinding>,
    line_input: ExactLineInput,
    source_line_metrics: ExactSourceLineMetrics,
    maximum_line_bytes: usize,
    acknowledged_lines: u64,
}

/// The mandatory same-build join is deliberately split across actor turns:
/// admission starts the writer checkpoint, capture advances it one bounded
/// transition at a time, and joined resumes the exact parser and writer as one
/// pair. No intermediate value can leave this module.
enum ParentSelectedExactBlockRejoin {
    Admission {
        parser: DirectValueBlockParser,
        bindings: Vec<CandidateWriterBinding>,
        acknowledged_lines: u64,
    },
    Capturing(Box<ExactBlockCheckpointCapture>),
    Joined(Box<ExactBlockCheckpoint>),
}

enum ParentSelectedExactBlockDriverState {
    Rejoining(ParentSelectedExactBlockRejoin),
    OldConvergenceStep {
        checkpoint: Box<ExactBlockCheckpoint>,
        step: ParentBoundDonorSuccessorStep,
    },
    AdvanceOldConvergence {
        checkpoint: Box<ExactBlockCheckpoint>,
        current: ParentBoundDonorSuccessor,
    },
    Mapping {
        checkpoint: Box<ExactBlockCheckpoint>,
        mapping: Box<ParentSelectedConvergenceMapJob>,
    },
    RunningToConvergence {
        job: Box<ExactBlockJob>,
        mapped: ParentSelectedMappedConvergence,
    },
    CapturingConvergence {
        capture: Box<ExactBlockCheckpointCapture>,
        mapped: ParentSelectedMappedConvergence,
    },
    JoinedConvergence {
        checkpoint: Box<ExactBlockCheckpoint>,
        mapped: ParentSelectedMappedConvergence,
    },
    RunningFullSuffix {
        job: Box<ExactBlockJob>,
    },
    ConvergenceJoinRequired {
        old_convergence: ParentBoundDonorSuccessor,
        receipt: crate::CandidateWriterTailAdoptionReceipt,
        certificate: crate::parent_selected_convergence::ParentSelectedMatchedLiveSampleCertificate,
        acknowledged_lines: u64,
    },
    FullSuffixReplacementRequired {
        job: Box<ExactBlockJob>,
    },
    AbortRequired,
}

/// Actor-owned suffix driver for one parent-selected persisted restart.
///
/// The exact donor parser and open bindings never cross into a scheduler
/// handle. The opaque adoption tail remains beside them through the mandatory
/// fresh same-build rejoin and suffix replay. This slice stops with the writer
/// completion sealed but uncommitted, before the future retained checkpoint-
/// index splice consumes the tail.
#[must_use = "the parent-selected suffix driver must reach splice/adoption or cancellation"]
pub(crate) struct ParentSelectedExactBlockDriver {
    epoch: LiveCandidateEpoch,
    state: ParentSelectedExactBlockDriverState,
    tail: ParentSelectedCandidateAdoptionTail,
}

/// Terminal exact-parser half of the parent-selected adoption rendezvous.
/// It can be opened only by `candidate_writer`, where it must meet the
/// source/composer/green/checkpoint bundle from the same live candidate.
#[must_use = "matched convergence authority must enter the adoption splice or be cancelled"]
pub(crate) struct ParentSelectedExactConvergenceAdoption {
    epoch: LiveCandidateEpoch,
    tail: ParentSelectedCandidateAdoptionTail,
    old_convergence: ParentBoundDonorSuccessor,
    certificate: crate::parent_selected_convergence::ParentSelectedMatchedLiveSampleCertificate,
    writer_receipt: crate::CandidateWriterTailAdoptionReceipt,
    acknowledged_lines: u64,
}

impl ParentSelectedExactConvergenceAdoption {
    pub(crate) fn into_candidate_splice_parts(
        self,
        _mint: crate::candidate_writer::ParentSelectedAdoptionSpliceMint,
    ) -> (
        LiveCandidateEpoch,
        ParentSelectedCandidateAdoptionTail,
        ParentBoundDonorSuccessor,
        crate::parent_selected_convergence::ParentSelectedMatchedLiveSampleCertificate,
        crate::CandidateWriterTailAdoptionReceipt,
        u64,
    ) {
        (
            self.epoch,
            self.tail,
            self.old_convergence,
            self.certificate,
            self.writer_receipt,
            self.acknowledged_lines,
        )
    }
}

impl std::fmt::Debug for ParentSelectedExactBlockDriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phase = match &self.state {
            ParentSelectedExactBlockDriverState::Rejoining(_) => "Rejoining",
            ParentSelectedExactBlockDriverState::OldConvergenceStep { .. }
            | ParentSelectedExactBlockDriverState::AdvanceOldConvergence { .. }
            | ParentSelectedExactBlockDriverState::Mapping { .. } => "Mapping",
            ParentSelectedExactBlockDriverState::RunningToConvergence { .. }
            | ParentSelectedExactBlockDriverState::RunningFullSuffix { .. } => "Running",
            ParentSelectedExactBlockDriverState::CapturingConvergence { .. }
            | ParentSelectedExactBlockDriverState::JoinedConvergence { .. } => "ConvergenceCapture",
            ParentSelectedExactBlockDriverState::ConvergenceJoinRequired { .. } => {
                "ConvergenceJoinRequired"
            }
            ParentSelectedExactBlockDriverState::FullSuffixReplacementRequired { .. } => {
                "FullSuffixReplacementRequired"
            }
            ParentSelectedExactBlockDriverState::AbortRequired => "AbortRequired",
        };
        formatter
            .debug_struct("ParentSelectedExactBlockDriver")
            .field("epoch", &self.epoch)
            .field("phase", &phase)
            .field("retained_parent_tail", &"opaque")
            .finish()
    }
}

/// Conservative composed bound for one source-backed donor poll.
///
/// Donor transition fuel and its reported lexical-work units are not the same
/// axis (fuel one may report several lexical units). The honest proof bound is
/// therefore the conservative sum of the actor's hard source-access cap and a
/// separately enforced donor lexical cap. Session admission, finish, and
/// donor commit are separate unit-cost polls.
const EXACT_SOURCE_LINE_MAX_DONOR_LEXICAL_WORK: usize = 4 * 1024;
pub(crate) const EXACT_SOURCE_LINE_MAX_COMPOSED_WORK_PER_POLL: usize =
    CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES + EXACT_SOURCE_LINE_MAX_DONOR_LEXICAL_WORK;
const EXACT_SOURCE_LINE_MAX_DONOR_FUEL: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExactLineInput {
    Buffered,
    SourceBackedAtx { donor_fuel: usize },
}

impl ExactLineInput {
    const fn source_backed_atx() -> Self {
        Self::SourceBackedAtx {
            donor_fuel: EXACT_SOURCE_LINE_MAX_DONOR_FUEL,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ExactSourceLineMetrics {
    lines_committed: u64,
    donor_polls: u64,
    donor_lexical_work_units: u64,
    source_first_reads: u64,
    actor_access_work_units: u64,
    actor_new_bytes: u64,
    actor_repeated_last_byte_peeks: u64,
    maximum_donor_retained_source_bytes: usize,
    maximum_actor_retained_byte_scratch: usize,
    maximum_source_request_rewind_bytes: usize,
    maximum_composed_work_per_poll: usize,
}

enum ExactSourceLinePhase {
    SessionOpen {
        session: CandidateRecognitionByteSession,
    },
    Polling {
        session: CandidateRecognitionByteSession,
        work: DirectSourceLineWork<CandidateRecognitionByteSession>,
    },
    Matched {
        session: CandidateRecognitionByteSession,
        work: DirectSourceLineWork<CandidateRecognitionByteSession>,
    },
    ActorFinished {
        work: DirectSourceLineWork<CandidateRecognitionByteSession>,
        receipt: CandidateRecognitionByteSessionFinishReceipt,
    },
}

impl ExactSourceLinePhase {
    const fn label(&self) -> &'static str {
        match self {
            Self::SessionOpen { .. } => "SessionOpen",
            Self::Polling { .. } => "Polling",
            Self::Matched { .. } => "Matched",
            Self::ActorFinished { .. } => "ActorFinished",
        }
    }

    fn retained_source_bytes(&self) -> usize {
        match self {
            Self::SessionOpen { .. } => 0,
            Self::Polling { work, .. }
            | Self::Matched { work, .. }
            | Self::ActorFinished { work, .. } => work.retained_source_bytes(),
        }
    }
}

struct ExactDirectSourceLineAdapter<'source, 'ledger> {
    source: &'source mut CandidateRecognitionByteSource<'ledger>,
}

enum ExactSourceLineScannerError {
    Donor(DirectSourceLinePollError<CandidateRecognitionByteAccessError>),
}

impl DirectSourceLineSource for ExactDirectSourceLineAdapter<'_, '_> {
    type Identity = CandidateRecognitionByteSession;
    type Error = CandidateRecognitionByteAccessError;

    fn identity(&self) -> Self::Identity {
        self.source.session()
    }

    fn len(&self) -> usize {
        self.source.len()
    }

    fn access_budget(&self) -> usize {
        self.source.remaining_access_budget()
    }

    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error> {
        self.source.byte_at(absolute_offset)
    }
}

#[derive(Debug)]
enum DriverMode {
    Initial,
    Recognizing,
    PollLine,
    PollFinish,
    Complete,
    Failed,
}

#[derive(Debug)]
struct CurrentLine {
    recognition: CandidateRecognitionLineReceipt,
    physical_bytes: u32,
    physical_utf16: u32,
    content_end: u32,
    authoritative_offset: u32,
}

#[derive(Clone, Copy, Debug)]
enum SourceWork {
    ConsumeRange {
        end: u32,
        owner_index: usize,
        part: CoveragePart,
        logical: DirectLogicalAction,
    },
    StageBlankRange {
        end: u32,
    },
    StageTerminator {
        end: u32,
        ending: DirectLineEnding,
    },
    EnsureLineEof {
        physical_bytes: u32,
        physical_utf16: u32,
    },
    EnsureEof,
}

#[derive(Debug)]
enum WriterWork {
    Open {
        kind: GreenKind,
    },
    AtomConsume {
        advance: u32,
    },
    RangeReplay {
        start: u32,
        end: u32,
    },
    FinalizeParagraph {
        expected_identity: CandidateWriterBindingIdentity,
        expected_facts: GreenHeadingOpenFacts,
    },
    ReferencePrefixSetup {
        request: DirectReferencePrefixRequest,
    },
    ReferencePrefixDrive {
        request: DirectReferencePrefixRequest,
        identity: crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionIdentity,
    },
    Resolve,
    Close,
    Finish,
}

/// Private linear job joining the exact donor state machine to one candidate.
///
/// Every call performs at most one parser transition, one source poll, or one
/// writer poll. The only temporary text is the parser proof slice's shared
/// `DirectValueBlockParser::MAX_LINE_BYTES` line buffer.
pub(crate) struct ExactBlockJob {
    epoch: LiveCandidateEpoch,
    parser: Option<DirectValueBlockParser>,
    mode: DriverMode,
    bindings: Vec<CandidateWriterBinding>,
    line_input: ExactLineInput,
    source_line_phase: Option<ExactSourceLinePhase>,
    source_line_metrics: ExactSourceLineMetrics,
    recognition_line: String,
    recognition_saw_atom: bool,
    current_line: Option<CurrentLine>,
    active_command: Option<DirectCommand>,
    source_work: Option<SourceWork>,
    writer_work: Option<WriterWork>,
    restart_samples: Option<RestartCheckpointSampleChain>,
    built: Option<CandidateWriterBuiltDocument>,
    failed_commit_abort: Option<CandidateAbort>,
    maximum_line_bytes: usize,
    acknowledged_lines: u64,
    checkpoint_eligible: bool,
}

impl std::fmt::Debug for ExactBlockJob {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExactBlockJob")
            .field("epoch", &self.epoch)
            .field("parser_present", &self.parser.is_some())
            .field("mode", &self.mode)
            .field("open_bindings", &self.bindings.len())
            .field("line_input", &self.line_input)
            .field(
                "source_line_phase",
                &self
                    .source_line_phase
                    .as_ref()
                    .map(ExactSourceLinePhase::label),
            )
            .field("source_line_metrics", &self.source_line_metrics)
            .field("active_command", &self.active_command)
            .field("current_line", &self.current_line)
            .field("restart_samples", &self.restart_samples.is_some())
            .field("failed_commit_abort", &self.failed_commit_abort)
            .field("maximum_line_bytes", &self.maximum_line_bytes)
            .field("acknowledged_lines", &self.acknowledged_lines)
            .field("checkpoint_eligible", &self.checkpoint_eligible)
            .finish_non_exhaustive()
    }
}

impl ExactBlockCheckpointCapture {
    /// Advances exactly one actor-side writer transition. The parser pause and
    /// driver state stay linear in this value until the actor installs its
    /// paused continuation and the full join succeeds.
    pub(crate) fn poll(
        mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<ExactBlockCheckpointCapturePoll, Box<ExactBlockCheckpointCaptureFailure>> {
        let progress = match document.poll_candidate_writer(self.epoch) {
            Ok(progress) => progress,
            Err(error) => {
                return Err(Box::new(ExactBlockCheckpointCaptureFailure {
                    error: ExactBlockJobError::Writer(error),
                    capture: self,
                }));
            }
        };
        match progress {
            CandidateWriterProgress::Pending => Ok(ExactBlockCheckpointCapturePoll::Pending(self)),
            CandidateWriterProgress::LineBoundaryCheckpointReady => {
                let parser = self
                    .parser
                    .take()
                    .expect("checkpoint capture owns one direct parser pause");
                match document.pause_candidate_writer_at_line_boundary(
                    self.epoch,
                    parser,
                    &self.bindings,
                ) {
                    Ok(joined) => Ok(ExactBlockCheckpointCapturePoll::Ready(
                        ExactBlockCheckpoint {
                            epoch: self.epoch,
                            joined,
                            bindings: self.bindings,
                            line_input: self.line_input,
                            source_line_metrics: self.source_line_metrics,
                            maximum_line_bytes: self.maximum_line_bytes,
                            acknowledged_lines: self.acknowledged_lines,
                        },
                    )),
                    Err(failure) => {
                        let (parser, error) = *failure;
                        self.parser = Some(parser);
                        Err(Box::new(ExactBlockCheckpointCaptureFailure {
                            error: ExactBlockJobError::Writer(error),
                            capture: self,
                        }))
                    }
                }
            }
            CandidateWriterProgress::ActionComplete
            | CandidateWriterProgress::Opened(_)
            | CandidateWriterProgress::Retyped { .. }
            | CandidateWriterProgress::RetypedWithDeferredResidual { .. }
            | CandidateWriterProgress::TableHeaderInputReady
            | CandidateWriterProgress::ReferencePrefixSourceReady { .. }
            | CandidateWriterProgress::ReferencePrefixTerminalReady(_)
            | CandidateWriterProgress::RetypedTable { .. }
            | CandidateWriterProgress::IdentityLineReady { .. }
            | CandidateWriterProgress::RangeReplayReady(_)
            | CandidateWriterProgress::CompletionReady => {
                Err(Box::new(ExactBlockCheckpointCaptureFailure {
                    error: ExactBlockJobError::Invariant(
                        "checkpoint writer returned unrelated progress",
                    ),
                    capture: self,
                }))
            }
        }
    }

    pub(crate) fn cancel(
        self,
        document: &mut LiveDocumentStore,
    ) -> Result<CandidateAbort, LiveDocumentError> {
        document.cancel_candidate(self.epoch)
    }
}

impl ExactBlockCheckpoint {
    pub(crate) const fn acknowledged_lines(&self) -> u64 {
        self.acknowledged_lines
    }

    /// Captures the donor/source/green draft while both halves of the joined
    /// checkpoint remain paused and retryable. Ordinary resume is unchanged
    /// and happens only after this read-only minting step succeeds.
    pub(crate) fn capture_in_memory_setext_checkpoint(
        &self,
        document: &LiveDocumentStore,
    ) -> Result<InMemorySetextCheckpointDraft, ExactBlockJobError> {
        let joined_donor = self.joined.capture_joined_donor_sample()?;
        document
            .capture_candidate_writer_in_memory_setext_checkpoint(
                self.epoch,
                &self.bindings,
                joined_donor,
            )
            .map_err(ExactBlockJobError::Writer)
    }

    /// Starts one sparse donor chain at this joined line boundary. The driver
    /// supplies only the checkpoint object; all five cumulative coordinates
    /// are observed inside the actor's paused writer.
    pub(crate) fn capture_first_donor_checkpoint_sample(
        &self,
        document: &mut LiveDocumentStore,
    ) -> Result<CapturedDonorCheckpointSample, ExactBlockJobError> {
        let joined_donor = self.joined.capture_joined_donor_sample()?;
        document
            .capture_candidate_writer_first_donor_checkpoint_sample(
                self.epoch,
                &self.bindings,
                joined_donor,
            )
            .map_err(ExactBlockJobError::Writer)
    }

    /// Extends the sparse donor chain. Every failure returns the unchanged
    /// cursor, including a parser-side donor capture failure that occurs
    /// before the actor is borrowed.
    pub(crate) fn capture_successive_donor_checkpoint_sample(
        &self,
        document: &mut LiveDocumentStore,
        cursor: DonorCheckpointSampleCursor,
    ) -> Result<CapturedDonorCheckpointSample, Box<ExactBlockDonorCheckpointSampleCaptureFailure>>
    {
        let joined_donor = match self.joined.capture_joined_donor_sample() {
            Ok(donor) => donor,
            Err(error) => {
                return Err(Box::new(ExactBlockDonorCheckpointSampleCaptureFailure {
                    error: ExactBlockJobError::Parser(error),
                    cursor,
                }));
            }
        };
        match document.capture_candidate_writer_successive_donor_checkpoint_sample(
            self.epoch,
            &self.bindings,
            joined_donor,
            cursor,
        ) {
            Ok(capture) => Ok(capture),
            Err(failure) => Err(Box::new(ExactBlockDonorCheckpointSampleCaptureFailure {
                error: ExactBlockJobError::Writer(failure.error),
                cursor: failure.cursor,
            })),
        }
    }

    /// Captures a post-restart sample from the writer-owned parent-selected
    /// chain. The mandatory restart rejoin and this ordinary line-boundary
    /// checkpoint share the exact parser/writer validation path; no driver
    /// coordinate or external cursor enters the operation.
    pub(crate) fn capture_parent_selected_suffix_sample(
        &self,
        document: &mut LiveDocumentStore,
    ) -> Result<CapturedParentSelectedSuffixSample, ExactBlockJobError> {
        let joined_donor = self.joined.capture_joined_donor_sample()?;
        document
            .capture_candidate_writer_parent_selected_suffix_sample(
                self.epoch,
                &self.bindings,
                joined_donor,
            )
            .map_err(ExactBlockJobError::Writer)
    }

    /// Selects the first old C while the mandatory fresh R rejoin keeps both
    /// parser and writer paused. No copied offset or persisted ordinal enters
    /// this transition.
    pub(crate) fn begin_parent_selected_old_convergence(
        &self,
        document: &LiveDocumentStore,
        tail: &ParentSelectedCandidateAdoptionTail,
    ) -> Result<ParentBoundDonorSuccessorStep, ExactBlockJobError> {
        document
            .begin_candidate_writer_parent_selected_old_convergence(self.epoch, tail)
            .map_err(ExactBlockJobError::Writer)
    }

    /// Advances old C after a live mismatch while the same actor checkpoint
    /// remains paused and the retained parent tail remains joined.
    pub(crate) fn advance_parent_selected_old_convergence(
        &self,
        document: &LiveDocumentStore,
        tail: &ParentSelectedCandidateAdoptionTail,
        current: ParentBoundDonorSuccessor,
    ) -> Result<ParentBoundDonorSuccessorStep, ExactBlockJobError> {
        document
            .advance_candidate_writer_parent_selected_old_convergence(self.epoch, tail, current)
            .map_err(ExactBlockJobError::Writer)
    }

    /// Crosses one immediate, retained-parent-authenticated donor partition.
    /// Barriers use a different terminal step and cannot enter this method.
    pub(crate) fn advance_parent_selected_old_convergence_partition(
        &self,
        document: &LiveDocumentStore,
        tail: &ParentSelectedCandidateAdoptionTail,
        transition: ParentBoundDonorPartitionTransition,
    ) -> Result<ParentBoundDonorSuccessorStep, ExactBlockJobError> {
        document
            .advance_candidate_writer_parent_selected_old_convergence_partition(
                self.epoch, tail, transition,
            )
            .map_err(ExactBlockJobError::Writer)
    }

    /// Starts source/green mapping for one parent-selected old C while this
    /// exact parser and the candidate writer remain joined and paused.
    pub(crate) fn begin_parent_selected_convergence_mapping(
        &self,
        document: &LiveDocumentStore,
        tail: &ParentSelectedCandidateAdoptionTail,
        old_convergence: ParentBoundDonorSuccessor,
    ) -> Result<ParentSelectedConvergenceMapStart, ExactBlockJobError> {
        document
            .begin_parent_selected_convergence_mapping(self.epoch, tail, old_convergence)
            .map_err(ExactBlockJobError::Convergence)
    }

    #[cfg(test)]
    fn parser_retention_for_test(
        &self,
    ) -> flark_comrak_value_block_core::DirectLineBoundaryPauseReceipt {
        self.joined.parser_retention_for_test()
    }

    /// Restores the actor-owned writer first while this value still owns the
    /// retryable parser pause. A writer-side validation failure therefore
    /// returns the intact checkpoint and leaves the continuation paused.
    pub(crate) fn resume(
        self,
        document: &mut LiveDocumentStore,
    ) -> Result<ExactBlockJob, Box<ExactBlockCheckpointResumeFailure>> {
        if let Err(error) = self.joined.validate_parser_resume() {
            return Err(Box::new(ExactBlockCheckpointResumeFailure {
                error: ExactBlockJobError::Parser(error),
                checkpoint: self,
            }));
        }
        if let Err(error) =
            document.resume_candidate_writer_at_line_boundary(self.epoch, &self.joined)
        {
            return Err(Box::new(ExactBlockCheckpointResumeFailure {
                error: ExactBlockJobError::Writer(error),
                checkpoint: self,
            }));
        }
        let parser = self
            .joined
            .resume_parser()
            .expect("the unchanged parser pause was validated before writer restoration");
        Ok(ExactBlockJob {
            epoch: self.epoch,
            parser: Some(parser),
            mode: DriverMode::Recognizing,
            bindings: self.bindings,
            line_input: self.line_input,
            source_line_phase: None,
            source_line_metrics: self.source_line_metrics,
            recognition_line: String::new(),
            recognition_saw_atom: false,
            current_line: None,
            active_command: None,
            source_work: None,
            writer_work: None,
            restart_samples: None,
            built: None,
            failed_commit_abort: None,
            maximum_line_bytes: self.maximum_line_bytes,
            acknowledged_lines: self.acknowledged_lines,
            checkpoint_eligible: false,
        })
    }

    pub(crate) fn cancel(
        self,
        document: &mut LiveDocumentStore,
    ) -> Result<CandidateAbort, LiveDocumentError> {
        document.cancel_candidate(self.epoch)
    }
}

impl ParentSelectedExactBlockDriver {
    /// Consumes the only candidate-writer carrier through this module's
    /// private-field mint. The transition is infallible and does no actor
    /// work: the first bounded `poll` performs mandatory rejoin admission.
    pub(crate) fn begin(driver: ParentSelectedCandidateWriterDriver) -> Self {
        let (epoch, parser, bindings, acknowledged_lines, tail) =
            driver.into_exact_driver_parts(ParentSelectedExactBlockDriverMint(()));
        Self {
            epoch,
            state: ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Admission {
                    parser,
                    bindings,
                    acknowledged_lines,
                },
            ),
            tail,
        }
    }

    /// Advances at most one existing exact-driver or checkpoint transition.
    /// Any failure poisons this carrier permanently; the actor must cancel the
    /// whole candidate, including the opaque retained-parent tail.
    pub(crate) fn poll(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<ParentSelectedExactBlockDriverProgress, ParentSelectedExactBlockAbortRequired> {
        if self.tail.build_id() != self.epoch.build_id() {
            return self.abort_required(
                ParentSelectedExactBlockAbortStage::Admission,
                ExactBlockJobError::Invariant(
                    "parent-selected exact driver and retained tail builds differ",
                ),
            );
        }

        let state = std::mem::replace(
            &mut self.state,
            ParentSelectedExactBlockDriverState::AbortRequired,
        );
        match state {
            ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Admission {
                    parser,
                    bindings,
                    acknowledged_lines,
                },
            ) => match ExactBlockJob::start_retained_rejoin_parts(
                self.epoch,
                parser,
                bindings,
                acknowledged_lines,
                document,
            ) {
                Ok(capture) => {
                    self.state = ParentSelectedExactBlockDriverState::Rejoining(
                        ParentSelectedExactBlockRejoin::Capturing(capture),
                    );
                    Ok(ParentSelectedExactBlockDriverProgress::Rejoining)
                }
                Err(failure) => {
                    let ExactBlockCheckpointStartFailure { error, job: _ } = *failure;
                    self.abort_required(ParentSelectedExactBlockAbortStage::Admission, error)
                }
            },
            ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Capturing(capture),
            ) => match (*capture).poll(document) {
                Ok(ExactBlockCheckpointCapturePoll::Pending(capture)) => {
                    self.state = ParentSelectedExactBlockDriverState::Rejoining(
                        ParentSelectedExactBlockRejoin::Capturing(Box::new(capture)),
                    );
                    Ok(ParentSelectedExactBlockDriverProgress::Rejoining)
                }
                Ok(ExactBlockCheckpointCapturePoll::Ready(checkpoint)) => {
                    self.state = ParentSelectedExactBlockDriverState::Rejoining(
                        ParentSelectedExactBlockRejoin::Joined(Box::new(checkpoint)),
                    );
                    Ok(ParentSelectedExactBlockDriverProgress::Rejoining)
                }
                Err(failure) => {
                    let ExactBlockCheckpointCaptureFailure { error, capture: _ } = *failure;
                    self.abort_required(ParentSelectedExactBlockAbortStage::RejoinCapture, error)
                }
            },
            ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Joined(checkpoint),
            ) => {
                let step = match checkpoint
                    .begin_parent_selected_old_convergence(document, &self.tail)
                {
                    Ok(step) => step,
                    Err(error) => {
                        return self
                            .abort_required(ParentSelectedExactBlockAbortStage::Mapping, error);
                    }
                };
                self.state =
                    ParentSelectedExactBlockDriverState::OldConvergenceStep { checkpoint, step };
                Ok(ParentSelectedExactBlockDriverProgress::Mapping)
            }
            ParentSelectedExactBlockDriverState::OldConvergenceStep { checkpoint, step } => {
                match step {
                    ParentBoundDonorSuccessorStep::Checkpoint(old_convergence) => {
                        match checkpoint.begin_parent_selected_convergence_mapping(
                            document,
                            &self.tail,
                            old_convergence,
                        ) {
                            Ok(ParentSelectedConvergenceMapStart::Mapping(mapping)) => {
                                self.state = ParentSelectedExactBlockDriverState::Mapping {
                                    checkpoint,
                                    mapping: Box::new(mapping),
                                };
                                Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                            }
                            Ok(ParentSelectedConvergenceMapStart::Ineligible {
                                old_convergence,
                                reason: _,
                            }) => {
                                self.state =
                                    ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                                        checkpoint,
                                        current: old_convergence,
                                    };
                                Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                            }
                            Err(error) => self
                                .abort_required(ParentSelectedExactBlockAbortStage::Mapping, error),
                        }
                    }
                    ParentBoundDonorSuccessorStep::NextPartition(transition) => {
                        match checkpoint.advance_parent_selected_old_convergence_partition(
                            document, &self.tail, transition,
                        ) {
                            Ok(step) => {
                                self.state =
                                    ParentSelectedExactBlockDriverState::OldConvergenceStep {
                                        checkpoint,
                                        step,
                                    };
                                Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                            }
                            Err(error) => self
                                .abort_required(ParentSelectedExactBlockAbortStage::Mapping, error),
                        }
                    }
                    ParentBoundDonorSuccessorStep::PartitionEnd(_) => {
                        match (*checkpoint).resume(document) {
                            Ok(job) => {
                                self.state =
                                    ParentSelectedExactBlockDriverState::RunningFullSuffix {
                                        job: Box::new(job),
                                    };
                                Ok(ParentSelectedExactBlockDriverProgress::Running)
                            }
                            Err(failure) => {
                                let ExactBlockCheckpointResumeFailure {
                                    error,
                                    checkpoint: _,
                                } = *failure;
                                self.abort_required(
                                    ParentSelectedExactBlockAbortStage::RejoinResume,
                                    error,
                                )
                            }
                        }
                    }
                }
            }
            ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                checkpoint,
                current,
            } => match checkpoint
                .advance_parent_selected_old_convergence(document, &self.tail, current)
            {
                Ok(step) => {
                    self.state = ParentSelectedExactBlockDriverState::OldConvergenceStep {
                        checkpoint,
                        step,
                    };
                    Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                }
                Err(error) => {
                    self.abort_required(ParentSelectedExactBlockAbortStage::Mapping, error)
                }
            },
            ParentSelectedExactBlockDriverState::Mapping {
                checkpoint,
                mut mapping,
            } => {
                match document.poll_parent_selected_convergence_mapping(self.epoch, &mut mapping, 1)
                {
                    Ok(ParentSelectedConvergenceMapProgress::Pending { .. }) => {
                        self.state = ParentSelectedExactBlockDriverState::Mapping {
                            checkpoint,
                            mapping,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                    }
                    Ok(ParentSelectedConvergenceMapProgress::Changed {
                        old_convergence,
                        region: _,
                        ..
                    }) => {
                        self.state = ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                            checkpoint,
                            current: old_convergence,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                    }
                    Ok(ParentSelectedConvergenceMapProgress::Ineligible {
                        old_convergence,
                        reason: _,
                    }) => {
                        self.state = ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                            checkpoint,
                            current: old_convergence,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                    }
                    Ok(ParentSelectedConvergenceMapProgress::Mapped(mapped)) => {
                        match document.candidate_writer_parent_selected_convergence_relation(
                            self.epoch, &mapped,
                        ) {
                            Ok(ParentSelectedConvergenceTargetRelation::Before) => {
                                match (*checkpoint).resume(document) {
                                    Ok(job) => {
                                        self.state = ParentSelectedExactBlockDriverState::RunningToConvergence {
                                        job: Box::new(job),
                                        mapped,
                                    };
                                        Ok(ParentSelectedExactBlockDriverProgress::Running)
                                    }
                                    Err(failure) => {
                                        let ExactBlockCheckpointResumeFailure {
                                            error,
                                            checkpoint: _,
                                        } = *failure;
                                        self.abort_required(
                                            ParentSelectedExactBlockAbortStage::RejoinResume,
                                            error,
                                        )
                                    }
                                }
                            }
                            Ok(ParentSelectedConvergenceTargetRelation::At)
                            | Ok(ParentSelectedConvergenceTargetRelation::Past) => {
                                self.state =
                                    ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                                        checkpoint,
                                        current: mapped.into_mismatch_old_convergence(),
                                    };
                                Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                            }
                            Err(error) => self.abort_required(
                                ParentSelectedExactBlockAbortStage::Mapping,
                                ExactBlockJobError::Writer(error),
                            ),
                        }
                    }
                    Err(error) => self.abort_required(
                        ParentSelectedExactBlockAbortStage::Mapping,
                        ExactBlockJobError::Convergence(error),
                    ),
                }
            }
            ParentSelectedExactBlockDriverState::RunningToConvergence { job, mapped } => {
                match job.poll_to_parent_selected_target(document, &mapped) {
                    Ok(ParentSelectedTargetPoll::Pending(job)) => {
                        self.state = ParentSelectedExactBlockDriverState::RunningToConvergence {
                            job,
                            mapped,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::Running)
                    }
                    Ok(ParentSelectedTargetPoll::Capturing(capture)) => {
                        self.state = ParentSelectedExactBlockDriverState::CapturingConvergence {
                            capture,
                            mapped,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::ConvergenceCapture)
                    }
                    Ok(ParentSelectedTargetPoll::FullSuffix(job)) => {
                        self.state = ParentSelectedExactBlockDriverState::RunningFullSuffix { job };
                        Ok(ParentSelectedExactBlockDriverProgress::Running)
                    }
                    Err(error) => {
                        self.abort_required(ParentSelectedExactBlockAbortStage::Running, error)
                    }
                }
            }
            ParentSelectedExactBlockDriverState::CapturingConvergence { capture, mapped } => {
                match (*capture).poll(document) {
                    Ok(ExactBlockCheckpointCapturePoll::Pending(capture)) => {
                        self.state = ParentSelectedExactBlockDriverState::CapturingConvergence {
                            capture: Box::new(capture),
                            mapped,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::ConvergenceCapture)
                    }
                    Ok(ExactBlockCheckpointCapturePoll::Ready(checkpoint)) => {
                        self.state = ParentSelectedExactBlockDriverState::JoinedConvergence {
                            checkpoint: Box::new(checkpoint),
                            mapped,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::ConvergenceCapture)
                    }
                    Err(failure) => {
                        let ExactBlockCheckpointCaptureFailure { error, capture: _ } = *failure;
                        self.abort_required(
                            ParentSelectedExactBlockAbortStage::ConvergenceCapture,
                            error,
                        )
                    }
                }
            }
            ParentSelectedExactBlockDriverState::JoinedConvergence { checkpoint, mapped } => {
                let sample = match checkpoint.capture_parent_selected_suffix_sample(document) {
                    Ok(sample) => sample,
                    Err(error) => {
                        return self.abort_required(
                            ParentSelectedExactBlockAbortStage::ConvergenceJoin,
                            error,
                        );
                    }
                };
                match mapped.join_live_donor(sample) {
                    Ok(ParentSelectedLiveDonorJoin::Mismatch {
                        old_convergence,
                        rejected,
                    }) => {
                        if let Err(error) = document
                            .reject_candidate_writer_parent_selected_suffix_sample(
                                self.epoch, rejected,
                            )
                        {
                            return self.abort_required(
                                ParentSelectedExactBlockAbortStage::ConvergenceJoin,
                                ExactBlockJobError::Writer(error),
                            );
                        }
                        self.state = ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                            checkpoint,
                            current: old_convergence,
                        };
                        Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                    }
                    Ok(ParentSelectedLiveDonorJoin::Match(matched)) => {
                        let (old_convergence, tail, certificate) = matched.into_adoption_parts();
                        match document
                            .candidate_writer_parent_selected_tail_splice_is_eligible(self.epoch)
                        {
                            Ok(false) => {
                                drop(tail);
                                drop(certificate);
                                self.state =
                                    ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                                        checkpoint,
                                        current: old_convergence,
                                    };
                                return Ok(ParentSelectedExactBlockDriverProgress::Mapping);
                            }
                            Err(error) => {
                                return self.abort_required(
                                    ParentSelectedExactBlockAbortStage::ConvergenceJoin,
                                    ExactBlockJobError::Writer(error),
                                );
                            }
                            Ok(true) => {}
                        }
                        match document.candidate_writer_parent_selected_green_suffix_preflight(
                            self.epoch, &self.tail, &tail,
                        ) {
                            Ok(crate::GreenJournalSuffixPreflight::Ineligible(_)) => {
                                drop(tail);
                                drop(certificate);
                                self.state =
                                    ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                                        checkpoint,
                                        current: old_convergence,
                                    };
                                return Ok(ParentSelectedExactBlockDriverProgress::Mapping);
                            }
                            Err(error) => {
                                return self.abort_required(
                                    ParentSelectedExactBlockAbortStage::ConvergenceJoin,
                                    ExactBlockJobError::Writer(error),
                                );
                            }
                            Ok(crate::GreenJournalSuffixPreflight::Eligible) => {}
                        }
                        match document.adopt_candidate_writer_source_composer_tail(self.epoch, tail)
                        {
                            Ok(receipt) => {
                                let acknowledged_lines = checkpoint.acknowledged_lines();
                                self.state =
                                    ParentSelectedExactBlockDriverState::ConvergenceJoinRequired {
                                        old_convergence,
                                        receipt,
                                        certificate,
                                        acknowledged_lines,
                                    };
                                Ok(ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired)
                            }
                            Err(error) if tail_adoption_is_ineligible(&error) => {
                                self.state =
                                    ParentSelectedExactBlockDriverState::AdvanceOldConvergence {
                                        checkpoint,
                                        current: old_convergence,
                                    };
                                Ok(ParentSelectedExactBlockDriverProgress::Mapping)
                            }
                            Err(error) => self.abort_required(
                                ParentSelectedExactBlockAbortStage::ConvergenceJoin,
                                ExactBlockJobError::Writer(error),
                            ),
                        }
                    }
                    Err(error) => self.abort_required(
                        ParentSelectedExactBlockAbortStage::ConvergenceJoin,
                        ExactBlockJobError::Convergence(error),
                    ),
                }
            }
            ParentSelectedExactBlockDriverState::RunningFullSuffix { mut job } => {
                match job.poll_parent_selected_suffix(document) {
                    Ok(ParentSelectedExactBlockRunningProgress::Pending) => {
                        self.state = ParentSelectedExactBlockDriverState::RunningFullSuffix { job };
                        Ok(ParentSelectedExactBlockDriverProgress::Running)
                    }
                    Ok(ParentSelectedExactBlockRunningProgress::CheckpointIndexSpliceRequired) => {
                        self.state =
                            ParentSelectedExactBlockDriverState::FullSuffixReplacementRequired {
                                job,
                            };
                        Ok(ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired)
                    }
                    Err(error) => {
                        self.abort_required(ParentSelectedExactBlockAbortStage::FullSuffix, error)
                    }
                }
            }
            ParentSelectedExactBlockDriverState::ConvergenceJoinRequired {
                old_convergence,
                receipt,
                certificate,
                acknowledged_lines,
            } => {
                self.state = ParentSelectedExactBlockDriverState::ConvergenceJoinRequired {
                    old_convergence,
                    receipt,
                    certificate,
                    acknowledged_lines,
                };
                Ok(ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired)
            }
            ParentSelectedExactBlockDriverState::FullSuffixReplacementRequired { job } => {
                self.state =
                    ParentSelectedExactBlockDriverState::FullSuffixReplacementRequired { job };
                Ok(ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired)
            }
            ParentSelectedExactBlockDriverState::AbortRequired => {
                self.state = ParentSelectedExactBlockDriverState::AbortRequired;
                Err(ParentSelectedExactBlockAbortRequired {
                    stage: ParentSelectedExactBlockAbortStage::PreviouslyFailed,
                    error: ExactBlockJobError::Failed,
                })
            }
        }
    }

    /// Consumes only the parked matched-C terminal state. Calling this before
    /// convergence preserves and returns the complete exact driver, so no
    /// retained-parent or parser authority is lost on phase mistakes.
    pub(crate) fn into_convergence_adoption(
        self,
    ) -> Result<ParentSelectedExactConvergenceAdoption, Self> {
        let Self { epoch, state, tail } = self;
        match state {
            ParentSelectedExactBlockDriverState::ConvergenceJoinRequired {
                old_convergence,
                receipt,
                certificate,
                acknowledged_lines,
            } => Ok(ParentSelectedExactConvergenceAdoption {
                epoch,
                tail,
                old_convergence,
                certificate,
                writer_receipt: receipt,
                acknowledged_lines,
            }),
            state => Err(Self { epoch, state, tail }),
        }
    }

    fn abort_required(
        &mut self,
        stage: ParentSelectedExactBlockAbortStage,
        error: ExactBlockJobError,
    ) -> Result<ParentSelectedExactBlockDriverProgress, ParentSelectedExactBlockAbortRequired> {
        self.state = ParentSelectedExactBlockDriverState::AbortRequired;
        Err(ParentSelectedExactBlockAbortRequired { stage, error })
    }

    #[must_use]
    pub(crate) const fn acknowledged_lines(&self) -> u64 {
        match &self.state {
            ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Admission {
                    acknowledged_lines, ..
                },
            ) => *acknowledged_lines,
            ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Capturing(capture),
            ) => capture.acknowledged_lines,
            ParentSelectedExactBlockDriverState::Rejoining(
                ParentSelectedExactBlockRejoin::Joined(checkpoint),
            ) => checkpoint.acknowledged_lines,
            ParentSelectedExactBlockDriverState::OldConvergenceStep { checkpoint, .. }
            | ParentSelectedExactBlockDriverState::AdvanceOldConvergence { checkpoint, .. }
            | ParentSelectedExactBlockDriverState::Mapping { checkpoint, .. }
            | ParentSelectedExactBlockDriverState::JoinedConvergence { checkpoint, .. } => {
                checkpoint.acknowledged_lines
            }
            ParentSelectedExactBlockDriverState::RunningToConvergence { job, .. }
            | ParentSelectedExactBlockDriverState::RunningFullSuffix { job }
            | ParentSelectedExactBlockDriverState::FullSuffixReplacementRequired { job } => {
                job.acknowledged_lines
            }
            ParentSelectedExactBlockDriverState::CapturingConvergence { capture, .. } => {
                capture.acknowledged_lines
            }
            ParentSelectedExactBlockDriverState::ConvergenceJoinRequired {
                acknowledged_lines,
                ..
            } => *acknowledged_lines,
            ParentSelectedExactBlockDriverState::AbortRequired => 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn old_convergence_for_test(
        &self,
    ) -> (
        Option<RelativeCheckpointMeasure>,
        Option<u64>,
        Option<&'static str>,
    ) {
        match &self.state {
            ParentSelectedExactBlockDriverState::OldConvergenceStep { step, .. } => (
                step.checkpoint_cut_for_test(),
                step.checkpoint_ordinal_for_test(),
                step.boundary_kind_for_test(),
            ),
            ParentSelectedExactBlockDriverState::AdvanceOldConvergence { current, .. }
            | ParentSelectedExactBlockDriverState::ConvergenceJoinRequired {
                old_convergence: current,
                ..
            } => (
                Some(current.checkpoint_cut_for_test()),
                Some(current.ordinal_for_test()),
                None,
            ),
            ParentSelectedExactBlockDriverState::Mapping { mapping, .. } => mapping
                .old_convergence_for_test()
                .map_or((None, None, None), |(cut, ordinal)| {
                    (Some(cut), Some(ordinal), None)
                }),
            ParentSelectedExactBlockDriverState::RunningToConvergence { mapped, .. }
            | ParentSelectedExactBlockDriverState::CapturingConvergence { mapped, .. }
            | ParentSelectedExactBlockDriverState::JoinedConvergence { mapped, .. } => {
                let (cut, ordinal) = mapped.old_convergence_for_test();
                (Some(cut), Some(ordinal), None)
            }
            _ => (None, None, None),
        }
    }

    #[cfg(test)]
    pub(crate) fn mapped_convergence_for_test(
        &self,
    ) -> Option<(
        RelativeCheckpointMeasure,
        crate::parent_selected_convergence::ParentSelectedConvergenceMapReceipt,
    )> {
        let mapped = match &self.state {
            ParentSelectedExactBlockDriverState::RunningToConvergence { mapped, .. }
            | ParentSelectedExactBlockDriverState::CapturingConvergence { mapped, .. }
            | ParentSelectedExactBlockDriverState::JoinedConvergence { mapped, .. } => mapped,
            _ => return None,
        };
        Some((mapped.target_for_test(), mapped.receipt_for_test()))
    }

    #[cfg(test)]
    pub(crate) fn matched_live_sample_certificate_for_test(
        &self,
    ) -> Option<(
        LiveCandidateEpoch,
        RelativeCheckpointMeasure,
        RelativeCheckpointMeasure,
        u64,
    )> {
        let ParentSelectedExactBlockDriverState::ConvergenceJoinRequired { certificate, .. } =
            &self.state
        else {
            return None;
        };
        Some(certificate.receipt_for_test())
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
}

fn tail_adoption_is_ineligible(error: &CandidateWriterError) -> bool {
    matches!(
        error,
        CandidateWriterError::SourceLedger(crate::SourceBoundLedgerError::TailAdoptionMismatch)
            | CandidateWriterError::Projection(
                crate::SourceProjectionComposerError::TailAdoptionMismatch
            )
    )
}

impl ExactBlockJob {
    /// Installs the actor-minted chain for the final v2 commit. The first
    /// prototype expects this after the EOF line-boundary checkpoint; the
    /// chain is intentionally not smuggled through an unrelated later pause.
    pub(crate) fn install_restart_sample_chain(
        &mut self,
        samples: RestartCheckpointSampleChain,
    ) -> Result<(), Box<RestartCheckpointSampleChain>> {
        if self.restart_samples.is_some()
            || matches!(self.mode, DriverMode::Complete | DriverMode::Failed)
        {
            return Err(Box::new(samples));
        }
        self.restart_samples = Some(samples);
        Ok(())
    }

    /// Converts a fully actor-installed retained writer/donor activation
    /// directly into the existing same-build checkpoint capture. No suffix
    /// recognition can run until that independent parser/source/composer/
    /// green/binding join reaches Ready and is resumed.
    pub(crate) fn start_retained_setext_rejoin(
        epoch: LiveCandidateEpoch,
        activation: RetainedSetextDriverActivation,
        document: &mut LiveDocumentStore,
    ) -> Result<Box<ExactBlockCheckpointCapture>, Box<ExactBlockCheckpointStartFailure>> {
        let (parser, bindings, acknowledged_lines) = activation.into_parts();
        Self::start_retained_rejoin_parts(epoch, parser, bindings, acknowledged_lines, document)
    }

    fn start_retained_rejoin_parts(
        epoch: LiveCandidateEpoch,
        parser: DirectValueBlockParser,
        bindings: Vec<CandidateWriterBinding>,
        acknowledged_lines: u64,
        document: &mut LiveDocumentStore,
    ) -> Result<Box<ExactBlockCheckpointCapture>, Box<ExactBlockCheckpointStartFailure>> {
        let job = Self {
            epoch,
            parser: Some(parser),
            mode: DriverMode::Recognizing,
            bindings,
            line_input: ExactLineInput::Buffered,
            source_line_phase: None,
            source_line_metrics: ExactSourceLineMetrics::default(),
            recognition_line: String::new(),
            recognition_saw_atom: false,
            current_line: None,
            active_command: None,
            source_work: None,
            writer_work: None,
            restart_samples: None,
            built: None,
            failed_commit_abort: None,
            maximum_line_bytes: 0,
            acknowledged_lines,
            checkpoint_eligible: true,
        };
        match job.start_line_boundary_checkpoint(document)? {
            ExactBlockCheckpointAdmission::Started(capture) => Ok(capture),
            ExactBlockCheckpointAdmission::Skipped { job, .. } => {
                Err(Box::new(ExactBlockCheckpointStartFailure {
                    error: ExactBlockJobError::Invariant(
                        "parent-selected retained activation did not enter mandatory fresh rejoin",
                    ),
                    job: *job,
                }))
            }
        }
    }

    pub(crate) fn new(epoch: LiveCandidateEpoch) -> Result<Self, ExactBlockJobError> {
        Self::new_with_line_input(epoch, ExactLineInput::Buffered)
    }

    /// Dedicated vertical proof path for donor-owned source-backed ATX lines.
    ///
    /// V3 never classifies the line or computes ATX cuts. Every non-empty
    /// physical line is admitted to the donor over the actor byte session;
    /// donor `NoMatch` is terminal for this unpublished candidate and cannot
    /// fall back to buffered recognition.
    pub(crate) fn new_source_backed_atx(
        epoch: LiveCandidateEpoch,
    ) -> Result<Self, ExactBlockJobError> {
        Self::new_with_line_input(epoch, ExactLineInput::source_backed_atx())
    }

    fn new_with_line_input(
        epoch: LiveCandidateEpoch,
        line_input: ExactLineInput,
    ) -> Result<Self, ExactBlockJobError> {
        Ok(Self {
            epoch,
            parser: Some(DirectValueBlockParser::new(SyntaxProfile::CommonMark)?),
            mode: DriverMode::Initial,
            bindings: Vec::new(),
            line_input,
            source_line_phase: None,
            source_line_metrics: ExactSourceLineMetrics::default(),
            recognition_line: String::new(),
            recognition_saw_atom: false,
            current_line: None,
            active_command: None,
            source_work: None,
            writer_work: None,
            restart_samples: None,
            built: None,
            failed_commit_abort: None,
            maximum_line_bytes: 0,
            acknowledged_lines: 0,
            checkpoint_eligible: false,
        })
    }

    /// Attempts the optional same-build checkpoint only at the exact seam
    /// following an acknowledged physical line. A scheduling miss returns the
    /// untouched job; it is never promoted into a parse failure.
    pub(crate) fn start_line_boundary_checkpoint(
        self,
        document: &mut LiveDocumentStore,
    ) -> Result<ExactBlockCheckpointAdmission, Box<ExactBlockCheckpointStartFailure>> {
        self.start_line_boundary_checkpoint_inner(document, false)
    }

    fn start_convergence_line_boundary_checkpoint(
        self,
        document: &mut LiveDocumentStore,
    ) -> Result<ExactBlockCheckpointAdmission, Box<ExactBlockCheckpointStartFailure>> {
        self.start_line_boundary_checkpoint_inner(document, true)
    }

    fn start_line_boundary_checkpoint_inner(
        mut self,
        document: &mut LiveDocumentStore,
        capture_green_prefix_snapshot: bool,
    ) -> Result<ExactBlockCheckpointAdmission, Box<ExactBlockCheckpointStartFailure>> {
        if !self.is_line_boundary_checkpoint_seam() {
            return Ok(ExactBlockCheckpointAdmission::Skipped {
                job: Box::new(self),
                reason: ExactBlockCheckpointSkip::ParserNotAtBoundary,
            });
        }
        // This boundary now has its one scheduling decision. A skip continues
        // ordinary recognition; a successful resume must not recapture the
        // same cut indefinitely.
        self.checkpoint_eligible = false;

        if let Err(error) = self
            .parser
            .as_ref()
            .expect("checkpoint seam requires the one live direct parser")
            .capture_line_boundary_pause()
        {
            return Err(Box::new(ExactBlockCheckpointStartFailure {
                error: ExactBlockJobError::Parser(error),
                job: self,
            }));
        }

        let writer_admission = if capture_green_prefix_snapshot {
            document.candidate_writer_start_convergence_line_boundary_checkpoint(self.epoch)
        } else {
            document.candidate_writer_start_line_boundary_checkpoint(self.epoch)
        };
        match writer_admission {
            Ok(CandidateLineBoundaryCheckpointAdmission::Skipped(reason)) => {
                Ok(ExactBlockCheckpointAdmission::Skipped {
                    job: Box::new(self),
                    reason: ExactBlockCheckpointSkip::Writer(reason),
                })
            }
            Err(error) => Err(Box::new(ExactBlockCheckpointStartFailure {
                error: ExactBlockJobError::Writer(error),
                job: self,
            })),
            Ok(CandidateLineBoundaryCheckpointAdmission::Started) => {
                let parser = self
                    .parser
                    .take()
                    .expect("checkpoint seam requires the one live direct parser");
                let parser =
                    match ParserLineBoundaryCheckpointAuthority::capture(self.epoch, parser) {
                        Ok(parser) => parser,
                        Err(failure) => {
                            self.parser = Some(failure.parser);
                            self.mode = DriverMode::Failed;
                            return Err(Box::new(ExactBlockCheckpointStartFailure {
                                error: ExactBlockJobError::Parser(failure.error),
                                job: self,
                            }));
                        }
                    };
                Ok(ExactBlockCheckpointAdmission::Started(Box::new(
                    ExactBlockCheckpointCapture {
                        epoch: self.epoch,
                        parser: Some(parser),
                        bindings: self.bindings,
                        line_input: self.line_input,
                        source_line_metrics: self.source_line_metrics,
                        maximum_line_bytes: self.maximum_line_bytes,
                        acknowledged_lines: self.acknowledged_lines,
                    },
                )))
            }
        }
    }

    pub(crate) fn is_line_boundary_checkpoint_seam(&self) -> bool {
        matches!(self.mode, DriverMode::Recognizing)
            && self.acknowledged_lines > 0
            && self.checkpoint_eligible
            && self.parser.is_some()
            && self
                .parser
                .as_ref()
                .is_some_and(|parser| parser.pending_command().is_none())
            && self.recognition_line.is_empty()
            && !self.recognition_saw_atom
            && self.source_line_phase.is_none()
            && self.current_line.is_none()
            && self.active_command.is_none()
            && self.source_work.is_none()
            && self.writer_work.is_none()
            && self.restart_samples.is_none()
            && self.built.is_none()
    }

    /// Drives the ordinary exact suffix while reserving its final completion
    /// for the parent-selected checkpoint-index splice. The guard runs before
    /// `poll`, so `WriterWork::Finish` can never enter either independent
    /// commit branch in `poll_writer`.
    fn poll_parent_selected_suffix(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<ParentSelectedExactBlockRunningProgress, ExactBlockJobError> {
        if matches!(&self.writer_work, Some(WriterWork::Finish)) {
            let result = match document.poll_candidate_writer(self.epoch) {
                Ok(CandidateWriterProgress::Pending) => {
                    Ok(ParentSelectedExactBlockRunningProgress::Pending)
                }
                Ok(CandidateWriterProgress::CompletionReady) => {
                    // Source/composer/green completion is now sealed but
                    // uncommitted. Keep the parser's Finish command and the
                    // writer work marker intact for the future atomic splice.
                    Ok(ParentSelectedExactBlockRunningProgress::CheckpointIndexSpliceRequired)
                }
                Ok(_) => Err(ExactBlockJobError::Invariant(
                    "parent-selected finish returned unrelated writer progress",
                )),
                Err(error) => Err(ExactBlockJobError::Writer(error)),
            };
            if result.is_err() {
                self.mode = DriverMode::Failed;
            }
            return result;
        }

        match self.poll(document)? {
            ExactBlockJobProgress::Pending => Ok(ParentSelectedExactBlockRunningProgress::Pending),
            ExactBlockJobProgress::Complete => Err(ExactBlockJobError::Invariant(
                "parent-selected exact driver entered an independent commit path",
            )),
        }
    }

    /// Advances one bounded exact-parser/writer transition toward opaque live
    /// C. Comparison is possible only at a quiescent physical-line seam; the
    /// job never receives or stores C's byte offset. Reaching EOF or crossing
    /// the target is a safe full-suffix fallback, not reusable-tail proof.
    fn poll_to_parent_selected_target(
        mut self: Box<Self>,
        document: &mut LiveDocumentStore,
        mapped: &ParentSelectedMappedConvergence,
    ) -> Result<ParentSelectedTargetPoll, ExactBlockJobError> {
        if self.is_line_boundary_checkpoint_seam() {
            match document
                .candidate_writer_parent_selected_convergence_relation(self.epoch, mapped)?
            {
                ParentSelectedConvergenceTargetRelation::Before => {}
                ParentSelectedConvergenceTargetRelation::At => {
                    return match (*self).start_convergence_line_boundary_checkpoint(document) {
                        Ok(ExactBlockCheckpointAdmission::Started(capture)) => {
                            Ok(ParentSelectedTargetPoll::Capturing(capture))
                        }
                        Ok(ExactBlockCheckpointAdmission::Skipped { job, reason: _ }) => {
                            Ok(ParentSelectedTargetPoll::FullSuffix(job))
                        }
                        Err(failure) => Err(failure.error),
                    };
                }
                ParentSelectedConvergenceTargetRelation::Past => {
                    return Ok(ParentSelectedTargetPoll::FullSuffix(self));
                }
            }
        }

        match self.poll_parent_selected_suffix(document)? {
            ParentSelectedExactBlockRunningProgress::Pending => {
                Ok(ParentSelectedTargetPoll::Pending(self))
            }
            ParentSelectedExactBlockRunningProgress::CheckpointIndexSpliceRequired => {
                Ok(ParentSelectedTargetPoll::FullSuffix(self))
            }
        }
    }

    pub(crate) fn poll(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<ExactBlockJobProgress, ExactBlockJobError> {
        if matches!(self.mode, DriverMode::Failed) {
            return Err(ExactBlockJobError::Failed);
        }
        if matches!(self.mode, DriverMode::Complete) {
            return Ok(ExactBlockJobProgress::Complete);
        }
        let result = self.poll_inner(document);
        if result.is_err() {
            self.mode = DriverMode::Failed;
        }
        result
    }

    /// Transfers a failed or superseded parse into the actor's existing
    /// constant-time abort admission. Arena-owner reclamation remains fuelled
    /// through `LiveDocumentStore::poll_candidate_abort`.
    pub(crate) fn cancel(
        self,
        document: &mut LiveDocumentStore,
    ) -> Result<CandidateAbort, LiveDocumentError> {
        match self.failed_commit_abort {
            Some(abort) => Ok(abort),
            None => document.cancel_candidate(self.epoch),
        }
    }

    fn poll_inner(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<ExactBlockJobProgress, ExactBlockJobError> {
        if let Some(work) = self.writer_work.take() {
            self.poll_writer(document, work)?;
            return Ok(self.progress());
        }
        if let Some(work) = self.source_work.take() {
            self.poll_source(document, work)?;
            return Ok(self.progress());
        }
        if self.active_command.is_some() {
            return Err(ExactBlockJobError::Invariant(
                "active parser command has no bounded work",
            ));
        }
        if let Some(command) = self.parser()?.pending_command().cloned() {
            self.active_command = Some(command.clone());
            self.start_command(document, command)?;
            return Ok(self.progress());
        }

        match self.mode {
            DriverMode::Initial => Err(ExactBlockJobError::Invariant(
                "direct parser did not expose its root command",
            )),
            DriverMode::Recognizing => {
                self.checkpoint_eligible = false;
                match self.line_input {
                    ExactLineInput::Buffered => self.poll_recognition(document)?,
                    ExactLineInput::SourceBackedAtx { donor_fuel } => {
                        self.poll_source_backed_atx(document, donor_fuel)?;
                    }
                }
                Ok(self.progress())
            }
            DriverMode::PollLine => {
                let receipt = self.parser_mut()?.poll_line(1)?;
                if receipt.transitions > 1 {
                    return Err(ExactBlockJobError::Invariant(
                        "direct parser exceeded fuel-one transition",
                    ));
                }
                match receipt.status {
                    DirectPollStatus::Pending | DirectPollStatus::CommandReady => {}
                    DirectPollStatus::ExternalWorkReady => {
                        self.start_reference_prefix(document)?;
                    }
                    DirectPollStatus::Complete => {
                        if self.current_line.is_some() {
                            return Err(ExactBlockJobError::Invariant(
                                "parser completed line before writer receipt",
                            ));
                        }
                        self.mode = DriverMode::Recognizing;
                        self.checkpoint_eligible = true;
                    }
                }
                Ok(self.progress())
            }
            DriverMode::PollFinish => {
                let receipt = self.parser_mut()?.poll_finish(1)?;
                if receipt.transitions > 1 {
                    return Err(ExactBlockJobError::Invariant(
                        "direct parser exceeded fuel-one finish transition",
                    ));
                }
                if receipt.status == DirectPollStatus::Complete {
                    return Err(ExactBlockJobError::Invariant(
                        "parser escaped before the joined finish command",
                    ));
                }
                if receipt.status == DirectPollStatus::ExternalWorkReady {
                    self.start_reference_prefix(document)?;
                }
                Ok(self.progress())
            }
            DriverMode::Complete => Ok(ExactBlockJobProgress::Complete),
            DriverMode::Failed => Err(ExactBlockJobError::Failed),
        }
    }

    fn poll_recognition(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<(), ExactBlockJobError> {
        match document.poll_candidate_writer_recognition(self.epoch, 1)? {
            CandidateRecognitionPoll::NeedFuel(_) => Ok(()),
            CandidateRecognitionPoll::Atom { atom, .. } => {
                self.recognition_saw_atom = true;
                let ended = self.push_recognition_atom(atom.kind())?;
                if ended {
                    self.begin_recognized_line(document)?;
                }
                Ok(())
            }
            CandidateRecognitionPoll::Eof(_) => {
                if self.recognition_saw_atom {
                    self.begin_recognized_line(document)
                } else {
                    self.parser_mut()?.begin_finish()?;
                    self.mode = DriverMode::PollFinish;
                    Ok(())
                }
            }
        }
    }

    fn poll_source_backed_atx(
        &mut self,
        document: &mut LiveDocumentStore,
        donor_fuel: usize,
    ) -> Result<(), ExactBlockJobError> {
        if donor_fuel == 0 || donor_fuel > EXACT_SOURCE_LINE_MAX_DONOR_FUEL {
            return Err(ExactBlockJobError::Invariant(
                "source-line donor fuel is inside the composed poll bound",
            ));
        }
        let actor_fuel = CANDIDATE_RECOGNITION_BYTE_POLL_MAX_ACCESSES;
        if !self.recognition_line.is_empty() || self.recognition_saw_atom {
            return Err(ExactBlockJobError::Invariant(
                "source-backed line path retains no buffered recognition text",
            ));
        }

        match self.source_line_phase.take() {
            None => {
                if document.candidate_writer_recognition_at_physical_eof(self.epoch)? {
                    self.parser_mut()?.begin_finish()?;
                    self.mode = DriverMode::PollFinish;
                    self.record_source_line_composed_work(1)?;
                    return Ok(());
                }
                let descriptor =
                    document.candidate_writer_recognition_line_descriptor(self.epoch)?;
                let session = document
                    .candidate_writer_begin_recognition_byte_session(self.epoch, descriptor)?;
                self.maximum_line_bytes = self.maximum_line_bytes.max(session.len());
                self.source_line_phase = Some(ExactSourceLinePhase::SessionOpen { session });
                self.record_source_line_composed_work(1)
            }
            Some(ExactSourceLinePhase::SessionOpen { session }) => {
                let work = self
                    .parser_mut()?
                    .begin_source_line_work(session, session.len())?;
                self.source_line_phase = Some(ExactSourceLinePhase::Polling { session, work });
                self.record_source_line_composed_work(1)
            }
            Some(ExactSourceLinePhase::Polling { session, mut work }) => {
                let mut donor_receipt = None;
                let mut scanner = |source: &mut CandidateRecognitionByteSource<'_>| {
                    let mut adapter = ExactDirectSourceLineAdapter { source };
                    let receipt = work
                        .poll_source(&mut adapter, donor_fuel)
                        .map_err(ExactSourceLineScannerError::Donor)?;
                    donor_receipt = Some(receipt);
                    Ok(())
                };
                let actor_receipt = match document.poll_candidate_writer_recognition_byte_session(
                    self.epoch,
                    session,
                    actor_fuel,
                    &mut scanner,
                ) {
                    Ok(receipt) => receipt,
                    Err(CandidateRecognitionBytePollError::Infrastructure(error)) => {
                        return Err(ExactBlockJobError::Writer(error));
                    }
                    Err(CandidateRecognitionBytePollError::Scanner(
                        ExactSourceLineScannerError::Donor(error),
                    )) => return Err(ExactBlockJobError::SourceLineDonor(error)),
                };
                let donor_receipt = donor_receipt.ok_or(ExactBlockJobError::Invariant(
                    "source-line adapter returned one donor receipt",
                ))?;
                self.validate_and_record_source_line_poll(
                    session,
                    actor_receipt,
                    donor_receipt,
                    work.retained_source_bytes(),
                    actor_fuel,
                )?;
                self.source_line_phase = Some(match donor_receipt.status {
                    DirectSourceLinePollStatus::NeedMore => {
                        ExactSourceLinePhase::Polling { session, work }
                    }
                    DirectSourceLinePollStatus::Matched => {
                        ExactSourceLinePhase::Matched { session, work }
                    }
                });
                Ok(())
            }
            Some(ExactSourceLinePhase::Matched { session, work }) => {
                let receipt = document
                    .candidate_writer_finish_recognition_byte_session(self.epoch, session)?;
                self.validate_source_line_finish(session, receipt)?;
                self.source_line_phase =
                    Some(ExactSourceLinePhase::ActorFinished { work, receipt });
                self.record_source_line_composed_work(1)
            }
            Some(ExactSourceLinePhase::ActorFinished { work, receipt }) => {
                let session = receipt.session();
                let recognition = receipt.line();
                let physical_bytes = u32::try_from(session.len()).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line physical bytes fit u32")
                })?;
                let physical_utf16 = u32::try_from(session.physical_utf16()).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line physical UTF-16 fits u32")
                })?;
                let content_end = u32::try_from(
                    session.content_end().checked_sub(session.start()).ok_or(
                        ExactBlockJobError::Invariant(
                            "source-line content starts inside its physical line",
                        ),
                    )?,
                )
                .map_err(|_| ExactBlockJobError::Invariant("source-line content bytes fit u32"))?;
                self.parser_mut()?
                    .commit_source_line(work, session, physical_utf16)?;
                self.current_line = Some(CurrentLine {
                    recognition,
                    physical_bytes,
                    physical_utf16,
                    content_end,
                    authoritative_offset: 0,
                });
                self.source_line_metrics.lines_committed = self
                    .source_line_metrics
                    .lines_committed
                    .checked_add(1)
                    .ok_or(ExactBlockJobError::Invariant(
                        "source-line committed-line count overflow",
                    ))?;
                self.mode = DriverMode::PollLine;
                self.record_source_line_composed_work(1)
            }
        }
    }

    fn validate_and_record_source_line_poll(
        &mut self,
        session: CandidateRecognitionByteSession,
        actor: crate::CandidateRecognitionBytePollReceipt,
        donor: DirectSourceLinePollReceipt,
        retained_source_bytes: usize,
        actor_fuel: usize,
    ) -> Result<(), ExactBlockJobError> {
        let expected_physical_high_water = session
            .start()
            .checked_add(donor.physical_high_water)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(ExactBlockJobError::Invariant(
                "source-line physical high-water fits u64",
            ))?;
        let composed_work = actor
            .access_work_units()
            .checked_add(donor.lexical_work_units)
            .ok_or(ExactBlockJobError::Invariant(
                "source-line composed work overflow",
            ))?;
        let invalid = actor.session() != session
            || actor.access_work_units() > actor_fuel
            || donor.lexical_work_units > EXACT_SOURCE_LINE_MAX_DONOR_LEXICAL_WORK
            || composed_work > EXACT_SOURCE_LINE_MAX_COMPOSED_WORK_PER_POLL
            || donor.physical_high_water != actor.exposed_high_water().1
            || actor.physical_high_water() != expected_physical_high_water
            || donor.source_first_reads != actor.new_bytes()
            || actor.access_work_units() != actor.new_bytes()
            || actor.repeated_last_byte_peeks() != 0
            || donor.maximum_source_request_rewind_bytes != 0
            || retained_source_bytes != donor.retained_source_bytes
            || retained_source_bytes > DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES
            || actor.maximum_retained_byte_scratch() > 1;
        if invalid {
            return Err(ExactBlockJobError::Invariant(
                "source-line actor/donor poll violated identity or bounded-work contract",
            ));
        }

        let metrics = &mut self.source_line_metrics;
        metrics.donor_polls =
            metrics
                .donor_polls
                .checked_add(1)
                .ok_or(ExactBlockJobError::Invariant(
                    "source-line donor poll count overflow",
                ))?;
        metrics.donor_lexical_work_units =
            metrics
                .donor_lexical_work_units
                .checked_add(u64::try_from(donor.lexical_work_units).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line lexical work fits u64")
                })?)
                .ok_or(ExactBlockJobError::Invariant(
                    "source-line lexical work overflow",
                ))?;
        metrics.source_first_reads =
            metrics
                .source_first_reads
                .checked_add(u64::try_from(donor.source_first_reads).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line first reads fit u64")
                })?)
                .ok_or(ExactBlockJobError::Invariant(
                    "source-line first reads overflow",
                ))?;
        metrics.actor_access_work_units =
            metrics
                .actor_access_work_units
                .checked_add(u64::try_from(actor.access_work_units()).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line actor work fits u64")
                })?)
                .ok_or(ExactBlockJobError::Invariant(
                    "source-line actor work overflow",
                ))?;
        metrics.actor_new_bytes =
            metrics
                .actor_new_bytes
                .checked_add(u64::try_from(actor.new_bytes()).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line actor bytes fit u64")
                })?)
                .ok_or(ExactBlockJobError::Invariant(
                    "source-line actor bytes overflow",
                ))?;
        metrics.actor_repeated_last_byte_peeks = metrics
            .actor_repeated_last_byte_peeks
            .checked_add(
                u64::try_from(actor.repeated_last_byte_peeks()).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line actor repeats fit u64")
                })?,
            )
            .ok_or(ExactBlockJobError::Invariant(
                "source-line actor repeats overflow",
            ))?;
        metrics.maximum_donor_retained_source_bytes = metrics
            .maximum_donor_retained_source_bytes
            .max(retained_source_bytes);
        metrics.maximum_actor_retained_byte_scratch = metrics
            .maximum_actor_retained_byte_scratch
            .max(actor.maximum_retained_byte_scratch());
        metrics.maximum_source_request_rewind_bytes = metrics
            .maximum_source_request_rewind_bytes
            .max(donor.maximum_source_request_rewind_bytes);
        metrics.maximum_composed_work_per_poll =
            metrics.maximum_composed_work_per_poll.max(composed_work);
        Ok(())
    }

    fn validate_source_line_finish(
        &self,
        session: CandidateRecognitionByteSession,
        receipt: CandidateRecognitionByteSessionFinishReceipt,
    ) -> Result<(), ExactBlockJobError> {
        let recognition = receipt.line();
        let expected_ending = match session.ending() {
            SourcePhysicalLineEnding::Lf => Some(CandidateLineEnding::Lf),
            SourcePhysicalLineEnding::LoneCr => Some(CandidateLineEnding::LoneCr),
            SourcePhysicalLineEnding::CrLf => Some(CandidateLineEnding::CrLf),
            SourcePhysicalLineEnding::BareEof => None,
        };
        if receipt.session() != session
            || session.epoch() != self.epoch
            || session.source() != self.epoch.source()
            || session.build_id() != self.epoch.build_id()
            || recognition.source() != session.source()
            || recognition.build_id() != session.build_id()
            || recognition.line_ordinal() != session.line_ordinal()
            || recognition.absolute_range()
                != (
                    u64::try_from(session.start())
                        .map_err(|_| ExactBlockJobError::Invariant("source-line start fits u64"))?,
                    u64::try_from(session.end())
                        .map_err(|_| ExactBlockJobError::Invariant("source-line end fits u64"))?,
                )
            || recognition.metric().bytes()
                != u64::try_from(session.len())
                    .map_err(|_| ExactBlockJobError::Invariant("source-line bytes fit u64"))?
            || recognition.metric().utf16()
                != u64::try_from(session.physical_utf16())
                    .map_err(|_| ExactBlockJobError::Invariant("source-line UTF-16 fits u64"))?
            || recognition.ending() != expected_ending
            || receipt.new_bytes()
                != u64::try_from(session.len()).map_err(|_| {
                    ExactBlockJobError::Invariant("source-line exposed bytes fit u64")
                })?
            || receipt.total_access_work_units() != receipt.new_bytes()
            || receipt.repeated_last_byte_peeks() != 0
            || receipt.physical_high_water()
                != u64::try_from(session.end())
                    .map_err(|_| ExactBlockJobError::Invariant("source-line high-water fits u64"))?
            || receipt.maximum_retained_byte_scratch() > 1
        {
            return Err(ExactBlockJobError::Invariant(
                "source-line finish receipt disagrees with actor session authority",
            ));
        }
        Ok(())
    }

    fn record_source_line_composed_work(
        &mut self,
        work_units: usize,
    ) -> Result<(), ExactBlockJobError> {
        if work_units > EXACT_SOURCE_LINE_MAX_COMPOSED_WORK_PER_POLL {
            return Err(ExactBlockJobError::Invariant(
                "source-line phase exceeded composed poll work bound",
            ));
        }
        self.source_line_metrics.maximum_composed_work_per_poll = self
            .source_line_metrics
            .maximum_composed_work_per_poll
            .max(work_units);
        Ok(())
    }

    fn push_recognition_atom(
        &mut self,
        kind: CandidateSourceAtomKind,
    ) -> Result<bool, ExactBlockJobError> {
        let additional = source_atom_bytes(kind) as usize;
        if self.recognition_line.len().saturating_add(additional)
            > DirectValueBlockParser::MAX_LINE_BYTES
        {
            return Err(ParseError::DirectUnsupported(DirectUnsupported::LineTooLarge).into());
        }
        match kind {
            CandidateSourceAtomKind::Scalar(value) => self.recognition_line.push(value),
            CandidateSourceAtomKind::Tab => self.recognition_line.push('\t'),
            CandidateSourceAtomKind::Nul => self.recognition_line.push('\0'),
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf) => {
                self.recognition_line.push('\n');
            }
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr) => {
                self.recognition_line.push('\r');
            }
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf) => {
                self.recognition_line.push_str("\r\n");
            }
        }
        self.maximum_line_bytes = self.maximum_line_bytes.max(self.recognition_line.len());
        Ok(matches!(kind, CandidateSourceAtomKind::LineEnding(_)))
    }

    fn begin_recognized_line(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<(), ExactBlockJobError> {
        if !self.recognition_saw_atom {
            return Err(ExactBlockJobError::Invariant(
                "cannot begin a phantom physical line",
            ));
        }
        let recognition = document.candidate_writer_finish_recognition_line(self.epoch)?;
        let physical_bytes = u32::try_from(self.recognition_line.len())
            .map_err(|_| ExactBlockJobError::Invariant("proof line bytes fit u32"))?;
        let physical_utf16 = u32::try_from(self.recognition_line.encode_utf16().count())
            .map_err(|_| ExactBlockJobError::Invariant("proof line UTF-16 fits u32"))?;
        if recognition.metric().bytes() != u64::from(physical_bytes)
            || recognition.metric().utf16() != u64::from(physical_utf16)
        {
            return Err(ExactBlockJobError::Invariant(
                "recognition text metric disagrees with source authority",
            ));
        }
        let ending_bytes = recognition.ending().map_or(0, candidate_ending_bytes);
        let content_end = physical_bytes
            .checked_sub(ending_bytes)
            .ok_or(ExactBlockJobError::Invariant("line ending exceeds line"))?;
        let line = std::mem::take(&mut self.recognition_line);
        self.parser_mut()?.begin_line(line)?;
        self.current_line = Some(CurrentLine {
            recognition,
            physical_bytes,
            physical_utf16,
            content_end,
            authoritative_offset: 0,
        });
        self.recognition_saw_atom = false;
        self.mode = DriverMode::PollLine;
        Ok(())
    }

    fn start_command(
        &mut self,
        document: &mut LiveDocumentStore,
        command: DirectCommand,
    ) -> Result<(), ExactBlockJobError> {
        match command {
            DirectCommand::Open { kind } => {
                self.start_open_command(document, kind)?;
            }
            DirectCommand::Consume {
                owner,
                part,
                range,
                logical,
            } => self.start_consume_command(document, owner, part, range, logical)?,
            DirectCommand::StageTerminator { range, ending } => {
                self.stage_terminator_command(document, range, ending)?;
            }
            DirectCommand::ResolveTerminator { resolution } => {
                self.start_resolve_terminator_command(document, resolution)?;
            }
            DirectCommand::StageBlankGap { range } => {
                let line = self.current_line()?;
                if range.start != line.authoritative_offset || range.end != line.physical_bytes {
                    return Err(ExactBlockJobError::Invariant(
                        "blank range is not the authoritative line remainder",
                    ));
                }
                self.source_work = Some(SourceWork::StageBlankRange { end: range.end });
            }
            DirectCommand::ResolveBlankGap { owner } => {
                let owner_index = self.binding_index(owner)?;
                document.candidate_writer_start_resolve_blank_gap(
                    self.epoch,
                    &self.bindings[owner_index],
                )?;
                self.writer_work = Some(WriterWork::Resolve);
            }
            DirectCommand::FinalizeParagraph { outcome } => {
                self.start_finalize_paragraph_command(document, outcome)?;
            }
            DirectCommand::MarkFencedCodeBoundary { boundary } => {
                self.mark_fenced_code_boundary_command(document, boundary)?;
            }
            DirectCommand::Close {
                kind,
                final_facts,
                last_line_blank,
                child,
            } => {
                self.start_close_command(document, kind, final_facts, last_line_blank, child)?;
            }
            DirectCommand::FinishLine {
                physical_bytes,
                physical_utf16,
            } => self.finish_line_command(document, physical_bytes, physical_utf16)?,
            DirectCommand::FinishDocument => {
                if !self.bindings.is_empty() || self.current_line.is_some() {
                    return Err(ExactBlockJobError::Invariant(
                        "document finish has outstanding parser/writer state",
                    ));
                }
                self.source_work = Some(SourceWork::EnsureEof);
            }
        }
        Ok(())
    }

    fn start_resolve_terminator_command(
        &mut self,
        document: &mut LiveDocumentStore,
        resolution: DirectTerminatorResolution,
    ) -> Result<(), ExactBlockJobError> {
        let resolution = match resolution {
            DirectTerminatorResolution::ContinueCanonicalNewline => {
                CandidateTerminatorResolution::ContinueCanonicalNewline
            }
            DirectTerminatorResolution::CloseNone => CandidateTerminatorResolution::CloseNone,
        };
        document.candidate_writer_start_resolve_terminator(self.epoch, resolution)?;
        self.writer_work = Some(WriterWork::Resolve);
        Ok(())
    }

    fn start_finalize_paragraph_command(
        &mut self,
        document: &mut LiveDocumentStore,
        outcome: DirectParagraphOutcome,
    ) -> Result<(), ExactBlockJobError> {
        let binding = self.bindings.pop().ok_or(ExactBlockJobError::Invariant(
            "Paragraph finalization has an open binding",
        ))?;
        if binding.kind() != GreenKind::PARAGRAPH {
            return Err(ExactBlockJobError::Invariant(
                "Paragraph finalization targets the terminal Paragraph",
            ));
        }
        let facts = match outcome {
            DirectParagraphOutcome::SetextHeading { level } => {
                GreenHeadingOpenFacts::setext(level).map_err(serialized_fact_error)?
            }
        };
        let expected_identity = binding.identity(self.epoch);
        document.candidate_writer_start_promote_setext(self.epoch, binding, facts)?;
        self.writer_work = Some(WriterWork::FinalizeParagraph {
            expected_identity,
            expected_facts: facts,
        });
        Ok(())
    }

    fn mark_fenced_code_boundary_command(
        &mut self,
        document: &mut LiveDocumentStore,
        boundary: DirectFencedCodeBoundary,
    ) -> Result<(), ExactBlockJobError> {
        let binding = self.bindings.last().ok_or(ExactBlockJobError::Invariant(
            "fenced-code boundary has an open binding",
        ))?;
        if binding.kind() != GreenKind::FENCED_CODE {
            return Err(ExactBlockJobError::Invariant(
                "fenced-code boundary does not target the terminal binding",
            ));
        }
        let boundary = match boundary {
            DirectFencedCodeBoundary::InfoEnd => CandidateFencedCodeBoundary::InfoEnd,
            DirectFencedCodeBoundary::LiteralStart => CandidateFencedCodeBoundary::LiteralStart,
        };
        document.candidate_writer_mark_fenced_code_boundary(self.epoch, binding, boundary)?;
        self.acknowledge_command()
    }

    fn start_open_command(
        &mut self,
        document: &mut LiveDocumentStore,
        kind: DirectBlockKind,
    ) -> Result<(), ExactBlockJobError> {
        let expected = direct_kind(kind);
        match kind {
            DirectBlockKind::List(facts) => document
                .candidate_writer_start_open_list(self.epoch, direct_list_open_facts(facts)?)?,
            DirectBlockKind::Item(facts) => document
                .candidate_writer_start_open_item(self.epoch, direct_item_open_facts(facts)?)?,
            DirectBlockKind::FencedCode(facts) => document
                .candidate_writer_start_open_fenced_code(
                    self.epoch,
                    direct_fenced_code_open_facts(facts)?,
                )?,
            DirectBlockKind::Document
            | DirectBlockKind::BlockQuote
            | DirectBlockKind::Paragraph => document.candidate_writer_start_open(
                self.epoch,
                expected,
                FactsEnvelope::empty(),
            )?,
            DirectBlockKind::Heading(facts) if !facts.setext => document
                .candidate_writer_start_open_heading(
                    self.epoch,
                    GreenHeadingOpenFacts::atx(facts.level).map_err(serialized_fact_error)?,
                )?,
            DirectBlockKind::Heading(_) => {
                return Err(ExactBlockJobError::Invariant(
                    "direct Setext Heading must come from Paragraph normalization",
                ));
            }
        }
        self.writer_work = Some(WriterWork::Open { kind: expected });
        Ok(())
    }

    fn start_close_command(
        &mut self,
        document: &mut LiveDocumentStore,
        kind: DirectBlockKind,
        final_facts: DirectFinalFacts,
        last_line_blank: bool,
        child: DirectClosedChild,
    ) -> Result<(), ExactBlockJobError> {
        let expected = direct_kind(kind);
        let binding = self
            .bindings
            .pop()
            .ok_or(ExactBlockJobError::Invariant("close has an open binding"))?;
        if binding.kind() != expected {
            return Err(ExactBlockJobError::Invariant(
                "parser and writer open stacks disagree",
            ));
        }
        let closed = ClosedChildAggregate {
            ends_blank: child.ends_blank,
            item_loose_if_nonlast: child.item_loose_if_nonlast,
            item_loose_if_last: child.item_loose_if_last,
        };
        if let (GreenKind::FENCED_CODE, DirectFinalFacts::FencedCode(facts)) =
            (expected, final_facts)
        {
            document.candidate_writer_start_close_fenced_code(
                self.epoch,
                binding,
                closed,
                last_line_blank,
                facts.closed,
            )?;
        } else {
            let facts = direct_close_facts(expected, final_facts)?;
            document.candidate_writer_start_close_with_facts(
                self.epoch,
                binding,
                closed,
                last_line_blank,
                facts,
            )?;
        }
        self.writer_work = Some(WriterWork::Close);
        Ok(())
    }

    fn start_consume_command(
        &mut self,
        document: &mut LiveDocumentStore,
        owner: DirectOwner,
        part: DirectCoveragePart,
        range: std::ops::Range<u32>,
        logical: DirectLogicalAction,
    ) -> Result<(), ExactBlockJobError> {
        let owner_index = self.binding_index(owner)?;
        let part = direct_part(part);
        let line = self.current_line()?;
        let maximum_end =
            if part == CoveragePart::TERMINAL || logical == DirectLogicalAction::CanonicalNewline {
                line.physical_bytes
            } else {
                line.content_end
            };
        if range.start != line.authoritative_offset || range.end > maximum_end {
            return Err(ExactBlockJobError::Invariant(
                "direct consume range is not the authoritative prefix",
            ));
        }
        match logical {
            DirectLogicalAction::Identity
            | DirectLogicalAction::HiddenUpstream
            | DirectLogicalAction::CanonicalText
            | DirectLogicalAction::None => {
                if matches!(
                    logical,
                    DirectLogicalAction::Identity
                        | DirectLogicalAction::HiddenUpstream
                        | DirectLogicalAction::CanonicalText
                ) && part != CoveragePart::CONTENT
                {
                    return Err(ExactBlockJobError::Invariant(
                        "visible/hidden range replay belongs to terminal content",
                    ));
                }
                let recipe = match logical {
                    DirectLogicalAction::Identity => CandidateWriterRangeRecipe::Identity,
                    DirectLogicalAction::HiddenUpstream => CandidateWriterRangeRecipe::Hidden {
                        affinity: GreenAffinity::Upstream,
                    },
                    DirectLogicalAction::CanonicalText => CandidateWriterRangeRecipe::CanonicalText,
                    DirectLogicalAction::None => CandidateWriterRangeRecipe::None,
                    DirectLogicalAction::CanonicalNewline | DirectLogicalAction::PartialTab(_) => {
                        unreachable!("outer match narrowed")
                    }
                };
                document.candidate_writer_start_range_replay(
                    self.epoch,
                    &self.bindings[owner_index],
                    part,
                    u64::from(range.end - range.start),
                    recipe,
                )?;
                self.writer_work = Some(WriterWork::RangeReplay {
                    start: range.start,
                    end: range.end,
                });
            }
            DirectLogicalAction::CanonicalNewline => {
                let (content_end, physical_bytes, ending) = {
                    let line = self.current_line()?;
                    (
                        line.content_end,
                        line.physical_bytes,
                        line.recognition.ending(),
                    )
                };
                if self.bindings[owner_index].kind() != GreenKind::FENCED_CODE
                    || part != CoveragePart::CONTENT
                    || range.start != content_end
                    || range.end != physical_bytes
                    || ending.is_none()
                {
                    return Err(ExactBlockJobError::Invariant(
                        "canonical newline must consume the exact physical terminator",
                    ));
                }
                self.source_work = Some(SourceWork::ConsumeRange {
                    end: range.end,
                    owner_index,
                    part,
                    logical,
                });
            }
            DirectLogicalAction::PartialTab(partial) => {
                let _logical_target = self.binding_index(partial.logical_target())?;
                if range.end - range.start != 1 {
                    return Err(ExactBlockJobError::Invariant(
                        "partial Tab command must consume exactly one physical byte",
                    ));
                }
                self.source_work = Some(SourceWork::ConsumeRange {
                    end: range.end,
                    owner_index,
                    part,
                    logical,
                });
            }
        }
        Ok(())
    }

    fn stage_terminator_command(
        &mut self,
        _document: &mut LiveDocumentStore,
        range: std::ops::Range<u32>,
        ending: DirectLineEnding,
    ) -> Result<(), ExactBlockJobError> {
        let line = self.current_line()?;
        if range.start != line.authoritative_offset
            || range.end != line.physical_bytes
            || range.start != line.content_end
        {
            return Err(ExactBlockJobError::Invariant(
                "terminator command does not cover the exact line tail",
            ));
        }
        self.source_work = Some(SourceWork::StageTerminator {
            end: range.end,
            ending,
        });
        Ok(())
    }

    fn finish_line_command(
        &mut self,
        document: &mut LiveDocumentStore,
        physical_bytes: u32,
        physical_utf16: u32,
    ) -> Result<(), ExactBlockJobError> {
        let line = self.current_line()?;
        if line.physical_bytes != physical_bytes
            || line.physical_utf16 != physical_utf16
            || line.authoritative_offset != physical_bytes
        {
            return Err(ExactBlockJobError::Invariant(
                "parser and writer physical-line receipts disagree",
            ));
        }
        if line.recognition.ending().is_none() {
            self.source_work = Some(SourceWork::EnsureLineEof {
                physical_bytes,
                physical_utf16,
            });
            return Ok(());
        }
        self.finish_line_after_source_end(document)
    }

    fn finish_line_after_source_end(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<(), ExactBlockJobError> {
        let line = self.current_line()?;
        let receipt = document.candidate_writer_finish_line(self.epoch)?;
        if !receipt.recognition_replay_matched()
            || receipt.absolute_range() != line.recognition.absolute_range()
            || receipt.metric() != line.recognition.metric()
            || receipt.ending() != line.recognition.ending()
        {
            return Err(ExactBlockJobError::Invariant(
                "authoritative replay did not equal recognition",
            ));
        }
        self.acknowledge_command()?;
        self.current_line = None;
        self.acknowledged_lines =
            self.acknowledged_lines
                .checked_add(1)
                .ok_or(ExactBlockJobError::Invariant(
                    "acknowledged physical-line count overflowed",
                ))?;
        Ok(())
    }

    fn poll_source(
        &mut self,
        document: &mut LiveDocumentStore,
        work: SourceWork,
    ) -> Result<(), ExactBlockJobError> {
        match work {
            SourceWork::EnsureEof => self.poll_source_eof(document),
            SourceWork::EnsureLineEof {
                physical_bytes,
                physical_utf16,
            } => self.poll_line_eof(document, physical_bytes, physical_utf16),
            SourceWork::ConsumeRange {
                end,
                owner_index,
                part,
                logical,
            } => self.poll_consume_range(document, end, owner_index, part, logical),
            SourceWork::StageBlankRange { end } => self.poll_blank_range(document, end),
            SourceWork::StageTerminator { end, ending } => {
                self.poll_stage_terminator(document, end, ending)
            }
        }
    }

    fn start_reference_prefix(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<(), ExactBlockJobError> {
        let external = self
            .parser()?
            .pending_external_work()
            .ok_or(ExactBlockJobError::Invariant(
                "external parser status exposes typed work",
            ))?;
        if external.kind() != DirectExternalWorkKind::ReferencePrefixFinalizer {
            return Err(ExactBlockJobError::ReferenceExternalWork(external.kind()));
        }
        let request = external.request();
        let paragraph = self.bindings.pop().ok_or(ExactBlockJobError::Invariant(
            "reference prefix requires the terminal Paragraph binding",
        ))?;
        if paragraph.kind() != GreenKind::PARAGRAPH {
            return Err(ExactBlockJobError::Invariant(
                "reference prefix targets the terminal Paragraph",
            ));
        }
        document.candidate_writer_start_reference_prefix(self.epoch, paragraph, request)?;
        self.writer_work = Some(WriterWork::ReferencePrefixSetup { request });
        Ok(())
    }

    fn poll_stage_terminator(
        &mut self,
        document: &mut LiveDocumentStore,
        end: u32,
        ending: DirectLineEnding,
    ) -> Result<(), ExactBlockJobError> {
        match document.poll_candidate_writer_source(self.epoch, 1)? {
            CandidateWriterSourcePoll::NeedFuel(_) => {
                self.source_work = Some(SourceWork::StageTerminator { end, ending });
            }
            CandidateWriterSourcePoll::Eof(_) => {
                return Err(ExactBlockJobError::Invariant(
                    "source ended before the parser-staged terminator",
                ));
            }
            CandidateWriterSourcePoll::Atom { atom, .. } => {
                let current = self.current_line()?.authoritative_offset;
                let advance = source_atom_bytes(atom.kind());
                if !direct_ending_matches(ending, atom.kind())
                    || current.checked_add(advance) != Some(end)
                {
                    return Err(ExactBlockJobError::Invariant(
                        "parser terminator kind/range disagrees with the source atom",
                    ));
                }
                let terminal = self.bindings.last().ok_or(ExactBlockJobError::Invariant(
                    "terminator has terminal binding",
                ))?;
                document.candidate_writer_stage_terminator(self.epoch, atom, terminal)?;
                self.current_line_mut()?.authoritative_offset = end;
                self.acknowledge_command()?;
            }
        }
        Ok(())
    }

    fn poll_source_eof(
        &mut self,
        document: &mut LiveDocumentStore,
    ) -> Result<(), ExactBlockJobError> {
        match document.poll_candidate_writer_source(self.epoch, 1)? {
            CandidateWriterSourcePoll::NeedFuel(_) => {
                self.source_work = Some(SourceWork::EnsureEof);
            }
            CandidateWriterSourcePoll::Atom { .. } => {
                return Err(ExactBlockJobError::Invariant(
                    "unclaimed source remained at document finish",
                ));
            }
            CandidateWriterSourcePoll::Eof(_) => {
                document.candidate_writer_start_finish(self.epoch)?;
                self.writer_work = Some(WriterWork::Finish);
            }
        }
        Ok(())
    }

    fn poll_line_eof(
        &mut self,
        document: &mut LiveDocumentStore,
        physical_bytes: u32,
        physical_utf16: u32,
    ) -> Result<(), ExactBlockJobError> {
        match document.poll_candidate_writer_source(self.epoch, 1)? {
            CandidateWriterSourcePoll::NeedFuel(_) => {
                self.source_work = Some(SourceWork::EnsureLineEof {
                    physical_bytes,
                    physical_utf16,
                });
            }
            CandidateWriterSourcePoll::Atom { .. } => {
                return Err(ExactBlockJobError::Invariant(
                    "unclaimed source remained after a bare EOF line",
                ));
            }
            CandidateWriterSourcePoll::Eof(_) => {
                let line = self.current_line()?;
                if line.physical_bytes != physical_bytes || line.physical_utf16 != physical_utf16 {
                    return Err(ExactBlockJobError::Invariant(
                        "bare EOF line metric changed while awaiting source authority",
                    ));
                }
                self.finish_line_after_source_end(document)?;
            }
        }
        Ok(())
    }

    fn poll_consume_range(
        &mut self,
        document: &mut LiveDocumentStore,
        end: u32,
        owner_index: usize,
        part: CoveragePart,
        logical: DirectLogicalAction,
    ) -> Result<(), ExactBlockJobError> {
        if self.current_line()?.authoritative_offset == end {
            return self.acknowledge_command();
        }
        match document.poll_candidate_writer_source(self.epoch, 1)? {
            CandidateWriterSourcePoll::NeedFuel(_) => {}
            CandidateWriterSourcePoll::Atom { atom, .. } => {
                let advance = source_atom_bytes(atom.kind());
                if self
                    .current_line()?
                    .authoritative_offset
                    .checked_add(advance)
                    .is_none_or(|next| next > end)
                {
                    return Err(ExactBlockJobError::Invariant(
                        "source atom crosses parser consume range",
                    ));
                }
                let logical_action = match logical {
                    DirectLogicalAction::None => CandidateWriterLogicalAction::None,
                    DirectLogicalAction::HiddenUpstream => CandidateWriterLogicalAction::Hidden {
                        target: &self.bindings[owner_index],
                        affinity: GreenAffinity::Upstream,
                    },
                    DirectLogicalAction::CanonicalNewline => {
                        if atom_ending(atom.kind()).is_none()
                            || advance != end - self.current_line()?.authoritative_offset
                        {
                            return Err(ExactBlockJobError::Invariant(
                                "canonical newline is not one exact line-ending atom",
                            ));
                        }
                        CandidateWriterLogicalAction::CanonicalLineEnding {
                            target: &self.bindings[owner_index],
                        }
                    }
                    DirectLogicalAction::Identity => CandidateWriterLogicalAction::Identity {
                        target: &self.bindings[owner_index],
                    },
                    DirectLogicalAction::PartialTab(partial) => {
                        if atom.kind() != CandidateSourceAtomKind::Tab || advance != 1 {
                            return Err(ExactBlockJobError::Invariant(
                                "partial Tab recipe did not receive one full Tab atom",
                            ));
                        }
                        let target_index = self.binding_index(partial.logical_target())?;
                        CandidateWriterLogicalAction::TabToSpaces {
                            target: &self.bindings[target_index],
                            spaces: partial.remaining_spaces(),
                        }
                    }
                    DirectLogicalAction::CanonicalText => {
                        return Err(ExactBlockJobError::Invariant(
                            "CanonicalText bypassed the bounded range replay",
                        ));
                    }
                };
                document.candidate_writer_start_consume(
                    self.epoch,
                    atom,
                    &self.bindings[owner_index],
                    part,
                    logical_action,
                )?;
                self.writer_work = Some(WriterWork::AtomConsume { advance });
            }
            CandidateWriterSourcePoll::Eof(_) => {
                return Err(ExactBlockJobError::Invariant(
                    "source ended inside parser consume range",
                ));
            }
        }
        self.source_work = Some(SourceWork::ConsumeRange {
            end,
            owner_index,
            part,
            logical,
        });
        Ok(())
    }

    fn poll_blank_range(
        &mut self,
        document: &mut LiveDocumentStore,
        end: u32,
    ) -> Result<(), ExactBlockJobError> {
        if self.current_line()?.authoritative_offset == end {
            document.candidate_writer_stage_blank_gap(self.epoch)?;
            return self.acknowledge_command();
        }
        match document.poll_candidate_writer_source(self.epoch, 1)? {
            CandidateWriterSourcePoll::NeedFuel(_) => {}
            CandidateWriterSourcePoll::Atom { atom, .. } => {
                let advance = source_atom_bytes(atom.kind());
                if self
                    .current_line()?
                    .authoritative_offset
                    .checked_add(advance)
                    .is_none_or(|next| next > end)
                {
                    return Err(ExactBlockJobError::Invariant(
                        "source atom crosses blank range",
                    ));
                }
                document.candidate_writer_defer_blank_gap_atom(self.epoch, atom)?;
                self.current_line_mut()?.authoritative_offset += advance;
            }
            CandidateWriterSourcePoll::Eof(_) => {
                return Err(ExactBlockJobError::Invariant(
                    "source ended inside blank range",
                ));
            }
        }
        self.source_work = Some(SourceWork::StageBlankRange { end });
        Ok(())
    }

    fn poll_writer(
        &mut self,
        document: &mut LiveDocumentStore,
        work: WriterWork,
    ) -> Result<(), ExactBlockJobError> {
        let progress = document.poll_candidate_writer(self.epoch)?;
        match (work, progress) {
            (work, CandidateWriterProgress::Pending) => {
                self.writer_work = Some(work);
            }
            (WriterWork::Open { kind }, CandidateWriterProgress::Opened(binding)) => {
                if binding.kind() != kind {
                    return Err(ExactBlockJobError::Invariant(
                        "writer opened a different block kind",
                    ));
                }
                self.bindings.push(binding);
                self.acknowledge_command()?;
            }
            (
                WriterWork::FinalizeParagraph {
                    expected_identity,
                    expected_facts,
                },
                CandidateWriterProgress::Retyped { binding, facts },
            ) => {
                if binding.kind() != GreenKind::HEADING
                    || binding.identity(self.epoch) != expected_identity
                    || facts != expected_facts
                {
                    return Err(ExactBlockJobError::Invariant(
                        "writer finalized Paragraph with different identity, kind, or facts",
                    ));
                }
                self.bindings.push(binding);
                self.acknowledge_command()?;
            }
            (
                WriterWork::FinalizeParagraph {
                    expected_identity,
                    expected_facts,
                },
                CandidateWriterProgress::RetypedWithDeferredResidual { binding, facts },
            ) => {
                if binding.kind() != GreenKind::HEADING
                    || binding.identity(self.epoch) == expected_identity
                    || facts != expected_facts
                {
                    return Err(ExactBlockJobError::Invariant(
                        "writer finalized restart-crossing Paragraph with a crossed residual",
                    ));
                }
                self.bindings.push(binding);
                self.acknowledge_command()?;
            }
            (WriterWork::AtomConsume { advance }, CandidateWriterProgress::ActionComplete) => {
                self.current_line_mut()?.authoritative_offset += advance;
            }
            (
                WriterWork::RangeReplay { start, end },
                CandidateWriterProgress::RangeReplayReady(receipt),
            ) => {
                let line = self.current_line()?;
                let absolute_line_start = line.recognition.absolute_range().0;
                if receipt.source() != self.epoch.source()
                    || receipt.build_id() != self.epoch.build_id()
                    || receipt.line_ordinal() != line.recognition.line_ordinal()
                    || receipt.absolute_range()
                        != (
                            absolute_line_start + u64::from(start),
                            absolute_line_start + u64::from(end),
                        )
                    || receipt.physical_bytes() != u64::from(end - start)
                {
                    return Err(ExactBlockJobError::Invariant(
                        "range replay returned a different source/build/line/endpoint",
                    ));
                }
                self.current_line_mut()?.authoritative_offset = end;
                self.acknowledge_command()?;
            }
            (WriterWork::Resolve | WriterWork::Close, CandidateWriterProgress::ActionComplete) => {
                self.acknowledge_command()?;
            }
            (WriterWork::Finish, CandidateWriterProgress::CompletionReady) => {
                if let Some(samples) = self.restart_samples.take() {
                    if let Err(failure) =
                        document.commit_candidate_writer_restart_composite(self.epoch, samples)
                    {
                        let crate::live_document::CandidateWriterMechanismCommitFailure {
                            error,
                            abort,
                        } = *failure;
                        self.failed_commit_abort = abort;
                        return Err(ExactBlockJobError::Commit(error));
                    }
                } else {
                    let built = match document.commit_candidate_writer_mechanism(self.epoch) {
                        Ok(built) => built,
                        Err(failure) => {
                            let crate::live_document::CandidateWriterMechanismCommitFailure {
                                error,
                                abort,
                            } = *failure;
                            self.failed_commit_abort = abort;
                            return Err(ExactBlockJobError::Commit(error));
                        }
                    };
                    self.built = Some(built);
                }
                self.acknowledge_command()?;
                self.mode = DriverMode::Complete;
            }
            (
                WriterWork::ReferencePrefixSetup { request },
                CandidateWriterProgress::ReferencePrefixSourceReady { identity },
            ) => {
                let work = self
                    .parser_mut()?
                    .begin_reference_prefix_work(request, identity)?;
                document.candidate_writer_install_reference_prefix_work(
                    self.epoch,
                    identity,
                    work,
                )?;
                self.writer_work = Some(WriterWork::ReferencePrefixDrive { request, identity });
            }
            (
                WriterWork::ReferencePrefixDrive { request, identity },
                CandidateWriterProgress::ReferencePrefixTerminalReady(terminal),
            ) => {
                let (binding, terminal_identity, ack) = terminal.into_parts();
                if terminal_identity != identity {
                    return Err(ExactBlockJobError::Invariant(
                        "reference terminal crossed its source identity",
                    ));
                }
                let status = self
                    .parser_mut()?
                    .commit_reference_prefix_terminal(ack, identity)?;
                let expects_binding = match status {
                    DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
                    | DirectReferencePrefixCommitStatus::VisibleRemainderArmed => true,
                    DirectReferencePrefixCommitStatus::ReferenceOnlyArmed => matches!(
                        request.context(),
                        flark_comrak_value_block_core::DirectReferencePrefixContext::SetextCandidate
                    ),
                };
                if expects_binding != binding.is_some() {
                    return Err(ExactBlockJobError::Invariant(
                        "reference terminal binding disagrees with parser disposition",
                    ));
                }
                if let Some(binding) = binding {
                    self.bindings.push(binding);
                }
            }
            _ => {
                return Err(ExactBlockJobError::Invariant(
                    "writer progress does not match parser command",
                ));
            }
        }
        Ok(())
    }

    fn acknowledge_command(&mut self) -> Result<(), ExactBlockJobError> {
        let command = self
            .active_command
            .take()
            .ok_or(ExactBlockJobError::Invariant("no parser command is active"))?;
        self.parser_mut()?.acknowledge_command()?;
        if matches!(self.mode, DriverMode::Initial) {
            if !matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::Document
                }
            ) {
                return Err(ExactBlockJobError::Invariant(
                    "first direct command must open the document",
                ));
            }
            self.mode = DriverMode::Recognizing;
        }
        Ok(())
    }

    fn binding_index(&self, owner: DirectOwner) -> Result<usize, ExactBlockJobError> {
        let generations = usize::try_from(owner.generations_from_top())
            .map_err(|_| ExactBlockJobError::Invariant("owner depth fits usize"))?;
        let distance = generations
            .checked_add(1)
            .ok_or(ExactBlockJobError::Invariant(
                "owner depth does not overflow",
            ))?;
        self.bindings
            .len()
            .checked_sub(distance)
            .ok_or(ExactBlockJobError::Invariant("selected owner is open"))
    }

    fn current_line(&self) -> Result<&CurrentLine, ExactBlockJobError> {
        self.current_line
            .as_ref()
            .ok_or(ExactBlockJobError::Invariant(
                "parser command has a current line",
            ))
    }

    fn current_line_mut(&mut self) -> Result<&mut CurrentLine, ExactBlockJobError> {
        self.current_line
            .as_mut()
            .ok_or(ExactBlockJobError::Invariant(
                "parser command has a current line",
            ))
    }

    fn parser(&self) -> Result<&DirectValueBlockParser, ExactBlockJobError> {
        self.parser.as_ref().ok_or(ExactBlockJobError::Invariant(
            "exact block job parser authority is outside the active job",
        ))
    }

    fn parser_mut(&mut self) -> Result<&mut DirectValueBlockParser, ExactBlockJobError> {
        self.parser.as_mut().ok_or(ExactBlockJobError::Invariant(
            "exact block job parser authority is outside the active job",
        ))
    }

    fn progress(&self) -> ExactBlockJobProgress {
        if matches!(self.mode, DriverMode::Complete) {
            ExactBlockJobProgress::Complete
        } else {
            ExactBlockJobProgress::Pending
        }
    }

    #[cfg(test)]
    fn take_built(&mut self) -> CandidateWriterBuiltDocument {
        self.built
            .take()
            .expect("completed exact job owns its output")
    }

    #[cfg(test)]
    const fn maximum_line_bytes(&self) -> usize {
        self.maximum_line_bytes
    }

    #[cfg(test)]
    fn legacy_event_count(&self) -> usize {
        self.parser()
            .expect("active test job owns its parser")
            .legacy_event_count()
    }

    #[cfg(test)]
    fn scratch_node_count(&self) -> usize {
        self.parser()
            .expect("active test job owns its parser")
            .scratch_node_count()
    }
}

const fn direct_kind(kind: DirectBlockKind) -> GreenKind {
    match kind {
        DirectBlockKind::Document => GreenKind::DOCUMENT,
        DirectBlockKind::BlockQuote => GreenKind::BLOCK_QUOTE,
        DirectBlockKind::List(_) => GreenKind::LIST,
        DirectBlockKind::Item(_) => GreenKind::ITEM,
        DirectBlockKind::Paragraph => GreenKind::PARAGRAPH,
        DirectBlockKind::Heading(_) => GreenKind::HEADING,
        DirectBlockKind::FencedCode(_) => GreenKind::FENCED_CODE,
    }
}

fn direct_fenced_code_open_facts(
    facts: DirectFencedCodeFacts,
) -> Result<GreenFencedCodeOpenFacts, ExactBlockJobError> {
    GreenFencedCodeOpenFacts::new(
        match facts.fence {
            DirectFenceCharacter::Backtick => GreenFenceCharacter::Backtick,
            DirectFenceCharacter::Tilde => GreenFenceCharacter::Tilde,
        },
        facts.minimum_closing_length,
        facts.fence_offset_columns,
    )
    .map_err(serialized_fact_error)
}

fn direct_list_open_facts(
    facts: DirectListFacts,
) -> Result<GreenListOpenFacts, ExactBlockJobError> {
    match facts.list_type {
        DirectListType::Bullet => GreenListBullet::try_from(facts.bullet_char)
            .map(GreenListOpenFacts::bullet)
            .map_err(serialized_fact_error),
        DirectListType::Ordered => GreenListOpenFacts::ordered(
            facts.start,
            match facts.delimiter {
                DirectListDelimiter::Period => GreenListDelimiter::Period,
                DirectListDelimiter::Paren => GreenListDelimiter::Parenthesis,
            },
        )
        .map_err(serialized_fact_error),
    }
}

fn direct_item_open_facts(
    facts: DirectItemFacts,
) -> Result<GreenItemOpenFacts, ExactBlockJobError> {
    GreenItemOpenFacts::new(facts.marker_offset, facts.padding).map_err(serialized_fact_error)
}

fn direct_close_facts(
    kind: GreenKind,
    facts: DirectFinalFacts,
) -> Result<GreenCloseFacts, ExactBlockJobError> {
    match (kind, facts) {
        (GreenKind::LIST, DirectFinalFacts::List { tight }) => Ok(GreenCloseFacts::List { tight }),
        (GreenKind::LIST, DirectFinalFacts::None) => Err(ExactBlockJobError::Invariant(
            "direct List close is missing final tightness",
        )),
        (GreenKind::FENCED_CODE, DirectFinalFacts::None) => Err(ExactBlockJobError::Invariant(
            "direct FencedCode close is missing final facts",
        )),
        (GreenKind::FENCED_CODE, DirectFinalFacts::FencedCode(_)) => Err(
            ExactBlockJobError::Invariant("FencedCode close facts must be writer-derived"),
        ),
        (_, DirectFinalFacts::None) => Ok(GreenCloseFacts::None),
        (_, DirectFinalFacts::List { .. }) => Err(ExactBlockJobError::Invariant(
            "direct List final facts close a non-List block",
        )),
        (_, DirectFinalFacts::FencedCode(_)) => Err(ExactBlockJobError::Invariant(
            "direct FencedCode final facts close a non-FencedCode block",
        )),
    }
}

fn serialized_fact_error(error: SerializedGreenError) -> ExactBlockJobError {
    ExactBlockJobError::Writer(error.into())
}

const fn direct_part(part: DirectCoveragePart) -> CoveragePart {
    match part {
        DirectCoveragePart::Content => CoveragePart::CONTENT,
        DirectCoveragePart::ContainerMarker => CoveragePart::CONTAINER_MARKER,
        DirectCoveragePart::BlockMarker => CoveragePart::BLOCK_MARKER,
        DirectCoveragePart::Gap => CoveragePart::GAP,
        DirectCoveragePart::Terminal => CoveragePart::TERMINAL,
    }
}

const fn candidate_ending_bytes(ending: CandidateLineEnding) -> u32 {
    match ending {
        CandidateLineEnding::Lf | CandidateLineEnding::LoneCr => 1,
        CandidateLineEnding::CrLf => 2,
    }
}

fn source_atom_bytes(kind: CandidateSourceAtomKind) -> u32 {
    match kind {
        CandidateSourceAtomKind::Scalar(value) => match value.len_utf8() {
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            _ => unreachable!("UTF-8 scalar width is one through four"),
        },
        CandidateSourceAtomKind::Tab | CandidateSourceAtomKind::Nul => 1,
        CandidateSourceAtomKind::LineEnding(ending) => candidate_ending_bytes(ending),
    }
}

const fn atom_ending(kind: CandidateSourceAtomKind) -> Option<CandidateLineEnding> {
    match kind {
        CandidateSourceAtomKind::LineEnding(ending) => Some(ending),
        _ => None,
    }
}

const fn direct_ending_matches(direct: DirectLineEnding, source: CandidateSourceAtomKind) -> bool {
    matches!(
        (direct, source),
        (
            DirectLineEnding::Lf,
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::Lf)
        ) | (
            DirectLineEnding::Cr,
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::LoneCr)
        ) | (
            DirectLineEnding::CrLf,
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf)
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use flark_comrak_inline_fragment_gate::{
        EMPTY_REFERENCE_SNAPSHOT, INLINE_FACT_FLAG_SOURCE_BACKED, InlineFactKind, InlineInputKind,
        MAX_INLINE_FRAGMENT_BYTES, OriginRunKind,
    };
    use flark_comrak_value_block_core::{
        DirectDurableGrammarCapture, DirectLineBoundaryDeferredRole,
    };

    use crate::committed_checkpoint_index::{
        DonorCheckpointSampleDraft, RelativeCheckpointMeasure, StorageOnlyCheckpointIndexBuilder,
        StorageOnlyCheckpointPartition, StorageOnlyCommittedCheckpointIndex,
    };
    use crate::setext_cross_build_restart::{
        InMemorySetextActivationError, InMemorySetextActivationProgress,
        InMemorySetextDonorJoinError, JoinedInMemorySetextRestart,
    };
    use crate::{
        BlockId, CandidateWriterConfig, GrammarRevision, GreenFencedCodeCloseFacts,
        GreenFencedCodeOpenFacts, GreenListStyle, InlineLeafOutcome, InlineLeafUnknownReason,
        PageArena, SerializedGreenTestCanonicalEvent, SerializedGreenTestEvent,
        SerializedGreenTestLogical, SerializedMetric, derive_inline_leaf_presentation,
        serialized_green_test_canonical_trace, serialized_green_test_close_facts,
        serialized_green_test_close_states, serialized_green_test_composite_canonical_trace,
        serialized_green_test_logical_segments, serialized_green_test_open_facts,
        serialized_green_test_trace,
    };

    const CONFIG: CandidateWriterConfig = CandidateWriterConfig {
        syntax_profile: 1,
        grammar_revision: GrammarRevision(1),
        semantic_epoch: 1,
    };

    const RETAINED_CONFIG: CandidateWriterConfig = CandidateWriterConfig {
        syntax_profile: 1,
        grammar_revision: GrammarRevision(1),
        semantic_epoch: 2,
    };

    fn document(source: &str) -> (LiveDocumentStore, ExactBlockJob) {
        let mut document = LiveDocumentStore::new(source, 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        document.activate_candidate_writer(epoch, CONFIG).unwrap();
        let job = ExactBlockJob::new(epoch).unwrap();
        (document, job)
    }

    fn drive(source: &str) -> (LiveDocumentStore, ExactBlockJob) {
        let (mut document, mut job) = document(source);
        for _ in 0..200_000 {
            let progress = job.poll(&mut document).unwrap_or_else(|error| {
                panic!(
                    "exact job failed: {error:?}; mode={:?}; command={:?}; line={:?}",
                    job.mode, job.active_command, job.current_line
                )
            });
            match progress {
                ExactBlockJobProgress::Pending => {}
                ExactBlockJobProgress::Complete => return (document, job),
            }
        }
        panic!("exact block job did not converge");
    }

    fn source_backed_atx_document(source: &str) -> (LiveDocumentStore, ExactBlockJob) {
        let mut document = LiveDocumentStore::new(source, 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        document.activate_candidate_writer(epoch, CONFIG).unwrap();
        let job = ExactBlockJob::new_source_backed_atx(epoch).unwrap();
        (document, job)
    }

    fn drive_source_backed_atx(
        source: &str,
        mut axis_fuel: impl FnMut(usize) -> usize,
    ) -> (LiveDocumentStore, ExactBlockJob) {
        let (mut document, mut job) = source_backed_atx_document(source);
        for poll in 0..500_000 {
            let requested = axis_fuel(poll).clamp(1, EXACT_SOURCE_LINE_MAX_DONOR_FUEL);
            let ExactLineInput::SourceBackedAtx { donor_fuel } = &mut job.line_input else {
                panic!("source-backed helper lost its dedicated line-input mode");
            };
            *donor_fuel = requested;
            let progress = job.poll(&mut document).unwrap_or_else(|error| {
                panic!(
                    "source-backed ATX job failed: {error:?}; mode={:?}; source_phase={:?}; command={:?}; line={:?}",
                    job.mode,
                    job.source_line_phase
                        .as_ref()
                        .map(ExactSourceLinePhase::label),
                    job.active_command,
                    job.current_line
                )
            });
            match progress {
                ExactBlockJobProgress::Pending => {}
                ExactBlockJobProgress::Complete => return (document, job),
            }
        }
        panic!(
            "source-backed ATX job did not converge: mode={:?}; source_phase={:?}; command={:?}; source_work={:?}; writer_work={:?}; metrics={:?}",
            job.mode,
            job.source_line_phase
                .as_ref()
                .map(ExactSourceLinePhase::label),
            job.active_command,
            job.source_work,
            job.writer_work,
            job.source_line_metrics,
        );
    }

    fn reach_source_backed_phase(
        source: &str,
        target: &'static str,
        require_donor_poll: bool,
    ) -> (LiveDocumentStore, ExactBlockJob) {
        let (mut document, mut job) = source_backed_atx_document(source);
        for _ in 0..100_000 {
            if job
                .source_line_phase
                .as_ref()
                .is_some_and(|phase| phase.label() == target)
                && (!require_donor_poll || job.source_line_metrics.donor_polls > 0)
            {
                return (document, job);
            }
            job.poll(&mut document)
                .unwrap_or_else(|error| panic!("failed before source phase {target}: {error:?}"));
        }
        panic!("source-backed ATX job did not reach phase {target}");
    }

    fn source_line_phase_ordinal(phase: &ExactSourceLinePhase) -> u64 {
        match phase {
            ExactSourceLinePhase::SessionOpen { session }
            | ExactSourceLinePhase::Polling { session, .. }
            | ExactSourceLinePhase::Matched { session, .. } => session.line_ordinal(),
            ExactSourceLinePhase::ActorFinished { receipt, .. } => receipt.session().line_ordinal(),
        }
    }

    fn reach_source_backed_line_phase(
        source: &str,
        line_ordinal: u64,
        target: &'static str,
    ) -> (LiveDocumentStore, ExactBlockJob) {
        let (mut document, mut job) = source_backed_atx_document(source);
        for _ in 0..100_000 {
            if job.source_line_phase.as_ref().is_some_and(|phase| {
                phase.label() == target && source_line_phase_ordinal(phase) == line_ordinal
            }) {
                return (document, job);
            }
            let ExactLineInput::SourceBackedAtx { donor_fuel } = &mut job.line_input else {
                panic!("source-backed helper lost its dedicated line-input mode");
            };
            *donor_fuel = 1;
            job.poll(&mut document).unwrap_or_else(|error| {
                panic!("failed before source line {line_ordinal} phase {target}: {error:?}")
            });
        }
        panic!("source-backed exact job did not reach line {line_ordinal} phase {target}");
    }

    fn assert_exact_content_ancestry(
        source: &str,
        trace: &[SerializedGreenTestEvent],
        needle: &str,
        expected_path: &[GreenKind],
    ) {
        let byte_start = source
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is present in {source:?}"));
        assert!(
            source[byte_start + needle.len()..].find(needle).is_none(),
            "{needle:?} must identify one source span"
        );
        let target =
            u64::try_from(byte_start).unwrap()..u64::try_from(byte_start + needle.len()).unwrap();
        let mut source_bytes = 0_u64;
        let mut source_utf16 = 0_u64;
        let mut open_path = Vec::new();
        let mut found = None;
        for event in trace {
            match event {
                SerializedGreenTestEvent::Enter { kind, .. } => open_path.push(*kind),
                SerializedGreenTestEvent::Coverage {
                    metric,
                    owner_relative_depth,
                    part,
                    ..
                } => {
                    let range = source_bytes..source_bytes + metric.bytes;
                    if range.start <= target.start && range.end >= target.end {
                        assert!(found.is_none(), "target content has one physical owner");
                        let depth = usize::try_from(*owner_relative_depth).unwrap();
                        let owner_index = open_path.len().checked_sub(depth + 1).unwrap();
                        let owner = open_path
                            .get(owner_index)
                            .copied()
                            .expect("coverage owner is on the open path");
                        found = Some((open_path.clone(), owner, *part));
                    }
                    source_bytes += metric.bytes;
                    source_utf16 += metric.utf16;
                }
                SerializedGreenTestEvent::Exit => {
                    open_path.pop().expect("trace exit has an open block");
                }
            }
        }
        assert!(open_path.is_empty(), "trace closes every block");
        assert_eq!(source_bytes, u64::try_from(source.len()).unwrap());
        assert_eq!(
            source_utf16,
            u64::try_from(source.encode_utf16().count()).unwrap()
        );
        let (actual_path, owner, part) = found.expect("target belongs to one coverage run");
        assert_eq!(actual_path, expected_path, "source target={needle:?}");
        assert_eq!(owner, *expected_path.last().unwrap());
        assert_eq!(part, CoveragePart::CONTENT);
    }

    fn cancel_exact_job(document: &mut LiveDocumentStore, job: ExactBlockJob) {
        let abort = job.cancel(document).unwrap();
        for _ in 0..100_000 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                return;
            }
        }
        panic!("source-backed exact job did not cancel with bounded polling");
    }

    fn assert_source_backed_line_bounds(job: &ExactBlockJob, source_bytes: usize) {
        assert_eq!(job.source_line_metrics.lines_committed, 1);
        assert_eq!(
            job.source_line_metrics.source_first_reads,
            source_bytes as u64
        );
        assert_eq!(job.source_line_metrics.actor_new_bytes, source_bytes as u64);
        assert_eq!(
            job.source_line_metrics.actor_access_work_units,
            source_bytes as u64
        );
        assert_eq!(job.source_line_metrics.actor_repeated_last_byte_peeks, 0);
        assert!(
            job.source_line_metrics.maximum_donor_retained_source_bytes
                <= DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES
        );
        assert!(job.source_line_metrics.maximum_actor_retained_byte_scratch <= 1);
        assert_eq!(
            job.source_line_metrics.maximum_source_request_rewind_bytes,
            0
        );
        assert!(
            job.source_line_metrics.maximum_composed_work_per_poll
                <= EXACT_SOURCE_LINE_MAX_COMPOSED_WORK_PER_POLL
        );
        assert!(job.recognition_line.is_empty());
        assert_eq!(job.maximum_line_bytes(), source_bytes);
        assert_eq!(job.parser().unwrap().retained_line_bytes(), 0);
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn parent_selected_suffix_poll_stops_before_independent_commit() {
        let (mut document, mut job) = document("lead\nbody\n");
        let mut reached_splice = false;
        for _ in 0..200_000 {
            match job
                .poll_parent_selected_suffix(&mut document)
                .expect("parent-selected suffix poll remains exact")
            {
                ParentSelectedExactBlockRunningProgress::Pending => {}
                ParentSelectedExactBlockRunningProgress::CheckpointIndexSpliceRequired => {
                    reached_splice = true;
                    break;
                }
            }
        }
        assert!(
            reached_splice,
            "suffix did not reach its atomic splice seam"
        );
        assert!(
            job.built.is_none(),
            "parent-selected suffix must not produce an independent document"
        );
        assert!(
            matches!(&job.writer_work, Some(WriterWork::Finish)),
            "the exact Finish command remains owned until splice/adoption"
        );
        assert!(!matches!(job.mode, DriverMode::Complete));

        let abort = job
            .cancel(&mut document)
            .expect("sealed-but-uncommitted suffix remains cancellable");
        for _ in 0..8 {
            if document
                .poll_candidate_abort(abort, 1)
                .expect("bounded candidate abort")
                .complete
            {
                return;
            }
        }
        panic!("sealed parent-selected suffix did not cancel with bounded fuel");
    }

    fn drive_with_checkpoint_after_every_line(
        source: &str,
    ) -> (LiveDocumentStore, ExactBlockJob, u64) {
        let (mut document, initial) = document(source);
        let mut job = Some(initial);
        let mut checkpoints = 0_u64;
        for _ in 0..400_000 {
            let progress = job
                .as_mut()
                .expect("driver owns one active exact job")
                .poll(&mut document)
                .unwrap_or_else(|error| panic!("checkpointed exact job failed: {error:?}"));
            if progress == ExactBlockJobProgress::Complete {
                let job = job.take().expect("completed driver owns its exact job");
                assert_eq!(
                    checkpoints, job.acknowledged_lines,
                    "every acknowledged physical line crossed the composite checkpoint"
                );
                return (document, job, checkpoints);
            }
            if !job
                .as_ref()
                .expect("pending driver owns its exact job")
                .is_line_boundary_checkpoint_seam()
            {
                continue;
            }

            let owned = job.take().expect("eligible driver owns its exact job");
            let mut capture = match owned.start_line_boundary_checkpoint(&mut document) {
                Ok(ExactBlockCheckpointAdmission::Started(capture)) => *capture,
                Ok(ExactBlockCheckpointAdmission::Skipped { reason, .. }) => {
                    panic!("eligible exact checkpoint was skipped: {reason:?}")
                }
                Err(failure) => panic!(
                    "eligible exact checkpoint failed to start: {:?}",
                    failure.error
                ),
            };
            let checkpoint = loop {
                match capture.poll(&mut document) {
                    Ok(ExactBlockCheckpointCapturePoll::Pending(next)) => capture = next,
                    Ok(ExactBlockCheckpointCapturePoll::Ready(checkpoint)) => break checkpoint,
                    Err(failure) => {
                        panic!("exact checkpoint capture failed: {:?}", failure.error)
                    }
                }
            };
            checkpoints = checkpoints
                .checked_add(1)
                .expect("test checkpoint count does not overflow");
            assert_eq!(checkpoint.acknowledged_lines(), checkpoints);
            assert!(matches!(
                document.poll_candidate_writer(checkpoint.epoch),
                Err(CandidateWriterError::Busy)
            ));
            job = Some(checkpoint.resume(&mut document).unwrap_or_else(|failure| {
                panic!(
                    "joined exact checkpoint failed to resume: {:?}",
                    failure.error
                )
            }));
        }
        panic!("checkpointed exact block job did not converge");
    }

    fn first_line_boundary_checkpoint_capture(
        source: &str,
    ) -> (LiveDocumentStore, ExactBlockCheckpointCapture) {
        let (mut document, job) = first_line_boundary_job(source);
        let capture = match job.start_line_boundary_checkpoint(&mut document).unwrap() {
            ExactBlockCheckpointAdmission::Started(capture) => *capture,
            ExactBlockCheckpointAdmission::Skipped { reason, .. } => {
                panic!("eligible checkpoint skipped: {reason:?}")
            }
        };
        (document, capture)
    }

    fn first_line_boundary_job(source: &str) -> (LiveDocumentStore, ExactBlockJob) {
        let (mut document, mut job) = document(source);
        for _ in 0..100_000 {
            assert_eq!(
                job.poll(&mut document).unwrap(),
                ExactBlockJobProgress::Pending
            );
            if job.is_line_boundary_checkpoint_seam() {
                return (document, job);
            }
        }
        panic!("exact job did not reach its first checkpoint seam");
    }

    fn ready_actor_writer_checkpoint(document: &mut LiveDocumentStore, epoch: LiveCandidateEpoch) {
        assert_eq!(
            document
                .candidate_writer_start_line_boundary_checkpoint(epoch)
                .unwrap(),
            CandidateLineBoundaryCheckpointAdmission::Started
        );
        for _ in 0..100_000 {
            match document.poll_candidate_writer(epoch).unwrap() {
                CandidateWriterProgress::Pending => {}
                CandidateWriterProgress::LineBoundaryCheckpointReady => return,
                progress => panic!("checkpoint writer returned {progress:?}"),
            }
        }
        panic!("actor writer checkpoint did not become ready");
    }

    fn take_parser_authority(
        job: &mut ExactBlockJob,
        epoch: LiveCandidateEpoch,
    ) -> ParserLineBoundaryCheckpointAuthority {
        let parser = job
            .parser
            .take()
            .expect("boundary job owns one direct parser");
        ParserLineBoundaryCheckpointAuthority::capture(epoch, parser).unwrap_or_else(|failure| {
            panic!("boundary parser failed to pause: {:?}", failure.error)
        })
    }

    fn reject_crossed_checkpoint_and_prove_actor_cancel(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        parser: ParserLineBoundaryCheckpointAuthority,
        bindings: &[CandidateWriterBinding],
        expected: CandidateWriterError,
        boundary: &str,
    ) {
        let failure = document
            .pause_candidate_writer_at_line_boundary(epoch, parser, bindings)
            .expect_err("crossed parser/writer checkpoint must be rejected");
        let (parser, error) = *failure;
        assert_eq!(error, expected, "unexpected rejection at {boundary}");
        DirectValueBlockParser::resume_line_boundary_pause(parser.into_pause()).unwrap_or_else(
            |error| panic!("rejected parser half was lost at {boundary}: {error:?}"),
        );

        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(document, abort, boundary);
    }

    fn first_ready_line_boundary_checkpoint(
        source: &str,
    ) -> (LiveDocumentStore, ExactBlockCheckpoint, usize) {
        let (mut document, mut capture) = first_line_boundary_checkpoint_capture(source);
        let mut polls = 0_usize;
        loop {
            polls = polls
                .checked_add(1)
                .expect("checkpoint capture poll count fits usize");
            match capture.poll(&mut document).unwrap() {
                ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => {
                    return (document, checkpoint, polls);
                }
            }
        }
    }

    fn ready_checkpoint_from_job(
        document: &mut LiveDocumentStore,
        job: ExactBlockJob,
    ) -> ExactBlockCheckpoint {
        let mut capture = match job.start_line_boundary_checkpoint(document).unwrap() {
            ExactBlockCheckpointAdmission::Started(capture) => *capture,
            ExactBlockCheckpointAdmission::Skipped { reason, .. } => {
                panic!("eligible sparse checkpoint skipped: {reason:?}")
            }
        };
        for _ in 0..100_000 {
            match capture.poll(document).unwrap() {
                ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => return checkpoint,
            }
        }
        panic!("sparse checkpoint capture did not converge");
    }

    fn capture_sparse_donor_samples(
        source: &str,
        line_cadence: u64,
    ) -> (
        LiveDocumentStore,
        Vec<RelativeCheckpointMeasure>,
        DonorCheckpointSampleCursor,
    ) {
        assert_ne!(line_cadence, 0);
        let (mut document, initial) = document(source);
        let mut job = Some(initial);
        let mut samples = Vec::new();
        let mut cursor = None;
        let maximum_polls = source
            .chars()
            .count()
            .saturating_mul(20)
            .saturating_add(2_000_000);
        for _ in 0..maximum_polls {
            let progress = job
                .as_mut()
                .expect("sparse sampler owns an exact job")
                .poll(&mut document)
                .unwrap();
            if progress == ExactBlockJobProgress::Complete {
                drop(job.take().expect("completed sparse sampler owns its job"));
                return (
                    document,
                    samples,
                    cursor.expect("sparse sampler captured at least one checkpoint"),
                );
            }
            let pending = job.as_ref().expect("pending sparse sampler owns its job");
            if !pending.is_line_boundary_checkpoint_seam()
                || pending.acknowledged_lines % line_cadence != 0
            {
                continue;
            }
            let checkpoint = ready_checkpoint_from_job(
                &mut document,
                job.take().expect("eligible sparse sampler owns its job"),
            );
            let capture = match cursor.take() {
                None => checkpoint
                    .capture_first_donor_checkpoint_sample(&mut document)
                    .unwrap(),
                Some(previous) => checkpoint
                    .capture_successive_donor_checkpoint_sample(&mut document, previous)
                    .unwrap_or_else(|failure| {
                        panic!("successive sparse sample failed: {:?}", failure.error)
                    }),
            };
            let (sample, next) = capture.into_parts();
            samples.push(sample);
            cursor = Some(next);
            job = Some(checkpoint.resume(&mut document).unwrap());
        }
        panic!("sparse checkpoint sampler did not converge");
    }

    type RestartDriveFailure = Box<(LiveDocumentStore, ExactBlockJob, ExactBlockJobError)>;
    type RestartDriveResult = Result<(LiveDocumentStore, ExactBlockJob), RestartDriveFailure>;

    fn drive_restart_parent(
        source: &str,
        terminal_sample_line: u64,
        sample_line_cadence: u64,
        forge_chain: bool,
    ) -> RestartDriveResult {
        assert_ne!(sample_line_cadence, 0);
        assert_eq!(terminal_sample_line % sample_line_cadence, 0);
        let (mut document, initial) = document(source);
        let mut job = Some(initial);
        let mut chain: Option<RestartCheckpointSampleChain> = None;
        let mut cursor = None;
        let maximum_polls = source
            .chars()
            .count()
            .saturating_mul(20)
            .saturating_add(2_000_000);
        for _ in 0..maximum_polls {
            let progress = match job
                .as_mut()
                .expect("restart driver owns an exact job")
                .poll(&mut document)
            {
                Ok(progress) => progress,
                Err(error) => {
                    return Err(Box::new((
                        document,
                        job.take().expect("failed restart driver owns its job"),
                        error,
                    )));
                }
            };
            if progress == ExactBlockJobProgress::Complete {
                return Ok((
                    document,
                    job.take().expect("completed restart driver owns its job"),
                ));
            }
            let pending = job.as_ref().expect("pending restart driver owns its job");
            if !pending.is_line_boundary_checkpoint_seam()
                || pending.acknowledged_lines > terminal_sample_line
                || pending.acknowledged_lines % sample_line_cadence != 0
            {
                continue;
            }
            let checkpoint = ready_checkpoint_from_job(
                &mut document,
                job.take().expect("eligible restart driver owns its job"),
            );
            let acknowledged_lines = checkpoint.acknowledged_lines();
            let capture = match cursor.take() {
                None => checkpoint
                    .capture_first_donor_checkpoint_sample(&mut document)
                    .unwrap(),
                Some(previous) => checkpoint
                    .capture_successive_donor_checkpoint_sample(&mut document, previous)
                    .unwrap_or_else(|failure| {
                        panic!("successive restart sample failed: {:?}", failure.error)
                    }),
            };
            if let Some(samples) = chain.as_mut() {
                cursor = Some(samples.try_append(capture).unwrap_or_else(|failure| {
                    panic!("restart chain append failed: {:?}", failure.error)
                }));
            } else {
                let (samples, next) = capture.try_start_restart_chain().unwrap_or_else(|failure| {
                    panic!("restart chain start failed: {:?}", failure.error)
                });
                chain = Some(samples);
                cursor = Some(next);
            }
            let mut resumed = checkpoint.resume(&mut document).unwrap();
            if acknowledged_lines == terminal_sample_line {
                if forge_chain {
                    chain
                        .as_mut()
                        .expect("terminal checkpoint owns sample chain")
                        .forge_next_ordinal_for_test();
                }
                resumed
                    .install_restart_sample_chain(
                        chain.take().expect("terminal checkpoint owns sample chain"),
                    )
                    .unwrap_or_else(|_| panic!("fresh exact job accepts one restart chain"));
                cursor = None;
            }
            job = Some(resumed);
        }
        panic!("restart parent driver did not converge");
    }

    fn summed_sample_intervals(samples: &[RelativeCheckpointMeasure]) -> RelativeCheckpointMeasure {
        samples
            .iter()
            .fold(RelativeCheckpointMeasure::default(), |total, sample| {
                let interval = *sample;
                RelativeCheckpointMeasure::new(
                    total
                        .source_bytes()
                        .checked_add(interval.source_bytes())
                        .unwrap(),
                    total
                        .source_utf16()
                        .checked_add(interval.source_utf16())
                        .unwrap(),
                    total
                        .physical_lines()
                        .checked_add(interval.physical_lines())
                        .unwrap(),
                    total
                        .green_events()
                        .checked_add(interval.green_events())
                        .unwrap(),
                    total
                        .projection_runs()
                        .checked_add(interval.projection_runs())
                        .unwrap(),
                )
            })
    }

    fn drain_abort_and_prove_fresh_candidate(
        document: &mut LiveDocumentStore,
        abort: CandidateAbort,
        boundary: &str,
    ) {
        assert_eq!(
            document.candidate_epoch(),
            None,
            "cancel detaches the candidate at {boundary}"
        );

        let without_fuel = document.poll_candidate_abort(abort, 0).unwrap();
        assert_eq!(
            without_fuel.owners_scheduled, 0,
            "zero fuel cannot reclaim an owner at {boundary}"
        );
        let mut complete = without_fuel.complete;
        if !complete {
            for _ in 0..1_000 {
                let receipt = document.poll_candidate_abort(abort, 1).unwrap();
                assert!(
                    receipt.owners_scheduled <= 1,
                    "unit fuel bounds owner reclamation at {boundary}"
                );
                if receipt.complete {
                    complete = true;
                    break;
                }
            }
        }
        assert!(
            complete,
            "fuelled abort did not converge at checkpoint boundary {boundary}"
        );

        let token = document.active_parse_plan().unwrap().token;
        let next = document
            .begin_candidate(token)
            .unwrap_or_else(|error| panic!("fresh candidate rejected at {boundary}: {error:?}"));
        let _identity = document
            .mint_block_permit(next)
            .unwrap_or_else(|error| panic!("fresh identity rejected at {boundary}: {error:?}"));
        let next_abort = document.cancel_candidate(next).unwrap();
        assert!(
            document
                .poll_candidate_abort(next_abort, 0)
                .unwrap()
                .complete,
            "identity-only fresh candidate aborts without arena work at {boundary}"
        );
    }

    fn install_parent_selected_restart(
        document: &mut LiveDocumentStore,
        restart_bytes: usize,
    ) -> (
        LiveCandidateEpoch,
        crate::live_document::persisted_restart_activation::PersistedRestartActivationHandle,
    ) {
        use crate::live_document::persisted_restart_activation::{
            PersistedRestartActivationProgress, PersistedRestartWriterProgress,
        };

        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let activation = document
            .begin_persisted_restart_activation(
                epoch,
                u64::try_from(restart_bytes).unwrap(),
                RETAINED_CONFIG,
            )
            .unwrap();
        loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::Ready(_) => break,
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("fixture unexpectedly lost its nonzero restart")
                }
            }
        }
        document
            .select_persisted_restart_parent_for_adoption(activation)
            .unwrap();
        loop {
            match document
                .poll_persisted_candidate_writer_restart(activation)
                .unwrap()
            {
                PersistedRestartWriterProgress::Pending => {}
                PersistedRestartWriterProgress::Installed(_) => break,
            }
        }
        (epoch, activation)
    }

    fn matched_direct_adoption_splice_fixture() -> (
        LiveDocumentStore,
        LiveCandidateEpoch,
        crate::live_document::persisted_restart_activation::PersistedRestartActivationHandle,
    ) {
        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document.accept_edit(parent, 7..7, "X").unwrap();
        let (epoch, activation) = install_parent_selected_restart(&mut document, 6);
        for _ in 0..200_000 {
            match document.poll_persisted_exact_restart(activation).unwrap() {
                ParentSelectedExactBlockDriverProgress::Rejoining
                | ParentSelectedExactBlockDriverProgress::Mapping
                | ParentSelectedExactBlockDriverProgress::Running
                | ParentSelectedExactBlockDriverProgress::ConvergenceCapture => {}
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired => {
                    return (document, epoch, activation);
                }
                ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired => {
                    panic!("unchanged retained tail unexpectedly required full replay")
                }
            }
        }
        panic!("matched direct fixture did not reach convergence join")
    }

    fn drive_parent_selected_to_mapped_capture(
        document: &mut LiveDocumentStore,
        activation: crate::live_document::persisted_restart_activation::PersistedRestartActivationHandle,
    ) -> (
        RelativeCheckpointMeasure,
        crate::parent_selected_convergence::ParentSelectedConvergenceMapReceipt,
    ) {
        for _ in 0..400_000 {
            match document.poll_persisted_exact_restart(activation).unwrap() {
                ParentSelectedExactBlockDriverProgress::Rejoining
                | ParentSelectedExactBlockDriverProgress::Mapping
                | ParentSelectedExactBlockDriverProgress::Running => {}
                ParentSelectedExactBlockDriverProgress::ConvergenceCapture => {
                    if let Some(mapped) = document
                        .persisted_mapped_convergence_for_test(activation)
                        .unwrap()
                    {
                        return mapped;
                    }
                }
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired => {
                    panic!("driver skipped the observable mapped-C capture phase")
                }
                ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired => {
                    panic!("fixture unexpectedly required full-suffix replacement")
                }
            }
        }
        panic!("mapped convergence capture did not converge")
    }

    fn finish_parent_selected_convergence(
        document: &mut LiveDocumentStore,
        activation: crate::live_document::persisted_restart_activation::PersistedRestartActivationHandle,
    ) -> ParentSelectedExactBlockDriverProgress {
        for _ in 0..400_000 {
            let progress = document.poll_persisted_exact_restart(activation).unwrap();
            if matches!(
                progress,
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
                    | ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired
            ) {
                return progress;
            }
        }
        panic!("parent-selected exact driver did not reach a terminal join phase")
    }

    fn assert_canonical_green_semantics_equal_without_history_ids(
        actual: &[SerializedGreenTestCanonicalEvent],
        expected: &[SerializedGreenTestCanonicalEvent],
    ) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            match (actual, expected) {
                (
                    SerializedGreenTestCanonicalEvent::Enter {
                        kind: actual_kind, ..
                    },
                    SerializedGreenTestCanonicalEvent::Enter {
                        kind: expected_kind,
                        ..
                    },
                ) => assert_eq!(actual_kind, expected_kind),
                (
                    SerializedGreenTestCanonicalEvent::Coverage {
                        metric: actual_metric,
                        owner_relative_depth: actual_depth,
                        part: actual_part,
                    },
                    SerializedGreenTestCanonicalEvent::Coverage {
                        metric: expected_metric,
                        owner_relative_depth: expected_depth,
                        part: expected_part,
                    },
                ) => {
                    assert_eq!(actual_metric, expected_metric);
                    assert_eq!(actual_depth, expected_depth);
                    assert_eq!(actual_part, expected_part);
                }
                (
                    SerializedGreenTestCanonicalEvent::Exit,
                    SerializedGreenTestCanonicalEvent::Exit,
                ) => {}
                _ => panic!("canonical green structure differs: {actual:?} != {expected:?}"),
            }
        }
    }

    fn donor_capture_after_line(line: &str) -> DirectDurableGrammarCapture {
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        assert!(parser.pending_command().is_some());
        parser.acknowledge_command().unwrap();
        parser.begin_line(line.to_owned()).unwrap();
        let limit = line.len().saturating_mul(8).saturating_add(256);
        for _ in 0..limit {
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
        panic!("direct donor fixture did not reach a line boundary");
    }

    fn joined_setext_fixture(
        source: &str,
        checkpoint_cut: u64,
    ) -> (
        LiveDocumentStore,
        CandidateWriterBuiltDocument,
        JoinedInMemorySetextRestart,
        StorageOnlyCommittedCheckpointIndex,
        crate::BlockId,
    ) {
        let (mut document, checkpoint, _) = first_ready_line_boundary_checkpoint(source);
        let (donor_sample, restart_draft) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let retained_block = restart_draft.green().block();
        let mut old_job = checkpoint.resume(&mut document).unwrap();
        for _ in 0..200_000 {
            if old_job.poll(&mut document).unwrap() == ExactBlockJobProgress::Complete {
                break;
            }
        }
        assert!(old_job.built.is_some());
        let old_document = old_job.take_built();
        let sealed = restart_draft
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        let mut index_builder = StorageOnlyCheckpointIndexBuilder::default();
        index_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(donor_sample))
            .unwrap();
        let (index, _) = index_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let donor = index
            .locate_donor_checkpoint_at_or_before_cut(
                document.candidate_writer_test_arena(),
                checkpoint_cut,
            )
            .unwrap()
            .unwrap();
        let joined = sealed.join_located_donor(donor).unwrap();
        (document, old_document, joined, index, retained_block)
    }

    fn reject_joined_setext_restart_without_checkpoint_coverage(
        document: &mut LiveDocumentStore,
        old_document: &CandidateWriterBuiltDocument,
        joined: JoinedInMemorySetextRestart,
        edit: std::ops::Range<usize>,
        replacement: &str,
    ) {
        let before_edit = document.source_descriptor();
        document
            .accept_edit(before_edit, edit, replacement)
            .unwrap();
        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        let mut activation = document
            .begin_in_memory_setext_activation(epoch, joined, old_document, RETAINED_CONFIG)
            .unwrap();
        for _ in 0..200_000 {
            if document
                .poll_in_memory_setext_activation(epoch, &mut activation, 1)
                .unwrap()
                == InMemorySetextActivationProgress::Ready
            {
                break;
            }
        }
        assert!(activation.is_ready());
        let ready = activation.take_ready().unwrap();
        let driver = document
            .activate_ready_in_memory_setext(epoch, ready)
            .unwrap();
        let mut capture =
            *ExactBlockJob::start_retained_setext_rejoin(epoch, driver, document).unwrap();
        let checkpoint = loop {
            match capture.poll(document).unwrap() {
                ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => break checkpoint,
            }
        };
        assert_eq!(checkpoint.acknowledged_lines(), 1);
        let mut job = checkpoint.resume(document).unwrap();
        let error = loop {
            match job.poll(document) {
                Ok(ExactBlockJobProgress::Pending) => {}
                Ok(ExactBlockJobProgress::Complete) => {
                    panic!(
                        "suffix-local Setext restart must not commit without cumulative coverage"
                    )
                }
                Err(error) => break error,
            }
        };
        assert!(matches!(
            error,
            ExactBlockJobError::Commit(CandidateWriterError::Projection(
                crate::SourceProjectionComposerError::MissingCheckpointCoverage
            ))
        ));
        let abort = job.cancel(document).unwrap();
        drain_abort_and_prove_fresh_candidate(
            document,
            abort,
            "suffix-local Setext missing checkpoint coverage",
        );
    }

    fn fenced_facts(
        document: &LiveDocumentStore,
        built: &CandidateWriterBuiltDocument,
    ) -> (GreenFencedCodeOpenFacts, GreenFencedCodeCloseFacts) {
        let arena = document.candidate_writer_test_arena();
        let open = serialized_green_test_open_facts(built.green_document(), arena)
            .unwrap()
            .into_iter()
            .find_map(|(kind, facts)| {
                (kind == GreenKind::FENCED_CODE)
                    .then(|| GreenFencedCodeOpenFacts::try_from_envelope(&facts).unwrap())
            })
            .expect("fenced-code open facts are stored");
        let close = serialized_green_test_close_facts(built.green_document(), arena)
            .unwrap()
            .into_iter()
            .find_map(|(kind, facts)| match (kind, facts) {
                (GreenKind::FENCED_CODE, GreenCloseFacts::FencedCode(facts)) => Some(facts),
                _ => None,
            })
            .expect("fenced-code close facts are stored");
        (open, close)
    }

    #[test]
    fn pause_after_every_physical_line_matches_uninterrupted_semantics() {
        for source in [
            "alpha\r\nbeta\n",
            "alpha\n===\ntail",
            "# alpha#   \nnext\n",
            "### alpha ###   \r\nnext",
            "> a\n> \n- b\n",
            "> ````lang\r\n> body\n> ````\n",
        ] {
            let (baseline_document, mut baseline_job) = drive(source);
            let (checkpoint_document, mut checkpoint_job, checkpoints) =
                drive_with_checkpoint_after_every_line(source);
            let baseline = baseline_job.take_built();
            let checkpointed = checkpoint_job.take_built();

            let baseline_trace = serialized_green_test_canonical_trace(
                baseline.green_document(),
                baseline_document.candidate_writer_test_arena(),
            )
            .unwrap();
            let checkpoint_trace = serialized_green_test_canonical_trace(
                checkpointed.green_document(),
                checkpoint_document.candidate_writer_test_arena(),
            )
            .unwrap();
            assert_eq!(checkpoint_trace, baseline_trace, "source {source:?}");
            assert_eq!(
                serialized_green_test_logical_segments(
                    checkpointed.green_document(),
                    checkpoint_document.candidate_writer_test_arena(),
                )
                .unwrap(),
                serialized_green_test_logical_segments(
                    baseline.green_document(),
                    baseline_document.candidate_writer_test_arena(),
                )
                .unwrap(),
                "source {source:?}"
            );
            assert_eq!(
                serialized_green_test_open_facts(
                    checkpointed.green_document(),
                    checkpoint_document.candidate_writer_test_arena(),
                )
                .unwrap(),
                serialized_green_test_open_facts(
                    baseline.green_document(),
                    baseline_document.candidate_writer_test_arena(),
                )
                .unwrap(),
                "source {source:?}"
            );
            assert_eq!(
                serialized_green_test_close_facts(
                    checkpointed.green_document(),
                    checkpoint_document.candidate_writer_test_arena(),
                )
                .unwrap(),
                serialized_green_test_close_facts(
                    baseline.green_document(),
                    baseline_document.candidate_writer_test_arena(),
                )
                .unwrap(),
                "source {source:?}"
            );
            assert_eq!(checkpointed.source_metric(), baseline.source_metric());
            assert_eq!(
                checkpointed.composer_receipt().source_pieces_consumed,
                baseline.composer_receipt().source_pieces_consumed,
                "checkpoint barriers cannot invent source pieces for {source:?}"
            );
            assert_eq!(
                checkpointed.green_runs_acknowledged(),
                checkpointed.composer_receipt().projection_runs_sealed
            );
            assert_eq!(checkpoints, checkpoint_job.acknowledged_lines);
            assert!(checkpoints > 0, "fixture must cross a real checkpoint");
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One receipt test keeps the full cross-build proof chain visible.
    fn legacy_suffix_local_setext_after_unchanged_prefix_fails_without_checkpoint_coverage() {
        // The legacy in-memory route still proves that restart and replay reach
        // a semantically complete candidate, but it cannot publish that green
        // tree: unlike the parent-selected route, it has no authenticated
        // cumulative projection origin. Clean-green equivalence therefore
        // remains a HOLD on the parent-selected green suffix splice rather
        // than being preserved through a test-only manifest escape.
        const OLD: &str = "lead\nold\n===\n";
        const EDITED: &str = "lead\n\n===\n";

        let (mut document, checkpoint, _) = first_ready_line_boundary_checkpoint(OLD);
        assert_eq!(checkpoint.acknowledged_lines(), 1);
        let (donor_sample, restart_draft) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let retained_block = restart_draft.green().block();
        let retained_blocks = restart_draft
            .source()
            .retained_block_ids()
            .collect::<Vec<_>>();
        assert_eq!(retained_blocks.len(), 2);
        assert_eq!(retained_blocks.last().copied(), Some(retained_block));
        assert_eq!(restart_draft.source().retained_source_bytes_for_test(), 0);

        let mut old_job = checkpoint.resume(&mut document).unwrap();
        for _ in 0..200_000 {
            match old_job.poll(&mut document).unwrap() {
                ExactBlockJobProgress::Pending => {}
                ExactBlockJobProgress::Complete => break,
            }
        }
        assert!(
            old_job.built.is_some(),
            "old Setext fixture did not complete"
        );
        let old_document = old_job.take_built();
        let sealed = restart_draft
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();

        let mut index_builder = StorageOnlyCheckpointIndexBuilder::default();
        index_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(donor_sample))
            .unwrap();
        let (index, _) = index_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let donor = index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .expect("the joined 4/5 checkpoint is indexed");
        let joined = sealed.join_located_donor(donor).unwrap();

        let before_edit = document.source_descriptor();
        let edit = document.accept_edit(before_edit, 5..8, "").unwrap();
        assert_eq!(
            document.query_source().materialize_for_testing(),
            EDITED,
            "fixture edit must preserve the checkpoint prefix exactly"
        );
        assert!(edit.admission().queued.is_some());
        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();

        let mut activation = document
            .begin_in_memory_setext_activation(epoch, joined, &old_document, RETAINED_CONFIG)
            .unwrap();
        let mut ready = false;
        for _ in 0..200_000 {
            match document
                .poll_in_memory_setext_activation(epoch, &mut activation, 1)
                .unwrap()
            {
                InMemorySetextActivationProgress::Pending => {}
                InMemorySetextActivationProgress::Ready => {
                    ready = true;
                    break;
                }
            }
        }
        assert!(ready, "retained Setext activation did not converge");
        let ready = activation.take_ready().unwrap();
        let driver = document
            .activate_ready_in_memory_setext(epoch, ready)
            .unwrap();

        let mut capture =
            *ExactBlockJob::start_retained_setext_rejoin(epoch, driver, &mut document).unwrap();
        let checkpoint = loop {
            match capture.poll(&mut document).unwrap() {
                ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => break checkpoint,
            }
        };
        assert_eq!(
            checkpoint.acknowledged_lines(),
            1,
            "retained activation must cross a nonzero fresh same-build rejoin"
        );
        let mut restarted_job = checkpoint.resume(&mut document).unwrap();
        let error = loop {
            match restarted_job.poll(&mut document) {
                Ok(ExactBlockJobProgress::Pending) => {}
                Ok(ExactBlockJobProgress::Complete) => {
                    panic!(
                        "suffix-local Setext restart must not commit without cumulative coverage"
                    )
                }
                Err(error) => break error,
            }
        };
        assert!(matches!(
            error,
            ExactBlockJobError::Commit(CandidateWriterError::Projection(
                crate::SourceProjectionComposerError::MissingCheckpointCoverage
            ))
        ));
        let abort = restarted_job.cancel(&mut document).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "unchanged-prefix Setext missing checkpoint coverage",
        );

        index
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    fn legacy_suffix_local_setext_with_distinct_axes_fails_without_checkpoint_coverage() {
        // `parent_selected_convergence_keeps_byte_and_utf16_axes_distinct`
        // preserves the positive production-path evidence. This legacy route
        // must still reject publication when those coordinates are exact but
        // cumulative projection coverage is unauthenticated.
        const OLD: &str = "😀lead\nold\n===\n";
        const EDITED: &str = "😀lead\n\n===\n";

        let (mut document, old_document, joined, index, _retained_block) =
            joined_setext_fixture(OLD, 9);
        reject_joined_setext_restart_without_checkpoint_coverage(
            &mut document,
            &old_document,
            joined,
            9..12,
            "",
        );
        assert_eq!(document.query_source().materialize_for_testing(), EDITED);
        assert_ne!(
            EDITED.len(),
            EDITED.encode_utf16().count(),
            "the fixture must exercise genuinely different byte and UTF-16 axes"
        );

        index
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    fn changed_checkpoint_prefix_returns_zero_fallback_without_mutation_or_publication() {
        let (mut document, old_document, joined, index, _) =
            joined_setext_fixture("lead\nold\n===\n", 5);
        let before_edit = document.source_descriptor();
        document.accept_edit(before_edit, 0..0, "x").unwrap();
        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        let old_publication = document.latest_mechanism_binding_for_test();
        let mut activation = document
            .begin_in_memory_setext_activation(epoch, joined, &old_document, RETAINED_CONFIG)
            .unwrap();
        let failure = loop {
            match document.poll_in_memory_setext_activation(epoch, &mut activation, 1) {
                Ok(InMemorySetextActivationProgress::Pending) => {}
                Ok(InMemorySetextActivationProgress::Ready) => {
                    panic!("changed-prefix restart must not become ready")
                }
                Err(failure) => break failure,
            }
        };
        assert!(matches!(
            failure.error,
            InMemorySetextActivationError::ZeroFallback
        ));
        assert_eq!(failure.abort, None);
        assert_eq!(failure.cleanup_error, None);
        assert_eq!(document.candidate_epoch(), Some(epoch));
        assert_eq!(
            document.latest_mechanism_binding_for_test(),
            old_publication
        );
        assert_eq!(
            document
                .candidate_writer_test_arena()
                .build_journal_metrics(epoch.build_id())
                .unwrap()
                .live_owners,
            0,
            "coordinate fallback must precede every green journal mutation"
        );
        drop(activation);
        document.activate_candidate_source_ledger(epoch).unwrap();
        let abort = document.cancel_candidate(epoch).unwrap();
        assert!(document.poll_candidate_abort(abort, 0).unwrap().complete);
        assert!(
            !serialized_green_test_trace(
                old_document.green_document(),
                document.candidate_writer_test_arena(),
            )
            .unwrap()
            .is_empty()
        );

        index
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    fn in_memory_setext_restart_rejects_a_wrong_composite_axis_and_same_cut_wrong_donor() {
        const OLD: &str = "lead\nold\n===\n";

        let (mut document, checkpoint, _) = first_ready_line_boundary_checkpoint(OLD);
        let (real_sample, restart_draft) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let mut old_job = checkpoint.resume(&mut document).unwrap();
        for _ in 0..200_000 {
            if old_job.poll(&mut document).unwrap() == ExactBlockJobProgress::Complete {
                break;
            }
        }
        assert!(old_job.built.is_some());
        let old_document = old_job.take_built();
        let sealed = restart_draft
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();

        let mut real_builder = StorageOnlyCheckpointIndexBuilder::default();
        real_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(real_sample))
            .unwrap();
        let (real_index, _) = real_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let expected = real_index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap()
            .checkpoint_cut();
        assert_eq!(expected.source_bytes(), 5);
        assert_eq!(expected.source_utf16(), 5);
        assert_eq!(expected.physical_lines(), 1);

        // Preserve the source cut exactly and alter only a non-source output
        // axis. A lookup by source bytes still selects the recipe, so the
        // sealed join itself must enforce all five axes.
        let wrong_axis = RelativeCheckpointMeasure::new(
            expected.source_bytes(),
            expected.source_utf16(),
            expected.physical_lines(),
            expected.green_events() + 1,
            expected.projection_runs(),
        );
        let wrong_axis_sample =
            DonorCheckpointSampleDraft::try_new(wrong_axis, donor_capture_after_line("lead\n"))
                .unwrap();
        let mut wrong_axis_builder = StorageOnlyCheckpointIndexBuilder::default();
        wrong_axis_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                wrong_axis_sample,
            ))
            .unwrap();
        let (wrong_axis_index, _) = wrong_axis_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let wrong_axis_donor = wrong_axis_index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap();
        let wrong_axis_failure = sealed.join_located_donor(wrong_axis_donor).unwrap_err();
        assert_eq!(
            wrong_axis_failure.error,
            InMemorySetextDonorJoinError::WrongCheckpointCut
        );

        // Relabel a real but structurally different BlockQuote donor with the
        // exact expected measure. Five-axis equality now passes; the byte-for-
        // byte opaque donor witness must independently reject substitution.
        let wrong_donor_sample =
            DonorCheckpointSampleDraft::try_new(expected, donor_capture_after_line("> x\n"))
                .unwrap();
        let mut wrong_donor_builder = StorageOnlyCheckpointIndexBuilder::default();
        wrong_donor_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                wrong_donor_sample,
            ))
            .unwrap();
        let (wrong_donor_index, _) = wrong_donor_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let wrong_donor = wrong_donor_index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap();
        let wrong_donor_failure = wrong_axis_failure
            .restart
            .join_located_donor(wrong_donor)
            .unwrap_err();
        assert_eq!(
            wrong_donor_failure.error,
            InMemorySetextDonorJoinError::WrongDonorIdentity
        );

        for index in [real_index, wrong_axis_index, wrong_donor_index] {
            index
                .release_later(document.candidate_writer_test_arena_mut())
                .unwrap();
        }
    }

    #[test]
    fn in_memory_setext_activation_slot_is_exclusive_and_pristine_abandon_restores_fallback() {
        const OLD: &str = "lead\nold\n===\n";

        let (mut document, checkpoint, _) = first_ready_line_boundary_checkpoint(OLD);
        let (donor_sample, restart_one) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let (_duplicate_sample, restart_two) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let mut old_job = checkpoint.resume(&mut document).unwrap();
        for _ in 0..200_000 {
            if old_job.poll(&mut document).unwrap() == ExactBlockJobProgress::Complete {
                break;
            }
        }
        assert!(old_job.built.is_some());
        let old_document = old_job.take_built();
        let sealed_one = restart_one
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        let sealed_two = restart_two
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();

        let mut index_builder = StorageOnlyCheckpointIndexBuilder::default();
        index_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(donor_sample))
            .unwrap();
        let (index, _) = index_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let donor_one = index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap();
        let donor_two = index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap();
        let joined_one = sealed_one.join_located_donor(donor_one).unwrap();
        let joined_two = sealed_two.join_located_donor(donor_two).unwrap();

        let before_edit = document.source_descriptor();
        document.accept_edit(before_edit, 5..8, "").unwrap();
        document.promote_latest_parse().unwrap();
        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let job = document
            .begin_in_memory_setext_activation(epoch, joined_one, &old_document, RETAINED_CONFIG)
            .unwrap();
        assert!(matches!(
            document.begin_in_memory_setext_activation(
                epoch,
                joined_two,
                &old_document,
                RETAINED_CONFIG,
            ),
            Err(InMemorySetextActivationError::Writer(
                CandidateWriterError::Busy
            ))
        ));

        assert!(document.poll_candidate_byte(epoch).is_err());
        assert!(document.activate_candidate_source_ledger(epoch).is_err());
        assert!(document.begin_source_projection_composer(epoch).is_err());
        assert!(document.mint_block_permit(epoch).is_err());
        assert!(document.mint_coverage_permit(epoch).is_err());
        assert!(matches!(
            document.activate_candidate_writer(epoch, RETAINED_CONFIG),
            Err(CandidateWriterError::Busy)
        ));
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 0);

        assert_eq!(
            document
                .abandon_in_memory_setext_activation(epoch, job)
                .unwrap(),
            None,
            "a pristine activation has no arena journal to abort"
        );
        document.activate_candidate_source_ledger(epoch).unwrap();
        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "pristine Setext abandon");

        index
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Cancellation assertions intentionally span every journal transition.
    fn dirty_in_memory_setext_abandon_aborts_the_fresh_journal_and_preserves_old_publication() {
        const OLD: &str = "lead\nold\n===\n";

        let (mut document, checkpoint, _) = first_ready_line_boundary_checkpoint(OLD);
        let (donor_sample, restart_draft) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let mut old_job = checkpoint.resume(&mut document).unwrap();
        for _ in 0..200_000 {
            if old_job.poll(&mut document).unwrap() == ExactBlockJobProgress::Complete {
                break;
            }
        }
        assert!(old_job.built.is_some());
        let old_document = old_job.take_built();
        let sealed = restart_draft
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        let mut index_builder = StorageOnlyCheckpointIndexBuilder::default();
        index_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(donor_sample))
            .unwrap();
        let (index, _) = index_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let donor = index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap();
        let joined = sealed.join_located_donor(donor).unwrap();

        let before_edit = document.source_descriptor();
        document.accept_edit(before_edit, 5..8, "").unwrap();
        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        while document
            .candidate_writer_test_arena()
            .metrics()
            .pending_releases
            != 0
        {
            document.poll_reclaim(1).unwrap();
        }
        let baseline_nodes = document.candidate_writer_test_arena().metrics().live_nodes;
        let old_publication = document.latest_mechanism_binding_for_test();
        let mut activation = document
            .begin_in_memory_setext_activation(epoch, joined, &old_document, RETAINED_CONFIG)
            .unwrap();

        let mut observed_dirty_journal = false;
        for _ in 0..200_000 {
            document
                .poll_in_memory_setext_activation(epoch, &mut activation, 1)
                .unwrap();
            let journal = document
                .candidate_writer_test_arena()
                .build_journal_metrics(epoch.build_id())
                .unwrap();
            if journal.live_owners > 0 {
                observed_dirty_journal = true;
                break;
            }
        }
        assert!(
            observed_dirty_journal,
            "fixture must abandon only after the fresh build owns retained green pages"
        );
        let abort = document
            .abandon_in_memory_setext_activation(epoch, activation)
            .unwrap()
            .expect("dirty abandonment must return a fuelled build abort");
        assert_eq!(document.candidate_epoch(), None);
        assert_eq!(
            document.latest_mechanism_binding_for_test(),
            old_publication,
            "dirty cancellation must not publish the fresh mechanism root"
        );
        assert!(
            document
                .candidate_writer_test_arena()
                .build_journal_metrics(epoch.build_id())
                .unwrap()
                .live_owners
                > 0
        );

        let mut complete = false;
        for _ in 0..10_000 {
            let receipt = document.poll_candidate_abort(abort, 1).unwrap();
            assert!(receipt.owners_scheduled <= 1);
            if receipt.complete {
                complete = true;
                break;
            }
        }
        assert!(complete, "unit-fuel dirty abort did not converge");
        while document
            .candidate_writer_test_arena()
            .metrics()
            .pending_releases
            != 0
        {
            document.poll_reclaim(1).unwrap();
        }
        assert_eq!(
            document.candidate_writer_test_arena().metrics().live_nodes,
            baseline_nodes,
            "dirty restart pages must reclaim back to the old-document baseline"
        );
        assert!(
            !serialized_green_test_trace(
                old_document.green_document(),
                document.candidate_writer_test_arena(),
            )
            .unwrap()
            .is_empty(),
            "the caller-borrowed old document remains decodable after fresh-build abort"
        );

        index
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Failure receipt and reclamation are verified in one provenance scenario.
    fn post_green_activation_failure_aborts_instead_of_reusing_the_dirty_candidate() {
        const OLD: &str = "lead\nold\n===\n";

        let (mut document, checkpoint, _) = first_ready_line_boundary_checkpoint(OLD);
        let (donor_sample, restart_draft) = checkpoint
            .capture_in_memory_setext_checkpoint(&document)
            .unwrap()
            .split();
        let mut old_job = checkpoint.resume(&mut document).unwrap();
        for _ in 0..200_000 {
            if old_job.poll(&mut document).unwrap() == ExactBlockJobProgress::Complete {
                break;
            }
        }
        assert!(old_job.built.is_some());
        let old_document = old_job.take_built();
        let sealed = restart_draft
            .seal_against_old_document(
                document.candidate_writer_test_arena(),
                &old_document,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        let mut index_builder = StorageOnlyCheckpointIndexBuilder::default();
        index_builder
            .push(StorageOnlyCheckpointPartition::donor_direct(donor_sample))
            .unwrap();
        let (index, _) = index_builder
            .commit(document.candidate_writer_test_arena_mut())
            .unwrap();
        let donor = index
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 5)
            .unwrap()
            .unwrap();
        let joined = sealed.join_located_donor(donor).unwrap();

        let before_edit = document.source_descriptor();
        document.accept_edit(before_edit, 5..8, "").unwrap();
        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        while document
            .candidate_writer_test_arena()
            .metrics()
            .pending_releases
            != 0
        {
            document.poll_reclaim(1).unwrap();
        }
        let baseline_nodes = document.candidate_writer_test_arena().metrics().live_nodes;
        let old_publication = document.latest_mechanism_binding_for_test();
        let mut activation = document
            .begin_in_memory_setext_activation(epoch, joined, &old_document, RETAINED_CONFIG)
            .unwrap();
        for _ in 0..200_000 {
            if document
                .poll_in_memory_setext_activation(epoch, &mut activation, 1)
                .unwrap()
                == InMemorySetextActivationProgress::Ready
            {
                break;
            }
        }
        assert!(activation.is_ready());
        let mut ready = activation.take_ready().unwrap();
        ready.corrupt_old_binding_for_test();
        let Err(failure) = document.activate_ready_in_memory_setext(epoch, ready) else {
            panic!("post-green provenance corruption must fail closed");
        };
        assert!(matches!(
            failure.error,
            InMemorySetextActivationError::Writer(CandidateWriterError::WrongCandidate)
        ));
        assert_eq!(failure.cleanup_error, None);
        let abort = failure
            .abort
            .expect("post-green failure must abort the whole fresh build");
        assert_eq!(document.candidate_epoch(), None);
        assert_eq!(
            document.latest_mechanism_binding_for_test(),
            old_publication
        );

        let mut complete = false;
        for _ in 0..10_000 {
            let receipt = document.poll_candidate_abort(abort, 1).unwrap();
            assert!(receipt.owners_scheduled <= 1);
            if receipt.complete {
                complete = true;
                break;
            }
        }
        assert!(complete);
        while document
            .candidate_writer_test_arena()
            .metrics()
            .pending_releases
            != 0
        {
            document.poll_reclaim(1).unwrap();
        }
        assert_eq!(
            document.candidate_writer_test_arena().metrics().live_nodes,
            baseline_nodes
        );
        assert!(
            !serialized_green_test_trace(
                old_document.green_document(),
                document.candidate_writer_test_arena(),
            )
            .unwrap()
            .is_empty()
        );

        index
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    fn actor_derived_first_and_successive_samples_cover_lf_crlf_and_emoji() {
        for (source, expected_bytes, expected_utf16) in [
            ("a\nb\n", 2_u64, 2_u64),
            ("a\r\nb\r\n", 3, 3),
            ("😀\n😀\n", 5, 3),
        ] {
            let (_document, samples, cursor) = capture_sparse_donor_samples(source, 1);
            assert_eq!(samples.len(), 2);
            assert_eq!(cursor.sample_ordinal_for_test(), 2);
            for sample in &samples {
                let interval = *sample;
                assert_eq!(interval.source_bytes(), expected_bytes);
                assert_eq!(interval.source_utf16(), expected_utf16);
                assert_eq!(interval.physical_lines(), 1);
            }
            assert_eq!(
                summed_sample_intervals(&samples),
                cursor.cumulative_cut_for_test(),
                "all five relative axes must sum to the actor-owned final cut for {source:?}",
            );
        }
    }

    #[test]
    fn donor_sample_cursor_rejects_wrong_build_and_wrong_source_without_consumption() {
        const SAME: &str = "a\nb\n";
        let (mut first_document, first_checkpoint, _) = first_ready_line_boundary_checkpoint(SAME);
        let first_capture = first_checkpoint
            .capture_first_donor_checkpoint_sample(&mut first_document)
            .unwrap();
        let (_first_sample, first_cursor) = first_capture.into_parts();
        let duplicate = first_checkpoint
            .capture_successive_donor_checkpoint_sample(&mut first_document, first_cursor)
            .unwrap_err();
        assert!(matches!(
            duplicate.error,
            ExactBlockJobError::Writer(CandidateWriterError::Invariant(
                "donor checkpoint cut did not advance"
            ))
        ));
        let first_cursor = duplicate.cursor;

        let old_epoch = first_cursor.epoch_for_test();
        let abort = first_checkpoint.cancel(&mut first_document).unwrap();
        let mut abort_complete = false;
        for _ in 0..1_000 {
            if first_document
                .poll_candidate_abort(abort, 1)
                .unwrap()
                .complete
            {
                abort_complete = true;
                break;
            }
        }
        assert!(abort_complete);
        assert_eq!(first_document.candidate_epoch(), None);
        let token = first_document.active_parse_plan().unwrap().token;
        let other_epoch = first_document.begin_candidate(token).unwrap();
        first_document
            .activate_candidate_source_ledger(other_epoch)
            .unwrap();
        first_document
            .activate_candidate_writer(other_epoch, CONFIG)
            .unwrap();
        let mut other_job = ExactBlockJob::new(other_epoch).unwrap();
        for _ in 0..100_000 {
            assert_eq!(
                other_job.poll(&mut first_document).unwrap(),
                ExactBlockJobProgress::Pending
            );
            if other_job.is_line_boundary_checkpoint_seam() {
                break;
            }
        }
        let other_build_checkpoint = ready_checkpoint_from_job(&mut first_document, other_job);
        let other_build_capture = other_build_checkpoint
            .capture_first_donor_checkpoint_sample(&mut first_document)
            .unwrap();
        let (_other_sample, other_build_cursor) = other_build_capture.into_parts();
        assert_eq!(
            old_epoch.source(),
            other_build_cursor.epoch_for_test().source()
        );
        assert_ne!(
            old_epoch.build_id(),
            other_build_cursor.epoch_for_test().build_id(),
        );
        let wrong_build = other_build_checkpoint
            .capture_successive_donor_checkpoint_sample(&mut first_document, first_cursor)
            .unwrap_err();
        assert!(matches!(
            wrong_build.error,
            ExactBlockJobError::Writer(CandidateWriterError::WrongCandidate)
        ));
        let first_cursor = wrong_build.cursor;

        let (mut other_source_document, other_source_checkpoint, _) =
            first_ready_line_boundary_checkpoint("longer\nsource\n");
        let other_source_capture = other_source_checkpoint
            .capture_first_donor_checkpoint_sample(&mut other_source_document)
            .unwrap();
        let (_other_source_sample, _other_source_cursor) = other_source_capture.into_parts();
        assert_ne!(
            first_cursor.epoch_for_test().source(),
            other_source_checkpoint.epoch.source(),
        );
        let wrong_source = other_source_checkpoint
            .capture_successive_donor_checkpoint_sample(&mut other_source_document, first_cursor)
            .unwrap_err();
        assert!(matches!(
            wrong_source.error,
            ExactBlockJobError::Writer(CandidateWriterError::WrongCandidate)
        ));
    }

    #[test]
    fn sparse_chain_accumulates_640_four_line_samples_with_exact_five_axis_sum() {
        const SAMPLE_COUNT: usize = 640;
        const CADENCE: u64 = 4;
        const TEN_MIB: usize = 10 * 1024 * 1024;
        let line = format!("{}\n", "x".repeat(4095));
        let source = line.repeat(SAMPLE_COUNT * usize::try_from(CADENCE).unwrap());
        assert_eq!(source.len(), TEN_MIB);
        let (_document, samples, cursor) = capture_sparse_donor_samples(&source, CADENCE);
        assert_eq!(samples.len(), SAMPLE_COUNT);
        assert_eq!(cursor.sample_ordinal_for_test(), SAMPLE_COUNT as u64);
        let summed = summed_sample_intervals(&samples);
        assert_eq!(summed, cursor.cumulative_cut_for_test());
        assert_eq!(summed.source_bytes(), source.len() as u64);
        assert_eq!(summed.source_utf16(), source.len() as u64);
        assert_eq!(summed.physical_lines(), SAMPLE_COUNT as u64 * CADENCE);

        let terminal_sample_line = u64::try_from(SAMPLE_COUNT)
            .unwrap()
            .checked_mul(CADENCE)
            .unwrap();
        let (document, _job) =
            drive_restart_parent(&source, terminal_sample_line, CADENCE, false).unwrap();
        let (_, composite, _) = document.latest_restart_view_for_test().unwrap();
        let receipt = composite.checkpoint_index;
        assert_eq!(receipt.donor_sample_headers, SAMPLE_COUNT);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert!(receipt.donor_retained_payload_bytes < 128 * 1024);
        assert!(receipt.maximum_donor_partition_draft_bytes < 1024 * 1024);
    }

    #[test]
    fn live_actor_restart_parent_commits_at_eof_and_returns_the_existing_allocator() {
        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let (epoch, receipt, view) = document
            .latest_restart_view_for_test()
            .expect("v2 commit stores one live-actor parent");
        assert_eq!(epoch.source().bytes, 11);
        assert_eq!(receipt.manifest_nodes_allocated, 1);
        assert_eq!(view.green().coverage_count(), 3);
        assert_eq!(view.checkpoint_index().physical_lines(), 2);
        assert!(view.checkpoint_index().has_terminal_tail());

        let parent = document.source_descriptor();
        document
            .accept_edit(parent, parent.bytes..parent.bytes, "X")
            .expect("a published parent needs a real next edit before another candidate");
        let token = document.active_parse_plan().unwrap().token;
        let next = document.begin_candidate(token).unwrap();
        let next_block = document.mint_block_permit(next).unwrap();
        let next_coverage = document.mint_coverage_permit(next).unwrap();
        assert_eq!(next_block.id(), crate::BlockId(3));
        assert_eq!(next_coverage.id(), crate::CoverageId(4));
        let abort = document.cancel_candidate(next).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "post-v2 success allocator");
    }

    #[test]
    fn live_actor_parent_drives_nonzero_persisted_source_resume_without_forged_epoch() {
        use crate::live_document::persisted_restart_activation::PersistedRestartActivationProgress;

        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document
            .accept_edit(parent, 7..7, "X")
            .expect("local suffix edit publishes one current source");
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 6, RETAINED_CONFIG)
            .expect("parent selects the first physical-line checkpoint");
        let ready = loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .expect("unchanged prefix and exact donor remain valid")
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("edit after the first checkpoint must preserve nonzero restart")
                }
                PersistedRestartActivationProgress::Ready(receipt) => break receipt,
            }
        };
        let source = ready.source;
        let reconstruction = ready.reconstruction;
        assert_eq!(
            source.deferred_lf_bytes_examined, 2,
            "bare-LF validation reads the terminator and its CR predecessor guard"
        );
        assert_eq!(source.retained_old_source_roots, 0);
        assert_eq!(source.retained_old_source_bytes, 0);
        assert_eq!(source.retained_old_writer_drafts, 0);
        assert_eq!(reconstruction.path_frames_consumed, 2);
        assert_eq!(reconstruction.logical_metrics_consumed, 2);
        assert_eq!(reconstruction.suffix_local_tab_baseline, 0);
        assert_eq!(reconstruction.suffix_local_nul_baseline, 0);
        assert_eq!(reconstruction.suffix_local_line_ending_baseline, 0);
        assert_eq!(reconstruction.suffix_local_claim_baseline, 0);
        assert_eq!(reconstruction.retained_old_source_bytes, 0);
        assert_eq!(reconstruction.retained_old_writer_drafts, 0);
        assert_eq!(reconstruction.parallel_restart_paths, 0);

        document
            .abandon_persisted_restart_activation(activation)
            .expect("read-only source phase can fall back without burning the candidate");
        document
            .activate_candidate_source_ledger(epoch)
            .expect("fallback still owns the untouched byte-zero source cursor");
        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "persisted source phase fallback",
        );
    }

    #[test]
    fn actor_owned_ready_restart_is_dropped_by_latest_wins_edit_cancellation() {
        use crate::live_document::persisted_restart_activation::{
            PersistedRestartActivationError, PersistedRestartActivationProgress,
        };

        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document.accept_edit(parent, 7..7, "X").unwrap();
        while let Some(retired) = document.take_retired_source_root() {
            drop(retired);
        }
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 6, RETAINED_CONFIG)
            .unwrap();
        loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::Ready(_) => break,
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("unchanged prefix unexpectedly fell back to zero")
                }
            }
        }
        assert_eq!(
            document.candidate_cursor_offset(epoch).unwrap(),
            6,
            "ready diagnostics report physical restart P, not the untouched fallback cursor"
        );

        let current = document.source_descriptor();
        let edit = document.accept_edit(current, 8..8, "Y").unwrap();
        let abort = edit
            .cancelled()
            .expect("latest-wins edit cancels the actor-owned ready phase");
        assert_eq!(document.candidate_epoch(), None);
        assert!(matches!(
            document.poll_persisted_restart_activation(activation, 1),
            Err(PersistedRestartActivationError::Actor(
                crate::LiveDocumentError::NoCandidate
            ))
        ));
        document
            .promote_latest_parse()
            .expect("the edit that cancelled the restart becomes the next active parse plan");
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "actor-owned persisted source cancellation",
        );
        assert_eq!(document.retired_source_root_count(), 1);
    }

    #[test]
    fn live_actor_ready_restart_brands_exact_parent_children_before_any_replay() {
        use crate::live_document::persisted_restart_activation::{
            PersistedRestartActivationError, PersistedRestartActivationProgress,
        };

        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document.accept_edit(parent, 7..7, "X").unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 6, RETAINED_CONFIG)
            .unwrap();
        let ready = loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::Ready(receipt) => break receipt,
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("unchanged prefix unexpectedly fell back to zero")
                }
            }
        };

        let selected = document
            .select_persisted_restart_parent_for_adoption(activation)
            .expect("the source-selected parent brands its own retained children");
        assert_eq!(selected.ready, ready);
        assert_eq!(selected.cursor_offset, 6);
        assert_eq!(selected.retained_child_owners, 2);
        assert_eq!(selected.parent_manifest_validations, 1);
        assert_eq!(selected.source_bytes_copied, 0);
        assert_eq!(selected.parent_pages_copied, 0);
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 6);
        assert!(matches!(
            document.abandon_persisted_restart_activation(activation),
            Err(PersistedRestartActivationError::WrongPhase)
        ));

        let abort = document.cancel_candidate(epoch).unwrap();
        let zero = document.poll_candidate_abort(abort, 0).unwrap();
        assert_eq!(zero.owners_scheduled, 0);
        assert!(!zero.complete);
        let first = document.poll_candidate_abort(abort, 1).unwrap();
        assert_eq!(first.owners_scheduled, 1);
        assert!(!first.complete);
        let second = document.poll_candidate_abort(abort, 1).unwrap();
        assert_eq!(second.owners_scheduled, 1);
        assert!(second.complete);

        let next = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .expect("whole-journal cancellation leaves the actor reusable");
        let _identity = document.mint_block_permit(next).unwrap();
        let next_abort = document.cancel_candidate(next).unwrap();
        assert!(
            document
                .poll_candidate_abort(next_abort, 0)
                .unwrap()
                .complete
        );
    }

    #[test]
    fn live_actor_installs_nonzero_direct_writer_without_exposing_linear_state() {
        use crate::live_document::persisted_restart_activation::{
            PersistedRestartActivationError, PersistedRestartActivationProgress,
            PersistedRestartWriterProgress,
        };

        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent_output = match document.restart_composite_publication_state().unwrap() {
            crate::live_document::RestartCompositePublicationState::Published {
                output, ..
            } => output,
            state => panic!("restart fixture did not publish its parent: {state:?}"),
        };
        let parent = document.source_descriptor();
        document.accept_edit(parent, 7..7, "X").unwrap();
        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 6, RETAINED_CONFIG)
            .unwrap();
        loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::Ready(_) => break,
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("unchanged first line must preserve the nonzero checkpoint")
                }
            }
        }
        let selected = document
            .select_persisted_restart_parent_for_adoption(activation)
            .unwrap();

        let installed = loop {
            match document
                .poll_persisted_candidate_writer_restart(activation)
                .unwrap()
            {
                PersistedRestartWriterProgress::Pending => {}
                PersistedRestartWriterProgress::Installed(receipt) => break receipt,
            }
        };
        assert_eq!(installed.parent, selected);
        assert_eq!(installed.cursor_offset, 6);
        assert_eq!(installed.open_bindings, 2);
        assert_eq!(installed.acknowledged_lines, 1);
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 6);
        assert!(!document.candidate_writer_is_poisoned(epoch).unwrap());
        assert_eq!(
            document
                .poll_persisted_candidate_writer_restart(activation)
                .unwrap(),
            PersistedRestartWriterProgress::Installed(installed),
            "the scheduler can observe completion repeatedly but cannot take the driver"
        );
        let mut saw_rejoin = false;
        let mut saw_mapping = false;
        let mut saw_running = false;
        let mut saw_capture = false;
        for _ in 0..200_000 {
            match document.poll_persisted_exact_restart(activation).unwrap() {
                ParentSelectedExactBlockDriverProgress::Rejoining => saw_rejoin = true,
                ParentSelectedExactBlockDriverProgress::Mapping => saw_mapping = true,
                ParentSelectedExactBlockDriverProgress::Running => saw_running = true,
                ParentSelectedExactBlockDriverProgress::ConvergenceCapture => saw_capture = true,
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired => break,
                ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired => {
                    panic!("unchanged retained tail unexpectedly required full replay")
                }
            }
        }
        assert!(
            saw_rejoin,
            "nonzero suffix must pass the mandatory same-build rejoin"
        );
        assert!(
            saw_mapping,
            "old semantic C must map to an authenticated current physical boundary"
        );
        assert!(
            saw_running,
            "the real grammar must drive the changed suffix"
        );
        assert!(
            saw_capture,
            "the live parser must pause and capture at mapped C"
        );
        assert_eq!(
            document.poll_persisted_exact_restart(activation).unwrap(),
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired,
            "source/composer adoption parks before the green/index/global join"
        );
        let (old_c, old_ordinal, boundary) = document
            .persisted_old_convergence_for_test(activation)
            .expect("the actor retains the old convergence probe beside the exact driver");
        let old_c = old_c.expect("the second retained line is old C after nonzero R");
        assert_eq!(old_ordinal, Some(1));
        assert_eq!(old_c.source_bytes(), 11);
        assert_eq!(old_c.source_utf16(), 11);
        assert_eq!(old_c.physical_lines(), 2);
        assert_eq!(boundary, None);
        let (certificate_epoch, interval, cumulative_cut, sample_ordinal) = document
            .persisted_matched_live_sample_certificate_for_test(activation)
            .unwrap()
            .expect("the terminal convergence state retains its matched live sample");
        assert_eq!(certificate_epoch, epoch);
        assert_eq!(interval, RelativeCheckpointMeasure::new(6, 6, 1, 1, 1));
        assert_eq!(
            cumulative_cut,
            RelativeCheckpointMeasure::new(12, 12, 2, 4, 2)
        );
        assert_eq!(sample_ordinal, 1);
        assert_eq!(
            document.candidate_cursor_offset(epoch).unwrap(),
            document.source_descriptor().bytes,
            "the suffix driver reaches the exact current-source EOF"
        );
        assert!(matches!(
            document.abandon_persisted_restart_activation(activation),
            Err(PersistedRestartActivationError::WrongPhase)
        ));

        let mut adoption_polls = 0_usize;
        loop {
            adoption_polls += 1;
            assert!(
                adoption_polls < 100_000,
                "bounded green/checkpoint adoption splice did not converge"
            );
            match document
                .poll_persisted_adoption_splice(activation)
                .expect("matched C must enter the actor-owned adoption splice")
            {
                crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                    panic!("fully preflighted parent join unexpectedly retried: {error:?}")
                }
                crate::ParentSelectedAdoptionSpliceProgress::Complete => break,
            }
        }
        assert!(
            adoption_polls > 3,
            "green and checkpoint persistent jobs must remain cooperatively pollable"
        );
        let splice = document
            .persisted_adoption_splice_receipt_for_test(activation)
            .unwrap();
        assert_eq!(splice.parent_join_attempts, 1);
        assert_eq!(
            adoption_polls,
            2 + splice.green_polls + splice.checkpoint_polls,
            "each actor call performs one checkpoint start, child transition, or parent join"
        );
        assert_eq!(splice.writer.final_projection_runs, 3);
        let green = splice.green.expect("completed splice records green work");
        assert!(green.current_prefix_leaves > 0);
        assert!(green.old_suffix_leaves > 0);
        assert!(green.build.sequence_leaves_reused > 0);
        assert_eq!(green.retained_source_bytes, 0);
        assert_eq!(green.document_sized_event_vectors, 0);
        assert_eq!(green.source_tail.retained_source_bytes, 0);
        assert_eq!(green.source_tail.document_sized_event_vectors, 0);
        #[cfg(feature = "host-mirror-probe")]
        {
            let (common_prefix, old_changed, new_changed, common_suffix) = document
                .persisted_adoption_host_splice_range_counts_for_test(activation)
                .expect("completed matched-C adoption exposes its final host proof state")
                .expect("Direct restart must mint final host publication authority");
            assert!(
                common_prefix > 0,
                "canonical Direct R must remain a shared prefix"
            );
            assert!(common_suffix > 0, "matched C must remain a shared suffix");
            assert!(old_changed > 0, "the old R-to-C interval is replaced");
            assert!(new_changed > 0, "the current R-to-C interval is inserted");
        }
        let checkpoint = splice
            .checkpoint
            .expect("completed splice records checkpoint work");
        assert!(checkpoint.old_samples_replaced > 0);
        assert!(checkpoint.fresh_samples_inserted > 0);
        assert_eq!(checkpoint.admission_fresh_samples_scanned, 0);
        assert_eq!(checkpoint.admission_fresh_samples_requeued, 0);
        assert_eq!(checkpoint.admission_fresh_chain_capacity_allocated, 0);
        assert!(checkpoint.boundary_leaf_pages_decoded <= 2);
        assert!(checkpoint.suffix_relevant_continuation_identity);
        assert_eq!(checkpoint.retained_source_bytes, 0);

        let published = document
            .commit_persisted_adoption_splice(activation)
            .expect("completed adoption splice must commit through one parent owner");
        let publication = match published {
            crate::live_document::RestartCompositeCommitProgress::Published {
                receipt,
                publication,
            } => {
                assert_eq!(receipt.manifest_nodes_allocated, 1);
                publication
            }
            crate::live_document::RestartCompositeCommitProgress::Held { error, .. } => {
                panic!("fully validated replacement publication was held: {error:?}")
            }
        };
        assert_eq!(publication.base_output, parent_output);
        assert_ne!(publication.offered_output, parent_output);
        assert_eq!(document.candidate_epoch(), None);
        match document.restart_composite_publication_state().unwrap() {
            crate::live_document::RestartCompositePublicationState::Published {
                epoch: published_epoch,
                output,
                ..
            } => {
                assert_eq!(published_epoch, epoch);
                assert_eq!(output, publication.offered_output);
            }
            state => panic!("replacement parent is not worker-current: {state:?}"),
        }
    }

    #[cfg(feature = "host-mirror-probe")]
    #[test]
    fn real_matched_direct_proof_applies_nonzero_delta_to_host_mirror() {
        use crate::SourceRevision;
        use crate::host_mirror::{
            DocumentSessionId, HostMirror, HostQuery, HostRevisionId, MetricRange,
            PublicationSessionId, SourceLineageEdit, SourceVersion, source_digest_for_test,
        };

        const OLD: &str = "alpha\nbeta\n";
        const TARGET: &str = "alpha\nbXeta\n";
        let metric = |text: &str| SerializedMetric {
            bytes: u64::try_from(text.len()).unwrap(),
            utf16: u64::try_from(text.encode_utf16().count()).unwrap(),
        };
        let document_session = DocumentSessionId([0x91; 16]);
        let publication_session = PublicationSessionId([0x92; 16]);
        let (mut document, _job) = drive_restart_parent(OLD, 2, 1, false).unwrap();
        let old_descriptor = document.source_descriptor();
        assert_eq!(
            old_descriptor.revision,
            SourceRevision(0),
            "the initial actor snapshot exercises valid source revision zero"
        );
        let old_source = SourceVersion {
            document_session,
            revision: old_descriptor.revision,
            metric: metric(OLD),
            hash: source_digest_for_test(OLD),
        };
        let snapshot = document
            .prepare_current_restart_host_snapshot_bundle(
                publication_session,
                HostRevisionId(1),
                old_source,
            )
            .unwrap();
        let mut host = HostMirror::new(old_source);
        let base_ack = host.apply_bundle(snapshot).unwrap();
        host.acknowledge_delivery(base_ack).unwrap();

        document.accept_edit(old_descriptor, 7..7, "X").unwrap();
        let target_descriptor = document.source_descriptor();
        let target_source = SourceVersion {
            document_session,
            revision: target_descriptor.revision,
            metric: metric(TARGET),
            hash: source_digest_for_test(TARGET),
        };
        host.observe_source_edit(
            target_source,
            vec![SourceLineageEdit {
                base: MetricRange {
                    start: SerializedMetric { bytes: 7, utf16: 7 },
                    end: SerializedMetric { bytes: 7, utf16: 7 },
                },
                target: MetricRange {
                    start: SerializedMetric { bytes: 7, utf16: 7 },
                    end: SerializedMetric { bytes: 8, utf16: 8 },
                },
            }],
        )
        .unwrap();
        assert!(matches!(
            host.query_metric(SerializedMetric::default()).unwrap(),
            HostQuery::SourceGap(_)
        ));

        let (_epoch, activation) = install_parent_selected_restart(&mut document, 6);
        assert_eq!(
            finish_parent_selected_convergence(&mut document, activation),
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
        );
        loop {
            match document.poll_persisted_adoption_splice(activation).unwrap() {
                crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                    panic!("preflighted matched-C parent join retried: {error:?}")
                }
                crate::ParentSelectedAdoptionSpliceProgress::Complete => break,
            }
        }

        let delta = document
            .prepare_persisted_adoption_host_delta_bundle(
                activation,
                base_ack,
                HostRevisionId(2),
                publication_session,
                target_source,
            )
            .expect("the exact actor proof must prepare the host delta in place");
        assert!(
            delta.splice.old_start > 0,
            "Direct R is a real common prefix"
        );
        assert!(
            delta.splice.old_delete > 0,
            "old R-to-C leaves are replaced"
        );
        assert!(
            !delta.splice.inserted.is_empty(),
            "current R-to-C leaves are copied"
        );
        let target_ack = host.apply_bundle(delta).unwrap();
        host.acknowledge_delivery(target_ack).unwrap();
        assert_eq!(
            target_ack.structural_source_revision,
            target_descriptor.revision
        );
        assert!(matches!(
            host.query_metric(SerializedMetric::default()).unwrap(),
            HostQuery::Structural(_)
        ));

        document
            .commit_persisted_adoption_splice(activation)
            .expect("host-copied proof leaves the worker parent committable");
    }

    #[cfg(feature = "host-mirror-probe")]
    #[test]
    fn real_matched_setext_normalization_applies_canonical_retained_prefix_delta() {
        use crate::SourceRevision;
        use crate::host_mirror::{
            DocumentSessionId, HostMirror, HostQuery, HostRevisionId, MetricRange,
            PublicationSessionId, SourceLineageEdit, SourceVersion, source_digest_for_test,
        };

        // The production-like edit hint selects R=15, the source-complete
        // Setext line boundary after the underline LF. The edit is in the
        // adjacent Direct partition, so an honest matched C proves both
        // partitions can be retained/spliced as one actor job.
        const OLD: &str = "lead\ntitle\n---\ntail\nend\nlast\nmore\n";
        const TARGET: &str = "lead\ntitle\n---\ntAil\nend\nlast\nmore\n";
        let metric = |text: &str| SerializedMetric {
            bytes: u64::try_from(text.len()).unwrap(),
            utf16: u64::try_from(text.encode_utf16().count()).unwrap(),
        };
        let document_session = DocumentSessionId([0xa1; 16]);
        let publication_session = PublicationSessionId([0xa2; 16]);
        let (mut document, _job) = drive_restart_parent(OLD, 7, 1, false).unwrap();
        let old_descriptor = document.source_descriptor();
        assert_eq!(old_descriptor.revision, SourceRevision(0));
        let old_source = SourceVersion {
            document_session,
            revision: old_descriptor.revision,
            metric: metric(OLD),
            hash: source_digest_for_test(OLD),
        };
        let snapshot = document
            .prepare_current_restart_host_snapshot_bundle(
                publication_session,
                HostRevisionId(1),
                old_source,
            )
            .unwrap();
        let mut host = HostMirror::new(old_source);
        let base_ack = host.apply_bundle(snapshot).unwrap();
        host.acknowledge_delivery(base_ack).unwrap();

        document.accept_edit(old_descriptor, 16..17, "A").unwrap();
        let target_descriptor = document.source_descriptor();
        let target_source = SourceVersion {
            document_session,
            revision: target_descriptor.revision,
            metric: metric(TARGET),
            hash: source_digest_for_test(TARGET),
        };
        host.observe_source_edit(
            target_source,
            vec![SourceLineageEdit {
                base: MetricRange {
                    start: SerializedMetric {
                        bytes: 16,
                        utf16: 16,
                    },
                    end: SerializedMetric {
                        bytes: 17,
                        utf16: 17,
                    },
                },
                target: MetricRange {
                    start: SerializedMetric {
                        bytes: 16,
                        utf16: 16,
                    },
                    end: SerializedMetric {
                        bytes: 17,
                        utf16: 17,
                    },
                },
            }],
        )
        .unwrap();

        let (epoch, activation) = install_parent_selected_restart(&mut document, 16);
        assert_eq!(
            document.candidate_cursor_offset(epoch).unwrap(),
            15,
            "the edit hint must select the final source-complete Setext checkpoint"
        );
        let terminal = loop {
            let progress = document.poll_persisted_exact_restart(activation).unwrap();
            if matches!(
                progress,
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
                    | ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired
            ) {
                break progress;
            }
        };
        assert_eq!(
            terminal,
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
        );
        loop {
            match document.poll_persisted_adoption_splice(activation).unwrap() {
                crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                    panic!("preflighted Setext matched-C parent join retried: {error:?}")
                }
                crate::ParentSelectedAdoptionSpliceProgress::Complete => break,
            }
        }

        let (common_prefix, old_changed, new_changed, common_suffix) = document
            .persisted_adoption_host_splice_range_counts_for_test(activation)
            .unwrap()
            .expect("Setext normalization must mint generic host authority");
        assert!(
            common_prefix > 0,
            "Setext must retain the distant prefix before its rewritten leaf"
        );
        assert!(old_changed > 0);
        assert!(new_changed > 0);
        assert!(common_suffix > 0);

        let delta = document
            .prepare_persisted_adoption_host_delta_bundle(
                activation,
                base_ack,
                HostRevisionId(2),
                publication_session,
                target_source,
            )
            .expect("post-normalization generic proof must prepare an exact host delta");
        assert!(delta.splice.old_start > 0);
        assert!(delta.splice.old_delete > 0);
        assert!(!delta.splice.inserted.is_empty());
        let target_ack = host.apply_bundle(delta).unwrap();
        host.acknowledge_delivery(target_ack).unwrap();
        assert!(matches!(
            host.query_metric(SerializedMetric::default()).unwrap(),
            HostQuery::Structural(_)
        ));

        document
            .commit_persisted_adoption_splice(activation)
            .expect("Setext host delta leaves the worker parent committable");
    }

    #[cfg(feature = "host-mirror-probe")]
    #[test]
    fn direct_restart_later_setext_promotion_monotonically_caps_host_prefix() {
        const OLD: &str = "title\nbody\ntail\n";
        let (mut document, _job) = drive_restart_parent(OLD, 3, 1, false).unwrap();
        let parent = document.source_descriptor();
        document.accept_edit(parent, 6..10, "---").unwrap();
        let (_epoch, activation) = install_parent_selected_restart(&mut document, 6);
        let terminal = loop {
            let progress = document.poll_persisted_exact_restart(activation).unwrap();
            if matches!(
                progress,
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
                    | ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired
            ) {
                break progress;
            }
        };
        assert_eq!(
            terminal,
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
        );
        loop {
            match document.poll_persisted_adoption_splice(activation).unwrap() {
                crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                    panic!("preflighted Direct-to-Setext parent join retried: {error:?}")
                }
                crate::ParentSelectedAdoptionSpliceProgress::Complete => break,
            }
        }

        let (common_prefix, old_changed, new_changed, common_suffix) = document
            .persisted_adoption_host_splice_range_counts_for_test(activation)
            .unwrap()
            .expect("post-restart Setext promotion must retain generic host authority");
        assert_eq!(
            common_prefix, 0,
            "later normalization must shrink an earlier Direct seed instead of invalidating it"
        );
        assert!(old_changed > 0);
        assert!(new_changed > 0);
        assert!(common_suffix > 0);

        document
            .commit_persisted_adoption_splice(activation)
            .expect("post-restart Setext normalization remains committable");
    }

    #[cfg(feature = "host-mirror-probe")]
    #[test]
    fn parent_selected_whole_setext_before_atx_restores_retired_paragraph_identity() {
        const OLD: &str = "title\nbody\n# next\nsame\n";
        const TARGET: &str = "title\n---\n# next\nsame\n";
        let (mut document, _job) = drive_restart_parent(OLD, 4, 1, false).unwrap();
        let old_trace = {
            let (_, _, view) = document
                .latest_restart_view_for_test()
                .expect("the old parent is published");
            serialized_green_test_composite_canonical_trace(
                view.green().descriptor_for_test(),
                document.candidate_writer_test_arena(),
            )
            .unwrap()
        };
        let retired_title = old_trace
            .iter()
            .find_map(|event| match event {
                SerializedGreenTestCanonicalEvent::Enter {
                    block,
                    kind: GreenKind::PARAGRAPH,
                } => Some(*block),
                SerializedGreenTestCanonicalEvent::Enter { .. }
                | SerializedGreenTestCanonicalEvent::Coverage { .. }
                | SerializedGreenTestCanonicalEvent::Exit => None,
            })
            .expect("the old title begins as a Paragraph");

        let parent = document.source_descriptor();
        document.accept_edit(parent, 6..10, "---").unwrap();
        assert_eq!(document.query_source().materialize_for_testing(), TARGET);
        let (_epoch, activation) = install_parent_selected_restart(&mut document, 6);
        assert_eq!(
            finish_parent_selected_convergence(&mut document, activation),
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
        );
        loop {
            match document.poll_persisted_adoption_splice(activation).unwrap() {
                crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                    panic!("preflighted whole-Setext parent join retried: {error:?}")
                }
                crate::ParentSelectedAdoptionSpliceProgress::Complete => break,
            }
        }
        let (_, _, _, common_suffix) = document
            .persisted_adoption_host_splice_range_counts_for_test(activation)
            .unwrap()
            .expect("whole Setext retains generic host authority");
        assert!(common_suffix > 0);
        document
            .commit_persisted_adoption_splice(activation)
            .expect("whole Setext before an ATX sibling remains committable");

        let incremental_trace = {
            let (_, _, view) = document
                .latest_restart_view_for_test()
                .expect("the incremental target is published");
            serialized_green_test_composite_canonical_trace(
                view.green().descriptor_for_test(),
                document.candidate_writer_test_arena(),
            )
            .unwrap()
        };
        let heading_blocks = incremental_trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestCanonicalEvent::Enter {
                    block,
                    kind: GreenKind::HEADING,
                } => Some(*block),
                SerializedGreenTestCanonicalEvent::Enter { .. }
                | SerializedGreenTestCanonicalEvent::Coverage { .. }
                | SerializedGreenTestCanonicalEvent::Exit => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(heading_blocks.first(), Some(&retired_title));
        assert!(
            !incremental_trace.iter().any(|event| matches!(
                event,
                SerializedGreenTestCanonicalEvent::Enter {
                    block,
                    kind: GreenKind::PARAGRAPH,
                } if *block == retired_title
            )),
            "whole Setext must not leave or reopen the retired Paragraph wrapper"
        );

        let clean_document = drive_restart_parent(TARGET, 4, 1, false).unwrap().0;
        let clean_trace = {
            let (_, _, view) = clean_document
                .latest_restart_view_for_test()
                .expect("the clean target is published");
            serialized_green_test_composite_canonical_trace(
                view.green().descriptor_for_test(),
                clean_document.candidate_writer_test_arena(),
            )
            .unwrap()
        };
        assert_canonical_green_semantics_equal_without_history_ids(
            &incremental_trace,
            &clean_trace,
        );
    }

    #[test]
    fn parent_selected_live_donor_mismatch_cannot_mint_a_certificate() {
        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document.accept_edit(parent, 6..10, "> beta").unwrap();
        let (epoch, activation) = install_parent_selected_restart(&mut document, 6);

        for _ in 0..400_000 {
            match document.poll_persisted_exact_restart(activation).unwrap() {
                ParentSelectedExactBlockDriverProgress::Rejoining
                | ParentSelectedExactBlockDriverProgress::Mapping
                | ParentSelectedExactBlockDriverProgress::Running
                | ParentSelectedExactBlockDriverProgress::ConvergenceCapture => {
                    assert_eq!(
                        document
                            .persisted_matched_live_sample_certificate_for_test(activation)
                            .unwrap(),
                        None,
                        "only a successful donor witness match may mint the certificate"
                    );
                }
                ParentSelectedExactBlockDriverProgress::FullSuffixReplacementRequired => break,
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired => {
                    panic!("a changed donor grammar must not mint a convergence certificate")
                }
            }
        }
        assert_eq!(
            document
                .persisted_matched_live_sample_certificate_for_test(activation)
                .unwrap(),
            None,
            "mismatch reaches the full-suffix terminal state without a certificate"
        );

        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "mismatched parent-selected live donor",
        );
    }

    #[test]
    fn actor_owned_adoption_splice_is_cancellable_after_every_poll_boundary() {
        let mut boundaries_covered = 0_usize;
        for polls_before_cancel in 1..=256_usize {
            let (mut document, epoch, activation) = matched_direct_adoption_splice_fixture();
            let mut complete = false;
            for _ in 0..polls_before_cancel {
                match document.poll_persisted_adoption_splice(activation).unwrap() {
                    crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                    crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                        panic!("preflighted parent join unexpectedly retried: {error:?}")
                    }
                    crate::ParentSelectedAdoptionSpliceProgress::Complete => {
                        complete = true;
                        break;
                    }
                }
            }
            boundaries_covered += 1;
            let receipt = document
                .persisted_adoption_splice_receipt_for_test(activation)
                .unwrap();
            let abort = document.cancel_candidate(epoch).unwrap();
            drain_abort_and_prove_fresh_candidate(
                &mut document,
                abort,
                "integrated adoption splice poll boundary",
            );
            if complete {
                assert_eq!(
                    polls_before_cancel,
                    2 + receipt.green_polls + receipt.checkpoint_polls,
                    "the terminal boundary accounts for every bounded child transition"
                );
                break;
            }
        }
        assert!(
            boundaries_covered > 3,
            "cancellation must cross multiple green/checkpoint/parent phases"
        );
    }

    #[test]
    fn parent_selected_convergence_maps_semantic_a_to_physical_p_for_all_line_endings() {
        for (ending, adjacent_bytes, source_queries, program_pages, decoder_lookahead) in
            [("\n", 1, 5, 0, 0), ("\r", 2, 6, 1, 1), ("\r\n", 2, 6, 1, 0)]
        {
            // Keep R on the persisted slice's independently tested bare-LF
            // bridge. This matrix varies C: restart-line-ending admission is a
            // separate capability from semantic-A/physical-P convergence.
            let old = format!("lead\nbody{ending}tail{ending}");
            let mut document = drive_restart_parent(&old, 3, 1, false).unwrap().0;
            let restart = "lead\n".len();
            let old_semantic_c = restart + "body".len();
            let old_physical_c = old_semantic_c + ending.len();
            let parent = document.source_descriptor();
            document
                .accept_edit(parent, restart..old_semantic_c, "changed")
                .unwrap();
            let current_semantic_c = restart + "changed".len();
            let current_physical_c = current_semantic_c + ending.len();
            let (epoch, activation) = install_parent_selected_restart(&mut document, restart);
            let (target, receipt) =
                drive_parent_selected_to_mapped_capture(&mut document, activation);
            let selected_diagnostic = document
                .persisted_old_convergence_for_test(activation)
                .unwrap();

            assert_eq!(
                target.source_bytes(),
                current_physical_c as u64,
                "ending={ending:?}; selected={selected_diagnostic:?}; receipt={receipt:?}"
            );
            assert_eq!(target.source_utf16(), current_physical_c as u64);
            assert_eq!(target.physical_lines(), 2);
            assert_eq!(
                document.candidate_cursor_offset(epoch).unwrap(),
                current_physical_c + decoder_lookahead,
                "a lone-CR decoder may hold one replay byte beyond emitted P"
            );
            let current = document.query_source().materialize_for_testing();
            assert_eq!(&current[current_physical_c..], format!("tail{ending}"));

            let (old_c, old_ordinal, boundary) = document
                .persisted_old_convergence_for_test(activation)
                .unwrap();
            let old_c = old_c.unwrap();
            assert_eq!(old_ordinal, Some(1));
            assert_eq!(old_c.source_bytes(), old_physical_c as u64);
            assert_eq!(old_c.physical_lines(), 2);
            assert_eq!(boundary, None);

            assert_eq!(
                receipt.green_source_tail.described_suffix_bytes,
                (old.len() - old_semantic_c) as u64
            );
            assert!(
                receipt.green_source_tail.source_adjacent_bytes_read >= adjacent_bytes
                    && receipt.green_source_tail.source_adjacent_bytes_read <= 6,
                "terminator plus two line-index queries must read only constant adjacent bytes"
            );
            assert_eq!(receipt.green_source_tail.source_queries, source_queries);
            assert_eq!(
                receipt
                    .green_source_tail
                    .green_projection_program_pages_validated,
                program_pages
            );
            assert!(
                receipt
                    .green_source_tail
                    .green_projection_program_bytes_validated
                    <= crate::PROJECTION_PROGRAM_PAGE_BYTES
            );
            assert_eq!(
                receipt
                    .green_source_tail
                    .green_projection_prefix_pieces_decoded,
                program_pages
            );
            assert_eq!(receipt.lineage.poll_records_examined, 1);
            assert_eq!(receipt.lineage.poll_mapping_attempts, 4);
            assert_eq!(receipt.lineage.poll_mappings_succeeded, 4);
            assert_eq!(receipt.lineage.records_copied, 0);
            assert_eq!(receipt.green_source_tail.retained_source_roots, 0);
            assert_eq!(receipt.green_source_tail.retained_source_bytes, 0);
            assert_eq!(receipt.green_source_tail.document_sized_event_vectors, 0);
            assert_eq!(
                finish_parent_selected_convergence(&mut document, activation),
                ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
            );
            assert_eq!(
                document.candidate_cursor_offset(epoch).unwrap(),
                current_physical_c,
                "tail adoption seals the emitted boundary, not decoder lookahead"
            );

            let abort = document.cancel_candidate(epoch).unwrap();
            drain_abort_and_prove_fresh_candidate(
                &mut document,
                abort,
                "line-ending convergence matrix",
            );
        }
    }

    #[test]
    fn parent_selected_convergence_keeps_byte_and_utf16_axes_distinct() {
        const OLD: &str = "😀lead\nbody\ntail\n";
        let mut document = drive_restart_parent(OLD, 3, 1, false).unwrap().0;
        let restart = "😀lead\n".len();
        assert_eq!(restart, 9);
        let parent = document.source_descriptor();
        document
            .accept_edit(parent, restart..restart, "é😀")
            .unwrap();
        let (epoch, activation) = install_parent_selected_restart(&mut document, restart);
        let (target, _) = drive_parent_selected_to_mapped_capture(&mut document, activation);
        let selected_diagnostic = document
            .persisted_old_convergence_for_test(activation)
            .unwrap();

        assert_eq!(
            target.source_bytes(),
            20,
            "selected={selected_diagnostic:?}"
        );
        assert_eq!(target.source_utf16(), 15);
        assert_eq!(target.physical_lines(), 2);
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 20);
        assert_eq!(
            &document.query_source().materialize_for_testing()[20..],
            "tail\n"
        );
        assert_eq!(
            finish_parent_selected_convergence(&mut document, activation),
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
        );

        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "UTF-8 convergence axes");
    }

    #[test]
    fn changed_first_old_c_advances_to_the_next_authenticated_successor() {
        const OLD: &str = "lead\nbody\nthird\ntail\n";
        let mut document = drive_restart_parent(OLD, 4, 1, false).unwrap().0;
        let parent = document.source_descriptor();
        document.accept_edit(parent, 10..15, "changed").unwrap();
        let (epoch, activation) = install_parent_selected_restart(&mut document, 5);
        let (target, receipt) = drive_parent_selected_to_mapped_capture(&mut document, activation);

        assert_eq!(target.source_bytes(), 18);
        assert_eq!(target.physical_lines(), 3);
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 18);
        assert_eq!(
            &document.query_source().materialize_for_testing()[18..],
            "tail\n"
        );
        let (old_c, old_ordinal, boundary) = document
            .persisted_old_convergence_for_test(activation)
            .unwrap();
        assert_eq!(old_ordinal, Some(2));
        assert_eq!(old_c.unwrap().source_bytes(), 16);
        assert_eq!(boundary, None);
        assert_eq!(receipt.lineage.poll_records_examined, 1);
        assert_eq!(receipt.lineage.poll_mappings_succeeded, 4);
        assert_eq!(
            finish_parent_selected_convergence(&mut document, activation),
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired
        );

        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "changed first old C successor",
        );
    }

    #[test]
    fn semantic_a_equal_to_restart_is_legal_and_blank_gap_advances_to_clean_convergence() {
        const OLD: &str = "lead\n\nbody\n";
        const TARGET: &str = "lead\n \nbody\n";
        let mut document = drive_restart_parent(OLD, 3, 1, false).unwrap().0;
        let parent = document.source_descriptor();
        document.accept_edit(parent, 5..5, " ").unwrap();
        let (epoch, activation) = install_parent_selected_restart(&mut document, 5);
        let (target, _) = drive_parent_selected_to_mapped_capture(&mut document, activation);

        // The first old C has semantic A == R. That interval is legal, so the
        // mapper reaches the tail check rather than aborting on its scalar
        // ordering. Its first retained run is an intentionally unsupported
        // blank GAP, so the driver safely advances to the next authenticated C.
        assert_eq!(target.source_bytes(), 12);
        assert_eq!(target.physical_lines(), 3);
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 12);
        let (old_c, old_ordinal, boundary) = document
            .persisted_old_convergence_for_test(activation)
            .unwrap();
        assert_eq!(old_ordinal, Some(2));
        assert_eq!(old_c.unwrap().source_bytes(), 11);
        assert_eq!(boundary, None);
        assert_eq!(
            finish_parent_selected_convergence(&mut document, activation),
            ParentSelectedExactBlockDriverProgress::ConvergenceJoinRequired,
            "the next authenticated source-complete C has matching donor and green authority"
        );
        loop {
            match document.poll_persisted_adoption_splice(activation).unwrap() {
                crate::ParentSelectedAdoptionSpliceProgress::Pending => {}
                crate::ParentSelectedAdoptionSpliceProgress::ParentJoinRetryable(error) => {
                    panic!("preflighted source-complete blank-gap join retried: {error:?}")
                }
                crate::ParentSelectedAdoptionSpliceProgress::Complete => break,
            }
        }
        document
            .commit_persisted_adoption_splice(activation)
            .expect("source-complete blank-gap convergence must publish");
        assert_eq!(document.candidate_epoch(), None);

        let incremental_trace = {
            let (_, _, view) = document
                .latest_restart_view_for_test()
                .expect("incremental blank-gap result is published");
            serialized_green_test_composite_canonical_trace(
                view.green().descriptor_for_test(),
                document.candidate_writer_test_arena(),
            )
            .unwrap()
        };
        let clean_document = drive_restart_parent(TARGET, 3, 1, false).unwrap().0;
        let clean_trace = {
            let (_, _, view) = clean_document
                .latest_restart_view_for_test()
                .expect("clean blank-gap target is published");
            serialized_green_test_composite_canonical_trace(
                view.green().descriptor_for_test(),
                clean_document.candidate_writer_test_arena(),
            )
            .unwrap()
        };
        let block_ids = |trace: &[SerializedGreenTestCanonicalEvent]| {
            trace
                .iter()
                .filter_map(|event| match event {
                    SerializedGreenTestCanonicalEvent::Enter { block, .. } => Some(*block),
                    SerializedGreenTestCanonicalEvent::Coverage { .. }
                    | SerializedGreenTestCanonicalEvent::Exit => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            block_ids(&incremental_trace),
            [BlockId(1), BlockId(2), BlockId(4)]
        );
        assert_eq!(
            block_ids(&clean_trace),
            [BlockId(1), BlockId(2), BlockId(3)]
        );
        assert_canonical_green_semantics_equal_without_history_ids(
            &incremental_trace,
            &clean_trace,
        );
    }

    #[test]
    fn live_actor_setext_restart_inverts_only_the_selected_heading_before_install() {
        use crate::live_document::persisted_restart_activation::{
            PersistedRestartActivationProgress, PersistedRestartWriterProgress,
        };

        let (mut document, _job) = drive_restart_parent("title\n---\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document
            .accept_edit(parent, 8..8, "-")
            .expect("the underline edit preserves the first physical line");
        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 6, RETAINED_CONFIG)
            .unwrap();
        loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::Ready(_) => break,
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("unchanged title line must preserve the Setext group checkpoint")
                }
            }
        }
        document
            .select_persisted_restart_parent_for_adoption(activation)
            .unwrap();
        let installed = loop {
            match document
                .poll_persisted_candidate_writer_restart(activation)
                .unwrap()
            {
                PersistedRestartWriterProgress::Pending => {}
                PersistedRestartWriterProgress::Installed(receipt) => break receipt,
            }
        };
        assert_eq!(installed.cursor_offset, 6);
        assert_eq!(installed.open_bindings, 2);
        assert_eq!(installed.acknowledged_lines, 1);
        assert_eq!(installed.green.inverse_leaf_pages_allocated, 1);
        assert!(installed.green.maximum_restored_open_depth >= 2);
        assert_eq!(installed.green.source_payload_bytes_materialized, 0);
        assert_eq!(installed.green.document_sized_event_vectors_materialized, 0);
        assert!(!document.candidate_writer_is_poisoned(epoch).unwrap());

        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "actor-owned nonzero Setext writer install",
        );
    }

    #[test]
    fn equal_cut_cross_parent_is_rejected_at_the_source_adoption_join() {
        use crate::live_document::persisted_restart_activation::{
            PersistedRestartActivationError, PersistedRestartActivationProgress,
        };

        let (mut document, _job) = drive_restart_parent("a\nb\n", 2, 1, false).unwrap();
        let selected_cut = {
            let (_, _, view) = document.latest_restart_view_for_test().unwrap();
            view.checkpoint_index()
                .parent()
                .locate_donor_checkpoint_at_or_before_cut(
                    document.restart_publication_test_coordinator(),
                    document.candidate_writer_test_arena(),
                    2,
                )
                .unwrap()
                .unwrap()
                .checkpoint_cut()
        };
        assert_eq!(selected_cut, RelativeCheckpointMeasure::new(2, 2, 1, 3, 1));

        let parent_source = document.source_descriptor();
        document.accept_edit(parent_source, 3..3, "X").unwrap();
        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 2, RETAINED_CONFIG)
            .unwrap();
        loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::Ready(_) => break,
                PersistedRestartActivationProgress::ZeroFallback => {
                    panic!("edit after the equal checkpoint cut must preserve restart authority")
                }
            }
        }

        let crossed_parent =
            crate::storage_only_composite_document::tests::commit_lf_restart_parent(
                document.candidate_writer_test_arena_mut(),
                None,
                None,
            );
        let crossed_cut = crossed_parent
            .locate_donor_checkpoint_at_or_before_cut(document.candidate_writer_test_arena(), 2)
            .unwrap()
            .unwrap()
            .checkpoint_cut();
        assert_eq!(crossed_cut, selected_cut);

        let error = document
            .select_crossed_persisted_restart_parent_for_test(activation, &crossed_parent)
            .unwrap_err();
        let PersistedRestartActivationError::ParentSelectionAborted {
            error,
            cleanup_error,
            abort,
        } = error
        else {
            panic!("equal-cut crossed parent must fail at the source/adoption join")
        };
        assert!(matches!(
            error,
            crate::PersistedSourceAdoptionJoinError::Parent(
                crate::storage_only_composite_document::RestartCompositeDocumentError::Invalid(
                    "restart selection, donor anchor, and retained parent activation differ"
                )
            )
        ));
        assert_eq!(cleanup_error, None);
        assert_eq!(document.candidate_epoch(), None);

        let zero = document.poll_candidate_abort(abort, 0).unwrap();
        assert_eq!(zero.owners_scheduled, 0);
        assert!(!zero.complete);
        let first = document.poll_candidate_abort(abort, 1).unwrap();
        assert_eq!(first.owners_scheduled, 1);
        assert!(!first.complete);
        let second = document.poll_candidate_abort(abort, 1).unwrap();
        assert_eq!(second.owners_scheduled, 1);
        assert!(second.complete);

        assert!(document.latest_restart_view_for_test().is_some());
        assert!(
            crossed_parent
                .view(document.candidate_writer_test_arena())
                .is_ok()
        );
        let next = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .expect("cross-parent rejection leaves the actor reusable");
        let _identity = document.mint_block_permit(next).unwrap();
        let next_abort = document.cancel_candidate(next).unwrap();
        assert!(
            document
                .poll_candidate_abort(next_abort, 0)
                .unwrap()
                .complete
        );

        crossed_parent
            .release_later(document.candidate_writer_test_arena_mut())
            .unwrap();
    }

    #[test]
    fn actor_persisted_changed_prefix_returns_zero_without_consuming_fallback_cursor() {
        use crate::live_document::persisted_restart_activation::PersistedRestartActivationProgress;

        let (mut document, _job) = drive_restart_parent("alpha\nbeta\n", 2, 1, false).unwrap();
        let parent = document.source_descriptor();
        document.accept_edit(parent, 0..0, "X").unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        let activation = document
            .begin_persisted_restart_activation(epoch, 6, RETAINED_CONFIG)
            .unwrap();
        loop {
            match document
                .poll_persisted_restart_activation(activation, 1)
                .unwrap()
            {
                PersistedRestartActivationProgress::Pending { .. } => {}
                PersistedRestartActivationProgress::ZeroFallback => break,
                PersistedRestartActivationProgress::Ready(_) => {
                    panic!("changed prefix must not preserve the nonzero checkpoint")
                }
            }
        }
        assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), 0);
        document.activate_candidate_source_ledger(epoch).unwrap();
        let abort = document.cancel_candidate(epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut document,
            abort,
            "actor zero fallback remains byte-zero ready",
        );
    }

    #[test]
    fn pre_eof_restart_chain_fails_before_adoption_and_restores_the_existing_allocator() {
        let Err(failure) = drive_restart_parent("alpha\nbeta\n", 1, 1, false) else {
            panic!("pre-EOF chain must not commit a restart parent");
        };
        let (mut document, job, error) = *failure;
        assert!(matches!(
            error,
            ExactBlockJobError::Commit(CandidateWriterError::Invariant(
                "final donor sample is not at source EOF"
            ))
        ));
        assert!(document.latest_restart_view_for_test().is_none());
        let abort = job.cancel(&mut document).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "pre-EOF v2 rejection");

        let token = document.active_parse_plan().unwrap().token;
        let next = document.begin_candidate(token).unwrap();
        assert_eq!(
            document.mint_block_permit(next).unwrap().id(),
            crate::BlockId(4)
        );
        assert_eq!(
            document.mint_coverage_permit(next).unwrap().id(),
            crate::CoverageId(4)
        );
        let abort = document.cancel_candidate(next).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "post-v2 failure allocator");
    }

    #[test]
    fn forged_restart_chain_fails_before_session_adoption_and_preserves_burned_ids() {
        let Err(failure) = drive_restart_parent("alpha\nbeta\n", 2, 1, true) else {
            panic!("forged actor-chain ordinal must not commit a restart parent");
        };
        let (mut document, job, error) = *failure;
        assert!(matches!(
            error,
            ExactBlockJobError::Commit(CandidateWriterError::Invariant(
                "restart sample chain and writer accumulator disagree"
            ))
        ));
        assert!(document.latest_restart_view_for_test().is_none());
        let abort = job.cancel(&mut document).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "forged v2 chain");

        let token = document.active_parse_plan().unwrap().token;
        let next = document.begin_candidate(token).unwrap();
        assert_eq!(
            document.mint_block_permit(next).unwrap().id(),
            crate::BlockId(4)
        );
        assert_eq!(
            document.mint_coverage_permit(next).unwrap().id(),
            crate::CoverageId(4)
        );
        let abort = document.cancel_candidate(next).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut document, abort, "post-forge allocator");
    }

    #[test]
    fn crossed_actor_sample_is_returned_and_cannot_enter_another_chain() {
        let (mut first_document, first_checkpoint, _) =
            first_ready_line_boundary_checkpoint("a\nb\n");
        let first_capture = first_checkpoint
            .capture_first_donor_checkpoint_sample(&mut first_document)
            .unwrap();
        let (mut first_chain, first_cursor) = first_capture
            .try_start_restart_chain()
            .unwrap_or_else(|failure| panic!("first chain failed: {:?}", failure.error));

        let (mut second_document, second_checkpoint, _) =
            first_ready_line_boundary_checkpoint("x\ny\n");
        let second_capture = second_checkpoint
            .capture_first_donor_checkpoint_sample(&mut second_document)
            .unwrap();
        let (mut second_chain, _second_cursor) = second_capture
            .try_start_restart_chain()
            .unwrap_or_else(|failure| panic!("second chain failed: {:?}", failure.error));

        let mut first_job = first_checkpoint.resume(&mut first_document).unwrap();
        for _ in 0..100_000 {
            assert_eq!(
                first_job.poll(&mut first_document).unwrap(),
                ExactBlockJobProgress::Pending
            );
            if first_job.is_line_boundary_checkpoint_seam() {
                break;
            }
        }
        let second_line = ready_checkpoint_from_job(&mut first_document, first_job);
        let first_successive = second_line
            .capture_successive_donor_checkpoint_sample(&mut first_document, first_cursor)
            .unwrap();
        let crossed = second_chain.try_append(first_successive).unwrap_err();
        assert!(matches!(
            crossed.error,
            CandidateWriterError::Invariant(
                "restart sample chain capture is crossed or noncontiguous"
            )
        ));
        let _final_cursor = first_chain
            .try_append(crossed.capture)
            .unwrap_or_else(|failure| panic!("returned capture was damaged: {:?}", failure.error));
        let mut first_job = second_line.resume(&mut first_document).unwrap();
        first_job
            .install_restart_sample_chain(first_chain)
            .unwrap_or_else(|_| panic!("fresh job accepts returned first chain"));
        let mut committed = false;
        for _ in 0..100_000 {
            if first_job.poll(&mut first_document).unwrap() == ExactBlockJobProgress::Complete {
                assert!(first_document.latest_restart_view_for_test().is_some());
                committed = true;
                break;
            }
        }
        assert!(committed, "returned first chain must still commit");

        drop(second_chain);
        let abort = second_checkpoint.cancel(&mut second_document).unwrap();
        drain_abort_and_prove_fresh_candidate(&mut second_document, abort, "crossed second chain");
    }

    #[test]
    fn empty_document_has_no_actor_minted_v2_sample_chain_yet() {
        let (mut document, mut job) = document("");
        for _ in 0..100_000 {
            assert!(!job.is_line_boundary_checkpoint_seam());
            if job.poll(&mut document).unwrap() == ExactBlockJobProgress::Complete {
                assert!(document.latest_restart_view_for_test().is_none());
                return;
            }
        }
        panic!("empty-document local fallback did not converge");
    }

    #[test]
    fn joined_line_boundary_checkpoint_is_actor_owned_and_cancellable() {
        let (mut document, mut job) = document("alpha\nbeta");
        for _ in 0..100_000 {
            assert_eq!(
                job.poll(&mut document).unwrap(),
                ExactBlockJobProgress::Pending
            );
            if job.is_line_boundary_checkpoint_seam() {
                break;
            }
        }
        assert!(job.is_line_boundary_checkpoint_seam());
        let mut capture = match job.start_line_boundary_checkpoint(&mut document).unwrap() {
            ExactBlockCheckpointAdmission::Started(capture) => *capture,
            ExactBlockCheckpointAdmission::Skipped { reason, .. } => {
                panic!("eligible checkpoint skipped: {reason:?}")
            }
        };
        let checkpoint = loop {
            match capture.poll(&mut document).unwrap() {
                ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => break checkpoint,
            }
        };
        let abort = checkpoint.cancel(&mut document).unwrap();
        assert_eq!(document.candidate_epoch(), None);
        for _ in 0..1_000 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                let token = document.active_parse_plan().unwrap().token;
                let next = document.begin_candidate(token).unwrap();
                let _identity = document.mint_block_permit(next).unwrap();
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
        panic!("paused checkpoint candidate did not complete fuelled abort");
    }

    #[test]
    fn composite_checkpoint_rejects_a_parser_from_another_candidate_epoch() {
        let (mut document, job) = first_line_boundary_job("alpha\nrest");
        let epoch = job.epoch;
        let (mut foreign_document, mut foreign_job) = first_line_boundary_job("alpha\nrest");
        assert_ne!(
            foreign_job.epoch, epoch,
            "independent candidates must carry distinct source/arena authority"
        );
        ready_actor_writer_checkpoint(&mut document, epoch);

        let foreign_epoch = foreign_job.epoch;
        let parser = take_parser_authority(&mut foreign_job, foreign_epoch);
        reject_crossed_checkpoint_and_prove_actor_cancel(
            &mut document,
            epoch,
            parser,
            &job.bindings,
            CandidateWriterError::WrongCandidate,
            "foreign parser epoch",
        );

        let foreign_abort = foreign_document.cancel_candidate(foreign_epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut foreign_document,
            foreign_abort,
            "foreign parser fixture",
        );
    }

    #[test]
    fn composite_checkpoint_rejects_bindings_from_another_arena_build() {
        let (mut document, mut job) = first_line_boundary_job("alpha\nrest");
        let epoch = job.epoch;
        let (mut foreign_document, foreign_job) = first_line_boundary_job("alpha\nrest");
        assert_eq!(
            foreign_job.bindings.len(),
            job.bindings.len(),
            "fixture paths must have the same shape before build identity is checked"
        );
        ready_actor_writer_checkpoint(&mut document, epoch);

        let parser = take_parser_authority(&mut job, epoch);
        reject_crossed_checkpoint_and_prove_actor_cancel(
            &mut document,
            epoch,
            parser,
            &foreign_job.bindings,
            CandidateWriterError::Invariant("parser, source ledger, and writer bindings disagree"),
            "foreign binding build",
        );

        let foreign_abort = foreign_job.cancel(&mut foreign_document).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut foreign_document,
            foreign_abort,
            "foreign binding fixture",
        );
    }

    #[test]
    fn composite_checkpoint_rejects_a_different_real_open_path() {
        let (mut document, job) = first_line_boundary_job("alpha\nrest");
        let epoch = job.epoch;
        let (mut foreign_document, mut foreign_job) = first_line_boundary_job("> abc\nrest");
        assert_ne!(
            foreign_job.bindings.len(),
            job.bindings.len(),
            "quoted fixture must carry a genuinely different open path"
        );
        ready_actor_writer_checkpoint(&mut document, epoch);

        // The parser pause is real and internally valid. Binding it to the
        // target epoch makes the composite join, rather than the epoch guard,
        // prove that its open stack cannot cross into this writer.
        let parser = take_parser_authority(&mut foreign_job, epoch);
        reject_crossed_checkpoint_and_prove_actor_cancel(
            &mut document,
            epoch,
            parser,
            &job.bindings,
            CandidateWriterError::Invariant(
                "parser and writer checkpoint cursor/path shapes disagree",
            ),
            "foreign parser open path",
        );

        let foreign_epoch = foreign_job.epoch;
        let foreign_abort = foreign_document.cancel_candidate(foreign_epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut foreign_document,
            foreign_abort,
            "foreign open-path fixture",
        );
    }

    #[test]
    fn composite_checkpoint_rejects_a_different_deferred_role_at_the_same_cut_shape() {
        let (mut document, job) = first_line_boundary_job("alpha\nrest");
        let epoch = job.epoch;
        let (mut foreign_document, mut foreign_job) = first_line_boundary_job("alpha");
        assert_eq!(
            foreign_job.bindings.len(),
            job.bindings.len(),
            "both fixtures must retain Document/Paragraph"
        );
        assert_eq!(
            foreign_job
                .parser
                .as_ref()
                .expect("fixture parser is live")
                .capture_line_boundary_pause()
                .expect("fixture parser pauses")
                .pairing_view()
                .last_line_length(),
            5,
            "bare-EOF fixture must match alpha-newline's content column"
        );
        ready_actor_writer_checkpoint(&mut document, epoch);

        // Bare-EOF `alpha` has the same line ordinal, content column, and open
        // Document/Paragraph path as `alpha\n`; only the latter owns a pending
        // paragraph terminator. Absolute source authority remains entirely on
        // the target writer side of the join.
        let parser = take_parser_authority(&mut foreign_job, epoch);
        assert_eq!(parser.deferred_role(), DirectLineBoundaryDeferredRole::None);
        reject_crossed_checkpoint_and_prove_actor_cancel(
            &mut document,
            epoch,
            parser,
            &job.bindings,
            CandidateWriterError::Invariant("parser and source deferred checkpoint roles disagree"),
            "foreign deferred role",
        );

        let foreign_epoch = foreign_job.epoch;
        let foreign_abort = foreign_document.cancel_candidate(foreign_epoch).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut foreign_document,
            foreign_abort,
            "foreign deferred-role fixture",
        );
    }

    #[test]
    fn line_boundary_checkpoint_is_cancellable_at_every_external_capture_boundary() {
        const SOURCE: &str = "> - alpha\n>   continuation\ntail";

        let (mut probe_document, mut probe_capture) =
            first_line_boundary_checkpoint_capture(SOURCE);
        let mut pending_boundaries = 0_usize;
        let ready = loop {
            match probe_capture.poll(&mut probe_document).unwrap() {
                ExactBlockCheckpointCapturePoll::Pending(next) => {
                    pending_boundaries = pending_boundaries
                        .checked_add(1)
                        .expect("checkpoint poll count fits usize");
                    probe_capture = next;
                }
                ExactBlockCheckpointCapturePoll::Ready(checkpoint) => break checkpoint,
            }
        };
        assert!(
            pending_boundaries >= 3,
            "fixture must expose drain and green-cut capture phases"
        );
        let abort = ready.cancel(&mut probe_document).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut probe_document,
            abort,
            "Ready/actor-paused checkpoint",
        );

        for pending_polls_before_cancel in 0..=pending_boundaries {
            let (mut document, mut capture) = first_line_boundary_checkpoint_capture(SOURCE);
            for poll_index in 0..pending_polls_before_cancel {
                capture = match capture.poll(&mut document).unwrap() {
                    ExactBlockCheckpointCapturePoll::Pending(next) => next,
                    ExactBlockCheckpointCapturePoll::Ready(_) => panic!(
                        "probe promised Pending poll {poll_index} before boundary \
                         {pending_polls_before_cancel}"
                    ),
                };
            }
            let abort = capture.cancel(&mut document).unwrap();
            drain_abort_and_prove_fresh_candidate(
                &mut document,
                abort,
                &format!("after {pending_polls_before_cancel} Pending capture polls"),
            );
        }
    }

    #[test]
    fn composite_checkpoint_retention_is_independent_of_a_ten_megabyte_suffix() {
        let short_source = "x\ntail";
        let mut large_source = String::with_capacity(2 + 10 * 1024 * 1024);
        large_source.push_str("x\n");
        large_source.push_str(&"z".repeat(10 * 1024 * 1024));

        let (mut short_document, short_checkpoint, short_polls) =
            first_ready_line_boundary_checkpoint(short_source);
        let (mut large_document, large_checkpoint, large_polls) =
            first_ready_line_boundary_checkpoint(&large_source);

        let short_parser = short_checkpoint.parser_retention_for_test();
        let large_parser = large_checkpoint.parser_retention_for_test();
        let short_writer = short_document
            .candidate_writer_checkpoint_retention_for_test(short_checkpoint.epoch)
            .unwrap();
        let large_writer = large_document
            .candidate_writer_checkpoint_retention_for_test(large_checkpoint.epoch)
            .unwrap();
        let short_arena = short_document.candidate_writer_test_arena().metrics();
        let large_arena = large_document.candidate_writer_test_arena().metrics();

        assert_eq!(large_parser, short_parser);
        assert_eq!(large_parser.retained_source_bytes, 0);
        assert_eq!(large_writer, short_writer);
        assert_eq!(large_writer.0, 0, "writer checkpoint owns no source bytes");
        assert_eq!(large_writer.2, 2, "Document + Paragraph are the open path");
        assert_eq!(large_polls, short_polls);
        assert_eq!(large_arena.live_nodes, short_arena.live_nodes);
        assert_eq!(
            large_arena.live_payload_bytes,
            short_arena.live_payload_bytes
        );
        assert_eq!(large_arena.live_edge_bytes, short_arena.live_edge_bytes);
        assert_eq!(
            large_arena.live_storage_bytes,
            short_arena.live_storage_bytes
        );
        assert!(large_parser.estimated_owned_bytes < 512);
        assert!(large_writer.1 < 512);
        assert!(large_source.len() > 10 * 1024 * 1024);

        let short_abort = short_checkpoint.cancel(&mut short_document).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut short_document,
            short_abort,
            "short suffix scale checkpoint",
        );
        let large_abort = large_checkpoint.cancel(&mut large_document).unwrap();
        drain_abort_and_prove_fresh_candidate(
            &mut large_document,
            large_abort,
            "10 MiB suffix scale checkpoint",
        );
    }

    #[test]
    fn empty_source_has_only_the_document_and_no_phantom_line() {
        let (document, mut job) = drive("");
        let built = job.take_built();
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(
            matches!(
                trace.as_slice(),
                [
                    SerializedGreenTestEvent::Enter {
                        kind: GreenKind::DOCUMENT,
                        ..
                    },
                    SerializedGreenTestEvent::Exit
                ]
            ),
            "{trace:#?}"
        );
        assert_eq!(built.source_metric().bytes(), 0);
        assert_eq!(job.maximum_line_bytes(), 0);
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn direct_parser_and_writer_join_crlf_unicode_without_a_second_event_tree() {
        let source = "alpha\r\n😀";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(
            built.source_metric().utf16(),
            source.encode_utf16().count() as u64
        );
        assert!(
            matches!(
                trace.as_slice(),
                [
                    SerializedGreenTestEvent::Enter {
                        kind: GreenKind::DOCUMENT,
                        ..
                    },
                    SerializedGreenTestEvent::Enter {
                        kind: GreenKind::PARAGRAPH,
                        ..
                    },
                    SerializedGreenTestEvent::Coverage {
                        metric: SerializedMetric {
                            bytes: 11,
                            utf16: 9
                        },
                        part: CoveragePart::CONTENT,
                        logical: SerializedGreenTestLogical::Program { piece_count: 3 },
                        ..
                    },
                    SerializedGreenTestEvent::Exit,
                    SerializedGreenTestEvent::Exit
                ]
            ),
            "{trace:#?}"
        );
        assert_eq!(job.legacy_event_count(), 0);
        assert!(job.scratch_node_count() <= 2);
    }

    #[test]
    fn canonical_text_ranges_join_paragraph_atx_and_fence_through_the_exact_driver() {
        for (source, target_kind) in [
            ("a\tb\0😀\r\n", GreenKind::PARAGRAPH),
            ("# a\tb\0😀\r\n", GreenKind::HEADING),
            ("```x\t\0\r\n\tbody\0😀\r\n```\r\n", GreenKind::FENCED_CODE),
        ] {
            let (document, mut job) = drive(source);
            let built = job.take_built();
            let arena = document.candidate_writer_test_arena();
            let logical =
                serialized_green_test_logical_segments(built.green_document(), arena).unwrap();
            let target = logical
                .iter()
                .filter(|segment| segment.target_kind == target_kind)
                .collect::<Vec<_>>();

            assert!(
                target.iter().any(|segment| matches!(
                    segment.mapping,
                    crate::LogicalSegmentMapping::AtomicAmbiguity {
                        transform: crate::AtomicProjectionKind::NulToReplacement,
                    }
                )),
                "CanonicalText must retain NUL replacement for {source:?}: {logical:#?}"
            );
            assert!(
                target.iter().any(|segment| {
                    segment.mapping == crate::LogicalSegmentMapping::ExactIdentity
                        && source.as_bytes()[usize::try_from(segment.byte_range.start).unwrap()
                            ..usize::try_from(segment.byte_range.end).unwrap()]
                            .contains(&b'\t')
                }),
                "a full CanonicalText Tab remains exact identity for {source:?}: {logical:#?}"
            );
            assert_eq!(built.source_metric().bytes(), source.len() as u64);
            assert_eq!(
                built.source_metric().utf16(),
                source.encode_utf16().count() as u64
            );
            assert_eq!(job.legacy_event_count(), 0);
        }
    }

    #[test]
    fn exact_fence_derives_unicode_crlf_slices_inside_the_writer() {
        let source = "  ```` lang😀\r\n body\r\n   ```\r\n  ````  \r\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let (open, close) = fenced_facts(&document, &built);

        assert_eq!(open.fence(), GreenFenceCharacter::Backtick);
        assert_eq!(open.minimum_closing_length(), 4);
        assert_eq!(open.fence_offset_columns(), 2);
        assert!(close.closed());
        assert_eq!(close.info().bytes(), 0..9);
        assert_eq!(close.info().utf16(), 0..7);
        assert_eq!(close.literal().bytes(), 10..20);
        assert_eq!(close.literal().utf16(), 8..18);
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(
            built.source_metric().utf16(),
            source.encode_utf16().count() as u64
        );
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn exact_fence_empty_info_canonicalizes_every_line_ending_without_a_staged_atom() {
        for ending in ["\n", "\r", "\r\n"] {
            let source = format!("```{ending}```{ending}");
            let (document, mut job) = drive(&source);
            let built = job.take_built();
            let (_, close) = fenced_facts(&document, &built);

            assert!(close.closed(), "ending {ending:?}");
            assert_eq!(close.info().bytes(), 0..0, "ending {ending:?}");
            assert_eq!(close.info().utf16(), 0..0, "ending {ending:?}");
            assert_eq!(close.literal().bytes(), 1..1, "ending {ending:?}");
            assert_eq!(close.literal().utf16(), 1..1, "ending {ending:?}");
            assert_eq!(built.source_metric().bytes(), source.len() as u64);
        }
    }

    #[test]
    fn exact_fence_preserves_long_run_and_bare_eof_facts() {
        let run = "~".repeat(300);
        let source = format!("{run}lang");
        let (document, mut job) = drive(&source);
        let built = job.take_built();
        let (open, close) = fenced_facts(&document, &built);

        assert_eq!(open.fence(), GreenFenceCharacter::Tilde);
        assert_eq!(open.minimum_closing_length(), 300);
        assert_eq!(open.fence_offset_columns(), 0);
        assert!(!close.closed());
        assert_eq!(close.info().bytes(), 0..4);
        assert_eq!(close.info().utf16(), 0..4);
        assert_eq!(close.literal().bytes(), 4..4);
        assert_eq!(close.literal().utf16(), 4..4);
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn two_sequential_exact_fences_teardown_and_restart_the_writer_fold() {
        let source = "```\n```\n~~~x\n~~~";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let closes = serialized_green_test_close_facts(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap()
        .into_iter()
        .filter_map(|(kind, facts)| match (kind, facts) {
            (GreenKind::FENCED_CODE, GreenCloseFacts::FencedCode(facts)) => Some(facts),
            _ => None,
        })
        .collect::<Vec<_>>();

        assert_eq!(closes.len(), 2);
        assert_eq!(closes[0].info().bytes(), 0..0);
        assert_eq!(closes[0].literal().bytes(), 1..1);
        assert_eq!(closes[1].info().bytes(), 0..1);
        assert_eq!(closes[1].literal().bytes(), 2..2);
        assert!(closes.iter().all(|facts| facts.closed()));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn exact_nested_fence_keeps_projection_on_the_fence_binding() {
        let source = "> ```x\n> body\n> ```\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let (_, close) = fenced_facts(&document, &built);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();

        assert!(close.closed());
        assert_eq!(close.info().bytes(), 0..1);
        assert_eq!(close.literal().bytes(), 2..7);
        assert!(trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Coverage {
                part: CoveragePart::CONTAINER_MARKER,
                owner_relative_depth,
                logical: SerializedGreenTestLogical::None,
                ..
            } if *owner_relative_depth > 0
        )));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn partial_tab_keeps_quote_physical_ownership_and_fence_logical_target_independent() {
        let source = ">  ```\n>\tbody\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let logical = serialized_green_test_logical_segments(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let partial = logical
            .iter()
            .find(|segment| {
                segment.mapping
                    == crate::LogicalSegmentMapping::AtomicAmbiguity {
                        transform: crate::AtomicProjectionKind::TabToSpaces { spaces: 1 },
                    }
            })
            .expect("quote-owned partial Tab contributes one space to fenced literal");

        assert_eq!(partial.byte_range, 8..9);
        assert_eq!(partial.part, CoveragePart::CONTAINER_MARKER);
        assert_eq!(partial.physical_owner_kind, GreenKind::BLOCK_QUOTE);
        assert_eq!(partial.target_kind, GreenKind::FENCED_CODE);
        assert_eq!(partial.consumer_kind, GreenKind::FENCED_CODE);
        assert_ne!(partial.physical_owner_block, partial.target_block);
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn initial_bom_stays_a_document_gap_before_exact_fence_logical_zero() {
        let source = "\u{feff}```lang\nbody\n```\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let (_, close) = fenced_facts(&document, &built);
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();

        assert_eq!(close.info().bytes(), 0..4);
        assert_eq!(close.literal().bytes(), 5..10);
        assert!(trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Coverage {
                metric: SerializedMetric { bytes: 3, utf16: 1 },
                owner_relative_depth: 0,
                part: CoveragePart::GAP,
                logical: SerializedGreenTestLogical::None,
                ..
            }
        )));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn fence_full_tab_and_nul_atoms_use_canonical_text_instead_of_failing_closed() {
        let source = "```\n\tbad\0\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let (_, close) = fenced_facts(&document, &built);
        let logical = serialized_green_test_logical_segments(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();

        assert!(!close.closed());
        assert!(logical.iter().any(|segment| {
            segment.target_kind == GreenKind::FENCED_CODE
                && segment.byte_range.start <= 4
                && segment.byte_range.end > 4
                && segment.mapping == crate::LogicalSegmentMapping::ExactIdentity
        }));
        assert!(logical.iter().any(|segment| {
            segment.target_kind == GreenKind::FENCED_CODE
                && segment.mapping
                    == crate::LogicalSegmentMapping::AtomicAmbiguity {
                        transform: crate::AtomicProjectionKind::NulToReplacement,
                    }
        }));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn direct_list_and_item_facts_normalize_before_the_writer_boundary() {
        let bullet = direct_list_open_facts(DirectListFacts {
            list_type: DirectListType::Bullet,
            start: 1,
            delimiter: DirectListDelimiter::Period,
            bullet_char: b'+',
        })
        .unwrap();
        assert_eq!(
            bullet.style(),
            GreenListStyle::Bullet {
                marker: GreenListBullet::Plus
            }
        );
        assert_eq!(bullet.start(), None);
        assert_eq!(bullet.delimiter(), None);

        let ordered = direct_list_open_facts(DirectListFacts {
            list_type: DirectListType::Ordered,
            start: 42,
            delimiter: DirectListDelimiter::Paren,
            bullet_char: 0,
        })
        .unwrap();
        assert_eq!(
            ordered.style(),
            GreenListStyle::Ordered {
                start: 42,
                delimiter: GreenListDelimiter::Parenthesis
            }
        );
        assert_eq!(ordered.start(), Some(42));

        let item = direct_item_open_facts(DirectItemFacts {
            marker_offset: 3,
            padding: 14,
        })
        .unwrap();
        assert_eq!(item.marker_offset_columns(), 3);
        assert_eq!(item.padding_columns(), 14);

        assert!(matches!(
            direct_list_open_facts(DirectListFacts {
                list_type: DirectListType::Bullet,
                start: 1,
                delimiter: DirectListDelimiter::Period,
                bullet_char: b'?',
            }),
            Err(ExactBlockJobError::Writer(CandidateWriterError::Green(
                SerializedGreenError::Invalid(_)
            )))
        ));
        assert_eq!(
            direct_close_facts(GreenKind::LIST, DirectFinalFacts::List { tight: false }).unwrap(),
            GreenCloseFacts::List { tight: false }
        );
        assert!(matches!(
            direct_close_facts(GreenKind::LIST, DirectFinalFacts::None),
            Err(ExactBlockJobError::Invariant(
                "direct List close is missing final tightness"
            ))
        ));
    }

    #[test]
    fn exact_nested_list_protocol_preserves_typed_tightness_and_ancestor_owners() {
        let source = "> - alpha\n> - beta\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
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
            [
                GreenKind::DOCUMENT,
                GreenKind::BLOCK_QUOTE,
                GreenKind::LIST,
                GreenKind::ITEM,
                GreenKind::PARAGRAPH,
                GreenKind::ITEM,
                GreenKind::PARAGRAPH,
            ]
        );
        assert!(trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Coverage {
                owner_relative_depth,
                part: CoveragePart::CONTAINER_MARKER,
                ..
            } if *owner_relative_depth > 0
        )));
        let closes = serialized_green_test_close_facts(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(closes.contains(&(GreenKind::LIST, GreenCloseFacts::List { tight: true })));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn source_backed_recursive_commonmark_containers_reach_the_generic_green_writer() {
        const COMMONMARK_321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
        const COMMONMARK_325: &str = "* foo\n  * bar\n\n  baz\n";

        for (source, expected_kinds, expected_tight) in [
            (
                COMMONMARK_321,
                &[
                    GreenKind::DOCUMENT,
                    GreenKind::LIST,
                    GreenKind::ITEM,
                    GreenKind::PARAGRAPH,
                    GreenKind::BLOCK_QUOTE,
                    GreenKind::PARAGRAPH,
                    GreenKind::FENCED_CODE,
                    GreenKind::ITEM,
                    GreenKind::PARAGRAPH,
                ][..],
                true,
            ),
            (
                COMMONMARK_325,
                &[
                    GreenKind::DOCUMENT,
                    GreenKind::LIST,
                    GreenKind::ITEM,
                    GreenKind::PARAGRAPH,
                    GreenKind::LIST,
                    GreenKind::ITEM,
                    GreenKind::PARAGRAPH,
                    GreenKind::PARAGRAPH,
                ][..],
                false,
            ),
        ] {
            let mut expected_canonical = None;
            let mut expected_logical = None;
            let mut expected_close_states = None;
            for fuel in [1, 7, 64] {
                let (document, mut job) = drive_source_backed_atx(source, |_| fuel);
                let built = job.take_built();
                let arena = document.candidate_writer_test_arena();
                let trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
                let kinds: Vec<_> = trace
                    .iter()
                    .filter_map(|event| match event {
                        SerializedGreenTestEvent::Enter { kind, .. } => Some(*kind),
                        SerializedGreenTestEvent::Coverage { .. }
                        | SerializedGreenTestEvent::Exit => None,
                    })
                    .collect();
                assert_eq!(kinds, expected_kinds, "source={source:?}; fuel={fuel}");

                if source == COMMONMARK_321 {
                    assert_exact_content_ancestry(
                        source,
                        &trace,
                        "b",
                        &[
                            GreenKind::DOCUMENT,
                            GreenKind::LIST,
                            GreenKind::ITEM,
                            GreenKind::BLOCK_QUOTE,
                            GreenKind::PARAGRAPH,
                        ],
                    );
                    assert_exact_content_ancestry(
                        source,
                        &trace,
                        "c",
                        &[
                            GreenKind::DOCUMENT,
                            GreenKind::LIST,
                            GreenKind::ITEM,
                            GreenKind::FENCED_CODE,
                        ],
                    );
                } else {
                    assert_exact_content_ancestry(
                        source,
                        &trace,
                        "bar",
                        &[
                            GreenKind::DOCUMENT,
                            GreenKind::LIST,
                            GreenKind::ITEM,
                            GreenKind::LIST,
                            GreenKind::ITEM,
                            GreenKind::PARAGRAPH,
                        ],
                    );
                    assert_exact_content_ancestry(
                        source,
                        &trace,
                        "baz",
                        &[
                            GreenKind::DOCUMENT,
                            GreenKind::LIST,
                            GreenKind::ITEM,
                            GreenKind::PARAGRAPH,
                        ],
                    );
                }

                let closes =
                    serialized_green_test_close_facts(built.green_document(), arena).unwrap();
                assert!(
                    closes.contains(&(
                        GreenKind::LIST,
                        GreenCloseFacts::List {
                            tight: expected_tight,
                        },
                    )),
                    "source={source:?}; fuel={fuel}; closes={closes:?}",
                );
                let canonical =
                    serialized_green_test_canonical_trace(built.green_document(), arena).unwrap();
                let logical =
                    serialized_green_test_logical_segments(built.green_document(), arena).unwrap();
                let close_states =
                    serialized_green_test_close_states(built.green_document(), arena).unwrap();
                if let Some(expected) = &expected_canonical {
                    assert_eq!(&canonical, expected, "source={source:?}; fuel={fuel}");
                    assert_eq!(
                        &logical,
                        expected_logical.as_ref().unwrap(),
                        "source={source:?}; fuel={fuel}",
                    );
                    assert_eq!(
                        &close_states,
                        expected_close_states.as_ref().unwrap(),
                        "source={source:?}; fuel={fuel}",
                    );
                } else {
                    expected_canonical = Some(canonical);
                    expected_logical = Some(logical);
                    expected_close_states = Some(close_states);
                }
                assert_eq!(built.source_metric().bytes(), source.len() as u64);
                assert_eq!(
                    built.source_metric().utf16(),
                    source.encode_utf16().count() as u64,
                );
                assert_eq!(job.legacy_event_count(), 0);
            }
        }
    }

    #[test]
    fn exact_list_blank_line_produces_loose_close_fact() {
        let source = "- alpha\n\n- beta\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let closes = serialized_green_test_close_facts(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(closes.contains(&(GreenKind::LIST, GreenCloseFacts::List { tight: false })));
        let close_states = serialized_green_test_close_states(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(
            close_states
                .iter()
                .any(|(_, _, last_line_blank, _)| *last_line_blank),
            "the donor's intrinsic blank truth must survive the exact writer and packed codec",
        );
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn blank_close_keeps_final_newline_physical_only_and_gap_on_document() {
        let source = "a\n\nb";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        assert!(trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Coverage {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
                part: CoveragePart::TERMINAL,
                logical: SerializedGreenTestLogical::None,
                ..
            }
        )));
        assert!(trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Coverage {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
                owner_relative_depth: 0,
                part: CoveragePart::GAP,
                logical: SerializedGreenTestLogical::None,
                ..
            }
        )));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
    }

    #[test]
    fn trailing_newline_has_no_phantom_line_and_leading_indent_belongs_to_document() {
        let source = "  alpha\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let trace = serialized_green_test_trace(
            built.green_document(),
            document.candidate_writer_test_arena(),
        )
        .unwrap();
        let gap = trace
            .iter()
            .position(|event| {
                matches!(
                    event,
                    SerializedGreenTestEvent::Coverage {
                        metric: SerializedMetric { bytes: 2, utf16: 2 },
                        owner_relative_depth: 0,
                        part: CoveragePart::GAP,
                        logical: SerializedGreenTestLogical::None,
                        ..
                    }
                )
            })
            .expect("leading indent is document-owned while Document is top");
        let paragraph = trace
            .iter()
            .position(|event| {
                matches!(
                    event,
                    SerializedGreenTestEvent::Enter {
                        kind: GreenKind::PARAGRAPH,
                        ..
                    }
                )
            })
            .expect("paragraph enters");
        assert!(
            gap < paragraph,
            "physical parent source precedes child Enter"
        );
        assert!(trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Coverage {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
                part: CoveragePart::TERMINAL,
                logical: SerializedGreenTestLogical::None,
                ..
            }
        )));
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(job.maximum_line_bytes(), source.len());
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn proof_line_ceiling_is_shared_and_fails_before_unbounded_scratch() {
        let supported = "x".repeat(DirectValueBlockParser::MAX_LINE_BYTES);
        let (_, job) = drive(&supported);
        assert_eq!(job.maximum_line_bytes(), supported.len());

        let oversized = "x".repeat(DirectValueBlockParser::MAX_LINE_BYTES + 1);
        let (mut document, mut job) = document(&oversized);
        let error = (0..20_000)
            .find_map(|_| job.poll(&mut document).err())
            .expect("oversized line fails closed");
        assert!(matches!(
            error,
            ExactBlockJobError::Parser(ParseError::DirectUnsupported(
                DirectUnsupported::LineTooLarge
            ))
        ));
        assert!(job.recognition_line.len() <= DirectValueBlockParser::MAX_LINE_BYTES);
    }

    #[test]
    fn setext_normalizes_the_real_writer_path_with_exact_source_and_typed_facts() {
        for (source, level) in [
            ("alpha\n===\n", 1_u8),
            ("alpha\r\n---\r\n", 2_u8),
            ("> alpha\n> ===\n", 1_u8),
        ] {
            let (document, mut job) = drive(source);
            let built = job.take_built();
            let arena = document.candidate_writer_test_arena();
            let trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
            assert!(trace.iter().any(|event| matches!(
                event,
                SerializedGreenTestEvent::Enter {
                    kind: GreenKind::HEADING,
                    ..
                }
            )));
            assert!(!trace.iter().any(|event| matches!(
                event,
                SerializedGreenTestEvent::Enter {
                    kind: GreenKind::PARAGRAPH,
                    ..
                }
            )));
            assert!(trace.iter().any(|event| matches!(
                event,
                SerializedGreenTestEvent::Coverage {
                    metric: SerializedMetric { bytes: 3, utf16: 3 },
                    part: CoveragePart::BLOCK_MARKER,
                    logical: SerializedGreenTestLogical::None,
                    ..
                }
            )));
            let heading_facts = serialized_green_test_open_facts(built.green_document(), arena)
                .unwrap()
                .into_iter()
                .find_map(|(kind, facts)| (kind == GreenKind::HEADING).then_some(facts))
                .expect("Setext output stores Heading facts");
            assert_eq!(
                GreenHeadingOpenFacts::try_from_envelope(&heading_facts),
                GreenHeadingOpenFacts::setext(level),
            );
            assert_eq!(built.source_metric().bytes(), source.len() as u64);
            assert_eq!(
                built.source_metric().utf16(),
                source.encode_utf16().count() as u64
            );
            if source == "alpha\n===\n" {
                let at_start = built
                    .green_document()
                    .seek(
                        arena,
                        crate::GreenCoordinate::Bytes,
                        0,
                        crate::GreenAffinity::Downstream,
                    )
                    .unwrap();
                let heading = at_start.open_path().last().unwrap().enter;
                let mut logical = built
                    .green_document()
                    .logical_cursor(arena, heading)
                    .unwrap();
                let segment = logical
                    .next_segment(built.green_document(), arena)
                    .unwrap()
                    .expect("Heading content contributes one logical segment");
                assert_eq!(segment.byte_range, 0..5);
                assert_eq!(segment.logical_byte_range, 0..5);
                assert_eq!(segment.mapping, crate::LogicalSegmentMapping::ExactIdentity);
                assert!(
                    logical
                        .next_segment(built.green_document(), arena)
                        .unwrap()
                        .is_none(),
                    "newline and Setext underline remain physical-only"
                );
            }
            assert_eq!(job.legacy_event_count(), 0);
        }
    }

    #[test]
    fn source_backed_atx_matches_buffered_output_with_tiny_and_random_fuel() {
        let cases = [
            "# alpha\n",
            "   ### indented\r\n",
            "\u{feff}  ## bom and indent\n",
            "# alpha#   \n",
            "######\tβ😀 ###\r\n",
            "#   \r",
            "## alpha",
        ];

        for (case, source) in cases.into_iter().enumerate() {
            let (buffered_document, mut buffered_job) = drive(source);
            let buffered_built = buffered_job.take_built();
            let buffered_arena = buffered_document.candidate_writer_test_arena();
            let expected_trace = serialized_green_test_canonical_trace(
                buffered_built.green_document(),
                buffered_arena,
            )
            .unwrap();
            let expected_logical = serialized_green_test_logical_segments(
                buffered_built.green_document(),
                buffered_arena,
            )
            .unwrap();
            let expected_facts =
                serialized_green_test_open_facts(buffered_built.green_document(), buffered_arena)
                    .unwrap();

            let mut schedules: Vec<Box<dyn FnMut(usize) -> usize>> = vec![
                Box::new(|_| 1),
                Box::new({
                    let mut state = u64::try_from(case).unwrap() + 1;
                    move |_| {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407);
                        usize::try_from((state >> 32) % 31).unwrap() + 1
                    }
                }),
            ];
            for schedule in &mut schedules {
                let (document, mut job) = drive_source_backed_atx(source, schedule);
                let built = job.take_built();
                let arena = document.candidate_writer_test_arena();
                assert_eq!(
                    serialized_green_test_canonical_trace(built.green_document(), arena).unwrap(),
                    expected_trace,
                    "source={source:?}"
                );
                assert_eq!(
                    serialized_green_test_logical_segments(built.green_document(), arena).unwrap(),
                    expected_logical,
                    "source={source:?}"
                );
                assert_eq!(
                    serialized_green_test_open_facts(built.green_document(), arena).unwrap(),
                    expected_facts,
                    "source={source:?}"
                );
                assert_eq!(built.source_metric().bytes(), source.len() as u64);
                assert_eq!(
                    built.source_metric().utf16(),
                    source.encode_utf16().count() as u64
                );
                assert_eq!(job.source_line_metrics.lines_committed, 1);
                assert_eq!(
                    job.source_line_metrics.source_first_reads,
                    source.len() as u64
                );
                assert_eq!(job.source_line_metrics.actor_new_bytes, source.len() as u64);
                assert_eq!(job.source_line_metrics.actor_repeated_last_byte_peeks, 0);
                assert!(
                    job.source_line_metrics.maximum_donor_retained_source_bytes
                        <= DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES
                );
                assert!(job.source_line_metrics.maximum_actor_retained_byte_scratch <= 1);
                assert_eq!(
                    job.source_line_metrics.maximum_source_request_rewind_bytes,
                    0
                );
                assert!(
                    job.source_line_metrics.maximum_composed_work_per_poll
                        <= EXACT_SOURCE_LINE_MAX_COMPOSED_WORK_PER_POLL
                );
                assert!(job.recognition_line.is_empty());
                assert_eq!(job.legacy_event_count(), 0);
            }
        }
    }

    #[test]
    fn source_backed_atx_green_leaf_materializes_through_real_comrak_and_origin_join() {
        let source = "# **β😀** ###\r\n";
        let (document, mut job) = drive_source_backed_atx(source, |poll| match poll % 3 {
            0 => 1,
            1 => 2,
            _ => 3,
        });
        assert_eq!(job.source_line_metrics.lines_committed, 1);
        assert_eq!(job.legacy_event_count(), 0);

        let built = job.take_built();
        let arena = document.candidate_writer_test_arena();
        let target = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                2,
                crate::GreenAffinity::Downstream,
            )
            .unwrap()
            .open_path()
            .last()
            .expect("real exact writer must leave the heading open over its content")
            .enter;
        assert_eq!(target.kind, GreenKind::HEADING);

        let outcome = derive_inline_leaf_presentation(
            built.green_document(),
            arena,
            document.query_source(),
            target,
            &EMPTY_REFERENCE_SNAPSHOT,
        )
        .unwrap();
        let InlineLeafOutcome::Ready(ready) = outcome else {
            panic!("real source-backed green leaf must produce exact inline presentation");
        };

        assert_eq!(ready.logical(), "**β😀**");
        assert_eq!(
            ready.input_kind(),
            InlineInputKind::Heading {
                level: 1,
                setext: false,
            }
        );
        assert_eq!(ready.binding().target(), target);
        assert_eq!(ready.origin_map().runs.len(), 1);
        assert_eq!(ready.origin_map().runs[0].logical, 0..10);
        assert_eq!(ready.origin_map().runs[0].physical, 2..12);
        assert_eq!(ready.origin_map().runs[0].kind, OriginRunKind::Identity);

        let strong = ready
            .fragment()
            .facts
            .iter()
            .find(|fact| fact.kind == InlineFactKind::Strong as u8)
            .expect("real Comrak must recognize Strong");
        assert_eq!((strong.logical_start, strong.logical_len), (0, 10));
        let text = ready
            .fragment()
            .facts
            .iter()
            .find(|fact| fact.kind == InlineFactKind::Text as u8)
            .expect("real Comrak must retain a source-backed text fact");
        assert_eq!(text.flags, INLINE_FACT_FLAG_SOURCE_BACKED);
        assert_eq!((text.logical_start, text.logical_len), (2, 6));

        let composed_strong = ready
            .composed()
            .semantic_facts
            .iter()
            .find(|mapped| mapped.fact.kind == InlineFactKind::Strong as u8)
            .expect("origin composition must retain Strong");
        assert_eq!(composed_strong.physical_parts, vec![2..12]);
        let composed_text = ready
            .composed()
            .semantic_facts
            .iter()
            .find(|mapped| mapped.fact.kind == InlineFactKind::Text as u8)
            .expect("origin composition must retain Text");
        assert_eq!(composed_text.physical_parts, vec![4..10]);
        assert_eq!(ready.receipt().source_bytes_copied, 10);
        assert_eq!(ready.receipt().inline_service_calls, 1);
    }

    #[test]
    fn source_backed_over_cap_atx_completes_structure_before_zero_copy_unknown() {
        let logical_bytes = MAX_INLINE_FRAGMENT_BYTES + 1;
        let source = format!("# {}\n", "a".repeat(logical_bytes));
        let (document, mut job) = drive_source_backed_atx(&source, |poll| match poll % 4 {
            0 => 1,
            1 => 7,
            2 => 31,
            _ => 4_096,
        });
        assert_eq!(job.source_line_metrics.lines_committed, 1);
        assert_eq!(job.legacy_event_count(), 0);

        let built = job.take_built();
        let arena = document.candidate_writer_test_arena();
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        let target = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                2,
                crate::GreenAffinity::Downstream,
            )
            .unwrap()
            .open_path()
            .last()
            .expect("the over-cap real parse must still complete a structural heading")
            .enter;
        assert_eq!(target.kind, GreenKind::HEADING);

        let outcome = derive_inline_leaf_presentation(
            built.green_document(),
            arena,
            document.query_source(),
            target,
            &EMPTY_REFERENCE_SNAPSHOT,
        )
        .unwrap();
        let InlineLeafOutcome::Unknown(unknown) = outcome else {
            panic!("over-cap real leaf must source-paint as explicit Unknown");
        };
        assert_eq!(
            unknown.reason(),
            &InlineLeafUnknownReason::OverInputCap {
                observed_logical_end: u64::try_from(logical_bytes).unwrap(),
                cap: MAX_INLINE_FRAGMENT_BYTES,
            }
        );
        assert_eq!(unknown.binding().target(), target);
        assert_eq!(unknown.receipt().logical_segments_visited, 1);
        assert_eq!(unknown.receipt().source_ranges_read, 0);
        assert_eq!(unknown.receipt().source_bytes_copied, 0);
        assert_eq!(unknown.receipt().inline_service_calls, 0);
    }

    #[test]
    fn source_backed_giant_atx_is_one_pass_bounded_and_packed_seekable() {
        const BODY_BYTES: usize = 10 * 1024 * 1024;
        let mut source = String::with_capacity(BODY_BYTES + 12);
        source.push_str("# ");
        source.push_str(&"a".repeat(BODY_BYTES));
        source.push_str(" ###   \r\n");

        let (document, mut job) = drive_source_backed_atx(&source, |poll| match poll % 4 {
            0 => 4_090,
            1 => 4_031,
            2 => 3_997,
            _ => 4_087,
        });
        assert!(job.source_line_metrics.donor_polls > 2_500);
        assert_source_backed_line_bounds(&job, source.len());

        let built = job.take_built();
        let arena = document.candidate_writer_test_arena();
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(built.source_metric().utf16(), source.len() as u64);
        let trace = serialized_green_test_canonical_trace(built.green_document(), arena).unwrap();
        assert_eq!(
            trace.len(),
            8,
            "line length must not grow the packed event vector"
        );
        let physical_trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
        assert_eq!(
            physical_trace
                .iter()
                .filter(|event| matches!(event, SerializedGreenTestEvent::Coverage { .. }))
                .count(),
            4
        );

        let content_range = 2_u64..u64::try_from(2 + BODY_BYTES).unwrap();
        let mut at_content = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                u64::try_from(2 + BODY_BYTES / 2).unwrap(),
                crate::GreenAffinity::Downstream,
            )
            .unwrap();
        assert_eq!(
            at_content.open_path().last().unwrap().kind,
            GreenKind::HEADING
        );
        let content = at_content
            .next_coverage(built.green_document(), arena)
            .unwrap()
            .unwrap();
        assert_eq!(content.part, CoveragePart::CONTENT);
        assert_eq!(content.owner.kind, GreenKind::HEADING);
        assert_eq!(content.byte_range, content_range.clone());

        let logical =
            serialized_green_test_logical_segments(built.green_document(), arena).unwrap();
        assert_eq!(logical.len(), 1);
        assert_eq!(logical[0].byte_range, content_range);
        assert_eq!(
            logical[0].mapping,
            crate::LogicalSegmentMapping::ExactIdentity
        );
    }

    #[test]
    fn source_backed_giant_separator_stays_constant_and_seekable() {
        const SEPARATOR_BYTES: usize = 10 * 1024 * 1024;
        let mut source = String::with_capacity(SEPARATOR_BYTES + 2);
        source.push('#');
        source.push_str(&" ".repeat(SEPARATOR_BYTES));
        source.push('x');

        let (document, mut job) = drive_source_backed_atx(&source, |_| 4_096);
        assert!(job.source_line_metrics.donor_polls > 2_500);
        assert_source_backed_line_bounds(&job, source.len());
        assert_eq!(
            job.source_line_metrics.maximum_donor_retained_source_bytes,
            DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES,
            "integration must track the donor-owned fixed retention bound"
        );

        let built = job.take_built();
        let arena = document.candidate_writer_test_arena();
        let trace = serialized_green_test_canonical_trace(built.green_document(), arena).unwrap();
        assert_eq!(
            trace.len(),
            6,
            "separator length must not grow the packed event vector"
        );
        let physical_trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
        assert_eq!(
            physical_trace
                .iter()
                .filter(|event| matches!(event, SerializedGreenTestEvent::Coverage { .. }))
                .count(),
            2
        );

        let marker_end = u64::try_from(SEPARATOR_BYTES + 1).unwrap();
        let mut at_separator = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                u64::try_from(1 + SEPARATOR_BYTES / 2).unwrap(),
                crate::GreenAffinity::Downstream,
            )
            .unwrap();
        let marker = at_separator
            .next_coverage(built.green_document(), arena)
            .unwrap()
            .unwrap();
        assert_eq!(marker.part, CoveragePart::BLOCK_MARKER);
        assert_eq!(marker.owner.kind, GreenKind::HEADING);
        assert_eq!(marker.byte_range, 0..marker_end);

        let mut at_content = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                marker_end,
                crate::GreenAffinity::Downstream,
            )
            .unwrap();
        let content = at_content
            .next_coverage(built.green_document(), arena)
            .unwrap()
            .unwrap();
        assert_eq!(content.part, CoveragePart::CONTENT);
        assert_eq!(content.owner.kind, GreenKind::HEADING);
        assert_eq!(
            content.byte_range,
            marker_end..u64::try_from(source.len()).unwrap()
        );

        let logical =
            serialized_green_test_logical_segments(built.green_document(), arena).unwrap();
        assert_eq!(logical.len(), 1);
        assert_eq!(
            logical[0].byte_range,
            marker_end..u64::try_from(source.len()).unwrap()
        );
        assert_eq!(
            logical[0].mapping,
            crate::LogicalSegmentMapping::ExactIdentity
        );
    }

    #[test]
    fn source_backed_general_line_is_admitted_or_fails_only_at_exact_grammar() {
        let (_document, mut paragraph) = drive_source_backed_atx("plain paragraph\n", |_| 1);
        assert!(paragraph.take_built().source_metric().bytes() > 0);

        let source = "    # four-space code\n";
        let (mut document, mut job) = source_backed_atx_document(source);
        let error = (0..1_000)
            .find_map(|_| job.poll(&mut document).err())
            .expect("unsupported direct grammar fails after source recognition");
        assert!(matches!(
            error,
            ExactBlockJobError::Parser(ParseError::DirectUnsupported(_))
        ));
        assert!(matches!(job.mode, DriverMode::Failed));
        assert!(job.built.is_none());
        assert_eq!(job.source_line_metrics.lines_committed, 1);
        assert_eq!(job.legacy_event_count(), 0);
        cancel_exact_job(&mut document, job);
    }

    #[test]
    fn source_backed_atx_cancels_from_every_owned_phase() {
        for (source, phase, require_donor_poll) in [
            ("# alpha\n".to_owned(), "SessionOpen", false),
            ("# alpha\n".to_owned(), "Polling", false),
            (format!("# {}", "a".repeat(8 * 1024)), "Polling", true),
            ("# alpha\n".to_owned(), "Matched", false),
            ("# alpha\n".to_owned(), "ActorFinished", false),
        ] {
            let (mut document, job) = reach_source_backed_phase(&source, phase, require_donor_poll);
            assert_eq!(job.source_line_phase.as_ref().unwrap().label(), phase);
            if require_donor_poll {
                assert!(job.source_line_metrics.donor_polls > 0);
            }
            cancel_exact_job(&mut document, job);
        }
    }

    #[test]
    fn source_backed_recursive_container_cancels_at_nested_line_boundaries() {
        const COMMONMARK_321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";

        for phase in ["SessionOpen", "Polling", "Matched", "ActorFinished"] {
            let (mut document, job) = reach_source_backed_line_phase(COMMONMARK_321, 1, phase);
            let active = job
                .source_line_phase
                .as_ref()
                .expect("nested physical line owns its source admission");
            assert_eq!(active.label(), phase);
            assert_eq!(source_line_phase_ordinal(active), 1);
            assert_eq!(job.source_line_metrics.lines_committed, 1);
            cancel_exact_job(&mut document, job);
        }
    }

    #[test]
    fn source_backed_atx_rejects_crossed_identity_and_finishes_actor_before_commit() {
        let (mut first_document, mut first_job) =
            reach_source_backed_phase("# first\n", "Polling", false);
        let (mut second_document, second_job) = source_backed_atx_document("# second\n");
        let error = first_job.poll(&mut second_document).unwrap_err();
        assert!(matches!(
            error,
            ExactBlockJobError::Writer(CandidateWriterError::Actor(
                LiveDocumentError::WrongCandidateEpoch
            ))
        ));
        assert!(matches!(first_job.mode, DriverMode::Failed));
        assert!(
            !second_document
                .candidate_writer_is_poisoned(second_job.epoch)
                .unwrap()
        );
        cancel_exact_job(&mut first_document, first_job);
        cancel_exact_job(&mut second_document, second_job);

        let (mut receipt_document, mut receipt_job) =
            reach_source_backed_phase("# receipt\n", "Matched", false);
        assert!(receipt_job.current_line.is_none());
        assert_eq!(receipt_job.source_line_metrics.lines_committed, 0);
        receipt_job.poll(&mut receipt_document).unwrap();
        let (session, receipt) = match receipt_job.source_line_phase.as_ref().unwrap() {
            ExactSourceLinePhase::ActorFinished { receipt, .. } => (receipt.session(), *receipt),
            phase => panic!("actor finish entered wrong phase: {}", phase.label()),
        };
        assert!(receipt_job.current_line.is_none());
        assert_eq!(receipt_job.source_line_metrics.lines_committed, 0);

        let (mut foreign_document, foreign_job) = source_backed_atx_document("# foreign\n");
        assert!(matches!(
            foreign_job.validate_source_line_finish(session, receipt),
            Err(ExactBlockJobError::Invariant(
                "source-line finish receipt disagrees with actor session authority"
            ))
        ));

        receipt_job.poll(&mut receipt_document).unwrap();
        assert!(receipt_job.current_line.is_some());
        assert_eq!(receipt_job.source_line_metrics.lines_committed, 1);
        assert!(matches!(receipt_job.mode, DriverMode::PollLine));
        cancel_exact_job(&mut receipt_document, receipt_job);
        cancel_exact_job(&mut foreign_document, foreign_job);
    }

    #[test]
    fn atx_direct_output_preserves_exact_tail_projection_facts_and_eol_partition() {
        let cases = [
            (
                "### alpha ###   \r\n",
                3_u8,
                vec![
                    (
                        SerializedMetric { bytes: 4, utf16: 4 },
                        CoveragePart::BLOCK_MARKER,
                        SerializedGreenTestLogical::None,
                    ),
                    (
                        SerializedMetric { bytes: 5, utf16: 5 },
                        CoveragePart::CONTENT,
                        SerializedGreenTestLogical::Identity,
                    ),
                    (
                        SerializedMetric { bytes: 7, utf16: 7 },
                        CoveragePart::BLOCK_MARKER,
                        SerializedGreenTestLogical::None,
                    ),
                    (
                        SerializedMetric { bytes: 2, utf16: 2 },
                        CoveragePart::TERMINAL,
                        SerializedGreenTestLogical::None,
                    ),
                ],
            ),
            (
                "# alpha#   \n",
                1_u8,
                vec![
                    (
                        SerializedMetric { bytes: 2, utf16: 2 },
                        CoveragePart::BLOCK_MARKER,
                        SerializedGreenTestLogical::None,
                    ),
                    (
                        SerializedMetric { bytes: 9, utf16: 9 },
                        CoveragePart::CONTENT,
                        SerializedGreenTestLogical::Program { piece_count: 2 },
                    ),
                    (
                        SerializedMetric { bytes: 1, utf16: 1 },
                        CoveragePart::TERMINAL,
                        SerializedGreenTestLogical::None,
                    ),
                ],
            ),
            (
                "# alpha\n",
                1_u8,
                vec![
                    (
                        SerializedMetric { bytes: 2, utf16: 2 },
                        CoveragePart::BLOCK_MARKER,
                        SerializedGreenTestLogical::None,
                    ),
                    (
                        SerializedMetric { bytes: 5, utf16: 5 },
                        CoveragePart::CONTENT,
                        SerializedGreenTestLogical::Identity,
                    ),
                    (
                        SerializedMetric { bytes: 1, utf16: 1 },
                        CoveragePart::TERMINAL,
                        SerializedGreenTestLogical::None,
                    ),
                ],
            ),
            (
                "# alpha\r",
                1_u8,
                vec![
                    (
                        SerializedMetric { bytes: 2, utf16: 2 },
                        CoveragePart::BLOCK_MARKER,
                        SerializedGreenTestLogical::None,
                    ),
                    (
                        SerializedMetric { bytes: 5, utf16: 5 },
                        CoveragePart::CONTENT,
                        SerializedGreenTestLogical::Identity,
                    ),
                    (
                        SerializedMetric { bytes: 1, utf16: 1 },
                        CoveragePart::TERMINAL,
                        SerializedGreenTestLogical::None,
                    ),
                ],
            ),
            (
                "## alpha",
                2_u8,
                vec![
                    (
                        SerializedMetric { bytes: 3, utf16: 3 },
                        CoveragePart::BLOCK_MARKER,
                        SerializedGreenTestLogical::None,
                    ),
                    (
                        SerializedMetric { bytes: 5, utf16: 5 },
                        CoveragePart::CONTENT,
                        SerializedGreenTestLogical::Identity,
                    ),
                ],
            ),
        ];

        for (source, level, expected_coverage) in cases {
            let (document, mut job) = drive(source);
            let built = job.take_built();
            let arena = document.candidate_writer_test_arena();
            let trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
            let facts = serialized_green_test_open_facts(built.green_document(), arena)
                .unwrap()
                .into_iter()
                .find_map(|(kind, facts)| (kind == GreenKind::HEADING).then_some(facts))
                .expect("ATX output stores typed Heading facts");
            assert_eq!(
                GreenHeadingOpenFacts::try_from_envelope(&facts),
                GreenHeadingOpenFacts::atx(level),
                "wrong ATX facts for {source:?}"
            );
            let coverage = trace
                .iter()
                .filter_map(|event| match event {
                    SerializedGreenTestEvent::Coverage {
                        metric,
                        part,
                        logical,
                        ..
                    } => Some((*metric, *part, logical.clone())),
                    SerializedGreenTestEvent::Enter { .. } | SerializedGreenTestEvent::Exit => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                coverage, expected_coverage,
                "wrong ATX trace for {source:?}"
            );
            let logical =
                serialized_green_test_logical_segments(built.green_document(), arena).unwrap();
            if source == "# alpha#   \n" {
                assert_eq!(logical.len(), 2, "ATX tail remains two exact pieces");
                assert_eq!(logical[0].byte_range, 2..8);
                assert_eq!(logical[0].logical_byte_range, 0..6);
                assert_eq!(
                    logical[0].mapping,
                    crate::LogicalSegmentMapping::ExactIdentity
                );
                assert_eq!(logical[1].byte_range, 8..11);
                assert_eq!(logical[1].logical_byte_range, 6..6);
                assert_eq!(
                    logical[1].mapping,
                    crate::LogicalSegmentMapping::Hidden {
                        affinity: GreenAffinity::Upstream,
                    }
                );
            } else {
                assert_eq!(logical.len(), 1, "ATX marker and EOL stay physical-only");
                assert_eq!(
                    logical[0].mapping,
                    crate::LogicalSegmentMapping::ExactIdentity
                );
            }
            assert_eq!(built.source_metric().bytes(), source.len() as u64);
            assert_eq!(
                built.source_metric().utf16(),
                source.encode_utf16().count() as u64
            );
            assert_eq!(job.legacy_event_count(), 0);
        }
    }

    fn assert_setext_logical_content(
        built: &CandidateWriterBuiltDocument,
        arena: &PageArena,
        content_bytes: u64,
        content_utf16: u64,
    ) {
        let at_start = built
            .green_document()
            .seek(
                arena,
                crate::GreenCoordinate::Bytes,
                0,
                crate::GreenAffinity::Downstream,
            )
            .unwrap();
        let heading = at_start.open_path().last().unwrap().enter;
        let mut cursor = built
            .green_document()
            .logical_cursor(arena, heading)
            .unwrap();
        let mut segments = Vec::new();
        while let Some(segment) = cursor.next_segment(built.green_document(), arena).unwrap() {
            segments.push(segment);
        }
        assert!(!segments.is_empty());
        assert!(segments.iter().all(|segment| {
            segment.mapping == crate::LogicalSegmentMapping::ExactIdentity
                && segment.byte_range.end <= content_bytes
        }));
        assert_eq!(segments.first().unwrap().byte_range.start, 0);
        assert_eq!(segments.last().unwrap().byte_range.end, content_bytes);
        assert_eq!(segments.first().unwrap().logical_byte_range.start, 0);
        assert_eq!(
            segments.last().unwrap().logical_byte_range.end,
            content_bytes
        );
        assert_eq!(segments.first().unwrap().logical_utf16_range.start, 0);
        assert_eq!(
            segments.last().unwrap().logical_utf16_range.end,
            content_utf16
        );
    }

    #[test]
    fn setext_preserves_unicode_lone_cr_and_bare_eof_without_logical_markers() {
        for (source, level, content_bytes, content_utf16, terminal_count) in [
            ("😀 café\n===\n", 1_u8, 10_u64, 7_u64, 2_usize),
            ("alpha\r---\r", 2_u8, 5_u64, 5_u64, 2_usize),
            ("alpha\n===", 1_u8, 5_u64, 5_u64, 1_usize),
        ] {
            let (document, mut job) = drive(source);
            let built = job.take_built();
            let arena = document.candidate_writer_test_arena();
            let trace = serialized_green_test_trace(built.green_document(), arena).unwrap();

            let headings = trace
                .iter()
                .filter_map(|event| match event {
                    SerializedGreenTestEvent::Enter {
                        block,
                        kind: GreenKind::HEADING,
                    } => Some(*block),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(headings.len(), 1, "source {source:?}: {trace:#?}");
            assert!(!trace.iter().any(|event| matches!(
                event,
                SerializedGreenTestEvent::Enter {
                    kind: GreenKind::PARAGRAPH,
                    ..
                }
            )));

            let block_markers = trace
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        SerializedGreenTestEvent::Coverage {
                            metric: SerializedMetric { bytes: 3, utf16: 3 },
                            part: CoveragePart::BLOCK_MARKER,
                            logical: SerializedGreenTestLogical::None,
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(block_markers, 1, "source {source:?}: {trace:#?}");
            let terminals = trace
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        SerializedGreenTestEvent::Coverage {
                            part: CoveragePart::TERMINAL,
                            logical: SerializedGreenTestLogical::None,
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(
                terminals, terminal_count,
                "source {source:?} must not invent or hide a line ending"
            );

            let heading_facts = serialized_green_test_open_facts(built.green_document(), arena)
                .unwrap()
                .into_iter()
                .find_map(|(kind, facts)| (kind == GreenKind::HEADING).then_some(facts))
                .expect("Setext output stores Heading facts");
            assert_eq!(
                GreenHeadingOpenFacts::try_from_envelope(&heading_facts),
                GreenHeadingOpenFacts::setext(level),
            );
            assert_setext_logical_content(&built, arena, content_bytes, content_utf16);
            assert_eq!(built.source_metric().bytes(), source.len() as u64);
            assert_eq!(
                built.source_metric().utf16(),
                source.encode_utf16().count() as u64
            );
            assert_eq!(job.legacy_event_count(), 0);
        }
    }

    #[test]
    fn sequential_setext_headings_close_and_reopen_the_normalization_group() {
        let source = "one\n===\ntwo\n---\n";
        let (document, mut job) = drive(source);
        let built = job.take_built();
        let arena = document.candidate_writer_test_arena();
        let trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
        let heading_blocks = trace
            .iter()
            .filter_map(|event| match event {
                SerializedGreenTestEvent::Enter {
                    block,
                    kind: GreenKind::HEADING,
                } => Some(*block),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(heading_blocks.len(), 2, "{trace:#?}");
        assert_ne!(heading_blocks[0], heading_blocks[1]);
        assert!(!trace.iter().any(|event| matches!(
            event,
            SerializedGreenTestEvent::Enter {
                kind: GreenKind::PARAGRAPH,
                ..
            }
        )));
        assert_eq!(
            trace
                .iter()
                .filter(|event| matches!(
                    event,
                    SerializedGreenTestEvent::Coverage {
                        metric: SerializedMetric { bytes: 3, utf16: 3 },
                        part: CoveragePart::BLOCK_MARKER,
                        logical: SerializedGreenTestLogical::None,
                        ..
                    }
                ))
                .count(),
            2
        );
        let heading_facts = serialized_green_test_open_facts(built.green_document(), arena)
            .unwrap()
            .into_iter()
            .filter_map(|(kind, facts)| {
                (kind == GreenKind::HEADING)
                    .then(|| GreenHeadingOpenFacts::try_from_envelope(&facts).unwrap())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            heading_facts,
            [
                GreenHeadingOpenFacts::setext(1).unwrap(),
                GreenHeadingOpenFacts::setext(2).unwrap(),
            ]
        );
        assert_eq!(built.source_metric().bytes(), source.len() as u64);
        assert_eq!(job.legacy_event_count(), 0);
    }

    #[test]
    fn nested_setext_headings_preserve_real_container_ownership() {
        for (source, level, expected_kinds) in [
            (
                "> > alpha\n> > ===\n",
                1_u8,
                vec![
                    GreenKind::DOCUMENT,
                    GreenKind::BLOCK_QUOTE,
                    GreenKind::BLOCK_QUOTE,
                    GreenKind::HEADING,
                ],
            ),
            (
                "- alpha\n  ===\n",
                1_u8,
                vec![
                    GreenKind::DOCUMENT,
                    GreenKind::LIST,
                    GreenKind::ITEM,
                    GreenKind::HEADING,
                ],
            ),
            (
                "> - alpha\n>   ---\n",
                2_u8,
                vec![
                    GreenKind::DOCUMENT,
                    GreenKind::BLOCK_QUOTE,
                    GreenKind::LIST,
                    GreenKind::ITEM,
                    GreenKind::HEADING,
                ],
            ),
        ] {
            let (document, mut job) = drive(source);
            let built = job.take_built();
            let arena = document.candidate_writer_test_arena();
            let trace = serialized_green_test_trace(built.green_document(), arena).unwrap();
            let kinds = trace
                .iter()
                .filter_map(|event| match event {
                    SerializedGreenTestEvent::Enter { kind, .. } => Some(*kind),
                    SerializedGreenTestEvent::Coverage { .. } | SerializedGreenTestEvent::Exit => {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(kinds, expected_kinds, "source {source:?}: {trace:#?}");
            assert!(trace.iter().any(|event| matches!(
                event,
                SerializedGreenTestEvent::Coverage {
                    part: CoveragePart::CONTAINER_MARKER,
                    owner_relative_depth,
                    logical: SerializedGreenTestLogical::None,
                    ..
                } if *owner_relative_depth > 0
            )));
            let heading_facts = serialized_green_test_open_facts(built.green_document(), arena)
                .unwrap()
                .into_iter()
                .find_map(|(kind, facts)| (kind == GreenKind::HEADING).then_some(facts))
                .expect("nested Setext output stores Heading facts");
            assert_eq!(
                GreenHeadingOpenFacts::try_from_envelope(&heading_facts),
                GreenHeadingOpenFacts::setext(level),
            );
            assert_eq!(built.source_metric().bytes(), source.len() as u64);
            assert_eq!(
                built.source_metric().utf16(),
                source.encode_utf16().count() as u64
            );
            assert_eq!(job.legacy_event_count(), 0);
        }
    }

    #[test]
    fn unsupported_grammar_publishes_nothing_and_enters_fuelled_abort() {
        let (mut document, mut job) = document("***\n");
        let error = (0..1_000)
            .find_map(|_| job.poll(&mut document).err())
            .expect("unsupported heading fails closed");
        assert!(matches!(
            error,
            ExactBlockJobError::Parser(ParseError::DirectUnsupported(DirectUnsupported::BlockKind))
        ));
        assert!(job.built.is_none());

        let abort = job.cancel(&mut document).unwrap();
        for _ in 0..100 {
            if document.poll_candidate_abort(abort, 1).unwrap().complete {
                return;
            }
        }
        panic!("failed exact candidate did not complete fuelled abort");
    }

    /// Architecture gate: the selected direct donor must own a resumable,
    /// source-backed reference-prefix finalizer before the writer may
    /// normalize either outcome or mint occurrence/winner state. Merely
    /// avoiding an error is insufficient: both paths must reach integrated
    /// publication within a bounded number of actor polls.
    #[test]
    fn reference_prefix_requires_one_parser_owned_finalizer_before_integrated_publication() {
        let mut unsupported = Vec::new();
        for (outcome, source) in [
            (
                "reference-only",
                "[duplicate]: /first\n[duplicate]: /second\n",
            ),
            ("visible-remainder", "[visible]: /target\nbody\n"),
        ] {
            let (mut document, mut job) = document(source);
            let mut error = None;
            let mut completed = false;
            for _ in 0..10_000 {
                match job.poll(&mut document) {
                    Ok(ExactBlockJobProgress::Pending) => {}
                    Ok(ExactBlockJobProgress::Complete) => {
                        completed = true;
                        break;
                    }
                    Err(failure) => {
                        error = Some(failure);
                        break;
                    }
                }
            }
            match error {
                None => {
                    assert!(completed, "{outcome} reference finalizer did not converge");
                    assert!(
                        job.built.is_some(),
                        "{outcome} reference finalizer completed without integrated publication"
                    );
                    if outcome == "reference-only" {
                        assert_eq!(
                            job.built
                                .as_ref()
                                .expect("integrated reference publication was just proved")
                                .reference_composite_receipt_for_test(),
                            (2, 1, 2, 1),
                            "duplicate definitions must publish two occurrences, one exact label, and one atomic two-child parent",
                        );
                    }
                }
                Some(ExactBlockJobError::ReferenceExternalWork(kind)) => {
                    assert!(job.built.is_none());
                    unsupported.push((outcome, kind));
                    let abort = job.cancel(&mut document).unwrap();
                    let mut cancelled = false;
                    for _ in 0..100 {
                        if document.poll_candidate_abort(abort, 1).unwrap().complete {
                            cancelled = true;
                            break;
                        }
                    }
                    assert!(cancelled, "paused reference candidate did not cancel");
                }
                Some(ExactBlockJobError::Parser(ParseError::DirectUnsupported(reason))) => {
                    assert!(job.built.is_none());
                    let abort = job.cancel(&mut document).unwrap();
                    let mut cancelled = false;
                    for _ in 0..100 {
                        if document.poll_candidate_abort(abort, 1).unwrap().complete {
                            cancelled = true;
                            break;
                        }
                    }
                    assert!(cancelled, "failed reference candidate did not cancel");
                    panic!(
                        "reference candidate regressed to generic unsupported boundary: \
                         {outcome}: {reason:?}"
                    );
                }
                Some(other) => panic!(
                    "reference-prefix integration failed through an unexpected seam: {other:?}"
                ),
            }
        }

        assert!(
            unsupported.is_empty(),
            "REFERENCE_PREFIX_INTEGRATION_GATE: both outcomes must pass through one parser-owned \
             resumable reference finalizer before candidate normalization and occurrence/winner \
             publication; observed fail-closed boundaries: {unsupported:?}"
        );
    }
}

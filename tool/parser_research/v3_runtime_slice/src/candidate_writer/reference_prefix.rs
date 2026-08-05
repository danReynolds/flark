//! Candidate-owned join for parser reference-prefix finalization.
//!
//! The concrete state machine is kept beside `CandidateWriter` so parser DFA,
//! active-Paragraph projection, cooked values, and the unpublished reference
//! index cannot be driven or published independently.

use super::*;

use flark_comrak_value_block_core::{
    DirectReferencePrefixContext, DirectReferencePrefixDisposition,
    DirectReferencePrefixOutputAck, DirectReferencePrefixOutputAckStatus,
    DirectReferencePrefixPollError, DirectReferencePrefixPollStatus, DirectReferencePrefixRequest,
    DirectReferencePrefixSource, DirectReferencePrefixTerminalAck, DirectReferencePrefixWork,
    DirectReferenceValueTransform,
};

use crate::persistent_blob::PersistentByteBlob;
use crate::reference_label_interner::{
    CandidateReferenceLabel, ReferenceLabelInterner, ReferenceLabelInternerProgress,
};
use crate::reference_restart_index::{
    ReferenceCandidateIndexAuthority, ReferenceCandidateIndexBuilder,
    ReferenceCandidateIndexManifest, ReferenceCandidateIndexProgress,
    WriterAuthenticatedReferenceOccurrence,
};
use crate::reference_value_blob::{
    ReferenceValueBlobMaterializer, ReferenceValueBlobProgress, ReferenceValueKind,
};
use crate::serialized_green::active_paragraph_projection_cursor::{
    ActiveParagraphCanonicalRewriteBegin, ActiveParagraphCanonicalRewriteProgress,
    ActiveParagraphProjectedRangeCapability, ActiveParagraphProjectedReferenceTerminal,
    ActiveParagraphProjectionCursor, ActiveParagraphProjectionError,
    ActiveParagraphProjectionIdentity, ActiveParagraphProjectionPoll,
    ActiveParagraphProjectionSourcePass, ActiveParagraphProjectionTransactionSeal,
    ActiveParagraphRangeReplayPass, ActorProjectionBinding, StagedParagraphTerminator,
    StagedTerminatorKind,
};
use crate::{SourceProjectionSession, SourceProjectionSessionReceipt, SourceStore};

enum ReferenceValueStage {
    Probe,
    Replay,
    Finish,
}

/// One exact two-pass cooked-value transaction. Both passes consume the same
/// parser-authenticated range capability through the cursor's sole source
/// session; only the durable cooked blob leaves this state.
struct ReferenceValueTransaction {
    replay: Option<ActiveParagraphRangeReplayPass>,
    source: Option<SourceProjectionSession>,
    materializer: ReferenceValueBlobMaterializer,
    stage: ReferenceValueStage,
}

struct PendingReferenceOccurrence {
    normalized_label: String,
    title_range: Option<ActiveParagraphProjectedRangeCapability>,
    destination: Option<PersistentByteBlob>,
    title: Option<PersistentByteBlob>,
    output_ack: Option<DirectReferencePrefixOutputAck<ActiveParagraphProjectionIdentity>>,
}

enum ReferenceDriveStage {
    Failed,
    Scan {
        source: ActiveParagraphProjectionSourcePass,
    },
    DrainCursor {
        source: ActiveParagraphProjectionSourcePass,
    },
    Destination {
        occurrence: PendingReferenceOccurrence,
        value: ReferenceValueTransaction,
    },
    Title {
        occurrence: PendingReferenceOccurrence,
        value: ReferenceValueTransaction,
    },
    BeginIntern {
        occurrence: PendingReferenceOccurrence,
        source: SourceProjectionSession,
    },
    PollInterner {
        occurrence: PendingReferenceOccurrence,
        source: SourceProjectionSession,
    },
    PollIndex {
        output_ack: DirectReferencePrefixOutputAck<ActiveParagraphProjectionIdentity>,
        source: SourceProjectionSession,
    },
}

struct ReferencePrefixDrive {
    binding: ActorProjectionBinding,
    cursor: Option<ActiveParagraphProjectionCursor>,
    work: Option<DirectReferencePrefixWork<ActiveParagraphProjectionIdentity>>,
    stage: ReferenceDriveStage,
}

enum ReferenceDriveOutcome {
    Pending(Box<ReferencePrefixDrive>),
    Rewrite {
        binding: ActorProjectionBinding,
        seal: ActiveParagraphProjectionTransactionSeal,
        terminal: ActiveParagraphProjectedReferenceTerminal,
        retired_source: SourceProjectionSessionReceipt,
    },
}

enum ReferenceCanonicalMode {
    Visible {
        paragraph: CandidateWriterBinding,
    },
    ReferenceOnly {
        retired_block: BlockId,
        parent: CandidateWriterBinding,
        terminator_gap: Option<crate::ConsumedSourcePiece>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReferenceRewriteAwaiting {
    FragmentStart,
    None,
    Event,
    Survivor,
}

struct ReferenceCanonicalRewrite {
    begin: ActiveParagraphCanonicalRewriteBegin,
    mode: ReferenceCanonicalMode,
    awaiting: ReferenceRewriteAwaiting,
    survivor_offered: bool,
}

/// Document-scoped semantic state. Every reference-prefix job borrows this
/// same interner/index pair, so duplicate-first winner order spans the whole
/// candidate rather than resetting at Paragraph boundaries.
#[derive(Debug)]
pub(super) struct CandidateReferenceSemanticTransaction {
    pub(super) interner: ReferenceLabelInterner,
    pub(super) index: ReferenceCandidateIndexBuilder,
}

impl CandidateReferenceSemanticTransaction {
    pub(super) fn new(epoch: LiveCandidateEpoch) -> Result<Self, CandidateWriterError> {
        let generation = epoch.parse_token().generation.0;
        let mut mint = ReferenceCandidateIndexWriterMint(());
        let authority = ReferenceCandidateIndexAuthority::from_writer_join(
            epoch.build_id(),
            epoch.source(),
            1,
            generation,
            generation,
            &mut mint,
        )
        .map_err(|_| {
            CandidateWriterError::Invariant("reference index authority rejected writer join")
        })?;
        let interner = ReferenceLabelInterner::new_initial(epoch.build_id(), 1).map_err(|_| {
            CandidateWriterError::Invariant("reference label interner rejected writer join")
        })?;
        let index = ReferenceCandidateIndexBuilder::new(authority).map_err(|_| {
            CandidateWriterError::Invariant("reference index rejected writer join")
        })?;
        Ok(Self { interner, index })
    }
}

pub(super) enum ReferencePrefixPhase {
    RequestStructuralFlush,
    Drain(ComposerDrain),
    BeginLeafBarrier,
    AwaitLeafBarrier,
    BeginWorkingPrefixReduction {
        barrier_generation: u64,
    },
    AwaitWorkingPrefixReduction {
        barrier_generation: u64,
    },
    OpenProjection {
        barrier_generation: u64,
    },
    AwaitParserWork {
        binding: ActorProjectionBinding,
        cursor: ActiveParagraphProjectionCursor,
    },
    Drive {
        drive: Box<ReferencePrefixDrive>,
    },
    Rewrite {
        binding: ActorProjectionBinding,
        seal: ActiveParagraphProjectionTransactionSeal,
        terminal: ActiveParagraphProjectedReferenceTerminal,
        retired_source: SourceProjectionSessionReceipt,
    },
    DriveRewrite {
        rewrite: Box<ReferenceCanonicalRewrite>,
    },
    AwaitRewriteCommit {
        begin: ActiveParagraphCanonicalRewriteBegin,
        mode: ReferenceCanonicalMode,
        survivor_offered: bool,
    },
    DrainReferenceOnlyGap {
        terminal: ActiveParagraphProjectedReferenceTerminal,
        drain: ComposerDrain,
    },
}

impl fmt::Debug for ReferencePrefixPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::RequestStructuralFlush => "RequestStructuralFlush",
            Self::Drain(_) => "Drain",
            Self::BeginLeafBarrier => "BeginLeafBarrier",
            Self::AwaitLeafBarrier => "AwaitLeafBarrier",
            Self::BeginWorkingPrefixReduction { .. } => "BeginWorkingPrefixReduction",
            Self::AwaitWorkingPrefixReduction { .. } => "AwaitWorkingPrefixReduction",
            Self::OpenProjection { .. } => "OpenProjection",
            Self::AwaitParserWork { .. } => "AwaitParserWork",
            Self::Drive { .. } => "Drive",
            Self::Rewrite { .. } => "Rewrite",
            Self::DriveRewrite { .. } => "DriveRewrite",
            Self::AwaitRewriteCommit { .. } => "AwaitRewriteCommit",
            Self::DrainReferenceOnlyGap { .. } => "DrainReferenceOnlyGap",
        };
        formatter.write_str(name)
    }
}

/// One candidate-owned reference-prefix transaction. The provisional
/// Paragraph binding, Green Enter capability, composer origin, parser work,
/// projection cursor, and source replay session remain in this action until a
/// terminal result has been normalized and acknowledged.
pub(super) struct ReferencePrefixJob {
    pub(super) request: DirectReferencePrefixRequest,
    pub(super) paragraph: Option<CandidateWriterBinding>,
    pub(super) enter: Option<ProvisionalParagraphEnter>,
    pub(super) projection_origin: Option<CanonicalFragmentProjectionOrigin>,
    pub(super) phase: ReferencePrefixPhase,
}

impl fmt::Debug for ReferencePrefixJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferencePrefixJob")
            .field("request", &self.request)
            .field("paragraph_present", &self.paragraph.is_some())
            .field("enter_present", &self.enter.is_some())
            .field("projection_origin_present", &self.projection_origin.is_some())
            .field("phase", &self.phase)
            .finish()
    }
}

/// Terminal join returned to the exact parser only after CandidateWriter has
/// completed both the semantic and canonical storage transactions.
#[must_use = "the exact parser must commit the reference terminal acknowledgement"]
pub(crate) struct CandidateReferencePrefixTerminal {
    pub(super) binding: Option<CandidateWriterBinding>,
    pub(super) identity: ActiveParagraphProjectionIdentity,
    pub(super) ack: DirectReferencePrefixTerminalAck<ActiveParagraphProjectionIdentity>,
}

impl fmt::Debug for CandidateReferencePrefixTerminal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CandidateReferencePrefixTerminal")
            .field("binding_present", &self.binding.is_some())
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl CandidateReferencePrefixTerminal {
    pub(crate) fn into_parts(
        self,
    ) -> (
        Option<CandidateWriterBinding>,
        ActiveParagraphProjectionIdentity,
        DirectReferencePrefixTerminalAck<ActiveParagraphProjectionIdentity>,
    ) {
        (self.binding, self.identity, self.ack)
    }
}

fn serialized_metric(metric: SourceLedgerMetric) -> SerializedMetric {
    SerializedMetric {
        bytes: metric.bytes(),
        utf16: metric.utf16(),
    }
}

fn staged_terminator(
    projection_generation: u64,
    pending: crate::source_bound_ledger::CandidatePendingTerminator,
) -> StagedParagraphTerminator {
    let kind = match pending.ending() {
        crate::CandidateLineEnding::Lf => StagedTerminatorKind::Lf,
        crate::CandidateLineEnding::CrLf => StagedTerminatorKind::CrLf,
        crate::CandidateLineEnding::LoneCr => StagedTerminatorKind::LoneCr,
    };
    StagedParagraphTerminator::from_writer_join(
        projection_generation,
        serialized_metric(pending.source_start()),
        kind,
    )
}

impl CandidateWriter {
    pub(super) fn with_reference_semantic_session<T>(
        &mut self,
        arena: &mut PageArena,
        operation: impl FnOnce(
            &mut CandidateReferenceSemanticTransaction,
            &mut crate::ArenaBuildSession<'_>,
        ) -> Result<T, CandidateWriterError>,
    ) -> Result<T, CandidateWriterError> {
        let mut semantic = self.reference_semantic.take().ok_or(
            CandidateWriterError::Invariant("candidate reference semantic state is missing"),
        )?;
        let result = self.with_short_session(arena, |_, session| {
            operation(&mut semantic, session)
        });
        self.reference_semantic = Some(semantic);
        result
    }

    fn begin_reference_value_transaction(
        &mut self,
        arena: &mut PageArena,
        cursor: &ActiveParagraphProjectionCursor,
        binding: ActorProjectionBinding,
        range: ActiveParagraphProjectedRangeCapability,
        source: SourceProjectionSession,
        kind: ReferenceValueKind,
    ) -> Result<ReferenceValueTransaction, CandidateWriterError> {
        let request = range.prepare_replay()?;
        let replay = self.with_short_session(arena, |builder, session| {
            cursor.begin_range_replay_in_source_session(
                builder, session, binding, request, source,
            )
        })?;
        let materializer = ReferenceValueBlobMaterializer::try_new(self.epoch.build_id(), kind)
            .map_err(|_| {
                CandidateWriterError::Invariant("reference value materializer rejected writer join")
            })?;
        Ok(ReferenceValueTransaction {
            replay: Some(replay),
            source: None,
            materializer,
            stage: ReferenceValueStage::Probe,
        })
    }

    /// Advances at most one projection byte or one cooked-value storage poll.
    /// Completion returns the durable blob and the transaction's sole source
    /// session so the next field or parser scan can resume without aliasing.
    fn poll_reference_value_transaction(
        &mut self,
        arena: &mut PageArena,
        cursor: &ActiveParagraphProjectionCursor,
        binding: ActorProjectionBinding,
        value: &mut ReferenceValueTransaction,
    ) -> Result<Option<(PersistentByteBlob, SourceProjectionSession)>, CandidateWriterError> {
        match value.stage {
            ReferenceValueStage::Probe => {
                let replay = value.replay.as_mut().ok_or(CandidateWriterError::Invariant(
                    "reference value probe lost its projected range",
                ))?;
                let progress = self.with_short_session(arena, |builder, session| {
                    replay.poll_byte(builder, session, binding, false)
                })?;
                match progress {
                    ActiveParagraphProjectionPoll::Pending => {}
                    ActiveParagraphProjectionPoll::ByteReady => {
                        let identity = cursor.identity();
                        let mut source = replay.direct_source(identity)?;
                        let offset = source.available_len().checked_sub(1).ok_or(
                            CandidateWriterError::Invariant(
                                "reference probe exposed no readable byte",
                            ),
                        )?;
                        let byte = source.read_byte(offset)?;
                        value.materializer.offer_probe_byte(byte).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference value probe rejected projected byte",
                            )
                        })?;
                    }
                    ActiveParagraphProjectionPoll::Complete => {
                        let replay = value.replay.take().ok_or(
                            CandidateWriterError::Invariant(
                                "completed reference probe lost its range",
                            ),
                        )?;
                        let (completed, source) = replay.take_completed()?;
                        value.materializer.finish_probe().map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference value probe could not select its body",
                            )
                        })?;
                        let request = completed.into_capability().prepare_replay()?;
                        let replay = self.with_short_session(arena, |builder, session| {
                            cursor.begin_range_replay_in_source_session(
                                builder, session, binding, request, source,
                            )
                        })?;
                        value.replay = Some(replay);
                        value.stage = ReferenceValueStage::Replay;
                    }
                    ActiveParagraphProjectionPoll::Cancelled => {
                        return Err(CandidateWriterError::Invariant(
                            "reference value probe was cancelled",
                        ));
                    }
                }
            }
            ReferenceValueStage::Replay => {
                if value.materializer.ready_for_replay_byte() {
                    let replay = value.replay.as_mut().ok_or(
                        CandidateWriterError::Invariant(
                            "reference value replay lost its projected range",
                        ),
                    )?;
                    let progress = self.with_short_session(arena, |builder, session| {
                        replay.poll_byte(builder, session, binding, false)
                    })?;
                    match progress {
                        ActiveParagraphProjectionPoll::Pending => {}
                        ActiveParagraphProjectionPoll::ByteReady => {
                            let identity = cursor.identity();
                            let mut source = replay.direct_source(identity)?;
                            let offset = source.available_len().checked_sub(1).ok_or(
                                CandidateWriterError::Invariant(
                                    "reference replay exposed no readable byte",
                                ),
                            )?;
                            let byte = source.read_byte(offset)?;
                            value.materializer.offer_replay_byte(byte).map_err(|_| {
                                CandidateWriterError::Invariant(
                                    "reference cleaner rejected projected byte",
                                )
                            })?;
                        }
                        ActiveParagraphProjectionPoll::Complete => {
                            return Err(CandidateWriterError::Invariant(
                                "reference value replay ended before the cooked input",
                            ));
                        }
                        ActiveParagraphProjectionPoll::Cancelled => {
                            return Err(CandidateWriterError::Invariant(
                                "reference value replay was cancelled",
                            ));
                        }
                    }
                } else if value.materializer.ready_to_finish_replay() {
                    let replay = value.replay.as_mut().ok_or(
                        CandidateWriterError::Invariant(
                            "finishing reference replay lost its projected range",
                        ),
                    )?;
                    let progress = self.with_short_session(arena, |builder, session| {
                        replay.poll_byte(builder, session, binding, false)
                    })?;
                    match progress {
                        ActiveParagraphProjectionPoll::Pending => {}
                        ActiveParagraphProjectionPoll::Complete => {
                            let replay = value.replay.take().ok_or(
                                CandidateWriterError::Invariant(
                                    "completed reference replay lost its range",
                                ),
                            )?;
                            let (completed, source) = replay.take_completed()?;
                            drop(completed.into_capability());
                            value.materializer.finish_replay().map_err(|_| {
                                CandidateWriterError::Invariant(
                                    "reference cleaner rejected replay completion",
                                )
                            })?;
                            value.source = Some(source);
                            value.stage = ReferenceValueStage::Finish;
                        }
                        ActiveParagraphProjectionPoll::ByteReady => {
                            return Err(CandidateWriterError::Invariant(
                                "reference replay exceeded the cooked input",
                            ));
                        }
                        ActiveParagraphProjectionPoll::Cancelled => {
                            return Err(CandidateWriterError::Invariant(
                                "reference value replay was cancelled",
                            ));
                        }
                    }
                } else {
                    let progress = self.with_short_session(arena, |_, session| {
                        value.materializer.poll(session).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference value cleaner failed during replay",
                            )
                        })
                    })?;
                    if matches!(
                        progress,
                        ReferenceValueBlobProgress::Complete
                            | ReferenceValueBlobProgress::Cancelled
                    ) {
                        return Err(CandidateWriterError::Invariant(
                            "reference value cleaner left replay prematurely",
                        ));
                    }
                }
            }
            ReferenceValueStage::Finish => {
                let progress = self.with_short_session(arena, |_, session| {
                    value.materializer.poll(session).map_err(|_| {
                        CandidateWriterError::Invariant(
                            "reference value storage failed during completion",
                        )
                    })
                })?;
                match progress {
                    ReferenceValueBlobProgress::Complete => {
                        let blob = value.materializer.take_blob().map_err(|_| {
                            CandidateWriterError::Invariant(
                                "completed reference value lost its blob",
                            )
                        })?;
                        let source = value.source.take().ok_or(
                            CandidateWriterError::Invariant(
                                "completed reference value lost its source session",
                            ),
                        )?;
                        return Ok(Some((blob, source)));
                    }
                    ReferenceValueBlobProgress::Cancelled => {
                        return Err(CandidateWriterError::Invariant(
                            "reference value storage was cancelled",
                        ));
                    }
                    ReferenceValueBlobProgress::ReadyForReplayByte
                    | ReferenceValueBlobProgress::ReadyToFinishReplay
                    | ReferenceValueBlobProgress::Pending => {}
                }
            }
        }
        Ok(None)
    }

    fn map_reference_poll_error(
        error: DirectReferencePrefixPollError<ActiveParagraphProjectionError>,
    ) -> CandidateWriterError {
        match error {
            DirectReferencePrefixPollError::Source(error) => error.into(),
            DirectReferencePrefixPollError::ZeroFuel
            | DirectReferencePrefixPollError::WrongSource
            | DirectReferencePrefixPollError::SourceBudgetContractViolated
            | DirectReferencePrefixPollError::NonSequentialSource
            | DirectReferencePrefixPollError::InvalidUtf8 { .. }
            | DirectReferencePrefixPollError::InvalidRawCodepointContribution { .. }
            | DirectReferencePrefixPollError::PollAfterComplete
            | DirectReferencePrefixPollError::PollAfterCancelled
            | DirectReferencePrefixPollError::OutputNotAcknowledged
            | DirectReferencePrefixPollError::OutputNotReady
            | DirectReferencePrefixPollError::WrongOutput
            | DirectReferencePrefixPollError::CounterOverflow => CandidateWriterError::Invariant(
                "reference parser work rejected its writer-owned source",
            ),
        }
    }

    fn begin_reference_occurrence(
        &mut self,
        arena: &mut PageArena,
        drive: &mut ReferencePrefixDrive,
        source: SourceProjectionSession,
        request_nonce: u64,
    ) -> Result<(), CandidateWriterError> {
        let output = drive
            .work
            .as_mut()
            .ok_or(CandidateWriterError::Invariant(
                "reference parser work disappeared before output",
            ))?
            .take_output()
            .map_err(|_| CandidateWriterError::Invariant("reference parser output disappeared"))?;
        let cursor = drive.cursor.as_ref().ok_or(CandidateWriterError::Invariant(
            "reference cursor disappeared before output projection",
        ))?;
        let projected = self.with_short_session(arena, |builder, session| {
            cursor.project_reference_output(builder, session, drive.binding, output)
        })?;
        let (definition, source_range, label_range, destination_range, title_range, output_ack) =
            projected.into_parts();
        if definition.destination_transform != DirectReferenceValueTransform::CleanDestination
            || definition.title_transform
                != definition
                    .logical_title
                    .as_ref()
                    .map(|_| DirectReferenceValueTransform::CleanTitle)
        {
            return Err(CandidateWriterError::Invariant(
                "reference parser output selected an unsupported value transform",
            ));
        }
        // Projection already consumed and authenticated all field cuts. Only
        // destination/title need replay; source/label provenance deliberately
        // dies inside this unpublished occurrence transaction.
        drop(source_range);
        drop(label_range);
        let occurrence = PendingReferenceOccurrence {
            normalized_label: definition.normalized_label,
            title_range,
            destination: None,
            title: None,
            output_ack: Some(output_ack),
        };
        let value = self.begin_reference_value_transaction(
            arena,
            cursor,
            drive.binding,
            destination_range,
            source,
            ReferenceValueKind::Destination,
        )?;
        drive.stage = ReferenceDriveStage::Destination { occurrence, value };
        let _ = request_nonce;
        Ok(())
    }

    fn finish_reference_parser_work(
        &mut self,
        arena: &mut PageArena,
        mut drive: Box<ReferencePrefixDrive>,
        source: SourceProjectionSession,
    ) -> Result<ReferenceDriveOutcome, CandidateWriterError> {
        let work = drive.work.take().ok_or(CandidateWriterError::Invariant(
            "completed reference parser work disappeared",
        ))?;
        let output = work.take_terminal().map_err(|_| {
            CandidateWriterError::Invariant("reference parser terminal was not ready")
        })?;
        let cursor = drive.cursor.take().ok_or(CandidateWriterError::Invariant(
            "completed reference cursor disappeared",
        ))?;
        let identity = cursor.identity();
        let binding = drive.binding;
        let (terminal, seal) = self.with_short_session(arena, |builder, session| {
            let terminal =
                cursor.project_reference_terminal(builder, session, binding, output)?;
            let seal = cursor.into_transaction_seal(builder, session, binding)?;
            Ok::<_, ActiveParagraphProjectionError>((terminal, seal))
        })?;
        let retired_source = source
            .retire(identity.source(), identity.cursor_nonce())
            .map_err(|_| {
                CandidateWriterError::Invariant(
                    "reference projection source session did not retire cleanly",
                )
            })?;
        Ok(ReferenceDriveOutcome::Rewrite {
            binding,
            seal,
            terminal,
            retired_source,
        })
    }

    fn poll_reference_drive(
        &mut self,
        arena: &mut PageArena,
        request_nonce: u64,
        mut drive: Box<ReferencePrefixDrive>,
    ) -> Result<ReferenceDriveOutcome, CandidateWriterError> {
        let stage = std::mem::replace(&mut drive.stage, ReferenceDriveStage::Failed);
        match stage {
            ReferenceDriveStage::Failed => {
                return Err(CandidateWriterError::Invariant(
                    "reference drive entered a failed phase",
                ));
            }
            ReferenceDriveStage::Scan { mut source } => {
                let cursor = drive.cursor.as_mut().ok_or(CandidateWriterError::Invariant(
                    "reference scan lost its projection cursor",
                ))?;
                let progress = self.with_short_session(arena, |builder, session| {
                    cursor.poll_byte(builder, session, drive.binding, &mut source, false)
                })?;
                let parser_status = match progress {
                    ActiveParagraphProjectionPoll::Pending => None,
                    ActiveParagraphProjectionPoll::ByteReady => {
                        let identity = cursor.identity();
                        let mut projected = cursor.direct_source(identity)?;
                        Some(
                            drive
                                .work
                                .as_mut()
                                .ok_or(CandidateWriterError::Invariant(
                                    "reference scan lost parser work",
                                ))?
                                .poll_source(&mut projected, 1, false)
                                .map_err(Self::map_reference_poll_error)?
                                .status,
                        )
                    }
                    ActiveParagraphProjectionPoll::Complete => {
                        let identity = cursor.identity();
                        let mut projected = cursor.direct_source(identity)?;
                        Some(
                            drive
                                .work
                                .as_mut()
                                .ok_or(CandidateWriterError::Invariant(
                                    "reference EOF lost parser work",
                                ))?
                                .poll_source(&mut projected, 1, false)
                                .map_err(Self::map_reference_poll_error)?
                                .status,
                        )
                    }
                    ActiveParagraphProjectionPoll::Cancelled => {
                        return Err(CandidateWriterError::Invariant(
                            "reference projection scan was cancelled",
                        ));
                    }
                };
                match parser_status {
                    None | Some(DirectReferencePrefixPollStatus::NeedMore) => {
                        drive.stage = ReferenceDriveStage::Scan { source };
                    }
                    Some(DirectReferencePrefixPollStatus::OutputReady) => {
                        let source = source.finish()?;
                        self.begin_reference_occurrence(
                            arena,
                            &mut drive,
                            source,
                            request_nonce,
                        )?;
                    }
                    Some(DirectReferencePrefixPollStatus::Complete) => {
                        let source = source.finish()?;
                        return self.finish_reference_parser_work(arena, drive, source);
                    }
                    Some(DirectReferencePrefixPollStatus::Cancelled) => {
                        return Err(CandidateWriterError::Invariant(
                            "reference parser work was cancelled",
                        ));
                    }
                }
            }
            ReferenceDriveStage::DrainCursor { mut source } => {
                let cursor = drive.cursor.as_mut().ok_or(CandidateWriterError::Invariant(
                    "reference terminal drain lost its projection cursor",
                ))?;
                match self.with_short_session(arena, |builder, session| {
                    cursor.poll_byte(builder, session, drive.binding, &mut source, false)
                })? {
                    ActiveParagraphProjectionPoll::Pending => {
                        drive.stage = ReferenceDriveStage::DrainCursor { source };
                    }
                    ActiveParagraphProjectionPoll::ByteReady => {
                        let identity = cursor.identity();
                        let mut projected = cursor.direct_source(identity)?;
                        let offset = projected.available_len().checked_sub(1).ok_or(
                            CandidateWriterError::Invariant(
                                "reference terminal drain exposed no byte",
                            ),
                        )?;
                        let _discarded = projected.read_byte(offset)?;
                        drive.stage = ReferenceDriveStage::DrainCursor { source };
                    }
                    ActiveParagraphProjectionPoll::Complete => {
                        let source = source.finish()?;
                        return self.finish_reference_parser_work(arena, drive, source);
                    }
                    ActiveParagraphProjectionPoll::Cancelled => {
                        return Err(CandidateWriterError::Invariant(
                            "reference terminal projection drain was cancelled",
                        ));
                    }
                }
            }
            ReferenceDriveStage::Destination {
                mut occurrence,
                mut value,
            } => {
                let cursor = drive.cursor.as_ref().ok_or(CandidateWriterError::Invariant(
                    "reference destination lost its projection cursor",
                ))?;
                match self.poll_reference_value_transaction(
                    arena,
                    cursor,
                    drive.binding,
                    &mut value,
                )? {
                    None => {
                        drive.stage = ReferenceDriveStage::Destination { occurrence, value };
                    }
                    Some((blob, source)) => {
                        occurrence.destination = Some(blob);
                        if let Some(title) = occurrence.title_range.take() {
                            let value = self.begin_reference_value_transaction(
                                arena,
                                cursor,
                                drive.binding,
                                title,
                                source,
                                ReferenceValueKind::Title,
                            )?;
                            drive.stage = ReferenceDriveStage::Title { occurrence, value };
                        } else {
                            drive.stage = ReferenceDriveStage::BeginIntern { occurrence, source };
                        }
                    }
                }
            }
            ReferenceDriveStage::Title {
                mut occurrence,
                mut value,
            } => {
                let cursor = drive.cursor.as_ref().ok_or(CandidateWriterError::Invariant(
                    "reference title lost its projection cursor",
                ))?;
                match self.poll_reference_value_transaction(
                    arena,
                    cursor,
                    drive.binding,
                    &mut value,
                )? {
                    None => drive.stage = ReferenceDriveStage::Title { occurrence, value },
                    Some((blob, source)) => {
                        occurrence.title = Some(blob);
                        drive.stage = ReferenceDriveStage::BeginIntern { occurrence, source };
                    }
                }
            }
            ReferenceDriveStage::BeginIntern {
                mut occurrence,
                source,
            } => {
                let normalized = std::mem::take(&mut occurrence.normalized_label);
                let mut mint = ReferenceCandidateIndexWriterMint(());
                let label = CandidateReferenceLabel::from_writer_join(
                    normalized,
                    request_nonce,
                    &mut mint,
                )
                .map_err(|_| {
                    CandidateWriterError::Invariant(
                        "reference normalized label rejected writer join",
                    )
                })?;
                self.reference_semantic
                    .as_mut()
                    .ok_or(CandidateWriterError::Invariant(
                        "reference semantic state disappeared before interning",
                    ))?
                    .interner
                    .begin_intern(label)
                    .map_err(|_| {
                        CandidateWriterError::Invariant(
                            "reference interner rejected the parser label",
                        )
                    })?;
                drive.stage = ReferenceDriveStage::PollInterner { occurrence, source };
            }
            ReferenceDriveStage::PollInterner {
                mut occurrence,
                source,
            } => {
                let progress = self.with_reference_semantic_session(
                    arena,
                    |semantic, session| {
                        semantic.interner.poll(session).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference label interner failed during occurrence publication",
                            )
                        })
                    },
                )?;
                if progress == ReferenceLabelInternerProgress::LabelReady {
                    let destination = occurrence.destination.take().ok_or(
                        CandidateWriterError::Invariant(
                            "reference occurrence lost its cooked destination",
                        ),
                    )?;
                    let output_ack = occurrence.output_ack.take().ok_or(
                        CandidateWriterError::Invariant(
                            "reference occurrence lost its parser acknowledgement",
                        ),
                    )?;
                    let title = occurrence.title.take();
                    let mut mint = ReferenceCandidateIndexWriterMint(());
                    self.with_reference_semantic_session(arena, |semantic, session| {
                        let label = semantic.interner.take_label().map_err(|_| {
                            CandidateWriterError::Invariant(
                                "ready reference label disappeared",
                            )
                        })?;
                        let occurrence = WriterAuthenticatedReferenceOccurrence::from_writer_cooked(
                            label,
                            destination,
                            title,
                            &mut mint,
                        );
                        semantic.index.begin_occurrence(session, occurrence).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference index rejected writer-authenticated occurrence",
                            )
                        })
                    })?;
                    drive.stage = ReferenceDriveStage::PollIndex { output_ack, source };
                } else {
                    drive.stage = ReferenceDriveStage::PollInterner { occurrence, source };
                }
            }
            ReferenceDriveStage::PollIndex {
                output_ack,
                source,
            } => {
                let progress = self.with_reference_semantic_session(
                    arena,
                    |semantic, session| {
                        semantic.index.poll(session).map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference index failed during occurrence publication",
                            )
                        })
                    },
                )?;
                if progress == ReferenceCandidateIndexProgress::OccurrenceAckReady {
                    self.with_reference_semantic_session(arena, |semantic, _| {
                        let ack = semantic.index.take_occurrence_ack().map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference occurrence acknowledgement disappeared",
                            )
                        })?;
                        semantic
                            .interner
                            .acknowledge_label_use(ack.into_label_use())
                            .map_err(|_| {
                                CandidateWriterError::Invariant(
                                    "reference label-use acknowledgement crossed the interner",
                                )
                            })
                    })?;
                    let status = drive
                        .work
                        .as_mut()
                        .ok_or(CandidateWriterError::Invariant(
                            "reference parser work disappeared before occurrence ack",
                        ))?
                        .acknowledge_output(output_ack)
                        .map_err(|_| {
                            CandidateWriterError::Invariant(
                                "reference parser rejected durable occurrence ack",
                            )
                        })?;
                    let cursor = drive.cursor.as_ref().ok_or(
                        CandidateWriterError::Invariant(
                            "reference occurrence ack lost its projection cursor",
                        ),
                    )?;
                    let source = cursor.source_start()?.begin_source_pass(source)?;
                    drive.stage = match status {
                        DirectReferencePrefixOutputAckStatus::Rearmed => {
                            ReferenceDriveStage::Scan { source }
                        }
                        DirectReferencePrefixOutputAckStatus::Complete => {
                            ReferenceDriveStage::DrainCursor { source }
                        }
                    };
                } else {
                    drive.stage = ReferenceDriveStage::PollIndex { output_ack, source };
                }
            }
        }
        Ok(ReferenceDriveOutcome::Pending(drive))
    }

    pub(crate) fn start_reference_prefix(
        &mut self,
        epoch: LiveCandidateEpoch,
        paragraph: CandidateWriterBinding,
        request: DirectReferencePrefixRequest,
    ) -> Result<(), CandidateWriterError> {
        self.require_start(epoch)?;
        if paragraph.kind() != GreenKind::PARAGRAPH || request.rendezvous_id() == 0 {
            return Err(self.poison(CandidateWriterError::Invariant(
                "reference finalizer requires one active Paragraph rendezvous",
            )));
        }
        let block = paragraph.binding.block_id();
        let pending_terminator = self
            .ledger
            .pending_terminator_for(epoch, &paragraph.binding)
            .map_err(|error| self.poison(error.into()))?;
        if request.include_pending_terminator() != pending_terminator.is_some() {
            return Err(self.poison(CandidateWriterError::Invariant(
                "parser reference request crossed the writer staged terminator",
            )));
        }
        let mut group = self.active_paragraph.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "reference finalizer has no active Paragraph group",
            ))
        })?;
        if group.build != epoch.build_id()
            || group.block != block
            || group.promoted_setext
            || group.deferred_identity.is_some()
            || group.deferred_storage.is_some()
        {
            return Err(self.poison(CandidateWriterError::Invariant(
                "reference finalizer targets another Paragraph group",
            )));
        }
        let enter = group.enter.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "reference finalizer lost its provisional Paragraph Enter",
            ))
        })?;
        let projection_origin = group.projection_origin.take().ok_or_else(|| {
            self.poison(CandidateWriterError::Invariant(
                "reference finalizer lost its canonical projection origin",
            ))
        })?;
        self.action = Some(WriterAction::ReferencePrefix(ReferencePrefixJob {
            request,
            paragraph: Some(paragraph),
            enter: Some(enter),
            projection_origin: Some(projection_origin),
            phase: ReferencePrefixPhase::RequestStructuralFlush,
        }));
        Ok(())
    }

    pub(crate) fn install_reference_prefix_work(
        &mut self,
        epoch: LiveCandidateEpoch,
        identity: ActiveParagraphProjectionIdentity,
        work: DirectReferencePrefixWork<ActiveParagraphProjectionIdentity>,
        arena: &mut PageArena,
        source_store: &SourceStore,
    ) -> Result<(), CandidateWriterError> {
        self.require_ready(epoch)?;
        let action = self.action.take().ok_or(CandidateWriterError::NoAction)?;
        let WriterAction::ReferencePrefix(mut job) = action else {
            self.action = Some(action);
            return Err(CandidateWriterError::Busy);
        };
        let phase = job.phase;
        let ReferencePrefixPhase::AwaitParserWork { binding, cursor } = phase else {
            job.phase = phase;
            self.action = Some(WriterAction::ReferencePrefix(job));
            return Err(CandidateWriterError::Busy);
        };
        if identity != cursor.identity() {
            job.phase = ReferencePrefixPhase::AwaitParserWork { binding, cursor };
            self.action = Some(WriterAction::ReferencePrefix(job));
            return Err(self.poison(CandidateWriterError::Invariant(
                "reference parser work crossed its projection identity",
            )));
        }
        let source_session = self.with_short_session(arena, |builder, session| {
            cursor
                .open_source_projection_session(builder, session, binding, source_store)
        })?;
        let source = cursor.source_start()?.begin_source_pass(source_session)?;
        if self.reference_semantic.is_none() {
            self.reference_semantic = Some(CandidateReferenceSemanticTransaction::new(epoch)?);
        }
        job.phase = ReferencePrefixPhase::Drive {
            drive: Box::new(ReferencePrefixDrive {
                binding,
                cursor: Some(cursor),
                work: Some(work),
                stage: ReferenceDriveStage::Scan { source },
            }),
        };
        self.action = Some(WriterAction::ReferencePrefix(job));
        Ok(())
    }

    fn complete_reference_prefix_terminal(
        binding: Option<CandidateWriterBinding>,
        terminal: ActiveParagraphProjectedReferenceTerminal,
    ) -> (Option<WriterAction>, CandidateWriterProgress) {
        let identity = terminal.identity();
        let (_terminal, reference_prefix, recognition, ack) = terminal.into_parts();
        // These ranges authenticated the canonical rewrite but carry no
        // durable navigation authority. The parser receives only its linear
        // acknowledgement after both storage transactions have joined.
        drop(reference_prefix);
        drop(recognition);
        (
            None,
            CandidateWriterProgress::ReferencePrefixTerminalReady(
                CandidateReferencePrefixTerminal {
                    binding,
                    identity,
                    ack,
                },
            ),
        )
    }

    pub(super) fn poll_reference_prefix(
        &mut self,
        mut job: ReferencePrefixJob,
        arena: &mut PageArena,
    ) -> Result<(Option<WriterAction>, CandidateWriterProgress), CandidateWriterError> {
        match job.phase {
            ReferencePrefixPhase::RequestStructuralFlush => {
                let progress = self.composer_mut()?.flush_before_structure()?;
                job.phase = ReferencePrefixPhase::Drain(ComposerDrain::begin(progress, false)?);
            }
            ReferencePrefixPhase::Drain(mut drain) => {
                self.poll_drain(&mut drain, arena)?;
                job.phase = if drain.is_complete() {
                    ReferencePrefixPhase::BeginLeafBarrier
                } else {
                    ReferencePrefixPhase::Drain(drain)
                };
            }
            ReferencePrefixPhase::BeginLeafBarrier => {
                self.with_short_session(arena, |builder, session| {
                    builder.begin_leaf_barrier(session)
                })?;
                job.phase = ReferencePrefixPhase::AwaitLeafBarrier;
            }
            ReferencePrefixPhase::AwaitLeafBarrier => match self.poll_green_builder(arena)? {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => {
                    let cut = self.with_short_session(arena, |builder, session| {
                        builder.take_leaf_barrier_cut(session)
                    })?;
                    let barrier_generation = cut.events_before();
                    if barrier_generation == 0 {
                        return Err(CandidateWriterError::Invariant(
                            "reference projection barrier has zero generation",
                        ));
                    }
                    job.phase = ReferencePrefixPhase::BeginWorkingPrefixReduction {
                        barrier_generation,
                    };
                }
                SerializedGreenStreamProgress::ManifestReady => {
                    return Err(CandidateWriterError::Invariant(
                        "reference projection barrier reached a finished manifest",
                    ));
                }
            },
            ReferencePrefixPhase::BeginWorkingPrefixReduction {
                barrier_generation,
            } => {
                self.with_short_session(arena, |builder, session| {
                    builder.begin_working_prefix_reduction(session)
                })?;
                job.phase = ReferencePrefixPhase::AwaitWorkingPrefixReduction {
                    barrier_generation,
                };
            }
            ReferencePrefixPhase::AwaitWorkingPrefixReduction {
                barrier_generation,
            } => match self.poll_green_builder(arena)? {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => {
                    self.with_short_session(arena, |builder, session| {
                        builder.take_working_prefix_cut(session)
                    })?;
                    job.phase = ReferencePrefixPhase::OpenProjection {
                        barrier_generation,
                    };
                }
                SerializedGreenStreamProgress::ManifestReady => {
                    return Err(CandidateWriterError::Invariant(
                        "reference working-prefix reduction reached a finished manifest",
                    ));
                }
            },
            ReferencePrefixPhase::OpenProjection {
                barrier_generation,
            } => {
                let paragraph = job.paragraph.as_ref().ok_or(CandidateWriterError::Invariant(
                    "reference projection lost its Paragraph binding",
                ))?;
                let enter = job.enter.as_ref().ok_or(CandidateWriterError::Invariant(
                    "reference projection lost its Paragraph Enter",
                ))?;
                let origin = job.projection_origin.as_ref().ok_or(
                    CandidateWriterError::Invariant("reference projection lost its origin"),
                )?;
                let projection_generation = origin.projection_generation();
                let high_water = serialized_metric(self.ledger.physical_metric());
                let pending = self
                    .ledger
                    .pending_terminator_for(self.epoch, &paragraph.binding)?;
                if job.request.include_pending_terminator() != pending.is_some() {
                    return Err(CandidateWriterError::Invariant(
                        "reference projection staged terminator changed before cursor open",
                    ));
                }
                let staged = pending.map(|pending| staged_terminator(projection_generation, pending));
                let binding = ActorProjectionBinding::from_writer_join(
                    self.epoch.source(),
                    enter,
                    projection_generation,
                    high_water,
                    barrier_generation,
                );
                let cursor = self.with_short_session(arena, |builder, session| {
                    builder.open_active_paragraph_projection_cursor(
                        session, enter, binding, staged,
                    )
                })?;
                let identity = cursor.identity();
                job.phase = ReferencePrefixPhase::AwaitParserWork { binding, cursor };
                return Ok((
                    Some(WriterAction::ReferencePrefix(job)),
                    CandidateWriterProgress::ReferencePrefixSourceReady { identity },
                ));
            }
            ReferencePrefixPhase::AwaitParserWork { .. } => {
                let identity = match &job.phase {
                    ReferencePrefixPhase::AwaitParserWork { cursor, .. } => cursor.identity(),
                    _ => unreachable!(),
                };
                return Ok((
                    Some(WriterAction::ReferencePrefix(job)),
                    CandidateWriterProgress::ReferencePrefixSourceReady { identity },
                ));
            }
            ReferencePrefixPhase::Drive { drive } => {
                match self.poll_reference_drive(arena, job.request.rendezvous_id(), drive)? {
                    ReferenceDriveOutcome::Pending(drive) => {
                        job.phase = ReferencePrefixPhase::Drive { drive };
                    }
                    ReferenceDriveOutcome::Rewrite {
                        binding,
                        seal,
                        terminal,
                        retired_source,
                    } => {
                        job.phase = ReferencePrefixPhase::Rewrite {
                            binding,
                            seal,
                            terminal,
                            retired_source,
                        };
                    }
                }
            }
            ReferencePrefixPhase::Rewrite {
                binding,
                seal,
                terminal,
                retired_source,
            } => match terminal.terminal().disposition {
                DirectReferencePrefixDisposition::NoDefinitions => {
                    let paragraph = job.paragraph.take().ok_or(
                        CandidateWriterError::Invariant(
                            "unchanged reference terminal lost its Paragraph binding",
                        ),
                    )?;
                    let enter = job.enter.take().ok_or(CandidateWriterError::Invariant(
                        "unchanged reference terminal lost its Paragraph Enter",
                    ))?;
                    let origin = job.projection_origin.take().ok_or(
                        CandidateWriterError::Invariant(
                            "unchanged reference terminal lost its composer origin",
                        ),
                    )?;
                    let unchanged = self.with_short_session(arena, |builder, session| {
                        seal.validate_reference_unchanged(
                            builder,
                            session,
                            binding,
                            enter,
                            terminal,
                            retired_source,
                        )
                    })?;
                    let (enter, terminal, staged) = unchanged.into_parts();
                    if staged.is_some() != job.request.include_pending_terminator()
                        || self.active_paragraph.is_some()
                    {
                        return Err(CandidateWriterError::Invariant(
                            "unchanged reference terminal crossed its Paragraph continuation",
                        ));
                    }
                    self.active_paragraph = Some(ActiveParagraphNormalizationGroup {
                        build: self.epoch.build_id(),
                        block: paragraph.binding.block_id(),
                        enter: Some(enter),
                        projection_origin: Some(origin),
                        promoted_setext: false,
                        deferred_identity: None,
                        deferred_storage: None,
                    });
                    return Ok(Self::complete_reference_prefix_terminal(
                        Some(paragraph),
                        terminal,
                    ));
                }
                DirectReferencePrefixDisposition::VisibleRemainder => {
                    let paragraph = job.paragraph.take().ok_or(
                        CandidateWriterError::Invariant(
                            "visible reference rewrite lost its Paragraph binding",
                        ),
                    )?;
                    let enter = job.enter.take().ok_or(CandidateWriterError::Invariant(
                        "visible reference rewrite lost its Paragraph Enter",
                    ))?;
                    let origin = job.projection_origin.take().ok_or(
                        CandidateWriterError::Invariant(
                            "visible reference rewrite lost its composer origin",
                        ),
                    )?;
                    let block = paragraph.binding.block_id();
                    let split_suffix_coverage = self.mint_coverage()?;
                    let begin = self.with_short_session(arena, |builder, session| {
                        seal.begin_reference_visible_remainder(
                            builder,
                            session,
                            binding,
                            enter,
                            terminal,
                            retired_source,
                            block,
                            GreenKind::PARAGRAPH,
                            split_suffix_coverage,
                        )
                    })?;
                    self.composer_mut()?
                        .begin_canonical_fragment_replacement(origin, begin.green_physical())?;
                    job.phase = ReferencePrefixPhase::DriveRewrite {
                        rewrite: Box::new(ReferenceCanonicalRewrite {
                            begin,
                            mode: ReferenceCanonicalMode::Visible { paragraph },
                            awaiting: ReferenceRewriteAwaiting::FragmentStart,
                            survivor_offered: false,
                        }),
                    };
                }
                DirectReferencePrefixDisposition::ReferenceOnly => {
                    if job.request.context() != DirectReferencePrefixContext::ParagraphFinalization {
                        return Err(CandidateWriterError::Invariant(
                            "Setext reference-only normalization requires an empty Paragraph shell",
                        ));
                    }
                    let paragraph = job.paragraph.take().ok_or(
                        CandidateWriterError::Invariant(
                            "reference-only rewrite lost its Paragraph binding",
                        ),
                    )?;
                    let enter = job.enter.take().ok_or(CandidateWriterError::Invariant(
                        "reference-only rewrite lost its Paragraph Enter",
                    ))?;
                    let origin = job.projection_origin.take().ok_or(
                        CandidateWriterError::Invariant(
                            "reference-only rewrite lost its composer origin",
                        ),
                    )?;
                    let retired_block = paragraph.binding.block_id();
                    let retirement = self
                        .ledger
                        .retire_reference_only_paragraph(self.epoch, paragraph.binding)?;
                    let (parent, terminator_gap) = retirement.into_parts();
                    let parent = CandidateWriterBinding { binding: parent };
                    let parent_block = parent.binding.block_id();
                    let parent_kind = parent.kind();
                    let begin = self.with_short_session(arena, |builder, session| {
                        seal.begin_reference_only_removal(
                            builder,
                            session,
                            binding,
                            enter,
                            terminal,
                            retired_source,
                            parent_block,
                            parent_kind,
                        )
                    })?;
                    self.composer_mut()?
                        .begin_canonical_fragment_replacement(origin, begin.green_physical())?;
                    self.donor_checkpoint_samples
                        .finish_paragraph_group(retired_block)?;
                    job.phase = ReferencePrefixPhase::DriveRewrite {
                        rewrite: Box::new(ReferenceCanonicalRewrite {
                            begin,
                            mode: ReferenceCanonicalMode::ReferenceOnly {
                                retired_block,
                                parent,
                                terminator_gap,
                            },
                            awaiting: ReferenceRewriteAwaiting::FragmentStart,
                            survivor_offered: false,
                        }),
                    };
                }
            },
            ReferencePrefixPhase::DriveRewrite { mut rewrite } => {
                if rewrite.awaiting != ReferenceRewriteAwaiting::None {
                    if self.poll_green_acknowledgement(arena)? {
                        if rewrite.awaiting == ReferenceRewriteAwaiting::Survivor {
                            match &rewrite.mode {
                                ReferenceCanonicalMode::Visible { .. } => {}
                                ReferenceCanonicalMode::ReferenceOnly { .. } => {
                                    return Err(CandidateWriterError::Invariant(
                                        "reference-only rewrite emitted a Paragraph survivor",
                                    ));
                                }
                            }
                            if rewrite.survivor_offered {
                                return Err(CandidateWriterError::Invariant(
                                    "visible reference rewrite emitted two Paragraph survivors",
                                ));
                            }
                            rewrite.survivor_offered = true;
                        }
                        rewrite.awaiting = ReferenceRewriteAwaiting::None;
                    }
                    job.phase = ReferencePrefixPhase::DriveRewrite { rewrite };
                } else {
                    let progress = self.with_short_session(arena, |builder, session| {
                        rewrite.begin.poll_reference_rewrite(builder, session)
                    })?;
                    match progress {
                        ActiveParagraphCanonicalRewriteProgress::Pending => {
                            job.phase = ReferencePrefixPhase::DriveRewrite { rewrite };
                        }
                        ActiveParagraphCanonicalRewriteProgress::EventOffered => {
                            rewrite.awaiting = ReferenceRewriteAwaiting::Event;
                            job.phase = ReferencePrefixPhase::DriveRewrite { rewrite };
                        }
                        ActiveParagraphCanonicalRewriteProgress::SurvivingParagraphEnterOffered => {
                            rewrite.awaiting = ReferenceRewriteAwaiting::Survivor;
                            job.phase = ReferencePrefixPhase::DriveRewrite { rewrite };
                        }
                        ActiveParagraphCanonicalRewriteProgress::Complete => {
                            self.with_short_session(
                                arena,
                                ResumableSerializedGreenBuild::finish_canonical_fragment_replacement,
                            )?;
                            let ReferenceCanonicalRewrite {
                                begin,
                                mode,
                                awaiting: _,
                                survivor_offered,
                            } = *rewrite;
                            job.phase = ReferencePrefixPhase::AwaitRewriteCommit {
                                begin,
                                mode,
                                survivor_offered,
                            };
                        }
                    }
                }
            }
            ReferencePrefixPhase::AwaitRewriteCommit {
                begin,
                mode,
                survivor_offered,
            } => {
                if !self.poll_green_acknowledgement(arena)? {
                    job.phase = ReferencePrefixPhase::AwaitRewriteCommit {
                        begin,
                        mode,
                        survivor_offered,
                    };
                } else {
                    let storage = self.with_short_session(arena, |builder, session| match &mode {
                        ReferenceCanonicalMode::Visible { paragraph } => builder
                            .take_canonical_fragment_replacement(
                                session,
                                paragraph.binding.block_id(),
                            ),
                        ReferenceCanonicalMode::ReferenceOnly { parent, .. } => builder
                            .take_canonical_fragment_removal(
                                session,
                                parent.binding.block_id(),
                            ),
                    })?;
                    let expected_physical = begin.green_physical();
                    let (terminal, staged, survivor_seed) = begin.into_parts()?;
                    let (rebase, survivor_origin) = match &mode {
                        ReferenceCanonicalMode::Visible { .. } => {
                            let seed = survivor_seed.ok_or(CandidateWriterError::Invariant(
                                "visible reference rewrite lost its authenticated survivor seed",
                            ))?;
                            let (rebase, origin) = self
                                .composer_mut()?
                                .finish_canonical_fragment_replacement_with_survivor(
                                    &storage, seed,
                                )?;
                            (rebase, Some(origin))
                        }
                        ReferenceCanonicalMode::ReferenceOnly { .. } => {
                            if survivor_seed.is_some() {
                                return Err(CandidateWriterError::Invariant(
                                    "reference-only rewrite minted a Paragraph survivor seed",
                                ));
                            }
                            (
                                self.composer_mut()?
                                    .finish_canonical_fragment_replacement(&storage)?,
                                None,
                            )
                        }
                    };
                    if storage.build_id() != self.epoch.build_id()
                        || storage.physical_metric() != expected_physical
                        || rebase.build_id() != self.epoch.build_id()
                        || rebase.physical_metric() != expected_physical
                        || rebase.retired_projection_runs() != storage.retired_coverage_runs()
                        || rebase.installed_projection_runs()
                            != storage.replacement_coverage_runs()
                    {
                        return Err(CandidateWriterError::Invariant(
                            "reference canonical rewrite crossed the Green/composer join",
                        ));
                    }
                    self.green_runs_acknowledged = rebase.canonical_suffix_projection_runs();
                    match mode {
                        ReferenceCanonicalMode::Visible { paragraph } => {
                            if !survivor_offered {
                                return Err(CandidateWriterError::Invariant(
                                    "visible reference rewrite lost its Paragraph survivor",
                                ));
                            }
                            let block = paragraph.binding.block_id();
                            let survivor = self.with_short_session(arena, |builder, session| {
                                builder.take_provisional_paragraph_enter(session, block)
                            })?;
                            let origin = survivor_origin.ok_or(CandidateWriterError::Invariant(
                                "visible reference rewrite lost its typed composer origin",
                            ))?;
                            if storage.retired_block() != block
                                || storage.replacement_block() != block
                                || storage.replacement_kind() != GreenKind::PARAGRAPH
                                || storage.removed_terminal()
                                || staged.is_some()
                                    != job.request.include_pending_terminator()
                                || self.active_paragraph.is_some()
                            {
                                return Err(CandidateWriterError::Invariant(
                                    "visible reference rewrite crossed its Paragraph continuation",
                                ));
                            }
                            self.active_paragraph = Some(ActiveParagraphNormalizationGroup {
                                build: self.epoch.build_id(),
                                block,
                                enter: Some(survivor),
                                projection_origin: Some(origin),
                                promoted_setext: false,
                                deferred_identity: None,
                                deferred_storage: None,
                            });
                            return Ok(Self::complete_reference_prefix_terminal(
                                Some(paragraph),
                                terminal,
                            ));
                        }
                        ReferenceCanonicalMode::ReferenceOnly {
                            retired_block,
                            parent,
                            terminator_gap,
                        } => {
                            if survivor_origin.is_some()
                                || survivor_offered
                                || storage.retired_block() != retired_block
                                || storage.replacement_block() != parent.binding.block_id()
                                || storage.replacement_kind() != parent.kind()
                                || !storage.removed_terminal()
                                || staged.is_some() != terminator_gap.is_some()
                            {
                                return Err(CandidateWriterError::Invariant(
                                    "reference-only rewrite crossed its staged terminator",
                                ));
                            }
                            if let Some(piece) = terminator_gap {
                                let progress = self.composer_mut()?.push_piece(piece)?;
                                job.phase = ReferencePrefixPhase::DrainReferenceOnlyGap {
                                    terminal,
                                    drain: ComposerDrain::begin(progress, false)?,
                                };
                            } else {
                                return Ok(Self::complete_reference_prefix_terminal(None, terminal));
                            }
                        }
                    }
                }
            }
            ReferencePrefixPhase::DrainReferenceOnlyGap {
                terminal,
                mut drain,
            } => {
                self.poll_drain(&mut drain, arena)?;
                if drain.is_complete() {
                    return Ok(Self::complete_reference_prefix_terminal(None, terminal));
                }
                job.phase = ReferencePrefixPhase::DrainReferenceOnlyGap { terminal, drain };
            }
        }
        Ok((
            Some(WriterAction::ReferencePrefix(job)),
            CandidateWriterProgress::Pending,
        ))
    }
}

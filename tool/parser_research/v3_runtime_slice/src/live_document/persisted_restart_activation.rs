//! Actor-owned admission for one persisted nonzero restart source phase.
//!
//! Parent selection, current-source lineage, ledger reconstruction, and donor
//! resume remain inside [`LiveDocumentStore`]. The scheduler receives only a
//! copyable wakeup handle and copyable work receipts; it never owns a source
//! cursor, parser, checkpoint recipe, normalized green path, or candidate
//! epoch authority.

use super::*;
use crate::retained_restart_coordinate::{
    PersistedRestartCoordinateAuthority, PersistedRestartCoordinateJob,
    PersistedRestartCoordinateProgress, PersistedRestartSourceReceipt,
    RetainedRestartCoordinateError,
};
use crate::storage_only_composite_document::{
    RestartCompositeAdoptionRetentionFailure, RestartCompositeDocumentError,
    RestartParentDonorResumeError,
};
use crate::{
    DonorResumedPersistedSourceLedger, ParentSelectedPersistedSourceActivation,
    PersistedSourceAdoptionJoinError, PersistedSourceDonorResumeError,
    PersistedSourceLedgerReconstructionReceipt,
};

/// Candidate-owned source phase. Cancellation drops this enum in the same
/// actor turn that detaches the candidate, so no current Crop cursor can
/// survive outside source-retirement accounting.
pub(super) enum PersistedRestartSourcePhase {
    Inactive,
    Resolving {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        coordinate: PersistedRestartCoordinateJob,
    },
    Ready {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        source: DonorResumedPersistedSourceLedger,
        receipt: PersistedRestartReadyReceipt,
    },
    /// The exact parent selected during source reconstruction has now branded
    /// both children retained into this candidate's still-suspended journal.
    /// The scheduler owns only its copyable handle; source, parser, normalized
    /// path, parent stamp, child owners, and inverse authority remain one
    /// actor-owned linear value.
    ParentSelected {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        source: ParentSelectedPersistedSourceActivation,
        receipt: PersistedRestartParentSelectionReceipt,
    },
    /// The source-selected parent, donor parser, current source ledger, and
    /// retained green prefix are advancing inside the same resumed candidate
    /// journal. No part of this job crosses the actor boundary.
    WriterRestarting {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        restart: crate::ParentSelectedCandidateWriterRestart,
        receipt: PersistedRestartParentSelectionReceipt,
    },
    /// The retained prefix has been installed into CandidateWriter. The
    /// exact parser state and branded two-child adoption lease remain here
    /// until the actor-owned suffix driver and final composite splice consume
    /// them together.
    WriterInstalled {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        driver: crate::ParentSelectedCandidateWriterDriver,
        receipt: PersistedRestartWriterInstallReceipt,
    },
    /// The exact donor parser and writer are rejoined and then advanced only
    /// inside this actor. Completion parks before independent commit while the
    /// opaque retained-parent tail waits for checkpoint-index suffix splice.
    ExactDriving {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        driver: crate::exact_block_job::ParentSelectedExactBlockDriver,
        receipt: PersistedRestartWriterInstallReceipt,
    },
    /// Matched C, adopted source/composer state, green suffix, checkpoint
    /// suffix, and retained parent now advance as one lifetime-free job. The
    /// ordinary candidate ticket/allocator slots again own cancellation.
    AdoptionSplicing {
        activation: NonZeroU64,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
        job: crate::ParentSelectedAdoptionSpliceJob,
        receipt: PersistedRestartWriterInstallReceipt,
    },
}

impl fmt::Debug for PersistedRestartSourcePhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inactive => formatter.write_str("Inactive"),
            Self::Resolving {
                activation,
                parent_epoch,
                config,
                ..
            } => formatter
                .debug_struct("Resolving")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .finish_non_exhaustive(),
            Self::Ready {
                activation,
                parent_epoch,
                config,
                receipt,
                ..
            } => formatter
                .debug_struct("Ready")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .field("receipt", receipt)
                .field("source", &"opaque donor-resumed persisted source")
                .finish(),
            Self::ParentSelected {
                activation,
                parent_epoch,
                config,
                receipt,
                ..
            } => formatter
                .debug_struct("ParentSelected")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .field("receipt", receipt)
                .field("source", &"opaque parent-selected persisted activation")
                .finish(),
            Self::WriterRestarting {
                activation,
                parent_epoch,
                config,
                receipt,
                ..
            } => formatter
                .debug_struct("WriterRestarting")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .field("receipt", receipt)
                .field("restart", &"opaque actor-owned retained writer restart")
                .finish(),
            Self::WriterInstalled {
                activation,
                parent_epoch,
                config,
                receipt,
                ..
            } => formatter
                .debug_struct("WriterInstalled")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .field("receipt", receipt)
                .field("driver", &"opaque actor-owned exact suffix driver")
                .finish(),
            Self::ExactDriving {
                activation,
                parent_epoch,
                config,
                receipt,
                ..
            } => formatter
                .debug_struct("ExactDriving")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .field("receipt", receipt)
                .field("driver", &"opaque actor-owned exact suffix state machine")
                .finish(),
            Self::AdoptionSplicing {
                activation,
                parent_epoch,
                config,
                job,
                receipt,
            } => formatter
                .debug_struct("AdoptionSplicing")
                .field("activation", activation)
                .field("parent_epoch", parent_epoch)
                .field("config", config)
                .field("receipt", receipt)
                .field("job", job)
                .finish(),
        }
    }
}

impl PersistedRestartSourcePhase {
    const fn activation(&self) -> Option<NonZeroU64> {
        match self {
            Self::Inactive => None,
            Self::Resolving { activation, .. }
            | Self::Ready { activation, .. }
            | Self::ParentSelected { activation, .. }
            | Self::WriterRestarting { activation, .. }
            | Self::WriterInstalled { activation, .. }
            | Self::ExactDriving { activation, .. }
            | Self::AdoptionSplicing { activation, .. } => Some(*activation),
        }
    }

    const fn parent_and_config(&self) -> Option<(LiveCandidateEpoch, CandidateWriterConfig)> {
        match self {
            Self::Inactive => None,
            Self::Resolving {
                parent_epoch,
                config,
                ..
            }
            | Self::Ready {
                parent_epoch,
                config,
                ..
            }
            | Self::ParentSelected {
                parent_epoch,
                config,
                ..
            }
            | Self::WriterRestarting {
                parent_epoch,
                config,
                ..
            }
            | Self::WriterInstalled {
                parent_epoch,
                config,
                ..
            }
            | Self::ExactDriving {
                parent_epoch,
                config,
                ..
            }
            | Self::AdoptionSplicing {
                parent_epoch,
                config,
                ..
            } => Some((*parent_epoch, *config)),
        }
    }

    pub(super) fn cursor_offset(&self) -> Option<usize> {
        match self {
            Self::Ready { source, .. } => Some(source.cursor_offset()),
            Self::ParentSelected { receipt, .. } => Some(receipt.cursor_offset),
            Self::WriterRestarting { restart, .. } => Some(restart.cursor_offset()),
            Self::Inactive
            | Self::Resolving { .. }
            | Self::WriterInstalled { .. }
            | Self::ExactDriving { .. } => None,
            Self::AdoptionSplicing { .. } => None,
        }
    }

    /// Extracts the only phase-local checkpoint chain whose length can scale
    /// with the changed suffix. Earlier phases either own no writer samples or
    /// keep them in `CandidateWriterSlot`, which the ordinary cancel path
    /// retires separately.
    pub(super) fn into_heap_retirement(
        self,
    ) -> crate::candidate_writer::CandidateWriterHeapRetirement {
        match self {
            Self::AdoptionSplicing { job, .. } => job.into_heap_retirement(),
            other => {
                drop(other);
                crate::candidate_writer::CandidateWriterHeapRetirement::empty()
            }
        }
    }
}

/// Copyable scheduler identity. It can request actor work but owns none of the
/// linear parse transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistedRestartActivationHandle {
    epoch: LiveCandidateEpoch,
    activation: NonZeroU64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistedRestartReadyReceipt {
    pub(crate) source: PersistedRestartSourceReceipt,
    pub(crate) reconstruction: PersistedSourceLedgerReconstructionReceipt,
}

/// Bounded receipt for the first mutating restart admission. Retaining the
/// two parent children is constant-size; no parent pages, source bytes, or
/// checkpoint samples are copied or scanned here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistedRestartParentSelectionReceipt {
    pub(crate) ready: PersistedRestartReadyReceipt,
    pub(crate) cursor_offset: usize,
    pub(crate) retained_child_owners: usize,
    pub(crate) parent_manifest_validations: usize,
    pub(crate) source_bytes_copied: usize,
    pub(crate) parent_pages_copied: usize,
}

/// Bounded receipt for installing a nonzero retained prefix into the ordinary
/// CandidateWriter. Counts are observations from the opaque driver, not
/// caller-supplied coordinates; the driver and its parent lease remain inside
/// the actor after this receipt is copied out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistedRestartWriterInstallReceipt {
    pub(crate) parent: PersistedRestartParentSelectionReceipt,
    pub(crate) cursor_offset: usize,
    pub(crate) open_bindings: usize,
    pub(crate) acknowledged_lines: u64,
    pub(crate) green:
        crate::serialized_green::setext_retained_restart::SetextRetainedGreenRestartReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistedRestartActivationProgress {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    /// The preferred checkpoint no longer belongs to an unchanged prefix. No
    /// candidate state was consumed, so this same fresh candidate can start
    /// the ordinary byte-zero parse.
    ZeroFallback,
    Ready(PersistedRestartReadyReceipt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PersistedRestartWriterProgress {
    Pending,
    Installed(PersistedRestartWriterInstallReceipt),
}

#[derive(Debug)]
pub(crate) enum PersistedRestartActivationError {
    Actor(LiveDocumentError),
    Parent(RestartCompositeDocumentError),
    ParentDonor(RestartParentDonorResumeError),
    Coordinate(RetainedRestartCoordinateError),
    SourceDonor(PersistedSourceDonorResumeError),
    NoCommittedParent,
    NoCheckpointAtOrBeforeHint,
    WrongPhase,
    /// The actor had already retained parent children, so a failed brand is
    /// fail-closed: the entire candidate journal is detached and must be
    /// drained through the returned abort handle.
    ParentSelectionAborted {
        error: PersistedSourceAdoptionJoinError,
        cleanup_error: Option<RestartCompositeDocumentError>,
        abort: CandidateAbort,
    },
    AdmissionAborted {
        error: LiveDocumentError,
        abort: CandidateAbort,
    },
    ParentRetentionAborted {
        error: RestartCompositeDocumentError,
        cleanup_error: Option<RestartCompositeDocumentError>,
        abort: CandidateAbort,
    },
    /// A source/green/writer validation failed after the candidate journal
    /// retained parent children. The entire candidate is already in the
    /// bounded abort queue; byte-zero fallback is no longer legal.
    WriterRestartAborted {
        error: CandidateWriterError,
        abort: CandidateAbort,
    },
    /// Abort admission itself rejected the still-linear ticket. The actor
    /// keeps a cancellation-only candidate so a later cancellation can retry;
    /// no parser or parent authority escapes in this error.
    WriterRestartCancellationFailed {
        error: CandidateWriterError,
        cleanup_error: LiveDocumentError,
    },
    ExactDriverAborted {
        failure: Box<crate::exact_block_job::ParentSelectedExactBlockAbortRequired>,
        abort: CandidateAbort,
    },
    ExactDriverCancellationFailed {
        failure: Box<crate::exact_block_job::ParentSelectedExactBlockAbortRequired>,
        cleanup_error: LiveDocumentError,
    },
    AdoptionSpliceAborted {
        error: CandidateWriterError,
        abort: CandidateAbort,
    },
    AdoptionSpliceCancellationFailed {
        error: CandidateWriterError,
        cleanup_error: LiveDocumentError,
    },
    AdoptionCommitAborted {
        error: CandidateWriterError,
        abort: CandidateAbort,
    },
    AdoptionCommitCancellationFailed {
        error: CandidateWriterError,
        cleanup_error: LiveDocumentError,
    },
}

impl fmt::Display for PersistedRestartActivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Actor(error) => error.fmt(formatter),
            Self::Parent(error) => error.fmt(formatter),
            Self::ParentDonor(error) => write!(formatter, "parent donor restart failed: {error:?}"),
            Self::Coordinate(error) => write!(formatter, "restart coordinate failed: {error:?}"),
            Self::SourceDonor(error) => write!(formatter, "source donor resume failed: {error:?}"),
            Self::NoCommittedParent => formatter.write_str("no committed restart parent exists"),
            Self::NoCheckpointAtOrBeforeHint => {
                formatter.write_str("no donor checkpoint exists at or before the source hint")
            }
            Self::WrongPhase => formatter.write_str("persisted restart source phase is not active"),
            Self::ParentSelectionAborted {
                error,
                cleanup_error,
                ..
            } => write!(
                formatter,
                "persisted restart parent selection failed closed: {error:?}; cleanup: {cleanup_error:?}"
            ),
            Self::AdmissionAborted { error, .. } => write!(
                formatter,
                "persisted restart admission failed after journal mutation: {error}"
            ),
            Self::ParentRetentionAborted {
                error,
                cleanup_error,
                ..
            } => write!(
                formatter,
                "persisted restart parent retention failed closed: {error}; cleanup: {cleanup_error:?}"
            ),
            Self::WriterRestartAborted { error, .. } => write!(
                formatter,
                "persisted CandidateWriter restart failed closed: {error}"
            ),
            Self::WriterRestartCancellationFailed {
                error,
                cleanup_error,
            } => write!(
                formatter,
                "persisted CandidateWriter restart failed: {error}; cancellation admission: {cleanup_error}"
            ),
            Self::ExactDriverAborted { failure, .. } => write!(
                formatter,
                "persisted exact suffix driver failed closed at {:?}: {:?}",
                failure.stage(),
                failure.error()
            ),
            Self::ExactDriverCancellationFailed {
                failure,
                cleanup_error,
            } => write!(
                formatter,
                "persisted exact suffix driver failed at {:?}: {:?}; cancellation admission: {cleanup_error}",
                failure.stage(),
                failure.error()
            ),
            Self::AdoptionSpliceAborted { error, .. } => {
                write!(
                    formatter,
                    "persisted adoption splice failed closed: {error}"
                )
            }
            Self::AdoptionSpliceCancellationFailed {
                error,
                cleanup_error,
            } => write!(
                formatter,
                "persisted adoption splice failed: {error}; cancellation admission: {cleanup_error}"
            ),
            Self::AdoptionCommitAborted { error, .. } => write!(
                formatter,
                "persisted adoption commit failed closed: {error}"
            ),
            Self::AdoptionCommitCancellationFailed {
                error,
                cleanup_error,
            } => write!(
                formatter,
                "persisted adoption commit failed: {error}; cancellation admission: {cleanup_error}"
            ),
        }
    }
}

impl std::error::Error for PersistedRestartActivationError {}

impl From<LiveDocumentError> for PersistedRestartActivationError {
    fn from(error: LiveDocumentError) -> Self {
        Self::Actor(error)
    }
}

impl From<RestartCompositeDocumentError> for PersistedRestartActivationError {
    fn from(error: RestartCompositeDocumentError) -> Self {
        Self::Parent(error)
    }
}

impl From<RestartParentDonorResumeError> for PersistedRestartActivationError {
    fn from(error: RestartParentDonorResumeError) -> Self {
        Self::ParentDonor(error)
    }
}

impl From<RetainedRestartCoordinateError> for PersistedRestartActivationError {
    fn from(error: RetainedRestartCoordinateError) -> Self {
        Self::Coordinate(error)
    }
}

impl From<PersistedSourceDonorResumeError> for PersistedRestartActivationError {
    fn from(error: PersistedSourceDonorResumeError) -> Self {
        Self::SourceDonor(error)
    }
}

impl LiveDocumentStore {
    /// Selects the newest authenticated parent checkpoint at or before one
    /// performance hint and starts the current-source lineage proof.
    ///
    /// `source_cut_hint` is not authority: the committed parent chooses and
    /// validates the actual sample. `config` is frozen here and rechecked
    /// against the parent on every poll and again at retained-green install.
    pub(crate) fn begin_persisted_restart_activation(
        &mut self,
        epoch: LiveCandidateEpoch,
        source_cut_hint: u64,
        config: CandidateWriterConfig,
    ) -> Result<PersistedRestartActivationHandle, PersistedRestartActivationError> {
        self.require_pristine_persisted_restart_slot(epoch)?;
        let parent = self
            .latest_restart_document
            .as_ref()
            .ok_or(PersistedRestartActivationError::NoCommittedParent)?;
        let parent_epoch = parent.epoch;
        self.validate_persisted_restart_parent(epoch, parent_epoch, config)?;
        let selected = parent
            .published
            .locate_donor_checkpoint_at_or_before_cut(
                &self.coordinator,
                &self.arena,
                source_cut_hint,
            )?
            .ok_or(PersistedRestartActivationError::NoCheckpointAtOrBeforeHint)?;
        let mint = selected.into_source_ledger_restart_mint()?;
        let coordinate = PersistedRestartCoordinateJob::begin(&self.source, mint)?;

        let activation = self.next_retained_activation;
        let next = activation
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(PersistedRestartActivationError::Actor(
                LiveDocumentError::Invariant("retained activation identity exhausted"),
            ))?;
        let candidate = self
            .candidate
            .as_mut()
            .expect("persisted restart candidate was validated");
        candidate.retained_activation = Some(activation);
        candidate.persisted_restart = PersistedRestartSourcePhase::Resolving {
            activation,
            parent_epoch,
            config,
            coordinate,
        };
        self.next_retained_activation = next;
        Ok(PersistedRestartActivationHandle { epoch, activation })
    }

    /// Advances only actor-owned scalar lineage work. Terminal reconstruction
    /// and donor resume occur in this same turn and the resulting source
    /// cursors remain inside `CandidateJob`.
    pub(crate) fn poll_persisted_restart_activation(
        &mut self,
        handle: PersistedRestartActivationHandle,
        fuel: usize,
    ) -> Result<PersistedRestartActivationProgress, PersistedRestartActivationError> {
        let (parent_epoch, config) = self.require_persisted_restart_poll_slot(handle)?;
        self.validate_persisted_restart_parent(handle.epoch, parent_epoch, config)?;
        if let PersistedRestartSourcePhase::Ready { receipt, .. } = &self
            .candidate
            .as_ref()
            .expect("persisted restart candidate was validated")
            .persisted_restart
        {
            return Ok(PersistedRestartActivationProgress::Ready(*receipt));
        }

        let progress = {
            let source = &self.source;
            let candidate = self
                .candidate
                .as_mut()
                .expect("persisted restart candidate was validated");
            let PersistedRestartSourcePhase::Resolving { coordinate, .. } =
                &mut candidate.persisted_restart
            else {
                return Err(PersistedRestartActivationError::WrongPhase);
            };
            coordinate.poll(source, fuel)
        };
        let progress = match progress {
            Ok(progress) => progress,
            Err(error) => {
                self.clear_persisted_restart_phase(handle)?;
                return Err(error.into());
            }
        };
        match progress {
            PersistedRestartCoordinateProgress::Pending {
                processed_records,
                remaining_records,
            } => Ok(PersistedRestartActivationProgress::Pending {
                processed_records,
                remaining_records,
            }),
            PersistedRestartCoordinateProgress::Ready(authority) => {
                let restored = match *authority {
                    PersistedRestartCoordinateAuthority::ZeroFallback(_) => {
                        self.clear_persisted_restart_phase(handle)?;
                        return Ok(PersistedRestartActivationProgress::ZeroFallback);
                    }
                    PersistedRestartCoordinateAuthority::PreferredDeferredLf(preferred) => {
                        CandidateSourceLedger::restore_parent_selected_lf(handle.epoch, preferred)
                    }
                    PersistedRestartCoordinateAuthority::PreferredSourceCompleteLineBoundary(
                        preferred,
                    ) => {
                        CandidateSourceLedger::restore_parent_selected_source_complete_line_boundary(
                            handle.epoch,
                            preferred,
                        )
                    }
                };
                let restored = match restored {
                    Ok(restored) => restored,
                    Err(error) => {
                        self.clear_persisted_restart_phase(handle)?;
                        return Err(PersistedRestartActivationError::Actor(
                            LiveDocumentError::SourceLedger(error),
                        ));
                    }
                };
                let source = match restored.resume_parent_selected_donor() {
                    Ok(source) => source,
                    Err(error) => {
                        self.clear_persisted_restart_phase(handle)?;
                        return Err(error.into());
                    }
                };
                let receipt = PersistedRestartReadyReceipt {
                    source: source.source_receipt(),
                    reconstruction: source.reconstruction_receipt(),
                };
                self.candidate
                    .as_mut()
                    .expect("persisted restart candidate was validated")
                    .persisted_restart = PersistedRestartSourcePhase::Ready {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    source,
                    receipt,
                };
                Ok(PersistedRestartActivationProgress::Ready(receipt))
            }
        }
    }

    /// Performs the first mutating restart step inside the live actor. The
    /// exact parent chosen by source reconstruction retains both of its
    /// storage children into the candidate journal, then brands that lease
    /// with the non-cloneable selection stamp already carried by the resumed
    /// source/parser/path bundle.
    ///
    /// On success the journal is immediately suspended again and every linear
    /// capability remains in `CandidateJob`. Any storage result which cannot
    /// certify an untouched journal fails closed by detaching the candidate
    /// and returning its bounded abort.
    pub(crate) fn select_persisted_restart_parent_for_adoption(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<PersistedRestartParentSelectionReceipt, PersistedRestartActivationError> {
        #[cfg(test)]
        {
            self.select_persisted_restart_parent_for_adoption_inner(handle, None)
        }
        #[cfg(not(test))]
        {
            self.select_persisted_restart_parent_for_adoption_inner(handle)
        }
    }

    /// Test-only crossed-parent probe. It exercises the exact production
    /// Ready-to-ParentSelected transition while deliberately retaining an
    /// equal-cut parent other than the one whose non-cloneable selection stamp
    /// travelled through source reconstruction and donor resume.
    #[cfg(test)]
    pub(crate) fn select_crossed_persisted_restart_parent_for_test(
        &mut self,
        handle: PersistedRestartActivationHandle,
        crossed_parent: &RestartCompositeDocument,
    ) -> Result<PersistedRestartParentSelectionReceipt, PersistedRestartActivationError> {
        self.select_persisted_restart_parent_for_adoption_inner(handle, Some(crossed_parent))
    }

    fn select_persisted_restart_parent_for_adoption_inner(
        &mut self,
        handle: PersistedRestartActivationHandle,
        #[cfg(test)] crossed_parent_for_test: Option<&RestartCompositeDocument>,
    ) -> Result<PersistedRestartParentSelectionReceipt, PersistedRestartActivationError> {
        let (parent_epoch, config) = self.require_persisted_restart_poll_slot(handle)?;
        self.validate_persisted_restart_parent(handle.epoch, parent_epoch, config)?;
        if !matches!(
            self.candidate
                .as_ref()
                .expect("persisted restart candidate was validated")
                .persisted_restart,
            PersistedRestartSourcePhase::Ready { .. }
        ) {
            return Err(PersistedRestartActivationError::WrongPhase);
        }

        let (ticket, source, ready) = {
            let candidate = self
                .candidate
                .as_mut()
                .expect("persisted restart candidate was validated");
            let ticket = candidate
                .ticket
                .take()
                .ok_or(PersistedRestartActivationError::Actor(
                    LiveDocumentError::Invariant("persisted restart candidate ticket is missing"),
                ))?;
            let phase = std::mem::replace(
                &mut candidate.persisted_restart,
                PersistedRestartSourcePhase::Inactive,
            );
            let PersistedRestartSourcePhase::Ready {
                activation,
                parent_epoch: owned_parent_epoch,
                config: owned_config,
                source,
                receipt,
            } = phase
            else {
                unreachable!("ready phase was preflighted in the same actor turn");
            };
            debug_assert_eq!(activation, handle.activation);
            debug_assert_eq!(owned_parent_epoch, parent_epoch);
            debug_assert_eq!(owned_config, config);
            (ticket, source, receipt)
        };
        let cursor_offset = source.cursor_offset();

        let mut session = match self.arena.resume_build(ticket) {
            Ok(session) => session,
            Err(failure) => {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate remains active");
                candidate.ticket = Some(failure.ticket);
                candidate.persisted_restart = PersistedRestartSourcePhase::Ready {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    source,
                    receipt: ready,
                };
                return Err(PersistedRestartActivationError::Actor(
                    LiveDocumentError::ArenaBuild(failure.error),
                ));
            }
        };

        #[cfg(test)]
        let retained = match crossed_parent_for_test {
            Some(crossed_parent) => crossed_parent.retain_children_for_adoption(&mut session),
            None => {
                let parent = self
                    .latest_restart_document
                    .as_ref()
                    .expect("persisted restart parent was revalidated");
                parent
                    .published
                    .retain_children_for_adoption(&self.coordinator, &mut session)
            }
        };
        #[cfg(not(test))]
        let retained = {
            let parent = self
                .latest_restart_document
                .as_ref()
                .expect("persisted restart parent was revalidated");
            parent
                .published
                .retain_children_for_adoption(&self.coordinator, &mut session)
        };
        let lease = match retained {
            Ok(lease) => lease,
            Err(RestartCompositeAdoptionRetentionFailure::Pristine(error)) => {
                return match session.suspend() {
                    Ok(ticket) => {
                        let candidate = self
                            .candidate
                            .as_mut()
                            .expect("persisted restart candidate remains active");
                        candidate.ticket = Some(ticket);
                        candidate.persisted_restart = PersistedRestartSourcePhase::Ready {
                            activation: handle.activation,
                            parent_epoch,
                            config,
                            source,
                            receipt: ready,
                        };
                        Err(error.into())
                    }
                    Err(suspend_error) => {
                        // Session Drop has transitioned the build to Aborting;
                        // the source-selected activation cannot be retried.
                        drop(source);
                        let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                        Err(PersistedRestartActivationError::AdmissionAborted {
                            error: LiveDocumentError::ArenaBuild(suspend_error),
                            abort,
                        })
                    }
                };
            }
            Err(RestartCompositeAdoptionRetentionFailure::Mutated {
                error,
                cleanup_error,
            }) => {
                // Even successful compensating releases do not make a failed
                // post-retain integrity check retryable. Session Drop puts the
                // whole journal into Aborting and burns the selected source.
                drop(source);
                drop(session);
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                return Err(PersistedRestartActivationError::ParentRetentionAborted {
                    error,
                    cleanup_error,
                    abort,
                });
            }
        };

        let selected = match source.join_parent_adoption_lease(lease) {
            Ok(selected) => selected,
            Err(failure) => {
                let error = failure.error;
                let cleanup_error = failure.lease.cancel(session).err();
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                return Err(PersistedRestartActivationError::ParentSelectionAborted {
                    error,
                    cleanup_error,
                    abort,
                });
            }
        };
        let receipt = PersistedRestartParentSelectionReceipt {
            ready,
            cursor_offset,
            retained_child_owners: 2,
            parent_manifest_validations: 1,
            source_bytes_copied: 0,
            parent_pages_copied: 0,
        };
        let ticket = match session.suspend() {
            Ok(ticket) => ticket,
            Err(error) => {
                drop(selected);
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                return Err(PersistedRestartActivationError::AdmissionAborted {
                    error: LiveDocumentError::ArenaBuild(error),
                    abort,
                });
            }
        };
        let candidate = self
            .candidate
            .as_mut()
            .expect("persisted restart candidate remains active");
        candidate.ticket = Some(ticket);
        candidate.persisted_restart = PersistedRestartSourcePhase::ParentSelected {
            activation: handle.activation,
            parent_epoch,
            config,
            source: selected,
            receipt,
        };
        Ok(receipt)
    }

    /// Advances the complete parent-selected source/green handoff by at most
    /// one retained-green journal step. The scheduler supplies only the
    /// copyable activation handle; the restart job, parser, source ledger,
    /// ticket, identities, and branded parent lease never leave this actor.
    ///
    /// Once parent children have been retained, every validation or poll
    /// failure is abort-only. A successful terminal poll performs the final
    /// fallible green/source/composer join, suspends the same journal, and then
    /// installs CandidateWriter by infallible moves.
    pub(crate) fn poll_persisted_candidate_writer_restart(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<PersistedRestartWriterProgress, PersistedRestartActivationError> {
        let (parent_epoch, config) = self.require_persisted_restart_poll_slot(handle)?;
        self.validate_persisted_restart_parent(handle.epoch, parent_epoch, config)?;

        if let PersistedRestartSourcePhase::WriterInstalled { receipt, .. } = &self
            .candidate
            .as_ref()
            .expect("persisted restart candidate was validated")
            .persisted_restart
        {
            return Ok(PersistedRestartWriterProgress::Installed(*receipt));
        }

        if matches!(
            self.candidate
                .as_ref()
                .expect("persisted restart candidate was validated")
                .persisted_restart,
            PersistedRestartSourcePhase::ParentSelected { .. }
        ) {
            let (source, parent_receipt) = {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate was validated");
                let phase = std::mem::replace(
                    &mut candidate.persisted_restart,
                    PersistedRestartSourcePhase::Inactive,
                );
                let PersistedRestartSourcePhase::ParentSelected {
                    activation,
                    parent_epoch: owned_parent_epoch,
                    config: owned_config,
                    source,
                    receipt,
                } = phase
                else {
                    unreachable!("parent-selected phase was preflighted in the same actor turn");
                };
                debug_assert_eq!(activation, handle.activation);
                debug_assert_eq!(owned_parent_epoch, parent_epoch);
                debug_assert_eq!(owned_config, config);
                (source, receipt)
            };
            let restart = {
                let candidate = self
                    .candidate
                    .as_ref()
                    .expect("persisted restart candidate remains active");
                let Some(ticket) = candidate.ticket.as_ref() else {
                    drop(source);
                    let error = CandidateWriterError::Invariant(
                        "parent-selected CandidateWriter restart ticket is missing",
                    );
                    return Err(self.abort_parent_selected_writer_constructor(handle.epoch, error));
                };
                source.try_begin_candidate_writer_restart(ticket, &self.arena, config)
            };
            let restart = match restart {
                Ok(restart) => restart,
                Err(error) => {
                    return Err(self.abort_parent_selected_writer_constructor(handle.epoch, error));
                }
            };
            self.candidate
                .as_mut()
                .expect("persisted restart candidate remains active")
                .persisted_restart = PersistedRestartSourcePhase::WriterRestarting {
                activation: handle.activation,
                parent_epoch,
                config,
                restart,
                receipt: parent_receipt,
            };
        }

        let (ticket, mut restart, parent_receipt) = {
            let candidate = self
                .candidate
                .as_mut()
                .expect("persisted restart candidate was validated");
            let ticket = candidate
                .ticket
                .take()
                .ok_or(PersistedRestartActivationError::Actor(
                    LiveDocumentError::Invariant(
                        "parent-selected CandidateWriter restart ticket is missing",
                    ),
                ))?;
            let phase = std::mem::replace(
                &mut candidate.persisted_restart,
                PersistedRestartSourcePhase::Inactive,
            );
            let (activation, owned_parent_epoch, owned_config, restart, receipt) = match phase {
                PersistedRestartSourcePhase::WriterRestarting {
                    activation,
                    parent_epoch,
                    config,
                    restart,
                    receipt,
                } => (activation, parent_epoch, config, restart, receipt),
                other => {
                    candidate.ticket = Some(ticket);
                    candidate.persisted_restart = other;
                    return Err(PersistedRestartActivationError::WrongPhase);
                }
            };
            debug_assert_eq!(activation, handle.activation);
            debug_assert_eq!(owned_parent_epoch, parent_epoch);
            debug_assert_eq!(owned_config, config);
            (ticket, restart, receipt)
        };

        let mut session = match self.arena.resume_build(ticket) {
            Ok(session) => session,
            Err(failure) => {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate remains active");
                candidate.ticket = Some(failure.ticket);
                candidate.persisted_restart = PersistedRestartSourcePhase::WriterRestarting {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    restart,
                    receipt: parent_receipt,
                };
                return Err(PersistedRestartActivationError::Actor(
                    LiveDocumentError::ArenaBuild(failure.error),
                ));
            }
        };

        let progress = match restart.poll(&mut session) {
            Ok(progress) => progress,
            Err(error) => {
                drop(restart);
                drop(session);
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                return Err(PersistedRestartActivationError::WriterRestartAborted { error, abort });
            }
        };
        if progress == crate::ParentSelectedCandidateWriterRestartProgress::Pending {
            return match session.suspend() {
                Ok(ticket) => {
                    let candidate = self
                        .candidate
                        .as_mut()
                        .expect("persisted restart candidate remains active");
                    candidate.ticket = Some(ticket);
                    candidate.persisted_restart = PersistedRestartSourcePhase::WriterRestarting {
                        activation: handle.activation,
                        parent_epoch,
                        config,
                        restart,
                        receipt: parent_receipt,
                    };
                    Ok(PersistedRestartWriterProgress::Pending)
                }
                Err(error) => {
                    drop(restart);
                    let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                    Err(PersistedRestartActivationError::AdmissionAborted {
                        error: LiveDocumentError::ArenaBuild(error),
                        abort,
                    })
                }
            };
        }

        let prepared = match restart.take_output(&session) {
            Ok(prepared) => prepared,
            Err(error) => {
                drop(session);
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                return Err(PersistedRestartActivationError::WriterRestartAborted { error, abort });
            }
        };
        let ticket = match session.suspend() {
            Ok(ticket) => ticket,
            Err(error) => {
                drop(prepared);
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                return Err(PersistedRestartActivationError::AdmissionAborted {
                    error: LiveDocumentError::ArenaBuild(error),
                    abort,
                });
            }
        };

        let candidate = self
            .candidate
            .as_mut()
            .expect("persisted restart candidate remains active");
        let identities = candidate
            .identities
            .take()
            .expect("fresh parent-selected candidate owns its identity allocator");
        let (writer, driver) = prepared.install(ticket, identities);
        let receipt = PersistedRestartWriterInstallReceipt {
            parent: parent_receipt,
            cursor_offset: parent_receipt.cursor_offset,
            open_bindings: driver.binding_count(),
            acknowledged_lines: driver.acknowledged_lines(),
            green: driver.green_receipt(),
        };
        drop(candidate.raw_source.take());
        candidate.writer = CandidateWriterSlot::Active(Box::new(writer));
        candidate.projection_composer_admitted = true;
        candidate.persisted_restart = PersistedRestartSourcePhase::WriterInstalled {
            activation: handle.activation,
            parent_epoch,
            config,
            driver,
            receipt,
        };
        Ok(PersistedRestartWriterProgress::Installed(receipt))
    }

    /// Consumes the installed parent-selected driver into the actor-owned
    /// exact suffix state machine, then advances exactly one rejoin/parser/
    /// writer transition. The returned progress is copyable observation only.
    /// Reaching `CheckpointIndexSpliceRequired` proves source/composer/green
    /// completion is sealed while the retained-parent tail is still present;
    /// it does not publish or independently commit anything.
    pub(crate) fn poll_persisted_exact_restart(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<
        crate::exact_block_job::ParentSelectedExactBlockDriverProgress,
        PersistedRestartActivationError,
    > {
        let (parent_epoch, config) = self.require_persisted_restart_poll_slot(handle)?;
        self.validate_persisted_restart_parent(handle.epoch, parent_epoch, config)?;

        if matches!(
            self.candidate
                .as_ref()
                .expect("persisted restart candidate was validated")
                .persisted_restart,
            PersistedRestartSourcePhase::WriterInstalled { .. }
        ) {
            let (driver, receipt) = {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate was validated");
                let phase = std::mem::replace(
                    &mut candidate.persisted_restart,
                    PersistedRestartSourcePhase::Inactive,
                );
                let PersistedRestartSourcePhase::WriterInstalled {
                    activation,
                    parent_epoch: owned_parent_epoch,
                    config: owned_config,
                    driver,
                    receipt,
                } = phase
                else {
                    unreachable!("installed phase was preflighted in the same actor turn");
                };
                debug_assert_eq!(activation, handle.activation);
                debug_assert_eq!(owned_parent_epoch, parent_epoch);
                debug_assert_eq!(owned_config, config);
                (driver, receipt)
            };
            self.candidate
                .as_mut()
                .expect("persisted restart candidate remains active")
                .persisted_restart = PersistedRestartSourcePhase::ExactDriving {
                activation: handle.activation,
                parent_epoch,
                config,
                driver: crate::exact_block_job::ParentSelectedExactBlockDriver::begin(driver),
                receipt,
            };
        }

        let (mut driver, receipt) = {
            let candidate = self
                .candidate
                .as_mut()
                .expect("persisted restart candidate was validated");
            let phase = std::mem::replace(
                &mut candidate.persisted_restart,
                PersistedRestartSourcePhase::Inactive,
            );
            match phase {
                PersistedRestartSourcePhase::ExactDriving {
                    activation,
                    parent_epoch: owned_parent_epoch,
                    config: owned_config,
                    driver,
                    receipt,
                } => {
                    debug_assert_eq!(activation, handle.activation);
                    debug_assert_eq!(owned_parent_epoch, parent_epoch);
                    debug_assert_eq!(owned_config, config);
                    (driver, receipt)
                }
                other => {
                    candidate.persisted_restart = other;
                    return Err(PersistedRestartActivationError::WrongPhase);
                }
            }
        };

        match driver.poll(self) {
            Ok(progress) => {
                self.candidate
                    .as_mut()
                    .expect("exact suffix driver cannot independently consume its candidate")
                    .persisted_restart = PersistedRestartSourcePhase::ExactDriving {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    driver,
                    receipt,
                };
                Ok(progress)
            }
            Err(failure) => {
                drop(driver);
                let failure = Box::new(failure);
                match self.cancel_candidate(handle.epoch) {
                    Ok(abort) => {
                        Err(PersistedRestartActivationError::ExactDriverAborted { failure, abort })
                    }
                    Err(cleanup_error) => Err(
                        PersistedRestartActivationError::ExactDriverCancellationFailed {
                            failure,
                            cleanup_error,
                        },
                    ),
                }
            }
        }
    }

    /// Consumes the parked matched-C exact driver and adopted writer into one
    /// candidate-owned green/checkpoint/parent splice, then advances exactly
    /// one storage transition. The same generic candidate ticket remains the
    /// latest-wins cancellation authority between every poll.
    pub(crate) fn poll_persisted_adoption_splice(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<crate::ParentSelectedAdoptionSpliceProgress, PersistedRestartActivationError> {
        let (parent_epoch, config) = self.require_persisted_restart_poll_slot(handle)?;
        self.validate_persisted_restart_parent(handle.epoch, parent_epoch, config)?;

        if matches!(
            self.candidate
                .as_ref()
                .expect("persisted restart candidate was validated")
                .persisted_restart,
            PersistedRestartSourcePhase::ExactDriving { .. }
        ) {
            let (driver, install_receipt) = {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate was validated");
                let phase = std::mem::replace(
                    &mut candidate.persisted_restart,
                    PersistedRestartSourcePhase::Inactive,
                );
                let PersistedRestartSourcePhase::ExactDriving {
                    activation,
                    parent_epoch: owned_parent_epoch,
                    config: owned_config,
                    driver,
                    receipt,
                } = phase
                else {
                    unreachable!("exact-driving phase was preflighted in the same actor turn");
                };
                debug_assert_eq!(activation, handle.activation);
                debug_assert_eq!(owned_parent_epoch, parent_epoch);
                debug_assert_eq!(owned_config, config);
                (driver, receipt)
            };
            let convergence = match driver.into_convergence_adoption() {
                Ok(convergence) => convergence,
                Err(driver) => {
                    self.candidate
                        .as_mut()
                        .expect("persisted restart candidate remains active")
                        .persisted_restart = PersistedRestartSourcePhase::ExactDriving {
                        activation: handle.activation,
                        parent_epoch,
                        config,
                        driver,
                        receipt: install_receipt,
                    };
                    return Err(PersistedRestartActivationError::WrongPhase);
                }
            };

            let adopted = {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate remains active");
                match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
                    CandidateWriterSlot::AdoptedTail(writer) => writer,
                    other => {
                        candidate.writer = other;
                        candidate.commit_recovery = true;
                        drop(convergence);
                        let error = CandidateWriterError::Invariant(
                            "matched convergence lost its adopted-tail writer",
                        );
                        return Err(self.abort_persisted_adoption_splice(handle.epoch, error));
                    }
                }
            };
            let bundle = match (*adopted).try_into_parent_selected_splice_bundle() {
                Ok(bundle) => bundle,
                Err(failure) => {
                    let failure = *failure;
                    let candidate = self
                        .candidate
                        .as_mut()
                        .expect("persisted restart candidate remains active");
                    candidate.writer = CandidateWriterSlot::AdoptedTail(Box::new(failure.writer));
                    candidate.commit_recovery = true;
                    drop(convergence);
                    return Err(self.abort_persisted_adoption_splice(handle.epoch, failure.error));
                }
            };
            match crate::ParentSelectedAdoptionSpliceJob::try_begin_suspended(
                &self.arena,
                bundle,
                convergence,
            ) {
                Ok(start) => {
                    let (job, ticket, identities) = start.into_parts();
                    let candidate = self
                        .candidate
                        .as_mut()
                        .expect("persisted restart candidate remains active");
                    debug_assert!(candidate.ticket.is_none());
                    debug_assert!(candidate.identities.is_none());
                    candidate.ticket = Some(ticket);
                    candidate.identities = Some(identities);
                    candidate.persisted_restart = PersistedRestartSourcePhase::AdoptionSplicing {
                        activation: handle.activation,
                        parent_epoch,
                        config,
                        job,
                        receipt: install_receipt,
                    };
                }
                Err(failure) => {
                    let candidate = self
                        .candidate
                        .as_mut()
                        .expect("persisted restart candidate remains active");
                    debug_assert!(candidate.ticket.is_none());
                    debug_assert!(candidate.identities.is_none());
                    candidate.ticket = Some(failure.ticket);
                    candidate.identities = Some(failure.identities);
                    candidate.commit_recovery = true;
                    return Err(self.abort_persisted_adoption_splice(handle.epoch, failure.error));
                }
            }
        }

        let (mut job, install_receipt, ticket) = {
            let candidate = self
                .candidate
                .as_mut()
                .expect("persisted restart candidate was validated");
            let phase = std::mem::replace(
                &mut candidate.persisted_restart,
                PersistedRestartSourcePhase::Inactive,
            );
            let PersistedRestartSourcePhase::AdoptionSplicing {
                activation,
                parent_epoch: owned_parent_epoch,
                config: owned_config,
                job,
                receipt,
            } = phase
            else {
                candidate.persisted_restart = phase;
                return Err(PersistedRestartActivationError::WrongPhase);
            };
            debug_assert_eq!(activation, handle.activation);
            debug_assert_eq!(owned_parent_epoch, parent_epoch);
            debug_assert_eq!(owned_config, config);
            let ticket = candidate
                .ticket
                .take()
                .ok_or(PersistedRestartActivationError::Actor(
                    LiveDocumentError::Invariant(
                        "adoption splice candidate lost its suspended ticket",
                    ),
                ))?;
            (job, receipt, ticket)
        };

        let mut session = match self.arena.resume_build(ticket) {
            Ok(session) => session,
            Err(failure) => {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate remains active");
                candidate.ticket = Some(failure.ticket);
                candidate.persisted_restart = PersistedRestartSourcePhase::AdoptionSplicing {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    job,
                    receipt: install_receipt,
                };
                let error = CandidateWriterError::ArenaBuild(failure.error);
                return Err(
                    PersistedRestartActivationError::AdoptionSpliceCancellationFailed {
                        error,
                        cleanup_error: LiveDocumentError::ArenaBuild(failure.error),
                    },
                );
            }
        };
        let polled = job.poll(&mut session);
        let suspended = session.suspend();
        match suspended {
            Ok(ticket) => {
                let candidate = self
                    .candidate
                    .as_mut()
                    .expect("persisted restart candidate remains active");
                candidate.ticket = Some(ticket);
                candidate.persisted_restart = PersistedRestartSourcePhase::AdoptionSplicing {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    job,
                    receipt: install_receipt,
                };
                match polled {
                    Ok(progress) => Ok(progress),
                    Err(error) => Err(self.abort_persisted_adoption_splice(handle.epoch, error)),
                }
            }
            Err(error) => {
                // Failed suspension drops the resumed session into the arena's
                // aborting lifecycle. Reinstall the opaque job only long
                // enough for the actor detach helper to drop every capability.
                self.candidate
                    .as_mut()
                    .expect("persisted restart candidate remains active")
                    .persisted_restart = PersistedRestartSourcePhase::AdoptionSplicing {
                    activation: handle.activation,
                    parent_epoch,
                    config,
                    job,
                    receipt: install_receipt,
                };
                let abort = self.detach_persisted_restart_aborting_candidate(handle.epoch);
                Err(PersistedRestartActivationError::AdoptionSpliceAborted {
                    error: CandidateWriterError::ArenaBuild(error),
                    abort,
                })
            }
        }
    }

    /// Copies the currently published composite green child into a host-owned
    /// full snapshot without exposing its child owner.
    #[cfg(feature = "host-mirror-probe")]
    pub(crate) fn prepare_current_restart_host_snapshot_bundle(
        &self,
        publication_session: crate::host_mirror::PublicationSessionId,
        target: crate::host_mirror::HostRevisionId,
        source: crate::host_mirror::SourceVersion,
    ) -> Result<crate::host_mirror::StructuralBundle, crate::host_mirror::HostMirrorError> {
        let parent = self.latest_restart_document.as_ref().ok_or(
            crate::host_mirror::HostMirrorError::Invalid(
                "host snapshot requires a published restart parent",
            ),
        )?;
        let view = parent
            .published
            .view(&self.coordinator, &self.arena)
            .map_err(|_| {
                crate::host_mirror::HostMirrorError::Invalid(
                    "published restart parent failed host snapshot validation",
                )
            })?;
        crate::host_mirror::prepare_full_snapshot_bundle_from_composite(
            &self.arena,
            view.green().descriptor_for_host_snapshot(),
            publication_session,
            target,
            source,
        )
    }

    /// Prepares the exact actor's finalized Direct delta without extracting or
    /// cloning its linear proof. Host application/ACK is external, so worker
    /// composite commit can follow only after this copied bundle exists.
    #[cfg(feature = "host-mirror-probe")]
    pub(crate) fn prepare_persisted_adoption_host_delta_bundle(
        &self,
        handle: PersistedRestartActivationHandle,
        base: crate::host_mirror::StructuralAck,
        target: crate::host_mirror::HostRevisionId,
        publication_session: crate::host_mirror::PublicationSessionId,
        source: crate::host_mirror::SourceVersion,
    ) -> Result<crate::host_mirror::StructuralBundle, crate::host_mirror::HostMirrorError> {
        self.require_live_epoch(handle.epoch).map_err(|_| {
            crate::host_mirror::HostMirrorError::Invalid(
                "host delta activation is not the live candidate",
            )
        })?;
        let candidate = self
            .candidate
            .as_ref()
            .expect("host delta live candidate was validated");
        let PersistedRestartSourcePhase::AdoptionSplicing {
            activation, job, ..
        } = &candidate.persisted_restart
        else {
            return Err(crate::host_mirror::HostMirrorError::Invalid(
                "host delta requires the adoption splice phase",
            ));
        };
        if *activation != handle.activation {
            return Err(crate::host_mirror::HostMirrorError::Invalid(
                "host delta activation handle changed",
            ));
        }
        job.prepare_completed_host_delta_bundle(
            &self.arena,
            base,
            target,
            publication_session,
            source,
        )
    }

    /// Commits and atomically publishes the completed green/checkpoint parent.
    /// No scalar child descriptor or raw arena owner crosses this boundary:
    /// the exact actor-owned splice job yields the sole parent manifest, and
    /// the existing restart publication transaction consumes it directly.
    pub(crate) fn commit_persisted_adoption_splice(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<RestartCompositeCommitProgress, PersistedRestartActivationError> {
        let (parent_epoch, config) = self.require_persisted_restart_poll_slot(handle)?;
        self.validate_persisted_restart_parent(handle.epoch, parent_epoch, config)?;
        let complete = matches!(
            &self
                .candidate
                .as_ref()
                .expect("persisted restart candidate was validated")
                .persisted_restart,
            PersistedRestartSourcePhase::AdoptionSplicing {
                activation,
                job,
                ..
            } if *activation == handle.activation && job.is_complete()
        );
        if !complete {
            return Err(PersistedRestartActivationError::WrongPhase);
        }

        let mut candidate = self
            .candidate
            .take()
            .expect("persisted restart candidate was validated");
        let phase = std::mem::replace(
            &mut candidate.persisted_restart,
            PersistedRestartSourcePhase::Inactive,
        );
        let PersistedRestartSourcePhase::AdoptionSplicing {
            activation,
            parent_epoch: owned_parent_epoch,
            config: owned_config,
            mut job,
            receipt,
        } = phase
        else {
            unreachable!("completed adoption phase was checked in the same actor turn");
        };
        debug_assert_eq!(activation, handle.activation);
        debug_assert_eq!(owned_parent_epoch, parent_epoch);
        debug_assert_eq!(owned_config, config);
        debug_assert!(matches!(candidate.writer, CandidateWriterSlot::None));
        let ticket = candidate
            .ticket
            .take()
            .expect("completed adoption lost ticket");
        let identities = candidate
            .identities
            .take()
            .expect("completed adoption lost identity allocator");

        let session = match self.arena.resume_build(ticket) {
            Ok(session) => session,
            Err(failure) => {
                let error = CandidateWriterError::ArenaBuild(failure.error);
                candidate.ticket = Some(failure.ticket);
                candidate.identities = Some(identities);
                candidate.persisted_restart = PersistedRestartSourcePhase::AdoptionSplicing {
                    activation,
                    parent_epoch,
                    config,
                    job,
                    receipt,
                };
                candidate.commit_recovery = true;
                self.candidate = Some(candidate);
                return Err(
                    PersistedRestartActivationError::AdoptionCommitCancellationFailed {
                        error,
                        cleanup_error: LiveDocumentError::ArenaBuild(failure.error),
                    },
                );
            }
        };
        let parent = match job.take_parent_manifest() {
            Ok(parent) => parent,
            Err(error) => {
                drop(session);
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                self.aborting.push(AbortingCandidate::empty(handle.epoch));
                return Err(PersistedRestartActivationError::AdoptionCommitAborted {
                    error,
                    abort: CandidateAbort {
                        epoch: handle.epoch,
                    },
                });
            }
        };
        let (document, build_receipt) = match parent.commit(session) {
            Ok(committed) => committed,
            Err(error) => {
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                self.aborting.push(AbortingCandidate::empty(handle.epoch));
                return Err(PersistedRestartActivationError::AdoptionCommitAborted {
                    error: error.into(),
                    abort: CandidateAbort {
                        epoch: handle.epoch,
                    },
                });
            }
        };
        debug_assert!(self.identities.is_none());
        self.identities = Some(identities);
        drop(candidate);
        Ok(self.prepare_and_publish_restart_composite(handle.epoch, build_receipt, document))
    }

    fn abort_persisted_adoption_splice(
        &mut self,
        epoch: LiveCandidateEpoch,
        error: CandidateWriterError,
    ) -> PersistedRestartActivationError {
        match self.cancel_candidate(epoch) {
            Ok(abort) => PersistedRestartActivationError::AdoptionSpliceAborted { error, abort },
            Err(cleanup_error) => {
                PersistedRestartActivationError::AdoptionSpliceCancellationFailed {
                    error,
                    cleanup_error,
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn persisted_adoption_splice_receipt_for_test(
        &self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<crate::ParentSelectedAdoptionSpliceReceipt, PersistedRestartActivationError> {
        self.require_live_epoch(handle.epoch)?;
        let candidate = self
            .candidate
            .as_ref()
            .expect("persisted restart candidate was validated");
        let PersistedRestartSourcePhase::AdoptionSplicing {
            activation, job, ..
        } = &candidate.persisted_restart
        else {
            return Err(PersistedRestartActivationError::WrongPhase);
        };
        if *activation != handle.activation {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        Ok(job.receipt())
    }

    #[cfg(all(test, feature = "host-mirror-probe"))]
    pub(crate) fn persisted_adoption_host_splice_range_counts_for_test(
        &self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<Option<(u64, u64, u64, u64)>, PersistedRestartActivationError> {
        self.require_live_epoch(handle.epoch)?;
        let candidate = self
            .candidate
            .as_ref()
            .expect("persisted restart candidate was validated");
        let PersistedRestartSourcePhase::AdoptionSplicing {
            activation, job, ..
        } = &candidate.persisted_restart
        else {
            return Err(PersistedRestartActivationError::WrongPhase);
        };
        if *activation != handle.activation {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        Ok(job.host_splice_range_counts_for_test())
    }

    #[cfg(test)]
    pub(crate) fn persisted_old_convergence_for_test(
        &self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<
        (
            Option<crate::committed_checkpoint_index::RelativeCheckpointMeasure>,
            Option<u64>,
            Option<&'static str>,
        ),
        PersistedRestartActivationError,
    > {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(PersistedRestartActivationError::Actor(
                LiveDocumentError::NoCandidate,
            ))?;
        if candidate.epoch != handle.epoch {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        let PersistedRestartSourcePhase::ExactDriving {
            activation, driver, ..
        } = &candidate.persisted_restart
        else {
            return Err(PersistedRestartActivationError::WrongPhase);
        };
        if *activation != handle.activation {
            return Err(PersistedRestartActivationError::WrongPhase);
        }
        Ok(driver.old_convergence_for_test())
    }

    #[cfg(test)]
    pub(crate) fn persisted_mapped_convergence_for_test(
        &self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<
        Option<(
            crate::committed_checkpoint_index::RelativeCheckpointMeasure,
            crate::parent_selected_convergence::ParentSelectedConvergenceMapReceipt,
        )>,
        PersistedRestartActivationError,
    > {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(PersistedRestartActivationError::Actor(
                LiveDocumentError::NoCandidate,
            ))?;
        if candidate.epoch != handle.epoch {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        let PersistedRestartSourcePhase::ExactDriving {
            activation, driver, ..
        } = &candidate.persisted_restart
        else {
            return Err(PersistedRestartActivationError::WrongPhase);
        };
        if *activation != handle.activation {
            return Err(PersistedRestartActivationError::WrongPhase);
        }
        Ok(driver.mapped_convergence_for_test())
    }

    #[cfg(test)]
    pub(crate) fn persisted_matched_live_sample_certificate_for_test(
        &self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<
        Option<(
            crate::LiveCandidateEpoch,
            crate::committed_checkpoint_index::RelativeCheckpointMeasure,
            crate::committed_checkpoint_index::RelativeCheckpointMeasure,
            u64,
        )>,
        PersistedRestartActivationError,
    > {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(PersistedRestartActivationError::Actor(
                LiveDocumentError::NoCandidate,
            ))?;
        if candidate.epoch != handle.epoch {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        let PersistedRestartSourcePhase::ExactDriving {
            activation, driver, ..
        } = &candidate.persisted_restart
        else {
            return Err(PersistedRestartActivationError::WrongPhase);
        };
        if *activation != handle.activation {
            return Err(PersistedRestartActivationError::WrongPhase);
        }
        Ok(driver.matched_live_sample_certificate_for_test())
    }

    fn abort_parent_selected_writer_constructor(
        &mut self,
        epoch: LiveCandidateEpoch,
        error: CandidateWriterError,
    ) -> PersistedRestartActivationError {
        match self.cancel_candidate(epoch) {
            Ok(abort) => PersistedRestartActivationError::WriterRestartAborted { error, abort },
            Err(cleanup_error) => {
                PersistedRestartActivationError::WriterRestartCancellationFailed {
                    error,
                    cleanup_error,
                }
            }
        }
    }

    /// Drops resolving or ready state without consulting the parent. Cleanup
    /// depends only on the exact live candidate and activation, so later parent
    /// release cannot strand the actor slot.
    pub(crate) fn abandon_persisted_restart_activation(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<(), PersistedRestartActivationError> {
        self.clear_persisted_restart_phase(handle)
    }

    fn require_pristine_persisted_restart_slot(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), PersistedRestartActivationError> {
        self.require_live_epoch(epoch)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        if candidate.ledger.is_some()
            || candidate.projection_composer_admitted
            || !matches!(candidate.writer, CandidateWriterSlot::None)
            || candidate.retained_activation.is_some()
            || !matches!(
                candidate.persisted_restart,
                PersistedRestartSourcePhase::Inactive
            )
            || candidate.commit_recovery
            || candidate.ticket.is_none()
            || candidate.identities.is_none()
            || candidate.raw_source.as_ref().is_none_or(|source| {
                source.descriptor != epoch.source() || source.cursor.offset() != 0
            })
        {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::Invariant(
                    "persisted restart requires one untouched fresh candidate",
                ),
            ));
        }
        Ok(())
    }

    fn require_persisted_restart_poll_slot(
        &self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<(LiveCandidateEpoch, CandidateWriterConfig), PersistedRestartActivationError> {
        self.require_live_epoch(handle.epoch)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        if candidate.retained_activation != Some(handle.activation)
            || candidate.persisted_restart.activation() != Some(handle.activation)
        {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        candidate
            .persisted_restart
            .parent_and_config()
            .ok_or(PersistedRestartActivationError::WrongPhase)
    }

    fn validate_persisted_restart_parent(
        &self,
        epoch: LiveCandidateEpoch,
        parent_epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
    ) -> Result<(), PersistedRestartActivationError> {
        let parent = self
            .latest_restart_document
            .as_ref()
            .ok_or(PersistedRestartActivationError::NoCommittedParent)?;
        let view = parent.published.view(&self.coordinator, &self.arena)?;
        let green = view.green();
        let active =
            self.coordinator
                .active_plan()
                .ok_or(PersistedRestartActivationError::Actor(
                    LiveDocumentError::CandidateStale,
                ))?;
        let parent_lease = parent.published.output_lease();
        if config.syntax_profile == 0
            || config.grammar_revision.0 == 0
            || config.semantic_epoch == 0
            || parent.epoch != parent_epoch
            || active.token != epoch.parse_token()
            || active.base_output != parent_lease
            || self.coordinator.current_output() != parent_lease
            || parent_epoch.arena_identity() != self.arena.identity()
            || epoch.arena_identity() != self.arena.identity()
            || parent_epoch.build_id() == epoch.build_id()
            || green.source_root() != parent_epoch.source().root
            || green.source_revision() != parent_epoch.source().revision
            || usize::try_from(green.source_metric().bytes).ok()
                != Some(parent_epoch.source().bytes)
            || green.parse_generation() != parent_epoch.parse_token().generation
            || green.syntax_profile() != config.syntax_profile
            || green.grammar_revision() != config.grammar_revision
            || green.parse_generation().0 >= epoch.parse_token().generation.0
            || green.semantic_epoch() >= config.semantic_epoch
            || self.source.descriptor() != epoch.source()
        {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::CandidateStale,
            ));
        }
        Ok(())
    }

    fn clear_persisted_restart_phase(
        &mut self,
        handle: PersistedRestartActivationHandle,
    ) -> Result<(), PersistedRestartActivationError> {
        self.require_live_epoch(handle.epoch)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        if candidate.retained_activation != Some(handle.activation)
            || candidate.persisted_restart.activation() != Some(handle.activation)
        {
            return Err(PersistedRestartActivationError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        if matches!(
            candidate.persisted_restart,
            PersistedRestartSourcePhase::ParentSelected { .. }
                | PersistedRestartSourcePhase::WriterRestarting { .. }
                | PersistedRestartSourcePhase::WriterInstalled { .. }
                | PersistedRestartSourcePhase::ExactDriving { .. }
                | PersistedRestartSourcePhase::AdoptionSplicing { .. }
        ) {
            // The journal owns retained parent children after this boundary.
            // Only whole-candidate cancellation or downstream writer install
            // may consume it; byte-zero fallback is no longer clean.
            return Err(PersistedRestartActivationError::WrongPhase);
        }
        drop(std::mem::replace(
            &mut candidate.persisted_restart,
            PersistedRestartSourcePhase::Inactive,
        ));
        candidate.retained_activation = None;
        Ok(())
    }

    /// Completes the actor bookkeeping after a resumed journal has already
    /// entered Aborting. `begin_candidate` reserved this queue slot before it
    /// issued any authority, so the push performs no hidden allocation.
    fn detach_persisted_restart_aborting_candidate(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> CandidateAbort {
        let CandidateJob {
            epoch: owned_epoch,
            raw_source,
            ledger,
            projection_composer_admitted: _,
            ticket,
            identities,
            writer,
            retained_activation: _,
            persisted_restart,
            commit_recovery,
        } = self
            .candidate
            .take()
            .expect("persisted restart candidate remains actor-owned");
        debug_assert_eq!(owned_epoch, epoch);
        debug_assert!(ticket.is_none());
        debug_assert!(ledger.is_none());
        debug_assert!(matches!(writer, CandidateWriterSlot::None));
        debug_assert!(!commit_recovery);
        drop(raw_source);
        let heap = persisted_restart.into_heap_retirement();
        debug_assert!(self.identities.is_none());
        self.identities = identities;
        self.aborting
            .push(AbortingCandidate::with_heap(epoch, heap));
        CandidateAbort { epoch }
    }
}

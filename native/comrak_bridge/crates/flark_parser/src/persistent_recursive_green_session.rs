//! Session-owned recursive Green and reference authority.
//!
//! This is the production migration seam for the candidate endpoint.  A clean
//! build is explicit caller-fuelled work performed once when a document is
//! opened.  The resulting roots remain owned by the session, so hot-inline
//! queries never rebuild or discard document structure.

use std::{fmt, ops::Range};

use flark_engine::parser_internal::{
    M11RecursiveGreenError, M11RecursiveGreenFrameQueryError, M11RecursiveGreenFrameQueryLimits,
    M11RecursiveGreenLocation, M11RecursiveGreenPoint, M11RecursiveGreenRoot,
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenRowWindow,
    M11RecursiveGreenStoragePageIdentity, M11RecursiveGreenStructuralSpliceSelection,
    M11ReferenceJournal, M11ReferenceJournalAdoptionStatus, M11ReferenceJournalError,
    M11ReferenceJournalRangeReplacement, M11ReferenceJournalRangeReplacementStatus,
    M11ReferenceJournalRoot, M11ReferenceJournalStatus,
    M11ReferenceJournalUnchangedPrefixAdoption, BLOCK_QUOTE_WINDOW_MAX_BYTES,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeError, ExactUnchangedPrefixWitness,
    ExactUnchangedSuffixWitness, SourceSnapshotLease, SourceVersion,
};

use crate::block_core::{
    resolve_m11_recursive_green_inline_leaf_row_fence, resolve_m11_recursive_green_paragraph_fence,
    BlockCommand, BlockKind, M11BlockRestartCheckpoint, M11BlockRestartError,
    M11BlockStructuralAdoptionReceipt, M11BlockTerminalConvergenceCheckpoint, M11BlockWriter,
    M11BlockWriterError, M11BlockWriterOfferStatus, M11BlockWriterPollStatus,
    M11DirectBlockController, M11DirectBlockControllerError, M11DirectBlockError,
    M11DirectBlockPollStatus, M11DirectSourceLineAdmission, M11ReferenceRendezvous,
    M11ReferenceRendezvousError, M11ReferenceRendezvousStatus, SourceMetric,
};
use crate::recursive_green_block_quote_projection::{
    resolve_m11_recursive_green_block_quote_projection_fence,
    M11RecursiveGreenBlockQuoteProjectionPreparation,
};
use crate::recursive_green_paragraph_inline::{
    M11RecursiveGreenInlineLeafPreparation, M11RecursiveGreenParagraphInlinePreparation,
};
use crate::source_adapter::SnapshotLineRetainedPoll;
use crate::{
    M11ExactController, M11PhysicalLineFacts, M11SourceLinePollStatus, M11SourceLineSource,
    SnapshotLineScanner, SnapshotLineSource, SnapshotPhysicalLine, SourceAdapterError,
};

const SOURCE_WORK_QUANTUM: usize = flark_engine::SOURCE_CURSOR_WINDOW_BYTES;
const CHECKPOINT_STRIDE_BYTES: u64 = 4 * 1024;
const LATER_CONVERGENCE_MAX_BYTES: usize = 64 * 1024;
const LATER_CONVERGENCE_MAX_PHYSICAL_LINES: u64 = 512;
const LATER_CONVERGENCE_MAX_TRANSITIONS: usize = 4_096;

#[derive(Debug)]
pub enum M11PersistentRecursiveGreenSessionError {
    ZeroFuel,
    InvalidState(&'static str),
    Document(DocumentRuntimeError),
    Source(SourceAdapterError),
    Controller(M11DirectBlockError),
    SourceController(M11DirectBlockControllerError<SourceAdapterError>),
    Writer(M11BlockWriterError),
    Reference(M11ReferenceRendezvousError),
    Journal(M11ReferenceJournalError),
    Green(M11RecursiveGreenError),
    Query(M11RecursiveGreenFrameQueryError),
    Restart(M11BlockRestartError),
}

impl fmt::Display for M11PersistentRecursiveGreenSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFuel => formatter.write_str("persistent recursive-Green work requires fuel"),
            Self::InvalidState(message) => formatter.write_str(message),
            Self::Document(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
            Self::Controller(error) => write!(formatter, "direct block controller: {error:?}"),
            Self::SourceController(error) => {
                write!(formatter, "direct block source controller: {error:?}")
            }
            Self::Writer(error) => error.fmt(formatter),
            Self::Reference(error) => error.fmt(formatter),
            Self::Journal(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
            Self::Query(error) => error.fmt(formatter),
            Self::Restart(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11PersistentRecursiveGreenSessionError {}

impl From<DocumentRuntimeError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self::Document(error)
    }
}

impl From<SourceAdapterError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: SourceAdapterError) -> Self {
        Self::Source(error)
    }
}

impl From<M11DirectBlockError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11DirectBlockError) -> Self {
        Self::Controller(error)
    }
}

impl From<M11DirectBlockControllerError<SourceAdapterError>>
    for M11PersistentRecursiveGreenSessionError
{
    fn from(error: M11DirectBlockControllerError<SourceAdapterError>) -> Self {
        Self::SourceController(error)
    }
}

impl From<M11BlockWriterError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11BlockWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<M11ReferenceRendezvousError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11ReferenceRendezvousError) -> Self {
        Self::Reference(error)
    }
}

impl From<M11ReferenceJournalError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11ReferenceJournalError) -> Self {
        Self::Journal(error)
    }
}

impl From<M11RecursiveGreenError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self::Green(error)
    }
}

impl From<M11RecursiveGreenFrameQueryError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11RecursiveGreenFrameQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<M11BlockRestartError> for M11PersistentRecursiveGreenSessionError {
    fn from(error: M11BlockRestartError) -> Self {
        Self::Restart(error)
    }
}

/// Move-only leases which can begin one clean composite build once the
/// endpoint next receives mutable runtime authority.
pub struct M11PersistentRecursiveGreenCleanPlan {
    scanner_lease: SourceSnapshotLease,
    writer_lease: SourceSnapshotLease,
    syntax_profile: u32,
}

impl M11PersistentRecursiveGreenCleanPlan {
    pub fn new(
        scanner_lease: SourceSnapshotLease,
        writer_lease: SourceSnapshotLease,
        syntax_profile: u32,
    ) -> Result<Self, M11PersistentRecursiveGreenSessionError> {
        if scanner_lease.version() != writer_lease.version() || syntax_profile == 0 {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green clean plan crossed source or syntax authority",
            ));
        }
        Ok(Self {
            scanner_lease,
            writer_lease,
            syntax_profile,
        })
    }

    pub fn begin(
        self,
        runtime: &mut DocumentRuntime,
    ) -> Result<M11PersistentRecursiveGreenCleanBuild, M11PersistentRecursiveGreenSessionError>
    {
        let source = self.scanner_lease.version();
        if runtime.current_source_version() != Some(source) {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green clean plan is not current",
            ));
        }
        let scanner = SnapshotLineScanner::new(self.scanner_lease)?;
        let controller = M11DirectBlockController::new()?;
        let writer = M11BlockWriter::new(runtime, self.writer_lease)?;
        let journal = M11ReferenceJournal::new(runtime, source, self.syntax_profile)?;
        Ok(M11PersistentRecursiveGreenCleanBuild {
            source,
            syntax_profile: self.syntax_profile,
            phase: CleanPhase::ControllerLine,
            scanner: Some(scanner),
            pending_line: None,
            active_line: None,
            controller: Some(controller),
            writer: Some(writer),
            writer_command_pending: false,
            rendezvous: None,
            journal: Some(journal),
            checkpoints: Vec::new(),
            terminal_convergence: None,
            initial_boundary_captured: false,
            green: None,
            references: None,
            output: None,
            cancelling: false,
            writer_cancel_complete: false,
            journal_cancel_complete: false,
            green_release_complete: false,
            references_release_complete: false,
        })
    }
}

struct ActiveLine {
    facts: M11PhysicalLineFacts,
    source: SnapshotLineSource,
    admission: M11DirectSourceLineAdmission,
    matched: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanPhase {
    ControllerLine,
    Scanning,
    BeginFinish,
    ControllerFinish,
    FinishReferences,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11PersistentRecursiveGreenBuildStatus {
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PersistentRecursiveGreenBuildPoll {
    status: M11PersistentRecursiveGreenBuildStatus,
    transitions: usize,
}

impl M11PersistentRecursiveGreenBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11PersistentRecursiveGreenBuildStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

/// Caller-fuelled clean build of one composite recursive-Green/reference
/// session. Each transition performs at most one source quantum or one
/// controller/writer/reference state transition.
#[must_use = "clean recursive-Green builds require session transfer or cancellation"]
pub struct M11PersistentRecursiveGreenCleanBuild {
    source: SourceVersion,
    syntax_profile: u32,
    phase: CleanPhase,
    scanner: Option<SnapshotLineScanner>,
    pending_line: Option<SnapshotPhysicalLine>,
    active_line: Option<ActiveLine>,
    controller: Option<M11DirectBlockController>,
    writer: Option<M11BlockWriter>,
    writer_command_pending: bool,
    rendezvous: Option<M11ReferenceRendezvous>,
    journal: Option<M11ReferenceJournal>,
    checkpoints: Vec<M11BlockRestartCheckpoint>,
    terminal_convergence: Option<M11BlockTerminalConvergenceCheckpoint>,
    initial_boundary_captured: bool,
    green: Option<M11RecursiveGreenRoot>,
    references: Option<M11ReferenceJournalRoot>,
    output: Option<M11PersistentRecursiveGreenSession>,
    cancelling: bool,
    writer_cancel_complete: bool,
    journal_cancel_complete: bool,
    green_release_complete: bool,
    references_release_complete: bool,
}

impl M11PersistentRecursiveGreenCleanBuild {
    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11PersistentRecursiveGreenBuildPoll, M11PersistentRecursiveGreenSessionError> {
        if fuel == 0 {
            return Err(M11PersistentRecursiveGreenSessionError::ZeroFuel);
        }
        if self.cancelling {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "cancelled recursive-Green build cannot resume",
            ));
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green clean build crossed source authority",
            ));
        }
        let mut transitions = 0;
        while transitions < fuel && self.phase != CleanPhase::Complete {
            self.poll_one(runtime)?;
            transitions += 1;
        }
        Ok(M11PersistentRecursiveGreenBuildPoll {
            status: if self.phase == CleanPhase::Complete {
                M11PersistentRecursiveGreenBuildStatus::Complete
            } else {
                M11PersistentRecursiveGreenBuildStatus::Pending
            },
            transitions,
        })
    }

    fn poll_one(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        if let Some(mut rendezvous) = self.rendezvous.take() {
            let controller = self.controller.as_mut().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green controller is missing",
                ),
            )?;
            let writer = self.writer.as_mut().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green writer is missing",
                ),
            )?;
            let journal = self.journal.as_mut().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green reference journal is missing",
                ),
            )?;
            let poll = rendezvous.poll(controller, writer, journal, runtime, 1)?;
            if poll.status != M11ReferenceRendezvousStatus::Complete {
                self.rendezvous = Some(rendezvous);
            } else if let Some(remainder) = rendezvous.take_leading_reference_remainder() {
                let (parser, green) = remainder.into_parts();
                let checkpoint =
                    writer.capture_leading_reference_remainder_checkpoint(parser, green)?;
                let insertion = self.checkpoints.partition_point(|existing| {
                    existing.parser_physical().bytes() < checkpoint.parser_physical().bytes()
                });
                if self.checkpoints.get(insertion).is_some_and(|existing| {
                    existing.parser_physical() == checkpoint.parser_physical()
                }) {
                    return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "leading-reference remainder duplicated a restart cut",
                    ));
                }
                self.checkpoints.try_reserve(1).map_err(|_| {
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "leading-reference restart allocation failed",
                    )
                })?;
                self.checkpoints.insert(insertion, checkpoint);
            }
            return Ok(());
        }
        if self.writer_command_pending {
            let poll = self.writer_mut()?.poll(runtime, 1)?;
            if matches!(
                poll.status(),
                M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete
            ) {
                self.writer_command_pending = false;
                self.controller_mut()?.acknowledge_command()?;
            }
            return Ok(());
        }

        match self.phase {
            CleanPhase::ControllerLine => {
                if !self.initial_boundary_captured
                    && self.active_line.is_none()
                    && self.pending_line.is_none()
                {
                    if self.controller_mut()?.pending_command().is_some() {
                        self.offer_pending_command()?;
                    } else {
                        if !self.initial_boundary_captured {
                            // Preserve an authenticated Document-only restart at
                            // BOF. Without this cut, an edit in the first sparse
                            // checkpoint interval has no predecessor and must
                            // fall back to a whole-document clean build.
                            self.capture_document_start_checkpoint()?;
                            self.initial_boundary_captured = true;
                        }
                        self.phase = CleanPhase::Scanning;
                    }
                    return Ok(());
                }
                if let Some(mut active) = self.active_line.take() {
                    if active.matched {
                        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
                            self.controller_mut()?,
                            active.admission,
                            active.facts,
                        )?;
                        self.scanner = Some(active.source.finish()?);
                        return Ok(());
                    }
                    if active.source.access_budget() == 0
                        && active.source.position() < active.source.len()
                    {
                        active.source.replenish_access_budget(SOURCE_WORK_QUANTUM)?;
                    }
                    let receipt = <M11DirectBlockController as M11ExactController<
                        SnapshotLineSource,
                    >>::poll_source_line(
                        self.controller_mut()?,
                        &mut active.admission,
                        &mut active.source,
                        SOURCE_WORK_QUANTUM,
                    )?;
                    active.matched = receipt.status == M11SourceLinePollStatus::Matched;
                    self.active_line = Some(active);
                    return Ok(());
                }
                if let Some(line) = self.pending_line.take() {
                    let facts = line.facts();
                    let source = line.into_source()?;
                    let admission = <M11DirectBlockController as M11ExactController<
                        SnapshotLineSource,
                    >>::begin_source_line(
                        self.controller_mut()?, facts.identity()
                    )?;
                    self.active_line = Some(ActiveLine {
                        facts,
                        source,
                        admission,
                        matched: false,
                    });
                    return Ok(());
                }

                let poll = self.controller_mut()?.poll_line(1)?;
                match poll.status {
                    M11DirectBlockPollStatus::Pending => {}
                    M11DirectBlockPollStatus::CommandReady => self.offer_pending_command()?,
                    M11DirectBlockPollStatus::ExternalWorkReady => {
                        let controller = self.controller.as_mut().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green controller is missing",
                            ),
                        )?;
                        let writer = self.writer.as_mut().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green writer is missing",
                            ),
                        )?;
                        self.rendezvous = Some(M11ReferenceRendezvous::begin(controller, writer)?);
                    }
                    M11DirectBlockPollStatus::Complete => {
                        self.capture_checkpoint(false)?;
                        self.phase = CleanPhase::Scanning;
                    }
                }
            }
            CleanPhase::Scanning => {
                let scanner = self.scanner.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green scanner baton is missing",
                    ),
                )?;
                let (poll, _) = scanner.poll_counted_retaining_complete(SOURCE_WORK_QUANTUM)?;
                match poll {
                    SnapshotLineRetainedPoll::Pending(scanner) => self.scanner = Some(scanner),
                    SnapshotLineRetainedPoll::Line(line) => {
                        self.pending_line = Some(line);
                        self.phase = CleanPhase::ControllerLine;
                    }
                    SnapshotLineRetainedPoll::Complete(scanner) => {
                        drop(scanner.into_source_lease());
                        self.capture_checkpoint(true)?;
                        self.phase = CleanPhase::BeginFinish;
                    }
                }
            }
            CleanPhase::BeginFinish => {
                self.controller_mut()?.begin_finish()?;
                self.phase = CleanPhase::ControllerFinish;
            }
            CleanPhase::ControllerFinish => {
                let poll = self.controller_mut()?.poll_finish(1)?;
                match poll.status {
                    M11DirectBlockPollStatus::Pending => {}
                    M11DirectBlockPollStatus::CommandReady => {
                        let command = *self.controller_mut()?.pending_command().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "finish command readiness omitted its command",
                            ),
                        )?;
                        if matches!(
                            command,
                            BlockCommand::Close {
                                kind: BlockKind::Document,
                                ..
                            }
                        ) {
                            if self.terminal_convergence.is_some() {
                                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "clean parse encountered two terminal convergence cuts",
                                ));
                            }
                            self.terminal_convergence = Some(
                                self.writer_mut()?
                                    .capture_terminal_convergence_checkpoint()?,
                            );
                        }
                        self.offer_pending_command()?;
                    }
                    M11DirectBlockPollStatus::ExternalWorkReady => {
                        let controller = self.controller.as_mut().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green controller is missing",
                            ),
                        )?;
                        let writer = self.writer.as_mut().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green writer is missing",
                            ),
                        )?;
                        self.rendezvous = Some(M11ReferenceRendezvous::begin(controller, writer)?);
                    }
                    M11DirectBlockPollStatus::Complete => {
                        if self.terminal_convergence.is_none() {
                            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                "clean parse omitted its terminal convergence cut",
                            ));
                        }
                        self.green = Some(self.writer_mut()?.take_root().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "completed recursive-Green writer omitted its root",
                            ),
                        )?);
                        self.journal_mut()?.finish_input(runtime)?;
                        self.phase = CleanPhase::FinishReferences;
                    }
                }
            }
            CleanPhase::FinishReferences => {
                let poll = self.journal_mut()?.poll(runtime, 1)?;
                if poll.status() == M11ReferenceJournalStatus::Complete {
                    self.references = Some(self.journal_mut()?.take_root().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "completed reference journal omitted its root",
                        ),
                    )?);
                    self.writer = None;
                    self.journal = None;
                    self.controller = None;
                    self.output = Some(M11PersistentRecursiveGreenSession {
                        source: self.source,
                        syntax_profile: self.syntax_profile,
                        green: self.green.take(),
                        references: self.references.take(),
                        checkpoints: std::mem::take(&mut self.checkpoints),
                        terminal_convergence: self.terminal_convergence.take(),
                        release_begun: false,
                        green_release_complete: false,
                        references_release_complete: false,
                    });
                    self.phase = CleanPhase::Complete;
                }
            }
            CleanPhase::Complete => {}
        }
        Ok(())
    }

    fn offer_pending_command(&mut self) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let command = *self.controller_mut()?.pending_command().ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "direct controller reported a missing pending command",
            ),
        )?;
        match self.writer_mut()?.offer_command(command)? {
            M11BlockWriterOfferStatus::Complete => self.controller_mut()?.acknowledge_command()?,
            M11BlockWriterOfferStatus::Pending => self.writer_command_pending = true,
        }
        Ok(())
    }

    fn capture_checkpoint(
        &mut self,
        force: bool,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let Some(parser) = self.controller_mut()?.capture_restart_if_available()? else {
            return Ok(());
        };
        let checkpoint = self
            .writer_mut()?
            .capture_restart_checkpoint(parser)
            .map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "direct parser and recursive-Green checkpoint diverged",
                )
            })?;
        let cut = checkpoint.accepted_physical().bytes();
        let previous_cut = self
            .checkpoints
            .last()
            .map_or(0, |previous| previous.accepted_physical().bytes());
        let is_distinct = self
            .checkpoints
            .last()
            .is_none_or(|previous| previous.accepted_physical() != checkpoint.accepted_physical());
        let open_depth = u64::try_from(checkpoint.open_kinds().count()).map_err(|_| {
            M11PersistentRecursiveGreenSessionError::InvalidState("checkpoint open depth fits u64")
        })?;
        let minimum_stride = CHECKPOINT_STRIDE_BYTES
            .checked_mul(open_depth.max(1))
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "checkpoint spacing fits u64",
            ))?;
        let is_spaced_restart = cut.saturating_sub(previous_cut) >= minimum_stride;
        if is_distinct && (force || is_spaced_restart) {
            self.checkpoints.try_reserve(1).map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green checkpoint allocation failed",
                )
            })?;
            self.checkpoints.push(checkpoint);
        }
        Ok(())
    }

    fn capture_document_start_checkpoint(
        &mut self,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        if !self.checkpoints.is_empty() {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green BOF checkpoint must be first",
            ));
        }
        let parser = self.controller_mut()?.capture_document_start_restart()?;
        let checkpoint = self
            .writer_mut()?
            .capture_restart_checkpoint(parser)
            .map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "direct parser and recursive-Green BOF checkpoint diverged",
                )
            })?;
        if checkpoint.accepted_physical() != SourceMetric::default()
            || checkpoint.parser_physical() != SourceMetric::default()
            || checkpoint.logical_metric() != SourceMetric::default()
            || checkpoint.open_kinds().count() != 1
        {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green BOF checkpoint is Document-only at source zero",
            ));
        }
        self.checkpoints.try_reserve(1).map_err(|_| {
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green BOF checkpoint allocation failed",
            )
        })?;
        self.checkpoints.push(checkpoint);
        Ok(())
    }

    fn controller_mut(
        &mut self,
    ) -> Result<&mut M11DirectBlockController, M11PersistentRecursiveGreenSessionError> {
        self.controller
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green controller is missing",
            ))
    }

    fn writer_mut(
        &mut self,
    ) -> Result<&mut M11BlockWriter, M11PersistentRecursiveGreenSessionError> {
        self.writer
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green writer is missing",
            ))
    }

    fn journal_mut(
        &mut self,
    ) -> Result<&mut M11ReferenceJournal, M11PersistentRecursiveGreenSessionError> {
        self.journal
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green reference journal is missing",
            ))
    }

    #[must_use]
    pub fn take_session(&mut self) -> Option<M11PersistentRecursiveGreenSession> {
        (self.phase == CleanPhase::Complete)
            .then(|| self.output.take())
            .flatten()
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        if self.cancelling {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green build cancellation already began",
            ));
        }
        let mut output_release_begun = false;
        if let Some(mut output) = self.output.take() {
            output.begin_release(runtime)?;
            output_release_begun = true;
            self.green = output.green.take();
            self.references = output.references.take();
            self.green_release_complete = output.green_release_complete;
            self.references_release_complete = output.references_release_complete;
        }
        if output_release_begun {
            // Both roots already entered their release lifecycle together.
        } else if let Some(green) = self.green.as_mut() {
            green
                .begin_release(runtime)
                .map_err(M11BlockWriterError::from)?;
        } else {
            self.green_release_complete = true;
        }
        if output_release_begun {
            // Both roots already entered their release lifecycle together.
        } else if let Some(references) = self.references.as_mut() {
            references.begin_release(runtime)?;
        } else {
            self.references_release_complete = true;
        }
        if let Some(writer) = self.writer.as_mut() {
            writer.begin_cancel(runtime)?;
        } else {
            self.writer_cancel_complete = true;
        }
        if let Some(journal) = self.journal.as_mut() {
            journal.begin_cancel(runtime)?;
        } else {
            self.journal_cancel_complete = true;
        }
        self.rendezvous = None;
        self.active_line = None;
        self.pending_line = None;
        self.scanner = None;
        self.controller = None;
        self.cancelling = true;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11PersistentRecursiveGreenBuildPoll, M11PersistentRecursiveGreenSessionError> {
        if !self.cancelling || fuel == 0 {
            return Err(if fuel == 0 {
                M11PersistentRecursiveGreenSessionError::ZeroFuel
            } else {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green build cancellation has not begun",
                )
            });
        }
        let mut transitions = 0;
        while transitions < fuel {
            if !self.writer_cancel_complete {
                let complete = self.writer_mut()?.poll_cancel(runtime, 1)?.complete();
                transitions += 1;
                if complete {
                    self.writer_cancel_complete = true;
                    self.writer = None;
                }
                continue;
            }
            if !self.journal_cancel_complete {
                let complete = self.journal_mut()?.poll_cancel(runtime, 1)?.complete();
                transitions += 1;
                if complete {
                    self.journal_cancel_complete = true;
                    self.journal = None;
                }
                continue;
            }
            if !self.green_release_complete {
                let complete = self
                    .green
                    .as_mut()
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green cancellation lost its root",
                    ))?
                    .poll_release(runtime, 1)
                    .map_err(M11BlockWriterError::from)?
                    .complete();
                transitions += 1;
                if complete {
                    self.green_release_complete = true;
                    self.green = None;
                }
                continue;
            }
            if !self.references_release_complete {
                let complete = self
                    .references
                    .as_mut()
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green cancellation lost its references",
                    ))?
                    .poll_release(runtime, 1)?
                    .complete();
                transitions += 1;
                if complete {
                    self.references_release_complete = true;
                    self.references = None;
                }
                continue;
            }
            break;
        }
        let complete = self.writer_cancel_complete
            && self.journal_cancel_complete
            && self.green_release_complete
            && self.references_release_complete;
        Ok(M11PersistentRecursiveGreenBuildPoll {
            status: if complete {
                M11PersistentRecursiveGreenBuildStatus::Cancelled
            } else {
                M11PersistentRecursiveGreenBuildStatus::Pending
            },
            transitions,
        })
    }
}

/// Persistent structural and reference authority for one exact source.
#[must_use = "persistent recursive-Green sessions require explicit release"]
pub struct M11PersistentRecursiveGreenSession {
    source: SourceVersion,
    syntax_profile: u32,
    green: Option<M11RecursiveGreenRoot>,
    references: Option<M11ReferenceJournalRoot>,
    checkpoints: Vec<M11BlockRestartCheckpoint>,
    terminal_convergence: Option<M11BlockTerminalConvergenceCheckpoint>,
    release_begun: bool,
    green_release_complete: bool,
    references_release_complete: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CheckpointStorageReceipt {
    checkpoints: usize,
    retained_open_frames: usize,
    maximum_open_depth: usize,
}

#[cfg(test)]
impl CheckpointStorageReceipt {
    #[must_use]
    const fn checkpoints(self) -> usize {
        self.checkpoints
    }

    #[must_use]
    const fn retained_open_frames(self) -> usize {
        self.retained_open_frames
    }

    #[must_use]
    const fn maximum_open_depth(self) -> usize {
        self.maximum_open_depth
    }
}

/// A failed local-adoption start returns the still-owned base session so the
/// endpoint can choose its explicit clean fallback without leaking roots.
pub struct M11PersistentRecursiveGreenAdoptionStartFailure {
    error: M11PersistentRecursiveGreenSessionError,
    base: M11PersistentRecursiveGreenSession,
}

impl M11PersistentRecursiveGreenAdoptionStartFailure {
    #[must_use]
    pub const fn error(&self) -> &M11PersistentRecursiveGreenSessionError {
        &self.error
    }

    #[must_use]
    pub fn into_base(self) -> M11PersistentRecursiveGreenSession {
        self.base
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11PersistentRecursiveGreenAdoptionStatus {
    Pending,
    Complete,
    CleanFallbackRequired,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PersistentRecursiveGreenAdoptionPoll {
    status: M11PersistentRecursiveGreenAdoptionStatus,
    transitions: usize,
}

impl M11PersistentRecursiveGreenAdoptionPoll {
    #[must_use]
    pub const fn status(self) -> M11PersistentRecursiveGreenAdoptionStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11PersistentRecursiveGreenAdoptionWork {
    source_bytes_read: usize,
    high_level_events: usize,
    green_tree_nodes_rebuilt: usize,
    reference_rebind_transitions: usize,
}

impl M11PersistentRecursiveGreenAdoptionWork {
    #[must_use]
    pub const fn source_bytes_read(self) -> usize {
        self.source_bytes_read
    }

    #[must_use]
    pub const fn high_level_events(self) -> usize {
        self.high_level_events
    }

    #[must_use]
    pub const fn green_tree_nodes_rebuilt(self) -> usize {
        self.green_tree_nodes_rebuilt
    }

    #[must_use]
    pub const fn reference_rebind_transitions(self) -> usize {
        self.reference_rebind_transitions
    }
}

/// Atomic target plus the superseded base that remains live until endpoint
/// delivery commits the matching structural publication.
#[must_use = "recursive-Green updates require target installation and base release"]
pub struct M11PersistentRecursiveGreenUpdate {
    base: Option<M11PersistentRecursiveGreenSession>,
    target: Option<M11PersistentRecursiveGreenSession>,
    work: M11PersistentRecursiveGreenAdoptionWork,
    recursive_green_splice: M11RecursiveGreenStructuralSpliceSelection,
}

/// Unforgeable, crate-private proof that both sides of an exact structural
/// update and its adopted reference authority are still live.
///
/// Only a completed [`M11PersistentRecursiveGreenUpdate`] can mint this
/// borrow. Publication uses it to join the retained base, authenticated target
/// and exact Green event segment selection without accepting a caller-supplied
/// reuse flag.
pub(crate) struct M11PersistentRecursiveGreenExactPublication<'update> {
    base: &'update M11PersistentRecursiveGreenSession,
    target: &'update M11PersistentRecursiveGreenSession,
    recursive_green_splice: &'update M11RecursiveGreenStructuralSpliceSelection,
}

impl M11PersistentRecursiveGreenExactPublication<'_> {
    pub(crate) const fn base_session(&self) -> &M11PersistentRecursiveGreenSession {
        self.base
    }

    pub(crate) const fn target_session(&self) -> &M11PersistentRecursiveGreenSession {
        self.target
    }

    pub(crate) const fn recursive_green_splice_selection(
        &self,
    ) -> &M11RecursiveGreenStructuralSpliceSelection {
        self.recursive_green_splice
    }
}

impl M11PersistentRecursiveGreenUpdate {
    #[must_use]
    pub fn target_source(&self) -> SourceVersion {
        self.target
            .as_ref()
            .expect("target session is present")
            .source
    }

    #[must_use]
    pub fn target_session(&self) -> &M11PersistentRecursiveGreenSession {
        self.target.as_ref().expect("target session is present")
    }

    #[must_use]
    pub const fn work(&self) -> M11PersistentRecursiveGreenAdoptionWork {
        self.work
    }

    /// Exact changed Green leaf segments removed from the base and inserted in
    /// the target. These survive independently of aggregate adoption work so
    /// an exact-base publisher never has to infer sparse repairs from counts.
    #[must_use]
    pub const fn recursive_green_splice_selection(
        &self,
    ) -> &M11RecursiveGreenStructuralSpliceSelection {
        &self.recursive_green_splice
    }

    /// Fallibly duplicates the exact sparse selection for publication. The
    /// endpoint inspects the borrowed segment count first and uses whole-role
    /// transport when it exceeds the wire envelope.
    pub fn try_clone_recursive_green_splice_selection(
        &self,
    ) -> Result<M11RecursiveGreenStructuralSpliceSelection, M11PersistentRecursiveGreenSessionError>
    {
        self.recursive_green_splice.try_clone().map_err(Into::into)
    }

    pub(crate) fn exact_publication(
        &self,
    ) -> Result<
        M11PersistentRecursiveGreenExactPublication<'_>,
        M11PersistentRecursiveGreenSessionError,
    > {
        let base =
            self.base
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green exact publication omitted its base session",
                ))?;
        let target =
            self.target
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green exact publication omitted its target session",
                ))?;
        if base.release_begun
            || target.release_begun
            || base.source == target.source
            || base.syntax_profile != target.syntax_profile
            || base.green.is_none()
            || target.green.is_none()
            || base.references.is_none()
            || target.references.is_none()
        {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green exact publication authority is no longer live",
            ));
        }
        Ok(M11PersistentRecursiveGreenExactPublication {
            base,
            target,
            recursive_green_splice: &self.recursive_green_splice,
        })
    }

    #[must_use]
    pub fn take_target(&mut self) -> Option<M11PersistentRecursiveGreenSession> {
        self.target.take()
    }

    #[must_use]
    pub fn take_base(&mut self) -> Option<M11PersistentRecursiveGreenSession> {
        self.base.take()
    }
}

struct ZeroReferenceOccurrenceProof(());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdoptionPhase {
    ParseFragment,
    ProbeOrdinaryConvergence,
    BeginTerminalFinish,
    FinishTerminal,
    AdoptGreen,
    AdoptReferences,
    Complete,
    CleanFallbackRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdoptionConvergence {
    Ordinary { checkpoint_index: usize },
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdoptionCheckpointSelection {
    transaction_id: u64,
    restart_index: usize,
    convergence: AdoptionConvergence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LaterConvergenceSearch {
    first_target_parser_end: usize,
    first_base_line_ordinal: u64,
    physical_lines: u64,
    transitions: usize,
}

struct RebasedOrdinaryCheckpointSet {
    checkpoints: Vec<M11BlockRestartCheckpoint>,
    terminal: M11BlockTerminalConvergenceCheckpoint,
}

enum AdoptedCheckpointSet {
    Ordinary(RebasedOrdinaryCheckpointSet),
    Terminal {
        checkpoints: Vec<M11BlockRestartCheckpoint>,
        terminal: M11BlockTerminalConvergenceCheckpoint,
    },
}

fn replicate_base_checkpoint_range(
    base: &M11PersistentRecursiveGreenSession,
    range: Range<usize>,
    transaction_id: u64,
) -> Result<Vec<M11BlockRestartCheckpoint>, M11BlockRestartError> {
    let checkpoints = base
        .checkpoints
        .get(range)
        .ok_or(M11BlockRestartError::Pairing(
            "retained checkpoint range escaped the base session",
        ))?;
    let mut replicas = Vec::new();
    replicas
        .try_reserve_exact(checkpoints.len())
        .map_err(|_| M11BlockWriterError::Allocation)?;
    for checkpoint in checkpoints {
        replicas.push(
            checkpoint
                .replicate_for_transaction(transaction_id)?
                .into_checkpoint(transaction_id)?,
        );
    }
    Ok(replicas)
}

/// Proves that one parser checkpoint cannot split a committed reference
/// occurrence. A cut at or beyond the final occurrence is trivially safe. An
/// earlier cut is safe only when no Paragraph is open: reference occurrences
/// are recognized from Paragraph prefixes and are committed before that
/// Paragraph leaves the open parser path.
fn checkpoint_proves_reference_occurrence_cut(
    checkpoint: &M11BlockRestartCheckpoint,
    references: &M11ReferenceJournalRoot,
) -> bool {
    let cut = checkpoint.parser_physical();
    (cut.bytes() >= references.last_source_byte_end()
        && cut.utf16() >= references.last_source_utf16_end())
        || !checkpoint
            .open_kinds()
            .any(|kind| matches!(kind, BlockKind::Paragraph))
}

/// Fuelled same-island restart/convergence adoption. Parser-authenticated
/// reference work is journalled into the same atomic target revision as its
/// recursive-Green replacement.
#[must_use = "recursive-Green adoption requires update transfer or cancellation"]
pub struct M11PersistentRecursiveGreenAdoption {
    base: Option<M11PersistentRecursiveGreenSession>,
    target: SourceVersion,
    phase: AdoptionPhase,
    scanner: Option<SnapshotLineScanner>,
    pending_line: Option<SnapshotPhysicalLine>,
    active_line: Option<ActiveLine>,
    current_line_end: Option<usize>,
    target_parser_end: usize,
    controller: Option<M11DirectBlockController>,
    writer: Option<M11BlockWriter>,
    writer_command_pending: bool,
    target_restart: Option<M11BlockRestartCheckpoint>,
    checkpoint_selection: AdoptionCheckpointSelection,
    later_convergence_search: Option<LaterConvergenceSearch>,
    green_prefix: Option<ExactUnchangedPrefixWitness>,
    document_start: bool,
    green_suffix: Option<ExactUnchangedSuffixWitness>,
    reference_prefix: Option<ExactUnchangedPrefixWitness>,
    reference_range_prefix: Option<ExactUnchangedPrefixWitness>,
    reference_range_base_start: SourceMetric,
    reference_range_pending: bool,
    reference_range_ready: bool,
    pending_reference_rendezvous: bool,
    reference_rendezvous: Option<M11ReferenceRendezvous>,
    reference_replacement: Option<M11ReferenceJournalRangeReplacement>,
    reference_replacement_finishing: bool,
    reference_adoption: Option<M11ReferenceJournalUnchangedPrefixAdoption>,
    target_green: Option<M11RecursiveGreenRoot>,
    adopted_checkpoints: Option<AdoptedCheckpointSet>,
    recursive_green_splice: Option<M11RecursiveGreenStructuralSpliceSelection>,
    output: Option<M11PersistentRecursiveGreenUpdate>,
    work: M11PersistentRecursiveGreenAdoptionWork,
    cancelling: bool,
    cancel_target: Option<M11PersistentRecursiveGreenSession>,
    cancel_green_complete: bool,
    cancel_references_complete: bool,
}

impl fmt::Debug for M11PersistentRecursiveGreenSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11PersistentRecursiveGreenSession")
            .field("source", &self.source)
            .field("syntax_profile", &self.syntax_profile)
            .field("restart_checkpoints", &self.checkpoints.len())
            .finish_non_exhaustive()
    }
}

impl M11PersistentRecursiveGreenAdoption {
    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11PersistentRecursiveGreenAdoptionPoll, M11PersistentRecursiveGreenSessionError>
    {
        if fuel == 0 {
            return Err(M11PersistentRecursiveGreenSessionError::ZeroFuel);
        }
        if self.cancelling {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "cancelled recursive-Green adoption cannot resume",
            ));
        }
        if runtime.current_source_version() != Some(self.target) {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption target is not current",
            ));
        }
        let mut transitions = 0;
        while transitions < fuel
            && !matches!(
                self.phase,
                AdoptionPhase::Complete | AdoptionPhase::CleanFallbackRequired
            )
        {
            self.poll_one(runtime)?;
            transitions += 1;
        }
        Ok(M11PersistentRecursiveGreenAdoptionPoll {
            status: match self.phase {
                AdoptionPhase::Complete => M11PersistentRecursiveGreenAdoptionStatus::Complete,
                AdoptionPhase::CleanFallbackRequired => {
                    M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired
                }
                _ => M11PersistentRecursiveGreenAdoptionStatus::Pending,
            },
            transitions,
        })
    }

    fn poll_one(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        if let Some(search) = self.later_convergence_search.as_mut() {
            if search.transitions >= LATER_CONVERGENCE_MAX_TRANSITIONS {
                self.phase = AdoptionPhase::CleanFallbackRequired;
                return Ok(());
            }
            search.transitions += 1;
        }
        if self.reference_range_pending {
            if self.reference_replacement.is_some() {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "reference range replacement was started twice",
                ));
            }
            let prefix = self.reference_range_prefix.take();
            let start = self.reference_range_base_start;
            let replacement = {
                let base = self.base.as_ref().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted its base session",
                    ),
                )?;
                base.references
                    .as_ref()
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green base omitted its reference root",
                    ))?
                    .begin_range_replacement(
                        runtime,
                        start.bytes() as usize,
                        start.utf16() as usize,
                        prefix,
                    )?
            };
            self.reference_replacement = Some(replacement);
            self.reference_range_pending = false;
            return Ok(());
        }
        if self.reference_replacement.is_some()
            && !self.reference_range_ready
            && !self.reference_replacement_finishing
        {
            let poll = self
                .reference_replacement
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "reference replacement actor disappeared",
                ))?
                .poll(runtime, 1)?;
            self.record_reference_transitions(poll.transitions())?;
            match poll.status() {
                M11ReferenceJournalRangeReplacementStatus::Pending => {}
                M11ReferenceJournalRangeReplacementStatus::NeedsReplacementInput => {
                    self.reference_range_ready = true;
                }
                M11ReferenceJournalRangeReplacementStatus::Complete
                | M11ReferenceJournalRangeReplacementStatus::Cancelled => {
                    return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "reference replacement completed before parser input",
                    ));
                }
            }
            return Ok(());
        }
        if self.pending_reference_rendezvous {
            if !self.reference_range_ready || self.reference_rendezvous.is_some() {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "reference rendezvous began without a ready replacement journal",
                ));
            }
            let rendezvous = {
                let controller = self.controller.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green crop controller is missing",
                    ),
                )?;
                let writer = self.writer.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green crop writer is missing",
                    ),
                )?;
                match M11ReferenceRendezvous::begin(controller, writer) {
                    Ok(rendezvous) => rendezvous,
                    Err(M11ReferenceRendezvousError::Writer(
                        M11BlockWriterError::ReferenceParagraphPredatesRestart,
                    )) => {
                        // The active range actor still owns every retained and
                        // replacement References resource. Leave it reachable
                        // for the ordinary fallback cancellation path.
                        self.phase = AdoptionPhase::CleanFallbackRequired;
                        return Ok(());
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            self.reference_rendezvous = Some(rendezvous);
            self.pending_reference_rendezvous = false;
            return Ok(());
        }
        if let Some(mut rendezvous) = self.reference_rendezvous.take() {
            let poll = {
                let controller = self.controller.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green crop controller is missing",
                    ),
                )?;
                let writer = self.writer.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green crop writer is missing",
                    ),
                )?;
                let journal = self
                    .reference_replacement
                    .as_mut()
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "reference rendezvous omitted its range replacement actor",
                    ))?
                    .replacement_journal_mut()?;
                rendezvous.poll(controller, writer, journal, runtime, 1)?
            };
            self.record_reference_transitions(poll.transitions)?;
            if poll.status != M11ReferenceRendezvousStatus::Complete {
                self.reference_rendezvous = Some(rendezvous);
            } else {
                // A remainder checkpoint is an optimization inside this
                // bounded crop. The authenticated convergence checkpoint
                // remains the target session's durable restart authority.
                drop(rendezvous.take_leading_reference_remainder());
            }
            return Ok(());
        }
        if self.reference_replacement_finishing {
            let poll = self
                .reference_replacement
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "finishing reference replacement actor disappeared",
                ))?
                .poll(runtime, 1)?;
            self.record_reference_transitions(poll.transitions())?;
            match poll.status() {
                M11ReferenceJournalRangeReplacementStatus::Pending => {}
                M11ReferenceJournalRangeReplacementStatus::Complete => {
                    let references = self
                        .reference_replacement
                        .as_mut()
                        .and_then(M11ReferenceJournalRangeReplacement::take_root)
                        .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "completed reference replacement omitted its root",
                        ))?;
                    self.reference_replacement = None;
                    self.reference_replacement_finishing = false;
                    self.finish_target_with_references(references)?;
                }
                M11ReferenceJournalRangeReplacementStatus::NeedsReplacementInput
                | M11ReferenceJournalRangeReplacementStatus::Cancelled => {
                    return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "reference replacement did not finish after convergence",
                    ));
                }
            }
            return Ok(());
        }
        if let Some(adoption) = self.reference_adoption.as_mut() {
            let poll = adoption.poll(runtime, 1)?;
            let transitions = poll.transitions();
            let complete = poll.status() == M11ReferenceJournalAdoptionStatus::Complete;
            let _ = adoption;
            self.record_reference_transitions(transitions)?;
            if complete {
                let references = self
                    .reference_adoption
                    .as_mut()
                    .and_then(M11ReferenceJournalUnchangedPrefixAdoption::take_root)
                    .ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "completed reference adoption omitted its root",
                    ),
                )?;
                self.reference_adoption = None;
                self.finish_target_with_references(references)?;
            }
            return Ok(());
        }
        if self.writer_command_pending {
            let poll = self.writer_mut()?.poll(runtime, 1)?;
            if matches!(
                poll.status(),
                M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete
            ) {
                self.writer_command_pending = false;
                self.controller_mut()?.acknowledge_command()?;
            }
            return Ok(());
        }

        match self.phase {
            AdoptionPhase::ParseFragment => {
                if let Some(mut active) = self.active_line.take() {
                    if active.matched {
                        if let Some(search) = self.later_convergence_search.as_mut() {
                            search.physical_lines = search.physical_lines.checked_add(1).ok_or(
                                M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "later convergence physical-line work overflow",
                                ),
                            )?;
                        }
                        self.current_line_end = Some(active.facts.identity().end_byte() as usize);
                        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
                            self.controller_mut()?,
                            active.admission,
                            active.facts,
                        )?;
                        self.scanner = Some(active.source.finish()?);
                        return Ok(());
                    }
                    if active.source.access_budget() == 0
                        && active.source.position() < active.source.len()
                    {
                        active.source.replenish_access_budget(SOURCE_WORK_QUANTUM)?;
                    }
                    let receipt = <M11DirectBlockController as M11ExactController<
                        SnapshotLineSource,
                    >>::poll_source_line(
                        self.controller_mut()?,
                        &mut active.admission,
                        &mut active.source,
                        SOURCE_WORK_QUANTUM,
                    )?;
                    active.matched = receipt.status == M11SourceLinePollStatus::Matched;
                    self.active_line = Some(active);
                    return Ok(());
                }
                if let Some(line) = self.pending_line.take() {
                    if self.later_convergence_search.is_some_and(|search| {
                        search.physical_lines >= LATER_CONVERGENCE_MAX_PHYSICAL_LINES
                    }) {
                        self.phase = AdoptionPhase::CleanFallbackRequired;
                        return Ok(());
                    }
                    let facts = line.facts();
                    if facts.identity().end_byte() as usize > self.target_parser_end {
                        return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green crop crossed its convergence line boundary",
                        ));
                    }
                    let source = line.into_source()?;
                    let admission = <M11DirectBlockController as M11ExactController<
                        SnapshotLineSource,
                    >>::begin_source_line(
                        self.controller_mut()?, facts.identity()
                    )?;
                    self.active_line = Some(ActiveLine {
                        facts,
                        source,
                        admission,
                        matched: false,
                    });
                    return Ok(());
                }
                if self.current_line_end.is_some() {
                    let poll = self.controller_mut()?.poll_line(1)?;
                    match poll.status {
                        M11DirectBlockPollStatus::Pending => {}
                        M11DirectBlockPollStatus::CommandReady => self.offer_pending_command()?,
                        M11DirectBlockPollStatus::ExternalWorkReady => {
                            if self.reference_replacement.is_none() {
                                self.reference_range_pending = true;
                                self.reference_range_ready = false;
                            }
                            self.pending_reference_rendezvous = true;
                        }
                        M11DirectBlockPollStatus::Complete => {
                            let end = self.current_line_end.take().ok_or(
                                M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "recursive-Green crop lost its line end",
                                ),
                            )?;
                            if end == self.target_parser_end {
                                self.phase = if matches!(
                                    self.checkpoint_selection.convergence,
                                    AdoptionConvergence::Terminal
                                ) {
                                    AdoptionPhase::BeginTerminalFinish
                                } else {
                                    AdoptionPhase::ProbeOrdinaryConvergence
                                };
                            } else if end > self.target_parser_end {
                                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "recursive-Green crop crossed convergence",
                                ));
                            }
                        }
                    }
                    return Ok(());
                }

                let scanner = self.scanner.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green crop scanner baton is missing",
                    ),
                )?;
                if self.later_convergence_search.is_some_and(|search| {
                    search.physical_lines >= LATER_CONVERGENCE_MAX_PHYSICAL_LINES
                }) {
                    self.scanner = Some(scanner);
                    self.phase = AdoptionPhase::CleanFallbackRequired;
                    return Ok(());
                }
                let (poll, _) = scanner.poll_counted_retaining_complete(SOURCE_WORK_QUANTUM)?;
                match poll {
                    SnapshotLineRetainedPoll::Pending(scanner) => self.scanner = Some(scanner),
                    SnapshotLineRetainedPoll::Line(line) => self.pending_line = Some(line),
                    SnapshotLineRetainedPoll::Complete(scanner) => {
                        drop(scanner.into_source_lease());
                        if matches!(
                            self.checkpoint_selection.convergence,
                            AdoptionConvergence::Terminal
                        ) && self.target_parser_end == self.target.byte_len()
                        {
                            self.phase = AdoptionPhase::BeginTerminalFinish;
                        } else if self.later_convergence_search.is_some() {
                            self.phase = AdoptionPhase::CleanFallbackRequired;
                        } else {
                            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green crop reached EOF before convergence",
                            ));
                        }
                    }
                }
            }
            AdoptionPhase::ProbeOrdinaryConvergence => {
                if self.probe_ordinary_convergence(runtime)? {
                    self.later_convergence_search = None;
                    self.phase = AdoptionPhase::AdoptGreen;
                } else {
                    self.advance_ordinary_convergence(runtime)?;
                }
            }
            AdoptionPhase::BeginTerminalFinish => {
                self.controller_mut()?.begin_finish()?;
                self.phase = AdoptionPhase::FinishTerminal;
            }
            AdoptionPhase::FinishTerminal => {
                if matches!(
                    self.controller_mut()?.pending_command(),
                    Some(BlockCommand::Close {
                        kind: BlockKind::Document,
                        ..
                    })
                ) {
                    self.later_convergence_search = None;
                    self.phase = AdoptionPhase::AdoptGreen;
                    return Ok(());
                }
                let poll = self.controller_mut()?.poll_finish(1)?;
                match poll.status {
                    M11DirectBlockPollStatus::Pending => {}
                    M11DirectBlockPollStatus::CommandReady => {
                        if matches!(
                            self.controller_mut()?.pending_command(),
                            Some(BlockCommand::Close {
                                kind: BlockKind::Document,
                                ..
                            })
                        ) {
                            self.later_convergence_search = None;
                            self.phase = AdoptionPhase::AdoptGreen;
                        } else {
                            self.offer_pending_command()?;
                        }
                    }
                    M11DirectBlockPollStatus::ExternalWorkReady => {
                        if self.reference_replacement.is_none() {
                            self.reference_range_pending = true;
                            self.reference_range_ready = false;
                        }
                        self.pending_reference_rendezvous = true;
                    }
                    M11DirectBlockPollStatus::Complete => {
                        return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "terminal convergence passed Close(Document)",
                        ));
                    }
                }
            }
            AdoptionPhase::AdoptGreen => {
                let selection = self.checkpoint_selection;
                let terminal_close =
                    if matches!(selection.convergence, AdoptionConvergence::Terminal) {
                        Some(*self.controller_mut()?.pending_command().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "terminal convergence omitted its pending Document close",
                            ),
                        )?)
                    } else {
                        None
                    };
                let writer = self.writer.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green crop writer is missing",
                    ),
                )?;
                let target_restart = self.target_restart.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted target restart",
                    ),
                )?;
                let green_prefix = self.green_prefix.take();
                if green_prefix.is_none() && !self.document_start {
                    return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted prefix lineage",
                    ));
                }
                let green_suffix = self.green_suffix.take();
                let reference_prefix = self.reference_prefix.take();
                let parser =
                    if matches!(selection.convergence, AdoptionConvergence::Ordinary { .. }) {
                        Some(self.controller_mut()?.capture_restart()?)
                    } else {
                        None
                    };
                let adoption_result = {
                    let base = self.base.as_ref().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green adoption omitted its base session",
                        ),
                    )?;
                    let green_base = base.green.as_ref().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green base omitted its structural root",
                        ),
                    )?;
                    match selection.convergence {
                        AdoptionConvergence::Ordinary { checkpoint_index } => {
                            let old_convergence = base
                                .checkpoints
                                .get(checkpoint_index)
                                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "ordinary convergence checkpoint index escaped the base",
                                ))?
                                .replicate_for_transaction(selection.transaction_id)?
                                .into_checkpoint(selection.transaction_id)?;
                            let retained_prefix = replicate_base_checkpoint_range(
                                base,
                                0..selection.restart_index,
                                selection.transaction_id,
                            )?;
                            let retained_suffix = replicate_base_checkpoint_range(
                                base,
                                checkpoint_index + 1..base.checkpoints.len(),
                                selection.transaction_id,
                            )?;
                            let retained_terminal = base
                                .terminal_convergence
                                .as_ref()
                                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "ordinary adoption omitted terminal checkpoint authority",
                                ))?
                                .replicate_for_transaction(selection.transaction_id)?
                                .into_checkpoint(selection.transaction_id)?;
                            writer
                                .adopt_converged_fragment(
                                    parser.ok_or(
                                        M11PersistentRecursiveGreenSessionError::InvalidState(
                                            "ordinary convergence omitted its parser restart",
                                        ),
                                    )?,
                                    target_restart,
                                    old_convergence,
                                    runtime,
                                    green_base,
                                    green_prefix,
                                    green_suffix,
                                    retained_prefix,
                                    retained_suffix,
                                    retained_terminal,
                                )
                                .map(|(green, receipt, checkpoints, terminal)| {
                                    (
                                        green,
                                        receipt,
                                        AdoptedCheckpointSet::Ordinary(
                                            RebasedOrdinaryCheckpointSet {
                                                checkpoints,
                                                terminal,
                                            },
                                        ),
                                    )
                                })
                        }
                        AdoptionConvergence::Terminal => {
                            let old_terminal = base
                                .terminal_convergence
                                .as_ref()
                                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "terminal adoption omitted terminal checkpoint authority",
                                ))?
                                .replicate_for_transaction(selection.transaction_id)?
                                .into_checkpoint(selection.transaction_id)?;
                            let retained_prefix = replicate_base_checkpoint_range(
                                base,
                                0..selection.restart_index,
                                selection.transaction_id,
                            )?;
                            writer
                                .adopt_converged_terminal_fragment(
                                    terminal_close.ok_or(
                                        M11PersistentRecursiveGreenSessionError::InvalidState(
                                            "terminal convergence omitted its close state",
                                        ),
                                    )?,
                                    target_restart,
                                    old_terminal,
                                    runtime,
                                    green_base,
                                    green_prefix,
                                    retained_prefix,
                                )
                                .map(|(green, receipt, checkpoints, terminal)| {
                                    (
                                        green,
                                        receipt,
                                        AdoptedCheckpointSet::Terminal {
                                            checkpoints,
                                            terminal,
                                        },
                                    )
                                })
                        }
                    }
                };
                let (green, receipt, checkpoints) = match adoption_result {
                    Ok(result) => result,
                    Err(
                        M11BlockRestartError::Pairing(
                            "target fragment did not converge to its exact base boundary",
                        )
                        | M11BlockRestartError::Pairing(
                            "target tail did not converge at the pre-Document-close boundary",
                        )
                        | M11BlockRestartError::Pairing(
                            "ordinary spanning Exit state requires clean fallback",
                        ),
                    ) => {
                        self.controller = None;
                        self.phase = AdoptionPhase::CleanFallbackRequired;
                        return Ok(());
                    }
                    Err(error) => return Err(error.into()),
                };
                // Install move-only Green authority before any later fallible
                // work so cancellation always has a root it can release.
                self.target_green = Some(green);
                self.adopted_checkpoints = Some(checkpoints);
                self.controller = None;
                self.record_structural_work(receipt)?;
                if let Some(replacement) = self.reference_replacement.as_mut() {
                    let base = self.base.as_ref().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green adoption omitted its base session",
                        ),
                    )?;
                    let convergence = match selection.convergence {
                        AdoptionConvergence::Ordinary { checkpoint_index } => base
                            .checkpoints
                            .get(checkpoint_index)
                            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                                "reference convergence checkpoint escaped the base",
                            ))?
                            .parser_physical(),
                        AdoptionConvergence::Terminal => SourceMetric::new(
                            base.source.byte_len() as u64,
                            base.source.utf16_len() as u64,
                        )
                        .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "reference terminal source metric is invalid",
                        ))?,
                    };
                    let suffix = if convergence.bytes() as usize == base.source.byte_len()
                        && convergence.utf16() as usize == base.source.utf16_len()
                    {
                        None
                    } else {
                        let suffix = runtime.mint_exact_unchanged_suffix_witness(
                            base.source,
                            convergence.bytes() as usize,
                            convergence.utf16() as usize,
                        )?;
                        if suffix.target_byte_start() != self.target_parser_end {
                            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                "reference suffix differs from parser convergence",
                            ));
                        }
                        Some(suffix)
                    };
                    replacement.finish_replacement(runtime, suffix)?;
                    self.reference_range_ready = false;
                    self.reference_replacement_finishing = true;
                } else {
                    self.reference_range_prefix = None;
                    let reference_adoption = {
                    let base = self.base.as_ref().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green adoption omitted its base session",
                        ),
                    )?;
                    let references = base.references.as_ref().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green base omitted its reference root",
                        ),
                    )?;
                    begin_reference_adoption(
                        references,
                        runtime,
                        reference_prefix,
                        ZeroReferenceOccurrenceProof(()),
                    )?
                    };
                    self.reference_adoption = Some(reference_adoption);
                }
                self.phase = AdoptionPhase::AdoptReferences;
            }
            AdoptionPhase::AdoptReferences => {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green reference adoption actor is missing",
                ));
            }
            AdoptionPhase::Complete | AdoptionPhase::CleanFallbackRequired => {}
        }
        Ok(())
    }

    fn probe_ordinary_convergence(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<bool, M11PersistentRecursiveGreenSessionError> {
        let Some(parser) = self
            .controller
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green crop controller is missing",
            ))?
            .capture_restart_if_available()?
        else {
            return Ok(false);
        };
        let AdoptionConvergence::Ordinary { checkpoint_index } =
            self.checkpoint_selection.convergence
        else {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "ordinary convergence probe selected the terminal boundary",
            ));
        };
        let base =
            self.base
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green adoption omitted its base session",
                ))?;
        let old_convergence = base.checkpoints.get(checkpoint_index).ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "ordinary convergence checkpoint index escaped the base",
            ),
        )?;
        if self.reference_replacement.is_some()
            && !checkpoint_proves_reference_occurrence_cut(
                old_convergence,
                base.references.as_ref().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green base omitted its reference root",
                    ),
                )?,
            )
        {
            // A range-replacement suffix can replay only occurrences beginning
            // at or after the selected cut. Keep parsing until a later
            // checkpoint proves that no occurrence crosses that cut.
            return Ok(false);
        }
        let green_base =
            base.green
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green base omitted its structural root",
                ))?;
        let writer =
            self.writer
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green crop writer is missing",
                ))?;
        let target_restart = self.target_restart.as_ref().ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption omitted target restart",
            ),
        )?;
        Ok(writer.probe_converged_fragment(
            parser,
            target_restart,
            old_convergence,
            runtime,
            green_base,
            self.green_prefix.as_ref(),
            self.green_suffix.as_ref(),
        )?)
    }

    fn advance_ordinary_convergence(
        &mut self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let AdoptionConvergence::Ordinary { checkpoint_index } =
            self.checkpoint_selection.convergence
        else {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "later ordinary convergence search selected the terminal boundary",
            ));
        };
        let base =
            self.base
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green adoption omitted its base session",
                ))?;
        let current = base.checkpoints.get(checkpoint_index).ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "ordinary convergence checkpoint index escaped the base",
            ),
        )?;
        let search = self
            .later_convergence_search
            .get_or_insert(LaterConvergenceSearch {
                first_target_parser_end: self.target_parser_end,
                first_base_line_ordinal: current.next_line_ordinal(),
                physical_lines: 0,
                transitions: 0,
            });
        let first_target_parser_end = search.first_target_parser_end;
        let first_base_line_ordinal = search.first_base_line_ordinal;
        let Some(next_index) = checkpoint_index.checked_add(1) else {
            return self.select_terminal_convergence_if_bounded();
        };
        let Some(next) = base.checkpoints.get(next_index) else {
            return self.select_terminal_convergence_if_bounded();
        };
        let parser_convergence = next.parser_physical();
        if parser_convergence.bytes() as usize == base.source.byte_len()
            && parser_convergence.utf16() as usize == base.source.utf16_len()
        {
            return self.select_terminal_convergence_if_bounded();
        }
        let physical_lines = next
            .next_line_ordinal()
            .checked_sub(first_base_line_ordinal)
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "later convergence checkpoint line ordinals regressed",
            ))?;
        if physical_lines > LATER_CONVERGENCE_MAX_PHYSICAL_LINES {
            self.phase = AdoptionPhase::CleanFallbackRequired;
            return Ok(());
        }
        let parser_suffix = runtime.mint_exact_unchanged_suffix_witness(
            base.source,
            parser_convergence.bytes() as usize,
            parser_convergence.utf16() as usize,
        )?;
        let target_parser_end = parser_suffix.target_byte_start();
        let later_bytes = target_parser_end
            .checked_sub(first_target_parser_end)
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "later convergence target boundary regressed",
            ))?;
        if later_bytes > LATER_CONVERGENCE_MAX_BYTES {
            self.phase = AdoptionPhase::CleanFallbackRequired;
            return Ok(());
        }
        let green_convergence = next.accepted_physical();
        let green_suffix = if green_convergence.bytes() as usize == base.source.byte_len()
            && green_convergence.utf16() as usize == base.source.utf16_len()
        {
            None
        } else {
            Some(runtime.mint_exact_unchanged_suffix_witness(
                base.source,
                green_convergence.bytes() as usize,
                green_convergence.utf16() as usize,
            )?)
        };

        self.checkpoint_selection.convergence = AdoptionConvergence::Ordinary {
            checkpoint_index: next_index,
        };
        self.target_parser_end = target_parser_end;
        self.green_suffix = green_suffix;
        self.phase = AdoptionPhase::ParseFragment;
        Ok(())
    }

    fn select_terminal_convergence_if_bounded(
        &mut self,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let search = self.later_convergence_search.ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "terminal convergence fallback omitted its local search envelope",
            ),
        )?;
        let terminal_bytes = self
            .target
            .byte_len()
            .checked_sub(search.first_target_parser_end)
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "terminal convergence target boundary regressed",
            ))?;
        if terminal_bytes > LATER_CONVERGENCE_MAX_BYTES
            || search.physical_lines > LATER_CONVERGENCE_MAX_PHYSICAL_LINES
        {
            self.phase = AdoptionPhase::CleanFallbackRequired;
            return Ok(());
        }
        self.checkpoint_selection.convergence = AdoptionConvergence::Terminal;
        self.target_parser_end = self.target.byte_len();
        self.green_suffix = None;
        self.phase = AdoptionPhase::ParseFragment;
        Ok(())
    }

    fn offer_pending_command(&mut self) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let command = *self.controller_mut()?.pending_command().ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "direct controller reported a missing crop command",
            ),
        )?;
        match self.writer_mut()?.offer_command(command)? {
            M11BlockWriterOfferStatus::Complete => self.controller_mut()?.acknowledge_command()?,
            M11BlockWriterOfferStatus::Pending => self.writer_command_pending = true,
        }
        Ok(())
    }

    fn controller_mut(
        &mut self,
    ) -> Result<&mut M11DirectBlockController, M11PersistentRecursiveGreenSessionError> {
        self.controller
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green crop controller is missing",
            ))
    }

    fn writer_mut(
        &mut self,
    ) -> Result<&mut M11BlockWriter, M11PersistentRecursiveGreenSessionError> {
        self.writer
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green crop writer is missing",
            ))
    }

    fn record_reference_transitions(
        &mut self,
        transitions: usize,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        self.work.reference_rebind_transitions = self
            .work
            .reference_rebind_transitions
            .checked_add(transitions)
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "reference update work overflow",
            ))?;
        Ok(())
    }

    fn finish_target_with_references(
        &mut self,
        references: M11ReferenceJournalRoot,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let base = self.base.take().ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption omitted its base session",
            ),
        )?;
        let (checkpoints, terminal_convergence) = match self
            .adopted_checkpoints
            .take()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption omitted target checkpoint authority",
            ))? {
            AdoptedCheckpointSet::Ordinary(rebased) => {
                (rebased.checkpoints, Some(rebased.terminal))
            }
            AdoptedCheckpointSet::Terminal {
                checkpoints,
                terminal,
            } => (checkpoints, Some(terminal)),
        };
        let target = M11PersistentRecursiveGreenSession {
            source: self.target,
            syntax_profile: base.syntax_profile,
            green: self.target_green.take(),
            references: Some(references),
            checkpoints,
            terminal_convergence,
            release_begun: false,
            green_release_complete: false,
            references_release_complete: false,
        };
        self.output = Some(M11PersistentRecursiveGreenUpdate {
            base: Some(base),
            target: Some(target),
            work: self.work,
            recursive_green_splice: self.recursive_green_splice.take().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green update omitted its exact event selection",
                ),
            )?,
        });
        self.phase = AdoptionPhase::Complete;
        Ok(())
    }

    fn record_structural_work(
        &mut self,
        receipt: M11BlockStructuralAdoptionReceipt,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        self.work.source_bytes_read = usize::try_from(receipt.fragment_source_bytes_read())
            .map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green crop source work overflow",
                )
            })?;
        self.work.high_level_events = receipt.high_level_events();
        self.work.green_tree_nodes_rebuilt = receipt.green().tree_nodes_visited();
        if self
            .recursive_green_splice
            .replace(receipt.green_splice_selection().clone())
            .is_some()
        {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption recorded two structural selections",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn take_update(&mut self) -> Option<M11PersistentRecursiveGreenUpdate> {
        (self.phase == AdoptionPhase::Complete)
            .then(|| self.output.take())
            .flatten()
    }

    /// Begins fuelled cancellation while preserving the exact base session.
    /// Fragment-local source/controller state owns no committed root and can
    /// be discarded synchronously; any target roots already sealed by Green
    /// or reference adoption are released explicitly below.
    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        if self.cancelling {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption cancellation already began",
            ));
        }

        if let Some(mut update) = self.output.take() {
            self.base = update.take_base();
            let mut target = update.take_target().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "completed recursive-Green adoption omitted its target",
                ),
            )?;
            target.begin_release(runtime)?;
            self.cancel_target = Some(target);
            self.cancel_green_complete = true;
            self.cancel_references_complete = true;
        } else {
            if let Some(green) = self.target_green.as_mut() {
                green
                    .begin_release(runtime)
                    .map_err(M11BlockWriterError::from)?;
                self.cancel_green_complete = false;
            } else {
                self.cancel_green_complete = true;
            }
            if let Some(references) = self.reference_replacement.as_mut() {
                references.begin_cancel(runtime)?;
                self.cancel_references_complete = false;
            } else if let Some(references) = self.reference_adoption.as_mut() {
                references.begin_cancel(runtime)?;
                self.cancel_references_complete = false;
            } else {
                self.cancel_references_complete = true;
            }
        }

        self.scanner = None;
        self.pending_line = None;
        self.active_line = None;
        self.controller = None;
        self.writer = None;
        self.target_restart = None;
        self.green_prefix = None;
        self.green_suffix = None;
        self.reference_prefix = None;
        self.reference_range_prefix = None;
        self.reference_range_pending = false;
        self.reference_range_ready = false;
        self.pending_reference_rendezvous = false;
        self.reference_rendezvous = None;
        self.reference_replacement_finishing = false;
        self.later_convergence_search = None;
        self.adopted_checkpoints = None;
        self.cancelling = true;
        Ok(())
    }

    /// Drains target-only adoption state. The base is intentionally retained
    /// and becomes available through [`Self::take_base_after_cancel`].
    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11PersistentRecursiveGreenSessionError> {
        if !self.cancelling || fuel == 0 {
            return Err(if fuel == 0 {
                M11PersistentRecursiveGreenSessionError::ZeroFuel
            } else {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green adoption cancellation has not begun",
                )
            });
        }
        if let Some(target) = self.cancel_target.as_mut() {
            if target.poll_release(runtime, fuel)? {
                self.cancel_target = None;
            }
            return Ok(self.cancel_target.is_none());
        }
        if !self.cancel_green_complete {
            self.cancel_green_complete = self
                .target_green
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green adoption cancellation lost its Green root",
                ))?
                .poll_release(runtime, fuel)
                .map_err(M11BlockWriterError::from)?
                .complete();
            if self.cancel_green_complete {
                self.target_green = None;
            }
            return Ok(self.cancel_green_complete && self.cancel_references_complete);
        }
        if !self.cancel_references_complete {
            if let Some(replacement) = self.reference_replacement.as_mut() {
                self.cancel_references_complete =
                    replacement.poll_cancel(runtime, fuel)?.complete();
                if self.cancel_references_complete {
                    self.reference_replacement = None;
                }
            } else {
                self.cancel_references_complete = self
                    .reference_adoption
                    .as_mut()
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption cancellation lost its reference actor",
                    ))?
                    .poll_cancel(runtime, fuel)?
                    .complete();
                if self.cancel_references_complete {
                    self.reference_adoption = None;
                }
            }
        }
        Ok(self.cancel_green_complete && self.cancel_references_complete)
    }

    #[must_use]
    pub fn take_base_after_cancel(&mut self) -> Option<M11PersistentRecursiveGreenSession> {
        (self.cancelling
            && self.cancel_target.is_none()
            && self.cancel_green_complete
            && self.cancel_references_complete)
            .then(|| self.base.take())
            .flatten()
    }
}

fn begin_reference_adoption(
    references: &M11ReferenceJournalRoot,
    runtime: &mut DocumentRuntime,
    prefix: Option<ExactUnchangedPrefixWitness>,
    _proof: ZeroReferenceOccurrenceProof,
) -> Result<M11ReferenceJournalUnchangedPrefixAdoption, M11PersistentRecursiveGreenSessionError> {
    Ok(references.begin_unchanged_prefix_adoption(runtime, prefix, true)?)
}

impl M11PersistentRecursiveGreenSession {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn syntax_profile(&self) -> u32 {
        self.syntax_profile
    }

    pub fn query_renderable_rows(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<M11RecursiveGreenRowWindow, M11PersistentRecursiveGreenSessionError> {
        match self.query_renderable_rows_bounded(runtime, point, requested_end_byte, limits)? {
            M11RecursiveGreenRowQueryOutcome::Window(window) => Ok(window),
            M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => Err(
                M11PersistentRecursiveGreenSessionError::Green(M11RecursiveGreenError::ZeroFuel),
            ),
        }
    }

    /// Preserves the exact exhausted budget for callers that can represent a
    /// typed row-query gap.
    pub fn query_renderable_rows_bounded(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<M11RecursiveGreenRowQueryOutcome, M11PersistentRecursiveGreenSessionError> {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green row query is not bound to the current live source",
            ));
        }
        Ok(self
            .green
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))?
            .locate_renderable_rows_bounded(runtime, point, requested_end_byte, limits)?)
    }

    pub fn locate_point(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
    ) -> Result<Option<M11RecursiveGreenLocation>, M11PersistentRecursiveGreenSessionError> {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green point query is not bound to the current live source",
            ));
        }
        Ok(self
            .green
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))?
            .locate_point(runtime, point)?)
    }

    /// Borrows the live structural root for one authority-checked parser-side
    /// publication setup.
    ///
    /// The root remains owned by this session. Callers may only retain it into
    /// another runtime-owned publication while the session still represents
    /// the current source and release has not begun.
    pub(crate) fn current_green_root<'session>(
        &'session self,
        runtime: &DocumentRuntime,
    ) -> Result<&'session M11RecursiveGreenRoot, M11PersistentRecursiveGreenSessionError> {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session is not the current live source",
            ));
        }
        self.green
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))
    }

    /// Borrows the live parser-authenticated reference root for the same
    /// failure-atomic publication setup as [`Self::current_green_root`].
    ///
    /// The session keeps its committed owner; the candidate manifest retains
    /// the canonical root and binds it to the publication's fresh authority.
    pub(crate) fn current_reference_root<'session>(
        &'session self,
        runtime: &DocumentRuntime,
    ) -> Result<&'session M11ReferenceJournalRoot, M11PersistentRecursiveGreenSessionError> {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session is not the current live source",
            ));
        }
        self.references
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its reference root",
            ))
    }

    #[must_use]
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    #[must_use]
    pub fn reference_occurrence_count(&self) -> u64 {
        self.references
            .as_ref()
            .map_or(0, M11ReferenceJournalRoot::occurrence_count)
    }

    #[cfg(test)]
    fn checkpoint_storage_receipt_for_diagnostics(&self) -> CheckpointStorageReceipt {
        let mut retained_open_frames = 0_usize;
        let mut maximum_open_depth = 0_usize;
        for checkpoint in &self.checkpoints {
            let depth = checkpoint.open_kinds().len();
            retained_open_frames = retained_open_frames.saturating_add(depth);
            maximum_open_depth = maximum_open_depth.max(depth);
        }
        CheckpointStorageReceipt {
            checkpoints: self.checkpoints.len(),
            retained_open_frames,
            maximum_open_depth,
        }
    }

    /// Full semantic digest used by clean-vs-incremental conformance gates.
    /// Product queries remain bounded point/frame operations.
    #[doc(hidden)]
    pub fn semantic_digest_for_diagnostics(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<[u8; 32], M11PersistentRecursiveGreenSessionError> {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session is not the current live source",
            ));
        }
        Ok(self
            .green
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))?
            .semantic_digest(runtime)?)
    }

    /// Opaque leaf identity used to prove distant structural page reuse.
    #[doc(hidden)]
    pub fn storage_page_identity_at_source_byte_for_diagnostics(
        &self,
        runtime: &DocumentRuntime,
        byte_offset: usize,
    ) -> Result<M11RecursiveGreenStoragePageIdentity, M11PersistentRecursiveGreenSessionError> {
        if self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session release has begun",
            ));
        }
        Ok(self
            .green
            .as_ref()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))?
            .storage_page_identity_at_source_byte(runtime, byte_offset)?)
    }

    /// Starts one source-authenticated local restart/convergence transaction.
    /// The base session is returned intact when no sparse checkpoint pair can
    /// prove the edit. Reference coverage is updated through the same bounded,
    /// failure-atomic adoption transaction.
    pub fn begin_local_adoption(
        self,
        runtime: &DocumentRuntime,
        target_lease: SourceSnapshotLease,
        base_edit: Range<usize>,
    ) -> Result<M11PersistentRecursiveGreenAdoption, M11PersistentRecursiveGreenAdoptionStartFailure>
    {
        let start = (|| {
            let target = target_lease.version();
            if runtime.current_source_version() != Some(target)
                || target == self.source
                || base_edit.start > base_edit.end
                || base_edit.end > self.source.byte_len()
            {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green local adoption crossed source authority",
                ));
            }
            let references = self.references.as_ref().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its reference authority",
                ),
            )?;
            let reference_range_required = references.occurrence_count() != 0
                && base_edit.start <= references.last_source_byte_end() as usize;

            let restart_boundary = self.checkpoints.partition_point(|checkpoint| {
                checkpoint.parser_physical().bytes() as usize <= base_edit.start
                    && checkpoint.accepted_physical().bytes() as usize <= base_edit.start
            });
            let mut restart_index = restart_boundary.checked_sub(1).ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "sparse recursive-Green index has no restart before the edit",
                ),
            )?;
            // A clean EOF capture for an unterminated final line is a valid
            // base checkpoint, but an append extends that same physical line.
            // Walk back until the parser cut remains a line boundary in the
            // exact target source instead of parsing the appended suffix as a
            // synthetic next line.
            while {
                let checkpoint = &self.checkpoints[restart_index];
                let target_line_start = target_lease
                    .is_physical_line_start(checkpoint.parser_physical().bytes() as usize)
                    .map_err(|_| {
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green restart cut is not a target source boundary",
                        )
                    })?;
                !target_line_start
                    || !checkpoint_proves_reference_occurrence_cut(checkpoint, references)
            } {
                restart_index = restart_index.checked_sub(1).ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "sparse recursive-Green index has no reference-safe target restart",
                    ),
                )?;
            }
            let convergence_search_start = restart_index + 1;
            let convergence_offset =
                self.checkpoints[convergence_search_start..].partition_point(|checkpoint| {
                    (checkpoint.parser_physical().bytes() as usize) < base_edit.end
                        || (checkpoint.accepted_physical().bytes() as usize) < base_edit.end
                });
            let convergence_index = convergence_search_start
                .checked_add(convergence_offset)
                .filter(|index| *index < self.checkpoints.len());
            if convergence_index.is_none() && self.terminal_convergence.is_none() {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "sparse recursive-Green index has no convergence after the edit",
                ));
            }

            let selected_terminal = convergence_index.is_none_or(|index| {
                let checkpoint = &self.checkpoints[index];
                checkpoint.parser_physical().bytes() as usize == self.source.byte_len()
                    && checkpoint.parser_physical().utf16() as usize == self.source.utf16_len()
            });
            if self.terminal_convergence.is_none() {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its terminal convergence authority",
                ));
            }
            let restart = self.checkpoints.get(restart_index).ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "restart checkpoint index escaped the base",
                ),
            )?;
            let parser_restart = restart.parser_physical();
            let green_restart = restart.accepted_physical();
            let (parser_convergence, green_convergence, convergence) =
                if let Some(convergence_index) = convergence_index {
                    let convergence_checkpoint = self.checkpoints.get(convergence_index).ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "convergence checkpoint index escaped the base",
                        ),
                    )?;
                    let parser_convergence = convergence_checkpoint.parser_physical();
                    let green_convergence = convergence_checkpoint.accepted_physical();
                    let convergence = if selected_terminal {
                        AdoptionConvergence::Terminal
                    } else {
                        AdoptionConvergence::Ordinary {
                            checkpoint_index: convergence_index,
                        }
                    };
                    (parser_convergence, green_convergence, convergence)
                } else {
                    let eof = SourceMetric::new(
                        u64::try_from(self.source.byte_len()).map_err(|_| {
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green source bytes exceed u64",
                            )
                        })?,
                        u64::try_from(self.source.utf16_len()).map_err(|_| {
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green source UTF-16 exceeds u64",
                            )
                        })?,
                    )
                    .ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green terminal source metric is invalid",
                        ),
                    )?;
                    (eof, eof, AdoptionConvergence::Terminal)
                };
            let next_line_ordinal = u32::try_from(restart.next_line_ordinal()).map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green restart line ordinal exceeds u32",
                )
            })?;
            let transaction_id = M11BlockRestartCheckpoint::allocate_adoption_transaction_id()?;
            let restart = restart.replicate_for_transaction(transaction_id)?;

            let document_start = parser_restart == SourceMetric::default()
                && green_restart == SourceMetric::default();
            let parser_prefix = if document_start {
                None
            } else {
                Some(runtime.mint_exact_unchanged_prefix_witness(
                    self.source,
                    parser_restart.bytes() as usize,
                    parser_restart.utf16() as usize,
                )?)
            };
            let reference_range_prefix = if document_start {
                None
            } else {
                Some(runtime.mint_exact_unchanged_prefix_witness(
                    self.source,
                    parser_restart.bytes() as usize,
                    parser_restart.utf16() as usize,
                )?)
            };
            let reference_range_base_start = parser_restart;
            let green_prefix = if document_start {
                None
            } else {
                Some(runtime.mint_exact_unchanged_prefix_witness(
                    self.source,
                    green_restart.bytes() as usize,
                    green_restart.utf16() as usize,
                )?)
            };
            let green_suffix = if green_convergence.bytes() as usize == self.source.byte_len()
                && green_convergence.utf16() as usize == self.source.utf16_len()
            {
                None
            } else {
                Some(runtime.mint_exact_unchanged_suffix_witness(
                    self.source,
                    green_convergence.bytes() as usize,
                    green_convergence.utf16() as usize,
                )?)
            };
            let target_parser_end = if parser_convergence.bytes() as usize == self.source.byte_len()
                && parser_convergence.utf16() as usize == self.source.utf16_len()
            {
                target.byte_len()
            } else {
                runtime
                    .mint_exact_unchanged_suffix_witness(
                        self.source,
                        parser_convergence.bytes() as usize,
                        parser_convergence.utf16() as usize,
                    )?
                    .target_byte_start()
            };
            let reference_prefix = if references.occurrence_count() == 0
                || reference_range_required
            {
                None
            } else {
                Some(runtime.mint_exact_unchanged_prefix_witness(
                    self.source,
                    references.last_source_byte_end() as usize,
                    references.last_source_utf16_end() as usize,
                )?)
            };

            let scanner_lease = runtime.snapshot_current_source()?;
            let target_parser_start = parser_prefix
                .as_ref()
                .map_or(0, ExactUnchangedPrefixWitness::byte_end);
            let green = self.green.as_ref().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ),
            )?;
            let joined = match parser_prefix {
                Some(parser_prefix) => {
                    restart.resume(transaction_id, runtime, green, target_lease, parser_prefix)?
                }
                None => restart.resume_at_document_start(
                    transaction_id,
                    runtime,
                    green,
                    target_lease,
                )?,
            };
            let (controller, writer) = joined.into_local_fragment()?;
            let parser_restart = if document_start {
                controller.capture_document_start_restart()?
            } else {
                controller.capture_restart()?
            };
            let target_restart = writer
                .capture_restart_checkpoint(parser_restart)
                .map_err(M11PersistentRecursiveGreenSessionError::Restart)?;
            let scanner =
                SnapshotLineScanner::new_at(scanner_lease, target_parser_start, next_line_ordinal)?;
            Ok(M11PersistentRecursiveGreenAdoption {
                base: None,
                target,
                phase: AdoptionPhase::ParseFragment,
                scanner: Some(scanner),
                pending_line: None,
                active_line: None,
                current_line_end: None,
                target_parser_end,
                controller: Some(controller),
                writer: Some(writer),
                writer_command_pending: false,
                target_restart: Some(target_restart),
                checkpoint_selection: AdoptionCheckpointSelection {
                    transaction_id,
                    restart_index,
                    convergence,
                },
                later_convergence_search: None,
                green_prefix,
                document_start,
                green_suffix,
                reference_prefix,
                reference_range_prefix,
                reference_range_base_start,
                reference_range_pending: reference_range_required,
                reference_range_ready: false,
                pending_reference_rendezvous: false,
                reference_rendezvous: None,
                reference_replacement: None,
                reference_replacement_finishing: false,
                reference_adoption: None,
                target_green: None,
                adopted_checkpoints: None,
                recursive_green_splice: None,
                output: None,
                work: M11PersistentRecursiveGreenAdoptionWork::default(),
                cancelling: false,
                cancel_target: None,
                cancel_green_complete: false,
                cancel_references_complete: false,
            })
        })();

        match start {
            Ok(mut adoption) => {
                adoption.base = Some(self);
                Ok(adoption)
            }
            Err(error) => {
                Err(M11PersistentRecursiveGreenAdoptionStartFailure { error, base: self })
            }
        }
    }

    pub fn prepare_inline_leaf(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
    ) -> Result<M11RecursiveGreenInlineLeafPreparation, M11PersistentRecursiveGreenSessionError>
    {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session is not the current live source",
            ));
        }
        let limits = M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 512).ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green query limits must be nonzero",
            ),
        )?;
        let fence = resolve_m11_recursive_green_inline_leaf_row_fence(
            runtime,
            self.green
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ))?,
            point,
            limits,
            8192,
        )?
        .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
            "recursive-Green point is not owned by a final inline-bearing leaf",
        ))?;
        let block_source = to_u32_range(fence.block_source_range())?;
        let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
        let inline_source = to_u32_range(fence.inline_source_range())?;
        let inline_source_utf16 = to_u32_range(fence.inline_source_utf16_range())?;
        let query_receipt = fence.receipt();
        Ok(
            M11RecursiveGreenInlineLeafPreparation::from_persistent_session(
                block_source,
                block_source_utf16,
                inline_source,
                inline_source_utf16,
                query_receipt,
                fence,
            ),
        )
    }

    pub fn prepare_paragraph_inline(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
    ) -> Result<M11RecursiveGreenParagraphInlinePreparation, M11PersistentRecursiveGreenSessionError>
    {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session is not the current live source",
            ));
        }
        let limits = M11RecursiveGreenFrameQueryLimits::new(64, 8192, 64, 8192).ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green query limits must be nonzero",
            ),
        )?;
        let fence = resolve_m11_recursive_green_paragraph_fence(
            runtime,
            self.green
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ))?,
            point,
            limits,
        )?
        .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
            "recursive-Green point is not owned by a final Paragraph",
        ))?;
        let block_source = to_u32_range(fence.block_source_range())?;
        let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
        let inline_source = to_u32_range(fence.inline_source_range())?;
        let inline_source_utf16 = to_u32_range(fence.inline_source_utf16_range())?;
        let query_receipt = fence.receipt();
        Ok(
            M11RecursiveGreenParagraphInlinePreparation::from_persistent_session(
                block_source,
                block_source_utf16,
                inline_source,
                inline_source_utf16,
                query_receipt,
                fence,
            ),
        )
    }

    /// Authenticates one top-level single-Paragraph block quote for exact
    /// marker projection. Unlike inline-leaf preparation, this authority spans
    /// the physically disjoint container and carries Green-derived metrics.
    pub fn prepare_block_quote_projection(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
    ) -> Result<
        Option<M11RecursiveGreenBlockQuoteProjectionPreparation>,
        M11PersistentRecursiveGreenSessionError,
    > {
        if runtime.current_source_version() != Some(self.source) || self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session is not the current live source",
            ));
        }
        let limits = M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 512).ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green block-quote query limits must be nonzero",
            ),
        )?;
        let fence = match resolve_m11_recursive_green_block_quote_projection_fence(
            runtime,
            self.green
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ))?,
            point,
            limits,
            u64::try_from(BLOCK_QUOTE_WINDOW_MAX_BYTES).map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "block-quote window cap exceeds recursive-Green metrics",
                )
            })?,
        ) {
            Ok(Some(fence)) => fence,
            Ok(None) | Err(M11RecursiveGreenFrameQueryError::BoundExceeded(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let block_source = to_u32_range(fence.block_source_range())?;
        let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
        let query_receipt = fence.receipt();
        Ok(Some(
            M11RecursiveGreenBlockQuoteProjectionPreparation::from_persistent_session(
                block_source,
                block_source_utf16,
                query_receipt,
                fence,
            ),
        ))
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        if self.release_begun {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session release already began",
            ));
        }
        self.green
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))?
            .begin_release(runtime)
            .map_err(M11BlockWriterError::from)?;
        self.references
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its reference root",
            ))?
            .begin_release(runtime)?;
        self.release_begun = true;
        Ok(())
    }

    pub fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, M11PersistentRecursiveGreenSessionError> {
        if !self.release_begun || fuel == 0 {
            return Err(if fuel == 0 {
                M11PersistentRecursiveGreenSessionError::ZeroFuel
            } else {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session release has not begun",
                )
            });
        }
        let mut remaining = fuel;
        if !self.green_release_complete && remaining > 0 {
            self.green_release_complete = self
                .green
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ))?
                .poll_release(runtime, remaining)
                .map_err(M11BlockWriterError::from)?
                .complete();
            remaining = 0;
        }
        if !self.references_release_complete && remaining > 0 {
            self.references_release_complete = self
                .references
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its reference root",
                ))?
                .poll_release(runtime, remaining)?
                .complete();
        }
        Ok(self.green_release_complete && self.references_release_complete)
    }
}

fn to_u32_range(range: Range<u64>) -> Result<Range<u32>, M11PersistentRecursiveGreenSessionError> {
    Ok(u32::try_from(range.start).map_err(|_| {
        M11PersistentRecursiveGreenSessionError::InvalidState(
            "recursive-Green range exceeds the candidate ABI",
        )
    })?..u32::try_from(range.end).map_err(|_| {
        M11PersistentRecursiveGreenSessionError::InvalidState(
            "recursive-Green range exceeds the candidate ABI",
        )
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flark_engine::DocumentRuntimeConfig;

    fn build_session(runtime: &mut DocumentRuntime) -> M11PersistentRecursiveGreenSession {
        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime.snapshot_current_source().expect("scanner lease"),
            runtime.snapshot_current_source().expect("writer lease"),
            1,
        )
        .expect("clean plan");
        let mut build = plan.begin(runtime).expect("clean build");
        loop {
            let poll = build.poll(runtime, 64).expect("poll clean build");
            if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                return build.take_session().expect("persistent session");
            }
        }
    }

    fn finish_local_adoption(
        runtime: &mut DocumentRuntime,
        session: M11PersistentRecursiveGreenSession,
        base_edit: Range<usize>,
    ) -> M11PersistentRecursiveGreenUpdate {
        let target = runtime.snapshot_current_source().expect("target lease");
        let mut adoption = session
            .begin_local_adoption(runtime, target, base_edit)
            .unwrap_or_else(|failure| panic!("local adoption start: {}", failure.error()));
        loop {
            match adoption
                .poll(runtime, 64)
                .expect("poll local adoption")
                .status()
            {
                M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                    return adoption.take_update().expect("completed local update");
                }
                M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                    panic!("reference mutation required clean fallback")
                }
                M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                    panic!("reference mutation was cancelled")
                }
            }
        }
    }

    fn winner(
        runtime: &DocumentRuntime,
        session: &M11PersistentRecursiveGreenSession,
        label: &[u8],
    ) -> Option<u64> {
        session
            .references
            .as_ref()
            .expect("session reference root")
            .winner_ordinal(runtime, label)
            .expect("reference winner query")
    }

    fn close_runtime(runtime: &mut DocumentRuntime) {
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
    }

    fn release_session(
        runtime: &mut DocumentRuntime,
        session: &mut M11PersistentRecursiveGreenSession,
    ) {
        session.begin_release(runtime).expect("begin session release");
        while !session.poll_release(runtime, 64).expect("poll session release") {}
    }

    #[test]
    fn dynamic_reference_replacement_keeps_the_prefix_before_a_remainder_restart() {
        const BASE: &str = "[base]: /base\n!x]: /new\nsee [x]\n";
        let edit_start = BASE.find("!x]").expect("new definition marker");

        let mut runtime = DocumentRuntime::new(BASE, DocumentRuntimeConfig::default())
            .expect("prefix-boundary runtime");
        let session = build_session(&mut runtime);
        let base_source = session.source();
        runtime
            .apply_edit(base_source, edit_start..edit_start + 1, "[")
            .expect("activate second definition");
        let mut update = finish_local_adoption(
            &mut runtime,
            session,
            edit_start..edit_start + 1,
        );
        let mut base = update.take_base().expect("superseded prefix base");
        let mut target = update.take_target().expect("prefix-safe target");

        assert_eq!(target.reference_occurrence_count(), 2);
        assert_eq!(winner(&runtime, &target, b"base"), Some(0));
        assert_eq!(winner(&runtime, &target, b"x"), Some(1));

        release_session(&mut runtime, &mut base);
        release_session(&mut runtime, &mut target);
        close_runtime(&mut runtime);
    }

    #[test]
    fn local_replacement_advances_past_a_multiline_reference_occurrence() {
        let destination = "a".repeat(CHECKPOINT_STRIDE_BYTES as usize + 256);
        let base = format!("before\n\n[x]: /{destination}\n  \"title\"\n\nafter [x]\n");
        let definition_start = base.find("[x]:").expect("definition start");
        let title_start = base.find("  \"title\"").expect("title start");

        let mut runtime = DocumentRuntime::new(&base, DocumentRuntimeConfig::default())
            .expect("multiline reference runtime");
        let session = build_session(&mut runtime);
        let references = session.references.as_ref().expect("base reference root");
        assert!(session.checkpoints.iter().any(|checkpoint| {
            let cut = checkpoint.parser_physical().bytes() as usize;
            definition_start < cut
                && cut <= title_start
                && !checkpoint_proves_reference_occurrence_cut(checkpoint, references)
        }));

        let base_source = session.source();
        runtime
            .apply_edit(base_source, 0..1, "B")
            .expect("edit before multiline definition");
        let mut update = finish_local_adoption(&mut runtime, session, 0..1);
        let mut superseded = update.take_base().expect("superseded multiline base");
        let mut target = update.take_target().expect("multiline-safe target");

        assert_eq!(target.reference_occurrence_count(), 1);
        assert_eq!(winner(&runtime, &target, b"x"), Some(0));

        release_session(&mut runtime, &mut superseded);
        release_session(&mut runtime, &mut target);
        close_runtime(&mut runtime);
    }

    #[test]
    fn reference_paragraph_before_restart_requests_and_drains_clean_fallback() {
        let whitespace = " ".repeat(CHECKPOINT_STRIDE_BYTES as usize + 256);
        let base = format!("[x]:{whitespace}\n!u\n\nafter\n");
        let edit_start = base.find("!u").expect("destination marker");

        let mut runtime = DocumentRuntime::new(&base, DocumentRuntimeConfig::default())
            .expect("predating Paragraph runtime");
        let session = build_session(&mut runtime);
        assert_eq!(session.reference_occurrence_count(), 0);
        let base_source = session.source();
        runtime
            .apply_edit(base_source, edit_start..edit_start + 1, "/")
            .expect("activate multiline definition");
        let target = runtime.snapshot_current_source().expect("target lease");
        let mut adoption = session
            .begin_local_adoption(&runtime, target, edit_start..edit_start + 1)
            .unwrap_or_else(|failure| panic!("local adoption start: {}", failure.error()));

        loop {
            match adoption.poll(&mut runtime, 64).expect("poll fallback").status() {
                M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => break,
                status => panic!("expected clean fallback, got {status:?}"),
            }
        }
        adoption.begin_cancel(&mut runtime).expect("begin fallback cancellation");
        while !adoption
            .poll_cancel(&mut runtime, 64)
            .expect("poll fallback cancellation")
        {}
        let mut base = adoption
            .take_base_after_cancel()
            .expect("fallback preserves exact base");
        assert_eq!(base.reference_occurrence_count(), 0);
        release_session(&mut runtime, &mut base);
        close_runtime(&mut runtime);
    }

    #[test]
    fn local_definition_rename_rebuilds_first_winners_and_replays_suffix() {
        const BASE: &str = "[a]: /first\n[a]: /second\n\nbody [a] [b]\n";
        const TARGET: &str = "[b]: /first\n[a]: /second\n\nbody [a] [b]\n";

        let mut runtime = DocumentRuntime::new(BASE, DocumentRuntimeConfig::default())
            .expect("reference rename runtime");
        let session = build_session(&mut runtime);
        let base_source = session.source();
        runtime
            .apply_edit(base_source, 1..2, "b")
            .expect("rename first definition");
        let mut update = finish_local_adoption(&mut runtime, session, 1..2);
        let mut base = update.take_base().expect("superseded base");
        let mut target = update.take_target().expect("incremental target");

        assert_eq!(target.reference_occurrence_count(), 2);
        assert_eq!(winner(&runtime, &target, b"b"), Some(0));
        assert_eq!(winner(&runtime, &target, b"a"), Some(1));

        let mut clean_runtime = DocumentRuntime::new(TARGET, DocumentRuntimeConfig::default())
            .expect("clean reference rename runtime");
        let mut clean = build_session(&mut clean_runtime);
        assert_eq!(target.reference_occurrence_count(), clean.reference_occurrence_count());
        assert_eq!(winner(&runtime, &target, b"a"), winner(&clean_runtime, &clean, b"a"));
        assert_eq!(winner(&runtime, &target, b"b"), winner(&clean_runtime, &clean, b"b"));

        base.begin_release(&mut runtime).expect("release base");
        while !base.poll_release(&mut runtime, 64).expect("poll base release") {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        clean.begin_release(&mut clean_runtime).expect("release clean");
        while !clean
            .poll_release(&mut clean_runtime, 64)
            .expect("poll clean release")
        {}
        close_runtime(&mut runtime);
        close_runtime(&mut clean_runtime);
    }

    #[test]
    fn local_definition_deletion_promotes_later_duplicate() {
        const BASE: &str = "[a]: /first\n\n[a]: /second\n\nbody [a]\n";
        const TARGET: &str = "ordinary text\n\n[a]: /second\n\nbody [a]\n";
        const FIRST_DEFINITION_END: usize = "[a]: /first".len();

        let mut runtime = DocumentRuntime::new(BASE, DocumentRuntimeConfig::default())
            .expect("reference deletion runtime");
        let session = build_session(&mut runtime);
        let base_source = session.source();
        runtime
            .apply_edit(base_source, 0..FIRST_DEFINITION_END, "ordinary text")
            .expect("remove first definition");
        let mut update = finish_local_adoption(
            &mut runtime,
            session,
            0..FIRST_DEFINITION_END,
        );
        let mut base = update.take_base().expect("superseded base");
        let mut target = update.take_target().expect("incremental target");

        assert_eq!(target.reference_occurrence_count(), 1);
        assert_eq!(winner(&runtime, &target, b"a"), Some(0));

        let mut clean_runtime = DocumentRuntime::new(TARGET, DocumentRuntimeConfig::default())
            .expect("clean reference deletion runtime");
        let mut clean = build_session(&mut clean_runtime);
        assert_eq!(target.reference_occurrence_count(), clean.reference_occurrence_count());
        assert_eq!(winner(&runtime, &target, b"a"), winner(&clean_runtime, &clean, b"a"));

        base.begin_release(&mut runtime).expect("release base");
        while !base.poll_release(&mut runtime, 64).expect("poll base release") {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        clean.begin_release(&mut clean_runtime).expect("release clean");
        while !clean
            .poll_release(&mut clean_runtime, 64)
            .expect("poll clean release")
        {}
        close_runtime(&mut runtime);
        close_runtime(&mut clean_runtime);
    }

    #[test]
    fn local_consumer_edit_replays_a_later_definition_without_parsing_to_it() {
        let prefix = (0..384)
            .map(|index| format!("prefix paragraph {index:04}\n\n"))
            .collect::<String>();
        let suffix = (0..384)
            .map(|index| format!("suffix paragraph {index:04}\n\n"))
            .collect::<String>();
        let base = format!("{prefix}body [a]\n\n{suffix}[a]: /winner\n");
        let consumer = base.find("body [a]").expect("consumer Paragraph");
        let edit = consumer + "body [".len()..consumer + "body [a".len();

        let mut runtime = DocumentRuntime::new(&base, DocumentRuntimeConfig::default())
            .expect("later-definition runtime");
        let session = build_session(&mut runtime);
        let base_source = session.source();
        runtime
            .apply_edit(base_source, edit.clone(), "b")
            .expect("edit reference consumer");
        let mut update = finish_local_adoption(&mut runtime, session, edit);

        assert!(
            update.work().source_bytes_read() < suffix.len(),
            "the local parser should converge before the distant definition"
        );
        let mut base_session = update.take_base().expect("superseded base");
        let mut target = update.take_target().expect("incremental target");
        assert_eq!(target.reference_occurrence_count(), 1);
        assert_eq!(winner(&runtime, &target, b"a"), Some(0));
        assert_eq!(winner(&runtime, &target, b"b"), None);

        base_session
            .begin_release(&mut runtime)
            .expect("release base");
        while !base_session
            .poll_release(&mut runtime, 64)
            .expect("poll base release")
        {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        close_runtime(&mut runtime);
    }

    #[test]
    fn local_paragraph_to_definition_starts_reference_replacement_on_demand() {
        const BASE: &str = "xa]: /new\n\nbody [a]\n";
        const TARGET: &str = "[a]: /new\n\nbody [a]\n";

        let mut runtime = DocumentRuntime::new(BASE, DocumentRuntimeConfig::default())
            .expect("new-definition runtime");
        let session = build_session(&mut runtime);
        assert_eq!(session.reference_occurrence_count(), 0);
        let base_source = session.source();
        runtime
            .apply_edit(base_source, 0..1, "[")
            .expect("promote Paragraph to definition");
        let mut update = finish_local_adoption(&mut runtime, session, 0..1);
        let mut base = update.take_base().expect("superseded base");
        let mut target = update.take_target().expect("incremental target");

        assert_eq!(target.reference_occurrence_count(), 1);
        assert_eq!(winner(&runtime, &target, b"a"), Some(0));

        let mut clean_runtime = DocumentRuntime::new(TARGET, DocumentRuntimeConfig::default())
            .expect("clean new-definition runtime");
        let mut clean = build_session(&mut clean_runtime);
        assert_eq!(target.reference_occurrence_count(), clean.reference_occurrence_count());
        assert_eq!(winner(&runtime, &target, b"a"), winner(&clean_runtime, &clean, b"a"));

        base.begin_release(&mut runtime).expect("release base");
        while !base.poll_release(&mut runtime, 64).expect("poll base release") {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        clean.begin_release(&mut clean_runtime).expect("release clean");
        while !clean
            .poll_release(&mut clean_runtime, 64)
            .expect("poll clean release")
        {}
        close_runtime(&mut runtime);
        close_runtime(&mut clean_runtime);
    }

    #[test]
    fn checkpoint_storage_scales_with_bytes_in_a_deep_open_container() {
        const DEPTH: usize = 24;
        const LINES: usize = 4_096;

        let marker = "> ".repeat(DEPTH);
        let mut source = String::new();
        for ordinal in 0..LINES {
            source.push_str(&marker);
            source.push_str(&format!(
                "deep quoted line {ordinal:04} remains in one paragraph for bounded restart storage.\n"
            ));
        }
        source.push('\n');

        let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("deep quote runtime");
        let mut session = build_session(&mut runtime);
        let receipt = session.checkpoint_storage_receipt_for_diagnostics();
        assert!(
            receipt.maximum_open_depth() >= DEPTH + 2,
            "receipt={receipt:?}",
        );

        // A fixed 4 KiB cadence would retain every open frame at every cut.
        // Scaling the cadence by open depth keeps aggregate retained frames
        // linear in source bytes even for deeply nested parser recipes.
        let source_quanta = source.len().div_ceil(CHECKPOINT_STRIDE_BYTES as usize);
        assert!(
            receipt.retained_open_frames() <= source_quanta + 3 * receipt.maximum_open_depth(),
            "source_quanta={source_quanta} receipt={receipt:?}",
        );
        assert!(
            receipt.checkpoints() * DEPTH < source_quanta + 3 * DEPTH,
            "source_quanta={source_quanta} receipt={receipt:?}",
        );

        session.begin_release(&mut runtime).expect("begin release");
        while !session
            .poll_release(&mut runtime, 64)
            .expect("poll release")
        {}
        runtime.begin_close().expect("begin deep quote close");
        while !runtime
            .poll_close(64)
            .expect("poll deep quote close")
            .complete
        {}
    }
}

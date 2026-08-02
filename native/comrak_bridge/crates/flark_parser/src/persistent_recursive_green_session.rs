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
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowWindow,
    M11RecursiveGreenStoragePageIdentity,
    M11RecursiveGreenStructuralSpliceSelection, M11ReferenceJournal,
    M11ReferenceJournalAdoptionStatus, M11ReferenceJournalError, M11ReferenceJournalRoot,
    M11ReferenceJournalStatus, M11ReferenceJournalUnchangedPrefixAdoption,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeError, ExactUnchangedPrefixWitness,
    ExactUnchangedSuffixWitness, SourceSnapshotLease, SourceVersion,
};

use crate::block_core::{
    resolve_m11_recursive_green_inline_leaf_fence,
    resolve_m11_recursive_green_paragraph_fence, BlockCommand, BlockKind,
    M11BlockRestartCheckpoint, M11BlockRestartError, M11BlockStructuralAdoptionReceipt,
    M11BlockTerminalConvergenceCheckpoint, M11BlockWriter, M11BlockWriterError,
    M11BlockWriterOfferStatus, M11BlockWriterPollStatus, M11DirectBlockController,
    M11DirectBlockControllerError, M11DirectBlockError, M11DirectBlockPollStatus,
    M11DirectSourceLineAdmission, M11ReferenceRendezvous, M11ReferenceRendezvousError,
    M11ReferenceRendezvousStatus, SourceMetric,
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
        let is_spaced_top_level = checkpoint.open_kinds().count() == 1
            && cut.saturating_sub(previous_cut) >= CHECKPOINT_STRIDE_BYTES;
        if is_distinct && (force || is_spaced_top_level) {
            self.checkpoints.try_reserve(1).map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green checkpoint allocation failed",
                )
            })?;
            self.checkpoints.push(checkpoint);
        }
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
/// and exact Green event selection without accepting a caller-supplied reuse
/// flag.
pub(crate) struct M11PersistentRecursiveGreenExactPublication<'update> {
    base: &'update M11PersistentRecursiveGreenSession,
    target: &'update M11PersistentRecursiveGreenSession,
    recursive_green_splice: M11RecursiveGreenStructuralSpliceSelection,
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
    ) -> M11RecursiveGreenStructuralSpliceSelection {
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

    /// Exact semantic Green ranges removed from the base and inserted in the
    /// target. These survive independently of aggregate adoption work so an
    /// exact-base publisher never has to infer a splice from event counts.
    #[must_use]
    pub const fn recursive_green_splice_selection(
        &self,
    ) -> M11RecursiveGreenStructuralSpliceSelection {
        self.recursive_green_splice
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
            recursive_green_splice: self.recursive_green_splice,
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
    BeginTerminalFinish,
    FinishTerminal,
    AdoptGreen,
    AdoptReferences,
    Complete,
    CleanFallbackRequired,
}

enum AdoptionConvergence {
    Ordinary(M11BlockRestartCheckpoint),
    Terminal(M11BlockTerminalConvergenceCheckpoint),
}

enum AdoptedConvergence {
    Ordinary(M11BlockRestartCheckpoint),
    Terminal(M11BlockTerminalConvergenceCheckpoint),
}

/// Fuelled same-island restart/convergence adoption. Any reference work in
/// the replacement crop fails closed to a clean composite build.
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
    convergence: Option<AdoptionConvergence>,
    green_prefix: Option<ExactUnchangedPrefixWitness>,
    green_suffix: Option<ExactUnchangedSuffixWitness>,
    reference_prefix: Option<ExactUnchangedPrefixWitness>,
    reference_adoption: Option<M11ReferenceJournalUnchangedPrefixAdoption>,
    target_green: Option<M11RecursiveGreenRoot>,
    adopted_convergence: Option<AdoptedConvergence>,
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
        if let Some(adoption) = self.reference_adoption.as_mut() {
            let poll = adoption.poll(runtime, 1)?;
            self.work.reference_rebind_transitions = self
                .work
                .reference_rebind_transitions
                .checked_add(poll.transitions())
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "reference adoption work overflow",
                ))?;
            if poll.status() == M11ReferenceJournalAdoptionStatus::Complete {
                let references = adoption.take_root().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "completed reference adoption omitted its root",
                    ),
                )?;
                self.reference_adoption = None;
                let base = self.base.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted its base session",
                    ),
                )?;
                let restart = self.target_restart.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted its target restart",
                    ),
                )?;
                let (checkpoints, terminal_convergence) = match self
                    .adopted_convergence
                    .take()
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted target convergence",
                    ))? {
                    AdoptedConvergence::Ordinary(convergence) => (vec![restart, convergence], None),
                    AdoptedConvergence::Terminal(terminal) => (vec![restart], Some(terminal)),
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
                            self.phase = AdoptionPhase::CleanFallbackRequired;
                        }
                        M11DirectBlockPollStatus::Complete => {
                            let end = self.current_line_end.take().ok_or(
                                M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "recursive-Green crop lost its line end",
                                ),
                            )?;
                            if end == self.target_parser_end {
                                self.phase = if matches!(
                                    self.convergence.as_ref(),
                                    Some(AdoptionConvergence::Terminal(_))
                                ) {
                                    AdoptionPhase::BeginTerminalFinish
                                } else {
                                    AdoptionPhase::AdoptGreen
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
                let (poll, _) = scanner.poll_counted_retaining_complete(SOURCE_WORK_QUANTUM)?;
                match poll {
                    SnapshotLineRetainedPoll::Pending(scanner) => self.scanner = Some(scanner),
                    SnapshotLineRetainedPoll::Line(line) => self.pending_line = Some(line),
                    SnapshotLineRetainedPoll::Complete(scanner) => {
                        drop(scanner.into_source_lease());
                        if matches!(
                            self.convergence.as_ref(),
                            Some(AdoptionConvergence::Terminal(_))
                        ) && self.target_parser_end == self.target.byte_len()
                        {
                            self.phase = AdoptionPhase::BeginTerminalFinish;
                        } else {
                            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                "recursive-Green crop reached EOF before convergence",
                            ));
                        }
                    }
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
                            self.phase = AdoptionPhase::AdoptGreen;
                        } else {
                            self.offer_pending_command()?;
                        }
                    }
                    M11DirectBlockPollStatus::ExternalWorkReady => {
                        self.phase = AdoptionPhase::CleanFallbackRequired;
                    }
                    M11DirectBlockPollStatus::Complete => {
                        return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "terminal convergence passed Close(Document)",
                        ));
                    }
                }
            }
            AdoptionPhase::AdoptGreen => {
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
                let convergence = self.convergence.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted base convergence",
                    ),
                )?;
                let green_prefix = self.green_prefix.take().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green adoption omitted prefix lineage",
                    ),
                )?;
                let green_suffix = self.green_suffix.take();
                let reference_prefix = self.reference_prefix.take();
                let parser = if matches!(&convergence, AdoptionConvergence::Ordinary(_)) {
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
                    match convergence {
                        AdoptionConvergence::Ordinary(old_convergence) => writer
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
                            )
                            .map(|(green, receipt, restart, convergence)| {
                                (
                                    green,
                                    receipt,
                                    restart,
                                    AdoptedConvergence::Ordinary(convergence),
                                )
                            }),
                        AdoptionConvergence::Terminal(old_terminal) => writer
                            .adopt_converged_terminal_fragment(
                                target_restart,
                                old_terminal,
                                runtime,
                                green_base,
                                green_prefix,
                            )
                            .map(|(green, receipt, restart, terminal)| {
                                (
                                    green,
                                    receipt,
                                    restart,
                                    AdoptedConvergence::Terminal(terminal),
                                )
                            }),
                    }
                };
                let (green, receipt, restart, convergence) = match adoption_result {
                    Ok(result) => result,
                    Err(
                        M11BlockRestartError::Pairing(
                            "target fragment did not converge to its exact base boundary",
                        )
                        | M11BlockRestartError::Pairing(
                            "target tail did not converge at the pre-Document-close boundary",
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
                self.target_restart = Some(restart);
                self.adopted_convergence = Some(convergence);
                self.controller = None;
                self.record_structural_work(receipt)?;
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
            .replace(receipt.green_splice_selection())
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
            if let Some(references) = self.reference_adoption.as_mut() {
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
        self.convergence = None;
        self.green_prefix = None;
        self.green_suffix = None;
        self.reference_prefix = None;
        self.adopted_convergence = None;
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
            .locate_renderable_rows(runtime, point, requested_end_byte, limits)?)
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
    /// prove the edit, or when reference coverage intersects the edit.
    pub fn begin_local_adoption(
        mut self,
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
            if references.occurrence_count() != 0
                && base_edit.start <= references.last_source_byte_end() as usize
            {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "edit intersects or precedes committed reference coverage",
                ));
            }

            let restart_boundary = self.checkpoints.partition_point(|checkpoint| {
                checkpoint.parser_physical().bytes() as usize <= base_edit.start
                    && checkpoint.accepted_physical().bytes() as usize <= base_edit.start
            });
            let restart_index = restart_boundary.checked_sub(1).ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "sparse recursive-Green index has no restart before the edit",
                ),
            )?;
            let convergence_search_start = restart_index + 1;
            let convergence_offset = self.checkpoints[convergence_search_start..]
                .partition_point(|checkpoint| {
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

            let convergence_checkpoint =
                convergence_index.map(|index| self.checkpoints.remove(index));
            let restart = self.checkpoints.remove(restart_index);
            let parser_restart = restart.parser_physical();
            let green_restart = restart.accepted_physical();
            let (parser_convergence, green_convergence, convergence) =
                if let Some(convergence_checkpoint) = convergence_checkpoint {
                    let parser_convergence = convergence_checkpoint.parser_physical();
                    let green_convergence = convergence_checkpoint.accepted_physical();
                    let terminal_convergence = parser_convergence.bytes() as usize
                        == self.source.byte_len()
                        && parser_convergence.utf16() as usize == self.source.utf16_len();
                    let convergence = if terminal_convergence {
                        AdoptionConvergence::Terminal(self.terminal_convergence.take().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green session omitted its terminal convergence authority",
                        ),
                    )?)
                    } else {
                        AdoptionConvergence::Ordinary(convergence_checkpoint)
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
                    let terminal =
                        self.terminal_convergence
                            .take()
                            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green session omitted its terminal convergence authority",
                        ))?;
                    (eof, eof, AdoptionConvergence::Terminal(terminal))
                };
            let next_line_ordinal = u32::try_from(restart.next_line_ordinal()).map_err(|_| {
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green restart line ordinal exceeds u32",
                )
            })?;

            let parser_prefix = runtime.mint_exact_unchanged_prefix_witness(
                self.source,
                parser_restart.bytes() as usize,
                parser_restart.utf16() as usize,
            )?;
            let green_prefix = runtime.mint_exact_unchanged_prefix_witness(
                self.source,
                green_restart.bytes() as usize,
                green_restart.utf16() as usize,
            )?;
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
            let reference_prefix = if references.occurrence_count() == 0 {
                None
            } else {
                Some(runtime.mint_exact_unchanged_prefix_witness(
                    self.source,
                    references.last_source_byte_end() as usize,
                    references.last_source_utf16_end() as usize,
                )?)
            };

            let scanner_lease = runtime.snapshot_current_source()?;
            let target_parser_start = parser_prefix.byte_end();
            let green = self.green.as_ref().ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ),
            )?;
            let joined = restart.resume(runtime, green, target_lease, parser_prefix)?;
            let (controller, writer) = joined.into_local_fragment()?;
            let target_restart = writer
                .capture_restart_checkpoint(controller.capture_restart()?)
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
                convergence: Some(convergence),
                green_prefix: Some(green_prefix),
                green_suffix,
                reference_prefix,
                reference_adoption: None,
                target_green: None,
                adopted_convergence: None,
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
        let limits = M11RecursiveGreenFrameQueryLimits::new(64, 8192, 64, 8192).ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green query limits must be nonzero",
            ),
        )?;
        let fence = resolve_m11_recursive_green_inline_leaf_fence(
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
            "recursive-Green point is not owned by a final inline-bearing leaf",
        ))?;
        let block_source = to_u32_range(fence.block_source_range())?;
        let block_source_utf16 = to_u32_range(fence.block_source_utf16_range())?;
        let inline_source = to_u32_range(fence.inline_source_range())?;
        let inline_source_utf16 = to_u32_range(fence.inline_source_utf16_range())?;
        let query_receipt = fence.receipt();
        Ok(M11RecursiveGreenInlineLeafPreparation::from_persistent_session(
            block_source,
            block_source_utf16,
            inline_source,
            inline_source_utf16,
            query_receipt,
            fence,
        ))
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

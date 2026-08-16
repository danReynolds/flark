//! Session-owned recursive Green and reference authority.
//!
//! This is the production migration seam for the candidate endpoint.  A clean
//! build is explicit caller-fuelled work performed once when a document is
//! opened.  The resulting roots remain owned by the session, so hot-inline
//! queries never rebuild or discard document structure.

use std::{
    collections::BTreeMap,
    fmt,
    ops::{Index, Range},
};

#[cfg(test)]
use std::{collections::btree_map, slice};

#[cfg(test)]
use flark_engine::parser_internal::M11RecursiveGreenEvent;
use flark_engine::parser_internal::{
    M11RecursiveGreenError, M11RecursiveGreenFrameQueryError, M11RecursiveGreenFrameQueryLimits,
    M11RecursiveGreenLocation, M11RecursiveGreenPoint, M11RecursiveGreenRoot,
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenRowWindow,
    M11RecursiveGreenStoragePageIdentity, M11RecursiveGreenStructuralSpliceSelection,
    M11ReferenceJournal, M11ReferenceJournalAdoptionStatus, M11ReferenceJournalError,
    M11ReferenceJournalRangeReplacement, M11ReferenceJournalRangeReplacementStatus,
    M11ReferenceJournalRoot, M11ReferenceJournalStatus, M11ReferenceJournalUnchangedPrefixAdoption,
    M11ReferenceResolver, BLOCK_QUOTE_WINDOW_MAX_BYTES,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeError, ExactUnchangedPrefixWitness,
    ExactUnchangedSuffixWitness, SourceSnapshotLease, SourceVersion,
};

use crate::block_core::{
    resolve_m11_recursive_green_inline_leaf_row_fence, resolve_m11_recursive_green_paragraph_fence,
    BlockCommand, BlockKind, M11BlockCheckpointRebase, M11BlockOrdinaryCheckpointAdoption,
    M11BlockRestartCheckpoint, M11BlockRestartError, M11BlockStructuralAdoptionReceipt,
    M11BlockTerminalCheckpointAdoption, M11BlockTerminalConvergenceCheckpoint, M11BlockWriter,
    M11BlockWriterError, M11BlockWriterOfferStatus, M11BlockWriterPollStatus,
    M11DirectBlockController, M11DirectBlockControllerError, M11DirectBlockError,
    M11DirectBlockPollStatus, M11DirectSourceLineAdmission, M11ReferenceRendezvous,
    M11ReferenceRendezvousError, M11ReferenceRendezvousStatus, SourceMetric,
};
#[cfg(test)]
use crate::block_core::{
    M11CompactProbeCheckpointFacts, M11CompactProbeFirstSlice, M11CompactProbeWriterReceipt,
    M11CompactReferenceJournal, M11CompactReferenceReceipt, M11DirectDurableBlockRestart,
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
// Product profile identity shared with `flark-runtime`. Other nonzero profile
// identities retain the compatibility CommonMark grammar until explicitly
// promoted rather than silently inheriting GFM behavior.
const SYNTAX_PROFILE_GFM_V1: u32 = 1;
const CHECKPOINT_STRIDE_BYTES: u64 = 4 * 1024;
const LATER_CONVERGENCE_MAX_BYTES: usize = 64 * 1024;
const LATER_CONVERGENCE_MAX_PHYSICAL_LINES: u64 = 512;
// A semantic split can require the parser to reach the terminal checkpoint
// even while remaining inside the independent 64 KiB / 512-line locality
// envelope. Count enough actor quanta for that bounded tail, including line
// scanning, block commands, and reference rendezvous work.
const LATER_CONVERGENCE_MAX_TRANSITIONS: usize = 16_384;
const CHECKPOINT_PAGE_CAPACITY: usize = 64;
#[cfg(test)]
const COMPACT_RESTART_PAGE_BYTES: usize = 4 * 1024;

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
        let controller = if self.syntax_profile == SYNTAX_PROFILE_GFM_V1 {
            M11DirectBlockController::new_gfm()?
        } else {
            M11DirectBlockController::new()?
        };
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
            #[cfg(test)]
            compact_reference_journal: None,
            #[cfg(test)]
            compact_checkpoint_journal: None,
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
            #[cfg(test)]
            compact_probe: false,
            #[cfg(test)]
            compact_probe_receipt: None,
            #[cfg(test)]
            compact_reference_receipt: None,
            #[cfg(test)]
            compact_checkpoint_boundaries_seen: 0,
            #[cfg(test)]
            compact_restart_captures: 0,
        })
    }

    #[cfg(test)]
    fn begin_compact_probe(
        self,
        runtime: &mut DocumentRuntime,
    ) -> Result<M11PersistentRecursiveGreenCleanBuild, M11PersistentRecursiveGreenSessionError>
    {
        let source = self.scanner_lease.version();
        if runtime.current_source_version() != Some(source) {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "compact clean probe is not current",
            ));
        }
        let scanner = SnapshotLineScanner::new(self.scanner_lease)?;
        let controller = if self.syntax_profile == SYNTAX_PROFILE_GFM_V1 {
            M11DirectBlockController::new_gfm()?
        } else {
            M11DirectBlockController::new()?
        };
        let writer = M11BlockWriter::new_compact_probe(runtime, self.writer_lease)?;
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
            journal: None,
            compact_reference_journal: Some(M11CompactReferenceJournal::new()),
            compact_checkpoint_journal: Some(M11CompactCheckpointJournal::new()),
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
            compact_probe: true,
            compact_probe_receipt: None,
            compact_reference_receipt: None,
            compact_checkpoint_boundaries_seen: 0,
            compact_restart_captures: 0,
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
    #[cfg(test)]
    compact_reference_journal: Option<M11CompactReferenceJournal>,
    #[cfg(test)]
    compact_checkpoint_journal: Option<M11CompactCheckpointJournal>,
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
    #[cfg(test)]
    compact_probe: bool,
    #[cfg(test)]
    compact_probe_receipt: Option<M11CompactProbeWriterReceipt>,
    #[cfg(test)]
    compact_reference_receipt: Option<M11CompactReferenceReceipt>,
    #[cfg(test)]
    compact_checkpoint_boundaries_seen: usize,
    #[cfg(test)]
    compact_restart_captures: usize,
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
            #[cfg(test)]
            let compact_probe = self.compact_probe;
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
            #[cfg(test)]
            let poll = if compact_probe {
                let journal = self.compact_reference_journal.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "compact reference journal is missing",
                    ),
                )?;
                rendezvous.poll_compact(controller, writer, journal, runtime, 256)?
            } else {
                let journal = self.journal.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green reference journal is missing",
                    ),
                )?;
                rendezvous.poll(controller, writer, journal, runtime, 1)?
            };
            #[cfg(not(test))]
            let poll = {
                let journal = self.journal.as_mut().ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "recursive-Green reference journal is missing",
                    ),
                )?;
                rendezvous.poll(controller, writer, journal, runtime, 1)?
            };
            if poll.status != M11ReferenceRendezvousStatus::Complete {
                self.rendezvous = Some(rendezvous);
            } else {
                if let Some(invalidation_start) = rendezvous.take_checkpoint_invalidation_start() {
                    self.checkpoints.retain(|checkpoint| {
                        let accepted = checkpoint.accepted_physical();
                        let parser = checkpoint.parser_physical();
                        let before_start = accepted.bytes() < invalidation_start.bytes()
                            && accepted.utf16() < invalidation_start.utf16()
                            && parser.bytes() < invalidation_start.bytes()
                            && parser.utf16() < invalidation_start.utf16();
                        let closed_at_start = accepted.bytes() <= invalidation_start.bytes()
                            && accepted.utf16() <= invalidation_start.utf16()
                            && parser.bytes() <= invalidation_start.bytes()
                            && parser.utf16() <= invalidation_start.utf16()
                            && !checkpoint
                                .open_kinds()
                                .any(|kind| matches!(kind, BlockKind::Paragraph));
                        // The rewrite starts after this closed structural
                        // boundary, so a BOF/parent-only checkpoint at the
                        // same physical cut remains valid. Any checkpoint in
                        // the rewritten Paragraph carries stale Green/logical
                        // authority and must be discarded.
                        before_start || closed_at_start
                    });
                }
                if let Some(remainder) = rendezvous.take_leading_reference_remainder() {
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
                        #[cfg(test)]
                        if self.compact_probe
                            && !self
                                .controller_mut()?
                                .paragraph_may_have_reference_prefix()?
                        {
                            self.writer_mut()?
                                .compact_probe_abandon_reference_window()?;
                        }
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
                        #[cfg(test)]
                        if self.compact_probe {
                            self.compact_probe_receipt =
                                Some(self.writer_mut()?.compact_probe_receipt()?);
                        } else {
                            self.green = Some(self.writer_mut()?.take_root().ok_or(
                                M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "completed recursive-Green writer omitted its root",
                                ),
                            )?);
                        }
                        #[cfg(not(test))]
                        {
                            self.green = Some(self.writer_mut()?.take_root().ok_or(
                                M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "completed recursive-Green writer omitted its root",
                                ),
                            )?);
                        }
                        #[cfg(test)]
                        if self.compact_probe {
                            let journal = self.compact_reference_journal.as_mut().ok_or(
                                M11PersistentRecursiveGreenSessionError::InvalidState(
                                    "compact reference journal is missing",
                                ),
                            )?;
                            journal.finish_input()?;
                            self.compact_reference_receipt = Some(journal.receipt());
                        } else {
                            self.journal_mut()?.finish_input(runtime)?;
                        }
                        #[cfg(not(test))]
                        self.journal_mut()?.finish_input(runtime)?;
                        self.phase = CleanPhase::FinishReferences;
                    }
                }
            }
            CleanPhase::FinishReferences => {
                #[cfg(test)]
                if self.compact_probe {
                    self.writer = None;
                    self.compact_reference_journal = None;
                    self.controller = None;
                    self.output = Some(M11PersistentRecursiveGreenSession {
                        source: self.source,
                        syntax_profile: self.syntax_profile,
                        green: self.green.take(),
                        references: None,
                        checkpoints: M11CheckpointStore::from_contiguous(std::mem::take(
                            &mut self.checkpoints,
                        )),
                        compact_checkpoints: self.compact_checkpoint_journal.take(),
                        terminal_convergence: self.terminal_convergence.take(),
                        release_begun: false,
                        green_release_complete: true,
                        references_release_complete: true,
                        compact_probe: true,
                    });
                    self.phase = CleanPhase::Complete;
                    return Ok(());
                }
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
                        checkpoints: M11CheckpointStore::from_contiguous(std::mem::take(
                            &mut self.checkpoints,
                        )),
                        terminal_convergence: self.terminal_convergence.take(),
                        release_begun: false,
                        green_release_complete: false,
                        references_release_complete: false,
                        #[cfg(test)]
                        compact_probe: self.compact_probe,
                        #[cfg(test)]
                        compact_checkpoints: self.compact_checkpoint_journal.take(),
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
        #[cfg(test)]
        if self.compact_probe {
            self.compact_checkpoint_boundaries_seen = self
                .compact_checkpoint_boundaries_seen
                .checked_add(1)
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact checkpoint boundary count fits usize",
                ))?;
            let (metric, open_depth) = self
                .writer
                .as_ref()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact writer is missing",
                ))?
                .compact_probe_checkpoint_candidate()?;
            self.writer_mut()?
                .compact_probe_maybe_freeze_first_slice(metric, open_depth)?;
            let previous_cut = self
                .compact_checkpoint_journal
                .as_ref()
                .and_then(M11CompactCheckpointJournal::last_cut)
                .map_or(0, SourceMetric::bytes);
            let minimum_stride = CHECKPOINT_STRIDE_BYTES
                .checked_mul(u64::try_from(open_depth).unwrap_or(u64::MAX).max(1))
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact checkpoint spacing fits u64",
                ))?;
            let distinct = self
                .compact_checkpoint_journal
                .as_ref()
                .and_then(M11CompactCheckpointJournal::last_cut)
                != Some(metric);
            if !distinct || (!force && metric.bytes().saturating_sub(previous_cut) < minimum_stride)
            {
                return Ok(());
            }
            let Some(parser) = self
                .controller_mut()?
                .capture_durable_restart_if_available()?
            else {
                return Ok(());
            };
            let facts = self
                .writer_mut()?
                .capture_compact_probe_checkpoint_facts(&parser)?;
            if facts.accepted_physical != metric || facts.open_depth != open_depth {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact checkpoint selection and joined durable facts differ",
                ));
            }
            self.compact_checkpoint_journal
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact checkpoint journal is missing",
                ))?
                .push(&parser, facts)
                .map_err(M11PersistentRecursiveGreenSessionError::InvalidState)?;
            self.compact_restart_captures = self.compact_restart_captures.checked_add(1).ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact restart capture count fits usize",
                ),
            )?;
            return Ok(());
        }
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
        #[cfg(test)]
        if self.compact_probe {
            let journal_empty = self
                .compact_checkpoint_journal
                .as_ref()
                .is_some_and(|journal| journal.entries.is_empty());
            if !journal_empty {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact BOF checkpoint must be first",
                ));
            }
            let parser = self
                .controller_mut()?
                .capture_durable_document_start_restart()?;
            let facts = self
                .writer_mut()?
                .capture_compact_probe_checkpoint_facts(&parser)?;
            if facts.accepted_physical != SourceMetric::default()
                || facts.logical != SourceMetric::default()
                || facts.open_depth != 1
            {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact BOF checkpoint is Document-only at source zero",
                ));
            }
            self.compact_checkpoint_journal
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact checkpoint journal is missing",
                ))?
                .push(&parser, facts)
                .map_err(M11PersistentRecursiveGreenSessionError::InvalidState)?;
            self.compact_restart_captures = self.compact_restart_captures.checked_add(1).ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact BOF restart capture count fits usize",
                ),
            )?;
            return Ok(());
        }
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
        #[cfg(test)]
        if self.compact_probe {
            self.compact_restart_captures = self.compact_restart_captures.checked_add(1).ok_or(
                M11PersistentRecursiveGreenSessionError::InvalidState(
                    "compact BOF restart capture count fits usize",
                ),
            )?;
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

    #[cfg(test)]
    fn take_compact_probe_receipt(
        &mut self,
    ) -> Option<(M11CompactProbeWriterReceipt, usize, usize)> {
        if self.phase != CleanPhase::Complete || !self.compact_probe {
            return None;
        }
        self.compact_probe_receipt.take().map(|mut receipt| {
            if let Some(reference) = self.compact_reference_receipt.take() {
                receipt.reference_occurrences = reference.occurrences;
                receipt.reference_winners = reference.winners;
                receipt.reference_allocated_bytes = reference.allocated_bytes;
                receipt.reference_normalized_label_bytes = reference.normalized_label_bytes;
                receipt.reference_phase_transitions = reference.rendezvous_phase_transitions;
            }
            (
                receipt,
                self.compact_checkpoint_boundaries_seen,
                self.compact_restart_captures,
            )
        })
    }

    #[cfg(test)]
    fn compact_probe_current_writer_receipt(&self) -> Option<M11CompactProbeWriterReceipt> {
        self.compact_probe
            .then(|| self.writer.as_ref()?.compact_probe_receipt().ok())
            .flatten()
    }

    #[cfg(test)]
    fn take_compact_probe_first_slice(&mut self) -> Option<M11CompactProbeFirstSlice> {
        self.compact_probe
            .then(|| {
                self.writer
                    .as_mut()?
                    .compact_probe_take_first_slice()
                    .ok()?
            })
            .flatten()
    }

    #[cfg(test)]
    fn compact_probe_first_slice_over_cap(&self) -> bool {
        self.compact_probe
            && self
                .writer
                .as_ref()
                .is_some_and(|writer| writer.compact_probe_first_slice_over_cap().unwrap_or(false))
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

enum M11CheckpointStore {
    Contiguous(Vec<M11BlockRestartCheckpoint>),
    Paged {
        pages: BTreeMap<usize, Box<[M11BlockRestartCheckpoint]>>,
        len: usize,
    },
}

impl M11CheckpointStore {
    fn from_contiguous(checkpoints: Vec<M11BlockRestartCheckpoint>) -> Self {
        Self::Contiguous(checkpoints)
    }

    fn len(&self) -> usize {
        match self {
            Self::Contiguous(checkpoints) => checkpoints.len(),
            Self::Paged { len, .. } => *len,
        }
    }

    fn get(&self, index: usize) -> Option<&M11BlockRestartCheckpoint> {
        match self {
            Self::Contiguous(checkpoints) => checkpoints.get(index),
            Self::Paged { pages, len } => {
                if index >= *len {
                    return None;
                }
                let (start, page) = pages.range(..=index).next_back()?;
                page.get(index - *start)
            }
        }
    }

    fn partition_point<P>(&self, mut predicate: P) -> usize
    where
        P: FnMut(&M11BlockRestartCheckpoint) -> bool,
    {
        let mut left = 0;
        let mut right = self.len();
        while left < right {
            let middle = left + (right - left) / 2;
            if predicate(
                self.get(middle)
                    .expect("checkpoint partition index remains in bounds"),
            ) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left
    }

    fn partition_point_from<P>(&self, start: usize, mut predicate: P) -> usize
    where
        P: FnMut(&M11BlockRestartCheckpoint) -> bool,
    {
        let mut left = start.min(self.len());
        let mut right = self.len();
        while left < right {
            let middle = left + (right - left) / 2;
            if predicate(
                self.get(middle)
                    .expect("checkpoint partition index remains in bounds"),
            ) {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        left - start.min(self.len())
    }

    #[cfg(test)]
    fn iter(&self) -> M11CheckpointStoreIter<'_> {
        match self {
            Self::Contiguous(checkpoints) => M11CheckpointStoreIter::Contiguous(checkpoints.iter()),
            Self::Paged { pages, .. } => M11CheckpointStoreIter::Paged {
                pages: pages.values(),
                current: None,
            },
        }
    }
}

impl Index<usize> for M11CheckpointStore {
    type Output = M11BlockRestartCheckpoint;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
            .expect("checkpoint index remains inside persistent store")
    }
}

#[cfg(test)]
enum M11CheckpointStoreIter<'a> {
    Contiguous(slice::Iter<'a, M11BlockRestartCheckpoint>),
    Paged {
        pages: btree_map::Values<'a, usize, Box<[M11BlockRestartCheckpoint]>>,
        current: Option<slice::Iter<'a, M11BlockRestartCheckpoint>>,
    },
}

#[cfg(test)]
impl<'a> Iterator for M11CheckpointStoreIter<'a> {
    type Item = &'a M11BlockRestartCheckpoint;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Contiguous(checkpoints) => checkpoints.next(),
            Self::Paged { pages, current } => loop {
                if let Some(checkpoint) = current.as_mut().and_then(Iterator::next) {
                    return Some(checkpoint);
                }
                *current = Some(pages.next()?.iter());
            },
        }
    }
}

struct M11PagedCheckpointBuilder {
    pages: BTreeMap<usize, Box<[M11BlockRestartCheckpoint]>>,
    tail: Vec<M11BlockRestartCheckpoint>,
    len: usize,
}

impl M11PagedCheckpointBuilder {
    fn new() -> Self {
        Self {
            pages: BTreeMap::new(),
            tail: Vec::new(),
            len: 0,
        }
    }

    fn last(&self) -> Option<&M11BlockRestartCheckpoint> {
        self.tail.last().or_else(|| {
            self.pages
                .last_key_value()
                .and_then(|(_, page)| page.last())
        })
    }

    fn push(&mut self, checkpoint: M11BlockRestartCheckpoint) -> Result<(), M11BlockWriterError> {
        if self.tail.is_empty() {
            self.tail
                .try_reserve_exact(CHECKPOINT_PAGE_CAPACITY)
                .map_err(|_| M11BlockWriterError::Allocation)?;
        }
        self.tail.push(checkpoint);
        self.len = self
            .len
            .checked_add(1)
            .ok_or(M11BlockWriterError::Allocation)?;
        if self.tail.len() == CHECKPOINT_PAGE_CAPACITY {
            self.flush_tail()?;
        }
        Ok(())
    }

    fn flush_tail(&mut self) -> Result<(), M11BlockWriterError> {
        if self.tail.is_empty() {
            return Ok(());
        }
        let start = self
            .len
            .checked_sub(self.tail.len())
            .ok_or(M11BlockWriterError::Allocation)?;
        let page = std::mem::take(&mut self.tail).into_boxed_slice();
        if self.pages.insert(start, page).is_some() {
            return Err(M11BlockWriterError::Allocation);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<M11CheckpointStore, M11BlockWriterError> {
        self.flush_tail()?;
        Ok(M11CheckpointStore::Paged {
            pages: self.pages,
            len: self.len,
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
    checkpoints: M11CheckpointStore,
    terminal_convergence: Option<M11BlockTerminalConvergenceCheckpoint>,
    release_begun: bool,
    green_release_complete: bool,
    references_release_complete: bool,
    #[cfg(test)]
    compact_probe: bool,
    #[cfg(test)]
    compact_checkpoints: Option<M11CompactCheckpointJournal>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M11CompactCheckpointEntry {
    stream_offset: u64,
    encoded_len: u32,
    line_ordinal: u64,
    last_line_length: u64,
    accepted_physical: SourceMetric,
    logical: SourceMetric,
    event_cut: u64,
    renderable_rows: u64,
    open_depth: u32,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct M11CompactCheckpointJournal {
    pages: Vec<Box<[u8]>>,
    entries: Vec<M11CompactCheckpointEntry>,
    stream_len: usize,
}

#[cfg(test)]
impl M11CompactCheckpointJournal {
    const fn new() -> Self {
        Self {
            pages: Vec::new(),
            entries: Vec::new(),
            stream_len: 0,
        }
    }

    fn last_cut(&self) -> Option<SourceMetric> {
        self.entries.last().map(|entry| entry.accepted_physical)
    }

    fn append_bytes(&mut self, mut bytes: &[u8]) -> Result<(), &'static str> {
        while !bytes.is_empty() {
            let page_index = self.stream_len / COMPACT_RESTART_PAGE_BYTES;
            let page_offset = self.stream_len % COMPACT_RESTART_PAGE_BYTES;
            if page_index == self.pages.len() {
                let mut page = Vec::new();
                page.try_reserve_exact(COMPACT_RESTART_PAGE_BYTES)
                    .map_err(|_| "compact restart page allocation failed")?;
                page.resize(COMPACT_RESTART_PAGE_BYTES, 0);
                self.pages
                    .try_reserve(1)
                    .map_err(|_| "compact restart page directory allocation failed")?;
                self.pages.push(page.into_boxed_slice());
            }
            let accepted = bytes
                .len()
                .min(COMPACT_RESTART_PAGE_BYTES.saturating_sub(page_offset));
            self.pages[page_index][page_offset..page_offset + accepted]
                .copy_from_slice(&bytes[..accepted]);
            self.stream_len = self
                .stream_len
                .checked_add(accepted)
                .ok_or("compact restart stream length overflow")?;
            bytes = &bytes[accepted..];
        }
        Ok(())
    }

    fn push(
        &mut self,
        parser: &M11DirectDurableBlockRestart,
        facts: M11CompactProbeCheckpointFacts,
    ) -> Result<(), &'static str> {
        let encoded_len = u32::try_from(parser.encoded_len())
            .map_err(|_| "compact restart record length fits u32")?;
        let stream_offset =
            u64::try_from(self.stream_len).map_err(|_| "compact restart offset fits u64")?;
        let open_depth =
            u32::try_from(facts.open_depth).map_err(|_| "compact restart open depth fits u32")?;
        self.entries
            .try_reserve(1)
            .map_err(|_| "compact restart entry allocation failed")?;
        let mut append_error = None;
        parser.visit_encoded_bytes(|bytes| {
            if append_error.is_none() {
                append_error = self.append_bytes(bytes).err();
            }
        });
        if let Some(error) = append_error {
            return Err(error);
        }
        self.entries.push(M11CompactCheckpointEntry {
            stream_offset,
            encoded_len,
            line_ordinal: parser.line_ordinal(),
            last_line_length: parser.last_line_length(),
            accepted_physical: facts.accepted_physical,
            logical: facts.logical,
            event_cut: facts.event_cut,
            renderable_rows: facts.renderable_rows,
            open_depth,
        });
        Ok(())
    }

    fn receipt(&self) -> CheckpointStorageReceipt {
        let retained_open_frames = self.entries.iter().fold(0_usize, |sum, entry| {
            sum.saturating_add(entry.open_depth as usize)
        });
        let maximum_open_depth = self
            .entries
            .iter()
            .map(|entry| entry.open_depth as usize)
            .max()
            .unwrap_or(0);
        CheckpointStorageReceipt {
            checkpoints: self.entries.len(),
            retained_open_frames,
            maximum_open_depth,
            allocated_bytes: self
                .pages
                .capacity()
                .saturating_mul(std::mem::size_of::<Box<[u8]>>())
                .saturating_add(
                    self.pages
                        .iter()
                        .fold(0_usize, |sum, page| sum.saturating_add(page.len())),
                )
                .saturating_add(
                    self.entries
                        .capacity()
                        .saturating_mul(std::mem::size_of::<M11CompactCheckpointEntry>()),
                ),
        }
    }

    fn encoded_entry(&self, index: usize) -> Result<Vec<u8>, &'static str> {
        let entry = self
            .entries
            .get(index)
            .ok_or("compact restart entry index is in bounds")?;
        let start = usize::try_from(entry.stream_offset)
            .map_err(|_| "compact restart offset fits usize")?;
        let len = entry.encoded_len as usize;
        let end = start
            .checked_add(len)
            .ok_or("compact restart range does not overflow")?;
        if end > self.stream_len {
            return Err("compact restart range is inside the encoded stream");
        }
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(len)
            .map_err(|_| "compact restart decode allocation failed")?;
        let mut cursor = start;
        while cursor < end {
            let page_index = cursor / COMPACT_RESTART_PAGE_BYTES;
            let page_offset = cursor % COMPACT_RESTART_PAGE_BYTES;
            let accepted = (end - cursor).min(COMPACT_RESTART_PAGE_BYTES - page_offset);
            encoded.extend_from_slice(
                self.pages
                    .get(page_index)
                    .and_then(|page| page.get(page_offset..page_offset + accepted))
                    .ok_or("compact restart bytes are inside retained pages")?,
            );
            cursor += accepted;
        }
        Ok(encoded)
    }

    fn validate_metadata_and_durable_samples(&self) -> Result<(), &'static str> {
        let first = self
            .entries
            .first()
            .ok_or("compact restart journal retains BOF")?;
        if first.accepted_physical != SourceMetric::default()
            || first.logical != SourceMetric::default()
            || first.event_cut != 1
            || first.renderable_rows != 0
            || first.line_ordinal != 0
            || first.last_line_length != 0
        {
            return Err("compact restart journal begins with canonical BOF metadata");
        }
        for pair in self.entries.windows(2) {
            if pair[0].accepted_physical >= pair[1].accepted_physical
                || pair[0].logical > pair[1].logical
                || pair[0].event_cut > pair[1].event_cut
                || pair[0].renderable_rows > pair[1].renderable_rows
                || pair[0].stream_offset + u64::from(pair[0].encoded_len) != pair[1].stream_offset
            {
                return Err("compact restart metadata and stream offsets are monotonic");
            }
        }
        // BOF is reconstructed from the canonical fresh-parser constructor;
        // persisted restart decoding is needed only for interior/EOF cuts.
        let mut indices = vec![self.entries.len() / 2, self.entries.len() - 1];
        indices.sort_unstable();
        indices.dedup();
        for index in indices {
            let entry = self.entries[index];
            let encoded = self.encoded_entry(index)?;
            let resumed = M11DirectBlockController::resume_durable_encoded_restart(
                &encoded,
                entry.line_ordinal,
                entry.last_line_length,
                None,
            )
            .map_err(|_| "compact donor restart decodes without a Green root")?;
            let recaptured = resumed
                .capture_durable_restart_if_available()
                .map_err(|_| "decoded compact restart can be recaptured")?
                .ok_or("decoded compact restart remains donor-reachable")?;
            let mut roundtrip = Vec::new();
            roundtrip
                .try_reserve_exact(recaptured.encoded_len())
                .map_err(|_| "compact restart roundtrip allocation failed")?;
            recaptured.visit_encoded_bytes(|bytes| roundtrip.extend_from_slice(bytes));
            if roundtrip != encoded {
                return Err("compact durable restart roundtrips canonically");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CheckpointStorageReceipt {
    checkpoints: usize,
    retained_open_frames: usize,
    maximum_open_depth: usize,
    allocated_bytes: usize,
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

    #[must_use]
    const fn allocated_bytes(self) -> usize {
        self.allocated_bytes
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

/// Which parser-authenticated unchanged side of an in-flight adoption can
/// answer current-revision structural row queries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11PersistentRecursiveGreenProjectionRegionKind {
    Prefix,
    Suffix,
}

/// Exact base-to-target coordinate map for one parser-authenticated unchanged
/// range. The range contains structural facts that can be projected into the
/// target revision while the edited middle remains pending.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PersistentRecursiveGreenProjectionRegion {
    kind: M11PersistentRecursiveGreenProjectionRegionKind,
    base_start_byte: usize,
    base_end_byte: usize,
    base_start_utf16: usize,
    base_end_utf16: usize,
    target_start_byte: usize,
    target_end_byte: usize,
    target_start_utf16: usize,
    target_end_utf16: usize,
}

impl M11PersistentRecursiveGreenProjectionRegion {
    #[must_use]
    pub const fn kind(self) -> M11PersistentRecursiveGreenProjectionRegionKind {
        self.kind
    }

    #[must_use]
    pub const fn base_byte_range(self) -> Range<usize> {
        self.base_start_byte..self.base_end_byte
    }

    #[must_use]
    pub const fn base_utf16_range(self) -> Range<usize> {
        self.base_start_utf16..self.base_end_utf16
    }

    #[must_use]
    pub const fn target_byte_range(self) -> Range<usize> {
        self.target_start_byte..self.target_end_byte
    }

    #[must_use]
    pub const fn target_utf16_range(self) -> Range<usize> {
        self.target_start_utf16..self.target_end_utf16
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct M11PersistentRecursiveGreenLiveProjection {
    prefix: Option<M11PersistentRecursiveGreenProjectionRegion>,
    suffix: Option<M11PersistentRecursiveGreenProjectionRegion>,
    suffix_ready: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11PersistentRecursiveGreenAdoptionPoll {
    status: M11PersistentRecursiveGreenAdoptionStatus,
    transitions: usize,
    checkpoint_records_processed: usize,
    maximum_checkpoint_records_per_transition: usize,
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

    #[must_use]
    pub const fn checkpoint_records_processed(self) -> usize {
        self.checkpoint_records_processed
    }

    #[must_use]
    pub const fn maximum_checkpoint_records_per_transition(self) -> usize {
        self.maximum_checkpoint_records_per_transition
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11PersistentRecursiveGreenAdoptionWork {
    source_bytes_read: usize,
    high_level_events: usize,
    green_tree_nodes_rebuilt: usize,
    reference_rebind_transitions: usize,
    checkpoint_records_processed: usize,
    maximum_checkpoint_records_per_transition: usize,
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

    #[must_use]
    pub const fn checkpoint_records_processed(self) -> usize {
        self.checkpoint_records_processed
    }

    #[must_use]
    pub const fn maximum_checkpoint_records_per_transition(self) -> usize {
        self.maximum_checkpoint_records_per_transition
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
    RebaseCheckpoints,
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

struct AdoptedCheckpointSet {
    checkpoints: M11CheckpointStore,
    terminal: M11BlockTerminalConvergenceCheckpoint,
}

struct M11CheckpointAdoption {
    rebase: M11BlockCheckpointRebase,
    output: M11PagedCheckpointBuilder,
    prefix_cursor: usize,
    prefix_end: usize,
    target_restart: Option<M11BlockRestartCheckpoint>,
    target_convergence: Option<M11BlockRestartCheckpoint>,
    suffix_cursor: usize,
    suffix_end: usize,
    terminal: Option<M11BlockTerminalConvergenceCheckpoint>,
    rebase_terminal: bool,
    complete: bool,
}

impl M11CheckpointAdoption {
    fn ordinary(
        adoption: M11BlockOrdinaryCheckpointAdoption,
        restart_index: usize,
        checkpoint_index: usize,
        base_checkpoint_count: usize,
    ) -> Result<Self, M11BlockRestartError> {
        let suffix_cursor =
            checkpoint_index
                .checked_add(1)
                .ok_or(M11BlockRestartError::Pairing(
                    "ordinary checkpoint suffix index overflow",
                ))?;
        if restart_index > checkpoint_index || suffix_cursor > base_checkpoint_count {
            return Err(M11BlockRestartError::Pairing(
                "ordinary checkpoint ranges escaped the base session",
            ));
        }
        Ok(Self {
            rebase: adoption.rebase,
            output: M11PagedCheckpointBuilder::new(),
            prefix_cursor: 0,
            prefix_end: restart_index,
            target_restart: Some(adoption.target_restart),
            target_convergence: Some(adoption.target_convergence),
            suffix_cursor,
            suffix_end: base_checkpoint_count,
            terminal: Some(adoption.retained_terminal),
            rebase_terminal: true,
            complete: false,
        })
    }

    fn terminal(
        adoption: M11BlockTerminalCheckpointAdoption,
        restart_index: usize,
        base_checkpoint_count: usize,
    ) -> Result<Self, M11BlockRestartError> {
        if restart_index > base_checkpoint_count {
            return Err(M11BlockRestartError::Pairing(
                "terminal checkpoint prefix escaped the base session",
            ));
        }
        Ok(Self {
            rebase: adoption.rebase,
            output: M11PagedCheckpointBuilder::new(),
            prefix_cursor: 0,
            prefix_end: restart_index,
            target_restart: Some(adoption.target_restart),
            target_convergence: None,
            suffix_cursor: base_checkpoint_count,
            suffix_end: base_checkpoint_count,
            terminal: Some(adoption.target_terminal),
            rebase_terminal: false,
            complete: false,
        })
    }

    /// Performs exactly one checkpoint-record unit. Empty ranges are skipped
    /// inside the transition, but cloning, rebasing, validation, and paged
    /// storage insertion never cover more than one retained/synthetic record.
    fn poll_one(
        &mut self,
        base: &M11CheckpointStore,
        transaction_id: u64,
    ) -> Result<(), M11BlockRestartError> {
        if self.complete {
            return Err(M11BlockRestartError::Pairing(
                "completed checkpoint adoption cannot resume",
            ));
        }
        if self.prefix_cursor < self.prefix_end {
            let mut checkpoint = base
                .get(self.prefix_cursor)
                .ok_or(M11BlockRestartError::Pairing(
                    "retained prefix checkpoint escaped the base session",
                ))?
                .replicate_for_transaction(transaction_id)?
                .into_checkpoint(transaction_id)?;
            self.rebase.rebase_prefix(&mut checkpoint)?;
            self.push(checkpoint)?;
            self.prefix_cursor += 1;
            return Ok(());
        }
        if let Some(checkpoint) = self.target_restart.take() {
            self.push(checkpoint)?;
            return Ok(());
        }
        if let Some(checkpoint) = self.target_convergence.take() {
            self.push(checkpoint)?;
            return Ok(());
        }
        if self.suffix_cursor < self.suffix_end {
            let mut checkpoint = base
                .get(self.suffix_cursor)
                .ok_or(M11BlockRestartError::Pairing(
                    "retained suffix checkpoint escaped the base session",
                ))?
                .replicate_for_transaction(transaction_id)?
                .into_checkpoint(transaction_id)?;
            self.rebase.rebase_suffix(&mut checkpoint)?;
            self.push(checkpoint)?;
            self.suffix_cursor += 1;
            return Ok(());
        }
        let mut terminal = self.terminal.take().ok_or(M11BlockRestartError::Pairing(
            "checkpoint adoption omitted terminal authority",
        ))?;
        if self.rebase_terminal {
            self.rebase.rebase_terminal(&mut terminal)?;
        }
        self.rebase.validate_terminal(&terminal)?;
        self.terminal = Some(terminal);
        self.complete = true;
        Ok(())
    }

    fn push(&mut self, checkpoint: M11BlockRestartCheckpoint) -> Result<(), M11BlockRestartError> {
        self.rebase.validate_next(self.output.last(), &checkpoint)?;
        self.output.push(checkpoint)?;
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn into_adopted(mut self) -> Result<AdoptedCheckpointSet, M11BlockRestartError> {
        if !self.complete {
            return Err(M11BlockRestartError::Pairing(
                "checkpoint adoption completed before its terminal authority",
            ));
        }
        let terminal = self.terminal.take().ok_or(M11BlockRestartError::Pairing(
            "checkpoint adoption lost terminal authority",
        ))?;
        Ok(AdoptedCheckpointSet {
            checkpoints: self.output.finish()?,
            terminal,
        })
    }
}

/// Proves that one parser checkpoint cannot split a committed reference
/// occurrence. A cut strictly after the final occurrence is trivially safe.
/// A cut exactly at its end can still be a leading-reference remainder whose
/// parser continuation has committed that no more definitions follow. That
/// cut, and every earlier cut, is safe only when no Paragraph is open.
fn checkpoint_proves_reference_occurrence_cut(
    checkpoint: &M11BlockRestartCheckpoint,
    references: &M11ReferenceJournalRoot,
) -> bool {
    let cut = checkpoint.parser_physical();
    (cut.bytes() > references.last_source_byte_end()
        && cut.utf16() > references.last_source_utf16_end())
        || !checkpoint
            .open_kinds()
            .any(|kind| matches!(kind, BlockKind::Paragraph))
}

/// A leading-reference remainder at the exact final occurrence end is not a
/// globally safe convergence cut while its Paragraph remains open: an edit at
/// that cut can extend the reference prefix. It is nevertheless a safe restart
/// for an edit strictly later, because the unchanged bytes between the cut and
/// edit preserve the decision that ended the reference prefix.
fn checkpoint_proves_reference_restart_cut(
    checkpoint: &M11BlockRestartCheckpoint,
    references: &M11ReferenceJournalRoot,
    edit_start: usize,
) -> bool {
    if checkpoint_proves_reference_occurrence_cut(checkpoint, references) {
        return true;
    }
    let cut = checkpoint.parser_physical();
    cut.bytes() == references.last_source_byte_end()
        && cut.utf16() == references.last_source_utf16_end()
        && edit_start > cut.bytes() as usize
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
    live_projection: M11PersistentRecursiveGreenLiveProjection,
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
    checkpoint_adoption: Option<M11CheckpointAdoption>,
    adopted_checkpoints: Option<AdoptedCheckpointSet>,
    recursive_green_splice: Option<M11RecursiveGreenStructuralSpliceSelection>,
    output: Option<M11PersistentRecursiveGreenUpdate>,
    work: M11PersistentRecursiveGreenAdoptionWork,
    cancelling: bool,
    cancel_target: Option<M11PersistentRecursiveGreenSession>,
    cancel_green_complete: bool,
    cancel_references_complete: bool,
}

impl M11PersistentRecursiveGreenAdoption {
    /// Returns at most one exact prefix and one exact suffix in source order.
    /// Missing regions are intentionally pending; callers may never infer a
    /// certified range from the edit coordinates alone.
    #[must_use]
    pub const fn live_projection_regions(
        &self,
    ) -> [Option<M11PersistentRecursiveGreenProjectionRegion>; 2] {
        [
            self.live_projection.prefix,
            if self.live_projection.suffix_ready {
                self.live_projection.suffix
            } else {
                None
            },
        ]
    }
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
        let checkpoint_start = self.work.checkpoint_records_processed;
        let mut maximum_checkpoint_records_per_transition = 0;
        let mut transitions = 0;
        while transitions < fuel
            && !matches!(
                self.phase,
                AdoptionPhase::Complete | AdoptionPhase::CleanFallbackRequired
            )
        {
            let transition_checkpoint_start = self.work.checkpoint_records_processed;
            self.poll_one(runtime)?;
            let transition_checkpoint_records = self
                .work
                .checkpoint_records_processed
                .checked_sub(transition_checkpoint_start)
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "checkpoint transition work regressed",
                ))?;
            if transition_checkpoint_records > 1 {
                return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "checkpoint transition exceeded one-record fuel",
                ));
            }
            maximum_checkpoint_records_per_transition =
                maximum_checkpoint_records_per_transition.max(transition_checkpoint_records);
            self.work.maximum_checkpoint_records_per_transition = self
                .work
                .maximum_checkpoint_records_per_transition
                .max(transition_checkpoint_records);
            transitions += 1;
        }
        let checkpoint_records_processed = self
            .work
            .checkpoint_records_processed
            .checked_sub(checkpoint_start)
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "checkpoint poll work regressed",
            ))?;
        Ok(M11PersistentRecursiveGreenAdoptionPoll {
            status: match self.phase {
                AdoptionPhase::Complete => M11PersistentRecursiveGreenAdoptionStatus::Complete,
                AdoptionPhase::CleanFallbackRequired => {
                    M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired
                }
                _ => M11PersistentRecursiveGreenAdoptionStatus::Pending,
            },
            transitions,
            checkpoint_records_processed,
            maximum_checkpoint_records_per_transition,
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
                    .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "completed reference adoption omitted its root",
                    ))?;
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
                    self.live_projection.suffix_ready = true;
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
                    let base_checkpoint_count = base.checkpoints.len();
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
                                    retained_terminal,
                                )
                                .and_then(|(green, receipt, checkpoint_adoption)| {
                                    Ok((
                                        green,
                                        receipt,
                                        M11CheckpointAdoption::ordinary(
                                            checkpoint_adoption,
                                            selection.restart_index,
                                            checkpoint_index,
                                            base_checkpoint_count,
                                        )?,
                                    ))
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
                                )
                                .and_then(|(green, receipt, checkpoint_adoption)| {
                                    Ok((
                                        green,
                                        receipt,
                                        M11CheckpointAdoption::terminal(
                                            checkpoint_adoption,
                                            selection.restart_index,
                                            base_checkpoint_count,
                                        )?,
                                    ))
                                })
                        }
                    }
                };
                let (green, receipt, checkpoint_adoption) = match adoption_result {
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
                self.checkpoint_adoption = Some(checkpoint_adoption);
                self.controller = None;
                self.record_structural_work(receipt)?;
                self.phase = AdoptionPhase::RebaseCheckpoints;
            }
            AdoptionPhase::RebaseCheckpoints => {
                let transaction_id = self.checkpoint_selection.transaction_id;
                {
                    let base = self.base.as_ref().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green checkpoint adoption omitted its base session",
                        ),
                    )?;
                    self.checkpoint_adoption
                        .as_mut()
                        .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green checkpoint adoption actor is missing",
                        ))?
                        .poll_one(&base.checkpoints, transaction_id)?;
                }
                self.record_checkpoint_record()?;
                if self
                    .checkpoint_adoption
                    .as_ref()
                    .is_some_and(M11CheckpointAdoption::is_complete)
                {
                    let checkpoints = self
                        .checkpoint_adoption
                        .take()
                        .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "completed checkpoint adoption actor disappeared",
                        ))?
                        .into_adopted()?;
                    self.adopted_checkpoints = Some(checkpoints);
                    self.begin_reference_finish(runtime)?;
                }
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
        let live_suffix =
            green_suffix
                .as_ref()
                .map(|suffix| M11PersistentRecursiveGreenProjectionRegion {
                    kind: M11PersistentRecursiveGreenProjectionRegionKind::Suffix,
                    base_start_byte: suffix.base_byte_start(),
                    base_end_byte: base.source.byte_len(),
                    base_start_utf16: suffix.base_utf16_start(),
                    base_end_utf16: base.source.utf16_len(),
                    target_start_byte: suffix.target_byte_start(),
                    target_end_byte: self.target.byte_len(),
                    target_start_utf16: suffix.target_utf16_start(),
                    target_end_utf16: self.target.utf16_len(),
                });

        self.checkpoint_selection.convergence = AdoptionConvergence::Ordinary {
            checkpoint_index: next_index,
        };
        self.target_parser_end = target_parser_end;
        self.green_suffix = green_suffix;
        self.live_projection.suffix = live_suffix;
        self.live_projection.suffix_ready = false;
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
        self.live_projection.suffix = None;
        self.live_projection.suffix_ready = false;
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

    fn record_checkpoint_record(&mut self) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        self.work.checkpoint_records_processed = self
            .work
            .checkpoint_records_processed
            .checked_add(1)
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "checkpoint adoption work overflow",
            ))?;
        Ok(())
    }

    fn begin_reference_finish(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let selection = self.checkpoint_selection;
        if self.reference_replacement.is_some() {
            let (base_source, convergence) = {
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
                    .ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "reference terminal source metric is invalid",
                        ),
                    )?,
                };
                (base.source, convergence)
            };
            let suffix = if convergence.bytes() as usize == base_source.byte_len()
                && convergence.utf16() as usize == base_source.utf16_len()
            {
                None
            } else {
                let suffix = runtime.mint_exact_unchanged_suffix_witness(
                    base_source,
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
            self.reference_replacement
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "reference replacement actor disappeared before finish",
                ))?
                .finish_replacement(runtime, suffix)?;
            self.reference_range_ready = false;
            self.reference_replacement_finishing = true;
        } else {
            self.reference_range_prefix = None;
            let reference_prefix = self.reference_prefix.take();
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
        Ok(())
    }

    fn finish_target_with_references(
        &mut self,
        references: M11ReferenceJournalRoot,
    ) -> Result<(), M11PersistentRecursiveGreenSessionError> {
        let base =
            self.base
                .take()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green adoption omitted its base session",
                ))?;
        let adopted = self.adopted_checkpoints.take().ok_or(
            M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green adoption omitted target checkpoint authority",
            ),
        )?;
        let checkpoints = adopted.checkpoints;
        let terminal_convergence = Some(adopted.terminal);
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
            #[cfg(test)]
            compact_probe: false,
            #[cfg(test)]
            compact_checkpoints: None,
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
        self.checkpoint_adoption = None;
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
        #[cfg(test)]
        if let Some(compact) = &self.compact_checkpoints {
            return compact.entries.len();
        }
        self.checkpoints.len()
    }

    #[must_use]
    pub fn reference_occurrence_count(&self) -> u64 {
        self.references
            .as_ref()
            .map_or(0, M11ReferenceJournalRoot::occurrence_count)
    }

    /// Returns a cheap resolver over the current session-owned reference
    /// winners. The session remains the arena-page owner for the resolver's
    /// entire use.
    pub fn reference_resolver(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ReferenceResolver, M11PersistentRecursiveGreenSessionError> {
        let root = self.current_reference_root(runtime)?;
        Ok(M11ReferenceResolver::from_live_reference_journal(
            runtime, root,
        )?)
    }

    /// Whether an edit starts before the base's first authenticated reference
    /// occurrence. A local structural crop may converge before that distant
    /// occurrence, but it cannot certify the absolute coordinates of the
    /// untouched reference suffix. The endpoint must use the definitive clean
    /// parser/Green path for this shape.
    #[must_use]
    pub fn base_edit_precedes_reference_coverage(&self, base_edit: &Range<usize>) -> bool {
        self.references.as_ref().is_some_and(|references| {
            references.occurrence_count() != 0
                && u64::try_from(base_edit.start)
                    .is_ok_and(|start| start < references.first_source_byte_start())
        })
    }

    #[cfg(test)]
    fn checkpoint_storage_receipt_for_diagnostics(&self) -> CheckpointStorageReceipt {
        if let Some(compact) = &self.compact_checkpoints {
            return compact.receipt();
        }
        let mut retained_open_frames = 0_usize;
        let mut maximum_open_depth = 0_usize;
        let mut allocated_bytes = match &self.checkpoints {
            M11CheckpointStore::Contiguous(checkpoints) => checkpoints
                .capacity()
                .saturating_mul(std::mem::size_of::<M11BlockRestartCheckpoint>()),
            M11CheckpointStore::Paged { pages, .. } => {
                pages.values().fold(0_usize, |bytes, page| {
                    bytes.saturating_add(
                        page.len()
                            .saturating_mul(std::mem::size_of::<M11BlockRestartCheckpoint>()),
                    )
                })
            }
        };
        for checkpoint in self.checkpoints.iter() {
            let depth = checkpoint.open_kinds().len();
            retained_open_frames = retained_open_frames.saturating_add(depth);
            maximum_open_depth = maximum_open_depth.max(depth);
            allocated_bytes =
                allocated_bytes.saturating_add(checkpoint.heap_allocated_bytes_for_diagnostics());
        }
        if let Some(terminal) = &self.terminal_convergence {
            allocated_bytes =
                allocated_bytes.saturating_add(terminal.heap_allocated_bytes_for_diagnostics());
        }
        CheckpointStorageReceipt {
            checkpoints: self.checkpoints.len(),
            retained_open_frames,
            maximum_open_depth,
            allocated_bytes,
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
                    || !checkpoint_proves_reference_restart_cut(
                        checkpoint,
                        references,
                        base_edit.start,
                    )
            } {
                restart_index = restart_index.checked_sub(1).ok_or(
                    M11PersistentRecursiveGreenSessionError::InvalidState(
                        "sparse recursive-Green index has no reference-safe target restart",
                    ),
                )?;
            }
            let convergence_search_start = restart_index + 1;
            let convergence_offset =
                self.checkpoints
                    .partition_point_from(convergence_search_start, |checkpoint| {
                        (checkpoint.parser_physical().bytes() as usize) < base_edit.end
                            || (checkpoint.accepted_physical().bytes() as usize) < base_edit.end
                    });
            let mut convergence_index = convergence_search_start
                .checked_add(convergence_offset)
                .filter(|index| *index < self.checkpoints.len());
            // Deleting a newline can map an otherwise valid base checkpoint
            // into the middle of one target physical line. Such a point is an
            // exact unchanged suffix cut, but not a parser convergence cut.
            while let Some(index) = convergence_index {
                let checkpoint = &self.checkpoints[index];
                let parser = checkpoint.parser_physical();
                if parser.bytes() as usize == self.source.byte_len()
                    && parser.utf16() as usize == self.source.utf16_len()
                {
                    break;
                }
                let suffix = runtime.mint_exact_unchanged_suffix_witness(
                    self.source,
                    parser.bytes() as usize,
                    parser.utf16() as usize,
                )?;
                if target_lease
                    .is_physical_line_start(suffix.target_byte_start())
                    .map_err(|_| {
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "recursive-Green convergence cut is not a target source boundary",
                        )
                    })?
                {
                    break;
                }
                convergence_index = index
                    .checked_add(1)
                    .filter(|next| *next < self.checkpoints.len());
            }
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
            let live_projection = M11PersistentRecursiveGreenLiveProjection {
                prefix: green_prefix.as_ref().map(|prefix| {
                    M11PersistentRecursiveGreenProjectionRegion {
                        kind: M11PersistentRecursiveGreenProjectionRegionKind::Prefix,
                        base_start_byte: 0,
                        base_end_byte: prefix.byte_end(),
                        base_start_utf16: 0,
                        base_end_utf16: prefix.utf16_end(),
                        target_start_byte: 0,
                        target_end_byte: prefix.byte_end(),
                        target_start_utf16: 0,
                        target_end_utf16: prefix.utf16_end(),
                    }
                }),
                suffix: green_suffix.as_ref().map(|suffix| {
                    M11PersistentRecursiveGreenProjectionRegion {
                        kind: M11PersistentRecursiveGreenProjectionRegionKind::Suffix,
                        base_start_byte: suffix.base_byte_start(),
                        base_end_byte: self.source.byte_len(),
                        base_start_utf16: suffix.base_utf16_start(),
                        base_end_utf16: self.source.utf16_len(),
                        target_start_byte: suffix.target_byte_start(),
                        target_end_byte: target.byte_len(),
                        target_start_utf16: suffix.target_utf16_start(),
                        target_end_utf16: target.utf16_len(),
                    }
                }),
                suffix_ready: false,
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
            let reference_prefix = if references.occurrence_count() == 0 || reference_range_required
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
                live_projection,
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
                checkpoint_adoption: None,
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
        #[cfg(test)]
        if self.compact_probe {
            self.green_release_complete = true;
            self.references_release_complete = true;
        } else {
            self.green
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its structural root",
                ))?
                .begin_release(runtime)
                .map_err(M11BlockWriterError::from)?;
        }
        #[cfg(not(test))]
        self.green
            .as_mut()
            .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                "recursive-Green session omitted its structural root",
            ))?
            .begin_release(runtime)
            .map_err(M11BlockWriterError::from)?;
        #[cfg(test)]
        if !self.compact_probe {
            self.references
                .as_mut()
                .ok_or(M11PersistentRecursiveGreenSessionError::InvalidState(
                    "recursive-Green session omitted its reference root",
                ))?
                .begin_release(runtime)?;
        }
        #[cfg(not(test))]
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
    use flark_engine::{DocumentRuntimeConfig, ParserProfileId, SourceBoundaryAffinity};
    use std::time::Instant;

    fn capture_inline_facts_for_slice_differential(
        runtime: &mut DocumentRuntime,
        prepared: M11RecursiveGreenInlineLeafPreparation,
    ) -> (
        Vec<flark_engine::parser_internal::M11InlineProjectionFact>,
        Vec<flark_engine::parser_internal::M11InlineLinkValue>,
    ) {
        let profile =
            ParserProfileId::new(u64::from(SYNTAX_PROFILE_GFM_V1)).expect("GFM profile identity");
        let mut job =
            crate::M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_fact_capture(
                runtime,
                prepared.into_fence(),
                crate::M11ParserBinding::current(profile),
            )
            .expect("start inline fact capture");
        loop {
            let poll = job.poll(runtime, 4_096).expect("poll inline fact capture");
            if poll.status() == crate::M11InlineProjectionJobPollStatus::Complete {
                break;
            }
            assert!(poll.transitions() > 0, "inline fact capture must progress");
        }
        assert_eq!(job.projected_facts_are_authoritative(), Some(true));
        let facts = job.take_projected_facts().expect("captured inline facts");
        let links = job
            .take_projected_link_values()
            .expect("captured inline link values");
        job.begin_abort(runtime).expect("begin inline cleanup");
        while !job
            .poll_abort(runtime, 4_096)
            .expect("poll inline cleanup")
            .complete()
        {}
        (facts, links)
    }

    fn build_green_slice_from_primary_events(
        runtime: &mut DocumentRuntime,
        source_start_byte: usize,
        source_end_byte: usize,
        row_base: u64,
        events: &[M11RecursiveGreenEvent],
    ) -> flark_engine::parser_internal::M11RecursiveGreenSliceRoot {
        use flark_engine::parser_internal::{
            M11RecursiveGreenBuildStatus, M11RecursiveGreenClosedChild, M11RecursiveGreenSliceBuild,
        };

        let (document_frame, document_kind) = match events.first().copied() {
            Some(M11RecursiveGreenEvent::Enter { frame, kind }) => (frame, kind),
            _ => panic!("primary slice omitted its Document enter"),
        };
        let mut slice_build = M11RecursiveGreenSliceBuild::new_at(
            runtime,
            runtime.snapshot_current_source().expect("slice source"),
            source_start_byte,
            source_end_byte,
            row_base,
        )
        .expect("begin Green slice");
        for event in events.iter().copied() {
            slice_build.offer_event(event).expect("offer slice event");
            loop {
                match slice_build
                    .poll(runtime, 4_096)
                    .expect("poll slice event")
                    .status()
                {
                    M11RecursiveGreenBuildStatus::NeedsInput => break,
                    M11RecursiveGreenBuildStatus::Pending => {}
                    status => panic!("slice event terminated build as {status:?}"),
                }
            }
        }
        slice_build
            .offer_event(M11RecursiveGreenEvent::Exit {
                frame: document_frame,
                final_kind: document_kind,
                close: None,
                last_line_blank: false,
                child: M11RecursiveGreenClosedChild::default(),
            })
            .expect("offer synthetic slice envelope close");
        loop {
            match slice_build
                .poll(runtime, 4_096)
                .expect("poll slice envelope")
                .status()
            {
                M11RecursiveGreenBuildStatus::NeedsInput => break,
                M11RecursiveGreenBuildStatus::Pending => {}
                status => panic!("slice envelope terminated build as {status:?}"),
            }
        }
        slice_build.finish_input().expect("finish slice input");
        loop {
            if slice_build
                .poll(runtime, 4_096)
                .expect("finish Green slice")
                .status()
                == M11RecursiveGreenBuildStatus::Complete
            {
                break;
            }
        }
        slice_build.take_root().expect("Green slice root")
    }

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

    fn build_compact_probe(
        runtime: &mut DocumentRuntime,
        admitted_at: Instant,
    ) -> (
        M11PersistentRecursiveGreenSession,
        M11CompactProbeWriterReceipt,
        usize,
        usize,
        std::time::Duration,
    ) {
        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime.snapshot_current_source().expect("scanner lease"),
            runtime.snapshot_current_source().expect("writer lease"),
            1,
        )
        .expect("compact plan");
        let mut build = plan
            .begin_compact_probe(runtime)
            .expect("compact probe build");
        let mut first_32_rows = None;
        let mut driver_transitions = 0_u64;
        loop {
            let fuel = if first_32_rows.is_none() { 64 } else { 4_096 };
            let poll = build.poll(runtime, fuel).expect("poll compact probe");
            driver_transitions = driver_transitions
                .checked_add(u64::try_from(poll.transitions()).expect("transition count"))
                .expect("driver transition count");
            if first_32_rows.is_none()
                && build
                    .compact_probe_current_writer_receipt()
                    .is_some_and(|receipt| receipt.renderable_rows >= 32)
            {
                first_32_rows = Some(admitted_at.elapsed());
            }
            if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                let (mut receipt, boundaries, captures) = build
                    .take_compact_probe_receipt()
                    .expect("compact probe receipt");
                receipt.driver_transitions = driver_transitions;
                return (
                    build.take_session().expect("compact probe session"),
                    receipt,
                    boundaries,
                    captures,
                    first_32_rows.unwrap_or_else(|| admitted_at.elapsed()),
                );
            }
        }
    }

    #[test]
    fn compact_probe_streams_exact_structure_without_green_or_event_journal() {
        const SOURCE: &str = "# Heading\n\n[alpha]: /one \"title\"\n[αλφα]: /δ \"τίτλος\"\n\nParagraph with **strong**, `code`, [alpha], and [αλφα].\n\n- one\n- two\n\n> quote\n\n```dart\nvoid main() {}\n```\n";
        let admitted_at = Instant::now();
        let mut runtime = DocumentRuntime::new(SOURCE, DocumentRuntimeConfig::default())
            .expect("compact smoke runtime");
        let (mut session, receipt, boundaries, captures, first_32_rows) =
            build_compact_probe(&mut runtime, admitted_at);

        assert_eq!(receipt.source_bytes as usize, SOURCE.len());
        assert_eq!(receipt.source_utf16 as usize, SOURCE.encode_utf16().count());
        assert!(receipt.high_level_events > 0);
        assert!(receipt.renderable_rows >= 6);
        assert!(receipt.packed_events >= receipt.high_level_events);
        assert!(receipt.logical_bytes > 0);
        assert!(receipt.source_bytes_read <= (SOURCE.len() as u64).saturating_mul(8));
        assert!(receipt.maximum_reference_events > 0);
        assert_eq!(receipt.reference_occurrences, 2);
        assert_eq!(receipt.reference_winners, 2);
        assert!(receipt.logical_bytes < receipt.source_bytes);
        assert!(boundaries > 1);
        assert!(captures <= SOURCE.len().div_ceil(CHECKPOINT_STRIDE_BYTES as usize) + 2);
        assert!(session.green.is_none());
        assert!(session.checkpoint_count() <= captures);
        session
            .compact_checkpoints
            .as_ref()
            .expect("compact restart pages")
            .validate_metadata_and_durable_samples()
            .expect("root-independent durable restart samples");
        assert!(first_32_rows <= admitted_at.elapsed());

        release_session(&mut runtime, &mut session);
        close_runtime(&mut runtime);
    }

    #[test]
    fn compact_probe_captures_one_bounded_primary_stream_slice() {
        use flark_engine::parser_internal::M11RecursiveGreenRowQueryLimits;

        let source = if let Some(target) = std::env::var("FLARK_FIRST_SLICE_TARGET_BYTES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            repeat_ascii_exact(
                "",
                "Paragraph has **strong**, _emphasis_, and a [direct link](https://example.invalid/).\n\n",
                target,
            )
        } else {
            (0..96)
                .map(|index| {
                    format!(
                        "Paragraph {index} has **strong**, _emphasis_, and a [direct link](https://example.invalid/{index}).\n\n"
                    )
                })
                .collect::<String>()
        };
        let admitted_at = Instant::now();
        let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("first-slice runtime");
        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime.snapshot_current_source().expect("scanner lease"),
            runtime.snapshot_current_source().expect("writer lease"),
            1,
        )
        .expect("first-slice plan");
        let mut build = plan
            .begin_compact_probe(&mut runtime)
            .expect("first-slice compact build");
        let slice = loop {
            let poll = build.poll(&mut runtime, 64).expect("poll first slice");
            if let Some(slice) = build.take_compact_probe_first_slice() {
                break slice;
            }
            assert_ne!(
                poll.status(),
                M11PersistentRecursiveGreenBuildStatus::Complete,
                "ordinary source must publish its first slice before EOF"
            );
        };
        let captured_at = admitted_at.elapsed();

        assert!(slice.physical.bytes() < source.len() as u64);
        assert!(slice.physical.bytes() <= 64 * 1024);
        assert!(slice.events.len() <= 8 * 1024);
        assert!(matches!(
            slice.events.first(),
            Some(M11RecursiveGreenEvent::Enter { .. })
        ));
        assert!(
            slice
                .events
                .iter()
                .filter(
                    |event| matches!(event, M11RecursiveGreenEvent::Exit { final_kind, .. }
                    if matches!(final_kind.get(), 5 | 6 | 7 | 8 | 12 | 13))
                )
                .count()
                >= 32
        );
        let slice_event_count = slice.events.len();

        let source_end = usize::try_from(slice.physical.bytes()).expect("slice end");
        let mut slice_root =
            build_green_slice_from_primary_events(&mut runtime, 0, source_end, 0, &slice.events);
        let limits = M11RecursiveGreenRowQueryLimits::new(64, 4_096, 32_768, 64, 32_768)
            .expect("slice query limits");
        let slice_rows = slice_root
            .locate_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(0, 0, flark_engine::SourceBoundaryAffinity::After),
                source_end as u64,
                limits,
            )
            .expect("query Green slice")
            .rows()
            .to_vec();
        let slice_inline = slice_rows
            .iter()
            .map(|slice_row| {
                let slice_point = M11RecursiveGreenPoint::new(
                    usize::try_from(slice_row.physical_range().start).expect("slice row byte"),
                    usize::try_from(slice_row.physical_utf16_range().start)
                        .expect("slice row UTF-16"),
                    SourceBoundaryAffinity::After,
                );
                let prepared = crate::prepare_m11_recursive_green_slice_inline_leaf(
                    &runtime,
                    &slice_root,
                    slice_point,
                )
                .expect("prepare slice inline leaf");
                capture_inline_facts_for_slice_differential(&mut runtime, prepared)
            })
            .collect::<Vec<_>>();
        let engine_ready_at = admitted_at.elapsed();
        eprintln!(
            "primary first slice: captured={captured_at:?} engine_ready={engine_ready_at:?} source_bytes={source_end} events={slice_event_count} rows={}",
            slice_rows.len()
        );

        loop {
            if build
                .poll(&mut runtime, 4_096)
                .expect("finish compact source")
                .status()
                == M11PersistentRecursiveGreenBuildStatus::Complete
            {
                break;
            }
        }
        let mut session = build.take_session().expect("compact session");
        release_session(&mut runtime, &mut session);

        let mut complete = build_session(&mut runtime);
        let complete_rows = complete
            .query_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(0, 0, flark_engine::SourceBoundaryAffinity::After),
                source_end as u64,
                limits,
            )
            .expect("query complete Green")
            .rows()
            .to_vec();
        assert_eq!(slice_rows.len(), complete_rows.len());
        for ((slice_row, complete_row), expected_inline) in
            slice_rows.iter().zip(&complete_rows).zip(&slice_inline)
        {
            assert_eq!(slice_row.ordinal(), complete_row.ordinal());
            assert_eq!(slice_row.frame(), complete_row.frame());
            assert_eq!(slice_row.kind(), complete_row.kind());
            assert_eq!(slice_row.physical_range(), complete_row.physical_range());
            assert_eq!(
                slice_row.physical_utf16_range(),
                complete_row.physical_utf16_range()
            );
            assert_eq!(slice_row.edit_capability(), complete_row.edit_capability());
            assert_eq!(slice_row.editable_range(), complete_row.editable_range());
            assert_eq!(
                slice_row.editable_utf16_range(),
                complete_row.editable_utf16_range()
            );
            assert_eq!(
                slice_row.editable_segments(),
                complete_row.editable_segments()
            );
            assert_eq!(slice_row.path().len(), complete_row.path().len());
            assert_eq!(
                &slice_row.path()[1..],
                &complete_row.path()[1..],
                "only the deliberately shorter synthetic Document envelope may differ"
            );

            let slice_point = M11RecursiveGreenPoint::new(
                usize::try_from(slice_row.physical_range().start).expect("slice row byte"),
                usize::try_from(slice_row.physical_utf16_range().start).expect("slice row UTF-16"),
                SourceBoundaryAffinity::After,
            );
            let complete_inline = complete
                .prepare_inline_leaf(&runtime, slice_point)
                .expect("prepare complete inline leaf");
            let actual_inline =
                capture_inline_facts_for_slice_differential(&mut runtime, complete_inline);
            assert_eq!(
                expected_inline, &actual_inline,
                "bounded primary-stream slice must yield the eventual inline facts"
            );
        }

        const REBASED_ROW: u64 = 73;
        let prefix = "outside 🦀 prefix\n\n";
        let prefix_bytes = prefix.len();
        let prefix_utf16 = prefix.encode_utf16().count();
        let rebased_source = format!("{prefix}{}trailing source", &source[..source_end]);
        let rebased_end = prefix_bytes + source_end;
        let mut rebased_runtime =
            DocumentRuntime::new(&rebased_source, DocumentRuntimeConfig::default())
                .expect("rebased slice runtime");
        let mut rebased_root = build_green_slice_from_primary_events(
            &mut rebased_runtime,
            prefix_bytes,
            rebased_end,
            REBASED_ROW,
            &slice.events,
        );
        assert_eq!(
            rebased_root.source_range(),
            prefix_bytes as u64..rebased_end as u64
        );
        assert_eq!(
            rebased_root.source_utf16_range(),
            prefix_utf16 as u64
                ..(prefix_utf16 + source[..source_end].encode_utf16().count()) as u64
        );
        assert_eq!(rebased_root.row_base(), REBASED_ROW);
        let rebased_rows = rebased_root
            .locate_renderable_rows(
                &rebased_runtime,
                M11RecursiveGreenPoint::new(
                    prefix_bytes,
                    prefix_utf16,
                    SourceBoundaryAffinity::After,
                ),
                rebased_end as u64,
                limits,
            )
            .expect("query rebased Green slice")
            .rows()
            .to_vec();
        assert_eq!(rebased_rows.len(), slice_rows.len());
        for ((rebased_row, slice_row), expected_inline) in
            rebased_rows.iter().zip(&slice_rows).zip(&slice_inline)
        {
            assert_eq!(rebased_row.ordinal(), slice_row.ordinal() + REBASED_ROW);
            assert_eq!(rebased_row.frame(), slice_row.frame());
            assert_eq!(rebased_row.kind(), slice_row.kind());
            assert_eq!(
                rebased_row.physical_range(),
                (slice_row.physical_range().start + prefix_bytes as u64
                    ..slice_row.physical_range().end + prefix_bytes as u64)
            );
            assert_eq!(
                rebased_row.physical_utf16_range(),
                (slice_row.physical_utf16_range().start + prefix_utf16 as u64
                    ..slice_row.physical_utf16_range().end + prefix_utf16 as u64)
            );
            let point = M11RecursiveGreenPoint::new(
                usize::try_from(rebased_row.physical_range().start).expect("rebased row byte"),
                usize::try_from(rebased_row.physical_utf16_range().start)
                    .expect("rebased row UTF-16"),
                SourceBoundaryAffinity::After,
            );
            let prepared = crate::prepare_m11_recursive_green_slice_inline_leaf(
                &rebased_runtime,
                &rebased_root,
                point,
            )
            .expect("prepare rebased inline leaf");
            assert_eq!(
                expected_inline,
                &capture_inline_facts_for_slice_differential(&mut rebased_runtime, prepared),
                "rebasing must not change parser-authored inline semantics"
            );
        }

        release_session(&mut runtime, &mut complete);
        slice_root
            .begin_release(&mut runtime)
            .expect("begin slice release");
        while !slice_root
            .poll_release(&mut runtime, 256)
            .expect("poll slice release")
            .complete()
        {}
        rebased_root
            .begin_release(&mut rebased_runtime)
            .expect("begin rebased slice release");
        while !rebased_root
            .poll_release(&mut rebased_runtime, 256)
            .expect("poll rebased slice release")
            .complete()
        {}
        close_runtime(&mut rebased_runtime);
        close_runtime(&mut runtime);
    }

    #[test]
    fn compact_probe_abandons_unbounded_first_slice() {
        let source = format!(
            "~~~markdown\n{}",
            "literal text remains fenced\n".repeat(4_096)
        );
        let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("over-cap runtime");
        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime.snapshot_current_source().expect("scanner lease"),
            runtime.snapshot_current_source().expect("writer lease"),
            1,
        )
        .expect("over-cap plan");
        let mut build = plan
            .begin_compact_probe(&mut runtime)
            .expect("over-cap compact build");
        let mut observed_cap = false;
        loop {
            let poll = build.poll(&mut runtime, 256).expect("poll over-cap source");
            observed_cap |= build.compact_probe_first_slice_over_cap();
            assert!(build.take_compact_probe_first_slice().is_none());
            if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                break;
            }
        }
        assert!(
            observed_cap,
            "spanning construct must use the bounded fallback"
        );
        let mut session = build.take_session().expect("compact session");
        release_session(&mut runtime, &mut session);
        close_runtime(&mut runtime);
    }

    fn repeat_ascii_exact(prefix: &str, cycle: &str, target_bytes: usize) -> String {
        assert!(prefix.is_ascii() && cycle.is_ascii() && !cycle.is_empty());
        assert!(prefix.len() <= target_bytes);
        let mut output = String::with_capacity(target_bytes);
        output.push_str(prefix);
        let remaining = target_bytes - prefix.len();
        output.push_str(&cycle.repeat(remaining / cycle.len()));
        output.push_str(&cycle[..remaining % cycle.len()]);
        output
    }

    fn indexed_ascii_exact(record: &str, target_bytes: usize) -> String {
        assert!(record.is_ascii() && record.contains("{index}"));
        let mut output = String::with_capacity(target_bytes);
        let mut index = 0_u64;
        while output.len() < target_bytes {
            let row = record.replace("{index}", &format!("{index:08}"));
            let remaining = target_bytes - output.len();
            output.push_str(&row[..remaining.min(row.len())]);
            index += 1;
        }
        output
    }

    #[test]
    #[ignore = "gate-one release receipt"]
    fn compact_probe_gate_one_structural_receipt() {
        const MIB: usize = 1024 * 1024;
        let target_override = std::env::var("FLARK_COMPACT_TARGET_BYTES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok());
        let ordinary_target = target_override.unwrap_or(10 * MIB);
        let stress_target = target_override.unwrap_or(5 * MIB);
        let shapes = [
            (
                "ordinary-10mib",
                repeat_ascii_exact(
                    "",
                    "Ordinary prose opens with a clear sentence and a small **bold** run.\nIt continues with _emphasis_, `code`, and a direct [link](https://example.invalid/).\n\n",
                    ordinary_target,
                ),
            ),
            (
                "tiny-block-5mib",
                indexed_ascii_exact("p-{index}\n\n", stress_target),
            ),
            (
                "nested-5mib",
                repeat_ascii_exact(
                    "",
                    "> - outer item\n>   1. inner item with **bold**\n>      - third level\n>\n> lazy continuation\n\n",
                    stress_target,
                ),
            ),
            (
                "giant-paragraph-5mib",
                repeat_ascii_exact(
                    "",
                    "One paragraph continues across this physical line with words and **markup**.\n",
                    stress_target,
                ),
            ),
            (
                "giant-line-5mib",
                repeat_ascii_exact(
                    "",
                    "one-giant-line-with-words-and-**markup**-",
                    stress_target,
                ),
            ),
            (
                "open-fence-5mib",
                repeat_ascii_exact(
                    "~~~markdown\n",
                    "literal **marker** and [reference] text remains inside the open fence\n",
                    stress_target,
                ),
            ),
            (
                "delimiter-dense-5mib",
                repeat_ascii_exact(
                    "",
                    "***strong-em*** ~~strike~~ `code * literal` [label](https://example.invalid/a_(b)) ![alt](img.png)  \\\nnext\\*line\\_ []() <tag> &amp;\n\n",
                    stress_target,
                ),
            ),
            (
                "tables-tasks-5mib",
                repeat_ascii_exact(
                    "",
                    "| Name | Value |\n| --- | ---: |\n| alpha | 1 |\n| beta | 2 |\n\n- [x] complete\n- [ ] pending\n\n",
                    stress_target,
                ),
            ),
            (
                "many-references-5mib",
                indexed_ascii_exact(
                    "Use [ref-{index}] here.\n\n[ref-{index}]: https://example.invalid/{index}\n\n",
                    stress_target,
                ),
            ),
            (
                "html-type-1-5mib",
                repeat_ascii_exact(
                    "",
                    "<script>\nconst marker = '<!-- not a terminator -->';\n</script>\n\n",
                    stress_target,
                ),
            ),
            (
                "html-type-2-5mib",
                repeat_ascii_exact(
                    "",
                    "<!--\ncomment with **literal** markdown\n-->\n\n",
                    stress_target,
                ),
            ),
            (
                "html-type-3-5mib",
                repeat_ascii_exact(
                    "",
                    "<?processing\nvalue='**literal**'\n?>\n\n",
                    stress_target,
                ),
            ),
            (
                "html-type-4-5mib",
                repeat_ascii_exact("", "<!DOCTYPE html PUBLIC 'flark'>\n\n", stress_target),
            ),
            (
                "html-type-5-5mib",
                repeat_ascii_exact(
                    "",
                    "<![CDATA[\n<literal>**markdown**</literal>\n]]>\n\n",
                    stress_target,
                ),
            ),
            (
                "html-type-6-5mib",
                repeat_ascii_exact(
                    "",
                    "<div>\nblock tag continues until the blank line\n\n",
                    stress_target,
                ),
            ),
            (
                "html-type-7-5mib",
                repeat_ascii_exact(
                    "",
                    "<flark-panel data-mode='live'>\ncustom tag continues until the blank line\n\n",
                    stress_target,
                ),
            ),
        ];

        let shape_filter = std::env::var("FLARK_COMPACT_SHAPE").ok();
        for (name, source) in shapes {
            if shape_filter.as_deref().is_some_and(|filter| filter != name) {
                continue;
            }
            let started = Instant::now();
            let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
                .unwrap_or_else(|error| panic!("{name} runtime: {error}"));
            let (mut session, receipt, boundaries, captures, first_32_rows) =
                build_compact_probe(&mut runtime, started);
            let elapsed = started.elapsed();
            let storage = session.checkpoint_storage_receipt_for_diagnostics();
            session
                .compact_checkpoints
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: compact restart pages"))
                .validate_metadata_and_durable_samples()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            println!(
                "COMPACT_GATE_ONE name={name} bytes={} first32_us={} elapsed_us={} transitions={} high_events={} packed_events={} rows={} boundaries={} captures={} retained={} checkpoint_bytes={} open_frames={} max_depth={} source_reads={} max_reference_events={} max_reference_window_bytes={} references={} winners={} reference_bytes={} label_bytes={} reference_phases={:?}",
                source.len(),
                first_32_rows.as_micros(),
                elapsed.as_micros(),
                receipt.driver_transitions,
                receipt.high_level_events,
                receipt.packed_events,
                receipt.renderable_rows,
                boundaries,
                captures,
                storage.checkpoints(),
                storage.allocated_bytes(),
                storage.retained_open_frames(),
                storage.maximum_open_depth(),
                receipt.source_bytes_read,
                receipt.maximum_reference_events,
                receipt.maximum_reference_allocated_bytes,
                receipt.reference_occurrences,
                receipt.reference_winners,
                receipt.reference_allocated_bytes,
                receipt.reference_normalized_label_bytes,
                receipt.reference_phase_transitions,
            );
            assert_eq!(receipt.source_bytes as usize, source.len(), "{name}");
            assert_eq!(
                receipt.source_utf16 as usize,
                source.encode_utf16().count(),
                "{name}"
            );
            assert!(
                captures <= source.len().div_ceil(CHECKPOINT_STRIDE_BYTES as usize) + 2,
                "{name}: captures={captures}"
            );
            assert!(storage.checkpoints() <= captures, "{name}");
            assert!(
                storage
                    .allocated_bytes()
                    .saturating_add(receipt.reference_allocated_bytes)
                    <= 12 * 1024 * 1024,
                "{name}: checkpoint_bytes={} reference_bytes={}",
                storage.allocated_bytes(),
                receipt.reference_allocated_bytes,
            );
            assert!(
                receipt.maximum_reference_allocated_bytes <= 8 * 1024 * 1024,
                "{name}: reference_window_bytes={}",
                receipt.maximum_reference_allocated_bytes,
            );
            release_session(&mut runtime, &mut session);
            close_runtime(&mut runtime);
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
        session
            .begin_release(runtime)
            .expect("begin session release");
        while !session
            .poll_release(runtime, 64)
            .expect("poll session release")
        {}
    }

    #[test]
    fn dynamic_reference_replacement_keeps_the_prefix_before_a_remainder_restart() {
        const BASE: &str = "[base]: /base\n!x]: /new\nsee [x]\n";
        let edit_start = BASE.find("!x]").expect("new definition marker");

        let mut runtime = DocumentRuntime::new(BASE, DocumentRuntimeConfig::default())
            .expect("prefix-boundary runtime");
        let session = build_session(&mut runtime);
        let references = session.references.as_ref().expect("base reference root");
        let remainder = session
            .checkpoints
            .iter()
            .find(|checkpoint| {
                let cut = checkpoint.parser_physical();
                cut.bytes() == references.last_source_byte_end()
                    && cut.utf16() == references.last_source_utf16_end()
                    && checkpoint
                        .open_kinds()
                        .any(|kind| matches!(kind, BlockKind::Paragraph))
            })
            .expect("leading-reference remainder checkpoint");
        assert_eq!(remainder.parser_physical().bytes() as usize, edit_start);
        assert!(!checkpoint_proves_reference_restart_cut(
            remainder, references, edit_start,
        ));
        assert!(checkpoint_proves_reference_restart_cut(
            remainder,
            references,
            edit_start + 1,
        ));
        let base_source = session.source();
        runtime
            .apply_edit(base_source, edit_start..edit_start + 1, "[")
            .expect("activate second definition");
        let mut update = finish_local_adoption(&mut runtime, session, edit_start..edit_start + 1);
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
    fn reference_rewrite_prunes_checkpoints_inside_a_multiline_occurrence() {
        let destination_gap = " ".repeat(CHECKPOINT_STRIDE_BYTES as usize * 2 + 256);
        let base = format!("before\n\n[x]:{destination_gap}/a\n  \"title\"\n\nafter [x]\n");
        let definition_start = base.find("[x]:").expect("definition start");
        let title_start = base.find("  \"title\"").expect("title start");

        let mut runtime = DocumentRuntime::new(&base, DocumentRuntimeConfig::default())
            .expect("multiline reference runtime");
        let session = build_session(&mut runtime);
        let references = session.references.as_ref().expect("base reference root");
        assert!(session.checkpoints.iter().all(|checkpoint| {
            let cut = checkpoint.parser_physical().bytes() as usize;
            !(definition_start < cut
                && cut <= title_start
                && !checkpoint_proves_reference_occurrence_cut(checkpoint, references))
        }), "a reference rewrite must not retain stale Green authority inside its multiline occurrence");

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
        let whitespace = " ".repeat(CHECKPOINT_STRIDE_BYTES as usize * 2 + 256);
        let base = format!("[x]:{whitespace}\n<u\n\nafter\n");
        let edit_start = base.find("<u").expect("destination marker");

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
            match adoption
                .poll(&mut runtime, 64)
                .expect("poll fallback")
                .status()
            {
                M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => break,
                status => panic!("expected clean fallback, got {status:?}"),
            }
        }
        adoption
            .begin_cancel(&mut runtime)
            .expect("begin fallback cancellation");
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
        assert_eq!(
            target.reference_occurrence_count(),
            clean.reference_occurrence_count()
        );
        assert_eq!(
            winner(&runtime, &target, b"a"),
            winner(&clean_runtime, &clean, b"a")
        );
        assert_eq!(
            winner(&runtime, &target, b"b"),
            winner(&clean_runtime, &clean, b"b")
        );

        base.begin_release(&mut runtime).expect("release base");
        while !base
            .poll_release(&mut runtime, 64)
            .expect("poll base release")
        {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        clean
            .begin_release(&mut clean_runtime)
            .expect("release clean");
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
        let mut update = finish_local_adoption(&mut runtime, session, 0..FIRST_DEFINITION_END);
        let mut base = update.take_base().expect("superseded base");
        let mut target = update.take_target().expect("incremental target");

        assert_eq!(target.reference_occurrence_count(), 1);
        assert_eq!(winner(&runtime, &target, b"a"), Some(0));

        let mut clean_runtime = DocumentRuntime::new(TARGET, DocumentRuntimeConfig::default())
            .expect("clean reference deletion runtime");
        let mut clean = build_session(&mut clean_runtime);
        assert_eq!(
            target.reference_occurrence_count(),
            clean.reference_occurrence_count()
        );
        assert_eq!(
            winner(&runtime, &target, b"a"),
            winner(&clean_runtime, &clean, b"a")
        );

        base.begin_release(&mut runtime).expect("release base");
        while !base
            .poll_release(&mut runtime, 64)
            .expect("poll base release")
        {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        clean
            .begin_release(&mut clean_runtime)
            .expect("release clean");
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
        assert_eq!(
            target.reference_occurrence_count(),
            clean.reference_occurrence_count()
        );
        assert_eq!(
            winner(&runtime, &target, b"a"),
            winner(&clean_runtime, &clean, b"a")
        );

        base.begin_release(&mut runtime).expect("release base");
        while !base
            .poll_release(&mut runtime, 64)
            .expect("poll base release")
        {}
        target.begin_release(&mut runtime).expect("release target");
        while !target
            .poll_release(&mut runtime, 64)
            .expect("poll target release")
        {}
        clean
            .begin_release(&mut clean_runtime)
            .expect("release clean");
        while !clean
            .poll_release(&mut clean_runtime, 64)
            .expect("poll clean release")
        {}
        close_runtime(&mut runtime);
        close_runtime(&mut clean_runtime);
    }

    fn checkpoint_rebase_work_for_lines(
        lines: usize,
    ) -> (usize, M11PersistentRecursiveGreenAdoptionWork) {
        use std::fmt::Write as _;

        let mut source = String::new();
        let mut edit = 0..0;
        for ordinal in 0..lines {
            let line_start = source.len();
            writeln!(
                &mut source,
                "paragraph {ordinal:05} has an editable word and enough padding to exercise sparse checkpoints.\n"
            )
            .expect("checkpoint fixture write");
            if ordinal == lines / 2 {
                let offset = source[line_start..]
                    .find("editable")
                    .expect("editable word");
                edit = line_start + offset..line_start + offset + "editable".len();
            }
        }

        let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("checkpoint-rebase runtime");
        let session = build_session(&mut runtime);
        let base_checkpoint_count = session.checkpoint_count();
        let base_source = session.source();
        runtime
            .apply_edit(base_source, edit.clone(), "EDITABLE")
            .expect("checkpoint-rebase edit");
        let target = runtime.snapshot_current_source().expect("target lease");
        let mut adoption = session
            .begin_local_adoption(&runtime, target, edit)
            .unwrap_or_else(|failure| panic!("local adoption start: {}", failure.error()));
        let mut checkpoint_records_processed = 0;
        let mut maximum_checkpoint_records_per_transition = 0;
        loop {
            let poll = adoption
                .poll(&mut runtime, 11)
                .expect("poll checkpoint-rebase adoption");
            assert!(poll.checkpoint_records_processed() <= poll.transitions());
            assert!(poll.maximum_checkpoint_records_per_transition() <= 1);
            checkpoint_records_processed += poll.checkpoint_records_processed();
            maximum_checkpoint_records_per_transition = maximum_checkpoint_records_per_transition
                .max(poll.maximum_checkpoint_records_per_transition());
            match poll.status() {
                M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
                status => panic!("checkpoint-rebase adoption failed: {status:?}"),
            }
        }

        let mut update = adoption.take_update().expect("checkpoint-rebase update");
        let work = update.work();
        assert_eq!(
            work.checkpoint_records_processed(),
            checkpoint_records_processed
        );
        assert_eq!(
            work.maximum_checkpoint_records_per_transition(),
            maximum_checkpoint_records_per_transition
        );
        let mut base = update.take_base().expect("checkpoint-rebase base");
        let mut target = update.take_target().expect("checkpoint-rebase target");
        assert_eq!(
            work.checkpoint_records_processed(),
            target.checkpoint_count() + 1,
            "every target checkpoint plus terminal authority is one explicit work record",
        );
        release_session(&mut runtime, &mut base);
        release_session(&mut runtime, &mut target);
        close_runtime(&mut runtime);
        (base_checkpoint_count, work)
    }

    #[test]
    fn checkpoint_rebase_work_per_transition_is_independent_of_document_checkpoint_count() {
        let (small_checkpoints, small) = checkpoint_rebase_work_for_lines(512);
        let (medium_checkpoints, medium) = checkpoint_rebase_work_for_lines(8_192);

        assert!(medium_checkpoints > small_checkpoints * 8);
        assert!(medium.checkpoint_records_processed() > small.checkpoint_records_processed() * 8);
        assert_eq!(small.maximum_checkpoint_records_per_transition(), 1);
        assert_eq!(medium.maximum_checkpoint_records_per_transition(), 1);
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

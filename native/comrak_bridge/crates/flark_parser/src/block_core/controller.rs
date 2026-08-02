// SPDX-License-Identifier: BSD-2-Clause
// SPDX-FileCopyrightText: 2017-2026 Comrak contributors
// SPDX-FileCopyrightText: 2026 Flark contributors
//
// Thin production adapter over the mechanically promoted direct controller in
// `donor/core`. The pinned donor commit is
// 172c2ee7d2c5c262a28be3e407aadf705daea2b7. The complete license notice is in
// `vendor/comrak/COPYING`; the extraction note is in `donor/core/NOTICE.md`.

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
};

use flark_block_core_donor as donor;
use flark_engine::{LineEnding as SourceLineEnding, SourceBoundaryAffinity, SourceSnapshotLease};

use super::{
    BlockCommand, BlockKind, BulletMarker, ClosedChild, CoveragePart, FenceCharacter,
    FencedCodeBoundary, FencedCodeCloseFacts, FencedCodeFacts, FinalFacts, HeadingFacts,
    HeadingStyle, HtmlBlockFacts, HtmlBlockType, ItemFacts, LineEnding, LineSourcePosition,
    LineSourceRange, ListDelimiter, ListFacts, LogicalAction, ParagraphOutcome, PartialTab,
    SourceMetric, StackOwner, TerminatorResolution,
};
use crate::{
    M11ExactController, M11LineEnding, M11PhysicalLineFacts, M11SourceLinePollReceipt,
    M11SourceLinePollStatus, M11SourceLineSource, SourceLineIdentity,
};

static CONTROLLER_IDS: AtomicU64 = AtomicU64::new(1);

/// Aggregate retained-source ceiling of the promoted donor and this adapter's
/// byte-to-UTF-16 coordinate prefix. It is independent of physical line size.
pub const M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES: usize =
    donor::DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES * 3;

/// Fixed generated-DFA overhead beyond caller fuel.
///
/// Every source-backed poll guarantees
/// `lexical_work_units <= fuel + M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK`. The
/// additive term is Comrak's generated ATX `YYMAXFILL - 1`; it is scanner work
/// only and never authorizes an extra [`M11SourceLineSource::read_byte`].
pub const M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK: usize = donor::DIRECT_SOURCE_LINE_MAX_LEXICAL_SLACK;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11DirectBlockPollStatus {
    Pending,
    CommandReady,
    ExternalWorkReady,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11DirectBlockPollReceipt {
    pub transitions: usize,
    pub status: M11DirectBlockPollStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11DirectBlockUnsupported {
    SyntaxProfile,
    LineTooLarge,
    EmbeddedLineEnding,
    TabOrNul,
    BlockKind,
    AggregateContent,
    SegmentedLine,
    ReferenceExternalWork,
    CoordinateOutsideRetainedWindow,
}

/// Deferred physical-source shape at one acknowledged parser line boundary.
///
/// The value is an observation used to cross-check the writer continuation;
/// it is not independently resumable authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11DirectBlockDeferredRole {
    None,
    Terminator,
    BlankGap { floor_depth: Option<usize> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11DirectBlockError {
    Unsupported(M11DirectBlockUnsupported),
    InvalidUtf8Boundary,
    InvalidSourceFacts,
    Invariant(&'static str),
}

#[derive(Debug, Eq, PartialEq)]
pub enum M11DirectBlockControllerError<SourceError> {
    Controller(M11DirectBlockError),
    ZeroFuel,
    WrongSource,
    Source(SourceError),
    InvalidSourceByte { absolute_offset: usize },
    InvalidUtf8 { absolute_offset: usize },
    EmbeddedLineEnding { absolute_offset: usize },
    SourceBudgetContractViolated,
    ScannerInvariant,
    PollAfterComplete,
    PollAfterFailure,
}

/// Opaque, consuming authority for one in-flight source-backed physical line.
pub struct M11DirectSourceLineAdmission {
    controller_id: u64,
    identity: SourceLineIdentity,
    work: donor::DirectSourceLineWork<SourceLineIdentity>,
    coordinate_prefix: Vec<u8>,
    staged_source: VecDeque<u8>,
    source_high_water: usize,
    donor_high_water: usize,
}

/// Opaque, consuming direct-parser restart authority for one line boundary.
///
/// Source revision, Green cut, and writer open-path authority deliberately do
/// not live here.  [`M11BlockWriter`](super::M11BlockWriter) joins those facts
/// before this parser-only value may be selected for a production restart.
#[must_use = "direct-parser restart authority must be joined or discarded"]
pub struct M11DirectBlockRestart {
    grammar: donor::DirectGrammarContinuation,
    output: donor::DirectRestartOutput,
    line_ordinal: u64,
    last_line_length: u64,
    open_kinds: Box<[BlockKind]>,
    deferred: M11DirectBlockDeferredRole,
    restart_join: Option<u64>,
}

/// Donor-certified semantic continuation at the internal cut after leading
/// reference definitions and before their visible Paragraph remainder.
/// Physical cursor and writer/Green authority are joined separately.
pub(crate) struct M11DirectLeadingReferenceRemainderContinuation {
    donor: donor::DirectLeadingReferenceRemainderContinuation,
    restart_join: Option<u64>,
    cursor: Option<(u64, u64)>,
}

impl M11DirectLeadingReferenceRemainderContinuation {
    pub(super) fn bind_authenticated_source_cut(
        &mut self,
        lease: &SourceSnapshotLease,
        cut: SourceMetric,
    ) -> Result<(), M11DirectBlockError> {
        let byte_cut = usize::try_from(cut.bytes())
            .map_err(|_| M11DirectBlockError::Invariant("remainder byte cut fits usize"))?;
        let utf16_cut = usize::try_from(cut.utf16())
            .map_err(|_| M11DirectBlockError::Invariant("remainder UTF-16 cut fits usize"))?;
        if self.cursor.is_some()
            || lease.utf16_offset_for_byte(byte_cut).map_err(|_| {
                M11DirectBlockError::Invariant("remainder cut is a source scalar boundary")
            })? != utf16_cut
            || !lease.is_physical_line_start(byte_cut).map_err(|_| {
                M11DirectBlockError::Invariant("remainder cut is inside current source")
            })?
        {
            return Err(M11DirectBlockError::Invariant(
                "remainder cut is one exact physical-line boundary",
            ));
        }
        let previous = lease
            .locate_physical_line(byte_cut, SourceBoundaryAffinity::Before)
            .map_err(|_| M11DirectBlockError::Invariant("remainder predecessor line exists"))?
            .ok_or(M11DirectBlockError::Invariant(
                "remainder follows at least one definition line",
            ))?;
        let ending_bytes = match previous.ending() {
            SourceLineEnding::CrLf => 2,
            SourceLineEnding::Lf | SourceLineEnding::Cr => 1,
            SourceLineEnding::Eof => 0,
        };
        let next_ordinal =
            previous
                .ordinal()
                .checked_add(1)
                .ok_or(M11DirectBlockError::Invariant(
                    "remainder ordinal does not overflow",
                ))?;
        let last_line_length = previous
            .byte_range()
            .len()
            .checked_sub(ending_bytes)
            .ok_or(M11DirectBlockError::Invariant(
                "remainder predecessor terminator is inside its line",
            ))?;
        self.cursor = Some((
            u64::try_from(next_ordinal)
                .map_err(|_| M11DirectBlockError::Invariant("remainder ordinal fits u64"))?,
            u64::try_from(last_line_length)
                .map_err(|_| M11DirectBlockError::Invariant("remainder line length fits u64"))?,
        ));
        Ok(())
    }

    pub(super) fn into_restart(self) -> Result<M11DirectBlockRestart, M11DirectBlockError> {
        let (line_ordinal, last_line_length) = self.cursor.ok_or(
            M11DirectBlockError::Invariant("remainder continuation has its source cursor"),
        )?;
        let (grammar, output) = self.donor.into_restart_parts();
        Ok(M11DirectBlockRestart {
            grammar,
            output,
            line_ordinal,
            last_line_length,
            open_kinds: vec![BlockKind::Document, BlockKind::Paragraph].into_boxed_slice(),
            deferred: M11DirectBlockDeferredRole::None,
            restart_join: self.restart_join,
        })
    }
}

/// Crate-private parser-state replica scoped to one adoption transaction.
///
/// Keeping this separate from `M11DirectBlockRestart` avoids making restart
/// authority publicly cloneable while an exact adoption keeps its base
/// checkpoint intact.
pub(crate) struct M11DirectBlockRestartTransactionReplica {
    transaction_id: u64,
    restart: M11DirectBlockRestart,
}

impl std::fmt::Debug for M11DirectBlockRestart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("M11DirectBlockRestart")
            .field("line_ordinal", &self.line_ordinal)
            .field("last_line_length", &self.last_line_length)
            .field("open_kinds", &self.open_kinds)
            .field("deferred", &self.deferred)
            .finish_non_exhaustive()
    }
}

impl M11DirectBlockRestart {
    pub(crate) fn replicate_for_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<M11DirectBlockRestartTransactionReplica, M11DirectBlockError> {
        if transaction_id == 0 {
            return Err(M11DirectBlockError::Invariant(
                "restart transaction identity must be nonzero",
            ));
        }
        Ok(M11DirectBlockRestartTransactionReplica {
            transaction_id,
            restart: Self {
                grammar: self.grammar.clone(),
                output: self.output.clone(),
                line_ordinal: self.line_ordinal,
                last_line_length: self.last_line_length,
                open_kinds: self.open_kinds.clone(),
                deferred: self.deferred,
                restart_join: self.restart_join,
            },
        })
    }

    #[must_use]
    pub const fn line_ordinal(&self) -> u64 {
        self.line_ordinal
    }

    #[must_use]
    pub const fn last_line_length(&self) -> u64 {
        self.last_line_length
    }

    /// Rebases this unchanged-suffix cursor by the authenticated line delta
    /// observed at parser convergence.
    ///
    /// The suffix bytes remain exact lineage authority, so its preceding-line
    /// length does not change. Only the absolute physical-line ordinal moves
    /// when the replaced fragment inserted or removed line endings.
    pub(super) fn rebase_unchanged_suffix_line_ordinal(
        &mut self,
        base_convergence_line_ordinal: u64,
        target_convergence_line_ordinal: u64,
    ) -> Result<(), M11DirectBlockError> {
        if self.line_ordinal < base_convergence_line_ordinal {
            return Err(M11DirectBlockError::Invariant(
                "retained suffix restart follows parser convergence",
            ));
        }
        self.line_ordinal = if target_convergence_line_ordinal >= base_convergence_line_ordinal {
            self.line_ordinal
                .checked_add(target_convergence_line_ordinal - base_convergence_line_ordinal)
                .ok_or(M11DirectBlockError::Invariant(
                    "rebased restart line ordinal fits u64",
                ))?
        } else {
            self.line_ordinal
                .checked_sub(base_convergence_line_ordinal - target_convergence_line_ordinal)
                .ok_or(M11DirectBlockError::Invariant(
                    "rebased restart line ordinal remains nonnegative",
                ))?
        };
        Ok(())
    }

    #[must_use]
    pub fn open_kinds(&self) -> &[BlockKind] {
        &self.open_kinds
    }

    #[must_use]
    pub const fn deferred_role(&self) -> M11DirectBlockDeferredRole {
        self.deferred
    }

    pub(super) const fn restart_join(&self) -> Option<u64> {
        self.restart_join
    }

    /// Necessary parser-state equality at a candidate suffix convergence cut.
    /// Source-line identity and Green/writer authority remain separate gates.
    #[must_use]
    pub fn is_future_compatible_with(&self, old: &Self) -> bool {
        self.grammar.is_future_grammar_compatible(&old.grammar)
            && self.output.is_future_line_local_compatible(&old.output)
    }
}

impl M11DirectBlockRestartTransactionReplica {
    pub(crate) fn into_restart(
        self,
        transaction_id: u64,
    ) -> Result<M11DirectBlockRestart, M11DirectBlockError> {
        if transaction_id == 0 || self.transaction_id != transaction_id {
            return Err(M11DirectBlockError::Invariant(
                "restart replica crossed adoption transactions",
            ));
        }
        Ok(self.restart)
    }
}

struct LineCoordinates {
    facts: M11PhysicalLineFacts,
    prefix: Vec<u8>,
}

/// CommonMark direct block controller promoted from the corpus-proven donor.
///
/// The donor retains only its open semantic path and fixed line-local scratch.
/// This adapter adds no grammar classifier: it converts the donor's scalar
/// commands and source lifecycle to the production contracts.
pub struct M11DirectBlockController {
    id: u64,
    parser: donor::DirectValueBlockParser,
    pending_command: Option<BlockCommand>,
    coordinates: Option<LineCoordinates>,
    restart_join: Option<u64>,
    poisoned: bool,
}

impl M11DirectBlockController {
    /// Creates a CommonMark controller with its initial Document command ready.
    pub fn new() -> Result<Self, M11DirectBlockError> {
        let id = next_controller_id()?;
        let parser = donor::DirectValueBlockParser::new(donor::SyntaxProfile::CommonMark)
            .map_err(map_parse_error)?;
        let mut controller = Self {
            id,
            parser,
            pending_command: None,
            coordinates: None,
            restart_join: None,
            poisoned: false,
        };
        if !controller.synchronize_pending_command()? {
            return Err(M11DirectBlockError::Invariant(
                "initial Document command is visible",
            ));
        }
        Ok(controller)
    }

    /// Captures the donor-owned half of a restart at an acknowledged physical
    /// line boundary.  The returned value is intentionally not source-bound;
    /// the writer must join it with the exact source revision and Green cut.
    pub fn capture_restart(&self) -> Result<M11DirectBlockRestart, M11DirectBlockError> {
        self.ensure_live()?;
        let pause = self
            .parser
            .capture_line_boundary_pause()
            .map_err(map_parse_error)?;
        let view = pause.pairing_view();
        let line_ordinal = u64::try_from(view.line_number())
            .map_err(|_| M11DirectBlockError::Invariant("line ordinal fits u64"))?;
        let last_line_length = u64::try_from(view.last_line_length())
            .map_err(|_| M11DirectBlockError::Invariant("line length fits u64"))?;
        let mut open_kinds = Vec::new();
        open_kinds
            .try_reserve_exact(view.open_frame_count())
            .map_err(|_| M11DirectBlockError::Invariant("restart path allocation failed"))?;
        for kind in view.open_kinds() {
            open_kinds.push(map_kind(kind)?);
        }
        let deferred = map_deferred_role(view.deferred_role())?;
        let (grammar, output) = pause.into_restart_parts().map_err(map_parse_error)?;
        Ok(M11DirectBlockRestart {
            grammar,
            output,
            line_ordinal,
            last_line_length,
            open_kinds: open_kinds.into_boxed_slice(),
            deferred,
            restart_join: self.restart_join,
        })
    }

    /// Captures a restart when the acknowledged line boundary has a
    /// donor-reachable restart encoding. Some valid container transitions
    /// deliberately have no resumable sample; callers building sparse restart
    /// indexes must skip those boundaries while continuing the definitive
    /// parse.
    pub fn capture_restart_if_available(
        &self,
    ) -> Result<Option<M11DirectBlockRestart>, M11DirectBlockError> {
        match self.capture_restart() {
            Ok(restart) => Ok(Some(restart)),
            Err(M11DirectBlockError::Invariant(
                "direct restart line-local blankness is donor-reachable",
            )) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Reconstructs a fresh direct controller from one joined parser restart.
    /// The composite writer/source authority is responsible for selecting the
    /// current output revision before handing this value here.
    pub(super) fn resume_joined(
        restart: M11DirectBlockRestart,
        restart_join: u64,
    ) -> Result<Self, M11DirectBlockError> {
        let cursor = donor::DirectLineBoundaryResumeCursor::new(
            restart.line_ordinal,
            restart.last_line_length,
        )
        .map_err(map_parse_error)?;
        let parser = donor::DirectValueBlockParser::resume_restart_parts(
            &restart.grammar,
            restart.output,
            cursor,
        )
        .map_err(map_parse_error)?;
        Ok(Self {
            id: next_controller_id()?,
            parser,
            pending_command: None,
            coordinates: None,
            restart_join: Some(restart_join),
            poisoned: false,
        })
    }

    #[must_use]
    pub const fn pending_command(&self) -> Option<&BlockCommand> {
        self.pending_command.as_ref()
    }

    /// Acknowledges exactly the visible production command.
    pub fn acknowledge_command(&mut self) -> Result<(), M11DirectBlockError> {
        self.ensure_live()?;
        let command = self
            .pending_command
            .take()
            .ok_or(M11DirectBlockError::Invariant("no command is pending"))?;
        self.parser
            .acknowledge_command()
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)?;
        if matches!(command, BlockCommand::FinishLine { .. }) {
            self.coordinates = None;
        }
        Ok(())
    }

    pub fn poll_line(
        &mut self,
        fuel: usize,
    ) -> Result<M11DirectBlockPollReceipt, M11DirectBlockError> {
        self.ensure_live()?;
        if fuel == 0 {
            return Err(M11DirectBlockError::Invariant("line poll fuel is nonzero"));
        }
        if self.pending_command.is_some() {
            return Ok(M11DirectBlockPollReceipt {
                transitions: 0,
                status: M11DirectBlockPollStatus::CommandReady,
            });
        }
        let receipt = self
            .parser
            .poll_line(fuel)
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)?;
        let status = self.map_poll_status(receipt.status)?;
        Ok(M11DirectBlockPollReceipt {
            transitions: receipt.transitions,
            status,
        })
    }

    pub fn begin_finish(&mut self) -> Result<(), M11DirectBlockError> {
        self.ensure_live()?;
        self.parser
            .begin_finish()
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)
    }

    pub fn poll_finish(
        &mut self,
        fuel: usize,
    ) -> Result<M11DirectBlockPollReceipt, M11DirectBlockError> {
        self.ensure_live()?;
        if fuel == 0 {
            return Err(M11DirectBlockError::Invariant(
                "finish poll fuel is nonzero",
            ));
        }
        if self.pending_command.is_some() {
            return Ok(M11DirectBlockPollReceipt {
                transitions: 0,
                status: M11DirectBlockPollStatus::CommandReady,
            });
        }
        let receipt = self
            .parser
            .poll_finish(fuel)
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)?;
        let status = self.map_poll_status(receipt.status)?;
        Ok(M11DirectBlockPollReceipt {
            transitions: receipt.transitions,
            status,
        })
    }

    fn map_poll_status(
        &mut self,
        status: donor::DirectPollStatus,
    ) -> Result<M11DirectBlockPollStatus, M11DirectBlockError> {
        match status {
            donor::DirectPollStatus::Pending => Ok(M11DirectBlockPollStatus::Pending),
            donor::DirectPollStatus::Complete => Ok(M11DirectBlockPollStatus::Complete),
            donor::DirectPollStatus::CommandReady => {
                if self.synchronize_pending_command()? {
                    Ok(M11DirectBlockPollStatus::CommandReady)
                } else {
                    // One donor command was acknowledged and elided. Returning
                    // Pending makes the next donor transition consume a later
                    // caller poll instead of hiding unbounded work here.
                    Ok(M11DirectBlockPollStatus::Pending)
                }
            }
            donor::DirectPollStatus::ExternalWorkReady => {
                Ok(M11DirectBlockPollStatus::ExternalWorkReady)
            }
        }
    }

    pub(super) fn pending_reference_prefix_request(
        &self,
    ) -> Result<donor::DirectReferencePrefixRequest, M11DirectBlockError> {
        self.ensure_live()?;
        self.parser
            .pending_external_work()
            .filter(|work| work.kind() == donor::DirectExternalWorkKind::ReferencePrefixFinalizer)
            .map(donor::DirectExternalWork::request)
            .ok_or(M11DirectBlockError::Invariant(
                "reference-prefix work is ready",
            ))
    }

    pub(super) fn begin_reference_prefix_work<I: Copy + Eq>(
        &mut self,
        request: donor::DirectReferencePrefixRequest,
        identity: I,
    ) -> Result<donor::DirectReferencePrefixWork<I>, M11DirectBlockError> {
        self.ensure_live()?;
        self.parser
            .begin_reference_prefix_work(request, identity)
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)
    }

    pub(super) fn commit_reference_prefix_terminal<I: Copy + Eq>(
        &mut self,
        ack: donor::DirectReferencePrefixTerminalAck<I>,
        identity: I,
    ) -> Result<donor::DirectReferencePrefixCommitStatus, M11DirectBlockError> {
        self.ensure_live()?;
        self.parser
            .commit_reference_prefix_terminal(ack, identity)
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)
    }

    pub(super) fn capture_leading_reference_remainder_continuation(
        &self,
    ) -> Result<Option<M11DirectLeadingReferenceRemainderContinuation>, M11DirectBlockError> {
        self.ensure_live()?;
        let Some(donor) = self
            .parser
            .capture_leading_reference_remainder_continuation()
            .map_err(map_parse_error)?
        else {
            return Ok(None);
        };
        Ok(Some(M11DirectLeadingReferenceRemainderContinuation {
            donor,
            restart_join: self.restart_join,
            cursor: None,
        }))
    }

    /// Maps one ready donor command, returning whether it is consumer-visible.
    fn synchronize_pending_command(&mut self) -> Result<bool, M11DirectBlockError> {
        if self.pending_command.is_some() {
            return Err(M11DirectBlockError::Invariant(
                "one production command awaits acknowledgement",
            ));
        }
        let direct = self
            .parser
            .pending_command()
            .ok_or(M11DirectBlockError::Invariant("donor command is ready"))?;
        let mapped = map_command(direct, self.coordinates.as_ref())?;
        if let Some(mapped) = mapped {
            self.pending_command = Some(mapped);
            return Ok(true);
        }

        // A tab can satisfy columns at multiple container depths while its one
        // physical byte is owned by the deepest surviving marker. Comrak emits
        // an empty, LogicalAction::None continuation claim for the outer depth.
        // It has no physical/logical Green effect, so the production adapter
        // elides it rather than weakening the nonempty coverage protocol.
        self.parser
            .acknowledge_command()
            .map_err(map_parse_error)
            .inspect_err(|_| self.poisoned = true)?;
        Ok(false)
    }

    fn ensure_live(&self) -> Result<(), M11DirectBlockError> {
        if self.poisoned {
            Err(M11DirectBlockError::Invariant("controller is poisoned"))
        } else {
            Ok(())
        }
    }
}

fn next_controller_id() -> Result<u64, M11DirectBlockError> {
    CONTROLLER_IDS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| M11DirectBlockError::Invariant("controller identity exhausted"))
}

fn map_deferred_role(
    role: donor::DirectLineBoundaryDeferredRole,
) -> Result<M11DirectBlockDeferredRole, M11DirectBlockError> {
    Ok(match role {
        donor::DirectLineBoundaryDeferredRole::None => M11DirectBlockDeferredRole::None,
        donor::DirectLineBoundaryDeferredRole::Terminator => M11DirectBlockDeferredRole::Terminator,
        donor::DirectLineBoundaryDeferredRole::BlankGap { floor_depth } => {
            M11DirectBlockDeferredRole::BlankGap { floor_depth }
        }
        donor::DirectLineBoundaryDeferredRole::Invalid => {
            return Err(M11DirectBlockError::Invariant(
                "donor restart deferred state is valid",
            ));
        }
    })
}

impl<S> M11ExactController<S> for M11DirectBlockController
where
    S: M11SourceLineSource<Identity = SourceLineIdentity>,
{
    type Admission = M11DirectSourceLineAdmission;
    type Error = M11DirectBlockControllerError<S::Error>;

    fn begin_source_line(
        &mut self,
        identity: SourceLineIdentity,
    ) -> Result<Self::Admission, Self::Error> {
        self.ensure_live().map_err(Self::Error::Controller)?;
        let physical_bytes = usize::try_from(identity.physical_bytes())
            .map_err(|_| Self::Error::Controller(M11DirectBlockError::InvalidSourceFacts))?;
        let work = self
            .parser
            .begin_source_line_work(identity, physical_bytes)
            .map_err(map_source_parse_error)?;
        Ok(M11DirectSourceLineAdmission {
            controller_id: self.id,
            identity,
            work,
            coordinate_prefix: Vec::with_capacity(
                donor::DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES,
            ),
            staged_source: VecDeque::with_capacity(
                donor::DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES,
            ),
            source_high_water: 0,
            donor_high_water: 0,
        })
    }

    fn poll_source_line(
        &mut self,
        admission: &mut Self::Admission,
        source: &mut S,
        fuel: usize,
    ) -> Result<M11SourceLinePollReceipt, Self::Error> {
        self.ensure_live().map_err(Self::Error::Controller)?;
        if admission.controller_id != self.id || admission.identity != source.identity() {
            return Err(Self::Error::WrongSource);
        }
        if fuel == 0 {
            return Err(Self::Error::ZeroFuel);
        }
        if source.len() != admission.work_physical_bytes() {
            return Err(Self::Error::WrongSource);
        }

        // The generated ATX DFA can inspect a fixed YYMAXFILL lookahead beyond
        // its logical fuel. Stage that bounded lookahead under earlier M11
        // polls instead of turning it into a virtual physical source grant.
        // Thus every real M11 read remains charged to this poll's caller fuel.
        let lookahead = admission.work.logical_access_budget_slack();
        let staging_cap = donor::DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES;
        let requested_donor_fuel = fuel.min(staging_cap.saturating_sub(lookahead));
        let logical_staging_budget = requested_donor_fuel.saturating_add(lookahead);
        let target_staged = logical_staging_budget.min(
            admission
                .work_physical_bytes()
                .saturating_sub(admission.donor_high_water),
        );
        let mut source_first_reads = 0;
        while admission.staged_source.len() < target_staged
            && admission.source_high_water < admission.work_physical_bytes()
            && source_first_reads < fuel
            && source.access_budget() > 0
        {
            let offset = admission.source_high_water;
            let byte = source.read_byte(offset).map_err(Self::Error::Source)?;
            if admission.coordinate_prefix.len()
                < donor::DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES
                && admission.coordinate_prefix.len() == offset
            {
                admission.coordinate_prefix.push(byte);
            }
            admission.staged_source.push_back(byte);
            admission.source_high_water += 1;
            source_first_reads += 1;
        }
        if admission.staged_source.len() < target_staged {
            return Ok(M11SourceLinePollReceipt {
                status: M11SourceLinePollStatus::NeedMore,
                lexical_work_units: source_first_reads,
                source_first_reads,
                physical_high_water: admission.donor_high_water,
                retained_source_bytes: admission.retained_source_bytes()?,
                source_budget_exhausted: source.access_budget() == 0
                    && admission.source_high_water < admission.work_physical_bytes(),
                maximum_source_request_rewind_bytes: 0,
            });
        }

        let mut adapted = ProductionSource {
            identity: admission.identity,
            len: admission.work_physical_bytes(),
            staged_source: &mut admission.staged_source,
            donor_high_water: &mut admission.donor_high_water,
            logical_access_budget: logical_staging_budget,
        };
        let receipt = admission
            .work
            .poll_source(&mut adapted, fuel)
            .map_err(map_staged_source_poll_error::<S::Error>)?;
        if receipt.physical_high_water != admission.donor_high_water {
            return Err(Self::Error::ScannerInvariant);
        }
        let status = match receipt.status {
            donor::DirectSourceLinePollStatus::NeedMore => M11SourceLinePollStatus::NeedMore,
            donor::DirectSourceLinePollStatus::Matched => M11SourceLinePollStatus::Matched,
        };
        let retained_source_bytes = receipt
            .retained_source_bytes
            .checked_add(admission.coordinate_prefix.len())
            .and_then(|retained| retained.checked_add(admission.staged_source.len()))
            .ok_or(Self::Error::ScannerInvariant)?;
        if retained_source_bytes > M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES {
            return Err(Self::Error::ScannerInvariant);
        }
        Ok(M11SourceLinePollReceipt {
            status,
            lexical_work_units: receipt.lexical_work_units,
            source_first_reads,
            physical_high_water: receipt.physical_high_water,
            retained_source_bytes,
            source_budget_exhausted: source.access_budget() == 0
                && admission.source_high_water < admission.work_physical_bytes(),
            maximum_source_request_rewind_bytes: receipt.maximum_source_request_rewind_bytes,
        })
    }

    fn commit_source_line(
        &mut self,
        admission: Self::Admission,
        facts: M11PhysicalLineFacts,
    ) -> Result<(), Self::Error> {
        self.ensure_live().map_err(Self::Error::Controller)?;
        if admission.controller_id != self.id
            || admission.identity != facts.identity()
            || usize::try_from(facts.physical_bytes()).ok() != Some(admission.work_physical_bytes())
            || admission.source_high_water != admission.work_physical_bytes()
            || admission.donor_high_water != admission.work_physical_bytes()
            || !admission.staged_source.is_empty()
        {
            return Err(Self::Error::Controller(
                M11DirectBlockError::InvalidSourceFacts,
            ));
        }
        let coordinates = LineCoordinates::new(facts, admission.coordinate_prefix)
            .map_err(Self::Error::Controller)?;
        self.parser
            .commit_source_line(admission.work, admission.identity, facts.physical_utf16())
            .map_err(map_source_parse_error)?;
        self.coordinates = Some(coordinates);
        Ok(())
    }

    fn cancel_source_line(&mut self, admission: Self::Admission) -> Result<(), Self::Error> {
        self.ensure_live().map_err(Self::Error::Controller)?;
        if admission.controller_id != self.id {
            return Err(Self::Error::WrongSource);
        }
        self.parser
            .cancel_source_line(admission.work)
            .map_err(map_source_parse_error)
    }
}

impl M11DirectSourceLineAdmission {
    fn work_physical_bytes(&self) -> usize {
        usize::try_from(self.identity.physical_bytes()).expect("u32 fits usize")
    }

    fn retained_source_bytes<E>(&self) -> Result<usize, M11DirectBlockControllerError<E>> {
        self.work
            .retained_source_bytes()
            .checked_add(self.coordinate_prefix.len())
            .and_then(|retained| retained.checked_add(self.staged_source.len()))
            .filter(|retained| *retained <= M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES)
            .ok_or(M11DirectBlockControllerError::ScannerInvariant)
    }
}

struct ProductionSource<'a> {
    identity: SourceLineIdentity,
    len: usize,
    staged_source: &'a mut VecDeque<u8>,
    donor_high_water: &'a mut usize,
    logical_access_budget: usize,
}

#[derive(Debug)]
enum StagedSourceError {
    NonSequential,
    BudgetExhausted,
}

impl donor::DirectSourceLineSource for ProductionSource<'_> {
    type Identity = SourceLineIdentity;
    type Error = StagedSourceError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn len(&self) -> usize {
        self.len
    }

    fn access_budget(&self) -> usize {
        self.logical_access_budget
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        if relative_offset != *self.donor_high_water {
            return Err(StagedSourceError::NonSequential);
        }
        let byte = self
            .staged_source
            .pop_front()
            .ok_or(StagedSourceError::BudgetExhausted)?;
        *self.donor_high_water += 1;
        Ok(byte)
    }
}

impl LineCoordinates {
    fn new(facts: M11PhysicalLineFacts, mut prefix: Vec<u8>) -> Result<Self, M11DirectBlockError> {
        if prefix.len()
            > usize::try_from(facts.physical_bytes())
                .map_err(|_| M11DirectBlockError::InvalidSourceFacts)?
        {
            return Err(M11DirectBlockError::InvalidSourceFacts);
        }
        if let Err(error) = std::str::from_utf8(&prefix) {
            if error.error_len().is_some() {
                return Err(M11DirectBlockError::InvalidUtf8Boundary);
            }
            prefix.truncate(error.valid_up_to());
        }
        let ending_bytes = match facts.ending() {
            M11LineEnding::Lf | M11LineEnding::Cr => 1,
            M11LineEnding::CrLf => 2,
            M11LineEnding::Eof => 0,
        };
        if facts.content_bytes().checked_add(ending_bytes) != Some(facts.physical_bytes())
            || facts.content_utf16().checked_add(ending_bytes) != Some(facts.physical_utf16())
        {
            return Err(M11DirectBlockError::InvalidSourceFacts);
        }
        if prefix.len() == usize::try_from(facts.physical_bytes()).unwrap_or(usize::MAX) {
            let text = std::str::from_utf8(&prefix)
                .map_err(|_| M11DirectBlockError::InvalidUtf8Boundary)?;
            let utf16 = u32::try_from(text.encode_utf16().count())
                .map_err(|_| M11DirectBlockError::InvalidSourceFacts)?;
            if utf16 != facts.physical_utf16() {
                return Err(M11DirectBlockError::InvalidSourceFacts);
            }
        }
        Ok(Self { facts, prefix })
    }

    fn position(&self, byte: u32) -> Result<LineSourcePosition, M11DirectBlockError> {
        if byte == self.facts.content_bytes() {
            return Ok(LineSourcePosition::new(
                u64::from(byte),
                u64::from(self.facts.content_utf16()),
            ));
        }
        if byte == self.facts.physical_bytes() {
            return Ok(LineSourcePosition::new(
                u64::from(byte),
                u64::from(self.facts.physical_utf16()),
            ));
        }
        let end = usize::try_from(byte).map_err(|_| M11DirectBlockError::InvalidSourceFacts)?;
        let prefix = self
            .prefix
            .get(..end)
            .ok_or(M11DirectBlockError::Unsupported(
                M11DirectBlockUnsupported::CoordinateOutsideRetainedWindow,
            ))?;
        let text =
            std::str::from_utf8(prefix).map_err(|_| M11DirectBlockError::InvalidUtf8Boundary)?;
        let utf16 = u64::try_from(text.encode_utf16().count())
            .map_err(|_| M11DirectBlockError::InvalidSourceFacts)?;
        Ok(LineSourcePosition::new(u64::from(byte), utf16))
    }

    fn range(&self, range: &std::ops::Range<u32>) -> Result<LineSourceRange, M11DirectBlockError> {
        let start = self.position(range.start)?;
        let end = self.position(range.end)?;
        LineSourceRange::new(start, end).ok_or(M11DirectBlockError::InvalidSourceFacts)
    }
}

fn map_command(
    command: &donor::DirectCommand,
    coordinates: Option<&LineCoordinates>,
) -> Result<Option<BlockCommand>, M11DirectBlockError> {
    if matches!(
        command,
        donor::DirectCommand::Consume {
            range,
            logical: donor::DirectLogicalAction::None,
            ..
        } if range.is_empty()
    ) {
        return Ok(None);
    }
    let range = |source: &std::ops::Range<u32>| {
        coordinates
            .ok_or(M11DirectBlockError::Invariant(
                "source command has active line coordinates",
            ))?
            .range(source)
    };
    Ok(Some(match command {
        donor::DirectCommand::Open { kind } => BlockCommand::Enter {
            kind: map_kind(*kind)?,
        },
        donor::DirectCommand::Consume {
            owner,
            part,
            range: source,
            logical,
        } => BlockCommand::Coverage {
            owner: map_owner(*owner),
            part: map_coverage_part(*part),
            source: range(source)?,
            logical: map_logical_action(*logical)?,
        },
        donor::DirectCommand::StageTerminator {
            range: source,
            ending,
        } => BlockCommand::StageTerminator {
            source: range(source)?,
            ending: map_line_ending(*ending),
        },
        donor::DirectCommand::ResolveTerminator { resolution } => BlockCommand::ResolveTerminator {
            resolution: match resolution {
                donor::DirectTerminatorResolution::ContinueCanonicalNewline => {
                    TerminatorResolution::ContinueCanonicalNewline
                }
                donor::DirectTerminatorResolution::CloseNone => TerminatorResolution::CloseNone,
            },
        },
        donor::DirectCommand::StageBlankGap { range: source } => BlockCommand::StageBlankGap {
            source: range(source)?,
        },
        donor::DirectCommand::ResolveBlankGap { owner } => BlockCommand::ResolveBlankGap {
            owner: map_owner(*owner),
        },
        donor::DirectCommand::FinalizeParagraph { outcome } => {
            let level = match outcome {
                donor::DirectParagraphOutcome::SetextHeading { level } => *level,
            };
            BlockCommand::FinalizeParagraph {
                outcome: ParagraphOutcome::setext_heading(level).ok_or(
                    M11DirectBlockError::Invariant("donor Setext level is one or two"),
                )?,
            }
        }
        donor::DirectCommand::MarkFencedCodeBoundary { boundary } => {
            BlockCommand::MarkFencedCodeBoundary {
                boundary: match boundary {
                    donor::DirectFencedCodeBoundary::InfoEnd => FencedCodeBoundary::InfoEnd,
                    donor::DirectFencedCodeBoundary::LiteralStart => {
                        FencedCodeBoundary::LiteralStart
                    }
                },
            }
        }
        donor::DirectCommand::Close {
            kind,
            final_facts,
            last_line_blank,
            child,
        } => BlockCommand::Close {
            kind: map_kind(*kind)?,
            final_facts: map_final_facts(*final_facts),
            last_line_blank: *last_line_blank,
            child: ClosedChild::new(
                child.ends_blank,
                child.item_loose_if_nonlast,
                child.item_loose_if_last,
            ),
        },
        donor::DirectCommand::FinishLine {
            physical_bytes,
            physical_utf16,
        } => {
            let coordinates = coordinates.ok_or(M11DirectBlockError::Invariant(
                "FinishLine has active line coordinates",
            ))?;
            if *physical_bytes != coordinates.facts.physical_bytes()
                || *physical_utf16 != coordinates.facts.physical_utf16()
            {
                return Err(M11DirectBlockError::InvalidSourceFacts);
            }
            BlockCommand::FinishLine {
                physical: SourceMetric::new(u64::from(*physical_bytes), u64::from(*physical_utf16))
                    .ok_or(M11DirectBlockError::InvalidSourceFacts)?,
            }
        }
        donor::DirectCommand::FinishDocument => BlockCommand::FinishDocument,
    }))
}

fn map_kind(kind: donor::DirectBlockKind) -> Result<BlockKind, M11DirectBlockError> {
    Ok(match kind {
        donor::DirectBlockKind::Document => BlockKind::Document,
        donor::DirectBlockKind::BlockQuote => BlockKind::BlockQuote,
        donor::DirectBlockKind::Paragraph => BlockKind::Paragraph,
        donor::DirectBlockKind::Heading(facts) => BlockKind::Heading(
            HeadingFacts::new(
                facts.level,
                if facts.setext {
                    HeadingStyle::Setext
                } else {
                    HeadingStyle::Atx
                },
            )
            .ok_or(M11DirectBlockError::Invariant(
                "donor heading facts are valid",
            ))?,
        ),
        donor::DirectBlockKind::List(facts) => BlockKind::List(map_list_facts(facts)?),
        donor::DirectBlockKind::Item(facts) => BlockKind::Item(
            ItemFacts::new(facts.marker_offset, facts.padding)
                .ok_or(M11DirectBlockError::Invariant("donor item facts are valid"))?,
        ),
        donor::DirectBlockKind::IndentedCode => BlockKind::IndentedCode,
        donor::DirectBlockKind::FencedCode(facts) => {
            let fence = match facts.fence {
                donor::DirectFenceCharacter::Backtick => FenceCharacter::Backtick,
                donor::DirectFenceCharacter::Tilde => FenceCharacter::Tilde,
            };
            BlockKind::FencedCode(
                FencedCodeFacts::new(
                    fence,
                    facts.minimum_closing_length,
                    facts.fence_offset_columns,
                )
                .ok_or(M11DirectBlockError::Invariant(
                    "donor fence facts are valid",
                ))?,
            )
        }
        donor::DirectBlockKind::HtmlBlock(facts) => BlockKind::HtmlBlock(HtmlBlockFacts::new(
            HtmlBlockType::new(facts.block_type).ok_or(M11DirectBlockError::Invariant(
                "donor HTML block type is valid",
            ))?,
        )),
        donor::DirectBlockKind::ThematicBreak => BlockKind::ThematicBreak,
    })
}

fn map_list_facts(facts: donor::DirectListFacts) -> Result<ListFacts, M11DirectBlockError> {
    Ok(match facts.list_type {
        donor::ListType::Bullet => ListFacts::bullet(match facts.bullet_char {
            b'-' => BulletMarker::Hyphen,
            b'+' => BulletMarker::Plus,
            b'*' => BulletMarker::Asterisk,
            _ => {
                return Err(M11DirectBlockError::Invariant(
                    "donor bullet marker is valid",
                ));
            }
        }),
        donor::ListType::Ordered => ListFacts::ordered(
            facts.start,
            match facts.delimiter {
                donor::ListDelimiter::Period => ListDelimiter::Period,
                donor::ListDelimiter::Paren => ListDelimiter::Parenthesis,
            },
        )
        .ok_or(M11DirectBlockError::Invariant(
            "donor ordered start is valid",
        ))?,
    })
}

const fn map_owner(owner: donor::DirectOwner) -> StackOwner {
    StackOwner::ancestor(owner.generations_from_top())
}

const fn map_coverage_part(part: donor::DirectCoveragePart) -> CoveragePart {
    match part {
        donor::DirectCoveragePart::Content => CoveragePart::Content,
        donor::DirectCoveragePart::ContainerMarker => CoveragePart::ContainerMarker,
        donor::DirectCoveragePart::BlockMarker => CoveragePart::BlockMarker,
        donor::DirectCoveragePart::Gap => CoveragePart::Gap,
        donor::DirectCoveragePart::Terminal => CoveragePart::Terminal,
    }
}

fn map_logical_action(
    action: donor::DirectLogicalAction,
) -> Result<LogicalAction, M11DirectBlockError> {
    Ok(match action {
        donor::DirectLogicalAction::Identity => LogicalAction::Identity,
        donor::DirectLogicalAction::CanonicalText => LogicalAction::CanonicalText,
        donor::DirectLogicalAction::HiddenUpstream => LogicalAction::HiddenUpstream,
        donor::DirectLogicalAction::CanonicalNewline => LogicalAction::CanonicalNewline,
        donor::DirectLogicalAction::None => LogicalAction::None,
        donor::DirectLogicalAction::PartialTab(partial) => LogicalAction::PartialTab(
            PartialTab::new(
                map_owner(partial.logical_target()),
                partial.remaining_spaces(),
            )
            .ok_or(M11DirectBlockError::Invariant("donor partial tab is valid"))?,
        ),
    })
}

const fn map_line_ending(ending: donor::DirectLineEnding) -> LineEnding {
    match ending {
        donor::DirectLineEnding::Lf => LineEnding::Lf,
        donor::DirectLineEnding::Cr => LineEnding::Cr,
        donor::DirectLineEnding::CrLf => LineEnding::CrLf,
    }
}

const fn map_final_facts(facts: donor::DirectFinalFacts) -> FinalFacts {
    match facts {
        donor::DirectFinalFacts::None => FinalFacts::None,
        donor::DirectFinalFacts::List { tight } => FinalFacts::List { tight },
        donor::DirectFinalFacts::FencedCode(facts) => {
            FinalFacts::FencedCode(FencedCodeCloseFacts::new(facts.closed))
        }
    }
}

fn map_parse_error(error: donor::ParseError) -> M11DirectBlockError {
    match error {
        donor::ParseError::InvalidUtf8Boundary => M11DirectBlockError::InvalidUtf8Boundary,
        donor::ParseError::Invariant(message) => M11DirectBlockError::Invariant(message),
        donor::ParseError::DirectUnsupported(unsupported) => {
            M11DirectBlockError::Unsupported(map_unsupported(unsupported))
        }
        donor::ParseError::DirectExternalWork(_) => {
            M11DirectBlockError::Unsupported(M11DirectBlockUnsupported::ReferenceExternalWork)
        }
        donor::ParseError::Facade(_) => {
            M11DirectBlockError::Invariant("donor lexical facade rejected input")
        }
    }
}

fn map_source_parse_error<E>(error: donor::ParseError) -> M11DirectBlockControllerError<E> {
    M11DirectBlockControllerError::Controller(map_parse_error(error))
}

const fn map_unsupported(unsupported: donor::DirectUnsupported) -> M11DirectBlockUnsupported {
    match unsupported {
        donor::DirectUnsupported::SyntaxProfile => M11DirectBlockUnsupported::SyntaxProfile,
        donor::DirectUnsupported::LineTooLarge => M11DirectBlockUnsupported::LineTooLarge,
        donor::DirectUnsupported::EmbeddedLineEnding => {
            M11DirectBlockUnsupported::EmbeddedLineEnding
        }
        donor::DirectUnsupported::TabOrNul => M11DirectBlockUnsupported::TabOrNul,
        donor::DirectUnsupported::BlockKind => M11DirectBlockUnsupported::BlockKind,
        donor::DirectUnsupported::AggregateContent => M11DirectBlockUnsupported::AggregateContent,
        donor::DirectUnsupported::SegmentedLine => M11DirectBlockUnsupported::SegmentedLine,
    }
}

fn map_staged_source_poll_error<E>(
    error: donor::DirectSourceLinePollError<StagedSourceError>,
) -> M11DirectBlockControllerError<E> {
    match error {
        donor::DirectSourceLinePollError::ZeroFuel => M11DirectBlockControllerError::ZeroFuel,
        donor::DirectSourceLinePollError::WrongSource => M11DirectBlockControllerError::WrongSource,
        donor::DirectSourceLinePollError::Source(
            StagedSourceError::NonSequential | StagedSourceError::BudgetExhausted,
        ) => M11DirectBlockControllerError::ScannerInvariant,
        donor::DirectSourceLinePollError::InvalidSourceByte { absolute_offset } => {
            M11DirectBlockControllerError::InvalidSourceByte { absolute_offset }
        }
        donor::DirectSourceLinePollError::InvalidUtf8 { absolute_offset } => {
            M11DirectBlockControllerError::InvalidUtf8 { absolute_offset }
        }
        donor::DirectSourceLinePollError::EmbeddedLineEnding { absolute_offset } => {
            M11DirectBlockControllerError::EmbeddedLineEnding { absolute_offset }
        }
        donor::DirectSourceLinePollError::SourceBudgetContractViolated => {
            M11DirectBlockControllerError::SourceBudgetContractViolated
        }
        donor::DirectSourceLinePollError::ScannerInvariant => {
            M11DirectBlockControllerError::ScannerInvariant
        }
        donor::DirectSourceLinePollError::PollAfterComplete => {
            M11DirectBlockControllerError::PollAfterComplete
        }
        donor::DirectSourceLinePollError::PollAfterFailure => {
            M11DirectBlockControllerError::PollAfterFailure
        }
    }
}

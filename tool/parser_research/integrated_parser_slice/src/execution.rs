//! Receipt-driven execution between the scheduler and physical parser arena.
//!
//! The scheduler's original [`ParseSpec`](crate::scheduler::ParseSpec) lane is
//! a deterministic scheduling harness: it advances a caller-supplied forecast.
//! That is useful for queue proofs but is not evidence that real parser work is
//! bounded. This module exercises the replacement seam:
//!
//! 1. the scheduler issues a one-shot capability containing hard limits;
//! 2. a parser performs work and returns a measured [`Audit`];
//! 3. only an accepted receipt advances scheduler parse progress.
//!
//! [`MeteredDelimiterPageJob`] is intentionally smaller than Markdown. It
//! inspects real input bytes, maintains parser state, and appends a real arena
//! page whose [`PageAllocationReceipt`](crate::lifetime::PageAllocationReceipt)
//! is folded into the same receipt. Its purpose is to prove this protocol, not
//! to stand in for the grammar commitment slice.

use std::error::Error as StdError;
use std::fmt;

use crate::lifetime::{JobActivationReceipt, LifetimeError, PhysicalLifetime};
use crate::scheduler::{
    ArenaJobId, Audit, JobActivationAvailability, MeasuredParseReceipt, MeasuredParseReport,
    ParseSliceReport, ParseToken, ParseWorkAvailability, ParseWorkPermit, Scheduler,
};

/// Why the existing shared lexer + grammar job cannot yet be admitted to this
/// receipt-driven lane without inventing measurements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrammarExecutionGap {
    /// `SharedLexer::poll` reports transition fuel and cursor totals, but not
    /// per-poll allocation, copy, hash, or page-index work.
    LexerHasNoCompletePerPollReceipt,
    /// `GrammarJob` exposes its complete multidimensional receipt only after
    /// the job reaches `Ready`, so a pending poll cannot be audited exactly.
    GrammarReceiptUnavailableWhilePending,
    /// One grammar poll transition can run the bounded but multi-unit atomic
    /// inline resolver, whose exact next cost cannot currently be preflighted.
    GrammarAtomicResolverCannotBePreflighted,
    /// Completed normal grammar jobs explicitly report four upstream cursor /
    /// event-iterator allocation sites whose allocator work is unobservable.
    GrammarReportsUnmeteredUpstreamAllocations,
    /// Packed grammar page bytes are private and cannot yet be transferred to
    /// physical arena pages with one-to-one allocation receipts.
    GrammarPagesCannotEnterPhysicalArena,
}

/// Current blockers to treating a real `SharedLexer` + `GrammarJob` execution
/// as bounded receipt-driven evidence.
pub const GRAMMAR_EXECUTION_GAPS: &[GrammarExecutionGap] = &[
    GrammarExecutionGap::LexerHasNoCompletePerPollReceipt,
    GrammarExecutionGap::GrammarReceiptUnavailableWhilePending,
    GrammarExecutionGap::GrammarAtomicResolverCannotBePreflighted,
    GrammarExecutionGap::GrammarReportsUnmeteredUpstreamAllocations,
    GrammarExecutionGap::GrammarPagesCannotEnterPhysicalArena,
];

/// Typed generic-protocol gap set.
///
/// The commitment slice currently has no known gap at this generic seam:
/// measured activation is capability-driven, and every slab/directory growth
/// unit has an exact bounded preflight and receipt. This does **not** make the
/// real Markdown grammar admissible; [`GRAMMAR_EXECUTION_GAPS`] remains the
/// honest blocker list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionProtocolGap {}

pub const EXECUTION_PROTOCOL_GAPS: &[ExecutionProtocolGap] = &[];

/// Result of one capability-driven measured activation slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationSliceReport {
    Status(ParseSliceReport),
    Activated(JobActivationReceipt),
}

/// Issues and executes one bounded physical candidate activation.
///
/// # Errors
///
/// Returns scheduler, arena, identity, or exact-receipt validation failures.
pub fn run_measured_activation(
    scheduler: &mut Scheduler,
    lifetime: &mut PhysicalLifetime,
    anchor_payload: &[u8],
) -> Result<ActivationSliceReport, LifetimeError> {
    match scheduler.issue_measured_job_activation()? {
        JobActivationAvailability::Status(report) => Ok(ActivationSliceReport::Status(report)),
        JobActivationAvailability::Permit(permit) => lifetime
            .activate_measured_scheduler_job(scheduler, permit, anchor_payload)
            .map(ActivationSliceReport::Activated),
    }
}

/// Parser failure with the complete audit accumulated before the failure.
///
/// A failed job is never adopted, but actual work is still reported and its
/// physical candidate is poisoned for bounded reclaim.
#[derive(Debug)]
pub struct JobPollFailure<E> {
    pub error: E,
    pub receipt: MeasuredParseReceipt,
}

/// Receipt-driven parser implemented by the worker.
///
/// Implementations are part of the accounting trust boundary. They must stop
/// before the permit's hard limits whenever the next operation's cost is
/// knowable. If an underlying allocator returns an unexpectedly larger real
/// receipt, returning it is still mandatory: scheduler adoption will reject
/// the overrun rather than advance false progress.
pub trait MeasuredParseJob {
    type Error: StdError;

    fn token(&self) -> ParseToken;
    fn arena_job(&self) -> ArenaJobId;
    fn is_ready(&self) -> bool;

    /// Runs actual work under `permit` and returns its complete measured audit.
    ///
    /// # Errors
    ///
    /// Returns a job-specific error together with the partial receipt for all
    /// work performed before failure.
    fn poll_measured(
        &mut self,
        permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>>;
}

/// Observable result of one receipt-driven execution slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionSliceReport {
    /// Scheduler-only state transition, such as promotion or waiting for job
    /// activation. No parser job was called.
    Status(ParseSliceReport),
    /// Real work was measured and adopted.
    Measured(MeasuredParseReport),
}

/// Failure while coordinating a measured parser slice.
#[derive(Debug)]
pub enum ExecutionError<E> {
    Scheduler(crate::scheduler::Error),
    Job(JobPollFailure<E>),
    JobTokenMismatch {
        expected: ParseToken,
        provided: ParseToken,
    },
    JobArenaMismatch {
        expected: ArenaJobId,
        provided: ArenaJobId,
    },
    JobCompletionMismatch {
        receipt: MeasuredParseReceipt,
    },
}

impl<E: fmt::Display> fmt::Display for ExecutionError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scheduler(error) => write!(formatter, "scheduler rejected execution: {error}"),
            Self::Job(failure) => write!(formatter, "parser job failed: {}", failure.error),
            Self::JobTokenMismatch { expected, provided } => write!(
                formatter,
                "parser job token {provided:?} does not match permit {expected:?}"
            ),
            Self::JobArenaMismatch { expected, provided } => write!(
                formatter,
                "parser arena job {provided:?} does not match permit {expected:?}"
            ),
            Self::JobCompletionMismatch { .. } => {
                formatter.write_str("parser completion state does not match its measured receipt")
            }
        }
    }
}

impl<E: StdError + 'static> StdError for ExecutionError<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Scheduler(error) => Some(error),
            Self::Job(failure) => Some(&failure.error),
            Self::JobTokenMismatch { .. }
            | Self::JobArenaMismatch { .. }
            | Self::JobCompletionMismatch { .. } => None,
        }
    }
}

impl<E> From<crate::scheduler::Error> for ExecutionError<E> {
    fn from(error: crate::scheduler::Error) -> Self {
        Self::Scheduler(error)
    }
}

/// Runs one scheduler permit through one real measured job.
///
/// Completion consistency is checked before scheduler adoption, so a broken
/// parser cannot claim the final unit while retaining unfinished local state.
/// An audit overrun is reported by `Scheduler` after consuming the one-shot
/// permit; the candidate remains uncommitted and is poisoned for bounded retry.
///
/// # Errors
///
/// Returns a scheduler error for permit/adoption rejection, a job failure with
/// its partial receipt, or a fail-closed local identity/completion mismatch.
pub fn run_measured_slice<J: MeasuredParseJob>(
    scheduler: &mut Scheduler,
    lifetime: &mut PhysicalLifetime,
    job: &mut J,
) -> Result<ExecutionSliceReport, ExecutionError<J::Error>> {
    let availability = scheduler.issue_measured_parse_slice()?;
    let ParseWorkAvailability::Work(permit) = availability else {
        let ParseWorkAvailability::Status(report) = availability else {
            unreachable!("parse work availability has two variants")
        };
        return Ok(ExecutionSliceReport::Status(report));
    };
    if job.token() != permit.token() {
        scheduler.poison_measured_parse_slice(permit)?;
        return Err(ExecutionError::JobTokenMismatch {
            expected: permit.token(),
            provided: job.token(),
        });
    }
    if job.arena_job() != permit.arena_job() {
        scheduler.poison_measured_parse_slice(permit)?;
        return Err(ExecutionError::JobArenaMismatch {
            expected: permit.arena_job(),
            provided: job.arena_job(),
        });
    }
    let receipt = match job.poll_measured(permit, lifetime) {
        Ok(receipt) => receipt,
        Err(failure) => {
            scheduler.poison_measured_parse_slice(permit)?;
            return Err(ExecutionError::Job(failure));
        }
    };
    if job.is_ready() != receipt.complete {
        scheduler.poison_measured_parse_slice(permit)?;
        return Err(ExecutionError::JobCompletionMismatch { receipt });
    }
    let report = scheduler.adopt_measured_parse_slice(permit, receipt)?;
    Ok(ExecutionSliceReport::Measured(report))
}

/// A narrow real parser used to prove receipt-driven scheduling and lifetime.
///
/// One parser unit reads and hashes one byte. The final unit writes a compact
/// `(digest, delimiter_count)` page above the active physical job root. Input
/// ownership/construction is outside the parse slice; every byte inspection and
/// every physical output allocation performed by this job is charged.
#[derive(Debug)]
pub struct MeteredDelimiterPageJob {
    token: ParseToken,
    arena_job: ArenaJobId,
    input: Box<[u8]>,
    offset: usize,
    digest: u64,
    delimiters: u64,
    ready: bool,
}

impl MeteredDelimiterPageJob {
    #[must_use]
    pub fn new(token: ParseToken, arena_job: ArenaJobId, input: impl Into<Box<[u8]>>) -> Self {
        Self {
            token,
            arena_job,
            input: input.into(),
            offset: 0,
            digest: 0,
            delimiters: 0,
            ready: false,
        }
    }

    #[must_use]
    pub const fn digest(&self) -> Option<u64> {
        if self.ready {
            Some(self.digest)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn delimiter_count(&self) -> Option<u64> {
        if self.ready {
            Some(self.delimiters)
        } else {
            None
        }
    }
}

impl MeasuredParseJob for MeteredDelimiterPageJob {
    type Error = LifetimeError;

    fn token(&self) -> ParseToken {
        self.token
    }

    fn arena_job(&self) -> ArenaJobId {
        self.arena_job
    }

    fn is_ready(&self) -> bool {
        self.ready
    }

    fn poll_measured(
        &mut self,
        permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>> {
        let mut audit = Audit::ZERO;
        let mut progressed_units = 0_u64;
        let byte_audit = Audit {
            source_bytes: 1,
            transitions: 1,
            allocated_bytes: 0,
            copied_bytes: 0,
            hashed_bytes: 1,
            index_nodes: 0,
            reclaimed_nodes: 0,
        };

        while self.offset < self.input.len()
            && checked_add_audit(audit, byte_audit).is_some_and(|next| next.fits(permit.limits()))
        {
            let byte = self.input[self.offset];
            self.digest = self
                .digest
                .wrapping_mul(0x0000_0100_0000_01b3)
                .wrapping_add(u64::from(byte) + 1);
            if matches!(byte, b'*' | b'_' | b'`' | b'|') {
                self.delimiters = self.delimiters.saturating_add(1);
            }
            self.offset += 1;
            progressed_units += 1;
            audit =
                checked_add_audit(audit, byte_audit).expect("preflight proved byte audit addition");
        }

        if self.offset == self.input.len() && !self.ready {
            let mut payload = [0_u8; 16];
            payload[..8].copy_from_slice(&self.digest.to_le_bytes());
            payload[8..].copy_from_slice(&self.delimiters.to_le_bytes());
            let remaining = remaining_limits(permit.limits(), audit)
                .expect("performed work was preflighted against permit limits");
            let allocation = lifetime
                .try_append_job_page_under_limits(
                    self.arena_job,
                    &payload,
                    remaining,
                    permit.limits(),
                )
                .map_err(|error| JobPollFailure {
                    error,
                    receipt: MeasuredParseReceipt {
                        progressed_units,
                        audit,
                        complete: false,
                    },
                })?;
            let Some(allocation) = allocation else {
                return Ok(MeasuredParseReceipt {
                    progressed_units,
                    audit,
                    complete: false,
                });
            };
            audit = checked_add_audit(audit, allocation.audit())
                .expect("preflight proved allocation audit addition");
            progressed_units += 1;
            self.ready = true;
        }

        Ok(MeasuredParseReceipt {
            progressed_units,
            audit,
            complete: self.ready,
        })
    }
}

fn checked_add_audit(left: Audit, right: Audit) -> Option<Audit> {
    Some(Audit {
        source_bytes: left.source_bytes.checked_add(right.source_bytes)?,
        transitions: left.transitions.checked_add(right.transitions)?,
        allocated_bytes: left.allocated_bytes.checked_add(right.allocated_bytes)?,
        copied_bytes: left.copied_bytes.checked_add(right.copied_bytes)?,
        hashed_bytes: left.hashed_bytes.checked_add(right.hashed_bytes)?,
        index_nodes: left.index_nodes.checked_add(right.index_nodes)?,
        reclaimed_nodes: left.reclaimed_nodes.checked_add(right.reclaimed_nodes)?,
    })
}

fn remaining_limits(
    limits: crate::scheduler::SliceLimits,
    audit: Audit,
) -> Option<crate::scheduler::SliceLimits> {
    Some(crate::scheduler::SliceLimits {
        source_bytes: limits.source_bytes.checked_sub(audit.source_bytes)?,
        transitions: limits.transitions.checked_sub(audit.transitions)?,
        allocated_bytes: limits.allocated_bytes.checked_sub(audit.allocated_bytes)?,
        copied_bytes: limits.copied_bytes.checked_sub(audit.copied_bytes)?,
        hashed_bytes: limits.hashed_bytes.checked_sub(audit.hashed_bytes)?,
        index_nodes: limits.index_nodes.checked_sub(audit.index_nodes)?,
        reclaimed_nodes: limits.reclaimed_nodes.checked_sub(audit.reclaimed_nodes)?,
    })
}

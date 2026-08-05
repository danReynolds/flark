//! Revision, supersession, remote-root, and reclamation scheduling.
//!
//! This module is intentionally grammar-free. The source store hands it an
//! exact immutable source-root transition; a parser driver consumes the active
//! generation under [`SliceLimits`]; and an arena implementation owns physical
//! allocation and reclamation behind opaque [`ArenaRootId`]s.
//!
//! Three clocks never alias:
//!
//! - [`SourceRevision`] (`S`) advances for every accepted source operation;
//! - [`GrammarRevision`] (`G`) advances only after an exact current parse is
//!   committed, and may legally jump from `G` to the latest `S`;
//! - [`ParseGeneration`] identifies one parse attempt and invalidates every
//!   older result as soon as a newer source operation is accepted, and also
//!   distinguishes an explicit retry after a measured receipt violation.
//!
//! Source operations are never discarded. While a parse is active, its single
//! queued successor is overwritten with a descriptor for the newest cumulative
//! source root. Queued descriptors do not own parser-arena storage. Promotion
//! pauses in an explicit activation state; only binding an opaque arena job
//! starts ownership and permits parser work. Cancellation then produces one
//! bounded reclaim ticket for that same root.

use std::collections::VecDeque;
use std::fmt;

/// Maximum number of remotely addressable grammar roots retained at once.
///
/// The normal steady state uses at most two: the worker-current root and the
/// UI-acknowledged root. The third slot is reserved for an atomic transition.
pub const MAX_RETAINED_ROOTS: usize = 3;

/// Fixed upper bound for outstanding arena-reclamation obligations.
///
/// A full queue applies parser backpressure; it never grows with edit count.
pub const MAX_RECLAIM_TICKETS: usize = 4;

/// Protocol cap independent of document size for one visible-page response.
pub const MAX_VISIBLE_PAGES_PER_QUERY: u32 = 64;

/// Logical allocation charged for one page handle in a transport response.
///
/// This is a protocol ledger value, not a claim about a Rust allocator's size
/// class. Physical allocation is checked by the external measurement lane.
pub const VISIBLE_PAGE_HANDLE_LOGICAL_BYTES: u64 = 24;

const CANCELLATION_AUDIT: Audit = Audit {
    source_bytes: 0,
    transitions: 1,
    allocated_bytes: 0,
    copied_bytes: 0,
    hashed_bytes: 0,
    index_nodes: 0,
    reclaimed_nodes: 0,
};

const PAGE_HANDLE_AUDIT: Audit = Audit {
    source_bytes: 0,
    transitions: 1,
    allocated_bytes: VISIBLE_PAGE_HANDLE_LOGICAL_BYTES,
    copied_bytes: 0,
    hashed_bytes: 0,
    index_nodes: 1,
    reclaimed_nodes: 0,
};

/// Exact revision of the latest adopted immutable source root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRevision(pub u64);

/// Source revision represented by the worker's committed grammar root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GrammarRevision(pub u64);

/// Identity of one parse attempt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParseGeneration(pub u64);

/// Opaque identity of an immutable source root owned by the source subsystem.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRootId(pub u64);

/// Opaque, generation-safe arena root handle supplied by the parser arena.
///
/// If an arena reuses physical slots, that arena must include its slot
/// generation in this value. The scheduler deliberately cannot dereference it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaRootId(pub u64);

/// Opaque, generation-safe mutable job handle supplied by the parser arena.
///
/// A job owns the candidate allocations and evolving immutable output roots
/// for exactly one active parse. Sealing consumes the job and returns a
/// distinct [`ArenaRootId`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaJobId(pub u64);

/// Worker-local identity for a remotely queryable grammar root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteRootId(pub u64);

/// Complete per-slice work ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Audit {
    pub source_bytes: u64,
    pub transitions: u64,
    pub allocated_bytes: u64,
    pub copied_bytes: u64,
    pub hashed_bytes: u64,
    pub index_nodes: u64,
    pub reclaimed_nodes: u64,
}

impl Audit {
    /// A ledger with no charged work.
    pub const ZERO: Self = Self {
        source_bytes: 0,
        transitions: 0,
        allocated_bytes: 0,
        copied_bytes: 0,
        hashed_bytes: 0,
        index_nodes: 0,
        reclaimed_nodes: 0,
    };

    /// Whether every dimension is within `limits`.
    #[must_use]
    pub const fn fits(self, limits: SliceLimits) -> bool {
        self.source_bytes <= limits.source_bytes
            && self.transitions <= limits.transitions
            && self.allocated_bytes <= limits.allocated_bytes
            && self.copied_bytes <= limits.copied_bytes
            && self.hashed_bytes <= limits.hashed_bytes
            && self.index_nodes <= limits.index_nodes
            && self.reclaimed_nodes <= limits.reclaimed_nodes
    }

    #[must_use]
    const fn is_zero(self) -> bool {
        self.source_bytes == 0
            && self.transitions == 0
            && self.allocated_bytes == 0
            && self.copied_bytes == 0
            && self.hashed_bytes == 0
            && self.index_nodes == 0
            && self.reclaimed_nodes == 0
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            source_bytes: self.source_bytes.checked_add(other.source_bytes)?,
            transitions: self.transitions.checked_add(other.transitions)?,
            allocated_bytes: self.allocated_bytes.checked_add(other.allocated_bytes)?,
            copied_bytes: self.copied_bytes.checked_add(other.copied_bytes)?,
            hashed_bytes: self.hashed_bytes.checked_add(other.hashed_bytes)?,
            index_nodes: self.index_nodes.checked_add(other.index_nodes)?,
            reclaimed_nodes: self.reclaimed_nodes.checked_add(other.reclaimed_nodes)?,
        })
    }

    #[must_use]
    fn can_add(self, other: Self, limits: SliceLimits) -> bool {
        self.checked_add(other)
            .is_some_and(|combined| combined.fits(limits))
    }
}

/// Hard budget applied independently to every parser, query, or reclaim slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceLimits {
    pub source_bytes: u64,
    pub transitions: u64,
    pub allocated_bytes: u64,
    pub copied_bytes: u64,
    pub hashed_bytes: u64,
    pub index_nodes: u64,
    pub reclaimed_nodes: u64,
}

impl SliceLimits {
    /// Whether an audit is legal for one slice.
    #[must_use]
    pub const fn allows(self, audit: Audit) -> bool {
        audit.fits(self)
    }

    fn remaining_after(self, audit: Audit) -> Option<Self> {
        Some(Self {
            source_bytes: self.source_bytes.checked_sub(audit.source_bytes)?,
            transitions: self.transitions.checked_sub(audit.transitions)?,
            allocated_bytes: self.allocated_bytes.checked_sub(audit.allocated_bytes)?,
            copied_bytes: self.copied_bytes.checked_sub(audit.copied_bytes)?,
            hashed_bytes: self.hashed_bytes.checked_sub(audit.hashed_bytes)?,
            index_nodes: self.index_nodes.checked_sub(audit.index_nodes)?,
            reclaimed_nodes: self.reclaimed_nodes.checked_sub(audit.reclaimed_nodes)?,
        })
    }
}

/// One exact immutable source-root transition.
///
/// Applying text is the source subsystem's job. The scheduler validates the
/// complete root chain, so replacing a queued parse cannot skip a source
/// operation: the next transition must name both the current revision and the
/// current immutable root as its base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceOperation {
    pub base_revision: SourceRevision,
    pub target_revision: SourceRevision,
    pub base_root: SourceRootId,
    pub result_root: SourceRootId,
}

/// Smallest resumable parser work unit and simulated job length.
///
/// This is mechanism fuel, not Markdown grammar. A production parser driver
/// must choose work units small enough that `unit_audit` fits one slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseSpec {
    pub work_units: u64,
    pub unit_audit: Audit,
}

/// Generation-scoped identity returned for one scheduled parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ParseToken {
    generation: ParseGeneration,
    target_revision: SourceRevision,
}

impl ParseToken {
    #[must_use]
    pub const fn generation(self) -> ParseGeneration {
        self.generation
    }

    #[must_use]
    pub const fn target_revision(self) -> SourceRevision {
        self.target_revision
    }
}

/// Where a newly submitted generation resides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// It immediately became the one active parse.
    Active,
    /// It replaced the one unstarted queued descriptor.
    Queued,
}

/// Receipt for an accepted source operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Submission {
    pub token: ParseToken,
    pub admission: Admission,
}

/// State of the active parse after one bounded parser slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseSliceStatus {
    Idle,
    /// The active descriptor owns no arena job yet. Parser work is forbidden
    /// until [`Scheduler::activate_active`] binds an opaque arena job.
    AwaitingActivation,
    Pending,
    ReadyToSeal,
    SealedReadyToCommit,
    /// The active candidate could not be retired without exceeding the fixed
    /// reclaim-ticket capacity. Source ingestion may continue to replace the
    /// queued descriptor while reclamation catches up.
    ReclaimBackpressure,
}

/// Observable result of one parser slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseSliceReport {
    pub status: ParseSliceStatus,
    pub audit: Audit,
    pub progressed_units: u64,
    pub worked_on: Option<ParseToken>,
    pub worked_arena_job: Option<ArenaJobId>,
    pub cancelled: Option<ParseToken>,
    pub promoted: Option<ParseToken>,
}

/// Capability for one receipt-driven parser slice.
///
/// The limits are a hard ceiling, not a planning estimate. A parser driver
/// must stop before exceeding any dimension and report the work it actually
/// performed to [`Scheduler::adopt_measured_parse_slice`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseWorkPermit {
    sequence: u64,
    token: ParseToken,
    arena_job: ArenaJobId,
    limits: SliceLimits,
}

/// One-shot capability for physically creating and binding a measured job.
///
/// Activation is its own bounded worker slice. The arena must preflight the
/// exact anchor allocation—including slab and directory initialization—against
/// these limits before mutating physical state, then submit the actual receipt
/// through [`Scheduler::adopt_measured_job_activation`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobActivationPermit {
    sequence: u64,
    token: ParseToken,
    limits: SliceLimits,
}

impl JobActivationPermit {
    #[must_use]
    pub const fn token(self) -> ParseToken {
        self.token
    }

    #[must_use]
    pub const fn limits(self) -> SliceLimits {
        self.limits
    }
}

/// Result of requesting a measured job-activation slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobActivationAvailability {
    /// Supersession, idle, or already-activated state; no allocation capability
    /// was issued.
    Status(ParseSliceReport),
    Permit(JobActivationPermit),
}

/// Accepted physical activation receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredJobActivationReport {
    pub token: ParseToken,
    pub arena_job: ArenaJobId,
    pub audit: Audit,
}

impl ParseWorkPermit {
    #[must_use]
    pub const fn token(self) -> ParseToken {
        self.token
    }

    #[must_use]
    pub const fn arena_job(self) -> ArenaJobId {
        self.arena_job
    }

    #[must_use]
    pub const fn limits(self) -> SliceLimits {
        self.limits
    }
}

/// Result of asking the scheduler for receipt-driven work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseWorkAvailability {
    /// No parser work may run. This also carries supersession/promotion
    /// receipts so the physical lifetime can reclaim the cancelled job.
    Status(ParseSliceReport),
    Work(ParseWorkPermit),
}

/// Work measured by a real parser/arena driver under one permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredParseReceipt {
    pub progressed_units: u64,
    pub audit: Audit,
    /// Exact parser completion observed by the real job. Measured parses do
    /// not forecast a dynamic total transition count before execution.
    pub complete: bool,
}

/// State after accepting a measured parser receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredParseReport {
    pub status: ParseSliceStatus,
    pub audit: Audit,
    pub progressed_units: u64,
    pub worked_on: ParseToken,
    pub worked_arena_job: ArenaJobId,
}

/// Generation-scoped remote lease. It never exposes the parser-arena handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RootLease {
    remote_root: RemoteRootId,
    parse_generation: ParseGeneration,
    source_revision: SourceRevision,
}

impl RootLease {
    #[must_use]
    pub const fn remote_root(self) -> RemoteRootId {
        self.remote_root
    }

    #[must_use]
    pub const fn parse_generation(self) -> ParseGeneration {
        self.parse_generation
    }

    #[must_use]
    pub const fn source_revision(self) -> SourceRevision {
        self.source_revision
    }
}

/// One legal committed jump from adopted grammar `G` to current source `S`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrammarDelta {
    pub base_grammar_revision: GrammarRevision,
    pub target_source_revision: SourceRevision,
    pub parse_generation: ParseGeneration,
    pub source_root: SourceRootId,
    pub root_lease: RootLease,
}

/// Atomic root acknowledgement receipt.
///
/// Acknowledging a newer offer also releases the previously acknowledged UI
/// lease. That atomic handoff is what bounds stalled-ack root retention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AckReceipt {
    pub acknowledged: RootLease,
    pub released_previous: Option<RootLease>,
}

/// One opaque page handle returned for the requested remote root generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisiblePageHandle {
    pub remote_root: RemoteRootId,
    pub parse_generation: ParseGeneration,
    pub page_index: u32,
}

/// Bounded visible-page response with a continuation inside the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisiblePageBatch {
    pub lease: RootLease,
    pub pages: Vec<VisiblePageHandle>,
    pub next_page: Option<u32>,
    pub audit: Audit,
}

/// Why an opaque arena ownership handle left live ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReclaimReason {
    CandidateCancelled,
    RemoteRootRetired,
}

/// Typed arena ownership handle; job and frozen-root namespaces never alias.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReclaimHandle {
    Job(ArenaJobId),
    Root(ArenaRootId),
}

/// Arena-owned state that must be reclaimed iteratively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReclaimTarget {
    pub handle: ReclaimHandle,
    pub reason: ReclaimReason,
}

/// One FIFO obligation in the scheduler's fixed reclaim queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimTicket {
    pub target: ReclaimTarget,
}

/// Budgeted request issued to the physical arena implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimRequest {
    pub ticket: ReclaimTicket,
    pub limits: SliceLimits,
}

/// Receipt from one physical arena reclaim step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimOutcome {
    /// `complete` means the target no longer owns arena storage. A pending
    /// receipt must report nonzero audited work or the scheduler stops the
    /// slice to avoid spinning.
    Progress { audit: Audit, complete: bool },
    /// The arena did not recognize a scheduler-owned target.
    Rejected,
}

/// Physical reclamation seam implemented by the parser arena.
///
/// The scheduler proves ticket bounds, ordering, backpressure, and audit
/// enforcement. This trait deliberately leaves nonrecursive node traversal and
/// actual frees to the arena proof.
pub trait Reclaimer {
    fn reclaim(&mut self, request: ReclaimRequest) -> ReclaimOutcome;
}

/// Result of one bounded reclaim slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimSliceReport {
    pub audit: Audit,
    pub completed_tickets: usize,
    pub remaining_tickets: usize,
    pub stalled: bool,
}

/// Construction parameters for revision zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialState {
    pub source_root: SourceRootId,
    pub arena_root: ArenaRootId,
    pub visible_pages: u32,
}

/// Rejected state transition. Every error leaves scheduler-owned state intact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSliceLimits(&'static str),
    SourceRevisionMismatch {
        expected: SourceRevision,
        provided: SourceRevision,
    },
    SourceTargetNotNext {
        base: SourceRevision,
        target: SourceRevision,
    },
    SourceRootMismatch {
        expected: SourceRootId,
        provided: SourceRootId,
    },
    CounterExhausted(&'static str),
    InvalidParseSpec(&'static str),
    ParseUnitExceedsSliceLimits {
        unit: Audit,
        limits: SliceLimits,
    },
    ParseSliceAlreadyIssued,
    NoIssuedParseSlice,
    ParseSlicePermitMismatch,
    JobActivationSliceAlreadyIssued,
    NoIssuedJobActivation,
    JobActivationPermitMismatch,
    InvalidJobActivationReceipt(&'static str),
    JobActivationReceiptExceedsLimits {
        audit: Audit,
        limits: SliceLimits,
    },
    InvalidMeasuredParseReceipt(&'static str),
    MeasuredParseReceiptExceedsLimits {
        audit: Audit,
        limits: SliceLimits,
    },
    MeasuredParseRequiresPermit,
    ForecastParseRequiresLegacyRunner,
    MeasuredParseNotComplete,
    MeasuredParsePoisoned,
    MeasuredParseNotPoisoned,
    PoisonedRetryHasQueuedSource,
    NoActiveParse,
    StaleParseGeneration {
        provided: ParseGeneration,
        latest: ParseGeneration,
    },
    ParseTokenMismatch {
        expected: ParseToken,
        provided: ParseToken,
    },
    ParseNotReady {
        remaining_units: u64,
    },
    ParseNotActivated,
    ParseAlreadyActivated,
    ParseAlreadySealed,
    ParseNotSealed,
    ParseTargetBehindSource {
        target: SourceRevision,
        current: SourceRevision,
    },
    ParseSourceRootMismatch {
        parsed: SourceRootId,
        current: SourceRootId,
    },
    ArenaRootAlreadyOwned(ArenaRootId),
    ArenaJobAlreadyOwned(ArenaJobId),
    ArenaJobMismatch {
        expected: ArenaJobId,
        provided: ArenaJobId,
    },
    StaleRootLease {
        remote_root: RemoteRootId,
        parse_generation: ParseGeneration,
    },
    VisiblePageUnitExceedsSliceLimits,
    ReclaimQueueFull {
        needed: usize,
        available: usize,
    },
    DuplicateReclaimTarget(ReclaimTarget),
    ReclaimerRejected(ReclaimTarget),
    ReclaimReceiptExceedsSliceLimits {
        audit: Audit,
        remaining: SliceLimits,
    },
    InvariantViolation(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSliceLimits(reason) => write!(formatter, "invalid slice limits: {reason}"),
            Self::SourceRevisionMismatch { expected, provided } => write!(
                formatter,
                "source operation starts at {provided:?}, current source is {expected:?}"
            ),
            Self::SourceTargetNotNext { base, target } => write!(
                formatter,
                "source transition {base:?} -> {target:?} is not one revision"
            ),
            Self::SourceRootMismatch { expected, provided } => write!(
                formatter,
                "source operation names base root {provided:?}, current root is {expected:?}"
            ),
            Self::CounterExhausted(counter) => write!(formatter, "{counter} exhausted"),
            Self::InvalidParseSpec(reason) => write!(formatter, "invalid parse spec: {reason}"),
            Self::ParseUnitExceedsSliceLimits { .. } => {
                formatter.write_str("one parser transition exceeds the configured slice limits")
            }
            Self::ParseSliceAlreadyIssued
            | Self::NoIssuedParseSlice
            | Self::ParseSlicePermitMismatch
            | Self::JobActivationSliceAlreadyIssued
            | Self::NoIssuedJobActivation
            | Self::JobActivationPermitMismatch
            | Self::InvalidJobActivationReceipt(_)
            | Self::JobActivationReceiptExceedsLimits { .. }
            | Self::InvalidMeasuredParseReceipt(_)
            | Self::MeasuredParseReceiptExceedsLimits { .. }
            | Self::MeasuredParseRequiresPermit
            | Self::ForecastParseRequiresLegacyRunner
            | Self::MeasuredParseNotComplete
            | Self::MeasuredParsePoisoned
            | Self::MeasuredParseNotPoisoned
            | Self::PoisonedRetryHasQueuedSource => self.fmt_measured(formatter),
            Self::NoActiveParse => formatter.write_str("there is no active parse"),
            Self::StaleParseGeneration { provided, latest } => write!(
                formatter,
                "parse generation {provided:?} is stale; latest is {latest:?}"
            ),
            Self::ParseTokenMismatch { .. } => {
                formatter.write_str("parse token does not identify the active parse")
            }
            Self::ParseNotReady { remaining_units } => {
                write!(formatter, "parse still has {remaining_units} work units")
            }
            Self::ParseNotActivated => formatter.write_str("active parse has no bound arena job"),
            Self::ParseAlreadyActivated => {
                formatter.write_str("active parse already has a bound arena job")
            }
            Self::ParseAlreadySealed => formatter.write_str("active parse is already sealed"),
            Self::ParseNotSealed => formatter.write_str("active parse has no sealed arena root"),
            Self::ParseTargetBehindSource { target, current } => write!(
                formatter,
                "parse target {target:?} is behind current source {current:?}"
            ),
            Self::ParseSourceRootMismatch { parsed, current } => write!(
                formatter,
                "parse captured source root {parsed:?}, current root is {current:?}"
            ),
            Self::ArenaRootAlreadyOwned(root) => {
                write!(formatter, "arena root {root:?} is already owned")
            }
            Self::ArenaJobAlreadyOwned(job) => {
                write!(formatter, "arena job {job:?} is already owned")
            }
            Self::ArenaJobMismatch { expected, provided } => write!(
                formatter,
                "sealed arena job {provided:?} does not match active job {expected:?}"
            ),
            Self::StaleRootLease {
                remote_root,
                parse_generation,
            } => write!(
                formatter,
                "remote root {remote_root:?} generation {parse_generation:?} is stale or released"
            ),
            Self::VisiblePageUnitExceedsSliceLimits => {
                formatter.write_str("one visible-page handle exceeds the configured slice limits")
            }
            Self::ReclaimQueueFull { needed, available } => write!(
                formatter,
                "reclaim queue needs {needed} slots but only {available} are free"
            ),
            Self::DuplicateReclaimTarget(target) => {
                write!(formatter, "duplicate reclaim target {target:?}")
            }
            Self::ReclaimerRejected(target) => {
                write!(formatter, "arena rejected reclaim target {target:?}")
            }
            Self::ReclaimReceiptExceedsSliceLimits { .. } => {
                formatter.write_str("arena reclaim receipt exceeds its issued limits")
            }
            Self::InvariantViolation(reason) => {
                write!(formatter, "scheduler invariant violated: {reason}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    fn fmt_measured(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ParseSliceAlreadyIssued => "a measured parser slice is already outstanding",
            Self::NoIssuedParseSlice => "there is no outstanding measured parser slice",
            Self::ParseSlicePermitMismatch => {
                "measured parser receipt used the wrong or stale permit"
            }
            Self::JobActivationSliceAlreadyIssued => {
                "a measured job-activation slice is already outstanding"
            }
            Self::NoIssuedJobActivation => "there is no outstanding measured job-activation slice",
            Self::JobActivationPermitMismatch => "job activation used the wrong or stale permit",
            Self::InvalidJobActivationReceipt(reason) => reason,
            Self::JobActivationReceiptExceedsLimits { .. } => {
                "measured job activation exceeded its issued hard limits"
            }
            Self::InvalidMeasuredParseReceipt(reason) => {
                return write!(formatter, "invalid measured parser receipt: {reason}");
            }
            Self::MeasuredParseReceiptExceedsLimits { .. } => {
                "measured parser receipt exceeds its issued hard limits"
            }
            Self::MeasuredParseRequiresPermit => {
                "receipt-driven parse must use issue/adopt measured parser slices"
            }
            Self::ForecastParseRequiresLegacyRunner => {
                "forecast-driven parse must use the legacy simulated runner"
            }
            Self::MeasuredParseNotComplete => "measured parser has not reported exact completion",
            Self::MeasuredParsePoisoned => {
                "measured parser receipt was rejected; candidate must be superseded and reclaimed"
            }
            Self::MeasuredParseNotPoisoned => "active measured parse is not poisoned",
            Self::PoisonedRetryHasQueuedSource => {
                "queued source already supersedes the poisoned measured parse"
            }
            _ => unreachable!("only measured-execution errors use this formatter"),
        };
        formatter.write_str(message)
    }
}

#[derive(Clone, Copy, Debug)]
struct SealedCandidate {
    arena_root: ArenaRootId,
    visible_pages: u32,
}

#[derive(Clone, Copy, Debug)]
enum ParseExecution {
    Forecast {
        unit_audit: Audit,
        remaining_units: u64,
    },
    Measured {
        complete: bool,
        poisoned: bool,
    },
}

impl ParseExecution {
    #[must_use]
    const fn is_ready(self) -> bool {
        match self {
            Self::Forecast {
                remaining_units, ..
            } => remaining_units == 0,
            Self::Measured {
                complete, poisoned, ..
            } => complete && !poisoned,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ParsePlan {
    token: ParseToken,
    source_root: SourceRootId,
    operation_count: u64,
    execution: ParseExecution,
    arena_job: Option<ArenaJobId>,
    sealed: Option<SealedCandidate>,
}

impl ParsePlan {
    #[must_use]
    fn cancel_target(self) -> Option<ReclaimTarget> {
        self.sealed.map_or_else(
            || {
                self.arena_job.map(|arena_job| ReclaimTarget {
                    handle: ReclaimHandle::Job(arena_job),
                    reason: ReclaimReason::CandidateCancelled,
                })
            },
            |sealed| {
                Some(ReclaimTarget {
                    handle: ReclaimHandle::Root(sealed.arena_root),
                    reason: ReclaimReason::CandidateCancelled,
                })
            },
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct RootRecord {
    lease: RootLease,
    arena_root: ArenaRootId,
    visible_pages: u32,
    worker_current: bool,
    offered: bool,
    acknowledged: bool,
}

impl RootRecord {
    #[must_use]
    const fn retained(self) -> bool {
        self.worker_current || self.offered || self.acknowledged
    }

    #[must_use]
    const fn remotely_live(self) -> bool {
        self.offered || self.acknowledged
    }
}

#[derive(Debug)]
struct ReclaimQueue {
    tickets: VecDeque<ReclaimTicket>,
}

impl Default for ReclaimQueue {
    fn default() -> Self {
        Self {
            // Ticket insertion is on the live parser path. Reserve the fixed
            // protocol maximum at document construction so enqueue never
            // triggers capacity growth.
            tickets: VecDeque::with_capacity(MAX_RECLAIM_TICKETS),
        }
    }
}

impl ReclaimQueue {
    #[must_use]
    fn available(&self) -> usize {
        MAX_RECLAIM_TICKETS - self.tickets.len()
    }

    #[must_use]
    fn contains_handle(&self, handle: ReclaimHandle) -> bool {
        self.tickets
            .iter()
            .any(|ticket| ticket.target.handle == handle)
    }

    fn preflight(&self, targets: &[ReclaimTarget]) -> Result<(), Error> {
        if targets.len() > self.available() {
            return Err(Error::ReclaimQueueFull {
                needed: targets.len(),
                available: self.available(),
            });
        }
        for (index, target) in targets.iter().copied().enumerate() {
            if self.contains_handle(target.handle)
                || targets[..index]
                    .iter()
                    .any(|prior| prior.handle == target.handle)
            {
                return Err(Error::DuplicateReclaimTarget(target));
            }
        }
        Ok(())
    }

    fn enqueue_all(&mut self, targets: &[ReclaimTarget]) {
        debug_assert!(self.preflight(targets).is_ok());
        self.tickets.extend(
            targets
                .iter()
                .copied()
                .map(|target| ReclaimTicket { target }),
        );
    }
}

/// Pure scheduling state for one document.
#[derive(Debug)]
pub struct Scheduler {
    limits: SliceLimits,
    source_revision: SourceRevision,
    grammar_revision: GrammarRevision,
    source_root: SourceRootId,
    adopted_source_root: SourceRootId,
    operation_count: u64,
    latest_generation: ParseGeneration,
    last_parse_slice_sequence: u64,
    issued_job_activation: Option<JobActivationPermit>,
    issued_parse_slice: Option<ParseWorkPermit>,
    active: Option<ParsePlan>,
    queued: Option<ParsePlan>,
    roots: Vec<RootRecord>,
    last_remote_root: RemoteRootId,
    reclaim: ReclaimQueue,
}

impl Scheduler {
    /// Creates revision zero with one worker-current, UI-acknowledged root.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSliceLimits`] when a fundamental scheduler or
    /// visible-query transition cannot fit one configured slice.
    pub fn new(limits: SliceLimits, initial: InitialState) -> Result<Self, Error> {
        validate_limits(limits)?;
        let initial_lease = RootLease {
            remote_root: RemoteRootId(1),
            parse_generation: ParseGeneration(0),
            source_revision: SourceRevision(0),
        };
        let mut roots = Vec::with_capacity(MAX_RETAINED_ROOTS);
        roots.push(RootRecord {
            lease: initial_lease,
            arena_root: initial.arena_root,
            visible_pages: initial.visible_pages,
            worker_current: true,
            offered: false,
            acknowledged: true,
        });
        Ok(Self {
            limits,
            source_revision: SourceRevision(0),
            grammar_revision: GrammarRevision(0),
            source_root: initial.source_root,
            adopted_source_root: initial.source_root,
            operation_count: 0,
            latest_generation: ParseGeneration(0),
            last_parse_slice_sequence: 0,
            issued_job_activation: None,
            issued_parse_slice: None,
            active: None,
            queued: None,
            roots,
            last_remote_root: RemoteRootId(1),
            reclaim: ReclaimQueue::default(),
        })
    }

    /// Accepts the next exact source-root transition and schedules its latest
    /// cumulative snapshot.
    ///
    /// # Errors
    ///
    /// Rejects a broken revision/root chain, an impossible parse unit, or a
    /// counter overflow. State is unchanged on error.
    pub fn submit_source_operation(
        &mut self,
        operation: SourceOperation,
        spec: ParseSpec,
    ) -> Result<Submission, Error> {
        self.validate_operation(operation)?;
        self.validate_parse_spec(spec)?;
        self.submit_validated_operation(
            operation,
            ParseExecution::Forecast {
                unit_audit: spec.unit_audit,
                remaining_units: spec.work_units,
            },
        )
    }

    /// Accepts the next exact source-root transition for receipt-driven work.
    ///
    /// No forecast audit is stored or consumed. The parse can advance only
    /// through an issued [`ParseWorkPermit`] and an accepted
    /// [`MeasuredParseReceipt`].
    ///
    /// # Errors
    ///
    /// Rejects a broken revision/root chain or a counter overflow. State is
    /// unchanged on error. No total work count is requested because a real
    /// grammar generally cannot know its dynamic transition count pre-parse.
    pub fn submit_measured_source_operation(
        &mut self,
        operation: SourceOperation,
    ) -> Result<Submission, Error> {
        self.validate_operation(operation)?;
        self.submit_validated_operation(
            operation,
            ParseExecution::Measured {
                complete: false,
                poisoned: false,
            },
        )
    }

    fn submit_validated_operation(
        &mut self,
        operation: SourceOperation,
        execution: ParseExecution,
    ) -> Result<Submission, Error> {
        let next_generation = self
            .latest_generation
            .0
            .checked_add(1)
            .map(ParseGeneration)
            .ok_or(Error::CounterExhausted("parse generation"))?;
        let next_operation_count = self
            .operation_count
            .checked_add(1)
            .ok_or(Error::CounterExhausted("source operation count"))?;
        let token = ParseToken {
            generation: next_generation,
            target_revision: operation.target_revision,
        };
        let plan = ParsePlan {
            token,
            source_root: operation.result_root,
            operation_count: next_operation_count,
            execution,
            arena_job: None,
            sealed: None,
        };
        let admission = if self.active.is_none() {
            debug_assert!(self.queued.is_none());
            self.active = Some(plan);
            Admission::Active
        } else {
            self.queued = Some(plan);
            Admission::Queued
        };
        self.source_revision = operation.target_revision;
        self.source_root = operation.result_root;
        self.operation_count = next_operation_count;
        self.latest_generation = next_generation;
        // Source adoption invalidates an in-flight parser capability. The
        // physical job remains scheduler-owned and is reclaimed when the
        // queued latest snapshot is promoted.
        self.issued_job_activation = None;
        self.issued_parse_slice = None;
        Ok(Submission { token, admission })
    }

    /// Issues a hard-limited capability for one measured job activation.
    ///
    /// Supersession is resolved before any capability is issued. The returned
    /// permit authorizes exactly one physical anchor preflight/allocation; it
    /// does not itself bind an arena job or advance parse progress.
    ///
    /// # Errors
    ///
    /// Rejects another outstanding execution capability, forecast-mode work,
    /// a poisoned candidate, counter exhaustion, or invalid supersession.
    pub fn issue_measured_job_activation(&mut self) -> Result<JobActivationAvailability, Error> {
        if self.issued_job_activation.is_some() {
            return Err(Error::JobActivationSliceAlreadyIssued);
        }
        if self.issued_parse_slice.is_some() {
            return Err(Error::ParseSliceAlreadyIssued);
        }
        if let Some(report) = self.promote_queued_for_slice()? {
            return Ok(JobActivationAvailability::Status(report));
        }
        let Some(active) = self.active else {
            return Ok(JobActivationAvailability::Status(Self::status_only_report(
                ParseSliceStatus::Idle,
            )));
        };
        let ParseExecution::Measured { complete, poisoned } = active.execution else {
            return Err(Error::ForecastParseRequiresLegacyRunner);
        };
        if poisoned {
            return Err(Error::MeasuredParsePoisoned);
        }
        if active.sealed.is_some() {
            return Ok(JobActivationAvailability::Status(Self::status_only_report(
                ParseSliceStatus::SealedReadyToCommit,
            )));
        }
        if active.arena_job.is_some() {
            let status = if complete {
                ParseSliceStatus::ReadyToSeal
            } else {
                ParseSliceStatus::Pending
            };
            return Ok(JobActivationAvailability::Status(Self::status_only_report(
                status,
            )));
        }
        let sequence = self
            .last_parse_slice_sequence
            .checked_add(1)
            .ok_or(Error::CounterExhausted("measured execution slice sequence"))?;
        let permit = JobActivationPermit {
            sequence,
            token: active.token,
            limits: self.limits,
        };
        self.last_parse_slice_sequence = sequence;
        self.issued_job_activation = Some(permit);
        Ok(JobActivationAvailability::Permit(permit))
    }

    /// Validates a predicted physical activation receipt against the currently
    /// issued capability without consuming it.
    ///
    /// This crate-private preflight lets the lifetime reject an impossible
    /// anchor before allocation. Actual work must still be submitted through
    /// [`Self::adopt_measured_job_activation`].
    pub(crate) fn validate_measured_job_activation(
        &self,
        permit: JobActivationPermit,
        audit: Audit,
    ) -> Result<(), Error> {
        let issued = self
            .issued_job_activation
            .ok_or(Error::NoIssuedJobActivation)?;
        if issued != permit {
            return Err(Error::JobActivationPermitMismatch);
        }
        self.validate_current_token(permit.token)?;
        let active = self.active.ok_or(Error::NoActiveParse)?;
        if active.arena_job.is_some() {
            return Err(Error::ParseAlreadyActivated);
        }
        if !matches!(
            active.execution,
            ParseExecution::Measured {
                complete: false,
                poisoned: false
            }
        ) {
            return Err(Error::MeasuredParsePoisoned);
        }
        validate_job_activation_receipt(permit, audit)
    }

    /// Consumes an issued activation capability without binding a job.
    ///
    /// The physical lifetime uses this after a preflight or arena failure so a
    /// one-shot capability cannot wedge subsequent attempts.
    pub(crate) fn cancel_measured_job_activation(
        &mut self,
        permit: JobActivationPermit,
    ) -> Result<(), Error> {
        let issued = self
            .issued_job_activation
            .ok_or(Error::NoIssuedJobActivation)?;
        if issued != permit {
            return Err(Error::JobActivationPermitMismatch);
        }
        self.issued_job_activation = None;
        Ok(())
    }

    /// Atomically binds a real arena job after accepting its exact physical
    /// activation receipt.
    ///
    /// The permit is consumed before validation because allocation may already
    /// have occurred. On rejection, the lifetime must immediately discard the
    /// still-unlinked anchor.
    pub(crate) fn adopt_measured_job_activation(
        &mut self,
        permit: JobActivationPermit,
        arena_job: ArenaJobId,
        audit: Audit,
    ) -> Result<MeasuredJobActivationReport, Error> {
        let issued = self
            .issued_job_activation
            .ok_or(Error::NoIssuedJobActivation)?;
        if issued != permit {
            return Err(Error::JobActivationPermitMismatch);
        }
        self.issued_job_activation = None;
        self.validate_current_token(permit.token)?;
        let active = self.active.ok_or(Error::NoActiveParse)?;
        if active.arena_job.is_some() {
            return Err(Error::ParseAlreadyActivated);
        }
        if !matches!(
            active.execution,
            ParseExecution::Measured {
                complete: false,
                poisoned: false
            }
        ) {
            return Err(Error::MeasuredParsePoisoned);
        }
        validate_job_activation_receipt(permit, audit)?;
        if self.arena_job_is_owned(arena_job) {
            return Err(Error::ArenaJobAlreadyOwned(arena_job));
        }
        self.active.as_mut().ok_or(Error::NoActiveParse)?.arena_job = Some(arena_job);
        Ok(MeasuredJobActivationReport {
            token: permit.token,
            arena_job,
            audit,
        })
    }

    /// Binds a forecast-harness generation to one opaque arena job before
    /// simulated parser work begins.
    ///
    /// A queued descriptor deliberately has no arena handle. After promotion,
    /// [`run_parse_slice`](Self::run_parse_slice) returns
    /// [`ParseSliceStatus::AwaitingActivation`] without doing grammar work;
    /// the worker binds the arena handle here and resumes in a later slice.
    ///
    /// # Errors
    ///
    /// Rejects measured jobs (which require an issued activation capability),
    /// stale/wrong generations, a second activation, a sealed parse, or an
    /// arena job already live or pending reclamation.
    pub fn activate_active(
        &mut self,
        token: ParseToken,
        arena_job: ArenaJobId,
    ) -> Result<(), Error> {
        self.validate_forecast_job_activation(token, arena_job)?;
        self.active.as_mut().ok_or(Error::NoActiveParse)?.arena_job = Some(arena_job);
        Ok(())
    }

    pub(crate) fn validate_forecast_job_activation(
        &self,
        token: ParseToken,
        arena_job: ArenaJobId,
    ) -> Result<(), Error> {
        self.validate_current_token(token)?;
        let active = self.active.ok_or(Error::NoActiveParse)?;
        if matches!(active.execution, ParseExecution::Measured { .. }) {
            return Err(Error::MeasuredParseRequiresPermit);
        }
        if active.arena_job.is_some() {
            return Err(Error::ParseAlreadyActivated);
        }
        if active.sealed.is_some() {
            return Err(Error::ParseAlreadySealed);
        }
        if self.arena_job_is_owned(arena_job) {
            return Err(Error::ArenaJobAlreadyOwned(arena_job));
        }
        Ok(())
    }

    /// Runs at most one configured parser slice.
    ///
    /// Supersession is charged as a transition. If its cleanup ticket cannot
    /// fit the fixed reclaim queue, the active parse is left untouched and a
    /// backpressure status is returned.
    ///
    /// # Errors
    ///
    /// Returns an error only if an internally generated reclaim target would
    /// duplicate an outstanding target.
    pub fn run_parse_slice(&mut self) -> Result<ParseSliceReport, Error> {
        if self.issued_job_activation.is_some() {
            return Err(Error::JobActivationSliceAlreadyIssued);
        }
        if self.issued_parse_slice.is_some() {
            return Err(Error::ParseSliceAlreadyIssued);
        }
        if let Some(report) = self.promote_queued_for_slice()? {
            return Ok(report);
        }
        let mut audit = Audit::ZERO;

        let mut progressed_units = 0_u64;
        let mut worked_on = None;
        let mut worked_arena_job = None;
        if let Some(active) = self.active.as_mut() {
            let ParseExecution::Forecast {
                unit_audit,
                remaining_units,
            } = &mut active.execution
            else {
                return Err(Error::MeasuredParseRequiresPermit);
            };
            while active.arena_job.is_some()
                && *remaining_units > 0
                && audit.can_add(*unit_audit, self.limits)
            {
                audit = audit
                    .checked_add(*unit_audit)
                    .ok_or(Error::InvariantViolation(
                        "validated parser audit overflowed",
                    ))?;
                *remaining_units -= 1;
                progressed_units = progressed_units
                    .checked_add(1)
                    .ok_or(Error::CounterExhausted("parse slice work units"))?;
                worked_on = Some(active.token);
                worked_arena_job = active.arena_job;
            }
        }

        let status = self.active.map_or(ParseSliceStatus::Idle, |active| {
            if active.sealed.is_some() {
                ParseSliceStatus::SealedReadyToCommit
            } else if active.arena_job.is_none() {
                ParseSliceStatus::AwaitingActivation
            } else if active.execution.is_ready() {
                ParseSliceStatus::ReadyToSeal
            } else {
                ParseSliceStatus::Pending
            }
        });
        debug_assert!(audit.fits(self.limits));
        Ok(ParseSliceReport {
            status,
            audit,
            progressed_units,
            worked_on,
            worked_arena_job,
            cancelled: None,
            promoted: None,
        })
    }

    /// Issues one hard-limited capability for real parser/arena work.
    ///
    /// No parse progress changes here. Supersession is resolved first and is
    /// returned as a status-only slice, preserving latest-wins cancellation.
    ///
    /// # Errors
    ///
    /// Rejects a second outstanding permit, identity exhaustion, or an
    /// impossible reclaim transition while promoting queued work.
    pub fn issue_measured_parse_slice(&mut self) -> Result<ParseWorkAvailability, Error> {
        if self.issued_job_activation.is_some() {
            return Err(Error::JobActivationSliceAlreadyIssued);
        }
        if self.issued_parse_slice.is_some() {
            return Err(Error::ParseSliceAlreadyIssued);
        }
        if let Some(report) = self.promote_queued_for_slice()? {
            return Ok(ParseWorkAvailability::Status(report));
        }
        let Some(active) = self.active else {
            return Ok(ParseWorkAvailability::Status(Self::status_only_report(
                ParseSliceStatus::Idle,
            )));
        };
        let Some(arena_job) = active.arena_job else {
            return Ok(ParseWorkAvailability::Status(Self::status_only_report(
                ParseSliceStatus::AwaitingActivation,
            )));
        };
        if active.sealed.is_some() {
            return Ok(ParseWorkAvailability::Status(Self::status_only_report(
                ParseSliceStatus::SealedReadyToCommit,
            )));
        }
        let ParseExecution::Measured { complete, poisoned } = active.execution else {
            return Err(Error::ForecastParseRequiresLegacyRunner);
        };
        if poisoned {
            return Err(Error::MeasuredParsePoisoned);
        }
        if complete {
            return Ok(ParseWorkAvailability::Status(Self::status_only_report(
                ParseSliceStatus::ReadyToSeal,
            )));
        }
        let sequence = self
            .last_parse_slice_sequence
            .checked_add(1)
            .ok_or(Error::CounterExhausted("measured parse slice sequence"))?;
        let permit = ParseWorkPermit {
            sequence,
            token: active.token,
            arena_job,
            limits: self.limits,
        };
        self.last_parse_slice_sequence = sequence;
        self.issued_parse_slice = Some(permit);
        Ok(ParseWorkAvailability::Work(permit))
    }

    /// Consumes an outstanding permit and fail-closes its candidate without
    /// claiming parser progress.
    ///
    /// Execution coordinators use this when a job fails after possible
    /// physical mutation, reports inconsistent completion, or is bound to the
    /// wrong local object. [`retry_poisoned_measured_parse`](Self::retry_poisoned_measured_parse)
    /// then supplies the bounded reclaim/retry path.
    ///
    /// # Errors
    ///
    /// Rejects stale, replayed, or mismatched permits without changing state.
    pub fn poison_measured_parse_slice(&mut self, permit: ParseWorkPermit) -> Result<(), Error> {
        self.validate_current_token(permit.token)?;
        let issued = self.issued_parse_slice.ok_or(Error::NoIssuedParseSlice)?;
        if issued != permit {
            return Err(Error::ParseSlicePermitMismatch);
        }
        let active = self.active.ok_or(Error::NoActiveParse)?;
        let active_job = active.arena_job.ok_or(Error::ParseNotActivated)?;
        if active_job != permit.arena_job {
            return Err(Error::ArenaJobMismatch {
                expected: active_job,
                provided: permit.arena_job,
            });
        }
        if !matches!(
            active.execution,
            ParseExecution::Measured {
                complete: false,
                poisoned: false
            }
        ) {
            return Err(Error::MeasuredParsePoisoned);
        }
        self.issued_parse_slice = None;
        self.active.as_mut().ok_or(Error::NoActiveParse)?.execution = ParseExecution::Measured {
            complete: false,
            poisoned: true,
        };
        Ok(())
    }

    /// Adopts work measured after a real parser/arena slice.
    ///
    /// The outstanding capability is consumed before receipt validation:
    /// physical work may already have occurred, so a bad receipt can never be
    /// replayed. A rejected real receipt poisons the candidate without marking
    /// it complete; supersession then reclaims its physical job graph.
    ///
    /// # Errors
    ///
    /// Rejects stale/replayed permits, zero or impossible progress, reclaim
    /// claims, and any audit dimension beyond the issued hard limit.
    pub fn adopt_measured_parse_slice(
        &mut self,
        permit: ParseWorkPermit,
        receipt: MeasuredParseReceipt,
    ) -> Result<MeasuredParseReport, Error> {
        if permit.token.generation != self.latest_generation {
            return Err(Error::StaleParseGeneration {
                provided: permit.token.generation,
                latest: self.latest_generation,
            });
        }
        let issued = self.issued_parse_slice.ok_or(Error::NoIssuedParseSlice)?;
        if issued != permit {
            return Err(Error::ParseSlicePermitMismatch);
        }
        // A matching permit is one-shot even when the worker violated it.
        self.issued_parse_slice = None;

        self.validate_current_token(permit.token)?;
        let active = self.active.ok_or(Error::NoActiveParse)?;
        let active_job = active.arena_job.ok_or(Error::ParseNotActivated)?;
        if active_job != permit.arena_job {
            return Err(Error::ArenaJobMismatch {
                expected: active_job,
                provided: permit.arena_job,
            });
        }
        if !matches!(
            active.execution,
            ParseExecution::Measured {
                complete: false,
                poisoned: false
            }
        ) {
            return Err(Error::MeasuredParsePoisoned);
        }
        if let Err(error) = validate_measured_receipt(permit, receipt) {
            let active = self.active.as_mut().ok_or(Error::NoActiveParse)?;
            active.execution = ParseExecution::Measured {
                complete: false,
                poisoned: true,
            };
            return Err(error);
        }
        let active = self.active.as_mut().ok_or(Error::NoActiveParse)?;
        active.execution = ParseExecution::Measured {
            complete: receipt.complete,
            poisoned: false,
        };
        let status = if receipt.complete {
            ParseSliceStatus::ReadyToSeal
        } else {
            ParseSliceStatus::Pending
        };
        Ok(MeasuredParseReport {
            status,
            audit: receipt.audit,
            progressed_units: receipt.progressed_units,
            worked_on: permit.token,
            worked_arena_job: permit.arena_job,
        })
    }

    /// Cancels a receipt-poisoned physical job and schedules a fresh measured
    /// attempt for the exact same `(S, source_root, operation_count)` snapshot.
    ///
    /// This is the no-new-edit recovery path after an allocator or parser
    /// violates a permit. It enqueues one normal bounded reclaim ticket and
    /// advances only `ParseGeneration`; source `S` and adopted grammar `G` do
    /// not change.
    ///
    /// # Errors
    ///
    /// Rejects a non-poisoned attempt, an already queued newer source, reclaim
    /// backpressure, or generation exhaustion. State is unchanged on error.
    pub fn retry_poisoned_measured_parse(&mut self) -> Result<ParseSliceReport, Error> {
        if self.issued_job_activation.is_some() {
            return Err(Error::JobActivationSliceAlreadyIssued);
        }
        if self.issued_parse_slice.is_some() {
            return Err(Error::ParseSliceAlreadyIssued);
        }
        if self.queued.is_some() {
            return Err(Error::PoisonedRetryHasQueuedSource);
        }
        let active = self.active.ok_or(Error::NoActiveParse)?;
        if !matches!(
            active.execution,
            ParseExecution::Measured {
                complete: false,
                poisoned: true
            }
        ) {
            return Err(Error::MeasuredParseNotPoisoned);
        }
        let target = active.cancel_target().ok_or(Error::ParseNotActivated)?;
        self.reclaim.preflight(&[target])?;
        let next_generation = self
            .latest_generation
            .0
            .checked_add(1)
            .map(ParseGeneration)
            .ok_or(Error::CounterExhausted("parse generation"))?;
        let replacement_token = ParseToken {
            generation: next_generation,
            target_revision: active.token.target_revision,
        };
        let replacement = ParsePlan {
            token: replacement_token,
            source_root: active.source_root,
            operation_count: active.operation_count,
            execution: ParseExecution::Measured {
                complete: false,
                poisoned: false,
            },
            arena_job: None,
            sealed: None,
        };
        self.reclaim.enqueue_all(&[target]);
        self.latest_generation = next_generation;
        self.active = Some(replacement);
        Ok(ParseSliceReport {
            status: ParseSliceStatus::AwaitingActivation,
            audit: CANCELLATION_AUDIT,
            progressed_units: 0,
            worked_on: None,
            worked_arena_job: None,
            cancelled: Some(active.token),
            promoted: Some(replacement_token),
        })
    }

    /// Atomically substitutes the active mutable arena job with its frozen
    /// output root and records the visible-page count.
    ///
    /// The arena side must first consume `arena_job` and return `arena_root` in
    /// the same worker transaction. Naming both handles here prevents a root
    /// from a different job being published accidentally.
    ///
    /// # Errors
    ///
    /// Rejects stale/wrong generations, a mismatched job, unactivated,
    /// unfinished, or already sealed parses, duplicate roots, and non-current
    /// source snapshots.
    pub fn seal_active(
        &mut self,
        token: ParseToken,
        arena_job: ArenaJobId,
        arena_root: ArenaRootId,
        visible_pages: u32,
    ) -> Result<(), Error> {
        self.validate_current_token(token)?;
        let active = self.active.ok_or(Error::NoActiveParse)?;
        match active.execution {
            ParseExecution::Forecast {
                remaining_units, ..
            } if remaining_units != 0 => {
                return Err(Error::ParseNotReady { remaining_units });
            }
            ParseExecution::Measured { poisoned: true, .. } => {
                return Err(Error::MeasuredParsePoisoned);
            }
            ParseExecution::Measured {
                complete: false, ..
            } => return Err(Error::MeasuredParseNotComplete),
            ParseExecution::Forecast { .. } | ParseExecution::Measured { .. } => {}
        }
        let expected_job = active.arena_job.ok_or(Error::ParseNotActivated)?;
        if arena_job != expected_job {
            return Err(Error::ArenaJobMismatch {
                expected: expected_job,
                provided: arena_job,
            });
        }
        if active.sealed.is_some() {
            return Err(Error::ParseAlreadySealed);
        }
        self.validate_parse_snapshot(active)?;
        if self.arena_root_is_owned(arena_root) {
            return Err(Error::ArenaRootAlreadyOwned(arena_root));
        }
        let active = self.active.as_mut().ok_or(Error::NoActiveParse)?;
        active.arena_job = None;
        active.sealed = Some(SealedCandidate {
            arena_root,
            visible_pages,
        });
        Ok(())
    }

    /// Atomically adopts a sealed current generation and publishes a root
    /// lease. The delta may legally jump from old `G` directly to current `S`.
    ///
    /// # Errors
    ///
    /// Rejects stale/unsealed parses, source snapshot mismatch, remote-ID
    /// exhaustion, or reclaim backpressure while retiring a superseded offer.
    pub fn commit_sealed(&mut self, token: ParseToken) -> Result<GrammarDelta, Error> {
        self.validate_current_token(token)?;
        let active = self.active.ok_or(Error::NoActiveParse)?;
        self.validate_parse_snapshot(active)?;
        let sealed = active.sealed.ok_or(Error::ParseNotSealed)?;
        let arena_root = sealed.arena_root;
        let next_remote_root = self
            .last_remote_root
            .0
            .checked_add(1)
            .map(RemoteRootId)
            .ok_or(Error::CounterExhausted("remote root id"))?;

        let mut next_roots = self.roots.clone();
        for root in &mut next_roots {
            root.worker_current = false;
            root.offered = false;
        }
        let (mut next_roots, retired) = prune_roots(next_roots);
        self.reclaim.preflight(&retired)?;
        if next_roots.len() + 1 > MAX_RETAINED_ROOTS {
            return Err(Error::InvariantViolation("remote root cap exceeded"));
        }

        let lease = RootLease {
            remote_root: next_remote_root,
            parse_generation: token.generation,
            source_revision: active.token.target_revision,
        };
        next_roots.push(RootRecord {
            lease,
            arena_root,
            visible_pages: sealed.visible_pages,
            worker_current: true,
            offered: true,
            acknowledged: false,
        });

        self.reclaim.enqueue_all(&retired);
        self.roots = next_roots;
        self.last_remote_root = next_remote_root;
        let base_grammar_revision = self.grammar_revision;
        self.grammar_revision = GrammarRevision(active.token.target_revision.0);
        self.adopted_source_root = active.source_root;
        self.active = None;
        debug_assert!(self.queued.is_none());

        Ok(GrammarDelta {
            base_grammar_revision,
            target_source_revision: token.target_revision,
            parse_generation: token.generation,
            source_root: active.source_root,
            root_lease: lease,
        })
    }

    /// Acknowledges the newest offered root and atomically releases the older
    /// acknowledged UI root.
    ///
    /// # Errors
    ///
    /// Rejects stale/released leases or reclaim backpressure. State is
    /// unchanged when the previous acknowledged root cannot be queued.
    pub fn acknowledge_root(&mut self, lease: RootLease) -> Result<AckReceipt, Error> {
        let target = self.live_root(lease)?;
        if target.acknowledged {
            return Ok(AckReceipt {
                acknowledged: lease,
                released_previous: None,
            });
        }
        debug_assert!(target.offered);
        let released_previous = self
            .roots
            .iter()
            .find(|root| root.acknowledged)
            .map(|root| root.lease)
            .filter(|previous| *previous != lease);

        let mut next_roots = self.roots.clone();
        for root in &mut next_roots {
            root.acknowledged = false;
            root.offered = false;
            if root.lease == lease {
                root.acknowledged = true;
            }
        }
        let (next_roots, retired) = prune_roots(next_roots);
        self.reclaim.preflight(&retired)?;
        self.reclaim.enqueue_all(&retired);
        self.roots = next_roots;
        Ok(AckReceipt {
            acknowledged: lease,
            released_previous,
        })
    }

    /// Releases UI access to an offered or acknowledged root.
    ///
    /// The worker-current grammar root remains internally retained until a
    /// later commit, but the released lease immediately becomes invalid for
    /// remote queries.
    ///
    /// # Errors
    ///
    /// Rejects stale/released leases or reclaim backpressure.
    pub fn release_root(&mut self, lease: RootLease) -> Result<(), Error> {
        self.live_root(lease)?;
        let mut next_roots = self.roots.clone();
        for root in &mut next_roots {
            if root.lease == lease {
                root.offered = false;
                root.acknowledged = false;
            }
        }
        let (next_roots, retired) = prune_roots(next_roots);
        self.reclaim.preflight(&retired)?;
        self.reclaim.enqueue_all(&retired);
        self.roots = next_roots;
        Ok(())
    }

    /// Returns a bounded page-handle batch for a live offered or acknowledged
    /// lease. Querying is valid before acknowledgement so the client can
    /// materialize visible pages and then ack the root atomically.
    ///
    /// # Errors
    ///
    /// Rejects stale/released leases or limits too small for one page handle.
    pub fn query_visible_pages(
        &self,
        lease: RootLease,
        first_page: u32,
        requested_pages: u32,
    ) -> Result<VisiblePageBatch, Error> {
        let root = self.live_root(lease)?;
        if requested_pages == 0 || first_page >= root.visible_pages {
            return Ok(VisiblePageBatch {
                lease,
                pages: Vec::new(),
                next_page: None,
                audit: Audit::ZERO,
            });
        }
        if !PAGE_HANDLE_AUDIT.fits(self.limits) {
            return Err(Error::VisiblePageUnitExceedsSliceLimits);
        }
        let wanted = requested_pages.min(root.visible_pages - first_page);
        let capped = wanted.min(MAX_VISIBLE_PAGES_PER_QUERY);
        let mut audit = Audit::ZERO;
        let mut page_count = 0_u32;
        while page_count < capped && audit.can_add(PAGE_HANDLE_AUDIT, self.limits) {
            audit = audit
                .checked_add(PAGE_HANDLE_AUDIT)
                .ok_or(Error::InvariantViolation(
                    "bounded visible-page audit overflowed",
                ))?;
            page_count += 1;
        }
        let capacity = usize::try_from(page_count)
            .map_err(|_| Error::InvariantViolation("visible-page cap does not fit usize"))?;
        let mut pages = Vec::with_capacity(capacity);
        for offset in 0..page_count {
            pages.push(VisiblePageHandle {
                remote_root: lease.remote_root,
                parse_generation: lease.parse_generation,
                page_index: first_page + offset,
            });
        }
        let next_page = (page_count < wanted).then_some(first_page + page_count);
        debug_assert!(audit.fits(self.limits));
        Ok(VisiblePageBatch {
            lease,
            pages,
            next_page,
            audit,
        })
    }

    /// Drives physical arena cleanup through one bounded scheduler slice.
    ///
    /// # Errors
    ///
    /// Rejects an arena that does not own the ticket it was issued or reports
    /// any audit dimension beyond the remaining slice limit.
    pub fn run_reclaim_slice<R: Reclaimer>(
        &mut self,
        reclaimer: &mut R,
    ) -> Result<ReclaimSliceReport, Error> {
        let mut audit = Audit::ZERO;
        let mut completed_tickets = 0_usize;
        let mut stalled = false;

        loop {
            let Some(ticket) = self.reclaim.tickets.front().copied() else {
                break;
            };
            let remaining = self
                .limits
                .remaining_after(audit)
                .ok_or(Error::InvariantViolation(
                    "accepted reclaim audit exceeded slice limits",
                ))?;
            if remaining.transitions == 0 || remaining.reclaimed_nodes == 0 {
                break;
            }
            let outcome = reclaimer.reclaim(ReclaimRequest {
                ticket,
                limits: remaining,
            });
            let ReclaimOutcome::Progress {
                audit: step_audit,
                complete,
            } = outcome
            else {
                return Err(Error::ReclaimerRejected(ticket.target));
            };
            if !step_audit.fits(remaining) {
                return Err(Error::ReclaimReceiptExceedsSliceLimits {
                    audit: step_audit,
                    remaining,
                });
            }
            if step_audit.is_zero() && !complete {
                stalled = true;
                break;
            }
            audit = audit
                .checked_add(step_audit)
                .ok_or(Error::InvariantViolation(
                    "validated reclaim audit overflowed",
                ))?;
            if complete {
                let popped = self.reclaim.tickets.pop_front();
                debug_assert_eq!(popped, Some(ticket));
                completed_tickets += 1;
            }
        }
        debug_assert!(audit.fits(self.limits));
        Ok(ReclaimSliceReport {
            audit,
            completed_tickets,
            remaining_tickets: self.reclaim.tickets.len(),
            stalled,
        })
    }

    #[must_use]
    pub const fn source_revision(&self) -> SourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn grammar_revision(&self) -> GrammarRevision {
        self.grammar_revision
    }

    #[must_use]
    pub const fn source_root(&self) -> SourceRootId {
        self.source_root
    }

    #[must_use]
    pub const fn adopted_source_root(&self) -> SourceRootId {
        self.adopted_source_root
    }

    #[must_use]
    pub const fn applied_operation_count(&self) -> u64 {
        self.operation_count
    }

    #[must_use]
    pub const fn latest_parse_generation(&self) -> ParseGeneration {
        self.latest_generation
    }

    #[must_use]
    pub fn active_parse_token(&self) -> Option<ParseToken> {
        self.active.map(|active| active.token)
    }

    #[must_use]
    pub fn queued_parse_token(&self) -> Option<ParseToken> {
        self.queued.map(|queued| queued.token)
    }

    #[must_use]
    pub fn parse_slot_count(&self) -> usize {
        usize::from(self.active.is_some()) + usize::from(self.queued.is_some())
    }

    #[must_use]
    pub fn retained_root_count(&self) -> usize {
        self.roots.len()
    }

    #[must_use]
    pub fn offered_root_lease(&self) -> Option<RootLease> {
        self.roots
            .iter()
            .find(|root| root.offered)
            .map(|root| root.lease)
    }

    #[must_use]
    pub fn acknowledged_root_lease(&self) -> Option<RootLease> {
        self.roots
            .iter()
            .find(|root| root.acknowledged)
            .map(|root| root.lease)
    }

    #[must_use]
    pub fn pending_reclaim_tickets(&self) -> usize {
        self.reclaim.tickets.len()
    }

    #[must_use]
    pub const fn slice_limits(&self) -> SliceLimits {
        self.limits
    }

    /// Checks the scheduler's structural invariants without inspecting arena
    /// memory.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvariantViolation`] for any impossible state.
    pub fn validate_invariants(&self) -> Result<(), Error> {
        if self.grammar_revision.0 > self.source_revision.0 {
            return Err(Error::InvariantViolation("G is ahead of S"));
        }
        self.validate_parse_invariants()?;
        self.validate_root_invariants()?;
        self.validate_reclaim_invariants()
    }

    fn validate_parse_invariants(&self) -> Result<(), Error> {
        if self.active.is_none() && self.queued.is_some() {
            return Err(Error::InvariantViolation(
                "queued parse exists without active parse",
            ));
        }
        if self.parse_slot_count() > 2 {
            return Err(Error::InvariantViolation("more than two parse slots"));
        }
        if let Some(latest) = self.queued.or(self.active) {
            if latest.token.generation != self.latest_generation {
                return Err(Error::InvariantViolation(
                    "latest parse descriptor has wrong generation",
                ));
            }
        }
        if let Some(queued) = self.queued {
            if queued.token.target_revision != self.source_revision
                || queued.source_root != self.source_root
                || queued.operation_count != self.operation_count
            {
                return Err(Error::InvariantViolation(
                    "queued parse is not the cumulative latest source",
                ));
            }
            if queued.arena_job.is_some() || queued.sealed.is_some() {
                return Err(Error::InvariantViolation(
                    "queued descriptor owns parser-arena state",
                ));
            }
        }
        if let Some(active) = self.active {
            if active.sealed.is_some()
                && (active.arena_job.is_some() || !active.execution.is_ready())
            {
                return Err(Error::InvariantViolation(
                    "sealed parse still owns its job or is unfinished",
                ));
            }
        }
        if self.issued_job_activation.is_some() && self.issued_parse_slice.is_some() {
            return Err(Error::InvariantViolation(
                "activation and parser permits are simultaneously live",
            ));
        }
        if let Some(permit) = self.issued_job_activation {
            let active = self.active.ok_or(Error::InvariantViolation(
                "activation permit exists without an active parse",
            ))?;
            if self.queued.is_some()
                || active.token != permit.token
                || active.arena_job.is_some()
                || active.sealed.is_some()
                || !matches!(
                    active.execution,
                    ParseExecution::Measured {
                        complete: false,
                        poisoned: false
                    }
                )
                || permit.limits != self.limits
                || permit.sequence > self.last_parse_slice_sequence
            {
                return Err(Error::InvariantViolation(
                    "issued activation permit does not match live parser state",
                ));
            }
        }
        if let Some(permit) = self.issued_parse_slice {
            let active = self.active.ok_or(Error::InvariantViolation(
                "measured permit exists without an active parse",
            ))?;
            if self.queued.is_some()
                || active.token != permit.token
                || active.arena_job != Some(permit.arena_job)
                || active.sealed.is_some()
                || !matches!(
                    active.execution,
                    ParseExecution::Measured {
                        complete: false,
                        poisoned: false
                    }
                )
                || permit.limits != self.limits
                || permit.sequence > self.last_parse_slice_sequence
            {
                return Err(Error::InvariantViolation(
                    "issued measured permit does not match live parser state",
                ));
            }
        }
        Ok(())
    }

    fn validate_root_invariants(&self) -> Result<(), Error> {
        if self.roots.is_empty() || self.roots.len() > MAX_RETAINED_ROOTS {
            return Err(Error::InvariantViolation("invalid retained-root count"));
        }
        if self.roots.iter().filter(|root| root.worker_current).count() != 1 {
            return Err(Error::InvariantViolation(
                "there must be exactly one worker-current root",
            ));
        }
        if self.roots.iter().filter(|root| root.offered).count() > 1
            || self.roots.iter().filter(|root| root.acknowledged).count() > 1
        {
            return Err(Error::InvariantViolation(
                "more than one offered or acknowledged root",
            ));
        }
        let Some(current) = self.roots.iter().find(|root| root.worker_current) else {
            return Err(Error::InvariantViolation("worker-current root disappeared"));
        };
        if current.lease.source_revision.0 != self.grammar_revision.0 {
            return Err(Error::InvariantViolation(
                "worker root does not represent adopted G",
            ));
        }
        for (index, root) in self.roots.iter().enumerate() {
            if !root.retained() {
                return Err(Error::InvariantViolation("unretained root record"));
            }
            if self
                .roots
                .iter()
                .skip(index + 1)
                .any(|other| other.arena_root == root.arena_root)
            {
                return Err(Error::InvariantViolation("duplicate live arena root"));
            }
        }
        Ok(())
    }

    fn validate_reclaim_invariants(&self) -> Result<(), Error> {
        if self.reclaim.tickets.len() > MAX_RECLAIM_TICKETS {
            return Err(Error::InvariantViolation("reclaim queue exceeded cap"));
        }
        for (index, ticket) in self.reclaim.tickets.iter().enumerate() {
            if self
                .reclaim
                .tickets
                .iter()
                .skip(index + 1)
                .any(|other| other.target.handle == ticket.target.handle)
            {
                return Err(Error::InvariantViolation("duplicate reclaim target"));
            }
            if matches!(
                ticket.target.handle,
                ReclaimHandle::Root(arena_root)
                    if self.roots.iter().any(|root| root.arena_root == arena_root)
            ) {
                return Err(Error::InvariantViolation(
                    "live root is also queued for reclamation",
                ));
            }
        }
        if let Some(active_job) = self.active.and_then(|active| active.arena_job) {
            if self.reclaim.contains_handle(ReclaimHandle::Job(active_job)) {
                return Err(Error::InvariantViolation(
                    "active arena job is also queued for reclamation",
                ));
            }
        }
        if let Some(sealed_root) = self
            .active
            .and_then(|active| active.sealed.map(|sealed| sealed.arena_root))
        {
            if self.roots.iter().any(|root| root.arena_root == sealed_root)
                || self
                    .reclaim
                    .contains_handle(ReclaimHandle::Root(sealed_root))
            {
                return Err(Error::InvariantViolation(
                    "sealed candidate root has conflicting ownership",
                ));
            }
        }
        Ok(())
    }

    fn status_only_report(status: ParseSliceStatus) -> ParseSliceReport {
        ParseSliceReport {
            status,
            audit: Audit::ZERO,
            progressed_units: 0,
            worked_on: None,
            worked_arena_job: None,
            cancelled: None,
            promoted: None,
        }
    }

    fn promote_queued_for_slice(&mut self) -> Result<Option<ParseSliceReport>, Error> {
        let Some(queued) = self.queued else {
            return Ok(None);
        };
        let active = self.active.ok_or(Error::InvariantViolation(
            "queued parse exists without an active parse",
        ))?;
        let target = active.cancel_target();
        if let Some(target) = target {
            if self.reclaim.available() == 0 {
                return Ok(Some(Self::status_only_report(
                    ParseSliceStatus::ReclaimBackpressure,
                )));
            }
            self.reclaim.preflight(&[target])?;
        }
        debug_assert!(CANCELLATION_AUDIT.fits(self.limits));
        if let Some(target) = target {
            self.reclaim.enqueue_all(&[target]);
        }
        self.active = Some(queued);
        self.queued = None;
        Ok(Some(ParseSliceReport {
            status: ParseSliceStatus::AwaitingActivation,
            audit: CANCELLATION_AUDIT,
            progressed_units: 0,
            worked_on: None,
            worked_arena_job: None,
            cancelled: Some(active.token),
            promoted: Some(queued.token),
        }))
    }

    fn validate_operation(&self, operation: SourceOperation) -> Result<(), Error> {
        if operation.base_revision != self.source_revision {
            return Err(Error::SourceRevisionMismatch {
                expected: self.source_revision,
                provided: operation.base_revision,
            });
        }
        let expected_target = operation
            .base_revision
            .0
            .checked_add(1)
            .map(SourceRevision)
            .ok_or(Error::CounterExhausted("source revision"))?;
        if operation.target_revision != expected_target {
            return Err(Error::SourceTargetNotNext {
                base: operation.base_revision,
                target: operation.target_revision,
            });
        }
        if operation.base_root != self.source_root {
            return Err(Error::SourceRootMismatch {
                expected: self.source_root,
                provided: operation.base_root,
            });
        }
        Ok(())
    }

    fn validate_parse_spec(&self, spec: ParseSpec) -> Result<(), Error> {
        if spec.work_units == 0 {
            return Err(Error::InvalidParseSpec("work_units must be nonzero"));
        }
        if spec.unit_audit.transitions == 0 {
            return Err(Error::InvalidParseSpec(
                "each resumable parser unit must charge a transition",
            ));
        }
        if spec.unit_audit.reclaimed_nodes != 0 {
            return Err(Error::InvalidParseSpec(
                "parser work cannot claim arena reclamation",
            ));
        }
        if !spec.unit_audit.fits(self.limits) {
            return Err(Error::ParseUnitExceedsSliceLimits {
                unit: spec.unit_audit,
                limits: self.limits,
            });
        }
        Ok(())
    }

    fn validate_current_token(&self, token: ParseToken) -> Result<(), Error> {
        if token.generation != self.latest_generation {
            return Err(Error::StaleParseGeneration {
                provided: token.generation,
                latest: self.latest_generation,
            });
        }
        let active = self.active.ok_or(Error::NoActiveParse)?;
        if active.token != token {
            return Err(Error::ParseTokenMismatch {
                expected: active.token,
                provided: token,
            });
        }
        Ok(())
    }

    fn validate_parse_snapshot(&self, active: ParsePlan) -> Result<(), Error> {
        if active.token.target_revision != self.source_revision {
            return Err(Error::ParseTargetBehindSource {
                target: active.token.target_revision,
                current: self.source_revision,
            });
        }
        if active.source_root != self.source_root || active.operation_count != self.operation_count
        {
            return Err(Error::ParseSourceRootMismatch {
                parsed: active.source_root,
                current: self.source_root,
            });
        }
        Ok(())
    }

    fn live_root(&self, lease: RootLease) -> Result<RootRecord, Error> {
        self.roots
            .iter()
            .copied()
            .find(|root| root.lease == lease && root.remotely_live())
            .ok_or(Error::StaleRootLease {
                remote_root: lease.remote_root,
                parse_generation: lease.parse_generation,
            })
    }

    #[must_use]
    fn arena_root_is_owned(&self, arena_root: ArenaRootId) -> bool {
        self.roots.iter().any(|root| root.arena_root == arena_root)
            || self
                .reclaim
                .contains_handle(ReclaimHandle::Root(arena_root))
            || self.active.is_some_and(|active| {
                active
                    .sealed
                    .is_some_and(|sealed| sealed.arena_root == arena_root)
            })
    }

    #[must_use]
    fn arena_job_is_owned(&self, arena_job: ArenaJobId) -> bool {
        self.reclaim.contains_handle(ReclaimHandle::Job(arena_job))
            || self
                .active
                .is_some_and(|active| active.arena_job == Some(arena_job))
    }
}

fn validate_limits(limits: SliceLimits) -> Result<(), Error> {
    if limits.transitions == 0 {
        return Err(Error::InvalidSliceLimits(
            "transitions must permit at least one state step",
        ));
    }
    if limits.reclaimed_nodes == 0 {
        return Err(Error::InvalidSliceLimits(
            "reclaimed_nodes must permit iterative cleanup",
        ));
    }
    if limits.index_nodes == 0 {
        return Err(Error::InvalidSliceLimits(
            "index_nodes must permit visible-page lookup",
        ));
    }
    if limits.allocated_bytes < VISIBLE_PAGE_HANDLE_LOGICAL_BYTES {
        return Err(Error::InvalidSliceLimits(
            "allocated_bytes must fit one visible-page handle",
        ));
    }
    Ok(())
}

fn validate_measured_receipt(
    permit: ParseWorkPermit,
    receipt: MeasuredParseReceipt,
) -> Result<(), Error> {
    if !receipt.audit.fits(permit.limits) {
        return Err(Error::MeasuredParseReceiptExceedsLimits {
            audit: receipt.audit,
            limits: permit.limits,
        });
    }
    if receipt.audit.reclaimed_nodes != 0 {
        return Err(Error::InvalidMeasuredParseReceipt(
            "parser work cannot claim arena reclamation",
        ));
    }
    if receipt.progressed_units == 0 && !receipt.audit.is_zero() {
        return Err(Error::InvalidMeasuredParseReceipt(
            "zero progress must report zero work",
        ));
    }
    if receipt.audit.transitions < receipt.progressed_units {
        return Err(Error::InvalidMeasuredParseReceipt(
            "each progressed parser unit must charge a transition",
        ));
    }
    Ok(())
}

fn validate_job_activation_receipt(permit: JobActivationPermit, audit: Audit) -> Result<(), Error> {
    if !audit.fits(permit.limits) {
        return Err(Error::JobActivationReceiptExceedsLimits {
            audit,
            limits: permit.limits,
        });
    }
    if audit.is_zero() || audit.transitions == 0 {
        return Err(Error::InvalidJobActivationReceipt(
            "physical job activation must charge nonzero transition work",
        ));
    }
    if audit.reclaimed_nodes != 0 {
        return Err(Error::InvalidJobActivationReceipt(
            "job activation cannot claim arena reclamation",
        ));
    }
    Ok(())
}

fn prune_roots(roots: Vec<RootRecord>) -> (Vec<RootRecord>, Vec<ReclaimTarget>) {
    let mut retained = Vec::with_capacity(roots.len());
    let mut retired = Vec::new();
    for root in roots {
        if root.retained() {
            retained.push(root);
        } else {
            retired.push(ReclaimTarget {
                handle: ReclaimHandle::Root(root.arena_root),
                reason: ReclaimReason::RemoteRootRetired,
            });
        }
    }
    (retained, retired)
}

//! Exact, fuelled multi-revision restart and suffix-convergence proof.
//!
//! This module deliberately proves less than a complete incremental parser.
//! It records edits by applying them to the real [`PersistentSource`], maps a
//! current checkpoint backwards through every skipped revision retained in a
//! hard fixed window, and accepts a suffix only when all exact checkpoint
//! components agree. Window overflow discards continuity and forces a fresh
//! restart. It never treats a hash, equal length, or equal endpoints as an
//! identity proof.
//!
//! The current frontier does not yet expose splice-persistent logical roots and
//! the output arena is not yet connected to grammar output. Consequently,
//! [`LogicalInputSnapshot::proof_root`] and [`OutputSuffixRoot::fresh`] are
//! capability models for those future immutable roots. A candidate checkpoint
//! owns its old output suffix; the current parse does not. Successful proof
//! authorizes adopting that candidate root. [`LogicalInputSnapshot::from_frontier`]
//! can use a real [`SegmentedLeaf`] root today, but that root cannot yet be
//! spliced across a source edit.

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::frontier::SegmentedLeaf;
use crate::source::{
    Anchor, BufferRetentionMetrics, PersistentSource, SourceError, SourceRootIdentity,
};

/// Maximum work one poll may consume even when the caller offers more fuel.
pub const MAX_CONVERGENCE_POLL_WORK: usize = 4 * 1024;

/// Hard upper bound for exact edits retained between parser checkpoints.
/// It covers the scheduler's 600-edit stall scenario while retaining a small,
/// fixed record allocation. Crossing it deliberately discards continuity and
/// forces a fresh restart.
pub const MAX_RECORDED_CHANGES: usize = 1_024;

/// Integration work that this proof intentionally does not pretend is done.
pub const REMAINING_INTEGRATION_GAP: &str = "the frontier has no splice-persistent multi-leaf logical suffix root; GrammarOutput retains one Arc-backed leaf origin, but proof capabilities and output/input roots must become real arena roots";

/// Preconditions the eventual integrated parser must enforce. Violating any
/// item can create a false convergence even though this comparator is exact.
pub const PROOF_ASSUMPTIONS: &[&str] = &[
    "parser-state atoms are a lossless serialization, never a digest",
    "the dependency snapshot is complete, canonical, and scoped to the candidate suffix",
    "the logical-input identity covers the complete remaining suffix and every suffix edit mints a new root",
    "candidate output-suffix identities are cloned only for the same immutable output root",
    "every source transition in scope is recorded through ChangeHistory::apply_edit",
    "history and snapshot pages move into the bounded lifetime arena before production use",
];

const VALUE_PAGE_ITEMS: usize = 128;

static NEXT_EXACT_ROOT: AtomicU64 = AtomicU64::new(1);

fn mint_exact_root() -> u64 {
    NEXT_EXACT_ROOT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("exact convergence root identity space exhausted"))
}

/// Monotonic revision identity supplied by the scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionId(pub u64);

/// Which side of a changed region owns an otherwise ambiguous boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryAffinity {
    /// The boundary is attached to the source byte following it. This is the
    /// normal choice for the start of a candidate reusable suffix.
    Suffix,
    /// The boundary is attached to the source byte preceding it.
    Prefix,
}

/// One exact source splice between adjacent recorded revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactChange {
    pub from_revision: RevisionId,
    pub to_revision: RevisionId,
    pub from_source: SourceRootIdentity,
    pub to_source: SourceRootIdentity,
    pub old_len: usize,
    pub new_len: usize,
    pub replaced_old: Range<usize>,
    pub inserted_new: Range<usize>,
    /// Logically unchanged old bytes copied into new edit-boundary buffers.
    /// These ranges let stable-byte lineage survive deliberate source
    /// compaction even though the physical [`Anchor`] changes.
    pub compacted_prefix: Range<usize>,
    pub compacted_suffix: Range<usize>,
}

/// Fixed-window exact edit history. Each append invokes the tracked source
/// root's real edit operation, so a record cannot accidentally describe an
/// unrelated equal-length root. Only the latest source root is retained;
/// earlier records contain scalar coordinates and minted identities only.
#[derive(Debug)]
pub struct ChangeHistory {
    latest_revision: RevisionId,
    latest_source: Arc<PersistentSource>,
    changes: VecDeque<ExactChange>,
    continuity_resets: usize,
}

/// Whether one edit remained inside the exact convergence window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryAdvance {
    ContinuityRetained,
    /// The hard record cap was reached. The edit succeeded, but convergence to
    /// every older revision is unavailable and parsing must restart fresh.
    ContinuityReset,
}

/// Bounded ownership receipt for an exact history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeHistoryRetention {
    /// Structurally fixed at one: records never own source roots.
    pub retained_source_roots: usize,
    pub recorded_changes: usize,
    pub record_capacity: usize,
    pub record_storage_bytes: usize,
    pub continuity_resets: usize,
    pub latest_source_buffers: BufferRetentionMetrics,
}

/// Failure while extending an exact history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryError {
    RevisionDidNotAdvance {
        current: RevisionId,
        requested: RevisionId,
    },
    LengthOverflow,
    Source(SourceError),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RevisionDidNotAdvance { current, requested } => write!(
                formatter,
                "revision {requested:?} must be greater than current {current:?}"
            ),
            Self::LengthOverflow => formatter.write_str("edited source length overflowed usize"),
            Self::Source(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<SourceError> for HistoryError {
    fn from(value: SourceError) -> Self {
        Self::Source(value)
    }
}

impl ChangeHistory {
    /// Starts an exact history at one immutable source root.
    #[must_use]
    pub fn new(revision: RevisionId, source: PersistentSource) -> Self {
        Self {
            latest_revision: revision,
            latest_source: Arc::new(source),
            changes: VecDeque::with_capacity(MAX_RECORDED_CHANGES),
            continuity_resets: 0,
        }
    }

    /// Current recorded revision.
    #[must_use]
    pub fn latest_revision(&self) -> RevisionId {
        self.latest_revision
    }

    /// Current exact persistent source root.
    #[must_use]
    pub fn latest_source(&self) -> Arc<PersistentSource> {
        self.latest_source.clone()
    }

    /// Exact bounded-retention receipt. Allocator headers and size-class slack
    /// are excluded from `record_storage_bytes`.
    #[must_use]
    pub fn retention(&self) -> ChangeHistoryRetention {
        ChangeHistoryRetention {
            retained_source_roots: 1,
            recorded_changes: self.changes.len(),
            record_capacity: self.changes.capacity(),
            record_storage_bytes: self.changes.capacity() * std::mem::size_of::<ExactChange>(),
            continuity_resets: self.continuity_resets,
            latest_source_buffers: self.latest_source.buffer_retention(),
        }
    }

    /// Applies and records one scalar-safe edit against the exact current root.
    /// When the fixed record window is full, this still applies the edit but
    /// drops all earlier continuity and reports [`HistoryAdvance::ContinuityReset`].
    ///
    /// # Errors
    ///
    /// Returns an error for a non-increasing revision or an invalid source edit.
    pub fn apply_edit(
        &mut self,
        new_revision: RevisionId,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<HistoryAdvance, HistoryError> {
        if new_revision <= self.latest_revision {
            return Err(HistoryError::RevisionDidNotAdvance {
                current: self.latest_revision,
                requested: new_revision,
            });
        }
        let inserted_end = range
            .start
            .checked_add(replacement.len())
            .ok_or(HistoryError::LengthOverflow)?;
        let old_len = self.latest_source.len_bytes();
        let old_identity = self.latest_source.identity();
        let range_for_change = range.clone();
        let outcome = self.latest_source.edit(range, replacement)?;
        let compacted_prefix = outcome.provenance.compacted_prefix.clone();
        let compacted_suffix = outcome.provenance.compacted_suffix.clone();
        let source = Arc::new(outcome.source);
        let change = ExactChange {
            from_revision: self.latest_revision,
            to_revision: new_revision,
            from_source: old_identity,
            to_source: source.identity(),
            old_len,
            new_len: source.len_bytes(),
            replaced_old: range_for_change.clone(),
            inserted_new: range_for_change.start..inserted_end,
            compacted_prefix,
            compacted_suffix,
        };
        let advance = if self.changes.len() == MAX_RECORDED_CHANGES {
            self.changes.clear();
            self.continuity_resets += 1;
            HistoryAdvance::ContinuityReset
        } else {
            self.changes.push_back(change);
            HistoryAdvance::ContinuityRetained
        };
        self.latest_revision = new_revision;
        self.latest_source = source;
        Ok(advance)
    }

    /// Starts a fuelled map from a boundary in the latest revision to an exact
    /// ancestor revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the current boundary is invalid or splits UTF-8.
    pub fn backward_map(
        &self,
        target_revision: RevisionId,
        current_boundary: usize,
        affinity: BoundaryAffinity,
    ) -> Result<BackwardMapJob<'_>, CheckpointError> {
        BackwardMapJob::new(self, target_revision, current_boundary, affinity)
    }
}

/// Exact source evidence attached to a checkpoint boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceBoundaryWitness {
    /// Stable anchor of the first byte in the candidate suffix.
    NextByte(Anchor),
    /// Exact end of the captured source root. EOF convergence additionally
    /// requires the complete parser/dependency/input/output identity below.
    EndOfFile,
}

#[derive(Debug)]
struct ValuePage<T> {
    values: Box<[T]>,
    next: Option<Arc<ValuePage<T>>>,
}

#[derive(Clone, Debug)]
struct PagedValues<T> {
    head: Option<Arc<ValuePage<T>>>,
    len: usize,
}

impl<T: Copy> PagedValues<T> {
    fn from_slice(values: &[T]) -> Self {
        let mut head = None;
        let mut end = values.len();
        while end != 0 {
            let start = end.saturating_sub(VALUE_PAGE_ITEMS);
            head = Some(Arc::new(ValuePage {
                values: Box::from(&values[start..end]),
                next: head,
            }));
            end = start;
        }
        Self {
            head,
            len: values.len(),
        }
    }

    fn cursor(&self) -> ValueCursor<T> {
        ValueCursor {
            page: self.head.clone(),
            index: 0,
        }
    }
}

#[derive(Debug)]
struct ValueCursor<T> {
    page: Option<Arc<ValuePage<T>>>,
    index: usize,
}

impl<T: Copy> ValueCursor<T> {
    fn next(&mut self) -> Option<T> {
        loop {
            let page = self.page.as_ref()?;
            if let Some(value) = page.values.get(self.index) {
                self.index += 1;
                return Some(*value);
            }
            self.page = page.next.clone();
            self.index = 0;
        }
    }
}

/// Exact serialized parser-state atoms. Values are retained in independently
/// allocated fixed-size pages; comparison is value-exact and fuelled.
#[derive(Clone, Debug)]
pub struct ParserStateSnapshot {
    atoms: PagedValues<u64>,
}

impl ParserStateSnapshot {
    /// Copies state atoms directly into pages of at most 128 entries without a
    /// document-sized intermediate allocation.
    #[must_use]
    pub fn from_atoms(atoms: &[u64]) -> Self {
        Self {
            atoms: PagedValues::from_slice(atoms),
        }
    }

    #[must_use]
    pub fn atom_count(&self) -> usize {
        self.atoms.len
    }
}

/// One canonical dependency generation entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependencyGeneration {
    pub dependency: u64,
    pub generation: u64,
}

/// Canonically ordered exact dependency generations.
#[derive(Clone, Debug)]
pub struct DependencySnapshot {
    entries: PagedValues<DependencyGeneration>,
}

/// Dependency snapshots reject ambiguous ordering and duplicate keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependencyOrderError {
    pub previous: u64,
    pub next: u64,
}

impl fmt::Display for DependencyOrderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "dependency IDs must be strictly increasing: {} then {}",
            self.previous, self.next
        )
    }
}

impl std::error::Error for DependencyOrderError {}

impl DependencySnapshot {
    /// Copies a strictly increasing generation set into bounded pages.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate or decreasing dependency IDs.
    pub fn from_sorted(entries: &[DependencyGeneration]) -> Result<Self, DependencyOrderError> {
        for pair in entries.windows(2) {
            if pair[0].dependency >= pair[1].dependency {
                return Err(DependencyOrderError {
                    previous: pair[0].dependency,
                    next: pair[1].dependency,
                });
            }
        }
        Ok(Self {
            entries: PagedValues::from_slice(entries),
        })
    }

    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len
    }
}

#[derive(Debug)]
struct ExactRootToken {
    serial: u64,
}

#[derive(Clone, Debug)]
enum LogicalInputRoot {
    Frontier(Arc<SegmentedLeaf>),
    Proof(Arc<ExactRootToken>),
}

impl LogicalInputRoot {
    fn exact_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Frontier(left), Self::Frontier(right)) => left.identity() == right.identity(),
            (Self::Proof(left), Self::Proof(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

/// Exact root for all logical input at and after a checkpoint, plus its scoped
/// generation. A one-leaf frontier root is sufficient only when that leaf is
/// the complete remaining suffix; multi-leaf suffix roots remain an explicit
/// integration gap.
#[derive(Clone, Debug)]
pub struct LogicalInputSnapshot {
    generation: u64,
    root: LogicalInputRoot,
}

impl LogicalInputSnapshot {
    /// Uses a real current frontier leaf identity when that leaf is the complete
    /// remaining logical suffix. Two snapshots are identical only when they
    /// retain clones of the same minted leaf identity and generation. Wrapping
    /// a leaf clone in another `Arc` does not destroy the exact identity.
    #[must_use]
    pub fn from_frontier(generation: u64, root: Arc<SegmentedLeaf>) -> Self {
        Self {
            generation,
            root: LogicalInputRoot::Frontier(root),
        }
    }

    /// Mints a capability standing in for a future splice-persistent logical
    /// root. This is intentionally named as proof-only until the frontier owns
    /// such roots itself.
    #[must_use]
    pub fn proof_root(generation: u64) -> Self {
        Self {
            generation,
            root: LogicalInputRoot::Proof(Arc::new(ExactRootToken {
                serial: mint_exact_root(),
            })),
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Diagnostic only; equality still uses capability identity rather than
    /// this serial value.
    #[must_use]
    pub fn diagnostic_serial(&self) -> Option<u64> {
        match &self.root {
            LogicalInputRoot::Frontier(_) => None,
            LogicalInputRoot::Proof(token) => Some(token.serial),
        }
    }
}

/// Exact persistent output suffix identity. Only cloning the same root can
/// preserve identity; equal payload or equal length is intentionally absent.
#[derive(Clone, Debug)]
pub struct OutputSuffixRoot(Arc<ExactRootToken>);

impl PartialEq for OutputSuffixRoot {
    fn eq(&self, other: &Self) -> bool {
        self.exact_eq(other)
    }
}

impl Eq for OutputSuffixRoot {}

impl OutputSuffixRoot {
    #[must_use]
    pub fn fresh() -> Self {
        Self(Arc::new(ExactRootToken {
            serial: mint_exact_root(),
        }))
    }

    #[must_use]
    pub fn diagnostic_serial(&self) -> u64 {
        self.0.serial
    }

    fn exact_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Exact state and input identity present at both an old candidate and the
/// current parse position. Output is deliberately absent: only the old
/// candidate owns an adoptable output suffix.
#[derive(Clone, Debug)]
pub struct CheckpointIdentity {
    pub parser_state: ParserStateSnapshot,
    pub logical_input: LogicalInputSnapshot,
    pub dependencies: DependencySnapshot,
}

/// One revision-specific checkpoint and its exact identity.
#[derive(Clone, Debug)]
pub struct ExactCheckpoint {
    revision: RevisionId,
    source: SourceRootIdentity,
    boundary: usize,
    witness: SourceBoundaryWitness,
    identity: CheckpointIdentity,
}

/// Old checkpoint plus the immutable output suffix it can contribute after an
/// exact convergence proof. Current checkpoints intentionally cannot assert an
/// output suffix identity.
#[derive(Clone, Debug)]
pub struct CandidateCheckpoint {
    checkpoint: ExactCheckpoint,
    output_suffix: OutputSuffixRoot,
}

/// Invalid checkpoint/map boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    InvalidBoundary { boundary: usize, source_len: usize },
    BoundarySplitsScalar(usize),
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBoundary {
                boundary,
                source_len,
            } => write!(
                formatter,
                "checkpoint boundary {boundary} exceeds {source_len} source bytes"
            ),
            Self::BoundarySplitsScalar(boundary) => {
                write!(formatter, "checkpoint boundary {boundary} splits UTF-8")
            }
        }
    }
}

impl std::error::Error for CheckpointError {}

impl ExactCheckpoint {
    /// Captures an exact source witness without flattening the source.
    ///
    /// # Errors
    ///
    /// Returns an error for an out-of-range or non-scalar boundary.
    pub fn capture(
        revision: RevisionId,
        source: &PersistentSource,
        boundary: usize,
        identity: CheckpointIdentity,
    ) -> Result<Self, CheckpointError> {
        if boundary > source.len_bytes() {
            return Err(CheckpointError::InvalidBoundary {
                boundary,
                source_len: source.len_bytes(),
            });
        }
        if !source.is_char_boundary(boundary) {
            return Err(CheckpointError::BoundarySplitsScalar(boundary));
        }
        let witness = if boundary == source.len_bytes() {
            SourceBoundaryWitness::EndOfFile
        } else {
            let Some(anchor) = source.anchor_at(boundary) else {
                return Err(CheckpointError::InvalidBoundary {
                    boundary,
                    source_len: source.len_bytes(),
                });
            };
            SourceBoundaryWitness::NextByte(anchor)
        };
        Ok(Self {
            revision,
            source: source.identity(),
            boundary,
            witness,
            identity,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> RevisionId {
        self.revision
    }

    #[must_use]
    pub const fn boundary(&self) -> usize {
        self.boundary
    }

    #[must_use]
    pub const fn source_identity(&self) -> SourceRootIdentity {
        self.source
    }

    #[must_use]
    pub const fn witness(&self) -> SourceBoundaryWitness {
        self.witness
    }

    /// Turns an old checkpoint into a convergence candidate by attaching the
    /// output suffix that may be adopted only after proof succeeds.
    #[must_use]
    pub fn with_output_suffix(self, output_suffix: OutputSuffixRoot) -> CandidateCheckpoint {
        CandidateCheckpoint {
            checkpoint: self,
            output_suffix,
        }
    }
}

impl CandidateCheckpoint {
    #[must_use]
    pub const fn checkpoint(&self) -> &ExactCheckpoint {
        &self.checkpoint
    }

    #[must_use]
    pub const fn output_suffix(&self) -> &OutputSuffixRoot {
        &self.output_suffix
    }

    #[must_use]
    pub const fn witness(&self) -> SourceBoundaryWitness {
        self.checkpoint.witness
    }
}

/// Why exact backward mapping failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapFailure {
    BoundaryInsideChangedBytes {
        to_revision: RevisionId,
        boundary: usize,
        inserted_new: Range<usize>,
    },
    RevisionNotInHistory(RevisionId),
    /// Internal continuity record identities did not form an exact chain.
    HistoryWindowBroken,
}

/// Successful exact backward map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappedBoundary {
    pub target_revision: RevisionId,
    pub target_source: SourceRootIdentity,
    pub old_boundary: usize,
    pub current_boundary: usize,
    pub change_steps: usize,
    /// Number of edits that copied the unchanged next byte into a compacted
    /// boundary buffer. Nonzero explains a changed anchor without weakening
    /// the exact edit-lineage proof.
    pub copied_next_byte_steps: usize,
}

/// Poll state for a backward map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackwardMapStatus {
    Pending,
    Mapped(MappedBoundary),
    Rejected(MapFailure),
}

/// Work consumed by one backward-map poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackwardMapPollReceipt {
    pub work: usize,
    pub change_steps: usize,
}

/// Cumulative backward-map audit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackwardMapAudit {
    pub polls: usize,
    pub total_work: usize,
    pub change_steps: usize,
    pub max_poll_work: usize,
}

/// One backward-map poll result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackwardMapPoll {
    pub status: BackwardMapStatus,
    pub receipt: BackwardMapPollReceipt,
}

/// Pollable exact change-map composition. The cursor walks the fixed record
/// window newest-to-oldest, so skipped revisions require no reverse temporary
/// vector and never retain historical source roots.
#[derive(Debug)]
pub struct BackwardMapJob<'a> {
    changes: &'a VecDeque<ExactChange>,
    next_change: usize,
    cursor_revision: RevisionId,
    cursor_source: SourceRootIdentity,
    target_revision: RevisionId,
    current_boundary: usize,
    mapped_boundary: usize,
    affinity: BoundaryAffinity,
    traversed: usize,
    copied_next_byte_steps: usize,
    status: BackwardMapStatus,
    audit: BackwardMapAudit,
}

impl<'a> BackwardMapJob<'a> {
    fn new(
        history: &'a ChangeHistory,
        target_revision: RevisionId,
        current_boundary: usize,
        affinity: BoundaryAffinity,
    ) -> Result<Self, CheckpointError> {
        if current_boundary > history.latest_source.len_bytes() {
            return Err(CheckpointError::InvalidBoundary {
                boundary: current_boundary,
                source_len: history.latest_source.len_bytes(),
            });
        }
        if !history.latest_source.is_char_boundary(current_boundary) {
            return Err(CheckpointError::BoundarySplitsScalar(current_boundary));
        }
        Ok(Self {
            changes: &history.changes,
            next_change: history.changes.len(),
            cursor_revision: history.latest_revision,
            cursor_source: history.latest_source.identity(),
            target_revision,
            current_boundary,
            mapped_boundary: current_boundary,
            affinity,
            traversed: 0,
            copied_next_byte_steps: 0,
            status: BackwardMapStatus::Pending,
            audit: BackwardMapAudit::default(),
        })
    }

    /// Consumes at most `min(fuel, MAX_CONVERGENCE_POLL_WORK)` change steps.
    #[must_use]
    pub fn poll(&mut self, fuel: usize) -> BackwardMapPoll {
        let mut receipt = BackwardMapPollReceipt::default();
        let limit = fuel.min(MAX_CONVERGENCE_POLL_WORK);
        if matches!(self.status, BackwardMapStatus::Pending) {
            self.settle_terminal();
        }
        while receipt.work < limit && matches!(self.status, BackwardMapStatus::Pending) {
            let Some(next_change) = self.next_change.checked_sub(1) else {
                self.status = BackwardMapStatus::Rejected(MapFailure::RevisionNotInHistory(
                    self.target_revision,
                ));
                break;
            };
            let change = &self.changes[next_change];
            if change.to_revision != self.cursor_revision || change.to_source != self.cursor_source
            {
                self.status = BackwardMapStatus::Rejected(MapFailure::HistoryWindowBroken);
                break;
            }
            let mapped = map_boundary_back(self.mapped_boundary, change, self.affinity);
            receipt.work += 1;
            receipt.change_steps += 1;
            self.traversed += 1;
            match mapped {
                Ok(boundary) => {
                    if matches!(self.affinity, BoundaryAffinity::Suffix)
                        && boundary < change.old_len
                        && (change.compacted_prefix.contains(&boundary)
                            || change.compacted_suffix.contains(&boundary))
                    {
                        self.copied_next_byte_steps += 1;
                    }
                    self.mapped_boundary = boundary;
                    self.next_change = next_change;
                    self.cursor_revision = change.from_revision;
                    self.cursor_source = change.from_source;
                    self.settle_terminal();
                }
                Err(failure) => {
                    self.status = BackwardMapStatus::Rejected(failure);
                }
            }
        }
        self.audit.polls += 1;
        self.audit.total_work += receipt.work;
        self.audit.change_steps += receipt.change_steps;
        self.audit.max_poll_work = self.audit.max_poll_work.max(receipt.work);
        BackwardMapPoll {
            status: self.status.clone(),
            receipt,
        }
    }

    fn settle_terminal(&mut self) {
        if self.cursor_revision == self.target_revision {
            self.status = BackwardMapStatus::Mapped(MappedBoundary {
                target_revision: self.target_revision,
                target_source: self.cursor_source,
                old_boundary: self.mapped_boundary,
                current_boundary: self.current_boundary,
                change_steps: self.traversed,
                copied_next_byte_steps: self.copied_next_byte_steps,
            });
        } else if self.cursor_revision < self.target_revision {
            self.status =
                BackwardMapStatus::Rejected(MapFailure::RevisionNotInHistory(self.target_revision));
        }
    }

    #[must_use]
    pub const fn audit(&self) -> BackwardMapAudit {
        self.audit
    }
}

fn map_boundary_back(
    boundary: usize,
    change: &ExactChange,
    affinity: BoundaryAffinity,
) -> Result<usize, MapFailure> {
    let new = &change.inserted_new;
    let old = &change.replaced_old;
    let changed = match affinity {
        BoundaryAffinity::Suffix => new.start <= boundary && boundary < new.end,
        BoundaryAffinity::Prefix => new.start < boundary && boundary <= new.end,
    };
    if changed {
        return Err(MapFailure::BoundaryInsideChangedBytes {
            to_revision: change.to_revision,
            boundary,
            inserted_new: new.clone(),
        });
    }
    let before = match affinity {
        BoundaryAffinity::Suffix => boundary < new.start,
        BoundaryAffinity::Prefix => boundary <= new.start,
    };
    if before {
        Ok(boundary)
    } else {
        Ok(old.end + (boundary - new.end))
    }
}

/// Exact reason a convergence candidate was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvergenceRejection {
    Map(MapFailure),
    CurrentRevisionMismatch,
    CurrentSourceMismatch,
    CandidateSourceMismatch,
    MappedBoundaryMismatch { mapped: usize, candidate: usize },
    SourceWitnessMismatch,
    LogicalInputGenerationMismatch,
    LogicalInputRootMismatch,
    ParserStateLengthMismatch,
    ParserStateMismatch { atom: usize },
    DependencyCountMismatch,
    DependencyGenerationMismatch { entry: usize },
    ProofInvariantViolation,
}

/// Successful suffix convergence proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvergedSuffix {
    pub candidate_revision: RevisionId,
    pub candidate_boundary: usize,
    pub current_revision: RevisionId,
    pub current_boundary: usize,
    pub composed_change_steps: usize,
    /// Candidate output root authorized for adoption by this proof.
    pub adopted_output_suffix: OutputSuffixRoot,
}

/// Poll state for exact convergence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvergenceStatus {
    Pending,
    Converged(ConvergedSuffix),
    Rejected(ConvergenceRejection),
}

/// Work consumed by one convergence poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConvergencePollReceipt {
    pub work: usize,
    pub change_steps: usize,
    pub fixed_checks: usize,
    pub parser_state_atoms: usize,
    pub dependency_entries: usize,
}

/// Cumulative convergence audit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConvergenceAudit {
    pub polls: usize,
    pub total_work: usize,
    pub change_steps: usize,
    pub fixed_checks: usize,
    pub parser_state_atoms: usize,
    pub dependency_entries: usize,
    pub max_poll_work: usize,
}

/// One convergence poll result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvergencePoll {
    pub status: ConvergenceStatus,
    pub receipt: ConvergencePollReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComparePhase {
    Mapping,
    Fixed,
    ParserState,
    Dependencies,
    Done,
}

/// Pollable exact checkpoint comparator.
#[derive(Debug)]
pub struct ConvergenceJob<'a> {
    history_revision: RevisionId,
    history_source: SourceRootIdentity,
    candidate: CandidateCheckpoint,
    current: ExactCheckpoint,
    mapper: BackwardMapJob<'a>,
    mapped: Option<MappedBoundary>,
    fixed_check: usize,
    parser_atom: usize,
    dependency_entry: usize,
    candidate_parser: ValueCursor<u64>,
    current_parser: ValueCursor<u64>,
    candidate_dependencies: ValueCursor<DependencyGeneration>,
    current_dependencies: ValueCursor<DependencyGeneration>,
    phase: ComparePhase,
    status: ConvergenceStatus,
    audit: ConvergenceAudit,
}

impl<'a> ConvergenceJob<'a> {
    /// Starts an exact proof from a current checkpoint back to a candidate in
    /// an ancestor revision.
    ///
    /// # Errors
    ///
    /// Returns an error only when the current boundary is invalid for the
    /// history's latest source; identity mismatches are fuelled rejections.
    pub fn new(
        history: &'a ChangeHistory,
        candidate: CandidateCheckpoint,
        current: ExactCheckpoint,
        affinity: BoundaryAffinity,
    ) -> Result<Self, CheckpointError> {
        let mapper =
            history.backward_map(candidate.checkpoint.revision, current.boundary, affinity)?;
        let candidate_parser = candidate.checkpoint.identity.parser_state.atoms.cursor();
        let current_parser = current.identity.parser_state.atoms.cursor();
        let candidate_dependencies = candidate.checkpoint.identity.dependencies.entries.cursor();
        let current_dependencies = current.identity.dependencies.entries.cursor();
        Ok(Self {
            history_revision: history.latest_revision,
            history_source: history.latest_source.identity(),
            candidate,
            current,
            mapper,
            mapped: None,
            fixed_check: 0,
            parser_atom: 0,
            dependency_entry: 0,
            candidate_parser,
            current_parser,
            candidate_dependencies,
            current_dependencies,
            phase: ComparePhase::Mapping,
            status: ConvergenceStatus::Pending,
            audit: ConvergenceAudit::default(),
        })
    }

    /// Consumes bounded work. Every traversed revision, fixed identity check,
    /// parser-state atom, and dependency entry costs one unit.
    #[must_use]
    pub fn poll(&mut self, fuel: usize) -> ConvergencePoll {
        let mut receipt = ConvergencePollReceipt::default();
        let limit = fuel.min(MAX_CONVERGENCE_POLL_WORK);
        while receipt.work < limit && matches!(self.status, ConvergenceStatus::Pending) {
            match self.phase {
                ComparePhase::Mapping => {
                    let map_poll = self.mapper.poll(limit - receipt.work);
                    receipt.work += map_poll.receipt.work;
                    receipt.change_steps += map_poll.receipt.change_steps;
                    match map_poll.status {
                        BackwardMapStatus::Pending => break,
                        BackwardMapStatus::Rejected(failure) => {
                            self.reject(ConvergenceRejection::Map(failure));
                        }
                        BackwardMapStatus::Mapped(mapped) => {
                            self.mapped = Some(mapped);
                            self.phase = ComparePhase::Fixed;
                        }
                    }
                }
                ComparePhase::Fixed => {
                    let rejection = self.run_fixed_check();
                    receipt.work += 1;
                    receipt.fixed_checks += 1;
                    if let Some(rejection) = rejection {
                        self.reject(rejection);
                    } else {
                        self.fixed_check += 1;
                        if self.fixed_check == FIXED_CHECKS {
                            self.phase = ComparePhase::ParserState;
                        }
                    }
                }
                ComparePhase::ParserState => {
                    if self.parser_atom
                        == self.candidate.checkpoint.identity.parser_state.atom_count()
                    {
                        self.phase = ComparePhase::Dependencies;
                        continue;
                    }
                    let (Some(candidate), Some(current)) =
                        (self.candidate_parser.next(), self.current_parser.next())
                    else {
                        self.reject(ConvergenceRejection::ProofInvariantViolation);
                        continue;
                    };
                    receipt.work += 1;
                    receipt.parser_state_atoms += 1;
                    if candidate == current {
                        self.parser_atom += 1;
                    } else {
                        self.reject(ConvergenceRejection::ParserStateMismatch {
                            atom: self.parser_atom,
                        });
                    }
                }
                ComparePhase::Dependencies => {
                    if self.dependency_entry
                        == self
                            .candidate
                            .checkpoint
                            .identity
                            .dependencies
                            .entry_count()
                    {
                        self.converge();
                        continue;
                    }
                    let (Some(candidate), Some(current)) = (
                        self.candidate_dependencies.next(),
                        self.current_dependencies.next(),
                    ) else {
                        self.reject(ConvergenceRejection::ProofInvariantViolation);
                        continue;
                    };
                    receipt.work += 1;
                    receipt.dependency_entries += 1;
                    if candidate == current {
                        self.dependency_entry += 1;
                    } else {
                        self.reject(ConvergenceRejection::DependencyGenerationMismatch {
                            entry: self.dependency_entry,
                        });
                    }
                }
                ComparePhase::Done => break,
            }
        }
        self.audit.polls += 1;
        self.audit.total_work += receipt.work;
        self.audit.change_steps += receipt.change_steps;
        self.audit.fixed_checks += receipt.fixed_checks;
        self.audit.parser_state_atoms += receipt.parser_state_atoms;
        self.audit.dependency_entries += receipt.dependency_entries;
        self.audit.max_poll_work = self.audit.max_poll_work.max(receipt.work);
        ConvergencePoll {
            status: self.status.clone(),
            receipt,
        }
    }

    fn run_fixed_check(&self) -> Option<ConvergenceRejection> {
        let Some(mapped) = self.mapped else {
            return Some(ConvergenceRejection::ProofInvariantViolation);
        };
        match self.fixed_check {
            0 if self.current.revision != self.history_revision => {
                Some(ConvergenceRejection::CurrentRevisionMismatch)
            }
            1 if self.current.source != self.history_source => {
                Some(ConvergenceRejection::CurrentSourceMismatch)
            }
            2 if self.candidate.checkpoint.source != mapped.target_source => {
                Some(ConvergenceRejection::CandidateSourceMismatch)
            }
            3 if self.candidate.checkpoint.boundary != mapped.old_boundary => {
                Some(ConvergenceRejection::MappedBoundaryMismatch {
                    mapped: mapped.old_boundary,
                    candidate: self.candidate.checkpoint.boundary,
                })
            }
            4 if self.candidate.checkpoint.witness != self.current.witness
                && mapped.copied_next_byte_steps == 0 =>
            {
                Some(ConvergenceRejection::SourceWitnessMismatch)
            }
            5 if self.candidate.checkpoint.identity.logical_input.generation
                != self.current.identity.logical_input.generation =>
            {
                Some(ConvergenceRejection::LogicalInputGenerationMismatch)
            }
            6 if !self
                .candidate
                .checkpoint
                .identity
                .logical_input
                .root
                .exact_eq(&self.current.identity.logical_input.root) =>
            {
                Some(ConvergenceRejection::LogicalInputRootMismatch)
            }
            7 if self.candidate.checkpoint.identity.parser_state.atom_count()
                != self.current.identity.parser_state.atom_count() =>
            {
                Some(ConvergenceRejection::ParserStateLengthMismatch)
            }
            8 if self
                .candidate
                .checkpoint
                .identity
                .dependencies
                .entry_count()
                != self.current.identity.dependencies.entry_count() =>
            {
                Some(ConvergenceRejection::DependencyCountMismatch)
            }
            _ => None,
        }
    }

    fn reject(&mut self, rejection: ConvergenceRejection) {
        self.phase = ComparePhase::Done;
        self.status = ConvergenceStatus::Rejected(rejection);
    }

    fn converge(&mut self) {
        let Some(mapped) = self.mapped else {
            self.reject(ConvergenceRejection::ProofInvariantViolation);
            return;
        };
        self.phase = ComparePhase::Done;
        self.status = ConvergenceStatus::Converged(ConvergedSuffix {
            candidate_revision: self.candidate.checkpoint.revision,
            candidate_boundary: self.candidate.checkpoint.boundary,
            current_revision: self.current.revision,
            current_boundary: self.current.boundary,
            composed_change_steps: mapped.change_steps,
            adopted_output_suffix: self.candidate.output_suffix.clone(),
        });
    }

    #[must_use]
    pub const fn audit(&self) -> ConvergenceAudit {
        self.audit
    }
}

const FIXED_CHECKS: usize = 9;

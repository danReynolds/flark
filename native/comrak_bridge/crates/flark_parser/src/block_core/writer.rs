// SPDX-License-Identifier: MIT

//! Fuelled adapter from parser-certified block commands to recursive Green.
//!
//! This module contains no Markdown recognition. It validates command order,
//! keeps the small amount of writer-owned deferred state, and translates the
//! scalar block protocol into the generic persistent storage protocol.

use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use flark_engine::parser_internal::{
    splice_m11_recursive_green_structural_atomic, M11RecursiveGreenBuild,
    M11RecursiveGreenBuildStatus, M11RecursiveGreenCachedRowEditCapability,
    M11RecursiveGreenCachedRowEditable, M11RecursiveGreenCloseFacts, M11RecursiveGreenClosedChild,
    M11RecursiveGreenCoveragePart, M11RecursiveGreenError, M11RecursiveGreenEvent,
    M11RecursiveGreenFactTag, M11RecursiveGreenFrameId, M11RecursiveGreenKind,
    M11RecursiveGreenLogicalAction, M11RecursiveGreenPropertyChunk, M11RecursiveGreenReclaimPoll,
    M11RecursiveGreenRoot, M11RecursiveGreenSourceMetric, M11RecursiveGreenStructuralBoundary,
    M11RecursiveGreenStructuralBoundaryTransactionReplica, M11RecursiveGreenStructuralSpliceRebase,
    M11RecursiveGreenStructuralSpliceReceipt, M11RecursiveGreenStructuralSpliceSelection,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeError, ExactUnchangedPrefixWitness,
    ExactUnchangedSuffixWitness, SourceEditError, SourceSnapshotLease, SourceVersion,
};

use super::controller::{
    M11DirectBlockRestartTransactionReplica, M11DirectLeadingReferenceRemainderContinuation,
};
use super::{
    BlockCommand, BlockKind, BulletMarker, CoveragePart, FenceCharacter, FencedCodeBoundary,
    FinalFacts, HeadingStyle, LineEnding, LineSourcePosition, LineSourceRange, ListDelimiter,
    ListStyle, LogicalAction, M11DirectBlockController, M11DirectBlockDeferredRole,
    M11DirectBlockError, M11DirectBlockRestart, ParagraphOutcome, SourceMetric, StackOwner,
    TerminatorResolution,
};

const KIND_DOCUMENT: u16 = 1;
const KIND_BLOCK_QUOTE: u16 = 2;
const KIND_LIST: u16 = 3;
const KIND_ITEM: u16 = 4;
pub(super) const KIND_PARAGRAPH: u16 = 5;
const KIND_INDENTED_CODE: u16 = 6;
const KIND_FENCED_CODE: u16 = 7;
const KIND_HTML_BLOCK: u16 = 8;
pub(super) const KIND_HEADING: u16 = 12;
const KIND_THEMATIC_BREAK: u16 = 13;
const KIND_EMPTY_ITEM_ROW: u16 = 14;

const FACT_LIST: u16 = 1;
const FACT_ITEM: u16 = 2;
const FACT_HEADING: u16 = 3;
const FACT_CODE: u16 = 4;
const FACT_HTML: u16 = 5;
const FACT_ROW_EDITABLE: u16 = 6;

static RESTART_JOIN_IDS: AtomicU64 = AtomicU64::new(1);
static ADOPTION_TRANSACTION_IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum M11BlockWriterError {
    ZeroFuel,
    CommandPending,
    InvalidCommand(&'static str),
    CounterOverflow,
    Allocation,
    Poisoned,
    Source(SourceEditError),
    Engine(M11RecursiveGreenError),
}

#[derive(Debug)]
pub enum M11BlockRestartError {
    Writer(M11BlockWriterError),
    Controller(M11DirectBlockError),
    Document(DocumentRuntimeError),
    Engine(M11RecursiveGreenError),
    Pairing(&'static str),
}

impl fmt::Display for M11BlockRestartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Writer(error) => error.fmt(formatter),
            Self::Controller(error) => write!(formatter, "direct block restart: {error:?}"),
            Self::Document(error) => error.fmt(formatter),
            Self::Engine(error) => error.fmt(formatter),
            Self::Pairing(message) => write!(formatter, "block restart pairing failed: {message}"),
        }
    }
}

impl std::error::Error for M11BlockRestartError {}

impl From<M11BlockWriterError> for M11BlockRestartError {
    fn from(error: M11BlockWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<M11DirectBlockError> for M11BlockRestartError {
    fn from(error: M11DirectBlockError) -> Self {
        Self::Controller(error)
    }
}

impl From<DocumentRuntimeError> for M11BlockRestartError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self::Document(error)
    }
}

impl From<M11RecursiveGreenError> for M11BlockRestartError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self::Engine(error)
    }
}

impl fmt::Display for M11BlockWriterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFuel => formatter.write_str("block-writer poll requires nonzero fuel"),
            Self::CommandPending => formatter.write_str("a block command is already pending"),
            Self::InvalidCommand(message) => write!(formatter, "invalid block command: {message}"),
            Self::CounterOverflow => formatter.write_str("block-writer counter overflow"),
            Self::Allocation => formatter.write_str("block-writer allocation failed"),
            Self::Poisoned => formatter.write_str("block writer is poisoned"),
            Self::Source(error) => error.fmt(formatter),
            Self::Engine(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11BlockWriterError {}

impl From<M11RecursiveGreenError> for M11BlockWriterError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self::Engine(error)
    }
}

impl From<SourceEditError> for M11BlockWriterError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockWriterOfferStatus {
    Complete,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11BlockWriterPollStatus {
    Pending,
    CommandComplete,
    DocumentComplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockWriterPoll {
    status: M11BlockWriterPollStatus,
    transitions: usize,
}

impl M11BlockWriterPoll {
    #[must_use]
    pub const fn status(self) -> M11BlockWriterPollStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug)]
struct FenceFold {
    logical_base: SourceMetric,
    info_end: Option<SourceMetric>,
    literal_start: Option<SourceMetric>,
}

#[derive(Clone, Copy, Debug)]
struct RowEditableFold {
    physical_base: SourceMetric,
    start: Option<SourceMetric>,
    end: SourceMetric,
    gap_after: bool,
    contiguous: bool,
    tracking: bool,
}

impl RowEditableFold {
    const fn new(physical_base: SourceMetric, tracking: bool) -> Self {
        Self {
            physical_base,
            start: None,
            end: SourceMetric::new(0, 0).expect("zero source metric is valid"),
            gap_after: false,
            contiguous: true,
            tracking,
        }
    }

    fn reset_at(&mut self, physical: SourceMetric) -> Result<(), M11BlockWriterError> {
        let relative = metric_difference(physical, self.physical_base)?;
        self.start = Some(relative);
        self.end = relative;
        self.gap_after = false;
        self.contiguous = true;
        self.tracking = true;
        Ok(())
    }

    fn observe(
        &mut self,
        physical_start: SourceMetric,
        physical: SourceMetric,
        compatible: bool,
    ) -> Result<(), M11BlockWriterError> {
        if !self.tracking {
            return Ok(());
        }
        let start = metric_difference(physical_start, self.physical_base)?;
        let end = start
            .checked_add(physical)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        if compatible {
            if self.start.is_some() && self.gap_after {
                self.contiguous = false;
            }
            self.start.get_or_insert(start);
            self.end = end;
            self.gap_after = false;
        } else if self.start.is_some() {
            self.gap_after = true;
        }
        Ok(())
    }

    fn retain_visible_suffix_at(
        &mut self,
        physical: SourceMetric,
    ) -> Result<(), M11BlockWriterError> {
        let relative = metric_difference(physical, self.physical_base)?;
        if relative.bytes() > self.end.bytes() || relative.utf16() > self.end.utf16() {
            return Err(M11BlockWriterError::InvalidCommand(
                "reference suffix begins after Paragraph editable coverage",
            ));
        }
        self.start = Some(relative);
        self.gap_after = false;
        // The authenticated rewrite removed every preceding projection atom;
        // contiguity is re-established at the surviving suffix boundary.
        self.contiguous = true;
        Ok(())
    }

    fn cached(self) -> Result<M11RecursiveGreenCachedRowEditable, M11BlockWriterError> {
        let start = self.start.unwrap_or_default();
        M11RecursiveGreenCachedRowEditable::new(
            if self.contiguous {
                M11RecursiveGreenCachedRowEditCapability::Contiguous
            } else {
                M11RecursiveGreenCachedRowEditCapability::Unavailable
            },
            green_metric_allow_empty(start)?,
            green_metric_allow_empty(self.end)?,
        )
        .ok_or(M11BlockWriterError::InvalidCommand(
            "cached row-editable bounds are reversed",
        ))
    }
}

#[derive(Clone, Copy, Debug)]
struct OpenFrame {
    id: M11RecursiveGreenFrameId,
    kind: BlockKind,
    fence: Option<FenceFold>,
    row_editable: Option<RowEditableFold>,
    has_renderable_descendant: bool,
}

#[derive(Clone, Copy, Debug)]
enum StagedSource {
    Terminator {
        metric: SourceMetric,
        terminal: M11RecursiveGreenFrameId,
        terminal_index: usize,
    },
    BlankGap {
        metric: SourceMetric,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct M11ReferenceStagedTerminator {
    pub(super) start: SourceMetric,
    pub(super) end: SourceMetric,
    pub(super) raw_codepoint_contribution: u8,
}

#[derive(Clone, Copy, Debug)]
struct PendingEvents {
    events: [Option<M11RecursiveGreenEvent>; 3],
    len: u8,
    next: u8,
    in_flight: bool,
}

impl PendingEvents {
    const fn one(event: M11RecursiveGreenEvent) -> Self {
        Self {
            events: [Some(event), None, None],
            len: 1,
            next: 0,
            in_flight: false,
        }
    }

    const fn two(first: M11RecursiveGreenEvent, second: M11RecursiveGreenEvent) -> Self {
        Self {
            events: [Some(first), Some(second), None],
            len: 2,
            next: 0,
            in_flight: false,
        }
    }

    const fn three(
        first: M11RecursiveGreenEvent,
        second: M11RecursiveGreenEvent,
        third: M11RecursiveGreenEvent,
    ) -> Self {
        Self {
            events: [Some(first), Some(second), Some(third)],
            len: 3,
            next: 0,
            in_flight: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Pending {
    Events(PendingEvents),
    Finish,
}

#[derive(Clone, Copy, Debug, Default)]
struct WriterOutputReceipt {
    events: u64,
    source_bytes: u64,
    source_utf16: u64,
    logical_bytes: u64,
    logical_utf16: u64,
}

enum WriterOutput {
    Document(M11RecursiveGreenBuild),
    Fragment(M11BlockFragmentOutput),
}

struct M11BlockFragmentOutput {
    lease: Option<SourceSnapshotLease>,
    events: Vec<M11RecursiveGreenEvent>,
    receipt: WriterOutputReceipt,
    source_bytes_read: u64,
}

#[derive(Debug)]
struct M11BlockRestartProvenance {
    base_source: SourceVersion,
    base_maximum_frame_id: u64,
    base_event_cut: u64,
    external_open_depth: usize,
    target_accepted_start: SourceMetric,
    target_logical_start: SourceMetric,
    start_staged: Option<StagedSource>,
    start_boundary: M11RecursiveGreenStructuralBoundary,
}

/// Parser- and storage-side bounded work for one authenticated local adoption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11BlockStructuralAdoptionReceipt {
    green: M11RecursiveGreenStructuralSpliceReceipt,
    high_level_events: usize,
    fragment_source_bytes_read: u64,
}

impl M11BlockStructuralAdoptionReceipt {
    #[must_use]
    pub const fn green(self) -> M11RecursiveGreenStructuralSpliceReceipt {
        self.green
    }

    /// Exact base and target Green event intervals selected by the writer's
    /// authenticated restart/convergence splice.
    #[must_use]
    pub const fn green_splice_selection(self) -> M11RecursiveGreenStructuralSpliceSelection {
        self.green.selection()
    }

    #[must_use]
    pub const fn high_level_events(self) -> usize {
        self.high_level_events
    }

    #[must_use]
    pub const fn fragment_source_bytes_read(self) -> u64 {
        self.fragment_source_bytes_read
    }
}

impl fmt::Debug for M11BlockFragmentOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockFragmentOutput")
            .field(
                "source",
                &self.lease.as_ref().map(SourceSnapshotLease::version),
            )
            .field("events", &self.events.len())
            .field("receipt", &self.receipt)
            .field("source_bytes_read", &self.source_bytes_read)
            .finish_non_exhaustive()
    }
}

impl WriterOutput {
    fn receipt(&self) -> WriterOutputReceipt {
        match self {
            Self::Document(build) => {
                let receipt = build.receipt();
                WriterOutputReceipt {
                    events: receipt.events(),
                    source_bytes: receipt.source_bytes(),
                    source_utf16: receipt.source_utf16(),
                    logical_bytes: receipt.logical_bytes(),
                    logical_utf16: receipt.logical_utf16(),
                }
            }
            Self::Fragment(fragment) => fragment.receipt,
        }
    }

    fn offer_event(&mut self, event: M11RecursiveGreenEvent) -> Result<(), M11BlockWriterError> {
        match self {
            Self::Document(build) => Ok(build.offer_event(event)?),
            Self::Fragment(fragment) => fragment.offer_event(event),
        }
    }
}

impl M11BlockFragmentOutput {
    fn offer_event(&mut self, event: M11RecursiveGreenEvent) -> Result<(), M11BlockWriterError> {
        let mut packed_events = 1_u64;
        let mut logical = SourceMetric::default();
        if let M11RecursiveGreenEvent::Coverage {
            physical,
            logical: action,
            ..
        } = event
        {
            let start = usize::try_from(self.receipt.source_bytes)
                .map_err(|_| M11BlockWriterError::CounterOverflow)?;
            let physical_bytes = usize::try_from(physical.bytes())
                .map_err(|_| M11BlockWriterError::CounterOverflow)?;
            let end = start
                .checked_add(physical_bytes)
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            let lease = self.lease.as_ref().ok_or(M11BlockWriterError::Poisoned)?;
            if end > lease.version().byte_len() {
                return Err(M11BlockWriterError::InvalidCommand(
                    "fragment coverage exceeds target source",
                ));
            }
            let utf16_start = lease.utf16_offset_for_byte(start)?;
            let utf16_end = lease.utf16_offset_for_byte(end)?;
            if u64::try_from(utf16_end - utf16_start).ok() != Some(physical.utf16()) {
                return Err(M11BlockWriterError::InvalidCommand(
                    "fragment coverage UTF-16 metric differs from target source",
                ));
            }
            logical = match action {
                M11RecursiveGreenLogicalAction::None
                | M11RecursiveGreenLogicalAction::HiddenUpstream => SourceMetric::default(),
                M11RecursiveGreenLogicalAction::Identity => {
                    SourceMetric::new(physical.bytes(), physical.utf16())
                        .expect("engine physical metrics are valid")
                }
                M11RecursiveGreenLogicalAction::PartialTab {
                    remaining_spaces, ..
                } => {
                    let bytes = read_fragment_bytes(&mut self.lease, start, end)?;
                    self.source_bytes_read = self
                        .source_bytes_read
                        .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                    if bytes != [b'\t'] || !(1..=3).contains(&remaining_spaces) {
                        return Err(M11BlockWriterError::InvalidCommand(
                            "fragment partial-tab recipe differs from target source",
                        ));
                    }
                    SourceMetric::new(u64::from(remaining_spaces), u64::from(remaining_spaces))
                        .expect("tab replacement metrics are valid")
                }
                M11RecursiveGreenLogicalAction::CanonicalNewline => {
                    let bytes = read_fragment_bytes(&mut self.lease, start, end)?;
                    self.source_bytes_read = self
                        .source_bytes_read
                        .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                    if !matches!(bytes.as_slice(), b"\n" | b"\r" | b"\r\n") {
                        return Err(M11BlockWriterError::InvalidCommand(
                            "fragment newline recipe differs from target source",
                        ));
                    }
                    SourceMetric::new(1, 1).expect("newline metric is valid")
                }
                M11RecursiveGreenLogicalAction::CanonicalText => {
                    let bytes = read_fragment_bytes(&mut self.lease, start, end)?;
                    self.source_bytes_read = self
                        .source_bytes_read
                        .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                    let mut atoms = 0_u64;
                    let mut in_identity = false;
                    let mut nul_count = 0_u64;
                    for byte in bytes {
                        if byte == 0 {
                            if in_identity {
                                atoms = atoms
                                    .checked_add(1)
                                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                                in_identity = false;
                            }
                            atoms = atoms
                                .checked_add(1)
                                .ok_or(M11BlockWriterError::CounterOverflow)?;
                            nul_count = nul_count
                                .checked_add(1)
                                .ok_or(M11BlockWriterError::CounterOverflow)?;
                        } else {
                            in_identity = true;
                        }
                    }
                    if in_identity {
                        atoms = atoms
                            .checked_add(1)
                            .ok_or(M11BlockWriterError::CounterOverflow)?;
                    }
                    packed_events = atoms.max(1);
                    SourceMetric::new(
                        physical
                            .bytes()
                            .checked_add(
                                nul_count
                                    .checked_mul(2)
                                    .ok_or(M11BlockWriterError::CounterOverflow)?,
                            )
                            .ok_or(M11BlockWriterError::CounterOverflow)?,
                        physical.utf16(),
                    )
                    .ok_or(M11BlockWriterError::CounterOverflow)?
                }
            };
            self.receipt.source_bytes = self
                .receipt
                .source_bytes
                .checked_add(physical.bytes())
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            self.receipt.source_utf16 = self
                .receipt
                .source_utf16
                .checked_add(physical.utf16())
                .ok_or(M11BlockWriterError::CounterOverflow)?;
        }
        self.receipt.logical_bytes = self
            .receipt
            .logical_bytes
            .checked_add(logical.bytes())
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        self.receipt.logical_utf16 = self
            .receipt
            .logical_utf16
            .checked_add(logical.utf16())
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        self.receipt.events = self
            .receipt
            .events
            .checked_add(packed_events)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        self.events
            .try_reserve(1)
            .map_err(|_| M11BlockWriterError::Allocation)?;
        self.events.push(event);
        Ok(())
    }
}

/// One source-bound, unpublished recursive-Green build.
#[must_use = "block writers require root transfer or explicit cancellation"]
pub struct M11BlockWriter {
    source: SourceVersion,
    geometry_lease: Option<SourceSnapshotLease>,
    output: WriterOutput,
    open: Vec<OpenFrame>,
    next_frame: u64,
    line_cursor: LineSourcePosition,
    staged: Option<StagedSource>,
    pending: Option<Pending>,
    restart_join: Option<u64>,
    restart_provenance: Option<M11BlockRestartProvenance>,
    document_complete: bool,
    poisoned: bool,
}

/// One source-, writer-, and parser-bound line-boundary restart checkpoint.
///
/// The parser recipe is kept opaque and inseparable from the writer's exact
/// open path, deferred predecessor, Green event cut, and cumulative logical
/// metric.  Construction is possible only from the two live production
/// actors at the same quiescent boundary.
#[must_use = "a block restart checkpoint must be resumed or discarded"]
pub struct M11BlockRestartCheckpoint {
    source: SourceVersion,
    parser: M11DirectBlockRestart,
    open: Box<[OpenFrame]>,
    next_frame: u64,
    accepted_physical: SourceMetric,
    parser_physical: SourceMetric,
    logical: SourceMetric,
    event_cut: u64,
    staged: Option<StagedSource>,
    restart_join: Option<u64>,
    green_boundary: Option<M11RecursiveGreenStructuralBoundary>,
}

/// Private identity joining every parser and Green replica minted for one
/// persistent-session adoption attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct M11BlockAdoptionTransactionId(u64);

/// Private checkpoint replica. The original checkpoint remains in the base
/// session until the target transaction commits.
pub(crate) struct M11BlockRestartCheckpointTransactionReplica {
    transaction: M11BlockAdoptionTransactionId,
    source: SourceVersion,
    parser: M11DirectBlockRestartTransactionReplica,
    open: Box<[OpenFrame]>,
    next_frame: u64,
    accepted_physical: SourceMetric,
    parser_physical: SourceMetric,
    logical: SourceMetric,
    event_cut: u64,
    staged: Option<StagedSource>,
    restart_join: Option<u64>,
    green_boundary: Option<M11RecursiveGreenStructuralBoundaryTransactionReplica>,
}

/// Private EOF-boundary replica paired with the same adoption identity as all
/// ordinary checkpoint replicas in the transaction.
pub(crate) struct M11BlockTerminalConvergenceCheckpointTransactionReplica {
    transaction: M11BlockAdoptionTransactionId,
    source: SourceVersion,
    open: Box<[OpenFrame]>,
    next_frame: u64,
    accepted_physical: SourceMetric,
    logical: SourceMetric,
    event_cut: u64,
    green_boundary: Option<M11RecursiveGreenStructuralBoundaryTransactionReplica>,
}

/// Authenticated Green cut after every source-backed child has closed and
/// immediately before the parser emits `Close(Document)` at EOF.
///
/// This is deliberately not a resumable parser checkpoint. It is the stable
/// convergence boundary for a tail reparse when no later physical line exists.
#[must_use = "an EOF convergence checkpoint must be adopted or discarded"]
pub struct M11BlockTerminalConvergenceCheckpoint {
    source: SourceVersion,
    open: Box<[OpenFrame]>,
    next_frame: u64,
    accepted_physical: SourceMetric,
    logical: SourceMetric,
    event_cut: u64,
    green_boundary: Option<M11RecursiveGreenStructuralBoundary>,
}

impl fmt::Debug for M11BlockRestartCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockRestartCheckpoint")
            .field("source", &self.source)
            .field("parser", &self.parser)
            .field("open_depth", &self.open.len())
            .field("accepted_physical", &self.accepted_physical)
            .field("parser_physical", &self.parser_physical)
            .field("logical", &self.logical)
            .field("event_cut", &self.event_cut)
            .finish_non_exhaustive()
    }
}

impl M11BlockRestartCheckpoint {
    pub(crate) fn allocate_adoption_transaction_id() -> Result<u64, M11BlockRestartError> {
        Ok(M11BlockAdoptionTransactionId::allocate()?.get())
    }

    pub(crate) fn replicate_for_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<M11BlockRestartCheckpointTransactionReplica, M11BlockRestartError> {
        let transaction = M11BlockAdoptionTransactionId::from_raw(transaction_id)?;
        let green_boundary = self
            .green_boundary
            .as_ref()
            .map(|boundary| boundary.replicate_for_parser_transaction(transaction.get()))
            .transpose()?;
        Ok(M11BlockRestartCheckpointTransactionReplica {
            transaction,
            source: self.source,
            parser: self.parser.replicate_for_transaction(transaction.get())?,
            open: replicate_open_frames(&self.open)?,
            next_frame: self.next_frame,
            accepted_physical: self.accepted_physical,
            parser_physical: self.parser_physical,
            logical: self.logical,
            event_cut: self.event_cut,
            staged: self.staged,
            restart_join: self.restart_join,
            green_boundary,
        })
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    /// Ordinal of the first physical line after this restart boundary.
    #[must_use]
    pub const fn next_line_ordinal(&self) -> u64 {
        self.parser.line_ordinal()
    }

    /// Physical source already represented by Green events at this cut.
    #[must_use]
    pub const fn accepted_physical(&self) -> SourceMetric {
        self.accepted_physical
    }

    /// Physical source consumed by the parser.  This may lead Green by one
    /// deferred terminator or blank-gap atom.
    #[must_use]
    pub const fn parser_physical(&self) -> SourceMetric {
        self.parser_physical
    }

    #[must_use]
    pub const fn logical_metric(&self) -> SourceMetric {
        self.logical
    }

    #[must_use]
    pub const fn event_cut(&self) -> u64 {
        self.event_cut
    }

    #[must_use]
    pub fn open_kinds(&self) -> impl ExactSizeIterator<Item = BlockKind> + '_ {
        self.open.iter().map(|frame| frame.kind)
    }

    /// Consumes this exact-base checkpoint after revalidating an unchanged
    /// parser prefix in the current target revision.
    pub fn resume(
        self,
        runtime: &DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        target_lease: SourceSnapshotLease,
        prefix: ExactUnchangedPrefixWitness,
    ) -> Result<M11JoinedBlockRestart, M11BlockRestartError> {
        self.resume_with_prefix(runtime, base, target_lease, Some(prefix))
    }

    pub(crate) fn resume_at_document_start(
        self,
        runtime: &DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        target_lease: SourceSnapshotLease,
    ) -> Result<M11JoinedBlockRestart, M11BlockRestartError> {
        self.resume_with_prefix(runtime, base, target_lease, None)
    }

    fn resume_with_prefix(
        self,
        runtime: &DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        target_lease: SourceSnapshotLease,
        prefix: Option<ExactUnchangedPrefixWitness>,
    ) -> Result<M11JoinedBlockRestart, M11BlockRestartError> {
        let prefix = prefix
            .map(|prefix| runtime.take_exact_unchanged_prefix_witness(prefix))
            .transpose()?;
        let parser_byte_cut = usize::try_from(self.parser_physical.bytes())
            .map_err(|_| M11BlockRestartError::Pairing("parser byte cut fits usize"))?;
        let parser_utf16_cut = usize::try_from(self.parser_physical.utf16())
            .map_err(|_| M11BlockRestartError::Pairing("parser UTF-16 cut fits usize"))?;
        if base.source() != self.source
            || base.event_count() < self.event_cut
            || match prefix.as_ref() {
                Some(prefix) => {
                    prefix.base() != self.source
                        || prefix.target() != target_lease.version()
                        || prefix.byte_end() != parser_byte_cut
                        || prefix.utf16_end() != parser_utf16_cut
                }
                None => parser_byte_cut != 0 || parser_utf16_cut != 0,
            }
        {
            return Err(M11BlockRestartError::Pairing(
                "source lineage does not end at the parser restart cut",
            ));
        }
        let next_frame = base
            .maximum_frame_id()
            .checked_add(1)
            .map(|next| next.max(self.next_frame))
            .ok_or(M11BlockRestartError::Pairing(
                "base Green frame identity space is exhausted",
            ))?;
        let start_boundary = self.green_boundary.ok_or(M11BlockRestartError::Pairing(
            "restart checkpoint has not been activated by a committed Green root",
        ))?;
        let restart_join = RESTART_JOIN_IDS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| M11BlockRestartError::Pairing("restart join identity exhausted"))?;
        let controller = M11DirectBlockController::resume_joined(self.parser, restart_join)?;
        let geometry_lease = runtime.snapshot_current_source()?;
        if geometry_lease.version() != target_lease.version() {
            return Err(M11BlockRestartError::Pairing(
                "row geometry lease differs from target source",
            ));
        }
        Ok(M11JoinedBlockRestart {
            controller,
            writer: M11BlockWriterRestartSeed {
                base_source: self.source,
                target_source: target_lease.version(),
                target_lease,
                geometry_lease,
                open: self.open,
                next_frame,
                base_maximum_frame_id: base.maximum_frame_id(),
                accepted_physical: self.accepted_physical,
                parser_physical: self.parser_physical,
                logical: self.logical,
                base_event_cut: self.event_cut,
                staged: self.staged,
                restart_join,
                start_boundary,
            },
        })
    }
}

impl M11BlockAdoptionTransactionId {
    pub(crate) fn allocate() -> Result<Self, M11BlockRestartError> {
        ADOPTION_TRANSACTION_IDS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(Self)
            .map_err(|_| M11BlockRestartError::Pairing("adoption transaction identity exhausted"))
    }

    const fn get(self) -> u64 {
        self.0
    }

    fn from_raw(value: u64) -> Result<Self, M11BlockRestartError> {
        if value == 0 {
            return Err(M11BlockRestartError::Pairing(
                "adoption transaction identity must be nonzero",
            ));
        }
        Ok(Self(value))
    }
}

impl M11BlockRestartCheckpointTransactionReplica {
    pub(crate) fn into_checkpoint(
        self,
        transaction_id: u64,
    ) -> Result<M11BlockRestartCheckpoint, M11BlockRestartError> {
        let transaction = M11BlockAdoptionTransactionId::from_raw(transaction_id)?;
        if self.transaction != transaction {
            return Err(M11BlockRestartError::Pairing(
                "checkpoint replica crossed adoption transactions",
            ));
        }
        let green_boundary = match self.green_boundary {
            Some(boundary) => {
                Some(boundary.into_boundary_for_parser_transaction(transaction.get())?)
            }
            None => None,
        };
        Ok(M11BlockRestartCheckpoint {
            source: self.source,
            parser: self.parser.into_restart(transaction.get())?,
            open: self.open,
            next_frame: self.next_frame,
            accepted_physical: self.accepted_physical,
            parser_physical: self.parser_physical,
            logical: self.logical,
            event_cut: self.event_cut,
            staged: self.staged,
            restart_join: self.restart_join,
            green_boundary,
        })
    }

    pub(crate) fn resume(
        self,
        transaction_id: u64,
        runtime: &DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        target_lease: SourceSnapshotLease,
        prefix: ExactUnchangedPrefixWitness,
    ) -> Result<M11JoinedBlockRestart, M11BlockRestartError> {
        self.into_checkpoint(transaction_id)?
            .resume(runtime, base, target_lease, prefix)
    }

    pub(crate) fn resume_at_document_start(
        self,
        transaction_id: u64,
        runtime: &DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        target_lease: SourceSnapshotLease,
    ) -> Result<M11JoinedBlockRestart, M11BlockRestartError> {
        self.into_checkpoint(transaction_id)?
            .resume_at_document_start(runtime, base, target_lease)
    }
}

impl fmt::Debug for M11BlockTerminalConvergenceCheckpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockTerminalConvergenceCheckpoint")
            .field("source", &self.source)
            .field("open_depth", &self.open.len())
            .field("accepted_physical", &self.accepted_physical)
            .field("logical", &self.logical)
            .field("event_cut", &self.event_cut)
            .finish_non_exhaustive()
    }
}

impl M11BlockTerminalConvergenceCheckpoint {
    pub(crate) fn replicate_for_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<M11BlockTerminalConvergenceCheckpointTransactionReplica, M11BlockRestartError> {
        let transaction = M11BlockAdoptionTransactionId::from_raw(transaction_id)?;
        let green_boundary = self
            .green_boundary
            .as_ref()
            .map(|boundary| boundary.replicate_for_parser_transaction(transaction.get()))
            .transpose()?;
        Ok(M11BlockTerminalConvergenceCheckpointTransactionReplica {
            transaction,
            source: self.source,
            open: replicate_open_frames(&self.open)?,
            next_frame: self.next_frame,
            accepted_physical: self.accepted_physical,
            logical: self.logical,
            event_cut: self.event_cut,
            green_boundary,
        })
    }
}

impl M11BlockTerminalConvergenceCheckpointTransactionReplica {
    pub(crate) fn into_checkpoint(
        self,
        transaction_id: u64,
    ) -> Result<M11BlockTerminalConvergenceCheckpoint, M11BlockRestartError> {
        let transaction = M11BlockAdoptionTransactionId::from_raw(transaction_id)?;
        if self.transaction != transaction {
            return Err(M11BlockRestartError::Pairing(
                "terminal checkpoint replica crossed adoption transactions",
            ));
        }
        let green_boundary = match self.green_boundary {
            Some(boundary) => {
                Some(boundary.into_boundary_for_parser_transaction(transaction.get())?)
            }
            None => None,
        };
        Ok(M11BlockTerminalConvergenceCheckpoint {
            source: self.source,
            open: self.open,
            next_frame: self.next_frame,
            accepted_physical: self.accepted_physical,
            logical: self.logical,
            event_cut: self.event_cut,
            green_boundary,
        })
    }
}

fn replicate_open_frames(open: &[OpenFrame]) -> Result<Box<[OpenFrame]>, M11BlockRestartError> {
    let mut replica = Vec::new();
    replica
        .try_reserve_exact(open.len())
        .map_err(|_| M11BlockWriterError::Allocation)?;
    replica.extend_from_slice(open);
    Ok(replica.into_boxed_slice())
}

/// Parser plus writer seed reconstructed by one successful composite join.
/// Keeping the parts linear prevents a parser recipe from being paired with a
/// different Green cut or target source after validation.
#[must_use = "a joined block restart must enter the fragment writer or be discarded"]
pub struct M11JoinedBlockRestart {
    controller: M11DirectBlockController,
    writer: M11BlockWriterRestartSeed,
}

impl fmt::Debug for M11JoinedBlockRestart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11JoinedBlockRestart")
            .field("writer", &self.writer)
            .finish_non_exhaustive()
    }
}

impl M11JoinedBlockRestart {
    /// Opens the bounded target-side command fragment selected by this join.
    /// The returned parser and writer carry the same private restart identity;
    /// a crossed pair cannot mint a convergence checkpoint.
    pub fn into_local_fragment(
        self,
    ) -> Result<(M11DirectBlockController, M11BlockWriter), M11BlockWriterError> {
        Ok((self.controller, self.writer.into_writer()?))
    }
}

#[must_use = "a writer restart seed must enter one target fragment build"]
pub(super) struct M11BlockWriterRestartSeed {
    base_source: SourceVersion,
    target_source: SourceVersion,
    target_lease: SourceSnapshotLease,
    geometry_lease: SourceSnapshotLease,
    open: Box<[OpenFrame]>,
    next_frame: u64,
    base_maximum_frame_id: u64,
    accepted_physical: SourceMetric,
    parser_physical: SourceMetric,
    logical: SourceMetric,
    base_event_cut: u64,
    staged: Option<StagedSource>,
    restart_join: u64,
    start_boundary: M11RecursiveGreenStructuralBoundary,
}

impl fmt::Debug for M11BlockWriterRestartSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockWriterRestartSeed")
            .field("base_source", &self.base_source)
            .field("target_source", &self.target_source)
            .field("open_depth", &self.open.len())
            .field("accepted_physical", &self.accepted_physical)
            .field("parser_physical", &self.parser_physical)
            .field("logical", &self.logical)
            .field("base_event_cut", &self.base_event_cut)
            .finish_non_exhaustive()
    }
}

impl M11BlockWriterRestartSeed {
    fn into_writer(self) -> Result<M11BlockWriter, M11BlockWriterError> {
        let mut events = Vec::new();
        events
            .try_reserve(64)
            .map_err(|_| M11BlockWriterError::Allocation)?;
        let external_open_depth = self.open.len();
        Ok(M11BlockWriter {
            source: self.target_source,
            geometry_lease: Some(self.geometry_lease),
            output: WriterOutput::Fragment(M11BlockFragmentOutput {
                lease: Some(self.target_lease),
                events,
                receipt: WriterOutputReceipt {
                    events: self.base_event_cut,
                    source_bytes: self.accepted_physical.bytes(),
                    source_utf16: self.accepted_physical.utf16(),
                    logical_bytes: self.logical.bytes(),
                    logical_utf16: self.logical.utf16(),
                },
                source_bytes_read: 0,
            }),
            open: self.open.into_vec(),
            next_frame: self.next_frame,
            line_cursor: LineSourcePosition::default(),
            staged: self.staged,
            pending: None,
            restart_join: Some(self.restart_join),
            restart_provenance: Some(M11BlockRestartProvenance {
                base_source: self.base_source,
                base_maximum_frame_id: self.base_maximum_frame_id,
                base_event_cut: self.base_event_cut,
                external_open_depth,
                target_accepted_start: self.accepted_physical,
                target_logical_start: self.logical,
                start_staged: self.staged,
                start_boundary: self.start_boundary,
            }),
            document_complete: false,
            poisoned: false,
        })
    }
}

impl fmt::Debug for M11BlockWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11BlockWriter")
            .field("open_depth", &self.open.len())
            .field("line_cursor", &self.line_cursor)
            .field("staged", &self.staged)
            .field("pending", &self.pending)
            .field("document_complete", &self.document_complete)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

impl M11BlockWriter {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
    ) -> Result<Self, M11BlockWriterError> {
        let source = lease.version();
        let geometry_lease = runtime.snapshot_current_source().map_err(|_| {
            M11BlockWriterError::InvalidCommand("row geometry source is unavailable")
        })?;
        if geometry_lease.version() != source {
            return Err(M11BlockWriterError::InvalidCommand(
                "row geometry source differs from writer source",
            ));
        }
        Ok(Self {
            source,
            geometry_lease: Some(geometry_lease),
            output: WriterOutput::Document(M11RecursiveGreenBuild::new(runtime, lease)?),
            open: Vec::new(),
            next_frame: 1,
            line_cursor: LineSourcePosition::default(),
            staged: None,
            pending: None,
            restart_join: None,
            restart_provenance: None,
            document_complete: false,
            poisoned: false,
        })
    }

    pub(super) fn reference_paragraph_frame(
        &self,
    ) -> Result<M11RecursiveGreenFrameId, M11BlockWriterError> {
        if self.poisoned || self.pending.is_some() || self.document_complete {
            return Err(M11BlockWriterError::InvalidCommand(
                "reference finalization requires a quiescent writer",
            ));
        }
        let frame = self.open.last().ok_or(M11BlockWriterError::InvalidCommand(
            "reference finalization requires an open Paragraph",
        ))?;
        if frame.kind != BlockKind::Paragraph {
            return Err(M11BlockWriterError::InvalidCommand(
                "reference finalization targets the terminal Paragraph",
            ));
        }
        Ok(frame.id)
    }

    pub(super) fn reference_green_build_mut(
        &mut self,
    ) -> Result<&mut M11RecursiveGreenBuild, M11BlockWriterError> {
        match &mut self.output {
            WriterOutput::Document(build) => Ok(build),
            WriterOutput::Fragment(_) => Err(M11BlockWriterError::InvalidCommand(
                "reference finalization requires the unpublished document build",
            )),
        }
    }

    pub(super) fn reference_staged_terminator(
        &self,
        frame: M11RecursiveGreenFrameId,
    ) -> Result<Option<M11ReferenceStagedTerminator>, M11BlockWriterError> {
        let Some(StagedSource::Terminator {
            metric,
            terminal,
            terminal_index,
        }) = self.staged
        else {
            return Ok(None);
        };
        if terminal != frame || self.open.get(terminal_index).map(|open| open.id) != Some(frame) {
            return Err(M11BlockWriterError::InvalidCommand(
                "staged reference terminator crossed its Paragraph",
            ));
        }
        let receipt = self.output.receipt();
        let start = SourceMetric::new(receipt.source_bytes, receipt.source_utf16)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        let end = start
            .checked_add(metric)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        Ok(Some(M11ReferenceStagedTerminator {
            start,
            end,
            raw_codepoint_contribution: u8::try_from(metric.utf16())
                .map_err(|_| M11BlockWriterError::CounterOverflow)?,
        }))
    }

    pub(super) fn complete_reference_fragment(
        &mut self,
        frame: M11RecursiveGreenFrameId,
        remove_paragraph: bool,
        consume_staged_terminator: bool,
        visible_remainder: Option<SourceMetric>,
    ) -> Result<Option<M11RecursiveGreenEvent>, M11BlockWriterError> {
        if self.pending.is_some()
            || self.open.last().map(|open| open.id) != Some(frame)
            || self.open.last().map(|open| open.kind) != Some(BlockKind::Paragraph)
        {
            return Err(M11BlockWriterError::InvalidCommand(
                "reference rewrite authority crossed the writer Paragraph",
            ));
        }
        if remove_paragraph {
            self.open.pop();
        } else if let Some(visible_remainder) = visible_remainder {
            self.open
                .last_mut()
                .and_then(|frame| frame.row_editable.as_mut())
                .ok_or(M11BlockWriterError::InvalidCommand(
                    "reference suffix lost Paragraph row geometry",
                ))?
                .retain_visible_suffix_at(visible_remainder)?;
        }
        if !consume_staged_terminator {
            return Ok(None);
        }
        let Some(StagedSource::Terminator {
            metric,
            terminal,
            terminal_index,
        }) = self.staged.take()
        else {
            return Err(M11BlockWriterError::InvalidCommand(
                "reference-only terminal lost its staged line ending",
            ));
        };
        let expected_terminal_index = if remove_paragraph {
            self.open.len()
        } else {
            self.open
                .len()
                .checked_sub(1)
                .ok_or(M11BlockWriterError::InvalidCommand(
                    "reference-only Paragraph has no owner",
                ))?
        };
        if terminal != frame || terminal_index != expected_terminal_index {
            return Err(M11BlockWriterError::InvalidCommand(
                "reference-only staged terminator crossed its Paragraph",
            ));
        }
        let owner_depth = u32::try_from(self.open.len().checked_sub(1).ok_or(
            M11BlockWriterError::InvalidCommand("reference-only terminator has no surviving owner"),
        )?)
        .map_err(|_| M11BlockWriterError::CounterOverflow)?;
        Ok(Some(M11RecursiveGreenEvent::Coverage {
            physical: green_metric(metric)?,
            owner_depth,
            part: M11RecursiveGreenCoveragePart::Gap,
            logical: M11RecursiveGreenLogicalAction::None,
        }))
    }

    /// Joins a parser capture to this writer's exact line-boundary state.
    ///
    /// The join rejects crossed open paths, deferred predecessors, active
    /// commands, and any non-line-boundary source cursor before minting the
    /// composite checkpoint.
    pub fn capture_restart_checkpoint(
        &self,
        parser: M11DirectBlockRestart,
    ) -> Result<M11BlockRestartCheckpoint, M11BlockRestartError> {
        if self.poisoned
            || self.document_complete
            || self.pending.is_some()
            || self.line_cursor != LineSourcePosition::default()
        {
            return Err(M11BlockRestartError::Pairing(
                "writer is not at a quiescent physical-line boundary",
            ));
        }
        if parser.open_kinds().len() != self.open.len()
            || !parser
                .open_kinds()
                .iter()
                .copied()
                .eq(self.open.iter().map(|frame| frame.kind))
        {
            return Err(M11BlockRestartError::Pairing(
                "parser and writer open paths differ",
            ));
        }
        if parser.restart_join() != self.restart_join {
            return Err(M11BlockRestartError::Pairing(
                "parser and writer restart transactions differ",
            ));
        }
        let deferred_matches = match (parser.deferred_role(), self.staged) {
            (M11DirectBlockDeferredRole::None, None) => true,
            (
                M11DirectBlockDeferredRole::Terminator,
                Some(StagedSource::Terminator {
                    terminal,
                    terminal_index,
                    ..
                }),
            ) => self
                .open
                .get(terminal_index)
                .is_some_and(|frame| frame.id == terminal),
            (
                M11DirectBlockDeferredRole::BlankGap { floor_depth },
                Some(StagedSource::BlankGap { .. }),
            ) => floor_depth.is_none_or(|depth| {
                self.open.get(depth).is_some_and(|frame| {
                    matches!(frame.kind, BlockKind::BlockQuote | BlockKind::Item(_))
                })
            }),
            _ => false,
        };
        if !deferred_matches {
            return Err(M11BlockRestartError::Pairing(
                "parser and writer deferred-source roles differ",
            ));
        }

        let receipt = self.output.receipt();
        let accepted_physical = SourceMetric::new(receipt.source_bytes, receipt.source_utf16)
            .ok_or(M11BlockRestartError::Pairing(
                "writer accepted source metric is valid",
            ))?;
        let deferred_metric = match self.staged {
            Some(StagedSource::Terminator { metric, .. })
            | Some(StagedSource::BlankGap { metric }) => metric,
            None => SourceMetric::default(),
        };
        let parser_physical =
            accepted_physical
                .checked_add(deferred_metric)
                .ok_or(M11BlockRestartError::Pairing(
                    "parser source metric overflow",
                ))?;
        if parser_physical.bytes() > u64::try_from(self.source.byte_len()).unwrap_or(u64::MAX)
            || parser_physical.utf16() > u64::try_from(self.source.utf16_len()).unwrap_or(u64::MAX)
        {
            return Err(M11BlockRestartError::Pairing(
                "parser source cut is inside the writer source",
            ));
        }
        let logical = SourceMetric::new(receipt.logical_bytes, receipt.logical_utf16).ok_or(
            M11BlockRestartError::Pairing("writer logical metric is valid"),
        )?;
        let green_boundary = match &self.output {
            WriterOutput::Document(build) => Some(build.capture_structural_boundary()?),
            WriterOutput::Fragment(_) => None,
        };
        if let Some(boundary) = &green_boundary {
            let physical = boundary.physical_metric();
            let boundary_logical = boundary.logical_metric();
            if boundary.source() != self.source
                || boundary.event_cut() != receipt.events
                || physical.bytes() != accepted_physical.bytes()
                || physical.utf16() != accepted_physical.utf16()
                || boundary_logical.bytes() != logical.bytes()
                || boundary_logical.utf16() != logical.utf16()
                || boundary.open_path().len() != self.open.len()
                || !boundary
                    .open_path()
                    .iter()
                    .zip(&self.open)
                    .all(|(green, writer)| {
                        green.frame() == writer.id && green.kind() == green_kind(writer.kind)
                    })
            {
                return Err(M11BlockRestartError::Pairing(
                    "Green and writer restart boundaries differ",
                ));
            }
        }
        Ok(M11BlockRestartCheckpoint {
            source: self.source,
            parser,
            open: self.open.clone().into_boxed_slice(),
            next_frame: self.next_frame,
            accepted_physical,
            parser_physical,
            logical,
            event_cut: receipt.events,
            staged: self.staged,
            restart_join: self.restart_join,
            green_boundary,
        })
    }

    /// Joins a donor-certified leading-reference remainder continuation to
    /// the exact structural cut minted by the canonical Green rewrite.
    pub(crate) fn capture_leading_reference_remainder_checkpoint(
        &self,
        parser: M11DirectLeadingReferenceRemainderContinuation,
        green_boundary: M11RecursiveGreenStructuralBoundary,
    ) -> Result<M11BlockRestartCheckpoint, M11BlockRestartError> {
        if self.poisoned
            || self.document_complete
            || self.pending.is_some()
            || self.line_cursor != LineSourcePosition::default()
            || self.open.len() != 2
            || self.open[0].kind != BlockKind::Document
            || self.open[1].kind != BlockKind::Paragraph
        {
            return Err(M11BlockRestartError::Pairing(
                "leading-reference remainder writer is a quiescent top-level Paragraph",
            ));
        }
        let physical = green_boundary.physical_metric();
        let logical = green_boundary.logical_metric();
        if green_boundary.source() != self.source
            || green_boundary.open_path().len() != self.open.len()
            || !green_boundary
                .open_path()
                .iter()
                .zip(&self.open)
                .all(|(green, writer)| {
                    green.frame() == writer.id && green.kind() == green_kind(writer.kind)
                })
        {
            return Err(M11BlockRestartError::Pairing(
                "leading-reference remainder Green and writer paths differ",
            ));
        }
        let accepted_physical = SourceMetric::new(physical.bytes(), physical.utf16()).ok_or(
            M11BlockRestartError::Pairing("leading-reference physical cut is valid"),
        )?;
        let logical = SourceMetric::new(logical.bytes(), logical.utf16()).ok_or(
            M11BlockRestartError::Pairing("leading-reference logical cut is valid"),
        )?;
        let parser = parser.into_restart()?;
        Ok(M11BlockRestartCheckpoint {
            source: self.source,
            parser,
            open: self.open.clone().into_boxed_slice(),
            next_frame: self.next_frame,
            accepted_physical,
            parser_physical: accepted_physical,
            logical,
            event_cut: green_boundary.event_cut(),
            staged: None,
            restart_join: self.restart_join,
            green_boundary: Some(green_boundary),
        })
    }

    /// Captures the stable EOF cut used to replace a reparsed document tail.
    /// The parser must be paused on its pending `Close(Document)` command;
    /// this writer-side proof independently requires that every child frame
    /// and deferred source role has already closed.
    pub fn capture_terminal_convergence_checkpoint(
        &self,
    ) -> Result<M11BlockTerminalConvergenceCheckpoint, M11BlockRestartError> {
        if self.poisoned
            || self.document_complete
            || self.pending.is_some()
            || self.line_cursor != LineSourcePosition::default()
            || self.staged.is_some()
            || self.open.len() != 1
            || self.open[0].kind != BlockKind::Document
        {
            return Err(M11BlockRestartError::Pairing(
                "writer is not at the pre-Document-close EOF boundary",
            ));
        }
        let WriterOutput::Document(build) = &self.output else {
            return Err(M11BlockRestartError::Pairing(
                "only a clean Green build can mint the base EOF boundary",
            ));
        };
        let receipt = self.output.receipt();
        let accepted_physical = SourceMetric::new(receipt.source_bytes, receipt.source_utf16)
            .ok_or(M11BlockRestartError::Pairing(
                "terminal writer source metric is valid",
            ))?;
        if accepted_physical.bytes() != u64::try_from(self.source.byte_len()).unwrap_or(u64::MAX)
            || accepted_physical.utf16()
                != u64::try_from(self.source.utf16_len()).unwrap_or(u64::MAX)
        {
            return Err(M11BlockRestartError::Pairing(
                "terminal writer boundary does not cover exact EOF",
            ));
        }
        let logical = SourceMetric::new(receipt.logical_bytes, receipt.logical_utf16).ok_or(
            M11BlockRestartError::Pairing("terminal writer logical metric is valid"),
        )?;
        Ok(M11BlockTerminalConvergenceCheckpoint {
            source: self.source,
            open: self.open.clone().into_boxed_slice(),
            next_frame: self.next_frame,
            accepted_physical,
            logical,
            event_cut: receipt.events,
            green_boundary: Some(build.capture_structural_boundary()?),
        })
    }

    /// Authenticates parser convergence, adopts the bounded target fragment,
    /// and activates the returned target checkpoint against the new committed
    /// Green root.
    ///
    /// Neither event/source cuts nor external depth are caller input. They are
    /// taken from the move-only Green boundaries joined into the restart and
    /// convergence checkpoints. Crossed roots, parser transactions, or open
    /// paths fail closed before storage mutation begins.
    pub fn adopt_converged_fragment(
        mut self,
        parser: M11DirectBlockRestart,
        mut target_restart: M11BlockRestartCheckpoint,
        mut old_convergence: M11BlockRestartCheckpoint,
        runtime: &mut DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        prefix: Option<ExactUnchangedPrefixWitness>,
        suffix: Option<ExactUnchangedSuffixWitness>,
        mut retained_prefix: Vec<M11BlockRestartCheckpoint>,
        mut retained_suffix: Vec<M11BlockRestartCheckpoint>,
        mut retained_terminal: M11BlockTerminalConvergenceCheckpoint,
    ) -> Result<
        (
            M11RecursiveGreenRoot,
            M11BlockStructuralAdoptionReceipt,
            Vec<M11BlockRestartCheckpoint>,
            M11BlockTerminalConvergenceCheckpoint,
        ),
        M11BlockRestartError,
    > {
        let mut fresh = self.capture_restart_checkpoint(parser)?;
        let provenance = self
            .restart_provenance
            .take()
            .ok_or(M11BlockRestartError::Pairing(
                "writer is not an active local restart fragment",
            ))?;
        if provenance.base_source != base.source()
            || provenance.base_maximum_frame_id != base.maximum_frame_id()
            || old_convergence.source != base.source()
            || fresh.source != self.source
            || fresh.source
                != runtime
                    .current_source_version()
                    .ok_or(M11BlockRestartError::Pairing(
                        "target source is not installed",
                    ))?
            || provenance.base_event_cut >= old_convergence.event_cut
            || provenance.target_accepted_start.bytes() >= fresh.accepted_physical.bytes()
            || !fresh
                .parser
                .is_future_compatible_with(&old_convergence.parser)
            || !same_open_path(&fresh.open, &old_convergence.open)
            || !same_staged_role(fresh.staged, old_convergence.staged)
        {
            return Err(M11BlockRestartError::Pairing(
                "target fragment did not converge to its exact base boundary",
            ));
        }
        if fresh.parser.last_line_length() != old_convergence.parser.last_line_length() {
            return Err(M11BlockRestartError::Pairing(
                "target convergence changed unchanged-suffix line length",
            ));
        }
        let base_convergence_line_ordinal = old_convergence.parser.line_ordinal();
        let target_convergence_line_ordinal = fresh.parser.line_ordinal();

        if target_restart.source != fresh.source
            || target_restart.green_boundary.is_some()
            || target_restart.event_cut != provenance.base_event_cut
            || target_restart.accepted_physical != provenance.target_accepted_start
            || target_restart.logical != provenance.target_logical_start
            || target_restart.restart_join != self.restart_join
            || !same_staged_role(target_restart.staged, provenance.start_staged)
            || !boundary_matches_open(&provenance.start_boundary, &target_restart.open)
        {
            return Err(M11BlockRestartError::Pairing(
                "target restart checkpoint was not captured at this fragment start",
            ));
        }

        let start_physical = provenance.start_boundary.physical_metric();
        let start_logical = provenance.start_boundary.logical_metric();
        if provenance.start_boundary.source() != base.source()
            || provenance.start_boundary.event_cut() != provenance.base_event_cut
            || start_physical.bytes() != provenance.target_accepted_start.bytes()
            || start_physical.utf16() != provenance.target_accepted_start.utf16()
            || start_logical.bytes() != provenance.target_logical_start.bytes()
            || start_logical.utf16() != provenance.target_logical_start.utf16()
            || provenance.start_boundary.open_path().len() != provenance.external_open_depth
            || !boundary_matches_open(&provenance.start_boundary, &self.open)
        {
            return Err(M11BlockRestartError::Pairing(
                "joined restart no longer matches its Green boundary",
            ));
        }

        let end_boundary =
            old_convergence
                .green_boundary
                .take()
                .ok_or(M11BlockRestartError::Pairing(
                    "base convergence checkpoint lacks committed Green authority",
                ))?;
        if !boundary_matches_open(&end_boundary, &old_convergence.open)
            || end_boundary.event_cut() != old_convergence.event_cut
            || end_boundary.physical_metric().bytes() != old_convergence.accepted_physical.bytes()
            || end_boundary.physical_metric().utf16() != old_convergence.accepted_physical.utf16()
            || end_boundary.logical_metric().bytes() != old_convergence.logical.bytes()
            || end_boundary.logical_metric().utf16() != old_convergence.logical.utf16()
        {
            return Err(M11BlockRestartError::Pairing(
                "base convergence checkpoint differs from its Green boundary",
            ));
        }

        let start_byte = usize::try_from(provenance.target_accepted_start.bytes())
            .map_err(|_| M11BlockRestartError::Pairing("restart byte cut fits usize"))?;
        let start_utf16 = usize::try_from(provenance.target_accepted_start.utf16())
            .map_err(|_| M11BlockRestartError::Pairing("restart UTF-16 cut fits usize"))?;
        let old_end_byte = usize::try_from(old_convergence.accepted_physical.bytes())
            .map_err(|_| M11BlockRestartError::Pairing("base end byte cut fits usize"))?;
        let old_end_utf16 = usize::try_from(old_convergence.accepted_physical.utf16())
            .map_err(|_| M11BlockRestartError::Pairing("base end UTF-16 cut fits usize"))?;
        let fresh_end_byte = usize::try_from(fresh.accepted_physical.bytes())
            .map_err(|_| M11BlockRestartError::Pairing("target end byte cut fits usize"))?;
        let fresh_end_utf16 = usize::try_from(fresh.accepted_physical.utf16())
            .map_err(|_| M11BlockRestartError::Pairing("target end UTF-16 cut fits usize"))?;
        let suffix_matches = match suffix.as_ref() {
            Some(suffix) => {
                suffix.base() == base.source()
                    && suffix.target() == fresh.source
                    && suffix.base_byte_start() == old_end_byte
                    && suffix.base_utf16_start() == old_end_utf16
                    && suffix.target_byte_start() == fresh_end_byte
                    && suffix.target_utf16_start() == fresh_end_utf16
            }
            None => {
                old_end_byte == base.source().byte_len()
                    && old_end_utf16 == base.source().utf16_len()
                    && fresh_end_byte == fresh.source.byte_len()
                    && fresh_end_utf16 == fresh.source.utf16_len()
            }
        };
        let prefix_matches = match prefix.as_ref() {
            Some(prefix) => {
                prefix.base() == base.source()
                    && prefix.target() == fresh.source
                    && prefix.byte_end() == start_byte
                    && prefix.utf16_end() == start_utf16
            }
            None => start_byte == 0 && start_utf16 == 0,
        };
        if !prefix_matches || !suffix_matches {
            return Err(M11BlockRestartError::Pairing(
                "prefix/suffix lineage differs from restart convergence cuts",
            ));
        }

        let WriterOutput::Fragment(mut fragment) = self.output else {
            return Err(M11BlockRestartError::Pairing(
                "convergence adoption requires a local fragment output",
            ));
        };
        let target_lease = fragment.lease.take().ok_or(M11BlockRestartError::Pairing(
            "fragment target lease was already transferred",
        ))?;
        let high_level_events = fragment.events.len();
        let fragment_source_bytes_read = fragment.source_bytes_read;
        let target_end_physical = green_metric(fresh.accepted_physical)?;
        let (mut root, green, target_start_boundary, target_end_boundary, rebase) =
            splice_m11_recursive_green_structural_atomic(
                runtime,
                base,
                target_lease,
                prefix,
                suffix,
                provenance.start_boundary,
                end_boundary,
                target_end_physical,
                &fragment.events,
            )?;
        let boundary_logical = target_end_boundary.logical_metric();
        if target_end_boundary.source() != fresh.source
            || target_end_boundary.event_cut() != fresh.event_cut
            || target_end_boundary.physical_metric().bytes() != fresh.accepted_physical.bytes()
            || target_end_boundary.physical_metric().utf16() != fresh.accepted_physical.utf16()
            || boundary_logical.bytes() != fresh.logical.bytes()
            || boundary_logical.utf16() != fresh.logical.utf16()
            || !boundary_matches_open(&target_end_boundary, &fresh.open)
            || target_start_boundary.source() != target_restart.source
            || target_start_boundary.event_cut() != target_restart.event_cut
            || target_start_boundary.physical_metric().bytes()
                != target_restart.accepted_physical.bytes()
            || target_start_boundary.physical_metric().utf16()
                != target_restart.accepted_physical.utf16()
            || target_start_boundary.logical_metric().bytes() != target_restart.logical.bytes()
            || target_start_boundary.logical_metric().utf16() != target_restart.logical.utf16()
            || !boundary_matches_open(&target_start_boundary, &target_restart.open)
        {
            root.begin_release(runtime)?;
            return Err(M11BlockRestartError::Pairing(
                "adopted Green boundary differs from parser/writer convergence",
            ));
        }
        target_restart.green_boundary = Some(target_start_boundary);
        fresh.green_boundary = Some(target_end_boundary);
        let Some(target_frame_floor) = root.maximum_frame_id().checked_add(1) else {
            root.begin_release(runtime)?;
            return Err(M11BlockRestartError::Pairing(
                "target Green frame identity space is exhausted",
            ));
        };
        let rebased =
            (|| {
                for checkpoint in &mut retained_prefix {
                    rebase_retained_prefix_checkpoint(checkpoint, &rebase, target_frame_floor)?;
                }
                for checkpoint in &mut retained_suffix {
                    rebase_retained_suffix_checkpoint(
                        checkpoint,
                        &rebase,
                        old_convergence.accepted_physical,
                        fresh.accepted_physical,
                        old_convergence.logical,
                        fresh.logical,
                        base_convergence_line_ordinal,
                        target_convergence_line_ordinal,
                        target_frame_floor,
                    )?;
                }
                rebase_retained_terminal_checkpoint(
                    &mut retained_terminal,
                    &rebase,
                    target_frame_floor,
                )?;
                let additional = 2_usize.checked_add(retained_suffix.len()).ok_or(
                    M11BlockRestartError::Pairing("rebased checkpoint count overflow"),
                )?;
                retained_prefix
                    .try_reserve(additional)
                    .map_err(|_| M11BlockWriterError::Allocation)?;
                retained_prefix.push(target_restart);
                retained_prefix.push(fresh);
                retained_prefix.append(&mut retained_suffix);
                validate_rebased_checkpoint_set(&retained_prefix, &retained_terminal)?;
                Ok::<_, M11BlockRestartError>((retained_prefix, retained_terminal))
            })();
        let (checkpoints, terminal) = match rebased {
            Ok(rebased) => rebased,
            Err(error) => {
                root.begin_release(runtime)?;
                return Err(error);
            }
        };
        Ok((
            root,
            M11BlockStructuralAdoptionReceipt {
                green,
                high_level_events,
                fragment_source_bytes_read,
            },
            checkpoints,
            terminal,
        ))
    }

    /// Adopts a bounded reparsed tail at the stable boundary immediately
    /// before `Close(Document)`.
    ///
    /// The target parser has already finalized every child block, but the
    /// invariant Document close remains in the retained base suffix. This
    /// gives EOF edits the same prefix-sharing structural splice as ordinary
    /// restart/convergence edits without pretending a still-open final child
    /// has the base frame identity.
    pub fn adopt_converged_terminal_fragment(
        mut self,
        mut target_restart: M11BlockRestartCheckpoint,
        mut old_terminal: M11BlockTerminalConvergenceCheckpoint,
        runtime: &mut DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        prefix: Option<ExactUnchangedPrefixWitness>,
        mut retained_prefix: Vec<M11BlockRestartCheckpoint>,
    ) -> Result<
        (
            M11RecursiveGreenRoot,
            M11BlockStructuralAdoptionReceipt,
            Vec<M11BlockRestartCheckpoint>,
            M11BlockTerminalConvergenceCheckpoint,
        ),
        M11BlockRestartError,
    > {
        let provenance = self
            .restart_provenance
            .take()
            .ok_or(M11BlockRestartError::Pairing(
                "writer is not an active local restart fragment",
            ))?;
        let receipt = self.output.receipt();
        let target_physical = SourceMetric::new(receipt.source_bytes, receipt.source_utf16).ok_or(
            M11BlockRestartError::Pairing("terminal target source metric is valid"),
        )?;
        let target_logical = SourceMetric::new(receipt.logical_bytes, receipt.logical_utf16)
            .ok_or(M11BlockRestartError::Pairing(
                "terminal target logical metric is valid",
            ))?;
        if self.poisoned
            || self.document_complete
            || self.pending.is_some()
            || self.line_cursor != LineSourcePosition::default()
            || self.staged.is_some()
            || self.open.len() != 1
            || self.open[0].kind != BlockKind::Document
            || provenance.base_source != base.source()
            || provenance.base_maximum_frame_id != base.maximum_frame_id()
            || old_terminal.source != base.source()
            || self.source
                != runtime
                    .current_source_version()
                    .ok_or(M11BlockRestartError::Pairing(
                        "terminal target source is not installed",
                    ))?
            || provenance.base_event_cut >= old_terminal.event_cut
            || provenance.target_accepted_start.bytes() >= target_physical.bytes()
            || !same_open_path(&self.open, &old_terminal.open)
        {
            return Err(M11BlockRestartError::Pairing(
                "target tail did not converge at the pre-Document-close boundary",
            ));
        }
        if target_physical.bytes() != u64::try_from(self.source.byte_len()).unwrap_or(u64::MAX)
            || target_physical.utf16() != u64::try_from(self.source.utf16_len()).unwrap_or(u64::MAX)
            || old_terminal.accepted_physical.bytes()
                != u64::try_from(base.source().byte_len()).unwrap_or(u64::MAX)
            || old_terminal.accepted_physical.utf16()
                != u64::try_from(base.source().utf16_len()).unwrap_or(u64::MAX)
        {
            return Err(M11BlockRestartError::Pairing(
                "terminal convergence does not cover exact source EOF",
            ));
        }
        if target_restart.source != self.source
            || target_restart.green_boundary.is_some()
            || target_restart.event_cut != provenance.base_event_cut
            || target_restart.accepted_physical != provenance.target_accepted_start
            || target_restart.logical != provenance.target_logical_start
            || target_restart.restart_join != self.restart_join
            || !same_staged_role(target_restart.staged, provenance.start_staged)
            || !boundary_matches_open(&provenance.start_boundary, &target_restart.open)
        {
            return Err(M11BlockRestartError::Pairing(
                "target restart checkpoint was not captured at this terminal fragment start",
            ));
        }
        let end_boundary =
            old_terminal
                .green_boundary
                .take()
                .ok_or(M11BlockRestartError::Pairing(
                    "base terminal checkpoint lacks committed Green authority",
                ))?;
        if !boundary_matches_open(&end_boundary, &old_terminal.open)
            || end_boundary.event_cut() != old_terminal.event_cut
            || end_boundary.physical_metric().bytes() != old_terminal.accepted_physical.bytes()
            || end_boundary.physical_metric().utf16() != old_terminal.accepted_physical.utf16()
            || end_boundary.logical_metric().bytes() != old_terminal.logical.bytes()
            || end_boundary.logical_metric().utf16() != old_terminal.logical.utf16()
        {
            return Err(M11BlockRestartError::Pairing(
                "base terminal checkpoint differs from its Green boundary",
            ));
        }
        let start_byte = usize::try_from(provenance.target_accepted_start.bytes())
            .map_err(|_| M11BlockRestartError::Pairing("restart byte cut fits usize"))?;
        let start_utf16 = usize::try_from(provenance.target_accepted_start.utf16())
            .map_err(|_| M11BlockRestartError::Pairing("restart UTF-16 cut fits usize"))?;
        let prefix_matches = match prefix.as_ref() {
            Some(prefix) => {
                prefix.base() == base.source()
                    && prefix.target() == self.source
                    && prefix.byte_end() == start_byte
                    && prefix.utf16_end() == start_utf16
            }
            None => start_byte == 0 && start_utf16 == 0,
        };
        if !prefix_matches {
            return Err(M11BlockRestartError::Pairing(
                "terminal prefix lineage differs from the restart cut",
            ));
        }

        let target_source = self.source;
        let target_open = self.open.clone().into_boxed_slice();
        let target_next_frame = self.next_frame;
        let WriterOutput::Fragment(mut fragment) = self.output else {
            return Err(M11BlockRestartError::Pairing(
                "terminal convergence adoption requires a local fragment output",
            ));
        };
        let target_lease = fragment.lease.take().ok_or(M11BlockRestartError::Pairing(
            "fragment target lease was already transferred",
        ))?;
        let high_level_events = fragment.events.len();
        let fragment_source_bytes_read = fragment.source_bytes_read;
        let target_end_physical = green_metric(target_physical)?;
        let (mut root, green, target_start_boundary, target_end_boundary, rebase) =
            splice_m11_recursive_green_structural_atomic(
                runtime,
                base,
                target_lease,
                prefix,
                None,
                provenance.start_boundary,
                end_boundary,
                target_end_physical,
                &fragment.events,
            )?;
        let target_boundary_logical = target_end_boundary.logical_metric();
        if target_end_boundary.source() != target_source
            || target_end_boundary.physical_metric().bytes() != target_physical.bytes()
            || target_end_boundary.physical_metric().utf16() != target_physical.utf16()
            || target_boundary_logical.bytes() != target_logical.bytes()
            || target_boundary_logical.utf16() != target_logical.utf16()
            || !boundary_matches_open(&target_end_boundary, &target_open)
            || target_start_boundary.source() != target_restart.source
            || target_start_boundary.event_cut() != target_restart.event_cut
            || target_start_boundary.physical_metric().bytes()
                != target_restart.accepted_physical.bytes()
            || target_start_boundary.physical_metric().utf16()
                != target_restart.accepted_physical.utf16()
            || target_start_boundary.logical_metric().bytes() != target_restart.logical.bytes()
            || target_start_boundary.logical_metric().utf16() != target_restart.logical.utf16()
            || !boundary_matches_open(&target_start_boundary, &target_restart.open)
        {
            root.begin_release(runtime)?;
            return Err(M11BlockRestartError::Pairing(
                "adopted terminal Green boundary differs from writer convergence",
            ));
        }
        target_restart.green_boundary = Some(target_start_boundary);
        let target_terminal = M11BlockTerminalConvergenceCheckpoint {
            source: target_source,
            open: target_open,
            next_frame: target_next_frame,
            accepted_physical: target_physical,
            logical: target_logical,
            event_cut: target_end_boundary.event_cut(),
            green_boundary: Some(target_end_boundary),
        };
        let Some(target_frame_floor) = root.maximum_frame_id().checked_add(1) else {
            root.begin_release(runtime)?;
            return Err(M11BlockRestartError::Pairing(
                "target Green frame identity space is exhausted",
            ));
        };
        let rebased = (|| {
            for checkpoint in &mut retained_prefix {
                rebase_retained_prefix_checkpoint(checkpoint, &rebase, target_frame_floor)?;
            }
            retained_prefix
                .try_reserve(1)
                .map_err(|_| M11BlockWriterError::Allocation)?;
            retained_prefix.push(target_restart);
            validate_rebased_checkpoint_set(&retained_prefix, &target_terminal)?;
            Ok::<_, M11BlockRestartError>(retained_prefix)
        })();
        let checkpoints = match rebased {
            Ok(checkpoints) => checkpoints,
            Err(error) => {
                root.begin_release(runtime)?;
                return Err(error);
            }
        };
        Ok((
            root,
            M11BlockStructuralAdoptionReceipt {
                green,
                high_level_events,
                fragment_source_bytes_read,
            },
            checkpoints,
            target_terminal,
        ))
    }

    /// Offers one parser command. Constant-time commands complete immediately;
    /// storage-producing commands complete through [`Self::poll`].
    pub fn offer_command(
        &mut self,
        command: BlockCommand,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        if self.poisoned {
            return Err(M11BlockWriterError::Poisoned);
        }
        if self.pending.is_some() {
            return Err(M11BlockWriterError::CommandPending);
        }
        if self.document_complete {
            return self.reject("command follows document completion");
        }
        let result = self.offer_command_inner(command);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn offer_command_inner(
        &mut self,
        command: BlockCommand,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        match command {
            BlockCommand::Enter { kind } => self.offer_enter(kind),
            BlockCommand::Coverage {
                owner,
                part,
                source,
                logical,
            } => self.offer_coverage(owner, part, source, logical),
            BlockCommand::StageTerminator { source, ending } => {
                self.stage_terminator(source, ending)
            }
            BlockCommand::ResolveTerminator { resolution } => self.resolve_terminator(resolution),
            BlockCommand::StageBlankGap { source } => self.stage_blank_gap(source),
            BlockCommand::ResolveBlankGap { owner } => self.resolve_blank_gap(owner),
            BlockCommand::FinalizeParagraph { outcome } => self.finalize_paragraph(outcome),
            BlockCommand::MarkFencedCodeBoundary { boundary } => {
                self.mark_fenced_code_boundary(boundary)
            }
            BlockCommand::Close {
                kind,
                final_facts,
                last_line_blank,
                child,
            } => self.offer_close(kind, final_facts, last_line_blank, child),
            BlockCommand::FinishLine { physical } => self.finish_line(physical),
            BlockCommand::FinishDocument => self.finish_document(),
        }
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockWriterPoll, M11BlockWriterError> {
        if fuel == 0 {
            return Err(M11BlockWriterError::ZeroFuel);
        }
        if self.poisoned {
            return Err(M11BlockWriterError::Poisoned);
        }
        let Some(mut pending) = self.pending.take() else {
            return Err(M11BlockWriterError::InvalidCommand(
                "no command awaits polling",
            ));
        };
        let result = match &mut pending {
            Pending::Events(events) => self.poll_events(runtime, fuel, events),
            Pending::Finish => self.poll_finish(runtime, fuel),
        };
        match result {
            Ok(poll) if poll.status == M11BlockWriterPollStatus::Pending => {
                self.pending = Some(pending);
                Ok(poll)
            }
            Ok(poll) => Ok(poll),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    fn poll_events(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        events: &mut PendingEvents,
    ) -> Result<M11BlockWriterPoll, M11BlockWriterError> {
        let mut transitions = 0;
        while transitions < fuel {
            if !events.in_flight {
                let event = events.events[usize::from(events.next)].ok_or(
                    M11BlockWriterError::InvalidCommand("pending event is absent"),
                )?;
                self.output.offer_event(event)?;
                events.in_flight = true;
            }
            if matches!(&self.output, WriterOutput::Fragment(_)) {
                transitions = transitions
                    .checked_add(1)
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                events.in_flight = false;
                events.next = events
                    .next
                    .checked_add(1)
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                if events.next == events.len {
                    return Ok(M11BlockWriterPoll {
                        status: M11BlockWriterPollStatus::CommandComplete,
                        transitions,
                    });
                }
                continue;
            }
            let WriterOutput::Document(build) = &mut self.output else {
                unreachable!("fragment output handled above")
            };
            let poll = build.poll(runtime, fuel - transitions)?;
            transitions = transitions
                .checked_add(poll.transitions())
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            match poll.status() {
                M11RecursiveGreenBuildStatus::NeedsInput => {
                    events.in_flight = false;
                    events.next = events
                        .next
                        .checked_add(1)
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                    if events.next == events.len {
                        return Ok(M11BlockWriterPoll {
                            status: M11BlockWriterPollStatus::CommandComplete,
                            transitions,
                        });
                    }
                }
                M11RecursiveGreenBuildStatus::Pending => {
                    return Ok(M11BlockWriterPoll {
                        status: M11BlockWriterPollStatus::Pending,
                        transitions,
                    });
                }
                M11RecursiveGreenBuildStatus::Complete
                | M11RecursiveGreenBuildStatus::Cancelled => {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "green build terminated while accepting a command",
                    ));
                }
            }
        }
        Ok(M11BlockWriterPoll {
            status: M11BlockWriterPollStatus::Pending,
            transitions,
        })
    }

    fn poll_finish(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11BlockWriterPoll, M11BlockWriterError> {
        let WriterOutput::Document(build) = &mut self.output else {
            return Err(M11BlockWriterError::InvalidCommand(
                "a local fragment cannot finish the complete document",
            ));
        };
        let poll = build.poll(runtime, fuel)?;
        match poll.status() {
            M11RecursiveGreenBuildStatus::Complete => {
                self.document_complete = true;
                Ok(M11BlockWriterPoll {
                    status: M11BlockWriterPollStatus::DocumentComplete,
                    transitions: poll.transitions(),
                })
            }
            M11RecursiveGreenBuildStatus::Pending => Ok(M11BlockWriterPoll {
                status: M11BlockWriterPollStatus::Pending,
                transitions: poll.transitions(),
            }),
            M11RecursiveGreenBuildStatus::NeedsInput => Err(M11BlockWriterError::InvalidCommand(
                "finished green input requested another event",
            )),
            M11RecursiveGreenBuildStatus::Cancelled => Err(M11BlockWriterError::InvalidCommand(
                "green build cancelled while finishing",
            )),
        }
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11RecursiveGreenRoot> {
        if !self.document_complete {
            return None;
        }
        match &mut self.output {
            WriterOutput::Document(build) => build.take_root(),
            WriterOutput::Fragment(_) => None,
        }
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11BlockWriterError> {
        self.pending = None;
        self.open.clear();
        self.staged = None;
        match &mut self.output {
            WriterOutput::Document(build) => build.begin_cancel(runtime)?,
            WriterOutput::Fragment(_) => {
                return Err(M11BlockWriterError::InvalidCommand(
                    "fragment cancellation is owned by its adoption transaction",
                ));
            }
        }
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenReclaimPoll, M11BlockWriterError> {
        match &mut self.output {
            WriterOutput::Document(build) => Ok(build.poll_cancel(runtime, fuel)?),
            WriterOutput::Fragment(_) => Err(M11BlockWriterError::InvalidCommand(
                "fragment cancellation completes synchronously",
            )),
        }
    }

    fn offer_enter(
        &mut self,
        kind: BlockKind,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        if is_renderable_block_kind(kind) {
            for ancestor in &mut self.open {
                if matches!(ancestor.kind, BlockKind::Item(_)) {
                    ancestor.has_renderable_descendant = true;
                }
            }
        }
        let green_kind = green_kind(kind);
        let property = open_property(kind)?;
        let frame = M11RecursiveGreenFrameId::new(self.next_frame)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        self.next_frame = self
            .next_frame
            .checked_add(1)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        self.open
            .try_reserve(1)
            .map_err(|_| M11BlockWriterError::Allocation)?;
        let logical_base = self.current_logical_metric()?;
        let physical_base = self.current_physical_metric()?;
        self.open.push(OpenFrame {
            id: frame,
            kind,
            fence: matches!(kind, BlockKind::FencedCode(_)).then_some(FenceFold {
                logical_base,
                info_end: None,
                literal_start: None,
            }),
            row_editable: is_renderable_block_kind(kind).then_some(RowEditableFold::new(
                physical_base,
                !matches!(kind, BlockKind::FencedCode(_)),
            )),
            has_renderable_descendant: false,
        });
        let enter = M11RecursiveGreenEvent::Enter {
            frame,
            kind: green_kind,
        };
        self.pending = Some(Pending::Events(match property {
            Some(property) => PendingEvents::two(enter, M11RecursiveGreenEvent::Property(property)),
            None => PendingEvents::one(enter),
        }));
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn offer_coverage(
        &mut self,
        owner: StackOwner,
        part: CoveragePart,
        source: LineSourceRange,
        logical: LogicalAction,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        self.require_no_staged_source()?;
        self.advance_line(source)?;
        let owner_depth = owner_depth(&self.open, owner)?;
        self.observe_row_coverage(owner_depth, green_part(part), logical, source.metric())?;
        let logical = green_logical_action(logical)?;
        self.pending = Some(Pending::Events(PendingEvents::one(
            M11RecursiveGreenEvent::Coverage {
                physical: green_metric(source.metric())?,
                owner_depth,
                part: green_part(part),
                logical,
            },
        )));
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn stage_terminator(
        &mut self,
        source: LineSourceRange,
        ending: LineEnding,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        self.require_no_staged_source()?;
        let terminal_index =
            self.open
                .len()
                .checked_sub(1)
                .ok_or(M11BlockWriterError::InvalidCommand(
                    "terminator has no open owner",
                ))?;
        let terminal = self
            .open
            .get(terminal_index)
            .ok_or(M11BlockWriterError::InvalidCommand(
                "terminator has no open owner",
            ))?
            .id;
        let expected = match ending {
            LineEnding::Lf | LineEnding::Cr => SourceMetric::new(1, 1),
            LineEnding::CrLf => SourceMetric::new(2, 2),
        }
        .expect("line-ending metrics are valid");
        if source.metric() != expected {
            return self.reject("terminator range differs from its ending kind");
        }
        self.advance_line(source)?;
        self.staged = Some(StagedSource::Terminator {
            metric: source.metric(),
            terminal,
            terminal_index,
        });
        Ok(M11BlockWriterOfferStatus::Complete)
    }

    fn resolve_terminator(
        &mut self,
        resolution: TerminatorResolution,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        let Some(StagedSource::Terminator {
            metric,
            terminal,
            terminal_index,
        }) = self.staged.take()
        else {
            return self.reject("terminator resolution has no staged terminator");
        };
        let owner_depth = owner_depth_for_frame(&self.open, terminal, terminal_index)?;
        let (part, logical) = match resolution {
            TerminatorResolution::ContinueCanonicalNewline => (
                M11RecursiveGreenCoveragePart::Content,
                M11RecursiveGreenLogicalAction::CanonicalNewline,
            ),
            TerminatorResolution::CloseNone => (
                M11RecursiveGreenCoveragePart::Terminal,
                M11RecursiveGreenLogicalAction::None,
            ),
        };
        self.observe_row_coverage(
            owner_depth,
            part,
            match resolution {
                TerminatorResolution::ContinueCanonicalNewline => LogicalAction::CanonicalNewline,
                TerminatorResolution::CloseNone => LogicalAction::None,
            },
            metric,
        )?;
        self.pending = Some(Pending::Events(PendingEvents::one(
            M11RecursiveGreenEvent::Coverage {
                physical: green_metric(metric)?,
                owner_depth,
                part,
                logical,
            },
        )));
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn stage_blank_gap(
        &mut self,
        source: LineSourceRange,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        self.require_no_staged_source()?;
        self.advance_line(source)?;
        self.staged = Some(StagedSource::BlankGap {
            metric: source.metric(),
        });
        Ok(M11BlockWriterOfferStatus::Complete)
    }

    fn resolve_blank_gap(
        &mut self,
        owner: StackOwner,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        let Some(StagedSource::BlankGap { metric }) = self.staged.take() else {
            return self.reject("blank-gap resolution has no staged gap");
        };
        let owner_depth = owner_depth(&self.open, owner)?;
        self.observe_row_coverage(
            owner_depth,
            M11RecursiveGreenCoveragePart::Gap,
            LogicalAction::None,
            metric,
        )?;
        self.pending = Some(Pending::Events(PendingEvents::one(
            M11RecursiveGreenEvent::Coverage {
                physical: green_metric(metric)?,
                owner_depth,
                part: M11RecursiveGreenCoveragePart::Gap,
                logical: M11RecursiveGreenLogicalAction::None,
            },
        )));
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn finalize_paragraph(
        &mut self,
        outcome: ParagraphOutcome,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        let frame = self
            .open
            .last_mut()
            .ok_or(M11BlockWriterError::InvalidCommand(
                "paragraph finalization has no open frame",
            ))?;
        if frame.kind != BlockKind::Paragraph {
            return self.reject("paragraph finalization does not target Paragraph");
        }
        let ParagraphOutcome::SetextHeading { level } = outcome;
        let heading = super::HeadingFacts::new(level.get(), HeadingStyle::Setext).ok_or(
            M11BlockWriterError::InvalidCommand("invalid Setext heading facts"),
        )?;
        let final_kind = BlockKind::Heading(heading);
        let property = open_property(final_kind)?.ok_or(M11BlockWriterError::InvalidCommand(
            "Heading property is absent",
        ))?;
        frame.kind = final_kind;
        self.pending = Some(Pending::Events(PendingEvents::one(
            M11RecursiveGreenEvent::RetypeOpen {
                frame: frame.id,
                kind: green_kind(final_kind),
                property: Some(property),
            },
        )));
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn mark_fenced_code_boundary(
        &mut self,
        boundary: FencedCodeBoundary,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        let logical_now = self.current_logical_metric()?;
        let physical_now = self.current_physical_metric()?;
        let frame = self
            .open
            .last_mut()
            .ok_or(M11BlockWriterError::InvalidCommand(
                "fence boundary has no open frame",
            ))?;
        let fold = frame
            .fence
            .as_mut()
            .ok_or(M11BlockWriterError::InvalidCommand(
                "fence boundary targets another kind",
            ))?;
        let relative = metric_difference(logical_now, fold.logical_base)?;
        match boundary {
            FencedCodeBoundary::InfoEnd
                if fold.info_end.is_none() && fold.literal_start.is_none() =>
            {
                fold.info_end = Some(relative);
            }
            FencedCodeBoundary::LiteralStart
                if fold.info_end.is_some() && fold.literal_start.is_none() =>
            {
                fold.literal_start = Some(relative);
                frame
                    .row_editable
                    .as_mut()
                    .ok_or(M11BlockWriterError::InvalidCommand(
                        "FencedCode row-editable fold is absent",
                    ))?
                    .reset_at(physical_now)?;
            }
            FencedCodeBoundary::InfoEnd | FencedCodeBoundary::LiteralStart => {
                return self.reject("fence boundaries are duplicated or reversed");
            }
        }
        Ok(M11BlockWriterOfferStatus::Complete)
    }

    fn offer_close(
        &mut self,
        kind: BlockKind,
        final_facts: FinalFacts,
        last_line_blank: bool,
        child: super::ClosedChild,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        let frame = *self.open.last().ok_or(M11BlockWriterError::InvalidCommand(
            "close has no open frame",
        ))?;
        if frame.kind != kind {
            return self.reject("close kind differs from open frame");
        }
        let needs_empty_item_row =
            matches!(frame.kind, BlockKind::Item(_)) && !frame.has_renderable_descendant;
        let close = self.close_facts(frame, final_facts)?;
        self.open.pop();
        let item_exit = M11RecursiveGreenEvent::Exit {
            frame: frame.id,
            final_kind: green_kind(kind),
            close,
            last_line_blank,
            child: M11RecursiveGreenClosedChild::new(
                child.ends_blank(),
                child.item_loose_if_nonlast(),
                child.item_loose_if_last(),
            ),
        };
        self.pending = Some(Pending::Events(if needs_empty_item_row {
            for ancestor in &mut self.open {
                if matches!(ancestor.kind, BlockKind::Item(_)) {
                    ancestor.has_renderable_descendant = true;
                }
            }
            let row_frame = M11RecursiveGreenFrameId::new(self.next_frame)
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            self.next_frame = self
                .next_frame
                .checked_add(1)
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            let row_kind = M11RecursiveGreenKind::new(KIND_EMPTY_ITEM_ROW)
                .expect("empty-item row kind is nonzero");
            PendingEvents::three(
                M11RecursiveGreenEvent::Enter {
                    frame: row_frame,
                    kind: row_kind,
                },
                M11RecursiveGreenEvent::Exit {
                    frame: row_frame,
                    final_kind: row_kind,
                    close: None,
                    last_line_blank: false,
                    child: M11RecursiveGreenClosedChild::new(false, false, false),
                },
                item_exit,
            )
        } else {
            PendingEvents::one(item_exit)
        }));
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn finish_line(
        &mut self,
        physical: SourceMetric,
    ) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        if self.line_cursor.byte() != physical.bytes()
            || self.line_cursor.utf16() != physical.utf16()
        {
            return self.reject("FinishLine differs from claimed source partition");
        }
        self.line_cursor = LineSourcePosition::default();
        Ok(M11BlockWriterOfferStatus::Complete)
    }

    fn finish_document(&mut self) -> Result<M11BlockWriterOfferStatus, M11BlockWriterError> {
        if !self.open.is_empty()
            || self.staged.is_some()
            || self.line_cursor != LineSourcePosition::default()
        {
            return self.reject("FinishDocument has outstanding writer state");
        }
        let WriterOutput::Document(build) = &mut self.output else {
            return self.reject("a local fragment cannot emit FinishDocument");
        };
        build.finish_input()?;
        self.pending = Some(Pending::Finish);
        Ok(M11BlockWriterOfferStatus::Pending)
    }

    fn advance_line(&mut self, source: LineSourceRange) -> Result<(), M11BlockWriterError> {
        if source.start() != self.line_cursor {
            return self.reject("source range is not the next line-relative partition");
        }
        self.line_cursor = source.end();
        Ok(())
    }

    fn require_no_staged_source(&self) -> Result<(), M11BlockWriterError> {
        if self.staged.is_some() {
            Err(M11BlockWriterError::InvalidCommand(
                "source command crosses an unresolved staged range",
            ))
        } else {
            Ok(())
        }
    }

    fn current_logical_metric(&self) -> Result<SourceMetric, M11BlockWriterError> {
        let receipt = self.output.receipt();
        SourceMetric::new(receipt.logical_bytes, receipt.logical_utf16).ok_or(
            M11BlockWriterError::InvalidCommand("green logical metric is invalid"),
        )
    }

    fn current_physical_metric(&self) -> Result<SourceMetric, M11BlockWriterError> {
        let receipt = self.output.receipt();
        SourceMetric::new(receipt.source_bytes, receipt.source_utf16).ok_or(
            M11BlockWriterError::InvalidCommand("green physical metric is invalid"),
        )
    }

    fn observe_row_coverage(
        &mut self,
        owner_depth: u32,
        part: M11RecursiveGreenCoveragePart,
        logical: LogicalAction,
        physical: SourceMetric,
    ) -> Result<(), M11BlockWriterError> {
        let Some(row_index) = self
            .open
            .iter()
            .rposition(|frame| frame.row_editable.is_some())
        else {
            return Ok(());
        };
        if !self.open[row_index]
            .row_editable
            .is_some_and(|fold| fold.tracking)
        {
            return Ok(());
        }
        let owner_index = self
            .open
            .len()
            .checked_sub(
                usize::try_from(owner_depth)
                    .map_err(|_| M11BlockWriterError::CounterOverflow)?
                    .checked_add(1)
                    .ok_or(M11BlockWriterError::CounterOverflow)?,
            )
            .ok_or(M11BlockWriterError::InvalidCommand(
                "row-editable coverage owner is outside the open path",
            ))?;
        let physical_start = self.current_physical_metric()?;
        let source_compatible =
            owner_index == row_index && part == M11RecursiveGreenCoveragePart::Content;
        if source_compatible && logical == LogicalAction::CanonicalText {
            return self.observe_canonical_text_row_coverage(row_index, physical_start, physical);
        }
        let compatible = source_compatible
            && matches!(
                logical,
                LogicalAction::Identity | LogicalAction::CanonicalNewline
            );
        self.observe_row_segment(row_index, physical_start, physical, compatible)
    }

    fn observe_canonical_text_row_coverage(
        &mut self,
        row_index: usize,
        physical_start: SourceMetric,
        physical: SourceMetric,
    ) -> Result<(), M11BlockWriterError> {
        let start_byte = usize::try_from(physical_start.bytes())
            .map_err(|_| M11BlockWriterError::CounterOverflow)?;
        let physical_bytes =
            usize::try_from(physical.bytes()).map_err(|_| M11BlockWriterError::CounterOverflow)?;
        let end_byte = start_byte
            .checked_add(physical_bytes)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        let lease = self
            .geometry_lease
            .take()
            .ok_or(M11BlockWriterError::Poisoned)?;
        let mut cursor = lease.cursor_in(start_byte..end_byte)?;
        let mut chunk = [0_u8; 512];
        let mut absolute_bytes = physical_start.bytes();
        let mut absolute_utf16 = physical_start.utf16();
        let mut segment_start = physical_start;
        let mut saw_nul = false;
        while cursor.position() < cursor.end() {
            let read = cursor.read(&mut chunk);
            if read == 0 {
                return Err(M11BlockWriterError::InvalidCommand(
                    "canonical-text geometry cursor stopped early",
                ));
            }
            for byte in chunk[..read].iter().copied() {
                if byte == 0 {
                    let absolute = SourceMetric::new(absolute_bytes, absolute_utf16)
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                    saw_nul = true;
                    let prefix = metric_difference(absolute, segment_start)?;
                    if prefix.bytes() != 0 {
                        self.observe_row_segment(row_index, segment_start, prefix, true)?;
                    }
                    let nul_metric = SourceMetric::new(1, 1).expect("NUL source metric is valid");
                    self.observe_row_segment(row_index, absolute, nul_metric, false)?;
                    segment_start = absolute
                        .checked_add(nul_metric)
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                    absolute_bytes = segment_start.bytes();
                    absolute_utf16 = segment_start.utf16();
                    continue;
                }
                let utf16 = if byte < 0x80 || (0xc0..0xf0).contains(&byte) {
                    1
                } else if byte >= 0xf0 {
                    2
                } else {
                    0
                };
                absolute_bytes = absolute_bytes
                    .checked_add(1)
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                absolute_utf16 = absolute_utf16
                    .checked_add(utf16)
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
            }
        }
        let lease = cursor.finish()?;
        if lease.version() != self.source {
            return Err(M11BlockWriterError::InvalidCommand(
                "canonical-text geometry crossed source versions",
            ));
        }
        self.geometry_lease = Some(lease);
        let physical_end = physical_start
            .checked_add(physical)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        let absolute = SourceMetric::new(absolute_bytes, absolute_utf16)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        if absolute != physical_end {
            return Err(M11BlockWriterError::InvalidCommand(
                "canonical-text geometry differs from source metrics",
            ));
        }
        if !saw_nul {
            return self.observe_row_segment(row_index, physical_start, physical, true);
        }
        let suffix = metric_difference(physical_end, segment_start)?;
        if suffix.bytes() != 0 {
            self.observe_row_segment(row_index, segment_start, suffix, true)?;
        }
        Ok(())
    }

    fn observe_row_segment(
        &mut self,
        row_index: usize,
        physical_start: SourceMetric,
        physical: SourceMetric,
        compatible: bool,
    ) -> Result<(), M11BlockWriterError> {
        self.open
            .get_mut(row_index)
            .and_then(|frame| frame.row_editable.as_mut())
            .ok_or(M11BlockWriterError::InvalidCommand(
                "renderable frame lost its row-editable fold",
            ))?
            .observe(physical_start, physical, compatible)
    }

    fn close_facts(
        &self,
        frame: OpenFrame,
        facts: FinalFacts,
    ) -> Result<Option<M11RecursiveGreenCloseFacts>, M11BlockWriterError> {
        let cached_row = frame
            .row_editable
            .map(RowEditableFold::cached)
            .transpose()?;
        if cached_row.is_some() != is_renderable_block_kind(frame.kind) {
            return Err(M11BlockWriterError::InvalidCommand(
                "row-editable close facts differ from final block kind",
            ));
        }
        match (frame.kind, facts) {
            (BlockKind::List(_), FinalFacts::List { tight }) => {
                Ok(Some(close_facts(FACT_LIST, &[u8::from(tight)])?))
            }
            (BlockKind::FencedCode(_), FinalFacts::FencedCode(facts)) => {
                let fold = frame.fence.ok_or(M11BlockWriterError::InvalidCommand(
                    "FencedCode close lost its projection fold",
                ))?;
                let info_end = fold.info_end.ok_or(M11BlockWriterError::InvalidCommand(
                    "FencedCode close is missing InfoEnd",
                ))?;
                let literal_start =
                    fold.literal_start
                        .ok_or(M11BlockWriterError::InvalidCommand(
                            "FencedCode close is missing LiteralStart",
                        ))?;
                let logical_end =
                    metric_difference(self.current_logical_metric()?, fold.logical_base)?;
                if !metric_precedes(info_end, literal_start)
                    || !metric_precedes(literal_start, logical_end)
                {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "FencedCode logical boundaries are out of order",
                    ));
                }
                Ok(Some(close_facts_with_cached_row(
                    FACT_CODE,
                    &[u8::from(facts.closed())],
                    cached_row.ok_or(M11BlockWriterError::InvalidCommand(
                        "FencedCode close lost cached row geometry",
                    ))?,
                )?))
            }
            (BlockKind::List(_), _)
            | (BlockKind::FencedCode(_), _)
            | (_, FinalFacts::List { .. } | FinalFacts::FencedCode(_)) => Err(
                M11BlockWriterError::InvalidCommand("close facts differ from block kind"),
            ),
            (_, FinalFacts::None) => cached_row
                .map(|cached| close_facts_with_cached_row(FACT_ROW_EDITABLE, &[], cached))
                .transpose(),
        }
    }

    fn reject<T>(&self, message: &'static str) -> Result<T, M11BlockWriterError> {
        Err(M11BlockWriterError::InvalidCommand(message))
    }
}

fn same_open_path(left: &[OpenFrame], right: &[OpenFrame]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.id == right.id && left.kind == right.kind)
}

fn rebase_retained_prefix_checkpoint(
    checkpoint: &mut M11BlockRestartCheckpoint,
    rebase: &M11RecursiveGreenStructuralSpliceRebase,
    target_frame_floor: u64,
) -> Result<(), M11BlockRestartError> {
    let boundary = checkpoint
        .green_boundary
        .take()
        .ok_or(M11BlockRestartError::Pairing(
            "retained prefix checkpoint lacks committed Green authority",
        ))?;
    validate_restart_boundary(checkpoint, &boundary)?;
    let boundary = rebase.rebase_prefix(boundary)?;
    checkpoint.source = boundary.source();
    checkpoint.accepted_physical = block_metric(boundary.physical_metric())?;
    checkpoint.logical = block_metric(boundary.logical_metric())?;
    checkpoint.event_cut = boundary.event_cut();
    checkpoint.next_frame = checkpoint.next_frame.max(target_frame_floor);
    validate_restart_boundary(checkpoint, &boundary)?;
    checkpoint.green_boundary = Some(boundary);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rebase_retained_suffix_checkpoint(
    checkpoint: &mut M11BlockRestartCheckpoint,
    rebase: &M11RecursiveGreenStructuralSpliceRebase,
    base_physical_end: SourceMetric,
    target_physical_end: SourceMetric,
    base_logical_end: SourceMetric,
    target_logical_end: SourceMetric,
    base_convergence_line_ordinal: u64,
    target_convergence_line_ordinal: u64,
    target_frame_floor: u64,
) -> Result<(), M11BlockRestartError> {
    let boundary = checkpoint
        .green_boundary
        .take()
        .ok_or(M11BlockRestartError::Pairing(
            "retained suffix checkpoint lacks committed Green authority",
        ))?;
    validate_restart_boundary(checkpoint, &boundary)?;
    checkpoint.parser_physical = translate_block_metric(
        checkpoint.parser_physical,
        base_physical_end,
        target_physical_end,
    )?;
    checkpoint.parser.rebase_unchanged_suffix_line_ordinal(
        base_convergence_line_ordinal,
        target_convergence_line_ordinal,
    )?;
    for frame in &mut checkpoint.open {
        if let Some(row) = frame.row_editable.as_mut() {
            rebase_retained_row_fold(row, base_physical_end, target_physical_end)?;
        }
        let Some(fence) = frame.fence.as_mut() else {
            continue;
        };
        let after_convergence = fence.logical_base.bytes() >= base_logical_end.bytes()
            && fence.logical_base.utf16() >= base_logical_end.utf16();
        let before_convergence = fence.logical_base.bytes() <= base_logical_end.bytes()
            && fence.logical_base.utf16() <= base_logical_end.utf16();
        if !after_convergence && !before_convergence {
            return Err(M11BlockRestartError::Pairing(
                "retained suffix frame crossed logical convergence coordinates",
            ));
        }
        if after_convergence {
            fence.logical_base =
                translate_block_metric(fence.logical_base, base_logical_end, target_logical_end)?;
        }
    }
    let boundary = rebase.rebase_suffix(boundary)?;
    checkpoint.source = boundary.source();
    checkpoint.accepted_physical = block_metric(boundary.physical_metric())?;
    checkpoint.logical = block_metric(boundary.logical_metric())?;
    checkpoint.event_cut = boundary.event_cut();
    checkpoint.next_frame = checkpoint.next_frame.max(target_frame_floor);
    validate_restart_boundary(checkpoint, &boundary)?;
    checkpoint.green_boundary = Some(boundary);
    Ok(())
}

fn rebase_retained_terminal_checkpoint(
    checkpoint: &mut M11BlockTerminalConvergenceCheckpoint,
    rebase: &M11RecursiveGreenStructuralSpliceRebase,
    target_frame_floor: u64,
) -> Result<(), M11BlockRestartError> {
    let boundary = checkpoint
        .green_boundary
        .take()
        .ok_or(M11BlockRestartError::Pairing(
            "retained terminal checkpoint lacks committed Green authority",
        ))?;
    validate_terminal_boundary(checkpoint, &boundary)?;
    let boundary = rebase.rebase_suffix(boundary)?;
    checkpoint.source = boundary.source();
    checkpoint.accepted_physical = block_metric(boundary.physical_metric())?;
    checkpoint.logical = block_metric(boundary.logical_metric())?;
    checkpoint.event_cut = boundary.event_cut();
    checkpoint.next_frame = checkpoint.next_frame.max(target_frame_floor);
    validate_terminal_boundary(checkpoint, &boundary)?;
    checkpoint.green_boundary = Some(boundary);
    Ok(())
}

fn validate_restart_boundary(
    checkpoint: &M11BlockRestartCheckpoint,
    boundary: &M11RecursiveGreenStructuralBoundary,
) -> Result<(), M11BlockRestartError> {
    if boundary.source() != checkpoint.source
        || boundary.event_cut() != checkpoint.event_cut
        || boundary.physical_metric().bytes() != checkpoint.accepted_physical.bytes()
        || boundary.physical_metric().utf16() != checkpoint.accepted_physical.utf16()
        || boundary.logical_metric().bytes() != checkpoint.logical.bytes()
        || boundary.logical_metric().utf16() != checkpoint.logical.utf16()
        || !boundary_matches_open(boundary, &checkpoint.open)
    {
        return Err(M11BlockRestartError::Pairing(
            "retained restart checkpoint differs from its Green boundary",
        ));
    }
    Ok(())
}

fn validate_terminal_boundary(
    checkpoint: &M11BlockTerminalConvergenceCheckpoint,
    boundary: &M11RecursiveGreenStructuralBoundary,
) -> Result<(), M11BlockRestartError> {
    if boundary.source() != checkpoint.source
        || boundary.event_cut() != checkpoint.event_cut
        || boundary.physical_metric().bytes() != checkpoint.accepted_physical.bytes()
        || boundary.physical_metric().utf16() != checkpoint.accepted_physical.utf16()
        || boundary.logical_metric().bytes() != checkpoint.logical.bytes()
        || boundary.logical_metric().utf16() != checkpoint.logical.utf16()
        || !boundary_matches_open(boundary, &checkpoint.open)
    {
        return Err(M11BlockRestartError::Pairing(
            "retained terminal checkpoint differs from its Green boundary",
        ));
    }
    Ok(())
}

fn validate_rebased_checkpoint_set(
    checkpoints: &[M11BlockRestartCheckpoint],
    terminal: &M11BlockTerminalConvergenceCheckpoint,
) -> Result<(), M11BlockRestartError> {
    if checkpoints.is_empty()
        || checkpoints.iter().any(|checkpoint| {
            checkpoint.source != terminal.source || checkpoint.green_boundary.is_none()
        })
        || checkpoints.windows(2).any(|pair| {
            pair[0].parser_physical.bytes() > pair[1].parser_physical.bytes()
                || pair[0].parser_physical.utf16() > pair[1].parser_physical.utf16()
                || pair[0].accepted_physical.bytes() > pair[1].accepted_physical.bytes()
                || pair[0].accepted_physical.utf16() > pair[1].accepted_physical.utf16()
        })
        || terminal.green_boundary.is_none()
        || terminal.accepted_physical.bytes()
            != u64::try_from(terminal.source.byte_len()).unwrap_or(u64::MAX)
        || terminal.accepted_physical.utf16()
            != u64::try_from(terminal.source.utf16_len()).unwrap_or(u64::MAX)
    {
        return Err(M11BlockRestartError::Pairing(
            "rebased checkpoint set is not ordered target authority",
        ));
    }
    Ok(())
}

fn block_metric(
    metric: M11RecursiveGreenSourceMetric,
) -> Result<SourceMetric, M11BlockRestartError> {
    SourceMetric::new(metric.bytes(), metric.utf16()).ok_or(M11BlockRestartError::Pairing(
        "recursive-Green checkpoint metric is invalid",
    ))
}

fn translate_block_metric(
    value: SourceMetric,
    base: SourceMetric,
    target: SourceMetric,
) -> Result<SourceMetric, M11BlockRestartError> {
    SourceMetric::new(
        translate_checkpoint_cut(value.bytes(), base.bytes(), target.bytes())?,
        translate_checkpoint_cut(value.utf16(), base.utf16(), target.utf16())?,
    )
    .ok_or(M11BlockRestartError::Pairing(
        "rebased checkpoint metric is invalid",
    ))
}

fn rebase_retained_row_fold(
    row: &mut RowEditableFold,
    base_convergence: SourceMetric,
    target_convergence: SourceMetric,
) -> Result<(), M11BlockRestartError> {
    let base = row.physical_base;
    let target_base = translate_row_fold_cut(base, base_convergence, target_convergence)?;
    row.start = row
        .start
        .map(|relative| {
            let absolute = base
                .checked_add(relative)
                .ok_or(M11BlockRestartError::Pairing("retained row start overflow"))?;
            let target = translate_row_fold_cut(absolute, base_convergence, target_convergence)?;
            metric_difference(target, target_base).map_err(M11BlockRestartError::Writer)
        })
        .transpose()?;
    let absolute_end = base
        .checked_add(row.end)
        .ok_or(M11BlockRestartError::Pairing("retained row end overflow"))?;
    let target_end = translate_row_fold_cut(absolute_end, base_convergence, target_convergence)?;
    row.end = metric_difference(target_end, target_base).map_err(M11BlockRestartError::Writer)?;
    row.physical_base = target_base;
    Ok(())
}

fn translate_row_fold_cut(
    value: SourceMetric,
    base: SourceMetric,
    target: SourceMetric,
) -> Result<SourceMetric, M11BlockRestartError> {
    let after = value.bytes() >= base.bytes() && value.utf16() >= base.utf16();
    let before = value.bytes() <= base.bytes() && value.utf16() <= base.utf16();
    if !after && !before {
        return Err(M11BlockRestartError::Pairing(
            "retained row cut crossed physical convergence coordinates",
        ));
    }
    if after {
        translate_block_metric(value, base, target)
    } else {
        Ok(value)
    }
}

fn translate_checkpoint_cut(
    value: u64,
    base: u64,
    target: u64,
) -> Result<u64, M11BlockRestartError> {
    if target >= base {
        value
            .checked_add(target - base)
            .ok_or(M11BlockRestartError::Pairing(
                "rebased checkpoint coordinate overflow",
            ))
    } else {
        value
            .checked_sub(base - target)
            .ok_or(M11BlockRestartError::Pairing(
                "rebased checkpoint coordinate underflow",
            ))
    }
}

fn boundary_matches_open(
    boundary: &M11RecursiveGreenStructuralBoundary,
    open: &[OpenFrame],
) -> bool {
    boundary.open_path().len() == open.len()
        && boundary
            .open_path()
            .iter()
            .zip(open)
            .all(|(green, writer)| {
                green.frame() == writer.id && green.kind() == green_kind(writer.kind)
            })
}

fn same_staged_role(left: Option<StagedSource>, right: Option<StagedSource>) -> bool {
    match (left, right) {
        (None, None) => true,
        (
            Some(StagedSource::Terminator {
                metric: left_metric,
                terminal: left_terminal,
                terminal_index: left_index,
            }),
            Some(StagedSource::Terminator {
                metric: right_metric,
                terminal: right_terminal,
                terminal_index: right_index,
            }),
        ) => {
            left_metric == right_metric
                && left_terminal == right_terminal
                && left_index == right_index
        }
        (
            Some(StagedSource::BlankGap {
                metric: left_metric,
            }),
            Some(StagedSource::BlankGap {
                metric: right_metric,
            }),
        ) => left_metric == right_metric,
        _ => false,
    }
}

fn owner_depth(open: &[OpenFrame], owner: StackOwner) -> Result<u32, M11BlockWriterError> {
    let depth = owner.generations_from_top();
    if usize::try_from(depth)
        .ok()
        .is_none_or(|depth| depth >= open.len())
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "coverage owner is outside the open path",
        ));
    }
    Ok(depth)
}

fn owner_depth_for_frame(
    open: &[OpenFrame],
    frame: M11RecursiveGreenFrameId,
    index: usize,
) -> Result<u32, M11BlockWriterError> {
    if open
        .get(index)
        .is_none_or(|candidate| candidate.id != frame)
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "staged source owner is no longer open",
        ));
    }
    u32::try_from(open.len() - 1 - index).map_err(|_| M11BlockWriterError::CounterOverflow)
}

fn green_kind(kind: BlockKind) -> M11RecursiveGreenKind {
    let value = match kind {
        BlockKind::Document => KIND_DOCUMENT,
        BlockKind::BlockQuote => KIND_BLOCK_QUOTE,
        BlockKind::List(_) => KIND_LIST,
        BlockKind::Item(_) => KIND_ITEM,
        BlockKind::Paragraph => KIND_PARAGRAPH,
        BlockKind::IndentedCode => KIND_INDENTED_CODE,
        BlockKind::FencedCode(_) => KIND_FENCED_CODE,
        BlockKind::HtmlBlock(_) => KIND_HTML_BLOCK,
        BlockKind::Heading(_) => KIND_HEADING,
        BlockKind::ThematicBreak => KIND_THEMATIC_BREAK,
    };
    M11RecursiveGreenKind::new(value).expect("kind registry values are nonzero")
}

fn fact_tag(value: u16) -> M11RecursiveGreenFactTag {
    M11RecursiveGreenFactTag::new(value).expect("fact registry values are nonzero")
}

fn property(tag: u16, bytes: &[u8]) -> Result<M11RecursiveGreenPropertyChunk, M11BlockWriterError> {
    Ok(M11RecursiveGreenPropertyChunk::new(fact_tag(tag), bytes)?)
}

fn close_facts(tag: u16, bytes: &[u8]) -> Result<M11RecursiveGreenCloseFacts, M11BlockWriterError> {
    Ok(M11RecursiveGreenCloseFacts::new(fact_tag(tag), bytes)?)
}

fn close_facts_with_cached_row(
    tag: u16,
    semantic: &[u8],
    cached: M11RecursiveGreenCachedRowEditable,
) -> Result<M11RecursiveGreenCloseFacts, M11BlockWriterError> {
    Ok(M11RecursiveGreenCloseFacts::new_with_cached_row_editable(
        fact_tag(tag),
        semantic,
        cached,
    )?)
}

fn open_property(
    kind: BlockKind,
) -> Result<Option<M11RecursiveGreenPropertyChunk>, M11BlockWriterError> {
    match kind {
        BlockKind::List(facts) => {
            let mut payload = [0_u8; 8];
            match facts.style() {
                ListStyle::Bullet { marker } => {
                    payload[0] = 1;
                    payload[1] = match marker {
                        BulletMarker::Hyphen => b'-',
                        BulletMarker::Plus => b'+',
                        BulletMarker::Asterisk => b'*',
                    };
                    payload[4..8].copy_from_slice(&1_u32.to_le_bytes());
                }
                ListStyle::Ordered { start, delimiter } => {
                    payload[0] = 2;
                    payload[2] = match delimiter {
                        ListDelimiter::Period => b'.',
                        ListDelimiter::Parenthesis => b')',
                    };
                    payload[4..8].copy_from_slice(&start.to_le_bytes());
                }
            }
            Ok(Some(property(FACT_LIST, &payload)?))
        }
        BlockKind::Item(facts) => {
            let mut payload = [0_u8; 4];
            payload[..2].copy_from_slice(&facts.marker_offset().to_le_bytes());
            payload[2..].copy_from_slice(&facts.padding().to_le_bytes());
            Ok(Some(property(FACT_ITEM, &payload)?))
        }
        BlockKind::Heading(facts) => Ok(Some(property(
            FACT_HEADING,
            &[
                facts.level(),
                match facts.style() {
                    HeadingStyle::Atx => 0,
                    HeadingStyle::Setext => 1,
                },
            ],
        )?)),
        BlockKind::FencedCode(facts) => {
            let mut payload = [0_u8; 10];
            payload[0] = match facts.fence() {
                FenceCharacter::Backtick => b'`',
                FenceCharacter::Tilde => b'~',
            };
            payload[1] = facts.fence_offset_columns();
            payload[2..].copy_from_slice(&facts.minimum_closing_length().to_le_bytes());
            Ok(Some(property(FACT_CODE, &payload)?))
        }
        BlockKind::HtmlBlock(facts) => Ok(Some(property(FACT_HTML, &[facts.block_type().get()])?)),
        BlockKind::Document
        | BlockKind::BlockQuote
        | BlockKind::Paragraph
        | BlockKind::IndentedCode
        | BlockKind::ThematicBreak => Ok(None),
    }
}

fn green_metric(
    metric: SourceMetric,
) -> Result<M11RecursiveGreenSourceMetric, M11BlockWriterError> {
    M11RecursiveGreenSourceMetric::new(metric.bytes(), metric.utf16()).ok_or(
        M11BlockWriterError::InvalidCommand("source metric is not a nonempty UTF-8 partition"),
    )
}

fn green_metric_allow_empty(
    metric: SourceMetric,
) -> Result<M11RecursiveGreenSourceMetric, M11BlockWriterError> {
    M11RecursiveGreenSourceMetric::new(metric.bytes(), metric.utf16()).ok_or(
        M11BlockWriterError::InvalidCommand("source metric axes differ in emptiness"),
    )
}

const fn is_renderable_block_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Paragraph
            | BlockKind::IndentedCode
            | BlockKind::FencedCode(_)
            | BlockKind::HtmlBlock(_)
            | BlockKind::Heading(_)
            | BlockKind::ThematicBreak
    )
}

const fn green_part(part: CoveragePart) -> M11RecursiveGreenCoveragePart {
    match part {
        CoveragePart::Content => M11RecursiveGreenCoveragePart::Content,
        CoveragePart::ContainerMarker => M11RecursiveGreenCoveragePart::ContainerMarker,
        CoveragePart::BlockMarker => M11RecursiveGreenCoveragePart::BlockMarker,
        CoveragePart::Gap => M11RecursiveGreenCoveragePart::Gap,
        CoveragePart::Terminal => M11RecursiveGreenCoveragePart::Terminal,
    }
}

fn green_logical_action(
    action: LogicalAction,
) -> Result<M11RecursiveGreenLogicalAction, M11BlockWriterError> {
    Ok(match action {
        LogicalAction::Identity => M11RecursiveGreenLogicalAction::Identity,
        LogicalAction::CanonicalText => M11RecursiveGreenLogicalAction::CanonicalText,
        LogicalAction::PartialTab(tab) => M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth: tab.logical_target().generations_from_top(),
            remaining_spaces: tab.remaining_spaces(),
        },
        LogicalAction::HiddenUpstream => M11RecursiveGreenLogicalAction::HiddenUpstream,
        LogicalAction::CanonicalNewline => M11RecursiveGreenLogicalAction::CanonicalNewline,
        LogicalAction::None => M11RecursiveGreenLogicalAction::None,
    })
}

fn metric_difference(
    later: SourceMetric,
    earlier: SourceMetric,
) -> Result<SourceMetric, M11BlockWriterError> {
    let bytes =
        later
            .bytes()
            .checked_sub(earlier.bytes())
            .ok_or(M11BlockWriterError::InvalidCommand(
                "logical byte metric moved backwards",
            ))?;
    let utf16 =
        later
            .utf16()
            .checked_sub(earlier.utf16())
            .ok_or(M11BlockWriterError::InvalidCommand(
                "logical UTF-16 metric moved backwards",
            ))?;
    SourceMetric::new(bytes, utf16).ok_or(M11BlockWriterError::InvalidCommand(
        "logical metric difference is invalid",
    ))
}

fn read_fragment_bytes(
    lease_slot: &mut Option<SourceSnapshotLease>,
    start: usize,
    end: usize,
) -> Result<Vec<u8>, M11BlockWriterError> {
    let len = end
        .checked_sub(start)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| M11BlockWriterError::Allocation)?;
    bytes.resize(len, 0);
    let lease = lease_slot.take().ok_or(M11BlockWriterError::Poisoned)?;
    let mut cursor = lease.cursor_in(start..end)?;
    if cursor.read(&mut bytes) != len {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment source cursor ended early",
        ));
    }
    *lease_slot = Some(cursor.finish()?);
    Ok(bytes)
}

const fn metric_precedes(earlier: SourceMetric, later: SourceMetric) -> bool {
    earlier.bytes() <= later.bytes() && earlier.utf16() <= later.utf16()
}

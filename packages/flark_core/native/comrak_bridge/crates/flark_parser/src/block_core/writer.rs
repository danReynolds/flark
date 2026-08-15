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
    splice_m11_recursive_green_structural_with_spanning_exit_repairs_atomic,
    M11RecursiveGreenBuild, M11RecursiveGreenBuildStatus, M11RecursiveGreenCachedRowEditCapability,
    M11RecursiveGreenCachedRowEditable, M11RecursiveGreenCloseFacts, M11RecursiveGreenClosedChild,
    M11RecursiveGreenCoveragePart, M11RecursiveGreenError, M11RecursiveGreenEvent,
    M11RecursiveGreenFactTag, M11RecursiveGreenFrameId, M11RecursiveGreenKind,
    M11RecursiveGreenLogicalAction, M11RecursiveGreenLogicalPosition,
    M11RecursiveGreenLogicalRange, M11RecursiveGreenPropertyChunk, M11RecursiveGreenReclaimPoll,
    M11RecursiveGreenRoot, M11RecursiveGreenSourceMetric, M11RecursiveGreenSpanningExitRepair,
    M11RecursiveGreenStructuralBoundary, M11RecursiveGreenStructuralBoundaryTransactionReplica,
    M11RecursiveGreenStructuralSpliceRebase, M11RecursiveGreenStructuralSpliceReceipt,
    M11RecursiveGreenStructuralSpliceSelection, M11RecursiveGreenTerminalFragmentBarrierStatus,
    M11RecursiveGreenTerminalFragmentBinding, M11RecursiveGreenTerminalFragmentCursor,
    M11RecursiveGreenTerminalFragmentCursorPoll, M11RecursiveGreenTerminalFragmentCursorStatus,
    M11RecursiveGreenTerminalFragmentDisposition, M11RecursiveGreenTerminalFragmentIdentity,
    M11RecursiveGreenTerminalFragmentRange, M11RecursiveGreenTerminalFragmentRewrite,
    M11RecursiveGreenTerminalFragmentRewritePoll, M11RecursiveGreenTerminalFragmentRewriteWork,
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

pub(crate) const KIND_DOCUMENT: u16 = 1;
pub(crate) const KIND_BLOCK_QUOTE: u16 = 2;
pub(super) const KIND_LIST: u16 = 3;
pub(super) const KIND_ITEM: u16 = 4;
pub(crate) const KIND_PARAGRAPH: u16 = 5;
pub(super) const KIND_INDENTED_CODE: u16 = 6;
pub(super) const KIND_FENCED_CODE: u16 = 7;
const KIND_HTML_BLOCK: u16 = 8;
pub(super) const KIND_HEADING: u16 = 12;
pub(super) const KIND_THEMATIC_BREAK: u16 = 13;
pub(super) const KIND_EMPTY_ITEM_ROW: u16 = 14;
pub(super) const KIND_EMPTY_BLOCK_QUOTE_ROW: u16 = 15;

pub(super) const FACT_LIST: u16 = 1;
pub(super) const FACT_ITEM: u16 = 2;
pub(super) const FACT_HEADING: u16 = 3;
pub(super) const FACT_CODE: u16 = 4;
const FACT_HTML: u16 = 5;
const FACT_ROW_EDITABLE: u16 = 6;

static RESTART_JOIN_IDS: AtomicU64 = AtomicU64::new(1);
static ADOPTION_TRANSACTION_IDS: AtomicU64 = AtomicU64::new(1);
static REFERENCE_FRAGMENT_IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum M11BlockWriterError {
    ZeroFuel,
    CommandPending,
    InvalidCommand(&'static str),
    CounterOverflow,
    Allocation,
    Poisoned,
    /// A restarted writer can only project a Paragraph whose canonical Enter
    /// belongs to the local fragment.  Callers must abandon incremental
    /// adoption and retry from a clean build when this typed condition occurs.
    ReferenceParagraphPredatesRestart,
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
            Self::ReferenceParagraphPredatesRestart => {
                formatter.write_str("reference Paragraph predates the incremental writer restart")
            }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FenceFold {
    logical_base: SourceMetric,
    info_end: Option<SourceMetric>,
    literal_start: Option<SourceMetric>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    has_unrepresented_container_marker: bool,
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
    reference: Option<M11FragmentReferenceState>,
}

/// Source identity shared with the donor while a terminal Paragraph
/// projection is frozen.  The fragment arm is intentionally opaque: unlike a
/// Green identity it certifies the writer-local high-level event journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum M11ReferenceOutputIdentity {
    Document(M11RecursiveGreenTerminalFragmentIdentity),
    Fragment(u64),
}

pub(super) enum M11ReferenceOutputBinding {
    Document(M11RecursiveGreenTerminalFragmentBinding),
    Fragment(M11FragmentReferenceBinding),
}

impl M11ReferenceOutputBinding {
    pub(super) const fn identity(&self) -> M11ReferenceOutputIdentity {
        match self {
            Self::Document(binding) => M11ReferenceOutputIdentity::Document(binding.identity()),
            Self::Fragment(binding) => M11ReferenceOutputIdentity::Fragment(binding.generation),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct M11FragmentReferenceBinding {
    generation: u64,
    frame: M11RecursiveGreenFrameId,
    enter_event: usize,
    events_end: usize,
    physical_before: SourceMetric,
    physical_end: SourceMetric,
    base_receipt: WriterOutputReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11FragmentReferencePhase {
    Barrier,
    Frozen,
    Rewriting,
}

#[derive(Clone, Copy, Debug)]
struct M11FragmentReferenceState {
    binding: M11FragmentReferenceBinding,
    phase: M11FragmentReferencePhase,
}

pub(super) enum M11ReferenceOutputCursor {
    Document(M11RecursiveGreenTerminalFragmentCursor),
    Fragment(M11FragmentReferenceCursor),
}

pub(super) enum M11ReferenceOutputRange {
    Document(M11RecursiveGreenTerminalFragmentRange),
    Fragment(M11FragmentReferenceRange),
}

impl M11ReferenceOutputRange {
    pub(super) fn physical_range(&self) -> Option<(std::ops::Range<u64>, std::ops::Range<u64>)> {
        match self {
            Self::Document(range) => range
                .physical_range()
                .map(|physical| (physical.byte_range(), physical.utf16_range())),
            Self::Fragment(range) if range.replay_validated => range.physical.clone(),
            Self::Fragment(_) => None,
        }
    }
}

#[derive(Debug)]
struct M11FragmentReferenceRange {
    generation: u64,
    logical: M11RecursiveGreenLogicalRange,
    physical: Option<(std::ops::Range<u64>, std::ops::Range<u64>)>,
    replay_validated: bool,
}

pub(super) enum M11ReferenceOutputRewrite {
    Unchanged,
    RemoveWrapper {
        whole_fragment: M11ReferenceOutputRange,
    },
    RetainVisibleSuffix {
        removed_prefix: M11ReferenceOutputRange,
    },
}

pub(super) enum M11ReferenceOutputRewriteWork {
    Document(M11RecursiveGreenTerminalFragmentRewriteWork),
    Fragment(M11FragmentReferenceRewriteWork),
}

pub(super) struct M11ReferenceOutputRewriteAuthority {
    frame: M11RecursiveGreenFrameId,
    disposition: M11RecursiveGreenTerminalFragmentDisposition,
    visible_remainder_boundary: Option<M11RecursiveGreenStructuralBoundary>,
    visible_remainder_physical: Option<SourceMetric>,
}

impl M11ReferenceOutputRewriteAuthority {
    pub(super) const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.frame
    }

    pub(super) const fn disposition(&self) -> M11RecursiveGreenTerminalFragmentDisposition {
        self.disposition
    }

    pub(super) fn take_visible_remainder_boundary(
        &mut self,
    ) -> Option<M11RecursiveGreenStructuralBoundary> {
        self.visible_remainder_boundary.take()
    }

    pub(super) const fn visible_remainder_physical(&self) -> Option<SourceMetric> {
        self.visible_remainder_physical
    }
}

pub(super) enum M11ReferenceOutputRewritePoll {
    Pending,
    Complete(M11ReferenceOutputRewriteAuthority),
}

#[derive(Debug)]
enum M11FragmentProjectedAtom {
    Source {
        end: SourceMetric,
        canonical_text: bool,
    },
    Static {
        bytes: [u8; 3],
        len: u8,
        next: u8,
        logical_utf16: u8,
        raw_utf16_at_end: u8,
        physical: (SourceMetric, SourceMetric),
    },
}

#[derive(Debug)]
pub(super) struct M11FragmentReferenceCursor {
    binding: M11FragmentReferenceBinding,
    next_event: usize,
    physical_position: SourceMetric,
    logical_bytes: u64,
    logical_utf16: u64,
    atom: Option<M11FragmentProjectedAtom>,
    ready_bytes: Vec<u8>,
    ready_raw_contributions: Vec<u8>,
    ready_start: usize,
    ready_base_offset: u64,
    yielded_bytes: u64,
    last_raw_contribution: Option<(u64, u8)>,
    yield_bytes: std::ops::Range<u64>,
    expected_yield_utf16: Option<std::ops::Range<u64>>,
    range_authority: Option<M11FragmentReferenceRange>,
    yielded_physical: Option<(SourceMetric, SourceMetric)>,
    complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11FragmentReferenceRewriteMode {
    Unchanged,
    Remove { cut: SourceMetric },
    Retain { cut: SourceMetric },
}

#[derive(Debug)]
pub(super) struct M11FragmentReferenceRewriteWork {
    binding: M11FragmentReferenceBinding,
    mode: M11FragmentReferenceRewriteMode,
    next_event: usize,
    physical_position: SourceMetric,
    replacement: Vec<M11RecursiveGreenEvent>,
    visible_remainder: Option<SourceMetric>,
}

impl M11ReferenceOutputCursor {
    pub(super) fn available_len(&self) -> u64 {
        match self {
            Self::Document(cursor) => cursor.available_len(),
            Self::Fragment(cursor) => cursor.yielded_bytes,
        }
    }

    pub(super) fn is_final(&self) -> bool {
        match self {
            Self::Document(cursor) => cursor.is_final(),
            Self::Fragment(cursor) => cursor.complete,
        }
    }

    pub(super) fn ready_chunk(&self) -> &[u8] {
        match self {
            Self::Document(cursor) => cursor.ready_chunk(),
            Self::Fragment(cursor) => &cursor.ready_bytes[cursor.ready_start..],
        }
    }

    pub(super) fn ready_byte(&self) -> Option<(u64, u8)> {
        match self {
            Self::Document(cursor) => cursor
                .ready_byte()
                .map(|ready| (ready.relative_offset(), ready.byte())),
            Self::Fragment(cursor) => {
                let index = cursor.ready_start;
                Some((
                    cursor
                        .ready_base_offset
                        .checked_add(u64::try_from(index).ok()?)?,
                    *cursor.ready_bytes.get(index)?,
                ))
            }
        }
    }

    pub(super) fn read_byte(&mut self, relative_offset: u64) -> Result<u8, M11BlockWriterError> {
        match self {
            Self::Document(cursor) => Ok(cursor.read_byte(relative_offset)?),
            Self::Fragment(cursor) => {
                let index = cursor.ready_start;
                let expected = cursor
                    .ready_base_offset
                    .checked_add(
                        u64::try_from(index).map_err(|_| M11BlockWriterError::CounterOverflow)?,
                    )
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                if expected != relative_offset {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment reference cursor read is not sequential",
                    ));
                }
                let byte =
                    *cursor
                        .ready_bytes
                        .get(index)
                        .ok_or(M11BlockWriterError::InvalidCommand(
                            "fragment reference cursor has no ready byte",
                        ))?;
                consume_fragment_ready_prefix(cursor, 1)?;
                Ok(byte)
            }
        }
    }

    pub(super) fn consume_ready_prefix(&mut self, len: usize) -> Result<(), M11BlockWriterError> {
        match self {
            Self::Document(cursor) => Ok(cursor.consume_ready_prefix(len)?),
            Self::Fragment(cursor) => consume_fragment_ready_prefix(cursor, len),
        }
    }

    pub(super) fn raw_codepoint_contribution(&self, relative_offset: u64) -> u8 {
        match self {
            Self::Document(cursor) => cursor.raw_codepoint_contribution(relative_offset),
            Self::Fragment(cursor) => cursor
                .last_raw_contribution
                .filter(|(offset, _)| *offset == relative_offset)
                .map_or(0, |(_, contribution)| contribution),
        }
    }

    pub(super) fn logical_position(&self) -> M11RecursiveGreenLogicalPosition {
        match self {
            Self::Document(cursor) => cursor.logical_position(),
            Self::Fragment(cursor) => {
                M11RecursiveGreenLogicalPosition::new(cursor.logical_bytes, cursor.logical_utf16)
                    .expect("fragment cursor logical metrics are valid")
            }
        }
    }

    pub(super) fn take_completed_range(
        &mut self,
    ) -> Result<M11ReferenceOutputRange, M11BlockWriterError> {
        match self {
            Self::Document(cursor) => Ok(M11ReferenceOutputRange::Document(
                cursor.take_completed_range()?,
            )),
            Self::Fragment(cursor) => {
                if !cursor.complete || !cursor.ready_bytes[cursor.ready_start..].is_empty() {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment reference range replay is incomplete",
                    ));
                }
                let mut range =
                    cursor
                        .range_authority
                        .take()
                        .ok_or(M11BlockWriterError::InvalidCommand(
                            "fragment reference cursor has no range authority",
                        ))?;
                range.physical = cursor
                    .yielded_physical
                    .map(|(start, end)| (start.bytes()..end.bytes(), start.utf16()..end.utf16()));
                range.replay_validated = true;
                Ok(M11ReferenceOutputRange::Fragment(range))
            }
        }
    }
}

fn consume_fragment_ready_prefix(
    cursor: &mut M11FragmentReferenceCursor,
    len: usize,
) -> Result<(), M11BlockWriterError> {
    let ready_len = cursor.ready_bytes.len().saturating_sub(cursor.ready_start);
    if len == 0 || len > ready_len {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference cursor consumed outside its ready chunk",
        ));
    }
    let last = cursor
        .ready_start
        .checked_add(len - 1)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let offset = cursor
        .ready_base_offset
        .checked_add(u64::try_from(last).map_err(|_| M11BlockWriterError::CounterOverflow)?)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let contribution =
        *cursor
            .ready_raw_contributions
            .get(last)
            .ok_or(M11BlockWriterError::InvalidCommand(
                "fragment reference cursor lost its raw contribution",
            ))?;
    cursor.last_raw_contribution = Some((offset, contribution));
    cursor.ready_start = cursor
        .ready_start
        .checked_add(len)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    if cursor.ready_start == cursor.ready_bytes.len() {
        cursor.ready_bytes.clear();
        cursor.ready_raw_contributions.clear();
        cursor.ready_start = 0;
        cursor.ready_base_offset = cursor.yielded_bytes;
    }
    Ok(())
}

impl M11FragmentReferenceCursor {
    fn new(
        binding: M11FragmentReferenceBinding,
        range: Option<M11FragmentReferenceRange>,
    ) -> Result<Self, M11BlockWriterError> {
        let (yield_bytes, expected_yield_utf16) = range.as_ref().map_or_else(
            || (0..u64::MAX, None),
            |range| {
                (
                    range.logical.byte_range(),
                    Some(range.logical.utf16_range()),
                )
            },
        );
        let empty_at_origin = yield_bytes.start == 0
            && yield_bytes.end == 0
            && expected_yield_utf16
                .as_ref()
                .is_none_or(|utf16| utf16.start == 0 && utf16.end == 0);
        let mut ready_bytes = Vec::new();
        ready_bytes
            .try_reserve_exact(flark_engine::SOURCE_CURSOR_WINDOW_BYTES)
            .map_err(|_| M11BlockWriterError::Allocation)?;
        let mut ready_raw_contributions = Vec::new();
        ready_raw_contributions
            .try_reserve_exact(flark_engine::SOURCE_CURSOR_WINDOW_BYTES)
            .map_err(|_| M11BlockWriterError::Allocation)?;
        Ok(Self {
            binding,
            next_event: binding
                .enter_event
                .checked_add(1)
                .ok_or(M11BlockWriterError::CounterOverflow)?,
            physical_position: binding.physical_before,
            logical_bytes: 0,
            logical_utf16: 0,
            atom: None,
            ready_bytes,
            ready_raw_contributions,
            ready_start: 0,
            ready_base_offset: 0,
            yielded_bytes: 0,
            last_raw_contribution: None,
            yield_bytes,
            expected_yield_utf16,
            range_authority: range,
            yielded_physical: empty_at_origin
                .then_some((binding.physical_before, binding.physical_before)),
            complete: empty_at_origin,
        })
    }

    fn retarget_forward(
        &mut self,
        range: M11FragmentReferenceRange,
    ) -> Result<(), M11BlockWriterError> {
        let bytes = range.logical.byte_range();
        let utf16 = range.logical.utf16_range();
        let empty_at_cursor = bytes.start == bytes.end
            && utf16.start == utf16.end
            && bytes.start == self.logical_bytes
            && utf16.start == self.logical_utf16;
        if range.generation != self.binding.generation
            || !self.complete
            || !self.ready_bytes[self.ready_start..].is_empty()
            || self.range_authority.is_some()
            || bytes.start < self.logical_bytes
            || utf16.start < self.logical_utf16
        {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment reference range replay cannot move backwards",
            ));
        }
        self.yield_bytes = bytes;
        self.expected_yield_utf16 = Some(utf16);
        self.range_authority = Some(range);
        self.yielded_bytes = 0;
        self.yielded_physical =
            empty_at_cursor.then_some((self.physical_position, self.physical_position));
        self.last_raw_contribution = None;
        self.ready_base_offset = 0;
        self.complete = empty_at_cursor;
        Ok(())
    }
}

fn validate_fragment_reference_binding(
    fragment: &M11BlockFragmentOutput,
    binding: &M11FragmentReferenceBinding,
) -> Result<(), M11BlockWriterError> {
    let state = fragment
        .reference
        .as_ref()
        .ok_or(M11BlockWriterError::InvalidCommand(
            "fragment reference projection is not active",
        ))?;
    if state.phase != M11FragmentReferencePhase::Frozen
        || state.binding.generation != binding.generation
        || state.binding.frame != binding.frame
        || binding.events_end != fragment.events.len()
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference binding crossed its frozen event journal",
        ));
    }
    Ok(())
}

fn poll_fragment_reference_cursor(
    fragment: &mut M11BlockFragmentOutput,
    cursor: &mut M11FragmentReferenceCursor,
    fuel: usize,
    chunked: bool,
) -> Result<M11RecursiveGreenTerminalFragmentCursorStatus, M11BlockWriterError> {
    if fuel == 0 {
        return Err(M11BlockWriterError::ZeroFuel);
    }
    let state = fragment
        .reference
        .as_ref()
        .ok_or(M11BlockWriterError::InvalidCommand(
            "fragment reference projection is not active",
        ))?;
    if state.binding.generation != cursor.binding.generation
        || state.phase != M11FragmentReferencePhase::Frozen
        || cursor.binding.events_end != fragment.events.len()
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference cursor crossed its frozen event journal",
        ));
    }
    if !cursor.ready_bytes[cursor.ready_start..].is_empty() {
        return Ok(M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady);
    }
    if cursor.complete {
        return Ok(M11RecursiveGreenTerminalFragmentCursorStatus::Complete);
    }

    let maximum_steps = if chunked {
        fuel.checked_mul(flark_engine::SOURCE_CURSOR_WINDOW_BYTES)
            .ok_or(M11BlockWriterError::CounterOverflow)?
    } else {
        fuel
    };
    let mut steps = 0;
    while steps < maximum_steps && !cursor.complete {
        let ready_len = cursor.ready_bytes.len().saturating_sub(cursor.ready_start);
        if ready_len > flark_engine::SOURCE_CURSOR_WINDOW_BYTES.saturating_sub(4) {
            break;
        }
        step_fragment_reference_cursor(fragment, cursor)?;
        steps += 1;
        if !chunked && !cursor.ready_bytes[cursor.ready_start..].is_empty() {
            break;
        }
    }
    if !cursor.ready_bytes[cursor.ready_start..].is_empty() {
        Ok(M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady)
    } else if cursor.complete {
        Ok(M11RecursiveGreenTerminalFragmentCursorStatus::Complete)
    } else {
        Ok(M11RecursiveGreenTerminalFragmentCursorStatus::Pending)
    }
}

fn step_fragment_reference_cursor(
    fragment: &mut M11BlockFragmentOutput,
    cursor: &mut M11FragmentReferenceCursor,
) -> Result<(), M11BlockWriterError> {
    if let Some(mut atom) = cursor.atom.take() {
        match &mut atom {
            M11FragmentProjectedAtom::Source {
                end,
                canonical_text,
            } => {
                if cursor.physical_position == *end {
                    return Ok(());
                }
                let scalar_start = cursor.physical_position;
                let (raw, raw_utf16) =
                    read_fragment_scalar(fragment, scalar_start.bytes(), end.bytes())?;
                let raw_bytes =
                    u64::try_from(raw.len()).map_err(|_| M11BlockWriterError::CounterOverflow)?;
                let scalar_end = scalar_start
                    .checked_add(
                        SourceMetric::new(raw_bytes, u64::from(raw_utf16))
                            .ok_or(M11BlockWriterError::CounterOverflow)?,
                    )
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                if !metric_precedes(scalar_end, *end) {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment projection scalar crossed its coverage",
                    ));
                }
                let projected: &[u8] = if *canonical_text && raw == [0] {
                    b"\xef\xbf\xbd"
                } else {
                    &raw
                };
                emit_fragment_projected_scalar(
                    cursor,
                    projected,
                    u64::from(raw_utf16),
                    raw_utf16,
                    scalar_start,
                    scalar_end,
                )?;
                cursor.physical_position = scalar_end;
                if scalar_end != *end {
                    cursor.atom = Some(atom);
                }
            }
            M11FragmentProjectedAtom::Static {
                bytes,
                len,
                next,
                logical_utf16,
                raw_utf16_at_end,
                physical,
            } => {
                let index = usize::from(*next);
                if index >= usize::from(*len) {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment static projection advanced past its end",
                    ));
                }
                let last = index + 1 == usize::from(*len);
                emit_fragment_projected_scalar(
                    cursor,
                    &bytes[index..index + 1],
                    u64::from(*logical_utf16),
                    if last { *raw_utf16_at_end } else { 0 },
                    physical.0,
                    physical.1,
                )?;
                *next += 1;
                if last {
                    cursor.physical_position = physical.1;
                } else {
                    cursor.atom = Some(atom);
                }
            }
        }
        return Ok(());
    }

    if cursor.next_event == cursor.binding.events_end {
        if cursor.physical_position != cursor.binding.physical_end {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment reference cursor changed its physical boundary",
            ));
        }
        if cursor.yield_bytes.end != u64::MAX {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment reference range escaped the terminal Paragraph",
            ));
        }
        cursor.complete = true;
        return Ok(());
    }
    let event =
        *fragment
            .events
            .get(cursor.next_event)
            .ok_or(M11BlockWriterError::InvalidCommand(
                "fragment reference event journal ended early",
            ))?;
    cursor.next_event = cursor
        .next_event
        .checked_add(1)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let M11RecursiveGreenEvent::Coverage {
        physical,
        owner_depth,
        logical,
        ..
    } = event
    else {
        if matches!(
            event,
            M11RecursiveGreenEvent::Enter { .. } | M11RecursiveGreenEvent::Exit { .. }
        ) {
            return Err(M11BlockWriterError::InvalidCommand(
                "local reference Paragraph contains a nested structural event",
            ));
        }
        return Ok(());
    };
    let physical = SourceMetric::new(physical.bytes(), physical.utf16())
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let start = cursor.physical_position;
    let end = start
        .checked_add(physical)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    if !metric_precedes(end, cursor.binding.physical_end) {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference coverage crossed its frozen boundary",
        ));
    }
    let targets_paragraph = match logical {
        M11RecursiveGreenLogicalAction::None | M11RecursiveGreenLogicalAction::HiddenUpstream => {
            false
        }
        M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth, ..
        } => target_owner_depth == 0,
        M11RecursiveGreenLogicalAction::Identity
        | M11RecursiveGreenLogicalAction::CanonicalText
        | M11RecursiveGreenLogicalAction::CanonicalNewline => owner_depth == 0,
    };
    if !targets_paragraph {
        cursor.physical_position = end;
        return Ok(());
    }
    cursor.atom = Some(match logical {
        M11RecursiveGreenLogicalAction::Identity => M11FragmentProjectedAtom::Source {
            end,
            canonical_text: false,
        },
        M11RecursiveGreenLogicalAction::CanonicalText => M11FragmentProjectedAtom::Source {
            end,
            canonical_text: true,
        },
        M11RecursiveGreenLogicalAction::CanonicalNewline => {
            let raw = read_fragment_bytes(
                &mut fragment.lease,
                usize::try_from(start.bytes()).map_err(|_| M11BlockWriterError::CounterOverflow)?,
                usize::try_from(end.bytes()).map_err(|_| M11BlockWriterError::CounterOverflow)?,
            )?;
            fragment.source_bytes_read = fragment
                .source_bytes_read
                .checked_add(u64::try_from(raw.len()).unwrap_or(u64::MAX))
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            if !matches!(raw.as_slice(), b"\n" | b"\r" | b"\r\n") {
                return Err(M11BlockWriterError::InvalidCommand(
                    "fragment canonical newline differs from target source",
                ));
            }
            M11FragmentProjectedAtom::Static {
                bytes: [b'\n', 0, 0],
                len: 1,
                next: 0,
                logical_utf16: 1,
                raw_utf16_at_end: u8::try_from(physical.utf16())
                    .map_err(|_| M11BlockWriterError::CounterOverflow)?,
                physical: (start, end),
            }
        }
        M11RecursiveGreenLogicalAction::PartialTab {
            remaining_spaces, ..
        } => {
            let raw = read_fragment_bytes(
                &mut fragment.lease,
                usize::try_from(start.bytes()).map_err(|_| M11BlockWriterError::CounterOverflow)?,
                usize::try_from(end.bytes()).map_err(|_| M11BlockWriterError::CounterOverflow)?,
            )?;
            fragment.source_bytes_read = fragment
                .source_bytes_read
                .checked_add(u64::try_from(raw.len()).unwrap_or(u64::MAX))
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            if raw != [b'\t'] || !(1..=3).contains(&remaining_spaces) {
                return Err(M11BlockWriterError::InvalidCommand(
                    "fragment partial tab differs from target source",
                ));
            }
            M11FragmentProjectedAtom::Static {
                bytes: [b' '; 3],
                len: remaining_spaces,
                next: 0,
                logical_utf16: 1,
                raw_utf16_at_end: 1,
                physical: (start, end),
            }
        }
        M11RecursiveGreenLogicalAction::None | M11RecursiveGreenLogicalAction::HiddenUpstream => {
            unreachable!()
        }
    });
    Ok(())
}

fn emit_fragment_projected_scalar(
    cursor: &mut M11FragmentReferenceCursor,
    projected: &[u8],
    projected_utf16: u64,
    raw_utf16_contribution: u8,
    physical_start: SourceMetric,
    physical_end: SourceMetric,
) -> Result<(), M11BlockWriterError> {
    let projected_len =
        u64::try_from(projected.len()).map_err(|_| M11BlockWriterError::CounterOverflow)?;
    let logical_start = cursor.logical_bytes;
    let logical_end = logical_start
        .checked_add(projected_len)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let utf16_start = cursor.logical_utf16;
    let utf16_end = utf16_start
        .checked_add(projected_utf16)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    if (cursor.yield_bytes.start > logical_start && cursor.yield_bytes.start < logical_end)
        || (cursor.yield_bytes.end > logical_start && cursor.yield_bytes.end < logical_end)
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference range splits a projected scalar",
        ));
    }
    if let Some(expected) = &cursor.expected_yield_utf16 {
        if cursor.yield_bytes.start == logical_start && expected.start != utf16_start
            || cursor.yield_bytes.end == logical_end && expected.end != utf16_end
        {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment reference range UTF-16 endpoint is not exact",
            ));
        }
    }
    if logical_start >= cursor.yield_bytes.start && logical_end <= cursor.yield_bytes.end {
        cursor
            .ready_bytes
            .try_reserve(projected.len())
            .map_err(|_| M11BlockWriterError::Allocation)?;
        cursor
            .ready_raw_contributions
            .try_reserve(projected.len())
            .map_err(|_| M11BlockWriterError::Allocation)?;
        cursor.ready_bytes.extend_from_slice(projected);
        cursor
            .ready_raw_contributions
            .extend(std::iter::repeat_n(0, projected.len()));
        if let Some(last) = cursor.ready_raw_contributions.last_mut() {
            *last = raw_utf16_contribution;
        }
        cursor.yielded_bytes = cursor
            .yielded_bytes
            .checked_add(projected_len)
            .ok_or(M11BlockWriterError::CounterOverflow)?;
        match &mut cursor.yielded_physical {
            Some((_, end)) => *end = physical_end,
            None => cursor.yielded_physical = Some((physical_start, physical_end)),
        }
    }
    cursor.logical_bytes = logical_end;
    cursor.logical_utf16 = utf16_end;
    if logical_end == cursor.yield_bytes.end {
        cursor.complete = true;
    } else if logical_end > cursor.yield_bytes.end {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference range endpoint was skipped",
        ));
    }
    Ok(())
}

fn read_fragment_scalar(
    fragment: &mut M11BlockFragmentOutput,
    start: u64,
    end: u64,
) -> Result<(Vec<u8>, u8), M11BlockWriterError> {
    let remaining = end
        .checked_sub(start)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    if remaining == 0 {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment projection requested an empty scalar",
        ));
    }
    let start = usize::try_from(start).map_err(|_| M11BlockWriterError::CounterOverflow)?;
    let end = usize::try_from(end).map_err(|_| M11BlockWriterError::CounterOverflow)?;
    // SourceCursor ranges must be scalar aligned. A blind four-byte probe can
    // end inside a later multibyte scalar even though the coverage envelope is
    // exact (for example `[ΑΓΩ]`). Resolve the next scalar boundary through
    // the source's byte/UTF-16 authority before opening the bounded cursor.
    let lease = fragment
        .lease
        .as_ref()
        .ok_or(M11BlockWriterError::Poisoned)?;
    let utf16_start = lease.utf16_offset_for_byte(start)?;
    let next_utf16 = utf16_start
        .checked_add(1)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let scalar_end = match lease.byte_offset_for_utf16(next_utf16) {
        Ok(offset) => offset,
        Err(SourceEditError::SplitUtf16Scalar { .. }) => lease.byte_offset_for_utf16(
            next_utf16
                .checked_add(1)
                .ok_or(M11BlockWriterError::CounterOverflow)?,
        )?,
        Err(error) => return Err(error.into()),
    };
    if scalar_end > end {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment projection scalar crossed its coverage",
        ));
    }
    let probe = read_fragment_bytes(&mut fragment.lease, start, scalar_end)?;
    fragment.source_bytes_read = fragment
        .source_bytes_read
        .checked_add(u64::try_from(probe.len()).unwrap_or(u64::MAX))
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    let width = match probe[0] {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment projection source is not UTF-8",
            ));
        }
    };
    if width > probe.len() {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment projection scalar crossed its coverage",
        ));
    }
    let raw = probe[..width].to_vec();
    let scalar = std::str::from_utf8(&raw)
        .ok()
        .and_then(|text| text.chars().next())
        .ok_or(M11BlockWriterError::InvalidCommand(
            "fragment projection source is not UTF-8",
        ))?;
    Ok((
        raw,
        u8::try_from(scalar.len_utf16()).expect("one scalar uses at most two UTF-16 code units"),
    ))
}

fn begin_fragment_reference_rewrite(
    fragment: &mut M11BlockFragmentOutput,
    binding: M11FragmentReferenceBinding,
    rewrite: M11ReferenceOutputRewrite,
) -> Result<M11FragmentReferenceRewriteWork, M11BlockWriterError> {
    validate_fragment_reference_binding(fragment, &binding)?;
    let (mode, visible_remainder) = match rewrite {
        M11ReferenceOutputRewrite::Unchanged => (M11FragmentReferenceRewriteMode::Unchanged, None),
        M11ReferenceOutputRewrite::RemoveWrapper {
            whole_fragment: M11ReferenceOutputRange::Fragment(range),
        } => {
            let cut = validate_fragment_rewrite_range(&binding, &range, true)?;
            (M11FragmentReferenceRewriteMode::Remove { cut }, None)
        }
        M11ReferenceOutputRewrite::RetainVisibleSuffix {
            removed_prefix: M11ReferenceOutputRange::Fragment(range),
        } => {
            let cut = validate_fragment_rewrite_range(&binding, &range, false)?;
            (M11FragmentReferenceRewriteMode::Retain { cut }, Some(cut))
        }
        _ => {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment reference rewrite received a Green range",
            ));
        }
    };
    let mut replacement = Vec::new();
    replacement
        .try_reserve(fragment.events.len().saturating_add(1))
        .map_err(|_| M11BlockWriterError::Allocation)?;
    replacement.extend_from_slice(&fragment.events[..binding.enter_event]);
    fragment
        .reference
        .as_mut()
        .ok_or(M11BlockWriterError::InvalidCommand(
            "fragment reference rewrite lost its binding",
        ))?
        .phase = M11FragmentReferencePhase::Rewriting;
    Ok(M11FragmentReferenceRewriteWork {
        binding,
        mode,
        next_event: binding.enter_event,
        physical_position: binding.physical_before,
        replacement,
        visible_remainder,
    })
}

fn validate_fragment_rewrite_range(
    binding: &M11FragmentReferenceBinding,
    range: &M11FragmentReferenceRange,
    require_whole_fragment: bool,
) -> Result<SourceMetric, M11BlockWriterError> {
    let logical_bytes = range.logical.byte_range();
    let logical_utf16 = range.logical.utf16_range();
    let (physical_bytes, physical_utf16) =
        range
            .physical
            .clone()
            .ok_or(M11BlockWriterError::InvalidCommand(
                "fragment reference rewrite range has no physical envelope",
            ))?;
    if range.generation != binding.generation
        || !range.replay_validated
        || logical_bytes.start != 0
        || logical_utf16.start != 0
        || physical_bytes.start != binding.physical_before.bytes()
        || physical_utf16.start != binding.physical_before.utf16()
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference rewrite range crossed its binding",
        ));
    }
    let cut = SourceMetric::new(physical_bytes.end, physical_utf16.end).ok_or(
        M11BlockWriterError::InvalidCommand(
            "fragment reference rewrite has an invalid physical cut",
        ),
    )?;
    if !metric_precedes(binding.physical_before, cut)
        || !metric_precedes(cut, binding.physical_end)
        || require_whole_fragment && cut != binding.physical_end
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference rewrite escaped the terminal Paragraph",
        ));
    }
    Ok(cut)
}

fn poll_fragment_reference_rewrite(
    fragment: &mut M11BlockFragmentOutput,
    work: &mut M11FragmentReferenceRewriteWork,
    fuel: usize,
) -> Result<M11ReferenceOutputRewritePoll, M11BlockWriterError> {
    if fuel == 0 {
        return Err(M11BlockWriterError::ZeroFuel);
    }
    let state = fragment
        .reference
        .as_ref()
        .ok_or(M11BlockWriterError::InvalidCommand(
            "fragment reference rewrite is not active",
        ))?;
    if state.phase != M11FragmentReferencePhase::Rewriting
        || state.binding.generation != work.binding.generation
        || work.binding.events_end != fragment.events.len()
    {
        return Err(M11BlockWriterError::InvalidCommand(
            "fragment reference rewrite crossed its frozen event journal",
        ));
    }
    let mut transitions = 0;
    while transitions < fuel {
        if work.mode == M11FragmentReferenceRewriteMode::Unchanged {
            fragment.reference = None;
            return Ok(M11ReferenceOutputRewritePoll::Complete(
                M11ReferenceOutputRewriteAuthority {
                    frame: work.binding.frame,
                    disposition: M11RecursiveGreenTerminalFragmentDisposition::Surviving,
                    visible_remainder_boundary: None,
                    visible_remainder_physical: None,
                },
            ));
        }
        if work.next_event == work.binding.events_end {
            if work.physical_position != work.binding.physical_end {
                return Err(M11BlockWriterError::InvalidCommand(
                    "fragment reference rewrite changed physical coverage",
                ));
            }
            let replacement = std::mem::take(&mut work.replacement);
            rebuild_fragment_event_journal(fragment, replacement, work.binding.base_receipt)?;
            fragment.reference = None;
            let removed = matches!(work.mode, M11FragmentReferenceRewriteMode::Remove { .. });
            return Ok(M11ReferenceOutputRewritePoll::Complete(
                M11ReferenceOutputRewriteAuthority {
                    frame: work.binding.frame,
                    disposition: if removed {
                        M11RecursiveGreenTerminalFragmentDisposition::Removed
                    } else {
                        M11RecursiveGreenTerminalFragmentDisposition::Surviving
                    },
                    visible_remainder_boundary: None,
                    visible_remainder_physical: work.visible_remainder,
                },
            ));
        }
        transform_fragment_reference_event(fragment, work)?;
        transitions += 1;
    }
    Ok(M11ReferenceOutputRewritePoll::Pending)
}

fn transform_fragment_reference_event(
    fragment: &M11BlockFragmentOutput,
    work: &mut M11FragmentReferenceRewriteWork,
) -> Result<(), M11BlockWriterError> {
    let event =
        *fragment
            .events
            .get(work.next_event)
            .ok_or(M11BlockWriterError::InvalidCommand(
                "fragment reference rewrite event disappeared",
            ))?;
    let ordinal = work.next_event;
    work.next_event = work
        .next_event
        .checked_add(1)
        .ok_or(M11BlockWriterError::CounterOverflow)?;
    if ordinal == work.binding.enter_event {
        if !matches!(
            event,
            M11RecursiveGreenEvent::Enter { frame, kind }
                if frame == work.binding.frame && kind == green_kind(BlockKind::Paragraph)
        ) {
            return Err(M11BlockWriterError::InvalidCommand(
                "fragment reference rewrite target is not its Paragraph Enter",
            ));
        }
        if !matches!(work.mode, M11FragmentReferenceRewriteMode::Remove { .. }) {
            push_fragment_rewrite_event(work, event)?;
        }
        return Ok(());
    }
    match event {
        M11RecursiveGreenEvent::Enter { .. } | M11RecursiveGreenEvent::Exit { .. } => {
            Err(M11BlockWriterError::InvalidCommand(
                "fragment reference rewrite crossed a nested structural event",
            ))
        }
        M11RecursiveGreenEvent::Property(_) | M11RecursiveGreenEvent::RetypeOpen { .. }
            if matches!(work.mode, M11FragmentReferenceRewriteMode::Remove { .. }) =>
        {
            Ok(())
        }
        M11RecursiveGreenEvent::Property(_) | M11RecursiveGreenEvent::RetypeOpen { .. } => {
            push_fragment_rewrite_event(work, event)
        }
        M11RecursiveGreenEvent::Coverage {
            physical,
            owner_depth,
            part,
            logical,
        } => {
            let start = work.physical_position;
            let physical_metric = SourceMetric::new(physical.bytes(), physical.utf16())
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            let end = start
                .checked_add(physical_metric)
                .ok_or(M11BlockWriterError::CounterOverflow)?;
            work.physical_position = end;
            let targets = fragment_logical_targets_paragraph(owner_depth, logical);
            match work.mode {
                M11FragmentReferenceRewriteMode::Unchanged => unreachable!(),
                M11FragmentReferenceRewriteMode::Remove { cut } => {
                    if !metric_precedes(end, cut) {
                        return Err(M11BlockWriterError::InvalidCommand(
                            "fragment reference-only range did not cover its Paragraph",
                        ));
                    }
                    let (owner_depth, part, logical) = if targets {
                        (
                            owner_depth.saturating_sub(1),
                            if owner_depth == 0 {
                                M11RecursiveGreenCoveragePart::Gap
                            } else {
                                part
                            },
                            M11RecursiveGreenLogicalAction::None,
                        )
                    } else {
                        (
                            owner_depth.saturating_sub(1),
                            part,
                            rebase_removed_fragment_action(logical)?,
                        )
                    };
                    push_fragment_rewrite_event(
                        work,
                        M11RecursiveGreenEvent::Coverage {
                            physical,
                            owner_depth,
                            part,
                            logical,
                        },
                    )
                }
                M11FragmentReferenceRewriteMode::Retain { cut }
                    if targets && metric_precedes(start, cut) && start != cut =>
                {
                    if metric_precedes(end, cut) {
                        let (next_owner, next_part) = if owner_depth == 0 {
                            (1, M11RecursiveGreenCoveragePart::Gap)
                        } else {
                            (owner_depth, part)
                        };
                        push_fragment_rewrite_event(
                            work,
                            M11RecursiveGreenEvent::Coverage {
                                physical,
                                owner_depth: next_owner,
                                part: next_part,
                                logical: M11RecursiveGreenLogicalAction::None,
                            },
                        )
                    } else {
                        if !matches!(
                            logical,
                            M11RecursiveGreenLogicalAction::Identity
                                | M11RecursiveGreenLogicalAction::CanonicalText
                        ) {
                            return Err(M11BlockWriterError::InvalidCommand(
                                "fragment reference cut splits a non-text projection",
                            ));
                        }
                        let prefix = metric_difference(cut, start)?;
                        let suffix = metric_difference(end, cut)?;
                        if prefix.is_empty() || suffix.is_empty() {
                            return Err(M11BlockWriterError::InvalidCommand(
                                "fragment reference text split is empty",
                            ));
                        }
                        push_fragment_rewrite_event(
                            work,
                            M11RecursiveGreenEvent::Coverage {
                                physical: green_metric(prefix)?,
                                owner_depth: 1,
                                part: M11RecursiveGreenCoveragePart::Gap,
                                logical: M11RecursiveGreenLogicalAction::None,
                            },
                        )?;
                        push_fragment_rewrite_event(
                            work,
                            M11RecursiveGreenEvent::Coverage {
                                physical: green_metric(suffix)?,
                                owner_depth,
                                part,
                                logical,
                            },
                        )
                    }
                }
                M11FragmentReferenceRewriteMode::Retain { .. } => {
                    push_fragment_rewrite_event(work, event)
                }
            }
        }
    }
}

fn fragment_logical_targets_paragraph(
    owner_depth: u32,
    logical: M11RecursiveGreenLogicalAction,
) -> bool {
    match logical {
        M11RecursiveGreenLogicalAction::None | M11RecursiveGreenLogicalAction::HiddenUpstream => {
            false
        }
        M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth, ..
        } => target_owner_depth == 0,
        _ => owner_depth == 0,
    }
}

fn rebase_removed_fragment_action(
    logical: M11RecursiveGreenLogicalAction,
) -> Result<M11RecursiveGreenLogicalAction, M11BlockWriterError> {
    Ok(match logical {
        M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth: 0,
            ..
        } => M11RecursiveGreenLogicalAction::None,
        M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth,
            remaining_spaces,
        } => M11RecursiveGreenLogicalAction::PartialTab {
            target_owner_depth: target_owner_depth
                .checked_sub(1)
                .ok_or(M11BlockWriterError::CounterOverflow)?,
            remaining_spaces,
        },
        other => other,
    })
}

fn push_fragment_rewrite_event(
    work: &mut M11FragmentReferenceRewriteWork,
    event: M11RecursiveGreenEvent,
) -> Result<(), M11BlockWriterError> {
    work.replacement
        .try_reserve(1)
        .map_err(|_| M11BlockWriterError::Allocation)?;
    work.replacement.push(event);
    Ok(())
}

fn rebuild_fragment_event_journal(
    fragment: &mut M11BlockFragmentOutput,
    replacement: Vec<M11RecursiveGreenEvent>,
    base_receipt: WriterOutputReceipt,
) -> Result<(), M11BlockWriterError> {
    let original_events = std::mem::take(&mut fragment.events);
    let original_receipt = fragment.receipt;
    fragment.receipt = base_receipt;
    fragment.events = Vec::new();
    fragment
        .events
        .try_reserve(replacement.len())
        .map_err(|_| M11BlockWriterError::Allocation)?;
    for event in replacement {
        if let Err(error) = fragment.offer_event(event) {
            fragment.events = original_events;
            fragment.receipt = original_receipt;
            return Err(error);
        }
    }
    Ok(())
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11BlockStructuralAdoptionReceipt {
    green: M11RecursiveGreenStructuralSpliceReceipt,
    high_level_events: usize,
    fragment_source_bytes_read: u64,
}

/// Opaque coordinate authority used by the persistent-session checkpoint
/// actor after an atomic Green splice.  Replaying checkpoint replicas is
/// deliberately separate from the splice: callers can fuel one retained
/// checkpoint at a time instead of hiding document-sized work in adoption.
pub(crate) struct M11BlockCheckpointRebase {
    target_source: SourceVersion,
    splice: M11RecursiveGreenStructuralSpliceRebase,
    target_frame_floor: u64,
    suffix: Option<M11BlockCheckpointSuffixRebase>,
}

struct M11BlockCheckpointSuffixRebase {
    base_physical_end: SourceMetric,
    target_physical_end: SourceMetric,
    base_logical_end: SourceMetric,
    target_logical_end: SourceMetric,
    base_convergence_line_ordinal: u64,
    target_convergence_line_ordinal: u64,
}

pub(crate) struct M11BlockOrdinaryCheckpointAdoption {
    pub(crate) rebase: M11BlockCheckpointRebase,
    pub(crate) target_restart: M11BlockRestartCheckpoint,
    pub(crate) target_convergence: M11BlockRestartCheckpoint,
    pub(crate) retained_terminal: M11BlockTerminalConvergenceCheckpoint,
}

pub(crate) struct M11BlockTerminalCheckpointAdoption {
    pub(crate) rebase: M11BlockCheckpointRebase,
    pub(crate) target_restart: M11BlockRestartCheckpoint,
    pub(crate) target_terminal: M11BlockTerminalConvergenceCheckpoint,
}

impl M11BlockCheckpointRebase {
    pub(crate) fn rebase_prefix(
        &self,
        checkpoint: &mut M11BlockRestartCheckpoint,
    ) -> Result<(), M11BlockRestartError> {
        rebase_retained_prefix_checkpoint(checkpoint, &self.splice, self.target_frame_floor)
    }

    pub(crate) fn rebase_suffix(
        &self,
        checkpoint: &mut M11BlockRestartCheckpoint,
    ) -> Result<(), M11BlockRestartError> {
        let suffix = self.suffix.as_ref().ok_or(M11BlockRestartError::Pairing(
            "terminal checkpoint adoption has no unchanged suffix",
        ))?;
        rebase_retained_suffix_checkpoint(
            checkpoint,
            &self.splice,
            suffix.base_physical_end,
            suffix.target_physical_end,
            suffix.base_logical_end,
            suffix.target_logical_end,
            suffix.base_convergence_line_ordinal,
            suffix.target_convergence_line_ordinal,
            self.target_frame_floor,
        )
    }

    pub(crate) fn rebase_terminal(
        &self,
        checkpoint: &mut M11BlockTerminalConvergenceCheckpoint,
    ) -> Result<(), M11BlockRestartError> {
        rebase_retained_terminal_checkpoint(checkpoint, &self.splice, self.target_frame_floor)
    }

    pub(crate) fn validate_next(
        &self,
        previous: Option<&M11BlockRestartCheckpoint>,
        checkpoint: &M11BlockRestartCheckpoint,
    ) -> Result<(), M11BlockRestartError> {
        if checkpoint.source != self.target_source
            || checkpoint.green_boundary.is_none()
            || previous.is_some_and(|previous| {
                previous.parser_physical.bytes() > checkpoint.parser_physical.bytes()
                    || previous.parser_physical.utf16() > checkpoint.parser_physical.utf16()
                    || previous.accepted_physical.bytes() > checkpoint.accepted_physical.bytes()
                    || previous.accepted_physical.utf16() > checkpoint.accepted_physical.utf16()
            })
        {
            return Err(M11BlockRestartError::Pairing(
                "rebased checkpoint set is not ordered target authority",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_terminal(
        &self,
        checkpoint: &M11BlockTerminalConvergenceCheckpoint,
    ) -> Result<(), M11BlockRestartError> {
        if checkpoint.source != self.target_source
            || checkpoint.green_boundary.is_none()
            || checkpoint.accepted_physical.bytes()
                != u64::try_from(checkpoint.source.byte_len()).unwrap_or(u64::MAX)
            || checkpoint.accepted_physical.utf16()
                != u64::try_from(checkpoint.source.utf16_len()).unwrap_or(u64::MAX)
        {
            return Err(M11BlockRestartError::Pairing(
                "rebased checkpoint set is not ordered target authority",
            ));
        }
        Ok(())
    }
}

impl M11BlockStructuralAdoptionReceipt {
    #[must_use]
    pub const fn green(&self) -> &M11RecursiveGreenStructuralSpliceReceipt {
        &self.green
    }

    /// Exact changed Green leaf segments selected by the writer's
    /// authenticated restart/convergence splice and spanning repairs.
    #[must_use]
    pub const fn green_splice_selection(&self) -> &M11RecursiveGreenStructuralSpliceSelection {
        self.green.selection()
    }

    #[must_use]
    pub const fn high_level_events(&self) -> usize {
        self.high_level_events
    }

    #[must_use]
    pub const fn fragment_source_bytes_read(&self) -> u64 {
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
                reference: None,
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

    pub(super) fn begin_reference_output_fragment(
        &mut self,
        frame: M11RecursiveGreenFrameId,
    ) -> Result<(), M11BlockWriterError> {
        match &mut self.output {
            WriterOutput::Document(build) => {
                let fragment = build.mint_terminal_fragment(frame)?;
                Ok(build.begin_terminal_fragment_barrier(fragment)?)
            }
            WriterOutput::Fragment(fragment) => {
                let provenance =
                    self.restart_provenance
                        .as_ref()
                        .ok_or(M11BlockWriterError::InvalidCommand(
                            "fragment reference finalization lost restart provenance",
                        ))?;
                if frame.get() <= provenance.base_maximum_frame_id {
                    return Err(M11BlockWriterError::ReferenceParagraphPredatesRestart);
                }
                if fragment.reference.is_some() {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "a fragment reference projection is already active",
                    ));
                }
                let paragraph_kind = green_kind(BlockKind::Paragraph);
                let mut physical = provenance.target_accepted_start;
                let mut local_enter = None;
                for (index, event) in fragment.events.iter().copied().enumerate() {
                    match event {
                        M11RecursiveGreenEvent::Enter {
                            frame: candidate,
                            kind,
                        } if candidate == frame && kind == paragraph_kind => {
                            if local_enter.replace((index, physical)).is_some() {
                                return Err(M11BlockWriterError::InvalidCommand(
                                    "local reference Paragraph has more than one Enter",
                                ));
                            }
                        }
                        M11RecursiveGreenEvent::Coverage {
                            physical: metric, ..
                        } => {
                            physical = physical
                                .checked_add(
                                    SourceMetric::new(metric.bytes(), metric.utf16())
                                        .ok_or(M11BlockWriterError::CounterOverflow)?,
                                )
                                .ok_or(M11BlockWriterError::CounterOverflow)?;
                        }
                        _ => {}
                    }
                }
                let (enter_event, physical_before) =
                    local_enter.ok_or(M11BlockWriterError::ReferenceParagraphPredatesRestart)?;
                let physical_end =
                    SourceMetric::new(fragment.receipt.source_bytes, fragment.receipt.source_utf16)
                        .ok_or(M11BlockWriterError::CounterOverflow)?;
                if physical != physical_end {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment event journal and physical receipt differ",
                    ));
                }
                let generation = REFERENCE_FRAGMENT_IDS
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                        current.checked_add(1)
                    })
                    .map_err(|_| M11BlockWriterError::CounterOverflow)?;
                fragment.reference = Some(M11FragmentReferenceState {
                    binding: M11FragmentReferenceBinding {
                        generation,
                        frame,
                        enter_event,
                        events_end: fragment.events.len(),
                        physical_before,
                        physical_end,
                        base_receipt: WriterOutputReceipt {
                            events: provenance.base_event_cut,
                            source_bytes: provenance.target_accepted_start.bytes(),
                            source_utf16: provenance.target_accepted_start.utf16(),
                            logical_bytes: provenance.target_logical_start.bytes(),
                            logical_utf16: provenance.target_logical_start.utf16(),
                        },
                    },
                    phase: M11FragmentReferencePhase::Barrier,
                });
                Ok(())
            }
        }
    }

    pub(super) fn poll_reference_output_barrier(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenTerminalFragmentBarrierStatus, M11BlockWriterError> {
        match &mut self.output {
            WriterOutput::Document(build) => Ok(build
                .poll_terminal_fragment_barrier(runtime, fuel)?
                .status()),
            WriterOutput::Fragment(fragment) => {
                if fuel == 0 {
                    return Err(M11BlockWriterError::ZeroFuel);
                }
                let state =
                    fragment
                        .reference
                        .as_mut()
                        .ok_or(M11BlockWriterError::InvalidCommand(
                            "fragment reference barrier is not active",
                        ))?;
                match state.phase {
                    M11FragmentReferencePhase::Barrier => {
                        state.phase = M11FragmentReferencePhase::Frozen;
                        Ok(M11RecursiveGreenTerminalFragmentBarrierStatus::Ready)
                    }
                    M11FragmentReferencePhase::Frozen => {
                        Ok(M11RecursiveGreenTerminalFragmentBarrierStatus::Ready)
                    }
                    M11FragmentReferencePhase::Rewriting => {
                        Err(M11BlockWriterError::InvalidCommand(
                            "fragment reference barrier is already rewriting",
                        ))
                    }
                }
            }
        }
    }

    pub(super) fn take_reference_output_binding(
        &mut self,
    ) -> Result<M11ReferenceOutputBinding, M11BlockWriterError> {
        match &mut self.output {
            WriterOutput::Document(build) => Ok(M11ReferenceOutputBinding::Document(
                build.take_terminal_fragment_binding()?,
            )),
            WriterOutput::Fragment(fragment) => {
                let state =
                    fragment
                        .reference
                        .as_ref()
                        .ok_or(M11BlockWriterError::InvalidCommand(
                            "fragment reference binding is not active",
                        ))?;
                if state.phase != M11FragmentReferencePhase::Frozen {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment reference binding is not frozen",
                    ));
                }
                Ok(M11ReferenceOutputBinding::Fragment(state.binding))
            }
        }
    }

    pub(super) fn open_reference_output_cursor(
        &mut self,
        binding: &M11ReferenceOutputBinding,
    ) -> Result<M11ReferenceOutputCursor, M11BlockWriterError> {
        match (&mut self.output, binding) {
            (WriterOutput::Document(build), M11ReferenceOutputBinding::Document(binding)) => Ok(
                M11ReferenceOutputCursor::Document(build.open_terminal_fragment_cursor(binding)?),
            ),
            (WriterOutput::Fragment(fragment), M11ReferenceOutputBinding::Fragment(binding)) => {
                validate_fragment_reference_binding(fragment, binding)?;
                Ok(M11ReferenceOutputCursor::Fragment(
                    M11FragmentReferenceCursor::new(*binding, None)?,
                ))
            }
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference binding crossed its writer output",
            )),
        }
    }

    pub(super) fn bind_reference_output_logical_range(
        &mut self,
        binding: &M11ReferenceOutputBinding,
        range: M11RecursiveGreenLogicalRange,
    ) -> Result<M11ReferenceOutputRange, M11BlockWriterError> {
        match (&mut self.output, binding) {
            (WriterOutput::Document(build), M11ReferenceOutputBinding::Document(binding)) => {
                Ok(M11ReferenceOutputRange::Document(
                    build.bind_terminal_fragment_logical_range(binding, range)?,
                ))
            }
            (WriterOutput::Fragment(fragment), M11ReferenceOutputBinding::Fragment(binding)) => {
                validate_fragment_reference_binding(fragment, binding)?;
                Ok(M11ReferenceOutputRange::Fragment(
                    M11FragmentReferenceRange {
                        generation: binding.generation,
                        logical: range,
                        physical: None,
                        replay_validated: false,
                    },
                ))
            }
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference range crossed its writer output",
            )),
        }
    }

    pub(super) fn open_reference_output_range_replay(
        &mut self,
        binding: &M11ReferenceOutputBinding,
        range: M11ReferenceOutputRange,
    ) -> Result<M11ReferenceOutputCursor, M11BlockWriterError> {
        match (&mut self.output, binding, range) {
            (
                WriterOutput::Document(build),
                M11ReferenceOutputBinding::Document(binding),
                M11ReferenceOutputRange::Document(range),
            ) => Ok(M11ReferenceOutputCursor::Document(
                build.open_terminal_fragment_range_replay(binding, range)?,
            )),
            (
                WriterOutput::Fragment(fragment),
                M11ReferenceOutputBinding::Fragment(binding),
                M11ReferenceOutputRange::Fragment(range),
            ) => {
                validate_fragment_reference_binding(fragment, binding)?;
                if range.generation != binding.generation {
                    return Err(M11BlockWriterError::InvalidCommand(
                        "fragment reference range crossed its binding",
                    ));
                }
                Ok(M11ReferenceOutputCursor::Fragment(
                    M11FragmentReferenceCursor::new(*binding, Some(range))?,
                ))
            }
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference replay crossed its writer output",
            )),
        }
    }

    pub(super) fn retarget_reference_output_range_replay_forward(
        &mut self,
        binding: &M11ReferenceOutputBinding,
        cursor: &mut M11ReferenceOutputCursor,
        range: M11ReferenceOutputRange,
    ) -> Result<(), M11BlockWriterError> {
        match (&mut self.output, binding, cursor, range) {
            (
                WriterOutput::Document(build),
                M11ReferenceOutputBinding::Document(binding),
                M11ReferenceOutputCursor::Document(cursor),
                M11ReferenceOutputRange::Document(range),
            ) => Ok(build.retarget_terminal_fragment_range_replay_forward(binding, cursor, range)?),
            (
                WriterOutput::Fragment(fragment),
                M11ReferenceOutputBinding::Fragment(binding),
                M11ReferenceOutputCursor::Fragment(cursor),
                M11ReferenceOutputRange::Fragment(range),
            ) => {
                validate_fragment_reference_binding(fragment, binding)?;
                cursor.retarget_forward(range)
            }
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference replay retarget crossed its writer output",
            )),
        }
    }

    pub(super) fn poll_reference_output_cursor(
        &mut self,
        runtime: &mut DocumentRuntime,
        cursor: &mut M11ReferenceOutputCursor,
        fuel: usize,
        chunked: bool,
    ) -> Result<M11RecursiveGreenTerminalFragmentCursorStatus, M11BlockWriterError> {
        match (&mut self.output, cursor) {
            (WriterOutput::Document(build), M11ReferenceOutputCursor::Document(cursor)) => {
                let poll: M11RecursiveGreenTerminalFragmentCursorPoll = if chunked {
                    build.poll_terminal_fragment_cursor_chunk(runtime, cursor, fuel)?
                } else {
                    build.poll_terminal_fragment_cursor(runtime, cursor, fuel)?
                };
                Ok(poll.status())
            }
            (WriterOutput::Fragment(fragment), M11ReferenceOutputCursor::Fragment(cursor)) => {
                poll_fragment_reference_cursor(fragment, cursor, fuel, chunked)
            }
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference cursor crossed its writer output",
            )),
        }
    }

    pub(super) fn begin_reference_output_rewrite(
        &mut self,
        runtime: &mut DocumentRuntime,
        binding: M11ReferenceOutputBinding,
        rewrite: M11ReferenceOutputRewrite,
    ) -> Result<M11ReferenceOutputRewriteWork, M11BlockWriterError> {
        match (&mut self.output, binding, rewrite) {
            (
                WriterOutput::Document(build),
                M11ReferenceOutputBinding::Document(binding),
                M11ReferenceOutputRewrite::Unchanged,
            ) => Ok(M11ReferenceOutputRewriteWork::Document(
                build.begin_terminal_fragment_rewrite(
                    runtime,
                    binding,
                    M11RecursiveGreenTerminalFragmentRewrite::Unchanged,
                )?,
            )),
            (
                WriterOutput::Document(build),
                M11ReferenceOutputBinding::Document(binding),
                M11ReferenceOutputRewrite::RemoveWrapper {
                    whole_fragment: M11ReferenceOutputRange::Document(whole_fragment),
                },
            ) => Ok(M11ReferenceOutputRewriteWork::Document(
                build.begin_terminal_fragment_rewrite(
                    runtime,
                    binding,
                    M11RecursiveGreenTerminalFragmentRewrite::RemoveWrapper { whole_fragment },
                )?,
            )),
            (
                WriterOutput::Document(build),
                M11ReferenceOutputBinding::Document(binding),
                M11ReferenceOutputRewrite::RetainVisibleSuffix {
                    removed_prefix: M11ReferenceOutputRange::Document(removed_prefix),
                },
            ) => Ok(M11ReferenceOutputRewriteWork::Document(
                build.begin_terminal_fragment_rewrite(
                    runtime,
                    binding,
                    M11RecursiveGreenTerminalFragmentRewrite::RetainVisibleSuffix {
                        removed_prefix,
                    },
                )?,
            )),
            (
                WriterOutput::Fragment(fragment),
                M11ReferenceOutputBinding::Fragment(binding),
                rewrite,
            ) => Ok(M11ReferenceOutputRewriteWork::Fragment(
                begin_fragment_reference_rewrite(fragment, binding, rewrite)?,
            )),
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference rewrite crossed its writer output",
            )),
        }
    }

    pub(super) fn poll_reference_output_rewrite(
        &mut self,
        runtime: &mut DocumentRuntime,
        work: &mut M11ReferenceOutputRewriteWork,
        fuel: usize,
    ) -> Result<M11ReferenceOutputRewritePoll, M11BlockWriterError> {
        match (&mut self.output, work) {
            (WriterOutput::Document(build), M11ReferenceOutputRewriteWork::Document(work)) => {
                match build.poll_terminal_fragment_rewrite(runtime, work, fuel)? {
                    M11RecursiveGreenTerminalFragmentRewritePoll::Pending { .. } => {
                        Ok(M11ReferenceOutputRewritePoll::Pending)
                    }
                    M11RecursiveGreenTerminalFragmentRewritePoll::Complete {
                        mut authority,
                        ..
                    } => {
                        let visible_remainder_boundary =
                            authority.take_visible_remainder_boundary();
                        let visible_remainder_physical =
                            visible_remainder_boundary.as_ref().and_then(|boundary| {
                                let metric = boundary.physical_metric();
                                SourceMetric::new(metric.bytes(), metric.utf16())
                            });
                        Ok(M11ReferenceOutputRewritePoll::Complete(
                            M11ReferenceOutputRewriteAuthority {
                                frame: authority.frame(),
                                disposition: authority.disposition(),
                                visible_remainder_boundary,
                                visible_remainder_physical,
                            },
                        ))
                    }
                }
            }
            (WriterOutput::Fragment(fragment), M11ReferenceOutputRewriteWork::Fragment(work)) => {
                poll_fragment_reference_rewrite(fragment, work, fuel)
            }
            _ => Err(M11BlockWriterError::InvalidCommand(
                "reference rewrite work crossed its writer output",
            )),
        }
    }

    pub(super) fn offer_reference_output_event(
        &mut self,
        event: M11RecursiveGreenEvent,
    ) -> Result<(), M11BlockWriterError> {
        self.output.offer_event(event)
    }

    pub(super) fn poll_reference_output(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenBuildStatus, M11BlockWriterError> {
        match &mut self.output {
            WriterOutput::Document(build) => Ok(build.poll(runtime, fuel)?.status()),
            WriterOutput::Fragment(_) if fuel == 0 => Err(M11BlockWriterError::ZeroFuel),
            WriterOutput::Fragment(_) => Ok(M11RecursiveGreenBuildStatus::NeedsInput),
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

    /// Checks one ordinary suffix boundary without consuming fragment, Green,
    /// checkpoint, or lineage authority. A `false` result means the caller may
    /// continue the definitive parse and try a later authenticated checkpoint.
    pub(crate) fn probe_converged_fragment(
        &self,
        parser: M11DirectBlockRestart,
        target_restart: &M11BlockRestartCheckpoint,
        old_convergence: &M11BlockRestartCheckpoint,
        runtime: &DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        prefix: Option<&ExactUnchangedPrefixWitness>,
        suffix: Option<&ExactUnchangedSuffixWitness>,
    ) -> Result<bool, M11BlockRestartError> {
        let fresh = self.capture_restart_checkpoint(parser)?;
        let provenance = self
            .restart_provenance
            .as_ref()
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
        {
            return Err(M11BlockRestartError::Pairing(
                "convergence probe crossed source or checkpoint authority",
            ));
        }
        if provenance.base_event_cut >= old_convergence.event_cut
            || provenance.target_accepted_start.bytes() >= fresh.accepted_physical.bytes()
            || !fresh
                .parser
                .is_future_compatible_with(&old_convergence.parser)
            || !same_open_path(&fresh.open, &old_convergence.open)
            || !same_staged_role(fresh.staged, old_convergence.staged)
            || fresh.parser.last_line_length() != old_convergence.parser.last_line_length()
        {
            return Ok(false);
        }
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
                .as_ref()
                .ok_or(M11BlockRestartError::Pairing(
                    "base convergence checkpoint lacks committed Green authority",
                ))?;
        if !boundary_matches_open(end_boundary, &old_convergence.open)
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
        let suffix_matches = match suffix {
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
        let prefix_matches = match prefix {
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

        match plan_open_row_exit_repairs(&fresh.open, &old_convergence.open) {
            Ok(_) => Ok(true),
            Err(M11BlockRestartError::Pairing(
                "ordinary spanning Exit state requires clean fallback",
            )) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Authenticates parser convergence, adopts the bounded target fragment,
    /// and activates the returned target checkpoint against the new committed
    /// Green root.
    ///
    /// Neither event/source cuts nor external depth are caller input. They are
    /// taken from the move-only Green boundaries joined into the restart and
    /// convergence checkpoints. Crossed roots, parser transactions, or open
    /// paths fail closed before storage mutation begins.
    pub(crate) fn adopt_converged_fragment(
        mut self,
        parser: M11DirectBlockRestart,
        mut target_restart: M11BlockRestartCheckpoint,
        mut old_convergence: M11BlockRestartCheckpoint,
        runtime: &mut DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        prefix: Option<ExactUnchangedPrefixWitness>,
        suffix: Option<ExactUnchangedSuffixWitness>,
        retained_terminal: M11BlockTerminalConvergenceCheckpoint,
    ) -> Result<
        (
            M11RecursiveGreenRoot,
            M11BlockStructuralAdoptionReceipt,
            M11BlockOrdinaryCheckpointAdoption,
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

        let spanning_exit_repairs = plan_open_row_exit_repairs(&fresh.open, &old_convergence.open)?;
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
            splice_m11_recursive_green_structural_with_spanning_exit_repairs_atomic(
                runtime,
                base,
                target_lease,
                prefix,
                suffix,
                provenance.start_boundary,
                end_boundary,
                target_end_physical,
                &fragment.events,
                &spanning_exit_repairs,
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
        Ok((
            root,
            M11BlockStructuralAdoptionReceipt {
                green,
                high_level_events,
                fragment_source_bytes_read,
            },
            M11BlockOrdinaryCheckpointAdoption {
                rebase: M11BlockCheckpointRebase {
                    target_source: fresh.source,
                    splice: rebase,
                    target_frame_floor,
                    suffix: Some(M11BlockCheckpointSuffixRebase {
                        base_physical_end: old_convergence.accepted_physical,
                        target_physical_end: fresh.accepted_physical,
                        base_logical_end: old_convergence.logical,
                        target_logical_end: fresh.logical,
                        base_convergence_line_ordinal,
                        target_convergence_line_ordinal,
                    }),
                },
                target_restart,
                target_convergence: fresh,
                retained_terminal,
            },
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
    pub(crate) fn adopt_converged_terminal_fragment(
        mut self,
        terminal_close: BlockCommand,
        mut target_restart: M11BlockRestartCheckpoint,
        mut old_terminal: M11BlockTerminalConvergenceCheckpoint,
        runtime: &mut DocumentRuntime,
        base: &M11RecursiveGreenRoot,
        prefix: Option<ExactUnchangedPrefixWitness>,
    ) -> Result<
        (
            M11RecursiveGreenRoot,
            M11BlockStructuralAdoptionReceipt,
            M11BlockTerminalCheckpointAdoption,
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
        let BlockCommand::Close {
            kind: terminal_kind,
            final_facts,
            last_line_blank,
            child,
        } = terminal_close
        else {
            return Err(M11BlockRestartError::Pairing(
                "terminal convergence omitted its pending close state",
            ));
        };
        let terminal_frame = *self.open.last().ok_or(M11BlockRestartError::Pairing(
            "terminal convergence omitted its open Document",
        ))?;
        if terminal_kind != terminal_frame.kind || terminal_frame.kind != BlockKind::Document {
            return Err(M11BlockRestartError::Pairing(
                "terminal convergence close differs from its open Document",
            ));
        }
        let terminal_repair = M11RecursiveGreenSpanningExitRepair::Exact {
            frame: terminal_frame.id,
            final_kind: green_kind(terminal_kind),
            close: self.close_facts(terminal_frame, final_facts)?,
            last_line_blank,
            child: M11RecursiveGreenClosedChild::new(
                child.ends_blank(),
                child.item_loose_if_nonlast(),
                child.item_loose_if_last(),
            ),
        };
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
            splice_m11_recursive_green_structural_with_spanning_exit_repairs_atomic(
                runtime,
                base,
                target_lease,
                prefix,
                None,
                provenance.start_boundary,
                end_boundary,
                target_end_physical,
                &fragment.events,
                &[terminal_repair],
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
        Ok((
            root,
            M11BlockStructuralAdoptionReceipt {
                green,
                high_level_events,
                fragment_source_bytes_read,
            },
            M11BlockTerminalCheckpointAdoption {
                rebase: M11BlockCheckpointRebase {
                    target_source,
                    splice: rebase,
                    target_frame_floor,
                    suffix: None,
                },
                target_restart,
                target_terminal,
            },
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
                if matches!(ancestor.kind, BlockKind::Item(_) | BlockKind::BlockQuote) {
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
            has_unrepresented_container_marker: false,
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
        if part == CoveragePart::ContainerMarker {
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
                    "container marker owner is outside the open path",
                ))?;
            if matches!(self.open[owner_index].kind, BlockKind::BlockQuote) {
                self.open[owner_index].has_unrepresented_container_marker = true;
            }
        }
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
        let empty_container_row_kind = match frame.kind {
            BlockKind::Item(_) if !frame.has_renderable_descendant => Some(KIND_EMPTY_ITEM_ROW),
            BlockKind::BlockQuote
                if !frame.has_renderable_descendant || frame.has_unrepresented_container_marker =>
            {
                Some(KIND_EMPTY_BLOCK_QUOTE_ROW)
            }
            _ => None,
        };
        let close = self.close_facts(frame, final_facts)?;
        self.open.pop();
        if is_renderable_block_kind(frame.kind) {
            for ancestor in &mut self.open {
                if matches!(ancestor.kind, BlockKind::Item(_)) {
                    ancestor.has_renderable_descendant = true;
                }
                if matches!(ancestor.kind, BlockKind::BlockQuote) {
                    ancestor.has_renderable_descendant = true;
                    ancestor.has_unrepresented_container_marker = false;
                }
            }
        }
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
        self.pending = Some(Pending::Events(
            if let Some(empty_row_kind) = empty_container_row_kind {
                for ancestor in &mut self.open {
                    if matches!(ancestor.kind, BlockKind::Item(_)) {
                        ancestor.has_renderable_descendant = true;
                    }
                    if matches!(ancestor.kind, BlockKind::BlockQuote) {
                        ancestor.has_renderable_descendant = true;
                        ancestor.has_unrepresented_container_marker = false;
                    }
                }
                let row_frame = M11RecursiveGreenFrameId::new(self.next_frame)
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                self.next_frame = self
                    .next_frame
                    .checked_add(1)
                    .ok_or(M11BlockWriterError::CounterOverflow)?;
                let row_kind = M11RecursiveGreenKind::new(empty_row_kind)
                    .expect("empty-container row kind is nonzero");
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
            },
        ));
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
                // The frame summary already authenticates the terminal logical
                // metric, so retain only the two internal cuts here. This keeps
                // the grammar-owned fence recipe plus the versioned cached-row
                // trailer inside the fixed 64-byte close-facts envelope.
                let mut semantic = [0_u8; 33];
                semantic[0] = u8::from(facts.closed());
                let mut cursor = 1;
                for metric in [info_end, literal_start] {
                    semantic[cursor..cursor + 8].copy_from_slice(&metric.bytes().to_le_bytes());
                    cursor += 8;
                    semantic[cursor..cursor + 8].copy_from_slice(&metric.utf16().to_le_bytes());
                    cursor += 8;
                }
                Ok(Some(close_facts_with_cached_row(
                    FACT_CODE,
                    &semantic,
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

fn plan_open_row_exit_repairs(
    fresh: &[OpenFrame],
    old: &[OpenFrame],
) -> Result<Vec<M11RecursiveGreenSpanningExitRepair>, M11BlockRestartError> {
    if !same_open_path(fresh, old) {
        return Err(M11BlockRestartError::Pairing(
            "spanning Exit repair open paths differ",
        ));
    }
    let mut repairs = Vec::new();
    repairs
        .try_reserve_exact(fresh.len())
        .map_err(|_| M11BlockWriterError::Allocation)?;
    for (fresh, old) in fresh.iter().zip(old) {
        if fresh.fence != old.fence
            || fresh.has_renderable_descendant != old.has_renderable_descendant
            || fresh.has_unrepresented_container_marker != old.has_unrepresented_container_marker
        {
            return Err(M11BlockRestartError::Pairing(
                "ordinary spanning Exit state requires clean fallback",
            ));
        }
        match (fresh.row_editable, old.row_editable) {
            (None, None) => {}
            (Some(fresh_row), Some(old_row))
                if fresh_row.physical_base == old_row.physical_base
                    && fresh_row.start == old_row.start
                    && fresh_row.gap_after == old_row.gap_after
                    && fresh_row.contiguous == old_row.contiguous
                    && fresh_row.tracking == old_row.tracking =>
            {
                if fresh_row.end != old_row.end {
                    repairs.push(M11RecursiveGreenSpanningExitRepair::TranslateCachedRow {
                        frame: fresh.id,
                        base_convergence_end: green_metric_allow_empty(old_row.end)?,
                        target_convergence_end: green_metric_allow_empty(fresh_row.end)?,
                    });
                }
            }
            _ => {
                return Err(M11BlockRestartError::Pairing(
                    "ordinary spanning Exit state requires clean fallback",
                ));
            }
        }
    }
    Ok(repairs)
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
            let mut payload = [0_u8; 5];
            payload[..2].copy_from_slice(&facts.marker_offset().to_le_bytes());
            payload[2..4].copy_from_slice(&facts.padding().to_le_bytes());
            payload[4] = match facts.task_checked() {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            };
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

//! First coherent source-to-owned-arena Markdown parse job.
//!
//! This module deliberately composes the existing capture-backed block parser,
//! source-order block traversal, shared lexer, and inline resolver without
//! introducing another grammar representation. Completed inline payload pages
//! move directly from the resolver into [`PhysicalLifetime`]; leaf records and
//! the final manifest are the only newly serialized arena pages.
//!
//! The composition is executable evidence, not yet a production-admissibility
//! claim. The block parser and shared lexer still lack complete next-operation
//! allocation preflights. Those limitations remain typed below while physical
//! arena mutation itself is exactly preflighted.

use std::fmt;
use std::mem::size_of;
use std::sync::Arc;

#[cfg(feature = "crop-research")]
use crate::crop_source::CropSnapshotLease;

use crate::arena::ARENA_PAGE_BYTES;
use crate::block::{
    BlockContainer, BlockError, BlockJob, BlockLeaf, BlockLeafStep, BlockLeaves, BlockStatus,
    BlockWorkReceipt, MAX_BLOCK_CONTAINER_DEPTH,
};
use crate::execution::{JobPollFailure, MeasuredParseJob};
use crate::frontier::{CursorMetrics, LexerStatus, SharedLexer};
use crate::inline_machine::{
    InlineMachine, InlineOutputPageDrain, InlineOutputPageDrainMetrics, InlineOutputPageDrainStep,
    InlineStatus, InlineTelemetry, InlineWork, INLINE_OUTPUT_PAGE_BYTES,
};
use crate::lifetime::{LifetimeError, OwnedPageAppend, PhysicalLifetime};
use crate::scheduler::{
    ArenaJobId, Audit, MeasuredParseReceipt, ParseToken, ParseWorkPermit, SliceLimits,
};
use crate::source::{PersistentSource, SourceRootIdentity};

const _: () = assert!(INLINE_OUTPUT_PAGE_BYTES == ARENA_PAGE_BYTES);

/// Anchor payload used by measured activation before constructing this job.
pub const OWNED_PARSE_ANCHOR: &[u8] = b"FLARK-OWNED-PARSE\x01";
/// Fixed leaf-record payload size. Canonical inline bytes remain in their
/// original separate 4 KiB pages.
pub const OWNED_LEAF_RECORD_BYTES: usize = 160;
/// Fixed final manifest payload size.
pub const OWNED_MANIFEST_BYTES: usize = 80;

const LEAF_MAGIC: &[u8; 4] = b"FLKL";
const MANIFEST_MAGIC: &[u8; 4] = b"FLKM";
const FORMAT_VERSION: u16 = 1;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Remaining reasons this vertical composition is not yet a production
/// scheduler-admissible parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedParseGap {
    /// `BlockJob` reports actual work only after a tick; several allocation
    /// sites expose counts or lower bounds but no exact preflight receipt.
    BlockNextOperationHasNoCompletePreflight,
    /// `SharedLexer` exposes scalar fuel and cursor totals but not per-poll
    /// allocation/copy/page-index receipts.
    LexerHasNoCompletePerPollReceipt,
    /// The inline machine has exact multidimensional admission, but its next
    /// requirement is not projected into the scheduler's seven dimensions.
    InlineRequirementIsNotProjectedToSchedulerPermit,
    /// Mid-build block/lexer `Arc` graphs are released by ordinary Rust drop,
    /// outside the arena's bounded reclaim queue.
    IntermediateArcGraphsDropOutsideArena,
    /// The physical prototype is a reverse construction chain, not the final
    /// persistent visible-page index needed for bounded random queries.
    ArenaOutputIsAReverseConstructionChain,
    /// Virtual logical bytes are not separated from physical source bytes in
    /// the public lexer cursor totals, so scheduler source-byte mapping is a
    /// conservative upper bound for those transitions.
    LexerVirtualBytesAreNotSeparatedInCursorReceipt,
}

pub const OWNED_PARSE_GAPS: &[OwnedParseGap] = &[
    OwnedParseGap::BlockNextOperationHasNoCompletePreflight,
    OwnedParseGap::LexerHasNoCompletePerPollReceipt,
    OwnedParseGap::InlineRequirementIsNotProjectedToSchedulerPermit,
    OwnedParseGap::IntermediateArcGraphsDropOutsideArena,
    OwnedParseGap::ArenaOutputIsAReverseConstructionChain,
    OwnedParseGap::LexerVirtualBytesAreNotSeparatedInCursorReceipt,
];

/// Observable scalar phase of the whole-path job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedParsePhase {
    StartBlock,
    Block,
    PrepareLeaves,
    Leaves,
    StartLexer,
    Lexer,
    PrepareInline,
    Inline,
    PrepareOutput,
    DrainInlinePage,
    AdoptInlinePage,
    AppendLeafRecord,
    AppendManifest,
    Ready,
    Failed,
}

/// Selected actual multidimensional telemetry retained across all scalar
/// slices. The scheduler ledger remains available in `total_audit`; fields not
/// representable there stay explicit instead of being flattened into fuel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnedParseTelemetry {
    pub slices: u64,
    pub total_audit: Audit,
    pub block_polls: u64,
    pub block_leaf_steps: u64,
    pub block_leaves_sealed: u64,
    pub block_leaves_started: u64,
    pub lexer_polls: u64,
    pub lexer_logical_bytes: u64,
    pub lexer_descriptor_entries: u64,
    pub lexer_excluded_source_bytes: u64,
    pub lexer_skipped_source_bytes: u64,
    pub lexer_source_chunk_loads: u64,
    pub lexer_source_chunk_bytes_copied: u64,
    pub inline_polls: u64,
    pub inline_allocated_bytes: u64,
    pub inline_copy_bytes: u64,
    pub inline_hash_bytes: u64,
    pub inline_local_page_reclaims: u64,
    pub inline_local_reclaimed_bytes: u64,
    pub inline_source_skipped_bytes: u64,
    pub inline_source_chunk_loads: u64,
    pub inline_source_chunk_bytes_copied: u64,
    pub drain_steps: u64,
    pub drain_index_steps: u64,
    pub drain_directory_reclaims: u64,
    pub drain_directory_reclaimed_bytes: u64,
    pub canonical_pages_adopted: u64,
    pub canonical_payload_bytes_adopted: u64,
    pub copied_record_pages: u64,
    pub intermediate_drop_events: u64,
    pub unmetered_local_allocation_sites: u64,
}

/// Identity-independent semantic summary of a completed supported-subset
/// document. It can be compared across an edited persistent root and a clean
/// reconstruction of the same final text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedParseSummary {
    pub source_identity: SourceRootIdentity,
    pub leaf_count: u64,
    pub span_count: u64,
    pub canonical_page_count: u64,
    pub canonical_payload_bytes: u64,
    /// Leaf-record pages plus the final manifest page.
    pub record_page_count: u64,
    pub semantic_digest: u64,
}

impl Default for OwnedParseSummary {
    fn default() -> Self {
        Self {
            source_identity: SourceRootIdentity(0),
            leaf_count: 0,
            span_count: 0,
            canonical_page_count: 0,
            canonical_payload_bytes: 0,
            record_page_count: 0,
            semantic_digest: 0,
        }
    }
}

impl OwnedParseSummary {
    /// Number of query-visible pages, excluding the activation anchor.
    #[must_use]
    pub const fn visible_pages(self) -> u64 {
        self.canonical_page_count + self.record_page_count
    }
}

/// One decoded leaf record. Container slots after `context_depth` are zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedLeafRecord {
    pub ordinal: u64,
    pub leaf_id: u64,
    pub input_identity: u64,
    pub physical_start: u64,
    pub physical_end: u64,
    pub span_count: u64,
    pub canonical_page_count: u64,
    pub canonical_payload_bytes: u64,
    pub inline_digest: u64,
    pub source_identity: u64,
    pub context_depth: u8,
    /// `(kind, marker, continuation_indent)`: kind 0 is quote, kind 1 is
    /// bullet. Only the first `context_depth` entries are meaningful.
    pub context: [[u8; 3]; MAX_BLOCK_CONTAINER_DEPTH],
}

/// Decoded final root manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedManifest {
    pub source_identity: u64,
    pub leaf_count: u64,
    pub span_count: u64,
    pub canonical_page_count: u64,
    pub canonical_payload_bytes: u64,
    pub record_page_count: u64,
    pub visible_pages: u64,
    pub semantic_digest: u64,
}

/// Reads one fixed leaf-record page.
#[must_use]
pub fn decode_leaf_record(payload: &[u8]) -> Option<OwnedLeafRecord> {
    if payload.len() != OWNED_LEAF_RECORD_BYTES
        || payload.get(..4)? != LEAF_MAGIC
        || read_u16(payload, 4)? != FORMAT_VERSION
    {
        return None;
    }
    let mut context = [[0_u8; 3]; MAX_BLOCK_CONTAINER_DEPTH];
    for (index, slot) in context.iter_mut().enumerate() {
        let start = 104 + index * 3;
        slot.copy_from_slice(payload.get(start..start + 3)?);
    }
    Some(OwnedLeafRecord {
        ordinal: read_u64(payload, 8)?,
        leaf_id: read_u64(payload, 16)?,
        input_identity: read_u64(payload, 24)?,
        physical_start: read_u64(payload, 32)?,
        physical_end: read_u64(payload, 40)?,
        span_count: read_u64(payload, 48)?,
        canonical_page_count: read_u64(payload, 56)?,
        canonical_payload_bytes: read_u64(payload, 64)?,
        inline_digest: read_u64(payload, 72)?,
        source_identity: read_u64(payload, 80)?,
        context_depth: *payload.get(88)?,
        context,
    })
}

/// Reads the fixed final root manifest.
#[must_use]
pub fn decode_manifest(payload: &[u8]) -> Option<OwnedManifest> {
    if payload.len() != OWNED_MANIFEST_BYTES
        || payload.get(..4)? != MANIFEST_MAGIC
        || read_u16(payload, 4)? != FORMAT_VERSION
    {
        return None;
    }
    Some(OwnedManifest {
        source_identity: read_u64(payload, 8)?,
        leaf_count: read_u64(payload, 16)?,
        span_count: read_u64(payload, 24)?,
        canonical_page_count: read_u64(payload, 32)?,
        canonical_payload_bytes: read_u64(payload, 40)?,
        record_page_count: read_u64(payload, 48)?,
        visible_pages: read_u64(payload, 56)?,
        semantic_digest: read_u64(payload, 64)?,
    })
}

/// Honest lower bound for state that would be synchronously dropped if a
/// superseded worker discards this local job. Arena pages are deliberately
/// excluded because scheduler reclaim owns them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedDropAudit {
    pub phase: OwnedParsePhase,
    pub known_allocations_lower_bound: u64,
    pub known_bytes_lower_bound: u64,
    pub unmetered_drop_sites: u64,
    pub arena_pages_excluded: u64,
    /// Exact count of sealed leaves not yet started by the source-order
    /// composition. Their persistent Arc tree is outside arena reclamation.
    pub unvisited_block_leaves: u64,
    pub gaps: &'static [OwnedParseGap],
}

/// Failure from the real composed parser.
#[derive(Debug)]
pub enum OwnedParseError {
    Block(BlockError),
    Lifetime(LifetimeError),
    AuditOverflow,
    AuditExceedsPermit { audit: Audit, limits: SliceLimits },
    ValueExceedsFormat(&'static str),
    Invariant(&'static str),
}

impl fmt::Display for OwnedParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Block(error) => write!(formatter, "block parse failed: {error}"),
            Self::Lifetime(error) => write!(formatter, "physical lifetime failed: {error}"),
            Self::AuditOverflow => formatter.write_str("owned parse audit overflowed"),
            Self::AuditExceedsPermit { .. } => {
                formatter.write_str("actual owned parse work exceeded its scheduler permit")
            }
            Self::ValueExceedsFormat(field) => {
                write!(formatter, "{field} exceeds the owned output format")
            }
            Self::Invariant(reason) => write!(formatter, "owned parse invariant failed: {reason}"),
        }
    }
}

impl std::error::Error for OwnedParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Block(error) => Some(error),
            Self::Lifetime(error) => Some(error),
            _ => None,
        }
    }
}

struct LeafMeta {
    ordinal: u64,
    leaf_id: u64,
    input_identity: u64,
    source_identity: u64,
    physical_start: u64,
    physical_end: u64,
    context_depth: u8,
    context: [[u8; 3]; MAX_BLOCK_CONTAINER_DEPTH],
}

impl LeafMeta {
    fn from_leaf(ordinal: u64, leaf: &BlockLeaf) -> Result<Self, OwnedParseError> {
        let mut context = [[0_u8; 3]; MAX_BLOCK_CONTAINER_DEPTH];
        for (target, frame) in context.iter_mut().zip(leaf.context.frames()) {
            *target = match *frame {
                BlockContainer::BlockQuote => [0, 0, 0],
                BlockContainer::BulletItem {
                    marker,
                    continuation_indent,
                } => [1, marker, continuation_indent],
            };
        }
        Ok(Self {
            ordinal,
            leaf_id: leaf.id.0,
            input_identity: leaf.input.identity().0,
            source_identity: leaf.input.source_identity().0,
            physical_start: as_u64(leaf.physical_start, "leaf physical start")?,
            physical_end: as_u64(leaf.physical_end, "leaf physical end")?,
            context_depth: u8::try_from(leaf.context.depth())
                .map_err(|_| OwnedParseError::ValueExceedsFormat("block context depth"))?,
            context,
        })
    }
}

struct LeafOutput {
    drain: InlineOutputPageDrain,
    record: [u8; OWNED_LEAF_RECORD_BYTES],
}

enum JobState {
    Start(Arc<PersistentSource>),
    #[cfg(feature = "crop-research")]
    StartCrop,
    Block(BlockJob),
    PrepareLeaves(BlockJob),
    Leaves(BlockLeaves),
    StartLexer {
        leaves: BlockLeaves,
        leaf: Arc<BlockLeaf>,
    },
    Lexer {
        leaves: BlockLeaves,
        leaf: Arc<BlockLeaf>,
        lexer: SharedLexer,
    },
    PrepareInline {
        leaves: BlockLeaves,
        leaf: Arc<BlockLeaf>,
        lexer: SharedLexer,
    },
    Inline {
        leaves: BlockLeaves,
        meta: LeafMeta,
        machine: InlineMachine,
    },
    PrepareOutput {
        leaves: BlockLeaves,
        meta: LeafMeta,
        machine: InlineMachine,
    },
    Drain {
        leaves: BlockLeaves,
        output: LeafOutput,
    },
    Adopt {
        leaves: BlockLeaves,
        output: LeafOutput,
        allocation: Box<[u8; ARENA_PAGE_BYTES]>,
        used_len: usize,
    },
    AppendLeafRecord {
        leaves: BlockLeaves,
        output: LeafOutput,
    },
    AppendManifest([u8; OWNED_MANIFEST_BYTES]),
    Ready,
    Failed,
}

/// One real latest-wins parse candidate. It holds no document-sized `Vec`;
/// every source/block/lexer/inline/page transfer advances through a scalar or
/// fixed-size state transition.
pub struct OwnedParseJob {
    token: ParseToken,
    arena_job: ArenaJobId,
    state: Option<JobState>,
    summary: OwnedParseSummary,
    telemetry: OwnedParseTelemetry,
    failure_audit: Audit,
    #[cfg(feature = "crop-research")]
    crop_lease: Option<Arc<CropSnapshotLease>>,
}

impl OwnedParseJob {
    #[must_use]
    pub fn new(token: ParseToken, arena_job: ArenaJobId, source: Arc<PersistentSource>) -> Self {
        Self {
            token,
            arena_job,
            state: Some(JobState::Start(source)),
            summary: OwnedParseSummary {
                semantic_digest: FNV_OFFSET,
                ..OwnedParseSummary::default()
            },
            telemetry: OwnedParseTelemetry::default(),
            failure_audit: Audit::ZERO,
            #[cfg(feature = "crop-research")]
            crop_lease: None,
        }
    }

    /// Starts the identical block/lexer/inline/arena composition over a Crop
    /// snapshot. One outer lease object owns the Rope; block leaves retain only
    /// weak bindings and copyable root-bound ranges.
    #[cfg(feature = "crop-research")]
    #[must_use]
    pub fn new_crop(
        token: ParseToken,
        arena_job: ArenaJobId,
        source: Arc<CropSnapshotLease>,
    ) -> Self {
        Self {
            token,
            arena_job,
            state: Some(JobState::StartCrop),
            summary: OwnedParseSummary {
                semantic_digest: FNV_OFFSET,
                ..OwnedParseSummary::default()
            },
            telemetry: OwnedParseTelemetry::default(),
            failure_audit: Audit::ZERO,
            crop_lease: Some(source),
        }
    }

    #[must_use]
    pub fn phase(&self) -> OwnedParsePhase {
        let Some(state) = self.state.as_ref() else {
            return OwnedParsePhase::Failed;
        };
        match state {
            JobState::Start(_) => OwnedParsePhase::StartBlock,
            #[cfg(feature = "crop-research")]
            JobState::StartCrop => OwnedParsePhase::StartBlock,
            JobState::Block(_) => OwnedParsePhase::Block,
            JobState::PrepareLeaves(_) => OwnedParsePhase::PrepareLeaves,
            JobState::Leaves(_) => OwnedParsePhase::Leaves,
            JobState::StartLexer { .. } => OwnedParsePhase::StartLexer,
            JobState::Lexer { .. } => OwnedParsePhase::Lexer,
            JobState::PrepareInline { .. } => OwnedParsePhase::PrepareInline,
            JobState::Inline { .. } => OwnedParsePhase::Inline,
            JobState::PrepareOutput { .. } => OwnedParsePhase::PrepareOutput,
            JobState::Drain { .. } => OwnedParsePhase::DrainInlinePage,
            JobState::Adopt { .. } => OwnedParsePhase::AdoptInlinePage,
            JobState::AppendLeafRecord { .. } => OwnedParsePhase::AppendLeafRecord,
            JobState::AppendManifest(_) => OwnedParsePhase::AppendManifest,
            JobState::Ready => OwnedParsePhase::Ready,
            JobState::Failed => OwnedParsePhase::Failed,
        }
    }

    #[must_use]
    pub const fn telemetry(&self) -> OwnedParseTelemetry {
        self.telemetry
    }

    #[must_use]
    pub fn summary(&self) -> Option<OwnedParseSummary> {
        matches!(self.state, Some(JobState::Ready)).then_some(self.summary)
    }

    #[must_use]
    pub fn visible_pages(&self) -> Option<u32> {
        let visible = self.summary()?.visible_pages();
        u32::try_from(visible).ok()
    }

    /// Audits synchronous non-arena ownership before cancellation/drop.
    #[must_use]
    pub fn drop_audit(&self, lifetime: &PhysicalLifetime) -> OwnedDropAudit {
        let (allocations, bytes, sites) = match self.state.as_ref() {
            None => (0, 0, 1),
            Some(state) => match state {
                JobState::Start(source) => {
                    let retained = source.buffer_retention();
                    (retained.unique_buffers, retained.retained_buffer_bytes, 1)
                }
                #[cfg(feature = "crop-research")]
                JobState::StartCrop => self
                    .crop_lease
                    .as_ref()
                    .map_or((0, 0, 1), |source| (1, source.len_bytes(), 1)),
                JobState::Block(block) | JobState::PrepareLeaves(block) => {
                    let receipt = block.receipt();
                    (
                        receipt
                            .retained_descriptors
                            .allocations
                            .saturating_add(receipt.retained_block_allocations),
                        receipt
                            .retained_descriptors
                            .total()
                            .saturating_add(receipt.retained_block_structure_bytes),
                        receipt.unmetered_upstream_allocation_sites,
                    )
                }
                JobState::StartLexer { leaf, .. }
                | JobState::Lexer { leaf, .. }
                | JobState::PrepareInline { leaf, .. } => {
                    let descriptors = leaf.input.retained_descriptor_bytes();
                    let source = leaf.input.retained_source_metrics();
                    (
                        descriptors
                            .allocations
                            .saturating_add(source.unique_buffers),
                        descriptors
                            .total()
                            .saturating_add(source.retained_buffer_bytes),
                        2,
                    )
                }
                JobState::Inline { machine, .. } | JobState::PrepareOutput { machine, .. } => {
                    let retained = machine.retention();
                    (retained.allocations, retained.bytes, 2)
                }
                JobState::Drain { output, .. } | JobState::AppendLeafRecord { output, .. } => {
                    let transferred = output.drain.metrics().page_transfers;
                    let total = output
                        .drain
                        .payload_bytes()
                        .div_ceil(INLINE_OUTPUT_PAGE_BYTES);
                    let remaining = total.saturating_sub(transferred);
                    (
                        remaining,
                        remaining.saturating_mul(INLINE_OUTPUT_PAGE_BYTES),
                        1,
                    )
                }
                JobState::Adopt { output, .. } => {
                    let transferred = output.drain.metrics().page_transfers;
                    let total = output
                        .drain
                        .payload_bytes()
                        .div_ceil(INLINE_OUTPUT_PAGE_BYTES);
                    let remaining = total.saturating_sub(transferred).saturating_add(1);
                    (
                        remaining,
                        remaining.saturating_mul(INLINE_OUTPUT_PAGE_BYTES),
                        1,
                    )
                }
                JobState::Leaves(_)
                | JobState::AppendManifest(_)
                | JobState::Ready
                | JobState::Failed => (0, 0, 1),
            },
        };
        OwnedDropAudit {
            phase: self.phase(),
            known_allocations_lower_bound: as_u64_saturating(allocations),
            known_bytes_lower_bound: as_u64_saturating(bytes),
            unmetered_drop_sites: as_u64_saturating(sites),
            arena_pages_excluded: as_u64_saturating(lifetime.active_job_pages()),
            unvisited_block_leaves: self
                .telemetry
                .block_leaves_sealed
                .saturating_sub(self.telemetry.block_leaves_started),
            gaps: OWNED_PARSE_GAPS,
        }
    }

    /// Returns the audit and then drops this bounded local state. Physical
    /// `Scheduler::run_reclaim_slice`.
    #[must_use]
    pub fn cancel(self, lifetime: &PhysicalLifetime) -> OwnedDropAudit {
        self.drop_audit(lifetime)
    }

    // Keeping the state transitions together makes it possible to audit that
    // every enum variant performs exactly one bounded action. Splitting this
    // match across phase-specific trait objects would add ownership surfaces
    // without changing its cooperative granularity.
    #[allow(clippy::too_many_lines)]
    fn tick(
        &mut self,
        permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<Audit, OwnedParseError> {
        let state = self
            .state
            .take()
            .ok_or(OwnedParseError::Invariant("missing state"))?;
        let (next, audit) = match state {
            JobState::Start(source) => {
                self.summary.source_identity = source.identity();
                let block = BlockJob::new(source);
                let mut audit = block_audit(&block.receipt());
                audit.transitions = audit.transitions.saturating_add(1);
                (JobState::Block(block), audit)
            }
            #[cfg(feature = "crop-research")]
            JobState::StartCrop => {
                let source = self
                    .crop_lease
                    .as_ref()
                    .ok_or(OwnedParseError::Invariant("Crop job lost source lease"))?;
                self.summary.source_identity = source.identity();
                let block = BlockJob::new_crop(source.clone());
                let mut audit = block_audit(&block.receipt());
                audit.transitions = audit.transitions.saturating_add(1);
                (JobState::Block(block), audit)
            }
            JobState::Block(mut block) => {
                let poll = block.poll(1);
                self.telemetry.block_polls = self.telemetry.block_polls.saturating_add(1);
                let audit = block_audit(&poll.receipt_delta);
                match poll.status {
                    BlockStatus::Pending => (JobState::Block(block), audit),
                    BlockStatus::Ready => (JobState::PrepareLeaves(block), audit),
                    BlockStatus::Failed => {
                        let error = block
                            .error()
                            .cloned()
                            .ok_or(OwnedParseError::Invariant("failed block job has no error"))?;
                        self.failure_audit = audit;
                        self.state = Some(JobState::Failed);
                        return Err(OwnedParseError::Block(error));
                    }
                }
            }
            JobState::PrepareLeaves(block) => {
                let output = block
                    .result()
                    .ok_or(OwnedParseError::Invariant("ready block job has no output"))?
                    .clone();
                self.telemetry.block_leaves_sealed = as_u64_saturating(output.len());
                let leaves = output.leaves();
                drop(output);
                drop(block);
                self.telemetry.intermediate_drop_events =
                    self.telemetry.intermediate_drop_events.saturating_add(2);
                (JobState::Leaves(leaves), transition_audit(4))
            }
            JobState::Leaves(mut leaves) => {
                self.telemetry.block_leaf_steps = self.telemetry.block_leaf_steps.saturating_add(1);
                match leaves.step() {
                    BlockLeafStep::Progress => (JobState::Leaves(leaves), transition_audit(1)),
                    BlockLeafStep::Leaf(leaf) => (
                        {
                            self.telemetry.block_leaves_started =
                                self.telemetry.block_leaves_started.saturating_add(1);
                            JobState::StartLexer { leaves, leaf }
                        },
                        Audit {
                            transitions: 1,
                            index_nodes: 1,
                            ..Audit::ZERO
                        },
                    ),
                    BlockLeafStep::Done => {
                        let manifest = encode_manifest(self.summary)?;
                        (JobState::AppendManifest(manifest), transition_audit(1))
                    }
                }
            }
            JobState::StartLexer { leaves, leaf } => {
                let lexer = SharedLexer::new(&leaf.input);
                self.telemetry.unmetered_local_allocation_sites = self
                    .telemetry
                    .unmetered_local_allocation_sites
                    .saturating_add(1);
                (
                    JobState::Lexer {
                        leaves,
                        leaf,
                        lexer,
                    },
                    transition_audit(1),
                )
            }
            JobState::Lexer {
                leaves,
                leaf,
                mut lexer,
            } => {
                let before = lexer.cursor_metrics();
                let poll = lexer.poll(1);
                let delta = cursor_delta(lexer.cursor_metrics(), before);
                self.telemetry.lexer_polls = self.telemetry.lexer_polls.saturating_add(1);
                add_cursor_telemetry(&mut self.telemetry, delta);
                let audit = lexer_audit(poll.work, delta);
                if poll.status == LexerStatus::Ready {
                    (
                        JobState::PrepareInline {
                            leaves,
                            leaf,
                            lexer,
                        },
                        audit,
                    )
                } else {
                    (
                        JobState::Lexer {
                            leaves,
                            leaf,
                            lexer,
                        },
                        audit,
                    )
                }
            }
            JobState::PrepareInline {
                leaves,
                leaf,
                lexer,
            } => {
                let consumers = lexer
                    .consumers()
                    .ok_or(OwnedParseError::Invariant("ready lexer has no consumers"))?;
                let ordinal = self.summary.leaf_count;
                let meta = LeafMeta::from_leaf(ordinal, &leaf)?;
                let machine = InlineMachine::new(consumers.inline);
                drop(consumers.table);
                drop(lexer);
                drop(leaf);
                self.telemetry.intermediate_drop_events =
                    self.telemetry.intermediate_drop_events.saturating_add(3);
                (
                    JobState::Inline {
                        leaves,
                        meta,
                        machine,
                    },
                    transition_audit(6),
                )
            }
            JobState::Inline {
                leaves,
                meta,
                mut machine,
            } => {
                let poll = machine.poll(InlineWork::uniform(1));
                self.telemetry.inline_polls = self.telemetry.inline_polls.saturating_add(1);
                add_inline_poll_telemetry(&mut self.telemetry, &poll.delta, poll.telemetry_delta);
                let audit = inline_audit(&poll.delta);
                if poll.status == InlineStatus::Ready {
                    (
                        JobState::PrepareOutput {
                            leaves,
                            meta,
                            machine,
                        },
                        audit,
                    )
                } else {
                    (
                        JobState::Inline {
                            leaves,
                            meta,
                            machine,
                        },
                        audit,
                    )
                }
            }
            JobState::PrepareOutput {
                leaves,
                meta,
                mut machine,
            } => {
                let output = machine.take_output().ok_or(OwnedParseError::Invariant(
                    "ready inline machine has no output",
                ))?;
                let drain = output.into_page_drain();
                let canonical_page_count = as_u64(
                    drain.payload_bytes().div_ceil(INLINE_OUTPUT_PAGE_BYTES),
                    "canonical page count",
                )?;
                let span_count = as_u64(drain.span_count(), "span count")?;
                let payload_bytes = as_u64(drain.payload_bytes(), "canonical payload bytes")?;
                let record = encode_leaf_record(
                    &meta,
                    span_count,
                    canonical_page_count,
                    payload_bytes,
                    drain.digest(),
                );
                fold_leaf_summary(
                    &mut self.summary,
                    &meta,
                    span_count,
                    canonical_page_count,
                    payload_bytes,
                    drain.digest(),
                );
                let total_work = drain.total_work();
                let total_telemetry = drain.total_telemetry();
                self.telemetry.inline_local_page_reclaims = self
                    .telemetry
                    .inline_local_page_reclaims
                    .saturating_add(as_u64_saturating(total_work.page_reclaims));
                self.telemetry.inline_local_reclaimed_bytes = self
                    .telemetry
                    .inline_local_reclaimed_bytes
                    .saturating_add(as_u64_saturating(total_work.reclaimed_bytes));
                self.telemetry.inline_source_skipped_bytes = self
                    .telemetry
                    .inline_source_skipped_bytes
                    .saturating_add(as_u64_saturating(total_telemetry.source_skipped_bytes));
                drop(machine);
                self.telemetry.intermediate_drop_events =
                    self.telemetry.intermediate_drop_events.saturating_add(1);
                (
                    JobState::Drain {
                        leaves,
                        output: LeafOutput { drain, record },
                    },
                    transition_audit(3),
                )
            }
            JobState::Drain { leaves, mut output } => {
                let before = output.drain.metrics();
                let step = output.drain.step();
                let delta = drain_delta(output.drain.metrics(), before);
                add_drain_telemetry(&mut self.telemetry, delta);
                let audit = Audit {
                    transitions: as_u64_saturating(delta.transitions),
                    index_nodes: as_u64_saturating(delta.output_index_steps),
                    ..Audit::ZERO
                };
                match step {
                    InlineOutputPageDrainStep::Page(page) => {
                        let used_len = page.used_len();
                        (
                            JobState::Adopt {
                                leaves,
                                output,
                                allocation: page.into_allocation(),
                                used_len,
                            },
                            audit,
                        )
                    }
                    InlineOutputPageDrainStep::Done => {
                        (JobState::AppendLeafRecord { leaves, output }, audit)
                    }
                }
            }
            JobState::Adopt {
                leaves,
                output,
                allocation,
                used_len,
            } => {
                let outcome = lifetime
                    .try_adopt_job_page_under_limits(
                        self.arena_job,
                        allocation,
                        used_len,
                        permit.limits(),
                        permit.limits(),
                    )
                    .map_err(OwnedParseError::Lifetime)?;
                match outcome {
                    OwnedPageAppend::Appended(receipt) => {
                        self.telemetry.canonical_pages_adopted =
                            self.telemetry.canonical_pages_adopted.saturating_add(1);
                        self.telemetry.canonical_payload_bytes_adopted = self
                            .telemetry
                            .canonical_payload_bytes_adopted
                            .saturating_add(as_u64_saturating(used_len));
                        (JobState::Drain { leaves, output }, receipt.audit())
                    }
                    OwnedPageAppend::Deferred(allocation) => {
                        self.state = Some(JobState::Adopt {
                            leaves,
                            output,
                            allocation,
                            used_len,
                        });
                        return Ok(Audit::ZERO);
                    }
                }
            }
            JobState::AppendLeafRecord { leaves, output } => {
                let receipt = lifetime
                    .try_append_job_page_under_limits(
                        self.arena_job,
                        &output.record,
                        permit.limits(),
                        permit.limits(),
                    )
                    .map_err(OwnedParseError::Lifetime)?;
                let Some(receipt) = receipt else {
                    self.state = Some(JobState::AppendLeafRecord { leaves, output });
                    return Ok(Audit::ZERO);
                };
                self.summary.record_page_count = self.summary.record_page_count.saturating_add(1);
                self.telemetry.copied_record_pages =
                    self.telemetry.copied_record_pages.saturating_add(1);
                drop(output);
                (JobState::Leaves(leaves), receipt.audit())
            }
            JobState::AppendManifest(manifest) => {
                let receipt = lifetime
                    .try_append_job_page_under_limits(
                        self.arena_job,
                        &manifest,
                        permit.limits(),
                        permit.limits(),
                    )
                    .map_err(OwnedParseError::Lifetime)?;
                let Some(receipt) = receipt else {
                    self.state = Some(JobState::AppendManifest(manifest));
                    return Ok(Audit::ZERO);
                };
                self.summary.record_page_count = self.summary.record_page_count.saturating_add(1);
                self.telemetry.copied_record_pages =
                    self.telemetry.copied_record_pages.saturating_add(1);
                (JobState::Ready, receipt.audit())
            }
            JobState::Ready => {
                self.state = Some(JobState::Ready);
                return Err(OwnedParseError::Invariant("ready job was polled"));
            }
            JobState::Failed => {
                self.state = Some(JobState::Failed);
                return Err(OwnedParseError::Invariant("failed job was polled"));
            }
        };
        self.state = Some(next);
        Ok(audit)
    }
}

impl MeasuredParseJob for OwnedParseJob {
    type Error = OwnedParseError;

    fn token(&self) -> ParseToken {
        self.token
    }

    fn arena_job(&self) -> ArenaJobId {
        self.arena_job
    }

    fn is_ready(&self) -> bool {
        matches!(self.state, Some(JobState::Ready))
    }

    fn poll_measured(
        &mut self,
        permit: ParseWorkPermit,
        lifetime: &mut PhysicalLifetime,
    ) -> Result<MeasuredParseReceipt, JobPollFailure<Self::Error>> {
        let audit = match self.tick(permit, lifetime) {
            Ok(audit) => audit,
            Err(error) => {
                self.state = Some(JobState::Failed);
                let audit = self.failure_audit;
                self.failure_audit = Audit::ZERO;
                return Err(JobPollFailure {
                    error,
                    receipt: MeasuredParseReceipt {
                        progressed_units: u64::from(audit != Audit::ZERO),
                        audit,
                        complete: false,
                    },
                });
            }
        };
        let progressed_units = u64::from(audit != Audit::ZERO);
        if !audit.fits(permit.limits()) {
            self.state = Some(JobState::Failed);
            return Err(JobPollFailure {
                error: OwnedParseError::AuditExceedsPermit {
                    audit,
                    limits: permit.limits(),
                },
                receipt: MeasuredParseReceipt {
                    progressed_units,
                    audit,
                    complete: false,
                },
            });
        }
        self.telemetry.slices = self.telemetry.slices.saturating_add(1);
        self.telemetry.total_audit =
            checked_add_audit(self.telemetry.total_audit, audit).ok_or(JobPollFailure {
                error: OwnedParseError::AuditOverflow,
                receipt: MeasuredParseReceipt {
                    progressed_units,
                    audit,
                    complete: false,
                },
            })?;
        Ok(MeasuredParseReceipt {
            progressed_units,
            audit,
            complete: self.is_ready(),
        })
    }
}

fn encode_leaf_record(
    meta: &LeafMeta,
    span_count: u64,
    canonical_page_count: u64,
    canonical_payload_bytes: u64,
    inline_digest: u64,
) -> [u8; OWNED_LEAF_RECORD_BYTES] {
    let mut payload = [0_u8; OWNED_LEAF_RECORD_BYTES];
    payload[..4].copy_from_slice(LEAF_MAGIC);
    payload[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    write_u64(&mut payload, 8, meta.ordinal);
    write_u64(&mut payload, 16, meta.leaf_id);
    write_u64(&mut payload, 24, meta.input_identity);
    write_u64(&mut payload, 32, meta.physical_start);
    write_u64(&mut payload, 40, meta.physical_end);
    write_u64(&mut payload, 48, span_count);
    write_u64(&mut payload, 56, canonical_page_count);
    write_u64(&mut payload, 64, canonical_payload_bytes);
    write_u64(&mut payload, 72, inline_digest);
    write_u64(&mut payload, 80, meta.source_identity);
    payload[88] = meta.context_depth;
    for (index, frame) in meta.context.iter().enumerate() {
        let start = 104 + index * 3;
        payload[start..start + 3].copy_from_slice(frame);
    }
    payload
}

fn encode_manifest(
    summary: OwnedParseSummary,
) -> Result<[u8; OWNED_MANIFEST_BYTES], OwnedParseError> {
    let mut payload = [0_u8; OWNED_MANIFEST_BYTES];
    payload[..4].copy_from_slice(MANIFEST_MAGIC);
    payload[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    write_u64(&mut payload, 8, summary.source_identity.0);
    write_u64(&mut payload, 16, summary.leaf_count);
    write_u64(&mut payload, 24, summary.span_count);
    write_u64(&mut payload, 32, summary.canonical_page_count);
    write_u64(&mut payload, 40, summary.canonical_payload_bytes);
    let record_pages = summary
        .record_page_count
        .checked_add(1)
        .ok_or(OwnedParseError::ValueExceedsFormat("record page count"))?;
    write_u64(&mut payload, 48, record_pages);
    write_u64(
        &mut payload,
        56,
        summary
            .canonical_page_count
            .checked_add(record_pages)
            .ok_or(OwnedParseError::ValueExceedsFormat("visible page count"))?,
    );
    write_u64(&mut payload, 64, summary.semantic_digest);
    Ok(payload)
}

fn fold_leaf_summary(
    summary: &mut OwnedParseSummary,
    meta: &LeafMeta,
    span_count: u64,
    canonical_page_count: u64,
    canonical_payload_bytes: u64,
    inline_digest: u64,
) {
    let mut digest = summary.semantic_digest;
    for value in [
        meta.ordinal,
        meta.physical_start,
        meta.physical_end,
        u64::from(meta.context_depth),
        span_count,
        canonical_page_count,
        canonical_payload_bytes,
        inline_digest,
    ] {
        digest = fold_u64(digest, value);
    }
    for frame in meta.context.iter().take(usize::from(meta.context_depth)) {
        for byte in frame {
            digest = fold_byte(digest, *byte);
        }
    }
    summary.semantic_digest = digest;
    summary.leaf_count = summary.leaf_count.saturating_add(1);
    summary.span_count = summary.span_count.saturating_add(span_count);
    summary.canonical_page_count = summary
        .canonical_page_count
        .saturating_add(canonical_page_count);
    summary.canonical_payload_bytes = summary
        .canonical_payload_bytes
        .saturating_add(canonical_payload_bytes);
}

fn fold_u64(mut digest: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        digest = fold_byte(digest, byte);
    }
    digest
}

const fn fold_byte(digest: u64, byte: u8) -> u64 {
    (digest ^ byte as u64).wrapping_mul(FNV_PRIME)
}

fn block_audit(receipt: &BlockWorkReceipt) -> Audit {
    #[cfg(feature = "crop-research")]
    let backend_source_copy = receipt.source_chunk_bytes_copied;
    #[cfg(not(feature = "crop-research"))]
    let backend_source_copy = 0;
    Audit {
        source_bytes: sum_usize(&[
            receipt.source_bytes_inspected,
            receipt.source_boundary_bytes_examined,
        ]),
        transitions: sum_usize(&[
            receipt.parser_transitions,
            receipt.source_piece_transitions,
            receipt.source_cursor_tree_nodes_descended,
            receipt.source_capture_buffer_handle_clones,
            receipt.source_capture_checkpoint_piece_runs,
            receipt.prefix_bytes_examined,
            receipt.prefix_frame_transitions,
            receipt.descriptor_operations,
            receipt.block_allocation_requests,
            receipt.leaves_sealed,
            receipt.output_pages_sealed,
            receipt.output_tree_nodes,
        ]),
        allocated_bytes: sum_usize(&[
            receipt.retained_descriptors.total(),
            receipt.retained_block_structure_bytes,
        ]),
        copied_bytes: sum_usize(&[
            receipt.source_fragment_payload_bytes_copied,
            receipt.prefix_bytes_copied,
            backend_source_copy,
        ]),
        hashed_bytes: 0,
        index_nodes: sum_usize(&[
            receipt.source_index_nodes_examined,
            receipt.source_capture_checkpoint_tree_nodes_examined,
            receipt.source_fragment_nodes_allocated,
            receipt.output_tree_nodes,
        ]),
        reclaimed_nodes: 0,
    }
}

fn lexer_audit(work: usize, delta: CursorMetrics) -> Audit {
    Audit {
        source_bytes: sum_usize(&[delta.logical_bytes, delta.excluded_source_bytes]),
        transitions: as_u64_saturating(work),
        allocated_bytes: 0,
        copied_bytes: as_u64_saturating(delta.source_chunk_bytes_copied),
        hashed_bytes: 0,
        index_nodes: as_u64_saturating(delta.source_seek_index_nodes),
        reclaimed_nodes: 0,
    }
}

fn inline_audit(work: &InlineWork) -> Audit {
    Audit {
        source_bytes: sum_usize(&[work.source_logical_bytes, work.source_excluded_bytes]),
        transitions: as_u64_saturating(work.transitions),
        allocated_bytes: as_u64_saturating(work.allocated_bytes),
        copied_bytes: as_u64_saturating(work.copy_bytes),
        hashed_bytes: as_u64_saturating(work.hash_bytes),
        index_nodes: sum_usize(&[
            work.lexical_tree_nodes,
            work.source_index_nodes,
            work.code_index_steps,
            work.delimiter_index_steps,
            work.output_index_steps,
        ]),
        // Inline scratch-page cleanup is local Rust ownership, not physical
        // arena reclamation, and stays in explicit telemetry above.
        reclaimed_nodes: 0,
    }
}

const fn transition_audit(transitions: u64) -> Audit {
    Audit {
        transitions,
        ..Audit::ZERO
    }
}

fn cursor_delta(after: CursorMetrics, before: CursorMetrics) -> CursorMetrics {
    CursorMetrics {
        operations: after.operations - before.operations,
        logical_bytes: after.logical_bytes - before.logical_bytes,
        descriptor_entries: after.descriptor_entries - before.descriptor_entries,
        excluded_source_bytes: after.excluded_source_bytes - before.excluded_source_bytes,
        skipped_source_bytes: after.skipped_source_bytes - before.skipped_source_bytes,
        source_seek_operations: after.source_seek_operations - before.source_seek_operations,
        source_seek_index_nodes: after.source_seek_index_nodes - before.source_seek_index_nodes,
        source_chunk_loads: after.source_chunk_loads - before.source_chunk_loads,
        source_chunk_bytes_copied: after.source_chunk_bytes_copied
            - before.source_chunk_bytes_copied,
        maximum_source_chunk_bytes_copied: after.maximum_source_chunk_bytes_copied,
    }
}

fn drain_delta(
    after: InlineOutputPageDrainMetrics,
    before: InlineOutputPageDrainMetrics,
) -> InlineOutputPageDrainMetrics {
    InlineOutputPageDrainMetrics {
        transitions: after.transitions - before.transitions,
        output_index_steps: after.output_index_steps - before.output_index_steps,
        page_transfers: after.page_transfers - before.page_transfers,
        transferred_payload_bytes: after.transferred_payload_bytes
            - before.transferred_payload_bytes,
        directory_reclaims: after.directory_reclaims - before.directory_reclaims,
        directory_reclaimed_bytes: after.directory_reclaimed_bytes
            - before.directory_reclaimed_bytes,
    }
}

fn add_cursor_telemetry(telemetry: &mut OwnedParseTelemetry, delta: CursorMetrics) {
    telemetry.lexer_logical_bytes = telemetry
        .lexer_logical_bytes
        .saturating_add(as_u64_saturating(delta.logical_bytes));
    telemetry.lexer_descriptor_entries = telemetry
        .lexer_descriptor_entries
        .saturating_add(as_u64_saturating(delta.descriptor_entries));
    telemetry.lexer_excluded_source_bytes = telemetry
        .lexer_excluded_source_bytes
        .saturating_add(as_u64_saturating(delta.excluded_source_bytes));
    telemetry.lexer_skipped_source_bytes = telemetry
        .lexer_skipped_source_bytes
        .saturating_add(as_u64_saturating(delta.skipped_source_bytes));
    telemetry.lexer_source_chunk_loads = telemetry
        .lexer_source_chunk_loads
        .saturating_add(as_u64_saturating(delta.source_chunk_loads));
    telemetry.lexer_source_chunk_bytes_copied = telemetry
        .lexer_source_chunk_bytes_copied
        .saturating_add(as_u64_saturating(delta.source_chunk_bytes_copied));
}

fn add_inline_poll_telemetry(
    telemetry: &mut OwnedParseTelemetry,
    work: &InlineWork,
    semantic: InlineTelemetry,
) {
    telemetry.inline_allocated_bytes = telemetry
        .inline_allocated_bytes
        .saturating_add(as_u64_saturating(work.allocated_bytes));
    telemetry.inline_copy_bytes = telemetry
        .inline_copy_bytes
        .saturating_add(as_u64_saturating(work.copy_bytes));
    telemetry.inline_hash_bytes = telemetry
        .inline_hash_bytes
        .saturating_add(as_u64_saturating(work.hash_bytes));
    telemetry.inline_source_chunk_loads = telemetry
        .inline_source_chunk_loads
        .saturating_add(as_u64_saturating(semantic.source_chunk_loads));
    telemetry.inline_source_chunk_bytes_copied = telemetry
        .inline_source_chunk_bytes_copied
        .saturating_add(as_u64_saturating(semantic.source_chunk_bytes_copied));
}

fn add_drain_telemetry(telemetry: &mut OwnedParseTelemetry, delta: InlineOutputPageDrainMetrics) {
    telemetry.drain_steps = telemetry
        .drain_steps
        .saturating_add(as_u64_saturating(delta.transitions));
    telemetry.drain_index_steps = telemetry
        .drain_index_steps
        .saturating_add(as_u64_saturating(delta.output_index_steps));
    telemetry.drain_directory_reclaims = telemetry
        .drain_directory_reclaims
        .saturating_add(as_u64_saturating(delta.directory_reclaims));
    telemetry.drain_directory_reclaimed_bytes = telemetry
        .drain_directory_reclaimed_bytes
        .saturating_add(as_u64_saturating(delta.directory_reclaimed_bytes));
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

fn sum_usize(values: &[usize]) -> u64 {
    values.iter().fold(0_u64, |total, value| {
        total.saturating_add(as_u64_saturating(*value))
    })
}

fn as_u64(value: usize, field: &'static str) -> Result<u64, OwnedParseError> {
    u64::try_from(value).map_err(|_| OwnedParseError::ValueExceedsFormat(field))
}

fn as_u64_saturating(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn write_u64(payload: &mut [u8], offset: usize, value: u64) {
    payload[offset..offset + size_of::<u64>()].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(payload: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        payload
            .get(offset..offset + size_of::<u16>())?
            .try_into()
            .ok()?,
    ))
}

fn read_u64(payload: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        payload
            .get(offset..offset + size_of::<u64>())?
            .try_into()
            .ok()?,
    ))
}

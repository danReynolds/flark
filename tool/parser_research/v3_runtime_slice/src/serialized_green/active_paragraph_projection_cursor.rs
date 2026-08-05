//! Read-only logical projection over the active unpublished Paragraph.
//!
//! This is deliberately a private mechanism proof.  It traverses the same
//! packed working prefix and active partial leaf owned by
//! `ResumableSerializedGreenBuild`; it does not manufacture a manifest, an
//! `Arc` snapshot, a Paragraph `String`, or a second projection tree.  The
//! CandidateWriter mints `ActorProjectionBinding` from its live
//! composer/source/Paragraph join. Definition cuts remain transaction-local:
//! bounded range replay feeds cooked-value storage before canonical removal,
//! so no copied arena ID, old projection root, or finite-lineage coordinate is
//! mistaken for durable semantic authority.

#![allow(
    clippy::large_enum_variant,
    clippy::match_same_arms,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use flark_comrak_value_block_core::{
    DirectReferenceDefinition, DirectReferenceLogicalPosition, DirectReferenceLogicalRange,
    DirectReferencePrefixDisposition, DirectReferencePrefixOutput, DirectReferencePrefixOutputAck,
    DirectReferencePrefixSource, DirectReferencePrefixTerminal, DirectReferencePrefixTerminalAck,
    DirectReferencePrefixTerminalOutput,
};

use crate::{
    CanonicalFragmentSurvivorSeed, FreshCoveragePermit, SourceProjectionSession,
    SourceProjectionSessionError, SourceProjectionSessionReceipt, SourceSnapshotDescriptor,
    SourceStore,
};

use super::*;

static NEXT_CURSOR_NONCE: AtomicU64 = AtomicU64::new(1);
/// A source-ordered AVL over a `u64` leaf count cannot legitimately approach
/// this depth.  Keeping a hard ceiling also makes a corrupt/cyclic route fail
/// closed instead of turning cursor construction into document-sized work.
const ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH: usize = 128;
const ACTIVE_PARAGRAPH_MAX_READY_BYTES: usize = 1;

/// Writer-side projection generation joined to one active Paragraph.
///
/// Production mints this value while it still owns the matching
/// `CanonicalFragmentProjectionOrigin`, composer high-water and pending-line
/// terminator. The constructor derives the opaque provisional Paragraph
/// coordinates rather than accepting its block/generation as caller scalars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActorProjectionBinding {
    source_root: SourceRootId,
    source_revision: SourceRevision,
    source_bytes: usize,
    build: ArenaBuildId,
    paragraph: BlockId,
    paragraph_generation: u64,
    projection_generation: u64,
    composer_high_water: SerializedMetric,
    barrier_generation: u64,
}

impl ActorProjectionBinding {
    pub(crate) const fn source(self) -> SourceSnapshotDescriptor {
        SourceSnapshotDescriptor {
            revision: self.source_revision,
            root: self.source_root,
            bytes: self.source_bytes,
        }
    }

    pub(crate) fn from_writer_join(
        source: SourceSnapshotDescriptor,
        paragraph: &ProvisionalParagraphEnter,
        projection_generation: u64,
        composer_high_water: SerializedMetric,
        barrier_generation: u64,
    ) -> Self {
        Self {
            source_root: source.root,
            source_revision: source.revision,
            source_bytes: source.bytes,
            build: paragraph.build,
            paragraph: paragraph.block,
            paragraph_generation: paragraph.generation,
            projection_generation,
            composer_high_water,
            barrier_generation,
        }
    }

    #[cfg(test)]
    fn mechanism_only(
        build: &ResumableSerializedGreenBuild,
        paragraph: &ProvisionalParagraphEnter,
        projection_generation: u64,
        composer_high_water: SerializedMetric,
        barrier_generation: u64,
    ) -> Self {
        Self {
            source_root: build.spec.source_root,
            source_revision: build.spec.source_revision,
            source_bytes: usize::try_from(build.spec.source_bytes)
                .expect("test source extent fits usize"),
            build: build.build,
            paragraph: paragraph.block,
            paragraph_generation: paragraph.generation,
            projection_generation,
            composer_high_water,
            barrier_generation,
        }
    }

    fn first_mismatch(
        self,
        identity: ActiveParagraphProjectionIdentity,
    ) -> Option<ActiveParagraphProjectionBindingMismatch> {
        if self.source_root != identity.source_root {
            return Some(ActiveParagraphProjectionBindingMismatch::SourceRoot);
        }
        if self.source_revision != identity.source_revision {
            return Some(ActiveParagraphProjectionBindingMismatch::SourceRevision);
        }
        if self.source_bytes != identity.source_bytes {
            return Some(ActiveParagraphProjectionBindingMismatch::SourceExtent);
        }
        if self.build != identity.build {
            return Some(ActiveParagraphProjectionBindingMismatch::Build);
        }
        if self.paragraph != identity.paragraph {
            return Some(ActiveParagraphProjectionBindingMismatch::Paragraph);
        }
        if self.paragraph_generation != identity.paragraph_generation {
            return Some(ActiveParagraphProjectionBindingMismatch::ParagraphGeneration);
        }
        if self.projection_generation != identity.projection_generation {
            return Some(ActiveParagraphProjectionBindingMismatch::ProjectionGeneration);
        }
        if self.composer_high_water != identity.composer_high_water {
            return Some(ActiveParagraphProjectionBindingMismatch::ComposerHighWater);
        }
        if self.barrier_generation != identity.barrier_generation {
            return Some(ActiveParagraphProjectionBindingMismatch::BarrierGeneration);
        }
        None
    }
}

/// Physical terminator still owned by the writer when the donor requests its
/// provisional canonical LF.  It is not part of packed Green yet and therefore
/// cannot be inferred by this cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StagedTerminatorKind {
    Lf,
    CrLf,
    LoneCr,
}

impl StagedTerminatorKind {
    const fn physical_metric(self) -> SerializedMetric {
        match self {
            Self::Lf | Self::LoneCr => SerializedMetric { bytes: 1, utf16: 1 },
            Self::CrLf => SerializedMetric { bytes: 2, utf16: 2 },
        }
    }

    const fn raw_codepoint_contribution(self) -> u8 {
        match self {
            Self::CrLf => 2,
            Self::Lf | Self::LoneCr => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StagedParagraphTerminator {
    owner_generation: u64,
    source_start: SerializedMetric,
    kind: StagedTerminatorKind,
}

impl StagedParagraphTerminator {
    pub(crate) const fn from_writer_join(
        owner_generation: u64,
        source_start: SerializedMetric,
        kind: StagedTerminatorKind,
    ) -> Self {
        Self {
            owner_generation,
            source_start,
            kind,
        }
    }

    pub(crate) const fn source_start(self) -> SerializedMetric {
        self.source_start
    }

    pub(crate) const fn kind(self) -> StagedTerminatorKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActiveParagraphProjectionIdentity {
    source_root: SourceRootId,
    source_revision: SourceRevision,
    source_bytes: usize,
    build: ArenaBuildId,
    paragraph: BlockId,
    paragraph_generation: u64,
    paragraph_event_ordinal: u64,
    paragraph_source_before: SerializedMetric,
    projection_generation: u64,
    composer_high_water: SerializedMetric,
    barrier_generation: u64,
    cursor_nonce: u64,
}

impl ActiveParagraphProjectionIdentity {
    pub(crate) const fn source(self) -> SourceSnapshotDescriptor {
        SourceSnapshotDescriptor {
            revision: self.source_revision,
            root: self.source_root,
            bytes: self.source_bytes,
        }
    }

    pub(crate) const fn cursor_nonce(self) -> u64 {
        self.cursor_nonce
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BuilderProjectionStamp {
    root: Option<ArenaId>,
    prefix: GreenSummary,
    partial: GreenSummary,
    partial_bytes: usize,
    partial_programs: usize,
    sealed_leaves: u64,
    sealed_events: u64,
    sealed_metric: SerializedMetric,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActiveParagraphProjectionReceipt {
    polls: u64,
    root_descents: usize,
    sequence_nodes_visited: usize,
    leaf_pages_decoded: usize,
    partial_leaf_decodes: usize,
    events_decoded: usize,
    coverage_runs_visited: usize,
    projection_program_pages_decoded: usize,
    projection_program_bytes_validated: usize,
    projection_pieces_decoded: usize,
    hidden_pieces_visited: usize,
    atomic_pieces_visited: usize,
    virtual_pieces_visited: usize,
    identity_source_bytes_read: usize,
    logical_bytes_yielded: usize,
    maximum_route_depth: usize,
    maximum_decoded_page_bytes: usize,
    maximum_program_scratch_bytes: usize,
    maximum_ready_byte_cache_bytes: usize,
    retained_source_bytes: usize,
    document_sized_event_vectors: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveParagraphProjectionError {
    Green(SerializedGreenError),
    SourceSession(SourceProjectionSessionError),
    WrongSource,
    NonSequentialSource,
    SourceOutOfBounds,
    StaleBinding,
    CrossedParagraph,
    CrossedProjection(ActiveParagraphProjectionBindingMismatch),
    CrossedCursor,
    CursorCancelled,
    CursorComplete,
    LogicalBoundaryOutOfBounds,
    LogicalBoundaryMetricMismatch,
    ProjectionTransactionRequiresSealedBarrier,
    UnsupportedCanonicalRewrite(&'static str),
    CapabilityNotReady,
    Overflow(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveParagraphProjectionBindingMismatch {
    SourceRoot,
    SourceRevision,
    SourceExtent,
    Build,
    Paragraph,
    ParagraphGeneration,
    ProjectionGeneration,
    ComposerHighWater,
    BarrierGeneration,
    StagedTerminator,
}

/// One dual-coordinate range.  Byte and UTF-16 coordinates always describe
/// the same logical or physical interval; keeping them together prevents a
/// caller from splicing independently sourced scalar coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProjectionMetricRange {
    start: SerializedMetric,
    end: SerializedMetric,
}

impl ProjectionMetricRange {
    fn new(
        start: SerializedMetric,
        end: SerializedMetric,
    ) -> Result<Self, ActiveParagraphProjectionError> {
        if start.bytes > end.bytes || start.utf16 > end.utf16 {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
        }
        Ok(Self { start, end })
    }
}

/// Ambiguity-preserving observation of one authenticated logical boundary.
///
/// An atomic replacement can have real logical boundaries with no unique
/// physical cut (for example, two projected spaces inside one physical tab).
/// Such positions remain typed ambiguity; the writer must never fabricate a
/// scalar source coordinate for them.  This is observation data only.  The
/// enclosing range capability carries the linear publication authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveParagraphProjectedBoundaryObservation {
    ExactSource {
        physical: SerializedMetric,
    },
    AtomicAmbiguity {
        physical: ProjectionMetricRange,
        logical: ProjectionMetricRange,
        transform: AtomicProjectionKind,
    },
    Virtual {
        physical_boundary: SerializedMetric,
        logical: ProjectionMetricRange,
        kind: VirtualProjectionKind,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActiveParagraphProjectionSeekReceipt {
    root_descents: usize,
    sequence_nodes_visited: usize,
    summary_nodes_skipped: usize,
    leaf_pages_decoded: usize,
    partial_leaf_decodes: usize,
    events_decoded: usize,
    events_inspected: usize,
    projection_program_pages_decoded: usize,
    projection_program_bytes_validated: usize,
    projection_pieces_decoded: usize,
    maximum_route_depth: usize,
    maximum_decoded_page_bytes: usize,
    maximum_program_scratch_bytes: usize,
    retained_source_bytes: usize,
    document_sized_event_vectors: usize,
}

impl ActiveParagraphProjectionSeekReceipt {
    fn followed_by(mut self, suffix: Self) -> Result<Self, ActiveParagraphProjectionError> {
        macro_rules! add {
            ($field:ident, $label:literal) => {
                self.$field = self
                    .$field
                    .checked_add(suffix.$field)
                    .ok_or(ActiveParagraphProjectionError::Overflow($label))?;
            };
        }
        add!(root_descents, "active Paragraph seek root descents");
        add!(
            sequence_nodes_visited,
            "active Paragraph seek sequence nodes"
        );
        add!(summary_nodes_skipped, "active Paragraph seek skipped nodes");
        add!(leaf_pages_decoded, "active Paragraph seek leaf pages");
        add!(partial_leaf_decodes, "active Paragraph seek partial leaves");
        add!(events_decoded, "active Paragraph seek decoded events");
        add!(events_inspected, "active Paragraph seek inspected events");
        add!(
            projection_program_pages_decoded,
            "active Paragraph seek Program pages"
        );
        add!(
            projection_program_bytes_validated,
            "active Paragraph seek Program bytes"
        );
        add!(
            projection_pieces_decoded,
            "active Paragraph seek Program pieces"
        );
        self.maximum_route_depth = self.maximum_route_depth.max(suffix.maximum_route_depth);
        self.maximum_decoded_page_bytes = self
            .maximum_decoded_page_bytes
            .max(suffix.maximum_decoded_page_bytes);
        self.maximum_program_scratch_bytes = self
            .maximum_program_scratch_bytes
            .max(suffix.maximum_program_scratch_bytes);
        add!(
            retained_source_bytes,
            "active Paragraph seek retained source"
        );
        add!(
            document_sized_event_vectors,
            "active Paragraph seek document vectors"
        );
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveParagraphProjectedBoundary {
    source: SourceSnapshotDescriptor,
    identity: ActiveParagraphProjectionIdentity,
    logical: SerializedMetric,
    affinity: GreenAffinity,
    observation: ActiveParagraphProjectedBoundaryObservation,
}

/// Linear capability for one parser-authenticated logical field range.
/// Both endpoints were resolved against the exact same builder stamp, cursor
/// nonce and immutable source descriptor.
pub(crate) struct ActiveParagraphProjectedRangeCapability {
    identity: ActiveParagraphProjectionIdentity,
    logical: DirectReferenceLogicalRange,
    start: ActiveParagraphProjectedBoundary,
    end: ActiveParagraphProjectedBoundary,
    receipt: ActiveParagraphProjectionSeekReceipt,
}

/// Non-authoritative execution hint bound to one exact projection cursor.
/// `physical_lower_bound_bytes` is an authenticated scalar cut at or before
/// every physical source byte the pass may request. For a logical cut inside
/// an atomic projection it is the atomic physical start; for virtual output it
/// is the virtual anchor. It must never be persisted as semantic provenance.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ActiveParagraphProjectionSourceStart {
    identity: ActiveParagraphProjectionIdentity,
    source: SourceSnapshotDescriptor,
    physical_lower_bound_bytes: usize,
}

/// Linear preparation for one range replay. The source-start hint cannot be
/// detached from or cloned independently of the exact range capability and
/// cursor nonce. `begin_range_replay` consumes this whole request.
pub(crate) struct ActiveParagraphRangeReplayRequest {
    capability: ActiveParagraphProjectedRangeCapability,
    source_start: ActiveParagraphProjectionSourceStart,
}

/// One non-cloneable occurrence ready for the writer's semantic transaction.
/// The parser acknowledgement is deliberately trapped in this bundle until
/// all field cuts have been resolved without guessing through atomic output.
pub(crate) struct ActiveParagraphProjectedReferenceOutput {
    identity: ActiveParagraphProjectionIdentity,
    definition: DirectReferenceDefinition,
    source: ActiveParagraphProjectedRangeCapability,
    label: ActiveParagraphProjectedRangeCapability,
    destination: ActiveParagraphProjectedRangeCapability,
    title: Option<ActiveParagraphProjectedRangeCapability>,
    ack: DirectReferencePrefixOutputAck<ActiveParagraphProjectionIdentity>,
}

/// Terminal parser decision plus the two logical cuts projected by the same
/// authenticated cursor that produced its occurrences. The terminal ack is
/// trapped until CandidateWriter has consumed the prefix/recognition caps and
/// completed the canonical replacement transaction.
pub(crate) struct ActiveParagraphProjectedReferenceTerminal {
    identity: ActiveParagraphProjectionIdentity,
    terminal: DirectReferencePrefixTerminal,
    reference_prefix: ActiveParagraphProjectedRangeCapability,
    recognition: ActiveParagraphProjectedRangeCapability,
    ack: DirectReferencePrefixTerminalAck<ActiveParagraphProjectionIdentity>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActiveParagraphRangeReplayReceipt {
    seek: ActiveParagraphProjectionSeekReceipt,
    stream: ActiveParagraphProjectionReceipt,
}

/// One range-local second traversal over the same immutable builder stamp.
/// It owns bounded route/page/Program scratch, never a projection tree or
/// source buffer.  Completion returns the consumed range capability so the
/// writer can join provenance only after its cooked-value blob is durable.
pub(crate) struct ActiveParagraphRangeReplayCursor {
    identity: ActiveParagraphProjectionIdentity,
    start: SerializedMetric,
    end: SerializedMetric,
    inner: Option<ActiveParagraphProjectionCursor>,
    capability: Option<ActiveParagraphProjectedRangeCapability>,
    seek_receipt: ActiveParagraphProjectionSeekReceipt,
    complete: bool,
    cancelled: bool,
}

/// The sole physical-source role for one parser-authenticated projection
/// pass. It owns the transaction's source session, so two replay cursors
/// cannot read that root concurrently. Dropping an unfinished pass cancels it
/// and releases the session's root rather than leaking an installed cursor.
pub(crate) struct ActiveParagraphProjectionSourcePass {
    session: Option<SourceProjectionSession>,
    identity: ActiveParagraphProjectionIdentity,
    closed: bool,
}

/// One range replay joined to its exact source pass. The linear range request
/// is consumed once to construct both halves; callers cannot detach its
/// authenticated lower bound and reuse it with another cursor or build.
pub(crate) struct ActiveParagraphRangeReplayPass {
    replay: Option<ActiveParagraphRangeReplayCursor>,
    source: Option<ActiveParagraphProjectionSourcePass>,
}

pub(crate) struct ActiveParagraphCompletedRangeReplay {
    capability: ActiveParagraphProjectedRangeCapability,
    receipt: ActiveParagraphRangeReplayReceipt,
}

/// Transaction-local terminal authority.  The old projection stays owned by
/// the still-live builder only until this seal is consumed by canonical
/// replacement.  Cooked destination/title blobs, not this seal or old Green,
/// are the durable semantic values.
#[derive(Debug)]
pub(crate) struct ActiveParagraphProjectionTransactionSeal {
    identity: ActiveParagraphProjectionIdentity,
    source: SourceSnapshotDescriptor,
    root: ArenaId,
    prefix: GreenSummary,
    covered_leaf_range: Range<u64>,
    paragraph_storage: ProvisionalParagraphStorage,
    paragraph_physical: ProjectionMetricRange,
    paragraph_logical: ProjectionMetricRange,
    staged_terminator: Option<StagedParagraphTerminator>,
}

/// Receipt from the seal-consuming canonical begin. The staged physical line
/// ending was writer-owned and absent from packed Green, so it is returned as
/// a typed continuation for CandidateWriter to account exactly once after the
/// Green rewrite starts. No projection owner or source lease escapes.
pub(crate) struct ActiveParagraphCanonicalRewriteBegin {
    identity: ActiveParagraphProjectionIdentity,
    green_physical: SerializedMetric,
    paragraph_physical: ProjectionMetricRange,
    staged_terminator: Option<StagedParagraphTerminator>,
    terminal: ActiveParagraphProjectedReferenceTerminal,
    rewrite: ActiveParagraphReferenceRewritePass,
}

/// One cooperative step of the seal-owned reference canonicalizer.  An offered
/// event has already been installed into the Green builder; the caller must
/// drive the ordinary builder acknowledgement before polling this pass again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveParagraphCanonicalRewriteProgress {
    Pending,
    EventOffered,
    SurvivingParagraphEnterOffered,
    Complete,
}

/// Lexical friend token: only the completed active-Paragraph rewrite can mint
/// the opaque composer seed for its surviving Paragraph cut.
pub(crate) struct ActiveParagraphCanonicalSurvivorMint(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveParagraphReferenceRewriteDisposition {
    ReferenceOnly,
    VisibleRemainder,
}

/// Linear canonical replay over the exact packed range authenticated by the
/// projection transaction seal.  It retains only one leaf decoder position and
/// emits at most one Green event per poll.  Physical split coordinates never
/// escape this type.
struct ActiveParagraphReferenceRewritePass {
    root: ArenaId,
    leaf_range: Range<u64>,
    next_leaf_index: u64,
    current_leaf: Option<ArenaId>,
    event_cursor: usize,
    next_program_ordinal: usize,
    expected_leaf_summary: Option<GreenSummary>,
    actual_leaf_summary: GreenSummary,
    first_event_offset: usize,
    paragraph: BlockId,
    disposition: ActiveParagraphReferenceRewriteDisposition,
    physical_position: SerializedMetric,
    physical_end: SerializedMetric,
    prefix_physical_end: SerializedMetric,
    saw_paragraph_enter: bool,
    survivor_emitted: bool,
    replacement_prefix_runs: u64,
    split_suffix_coverage: Option<FreshCoveragePermit>,
    pending_split_suffix: Option<DecodedSourceProjectionRun>,
    complete: bool,
}

/// Validated no-definition terminal. The provisional Paragraph token is
/// returned unchanged because no canonical Green mutation was authorized.
pub(crate) struct ActiveParagraphReferenceUnchanged {
    paragraph: ProvisionalParagraphEnter,
    terminal: ActiveParagraphProjectedReferenceTerminal,
    staged_terminator: Option<StagedParagraphTerminator>,
}

impl ActiveParagraphProjectedRangeCapability {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    pub(crate) fn logical_range(&self) -> &DirectReferenceLogicalRange {
        &self.logical
    }

    /// Transaction-bound source observation. It is not a durable navigation
    /// coordinate and must not survive semantic publication.
    pub(crate) const fn source(&self) -> SourceSnapshotDescriptor {
        self.start.source
    }

    pub(crate) fn prepare_replay(
        self,
    ) -> Result<ActiveParagraphRangeReplayRequest, ActiveParagraphProjectionError> {
        let source_start = replay_source_start(&self)?;
        Ok(ActiveParagraphRangeReplayRequest {
            capability: self,
            source_start,
        })
    }
}

impl ActiveParagraphProjectionSourceStart {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    pub(crate) const fn source(&self) -> SourceSnapshotDescriptor {
        self.source
    }

    pub(crate) const fn physical_lower_bound_bytes(&self) -> usize {
        self.physical_lower_bound_bytes
    }

    /// Consumes the parser-authenticated start plan into the session's sole
    /// live source role. The full identity remains trapped in the returned
    /// adapter and the session independently rechecks source and cursor nonce
    /// on every physical read.
    pub(crate) fn begin_source_pass(
        self,
        mut session: SourceProjectionSession,
    ) -> Result<ActiveParagraphProjectionSourcePass, ActiveParagraphProjectionError> {
        if self.source != self.identity.source()
            || session.descriptor() != self.source
            || session.cursor_nonce() != self.identity.cursor_nonce
        {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        session.begin_pass_at(
            self.source,
            self.identity.cursor_nonce,
            self.physical_lower_bound_bytes,
        )?;
        Ok(ActiveParagraphProjectionSourcePass {
            session: Some(session),
            identity: self.identity,
            closed: false,
        })
    }
}

impl ActiveParagraphRangeReplayRequest {
    pub(crate) const fn source_start(&self) -> &ActiveParagraphProjectionSourceStart {
        &self.source_start
    }

    /// Validates the exact lower-bound scalar supplied to the actor-owned
    /// source-pass adapter. A later cut is not interchangeable: the range may
    /// begin with atomic/virtual output before its first identity byte.
    pub(crate) fn validate_source_pass_start(
        &self,
        identity: ActiveParagraphProjectionIdentity,
        source: SourceSnapshotDescriptor,
        physical_lower_bound_bytes: usize,
    ) -> Result<(), ActiveParagraphProjectionError> {
        if identity != self.source_start.identity
            || source != self.source_start.source
            || physical_lower_bound_bytes != self.source_start.physical_lower_bound_bytes
        {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        Ok(())
    }
}

impl ActiveParagraphProjectedReferenceOutput {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn into_parts(
        self,
    ) -> (
        DirectReferenceDefinition,
        ActiveParagraphProjectedRangeCapability,
        ActiveParagraphProjectedRangeCapability,
        ActiveParagraphProjectedRangeCapability,
        Option<ActiveParagraphProjectedRangeCapability>,
        DirectReferencePrefixOutputAck<ActiveParagraphProjectionIdentity>,
    ) {
        (
            self.definition,
            self.source,
            self.label,
            self.destination,
            self.title,
            self.ack,
        )
    }
}

impl ActiveParagraphProjectedReferenceTerminal {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    pub(crate) const fn terminal(&self) -> &DirectReferencePrefixTerminal {
        &self.terminal
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        DirectReferencePrefixTerminal,
        ActiveParagraphProjectedRangeCapability,
        ActiveParagraphProjectedRangeCapability,
        DirectReferencePrefixTerminalAck<ActiveParagraphProjectionIdentity>,
    ) {
        (
            self.terminal,
            self.reference_prefix,
            self.recognition,
            self.ack,
        )
    }
}

impl ActiveParagraphCompletedRangeReplay {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.capability.identity
    }

    pub(crate) const fn receipt(&self) -> ActiveParagraphRangeReplayReceipt {
        self.receipt
    }

    pub(crate) fn into_capability(self) -> ActiveParagraphProjectedRangeCapability {
        self.capability
    }
}

impl ActiveParagraphProjectionTransactionSeal {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    pub(crate) const fn source(&self) -> SourceSnapshotDescriptor {
        self.source
    }

    pub(crate) fn validates_range(&self, range: &ActiveParagraphProjectedRangeCapability) -> bool {
        range.identity == self.identity
            && range.start.source == self.source
            && range.end.source == self.source
            && metric_at_or_before(self.paragraph_logical.start, range.start.logical)
            && metric_at_or_before(range.end.logical, self.paragraph_logical.end)
    }

    fn validate_reference_terminal(
        &self,
        terminal: &ActiveParagraphProjectedReferenceTerminal,
        disposition: DirectReferencePrefixDisposition,
    ) -> Result<(), ActiveParagraphProjectionError> {
        if terminal.identity != self.identity
            || terminal.terminal.disposition != disposition
            || !self.validates_range(&terminal.reference_prefix)
            || !self.validates_range(&terminal.recognition)
            || terminal.reference_prefix.logical != terminal.terminal.logical_reference_prefix
            || terminal.recognition.logical != terminal.terminal.logical_recognition
        {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        Ok(())
    }

    fn validate_retired_source_session(
        &self,
        receipt: SourceProjectionSessionReceipt,
    ) -> Result<(), ActiveParagraphProjectionError> {
        if receipt.descriptor != self.source
            || receipt.cursor_nonce != self.identity.cursor_nonce
            || receipt.passes_cancelled != 0
            || receipt.passes_started != receipt.passes_finished
            || receipt.maximum_live_cursor_roles > 1
        {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        Ok(())
    }

    fn validate_current_builder(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        paragraph: &ProvisionalParagraphEnter,
    ) -> Result<SerializedMetric, ActiveParagraphProjectionError> {
        build.ensure_session(session)?;
        if let Some(mismatch) = binding.first_mismatch(self.identity) {
            return Err(ActiveParagraphProjectionError::CrossedProjection(mismatch));
        }
        let active = build
            .active_provisional_paragraph
            .ok_or(ActiveParagraphProjectionError::StaleBinding)?;
        let prefix = build
            .working_prefix
            .as_ref()
            .ok_or(ActiveParagraphProjectionError::StaleBinding)?;
        let root = session.owner_id(&prefix.owner)?;
        let ProvisionalParagraphStorage::Sealed { leaf_index, .. } = active.storage else {
            return Err(ActiveParagraphProjectionError::ProjectionTransactionRequiresSealedBarrier);
        };
        if paragraph.build != self.identity.build
            || paragraph.block != self.identity.paragraph
            || paragraph.generation != self.identity.paragraph_generation
            || paragraph.event_ordinal != self.identity.paragraph_event_ordinal
            || paragraph.source_before != self.identity.paragraph_source_before
            || active.build != paragraph.build
            || active.block != paragraph.block
            || active.generation != paragraph.generation
            || active.event_ordinal != paragraph.event_ordinal
            || active.source_before != paragraph.source_before
            || active.storage != self.paragraph_storage
            || root != self.root
            || prefix.summary != self.prefix
            || build.leaf.summary != GreenSummary::default()
            || build.leaf.bytes.len() != LEAF_HEADER_BYTES
            || !build.leaf.programs.is_empty()
            || self.covered_leaf_range != (leaf_index..build.sealed_leaves)
            || self.paragraph_physical.start != self.identity.paragraph_source_before
            || self.paragraph_physical.end != self.identity.composer_high_water
        {
            return Err(ActiveParagraphProjectionError::StaleBinding);
        }

        let green_end = build.sealed_metric.checked_add(build.leaf.summary.metric)?;
        match self.staged_terminator {
            Some(terminator) => {
                let staged_end = terminator
                    .source_start
                    .checked_add(terminator.kind.physical_metric())?;
                if terminator.owner_generation != self.identity.projection_generation
                    || terminator.source_start != green_end
                    || staged_end != self.paragraph_physical.end
                {
                    return Err(ActiveParagraphProjectionError::CrossedProjection(
                        ActiveParagraphProjectionBindingMismatch::StagedTerminator,
                    ));
                }
            }
            None if green_end != self.paragraph_physical.end => {
                return Err(ActiveParagraphProjectionError::CrossedProjection(
                    ActiveParagraphProjectionBindingMismatch::StagedTerminator,
                ));
            }
            None => {}
        }
        green_end
            .checked_sub(self.identity.paragraph_source_before)
            .map_err(Into::into)
    }

    fn begin_reference_rewrite(
        self,
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        paragraph: ProvisionalParagraphEnter,
        terminal: ActiveParagraphProjectedReferenceTerminal,
        retired_source_session: SourceProjectionSessionReceipt,
        replacement_block: BlockId,
        replacement_kind: GreenKind,
        split_suffix_coverage: Option<FreshCoveragePermit>,
        remove_paragraph: bool,
    ) -> Result<ActiveParagraphCanonicalRewriteBegin, ActiveParagraphProjectionError> {
        let expected_disposition = if remove_paragraph {
            DirectReferencePrefixDisposition::ReferenceOnly
        } else {
            DirectReferencePrefixDisposition::VisibleRemainder
        };
        self.validate_reference_terminal(&terminal, expected_disposition)?;
        self.validate_retired_source_session(retired_source_session)?;
        let green_physical = self.validate_current_builder(build, session, binding, &paragraph)?;
        let physical_end = self
            .identity
            .paragraph_source_before
            .checked_add(green_physical)?;
        let prefix_physical_end = if remove_paragraph {
            physical_end
        } else {
            match terminal.reference_prefix.end.observation {
                ActiveParagraphProjectedBoundaryObservation::ExactSource { physical } => physical,
                ActiveParagraphProjectedBoundaryObservation::AtomicAmbiguity { .. }
                | ActiveParagraphProjectedBoundaryObservation::Virtual { .. } => {
                    return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                        "visible reference boundary requires an exact physical cut",
                    ));
                }
            }
        };
        if !metric_at_or_before(self.identity.paragraph_source_before, prefix_physical_end)
            || !metric_at_or_before(prefix_physical_end, physical_end)
            || (!remove_paragraph
                && (prefix_physical_end == self.identity.paragraph_source_before
                    || prefix_physical_end == physical_end))
        {
            return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                "visible reference boundary does not partition packed Paragraph source",
            ));
        }
        let first_event_offset = match self.paragraph_storage {
            ProvisionalParagraphStorage::Sealed { byte_offset, .. } => usize::from(byte_offset),
            ProvisionalParagraphStorage::Partial { .. } => {
                return Err(
                    ActiveParagraphProjectionError::ProjectionTransactionRequiresSealedBarrier,
                );
            }
        };
        let rewrite = ActiveParagraphReferenceRewritePass {
            root: self.root,
            leaf_range: self.covered_leaf_range.clone(),
            next_leaf_index: self.covered_leaf_range.start,
            current_leaf: None,
            event_cursor: 0,
            next_program_ordinal: 0,
            expected_leaf_summary: None,
            actual_leaf_summary: GreenSummary::default(),
            first_event_offset,
            paragraph: paragraph.block,
            disposition: if remove_paragraph {
                ActiveParagraphReferenceRewriteDisposition::ReferenceOnly
            } else {
                ActiveParagraphReferenceRewriteDisposition::VisibleRemainder
            },
            physical_position: self.identity.paragraph_source_before,
            physical_end,
            prefix_physical_end,
            saw_paragraph_enter: false,
            survivor_emitted: false,
            replacement_prefix_runs: 0,
            split_suffix_coverage,
            pending_split_suffix: None,
            complete: false,
        };
        if remove_paragraph {
            build.begin_canonical_fragment_removal(
                session,
                paragraph,
                replacement_block,
                replacement_kind,
                green_physical,
            )?;
        } else {
            build.begin_canonical_fragment_replacement(
                session,
                paragraph,
                replacement_block,
                replacement_kind,
                green_physical,
            )?;
        }
        Ok(ActiveParagraphCanonicalRewriteBegin {
            identity: self.identity,
            green_physical,
            paragraph_physical: self.paragraph_physical,
            staged_terminator: self.staged_terminator,
            terminal,
            rewrite,
        })
    }

    pub(crate) fn begin_reference_visible_remainder(
        self,
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        paragraph: ProvisionalParagraphEnter,
        terminal: ActiveParagraphProjectedReferenceTerminal,
        retired_source_session: SourceProjectionSessionReceipt,
        replacement_block: BlockId,
        replacement_kind: GreenKind,
        split_suffix_coverage: FreshCoveragePermit,
    ) -> Result<ActiveParagraphCanonicalRewriteBegin, ActiveParagraphProjectionError> {
        if split_suffix_coverage.build_id() != self.identity.build {
            return Err(ActiveParagraphProjectionError::CrossedProjection(
                ActiveParagraphProjectionBindingMismatch::Build,
            ));
        }
        self.begin_reference_rewrite(
            build,
            session,
            binding,
            paragraph,
            terminal,
            retired_source_session,
            replacement_block,
            replacement_kind,
            Some(split_suffix_coverage),
            false,
        )
    }

    pub(crate) fn begin_reference_only_removal(
        self,
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        paragraph: ProvisionalParagraphEnter,
        terminal: ActiveParagraphProjectedReferenceTerminal,
        retired_source_session: SourceProjectionSessionReceipt,
        parent_block: BlockId,
        parent_kind: GreenKind,
    ) -> Result<ActiveParagraphCanonicalRewriteBegin, ActiveParagraphProjectionError> {
        self.begin_reference_rewrite(
            build,
            session,
            binding,
            paragraph,
            terminal,
            retired_source_session,
            parent_block,
            parent_kind,
            None,
            true,
        )
    }

    pub(crate) fn validate_reference_unchanged(
        self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        paragraph: ProvisionalParagraphEnter,
        terminal: ActiveParagraphProjectedReferenceTerminal,
        retired_source_session: SourceProjectionSessionReceipt,
    ) -> Result<ActiveParagraphReferenceUnchanged, ActiveParagraphProjectionError> {
        self.validate_reference_terminal(
            &terminal,
            DirectReferencePrefixDisposition::NoDefinitions,
        )?;
        self.validate_retired_source_session(retired_source_session)?;
        self.validate_current_builder(build, session, binding, &paragraph)?;
        Ok(ActiveParagraphReferenceUnchanged {
            paragraph,
            terminal,
            staged_terminator: self.staged_terminator,
        })
    }
}

impl ActiveParagraphCanonicalRewriteBegin {
    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    pub(crate) const fn green_physical(&self) -> SerializedMetric {
        self.green_physical
    }

    pub(crate) const fn paragraph_physical(&self) -> (SerializedMetric, SerializedMetric) {
        (self.paragraph_physical.start, self.paragraph_physical.end)
    }

    pub(crate) const fn staged_terminator(&self) -> Option<StagedParagraphTerminator> {
        self.staged_terminator
    }

    /// Drives the authenticated packed rewrite directly into the active Green
    /// builder. No event or physical partition is returned to the caller. The
    /// current production slice accepts every mapping in the removed prefix,
    /// and `Identity`/`None` in a visible suffix. A boundary through an atomic
    /// or Program mapping fails closed until the general projection splitter is
    /// installed.
    pub(crate) fn poll_reference_rewrite(
        &mut self,
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ActiveParagraphCanonicalRewriteProgress, ActiveParagraphProjectionError> {
        build.ensure_session(session)?;
        let action = self.rewrite.poll(session.arena())?;
        match action {
            ActiveParagraphReferenceRewriteAction::Pending => {
                Ok(ActiveParagraphCanonicalRewriteProgress::Pending)
            }
            ActiveParagraphReferenceRewriteAction::Event(event) => {
                build.offer_canonical_fragment_event(session, event)?;
                Ok(ActiveParagraphCanonicalRewriteProgress::EventOffered)
            }
            ActiveParagraphReferenceRewriteAction::SurvivingParagraphEnter => {
                build.offer_canonical_fragment_surviving_paragraph_enter(session)?;
                Ok(ActiveParagraphCanonicalRewriteProgress::SurvivingParagraphEnterOffered)
            }
            ActiveParagraphReferenceRewriteAction::Complete => {
                Ok(ActiveParagraphCanonicalRewriteProgress::Complete)
            }
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> Result<
        (
            ActiveParagraphProjectedReferenceTerminal,
            Option<StagedParagraphTerminator>,
            Option<CanonicalFragmentSurvivorSeed>,
        ),
        ActiveParagraphProjectionError,
    > {
        if !self.rewrite.complete {
            return Err(ActiveParagraphProjectionError::CapabilityNotReady);
        }
        let survivor = match self.rewrite.disposition {
            ActiveParagraphReferenceRewriteDisposition::ReferenceOnly => None,
            ActiveParagraphReferenceRewriteDisposition::VisibleRemainder => {
                if !self.rewrite.survivor_emitted || self.rewrite.replacement_prefix_runs == 0 {
                    return Err(ActiveParagraphProjectionError::CapabilityNotReady);
                }
                Some(
                    CanonicalFragmentSurvivorSeed::from_active_paragraph_rewrite(
                        ActiveParagraphCanonicalSurvivorMint(()),
                        self.identity.build,
                        self.identity.projection_generation,
                        self.rewrite.prefix_physical_end,
                        self.rewrite.replacement_prefix_runs,
                    ),
                )
            }
        };
        Ok((self.terminal, self.staged_terminator, survivor))
    }
}

enum ActiveParagraphReferenceRewriteAction {
    Pending,
    Event(GreenEvent),
    SurvivingParagraphEnter,
    Complete,
}

impl ActiveParagraphReferenceRewritePass {
    fn poll(
        &mut self,
        arena: &PageArena,
    ) -> Result<ActiveParagraphReferenceRewriteAction, ActiveParagraphProjectionError> {
        if self.complete {
            return Ok(ActiveParagraphReferenceRewriteAction::Complete);
        }

        if self.disposition == ActiveParagraphReferenceRewriteDisposition::VisibleRemainder
            && self.saw_paragraph_enter
            && !self.survivor_emitted
            && self.physical_position == self.prefix_physical_end
        {
            self.survivor_emitted = true;
            return Ok(ActiveParagraphReferenceRewriteAction::SurvivingParagraphEnter);
        }

        if self.survivor_emitted {
            if let Some(run) = self.pending_split_suffix.take() {
                let physical_end = self.physical_position.checked_add(run.metric)?;
                if self.physical_position != self.prefix_physical_end
                    || !metric_at_or_before(physical_end, self.physical_end)
                {
                    return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                        "visible split suffix crossed its authenticated source boundary",
                    ));
                }
                self.physical_position = physical_end;
                return Ok(ActiveParagraphReferenceRewriteAction::Event(
                    retain_identity_reference_suffix_run(run, self.paragraph)?,
                ));
            }
        }

        if !self.prepare_current_leaf(arena)? {
            if !self.saw_paragraph_enter
                || self.physical_position != self.physical_end
                || (self.disposition
                    == ActiveParagraphReferenceRewriteDisposition::VisibleRemainder
                    && !self.survivor_emitted)
            {
                return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                    "reference canonical replay ended on the wrong boundary",
                ));
            }
            self.complete = true;
            return Ok(ActiveParagraphReferenceRewriteAction::Complete);
        }

        let leaf = self
            .current_leaf
            .ok_or(ActiveParagraphProjectionError::CapabilityNotReady)?;
        let payload = arena.payload(leaf).map_err(SerializedGreenError::from)?;
        if self.event_cursor >= payload.len() {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("reference rewrite cursor escaped its leaf"),
            ));
        }
        let mut decoder = Decoder {
            bytes: payload,
            cursor: self.event_cursor,
        };
        let event = decode_event(&mut decoder, arena, leaf, &mut self.next_program_ordinal)?;
        self.event_cursor = decoder.cursor;
        self.actual_leaf_summary = self
            .actual_leaf_summary
            .followed_by(GreenSummary::decoded_event(&event))?;
        self.finish_current_leaf_if_complete(arena, leaf, payload.len())?;

        if !self.saw_paragraph_enter {
            match event {
                DecodedGreenEventKind::Enter { block, kind, facts }
                    if block == self.paragraph
                        && kind == GreenKind::PARAGRAPH
                        && facts.fields.is_empty() =>
                {
                    self.saw_paragraph_enter = true;
                    return Ok(ActiveParagraphReferenceRewriteAction::Pending);
                }
                _ => {
                    return Err(ActiveParagraphProjectionError::Green(
                        SerializedGreenError::Corrupt(
                            "reference rewrite did not begin at its Paragraph Enter",
                        ),
                    ));
                }
            }
        }

        let DecodedGreenEventKind::Coverage(run) = event else {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt(
                    "active reference Paragraph contains an unexpected structural event",
                ),
            ));
        };
        let physical_start = self.physical_position;
        let physical_end = physical_start.checked_add(run.metric)?;
        if !metric_at_or_before(physical_end, self.physical_end) {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("reference rewrite exceeded Paragraph source"),
            ));
        }

        let prefix = match self.disposition {
            ActiveParagraphReferenceRewriteDisposition::ReferenceOnly => true,
            ActiveParagraphReferenceRewriteDisposition::VisibleRemainder => {
                if metric_at_or_before(physical_end, self.prefix_physical_end) {
                    true
                } else if metric_at_or_before(self.prefix_physical_end, physical_start) {
                    if !self.survivor_emitted || physical_start != self.prefix_physical_end {
                        return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                            "visible reference boundary crossed a physical-only run",
                        ));
                    }
                    false
                } else {
                    if !matches!(
                        &run.logical_contribution,
                        DecodedLogicalContribution::Identity | DecodedLogicalContribution::None
                    ) {
                        return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                            "visible reference boundary splits a non-identity projection run",
                        ));
                    }
                    let permit = self.split_suffix_coverage.take().ok_or(
                        ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                            "visible identity split lacks its fresh suffix coverage",
                        ),
                    )?;
                    let prefix_metric = self.prefix_physical_end.checked_sub(physical_start)?;
                    let suffix_metric = physical_end.checked_sub(self.prefix_physical_end)?;
                    if prefix_metric.is_zero()
                        || prefix_metric.is_partially_zero()
                        || suffix_metric.is_zero()
                        || suffix_metric.is_partially_zero()
                    {
                        return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                            "visible identity split crosses a source scalar",
                        ));
                    }
                    let prefix_run = DecodedSourceProjectionRun {
                        id: run.id,
                        metric: prefix_metric,
                        owner_relative_depth: run.owner_relative_depth,
                        part: run.part,
                        logical_contribution: run.logical_contribution.clone(),
                        projection_reset_after: false,
                    };
                    self.pending_split_suffix = Some(DecodedSourceProjectionRun {
                        id: permit.id(),
                        metric: suffix_metric,
                        owner_relative_depth: run.owner_relative_depth,
                        part: run.part,
                        logical_contribution: run.logical_contribution,
                        projection_reset_after: run.projection_reset_after,
                    });
                    self.physical_position = self.prefix_physical_end;
                    self.replacement_prefix_runs = self
                        .replacement_prefix_runs
                        .checked_add(1)
                        .ok_or(ActiveParagraphProjectionError::Overflow(
                            "reference replacement prefix runs",
                        ))?;
                    return Ok(ActiveParagraphReferenceRewriteAction::Event(
                        reclassify_reference_prefix_run(prefix_run)?,
                    ));
                }
            }
        };
        self.physical_position = physical_end;
        let event = if prefix {
            self.replacement_prefix_runs = self.replacement_prefix_runs.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("reference replacement prefix runs"),
            )?;
            reclassify_reference_prefix_run(run)?
        } else {
            retain_identity_reference_suffix_run(run, self.paragraph)?
        };
        Ok(ActiveParagraphReferenceRewriteAction::Event(event))
    }

    fn prepare_current_leaf(
        &mut self,
        arena: &PageArena,
    ) -> Result<bool, ActiveParagraphProjectionError> {
        if self.current_leaf.is_some() {
            return Ok(true);
        }
        if self.next_leaf_index >= self.leaf_range.end {
            return Ok(false);
        }
        let (leaf, _) = locate_green_leaf_with_prefix(arena, self.root, self.next_leaf_index)?;
        let payload = arena.payload(leaf).map_err(SerializedGreenError::from)?;
        let expected = decode_summary(payload, LEAF_TAG)?;
        let start = if self.next_leaf_index == self.leaf_range.start {
            self.first_event_offset
        } else {
            LEAF_HEADER_BYTES
        };
        if start < LEAF_HEADER_BYTES || start >= payload.len() {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("reference rewrite leaf start is invalid"),
            ));
        }

        // Authenticate the untouched prefix of the first page without retaining
        // any decoded event. This is one arena-page-bounded setup step.
        let mut decoder = Decoder::new(payload);
        decoder.cursor = LEAF_HEADER_BYTES;
        let mut next_program_ordinal = 0;
        let mut actual = GreenSummary::default();
        while decoder.cursor < start {
            let event = decode_event(&mut decoder, arena, leaf, &mut next_program_ordinal)?;
            actual = actual.followed_by(GreenSummary::decoded_event(&event))?;
        }
        if decoder.cursor != start {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("reference rewrite begins inside an event"),
            ));
        }
        self.current_leaf = Some(leaf);
        self.event_cursor = start;
        self.next_program_ordinal = next_program_ordinal;
        self.expected_leaf_summary = Some(expected);
        self.actual_leaf_summary = actual;
        Ok(true)
    }

    fn finish_current_leaf_if_complete(
        &mut self,
        arena: &PageArena,
        leaf: ArenaId,
        payload_len: usize,
    ) -> Result<(), ActiveParagraphProjectionError> {
        if self.event_cursor != payload_len {
            return Ok(());
        }
        let mut actual = self.actual_leaf_summary;
        actual.leaves = 1;
        actual.height = 1;
        let expected = self
            .expected_leaf_summary
            .take()
            .ok_or(ActiveParagraphProjectionError::CapabilityNotReady)?;
        if actual != expected
            || self.next_program_ordinal
                != arena
                    .packed_child_count(leaf)
                    .map_err(SerializedGreenError::from)?
        {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("reference rewrite leaf summary changed"),
            ));
        }
        self.next_leaf_index =
            self.next_leaf_index
                .checked_add(1)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "reference rewrite leaf index",
                ))?;
        self.current_leaf = None;
        self.event_cursor = 0;
        self.next_program_ordinal = 0;
        self.actual_leaf_summary = GreenSummary::default();
        Ok(())
    }
}

fn reclassify_reference_prefix_run(
    run: DecodedSourceProjectionRun,
) -> Result<GreenEvent, ActiveParagraphProjectionError> {
    let (owner_relative_depth, part) = if run.owner_relative_depth == 0 {
        (0, CoveragePart::GAP)
    } else {
        if !matches!(run.logical_contribution, DecodedLogicalContribution::None) {
            return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                "ancestor-owned reference source contributes logical text",
            ));
        }
        (run.owner_relative_depth - 1, run.part)
    };
    let mut replacement = SourceProjectionRun::new(
        run.id,
        run.metric.bytes,
        run.metric.utf16,
        owner_relative_depth,
        part,
    )?;
    if run.projection_reset_after {
        replacement.mark_projection_reset_after();
    }
    Ok(GreenEvent::Coverage(replacement))
}

fn retain_identity_reference_suffix_run(
    run: DecodedSourceProjectionRun,
    paragraph: BlockId,
) -> Result<GreenEvent, ActiveParagraphProjectionError> {
    let logical = match run.logical_contribution {
        DecodedLogicalContribution::None => None,
        DecodedLogicalContribution::Identity => Some(LogicalContribution::Identity),
        DecodedLogicalContribution::Hidden { .. }
        | DecodedLogicalContribution::Atomic(_)
        | DecodedLogicalContribution::Program(_) => {
            return Err(ActiveParagraphProjectionError::UnsupportedCanonicalRewrite(
                "visible reference suffix requires projection splitting or retained Program reuse",
            ));
        }
    };
    let mut replacement = match logical {
        Some(logical) => SourceProjectionRun::with_logical(
            run.id,
            run.metric.bytes,
            run.metric.utf16,
            run.owner_relative_depth,
            run.part,
            paragraph,
            logical,
        )?,
        None => SourceProjectionRun::new(
            run.id,
            run.metric.bytes,
            run.metric.utf16,
            run.owner_relative_depth,
            run.part,
        )?,
    };
    if run.projection_reset_after {
        replacement.mark_projection_reset_after();
    }
    Ok(GreenEvent::Coverage(replacement))
}

impl ActiveParagraphReferenceUnchanged {
    pub(crate) fn into_parts(
        self,
    ) -> (
        ProvisionalParagraphEnter,
        ActiveParagraphProjectedReferenceTerminal,
        Option<StagedParagraphTerminator>,
    ) {
        (self.paragraph, self.terminal, self.staged_terminator)
    }
}

impl From<SerializedGreenError> for ActiveParagraphProjectionError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl From<ArenaBuildError> for ActiveParagraphProjectionError {
    fn from(value: ArenaBuildError) -> Self {
        Self::Green(value.into())
    }
}

impl From<SourceProjectionSessionError> for ActiveParagraphProjectionError {
    fn from(value: SourceProjectionSessionError) -> Self {
        Self::SourceSession(value)
    }
}

fn replay_source_start(
    capability: &ActiveParagraphProjectedRangeCapability,
) -> Result<ActiveParagraphProjectionSourceStart, ActiveParagraphProjectionError> {
    if capability.identity != capability.start.identity
        || capability.start.source != capability.identity.source()
    {
        return Err(ActiveParagraphProjectionError::CrossedCursor);
    }
    let physical = match capability.start.observation {
        ActiveParagraphProjectedBoundaryObservation::ExactSource { physical } => physical,
        ActiveParagraphProjectedBoundaryObservation::AtomicAmbiguity { physical, .. } => {
            physical.start
        }
        ActiveParagraphProjectedBoundaryObservation::Virtual {
            physical_boundary, ..
        } => physical_boundary,
    };
    let physical_lower_bound_bytes = usize::try_from(physical.bytes).map_err(|_| {
        ActiveParagraphProjectionError::Overflow(
            "active Paragraph replay source lower bound exceeds usize",
        )
    })?;
    if physical_lower_bound_bytes > capability.start.source.bytes {
        return Err(ActiveParagraphProjectionError::SourceOutOfBounds);
    }
    Ok(ActiveParagraphProjectionSourceStart {
        identity: capability.identity,
        source: capability.start.source,
        physical_lower_bound_bytes,
    })
}

pub(crate) trait ParagraphPhysicalSource {
    fn source_root(&self) -> SourceRootId;
    fn source_revision(&self) -> SourceRevision;
    fn source_extent_bytes(&self) -> usize;
    fn byte_at(&mut self, absolute: usize) -> Result<u8, ActiveParagraphProjectionError>;
}

impl ActiveParagraphProjectionSourcePass {
    fn finish_in_place(&mut self) -> Result<(), ActiveParagraphProjectionError> {
        if self.closed {
            return Err(ActiveParagraphProjectionError::CursorComplete);
        }
        self.session
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?
            .finish_pass(self.identity.source(), self.identity.cursor_nonce)?;
        self.closed = true;
        Ok(())
    }

    fn cancel_in_place(&mut self) -> Result<(), ActiveParagraphProjectionError> {
        if self.closed {
            return Ok(());
        }
        self.session
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?
            .cancel_pass(self.identity.source(), self.identity.cursor_nonce)?;
        self.closed = true;
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
    ) -> Result<SourceProjectionSession, ActiveParagraphProjectionError> {
        self.finish_in_place()?;
        self.session
            .take()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)
    }

    pub(crate) fn cancel(
        mut self,
    ) -> Result<SourceProjectionSession, ActiveParagraphProjectionError> {
        self.cancel_in_place()?;
        self.session
            .take()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)
    }
}

impl Drop for ActiveParagraphProjectionSourcePass {
    fn drop(&mut self) {
        if !self.closed {
            // Drop is a cancellation boundary, not a silent finish. The exact
            // session authority was checked when this adapter was minted.
            let _ = self.cancel_in_place();
        }
    }
}

impl ParagraphPhysicalSource for ActiveParagraphProjectionSourcePass {
    fn source_root(&self) -> SourceRootId {
        self.identity.source_root
    }

    fn source_revision(&self) -> SourceRevision {
        self.identity.source_revision
    }

    fn source_extent_bytes(&self) -> usize {
        self.identity.source_bytes
    }

    fn byte_at(&mut self, absolute: usize) -> Result<u8, ActiveParagraphProjectionError> {
        if self.closed {
            return Err(ActiveParagraphProjectionError::CursorComplete);
        }
        Ok(self
            .session
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?
            .read_pass_byte(self.identity.source(), self.identity.cursor_nonce, absolute)?)
    }
}

#[derive(Clone, Copy, Debug)]
struct BuildRouteFrame {
    branch: ArenaId,
    base_leaf_index: u64,
    went_right: bool,
}

#[derive(Clone, Debug)]
enum CursorLeaf {
    Sealed { id: ArenaId, index: u64 },
    Partial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocatedProjectionMapping {
    Identity,
    Atomic(AtomicProjectionKind),
    Virtual(VirtualProjectionKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedProjectionSegment {
    physical: ProjectionMetricRange,
    logical: ProjectionMetricRange,
    mapping: LocatedProjectionMapping,
}

struct ReplayStartState {
    next_event: usize,
    physical_position: SerializedMetric,
    byte_state: LogicalByteState,
    pending_program: Option<PendingBuildProgram>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ByteProjectionMapping {
    Identity,
    Atomic(AtomicProjectionKind),
    Virtual(VirtualProjectionKind),
}

#[derive(Clone, Debug)]
struct LogicalByteState {
    coverage: CoverageId,
    part: CoveragePart,
    physical: Range<u64>,
    physical_utf16: Range<u64>,
    logical: Range<u64>,
    logical_utf16: Range<u64>,
    mapping: ByteProjectionMapping,
    program: Option<ArenaId>,
    next_byte: u64,
    utf8_remaining: u8,
    utf8_codepoint: u32,
    utf8_minimum: u32,
}

impl LogicalByteState {
    fn logical_len(&self) -> u64 {
        self.logical.end - self.logical.start
    }

    fn complete(&self) -> bool {
        self.next_byte == self.logical_len()
    }
}

#[derive(Clone, Debug)]
struct PendingBuildProgram {
    coverage: CoverageId,
    part: CoveragePart,
    page: ArenaId,
    next_byte: usize,
    pieces_remaining: u16,
    physical_position: SerializedMetric,
    expected_physical_end: SerializedMetric,
    expected_logical_end: SerializedMetric,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReadyLogicalByte {
    offset: usize,
    byte: u8,
    raw_codepoint_contribution: u8,
    coverage: Option<CoverageId>,
    mapping: ByteProjectionMapping,
    physical: Range<u64>,
    logical: Range<u64>,
    program: Option<ArenaId>,
    delivered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveParagraphProjectionPoll {
    Pending,
    ByteReady,
    Complete,
    Cancelled,
}

/// Cursor over the exact active Paragraph in one unpublished builder.
///
/// The cursor owns only bounded route/page/program scratch.  It holds no arena
/// owner and therefore performs an exact builder/session/binding recheck on
/// every poll.  The actor must keep the builder quiescent while this value is
/// installed; any mutation changes the stamp and is rejected as stale.
#[derive(Debug)]
pub(crate) struct ActiveParagraphProjectionCursor {
    identity: ActiveParagraphProjectionIdentity,
    stamp: BuilderProjectionStamp,
    target_active: ActiveProvisionalParagraph,
    route: Vec<BuildRouteFrame>,
    leaf: CursorLeaf,
    events: Vec<DecodedLeafEvent>,
    next_event: usize,
    physical_position: SerializedMetric,
    global_logical_position: SerializedMetric,
    paragraph_global_logical_base: SerializedMetric,
    paragraph_logical_position: SerializedMetric,
    pending_program: Option<PendingBuildProgram>,
    byte_state: Option<LogicalByteState>,
    ready: Option<ReadyLogicalByte>,
    staged_terminator: Option<StagedParagraphTerminator>,
    staged_terminator_emitted: bool,
    complete: bool,
    cancelled: bool,
    receipt: ActiveParagraphProjectionReceipt,
}

impl ResumableSerializedGreenBuild {
    pub(crate) fn open_active_paragraph_projection_cursor(
        &self,
        session: &ArenaBuildSession<'_>,
        paragraph: &ProvisionalParagraphEnter,
        binding: ActorProjectionBinding,
        staged_terminator: Option<StagedParagraphTerminator>,
    ) -> Result<ActiveParagraphProjectionCursor, ActiveParagraphProjectionError> {
        self.ensure_session(session)?;
        if self.phase != SerializedGreenStreamPhase::Accepting
            || self.pending_event.is_some()
            || self.pending_provisional_paragraph.is_some()
            || self.ready_provisional_paragraph.is_some()
            || self.setext_job.is_some()
            || self.ready_setext_promotion.is_some()
            || self.whole_normalization_job.is_some()
            || self.ready_whole_normalization.is_some()
            || self.fragment_job.is_some()
            || self.ready_fragment_replacement.is_some()
            || self.pending_barrier_cut.is_some()
            || self.ready_barrier_cut.is_some()
            || self.ready_working_cut.is_some()
            || self.tail_sealed_leaves != 0
        {
            return Err(ActiveParagraphProjectionError::StaleBinding);
        }
        let active = self
            .active_provisional_paragraph
            .ok_or(ActiveParagraphProjectionError::CrossedParagraph)?;
        if paragraph.build != self.build
            || paragraph.block != active.block
            || paragraph.generation != active.generation
            || paragraph.event_ordinal != active.event_ordinal
            || paragraph.source_before != active.source_before
        {
            return Err(ActiveParagraphProjectionError::CrossedParagraph);
        }
        let prefix = self
            .working_prefix
            .as_ref()
            .map_or(GreenSummary::default(), |prefix| prefix.summary);
        let root = self
            .working_prefix
            .as_ref()
            .map(|prefix| session.owner_id(&prefix.owner))
            .transpose()?;
        if prefix.leaves != self.sealed_leaves
            || prefix.tokens != self.sealed_events
            || prefix.metric != self.sealed_metric
        {
            return Err(ActiveParagraphProjectionError::StaleBinding);
        }
        let green_high_water = self.sealed_metric.checked_add(self.leaf.summary.metric)?;
        let source_bytes = usize::try_from(self.spec.source_bytes).map_err(|_| {
            ActiveParagraphProjectionError::Overflow("active Paragraph source extent exceeds usize")
        })?;
        let expected_composer_high_water = match staged_terminator {
            Some(terminator)
                if terminator.owner_generation != binding.projection_generation
                    || terminator.source_start != green_high_water =>
            {
                return Err(ActiveParagraphProjectionError::CrossedProjection(
                    ActiveParagraphProjectionBindingMismatch::StagedTerminator,
                ));
            }
            Some(terminator) => terminator
                .source_start
                .checked_add(terminator.kind.physical_metric())?,
            None => green_high_water,
        };
        let initial_mismatch = if binding.source_root != self.spec.source_root {
            Some(ActiveParagraphProjectionBindingMismatch::SourceRoot)
        } else if binding.source_revision != self.spec.source_revision {
            Some(ActiveParagraphProjectionBindingMismatch::SourceRevision)
        } else if binding.source_bytes != source_bytes {
            Some(ActiveParagraphProjectionBindingMismatch::SourceExtent)
        } else if binding.build != self.build {
            Some(ActiveParagraphProjectionBindingMismatch::Build)
        } else if binding.paragraph != active.block {
            Some(ActiveParagraphProjectionBindingMismatch::Paragraph)
        } else if binding.paragraph_generation != active.generation {
            Some(ActiveParagraphProjectionBindingMismatch::ParagraphGeneration)
        } else if binding.projection_generation == 0 {
            Some(ActiveParagraphProjectionBindingMismatch::ProjectionGeneration)
        } else if binding.composer_high_water != expected_composer_high_water {
            Some(ActiveParagraphProjectionBindingMismatch::ComposerHighWater)
        } else if binding.barrier_generation == 0 {
            Some(ActiveParagraphProjectionBindingMismatch::BarrierGeneration)
        } else {
            None
        };
        if let Some(mismatch) = initial_mismatch {
            return Err(ActiveParagraphProjectionError::CrossedProjection(mismatch));
        }
        let stamp = BuilderProjectionStamp {
            root,
            prefix,
            partial: self.leaf.summary,
            partial_bytes: self.leaf.bytes.len(),
            partial_programs: self.leaf.programs.len(),
            sealed_leaves: self.sealed_leaves,
            sealed_events: self.sealed_events,
            sealed_metric: self.sealed_metric,
        };
        ActiveParagraphProjectionCursor::new(
            self,
            session,
            binding,
            active,
            stamp,
            staged_terminator,
        )
    }
}

impl ActiveParagraphProjectionCursor {
    fn new(
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        active: ActiveProvisionalParagraph,
        stamp: BuilderProjectionStamp,
        staged_terminator: Option<StagedParagraphTerminator>,
    ) -> Result<Self, ActiveParagraphProjectionError> {
        let mut receipt = ActiveParagraphProjectionReceipt::default();
        let (route, leaf, events, selected, physical_base, logical_base) = match active.storage {
            ProvisionalParagraphStorage::Sealed {
                leaf_index,
                byte_offset,
                ..
            } => {
                let root = stamp
                    .root
                    .ok_or(ActiveParagraphProjectionError::StaleBinding)?;
                let (route, leaf, physical_base, logical_base) =
                    descend_to_leaf(session.arena(), root, leaf_index, &mut receipt)?;
                let payload_bytes = session
                    .arena()
                    .payload(leaf)
                    .map_err(SerializedGreenError::from)?
                    .len();
                let (_, events) = decode_leaf(session.arena(), leaf)?;
                receipt.leaf_pages_decoded = 1;
                receipt.events_decoded = events.len();
                receipt.maximum_decoded_page_bytes = payload_bytes
                    .checked_add(
                        events
                            .capacity()
                            .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                            .ok_or(ActiveParagraphProjectionError::Overflow(
                                "decoded active Paragraph event bytes",
                            ))?,
                    )
                    .ok_or(ActiveParagraphProjectionError::Overflow(
                        "decoded active Paragraph page bytes",
                    ))?;
                let selected = select_paragraph_enter(&events, byte_offset, active.block)?;
                (
                    route,
                    CursorLeaf::Sealed {
                        id: leaf,
                        index: leaf_index,
                    },
                    events,
                    selected,
                    physical_base,
                    logical_base,
                )
            }
            ProvisionalParagraphStorage::Partial { byte_offset, .. } => {
                let events = decode_partial_leaf(build, session)?;
                receipt.partial_leaf_decodes = 1;
                receipt.events_decoded = events.len();
                receipt.maximum_decoded_page_bytes = build
                    .leaf
                    .bytes
                    .len()
                    .checked_add(
                        events
                            .capacity()
                            .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                            .ok_or(ActiveParagraphProjectionError::Overflow(
                                "decoded active Paragraph event bytes",
                            ))?,
                    )
                    .ok_or(ActiveParagraphProjectionError::Overflow(
                        "decoded active Paragraph page bytes",
                    ))?;
                let selected = select_paragraph_enter(&events, byte_offset, active.block)?;
                (
                    Vec::new(),
                    CursorLeaf::Partial,
                    events,
                    selected,
                    stamp.prefix.metric,
                    stamp.prefix.logical_metric,
                )
            }
        };

        let (within_physical, within_logical) = events[..selected].iter().try_fold::<_, _, Result<
            _,
            SerializedGreenError,
        >>(
            (SerializedMetric::default(), SerializedMetric::default()),
            |(physical, logical), decoded| match &decoded.event {
                DecodedGreenEventKind::Coverage(run) => Ok((
                    physical.checked_add(run.metric)?,
                    logical
                        .checked_add_logical(run.logical_contribution.summary_metric(run.metric))?,
                )),
                DecodedGreenEventKind::Enter { .. } | DecodedGreenEventKind::Exit { .. } => {
                    Ok((physical, logical))
                }
            },
        )?;
        let physical_position = physical_base.checked_add(within_physical)?;
        if physical_position != active.source_before {
            return Err(ActiveParagraphProjectionError::StaleBinding);
        }
        let paragraph_global_logical_base = logical_base.checked_add_logical(within_logical)?;
        let cursor_nonce = NEXT_CURSOR_NONCE.fetch_add(1, Ordering::Relaxed);
        if cursor_nonce == 0 {
            return Err(ActiveParagraphProjectionError::Overflow(
                "active Paragraph cursor nonce",
            ));
        }
        receipt.maximum_route_depth = route.len();
        Ok(Self {
            identity: ActiveParagraphProjectionIdentity {
                source_root: binding.source_root,
                source_revision: binding.source_revision,
                source_bytes: binding.source_bytes,
                build: binding.build,
                paragraph: binding.paragraph,
                paragraph_generation: binding.paragraph_generation,
                paragraph_event_ordinal: active.event_ordinal,
                paragraph_source_before: active.source_before,
                projection_generation: binding.projection_generation,
                composer_high_water: binding.composer_high_water,
                barrier_generation: binding.barrier_generation,
                cursor_nonce,
            },
            stamp,
            target_active: active,
            route,
            leaf,
            events,
            next_event: selected + 1,
            physical_position,
            global_logical_position: paragraph_global_logical_base,
            paragraph_global_logical_base,
            paragraph_logical_position: SerializedMetric::default(),
            pending_program: None,
            byte_state: None,
            ready: None,
            staged_terminator,
            staged_terminator_emitted: false,
            complete: false,
            cancelled: false,
            receipt,
        })
    }

    pub(crate) const fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.identity
    }

    /// Authenticated lower-bound cut for the initial full-Paragraph pass.
    /// The actor can start its transaction-local Crop pass here instead of
    /// scanning from document byte zero.
    pub(crate) fn source_start(
        &self,
    ) -> Result<ActiveParagraphProjectionSourceStart, ActiveParagraphProjectionError> {
        let physical_lower_bound_bytes =
            usize::try_from(self.identity.paragraph_source_before.bytes).map_err(|_| {
                ActiveParagraphProjectionError::Overflow(
                    "active Paragraph source lower bound exceeds usize",
                )
            })?;
        if physical_lower_bound_bytes > self.identity.source_bytes {
            return Err(ActiveParagraphProjectionError::SourceOutOfBounds);
        }
        Ok(ActiveParagraphProjectionSourceStart {
            identity: self.identity,
            source: self.identity.source(),
            physical_lower_bound_bytes,
        })
    }

    /// Opens the transaction's one immutable Crop-root role only after the
    /// active builder, writer binding, source descriptor, and cursor nonce
    /// have all joined. The session itself is non-cloneable and must retire or
    /// cancel before the writer replaces canonical state.
    pub(crate) fn open_source_projection_session(
        &self,
        build: &ResumableSerializedGreenBuild,
        arena_session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        store: &SourceStore,
    ) -> Result<SourceProjectionSession, ActiveParagraphProjectionError> {
        self.validate_live(build, arena_session, binding)?;
        if store.descriptor() != self.identity.source() {
            return Err(ActiveParagraphProjectionError::WrongSource);
        }
        Ok(store.issue_projection_session(self.identity.cursor_nonce)?)
    }

    pub(crate) const fn receipt(&self) -> ActiveParagraphProjectionReceipt {
        self.receipt
    }

    pub(crate) fn logical_end(&self) -> SerializedMetric {
        self.paragraph_logical_position
    }

    fn validate_live(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
    ) -> Result<(), ActiveParagraphProjectionError> {
        build.ensure_session(session)?;
        if let Some(mismatch) = binding.first_mismatch(self.identity) {
            return Err(ActiveParagraphProjectionError::CrossedProjection(mismatch));
        }
        let active = build
            .active_provisional_paragraph
            .ok_or(ActiveParagraphProjectionError::StaleBinding)?;
        let root = build
            .working_prefix
            .as_ref()
            .map(|prefix| session.owner_id(&prefix.owner))
            .transpose()?;
        let prefix = build
            .working_prefix
            .as_ref()
            .map_or(GreenSummary::default(), |prefix| prefix.summary);
        let current = BuilderProjectionStamp {
            root,
            prefix,
            partial: build.leaf.summary,
            partial_bytes: build.leaf.bytes.len(),
            partial_programs: build.leaf.programs.len(),
            sealed_leaves: build.sealed_leaves,
            sealed_events: build.sealed_events,
            sealed_metric: build.sealed_metric,
        };
        if build.phase != SerializedGreenStreamPhase::Accepting
            || build.tail_sealed_leaves != 0
            || current != self.stamp
            || self.identity.source_root != build.spec.source_root
            || self.identity.source_revision != build.spec.source_revision
            || usize::try_from(build.spec.source_bytes) != Ok(self.identity.source_bytes)
            || self.identity.build != build.build
            || self.identity.paragraph != active.block
            || self.identity.paragraph_generation != active.generation
            || self.identity.paragraph_event_ordinal != active.event_ordinal
            || self.identity.paragraph_source_before != active.source_before
            || active != self.target_active
        {
            return Err(ActiveParagraphProjectionError::StaleBinding);
        }
        Ok(())
    }

    pub(crate) fn cancel(&mut self) {
        self.pending_program = None;
        self.byte_state = None;
        self.ready = None;
        self.cancelled = true;
    }

    pub(crate) fn direct_source(
        &mut self,
        identity: ActiveParagraphProjectionIdentity,
    ) -> Result<ActiveParagraphDirectSource<'_>, ActiveParagraphProjectionError> {
        if identity != self.identity {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        Ok(ActiveParagraphDirectSource { cursor: self })
    }

    /// Resolves one parser-authenticated logical boundary without replaying
    /// from Paragraph byte zero.  A sealed lookup descends by logical summary
    /// in O(tree height), decodes one leaf page, and at most one bounded
    /// Program page.  A target in the active partial leaf stays page-bounded.
    fn resolve_boundary(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        position: DirectReferenceLogicalPosition,
        affinity: GreenAffinity,
    ) -> Result<
        (
            ActiveParagraphProjectedBoundary,
            ActiveParagraphProjectionSeekReceipt,
        ),
        ActiveParagraphProjectionError,
    > {
        if self.cancelled {
            return Err(ActiveParagraphProjectionError::CursorCancelled);
        }
        self.validate_live(build, session, binding)?;
        let target = SerializedMetric {
            bytes: position.bytes,
            utf16: position.utf16,
        };
        if !metric_at_or_before(target, self.paragraph_logical_position) {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
        }

        let green_global_end = self
            .stamp
            .prefix
            .logical_metric
            .checked_add_logical(self.stamp.partial.logical_metric)?;
        let green_paragraph_end =
            green_global_end.checked_sub(self.paragraph_global_logical_base)?;
        let paragraph_end = if self.staged_terminator.is_some() {
            green_paragraph_end.checked_add_logical(SerializedMetric { bytes: 1, utf16: 1 })?
        } else {
            green_paragraph_end
        };
        if !metric_at_or_before(target, paragraph_end) {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
        }

        let source = self.identity.source();
        let exact = |physical| ActiveParagraphProjectedBoundary {
            source,
            identity: self.identity,
            logical: target,
            affinity,
            observation: ActiveParagraphProjectedBoundaryObservation::ExactSource { physical },
        };

        // At the two outer boundaries there is no adjacent logical byte on
        // one side.  The active Paragraph token and writer high-water are the
        // exact authenticated source cuts for those affinities.
        if target.is_zero() && affinity == GreenAffinity::Upstream {
            return Ok((
                exact(self.identity.paragraph_source_before),
                ActiveParagraphProjectionSeekReceipt::default(),
            ));
        }
        if target == paragraph_end && affinity == GreenAffinity::Downstream {
            return Ok((
                exact(self.identity.composer_high_water),
                ActiveParagraphProjectionSeekReceipt::default(),
            ));
        }

        let probe = match affinity {
            GreenAffinity::Upstream => target
                .bytes
                .checked_sub(1)
                .ok_or(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds)?,
            GreenAffinity::Downstream => target.bytes,
        };
        if probe >= paragraph_end.bytes {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
        }

        let (segment, receipt) = if probe >= green_paragraph_end.bytes {
            let terminator = self
                .staged_terminator
                .ok_or(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds)?;
            let physical_end = terminator
                .source_start
                .checked_add(terminator.kind.physical_metric())?;
            let logical_end =
                green_paragraph_end.checked_add_logical(SerializedMetric { bytes: 1, utf16: 1 })?;
            (
                LocatedProjectionSegment {
                    physical: ProjectionMetricRange::new(terminator.source_start, physical_end)?,
                    logical: ProjectionMetricRange::new(green_paragraph_end, logical_end)?,
                    mapping: match terminator.kind {
                        StagedTerminatorKind::Lf => LocatedProjectionMapping::Identity,
                        StagedTerminatorKind::CrLf => {
                            LocatedProjectionMapping::Atomic(AtomicProjectionKind::CrLfToLf)
                        }
                        StagedTerminatorKind::LoneCr => {
                            LocatedProjectionMapping::Atomic(AtomicProjectionKind::LoneCrToLf)
                        }
                    },
                },
                ActiveParagraphProjectionSeekReceipt::default(),
            )
        } else {
            let global_probe = self
                .paragraph_global_logical_base
                .bytes
                .checked_add(probe)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph seek logical probe",
                ))?;
            let (mut segment, receipt) =
                locate_projection_segment(build, session, self.stamp, global_probe)?;
            segment.logical.start = segment
                .logical
                .start
                .checked_sub(self.paragraph_global_logical_base)?;
            segment.logical.end = segment
                .logical
                .end
                .checked_sub(self.paragraph_global_logical_base)?;
            (segment, receipt)
        };

        let observation = map_logical_boundary(segment, target)?;
        Ok((
            ActiveParagraphProjectedBoundary {
                source,
                identity: self.identity,
                logical: target,
                affinity,
                observation,
            },
            receipt,
        ))
    }

    fn resolve_range(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        logical: &DirectReferenceLogicalRange,
        start_affinity: GreenAffinity,
        end_affinity: GreenAffinity,
    ) -> Result<ActiveParagraphProjectedRangeCapability, ActiveParagraphProjectionError> {
        validate_direct_logical_range(logical)?;
        let (start, start_receipt) = self.resolve_boundary(
            build,
            session,
            binding,
            DirectReferenceLogicalPosition {
                bytes: logical.bytes.start,
                utf16: logical.utf16.start,
            },
            start_affinity,
        )?;
        let (end, end_receipt) = self.resolve_boundary(
            build,
            session,
            binding,
            DirectReferenceLogicalPosition {
                bytes: logical.bytes.end,
                utf16: logical.utf16.end,
            },
            end_affinity,
        )?;
        if start.identity != end.identity || start.source != end.source {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        Ok(ActiveParagraphProjectedRangeCapability {
            identity: self.identity,
            logical: logical.clone(),
            start,
            end,
            receipt: start_receipt.followed_by(end_receipt)?,
        })
    }

    /// Consumes one non-cloneable DFA occurrence only after every logical cut
    /// has been resolved through this exact cursor.  A failure drops the
    /// output without producing its acknowledgement, leaving the DFA safely
    /// frozen for candidate abort.
    pub(crate) fn project_reference_output(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        output: DirectReferencePrefixOutput<ActiveParagraphProjectionIdentity>,
    ) -> Result<ActiveParagraphProjectedReferenceOutput, ActiveParagraphProjectionError> {
        if output.source_identity() != self.identity {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        let definition = output.definition();
        let source = self.resolve_range(
            build,
            session,
            binding,
            &definition.logical_source,
            GreenAffinity::Upstream,
            GreenAffinity::Downstream,
        )?;
        let label = self.resolve_range(
            build,
            session,
            binding,
            &definition.logical_label,
            GreenAffinity::Downstream,
            GreenAffinity::Upstream,
        )?;
        let destination = self.resolve_range(
            build,
            session,
            binding,
            &definition.logical_destination,
            GreenAffinity::Downstream,
            GreenAffinity::Upstream,
        )?;
        let title = definition
            .logical_title
            .as_ref()
            .map(|title| {
                self.resolve_range(
                    build,
                    session,
                    binding,
                    title,
                    GreenAffinity::Downstream,
                    GreenAffinity::Upstream,
                )
            })
            .transpose()?;
        let (definition, ack) = output.acknowledge();
        Ok(ActiveParagraphProjectedReferenceOutput {
            identity: self.identity,
            definition,
            source,
            label,
            destination,
            title,
            ack,
        })
    }

    /// Projects the terminal prefix-removal and recognition cuts before the
    /// parser acknowledgement can escape. CandidateWriter therefore receives
    /// physical ambiguity-preserving capabilities, never caller-authored
    /// scalar offsets, for both the canonical rewrite and visible remainder.
    pub(crate) fn project_reference_terminal(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        output: DirectReferencePrefixTerminalOutput<ActiveParagraphProjectionIdentity>,
    ) -> Result<ActiveParagraphProjectedReferenceTerminal, ActiveParagraphProjectionError> {
        if output.source_identity() != self.identity {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        let terminal = output.terminal().clone();
        validate_direct_logical_range(&terminal.logical_reference_prefix)?;
        validate_direct_logical_range(&terminal.logical_recognition)?;
        if terminal.logical_reference_prefix.bytes.start != terminal.logical_recognition.bytes.start
            || terminal.logical_reference_prefix.utf16.start
                != terminal.logical_recognition.utf16.start
            || terminal.logical_reference_prefix.bytes.end > terminal.logical_recognition.bytes.end
            || terminal.logical_reference_prefix.utf16.end > terminal.logical_recognition.utf16.end
        {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
        }
        let reference_prefix = self.resolve_range(
            build,
            session,
            binding,
            &terminal.logical_reference_prefix,
            GreenAffinity::Upstream,
            GreenAffinity::Downstream,
        )?;
        let recognition = self.resolve_range(
            build,
            session,
            binding,
            &terminal.logical_recognition,
            GreenAffinity::Upstream,
            GreenAffinity::Downstream,
        )?;
        let ack = output.acknowledge();
        Ok(ActiveParagraphProjectedReferenceTerminal {
            identity: self.identity,
            terminal,
            reference_prefix,
            recognition,
            ack,
        })
    }

    pub(crate) fn begin_range_replay(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        request: ActiveParagraphRangeReplayRequest,
    ) -> Result<ActiveParagraphRangeReplayCursor, ActiveParagraphProjectionError> {
        self.begin_range_replay_parts(build, session, binding, request)
            .map(|(replay, _)| replay)
    }

    /// Consumes one linear parser-authenticated request and joins its bounded
    /// Green replay to the session's sole physical-source pass. This is the
    /// production-shaped entry point; the bare cursor constructor above is
    /// retained only for projection mechanism tests.
    pub(crate) fn begin_range_replay_in_source_session(
        &self,
        build: &ResumableSerializedGreenBuild,
        arena_session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        request: ActiveParagraphRangeReplayRequest,
        source_session: SourceProjectionSession,
    ) -> Result<ActiveParagraphRangeReplayPass, ActiveParagraphProjectionError> {
        let (replay, source_start) =
            self.begin_range_replay_parts(build, arena_session, binding, request)?;
        let source = source_start.begin_source_pass(source_session)?;
        Ok(ActiveParagraphRangeReplayPass {
            replay: Some(replay),
            source: Some(source),
        })
    }

    fn begin_range_replay_parts(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        request: ActiveParagraphRangeReplayRequest,
    ) -> Result<
        (
            ActiveParagraphRangeReplayCursor,
            ActiveParagraphProjectionSourceStart,
        ),
        ActiveParagraphProjectionError,
    > {
        if self.cancelled {
            return Err(ActiveParagraphProjectionError::CursorCancelled);
        }
        self.validate_live(build, session, binding)?;
        let expected_source_start = replay_source_start(&request.capability)?;
        if request.source_start != expected_source_start {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        let ActiveParagraphRangeReplayRequest {
            capability,
            source_start,
        } = request;
        self.validate_range_capability(&capability)?;
        let start = capability.start.logical;
        let end = capability.end.logical;
        let (inner, seek_receipt) = if start == end {
            (None, ActiveParagraphProjectionSeekReceipt::default())
        } else {
            let (inner, receipt) = build_range_replay_cursor(self, build, session, start)?;
            (Some(inner), receipt)
        };
        Ok((
            ActiveParagraphRangeReplayCursor {
                identity: self.identity,
                start,
                end,
                inner,
                capability: Some(capability),
                seek_receipt,
                complete: start == end,
                cancelled: false,
            },
            source_start,
        ))
    }

    /// Consumes a completed first-pass capability and narrows it to the body
    /// selected by `DestinationTrimProbe::finish` or
    /// `clean_title_body_range`. Those selectors remove only ASCII bytes at
    /// either edge, so their byte counts are also the exact UTF-16 deltas.
    /// The selected body is resolved again through the same live projection;
    /// no caller-authored physical cut is accepted.
    pub(crate) fn narrow_replayed_ascii_edge_selection(
        &self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        capability: ActiveParagraphProjectedRangeCapability,
        selected: Range<usize>,
    ) -> Result<ActiveParagraphProjectedRangeCapability, ActiveParagraphProjectionError> {
        if self.cancelled {
            return Err(ActiveParagraphProjectionError::CursorCancelled);
        }
        self.validate_live(build, session, binding)?;
        self.validate_range_capability(&capability)?;

        let outer_bytes = capability
            .logical
            .bytes
            .end
            .checked_sub(capability.logical.bytes.start)
            .ok_or(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch)?;
        let selected_start = u64::try_from(selected.start)
            .map_err(|_| ActiveParagraphProjectionError::Overflow("selected value body start"))?;
        let selected_end = u64::try_from(selected.end)
            .map_err(|_| ActiveParagraphProjectionError::Overflow("selected value body end"))?;
        if selected_start > selected_end || selected_end > outer_bytes {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
        }
        let trailing_ascii = outer_bytes
            .checked_sub(selected_end)
            .ok_or(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch)?;
        let selected_logical = DirectReferenceLogicalRange {
            bytes: capability
                .logical
                .bytes
                .start
                .checked_add(selected_start)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "selected value body byte start",
                ))?
                ..capability
                    .logical
                    .bytes
                    .end
                    .checked_sub(trailing_ascii)
                    .ok_or(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch)?,
            utf16: capability
                .logical
                .utf16
                .start
                .checked_add(selected_start)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "selected value body UTF-16 start",
                ))?
                ..capability
                    .logical
                    .utf16
                    .end
                    .checked_sub(trailing_ascii)
                    .ok_or(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch)?,
        };
        self.resolve_range(
            build,
            session,
            binding,
            &selected_logical,
            GreenAffinity::Downstream,
            GreenAffinity::Upstream,
        )
    }

    fn validate_range_capability(
        &self,
        capability: &ActiveParagraphProjectedRangeCapability,
    ) -> Result<(), ActiveParagraphProjectionError> {
        if capability.identity != self.identity
            || capability.start.identity != self.identity
            || capability.end.identity != self.identity
            || capability.start.source != self.identity.source()
            || capability.end.source != self.identity.source()
            || capability.start.logical.bytes != capability.logical.bytes.start
            || capability.start.logical.utf16 != capability.logical.utf16.start
            || capability.end.logical.bytes != capability.logical.bytes.end
            || capability.end.logical.utf16 != capability.logical.utf16.end
        {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        validate_direct_logical_range(&capability.logical)?;
        if !metric_at_or_before(capability.start.logical, capability.end.logical)
            || !metric_at_or_before(capability.end.logical, self.paragraph_logical_position)
        {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
        }
        Ok(())
    }

    /// Consumes the completed cursor into an exact transaction-local terminal
    /// seal. The active partial page must already have crossed a writer-owned
    /// leaf barrier and been reduced into `working_prefix`; the same barrier
    /// makes range replay and the immediately following replacement share one
    /// immutable stamp. No old Green owner is published by this method.
    pub(crate) fn into_transaction_seal(
        self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
    ) -> Result<ActiveParagraphProjectionTransactionSeal, ActiveParagraphProjectionError> {
        if self.cancelled {
            return Err(ActiveParagraphProjectionError::CursorCancelled);
        }
        self.validate_live(build, session, binding)?;
        if !self.complete
            || self.ready.is_some()
            || self.byte_state.is_some()
            || self.pending_program.is_some()
        {
            return Err(ActiveParagraphProjectionError::CapabilityNotReady);
        }
        let ProvisionalParagraphStorage::Sealed { leaf_index, .. } = self.target_active.storage
        else {
            return Err(ActiveParagraphProjectionError::ProjectionTransactionRequiresSealedBarrier);
        };
        if self.stamp.partial != GreenSummary::default()
            || self.stamp.partial_bytes != LEAF_HEADER_BYTES
            || self.stamp.partial_programs != 0
            || self.stamp.prefix.leaves != self.stamp.sealed_leaves
            || leaf_index >= self.stamp.sealed_leaves
        {
            return Err(ActiveParagraphProjectionError::ProjectionTransactionRequiresSealedBarrier);
        }
        let root = self
            .stamp
            .root
            .ok_or(ActiveParagraphProjectionError::ProjectionTransactionRequiresSealedBarrier)?;
        let green_logical_end = self
            .stamp
            .prefix
            .logical_metric
            .checked_sub(self.paragraph_global_logical_base)?;
        let paragraph_logical_end = if self.staged_terminator.is_some() {
            green_logical_end.checked_add_logical(SerializedMetric { bytes: 1, utf16: 1 })?
        } else {
            green_logical_end
        };
        if self.physical_position != self.identity.composer_high_water
            || self.paragraph_logical_position != paragraph_logical_end
        {
            return Err(ActiveParagraphProjectionError::CapabilityNotReady);
        }
        Ok(ActiveParagraphProjectionTransactionSeal {
            identity: self.identity,
            source: self.identity.source(),
            root,
            prefix: self.stamp.prefix,
            covered_leaf_range: leaf_index..self.stamp.sealed_leaves,
            paragraph_storage: self.target_active.storage,
            paragraph_physical: ProjectionMetricRange::new(
                self.identity.paragraph_source_before,
                self.identity.composer_high_water,
            )?,
            paragraph_logical: ProjectionMetricRange::new(
                SerializedMetric::default(),
                paragraph_logical_end,
            )?,
            staged_terminator: self.staged_terminator,
        })
    }

    fn install_byte_state(
        &mut self,
        coverage: CoverageId,
        part: CoveragePart,
        physical_metric: SerializedMetric,
        logical_metric: SerializedMetric,
        mapping: ByteProjectionMapping,
        program: Option<ArenaId>,
    ) -> Result<(), ActiveParagraphProjectionError> {
        if logical_metric.is_zero() || logical_metric.is_partially_zero() {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt(
                    "active Paragraph byte projection has an empty logical metric",
                ),
            ));
        }
        let physical_end = self.physical_position.checked_add(physical_metric)?;
        let logical_end = self
            .paragraph_logical_position
            .checked_add_logical(logical_metric)?;
        self.byte_state = Some(LogicalByteState {
            coverage,
            part,
            physical: self.physical_position.bytes..physical_end.bytes,
            physical_utf16: self.physical_position.utf16..physical_end.utf16,
            logical: self.paragraph_logical_position.bytes..logical_end.bytes,
            logical_utf16: self.paragraph_logical_position.utf16..logical_end.utf16,
            mapping,
            program,
            next_byte: 0,
            utf8_remaining: 0,
            utf8_codepoint: 0,
            utf8_minimum: 0,
        });
        Ok(())
    }

    fn process_event(
        &mut self,
        arena: &PageArena,
        decoded: DecodedLeafEvent,
    ) -> Result<(), ActiveParagraphProjectionError> {
        match decoded.event {
            DecodedGreenEventKind::Enter { .. } => Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("active Paragraph contains a nested block"),
            )),
            DecodedGreenEventKind::Exit { .. } => Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("active Paragraph was closed beneath its token"),
            )),
            DecodedGreenEventKind::Coverage(run) => {
                self.receipt.coverage_runs_visited =
                    self.receipt.coverage_runs_visited.checked_add(1).ok_or(
                        ActiveParagraphProjectionError::Overflow("active Paragraph coverage runs"),
                    )?;
                match run.logical_contribution {
                    DecodedLogicalContribution::None => {
                        self.physical_position = self.physical_position.checked_add(run.metric)?;
                    }
                    DecodedLogicalContribution::Hidden { .. } => {
                        self.receipt.hidden_pieces_visited =
                            self.receipt.hidden_pieces_visited.checked_add(1).ok_or(
                                ActiveParagraphProjectionError::Overflow(
                                    "active Paragraph hidden pieces",
                                ),
                            )?;
                        self.physical_position = self.physical_position.checked_add(run.metric)?;
                    }
                    DecodedLogicalContribution::Identity => {
                        self.install_byte_state(
                            run.id,
                            run.part,
                            run.metric,
                            run.metric,
                            ByteProjectionMapping::Identity,
                            None,
                        )?;
                    }
                    DecodedLogicalContribution::Atomic(projection) => {
                        projection.validate_kind().map_err(|_| {
                            ActiveParagraphProjectionError::Green(SerializedGreenError::Corrupt(
                                "active Paragraph has an invalid atomic projection",
                            ))
                        })?;
                        projection.validate_physical(run.metric).map_err(|_| {
                            ActiveParagraphProjectionError::Green(SerializedGreenError::Corrupt(
                                "active Paragraph atomic physical metric is invalid",
                            ))
                        })?;
                        self.receipt.atomic_pieces_visited =
                            self.receipt.atomic_pieces_visited.checked_add(1).ok_or(
                                ActiveParagraphProjectionError::Overflow(
                                    "active Paragraph atomic pieces",
                                ),
                            )?;
                        self.install_byte_state(
                            run.id,
                            run.part,
                            run.metric,
                            projection.logical_metric,
                            ByteProjectionMapping::Atomic(projection.kind),
                            None,
                        )?;
                    }
                    DecodedLogicalContribution::Program(program) => {
                        let page = program.retained_page()?;
                        let next_byte = validate_projection_program_edge_payload(
                            arena,
                            page,
                            usize::from(program.piece_count),
                            program.physical_metric,
                            program.logical_metric,
                        )?;
                        let payload_len = arena
                            .payload(page)
                            .map_err(SerializedGreenError::from)?
                            .len();
                        self.receipt.projection_program_pages_decoded = self
                            .receipt
                            .projection_program_pages_decoded
                            .checked_add(1)
                            .ok_or(ActiveParagraphProjectionError::Overflow(
                                "active Paragraph Program pages",
                            ))?;
                        self.receipt.projection_program_bytes_validated = self
                            .receipt
                            .projection_program_bytes_validated
                            .checked_add(payload_len)
                            .ok_or(ActiveParagraphProjectionError::Overflow(
                                "active Paragraph Program bytes",
                            ))?;
                        self.receipt.maximum_program_scratch_bytes =
                            self.receipt.maximum_program_scratch_bytes.max(
                                std::mem::size_of::<PendingBuildProgram>()
                                    + std::mem::size_of::<ProjectionPiece>()
                                    + std::mem::size_of::<Decoder<'_>>(),
                            );
                        self.pending_program = Some(PendingBuildProgram {
                            coverage: run.id,
                            part: run.part,
                            page,
                            next_byte,
                            pieces_remaining: program.piece_count,
                            physical_position: self.physical_position,
                            expected_physical_end: self
                                .physical_position
                                .checked_add(program.physical_metric)?,
                            expected_logical_end: self
                                .paragraph_logical_position
                                .checked_add_logical(program.logical_metric)?,
                        });
                    }
                }
                Ok(())
            }
        }
    }

    fn process_program_piece(
        &mut self,
        arena: &PageArena,
    ) -> Result<(), ActiveParagraphProjectionError> {
        let Some(mut pending) = self.pending_program.take() else {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("missing active Paragraph Program state"),
            ));
        };
        if pending.pieces_remaining == 0 {
            if pending.physical_position != pending.expected_physical_end
                || self.physical_position != pending.expected_physical_end
                || self.paragraph_logical_position != pending.expected_logical_end
            {
                return Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt(
                        "active Paragraph Program ended on the wrong partition boundary",
                    ),
                ));
            }
            let payload = arena
                .payload(pending.page)
                .map_err(SerializedGreenError::from)?;
            if pending.next_byte != payload.len() {
                return Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt("active Paragraph Program has trailing bytes"),
                ));
            }
            return Ok(());
        }
        if self.physical_position != pending.physical_position {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("active Paragraph Program position drifted"),
            ));
        }
        let payload = arena
            .payload(pending.page)
            .map_err(SerializedGreenError::from)?;
        if pending.next_byte >= payload.len() {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("active Paragraph Program escaped its page"),
            ));
        }
        let mut decoder = Decoder::new(payload);
        decoder.cursor = pending.next_byte;
        let piece = decode_projection_piece(&mut decoder)?;
        pending.next_byte = decoder.cursor;
        pending.pieces_remaining -= 1;
        self.receipt.projection_pieces_decoded = self
            .receipt
            .projection_pieces_decoded
            .checked_add(1)
            .ok_or(ActiveParagraphProjectionError::Overflow(
                "active Paragraph Program pieces",
            ))?;
        let (physical_metric, logical_metric) = piece.metrics();
        pending.physical_position = pending
            .physical_position
            .checked_add(physical_metric)
            .map_err(|_| {
                ActiveParagraphProjectionError::Green(SerializedGreenError::Corrupt(
                    "active Paragraph Program physical prefix overflow",
                ))
            })?;
        if pending.physical_position.bytes > pending.expected_physical_end.bytes
            || pending.physical_position.utf16 > pending.expected_physical_end.utf16
        {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt(
                    "active Paragraph Program physical prefix exceeds its partition",
                ),
            ));
        }
        let coverage = pending.coverage;
        let part = pending.part;
        let page = pending.page;
        self.pending_program = Some(pending);
        match piece {
            ProjectionPiece::Identity { .. } => self.install_byte_state(
                coverage,
                part,
                physical_metric,
                logical_metric,
                ByteProjectionMapping::Identity,
                Some(page),
            ),
            ProjectionPiece::Hidden { .. } => {
                self.receipt.hidden_pieces_visited =
                    self.receipt.hidden_pieces_visited.checked_add(1).ok_or(
                        ActiveParagraphProjectionError::Overflow(
                            "active Paragraph hidden Program pieces",
                        ),
                    )?;
                self.physical_position = self.physical_position.checked_add(physical_metric)?;
                Ok(())
            }
            ProjectionPiece::Atomic { projection, .. } => {
                self.receipt.atomic_pieces_visited =
                    self.receipt.atomic_pieces_visited.checked_add(1).ok_or(
                        ActiveParagraphProjectionError::Overflow(
                            "active Paragraph atomic Program pieces",
                        ),
                    )?;
                self.install_byte_state(
                    coverage,
                    part,
                    physical_metric,
                    logical_metric,
                    ByteProjectionMapping::Atomic(projection.kind),
                    Some(page),
                )
            }
            ProjectionPiece::Virtual { kind } => {
                self.receipt.virtual_pieces_visited =
                    self.receipt.virtual_pieces_visited.checked_add(1).ok_or(
                        ActiveParagraphProjectionError::Overflow(
                            "active Paragraph virtual Program pieces",
                        ),
                    )?;
                self.install_byte_state(
                    coverage,
                    part,
                    physical_metric,
                    logical_metric,
                    ByteProjectionMapping::Virtual(kind),
                    Some(page),
                )
            }
        }
    }

    fn emit_byte<S: ParagraphPhysicalSource>(
        &mut self,
        source: &mut S,
    ) -> Result<(), ActiveParagraphProjectionError> {
        let Some(mut state) = self.byte_state.take() else {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("missing active Paragraph byte state"),
            ));
        };
        let relative = state.next_byte;
        if relative >= state.logical_len() {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("active Paragraph byte state escaped its range"),
            ));
        }
        let byte = match state.mapping {
            ByteProjectionMapping::Identity => {
                let absolute = state.physical.start.checked_add(relative).ok_or(
                    ActiveParagraphProjectionError::Overflow("active Paragraph physical byte"),
                )?;
                let absolute = usize::try_from(absolute).map_err(|_| {
                    ActiveParagraphProjectionError::Overflow(
                        "active Paragraph physical byte exceeds usize",
                    )
                })?;
                let byte = source.byte_at(absolute)?;
                self.receipt.identity_source_bytes_read = self
                    .receipt
                    .identity_source_bytes_read
                    .checked_add(1)
                    .ok_or(ActiveParagraphProjectionError::Overflow(
                        "active Paragraph source reads",
                    ))?;
                byte
            }
            ByteProjectionMapping::Atomic(kind) => atomic_output_byte(kind, relative)?,
            ByteProjectionMapping::Virtual(VirtualProjectionKind::LineFeed) => b'\n',
        };
        let (scalar_complete, utf16_delta) = consume_projected_utf8(&mut state, byte)?;
        let raw_codepoint_contribution = match state.mapping {
            ByteProjectionMapping::Identity => u8::from(scalar_complete),
            ByteProjectionMapping::Atomic(AtomicProjectionKind::TabToSpaces { .. }) => {
                u8::from(relative == 0)
            }
            ByteProjectionMapping::Atomic(AtomicProjectionKind::CrLfToLf) => 2,
            ByteProjectionMapping::Atomic(AtomicProjectionKind::LoneCrToLf) => 1,
            ByteProjectionMapping::Atomic(AtomicProjectionKind::NulToReplacement) => {
                u8::from(scalar_complete)
            }
            ByteProjectionMapping::Virtual(_) => 0,
        };
        let offset = usize::try_from(state.logical.start.checked_add(relative).ok_or(
            ActiveParagraphProjectionError::Overflow("active Paragraph logical byte"),
        )?)
        .map_err(|_| {
            ActiveParagraphProjectionError::Overflow("active Paragraph logical byte exceeds usize")
        })?;
        state.next_byte += 1;
        self.paragraph_logical_position.bytes =
            self.paragraph_logical_position.bytes.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("active Paragraph logical bytes"),
            )?;
        self.global_logical_position.bytes =
            self.global_logical_position.bytes.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("active Paragraph global logical bytes"),
            )?;
        if scalar_complete {
            self.paragraph_logical_position.utf16 = self
                .paragraph_logical_position
                .utf16
                .checked_add(utf16_delta)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph logical UTF-16",
                ))?;
            self.global_logical_position.utf16 = self
                .global_logical_position
                .utf16
                .checked_add(utf16_delta)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph global logical UTF-16",
                ))?;
        }
        self.ready = Some(ReadyLogicalByte {
            offset,
            byte,
            raw_codepoint_contribution,
            coverage: Some(state.coverage),
            mapping: state.mapping,
            physical: state.physical.clone(),
            logical: state.logical.clone(),
            program: state.program,
            delivered: false,
        });
        self.receipt.maximum_ready_byte_cache_bytes = self
            .receipt
            .maximum_ready_byte_cache_bytes
            .max(ACTIVE_PARAGRAPH_MAX_READY_BYTES);
        self.receipt.logical_bytes_yielded =
            self.receipt.logical_bytes_yielded.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("active Paragraph yielded bytes"),
            )?;
        if state.complete() {
            if state.utf8_remaining != 0
                || self.paragraph_logical_position.bytes != state.logical.end
                || self.paragraph_logical_position.utf16 != state.logical_utf16.end
            {
                return Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt(
                        "active Paragraph projection output metric mismatch",
                    ),
                ));
            }
            self.physical_position = SerializedMetric {
                bytes: state.physical.end,
                utf16: state.physical_utf16.end,
            };
        } else {
            self.byte_state = Some(state);
        }
        Ok(())
    }

    fn advance_leaf(
        &mut self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
    ) -> Result<bool, ActiveParagraphProjectionError> {
        if matches!(self.leaf, CursorLeaf::Partial) {
            return Ok(false);
        }
        while let Some(mut frame) = self.route.pop() {
            if frame.went_right {
                continue;
            }
            let (_, SequenceNodeKind::Branch { left, right }) =
                sequence_node::<SerializedGreenSpec>(session.arena(), frame.branch)?
            else {
                return Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt("active Paragraph route branch became a leaf"),
                ));
            };
            let left_summary = sequence_node::<SerializedGreenSpec>(session.arena(), left)?.0;
            frame.went_right = true;
            let leaf_index = frame
                .base_leaf_index
                .checked_add(left_summary.leaves)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph successor leaf index",
                ))?;
            push_route_frame(&mut self.route, frame)?;
            let mut node = right;
            loop {
                self.receipt.sequence_nodes_visited =
                    self.receipt.sequence_nodes_visited.checked_add(1).ok_or(
                        ActiveParagraphProjectionError::Overflow(
                            "active Paragraph successor sequence nodes",
                        ),
                    )?;
                match sequence_node::<SerializedGreenSpec>(session.arena(), node)?.1 {
                    SequenceNodeKind::Leaf => {
                        let payload_bytes = session
                            .arena()
                            .payload(node)
                            .map_err(SerializedGreenError::from)?
                            .len();
                        let (_, events) = decode_leaf(session.arena(), node)?;
                        self.receipt.leaf_pages_decoded =
                            self.receipt.leaf_pages_decoded.checked_add(1).ok_or(
                                ActiveParagraphProjectionError::Overflow(
                                    "active Paragraph decoded leaves",
                                ),
                            )?;
                        self.receipt.events_decoded = self
                            .receipt
                            .events_decoded
                            .checked_add(events.len())
                            .ok_or(ActiveParagraphProjectionError::Overflow(
                                "active Paragraph decoded events",
                            ))?;
                        self.receipt.maximum_decoded_page_bytes =
                            self.receipt.maximum_decoded_page_bytes.max(
                                payload_bytes
                                    .checked_add(
                                        events.capacity() * std::mem::size_of::<DecodedLeafEvent>(),
                                    )
                                    .ok_or(ActiveParagraphProjectionError::Overflow(
                                        "active Paragraph decoded page bytes",
                                    ))?,
                            );
                        self.receipt.maximum_route_depth =
                            self.receipt.maximum_route_depth.max(self.route.len());
                        self.leaf = CursorLeaf::Sealed {
                            id: node,
                            index: leaf_index,
                        };
                        self.events = events;
                        self.next_event = 0;
                        return Ok(true);
                    }
                    SequenceNodeKind::Branch { left, .. } => {
                        push_route_frame(
                            &mut self.route,
                            BuildRouteFrame {
                                branch: node,
                                base_leaf_index: leaf_index,
                                went_right: false,
                            },
                        )?;
                        node = left;
                    }
                }
            }
        }
        let current_index = match self.leaf {
            CursorLeaf::Sealed { id: _, index } => index,
            CursorLeaf::Partial => unreachable!("partial leaf returned above"),
        };
        if current_index + 1 != self.stamp.sealed_leaves {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("active Paragraph successor route ended early"),
            ));
        }
        let events = decode_partial_leaf(build, session)?;
        self.receipt.partial_leaf_decodes =
            self.receipt.partial_leaf_decodes.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("active Paragraph partial leaf decodes"),
            )?;
        self.receipt.events_decoded = self
            .receipt
            .events_decoded
            .checked_add(events.len())
            .ok_or(ActiveParagraphProjectionError::Overflow(
                "active Paragraph partial events",
            ))?;
        self.receipt.maximum_decoded_page_bytes = self.receipt.maximum_decoded_page_bytes.max(
            build
                .leaf
                .bytes
                .len()
                .checked_add(events.capacity() * std::mem::size_of::<DecodedLeafEvent>())
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph partial page bytes",
                ))?,
        );
        self.leaf = CursorLeaf::Partial;
        self.events = events;
        self.next_event = 0;
        Ok(true)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn poll_byte<S: ParagraphPhysicalSource>(
        &mut self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        source: &mut S,
        cancelled: bool,
    ) -> Result<ActiveParagraphProjectionPoll, ActiveParagraphProjectionError> {
        if cancelled {
            self.cancel();
            return Ok(ActiveParagraphProjectionPoll::Cancelled);
        }
        if self.cancelled {
            return Err(ActiveParagraphProjectionError::CursorCancelled);
        }
        self.validate_live(build, session, binding)?;
        if source.source_root() != self.identity.source_root
            || source.source_revision() != self.identity.source_revision
            || source.source_extent_bytes() != self.identity.source_bytes
        {
            return Err(ActiveParagraphProjectionError::WrongSource);
        }
        self.receipt.polls =
            self.receipt
                .polls
                .checked_add(1)
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph polls",
                ))?;
        if self.ready.as_ref().is_some_and(|ready| !ready.delivered) {
            return Ok(ActiveParagraphProjectionPoll::ByteReady);
        }
        if self.ready.as_ref().is_some_and(|ready| ready.delivered) {
            self.ready = None;
        }
        if self.complete {
            return Ok(ActiveParagraphProjectionPoll::Complete);
        }
        if self.byte_state.is_some() {
            self.emit_byte(source)?;
            return Ok(ActiveParagraphProjectionPoll::ByteReady);
        }
        if self.pending_program.is_some() {
            self.process_program_piece(session.arena())?;
            return Ok(ActiveParagraphProjectionPoll::Pending);
        }
        if self.next_event < self.events.len() {
            let decoded = self.events[self.next_event].clone();
            self.next_event += 1;
            self.process_event(session.arena(), decoded)?;
            return Ok(ActiveParagraphProjectionPoll::Pending);
        }
        if self.advance_leaf(build, session)? {
            return Ok(ActiveParagraphProjectionPoll::Pending);
        }
        let green_physical_end = self
            .stamp
            .prefix
            .metric
            .checked_add(self.stamp.partial.metric)?;
        let green_logical_end = self
            .stamp
            .prefix
            .logical_metric
            .checked_add_logical(self.stamp.partial.logical_metric)?;
        if self.physical_position != green_physical_end
            || self.global_logical_position != green_logical_end
        {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt(
                    "active Paragraph cursor ended before the builder high-water",
                ),
            ));
        }
        if let Some(terminator) = self.staged_terminator {
            if self.staged_terminator_emitted {
                return Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt("staged Paragraph terminator was emitted twice"),
                ));
            }
            let physical_metric = terminator.kind.physical_metric();
            let physical_end = terminator.source_start.checked_add(physical_metric)?;
            let logical_start = self.paragraph_logical_position;
            let logical_end =
                logical_start.checked_add_logical(SerializedMetric { bytes: 1, utf16: 1 })?;
            self.ready = Some(ReadyLogicalByte {
                offset: usize::try_from(logical_start.bytes).map_err(|_| {
                    ActiveParagraphProjectionError::Overflow(
                        "staged Paragraph logical offset exceeds usize",
                    )
                })?,
                byte: b'\n',
                raw_codepoint_contribution: terminator.kind.raw_codepoint_contribution(),
                coverage: None,
                mapping: match terminator.kind {
                    StagedTerminatorKind::Lf => ByteProjectionMapping::Identity,
                    StagedTerminatorKind::CrLf => {
                        ByteProjectionMapping::Atomic(AtomicProjectionKind::CrLfToLf)
                    }
                    StagedTerminatorKind::LoneCr => {
                        ByteProjectionMapping::Atomic(AtomicProjectionKind::LoneCrToLf)
                    }
                },
                physical: terminator.source_start.bytes..physical_end.bytes,
                logical: logical_start.bytes..logical_end.bytes,
                program: None,
                delivered: false,
            });
            self.receipt.maximum_ready_byte_cache_bytes = self
                .receipt
                .maximum_ready_byte_cache_bytes
                .max(ACTIVE_PARAGRAPH_MAX_READY_BYTES);
            self.physical_position = physical_end;
            self.paragraph_logical_position = logical_end;
            self.global_logical_position = self
                .global_logical_position
                .checked_add_logical(SerializedMetric { bytes: 1, utf16: 1 })?;
            self.receipt.logical_bytes_yielded =
                self.receipt.logical_bytes_yielded.checked_add(1).ok_or(
                    ActiveParagraphProjectionError::Overflow("staged Paragraph yielded byte"),
                )?;
            self.staged_terminator_emitted = true;
            self.complete = true;
            return Ok(ActiveParagraphProjectionPoll::ByteReady);
        }
        self.complete = true;
        Ok(ActiveParagraphProjectionPoll::Complete)
    }
}

pub(crate) struct ActiveParagraphDirectSource<'a> {
    cursor: &'a mut ActiveParagraphProjectionCursor,
}

impl DirectReferencePrefixSource for ActiveParagraphDirectSource<'_> {
    type Identity = ActiveParagraphProjectionIdentity;
    type Error = ActiveParagraphProjectionError;

    fn identity(&self) -> Self::Identity {
        self.cursor.identity
    }

    fn available_len(&self) -> usize {
        usize::try_from(self.cursor.paragraph_logical_position.bytes).unwrap_or(usize::MAX)
    }

    fn is_final(&self) -> bool {
        self.cursor.complete
    }

    fn access_budget(&self) -> usize {
        usize::from(
            self.cursor
                .ready
                .as_ref()
                .is_some_and(|ready| !ready.delivered),
        )
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        let ready = self
            .cursor
            .ready
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::SourceOutOfBounds)?;
        if ready.delivered || ready.offset != relative_offset {
            return Err(ActiveParagraphProjectionError::NonSequentialSource);
        }
        ready.delivered = true;
        Ok(ready.byte)
    }

    fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8 {
        self.cursor.ready.as_ref().map_or(0, |ready| {
            if ready.offset == logical_scalar_end_offset && ready.delivered {
                ready.raw_codepoint_contribution
            } else {
                0
            }
        })
    }
}

impl ActiveParagraphRangeReplayCursor {
    pub(crate) fn poll_byte<S: ParagraphPhysicalSource>(
        &mut self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        source: &mut S,
        cancelled: bool,
    ) -> Result<ActiveParagraphProjectionPoll, ActiveParagraphProjectionError> {
        if cancelled {
            if let Some(inner) = self.inner.as_mut() {
                inner.cancel();
            }
            self.cancelled = true;
            return Ok(ActiveParagraphProjectionPoll::Cancelled);
        }
        if self.cancelled {
            return Err(ActiveParagraphProjectionError::CursorCancelled);
        }
        if self.complete {
            return Ok(ActiveParagraphProjectionPoll::Complete);
        }
        let inner = self
            .inner
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?;
        if inner.identity != self.identity {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        inner.validate_live(build, session, binding)?;
        if !metric_at_or_before(inner.paragraph_logical_position, self.end) {
            return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
        }
        if inner.ready.as_ref().is_some_and(|ready| !ready.delivered) {
            return Ok(ActiveParagraphProjectionPoll::ByteReady);
        }
        if inner.paragraph_logical_position == self.end {
            inner.ready = None;
            self.complete = true;
            return Ok(ActiveParagraphProjectionPoll::Complete);
        }
        match inner.poll_byte(build, session, binding, source, false)? {
            ActiveParagraphProjectionPoll::Pending => Ok(ActiveParagraphProjectionPoll::Pending),
            ActiveParagraphProjectionPoll::ByteReady => {
                if !metric_at_or_before(inner.paragraph_logical_position, self.end) {
                    return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
                }
                Ok(ActiveParagraphProjectionPoll::ByteReady)
            }
            ActiveParagraphProjectionPoll::Complete => {
                Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds)
            }
            ActiveParagraphProjectionPoll::Cancelled => {
                self.cancelled = true;
                Ok(ActiveParagraphProjectionPoll::Cancelled)
            }
        }
    }

    pub(crate) fn direct_source(
        &mut self,
        identity: ActiveParagraphProjectionIdentity,
    ) -> Result<ActiveParagraphRangeDirectSource<'_>, ActiveParagraphProjectionError> {
        if identity != self.identity {
            return Err(ActiveParagraphProjectionError::CrossedCursor);
        }
        Ok(ActiveParagraphRangeDirectSource { replay: self })
    }

    pub(crate) fn take_completed(
        mut self,
    ) -> Result<ActiveParagraphCompletedRangeReplay, ActiveParagraphProjectionError> {
        if !self.complete || self.cancelled {
            return Err(ActiveParagraphProjectionError::CapabilityNotReady);
        }
        let stream = self.inner.as_ref().map_or(
            ActiveParagraphProjectionReceipt::default(),
            ActiveParagraphProjectionCursor::receipt,
        );
        Ok(ActiveParagraphCompletedRangeReplay {
            capability: self
                .capability
                .take()
                .ok_or(ActiveParagraphProjectionError::CrossedCursor)?,
            receipt: ActiveParagraphRangeReplayReceipt {
                seek: self.seek_receipt,
                stream,
            },
        })
    }
}

impl ActiveParagraphRangeReplayPass {
    pub(crate) fn identity(&self) -> ActiveParagraphProjectionIdentity {
        self.replay.as_ref().map_or_else(
            || unreachable!("completed replay pass retained no cursor"),
            |replay| replay.identity,
        )
    }

    pub(crate) fn poll_byte(
        &mut self,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        cancelled: bool,
    ) -> Result<ActiveParagraphProjectionPoll, ActiveParagraphProjectionError> {
        let replay = self
            .replay
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?;
        let source = self
            .source
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?;
        match replay.poll_byte(build, session, binding, source, cancelled) {
            Ok(ActiveParagraphProjectionPoll::Cancelled) => {
                source.cancel_in_place()?;
                Ok(ActiveParagraphProjectionPoll::Cancelled)
            }
            Ok(status) => Ok(status),
            Err(error) => {
                // Stale/crossed/faulted replay cannot leave a source cursor
                // installed. Keep the original parser/projection failure;
                // the exact cleanup should be infallible for this owned pass.
                let _ = source.cancel_in_place();
                Err(error)
            }
        }
    }

    pub(crate) fn direct_source(
        &mut self,
        identity: ActiveParagraphProjectionIdentity,
    ) -> Result<ActiveParagraphRangeDirectSource<'_>, ActiveParagraphProjectionError> {
        self.replay
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?
            .direct_source(identity)
    }

    pub(crate) fn take_completed(
        mut self,
    ) -> Result<
        (ActiveParagraphCompletedRangeReplay, SourceProjectionSession),
        ActiveParagraphProjectionError,
    > {
        let replay = self
            .replay
            .take()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?;
        let completed = replay.take_completed()?;
        let source = self
            .source
            .take()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?;
        let source_session = source.finish()?;
        Ok((completed, source_session))
    }

    pub(crate) fn cancel(
        mut self,
    ) -> Result<SourceProjectionSession, ActiveParagraphProjectionError> {
        if let Some(replay) = self.replay.as_mut() {
            if let Some(inner) = replay.inner.as_mut() {
                inner.cancel();
            }
            replay.cancelled = true;
        }
        let source = self
            .source
            .take()
            .ok_or(ActiveParagraphProjectionError::CursorComplete)?;
        source.cancel()
    }
}

pub(crate) struct ActiveParagraphRangeDirectSource<'a> {
    replay: &'a mut ActiveParagraphRangeReplayCursor,
}

impl DirectReferencePrefixSource for ActiveParagraphRangeDirectSource<'_> {
    type Identity = ActiveParagraphProjectionIdentity;
    type Error = ActiveParagraphProjectionError;

    fn identity(&self) -> Self::Identity {
        self.replay.identity
    }

    fn available_len(&self) -> usize {
        self.replay.inner.as_ref().map_or(0, |inner| {
            usize::try_from(
                inner
                    .paragraph_logical_position
                    .bytes
                    .saturating_sub(self.replay.start.bytes),
            )
            .unwrap_or(usize::MAX)
        })
    }

    fn is_final(&self) -> bool {
        self.replay
            .inner
            .as_ref()
            .is_none_or(|inner| inner.paragraph_logical_position == self.replay.end)
    }

    fn access_budget(&self) -> usize {
        self.replay.inner.as_ref().map_or(0, |inner| {
            usize::from(inner.ready.as_ref().is_some_and(|ready| !ready.delivered))
        })
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        let absolute = self
            .replay
            .start
            .bytes
            .checked_add(u64::try_from(relative_offset).map_err(|_| {
                ActiveParagraphProjectionError::Overflow(
                    "active Paragraph range replay relative offset",
                )
            })?)
            .ok_or(ActiveParagraphProjectionError::Overflow(
                "active Paragraph range replay logical offset",
            ))?;
        let inner = self
            .replay
            .inner
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::SourceOutOfBounds)?;
        let ready = inner
            .ready
            .as_mut()
            .ok_or(ActiveParagraphProjectionError::SourceOutOfBounds)?;
        if ready.delivered
            || u64::try_from(ready.offset).map_err(|_| {
                ActiveParagraphProjectionError::Overflow(
                    "active Paragraph range replay ready offset",
                )
            })? != absolute
        {
            return Err(ActiveParagraphProjectionError::NonSequentialSource);
        }
        ready.delivered = true;
        Ok(ready.byte)
    }

    fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8 {
        let Some(inner) = self.replay.inner.as_ref() else {
            return 0;
        };
        let Ok(relative) = u64::try_from(logical_scalar_end_offset) else {
            return 0;
        };
        let Some(absolute) = self.replay.start.bytes.checked_add(relative) else {
            return 0;
        };
        inner.ready.as_ref().map_or(0, |ready| {
            if u64::try_from(ready.offset) == Ok(absolute) && ready.delivered {
                ready.raw_codepoint_contribution
            } else {
                0
            }
        })
    }
}

fn build_range_replay_cursor(
    parent: &ActiveParagraphProjectionCursor,
    build: &ResumableSerializedGreenBuild,
    session: &ArenaBuildSession<'_>,
    start: SerializedMetric,
) -> Result<
    (
        ActiveParagraphProjectionCursor,
        ActiveParagraphProjectionSeekReceipt,
    ),
    ActiveParagraphProjectionError,
> {
    let green_global_end = parent
        .stamp
        .prefix
        .logical_metric
        .checked_add_logical(parent.stamp.partial.logical_metric)?;
    let green_paragraph_end = green_global_end.checked_sub(parent.paragraph_global_logical_base)?;
    let mut seek_receipt = ActiveParagraphProjectionSeekReceipt::default();

    let (route, leaf, events, next_event, physical_position, byte_state, pending_program) =
        if start == green_paragraph_end {
            if parent.staged_terminator.is_none() {
                return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
            }
            (
                Vec::new(),
                CursorLeaf::Partial,
                Vec::new(),
                0,
                parent
                    .staged_terminator
                    .ok_or(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds)?
                    .source_start,
                None,
                None,
            )
        } else {
            if !metric_at_or_before(start, green_paragraph_end) {
                return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
            }
            let global_start = parent
                .paragraph_global_logical_base
                .checked_add_logical(start)?;
            let (route, leaf, events, physical_base, logical_base) =
                if global_start.bytes < parent.stamp.prefix.logical_metric.bytes {
                    let root = parent
                        .stamp
                        .root
                        .ok_or(ActiveParagraphProjectionError::StaleBinding)?;
                    let (route, leaf, leaf_index, physical_base, logical_base) =
                        descend_to_logical_byte_with_route(
                            session.arena(),
                            root,
                            global_start.bytes,
                            &mut seek_receipt,
                        )?;
                    let payload_bytes = session
                        .arena()
                        .payload(leaf)
                        .map_err(SerializedGreenError::from)?
                        .len();
                    let (_, events) = decode_leaf(session.arena(), leaf)?;
                    seek_receipt.leaf_pages_decoded = 1;
                    seek_receipt.events_decoded = events.len();
                    seek_receipt.maximum_decoded_page_bytes = decoded_page_scratch_bytes(
                        payload_bytes,
                        events.capacity(),
                        "active Paragraph replay decoded page bytes",
                    )?;
                    (
                        route,
                        CursorLeaf::Sealed {
                            id: leaf,
                            index: leaf_index,
                        },
                        events,
                        physical_base,
                        logical_base,
                    )
                } else {
                    let events = decode_partial_leaf(build, session)?;
                    seek_receipt.partial_leaf_decodes = 1;
                    seek_receipt.events_decoded = events.len();
                    seek_receipt.maximum_decoded_page_bytes = decoded_page_scratch_bytes(
                        build.leaf.bytes.len(),
                        events.capacity(),
                        "active Paragraph replay partial page bytes",
                    )?;
                    (
                        Vec::new(),
                        CursorLeaf::Partial,
                        events,
                        parent.stamp.prefix.metric,
                        parent.stamp.prefix.logical_metric,
                    )
                };
            let state = locate_replay_start_in_events(
                session.arena(),
                &events,
                physical_base,
                logical_base,
                parent.paragraph_global_logical_base,
                global_start,
                &mut seek_receipt,
            )?;
            (
                route,
                leaf,
                events,
                state.next_event,
                state.physical_position,
                Some(state.byte_state),
                state.pending_program,
            )
        };

    Ok((
        ActiveParagraphProjectionCursor {
            identity: parent.identity,
            stamp: parent.stamp,
            target_active: parent.target_active,
            route,
            leaf,
            events,
            next_event,
            physical_position,
            global_logical_position: parent
                .paragraph_global_logical_base
                .checked_add_logical(start)?,
            paragraph_global_logical_base: parent.paragraph_global_logical_base,
            paragraph_logical_position: start,
            pending_program,
            byte_state,
            ready: None,
            staged_terminator: parent.staged_terminator,
            staged_terminator_emitted: false,
            complete: false,
            cancelled: false,
            receipt: ActiveParagraphProjectionReceipt {
                maximum_route_depth: seek_receipt.maximum_route_depth,
                ..ActiveParagraphProjectionReceipt::default()
            },
        },
        seek_receipt,
    ))
}

fn decoded_page_scratch_bytes(
    payload_bytes: usize,
    event_capacity: usize,
    overflow: &'static str,
) -> Result<usize, ActiveParagraphProjectionError> {
    payload_bytes
        .checked_add(
            event_capacity
                .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                .ok_or(ActiveParagraphProjectionError::Overflow(overflow))?,
        )
        .ok_or(ActiveParagraphProjectionError::Overflow(overflow))
}

fn descend_to_logical_byte_with_route(
    arena: &PageArena,
    root: ArenaId,
    global_logical_probe: u64,
    receipt: &mut ActiveParagraphProjectionSeekReceipt,
) -> Result<
    (
        Vec<BuildRouteFrame>,
        ArenaId,
        u64,
        SerializedMetric,
        SerializedMetric,
    ),
    ActiveParagraphProjectionError,
> {
    let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if global_logical_probe >= root_summary.logical_metric.bytes {
        return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
    }
    receipt.root_descents =
        receipt
            .root_descents
            .checked_add(1)
            .ok_or(ActiveParagraphProjectionError::Overflow(
                "active Paragraph replay root descents",
            ))?;
    let mut node = root;
    let mut physical = SerializedMetric::default();
    let mut logical = SerializedMetric::default();
    let mut leaf_index = 0_u64;
    let mut route = Vec::new();
    loop {
        receipt.sequence_nodes_visited = receipt.sequence_nodes_visited.checked_add(1).ok_or(
            ActiveParagraphProjectionError::Overflow("active Paragraph replay sequence nodes"),
        )?;
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                receipt.maximum_route_depth = receipt.maximum_route_depth.max(route.len());
                return Ok((route, node, leaf_index, physical, logical));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                let left_logical_end = logical
                    .bytes
                    .checked_add(left_summary.logical_metric.bytes)
                    .ok_or(ActiveParagraphProjectionError::Overflow(
                        "active Paragraph replay left logical end",
                    ))?;
                if global_logical_probe < left_logical_end {
                    push_route_frame(
                        &mut route,
                        BuildRouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: false,
                        },
                    )?;
                    node = left;
                } else {
                    receipt.summary_nodes_skipped = receipt
                        .summary_nodes_skipped
                        .checked_add(1)
                        .ok_or(ActiveParagraphProjectionError::Overflow(
                            "active Paragraph replay skipped summaries",
                        ))?;
                    push_route_frame(
                        &mut route,
                        BuildRouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: true,
                        },
                    )?;
                    physical = physical.checked_add(left_summary.metric)?;
                    logical = logical.checked_add_logical(left_summary.logical_metric)?;
                    leaf_index = leaf_index.checked_add(left_summary.leaves).ok_or(
                        ActiveParagraphProjectionError::Overflow(
                            "active Paragraph replay leaf index",
                        ),
                    )?;
                    node = right;
                }
            }
        }
    }
}

fn locate_replay_start_in_events(
    arena: &PageArena,
    events: &[DecodedLeafEvent],
    mut physical: SerializedMetric,
    mut logical: SerializedMetric,
    paragraph_global_logical_base: SerializedMetric,
    global_start: SerializedMetric,
    receipt: &mut ActiveParagraphProjectionSeekReceipt,
) -> Result<ReplayStartState, ActiveParagraphProjectionError> {
    for (index, decoded) in events.iter().enumerate() {
        receipt.events_inspected = receipt.events_inspected.checked_add(1).ok_or(
            ActiveParagraphProjectionError::Overflow("active Paragraph replay inspected events"),
        )?;
        let DecodedGreenEventKind::Coverage(run) = &decoded.event else {
            continue;
        };
        let physical_end = physical.checked_add(run.metric)?;
        let logical_metric = run.logical_contribution.summary_metric(run.metric);
        let logical_end = logical.checked_add_logical(logical_metric)?;
        if global_start.bytes >= logical.bytes && global_start.bytes < logical_end.bytes {
            let logical_start_relative = logical.checked_sub(paragraph_global_logical_base)?;
            let logical_end_relative = logical_end.checked_sub(paragraph_global_logical_base)?;
            let relative_start = global_start.checked_sub(logical)?;
            let make_state = |mapping, program| LogicalByteState {
                coverage: run.id,
                part: run.part,
                physical: physical.bytes..physical_end.bytes,
                physical_utf16: physical.utf16..physical_end.utf16,
                logical: logical_start_relative.bytes..logical_end_relative.bytes,
                logical_utf16: logical_start_relative.utf16..logical_end_relative.utf16,
                mapping,
                program,
                next_byte: relative_start.bytes,
                utf8_remaining: 0,
                utf8_codepoint: 0,
                utf8_minimum: 0,
            };
            return match &run.logical_contribution {
                DecodedLogicalContribution::Identity => {
                    // Identity can contain non-BMP or multibyte scalars; the
                    // DFA-minted dual cut is the scalar-boundary authority, so
                    // unequal byte/UTF-16 deltas require no source rescan.
                    Ok(ReplayStartState {
                        next_event: index + 1,
                        physical_position: physical,
                        byte_state: make_state(ByteProjectionMapping::Identity, None),
                        pending_program: None,
                    })
                }
                DecodedLogicalContribution::Atomic(projection) => {
                    map_logical_boundary(
                        LocatedProjectionSegment {
                            physical: ProjectionMetricRange::new(physical, physical_end)?,
                            logical: ProjectionMetricRange::new(
                                logical_start_relative,
                                logical_end_relative,
                            )?,
                            mapping: LocatedProjectionMapping::Atomic(projection.kind),
                        },
                        global_start.checked_sub(paragraph_global_logical_base)?,
                    )?;
                    Ok(ReplayStartState {
                        next_event: index + 1,
                        physical_position: physical,
                        byte_state: make_state(
                            ByteProjectionMapping::Atomic(projection.kind),
                            None,
                        ),
                        pending_program: None,
                    })
                }
                DecodedLogicalContribution::Program(program) => locate_program_replay_start(
                    arena,
                    run.id,
                    run.part,
                    *program,
                    physical,
                    logical,
                    paragraph_global_logical_base,
                    global_start,
                    index + 1,
                    receipt,
                ),
                DecodedLogicalContribution::None | DecodedLogicalContribution::Hidden { .. } => {
                    Err(ActiveParagraphProjectionError::Green(
                        SerializedGreenError::Corrupt(
                            "zero-logical coverage selected for active Paragraph replay",
                        ),
                    ))
                }
            };
        }
        physical = physical_end;
        logical = logical_end;
    }
    Err(ActiveParagraphProjectionError::Green(
        SerializedGreenError::Corrupt(
            "active Paragraph replay start was absent from its selected leaf",
        ),
    ))
}

#[allow(clippy::too_many_arguments)]
fn locate_program_replay_start(
    arena: &PageArena,
    coverage: CoverageId,
    part: CoveragePart,
    program: RetainedProgramRef,
    mut physical: SerializedMetric,
    mut logical: SerializedMetric,
    paragraph_global_logical_base: SerializedMetric,
    global_start: SerializedMetric,
    next_event: usize,
    receipt: &mut ActiveParagraphProjectionSeekReceipt,
) -> Result<ReplayStartState, ActiveParagraphProjectionError> {
    let page = program.retained_page()?;
    let first_piece = validate_projection_program_edge_payload(
        arena,
        page,
        usize::from(program.piece_count),
        program.physical_metric,
        program.logical_metric,
    )?;
    let payload = arena.payload(page).map_err(SerializedGreenError::from)?;
    receipt.projection_program_pages_decoded = receipt
        .projection_program_pages_decoded
        .checked_add(1)
        .ok_or(ActiveParagraphProjectionError::Overflow(
            "active Paragraph replay Program pages",
        ))?;
    receipt.projection_program_bytes_validated = receipt
        .projection_program_bytes_validated
        .checked_add(payload.len())
        .ok_or(ActiveParagraphProjectionError::Overflow(
            "active Paragraph replay Program bytes",
        ))?;
    receipt.maximum_program_scratch_bytes = receipt
        .maximum_program_scratch_bytes
        .max(std::mem::size_of::<ProjectionPiece>() + std::mem::size_of::<Decoder<'_>>());
    let expected_physical_end = physical.checked_add(program.physical_metric)?;
    let expected_logical_global_end = logical.checked_add_logical(program.logical_metric)?;
    let expected_logical_relative_end =
        expected_logical_global_end.checked_sub(paragraph_global_logical_base)?;
    let mut decoder = Decoder::new(payload);
    decoder.cursor = first_piece;
    for ordinal in 0..program.piece_count {
        let piece = decode_projection_piece(&mut decoder)?;
        receipt.projection_pieces_decoded =
            receipt.projection_pieces_decoded.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("active Paragraph replay Program pieces"),
            )?;
        let (physical_metric, logical_metric) = piece.metrics();
        let physical_end = physical.checked_add(physical_metric)?;
        let logical_end = logical.checked_add_logical(logical_metric)?;
        if global_start.bytes >= logical.bytes && global_start.bytes < logical_end.bytes {
            let logical_start_relative = logical.checked_sub(paragraph_global_logical_base)?;
            let logical_end_relative = logical_end.checked_sub(paragraph_global_logical_base)?;
            let relative_start = global_start.checked_sub(logical)?;
            let (mapping, located_mapping) = match piece {
                ProjectionPiece::Identity { .. } => (
                    ByteProjectionMapping::Identity,
                    LocatedProjectionMapping::Identity,
                ),
                ProjectionPiece::Atomic { projection, .. } => (
                    ByteProjectionMapping::Atomic(projection.kind),
                    LocatedProjectionMapping::Atomic(projection.kind),
                ),
                ProjectionPiece::Virtual { kind } => (
                    ByteProjectionMapping::Virtual(kind),
                    LocatedProjectionMapping::Virtual(kind),
                ),
                ProjectionPiece::Hidden { .. } => {
                    return Err(ActiveParagraphProjectionError::Green(
                        SerializedGreenError::Corrupt(
                            "zero-logical Program piece selected for replay",
                        ),
                    ));
                }
            };
            map_logical_boundary(
                LocatedProjectionSegment {
                    physical: ProjectionMetricRange::new(physical, physical_end)?,
                    logical: ProjectionMetricRange::new(
                        logical_start_relative,
                        logical_end_relative,
                    )?,
                    mapping: located_mapping,
                },
                global_start.checked_sub(paragraph_global_logical_base)?,
            )?;
            return Ok(ReplayStartState {
                next_event,
                physical_position: physical,
                byte_state: LogicalByteState {
                    coverage,
                    part,
                    physical: physical.bytes..physical_end.bytes,
                    physical_utf16: physical.utf16..physical_end.utf16,
                    logical: logical_start_relative.bytes..logical_end_relative.bytes,
                    logical_utf16: logical_start_relative.utf16..logical_end_relative.utf16,
                    mapping,
                    program: Some(page),
                    next_byte: relative_start.bytes,
                    utf8_remaining: 0,
                    utf8_codepoint: 0,
                    utf8_minimum: 0,
                },
                pending_program: Some(PendingBuildProgram {
                    coverage,
                    part,
                    page,
                    next_byte: decoder.cursor,
                    pieces_remaining: program.piece_count - ordinal - 1,
                    physical_position: physical_end,
                    expected_physical_end,
                    expected_logical_end: expected_logical_relative_end,
                }),
            });
        }
        physical = physical_end;
        logical = logical_end;
    }
    Err(ActiveParagraphProjectionError::Green(
        SerializedGreenError::Corrupt("active Paragraph replay start was absent from its Program"),
    ))
}

fn metric_at_or_before(left: SerializedMetric, right: SerializedMetric) -> bool {
    left.bytes <= right.bytes && left.utf16 <= right.utf16
}

fn validate_direct_logical_range(
    range: &DirectReferenceLogicalRange,
) -> Result<(), ActiveParagraphProjectionError> {
    if range.bytes.start > range.bytes.end || range.utf16.start > range.utf16.end {
        return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
    }
    Ok(())
}

fn map_logical_boundary(
    segment: LocatedProjectionSegment,
    target: SerializedMetric,
) -> Result<ActiveParagraphProjectedBoundaryObservation, ActiveParagraphProjectionError> {
    if !metric_at_or_before(segment.logical.start, target)
        || !metric_at_or_before(target, segment.logical.end)
    {
        return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
    }
    let at_start = target == segment.logical.start;
    let at_end = target == segment.logical.end;
    match segment.mapping {
        LocatedProjectionMapping::Identity => {
            let logical_delta = target.checked_sub(segment.logical.start)?;
            let physical = segment.physical.start.checked_add(logical_delta)?;
            if !metric_at_or_before(physical, segment.physical.end) {
                return Err(ActiveParagraphProjectionError::LogicalBoundaryMetricMismatch);
            }
            Ok(ActiveParagraphProjectedBoundaryObservation::ExactSource { physical })
        }
        LocatedProjectionMapping::Atomic(transform) => {
            if at_start {
                Ok(ActiveParagraphProjectedBoundaryObservation::ExactSource {
                    physical: segment.physical.start,
                })
            } else if at_end {
                Ok(ActiveParagraphProjectedBoundaryObservation::ExactSource {
                    physical: segment.physical.end,
                })
            } else {
                Ok(
                    ActiveParagraphProjectedBoundaryObservation::AtomicAmbiguity {
                        physical: segment.physical,
                        logical: segment.logical,
                        transform,
                    },
                )
            }
        }
        LocatedProjectionMapping::Virtual(kind) => {
            if at_start || at_end {
                Ok(ActiveParagraphProjectedBoundaryObservation::ExactSource {
                    physical: segment.physical.start,
                })
            } else {
                Ok(ActiveParagraphProjectedBoundaryObservation::Virtual {
                    physical_boundary: segment.physical.start,
                    logical: segment.logical,
                    kind,
                })
            }
        }
    }
}

fn locate_projection_segment(
    build: &ResumableSerializedGreenBuild,
    session: &ArenaBuildSession<'_>,
    stamp: BuilderProjectionStamp,
    global_logical_probe: u64,
) -> Result<
    (
        LocatedProjectionSegment,
        ActiveParagraphProjectionSeekReceipt,
    ),
    ActiveParagraphProjectionError,
> {
    let total = stamp
        .prefix
        .logical_metric
        .checked_add_logical(stamp.partial.logical_metric)?;
    if global_logical_probe >= total.bytes {
        return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
    }
    let mut receipt = ActiveParagraphProjectionSeekReceipt::default();
    let (events, physical_base, logical_base) =
        if global_logical_probe < stamp.prefix.logical_metric.bytes {
            let root = stamp
                .root
                .ok_or(ActiveParagraphProjectionError::StaleBinding)?;
            let (leaf, physical_base, logical_base) =
                descend_to_logical_byte(session.arena(), root, global_logical_probe, &mut receipt)?;
            let payload_bytes = session
                .arena()
                .payload(leaf)
                .map_err(SerializedGreenError::from)?
                .len();
            let (_, events) = decode_leaf(session.arena(), leaf)?;
            receipt.leaf_pages_decoded = 1;
            receipt.events_decoded = events.len();
            receipt.maximum_decoded_page_bytes = payload_bytes
                .checked_add(
                    events
                        .capacity()
                        .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                        .ok_or(ActiveParagraphProjectionError::Overflow(
                            "active Paragraph seek decoded event bytes",
                        ))?,
                )
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph seek decoded page bytes",
                ))?;
            (events, physical_base, logical_base)
        } else {
            let events = decode_partial_leaf(build, session)?;
            receipt.partial_leaf_decodes = 1;
            receipt.events_decoded = events.len();
            receipt.maximum_decoded_page_bytes = build
                .leaf
                .bytes
                .len()
                .checked_add(
                    events
                        .capacity()
                        .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
                        .ok_or(ActiveParagraphProjectionError::Overflow(
                            "active Paragraph seek partial event bytes",
                        ))?,
                )
                .ok_or(ActiveParagraphProjectionError::Overflow(
                    "active Paragraph seek partial page bytes",
                ))?;
            (events, stamp.prefix.metric, stamp.prefix.logical_metric)
        };

    locate_projection_segment_in_events(
        session.arena(),
        &events,
        physical_base,
        logical_base,
        global_logical_probe,
        &mut receipt,
    )
    .map(|segment| (segment, receipt))
}

fn descend_to_logical_byte(
    arena: &PageArena,
    root: ArenaId,
    global_logical_probe: u64,
    receipt: &mut ActiveParagraphProjectionSeekReceipt,
) -> Result<(ArenaId, SerializedMetric, SerializedMetric), ActiveParagraphProjectionError> {
    let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if global_logical_probe >= root_summary.logical_metric.bytes {
        return Err(ActiveParagraphProjectionError::LogicalBoundaryOutOfBounds);
    }
    receipt.root_descents = 1;
    let mut node = root;
    let mut physical = SerializedMetric::default();
    let mut logical = SerializedMetric::default();
    let mut depth = 0_usize;
    loop {
        depth = depth
            .checked_add(1)
            .ok_or(ActiveParagraphProjectionError::Overflow(
                "active Paragraph seek depth",
            ))?;
        if depth > ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH {
            return Err(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt(
                    "active Paragraph logical seek exceeds its hard depth bound",
                ),
            ));
        }
        receipt.maximum_route_depth = receipt.maximum_route_depth.max(depth);
        receipt.sequence_nodes_visited = receipt.sequence_nodes_visited.checked_add(1).ok_or(
            ActiveParagraphProjectionError::Overflow("active Paragraph seek sequence nodes"),
        )?;
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => return Ok((node, physical, logical)),
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                let left_logical_end = logical
                    .bytes
                    .checked_add(left_summary.logical_metric.bytes)
                    .ok_or(ActiveParagraphProjectionError::Overflow(
                        "active Paragraph seek left logical end",
                    ))?;
                if global_logical_probe < left_logical_end {
                    node = left;
                } else {
                    receipt.summary_nodes_skipped = receipt
                        .summary_nodes_skipped
                        .checked_add(1)
                        .ok_or(ActiveParagraphProjectionError::Overflow(
                            "active Paragraph seek skipped summaries",
                        ))?;
                    physical = physical.checked_add(left_summary.metric)?;
                    logical = logical.checked_add_logical(left_summary.logical_metric)?;
                    node = right;
                }
            }
        }
    }
}

fn locate_projection_segment_in_events(
    arena: &PageArena,
    events: &[DecodedLeafEvent],
    mut physical: SerializedMetric,
    mut logical: SerializedMetric,
    global_logical_probe: u64,
    receipt: &mut ActiveParagraphProjectionSeekReceipt,
) -> Result<LocatedProjectionSegment, ActiveParagraphProjectionError> {
    for decoded in events {
        receipt.events_inspected = receipt.events_inspected.checked_add(1).ok_or(
            ActiveParagraphProjectionError::Overflow("active Paragraph seek inspected events"),
        )?;
        let DecodedGreenEventKind::Coverage(run) = &decoded.event else {
            continue;
        };
        let physical_end = physical.checked_add(run.metric)?;
        let logical_metric = run.logical_contribution.summary_metric(run.metric);
        let logical_end = logical.checked_add_logical(logical_metric)?;
        if global_logical_probe >= logical.bytes && global_logical_probe < logical_end.bytes {
            return match &run.logical_contribution {
                DecodedLogicalContribution::Identity => Ok(LocatedProjectionSegment {
                    physical: ProjectionMetricRange::new(physical, physical_end)?,
                    logical: ProjectionMetricRange::new(logical, logical_end)?,
                    mapping: LocatedProjectionMapping::Identity,
                }),
                DecodedLogicalContribution::Atomic(projection) => {
                    projection.validate_kind().map_err(|_| {
                        ActiveParagraphProjectionError::Green(SerializedGreenError::Corrupt(
                            "active Paragraph seek has invalid atomic projection",
                        ))
                    })?;
                    projection.validate_physical(run.metric).map_err(|_| {
                        ActiveParagraphProjectionError::Green(SerializedGreenError::Corrupt(
                            "active Paragraph seek has invalid atomic physical metric",
                        ))
                    })?;
                    Ok(LocatedProjectionSegment {
                        physical: ProjectionMetricRange::new(physical, physical_end)?,
                        logical: ProjectionMetricRange::new(logical, logical_end)?,
                        mapping: LocatedProjectionMapping::Atomic(projection.kind),
                    })
                }
                DecodedLogicalContribution::Program(program) => locate_program_segment(
                    arena,
                    *program,
                    physical,
                    logical,
                    global_logical_probe,
                    receipt,
                ),
                DecodedLogicalContribution::None | DecodedLogicalContribution::Hidden { .. } => {
                    Err(ActiveParagraphProjectionError::Green(
                        SerializedGreenError::Corrupt(
                            "zero-logical coverage selected by logical seek",
                        ),
                    ))
                }
            };
        }
        physical = physical_end;
        logical = logical_end;
    }
    Err(ActiveParagraphProjectionError::Green(
        SerializedGreenError::Corrupt(
            "active Paragraph logical summary did not locate a projected segment",
        ),
    ))
}

fn locate_program_segment(
    arena: &PageArena,
    program: RetainedProgramRef,
    mut physical: SerializedMetric,
    mut logical: SerializedMetric,
    global_logical_probe: u64,
    receipt: &mut ActiveParagraphProjectionSeekReceipt,
) -> Result<LocatedProjectionSegment, ActiveParagraphProjectionError> {
    let page = program.retained_page()?;
    let first_piece = validate_projection_program_edge_payload(
        arena,
        page,
        usize::from(program.piece_count),
        program.physical_metric,
        program.logical_metric,
    )?;
    let payload = arena.payload(page).map_err(SerializedGreenError::from)?;
    receipt.projection_program_pages_decoded = receipt
        .projection_program_pages_decoded
        .checked_add(1)
        .ok_or(ActiveParagraphProjectionError::Overflow(
            "active Paragraph seek Program pages",
        ))?;
    receipt.projection_program_bytes_validated = receipt
        .projection_program_bytes_validated
        .checked_add(payload.len())
        .ok_or(ActiveParagraphProjectionError::Overflow(
            "active Paragraph seek Program bytes",
        ))?;
    receipt.maximum_program_scratch_bytes = receipt
        .maximum_program_scratch_bytes
        .max(std::mem::size_of::<ProjectionPiece>() + std::mem::size_of::<Decoder<'_>>());
    let expected_physical_end = physical.checked_add(program.physical_metric)?;
    let expected_logical_end = logical.checked_add_logical(program.logical_metric)?;
    let mut decoder = Decoder::new(payload);
    decoder.cursor = first_piece;
    for _ in 0..program.piece_count {
        let piece = decode_projection_piece(&mut decoder)?;
        receipt.projection_pieces_decoded =
            receipt.projection_pieces_decoded.checked_add(1).ok_or(
                ActiveParagraphProjectionError::Overflow("active Paragraph seek Program pieces"),
            )?;
        let (physical_metric, logical_metric) = piece.metrics();
        let physical_end = physical.checked_add(physical_metric)?;
        let logical_end = logical.checked_add_logical(logical_metric)?;
        if global_logical_probe >= logical.bytes && global_logical_probe < logical_end.bytes {
            let mapping = match piece {
                ProjectionPiece::Identity { .. } => LocatedProjectionMapping::Identity,
                ProjectionPiece::Atomic { projection, .. } => {
                    LocatedProjectionMapping::Atomic(projection.kind)
                }
                ProjectionPiece::Virtual { kind } => LocatedProjectionMapping::Virtual(kind),
                ProjectionPiece::Hidden { .. } => {
                    return Err(ActiveParagraphProjectionError::Green(
                        SerializedGreenError::Corrupt(
                            "zero-logical Program piece selected by logical seek",
                        ),
                    ));
                }
            };
            return Ok(LocatedProjectionSegment {
                physical: ProjectionMetricRange::new(physical, physical_end)?,
                logical: ProjectionMetricRange::new(logical, logical_end)?,
                mapping,
            });
        }
        physical = physical_end;
        logical = logical_end;
    }
    if physical != expected_physical_end || logical != expected_logical_end || !decoder.is_empty() {
        return Err(ActiveParagraphProjectionError::Green(
            SerializedGreenError::Corrupt(
                "active Paragraph seek Program ended on the wrong partition",
            ),
        ));
    }
    Err(ActiveParagraphProjectionError::Green(
        SerializedGreenError::Corrupt(
            "active Paragraph Program summary did not locate a projected segment",
        ),
    ))
}

fn atomic_output_byte(
    kind: AtomicProjectionKind,
    relative: u64,
) -> Result<u8, ActiveParagraphProjectionError> {
    match kind {
        AtomicProjectionKind::TabToSpaces { spaces } => {
            if relative < u64::from(spaces) {
                Ok(b' ')
            } else {
                Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt("tab projection escaped its logical output"),
                ))
            }
        }
        AtomicProjectionKind::CrLfToLf | AtomicProjectionKind::LoneCrToLf => {
            if relative == 0 {
                Ok(b'\n')
            } else {
                Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt(
                        "line-ending projection escaped its logical output",
                    ),
                ))
            }
        }
        AtomicProjectionKind::NulToReplacement => [0xef, 0xbf, 0xbd]
            .get(
                usize::try_from(relative)
                    .map_err(|_| ActiveParagraphProjectionError::Overflow("NUL projection byte"))?,
            )
            .copied()
            .ok_or(ActiveParagraphProjectionError::Green(
                SerializedGreenError::Corrupt("NUL projection escaped its logical output"),
            )),
    }
}

fn consume_projected_utf8(
    state: &mut LogicalByteState,
    byte: u8,
) -> Result<(bool, u64), ActiveParagraphProjectionError> {
    if state.utf8_remaining == 0 {
        match byte {
            0x00..=0x7f => return Ok((true, 1)),
            0xc2..=0xdf => {
                state.utf8_remaining = 1;
                state.utf8_codepoint = u32::from(byte & 0x1f);
                state.utf8_minimum = 0x80;
            }
            0xe0..=0xef => {
                state.utf8_remaining = 2;
                state.utf8_codepoint = u32::from(byte & 0x0f);
                state.utf8_minimum = 0x800;
            }
            0xf0..=0xf4 => {
                state.utf8_remaining = 3;
                state.utf8_codepoint = u32::from(byte & 0x07);
                state.utf8_minimum = 0x1_0000;
            }
            _ => {
                return Err(ActiveParagraphProjectionError::Green(
                    SerializedGreenError::Corrupt(
                        "active Paragraph identity projection is not UTF-8",
                    ),
                ));
            }
        }
        return Ok((false, 0));
    }
    if !(0x80..=0xbf).contains(&byte) {
        return Err(ActiveParagraphProjectionError::Green(
            SerializedGreenError::Corrupt("active Paragraph identity projection is not UTF-8"),
        ));
    }
    state.utf8_codepoint = (state.utf8_codepoint << 6) | u32::from(byte & 0x3f);
    state.utf8_remaining -= 1;
    if state.utf8_remaining != 0 {
        return Ok((false, 0));
    }
    if state.utf8_codepoint < state.utf8_minimum
        || (0xd800..=0xdfff).contains(&state.utf8_codepoint)
        || state.utf8_codepoint > 0x10_ffff
    {
        return Err(ActiveParagraphProjectionError::Green(
            SerializedGreenError::Corrupt("active Paragraph identity projection is not UTF-8"),
        ));
    }
    Ok((true, u64::from(state.utf8_codepoint > 0xffff) + 1))
}

fn select_paragraph_enter(
    events: &[DecodedLeafEvent],
    byte_offset: u16,
    block: BlockId,
) -> Result<usize, ActiveParagraphProjectionError> {
    events
        .iter()
        .position(|decoded| {
            decoded.byte_offset == byte_offset
                && matches!(
                    decoded.event,
                    DecodedGreenEventKind::Enter { block: found, kind, .. }
                        if found == block && kind == GreenKind::PARAGRAPH
                )
        })
        .ok_or(ActiveParagraphProjectionError::CrossedParagraph)
}

fn push_route_frame(
    route: &mut Vec<BuildRouteFrame>,
    frame: BuildRouteFrame,
) -> Result<(), ActiveParagraphProjectionError> {
    if route.len() == ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH {
        return Err(ActiveParagraphProjectionError::Green(
            SerializedGreenError::Corrupt("active Paragraph route exceeds its hard bound"),
        ));
    }
    route.push(frame);
    Ok(())
}

fn descend_to_leaf(
    arena: &PageArena,
    root: ArenaId,
    target_leaf_index: u64,
    receipt: &mut ActiveParagraphProjectionReceipt,
) -> Result<
    (
        Vec<BuildRouteFrame>,
        ArenaId,
        SerializedMetric,
        SerializedMetric,
    ),
    ActiveParagraphProjectionError,
> {
    let summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    if target_leaf_index >= summary.leaves {
        return Err(ActiveParagraphProjectionError::StaleBinding);
    }
    receipt.root_descents =
        receipt
            .root_descents
            .checked_add(1)
            .ok_or(ActiveParagraphProjectionError::Overflow(
                "active Paragraph root descents",
            ))?;
    let mut route = Vec::new();
    let mut node = root;
    let mut remaining = target_leaf_index;
    let mut leaf_index = 0_u64;
    let mut physical = SerializedMetric::default();
    let mut logical = SerializedMetric::default();
    loop {
        receipt.sequence_nodes_visited = receipt.sequence_nodes_visited.checked_add(1).ok_or(
            ActiveParagraphProjectionError::Overflow("active Paragraph sequence nodes"),
        )?;
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                if remaining != 0 || leaf_index != target_leaf_index {
                    return Err(ActiveParagraphProjectionError::StaleBinding);
                }
                return Ok((route, node, physical, logical));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                if remaining < left_summary.leaves {
                    push_route_frame(
                        &mut route,
                        BuildRouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: false,
                        },
                    )?;
                    node = left;
                } else {
                    push_route_frame(
                        &mut route,
                        BuildRouteFrame {
                            branch: node,
                            base_leaf_index: leaf_index,
                            went_right: true,
                        },
                    )?;
                    remaining -= left_summary.leaves;
                    leaf_index = leaf_index.checked_add(left_summary.leaves).ok_or(
                        ActiveParagraphProjectionError::Overflow("active Paragraph leaf index"),
                    )?;
                    physical = physical.checked_add(left_summary.metric)?;
                    logical = logical.checked_add_logical(left_summary.logical_metric)?;
                    node = right;
                }
            }
        }
    }
}

fn decode_partial_leaf(
    build: &ResumableSerializedGreenBuild,
    session: &ArenaBuildSession<'_>,
) -> Result<Vec<DecodedLeafEvent>, ActiveParagraphProjectionError> {
    if build.leaf.bytes.len() < LEAF_HEADER_BYTES || build.leaf.bytes.len() > ARENA_PAGE_BYTES {
        return Err(ActiveParagraphProjectionError::Green(
            SerializedGreenError::Corrupt("partial green leaf escaped its page bound"),
        ));
    }
    let mut decoder = Decoder::new(&build.leaf.bytes[LEAF_HEADER_BYTES..]);
    let mut events = Vec::new();
    let mut next_program_ordinal = 0_usize;
    let mut actual = GreenSummary::default();
    while !decoder.is_empty() {
        let offset = u16::try_from(LEAF_HEADER_BYTES + decoder.cursor)
            .map_err(|_| ActiveParagraphProjectionError::Overflow("partial leaf offset"))?;
        let event = decode_event_with_program_resolver(
            &mut decoder,
            &mut next_program_ordinal,
            |ordinal| {
                let owner =
                    build
                        .leaf
                        .programs
                        .get(ordinal)
                        .ok_or(SerializedGreenError::Corrupt(
                            "partial green leaf Program ordinal is out of range",
                        ))?;
                session.owner_id(owner).map(Some).map_err(Into::into)
            },
        )?;
        actual = actual.followed_by(GreenSummary::decoded_event(&event))?;
        events.push(DecodedLeafEvent {
            byte_offset: offset,
            event,
        });
    }
    if next_program_ordinal != build.leaf.programs.len() || actual != build.leaf.summary {
        return Err(ActiveParagraphProjectionError::Green(
            SerializedGreenError::Corrupt("partial green leaf summary mismatch"),
        ));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentIdentityAllocator;
    use flark_reference_value_service::DestinationTrimProbe;

    #[derive(Debug)]
    struct FixturePhysicalSource<'a> {
        root: SourceRootId,
        revision: SourceRevision,
        bytes: &'a [u8],
        reads: Vec<usize>,
    }

    impl<'a> FixturePhysicalSource<'a> {
        fn new(spec: &SerializedGreenRootSpec, bytes: &'a [u8]) -> Self {
            Self {
                root: spec.source_root,
                revision: spec.source_revision,
                bytes,
                reads: Vec::new(),
            }
        }
    }

    impl ParagraphPhysicalSource for FixturePhysicalSource<'_> {
        fn source_root(&self) -> SourceRootId {
            self.root
        }

        fn source_revision(&self) -> SourceRevision {
            self.revision
        }

        fn source_extent_bytes(&self) -> usize {
            self.bytes.len()
        }

        fn byte_at(&mut self, absolute: usize) -> Result<u8, ActiveParagraphProjectionError> {
            let byte = self
                .bytes
                .get(absolute)
                .copied()
                .ok_or(ActiveParagraphProjectionError::SourceOutOfBounds)?;
            self.reads.push(absolute);
            Ok(byte)
        }
    }

    fn spec(metric: SerializedMetric) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(11),
            source_root: SourceRootId(13),
            source_bytes: metric.bytes,
            source_utf16: metric.utf16,
            grammar_revision: GrammarRevision(17),
            parse_generation: ParseGeneration(19),
            semantic_epoch: 23,
            known_bytes: 0..metric.bytes,
        }
    }

    fn poll_builder_to_boundary(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("active Paragraph fixture unexpectedly completed")
                }
            }
        }
    }

    fn offer_event(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        build.offer_event(session, event).unwrap();
        poll_builder_to_boundary(build, session);
    }

    fn offer_paragraph(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        block: BlockId,
    ) -> ProvisionalParagraphEnter {
        build
            .offer_provisional_paragraph_enter(session, block, FactsEnvelope::empty())
            .unwrap();
        poll_builder_to_boundary(build, session);
        build
            .take_provisional_paragraph_enter(session, block)
            .unwrap()
    }

    fn force_leaf_barrier(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        build.begin_leaf_barrier(session).unwrap();
        poll_builder_to_boundary(build, session);
        let _ = build.take_leaf_barrier_cut(session).unwrap();
    }

    fn reduce_prefix(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        build.begin_working_prefix_reduction(session).unwrap();
        poll_builder_to_boundary(build, session);
        let _ = build.take_working_prefix_cut(session).unwrap();
    }

    fn identity_run(id: u64, block: BlockId, metric: SerializedMetric) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(id),
                metric.bytes,
                metric.utf16,
                0,
                CoveragePart::CONTENT,
                block,
                LogicalContribution::Identity,
            )
            .unwrap(),
        )
    }

    fn drain_cursor<S: ParagraphPhysicalSource>(
        cursor: &mut ActiveParagraphProjectionCursor,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        source: &mut S,
    ) -> (Vec<u8>, Vec<u8>, Vec<ReadyLogicalByte>) {
        let mut bytes = Vec::new();
        let mut raw_codepoints = Vec::new();
        let mut ready_values = Vec::new();
        for _ in 0..1_000_000 {
            match cursor
                .poll_byte(build, session, binding, source, false)
                .unwrap()
            {
                ActiveParagraphProjectionPoll::Pending => {}
                ActiveParagraphProjectionPoll::ByteReady => {
                    let ready = cursor.ready.clone().expect("ByteReady has one byte");
                    let identity = cursor.identity();
                    let mut direct = cursor.direct_source(identity).unwrap();
                    assert_eq!(direct.access_budget(), 1);
                    assert_eq!(direct.available_len(), bytes.len() + 1);
                    let byte = direct.read_byte(bytes.len()).unwrap();
                    assert_eq!(byte, ready.byte);
                    raw_codepoints.push(direct.raw_codepoint_contribution(bytes.len()));
                    bytes.push(byte);
                    ready_values.push(ready);
                }
                ActiveParagraphProjectionPoll::Complete => {
                    return (bytes, raw_codepoints, ready_values);
                }
                ActiveParagraphProjectionPoll::Cancelled => {
                    panic!("successful projection cursor was cancelled")
                }
            }
        }
        panic!("active Paragraph projection cursor did not complete")
    }

    fn drain_replay<S: ParagraphPhysicalSource>(
        replay: &mut ActiveParagraphRangeReplayCursor,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
        source: &mut S,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..1_000_000 {
            match replay
                .poll_byte(build, session, binding, source, false)
                .unwrap()
            {
                ActiveParagraphProjectionPoll::Pending => {}
                ActiveParagraphProjectionPoll::ByteReady => {
                    let identity = replay.identity;
                    let mut direct = replay.direct_source(identity).unwrap();
                    assert_eq!(direct.access_budget(), 1);
                    assert_eq!(direct.available_len(), bytes.len() + 1);
                    bytes.push(direct.read_byte(bytes.len()).unwrap());
                }
                ActiveParagraphProjectionPoll::Complete => return bytes,
                ActiveParagraphProjectionPoll::Cancelled => {
                    panic!("successful active Paragraph range replay was cancelled")
                }
            }
        }
        panic!("active Paragraph range replay did not complete")
    }

    fn drain_owned_replay(
        replay: &mut ActiveParagraphRangeReplayPass,
        build: &ResumableSerializedGreenBuild,
        session: &ArenaBuildSession<'_>,
        binding: ActorProjectionBinding,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..1_000_000 {
            match replay.poll_byte(build, session, binding, false).unwrap() {
                ActiveParagraphProjectionPoll::Pending => {}
                ActiveParagraphProjectionPoll::ByteReady => {
                    let identity = replay.identity();
                    let mut direct = replay.direct_source(identity).unwrap();
                    assert_eq!(direct.access_budget(), 1);
                    assert_eq!(direct.available_len(), bytes.len() + 1);
                    bytes.push(direct.read_byte(bytes.len()).unwrap());
                }
                ActiveParagraphProjectionPoll::Complete => return bytes,
                ActiveParagraphProjectionPoll::Cancelled => {
                    panic!("successful owned active Paragraph replay was cancelled")
                }
            }
        }
        panic!("owned active Paragraph range replay did not complete")
    }

    fn settle_aborted_fixture(arena: &mut PageArena, abort: ArenaBuildId) {
        while !arena.poll_build_abort(abort, 1_000).unwrap().complete {}
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1_000).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    fn reference_rewrite_pass_from_seal(
        seal: ActiveParagraphProjectionTransactionSeal,
        disposition: ActiveParagraphReferenceRewriteDisposition,
        prefix_physical_end: SerializedMetric,
        split_suffix_coverage: Option<FreshCoveragePermit>,
    ) -> ActiveParagraphReferenceRewritePass {
        let first_event_offset = match seal.paragraph_storage {
            ProvisionalParagraphStorage::Sealed { byte_offset, .. } => usize::from(byte_offset),
            ProvisionalParagraphStorage::Partial { .. } => {
                panic!("rewrite fixture must seal the Paragraph")
            }
        };
        ActiveParagraphReferenceRewritePass {
            root: seal.root,
            leaf_range: seal.covered_leaf_range.clone(),
            next_leaf_index: seal.covered_leaf_range.start,
            current_leaf: None,
            event_cursor: 0,
            next_program_ordinal: 0,
            expected_leaf_summary: None,
            actual_leaf_summary: GreenSummary::default(),
            first_event_offset,
            paragraph: seal.identity.paragraph,
            disposition,
            physical_position: seal.paragraph_physical.start,
            physical_end: seal.paragraph_physical.end,
            prefix_physical_end,
            saw_paragraph_enter: false,
            survivor_emitted: false,
            replacement_prefix_runs: 0,
            split_suffix_coverage,
            pending_split_suffix: None,
            complete: false,
        }
    }

    fn drive_reference_rewrite_pass(
        pass: &mut ActiveParagraphReferenceRewritePass,
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        for _ in 0..1_000_000 {
            match pass.poll(session.arena()).unwrap() {
                ActiveParagraphReferenceRewriteAction::Pending => {}
                ActiveParagraphReferenceRewriteAction::Event(event) => {
                    build
                        .offer_canonical_fragment_event(session, event)
                        .unwrap();
                    poll_builder_to_boundary(build, session);
                }
                ActiveParagraphReferenceRewriteAction::SurvivingParagraphEnter => {
                    build
                        .offer_canonical_fragment_surviving_paragraph_enter(session)
                        .unwrap();
                    poll_builder_to_boundary(build, session);
                }
                ActiveParagraphReferenceRewriteAction::Complete => return,
            }
        }
        panic!("reference canonical rewrite did not complete")
    }

    fn finish_builder(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        build.finish_input(session).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => return,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("finished Green builder became writable")
                }
            }
        }
    }

    fn settle_committed_fixture(arena: &mut PageArena, document: SerializedGreenDocument) {
        document.release_later(arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1_000).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn reference_only_rewrite_streams_old_paragraph_source_as_parent_gap() {
        let source_bytes = b"[x]: /u";
        let physical = SerializedMetric { bytes: 7, utf16: 7 };
        let root_spec = spec(physical);
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        offer_event(
            &mut build,
            &mut session,
            identity_run(1, BlockId(2), physical),
        );
        force_leaf_barrier(&mut build, &mut session);
        reduce_prefix(&mut build, &mut session);

        let binding = ActorProjectionBinding::mechanism_only(&build, &paragraph, 83, physical, 89);
        let mut cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let mut source = FixturePhysicalSource::new(&root_spec, source_bytes);
        assert_eq!(
            drain_cursor(&mut cursor, &build, &session, binding, &mut source).0,
            source_bytes
        );
        let seal = cursor
            .into_transaction_seal(&build, &session, binding)
            .unwrap();
        let mut pass = reference_rewrite_pass_from_seal(
            seal,
            ActiveParagraphReferenceRewriteDisposition::ReferenceOnly,
            physical,
            None,
        );

        build
            .begin_canonical_fragment_removal(
                &mut session,
                paragraph,
                BlockId(1),
                GreenKind::DOCUMENT,
                physical,
            )
            .unwrap();
        poll_builder_to_boundary(&mut build, &mut session);
        drive_reference_rewrite_pass(&mut pass, &mut build, &mut session);
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_boundary(&mut build, &mut session);
        let removal = build
            .take_canonical_fragment_removal(&session, BlockId(1))
            .unwrap();
        assert_eq!(removal.retired_block(), BlockId(2));
        assert_eq!(removal.physical_metric(), physical);
        assert_eq!(removal.retired_coverage_runs(), 1);
        assert_eq!(removal.replacement_coverage_runs(), 1);

        offer_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_builder(&mut build, &mut session);
        let document = build.take_manifest().unwrap().commit(session).unwrap().0;
        assert_eq!(
            serialized_green_test_trace(&document, &arena).unwrap(),
            vec![
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(1),
                    metric: physical,
                    owner_relative_depth: 0,
                    part: CoveragePart::GAP,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Exit,
            ]
        );
        settle_committed_fixture(&mut arena, document);
    }

    #[test]
    fn visible_remainder_rewrite_restores_paragraph_at_identity_run_boundary() {
        let prefix = SerializedMetric { bytes: 8, utf16: 8 };
        let suffix = SerializedMetric { bytes: 4, utf16: 4 };
        let physical = prefix.checked_add(suffix).unwrap();
        let source_bytes = b"[x]: /u\nrest";
        let root_spec = spec(physical);
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        let mut identities = DocumentIdentityAllocator::default();
        let original_coverage = identities.mint_coverage(session.id()).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    original_coverage.id(),
                    physical.bytes,
                    physical.utf16,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        force_leaf_barrier(&mut build, &mut session);
        reduce_prefix(&mut build, &mut session);

        let binding = ActorProjectionBinding::mechanism_only(&build, &paragraph, 97, physical, 101);
        let mut cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let mut source = FixturePhysicalSource::new(&root_spec, source_bytes);
        assert_eq!(
            drain_cursor(&mut cursor, &build, &session, binding, &mut source).0,
            source_bytes
        );
        let seal = cursor
            .into_transaction_seal(&build, &session, binding)
            .unwrap();
        let suffix_coverage = identities.mint_coverage(session.id()).unwrap();
        let mut pass = reference_rewrite_pass_from_seal(
            seal,
            ActiveParagraphReferenceRewriteDisposition::VisibleRemainder,
            prefix,
            Some(suffix_coverage),
        );

        build
            .begin_canonical_fragment_replacement(
                &mut session,
                paragraph,
                BlockId(2),
                GreenKind::PARAGRAPH,
                physical,
            )
            .unwrap();
        poll_builder_to_boundary(&mut build, &mut session);
        drive_reference_rewrite_pass(&mut pass, &mut build, &mut session);
        build
            .finish_canonical_fragment_replacement(&mut session)
            .unwrap();
        poll_builder_to_boundary(&mut build, &mut session);
        let replacement = build
            .take_canonical_fragment_replacement(&session, BlockId(2))
            .unwrap();
        let survivor = build
            .take_provisional_paragraph_enter(&session, BlockId(2))
            .unwrap();
        assert_eq!(replacement.retired_block(), BlockId(2));
        assert_eq!(replacement.physical_metric(), physical);
        assert_eq!(replacement.retired_coverage_runs(), 1);
        assert_eq!(replacement.replacement_coverage_runs(), 2);
        assert_eq!(survivor.block, BlockId(2));
        assert_eq!(survivor.source_before, prefix);

        offer_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finish_builder(&mut build, &mut session);
        let document = build.take_manifest().unwrap().commit(session).unwrap().0;
        assert_eq!(
            serialized_green_test_trace(&document, &arena).unwrap(),
            vec![
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(1),
                    metric: prefix,
                    owner_relative_depth: 0,
                    part: CoveragePart::GAP,
                    logical: SerializedGreenTestLogical::None,
                },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(2),
                    kind: GreenKind::PARAGRAPH,
                },
                SerializedGreenTestEvent::Coverage {
                    coverage: CoverageId(2),
                    metric: suffix,
                    owner_relative_depth: 0,
                    part: CoveragePart::CONTENT,
                    logical: SerializedGreenTestLogical::Identity,
                },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
            ]
        );
        settle_committed_fixture(&mut arena, document);
    }

    #[test]
    fn same_builder_cursor_streams_sealed_and_partial_projection_without_a_second_tree() {
        let source_bytes = b"a!\t\0\r\nz";
        let physical = SerializedMetric { bytes: 7, utf16: 7 };
        let source_store = SourceStore::new("a!\t\0\r\nz", 8);
        let source_descriptor = source_store.descriptor();
        let mut root_spec = spec(physical);
        root_spec.source_revision = source_descriptor.revision;
        root_spec.source_root = source_descriptor.root;
        root_spec.source_bytes = u64::try_from(source_descriptor.bytes).unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        offer_event(
            &mut build,
            &mut session,
            identity_run(1, BlockId(2), SerializedMetric { bytes: 1, utf16: 1 }),
        );
        force_leaf_barrier(&mut build, &mut session);
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Hidden {
                        affinity: GreenAffinity::Downstream,
                    },
                )
                .unwrap(),
            ),
        );
        force_leaf_barrier(&mut build, &mut session);
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(3),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Atomic(AtomicProjection::tab_to_spaces(2).unwrap()),
                )
                .unwrap(),
            ),
        );
        force_leaf_barrier(&mut build, &mut session);
        let program = ProjectionProgram::new(vec![
            ProjectionPiece::Atomic {
                physical_metric: SerializedMetric { bytes: 1, utf16: 1 },
                projection: AtomicProjection::nul_to_replacement(),
            },
            ProjectionPiece::Atomic {
                physical_metric: SerializedMetric { bytes: 2, utf16: 2 },
                projection: AtomicProjection::crlf_to_lf(),
            },
            ProjectionPiece::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            },
            ProjectionPiece::Identity {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
            },
        ])
        .unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(4),
                    4,
                    4,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Program(program),
                )
                .unwrap(),
            ),
        );
        reduce_prefix(&mut build, &mut session);

        let binding = ActorProjectionBinding::mechanism_only(&build, &paragraph, 41, physical, 43);
        assert_eq!(
            binding,
            ActorProjectionBinding::from_writer_join(
                SourceSnapshotDescriptor {
                    revision: root_spec.source_revision,
                    root: root_spec.source_root,
                    bytes: source_bytes.len(),
                },
                &paragraph,
                41,
                physical,
                43,
            )
        );
        let mut cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let initial_source_start = cursor.source_start().unwrap();
        assert_eq!(initial_source_start.identity(), cursor.identity());
        assert_eq!(
            initial_source_start.source(),
            SourceSnapshotDescriptor {
                revision: root_spec.source_revision,
                root: root_spec.source_root,
                bytes: source_bytes.len(),
            }
        );
        assert_eq!(initial_source_start.physical_lower_bound_bytes(), 0);
        let projection_session = cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();
        let mut source_pass = cursor
            .source_start()
            .unwrap()
            .begin_source_pass(projection_session)
            .unwrap();
        let (logical, raw, _) =
            drain_cursor(&mut cursor, &build, &session, binding, &mut source_pass);
        let mut projection_session = source_pass.finish().unwrap();
        let mut source = FixturePhysicalSource::new(&root_spec, source_bytes);

        assert_eq!(logical, b"a  \xef\xbf\xbd\n\nz");
        assert_eq!(raw, [1, 1, 0, 0, 0, 1, 2, 0, 1]);
        assert_eq!(
            cursor.logical_end(),
            SerializedMetric { bytes: 9, utf16: 7 }
        );
        let receipt = cursor.receipt();
        assert_eq!(receipt.root_descents, 1);
        assert_eq!(receipt.leaf_pages_decoded, 3);
        assert_eq!(receipt.partial_leaf_decodes, 1);
        assert_eq!(receipt.coverage_runs_visited, 4);
        assert_eq!(receipt.projection_program_pages_decoded, 1);
        assert_eq!(receipt.projection_pieces_decoded, 4);
        assert_eq!(receipt.hidden_pieces_visited, 1);
        assert_eq!(receipt.atomic_pieces_visited, 3);
        assert_eq!(receipt.virtual_pieces_visited, 1);
        assert_eq!(receipt.identity_source_bytes_read, 2);
        assert_eq!(receipt.maximum_ready_byte_cache_bytes, 1);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(receipt.document_sized_event_vectors, 0);
        assert!(receipt.maximum_route_depth <= ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH);

        let (upstream, _) = cursor
            .resolve_boundary(
                &build,
                &session,
                binding,
                DirectReferenceLogicalPosition { bytes: 1, utf16: 1 },
                GreenAffinity::Upstream,
            )
            .unwrap();
        let (downstream, _) = cursor
            .resolve_boundary(
                &build,
                &session,
                binding,
                DirectReferenceLogicalPosition { bytes: 1, utf16: 1 },
                GreenAffinity::Downstream,
            )
            .unwrap();
        assert_eq!(
            upstream.observation,
            ActiveParagraphProjectedBoundaryObservation::ExactSource {
                physical: SerializedMetric { bytes: 1, utf16: 1 }
            }
        );
        assert_eq!(
            downstream.observation,
            ActiveParagraphProjectedBoundaryObservation::ExactSource {
                physical: SerializedMetric { bytes: 2, utf16: 2 }
            }
        );

        let (inside_tab, tab_receipt) = cursor
            .resolve_boundary(
                &build,
                &session,
                binding,
                DirectReferenceLogicalPosition { bytes: 2, utf16: 2 },
                GreenAffinity::Downstream,
            )
            .unwrap();
        assert_eq!(
            inside_tab.observation,
            ActiveParagraphProjectedBoundaryObservation::AtomicAmbiguity {
                physical: ProjectionMetricRange::new(
                    SerializedMetric { bytes: 2, utf16: 2 },
                    SerializedMetric { bytes: 3, utf16: 3 },
                )
                .unwrap(),
                logical: ProjectionMetricRange::new(
                    SerializedMetric { bytes: 1, utf16: 1 },
                    SerializedMetric { bytes: 3, utf16: 3 },
                )
                .unwrap(),
                transform: AtomicProjectionKind::TabToSpaces { spaces: 2 },
            }
        );
        assert_eq!(tab_receipt.root_descents, 1);
        assert_eq!(tab_receipt.leaf_pages_decoded, 1);
        assert!(tab_receipt.maximum_route_depth <= ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH);
        assert_eq!(tab_receipt.retained_source_bytes, 0);
        assert_eq!(tab_receipt.document_sized_event_vectors, 0);

        let atomic_request = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &DirectReferenceLogicalRange {
                    bytes: 2..3,
                    utf16: 2..3,
                },
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap()
            .prepare_replay()
            .unwrap();
        assert_eq!(
            atomic_request.source_start().physical_lower_bound_bytes(),
            2
        );
        assert!(matches!(
            atomic_request.capability.start.observation,
            ActiveParagraphProjectedBoundaryObservation::AtomicAmbiguity { .. }
        ));
        let mut atomic_replay = cursor
            .begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                atomic_request,
                projection_session,
            )
            .unwrap();
        assert_eq!(
            drain_owned_replay(&mut atomic_replay, &build, &session, binding),
            b" "
        );
        let (_, next_projection_session) = atomic_replay.take_completed().unwrap();
        projection_session = next_projection_session;

        let virtual_request = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &DirectReferenceLogicalRange {
                    bytes: 7..8,
                    utf16: 5..6,
                },
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap()
            .prepare_replay()
            .unwrap();
        assert_eq!(
            virtual_request.source_start().physical_lower_bound_bytes(),
            6
        );
        // A one-byte virtual LF has no interior logical boundary: both cuts
        // resolve to its exact physical anchor. Replay still emits the LF
        // without minting or reading a physical-source cursor.
        let mut virtual_replay = cursor
            .begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                virtual_request,
                projection_session,
            )
            .unwrap();
        assert_eq!(
            drain_owned_replay(&mut virtual_replay, &build, &session, binding),
            b"\n"
        );
        let (_, next_projection_session) = virtual_replay.take_completed().unwrap();
        projection_session = next_projection_session;

        let replay_range = DirectReferenceLogicalRange {
            bytes: 1..9,
            utf16: 1..7,
        };
        let capability = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &replay_range,
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        let request = capability.prepare_replay().unwrap();
        let source_start = request.source_start();
        assert_eq!(source_start.identity(), cursor.identity());
        assert_eq!(source_start.source(), binding.source());
        assert_eq!(source_start.physical_lower_bound_bytes(), 2);
        request
            .validate_source_pass_start(
                cursor.identity(),
                binding.source(),
                source_start.physical_lower_bound_bytes(),
            )
            .unwrap();
        assert_eq!(
            request.validate_source_pass_start(
                cursor.identity(),
                binding.source(),
                source_start.physical_lower_bound_bytes() + 1,
            ),
            Err(ActiveParagraphProjectionError::CrossedCursor)
        );
        let mut replay = cursor
            .begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                request,
                projection_session,
            )
            .unwrap();
        let replayed = drain_owned_replay(&mut replay, &build, &session, binding);
        assert_eq!(replayed, b"  \xef\xbf\xbd\n\nz");
        let mut trim_probe = DestinationTrimProbe::default();
        for &byte in &replayed {
            trim_probe.push(byte).unwrap();
        }
        let selected_body = trim_probe.finish();
        assert_eq!(selected_body, 2..8);
        let (completed, next_projection_session) = replay.take_completed().unwrap();
        projection_session = next_projection_session;
        assert_eq!(completed.capability.logical, replay_range);
        assert_eq!(completed.receipt.seek.root_descents, 1);
        assert_eq!(completed.receipt.seek.leaf_pages_decoded, 1);
        assert_eq!(completed.receipt.seek.retained_source_bytes, 0);
        assert_eq!(completed.receipt.seek.document_sized_event_vectors, 0);
        assert!(completed.receipt.stream.maximum_route_depth <= ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH);

        // A completed pass returns the linear capability. A second pass gets
        // fresh bounded traversal state over the same authenticated stamp,
        // which is the destination/title probe-before-cleaning chronology.
        let mut repeated_replay = cursor
            .begin_range_replay(
                &build,
                &session,
                binding,
                completed.into_capability().prepare_replay().unwrap(),
            )
            .unwrap();
        let reads_before_repeat = source.reads.len();
        assert_eq!(
            drain_replay(&mut repeated_replay, &build, &session, binding, &mut source,),
            b"  \xef\xbf\xbd\n\nz"
        );
        assert_eq!(&source.reads[reads_before_repeat..], [6]);
        let repeated = repeated_replay.take_completed().unwrap();
        assert_eq!(repeated.capability.logical, replay_range);

        // The probe trims two projected ASCII spaces. Narrowing derives the
        // dual-coordinate body without admitting a physical source guess,
        // then the cleaner's body pass replays only that selection.
        let selected_capability = cursor
            .narrow_replayed_ascii_edge_selection(
                &build,
                &session,
                binding,
                repeated.into_capability(),
                selected_body,
            )
            .unwrap();
        assert_eq!(
            selected_capability.logical,
            DirectReferenceLogicalRange {
                bytes: 3..9,
                utf16: 3..7,
            }
        );
        let mut selected_replay = cursor
            .begin_range_replay(
                &build,
                &session,
                binding,
                selected_capability.prepare_replay().unwrap(),
            )
            .unwrap();
        let reads_before_selected = source.reads.len();
        assert_eq!(
            drain_replay(&mut selected_replay, &build, &session, binding, &mut source,),
            b"\xef\xbf\xbd\n\nz"
        );
        assert_eq!(&source.reads[reads_before_selected..], [6]);
        let selected = selected_replay.take_completed().unwrap();
        assert_eq!(selected.capability.logical.bytes, 3..9);
        assert_eq!(selected.capability.logical.utf16, 3..7);

        let projection_receipt = projection_session
            .retire(binding.source(), cursor.identity().cursor_nonce())
            .unwrap();
        assert_eq!(projection_receipt.passes_started, 4);
        assert_eq!(projection_receipt.passes_finished, 4);
        assert_eq!(projection_receipt.passes_cancelled, 0);
        assert_eq!(projection_receipt.cursor_roles_minted, 3);
        assert_eq!(projection_receipt.forward_cursor_jumps, 1);
        assert_eq!(projection_receipt.source_bytes_read, 3);
        assert_eq!(projection_receipt.maximum_live_cursor_roles, 1);
        assert!(projection_receipt.maximum_chunk_bytes <= crate::SOURCE_CURSOR_COPY_CAP_BYTES);

        assert!(matches!(
            cursor.into_transaction_seal(&build, &session, binding),
            Err(ActiveParagraphProjectionError::ProjectionTransactionRequiresSealedBarrier)
        ));
        let abort = session.begin_abort().unwrap();
        drop(build);
        settle_aborted_fixture(&mut arena, abort);
    }

    #[test]
    fn staged_terminator_is_separate_writer_owned_input_and_remains_readable_at_finality() {
        let source_bytes = b"[x]: /u\r\n";
        let green = SerializedMetric { bytes: 7, utf16: 7 };
        let physical = SerializedMetric { bytes: 9, utf16: 9 };
        let source_store = SourceStore::new("[x]: /u\r\n", 8);
        let source_descriptor = source_store.descriptor();
        let mut root_spec = spec(physical);
        root_spec.source_revision = source_descriptor.revision;
        root_spec.source_root = source_descriptor.root;
        root_spec.source_bytes = u64::try_from(source_descriptor.bytes).unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        offer_event(&mut build, &mut session, identity_run(1, BlockId(2), green));
        let binding = ActorProjectionBinding::mechanism_only(&build, &paragraph, 47, physical, 53);
        let staged = StagedParagraphTerminator {
            owner_generation: 47,
            source_start: green,
            kind: StagedTerminatorKind::CrLf,
        };
        let mut cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, Some(staged))
            .unwrap();
        let mut source = FixturePhysicalSource::new(&root_spec, source_bytes);
        let (logical, raw, ready) =
            drain_cursor(&mut cursor, &build, &session, binding, &mut source);

        assert_eq!(logical, b"[x]: /u\n");
        assert_eq!(raw.last(), Some(&2));
        assert_eq!(source.reads, (0..7).collect::<Vec<_>>());
        let terminal = ready.last().unwrap();
        assert_eq!(terminal.coverage, None);
        assert_eq!(
            terminal.mapping,
            ByteProjectionMapping::Atomic(AtomicProjectionKind::CrLfToLf)
        );
        assert_eq!(terminal.physical, 7..9);
        assert_eq!(terminal.logical, 7..8);
        assert!(cursor.complete);
        assert!(cursor.ready.is_none());

        let (staged_start, start_receipt) = cursor
            .resolve_boundary(
                &build,
                &session,
                binding,
                DirectReferenceLogicalPosition { bytes: 7, utf16: 7 },
                GreenAffinity::Downstream,
            )
            .unwrap();
        assert_eq!(
            staged_start.observation,
            ActiveParagraphProjectedBoundaryObservation::ExactSource {
                physical: SerializedMetric { bytes: 7, utf16: 7 }
            }
        );
        assert_eq!(
            start_receipt,
            ActiveParagraphProjectionSeekReceipt::default()
        );
        let (staged_end, _) = cursor
            .resolve_boundary(
                &build,
                &session,
                binding,
                DirectReferenceLogicalPosition { bytes: 8, utf16: 8 },
                GreenAffinity::Upstream,
            )
            .unwrap();
        assert_eq!(
            staged_end.observation,
            ActiveParagraphProjectedBoundaryObservation::ExactSource {
                physical: SerializedMetric { bytes: 9, utf16: 9 }
            }
        );

        let staged_range = DirectReferenceLogicalRange {
            bytes: 7..8,
            utf16: 7..8,
        };
        let staged_capability = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &staged_range,
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        let staged_request = staged_capability.prepare_replay().unwrap();
        assert_eq!(
            staged_request.source_start().physical_lower_bound_bytes(),
            7
        );
        let projection_session = cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();
        let mut staged_replay = cursor
            .begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                staged_request,
                projection_session,
            )
            .unwrap();
        let replay_reads_before = source.reads.len();
        assert_eq!(
            drain_owned_replay(&mut staged_replay, &build, &session, binding),
            b"\n"
        );
        assert_eq!(source.reads.len(), replay_reads_before);
        let (completed, projection_session) = staged_replay.take_completed().unwrap();
        assert_eq!(completed.capability.logical, staged_range);
        let projection_receipt = projection_session
            .retire(binding.source(), cursor.identity().cursor_nonce())
            .unwrap();
        assert_eq!(projection_receipt.passes_started, 1);
        assert_eq!(projection_receipt.passes_finished, 1);
        assert_eq!(projection_receipt.cursor_roles_minted, 0);
        assert_eq!(projection_receipt.source_bytes_read, 0);

        let wrong_stage = StagedParagraphTerminator {
            source_start: SerializedMetric { bytes: 6, utf16: 6 },
            ..staged
        };
        assert!(matches!(
            build.open_active_paragraph_projection_cursor(
                &session,
                &paragraph,
                binding,
                Some(wrong_stage),
            ),
            Err(ActiveParagraphProjectionError::CrossedProjection(
                ActiveParagraphProjectionBindingMismatch::StagedTerminator
            ))
        ));

        drop(cursor);
        let abort = session.begin_abort().unwrap();
        drop(build);
        settle_aborted_fixture(&mut arena, abort);
    }

    #[test]
    fn every_writer_binding_dimension_and_cursor_nonce_fail_closed() {
        let physical = SerializedMetric { bytes: 1, utf16: 1 };
        let source_store = SourceStore::new("x", 8);
        let source_descriptor = source_store.descriptor();
        let mut root_spec = spec(physical);
        root_spec.source_revision = source_descriptor.revision;
        root_spec.source_root = source_descriptor.root;
        root_spec.source_bytes = u64::try_from(source_descriptor.bytes).unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        offer_event(
            &mut build,
            &mut session,
            identity_run(1, BlockId(2), physical),
        );
        let binding = ActorProjectionBinding::mechanism_only(&build, &paragraph, 59, physical, 61);
        let mut cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let second_cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        assert_ne!(cursor.identity(), second_cursor.identity());
        assert!(matches!(
            cursor.direct_source(second_cursor.identity()),
            Err(ActiveParagraphProjectionError::CrossedCursor)
        ));

        let mut source = FixturePhysicalSource::new(&root_spec, b"x");
        let mut foreign_arena = PageArena::new();
        let foreign_ticket = foreign_arena.begin_build().unwrap();
        let foreign_build = foreign_ticket.id();
        let checks = [
            (
                ActorProjectionBinding {
                    source_root: SourceRootId(999),
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::SourceRoot,
            ),
            (
                ActorProjectionBinding {
                    source_revision: SourceRevision(999),
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::SourceRevision,
            ),
            (
                ActorProjectionBinding {
                    source_bytes: 999,
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::SourceExtent,
            ),
            (
                ActorProjectionBinding {
                    build: foreign_build,
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::Build,
            ),
            (
                ActorProjectionBinding {
                    paragraph: BlockId(999),
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::Paragraph,
            ),
            (
                ActorProjectionBinding {
                    paragraph_generation: 999,
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::ParagraphGeneration,
            ),
            (
                ActorProjectionBinding {
                    projection_generation: 999,
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::ProjectionGeneration,
            ),
            (
                ActorProjectionBinding {
                    composer_high_water: SerializedMetric { bytes: 2, utf16: 2 },
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::ComposerHighWater,
            ),
            (
                ActorProjectionBinding {
                    barrier_generation: 999,
                    ..binding
                },
                ActiveParagraphProjectionBindingMismatch::BarrierGeneration,
            ),
        ];
        for (wrong, mismatch) in checks {
            assert_eq!(
                cursor.poll_byte(&build, &session, wrong, &mut source, false),
                Err(ActiveParagraphProjectionError::CrossedProjection(mismatch))
            );
        }
        source.root = SourceRootId(999);
        assert_eq!(
            cursor.poll_byte(&build, &session, binding, &mut source, false),
            Err(ActiveParagraphProjectionError::WrongSource)
        );
        source.root = root_spec.source_root;
        source.revision = SourceRevision(999);
        assert_eq!(
            cursor.poll_byte(&build, &session, binding, &mut source, false),
            Err(ActiveParagraphProjectionError::WrongSource)
        );
        source.revision = root_spec.source_revision;
        source.bytes = b"xx";
        assert_eq!(
            cursor.poll_byte(&build, &session, binding, &mut source, false),
            Err(ActiveParagraphProjectionError::WrongSource)
        );
        source.bytes = b"x";
        assert_eq!(
            cursor.poll_byte(&build, &session, binding, &mut source, false),
            Ok(ActiveParagraphProjectionPoll::Pending)
        );

        let (logical, _, _) = drain_cursor(&mut cursor, &build, &session, binding, &mut source);
        assert_eq!(logical, b"x");
        let capability = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &DirectReferenceLogicalRange {
                    bytes: 0..1,
                    utf16: 0..1,
                },
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        let mut forged_request = capability.prepare_replay().unwrap();
        forged_request.source_start.physical_lower_bound_bytes = 1;
        let projection_session = cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();
        assert!(matches!(
            cursor.begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                forged_request,
                projection_session,
            ),
            Err(ActiveParagraphProjectionError::CrossedCursor)
        ));

        let crossed_capability = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &DirectReferenceLogicalRange {
                    bytes: 0..1,
                    utf16: 0..1,
                },
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        let projection_session = cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();
        assert!(matches!(
            second_cursor.begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                crossed_capability.prepare_replay().unwrap(),
                projection_session,
            ),
            Err(ActiveParagraphProjectionError::CrossedCursor)
        ));

        drop(second_cursor);
        drop(cursor);
        let abort = session.begin_abort().unwrap();
        drop(build);
        settle_aborted_fixture(&mut arena, abort);
        let foreign_session = foreign_arena.resume_build(foreign_ticket).unwrap();
        let foreign_abort = foreign_session.begin_abort().unwrap();
        settle_aborted_fixture(&mut foreign_arena, foreign_abort);
    }

    #[test]
    fn cancellation_and_post_mint_builder_mutation_invalidate_cursor_work() {
        let metric = SerializedMetric { bytes: 2, utf16: 2 };
        let source_store = SourceStore::new("xy", 8);
        let source_descriptor = source_store.descriptor();
        let mut root_spec = spec(metric);
        root_spec.source_revision = source_descriptor.revision;
        root_spec.source_root = source_descriptor.root;
        root_spec.source_bytes = u64::try_from(source_descriptor.bytes).unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        offer_event(
            &mut build,
            &mut session,
            identity_run(1, BlockId(2), SerializedMetric { bytes: 1, utf16: 1 }),
        );
        let first_high_water = SerializedMetric { bytes: 1, utf16: 1 };
        let binding =
            ActorProjectionBinding::mechanism_only(&build, &paragraph, 67, first_high_water, 71);
        let mut cancelled_cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let mut stale_cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let mut source = FixturePhysicalSource::new(&root_spec, b"xy");
        assert_eq!(
            cancelled_cursor.poll_byte(&build, &session, binding, &mut source, true),
            Ok(ActiveParagraphProjectionPoll::Cancelled)
        );
        assert_eq!(
            cancelled_cursor.poll_byte(&build, &session, binding, &mut source, false),
            Err(ActiveParagraphProjectionError::CursorCancelled)
        );

        let (logical, _, _) =
            drain_cursor(&mut stale_cursor, &build, &session, binding, &mut source);
        assert_eq!(logical, b"x");
        let stale_capability = stale_cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &DirectReferenceLogicalRange {
                    bytes: 0..1,
                    utf16: 0..1,
                },
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        let projection_session = stale_cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();

        offer_event(
            &mut build,
            &mut session,
            identity_run(2, BlockId(2), SerializedMetric { bytes: 1, utf16: 1 }),
        );
        assert!(matches!(
            stale_cursor.begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                stale_capability.prepare_replay().unwrap(),
                projection_session,
            ),
            Err(ActiveParagraphProjectionError::StaleBinding)
        ));
        assert_eq!(
            stale_cursor.poll_byte(&build, &session, binding, &mut source, false),
            Err(ActiveParagraphProjectionError::StaleBinding)
        );

        drop(cancelled_cursor);
        drop(stale_cursor);
        let abort = session.begin_abort().unwrap();
        drop(build);
        settle_aborted_fixture(&mut arena, abort);
    }

    #[test]
    fn many_leaf_projection_keeps_only_logarithmic_route_and_one_page_of_events() {
        const LEAVES: u64 = 257;
        let metric = SerializedMetric {
            bytes: LEAVES,
            utf16: LEAVES,
        };
        let source_bytes = vec![b'x'; usize::try_from(LEAVES).unwrap()];
        let source_text = std::str::from_utf8(&source_bytes).unwrap();
        let source_store = SourceStore::new(source_text, 8);
        let source_descriptor = source_store.descriptor();
        let mut root_spec = spec(metric);
        root_spec.source_revision = source_descriptor.revision;
        root_spec.source_root = source_descriptor.root;
        root_spec.source_bytes = u64::try_from(source_descriptor.bytes).unwrap();
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec.clone()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer_event(
            &mut build,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let paragraph = offer_paragraph(&mut build, &mut session, BlockId(2));
        for id in 1..=LEAVES {
            offer_event(
                &mut build,
                &mut session,
                identity_run(id, BlockId(2), SerializedMetric { bytes: 1, utf16: 1 }),
            );
            force_leaf_barrier(&mut build, &mut session);
        }
        reduce_prefix(&mut build, &mut session);
        let binding = ActorProjectionBinding::mechanism_only(&build, &paragraph, 73, metric, 79);
        let mut cursor = build
            .open_active_paragraph_projection_cursor(&session, &paragraph, binding, None)
            .unwrap();
        let mut source = FixturePhysicalSource::new(&root_spec, &source_bytes);
        let (logical, _, _) = drain_cursor(&mut cursor, &build, &session, binding, &mut source);
        assert_eq!(logical, source_bytes);

        let receipt = cursor.receipt();
        assert_eq!(receipt.leaf_pages_decoded, usize::try_from(LEAVES).unwrap());
        assert_eq!(receipt.partial_leaf_decodes, 1);
        assert_eq!(receipt.root_descents, 1);
        assert!(receipt.maximum_route_depth <= ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH);
        assert!(
            receipt.maximum_decoded_page_bytes
                <= ARENA_PAGE_BYTES * (1 + std::mem::size_of::<DecodedLeafEvent>())
        );
        assert_eq!(receipt.maximum_program_scratch_bytes, 0);
        assert_eq!(receipt.maximum_ready_byte_cache_bytes, 1);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(receipt.document_sized_event_vectors, 0);

        let far_range = DirectReferenceLogicalRange {
            bytes: (LEAVES - 2)..LEAVES,
            utf16: (LEAVES - 2)..LEAVES,
        };
        let far_capability = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &far_range,
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        assert_eq!(far_capability.receipt.root_descents, 2);
        assert_eq!(far_capability.receipt.leaf_pages_decoded, 2);
        assert!(far_capability.receipt.maximum_route_depth <= ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH);
        assert!(far_capability.receipt.events_inspected <= 4);
        assert_eq!(far_capability.receipt.retained_source_bytes, 0);
        assert_eq!(far_capability.receipt.document_sized_event_vectors, 0);
        let far_request = far_capability.prepare_replay().unwrap();
        assert_eq!(
            far_request.source_start().physical_lower_bound_bytes(),
            usize::try_from(LEAVES - 2).unwrap()
        );
        assert!(matches!(
            far_request.capability.start.observation,
            ActiveParagraphProjectedBoundaryObservation::ExactSource { .. }
        ));
        let projection_session = cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();
        let mut far_replay = cursor
            .begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                far_request,
                projection_session,
            )
            .unwrap();
        assert_eq!(
            drain_owned_replay(&mut far_replay, &build, &session, binding),
            b"xx"
        );
        let (completed, projection_session) = far_replay.take_completed().unwrap();
        let projection_receipt = projection_session
            .retire(binding.source(), cursor.identity().cursor_nonce())
            .unwrap();
        assert_eq!(projection_receipt.cursor_roles_minted, 1);
        assert_eq!(projection_receipt.forward_cursor_jumps, 0);
        assert_eq!(projection_receipt.source_bytes_read, 2);
        assert_eq!(completed.capability.logical, far_range);
        assert_eq!(completed.receipt.seek.root_descents, 1);
        assert_eq!(completed.receipt.seek.leaf_pages_decoded, 1);
        assert!(completed.receipt.seek.maximum_route_depth <= ACTIVE_PARAGRAPH_MAX_ROUTE_DEPTH);

        let cancelled_range = DirectReferenceLogicalRange {
            bytes: 100..101,
            utf16: 100..101,
        };
        let cancelled_capability = cursor
            .resolve_range(
                &build,
                &session,
                binding,
                &cancelled_range,
                GreenAffinity::Downstream,
                GreenAffinity::Upstream,
            )
            .unwrap();
        let live_nodes_before_cancel = session.arena().metrics().live_nodes;
        let projection_session = cursor
            .open_source_projection_session(&build, &session, binding, &source_store)
            .unwrap();
        let mut cancelled_replay = cursor
            .begin_range_replay_in_source_session(
                &build,
                &session,
                binding,
                cancelled_capability.prepare_replay().unwrap(),
                projection_session,
            )
            .unwrap();
        let mut reached_byte = false;
        for _ in 0..16 {
            if cancelled_replay
                .poll_byte(&build, &session, binding, false)
                .unwrap()
                == ActiveParagraphProjectionPoll::ByteReady
            {
                reached_byte = true;
                break;
            }
        }
        assert!(reached_byte);
        assert_eq!(
            cancelled_replay
                .poll_byte(&build, &session, binding, true)
                .unwrap(),
            ActiveParagraphProjectionPoll::Cancelled
        );
        assert_eq!(
            cancelled_replay.poll_byte(&build, &session, binding, false),
            Err(ActiveParagraphProjectionError::CursorCancelled)
        );
        let projection_session = cancelled_replay.cancel().unwrap();
        let projection_receipt = projection_session
            .retire(binding.source(), cursor.identity().cursor_nonce())
            .unwrap();
        assert_eq!(projection_receipt.passes_started, 1);
        assert_eq!(projection_receipt.passes_finished, 0);
        assert_eq!(projection_receipt.passes_cancelled, 1);
        assert_eq!(
            session.arena().metrics().live_nodes,
            live_nodes_before_cancel
        );

        let seal = cursor
            .into_transaction_seal(&build, &session, binding)
            .unwrap();
        assert!(seal.validates_range(&completed.capability));
        assert_eq!(seal.identity.cursor_nonce != 0, true);
        assert_eq!(
            seal.root,
            build
                .working_prefix
                .as_ref()
                .map(|prefix| session.owner_id(&prefix.owner).unwrap())
                .unwrap()
        );
        assert_eq!(seal.covered_leaf_range, 0..LEAVES);
        assert_eq!(seal.paragraph_physical.start, SerializedMetric::default());
        assert_eq!(seal.paragraph_physical.end, metric);
        assert_eq!(seal.paragraph_logical.start, SerializedMetric::default());
        assert_eq!(seal.paragraph_logical.end, metric);
        assert_eq!(seal.staged_terminator, None);
        assert_eq!(seal.source.root, root_spec.source_root);
        assert_eq!(seal.source.revision, root_spec.source_revision);
        assert_eq!(seal.source.bytes, usize::try_from(LEAVES).unwrap());
        let abort = session.begin_abort().unwrap();
        drop(build);
        settle_aborted_fixture(&mut arena, abort);
    }
}

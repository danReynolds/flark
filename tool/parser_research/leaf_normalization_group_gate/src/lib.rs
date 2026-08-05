//! Representation-neutral authority and state-machine falsifier for a
//! `LeafNormalizationGroup`.
//!
//! This crate intentionally does not model packed-green layout, a Markdown
//! scanner, source-rope storage, or allocation rollback. It asks a narrower
//! question: can fresh parsing, restart, convergence, and every selected
//! Paragraph normalization share one generation-bound typed authority without
//! exposing mutation offsets or provisional semantic truth?

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// Numeric receipt for tests and diagnostic output. The value is
            /// never accepted back as authority.
            #[must_use]
            pub const fn receipt(self) -> u64 {
                self.0
            }
        }
    };
}

id_type!(RevisionId);
id_type!(BlockId);
id_type!(GroupLineageId);
id_type!(GroupGeneration);
id_type!(CheckpointId);
id_type!(ManifestId);
id_type!(SourceTailId);
id_type!(RunArenaId);
id_type!(RunIdentity);
id_type!(CapabilityId);
id_type!(ParentPathId);
id_type!(ProjectionPlanId);
id_type!(NormalizedLabelId);

static NEXT_KERNEL_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_RUN_ARENA_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunArenaStats {
    pub allocated_nodes: u64,
    pub represented_bytes: u64,
    pub copied_source_bytes: u64,
}

#[derive(Debug)]
struct RunArenaInner {
    id: RunArenaId,
    next_node: AtomicU64,
    allocated_nodes: AtomicU64,
    represented_bytes: AtomicU64,
}

/// Arena for persistent source-run expressions.
///
/// Leaves contain length metadata and an identity, not source bytes. This is
/// enough to falsify accidental aggregate copying in this state-machine model;
/// it is deliberately not a proof of the eventual source-rope implementation.
#[derive(Clone, Debug)]
pub struct RunArena(Arc<RunArenaInner>);

impl Default for RunArena {
    fn default() -> Self {
        Self::new()
    }
}

impl RunArena {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(RunArenaInner {
            id: RunArenaId(NEXT_RUN_ARENA_ID.fetch_add(1, Ordering::Relaxed)),
            next_node: AtomicU64::new(1),
            allocated_nodes: AtomicU64::new(0),
            represented_bytes: AtomicU64::new(0),
        }))
    }

    /// Registers one already-owned source page. No source payload is copied.
    pub fn page(&self, byte_len: u64, utf16_len: u64) -> Result<SourceRuns, GateError> {
        if byte_len == 0 {
            return Err(GateError::EmptySourceRuns);
        }
        let identity = self.allocate_node_identity()?;
        self.0.allocated_nodes.fetch_add(1, Ordering::Relaxed);
        self.0
            .represented_bytes
            .fetch_add(byte_len, Ordering::Relaxed);
        Ok(SourceRuns {
            arena: Arc::clone(&self.0),
            root: Arc::new(RunNode::Leaf {
                identity,
                byte_len,
                utf16_len,
            }),
        })
    }

    #[must_use]
    pub fn stats(&self) -> RunArenaStats {
        RunArenaStats {
            allocated_nodes: self.0.allocated_nodes.load(Ordering::Relaxed),
            represented_bytes: self.0.represented_bytes.load(Ordering::Relaxed),
            copied_source_bytes: 0,
        }
    }

    fn allocate_node_identity(&self) -> Result<RunIdentity, GateError> {
        let identity = self.0.next_node.fetch_add(1, Ordering::Relaxed);
        if identity == u64::MAX {
            return Err(GateError::CapacityExhausted);
        }
        Ok(RunIdentity(identity))
    }
}

#[derive(Debug)]
enum RunNode {
    Leaf {
        identity: RunIdentity,
        byte_len: u64,
        utf16_len: u64,
    },
    Concat {
        identity: RunIdentity,
        earlier: Arc<RunNode>,
        later: Arc<RunNode>,
        byte_len: u64,
        utf16_len: u64,
    },
}

impl RunNode {
    const fn identity(&self) -> RunIdentity {
        match self {
            Self::Leaf { identity, .. } | Self::Concat { identity, .. } => *identity,
        }
    }

    const fn byte_len(&self) -> u64 {
        match self {
            Self::Leaf { byte_len, .. } | Self::Concat { byte_len, .. } => *byte_len,
        }
    }

    const fn utf16_len(&self) -> u64 {
        match self {
            Self::Leaf { utf16_len, .. } | Self::Concat { utf16_len, .. } => *utf16_len,
        }
    }
}

/// Persistent expression over source-owned pages.
#[derive(Clone, Debug)]
pub struct SourceRuns {
    arena: Arc<RunArenaInner>,
    root: Arc<RunNode>,
}

impl SourceRuns {
    /// Allocates one fixed-size concat node and retains both input roots.
    pub fn concat(&self, later: &Self) -> Result<Self, GateError> {
        if !Arc::ptr_eq(&self.arena, &later.arena) {
            return Err(GateError::RunArenaMismatch);
        }
        let byte_len = self
            .byte_len()
            .checked_add(later.byte_len())
            .ok_or(GateError::CapacityExhausted)?;
        let utf16_len = self
            .utf16_len()
            .checked_add(later.utf16_len())
            .ok_or(GateError::CapacityExhausted)?;
        let arena = RunArena(Arc::clone(&self.arena));
        let identity = arena.allocate_node_identity()?;
        self.arena
            .allocated_nodes
            .fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            arena: Arc::clone(&self.arena),
            root: Arc::new(RunNode::Concat {
                identity,
                earlier: Arc::clone(&self.root),
                later: Arc::clone(&later.root),
                byte_len,
                utf16_len,
            }),
        })
    }

    #[must_use]
    pub fn byte_len(&self) -> u64 {
        self.root.byte_len()
    }

    #[must_use]
    pub fn utf16_len(&self) -> u64 {
        self.root.utf16_len()
    }

    #[must_use]
    pub fn identity(&self) -> RunIdentity {
        self.root.identity()
    }

    #[must_use]
    pub fn shares_exact_root(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.root, &other.root)
    }

    #[must_use]
    pub fn contains_exact_subtree(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.arena, &other.arena)
            && contains_run_subtree(&self.root, &other.root)
    }
}

fn contains_run_subtree(node: &Arc<RunNode>, needle: &Arc<RunNode>) -> bool {
    if Arc::ptr_eq(node, needle) {
        return true;
    }
    match node.as_ref() {
        RunNode::Leaf { .. } => false,
        RunNode::Concat { earlier, later, .. } => {
            contains_run_subtree(earlier, needle) || contains_run_subtree(later, needle)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GroupKey {
    lineage: GroupLineageId,
    generation: GroupGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scope {
    kernel: u64,
    group: GroupKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CapabilitySeal {
    scope: Scope,
    id: CapabilityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentShape {
    Document,
    QuoteItem,
}

/// Parser control snapshot. It is intentionally Paragraph-shaped and contains
/// no finalized semantic kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParagraphControl {
    pub lazy_continuation: bool,
    pub reference_prefix_open: bool,
    pub table_probe_columns: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    ProvisionalParagraph,
}

/// Non-cloneable authority for one candidate generation.
#[derive(Debug)]
pub struct OpenGroupLease {
    scope: Scope,
    revision: RevisionId,
    primary: BlockId,
    control: ParagraphControl,
    parent_path: ParentPathId,
    resumed_from: Option<CheckpointId>,
    base_manifest: Option<ManifestId>,
}

impl OpenGroupLease {
    #[must_use]
    pub const fn lineage(&self) -> GroupLineageId {
        self.scope.group.lineage
    }

    #[must_use]
    pub const fn generation(&self) -> GroupGeneration {
        self.scope.group.generation
    }

    #[must_use]
    pub const fn revision(&self) -> RevisionId {
        self.revision
    }

    #[must_use]
    pub const fn primary_id(&self) -> BlockId {
        self.primary
    }

    #[must_use]
    pub const fn control(&self) -> ParagraphControl {
        self.control
    }

    #[must_use]
    pub const fn control_kind(&self) -> ControlKind {
        ControlKind::ProvisionalParagraph
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExactSourceBoundary {
    tail: SourceTailId,
    physical_line: u64,
}

impl ExactSourceBoundary {
    #[must_use]
    pub const fn tail_receipt(self) -> u64 {
        self.tail.receipt()
    }

    #[must_use]
    pub const fn physical_line(self) -> u64 {
        self.physical_line
    }
}

#[derive(Debug)]
pub struct SourceBoundaryCapability {
    seal: CapabilitySeal,
    boundary: ExactSourceBoundary,
}

/// Scalar restart sample. It contains no semantic fragment or source-run root.
#[derive(Clone, Debug)]
pub struct ParagraphCheckpoint {
    id: CheckpointId,
    scope: Scope,
    control: ParagraphControl,
    parent_path: ParentPathId,
    boundary: ExactSourceBoundary,
}

impl ParagraphCheckpoint {
    #[must_use]
    pub const fn id(&self) -> CheckpointId {
        self.id
    }

    #[must_use]
    pub const fn lineage(&self) -> GroupLineageId {
        self.scope.group.lineage
    }

    #[must_use]
    pub const fn generation(&self) -> GroupGeneration {
        self.scope.group.generation
    }

    #[must_use]
    pub const fn control(&self) -> ParagraphControl {
        self.control
    }

    #[must_use]
    pub const fn control_kind(&self) -> ControlKind {
        ControlKind::ProvisionalParagraph
    }

    #[must_use]
    pub const fn boundary(&self) -> ExactSourceBoundary {
        self.boundary
    }

    /// Receipt proving the checkpoint does not own a deferred semantic/source
    /// fragment. The source subsystem retains the tail by scalar identity.
    #[must_use]
    pub const fn owned_fragment_count(&self) -> usize {
        0
    }
}

pub enum VisibleRole {}
pub enum DefinitionRole {}
pub enum SetextMarkerRole {}
pub enum TerminatorRole {}
pub enum TableDelimiterRole {}
pub enum HeaderCellRole {}
pub enum PrefaceRole {}
pub enum ChangedPrefixRole {}

/// Scanner-certified persistent source runs for one semantic role.
#[derive(Debug)]
pub struct CertifiedRuns<Role> {
    seal: CapabilitySeal,
    runs: SourceRuns,
    _role: PhantomData<fn() -> Role>,
}

pub type VisibleRuns = CertifiedRuns<VisibleRole>;
pub type DefinitionRuns = CertifiedRuns<DefinitionRole>;
pub type SetextMarkerRuns = CertifiedRuns<SetextMarkerRole>;
pub type TerminatorRuns = CertifiedRuns<TerminatorRole>;
pub type TableDelimiterRuns = CertifiedRuns<TableDelimiterRole>;
pub type HeaderCellRuns = CertifiedRuns<HeaderCellRole>;
pub type PrefaceRuns = CertifiedRuns<PrefaceRole>;
pub type ChangedPrefixRuns = CertifiedRuns<ChangedPrefixRole>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReferenceSummary {
    finalized_definitions: u32,
    reowned_to_parent: u32,
    stale_occurrences_retired: u32,
    fresh_finalization_suppressed: bool,
    changed_labels: Arc<[NormalizedLabelId]>,
}

#[derive(Debug)]
pub struct ParagraphReferenceEffect {
    seal: CapabilitySeal,
    summary: ReferenceSummary,
}

#[derive(Debug)]
pub struct HeadingReferenceEffect {
    seal: CapabilitySeal,
    summary: ReferenceSummary,
}

#[derive(Debug)]
pub struct ReferenceOnlyEffect {
    seal: CapabilitySeal,
    summary: ReferenceSummary,
}

#[derive(Debug)]
pub struct TableReferenceEffect {
    seal: CapabilitySeal,
    summary: ReferenceSummary,
}

#[derive(Clone, Debug)]
struct TableShapeRecipe {
    columns: u16,
    projection_plan: ProjectionPlanId,
    header_cells: Arc<[SourceRuns]>,
}

/// Typed table structural/projection capability. It contains no caller-visible
/// offsets or replacement range.
#[derive(Debug)]
pub struct TableShapeCapability {
    seal: CapabilitySeal,
    recipe: TableShapeRecipe,
}

impl TableShapeCapability {
    #[must_use]
    pub const fn columns(&self) -> u16 {
        self.recipe.columns
    }

    #[must_use]
    pub const fn projection_plan(&self) -> ProjectionPlanId {
        self.recipe.projection_plan
    }
}

#[derive(Debug)]
struct ParagraphRequest {
    scope: Scope,
    visible: SourceRuns,
    terminator: SourceRuns,
    references: ReferenceSummary,
}

#[derive(Debug)]
struct SetextRequest {
    scope: Scope,
    visible: SourceRuns,
    marker: SourceRuns,
    terminator: SourceRuns,
    level: u8,
    references: ReferenceSummary,
}

#[derive(Debug)]
struct ReferenceOnlyRequest {
    scope: Scope,
    definitions: SourceRuns,
    references: ReferenceSummary,
}

#[derive(Debug)]
struct WholeTableRequest {
    scope: Scope,
    header: SourceRuns,
    delimiter: SourceRuns,
    shape: TableShapeRecipe,
    references: ReferenceSummary,
}

#[derive(Debug)]
struct SplitTableRequest {
    scope: Scope,
    preface: SourceRuns,
    header: SourceRuns,
    delimiter: SourceRuns,
    shape: TableShapeRecipe,
    references: ReferenceSummary,
}

/// Exhaustive, typed request accepted by the selected Paragraph finalizer.
#[derive(Debug)]
pub enum NormalizationRequest {
    Paragraph(ParagraphRequest),
    Setext(SetextRequest),
    ReferenceOnly(ReferenceOnlyRequest),
    WholeTable(WholeTableRequest),
    SplitTable(SplitTableRequest),
}

impl NormalizationRequest {
    #[must_use]
    pub const fn outcome_kind(&self) -> OutcomeKind {
        match self {
            Self::Paragraph(_) => OutcomeKind::Paragraph,
            Self::Setext(_) => OutcomeKind::SetextHeading,
            Self::ReferenceOnly(_) => OutcomeKind::ReferenceOnly,
            Self::WholeTable(_) => OutcomeKind::WholeTable,
            Self::SplitTable(_) => OutcomeKind::SplitTable,
        }
    }

    const fn scope(&self) -> Scope {
        match self {
            Self::Paragraph(request) => request.scope,
            Self::Setext(request) => request.scope,
            Self::ReferenceOnly(request) => request.scope,
            Self::WholeTable(request) => request.scope,
            Self::SplitTable(request) => request.scope,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    Paragraph,
    SetextHeading,
    ReferenceOnly,
    WholeTable,
    SplitTable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphFootprint {
    pub paragraph: BlockId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingFootprint {
    pub heading: BlockId,
    pub level: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmptyFootprint {
    pub retired_primary: BlockId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableFootprint {
    pub retired_primary: BlockId,
    pub table: BlockId,
    pub header_row: BlockId,
    pub header_cells: Arc<[BlockId]>,
    pub columns: u16,
    pub projection_plan: ProjectionPlanId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitTableFootprint {
    pub preface: BlockId,
    pub table: BlockId,
    pub header_row: BlockId,
    pub header_cells: Arc<[BlockId]>,
    pub columns: u16,
    pub projection_plan: ProjectionPlanId,
}

/// Typed structural replacement footprint. No generic range or offset exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralFootprint {
    Paragraph(ParagraphFootprint),
    Heading(HeadingFootprint),
    Empty(EmptyFootprint),
    WholeTable(TableFootprint),
    SplitTable(SplitTableFootprint),
}

impl StructuralFootprint {
    #[must_use]
    pub fn node_count(&self) -> usize {
        match self {
            Self::Paragraph(_) | Self::Heading(_) => 1,
            Self::Empty(_) => 0,
            Self::WholeTable(table) => 2 + table.header_cells.len(),
            Self::SplitTable(table) => 3 + table.header_cells.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceFootprint {
    Paragraph {
        finalized_definitions: u32,
        changed_labels: Arc<[NormalizedLabelId]>,
    },
    Heading {
        reowned_to_parent: u32,
        changed_labels: Arc<[NormalizedLabelId]>,
    },
    ReferenceOnly {
        finalized_definitions: u32,
        changed_labels: Arc<[NormalizedLabelId]>,
    },
    Table {
        fresh_finalization_suppressed: bool,
        stale_occurrences_retired: u32,
        changed_labels: Arc<[NormalizedLabelId]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnedState {
    ClosedParagraph,
    ClosedHeading,
    CapturedParentPath,
    OpenTable(BlockId),
    ClosedPrefaceAndOpenTable(BlockId),
}

#[derive(Clone, Debug)]
enum ManifestRecipe {
    Paragraph {
        terminator: SourceRuns,
        references: ReferenceSummary,
    },
    Setext {
        marker: SourceRuns,
        terminator: SourceRuns,
        level: u8,
        references: ReferenceSummary,
    },
    ReferenceOnly {
        references: ReferenceSummary,
    },
    WholeTable {
        delimiter: SourceRuns,
        shape: TableShapeRecipe,
        references: ReferenceSummary,
    },
    SplitTable {
        header: SourceRuns,
        delimiter: SourceRuns,
        shape: TableShapeRecipe,
        references: ReferenceSummary,
    },
}

impl ManifestRecipe {
    const fn outcome_kind(&self) -> OutcomeKind {
        match self {
            Self::Paragraph { .. } => OutcomeKind::Paragraph,
            Self::Setext { .. } => OutcomeKind::SetextHeading,
            Self::ReferenceOnly { .. } => OutcomeKind::ReferenceOnly,
            Self::WholeTable { .. } => OutcomeKind::WholeTable,
            Self::SplitTable { .. } => OutcomeKind::SplitTable,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProvisionalReferenceState {
    pub definition_candidates: u32,
    pub finalization_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProvisionalTableProbe {
    pub columns: u16,
}

/// Restart-only semantic-prefix recipe for the Paragraph that existed before
/// the decisive line. It is deliberately distinct from the final projection.
#[derive(Clone, Debug)]
pub struct ProvisionalSemanticPrefixRecipe {
    primary: BlockId,
    control: ParagraphControl,
    runs: SourceRuns,
    references: ProvisionalReferenceState,
    table_probe: Option<ProvisionalTableProbe>,
}

impl ProvisionalSemanticPrefixRecipe {
    #[must_use]
    pub const fn primary(&self) -> BlockId {
        self.primary
    }

    #[must_use]
    pub const fn control_kind(&self) -> ControlKind {
        ControlKind::ProvisionalParagraph
    }

    #[must_use]
    pub const fn control(&self) -> ParagraphControl {
        self.control
    }

    #[must_use]
    pub fn runs(&self) -> &SourceRuns {
        &self.runs
    }

    #[must_use]
    pub const fn references(&self) -> ProvisionalReferenceState {
        self.references
    }

    #[must_use]
    pub const fn table_probe(&self) -> Option<ProvisionalTableProbe> {
        self.table_probe
    }
}

#[derive(Debug)]
struct SealedResultData {
    scope: Scope,
    revision: RevisionId,
    provisional_primary: BlockId,
    outcome: OutcomeKind,
    structure: StructuralFootprint,
    references: ReferenceFootprint,
    returned: ReturnedState,
    body: SourceRuns,
}

#[derive(Debug)]
struct ManifestData {
    id: ManifestId,
    result: Arc<SealedResultData>,
    provisional_prefix: ProvisionalSemanticPrefixRecipe,
    recipe: ManifestRecipe,
    checkpoint_count: usize,
}

/// Canonical sealed outcome. The restart manifest is absent unless this group
/// generation actually persisted at least one interior checkpoint.
#[derive(Clone, Debug)]
pub struct SealedGroup {
    result: Arc<SealedResultData>,
    restart_manifest: Option<Arc<ManifestData>>,
}

impl SealedGroup {
    #[must_use]
    pub fn outcome(&self) -> OutcomeKind {
        self.result.outcome
    }

    #[must_use]
    pub fn structure(&self) -> &StructuralFootprint {
        &self.result.structure
    }

    #[must_use]
    pub fn references(&self) -> &ReferenceFootprint {
        &self.result.references
    }

    #[must_use]
    pub fn returned_state(&self) -> ReturnedState {
        self.result.returned
    }

    #[must_use]
    pub fn body_runs(&self) -> &SourceRuns {
        &self.result.body
    }

    #[must_use]
    pub fn restart_manifest(&self) -> Option<SealedNormalizationManifest> {
        self.restart_manifest
            .as_ref()
            .map(|manifest| SealedNormalizationManifest(Arc::clone(manifest)))
    }
}

/// One sparse immutable normalization manifest shared by every persisted
/// checkpoint sampled in the sealed group generation.
#[derive(Clone, Debug)]
pub struct SealedNormalizationManifest(Arc<ManifestData>);

impl SealedNormalizationManifest {
    #[must_use]
    pub fn id(&self) -> ManifestId {
        self.0.id
    }

    #[must_use]
    pub fn lineage(&self) -> GroupLineageId {
        self.0.result.scope.group.lineage
    }

    #[must_use]
    pub fn generation(&self) -> GroupGeneration {
        self.0.result.scope.group.generation
    }

    #[must_use]
    pub fn revision(&self) -> RevisionId {
        self.0.result.revision
    }

    #[must_use]
    pub fn provisional_primary(&self) -> BlockId {
        self.0.result.provisional_primary
    }

    #[must_use]
    pub fn outcome(&self) -> OutcomeKind {
        self.0.result.outcome
    }

    #[must_use]
    pub fn structure(&self) -> &StructuralFootprint {
        &self.0.result.structure
    }

    #[must_use]
    pub fn references(&self) -> &ReferenceFootprint {
        &self.0.result.references
    }

    #[must_use]
    pub fn returned_state(&self) -> ReturnedState {
        self.0.result.returned
    }

    #[must_use]
    pub fn body_runs(&self) -> &SourceRuns {
        &self.0.result.body
    }

    #[must_use]
    pub fn provisional_prefix(&self) -> &ProvisionalSemanticPrefixRecipe {
        &self.0.provisional_prefix
    }

    #[must_use]
    pub fn checkpoint_count(&self) -> usize {
        self.0.checkpoint_count
    }

    #[must_use]
    pub fn shares_allocation_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelStats {
    pub open_generations: u64,
    pub sealed_manifests: u64,
    pub sampled_checkpoints: u64,
    pub checkpoint_owned_fragments: u64,
    pub source_tail_catalog_roots: u64,
}

#[derive(Debug)]
enum GroupState {
    Open,
    Sealed {
        result: Arc<SealedResultData>,
        restart_manifest: Option<Arc<ManifestData>>,
    },
}

#[derive(Debug)]
struct CheckpointRecord {
    control: ParagraphControl,
    parent_path: ParentPathId,
    boundary: ExactSourceBoundary,
}

#[derive(Debug)]
struct GroupRecord {
    revision: RevisionId,
    primary: BlockId,
    control: ParagraphControl,
    parent_path: ParentPathId,
    resumed_from: Option<CheckpointId>,
    base_manifest: Option<ManifestId>,
    checkpoints: HashMap<CheckpointId, CheckpointRecord>,
    state: GroupState,
}

#[derive(Debug)]
struct TailRecord {
    runs: SourceRuns,
}

#[derive(Debug)]
pub struct MappedRetainedTail {
    seal: CapabilitySeal,
    runs: SourceRuns,
    checkpoint: CheckpointId,
    manifest: ManifestId,
    outcome: OutcomeKind,
}

#[derive(Debug)]
pub struct CompatibleChangedPrefix {
    seal: CapabilitySeal,
    runs: SourceRuns,
    checkpoint: CheckpointId,
    manifest: ManifestId,
    outcome: OutcomeKind,
}

#[derive(Debug)]
pub struct ConvergedGroupBody {
    seal: CapabilitySeal,
    runs: SourceRuns,
    manifest: ManifestId,
    outcome: OutcomeKind,
    retained_suffix: RunIdentity,
    allocated_run_nodes: u64,
    copied_source_bytes: u64,
}

impl ConvergedGroupBody {
    #[must_use]
    pub fn runs(&self) -> &SourceRuns {
        &self.runs
    }

    #[must_use]
    pub const fn retained_suffix_identity(&self) -> RunIdentity {
        self.retained_suffix
    }

    #[must_use]
    pub const fn allocated_run_nodes(&self) -> u64 {
        self.allocated_run_nodes
    }

    #[must_use]
    pub const fn copied_source_bytes(&self) -> u64 {
        self.copied_source_bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateError {
    WrongKernel,
    UnknownGroup,
    GroupNotOpen,
    GroupNotSealed,
    StaleGeneration,
    CapabilityMismatch,
    CheckpointMismatch,
    ManifestMismatch,
    ControlMismatch,
    ParentPathMismatch,
    RunArenaMismatch,
    EmptySourceRuns,
    InvalidOutcome(&'static str),
    CapacityExhausted,
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GateError {}

/// In-memory authority kernel for the executable gate.
#[derive(Debug)]
pub struct NormalizationKernel {
    kernel_id: u64,
    next_lineage: u64,
    next_block: u64,
    next_checkpoint: u64,
    next_manifest: u64,
    next_tail: u64,
    next_capability: u64,
    next_parent_path: u64,
    next_projection_plan: u64,
    generations: HashMap<GroupKey, GroupRecord>,
    latest_generation: HashMap<GroupLineageId, GroupGeneration>,
    tails: HashMap<SourceTailId, TailRecord>,
    tail_by_root: HashMap<(RunArenaId, RunIdentity), SourceTailId>,
    manifest_allocations: u64,
    checkpoint_count: u64,
}

impl Default for NormalizationKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl NormalizationKernel {
    #[must_use]
    pub fn new() -> Self {
        Self {
            kernel_id: NEXT_KERNEL_ID.fetch_add(1, Ordering::Relaxed),
            next_lineage: 1,
            next_block: 1,
            next_checkpoint: 1,
            next_manifest: 1,
            next_tail: 1,
            next_capability: 1,
            next_parent_path: 1,
            next_projection_plan: 1,
            generations: HashMap::new(),
            latest_generation: HashMap::new(),
            tails: HashMap::new(),
            tail_by_root: HashMap::new(),
            manifest_allocations: 0,
            checkpoint_count: 0,
        }
    }

    pub fn begin_fresh(
        &mut self,
        revision: u64,
        control: ParagraphControl,
        _parent: ParentShape,
    ) -> Result<OpenGroupLease, GateError> {
        let lineage = GroupLineageId(self.take_counter(Counter::Lineage)?);
        let generation = GroupGeneration(1);
        let primary = BlockId(self.take_counter(Counter::Block)?);
        let parent_path = ParentPathId(self.take_counter(Counter::ParentPath)?);
        let scope = Scope {
            kernel: self.kernel_id,
            group: GroupKey {
                lineage,
                generation,
            },
        };
        let revision = RevisionId(revision);
        self.generations.insert(
            scope.group,
            GroupRecord {
                revision,
                primary,
                control,
                parent_path,
                resumed_from: None,
                base_manifest: None,
                checkpoints: HashMap::new(),
                state: GroupState::Open,
            },
        );
        self.latest_generation.insert(lineage, generation);
        Ok(OpenGroupLease {
            scope,
            revision,
            primary,
            control,
            parent_path,
            resumed_from: None,
            base_manifest: None,
        })
    }

    pub fn certify_source_boundary(
        &mut self,
        lease: &OpenGroupLease,
        physical_line: u64,
        retained_provisional_tail: &SourceRuns,
    ) -> Result<SourceBoundaryCapability, GateError> {
        self.require_open(lease)?;
        let root_key = (
            retained_provisional_tail.arena.id,
            retained_provisional_tail.identity(),
        );
        let tail = if let Some(existing) = self.tail_by_root.get(&root_key) {
            *existing
        } else {
            let tail = SourceTailId(self.take_counter(Counter::Tail)?);
            self.tails.insert(
                tail,
                TailRecord {
                    runs: retained_provisional_tail.clone(),
                },
            );
            self.tail_by_root.insert(root_key, tail);
            tail
        };
        Ok(SourceBoundaryCapability {
            seal: self.mint_seal(lease)?,
            boundary: ExactSourceBoundary {
                tail,
                physical_line,
            },
        })
    }

    pub fn sample_checkpoint(
        &mut self,
        lease: &OpenGroupLease,
        control: ParagraphControl,
        boundary: SourceBoundaryCapability,
    ) -> Result<ParagraphCheckpoint, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, boundary.seal)?;
        if control != lease.control {
            return Err(GateError::ControlMismatch);
        }
        let id = CheckpointId(self.take_counter(Counter::Checkpoint)?);
        let checkpoint = ParagraphCheckpoint {
            id,
            scope: lease.scope,
            control,
            parent_path: lease.parent_path,
            boundary: boundary.boundary,
        };
        let record = self
            .generations
            .get_mut(&lease.scope.group)
            .ok_or(GateError::UnknownGroup)?;
        record.checkpoints.insert(
            id,
            CheckpointRecord {
                control,
                parent_path: lease.parent_path,
                boundary: boundary.boundary,
            },
        );
        self.checkpoint_count = self
            .checkpoint_count
            .checked_add(1)
            .ok_or(GateError::CapacityExhausted)?;
        Ok(checkpoint)
    }

    pub fn begin_resume(
        &mut self,
        checkpoint: &ParagraphCheckpoint,
        manifest: &SealedNormalizationManifest,
        revision: u64,
    ) -> Result<OpenGroupLease, GateError> {
        self.validate_checkpoint_manifest(checkpoint, manifest)?;
        let old_key = checkpoint.scope.group;
        let old_record = self
            .generations
            .get(&old_key)
            .ok_or(GateError::UnknownGroup)?;
        let stored = old_record
            .checkpoints
            .get(&checkpoint.id)
            .ok_or(GateError::CheckpointMismatch)?;
        if stored.control != checkpoint.control {
            return Err(GateError::ControlMismatch);
        }
        if stored.parent_path != checkpoint.parent_path {
            return Err(GateError::ParentPathMismatch);
        }
        let latest = self
            .latest_generation
            .get(&old_key.lineage)
            .copied()
            .ok_or(GateError::UnknownGroup)?;
        let generation = GroupGeneration(
            latest
                .0
                .checked_add(1)
                .ok_or(GateError::CapacityExhausted)?,
        );
        let scope = Scope {
            kernel: self.kernel_id,
            group: GroupKey {
                lineage: old_key.lineage,
                generation,
            },
        };
        let revision = RevisionId(revision);
        let primary = manifest.0.result.provisional_primary;
        self.generations.insert(
            scope.group,
            GroupRecord {
                revision,
                primary,
                control: checkpoint.control,
                parent_path: checkpoint.parent_path,
                resumed_from: Some(checkpoint.id),
                base_manifest: Some(manifest.id()),
                checkpoints: HashMap::new(),
                state: GroupState::Open,
            },
        );
        self.latest_generation
            .insert(old_key.lineage, generation);
        Ok(OpenGroupLease {
            scope,
            revision,
            primary,
            control: checkpoint.control,
            parent_path: checkpoint.parent_path,
            resumed_from: Some(checkpoint.id),
            base_manifest: Some(manifest.id()),
        })
    }

    pub fn certify_visible_runs(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<VisibleRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_definition_runs(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<DefinitionRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_setext_marker(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<SetextMarkerRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_terminator(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<TerminatorRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_table_delimiter(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<TableDelimiterRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_header_cell(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<HeaderCellRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_preface(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<PrefaceRuns, GateError> {
        self.certify_runs(lease, runs)
    }

    pub fn certify_table_shape(
        &mut self,
        lease: &OpenGroupLease,
        declared_columns: u16,
        cells: Vec<HeaderCellRuns>,
    ) -> Result<TableShapeCapability, GateError> {
        self.require_open(lease)?;
        if declared_columns == 0 || declared_columns > 256 {
            return Err(GateError::InvalidOutcome(
                "table column count must be in 1..=256",
            ));
        }
        if usize::from(declared_columns) != cells.len() {
            return Err(GateError::InvalidOutcome(
                "certified header-cell count differs from declared columns",
            ));
        }
        let mut header_cells = Vec::with_capacity(cells.len());
        for cell in cells {
            self.validate_seal(lease, cell.seal)?;
            header_cells.push(cell.runs);
        }
        let projection_plan = ProjectionPlanId(self.take_counter(Counter::ProjectionPlan)?);
        Ok(TableShapeCapability {
            seal: self.mint_seal(lease)?,
            recipe: TableShapeRecipe {
                columns: declared_columns,
                projection_plan,
                header_cells: header_cells.into(),
            },
        })
    }

    pub fn certify_paragraph_references(
        &mut self,
        lease: &OpenGroupLease,
        finalized_definitions: u32,
        changed_labels: Vec<NormalizedLabelId>,
    ) -> Result<ParagraphReferenceEffect, GateError> {
        Ok(ParagraphReferenceEffect {
            seal: self.mint_seal(lease)?,
            summary: ReferenceSummary {
                finalized_definitions,
                reowned_to_parent: 0,
                stale_occurrences_retired: 0,
                fresh_finalization_suppressed: false,
                changed_labels: changed_labels.into(),
            },
        })
    }

    pub fn certify_heading_references(
        &mut self,
        lease: &OpenGroupLease,
        reowned_to_parent: u32,
        changed_labels: Vec<NormalizedLabelId>,
    ) -> Result<HeadingReferenceEffect, GateError> {
        Ok(HeadingReferenceEffect {
            seal: self.mint_seal(lease)?,
            summary: ReferenceSummary {
                finalized_definitions: 0,
                reowned_to_parent,
                stale_occurrences_retired: 0,
                fresh_finalization_suppressed: false,
                changed_labels: changed_labels.into(),
            },
        })
    }

    pub fn certify_reference_only_effect(
        &mut self,
        lease: &OpenGroupLease,
        finalized_definitions: u32,
        changed_labels: Vec<NormalizedLabelId>,
    ) -> Result<ReferenceOnlyEffect, GateError> {
        if finalized_definitions == 0 {
            return Err(GateError::InvalidOutcome(
                "reference-only outcome must finalize at least one definition",
            ));
        }
        Ok(ReferenceOnlyEffect {
            seal: self.mint_seal(lease)?,
            summary: ReferenceSummary {
                finalized_definitions,
                reowned_to_parent: 0,
                stale_occurrences_retired: 0,
                fresh_finalization_suppressed: false,
                changed_labels: changed_labels.into(),
            },
        })
    }

    pub fn certify_table_references(
        &mut self,
        lease: &OpenGroupLease,
        stale_occurrences_retired: u32,
        changed_labels: Vec<NormalizedLabelId>,
    ) -> Result<TableReferenceEffect, GateError> {
        Ok(TableReferenceEffect {
            seal: self.mint_seal(lease)?,
            summary: ReferenceSummary {
                finalized_definitions: 0,
                reowned_to_parent: 0,
                stale_occurrences_retired,
                fresh_finalization_suppressed: true,
                changed_labels: changed_labels.into(),
            },
        })
    }

    pub fn paragraph_request(
        &self,
        lease: &OpenGroupLease,
        visible: VisibleRuns,
        terminator: TerminatorRuns,
        references: ParagraphReferenceEffect,
    ) -> Result<NormalizationRequest, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, visible.seal)?;
        self.validate_seal(lease, terminator.seal)?;
        self.validate_seal(lease, references.seal)?;
        Ok(NormalizationRequest::Paragraph(ParagraphRequest {
            scope: lease.scope,
            visible: visible.runs,
            terminator: terminator.runs,
            references: references.summary,
        }))
    }

    pub fn setext_request(
        &self,
        lease: &OpenGroupLease,
        visible: VisibleRuns,
        marker: SetextMarkerRuns,
        terminator: TerminatorRuns,
        level: u8,
        references: HeadingReferenceEffect,
    ) -> Result<NormalizationRequest, GateError> {
        self.require_open(lease)?;
        if !matches!(level, 1 | 2) {
            return Err(GateError::InvalidOutcome(
                "Setext level must be exactly 1 or 2",
            ));
        }
        self.validate_seal(lease, visible.seal)?;
        self.validate_seal(lease, marker.seal)?;
        self.validate_seal(lease, terminator.seal)?;
        self.validate_seal(lease, references.seal)?;
        Ok(NormalizationRequest::Setext(SetextRequest {
            scope: lease.scope,
            visible: visible.runs,
            marker: marker.runs,
            terminator: terminator.runs,
            level,
            references: references.summary,
        }))
    }

    pub fn reference_only_request(
        &self,
        lease: &OpenGroupLease,
        definitions: DefinitionRuns,
        references: ReferenceOnlyEffect,
    ) -> Result<NormalizationRequest, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, definitions.seal)?;
        self.validate_seal(lease, references.seal)?;
        Ok(NormalizationRequest::ReferenceOnly(
            ReferenceOnlyRequest {
                scope: lease.scope,
                definitions: definitions.runs,
                references: references.summary,
            },
        ))
    }

    pub fn whole_table_request(
        &self,
        lease: &OpenGroupLease,
        header: VisibleRuns,
        delimiter: TableDelimiterRuns,
        shape: TableShapeCapability,
        references: TableReferenceEffect,
    ) -> Result<NormalizationRequest, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, header.seal)?;
        self.validate_seal(lease, delimiter.seal)?;
        self.validate_seal(lease, shape.seal)?;
        self.validate_seal(lease, references.seal)?;
        Ok(NormalizationRequest::WholeTable(WholeTableRequest {
            scope: lease.scope,
            header: header.runs,
            delimiter: delimiter.runs,
            shape: shape.recipe,
            references: references.summary,
        }))
    }

    pub fn split_table_request(
        &self,
        lease: &OpenGroupLease,
        preface: PrefaceRuns,
        header: VisibleRuns,
        delimiter: TableDelimiterRuns,
        shape: TableShapeCapability,
        references: TableReferenceEffect,
    ) -> Result<NormalizationRequest, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, preface.seal)?;
        self.validate_seal(lease, header.seal)?;
        self.validate_seal(lease, delimiter.seal)?;
        self.validate_seal(lease, shape.seal)?;
        self.validate_seal(lease, references.seal)?;
        Ok(NormalizationRequest::SplitTable(SplitTableRequest {
            scope: lease.scope,
            preface: preface.runs,
            header: header.runs,
            delimiter: delimiter.runs,
            shape: shape.recipe,
            references: references.summary,
        }))
    }

    pub fn normalize(
        &mut self,
        lease: &OpenGroupLease,
        request: &NormalizationRequest,
    ) -> Result<SealedGroup, GateError> {
        self.require_open(lease)?;
        self.validate_request_shape(lease, request)?;

        // All validation precedes identity and manifest allocation. Every
        // reject path above therefore leaves the publication counters and
        // group state untouched.
        let (structure, references, returned, body, recipe) = match request {
            NormalizationRequest::Paragraph(request) => {
                let footprint = StructuralFootprint::Paragraph(ParagraphFootprint {
                    paragraph: lease.primary,
                });
                (
                    footprint,
                    paragraph_reference_footprint(&request.references),
                    ReturnedState::ClosedParagraph,
                    request.visible.clone(),
                    ManifestRecipe::Paragraph {
                        terminator: request.terminator.clone(),
                        references: request.references.clone(),
                    },
                )
            }
            NormalizationRequest::Setext(request) => {
                let footprint = StructuralFootprint::Heading(HeadingFootprint {
                    heading: lease.primary,
                    level: request.level,
                });
                (
                    footprint,
                    heading_reference_footprint(&request.references),
                    ReturnedState::ClosedHeading,
                    request.visible.clone(),
                    ManifestRecipe::Setext {
                        marker: request.marker.clone(),
                        terminator: request.terminator.clone(),
                        level: request.level,
                        references: request.references.clone(),
                    },
                )
            }
            NormalizationRequest::ReferenceOnly(request) => {
                let footprint = StructuralFootprint::Empty(EmptyFootprint {
                    retired_primary: lease.primary,
                });
                (
                    footprint,
                    reference_only_footprint(&request.references),
                    ReturnedState::CapturedParentPath,
                    request.definitions.clone(),
                    ManifestRecipe::ReferenceOnly {
                        references: request.references.clone(),
                    },
                )
            }
            NormalizationRequest::WholeTable(request) => {
                let table = BlockId(self.take_counter(Counter::Block)?);
                let header_row = BlockId(self.take_counter(Counter::Block)?);
                let header_cells = self.allocate_cells(request.shape.columns)?;
                let footprint = StructuralFootprint::WholeTable(TableFootprint {
                    retired_primary: lease.primary,
                    table,
                    header_row,
                    header_cells,
                    columns: request.shape.columns,
                    projection_plan: request.shape.projection_plan,
                });
                (
                    footprint,
                    table_reference_footprint(&request.references),
                    ReturnedState::OpenTable(table),
                    request.header.clone(),
                    ManifestRecipe::WholeTable {
                        delimiter: request.delimiter.clone(),
                        shape: request.shape.clone(),
                        references: request.references.clone(),
                    },
                )
            }
            NormalizationRequest::SplitTable(request) => {
                let table = BlockId(self.take_counter(Counter::Block)?);
                let header_row = BlockId(self.take_counter(Counter::Block)?);
                let header_cells = self.allocate_cells(request.shape.columns)?;
                let footprint = StructuralFootprint::SplitTable(SplitTableFootprint {
                    preface: lease.primary,
                    table,
                    header_row,
                    header_cells,
                    columns: request.shape.columns,
                    projection_plan: request.shape.projection_plan,
                });
                (
                    footprint,
                    table_reference_footprint(&request.references),
                    ReturnedState::ClosedPrefaceAndOpenTable(table),
                    request.preface.clone(),
                    ManifestRecipe::SplitTable {
                        header: request.header.clone(),
                        delimiter: request.delimiter.clone(),
                        shape: request.shape.clone(),
                        references: request.references.clone(),
                    },
                )
            }
        };

        let checkpoint_count = self
            .generations
            .get(&lease.scope.group)
            .ok_or(GateError::UnknownGroup)?
            .checkpoints
            .len();
        let result = Arc::new(SealedResultData {
            scope: lease.scope,
            revision: lease.revision,
            provisional_primary: lease.primary,
            outcome: request.outcome_kind(),
            structure,
            references,
            returned,
            body,
        });
        let restart_manifest = if checkpoint_count == 0 {
            None
        } else {
            let manifest_id = ManifestId(self.take_counter(Counter::Manifest)?);
            let provisional_prefix = provisional_prefix_recipe(lease, request)?;
            let manifest = Arc::new(ManifestData {
                id: manifest_id,
                result: Arc::clone(&result),
                provisional_prefix,
                recipe,
                checkpoint_count,
            });
            self.manifest_allocations = self
                .manifest_allocations
                .checked_add(1)
                .ok_or(GateError::CapacityExhausted)?;
            Some(manifest)
        };
        let record = self
            .generations
            .get_mut(&lease.scope.group)
            .ok_or(GateError::UnknownGroup)?;
        record.state = GroupState::Sealed {
            result: Arc::clone(&result),
            restart_manifest: restart_manifest.as_ref().map(Arc::clone),
        };
        Ok(SealedGroup {
            result,
            restart_manifest,
        })
    }

    pub fn manifest_for_checkpoint(
        &self,
        checkpoint: &ParagraphCheckpoint,
    ) -> Result<SealedNormalizationManifest, GateError> {
        if checkpoint.scope.kernel != self.kernel_id {
            return Err(GateError::WrongKernel);
        }
        let record = self
            .generations
            .get(&checkpoint.scope.group)
            .ok_or(GateError::UnknownGroup)?;
        if !record.checkpoints.contains_key(&checkpoint.id) {
            return Err(GateError::CheckpointMismatch);
        }
        match &record.state {
            GroupState::Open => Err(GateError::GroupNotSealed),
            GroupState::Sealed {
                restart_manifest: Some(manifest),
                ..
            } => Ok(SealedNormalizationManifest(Arc::clone(manifest))),
            GroupState::Sealed {
                restart_manifest: None,
                ..
            } => Err(GateError::CheckpointMismatch),
        }
    }

    /// Maps a scalar old source-tail identity into the resumed generation.
    pub fn map_unchanged_tail(
        &mut self,
        lease: &OpenGroupLease,
        checkpoint: &ParagraphCheckpoint,
        manifest: &SealedNormalizationManifest,
    ) -> Result<MappedRetainedTail, GateError> {
        self.require_open(lease)?;
        self.validate_checkpoint_manifest(checkpoint, manifest)?;
        if lease.resumed_from != Some(checkpoint.id)
            || lease.base_manifest != Some(manifest.id())
        {
            return Err(GateError::CheckpointMismatch);
        }
        if lease.control != checkpoint.control {
            return Err(GateError::ControlMismatch);
        }
        if lease.parent_path != checkpoint.parent_path {
            return Err(GateError::ParentPathMismatch);
        }
        let tail = self
            .tails
            .get(&checkpoint.boundary.tail)
            .ok_or(GateError::CheckpointMismatch)?
            .runs
            .clone();
        Ok(MappedRetainedTail {
            seal: self.mint_seal(lease)?,
            runs: tail,
            checkpoint: checkpoint.id,
            manifest: manifest.id(),
            outcome: manifest.outcome(),
        })
    }

    /// Represents a scanner/composer proof that a changed prefix reaches the
    /// same Paragraph control, parent path, reference state, and typed outcome
    /// boundary as the selected old manifest. Scanner correctness is outside
    /// this gate; scoping and consumption of its proof are tested here.
    pub fn certify_compatible_changed_prefix(
        &mut self,
        lease: &OpenGroupLease,
        checkpoint: &ParagraphCheckpoint,
        manifest: &SealedNormalizationManifest,
        runs: SourceRuns,
    ) -> Result<CompatibleChangedPrefix, GateError> {
        self.require_open(lease)?;
        self.validate_checkpoint_manifest(checkpoint, manifest)?;
        if lease.resumed_from != Some(checkpoint.id)
            || lease.base_manifest != Some(manifest.id())
        {
            return Err(GateError::CheckpointMismatch);
        }
        Ok(CompatibleChangedPrefix {
            seal: self.mint_seal(lease)?,
            runs,
            checkpoint: checkpoint.id,
            manifest: manifest.id(),
            outcome: manifest.outcome(),
        })
    }

    pub fn compose_open_group(
        &self,
        lease: &OpenGroupLease,
        changed_prefix: CompatibleChangedPrefix,
        retained_tail: MappedRetainedTail,
    ) -> Result<ConvergedGroupBody, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, changed_prefix.seal)?;
        self.validate_seal(lease, retained_tail.seal)?;
        if changed_prefix.checkpoint != retained_tail.checkpoint {
            return Err(GateError::CheckpointMismatch);
        }
        if changed_prefix.manifest != retained_tail.manifest
            || changed_prefix.outcome != retained_tail.outcome
        {
            return Err(GateError::ManifestMismatch);
        }
        let before = changed_prefix
            .runs
            .arena
            .allocated_nodes
            .load(Ordering::Relaxed);
        let retained_suffix = retained_tail.runs.identity();
        let runs = changed_prefix.runs.concat(&retained_tail.runs)?;
        let after = runs.arena.allocated_nodes.load(Ordering::Relaxed);
        Ok(ConvergedGroupBody {
            seal: changed_prefix.seal,
            runs,
            manifest: changed_prefix.manifest,
            outcome: changed_prefix.outcome,
            retained_suffix,
            allocated_run_nodes: after.saturating_sub(before),
            copied_source_bytes: 0,
        })
    }

    /// Rebinds an old typed outcome recipe to a converged provisional body and
    /// returns the same `NormalizationRequest` enum used by fresh parsing.
    pub fn request_from_convergence(
        &self,
        lease: &OpenGroupLease,
        manifest: &SealedNormalizationManifest,
        body: ConvergedGroupBody,
    ) -> Result<NormalizationRequest, GateError> {
        self.require_open(lease)?;
        self.validate_seal(lease, body.seal)?;
        if body.manifest != manifest.id() || body.outcome != manifest.outcome() {
            return Err(GateError::ManifestMismatch);
        }
        if lease.base_manifest != Some(manifest.id()) {
            return Err(GateError::ManifestMismatch);
        }
        let request = match &manifest.0.recipe {
            ManifestRecipe::Paragraph {
                terminator,
                references,
            } => NormalizationRequest::Paragraph(ParagraphRequest {
                scope: lease.scope,
                visible: body.runs,
                terminator: terminator.clone(),
                references: references.clone(),
            }),
            ManifestRecipe::Setext {
                marker,
                terminator,
                level,
                references,
            } => NormalizationRequest::Setext(SetextRequest {
                scope: lease.scope,
                visible: body.runs,
                marker: marker.clone(),
                terminator: terminator.clone(),
                level: *level,
                references: references.clone(),
            }),
            ManifestRecipe::ReferenceOnly { references } => {
                NormalizationRequest::ReferenceOnly(ReferenceOnlyRequest {
                    scope: lease.scope,
                    definitions: body.runs,
                    references: references.clone(),
                })
            }
            ManifestRecipe::WholeTable {
                delimiter,
                shape,
                references,
            } => NormalizationRequest::WholeTable(WholeTableRequest {
                scope: lease.scope,
                header: body.runs,
                delimiter: delimiter.clone(),
                shape: shape.clone(),
                references: references.clone(),
            }),
            ManifestRecipe::SplitTable {
                header,
                delimiter,
                shape,
                references,
            } => NormalizationRequest::SplitTable(SplitTableRequest {
                scope: lease.scope,
                preface: body.runs,
                header: header.clone(),
                delimiter: delimiter.clone(),
                shape: shape.clone(),
                references: references.clone(),
            }),
        };
        Ok(request)
    }

    #[must_use]
    pub fn stats(&self) -> KernelStats {
        let open_generations = self
            .generations
            .values()
            .filter(|record| matches!(record.state, GroupState::Open))
            .count() as u64;
        KernelStats {
            open_generations,
            sealed_manifests: self.manifest_allocations,
            sampled_checkpoints: self.checkpoint_count,
            checkpoint_owned_fragments: 0,
            source_tail_catalog_roots: self.tails.len() as u64,
        }
    }

    fn certify_runs<Role>(
        &mut self,
        lease: &OpenGroupLease,
        runs: SourceRuns,
    ) -> Result<CertifiedRuns<Role>, GateError> {
        self.require_open(lease)?;
        if runs.byte_len() == 0 {
            return Err(GateError::EmptySourceRuns);
        }
        Ok(CertifiedRuns {
            seal: self.mint_seal(lease)?,
            runs,
            _role: PhantomData,
        })
    }

    fn validate_request_shape(
        &self,
        lease: &OpenGroupLease,
        request: &NormalizationRequest,
    ) -> Result<(), GateError> {
        self.require_open(lease)?;
        self.validate_scope(lease, request.scope())?;
        match request {
            NormalizationRequest::Paragraph(request) => {
                require_nonempty(&request.visible)?;
                require_nonempty(&request.terminator)?;
            }
            NormalizationRequest::Setext(request) => {
                require_nonempty(&request.visible)?;
                require_nonempty(&request.marker)?;
                require_nonempty(&request.terminator)?;
                if !matches!(request.level, 1 | 2) {
                    return Err(GateError::InvalidOutcome(
                        "Setext level must be exactly 1 or 2",
                    ));
                }
            }
            NormalizationRequest::ReferenceOnly(request) => {
                require_nonempty(&request.definitions)?;
                if request.references.finalized_definitions == 0 {
                    return Err(GateError::InvalidOutcome(
                        "reference-only outcome must finalize definitions",
                    ));
                }
            }
            NormalizationRequest::WholeTable(request) => {
                validate_table_request(
                    &request.header,
                    &request.delimiter,
                    &request.shape,
                    &request.references,
                )?;
            }
            NormalizationRequest::SplitTable(request) => {
                require_nonempty(&request.preface)?;
                validate_table_request(
                    &request.header,
                    &request.delimiter,
                    &request.shape,
                    &request.references,
                )?;
            }
        }
        Ok(())
    }

    fn allocate_cells(&mut self, count: u16) -> Result<Arc<[BlockId]>, GateError> {
        let mut cells = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            cells.push(BlockId(self.take_counter(Counter::Block)?));
        }
        Ok(cells.into())
    }

    fn require_open(&self, lease: &OpenGroupLease) -> Result<&GroupRecord, GateError> {
        if lease.scope.kernel != self.kernel_id {
            return Err(GateError::WrongKernel);
        }
        let record = self
            .generations
            .get(&lease.scope.group)
            .ok_or_else(|| {
                if self
                    .latest_generation
                    .contains_key(&lease.scope.group.lineage)
                {
                    GateError::StaleGeneration
                } else {
                    GateError::UnknownGroup
                }
            })?;
        if record.revision != lease.revision
            || record.primary != lease.primary
            || record.control != lease.control
            || record.parent_path != lease.parent_path
            || record.resumed_from != lease.resumed_from
            || record.base_manifest != lease.base_manifest
        {
            return Err(GateError::CapabilityMismatch);
        }
        match record.state {
            GroupState::Open => Ok(record),
            GroupState::Sealed { .. } => Err(GateError::GroupNotOpen),
        }
    }

    fn validate_seal(
        &self,
        lease: &OpenGroupLease,
        seal: CapabilitySeal,
    ) -> Result<(), GateError> {
        if seal.scope.kernel != self.kernel_id || lease.scope.kernel != self.kernel_id {
            return Err(GateError::WrongKernel);
        }
        if seal.scope.group.lineage != lease.scope.group.lineage {
            return Err(GateError::CapabilityMismatch);
        }
        if seal.scope.group.generation != lease.scope.group.generation {
            return Err(GateError::StaleGeneration);
        }
        if seal.id.0 == 0 {
            return Err(GateError::CapabilityMismatch);
        }
        Ok(())
    }

    fn validate_scope(&self, lease: &OpenGroupLease, scope: Scope) -> Result<(), GateError> {
        if scope.kernel != self.kernel_id || lease.scope.kernel != self.kernel_id {
            return Err(GateError::WrongKernel);
        }
        if scope.group.lineage != lease.scope.group.lineage {
            return Err(GateError::CapabilityMismatch);
        }
        if scope.group.generation != lease.scope.group.generation {
            return Err(GateError::StaleGeneration);
        }
        Ok(())
    }

    fn mint_seal(&mut self, lease: &OpenGroupLease) -> Result<CapabilitySeal, GateError> {
        self.require_open(lease)?;
        Ok(CapabilitySeal {
            scope: lease.scope,
            id: CapabilityId(self.take_counter(Counter::Capability)?),
        })
    }

    fn validate_checkpoint_manifest(
        &self,
        checkpoint: &ParagraphCheckpoint,
        manifest: &SealedNormalizationManifest,
    ) -> Result<(), GateError> {
        if checkpoint.scope.kernel != self.kernel_id
            || manifest.0.result.scope.kernel != self.kernel_id
        {
            return Err(GateError::WrongKernel);
        }
        if checkpoint.scope != manifest.0.result.scope {
            return Err(GateError::ManifestMismatch);
        }
        let record = self
            .generations
            .get(&checkpoint.scope.group)
            .ok_or(GateError::UnknownGroup)?;
        let stored_checkpoint = record
            .checkpoints
            .get(&checkpoint.id)
            .ok_or(GateError::CheckpointMismatch)?;
        if stored_checkpoint.boundary != checkpoint.boundary {
            return Err(GateError::CheckpointMismatch);
        }
        match &record.state {
            GroupState::Open => Err(GateError::GroupNotSealed),
            GroupState::Sealed {
                restart_manifest: Some(stored),
                ..
            } if Arc::ptr_eq(stored, &manifest.0) => Ok(()),
            GroupState::Sealed { .. } => Err(GateError::ManifestMismatch),
        }
    }

    fn take_counter(&mut self, counter: Counter) -> Result<u64, GateError> {
        let slot = match counter {
            Counter::Lineage => &mut self.next_lineage,
            Counter::Block => &mut self.next_block,
            Counter::Checkpoint => &mut self.next_checkpoint,
            Counter::Manifest => &mut self.next_manifest,
            Counter::Tail => &mut self.next_tail,
            Counter::Capability => &mut self.next_capability,
            Counter::ParentPath => &mut self.next_parent_path,
            Counter::ProjectionPlan => &mut self.next_projection_plan,
        };
        let value = *slot;
        *slot = slot.checked_add(1).ok_or(GateError::CapacityExhausted)?;
        Ok(value)
    }
}

#[derive(Clone, Copy)]
enum Counter {
    Lineage,
    Block,
    Checkpoint,
    Manifest,
    Tail,
    Capability,
    ParentPath,
    ProjectionPlan,
}

fn provisional_prefix_recipe(
    lease: &OpenGroupLease,
    request: &NormalizationRequest,
) -> Result<ProvisionalSemanticPrefixRecipe, GateError> {
    let (runs, references, table_probe) = match request {
        NormalizationRequest::Paragraph(request) => (
            request.visible.clone(),
            ProvisionalReferenceState {
                definition_candidates: request.references.finalized_definitions,
                finalization_pending: request.references.finalized_definitions > 0,
            },
            None,
        ),
        NormalizationRequest::Setext(request) => (
            request.visible.clone(),
            ProvisionalReferenceState {
                definition_candidates: request.references.reowned_to_parent,
                finalization_pending: request.references.reowned_to_parent > 0,
            },
            None,
        ),
        NormalizationRequest::ReferenceOnly(request) => (
            request.definitions.clone(),
            ProvisionalReferenceState {
                definition_candidates: request.references.finalized_definitions,
                finalization_pending: true,
            },
            None,
        ),
        NormalizationRequest::WholeTable(request) => (
            request.header.clone(),
            ProvisionalReferenceState {
                definition_candidates: 0,
                finalization_pending: false,
            },
            Some(ProvisionalTableProbe {
                columns: request.shape.columns,
            }),
        ),
        NormalizationRequest::SplitTable(request) => (
            request.preface.concat(&request.header)?,
            ProvisionalReferenceState {
                definition_candidates: 0,
                finalization_pending: false,
            },
            Some(ProvisionalTableProbe {
                columns: request.shape.columns,
            }),
        ),
    };
    Ok(ProvisionalSemanticPrefixRecipe {
        primary: lease.primary,
        control: lease.control,
        runs,
        references,
        table_probe,
    })
}

fn require_nonempty(runs: &SourceRuns) -> Result<(), GateError> {
    if runs.byte_len() == 0 {
        Err(GateError::EmptySourceRuns)
    } else {
        Ok(())
    }
}

fn validate_table_request(
    header: &SourceRuns,
    delimiter: &SourceRuns,
    shape: &TableShapeRecipe,
    references: &ReferenceSummary,
) -> Result<(), GateError> {
    require_nonempty(header)?;
    require_nonempty(delimiter)?;
    if shape.columns == 0
        || usize::from(shape.columns) != shape.header_cells.len()
        || shape.columns > 256
    {
        return Err(GateError::InvalidOutcome(
            "table shape is not a bounded exact header partition",
        ));
    }
    if !references.fresh_finalization_suppressed {
        return Err(GateError::InvalidOutcome(
            "table activation must suppress fresh Paragraph reference finalization",
        ));
    }
    Ok(())
}

fn paragraph_reference_footprint(summary: &ReferenceSummary) -> ReferenceFootprint {
    ReferenceFootprint::Paragraph {
        finalized_definitions: summary.finalized_definitions,
        changed_labels: Arc::clone(&summary.changed_labels),
    }
}

fn heading_reference_footprint(summary: &ReferenceSummary) -> ReferenceFootprint {
    ReferenceFootprint::Heading {
        reowned_to_parent: summary.reowned_to_parent,
        changed_labels: Arc::clone(&summary.changed_labels),
    }
}

fn reference_only_footprint(summary: &ReferenceSummary) -> ReferenceFootprint {
    ReferenceFootprint::ReferenceOnly {
        finalized_definitions: summary.finalized_definitions,
        changed_labels: Arc::clone(&summary.changed_labels),
    }
}

fn table_reference_footprint(summary: &ReferenceSummary) -> ReferenceFootprint {
    ReferenceFootprint::Table {
        fresh_finalization_suppressed: summary.fresh_finalization_suppressed,
        stale_occurrences_retired: summary.stale_occurrences_retired,
        changed_labels: Arc::clone(&summary.changed_labels),
    }
}

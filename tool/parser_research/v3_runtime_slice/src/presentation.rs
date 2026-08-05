//! Bounded, exact presentation facts for one active or viewport request.
//!
//! This is deliberately not a Markdown grammar and not a prediction layer.
//! An exact parser may publish compact facts into this gate; the UI may adopt
//! them only when source revision, grammar revision, parse generation, request
//! identity, scope, and queried range all match. Missing or mismatched output
//! is explicit unknown state and must source-paint.
//!
//! Host/layout lifetime is intentionally separate from semantic lifetime. A
//! [`HostLayoutLease`] contains no parser epoch, while a
//! [`PresentationFactLease`] is a non-clone arena owner for one exact epoch.
//!
//! The fixed-width manual codec is proof scaffolding: it is a semantic and
//! lifetime witness, not a claim that 56 bytes per fact is final production
//! packing. Production should generate codec/validation code from one schema,
//! then may delta-code anchors and factor repeated IDs after this contract holds.

use std::cmp::Ordering;
use std::fmt;

use crate::arena::{ArenaBuildTransaction, ArenaOwnerHandle};
use crate::{
    ARENA_PAGE_BYTES, ArenaError, ForestAnchor, ForestBlockId, ForestPropertyId, ForestRunCursorId,
    GrammarRevision, OwnedArenaRef, OwnerTransferError, PageArena, ParseGeneration,
    RecordForestError, SourceRevision, record_forest::CoverageOrderOracle,
};

const FORMAT_VERSION: u8 = 1;
const FACT_PAGE_TAG: u8 = 0x71;
const MANIFEST_TAG: u8 = 0x72;

pub const PRESENTATION_FACT_PAGE_HEADER_BYTES: usize = 8;
pub const PRESENTATION_PACKED_FACT_BYTES: usize = 56;
pub const PRESENTATION_MANIFEST_BYTES: usize = 96;
pub const PRESENTATION_FACTS_PER_PAGE: usize =
    (ARENA_PAGE_BYTES - PRESENTATION_FACT_PAGE_HEADER_BYTES) / PRESENTATION_PACKED_FACT_BYTES;
pub const PRESENTATION_HARD_MAX_PAGES: u16 = 8;
pub const PRESENTATION_HARD_MAX_RECORDS: u32 = 584;
pub const PRESENTATION_HARD_MAX_ARENA_PAYLOAD_BYTES: u32 = 32_864;

const FENCE_PROPERTY_PRESENT: u8 = 0x80;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationRequestId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationHostId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutGeneration(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplacementSymbolId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineSyntaxTag(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplacementTag(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PresentationStyleTag(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AmbiguityTag(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandScopeTag(pub u16);

/// Composite semantic-root generation.
///
/// This must advance whenever any semantic dependency visible to presentation
/// changes, including lazy reference-symbol resolution that does not edit the
/// source or restart the block parser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticRootGeneration(pub u64);

/// Dimensions an exact output atomically certifies as complete.
///
/// Block-forest authority belongs to the enclosing record-forest/composite
/// root, not this standalone fact leaf. These bits cover only presentation
/// dimensions this module can actually own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationAuthority(u32);

impl PresentationAuthority {
    pub const INLINE_PROJECTION: Self = Self(1 << 0);
    pub const REFERENCE_RESOLUTION: Self = Self(1 << 1);
    pub const INTERACTION_TARGETS: Self = Self(1 << 2);
    pub const COMMAND_CAPABILITIES: Self = Self(1 << 3);
    pub const NONE: Self = Self(0);
    const ALL_BITS: u32 = Self::INLINE_PROJECTION.0
        | Self::REFERENCE_RESOLUTION.0
        | Self::INTERACTION_TARGETS.0
        | Self::COMMAND_CAPABILITIES.0;

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    const fn is_valid(self) -> bool {
        self.0 & !Self::ALL_BITS == 0
    }
}

/// The only two scopes accepted by this storage gate.
///
/// There is intentionally no document-wide variant. Callers needing another
/// region must issue another bounded request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PresentationRequestScope {
    Viewport = 1,
    ActiveEdit = 2,
}

impl PresentationRequestScope {
    fn from_u8(value: u8) -> Result<Self, PresentationError> {
        match value {
            1 => Ok(Self::Viewport),
            2 => Ok(Self::ActiveEdit),
            _ => Err(PresentationError::Corrupt("unknown request scope")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationEpoch {
    pub source: SourceRevision,
    pub grammar: GrammarRevision,
    pub generation: ParseGeneration,
    pub semantic_root: SemanticRootGeneration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationRange {
    pub start: ForestAnchor,
    pub end: ForestAnchor,
}

impl PresentationRange {
    #[must_use]
    pub const fn point(anchor: ForestAnchor) -> Self {
        Self {
            start: anchor,
            end: anchor,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationRequest {
    pub id: PresentationRequestId,
    pub scope: PresentationRequestScope,
    pub range: PresentationRange,
    /// Dimensions the consumer needs to adopt atomically for this query.
    pub required_authority: PresentationAuthority,
}

/// Revision-independent identity for an already-mounted UI/layout surface.
///
/// Keeping this value alive does not authorize any semantic fact. It lets the
/// UI preserve widget identity and cached layout while an exact semantic lease
/// is missing, stale, or being replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostLayoutLease {
    host: PresentationHostId,
    layout_generation: LayoutGeneration,
}

impl HostLayoutLease {
    #[must_use]
    pub const fn new(host: PresentationHostId, layout_generation: LayoutGeneration) -> Self {
        Self {
            host,
            layout_generation,
        }
    }

    #[must_use]
    pub const fn host(&self) -> PresentationHostId {
        self.host
    }

    #[must_use]
    pub const fn layout_generation(&self) -> LayoutGeneration {
        self.layout_generation
    }

    /// Advances layout state without changing the stable host identity.
    #[must_use]
    pub const fn renew(self, layout_generation: LayoutGeneration) -> Self {
        Self {
            host: self.host,
            layout_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RunEdgeKind {
    Start = 1,
    End = 2,
    SplitBefore = 3,
    SplitAfter = 4,
}

impl RunEdgeKind {
    fn from_u8(value: u8) -> Result<Self, PresentationError> {
        match value {
            1 => Ok(Self::Start),
            2 => Ok(Self::End),
            3 => Ok(Self::SplitBefore),
            4 => Ok(Self::SplitAfter),
            _ => Err(PresentationError::Corrupt("unknown run edge kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TableTargetRole {
    Delimiter = 1,
    Cell = 2,
    Row = 3,
}

impl TableTargetRole {
    fn from_u8(value: u8) -> Result<Self, PresentationError> {
        match value {
            1 => Ok(Self::Delimiter),
            2 => Ok(Self::Cell),
            3 => Ok(Self::Row),
            _ => Err(PresentationError::Corrupt("unknown table target role")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FenceTargetRole {
    OpeningMarker = 1,
    Info = 2,
    Body = 3,
    ClosingMarker = 4,
}

impl FenceTargetRole {
    fn from_u8(value: u8) -> Result<Self, PresentationError> {
        match value {
            1 => Ok(Self::OpeningMarker),
            2 => Ok(Self::Info),
            3 => Ok(Self::Body),
            4 => Ok(Self::ClosingMarker),
            _ => Err(PresentationError::Corrupt("unknown fence target role")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskTargetState {
    Unchecked = 1,
    Checked = 2,
    Indeterminate = 3,
}

impl TaskTargetState {
    fn from_u8(value: u8) -> Result<Self, PresentationError> {
        match value {
            1 => Ok(Self::Unchecked),
            2 => Ok(Self::Checked),
            3 => Ok(Self::Indeterminate),
            _ => Err(PresentationError::Corrupt("unknown task target state")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandCapabilities(pub u64);

impl CommandCapabilities {
    pub const TOGGLE: Self = Self(1 << 0);
    pub const OPEN: Self = Self(1 << 1);
    pub const COPY: Self = Self(1 << 2);
    pub const NAVIGATE: Self = Self(1 << 3);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Compact, source-relative presentation facts. No variant owns source text,
/// Crop roots, child vectors, or transformed strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationFact {
    InlineHidden {
        range: PresentationRange,
        syntax: InlineSyntaxTag,
        nesting: u8,
    },
    Replacement {
        range: PresentationRange,
        replacement: ReplacementTag,
        symbol: ReplacementSymbolId,
    },
    Style {
        range: PresentationRange,
        style: PresentationStyleTag,
        layer: u8,
    },
    Ambiguity {
        range: PresentationRange,
        ambiguity: AmbiguityTag,
        alternatives: u8,
    },
    RunEdge {
        at: ForestAnchor,
        run: ForestRunCursorId,
        edge: RunEdgeKind,
        ordinal: u32,
    },
    TableTarget {
        range: PresentationRange,
        table: ForestBlockId,
        role: TableTargetRole,
        row: u32,
        column: u32,
    },
    FenceTarget {
        range: PresentationRange,
        fence: ForestBlockId,
        role: FenceTargetRole,
        property: Option<ForestPropertyId>,
    },
    TaskTarget {
        range: PresentationRange,
        item: ForestBlockId,
        state: TaskTargetState,
    },
    CommandCapabilities {
        range: PresentationRange,
        target: ForestBlockId,
        scope: CommandScopeTag,
        capabilities: CommandCapabilities,
    },
}

impl PresentationFact {
    #[must_use]
    pub const fn range(self) -> PresentationRange {
        match self {
            Self::InlineHidden { range, .. }
            | Self::Replacement { range, .. }
            | Self::Style { range, .. }
            | Self::Ambiguity { range, .. }
            | Self::TableTarget { range, .. }
            | Self::FenceTarget { range, .. }
            | Self::TaskTarget { range, .. }
            | Self::CommandCapabilities { range, .. } => range,
            Self::RunEdge { at, .. } => PresentationRange::point(at),
        }
    }

    #[must_use]
    pub const fn class(self) -> PresentationFactClass {
        match self {
            Self::InlineHidden { .. } => PresentationFactClass::InlineHidden,
            Self::Replacement { .. } => PresentationFactClass::Replacement,
            Self::Style { .. } => PresentationFactClass::Style,
            Self::Ambiguity { .. } => PresentationFactClass::Ambiguity,
            Self::RunEdge { .. } => PresentationFactClass::RunEdge,
            Self::TableTarget { .. } => PresentationFactClass::TableTarget,
            Self::FenceTarget { .. } => PresentationFactClass::FenceTarget,
            Self::TaskTarget { .. } => PresentationFactClass::TaskTarget,
            Self::CommandCapabilities { .. } => PresentationFactClass::CommandCapabilities,
        }
    }

    #[must_use]
    pub const fn required_authority(self) -> PresentationAuthority {
        self.class().required_authority()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PresentationFactClass {
    InlineHidden = 1,
    Replacement = 2,
    Style = 3,
    Ambiguity = 4,
    RunEdge = 5,
    TableTarget = 6,
    FenceTarget = 7,
    TaskTarget = 8,
    CommandCapabilities = 9,
}

impl PresentationFactClass {
    fn from_u8(value: u8) -> Result<Self, PresentationError> {
        match value {
            1 => Ok(Self::InlineHidden),
            2 => Ok(Self::Replacement),
            3 => Ok(Self::Style),
            4 => Ok(Self::Ambiguity),
            5 => Ok(Self::RunEdge),
            6 => Ok(Self::TableTarget),
            7 => Ok(Self::FenceTarget),
            8 => Ok(Self::TaskTarget),
            9 => Ok(Self::CommandCapabilities),
            _ => Err(PresentationError::Corrupt(
                "unknown presentation fact class",
            )),
        }
    }

    #[must_use]
    pub const fn required_authority(self) -> PresentationAuthority {
        match self {
            Self::InlineHidden
            | Self::Replacement
            | Self::Style
            | Self::Ambiguity
            | Self::RunEdge => PresentationAuthority::INLINE_PROJECTION,
            Self::TableTarget | Self::FenceTarget | Self::TaskTarget => {
                PresentationAuthority::INTERACTION_TARGETS
            }
            Self::CommandCapabilities => PresentationAuthority::COMMAND_CAPABILITIES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationCap {
    Records,
    Pages,
    ArenaPayloadBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationUnknownReason {
    MissingLease,
    CapExceeded(PresentationCap),
    StaleSourceRevision,
    StaleGrammarRevision,
    StaleParseGeneration,
    StaleSemanticRoot,
    WrongRequest,
    OutsideProvenRange,
    IncompleteAuthority {
        required: PresentationAuthority,
        certified: PresentationAuthority,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationUnknownRange {
    pub range: PresentationRange,
    pub reason: PresentationUnknownReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationBudget {
    pub max_pages: u16,
    pub max_records: u32,
    /// Includes the immutable manifest and every packed fact page.
    pub max_arena_payload_bytes: u32,
}

impl PresentationBudget {
    /// Constructs a per-request budget, clamped to the gate's hard upper bound.
    /// A caller cannot turn this active/viewport store into document-wide state.
    #[must_use]
    pub const fn new(max_pages: u16, max_records: u32, max_arena_payload_bytes: u32) -> Self {
        Self {
            max_pages: if max_pages < PRESENTATION_HARD_MAX_PAGES {
                max_pages
            } else {
                PRESENTATION_HARD_MAX_PAGES
            },
            max_records: if max_records < PRESENTATION_HARD_MAX_RECORDS {
                max_records
            } else {
                PRESENTATION_HARD_MAX_RECORDS
            },
            max_arena_payload_bytes: if max_arena_payload_bytes
                < PRESENTATION_HARD_MAX_ARENA_PAYLOAD_BYTES
            {
                max_arena_payload_bytes
            } else {
                PRESENTATION_HARD_MAX_ARENA_PAYLOAD_BYTES
            },
        }
    }

    #[must_use]
    pub const fn hard_max() -> Self {
        Self {
            max_pages: PRESENTATION_HARD_MAX_PAGES,
            max_records: PRESENTATION_HARD_MAX_RECORDS,
            max_arena_payload_bytes: PRESENTATION_HARD_MAX_ARENA_PAYLOAD_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationBuildReceipt {
    pub fact_pages_allocated: usize,
    pub manifest_pages_allocated: usize,
    pub facts_packed: usize,
    pub arena_payload_bytes: usize,
    pub child_references_added: usize,
    pub maximum_page_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationPushResult {
    Accepted,
    BecameUnknown(PresentationUnknownRange),
    AlreadyUnknown(PresentationUnknownRange),
}

#[derive(Debug)]
pub enum PresentationBuildOutcome {
    Exact {
        lease: PresentationFactLease,
        receipt: PresentationBuildReceipt,
    },
    Unknown(PresentationUnknownRange),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactPresentationFacts {
    pub epoch: PresentationEpoch,
    pub request: PresentationRequest,
    pub facts: Vec<PresentationFact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresentationLookup {
    Exact(ExactPresentationFacts),
    Unknown(PresentationUnknownRange),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationError {
    Arena(ArenaError),
    Coverage(RecordForestError),
    Corrupt(&'static str),
    Invalid(&'static str),
    Overflow(&'static str),
}

impl From<ArenaError> for PresentationError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<RecordForestError> for PresentationError {
    fn from(value: RecordForestError) -> Self {
        Self::Coverage(value)
    }
}

fn legacy_owner_transfer_error(failure: OwnerTransferError) -> PresentationError {
    // This proof presentation codec still exposes a copyable legacy error.
    // Keep its lossy bridge explicit; selected adoption paths return the owner.
    let OwnerTransferError { error, owner } = failure;
    drop(owner);
    PresentationError::Arena(error)
}

impl fmt::Display for PresentationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::Coverage(error) => error.fmt(formatter),
            Self::Corrupt(message) => write!(formatter, "corrupt presentation output: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid presentation output: {message}"),
            Self::Overflow(field) => write!(formatter, "presentation output {field} overflow"),
        }
    }
}

impl std::error::Error for PresentationError {}

#[derive(Debug)]
pub struct PresentationFactBuilder {
    epoch: PresentationEpoch,
    request: PresentationRequest,
    budget: PresentationBudget,
    certified_authority: PresentationAuthority,
    facts: Vec<PresentationFact>,
    unknown: Option<PresentationUnknownRange>,
}

impl PresentationFactBuilder {
    #[must_use]
    pub fn new(
        epoch: PresentationEpoch,
        request: PresentationRequest,
        certified_authority: PresentationAuthority,
        budget: PresentationBudget,
    ) -> Self {
        let unknown = (usize::try_from(budget.max_arena_payload_bytes).unwrap_or(usize::MAX)
            < PRESENTATION_MANIFEST_BYTES)
            .then_some(PresentationUnknownRange {
                range: request.range,
                reason: PresentationUnknownReason::CapExceeded(PresentationCap::ArenaPayloadBytes),
            });
        Self {
            epoch,
            request,
            budget,
            certified_authority,
            facts: Vec::new(),
            unknown,
        }
    }

    #[must_use]
    pub fn push(&mut self, fact: PresentationFact) -> PresentationPushResult {
        if let Some(unknown) = self.unknown {
            return PresentationPushResult::AlreadyUnknown(unknown);
        }
        let Some(next_count) = self.facts.len().checked_add(1) else {
            return self.fail_cap(PresentationCap::Records);
        };
        let Some((pages, bytes)) = required_shape(next_count) else {
            return self.fail_cap(PresentationCap::ArenaPayloadBytes);
        };
        if next_count > usize::try_from(self.budget.max_records).unwrap_or(usize::MAX) {
            return self.fail_cap(PresentationCap::Records);
        }
        if pages > usize::from(self.budget.max_pages) {
            return self.fail_cap(PresentationCap::Pages);
        }
        if bytes > usize::try_from(self.budget.max_arena_payload_bytes).unwrap_or(usize::MAX) {
            return self.fail_cap(PresentationCap::ArenaPayloadBytes);
        }
        self.facts.push(fact);
        PresentationPushResult::Accepted
    }

    fn fail_cap(&mut self, cap: PresentationCap) -> PresentationPushResult {
        self.facts.clear();
        let unknown = PresentationUnknownRange {
            range: self.request.range,
            reason: PresentationUnknownReason::CapExceeded(cap),
        };
        self.unknown = Some(unknown);
        PresentationPushResult::BecameUnknown(unknown)
    }

    #[allow(clippy::too_many_lines)] // One transaction proves preflight-to-atomic-manifest ownership.
    pub fn finish(
        self,
        arena: &mut PageArena,
        coverage_order: &impl CoverageOrderOracle,
    ) -> Result<PresentationBuildOutcome, PresentationError> {
        if let Some(unknown) = self.unknown {
            return Ok(PresentationBuildOutcome::Unknown(unknown));
        }
        if !self.certified_authority.is_valid() || !self.request.required_authority.is_valid() {
            return Err(PresentationError::Invalid("unknown authority dimension"));
        }
        if !self
            .certified_authority
            .contains(self.request.required_authority)
        {
            return Ok(PresentationBuildOutcome::Unknown(
                PresentationUnknownRange {
                    range: self.request.range,
                    reason: PresentationUnknownReason::IncompleteAuthority {
                        required: self.request.required_authority,
                        certified: self.certified_authority,
                    },
                },
            ));
        }
        validate_range(self.request.range, coverage_order)?;
        for fact in &self.facts {
            if !self.certified_authority.contains(fact.required_authority()) {
                return Err(PresentationError::Invalid(
                    "fact class was not certified complete",
                ));
            }
            let range = fact.range();
            validate_range(range, coverage_order)?;
            if !range_contains(self.request.range, range, coverage_order)? {
                return Err(PresentationError::Invalid(
                    "fact lies outside its active or viewport request",
                ));
            }
        }
        let (page_count, total_payload_bytes) =
            required_shape(self.facts.len()).ok_or(PresentationError::Overflow("packed bytes"))?;
        if self.facts.len() > usize::try_from(self.budget.max_records).unwrap_or(usize::MAX) {
            return Ok(PresentationBuildOutcome::Unknown(
                PresentationUnknownRange {
                    range: self.request.range,
                    reason: PresentationUnknownReason::CapExceeded(PresentationCap::Records),
                },
            ));
        }
        if page_count > usize::from(self.budget.max_pages) {
            return Ok(PresentationBuildOutcome::Unknown(
                PresentationUnknownRange {
                    range: self.request.range,
                    reason: PresentationUnknownReason::CapExceeded(PresentationCap::Pages),
                },
            ));
        }
        if total_payload_bytes
            > usize::try_from(self.budget.max_arena_payload_bytes).unwrap_or(usize::MAX)
        {
            return Ok(PresentationBuildOutcome::Unknown(
                PresentationUnknownRange {
                    range: self.request.range,
                    reason: PresentationUnknownReason::CapExceeded(
                        PresentationCap::ArenaPayloadBytes,
                    ),
                },
            ));
        }

        let mut receipt = PresentationBuildReceipt::default();
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut chain: Option<ArenaOwnerHandle> = None;
        for (page_index, facts) in self.facts.chunks(PRESENTATION_FACTS_PER_PAGE).enumerate() {
            let payload = encode_fact_page(page_index, facts)?;
            let children = chain
                .as_ref()
                .map_or_else(Vec::new, |owner| vec![transaction.id(owner)]);
            let (new_owner, allocation) = transaction.allocate(&payload, &children)?;
            receipt.fact_pages_allocated += 1;
            receipt.facts_packed += facts.len();
            receipt.arena_payload_bytes += allocation.payload_bytes_copied;
            receipt.child_references_added += allocation.child_references_added;
            receipt.maximum_page_payload_bytes = receipt
                .maximum_page_payload_bytes
                .max(allocation.payload_bytes_copied);
            if let Some(old) = chain {
                transaction.release(old)?;
            }
            chain = Some(new_owner);
        }

        let manifest_payload = encode_manifest(
            self.epoch,
            self.request,
            self.certified_authority,
            self.facts.len(),
            page_count,
            total_payload_bytes,
        )?;
        let children = chain
            .as_ref()
            .map_or_else(Vec::new, |owner| vec![transaction.id(owner)]);
        let (manifest_owner, allocation) = transaction.allocate(&manifest_payload, &children)?;
        receipt.manifest_pages_allocated = 1;
        receipt.arena_payload_bytes += allocation.payload_bytes_copied;
        receipt.child_references_added += allocation.child_references_added;
        receipt.maximum_page_payload_bytes = receipt
            .maximum_page_payload_bytes
            .max(allocation.payload_bytes_copied);
        if let Some(old) = chain {
            transaction.release(old)?;
        }
        let manifest_owner = transaction.take(manifest_owner);
        debug_assert_eq!(receipt.arena_payload_bytes, total_payload_bytes);
        debug_assert!(receipt.maximum_page_payload_bytes <= ARENA_PAGE_BYTES);
        Ok(PresentationBuildOutcome::Exact {
            lease: PresentationFactLease {
                owner: Some(manifest_owner),
            },
            receipt,
        })
    }
}

/// Sole owner of one immutable semantic-fact manifest.
///
/// This is intentionally non-`Clone` and exposes no unbound fact accessor.
/// Call [`Self::query`] with the current epoch and request before facts can be
/// decoded. Retirement is scheduled into [`PageArena`]'s fuelled queue.
#[derive(Debug)]
#[must_use = "semantic fact leases must be queried or explicitly retired"]
pub struct PresentationFactLease {
    owner: Option<OwnedArenaRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PresentationContract {
    pub epoch: PresentationEpoch,
    pub request: PresentationRequest,
}

impl PresentationFactLease {
    fn root(&self) -> Result<crate::ArenaId, PresentationError> {
        self.owner
            .as_ref()
            .map(OwnedArenaRef::id)
            .ok_or(PresentationError::Invalid("semantic lease was retired"))
    }

    pub fn query(
        &self,
        arena: &PageArena,
        expected_epoch: PresentationEpoch,
        requested: PresentationRequest,
        coverage_order: &impl CoverageOrderOracle,
    ) -> Result<PresentationLookup, PresentationError> {
        let root = self.root()?;
        query_presentation_root(arena, root, expected_epoch, requested, coverage_order)
    }

    /// Queries one required class. Exact coverage with no such fact is still
    /// surfaced as unknown to callers that require that semantic capability.
    pub fn query_required_class(
        &self,
        arena: &PageArena,
        expected_epoch: PresentationEpoch,
        requested: PresentationRequest,
        class: PresentationFactClass,
        coverage_order: &impl CoverageOrderOracle,
    ) -> Result<PresentationLookup, PresentationError> {
        let requested = PresentationRequest {
            required_authority: requested
                .required_authority
                .union(class.required_authority()),
            ..requested
        };
        match self.query(arena, expected_epoch, requested, coverage_order)? {
            PresentationLookup::Exact(mut exact) => {
                exact.facts.retain(|fact| fact.class() == class);
                // Exact-empty is meaningful only because query() proved this
                // class's authority dimension complete above.
                Ok(PresentationLookup::Exact(exact))
            }
            unknown @ PresentationLookup::Unknown(_) => Ok(unknown),
        }
    }

    pub fn release_later(mut self, arena: &mut PageArena) -> Result<(), PresentationError> {
        let owner = self.owner.take().ok_or(PresentationError::Invalid(
            "semantic lease was already retired",
        ))?;
        arena
            .release_later(owner)
            .map_err(legacy_owner_transfer_error)?;
        Ok(())
    }

    /// Transfers this leaf's sole owner into a composite output manifest.
    pub(crate) fn into_owner(mut self) -> OwnedArenaRef {
        self.owner
            .take()
            .expect("live presentation lease owns its manifest")
    }
}

pub(crate) fn presentation_contract_root(
    arena: &PageArena,
    root: crate::ArenaId,
) -> Result<PresentationContract, PresentationError> {
    let manifest = decode_manifest(arena, root)?;
    Ok(PresentationContract {
        epoch: manifest.epoch,
        request: manifest.request,
    })
}

pub(crate) fn query_presentation_root(
    arena: &PageArena,
    root: crate::ArenaId,
    expected_epoch: PresentationEpoch,
    requested: PresentationRequest,
    coverage_order: &impl CoverageOrderOracle,
) -> Result<PresentationLookup, PresentationError> {
    let manifest = decode_manifest(arena, root)?;
    if manifest.epoch.source != expected_epoch.source {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::StaleSourceRevision,
        ));
    }
    if manifest.epoch.grammar != expected_epoch.grammar {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::StaleGrammarRevision,
        ));
    }
    if manifest.epoch.generation != expected_epoch.generation {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::StaleParseGeneration,
        ));
    }
    if manifest.epoch.semantic_root != expected_epoch.semantic_root {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::StaleSemanticRoot,
        ));
    }
    if manifest.request.id != requested.id || manifest.request.scope != requested.scope {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::WrongRequest,
        ));
    }
    if !requested.required_authority.is_valid() {
        return Err(PresentationError::Invalid(
            "unknown requested authority dimension",
        ));
    }
    if !manifest
        .certified_authority
        .contains(requested.required_authority)
    {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::IncompleteAuthority {
                required: requested.required_authority,
                certified: manifest.certified_authority,
            },
        ));
    }
    validate_range(requested.range, coverage_order)?;
    if !range_contains(manifest.request.range, requested.range, coverage_order)? {
        return Ok(unknown(
            requested.range,
            PresentationUnknownReason::OutsideProvenRange,
        ));
    }
    let mut facts = Vec::new();
    for fact in decode_fact_chain(arena, root, manifest)? {
        let fact_range = fact.range();
        validate_range(fact_range, coverage_order)?;
        if !manifest
            .certified_authority
            .contains(fact.required_authority())
        {
            return Err(PresentationError::Corrupt(
                "fact class lacks certified authority",
            ));
        }
        if !range_contains(manifest.request.range, fact_range, coverage_order)? {
            return Err(PresentationError::Corrupt(
                "fact lies outside manifest range",
            ));
        }
        if ranges_intersect(fact_range, requested.range, coverage_order)? {
            facts.push(fact);
        }
    }
    Ok(PresentationLookup::Exact(ExactPresentationFacts {
        epoch: manifest.epoch,
        request: requested,
        facts,
    }))
}

/// Absence of a semantic lease is a first-class unknown result. Keeping a
/// [`HostLayoutLease`] alive never changes this outcome.
pub fn query_optional_presentation(
    lease: Option<&PresentationFactLease>,
    arena: &PageArena,
    expected_epoch: PresentationEpoch,
    requested: PresentationRequest,
    coverage_order: &impl CoverageOrderOracle,
) -> Result<PresentationLookup, PresentationError> {
    lease.map_or_else(
        || {
            Ok(unknown(
                requested.range,
                PresentationUnknownReason::MissingLease,
            ))
        },
        |lease| lease.query(arena, expected_epoch, requested, coverage_order),
    )
}

fn unknown(range: PresentationRange, reason: PresentationUnknownReason) -> PresentationLookup {
    PresentationLookup::Unknown(PresentationUnknownRange { range, reason })
}

#[derive(Clone, Copy, Debug)]
struct ManifestView {
    epoch: PresentationEpoch,
    request: PresentationRequest,
    certified_authority: PresentationAuthority,
    fact_count: usize,
    page_count: usize,
    total_payload_bytes: usize,
}

fn required_shape(fact_count: usize) -> Option<(usize, usize)> {
    let pages = fact_count.checked_add(PRESENTATION_FACTS_PER_PAGE.checked_sub(1)?)?
        / PRESENTATION_FACTS_PER_PAGE;
    let facts_bytes = fact_count.checked_mul(PRESENTATION_PACKED_FACT_BYTES)?;
    let page_headers = pages.checked_mul(PRESENTATION_FACT_PAGE_HEADER_BYTES)?;
    let total = PRESENTATION_MANIFEST_BYTES
        .checked_add(facts_bytes)?
        .checked_add(page_headers)?;
    Some((pages, total))
}

fn encode_fact_page(
    page_index: usize,
    facts: &[PresentationFact],
) -> Result<Vec<u8>, PresentationError> {
    if facts.is_empty() || facts.len() > PRESENTATION_FACTS_PER_PAGE {
        return Err(PresentationError::Invalid("invalid fact page count"));
    }
    let expected = PRESENTATION_FACT_PAGE_HEADER_BYTES
        .checked_add(
            facts
                .len()
                .checked_mul(PRESENTATION_PACKED_FACT_BYTES)
                .ok_or(PresentationError::Overflow("fact page bytes"))?,
        )
        .ok_or(PresentationError::Overflow("fact page bytes"))?;
    if expected > ARENA_PAGE_BYTES {
        return Err(PresentationError::Invalid("fact page exceeds arena page"));
    }
    let mut payload = Vec::with_capacity(expected);
    payload.push(FACT_PAGE_TAG);
    payload.push(FORMAT_VERSION);
    push_u16(
        &mut payload,
        u16::try_from(facts.len()).map_err(|_| PresentationError::Overflow("fact count"))?,
    );
    push_u32(
        &mut payload,
        u32::try_from(page_index).map_err(|_| PresentationError::Overflow("page index"))?,
    );
    for fact in facts {
        encode_fact(*fact, &mut payload);
    }
    debug_assert_eq!(payload.len(), expected);
    Ok(payload)
}

#[allow(clippy::too_many_lines)] // Proof codec stays visibly exhaustive until schema generation.
fn encode_fact(fact: PresentationFact, output: &mut Vec<u8>) {
    let start = output.len();
    let (class, flags, tag, auxiliary, owner, value, range) = match fact {
        PresentationFact::InlineHidden {
            range,
            syntax,
            nesting,
        } => (
            PresentationFactClass::InlineHidden,
            nesting,
            syntax.0,
            0,
            0,
            0,
            range,
        ),
        PresentationFact::Replacement {
            range,
            replacement,
            symbol,
        } => (
            PresentationFactClass::Replacement,
            0,
            replacement.0,
            0,
            0,
            symbol.0,
            range,
        ),
        PresentationFact::Style {
            range,
            style,
            layer,
        } => (PresentationFactClass::Style, layer, style.0, 0, 0, 0, range),
        PresentationFact::Ambiguity {
            range,
            ambiguity,
            alternatives,
        } => (
            PresentationFactClass::Ambiguity,
            alternatives,
            ambiguity.0,
            0,
            0,
            0,
            range,
        ),
        PresentationFact::RunEdge {
            at,
            run,
            edge,
            ordinal,
        } => (
            PresentationFactClass::RunEdge,
            edge as u8,
            0,
            ordinal,
            run.0,
            0,
            PresentationRange::point(at),
        ),
        PresentationFact::TableTarget {
            range,
            table,
            role,
            row,
            column,
        } => (
            PresentationFactClass::TableTarget,
            role as u8,
            0,
            row,
            table.0,
            u64::from(column),
            range,
        ),
        PresentationFact::FenceTarget {
            range,
            fence,
            role,
            property,
        } => (
            PresentationFactClass::FenceTarget,
            role as u8 | property.map_or(0, |_| FENCE_PROPERTY_PRESENT),
            0,
            0,
            fence.0,
            property.map_or(0, |property| property.0),
            range,
        ),
        PresentationFact::TaskTarget { range, item, state } => (
            PresentationFactClass::TaskTarget,
            state as u8,
            0,
            0,
            item.0,
            0,
            range,
        ),
        PresentationFact::CommandCapabilities {
            range,
            target,
            scope,
            capabilities,
        } => (
            PresentationFactClass::CommandCapabilities,
            0,
            scope.0,
            0,
            target.0,
            capabilities.0,
            range,
        ),
    };
    output.push(class as u8);
    output.push(flags);
    push_u16(output, tag);
    push_u32(output, auxiliary);
    push_u64(output, owner);
    push_u64(output, value);
    encode_range(range, output);
    debug_assert_eq!(output.len() - start, PRESENTATION_PACKED_FACT_BYTES);
}

fn decode_fact_page(
    payload: &[u8],
    expected_index: usize,
) -> Result<Vec<PresentationFact>, PresentationError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != FACT_PAGE_TAG || decoder.u8()? != FORMAT_VERSION {
        return Err(PresentationError::Corrupt("wrong fact page header"));
    }
    let count = usize::from(decoder.u16()?);
    let page_index =
        usize::try_from(decoder.u32()?).map_err(|_| PresentationError::Overflow("page index"))?;
    if page_index != expected_index || count == 0 || count > PRESENTATION_FACTS_PER_PAGE {
        return Err(PresentationError::Corrupt(
            "invalid fact page index or count",
        ));
    }
    let expected = PRESENTATION_FACT_PAGE_HEADER_BYTES
        .checked_add(
            count
                .checked_mul(PRESENTATION_PACKED_FACT_BYTES)
                .ok_or(PresentationError::Overflow("fact page bytes"))?,
        )
        .ok_or(PresentationError::Overflow("fact page bytes"))?;
    if payload.len() != expected || payload.len() > ARENA_PAGE_BYTES {
        return Err(PresentationError::Corrupt("wrong fact page size"));
    }
    let mut facts = Vec::with_capacity(count);
    for _ in 0..count {
        facts.push(decode_fact(&mut decoder)?);
    }
    if !decoder.finished() {
        return Err(PresentationError::Corrupt("trailing fact page bytes"));
    }
    Ok(facts)
}

#[allow(clippy::too_many_lines)] // Proof decoder mirrors every schema case explicitly.
fn decode_fact(decoder: &mut Decoder<'_>) -> Result<PresentationFact, PresentationError> {
    let class = PresentationFactClass::from_u8(decoder.u8()?)?;
    let flags = decoder.u8()?;
    let tag = decoder.u16()?;
    let auxiliary = decoder.u32()?;
    let owner = decoder.u64()?;
    let value = decoder.u64()?;
    let range = decoder.range()?;
    match class {
        PresentationFactClass::InlineHidden => {
            require(
                auxiliary == 0 && owner == 0 && value == 0,
                "hidden fact padding",
            )?;
            Ok(PresentationFact::InlineHidden {
                range,
                syntax: InlineSyntaxTag(tag),
                nesting: flags,
            })
        }
        PresentationFactClass::Replacement => {
            require(
                flags == 0 && auxiliary == 0 && owner == 0,
                "replacement fact padding",
            )?;
            Ok(PresentationFact::Replacement {
                range,
                replacement: ReplacementTag(tag),
                symbol: ReplacementSymbolId(value),
            })
        }
        PresentationFactClass::Style => {
            require(
                auxiliary == 0 && owner == 0 && value == 0,
                "style fact padding",
            )?;
            Ok(PresentationFact::Style {
                range,
                style: PresentationStyleTag(tag),
                layer: flags,
            })
        }
        PresentationFactClass::Ambiguity => {
            require(
                auxiliary == 0 && owner == 0 && value == 0,
                "ambiguity fact padding",
            )?;
            Ok(PresentationFact::Ambiguity {
                range,
                ambiguity: AmbiguityTag(tag),
                alternatives: flags,
            })
        }
        PresentationFactClass::RunEdge => {
            require(tag == 0 && value == 0, "run edge fact padding")?;
            if range.start != range.end {
                return Err(PresentationError::Corrupt("run edge is not a point"));
            }
            Ok(PresentationFact::RunEdge {
                at: range.start,
                run: ForestRunCursorId(owner),
                edge: RunEdgeKind::from_u8(flags)?,
                ordinal: auxiliary,
            })
        }
        PresentationFactClass::TableTarget => {
            require(
                tag == 0 && u32::try_from(value).is_ok(),
                "table target fields",
            )?;
            Ok(PresentationFact::TableTarget {
                range,
                table: ForestBlockId(owner),
                role: TableTargetRole::from_u8(flags)?,
                row: auxiliary,
                column: u32::try_from(value)
                    .map_err(|_| PresentationError::Corrupt("table column overflow"))?,
            })
        }
        PresentationFactClass::FenceTarget => {
            require(tag == 0 && auxiliary == 0, "fence target padding")?;
            let property_present = flags & FENCE_PROPERTY_PRESENT != 0;
            let role = FenceTargetRole::from_u8(flags & !FENCE_PROPERTY_PRESENT)?;
            if !property_present && value != 0 {
                return Err(PresentationError::Corrupt("unmarked fence property"));
            }
            Ok(PresentationFact::FenceTarget {
                range,
                fence: ForestBlockId(owner),
                role,
                property: property_present.then_some(ForestPropertyId(value)),
            })
        }
        PresentationFactClass::TaskTarget => {
            require(
                tag == 0 && auxiliary == 0 && value == 0,
                "task target padding",
            )?;
            Ok(PresentationFact::TaskTarget {
                range,
                item: ForestBlockId(owner),
                state: TaskTargetState::from_u8(flags)?,
            })
        }
        PresentationFactClass::CommandCapabilities => {
            require(flags == 0 && auxiliary == 0, "command fact padding")?;
            Ok(PresentationFact::CommandCapabilities {
                range,
                target: ForestBlockId(owner),
                scope: CommandScopeTag(tag),
                capabilities: CommandCapabilities(value),
            })
        }
    }
}

fn require(condition: bool, message: &'static str) -> Result<(), PresentationError> {
    if condition {
        Ok(())
    } else {
        Err(PresentationError::Corrupt(message))
    }
}

fn encode_manifest(
    epoch: PresentationEpoch,
    request: PresentationRequest,
    certified_authority: PresentationAuthority,
    fact_count: usize,
    page_count: usize,
    total_payload_bytes: usize,
) -> Result<Vec<u8>, PresentationError> {
    let mut payload = Vec::with_capacity(PRESENTATION_MANIFEST_BYTES);
    payload.push(MANIFEST_TAG);
    payload.push(FORMAT_VERSION);
    payload.push(request.scope as u8);
    payload.push(0);
    push_u32(
        &mut payload,
        u32::try_from(fact_count).map_err(|_| PresentationError::Overflow("fact count"))?,
    );
    push_u32(
        &mut payload,
        u32::try_from(page_count).map_err(|_| PresentationError::Overflow("page count"))?,
    );
    push_u32(&mut payload, certified_authority.0);
    push_u64(&mut payload, epoch.source.0);
    push_u64(&mut payload, epoch.grammar.0);
    push_u64(&mut payload, epoch.generation.0);
    push_u64(&mut payload, epoch.semantic_root.0);
    push_u64(&mut payload, request.id.0);
    encode_range(request.range, &mut payload);
    push_u64(
        &mut payload,
        u64::try_from(total_payload_bytes)
            .map_err(|_| PresentationError::Overflow("payload byte count"))?,
    );
    debug_assert_eq!(payload.len(), PRESENTATION_MANIFEST_BYTES);
    Ok(payload)
}

fn decode_manifest(
    arena: &PageArena,
    root: crate::ArenaId,
) -> Result<ManifestView, PresentationError> {
    let payload = arena.payload(root)?;
    if payload.len() != PRESENTATION_MANIFEST_BYTES {
        return Err(PresentationError::Corrupt("wrong manifest size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != MANIFEST_TAG || decoder.u8()? != FORMAT_VERSION {
        return Err(PresentationError::Corrupt("wrong manifest header"));
    }
    let scope = PresentationRequestScope::from_u8(decoder.u8()?)?;
    if decoder.u8()? != 0 {
        return Err(PresentationError::Corrupt("manifest padding"));
    }
    let fact_count =
        usize::try_from(decoder.u32()?).map_err(|_| PresentationError::Overflow("fact count"))?;
    let page_count =
        usize::try_from(decoder.u32()?).map_err(|_| PresentationError::Overflow("page count"))?;
    let certified_authority = PresentationAuthority(decoder.u32()?);
    if !certified_authority.is_valid() {
        return Err(PresentationError::Corrupt("unknown authority dimension"));
    }
    let epoch = PresentationEpoch {
        source: SourceRevision(decoder.u64()?),
        grammar: GrammarRevision(decoder.u64()?),
        generation: ParseGeneration(decoder.u64()?),
        semantic_root: SemanticRootGeneration(decoder.u64()?),
    };
    let request = PresentationRequest {
        id: PresentationRequestId(decoder.u64()?),
        scope,
        range: decoder.range()?,
        required_authority: certified_authority,
    };
    let total_payload_bytes = usize::try_from(decoder.u64()?)
        .map_err(|_| PresentationError::Overflow("payload byte count"))?;
    if !decoder.finished() {
        return Err(PresentationError::Corrupt("trailing manifest bytes"));
    }
    let (expected_pages, expected_bytes) =
        required_shape(fact_count).ok_or(PresentationError::Overflow("packed shape"))?;
    if page_count != expected_pages || total_payload_bytes != expected_bytes {
        return Err(PresentationError::Corrupt("manifest shape mismatch"));
    }
    let children = arena.children(root)?;
    if children[1].is_some() || (page_count == 0) != children[0].is_none() {
        return Err(PresentationError::Corrupt("manifest ownership edges"));
    }
    Ok(ManifestView {
        epoch,
        request,
        certified_authority,
        fact_count,
        page_count,
        total_payload_bytes,
    })
}

fn decode_fact_chain(
    arena: &PageArena,
    root: crate::ArenaId,
    manifest: ManifestView,
) -> Result<Vec<PresentationFact>, PresentationError> {
    let mut cursor = arena.children(root)?[0];
    let mut reversed_pages = Vec::with_capacity(manifest.page_count);
    let mut packed_bytes = PRESENTATION_MANIFEST_BYTES;
    for expected_index in (0..manifest.page_count).rev() {
        let id = cursor.ok_or(PresentationError::Corrupt("short fact page chain"))?;
        let payload = arena.payload(id)?;
        packed_bytes = packed_bytes
            .checked_add(payload.len())
            .ok_or(PresentationError::Overflow("decoded payload bytes"))?;
        reversed_pages.push(decode_fact_page(payload, expected_index)?);
        let children = arena.children(id)?;
        if children[1].is_some() {
            return Err(PresentationError::Corrupt("fact page has two child edges"));
        }
        cursor = children[0];
    }
    if cursor.is_some() {
        return Err(PresentationError::Corrupt("long fact page chain"));
    }
    reversed_pages.reverse();
    let facts = reversed_pages.into_iter().flatten().collect::<Vec<_>>();
    if facts.len() != manifest.fact_count || packed_bytes != manifest.total_payload_bytes {
        return Err(PresentationError::Corrupt("decoded manifest totals"));
    }
    Ok(facts)
}

fn validate_range(
    range: PresentationRange,
    coverage_order: &impl CoverageOrderOracle,
) -> Result<(), PresentationError> {
    if compare_anchor(range.start, range.end, coverage_order)? == Ordering::Greater {
        return Err(PresentationError::Invalid("range start follows range end"));
    }
    Ok(())
}

fn range_contains(
    outer: PresentationRange,
    inner: PresentationRange,
    coverage_order: &impl CoverageOrderOracle,
) -> Result<bool, PresentationError> {
    Ok(
        compare_anchor(outer.start, inner.start, coverage_order)? != Ordering::Greater
            && compare_anchor(inner.end, outer.end, coverage_order)? != Ordering::Greater,
    )
}

fn ranges_intersect(
    left: PresentationRange,
    right: PresentationRange,
    coverage_order: &impl CoverageOrderOracle,
) -> Result<bool, PresentationError> {
    if left.start == left.end {
        return range_contains(right, left, coverage_order);
    }
    if right.start == right.end {
        return range_contains(left, right, coverage_order);
    }
    Ok(
        compare_anchor(left.start, right.end, coverage_order)? == Ordering::Less
            && compare_anchor(right.start, left.end, coverage_order)? == Ordering::Less,
    )
}

fn compare_anchor(
    left: ForestAnchor,
    right: ForestAnchor,
    coverage_order: &impl CoverageOrderOracle,
) -> Result<Ordering, PresentationError> {
    if left.coverage != right.coverage {
        return Ok(coverage_order.compare(left.coverage, right.coverage)?);
    }
    let bytes = left.local_bytes.cmp(&right.local_bytes);
    let utf16 = left.local_utf16.cmp(&right.local_utf16);
    if bytes != utf16 {
        return Err(PresentationError::Invalid(
            "byte and UTF-16 anchor order disagree",
        ));
    }
    Ok(bytes)
}

fn encode_range(range: PresentationRange, output: &mut Vec<u8>) {
    encode_anchor(range.start, output);
    encode_anchor(range.end, output);
}

fn encode_anchor(anchor: ForestAnchor, output: &mut Vec<u8>) {
    push_u64(output, anchor.coverage.0);
    push_u32(output, anchor.local_bytes);
    push_u32(output, anchor.local_utf16);
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], PresentationError> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(PresentationError::Overflow("decode offset"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(PresentationError::Corrupt("truncated packed value"))?;
        self.offset = end;
        bytes
            .try_into()
            .map_err(|_| PresentationError::Corrupt("wrong packed width"))
    }

    fn u8(&mut self) -> Result<u8, PresentationError> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, PresentationError> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32, PresentationError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, PresentationError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    fn anchor(&mut self) -> Result<ForestAnchor, PresentationError> {
        Ok(ForestAnchor {
            coverage: crate::ForestCoverageId(self.u64()?),
            local_bytes: self.u32()?,
            local_utf16: self.u32()?,
        })
    }

    fn range(&mut self) -> Result<PresentationRange, PresentationError> {
        Ok(PresentationRange {
            start: self.anchor()?,
            end: self.anchor()?,
        })
    }

    const fn finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

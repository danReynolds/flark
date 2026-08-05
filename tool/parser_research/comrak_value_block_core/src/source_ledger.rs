//! Streaming, source-authoritative validation for one physical-line ledger.
//!
//! This module freezes the source-ledger contract before the block parser is
//! instrumented. It intentionally does not retain a vector of claims: each
//! claim is validated and returned to the caller immediately, while the ledger
//! retains only fixed-size cursors, metrics, and a deterministic digest.
//!
//! [`RevisionAuthority::lease_line_with_metric`] is a Stage 0 UTF-8 oracle: it
//! deliberately rescans the borrowed line to challenge a supplied metric. It
//! is **not** the giant-line Crop adapter. Stage 1 must mint the same private
//! lease identity from a source capability whose metrics are already certified
//! incrementally (or validate them under fuel), without materializing or
//! scanning the physical line twice. [`RevisionAuthority::lease_refillable_line`]
//! is the O(1) descriptor half of that boundary; the refillable recognizer
//! validates its claimed metric while pulling bounded windows. Stage 1 must
//! likewise turn the parser's
//! surviving open ancestry into the authority used to resolve a pending gap;
//! this validator only proves that resolution is explicit and snapshot-bound.
//! A caller must consume/coalesce each resolved or pending line as it advances;
//! retaining one [`PendingLineLedger`] per blank line would not satisfy the
//! Stage 1 bounded-restart contract.
//!
//! [`SourceRootAuthority`] and its private snapshot nonce are proof-harness
//! stand-ins, not a proposed second production identity vocabulary. The
//! `CandidateWriter` port must scope these same checks with the existing exact
//! source descriptor plus `LiveCandidateEpoch`/`ArenaBuildId`. Persisting a
//! parallel ledger root or nonce would be an architecture error.

use std::fmt;
use std::num::NonZeroU64;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

/// Version of the physical-part/logical-action contract frozen by Stage 0.
pub const SOURCE_LEDGER_SCHEMA_VERSION: u16 = 1;

static NEXT_ROOT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SNAPSHOT_NONCE: AtomicU64 = AtomicU64::new(1);

fn fresh_nonzero(counter: &AtomicU64, what: &'static str) -> NonZeroU64 {
    let value = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("{what} authority exhausted"));
    NonZeroU64::new(value).unwrap_or_else(|| panic!("{what} authority wrapped to zero"))
}

/// Stable source revision. Revision zero is valid for an initial document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRevision(pub u64);

/// Exact physical metric supplied by the source capability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceMetric {
    pub bytes: u64,
    pub utf16: u64,
}

impl SourceMetric {
    /// Derives exact byte and UTF-16 units from UTF-8 source.
    ///
    /// # Panics
    ///
    /// Panics only on a platform whose addressable string length cannot fit in
    /// `u64`, which is not a supported Rust target today.
    #[must_use]
    pub fn for_utf8(text: &str) -> Self {
        Self {
            bytes: u64::try_from(text.len()).expect("source length fits u64"),
            utf16: u64::try_from(text.encode_utf16().count()).expect("UTF-16 length fits u64"),
        }
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_add(other.bytes)?,
            utf16: self.utf16.checked_add(other.utf16)?,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SourceVersionIdentity {
    root: NonZeroU64,
    revision: SourceRevision,
    snapshot_nonce: NonZeroU64,
}

/// Non-cloneable authority used to mint revision-bound source snapshots.
pub struct SourceRootAuthority {
    id: NonZeroU64,
}

impl SourceRootAuthority {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: fresh_nonzero(&NEXT_ROOT_ID, "source root"),
        }
    }

    /// Creates an isolated authority for one parser job over one revision.
    ///
    /// The private snapshot nonce prevents two independently minted jobs at
    /// the same root/revision from accidentally accepting each other's local
    /// binding or line identities.
    #[must_use]
    pub fn begin_revision(&self, revision: SourceRevision) -> RevisionAuthority {
        RevisionAuthority {
            identity: SourceVersionIdentity {
                root: self.id,
                revision,
                snapshot_nonce: fresh_nonzero(&NEXT_SNAPSHOT_NONCE, "source snapshot"),
            },
            next_binding: 1,
        }
    }
}

impl Default for SourceRootAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for SourceRootAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SourceRootAuthority(<opaque>)")
    }
}

/// One non-cloneable source snapshot/build authority.
pub struct RevisionAuthority {
    identity: SourceVersionIdentity,
    next_binding: u32,
}

impl RevisionAuthority {
    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.identity.revision
    }

    /// Mints a stable semantic binding scoped to this snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown semantic kind or exhausted local IDs.
    pub fn open_binding(&mut self, kind: SemanticKind) -> Result<OpenBinding, AuthorityError> {
        if !kind.is_known() {
            return Err(AuthorityError::UnknownSemanticKind(kind.0));
        }
        let local_id = self.next_binding;
        self.next_binding = self
            .next_binding
            .checked_add(1)
            .ok_or(AuthorityError::BindingIdsExhausted)?;
        Ok(OpenBinding {
            version: self.identity,
            local_id,
            kind,
        })
    }

    /// Leases one exact UTF-8 physical line using its derived source metric.
    ///
    /// # Errors
    ///
    /// Returns an error if `text` contains multiple physical lines or its
    /// absolute source range overflows.
    pub fn lease_line<'source>(
        &self,
        line_ordinal: u64,
        absolute_start: u64,
        text: &'source str,
    ) -> Result<SourceLineLease<'source>, SourceLineError> {
        self.lease_line_with_metric(
            line_ordinal,
            absolute_start,
            text,
            SourceMetric::for_utf8(text),
        )
    }

    /// Leases a line while checking a source-backend-provided metric.
    ///
    /// Stage 1's Crop adapter can call this boundary with its authoritative
    /// byte/UTF-16 summary. A mismatch fails before parser claims are admitted.
    ///
    /// # Errors
    ///
    /// Returns an error for a metric mismatch, multiple physical lines, or an
    /// overflowing absolute source range.
    pub fn lease_line_with_metric<'source>(
        &self,
        line_ordinal: u64,
        absolute_start: u64,
        text: &'source str,
        source_metric: SourceMetric,
    ) -> Result<SourceLineLease<'source>, SourceLineError> {
        let derived = SourceMetric::for_utf8(text);
        if source_metric != derived {
            return Err(SourceLineError::MetricMismatch {
                source: source_metric,
                derived,
            });
        }
        let ending = LineEnding::classify(text)?;
        absolute_start
            .checked_add(derived.bytes)
            .ok_or(SourceLineError::AbsoluteRangeOverflow)?;
        Ok(SourceLineLease {
            identity: SourceLineIdentity {
                version: self.identity,
                line_ordinal,
                absolute_start,
                bytes: derived.bytes,
            },
            text,
            metric: source_metric,
            ending,
        })
    }

    /// Mints an O(1), revision-bound descriptor for a refillable physical line.
    ///
    /// Unlike [`Self::lease_line_with_metric`], this boundary neither borrows
    /// nor scans the source bytes. The source adapter is responsible for
    /// supplying windows for the exact returned key; the refillable recognizer
    /// derives UTF-8/UTF-16 and line-ending facts under fuel and rejects a
    /// mismatch before emitting a completed receipt.
    ///
    /// # Errors
    ///
    /// Returns an error if the absolute source range overflows.
    pub fn lease_refillable_line(
        &self,
        line_ordinal: u64,
        absolute_start: u64,
        source_metric: SourceMetric,
    ) -> Result<RefillableSourceLine, SourceLineError> {
        absolute_start
            .checked_add(source_metric.bytes)
            .ok_or(SourceLineError::AbsoluteRangeOverflow)?;
        Ok(RefillableSourceLine {
            identity: SourceLineIdentity {
                version: self.identity,
                line_ordinal,
                absolute_start,
                bytes: source_metric.bytes,
            },
            metric: source_metric,
        })
    }
}

impl fmt::Debug for RevisionAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RevisionAuthority")
            .field("revision", &self.identity.revision)
            .field("snapshot", &"<opaque>")
            .field("next_binding", &self.next_binding)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityError {
    UnknownSemanticKind(u16),
    BindingIdsExhausted,
}

/// Codec-stable semantic kind used by an open binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticKind(u16);

impl SemanticKind {
    pub const DOCUMENT: Self = Self(1);
    pub const BLOCK_QUOTE: Self = Self(2);
    pub const LIST: Self = Self(3);
    pub const ITEM: Self = Self(4);
    pub const PARAGRAPH: Self = Self(5);
    pub const INDENTED_CODE: Self = Self(6);
    pub const FENCED_CODE: Self = Self(7);
    pub const HTML_BLOCK: Self = Self(8);
    pub const TABLE: Self = Self(9);
    pub const TABLE_ROW: Self = Self(10);
    pub const TABLE_CELL: Self = Self(11);
    pub const HEADING: Self = Self(12);
    pub const THEMATIC_BREAK: Self = Self(13);

    const fn is_known(self) -> bool {
        self.0 >= Self::DOCUMENT.0 && self.0 <= Self::THEMATIC_BREAK.0
    }

    const fn logical_channel(self) -> Option<LogicalChannel> {
        if matches!(self, Self::PARAGRAPH | Self::TABLE_CELL | Self::HEADING) {
            Some(LogicalChannel::Inline)
        } else if matches!(
            self,
            Self::INDENTED_CODE | Self::FENCED_CODE | Self::HTML_BLOCK
        ) {
            Some(LogicalChannel::Literal)
        } else {
            None
        }
    }

    const fn golden_name(self) -> &'static str {
        match self {
            Self::DOCUMENT => "document",
            Self::BLOCK_QUOTE => "block-quote",
            Self::LIST => "list",
            Self::ITEM => "item",
            Self::PARAGRAPH => "paragraph",
            Self::INDENTED_CODE => "indented-code",
            Self::FENCED_CODE => "fenced-code",
            Self::HTML_BLOCK => "html-block",
            Self::TABLE => "table",
            Self::TABLE_ROW => "table-row",
            Self::TABLE_CELL => "table-cell",
            Self::HEADING => "heading",
            Self::THEMATIC_BREAK => "thematic-break",
            _ => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalChannel {
    Inline,
    Literal,
}

/// Stable, snapshot-scoped semantic owner. Its identity fields are private.
pub struct OpenBinding {
    version: SourceVersionIdentity,
    local_id: u32,
    kind: SemanticKind,
}

impl OpenBinding {
    #[must_use]
    pub const fn kind(&self) -> SemanticKind {
        self.kind
    }

    #[must_use]
    pub const fn logical_channel(&self) -> Option<LogicalChannel> {
        self.kind.logical_channel()
    }
}

impl fmt::Debug for OpenBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "OpenBinding({}#{}, revision={})",
            self.kind.golden_name(),
            self.local_id,
            self.version.revision.0
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BindingRef {
    version: SourceVersionIdentity,
    local_id: u32,
    kind: SemanticKind,
}

impl From<&OpenBinding> for BindingRef {
    fn from(binding: &OpenBinding) -> Self {
        Self {
            version: binding.version,
            local_id: binding.local_id,
            kind: binding.kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    None,
    Lf,
    Cr,
    CrLf,
}

impl LineEnding {
    fn classify(text: &str) -> Result<Self, SourceLineError> {
        let (body, ending) = if let Some(body) = text.strip_suffix("\r\n") {
            (body, Self::CrLf)
        } else if let Some(body) = text.strip_suffix('\n') {
            (body, Self::Lf)
        } else if let Some(body) = text.strip_suffix('\r') {
            (body, Self::Cr)
        } else {
            (text, Self::None)
        };
        if body.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
            Err(SourceLineError::MultiplePhysicalLines)
        } else {
            Ok(ending)
        }
    }

    const fn bytes(self) -> u64 {
        match self {
            Self::None => 0,
            Self::Lf | Self::Cr => 1,
            Self::CrLf => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SourceLineIdentity {
    version: SourceVersionIdentity,
    line_ordinal: u64,
    absolute_start: u64,
    bytes: u64,
}

/// Opaque identity a trusted source adapter returns with every bounded read.
///
/// Copying this key copies only fixed-size provenance metadata, never source
/// bytes. Its fields stay private so an adapter cannot synthesize a different
/// root, revision, snapshot, or line range.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RefillableSourceLineKey(SourceLineIdentity);

impl fmt::Debug for RefillableSourceLineKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RefillableSourceLineKey")
            .field("revision", &self.0.version.revision)
            .field("line_ordinal", &self.0.line_ordinal)
            .field("absolute_start", &self.0.absolute_start)
            .field("bytes", &self.0.bytes)
            .finish_non_exhaustive()
    }
}

/// Fixed-size source descriptor consumed by a refillable line recognizer.
///
/// This value deliberately owns no source text. The metric is challenged by
/// the streamed UTF-8 fold before the line can complete.
pub struct RefillableSourceLine {
    identity: SourceLineIdentity,
    metric: SourceMetric,
}

impl RefillableSourceLine {
    #[must_use]
    pub const fn key(&self) -> RefillableSourceLineKey {
        RefillableSourceLineKey(self.identity)
    }

    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.identity.version.revision
    }

    #[must_use]
    pub const fn line_ordinal(&self) -> u64 {
        self.identity.line_ordinal
    }

    #[must_use]
    pub const fn absolute_start(&self) -> u64 {
        self.identity.absolute_start
    }

    #[must_use]
    pub const fn metric(&self) -> SourceMetric {
        self.metric
    }
}

impl fmt::Debug for RefillableSourceLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RefillableSourceLine")
            .field("revision", &self.revision())
            .field("line_ordinal", &self.line_ordinal())
            .field("absolute_start", &self.absolute_start())
            .field("metric", &self.metric)
            .finish_non_exhaustive()
    }
}

/// Borrowed, revision-bound source line. Span capabilities can only be minted
/// from this lease.
pub struct SourceLineLease<'source> {
    identity: SourceLineIdentity,
    text: &'source str,
    metric: SourceMetric,
    ending: LineEnding,
}

impl SourceLineLease<'_> {
    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.identity.version.revision
    }

    #[must_use]
    pub const fn line_ordinal(&self) -> u64 {
        self.identity.line_ordinal
    }

    #[must_use]
    pub const fn absolute_start(&self) -> u64 {
        self.identity.absolute_start
    }

    #[must_use]
    pub const fn metric(&self) -> SourceMetric {
        self.metric
    }

    #[must_use]
    pub const fn line_ending(&self) -> LineEnding {
        self.ending
    }

    #[must_use]
    pub const fn text(&self) -> &str {
        self.text
    }

    /// Mints an exact, non-cloneable source range capability.
    ///
    /// # Errors
    ///
    /// Returns an error when the range is out of bounds or cuts through a
    /// UTF-8 code point.
    pub fn span(&self, range: Range<usize>) -> Result<SourceSpanCapability<'_>, SourceSpanError> {
        if range.start > range.end || range.end > self.text.len() {
            return Err(SourceSpanError::OutOfBounds {
                line_bytes: self.text.len(),
                range,
            });
        }
        let Some(text) = self.text.get(range.clone()) else {
            return Err(SourceSpanError::NotUtf8Boundary { range });
        };
        Ok(SourceSpanCapability {
            line: self.identity,
            range,
            metric: SourceMetric::for_utf8(text),
            text,
        })
    }
}

impl fmt::Debug for SourceLineLease<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceLineLease")
            .field("revision", &self.revision())
            .field("line_ordinal", &self.line_ordinal())
            .field("absolute_start", &self.absolute_start())
            .field("metric", &self.metric)
            .field("line_ending", &self.ending)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceLineError {
    MetricMismatch {
        source: SourceMetric,
        derived: SourceMetric,
    },
    MultiplePhysicalLines,
    AbsoluteRangeOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceSpanError {
    OutOfBounds {
        line_bytes: usize,
        range: Range<usize>,
    },
    NotUtf8Boundary {
        range: Range<usize>,
    },
}

/// Non-cloneable proof that a byte range came from one source-line lease.
pub struct SourceSpanCapability<'line> {
    line: SourceLineIdentity,
    range: Range<usize>,
    metric: SourceMetric,
    text: &'line str,
}

impl SourceSpanCapability<'_> {
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    #[must_use]
    pub const fn metric(&self) -> SourceMetric {
        self.metric
    }
}

impl fmt::Debug for SourceSpanCapability<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceSpanCapability")
            .field("revision", &self.line.version.revision)
            .field("line_ordinal", &self.line.line_ordinal)
            .field("range", &self.range)
            .field("metric", &self.metric)
            .finish()
    }
}

/// Orthogonal physical ownership classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoveragePart {
    Content,
    ContainerMarker,
    BlockMarker,
    Gap,
    Terminal,
}

impl CoveragePart {
    const fn tag(self) -> u8 {
        match self {
            Self::Content => 1,
            Self::ContainerMarker => 2,
            Self::BlockMarker => 3,
            Self::Gap => 4,
            Self::Terminal => 5,
        }
    }

    const fn golden_name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::ContainerMarker => "container-marker",
            Self::BlockMarker => "block-marker",
            Self::Gap => "gap",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryAffinity {
    Upstream,
    Downstream,
}

impl BoundaryAffinity {
    const fn tag(self) -> u8 {
        match self {
            Self::Upstream => 1,
            Self::Downstream => 2,
        }
    }

    const fn golden_name(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::Downstream => "downstream",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicProjection {
    TabToSpaces { spaces: u8 },
    CrLfToLf,
    LoneCrToLf,
    NulToReplacement,
}

impl AtomicProjection {
    fn validate_recipe(self) -> Result<(), LogicalActionError> {
        if let Self::TabToSpaces { spaces } = self
            && !(1..=4).contains(&spaces)
        {
            return Err(LogicalActionError::InvalidTabExpansion(spaces));
        }
        Ok(())
    }

    const fn tag(self) -> u8 {
        match self {
            Self::TabToSpaces { .. } => 1,
            Self::CrLfToLf => 2,
            Self::LoneCrToLf => 3,
            Self::NulToReplacement => 4,
        }
    }
}

/// Bounded, typed program recipes required by the Stage 0 policy goldens.
/// Dense table programs become separately certified capabilities in Stage 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionProgramRecipe {
    Hidden { affinity: BoundaryAffinity },
    TrimAndUnescapePipes,
    EntityAndBackslashNormalization,
}

impl ProjectionProgramRecipe {
    const fn tag(self) -> u8 {
        match self {
            Self::Hidden { .. } => 1,
            Self::TrimAndUnescapePipes => 2,
            Self::EntityAndBackslashNormalization => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogicalAction {
    None,
    Identity {
        target: LogicalTarget,
    },
    Atomic {
        target: LogicalTarget,
        projection: AtomicProjection,
    },
    Program {
        target: LogicalTarget,
        recipe: ProjectionProgramRecipe,
    },
}

impl LogicalAction {
    #[must_use]
    pub const fn none() -> Self {
        Self::None
    }

    /// Creates an identity contribution to a logical terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when `target` is not a logical terminal.
    pub fn identity(target: &OpenBinding) -> Result<Self, LogicalActionError> {
        Ok(Self::Identity {
            target: LogicalTarget::new(target)?,
        })
    }

    /// Creates a typed atomic contribution to a logical terminal.
    ///
    /// # Errors
    ///
    /// Returns an error for a nonterminal target or invalid typed recipe.
    pub fn atomic(
        target: &OpenBinding,
        projection: AtomicProjection,
    ) -> Result<Self, LogicalActionError> {
        projection.validate_recipe()?;
        Ok(Self::Atomic {
            target: LogicalTarget::new(target)?,
            projection,
        })
    }

    /// Creates a bounded typed program contribution to a logical terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when `target` is not a logical terminal.
    pub fn program(
        target: &OpenBinding,
        recipe: ProjectionProgramRecipe,
    ) -> Result<Self, LogicalActionError> {
        Ok(Self::Program {
            target: LogicalTarget::new(target)?,
            recipe,
        })
    }

    const fn target(self) -> Option<LogicalTarget> {
        match self {
            Self::None => None,
            Self::Identity { target }
            | Self::Atomic { target, .. }
            | Self::Program { target, .. } => Some(target),
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Identity { .. } => 1,
            Self::Atomic { .. } => 2,
            Self::Program { .. } => 3,
        }
    }
}

impl fmt::Debug for LogicalAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&GoldenLogicalAction(*self), formatter)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LogicalTarget(BindingRef);

impl LogicalTarget {
    fn new(binding: &OpenBinding) -> Result<Self, LogicalActionError> {
        if binding.logical_channel().is_none() {
            Err(LogicalActionError::TargetIsNotTerminal(binding.kind))
        } else {
            Ok(Self(BindingRef::from(binding)))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalActionError {
    TargetIsNotTerminal(SemanticKind),
    InvalidTabExpansion(u8),
}

/// One explicit source claim. All identity-bearing fields are privately
/// constructed; validation still checks that their scopes agree.
pub struct SourceClaim<'line> {
    source: SourceSpanCapability<'line>,
    physical_owner: BindingRef,
    part: CoveragePart,
    logical: LogicalAction,
    affinity: BoundaryAffinity,
}

impl<'line> SourceClaim<'line> {
    #[must_use]
    pub fn new(
        source: SourceSpanCapability<'line>,
        physical_owner: &OpenBinding,
        part: CoveragePart,
        logical: LogicalAction,
        affinity: BoundaryAffinity,
    ) -> Self {
        Self {
            source,
            physical_owner: BindingRef::from(physical_owner),
            part,
            logical,
            affinity,
        }
    }
}

/// Which authority failed a source-version check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoritySubject {
    SourceSpan,
    PhysicalOwner,
    LogicalTarget,
    PendingOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerError {
    Poisoned,
    WrongSourceRoot {
        subject: AuthoritySubject,
    },
    WrongRevision {
        subject: AuthoritySubject,
        expected: SourceRevision,
        actual: SourceRevision,
    },
    WrongSnapshot {
        subject: AuthoritySubject,
    },
    WrongSourceLine,
    EmptyClaim,
    Overlap {
        claimed_start: u64,
        next_unclaimed: u64,
    },
    GapBeforeClaim {
        claimed_start: u64,
        next_unclaimed: u64,
    },
    MetricOverflow,
    InvalidAtomicPhysicalInput(AtomicProjection),
    IncompleteCoverage {
        next_unclaimed: u64,
        line_bytes: u64,
    },
    PhysicalMetricMismatch {
        claimed: SourceMetric,
        source: SourceMetric,
    },
    PendingMustCoverTail,
    PendingAlreadyStaged,
    PendingGapMustBeBlank,
    PendingOwnerIsNotTerminal(SemanticKind),
    PendingTerminatorMustBeExactLineEnding,
    NoPhysicalLineEnding,
}

#[derive(Clone, Copy)]
struct LedgerCore {
    line: SourceLineIdentity,
    source_metric: SourceMetric,
    ending: LineEnding,
    next_unclaimed: u64,
    claimed_metric: SourceMetric,
    claim_count: u64,
    claim_digest: u64,
    poisoned: bool,
}

impl LedgerCore {
    fn new(line: &SourceLineLease<'_>) -> Self {
        Self {
            line: line.identity,
            source_metric: line.metric,
            ending: line.ending,
            next_unclaimed: 0,
            claimed_metric: SourceMetric::default(),
            claim_count: 0,
            claim_digest: FNV_OFFSET_BASIS,
            poisoned: false,
        }
    }

    fn validate_version(
        &self,
        actual: SourceVersionIdentity,
        subject: AuthoritySubject,
    ) -> Result<(), LedgerError> {
        let expected = self.line.version;
        if actual.root != expected.root {
            return Err(LedgerError::WrongSourceRoot { subject });
        }
        if actual.revision != expected.revision {
            return Err(LedgerError::WrongRevision {
                subject,
                expected: expected.revision,
                actual: actual.revision,
            });
        }
        if actual.snapshot_nonce != expected.snapshot_nonce {
            return Err(LedgerError::WrongSnapshot { subject });
        }
        Ok(())
    }

    fn validate_span(&self, span: &SourceSpanCapability<'_>) -> Result<(), LedgerError> {
        self.validate_version(span.line.version, AuthoritySubject::SourceSpan)?;
        if span.line != self.line {
            return Err(LedgerError::WrongSourceLine);
        }
        let start = u64::try_from(span.range.start).expect("source range fits u64");
        let end = u64::try_from(span.range.end).expect("source range fits u64");
        if start == end {
            return Err(LedgerError::EmptyClaim);
        }
        if start < self.next_unclaimed {
            return Err(LedgerError::Overlap {
                claimed_start: start,
                next_unclaimed: self.next_unclaimed,
            });
        }
        if start > self.next_unclaimed {
            return Err(LedgerError::GapBeforeClaim {
                claimed_start: start,
                next_unclaimed: self.next_unclaimed,
            });
        }
        Ok(())
    }

    fn validate_binding(
        &self,
        binding: BindingRef,
        subject: AuthoritySubject,
    ) -> Result<(), LedgerError> {
        self.validate_version(binding.version, subject)
    }

    fn validate_logical(&self, action: LogicalAction) -> Result<(), LedgerError> {
        if let Some(target) = action.target() {
            self.validate_binding(target.0, AuthoritySubject::LogicalTarget)?;
        }
        Ok(())
    }

    fn validate_atomic_input(
        span: &SourceSpanCapability<'_>,
        action: LogicalAction,
    ) -> Result<(), LedgerError> {
        let LogicalAction::Atomic { projection, .. } = action else {
            return Ok(());
        };
        let valid = match projection {
            AtomicProjection::TabToSpaces { .. } => span.text == "\t",
            AtomicProjection::CrLfToLf => span.text == "\r\n",
            AtomicProjection::LoneCrToLf => span.text == "\r",
            AtomicProjection::NulToReplacement => span.text.as_bytes() == [0],
        };
        if valid {
            Ok(())
        } else {
            Err(LedgerError::InvalidAtomicPhysicalInput(projection))
        }
    }

    fn accept(
        &mut self,
        span: &SourceSpanCapability<'_>,
        owner: BindingRef,
        part: CoveragePart,
        logical: LogicalAction,
        affinity: BoundaryAffinity,
    ) -> Result<ValidatedClaim, LedgerError> {
        self.validate_span(span)?;
        self.validate_binding(owner, AuthoritySubject::PhysicalOwner)?;
        self.validate_logical(logical)?;
        Self::validate_atomic_input(span, logical)?;

        let metric = self
            .claimed_metric
            .checked_add(span.metric)
            .ok_or(LedgerError::MetricOverflow)?;
        let source_start = u64::try_from(span.range.start).expect("source range fits u64");
        let source_end = u64::try_from(span.range.end).expect("source range fits u64");
        let claim = ValidatedClaim {
            revision: self.line.version.revision,
            line_ordinal: self.line.line_ordinal,
            absolute_start: self
                .line
                .absolute_start
                .checked_add(source_start)
                .expect("leased source line absolute range was checked"),
            source_start,
            source_end,
            metric: span.metric,
            physical_owner: owner,
            part,
            logical,
            affinity,
        };
        self.next_unclaimed = source_end;
        self.claimed_metric = metric;
        self.claim_count = self
            .claim_count
            .checked_add(1)
            .ok_or(LedgerError::MetricOverflow)?;
        self.claim_digest = claim.fold_digest(self.claim_digest);
        Ok(claim)
    }

    fn check_complete(&self) -> Result<(), LedgerError> {
        if self.poisoned {
            return Err(LedgerError::Poisoned);
        }
        if self.next_unclaimed != self.source_metric.bytes {
            return Err(LedgerError::IncompleteCoverage {
                next_unclaimed: self.next_unclaimed,
                line_bytes: self.source_metric.bytes,
            });
        }
        if self.claimed_metric != self.source_metric {
            return Err(LedgerError::PhysicalMetricMismatch {
                claimed: self.claimed_metric,
                source: self.source_metric,
            });
        }
        Ok(())
    }

    fn receipt(self) -> LineLedgerReceipt {
        LineLedgerReceipt {
            revision: self.line.version.revision,
            line_ordinal: self.line.line_ordinal,
            absolute_start: self.line.absolute_start,
            metric: self.source_metric,
            ending: self.ending,
            claim_count: self.claim_count,
            claim_digest: self.claim_digest,
        }
    }
}

/// Fixed-size streaming validator for one physical source line.
pub struct LineLedger<'line> {
    core: LedgerCore,
    pending: Option<PendingTail<'line>>,
}

impl<'line> LineLedger<'line> {
    #[must_use]
    pub fn begin(line: &'line SourceLineLease<'_>) -> Self {
        Self {
            core: LedgerCore::new(line),
            pending: None,
        }
    }

    /// Validates and advances one explicit claim. On error the ledger is
    /// poisoned, so it can never publish a partial receipt.
    ///
    /// # Errors
    ///
    /// Returns an error for stale/mismatched authority, non-ordered coverage,
    /// invalid atomic input, overflow, or an already poisoned ledger.
    pub fn claim(&mut self, claim: SourceClaim<'line>) -> Result<ValidatedClaim, LedgerError> {
        if self.core.poisoned {
            return Err(LedgerError::Poisoned);
        }
        if self.pending.is_some() {
            self.core.poisoned = true;
            return Err(LedgerError::PendingAlreadyStaged);
        }
        let SourceClaim {
            source,
            physical_owner,
            part,
            logical,
            affinity,
        } = claim;
        let result = self
            .core
            .accept(&source, physical_owner, part, logical, affinity);
        if result.is_err() {
            self.core.poisoned = true;
        }
        result
    }

    /// Stages an unresolved blank tail. No owner or logical action is guessed.
    ///
    /// # Errors
    ///
    /// Returns an error unless `source` is the next exact blank tail bound to
    /// this ledger, or if a pending tail already exists.
    pub fn stage_pending_gap(
        &mut self,
        source: SourceSpanCapability<'line>,
        affinity: BoundaryAffinity,
    ) -> Result<(), LedgerError> {
        self.stage_pending(source, PendingTailKind::Gap { affinity })
    }

    /// Stages the exact physical line ending for a known terminal. Its
    /// Content-vs-Terminal part and logical newline action remain unresolved.
    ///
    /// # Errors
    ///
    /// Returns an error unless `source` is the exact physical line ending and
    /// `terminal` is a matching logical terminal in this snapshot.
    pub fn stage_pending_terminator(
        &mut self,
        source: SourceSpanCapability<'line>,
        terminal: &OpenBinding,
        affinity: BoundaryAffinity,
    ) -> Result<(), LedgerError> {
        if self.core.ending == LineEnding::None {
            self.core.poisoned = true;
            return Err(LedgerError::NoPhysicalLineEnding);
        }
        if terminal.logical_channel().is_none() {
            self.core.poisoned = true;
            return Err(LedgerError::PendingOwnerIsNotTerminal(terminal.kind));
        }
        self.stage_pending(
            source,
            PendingTailKind::Terminator {
                terminal: BindingRef::from(terminal),
                affinity,
            },
        )
    }

    fn stage_pending(
        &mut self,
        source: SourceSpanCapability<'line>,
        kind: PendingTailKind,
    ) -> Result<(), LedgerError> {
        if self.core.poisoned {
            return Err(LedgerError::Poisoned);
        }
        if self.pending.is_some() {
            self.core.poisoned = true;
            return Err(LedgerError::PendingAlreadyStaged);
        }
        let result = (|| {
            self.core.validate_span(&source)?;
            let source_end = u64::try_from(source.range.end).expect("source range fits u64");
            if source_end != self.core.source_metric.bytes {
                return Err(LedgerError::PendingMustCoverTail);
            }
            match kind {
                PendingTailKind::Gap { .. } => {
                    if !source
                        .text
                        .bytes()
                        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
                    {
                        return Err(LedgerError::PendingGapMustBeBlank);
                    }
                }
                PendingTailKind::Terminator { terminal, .. } => {
                    self.core
                        .validate_binding(terminal, AuthoritySubject::PendingOwner)?;
                    let source_start =
                        u64::try_from(source.range.start).expect("source range fits u64");
                    if source_start != self.core.line.bytes - self.core.ending.bytes()
                        || source.metric.bytes != self.core.ending.bytes()
                    {
                        return Err(LedgerError::PendingTerminatorMustBeExactLineEnding);
                    }
                }
            }
            let next_metric = self
                .core
                .claimed_metric
                .checked_add(source.metric)
                .ok_or(LedgerError::MetricOverflow)?;
            self.core.next_unclaimed = source_end;
            self.core.claimed_metric = next_metric;
            self.pending = Some(PendingTail { source, kind });
            Ok(())
        })();
        if result.is_err() {
            self.core.poisoned = true;
        }
        result
    }

    /// Finishes physical validation without inventing any missing claim.
    ///
    /// # Errors
    ///
    /// Returns an error for poisoned, incomplete, or metric-inexact coverage.
    pub fn finish_line(self) -> Result<LineLedgerFinish<'line>, LedgerError> {
        self.core.check_complete()?;
        if let Some(pending) = self.pending {
            Ok(LineLedgerFinish::Pending(PendingLineLedger {
                core: self.core,
                pending,
            }))
        } else {
            Ok(LineLedgerFinish::Complete(self.core.receipt()))
        }
    }
}

#[derive(Clone, Copy)]
enum PendingTailKind {
    Gap {
        affinity: BoundaryAffinity,
    },
    Terminator {
        terminal: BindingRef,
        affinity: BoundaryAffinity,
    },
}

struct PendingTail<'line> {
    source: SourceSpanCapability<'line>,
    kind: PendingTailKind,
}

pub enum LineLedgerFinish<'line> {
    Complete(LineLedgerReceipt),
    Pending(PendingLineLedger<'line>),
}

impl fmt::Debug for LineLedgerFinish<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete(receipt) => formatter.debug_tuple("Complete").field(receipt).finish(),
            Self::Pending(pending) => formatter.debug_tuple("Pending").field(pending).finish(),
        }
    }
}

impl LineLedgerFinish<'_> {
    #[must_use]
    pub const fn pending_kind(&self) -> Option<PendingKind> {
        match self {
            Self::Complete(_) => None,
            Self::Pending(pending) => Some(pending.kind()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingKind {
    Gap,
    Terminator,
}

/// Finished physical coverage whose semantic tail is still future-dependent.
/// This value is non-cloneable and must be consumed by a typed resolution.
pub struct PendingLineLedger<'line> {
    core: LedgerCore,
    pending: PendingTail<'line>,
}

impl fmt::Debug for PendingLineLedger<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingLineLedger")
            .field("revision", &self.core.line.version.revision)
            .field("line_ordinal", &self.core.line.line_ordinal)
            .field("metric", &self.core.source_metric)
            .field("kind", &self.kind())
            .finish()
    }
}

impl<'line> PendingLineLedger<'line> {
    #[must_use]
    pub const fn kind(&self) -> PendingKind {
        match self.pending.kind {
            PendingTailKind::Gap { .. } => PendingKind::Gap,
            PendingTailKind::Terminator { .. } => PendingKind::Terminator,
        }
    }

    /// Resolves a blank tail to an explicitly selected surviving owner.
    ///
    /// # Errors
    ///
    /// Returns the still-owned pending authority on kind or source-snapshot
    /// mismatch, allowing the caller to retry with a certified owner.
    pub fn resolve_gap(
        mut self,
        owner: &OpenBinding,
    ) -> Result<ResolvedPendingLine, PendingResolutionError<'line>> {
        let PendingTailKind::Gap { affinity } = self.pending.kind else {
            return Err(PendingResolutionError {
                error: PendingResolutionFailure::WrongPendingKind {
                    expected: PendingKind::Gap,
                    actual: self.kind(),
                },
                pending: Box::new(self),
            });
        };
        if let Err(error) = self
            .core
            .validate_binding(BindingRef::from(owner), AuthoritySubject::PendingOwner)
        {
            return Err(PendingResolutionError {
                error: PendingResolutionFailure::Ledger(error),
                pending: Box::new(self),
            });
        }
        let claim = self.finish_pending_claim(
            BindingRef::from(owner),
            CoveragePart::Gap,
            LogicalAction::None,
            affinity,
        );
        Ok(claim)
    }

    /// Resolves a staged terminator without deriving policy from its part.
    ///
    /// # Errors
    ///
    /// Returns the still-owned pending authority when this is a gap rather
    /// than a terminator.
    pub fn resolve_terminator(
        mut self,
        resolution: TerminatorResolution,
    ) -> Result<ResolvedPendingLine, PendingResolutionError<'line>> {
        let PendingTailKind::Terminator { terminal, affinity } = self.pending.kind else {
            return Err(PendingResolutionError {
                error: PendingResolutionFailure::WrongPendingKind {
                    expected: PendingKind::Terminator,
                    actual: self.kind(),
                },
                pending: Box::new(self),
            });
        };
        let (part, logical) = match resolution {
            TerminatorResolution::ContinueCanonicalNewline => (
                CoveragePart::Content,
                canonical_newline_action(terminal, self.core.ending),
            ),
            TerminatorResolution::CloseNone => (CoveragePart::Terminal, LogicalAction::None),
            TerminatorResolution::CloseCanonicalNewline => (
                CoveragePart::Terminal,
                canonical_newline_action(terminal, self.core.ending),
            ),
        };
        Ok(self.finish_pending_claim(terminal, part, logical, affinity))
    }

    fn finish_pending_claim(
        &mut self,
        owner: BindingRef,
        part: CoveragePart,
        logical: LogicalAction,
        affinity: BoundaryAffinity,
    ) -> ResolvedPendingLine {
        let source_start =
            u64::try_from(self.pending.source.range.start).expect("source range fits u64");
        let source_end =
            u64::try_from(self.pending.source.range.end).expect("source range fits u64");
        let claim = ValidatedClaim {
            revision: self.core.line.version.revision,
            line_ordinal: self.core.line.line_ordinal,
            absolute_start: self
                .core
                .line
                .absolute_start
                .checked_add(source_start)
                .expect("leased source line absolute range was checked"),
            source_start,
            source_end,
            metric: self.pending.source.metric,
            physical_owner: owner,
            part,
            logical,
            affinity,
        };
        self.core.claim_count = self
            .core
            .claim_count
            .checked_add(1)
            .expect("claim count did not overflow during prior validation");
        self.core.claim_digest = claim.fold_digest(self.core.claim_digest);
        ResolvedPendingLine {
            receipt: self.core.receipt(),
            claim,
        }
    }
}

fn canonical_newline_action(terminal: BindingRef, ending: LineEnding) -> LogicalAction {
    let target = LogicalTarget(terminal);
    match ending {
        LineEnding::Lf => LogicalAction::Identity { target },
        LineEnding::Cr => LogicalAction::Atomic {
            target,
            projection: AtomicProjection::LoneCrToLf,
        },
        LineEnding::CrLf => LogicalAction::Atomic {
            target,
            projection: AtomicProjection::CrLfToLf,
        },
        LineEnding::None => unreachable!("pending terminators require a physical line ending"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminatorResolution {
    ContinueCanonicalNewline,
    CloseNone,
    CloseCanonicalNewline,
}

pub struct PendingResolutionError<'line> {
    error: PendingResolutionFailure,
    pending: Box<PendingLineLedger<'line>>,
}

impl<'line> PendingResolutionError<'line> {
    #[must_use]
    pub const fn error(&self) -> PendingResolutionFailure {
        self.error
    }

    #[must_use]
    pub fn into_pending(self) -> PendingLineLedger<'line> {
        *self.pending
    }
}

impl fmt::Debug for PendingResolutionError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingResolutionError")
            .field("error", &self.error)
            .field("pending_kind", &self.pending.kind())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingResolutionFailure {
    WrongPendingKind {
        expected: PendingKind,
        actual: PendingKind,
    },
    Ledger(LedgerError),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ValidatedClaim {
    revision: SourceRevision,
    line_ordinal: u64,
    absolute_start: u64,
    source_start: u64,
    source_end: u64,
    metric: SourceMetric,
    physical_owner: BindingRef,
    part: CoveragePart,
    logical: LogicalAction,
    affinity: BoundaryAffinity,
}

impl ValidatedClaim {
    #[must_use]
    pub const fn metric(self) -> SourceMetric {
        self.metric
    }

    #[must_use]
    pub const fn part(self) -> CoveragePart {
        self.part
    }

    #[must_use]
    pub const fn logical(self) -> LogicalAction {
        self.logical
    }

    #[must_use]
    pub const fn golden_debug(&self) -> GoldenClaimDebug<'_> {
        GoldenClaimDebug(self)
    }

    fn fold_digest(self, mut digest: u64) -> u64 {
        fold_u64(&mut digest, self.source_start);
        fold_u64(&mut digest, self.source_end);
        fold_u64(&mut digest, self.metric.bytes);
        fold_u64(&mut digest, self.metric.utf16);
        fold_u32(&mut digest, self.physical_owner.local_id);
        fold_u16(&mut digest, self.physical_owner.kind.0);
        fold_byte(&mut digest, self.part.tag());
        fold_byte(&mut digest, self.logical.tag());
        if let Some(target) = self.logical.target() {
            fold_u32(&mut digest, target.0.local_id);
            fold_u16(&mut digest, target.0.kind.0);
        }
        match self.logical {
            LogicalAction::Atomic { projection, .. } => {
                fold_byte(&mut digest, projection.tag());
                if let AtomicProjection::TabToSpaces { spaces } = projection {
                    fold_byte(&mut digest, spaces);
                }
            }
            LogicalAction::Program { recipe, .. } => {
                fold_byte(&mut digest, recipe.tag());
                if let ProjectionProgramRecipe::Hidden { affinity } = recipe {
                    fold_byte(&mut digest, affinity.tag());
                }
            }
            LogicalAction::None | LogicalAction::Identity { .. } => {}
        }
        fold_byte(&mut digest, self.affinity.tag());
        digest
    }
}

impl fmt::Debug for ValidatedClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.golden_debug().fmt(formatter)
    }
}

pub struct GoldenClaimDebug<'claim>(&'claim ValidatedClaim);

impl fmt::Display for GoldenClaimDebug<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let claim = self.0;
        write!(
            formatter,
            "rev={} line={} rel={}..{} abs={}..{} metric={}b/{}u16 owner={}#{} part={} logical=",
            claim.revision.0,
            claim.line_ordinal,
            claim.source_start,
            claim.source_end,
            claim.absolute_start,
            claim.absolute_start + claim.metric.bytes,
            claim.metric.bytes,
            claim.metric.utf16,
            claim.physical_owner.kind.golden_name(),
            claim.physical_owner.local_id,
            claim.part.golden_name(),
        )?;
        fmt::Debug::fmt(&GoldenLogicalAction(claim.logical), formatter)?;
        write!(formatter, " affinity={}", claim.affinity.golden_name())
    }
}

impl fmt::Debug for GoldenClaimDebug<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

struct GoldenLogicalAction(LogicalAction);

impl fmt::Debug for GoldenLogicalAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            LogicalAction::None => formatter.write_str("none"),
            LogicalAction::Identity { target } => write!(
                formatter,
                "identity->{}#{}",
                target.0.kind.golden_name(),
                target.0.local_id
            ),
            LogicalAction::Atomic { target, projection } => {
                formatter.write_str("atomic(")?;
                match projection {
                    AtomicProjection::TabToSpaces { spaces } => {
                        write!(formatter, "tab-to-{spaces}-spaces")?;
                    }
                    AtomicProjection::CrLfToLf => formatter.write_str("crlf-to-lf")?,
                    AtomicProjection::LoneCrToLf => formatter.write_str("cr-to-lf")?,
                    AtomicProjection::NulToReplacement => {
                        formatter.write_str("nul-to-replacement")?;
                    }
                }
                write!(
                    formatter,
                    ")->{}#{}",
                    target.0.kind.golden_name(),
                    target.0.local_id
                )
            }
            LogicalAction::Program { target, recipe } => {
                formatter.write_str("program(")?;
                match recipe {
                    ProjectionProgramRecipe::Hidden { affinity } => {
                        write!(formatter, "hidden:{}", affinity.golden_name())?;
                    }
                    ProjectionProgramRecipe::TrimAndUnescapePipes => {
                        formatter.write_str("trim-and-unescape-pipes")?;
                    }
                    ProjectionProgramRecipe::EntityAndBackslashNormalization => {
                        formatter.write_str("entity-and-backslash-normalization")?;
                    }
                }
                write!(
                    formatter,
                    ")->{}#{}",
                    target.0.kind.golden_name(),
                    target.0.local_id
                )
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LineLedgerReceipt {
    revision: SourceRevision,
    line_ordinal: u64,
    absolute_start: u64,
    metric: SourceMetric,
    ending: LineEnding,
    claim_count: u64,
    claim_digest: u64,
}

impl LineLedgerReceipt {
    #[must_use]
    pub const fn revision(self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub const fn metric(self) -> SourceMetric {
        self.metric
    }

    #[must_use]
    pub const fn claim_count(self) -> u64 {
        self.claim_count
    }

    /// Deterministic golden/debug fingerprint of the accepted claim sequence.
    ///
    /// This non-cryptographic value is never source or adoption authority.
    #[must_use]
    pub const fn claim_digest(self) -> u64 {
        self.claim_digest
    }
}

impl fmt::Debug for LineLedgerReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LineLedgerReceipt")
            .field("schema_version", &SOURCE_LEDGER_SCHEMA_VERSION)
            .field("revision", &self.revision.0)
            .field("line_ordinal", &self.line_ordinal)
            .field("absolute_start", &self.absolute_start)
            .field("metric", &self.metric)
            .field("line_ending", &self.ending)
            .field("claim_count", &self.claim_count)
            .field("claim_digest", &format_args!("{:#018x}", self.claim_digest))
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPendingLine {
    pub receipt: LineLedgerReceipt,
    pub claim: ValidatedClaim,
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fold_byte(digest: &mut u64, byte: u8) {
    *digest ^= u64::from(byte);
    *digest = digest.wrapping_mul(FNV_PRIME);
}

fn fold_u16(digest: &mut u64, value: u16) {
    for byte in value.to_le_bytes() {
        fold_byte(digest, byte);
    }
}

fn fold_u32(digest: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        fold_byte(digest, byte);
    }
}

fn fold_u64(digest: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        fold_byte(digest, byte);
    }
}

use std::cell::Cell;
use std::collections::{TryReserveError, VecDeque};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;

use crate::identity::{RuntimeIdentity, RuntimeIdentityError, SourceRevision};
#[cfg(feature = "progressive-source-probe")]
use crate::source::{
    OpeningSourceAppendProof, OpeningSourceError, OpeningSourceSnapshot, SourceAppendReceipt,
};
use crate::source::{
    SourceEditError, SourceEditIntentReceipt, SourceEditLineage, SourceEditReceipt,
    SourceSnapshotLease, SourceStore, SourceUtf16Operation, SourceVersion,
    SOURCE_CURSOR_WINDOW_BYTES,
};
use crate::storage::{ArenaError, ArenaLimits, ArenaMetrics, PageArena};

/// Explicit document lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentState {
    Open,
    Closing,
    Closed,
}

/// Bounded runtime configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentRuntimeConfig {
    pub max_retired_sources: usize,
    /// Maximum sum of logical UTF-8 lengths held by retirement leases.
    ///
    /// This is intentionally conservative: two leases on the same persistent
    /// Crop root are charged twice even though their storage may be shared.
    pub max_retired_source_bytes: usize,
    /// Maximum number of consecutive scalar edit lineages retained for
    /// authenticated incremental adoption.
    ///
    /// Once this many commits are retained, the oldest transition expires.
    /// Callers that need an expired transition must fall back to a clean parse.
    pub max_retained_source_edit_lineages: usize,
    pub arena_limits: ArenaLimits,
}

impl Default for DocumentRuntimeConfig {
    fn default() -> Self {
        Self {
            max_retired_sources: 8,
            max_retired_source_bytes: 256 * 1024 * 1024,
            max_retained_source_edit_lineages: 64,
            arena_limits: ArenaLimits::default(),
        }
    }
}

/// Move-only proof that an exact source prefix survived a retained edit chain
/// at the same absolute byte and UTF-16 coordinates.
///
/// This witness carries no source text, parser state, checkpoint, role root,
/// or arena identity. A parser-owned restart checkpoint must independently
/// bind the base source and prefix boundary before consuming it.
///
/// ```compile_fail
/// use flark_engine::ExactUnchangedPrefixWitness;
///
/// fn duplicate(witness: ExactUnchangedPrefixWitness) {
///     let _copy = witness.clone();
/// }
/// ```
#[must_use = "an exact-prefix witness must be consumed or deliberately dropped"]
#[derive(Eq, PartialEq)]
pub struct ExactUnchangedPrefixWitness {
    runtime_identity: RuntimeIdentity,
    base: SourceVersion,
    target: SourceVersion,
    byte_end: usize,
    utf16_end: usize,
    lineage_transitions: usize,
}

impl fmt::Debug for ExactUnchangedPrefixWitness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExactUnchangedPrefixWitness")
            .field("base", &self.base)
            .field("target", &self.target)
            .field("byte_end", &self.byte_end)
            .field("utf16_end", &self.utf16_end)
            .field("lineage_transitions", &self.lineage_transitions)
            .finish()
    }
}

impl ExactUnchangedPrefixWitness {
    #[must_use]
    pub const fn base(&self) -> SourceVersion {
        self.base
    }

    #[must_use]
    pub const fn target(&self) -> SourceVersion {
        self.target
    }

    #[must_use]
    pub const fn byte_end(&self) -> usize {
        self.byte_end
    }

    #[must_use]
    pub const fn utf16_end(&self) -> usize {
        self.utf16_end
    }

    #[must_use]
    pub const fn lineage_transitions(&self) -> usize {
        self.lineage_transitions
    }
}

/// Move-only proof that an exact non-empty source suffix survived a retained
/// edit chain, with its shifted target coordinates recorded explicitly.
///
/// This witness carries no source text, parser state, checkpoint, role root,
/// or arena identity. A parser-owned convergence checkpoint must independently
/// bind the base source and suffix boundary before consuming it.
///
/// ```compile_fail
/// use flark_engine::ExactUnchangedSuffixWitness;
///
/// fn duplicate(witness: ExactUnchangedSuffixWitness) {
///     let _copy = witness.clone();
/// }
/// ```
#[must_use = "an exact-suffix witness must be consumed or deliberately dropped"]
#[derive(Eq, PartialEq)]
pub struct ExactUnchangedSuffixWitness {
    runtime_identity: RuntimeIdentity,
    base: SourceVersion,
    target: SourceVersion,
    base_byte_start: usize,
    base_utf16_start: usize,
    target_byte_start: usize,
    target_utf16_start: usize,
    lineage_transitions: usize,
}

impl fmt::Debug for ExactUnchangedSuffixWitness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExactUnchangedSuffixWitness")
            .field("base", &self.base)
            .field("target", &self.target)
            .field("base_byte_start", &self.base_byte_start)
            .field("base_utf16_start", &self.base_utf16_start)
            .field("target_byte_start", &self.target_byte_start)
            .field("target_utf16_start", &self.target_utf16_start)
            .field("lineage_transitions", &self.lineage_transitions)
            .finish()
    }
}

impl ExactUnchangedSuffixWitness {
    #[must_use]
    pub const fn base(&self) -> SourceVersion {
        self.base
    }

    #[must_use]
    pub const fn target(&self) -> SourceVersion {
        self.target
    }

    #[must_use]
    pub const fn base_byte_start(&self) -> usize {
        self.base_byte_start
    }

    #[must_use]
    pub const fn base_utf16_start(&self) -> usize {
        self.base_utf16_start
    }

    #[must_use]
    pub const fn target_byte_start(&self) -> usize {
        self.target_byte_start
    }

    #[must_use]
    pub const fn target_utf16_start(&self) -> usize {
        self.target_utf16_start
    }

    #[must_use]
    pub const fn lineage_transitions(&self) -> usize {
        self.lineage_transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExactUnchangedSuffixProof {
    target_byte_start: usize,
    target_utf16_start: usize,
    lineage_transitions: usize,
}

/// A document lifecycle or admission failure.
#[derive(Debug)]
pub enum DocumentRuntimeError {
    InvalidConfig,
    AllocationFailed,
    NotOpen {
        state: DocumentState,
    },
    ExactUnchangedPrefixLineageUnavailable,
    ExactUnchangedPrefixForeignRuntime,
    ExactUnchangedPrefixStale,
    ExactUnchangedSuffixLineageUnavailable,
    ExactUnchangedSuffixForeignRuntime,
    ExactUnchangedSuffixStale,
    SourceReadWindowTooLarge {
        observed: usize,
        limit: usize,
    },
    RetirementBackpressure {
        needed_leases: usize,
        available_leases: usize,
        needed_bytes: usize,
        available_bytes: usize,
    },
    SourceExceedsRetirementBudget {
        source_bytes: usize,
        limit: usize,
    },
    IdentityExhausted,
    #[cfg(feature = "progressive-source-probe")]
    OpeningSource(OpeningSourceError),
    Source(SourceEditError),
    Arena(ArenaError),
}

impl fmt::Display for DocumentRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => formatter.write_str("document runtime configuration is invalid"),
            Self::AllocationFailed => formatter.write_str("document runtime allocation failed"),
            Self::NotOpen { state } => write!(formatter, "document is not open: {state:?}"),
            Self::ExactUnchangedPrefixLineageUnavailable => {
                formatter.write_str("exact unchanged source-prefix lineage is unavailable")
            }
            Self::ExactUnchangedPrefixForeignRuntime => {
                formatter.write_str("exact source-prefix witness belongs to another runtime")
            }
            Self::ExactUnchangedPrefixStale => {
                formatter.write_str("exact source-prefix witness is stale")
            }
            Self::ExactUnchangedSuffixLineageUnavailable => {
                formatter.write_str("exact unchanged source-suffix lineage is unavailable")
            }
            Self::ExactUnchangedSuffixForeignRuntime => {
                formatter.write_str("exact source-suffix witness belongs to another runtime")
            }
            Self::ExactUnchangedSuffixStale => {
                formatter.write_str("exact source-suffix witness is stale")
            }
            Self::SourceReadWindowTooLarge { observed, limit } => write!(
                formatter,
                "source read window has {observed} bytes but the limit is {limit}"
            ),
            Self::RetirementBackpressure {
                needed_leases,
                available_leases,
                needed_bytes,
                available_bytes,
            } => write!(
                formatter,
                "source retirement requires {needed_leases} leases/{needed_bytes} logical bytes \
                 but only {available_leases} leases/{available_bytes} logical bytes are available"
            ),
            Self::SourceExceedsRetirementBudget {
                source_bytes,
                limit,
            } => write!(
                formatter,
                "source has {source_bytes} logical bytes but the retirement budget is {limit}"
            ),
            Self::IdentityExhausted => formatter.write_str("runtime identity space is exhausted"),
            #[cfg(feature = "progressive-source-probe")]
            Self::OpeningSource(error) => {
                write!(formatter, "opening source transition failed: {error}")
            }
            Self::Source(error) => write!(formatter, "source transition failed: {error}"),
            Self::Arena(error) => write!(formatter, "parser storage failed: {error}"),
        }
    }
}

impl std::error::Error for DocumentRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(feature = "progressive-source-probe")]
            Self::OpeningSource(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::Arena(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SourceEditError> for DocumentRuntimeError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

#[cfg(feature = "progressive-source-probe")]
impl From<OpeningSourceError> for DocumentRuntimeError {
    fn from(error: OpeningSourceError) -> Self {
        Self::OpeningSource(error)
    }
}

impl From<TryReserveError> for DocumentRuntimeError {
    fn from(_: TryReserveError) -> Self {
        Self::AllocationFailed
    }
}

impl From<ArenaError> for DocumentRuntimeError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<RuntimeIdentityError> for DocumentRuntimeError {
    fn from(_: RuntimeIdentityError) -> Self {
        Self::IdentityExhausted
    }
}

/// Receipt for an admitted document edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReceipt {
    source: SourceEditReceipt,
    retired_source_leases: usize,
    retired_source_bytes: usize,
}

impl EditReceipt {
    #[must_use]
    pub const fn source(&self) -> &SourceEditReceipt {
        &self.source
    }

    /// Returns the retirement leases admitted by this edit.
    #[must_use]
    pub const fn retired_source_leases(&self) -> usize {
        self.retired_source_leases
    }

    /// Returns the conservative logical-byte charge admitted by this edit.
    #[must_use]
    pub const fn retired_source_bytes(&self) -> usize {
        self.retired_source_bytes
    }
}

/// Receipt for an admitted atomic UTF-16 document edit intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16EditReceipt {
    source: SourceEditIntentReceipt,
    retired_source_leases: usize,
    retired_source_bytes: usize,
}

impl Utf16EditReceipt {
    /// Returns exact source-version and operation metrics from the commit.
    #[must_use]
    pub const fn source(&self) -> &SourceEditIntentReceipt {
        &self.source
    }

    /// Returns the retirement leases admitted by this edit.
    #[must_use]
    pub const fn retired_source_leases(&self) -> usize {
        self.retired_source_leases
    }

    /// Returns the conservative logical-byte charge admitted by this edit.
    #[must_use]
    pub const fn retired_source_bytes(&self) -> usize {
        self.retired_source_bytes
    }
}

/// Work completed by one fuel-bounded retirement or close poll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrainPoll {
    pub released_source_leases: usize,
    pub released_source_bytes: usize,
    pub arena_transitions: usize,
    pub arena_nodes_reclaimed: usize,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetirementLane {
    Source,
    Arena,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RetirementDemand {
    leases: usize,
    bytes: usize,
}

/// Owns the document source and bounded parser storage without parser semantics.
///
/// The runtime is `Send` so a logically serialized Dart isolate may migrate
/// the endpoint between host OS threads. It is deliberately `!Sync`: one owner
/// must perform every transition, or callers must provide explicit external
/// serialization such as a mutex.
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<flark_engine::DocumentRuntime>();
/// ```
pub struct DocumentRuntime {
    state: DocumentState,
    source: Option<SourceStore>,
    retired_sources: VecDeque<SourceSnapshotLease>,
    retained_source_edit_lineages: VecDeque<SourceEditLineage>,
    max_retained_source_edit_lineages: usize,
    max_retired_sources: usize,
    retired_source_bytes: usize,
    max_retired_source_bytes: usize,
    arena: PageArena,
    document_identity: RuntimeIdentity,
    next_retirement_lane: RetirementLane,
    _not_sync: PhantomData<Cell<()>>,
}

impl Drop for DocumentRuntime {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            let arena = self.arena.metrics();
            debug_assert!(
                self.state == DocumentState::Closed
                    && self.source.is_none()
                    && self.retired_sources.is_empty()
                    && self.retained_source_edit_lineages.is_empty()
                    && self.retired_source_bytes == 0
                    && arena.resident_nodes == 0
                    && arena.reserved_external_payload_bytes == 0
                    && arena.live_builds == 0,
                "DocumentRuntime must be explicitly closed and fuel-drained by its owner; \
                 ordinary Drop cannot yield while releasing persistent source/storage roots"
            );
        }
    }
}

impl fmt::Debug for DocumentRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentRuntime")
            .field("state", &self.state)
            .field("current_source", &self.current_source_version())
            .field("retired_source_count", &self.retired_sources.len())
            .field("retired_source_bytes", &self.retired_source_bytes)
            .field(
                "retained_source_edit_lineage_count",
                &self.retained_source_edit_lineages.len(),
            )
            .field("arena", &self.arena.metrics())
            .finish_non_exhaustive()
    }
}

impl DocumentRuntime {
    /// Creates an open document.
    pub fn new(text: &str, config: DocumentRuntimeConfig) -> Result<Self, DocumentRuntimeError> {
        Self::validate_initial_source(text.len(), config)?;
        let source = SourceStore::new(text)?;
        Self::from_validated_source_store(source, config)
    }

    /// Creates an open document around one already validated source replica.
    ///
    /// The store's exact externally assigned revision and immutable root become
    /// the source authority; this constructor never
    /// re-materializes the source from a `String`.
    pub fn from_source_store(
        source: SourceStore,
        config: DocumentRuntimeConfig,
    ) -> Result<Self, DocumentRuntimeError> {
        Self::validate_initial_source(source.version().byte_len(), config)?;
        Self::from_validated_source_store(source, config)
    }

    /// Creates the probe runtime over one exact admitted opening snapshot.
    ///
    /// The opening store remains the mutation authority. This runtime owns a
    /// serialized read replica that can advance only through a store-minted
    /// append proof; it cannot infer append continuity from roots or lengths.
    #[cfg(feature = "progressive-source-probe")]
    pub fn from_opening_snapshot(
        snapshot: OpeningSourceSnapshot,
        config: DocumentRuntimeConfig,
    ) -> Result<Self, DocumentRuntimeError> {
        let source = snapshot.into_source_store_replica();
        Self::validate_initial_source(source.version().byte_len(), config)?;
        Self::from_validated_source_store(source, config)
    }

    /// Advances the runtime's exact read replica through one append-only
    /// opening transition while retaining the same edit revision.
    ///
    /// The caller consumes the returned receipt explicitly so downstream
    /// progressive state advances from the same authenticated transition.
    #[cfg(feature = "progressive-source-probe")]
    pub fn adopt_opening_append(
        &mut self,
        proof: OpeningSourceAppendProof,
    ) -> Result<SourceAppendReceipt, DocumentRuntimeError> {
        self.ensure_open()?;
        let current = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        self.ensure_retirement_capacity(RetirementDemand {
            leases: 1,
            bytes: current.byte_len(),
        })?;
        let commit = self
            .source
            .as_mut()
            .expect("open documents always own a source")
            .adopt_opening_append(proof)?;
        let (receipt, retired) = commit.into_parts();
        self.enqueue_retired_source(retired);
        Ok(receipt)
    }

    fn validate_initial_source(
        source_bytes: usize,
        config: DocumentRuntimeConfig,
    ) -> Result<(), DocumentRuntimeError> {
        if config.max_retired_sources == 0
            || config.max_retired_source_bytes == 0
            || config.max_retained_source_edit_lineages == 0
        {
            return Err(DocumentRuntimeError::InvalidConfig);
        }
        if source_bytes > config.max_retired_source_bytes {
            return Err(DocumentRuntimeError::SourceExceedsRetirementBudget {
                source_bytes,
                limit: config.max_retired_source_bytes,
            });
        }
        Ok(())
    }

    fn from_validated_source_store(
        source: SourceStore,
        config: DocumentRuntimeConfig,
    ) -> Result<Self, DocumentRuntimeError> {
        let arena = PageArena::new(config.arena_limits)?;
        let document_identity = RuntimeIdentity::allocate(b"document")?;
        let mut retired_sources = VecDeque::new();
        let retirement_capacity = config
            .max_retired_sources
            .checked_add(2)
            .ok_or(DocumentRuntimeError::InvalidConfig)?;
        retired_sources.try_reserve_exact(retirement_capacity)?;
        let mut retained_source_edit_lineages = VecDeque::new();
        retained_source_edit_lineages
            .try_reserve_exact(config.max_retained_source_edit_lineages)?;
        Ok(Self {
            state: DocumentState::Open,
            source: Some(source),
            retired_sources,
            retained_source_edit_lineages,
            max_retained_source_edit_lineages: config.max_retained_source_edit_lineages,
            max_retired_sources: config.max_retired_sources,
            retired_source_bytes: 0,
            max_retired_source_bytes: config.max_retired_source_bytes,
            arena,
            document_identity,
            next_retirement_lane: RetirementLane::Source,
            _not_sync: PhantomData,
        })
    }

    #[must_use]
    pub const fn state(&self) -> DocumentState {
        self.state
    }

    #[must_use]
    pub fn current_source_version(&self) -> Option<SourceVersion> {
        self.source.as_ref().map(SourceStore::version)
    }

    /// Borrows the exact current immutable source for a bounded parser job.
    ///
    /// The returned lease owns no copied document buffer and participates in
    /// the runtime's explicit source-retirement lifecycle.
    pub fn snapshot_current_source(&self) -> Result<SourceSnapshotLease, DocumentRuntimeError> {
        self.ensure_open()?;
        Ok(self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .snapshot())
    }

    /// Returns the retained scalar lineage whose old authority is `previous`.
    ///
    /// Each successful call exposes exactly one consecutive source transition,
    /// so a future adoption job can charge one lineage traversal to one unit of
    /// fuel. `None` means the transition was never retained, has expired, or
    /// was cleared by close; the only correct response is a clean parse.
    #[must_use]
    pub fn retained_source_edit_lineage_after(
        &self,
        previous: SourceVersion,
    ) -> Option<&SourceEditLineage> {
        let oldest = self.retained_source_edit_lineages.front()?;
        let revision_offset = previous
            .revision()
            .get()
            .checked_sub(oldest.previous().revision().get())?;
        let index = usize::try_from(revision_offset).ok()?;
        let lineage = self.retained_source_edit_lineages.get(index)?;
        (lineage.previous() == previous).then_some(lineage)
    }

    /// Proves that `0..byte_end` and `0..utf16_end` survived every retained
    /// source transition to the current revision at identical absolute
    /// coordinates.
    ///
    /// The parser supplies these two ends from its opaque restart checkpoint;
    /// this runtime supplies only edit-lineage authority. Equal bytes, hashes,
    /// or nearby source inspection are intentionally insufficient.
    pub fn mint_exact_unchanged_prefix_witness(
        &self,
        base: SourceVersion,
        byte_end: usize,
        utf16_end: usize,
    ) -> Result<ExactUnchangedPrefixWitness, DocumentRuntimeError> {
        self.ensure_open()?;
        let target = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let lineage_transitions = self
            .prove_exact_unchanged_prefix(base, target, byte_end, utf16_end)
            .ok_or(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable)?;
        Ok(ExactUnchangedPrefixWitness {
            runtime_identity: self.document_identity,
            base,
            target,
            byte_end,
            utf16_end,
            lineage_transitions,
        })
    }

    /// Consumes and revalidates a runtime-minted exact-prefix witness.
    ///
    /// The returned value remains move-only and is ready for a parser-owned
    /// checkpoint consumer. Any intervening edit, expired transition, or
    /// foreign runtime fails closed.
    pub fn take_exact_unchanged_prefix_witness(
        &self,
        witness: ExactUnchangedPrefixWitness,
    ) -> Result<ExactUnchangedPrefixWitness, DocumentRuntimeError> {
        self.ensure_open()?;
        if witness.runtime_identity != self.document_identity {
            return Err(DocumentRuntimeError::ExactUnchangedPrefixForeignRuntime);
        }
        let current = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        if current != witness.target
            || self.prove_exact_unchanged_prefix(
                witness.base,
                witness.target,
                witness.byte_end,
                witness.utf16_end,
            ) != Some(witness.lineage_transitions)
        {
            return Err(DocumentRuntimeError::ExactUnchangedPrefixStale);
        }
        Ok(witness)
    }

    fn prove_exact_unchanged_prefix(
        &self,
        base: SourceVersion,
        target: SourceVersion,
        byte_end: usize,
        utf16_end: usize,
    ) -> Option<usize> {
        if base == target
            || byte_end == 0
            || utf16_end == 0
            || byte_end > base.byte_len()
            || utf16_end > base.utf16_len()
            || byte_end > target.byte_len()
            || utf16_end > target.utf16_len()
        {
            return None;
        }
        let expected_bytes = 0..byte_end;
        let expected_utf16 = 0..utf16_end;
        let mut current = base;
        for transitions in 1..=self.max_retained_source_edit_lineages {
            let lineage = self.retained_source_edit_lineage_after(current)?;
            let next = lineage.current();
            if lineage
                .map_unchanged_byte_range(current, next, expected_bytes.clone())
                .ok()?
                != expected_bytes
                || lineage
                    .map_unchanged_utf16_range(current, next, expected_utf16.clone())
                    .ok()?
                    != expected_utf16
            {
                return None;
            }
            if next == target {
                return Some(transitions);
            }
            current = next;
        }
        None
    }

    /// Proves that the non-empty byte and UTF-16 suffixes beginning at the
    /// supplied base coordinates survived every retained source transition to
    /// the current revision.
    ///
    /// Unlike an unchanged prefix, edits before the suffix may shift its target
    /// coordinates. The returned witness records both exact starts. The parser
    /// must independently prove that they are the physical-line boundary bound
    /// into its convergence checkpoint.
    pub fn mint_exact_unchanged_suffix_witness(
        &self,
        base: SourceVersion,
        base_byte_start: usize,
        base_utf16_start: usize,
    ) -> Result<ExactUnchangedSuffixWitness, DocumentRuntimeError> {
        self.ensure_open()?;
        let target = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let proof = self
            .prove_exact_unchanged_suffix(base, target, base_byte_start, base_utf16_start)
            .ok_or(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable)?;
        Ok(ExactUnchangedSuffixWitness {
            runtime_identity: self.document_identity,
            base,
            target,
            base_byte_start,
            base_utf16_start,
            target_byte_start: proof.target_byte_start,
            target_utf16_start: proof.target_utf16_start,
            lineage_transitions: proof.lineage_transitions,
        })
    }

    /// Consumes and revalidates a runtime-minted exact-suffix witness.
    ///
    /// The returned value remains move-only and is ready for a parser-owned
    /// convergence consumer. Any intervening edit, expired transition, or
    /// foreign runtime fails closed.
    pub fn take_exact_unchanged_suffix_witness(
        &self,
        witness: ExactUnchangedSuffixWitness,
    ) -> Result<ExactUnchangedSuffixWitness, DocumentRuntimeError> {
        self.ensure_open()?;
        if witness.runtime_identity != self.document_identity {
            return Err(DocumentRuntimeError::ExactUnchangedSuffixForeignRuntime);
        }
        let current = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let expected = ExactUnchangedSuffixProof {
            target_byte_start: witness.target_byte_start,
            target_utf16_start: witness.target_utf16_start,
            lineage_transitions: witness.lineage_transitions,
        };
        if current != witness.target
            || self.prove_exact_unchanged_suffix(
                witness.base,
                witness.target,
                witness.base_byte_start,
                witness.base_utf16_start,
            ) != Some(expected)
        {
            return Err(DocumentRuntimeError::ExactUnchangedSuffixStale);
        }
        Ok(witness)
    }

    fn prove_exact_unchanged_suffix(
        &self,
        base: SourceVersion,
        target: SourceVersion,
        base_byte_start: usize,
        base_utf16_start: usize,
    ) -> Option<ExactUnchangedSuffixProof> {
        if base == target
            || base_byte_start >= base.byte_len()
            || base_utf16_start >= base.utf16_len()
        {
            return None;
        }
        let mut byte_start = base_byte_start;
        let mut utf16_start = base_utf16_start;
        let mut current = base;
        for transitions in 1..=self.max_retained_source_edit_lineages {
            let lineage = self.retained_source_edit_lineage_after(current)?;
            let next = lineage.current();
            let mapped_bytes = lineage
                .map_unchanged_byte_range(current, next, byte_start..current.byte_len())
                .ok()?;
            let mapped_utf16 = lineage
                .map_unchanged_utf16_range(current, next, utf16_start..current.utf16_len())
                .ok()?;
            if mapped_bytes.end != next.byte_len() || mapped_utf16.end != next.utf16_len() {
                return None;
            }
            byte_start = mapped_bytes.start;
            utf16_start = mapped_utf16.start;
            if next == target {
                return Some(ExactUnchangedSuffixProof {
                    target_byte_start: byte_start,
                    target_utf16_start: utf16_start,
                    lineage_transitions: transitions,
                });
            }
            current = next;
        }
        None
    }

    /// Copies one scalar-aligned, bounded window without letting a source lease
    /// escape runtime ownership. This is intended for diagnostics and narrow
    /// adapters; full parsing should borrow an immutable source snapshot.
    pub fn read_current_source_window(
        &self,
        range: Range<usize>,
        output: &mut [u8],
    ) -> Result<usize, DocumentRuntimeError> {
        self.ensure_open()?;
        let requested = range.end.saturating_sub(range.start);
        if requested > SOURCE_CURSOR_WINDOW_BYTES || requested > output.len() {
            return Err(DocumentRuntimeError::SourceReadWindowTooLarge {
                observed: requested,
                limit: SOURCE_CURSOR_WINDOW_BYTES.min(output.len()),
            });
        }
        let source = self
            .source
            .as_ref()
            .expect("open documents always own a source");
        let mut cursor = source.snapshot().cursor_in(range)?;
        Ok(cursor.read(&mut output[..requested]))
    }

    #[must_use]
    pub fn retired_source_count(&self) -> usize {
        self.retired_sources.len()
    }

    /// Returns the conservatively charged logical bytes awaiting release.
    #[must_use]
    pub const fn retired_source_bytes(&self) -> usize {
        self.retired_source_bytes
    }

    /// Returns parser-arena residency without exposing mutation.
    #[must_use]
    pub const fn arena_metrics(&self) -> ArenaMetrics {
        self.arena.metrics()
    }

    /// Borrows the one document-owned arena used by parser roots.
    ///
    /// Parser capabilities live outside the runtime state machine, but
    /// every read remains scoped to the document owner so no arena handle can
    /// escape into a long-lived producer object.
    pub(crate) const fn producer_arena(&self) -> &PageArena {
        &self.arena
    }

    /// Mutably borrows the one document-owned arena used by parser builds and
    /// explicit root reclamation.
    pub(crate) fn producer_arena_mut(&mut self) -> &mut PageArena {
        &mut self.arena
    }

    /// Stable capability identity used to reject parser work presented
    /// with a different document runtime after the arena borrow has ended.
    #[cfg(feature = "parser-internal")]
    pub(crate) const fn producer_identity(&self) -> RuntimeIdentity {
        self.document_identity
    }

    /// Admits an edit and retires the previous immutable source root.
    pub fn apply_edit(
        &mut self,
        expected: SourceVersion,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<EditReceipt, DocumentRuntimeError> {
        self.ensure_open()?;
        let current = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        self.source
            .as_ref()
            .expect("open documents always own a source")
            .validate_edit(expected, &range)?;
        let next_source_bytes = current
            .byte_len()
            .checked_sub(range.end - range.start)
            .and_then(|len| len.checked_add(replacement.len()))
            .ok_or(DocumentRuntimeError::SourceExceedsRetirementBudget {
                source_bytes: usize::MAX,
                limit: self.max_retired_source_bytes,
            })?;
        if next_source_bytes > self.max_retired_source_bytes {
            return Err(DocumentRuntimeError::SourceExceedsRetirementBudget {
                source_bytes: next_source_bytes,
                limit: self.max_retired_source_bytes,
            });
        }
        let retirement = self.edit_retirement_demand(current);
        self.ensure_retirement_capacity(retirement)?;
        let prepared = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .prepare_edit(expected, range, replacement)?;

        let commit = self
            .source
            .as_mut()
            .expect("open documents always own a source")
            .commit_prepared_edit(prepared)?;
        let (source_receipt, retired_source, lineage) = commit.into_parts_with_lineage();
        self.retain_source_edit_lineage(lineage);
        self.enqueue_retired_source(retired_source);
        Ok(EditReceipt {
            source: source_receipt,
            retired_source_leases: retirement.leases,
            retired_source_bytes: retirement.bytes,
        })
    }

    /// Admits one atomic edit intent expressed in base-revision UTF-16 units.
    ///
    /// Source validation and target construction happen off-authority. The
    /// target-size and retirement budgets are then checked before the prepared
    /// root can become current, so every rejection leaves the source untouched.
    pub fn apply_utf16_edit_intent(
        &mut self,
        expected: SourceVersion,
        declared_revision: SourceRevision,
        operations: &[SourceUtf16Operation<'_>],
    ) -> Result<Utf16EditReceipt, DocumentRuntimeError> {
        self.ensure_open()?;
        let current = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let plan = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .plan_utf16_edit_intent(expected, declared_revision, operations)?;

        let next_source_bytes = plan.target_byte_len();
        if next_source_bytes > self.max_retired_source_bytes {
            return Err(DocumentRuntimeError::SourceExceedsRetirementBudget {
                source_bytes: next_source_bytes,
                limit: self.max_retired_source_bytes,
            });
        }
        let retirement = self.edit_retirement_demand(current);
        self.ensure_retirement_capacity(retirement)?;

        let prepared = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .materialize_utf16_edit_intent(plan)?;

        let commit = self
            .source
            .as_mut()
            .expect("open documents always own a source")
            .commit_prepared_utf16_edit_intent(prepared)?;
        let (source_receipt, retired_source, lineage) = commit.into_parts_with_lineage();
        self.retain_source_edit_lineage(lineage);
        self.enqueue_retired_source(retired_source);
        Ok(Utf16EditReceipt {
            source: source_receipt,
            retired_source_leases: retirement.leases,
            retired_source_bytes: retirement.bytes,
        })
    }

    /// Transfers all live source leases into Closing. Repeated calls are no-ops.
    pub fn begin_close(&mut self) -> Result<bool, DocumentRuntimeError> {
        match self.state {
            DocumentState::Closed | DocumentState::Closing => return Ok(false),
            DocumentState::Open => {}
        }
        // Close is terminal and cannot admit more work, so its final source
        // leases use the pre-reserved close margin rather than becoming
        // impossible solely because the open-state backlog is at its cap.
        let source = self
            .source
            .take()
            .expect("open documents always own a source")
            .into_snapshot();
        self.enqueue_retired_source(source);
        self.retained_source_edit_lineages.clear();
        self.state = DocumentState::Closing;
        Ok(true)
    }

    /// Drops at most `fuel` retired source leases while Closing.
    pub fn poll_close(&mut self, fuel: usize) -> Result<DrainPoll, DocumentRuntimeError> {
        if self.state == DocumentState::Open {
            return Err(DocumentRuntimeError::NotOpen { state: self.state });
        }
        if self.state == DocumentState::Closed {
            return Ok(DrainPoll {
                released_source_leases: 0,
                released_source_bytes: 0,
                arena_transitions: 0,
                arena_nodes_reclaimed: 0,
                complete: true,
            });
        }

        Ok(self.poll_retirement(fuel))
    }

    /// Drains superseded source and parser storage in both Open and Closing.
    pub fn poll_retirement(&mut self, fuel: usize) -> DrainPoll {
        let mut released_source_leases = 0;
        let mut released_source_bytes = 0;
        let mut arena_transitions = 0;
        let mut arena_nodes_reclaimed = 0;
        let mut transitions = 0;
        while transitions < fuel {
            let source_pending = !self.retired_sources.is_empty();
            let arena_metrics = self.arena.metrics();
            let arena_pending =
                arena_metrics.pending_reclaims > 0 || arena_metrics.pending_build_aborts > 0;
            if !source_pending && !arena_pending {
                break;
            }

            let lane = match (source_pending, arena_pending) {
                (true, true) => self.next_retirement_lane,
                (true, false) => RetirementLane::Source,
                (false, true) => RetirementLane::Arena,
                (false, false) => unreachable!(),
            };
            match lane {
                RetirementLane::Source => {
                    let lease = self
                        .retired_sources
                        .pop_front()
                        .expect("source lane requires a retired lease");
                    let bytes = lease.version().byte_len();
                    self.retired_source_bytes -= bytes;
                    drop(lease);
                    released_source_leases += 1;
                    released_source_bytes += bytes;
                    self.next_retirement_lane = RetirementLane::Arena;
                }
                RetirementLane::Arena => {
                    let receipt = self.arena.poll_reclaim(1);
                    debug_assert_eq!(receipt.transitions, 1);
                    arena_transitions += receipt.transitions;
                    arena_nodes_reclaimed += receipt.nodes_reclaimed;
                    self.next_retirement_lane = RetirementLane::Source;
                }
            }
            transitions += 1;
        }
        let arena_metrics = self.arena.metrics();
        let retirement_idle = self.retired_sources.is_empty()
            && arena_metrics.pending_reclaims == 0
            && arena_metrics.pending_build_aborts == 0;
        if self.state == DocumentState::Closing
            && retirement_idle
            && arena_metrics.resident_nodes == 0
            && arena_metrics.reserved_external_payload_bytes == 0
            && arena_metrics.live_builds == 0
        {
            self.state = DocumentState::Closed;
        }
        DrainPoll {
            released_source_leases,
            released_source_bytes,
            arena_transitions,
            arena_nodes_reclaimed,
            complete: if self.state == DocumentState::Open {
                retirement_idle
            } else {
                self.state == DocumentState::Closed
            },
        }
    }

    fn ensure_open(&self) -> Result<(), DocumentRuntimeError> {
        if self.state == DocumentState::Open {
            Ok(())
        } else {
            Err(DocumentRuntimeError::NotOpen { state: self.state })
        }
    }

    fn ensure_retirement_capacity(
        &self,
        needed: RetirementDemand,
    ) -> Result<(), DocumentRuntimeError> {
        let available_leases = self
            .max_retired_sources
            .saturating_sub(self.retired_sources.len());
        let available_bytes = self
            .max_retired_source_bytes
            .saturating_sub(self.retired_source_bytes);
        if needed.leases > available_leases || needed.bytes > available_bytes {
            Err(DocumentRuntimeError::RetirementBackpressure {
                needed_leases: needed.leases,
                available_leases,
                needed_bytes: needed.bytes,
                available_bytes,
            })
        } else {
            Ok(())
        }
    }

    fn edit_retirement_demand(&self, current: SourceVersion) -> RetirementDemand {
        RetirementDemand {
            leases: 1,
            bytes: current.byte_len(),
        }
    }

    fn enqueue_retired_source(&mut self, source: SourceSnapshotLease) {
        self.retired_source_bytes = self
            .retired_source_bytes
            .saturating_add(source.version().byte_len());
        self.retired_sources.push_back(source);
    }

    fn retain_source_edit_lineage(&mut self, lineage: SourceEditLineage) {
        debug_assert_eq!(
            self.source
                .as_ref()
                .expect("committed edits keep the document source open")
                .version(),
            lineage.current()
        );
        if let Some(previous) = self.retained_source_edit_lineages.back() {
            debug_assert_eq!(previous.current(), lineage.previous());
        }
        if self.retained_source_edit_lineages.len() == self.max_retained_source_edit_lineages {
            self.retained_source_edit_lineages.pop_front();
        }
        debug_assert!(
            self.retained_source_edit_lineages.capacity() >= self.max_retained_source_edit_lineages
        );
        self.retained_source_edit_lineages.push_back(lineage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(mut runtime: DocumentRuntime) {
        if runtime.state() == DocumentState::Open {
            runtime.begin_close().expect("begin close");
        }
        while runtime.state() != DocumentState::Closed {
            runtime.poll_close(usize::MAX).expect("poll close");
        }
    }

    #[test]
    fn both_edit_paths_retain_consecutive_scalar_lineage() {
        let mut runtime =
            DocumentRuntime::new("a😀b", DocumentRuntimeConfig::default()).expect("runtime");
        let initial = runtime.current_source_version().expect("initial source");

        let byte_edit = runtime.apply_edit(initial, 0..0, "<").expect("byte edit");
        let after_byte_edit = byte_edit.source().current();
        let byte_lineage = runtime
            .retained_source_edit_lineage_after(initial)
            .expect("byte lineage");
        assert_eq!(byte_lineage.previous(), initial);
        assert_eq!(byte_lineage.current(), after_byte_edit);
        assert_eq!(
            byte_lineage
                .map_unchanged_byte_range(initial, after_byte_edit, 0..6)
                .expect("unchanged original source"),
            1..7
        );
        let foreign = SourceStore::new("a😀b").expect("foreign source").version();
        assert!(runtime
            .retained_source_edit_lineage_after(foreign)
            .is_none());

        let append = after_byte_edit.utf16_len();
        let intent_edit = runtime
            .apply_utf16_edit_intent(
                after_byte_edit,
                SourceRevision::new(after_byte_edit.revision().get() + 1),
                &[SourceUtf16Operation::new(append..append, ">")],
            )
            .expect("UTF-16 edit intent");
        let current = intent_edit.source().current();
        let intent_lineage = runtime
            .retained_source_edit_lineage_after(after_byte_edit)
            .expect("intent lineage");
        assert_eq!(intent_lineage.previous(), after_byte_edit);
        assert_eq!(intent_lineage.current(), current);
        assert_eq!(runtime.retained_source_edit_lineages.len(), 2);

        close(runtime);
    }

    #[test]
    fn exact_unchanged_prefix_witness_survives_tail_edits_at_absolute_coordinates() {
        let source = "[é]: /世界\r\n[b]: /two\r\n\nvisible 😀\n";
        let prefix_end = source.find("visible").expect("visible tail");
        let prefix_utf16 = source[..prefix_end].encode_utf16().count();
        assert_ne!(prefix_end, prefix_utf16);
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let base = runtime.current_source_version().expect("base source");
        let first = runtime
            .apply_edit(base, prefix_end..prefix_end + "visible".len(), "shown")
            .expect("tail replacement")
            .source()
            .current();
        let target = runtime
            .apply_edit(first, prefix_end..prefix_end, "fresh ")
            .expect("boundary insertion")
            .source()
            .current();

        let witness = runtime
            .mint_exact_unchanged_prefix_witness(base, prefix_end, prefix_utf16)
            .expect("unchanged prefix witness");
        assert_eq!(witness.base(), base);
        assert_eq!(witness.target(), target);
        assert_eq!(witness.byte_end(), prefix_end);
        assert_eq!(witness.utf16_end(), prefix_utf16);
        assert_eq!(witness.lineage_transitions(), 2);
        let consumed = runtime
            .take_exact_unchanged_prefix_witness(witness)
            .expect("revalidated one-use witness");
        assert_eq!(consumed.target(), target);

        close(runtime);
    }

    #[test]
    fn exact_unchanged_prefix_witness_rejects_crossed_or_shifted_prefixes() {
        let source = "[a]: /one\n\nvisible\n";
        let prefix_end = source.find("visible").expect("visible tail");

        let mut crossed = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .expect("crossed runtime");
        let crossed_base = crossed.current_source_version().expect("crossed base");
        crossed
            .apply_edit(crossed_base, 1..2, "z")
            .expect("prefix replacement");
        assert!(matches!(
            crossed.mint_exact_unchanged_prefix_witness(crossed_base, prefix_end, prefix_end),
            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable)
        ));
        close(crossed);

        let mut shifted = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .expect("shifted runtime");
        let shifted_base = shifted.current_source_version().expect("shifted base");
        shifted
            .apply_edit(shifted_base, 0..0, "x")
            .expect("prefix shift");
        assert!(matches!(
            shifted.mint_exact_unchanged_prefix_witness(shifted_base, prefix_end, prefix_end),
            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable)
        ));
        close(shifted);
    }

    #[test]
    fn exact_unchanged_prefix_witness_is_runtime_bound_and_stale_after_edit() {
        let source = "[a]: /one\n\nvisible\n";
        let prefix_end = source.find("visible").expect("visible tail");
        let mut origin =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("origin runtime");
        let origin_base = origin.current_source_version().expect("origin base");
        let origin_target = origin
            .apply_edit(origin_base, prefix_end..prefix_end + 1, "V")
            .expect("origin tail edit")
            .source()
            .current();
        let foreign_witness = origin
            .mint_exact_unchanged_prefix_witness(origin_base, prefix_end, prefix_end)
            .expect("foreign witness");

        let mut foreign = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .expect("foreign runtime");
        let foreign_base = foreign.current_source_version().expect("foreign base");
        foreign
            .apply_edit(foreign_base, prefix_end..prefix_end + 1, "V")
            .expect("foreign tail edit");
        assert!(matches!(
            foreign.take_exact_unchanged_prefix_witness(foreign_witness),
            Err(DocumentRuntimeError::ExactUnchangedPrefixForeignRuntime)
        ));

        let stale_witness = origin
            .mint_exact_unchanged_prefix_witness(origin_base, prefix_end, prefix_end)
            .expect("stale witness");
        origin
            .apply_edit(
                origin_target,
                origin_target.byte_len()..origin_target.byte_len(),
                "more",
            )
            .expect("later edit");
        assert!(matches!(
            origin.take_exact_unchanged_prefix_witness(stale_witness),
            Err(DocumentRuntimeError::ExactUnchangedPrefixStale)
        ));

        close(origin);
        close(foreign);
    }

    #[test]
    fn exact_unchanged_suffix_witness_maps_unicode_suffix_through_length_changes() {
        let source = "α😀 prefix\nedit me\nunchanged 世界\r\nlast 😀\n";
        let edited = "edit me";
        let replacement = "changed much longer 😀";
        let insertion = "inserted line\n";
        let edit_start = source.find(edited).expect("edited source");
        let suffix_start = source.find("unchanged").expect("unchanged suffix");
        let suffix_utf16 = source[..suffix_start].encode_utf16().count();
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let base = runtime.current_source_version().expect("base source");

        let first = runtime
            .apply_edit(base, edit_start..edit_start + edited.len(), replacement)
            .expect("length-changing replacement")
            .source()
            .current();
        let first_suffix_start = suffix_start - edited.len() + replacement.len();
        let first_suffix_utf16 =
            suffix_utf16 - edited.encode_utf16().count() + replacement.encode_utf16().count();
        let target = runtime
            .apply_edit(first, first_suffix_start..first_suffix_start, insertion)
            .expect("insertion before suffix")
            .source()
            .current();
        let target_suffix_start = first_suffix_start + insertion.len();
        let target_suffix_utf16 = first_suffix_utf16 + insertion.encode_utf16().count();

        let witness = runtime
            .mint_exact_unchanged_suffix_witness(base, suffix_start, suffix_utf16)
            .expect("unchanged suffix witness");
        assert_eq!(witness.base(), base);
        assert_eq!(witness.target(), target);
        assert_eq!(witness.base_byte_start(), suffix_start);
        assert_eq!(witness.base_utf16_start(), suffix_utf16);
        assert_eq!(witness.target_byte_start(), target_suffix_start);
        assert_eq!(witness.target_utf16_start(), target_suffix_utf16);
        assert_eq!(witness.lineage_transitions(), 2);
        let consumed = runtime
            .take_exact_unchanged_suffix_witness(witness)
            .expect("revalidated one-use witness");
        assert_eq!(consumed.target_byte_start(), target_suffix_start);
        assert_eq!(consumed.target_utf16_start(), target_suffix_utf16);

        close(runtime);
    }

    #[test]
    fn exact_unchanged_suffix_witness_rejects_edited_or_empty_suffixes() {
        let source = "before\nunchanged 😀\nlast\n";
        let suffix_start = source.find("unchanged").expect("suffix");
        let suffix_utf16 = source[..suffix_start].encode_utf16().count();
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let base = runtime.current_source_version().expect("base source");
        runtime
            .apply_edit(base, suffix_start + 1..suffix_start + 2, "X")
            .expect("edit inside suffix");

        assert!(matches!(
            runtime.mint_exact_unchanged_suffix_witness(base, suffix_start, suffix_utf16),
            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable)
        ));
        assert!(matches!(
            runtime.mint_exact_unchanged_suffix_witness(base, base.byte_len(), base.utf16_len()),
            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable)
        ));

        close(runtime);
    }

    #[test]
    fn exact_unchanged_suffix_witness_is_runtime_bound_and_stale_after_edit() {
        let source = "before\nunchanged 😀\nlast\n";
        let before = "before";
        let replacement = "longer prefix";
        let suffix_start = source.find("unchanged").expect("suffix");
        let suffix_utf16 = source[..suffix_start].encode_utf16().count();
        let mut origin =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("origin runtime");
        let origin_base = origin.current_source_version().expect("origin base");
        let origin_target = origin
            .apply_edit(origin_base, 0..before.len(), replacement)
            .expect("origin prefix edit")
            .source()
            .current();
        let foreign_witness = origin
            .mint_exact_unchanged_suffix_witness(origin_base, suffix_start, suffix_utf16)
            .expect("foreign witness");

        let mut foreign = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .expect("foreign runtime");
        let foreign_base = foreign.current_source_version().expect("foreign base");
        foreign
            .apply_edit(foreign_base, 0..before.len(), replacement)
            .expect("foreign prefix edit");
        assert!(matches!(
            foreign.take_exact_unchanged_suffix_witness(foreign_witness),
            Err(DocumentRuntimeError::ExactUnchangedSuffixForeignRuntime)
        ));

        let stale_witness = origin
            .mint_exact_unchanged_suffix_witness(origin_base, suffix_start, suffix_utf16)
            .expect("stale witness");
        origin
            .apply_edit(origin_target, 0..0, "!")
            .expect("later prefix edit");
        assert!(matches!(
            origin.take_exact_unchanged_suffix_witness(stale_witness),
            Err(DocumentRuntimeError::ExactUnchangedSuffixStale)
        ));

        close(origin);
        close(foreign);
    }

    #[test]
    fn bounded_lineage_chain_expires_oldest_transition_to_clean_fallback() {
        let mut runtime = DocumentRuntime::new(
            "abc",
            DocumentRuntimeConfig {
                max_retained_source_edit_lineages: 2,
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect("runtime");
        let first = runtime.current_source_version().expect("first source");
        let second = runtime
            .apply_edit(first, 3..3, "1")
            .expect("first edit")
            .source()
            .current();
        let third = runtime
            .apply_edit(second, 4..4, "2")
            .expect("second edit")
            .source()
            .current();
        let fourth = runtime
            .apply_edit(third, 5..5, "3")
            .expect("third edit")
            .source()
            .current();

        assert_eq!(runtime.retained_source_edit_lineages.len(), 2);
        assert!(runtime.retained_source_edit_lineage_after(first).is_none());
        assert_eq!(
            runtime
                .retained_source_edit_lineage_after(second)
                .expect("second transition retained")
                .current(),
            third
        );
        assert_eq!(
            runtime
                .retained_source_edit_lineage_after(third)
                .expect("third transition retained")
                .current(),
            fourth
        );
        assert!(runtime.retained_source_edit_lineage_after(fourth).is_none());
        assert!(matches!(
            runtime.mint_exact_unchanged_prefix_witness(first, 1, 1),
            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable)
        ));
        assert!(matches!(
            runtime.mint_exact_unchanged_suffix_witness(first, 1, 1),
            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable)
        ));

        runtime.begin_close().expect("begin close");
        assert!(runtime.retained_source_edit_lineages.is_empty());
        assert!(runtime.retained_source_edit_lineage_after(third).is_none());
        close(runtime);
    }

    #[test]
    fn zero_lineage_capacity_is_rejected_before_runtime_construction() {
        let error = DocumentRuntime::new(
            "abc",
            DocumentRuntimeConfig {
                max_retained_source_edit_lineages: 0,
                ..DocumentRuntimeConfig::default()
            },
        )
        .expect_err("zero lineage capacity must be invalid");
        assert!(matches!(error, DocumentRuntimeError::InvalidConfig));
    }
}

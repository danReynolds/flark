use std::cell::Cell;
use std::collections::{TryReserveError, VecDeque};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Range;

use crate::candidate_manifest::{
    CandidateAuthority, CandidateManifestAssembler, CanonicalRoleInputs, ManifestError,
    StrongIdentity,
};
use crate::identity::{CandidateGeneration, SourceRevision};
use crate::measured_sequence::SequenceInspectionReceipt;
use crate::reference_root::ReferenceRootLimits;
use crate::source::{
    SourceBoundaryAffinity, SourceEditError, SourceEditIntentReceipt, SourceEditLineage,
    SourceEditLineageError, SourceEditReceipt, SourceSnapshotLease, SourceStore,
    SourceUtf16Operation, SourceVersion, SOURCE_CURSOR_WINDOW_BYTES,
};
#[cfg(feature = "progressive-source-probe")]
use crate::source::{
    OpeningSourceAppendProof, OpeningSourceError, OpeningSourceSnapshot, SourceAppendReceipt,
};
use crate::source_facts::{
    splice_persistent_source_facts_atomic_with_receipt, CertifiedSource, ParserProfileId,
    PersistentSourceFactsBuild, PersistentSourceFactsBuildPoll, PersistentSourceFactsRoot,
    PersistentSourceFactsRootAuthoritySnapshot, PersistentSourceFactsWork, SourceFactCheckpoint,
    SourceFactSegmentSummary, SourceFactsAssemblyError, SourceFactsCompletion, SourceFactsCoverage,
    SourceFactsError, SourceFactsPoll, SourceFactsRootAdmission, SourceFactsRootBuilder,
    SourceFactsRootLimits, SourceFactsScanProfile, SourceFactsScanner, SourceFactsWork,
    SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX,
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

/// The one newest parse request retained by the runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParsePlan {
    generation: CandidateGeneration,
    source: SourceVersion,
}

impl ParsePlan {
    #[must_use]
    pub const fn generation(self) -> CandidateGeneration {
        self.generation
    }

    #[must_use]
    pub const fn source(self) -> SourceVersion {
        self.source
    }
}

/// Observable identity of the one active candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveCandidateInfo {
    generation: CandidateGeneration,
    source: SourceVersion,
}

impl ActiveCandidateInfo {
    #[must_use]
    pub const fn generation(self) -> CandidateGeneration {
        self.generation
    }

    #[must_use]
    pub const fn source(self) -> SourceVersion {
        self.source
    }
}

struct ActiveCandidate {
    plan: ParsePlan,
    source: SourceSnapshotLease,
    manifest: CandidateManifestAssembler,
}

struct RuntimeSourceFactsJob {
    incremental: Option<RuntimeIncrementalSourceFacts>,
    scanner: Option<SourceFactsScanner>,
    builder: Option<SourceFactsRootBuilder>,
    persistent: Option<PersistentSourceFactsBuild>,
    completion: Option<SourceFactsCompletion>,
    certified: Option<CertifiedSource>,
}

struct RuntimeIncrementalSourceFacts {
    base: Option<PersistentSourceFactsRoot>,
    segment: Option<PersistentSourceFactsRoot>,
    base_source: SourceVersion,
    parser_profile: ParserProfileId,
    profile: SourceFactsScanProfile,
    base_page_range: Range<u64>,
    base_page_count: u64,
    base_byte_range: Range<usize>,
    target_byte_range: Range<usize>,
    exact_parser_edit_envelope: Option<ExactParserEditEnvelope>,
    target: SourceVersion,
    lineage_transitions: usize,
    planning_work: PersistentSourceFactsWork,
    scan_work: PersistentSourceFactsDeltaScanWork,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExactParserEditEnvelope {
    base_byte_range: Range<usize>,
    target_byte_range: Range<usize>,
}

impl ExactParserEditEnvelope {
    fn at_base(base_byte_range: Range<usize>) -> Self {
        Self {
            target_byte_range: base_byte_range.clone(),
            base_byte_range,
        }
    }

    fn map_through(
        self,
        lineage: &SourceEditLineage,
        previous: SourceVersion,
        current: SourceVersion,
    ) -> Option<Self> {
        if lineage.spans().iter().any(|span| {
            span.old_bytes().start < self.target_byte_range.start
                || span.old_bytes().end > self.target_byte_range.end
        }) {
            return None;
        }
        let target_start = lineage
            .map_byte_boundary(
                previous,
                current,
                self.target_byte_range.start,
                SourceBoundaryAffinity::Before,
            )
            .ok()?;
        let target_end = lineage
            .map_byte_boundary(
                previous,
                current,
                self.target_byte_range.end,
                SourceBoundaryAffinity::After,
            )
            .ok()?;
        (target_start <= target_end).then_some(Self {
            base_byte_range: self.base_byte_range,
            target_byte_range: target_start..target_end,
        })
    }

    fn is_valid_within(
        &self,
        base: SourceVersion,
        target: SourceVersion,
        base_facts_range: &Range<usize>,
        target_facts_range: &Range<usize>,
    ) -> bool {
        byte_range_is_contained(&self.base_byte_range, base_facts_range, base.byte_len())
            && byte_range_is_contained(
                &self.target_byte_range,
                target_facts_range,
                target.byte_len(),
            )
    }
}

fn byte_range_is_contained(inner: &Range<usize>, outer: &Range<usize>, source_len: usize) -> bool {
    outer.start <= outer.end
        && outer.end <= source_len
        && inner.start <= inner.end
        && inner.end <= source_len
        && outer.start <= inner.start
        && inner.end <= outer.end
}

/// Retains the last structurally acknowledged SourceFacts base until the host
/// either commits the corresponding target publication or a newer source edit
/// supersedes that in-flight transaction.
///
/// The target root remains in `DocumentRuntime::persistent_source_facts` so
/// parser work can continue against the current source. The base root is a
/// second persistent owner over the structurally shared tree, not a copied
/// checkpoint/page collection.
struct PendingPersistentSourceFactsDelta {
    serial: u64,
    base: PersistentSourceFactsRoot,
    target: SourceVersion,
    target_root_authority: PersistentSourceFactsRootAuthoritySnapshot,
}

/// Progress from the one runtime-owned source-fact job.
#[derive(Debug, Eq, PartialEq)]
pub enum RuntimeSourceFactsPoll {
    Pending(SourceFactsWork),
    /// One bounded actor-owned persistent-index promotion transition advanced.
    ///
    /// This is distinct from scanner work so an off-caller scheduler can keep
    /// polling after scanning finishes without treating an all-zero scanner
    /// receipt as quiescence.
    PromotionPending {
        transitions: usize,
    },
    /// Clean scanning and certification completed, but the actor-owned
    /// persistent index has not yet been promoted and installed.
    ScanComplete {
        completion: SourceFactsCompletion,
        work: SourceFactsWork,
    },
    IncrementalScanComplete {
        source: SourceVersion,
        byte_start: usize,
        byte_end: usize,
        work: SourceFactsWork,
    },
    Complete {
        completion: SourceFactsCompletion,
        work: SourceFactsWork,
    },
    IncrementalComplete {
        source: SourceVersion,
        work: PersistentSourceFactsWork,
        witness: Box<PersistentSourceFactsDeltaWitness>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncrementalSourceFactsPlan {
    source: SourceVersion,
    base: SourceVersion,
    base_page_range: Range<u64>,
    base_byte_range: Range<usize>,
    target_byte_range: Range<usize>,
    exact_parser_edit_envelope: Option<ExactParserEditEnvelope>,
    lineage_transitions: usize,
    planning_work: PersistentSourceFactsWork,
}

impl IncrementalSourceFactsPlan {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn base(&self) -> SourceVersion {
        self.base
    }

    #[must_use]
    pub const fn base_page_range(&self) -> &Range<u64> {
        &self.base_page_range
    }

    #[must_use]
    pub const fn base_byte_range(&self) -> &Range<usize> {
        &self.base_byte_range
    }

    #[must_use]
    pub const fn target_byte_range(&self) -> &Range<usize> {
        &self.target_byte_range
    }

    /// Returns the exact edit envelope in the persistent parser's base source.
    ///
    /// This is narrower than the page-aligned SourceFacts byte range whenever
    /// every retained edit remains inside or touches the original edit
    /// envelope. `None` requires the parser to use its existing wider or clean
    /// fallback lane.
    #[must_use]
    pub fn exact_parser_base_byte_range(&self) -> Option<&Range<usize>> {
        self.exact_parser_edit_envelope
            .as_ref()
            .map(|envelope| &envelope.base_byte_range)
    }

    /// Returns the mapped exact edit envelope in the current target source.
    #[must_use]
    pub fn exact_parser_target_byte_range(&self) -> Option<&Range<usize>> {
        self.exact_parser_edit_envelope
            .as_ref()
            .map(|envelope| &envelope.target_byte_range)
    }

    #[must_use]
    pub const fn lineage_transitions(&self) -> usize {
        self.lineage_transitions
    }

    #[must_use]
    pub const fn planning_work(&self) -> PersistentSourceFactsWork {
        self.planning_work
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
    runtime_identity: StrongIdentity,
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
    runtime_identity: StrongIdentity,
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

/// Cumulative scanner work bound into one incremental SourceFacts delta.
///
/// This is distinct from [`SourceFactsWork`], which describes only one
/// bounded poll. Every counter here is checked while the runtime accumulates
/// the complete cropped replacement scan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PersistentSourceFactsDeltaScanWork {
    source_bytes_examined: usize,
    source_bytes_buffered: usize,
    cursor_refills: usize,
    cursor_copy_bytes_upper_bound: usize,
    checkpoints_emitted: usize,
}

impl PersistentSourceFactsDeltaScanWork {
    fn checked_add_poll(self, poll: SourceFactsWork) -> Option<Self> {
        Some(Self {
            source_bytes_examined: self
                .source_bytes_examined
                .checked_add(poll.source_bytes_examined())?,
            source_bytes_buffered: self
                .source_bytes_buffered
                .checked_add(poll.source_bytes_buffered())?,
            cursor_refills: self.cursor_refills.checked_add(poll.cursor_refills())?,
            cursor_copy_bytes_upper_bound: self
                .cursor_copy_bytes_upper_bound
                .checked_add(poll.cursor_copy_bytes_upper_bound())?,
            checkpoints_emitted: self
                .checkpoints_emitted
                .checked_add(poll.checkpoints_emitted())?,
        })
    }

    #[must_use]
    pub const fn source_bytes_examined(self) -> usize {
        self.source_bytes_examined
    }

    #[must_use]
    pub const fn source_bytes_buffered(self) -> usize {
        self.source_bytes_buffered
    }

    #[must_use]
    pub const fn cursor_refills(self) -> usize {
        self.cursor_refills
    }

    #[must_use]
    pub const fn cursor_copy_bytes_upper_bound(self) -> usize {
        self.cursor_copy_bytes_upper_bound
    }

    #[must_use]
    pub const fn checkpoints_emitted(self) -> usize {
        self.checkpoints_emitted
    }
}

/// Opaque target-root identity and ordered commitment.
///
/// Equality compares both the persistent tree identity and its independently
/// composed commitment. The underlying arena handle is intentionally never
/// exposed: this value is meaningful only as one field of a runtime-bound
/// delta witness.
#[derive(Eq, PartialEq)]
pub struct PersistentSourceFactsDeltaRootAuthority(PersistentSourceFactsRootAuthoritySnapshot);

impl fmt::Debug for PersistentSourceFactsDeltaRootAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PersistentSourceFactsDeltaRootAuthority(..)")
    }
}

/// Move-only proof that one exact persistent SourceFacts splice completed.
///
/// The witness owns no source text, arena node, prior role wrapper, or prior
/// manifest. Its originating [`DocumentRuntime`] transactionally retains the
/// structurally acknowledged base root until the target host publication
/// commits or a newer edit supersedes it. The witness can be handed back
/// exactly once via [`DocumentRuntime::take_persistent_source_facts_delta`];
/// that consuming handoff rechecks every authority field against both the
/// runtime-owned target and retained base.
///
/// ```compile_fail
/// use flark_engine::PersistentSourceFactsDeltaWitness;
///
/// fn duplicate(witness: PersistentSourceFactsDeltaWitness) {
///     let _copy = witness.clone();
/// }
/// ```
#[must_use = "a SourceFacts delta witness must be consumed or deliberately dropped"]
#[derive(Eq, PartialEq)]
pub struct PersistentSourceFactsDeltaWitness {
    runtime_identity: StrongIdentity,
    serial: u64,
    base: SourceVersion,
    target: SourceVersion,
    parser_profile: ParserProfileId,
    profile: SourceFactsScanProfile,
    base_page_range: Range<u64>,
    base_page_count: u64,
    base_replacement_checkpoint_count: u64,
    target_page_range: Range<u64>,
    target_replacement_checkpoint_count: u64,
    base_byte_range: Range<usize>,
    target_byte_range: Range<usize>,
    exact_parser_edit_envelope: Option<ExactParserEditEnvelope>,
    lineage_transitions: usize,
    target_root_authority: PersistentSourceFactsDeltaRootAuthority,
    planning_work: PersistentSourceFactsWork,
    scan_work: PersistentSourceFactsDeltaScanWork,
    replacement_work: PersistentSourceFactsWork,
    splice_work: PersistentSourceFactsWork,
}

impl fmt::Debug for PersistentSourceFactsDeltaWitness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentSourceFactsDeltaWitness")
            .field("base", &self.base)
            .field("target", &self.target)
            .field("parser_profile", &self.parser_profile)
            .field("profile", &self.profile)
            .field("base_page_range", &self.base_page_range)
            .field("base_page_count", &self.base_page_count)
            .field(
                "base_replacement_checkpoint_count",
                &self.base_replacement_checkpoint_count,
            )
            .field("target_page_range", &self.target_page_range)
            .field(
                "target_replacement_checkpoint_count",
                &self.target_replacement_checkpoint_count,
            )
            .field("base_byte_range", &self.base_byte_range)
            .field("target_byte_range", &self.target_byte_range)
            .field(
                "exact_parser_edit_envelope",
                &self.exact_parser_edit_envelope,
            )
            .field("lineage_transitions", &self.lineage_transitions)
            .field("target_root_authority", &self.target_root_authority)
            .field("planning_work", &self.planning_work)
            .field("scan_work", &self.scan_work)
            .field("replacement_work", &self.replacement_work)
            .field("splice_work", &self.splice_work)
            .finish()
    }
}

impl PersistentSourceFactsDeltaWitness {
    #[must_use]
    pub const fn base(&self) -> SourceVersion {
        self.base
    }

    #[must_use]
    pub const fn target(&self) -> SourceVersion {
        self.target
    }

    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn profile(&self) -> SourceFactsScanProfile {
        self.profile
    }

    #[must_use]
    pub const fn base_page_range(&self) -> &Range<u64> {
        &self.base_page_range
    }

    #[must_use]
    pub const fn base_page_count(&self) -> u64 {
        self.base_page_count
    }

    #[must_use]
    pub const fn base_replacement_checkpoint_count(&self) -> u64 {
        self.base_replacement_checkpoint_count
    }

    #[must_use]
    pub const fn target_page_range(&self) -> &Range<u64> {
        &self.target_page_range
    }

    #[must_use]
    pub const fn target_replacement_checkpoint_count(&self) -> u64 {
        self.target_replacement_checkpoint_count
    }

    #[must_use]
    pub const fn base_byte_range(&self) -> &Range<usize> {
        &self.base_byte_range
    }

    #[must_use]
    pub const fn target_byte_range(&self) -> &Range<usize> {
        &self.target_byte_range
    }

    /// Returns the exact edit envelope in the persistent parser's base source.
    #[must_use]
    pub fn exact_parser_base_byte_range(&self) -> Option<&Range<usize>> {
        self.exact_parser_edit_envelope
            .as_ref()
            .map(|envelope| &envelope.base_byte_range)
    }

    /// Returns the mapped exact edit envelope in the current target source.
    #[must_use]
    pub fn exact_parser_target_byte_range(&self) -> Option<&Range<usize>> {
        self.exact_parser_edit_envelope
            .as_ref()
            .map(|envelope| &envelope.target_byte_range)
    }

    #[must_use]
    pub const fn lineage_transitions(&self) -> usize {
        self.lineage_transitions
    }

    #[must_use]
    pub const fn target_root_authority(&self) -> &PersistentSourceFactsDeltaRootAuthority {
        &self.target_root_authority
    }

    #[must_use]
    pub const fn planning_work(&self) -> PersistentSourceFactsWork {
        self.planning_work
    }

    #[must_use]
    pub const fn scan_work(&self) -> PersistentSourceFactsDeltaScanWork {
        self.scan_work
    }

    #[must_use]
    pub const fn replacement_work(&self) -> PersistentSourceFactsWork {
        self.replacement_work
    }

    #[must_use]
    pub const fn splice_work(&self) -> PersistentSourceFactsWork {
        self.splice_work
    }
}

/// Observable authority and bounded clean-promotion work for the actor-owned
/// persistent SourceFacts index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentSourceFactsInfo {
    source: SourceVersion,
    parser_profile: ParserProfileId,
    coverage: crate::source_facts::SourceFactsCoverage,
    profile: SourceFactsScanProfile,
    summary: SourceFactSegmentSummary,
    page_count: u64,
    checkpoint_count: u64,
    checkpoint_root_guard128: [u32; 4],
    work: PersistentSourceFactsWork,
}

/// Move-only exact-source authority backed by the runtime's current
/// clean-EOF persistent SourceFacts root.
///
/// The persistent root remains actor-owned by [`DocumentRuntime`]. Candidate
/// construction revalidates and retains that root later; this capability owns
/// only the immutable source lease needed by a clean parser fallback plus the
/// profiles that were authenticated when it was minted.
#[must_use = "persistent source certification must be consumed or deliberately dropped"]
pub struct PersistentCertifiedSource {
    lease: SourceSnapshotLease,
    parser_profile: ParserProfileId,
    source_facts_profile: SourceFactsScanProfile,
}

impl fmt::Debug for PersistentCertifiedSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentCertifiedSource")
            .field("source", &self.lease.version())
            .field("parser_profile", &self.parser_profile)
            .field("source_facts_profile", &self.source_facts_profile)
            .finish()
    }
}

impl PersistentCertifiedSource {
    #[must_use]
    pub fn source(&self) -> SourceVersion {
        self.lease.version()
    }

    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn source_facts_profile(&self) -> SourceFactsScanProfile {
        self.source_facts_profile
    }

    /// Mints the immutable lease consumed by one exact clean parse.
    #[must_use]
    pub fn exact_parse_lease(&self) -> SourceSnapshotLease {
        self.lease.duplicate()
    }

    /// Transfers the exact lease and authenticated profiles to parser
    /// candidate derivation.
    #[must_use]
    pub fn into_parts(self) -> (SourceSnapshotLease, ParserProfileId, SourceFactsScanProfile) {
        (self.lease, self.parser_profile, self.source_facts_profile)
    }
}

/// One immutable persistent SourceFacts page projected for diagnostics and
/// bounded publication. The canonical page remains actor-owned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentSourceFactsPageInfo {
    id: crate::identity::ArenaId,
    ordinal: u64,
    content_digest: crate::source_facts::SourceFactPageDigest,
    checkpoint_count: usize,
    first_checkpoint: crate::source_facts::SourceFactCheckpoint,
    terminal_checkpoint: crate::source_facts::SourceFactCheckpoint,
    checkpoints: [SourceFactCheckpoint; SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX],
}

impl PersistentSourceFactsPageInfo {
    #[must_use]
    pub const fn id(self) -> crate::identity::ArenaId {
        self.id
    }

    #[must_use]
    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }

    #[must_use]
    pub const fn content_digest(self) -> crate::source_facts::SourceFactPageDigest {
        self.content_digest
    }

    #[must_use]
    pub const fn checkpoint_count(self) -> usize {
        self.checkpoint_count
    }

    #[must_use]
    pub const fn first_checkpoint(self) -> crate::source_facts::SourceFactCheckpoint {
        self.first_checkpoint
    }

    #[must_use]
    pub const fn terminal_checkpoint(self) -> crate::source_facts::SourceFactCheckpoint {
        self.terminal_checkpoint
    }

    /// Returns a bounded owned projection of every absolute checkpoint.
    #[must_use]
    pub fn checkpoints(&self) -> &[SourceFactCheckpoint] {
        &self.checkpoints[..self.checkpoint_count]
    }
}

impl PersistentSourceFactsInfo {
    #[must_use]
    pub const fn source(self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn parser_profile(self) -> ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    pub const fn coverage(self) -> crate::source_facts::SourceFactsCoverage {
        self.coverage
    }

    #[must_use]
    pub const fn profile(self) -> SourceFactsScanProfile {
        self.profile
    }

    #[must_use]
    pub const fn summary(self) -> SourceFactSegmentSummary {
        self.summary
    }

    #[must_use]
    pub const fn page_count(self) -> u64 {
        self.page_count
    }

    #[must_use]
    pub const fn checkpoint_count(self) -> u64 {
        self.checkpoint_count
    }

    /// Returns the v2 persistent checkpoint-root guard in wire lane order.
    #[must_use]
    pub const fn checkpoint_root_guard128(self) -> [u32; 4] {
        self.checkpoint_root_guard128
    }

    #[must_use]
    pub const fn work(self) -> PersistentSourceFactsWork {
        self.work
    }
}

/// A document lifecycle or admission failure.
#[derive(Debug)]
pub enum DocumentRuntimeError {
    InvalidConfig,
    AllocationFailed,
    NotOpen {
        state: DocumentState,
    },
    CandidateAlreadyActive,
    NoCandidatePlan,
    NoActiveCandidate,
    SourceFactsAlreadyActive,
    NoSourceFactsJob,
    SourceFactsAlreadyComplete,
    NoPersistentSourceFactsBase,
    IncrementalSourceFactsProfileMismatch,
    IncrementalSourceFactsLineageUnavailable,
    ExactUnchangedPrefixLineageUnavailable,
    ExactUnchangedPrefixForeignRuntime,
    ExactUnchangedPrefixStale,
    ExactUnchangedSuffixLineageUnavailable,
    ExactUnchangedSuffixForeignRuntime,
    ExactUnchangedSuffixStale,
    PersistentSourceFactsDeltaForeignRuntime,
    PersistentSourceFactsDeltaStale,
    PersistentSourceFactsDeltaAuthorityMismatch,
    SourceReadWindowTooLarge {
        observed: usize,
        limit: usize,
    },
    StaleCandidate {
        expected: CandidateGeneration,
        actual: CandidateGeneration,
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
    OpeningAppendBusy,
    #[cfg(feature = "progressive-source-probe")]
    OpeningSource(OpeningSourceError),
    Source(SourceEditError),
    SourceFacts(SourceFactsError),
    SourceFactsAssembly(SourceFactsAssemblyError),
    Arena(ArenaError),
}

impl fmt::Display for DocumentRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => formatter.write_str("document runtime configuration is invalid"),
            Self::AllocationFailed => formatter.write_str("document runtime allocation failed"),
            Self::NotOpen { state } => write!(formatter, "document is not open: {state:?}"),
            Self::CandidateAlreadyActive => {
                formatter.write_str("a parse candidate is already active")
            }
            Self::NoCandidatePlan => formatter.write_str("no parse candidate is planned"),
            Self::NoActiveCandidate => formatter.write_str("no parse candidate is active"),
            Self::SourceFactsAlreadyActive => {
                formatter.write_str("a source-fact job is already active")
            }
            Self::NoSourceFactsJob => formatter.write_str("no source-fact job is active"),
            Self::SourceFactsAlreadyComplete => {
                formatter.write_str("the source-fact job is already complete")
            }
            Self::NoPersistentSourceFactsBase => {
                formatter.write_str("no persistent source-fact base is available")
            }
            Self::IncrementalSourceFactsProfileMismatch => {
                formatter.write_str("persistent source-fact base uses a different profile")
            }
            Self::IncrementalSourceFactsLineageUnavailable => {
                formatter.write_str("exact source lineage is unavailable for incremental facts")
            }
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
            Self::PersistentSourceFactsDeltaForeignRuntime => {
                formatter.write_str("persistent SourceFacts delta belongs to another runtime")
            }
            Self::PersistentSourceFactsDeltaStale => {
                formatter.write_str("persistent SourceFacts delta is stale or already consumed")
            }
            Self::PersistentSourceFactsDeltaAuthorityMismatch => formatter.write_str(
                "persistent SourceFacts delta no longer matches the runtime-owned target",
            ),
            Self::SourceReadWindowTooLarge { observed, limit } => write!(
                formatter,
                "source read window has {observed} bytes but the limit is {limit}"
            ),
            Self::StaleCandidate { .. } => formatter.write_str("candidate generation is stale"),
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
            Self::IdentityExhausted => formatter.write_str("candidate identity space is exhausted"),
            #[cfg(feature = "progressive-source-probe")]
            Self::OpeningAppendBusy => formatter.write_str(
                "opening append cannot cross an active root-bound runtime job",
            ),
            #[cfg(feature = "progressive-source-probe")]
            Self::OpeningSource(error) => {
                write!(formatter, "opening source transition failed: {error}")
            }
            Self::Source(error) => write!(formatter, "source transition failed: {error}"),
            Self::SourceFacts(error) => write!(formatter, "source-fact scan failed: {error}"),
            Self::SourceFactsAssembly(error) => {
                write!(formatter, "source-fact assembly failed: {error}")
            }
            Self::Arena(error) => write!(formatter, "candidate storage failed: {error}"),
        }
    }
}

impl std::error::Error for DocumentRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(feature = "progressive-source-probe")]
            Self::OpeningSource(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::SourceFacts(error) => Some(error),
            Self::SourceFactsAssembly(error) => Some(error),
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

impl From<SourceEditLineageError> for DocumentRuntimeError {
    fn from(_: SourceEditLineageError) -> Self {
        Self::IncrementalSourceFactsLineageUnavailable
    }
}

impl From<SourceFactsError> for DocumentRuntimeError {
    fn from(error: SourceFactsError) -> Self {
        Self::SourceFacts(error)
    }
}

impl From<SourceFactsAssemblyError> for DocumentRuntimeError {
    fn from(error: SourceFactsAssemblyError) -> Self {
        Self::SourceFactsAssembly(error)
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

impl From<ManifestError> for DocumentRuntimeError {
    fn from(error: ManifestError) -> Self {
        match error {
            ManifestError::Arena(error) => Self::Arena(error),
            ManifestError::InvalidAuthority => Self::IdentityExhausted,
            _ => Self::InvalidConfig,
        }
    }
}

/// Receipt for an admitted document edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReceipt {
    source: SourceEditReceipt,
    superseded_active_candidate: bool,
    retired_source_leases: usize,
    retired_source_bytes: usize,
    latest_plan: ParsePlan,
}

impl EditReceipt {
    #[must_use]
    pub const fn source(&self) -> &SourceEditReceipt {
        &self.source
    }

    #[must_use]
    pub const fn superseded_active_candidate(&self) -> bool {
        self.superseded_active_candidate
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

    #[must_use]
    pub const fn latest_plan(&self) -> ParsePlan {
        self.latest_plan
    }
}

/// Receipt for an admitted atomic UTF-16 document edit intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16EditReceipt {
    source: SourceEditIntentReceipt,
    superseded_active_candidate: bool,
    retired_source_leases: usize,
    retired_source_bytes: usize,
    latest_plan: ParsePlan,
}

impl Utf16EditReceipt {
    /// Returns exact source-version and operation metrics from the commit.
    #[must_use]
    pub const fn source(&self) -> &SourceEditIntentReceipt {
        &self.source
    }

    /// Returns whether this edit superseded the one active parse candidate.
    #[must_use]
    pub const fn superseded_active_candidate(&self) -> bool {
        self.superseded_active_candidate
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

    /// Returns the one newest parse plan installed by this edit.
    #[must_use]
    pub const fn latest_plan(&self) -> ParsePlan {
        self.latest_plan
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

/// Owns the source/candidate lifecycle without owning parser semantics.
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
    source_facts_job: Option<RuntimeSourceFactsJob>,
    persistent_source_facts: Option<PersistentSourceFactsRoot>,
    pending_persistent_source_facts_delta: Option<PendingPersistentSourceFactsDelta>,
    live_source_facts_delta_serial: Option<u64>,
    last_source_facts_delta_serial: u64,
    active_candidate: Option<ActiveCandidate>,
    latest_plan: Option<ParsePlan>,
    last_generation: CandidateGeneration,
    retired_sources: VecDeque<SourceSnapshotLease>,
    retained_source_edit_lineages: VecDeque<SourceEditLineage>,
    max_retained_source_edit_lineages: usize,
    max_retired_sources: usize,
    retired_source_bytes: usize,
    max_retired_source_bytes: usize,
    arena: PageArena,
    document_identity: StrongIdentity,
    syntax_profile: u32,
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
                    && self.source_facts_job.is_none()
                    && self.persistent_source_facts.is_none()
                    && self.pending_persistent_source_facts_delta.is_none()
                    && self.live_source_facts_delta_serial.is_none()
                    && self.active_candidate.is_none()
                    && self.retired_sources.is_empty()
                    && self.retained_source_edit_lineages.is_empty()
                    && self.retired_source_bytes == 0
                    && arena.resident_nodes == 0
                    && arena.reserved_external_payload_bytes == 0
                    && arena.live_builds == 0,
                "DocumentRuntime must be explicitly closed and fuel-drained by its parser endpoint; \
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
            .field("source_facts_active", &self.source_facts_job.is_some())
            .field(
                "source_facts_delta_live",
                &self.live_source_facts_delta_serial.is_some(),
            )
            .field(
                "persistent_source_facts",
                &self
                    .persistent_source_facts
                    .as_ref()
                    .map(PersistentSourceFactsRoot::source),
            )
            .field(
                "pending_persistent_source_facts_delta",
                &self
                    .pending_persistent_source_facts_delta
                    .as_ref()
                    .map(|pending| (pending.base.source(), pending.target)),
            )
            .field("active_candidate", &self.active_candidate())
            .field("latest_plan", &self.latest_plan)
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
    /// Creates an open document with exactly one initial parse plan.
    pub fn new(text: &str, config: DocumentRuntimeConfig) -> Result<Self, DocumentRuntimeError> {
        Self::validate_initial_source(text.len(), config)?;
        let source = SourceStore::new(text)?;
        Self::from_validated_source_store(source, config)
    }

    /// Creates an open document around one already validated source replica.
    ///
    /// The store's exact externally assigned revision and immutable root become
    /// the source authority of the initial parse plan; this constructor never
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
    /// Root-bound candidate and source-fact jobs are rejected rather than
    /// silently rebound. The progressive compact-index builder is external to
    /// those jobs and consumes the returned receipt explicitly.
    #[cfg(feature = "progressive-source-probe")]
    pub fn adopt_opening_append(
        &mut self,
        proof: OpeningSourceAppendProof,
    ) -> Result<SourceAppendReceipt, DocumentRuntimeError> {
        self.ensure_open()?;
        if self.active_candidate.is_some()
            || self.source_facts_job.is_some()
            || self.persistent_source_facts.is_some()
            || self.pending_persistent_source_facts_delta.is_some()
        {
            return Err(DocumentRuntimeError::OpeningAppendBusy);
        }
        let current = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        if self.latest_plan.is_some_and(|plan| plan.source != current) {
            return Err(DocumentRuntimeError::OpeningAppendBusy);
        }
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
        if let Some(plan) = &mut self.latest_plan {
            debug_assert_eq!(plan.source, receipt.previous());
            plan.source = receipt.current();
        }
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
        let document_identity = StrongIdentity::allocate(b"document")?;
        let source_version = source.version();
        let initial_plan = ParsePlan {
            generation: CandidateGeneration::FIRST,
            source: source_version,
        };
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
            source_facts_job: None,
            persistent_source_facts: None,
            pending_persistent_source_facts_delta: None,
            live_source_facts_delta_serial: None,
            last_source_facts_delta_serial: 0,
            active_candidate: None,
            latest_plan: Some(initial_plan),
            last_generation: CandidateGeneration::FIRST,
            retired_sources,
            retained_source_edit_lineages,
            max_retained_source_edit_lineages: config.max_retained_source_edit_lineages,
            max_retired_sources: config.max_retired_sources,
            retired_source_bytes: 0,
            max_retired_source_bytes: config.max_retired_source_bytes,
            arena,
            document_identity,
            syntax_profile: 1,
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

    fn retained_lineage_transition_count(
        &self,
        base: SourceVersion,
        target: SourceVersion,
    ) -> Option<usize> {
        if base == target {
            return Some(0);
        }
        let mut current = base;
        for transitions in 1..=self.max_retained_source_edit_lineages {
            let lineage = self.retained_source_edit_lineage_after(current)?;
            current = lineage.current();
            if current == target {
                return Some(transitions);
            }
        }
        None
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

    /// Starts the one source-fact scan whose leases remain runtime-owned.
    pub fn begin_source_facts(
        &mut self,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        limits: SourceFactsRootLimits,
    ) -> Result<SourceVersion, DocumentRuntimeError> {
        self.ensure_open()?;
        if self.source_facts_job.is_some() {
            return Err(DocumentRuntimeError::SourceFactsAlreadyActive);
        }
        let source = self
            .source
            .as_ref()
            .expect("open documents always own a source");
        let version = source.version();
        let _ = SourceFactsRootAdmission::for_source(version, profile, limits)?;
        let builder_lease = source.snapshot();
        let scanner = SourceFactsScanner::with_profile(builder_lease.duplicate(), profile)?;
        let builder = SourceFactsRootBuilder::new(builder_lease, profile, parser_profile, limits)?;
        self.rollback_pending_persistent_source_facts_delta();
        self.invalidate_persistent_source_facts_delta();
        self.source_facts_job = Some(RuntimeSourceFactsJob {
            incremental: None,
            scanner: Some(scanner),
            builder: Some(builder),
            persistent: None,
            completion: None,
            certified: None,
        });
        Ok(version)
    }

    /// Starts a lineage-authenticated local crop scan against the current
    /// actor-owned persistent base.
    ///
    /// Absence, expiry, or profile mismatch is an explicit clean-fallback
    /// outcome; this method never guesses reuse from hashes or nearby text.
    pub fn begin_incremental_source_facts(
        &mut self,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        limits: SourceFactsRootLimits,
    ) -> Result<IncrementalSourceFactsPlan, DocumentRuntimeError> {
        self.ensure_open()?;
        if self.source_facts_job.is_some() {
            return Err(DocumentRuntimeError::SourceFactsAlreadyActive);
        }
        if self.pending_persistent_source_facts_delta.is_some() {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale);
        }
        let source = self
            .source
            .as_ref()
            .expect("open documents always own a source");
        let target = source.version();
        let base = self
            .persistent_source_facts
            .as_ref()
            .ok_or(DocumentRuntimeError::NoPersistentSourceFactsBase)?;
        if base.coverage() != SourceFactsCoverage::CleanEof
            || base.profile() != profile
            || base.parser_profile() != parser_profile
        {
            return Err(DocumentRuntimeError::IncrementalSourceFactsProfileMismatch);
        }
        if base.source() == target || base.source().byte_len() == 0 {
            return Err(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable);
        }
        let (edit_start, edit_end) = {
            let lineage = self
                .retained_source_edit_lineage_after(base.source())
                .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
            let first = lineage
                .spans()
                .first()
                .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
            let last = lineage
                .spans()
                .last()
                .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
            (first.old_bytes().start, last.old_bytes().end)
        };
        let old_len = base.source().byte_len();
        let restart_probe = if edit_start == old_len {
            old_len - 1
        } else {
            edit_start
        };
        let end_probe = if edit_end > edit_start {
            edit_end - 1
        } else {
            restart_probe
        };
        let mut planning_receipt = SequenceInspectionReceipt::default();
        let restart = base
            .locate_byte(
                &self.arena,
                u64::try_from(restart_probe)
                    .map_err(|_| DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?,
                &mut planning_receipt,
            )?
            .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
        let end = base
            .locate_byte(
                &self.arena,
                u64::try_from(end_probe)
                    .map_err(|_| DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?,
                &mut planning_receipt,
            )?
            .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
        let base_page_range = restart.ordinal
            ..end
                .ordinal
                .checked_add(1)
                .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
        let base_start = usize::try_from(restart.byte_start)
            .map_err(|_| DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
        let base_end = usize::try_from(end.byte_end)
            .map_err(|_| DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
        let base_byte_range = base_start..base_end;
        let mut target_start = base_start;
        let mut target_end = base_end;
        let mut exact_parser_edit_envelope =
            Some(ExactParserEditEnvelope::at_base(edit_start..edit_end));
        let mut lineage_previous = base.source();
        let mut lineage_transitions = 0_usize;
        loop {
            let lineage = self
                .retained_source_edit_lineage_after(lineage_previous)
                .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
            if lineage.spans().iter().any(|span| {
                span.old_bytes().start < target_start || span.old_bytes().end > target_end
            }) {
                return Err(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable);
            }
            let lineage_current = lineage.current();
            exact_parser_edit_envelope = exact_parser_edit_envelope.and_then(|envelope| {
                envelope.map_through(lineage, lineage_previous, lineage_current)
            });
            target_start = lineage.map_byte_boundary(
                lineage_previous,
                lineage_current,
                target_start,
                SourceBoundaryAffinity::Before,
            )?;
            target_end = lineage.map_byte_boundary(
                lineage_previous,
                lineage_current,
                target_end,
                SourceBoundaryAffinity::After,
            )?;
            lineage_transitions = lineage_transitions
                .checked_add(1)
                .ok_or(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)?;
            if lineage_current == target {
                break;
            }
            if lineage_transitions >= self.max_retained_source_edit_lineages {
                return Err(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable);
            }
            lineage_previous = lineage_current;
        }
        if target_start > target_end {
            return Err(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable);
        }
        let target_byte_range = target_start..target_end;
        let builder_lease = source.snapshot();
        let scanner = SourceFactsScanner::with_profile_range(
            builder_lease.duplicate(),
            profile,
            target_byte_range.clone(),
        )?;
        let builder = SourceFactsRootBuilder::new_range(
            builder_lease,
            target_byte_range.clone(),
            profile,
            parser_profile,
            limits,
        )?;
        let base_source = base.source();
        let base_page_count = base.page_count();
        let planning_work = PersistentSourceFactsWork::from_inspection(planning_receipt);
        let base = self
            .persistent_source_facts
            .take()
            .expect("incremental base presence was checked");
        let exact_parser_edit_envelope_preview = exact_parser_edit_envelope.clone();
        self.invalidate_persistent_source_facts_delta();
        self.source_facts_job = Some(RuntimeSourceFactsJob {
            incremental: Some(RuntimeIncrementalSourceFacts {
                base: Some(base),
                segment: None,
                base_source,
                parser_profile,
                profile,
                base_page_range: base_page_range.clone(),
                base_page_count,
                base_byte_range: base_byte_range.clone(),
                target_byte_range: target_byte_range.clone(),
                exact_parser_edit_envelope,
                target,
                lineage_transitions,
                planning_work,
                scan_work: PersistentSourceFactsDeltaScanWork::default(),
            }),
            scanner: Some(scanner),
            builder: Some(builder),
            persistent: None,
            completion: None,
            certified: None,
        });
        Ok(IncrementalSourceFactsPlan {
            source: target,
            base: base_source,
            base_page_range,
            base_byte_range,
            target_byte_range,
            exact_parser_edit_envelope: exact_parser_edit_envelope_preview,
            lineage_transitions,
            planning_work,
        })
    }

    /// Advances the runtime-owned scanner and assembler as one linear job.
    pub fn poll_source_facts(
        &mut self,
        maximum_source_bytes: usize,
        maximum_checkpoints: usize,
    ) -> Result<RuntimeSourceFactsPoll, DocumentRuntimeError> {
        self.ensure_open()?;
        let mut job = self
            .source_facts_job
            .take()
            .ok_or(DocumentRuntimeError::NoSourceFactsJob)?;
        if job.certified.is_some() {
            self.source_facts_job = Some(job);
            return Err(DocumentRuntimeError::SourceFactsAlreadyComplete);
        }
        let splice_ready = job
            .incremental
            .as_ref()
            .is_some_and(|incremental| incremental.segment.is_some());
        if splice_ready {
            let incremental = job
                .incremental
                .as_ref()
                .expect("splice-ready SourceFacts job is incremental");
            let segment_page_count = incremental
                .segment
                .as_ref()
                .expect("splice-ready SourceFacts job retains its segment")
                .page_count();
            let target_page_start = incremental.base_page_range.start;
            let Some(target_page_end) = target_page_start.checked_add(segment_page_count) else {
                self.source_facts_job = Some(job);
                return Err(SourceFactsAssemblyError::CounterExhausted.into());
            };
            let target_page_range = target_page_start..target_page_end;
            let Some(serial) = self.last_source_facts_delta_serial.checked_add(1) else {
                self.source_facts_job = Some(job);
                return Err(DocumentRuntimeError::IdentityExhausted);
            };
            if self.pending_persistent_source_facts_delta.is_some() {
                self.source_facts_job = Some(job);
                return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
            }
            let incremental = job
                .incremental
                .as_mut()
                .expect("splice-ready SourceFacts job is incremental");
            let base = incremental
                .base
                .take()
                .expect("incremental splice retains its base");
            let segment = incremental
                .segment
                .take()
                .expect("incremental splice retains its segment");
            let replacement_work = segment.work();
            let output = match splice_persistent_source_facts_atomic_with_receipt(
                &mut self.arena,
                &base,
                &segment,
                incremental.base_page_range.clone(),
                incremental.target,
            ) {
                Ok(output) => output,
                Err(error) => {
                    incremental.base = Some(base);
                    incremental.segment = Some(segment);
                    self.source_facts_job = Some(job);
                    return Err(error.into());
                }
            };
            let (updated, splice_work) = output.into_parts();
            let target_root_authority_snapshot = updated.authority_snapshot();
            let target_root_authority =
                PersistentSourceFactsDeltaRootAuthority(target_root_authority_snapshot);
            let source = updated.source();
            let work = updated.work();
            let base_replacement_checkpoint_count = base
                .checkpoint_count()
                .checked_add(segment.checkpoint_count())
                .and_then(|combined| combined.checked_sub(updated.checkpoint_count()));
            if target_page_range.end > updated.page_count()
                || base_replacement_checkpoint_count.is_none()
            {
                self.release_runtime_persistent_root(updated);
                incremental.base = Some(base);
                incremental.segment = Some(segment);
                self.source_facts_job = Some(job);
                return Err(SourceFactsAssemblyError::CanonicalSummaryMismatch.into());
            }
            let witness = PersistentSourceFactsDeltaWitness {
                runtime_identity: self.document_identity,
                serial,
                base: incremental.base_source,
                target: incremental.target,
                parser_profile: incremental.parser_profile,
                profile: incremental.profile,
                base_page_range: incremental.base_page_range.clone(),
                base_page_count: incremental.base_page_count,
                base_replacement_checkpoint_count: base_replacement_checkpoint_count
                    .expect("validated checkpoint delta arithmetic"),
                target_page_range,
                target_replacement_checkpoint_count: segment.checkpoint_count(),
                base_byte_range: incremental.base_byte_range.clone(),
                target_byte_range: incremental.target_byte_range.clone(),
                exact_parser_edit_envelope: incremental.exact_parser_edit_envelope.take(),
                lineage_transitions: incremental.lineage_transitions,
                target_root_authority,
                planning_work: incremental.planning_work,
                scan_work: incremental.scan_work,
                replacement_work,
                splice_work,
            };
            self.release_runtime_persistent_root(segment);
            assert!(
                self.persistent_source_facts.is_none(),
                "incremental SourceFacts target has one actor owner"
            );
            self.pending_persistent_source_facts_delta = Some(PendingPersistentSourceFactsDelta {
                serial,
                base,
                target: source,
                target_root_authority: target_root_authority_snapshot,
            });
            self.persistent_source_facts = Some(updated);
            self.last_source_facts_delta_serial = serial;
            self.live_source_facts_delta_serial = Some(serial);
            return Ok(RuntimeSourceFactsPoll::IncrementalComplete {
                source,
                work,
                witness: Box::new(witness),
            });
        }
        if job.persistent.is_some() {
            let persistent_poll = job
                .persistent
                .as_mut()
                .expect("persistent SourceFacts job was checked")
                .poll(&mut self.arena);
            let persistent_poll = match persistent_poll {
                Ok(poll) => poll,
                Err(error) => {
                    self.source_facts_job = Some(job);
                    return Err(error.into());
                }
            };
            return match persistent_poll {
                PersistentSourceFactsBuildPoll::Pending => {
                    self.source_facts_job = Some(job);
                    Ok(RuntimeSourceFactsPoll::PromotionPending { transitions: 1 })
                }
                PersistentSourceFactsBuildPoll::Complete(output) => {
                    let output = *output;
                    job.persistent = None;
                    match job.incremental.as_mut() {
                        None => {
                            let completion = job
                                .completion
                                .take()
                                .expect("persistent SourceFacts promotion retains scan completion");
                            self.install_persistent_source_facts(output.root);
                            job.certified = Some(output.certified);
                            self.source_facts_job = Some(job);
                            Ok(RuntimeSourceFactsPoll::Complete {
                                completion,
                                work: SourceFactsWork::default(),
                            })
                        }
                        Some(incremental) => {
                            debug_assert!(matches!(
                                output.certified.coverage(),
                                SourceFactsCoverage::ExactRange { .. }
                            ));
                            drop(output.certified);
                            incremental.segment = Some(output.root);
                            self.source_facts_job = Some(job);
                            Ok(RuntimeSourceFactsPoll::PromotionPending { transitions: 1 })
                        }
                    }
                }
            };
        }
        let scanner = job
            .scanner
            .as_mut()
            .ok_or(DocumentRuntimeError::SourceFactsAlreadyComplete)?;
        let poll = match scanner.poll(maximum_source_bytes, maximum_checkpoints) {
            Ok(poll) => poll,
            Err(error) => {
                self.source_facts_job = Some(job);
                return Err(error.into());
            }
        };
        let poll_work = match &poll {
            SourceFactsPoll::Pending(work)
            | SourceFactsPoll::Page { work, .. }
            | SourceFactsPoll::Complete { work, .. } => Some(*work),
            SourceFactsPoll::Cancelled => None,
        };
        if let (Some(incremental), Some(work)) = (job.incremental.as_mut(), poll_work) {
            let Some(scan_work) = incremental.scan_work.checked_add_poll(work) else {
                self.source_facts_job = Some(job);
                return Err(SourceFactsAssemblyError::CounterExhausted.into());
            };
            incremental.scan_work = scan_work;
        }
        match poll {
            SourceFactsPoll::Pending(work) => {
                self.source_facts_job = Some(job);
                Ok(RuntimeSourceFactsPoll::Pending(work))
            }
            SourceFactsPoll::Page { page, work } => {
                let pushed = job
                    .builder
                    .as_mut()
                    .expect("active scanner retains its SourceFacts builder")
                    .push_page(page);
                if let Err(error) = pushed {
                    self.source_facts_job = Some(job);
                    return Err(error.into());
                }
                self.source_facts_job = Some(job);
                Ok(RuntimeSourceFactsPoll::Pending(work))
            }
            SourceFactsPoll::Complete { completion, work } => {
                let builder = job
                    .builder
                    .take()
                    .expect("active scanner retains its SourceFacts builder");
                let certified = match job.incremental.as_ref() {
                    None => builder.certify(completion),
                    Some(_) => builder.finish_segment(completion),
                };
                let certified = match certified {
                    Ok(certified) => certified,
                    Err(error) => {
                        self.cancel_runtime_source_facts_job(job);
                        return Err(error.into());
                    }
                };
                job.scanner = None;
                job.completion = Some(completion);
                job.persistent = Some(PersistentSourceFactsBuild::new(certified));
                let result = match job.incremental.as_ref() {
                    None => RuntimeSourceFactsPoll::ScanComplete { completion, work },
                    Some(incremental) => RuntimeSourceFactsPoll::IncrementalScanComplete {
                        source: incremental.target,
                        byte_start: incremental.target_byte_range.start,
                        byte_end: incremental.target_byte_range.end,
                        work,
                    },
                };
                self.source_facts_job = Some(job);
                Ok(result)
            }
            SourceFactsPoll::Cancelled => {
                self.cancel_runtime_source_facts_job(job);
                Err(DocumentRuntimeError::NoSourceFactsJob)
            }
        }
    }

    /// Borrows completed facts without exposing or duplicating their lease.
    #[must_use]
    pub fn certified_source(&self) -> Option<&CertifiedSource> {
        self.source_facts_job
            .as_ref()
            .and_then(|job| job.certified.as_ref())
    }

    /// Returns the current actor-owned persistent SourceFacts authority.
    ///
    /// This survives transfer of the legacy `CertifiedSource` publication
    /// projection. During an uncommitted exact-base transaction it names the
    /// current target while the last host-acknowledged root remains privately
    /// retained for cancellation; after host commit it becomes the next base.
    #[must_use]
    pub fn persistent_source_facts(&self) -> Option<PersistentSourceFactsInfo> {
        self.persistent_source_facts
            .as_ref()
            .map(|root| PersistentSourceFactsInfo {
                source: root.source(),
                parser_profile: root.parser_profile(),
                coverage: root.coverage(),
                profile: root.profile(),
                summary: root.summary(),
                page_count: root.page_count(),
                checkpoint_count: root.checkpoint_count(),
                checkpoint_root_guard128: root.checkpoint_root_guard128(),
                work: root.work(),
            })
    }

    /// Authenticates the current immutable source against the actor-owned
    /// clean-EOF persistent SourceFacts root.
    ///
    /// This is the narrow clean-parser fallback seam after an incremental
    /// grammar crop declines to converge. It does not duplicate or transfer
    /// the persistent root; candidate construction must still revalidate and
    /// retain that root through this same runtime.
    pub fn certify_current_persistent_source(
        &self,
    ) -> Result<PersistentCertifiedSource, DocumentRuntimeError> {
        self.ensure_open()?;
        let source = self
            .source
            .as_ref()
            .expect("open documents always own a source");
        let root = self
            .persistent_source_facts
            .as_ref()
            .ok_or(DocumentRuntimeError::NoPersistentSourceFactsBase)?;
        let version = source.version();
        let summary = root.summary();
        if root.source() != version
            || root.coverage() != SourceFactsCoverage::CleanEof
            || summary.byte_len()
                != u64::try_from(version.byte_len()).map_err(|_| {
                    DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch
                })?
            || summary.utf16_len()
                != u64::try_from(version.utf16_len()).map_err(|_| {
                    DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch
                })?
        {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
        }
        Ok(PersistentCertifiedSource {
            lease: source.snapshot(),
            parser_profile: root.parser_profile(),
            source_facts_profile: root.profile(),
        })
    }

    /// Consumes and revalidates one runtime-minted persistent delta witness.
    ///
    /// Success is a one-use handoff into the in-flight exact-base transaction.
    /// Failure also consumes the presented witness. A foreign witness does not
    /// disturb this runtime's own eligible witness; a same-runtime stale or
    /// mismatched witness fails closed.
    pub fn take_persistent_source_facts_delta(
        &mut self,
        witness: Box<PersistentSourceFactsDeltaWitness>,
    ) -> Result<Box<PersistentSourceFactsDeltaWitness>, DocumentRuntimeError> {
        self.ensure_open()?;
        if witness.runtime_identity != self.document_identity {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaForeignRuntime);
        }
        let Some(live_serial) = self.live_source_facts_delta_serial else {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale);
        };
        if live_serial != witness.serial {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale);
        }

        // From this point every outcome consumes the one matching eligibility
        // slot. The immutable witness itself is also consumed by the call.
        self.live_source_facts_delta_serial = None;
        let current_source = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let lineage_transitions =
            self.retained_lineage_transition_count(witness.base, witness.target);
        let Some(root) = self.persistent_source_facts.as_ref() else {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
        };
        let Some(pending) = self.pending_persistent_source_facts_delta.as_ref() else {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
        };
        if current_source != witness.target
            || witness.base == witness.target
            || lineage_transitions != Some(witness.lineage_transitions)
            || pending.serial != witness.serial
            || pending.base.source() != witness.base
            || pending.target != witness.target
            || pending.target_root_authority != witness.target_root_authority.0
            || root.source() != witness.target
            || root.coverage() != SourceFactsCoverage::CleanEof
            || root.parser_profile() != witness.parser_profile
            || root.profile() != witness.profile
            || root.authority_snapshot() != witness.target_root_authority.0
            || witness.base_page_range.start > witness.base_page_range.end
            || witness.base_page_range.end > witness.base_page_count
            || witness.base_replacement_checkpoint_count > pending.base.checkpoint_count()
            || witness.target_page_range.start != witness.base_page_range.start
            || witness.target_page_range.start > witness.target_page_range.end
            || witness.target_page_range.end > root.page_count()
            || witness.target_replacement_checkpoint_count > root.checkpoint_count()
            || pending
                .base
                .checkpoint_count()
                .checked_sub(witness.base_replacement_checkpoint_count)
                .and_then(|retained| {
                    retained.checked_add(witness.target_replacement_checkpoint_count)
                })
                != Some(root.checkpoint_count())
            || witness.base_byte_range.start > witness.base_byte_range.end
            || witness.base_byte_range.end > witness.base.byte_len()
            || witness.target_byte_range.start > witness.target_byte_range.end
            || witness.target_byte_range.end > witness.target.byte_len()
            || witness
                .exact_parser_edit_envelope
                .as_ref()
                .is_some_and(|envelope| {
                    !envelope.is_valid_within(
                        witness.base,
                        witness.target,
                        &witness.base_byte_range,
                        &witness.target_byte_range,
                    )
                })
        {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
        }
        Ok(witness)
    }

    /// Commits the current persistent SourceFacts target after the structural
    /// host has atomically acknowledged that same target publication.
    ///
    /// Until this receipt arrives, the previous persistent root remains owned
    /// as the only honest base for cancellation or rapid supersession. A
    /// successful commit releases that base and makes the target root the base
    /// for the next edit. Full-snapshot deliveries may call this method too;
    /// `Ok(false)` means no SourceFacts delta transaction was pending.
    pub fn commit_persistent_source_facts_delta(
        &mut self,
        target: SourceVersion,
    ) -> Result<bool, DocumentRuntimeError> {
        self.ensure_open()?;
        let Some(pending) = self.pending_persistent_source_facts_delta.as_ref() else {
            return Ok(false);
        };
        let current_source = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let Some(root) = self.persistent_source_facts.as_ref() else {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
        };
        if target != current_source
            || pending.target != target
            || root.source() != target
            || root.authority_snapshot() != pending.target_root_authority
        {
            return Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch);
        }
        let pending = self
            .pending_persistent_source_facts_delta
            .take()
            .expect("pending SourceFacts transaction was validated");
        self.invalidate_persistent_source_facts_delta();
        self.release_runtime_persistent_root(pending.base);
        Ok(true)
    }

    /// Projects one persistent page without transferring its arena owner.
    pub fn persistent_source_facts_page(
        &self,
        ordinal: u64,
    ) -> Result<Option<PersistentSourceFactsPageInfo>, DocumentRuntimeError> {
        self.ensure_open()?;
        let Some(root) = self.persistent_source_facts.as_ref() else {
            return Ok(None);
        };
        let mut inspection = SequenceInspectionReceipt::default();
        let Some(id) = root.page_id(&self.arena, ordinal, &mut inspection)? else {
            return Ok(None);
        };
        let page = root
            .materialize_page(&self.arena, ordinal, &mut inspection)?
            .ok_or(SourceFactsAssemblyError::CanonicalSummaryMismatch)?;
        let checkpoints = page.checkpoints();
        let first_checkpoint = checkpoints
            .first()
            .copied()
            .ok_or(SourceFactsAssemblyError::CanonicalSummaryMismatch)?;
        let terminal_checkpoint = checkpoints
            .last()
            .copied()
            .ok_or(SourceFactsAssemblyError::CanonicalSummaryMismatch)?;
        let mut owned_checkpoints =
            [SourceFactCheckpoint::default(); SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX];
        owned_checkpoints[..checkpoints.len()].copy_from_slice(checkpoints);
        Ok(Some(PersistentSourceFactsPageInfo {
            id,
            ordinal,
            content_digest: page.content_digest(),
            checkpoint_count: checkpoints.len(),
            first_checkpoint,
            terminal_checkpoint,
            checkpoints: owned_checkpoints,
        }))
    }

    /// Transfers completed certification to the endpoint-owned exact parser.
    ///
    /// Incomplete source-fact work remains runtime-owned. A successful take
    /// removes the completed job so edit, close, and recovery have one obvious
    /// owner to cancel rather than two independently mutable derived lanes.
    pub fn take_certified_source(&mut self) -> Option<CertifiedSource> {
        let job = self.source_facts_job.as_mut()?;
        if job.scanner.is_some() || job.builder.is_some() {
            return None;
        }
        let certified = job.certified.take()?;
        self.source_facts_job = None;
        Some(certified)
    }

    /// Cancels scanning or releases completed certification before edit/close.
    pub fn cancel_source_facts(&mut self) -> bool {
        let Some(job) = self.source_facts_job.take() else {
            return false;
        };
        self.cancel_runtime_source_facts_job(job);
        true
    }

    fn cancel_runtime_source_facts_job(&mut self, mut job: RuntimeSourceFactsJob) {
        if let Some(persistent) = job.persistent.as_mut() {
            persistent
                .cancel(&mut self.arena)
                .expect("runtime-owned persistent SourceFacts build uses this arena");
        }
        if let Some(incremental) = job.incremental.as_mut() {
            if let Some(segment) = incremental.segment.take() {
                self.release_runtime_persistent_root(segment);
            }
            if let Some(base) = incremental.base.take() {
                assert!(
                    self.persistent_source_facts.is_none(),
                    "incremental SourceFacts base has one actor owner"
                );
                self.persistent_source_facts = Some(base);
            }
        }
    }

    /// Copies one scalar-aligned, bounded window without letting a source lease
    /// escape runtime ownership. This is intended for diagnostics and narrow
    /// adapters; full parsing consumes runtime-owned certification instead.
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
    pub const fn latest_plan(&self) -> Option<ParsePlan> {
        self.latest_plan
    }

    #[must_use]
    pub fn active_candidate(&self) -> Option<ActiveCandidateInfo> {
        self.active_candidate
            .as_ref()
            .map(|candidate| ActiveCandidateInfo {
                generation: candidate.plan.generation,
                source: candidate.plan.source,
            })
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

    /// Returns candidate-arena residency without exposing mutation.
    #[must_use]
    pub const fn arena_metrics(&self) -> ArenaMetrics {
        self.arena.metrics()
    }

    /// Borrows the one document-owned arena used by parser publications.
    ///
    /// Publication capabilities live outside the runtime state machine, but
    /// every read remains scoped to the document owner so no arena handle can
    /// escape into a long-lived producer object.
    pub(crate) const fn producer_arena(&self) -> &PageArena {
        &self.arena
    }

    /// Mutably borrows the one document-owned arena used by parser builds and
    /// explicit publication reclamation.
    pub(crate) fn producer_arena_mut(&mut self) -> &mut PageArena {
        &mut self.arena
    }

    /// Splits the parser-internal publication borrow across the arena and the
    /// current actor-owned persistent SourceFacts authority.
    ///
    /// The facts root remains immutably owned by this runtime while the
    /// candidate journal retains its measured root in the same arena.
    #[cfg(feature = "parser-internal")]
    pub(crate) fn producer_arena_and_persistent_source_facts(
        &mut self,
    ) -> (&mut PageArena, Option<&PersistentSourceFactsRoot>) {
        (&mut self.arena, self.persistent_source_facts.as_ref())
    }

    /// Stable capability identity used to reject publication work presented
    /// with a different document runtime after the arena borrow has ended.
    pub(crate) const fn producer_identity(&self) -> StrongIdentity {
        self.document_identity
    }

    /// Starts the latest plan, retaining exactly one candidate source lease.
    pub fn begin_candidate(&mut self) -> Result<ActiveCandidateInfo, DocumentRuntimeError> {
        self.ensure_open()?;
        if self.active_candidate.is_some() {
            return Err(DocumentRuntimeError::CandidateAlreadyActive);
        }
        let plan = self
            .latest_plan
            .ok_or(DocumentRuntimeError::NoCandidatePlan)?;
        let source = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .snapshot();
        let authority = CandidateAuthority::new(
            self.document_identity,
            StrongIdentity::allocate(b"publication")?,
            plan.source,
            plan.generation,
            self.syntax_profile,
        )?;
        // Actual parser records enter through a private controller endpoint in
        // the next slice. Empty canonical records keep this lifecycle join
        // schema-correct without pretending placeholder bytes are parse truth.
        let arena_limits = self.arena.limits();
        let manifest = CandidateManifestAssembler::new(
            &mut self.arena,
            authority,
            ReferenceRootLimits {
                arena: arena_limits,
                ..ReferenceRootLimits::default()
            },
            CanonicalRoleInputs::single(&[][..], &[][..], &[][..]),
        )?;
        self.latest_plan = None;
        self.active_candidate = Some(ActiveCandidate {
            plan,
            source,
            manifest,
        });
        Ok(ActiveCandidateInfo {
            generation: plan.generation,
            source: plan.source,
        })
    }

    /// Admits an edit, keeping only the newest candidate plan.
    pub fn apply_edit(
        &mut self,
        expected: SourceVersion,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<EditReceipt, DocumentRuntimeError> {
        self.ensure_open()?;
        let generation = self.next_generation()?;
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

        self.cancel_source_facts();
        let commit = self
            .source
            .as_mut()
            .expect("open documents always own a source")
            .commit_prepared_edit(prepared)?;
        self.rollback_pending_persistent_source_facts_delta();
        self.invalidate_persistent_source_facts_delta();
        let (source_receipt, retired_source, lineage) = commit.into_parts_with_lineage();
        self.retain_source_edit_lineage(lineage);
        let superseded_active_candidate = self.active_candidate.is_some();
        if let Some(candidate) = self.active_candidate.take() {
            self.retire_candidate(candidate);
        }
        self.enqueue_retired_source(retired_source);

        let latest_plan = ParsePlan {
            generation,
            source: source_receipt.current(),
        };
        self.last_generation = generation;
        self.latest_plan = Some(latest_plan);
        Ok(EditReceipt {
            source: source_receipt,
            superseded_active_candidate,
            retired_source_leases: retirement.leases,
            retired_source_bytes: retirement.bytes,
            latest_plan,
        })
    }

    /// Admits one atomic edit intent expressed in base-revision UTF-16 units.
    ///
    /// Source validation and target construction happen off-authority. The
    /// target-size and retirement budgets are then checked before the prepared
    /// root can become current, so every rejection leaves the source,
    /// candidate, and newest plan untouched.
    pub fn apply_utf16_edit_intent(
        &mut self,
        expected: SourceVersion,
        declared_revision: SourceRevision,
        operations: &[SourceUtf16Operation<'_>],
    ) -> Result<Utf16EditReceipt, DocumentRuntimeError> {
        self.ensure_open()?;
        let generation = self.next_generation()?;
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

        self.cancel_source_facts();
        let commit = self
            .source
            .as_mut()
            .expect("open documents always own a source")
            .commit_prepared_utf16_edit_intent(prepared)?;
        self.rollback_pending_persistent_source_facts_delta();
        self.invalidate_persistent_source_facts_delta();
        let (source_receipt, retired_source, lineage) = commit.into_parts_with_lineage();
        self.retain_source_edit_lineage(lineage);
        let superseded_active_candidate = self.active_candidate.is_some();
        if let Some(candidate) = self.active_candidate.take() {
            self.retire_candidate(candidate);
        }
        self.enqueue_retired_source(retired_source);

        let latest_plan = ParsePlan {
            generation,
            source: source_receipt.current(),
        };
        self.last_generation = generation;
        self.latest_plan = Some(latest_plan);
        Ok(Utf16EditReceipt {
            source: source_receipt,
            superseded_active_candidate,
            retired_source_leases: retirement.leases,
            retired_source_bytes: retirement.bytes,
            latest_plan,
        })
    }

    /// Retires the active attempt and plans a fresh attempt on current source.
    pub fn supersede_candidate(&mut self) -> Result<ParsePlan, DocumentRuntimeError> {
        self.ensure_open()?;
        if self.active_candidate.is_none() {
            return Err(DocumentRuntimeError::NoActiveCandidate);
        }
        let retirement = RetirementDemand {
            leases: 1,
            bytes: self
                .active_candidate
                .as_ref()
                .expect("candidate presence was checked")
                .source
                .version()
                .byte_len(),
        };
        self.ensure_retirement_capacity(retirement)?;
        let generation = self.next_generation()?;
        let source = self
            .source
            .as_ref()
            .expect("open documents always own a source")
            .version();
        let candidate = self
            .active_candidate
            .take()
            .expect("candidate presence was checked");
        self.rollback_pending_persistent_source_facts_delta();
        self.invalidate_persistent_source_facts_delta();
        self.retire_candidate(candidate);
        let plan = ParsePlan { generation, source };
        self.last_generation = generation;
        self.latest_plan = Some(plan);
        Ok(plan)
    }

    /// Ends and retires the named M1.0 attempt.
    ///
    /// This is cancellation-shaped until the parser supplies a typed manifest
    /// owner and an atomic publication transaction; it must not be mistaken for
    /// accepting parser output.
    pub fn complete_candidate(
        &mut self,
        generation: CandidateGeneration,
    ) -> Result<(), DocumentRuntimeError> {
        self.ensure_open()?;
        let active = self
            .active_candidate
            .as_ref()
            .ok_or(DocumentRuntimeError::NoActiveCandidate)?;
        if active.plan.generation != generation {
            return Err(DocumentRuntimeError::StaleCandidate {
                expected: generation,
                actual: active.plan.generation,
            });
        }
        let retirement = RetirementDemand {
            leases: 1,
            bytes: active.source.version().byte_len(),
        };
        self.ensure_retirement_capacity(retirement)?;
        let active = self
            .active_candidate
            .take()
            .expect("candidate presence was checked");
        self.retire_candidate(active);
        Ok(())
    }

    /// Transfers all live source leases into Closing. Repeated calls are no-ops.
    pub fn begin_close(&mut self) -> Result<bool, DocumentRuntimeError> {
        match self.state {
            DocumentState::Closed | DocumentState::Closing => return Ok(false),
            DocumentState::Open => {}
        }
        // Close is terminal and cannot admit more work, so its final one or two
        // leases use the pre-reserved close margin rather than becoming
        // impossible solely because the open-state backlog is at its cap.
        self.cancel_source_facts();
        self.release_persistent_source_facts();
        if let Some(candidate) = self.active_candidate.take() {
            self.retire_candidate(candidate);
        }
        let source = self
            .source
            .take()
            .expect("open documents always own a source")
            .into_snapshot();
        self.enqueue_retired_source(source);
        self.latest_plan = None;
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

    /// Drains superseded source and candidate storage in both Open and Closing.
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

    fn install_persistent_source_facts(&mut self, root: PersistentSourceFactsRoot) {
        assert!(
            self.pending_persistent_source_facts_delta.is_none(),
            "clean SourceFacts installation cannot overwrite an in-flight delta base"
        );
        self.invalidate_persistent_source_facts_delta();
        if let Some(previous) = self.persistent_source_facts.take() {
            self.release_runtime_persistent_root(previous);
        }
        self.persistent_source_facts = Some(root);
    }

    fn invalidate_persistent_source_facts_delta(&mut self) {
        self.live_source_facts_delta_serial = None;
    }

    fn release_runtime_persistent_root(&mut self, root: PersistentSourceFactsRoot) {
        if let Err(failure) = root.release(&mut self.arena) {
            let error = failure.error;
            let _root = failure.root;
            panic!(
                "runtime-owned persistent SourceFacts root rejected its arena: {}",
                error
            );
        }
    }

    /// Restores the last structurally acknowledged SourceFacts base after a
    /// newer source revision supersedes an uncommitted target.
    ///
    /// This is constant-sized ownership movement plus two arena-root reference
    /// transitions. It never clones or walks the persistent page tree.
    fn rollback_pending_persistent_source_facts_delta(&mut self) -> bool {
        let Some(pending) = self.pending_persistent_source_facts_delta.take() else {
            return false;
        };
        self.invalidate_persistent_source_facts_delta();
        let target = self
            .persistent_source_facts
            .take()
            .expect("pending SourceFacts delta retains its target root");
        assert_eq!(
            target.source(),
            pending.target,
            "pending SourceFacts target source must remain installed"
        );
        assert!(
            target.authority_snapshot() == pending.target_root_authority,
            "pending SourceFacts target authority must remain installed"
        );
        self.release_runtime_persistent_root(target);
        self.persistent_source_facts = Some(pending.base);
        true
    }

    fn release_persistent_source_facts(&mut self) {
        self.invalidate_persistent_source_facts_delta();
        if let Some(pending) = self.pending_persistent_source_facts_delta.take() {
            self.release_runtime_persistent_root(pending.base);
        }
        let Some(root) = self.persistent_source_facts.take() else {
            return;
        };
        self.release_runtime_persistent_root(root);
    }

    fn ensure_open(&self) -> Result<(), DocumentRuntimeError> {
        if self.state == DocumentState::Open {
            Ok(())
        } else {
            Err(DocumentRuntimeError::NotOpen { state: self.state })
        }
    }

    fn next_generation(&self) -> Result<CandidateGeneration, DocumentRuntimeError> {
        self.last_generation
            .checked_next()
            .ok_or(DocumentRuntimeError::IdentityExhausted)
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
            leases: 1 + usize::from(self.active_candidate.is_some()),
            bytes: current.byte_len().saturating_add(
                self.active_candidate
                    .as_ref()
                    .map_or(0, |candidate| candidate.source.version().byte_len()),
            ),
        }
    }

    fn retire_candidate(&mut self, candidate: ActiveCandidate) {
        let mut manifest = candidate.manifest;
        manifest
            .begin_abort(&mut self.arena)
            .expect("runtime-owned candidate manifests remain live in their arena");
        drop(manifest);
        self.enqueue_retired_source(candidate.source);
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
    use crate::measured_sequence::SequenceInspectionReceipt;

    fn close(mut runtime: DocumentRuntime) {
        if runtime.state() == DocumentState::Open {
            runtime.begin_close().expect("begin close");
        }
        while runtime.state() != DocumentState::Closed {
            runtime.poll_close(usize::MAX).expect("poll close");
        }
    }

    fn complete_clean_source_facts(
        runtime: &mut DocumentRuntime,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        limits: SourceFactsRootLimits,
    ) {
        runtime
            .begin_source_facts(profile, parser_profile, limits)
            .expect("begin clean SourceFacts");
        loop {
            match runtime
                .poll_source_facts(128, 64)
                .expect("bounded clean SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { .. } => break,
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean SourceFacts job reported incremental progress")
                }
            }
        }
    }

    fn complete_incremental_source_facts(
        runtime: &mut DocumentRuntime,
    ) -> (
        PersistentSourceFactsWork,
        Box<PersistentSourceFactsDeltaWitness>,
    ) {
        loop {
            match runtime
                .poll_source_facts(19, 5)
                .expect("bounded incremental SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
                RuntimeSourceFactsPoll::IncrementalComplete { work, witness, .. } => {
                    break (work, witness);
                }
                RuntimeSourceFactsPoll::ScanComplete { .. }
                | RuntimeSourceFactsPoll::Complete { .. } => {
                    panic!("incremental SourceFacts job reported clean progress")
                }
            }
        }
    }

    fn persistent_page_ids(runtime: &DocumentRuntime) -> Vec<crate::identity::ArenaId> {
        let root = runtime
            .persistent_source_facts
            .as_ref()
            .expect("persistent SourceFacts root");
        (0..root.page_count())
            .map(|ordinal| {
                root.page_id(
                    &runtime.arena,
                    ordinal,
                    &mut SequenceInspectionReceipt::default(),
                )
                .expect("locate persistent SourceFacts page")
                .expect("persistent SourceFacts page")
            })
            .collect()
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

    #[test]
    fn incremental_source_facts_preserve_untouched_page_identity() {
        let unit = "alpha **bold** 😀\r\nbeta\n";
        let source = unit.repeat(120);
        let profile = SourceFactsScanProfile::new(4).expect("source-fact profile");
        let parser_profile = ParserProfileId::new(23).expect("parser profile");
        let limits = SourceFactsRootLimits::default();
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        runtime
            .begin_source_facts(profile, parser_profile, limits)
            .expect("begin clean facts");
        loop {
            match runtime
                .poll_source_facts(128, 64)
                .expect("bounded clean SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { .. } => break,
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean SourceFacts job reported incremental progress")
                }
            }
        }
        let base_root = runtime
            .persistent_source_facts
            .as_ref()
            .expect("persistent clean base");
        let mut base_ids = Vec::new();
        for ordinal in 0..base_root.page_count() {
            base_ids.push(
                base_root
                    .page_id(
                        &runtime.arena,
                        ordinal,
                        &mut SequenceInspectionReceipt::default(),
                    )
                    .expect("locate base page")
                    .expect("base page"),
            );
        }
        drop(
            runtime
                .take_certified_source()
                .expect("release legacy projection"),
        );

        let edit_start = source
            .match_indices("**bold**")
            .nth(60)
            .expect("middle markdown occurrence")
            .0;
        let edit_end = edit_start + "**bold**".len();
        let target = runtime
            .apply_edit(
                runtime.current_source_version().expect("base source"),
                edit_start..edit_end,
                "**much stronger markdown 😀**",
            )
            .expect("variable-length edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(profile, parser_profile, limits)
            .expect("incremental plan");
        let (work, witness) = loop {
            match runtime
                .poll_source_facts(19, 5)
                .expect("bounded incremental SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
                RuntimeSourceFactsPoll::IncrementalComplete {
                    source,
                    work,
                    witness,
                } => {
                    assert_eq!(source, target);
                    break (work, witness);
                }
                RuntimeSourceFactsPoll::ScanComplete { .. }
                | RuntimeSourceFactsPoll::Complete { .. } => {
                    panic!("incremental SourceFacts job reported clean progress")
                }
            }
        };
        let witness = runtime
            .take_persistent_source_facts_delta(witness)
            .expect("consume exact incremental witness");
        let updated_root = runtime
            .persistent_source_facts
            .as_ref()
            .expect("incrementally updated root");
        let mut updated_ids = Vec::new();
        for ordinal in 0..updated_root.page_count() {
            updated_ids.push(
                updated_root
                    .page_id(
                        &runtime.arena,
                        ordinal,
                        &mut SequenceInspectionReceipt::default(),
                    )
                    .expect("locate updated page")
                    .expect("updated page"),
            );
        }

        let old_start = usize::try_from(plan.base_page_range.start).expect("old prefix page count");
        let old_end = usize::try_from(plan.base_page_range.end).expect("old suffix page start");
        let replacement_pages =
            usize::try_from(witness.target_page_range.end - witness.target_page_range.start)
                .expect("replacement page count");
        let new_suffix_start = old_start + replacement_pages;
        assert_eq!(&updated_ids[..old_start], &base_ids[..old_start]);
        assert_eq!(&updated_ids[new_suffix_start..], &base_ids[old_end..]);
        assert_eq!(work.leaves_reused(), base_ids.len() - (old_end - old_start));
        assert_eq!(
            work.committed_leaves_retained(),
            base_ids.len() + replacement_pages
        );

        close(runtime);
    }

    #[test]
    fn incremental_source_facts_cover_checkpoint_b_edit_shapes() {
        let mut text = "# heading\r\nalpha **bold** 😀 beta\n".repeat(140);
        let profile = SourceFactsScanProfile::new(4).expect("source-fact profile");
        let parser_profile = ParserProfileId::new(31).expect("parser profile");
        let limits = SourceFactsRootLimits::default();
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
        drop(
            runtime
                .take_certified_source()
                .expect("release legacy clean projection"),
        );

        for case in 0..4 {
            let range;
            let replacement;
            match case {
                0 => {
                    range = 0..0;
                    replacement = "<!-- prefix -->\n";
                }
                1 => {
                    let start = text
                        .match_indices('😀')
                        .nth(70)
                        .expect("middle Unicode scalar")
                        .0;
                    range = start..start + '😀'.len_utf8();
                    replacement = "🌍✨";
                }
                2 => {
                    range = text.len()..text.len();
                    replacement = "\n<!-- tail -->";
                }
                3 => {
                    let split = text
                        .match_indices("\r\n")
                        .nth(90)
                        .expect("split CRLF boundary")
                        .0
                        + 1;
                    range = split..split;
                    replacement = "X";
                }
                _ => unreachable!(),
            }

            let before = runtime
                .persistent_source_facts
                .as_ref()
                .expect("persistent base")
                .source();
            let before_ids = persistent_page_ids(&runtime);
            text.replace_range(range.clone(), replacement);
            let target = runtime
                .apply_edit(before, range, replacement)
                .expect("checkpoint edit")
                .source()
                .current();
            let plan = runtime
                .begin_incremental_source_facts(profile, parser_profile, limits)
                .expect("checkpoint incremental plan");
            assert_eq!(plan.base(), before);
            assert_eq!(plan.source(), target);
            assert_eq!(plan.lineage_transitions(), 1);
            let (work, witness) = complete_incremental_source_facts(&mut runtime);
            let witness = runtime
                .take_persistent_source_facts_delta(witness)
                .expect("consume checkpoint delta witness");
            assert!(runtime
                .commit_persistent_source_facts_delta(target)
                .expect("commit checkpoint target"));
            let after_ids = persistent_page_ids(&runtime);

            let old_start = usize::try_from(plan.base_page_range.start).expect("prefix page count");
            let old_end = usize::try_from(plan.base_page_range.end).expect("suffix page start");
            let replacement_pages =
                usize::try_from(witness.target_page_range.end - witness.target_page_range.start)
                    .expect("replacement page count");
            let new_suffix_start = old_start + replacement_pages;
            assert_eq!(&after_ids[..old_start], &before_ids[..old_start]);
            assert_eq!(&after_ids[new_suffix_start..], &before_ids[old_end..]);
            assert_eq!(
                work.leaves_reused(),
                before_ids.len() - (old_end - old_start)
            );
            assert_eq!(
                work.committed_leaves_retained(),
                before_ids.len() + replacement_pages
            );

            let mut oracle = DocumentRuntime::new(&text, DocumentRuntimeConfig::default())
                .expect("clean oracle");
            complete_clean_source_facts(&mut oracle, profile, parser_profile, limits);
            assert_eq!(
                runtime
                    .persistent_source_facts
                    .as_ref()
                    .expect("incremental facts")
                    .summary(),
                oracle
                    .persistent_source_facts
                    .as_ref()
                    .expect("clean facts")
                    .summary()
            );
            close(oracle);
            while !runtime.poll_retirement(1).complete {}
        }

        close(runtime);
    }

    #[test]
    fn repeated_incremental_source_facts_replan_from_underfilled_committed_topology() {
        const CHECKPOINT_SPACING: usize = 4;
        const BASE_CHECKPOINTS: u64 = 5 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64;
        const DELETED_CHECKPOINTS: u64 = 8;

        let mut text = "a".repeat(
            usize::try_from(BASE_CHECKPOINTS).expect("bounded checkpoint count")
                * CHECKPOINT_SPACING,
        );
        let profile = SourceFactsScanProfile::new(CHECKPOINT_SPACING).expect("source-fact profile");
        let parser_profile = ParserProfileId::new(37).expect("parser profile");
        let limits = SourceFactsRootLimits::default();
        let mut runtime =
            DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
        complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
        drop(
            runtime
                .take_certified_source()
                .expect("release legacy clean projection"),
        );

        let clean_base = runtime
            .persistent_source_facts()
            .expect("clean persistent base");
        assert_eq!(clean_base.page_count(), 5);
        assert_eq!(clean_base.checkpoint_count(), BASE_CHECKPOINTS);

        // Delete eight checkpoint intervals from the middle canonical page.
        // The committed target deliberately keeps the four untouched page
        // identities around one 56-checkpoint interior replacement page.
        let first_range = 600..632;
        let first_base = runtime.current_source_version().expect("first base");
        text.replace_range(first_range.clone(), "");
        let first_target = runtime
            .apply_edit(first_base, first_range, "")
            .expect("first middle edit")
            .source()
            .current();
        let first_plan = runtime
            .begin_incremental_source_facts(profile, parser_profile, limits)
            .expect("first incremental plan");
        assert_eq!(first_plan.base_page_range, 2..3);
        let (_, first_witness) = complete_incremental_source_facts(&mut runtime);
        let first_delta_guard = runtime
            .persistent_source_facts()
            .expect("first uncommitted persistent target")
            .checkpoint_root_guard128();
        let first_witness = runtime
            .take_persistent_source_facts_delta(first_witness)
            .expect("consume first persistent delta witness");
        assert_eq!(
            first_witness.base_replacement_checkpoint_count(),
            SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64
        );
        assert_eq!(
            first_witness.target_replacement_checkpoint_count(),
            SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64 - DELETED_CHECKPOINTS
        );
        assert!(runtime
            .commit_persistent_source_facts_delta(first_target)
            .expect("commit first persistent target"));
        assert_eq!(
            runtime
                .persistent_source_facts()
                .expect("first committed persistent target")
                .checkpoint_root_guard128(),
            first_delta_guard
        );
        while !runtime.poll_retirement(1).complete {}

        let first_page_checkpoint_counts = (0..5)
            .map(|ordinal| {
                runtime
                    .persistent_source_facts_page(ordinal)
                    .expect("inspect first target page")
                    .expect("first target page")
                    .checkpoint_count()
            })
            .collect::<Vec<_>>();
        assert_eq!(first_page_checkpoint_counts, [64, 64, 56, 64, 64]);
        let first_settled_metrics = runtime.arena_metrics();
        assert_eq!(first_settled_metrics.live_builds, 0);
        assert_eq!(first_settled_metrics.pending_build_aborts, 0);
        assert_eq!(first_settled_metrics.pending_reclaims, 0);

        // Replan from that committed, non-canonical physical partition and
        // replace one byte inside the underfilled page without changing its
        // checkpoint cardinality.
        let second_range = 620..621;
        let second_base = runtime.current_source_version().expect("second base");
        assert_eq!(second_base, first_target);
        text.replace_range(second_range.clone(), "b");
        let second_target = runtime
            .apply_edit(second_base, second_range, "b")
            .expect("second local edit")
            .source()
            .current();
        let second_plan = runtime
            .begin_incremental_source_facts(profile, parser_profile, limits)
            .expect("second incremental plan");
        assert_eq!(second_plan.base(), first_target);
        assert_eq!(second_plan.source(), second_target);
        assert_eq!(second_plan.base_page_range, 2..3);
        let (_, second_witness) = complete_incremental_source_facts(&mut runtime);
        let second_delta_guard = runtime
            .persistent_source_facts()
            .expect("second uncommitted persistent target")
            .checkpoint_root_guard128();
        let second_witness = runtime
            .take_persistent_source_facts_delta(second_witness)
            .expect("consume second persistent delta witness");
        assert_eq!(second_witness.base(), first_target);
        assert_eq!(second_witness.target(), second_target);
        assert_eq!(
            second_witness.base_replacement_checkpoint_count(),
            SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64 - DELETED_CHECKPOINTS
        );
        assert_eq!(
            second_witness.target_replacement_checkpoint_count(),
            SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64 - DELETED_CHECKPOINTS
        );
        assert!(runtime
            .commit_persistent_source_facts_delta(second_target)
            .expect("commit second persistent target"));
        assert_eq!(
            runtime
                .persistent_source_facts()
                .expect("second committed persistent target")
                .checkpoint_root_guard128(),
            second_delta_guard
        );
        while !runtime.poll_retirement(1).complete {}

        let incremental = runtime
            .persistent_source_facts()
            .expect("second persistent target");
        let mut incremental_checkpoints = Vec::new();
        for ordinal in 0..incremental.page_count() {
            incremental_checkpoints.extend_from_slice(
                runtime
                    .persistent_source_facts_page(ordinal)
                    .expect("inspect incremental final page")
                    .expect("incremental final page")
                    .checkpoints(),
            );
        }
        let mut oracle = DocumentRuntime::new(&text, DocumentRuntimeConfig::default())
            .expect("clean final oracle");
        complete_clean_source_facts(&mut oracle, profile, parser_profile, limits);
        let clean = oracle
            .persistent_source_facts()
            .expect("clean final persistent facts");
        let clean_fingerprint = oracle
            .certified_source()
            .expect("clean final certification")
            .facts()
            .fingerprint();
        let mut clean_checkpoints = Vec::new();
        for ordinal in 0..clean.page_count() {
            clean_checkpoints.extend_from_slice(
                oracle
                    .persistent_source_facts_page(ordinal)
                    .expect("inspect clean final page")
                    .expect("clean final page")
                    .checkpoints(),
            );
        }
        assert_eq!(incremental.coverage(), SourceFactsCoverage::CleanEof);
        assert_eq!(incremental.coverage(), clean.coverage());
        assert_eq!(incremental.summary(), clean.summary());
        assert_eq!(incremental.checkpoint_count(), clean.checkpoint_count());
        assert_eq!(incremental_checkpoints, clean_checkpoints);
        assert_eq!(incremental.page_count(), clean.page_count());
        assert_eq!(
            incremental.summary().byte_len(),
            clean_fingerprint.byte_len()
        );
        assert_eq!(
            incremental.summary().utf16_len(),
            clean_fingerprint.utf16_len()
        );
        assert_eq!(
            incremental.summary().rolling_hash(),
            clean_fingerprint.rolling_hash()
        );
        assert_eq!(
            incremental.profile().content_fingerprint_algorithm(),
            clean_fingerprint.algorithm()
        );

        let second_settled_metrics = runtime.arena_metrics();
        assert_eq!(second_settled_metrics, first_settled_metrics);

        close(oracle);
        close(runtime);
    }
}

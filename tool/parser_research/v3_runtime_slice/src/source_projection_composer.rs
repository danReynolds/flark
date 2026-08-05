//! Candidate-bound composition of exact source pieces into packed projection
//! runs.
//!
//! The authoritative source ledger owns classification. This module owns the
//! separate linear transition from those classified pieces to freshly
//! identified storage runs. No caller-provided metric or raw packed run can
//! stand in for either authority.

use std::{fmt, num::NonZeroU64};

#[cfg(feature = "exact-parser")]
use crate::ParentSelectedComposerCoverage;
#[cfg(feature = "exact-parser")]
use crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphCanonicalSurvivorMint;
#[cfg(feature = "exact-parser")]
use crate::serialized_green::setext_retained_restart::ParentSelectedCanonicalFragmentOriginSeed;
use crate::{
    ArenaBuildId, AtomicProjection, BlockId, CandidateAdoptedSourceSeal, CandidateAtomicProjection,
    CandidateSourceSeal, CanonicalFragmentReplacement, ConsumedSourcePiece, CoverageId,
    CoveragePart, FreshCoveragePermit, GreenComposerTailAdoptionAuthority,
    GreenSourceTailAdoptionCapability, LiveCandidateEpoch, LogicalContribution, ProjectionChunk,
    ProjectionChunkerFinish, ProjectionPiece, ProjectionProgramChunker, SerializedGreenError,
    SerializedGreenLeafCut, SerializedMetric, SourceProjectionRun, SourceSnapshotDescriptor,
    ValidatedLogicalKind,
};

/// Storage-owned acknowledgement that the exact build cut was accepted by the
/// packed-green sink and force-sealed into one build-local leaf barrier. It
/// becomes source-bound only when the composer consumes it alongside its
/// opaque candidate epoch.
///
/// The only production constructor consumes [`SerializedGreenLeafCut`], so a
/// caller cannot manufacture a source metric, event ordinal, or leaf ordinal.
/// The capability deliberately does *not* claim that the preceding Coverage
/// run carries a projection-reset marker. The eventual composite checkpoint
/// instead role-types this exact cut itself as a projection reset after the
/// parser pause, source-ledger continuation, dedicated checkpoint drain, and
/// packed-green cut have all been cross-checked. That avoids retroactively
/// mutating an already accepted Coverage run and avoids predecessor scans.
#[must_use = "the acknowledged green cut must be resumed or bound into its candidate checkpoint"]
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SourceProjectionLineBoundaryStorageAck {
    cut: SourceProjectionLineBoundaryCut,
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
enum SourceProjectionLineBoundaryCut {
    #[allow(dead_code)] // Wired only when CandidateWriter owns the composite cut.
    Green(SerializedGreenLeafCut),
    #[cfg(test)]
    MechanismOnly {
        build: ArenaBuildId,
        source_before: SerializedMetric,
        /// Zero is the test-only absence sentinel. Production green cuts and
        /// every selected checkpoint require a nonzero event ordinal.
        event_cut: u64,
    },
}

#[cfg_attr(not(test), allow(dead_code))]
impl SourceProjectionLineBoundaryStorageAck {
    /// Converts the green builder's exact, non-forgeable barrier result into
    /// the source-bound half of a line-boundary checkpoint. The cut remains
    /// owned by this capability; it is not reduced to caller-provided scalar
    /// coordinates.
    #[allow(dead_code)] // Typed CandidateWriter integration seam.
    pub(crate) fn from_green_cut(
        epoch: LiveCandidateEpoch,
        cut: SerializedGreenLeafCut,
    ) -> Result<Self, SourceProjectionComposerError> {
        if cut.build_id() != epoch.build_id() {
            return Err(SourceProjectionComposerError::WrongLineBoundaryStorage);
        }
        Ok(Self {
            cut: SourceProjectionLineBoundaryCut::Green(cut),
        })
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        match &self.cut {
            SourceProjectionLineBoundaryCut::Green(cut) => cut.build_id(),
            #[cfg(test)]
            SourceProjectionLineBoundaryCut::MechanismOnly { build, .. } => *build,
        }
    }

    pub(crate) const fn source_before(&self) -> SerializedMetric {
        match &self.cut {
            SourceProjectionLineBoundaryCut::Green(cut) => cut.source_before(),
            #[cfg(test)]
            SourceProjectionLineBoundaryCut::MechanismOnly { source_before, .. } => *source_before,
        }
    }

    /// Borrowed storage identity used only by the writer to decide whether an
    /// empty/no-new-event boundary can move the already-authorized exact cut
    /// through another same-build pause. No coordinate is copied out.
    #[allow(clippy::unnecessary_wraps)] // Test-only mechanism cuts deliberately carry no green authority.
    pub(crate) const fn green_cut(&self) -> Option<&SerializedGreenLeafCut> {
        match &self.cut {
            SourceProjectionLineBoundaryCut::Green(cut) => Some(cut),
            #[cfg(test)]
            SourceProjectionLineBoundaryCut::MechanismOnly { .. } => None,
        }
    }

    const fn event_cut(&self) -> Option<u64> {
        match &self.cut {
            SourceProjectionLineBoundaryCut::Green(cut) => Some(cut.events_before()),
            #[cfg(test)]
            SourceProjectionLineBoundaryCut::MechanismOnly { event_cut, .. } => {
                if *event_cut == 0 {
                    None
                } else {
                    Some(*event_cut)
                }
            }
        }
    }

    /// Returns the still-linear green cut to the writer after composer resume
    /// validation. Only the production variant can cross this seam.
    #[allow(dead_code)] // Typed CandidateWriter integration seam.
    pub(crate) fn into_green_cut(self) -> SerializedGreenLeafCut {
        match self.cut {
            SourceProjectionLineBoundaryCut::Green(cut) => cut,
            #[cfg(test)]
            SourceProjectionLineBoundaryCut::MechanismOnly { .. } => {
                panic!("mechanism-only line-boundary storage has no green cut")
            }
        }
    }

    #[cfg(test)]
    const fn mechanism_only(build: ArenaBuildId, source_before: SerializedMetric) -> Self {
        Self {
            cut: SourceProjectionLineBoundaryCut::MechanismOnly {
                build,
                source_before,
                event_cut: 0,
            },
        }
    }

    #[cfg(test)]
    const fn mechanism_only_at_event_cut(
        build: ArenaBuildId,
        source_before: SerializedMetric,
        event_cut: u64,
    ) -> Self {
        Self {
            cut: SourceProjectionLineBoundaryCut::MechanismOnly {
                build,
                source_before,
                event_cut,
            },
        }
    }
}

/// Small, same-build continuation of an empty projection composer at one
/// acknowledged line boundary.
///
/// There is no source payload, projection program, envelope encoder, pending
/// run, or growable scratch here. The token is intentionally neither `Clone`
/// nor `Copy`; successful or failed resume consumes it exactly once.
/// Compact private encoding of the cumulative coverage origin.
///
/// Zero means a byte-zero replay, `u64::MAX` means a suffix-local restart
/// which lacks cumulative authority, and every other nonzero value is the
/// authenticated selected-checkpoint run count. Constructors keep those
/// sentinels private while avoiding an extra enum discriminant word in every
/// line-boundary continuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ComposerCoverageOrigin(u64);

impl ComposerCoverageOrigin {
    const ZERO_RESTART: Self = Self(0);
    const SUFFIX_LOCAL_RESTART: Self = Self(u64::MAX);

    const fn selected_checkpoint(projection_runs: u64) -> Option<Self> {
        if projection_runs == 0 || projection_runs == u64::MAX {
            None
        } else {
            Some(Self(projection_runs))
        }
    }

    const fn checkpoint_prefix_projection_runs(self) -> Result<u64, SourceProjectionComposerError> {
        if self.0 == u64::MAX {
            Err(SourceProjectionComposerError::MissingCheckpointCoverage)
        } else {
            Ok(self.0)
        }
    }
}

#[must_use = "the line-boundary continuation must be resumed or discarded with its candidate"]
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SourceProjectionComposerLineBoundaryContinuation {
    epoch: LiveCandidateEpoch,
    next_fragment_generation: u64,
    issued_fragment_origin: Option<NonZeroU64>,
    receipt: SourceProjectionComposerReceipt,
    coverage_origin: ComposerCoverageOrigin,
    storage: SourceProjectionLineBoundaryStorageAck,
}

#[cfg_attr(not(test), allow(dead_code))]
impl SourceProjectionComposerLineBoundaryContinuation {
    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.storage.build_id()
    }

    pub(crate) const fn source_before(&self) -> SerializedMetric {
        self.storage.source_before()
    }

    /// Read-only receipt used by the composite join to prove that every
    /// composer run sealed before this cut was acknowledged by the packed
    /// green writer. The continuation remains the sole resume authority.
    pub(crate) const fn receipt(&self) -> SourceProjectionComposerReceipt {
        self.receipt
    }

    pub(crate) const fn green_cut(&self) -> Option<&SerializedGreenLeafCut> {
        self.storage.green_cut()
    }

    pub(crate) const fn checkpoint_prefix_projection_runs(
        &self,
    ) -> Result<u64, SourceProjectionComposerError> {
        self.coverage_origin.checkpoint_prefix_projection_runs()
    }

    /// Cumulative packed-green coverage at this paused line-boundary cut.
    /// The receipt is deliberately suffix-local, so a nonzero restart must
    /// add its authenticated parent checkpoint origin exactly once.
    pub(crate) fn cumulative_projection_runs(&self) -> Result<u64, SourceProjectionComposerError> {
        self.receipt
            .cumulative_projection_runs(self.checkpoint_prefix_projection_runs()?)
    }

    /// Consumes the real empty composer continuation after source adoption.
    ///
    /// A byte-zero restart has a zero checkpoint base. A parent-selected
    /// nonzero restart carries its authenticated cumulative base privately;
    /// the receipt itself always remains honest suffix-local work.
    pub(crate) fn seal_adopted_tail(
        self,
        source: CandidateAdoptedSourceSeal,
        tail: GreenComposerTailAdoptionAuthority,
    ) -> Result<SourceProjectionComposerTailAdoptionSeal, SourceProjectionComposerError> {
        let checkpoint_prefix_projection_runs = self.checkpoint_prefix_projection_runs()?;
        let prefix = tail.current_prefix();
        let final_metric = tail.final_metric();
        let source_prefix = source.accepted_projection_prefix_metric();
        let source_final = source.metric();
        // Guard the generation that this continuation would resume with. It
        // is exactly derived from the private sealed-run receipt and need not
        // be retained as an independently copied continuation field.
        self.receipt.projection_runs_sealed.checked_add(1).ok_or(
            SourceProjectionComposerError::Overflow("tail-adoption composer generation"),
        )?;
        if self.epoch != tail.epoch()
            || source.source() != self.epoch.source()
            || source.build_id() != self.epoch.build_id()
            || self.storage.build_id() != self.epoch.build_id()
            || self.storage.source_before() != prefix
            || source_prefix.bytes() != prefix.bytes
            || source_prefix.utf16() != prefix.utf16
            || source_final.bytes() != final_metric.bytes
            || source_final.utf16() != final_metric.utf16
            || source.replayed_source_piece_count() != self.receipt.source_pieces_consumed
        {
            return Err(SourceProjectionComposerError::TailAdoptionMismatch);
        }
        let replayed_prefix_projection_runs = self.receipt.canonical_projection_runs()?;
        let cumulative_prefix_projection_runs = checkpoint_prefix_projection_runs
            .checked_add(replayed_prefix_projection_runs)
            .ok_or(SourceProjectionComposerError::Overflow(
                "tail-adoption cumulative prefix projection runs",
            ))?;
        // The tail's prefix count belongs to old C. A valid edit between R and
        // C may change the number of current projection runs while leaving the
        // source/grammar/green suffix reusable. Rebase the authenticated old
        // suffix count onto the independently observed current prefix count.
        let adopted_suffix_projection_runs = tail.suffix_coverage_runs();
        let final_projection_runs = cumulative_prefix_projection_runs
            .checked_add(adopted_suffix_projection_runs)
            .ok_or(SourceProjectionComposerError::Overflow(
                "tail-adoption final projection runs",
            ))?;
        Ok(SourceProjectionComposerTailAdoptionSeal {
            source,
            prefix_receipt: self.receipt,
            checkpoint_prefix_projection_runs,
            replayed_prefix_projection_runs,
            cumulative_prefix_projection_runs,
            adopted_suffix_projection_runs,
            final_projection_runs,
            current_storage: self.storage,
            old_tail: tail.into_storage(),
        })
    }
}

#[cfg(test)]
impl SourceProjectionComposerLineBoundaryContinuation {
    #[allow(clippy::unused_self)] // Receipt is intentionally queried on the captured token.
    const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }

    #[allow(clippy::unused_self)] // Receipt is intentionally queried on the captured token.
    const fn retained_heap_bytes_for_test(&self) -> usize {
        0
    }
}

/// One source-bound, freshly identified run ready for the reset join and then
/// the green sink. It is intentionally non-cloneable and exposes no public raw
/// `SourceProjectionRun` extraction.
#[must_use = "the sealed run must enter the reset/green sink or be discarded with its candidate"]
pub struct ComposerSealedProjectionRunCapability {
    source: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    source_start: SerializedMetric,
    source_end: SerializedMetric,
    coverage: CoverageId,
    composer_generation: u64,
    run: SourceProjectionRun,
}

impl ComposerSealedProjectionRunCapability {
    #[must_use]
    pub const fn source(&self) -> SourceSnapshotDescriptor {
        self.source
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    #[must_use]
    pub const fn source_start(&self) -> SerializedMetric {
        self.source_start
    }

    #[must_use]
    pub const fn source_end(&self) -> SerializedMetric {
        self.source_end
    }

    #[must_use]
    pub const fn coverage_id(&self) -> CoverageId {
        self.coverage
    }

    #[must_use]
    pub const fn composer_generation(&self) -> u64 {
        self.composer_generation
    }

    /// The reset codec can only mark a run while preserving the same sealed
    /// source/composer authority. It cannot extract or replace the run.
    pub(crate) fn mark_projection_reset_after(mut self) -> Self {
        self.run.mark_projection_reset_after();
        self
    }

    /// Sole direct-sink transition. The raw run remains crate-private and is
    /// consumed immediately by `CandidateWriter`; parser-facing code never
    /// receives it.
    pub(crate) fn into_run(self) -> SourceProjectionRun {
        self.run
    }

    /// Unit-only construction for reset-codec falsification before the exact
    /// parser/composer is present in that test. Integration callers cannot
    /// mint this capability.
    #[cfg(test)]
    pub(crate) fn mechanism_only(
        source: SourceSnapshotDescriptor,
        build: ArenaBuildId,
        source_start: SerializedMetric,
        source_end: SerializedMetric,
        composer_generation: u64,
        run: SourceProjectionRun,
    ) -> Self {
        let coverage = run.id;
        debug_assert_ne!(composer_generation, 0);
        debug_assert_eq!(run.metric.bytes, source_end.bytes - source_start.bytes);
        debug_assert_eq!(run.metric.utf16, source_end.utf16 - source_start.utf16);
        Self {
            source,
            build,
            source_start,
            source_end,
            coverage,
            composer_generation,
            run,
        }
    }
}

impl fmt::Debug for ComposerSealedProjectionRunCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComposerSealedProjectionRunCapability")
            .field("source", &self.source)
            .field("build", &self.build)
            .field("source_start", &self.source_start)
            .field("source_end", &self.source_end)
            .field("coverage", &self.coverage)
            .field("composer_generation", &self.composer_generation)
            .finish_non_exhaustive()
    }
}

/// Linear source/composer coordinate captured when one provisional canonical
/// fragment opens. It authorizes no Green mutation by itself; only the same
/// live composer can consume it after all source in the fragment has drained.
#[must_use = "a canonical fragment origin must be replaced or retired with its Paragraph"]
#[derive(Debug)]
pub(crate) struct CanonicalFragmentProjectionOrigin {
    epoch: LiveCandidateEpoch,
    generation: u64,
    source_before: SerializedMetric,
    canonical_projection_runs_before: u64,
    parent_checkpoint: Option<ParentSelectedFragmentCheckpointOrigin>,
}

/// Opaque projection-authenticated partition of one visible-remainder
/// replacement. CandidateWriter can move this capability but cannot mint its
/// source cut or replacement-run partition from scalars.
#[cfg(feature = "exact-parser")]
#[must_use = "the survivor seed must be consumed while finishing its canonical rebase"]
#[derive(Debug)]
pub(crate) struct CanonicalFragmentSurvivorSeed {
    build: ArenaBuildId,
    old_generation: u64,
    source_before: SerializedMetric,
    replacement_prefix_runs: u64,
}

#[cfg(feature = "exact-parser")]
impl CanonicalFragmentSurvivorSeed {
    pub(crate) const fn from_active_paragraph_rewrite(
        _mint: ActiveParagraphCanonicalSurvivorMint,
        build: ArenaBuildId,
        old_generation: u64,
        source_before: SerializedMetric,
        replacement_prefix_runs: u64,
    ) -> Self {
        Self {
            build,
            old_generation,
            source_before,
            replacement_prefix_runs,
        }
    }
}

impl CanonicalFragmentProjectionOrigin {
    /// CandidateWriter-only generation used to bind an active-Paragraph
    /// projection cursor to this exact still-live fragment origin.
    pub(crate) const fn projection_generation(&self) -> u64 {
        self.generation
    }

    /// Exact physical source cut at the provisional Paragraph Enter.
    pub(crate) const fn source_before(&self) -> SerializedMetric {
        self.source_before
    }

    /// True only when the authenticated fragment Enter precedes the selected
    /// restart cut. CandidateWriter may use this to choose its hidden
    /// deferred-identity normalization route; copied source offsets cannot
    /// manufacture the predicate.
    pub(crate) const fn crosses_parent_selected_restart(&self) -> bool {
        match self.parent_checkpoint {
            Some(parent) => {
                self.source_before.bytes < parent.accepted_source_cut.bytes
                    || self.source_before.utf16 < parent.accepted_source_cut.utf16
            }
            None => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn mark_crossing_parent_selected_restart_for_test(
        &mut self,
    ) -> Result<(), SourceProjectionComposerError> {
        let accepted_source_cut = SerializedMetric {
            bytes: self.source_before.bytes.checked_add(1).ok_or(
                SourceProjectionComposerError::Overflow("test fragment source bytes"),
            )?,
            utf16: self.source_before.utf16.checked_add(1).ok_or(
                SourceProjectionComposerError::Overflow("test fragment source UTF-16"),
            )?,
        };
        self.parent_checkpoint = Some(ParentSelectedFragmentCheckpointOrigin {
            accepted_source_cut,
            accepted_projection_runs: self.canonical_projection_runs_before,
        });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParentSelectedFragmentCheckpointOrigin {
    accepted_source_cut: SerializedMetric,
    accepted_projection_runs: u64,
}

#[derive(Debug)]
struct ActiveCanonicalFragmentProjectionReplacement {
    generation: u64,
    source_start: SerializedMetric,
    source_end: SerializedMetric,
    canonical_projection_runs_before: u64,
    retired_projection_runs: u64,
    checkpoint_projection_runs_retired: u64,
    suffix_projection_runs_retired: u64,
    parent_checkpoint: Option<ParentSelectedFragmentCheckpointOrigin>,
}

/// Composer-side acknowledgement of one cardinality-changing canonical
/// fragment rebase. This remains distinct from packed-green storage authority
/// until CandidateWriter consumes both in the ledger rebind transaction.
#[must_use = "a composer fragment rebase must join packed green and the source ledger"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CanonicalFragmentProjectionRebase {
    epoch: LiveCandidateEpoch,
    generation: u64,
    source_start: SerializedMetric,
    source_end: SerializedMetric,
    retired_projection_runs: u64,
    installed_projection_runs: u64,
    canonical_suffix_projection_runs: u64,
}

impl CanonicalFragmentProjectionRebase {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.epoch.build_id()
    }

    pub(crate) const fn physical_metric(&self) -> SerializedMetric {
        SerializedMetric {
            bytes: self.source_end.bytes - self.source_start.bytes,
            utf16: self.source_end.utf16 - self.source_start.utf16,
        }
    }

    pub(crate) const fn retired_projection_runs(&self) -> u64 {
        self.retired_projection_runs
    }

    pub(crate) const fn installed_projection_runs(&self) -> u64 {
        self.installed_projection_runs
    }

    pub(crate) const fn canonical_suffix_projection_runs(&self) -> u64 {
        self.canonical_suffix_projection_runs
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceProjectionComposerReceipt {
    pub source_pieces_consumed: u64,
    pub projection_runs_sealed: u64,
    /// Runs removed from the current canonical Green stream by authenticated
    /// fragment replacement. This does not erase the historical seal-work
    /// count above.
    pub projection_runs_retired_by_normalization: u64,
    /// Runs removed from the authenticated retained checkpoint prefix by a
    /// replacement whose fragment began before the restart cut.
    pub checkpoint_projection_runs_retired_by_normalization: u64,
    /// Canonical replacement runs installed for the retired ranges.
    pub projection_runs_installed_by_normalization: u64,
    pub maximum_buffered_projection_bytes: usize,
    pub maximum_projection_buffer_capacity_bytes: usize,
    pub maximum_pending_source_pieces: usize,
    pub maximum_pending_runs: usize,
}

impl SourceProjectionComposerReceipt {
    pub(crate) fn canonical_projection_runs(self) -> Result<u64, SourceProjectionComposerError> {
        self.projection_runs_sealed
            .checked_sub(self.projection_runs_retired_by_normalization)
            .and_then(|runs| runs.checked_add(self.projection_runs_installed_by_normalization))
            .ok_or(SourceProjectionComposerError::Overflow(
                "canonical projection run count",
            ))
    }

    fn cumulative_projection_runs(
        self,
        checkpoint_prefix_projection_runs: u64,
    ) -> Result<u64, SourceProjectionComposerError> {
        checkpoint_prefix_projection_runs
            .checked_sub(self.checkpoint_projection_runs_retired_by_normalization)
            .and_then(|runs| runs.checked_add(self.canonical_projection_runs().ok()?))
            .ok_or(SourceProjectionComposerError::Overflow(
                "cumulative canonical projection run count",
            ))
    }
}

/// Non-cloneable proof that exact source EOF and every composer output belong
/// to the same candidate. It can only be extracted by consuming a completed
/// composer, so a copyable receipt cannot stand in for source completion.
#[must_use = "the composer completion seal must join the sole green build or be discarded"]
pub(crate) struct SourceProjectionComposerCompletionSeal {
    source: CandidateSourceSeal,
    receipt: SourceProjectionComposerReceipt,
    coverage_origin: ComposerCoverageOrigin,
}

impl SourceProjectionComposerCompletionSeal {
    #[must_use]
    pub(crate) const fn source(&self) -> SourceSnapshotDescriptor {
        self.source.source()
    }

    #[must_use]
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.source.build_id()
    }

    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn metric(&self) -> crate::SourceLedgerMetric {
        self.source.metric()
    }

    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn receipt(&self) -> SourceProjectionComposerReceipt {
        self.receipt
    }

    /// Authenticated cumulative projection coverage that existed before this
    /// composer began. The completion receipt remains suffix-local; callers
    /// must add this origin exactly once when comparing against cumulative
    /// packed-green coverage.
    #[must_use]
    pub(crate) const fn checkpoint_prefix_projection_runs(
        &self,
    ) -> Result<u64, SourceProjectionComposerError> {
        self.coverage_origin.checkpoint_prefix_projection_runs()
    }

    /// Cumulative packed-green coverage expected at EOF. This is the only
    /// completion operation that adds the authenticated checkpoint origin to
    /// the honest suffix-local receipt.
    pub(crate) fn cumulative_projection_runs(&self) -> Result<u64, SourceProjectionComposerError> {
        self.receipt
            .cumulative_projection_runs(self.checkpoint_prefix_projection_runs()?)
    }
}

impl fmt::Debug for SourceProjectionComposerCompletionSeal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceProjectionComposerCompletionSeal")
            .field("source", &self.source.source())
            .field("build", &self.source.build_id())
            .field("metric", &self.source.metric())
            .field("receipt", &self.receipt)
            .field("coverage_origin", &self.coverage_origin)
            .finish_non_exhaustive()
    }
}

/// Composer completion for a freshly replayed prefix plus a storage-adopted
/// unchanged suffix.
///
/// Unlike normal EOF completion, the source-piece and composer receipts remain
/// prefix-only. The final projection-run count is a separate, honest sum of
/// the current prefix runs and old-green suffix runs. This first mechanism
/// proof accepts only a composer that began at byte zero. A future nonzero
/// restart must consume the selected composite checkpoint's opaque five-axis
/// coverage base; there is deliberately no scalar constructor for that base.
#[must_use = "the composer tail seal must enter the matching candidate writer or be discarded"]
pub(crate) struct SourceProjectionComposerTailAdoptionSeal {
    source: CandidateAdoptedSourceSeal,
    prefix_receipt: SourceProjectionComposerReceipt,
    checkpoint_prefix_projection_runs: u64,
    replayed_prefix_projection_runs: u64,
    cumulative_prefix_projection_runs: u64,
    adopted_suffix_projection_runs: u64,
    final_projection_runs: u64,
    current_storage: SourceProjectionLineBoundaryStorageAck,
    old_tail: GreenSourceTailAdoptionCapability,
}

impl SourceProjectionComposerTailAdoptionSeal {
    pub(crate) const fn source(&self) -> SourceSnapshotDescriptor {
        self.source.source()
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.source.build_id()
    }

    pub(crate) const fn metric(&self) -> crate::SourceLedgerMetric {
        self.source.metric()
    }

    pub(crate) const fn accepted_projection_prefix_metric(&self) -> crate::SourceLedgerMetric {
        self.source.accepted_projection_prefix_metric()
    }

    pub(crate) const fn physical_parser_prefix_metric(&self) -> crate::SourceLedgerMetric {
        self.source.physical_parser_prefix_metric()
    }

    pub(crate) const fn replayed_prefix_source_pieces(&self) -> u64 {
        self.source.replayed_source_piece_count()
    }

    pub(crate) const fn prefix_receipt(&self) -> SourceProjectionComposerReceipt {
        self.prefix_receipt
    }

    pub(crate) const fn checkpoint_prefix_projection_runs(&self) -> u64 {
        self.checkpoint_prefix_projection_runs
    }

    pub(crate) const fn replayed_prefix_projection_runs(&self) -> u64 {
        self.replayed_prefix_projection_runs
    }

    pub(crate) const fn cumulative_prefix_projection_runs(&self) -> u64 {
        self.cumulative_prefix_projection_runs
    }

    pub(crate) const fn adopted_suffix_projection_runs(&self) -> u64 {
        self.adopted_suffix_projection_runs
    }

    pub(crate) const fn final_projection_runs(&self) -> u64 {
        self.final_projection_runs
    }

    pub(crate) const fn tail_adoption_receipt(&self) -> crate::GreenSourceTailAdoptionReceipt {
        self.old_tail.receipt()
    }

    pub(crate) const fn green_cut(&self) -> Option<&SerializedGreenLeafCut> {
        self.current_storage.green_cut()
    }

    pub(crate) fn into_green_storage_and_old_tail(
        self,
    ) -> (
        CandidateAdoptedSourceSeal,
        SourceProjectionLineBoundaryStorageAck,
        GreenSourceTailAdoptionCapability,
    ) {
        (self.source, self.current_storage, self.old_tail)
    }

    #[cfg(test)]
    pub(crate) const fn retained_source_bytes_for_test(&self) -> usize {
        0
    }
}

impl fmt::Debug for SourceProjectionComposerTailAdoptionSeal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceProjectionComposerTailAdoptionSeal")
            .field("source", &self.source)
            .field("prefix_receipt", &self.prefix_receipt)
            .field(
                "checkpoint_prefix_projection_runs",
                &self.checkpoint_prefix_projection_runs,
            )
            .field(
                "replayed_prefix_projection_runs",
                &self.replayed_prefix_projection_runs,
            )
            .field(
                "cumulative_prefix_projection_runs",
                &self.cumulative_prefix_projection_runs,
            )
            .field(
                "adopted_suffix_projection_runs",
                &self.adopted_suffix_projection_runs,
            )
            .field("final_projection_runs", &self.final_projection_runs)
            .field("current_storage", &self.current_storage)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProjectionComposerProgress {
    Idle,
    RunReady,
    Complete(SourceProjectionComposerReceipt),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceProjectionComposerError {
    WrongCandidate,
    OutOfOrderSource,
    InvalidSourcePiece,
    WrongCoveragePermit,
    WrongSourceSeal,
    TailAdoptionMismatch,
    MissingCheckpointCoverage,
    NoPendingRun,
    PendingRunMustBeSealed,
    StructuralFlushAlreadyStarted,
    LineBoundaryNotReady,
    LineBoundaryVirtualUnsafe,
    WrongLineBoundaryStorage,
    FragmentOriginNotReady,
    FragmentReplacementNotReady,
    WrongFragmentReplacement,
    CompletionNotReady,
    FinishAlreadyStarted,
    ComposerPoisoned,
    Overflow(&'static str),
    Codec(SerializedGreenError),
}

impl fmt::Display for SourceProjectionComposerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "source projection composer error: {self:?}")
    }
}

impl std::error::Error for SourceProjectionComposerError {}

impl From<SerializedGreenError> for SourceProjectionComposerError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Codec(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnvelopeKey {
    physical_owner: crate::source_bound_ledger::BindingStamp,
    owner_relative_depth: u32,
    structural_state_generation: u64,
    part: CoveragePart,
    logical_target: Option<(BlockId, u32)>,
}

#[derive(Debug)]
// The 4 KiB encoder allocation stays inline and is reused for the entire
// envelope. Boxing would add a hidden per-envelope heap allocation merely to
// shrink the physical-only discriminant.
#[allow(clippy::large_enum_variant)]
enum EnvelopeProjection {
    PhysicalOnly,
    Logical(ProjectionProgramChunker),
}

#[derive(Debug)]
struct OpenEnvelope {
    key: EnvelopeKey,
    source_start: SerializedMetric,
    source_end: SerializedMetric,
    next_chunk_start: SerializedMetric,
    projection: EnvelopeProjection,
}

#[derive(Debug)]
struct PendingRun {
    key: EnvelopeKey,
    source_start: SerializedMetric,
    source_end: SerializedMetric,
    logical_contribution: LogicalContribution,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StructuralFlushState {
    #[default]
    None,
    Requested,
    Ready,
}

/// The one candidate-owned transition from exact source pieces to packed run
/// capabilities. The constructor is crate-private; `LiveDocumentStore`
/// admits at most one instance for one candidate generation.
#[derive(Debug)]
pub struct SourceBoundProjectionComposer {
    epoch: LiveCandidateEpoch,
    next_composer_generation: u64,
    next_fragment_generation: u64,
    issued_fragment_origin: Option<NonZeroU64>,
    coverage_origin: ComposerCoverageOrigin,
    next_source: SerializedMetric,
    envelope: Option<OpenEnvelope>,
    pending_piece: Option<ConsumedSourcePiece>,
    pending_run: Option<PendingRun>,
    flushing: bool,
    structural_flush: StructuralFlushState,
    finish_requested: bool,
    completion_source_seal: Option<CandidateSourceSeal>,
    fragment_replacement: Option<ActiveCanonicalFragmentProjectionReplacement>,
    poisoned: bool,
    receipt: SourceProjectionComposerReceipt,
}

impl SourceBoundProjectionComposer {
    pub(crate) fn begin(epoch: LiveCandidateEpoch) -> Self {
        Self {
            epoch,
            next_composer_generation: 1,
            next_fragment_generation: 1,
            issued_fragment_origin: None,
            coverage_origin: ComposerCoverageOrigin::ZERO_RESTART,
            next_source: SerializedMetric::default(),
            envelope: None,
            pending_piece: None,
            pending_run: None,
            flushing: false,
            structural_flush: StructuralFlushState::None,
            finish_requested: false,
            completion_source_seal: None,
            fragment_replacement: None,
            poisoned: false,
            receipt: SourceProjectionComposerReceipt::default(),
        }
    }

    /// Starts a fresh-build composer at the accepted source cut restored by
    /// the packed-green Setext inverse. Cumulative source position survives,
    /// while composer generations and receipts are intentionally suffix-local.
    /// The exact green cut is returned for the writer's mandatory immediate
    /// same-build rejoin; no scalar accepted coordinate is caller supplied.
    #[allow(dead_code)] // Reached through the feasibility activation before product-root wiring.
    pub(crate) fn begin_retained_line_boundary(
        epoch: LiveCandidateEpoch,
        storage: SourceProjectionLineBoundaryStorageAck,
    ) -> Result<(Self, SourceProjectionLineBoundaryStorageAck), SourceProjectionComposerError> {
        if storage.build_id() != epoch.build_id() {
            return Err(SourceProjectionComposerError::WrongLineBoundaryStorage);
        }
        let next_source = storage.source_before();
        Ok((
            Self {
                epoch,
                next_composer_generation: 1,
                next_fragment_generation: 1,
                issued_fragment_origin: None,
                coverage_origin: ComposerCoverageOrigin::SUFFIX_LOCAL_RESTART,
                next_source,
                envelope: None,
                pending_piece: None,
                pending_run: None,
                flushing: false,
                structural_flush: StructuralFlushState::Ready,
                finish_requested: false,
                completion_source_seal: None,
                fragment_replacement: None,
                poisoned: false,
                receipt: SourceProjectionComposerReceipt::default(),
            },
            storage,
        ))
    }

    /// Starts at the exact nonzero cumulative coverage base selected from a
    /// committed composite parent and already consumed through source-lineage,
    /// normalized-current-path, and donor-resume validation.
    ///
    /// The returned composer still counts only work performed after this
    /// checkpoint. The opaque base is added exactly once by tail adoption.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn begin_parent_selected_line_boundary(
        epoch: LiveCandidateEpoch,
        storage: SourceProjectionLineBoundaryStorageAck,
        coverage: ParentSelectedComposerCoverage,
    ) -> Result<(Self, SourceProjectionLineBoundaryStorageAck), SourceProjectionComposerError> {
        if storage.build_id() != epoch.build_id()
            || coverage.epoch() != epoch
            || coverage.accepted_source() != storage.source_before()
            || coverage.event_cut() == 0
            || coverage.projection_runs() == 0
            || storage
                .event_cut()
                .is_some_and(|event_cut| event_cut != coverage.event_cut())
        {
            return Err(SourceProjectionComposerError::WrongLineBoundaryStorage);
        }
        let next_source = storage.source_before();
        let coverage_origin =
            ComposerCoverageOrigin::selected_checkpoint(coverage.projection_runs())
                .ok_or(SourceProjectionComposerError::WrongLineBoundaryStorage)?;
        Ok((
            Self {
                epoch,
                next_composer_generation: 1,
                next_fragment_generation: 1,
                issued_fragment_origin: None,
                coverage_origin,
                next_source,
                envelope: None,
                pending_piece: None,
                pending_run: None,
                flushing: false,
                structural_flush: StructuralFlushState::Ready,
                finish_requested: false,
                completion_source_seal: None,
                fragment_replacement: None,
                poisoned: false,
                receipt: SourceProjectionComposerReceipt::default(),
            },
            storage,
        ))
    }

    #[must_use]
    pub const fn receipt(&self) -> SourceProjectionComposerReceipt {
        self.receipt
    }

    /// Mints the one linear composer coordinate paired with a provisional
    /// Green fragment. The structural flush preceding the fragment Enter is
    /// what makes this cut exact without retaining any source or run vector.
    pub(crate) fn capture_canonical_fragment_origin(
        &mut self,
    ) -> Result<CanonicalFragmentProjectionOrigin, SourceProjectionComposerError> {
        self.require_live()?;
        if self.structural_flush != StructuralFlushState::Ready
            || self.envelope.is_some()
            || self.pending_piece.is_some()
            || self.pending_run.is_some()
            || self.flushing
            || self.finish_requested
            || self.completion_source_seal.is_some()
            || self.issued_fragment_origin.is_some()
            || self.fragment_replacement.is_some()
        {
            return self.fail(SourceProjectionComposerError::FragmentOriginNotReady);
        }
        let generation = self.next_fragment_generation;
        self.next_fragment_generation =
            generation
                .checked_add(1)
                .ok_or(SourceProjectionComposerError::Overflow(
                    "fragment origin generation",
                ))?;
        self.issued_fragment_origin = Some(NonZeroU64::new(generation).ok_or(
            SourceProjectionComposerError::Overflow("fragment origin generation"),
        )?);
        Ok(CanonicalFragmentProjectionOrigin {
            epoch: self.epoch,
            generation,
            source_before: self.next_source,
            canonical_projection_runs_before: self.receipt.canonical_projection_runs()?,
            parent_checkpoint: None,
        })
    }

    /// Restores the one fragment origin authenticated by the retained old
    /// manifest and already joined source/checkpoint coverage. This seam takes
    /// no caller coordinates and is valid only on a pristine parent-selected
    /// composer at its exact accepted cut.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn restore_parent_selected_canonical_fragment_origin(
        &mut self,
        seed: ParentSelectedCanonicalFragmentOriginSeed,
    ) -> Result<CanonicalFragmentProjectionOrigin, SourceProjectionComposerError> {
        self.require_live()?;
        let accepted_projection_runs = self.coverage_origin.checkpoint_prefix_projection_runs()?;
        if self.structural_flush != StructuralFlushState::Ready
            || self.envelope.is_some()
            || self.pending_piece.is_some()
            || self.pending_run.is_some()
            || self.flushing
            || self.finish_requested
            || self.completion_source_seal.is_some()
            || self.issued_fragment_origin.is_some()
            || self.fragment_replacement.is_some()
            || self.receipt != SourceProjectionComposerReceipt::default()
            || seed.build_id() != self.epoch.build_id()
            || seed.accepted_source_cut() != self.next_source
            || seed.accepted_projection_runs() != accepted_projection_runs
            || seed.source_before().bytes > self.next_source.bytes
            || seed.source_before().utf16 > self.next_source.utf16
            || seed.projection_runs_before() > accepted_projection_runs
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        let generation = self.next_fragment_generation;
        self.next_fragment_generation =
            generation
                .checked_add(1)
                .ok_or(SourceProjectionComposerError::Overflow(
                    "parent-selected fragment generation",
                ))?;
        self.issued_fragment_origin = NonZeroU64::new(generation);
        Ok(CanonicalFragmentProjectionOrigin {
            epoch: self.epoch,
            generation,
            source_before: seed.source_before(),
            canonical_projection_runs_before: seed.projection_runs_before(),
            parent_checkpoint: Some(ParentSelectedFragmentCheckpointOrigin {
                accepted_source_cut: seed.accepted_source_cut(),
                accepted_projection_runs,
            }),
        })
    }

    /// Retires the composer coordinate when the provisional fragment closes
    /// or retypes without changing projection cardinality.
    pub(crate) fn retire_canonical_fragment_origin(
        &mut self,
        origin: CanonicalFragmentProjectionOrigin,
    ) -> Result<(), SourceProjectionComposerError> {
        self.require_live()?;
        if origin.epoch != self.epoch
            || self.issued_fragment_origin != NonZeroU64::new(origin.generation)
            || self.fragment_replacement.is_some()
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        self.issued_fragment_origin = None;
        Ok(())
    }

    /// Consumes a fragment origin after all old source projections in that
    /// suffix have drained. Gross issuance counters stay monotonic; the
    /// cardinality rebase is applied only after packed green acknowledges it.
    pub(crate) fn begin_canonical_fragment_replacement(
        &mut self,
        origin: CanonicalFragmentProjectionOrigin,
        expected_physical: SerializedMetric,
    ) -> Result<(), SourceProjectionComposerError> {
        self.require_live()?;
        if self.structural_flush != StructuralFlushState::Ready
            || self.envelope.is_some()
            || self.pending_piece.is_some()
            || self.pending_run.is_some()
            || self.flushing
            || self.finish_requested
            || self.completion_source_seal.is_some()
            || self.fragment_replacement.is_some()
            || origin.epoch != self.epoch
            || self.issued_fragment_origin != NonZeroU64::new(origin.generation)
        {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        let physical = checked_sub_metric(self.next_source, origin.source_before)?;
        let canonical = self.receipt.canonical_projection_runs()?;
        let (
            canonical_projection_runs_before,
            checkpoint_projection_runs_retired,
            suffix_projection_runs_retired,
        ) = match origin.parent_checkpoint {
            Some(parent) => {
                if self.coverage_origin.checkpoint_prefix_projection_runs()?
                    != parent.accepted_projection_runs
                    || origin.source_before.bytes > parent.accepted_source_cut.bytes
                    || origin.source_before.utf16 > parent.accepted_source_cut.utf16
                    || origin.canonical_projection_runs_before > parent.accepted_projection_runs
                {
                    return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
                }
                (
                    0,
                    parent
                        .accepted_projection_runs
                        .checked_sub(origin.canonical_projection_runs_before)
                        .ok_or(SourceProjectionComposerError::WrongFragmentReplacement)?,
                    canonical,
                )
            }
            None => (
                origin.canonical_projection_runs_before,
                0,
                canonical
                    .checked_sub(origin.canonical_projection_runs_before)
                    .ok_or(SourceProjectionComposerError::WrongFragmentReplacement)?,
            ),
        };
        let retired_projection_runs = checkpoint_projection_runs_retired
            .checked_add(suffix_projection_runs_retired)
            .ok_or(SourceProjectionComposerError::Overflow(
                "restart-crossing retired projection runs",
            ))?;
        if physical != expected_physical
            || physical.bytes == 0
            || physical.utf16 == 0
            || retired_projection_runs == 0
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        self.issued_fragment_origin = None;
        self.fragment_replacement = Some(ActiveCanonicalFragmentProjectionReplacement {
            generation: origin.generation,
            source_start: origin.source_before,
            source_end: self.next_source,
            canonical_projection_runs_before,
            retired_projection_runs,
            checkpoint_projection_runs_retired,
            suffix_projection_runs_retired,
            parent_checkpoint: origin.parent_checkpoint,
        });
        Ok(())
    }

    /// Joins the composer source suffix with storage's independent Green
    /// replacement proof and rebases only canonical/net cardinality.
    pub(crate) fn finish_canonical_fragment_replacement(
        &mut self,
        storage: &CanonicalFragmentReplacement,
    ) -> Result<CanonicalFragmentProjectionRebase, SourceProjectionComposerError> {
        self.require_live()?;
        let replacement = self
            .fragment_replacement
            .take()
            .ok_or(SourceProjectionComposerError::FragmentReplacementNotReady)?;
        let physical = checked_sub_metric(replacement.source_end, replacement.source_start)?;
        if storage.build_id() != self.epoch.build_id()
            || storage.physical_metric() != physical
            || storage.retired_coverage_runs() != replacement.retired_projection_runs
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        let retired = self
            .receipt
            .projection_runs_retired_by_normalization
            .checked_add(replacement.suffix_projection_runs_retired)
            .ok_or(SourceProjectionComposerError::Overflow(
                "retired normalized projection runs",
            ))?;
        let checkpoint_retired = self
            .receipt
            .checkpoint_projection_runs_retired_by_normalization
            .checked_add(replacement.checkpoint_projection_runs_retired)
            .ok_or(SourceProjectionComposerError::Overflow(
                "retired checkpoint projection runs",
            ))?;
        let installed = self
            .receipt
            .projection_runs_installed_by_normalization
            .checked_add(storage.replacement_coverage_runs())
            .ok_or(SourceProjectionComposerError::Overflow(
                "installed normalized projection runs",
            ))?;
        self.receipt.projection_runs_retired_by_normalization = retired;
        self.receipt
            .checkpoint_projection_runs_retired_by_normalization = checkpoint_retired;
        self.receipt.projection_runs_installed_by_normalization = installed;
        let canonical_suffix_projection_runs = self.receipt.canonical_projection_runs()?;
        if canonical_suffix_projection_runs
            != replacement
                .canonical_projection_runs_before
                .checked_add(storage.replacement_coverage_runs())
                .ok_or(SourceProjectionComposerError::Overflow(
                    "replacement canonical projection runs",
                ))?
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        Ok(CanonicalFragmentProjectionRebase {
            epoch: self.epoch,
            generation: replacement.generation,
            source_start: replacement.source_start,
            source_end: replacement.source_end,
            retired_projection_runs: storage.retired_coverage_runs(),
            installed_projection_runs: storage.replacement_coverage_runs(),
            canonical_suffix_projection_runs,
        })
    }

    /// Atomically joins a visible-remainder storage replacement and mints the
    /// fresh origin for its surviving Paragraph. The new coordinate is derived
    /// from the old composer base plus the rewrite-owned prefix-run partition;
    /// no caller-authored source cut or run count is accepted.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn finish_canonical_fragment_replacement_with_survivor(
        &mut self,
        storage: &CanonicalFragmentReplacement,
        seed: CanonicalFragmentSurvivorSeed,
    ) -> Result<
        (
            CanonicalFragmentProjectionRebase,
            CanonicalFragmentProjectionOrigin,
        ),
        SourceProjectionComposerError,
    > {
        self.require_live()?;
        let Some(replacement) = self.fragment_replacement.as_ref() else {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        };
        let source_start = replacement.source_start;
        let source_end = replacement.source_end;
        let old_generation = replacement.generation;
        let canonical_projection_runs_before = replacement.canonical_projection_runs_before;
        let parent_checkpoint = replacement.parent_checkpoint;
        let survivor_after_retained_cut = parent_checkpoint.is_none_or(|parent| {
            seed.source_before.bytes >= parent.accepted_source_cut.bytes
                && seed.source_before.utf16 >= parent.accepted_source_cut.utf16
        });
        if self.issued_fragment_origin.is_some()
            || seed.build != self.epoch.build_id()
            || seed.old_generation != old_generation
            || seed.source_before.bytes <= source_start.bytes
            || seed.source_before.utf16 <= source_start.utf16
            || seed.source_before.bytes >= source_end.bytes
            || seed.source_before.utf16 >= source_end.utf16
            || seed.replacement_prefix_runs == 0
            || seed.replacement_prefix_runs >= storage.replacement_coverage_runs()
            || !survivor_after_retained_cut
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        let survivor_projection_runs_before = canonical_projection_runs_before
            .checked_add(seed.replacement_prefix_runs)
            .ok_or(SourceProjectionComposerError::Overflow(
                "surviving fragment projection runs before",
            ))?;
        let generation = self.next_fragment_generation;
        let next_generation =
            generation
                .checked_add(1)
                .ok_or(SourceProjectionComposerError::Overflow(
                    "surviving fragment origin generation",
                ))?;
        let issued_generation = NonZeroU64::new(generation).ok_or(
            SourceProjectionComposerError::Overflow("surviving fragment origin generation"),
        )?;

        let rebase = self.finish_canonical_fragment_replacement(storage)?;
        if rebase.generation != old_generation
            || rebase.canonical_suffix_projection_runs <= survivor_projection_runs_before
        {
            return self.fail(SourceProjectionComposerError::WrongFragmentReplacement);
        }
        self.next_fragment_generation = next_generation;
        self.issued_fragment_origin = Some(issued_generation);
        let origin = CanonicalFragmentProjectionOrigin {
            epoch: self.epoch,
            generation,
            source_before: seed.source_before,
            canonical_projection_runs_before: survivor_projection_runs_before,
            parent_checkpoint: None,
        };
        Ok((rebase, origin))
    }

    /// Consumes an empty, source-drained composer and the exact green sink
    /// acknowledgement at the same line boundary.
    ///
    /// `flush_before_structure` must already have reached `Idle` after every
    /// returned run was sealed and acknowledged by the caller. Requiring the
    /// storage capability closes the final gap that composer state alone
    /// cannot observe. A failure burns both linear inputs with the candidate.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn pause_at_line_boundary(
        mut self,
        storage: SourceProjectionLineBoundaryStorageAck,
    ) -> Result<SourceProjectionComposerLineBoundaryContinuation, SourceProjectionComposerError>
    {
        self.require_live()?;
        if self.structural_flush != StructuralFlushState::Ready
            || self.envelope.is_some()
            || self.pending_piece.is_some()
            || self.pending_run.is_some()
            || self.flushing
            || self.finish_requested
            || self.completion_source_seal.is_some()
            || self.fragment_replacement.is_some()
        {
            return self.fail(SourceProjectionComposerError::LineBoundaryNotReady);
        }
        if storage.build_id() != self.epoch.build_id() {
            return self.fail(SourceProjectionComposerError::WrongLineBoundaryStorage);
        }
        if storage.source_before() != self.next_source {
            return self.fail(SourceProjectionComposerError::OutOfOrderSource);
        }
        let expected_generation = self.receipt.projection_runs_sealed.checked_add(1).ok_or(
            SourceProjectionComposerError::Overflow("line-boundary composer generation"),
        )?;
        if self.next_composer_generation != expected_generation {
            return self.fail(SourceProjectionComposerError::WrongLineBoundaryStorage);
        }
        Ok(SourceProjectionComposerLineBoundaryContinuation {
            epoch: self.epoch,
            next_fragment_generation: self.next_fragment_generation,
            issued_fragment_origin: self.issued_fragment_origin,
            receipt: self.receipt,
            coverage_origin: self.coverage_origin,
            storage,
        })
    }

    /// Resumes one same-build line-boundary continuation. The returned storage
    /// acknowledgement still owns the exact green cut so the writer can bind
    /// it into its composite checkpoint entry; no scalar cut is reminted.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resume_line_boundary(
        epoch: LiveCandidateEpoch,
        continuation: SourceProjectionComposerLineBoundaryContinuation,
    ) -> Result<(Self, SourceProjectionLineBoundaryStorageAck), SourceProjectionComposerError> {
        let SourceProjectionComposerLineBoundaryContinuation {
            epoch: stored_epoch,
            next_fragment_generation,
            issued_fragment_origin,
            receipt,
            coverage_origin,
            storage,
        } = continuation;
        let next_source = storage.source_before();
        if stored_epoch != epoch || stored_epoch.build_id() != storage.build_id() {
            return Err(SourceProjectionComposerError::WrongLineBoundaryStorage);
        }
        let expected_generation = receipt.projection_runs_sealed.checked_add(1).ok_or(
            SourceProjectionComposerError::Overflow("line-boundary composer generation"),
        )?;
        Ok((
            Self {
                epoch,
                next_composer_generation: expected_generation,
                next_fragment_generation,
                issued_fragment_origin,
                coverage_origin,
                next_source,
                envelope: None,
                pending_piece: None,
                pending_run: None,
                flushing: false,
                structural_flush: StructuralFlushState::Ready,
                finish_requested: false,
                completion_source_seal: None,
                fragment_replacement: None,
                poisoned: false,
                receipt,
            },
            storage,
        ))
    }

    /// Consumes exactly one source piece. On an envelope boundary the piece is
    /// the sole pending input while the preceding bounded envelope drains.
    #[allow(clippy::needless_pass_by_value)]
    pub fn push_piece(
        &mut self,
        piece: ConsumedSourcePiece,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        self.require_live()?;
        if self.fragment_replacement.is_some() {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        if self.finish_requested {
            return self.fail(SourceProjectionComposerError::FinishAlreadyStarted);
        }
        if self.structural_flush == StructuralFlushState::Requested {
            return self.fail(SourceProjectionComposerError::StructuralFlushAlreadyStarted);
        }
        if self.pending_run.is_some() {
            return self.fail(SourceProjectionComposerError::PendingRunMustBeSealed);
        }
        if self.pending_piece.is_some() {
            return self.fail(SourceProjectionComposerError::InvalidSourcePiece);
        }
        self.structural_flush = StructuralFlushState::None;
        self.validate_piece(&piece)?;
        let key = match envelope_key(&piece) {
            Ok(key) => key,
            Err(error) => return self.fail(error),
        };
        if self
            .envelope
            .as_ref()
            .is_some_and(|envelope| envelope.key != key)
        {
            self.pending_piece = Some(piece);
            self.receipt.maximum_pending_source_pieces = 1;
            self.flushing = true;
            return self.advance();
        }
        self.accept_piece_guarded(piece)
    }

    /// Seals the one ready chunk with a fresh build-scoped coverage identity.
    /// A wrong/stale permit burns both inputs and poisons this composer.
    #[allow(clippy::needless_pass_by_value)]
    pub fn seal_pending_run(
        &mut self,
        permit: FreshCoveragePermit,
    ) -> Result<ComposerSealedProjectionRunCapability, SourceProjectionComposerError> {
        self.require_live()?;
        if self.fragment_replacement.is_some() {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        let Some(pending) = self.pending_run.take() else {
            return self.fail(SourceProjectionComposerError::NoPendingRun);
        };
        if permit.build_id() != self.epoch.build_id() {
            self.poison();
            return Err(SourceProjectionComposerError::WrongCoveragePermit);
        }
        let coverage = permit.id();
        let metric = match checked_sub_metric(pending.source_end, pending.source_start) {
            Ok(metric) => metric,
            Err(error) => return self.fail(error),
        };
        let run_result = match pending.logical_contribution {
            LogicalContribution::None => SourceProjectionRun::new(
                coverage,
                metric.bytes,
                metric.utf16,
                pending.key.owner_relative_depth,
                pending.key.part,
            ),
            logical => {
                let Some((target, _)) = pending.key.logical_target else {
                    self.poison();
                    return Err(SourceProjectionComposerError::InvalidSourcePiece);
                };
                SourceProjectionRun::with_logical(
                    coverage,
                    metric.bytes,
                    metric.utf16,
                    pending.key.owner_relative_depth,
                    pending.key.part,
                    target,
                    logical,
                )
            }
        };
        let run = match run_result {
            Ok(run) => run,
            Err(error) => {
                self.poison();
                return Err(SourceProjectionComposerError::Codec(error));
            }
        };
        let Some(projection_runs_sealed) = self.receipt.projection_runs_sealed.checked_add(1)
        else {
            return self.fail(SourceProjectionComposerError::Overflow(
                "sealed projection runs",
            ));
        };
        let composer_generation = self.next_composer_generation;
        let Some(next_composer_generation) = composer_generation.checked_add(1) else {
            return self.fail(SourceProjectionComposerError::Overflow(
                "composer generation",
            ));
        };
        self.next_composer_generation = next_composer_generation;
        self.receipt.projection_runs_sealed = projection_runs_sealed;
        Ok(ComposerSealedProjectionRunCapability {
            source: self.epoch.source(),
            build: self.epoch.build_id(),
            source_start: pending.source_start,
            source_end: pending.source_end,
            coverage,
            composer_generation,
            run,
        })
    }

    /// Advances a pending envelope flush by at most one chunk. Call only after
    /// consuming the previously returned sealed capability.
    pub fn poll(
        &mut self,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        self.require_live()?;
        if self.fragment_replacement.is_some() {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        if self.pending_run.is_some() {
            return self.fail(SourceProjectionComposerError::PendingRunMustBeSealed);
        }
        self.advance()
    }

    /// Ends the current projection envelope before a zero-width structural
    /// event is offered to the packed green builder. Unlike document finish,
    /// this preserves the composer's live semantic state and accepts later
    /// source pieces.
    pub(crate) fn flush_before_structure(
        &mut self,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        self.require_live()?;
        if self.fragment_replacement.is_some() {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        if self.finish_requested {
            return self.fail(SourceProjectionComposerError::FinishAlreadyStarted);
        }
        if self.pending_run.is_some() {
            return self.fail(SourceProjectionComposerError::PendingRunMustBeSealed);
        }
        if self.pending_piece.is_some() || self.flushing {
            return self.fail(SourceProjectionComposerError::StructuralFlushAlreadyStarted);
        }
        self.structural_flush = StructuralFlushState::Requested;
        self.flushing = self.envelope.is_some();
        self.advance()
    }

    /// Dedicated checkpoint drain admission.
    ///
    /// This is intentionally stricter than an ordinary structural flush. A
    /// projection Virtual that is still waiting for a physical anchor on its
    /// right must not be finalized merely because a checkpoint was requested:
    /// doing so would make checkpoint placement change affinity. The check is
    /// performed before any composer mutation. Current source-ledger input can
    /// produce only Identity, Hidden, and Atomic pieces, but this guard is the
    /// required update point if Program/inline admission later adds Virtuals.
    pub(crate) fn line_boundary_checkpoint_is_affinity_neutral(
        &self,
    ) -> Result<bool, SourceProjectionComposerError> {
        self.require_live()?;
        if self.fragment_replacement.is_some() {
            return Err(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        Ok(!self.envelope.as_ref().is_some_and(|envelope| {
            matches!(
                &envelope.projection,
                EnvelopeProjection::Logical(chunker)
                    if !chunker.checkpoint_cut_is_affinity_neutral()
            )
        }))
    }

    pub(crate) fn flush_for_line_boundary_checkpoint(
        &mut self,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        self.require_live()?;
        if !self.line_boundary_checkpoint_is_affinity_neutral()? {
            return Err(SourceProjectionComposerError::LineBoundaryVirtualUnsafe);
        }
        self.flush_before_structure()
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn begin_finish(
        &mut self,
        seal: CandidateSourceSeal,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        self.require_live()?;
        if self.fragment_replacement.is_some() || self.issued_fragment_origin.is_some() {
            return self.fail(SourceProjectionComposerError::FragmentReplacementNotReady);
        }
        if self.finish_requested {
            return self.fail(SourceProjectionComposerError::FinishAlreadyStarted);
        }
        if self.pending_run.is_some() {
            return self.fail(SourceProjectionComposerError::PendingRunMustBeSealed);
        }
        if self.structural_flush == StructuralFlushState::Requested {
            return self.fail(SourceProjectionComposerError::StructuralFlushAlreadyStarted);
        }
        self.structural_flush = StructuralFlushState::None;
        let seal_metric = seal.metric();
        if seal.source() != self.epoch.source()
            || seal.build_id() != self.epoch.build_id()
            || seal_metric.bytes() != self.next_source.bytes
            || seal_metric.utf16() != self.next_source.utf16
            || seal.source_piece_count() != self.receipt.source_pieces_consumed
        {
            return self.fail(SourceProjectionComposerError::WrongSourceSeal);
        }
        self.completion_source_seal = Some(seal);
        self.finish_requested = true;
        self.flushing = self.envelope.is_some();
        self.advance()
    }

    /// Consumes the completed composer and returns the exact source seal that
    /// entered it. No separately reusable EOF or scalar receipt escapes.
    pub(crate) fn into_completion_seal(
        mut self,
    ) -> Result<SourceProjectionComposerCompletionSeal, SourceProjectionComposerError> {
        self.require_live()?;
        if !self.finish_requested
            || self.flushing
            || self.envelope.is_some()
            || self.pending_piece.is_some()
            || self.pending_run.is_some()
            || self.issued_fragment_origin.is_some()
            || self.fragment_replacement.is_some()
        {
            return self.fail(SourceProjectionComposerError::CompletionNotReady);
        }
        let source = self
            .completion_source_seal
            .take()
            .ok_or(SourceProjectionComposerError::CompletionNotReady)?;
        Ok(SourceProjectionComposerCompletionSeal {
            source,
            receipt: self.receipt,
            coverage_origin: self.coverage_origin,
        })
    }

    /// Explicit cancellation burns every pending linear input and returns only
    /// scalar diagnostics.
    #[must_use]
    pub fn cancel(mut self) -> SourceProjectionComposerReceipt {
        self.poison();
        self.receipt
    }

    fn accept_piece_guarded(
        &mut self,
        piece: ConsumedSourcePiece,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        let result = self.accept_piece(piece);
        if result.is_err() {
            self.poison();
        }
        result
    }

    // Ownership, rather than field mutation, is the linear source transition.
    #[allow(clippy::needless_pass_by_value)]
    fn accept_piece(
        &mut self,
        piece: ConsumedSourcePiece,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        self.validate_piece(&piece)?;
        let key = envelope_key(&piece)?;
        let metric = source_metric(&piece);
        let piece_start = self.next_source;
        let piece_end = checked_add_metric(piece_start, metric)?;
        let projection_piece = retained_projection_piece(&piece)?;

        if self.envelope.is_none() {
            self.envelope = Some(OpenEnvelope {
                key,
                source_start: piece_start,
                source_end: piece_start,
                next_chunk_start: piece_start,
                projection: if projection_piece.is_some() {
                    EnvelopeProjection::Logical(ProjectionProgramChunker::new_source_bound())
                } else {
                    EnvelopeProjection::PhysicalOnly
                },
            });
        }
        let envelope = self.envelope.as_mut().expect("envelope was opened");
        if envelope.key != key {
            return self.fail(SourceProjectionComposerError::InvalidSourcePiece);
        }
        match (&mut envelope.projection, projection_piece) {
            (EnvelopeProjection::PhysicalOnly, None) => {}
            (EnvelopeProjection::Logical(chunker), Some(projection_piece)) => {
                let emitted = match chunker.push(projection_piece) {
                    Ok(emitted) => emitted,
                    Err(error) => {
                        self.poison();
                        return Err(SourceProjectionComposerError::Codec(error));
                    }
                };
                observe_chunker_receipt(&mut self.receipt, chunker);
                if let Some(chunk) = emitted {
                    let pending = match pending_from_chunk(envelope, chunk) {
                        Ok(pending) => pending,
                        Err(error) => return self.fail(error),
                    };
                    self.pending_run = Some(pending);
                    self.receipt.maximum_pending_runs = 1;
                }
            }
            (EnvelopeProjection::PhysicalOnly, Some(_))
            | (EnvelopeProjection::Logical(_), None) => {
                return self.fail(SourceProjectionComposerError::InvalidSourcePiece);
            }
        }
        envelope.source_end = piece_end;
        self.next_source = piece_end;
        self.receipt.source_pieces_consumed =
            self.receipt.source_pieces_consumed.checked_add(1).ok_or(
                SourceProjectionComposerError::Overflow("consumed source pieces"),
            )?;
        if self.pending_run.is_some() {
            Ok(SourceProjectionComposerProgress::RunReady)
        } else {
            Ok(SourceProjectionComposerProgress::Idle)
        }
    }

    fn advance(
        &mut self,
    ) -> Result<SourceProjectionComposerProgress, SourceProjectionComposerError> {
        if self.pending_run.is_some() {
            return self.fail(SourceProjectionComposerError::PendingRunMustBeSealed);
        }
        if self.flushing {
            let Some(envelope) = self.envelope.as_mut() else {
                return self.fail(SourceProjectionComposerError::InvalidSourcePiece);
            };
            match &mut envelope.projection {
                EnvelopeProjection::PhysicalOnly => {
                    self.pending_run = Some(PendingRun {
                        key: envelope.key,
                        source_start: envelope.source_start,
                        source_end: envelope.source_end,
                        logical_contribution: LogicalContribution::None,
                    });
                    self.receipt.maximum_pending_runs = 1;
                    self.envelope = None;
                    self.flushing = false;
                    return Ok(SourceProjectionComposerProgress::RunReady);
                }
                EnvelopeProjection::Logical(chunker) => {
                    let (chunk, finish) = match chunker.finish_source_bound() {
                        Ok(value) => value,
                        Err(error) => {
                            self.poison();
                            return Err(SourceProjectionComposerError::Codec(error));
                        }
                    };
                    observe_chunker_receipt(&mut self.receipt, chunker);
                    if let Some(chunk) = chunk {
                        let pending = match pending_from_chunk(envelope, chunk) {
                            Ok(pending) => pending,
                            Err(error) => return self.fail(error),
                        };
                        self.pending_run = Some(pending);
                        self.receipt.maximum_pending_runs = 1;
                        return Ok(SourceProjectionComposerProgress::RunReady);
                    }
                    if !matches!(finish, ProjectionChunkerFinish::Complete(_))
                        || envelope.next_chunk_start != envelope.source_end
                    {
                        return self.fail(SourceProjectionComposerError::InvalidSourcePiece);
                    }
                    self.envelope = None;
                    self.flushing = false;
                }
            }
        }

        if self.structural_flush == StructuralFlushState::Requested {
            if self.flushing
                || self.envelope.is_some()
                || self.pending_piece.is_some()
                || self.pending_run.is_some()
            {
                return self.fail(SourceProjectionComposerError::InvalidSourcePiece);
            }
            self.structural_flush = StructuralFlushState::Ready;
        }

        if let Some(piece) = self.pending_piece.take() {
            return self.accept_piece_guarded(piece);
        }
        if self.finish_requested {
            if self.envelope.is_some() {
                self.flushing = true;
                return self.advance();
            }
            return Ok(SourceProjectionComposerProgress::Complete(self.receipt));
        }
        Ok(SourceProjectionComposerProgress::Idle)
    }

    fn validate_piece(
        &mut self,
        piece: &ConsumedSourcePiece,
    ) -> Result<(), SourceProjectionComposerError> {
        if piece.source() != self.epoch.source() || piece.build_id() != self.epoch.build_id() {
            return self.fail(SourceProjectionComposerError::WrongCandidate);
        }
        let (start, end) = piece.absolute_range();
        let metric = source_metric(piece);
        if start != self.next_source.bytes
            || end <= start
            || end.checked_sub(start) != Some(metric.bytes)
            || metric.bytes == 0
            || metric.utf16 == 0
        {
            return self.fail(SourceProjectionComposerError::OutOfOrderSource);
        }
        Ok(())
    }

    fn require_live(&self) -> Result<(), SourceProjectionComposerError> {
        if self.poisoned {
            Err(SourceProjectionComposerError::ComposerPoisoned)
        } else {
            Ok(())
        }
    }

    fn fail<T>(
        &mut self,
        error: SourceProjectionComposerError,
    ) -> Result<T, SourceProjectionComposerError> {
        self.poison();
        Err(error)
    }

    fn poison(&mut self) {
        self.poisoned = true;
        self.envelope = None;
        self.pending_piece = None;
        self.pending_run = None;
        self.flushing = false;
        self.structural_flush = StructuralFlushState::None;
        self.completion_source_seal = None;
        self.issued_fragment_origin = None;
        self.fragment_replacement = None;
    }
}

fn envelope_key(piece: &ConsumedSourcePiece) -> Result<EnvelopeKey, SourceProjectionComposerError> {
    let logical = piece.logical();
    let logical_target = match (logical.target(), logical.target_depth()) {
        (None, None) if logical.kind() == ValidatedLogicalKind::None => None,
        (Some(target), Some(depth)) if logical.kind() != ValidatedLogicalKind::None => {
            Some((target, depth))
        }
        _ => return Err(SourceProjectionComposerError::InvalidSourcePiece),
    };
    Ok(EnvelopeKey {
        physical_owner: piece.physical_owner_stamp(),
        owner_relative_depth: piece.owner_relative_depth(),
        structural_state_generation: piece.structural_state_generation(),
        part: piece.part(),
        logical_target,
    })
}

fn source_metric(piece: &ConsumedSourcePiece) -> SerializedMetric {
    let metric = piece.metric();
    SerializedMetric {
        bytes: metric.bytes(),
        utf16: metric.utf16(),
    }
}

fn retained_projection_piece(
    piece: &ConsumedSourcePiece,
) -> Result<Option<ProjectionPiece>, SourceProjectionComposerError> {
    let metric = source_metric(piece);
    let logical = piece.logical();
    Ok(match logical.kind() {
        ValidatedLogicalKind::None => None,
        ValidatedLogicalKind::Identity => Some(ProjectionPiece::Identity { metric }),
        ValidatedLogicalKind::Hidden => Some(ProjectionPiece::Hidden {
            metric,
            affinity: logical
                .hidden_affinity()
                .ok_or(SourceProjectionComposerError::InvalidSourcePiece)?,
        }),
        ValidatedLogicalKind::Atomic => {
            let projection = match logical
                .projection()
                .ok_or(SourceProjectionComposerError::InvalidSourcePiece)?
            {
                CandidateAtomicProjection::TabToSpaces { spaces } => {
                    AtomicProjection::tab_to_spaces(spaces)?
                }
                CandidateAtomicProjection::CrLfToLf => AtomicProjection::crlf_to_lf(),
                CandidateAtomicProjection::LoneCrToLf => AtomicProjection::lone_cr_to_lf(),
                CandidateAtomicProjection::NulToReplacement => {
                    AtomicProjection::nul_to_replacement()
                }
            };
            Some(ProjectionPiece::Atomic {
                physical_metric: metric,
                projection,
            })
        }
    })
}

fn pending_from_chunk(
    envelope: &mut OpenEnvelope,
    chunk: ProjectionChunk,
) -> Result<PendingRun, SourceProjectionComposerError> {
    let source_start = envelope.next_chunk_start;
    let source_end = checked_add_metric(source_start, chunk.physical_metric)?;
    if source_end.bytes > envelope.source_end.bytes || source_end.utf16 > envelope.source_end.utf16
    {
        return Err(SourceProjectionComposerError::InvalidSourcePiece);
    }
    envelope.next_chunk_start = source_end;
    Ok(PendingRun {
        key: envelope.key,
        source_start,
        source_end,
        logical_contribution: chunk.logical_contribution,
    })
}

fn observe_chunker_receipt(
    receipt: &mut SourceProjectionComposerReceipt,
    chunker: &ProjectionProgramChunker,
) {
    let chunker = chunker.receipt();
    receipt.maximum_buffered_projection_bytes = receipt
        .maximum_buffered_projection_bytes
        .max(chunker.maximum_buffered_payload_bytes);
    receipt.maximum_projection_buffer_capacity_bytes = receipt
        .maximum_projection_buffer_capacity_bytes
        .max(chunker.maximum_buffer_capacity_bytes);
}

fn checked_add_metric(
    left: SerializedMetric,
    right: SerializedMetric,
) -> Result<SerializedMetric, SourceProjectionComposerError> {
    Ok(SerializedMetric {
        bytes: left
            .bytes
            .checked_add(right.bytes)
            .ok_or(SourceProjectionComposerError::Overflow("source bytes"))?,
        utf16: left
            .utf16
            .checked_add(right.utf16)
            .ok_or(SourceProjectionComposerError::Overflow("source UTF-16"))?,
    })
}

fn checked_sub_metric(
    later: SerializedMetric,
    earlier: SerializedMetric,
) -> Result<SerializedMetric, SourceProjectionComposerError> {
    Ok(SerializedMetric {
        bytes: later
            .bytes
            .checked_sub(earlier.bytes)
            .ok_or(SourceProjectionComposerError::OutOfOrderSource)?,
        utf16: later
            .utf16
            .checked_sub(earlier.utf16)
            .ok_or(SourceProjectionComposerError::OutOfOrderSource)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CandidateLineEnding, CandidateLogicalAction, CandidateOpenBinding, CandidateSourceAtom,
        CandidateSourceAtomKind, CandidateSourcePoll, CandidateTerminatorResolution, GreenKind,
        LiveDocumentError, LiveDocumentStore, PROJECTION_PROGRAM_PAGE_BYTES,
    };

    fn activate(
        source: &str,
    ) -> (
        LiveDocumentStore,
        LiveCandidateEpoch,
        CandidateOpenBinding,
        CandidateOpenBinding,
        SourceBoundProjectionComposer,
    ) {
        let mut document = LiveDocumentStore::new(source, 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        let root = document
            .candidate_open_binding(epoch, GreenKind::DOCUMENT)
            .unwrap();
        let paragraph = document
            .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
            .unwrap();
        let composer = document.begin_source_projection_composer(epoch).unwrap();
        (document, epoch, root, paragraph, composer)
    }

    fn next_atom(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> Option<CandidateSourceAtom> {
        loop {
            match document.poll_candidate_source(epoch, 1).unwrap() {
                CandidateSourcePoll::NeedFuel(_) => {}
                CandidateSourcePoll::Atom { atom, .. } => return Some(atom),
                CandidateSourcePoll::Eof(_) => return None,
            }
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct RunShape {
        metric: SerializedMetric,
        owner_relative_depth: u32,
        part: CoveragePart,
        logical_contribution: LogicalContribution,
        projection_reset_after: bool,
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct RunStats {
        runs: u64,
        programs: u64,
        identities: u64,
        physical_only: u64,
        bytes: u64,
        utf16: u64,
        run_shapes: Vec<RunShape>,
    }

    fn drive(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        composer: &mut SourceBoundProjectionComposer,
        mut progress: SourceProjectionComposerProgress,
        stats: &mut RunStats,
    ) -> Option<SourceProjectionComposerReceipt> {
        loop {
            match progress {
                SourceProjectionComposerProgress::Idle => return None,
                SourceProjectionComposerProgress::Complete(receipt) => return Some(receipt),
                SourceProjectionComposerProgress::RunReady => {
                    let permit = document.mint_coverage_permit(epoch).unwrap();
                    let cap = composer.seal_pending_run(permit).unwrap();
                    assert_eq!(cap.composer_generation(), stats.runs + 1);
                    let run = cap.into_run();
                    stats.runs += 1;
                    stats.bytes += run.metric.bytes;
                    stats.utf16 += run.metric.utf16;
                    stats.run_shapes.push(RunShape {
                        metric: run.metric,
                        owner_relative_depth: run.owner_relative_depth,
                        part: run.part,
                        logical_contribution: run.logical_contribution.clone(),
                        projection_reset_after: run.has_projection_reset_after(),
                    });
                    match run.logical_contribution {
                        LogicalContribution::Program(_) => stats.programs += 1,
                        LogicalContribution::Identity => stats.identities += 1,
                        LogicalContribution::None => stats.physical_only += 1,
                        LogicalContribution::Hidden { .. } | LogicalContribution::Atomic(_) => {}
                    }
                    progress = composer.poll().unwrap();
                }
            }
        }
    }

    fn push(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        composer: &mut SourceBoundProjectionComposer,
        piece: ConsumedSourcePiece,
        stats: &mut RunStats,
    ) {
        let progress = composer.push_piece(piece).unwrap();
        assert!(drive(document, epoch, composer, progress, stats).is_none());
    }

    fn finish(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        composer: &mut SourceBoundProjectionComposer,
        seal: CandidateSourceSeal,
        stats: &mut RunStats,
    ) -> SourceProjectionComposerReceipt {
        let progress = composer.begin_finish(seal).unwrap();
        drive(document, epoch, composer, progress, stats).expect("composer completes")
    }

    fn drain_structural_flush(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        composer: &mut SourceBoundProjectionComposer,
        stats: &mut RunStats,
    ) {
        let progress = composer.flush_before_structure().unwrap();
        assert!(
            drive(document, epoch, composer, progress, stats).is_none(),
            "a structural flush cannot complete the document composer"
        );
        assert_eq!(composer.structural_flush, StructuralFlushState::Ready);
        assert!(composer.envelope.is_none());
        assert!(composer.pending_piece.is_none());
        assert!(composer.pending_run.is_none());
        assert!(!composer.flushing);
    }

    fn mechanism_storage_ack(
        epoch: LiveCandidateEpoch,
        composer: &SourceBoundProjectionComposer,
    ) -> SourceProjectionLineBoundaryStorageAck {
        SourceProjectionLineBoundaryStorageAck::mechanism_only(
            epoch.build_id(),
            composer.next_source,
        )
    }

    fn pause_and_resume(
        epoch: LiveCandidateEpoch,
        composer: SourceBoundProjectionComposer,
    ) -> SourceBoundProjectionComposer {
        let expected_receipt = composer.receipt();
        let storage = mechanism_storage_ack(epoch, &composer);
        let continuation = composer.pause_at_line_boundary(storage).unwrap();
        assert_eq!(continuation.receipt(), expected_receipt);
        let (composer, storage) =
            SourceBoundProjectionComposer::resume_line_boundary(epoch, continuation).unwrap();
        assert_eq!(storage.build_id(), epoch.build_id());
        assert_eq!(storage.source_before(), composer.next_source);
        drop(storage);
        composer
    }

    fn consume_identity_atom(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
    ) -> ConsumedSourcePiece {
        let atom = next_atom(document, epoch).expect("the test source has another atom");
        let identity = CandidateLogicalAction::identity(owner).unwrap();
        document
            .candidate_consume_to(epoch, atom.boundary(), owner, part, &identity)
            .unwrap()
    }

    #[test]
    fn canonical_fragment_rebase_can_increase_run_cardinality_without_rewinding_generation() {
        let (mut document, epoch, _root, paragraph, mut composer) = activate("abc");
        let mut stats = RunStats::default();
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        let origin = composer.capture_canonical_fragment_origin().unwrap();

        for _ in 0..2 {
            let piece =
                consume_identity_atom(&mut document, epoch, &paragraph, CoveragePart::CONTENT);
            push(&mut document, epoch, &mut composer, piece, &mut stats);
        }
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        assert_eq!(
            stats.runs, 1,
            "the provisional Paragraph stays naturally coalesced"
        );

        let physical = SerializedMetric { bytes: 2, utf16: 2 };
        composer
            .begin_canonical_fragment_replacement(origin, physical)
            .unwrap();
        let storage = CanonicalFragmentReplacement::mechanism_only_for_projection_rebase(
            epoch.build_id(),
            physical,
            1,
            3,
        );
        let rebase = composer
            .finish_canonical_fragment_replacement(&storage)
            .unwrap();
        assert_eq!(rebase.retired_projection_runs(), 1);
        assert_eq!(rebase.installed_projection_runs(), 3);
        assert_eq!(rebase.canonical_suffix_projection_runs(), 3);
        assert_eq!(composer.receipt().projection_runs_sealed, 1);
        assert_eq!(composer.receipt().canonical_projection_runs(), Ok(3));

        let piece = consume_identity_atom(&mut document, epoch, &paragraph, CoveragePart::CONTENT);
        push(&mut document, epoch, &mut composer, piece, &mut stats);
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        assert_eq!(stats.runs, 2, "fresh issuance continues at generation two");
        assert_eq!(composer.receipt().projection_runs_sealed, 2);
        assert_eq!(composer.receipt().canonical_projection_runs(), Ok(4));
    }

    #[test]
    fn canonical_fragment_rebase_can_decrease_run_cardinality() {
        let (mut document, epoch, _root, paragraph, mut composer) = activate("abc");
        let mut stats = RunStats::default();
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        let origin = composer.capture_canonical_fragment_origin().unwrap();

        for part in [
            CoveragePart::CONTENT,
            CoveragePart::BLOCK_MARKER,
            CoveragePart::CONTENT,
        ] {
            let piece = consume_identity_atom(&mut document, epoch, &paragraph, part);
            push(&mut document, epoch, &mut composer, piece, &mut stats);
        }
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        assert_eq!(stats.runs, 3);

        let physical = SerializedMetric { bytes: 3, utf16: 3 };
        composer
            .begin_canonical_fragment_replacement(origin, physical)
            .unwrap();
        let storage = CanonicalFragmentReplacement::mechanism_only_for_projection_rebase(
            epoch.build_id(),
            physical,
            3,
            1,
        );
        let rebase = composer
            .finish_canonical_fragment_replacement(&storage)
            .unwrap();
        assert_eq!(rebase.canonical_suffix_projection_runs(), 1);
        assert_eq!(composer.receipt().projection_runs_sealed, 3);
        assert_eq!(composer.receipt().canonical_projection_runs(), Ok(1));
    }

    #[test]
    fn canonical_fragment_rebase_mismatch_poisoned_before_cardinality_commit() {
        let (mut document, epoch, _root, paragraph, mut composer) = activate("a");
        let mut stats = RunStats::default();
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        let origin = composer.capture_canonical_fragment_origin().unwrap();
        let piece = consume_identity_atom(&mut document, epoch, &paragraph, CoveragePart::CONTENT);
        push(&mut document, epoch, &mut composer, piece, &mut stats);
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);

        let physical = SerializedMetric { bytes: 1, utf16: 1 };
        composer
            .begin_canonical_fragment_replacement(origin, physical)
            .unwrap();
        let mismatched = CanonicalFragmentReplacement::mechanism_only_for_projection_rebase(
            epoch.build_id(),
            physical,
            2,
            3,
        );
        assert_eq!(
            composer.finish_canonical_fragment_replacement(&mismatched),
            Err(SourceProjectionComposerError::WrongFragmentReplacement)
        );
        assert!(composer.poisoned);
        assert_eq!(
            composer.receipt().projection_runs_retired_by_normalization,
            0
        );
        assert_eq!(
            composer
                .receipt()
                .projection_runs_installed_by_normalization,
            0
        );
    }

    #[test]
    fn suffix_local_restart_cannot_claim_cumulative_tail_coverage() {
        let (_document, epoch, _root, _paragraph, _zero_restart) = activate("a\n");
        let storage = SourceProjectionLineBoundaryStorageAck::mechanism_only(
            epoch.build_id(),
            SerializedMetric::default(),
        );
        let (suffix_local, storage) =
            SourceBoundProjectionComposer::begin_retained_line_boundary(epoch, storage)
                .expect("retained composer starts from authenticated storage cut");
        let continuation = suffix_local
            .pause_at_line_boundary(storage)
            .expect("empty retained composer pauses at its cut");

        assert_eq!(
            continuation.checkpoint_prefix_projection_runs(),
            Err(SourceProjectionComposerError::MissingCheckpointCoverage)
        );
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn parent_selected_composer_coverage_is_nonzero_typed_and_exact() {
        let (_document, epoch, _root, _paragraph, _zero_restart) = activate("a\n");
        let accepted = SerializedMetric { bytes: 2, utf16: 2 };
        let event_cut = 7;
        let projection_runs = 3;
        let coverage = ParentSelectedComposerCoverage::mechanism_only_for_test(
            epoch,
            accepted,
            event_cut,
            projection_runs,
        );
        let storage = SourceProjectionLineBoundaryStorageAck::mechanism_only_at_event_cut(
            epoch.build_id(),
            accepted,
            event_cut,
        );
        let (composer, storage) =
            SourceBoundProjectionComposer::begin_parent_selected_line_boundary(
                epoch, storage, coverage,
            )
            .expect("the exact parent-selected coverage base is admitted");
        assert_eq!(composer.next_source, accepted);
        assert_eq!(
            composer.receipt(),
            SourceProjectionComposerReceipt::default()
        );
        let continuation = composer
            .pause_at_line_boundary(storage)
            .expect("an idle parent-selected composer pauses at the exact cut");
        assert_eq!(
            continuation.checkpoint_prefix_projection_runs(),
            Ok(projection_runs)
        );
        assert_eq!(continuation.receipt().projection_runs_sealed, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn completion_adds_selected_checkpoint_origin_to_suffix_receipt_once() {
        let (mut document, epoch, root, paragraph, mut composer) = activate("x");
        // Admission of this value is covered by
        // `parent_selected_composer_coverage_is_nonzero_typed_and_exact`; this
        // unit test isolates preservation through the EOF completion seal.
        composer.coverage_origin = ComposerCoverageOrigin::selected_checkpoint(3).unwrap();
        let mut stats = RunStats::default();
        let x = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let x = document
            .candidate_consume_to(
                epoch,
                x.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, x, &mut stats);
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let source = document.seal_candidate_source(epoch).unwrap();
        let suffix = finish(&mut document, epoch, &mut composer, source, &mut stats);
        assert_eq!(suffix.projection_runs_sealed, 1);

        let completion = composer.into_completion_seal().unwrap();
        assert_eq!(completion.checkpoint_prefix_projection_runs(), Ok(3));
        assert_eq!(completion.receipt().projection_runs_sealed, 1);
        assert_eq!(completion.cumulative_projection_runs(), Ok(4));
    }

    #[test]
    fn completion_rejects_suffix_local_coverage_without_checkpoint_origin() {
        let (mut document, epoch, root, paragraph, mut composer) = activate("x");
        composer.coverage_origin = ComposerCoverageOrigin::SUFFIX_LOCAL_RESTART;
        let mut stats = RunStats::default();
        let x = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let x = document
            .candidate_consume_to(
                epoch,
                x.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, x, &mut stats);
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let source = document.seal_candidate_source(epoch).unwrap();
        finish(&mut document, epoch, &mut composer, source, &mut stats);

        let completion = composer.into_completion_seal().unwrap();
        assert_eq!(
            completion.cumulative_projection_runs(),
            Err(SourceProjectionComposerError::MissingCheckpointCoverage)
        );
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn parent_selected_composer_coverage_rejects_crossed_epoch_a_and_event_cut() {
        let (_document, epoch, _root, _paragraph, _zero_restart) = activate("a\n");
        let (_other_document, other_epoch, ..) = activate("b\n");
        let accepted = SerializedMetric { bytes: 2, utf16: 2 };

        let wrong_epoch =
            ParentSelectedComposerCoverage::mechanism_only_for_test(other_epoch, accepted, 7, 3);
        let storage = SourceProjectionLineBoundaryStorageAck::mechanism_only_at_event_cut(
            epoch.build_id(),
            accepted,
            7,
        );
        assert!(matches!(
            SourceBoundProjectionComposer::begin_parent_selected_line_boundary(
                epoch,
                storage,
                wrong_epoch,
            ),
            Err(SourceProjectionComposerError::WrongLineBoundaryStorage)
        ));

        let wrong_a = ParentSelectedComposerCoverage::mechanism_only_for_test(
            epoch,
            SerializedMetric { bytes: 1, utf16: 1 },
            7,
            3,
        );
        let storage = SourceProjectionLineBoundaryStorageAck::mechanism_only_at_event_cut(
            epoch.build_id(),
            accepted,
            7,
        );
        assert!(matches!(
            SourceBoundProjectionComposer::begin_parent_selected_line_boundary(
                epoch, storage, wrong_a,
            ),
            Err(SourceProjectionComposerError::WrongLineBoundaryStorage)
        ));

        let wrong_event =
            ParentSelectedComposerCoverage::mechanism_only_for_test(epoch, accepted, 8, 3);
        let storage = SourceProjectionLineBoundaryStorageAck::mechanism_only_at_event_cut(
            epoch.build_id(),
            accepted,
            7,
        );
        assert!(matches!(
            SourceBoundProjectionComposer::begin_parent_selected_line_boundary(
                epoch,
                storage,
                wrong_event,
            ),
            Err(SourceProjectionComposerError::WrongLineBoundaryStorage)
        ));

        let zero_base =
            ParentSelectedComposerCoverage::mechanism_only_for_test(epoch, accepted, 7, 0);
        let storage = SourceProjectionLineBoundaryStorageAck::mechanism_only_at_event_cut(
            epoch.build_id(),
            accepted,
            7,
        );
        assert!(matches!(
            SourceBoundProjectionComposer::begin_parent_selected_line_boundary(
                epoch, storage, zero_base,
            ),
            Err(SourceProjectionComposerError::WrongLineBoundaryStorage)
        ));
    }

    fn compose_two_lines(
        pause_at_first_boundary: bool,
    ) -> (SourceProjectionComposerReceipt, RunStats) {
        let (mut document, epoch, root, paragraph, mut composer) = activate("a\r\nb");
        let mut stats = RunStats::default();

        let a = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let a = document
            .candidate_consume_to(
                epoch,
                a.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, a, &mut stats);

        let crlf = next_atom(&mut document, epoch).unwrap();
        assert_eq!(
            crlf.kind(),
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf)
        );
        let canonical = CandidateLogicalAction::canonical_line_ending(&paragraph, &crlf).unwrap();
        let crlf = document
            .candidate_consume_to(
                epoch,
                crlf.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &canonical,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, crlf, &mut stats);
        document.candidate_finish_line(epoch).unwrap();
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        if pause_at_first_boundary {
            composer = pause_and_resume(epoch, composer);
        }

        let b = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let b = document
            .candidate_consume_to(
                epoch,
                b.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, b, &mut stats);
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        (receipt, stats)
    }

    #[test]
    fn production_ordinary_terminator_and_coalesced_gap_share_one_source_authority() {
        let source = "a\r\n \n";
        let (mut document, epoch, root, paragraph, mut composer) = activate(source);
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let mut stats = RunStats::default();

        let a = next_atom(&mut document, epoch).unwrap();
        let a = document
            .candidate_consume_to(
                epoch,
                a.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, a, &mut stats);

        let crlf = next_atom(&mut document, epoch).unwrap();
        assert_eq!(
            crlf.kind(),
            CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf)
        );
        document
            .candidate_stage_consumed_terminator(epoch, &crlf, &paragraph)
            .unwrap();
        document.candidate_finish_line(epoch).unwrap();
        let crlf = document
            .candidate_resolve_consumed_terminator(
                epoch,
                CandidateTerminatorResolution::ContinueCanonicalNewline,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, crlf, &mut stats);

        while !matches!(
            next_atom(&mut document, epoch).unwrap().kind(),
            CandidateSourceAtomKind::LineEnding(_)
        ) {}
        document.candidate_stage_consumed_blank_gap(epoch).unwrap();
        document.candidate_finish_line(epoch).unwrap();
        let gap = document
            .candidate_resolve_consumed_blank_gap(epoch, &root)
            .unwrap();
        push(&mut document, epoch, &mut composer, gap, &mut stats);

        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(receipt.source_pieces_consumed, 3);
        assert_eq!(receipt.projection_runs_sealed, 2);
        assert_eq!(stats.programs, 1);
        assert_eq!(stats.physical_only, 1);
        assert_eq!(stats.bytes, u64::try_from(source.len()).unwrap());
        assert_eq!(
            stats.utf16,
            u64::try_from(source.encode_utf16().count()).unwrap()
        );
    }

    #[test]
    fn dense_typed_atoms_pack_many_source_pieces_into_bounded_program_runs() {
        const REPETITIONS: usize = 4_000;
        let source = "\t\0\r\n".repeat(REPETITIONS);
        let (mut document, epoch, root, paragraph, mut composer) = activate(&source);
        let mut stats = RunStats::default();
        let mut source_pieces = 0_u64;
        while let Some(atom) = next_atom(&mut document, epoch) {
            let logical = match atom.kind() {
                CandidateSourceAtomKind::Tab => {
                    CandidateLogicalAction::tab_to_spaces(&paragraph, &atom, 4).unwrap()
                }
                CandidateSourceAtomKind::Nul => {
                    CandidateLogicalAction::nul_to_replacement(&paragraph, &atom).unwrap()
                }
                CandidateSourceAtomKind::LineEnding(CandidateLineEnding::CrLf) => {
                    CandidateLogicalAction::canonical_line_ending(&paragraph, &atom).unwrap()
                }
                other => panic!("unexpected dense atom {other:?}"),
            };
            let line_ended = matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_));
            let piece = document
                .candidate_consume_to(
                    epoch,
                    atom.boundary(),
                    &paragraph,
                    CoveragePart::CONTENT,
                    &logical,
                )
                .unwrap();
            push(&mut document, epoch, &mut composer, piece, &mut stats);
            source_pieces += 1;
            if line_ended {
                document.candidate_finish_line(epoch).unwrap();
            }
        }
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(source_pieces, u64::try_from(REPETITIONS * 3).unwrap());
        assert_eq!(receipt.source_pieces_consumed, source_pieces);
        assert_eq!(receipt.projection_runs_sealed, stats.runs);
        assert!(stats.runs * 100 < source_pieces, "runs={}", stats.runs);
        assert_eq!(stats.runs, stats.programs);
        assert!(receipt.maximum_buffered_projection_bytes <= PROJECTION_PROGRAM_PAGE_BYTES);
        assert_eq!(
            receipt.maximum_projection_buffer_capacity_bytes,
            PROJECTION_PROGRAM_PAGE_BYTES
        );
        assert_eq!(stats.bytes, u64::try_from(source.len()).unwrap());
        assert_eq!(stats.utf16, u64::try_from(source.len()).unwrap());
        assert_eq!(receipt.maximum_pending_source_pieces, 0);
        assert_eq!(receipt.maximum_pending_runs, 1);
    }

    #[test]
    fn canonical_one_piece_run_stays_inline_and_preserves_unicode_metric() {
        let source = "😀";
        let (mut document, epoch, root, paragraph, mut composer) = activate(source);
        let atom = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let piece = document
            .candidate_consume_to(
                epoch,
                atom.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        let mut stats = RunStats::default();
        push(&mut document, epoch, &mut composer, piece, &mut stats);
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(receipt.source_pieces_consumed, 1);
        assert_eq!(stats.runs, 1);
        assert_eq!(stats.identities, 1);
        assert_eq!(stats.programs, 0);
        assert_eq!((stats.bytes, stats.utf16), (4, 2));
    }

    #[test]
    fn close_open_at_equal_relative_depth_forces_distinct_runs() {
        let source = "ab";
        let (mut document, epoch, root, first, mut composer) = activate(source);
        let a = next_atom(&mut document, epoch).unwrap();
        let first_identity = CandidateLogicalAction::identity(&first).unwrap();
        let first_piece = document
            .candidate_consume_to(
                epoch,
                a.boundary(),
                &first,
                CoveragePart::CONTENT,
                &first_identity,
            )
            .unwrap();
        let mut stats = RunStats::default();
        push(&mut document, epoch, &mut composer, first_piece, &mut stats);
        document.candidate_close_binding(epoch, &first).unwrap();
        let second = document
            .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
            .unwrap();
        let b = next_atom(&mut document, epoch).unwrap();
        let second_identity = CandidateLogicalAction::identity(&second).unwrap();
        let second_piece = document
            .candidate_consume_to(
                epoch,
                b.boundary(),
                &second,
                CoveragePart::CONTENT,
                &second_identity,
            )
            .unwrap();
        push(
            &mut document,
            epoch,
            &mut composer,
            second_piece,
            &mut stats,
        );
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &second).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(receipt.source_pieces_consumed, 2);
        assert_eq!(receipt.projection_runs_sealed, 2);
        assert_eq!(stats.identities, 2);
        assert_eq!(receipt.maximum_pending_source_pieces, 1);
    }

    #[test]
    fn logical_target_part_and_physical_owner_changes_each_force_a_flush() {
        let source = "abcd";
        let (mut document, epoch, root, paragraph, mut composer) = activate(source);
        let paragraph_identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let mut stats = RunStats::default();

        let cases = [
            (&root, CoveragePart::CONTENT),
            (&root, CoveragePart::BLOCK_MARKER),
            (&paragraph, CoveragePart::BLOCK_MARKER),
        ];
        for (owner, part) in cases {
            let atom = next_atom(&mut document, epoch).unwrap();
            let piece = document
                .candidate_consume_to(epoch, atom.boundary(), owner, part, &paragraph_identity)
                .unwrap();
            push(&mut document, epoch, &mut composer, piece, &mut stats);
        }
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        let heading = document
            .candidate_open_binding(epoch, GreenKind::HEADING)
            .unwrap();
        let heading_identity = CandidateLogicalAction::identity(&heading).unwrap();
        let atom = next_atom(&mut document, epoch).unwrap();
        let piece = document
            .candidate_consume_to(
                epoch,
                atom.boundary(),
                &root,
                CoveragePart::BLOCK_MARKER,
                &heading_identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, piece, &mut stats);
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &heading).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(receipt.source_pieces_consumed, 4);
        assert_eq!(receipt.projection_runs_sealed, 4);
        assert_eq!(stats.identities, 4);
        assert_eq!(receipt.maximum_pending_source_pieces, 1);
    }

    #[test]
    fn wrong_build_piece_and_permit_poison_without_returning_authority() {
        let (mut first_doc, first_epoch, first_root, first_paragraph, mut composer) = activate("a");
        let (mut other_doc, other_epoch, _other_root, other_paragraph, _other_composer) =
            activate("b");
        let other_atom = next_atom(&mut other_doc, other_epoch).unwrap();
        let other_identity = CandidateLogicalAction::identity(&other_paragraph).unwrap();
        let other_piece = other_doc
            .candidate_consume_to(
                other_epoch,
                other_atom.boundary(),
                &other_paragraph,
                CoveragePart::CONTENT,
                &other_identity,
            )
            .unwrap();
        assert_eq!(
            composer.push_piece(other_piece),
            Err(SourceProjectionComposerError::WrongCandidate)
        );
        assert_eq!(
            composer.poll(),
            Err(SourceProjectionComposerError::ComposerPoisoned)
        );

        // A fresh composer reaches one pending run, then a permit from the
        // other candidate burns that run and poisons the writer.
        let mut second_composer = SourceBoundProjectionComposer::begin(first_epoch);
        let atom = next_atom(&mut first_doc, first_epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&first_paragraph).unwrap();
        let piece = first_doc
            .candidate_consume_to(
                first_epoch,
                atom.boundary(),
                &first_paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        assert_eq!(
            second_composer.push_piece(piece).unwrap(),
            SourceProjectionComposerProgress::Idle
        );
        assert!(next_atom(&mut first_doc, first_epoch).is_none());
        first_doc.candidate_finish_line(first_epoch).unwrap();
        first_doc
            .candidate_close_binding(first_epoch, &first_paragraph)
            .unwrap();
        first_doc
            .candidate_close_binding(first_epoch, &first_root)
            .unwrap();
        let seal = first_doc.seal_candidate_source(first_epoch).unwrap();
        assert_eq!(
            second_composer.begin_finish(seal).unwrap(),
            SourceProjectionComposerProgress::RunReady
        );
        let wrong_permit = other_doc.mint_coverage_permit(other_epoch).unwrap();
        assert!(matches!(
            second_composer.seal_pending_run(wrong_permit),
            Err(SourceProjectionComposerError::WrongCoveragePermit)
        ));
        assert_eq!(
            second_composer.poll(),
            Err(SourceProjectionComposerError::ComposerPoisoned)
        );
    }

    #[test]
    fn out_of_order_piece_and_bad_seal_poison_the_candidate() {
        let (mut document, epoch, _root, paragraph, mut composer) = activate("ab");
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let a = next_atom(&mut document, epoch).unwrap();
        let first = document
            .candidate_consume_to(
                epoch,
                a.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        let b = next_atom(&mut document, epoch).unwrap();
        let second = document
            .candidate_consume_to(
                epoch,
                b.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        drop(first);
        assert_eq!(
            composer.push_piece(second),
            Err(SourceProjectionComposerError::OutOfOrderSource)
        );
        assert_eq!(
            composer.poll(),
            Err(SourceProjectionComposerError::ComposerPoisoned)
        );

        let (mut other, other_epoch, other_root, other_paragraph, mut other_composer) =
            activate("x");
        let x = next_atom(&mut other, other_epoch).unwrap();
        let x_identity = CandidateLogicalAction::identity(&other_paragraph).unwrap();
        let x = other
            .candidate_consume_to(
                other_epoch,
                x.boundary(),
                &other_paragraph,
                CoveragePart::CONTENT,
                &x_identity,
            )
            .unwrap();
        other_composer.push_piece(x).unwrap();
        assert!(next_atom(&mut other, other_epoch).is_none());
        other.candidate_finish_line(other_epoch).unwrap();
        other
            .candidate_close_binding(other_epoch, &other_paragraph)
            .unwrap();
        other
            .candidate_close_binding(other_epoch, &other_root)
            .unwrap();
        let other_seal = other.seal_candidate_source(other_epoch).unwrap();

        let (mut wrong, wrong_epoch, wrong_root, wrong_paragraph, _wrong_composer) = activate("y");
        let y = next_atom(&mut wrong, wrong_epoch).unwrap();
        let y_identity = CandidateLogicalAction::identity(&wrong_paragraph).unwrap();
        drop(
            wrong
                .candidate_consume_to(
                    wrong_epoch,
                    y.boundary(),
                    &wrong_paragraph,
                    CoveragePart::CONTENT,
                    &y_identity,
                )
                .unwrap(),
        );
        assert!(next_atom(&mut wrong, wrong_epoch).is_none());
        wrong.candidate_finish_line(wrong_epoch).unwrap();
        wrong
            .candidate_close_binding(wrong_epoch, &wrong_paragraph)
            .unwrap();
        wrong
            .candidate_close_binding(wrong_epoch, &wrong_root)
            .unwrap();
        let wrong_seal = wrong.seal_candidate_source(wrong_epoch).unwrap();
        assert_eq!(
            other_composer.begin_finish(wrong_seal),
            Err(SourceProjectionComposerError::WrongSourceSeal)
        );
        assert_eq!(
            other_composer.begin_finish(other_seal),
            Err(SourceProjectionComposerError::ComposerPoisoned)
        );
    }

    #[test]
    fn line_boundary_capture_requires_an_explicit_fully_drained_structural_flush() {
        let (_document, epoch, _root, _paragraph, composer) = activate("");
        let storage = mechanism_storage_ack(epoch, &composer);
        assert!(matches!(
            composer.pause_at_line_boundary(storage),
            Err(SourceProjectionComposerError::LineBoundaryNotReady)
        ));

        let (mut document, epoch, _root, paragraph, mut composer) = activate("a");
        let atom = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let piece = document
            .candidate_consume_to(
                epoch,
                atom.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        assert_eq!(
            composer.push_piece(piece).unwrap(),
            SourceProjectionComposerProgress::Idle
        );
        assert_eq!(
            composer.flush_before_structure().unwrap(),
            SourceProjectionComposerProgress::RunReady
        );
        let permit = document.mint_coverage_permit(epoch).unwrap();
        drop(composer.seal_pending_run(permit).unwrap());
        let storage = mechanism_storage_ack(epoch, &composer);
        assert!(matches!(
            composer.pause_at_line_boundary(storage),
            Err(SourceProjectionComposerError::LineBoundaryNotReady)
        ));
    }

    #[test]
    fn line_boundary_storage_rejects_wrong_build_and_metric() {
        let (_other_document, other_epoch, _, _, _) = activate("");

        let mut stats = RunStats::default();
        let (mut document, epoch, _, _, mut composer) = activate("");
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        let wrong_build = SourceProjectionLineBoundaryStorageAck::mechanism_only(
            other_epoch.build_id(),
            SerializedMetric::default(),
        );
        assert!(matches!(
            composer.pause_at_line_boundary(wrong_build),
            Err(SourceProjectionComposerError::WrongLineBoundaryStorage)
        ));

        let (mut document, epoch, _, _, mut composer) = activate("");
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        let wrong_metric = SourceProjectionLineBoundaryStorageAck::mechanism_only(
            epoch.build_id(),
            SerializedMetric { bytes: 1, utf16: 1 },
        );
        assert!(matches!(
            composer.pause_at_line_boundary(wrong_metric),
            Err(SourceProjectionComposerError::OutOfOrderSource)
        ));
    }

    #[test]
    fn line_boundary_resume_consumes_exact_epoch_and_cannot_be_replayed() {
        let (mut document, epoch, _, _, mut composer) = activate("");
        let mut stats = RunStats::default();
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        let storage = mechanism_storage_ack(epoch, &composer);
        let continuation = composer.pause_at_line_boundary(storage).unwrap();
        assert_eq!(
            continuation.receipt(),
            SourceProjectionComposerReceipt::default(),
            "the composite join observes the exact sealed-run receipt retained by the pause"
        );
        assert_eq!(continuation.retained_source_bytes_for_test(), 0);
        assert_eq!(continuation.retained_heap_bytes_for_test(), 0);
        // Relative to the original 192-byte continuation, two niche-packed
        // fragment-generation words and three monotonic normalization counters
        // add 40 bytes. The composer generation is exactly the sealed-run count
        // plus one, so the continuation derives it on resume instead of storing
        // a redundant eight-byte copy. The net 32 added bytes are scalar
        // transaction authority/receipts; source payload and heap retention
        // remain separately pinned to zero above.
        const MAX_LINE_BOUNDARY_CONTINUATION_BYTES: usize = 224;
        let continuation_bytes = std::mem::size_of_val(&continuation);
        assert!(
            continuation_bytes <= MAX_LINE_BOUNDARY_CONTINUATION_BYTES,
            "line-boundary continuation retained {continuation_bytes} bytes, more than bounded scalar state"
        );

        let (_other_document, other_epoch, _, _, _) = activate("different source");
        assert_ne!(other_epoch.source(), epoch.source());
        assert!(matches!(
            SourceBoundProjectionComposer::resume_line_boundary(other_epoch, continuation),
            Err(SourceProjectionComposerError::WrongLineBoundaryStorage)
        ));

        // This assignment pins the API contract: resume takes the opaque token
        // by value. Because the token implements neither Copy nor Clone, a
        // second call with the same value is rejected by the Rust compiler.
        let _: fn(
            LiveCandidateEpoch,
            SourceProjectionComposerLineBoundaryContinuation,
        ) -> Result<
            (
                SourceBoundProjectionComposer,
                SourceProjectionLineBoundaryStorageAck,
            ),
            SourceProjectionComposerError,
        > = SourceBoundProjectionComposer::resume_line_boundary;
    }

    #[test]
    fn empty_and_no_new_run_boundaries_resume_repeatedly_without_scratch() {
        let (mut document, epoch, root, paragraph, mut composer) = activate("x");
        let mut stats = RunStats::default();
        for _ in 0..8 {
            drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
            composer = pause_and_resume(epoch, composer);
            assert_eq!(composer.next_source, SerializedMetric::default());
            assert_eq!(composer.next_composer_generation, 1);
            assert_eq!(
                composer.receipt(),
                SourceProjectionComposerReceipt::default()
            );
            assert!(composer.envelope.is_none());
        }
        let x = next_atom(&mut document, epoch).unwrap();
        let identity = CandidateLogicalAction::identity(&paragraph).unwrap();
        let x = document
            .candidate_consume_to(
                epoch,
                x.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &identity,
            )
            .unwrap();
        push(&mut document, epoch, &mut composer, x, &mut stats);
        assert!(next_atom(&mut document, epoch).is_none());
        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(receipt.source_pieces_consumed, 1);
        assert_eq!(receipt.projection_runs_sealed, 1);
        assert_eq!(stats.runs, 1);
    }

    #[test]
    fn line_boundary_continuation_survives_a_program_chunk_boundary() {
        const PIECES: usize = 5_000;
        let source = "\t".repeat(PIECES);
        let (mut document, epoch, root, paragraph, mut composer) = activate(&source);
        let mut stats = RunStats::default();
        while let Some(atom) = next_atom(&mut document, epoch) {
            assert_eq!(atom.kind(), CandidateSourceAtomKind::Tab);
            let logical = CandidateLogicalAction::tab_to_spaces(&paragraph, &atom, 4).unwrap();
            let piece = document
                .candidate_consume_to(
                    epoch,
                    atom.boundary(),
                    &paragraph,
                    CoveragePart::CONTENT,
                    &logical,
                )
                .unwrap();
            push(&mut document, epoch, &mut composer, piece, &mut stats);
        }
        drain_structural_flush(&mut document, epoch, &mut composer, &mut stats);
        assert!(composer.receipt.projection_runs_sealed > 1);
        let before = composer.receipt();
        composer = pause_and_resume(epoch, composer);
        assert_eq!(composer.receipt(), before);
        assert_eq!(
            composer.next_source.bytes,
            u64::try_from(source.len()).unwrap()
        );
        assert_eq!(composer.next_source.utf16, u64::try_from(PIECES).unwrap());
        assert!(composer.envelope.is_none());

        document.candidate_finish_line(epoch).unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        let receipt = finish(&mut document, epoch, &mut composer, seal, &mut stats);
        assert_eq!(receipt, before);
        assert_eq!(stats.bytes, u64::try_from(source.len()).unwrap());
    }

    #[test]
    fn paused_and_uninterrupted_composition_are_exactly_equivalent() {
        let uninterrupted = compose_two_lines(false);
        let resumed = compose_two_lines(true);
        assert_eq!(resumed, uninterrupted);
        assert_eq!(resumed.0.source_pieces_consumed, 3);
        assert_eq!(resumed.0.projection_runs_sealed, 2);
        assert_eq!((resumed.1.bytes, resumed.1.utf16), (4, 4));
    }

    #[test]
    fn actor_admits_only_one_composer_and_cancellation_returns_no_authority() {
        let (mut document, epoch, _root, _paragraph, composer) = activate("a");
        assert!(matches!(
            document.begin_source_projection_composer(epoch),
            Err(LiveDocumentError::Invariant(
                "candidate projection composer already admitted"
            ))
        ));
        let receipt = composer.cancel();
        assert_eq!(receipt.source_pieces_consumed, 0);
        assert_eq!(receipt.projection_runs_sealed, 0);
    }
}

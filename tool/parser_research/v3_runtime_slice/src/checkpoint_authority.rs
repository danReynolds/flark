//! Storage/source checkpoint-boundary authority without parser-state claims.
//!
//! The executable capabilities in this module are deliberately named
//! `Mechanism`: the current green manifest has no restart-state index. Only a
//! crate-private linear index-entry permit can mint one, and the only permit
//! constructor in this slice is compiled for the proof harness. Production
//! parser checkpoint and candidate-cursor binding therefore remain a HOLD.

use std::fmt;

use crate::serialized_green::source_boundary_resolver::{
    SerializedGreenAdjacentCoverageSide, SerializedGreenCoverageSideBias,
    SerializedGreenCoverageSideObservation, SerializedGreenCoverageSideOutcome,
};
use crate::{
    BoundaryAffinity, LineageAdoptionBundleJob, LineageAdoptionBundleProof,
    LineageAdoptionBundleStatus, PageArena, ParseGeneration, ProjectionResetSeekOutcome,
    ProvenSourceMapping, RestartSelectionJob, RestartSelectionProof, RestartSelectionStatus,
    SerializedGreenDocument, SerializedGreenError, SerializedGreenManifestId, SerializedMetric,
    SourceSnapshotDescriptor, SourceStore,
};

/// Decoded identity and generation fields from one revalidated green manifest.
///
/// This is observation data. Authority comes from resolving it through the
/// live `SerializedGreenDocument`, not from constructing an equal value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BaseManifestBinding {
    manifest: SerializedGreenManifestId,
    source: SourceSnapshotDescriptor,
    source_utf16: u64,
    syntax_profile: u64,
    grammar_revision: crate::GrammarRevision,
    parse_generation: ParseGeneration,
    semantic_epoch: u64,
    known_bytes_start: u64,
    known_bytes_end: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct StoredBoundaryMechanism {
    base: BaseManifestBinding,
    adjacent_coverage: SerializedGreenCoverageSideObservation,
    receipt: StorageBoundaryMechanismReceipt,
}

/// O(1) retained receipt from proof-harness boundary resolution.
///
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StorageBoundaryMechanismReceipt {
    pub adjacent_source_bytes_read: usize,
    pub root_descents: usize,
    pub sequence_nodes_read: usize,
    pub maximum_route_depth: usize,
    pub leaf_pages_scanned: usize,
    pub events_inspected: usize,
    pub borrowed_leaf_payload_bytes: usize,
    pub decoded_event_capacity: usize,
    pub maximum_transient_event_heap_bytes: usize,
    pub maximum_scanner_scratch_bytes: usize,
    pub retained_heap_bytes: usize,
}

/// Storage-bound restart role. It is not a parser restart checkpoint because
/// the manifest does not yet carry parser control or semantic restart state.
///
/// ```compile_fail
/// use flark_v3_runtime_slice::{
///     StoredConvergenceCheckpointMechanism, StoredRestartCheckpointMechanism,
/// };
/// fn swap(
///     convergence: StoredConvergenceCheckpointMechanism,
/// ) -> StoredRestartCheckpointMechanism {
///     convergence
/// }
/// ```
#[must_use = "restart-boundary authority must be consumed or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct StoredRestartCheckpointMechanism {
    boundary: StoredBoundaryMechanism,
}

impl StoredRestartCheckpointMechanism {
    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.boundary.base.manifest
    }

    #[must_use]
    pub const fn base_source(&self) -> SourceSnapshotDescriptor {
        self.boundary.base.source
    }

    /// Diagnostic coordinate only. No public API accepts this scalar in place
    /// of the capability.
    #[must_use]
    pub const fn source_byte_cut(&self) -> u64 {
        self.boundary.adjacent_coverage.source_cut.bytes
    }

    #[must_use]
    pub const fn source_utf16_cut(&self) -> u64 {
        self.boundary.adjacent_coverage.source_cut.utf16
    }

    #[must_use]
    pub const fn syntax_profile(&self) -> u64 {
        self.boundary.base.syntax_profile
    }

    #[must_use]
    pub const fn grammar_revision(&self) -> crate::GrammarRevision {
        self.boundary.base.grammar_revision
    }

    #[must_use]
    pub const fn parse_generation(&self) -> ParseGeneration {
        self.boundary.base.parse_generation
    }

    /// Makes the current storage-only status queryable rather than implying
    /// that a parser restart state was recovered.
    #[must_use]
    pub const fn has_parser_restart_state(&self) -> bool {
        false
    }

    /// An adjacent Coverage observation cannot choose among intervening
    /// zero-metric structural events and is not a general green sequence cut.
    #[must_use]
    pub const fn has_sequence_cut_authority(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn receipt(&self) -> StorageBoundaryMechanismReceipt {
        self.boundary.receipt
    }

    pub(crate) const fn adjacent_coverage_observation(
        &self,
    ) -> &SerializedGreenCoverageSideObservation {
        &self.boundary.adjacent_coverage
    }

    /// Bounded storage join for the distinct projection-reset role. The
    /// restart mechanism supplies the exact manifest-bound adjacent-Coverage
    /// observation; no raw coordinate or generic green cursor is accepted.
    pub fn previous_projection_reset_mechanism(
        &self,
        document: &SerializedGreenDocument,
        arena: &PageArena,
        maximum_pages: usize,
    ) -> Result<ProjectionResetSeekOutcome, CheckpointMechanismError> {
        document
            .previous_projection_reset_from_observation(
                arena,
                self.adjacent_coverage_observation(),
                maximum_pages,
            )
            .map_err(CheckpointMechanismError::Green)
    }
}

/// Storage-bound convergence role. It is intentionally not interchangeable
/// with [`StoredRestartCheckpointMechanism`].
#[must_use = "convergence-boundary authority must be consumed or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct StoredConvergenceCheckpointMechanism {
    boundary: StoredBoundaryMechanism,
    affinity: BoundaryAffinity,
}

impl StoredConvergenceCheckpointMechanism {
    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.boundary.base.manifest
    }

    #[must_use]
    pub const fn base_source(&self) -> SourceSnapshotDescriptor {
        self.boundary.base.source
    }

    /// Diagnostic coordinate only. The lineage adapter derives its private raw
    /// Stage-0 input from the consumed capability.
    #[must_use]
    pub const fn source_byte_cut(&self) -> u64 {
        self.boundary.adjacent_coverage.source_cut.bytes
    }

    #[must_use]
    pub const fn source_utf16_cut(&self) -> u64 {
        self.boundary.adjacent_coverage.source_cut.utf16
    }

    #[must_use]
    pub const fn affinity(&self) -> BoundaryAffinity {
        self.affinity
    }

    #[must_use]
    pub const fn has_parser_convergence_state(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn has_sequence_cut_authority(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn receipt(&self) -> StorageBoundaryMechanismReceipt {
        self.boundary.receipt
    }
}

/// Typed failure from the storage-boundary/lineage mechanism layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointMechanismError {
    Green(SerializedGreenError),
    ManifestSourceLengthOverflow,
    ManifestSourceMismatch {
        manifest: SourceSnapshotDescriptor,
        store: SourceSnapshotDescriptor,
    },
    ManifestUtf16Mismatch {
        manifest: u64,
        store: u64,
    },
    NotScalarBoundary,
    NotPhysicalLineStart,
    NotCoverageBoundary,
    CheckpointOutsideKnownRange,
    BaseManifestMismatch,
    InvalidZeroFallback,
    SourceAdvanced {
        expected: SourceSnapshotDescriptor,
        actual: SourceSnapshotDescriptor,
    },
    LineageRejected,
    InconsistentLineageProof,
}

impl From<SerializedGreenError> for CheckpointMechanismError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

impl fmt::Display for CheckpointMechanismError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Green(error) => error.fmt(formatter),
            Self::ManifestSourceLengthOverflow => {
                formatter.write_str("green manifest source length exceeds this target")
            }
            Self::ManifestSourceMismatch { .. } => {
                formatter.write_str("green manifest and source store identify different sources")
            }
            Self::ManifestUtf16Mismatch { .. } => {
                formatter.write_str("green manifest and source store have different UTF-16 metrics")
            }
            Self::NotScalarBoundary => formatter.write_str("checkpoint splits a UTF-8 scalar"),
            Self::NotPhysicalLineStart => {
                formatter.write_str("checkpoint is not a physical-line start")
            }
            Self::NotCoverageBoundary => {
                formatter.write_str("checkpoint is not a serialized source-run boundary")
            }
            Self::CheckpointOutsideKnownRange => {
                formatter.write_str("checkpoint is outside the manifest's exact known range")
            }
            Self::BaseManifestMismatch => {
                formatter.write_str("checkpoint roles name different base manifests")
            }
            Self::InvalidZeroFallback => {
                formatter.write_str("restart fallback is not the manifest's exact zero cut")
            }
            Self::SourceAdvanced { .. } => {
                formatter.write_str("source advanced after the lineage mechanism froze its target")
            }
            Self::LineageRejected => formatter.write_str("source lineage rejected the checkpoint"),
            Self::InconsistentLineageProof => {
                formatter.write_str("lineage proof does not reproduce the stored checkpoint cuts")
            }
        }
    }
}

impl std::error::Error for CheckpointMechanismError {}

/// Future manifest checkpoint-index entry. Its fields and constructor remain
/// private; production code currently has no way to mint it.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // Future restart-index resolver is intentionally not implemented yet.
struct StoredRestartIndexEntryPermit {
    byte_cut: usize,
    coverage_side: SerializedGreenCoverageSideBias,
}

/// Role-distinct future convergence-index entry.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)] // Role-distinct future index entry; proof harness is the only current mint.
struct StoredConvergenceIndexEntryPermit {
    byte_cut: usize,
    coverage_side: SerializedGreenCoverageSideBias,
    affinity: BoundaryAffinity,
}

#[cfg(test)]
fn restart_index_entry_for_proof_harness(byte_cut: usize) -> StoredRestartIndexEntryPermit {
    StoredRestartIndexEntryPermit {
        byte_cut,
        coverage_side: SerializedGreenCoverageSideBias::BeforeFollowing,
    }
}

#[cfg(test)]
fn restart_index_entry_with_side_for_proof_harness(
    byte_cut: usize,
    coverage_side: SerializedGreenCoverageSideBias,
) -> StoredRestartIndexEntryPermit {
    StoredRestartIndexEntryPermit {
        byte_cut,
        coverage_side,
    }
}

#[cfg(test)]
fn convergence_index_entry_for_proof_harness(
    byte_cut: usize,
    affinity: BoundaryAffinity,
) -> StoredConvergenceIndexEntryPermit {
    StoredConvergenceIndexEntryPermit {
        byte_cut,
        coverage_side: SerializedGreenCoverageSideBias::BeforeFollowing,
        affinity,
    }
}

fn resolve_boundary_mechanism(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    source: &SourceStore,
    byte_cut: usize,
    coverage_side: SerializedGreenCoverageSideBias,
) -> Result<StoredBoundaryMechanism, CheckpointMechanismError> {
    let line = source.check_physical_line_start(byte_cut);
    if !line.scalar_boundary {
        return Err(CheckpointMechanismError::NotScalarBoundary);
    }
    if !line.physical_line_start {
        return Err(CheckpointMechanismError::NotPhysicalLineStart);
    }
    let byte_cut_u64 = u64::try_from(byte_cut)
        .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
    let (adjacent_coverage, storage_receipt) =
        match document.resolve_storage_source_coverage_side(arena, byte_cut_u64, coverage_side)? {
            SerializedGreenCoverageSideOutcome::Found {
                observation,
                receipt,
            } => (observation, receipt),
            SerializedGreenCoverageSideOutcome::NoAdjacentCoverage(_)
            | SerializedGreenCoverageSideOutcome::NotCoverageBoundary(_) => {
                return Err(CheckpointMechanismError::NotCoverageBoundary);
            }
        };
    let manifest = adjacent_coverage.manifest;
    let source_bytes = usize::try_from(manifest.source_bytes)
        .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
    let manifest_source = SourceSnapshotDescriptor {
        revision: manifest.source_revision,
        root: manifest.source_root,
        bytes: source_bytes,
    };
    let store_source = source.descriptor();
    if manifest_source != store_source {
        return Err(CheckpointMechanismError::ManifestSourceMismatch {
            manifest: manifest_source,
            store: store_source,
        });
    }
    let store_utf16 = u64::try_from(source.query_view().len_utf16())
        .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
    if manifest.source_utf16 != store_utf16 {
        return Err(CheckpointMechanismError::ManifestUtf16Mismatch {
            manifest: manifest.source_utf16,
            store: store_utf16,
        });
    }
    if byte_cut != 0
        && (byte_cut_u64 < manifest.known_bytes_start || byte_cut_u64 > manifest.known_bytes_end)
    {
        return Err(CheckpointMechanismError::CheckpointOutsideKnownRange);
    }
    if adjacent_coverage.source_cut.bytes != byte_cut_u64 {
        return Err(CheckpointMechanismError::NotCoverageBoundary);
    }
    let coverage_manifest = match adjacent_coverage.adjacent {
        SerializedGreenAdjacentCoverageSide::EmptyDocument { manifest } => manifest,
        SerializedGreenAdjacentCoverageSide::BeforeFollowing(capability)
        | SerializedGreenAdjacentCoverageSide::AfterPreceding(capability) => capability.manifest,
    };
    if coverage_manifest != manifest.manifest {
        return Err(CheckpointMechanismError::BaseManifestMismatch);
    }
    Ok(StoredBoundaryMechanism {
        base: BaseManifestBinding {
            manifest: manifest.manifest,
            source: manifest_source,
            source_utf16: manifest.source_utf16,
            syntax_profile: manifest.syntax_profile,
            grammar_revision: manifest.grammar_revision,
            parse_generation: manifest.parse_generation,
            semantic_epoch: manifest.semantic_epoch,
            known_bytes_start: manifest.known_bytes_start,
            known_bytes_end: manifest.known_bytes_end,
        },
        adjacent_coverage,
        receipt: StorageBoundaryMechanismReceipt {
            adjacent_source_bytes_read: line.adjacent_bytes_read,
            root_descents: storage_receipt.root_descents,
            sequence_nodes_read: storage_receipt.sequence_nodes_read,
            maximum_route_depth: storage_receipt.maximum_route_depth,
            leaf_pages_scanned: storage_receipt.leaf_pages_scanned,
            events_inspected: storage_receipt.events_inspected,
            borrowed_leaf_payload_bytes: storage_receipt.borrowed_leaf_payload_bytes,
            decoded_event_capacity: storage_receipt.decoded_event_capacity,
            maximum_transient_event_heap_bytes: storage_receipt.maximum_transient_event_heap_bytes,
            maximum_scanner_scratch_bytes: storage_receipt.maximum_scanner_scratch_bytes,
            retained_heap_bytes: storage_receipt.retained_heap_bytes,
        },
    })
}

#[allow(dead_code)] // Production has no restart-index permit mint in this slice.
#[allow(clippy::needless_pass_by_value)] // Linear future index authority is intentionally consumed.
fn resolve_restart_checkpoint_mechanism(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    source: &SourceStore,
    permit: StoredRestartIndexEntryPermit,
) -> Result<StoredRestartCheckpointMechanism, CheckpointMechanismError> {
    let StoredRestartIndexEntryPermit {
        byte_cut,
        coverage_side,
    } = permit;
    Ok(StoredRestartCheckpointMechanism {
        boundary: resolve_boundary_mechanism(document, arena, source, byte_cut, coverage_side)?,
    })
}

#[allow(dead_code)] // Production has no convergence-index permit mint in this slice.
#[allow(clippy::needless_pass_by_value)] // Role-distinct linear authority is consumed.
fn resolve_convergence_checkpoint_mechanism(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    source: &SourceStore,
    permit: StoredConvergenceIndexEntryPermit,
) -> Result<StoredConvergenceCheckpointMechanism, CheckpointMechanismError> {
    let StoredConvergenceIndexEntryPermit {
        byte_cut,
        coverage_side,
        affinity,
    } = permit;
    Ok(StoredConvergenceCheckpointMechanism {
        boundary: resolve_boundary_mechanism(document, arena, source, byte_cut, coverage_side)?,
        affinity,
    })
}

/// One preferred restart plus the independently storage-resolved zero fallback.
#[must_use = "restart selection mechanisms must be consumed or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct StoredRestartSelectionMechanisms {
    preferred: StoredRestartCheckpointMechanism,
    zero: StoredRestartCheckpointMechanism,
}

impl StoredRestartSelectionMechanisms {
    pub fn new(
        preferred: StoredRestartCheckpointMechanism,
        zero: StoredRestartCheckpointMechanism,
    ) -> Result<Self, CheckpointMechanismError> {
        if preferred.boundary.base != zero.boundary.base {
            return Err(CheckpointMechanismError::BaseManifestMismatch);
        }
        if zero.boundary.adjacent_coverage.source_cut != SerializedMetric::default()
            || !matches!(
                zero.boundary.adjacent_coverage.adjacent,
                SerializedGreenAdjacentCoverageSide::BeforeFollowing(_)
                    | SerializedGreenAdjacentCoverageSide::EmptyDocument { .. }
            )
        {
            return Err(CheckpointMechanismError::InvalidZeroFallback);
        }
        Ok(Self { preferred, zero })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartCheckpointSelectionMechanismStatus {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    Selected {
        old: usize,
        current: usize,
        used_zero_fallback: bool,
    },
    Rejected(CheckpointMechanismError),
}

/// High-level adapter over Stage-0 restart selection. Its public constructor
/// accepts storage capabilities, never a source descriptor or byte offset.
#[derive(Debug)]
pub struct RestartCheckpointSelectionMechanismJob {
    mechanisms: Option<StoredRestartSelectionMechanisms>,
    inner: Option<RestartSelectionJob>,
    frozen_target: SourceSnapshotDescriptor,
    terminal_error: Option<CheckpointMechanismError>,
}

impl RestartCheckpointSelectionMechanismJob {
    pub fn begin(
        source: &SourceStore,
        mechanisms: StoredRestartSelectionMechanisms,
    ) -> Result<Self, CheckpointMechanismError> {
        let base = mechanisms.preferred.boundary.base.source;
        let preferred = usize::try_from(
            mechanisms
                .preferred
                .boundary
                .adjacent_coverage
                .source_cut
                .bytes,
        )
        .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
        let inner = source
            .begin_restart_selection(base, preferred)
            .map_err(|_| CheckpointMechanismError::LineageRejected)?;
        Ok(Self {
            mechanisms: Some(mechanisms),
            inner: Some(inner),
            frozen_target: source.descriptor(),
            terminal_error: None,
        })
    }

    #[must_use]
    pub fn poll(
        &mut self,
        source: &SourceStore,
        fuel: usize,
    ) -> RestartCheckpointSelectionMechanismStatus {
        if let Some(error) = self.terminal_error {
            return RestartCheckpointSelectionMechanismStatus::Rejected(error);
        }
        let actual = source.descriptor();
        if actual != self.frozen_target {
            let error = CheckpointMechanismError::SourceAdvanced {
                expected: self.frozen_target,
                actual,
            };
            self.terminal_error = Some(error);
            return RestartCheckpointSelectionMechanismStatus::Rejected(error);
        }
        let Some(inner) = &mut self.inner else {
            let error = CheckpointMechanismError::InconsistentLineageProof;
            self.terminal_error = Some(error);
            return RestartCheckpointSelectionMechanismStatus::Rejected(error);
        };
        match inner.poll(fuel) {
            RestartSelectionStatus::Pending {
                processed_records,
                remaining_records,
            } => RestartCheckpointSelectionMechanismStatus::Pending {
                processed_records,
                remaining_records,
            },
            RestartSelectionStatus::Selected {
                old,
                current,
                used_zero_fallback,
            } => RestartCheckpointSelectionMechanismStatus::Selected {
                old,
                current,
                used_zero_fallback,
            },
            RestartSelectionStatus::Failed(_) => {
                self.terminal_error = Some(CheckpointMechanismError::LineageRejected);
                RestartCheckpointSelectionMechanismStatus::Rejected(
                    CheckpointMechanismError::LineageRejected,
                )
            }
        }
    }

    pub fn into_selected(
        mut self,
        source: &SourceStore,
    ) -> Result<SelectedRestartCheckpointMechanism, CheckpointMechanismError> {
        if let Some(error) = self.terminal_error {
            return Err(error);
        }
        let actual = source.descriptor();
        if actual != self.frozen_target {
            return Err(CheckpointMechanismError::SourceAdvanced {
                expected: self.frozen_target,
                actual,
            });
        }
        let inner = self
            .inner
            .take()
            .ok_or(CheckpointMechanismError::InconsistentLineageProof)?;
        let proof = inner
            .into_proof()
            .map_err(|_| CheckpointMechanismError::LineageRejected)?;
        let (old, current) = proof.selected_boundaries();
        let mechanisms = self
            .mechanisms
            .take()
            .ok_or(CheckpointMechanismError::InconsistentLineageProof)?;
        let preferred_cut = usize::try_from(
            mechanisms
                .preferred
                .boundary
                .adjacent_coverage
                .source_cut
                .bytes,
        )
        .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
        let (chosen, used_zero_fallback) = if old == preferred_cut {
            (mechanisms.preferred, false)
        } else if old == 0 {
            (mechanisms.zero, true)
        } else {
            return Err(CheckpointMechanismError::InconsistentLineageProof);
        };
        if proof.from() != chosen.boundary.base.source || proof.to() != self.frozen_target {
            return Err(CheckpointMechanismError::InconsistentLineageProof);
        }
        Ok(SelectedRestartCheckpointMechanism {
            chosen,
            proof,
            current_byte_cut: current,
            used_zero_fallback,
        })
    }
}

/// Consumed restart choice, still storage/lineage mechanism only. It does not
/// own or authorize a parser cursor.
#[must_use = "selected restart mechanism must feed lineage adoption or be discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct SelectedRestartCheckpointMechanism {
    chosen: StoredRestartCheckpointMechanism,
    proof: RestartSelectionProof,
    current_byte_cut: usize,
    used_zero_fallback: bool,
}

impl SelectedRestartCheckpointMechanism {
    #[must_use]
    pub const fn base_source(&self) -> SourceSnapshotDescriptor {
        self.chosen.boundary.base.source
    }

    #[must_use]
    pub const fn target_source(&self) -> SourceSnapshotDescriptor {
        self.proof.to()
    }

    #[must_use]
    pub const fn used_zero_fallback(&self) -> bool {
        self.used_zero_fallback
    }

    /// Diagnostic only; the future actor binding must seed its own cursor from
    /// the consumed selection rather than accept this value back.
    #[must_use]
    pub const fn current_byte_cut(&self) -> usize {
        self.current_byte_cut
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredLineageAdoptionMechanismStatus {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    Proven {
        restart: usize,
        convergence: usize,
    },
    Rejected(CheckpointMechanismError),
}

/// One-pass lineage adapter deriving every raw Stage-0 input from consumed,
/// role-distinct storage mechanisms.
#[derive(Debug)]
pub struct StoredLineageAdoptionMechanismJob {
    restart: Option<SelectedRestartCheckpointMechanism>,
    convergence: Option<StoredConvergenceCheckpointMechanism>,
    inner: Option<LineageAdoptionBundleJob>,
    frozen_target: SourceSnapshotDescriptor,
    terminal_error: Option<CheckpointMechanismError>,
}

impl StoredLineageAdoptionMechanismJob {
    pub fn begin(
        source: &SourceStore,
        restart: SelectedRestartCheckpointMechanism,
        convergence: StoredConvergenceCheckpointMechanism,
    ) -> Result<Self, CheckpointMechanismError> {
        if restart.chosen.boundary.base != convergence.boundary.base {
            return Err(CheckpointMechanismError::BaseManifestMismatch);
        }
        let actual = source.descriptor();
        if restart.proof.to() != actual {
            return Err(CheckpointMechanismError::SourceAdvanced {
                expected: restart.proof.to(),
                actual,
            });
        }
        let old_restart =
            usize::try_from(restart.chosen.boundary.adjacent_coverage.source_cut.bytes)
                .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
        let old_convergence =
            usize::try_from(convergence.boundary.adjacent_coverage.source_cut.bytes)
                .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
        let inner = source
            .begin_lineage_adoption_bundle(
                restart.chosen.boundary.base.source,
                old_restart,
                old_convergence,
                convergence.affinity,
            )
            .map_err(|_| CheckpointMechanismError::LineageRejected)?;
        Ok(Self {
            restart: Some(restart),
            convergence: Some(convergence),
            inner: Some(inner),
            frozen_target: actual,
            terminal_error: None,
        })
    }

    #[must_use]
    pub fn poll(
        &mut self,
        source: &SourceStore,
        fuel: usize,
    ) -> StoredLineageAdoptionMechanismStatus {
        if let Some(error) = self.terminal_error {
            return StoredLineageAdoptionMechanismStatus::Rejected(error);
        }
        let actual = source.descriptor();
        if actual != self.frozen_target {
            let error = CheckpointMechanismError::SourceAdvanced {
                expected: self.frozen_target,
                actual,
            };
            self.terminal_error = Some(error);
            return StoredLineageAdoptionMechanismStatus::Rejected(error);
        }
        let Some(inner) = &mut self.inner else {
            let error = CheckpointMechanismError::InconsistentLineageProof;
            self.terminal_error = Some(error);
            return StoredLineageAdoptionMechanismStatus::Rejected(error);
        };
        match inner.poll(fuel) {
            LineageAdoptionBundleStatus::Pending {
                processed_records,
                remaining_records,
            } => StoredLineageAdoptionMechanismStatus::Pending {
                processed_records,
                remaining_records,
            },
            LineageAdoptionBundleStatus::Proven {
                restart,
                convergence,
                ..
            } => StoredLineageAdoptionMechanismStatus::Proven {
                restart,
                convergence,
            },
            LineageAdoptionBundleStatus::Changed { .. }
            | LineageAdoptionBundleStatus::Failed(_) => {
                self.terminal_error = Some(CheckpointMechanismError::LineageRejected);
                StoredLineageAdoptionMechanismStatus::Rejected(
                    CheckpointMechanismError::LineageRejected,
                )
            }
        }
    }

    pub fn into_proof(
        mut self,
        source: &SourceStore,
    ) -> Result<StoredLineageAdoptionMechanismProof, CheckpointMechanismError> {
        if let Some(error) = self.terminal_error {
            return Err(error);
        }
        let actual = source.descriptor();
        if actual != self.frozen_target {
            return Err(CheckpointMechanismError::SourceAdvanced {
                expected: self.frozen_target,
                actual,
            });
        }
        let restart = self
            .restart
            .take()
            .ok_or(CheckpointMechanismError::InconsistentLineageProof)?;
        let convergence = self
            .convergence
            .take()
            .ok_or(CheckpointMechanismError::InconsistentLineageProof)?;
        let lineage = self
            .inner
            .take()
            .ok_or(CheckpointMechanismError::InconsistentLineageProof)?
            .into_proof()
            .map_err(|_| CheckpointMechanismError::LineageRejected)?;
        let old_restart =
            usize::try_from(restart.chosen.boundary.adjacent_coverage.source_cut.bytes)
                .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
        let old_convergence =
            usize::try_from(convergence.boundary.adjacent_coverage.source_cut.bytes)
                .map_err(|_| CheckpointMechanismError::ManifestSourceLengthOverflow)?;
        let restart_mapping_matches = matches!(
            lineage.restart(),
            ProvenSourceMapping::Boundary {
                from,
                to,
                affinity: BoundaryAffinity::Before,
            } if *from == old_restart && *to == restart.current_byte_cut
        );
        let convergence_mapping_matches = matches!(
            lineage.convergence(),
            ProvenSourceMapping::Boundary { from, affinity, .. }
                if *from == old_convergence && *affinity == convergence.affinity
        );
        if lineage.from() != restart.chosen.boundary.base.source
            || lineage.to() != self.frozen_target
            || !restart_mapping_matches
            || !convergence_mapping_matches
        {
            return Err(CheckpointMechanismError::InconsistentLineageProof);
        }
        Ok(StoredLineageAdoptionMechanismProof {
            restart,
            convergence,
            lineage,
        })
    }
}

/// Complete storage/source lineage mechanism proof. It still owns no arena
/// suffix and has no attach/commit operation.
#[must_use = "lineage mechanism proof must feed the future storage resolver or be discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct StoredLineageAdoptionMechanismProof {
    restart: SelectedRestartCheckpointMechanism,
    convergence: StoredConvergenceCheckpointMechanism,
    lineage: LineageAdoptionBundleProof,
}

impl StoredLineageAdoptionMechanismProof {
    #[must_use]
    pub const fn base_source(&self) -> SourceSnapshotDescriptor {
        self.lineage.from()
    }

    #[must_use]
    pub const fn target_source(&self) -> SourceSnapshotDescriptor {
        self.lineage.to()
    }

    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.restart.chosen.boundary.base.manifest
    }

    #[must_use]
    pub const fn convergence_manifest(&self) -> SerializedGreenManifestId {
        self.convergence.boundary.base.manifest
    }

    #[must_use]
    pub const fn has_suffix_attachment_authority(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::*;
    use crate::{
        ARENA_PAGE_BYTES, BlockId, ClosedChildAggregate, CoverageId, CoveragePart, CoverageRun,
        FactsEnvelope, GrammarRevision, GreenEvent, GreenKind, SerializedGreenBuildReceipt,
        SerializedGreenRootSpec, SourceRevision,
    };

    fn enter(block: u64, kind: GreenKind) -> GreenEvent {
        GreenEvent::enter(BlockId(block), kind, FactsEnvelope::empty())
    }

    fn exit() -> GreenEvent {
        GreenEvent::exit(ClosedChildAggregate::default())
    }

    fn build_document(
        arena: &mut PageArena,
        source: &SourceStore,
        syntax_profile: u64,
    ) -> SerializedGreenDocument {
        let descriptor = source.descriptor();
        let events = [
            enter(1, GreenKind::DOCUMENT),
            enter(2, GreenKind::PARAGRAPH),
            GreenEvent::Coverage(
                CoverageRun::new(CoverageId(1), 3, 3, 0, CoveragePart::CONTENT).unwrap(),
            ),
            GreenEvent::Coverage(
                CoverageRun::new(CoverageId(2), 3, 3, 0, CoveragePart::CONTENT).unwrap(),
            ),
            GreenEvent::Coverage(
                CoverageRun::new(CoverageId(3), 2, 2, 0, CoveragePart::CONTENT).unwrap(),
            ),
            exit(),
            exit(),
        ];
        SerializedGreenDocument::build(
            arena,
            SerializedGreenRootSpec {
                syntax_profile,
                source_revision: descriptor.revision,
                source_root: descriptor.root,
                source_bytes: u64::try_from(descriptor.bytes).unwrap(),
                source_utf16: u64::try_from(source.query_view().len_utf16()).unwrap(),
                grammar_revision: GrammarRevision(7),
                parse_generation: ParseGeneration(11),
                semantic_epoch: 13,
                known_bytes: 0..u64::try_from(descriptor.bytes).unwrap(),
            },
            events,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap()
    }

    fn build_single_run_document(
        arena: &mut PageArena,
        source: &SourceStore,
    ) -> SerializedGreenDocument {
        let descriptor = source.descriptor();
        let utf16 = u64::try_from(source.query_view().len_utf16()).unwrap();
        let bytes = u64::try_from(descriptor.bytes).unwrap();
        SerializedGreenDocument::build(
            arena,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: descriptor.revision,
                source_root: descriptor.root,
                source_bytes: bytes,
                source_utf16: utf16,
                grammar_revision: GrammarRevision(7),
                parse_generation: ParseGeneration(11),
                semantic_epoch: 13,
                known_bytes: 0..bytes,
            },
            [
                enter(1, GreenKind::DOCUMENT),
                enter(2, GreenKind::PARAGRAPH),
                GreenEvent::Coverage(
                    CoverageRun::new(CoverageId(1), bytes, utf16, 0, CoveragePart::CONTENT)
                        .unwrap(),
                ),
                exit(),
                exit(),
            ],
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap()
    }

    fn build_deep_document(
        arena: &mut PageArena,
        source: &SourceStore,
        depth: usize,
    ) -> SerializedGreenDocument {
        let descriptor = source.descriptor();
        let bytes = u64::try_from(descriptor.bytes).unwrap();
        let mut events = Vec::with_capacity(depth.saturating_mul(2).saturating_add(5));
        events.push(enter(1, GreenKind::DOCUMENT));
        for index in 0..depth {
            events.push(enter(
                u64::try_from(index).unwrap() + 2,
                GreenKind::BLOCK_QUOTE,
            ));
        }
        events.push(enter(
            u64::try_from(depth).unwrap() + 2,
            GreenKind::PARAGRAPH,
        ));
        events.push(GreenEvent::Coverage(
            CoverageRun::new(CoverageId(1), bytes, bytes, 0, CoveragePart::CONTENT).unwrap(),
        ));
        for _ in 0..depth.saturating_add(2) {
            events.push(exit());
        }
        SerializedGreenDocument::build(
            arena,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: descriptor.revision,
                source_root: descriptor.root,
                source_bytes: bytes,
                source_utf16: bytes,
                grammar_revision: GrammarRevision(7),
                parse_generation: ParseGeneration(11),
                semantic_epoch: 13,
                known_bytes: 0..bytes,
            },
            events,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap()
    }

    fn build_structurally_ambiguous_boundary_document(
        arena: &mut PageArena,
        source: &SourceStore,
    ) -> SerializedGreenDocument {
        let descriptor = source.descriptor();
        let bytes = u64::try_from(descriptor.bytes).unwrap();
        SerializedGreenDocument::build(
            arena,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: descriptor.revision,
                source_root: descriptor.root,
                source_bytes: bytes,
                source_utf16: bytes,
                grammar_revision: GrammarRevision(7),
                parse_generation: ParseGeneration(11),
                semantic_epoch: 13,
                known_bytes: 0..bytes,
            },
            [
                enter(1, GreenKind::DOCUMENT),
                enter(2, GreenKind::PARAGRAPH),
                GreenEvent::Coverage(
                    CoverageRun::new(CoverageId(1), 3, 3, 0, CoveragePart::CONTENT).unwrap(),
                ),
                exit(),
                enter(3, GreenKind::BLOCK_QUOTE),
                enter(4, GreenKind::PARAGRAPH),
                GreenEvent::Coverage(
                    CoverageRun::new(CoverageId(2), 3, 3, 0, CoveragePart::CONTENT).unwrap(),
                ),
                exit(),
                exit(),
                enter(5, GreenKind::PARAGRAPH),
                GreenEvent::Coverage(
                    CoverageRun::new(CoverageId(3), 2, 2, 0, CoveragePart::CONTENT).unwrap(),
                ),
                exit(),
                exit(),
            ],
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap()
    }

    fn restart(
        document: &SerializedGreenDocument,
        arena: &PageArena,
        source: &SourceStore,
        cut: usize,
    ) -> Result<StoredRestartCheckpointMechanism, CheckpointMechanismError> {
        resolve_restart_checkpoint_mechanism(
            document,
            arena,
            source,
            restart_index_entry_for_proof_harness(cut),
        )
    }

    fn convergence(
        document: &SerializedGreenDocument,
        arena: &PageArena,
        source: &SourceStore,
        cut: usize,
    ) -> Result<StoredConvergenceCheckpointMechanism, CheckpointMechanismError> {
        resolve_convergence_checkpoint_mechanism(
            document,
            arena,
            source,
            convergence_index_entry_for_proof_harness(cut, BoundaryAffinity::After),
        )
    }

    #[test]
    fn proof_harness_mints_exact_role_distinct_line_and_sequence_boundaries() {
        assert_ne!(
            TypeId::of::<StoredRestartCheckpointMechanism>(),
            TypeId::of::<StoredConvergenceCheckpointMechanism>()
        );
        let source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_document(&mut arena, &source, 1);

        let restart = restart(&document, &arena, &source, 3).unwrap();
        let convergence = convergence(&document, &arena, &source, 6).unwrap();
        assert_eq!(restart.source_byte_cut(), 3);
        assert_eq!(restart.source_utf16_cut(), 3);
        assert_eq!(convergence.source_byte_cut(), 6);
        assert_eq!(restart.manifest(), convergence.manifest());
        assert_eq!(restart.syntax_profile(), 1);
        assert_eq!(restart.grammar_revision(), GrammarRevision(7));
        assert_eq!(restart.parse_generation(), ParseGeneration(11));
        assert!(!restart.has_parser_restart_state());
        assert!(!convergence.has_parser_convergence_state());
        assert!(restart.receipt().adjacent_source_bytes_read <= 2);
        assert!(convergence.receipt().adjacent_source_bytes_read <= 2);
        assert!(!std::mem::needs_drop::<StoredRestartCheckpointMechanism>());
    }

    #[test]
    fn proof_harness_rejects_mid_scalar_mid_line_crlf_and_mid_run_cuts() {
        let unicode = SourceStore::new("a😀\nxyz", 8);
        let mut unicode_arena = PageArena::new();
        let unicode_document = build_single_run_document(&mut unicode_arena, &unicode);
        assert_eq!(
            restart(&unicode_document, &unicode_arena, &unicode, 2),
            Err(CheckpointMechanismError::NotScalarBoundary)
        );

        let source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_document(&mut arena, &source, 1);
        assert_eq!(
            restart(&document, &arena, &source, 1),
            Err(CheckpointMechanismError::NotPhysicalLineStart)
        );

        let crlf = SourceStore::new("aa\r\nbbcc", 8);
        let mut crlf_arena = PageArena::new();
        let crlf_document = build_single_run_document(&mut crlf_arena, &crlf);
        assert_eq!(
            restart(&crlf_document, &crlf_arena, &crlf, 3),
            Err(CheckpointMechanismError::NotPhysicalLineStart)
        );

        let split_runs = SourceStore::new("aa\nbb\ncc", 8);
        let mut split_arena = PageArena::new();
        let split_document = build_single_run_document(&mut split_arena, &split_runs);
        assert_eq!(
            restart(&split_document, &split_arena, &split_runs, 3),
            Err(CheckpointMechanismError::NotCoverageBoundary)
        );
    }

    #[test]
    fn deep_nesting_uses_logarithmic_scalar_descent_and_retains_no_path() {
        let source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_deep_document(&mut arena, &source, 512);
        let restart = restart(&document, &arena, &source, 0).unwrap();

        assert_eq!(restart.receipt().root_descents, 1);
        assert_eq!(restart.receipt().leaf_pages_scanned, 1);
        assert!(restart.receipt().maximum_route_depth < 32);
        assert!(restart.receipt().borrowed_leaf_payload_bytes <= ARENA_PAGE_BYTES);
        assert_eq!(restart.receipt().decoded_event_capacity, 0);
        assert!(restart.receipt().maximum_scanner_scratch_bytes < 1024);
        assert_eq!(restart.receipt().retained_heap_bytes, 0);
        assert!(!std::mem::needs_drop::<StoredRestartCheckpointMechanism>());
        assert!(!restart.has_parser_restart_state());
    }

    #[test]
    fn equal_source_cut_has_two_coverage_sides_across_zero_metric_structure() {
        let source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_structurally_ambiguous_boundary_document(&mut arena, &source);

        let before_following = resolve_restart_checkpoint_mechanism(
            &document,
            &arena,
            &source,
            restart_index_entry_with_side_for_proof_harness(
                3,
                SerializedGreenCoverageSideBias::BeforeFollowing,
            ),
        )
        .unwrap();
        let after_preceding = resolve_restart_checkpoint_mechanism(
            &document,
            &arena,
            &source,
            restart_index_entry_with_side_for_proof_harness(
                3,
                SerializedGreenCoverageSideBias::AfterPreceding,
            ),
        )
        .unwrap();

        let SerializedGreenAdjacentCoverageSide::BeforeFollowing(following) =
            before_following.boundary.adjacent_coverage.adjacent
        else {
            panic!("right-biased observation must name following Coverage");
        };
        let SerializedGreenAdjacentCoverageSide::AfterPreceding(preceding) =
            after_preceding.boundary.adjacent_coverage.adjacent
        else {
            panic!("left-biased observation must name preceding Coverage");
        };
        assert_eq!(before_following.source_byte_cut(), 3);
        assert_eq!(after_preceding.source_byte_cut(), 3);
        assert_eq!(following.coverage, CoverageId(2));
        assert_eq!(preceding.coverage, CoverageId(1));
        assert_ne!(following, preceding);
        assert!(!before_following.has_sequence_cut_authority());
        assert!(!after_preceding.has_sequence_cut_authority());

        let before_reset = before_following
            .previous_projection_reset_mechanism(&document, &arena, 1)
            .unwrap();
        let after_reset = after_preceding
            .previous_projection_reset_mechanism(&document, &arena, 1)
            .unwrap();
        assert!(matches!(
            before_reset,
            ProjectionResetSeekOutcome::ImplicitZero { .. }
        ));
        assert!(matches!(
            after_reset,
            ProjectionResetSeekOutcome::ImplicitZero { .. }
        ));
        assert_eq!(before_reset.receipt().pages_scanned, 1);
        assert_eq!(after_reset.receipt().pages_scanned, 1);
    }

    #[test]
    fn restart_and_adoption_lineage_derive_private_offsets_from_consumed_roles() {
        let mut source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_document(&mut arena, &source, 1);
        let mechanisms = StoredRestartSelectionMechanisms::new(
            restart(&document, &arena, &source, 3).unwrap(),
            restart(&document, &arena, &source, 0).unwrap(),
        )
        .unwrap();
        let convergence = convergence(&document, &arena, &source, 6).unwrap();

        source
            .apply_edit(SourceRevision(0), 4..5, "Z")
            .expect("edit only the reparsed middle line");
        let mut selection =
            RestartCheckpointSelectionMechanismJob::begin(&source, mechanisms).unwrap();
        assert_eq!(
            selection.poll(&source, 1),
            RestartCheckpointSelectionMechanismStatus::Selected {
                old: 3,
                current: 3,
                used_zero_fallback: false,
            }
        );
        let selected = selection.into_selected(&source).unwrap();
        assert!(!selected.used_zero_fallback());
        assert_eq!(selected.current_byte_cut(), 3);

        let mut adoption =
            StoredLineageAdoptionMechanismJob::begin(&source, selected, convergence).unwrap();
        assert_eq!(
            adoption.poll(&source, 1),
            StoredLineageAdoptionMechanismStatus::Proven {
                restart: 3,
                convergence: 6,
            }
        );
        let proof = adoption.into_proof(&source).unwrap();
        assert_eq!(proof.target_source(), source.descriptor());
        assert_eq!(proof.manifest(), proof.convergence_manifest());
        assert!(!proof.has_suffix_attachment_authority());
    }

    #[test]
    fn zero_fallback_is_selected_without_accepting_a_second_offset_echo() {
        let mut source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_document(&mut arena, &source, 1);
        let mechanisms = StoredRestartSelectionMechanisms::new(
            restart(&document, &arena, &source, 3).unwrap(),
            restart(&document, &arena, &source, 0).unwrap(),
        )
        .unwrap();
        source
            .apply_edit(SourceRevision(0), 1..2, "Z")
            .expect("edit invalidates preferred retained prefix");

        let mut selection =
            RestartCheckpointSelectionMechanismJob::begin(&source, mechanisms).unwrap();
        assert_eq!(
            selection.poll(&source, 1),
            RestartCheckpointSelectionMechanismStatus::Selected {
                old: 0,
                current: 0,
                used_zero_fallback: true,
            }
        );
        let selected = selection.into_selected(&source).unwrap();
        assert!(selected.used_zero_fallback());
        assert_eq!(selected.current_byte_cut(), 0);
    }

    #[test]
    fn mismatched_manifest_roles_and_advanced_source_fail_closed() {
        let mut source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_document(&mut arena, &source, 1);
        let other_source = SourceStore::new("aa\nbb\ncc", 8);
        let mut other_arena = PageArena::new();
        let other_document = build_document(&mut other_arena, &other_source, 2);

        assert!(matches!(
            restart(&document, &arena, &other_source, 0),
            Err(CheckpointMechanismError::ManifestSourceMismatch { .. })
        ));

        let preferred = restart(&document, &arena, &source, 3).unwrap();
        let foreign_zero = restart(&other_document, &other_arena, &other_source, 0).unwrap();
        assert_eq!(
            StoredRestartSelectionMechanisms::new(preferred, foreign_zero),
            Err(CheckpointMechanismError::BaseManifestMismatch)
        );

        let mechanisms = StoredRestartSelectionMechanisms::new(
            restart(&document, &arena, &source, 3).unwrap(),
            restart(&document, &arena, &source, 0).unwrap(),
        )
        .unwrap();
        source.apply_edit(SourceRevision(0), 4..5, "Z").unwrap();
        let mut selection =
            RestartCheckpointSelectionMechanismJob::begin(&source, mechanisms).unwrap();
        let frozen = source.descriptor();
        source.apply_edit(SourceRevision(1), 4..5, "Y").unwrap();
        assert_eq!(
            selection.poll(&source, 1),
            RestartCheckpointSelectionMechanismStatus::Rejected(
                CheckpointMechanismError::SourceAdvanced {
                    expected: frozen,
                    actual: source.descriptor(),
                }
            )
        );
        assert_eq!(
            selection.into_selected(&source),
            Err(CheckpointMechanismError::SourceAdvanced {
                expected: frozen,
                actual: source.descriptor(),
            })
        );
    }

    #[test]
    fn adoption_job_rechecks_its_frozen_source_before_every_poll() {
        let mut source = SourceStore::new("aa\nbb\ncc", 8);
        let mut arena = PageArena::new();
        let document = build_document(&mut arena, &source, 1);
        let mechanisms = StoredRestartSelectionMechanisms::new(
            restart(&document, &arena, &source, 3).unwrap(),
            restart(&document, &arena, &source, 0).unwrap(),
        )
        .unwrap();
        let convergence = convergence(&document, &arena, &source, 6).unwrap();
        source.apply_edit(SourceRevision(0), 4..5, "Z").unwrap();
        let mut selection =
            RestartCheckpointSelectionMechanismJob::begin(&source, mechanisms).unwrap();
        assert!(matches!(
            selection.poll(&source, 1),
            RestartCheckpointSelectionMechanismStatus::Selected { .. }
        ));
        let selected = selection.into_selected(&source).unwrap();
        let mut adoption =
            StoredLineageAdoptionMechanismJob::begin(&source, selected, convergence).unwrap();
        let frozen = source.descriptor();
        source.apply_edit(SourceRevision(1), 4..5, "Y").unwrap();

        assert_eq!(
            adoption.poll(&source, 1),
            StoredLineageAdoptionMechanismStatus::Rejected(
                CheckpointMechanismError::SourceAdvanced {
                    expected: frozen,
                    actual: source.descriptor(),
                }
            )
        );
    }
}

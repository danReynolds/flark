//! Source-only authority for one retained cross-build restart coordinate.
//!
//! This slice deliberately proves less than a parser checkpoint. It joins a
//! real [`RestartSelectionProof`] with the current [`SourceStore`], preserves
//! the distinction between a physical line restart and the accepted
//! projection prefix before one deferred LF, and mints current source cursors.
//! It owns no parser control, semantic identity, green cut, or storage root.

#[cfg(feature = "exact-parser")]
#[cfg(feature = "exact-parser")]
use crate::serialized_green::CurrentRestartPath;
use crate::source_bound_ledger::RetainedSetextSourceLedgerDraft;
#[cfg(feature = "exact-parser")]
use crate::storage_only_composite_document::{
    RestartParentSelectionStamp, RestartSourceLedgerCheckpointKind,
    RestartSourceLedgerCheckpointMint,
};
use crate::{
    BoundaryAffinity, LineageSnapshotRetention, ProvenRestartSelection, ProvenRetainedPrefix,
    ProvenSourceMapping, RejectedPreferredRestart, RestartSelectionError, RestartSelectionJob,
    RestartSelectionMetrics, RestartSelectionStatus, SourceError, SourceLfLineBoundaryMetric,
    SourcePhysicalLinePredecessor, SourcePhysicalLineQueryReceipt, SourcePrefixMetric,
    SourceResumeCursorPair, SourceSnapshotDescriptor, SourceStore, SourceStoreError,
};

/// Immutable checkpoint data, not restart authority. Its fields remain private
/// so another component cannot relabel one scalar as both the physical and
/// accepted-projection cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredDeferredLfRestart {
    source: SourceSnapshotDescriptor,
    physical_restart: usize,
    affinity: BoundaryAffinity,
    deferred_lf: DeferredLfBridge,
    physical_line: SourceLfLineBoundaryMetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeferredLfBridge {
    accepted_projection_cut: usize,
    accepted_projection_utf16: usize,
    physical_restart: usize,
    physical_restart_utf16: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedRestartCoordinateError {
    CollapsedCoordinates {
        coordinate: usize,
    },
    DeferredLfWidth {
        accepted_projection_cut: usize,
        physical_restart: usize,
    },
    DeferredLfMismatch {
        source: SourceSnapshotDescriptor,
        offset: usize,
        observed: Option<u8>,
    },
    WrongAffinity(BoundaryAffinity),
    Selection(RestartSelectionError),
    SourceAdvanced {
        expected: SourceSnapshotDescriptor,
        actual: SourceSnapshotDescriptor,
    },
    ProofMismatch,
    Source(SourceError),
    SourceStore(SourceStoreError),
    AlreadyComplete,
}

impl From<RestartSelectionError> for RetainedRestartCoordinateError {
    fn from(error: RestartSelectionError) -> Self {
        Self::Selection(error)
    }
}

impl From<SourceError> for RetainedRestartCoordinateError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<SourceStoreError> for RetainedRestartCoordinateError {
    fn from(error: SourceStoreError) -> Self {
        Self::SourceStore(error)
    }
}

impl StoredDeferredLfRestart {
    /// Captures only typed scalar checkpoint data after validating that the
    /// old source really contains the deferred LF. No source byte or source
    /// lease is retained in the result.
    pub(crate) fn capture(
        source: &SourceStore,
        accepted_projection_cut: usize,
        physical_restart: usize,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        let accepted = source.observe_prefix_metric_at(accepted_projection_cut)?;
        let physical = source.observe_prefix_metric_at(physical_restart)?;
        let deferred_lf = DeferredLfBridge::try_new(accepted, physical)?;
        validate_current_lf(source, deferred_lf)?;
        let physical_line = source.observe_lf_line_boundary_at(physical_restart)?;
        Ok(Self {
            source: source.descriptor(),
            physical_restart,
            affinity: BoundaryAffinity::Before,
            deferred_lf,
            physical_line,
        })
    }

    /// Derives the selected 4/5-style bridge from the already joined writer
    /// checkpoint. No caller-provided source coordinate enters this path.
    pub(crate) fn capture_from_joined_setext(
        draft: &RetainedSetextSourceLedgerDraft,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        let source = draft.descriptor();
        let accepted_projection_cut = usize::try_from(draft.accepted_bytes())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let accepted_projection_utf16 = usize::try_from(draft.accepted_utf16())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let physical_restart = usize::try_from(draft.physical_bytes())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let physical_restart_utf16 = usize::try_from(draft.physical_utf16())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let deferred_lf = DeferredLfBridge::try_new(
            SourcePrefixMetric {
                root: source.root,
                bytes: accepted_projection_cut,
                utf16: accepted_projection_utf16,
            },
            SourcePrefixMetric {
                root: source.root,
                bytes: physical_restart,
                utf16: physical_restart_utf16,
            },
        )?;
        let physical_line = SourceLfLineBoundaryMetric {
            root: source.root,
            offset: physical_restart,
            completed_line_ordinal: draft.line_ordinal(),
            previous_content_bytes: draft
                .last_line_length()
                .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?,
            adjacent_bytes_read: 0,
        };
        let stored = Self {
            source,
            physical_restart,
            affinity: BoundaryAffinity::Before,
            deferred_lf,
            physical_line,
        };
        stored.validate_shape()?;
        Ok(stored)
    }

    fn validate_shape(&self) -> Result<(), RetainedRestartCoordinateError> {
        if self.affinity != BoundaryAffinity::Before {
            return Err(RetainedRestartCoordinateError::WrongAffinity(self.affinity));
        }
        if self.deferred_lf.physical_restart != self.physical_restart {
            return Err(RetainedRestartCoordinateError::ProofMismatch);
        }
        if self.physical_line.root != self.source.root
            || self.physical_line.offset != self.physical_restart
            || self.physical_line.completed_line_ordinal == 0
        {
            return Err(RetainedRestartCoordinateError::ProofMismatch);
        }
        DeferredLfBridge::try_new(
            SourcePrefixMetric {
                root: self.source.root,
                bytes: self.deferred_lf.accepted_projection_cut,
                utf16: self.deferred_lf.accepted_projection_utf16,
            },
            SourcePrefixMetric {
                root: self.source.root,
                bytes: self.deferred_lf.physical_restart,
                utf16: self.deferred_lf.physical_restart_utf16,
            },
        )?;
        Ok(())
    }

    #[cfg(test)]
    const RETAINED_SOURCE_BYTES_FOR_TEST: usize = 0;
}

impl DeferredLfBridge {
    fn try_new(
        accepted: SourcePrefixMetric,
        physical: SourcePrefixMetric,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        if accepted.root != physical.root {
            return Err(RetainedRestartCoordinateError::ProofMismatch);
        }
        let accepted_projection_cut = accepted.bytes;
        let physical_restart = physical.bytes;
        if accepted_projection_cut == physical_restart {
            return Err(RetainedRestartCoordinateError::CollapsedCoordinates {
                coordinate: physical_restart,
            });
        }
        if accepted_projection_cut.checked_add(1) != Some(physical_restart) {
            return Err(RetainedRestartCoordinateError::DeferredLfWidth {
                accepted_projection_cut,
                physical_restart,
            });
        }
        if accepted.utf16.checked_add(1) != Some(physical.utf16) {
            return Err(RetainedRestartCoordinateError::DeferredLfWidth {
                accepted_projection_cut,
                physical_restart,
            });
        }
        Ok(Self {
            accepted_projection_cut,
            accepted_projection_utf16: accepted.utf16,
            physical_restart,
            physical_restart_utf16: physical.utf16,
        })
    }
}

/// Parent-selected persisted restart input for the first nonzero LF gate.
///
/// Construction is possible only by consuming the private composite-parent
/// mint. The physical cut comes from the selected checkpoint measure while the
/// accepted projection cut and the sole open path come from the exact green
/// query at that checkpoint's event cut. Keeping those values in one linear
/// carrier prevents a caller from pairing a valid source cut with an unrelated
/// green path.
#[cfg(feature = "exact-parser")]
#[must_use = "a parent-selected restart must enter lineage resolution or be discarded"]
#[derive(Debug)]
struct ParentSelectedDeferredLfRestart {
    parent_selection: RestartParentSelectionStamp,
    source: SourceSnapshotDescriptor,
    deferred_lf: DeferredLfBridge,
    completed_line_ordinal: u64,
    path: CurrentRestartPath,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedDeferredLfRestart {
    fn from_parts(
        parent_selection: RestartParentSelectionStamp,
        source: SourceSnapshotDescriptor,
        checkpoint_cut: crate::committed_checkpoint_index::RelativeCheckpointMeasure,
        path: CurrentRestartPath,
        restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        let accepted = path.source_metric();
        let physical_restart = usize::try_from(checkpoint_cut.source_bytes())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let physical_restart_utf16 = usize::try_from(checkpoint_cut.source_utf16())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let deferred_lf = DeferredLfBridge::try_new(
            SourcePrefixMetric {
                root: source.root,
                bytes: usize::try_from(accepted.bytes)
                    .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?,
                utf16: usize::try_from(accepted.utf16)
                    .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?,
            },
            SourcePrefixMetric {
                root: source.root,
                bytes: physical_restart,
                utf16: physical_restart_utf16,
            },
        )?;
        if physical_restart == 0
            || physical_restart > source.bytes
            || checkpoint_cut.physical_lines() == 0
            || path.event_cut() != checkpoint_cut.green_events()
            || path.coverage_count() != checkpoint_cut.projection_runs()
            || path.open_depth()
                != u64::try_from(path.frames().len())
                    .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?
            || path.frames().is_empty()
            || path.mapping_receipt().retained_source_bytes != 0
            || path.mapping_receipt().document_sized_event_vectors != 0
        {
            return Err(RetainedRestartCoordinateError::ProofMismatch);
        }
        Ok(Self {
            parent_selection,
            source,
            deferred_lf,
            completed_line_ordinal: checkpoint_cut.physical_lines(),
            path,
            restart_anchor,
        })
    }

    const fn physical_restart(&self) -> usize {
        self.deferred_lf.physical_restart
    }
}

/// Parent-selected restart whose accepted green/source cut is the complete
/// physical line frontier itself. It owns no deferred byte bridge: A=P on
/// both source axes. Source completeness does not imply structural closure;
/// the exact authenticated terminal frame may remain open until the next line.
#[cfg(feature = "exact-parser")]
#[must_use = "a parent-selected restart must enter lineage resolution or be discarded"]
#[derive(Debug)]
struct ParentSelectedSourceCompleteLineBoundaryRestart {
    parent_selection: RestartParentSelectionStamp,
    source: SourceSnapshotDescriptor,
    physical_restart: usize,
    physical_restart_utf16: usize,
    completed_line_ordinal: u64,
    path: CurrentRestartPath,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSourceCompleteLineBoundaryRestart {
    fn from_parts(
        parent_selection: RestartParentSelectionStamp,
        source: SourceSnapshotDescriptor,
        checkpoint_cut: crate::committed_checkpoint_index::RelativeCheckpointMeasure,
        path: CurrentRestartPath,
        restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        let physical_restart = usize::try_from(checkpoint_cut.source_bytes())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        let physical_restart_utf16 = usize::try_from(checkpoint_cut.source_utf16())
            .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
        if physical_restart == 0
            || physical_restart > source.bytes
            || checkpoint_cut.physical_lines() == 0
            || path.source_metric().bytes != checkpoint_cut.source_bytes()
            || path.source_metric().utf16 != checkpoint_cut.source_utf16()
            || path.event_cut() != checkpoint_cut.green_events()
            || path.coverage_count() != checkpoint_cut.projection_runs()
            || path.open_depth()
                != u64::try_from(path.frames().len())
                    .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?
            || path.frames().is_empty()
            || path.mapping_receipt().retained_source_bytes != 0
            || path.mapping_receipt().document_sized_event_vectors != 0
        {
            return Err(RetainedRestartCoordinateError::ProofMismatch);
        }
        Ok(Self {
            parent_selection,
            source,
            physical_restart,
            physical_restart_utf16,
            completed_line_ordinal: checkpoint_cut.physical_lines(),
            path,
            restart_anchor,
        })
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
enum ParentSelectedPersistedRestart {
    DeferredLf(ParentSelectedDeferredLfRestart),
    SourceCompleteLineBoundary(ParentSelectedSourceCompleteLineBoundaryRestart),
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedPersistedRestart {
    fn from_parent_mint(
        mint: RestartSourceLedgerCheckpointMint,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        let (source, checkpoint_cut, path, kind, restart_anchor, parent_selection) =
            mint.into_source_ledger_parts();
        if kind == RestartSourceLedgerCheckpointKind::SourceCompleteLineBoundary {
            let (grammar, _) = restart_anchor
                .decode_grammar_parts()
                .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
            if grammar.deferred_role()
                != flark_comrak_value_block_core::DirectLineBoundaryDeferredRole::None
            {
                return Err(RetainedRestartCoordinateError::ProofMismatch);
            }
        }
        match kind {
            RestartSourceLedgerCheckpointKind::DeferredLf => {
                ParentSelectedDeferredLfRestart::from_parts(
                    parent_selection,
                    source,
                    checkpoint_cut,
                    path,
                    restart_anchor,
                )
                .map(Self::DeferredLf)
            }
            RestartSourceLedgerCheckpointKind::SourceCompleteLineBoundary => {
                ParentSelectedSourceCompleteLineBoundaryRestart::from_parts(
                    parent_selection,
                    source,
                    checkpoint_cut,
                    path,
                    restart_anchor,
                )
                .map(Self::SourceCompleteLineBoundary)
            }
        }
    }

    const fn source(&self) -> SourceSnapshotDescriptor {
        match self {
            Self::DeferredLf(checkpoint) => checkpoint.source,
            Self::SourceCompleteLineBoundary(checkpoint) => checkpoint.source,
        }
    }

    const fn physical_restart(&self) -> usize {
        match self {
            Self::DeferredLf(checkpoint) => checkpoint.physical_restart(),
            Self::SourceCompleteLineBoundary(checkpoint) => checkpoint.physical_restart,
        }
    }
}

/// Fuelled persisted-parent restart selection. The only retained variable-size
/// value is the already-bounded current green open path; source lineage remains
/// scalar-only and the job owns no old Crop root or transient writer draft.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct PersistedRestartCoordinateJob {
    checkpoint: Option<ParentSelectedPersistedRestart>,
    expected_current: SourceSnapshotDescriptor,
    selection: Option<RestartSelectionJob>,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum PersistedRestartCoordinateProgress {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    Ready(Box<PersistedRestartCoordinateAuthority>),
}

/// The persisted gate either preserves the parent-selected nonzero restart or
/// returns the independently proven zero fallback. It never silently weakens a
/// failed preferred proof into caller-selected coordinates.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum PersistedRestartCoordinateAuthority {
    PreferredDeferredLf(PreferredPersistedDeferredLfRestart),
    PreferredSourceCompleteLineBoundary(PreferredPersistedSourceCompleteLineBoundaryRestart),
    ZeroFallback(ZeroRestartCoordinate),
}

/// Auditable source work performed after the lineage proof. The green mapping
/// receipt remains on `CurrentRestartPath`; this receipt covers only the
/// current SourceStore observations needed to reconstruct the line cursor.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PersistedRestartSourceReceipt {
    pub line_query: SourcePhysicalLineQueryReceipt,
    pub deferred_lf_bytes_examined: usize,
    pub retained_old_source_roots: usize,
    pub retained_old_source_bytes: usize,
    pub retained_old_writer_drafts: usize,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistedRestartCoordinateView {
    pub old_accepted_projection_cut: usize,
    pub old_accepted_projection_utf16: usize,
    pub old_physical_restart: usize,
    pub old_physical_restart_utf16: usize,
    pub current_accepted_projection_cut: usize,
    pub current_accepted_projection_utf16: usize,
    pub current_physical_restart: usize,
    pub current_physical_restart_utf16: usize,
    pub parent_completed_line_ordinal: u64,
    pub current_completed_line_ordinal: u64,
    pub current_previous_content_bytes: u64,
    pub current_previous_content_utf16: u64,
    pub affinity: BoundaryAffinity,
}

/// Preferred persisted restart after the unchanged-prefix and current-source
/// joins. The old path remains linear and is returned together with the fresh
/// ledger reconstruction; source and donor consumers cannot independently
/// query two paths and drift apart.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct PreferredPersistedDeferredLfRestart {
    parent_selection: RestartParentSelectionStamp,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    coordinates: PersistedRestartCoordinateView,
    previous: SourcePhysicalLinePredecessor,
    cursors: SourceResumeCursorPair,
    path: CurrentRestartPath,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    receipt: PersistedRestartSourceReceipt,
}

#[cfg(feature = "exact-parser")]
impl PreferredPersistedDeferredLfRestart {
    #[must_use]
    pub(crate) const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub(crate) const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub(crate) const fn coordinates(&self) -> PersistedRestartCoordinateView {
        self.coordinates
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> PersistedRestartSourceReceipt {
        self.receipt
    }

    pub(crate) fn into_reconstruction_parts(
        self,
    ) -> (
        SourceSnapshotDescriptor,
        SourceSnapshotDescriptor,
        PersistedRestartCoordinateView,
        SourcePhysicalLinePredecessor,
        SourceResumeCursorPair,
        CurrentRestartPath,
        crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
        RestartParentSelectionStamp,
        PersistedRestartSourceReceipt,
    ) {
        (
            self.from,
            self.to,
            self.coordinates,
            self.previous,
            self.cursors,
            self.path,
            self.restart_anchor,
            self.parent_selection,
            self.receipt,
        )
    }
}

/// Preferred source-complete line-boundary restart after the same unchanged-prefix
/// lineage proof and current-source line query. This carrier is intentionally
/// distinct from the deferred-LF form: its accepted and physical cuts are
/// equal and no byte bridge is present or examined.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct PreferredPersistedSourceCompleteLineBoundaryRestart {
    parent_selection: RestartParentSelectionStamp,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    coordinates: PersistedRestartCoordinateView,
    previous: SourcePhysicalLinePredecessor,
    cursors: SourceResumeCursorPair,
    path: CurrentRestartPath,
    restart_anchor: crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
    receipt: PersistedRestartSourceReceipt,
}

#[cfg(feature = "exact-parser")]
impl PreferredPersistedSourceCompleteLineBoundaryRestart {
    #[must_use]
    pub(crate) const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub(crate) const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub(crate) const fn coordinates(&self) -> PersistedRestartCoordinateView {
        self.coordinates
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> PersistedRestartSourceReceipt {
        self.receipt
    }

    pub(crate) fn into_reconstruction_parts(
        self,
    ) -> (
        SourceSnapshotDescriptor,
        SourceSnapshotDescriptor,
        PersistedRestartCoordinateView,
        SourcePhysicalLinePredecessor,
        SourceResumeCursorPair,
        CurrentRestartPath,
        crate::committed_checkpoint_index::ParentSelectedRestartAnchor,
        RestartParentSelectionStamp,
        PersistedRestartSourceReceipt,
    ) {
        (
            self.from,
            self.to,
            self.coordinates,
            self.previous,
            self.cursors,
            self.path,
            self.restart_anchor,
            self.parent_selection,
            self.receipt,
        )
    }
}

#[cfg(feature = "exact-parser")]
impl PersistedRestartCoordinateJob {
    pub(crate) fn begin(
        source: &SourceStore,
        mint: RestartSourceLedgerCheckpointMint,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        let checkpoint = ParentSelectedPersistedRestart::from_parent_mint(mint)?;
        let selection = source
            .begin_restart_selection(checkpoint.source(), checkpoint.physical_restart())
            .map_err(RetainedRestartCoordinateError::Selection)?;
        Ok(Self {
            checkpoint: Some(checkpoint),
            expected_current: source.descriptor(),
            selection: Some(selection),
        })
    }

    pub(crate) fn poll(
        &mut self,
        source: &SourceStore,
        fuel: usize,
    ) -> Result<PersistedRestartCoordinateProgress, RetainedRestartCoordinateError> {
        let status = self
            .selection
            .as_mut()
            .ok_or(RetainedRestartCoordinateError::AlreadyComplete)?
            .poll(fuel);
        match status {
            RestartSelectionStatus::Pending {
                processed_records,
                remaining_records,
            } => Ok(PersistedRestartCoordinateProgress::Pending {
                processed_records,
                remaining_records,
            }),
            RestartSelectionStatus::Failed(error) => {
                self.selection.take();
                self.checkpoint.take();
                Err(RetainedRestartCoordinateError::Selection(error))
            }
            RestartSelectionStatus::Selected { .. } => {
                let selection = self
                    .selection
                    .take()
                    .ok_or(RetainedRestartCoordinateError::AlreadyComplete)?;
                let checkpoint = self
                    .checkpoint
                    .take()
                    .ok_or(RetainedRestartCoordinateError::AlreadyComplete)?;
                let proof = selection
                    .into_proof()
                    .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
                resolve_persisted_proof(source, checkpoint, self.expected_current, &proof)
                    .map(Box::new)
                    .map(PersistedRestartCoordinateProgress::Ready)
            }
        }
    }

    pub(crate) fn cancel(mut self) -> RetainedRestartCoordinateCancellation {
        self.checkpoint.take();
        let selection = self.selection.take();
        let (metrics, retention) = selection.as_ref().map_or_else(
            || {
                (
                    RestartSelectionMetrics::default(),
                    LineageSnapshotRetention::default(),
                )
            },
            |job| (job.metrics(), job.retention()),
        );
        drop(selection);
        RetainedRestartCoordinateCancellation { metrics, retention }
    }
}

#[cfg(feature = "exact-parser")]
fn resolve_persisted_proof(
    source: &SourceStore,
    checkpoint: ParentSelectedPersistedRestart,
    expected_current: SourceSnapshotDescriptor,
    proof: &crate::RestartSelectionProof,
) -> Result<PersistedRestartCoordinateAuthority, RetainedRestartCoordinateError> {
    let actual = source.descriptor();
    if actual != expected_current {
        return Err(RetainedRestartCoordinateError::SourceAdvanced {
            expected: expected_current,
            actual,
        });
    }
    if proof.from() != checkpoint.source() || proof.to() != expected_current {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    match (checkpoint, proof.selection()) {
        (
            ParentSelectedPersistedRestart::DeferredLf(checkpoint),
            ProvenRestartSelection::Preferred { boundary, prefix },
        ) => resolve_persisted_deferred_lf_preferred(
            source,
            checkpoint,
            proof.from(),
            proof.to(),
            boundary,
            prefix,
        )
        .map(PersistedRestartCoordinateAuthority::PreferredDeferredLf),
        (
            ParentSelectedPersistedRestart::SourceCompleteLineBoundary(checkpoint),
            ProvenRestartSelection::Preferred { boundary, prefix },
        ) => resolve_persisted_source_complete_line_boundary_preferred(
            source,
            checkpoint,
            proof.from(),
            proof.to(),
            boundary,
            prefix,
        )
        .map(PersistedRestartCoordinateAuthority::PreferredSourceCompleteLineBoundary),
        (_, ProvenRestartSelection::ZeroFallback { boundary, rejected }) => {
            resolve_zero(source, proof.from(), proof.to(), boundary, *rejected)
                .map(PersistedRestartCoordinateAuthority::ZeroFallback)
        }
    }
}

#[cfg(feature = "exact-parser")]
fn resolve_persisted_deferred_lf_preferred(
    source: &SourceStore,
    checkpoint: ParentSelectedDeferredLfRestart,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    boundary: &ProvenSourceMapping,
    prefix: &ProvenRetainedPrefix,
) -> Result<PreferredPersistedDeferredLfRestart, RetainedRestartCoordinateError> {
    let ProvenSourceMapping::Boundary {
        from: old_physical_restart,
        to: current_physical_restart,
        affinity: BoundaryAffinity::Before,
    } = boundary
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    let ProvenRetainedPrefix::Range {
        from: old_prefix,
        to: current_prefix,
    } = prefix
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    if *old_physical_restart != checkpoint.deferred_lf.physical_restart
        || old_prefix != &(0..*old_physical_restart)
        || current_prefix != &(0..*current_physical_restart)
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }

    let relative_accepted = checkpoint
        .deferred_lf
        .accepted_projection_cut
        .checked_sub(old_prefix.start)
        .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    let current_accepted_projection_cut = current_prefix
        .start
        .checked_add(relative_accepted)
        .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    let current_accepted = source.observe_prefix_metric_at(current_accepted_projection_cut)?;
    let current_physical = source.observe_prefix_metric_at(*current_physical_restart)?;
    let current_bridge = DeferredLfBridge::try_new(current_accepted, current_physical)?;
    if current_bridge.accepted_projection_utf16 != checkpoint.deferred_lf.accepted_projection_utf16
        || current_bridge.physical_restart_utf16 != checkpoint.deferred_lf.physical_restart_utf16
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let deferred_lf_bytes_examined = validate_current_bare_lf(source, current_bridge)?;
    let cut = source.certify_current_byte_cut(to, *current_physical_restart)?;
    let line = source.query_physical_line_at_cut(cut)?;
    let previous = line
        .previous()
        .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    if line.snapshot() != to
        || line.offset() != *current_physical_restart
        || !line.is_physical_line_start()
        || line.line_ordinal() != checkpoint.completed_line_ordinal
        || previous.content_bytes()
            > u64::try_from(current_accepted_projection_cut)
                .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?
        || previous.content_utf16()
            > u64::try_from(current_bridge.accepted_projection_utf16)
                .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let cursors = source.issue_resume_cursor_pair(*current_physical_restart)?;
    if cursors.descriptor() != to
        || cursors.offset() != *current_physical_restart
        || !cursors.is_physical_line_start()
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    Ok(PreferredPersistedDeferredLfRestart {
        parent_selection: checkpoint.parent_selection,
        from,
        to,
        coordinates: PersistedRestartCoordinateView {
            old_accepted_projection_cut: checkpoint.deferred_lf.accepted_projection_cut,
            old_accepted_projection_utf16: checkpoint.deferred_lf.accepted_projection_utf16,
            old_physical_restart: *old_physical_restart,
            old_physical_restart_utf16: checkpoint.deferred_lf.physical_restart_utf16,
            current_accepted_projection_cut,
            current_accepted_projection_utf16: current_bridge.accepted_projection_utf16,
            current_physical_restart: *current_physical_restart,
            current_physical_restart_utf16: current_bridge.physical_restart_utf16,
            parent_completed_line_ordinal: checkpoint.completed_line_ordinal,
            current_completed_line_ordinal: line.line_ordinal(),
            current_previous_content_bytes: previous.content_bytes(),
            current_previous_content_utf16: previous.content_utf16(),
            affinity: BoundaryAffinity::Before,
        },
        previous,
        cursors,
        path: checkpoint.path,
        restart_anchor: checkpoint.restart_anchor,
        receipt: PersistedRestartSourceReceipt {
            line_query: line.receipt(),
            deferred_lf_bytes_examined,
            retained_old_source_roots: 0,
            retained_old_source_bytes: 0,
            retained_old_writer_drafts: 0,
        },
    })
}

#[cfg(feature = "exact-parser")]
fn resolve_persisted_source_complete_line_boundary_preferred(
    source: &SourceStore,
    checkpoint: ParentSelectedSourceCompleteLineBoundaryRestart,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    boundary: &ProvenSourceMapping,
    prefix: &ProvenRetainedPrefix,
) -> Result<PreferredPersistedSourceCompleteLineBoundaryRestart, RetainedRestartCoordinateError> {
    let ProvenSourceMapping::Boundary {
        from: old_physical_restart,
        to: current_physical_restart,
        affinity: BoundaryAffinity::Before,
    } = boundary
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    let ProvenRetainedPrefix::Range {
        from: old_prefix,
        to: current_prefix,
    } = prefix
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    if *old_physical_restart != checkpoint.physical_restart
        || old_prefix != &(0..*old_physical_restart)
        || current_prefix != &(0..*current_physical_restart)
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }

    let current_physical = source.observe_prefix_metric_at(*current_physical_restart)?;
    if current_physical.utf16 != checkpoint.physical_restart_utf16 {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let cut = source.certify_current_byte_cut(to, *current_physical_restart)?;
    let line = source.query_physical_line_at_cut(cut)?;
    let previous = line
        .previous()
        .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    if line.snapshot() != to
        || line.offset() != *current_physical_restart
        || !line.is_physical_line_start()
        || line.line_ordinal() != checkpoint.completed_line_ordinal
        || previous.content_bytes()
            > u64::try_from(*current_physical_restart)
                .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?
        || previous.content_utf16()
            > u64::try_from(current_physical.utf16)
                .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let cursors = source.issue_resume_cursor_pair(*current_physical_restart)?;
    if cursors.descriptor() != to
        || cursors.offset() != *current_physical_restart
        || !cursors.is_physical_line_start()
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }

    Ok(PreferredPersistedSourceCompleteLineBoundaryRestart {
        parent_selection: checkpoint.parent_selection,
        from,
        to,
        coordinates: PersistedRestartCoordinateView {
            old_accepted_projection_cut: checkpoint.physical_restart,
            old_accepted_projection_utf16: checkpoint.physical_restart_utf16,
            old_physical_restart: *old_physical_restart,
            old_physical_restart_utf16: checkpoint.physical_restart_utf16,
            current_accepted_projection_cut: *current_physical_restart,
            current_accepted_projection_utf16: current_physical.utf16,
            current_physical_restart: *current_physical_restart,
            current_physical_restart_utf16: current_physical.utf16,
            parent_completed_line_ordinal: checkpoint.completed_line_ordinal,
            current_completed_line_ordinal: line.line_ordinal(),
            current_previous_content_bytes: previous.content_bytes(),
            current_previous_content_utf16: previous.content_utf16(),
            affinity: BoundaryAffinity::Before,
        },
        previous,
        cursors,
        path: checkpoint.path,
        restart_anchor: checkpoint.restart_anchor,
        receipt: PersistedRestartSourceReceipt {
            line_query: line.receipt(),
            deferred_lf_bytes_examined: 0,
            retained_old_source_roots: 0,
            retained_old_source_bytes: 0,
            retained_old_writer_drafts: 0,
        },
    })
}

/// Fuelled source-lineage resolution. The current source is rechecked on the
/// terminal poll so a job frozen before a later edit cannot mint stale cursors.
#[derive(Debug)]
pub(crate) struct RetainedRestartCoordinateJob {
    stored: StoredDeferredLfRestart,
    expected_current: SourceSnapshotDescriptor,
    selection: Option<RestartSelectionJob>,
}

#[derive(Debug)]
pub(crate) enum RetainedRestartCoordinateProgress {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    Ready(Box<RetainedRestartCoordinateAuthority>),
}

impl RetainedRestartCoordinateJob {
    pub(crate) fn begin(
        source: &SourceStore,
        stored: StoredDeferredLfRestart,
    ) -> Result<Self, RetainedRestartCoordinateError> {
        stored.validate_shape()?;
        let selection = source
            .begin_restart_selection(stored.source, stored.physical_restart)
            .map_err(RetainedRestartCoordinateError::Selection)?;
        Ok(Self {
            stored,
            expected_current: source.descriptor(),
            selection: Some(selection),
        })
    }

    pub(crate) fn poll(
        &mut self,
        source: &SourceStore,
        fuel: usize,
    ) -> Result<RetainedRestartCoordinateProgress, RetainedRestartCoordinateError> {
        let status = self
            .selection
            .as_mut()
            .ok_or(RetainedRestartCoordinateError::AlreadyComplete)?
            .poll(fuel);
        match status {
            RestartSelectionStatus::Pending {
                processed_records,
                remaining_records,
            } => Ok(RetainedRestartCoordinateProgress::Pending {
                processed_records,
                remaining_records,
            }),
            RestartSelectionStatus::Failed(error) => {
                self.selection.take();
                Err(RetainedRestartCoordinateError::Selection(error))
            }
            RestartSelectionStatus::Selected { .. } => {
                let selection = self
                    .selection
                    .take()
                    .ok_or(RetainedRestartCoordinateError::AlreadyComplete)?;
                let proof = selection
                    .into_proof()
                    .map_err(|_| RetainedRestartCoordinateError::ProofMismatch)?;
                resolve_proof(source, &self.stored, self.expected_current, &proof)
                    .map(Box::new)
                    .map(RetainedRestartCoordinateProgress::Ready)
            }
        }
    }

    /// Cancellation is immediate because the job owns scalar lineage only. The
    /// receipt is explicit that dropping the persistent `Arc` snapshot can
    /// synchronously release this bounded scalar tree; it retains no Crop root.
    pub(crate) fn cancel(mut self) -> RetainedRestartCoordinateCancellation {
        let selection = self.selection.take();
        let (metrics, retention) = selection.as_ref().map_or_else(
            || {
                (
                    RestartSelectionMetrics::default(),
                    LineageSnapshotRetention::default(),
                )
            },
            |job| (job.metrics(), job.retention()),
        );
        drop(selection);
        RetainedRestartCoordinateCancellation { metrics, retention }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedRestartCoordinateCancellation {
    metrics: RestartSelectionMetrics,
    retention: LineageSnapshotRetention,
}

/// Non-cloneable source authority. Only the preferred form carries a typed LF
/// bridge; zero fallback deliberately discards every nonzero coordinate.
#[derive(Debug)]
pub(crate) enum RetainedRestartCoordinateAuthority {
    Preferred(PreferredDeferredLfRestart),
    ZeroFallback(ZeroRestartCoordinate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RestartCoordinateView {
    pub old_accepted_projection_cut: usize,
    pub old_accepted_projection_utf16: usize,
    pub old_physical_restart: usize,
    pub old_physical_restart_utf16: usize,
    pub current_accepted_projection_cut: usize,
    pub current_accepted_projection_utf16: usize,
    pub current_physical_restart: usize,
    pub current_physical_restart_utf16: usize,
    pub old_completed_line_ordinal: u64,
    pub old_previous_content_bytes: u64,
    pub current_completed_line_ordinal: u64,
    pub current_previous_content_bytes: u64,
    pub affinity: BoundaryAffinity,
}

#[derive(Debug)]
pub(crate) struct PreferredDeferredLfRestart {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    coordinates: RestartCoordinateView,
    bridge: RevalidatedDeferredLfBridge,
    cursors: SourceResumeCursorPair,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevalidatedDeferredLfBridge {
    old_start: usize,
    old_end: usize,
    current_start: usize,
    current_end: usize,
    source_bytes_examined: usize,
}

#[derive(Debug)]
pub(crate) struct ZeroRestartCoordinate {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    rejected: RejectedPreferredRestart,
    cursors: SourceResumeCursorPair,
}

impl PreferredDeferredLfRestart {
    #[must_use]
    pub(crate) const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub(crate) const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub(crate) const fn coordinates(&self) -> RestartCoordinateView {
        self.coordinates
    }

    pub(crate) fn into_cursor_pair(self) -> SourceResumeCursorPair {
        self.cursors
    }
}

impl ZeroRestartCoordinate {
    #[must_use]
    pub(crate) const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub(crate) const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub(crate) const fn rejected(&self) -> RejectedPreferredRestart {
        self.rejected
    }

    pub(crate) fn into_cursor_pair(self) -> SourceResumeCursorPair {
        self.cursors
    }
}

fn resolve_proof(
    source: &SourceStore,
    stored: &StoredDeferredLfRestart,
    expected_current: SourceSnapshotDescriptor,
    proof: &crate::RestartSelectionProof,
) -> Result<RetainedRestartCoordinateAuthority, RetainedRestartCoordinateError> {
    let actual = source.descriptor();
    if actual != expected_current {
        return Err(RetainedRestartCoordinateError::SourceAdvanced {
            expected: expected_current,
            actual,
        });
    }
    if proof.from() != stored.source || proof.to() != expected_current {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    match proof.selection() {
        ProvenRestartSelection::Preferred { boundary, prefix } => {
            resolve_preferred(source, stored, proof.from(), proof.to(), boundary, prefix)
                .map(RetainedRestartCoordinateAuthority::Preferred)
        }
        ProvenRestartSelection::ZeroFallback { boundary, rejected } => {
            resolve_zero(source, proof.from(), proof.to(), boundary, *rejected)
                .map(RetainedRestartCoordinateAuthority::ZeroFallback)
        }
    }
}

fn resolve_preferred(
    source: &SourceStore,
    stored: &StoredDeferredLfRestart,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    boundary: &ProvenSourceMapping,
    prefix: &ProvenRetainedPrefix,
) -> Result<PreferredDeferredLfRestart, RetainedRestartCoordinateError> {
    let ProvenSourceMapping::Boundary {
        from: old_physical_restart,
        to: current_physical_restart,
        affinity,
    } = boundary
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    if *old_physical_restart != stored.physical_restart
        || *affinity != stored.affinity
        || *affinity != BoundaryAffinity::Before
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let ProvenRetainedPrefix::Range {
        from: old_prefix,
        to: current_prefix,
    } = prefix
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    if old_prefix.start != 0
        || old_prefix.end != *old_physical_restart
        || current_prefix.start != 0
        || current_prefix.end != *current_physical_restart
        || stored.deferred_lf.accepted_projection_cut >= old_prefix.end
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let relative_accepted = stored
        .deferred_lf
        .accepted_projection_cut
        .checked_sub(old_prefix.start)
        .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    let current_accepted_projection_cut = current_prefix
        .start
        .checked_add(relative_accepted)
        .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    let current_accepted = source.observe_prefix_metric_at(current_accepted_projection_cut)?;
    let current_physical = source.observe_prefix_metric_at(*current_physical_restart)?;
    let current_bridge = DeferredLfBridge::try_new(current_accepted, current_physical)?;
    let current_line = source.observe_lf_line_boundary_at(*current_physical_restart)?;
    if current_bridge.accepted_projection_utf16 != stored.deferred_lf.accepted_projection_utf16
        || current_bridge.physical_restart_utf16 != stored.deferred_lf.physical_restart_utf16
        || current_line.completed_line_ordinal != stored.physical_line.completed_line_ordinal
        || current_line.previous_content_bytes != stored.physical_line.previous_content_bytes
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    let source_bytes_examined = validate_current_lf(source, current_bridge)?;
    let cursors = source.issue_resume_cursor_pair(*current_physical_restart)?;
    if cursors.descriptor() != to
        || cursors.offset() != *current_physical_restart
        || !cursors.is_physical_line_start()
    {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    Ok(PreferredDeferredLfRestart {
        from,
        to,
        coordinates: RestartCoordinateView {
            old_accepted_projection_cut: stored.deferred_lf.accepted_projection_cut,
            old_accepted_projection_utf16: stored.deferred_lf.accepted_projection_utf16,
            old_physical_restart: *old_physical_restart,
            old_physical_restart_utf16: stored.deferred_lf.physical_restart_utf16,
            current_accepted_projection_cut,
            current_accepted_projection_utf16: current_bridge.accepted_projection_utf16,
            current_physical_restart: *current_physical_restart,
            current_physical_restart_utf16: current_bridge.physical_restart_utf16,
            old_completed_line_ordinal: stored.physical_line.completed_line_ordinal,
            old_previous_content_bytes: stored.physical_line.previous_content_bytes,
            current_completed_line_ordinal: current_line.completed_line_ordinal,
            current_previous_content_bytes: current_line.previous_content_bytes,
            affinity: *affinity,
        },
        bridge: RevalidatedDeferredLfBridge {
            old_start: stored.deferred_lf.accepted_projection_cut,
            old_end: stored.deferred_lf.physical_restart,
            current_start: current_accepted_projection_cut,
            current_end: *current_physical_restart,
            source_bytes_examined,
        },
        cursors,
    })
}

fn resolve_zero(
    source: &SourceStore,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    boundary: &ProvenSourceMapping,
    rejected: Option<RejectedPreferredRestart>,
) -> Result<ZeroRestartCoordinate, RetainedRestartCoordinateError> {
    let ProvenSourceMapping::Boundary {
        from: 0,
        to: 0,
        affinity: BoundaryAffinity::Before,
    } = boundary
    else {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    };
    let rejected = rejected.ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
    let cursors = source.issue_resume_cursor_pair(0)?;
    if cursors.descriptor() != to || cursors.offset() != 0 || !cursors.is_physical_line_start() {
        return Err(RetainedRestartCoordinateError::ProofMismatch);
    }
    Ok(ZeroRestartCoordinate {
        from,
        to,
        rejected,
        cursors,
    })
}

/// Reads one typed LF through the source store's bounded borrowed observation.
/// No source lease, cursor, snapshot owner, or byte buffer survives the call.
fn validate_current_lf(
    source: &SourceStore,
    bridge: DeferredLfBridge,
) -> Result<usize, RetainedRestartCoordinateError> {
    let descriptor = source.descriptor();
    let observed = source.observe_byte_at(bridge.accepted_projection_cut)?;
    if observed
        .as_ref()
        .map(|byte| (byte.root, byte.offset, byte.byte))
        != Some((descriptor.root, bridge.accepted_projection_cut, b'\n'))
    {
        return Err(RetainedRestartCoordinateError::DeferredLfMismatch {
            source: descriptor,
            offset: bridge.accepted_projection_cut,
            observed: observed.map(|byte| byte.byte),
        });
    }
    Ok(1)
}

/// The persisted first gate is deliberately bare-LF only. Querying the byte
/// before the LF is bounded constant work and prevents a CRLF checkpoint from
/// being mislabeled as the narrower one-byte bridge merely because A+1=P.
#[cfg(feature = "exact-parser")]
fn validate_current_bare_lf(
    source: &SourceStore,
    bridge: DeferredLfBridge,
) -> Result<usize, RetainedRestartCoordinateError> {
    let mut examined = validate_current_lf(source, bridge)?;
    if bridge.accepted_projection_cut != 0 {
        examined = examined
            .checked_add(1)
            .ok_or(RetainedRestartCoordinateError::ProofMismatch)?;
        if source
            .observe_byte_at(bridge.accepted_projection_cut - 1)?
            .is_some_and(|byte| byte.byte == b'\r')
        {
            return Err(RetainedRestartCoordinateError::ProofMismatch);
        }
    }
    Ok(examined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RestartSelectionRegion, SourceRevision, SourceRootId};

    const OLD: &str = "lead\nold\n===\n";

    fn captured() -> (SourceStore, StoredDeferredLfRestart) {
        let store = SourceStore::new(OLD, 8);
        let stored = StoredDeferredLfRestart::capture(&store, 4, 5).unwrap();
        (store, stored)
    }

    fn drive_ready(
        job: &mut RetainedRestartCoordinateJob,
        source: &SourceStore,
    ) -> RetainedRestartCoordinateAuthority {
        for _ in 0..32 {
            match job.poll(source, 1).unwrap() {
                RetainedRestartCoordinateProgress::Pending { .. } => {}
                RetainedRestartCoordinateProgress::Ready(authority) => return *authority,
            }
        }
        panic!("restart-coordinate job did not converge");
    }

    #[test]
    fn setext_fixture_proves_nonzero_physical_restart_and_deferred_lf_cut() {
        let (mut source, stored) = captured();
        let old = source.descriptor();
        assert_eq!(StoredDeferredLfRestart::RETAINED_SOURCE_BYTES_FOR_TEST, 0);
        source
            .apply_edit(SourceRevision(0), 5..8, "")
            .expect("delete only `old`");
        assert_eq!(
            source.query_view().materialize_for_testing(),
            "lead\n\n===\n"
        );

        let mut job = RetainedRestartCoordinateJob::begin(&source, stored).unwrap();
        assert!(matches!(
            job.poll(&source, 0).unwrap(),
            RetainedRestartCoordinateProgress::Pending {
                processed_records: 0,
                remaining_records: 1
            }
        ));
        let RetainedRestartCoordinateAuthority::Preferred(preferred) =
            drive_ready(&mut job, &source)
        else {
            panic!("edit beginning at byte five must retain the nonzero prefix");
        };
        assert_eq!(preferred.from(), old);
        assert_eq!(preferred.to(), source.descriptor());
        assert_eq!(
            preferred.coordinates(),
            RestartCoordinateView {
                old_accepted_projection_cut: 4,
                old_accepted_projection_utf16: 4,
                old_physical_restart: 5,
                old_physical_restart_utf16: 5,
                current_accepted_projection_cut: 4,
                current_accepted_projection_utf16: 4,
                current_physical_restart: 5,
                current_physical_restart_utf16: 5,
                old_completed_line_ordinal: 1,
                old_previous_content_bytes: 4,
                current_completed_line_ordinal: 1,
                current_previous_content_bytes: 4,
                affinity: BoundaryAffinity::Before,
            }
        );
        assert_eq!(preferred.bridge.old_start, 4);
        assert_eq!(preferred.bridge.old_end, 5);
        assert_eq!(preferred.bridge.current_start, 4);
        assert_eq!(preferred.bridge.current_end, 5);
        assert_eq!(preferred.bridge.source_bytes_examined, 1);

        let pair = preferred.into_cursor_pair();
        assert_eq!(pair.descriptor(), source.descriptor());
        assert_eq!(pair.offset(), 5);
        assert!(pair.is_physical_line_start());
        let (mut authoritative, mut recognition) = pair.into_cursors();
        assert_eq!(authoritative.offset(), 5);
        assert_eq!(recognition.offset(), 5);
        assert_eq!(authoritative.next_byte().map(|byte| byte.byte), Some(b'\n'));
        assert_eq!(recognition.next_byte().map(|byte| byte.byte), Some(b'\n'));
    }

    #[test]
    fn retained_restart_certifies_non_ascii_utf16_prefix_metrics() {
        let mut source = SourceStore::new("😀\nold\n===\n", 8);
        let stored = StoredDeferredLfRestart::capture(&source, 4, 5).unwrap();
        source
            .apply_edit(SourceRevision(0), 5..8, "")
            .expect("delete only `old`");

        let mut job = RetainedRestartCoordinateJob::begin(&source, stored).unwrap();
        let RetainedRestartCoordinateAuthority::Preferred(preferred) =
            drive_ready(&mut job, &source)
        else {
            panic!("unchanged emoji prefix must retain the nonzero restart");
        };
        assert_eq!(
            preferred.coordinates(),
            RestartCoordinateView {
                old_accepted_projection_cut: 4,
                old_accepted_projection_utf16: 2,
                old_physical_restart: 5,
                old_physical_restart_utf16: 3,
                current_accepted_projection_cut: 4,
                current_accepted_projection_utf16: 2,
                current_physical_restart: 5,
                current_physical_restart_utf16: 3,
                old_completed_line_ordinal: 1,
                old_previous_content_bytes: 4,
                current_completed_line_ordinal: 1,
                current_previous_content_bytes: 4,
                affinity: BoundaryAffinity::Before,
            }
        );
        assert_eq!(preferred.into_cursor_pair().offset(), 5);
    }

    #[test]
    fn collapsed_or_non_lf_coordinates_never_become_stored_authority() {
        let source = SourceStore::new(OLD, 8);
        assert_eq!(
            StoredDeferredLfRestart::capture(&source, 5, 5),
            Err(RetainedRestartCoordinateError::CollapsedCoordinates { coordinate: 5 })
        );
        assert_eq!(
            StoredDeferredLfRestart::capture(&source, 3, 5),
            Err(RetainedRestartCoordinateError::DeferredLfWidth {
                accepted_projection_cut: 3,
                physical_restart: 5,
            })
        );
        let not_lf = SourceStore::new("leadxold\n===\n", 8);
        assert_eq!(
            StoredDeferredLfRestart::capture(&not_lf, 4, 5),
            Err(RetainedRestartCoordinateError::DeferredLfMismatch {
                source: not_lf.descriptor(),
                offset: 4,
                observed: Some(b'x'),
            })
        );
    }

    #[test]
    fn borrowed_byte_observation_is_exact_at_eof() {
        let source = SourceStore::new("x", 8);
        assert_eq!(
            source.observe_byte_at(0),
            Ok(Some(crate::SourceByte {
                root: source.descriptor().root,
                offset: 0,
                byte: b'x',
            }))
        );
        assert_eq!(source.observe_byte_at(1), Ok(None));
        assert_eq!(source.observe_byte_at(2), Err(SourceError::InvalidRange));
    }

    #[test]
    fn every_edit_touching_the_retained_prefix_mints_typed_zero_fallback() {
        for range in [0..1, 4..5, 3..5] {
            let (mut source, stored) = captured();
            let old = source.descriptor();
            source
                .apply_edit(SourceRevision(0), range, "")
                .expect("ASCII prefix edit");
            let mut job = RetainedRestartCoordinateJob::begin(&source, stored).unwrap();
            let RetainedRestartCoordinateAuthority::ZeroFallback(zero) =
                drive_ready(&mut job, &source)
            else {
                panic!("changed retained prefix must reject byte-five reuse");
            };
            assert_eq!(zero.from(), old);
            assert_eq!(zero.to(), source.descriptor());
            assert_eq!(
                zero.rejected().region,
                RestartSelectionRegion::RetainedPrefix
            );
            let pair = zero.into_cursor_pair();
            assert_eq!(pair.offset(), 0);
            assert!(pair.is_physical_line_start());
        }
    }

    #[test]
    fn forged_affinity_root_or_revision_is_rejected_before_resume() {
        let (source, stored) = captured();

        let mut wrong_affinity = stored.clone();
        wrong_affinity.affinity = BoundaryAffinity::After;
        assert!(matches!(
            RetainedRestartCoordinateJob::begin(&source, wrong_affinity),
            Err(RetainedRestartCoordinateError::WrongAffinity(
                BoundaryAffinity::After
            ))
        ));

        let mut wrong_root = stored.clone();
        wrong_root.source.root = SourceRootId(wrong_root.source.root.0 + 1);
        wrong_root.physical_line.root = wrong_root.source.root;
        assert!(matches!(
            RetainedRestartCoordinateJob::begin(&source, wrong_root),
            Err(RetainedRestartCoordinateError::Selection(
                RestartSelectionError::SnapshotMismatch { .. }
            ))
        ));

        let mut wrong_revision = stored;
        wrong_revision.source.revision = SourceRevision(1);
        assert!(matches!(
            RetainedRestartCoordinateJob::begin(&source, wrong_revision),
            Err(RetainedRestartCoordinateError::Selection(_))
        ));
    }

    #[test]
    fn source_advance_after_job_construction_rejects_frozen_old_generation() {
        let (mut source, stored) = captured();
        source
            .apply_edit(SourceRevision(0), 5..8, "")
            .expect("preferred restart survives");
        let frozen = source.descriptor();
        let mut job = RetainedRestartCoordinateJob::begin(&source, stored).unwrap();
        let end = source.descriptor().bytes;
        source
            .apply_edit(SourceRevision(1), end..end, "tail")
            .expect("live source advances after lineage snapshot");
        assert_eq!(
            job.poll(&source, 1).unwrap_err(),
            RetainedRestartCoordinateError::SourceAdvanced {
                expected: frozen,
                actual: source.descriptor(),
            }
        );
        assert_eq!(
            job.poll(&source, 1).unwrap_err(),
            RetainedRestartCoordinateError::AlreadyComplete
        );
    }

    #[test]
    fn polling_and_cancellation_report_real_scalar_lineage_work() {
        let (mut source, stored) = captured();
        source
            .apply_edit(SourceRevision(0), 5..8, "")
            .expect("first suffix edit");
        let end = source.descriptor().bytes;
        source
            .apply_edit(SourceRevision(1), end..end, "tail")
            .expect("second suffix edit");
        let mut job = RetainedRestartCoordinateJob::begin(&source, stored).unwrap();
        assert!(matches!(
            job.poll(&source, 0).unwrap(),
            RetainedRestartCoordinateProgress::Pending {
                processed_records: 0,
                remaining_records: 2
            }
        ));
        assert!(matches!(
            job.poll(&source, 1).unwrap(),
            RetainedRestartCoordinateProgress::Pending {
                processed_records: 1,
                remaining_records: 1
            }
        ));
        let cancelled = job.cancel();
        assert_eq!(cancelled.metrics.poll_records_examined, 1);
        assert_eq!(cancelled.retention.records, 2);
        assert_eq!(cancelled.retention.retained_source_roots, 0);
        assert!(cancelled.retention.tree_nodes <= cancelled.retention.maximum_tree_nodes);
    }
}

#[cfg(all(test, feature = "exact-parser"))]
mod persisted_parent_tests {
    use super::*;
    use crate::candidate_writer::RestartCompositeChildren;
    use crate::committed_checkpoint_index::{
        DonorCheckpointSampleDraft, RelativeCheckpointMeasure, StorageOnlyCheckpointIndexBuilder,
        StorageOnlyCheckpointPartition,
    };
    use crate::serialized_green::{
        CoveragePart, FactsEnvelope, GreenEvent, GreenHeadingOpenFacts, GreenKind,
        LogicalContribution, ResumableSerializedGreenBuild, SerializedGreenRootSpec,
        SerializedGreenStreamProgress, SourceProjectionRun,
    };
    use crate::storage_only_composite_document::{
        RestartCompositeDocument, RestartCompositeDocumentBuilder,
    };
    use crate::{
        BlockId, ClosedChildAggregate, CoverageId, GrammarRevision, GreenAffinity, PageArena,
        ParseGeneration, SourceRevision,
    };
    use flark_comrak_value_block_core::{
        DirectDurableGrammarCapture, DirectPollStatus, DirectValueBlockParser, SyntaxProfile,
    };

    #[derive(Clone, Copy)]
    enum CurrentTerminal {
        Paragraph,
        AtxHeading,
    }

    fn offer(
        build: &mut ResumableSerializedGreenBuild,
        session: &mut crate::ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        build.offer_event(session, event).unwrap();
        loop {
            match build.poll(session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => break,
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("event polling finalized the green manifest")
                }
            }
        }
    }

    fn durable_capture_after(lines: &[&str]) -> DirectDurableGrammarCapture {
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        parser.acknowledge_command().unwrap();
        for line in lines {
            parser.begin_line((*line).to_owned()).unwrap();
            let limit = line.len().saturating_mul(8).saturating_add(256);
            let mut complete = false;
            for _ in 0..limit {
                match parser.poll_line(1).unwrap().status {
                    DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                    DirectPollStatus::Pending => {}
                    DirectPollStatus::ExternalWorkReady => {
                        panic!("non-reference donor fixture unexpectedly requested external work")
                    }
                    DirectPollStatus::Complete => {
                        complete = true;
                        break;
                    }
                }
            }
            assert!(complete, "test line converges within its fuel bound");
        }
        parser
            .capture_durable_grammar_line_boundary_checkpoint()
            .unwrap()
    }

    fn build_parent(
        source_text: &str,
        accepted_bytes: u64,
        donor_lines: &[&str],
        terminal: CurrentTerminal,
    ) -> (PageArena, RestartCompositeDocument, SourceStore) {
        let source = SourceStore::new(source_text, 16);
        let descriptor = source.descriptor();
        let total_bytes = u64::try_from(descriptor.bytes).unwrap();
        let total_utf16 = u64::try_from(source.query_view().len_utf16()).unwrap();
        assert_eq!(total_bytes, total_utf16, "fixture is intentionally ASCII");
        assert!(accepted_bytes > 0 && accepted_bytes <= total_bytes);

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut green = ResumableSerializedGreenBuild::new(
            &ticket,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: descriptor.revision,
                source_root: descriptor.root,
                source_bytes: total_bytes,
                source_utf16: total_utf16,
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(1),
                semantic_epoch: 1,
                known_bytes: 0..total_bytes,
            },
        )
        .unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut green,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        let (terminal_kind, terminal_facts) = match terminal {
            CurrentTerminal::Paragraph => (GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            CurrentTerminal::AtxHeading => (
                GreenKind::HEADING,
                GreenHeadingOpenFacts::atx(1).unwrap().into_envelope(),
            ),
        };
        offer(
            &mut green,
            &mut session,
            GreenEvent::enter(BlockId(2), terminal_kind, terminal_facts),
        );
        offer(
            &mut green,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(1),
                    accepted_bytes,
                    accepted_bytes,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        let checkpoint_events = 3_u64;
        let checkpoint_runs = 1_u64;
        let remaining = total_bytes - accepted_bytes;
        if remaining != 0 {
            offer(
                &mut green,
                &mut session,
                GreenEvent::Coverage(
                    SourceProjectionRun::with_logical(
                        CoverageId(2),
                        remaining,
                        remaining,
                        0,
                        CoveragePart::TERMINAL,
                        BlockId(2),
                        LogicalContribution::Hidden {
                            affinity: GreenAffinity::Downstream,
                        },
                    )
                    .unwrap(),
                ),
            );
        }
        offer(
            &mut green,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut green,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        green.finish_input(&mut session).unwrap();
        loop {
            match green.poll(&mut session).unwrap() {
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("green finalization returned to input")
                }
            }
        }
        let green = green.take_manifest().unwrap();

        let checkpoint_cut = RelativeCheckpointMeasure::new(
            total_bytes,
            total_utf16,
            u64::try_from(donor_lines.len()).unwrap(),
            checkpoint_events,
            checkpoint_runs,
        );
        let sample =
            DonorCheckpointSampleDraft::try_new(checkpoint_cut, durable_capture_after(donor_lines))
                .unwrap();
        let mut index = StorageOnlyCheckpointIndexBuilder::default();
        index
            .push(StorageOnlyCheckpointPartition::donor_direct(sample))
            .unwrap();
        index
            .push(StorageOnlyCheckpointPartition::terminal_tail(
                RelativeCheckpointMeasure::new(
                    0,
                    0,
                    0,
                    2 + u64::from(remaining != 0),
                    u64::from(remaining != 0),
                ),
            ))
            .unwrap();
        let index = index.build_in_session(&mut session).unwrap();
        let children = RestartCompositeChildren::from_independent_test_children(green, index);
        let parent = RestartCompositeDocumentBuilder::join(&mut session, children)
            .unwrap()
            .commit(session)
            .unwrap()
            .0;
        (arena, parent, source)
    }

    fn parent_mint(
        arena: &PageArena,
        parent: &RestartCompositeDocument,
        physical_restart: u64,
    ) -> RestartSourceLedgerCheckpointMint {
        parent
            .locate_donor_checkpoint_at_or_before_cut(arena, physical_restart)
            .unwrap()
            .expect("fixture has one donor")
            .into_source_ledger_restart_mint()
            .unwrap()
    }

    fn drive_ready(
        job: &mut PersistedRestartCoordinateJob,
        source: &SourceStore,
    ) -> PersistedRestartCoordinateAuthority {
        for _ in 0..32 {
            match job.poll(source, 1).unwrap() {
                PersistedRestartCoordinateProgress::Pending { .. } => {}
                PersistedRestartCoordinateProgress::Ready(authority) => return *authority,
            }
        }
        panic!("persisted restart-coordinate job did not converge")
    }

    #[test]
    fn parent_mint_lineage_and_current_line_query_reconstruct_the_nonzero_coordinate() {
        let (arena, parent, mut source) =
            build_parent("lead\n", 4, &["lead\n"], CurrentTerminal::Paragraph);
        let old = source.descriptor();
        source
            .apply_edit(SourceRevision(0), 5..5, "tail")
            .expect("suffix-only insertion");
        let mint = parent_mint(&arena, &parent, 5);
        let mut job = PersistedRestartCoordinateJob::begin(&source, mint).unwrap();
        assert!(matches!(
            job.poll(&source, 0).unwrap(),
            PersistedRestartCoordinateProgress::Pending {
                processed_records: 0,
                remaining_records: 1
            }
        ));
        let PersistedRestartCoordinateAuthority::PreferredDeferredLf(preferred) =
            drive_ready(&mut job, &source)
        else {
            panic!("unchanged prefix must retain P=5")
        };
        assert_eq!(preferred.from(), old);
        assert_eq!(preferred.to(), source.descriptor());
        assert_eq!(
            preferred.coordinates(),
            PersistedRestartCoordinateView {
                old_accepted_projection_cut: 4,
                old_accepted_projection_utf16: 4,
                old_physical_restart: 5,
                old_physical_restart_utf16: 5,
                current_accepted_projection_cut: 4,
                current_accepted_projection_utf16: 4,
                current_physical_restart: 5,
                current_physical_restart_utf16: 5,
                parent_completed_line_ordinal: 1,
                current_completed_line_ordinal: 1,
                current_previous_content_bytes: 4,
                current_previous_content_utf16: 4,
                affinity: BoundaryAffinity::Before,
            }
        );
        let receipt = preferred.receipt();
        assert_eq!(receipt.deferred_lf_bytes_examined, 2);
        assert_eq!(receipt.retained_old_source_roots, 0);
        assert_eq!(receipt.retained_old_source_bytes, 0);
        assert_eq!(receipt.retained_old_writer_drafts, 0);
        assert_eq!(receipt.line_query.retained_source_roots, 0);
        assert_eq!(receipt.line_query.retained_source_bytes, 0);

        let (_, _, coordinates, _, _, path, donor, _parent_selection, _) =
            preferred.into_reconstruction_parts();
        assert_eq!(path.event_cut(), 3);
        assert_eq!(path.source_metric().bytes, 4);
        assert_eq!(path.frames().len(), 2);
        assert_eq!(path.frames()[0].block(), BlockId(1));
        assert_eq!(path.frames()[1].block(), BlockId(2));
        assert_eq!(path.frames()[1].logical_metric().bytes, 4);
        crate::source_bound_ledger::validate_persisted_lf_path_shape_for_test(&path, coordinates)
            .unwrap();
        let (grammar, line_local) = donor.decode_grammar_parts().unwrap();
        let bound = path
            .bind_direct_restart_output_from_stabilized_line_mechanism_only(&grammar, line_local)
            .unwrap();
        assert_eq!(bound.path().frames()[1].block(), BlockId(2));
    }

    #[test]
    fn crossed_source_complete_or_two_byte_parent_cuts_fail_before_lineage_authority() {
        let (arena, parent, source) =
            build_parent("lead\n", 5, &["lead\n"], CurrentTerminal::Paragraph);
        assert!(matches!(
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 5)),
            Err(RetainedRestartCoordinateError::ProofMismatch)
        ));

        let (arena, parent, _source) =
            build_parent("lead\n", 3, &["lead\n"], CurrentTerminal::Paragraph);
        assert!(
            parent
                .locate_donor_checkpoint_at_or_before_cut(&arena, 5)
                .unwrap()
                .expect("fixture has one donor")
                .into_source_ledger_restart_mint()
                .is_err()
        );
    }

    #[test]
    fn crlf_is_not_admitted_by_the_bare_lf_slice() {
        let (arena, parent, source) =
            build_parent("abc\r\n", 4, &["abc\r\n"], CurrentTerminal::Paragraph);
        let mut job =
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 5)).unwrap();
        assert_eq!(
            job.poll(&source, 1).unwrap_err(),
            RetainedRestartCoordinateError::ProofMismatch
        );
    }

    #[test]
    fn changed_prefix_forces_typed_zero_fallback() {
        let (arena, parent, mut source) =
            build_parent("lead\n", 4, &["lead\n"], CurrentTerminal::Paragraph);
        source
            .apply_edit(SourceRevision(0), 0..1, "L")
            .expect("prefix edit");
        let mut job =
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 5)).unwrap();
        let PersistedRestartCoordinateAuthority::ZeroFallback(zero) =
            drive_ready(&mut job, &source)
        else {
            panic!("changed prefix must reject the preferred checkpoint")
        };
        assert_eq!(zero.into_cursor_pair().offset(), 0);
    }

    #[test]
    fn source_advance_after_a_yield_rejects_the_frozen_generation() {
        let (arena, parent, mut source) =
            build_parent("lead\n", 4, &["lead\n"], CurrentTerminal::Paragraph);
        source
            .apply_edit(SourceRevision(0), 5..5, "one")
            .expect("first suffix edit");
        let frozen = source.descriptor();
        let mut job =
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 5)).unwrap();
        assert!(matches!(
            job.poll(&source, 0).unwrap(),
            PersistedRestartCoordinateProgress::Pending { .. }
        ));
        let end = source.descriptor().bytes;
        source
            .apply_edit(SourceRevision(1), end..end, "two")
            .expect("source advances after the lineage snapshot");
        assert_eq!(
            job.poll(&source, 1).unwrap_err(),
            RetainedRestartCoordinateError::SourceAdvanced {
                expected: frozen,
                actual: source.descriptor(),
            }
        );
    }

    #[test]
    fn cancellation_drops_path_and_donor_with_scalar_only_lineage_receipt() {
        let (arena, parent, mut source) =
            build_parent("lead\n", 4, &["lead\n"], CurrentTerminal::Paragraph);
        source
            .apply_edit(SourceRevision(0), 5..5, "one")
            .expect("first suffix edit");
        let end = source.descriptor().bytes;
        source
            .apply_edit(SourceRevision(1), end..end, "two")
            .expect("second suffix edit");
        let mut job =
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 5)).unwrap();
        assert!(matches!(
            job.poll(&source, 1).unwrap(),
            PersistedRestartCoordinateProgress::Pending {
                processed_records: 1,
                remaining_records: 1,
            }
        ));
        let cancelled = job.cancel();
        assert_eq!(cancelled.metrics.poll_records_examined, 1);
        assert_eq!(cancelled.retention.records, 2);
        assert_eq!(cancelled.retention.retained_source_roots, 0);
    }

    #[test]
    fn blank_gap_donor_cannot_enter_the_lf_terminator_reconstruction() {
        let (arena, parent, source) =
            build_parent("lead\n\n", 5, &["lead\n", "\n"], CurrentTerminal::Paragraph);
        let mut job =
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 6)).unwrap();
        let PersistedRestartCoordinateAuthority::PreferredDeferredLf(preferred) =
            drive_ready(&mut job, &source)
        else {
            panic!("source coordinate itself remains unchanged")
        };
        let (_, _, coordinates, _, _, path, donor, _parent_selection, _) =
            preferred.into_reconstruction_parts();
        crate::source_bound_ledger::validate_persisted_lf_path_shape_for_test(&path, coordinates)
            .unwrap();
        let (grammar, _) = donor.decode_grammar_parts().unwrap();
        assert!(matches!(
            crate::source_bound_ledger::require_persisted_lf_donor_role_for_test(&grammar),
            Err(
                crate::source_bound_ledger::PersistedSourceDonorResumeError::DeferredRole(
                    flark_comrak_value_block_core::DirectLineBoundaryDeferredRole::BlankGap { .. }
                )
            )
        ));
    }

    #[test]
    fn generic_heading_path_cannot_masquerade_as_the_lf_paragraph_restart() {
        let (arena, parent, source) =
            build_parent("lead\n", 4, &["lead\n"], CurrentTerminal::AtxHeading);
        let mut job =
            PersistedRestartCoordinateJob::begin(&source, parent_mint(&arena, &parent, 5)).unwrap();
        let PersistedRestartCoordinateAuthority::PreferredDeferredLf(preferred) =
            drive_ready(&mut job, &source)
        else {
            panic!("source coordinate itself remains unchanged")
        };
        let (_, _, coordinates, _, _, path, _, _parent_selection, _) =
            preferred.into_reconstruction_parts();
        assert_eq!(path.frames()[1].green_kind(), GreenKind::HEADING);
        assert!(matches!(
            crate::source_bound_ledger::validate_persisted_lf_path_shape_for_test(
                &path,
                coordinates,
            ),
            Err(crate::SourceBoundLedgerError::LineBoundaryContinuationUnavailable)
        ));
    }
}

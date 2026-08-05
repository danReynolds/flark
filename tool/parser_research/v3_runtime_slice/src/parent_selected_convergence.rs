//! Source/green mapping for one parent-selected convergence checkpoint.
//!
//! A committed parser checkpoint is physically after a line terminator (P),
//! while the reusable green/source suffix may begin before that deferred atom
//! (semantic cut A). This module keeps those cuts distinct: retained green
//! derives A, source lineage maps A, and the existing source-tail join derives
//! current P from authoritative source bytes plus the first retained coverage
//! run. It neither parses Markdown nor authorizes final tree reuse.

use crate::committed_checkpoint_index::{
    CommittedCheckpointIndexError, ParentBoundDonorSuccessor, ParentBoundSourceConvergence,
    RelativeCheckpointMeasure,
};
use crate::storage_only_composite_document::RestartCompositeDocumentError;
use crate::{
    BoundaryAffinity, GreenSourceTailAdoptionCapability, GreenSourceTailAdoptionReceipt,
    LineageAdoptionBundleError, LineageAdoptionBundleJob, LineageAdoptionBundleMetrics,
    LineageAdoptionBundleStatus, LineageAdoptionRegion, LineageSnapshotRetention,
    LiveCandidateEpoch, SerializedGreenError, SerializedMetric, SourceBoundGreenTailAdoption,
    SourceRevision, SourceSnapshotDescriptor, SourceStore, TailAdoptionJoinError,
};

/// Private mint allowing this module alone to split the old parent-bound R/C
/// checkpoint authority into inputs for the source-owned lineage primitive.
pub(crate) struct ParentBoundSourceLineageMint(());

/// Private mint allowing the candidate writer to hand one freshly captured
/// actor sample to this coordinator without exposing its coordinates.
pub(crate) struct ParentSelectedConvergenceSampleMint(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedConvergenceMapError {
    Checkpoint(CommittedCheckpointIndexError),
    Parent(RestartCompositeDocumentError),
    Green(SerializedGreenError),
    Lineage(LineageAdoptionBundleError),
    Tail(TailAdoptionJoinError),
    SourceAdvanced,
    Overflow(&'static str),
    Invariant(&'static str),
}

/// Expected reasons one authenticated old C cannot be reused. These are not
/// candidate corruption: the paused driver receives old-C authority back and
/// may advance to its next retained successor or choose full replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedConvergenceIneligibleReason {
    Lineage(LineageAdoptionBundleError),
    Tail(TailAdoptionJoinError),
    DeferredTerminatorChanged,
}

#[derive(Debug)]
pub(crate) enum ParentSelectedConvergenceMapStart {
    Mapping(ParentSelectedConvergenceMapJob),
    Ineligible {
        old_convergence: ParentBoundDonorSuccessor,
        reason: ParentSelectedConvergenceIneligibleReason,
    },
}

impl From<CommittedCheckpointIndexError> for ParentSelectedConvergenceMapError {
    fn from(error: CommittedCheckpointIndexError) -> Self {
        Self::Checkpoint(error)
    }
}

impl From<RestartCompositeDocumentError> for ParentSelectedConvergenceMapError {
    fn from(error: RestartCompositeDocumentError) -> Self {
        Self::Parent(error)
    }
}

impl From<SerializedGreenError> for ParentSelectedConvergenceMapError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

impl From<LineageAdoptionBundleError> for ParentSelectedConvergenceMapError {
    fn from(error: LineageAdoptionBundleError) -> Self {
        Self::Lineage(error)
    }
}

impl From<TailAdoptionJoinError> for ParentSelectedConvergenceMapError {
    fn from(error: TailAdoptionJoinError) -> Self {
        Self::Tail(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParentSelectedConvergenceMapReceipt {
    pub(crate) lineage: LineageAdoptionBundleMetrics,
    pub(crate) retention: LineageSnapshotRetention,
    pub(crate) green_source_tail: GreenSourceTailAdoptionReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedConvergenceTargetRelation {
    Before,
    At,
    Past,
}

#[derive(Debug)]
pub(crate) enum ParentSelectedLiveDonorJoin {
    Mismatch {
        old_convergence: ParentBoundDonorSuccessor,
        rejected: crate::ParentSelectedRejectedSuffixSample,
    },
    Match(ParentSelectedMatchedConvergence),
}

/// Linear proof that the exact live sample which matched old C remains beside
/// the pending global join. This module mints it only after the opaque donor
/// witness matches; no raw coordinate constructor escapes that decision.
#[must_use = "the matched live sample certificate must reach the pending convergence join"]
#[derive(Debug)]
pub(crate) struct ParentSelectedMatchedLiveSampleCertificate {
    epoch: LiveCandidateEpoch,
    interval: RelativeCheckpointMeasure,
    cumulative_cut: RelativeCheckpointMeasure,
    sample_ordinal: u64,
}

impl ParentSelectedMatchedLiveSampleCertificate {
    pub(crate) fn into_checkpoint_splice_parts(
        self,
        _mint: crate::committed_checkpoint_index::suffix_splice::ParentSelectedCheckpointSpliceMint,
    ) -> (
        LiveCandidateEpoch,
        RelativeCheckpointMeasure,
        RelativeCheckpointMeasure,
        u64,
    ) {
        (
            self.epoch,
            self.interval,
            self.cumulative_cut,
            self.sample_ordinal,
        )
    }

    #[cfg(test)]
    pub(crate) const fn receipt_for_test(
        &self,
    ) -> (
        LiveCandidateEpoch,
        RelativeCheckpointMeasure,
        RelativeCheckpointMeasure,
        u64,
    ) {
        (
            self.epoch,
            self.interval,
            self.cumulative_cut,
            self.sample_ordinal,
        )
    }
}

/// Live source axes and donor grammar both agree with old C. Green/composer
/// adoption is still a separate mandatory join.
#[derive(Debug)]
pub(crate) struct ParentSelectedMatchedConvergence {
    old_convergence: ParentBoundDonorSuccessor,
    tail: SourceBoundGreenTailAdoption,
    certificate: ParentSelectedMatchedLiveSampleCertificate,
}

#[derive(Debug)]
pub(crate) enum ParentSelectedConvergenceMapProgress {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    /// Source regions required by this C changed. The exact old checkpoint is
    /// returned so the paused driver can advance within its retained manifest.
    Changed {
        old_convergence: ParentBoundDonorSuccessor,
        region: LineageAdoptionRegion,
        at_revision: SourceRevision,
    },
    Ineligible {
        old_convergence: ParentBoundDonorSuccessor,
        reason: ParentSelectedConvergenceIneligibleReason,
    },
    Mapped(ParentSelectedMappedConvergence),
}

#[derive(Debug)]
pub(crate) struct ParentSelectedConvergenceMapJob {
    inner: Option<LineageAdoptionBundleJob>,
    epoch: LiveCandidateEpoch,
    frozen_target: SourceSnapshotDescriptor,
    old_convergence: Option<ParentBoundDonorSuccessor>,
    old_semantic_prefix: SerializedMetric,
    old_physical_prefix: SerializedMetric,
    storage: Option<GreenSourceTailAdoptionCapability>,
    retention: LineageSnapshotRetention,
}

/// Source-bound current C. It owns the old checkpoint and the consumed lineage
/// authority embodied by `tail`; target coordinates are never exposed to the
/// exact driver.
#[derive(Debug)]
pub(crate) struct ParentSelectedMappedConvergence {
    old_convergence: ParentBoundDonorSuccessor,
    tail: SourceBoundGreenTailAdoption,
    receipt: ParentSelectedConvergenceMapReceipt,
}

impl ParentSelectedConvergenceMapJob {
    pub(crate) fn begin(
        source: &SourceStore,
        epoch: LiveCandidateEpoch,
        bound: ParentBoundSourceConvergence,
        storage: GreenSourceTailAdoptionCapability,
    ) -> Result<ParentSelectedConvergenceMapStart, ParentSelectedConvergenceMapError> {
        let (restart_cut, old_physical_cut, old_convergence) =
            bound.into_lineage_parts(ParentBoundSourceLineageMint(()));
        let old_source = storage.old_source();
        let old_semantic_prefix = storage.old_prefix();
        let old_physical_prefix = SerializedMetric {
            bytes: old_physical_cut.source_bytes(),
            utf16: old_physical_cut.source_utf16(),
        };
        let old_restart = usize::try_from(restart_cut.source_bytes())
            .map_err(|_| ParentSelectedConvergenceMapError::Overflow("old restart source bytes"))?;
        let old_semantic = storage.old_convergence_bytes()?;
        if epoch.source() != source.descriptor()
            || old_restart > old_semantic
            || old_semantic >= old_source.bytes
            || old_physical_prefix.bytes < old_semantic_prefix.bytes
            || old_physical_prefix.utf16 < old_semantic_prefix.utf16
            || old_physical_prefix.bytes - old_semantic_prefix.bytes > 2
            || old_physical_prefix.utf16 - old_semantic_prefix.utf16 > 2
            || storage.prefix_coverage_runs() != old_physical_cut.projection_runs()
        {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "old semantic A and parser P do not form one parent-bound C",
            ));
        }
        let inner = match source.begin_lineage_adoption_bundle(
            old_source,
            old_restart,
            old_semantic,
            BoundaryAffinity::After,
        ) {
            Ok(inner) => inner,
            Err(error) => {
                return Ok(ParentSelectedConvergenceMapStart::Ineligible {
                    old_convergence,
                    reason: ParentSelectedConvergenceIneligibleReason::Lineage(error),
                });
            }
        };
        let retention = inner.retention();
        Ok(ParentSelectedConvergenceMapStart::Mapping(Self {
            inner: Some(inner),
            epoch,
            frozen_target: source.descriptor(),
            old_convergence: Some(old_convergence),
            old_semantic_prefix,
            old_physical_prefix,
            storage: Some(storage),
            retention,
        }))
    }

    pub(crate) fn poll(
        &mut self,
        source: &SourceStore,
        fuel: usize,
    ) -> Result<ParentSelectedConvergenceMapProgress, ParentSelectedConvergenceMapError> {
        if source.descriptor() != self.frozen_target || self.epoch.source() != self.frozen_target {
            return Err(ParentSelectedConvergenceMapError::SourceAdvanced);
        }
        let status = self
            .inner
            .as_mut()
            .ok_or(ParentSelectedConvergenceMapError::Invariant(
                "convergence lineage job was already consumed",
            ))?
            .poll(fuel);
        match status {
            LineageAdoptionBundleStatus::Pending {
                processed_records,
                remaining_records,
            } => Ok(ParentSelectedConvergenceMapProgress::Pending {
                processed_records,
                remaining_records,
            }),
            LineageAdoptionBundleStatus::Changed {
                region,
                at_revision,
            } => {
                drop(self.inner.take());
                drop(self.storage.take());
                let old_convergence = self.old_convergence.take().ok_or(
                    ParentSelectedConvergenceMapError::Invariant(
                        "changed convergence job lost its old checkpoint",
                    ),
                )?;
                Ok(ParentSelectedConvergenceMapProgress::Changed {
                    old_convergence,
                    region,
                    at_revision,
                })
            }
            LineageAdoptionBundleStatus::Failed(error) => {
                drop(self.inner.take());
                drop(self.storage.take());
                let old_convergence = self.old_convergence.take().ok_or(
                    ParentSelectedConvergenceMapError::Invariant(
                        "failed convergence job lost its old checkpoint",
                    ),
                )?;
                Ok(ParentSelectedConvergenceMapProgress::Ineligible {
                    old_convergence,
                    reason: ParentSelectedConvergenceIneligibleReason::Lineage(error),
                })
            }
            LineageAdoptionBundleStatus::Proven { .. } => {
                let inner =
                    self.inner
                        .take()
                        .ok_or(ParentSelectedConvergenceMapError::Invariant(
                            "proven convergence job lost its lineage authority",
                        ))?;
                let lineage_metrics = inner.metrics();
                let proof = inner.into_proof().map_err(|status| match status {
                    LineageAdoptionBundleStatus::Failed(error) => {
                        ParentSelectedConvergenceMapError::Lineage(error)
                    }
                    _ => ParentSelectedConvergenceMapError::Invariant(
                        "proven convergence job did not yield its proof",
                    ),
                })?;
                let storage =
                    self.storage
                        .take()
                        .ok_or(ParentSelectedConvergenceMapError::Invariant(
                            "proven convergence job lost its green tail authority",
                        ))?;
                let tail = match storage.join_current_source(source, self.epoch, proof) {
                    Ok(tail) => tail,
                    Err(error) => {
                        let old_convergence = self.old_convergence.take().ok_or(
                            ParentSelectedConvergenceMapError::Invariant(
                                "ineligible convergence job lost its old checkpoint",
                            ),
                        )?;
                        return Ok(ParentSelectedConvergenceMapProgress::Ineligible {
                            old_convergence,
                            reason: ParentSelectedConvergenceIneligibleReason::Tail(error),
                        });
                    }
                };
                let current_semantic = tail.current_prefix();
                let current_physical = tail.physical_prefix();
                if old_metric_delta(self.old_physical_prefix, self.old_semantic_prefix)?
                    != old_metric_delta(current_physical, current_semantic)?
                {
                    let old_convergence = self.old_convergence.take().ok_or(
                        ParentSelectedConvergenceMapError::Invariant(
                            "terminator-ineligible convergence lost its old checkpoint",
                        ),
                    )?;
                    return Ok(ParentSelectedConvergenceMapProgress::Ineligible {
                        old_convergence,
                        reason:
                            ParentSelectedConvergenceIneligibleReason::DeferredTerminatorChanged,
                    });
                }
                let receipt = ParentSelectedConvergenceMapReceipt {
                    lineage: lineage_metrics,
                    retention: self.retention,
                    green_source_tail: tail.receipt(),
                };
                let old_convergence = self.old_convergence.take().ok_or(
                    ParentSelectedConvergenceMapError::Invariant(
                        "proven convergence job lost its old checkpoint",
                    ),
                )?;
                Ok(ParentSelectedConvergenceMapProgress::Mapped(
                    ParentSelectedMappedConvergence {
                        old_convergence,
                        tail,
                        receipt,
                    },
                ))
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn old_convergence_for_test(&self) -> Option<(RelativeCheckpointMeasure, u64)> {
        self.old_convergence
            .as_ref()
            .map(|old| (old.checkpoint_cut_for_test(), old.ordinal_for_test()))
    }
}

impl ParentSelectedMappedConvergence {
    pub(crate) fn relation_to_current_cut(
        &self,
        epoch: LiveCandidateEpoch,
        current: RelativeCheckpointMeasure,
    ) -> Result<ParentSelectedConvergenceTargetRelation, ParentSelectedConvergenceMapError> {
        if epoch != self.tail.epoch() {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "mapped C and exact candidate epochs differ",
            ));
        }
        let target = self.tail.physical_prefix();
        if current.source_bytes() < target.bytes {
            return Ok(ParentSelectedConvergenceTargetRelation::Before);
        }
        if current.source_bytes() > target.bytes {
            return Ok(ParentSelectedConvergenceTargetRelation::Past);
        }
        if current.source_utf16() != target.utf16
            || current.physical_lines() != self.tail.current_line_ordinal()
        {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "actor source axes disagree at mapped physical C",
            ));
        }
        Ok(ParentSelectedConvergenceTargetRelation::At)
    }

    /// Consumes one fresh checkpoint sample into either a retryable old-C
    /// mismatch or a donor-matched source-tail capability. Old absolute green
    /// and projection coordinates are intentionally not compared here: the
    /// subsequent writer tail join validates their rebased current axes.
    pub(crate) fn join_live_donor(
        self,
        sample: crate::CapturedParentSelectedSuffixSample,
    ) -> Result<ParentSelectedLiveDonorJoin, ParentSelectedConvergenceMapError> {
        let (epoch, interval, current, sample_ordinal, witness, rollback) =
            sample.into_convergence_parts(ParentSelectedConvergenceSampleMint(()));
        if self.relation_to_current_cut(epoch, current)?
            != ParentSelectedConvergenceTargetRelation::At
        {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "fresh donor sample was not captured at mapped physical C",
            ));
        }
        if self.old_convergence.matches_identity_witness(&witness) {
            Ok(ParentSelectedLiveDonorJoin::Match(
                ParentSelectedMatchedConvergence {
                    old_convergence: self.old_convergence,
                    tail: self.tail,
                    certificate: ParentSelectedMatchedLiveSampleCertificate {
                        epoch,
                        interval,
                        cumulative_cut: current,
                        sample_ordinal,
                    },
                },
            ))
        } else {
            Ok(ParentSelectedLiveDonorJoin::Mismatch {
                old_convergence: self.old_convergence,
                rejected: crate::ParentSelectedRejectedSuffixSample::from_donor_mismatch(
                    epoch,
                    interval,
                    current,
                    sample_ordinal,
                    rollback,
                ),
            })
        }
    }

    pub(crate) fn into_mismatch_old_convergence(self) -> ParentBoundDonorSuccessor {
        self.old_convergence
    }

    #[cfg(test)]
    pub(crate) const fn target_for_test(&self) -> RelativeCheckpointMeasure {
        RelativeCheckpointMeasure::new(
            self.tail.physical_prefix().bytes,
            self.tail.physical_prefix().utf16,
            self.tail.current_line_ordinal(),
            0,
            0,
        )
    }

    #[cfg(test)]
    pub(crate) const fn old_convergence_for_test(&self) -> (RelativeCheckpointMeasure, u64) {
        (
            self.old_convergence.checkpoint_cut_for_test(),
            self.old_convergence.ordinal_for_test(),
        )
    }

    #[cfg(test)]
    pub(crate) const fn receipt_for_test(&self) -> ParentSelectedConvergenceMapReceipt {
        self.receipt
    }
}

impl ParentSelectedMatchedConvergence {
    pub(crate) fn into_adoption_parts(
        self,
    ) -> (
        ParentBoundDonorSuccessor,
        SourceBoundGreenTailAdoption,
        ParentSelectedMatchedLiveSampleCertificate,
    ) {
        (self.old_convergence, self.tail, self.certificate)
    }
}

fn old_metric_delta(
    physical: SerializedMetric,
    semantic: SerializedMetric,
) -> Result<SerializedMetric, ParentSelectedConvergenceMapError> {
    Ok(SerializedMetric {
        bytes: physical.bytes.checked_sub(semantic.bytes).ok_or(
            ParentSelectedConvergenceMapError::Invariant(
                "physical convergence precedes semantic convergence bytes",
            ),
        )?,
        utf16: physical.utf16.checked_sub(semantic.utf16).ok_or(
            ParentSelectedConvergenceMapError::Invariant(
                "physical convergence precedes semantic convergence UTF-16",
            ),
        )?,
    })
}

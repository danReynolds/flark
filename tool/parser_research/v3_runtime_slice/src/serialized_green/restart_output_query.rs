//! Current committed-green output at an exact event cut.
//!
//! Restart samples may retain coordinate-free grammar state across revisions,
//! but cumulative output facts must come from the current committed green
//! root. This query recovers only the open path and folds each frame's direct
//! closed children from persistent event-range summaries. It never retains or
//! materializes source text and never builds a document-sized event vector.
//! The logical total deliberately does not invent `FencedCode` info-end or
//! literal-start markers; those typed current-green boundaries remain required
//! follow-on work before fenced writer state can be resumed.

#[allow(clippy::wildcard_imports)]
// This private submodule implements one query surface over its parent's codec.
use super::*;

#[cfg(feature = "exact-parser")]
use flark_comrak_value_block_core::tree::ChildSequenceFold;
#[cfg(feature = "exact-parser")]
use flark_comrak_value_block_core::{
    DirectBlockKind, DirectFenceCharacter, DirectFencedCodeFacts, DirectGrammarContinuation,
    DirectHeadingFacts, DirectItemFacts, DirectListFacts, DirectRestartFrameOutput,
    DirectRestartLineLocalContinuation, DirectRestartOutput, ListDelimiter as DirectListDelimiter,
    ListType as DirectListType, ParseError,
};

/// One current-revision open frame at an exact event-token cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenRestartOutputFrame {
    block: BlockId,
    kind: GreenKind,
    facts: FactsEnvelope,
    enter: GreenEnterCapability,
    enter_event_ordinal: u64,
    closed_children: ChildSequenceAggregate,
    physical_metric: SerializedMetric,
    logical_metric: SerializedMetric,
}

impl GreenRestartOutputFrame {
    #[must_use]
    pub const fn block(&self) -> BlockId {
        self.block
    }

    #[must_use]
    pub const fn kind(&self) -> GreenKind {
        self.kind
    }

    #[must_use]
    pub const fn facts(&self) -> &FactsEnvelope {
        &self.facts
    }

    #[must_use]
    pub const fn enter(&self) -> GreenEnterCapability {
        self.enter
    }

    #[must_use]
    pub const fn enter_event_ordinal(&self) -> u64 {
        self.enter_event_ordinal
    }

    #[must_use]
    pub const fn closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children
    }

    /// Physical source covered after this Enter and before the queried cut.
    /// The output query already folds this range for structural validation;
    /// retaining the scalar avoids a second packed-leaf decode by consumers
    /// which must recover the Enter's exact source-before coordinate.
    #[must_use]
    pub const fn physical_metric(&self) -> SerializedMetric {
        self.physical_metric
    }

    /// Current-root logical output accumulated by the one open terminal.
    /// Container frames always report zero.
    #[must_use]
    pub const fn logical_metric(&self) -> SerializedMetric {
        self.logical_metric
    }

    /// Zero-copy handoff for the crate-internal donor/adoption join that will
    /// consume this authority in the next integration step.
    #[allow(dead_code)] // The donor adapter is deliberately not part of this prototype cut.
    pub(crate) fn into_parts(self) -> GreenRestartOutputFrameParts {
        GreenRestartOutputFrameParts {
            block: self.block,
            kind: self.kind,
            facts: self.facts,
            enter: self.enter,
            enter_event_ordinal: self.enter_event_ordinal,
            closed_children: self.closed_children,
            physical_metric: self.physical_metric,
            logical_metric: self.logical_metric,
        }
    }
}

#[allow(dead_code)] // Reserved for the deliberately deferred donor/adoption join.
pub(crate) struct GreenRestartOutputFrameParts {
    pub(crate) block: BlockId,
    pub(crate) kind: GreenKind,
    pub(crate) facts: FactsEnvelope,
    pub(crate) enter: GreenEnterCapability,
    pub(crate) enter_event_ordinal: u64,
    pub(crate) closed_children: ChildSequenceAggregate,
    pub(crate) physical_metric: SerializedMetric,
    pub(crate) logical_metric: SerializedMetric,
}

/// Auditable work and scratch performed by one event-cut output query.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenRestartOutputReceipt {
    /// One prefix validation range plus one child-output range per open frame.
    pub range_queries: usize,
    pub sequence_nodes_visited: usize,
    pub summary_nodes_reused: usize,
    pub leaf_pages_decoded: usize,
    pub events_decoded: usize,
    /// Maximum bytes simultaneously retained while decoding one packed leaf:
    /// its borrowed arena payload, the decoded event vector capacity, and all
    /// nested Enter fact-field and fact-value vector capacities.
    pub maximum_decoded_page_bytes: usize,
    pub maximum_route_depth: usize,
    pub maximum_open_depth: usize,
    /// Exact allocator capacities owned by the returned frame vector and its
    /// nested fact vectors. These fields exclude stack/inline bytes.
    pub open_frame_capacity_bytes: usize,
    pub open_fact_field_capacity_bytes: usize,
    pub open_fact_value_capacity_bytes: usize,
    pub open_output_heap_bytes: usize,
    pub output_frames: usize,
    /// This storage-only query has no source handle or byte payload.
    pub retained_source_bytes: usize,
    /// The only event vectors decoded are bounded packed leaves.
    pub document_sized_event_vectors: usize,
}

/// Manifest-bound current output required to reconstruct a restart recipe.
#[must_use = "restart output is current-root authority and must be consumed or discarded"]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenRestartOutputAtEventCut {
    manifest: SerializedGreenManifestId,
    event_cut: u64,
    source_metric: SerializedMetric,
    blocks: u64,
    open_depth: u64,
    coverage_count: u64,
    frames: Vec<GreenRestartOutputFrame>,
    receipt: GreenRestartOutputReceipt,
}

impl GreenRestartOutputAtEventCut {
    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.manifest
    }

    #[must_use]
    pub const fn event_cut(&self) -> u64 {
        self.event_cut
    }

    #[must_use]
    pub const fn source_metric(&self) -> SerializedMetric {
        self.source_metric
    }

    #[must_use]
    pub const fn blocks(&self) -> u64 {
        self.blocks
    }

    #[must_use]
    pub const fn open_depth(&self) -> u64 {
        self.open_depth
    }

    #[must_use]
    pub const fn coverage_count(&self) -> u64 {
        self.coverage_count
    }

    #[must_use]
    pub fn frames(&self) -> &[GreenRestartOutputFrame] {
        &self.frames
    }

    #[must_use]
    pub const fn receipt(&self) -> &GreenRestartOutputReceipt {
        &self.receipt
    }

    /// Zero-copy authority handoff for the crate-internal donor/adoption join.
    #[allow(dead_code)] // The donor adapter is deliberately not part of this prototype cut.
    pub(crate) fn into_parts(self) -> GreenRestartOutputParts {
        GreenRestartOutputParts {
            manifest: self.manifest,
            event_cut: self.event_cut,
            source_metric: self.source_metric,
            blocks: self.blocks,
            open_depth: self.open_depth,
            coverage_count: self.coverage_count,
            frames: self.frames,
            receipt: self.receipt,
        }
    }

    /// Convert this current-green query into the one linear path consumed by
    /// both donor reconstruction and the surrounding composite restart join.
    ///
    /// This mapping is intentionally direct-only. In particular, a committed
    /// Setext `Heading` remains a donor `Heading`; green facts alone never
    /// authorize the inverse `Heading`-to-provisional-`Paragraph`
    /// normalization used by a pre-underline checkpoint. That inverse needs a
    /// separate entry point which consumes the parent-bound normalization
    /// role and outcome.
    #[cfg(feature = "exact-parser")]
    pub fn into_current_restart_path(self) -> Result<CurrentRestartPath, CurrentRestartPathError> {
        CurrentRestartPath::try_from_green_output(self)
    }

    /// Map and bind current committed output to a caller-authorized,
    /// stabilized donor line-local continuation.
    ///
    /// The line-local continuation is deliberately caller supplied: this
    /// storage query proves current cumulative output, not the temporal
    /// authority of any donor sample. Binding is performed only through the
    /// donor's grammar-validating stabilized-line API.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn bind_direct_restart_output_from_stabilized_line_mechanism_only(
        self,
        grammar: &DirectGrammarContinuation,
        line_local: DirectRestartLineLocalContinuation,
    ) -> Result<BoundCurrentRestartOutput, CurrentRestartPathError> {
        self.into_current_restart_path()?
            .pair_with_stabilized_line_mechanism_only(line_local)
            .bind_direct_restart_output_mechanism_only(grammar)
    }
}

#[allow(dead_code)] // Reserved for the deliberately deferred donor/adoption join.
pub(crate) struct GreenRestartOutputParts {
    pub(crate) manifest: SerializedGreenManifestId,
    pub(crate) event_cut: u64,
    pub(crate) source_metric: SerializedMetric,
    pub(crate) blocks: u64,
    pub(crate) open_depth: u64,
    pub(crate) coverage_count: u64,
    pub(crate) frames: Vec<GreenRestartOutputFrame>,
    pub(crate) receipt: GreenRestartOutputReceipt,
}

/// The normalization role carried by one mapped current restart frame.
///
/// The generic green-to-donor adapter never produces a value of this type.
/// It exists as an opaque slot on [`CurrentRestartPathFrame`] so a future
/// normalization-specific adapter can attach parent-bound authority without
/// constructing a second, potentially divergent frame path.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrentRestartNormalizationMetadata {
    role: CurrentRestartNormalizationRole,
    checkpoint_cut: crate::committed_checkpoint_index::RelativeCheckpointMeasure,
    block: BlockId,
    final_heading: GreenHeadingOpenFacts,
}

/// A normalization which changes the donor-facing kind at a restart cut.
///
/// This enum is descriptive only. The enclosing metadata has no public
/// constructor, so callers cannot mint normalization authority from the role.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentRestartNormalizationRole {
    SetextHeadingToProvisionalParagraph,
}

#[cfg(feature = "exact-parser")]
impl CurrentRestartNormalizationMetadata {
    #[must_use]
    pub const fn role(self) -> CurrentRestartNormalizationRole {
        self.role
    }
}

/// One-shot green inverse selected by the same committed-parent checkpoint
/// which reconstructed the current donor/source restart.
///
/// This authority intentionally carries no parent stamp of its own. It is
/// minted only when the normalized current path is destroyed at the atomic
/// source/adoption join, then remains enclosed beside the already branded
/// parent lease until the parent-retained green restart consumes both.
#[cfg(feature = "exact-parser")]
#[must_use = "the selected Setext inverse must enter its branded parent green restart"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParentSelectedSetextGreenInverseAuthority {
    path: CurrentRestartPath,
    checkpoint_cut: crate::committed_checkpoint_index::RelativeCheckpointMeasure,
    block: BlockId,
    final_heading: GreenHeadingOpenFacts,
}

/// The exact current-green path selected by the same parent checkpoint which
/// resumed source and donor state. This is the only Direct retained-prefix
/// authority: no constructor accepts copied cuts, bindings, or frame facts.
#[cfg(feature = "exact-parser")]
#[must_use = "the selected direct green path must enter its branded parent restart"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParentSelectedDirectGreenRestartAuthority {
    path: CurrentRestartPath,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedDirectGreenRestartAuthority {
    /// Restricted to the serialized-green module and its descendants. The
    /// retained-prefix core consumes the complete path; sibling crate modules
    /// cannot split its authenticated frames back into caller-supplied scalars.
    pub(super) fn into_current_restart_path(self) -> CurrentRestartPath {
        self.path
    }
}

/// One mutually-exclusive green role emitted when the last routable current
/// path is consumed at the source/adoption join.
#[cfg(feature = "exact-parser")]
#[must_use = "the selected green authority must remain joined to its branded parent lease"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParentSelectedGreenRestartAuthority {
    Direct(ParentSelectedDirectGreenRestartAuthority),
    Setext(ParentSelectedSetextGreenInverseAuthority),
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSetextGreenInverseAuthority {
    /// Parent-retained green is the only consumer. No constructor accepts
    /// these coordinates, and the enclosing activation never exposes them as
    /// independently pairable authority.
    pub(super) fn into_parent_retained_restart_parts(
        self,
    ) -> (
        CurrentRestartPath,
        crate::committed_checkpoint_index::RelativeCheckpointMeasure,
        BlockId,
        GreenHeadingOpenFacts,
    ) {
        (
            self.path,
            self.checkpoint_cut,
            self.block,
            self.final_heading,
        )
    }
}

/// One frame in the single mapped current restart path.
///
/// Stable block/Enter authority and parser-facing green identity travel beside
/// the exact donor output frame. Source-ledger reconstruction can therefore
/// consume this same carrier instead of re-decoding facts through a parallel
/// mapping which could disagree with donor reconstruction.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentRestartPathFrame {
    block: BlockId,
    enter: GreenEnterCapability,
    enter_event_ordinal: u64,
    green_kind: GreenKind,
    closed_children: ChildSequenceAggregate,
    physical_metric: SerializedMetric,
    donor: DirectRestartFrameOutput,
    logical_metric: SerializedMetric,
    normalization: Option<CurrentRestartNormalizationMetadata>,
}

#[cfg(feature = "exact-parser")]
impl CurrentRestartPathFrame {
    #[must_use]
    pub const fn block(&self) -> BlockId {
        self.block
    }

    #[must_use]
    pub const fn enter(&self) -> GreenEnterCapability {
        self.enter
    }

    #[must_use]
    pub const fn enter_event_ordinal(&self) -> u64 {
        self.enter_event_ordinal
    }

    #[must_use]
    pub const fn green_kind(&self) -> GreenKind {
        self.green_kind
    }

    #[must_use]
    pub const fn closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children
    }

    #[must_use]
    pub const fn physical_metric(&self) -> SerializedMetric {
        self.physical_metric
    }

    #[must_use]
    pub const fn donor(&self) -> DirectRestartFrameOutput {
        self.donor
    }

    #[must_use]
    pub const fn logical_metric(&self) -> SerializedMetric {
        self.logical_metric
    }

    #[must_use]
    pub const fn normalization(&self) -> Option<&CurrentRestartNormalizationMetadata> {
        self.normalization.as_ref()
    }
}

/// Auditable bounded work performed while mapping one green open path.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentRestartPathMappingReceipt {
    pub input_frames: usize,
    pub mapped_frames: usize,
    pub typed_fact_decodes: usize,
    pub mapped_path_capacity_bytes: usize,
    pub retained_source_bytes: usize,
    pub document_sized_event_vectors: usize,
}

/// One current-revision linear restart path at an exact green event cut.
#[cfg(feature = "exact-parser")]
#[must_use = "the mapped current restart path is authority and must be consumed or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub struct CurrentRestartPath {
    manifest: SerializedGreenManifestId,
    event_cut: u64,
    source_metric: SerializedMetric,
    blocks: u64,
    open_depth: u64,
    coverage_count: u64,
    frames: Vec<CurrentRestartPathFrame>,
    query_receipt: GreenRestartOutputReceipt,
    mapping_receipt: CurrentRestartPathMappingReceipt,
}

#[cfg(feature = "exact-parser")]
impl CurrentRestartPath {
    fn try_from_green_output(
        output: GreenRestartOutputAtEventCut,
    ) -> Result<Self, CurrentRestartPathError> {
        let parts = output.into_parts();
        let expected_depth = usize::try_from(parts.open_depth)
            .map_err(|_| CurrentRestartPathError::Overflow("current restart open depth"))?;
        if expected_depth != parts.frames.len() {
            return Err(CurrentRestartPathError::InvalidGreenPath(
                "current restart frame count disagrees with open depth",
            ));
        }

        let mut frames = Vec::new();
        frames
            .try_reserve_exact(expected_depth)
            .map_err(|_| CurrentRestartPathError::Allocation)?;
        let mut typed_fact_decodes = 0_usize;
        let mut previous_enter_ordinal = None;
        for frame in parts.frames {
            let frame = frame.into_parts();
            if frame.enter.manifest != parts.manifest
                || frame.enter.block != frame.block
                || frame.enter.kind != frame.kind
                || frame.enter_event_ordinal >= parts.event_cut
                || previous_enter_ordinal
                    .is_some_and(|previous| previous >= frame.enter_event_ordinal)
            {
                return Err(CurrentRestartPathError::InvalidGreenPath(
                    "current restart Enter authority is crossed or unordered",
                ));
            }
            previous_enter_ordinal = Some(frame.enter_event_ordinal);
            let (kind, decoded_facts) =
                direct_kind_from_green_facts(frame.block, frame.kind, &frame.facts)?;
            typed_fact_decodes = typed_fact_decodes.checked_add(decoded_facts).ok_or(
                CurrentRestartPathError::Overflow("current restart typed fact decode receipt"),
            )?;
            frames.push(CurrentRestartPathFrame {
                block: frame.block,
                enter: frame.enter,
                enter_event_ordinal: frame.enter_event_ordinal,
                green_kind: frame.kind,
                closed_children: frame.closed_children,
                physical_metric: frame.physical_metric,
                donor: DirectRestartFrameOutput {
                    kind,
                    closed_children: direct_child_sequence_fold(frame.closed_children),
                },
                logical_metric: frame.logical_metric,
                normalization: None,
            });
        }
        let mapping_receipt = CurrentRestartPathMappingReceipt {
            input_frames: expected_depth,
            mapped_frames: frames.len(),
            typed_fact_decodes,
            mapped_path_capacity_bytes: frames
                .capacity()
                .checked_mul(std::mem::size_of::<CurrentRestartPathFrame>())
                .ok_or(CurrentRestartPathError::Overflow(
                    "current restart mapped path capacity",
                ))?,
            retained_source_bytes: 0,
            document_sized_event_vectors: 0,
        };
        Ok(Self {
            manifest: parts.manifest,
            event_cut: parts.event_cut,
            source_metric: parts.source_metric,
            blocks: parts.blocks,
            open_depth: parts.open_depth,
            coverage_count: parts.coverage_count,
            frames,
            query_receipt: parts.receipt,
            mapping_receipt,
        })
    }

    #[must_use]
    pub const fn manifest(&self) -> SerializedGreenManifestId {
        self.manifest
    }

    #[must_use]
    pub const fn event_cut(&self) -> u64 {
        self.event_cut
    }

    #[must_use]
    pub const fn source_metric(&self) -> SerializedMetric {
        self.source_metric
    }

    #[must_use]
    pub const fn blocks(&self) -> u64 {
        self.blocks
    }

    #[must_use]
    pub const fn open_depth(&self) -> u64 {
        self.open_depth
    }

    #[must_use]
    pub const fn coverage_count(&self) -> u64 {
        self.coverage_count
    }

    #[must_use]
    pub fn frames(&self) -> &[CurrentRestartPathFrame] {
        &self.frames
    }

    #[must_use]
    pub const fn query_receipt(&self) -> &GreenRestartOutputReceipt {
        &self.query_receipt
    }

    #[must_use]
    pub const fn mapping_receipt(&self) -> &CurrentRestartPathMappingReceipt {
        &self.mapping_receipt
    }

    /// Applies the one persisted Setext inverse authorized by the selected
    /// composite-parent checkpoint.
    ///
    /// The committed green remains a finalized `Heading`. Only the exact
    /// deepest donor-facing frame is restored to the provisional `Paragraph`
    /// shape which existed immediately before the deferred underline LF. The
    /// parent token supplies the full five-axis checkpoint cut, writer-owned
    /// block identity, and finalized typed Heading facts; no echoed scalar can
    /// authorize this rewrite.
    pub(crate) fn apply_parent_bound_normalization(
        mut self,
        authority: crate::committed_checkpoint_index::ParentBoundCurrentRestartNormalization,
    ) -> Result<Self, CurrentRestartPathError> {
        let (checkpoint_cut, target, final_facts) = authority.into_current_restart_path_parts();
        let mismatch = CurrentRestartPathError::NormalizationMismatch;

        if checkpoint_cut.green_events() != self.event_cut
            || checkpoint_cut.projection_runs() != self.coverage_count
            || checkpoint_cut.source_bytes()
                != self.source_metric.bytes.checked_add(1).ok_or(mismatch(
                    "accepted byte cut cannot advance through deferred LF",
                ))?
            || checkpoint_cut.source_utf16()
                != self.source_metric.utf16.checked_add(1).ok_or(mismatch(
                    "accepted UTF-16 cut cannot advance through deferred LF",
                ))?
            || final_facts.style() != GreenHeadingStyle::Setext
        {
            return Err(mismatch(
                "parent normalization does not match the current A/P checkpoint cut",
            ));
        }

        let frame = self.frames.last_mut().ok_or(mismatch(
            "parent normalization has no deepest current-green frame",
        ))?;
        let expected_heading = DirectBlockKind::Heading(DirectHeadingFacts {
            level: final_facts.level(),
            setext: true,
        });
        if frame.block != target
            || frame.green_kind != GreenKind::HEADING
            || frame.donor.kind != expected_heading
            || frame.normalization.is_some()
        {
            return Err(mismatch(
                "parent normalization target or finalized Heading facts disagree",
            ));
        }

        frame.donor.kind = DirectBlockKind::Paragraph;
        frame.normalization = Some(CurrentRestartNormalizationMetadata {
            role: CurrentRestartNormalizationRole::SetextHeadingToProvisionalParagraph,
            checkpoint_cut,
            block: target,
            final_heading: final_facts,
        });
        Ok(self)
    }

    /// Destroys the last routable current-green path after parent selection,
    /// donor resume, source reconstruction, and parent-lease branding have
    /// all succeeded. The cumulative composer base and the optional Setext
    /// inverse therefore originate from one path, never two scalar queries.
    pub(crate) fn into_parent_selected_activation_parts(
        self,
    ) -> Result<
        (
            SerializedMetric,
            u64,
            u64,
            ParentSelectedGreenRestartAuthority,
        ),
        CurrentRestartPathError,
    > {
        let mut inverse = None;
        let frame_count = self.frames.len();
        for (index, frame) in self.frames.iter().enumerate() {
            let Some(metadata) = frame.normalization else {
                continue;
            };
            if inverse.is_some()
                || index.checked_add(1) != Some(frame_count)
                || metadata.role
                    != CurrentRestartNormalizationRole::SetextHeadingToProvisionalParagraph
                || metadata.block != frame.block
                || metadata.checkpoint_cut.green_events() != self.event_cut
                || metadata.checkpoint_cut.projection_runs() != self.coverage_count
                || metadata.checkpoint_cut.source_bytes()
                    != self.source_metric.bytes.checked_add(1).ok_or(
                        CurrentRestartPathError::NormalizationMismatch(
                            "accepted byte cut cannot advance through deferred LF",
                        ),
                    )?
                || metadata.checkpoint_cut.source_utf16()
                    != self.source_metric.utf16.checked_add(1).ok_or(
                        CurrentRestartPathError::NormalizationMismatch(
                            "accepted UTF-16 cut cannot advance through deferred LF",
                        ),
                    )?
                || metadata.final_heading.style() != GreenHeadingStyle::Setext
            {
                return Err(CurrentRestartPathError::NormalizationMismatch(
                    "current path carries crossed Setext inverse authority",
                ));
            }
            inverse = Some((
                metadata.checkpoint_cut,
                metadata.block,
                metadata.final_heading,
            ));
        }
        let source_metric = self.source_metric;
        let event_cut = self.event_cut;
        let coverage_count = self.coverage_count;
        let green = match inverse {
            Some((checkpoint_cut, block, final_heading)) => {
                ParentSelectedGreenRestartAuthority::Setext(
                    ParentSelectedSetextGreenInverseAuthority {
                        path: self,
                        checkpoint_cut,
                        block,
                        final_heading,
                    },
                )
            }
            None => ParentSelectedGreenRestartAuthority::Direct(
                ParentSelectedDirectGreenRestartAuthority { path: self },
            ),
        };
        Ok((source_metric, event_cut, coverage_count, green))
    }

    /// Pair this current cumulative output path with the opaque line-local
    /// continuation authorized by the enclosing convergence induction.
    ///
    /// Green frames do not contain intrinsic per-line blankness. Keeping this
    /// token on the same carrier prevents a later adoption join from silently
    /// treating green cumulative output as complete donor state.
    #[must_use]
    pub(crate) fn pair_with_stabilized_line_mechanism_only(
        self,
        line_local: DirectRestartLineLocalContinuation,
    ) -> CurrentRestartPathWithStabilizedLine {
        CurrentRestartPathWithStabilizedLine {
            path: self,
            line_local,
        }
    }

    /// Bind through the donor's sole stabilized-line reconstruction seam.
    pub(crate) fn bind_direct_restart_output_from_stabilized_line_mechanism_only(
        self,
        grammar: &DirectGrammarContinuation,
        line_local: DirectRestartLineLocalContinuation,
    ) -> Result<BoundCurrentRestartOutput, CurrentRestartPathError> {
        self.pair_with_stabilized_line_mechanism_only(line_local)
            .bind_direct_restart_output_mechanism_only(grammar)
    }
}

/// The current mapped output path paired with a convergence-authorized opaque
/// donor line-local token.
///
/// This is the complete pre-bind carrier: current green supplies cumulative
/// display/fold facts, while the paired token supplies intrinsic blankness and
/// deferred line state. Neither half is treated as a substitute for the other.
#[cfg(feature = "exact-parser")]
#[must_use = "the stabilized current restart path must be bound or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CurrentRestartPathWithStabilizedLine {
    path: CurrentRestartPath,
    line_local: DirectRestartLineLocalContinuation,
}

#[cfg(feature = "exact-parser")]
impl CurrentRestartPathWithStabilizedLine {
    #[must_use]
    pub(crate) const fn path(&self) -> &CurrentRestartPath {
        &self.path
    }

    /// Bind only through the donor's grammar-validating stabilized-line API.
    pub(crate) fn bind_direct_restart_output_mechanism_only(
        self,
        grammar: &DirectGrammarContinuation,
    ) -> Result<BoundCurrentRestartOutput, CurrentRestartPathError> {
        let output = grammar
            .bind_current_restart_output_from_stabilized_line(
                self.line_local,
                self.path.frames.iter().map(CurrentRestartPathFrame::donor),
            )
            .map_err(CurrentRestartPathError::Donor)?;
        Ok(BoundCurrentRestartOutput {
            path: self.path,
            output,
        })
    }
}

/// Donor restart output bound to the exact mapped current-green path that
/// supplied every cumulative frame fact.
#[cfg(feature = "exact-parser")]
#[must_use = "bound current restart output must be resumed or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BoundCurrentRestartOutput {
    path: CurrentRestartPath,
    output: DirectRestartOutput,
}

#[cfg(feature = "exact-parser")]
impl BoundCurrentRestartOutput {
    #[must_use]
    pub(crate) const fn path(&self) -> &CurrentRestartPath {
        &self.path
    }

    #[must_use]
    pub(crate) const fn donor_output(&self) -> &DirectRestartOutput {
        &self.output
    }

    #[must_use]
    pub(crate) fn into_parts(self) -> (CurrentRestartPath, DirectRestartOutput) {
        (self.path, self.output)
    }
}

/// Fail-closed error from current-green direct restart mapping or donor bind.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CurrentRestartPathError {
    UnsupportedKind { block: BlockId, kind: GreenKind },
    NonCanonicalFacts { block: BlockId, kind: GreenKind },
    InvalidGreenPath(&'static str),
    NormalizationMismatch(&'static str),
    Overflow(&'static str),
    Allocation,
    Donor(ParseError),
}

#[cfg(feature = "exact-parser")]
impl fmt::Display for CurrentRestartPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedKind { block, kind } => write!(
                formatter,
                "green block {} kind {} is unsupported by the direct restart donor",
                block.0, kind.0,
            ),
            Self::NonCanonicalFacts { block, kind } => write!(
                formatter,
                "green block {} kind {} has noncanonical direct restart facts",
                block.0, kind.0,
            ),
            Self::InvalidGreenPath(message) => {
                write!(formatter, "invalid current green restart path: {message}")
            }
            Self::NormalizationMismatch(message) => {
                write!(
                    formatter,
                    "current restart normalization mismatch: {message}"
                )
            }
            Self::Overflow(field) => write!(formatter, "current restart {field} overflow"),
            Self::Allocation => formatter.write_str("current restart path allocation failed"),
            Self::Donor(error) => write!(formatter, "current restart donor bind failed: {error:?}"),
        }
    }
}

#[cfg(feature = "exact-parser")]
impl std::error::Error for CurrentRestartPathError {}

#[cfg(feature = "exact-parser")]
const fn direct_child_sequence_fold(fold: ChildSequenceAggregate) -> ChildSequenceFold {
    ChildSequenceFold {
        had_child: fold.had_child,
        any_nonlast_child_ends_blank: fold.any_nonlast_child_ends_blank,
        last_child_ends_blank: fold.last_child_ends_blank,
        list_loose_before_last: fold.list_loose_before_last,
        last_item_loose_if_nonlast: fold.last_item_loose_if_nonlast,
        last_item_loose_if_last: fold.last_item_loose_if_last,
    }
}

#[cfg(feature = "exact-parser")]
fn direct_kind_from_green_facts(
    block: BlockId,
    kind: GreenKind,
    facts: &FactsEnvelope,
) -> Result<(DirectBlockKind, usize), CurrentRestartPathError> {
    let noncanonical = || CurrentRestartPathError::NonCanonicalFacts { block, kind };
    let empty = || {
        if *facts == FactsEnvelope::empty() {
            Ok(0)
        } else {
            Err(noncanonical())
        }
    };
    match kind {
        GreenKind::DOCUMENT => empty().map(|decodes| (DirectBlockKind::Document, decodes)),
        GreenKind::BLOCK_QUOTE => empty().map(|decodes| (DirectBlockKind::BlockQuote, decodes)),
        GreenKind::PARAGRAPH => empty().map(|decodes| (DirectBlockKind::Paragraph, decodes)),
        GreenKind::LIST => {
            let decoded =
                GreenListOpenFacts::try_from_envelope(facts).map_err(|_| noncanonical())?;
            if decoded.into_envelope() != *facts {
                return Err(noncanonical());
            }
            let direct = match decoded.style() {
                GreenListStyle::Bullet { marker } => DirectListFacts {
                    list_type: DirectListType::Bullet,
                    start: 1,
                    delimiter: DirectListDelimiter::Period,
                    bullet_char: marker.marker(),
                },
                GreenListStyle::Ordered { start, delimiter } => DirectListFacts {
                    list_type: DirectListType::Ordered,
                    start,
                    delimiter: match delimiter {
                        GreenListDelimiter::Period => DirectListDelimiter::Period,
                        GreenListDelimiter::Parenthesis => DirectListDelimiter::Paren,
                    },
                    bullet_char: 0,
                },
            };
            Ok((DirectBlockKind::List(direct), 1))
        }
        GreenKind::ITEM => {
            let decoded =
                GreenItemOpenFacts::try_from_envelope(facts).map_err(|_| noncanonical())?;
            if decoded.into_envelope() != *facts {
                return Err(noncanonical());
            }
            Ok((
                DirectBlockKind::Item(DirectItemFacts {
                    marker_offset: decoded.marker_offset_columns(),
                    padding: decoded.padding_columns(),
                }),
                1,
            ))
        }
        GreenKind::HEADING => {
            let decoded =
                GreenHeadingOpenFacts::try_from_envelope(facts).map_err(|_| noncanonical())?;
            if decoded.into_envelope() != *facts {
                return Err(noncanonical());
            }
            Ok((
                DirectBlockKind::Heading(DirectHeadingFacts {
                    level: decoded.level(),
                    setext: decoded.style() == GreenHeadingStyle::Setext,
                }),
                1,
            ))
        }
        GreenKind::FENCED_CODE => {
            let decoded =
                GreenFencedCodeOpenFacts::try_from_envelope(facts).map_err(|_| noncanonical())?;
            if decoded.into_envelope() != *facts {
                return Err(noncanonical());
            }
            Ok((
                DirectBlockKind::FencedCode(DirectFencedCodeFacts {
                    fence: match decoded.fence() {
                        GreenFenceCharacter::Backtick => DirectFenceCharacter::Backtick,
                        GreenFenceCharacter::Tilde => DirectFenceCharacter::Tilde,
                    },
                    minimum_closing_length: decoded.minimum_closing_length(),
                    fence_offset_columns: decoded.fence_offset_columns(),
                }),
                1,
            ))
        }
        _ => Err(CurrentRestartPathError::UnsupportedKind { block, kind }),
    }
}

#[derive(Clone, Copy)]
pub(super) struct QueryNode {
    pub(super) id: ArenaId,
    pub(super) summary: GreenSummary,
    pub(super) kind: SequenceNodeKind,
}

pub(super) fn load_query_node(
    arena: &PageArena,
    id: ArenaId,
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<QueryNode, SerializedGreenError> {
    receipt.sequence_nodes_visited =
        receipt
            .sequence_nodes_visited
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "restart-output sequence-node receipt",
            ))?;
    let (summary, kind) = sequence_node::<SerializedGreenSpec>(arena, id)?;
    Ok(QueryNode { id, summary, kind })
}

fn decode_query_leaf(
    arena: &PageArena,
    leaf: ArenaId,
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<Vec<DecodedLeafEvent>, SerializedGreenError> {
    let payload_bytes = arena.payload(leaf)?.len();
    let (_, events) = decode_leaf(arena, leaf)?;
    receipt.leaf_pages_decoded =
        receipt
            .leaf_pages_decoded
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "restart-output decoded leaf receipt",
            ))?;
    receipt.events_decoded =
        receipt
            .events_decoded
            .checked_add(events.len())
            .ok_or(SerializedGreenError::Overflow(
                "restart-output decoded event receipt",
            ))?;
    let event_capacity_bytes = events
        .capacity()
        .checked_mul(std::mem::size_of::<DecodedLeafEvent>())
        .ok_or(SerializedGreenError::Overflow(
            "restart-output decoded-event heap receipt",
        ))?;
    let mut nested_fact_bytes = 0_usize;
    for decoded in &events {
        if let DecodedGreenEventKind::Enter { facts, .. } = &decoded.event {
            nested_fact_bytes = nested_fact_bytes
                .checked_add(
                    facts
                        .fields
                        .capacity()
                        .checked_mul(std::mem::size_of::<FactField>())
                        .ok_or(SerializedGreenError::Overflow(
                            "restart-output decoded fact-field heap receipt",
                        ))?,
                )
                .ok_or(SerializedGreenError::Overflow(
                    "restart-output decoded fact-field heap receipt",
                ))?;
            for field in &facts.fields {
                nested_fact_bytes = nested_fact_bytes
                    .checked_add(field.value.capacity())
                    .ok_or(SerializedGreenError::Overflow(
                        "restart-output decoded fact-value heap receipt",
                    ))?;
            }
        }
    }
    let decoded_page_bytes = payload_bytes
        .checked_add(event_capacity_bytes)
        .and_then(|bytes| bytes.checked_add(nested_fact_bytes))
        .ok_or(SerializedGreenError::Overflow(
            "restart-output decoded-page receipt",
        ))?;
    receipt.maximum_decoded_page_bytes = receipt.maximum_decoded_page_bytes.max(decoded_page_bytes);
    Ok(events)
}

struct ReversePrefixScan<'arena, 'receipt> {
    arena: &'arena PageArena,
    manifest: SerializedGreenManifestId,
    unmatched_exits: u64,
    output: Vec<GreenRestartOutputFrame>,
    receipt: &'receipt mut GreenRestartOutputReceipt,
}

impl ReversePrefixScan<'_, '_> {
    fn scan_leaf(
        &mut self,
        node: QueryNode,
        base_event_ordinal: u64,
        base_leaf_index: u64,
        event_cut: u64,
    ) -> Result<(), SerializedGreenError> {
        let events = decode_query_leaf(self.arena, node.id, self.receipt)?;
        let local_end = usize::try_from(event_cut - base_event_ordinal)
            .map_err(|_| SerializedGreenError::Overflow("restart-output leaf event cut"))?;
        if local_end > events.len()
            || u64::try_from(events.len())
                .map_err(|_| SerializedGreenError::Overflow("restart-output leaf event count"))?
                != node.summary.tokens
        {
            return Err(SerializedGreenError::Corrupt(
                "restart-output leaf token count changed",
            ));
        }
        for (local_ordinal, decoded) in events[..local_end].iter().enumerate().rev() {
            match &decoded.event {
                DecodedGreenEventKind::Exit { .. } => {
                    self.unmatched_exits = self.unmatched_exits.checked_add(1).ok_or(
                        SerializedGreenError::Overflow("restart-output reverse Exit count"),
                    )?;
                }
                DecodedGreenEventKind::Enter { block, kind, facts } => {
                    if self.unmatched_exits != 0 {
                        self.unmatched_exits -= 1;
                    } else {
                        let local_ordinal = u64::try_from(local_ordinal).map_err(|_| {
                            SerializedGreenError::Overflow("restart-output Enter ordinal")
                        })?;
                        let enter_event_ordinal = base_event_ordinal
                            .checked_add(local_ordinal)
                            .ok_or(SerializedGreenError::Overflow(
                                "restart-output Enter event ordinal",
                            ))?;
                        self.output.push(GreenRestartOutputFrame {
                            block: *block,
                            kind: *kind,
                            facts: facts.clone(),
                            enter: GreenEnterCapability {
                                manifest: self.manifest,
                                leaf: node.id,
                                base_leaf_index,
                                byte_offset: decoded.byte_offset,
                                block: *block,
                                kind: *kind,
                            },
                            enter_event_ordinal,
                            closed_children: ChildSequenceAggregate::default(),
                            physical_metric: SerializedMetric::default(),
                            logical_metric: SerializedMetric::default(),
                        });
                    }
                }
                DecodedGreenEventKind::Coverage(_) => {}
            }
        }
        self.receipt.maximum_open_depth = self.receipt.maximum_open_depth.max(self.output.len());
        Ok(())
    }

    fn scan(
        &mut self,
        node: QueryNode,
        base_event_ordinal: u64,
        base_leaf_index: u64,
        event_cut: u64,
        route_depth: usize,
    ) -> Result<(), SerializedGreenError> {
        self.receipt.maximum_route_depth = self.receipt.maximum_route_depth.max(route_depth);
        let node_end = base_event_ordinal.checked_add(node.summary.tokens).ok_or(
            SerializedGreenError::Overflow("restart-output node event end"),
        )?;
        if event_cut < base_event_ordinal || event_cut > node_end {
            return Err(SerializedGreenError::Corrupt(
                "restart-output prefix escapes a sequence node",
            ));
        }
        if event_cut == base_event_ordinal {
            return Ok(());
        }
        if event_cut == node_end {
            let (opens, closes) = node.summary.unmatched()?;
            if opens <= self.unmatched_exits {
                self.unmatched_exits = self
                    .unmatched_exits
                    .checked_sub(opens)
                    .and_then(|remaining| remaining.checked_add(closes))
                    .ok_or(SerializedGreenError::Overflow(
                        "restart-output reverse structural count",
                    ))?;
                self.receipt.summary_nodes_reused =
                    self.receipt.summary_nodes_reused.checked_add(1).ok_or(
                        SerializedGreenError::Overflow("restart-output reused-summary receipt"),
                    )?;
                return Ok(());
            }
        }

        match node.kind {
            SequenceNodeKind::Leaf => {
                self.scan_leaf(node, base_event_ordinal, base_leaf_index, event_cut)
            }
            SequenceNodeKind::Branch { left, right } => {
                let next_depth =
                    route_depth
                        .checked_add(1)
                        .ok_or(SerializedGreenError::Overflow(
                            "restart-output reverse route depth",
                        ))?;
                let left = load_query_node(self.arena, left, self.receipt)?;
                let left_end = base_event_ordinal.checked_add(left.summary.tokens).ok_or(
                    SerializedGreenError::Overflow("restart-output left event end"),
                )?;
                if event_cut > left_end {
                    let right_base_leaf = base_leaf_index.checked_add(left.summary.leaves).ok_or(
                        SerializedGreenError::Overflow("restart-output right leaf index"),
                    )?;
                    let right = load_query_node(self.arena, right, self.receipt)?;
                    self.scan(right, left_end, right_base_leaf, event_cut, next_depth)?;
                    self.scan(
                        left,
                        base_event_ordinal,
                        base_leaf_index,
                        left_end,
                        next_depth,
                    )
                } else {
                    self.scan(
                        left,
                        base_event_ordinal,
                        base_leaf_index,
                        event_cut,
                        next_depth,
                    )
                }
            }
        }
    }
}

fn fold_event_range(
    arena: &PageArena,
    node: QueryNode,
    base_event_ordinal: u64,
    range_start: u64,
    range_end: u64,
    route_depth: usize,
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<GreenSummary, SerializedGreenError> {
    receipt.maximum_route_depth = receipt.maximum_route_depth.max(route_depth);
    let node_end = base_event_ordinal.checked_add(node.summary.tokens).ok_or(
        SerializedGreenError::Overflow("restart-output range node end"),
    )?;
    if range_start < base_event_ordinal || range_start >= range_end || range_end > node_end {
        return Err(SerializedGreenError::Corrupt(
            "restart-output event range escapes a sequence node",
        ));
    }
    if range_start == base_event_ordinal && range_end == node_end {
        receipt.summary_nodes_reused =
            receipt
                .summary_nodes_reused
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "restart-output reused-summary receipt",
                ))?;
        return Ok(node.summary);
    }

    match node.kind {
        SequenceNodeKind::Leaf => {
            let events = decode_query_leaf(arena, node.id, receipt)?;
            if u64::try_from(events.len())
                .map_err(|_| SerializedGreenError::Overflow("restart-output leaf event count"))?
                != node.summary.tokens
            {
                return Err(SerializedGreenError::Corrupt(
                    "restart-output leaf token count changed",
                ));
            }
            let local_start = usize::try_from(range_start - base_event_ordinal)
                .map_err(|_| SerializedGreenError::Overflow("restart-output range start"))?;
            let local_end = usize::try_from(range_end - base_event_ordinal)
                .map_err(|_| SerializedGreenError::Overflow("restart-output range end"))?;
            events[local_start..local_end]
                .iter()
                .try_fold(GreenSummary::default(), |summary, event| {
                    summary.followed_by(GreenSummary::decoded_event(&event.event))
                })
        }
        SequenceNodeKind::Branch { left, right } => {
            let next_depth = route_depth
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "restart-output range route depth",
                ))?;
            let left = load_query_node(arena, left, receipt)?;
            let left_end = base_event_ordinal.checked_add(left.summary.tokens).ok_or(
                SerializedGreenError::Overflow("restart-output range left end"),
            )?;
            if range_end <= left_end {
                return fold_event_range(
                    arena,
                    left,
                    base_event_ordinal,
                    range_start,
                    range_end,
                    next_depth,
                    receipt,
                );
            }
            let right = load_query_node(arena, right, receipt)?;
            if range_start >= left_end {
                return fold_event_range(
                    arena,
                    right,
                    left_end,
                    range_start,
                    range_end,
                    next_depth,
                    receipt,
                );
            }
            let left_summary = fold_event_range(
                arena,
                left,
                base_event_ordinal,
                range_start,
                left_end,
                next_depth,
                receipt,
            )?;
            let right_summary = fold_event_range(
                arena, right, left_end, left_end, range_end, next_depth, receipt,
            )?;
            left_summary.followed_by(right_summary)
        }
    }
}

pub(super) fn event_range_summary(
    arena: &PageArena,
    root: QueryNode,
    range_start: u64,
    range_end: u64,
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<GreenSummary, SerializedGreenError> {
    receipt.range_queries =
        receipt
            .range_queries
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "restart-output range-query receipt",
            ))?;
    if range_start > range_end || range_end > root.summary.tokens {
        return Err(SerializedGreenError::StaleCursor);
    }
    if range_start == range_end {
        return Ok(GreenSummary::default());
    }
    fold_event_range(arena, root, 0, range_start, range_end, 0, receipt)
}

#[derive(Clone, Copy)]
struct RestartPrefixScalars {
    source_metric: SerializedMetric,
    blocks: u64,
    open_depth: u64,
    coverage_count: u64,
}

fn validate_restart_prefix(
    prefix: GreenSummary,
    event_cut: u64,
) -> Result<RestartPrefixScalars, SerializedGreenError> {
    if prefix.tokens != event_cut || prefix.minimum_prefix < 0 || prefix.balance < 0 {
        return Err(SerializedGreenError::Corrupt(
            "restart-output event cut is not a valid structural prefix",
        ));
    }
    let open_depth = u64::try_from(prefix.balance)
        .map_err(|_| SerializedGreenError::Corrupt("restart-output open depth is negative"))?;
    let exits = prefix
        .blocks
        .checked_sub(open_depth)
        .ok_or(SerializedGreenError::Corrupt(
            "restart-output open depth exceeds Enter count",
        ))?;
    let structural_tokens =
        prefix
            .blocks
            .checked_add(exits)
            .ok_or(SerializedGreenError::Overflow(
                "restart-output structural token count",
            ))?;
    let coverage_count =
        prefix
            .tokens
            .checked_sub(structural_tokens)
            .ok_or(SerializedGreenError::Corrupt(
                "restart-output structural count exceeds event count",
            ))?;
    Ok(RestartPrefixScalars {
        source_metric: prefix.metric,
        blocks: prefix.blocks,
        open_depth,
        coverage_count,
    })
}

fn recover_open_frames(
    arena: &PageArena,
    manifest: SerializedGreenManifestId,
    root: QueryNode,
    event_cut: u64,
    expected_open_depth: u64,
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<Vec<GreenRestartOutputFrame>, SerializedGreenError> {
    let mut reverse_scan = ReversePrefixScan {
        arena,
        manifest,
        unmatched_exits: 0,
        output: Vec::new(),
        receipt,
    };
    reverse_scan.scan(root, 0, 0, event_cut, 0)?;
    if reverse_scan.unmatched_exits != 0 {
        return Err(SerializedGreenError::Corrupt(
            "restart-output prefix retains an unmatched Exit",
        ));
    }
    let mut frames = reverse_scan.output;
    frames.reverse();
    let expected_open_depth = usize::try_from(expected_open_depth)
        .map_err(|_| SerializedGreenError::Overflow("restart-output open depth"))?;
    if frames.len() != expected_open_depth {
        return Err(SerializedGreenError::Corrupt(
            "restart-output reverse path disagrees with prefix balance",
        ));
    }
    Ok(frames)
}

fn open_terminal_index(
    frames: &[GreenRestartOutputFrame],
) -> Result<Option<usize>, SerializedGreenError> {
    let mut terminal = None;
    for (index, frame) in frames.iter().enumerate() {
        if frame.kind.logical_channel().is_some() {
            if terminal.is_some() || index + 1 != frames.len() {
                return Err(SerializedGreenError::Corrupt(
                    "restart-output terminal is nested or is not the deepest open frame",
                ));
            }
            terminal = Some(index);
        }
    }
    Ok(terminal)
}

fn populate_closed_child_output(
    arena: &PageArena,
    manifest: SerializedGreenManifestId,
    root: QueryNode,
    event_cut: u64,
    frames: &mut [GreenRestartOutputFrame],
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<(), SerializedGreenError> {
    let frame_count = frames.len();
    let terminal_index = open_terminal_index(frames)?;
    for index in 0..frame_count {
        let enter_event_ordinal = frames[index].enter_event_ordinal;
        let range_start =
            enter_event_ordinal
                .checked_add(1)
                .ok_or(SerializedGreenError::Overflow(
                    "restart-output frame range start",
                ))?;
        if range_start > event_cut
            || frames[index].enter.manifest != manifest
            || (index != 0 && frames[index - 1].enter_event_ordinal >= enter_event_ordinal)
        {
            return Err(SerializedGreenError::Corrupt(
                "restart-output open path has invalid Enter authority",
            ));
        }
        validate_facts_for_kind(frames[index].kind, &frames[index].facts).map_err(|_| {
            SerializedGreenError::Corrupt(
                "restart-output Enter facts changed after storage validation",
            )
        })?;
        let summary = event_range_summary(arena, root, range_start, event_cut, receipt)?;
        let expected_descendants = i64::try_from(frame_count - index - 1)
            .map_err(|_| SerializedGreenError::Overflow("restart-output descendant depth"))?;
        if summary.tokens != event_cut - range_start
            || summary.minimum_prefix < 0
            || summary.balance != expected_descendants
        {
            return Err(SerializedGreenError::Corrupt(
                "restart-output frame range has invalid structural shape",
            ));
        }
        frames[index].closed_children = match summary.minimum_closed_depth {
            None => ChildSequenceAggregate::default(),
            Some(0) => summary.outermost,
            Some(depth) if depth > 0 => ChildSequenceAggregate::default(),
            Some(_) => {
                return Err(SerializedGreenError::Corrupt(
                    "restart-output frame range closes its owning frame",
                ));
            }
        };
        frames[index].physical_metric = summary.metric;
        frames[index].logical_metric = if terminal_index == Some(index) {
            if summary.blocks != 0
                || summary.balance != 0
                || summary.minimum_prefix != 0
                || summary.minimum_closed_depth.is_some()
            {
                return Err(SerializedGreenError::Corrupt(
                    "restart-output terminal range contains structural events",
                ));
            }
            summary.logical_metric
        } else {
            SerializedMetric::default()
        };
    }
    Ok(())
}

fn record_open_output_heap(
    frames: &[GreenRestartOutputFrame],
    frame_capacity: usize,
    receipt: &mut GreenRestartOutputReceipt,
) -> Result<(), SerializedGreenError> {
    receipt.maximum_open_depth = receipt.maximum_open_depth.max(frames.len());
    receipt.open_frame_capacity_bytes = frame_capacity
        .checked_mul(std::mem::size_of::<GreenRestartOutputFrame>())
        .ok_or(SerializedGreenError::Overflow(
            "restart-output frame heap receipt",
        ))?;
    for frame in frames {
        receipt.open_fact_field_capacity_bytes = receipt
            .open_fact_field_capacity_bytes
            .checked_add(
                frame
                    .facts
                    .fields
                    .capacity()
                    .checked_mul(std::mem::size_of::<FactField>())
                    .ok_or(SerializedGreenError::Overflow(
                        "restart-output fact-field heap receipt",
                    ))?,
            )
            .ok_or(SerializedGreenError::Overflow(
                "restart-output fact-field heap receipt",
            ))?;
        for field in &frame.facts.fields {
            receipt.open_fact_value_capacity_bytes = receipt
                .open_fact_value_capacity_bytes
                .checked_add(field.value.capacity())
                .ok_or(SerializedGreenError::Overflow(
                    "restart-output fact-value heap receipt",
                ))?;
        }
    }
    receipt.open_output_heap_bytes = receipt
        .open_frame_capacity_bytes
        .checked_add(receipt.open_fact_field_capacity_bytes)
        .and_then(|bytes| bytes.checked_add(receipt.open_fact_value_capacity_bytes))
        .ok_or(SerializedGreenError::Overflow(
            "restart-output combined heap receipt",
        ))?;
    receipt.output_frames = frames.len();
    Ok(())
}

/// Queries the immutable green child selected by a freshly revalidated
/// restart-composite parent.
///
/// The parent mint carries no independently usable child handle. This seam
/// repeats complete child validation, checks both hidden local IDs against
/// the parent-derived descriptor, and only then mints the scoped manifest
/// capability used by the bounded event-range query.
#[cfg(feature = "exact-parser")]
pub(crate) fn restart_output_at_parent_bound_event_cut(
    mint: crate::storage_only_composite_document::RestartGreenQueryMint<'_>,
    event_cut: u64,
) -> Result<GreenRestartOutputAtEventCut, SerializedGreenError> {
    let (arena, expected) = mint.into_query_parts();
    let validated = validate_serialized_green_composite_child(arena, expected.manifest)?;
    if validated != expected {
        return Err(SerializedGreenError::Corrupt(
            "parent-bound restart-output green descriptor changed",
        ));
    }
    let (manifest, sequence_root) = decode_document(arena, expected.manifest)?;
    if sequence_root != expected.sequence_root {
        return Err(SerializedGreenError::Corrupt(
            "parent-bound restart-output sequence root changed",
        ));
    }
    let manifest_capability =
        SerializedGreenManifestId::new(arena.scoped_query_id(expected.manifest)?);
    restart_output_at_bound_root(
        arena,
        manifest_capability,
        &manifest,
        sequence_root,
        event_cut,
    )
}

impl SerializedGreenDocument {
    /// Reconstructs current cumulative block output at one exact event-token
    /// cut without source text or a document-sized event mirror.
    pub fn restart_output_at_event_cut(
        &self,
        arena: &PageArena,
        event_cut: u64,
    ) -> Result<GreenRestartOutputAtEventCut, SerializedGreenError> {
        let manifest_id = self.local_manifest_id(arena)?;
        let manifest_capability = self.manifest_id();
        let (manifest, root_id) = decode_document(arena, manifest_id)?;
        restart_output_at_bound_root(arena, manifest_capability, &manifest, root_id, event_cut)
    }
}

/// Sibling-private query used when the immutable green child is retained only
/// by an authenticated composite-parent lease. Neither manifest nor sequence
/// root is returned to that lease's caller.
pub(super) fn restart_output_at_bound_root(
    arena: &PageArena,
    manifest_capability: SerializedGreenManifestId,
    manifest: &Manifest,
    root_id: ArenaId,
    event_cut: u64,
) -> Result<GreenRestartOutputAtEventCut, SerializedGreenError> {
    let mut receipt = GreenRestartOutputReceipt::default();
    let root = load_query_node(arena, root_id, &mut receipt)?;
    if root.summary != manifest.summary {
        return Err(SerializedGreenError::Corrupt(
            "restart-output root and manifest summaries disagree",
        ));
    }
    if event_cut > root.summary.tokens {
        return Err(SerializedGreenError::StaleCursor);
    }

    let prefix = event_range_summary(arena, root, 0, event_cut, &mut receipt)?;
    let prefix_scalars = validate_restart_prefix(prefix, event_cut)?;
    let mut frames = recover_open_frames(
        arena,
        manifest_capability,
        root,
        event_cut,
        prefix_scalars.open_depth,
        &mut receipt,
    )?;
    populate_closed_child_output(
        arena,
        manifest_capability,
        root,
        event_cut,
        &mut frames,
        &mut receipt,
    )?;
    let frame_capacity = frames.capacity();
    record_open_output_heap(&frames, frame_capacity, &mut receipt)?;
    debug_assert_eq!(receipt.retained_source_bytes, 0);
    debug_assert_eq!(receipt.document_sized_event_vectors, 0);
    Ok(GreenRestartOutputAtEventCut {
        manifest: manifest_capability,
        event_cut,
        source_metric: prefix_scalars.source_metric,
        blocks: prefix_scalars.blocks,
        open_depth: prefix_scalars.open_depth,
        coverage_count: prefix_scalars.coverage_count,
        frames,
        receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "exact-parser")]
    use flark_comrak_value_block_core::{DirectPollStatus, DirectValueBlockParser, SyntaxProfile};

    #[derive(Clone, Copy)]
    struct FixtureFrame {
        block: BlockId,
        kind: GreenKind,
        children: ChildSequenceAggregate,
        logical_metric: SerializedMetric,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct NaivePrefixScalars {
        source_metric: SerializedMetric,
        blocks: u64,
        open_depth: u64,
        coverage_count: u64,
    }

    type NaiveSnapshots = (
        Vec<Vec<GreenRestartOutputFrame>>,
        Vec<NaivePrefixScalars>,
        Vec<u64>,
    );

    struct FixtureBuilder {
        events: Vec<GreenEvent>,
        open: Vec<FixtureFrame>,
        next_block: u64,
        next_coverage: u64,
    }

    impl FixtureBuilder {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                open: Vec::new(),
                next_block: 1,
                next_coverage: 1,
            }
        }

        fn enter(&mut self, kind: GreenKind, facts: FactsEnvelope) -> BlockId {
            let block = BlockId(self.next_block);
            self.next_block += 1;
            self.events.push(GreenEvent::enter(block, kind, facts));
            self.open.push(FixtureFrame {
                block,
                kind,
                children: ChildSequenceAggregate::default(),
                logical_metric: SerializedMetric::default(),
            });
            block
        }

        fn identity_coverage(&mut self, target: BlockId, bytes: u64, utf16: u64) {
            self.coverage(
                target,
                SerializedMetric { bytes, utf16 },
                Some(LogicalContribution::Identity),
            );
        }

        fn coverage(
            &mut self,
            target: BlockId,
            physical_metric: SerializedMetric,
            contribution: Option<LogicalContribution>,
        ) {
            let coverage = CoverageId(self.next_coverage);
            self.next_coverage += 1;
            let logical_metric = match contribution.as_ref() {
                None | Some(LogicalContribution::Hidden { .. }) => SerializedMetric::default(),
                Some(LogicalContribution::Identity) => physical_metric,
                Some(LogicalContribution::Atomic(projection)) => projection.logical_metric,
                Some(LogicalContribution::Program(program)) => program.logical_metric(),
                Some(LogicalContribution::None) => panic!("fixture uses None as no contribution"),
            };
            if !logical_metric.is_zero() {
                let terminal = self
                    .open
                    .iter_mut()
                    .find(|frame| frame.block == target)
                    .expect("logical fixture target is open");
                assert!(terminal.kind.logical_channel().is_some());
                terminal.logical_metric =
                    terminal.logical_metric.checked_add(logical_metric).unwrap();
            }
            let run = if let Some(contribution) = contribution {
                SourceProjectionRun::with_logical(
                    coverage,
                    physical_metric.bytes,
                    physical_metric.utf16,
                    0,
                    CoveragePart::CONTENT,
                    target,
                    contribution,
                )
                .unwrap()
            } else {
                SourceProjectionRun::new(
                    coverage,
                    physical_metric.bytes,
                    physical_metric.utf16,
                    0,
                    CoveragePart::CONTENT,
                )
                .unwrap()
            };
            self.events.push(GreenEvent::Coverage(run));
        }

        fn close(&mut self, last_line_blank: bool) -> ClosedChildAggregate {
            let frame = self.open.pop().expect("fixture close has an open frame");
            let semantics = ContainerFoldSemantics {
                descends_through_last_child: matches!(
                    frame.kind,
                    GreenKind::LIST | GreenKind::ITEM
                ),
                is_item: frame.kind == GreenKind::ITEM,
                last_line_blank,
            };
            let closed = semantics.closed_summary(frame.children);
            let facts = match frame.kind {
                GreenKind::LIST => GreenCloseFacts::List {
                    tight: frame.children.list_is_tight(),
                },
                GreenKind::FENCED_CODE => {
                    let empty = GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap();
                    let literal = GreenRelativeLogicalSlice::new(
                        0..frame.logical_metric.bytes,
                        0..frame.logical_metric.utf16,
                    )
                    .unwrap();
                    GreenCloseFacts::FencedCode(
                        GreenFencedCodeCloseFacts::new(false, empty, literal).unwrap(),
                    )
                }
                _ => GreenCloseFacts::None,
            };
            self.events
                .push(GreenEvent::exit_with_state(closed, last_line_blank, facts));
            if let Some(parent) = self.open.last_mut() {
                parent.children = parent
                    .children
                    .followed_by(ChildSequenceAggregate::singleton(closed));
            }
            closed
        }

        fn finish(mut self) -> Vec<GreenEvent> {
            while !self.open.is_empty() {
                self.close(false);
            }
            self.events
        }
    }

    fn root_spec(metric: SerializedMetric) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(1),
            source_root: SourceRootId(1),
            source_bytes: metric.bytes,
            source_utf16: metric.utf16,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(1),
            semantic_epoch: 1,
            known_bytes: 0..metric.bytes,
        }
    }

    fn build_document(events: Vec<GreenEvent>) -> (PageArena, SerializedGreenDocument) {
        let summary = events
            .iter()
            .try_fold(GreenSummary::default(), |summary, event| {
                summary.followed_by(GreenSummary::event(event))
            })
            .unwrap();
        let mut arena = PageArena::new();
        let mut receipt = SerializedGreenBuildReceipt::default();
        let document = SerializedGreenDocument::build(
            &mut arena,
            root_spec(summary.metric),
            events,
            &mut receipt,
        )
        .unwrap();
        (arena, document)
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1_000).unwrap();
        }
    }

    #[cfg(feature = "exact-parser")]
    fn drive_direct_line(parser: &mut DirectValueBlockParser, line: &str) {
        parser.begin_line(line.to_owned()).unwrap();
        let limit = line.len().saturating_mul(8).saturating_add(256);
        for _ in 0..limit {
            match parser.poll_line(1).unwrap().status {
                DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                DirectPollStatus::Pending => {}
                DirectPollStatus::ExternalWorkReady => {
                    panic!("non-reference donor fixture unexpectedly requested external work")
                }
                DirectPollStatus::Complete => return,
            }
        }
        panic!("direct test line did not converge");
    }

    #[cfg(feature = "exact-parser")]
    fn direct_parser_after(lines: &[&str]) -> DirectValueBlockParser {
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        parser.acknowledge_command().unwrap();
        for line in lines {
            drive_direct_line(&mut parser, line);
        }
        parser
    }

    #[cfg(feature = "exact-parser")]
    fn direct_restart_sample_after(
        lines: &[&str],
    ) -> (
        DirectGrammarContinuation,
        DirectRestartLineLocalContinuation,
        DirectRestartOutput,
    ) {
        let parser = direct_parser_after(lines);
        let capture = parser
            .capture_durable_grammar_line_boundary_checkpoint()
            .unwrap();
        let (grammar, line_local) = DirectValueBlockParser::decode_durable_grammar_restart_parts(
            capture.header(),
            capture.frame_records(),
        )
        .unwrap();
        let (projected, output) = parser.capture_restart_parts().unwrap();
        assert!(grammar.is_future_grammar_compatible(&projected));
        (grammar, line_local, output)
    }

    #[cfg(feature = "exact-parser")]
    fn open_ordered_list_green(start: u32, delimiter: GreenListDelimiter) -> FixtureBuilder {
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        fixture.enter(
            GreenKind::LIST,
            GreenListOpenFacts::ordered(start, delimiter)
                .unwrap()
                .into_envelope(),
        );
        fixture.enter(
            GreenKind::ITEM,
            GreenItemOpenFacts::new(0, 3).unwrap().into_envelope(),
        );
        let paragraph = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
        fixture.identity_coverage(paragraph, 9, 9);
        fixture
    }

    #[cfg(feature = "exact-parser")]
    fn output_at_open_cut(
        fixture: &FixtureBuilder,
    ) -> (
        PageArena,
        SerializedGreenDocument,
        GreenRestartOutputAtEventCut,
    ) {
        let event_cut = u64::try_from(fixture.events.len()).unwrap();
        let events = fixture.events.clone();
        let mut finishing = FixtureBuilder::new();
        finishing.events = events;
        finishing.open = fixture.open.clone();
        finishing.next_block = fixture.next_block;
        finishing.next_coverage = fixture.next_coverage;
        let (arena, document) = build_document(finishing.finish());
        let output = document
            .restart_output_at_event_cut(&arena, event_cut)
            .unwrap();
        (arena, document, output)
    }

    fn list_facts() -> FactsEnvelope {
        GreenListOpenFacts::bullet(GreenListBullet::Dash).into_envelope()
    }

    fn item_facts() -> FactsEnvelope {
        GreenItemOpenFacts::new(0, 2).unwrap().into_envelope()
    }

    fn mixed_projection_program() -> ProjectionProgram {
        ProjectionProgram::new(vec![
            ProjectionPiece::Hidden {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
                affinity: GreenAffinity::Upstream,
            },
            ProjectionPiece::Atomic {
                physical_metric: SerializedMetric { bytes: 1, utf16: 1 },
                projection: AtomicProjection::nul_to_replacement(),
            },
            ProjectionPiece::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            },
        ])
        .unwrap()
    }

    fn varied_logical_coverage(fixture: &mut FixtureBuilder, target: BlockId, variant: usize) {
        match variant % 5 {
            0 => fixture.coverage(
                target,
                SerializedMetric { bytes: 4, utf16: 2 },
                Some(LogicalContribution::Identity),
            ),
            1 => fixture.coverage(
                target,
                SerializedMetric { bytes: 3, utf16: 2 },
                Some(LogicalContribution::Hidden {
                    affinity: GreenAffinity::Downstream,
                }),
            ),
            2 => fixture.coverage(
                target,
                SerializedMetric { bytes: 1, utf16: 1 },
                Some(LogicalContribution::Atomic(
                    AtomicProjection::nul_to_replacement(),
                )),
            ),
            3 => {
                let program = mixed_projection_program();
                fixture.coverage(
                    target,
                    program.physical_metric(),
                    Some(LogicalContribution::Program(program)),
                );
            }
            4 => fixture.coverage(target, SerializedMetric { bytes: 2, utf16: 1 }, None),
            _ => unreachable!(),
        }
    }

    fn oracle_fixture() -> (Vec<GreenEvent>, u64) {
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        fixture.enter(GreenKind::LIST, list_facts());
        for item in 0..256 {
            fixture.enter(GreenKind::ITEM, item_facts());
            let paragraph = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
            varied_logical_coverage(&mut fixture, paragraph, item);
            fixture.close(item % 37 == 0);
            fixture.close(false);
        }
        fixture.enter(GreenKind::ITEM, item_facts());
        fixture.enter(GreenKind::BLOCK_QUOTE, FactsEnvelope::empty());
        let paragraph = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
        for variant in 0..5 {
            varied_logical_coverage(&mut fixture, paragraph, variant);
        }
        fixture.close(false);
        let nested_open_child_cut = u64::try_from(fixture.events.len()).unwrap();
        (fixture.finish(), nested_open_child_cut)
    }

    fn observe_naive_coverage(
        run: &DecodedSourceProjectionRun,
        open: &mut [GreenRestartOutputFrame],
        source_metric: &mut SerializedMetric,
        coverage_count: &mut u64,
    ) -> Result<(), SerializedGreenError> {
        *source_metric = source_metric.checked_add(run.metric)?;
        *coverage_count = coverage_count
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow("oracle coverage count"))?;
        for frame in open.iter_mut() {
            frame.physical_metric = frame.physical_metric.checked_add(run.metric)?;
        }
        let logical_metric = match &run.logical_contribution {
            DecodedLogicalContribution::None | DecodedLogicalContribution::Hidden { .. } => {
                SerializedMetric::default()
            }
            DecodedLogicalContribution::Identity => run.metric,
            DecodedLogicalContribution::Atomic(projection) => projection.logical_metric,
            DecodedLogicalContribution::Program(program) => program.logical_metric,
        };
        if !logical_metric.is_zero() {
            let terminal = open.last_mut().ok_or(SerializedGreenError::Corrupt(
                "oracle logical contribution has no open terminal",
            ))?;
            if terminal.kind.logical_channel().is_none() {
                return Err(SerializedGreenError::Corrupt(
                    "oracle logical contribution targets a container",
                ));
            }
            terminal.logical_metric = terminal.logical_metric.checked_add(logical_metric)?;
        }
        Ok(())
    }

    fn naive_snapshots(
        document: &SerializedGreenDocument,
        arena: &PageArena,
    ) -> Result<NaiveSnapshots, SerializedGreenError> {
        let mut snapshots = vec![Vec::new()];
        let mut prefix_scalars = vec![NaivePrefixScalars::default()];
        let mut open: Vec<GreenRestartOutputFrame> = Vec::new();
        let mut event_ordinal = 0_u64;
        let mut source_metric = SerializedMetric::default();
        let mut blocks = 0_u64;
        let mut coverage_count = 0_u64;
        let mut leaf_boundaries = Vec::new();
        for leaf_index in 0..document.leaf_count(arena)? {
            let leaf = document
                .leaf_at(arena, leaf_index)?
                .ok_or(SerializedGreenError::Corrupt("oracle leaf is missing"))?;
            let (summary, events) = decode_leaf(arena, leaf)?;
            for decoded in events {
                match decoded.event {
                    DecodedGreenEventKind::Enter { block, kind, facts } => {
                        blocks += 1;
                        open.push(GreenRestartOutputFrame {
                            block,
                            kind,
                            facts,
                            enter: GreenEnterCapability {
                                manifest: document.manifest_id(),
                                leaf,
                                base_leaf_index: leaf_index,
                                byte_offset: decoded.byte_offset,
                                block,
                                kind,
                            },
                            enter_event_ordinal: event_ordinal,
                            closed_children: ChildSequenceAggregate::default(),
                            physical_metric: SerializedMetric::default(),
                            logical_metric: SerializedMetric::default(),
                        });
                    }
                    DecodedGreenEventKind::Coverage(run) => {
                        observe_naive_coverage(
                            &run,
                            &mut open,
                            &mut source_metric,
                            &mut coverage_count,
                        )?;
                    }
                    DecodedGreenEventKind::Exit {
                        closed,
                        last_line_blank,
                        facts,
                    } => {
                        let frame = open.pop().ok_or(SerializedGreenError::Corrupt(
                            "oracle structural stack underflow",
                        ))?;
                        facts.validate_for_kind(frame.kind)?;
                        let semantics = ContainerFoldSemantics {
                            descends_through_last_child: matches!(
                                frame.kind,
                                GreenKind::LIST | GreenKind::ITEM
                            ),
                            is_item: frame.kind == GreenKind::ITEM,
                            last_line_blank,
                        };
                        if semantics.closed_summary(frame.closed_children) != closed {
                            return Err(SerializedGreenError::Corrupt(
                                "oracle close summary disagrees with its frame",
                            ));
                        }
                        if let GreenCloseFacts::List { tight } = facts
                            && tight != frame.closed_children.list_is_tight()
                        {
                            return Err(SerializedGreenError::Corrupt(
                                "oracle List tightness disagrees with its frame",
                            ));
                        }
                        if let Some(parent) = open.last_mut() {
                            parent.closed_children = parent
                                .closed_children
                                .followed_by(ChildSequenceAggregate::singleton(closed));
                        }
                    }
                }
                event_ordinal += 1;
                snapshots.push(open.clone());
                prefix_scalars.push(NaivePrefixScalars {
                    source_metric,
                    blocks,
                    open_depth: u64::try_from(open.len()).unwrap(),
                    coverage_count,
                });
            }
            if summary.tokens == 0 {
                return Err(SerializedGreenError::Corrupt(
                    "oracle encountered an empty packed leaf",
                ));
            }
            leaf_boundaries.push(event_ordinal);
        }
        if !open.is_empty() {
            return Err(SerializedGreenError::Corrupt(
                "oracle document ends with open frames",
            ));
        }
        Ok((snapshots, prefix_scalars, leaf_boundaries))
    }

    fn assert_exact_output_heap_receipt(output: &GreenRestartOutputAtEventCut) {
        let frame_bytes = output.frames.capacity() * std::mem::size_of::<GreenRestartOutputFrame>();
        let field_bytes = output
            .frames
            .iter()
            .map(|frame| frame.facts.fields.capacity() * std::mem::size_of::<FactField>())
            .sum::<usize>();
        let value_bytes = output
            .frames
            .iter()
            .flat_map(|frame| &frame.facts.fields)
            .map(|field| field.value.capacity())
            .sum::<usize>();
        assert_eq!(output.receipt.open_frame_capacity_bytes, frame_bytes);
        assert_eq!(output.receipt.open_fact_field_capacity_bytes, field_bytes);
        assert_eq!(output.receipt.open_fact_value_capacity_bytes, value_bytes);
        assert_eq!(
            output.receipt.open_output_heap_bytes,
            frame_bytes + field_bytes + value_bytes,
        );
    }

    fn packed_leaf_decode_byte_bound(
        document: &SerializedGreenDocument,
        arena: &PageArena,
    ) -> usize {
        (0..document.leaf_count(arena).unwrap())
            .map(|leaf_index| {
                let leaf = document
                    .leaf_at(arena, leaf_index)
                    .unwrap()
                    .expect("packed leaf exists");
                let payload_bytes = arena.payload(leaf).unwrap().len();
                let (_, events) = decode_leaf(arena, leaf).unwrap();
                let event_bytes = events.capacity() * std::mem::size_of::<DecodedLeafEvent>();
                let nested_fact_bytes = events
                    .iter()
                    .filter_map(|event| match &event.event {
                        DecodedGreenEventKind::Enter { facts, .. } => Some(facts),
                        DecodedGreenEventKind::Coverage(_) | DecodedGreenEventKind::Exit { .. } => {
                            None
                        }
                    })
                    .map(|facts| {
                        facts.fields.capacity() * std::mem::size_of::<FactField>()
                            + facts
                                .fields
                                .iter()
                                .map(|field| field.value.capacity())
                                .sum::<usize>()
                    })
                    .sum::<usize>();
                payload_bytes + event_bytes + nested_fact_bytes
            })
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn every_event_cut_and_packed_leaf_boundary_matches_the_naive_output_oracle() {
        let (events, nested_open_child_cut) = oracle_fixture();
        let (mut arena, document) = build_document(events);
        assert!(document.leaf_count(&arena).unwrap() > 1);
        let (snapshots, prefix_scalars, leaf_boundaries) =
            naive_snapshots(&document, &arena).unwrap();
        assert!(leaf_boundaries.len() > 1);
        let packed_page_decode_bound = packed_leaf_decode_byte_bound(&document, &arena);
        let mut maximum_observed_decode_bytes = 0;

        for (event_cut, expected) in snapshots.iter().enumerate() {
            let event_cut = u64::try_from(event_cut).unwrap();
            let output = document
                .restart_output_at_event_cut(&arena, event_cut)
                .unwrap();
            assert_eq!(output.manifest(), document.manifest_id());
            assert_eq!(output.event_cut(), event_cut);
            let expected_prefix = prefix_scalars[usize::try_from(event_cut).unwrap()];
            assert_eq!(output.source_metric(), expected_prefix.source_metric);
            assert_eq!(output.blocks(), expected_prefix.blocks);
            assert_eq!(output.open_depth(), expected_prefix.open_depth);
            assert_eq!(output.coverage_count(), expected_prefix.coverage_count);
            assert_eq!(output.frames(), expected, "event cut {event_cut}");
            assert_eq!(output.receipt().range_queries, expected.len() + 1);
            assert_eq!(output.receipt().output_frames, expected.len());
            assert_eq!(output.receipt().retained_source_bytes, 0);
            assert_eq!(output.receipt().document_sized_event_vectors, 0);
            assert!(output.receipt().maximum_decoded_page_bytes <= packed_page_decode_bound);
            maximum_observed_decode_bytes =
                maximum_observed_decode_bytes.max(output.receipt().maximum_decoded_page_bytes);
            assert_exact_output_heap_receipt(&output);
        }
        assert_eq!(
            maximum_observed_decode_bytes, packed_page_decode_bound,
            "every event cut exercises the exact packed-leaf decode bound",
        );

        for event_cut in leaf_boundaries {
            let output = document
                .restart_output_at_event_cut(&arena, event_cut)
                .unwrap();
            assert_eq!(
                output.frames(),
                snapshots[usize::try_from(event_cut).unwrap()].as_slice(),
                "packed leaf boundary {event_cut}",
            );
        }

        let nested = document
            .restart_output_at_event_cut(&arena, nested_open_child_cut)
            .unwrap();
        let block_quote = nested.frames().last().unwrap();
        assert_eq!(block_quote.kind(), GreenKind::BLOCK_QUOTE);
        assert_eq!(
            block_quote.closed_children(),
            ChildSequenceAggregate::singleton(ClosedChildAggregate::default()),
            "an open child retains its already-closed direct grandchild output",
        );

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    fn large_deep_fixture() -> (Vec<GreenEvent>, u64, usize) {
        const CLOSED_TOP_LEVEL_ITEMS: usize = 2_048;
        const NESTED_LEVELS: usize = 64;
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        fixture.enter(GreenKind::LIST, list_facts());
        for item in 0..CLOSED_TOP_LEVEL_ITEMS {
            fixture.enter(GreenKind::ITEM, item_facts());
            let paragraph = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
            if item % 5 == 0 {
                fixture.identity_coverage(paragraph, 4, 2);
            } else {
                fixture.identity_coverage(paragraph, 2, 2);
            }
            fixture.close(item % 113 == 0);
            fixture.close(false);
        }
        fixture.enter(GreenKind::ITEM, item_facts());
        for _ in 0..NESTED_LEVELS {
            fixture.enter(GreenKind::LIST, list_facts());
            fixture.enter(GreenKind::ITEM, item_facts());
            let paragraph = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
            fixture.identity_coverage(paragraph, 4, 2);
            fixture.close(false);
            fixture.close(false);
            fixture.enter(GreenKind::ITEM, item_facts());
        }
        let event_cut = u64::try_from(fixture.events.len()).unwrap();
        let open_depth = fixture.open.len();
        (fixture.finish(), event_cut, open_depth)
    }

    #[test]
    fn deep_large_list_query_is_bounded_by_open_depth_times_tree_height() {
        let (events, event_cut, open_depth) = large_deep_fixture();
        let (mut arena, document) = build_document(events);
        let manifest_id = document.local_manifest_id(&arena).unwrap();
        let (manifest, _) = decode_document(&arena, manifest_id).unwrap();
        assert!(manifest.summary.leaves > 16);
        let height = usize::from(manifest.summary.height);

        let output = document
            .restart_output_at_event_cut(&arena, event_cut)
            .unwrap();
        let receipt = *output.receipt();
        let packed_page_decode_bound = packed_leaf_decode_byte_bound(&document, &arena);
        assert_eq!(output.frames().len(), open_depth);
        assert_eq!(receipt.range_queries, open_depth + 1);
        assert_eq!(receipt.maximum_open_depth, open_depth);
        assert_eq!(receipt.output_frames, open_depth);
        assert!(receipt.summary_nodes_reused > 0);
        assert!(receipt.maximum_route_depth <= height);
        assert!(receipt.maximum_decoded_page_bytes <= packed_page_decode_bound);
        assert!(
            receipt.leaf_pages_decoded <= 3 * (open_depth + 1),
            "only open-path and event-range boundary leaves may be decoded",
        );
        let node_visit_bound = 8 * (open_depth + 1) * (height + 1);
        assert!(
            receipt.sequence_nodes_visited <= node_visit_bound,
            "query must remain O((open_depth + 1) * (height + 1))",
        );
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(receipt.document_sized_event_vectors, 0);
        assert_exact_output_heap_receipt(&output);
        assert!(
            receipt.open_output_heap_bytes
                <= 4 * open_depth
                    * (std::mem::size_of::<GreenRestartOutputFrame>()
                        + std::mem::size_of::<FactField>()
                        + MAX_INLINE_FACT_BYTES),
        );
        assert_eq!(output.frames()[0].kind(), GreenKind::DOCUMENT);
        assert!(
            output.frames()[1].closed_children().had_child,
            "the current outer List output includes its large closed prefix",
        );

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    fn giant_terminal_fixture(
        kind: GreenKind,
        runs: usize,
    ) -> (Vec<GreenEvent>, u64, SerializedMetric, SerializedMetric) {
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        let facts = if kind == GreenKind::FENCED_CODE {
            GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 3, 0)
                .unwrap()
                .into_envelope()
        } else {
            FactsEnvelope::empty()
        };
        let terminal = fixture.enter(kind, facts);
        let physical_run = SerializedMetric { bytes: 4, utf16: 2 };
        let mut physical_metric = SerializedMetric::default();
        let mut logical_metric = SerializedMetric::default();
        for index in 0..runs {
            let contribution = if index.is_multiple_of(2) {
                logical_metric = logical_metric.checked_add(physical_run).unwrap();
                LogicalContribution::Identity
            } else {
                LogicalContribution::Hidden {
                    affinity: GreenAffinity::Downstream,
                }
            };
            fixture.coverage(terminal, physical_run, Some(contribution));
            physical_metric = physical_metric.checked_add(physical_run).unwrap();
        }
        let event_cut = u64::try_from(fixture.events.len()).unwrap();
        (fixture.finish(), event_cut, physical_metric, logical_metric)
    }

    #[test]
    fn giant_open_terminal_logical_query_skips_terminal_length_history() {
        const RUNS: usize = 16_384;
        for kind in [GreenKind::PARAGRAPH, GreenKind::FENCED_CODE] {
            let (events, event_cut, physical_metric, logical_metric) =
                giant_terminal_fixture(kind, RUNS);
            let (mut arena, document) = build_document(events);
            let manifest_id = document.local_manifest_id(&arena).unwrap();
            let (manifest, _) = decode_document(&arena, manifest_id).unwrap();
            assert!(manifest.summary.leaves > 32);
            let height = usize::from(manifest.summary.height);

            let output = document
                .restart_output_at_event_cut(&arena, event_cut)
                .unwrap();
            let receipt = *output.receipt();
            assert_eq!(output.source_metric(), physical_metric);
            assert_eq!(output.coverage_count(), u64::try_from(RUNS).unwrap());
            assert_eq!(output.open_depth(), 2);
            assert_eq!(output.frames().len(), 2);
            assert_eq!(output.frames()[0].kind(), GreenKind::DOCUMENT);
            assert_eq!(
                output.frames()[0].logical_metric(),
                SerializedMetric::default(),
                "containers never inherit descendant logical output",
            );
            assert_eq!(output.frames()[1].kind(), kind);
            assert_eq!(output.frames()[1].logical_metric(), logical_metric);

            let depth = output.frames().len();
            assert_eq!(receipt.range_queries, depth + 1);
            assert!(receipt.summary_nodes_reused > 0);
            assert!(receipt.maximum_route_depth <= height);
            assert!(receipt.leaf_pages_decoded <= 3 * (depth + 1));
            assert!(
                receipt.sequence_nodes_visited <= 8 * (depth + 1) * (height + 1),
                "query work is O((depth + 1) * (height + 1))",
            );
            assert!(
                receipt.events_decoded < RUNS / 8,
                "the open terminal's event history must not be decoded linearly",
            );
            assert_eq!(receipt.retained_source_bytes, 0);
            assert_eq!(receipt.document_sized_event_vectors, 0);
            assert_exact_output_heap_receipt(&output);

            document.release_later(&mut arena).unwrap();
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, 0);
        }
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn current_ordered_display_start_binds_to_compatible_retained_grammar() {
        let (retained_grammar, _retained_line_local, retained_output) =
            direct_restart_sample_after(&["1. alpha\n"]);
        let (current_grammar, current_line_local, current_output) =
            direct_restart_sample_after(&["7. alpha\n"]);
        assert!(retained_grammar.is_future_grammar_compatible(&current_grammar));
        assert_ne!(
            retained_output, current_output,
            "ordered start is current output"
        );

        let fixture = open_ordered_list_green(7, GreenListDelimiter::Period);
        let (mut arena, document, output) = output_at_open_cut(&fixture);
        let path = output.into_current_restart_path().unwrap();
        assert_eq!(path.frames().len(), 4);
        let list = path
            .frames()
            .iter()
            .find(|frame| frame.green_kind() == GreenKind::LIST)
            .unwrap();
        assert_eq!(
            list.donor().kind,
            DirectBlockKind::List(DirectListFacts {
                list_type: DirectListType::Ordered,
                start: 7,
                delimiter: DirectListDelimiter::Period,
                bullet_char: 0,
            })
        );
        assert!(list.normalization().is_none());
        let bound = path
            .pair_with_stabilized_line_mechanism_only(current_line_local)
            .bind_direct_restart_output_mechanism_only(&retained_grammar)
            .unwrap();
        assert_eq!(bound.donor_output(), &current_output);
        assert_eq!(bound.path().query_receipt().range_queries, 5);
        assert_eq!(bound.path().query_receipt().retained_source_bytes, 0);
        assert_eq!(bound.path().mapping_receipt().mapped_frames, 4);
        assert_eq!(bound.path().mapping_receipt().typed_fact_decodes, 2);
        assert_eq!(bound.path().mapping_receipt().retained_source_bytes, 0);

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn setext_heading_facts_bind_directly_but_never_invert_to_paragraph() {
        let (heading_grammar, heading_line_local, heading_output) =
            direct_restart_sample_after(&["alpha\n", "===\n"]);
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        let heading = fixture.enter(
            GreenKind::HEADING,
            GreenHeadingOpenFacts::setext(1).unwrap().into_envelope(),
        );
        fixture.identity_coverage(heading, 10, 10);
        let event_cut = u64::try_from(fixture.events.len()).unwrap();
        let (mut arena, document, output) = output_at_open_cut(&fixture);
        let path = output.into_current_restart_path().unwrap();
        assert_eq!(
            path.frames()[1].donor().kind,
            DirectBlockKind::Heading(DirectHeadingFacts {
                level: 1,
                setext: true,
            })
        );
        let bound = path
            .pair_with_stabilized_line_mechanism_only(heading_line_local)
            .bind_direct_restart_output_mechanism_only(&heading_grammar)
            .unwrap();
        assert_eq!(bound.donor_output(), &heading_output);

        let (paragraph_grammar, paragraph_line_local, _paragraph_output) =
            direct_restart_sample_after(&["alpha\n"]);
        let second_path = document
            .restart_output_at_event_cut(&arena, event_cut)
            .unwrap()
            .into_current_restart_path()
            .unwrap();
        assert!(matches!(
            second_path
                .pair_with_stabilized_line_mechanism_only(paragraph_line_local)
                .bind_direct_restart_output_mechanism_only(&paragraph_grammar),
            Err(CurrentRestartPathError::Donor(_))
        ));

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_child_fold_mapping_preserves_every_list_tightness_bit() {
        let green = ChildSequenceAggregate {
            had_child: true,
            any_nonlast_child_ends_blank: false,
            last_child_ends_blank: true,
            list_loose_before_last: true,
            last_item_loose_if_nonlast: false,
            last_item_loose_if_last: true,
        };
        assert_eq!(
            direct_child_sequence_fold(green),
            ChildSequenceFold {
                had_child: true,
                any_nonlast_child_ends_blank: false,
                last_child_ends_blank: true,
                list_loose_before_last: true,
                last_item_loose_if_nonlast: false,
                last_item_loose_if_last: true,
            }
        );

        let (_current_grammar, current_line_local, current_output) =
            direct_restart_sample_after(&["- a\n", "- b\n"]);
        let (retained_grammar, _retained_line_local, _) =
            direct_restart_sample_after(&["- x\n", "- y\n"]);
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        fixture.enter(GreenKind::LIST, list_facts());
        fixture.enter(GreenKind::ITEM, item_facts());
        let first = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
        fixture.identity_coverage(first, 4, 4);
        fixture.close(false);
        fixture.close(false);
        fixture.enter(GreenKind::ITEM, item_facts());
        let second = fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
        fixture.identity_coverage(second, 4, 4);
        let (mut arena, document, output) = output_at_open_cut(&fixture);
        let bound = output
            .bind_direct_restart_output_from_stabilized_line_mechanism_only(
                &retained_grammar,
                current_line_local,
            )
            .unwrap();
        assert_eq!(bound.donor_output(), &current_output);
        assert!(bound.path().frames()[1].donor().closed_children.had_child);

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn crossed_direct_grammar_and_unsupported_green_inputs_fail_closed() {
        let (period_grammar, _period_line_local, _) = direct_restart_sample_after(&["1. alpha\n"]);
        let (_paren_grammar, paren_line_local, _) = direct_restart_sample_after(&["1) alpha\n"]);
        let paren = open_ordered_list_green(1, GreenListDelimiter::Parenthesis);
        let (mut arena, document, output) = output_at_open_cut(&paren);
        assert!(matches!(
            output.bind_direct_restart_output_from_stabilized_line_mechanism_only(
                &period_grammar,
                paren_line_local,
            ),
            Err(CurrentRestartPathError::Donor(_))
        ));
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut unsupported = FixtureBuilder::new();
        unsupported.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        unsupported.enter(GreenKind::INDENTED_CODE, FactsEnvelope::empty());
        let (mut arena, document, output) = output_at_open_cut(&unsupported);
        assert!(matches!(
            output.into_current_restart_path(),
            Err(CurrentRestartPathError::UnsupportedKind {
                kind: GreenKind::INDENTED_CODE,
                ..
            })
        ));
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut noncanonical = FixtureBuilder::new();
        noncanonical.enter(
            GreenKind::DOCUMENT,
            FactsEnvelope::new(vec![FactField::optional(FactId(99), vec![1])]).unwrap(),
        );
        let (mut arena, document, output) = output_at_open_cut(&noncanonical);
        assert!(matches!(
            output.into_current_restart_path(),
            Err(CurrentRestartPathError::NonCanonicalFacts {
                kind: GreenKind::DOCUMENT,
                ..
            })
        ));
        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn large_mapped_path_preserves_bounded_query_receipts() {
        let (events, event_cut, open_depth) = large_deep_fixture();
        let (mut arena, document) = build_document(events);
        let path = document
            .restart_output_at_event_cut(&arena, event_cut)
            .unwrap()
            .into_current_restart_path()
            .unwrap();
        let receipt = path.query_receipt();
        let mapping = path.mapping_receipt();
        assert_eq!(path.frames().len(), open_depth);
        assert_eq!(receipt.range_queries, open_depth + 1);
        assert_eq!(mapping.input_frames, open_depth);
        assert_eq!(mapping.mapped_frames, open_depth);
        assert_eq!(
            mapping.mapped_path_capacity_bytes,
            path.frames().len() * std::mem::size_of::<CurrentRestartPathFrame>()
        );
        assert_eq!(receipt.retained_source_bytes, 0);
        assert_eq!(mapping.retained_source_bytes, 0);
        assert_eq!(receipt.document_sized_event_vectors, 0);
        assert_eq!(mapping.document_sized_event_vectors, 0);

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn impossible_open_terminal_nesting_fails_closed() {
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        fixture.enter(GreenKind::PARAGRAPH, FactsEnvelope::empty());
        let event_cut = u64::try_from(fixture.events.len()).unwrap();
        let (mut arena, document) = build_document(fixture.finish());
        let output = document
            .restart_output_at_event_cut(&arena, event_cut)
            .unwrap();
        let valid = output.frames().to_vec();
        assert_eq!(open_terminal_index(&valid), Ok(Some(1)));

        let mut terminal_above_container = valid.clone();
        let mut container = terminal_above_container[0].clone();
        container.kind = GreenKind::BLOCK_QUOTE;
        terminal_above_container.push(container);
        assert_eq!(
            open_terminal_index(&terminal_above_container),
            Err(SerializedGreenError::Corrupt(
                "restart-output terminal is nested or is not the deepest open frame"
            )),
        );

        let mut two_terminals = valid.clone();
        two_terminals.push(valid[1].clone());
        assert_eq!(
            open_terminal_index(&two_terminals),
            Err(SerializedGreenError::Corrupt(
                "restart-output terminal is nested or is not the deepest open frame"
            )),
        );

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn wrong_arena_and_out_of_range_event_cuts_fail_closed() {
        let mut fixture = FixtureBuilder::new();
        fixture.enter(GreenKind::DOCUMENT, FactsEnvelope::empty());
        let events = fixture.finish();
        let (mut arena, document) = build_document(events.clone());
        let token_count = decode_document(&arena, document.local_manifest_id(&arena).unwrap())
            .unwrap()
            .0
            .summary
            .tokens;
        assert_eq!(
            document.restart_output_at_event_cut(&arena, token_count + 1),
            Err(SerializedGreenError::StaleCursor),
        );

        let (mut other_arena, other_document) = build_document(events);
        assert!(matches!(
            document.restart_output_at_event_cut(&other_arena, 0),
            Err(SerializedGreenError::Arena(_)),
        ));

        document.release_later(&mut arena).unwrap();
        other_document.release_later(&mut other_arena).unwrap();
        settle(&mut arena);
        settle(&mut other_arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(other_arena.metrics().live_nodes, 0);
    }
}

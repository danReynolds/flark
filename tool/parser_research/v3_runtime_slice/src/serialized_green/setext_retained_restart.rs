//! Storage-only proof for reopening a finalized Setext prefix across builds.
//!
//! The old immutable document remains the only authority for packed bytes. A
//! sealed normalization manifest records canonical event/source coordinates,
//! never a capture-time leaf ordinal. Restart resolves those coordinates
//! again, retains the aligned prefix through the persistent sequence, and
//! performs exactly one typed `Heading(Setext) -> Paragraph` inverse before
//! exposing a fresh-build provisional Paragraph capability.
//!
//! This module deliberately proves no source-lineage, parser-continuation, or
//! composer authority. Those capabilities must be joined by the actor before
//! a production candidate can be published.

use crate::arena::{ArenaBuildOwner, ArenaBuildSession, ArenaBuildTicket};
#[cfg(feature = "exact-parser")]
use crate::candidate_writer::{CandidateWriterError, ParentSelectedCandidateWriterGreenReady};
use crate::persistent_sequence::{ResumableSequenceRetainedRange, ResumableSequenceSplitProgress};
#[cfg(feature = "exact-parser")]
use crate::storage_only_composite_document::{
    ParentSelectedRestartCompositeAdoptionLease, RestartCompositeDocumentError,
};
use crate::{ArenaId, BlockId, LiveCandidateEpoch, PageArena};

#[allow(clippy::wildcard_imports)]
// This private submodule is the implementation split of its parent.
use super::*;

/// Audit counters for the bounded storage proof. Both canonical-resolution
/// passes descend persistent summaries, then decode only the target/cut pages
/// plus the open path. `seek` allocates one bounded decoded-leaf vector and
/// reports its footprint; neither source text nor a document-sized event
/// vector is materialized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SetextRetainedGreenRestartReceipt {
    pub(crate) canonical_resolution_passes: u64,
    pub(crate) canonical_pages_scanned: u64,
    pub(crate) canonical_events_scanned: u64,
    pub(crate) canonical_sequence_nodes_visited: u64,
    pub(crate) maximum_canonical_route_depth: usize,
    pub(crate) maximum_restored_open_depth: usize,
    pub(crate) maximum_bounded_page_decode_bytes: usize,
    pub(crate) source_payload_bytes_materialized: usize,
    pub(crate) document_sized_event_vectors_materialized: usize,
    pub(crate) retained_leaves: u64,
    pub(crate) inverse_leaf_pages_allocated: u64,
    pub(crate) persistent_sequence_leaves_reused: usize,
}

impl SetextRetainedGreenRestartReceipt {
    fn absorb_resolution(&mut self, resolution: ResolutionReceipt) {
        self.canonical_resolution_passes += 1;
        self.canonical_pages_scanned += resolution.pages_scanned;
        self.canonical_events_scanned += resolution.events_scanned;
        self.canonical_sequence_nodes_visited += resolution.sequence_nodes_visited;
        self.maximum_canonical_route_depth = self
            .maximum_canonical_route_depth
            .max(resolution.maximum_route_depth);
        self.maximum_restored_open_depth = self
            .maximum_restored_open_depth
            .max(resolution.maximum_open_depth);
        self.maximum_bounded_page_decode_bytes = self
            .maximum_bounded_page_decode_bytes
            .max(resolution.maximum_bounded_page_decode_bytes);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ResolutionReceipt {
    pages_scanned: u64,
    events_scanned: u64,
    sequence_nodes_visited: u64,
    maximum_route_depth: usize,
    maximum_open_depth: usize,
    maximum_bounded_page_decode_bytes: usize,
}

impl ResolutionReceipt {
    fn absorb_restart_output(
        &mut self,
        restart: &GreenRestartOutputReceipt,
    ) -> Result<(), SerializedGreenError> {
        self.pages_scanned = self
            .pages_scanned
            .checked_add(u64::try_from(restart.leaf_pages_decoded).map_err(|_| {
                SerializedGreenError::Overflow("Setext restart-output decoded page receipt")
            })?)
            .ok_or(SerializedGreenError::Overflow(
                "Setext restart-output decoded page receipt",
            ))?;
        self.events_scanned = self
            .events_scanned
            .checked_add(u64::try_from(restart.events_decoded).map_err(|_| {
                SerializedGreenError::Overflow("Setext restart-output decoded event receipt")
            })?)
            .ok_or(SerializedGreenError::Overflow(
                "Setext restart-output decoded event receipt",
            ))?;
        self.sequence_nodes_visited = self
            .sequence_nodes_visited
            .checked_add(u64::try_from(restart.sequence_nodes_visited).map_err(|_| {
                SerializedGreenError::Overflow("Setext restart-output sequence-node receipt")
            })?)
            .ok_or(SerializedGreenError::Overflow(
                "Setext restart-output sequence-node receipt",
            ))?;
        self.maximum_route_depth = self.maximum_route_depth.max(restart.maximum_route_depth);
        self.maximum_open_depth = self.maximum_open_depth.max(restart.maximum_open_depth);
        self.maximum_bounded_page_decode_bytes = self
            .maximum_bounded_page_decode_bytes
            .max(restart.maximum_decoded_page_bytes);
        Ok(())
    }
}

/// Immutable authority that one finalized Enter was produced by Setext
/// normalization and may be inverted at one exact accepted green cut.
///
/// Every field is private and the only constructor validates the committed
/// old document. In particular, no caller can infer a Paragraph from an
/// arbitrary Heading or substitute a different block, outcome, profile, or
/// generation.
#[derive(Debug)]
pub(crate) struct SealedSetextNormalizationManifest {
    old_binding: SerializedGreenManifestDescriptor,
    block: BlockId,
    final_heading: GreenHeadingOpenFacts,
    accepted_event_cut: u64,
    accepted_source_cut: SerializedMetric,
    target_event_ordinal: u64,
    target_source_before: SerializedMetric,
    capture_receipt: ResolutionReceipt,
}

#[derive(Debug)]
struct ResolvedRetainedParagraphCheckpoint {
    block: BlockId,
    terminal_kind: GreenKind,
    old_root: ArenaId,
    cut_leaf_count: u64,
    prefix_summary: GreenSummary,
    target_leaf: ArenaId,
    target_leaf_index: u64,
    target_event_ordinal: u64,
    target_source_before: SerializedMetric,
    target_projection_runs_before: u64,
    target_event_ordinal_in_leaf: u64,
    target_source_before_in_leaf: SerializedMetric,
    target_byte_offset: u16,
    restored_validator: Option<StructuralValidator>,
}

/// Linear green-side origin for a canonical fragment which was already open
/// at a parent-selected restart cut.  Every coordinate is derived from the
/// same old manifest/path query; no source or parser caller can construct this
/// value from copied counters.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedCanonicalFragmentOriginSeed {
    old_manifest: SerializedGreenManifestId,
    build: ArenaBuildId,
    block: BlockId,
    enter_event_ordinal: u64,
    source_before: SerializedMetric,
    projection_runs_before: u64,
    accepted_event_cut: u64,
    accepted_source_cut: SerializedMetric,
    accepted_projection_runs: u64,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedCanonicalFragmentOriginSeed {
    fn from_resolved_plan(
        old_manifest: SerializedGreenManifestId,
        build: ArenaBuildId,
        accepted_event_cut: u64,
        accepted_source_cut: SerializedMetric,
        accepted_projection_runs: u64,
        plan: &ResolvedRetainedParagraphCheckpoint,
    ) -> Result<Self, SerializedGreenError> {
        if plan.terminal_kind != GreenKind::PARAGRAPH
            || plan.block.0 == 0
            || plan.target_event_ordinal >= accepted_event_cut
            || plan.target_source_before.bytes > accepted_source_cut.bytes
            || plan.target_source_before.utf16 > accepted_source_cut.utf16
            || plan.target_projection_runs_before > accepted_projection_runs
        {
            return Err(SerializedGreenError::Corrupt(
                "retained canonical fragment origin crosses its selected cut",
            ));
        }
        Ok(Self {
            old_manifest,
            build,
            block: plan.block,
            enter_event_ordinal: plan.target_event_ordinal,
            source_before: plan.target_source_before,
            projection_runs_before: plan.target_projection_runs_before,
            accepted_event_cut,
            accepted_source_cut,
            accepted_projection_runs,
        })
    }

    pub(crate) fn matches_parent_selected_join(
        &self,
        epoch: LiveCandidateEpoch,
        coverage: &crate::ParentSelectedComposerCoverage,
        provisional: &ProvisionalParagraphEnter,
        cut: &SerializedGreenLeafCut,
    ) -> bool {
        // Merely reading the manifest here is intentional: keeping it inside
        // this consumed carrier preserves the old-manifest provenance across
        // the source/green join without exposing a forgeable descriptor.
        let _old_manifest = self.old_manifest;
        self.build == epoch.build_id()
            && coverage.epoch() == epoch
            && self.block == provisional.block
            && self.build == provisional.build
            && self.enter_event_ordinal == provisional.event_ordinal
            && self.source_before == provisional.source_before
            && self.accepted_event_cut == coverage.event_cut()
            && self.accepted_event_cut == cut.events_before()
            && self.accepted_source_cut == coverage.accepted_source()
            && self.accepted_source_cut == cut.source_before()
            && self.accepted_projection_runs == coverage.projection_runs()
            && self.projection_runs_before <= self.accepted_projection_runs
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn source_before(&self) -> SerializedMetric {
        self.source_before
    }

    pub(crate) const fn projection_runs_before(&self) -> u64 {
        self.projection_runs_before
    }

    pub(crate) const fn accepted_source_cut(&self) -> SerializedMetric {
        self.accepted_source_cut
    }

    pub(crate) const fn accepted_projection_runs(&self) -> u64 {
        self.accepted_projection_runs
    }
}

impl SerializedGreenDocument {
    /// Seals one Setext normalization recipe after resolving the requested cut
    /// from canonical event/source coordinates in the committed document.
    /// No leaf index is stored in the resulting manifest.
    #[allow(clippy::too_many_arguments)] // Every scalar is independently cross-checked into one sealed recipe.
    pub(crate) fn seal_setext_normalization_manifest(
        &self,
        arena: &PageArena,
        block: BlockId,
        final_heading: GreenHeadingOpenFacts,
        target_event_ordinal: u64,
        target_source_before: SerializedMetric,
        accepted_event_cut: u64,
        accepted_source_cut: SerializedMetric,
    ) -> Result<SealedSetextNormalizationManifest, SerializedGreenError> {
        if final_heading.style() != GreenHeadingStyle::Setext {
            return Err(SerializedGreenError::Invalid(
                "normalization manifest requires a final Setext Heading",
            ));
        }
        let manifest_id = self.local_manifest_id(arena)?;
        let (old_manifest, _) = decode_document(arena, manifest_id)?;
        let old_binding = SerializedGreenManifestDescriptor::new(self.manifest_id(), &old_manifest);
        let (_, capture_receipt) = resolve_setext_checkpoint(
            self,
            arena,
            block,
            final_heading,
            target_event_ordinal,
            target_source_before,
            accepted_event_cut,
            accepted_source_cut,
        )?;
        Ok(SealedSetextNormalizationManifest {
            old_binding,
            block,
            final_heading,
            accepted_event_cut,
            accepted_source_cut,
            target_event_ordinal,
            target_source_before,
            capture_receipt,
        })
    }

    /// Finalizes the normalization authority from the already joined green
    /// checkpoint draft. The integrated proof never supplies a `BlockId` or
    /// event/source ordinal to the manifest constructor.
    pub(crate) fn seal_setext_normalization_from_joined_checkpoint(
        &self,
        arena: &PageArena,
        checkpoint: &RetainedSetextGreenCheckpointDraft,
        final_heading: GreenHeadingOpenFacts,
    ) -> Result<SealedSetextNormalizationManifest, SerializedGreenError> {
        self.seal_setext_normalization_manifest(
            arena,
            checkpoint.block,
            final_heading,
            checkpoint.target_event_ordinal,
            checkpoint.target_source_before,
            checkpoint.accepted_event_cut,
            checkpoint.accepted_source_cut,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedEventLeaf {
    leaf: ArenaId,
    leaf_index: u64,
    prefix: GreenSummary,
    event_ordinal_in_leaf: u64,
}

trait AuthenticatedRetainedRestartFrame {
    fn block(&self) -> BlockId;
    fn kind(&self) -> GreenKind;
    /// `None` is deliberately conservative: callers must not reconstruct a
    /// fact-sensitive direct-suffix cut from a restart path that did not carry
    /// canonical Enter facts.
    fn facts(&self) -> Option<&FactsEnvelope>;
    fn enter(&self) -> GreenEnterCapability;
    fn enter_event_ordinal(&self) -> u64;
    fn closed_children(&self) -> ChildSequenceAggregate;
    fn physical_metric(&self) -> SerializedMetric;
}

impl AuthenticatedRetainedRestartFrame for GreenRestartOutputFrame {
    fn block(&self) -> BlockId {
        self.block()
    }

    fn kind(&self) -> GreenKind {
        self.kind()
    }

    fn facts(&self) -> Option<&FactsEnvelope> {
        Some(self.facts())
    }

    fn enter(&self) -> GreenEnterCapability {
        self.enter()
    }

    fn enter_event_ordinal(&self) -> u64 {
        self.enter_event_ordinal()
    }

    fn closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children()
    }

    fn physical_metric(&self) -> SerializedMetric {
        self.physical_metric()
    }
}

#[cfg(feature = "exact-parser")]
impl AuthenticatedRetainedRestartFrame for CurrentRestartPathFrame {
    fn block(&self) -> BlockId {
        self.block()
    }

    fn kind(&self) -> GreenKind {
        self.green_kind()
    }

    fn facts(&self) -> Option<&FactsEnvelope> {
        None
    }

    fn enter(&self) -> GreenEnterCapability {
        self.enter()
    }

    fn enter_event_ordinal(&self) -> u64 {
        self.enter_event_ordinal()
    }

    fn closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children()
    }

    fn physical_metric(&self) -> SerializedMetric {
        self.physical_metric()
    }
}

fn locate_event_leaf_with_prefix(
    arena: &PageArena,
    root: ArenaId,
    event_ordinal: u64,
    receipt: &mut ResolutionReceipt,
) -> Result<LocatedEventLeaf, SerializedGreenError> {
    let root_summary = sequence_node::<SerializedGreenSpec>(arena, root)?.0;
    receipt.sequence_nodes_visited += 1;
    if event_ordinal >= root_summary.tokens {
        return Err(SerializedGreenError::StaleCursor);
    }
    let mut node = root;
    let mut remaining = event_ordinal;
    let mut leaf_index = 0_u64;
    let mut prefix = GreenSummary::default();
    let mut route_depth = 0_usize;
    loop {
        receipt.sequence_nodes_visited += 1;
        match sequence_node::<SerializedGreenSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                receipt.maximum_route_depth = receipt.maximum_route_depth.max(route_depth);
                return Ok(LocatedEventLeaf {
                    leaf: node,
                    leaf_index,
                    prefix,
                    event_ordinal_in_leaf: remaining,
                });
            }
            SequenceNodeKind::Branch { left, right } => {
                route_depth = route_depth
                    .checked_add(1)
                    .ok_or(SerializedGreenError::Overflow(
                        "Setext canonical route depth",
                    ))?;
                let left_summary = sequence_node::<SerializedGreenSpec>(arena, left)?.0;
                receipt.sequence_nodes_visited += 1;
                if remaining < left_summary.tokens {
                    node = left;
                } else {
                    remaining -= left_summary.tokens;
                    leaf_index = leaf_index.checked_add(left_summary.leaves).ok_or(
                        SerializedGreenError::Overflow("Setext canonical leaf index"),
                    )?;
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

/// Counts canonical projection runs before one event in a page-bounded leaf.
/// The route prefix came from the same immutable sequence descent, so this is
/// one bounded decode rather than a source or suffix scan.
fn projection_runs_before_located_event(
    arena: &PageArena,
    located: LocatedEventLeaf,
) -> Result<u64, SerializedGreenError> {
    let mut ordinal = 0_u64;
    let mut local_prefix = GreenSummary::default();
    visit_decoded_leaf_events(arena, located.leaf, |_, event| {
        if ordinal < located.event_ordinal_in_leaf {
            local_prefix = local_prefix.followed_by(GreenSummary::decoded_event(&event))?;
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "retained fragment local event ordinal",
            ))?;
        Ok(())
    })?;
    if ordinal <= located.event_ordinal_in_leaf {
        return Err(SerializedGreenError::StaleCursor);
    }
    located
        .prefix
        .followed_by(local_prefix)?
        .coverage_runs_for_valid_prefix()
}

/// Reconstructs the retained prefix and builder validator from one already
/// authenticated current-green path. Every open ancestor and its exact closed
/// child fold survives; Setext changes only the deepest frame's restored kind.
fn resolve_retained_checkpoint_from_path<F: AuthenticatedRetainedRestartFrame>(
    arena: &PageArena,
    manifest_capability: SerializedGreenManifestId,
    old_root: ArenaId,
    path: &[F],
    accepted_event_cut: u64,
    accepted_source_cut: SerializedMetric,
    transform: RetainedGreenTransform,
    receipt: &mut ResolutionReceipt,
) -> Result<ResolvedRetainedParagraphCheckpoint, SerializedGreenError> {
    let terminal_index = path
        .len()
        .checked_sub(1)
        .ok_or(SerializedGreenError::StaleCursor)?;
    if path[0].kind() != GreenKind::DOCUMENT || path[0].block().0 == 0 {
        return Err(SerializedGreenError::StaleCursor);
    }

    let mut previous_enter = None;
    for (index, frame) in path.iter().enumerate() {
        let enter = frame.enter();
        if frame.block().0 == 0
            || enter.manifest != manifest_capability
            || enter.block != frame.block()
            || enter.kind != frame.kind()
            || frame.enter_event_ordinal() >= accepted_event_cut
            || previous_enter.is_some_and(|previous| previous >= frame.enter_event_ordinal())
            || (index != 0 && frame.kind() == GreenKind::DOCUMENT)
            || (index != terminal_index && frame.kind().logical_channel().is_some())
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        previous_enter = Some(frame.enter_event_ordinal());
    }

    let terminal = &path[terminal_index];
    let terminal_kind = match transform {
        RetainedGreenTransform::Direct => terminal.kind(),
        RetainedGreenTransform::InvertSetext(_) => {
            if terminal.kind() != GreenKind::HEADING {
                return Err(SerializedGreenError::StaleCursor);
            }
            GreenKind::PARAGRAPH
        }
    };
    let target_event_ordinal = terminal.enter_event_ordinal();
    let target_enter = terminal.enter();
    let target = locate_event_leaf_with_prefix(arena, old_root, target_event_ordinal, receipt)?;
    if target_enter.leaf != target.leaf
        || target_enter.base_leaf_index != target.leaf_index
        || target_enter.block != terminal.block()
        || target_enter.kind != terminal.kind()
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    let target_source_before = accepted_source_cut.checked_sub(terminal.physical_metric())?;
    let target_source_before_in_leaf = target_source_before.checked_sub(target.prefix.metric)?;
    let target_projection_runs_before = projection_runs_before_located_event(arena, target)?;

    let cut_event_ordinal = accepted_event_cut
        .checked_sub(1)
        .ok_or(SerializedGreenError::StaleCursor)?;
    let cut = locate_event_leaf_with_prefix(arena, old_root, cut_event_ordinal, receipt)?;
    receipt.sequence_nodes_visited =
        receipt
            .sequence_nodes_visited
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "retained restart canonical sequence-node receipt",
            ))?;
    let (cut_leaf_summary, cut_kind) = sequence_node::<SerializedGreenSpec>(arena, cut.leaf)?;
    if !matches!(cut_kind, SequenceNodeKind::Leaf)
        || cut.event_ordinal_in_leaf + 1 != cut_leaf_summary.tokens
    {
        return Err(SerializedGreenError::Invalid(
            "retained restart checkpoint is not a final leaf boundary",
        ));
    }
    let prefix_summary = cut.prefix.followed_by(cut_leaf_summary)?;
    let cut_leaf_count = cut
        .leaf_index
        .checked_add(1)
        .ok_or(SerializedGreenError::Overflow(
            "retained restart cut leaf count",
        ))?;
    let expected_balance = i64::try_from(path.len())
        .map_err(|_| SerializedGreenError::Overflow("retained restart open depth"))?;
    if target_event_ordinal >= accepted_event_cut
        || target.leaf_index >= cut_leaf_count
        || prefix_summary.tokens != accepted_event_cut
        || prefix_summary.metric != accepted_source_cut
        || prefix_summary.balance != expected_balance
    {
        return Err(SerializedGreenError::Corrupt(
            "canonical retained prefix disagrees with its authenticated path",
        ));
    }

    let mut open_frames = Vec::new();
    open_frames
        .try_reserve_exact(path.len())
        .map_err(|_| SerializedGreenError::Invalid("restored green frame reservation failed"))?;
    for (index, frame) in path.iter().enumerate() {
        open_frames.push(StructuralOpenFrame {
            block: frame.block(),
            kind: if index == terminal_index {
                terminal_kind
            } else {
                frame.kind()
            },
            // A Setext inverse changes the terminal from Heading back to its
            // canonical provisional Paragraph, so the old heading envelope
            // must not cross that retype. Direct frames and unchanged
            // ancestors retain their canonical facts when the persisted path
            // carried them; absent facts remain a fail-closed placeholder for
            // later fact-sensitive suffix reuse.
            facts: if index == terminal_index && terminal_kind != frame.kind() {
                FactsEnvelope::empty()
            } else {
                frame.facts().cloned().unwrap_or_else(FactsEnvelope::empty)
            },
            closed_children: frame.closed_children(),
        });
    }
    let restored_validator = StructuralValidator {
        open_frames,
        coverage_runs: prefix_summary.coverage_runs_for_valid_prefix()?,
        active_terminal: terminal_kind.logical_channel().map(|_| terminal.block()),
        saw_root: true,
        finished_root: false,
    };
    restored_validator.validate_stack_shape()?;

    Ok(ResolvedRetainedParagraphCheckpoint {
        block: terminal.block(),
        terminal_kind,
        old_root,
        cut_leaf_count,
        prefix_summary,
        target_leaf: target.leaf,
        target_leaf_index: target.leaf_index,
        target_event_ordinal,
        target_source_before,
        target_projection_runs_before,
        target_event_ordinal_in_leaf: target.event_ordinal_in_leaf,
        target_source_before_in_leaf,
        target_byte_offset: target_enter.byte_offset,
        restored_validator: Some(restored_validator),
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_setext_checkpoint(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    block: BlockId,
    final_heading: GreenHeadingOpenFacts,
    target_event_ordinal: u64,
    target_source_before: SerializedMetric,
    accepted_event_cut: u64,
    accepted_source_cut: SerializedMetric,
) -> Result<(ResolvedRetainedParagraphCheckpoint, ResolutionReceipt), SerializedGreenError> {
    let manifest_id = document.local_manifest_id(arena)?;
    let (manifest, old_root) = decode_document(arena, manifest_id)?;
    let (plan, receipt) = resolve_setext_checkpoint_at_bound_root(
        arena,
        document.manifest_id(),
        &manifest,
        old_root,
        block,
        final_heading,
        None,
        accepted_event_cut,
        accepted_source_cut,
    )?;
    if plan.target_event_ordinal != target_event_ordinal
        || plan.target_source_before != target_source_before
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    Ok((plan, receipt))
}

/// Resolves the canonical Setext target exclusively from a validated old
/// root, the accepted event cut, and typed parent-selected normalization
/// facts. No caller supplies an Enter ordinal, leaf location, or source-before
/// coordinate.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn resolve_setext_checkpoint_at_bound_root(
    arena: &PageArena,
    manifest_capability: SerializedGreenManifestId,
    manifest: &Manifest,
    old_root: ArenaId,
    block: BlockId,
    final_heading: GreenHeadingOpenFacts,
    accepted_projection_runs: Option<u64>,
    accepted_event_cut: u64,
    accepted_source_cut: SerializedMetric,
) -> Result<(ResolvedRetainedParagraphCheckpoint, ResolutionReceipt), SerializedGreenError> {
    if block.0 == 0
        || final_heading.style() != GreenHeadingStyle::Setext
        || accepted_event_cut == 0
        || accepted_source_cut.is_zero()
        || accepted_source_cut.is_partially_zero()
    {
        return Err(SerializedGreenError::Invalid(
            "invalid Setext normalization checkpoint request",
        ));
    }
    let mut receipt = ResolutionReceipt::default();
    let restart = restart_output_at_bound_root(
        arena,
        manifest_capability,
        manifest,
        old_root,
        accepted_event_cut,
    )?;
    receipt.absorb_restart_output(restart.receipt())?;
    let path = restart.frames();
    let terminal = path.last().ok_or(SerializedGreenError::StaleCursor)?;
    if restart.source_metric() != accepted_source_cut
        || accepted_projection_runs
            .is_some_and(|projection_runs| restart.coverage_count() != projection_runs)
        || terminal.block() != block
        || terminal.kind() != GreenKind::HEADING
        || GreenHeadingOpenFacts::try_from_envelope(terminal.facts())? != final_heading
    {
        return Err(SerializedGreenError::StaleCursor);
    }
    let plan = resolve_retained_checkpoint_from_path(
        arena,
        manifest_capability,
        old_root,
        path,
        accepted_event_cut,
        accepted_source_cut,
        RetainedGreenTransform::InvertSetext(final_heading),
        &mut receipt,
    )?;
    Ok((plan, receipt))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetextRetainedGreenRestartProgress {
    Pending,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartPhase {
    RetainingPrefix,
    AllocatingInverseLeaf,
    SplicingInverseLeaf,
    InstallingPrefix,
    Ready,
    Taken,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetainedGreenTransform {
    /// The committed parent already contains the provisional Paragraph shape.
    Direct,
    /// The committed parent finalized the provisional Paragraph as Setext and
    /// must contract that exact Enter before suffix parsing resumes.
    InvertSetext(GreenHeadingOpenFacts),
}

/// Internal authority mode for the fuelled cross-build storage core. Legacy
/// tests hold the committed document borrow directly; production encloses the
/// marker inside an owner of the complete branded parent-adoption lease.
#[derive(Debug)]
enum RetainedSetextGreenAuthority<'old> {
    LegacyDocument {
        document: &'old SerializedGreenDocument,
        binding: SerializedGreenManifestDescriptor,
    },
    #[cfg(feature = "exact-parser")]
    /// Marker used only inside `ParentSelectedSetextRetainedGreenRestart`.
    /// The outer production wrapper owns and revalidates the branded parent
    /// lease before every inner poll, avoiding a self-referential borrow.
    ParentSelected,
}

impl RetainedSetextGreenAuthority<'_> {
    fn validate_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), SerializedGreenError> {
        match self {
            Self::LegacyDocument { document, .. } => {
                document.local_manifest_id(session.arena()).map(|_| ())
            }
            #[cfg(feature = "exact-parser")]
            Self::ParentSelected => Ok(()),
        }
    }

    fn legacy_binding(&self) -> Result<SerializedGreenManifestDescriptor, SerializedGreenError> {
        match self {
            Self::LegacyDocument { binding, .. } => Ok(*binding),
            #[cfg(feature = "exact-parser")]
            Self::ParentSelected => Err(SerializedGreenError::Invalid(
                "parent-retained Setext restart requires its parent output seam",
            )),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SetextRetainedGreenRestart<'old> {
    old_authority: RetainedSetextGreenAuthority<'old>,
    plan: ResolvedRetainedParagraphCheckpoint,
    block: BlockId,
    transform: RetainedGreenTransform,
    retained_range: ResumableSequenceRetainedRange<SerializedGreenSpec>,
    retained_root: Option<ArenaBuildOwner>,
    builder: Option<ResumableSerializedGreenBuild>,
    restored_validator: Option<StructuralValidator>,
    receipt: SetextRetainedGreenRestartReceipt,
    phase: RestartPhase,
}

impl<'old> SetextRetainedGreenRestart<'old> {
    /// Revalidates the sealed old binding and canonical coordinates before any
    /// build-owned page exists. The new root spec is checked only for storage
    /// compatibility; source-lineage authority remains outside this module.
    #[allow(clippy::needless_pass_by_value)] // Consuming the sealed manifest prevents authority replay.
    pub(crate) fn try_new(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        old_document: &'old SerializedGreenDocument,
        manifest: SealedSetextNormalizationManifest,
        new_spec: SerializedGreenRootSpec,
    ) -> Result<Self, SerializedGreenError> {
        validate_root_spec(&new_spec)?;
        let old_manifest_id = old_document.local_manifest_id(arena)?;
        let (old_manifest, _) = decode_document(arena, old_manifest_id)?;
        if !manifest
            .old_binding
            .matches(old_document.manifest_id(), &old_manifest)
        {
            return Err(SerializedGreenError::StaleCursor);
        }
        if new_spec.syntax_profile != manifest.old_binding.syntax_profile
            || new_spec.grammar_revision != manifest.old_binding.grammar_revision
            || new_spec.parse_generation.0 <= manifest.old_binding.parse_generation.0
            || new_spec.semantic_epoch <= manifest.old_binding.semantic_epoch
            || new_spec.source_revision.0 <= manifest.old_binding.source_revision.0
            || new_spec.source_root == manifest.old_binding.source_root
            || new_spec.source_bytes < manifest.accepted_source_cut.bytes
            || new_spec.source_utf16 < manifest.accepted_source_cut.utf16
        {
            return Err(SerializedGreenError::Invalid(
                "new green spec is incompatible with the sealed Setext prefix",
            ));
        }
        let (plan, resolution_receipt) = resolve_setext_checkpoint(
            old_document,
            arena,
            manifest.block,
            manifest.final_heading,
            manifest.target_event_ordinal,
            manifest.target_source_before,
            manifest.accepted_event_cut,
            manifest.accepted_source_cut,
        )?;
        if plan.target_event_ordinal != manifest.target_event_ordinal
            || plan.target_source_before != manifest.target_source_before
        {
            return Err(SerializedGreenError::StaleCursor);
        }

        let mut receipt = SetextRetainedGreenRestartReceipt::default();
        receipt.absorb_resolution(manifest.capture_receipt);
        receipt.absorb_resolution(resolution_receipt);
        Self::begin_resolved(
            ticket,
            arena,
            new_spec,
            RetainedSetextGreenAuthority::LegacyDocument {
                document: old_document,
                binding: manifest.old_binding,
            },
            plan,
            manifest.block,
            RetainedGreenTransform::InvertSetext(manifest.final_heading),
            receipt,
        )
    }

    /// Shared journal initializer after one authority-specific canonical
    /// resolution. It stores no raw parent ID; the persistent-range job owns
    /// only its bounded traversal state and the enclosing authority keeps the
    /// old root alive until retention has entered the fresh journal.
    #[allow(clippy::too_many_arguments)]
    fn begin_resolved(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        new_spec: SerializedGreenRootSpec,
        old_authority: RetainedSetextGreenAuthority<'old>,
        mut plan: ResolvedRetainedParagraphCheckpoint,
        block: BlockId,
        transform: RetainedGreenTransform,
        mut receipt: SetextRetainedGreenRestartReceipt,
    ) -> Result<Self, SerializedGreenError> {
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let retained_range = ResumableSequenceRetainedRange::<SerializedGreenSpec>::try_new(
            ticket,
            arena,
            plan.old_root,
            0..plan.cut_leaf_count,
            &mut sequence_receipt,
        )?;
        let mut builder = ResumableSerializedGreenBuild::new(ticket, new_spec)?;
        merge_sequence_receipt(&mut builder.receipt, sequence_receipt);
        let restored_validator =
            plan.restored_validator
                .take()
                .ok_or(SerializedGreenError::Corrupt(
                    "resolved retained restart lost its structural validator",
                ))?;
        receipt.retained_leaves = plan.cut_leaf_count;
        Ok(Self {
            old_authority,
            plan,
            block,
            transform,
            retained_range,
            retained_root: None,
            builder: Some(builder),
            restored_validator: Some(restored_validator),
            receipt,
            phase: RestartPhase::RetainingPrefix,
        })
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> SetextRetainedGreenRestartReceipt {
        self.receipt
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SetextRetainedGreenRestartProgress, SerializedGreenError> {
        let phase = std::mem::replace(&mut self.phase, RestartPhase::Failed);
        let result = self.poll_phase(session, phase);
        match result {
            Ok((next, progress)) => {
                self.phase = next;
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    #[allow(clippy::too_many_lines)] // Explicit phases mirror the resumable journal transitions under audit.
    fn poll_phase(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        phase: RestartPhase,
    ) -> Result<(RestartPhase, SetextRetainedGreenRestartProgress), SerializedGreenError> {
        let builder = self.builder.as_mut().ok_or(SerializedGreenError::Corrupt(
            "Setext restart lost its green builder",
        ))?;
        if session.id() != builder.build_id() {
            return Err(SerializedGreenError::Invalid(
                "Setext restart belongs to another build generation",
            ));
        }
        match phase {
            RestartPhase::RetainingPrefix => {
                let progress = self
                    .retained_range
                    .poll(session, &mut builder.sequence_receipt)?;
                if progress == ResumableSequenceSplitProgress::Pending {
                    return Ok((
                        RestartPhase::RetainingPrefix,
                        SetextRetainedGreenRestartProgress::Pending,
                    ));
                }
                self.retained_root = self.retained_range.take_root()?;
                if self.retained_root.is_none() {
                    return Err(SerializedGreenError::Corrupt(
                        "nonempty Setext prefix retained no root",
                    ));
                }
                let next = match self.transform {
                    RetainedGreenTransform::Direct => RestartPhase::InstallingPrefix,
                    RetainedGreenTransform::InvertSetext(_) => RestartPhase::AllocatingInverseLeaf,
                };
                Ok((next, SetextRetainedGreenRestartProgress::Pending))
            }
            RestartPhase::AllocatingInverseLeaf => {
                // Revalidate either the legacy committed document or the
                // exact branded parent-retained owner before transferring any
                // child edge into the new prefix.
                self.old_authority.validate_session(session)?;
                let RetainedGreenTransform::InvertSetext(final_heading) = self.transform else {
                    return Err(SerializedGreenError::Corrupt(
                        "direct retained green entered the Setext inverse phase",
                    ));
                };
                let (old_summary, replacement_summary) = prepare_setext_inverse_leaf(
                    session.arena(),
                    self.plan.target_leaf,
                    self.plan.target_event_ordinal_in_leaf,
                    self.plan.target_byte_offset,
                    self.block,
                    final_heading,
                    &mut builder.setext_scratch,
                )?;
                if !old_summary.same_semantics(replacement_summary)
                    || replacement_summary.leaves != 1
                {
                    return Err(SerializedGreenError::Corrupt(
                        "Setext inverse changed retained leaf semantics",
                    ));
                }
                let page = &builder.setext_scratch.pages[0];
                page.require_fixed_capacity()?;
                if !page.sealed || builder.setext_scratch.page_count != 1 {
                    return Err(SerializedGreenError::Corrupt(
                        "Setext inverse did not produce exactly one sealed leaf",
                    ));
                }
                let (replacement, allocation) =
                    session.allocate_packed(&page.bytes, &page.programs)?;
                builder.receipt.leaf_pages_allocated += 1;
                builder.receipt.resumable_arena_allocations += 1;
                builder.receipt.payload_bytes_copied += allocation.payload_bytes_copied;
                builder.receipt.edge_bytes_copied += allocation.edge_bytes_copied;
                self.receipt.inverse_leaf_pages_allocated += 1;
                let retained = self
                    .retained_root
                    .take()
                    .ok_or(SerializedGreenError::Corrupt(
                        "Setext restart lost its retained prefix",
                    ))?;
                builder.begin_canonical_leaf_replacement(
                    session,
                    retained,
                    self.plan.target_leaf_index..self.plan.target_leaf_index + 1,
                    replacement,
                )?;
                Ok((
                    RestartPhase::SplicingInverseLeaf,
                    SetextRetainedGreenRestartProgress::Pending,
                ))
            }
            RestartPhase::SplicingInverseLeaf => {
                if builder
                    .splice
                    .poll(session, &mut builder.sequence_receipt)?
                    == ResumableSequenceSplitProgress::Pending
                {
                    return Ok((
                        RestartPhase::SplicingInverseLeaf,
                        SetextRetainedGreenRestartProgress::Pending,
                    ));
                }
                Ok((
                    RestartPhase::InstallingPrefix,
                    SetextRetainedGreenRestartProgress::Pending,
                ))
            }
            RestartPhase::InstallingPrefix => {
                let root =
                    match self.transform {
                        RetainedGreenTransform::Direct => {
                            self.retained_root
                                .take()
                                .ok_or(SerializedGreenError::Corrupt(
                                    "direct retained green lost its prefix root",
                                ))?
                        }
                        RetainedGreenTransform::InvertSetext(_) => builder
                            .splice
                            .take_root()?
                            .ok_or(SerializedGreenError::Corrupt(
                                "Setext inverse splice produced no root",
                            ))?,
                    };
                let summary = sequence_node::<SerializedGreenSpec>(
                    session.arena(),
                    session.owner_id(&root)?,
                )?
                .0;
                if summary.leaves != self.plan.prefix_summary.leaves
                    || !summary.same_semantics(self.plan.prefix_summary)
                    || summary.tokens != self.plan.prefix_summary.tokens
                    || summary.metric != self.plan.prefix_summary.metric
                {
                    return Err(SerializedGreenError::Corrupt(
                        "Setext inverse splice changed canonical prefix coordinates",
                    ));
                }
                builder.sealed_leaves = summary.leaves;
                builder.sealed_events = summary.tokens;
                builder.sealed_metric = summary.metric;
                builder.install_working_prefix(session, root, summary)?;
                builder.validator =
                    self.restored_validator
                        .take()
                        .ok_or(SerializedGreenError::Corrupt(
                            "Setext restart lost its restored validator",
                        ))?;
                if self.plan.terminal_kind == GreenKind::PARAGRAPH {
                    let generation = builder.next_provisional_generation;
                    builder.next_provisional_generation =
                        generation
                            .checked_add(1)
                            .ok_or(SerializedGreenError::Overflow(
                                "restored provisional generation",
                            ))?;
                    let active = ActiveProvisionalParagraph {
                        build: builder.build,
                        block: self.block,
                        generation,
                        event_ordinal: self.plan.target_event_ordinal,
                        source_before: self.plan.target_source_before,
                        storage: ProvisionalParagraphStorage::Sealed {
                            leaf_index: self.plan.target_leaf_index,
                            byte_offset: self.plan.target_byte_offset,
                            event_ordinal_in_leaf: self.plan.target_event_ordinal_in_leaf,
                            source_before_in_leaf: self.plan.target_source_before_in_leaf,
                        },
                    };
                    builder.active_provisional_paragraph = Some(active);
                    builder.ready_provisional_paragraph = Some(ProvisionalParagraphEnter {
                        build: builder.build,
                        block: self.block,
                        generation,
                        event_ordinal: self.plan.target_event_ordinal,
                        source_before: self.plan.target_source_before,
                    });
                } else {
                    builder.active_provisional_paragraph = None;
                    builder.ready_provisional_paragraph = None;
                }
                builder.setext_scratch.reset()?;
                self.receipt.persistent_sequence_leaves_reused =
                    builder.sequence_receipt.leaves_reused;
                Ok((
                    RestartPhase::Ready,
                    SetextRetainedGreenRestartProgress::Ready,
                ))
            }
            RestartPhase::Ready => Ok((
                RestartPhase::Ready,
                SetextRetainedGreenRestartProgress::Ready,
            )),
            RestartPhase::Taken => Err(SerializedGreenError::Invalid(
                "Setext restart output was already taken",
            )),
            RestartPhase::Failed => Err(SerializedGreenError::Invalid(
                "Setext restart is terminally failed",
            )),
        }
    }

    pub(crate) fn take_output(
        mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<SetextRetainedGreenRestartOutput, SerializedGreenError> {
        let old_binding = self.old_authority.legacy_binding()?;
        let (builder, provisional, line_cut) = self.take_ready_green_parts(session)?;
        let provisional = provisional.ok_or(SerializedGreenError::Corrupt(
            "legacy Setext restart did not restore a provisional Paragraph",
        ))?;
        Ok(SetextRetainedGreenRestartOutput {
            builder,
            provisional,
            old_binding,
            line_cut,
            receipt: self.receipt,
        })
    }

    fn take_ready_green_parts(
        &mut self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<
        (
            ResumableSerializedGreenBuild,
            Option<ProvisionalParagraphEnter>,
            SerializedGreenLeafCut,
        ),
        SerializedGreenError,
    > {
        if self.phase != RestartPhase::Ready {
            return Err(SerializedGreenError::Invalid(
                "Setext restart output is not ready",
            ));
        }
        let mut builder = self.builder.take().ok_or(SerializedGreenError::Corrupt(
            "Setext restart lost its ready builder",
        ))?;
        let provisional = if self.plan.terminal_kind == GreenKind::PARAGRAPH {
            Some(builder.take_provisional_paragraph_enter(session, self.block)?)
        } else {
            None
        };
        let line_cut = builder.take_natural_line_boundary_cut(session)?;
        self.phase = RestartPhase::Taken;
        Ok((builder, provisional, line_cut))
    }
}

#[derive(Debug)]
pub(crate) struct SetextRetainedGreenRestartOutput {
    builder: ResumableSerializedGreenBuild,
    provisional: ProvisionalParagraphEnter,
    old_binding: SerializedGreenManifestDescriptor,
    line_cut: SerializedGreenLeafCut,
    receipt: SetextRetainedGreenRestartReceipt,
}

impl SetextRetainedGreenRestartOutput {
    pub(crate) fn into_parts(
        self,
    ) -> (
        ResumableSerializedGreenBuild,
        ProvisionalParagraphEnter,
        SerializedGreenManifestDescriptor,
        SetextRetainedGreenRestartReceipt,
    ) {
        let Self {
            builder,
            provisional,
            old_binding,
            line_cut: _,
            receipt,
        } = self;
        (builder, provisional, old_binding, receipt)
    }

    pub(crate) const fn old_binding(&self) -> SerializedGreenManifestDescriptor {
        self.old_binding
    }

    pub(crate) fn matches_activation(
        &self,
        epoch: LiveCandidateEpoch,
        block: BlockId,
        accepted_source: SerializedMetric,
        old_binding: SerializedGreenManifestDescriptor,
        expected_spec: &SerializedGreenRootSpec,
    ) -> bool {
        self.old_binding == old_binding
            && &self.builder.spec == expected_spec
            && self.builder.build_id() == epoch.build_id()
            && self.line_cut.build_id() == epoch.build_id()
            && self.line_cut.source_before() == accepted_source
            && self.builder.line_boundary_cut_is_current(&self.line_cut)
            && self
                .builder
                .retained_provisional_matches(&self.provisional, block)
    }

    pub(crate) fn into_activation_parts(
        self,
    ) -> (
        ResumableSerializedGreenBuild,
        ProvisionalParagraphEnter,
        SerializedGreenManifestDescriptor,
        SerializedGreenLeafCut,
        SetextRetainedGreenRestartReceipt,
    ) {
        (
            self.builder,
            self.provisional,
            self.old_binding,
            self.line_cut,
            self.receipt,
        )
    }
}

/// Error boundary for the production parent-retained Setext inverse. Parent
/// identity failures remain distinguishable from bounded green-codec failures
/// so the actor can classify a stale parent separately from candidate damage.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum ParentSelectedSetextRetainedGreenRestartError {
    Parent(RestartCompositeDocumentError),
    Green(SerializedGreenError),
}

#[cfg(feature = "exact-parser")]
impl From<RestartCompositeDocumentError> for ParentSelectedSetextRetainedGreenRestartError {
    fn from(error: RestartCompositeDocumentError) -> Self {
        Self::Parent(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<SerializedGreenError> for ParentSelectedSetextRetainedGreenRestartError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

#[cfg(feature = "exact-parser")]
impl std::fmt::Display for ParentSelectedSetextRetainedGreenRestartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parent(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
        }
    }
}

#[cfg(feature = "exact-parser")]
impl std::error::Error for ParentSelectedSetextRetainedGreenRestartError {}

/// Production-shaped retained-green inverse. The wrapper owns the complete
/// branded parent lease rather than borrowing one of its child handles, so it
/// can live directly inside an actor job without a self-reference. Every poll
/// revalidates the exact actor activation and both retained child owners.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedSetextRetainedGreenRestart {
    lease: ParentSelectedRestartCompositeAdoptionLease,
    core: SetextRetainedGreenRestart<'static>,
    fragment_origin: ParentSelectedCanonicalFragmentOriginSeed,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSetextRetainedGreenRestart {
    /// Consumes the parent-selected normalization carrier and derives all
    /// canonical green coordinates internally. The accepted prefix is the
    /// checkpoint cut immediately before its authenticated deferred LF; no
    /// caller supplies an Enter ordinal, source coordinate, leaf, or root.
    pub(crate) fn try_new(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        lease: ParentSelectedRestartCompositeAdoptionLease,
        authority: ParentSelectedSetextGreenInverseAuthority,
        new_spec: SerializedGreenRootSpec,
    ) -> Result<Self, ParentSelectedSetextRetainedGreenRestartError> {
        validate_root_spec(&new_spec)?;
        let descriptor = lease.validated_suspended_green_for_restart(ticket, arena)?;
        let (path, checkpoint_cut, block, final_heading) =
            authority.into_parent_retained_restart_parts();
        let accepted_source_cut = SerializedMetric {
            bytes: checkpoint_cut.source_bytes().checked_sub(1).ok_or(
                SerializedGreenError::Invalid(
                    "Setext checkpoint has no deferred LF byte to invert",
                ),
            )?,
            utf16: checkpoint_cut.source_utf16().checked_sub(1).ok_or(
                SerializedGreenError::Invalid(
                    "Setext checkpoint has no deferred LF UTF-16 unit to invert",
                ),
            )?,
        };
        if checkpoint_cut.physical_lines() == 0
            || checkpoint_cut.green_events() == 0
            || checkpoint_cut.projection_runs() == 0
            || accepted_source_cut.is_zero()
            || accepted_source_cut.is_partially_zero()
            || final_heading.style() != GreenHeadingStyle::Setext
            || path.source_metric() != accepted_source_cut
            || path.event_cut() != checkpoint_cut.green_events()
            || path.coverage_count() != checkpoint_cut.projection_runs()
        {
            return Err(
                SerializedGreenError::Invalid("invalid parent-selected Setext checkpoint").into(),
            );
        }
        if new_spec.syntax_profile != descriptor.syntax_profile()
            || new_spec.grammar_revision != descriptor.grammar_revision()
            || new_spec.parse_generation.0 <= descriptor.parse_generation().0
            || new_spec.semantic_epoch <= descriptor.semantic_epoch()
            || new_spec.source_revision.0 <= descriptor.source_revision().0
            || new_spec.source_root == descriptor.source_root()
            || new_spec.source_bytes < accepted_source_cut.bytes
            || new_spec.source_utf16 < accepted_source_cut.utf16
        {
            return Err(SerializedGreenError::Invalid(
                "new green spec is incompatible with the parent-selected Setext prefix",
            )
            .into());
        }

        // The storage parent has just revalidated this descriptor and both
        // owner handles. Decode through that typed descriptor only; raw arena
        // roots never cross the public production seam.
        let (_old_manifest, old_root) = decode_document(arena, descriptor.manifest)?;
        if old_root != descriptor.sequence_root
            || validate_serialized_green_composite_child(arena, descriptor.manifest)? != descriptor
        {
            return Err(SerializedGreenError::Corrupt(
                "parent-selected green descriptor changed during Setext admission",
            )
            .into());
        }
        let manifest_capability = SerializedGreenManifestId::new(
            arena
                .scoped_query_id(descriptor.manifest)
                .map_err(SerializedGreenError::from)?,
        );
        let terminal = path
            .frames()
            .last()
            .ok_or(SerializedGreenError::StaleCursor)?;
        if path.manifest() != manifest_capability
            || terminal.block() != block
            || terminal.green_kind() != GreenKind::HEADING
            || terminal
                .normalization()
                .map(|normalization| normalization.role())
                != Some(CurrentRestartNormalizationRole::SetextHeadingToProvisionalParagraph)
        {
            return Err(SerializedGreenError::StaleCursor.into());
        }
        let mut resolution_receipt = ResolutionReceipt::default();
        resolution_receipt.absorb_restart_output(path.query_receipt())?;
        let plan = resolve_retained_checkpoint_from_path(
            arena,
            manifest_capability,
            old_root,
            path.frames(),
            path.event_cut(),
            accepted_source_cut,
            RetainedGreenTransform::InvertSetext(final_heading),
            &mut resolution_receipt,
        )?;
        let fragment_origin = ParentSelectedCanonicalFragmentOriginSeed::from_resolved_plan(
            manifest_capability,
            ticket.id(),
            path.event_cut(),
            accepted_source_cut,
            path.coverage_count(),
            &plan,
        )?;
        #[cfg(feature = "host-mirror-probe")]
        let retained_host_prefix =
            crate::host_mirror::CanonicalRetainedGreenPrefixSeed::from_retained_restart(
                HostRetainedPrefixMint(()),
                ticket.id(),
                manifest_capability.scoped(),
                descriptor.source_revision(),
                descriptor.source_root(),
                new_spec.source_revision,
                new_spec.source_root,
                new_spec.grammar_revision,
                new_spec.parse_generation,
                plan.cut_leaf_count,
            )?;
        let mut receipt = SetextRetainedGreenRestartReceipt::default();
        receipt.absorb_resolution(resolution_receipt);
        let mut core = SetextRetainedGreenRestart::<'static>::begin_resolved(
            ticket,
            arena,
            new_spec,
            RetainedSetextGreenAuthority::ParentSelected,
            plan,
            block,
            RetainedGreenTransform::InvertSetext(final_heading),
            receipt,
        )?;
        #[cfg(feature = "host-mirror-probe")]
        {
            core.builder
                .as_mut()
                .ok_or(SerializedGreenError::Corrupt(
                    "Setext retained restart lost its builder before host provenance install",
                ))?
                .retained_host_prefix = Some(retained_host_prefix);
        }
        Ok(Self {
            lease,
            core,
            fragment_origin,
        })
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> SetextRetainedGreenRestartReceipt {
        self.core.receipt()
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SetextRetainedGreenRestartProgress, ParentSelectedSetextRetainedGreenRestartError>
    {
        self.lease.revalidate_green_for_restart(session)?;
        self.core.poll(session).map_err(Into::into)
    }

    pub(crate) fn take_output(
        self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<
        ParentSelectedSetextRetainedGreenRestartOutput,
        ParentSelectedSetextRetainedGreenRestartError,
    > {
        let Self {
            lease,
            mut core,
            fragment_origin,
        } = self;
        lease.revalidate_green_for_restart(session)?;
        if !matches!(
            &core.old_authority,
            RetainedSetextGreenAuthority::ParentSelected
        ) {
            return Err(SerializedGreenError::Corrupt(
                "parent-selected Setext wrapper lost its authority marker",
            )
            .into());
        }
        let receipt = core.receipt;
        let (builder, provisional, line_cut) = core.take_ready_green_parts(session)?;
        let provisional = provisional.ok_or(SerializedGreenError::Corrupt(
            "parent-selected Setext restart did not restore a provisional Paragraph",
        ))?;
        Ok(ParentSelectedSetextRetainedGreenRestartOutput {
            lease,
            builder,
            provisional,
            line_cut,
            receipt,
            fragment_origin,
        })
    }
}

/// Linear production output. The same branded parent lease is returned so
/// checkpoint-index splice and final composite adoption remain joined to the
/// exact parent which authorized this green inverse.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedSetextRetainedGreenRestartOutput {
    lease: ParentSelectedRestartCompositeAdoptionLease,
    builder: ResumableSerializedGreenBuild,
    provisional: ProvisionalParagraphEnter,
    line_cut: SerializedGreenLeafCut,
    receipt: SetextRetainedGreenRestartReceipt,
    fragment_origin: ParentSelectedCanonicalFragmentOriginSeed,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSetextRetainedGreenRestartOutput {
    /// Consumes the complete parent-selected output directly into the sole
    /// CandidateWriter green-ready mint. No lease/builder/cut tuple exists at
    /// crate scope.
    pub(crate) fn into_parent_selected_candidate_ready(
        self,
        epoch: LiveCandidateEpoch,
        coverage: crate::ParentSelectedComposerCoverage,
    ) -> Result<ParentSelectedCandidateWriterGreenReady, CandidateWriterError> {
        let block = self.provisional.block;
        ParentSelectedCandidateWriterGreenReady::try_from_parent_green_mint(
            ParentSelectedCandidateGreenReadyMint(()),
            epoch,
            self.lease,
            self.builder,
            Some(self.provisional),
            self.line_cut,
            self.receipt,
            coverage,
            Some(self.fragment_origin),
            block,
            GreenKind::PARAGRAPH,
        )
    }

    #[cfg(test)]
    pub(crate) const fn build_id_for_test(&self) -> crate::ArenaBuildId {
        self.builder.build_id()
    }

    #[cfg(test)]
    pub(crate) const fn source_before_for_test(&self) -> SerializedMetric {
        self.line_cut.source_before()
    }

    #[cfg(test)]
    pub(crate) const fn receipt_for_test(&self) -> SetextRetainedGreenRestartReceipt {
        self.receipt
    }

    #[cfg(test)]
    pub(crate) fn revalidate_parent_for_test(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), RestartCompositeDocumentError> {
        self.lease.revalidate_green_for_restart(session).map(|_| ())
    }

    #[cfg(test)]
    pub(crate) fn validate_pristine_parent_for_test(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), RestartCompositeDocumentError> {
        self.lease.validate_session(session)
    }
}

/// Parent-retained Direct prefix restoration. The wrapper owns both the exact
/// branded parent lease and the complete authenticated current-green path;
/// callers cannot synthesize it from bindings or copied cut coordinates.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedDirectRetainedGreenRestart {
    lease: ParentSelectedRestartCompositeAdoptionLease,
    core: SetextRetainedGreenRestart<'static>,
    fragment_origin: Option<ParentSelectedCanonicalFragmentOriginSeed>,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum ParentSelectedDirectRetainedGreenRestartError {
    Parent(RestartCompositeDocumentError),
    Green(SerializedGreenError),
}

#[cfg(feature = "exact-parser")]
impl From<RestartCompositeDocumentError> for ParentSelectedDirectRetainedGreenRestartError {
    fn from(error: RestartCompositeDocumentError) -> Self {
        Self::Parent(error)
    }
}

#[cfg(feature = "exact-parser")]
impl From<SerializedGreenError> for ParentSelectedDirectRetainedGreenRestartError {
    fn from(error: SerializedGreenError) -> Self {
        Self::Green(error)
    }
}

#[cfg(feature = "exact-parser")]
impl std::fmt::Display for ParentSelectedDirectRetainedGreenRestartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parent(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
        }
    }
}

#[cfg(feature = "exact-parser")]
impl std::error::Error for ParentSelectedDirectRetainedGreenRestartError {}

#[cfg(feature = "exact-parser")]
impl ParentSelectedDirectRetainedGreenRestart {
    /// Consumes the whole selected Direct path. Admission cross-checks its
    /// arena-scoped manifest against the descriptor returned by the branded
    /// lease before starting retained-range work.
    pub(crate) fn try_new(
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        lease: ParentSelectedRestartCompositeAdoptionLease,
        authority: ParentSelectedDirectGreenRestartAuthority,
        new_spec: SerializedGreenRootSpec,
    ) -> Result<Self, ParentSelectedDirectRetainedGreenRestartError> {
        validate_root_spec(&new_spec)?;
        let descriptor = lease.validated_suspended_green_for_restart(ticket, arena)?;
        let path = authority.into_current_restart_path();
        let accepted_source_cut = path.source_metric();
        if path.event_cut() == 0
            || path.coverage_count() == 0
            || accepted_source_cut.is_zero()
            || accepted_source_cut.is_partially_zero()
        {
            return Err(
                SerializedGreenError::Invalid("invalid parent-selected Direct checkpoint").into(),
            );
        }
        if new_spec.syntax_profile != descriptor.syntax_profile()
            || new_spec.grammar_revision != descriptor.grammar_revision()
            || new_spec.parse_generation.0 <= descriptor.parse_generation().0
            || new_spec.semantic_epoch <= descriptor.semantic_epoch()
            || new_spec.source_revision.0 <= descriptor.source_revision().0
            || new_spec.source_root == descriptor.source_root()
            || new_spec.source_bytes < accepted_source_cut.bytes
            || new_spec.source_utf16 < accepted_source_cut.utf16
        {
            return Err(SerializedGreenError::Invalid(
                "new green spec is incompatible with the parent-selected Direct prefix",
            )
            .into());
        }

        let (_old_manifest, old_root) = decode_document(arena, descriptor.manifest)?;
        if old_root != descriptor.sequence_root
            || validate_serialized_green_composite_child(arena, descriptor.manifest)? != descriptor
        {
            return Err(SerializedGreenError::Corrupt(
                "parent-selected green descriptor changed during Direct admission",
            )
            .into());
        }
        let manifest_capability = SerializedGreenManifestId::new(
            arena
                .scoped_query_id(descriptor.manifest)
                .map_err(SerializedGreenError::from)?,
        );
        if path.manifest() != manifest_capability {
            return Err(SerializedGreenError::StaleCursor.into());
        }

        let mut resolution_receipt = ResolutionReceipt::default();
        resolution_receipt.absorb_restart_output(path.query_receipt())?;
        let plan = resolve_retained_checkpoint_from_path(
            arena,
            manifest_capability,
            old_root,
            path.frames(),
            path.event_cut(),
            accepted_source_cut,
            RetainedGreenTransform::Direct,
            &mut resolution_receipt,
        )?;
        let fragment_origin = if plan.terminal_kind == GreenKind::PARAGRAPH {
            Some(
                ParentSelectedCanonicalFragmentOriginSeed::from_resolved_plan(
                    manifest_capability,
                    ticket.id(),
                    path.event_cut(),
                    accepted_source_cut,
                    path.coverage_count(),
                    &plan,
                )?,
            )
        } else {
            None
        };
        #[cfg(feature = "host-mirror-probe")]
        let retained_host_prefix =
            crate::host_mirror::CanonicalRetainedGreenPrefixSeed::from_retained_restart(
                HostRetainedPrefixMint(()),
                ticket.id(),
                manifest_capability.scoped(),
                descriptor.source_revision(),
                descriptor.source_root(),
                new_spec.source_revision,
                new_spec.source_root,
                new_spec.grammar_revision,
                new_spec.parse_generation,
                plan.cut_leaf_count,
            )?;
        let block = plan.block;
        let mut receipt = SetextRetainedGreenRestartReceipt::default();
        receipt.absorb_resolution(resolution_receipt);
        let mut core = SetextRetainedGreenRestart::<'static>::begin_resolved(
            ticket,
            arena,
            new_spec,
            RetainedSetextGreenAuthority::ParentSelected,
            plan,
            block,
            RetainedGreenTransform::Direct,
            receipt,
        )?;
        #[cfg(feature = "host-mirror-probe")]
        {
            core.builder
                .as_mut()
                .ok_or(SerializedGreenError::Corrupt(
                    "Direct retained restart lost its builder before host provenance install",
                ))?
                .retained_host_prefix = Some(retained_host_prefix);
        }
        Ok(Self {
            lease,
            core,
            fragment_origin,
        })
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> SetextRetainedGreenRestartReceipt {
        self.core.receipt()
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<SetextRetainedGreenRestartProgress, ParentSelectedDirectRetainedGreenRestartError>
    {
        self.lease.revalidate_green_for_restart(session)?;
        self.core.poll(session).map_err(Into::into)
    }

    pub(crate) fn take_output(
        self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<
        ParentSelectedDirectRetainedGreenRestartOutput,
        ParentSelectedDirectRetainedGreenRestartError,
    > {
        let Self {
            lease,
            mut core,
            fragment_origin,
        } = self;
        lease.revalidate_green_for_restart(session)?;
        if !matches!(
            &core.old_authority,
            RetainedSetextGreenAuthority::ParentSelected
        ) {
            return Err(SerializedGreenError::Corrupt(
                "parent-selected Direct wrapper lost its authority marker",
            )
            .into());
        }
        let terminal_block = core.plan.block;
        let terminal_kind = core.plan.terminal_kind;
        let receipt = core.receipt;
        let (builder, provisional, line_cut) = core.take_ready_green_parts(session)?;
        Ok(ParentSelectedDirectRetainedGreenRestartOutput {
            lease,
            builder,
            provisional,
            line_cut,
            receipt,
            terminal_block,
            terminal_kind,
            fragment_origin,
        })
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedDirectRetainedGreenRestartOutput {
    lease: ParentSelectedRestartCompositeAdoptionLease,
    builder: ResumableSerializedGreenBuild,
    provisional: Option<ProvisionalParagraphEnter>,
    line_cut: SerializedGreenLeafCut,
    receipt: SetextRetainedGreenRestartReceipt,
    terminal_block: BlockId,
    terminal_kind: GreenKind,
    fragment_origin: Option<ParentSelectedCanonicalFragmentOriginSeed>,
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedDirectRetainedGreenRestartOutput {
    /// The sole consuming seam from Direct retained storage into CandidateWriter.
    pub(crate) fn into_parent_selected_candidate_ready(
        self,
        epoch: LiveCandidateEpoch,
        coverage: crate::ParentSelectedComposerCoverage,
    ) -> Result<ParentSelectedCandidateWriterGreenReady, CandidateWriterError> {
        ParentSelectedCandidateWriterGreenReady::try_from_parent_green_mint(
            ParentSelectedCandidateGreenReadyMint(()),
            epoch,
            self.lease,
            self.builder,
            self.provisional,
            self.line_cut,
            self.receipt,
            coverage,
            self.fragment_origin,
            self.terminal_block,
            self.terminal_kind,
        )
    }

    #[cfg(test)]
    pub(crate) const fn terminal_for_test(&self) -> (BlockId, GreenKind) {
        (self.terminal_block, self.terminal_kind)
    }

    #[cfg(test)]
    pub(crate) const fn has_provisional_for_test(&self) -> bool {
        self.provisional.is_some()
    }
}

/// Bounded inverse of the existing Setext promotion repack. The generic
/// `rewrite_enters` surface remains unchanged and still rejects kind changes.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn prepare_setext_inverse_leaf(
    arena: &PageArena,
    leaf: ArenaId,
    target_event_ordinal: u64,
    target_byte_offset: u16,
    target_block: BlockId,
    expected_heading: GreenHeadingOpenFacts,
    scratch: &mut SetextRepackScratch,
) -> Result<(GreenSummary, GreenSummary), SerializedGreenError> {
    scratch.reset()?;
    let paragraph = encode_event(
        &GreenEvent::enter(target_block, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        0,
    )?;
    if paragraph.program.is_some() || paragraph.program_ordinal_offset.is_some() {
        return Err(SerializedGreenError::Corrupt(
            "Paragraph Enter unexpectedly owns a Program edge",
        ));
    }
    let child_count = arena.packed_child_count(leaf)?;
    let payload = arena.payload(leaf)?;
    let expected = decode_summary(payload, LEAF_TAG)?;
    let mut decoder = Decoder::new(&payload[LEAF_HEADER_BYTES..]);
    let mut actual = GreenSummary::default();
    let mut next_program_ordinal = 0_usize;
    let mut event_ordinal = 0_u64;
    let mut found_target = false;

    while !decoder.is_empty() {
        let start =
            LEAF_HEADER_BYTES
                .checked_add(decoder.cursor)
                .ok_or(SerializedGreenError::Overflow(
                    "Setext inverse decoded event offset",
                ))?;
        let event = decode_event(&mut decoder, arena, leaf, &mut next_program_ordinal)?;
        let end =
            LEAF_HEADER_BYTES
                .checked_add(decoder.cursor)
                .ok_or(SerializedGreenError::Overflow(
                    "Setext inverse decoded event end",
                ))?;
        let raw = payload
            .get(start..end)
            .ok_or(SerializedGreenError::Corrupt(
                "Setext inverse event escapes its leaf",
            ))?;
        let offset = u16::try_from(start)
            .map_err(|_| SerializedGreenError::Corrupt("Setext inverse offset exceeds u16"))?;
        let at_target = event_ordinal == target_event_ordinal;
        let (output, program) = if at_target {
            if offset != target_byte_offset || found_target {
                return Err(SerializedGreenError::StaleCursor);
            }
            let DecodedGreenEventKind::Enter { block, kind, facts } = &event else {
                return Err(SerializedGreenError::StaleCursor);
            };
            if *block != target_block
                || *kind != GreenKind::HEADING
                || GreenHeadingOpenFacts::try_from_envelope(facts)? != expected_heading
                || expected_heading.style() != GreenHeadingStyle::Setext
                || raw.len() != paragraph.bytes.len() + 7
            {
                return Err(SerializedGreenError::StaleCursor);
            }
            found_target = true;
            (paragraph.bytes.as_slice(), None)
        } else {
            let program = match &event {
                DecodedGreenEventKind::Coverage(DecodedSourceProjectionRun {
                    logical_contribution: DecodedLogicalContribution::Program(program),
                    ..
                }) => Some((
                    program.retained_page()?,
                    usize::from(program.encoded_ordinal_offset),
                )),
                DecodedGreenEventKind::Enter { .. }
                | DecodedGreenEventKind::Coverage(_)
                | DecodedGreenEventKind::Exit { .. } => None,
            };
            (raw, program)
        };
        let event_summary = GreenSummary::decoded_event(&event);
        actual = actual.followed_by(event_summary)?;
        if !scratch.pages[0].can_fit(output.len(), program.is_some()) {
            return Err(SerializedGreenError::Corrupt(
                "contracting Setext inverse no longer fits its source leaf",
            ));
        }
        scratch.pages[0].push_raw(output, event_summary, program)?;
        event_ordinal = event_ordinal
            .checked_add(1)
            .ok_or(SerializedGreenError::Overflow(
                "Setext inverse event ordinal",
            ))?;
    }
    if next_program_ordinal != child_count {
        return Err(SerializedGreenError::Corrupt(
            "Setext inverse leaf has an unreferenced Program edge",
        ));
    }
    actual.leaves = 1;
    actual.height = 1;
    if actual != expected || !found_target {
        return Err(SerializedGreenError::StaleCursor);
    }
    scratch.pages[0].seal()?;
    scratch.page_count = 1;
    Ok((expected, scratch.pages[0].summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaBuildAbortReceipt;
    use crate::record_forest::ClosedChildAggregate;
    use crate::{CoverageId, GrammarRevision, ParseGeneration, SourceRevision, SourceRootId};

    const OLD_SOURCE: &str = "lead\nold\n===\n";
    const DELETION_SOURCE: &str = "lead\n\n===\n";
    const OLD_BYTES: u64 = 13;
    const DELETION_BYTES: u64 = 10;
    const TARGET: BlockId = BlockId(20);
    const CAPACITY_CLIFF_RUNS: u64 = 331;
    const CAPACITY_CLIFF_WIDE_RUNS: u64 = 8;

    fn spec(
        source_revision: u64,
        source_root: u64,
        source_bytes: u64,
        parse_generation: u64,
        semantic_epoch: u64,
    ) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 7,
            source_revision: SourceRevision(source_revision),
            source_root: SourceRootId(source_root),
            source_bytes,
            source_utf16: source_bytes,
            grammar_revision: GrammarRevision(11),
            parse_generation: ParseGeneration(parse_generation),
            semantic_epoch,
            known_bytes: 0..source_bytes,
        }
    }

    fn coverage(id: u64, bytes: u64, depth: u32) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::new(CoverageId(id), bytes, bytes, depth, CoveragePart(1)).unwrap(),
        )
    }

    fn identity_coverage(id: u64) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(id),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                TARGET,
                LogicalContribution::Identity,
            )
            .unwrap(),
        )
    }

    fn capacity_cliff_metric() -> SerializedMetric {
        SerializedMetric {
            bytes: CAPACITY_CLIFF_RUNS + CAPACITY_CLIFF_WIDE_RUNS,
            utf16: CAPACITY_CLIFF_RUNS,
        }
    }

    fn capacity_cliff_spec(
        source_revision: u64,
        source_root: u64,
        parse_generation: u64,
        semantic_epoch: u64,
    ) -> SerializedGreenRootSpec {
        let metric = capacity_cliff_metric();
        let mut root_spec = spec(
            source_revision,
            source_root,
            metric.bytes,
            parse_generation,
            semantic_epoch,
        );
        root_spec.source_utf16 = metric.utf16;
        root_spec
    }

    fn capacity_cliff_identity_coverage(id: u64) -> GreenEvent {
        if id <= CAPACITY_CLIFF_WIDE_RUNS {
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(id),
                    2,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    TARGET,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            )
        } else {
            identity_coverage(id)
        }
    }

    fn poll_to_event_boundary(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        loop {
            match builder.poll(session).unwrap() {
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("builder finalized while awaiting an event boundary")
                }
            }
        }
    }

    fn offer(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
        event: GreenEvent,
    ) {
        builder.offer_event(session, event).unwrap();
        poll_to_event_boundary(builder, session);
    }

    fn finalize(
        mut builder: ResumableSerializedGreenBuild,
        mut session: ArenaBuildSession<'_>,
    ) -> SerializedGreenDocument {
        builder.finish_input(&mut session).unwrap();
        loop {
            match builder.poll(&mut session).unwrap() {
                SerializedGreenStreamProgress::ManifestReady => break,
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ReadyForEvent => {
                    panic!("finished builder requested another event")
                }
            }
        }
        builder.take_manifest().unwrap().commit(session).unwrap().0
    }

    fn build_old(arena: &mut PageArena, filler_blocks: usize) -> SerializedGreenDocument {
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, spec(1, 101, OLD_BYTES, 1, 1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        for ordinal in 0..filler_blocks {
            let block = BlockId(1_000 + u64::try_from(ordinal).unwrap());
            offer(
                &mut builder,
                &mut session,
                GreenEvent::enter(block, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
            );
            offer(
                &mut builder,
                &mut session,
                GreenEvent::exit(ClosedChildAggregate::default()),
            );
        }
        builder
            .offer_provisional_paragraph_enter(&mut session, TARGET, FactsEnvelope::empty())
            .unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let provisional = builder
            .take_provisional_paragraph_enter(&session, TARGET)
            .unwrap();
        offer(&mut builder, &mut session, coverage(1, 4, 0));
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let cut = builder.take_leaf_barrier_cut(&session).unwrap();
        assert_eq!(cut.source_before(), SerializedMetric { bytes: 4, utf16: 4 });
        offer(&mut builder, &mut session, coverage(2, 9, 0));
        builder
            .begin_setext_promotion(
                &mut session,
                provisional,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let _ = builder.take_setext_promotion(&session, TARGET).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finalize(builder, session)
    }

    /// Builds a current cut with two nonempty ancestor child folds:
    /// Document has one closed BlockQuote sibling, and the active BlockQuote
    /// has one closed Paragraph before the deepest target.
    fn build_nested_old(arena: &mut PageArena, setext_terminal: bool) -> SerializedGreenDocument {
        const CLOSED_CHILD: BlockId = BlockId(41);
        const ACTIVE_QUOTE: BlockId = BlockId(42);
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, spec(1, 401, 5, 1, 1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(40), GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(ACTIVE_QUOTE, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(CLOSED_CHILD, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(101),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    CLOSED_CHILD,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        let (kind, facts) = if setext_terminal {
            (
                GreenKind::HEADING,
                GreenHeadingOpenFacts::setext(1).unwrap().into_envelope(),
            )
        } else {
            (GreenKind::PARAGRAPH, FactsEnvelope::empty())
        };
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(TARGET, kind, facts),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(102),
                    3,
                    3,
                    0,
                    CoveragePart::CONTENT,
                    TARGET,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let cut = builder.take_leaf_barrier_cut(&session).unwrap();
        assert_eq!(cut.source_before(), SerializedMetric { bytes: 4, utf16: 4 });
        offer(
            &mut builder,
            &mut session,
            GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(103),
                    1,
                    1,
                    0,
                    CoveragePart::TERMINAL,
                    TARGET,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
        );
        for _ in 0..3 {
            offer(
                &mut builder,
                &mut session,
                GreenEvent::exit(ClosedChildAggregate::default()),
            );
        }
        finalize(builder, session)
    }

    /// Storage-level capacity fixture: the checkpoint force-seals one full
    /// Paragraph leaf, then Setext's seven-byte Enter expansion repacks that
    /// same canonical cut into two final leaves.
    fn build_capacity_cliff_old(
        arena: &mut PageArena,
    ) -> (SerializedGreenDocument, SerializedGreenLeafCut) {
        let ticket = arena.begin_build().unwrap();
        let mut builder =
            ResumableSerializedGreenBuild::new(&ticket, capacity_cliff_spec(1, 301, 1, 1)).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        builder
            .offer_provisional_paragraph_enter(&mut session, TARGET, FactsEnvelope::empty())
            .unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let provisional = builder
            .take_provisional_paragraph_enter(&session, TARGET)
            .unwrap();
        for run in 0..CAPACITY_CLIFF_RUNS {
            offer(
                &mut builder,
                &mut session,
                capacity_cliff_identity_coverage(run + 1),
            );
        }
        assert_eq!(builder.leaf.bytes.len(), ARENA_PAGE_BYTES);
        assert!(!builder.partial_setext_can_fit(7));
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let captured = builder.take_leaf_barrier_cut(&session).unwrap();
        assert_eq!(captured.leaves_before(), 1);
        builder
            .begin_setext_promotion(
                &mut session,
                provisional,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let _ = builder.take_setext_promotion(&session, TARGET).unwrap();
        assert_eq!(builder.receipt().setext_replacement_leaf_pages_allocated, 2);
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        (finalize(builder, session), captured)
    }

    fn accepted_event_cut(filler_blocks: usize) -> u64 {
        3 + u64::try_from(filler_blocks).unwrap() * 2
    }

    fn target_event_ordinal(filler_blocks: usize) -> u64 {
        1 + u64::try_from(filler_blocks).unwrap() * 2
    }

    fn seal(
        document: &SerializedGreenDocument,
        arena: &PageArena,
        filler_blocks: usize,
    ) -> SealedSetextNormalizationManifest {
        document
            .seal_setext_normalization_manifest(
                arena,
                TARGET,
                GreenHeadingOpenFacts::setext(1).unwrap(),
                target_event_ordinal(filler_blocks),
                SerializedMetric::default(),
                accepted_event_cut(filler_blocks),
                SerializedMetric { bytes: 4, utf16: 4 },
            )
            .unwrap()
    }

    fn begin_restart<'a>(
        arena: &mut PageArena,
        old: &'a SerializedGreenDocument,
        filler_blocks: usize,
        source_bytes: u64,
    ) -> (ArenaBuildTicket, SetextRetainedGreenRestart<'a>) {
        let manifest = seal(old, arena, filler_blocks);
        let ticket = arena.begin_build().unwrap();
        let restart = SetextRetainedGreenRestart::try_new(
            &ticket,
            arena,
            old,
            manifest,
            spec(2, 202, source_bytes, 2, 2),
        )
        .unwrap();
        (ticket, restart)
    }

    fn poll_restart<'a>(
        arena: &'a mut PageArena,
        ticket: ArenaBuildTicket,
        restart: &mut SetextRetainedGreenRestart<'_>,
    ) -> ArenaBuildSession<'a> {
        let mut session = arena.resume_build(ticket).unwrap();
        loop {
            match restart.poll(&mut session).unwrap() {
                SetextRetainedGreenRestartProgress::Pending => {}
                SetextRetainedGreenRestartProgress::Ready => return session,
            }
        }
    }

    fn finish_deletion(
        mut builder: ResumableSerializedGreenBuild,
        _provisional: ProvisionalParagraphEnter,
        mut session: ArenaBuildSession<'_>,
    ) -> SerializedGreenDocument {
        offer(&mut builder, &mut session, coverage(10, 1, 0));
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(&mut builder, &mut session, coverage(11, 1, 0));
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(30), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer(&mut builder, &mut session, coverage(12, 4, 0));
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finalize(builder, session)
    }

    fn finish_repromotion(
        mut builder: ResumableSerializedGreenBuild,
        provisional: ProvisionalParagraphEnter,
        mut session: ArenaBuildSession<'_>,
    ) -> SerializedGreenDocument {
        offer(&mut builder, &mut session, coverage(20, 9, 0));
        builder
            .begin_setext_promotion(
                &mut session,
                provisional,
                GreenHeadingOpenFacts::setext(1).unwrap(),
            )
            .unwrap();
        poll_to_event_boundary(&mut builder, &mut session);
        let _ = builder.take_setext_promotion(&session, TARGET).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        finalize(builder, session)
    }

    fn assert_nested_restored_validator(builder: &ResumableSerializedGreenBuild) {
        assert_eq!(
            builder.validator.open_frames,
            vec![
                StructuralOpenFrame {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                    facts: FactsEnvelope::empty(),
                    closed_children: ChildSequenceAggregate::singleton(
                        ClosedChildAggregate::default(),
                    ),
                },
                StructuralOpenFrame {
                    block: BlockId(42),
                    kind: GreenKind::BLOCK_QUOTE,
                    facts: FactsEnvelope::empty(),
                    closed_children: ChildSequenceAggregate::singleton(
                        ClosedChildAggregate::default(),
                    ),
                },
                StructuralOpenFrame {
                    block: TARGET,
                    kind: GreenKind::PARAGRAPH,
                    facts: FactsEnvelope::empty(),
                    closed_children: ChildSequenceAggregate::default(),
                },
            ]
        );
        assert_eq!(builder.validator.active_terminal, Some(TARGET));
        assert_eq!(builder.validator.coverage_runs, 2);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn nested_direct_restart_preserves_ancestor_kinds_and_exact_child_folds() {
        const EVENT_CUT: u64 = 9;
        let mut arena = PageArena::new();
        let old = build_nested_old(&mut arena, false);
        let current = old.restart_output_at_event_cut(&arena, EVENT_CUT).unwrap();
        assert_eq!(current.frames().len(), 3);
        assert!(current.frames()[0].closed_children().had_child);
        assert!(current.frames()[1].closed_children().had_child);
        let path = current.into_current_restart_path().unwrap();
        let (accepted, event_cut, _coverage, green) =
            path.into_parent_selected_activation_parts().unwrap();
        let authority = match green {
            ParentSelectedGreenRestartAuthority::Direct(authority) => authority,
            ParentSelectedGreenRestartAuthority::Setext(_) => {
                panic!("plain nested Paragraph unexpectedly requested Setext inverse")
            }
        };
        let path = authority.into_current_restart_path();
        let manifest_id = old.local_manifest_id(&arena).unwrap();
        let (manifest, old_root) = decode_document(&arena, manifest_id).unwrap();
        let binding = SerializedGreenManifestDescriptor::new(old.manifest_id(), &manifest);
        let mut resolution = ResolutionReceipt::default();
        resolution
            .absorb_restart_output(path.query_receipt())
            .unwrap();
        let plan = resolve_retained_checkpoint_from_path(
            &arena,
            old.manifest_id(),
            old_root,
            path.frames(),
            event_cut,
            accepted,
            RetainedGreenTransform::Direct,
            &mut resolution,
        )
        .unwrap();
        let block = plan.block;
        let ticket = arena.begin_build().unwrap();
        let mut receipt = SetextRetainedGreenRestartReceipt::default();
        receipt.absorb_resolution(resolution);
        let mut restart = SetextRetainedGreenRestart::begin_resolved(
            &ticket,
            &arena,
            spec(2, 402, 5, 2, 2),
            RetainedSetextGreenAuthority::LegacyDocument {
                document: &old,
                binding,
            },
            plan,
            block,
            RetainedGreenTransform::Direct,
            receipt,
        )
        .unwrap();
        let session = poll_restart(&mut arena, ticket, &mut restart);
        assert_eq!(restart.receipt().maximum_restored_open_depth, 3);
        let (builder, provisional, _, _) = restart.take_output(&session).unwrap().into_parts();
        assert_nested_restored_validator(&builder);
        assert_eq!(provisional.block, TARGET);
        drop((builder, provisional));
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 8).unwrap().complete {}
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn nested_setext_restart_preserves_ancestors_and_retypes_only_deepest_frame() {
        const EVENT_CUT: u64 = 9;
        let mut arena = PageArena::new();
        let old = build_nested_old(&mut arena, true);
        let current = old.restart_output_at_event_cut(&arena, EVENT_CUT).unwrap();
        assert_eq!(
            current
                .frames()
                .iter()
                .map(GreenRestartOutputFrame::kind)
                .collect::<Vec<_>>(),
            vec![
                GreenKind::DOCUMENT,
                GreenKind::BLOCK_QUOTE,
                GreenKind::HEADING,
            ]
        );
        assert!(current.frames()[0].closed_children().had_child);
        assert!(current.frames()[1].closed_children().had_child);
        let manifest = old
            .seal_setext_normalization_manifest(
                &arena,
                TARGET,
                GreenHeadingOpenFacts::setext(1).unwrap(),
                7,
                SerializedMetric { bytes: 1, utf16: 1 },
                EVENT_CUT,
                SerializedMetric { bytes: 4, utf16: 4 },
            )
            .unwrap();
        let ticket = arena.begin_build().unwrap();
        let mut restart = SetextRetainedGreenRestart::try_new(
            &ticket,
            &arena,
            &old,
            manifest,
            spec(2, 403, 5, 2, 2),
        )
        .unwrap();
        let session = poll_restart(&mut arena, ticket, &mut restart);
        assert_eq!(restart.receipt().maximum_restored_open_depth, 3);
        let (builder, provisional, _, _) = restart.take_output(&session).unwrap().into_parts();
        assert_nested_restored_validator(&builder);
        assert_eq!(provisional.block, TARGET);
        drop((builder, provisional));
        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 8).unwrap().complete {}
        old.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn deletion_reopens_finalized_heading_as_same_id_paragraph_at_accepted_cut() {
        assert_eq!(u64::try_from(OLD_SOURCE.len()).unwrap(), OLD_BYTES);
        assert_eq!(
            u64::try_from(DELETION_SOURCE.len()).unwrap(),
            DELETION_BYTES
        );
        let mut arena = PageArena::new();
        let old = build_old(&mut arena, 0);
        let old_facts = serialized_green_test_open_facts(&old, &arena).unwrap();
        assert_eq!(old_facts[1].0, GreenKind::HEADING);

        let (ticket, mut restart) = begin_restart(&mut arena, &old, 0, DELETION_BYTES);
        let session = poll_restart(&mut arena, ticket, &mut restart);
        let receipt = restart.receipt();
        let output = restart.take_output(&session).unwrap();
        let (builder, provisional, _, output_receipt) = output.into_parts();
        assert_eq!(receipt, output_receipt);
        assert_eq!(receipt.canonical_resolution_passes, 2);
        assert_eq!(receipt.source_payload_bytes_materialized, 0);
        assert_eq!(receipt.document_sized_event_vectors_materialized, 0);
        assert!(receipt.maximum_bounded_page_decode_bytes > 0);
        assert_eq!(receipt.inverse_leaf_pages_allocated, 1);
        let next = finish_deletion(builder, provisional, session);

        let trace = serialized_green_test_trace(&next, &arena).unwrap();
        assert!(matches!(
            trace.as_slice(),
            [
                SerializedGreenTestEvent::Enter {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT
                },
                SerializedGreenTestEvent::Enter {
                    block: TARGET,
                    kind: GreenKind::PARAGRAPH
                },
                SerializedGreenTestEvent::Coverage { .. },
                SerializedGreenTestEvent::Coverage { .. },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Coverage { .. },
                SerializedGreenTestEvent::Enter {
                    block: BlockId(30),
                    kind: GreenKind::PARAGRAPH
                },
                SerializedGreenTestEvent::Coverage { .. },
                SerializedGreenTestEvent::Exit,
                SerializedGreenTestEvent::Exit,
            ]
        ));
        assert_eq!(
            next.metric(&arena).unwrap(),
            SerializedMetric {
                bytes: 10,
                utf16: 10
            }
        );
    }

    #[test]
    fn canonical_cut_resolution_follows_setext_repack_not_captured_leaf_count() {
        let mut arena = PageArena::new();
        let (old, captured) = build_capacity_cliff_old(&mut arena);
        let metric = capacity_cliff_metric();
        assert_eq!(captured.leaves_before(), 1);
        let manifest = old
            .seal_setext_normalization_manifest(
                &arena,
                TARGET,
                GreenHeadingOpenFacts::setext(1).unwrap(),
                1,
                SerializedMetric::default(),
                CAPACITY_CLIFF_RUNS + 2,
                metric,
            )
            .unwrap();
        let ticket = arena.begin_build().unwrap();
        let mut restart = SetextRetainedGreenRestart::try_new(
            &ticket,
            &arena,
            &old,
            manifest,
            capacity_cliff_spec(2, 302, 2, 2),
        )
        .unwrap();
        let mut session = poll_restart(&mut arena, ticket, &mut restart);
        assert_eq!(restart.receipt().retained_leaves, 2);
        let (mut builder, provisional, _, receipt) =
            restart.take_output(&session).unwrap().into_parts();
        drop(provisional);
        assert_eq!(receipt.retained_leaves, 2);
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        let next = finalize(builder, session);
        assert_eq!(next.metric(&arena).unwrap(), metric);
        assert!(matches!(
            serialized_green_test_trace(&next, &arena).unwrap().get(1),
            Some(SerializedGreenTestEvent::Enter {
                block: TARGET,
                kind: GreenKind::PARAGRAPH
            })
        ));
    }

    #[test]
    fn unchanged_setext_shape_can_repromote_the_restored_paragraph() {
        let mut arena = PageArena::new();
        let old = build_old(&mut arena, 0);
        let (ticket, mut restart) = begin_restart(&mut arena, &old, 0, OLD_BYTES);
        let session = poll_restart(&mut arena, ticket, &mut restart);
        let (builder, provisional, _, _) = restart.take_output(&session).unwrap().into_parts();
        let next = finish_repromotion(builder, provisional, session);
        let trace = serialized_green_test_trace(&next, &arena).unwrap();
        assert!(matches!(
            trace.get(1),
            Some(SerializedGreenTestEvent::Enter {
                block: TARGET,
                kind: GreenKind::HEADING
            })
        ));
        assert_eq!(
            next.metric(&arena).unwrap(),
            SerializedMetric {
                bytes: OLD_BYTES,
                utf16: OLD_BYTES
            }
        );
    }

    fn abort_empty_build(arena: &mut PageArena, ticket: ArenaBuildTicket) {
        let id = arena.resume_build(ticket).unwrap().begin_abort().unwrap();
        loop {
            let ArenaBuildAbortReceipt { complete, .. } = arena.poll_build_abort(id, 8).unwrap();
            if complete {
                break;
            }
        }
    }

    #[test]
    fn sealed_manifest_rejects_wrong_binding_identity_outcome_and_cut() {
        let mut arena = PageArena::new();
        let old = build_old(&mut arena, 0);

        for case in 0..7 {
            let mut manifest = seal(&old, &arena, 0);
            match case {
                0 => manifest.old_binding.source_root = SourceRootId(999),
                1 => manifest.old_binding.parse_generation = ParseGeneration(999),
                2 => manifest.old_binding.syntax_profile = 999,
                3 => manifest.block = BlockId(999),
                4 => manifest.final_heading = GreenHeadingOpenFacts::setext(2).unwrap(),
                5 => manifest.accepted_event_cut += 1,
                6 => manifest.accepted_source_cut.bytes += 1,
                _ => unreachable!(),
            }
            let ticket = arena.begin_build().unwrap();
            let result = SetextRetainedGreenRestart::try_new(
                &ticket,
                &arena,
                &old,
                manifest,
                spec(2, 202, DELETION_BYTES, 2, 2),
            );
            assert!(result.is_err(), "forged case {case} unexpectedly admitted");
            abort_empty_build(&mut arena, ticket);
        }
        assert!(
            old.seal_setext_normalization_manifest(
                &arena,
                TARGET,
                GreenHeadingOpenFacts::atx(1).unwrap(),
                1,
                SerializedMetric::default(),
                3,
                SerializedMetric { bytes: 4, utf16: 4 },
            )
            .is_err()
        );
        assert!(
            old.seal_setext_normalization_manifest(
                &arena,
                TARGET,
                GreenHeadingOpenFacts::setext(1).unwrap(),
                1,
                SerializedMetric::default(),
                4,
                SerializedMetric { bytes: 4, utf16: 4 },
            )
            .is_err()
        );
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1_000).unwrap();
        }
    }

    #[test]
    fn retained_restart_can_abort_after_every_fuelled_poll() {
        let mut arena = PageArena::new();
        let old = build_old(&mut arena, 0);
        settle(&mut arena);
        let baseline_nodes = arena.metrics().live_nodes;
        let mut reached_ready = false;

        for polls_before_abort in 0..64 {
            let manifest = seal(&old, &arena, 0);
            let ticket = arena.begin_build().unwrap();
            let mut restart = SetextRetainedGreenRestart::try_new(
                &ticket,
                &arena,
                &old,
                manifest,
                spec(2, 202 + polls_before_abort, DELETION_BYTES, 2, 2),
            )
            .unwrap();
            let mut session = arena.resume_build(ticket).unwrap();
            for poll in 0..polls_before_abort {
                let before = restart
                    .builder
                    .as_ref()
                    .unwrap()
                    .receipt()
                    .resumable_arena_allocations;
                let progress = restart.poll(&mut session).unwrap();
                let after = restart
                    .builder
                    .as_ref()
                    .unwrap()
                    .receipt()
                    .resumable_arena_allocations;
                assert!(after - before <= 1, "restart poll {poll} allocated twice");
                if progress == SetextRetainedGreenRestartProgress::Ready {
                    reached_ready = true;
                    break;
                }
            }
            let abort = session.begin_abort().unwrap();
            drop(restart);
            while !arena.poll_build_abort(abort, 1).unwrap().complete {}
            settle(&mut arena);
            assert_eq!(arena.metrics().live_nodes, baseline_nodes);
            if reached_ready {
                break;
            }
        }
        assert!(
            reached_ready,
            "restart cancellation sweep never reached Ready"
        );
    }

    #[test]
    fn distant_prefix_leaf_identity_survives_retained_range_and_inverse_splice() {
        const FILLERS: usize = 700;
        let mut arena = PageArena::new();
        let old = build_old(&mut arena, FILLERS);
        assert!(old.leaf_count(&arena).unwrap() > 2);
        let distant = old.leaf_at(&arena, 0).unwrap().unwrap();
        let (ticket, mut restart) = begin_restart(&mut arena, &old, FILLERS, DELETION_BYTES);
        let session = poll_restart(&mut arena, ticket, &mut restart);
        let receipt = restart.receipt();
        let (builder, provisional, _, _) = restart.take_output(&session).unwrap().into_parts();
        let next = finish_deletion(builder, provisional, session);
        assert_eq!(next.leaf_at(&arena, 0).unwrap(), Some(distant));
        assert!(receipt.persistent_sequence_leaves_reused > 0);
        assert!(
            receipt.canonical_pages_scanned <= 8,
            "bounded canonical work regressed: {receipt:?}"
        );
        assert!(receipt.canonical_sequence_nodes_visited < 256);
        assert_eq!(receipt.source_payload_bytes_materialized, 0);
        assert_eq!(receipt.document_sized_event_vectors_materialized, 0);
        assert!(receipt.maximum_bounded_page_decode_bytes > 0);
    }
}

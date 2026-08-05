//! Executable authority seam between the restart composer and packed green.
//!
//! This gate deliberately supports only the Setext action required by its
//! first witness. Its purpose is to prove that storage consumes the composer's
//! one-use permit through an ordered concrete capability path, never through a
//! `BlockId` lookup. Unsupported actions fail before a candidate root exists.

#![forbid(unsafe_code)]

use flark_restart_composer_gate::{
    AdoptionAction, AdoptionPermit, AdoptionStamp, AdoptionUseRejection, BoundAdoptionAction,
    RootGeneration, SemanticRootId, StableBinding, StorageAdoptionContext,
};
use flark_v3_runtime_slice::{
    FactField, FactId, FactsEnvelope, GreenEnterCapability, GreenEnterRewrite, GreenKind,
    PageArena, ParseGeneration, SerializedGreenBuildReceipt, SerializedGreenDocument,
    SerializedGreenError,
};

/// One composer binding paired with the already-resolved current-manifest
/// green capability at the same outer-to-inner open depth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConcreteOpenBinding {
    pub stable: StableBinding,
    pub enter: GreenEnterCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageApplySpec {
    pub stamp: AdoptionStamp,
    pub next_parse_generation: ParseGeneration,
    pub next_semantic_epoch: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StorageApplyError {
    Adoption(AdoptionUseRejection),
    ConcreteRootMismatch,
    ConcretePathShapeMismatch,
    ConcreteBindingMismatch { open_depth: u32 },
    UnsupportedAction { open_depth: u32 },
    Green(SerializedGreenError),
}

impl From<AdoptionUseRejection> for StorageApplyError {
    fn from(value: AdoptionUseRejection) -> Self {
        Self::Adoption(value)
    }
}

impl From<SerializedGreenError> for StorageApplyError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

/// Storage-facing root identity derived from the generation-safe manifest
/// capability itself. A caller cannot substitute a semantic-root scalar that
/// names another arena manifest.
#[must_use]
pub fn semantic_root_identity(
    document: &SerializedGreenDocument,
) -> (SemanticRootId, RootGeneration) {
    let manifest = document.manifest_id();
    (
        SemanticRootId((u64::from(manifest.generation) << 32) | u64::from(manifest.index)),
        RootGeneration(u64::from(manifest.generation)),
    )
}

/// Consume one composer permit and apply its actions through the exact
/// outer-to-inner green capabilities supplied by the current root.
///
/// Every action is validated and translated before `rewrite_enters` starts its
/// arena transaction. Failure therefore leaves the committed root untouched.
///
/// # Errors
///
/// Returns a fail-closed authority, concrete-capability, unsupported-action,
/// or packed-green validation error before publishing a candidate root.
pub fn apply_adoption(
    document: &SerializedGreenDocument,
    arena: &mut PageArena,
    permit: AdoptionPermit,
    concrete: &[ConcreteOpenBinding],
    spec: StorageApplySpec,
    receipt: &mut SerializedGreenBuildReceipt,
) -> Result<SerializedGreenDocument, StorageApplyError> {
    let (root, generation) = semantic_root_identity(document);
    if spec.stamp.root != root || spec.stamp.root_generation != generation {
        return Err(StorageApplyError::ConcreteRootMismatch);
    }

    let stable = concrete
        .iter()
        .map(|binding| binding.stable)
        .collect::<Vec<_>>();
    let plan = permit.authorize_storage(StorageAdoptionContext {
        stamp: spec.stamp,
        bindings_outer_to_inner: &stable,
    })?;
    let actions = plan.into_actions();
    if actions.len() != concrete.len() {
        return Err(StorageApplyError::ConcretePathShapeMismatch);
    }
    let mut rewrites = Vec::with_capacity(concrete.len());
    for (index, (bound, binding)) in actions.zip(concrete).enumerate() {
        let expected_depth =
            u32::try_from(index + 1).map_err(|_| StorageApplyError::ConcretePathShapeMismatch)?;
        if bound.open_depth() != expected_depth {
            return Err(StorageApplyError::ConcreteBindingMismatch {
                open_depth: expected_depth,
            });
        }
        rewrites.push(translate_action(document, binding, bound)?);
    }
    document
        .rewrite_enters(
            arena,
            spec.next_parse_generation,
            spec.next_semantic_epoch,
            rewrites,
            receipt,
        )
        .map_err(StorageApplyError::Green)
}

fn translate_action(
    document: &SerializedGreenDocument,
    binding: &ConcreteOpenBinding,
    bound: BoundAdoptionAction,
) -> Result<GreenEnterRewrite, StorageApplyError> {
    let depth = bound.open_depth();
    if bound.binding() != binding.stable
        || binding.enter.manifest != document.manifest_id()
        || binding.enter.block.0 != binding.stable.block.0
    {
        return Err(StorageApplyError::ConcreteBindingMismatch { open_depth: depth });
    }
    match bound.into_action() {
        AdoptionAction::PromoteSetext { block, level, .. }
            if block == binding.stable.block && binding.enter.kind == GreenKind::PARAGRAPH =>
        {
            Ok(GreenEnterRewrite {
                target: binding.enter,
                kind: GreenKind::HEADING,
                facts: FactsEnvelope::new(vec![FactField::critical(
                    FactId::HEADING,
                    vec![level, 1],
                )])?,
            })
        }
        _ => Err(StorageApplyError::UnsupportedAction { open_depth: depth }),
    }
}

#[cfg(test)]
mod tests {
    use flark_restart_composer_gate::{
        BindingRole, BlockId as ComposerBlockId, CapabilityId, ChangedLabels, Composer,
        CompositionContext, ConsumerIndex, ControlContinuation, ControlFrame, DefinitionFold,
        EditLineageProof, GrammarVersion, LineageId, ParagraphHandoff, ParagraphPrefix,
        ParagraphSuffix, ParagraphTransition, PieceId, ProfileId, ReferenceSuffix, RestartState,
        RevisionId, SchedulerCursor, SchedulerPhase, SemanticPrefix, SemanticPrefixFrame,
        SemanticPrefixState, SemanticSuffix, SemanticSuffixFrame, SemanticSuffixState,
        SourceCursor, SourceRuns, SourceSpan, SourceTailId, StableAnchor, StableOpenBindings,
        SuffixCheckpoint, WinnerIndex,
    };
    use flark_v3_runtime_slice::{
        BlockId as GreenBlockId, ClosedChildAggregate, CoverageId, CoveragePart, CoverageRun,
        GrammarRevision, GreenAffinity, GreenCoordinate, GreenEvent, SerializedGreenRootSpec,
        SourceRevision,
    };

    use super::*;

    const TARGET_BLOCK: u64 = 100;
    const OLD: RevisionId = RevisionId(10);
    const CURRENT: RevisionId = RevisionId(11);
    const LINEAGE: LineageId = LineageId(7);
    const TAIL: SourceTailId = SourceTailId(500);

    struct BaseDocument {
        arena: PageArena,
        document: SerializedGreenDocument,
        target: GreenEnterCapability,
        far_leaf: flark_v3_runtime_slice::ArenaId,
    }

    fn build_base() -> BaseDocument {
        const TAIL_BLOCKS: u64 = 3_000;
        let mut events = vec![
            GreenEvent::enter(GreenBlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(
                GreenBlockId(TARGET_BLOCK),
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ),
            GreenEvent::Coverage(
                CoverageRun::new(CoverageId(1), 4, 4, 0, CoveragePart::CONTENT).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ];
        for index in 0..TAIL_BLOCKS {
            events.push(GreenEvent::enter(
                GreenBlockId(1_000 + index),
                GreenKind::PARAGRAPH,
                FactsEnvelope::empty(),
            ));
            events.push(GreenEvent::Coverage(
                CoverageRun::new(CoverageId(2 + index), 1, 1, 0, CoveragePart::CONTENT).unwrap(),
            ));
            events.push(GreenEvent::exit(ClosedChildAggregate::default()));
        }
        events.push(GreenEvent::exit(ClosedChildAggregate::default()));

        let mut arena = PageArena::new();
        let mut receipt = SerializedGreenBuildReceipt::default();
        let document = SerializedGreenDocument::build(
            &mut arena,
            SerializedGreenRootSpec {
                syntax_profile: 1,
                source_revision: SourceRevision(1),
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(1),
                semantic_epoch: 1,
                known_bytes: 0..4 + TAIL_BLOCKS,
            },
            events,
            &mut receipt,
        )
        .unwrap();
        while arena.metrics().pending_releases > 0 {
            arena.poll_reclaim(64).unwrap();
        }
        let far_leaf = document
            .leaf_at(&arena, document.leaf_count(&arena).unwrap() - 1)
            .unwrap()
            .unwrap();
        let mut cursor = document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        let target = cursor
            .next_coverage(&document, &arena)
            .unwrap()
            .unwrap()
            .owner
            .enter;
        assert_eq!(target.block, GreenBlockId(TARGET_BLOCK));
        BaseDocument {
            arena,
            document,
            target,
            far_leaf,
        }
    }

    fn source_cursor(revision: RevisionId, boundary_offset: u32) -> SourceCursor {
        SourceCursor {
            revision,
            lineage: LINEAGE,
            boundary: StableAnchor {
                piece: PieceId(99),
                offset: boundary_offset,
            },
            suffix_tail: TAIL,
            physical_line: 20,
        }
    }

    fn compose_setext(document: &SerializedGreenDocument, stable: StableBinding) -> AdoptionPermit {
        let (root, root_generation) = semantic_root_identity(document);
        let control = ControlContinuation::root(GrammarVersion(1), ProfileId(1)).push(
            ControlFrame::Paragraph {
                table_columns: None,
            },
        );
        let bindings = StableOpenBindings::new(root, root_generation).push(stable);
        let current = RestartState {
            control: control.clone(),
            bindings: bindings.clone(),
            semantic_prefix: SemanticPrefixState::default().push(SemanticPrefixFrame {
                block: stable.block,
                value: SemanticPrefix::Paragraph(ParagraphPrefix {
                    visible_runs: SourceRuns::one(SourceSpan::new(PieceId(1), 0, 2, 2)),
                    definitions: DefinitionFold::empty(),
                    changed_labels: ChangedLabels::default(),
                    handoff: ParagraphHandoff::Plain,
                }),
            }),
            source: source_cursor(CURRENT, 4),
            scheduler: SchedulerCursor {
                phase: SchedulerPhase::AtLineBoundary,
                work_offset: 0,
            },
        };
        let suffix = SuffixCheckpoint {
            control,
            bindings,
            semantic_suffix: SemanticSuffixState::default().push(SemanticSuffixFrame {
                block: stable.block,
                value: SemanticSuffix::Paragraph(ParagraphSuffix {
                    transition: ParagraphTransition::SetextUnderline { level: 1 },
                    visible_runs: SourceRuns::one(SourceSpan::new(PieceId(2), 0, 2, 2)),
                    references: ReferenceSuffix {
                        definitions: DefinitionFold::empty(),
                        old_global_winners: WinnerIndex::default(),
                        consumers: ConsumerIndex::default(),
                    },
                    table_body: flark_restart_composer_gate::ChildFold::default(),
                }),
            }),
            source: source_cursor(OLD, 3),
        };
        let witness = Composer::match_control(&current, &suffix).unwrap();
        Composer::compose(
            witness,
            CompositionContext {
                lineage: EditLineageProof {
                    lineage: LINEAGE,
                    old_revision: OLD,
                    current_revision: CURRENT,
                    old_boundary: suffix.source.boundary,
                    current_boundary: current.source.boundary,
                    unchanged_suffix_tail: TAIL,
                },
                live_root: root,
                live_root_generation: root_generation,
            },
        )
        .unwrap()
    }

    fn stable_binding() -> StableBinding {
        StableBinding {
            block: ComposerBlockId(TARGET_BLOCK),
            role: BindingRole::Paragraph,
            capability: CapabilityId(77),
            opened_at: StableAnchor {
                piece: PieceId(1),
                offset: 0,
            },
        }
    }

    #[test]
    fn consuming_setext_permit_rewrites_exact_capability_and_keeps_far_page() {
        let BaseDocument {
            mut arena,
            document,
            target,
            far_leaf,
        } = build_base();
        let stable = stable_binding();
        let permit = compose_setext(&document, stable);
        let stamp = permit.stamp();
        let mut receipt = SerializedGreenBuildReceipt::default();
        let next = apply_adoption(
            &document,
            &mut arena,
            permit,
            &[ConcreteOpenBinding {
                stable,
                enter: target,
            }],
            StorageApplySpec {
                stamp,
                next_parse_generation: ParseGeneration(2),
                next_semantic_epoch: 2,
            },
            &mut receipt,
        )
        .unwrap();

        let next_far = next
            .leaf_at(&arena, next.leaf_count(&arena).unwrap() - 1)
            .unwrap()
            .unwrap();
        assert_eq!(next_far, far_leaf);
        assert!(receipt.sequence_leaves_reused > 0);

        let mut next_cursor = next
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        let next_owner = next_cursor
            .next_coverage(&next, &arena)
            .unwrap()
            .unwrap()
            .owner;
        assert_eq!(next_owner.block, GreenBlockId(TARGET_BLOCK));
        assert_eq!(next_owner.kind, GreenKind::HEADING);
        assert_eq!(next_owner.facts.fields[0].value, [1, 1]);

        let mut old_cursor = document
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .unwrap();
        assert_eq!(
            old_cursor
                .next_coverage(&document, &arena)
                .unwrap()
                .unwrap()
                .owner
                .kind,
            GreenKind::PARAGRAPH
        );
    }

    #[test]
    fn wrong_concrete_capability_fails_closed_and_old_root_survives() {
        let BaseDocument {
            mut arena,
            document,
            mut target,
            far_leaf,
        } = build_base();
        let stable = stable_binding();
        let permit = compose_setext(&document, stable);
        let stamp = permit.stamp();
        target.leaf = far_leaf;
        let before = arena.metrics().live_nodes;
        let error = apply_adoption(
            &document,
            &mut arena,
            permit,
            &[ConcreteOpenBinding {
                stable,
                enter: target,
            }],
            StorageApplySpec {
                stamp,
                next_parse_generation: ParseGeneration(2),
                next_semantic_epoch: 2,
            },
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            StorageApplyError::Green(SerializedGreenError::StaleCursor)
        );
        while arena.metrics().pending_releases > 0 {
            arena.poll_reclaim(64).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, before);
        assert_eq!(document.block_count(&arena).unwrap(), 3_002);
    }

    #[test]
    fn caller_stamp_cannot_substitute_another_concrete_manifest() {
        let BaseDocument {
            mut arena,
            document,
            target,
            ..
        } = build_base();
        let stable = stable_binding();
        let permit = compose_setext(&document, stable);
        let mut stamp = permit.stamp();
        stamp.root = SemanticRootId(stamp.root.0 + 1);
        let error = apply_adoption(
            &document,
            &mut arena,
            permit,
            &[ConcreteOpenBinding {
                stable,
                enter: target,
            }],
            StorageApplySpec {
                stamp,
                next_parse_generation: ParseGeneration(2),
                next_semantic_epoch: 2,
            },
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err();
        assert_eq!(error, StorageApplyError::ConcreteRootMismatch);
    }
}

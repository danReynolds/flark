//! Exact current-prefix observation at a parser-selected semantic cut.
//!
//! The packed leaf barrier already proves the event/source coordinate. The
//! structural validator owns the corresponding open output path, so capturing
//! that path directly is both stronger and cheaper than rediscovering it by
//! querying the persistent prefix. The returned snapshot retains only
//! O(open depth) state and no source bytes, arena node identity, or reusable
//! leaf-cut authority. It can describe a candidate join, but cannot authorize
//! suffix reuse by itself.

use std::ops::Range;

use super::{
    ArenaBuildId, ArenaBuildSession, BlockId, ChildSequenceAggregate, FactsEnvelope,
    GrammarRevision, GreenKind, ParseGeneration, ResumableSerializedGreenBuild,
    SerializedGreenError, SerializedGreenLeafCut, SerializedMetric, SourceRevision, SourceRootId,
};

/// One exact current open output frame at semantic cut A.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuilderGreenPrefixFrame {
    block: BlockId,
    kind: GreenKind,
    facts: FactsEnvelope,
    closed_children: ChildSequenceAggregate,
}

impl BuilderGreenPrefixFrame {
    pub(crate) const fn block(&self) -> BlockId {
        self.block
    }

    pub(crate) const fn kind(&self) -> GreenKind {
        self.kind
    }

    pub(crate) fn facts(&self) -> &FactsEnvelope {
        &self.facts
    }

    pub(crate) const fn closed_children(&self) -> ChildSequenceAggregate {
        self.closed_children
    }
}

/// Honest bounded-storage receipt for one current-prefix capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BuilderGreenPrefixSnapshotReceipt {
    pub(crate) output_frames: usize,
    pub(crate) output_frame_capacity_bytes: usize,
    pub(crate) output_fact_field_capacity_bytes: usize,
    pub(crate) output_fact_value_capacity_bytes: usize,
    pub(crate) retained_source_bytes: usize,
    pub(crate) retained_arena_node_ids: usize,
    pub(crate) document_sized_event_vectors: usize,
}

/// Build-local observation of the exact green prefix already accepted at A.
///
/// Leaf/event/source coordinates and root metadata are copied from the
/// builder's private state, never supplied by the convergence caller. The
/// linear `SerializedGreenLeafCut` remains with the source/composer checkpoint;
/// this snapshot deliberately cannot substitute for it. Open frames are a
/// bounded observation of the validator that accepted those events, including
/// canonical Enter facts needed to reject a changed spanning path.
#[must_use = "the current green prefix snapshot must be joined or discarded"]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BuilderGreenPrefixSnapshot {
    build: ArenaBuildId,
    leaves_before: u64,
    events_before: u64,
    source_before: SerializedMetric,
    syntax_profile: u64,
    source_revision: SourceRevision,
    source_root: SourceRootId,
    source_total: SerializedMetric,
    grammar_revision: GrammarRevision,
    parse_generation: ParseGeneration,
    semantic_epoch: u64,
    known_bytes: Range<u64>,
    block_enters_before: u64,
    coverage_runs_before: u64,
    open_frames: Vec<BuilderGreenPrefixFrame>,
    receipt: BuilderGreenPrefixSnapshotReceipt,
}

impl BuilderGreenPrefixSnapshot {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn leaves_before(&self) -> u64 {
        self.leaves_before
    }

    pub(crate) const fn events_before(&self) -> u64 {
        self.events_before
    }

    pub(crate) const fn source_before(&self) -> SerializedMetric {
        self.source_before
    }

    pub(crate) const fn syntax_profile(&self) -> u64 {
        self.syntax_profile
    }

    pub(crate) const fn source_revision(&self) -> SourceRevision {
        self.source_revision
    }

    pub(crate) const fn source_root(&self) -> SourceRootId {
        self.source_root
    }

    pub(crate) const fn source_total(&self) -> SerializedMetric {
        self.source_total
    }

    pub(crate) const fn grammar_revision(&self) -> GrammarRevision {
        self.grammar_revision
    }

    pub(crate) const fn parse_generation(&self) -> ParseGeneration {
        self.parse_generation
    }

    pub(crate) const fn semantic_epoch(&self) -> u64 {
        self.semantic_epoch
    }

    pub(crate) fn known_bytes(&self) -> Range<u64> {
        self.known_bytes.clone()
    }

    pub(crate) const fn block_enters_before(&self) -> u64 {
        self.block_enters_before
    }

    pub(crate) const fn coverage_runs_before(&self) -> u64 {
        self.coverage_runs_before
    }

    pub(crate) fn open_frames(&self) -> &[BuilderGreenPrefixFrame] {
        &self.open_frames
    }

    pub(crate) const fn receipt(&self) -> BuilderGreenPrefixSnapshotReceipt {
        self.receipt
    }
}

impl ResumableSerializedGreenBuild {
    /// Borrows the existing exact leaf barrier and snapshots the validator's
    /// matching current open path. The caller retains the sole linear cut for
    /// the source/composer join. Work and retained memory are O(open depth); no
    /// persistent sequence node is visited and no source byte is retained.
    pub(crate) fn capture_builder_green_prefix_snapshot(
        &self,
        session: &ArenaBuildSession<'_>,
        leaf: &SerializedGreenLeafCut,
    ) -> Result<BuilderGreenPrefixSnapshot, SerializedGreenError> {
        self.ensure_session(session)?;
        self.validator.validate_stack_shape()?;
        if !self.line_boundary_cut_is_current(&leaf)
            || !self.validator.saw_root
            || self.validator.finished_root
            || self.validator.open_frames.is_empty()
            || self.validator.open_frames[0].kind != GreenKind::DOCUMENT
            || self.validator.open_frames[0].block.0 == 0
            || leaf.source_before.bytes > self.spec.source_bytes
            || leaf.source_before.utf16 > self.spec.source_utf16
        {
            return Err(SerializedGreenError::Invalid(
                "green prefix snapshot capture requires the exact live open-prefix barrier",
            ));
        }

        let open_depth = u64::try_from(self.validator.open_frames.len())
            .map_err(|_| SerializedGreenError::Overflow("green prefix snapshot open depth"))?;
        let structural_tokens = leaf
            .events_before
            .checked_sub(self.validator.coverage_runs)
            .ok_or(SerializedGreenError::Corrupt(
                "green prefix snapshot coverage count exceeds event count",
            ))?;
        let closed_structural_tokens =
            structural_tokens
                .checked_sub(open_depth)
                .ok_or(SerializedGreenError::Corrupt(
                    "green prefix snapshot open depth exceeds structural event count",
                ))?;
        if closed_structural_tokens % 2 != 0 {
            return Err(SerializedGreenError::Corrupt(
                "green prefix snapshot structural event parity is invalid",
            ));
        }
        let exits_before = closed_structural_tokens / 2;
        let block_enters_before =
            exits_before
                .checked_add(open_depth)
                .ok_or(SerializedGreenError::Overflow(
                    "green prefix snapshot Enter count",
                ))?;

        let mut open_frames = Vec::new();
        open_frames
            .try_reserve_exact(self.validator.open_frames.len())
            .map_err(|_| {
                SerializedGreenError::Invalid("green prefix snapshot frame reservation failed")
            })?;
        open_frames.extend(self.validator.open_frames.iter().map(|frame| {
            BuilderGreenPrefixFrame {
                block: frame.block,
                kind: frame.kind,
                facts: frame.facts.clone(),
                closed_children: frame.closed_children,
            }
        }));
        let output_fact_field_capacity_bytes =
            open_frames.iter().try_fold(0_usize, |total, frame| {
                total
                    .checked_add(
                        frame
                            .facts
                            .fields
                            .capacity()
                            .checked_mul(std::mem::size_of::<super::FactField>())
                            .ok_or(SerializedGreenError::Overflow(
                                "green prefix snapshot fact-field capacity",
                            ))?,
                    )
                    .ok_or(SerializedGreenError::Overflow(
                        "green prefix snapshot fact-field capacity",
                    ))
            })?;
        let output_fact_value_capacity_bytes =
            open_frames.iter().try_fold(0_usize, |total, frame| {
                frame.facts.fields.iter().try_fold(total, |total, field| {
                    total
                        .checked_add(field.value.capacity())
                        .ok_or(SerializedGreenError::Overflow(
                            "green prefix snapshot fact-value capacity",
                        ))
                })
            })?;
        let receipt = BuilderGreenPrefixSnapshotReceipt {
            output_frames: open_frames.len(),
            output_frame_capacity_bytes: open_frames
                .capacity()
                .checked_mul(std::mem::size_of::<BuilderGreenPrefixFrame>())
                .ok_or(SerializedGreenError::Overflow(
                    "green prefix snapshot frame capacity",
                ))?,
            output_fact_field_capacity_bytes,
            output_fact_value_capacity_bytes,
            retained_source_bytes: 0,
            retained_arena_node_ids: 0,
            document_sized_event_vectors: 0,
        };

        Ok(BuilderGreenPrefixSnapshot {
            build: leaf.build,
            leaves_before: leaf.leaves_before,
            events_before: leaf.events_before,
            source_before: leaf.source_before,
            syntax_profile: self.spec.syntax_profile,
            source_revision: self.spec.source_revision,
            source_root: self.spec.source_root,
            source_total: SerializedMetric {
                bytes: self.spec.source_bytes,
                utf16: self.spec.source_utf16,
            },
            grammar_revision: self.spec.grammar_revision,
            parse_generation: self.spec.parse_generation,
            semantic_epoch: self.spec.semantic_epoch,
            known_bytes: self.spec.known_bytes.clone(),
            block_enters_before,
            coverage_runs_before: self.validator.coverage_runs,
            open_frames,
            receipt,
        })
    }

    /// Admission-time stale-snapshot guard for the later prefix/suffix join.
    ///
    /// This intentionally compares the O(open-depth) frame snapshot. It is
    /// called once while admitting the actor-owned job, never on each poll;
    /// subsequent mutation is impossible because the job owns the builder.
    pub(crate) fn builder_green_prefix_snapshot_is_current(
        &self,
        cut: &BuilderGreenPrefixSnapshot,
    ) -> bool {
        let observed_leaf = SerializedGreenLeafCut {
            build: cut.build,
            leaves_before: cut.leaves_before,
            events_before: cut.events_before,
            source_before: cut.source_before,
        };
        self.line_boundary_cut_is_current(&observed_leaf)
            && cut.syntax_profile == self.spec.syntax_profile
            && cut.source_revision == self.spec.source_revision
            && cut.source_root == self.spec.source_root
            && cut.source_total
                == (SerializedMetric {
                    bytes: self.spec.source_bytes,
                    utf16: self.spec.source_utf16,
                })
            && cut.grammar_revision == self.spec.grammar_revision
            && cut.parse_generation == self.spec.parse_generation
            && cut.semantic_epoch == self.spec.semantic_epoch
            && cut.known_bytes == self.spec.known_bytes
            && cut.coverage_runs_before == self.validator.coverage_runs
            && cut.open_frames.len() == self.validator.open_frames.len()
            && cut
                .open_frames
                .iter()
                .zip(&self.validator.open_frames)
                .all(|(cut, live)| {
                    cut.block == live.block
                        && cut.kind == live.kind
                        && cut.facts == live.facts
                        && cut.closed_children == live.closed_children
                })
    }
}

#[cfg(test)]
mod tests {
    use super::{BuilderGreenPrefixFrame, ResumableSerializedGreenBuild, SerializedMetric};
    use crate::{
        ArenaBuildSession, BlockId, ChildSequenceAggregate, ClosedChildAggregate, CoverageId,
        CoveragePart, FactsEnvelope, GrammarRevision, GreenEvent, GreenKind, LogicalContribution,
        PageArena, ParseGeneration, SerializedGreenRootSpec, SerializedGreenStreamProgress,
        SourceProjectionRun, SourceRevision, SourceRootId,
    };

    fn spec() -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 7,
            source_revision: SourceRevision(11),
            source_root: SourceRootId(13),
            source_bytes: 2,
            source_utf16: 2,
            grammar_revision: GrammarRevision(17),
            parse_generation: ParseGeneration(19),
            semantic_epoch: 23,
            known_bytes: 0..2,
        }
    }

    fn poll_to_boundary(
        builder: &mut ResumableSerializedGreenBuild,
        session: &mut ArenaBuildSession<'_>,
    ) {
        loop {
            match builder.poll(session).unwrap() {
                SerializedGreenStreamProgress::ReadyForEvent => return,
                SerializedGreenStreamProgress::Pending => {}
                SerializedGreenStreamProgress::ManifestReady => {
                    panic!("convergence fixture unexpectedly completed")
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
        poll_to_boundary(builder, session);
    }

    fn coverage(id: u64, block: BlockId) -> GreenEvent {
        GreenEvent::Coverage(
            SourceProjectionRun::with_logical(
                CoverageId(id),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                block,
                LogicalContribution::Identity,
            )
            .unwrap(),
        )
    }

    #[test]
    fn capture_carries_exact_lineage_counts_and_open_child_folds() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut builder = ResumableSerializedGreenBuild::new(&ticket, spec()).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(2), GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(3), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer(&mut builder, &mut session, coverage(1, BlockId(3)));
        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(4), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        );
        offer(&mut builder, &mut session, coverage(2, BlockId(4)));

        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_boundary(&mut builder, &mut session);
        let leaf = builder.take_leaf_barrier_cut(&session).unwrap();
        let cut = builder
            .capture_builder_green_prefix_snapshot(&session, &leaf)
            .unwrap();

        assert_eq!(cut.build_id(), session.id());
        assert_eq!(cut.leaves_before(), 1);
        assert_eq!(cut.events_before(), 7);
        assert_eq!(cut.source_before(), SerializedMetric { bytes: 2, utf16: 2 });
        assert_eq!(cut.syntax_profile(), 7);
        assert_eq!(cut.source_revision(), SourceRevision(11));
        assert_eq!(cut.source_root(), SourceRootId(13));
        assert_eq!(cut.source_total(), SerializedMetric { bytes: 2, utf16: 2 });
        assert_eq!(cut.grammar_revision(), GrammarRevision(17));
        assert_eq!(cut.parse_generation(), ParseGeneration(19));
        assert_eq!(cut.semantic_epoch(), 23);
        assert_eq!(cut.known_bytes(), 0..2);
        assert_eq!(cut.block_enters_before(), 4);
        assert_eq!(cut.coverage_runs_before(), 2);
        assert_eq!(
            cut.open_frames(),
            [
                BuilderGreenPrefixFrame {
                    block: BlockId(1),
                    kind: GreenKind::DOCUMENT,
                    facts: FactsEnvelope::empty(),
                    closed_children: ChildSequenceAggregate::default(),
                },
                BuilderGreenPrefixFrame {
                    block: BlockId(2),
                    kind: GreenKind::BLOCK_QUOTE,
                    facts: FactsEnvelope::empty(),
                    closed_children: ChildSequenceAggregate::singleton(
                        ClosedChildAggregate::default(),
                    ),
                },
                BuilderGreenPrefixFrame {
                    block: BlockId(4),
                    kind: GreenKind::PARAGRAPH,
                    facts: FactsEnvelope::empty(),
                    closed_children: ChildSequenceAggregate::default(),
                },
            ]
        );
        assert_eq!(cut.receipt().output_frames, 3);
        assert!(
            cut.receipt().output_frame_capacity_bytes
                >= 3 * std::mem::size_of::<BuilderGreenPrefixFrame>()
        );
        assert_eq!(cut.receipt().retained_source_bytes, 0);
        assert_eq!(cut.receipt().output_fact_field_capacity_bytes, 0);
        assert_eq!(cut.receipt().output_fact_value_capacity_bytes, 0);
        assert_eq!(cut.receipt().retained_arena_node_ids, 0);
        assert_eq!(cut.receipt().document_sized_event_vectors, 0);
        assert!(builder.builder_green_prefix_snapshot_is_current(&cut));

        offer(
            &mut builder,
            &mut session,
            GreenEvent::exit(ClosedChildAggregate::default()),
        );
        assert!(!builder.builder_green_prefix_snapshot_is_current(&cut));

        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 16).unwrap().complete {}
        drop(builder);
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(16).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn capture_storage_is_exactly_proportional_to_open_depth() {
        const DEPTH: usize = 257;
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut empty_spec = spec();
        empty_spec.source_bytes = 0;
        empty_spec.source_utf16 = 0;
        empty_spec.known_bytes = 0..0;
        let mut builder = ResumableSerializedGreenBuild::new(&ticket, empty_spec).unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        offer(
            &mut builder,
            &mut session,
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        );
        for depth in 1..DEPTH {
            offer(
                &mut builder,
                &mut session,
                GreenEvent::enter(
                    BlockId(u64::try_from(depth + 1).unwrap()),
                    GreenKind::BLOCK_QUOTE,
                    FactsEnvelope::empty(),
                ),
            );
        }
        builder.begin_leaf_barrier(&mut session).unwrap();
        poll_to_boundary(&mut builder, &mut session);
        let leaf = builder.take_leaf_barrier_cut(&session).unwrap();
        let cut = builder
            .capture_builder_green_prefix_snapshot(&session, &leaf)
            .unwrap();

        assert_eq!(cut.open_frames().len(), DEPTH);
        assert_eq!(cut.block_enters_before(), u64::try_from(DEPTH).unwrap());
        assert_eq!(cut.coverage_runs_before(), 0);
        assert_eq!(cut.receipt().output_frames, DEPTH);
        assert!(
            cut.receipt().output_frame_capacity_bytes
                >= DEPTH * std::mem::size_of::<BuilderGreenPrefixFrame>()
        );
        assert!(
            cut.receipt().output_frame_capacity_bytes
                <= DEPTH * std::mem::size_of::<BuilderGreenPrefixFrame>() * 2
        );
        assert_eq!(cut.receipt().retained_source_bytes, 0);
        assert_eq!(cut.receipt().retained_arena_node_ids, 0);
        assert_eq!(cut.receipt().document_sized_event_vectors, 0);

        let abort = session.begin_abort().unwrap();
        while !arena.poll_build_abort(abort, 16).unwrap().complete {}
        drop(builder);
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(16).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}

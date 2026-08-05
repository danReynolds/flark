//! Source-bound authority-join mechanism for stable projection-reset markers.
//!
//! The packed marker is intentionally smaller than the proof needed to mint
//! it. A source claim, parser continuation permit, and composer sink state are
//! separate linear inputs. Stored reset lookup later proves only a storage
//! boundary; it never recreates the parser permit. The join below is an
//! isolated falsifier, not a second production composer: the eventual marker
//! operation must live on `SourceBoundProjectionComposer` (or its exclusive
//! writer), and its pending-Virtual fact must come from that same state rather
//! than a manually synchronized boolean.

use std::fmt;

use crate::{
    ArenaBuildId, ComposerSealedProjectionRunCapability, CoverageId, LiveCandidateEpoch,
    SerializedMetric, SourceSnapshotDescriptor,
};

/// Distinct parser/restart-continuation evidence. The reset codec does not yet
/// own the exact block parser, so production construction deliberately remains
/// absent. Unit tests use the explicitly named mechanism-only constructor to
/// falsify the join without claiming the parser gate is complete.
#[must_use = "parser reset authority must be joined once or discarded"]
pub struct ParserProjectionResetPermit {
    source: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    coverage: CoverageId,
    source_end: SerializedMetric,
    composer_state_generation: u64,
    continuation_identity: u64,
}

impl fmt::Debug for ParserProjectionResetPermit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ParserProjectionResetPermit")
            .field("source", &self.source)
            .field("build", &self.build)
            .field("coverage", &self.coverage)
            .field("source_end", &self.source_end)
            .field("composer_state_generation", &self.composer_state_generation)
            .field("continuation_identity", &self.continuation_identity)
            .finish_non_exhaustive()
    }
}

/// Parser authority that a logical envelope really ends. It is intentionally
/// not accepted by `push_reset_run`; finishing may left-attach a pending
/// Virtual while a storage reset must leave it right-biased.
#[must_use = "semantic-envelope authority must be consumed once or discarded"]
pub struct SemanticEnvelopeEnd {
    source: SourceSnapshotDescriptor,
    build: ArenaBuildId,
    source_end: SerializedMetric,
    composer_state_generation: u64,
    envelope_identity: u64,
}

impl fmt::Debug for SemanticEnvelopeEnd {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SemanticEnvelopeEnd")
            .field("source", &self.source)
            .field("build", &self.build)
            .field("source_end", &self.source_end)
            .field("composer_state_generation", &self.composer_state_generation)
            .field("envelope_identity", &self.envelope_identity)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionResetCompositionError {
    ComposerPoisoned,
    WrongCandidate,
    OutOfOrderSource,
    WrongComposerState,
    WrongParserPermit,
    PendingRightBiasedVirtual,
    WrongSemanticEnvelopeEnd,
}

impl fmt::Display for ProjectionResetCompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "source-bound projection reset error: {self:?}")
    }
}

impl std::error::Error for ProjectionResetCompositionError {}

/// Mechanism-only model of the marker join that the eventual direct projection
/// composer must own. No method accepts a raw `SourceProjectionRun`: the run
/// is already inside the linear sealed-run capability before reset authority
/// is considered.
///
/// This separate object must not be wired beside the production composer: its
/// pending-Virtual bit would become a second source of truth. Its purpose is to
/// prove the typed join, linear-input burn, and terminal-poison semantics until
/// those operations move into the one canonical composer/writer.
#[derive(Debug)]
pub struct ProjectionResetComposerJoinMechanism {
    epoch: LiveCandidateEpoch,
    next_source: SerializedMetric,
    next_composer_state_generation: u64,
    pending_right_biased_virtual: bool,
    poisoned: bool,
}

impl ProjectionResetComposerJoinMechanism {
    #[must_use]
    pub const fn begin_clean(epoch: LiveCandidateEpoch) -> Self {
        Self {
            epoch,
            next_source: SerializedMetric { bytes: 0, utf16: 0 },
            next_composer_state_generation: 1,
            pending_right_biased_virtual: false,
            poisoned: false,
        }
    }

    /// Admits one already-sealed ordinary run. Raw logical contribution,
    /// owner depth, coverage ID, and Program payload cannot be swapped here.
    pub fn push_sealed_run(
        &mut self,
        sealed: ComposerSealedProjectionRunCapability,
    ) -> Result<ComposerSealedProjectionRunCapability, ProjectionResetCompositionError> {
        self.require_live()?;
        if let Err(error) = self.validate_sealed_run(&sealed) {
            return self.fail(error);
        }
        self.advance(sealed.source_end())?;
        Ok(sealed)
    }

    /// Joins exact sealed-run ownership with a distinct parser continuation
    /// permit and the composer's Virtual-safe state, then marks the owned run.
    #[allow(clippy::needless_pass_by_value)] // Parser authority burns on every attempt.
    pub fn push_reset_run(
        &mut self,
        sealed: ComposerSealedProjectionRunCapability,
        parser: ParserProjectionResetPermit,
    ) -> Result<ComposerSealedProjectionRunCapability, ProjectionResetCompositionError> {
        self.require_live()?;
        if let Err(error) = self.validate_sealed_run(&sealed) {
            return self.fail(error);
        }
        if self.pending_right_biased_virtual {
            return self.fail(ProjectionResetCompositionError::PendingRightBiasedVirtual);
        }
        if parser.source != sealed.source()
            || parser.build != sealed.build_id()
            || parser.coverage != sealed.coverage_id()
            || parser.source_end != sealed.source_end()
            || parser.composer_state_generation != sealed.composer_generation()
            || parser.continuation_identity == 0
        {
            return self.fail(ProjectionResetCompositionError::WrongParserPermit);
        }
        let source_end = sealed.source_end();
        let sealed = sealed.mark_projection_reset_after();
        self.advance(source_end)?;
        Ok(sealed)
    }

    fn validate_sealed_run(
        &self,
        sealed: &ComposerSealedProjectionRunCapability,
    ) -> Result<(), ProjectionResetCompositionError> {
        if sealed.source() != self.epoch.source() || sealed.build_id() != self.epoch.build_id() {
            return Err(ProjectionResetCompositionError::WrongCandidate);
        }
        if sealed.source_start() != self.next_source {
            return Err(ProjectionResetCompositionError::OutOfOrderSource);
        }
        if sealed.composer_generation() != self.next_composer_state_generation {
            return Err(ProjectionResetCompositionError::WrongComposerState);
        }
        Ok(())
    }

    fn advance(
        &mut self,
        source_end: SerializedMetric,
    ) -> Result<(), ProjectionResetCompositionError> {
        let Some(next_generation) = self.next_composer_state_generation.checked_add(1) else {
            return self.fail(ProjectionResetCompositionError::WrongComposerState);
        };
        self.next_source = source_end;
        self.next_composer_state_generation = next_generation;
        Ok(())
    }

    fn require_live(&self) -> Result<(), ProjectionResetCompositionError> {
        if self.poisoned {
            Err(ProjectionResetCompositionError::ComposerPoisoned)
        } else {
            Ok(())
        }
    }

    fn fail<T>(
        &mut self,
        error: ProjectionResetCompositionError,
    ) -> Result<T, ProjectionResetCompositionError> {
        self.poisoned = true;
        Err(error)
    }

    /// EOF-only semantic operation. It deliberately has a different input
    /// type from `push_reset_run` and never marks a storage reset.
    #[allow(clippy::needless_pass_by_value)] // Semantic-end authority is linear.
    pub fn finish_envelope(
        &mut self,
        end: SemanticEnvelopeEnd,
    ) -> Result<(), ProjectionResetCompositionError> {
        self.require_live()?;
        if end.source != self.epoch.source()
            || end.build != self.epoch.build_id()
            || end.source_end != self.next_source
            || end.composer_state_generation != self.next_composer_state_generation
            || end.envelope_identity == 0
        {
            return self.fail(ProjectionResetCompositionError::WrongSemanticEnvelopeEnd);
        }
        self.pending_right_biased_virtual = false;
        Ok(())
    }

    #[cfg(test)]
    fn mechanism_only_defer_virtual(&mut self) {
        assert!(
            !self.poisoned,
            "poisoned join cannot change mechanism state"
        );
        self.pending_right_biased_virtual = true;
    }
}

#[cfg(test)]
impl ParserProjectionResetPermit {
    fn mechanism_only(
        sealed: &ComposerSealedProjectionRunCapability,
        continuation_identity: u64,
    ) -> Self {
        Self {
            source: sealed.source(),
            build: sealed.build_id(),
            coverage: sealed.coverage_id(),
            source_end: sealed.source_end(),
            composer_state_generation: sealed.composer_generation(),
            continuation_identity,
        }
    }
}

#[cfg(test)]
impl SemanticEnvelopeEnd {
    fn mechanism_only(
        epoch: LiveCandidateEpoch,
        source_end: SerializedMetric,
        composer_state_generation: u64,
        envelope_identity: u64,
    ) -> Self {
        Self {
            source: epoch.source(),
            build: epoch.build_id(),
            source_end,
            composer_state_generation,
            envelope_identity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialized_green::source_boundary_resolver::{
        SerializedGreenCoverageSideBias, SerializedGreenCoverageSideOutcome,
    };
    use crate::{
        AtomicProjection, BlockId, CandidateLogicalAction, CandidateSourcePoll,
        ClosedChildAggregate, CoveragePart, FactsEnvelope, GrammarRevision, GreenAffinity,
        GreenCoordinate, GreenEvent, GreenKind, LogicalContribution, LogicalSegmentMapping,
        PageArena, ParseGeneration, ProjectionPiece, ProjectionProgram, ProjectionResetSeekOutcome,
        ResolvedProjectionReset, SerializedGreenBuildReceipt, SerializedGreenDocument,
        SerializedGreenError, SerializedGreenRootSpec, SourceProjectionRun, SourceRevision,
        SourceRootId, StoredProjectionResetCapability, ValidatedSourceClaim, VirtualProjectionKind,
    };

    fn exact_claim(
        source: &str,
    ) -> (
        LiveCandidateEpoch,
        ValidatedSourceClaim,
        SourceProjectionRun,
    ) {
        let mut document = crate::LiveDocumentStore::new(source, 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        let _root = document
            .candidate_open_binding(epoch, GreenKind::DOCUMENT)
            .unwrap();
        let paragraph = document
            .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
            .unwrap();
        let atom = loop {
            match document.poll_candidate_source(epoch, 1).unwrap() {
                CandidateSourcePoll::NeedFuel(_) => {}
                CandidateSourcePoll::Atom { atom, .. } => break atom,
                CandidateSourcePoll::Eof(_) => panic!("one exact source atom expected"),
            }
        };
        let logical = CandidateLogicalAction::identity(&paragraph).unwrap();
        let claim = document
            .candidate_claim_to(
                epoch,
                atom.boundary(),
                &paragraph,
                CoveragePart::CONTENT,
                &logical,
                GreenAffinity::Downstream,
            )
            .unwrap();
        let metric = claim.metric();
        let run = SourceProjectionRun::with_logical(
            claim.coverage_id(),
            metric.bytes(),
            metric.utf16(),
            0,
            claim.part(),
            paragraph.block_id(),
            LogicalContribution::Identity,
        )
        .unwrap();
        (epoch, claim, run)
    }

    #[allow(clippy::needless_pass_by_value)] // The debug claim is deliberately burned.
    fn mechanism_only_sealed(
        claim: ValidatedSourceClaim,
        run: SourceProjectionRun,
        composer_generation: u64,
    ) -> ComposerSealedProjectionRunCapability {
        let (_, byte_end) = claim.absolute_range();
        let metric = claim.metric();
        ComposerSealedProjectionRunCapability::mechanism_only(
            claim.source(),
            claim.build_id(),
            SerializedMetric { bytes: 0, utf16: 0 },
            SerializedMetric {
                bytes: byte_end,
                utf16: metric.utf16(),
            },
            composer_generation,
            run,
        )
    }

    #[test]
    fn reset_marker_requires_source_parser_and_virtual_safe_composer_state() {
        let (epoch, claim, run) = exact_claim("😀");
        let sealed = mechanism_only_sealed(claim, run, 1);
        assert_eq!(sealed.source_end(), SerializedMetric { bytes: 4, utf16: 2 });
        let parser = ParserProjectionResetPermit::mechanism_only(&sealed, 17);
        let mut composer = ProjectionResetComposerJoinMechanism::begin_clean(epoch);
        let run = composer.push_reset_run(sealed, parser).unwrap().into_run();
        assert!(run.has_projection_reset_after());

        let (epoch, claim, run) = exact_claim("x");
        let sealed = mechanism_only_sealed(claim, run, 1);
        let parser = ParserProjectionResetPermit::mechanism_only(&sealed, 18);
        let mut composer = ProjectionResetComposerJoinMechanism::begin_clean(epoch);
        composer.mechanism_only_defer_virtual();
        assert!(matches!(
            composer.push_reset_run(sealed, parser),
            Err(ProjectionResetCompositionError::PendingRightBiasedVirtual)
        ));
        assert_eq!(
            composer.finish_envelope(SemanticEnvelopeEnd::mechanism_only(
                epoch,
                SerializedMetric::default(),
                1,
                19,
            )),
            Err(ProjectionResetCompositionError::ComposerPoisoned),
            "a consumed linear-input failure must terminally poison the join"
        );

        let (epoch, claim, run) = exact_claim("y");
        let sealed = mechanism_only_sealed(claim, run, 1);
        let mut composer = ProjectionResetComposerJoinMechanism::begin_clean(epoch);
        let run = composer.push_sealed_run(sealed).unwrap().into_run();
        assert!(!run.has_projection_reset_after());
        composer.mechanism_only_defer_virtual();
        composer
            .finish_envelope(SemanticEnvelopeEnd::mechanism_only(
                epoch,
                SerializedMetric { bytes: 1, utf16: 1 },
                2,
                29,
            ))
            .unwrap();
        assert!(!run.has_projection_reset_after());
    }

    #[test]
    fn candidate_and_parser_permits_cannot_cross_builds() {
        let (epoch_a, claim_a, run_a) = exact_claim("a");
        let sealed_a = mechanism_only_sealed(claim_a, run_a, 1);
        let (_epoch_b, claim_b, run_b) = exact_claim("b");
        let sealed_b = mechanism_only_sealed(claim_b, run_b, 1);
        let parser_b = ParserProjectionResetPermit::mechanism_only(&sealed_b, 2);
        let mut composer = ProjectionResetComposerJoinMechanism::begin_clean(epoch_a);
        assert!(matches!(
            composer.push_reset_run(sealed_a, parser_b),
            Err(ProjectionResetCompositionError::WrongParserPermit)
        ));

        let (epoch_a, _claim_a, _run_a) = exact_claim("a");
        let (_epoch_b, claim_b, run_b) = exact_claim("b");
        let sealed_b = mechanism_only_sealed(claim_b, run_b, 1);
        let mut composer = ProjectionResetComposerJoinMechanism::begin_clean(epoch_a);
        assert!(matches!(
            composer.push_sealed_run(sealed_b),
            Err(ProjectionResetCompositionError::WrongCandidate)
        ));
        assert_eq!(
            composer.finish_envelope(SemanticEnvelopeEnd::mechanism_only(
                epoch_a,
                SerializedMetric::default(),
                1,
                4,
            )),
            Err(ProjectionResetCompositionError::ComposerPoisoned)
        );

        let (epoch, claim, run) = exact_claim("same-source");
        let wrong_generation = mechanism_only_sealed(claim, run, 2);
        let parser = ParserProjectionResetPermit::mechanism_only(&wrong_generation, 3);
        let mut composer = ProjectionResetComposerJoinMechanism::begin_clean(epoch);
        assert!(matches!(
            composer.push_reset_run(wrong_generation, parser),
            Err(ProjectionResetCompositionError::WrongComposerState)
        ));
    }

    fn root_spec(bytes: u64, utf16: u64, generation: u64) -> SerializedGreenRootSpec {
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: SourceRevision(generation),
            source_root: SourceRootId(generation + 1),
            source_bytes: bytes,
            source_utf16: utf16,
            grammar_revision: GrammarRevision(1),
            parse_generation: ParseGeneration(generation + 1),
            semantic_epoch: generation + 1,
            known_bytes: 0..bytes,
        }
    }

    fn identity_run(id: u64, bytes: u64, utf16: u64) -> SourceProjectionRun {
        SourceProjectionRun::with_logical(
            CoverageId(id),
            bytes,
            utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(2),
            LogicalContribution::Identity,
        )
        .unwrap()
    }

    fn mechanism_only_mark(mut run: SourceProjectionRun) -> SourceProjectionRun {
        // Query/locality fixtures bypass the missing exact parser core only
        // inside this unit module. Production has no raw marker setter.
        run.mark_projection_reset_after();
        run
    }

    fn build_document(
        arena: &mut PageArena,
        runs: Vec<SourceProjectionRun>,
        generation: u64,
    ) -> SerializedGreenDocument {
        let metric = runs
            .iter()
            .fold(SerializedMetric { bytes: 0, utf16: 0 }, |total, run| {
                SerializedMetric {
                    bytes: total.bytes + run.metric.bytes,
                    utf16: total.utf16 + run.metric.utf16,
                }
            });
        let events = [
            GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
            GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        ]
        .into_iter()
        .chain(runs.into_iter().map(GreenEvent::Coverage))
        .chain([
            GreenEvent::exit(ClosedChildAggregate::default()),
            GreenEvent::exit(ClosedChildAggregate::default()),
        ]);
        SerializedGreenDocument::build(
            arena,
            root_spec(metric.bytes, metric.utf16, generation),
            events,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap()
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(10_000).unwrap();
        }
    }

    fn found(outcome: ProjectionResetSeekOutcome) -> StoredProjectionResetCapability {
        match outcome {
            ProjectionResetSeekOutcome::Found { reset, .. } => reset,
            other => panic!("stored reset expected, got {other:?}"),
        }
    }

    #[test]
    fn unicode_and_boundary_affinity_find_exact_flat_run_resets() {
        let mut arena = PageArena::new();
        let runs = vec![
            mechanism_only_mark(identity_run(1, 2, 1)),
            mechanism_only_mark(
                SourceProjectionRun::with_logical(
                    CoverageId(2),
                    2,
                    2,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Atomic(AtomicProjection::crlf_to_lf()),
                )
                .unwrap(),
            ),
            identity_run(3, 4, 2),
        ];
        let document = build_document(&mut arena, runs, 1);

        let downstream = document
            .seek(&arena, GreenCoordinate::Bytes, 2, GreenAffinity::Downstream)
            .unwrap();
        let reset = found(
            document
                .previous_projection_reset(&arena, downstream, 1)
                .unwrap(),
        );
        assert_eq!(reset.coverage(), Some(CoverageId(1)));
        assert_eq!(reset.source_end(), SerializedMetric { bytes: 2, utf16: 1 });
        assert_eq!(
            document.resolve_projection_reset(&arena, &reset).unwrap(),
            ResolvedProjectionReset {
                source_end: SerializedMetric { bytes: 2, utf16: 1 },
                coverage: Some(CoverageId(1)),
                implicit_zero: false,
            }
        );

        let upstream = document
            .seek(&arena, GreenCoordinate::Bytes, 2, GreenAffinity::Upstream)
            .unwrap();
        let zero = document
            .previous_projection_reset(&arena, upstream, 1)
            .unwrap();
        let ProjectionResetSeekOutcome::ImplicitZero { reset: zero, .. } = zero else {
            panic!("upstream cursor starts before the touched run")
        };
        assert!(zero.is_implicit_zero());

        let inside_emoji = document
            .seek(&arena, GreenCoordinate::Bytes, 6, GreenAffinity::Downstream)
            .unwrap();
        let reset = found(
            document
                .previous_projection_reset(&arena, inside_emoji, 1)
                .unwrap(),
        );
        assert_eq!(reset.coverage(), Some(CoverageId(2)));
        assert_eq!(reset.source_end(), SerializedMetric { bytes: 4, utf16: 3 });

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn virtual_stays_with_following_run_across_storage_reset() {
        let mut arena = PageArena::new();
        let virtual_program = ProjectionProgram::new(vec![
            ProjectionPiece::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            },
            ProjectionPiece::Identity {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
            },
        ])
        .unwrap();
        let second = SourceProjectionRun::with_logical(
            CoverageId(2),
            1,
            1,
            0,
            CoveragePart::CONTENT,
            BlockId(2),
            LogicalContribution::Program(virtual_program),
        )
        .unwrap();
        let document = build_document(
            &mut arena,
            vec![mechanism_only_mark(identity_run(1, 1, 1)), second],
            2,
        );

        let boundary = document
            .seek(&arena, GreenCoordinate::Bytes, 1, GreenAffinity::Downstream)
            .unwrap();
        let paragraph = boundary.open_path().last().unwrap().enter;
        let reset = found(
            document
                .previous_projection_reset(&arena, boundary, 1)
                .unwrap(),
        );
        assert_eq!(reset.coverage(), Some(CoverageId(1)));

        let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
        let first = logical.next_segment(&document, &arena).unwrap().unwrap();
        assert_eq!(first.coverage, CoverageId(1));
        let virtual_segment = logical.next_segment(&document, &arena).unwrap().unwrap();
        assert_eq!(virtual_segment.coverage, CoverageId(2));
        assert!(matches!(
            virtual_segment.mapping,
            LogicalSegmentMapping::Virtual {
                kind: VirtualProjectionKind::LineFeed
            }
        ));
        let following = logical.next_segment(&document, &arena).unwrap().unwrap();
        assert_eq!(following.coverage, CoverageId(2));

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn stored_reset_rejects_cross_manifest_even_for_equal_source_shape() {
        let mut arena = PageArena::new();
        let first = build_document(
            &mut arena,
            vec![mechanism_only_mark(identity_run(1, 1, 1))],
            3,
        );
        let mut cursor = first
            .seek(&arena, GreenCoordinate::Bytes, 1, GreenAffinity::Downstream)
            .unwrap();
        assert!(cursor.next_coverage(&first, &arena).unwrap().is_some());
        let reset = found(first.previous_projection_reset(&arena, cursor, 1).unwrap());
        let second = build_document(
            &mut arena,
            vec![mechanism_only_mark(identity_run(1, 1, 1))],
            4,
        );
        assert_eq!(
            second.resolve_projection_reset(&arena, &reset),
            Err(SerializedGreenError::StaleCursor)
        );
        assert!(first.resolve_projection_reset(&arena, &reset).is_ok());

        first.release_later(&mut arena).unwrap();
        second.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }

    #[test]
    fn predecessor_scan_is_page_bounded_without_a_global_reset_directory() {
        const RUNS: usize = 24_000;
        let mut arena = PageArena::new();
        let mut runs = Vec::with_capacity(RUNS);
        for index in 0..RUNS {
            let run = identity_run(u64::try_from(index + 1).unwrap(), 1, 1);
            runs.push(if index == 0 {
                mechanism_only_mark(run)
            } else {
                run
            });
        }
        let document = build_document(&mut arena, runs, 5);
        let leaves = usize::try_from(document.leaf_count(&arena).unwrap()).unwrap();
        assert!(leaves >= 3, "fixture must span several encoded pages");
        let end = match document
            .resolve_storage_source_coverage_side(
                &arena,
                u64::try_from(RUNS).unwrap(),
                SerializedGreenCoverageSideBias::AfterPreceding,
            )
            .unwrap()
        {
            SerializedGreenCoverageSideOutcome::Found { observation, .. } => observation,
            other => panic!("end Coverage side expected, got {other:?}"),
        };

        let bounded = document
            .previous_projection_reset_from_observation(&arena, &end, 1)
            .unwrap();
        assert!(matches!(
            bounded,
            ProjectionResetSeekOutcome::NotFoundWithinBound(_)
        ));
        assert_eq!(bounded.receipt().pages_scanned, 1);
        assert_eq!(bounded.receipt().predecessor_pages, 0);

        let complete = document
            .previous_projection_reset_from_observation(&arena, &end, leaves)
            .unwrap();
        let receipt = complete.receipt();
        let reset = found(complete);
        assert_eq!(reset.coverage(), Some(CoverageId(1)));
        assert_eq!(reset.source_end().bytes, 1);
        assert!(receipt.pages_scanned <= leaves);
        assert_eq!(receipt.predecessor_pages + 1, receipt.pages_scanned);
        assert!(receipt.sequence_nodes_visited < RUNS);

        document.release_later(&mut arena).unwrap();
        settle(&mut arena);
    }
}

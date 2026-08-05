//! Feature-gated adversarial fixtures for the inline liveness decision.
//!
//! This is research instrumentation, not product API. It constructs packed
//! green directly so repeated samples measure the current read-side
//! materializer rather than fixture construction or block parsing.

use std::collections::BTreeMap;
use std::ops::Range;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use flark_comrak_inline_fragment_gate::{
    ComposedInlineFragment, InlineFragment, InlineFragmentRequest, InlineInputKind, InlineProfile,
    InlineReferenceSnapshot, InlineReferenceTarget, LogicalOriginMap, LogicalOriginRun,
    MAX_INLINE_FRAGMENT_BYTES, OriginRunKind, parse_inline_fragment,
};

use crate::{
    AtomicProjection, BlockId, ClosedChildAggregate, CoverageId, CoveragePart, FactsEnvelope,
    GrammarRevision, GreenAffinity, GreenCoordinate, GreenEvent, GreenKind,
    InlineLeafMaterializationFuel, InlineLeafMaterializationJob, InlineLeafMaterializationProgress,
    InlineLeafOutcome, LiveDocumentStore, LogicalContribution, PageArena, ParseGeneration,
    ProjectionPiece, ProjectionProgram, SerializedGreenBuildReceipt, SerializedGreenDocument,
    SerializedGreenRootSpec, SourceProjectionRun, VirtualProjectionKind,
    derive_inline_leaf_presentation,
};

const PROGRAM_PIECES_PER_RUN: usize = 800;
const FRAGMENTED_LOGICAL_BYTES: usize = MAX_INLINE_FRAGMENT_BYTES;
const HIDDEN_CAP_SEGMENTS: usize = crate::MAX_INLINE_PROJECTION_SEGMENTS;
const REFERENCE_LABEL_BYTES: usize = 3;
const REFERENCE_TOKEN_BYTES: usize = REFERENCE_LABEL_BYTES + 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum InlineLivenessShape {
    Fragmented8KiB = 0,
    HiddenAtSegmentCap = 1,
    HiddenOverSegmentCap = 2,
    ReferenceDense8KiB = 3,
    ReferenceDenseOverInputCap = 4,
    FragmentedEnclosingEmphasis8KiB = 5,
}

impl InlineLivenessShape {
    #[must_use]
    pub const fn from_probe_code(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Fragmented8KiB),
            1 => Some(Self::HiddenAtSegmentCap),
            2 => Some(Self::HiddenOverSegmentCap),
            3 => Some(Self::ReferenceDense8KiB),
            4 => Some(Self::ReferenceDenseOverInputCap),
            5 => Some(Self::FragmentedEnclosingEmphasis8KiB),
            _ => None,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fragmented8KiB => "fragmented-8k",
            Self::HiddenAtSegmentCap => "hidden-cap",
            Self::HiddenOverSegmentCap => "hidden-over-cap",
            Self::ReferenceDense8KiB => "reference-dense-8k",
            Self::ReferenceDenseOverInputCap => "reference-dense-over-input-cap",
            Self::FragmentedEnclosingEmphasis8KiB => "fragmented-enclosing-emphasis-8k",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineLivenessMetadata {
    pub source_bytes: usize,
    pub logical_bytes: usize,
    pub logical_utf16: usize,
    pub logical_segments: usize,
    pub projection_program_runs: usize,
    pub source_copy_ranges: usize,
    pub source_bytes_copied: usize,
    pub origin_runs: usize,
    pub reference_symbols: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineLivenessCompositionMetadata {
    pub semantic_facts: usize,
    pub projection_facts: usize,
    pub mapped_physical_parts: usize,
    pub reference_dependencies: usize,
}

#[derive(Clone, Debug)]
struct FixtureReferenceSnapshot {
    symbols: BTreeMap<String, InlineReferenceTarget>,
    #[cfg(test)]
    resolve_calls: Arc<AtomicUsize>,
}

impl FixtureReferenceSnapshot {
    fn from_labels(labels: &[String]) -> Self {
        let symbols = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                (
                    label.clone(),
                    InlineReferenceTarget {
                        symbol_id: u64::try_from(index + 1).expect("bounded reference fixture"),
                        presence_generation: 1,
                        defined: true,
                    },
                )
            })
            .collect();
        Self {
            symbols,
            #[cfg(test)]
            resolve_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[cfg(test)]
    fn reset_resolve_calls(&self) {
        self.resolve_calls.store(0, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn resolve_calls(&self) -> usize {
        self.resolve_calls.load(Ordering::Relaxed)
    }
}

impl InlineReferenceSnapshot for FixtureReferenceSnapshot {
    fn identity(&self) -> u64 {
        1
    }

    fn generation(&self) -> u64 {
        1
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        #[cfg(test)]
        self.resolve_calls.fetch_add(1, Ordering::Relaxed);
        self.symbols
            .get(normalized)
            .cloned()
            .unwrap_or(InlineReferenceTarget {
                symbol_id: 0,
                presence_generation: 0,
                defined: false,
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CopyDestination {
    Logical,
    AtomicScratch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CopyInstruction {
    physical: Range<usize>,
    destination: CopyDestination,
}

#[derive(Debug)]
struct FixtureRecipe {
    source: String,
    logical: String,
    pieces: Vec<ProjectionPiece>,
    copy_instructions: Vec<CopyInstruction>,
    origin_runs: Vec<LogicalOriginRun>,
    physical: u64,
    logical_utf16: usize,
    reference_labels: Vec<String>,
}

impl FixtureRecipe {
    fn new() -> Self {
        Self {
            source: String::new(),
            logical: String::new(),
            pieces: Vec::new(),
            copy_instructions: Vec::new(),
            origin_runs: Vec::new(),
            physical: 0,
            logical_utf16: 0,
            reference_labels: Vec::new(),
        }
    }

    fn push_identity(&mut self, text: &str) {
        let physical_start = self.physical;
        let logical_start = u32::try_from(self.logical.len()).expect("bounded logical fixture");
        self.source.push_str(text);
        self.logical.push_str(text);
        let bytes = u64::try_from(text.len()).expect("bounded identity fixture");
        let utf16 = u64::try_from(text.encode_utf16().count()).expect("bounded identity fixture");
        assert_eq!(bytes, utf16, "probe identities are ASCII");
        self.pieces.push(ProjectionPiece::Identity {
            metric: crate::SerializedMetric { bytes, utf16 },
        });
        self.physical += bytes;
        self.logical_utf16 += usize::try_from(utf16).expect("bounded identity fixture");
        self.copy_instructions.push(CopyInstruction {
            physical: usize::try_from(physical_start).expect("bounded fixture")
                ..usize::try_from(self.physical).expect("bounded fixture"),
            destination: CopyDestination::Logical,
        });
        let logical_end = u32::try_from(self.logical.len()).expect("bounded fixture");
        if let Some(previous) = self.origin_runs.last_mut()
            && previous.kind == OriginRunKind::Identity
            && previous.logical.end == logical_start
            && previous.physical.end == physical_start
        {
            previous.logical.end = logical_end;
            previous.physical.end = self.physical;
        } else {
            self.origin_runs.push(LogicalOriginRun {
                logical: logical_start..logical_end,
                physical: physical_start..self.physical,
                kind: OriginRunKind::Identity,
            });
        }
    }

    fn push_hidden(&mut self, affinity: GreenAffinity) {
        let physical_start = self.physical;
        self.source.push('~');
        self.physical += 1;
        self.pieces.push(ProjectionPiece::Hidden {
            metric: crate::SerializedMetric { bytes: 1, utf16: 1 },
            affinity,
        });
        debug_assert_eq!(self.physical, physical_start + 1);
    }

    fn push_tab_atomic(&mut self) {
        let physical_start = self.physical;
        let logical_start = u32::try_from(self.logical.len()).expect("bounded logical fixture");
        self.source.push('\t');
        self.logical.push(' ');
        self.physical += 1;
        self.logical_utf16 += 1;
        self.pieces.push(ProjectionPiece::Atomic {
            physical_metric: crate::SerializedMetric { bytes: 1, utf16: 1 },
            projection: AtomicProjection::tab_to_spaces(1).expect("one-space tab projection"),
        });
        self.copy_instructions.push(CopyInstruction {
            physical: usize::try_from(physical_start).expect("bounded fixture")
                ..usize::try_from(self.physical).expect("bounded fixture"),
            destination: CopyDestination::AtomicScratch,
        });
        self.origin_runs.push(LogicalOriginRun {
            logical: logical_start..u32::try_from(self.logical.len()).expect("bounded fixture"),
            physical: physical_start..self.physical,
            kind: OriginRunKind::Atomic,
        });
    }

    fn push_virtual_lf(&mut self) {
        let logical_start = u32::try_from(self.logical.len()).expect("bounded logical fixture");
        self.logical.push('\n');
        self.logical_utf16 += 1;
        self.pieces.push(ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        });
        self.origin_runs.push(LogicalOriginRun {
            logical: logical_start..u32::try_from(self.logical.len()).expect("bounded fixture"),
            physical: self.physical..self.physical,
            kind: OriginRunKind::Virtual,
        });
    }
}

#[derive(Debug)]
pub struct InlineLivenessFixture {
    shape: InlineLivenessShape,
    source: LiveDocumentStore,
    arena: PageArena,
    green: SerializedGreenDocument,
    target: crate::GreenEnterCapability,
    logical: String,
    origin_map: LogicalOriginMap,
    copy_instructions: Vec<CopyInstruction>,
    metadata: InlineLivenessMetadata,
    references: FixtureReferenceSnapshot,
}

impl InlineLivenessFixture {
    #[must_use]
    pub fn new(shape: InlineLivenessShape) -> Self {
        let recipe = match shape {
            InlineLivenessShape::Fragmented8KiB => fragmented_recipe(),
            InlineLivenessShape::HiddenAtSegmentCap => hidden_recipe(HIDDEN_CAP_SEGMENTS),
            InlineLivenessShape::HiddenOverSegmentCap => hidden_recipe(HIDDEN_CAP_SEGMENTS + 1),
            InlineLivenessShape::ReferenceDense8KiB => {
                reference_dense_recipe(MAX_INLINE_FRAGMENT_BYTES)
            }
            InlineLivenessShape::ReferenceDenseOverInputCap => {
                reference_dense_recipe(MAX_INLINE_FRAGMENT_BYTES + 1)
            }
            InlineLivenessShape::FragmentedEnclosingEmphasis8KiB => {
                fragmented_enclosing_emphasis_recipe()
            }
        };
        let references = FixtureReferenceSnapshot::from_labels(&recipe.reference_labels);
        let source = LiveDocumentStore::new(&recipe.source, 8).expect("valid probe source");
        let source_view = source.query_source();
        let source_bytes = source_view.len_bytes();
        let source_utf16 = source_view.len_utf16();
        let source_revision = source_view.revision();
        let source_root = source_view.identity();

        let mut events = Vec::with_capacity(recipe.pieces.len() / PROGRAM_PIECES_PER_RUN + 4);
        events.push(GreenEvent::enter(
            BlockId(1),
            GreenKind::DOCUMENT,
            FactsEnvelope::empty(),
        ));
        events.push(GreenEvent::enter(
            BlockId(2),
            GreenKind::PARAGRAPH,
            FactsEnvelope::empty(),
        ));
        for (index, pieces) in recipe.pieces.chunks(PROGRAM_PIECES_PER_RUN).enumerate() {
            let program =
                ProjectionProgram::new(pieces.to_vec()).expect("page-bounded probe program");
            let physical = program.physical_metric();
            events.push(GreenEvent::Coverage(
                SourceProjectionRun::with_logical(
                    CoverageId(u64::try_from(index + 1).expect("bounded probe runs")),
                    physical.bytes,
                    physical.utf16,
                    0,
                    CoveragePart::CONTENT,
                    BlockId(2),
                    LogicalContribution::Program(program),
                )
                .expect("valid probe coverage"),
            ));
        }
        events.push(GreenEvent::exit(ClosedChildAggregate::default()));
        events.push(GreenEvent::exit(ClosedChildAggregate::default()));

        let mut arena = PageArena::new();
        let mut build_receipt = SerializedGreenBuildReceipt::default();
        let green = SerializedGreenDocument::build(
            &mut arena,
            SerializedGreenRootSpec {
                syntax_profile: crate::V3_COMMONMARK_SYNTAX_PROFILE,
                source_revision,
                source_root,
                source_bytes: u64::try_from(source_bytes).expect("bounded probe source"),
                source_utf16: u64::try_from(source_utf16).expect("bounded probe source"),
                grammar_revision: GrammarRevision(1),
                parse_generation: ParseGeneration(1),
                semantic_epoch: 1,
                known_bytes: 0..u64::try_from(source_bytes).expect("bounded probe source"),
            },
            events,
            &mut build_receipt,
        )
        .expect("valid packed-green probe");
        let target = green
            .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
            .expect("seek probe leaf")
            .open_path()
            .last()
            .expect("probe paragraph is physically open")
            .enter;
        assert_eq!(target.kind, GreenKind::PARAGRAPH);

        let source_bytes_copied = recipe
            .copy_instructions
            .iter()
            .map(|instruction| instruction.physical.len())
            .sum();
        let metadata = InlineLivenessMetadata {
            source_bytes,
            logical_bytes: recipe.logical.len(),
            logical_utf16: recipe.logical_utf16,
            logical_segments: recipe.pieces.len(),
            projection_program_runs: build_receipt.projection_program_pages_allocated,
            source_copy_ranges: recipe.copy_instructions.len(),
            source_bytes_copied,
            origin_runs: recipe.origin_runs.len(),
            reference_symbols: recipe.reference_labels.len(),
        };
        let origin_map = LogicalOriginMap {
            leaf_id: target.block.0,
            revision: source_revision.0,
            logical_len: u32::try_from(recipe.logical.len()).expect("bounded probe logical input"),
            runs: recipe.origin_runs,
        };

        Self {
            shape,
            source,
            arena,
            green,
            target,
            logical: recipe.logical,
            origin_map,
            copy_instructions: recipe.copy_instructions,
            metadata,
            references,
        }
    }

    #[must_use]
    pub const fn shape(&self) -> InlineLivenessShape {
        self.shape
    }

    #[must_use]
    pub const fn metadata(&self) -> InlineLivenessMetadata {
        self.metadata
    }

    #[must_use]
    pub fn sample_green_traversal(&self) -> u64 {
        let mut cursor = self
            .green
            .logical_cursor(&self.arena, self.target)
            .expect("valid probe cursor");
        let mut segments = 0_u64;
        while cursor
            .next_segment(&self.green, &self.arena)
            .expect("valid probe projection")
            .is_some()
        {
            segments += 1;
        }
        let receipt = cursor.receipt();
        segments
            ^ u64::try_from(receipt.projection_program_pages_decoded)
                .expect("bounded probe pages")
                .rotate_left(21)
    }

    #[must_use]
    pub fn sample_source_copy(&self) -> u64 {
        let source = self.source.query_source();
        let mut logical = Vec::with_capacity(self.metadata.source_bytes_copied);
        let mut atomic = Vec::with_capacity(2);
        let mut chunks = 0_usize;
        let mut copied = 0_usize;
        for instruction in &self.copy_instructions {
            let receipt = match instruction.destination {
                CopyDestination::Logical => source
                    .append_bounded_derived_range(
                        instruction.physical.clone(),
                        &mut logical,
                        MAX_INLINE_FRAGMENT_BYTES,
                    )
                    .expect("bounded logical probe copy"),
                CopyDestination::AtomicScratch => {
                    atomic.clear();
                    source
                        .append_bounded_derived_range(
                            instruction.physical.clone(),
                            &mut atomic,
                            instruction.physical.len(),
                        )
                        .expect("bounded atomic probe copy")
                }
            };
            chunks += receipt.chunks_visited;
            copied += receipt.bytes_copied;
        }
        u64::try_from(copied).expect("bounded probe copy")
            ^ u64::try_from(chunks)
                .expect("bounded probe chunks")
                .rotate_left(19)
    }

    #[must_use]
    pub fn parse_fragment(&self) -> InlineFragment {
        parse_inline_fragment(InlineFragmentRequest {
            logical: &self.logical,
            leaf_id: self.target.block.0,
            kind: InlineInputKind::Paragraph,
            profile: InlineProfile::CommonMark,
            reference_snapshot: &self.references,
            revision: self.source.query_source().revision().0,
            expected_revision: self.source.query_source().revision().0,
        })
        .expect("probe logical input is accepted by the bounded inline service")
    }

    #[must_use]
    pub fn sample_comrak(&self) -> u64 {
        let fragment = self.parse_fragment();
        u64::try_from(fragment.facts.len() + fragment.projection_facts.len())
            .expect("bounded probe facts")
            ^ u64::try_from(fragment.output_bytes())
                .expect("bounded probe output")
                .rotate_left(23)
    }

    #[must_use]
    pub fn sample_origin_composition(&self, fragment: &InlineFragment) -> u64 {
        let composed = self
            .origin_map
            .compose(fragment)
            .expect("probe origin map composes exactly");
        composed_digest(&composed)
    }

    #[must_use]
    pub fn composition_metadata(
        &self,
        fragment: &InlineFragment,
    ) -> InlineLivenessCompositionMetadata {
        let composed = self
            .origin_map
            .compose(fragment)
            .expect("probe origin map composes exactly");
        InlineLivenessCompositionMetadata {
            semantic_facts: composed.semantic_facts.len(),
            projection_facts: composed.projection_facts.len(),
            reference_dependencies: fragment.reference_dependencies.len(),
            mapped_physical_parts: composed
                .semantic_facts
                .iter()
                .chain(&composed.projection_facts)
                .map(|mapped| mapped.physical_parts.len())
                .sum(),
        }
    }

    #[must_use]
    pub fn start_job(&self) -> InlineLeafMaterializationJob {
        InlineLeafMaterializationJob::new_on_parser_worker(
            &self.green,
            &self.arena,
            self.source.query_source(),
            self.target,
        )
        .expect("valid probe job")
    }

    pub fn poll_job(
        &self,
        job: &mut InlineLeafMaterializationJob,
        fuel: InlineLeafMaterializationFuel,
    ) -> InlineLeafMaterializationProgress {
        job.poll(
            &self.green,
            &self.arena,
            self.source.query_source(),
            &self.references,
            fuel,
        )
        .expect("valid probe poll")
    }

    #[must_use]
    pub fn sample_full_materializer(&self) -> u64 {
        match derive_inline_leaf_presentation(
            &self.green,
            &self.arena,
            self.source.query_source(),
            self.target,
            &self.references,
        )
        .expect("valid probe materialization")
        {
            InlineLeafOutcome::Ready(ready) => {
                let receipt = ready.receipt();
                u64::try_from(ready.composed().semantic_facts.len()).expect("bounded probe facts")
                    ^ u64::try_from(receipt.logical_segments_visited)
                        .expect("bounded probe segments")
                        .rotate_left(17)
                    ^ u64::try_from(receipt.source_bytes_copied)
                        .expect("bounded probe source")
                        .rotate_left(37)
            }
            InlineLeafOutcome::Unknown(unknown) => {
                (1_u64 << 63)
                    | u64::try_from(unknown.receipt().logical_segments_visited)
                        .expect("bounded probe segments")
            }
        }
    }
}

fn composed_digest(composed: &ComposedInlineFragment) -> u64 {
    let physical_parts = composed
        .semantic_facts
        .iter()
        .chain(&composed.projection_facts)
        .map(|mapped| mapped.physical_parts.len())
        .sum::<usize>();
    u64::try_from(composed.semantic_facts.len() + composed.projection_facts.len())
        .expect("bounded probe facts")
        ^ u64::try_from(physical_parts)
            .expect("bounded probe physical parts")
            .rotate_left(29)
}

fn fragmented_recipe() -> FixtureRecipe {
    let mut recipe = FixtureRecipe::new();
    while recipe.logical.len() + 3 <= FRAGMENTED_LOGICAL_BYTES {
        recipe.push_identity("a");
        recipe.push_hidden(GreenAffinity::Downstream);
        recipe.push_tab_atomic();
        recipe.push_virtual_lf();
    }
    let remainder = FRAGMENTED_LOGICAL_BYTES - recipe.logical.len();
    if remainder != 0 {
        recipe.push_identity(&"x".repeat(remainder));
    }
    assert_eq!(recipe.logical.len(), FRAGMENTED_LOGICAL_BYTES);
    recipe
}

fn hidden_recipe(segments: usize) -> FixtureRecipe {
    let mut recipe = FixtureRecipe::new();
    for index in 0..segments {
        recipe.push_hidden(if index % 2 == 0 {
            GreenAffinity::Upstream
        } else {
            GreenAffinity::Downstream
        });
    }
    recipe
}

fn reference_dense_recipe(logical_bytes: usize) -> FixtureRecipe {
    let reference_count = logical_bytes / REFERENCE_TOKEN_BYTES;
    let mut logical = String::with_capacity(logical_bytes);
    let mut labels = Vec::with_capacity(reference_count);
    for index in 0..reference_count {
        let label = reference_label(index);
        logical.push('[');
        logical.push_str(&label);
        logical.push_str("] ");
        labels.push(label);
    }
    logical.push_str(&"x".repeat(logical_bytes - logical.len()));
    assert_eq!(logical.len(), logical_bytes);

    let mut recipe = FixtureRecipe::new();
    recipe.reference_labels = labels;
    recipe.push_identity(&logical);
    recipe
}

fn fragmented_enclosing_emphasis_recipe() -> FixtureRecipe {
    let mut recipe = FixtureRecipe::new();
    recipe.push_identity("*");
    for index in 0..(MAX_INLINE_FRAGMENT_BYTES - 2) {
        recipe.push_identity("a");
        if index + 1 != MAX_INLINE_FRAGMENT_BYTES - 2 {
            recipe.push_hidden(GreenAffinity::Downstream);
        }
    }
    recipe.push_identity("*");
    assert_eq!(recipe.logical.len(), MAX_INLINE_FRAGMENT_BYTES);
    assert!(recipe.pieces.len() <= HIDDEN_CAP_SEGMENTS);
    recipe
}

fn reference_label(mut index: usize) -> String {
    let mut bytes = [b'a'; REFERENCE_LABEL_BYTES];
    for byte in bytes.iter_mut().rev() {
        *byte = b'a' + u8::try_from(index % 26).expect("base-26 digit");
        index /= 26;
    }
    assert_eq!(index, 0, "reference fixture exhausted fixed-width labels");
    String::from_utf8(bytes.to_vec()).expect("reference labels are ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InlineLeafMaterializationFuel, InlineLeafMaterializationPhase,
        InlineLeafMaterializationProgress, InlineLeafUnknownReason,
    };

    #[test]
    fn fragmented_fixture_exercises_the_full_bounded_path() {
        let fixture = InlineLivenessFixture::new(InlineLivenessShape::Fragmented8KiB);
        assert_eq!(
            fixture.metadata(),
            InlineLivenessMetadata {
                source_bytes: 8_192,
                logical_bytes: 8_192,
                logical_utf16: 8_192,
                logical_segments: 10_921,
                projection_program_runs: 14,
                source_copy_ranges: 5_461,
                source_bytes_copied: 5_462,
                origin_runs: 8_191,
                reference_symbols: 0,
            }
        );
        let InlineLeafOutcome::Ready(ready) = derive_inline_leaf_presentation(
            &fixture.green,
            &fixture.arena,
            fixture.source.query_source(),
            fixture.target,
            &fixture.references,
        )
        .unwrap() else {
            panic!("bounded fragmented fixture must be Ready");
        };
        assert_eq!(ready.receipt().logical_segments_visited, 10_921);
        assert_eq!(ready.receipt().source_bytes_copied, 5_462);
        assert_eq!(ready.receipt().inline_service_calls, 1);
    }

    #[test]
    fn hidden_cap_is_ready_and_one_more_segment_fails_before_comrak() {
        let at_cap = InlineLivenessFixture::new(InlineLivenessShape::HiddenAtSegmentCap);
        assert_eq!(at_cap.metadata().logical_segments, HIDDEN_CAP_SEGMENTS);
        assert_eq!(at_cap.metadata().logical_bytes, 0);
        assert!(matches!(
            derive_inline_leaf_presentation(
                &at_cap.green,
                &at_cap.arena,
                at_cap.source.query_source(),
                at_cap.target,
                &at_cap.references,
            )
            .unwrap(),
            InlineLeafOutcome::Ready(_)
        ));

        let over_cap = InlineLivenessFixture::new(InlineLivenessShape::HiddenOverSegmentCap);
        let InlineLeafOutcome::Unknown(unknown) = derive_inline_leaf_presentation(
            &over_cap.green,
            &over_cap.arena,
            over_cap.source.query_source(),
            over_cap.target,
            &over_cap.references,
        )
        .unwrap() else {
            panic!("one segment beyond the shape cap must fail closed");
        };
        assert_eq!(
            unknown.reason(),
            &InlineLeafUnknownReason::ProjectionSegmentCapExceeded {
                cap: HIDDEN_CAP_SEGMENTS,
            }
        );
        assert_eq!(unknown.receipt().logical_segments_visited, 16_385);
        assert_eq!(unknown.receipt().source_bytes_copied, 0);
        assert_eq!(unknown.receipt().inline_service_calls, 0);
    }

    #[test]
    fn fuelled_job_is_differentially_identical_and_yields_at_declared_boundaries() {
        let fixture = InlineLivenessFixture::new(InlineLivenessShape::Fragmented8KiB);
        let synchronous = derive_inline_leaf_presentation(
            &fixture.green,
            &fixture.arena,
            fixture.source.query_source(),
            fixture.target,
            &fixture.references,
        )
        .unwrap();
        let fuel = InlineLeafMaterializationFuel::new(512).unwrap();
        let mut job = fixture.start_job();
        let mut turns = 0;
        let mut projection_pending = 0;
        let mut inline_pending = 0;
        let mut reference_pending = 0;
        let mut origin_pending = 0;
        let fuelled = loop {
            turns += 1;
            match fixture.poll_job(&mut job, fuel) {
                InlineLeafMaterializationProgress::Pending { phase, .. } => match phase {
                    InlineLeafMaterializationPhase::Projection => projection_pending += 1,
                    InlineLeafMaterializationPhase::InlineService => inline_pending += 1,
                    InlineLeafMaterializationPhase::ReferenceValidation => reference_pending += 1,
                    InlineLeafMaterializationPhase::OriginComposition => origin_pending += 1,
                },
                InlineLeafMaterializationProgress::Complete(outcome) => break outcome,
            }
        };

        assert_eq!(fuelled, synchronous);
        assert_eq!(turns, 35);
        assert_eq!(projection_pending, 21);
        assert_eq!(inline_pending, 1);
        assert_eq!(reference_pending, 1);
        assert_eq!(origin_pending, 11);
    }

    #[test]
    fn reference_dense_cap_is_differentially_exact_and_over_cap_fails_before_comrak() {
        let fixture = InlineLivenessFixture::new(InlineLivenessShape::ReferenceDense8KiB);
        assert_eq!(fixture.metadata().logical_bytes, MAX_INLINE_FRAGMENT_BYTES);
        assert_eq!(fixture.metadata().logical_segments, 1);
        assert_eq!(fixture.metadata().reference_symbols, 1_365);
        let fragment = fixture.parse_fragment();
        assert_eq!(fragment.reference_dependencies.len(), 1_365);
        assert!(
            fragment
                .reference_dependencies
                .iter()
                .all(|dependency| dependency.resolved)
        );

        let synchronous = derive_inline_leaf_presentation(
            &fixture.green,
            &fixture.arena,
            fixture.source.query_source(),
            fixture.target,
            &fixture.references,
        )
        .unwrap();
        fixture.references.reset_resolve_calls();
        let fuel = InlineLeafMaterializationFuel::new(512).unwrap();
        let mut job = fixture.start_job();
        let mut turns = 0;
        let fuelled = loop {
            turns += 1;
            match fixture.poll_job(&mut job, fuel) {
                InlineLeafMaterializationProgress::Pending { phase, .. } => match phase {
                    InlineLeafMaterializationPhase::InlineService => {
                        assert_eq!(fixture.references.resolve_calls(), 0);
                    }
                    InlineLeafMaterializationPhase::ReferenceValidation => {
                        assert!((1_365..=2_389).contains(&fixture.references.resolve_calls()));
                    }
                    InlineLeafMaterializationPhase::OriginComposition => {
                        assert_eq!(fixture.references.resolve_calls(), 2_730);
                    }
                    InlineLeafMaterializationPhase::Projection => {}
                },
                InlineLeafMaterializationProgress::Complete(outcome) => break outcome,
            }
        };
        assert_eq!(fuelled, synchronous);
        assert_eq!(turns, 19);
        assert_eq!(fixture.references.resolve_calls(), 2_730);

        let over_cap = InlineLivenessFixture::new(InlineLivenessShape::ReferenceDenseOverInputCap);
        let InlineLeafOutcome::Unknown(unknown) = derive_inline_leaf_presentation(
            &over_cap.green,
            &over_cap.arena,
            over_cap.source.query_source(),
            over_cap.target,
            &over_cap.references,
        )
        .unwrap() else {
            panic!("reference-dense input over the byte cap must fail closed");
        };
        assert_eq!(
            unknown.reason(),
            &InlineLeafUnknownReason::OverInputCap {
                observed_logical_end: u64::try_from(MAX_INLINE_FRAGMENT_BYTES + 1).unwrap(),
                cap: MAX_INLINE_FRAGMENT_BYTES,
            }
        );
        assert_eq!(unknown.receipt().source_bytes_copied, 0);
        assert_eq!(unknown.receipt().inline_service_calls, 0);
    }

    #[test]
    fn enclosing_fact_falsifier_spans_thousands_of_origin_runs() {
        let fixture =
            InlineLivenessFixture::new(InlineLivenessShape::FragmentedEnclosingEmphasis8KiB);
        assert_eq!(fixture.metadata().logical_bytes, MAX_INLINE_FRAGMENT_BYTES);
        assert_eq!(fixture.metadata().logical_segments, 16_381);
        assert_eq!(fixture.metadata().origin_runs, 8_190);
        let fragment = fixture.parse_fragment();
        assert!(
            fragment
                .facts
                .iter()
                .any(|fact| usize::try_from(fact.logical_len).unwrap() == MAX_INLINE_FRAGMENT_BYTES)
        );
        let composed = fixture.origin_map.compose(&fragment).unwrap();
        assert_eq!(
            composed
                .semantic_facts
                .iter()
                .chain(&composed.projection_facts)
                .map(|mapped| mapped.physical_parts.len())
                .max(),
            Some(8_190)
        );

        let synchronous = derive_inline_leaf_presentation(
            &fixture.green,
            &fixture.arena,
            fixture.source.query_source(),
            fixture.target,
            &fixture.references,
        )
        .unwrap();
        let fuel = InlineLeafMaterializationFuel::new(512).unwrap();
        let mut job = fixture.start_job();
        let mut turns = 0;
        let fuelled = loop {
            turns += 1;
            if let InlineLeafMaterializationProgress::Complete(outcome) =
                fixture.poll_job(&mut job, fuel)
            {
                break outcome;
            }
        };
        assert_eq!(fuelled, synchronous);
        assert_eq!(turns, 35);
    }
}

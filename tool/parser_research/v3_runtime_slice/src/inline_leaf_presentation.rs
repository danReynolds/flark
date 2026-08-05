//! Bounded inline presentation derived from one authenticated packed-green leaf.
//!
//! This module is a read-side join, not a second Markdown parser. Packed green
//! supplies the terminal kind and logical projection, a borrowed current Crop
//! view supplies only the exact physical bytes named by that projection, and
//! Comrak's bounded inline service remains the sole inline grammar authority.
//! Expected inability to produce exact presentation is returned as `Unknown`;
//! corrupt storage or a violated join invariant remains an error.

use std::fmt;
use std::ops::Range;

use flark_comrak_inline_fragment_gate::{
    ComposedInlineFragment, InlineFragment, InlineFragmentError, InlineFragmentRequest,
    InlineInputKind, InlineProfile, InlineReferenceSnapshot, LogicalOriginMap, LogicalOriginRun,
    MAX_INLINE_FRAGMENT_BYTES, MappedInlineFact, OriginMapError, OriginRunKind,
    parse_inline_fragment,
};

use crate::source::{DerivedSourceReadError, DerivedSourceReadReceipt};
use crate::{
    AtomicProjectionKind, BlockId, GrammarRevision, GreenEnterCapability, GreenHeadingOpenFacts,
    GreenHeadingStyle, GreenKind, LogicalChannel, LogicalSegmentMapping, PageArena,
    ParseGeneration, SerializedGreenDocument, SerializedGreenError, SerializedGreenManifestId,
    SourceQueryView, SourceRevision, SourceRootId, VirtualProjectionKind,
};

/// Current V3 profile registry entry. The exact V3 driver currently admits
/// `CommonMark` only; unknown values fail closed until a shared profile registry
/// explicitly adds `GFM`.
pub const V3_COMMONMARK_SYNTAX_PROFILE: u64 = 1;

/// Independent shape ceiling for a projection with little or no logical text.
/// The inline byte cap alone cannot bound a leaf made mostly from hidden runs.
pub const MAX_INLINE_PROJECTION_SEGMENTS: usize = 16 * 1024;

/// Execution lane required by the live-editor architecture. Materialization
/// never runs on Flutter's UI isolate or the browser main thread; callers drive
/// it from the dedicated parser isolate/native worker or a Web Worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineLeafExecutionLane {
    ParserIsolateOrWebWorker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineSourceObservation {
    revision: SourceRevision,
    root: SourceRootId,
    bytes: u64,
    utf16: u64,
}

impl InlineSourceObservation {
    #[must_use]
    pub const fn revision(self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub const fn root(self) -> SourceRootId {
        self.root
    }

    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn utf16(self) -> u64 {
        self.utf16
    }
}

/// Exact structural/source identity to which one derived result belongs.
/// Fields stay private so consumers cannot construct an adoptable binding from
/// echoed scalar ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineLeafBinding {
    manifest: SerializedGreenManifestId,
    target: GreenEnterCapability,
    source: InlineSourceObservation,
    syntax_profile: u64,
    grammar: GrammarRevision,
    generation: ParseGeneration,
    semantic_epoch: u64,
}

impl InlineLeafBinding {
    #[must_use]
    pub const fn manifest(self) -> SerializedGreenManifestId {
        self.manifest
    }

    #[must_use]
    pub const fn target(self) -> GreenEnterCapability {
        self.target
    }

    #[must_use]
    pub const fn block(self) -> BlockId {
        self.target.block
    }

    #[must_use]
    pub const fn source(self) -> InlineSourceObservation {
        self.source
    }

    #[must_use]
    pub const fn syntax_profile(self) -> u64 {
        self.syntax_profile
    }

    #[must_use]
    pub const fn grammar(self) -> GrammarRevision {
        self.grammar
    }

    #[must_use]
    pub const fn generation(self) -> ParseGeneration {
        self.generation
    }

    #[must_use]
    pub const fn semantic_epoch(self) -> u64 {
        self.semantic_epoch
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineLeafMaterializationReceipt {
    pub logical_segments_visited: usize,
    pub green_coverage_runs_visited: usize,
    pub projection_program_pages_decoded: usize,
    pub source_ranges_read: usize,
    pub source_chunks_visited: usize,
    pub source_bytes_copied: usize,
    pub maximum_source_chunk_bytes: usize,
    pub origin_runs: usize,
    pub inline_service_calls: usize,
    pub reference_dependencies_revalidated: usize,
    pub origin_facts_mapped: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineLeafUnknownReason {
    StaleSource {
        actual: InlineSourceObservation,
    },
    IncompleteGreenCoverage {
        known_bytes: Range<u64>,
        source_bytes: u64,
    },
    UnsupportedSyntaxProfile(u64),
    UnsupportedLeafKind(GreenKind),
    OverInputCap {
        observed_logical_end: u64,
        cap: usize,
    },
    ProjectionSegmentCapExceeded {
        cap: usize,
    },
    StaleReferenceDependency {
        normalized_label: String,
        expected_symbol_id: u64,
        actual_symbol_id: u64,
        expected_presence_generation: u64,
        actual_presence_generation: u64,
        expected_resolved: bool,
        actual_resolved: bool,
    },
    InlineServiceRejected(InlineFragmentError),
    OriginCompositionRejected(OriginMapError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownInlineLeaf {
    binding: InlineLeafBinding,
    reason: InlineLeafUnknownReason,
    receipt: InlineLeafMaterializationReceipt,
}

impl UnknownInlineLeaf {
    #[must_use]
    pub const fn binding(&self) -> InlineLeafBinding {
        self.binding
    }

    #[must_use]
    pub const fn reason(&self) -> &InlineLeafUnknownReason {
        &self.reason
    }

    #[must_use]
    pub const fn receipt(&self) -> InlineLeafMaterializationReceipt {
        self.receipt
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyInlineLeaf {
    binding: InlineLeafBinding,
    input_kind: InlineInputKind,
    logical: String,
    origin_map: LogicalOriginMap,
    fragment: InlineFragment,
    composed: ComposedInlineFragment,
    receipt: InlineLeafMaterializationReceipt,
}

impl ReadyInlineLeaf {
    #[must_use]
    pub const fn binding(&self) -> InlineLeafBinding {
        self.binding
    }

    #[must_use]
    pub const fn input_kind(&self) -> InlineInputKind {
        self.input_kind
    }

    #[must_use]
    pub fn logical(&self) -> &str {
        &self.logical
    }

    #[must_use]
    pub const fn origin_map(&self) -> &LogicalOriginMap {
        &self.origin_map
    }

    #[must_use]
    pub const fn fragment(&self) -> &InlineFragment {
        &self.fragment
    }

    #[must_use]
    pub const fn composed(&self) -> &ComposedInlineFragment {
        &self.composed
    }

    #[must_use]
    pub const fn receipt(&self) -> InlineLeafMaterializationReceipt {
        self.receipt
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineLeafOutcome {
    Ready(ReadyInlineLeaf),
    Unknown(UnknownInlineLeaf),
}

#[derive(Debug)]
pub enum InlineLeafMaterializationError {
    Green(SerializedGreenError),
    Source(crate::SourceError),
    Invariant(&'static str),
}

impl From<SerializedGreenError> for InlineLeafMaterializationError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl fmt::Display for InlineLeafMaterializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Green(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
            Self::Invariant(message) => write!(formatter, "inline leaf join invariant: {message}"),
        }
    }
}

impl std::error::Error for InlineLeafMaterializationError {}

/// Cooperative work allowance for one materialization poll. One unit admits
/// one packed-green projection segment, one reference dependency revalidation,
/// or one origin fact mapping. Only bounded Comrak remains atomic, and it
/// occupies its own parser-worker poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineLeafMaterializationFuel {
    work_units: std::num::NonZeroUsize,
}

impl InlineLeafMaterializationFuel {
    #[must_use]
    pub const fn new(work_units: usize) -> Option<Self> {
        match std::num::NonZeroUsize::new(work_units) {
            Some(work_units) => Some(Self { work_units }),
            None => None,
        }
    }

    #[must_use]
    pub const fn work_units(self) -> usize {
        self.work_units.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineLeafMaterializationPhase {
    Projection,
    InlineService,
    ReferenceValidation,
    OriginComposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineLeafMaterializationProgress {
    Pending {
        phase: InlineLeafMaterializationPhase,
        receipt: InlineLeafMaterializationReceipt,
    },
    Complete(InlineLeafOutcome),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineLeafJobPhase {
    Complete,
    Projection,
    InlineService,
    ReferenceValidation,
    OriginComposition,
    Taken,
}

/// Resumable read-side join for one authenticated packed-green terminal.
///
/// The job owns no source root and no green pages. Every poll receives the
/// current borrowed source view and immutable green document; a source edit or
/// document substitution therefore fails closed instead of letting a job pin
/// or publish stale state.
#[derive(Debug)]
pub struct InlineLeafMaterializationJob {
    binding: InlineLeafBinding,
    input_kind: Option<InlineInputKind>,
    phase: InlineLeafJobPhase,
    complete: Option<InlineLeafOutcome>,
    logical_cursor: Option<crate::GreenLogicalCursor>,
    logical_bytes: Vec<u8>,
    logical_utf16: u64,
    origin_runs: Vec<LogicalOriginRun>,
    atomic_source: Vec<u8>,
    logical: Option<String>,
    origin_map: Option<LogicalOriginMap>,
    fragment: Option<InlineFragment>,
    reference_dependency_index: usize,
    semantic_fact_index: usize,
    projection_fact_index: usize,
    mapped_semantic_facts: Vec<MappedInlineFact>,
    mapped_projection_facts: Vec<MappedInlineFact>,
    supersession_checks: usize,
    receipt: InlineLeafMaterializationReceipt,
}

impl InlineLeafMaterializationJob {
    /// Creates a job for the dedicated parser isolate/native worker or Web
    /// Worker. Product UI code must not call or poll this job directly.
    pub fn new_on_parser_worker(
        document: &SerializedGreenDocument,
        arena: &PageArena,
        source: SourceQueryView<'_>,
        target: GreenEnterCapability,
    ) -> Result<Self, InlineLeafMaterializationError> {
        let logical_cursor = document.logical_cursor(arena, target)?;
        let target_frame = logical_cursor.target_frame()?.clone();
        let manifest = document.manifest_descriptor(arena)?;
        let binding = InlineLeafBinding {
            manifest: manifest.manifest,
            target: target_frame.enter,
            source: InlineSourceObservation {
                revision: manifest.source_revision,
                root: manifest.source_root,
                bytes: manifest.source_bytes,
                utf16: manifest.source_utf16,
            },
            syntax_profile: manifest.syntax_profile,
            grammar: manifest.grammar_revision,
            generation: manifest.parse_generation,
            semantic_epoch: manifest.semantic_epoch,
        };
        let mut job = Self {
            binding,
            input_kind: None,
            phase: InlineLeafJobPhase::Projection,
            complete: None,
            logical_cursor: Some(logical_cursor),
            logical_bytes: Vec::new(),
            logical_utf16: 0,
            origin_runs: Vec::new(),
            atomic_source: Vec::with_capacity(2),
            logical: None,
            origin_map: None,
            fragment: None,
            reference_dependency_index: 0,
            semantic_fact_index: 0,
            projection_fact_index: 0,
            mapped_semantic_facts: Vec::new(),
            mapped_projection_facts: Vec::new(),
            supersession_checks: 0,
            receipt: InlineLeafMaterializationReceipt::default(),
        };
        let actual = source_observation(source)?;
        if actual != binding.source {
            job.finish_unknown(InlineLeafUnknownReason::StaleSource { actual });
            return Ok(job);
        }
        if manifest.known_bytes_start != 0 || manifest.known_bytes_end != manifest.source_bytes {
            job.finish_unknown(InlineLeafUnknownReason::IncompleteGreenCoverage {
                known_bytes: manifest.known_bytes_start..manifest.known_bytes_end,
                source_bytes: manifest.source_bytes,
            });
            return Ok(job);
        }
        if manifest.syntax_profile != V3_COMMONMARK_SYNTAX_PROFILE {
            job.finish_unknown(InlineLeafUnknownReason::UnsupportedSyntaxProfile(
                manifest.syntax_profile,
            ));
            return Ok(job);
        }
        job.input_kind = match input_kind(&target_frame) {
            Ok(kind) => Some(kind),
            Err(InputKindError::Green(error)) => return Err(error.into()),
            Err(InputKindError::Unsupported(kind)) => {
                job.finish_unknown(InlineLeafUnknownReason::UnsupportedLeafKind(kind));
                return Ok(job);
            }
        };
        Ok(job)
    }

    #[must_use]
    pub const fn binding(&self) -> InlineLeafBinding {
        self.binding
    }

    #[must_use]
    pub const fn receipt(&self) -> InlineLeafMaterializationReceipt {
        self.receipt
    }

    /// The only supported execution lane. The enum intentionally has no UI or
    /// browser-main-thread variant.
    #[must_use]
    pub const fn execution_lane(&self) -> InlineLeafExecutionLane {
        InlineLeafExecutionLane::ParserIsolateOrWebWorker
    }

    #[must_use]
    pub const fn supersession_checks(&self) -> usize {
        self.supersession_checks
    }

    pub fn poll(
        &mut self,
        document: &SerializedGreenDocument,
        arena: &PageArena,
        source: SourceQueryView<'_>,
        references: &dyn InlineReferenceSnapshot,
        fuel: InlineLeafMaterializationFuel,
    ) -> Result<InlineLeafMaterializationProgress, InlineLeafMaterializationError> {
        if self.phase == InlineLeafJobPhase::Taken {
            return Err(InlineLeafMaterializationError::Invariant(
                "completed inline materialization job polled again",
            ));
        }
        if self.phase == InlineLeafJobPhase::Complete {
            return self.take_complete();
        }
        if !self.validate_supersession(document, source)? {
            return self.take_complete();
        }

        let mut remaining = fuel.work_units();
        loop {
            match self.phase {
                InlineLeafJobPhase::Complete => return self.take_complete(),
                InlineLeafJobPhase::Projection => {
                    if remaining == 0 {
                        return Ok(self.pending(InlineLeafMaterializationPhase::Projection));
                    }
                    remaining -= 1;
                    if self.poll_projection_unit(document, arena, source)? {
                        self.phase = InlineLeafJobPhase::InlineService;
                        return Ok(self.pending(InlineLeafMaterializationPhase::InlineService));
                    }
                }
                InlineLeafJobPhase::InlineService => {
                    if remaining == 0 {
                        return Ok(self.pending(InlineLeafMaterializationPhase::InlineService));
                    }
                    self.poll_inline_service(document, source, references)?;
                    if self.phase == InlineLeafJobPhase::Complete {
                        return self.take_complete();
                    }
                    // Comrak is bounded but atomic. Isolate it in its own turn
                    // so no accumulated projection/revalidation/composition
                    // work can hide its latency.
                    return Ok(self.pending(InlineLeafMaterializationPhase::ReferenceValidation));
                }
                InlineLeafJobPhase::ReferenceValidation => {
                    if remaining == 0 {
                        return Ok(
                            self.pending(InlineLeafMaterializationPhase::ReferenceValidation)
                        );
                    }
                    remaining -= 1;
                    if self.poll_reference_dependency(references)? {
                        self.prepare_origin_composition();
                        if self.phase == InlineLeafJobPhase::Complete {
                            return self.take_complete();
                        }
                        // Keep the reference and origin distributions distinct;
                        // the next poll starts fact mapping with fresh fuel.
                        return Ok(self.pending(InlineLeafMaterializationPhase::OriginComposition));
                    }
                }
                InlineLeafJobPhase::OriginComposition => {
                    if remaining == 0 {
                        return Ok(self.pending(InlineLeafMaterializationPhase::OriginComposition));
                    }
                    remaining -= 1;
                    match self.poll_origin_fact() {
                        Ok(true) => {
                            let composed = ComposedInlineFragment {
                                semantic_facts: std::mem::take(&mut self.mapped_semantic_facts),
                                projection_facts: std::mem::take(&mut self.mapped_projection_facts),
                            };
                            self.finish_ready(composed)?;
                        }
                        Ok(false) => {}
                        Err(error) => self.finish_unknown(
                            InlineLeafUnknownReason::OriginCompositionRejected(error),
                        ),
                    }
                }
                InlineLeafJobPhase::Taken => {
                    return Err(InlineLeafMaterializationError::Invariant(
                        "inline materialization job entered Taken while polling",
                    ));
                }
            }
        }
    }

    fn pending(&self, phase: InlineLeafMaterializationPhase) -> InlineLeafMaterializationProgress {
        InlineLeafMaterializationProgress::Pending {
            phase,
            receipt: self.receipt,
        }
    }

    fn validate_supersession(
        &mut self,
        document: &SerializedGreenDocument,
        source: SourceQueryView<'_>,
    ) -> Result<bool, InlineLeafMaterializationError> {
        self.supersession_checks = self.supersession_checks.checked_add(1).ok_or(
            InlineLeafMaterializationError::Invariant("supersession check count overflow"),
        )?;
        if document.manifest_id() != self.binding.manifest {
            return Err(SerializedGreenError::StaleCursor.into());
        }
        let actual = source_observation(source)?;
        if actual != self.binding.source {
            self.finish_unknown(InlineLeafUnknownReason::StaleSource { actual });
            return Ok(false);
        }
        Ok(true)
    }

    fn take_complete(
        &mut self,
    ) -> Result<InlineLeafMaterializationProgress, InlineLeafMaterializationError> {
        let outcome = self
            .complete
            .take()
            .ok_or(InlineLeafMaterializationError::Invariant(
                "completed inline materialization job lost its outcome",
            ))?;
        self.phase = InlineLeafJobPhase::Taken;
        Ok(InlineLeafMaterializationProgress::Complete(outcome))
    }

    fn finish_unknown(&mut self, reason: InlineLeafUnknownReason) {
        self.complete = Some(unknown(self.binding, reason, self.receipt));
        self.phase = InlineLeafJobPhase::Complete;
    }

    fn poll_projection_unit(
        &mut self,
        document: &SerializedGreenDocument,
        arena: &PageArena,
        source: SourceQueryView<'_>,
    ) -> Result<bool, InlineLeafMaterializationError> {
        let cursor =
            self.logical_cursor
                .as_mut()
                .ok_or(InlineLeafMaterializationError::Invariant(
                    "projection phase lost its green cursor",
                ))?;
        let Some(segment) = cursor.next_segment(document, arena)? else {
            apply_green_receipt(&mut self.receipt, cursor.receipt());
            self.logical_cursor = None;
            self.finish_projection()?;
            return Ok(true);
        };
        self.receipt.logical_segments_visited =
            self.receipt.logical_segments_visited.checked_add(1).ok_or(
                InlineLeafMaterializationError::Invariant("projection segment count overflow"),
            )?;
        if self.receipt.logical_segments_visited > MAX_INLINE_PROJECTION_SEGMENTS {
            apply_green_receipt(&mut self.receipt, cursor.receipt());
            self.finish_unknown(InlineLeafUnknownReason::ProjectionSegmentCapExceeded {
                cap: MAX_INLINE_PROJECTION_SEGMENTS,
            });
            return Ok(false);
        }
        validate_segment_prefix(
            &segment,
            self.binding.target,
            self.logical_bytes.len(),
            self.logical_utf16,
        )?;
        let declared_end = usize::try_from(segment.logical_byte_range.end).map_err(|_| {
            InlineLeafMaterializationError::Invariant("logical byte endpoint exceeds usize")
        })?;
        if declared_end > MAX_INLINE_FRAGMENT_BYTES {
            apply_green_receipt(&mut self.receipt, cursor.receipt());
            self.finish_unknown(InlineLeafUnknownReason::OverInputCap {
                observed_logical_end: segment.logical_byte_range.end,
                cap: MAX_INLINE_FRAGMENT_BYTES,
            });
            return Ok(false);
        }
        self.materialize_segment(source, segment)?;
        Ok(false)
    }

    fn materialize_segment(
        &mut self,
        source: SourceQueryView<'_>,
        segment: crate::GreenLogicalSegment,
    ) -> Result<(), InlineLeafMaterializationError> {
        match segment.mapping {
            LogicalSegmentMapping::ExactIdentity => {
                let physical = usize_range(&segment.byte_range)?;
                let logical_length = segment
                    .logical_byte_range
                    .end
                    .checked_sub(segment.logical_byte_range.start)
                    .ok_or(InlineLeafMaterializationError::Invariant(
                        "identity logical range reversed",
                    ))?;
                if u64::try_from(physical.len()).ok() != Some(logical_length) {
                    return Err(InlineLeafMaterializationError::Invariant(
                        "identity physical and logical byte lengths differ",
                    ));
                }
                let start = self.logical_bytes.len();
                let read = source
                    .append_bounded_derived_range(
                        physical,
                        &mut self.logical_bytes,
                        MAX_INLINE_FRAGMENT_BYTES,
                    )
                    .map_err(map_source_read_error)?;
                merge_source_receipt(&mut self.receipt, read)?;
                let appended = std::str::from_utf8(&self.logical_bytes[start..]).map_err(|_| {
                    InlineLeafMaterializationError::Invariant(
                        "identity projection is not complete UTF-8",
                    )
                })?;
                self.logical_utf16 = self
                    .logical_utf16
                    .checked_add(u64::try_from(appended.encode_utf16().count()).map_err(|_| {
                        InlineLeafMaterializationError::Invariant(
                            "identity UTF-16 length exceeds u64",
                        )
                    })?)
                    .ok_or(InlineLeafMaterializationError::Invariant(
                        "logical UTF-16 length overflow",
                    ))?;
                push_origin_run(
                    &mut self.origin_runs,
                    logical_range_u32(&segment.logical_byte_range)?,
                    segment.byte_range,
                    OriginRunKind::Identity,
                );
            }
            LogicalSegmentMapping::Hidden { .. } => {
                if !segment.logical_byte_range.is_empty() || !segment.logical_utf16_range.is_empty()
                {
                    return Err(InlineLeafMaterializationError::Invariant(
                        "hidden projection contributes logical text",
                    ));
                }
            }
            LogicalSegmentMapping::AtomicAmbiguity { transform } => {
                self.atomic_source.clear();
                let physical = usize_range(&segment.byte_range)?;
                let physical_len = segment
                    .byte_range
                    .end
                    .checked_sub(segment.byte_range.start)
                    .ok_or(InlineLeafMaterializationError::Invariant(
                        "atomic physical range reversed",
                    ))?;
                let read = source
                    .append_bounded_derived_range(
                        physical,
                        &mut self.atomic_source,
                        usize::try_from(physical_len).map_err(|_| {
                            InlineLeafMaterializationError::Invariant(
                                "atomic physical length exceeds usize",
                            )
                        })?,
                    )
                    .map_err(map_source_read_error)?;
                merge_source_receipt(&mut self.receipt, read)?;
                self.logical_utf16 = self
                    .logical_utf16
                    .checked_add(append_atomic_replacement(
                        transform,
                        &self.atomic_source,
                        &mut self.logical_bytes,
                    )?)
                    .ok_or(InlineLeafMaterializationError::Invariant(
                        "logical UTF-16 length overflow",
                    ))?;
                push_origin_run(
                    &mut self.origin_runs,
                    logical_range_u32(&segment.logical_byte_range)?,
                    segment.byte_range,
                    OriginRunKind::Atomic,
                );
            }
            LogicalSegmentMapping::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            } => {
                self.logical_bytes.push(b'\n');
                self.logical_utf16 = self.logical_utf16.checked_add(1).ok_or(
                    InlineLeafMaterializationError::Invariant("logical UTF-16 length overflow"),
                )?;
                push_origin_run(
                    &mut self.origin_runs,
                    logical_range_u32(&segment.logical_byte_range)?,
                    segment.byte_range,
                    OriginRunKind::Virtual,
                );
            }
        }
        if u64::try_from(self.logical_bytes.len()).ok() != Some(segment.logical_byte_range.end)
            || self.logical_utf16 != segment.logical_utf16_range.end
        {
            return Err(InlineLeafMaterializationError::Invariant(
                "materialized projection disagrees with declared logical metric",
            ));
        }
        Ok(())
    }

    fn finish_projection(&mut self) -> Result<(), InlineLeafMaterializationError> {
        let logical_bytes = std::mem::take(&mut self.logical_bytes);
        let logical = String::from_utf8(logical_bytes).map_err(|_| {
            InlineLeafMaterializationError::Invariant("completed logical leaf is not UTF-8")
        })?;
        if logical.encode_utf16().count()
            != usize::try_from(self.logical_utf16).map_err(|_| {
                InlineLeafMaterializationError::Invariant("completed UTF-16 length exceeds usize")
            })?
        {
            return Err(InlineLeafMaterializationError::Invariant(
                "completed logical UTF-16 metric differs from materialized text",
            ));
        }
        let origin_map = LogicalOriginMap {
            leaf_id: self.binding.block().0,
            revision: self.binding.source.revision.0,
            logical_len: u32::try_from(logical.len()).map_err(|_| {
                InlineLeafMaterializationError::Invariant("bounded inline length exceeds u32")
            })?,
            runs: std::mem::take(&mut self.origin_runs),
        };
        self.receipt.origin_runs = origin_map.runs.len();
        self.logical = Some(logical);
        self.origin_map = Some(origin_map);
        Ok(())
    }

    fn poll_inline_service(
        &mut self,
        document: &SerializedGreenDocument,
        source: SourceQueryView<'_>,
        references: &dyn InlineReferenceSnapshot,
    ) -> Result<(), InlineLeafMaterializationError> {
        // The worker actor checks the job binding at its normal poll boundary
        // and again immediately around the sole atomic parser call.
        if !self.validate_supersession(document, source)? {
            return Ok(());
        }
        let input_kind = self
            .input_kind
            .ok_or(InlineLeafMaterializationError::Invariant(
                "inline service phase lost typed input kind",
            ))?;
        self.receipt.inline_service_calls = 1;
        let result = {
            let logical =
                self.logical
                    .as_deref()
                    .ok_or(InlineLeafMaterializationError::Invariant(
                        "inline service phase lost logical input",
                    ))?;
            parse_inline_fragment(InlineFragmentRequest {
                logical,
                leaf_id: self.binding.block().0,
                kind: input_kind,
                profile: InlineProfile::CommonMark,
                reference_snapshot: references,
                revision: self.binding.source.revision.0,
                expected_revision: self.binding.source.revision.0,
            })
        };
        if !self.validate_supersession(document, source)? {
            return Ok(());
        }
        match result {
            Ok(fragment) => {
                self.mapped_semantic_facts = Vec::with_capacity(fragment.facts.len());
                self.mapped_projection_facts = Vec::with_capacity(fragment.projection_facts.len());
                self.fragment = Some(fragment);
                self.phase = InlineLeafJobPhase::ReferenceValidation;
            }
            Err(error) => {
                self.finish_unknown(InlineLeafUnknownReason::InlineServiceRejected(error));
            }
        }
        Ok(())
    }

    fn poll_reference_dependency(
        &mut self,
        references: &dyn InlineReferenceSnapshot,
    ) -> Result<bool, InlineLeafMaterializationError> {
        let dependencies = &self
            .fragment
            .as_ref()
            .ok_or(InlineLeafMaterializationError::Invariant(
                "reference phase lost its fragment",
            ))?
            .reference_dependencies;
        let dependency_count = dependencies.len();
        let Some(expected) = dependencies.get(self.reference_dependency_index).cloned() else {
            return Ok(true);
        };
        self.reference_dependency_index = self.reference_dependency_index.checked_add(1).ok_or(
            InlineLeafMaterializationError::Invariant("reference dependency index overflow"),
        )?;
        self.receipt.reference_dependencies_revalidated = self
            .receipt
            .reference_dependencies_revalidated
            .checked_add(1)
            .ok_or(InlineLeafMaterializationError::Invariant(
                "reference dependency receipt overflow",
            ))?;
        let actual = references.resolve(&expected.normalized_label, &expected.normalized_label);
        if actual.symbol_id != expected.symbol_id
            || actual.presence_generation != expected.presence_generation
            || actual.defined != expected.resolved
        {
            self.finish_unknown(InlineLeafUnknownReason::StaleReferenceDependency {
                normalized_label: expected.normalized_label,
                expected_symbol_id: expected.symbol_id,
                actual_symbol_id: actual.symbol_id,
                expected_presence_generation: expected.presence_generation,
                actual_presence_generation: actual.presence_generation,
                expected_resolved: expected.resolved,
                actual_resolved: actual.defined,
            })
        }
        Ok(self.phase != InlineLeafJobPhase::Complete
            && self.reference_dependency_index == dependency_count)
    }

    fn prepare_origin_composition(&mut self) {
        let result = self
            .origin_map
            .as_ref()
            .expect("reference phase owns its origin map")
            .validate()
            .and_then(|_| {
                let map = self
                    .origin_map
                    .as_ref()
                    .expect("reference phase owns its origin map");
                let fragment = self
                    .fragment
                    .as_ref()
                    .expect("reference phase owns its fragment");
                if map.leaf_id != fragment.leaf_id {
                    Err(OriginMapError::LeafMismatch)
                } else if map.revision != fragment.revision {
                    Err(OriginMapError::RevisionMismatch)
                } else {
                    Ok(())
                }
            });
        match result {
            Ok(()) => self.phase = InlineLeafJobPhase::OriginComposition,
            Err(error) => {
                self.finish_unknown(InlineLeafUnknownReason::OriginCompositionRejected(error))
            }
        }
    }

    fn poll_origin_fact(&mut self) -> Result<bool, OriginMapError> {
        let fragment = self
            .fragment
            .as_ref()
            .expect("origin phase owns its fragment");
        let next = if let Some(fact) = fragment.facts.get(self.semantic_fact_index).copied() {
            self.semantic_fact_index += 1;
            Some((true, fact))
        } else if let Some(fact) = fragment
            .projection_facts
            .get(self.projection_fact_index)
            .copied()
        {
            self.projection_fact_index += 1;
            Some((false, fact))
        } else {
            None
        };
        let Some((semantic, fact)) = next else {
            return Ok(true);
        };
        let mapped = self
            .origin_map
            .as_ref()
            .expect("origin phase owns its map")
            .map_fact(fact)?;
        if semantic {
            self.mapped_semantic_facts.push(mapped);
        } else {
            self.mapped_projection_facts.push(mapped);
        }
        self.receipt.origin_facts_mapped = self
            .receipt
            .origin_facts_mapped
            .checked_add(1)
            .expect("bounded inline fact count cannot overflow usize");
        Ok(false)
    }

    fn finish_ready(
        &mut self,
        composed: ComposedInlineFragment,
    ) -> Result<(), InlineLeafMaterializationError> {
        let input_kind = self
            .input_kind
            .ok_or(InlineLeafMaterializationError::Invariant(
                "ready inline job lost typed input kind",
            ))?;
        let logical = self
            .logical
            .take()
            .ok_or(InlineLeafMaterializationError::Invariant(
                "ready inline job lost logical input",
            ))?;
        let origin_map =
            self.origin_map
                .take()
                .ok_or(InlineLeafMaterializationError::Invariant(
                    "ready inline job lost origin map",
                ))?;
        let fragment = self
            .fragment
            .take()
            .ok_or(InlineLeafMaterializationError::Invariant(
                "ready inline job lost fragment",
            ))?;
        self.complete = Some(InlineLeafOutcome::Ready(ReadyInlineLeaf {
            binding: self.binding,
            input_kind,
            logical,
            origin_map,
            fragment,
            composed,
            receipt: self.receipt,
        }));
        self.phase = InlineLeafJobPhase::Complete;
        Ok(())
    }
}

/// Derives exact inline presentation for one packed-green terminal.
///
/// This synchronous drain exists only as a prototype differential oracle and
/// test helper. The production API must keep it hidden or feature-gated so UI
/// code cannot bypass `InlineLeafExecutionLane::ParserIsolateOrWebWorker`.
///
/// `target` must be a capability minted by a green seek/traversal. The green
/// document revalidates it before any source observation is trusted. Source is
/// borrowed from the live actor, so this function cannot retain a historical
/// Crop root; its returned result owns only bounded logical/protocol data.
#[cfg(any(test, feature = "inline-liveness-probe"))]
#[doc(hidden)]
pub fn derive_inline_leaf_presentation(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    source: SourceQueryView<'_>,
    target: GreenEnterCapability,
    references: &dyn InlineReferenceSnapshot,
) -> Result<InlineLeafOutcome, InlineLeafMaterializationError> {
    let mut job =
        InlineLeafMaterializationJob::new_on_parser_worker(document, arena, source, target)?;
    let unbounded =
        InlineLeafMaterializationFuel::new(usize::MAX).expect("usize::MAX is a nonzero fuel value");
    loop {
        match job.poll(document, arena, source, references, unbounded)? {
            InlineLeafMaterializationProgress::Pending { .. } => {}
            InlineLeafMaterializationProgress::Complete(outcome) => return Ok(outcome),
        }
    }
}

fn unknown(
    binding: InlineLeafBinding,
    reason: InlineLeafUnknownReason,
    receipt: InlineLeafMaterializationReceipt,
) -> InlineLeafOutcome {
    InlineLeafOutcome::Unknown(UnknownInlineLeaf {
        binding,
        reason,
        receipt,
    })
}

fn source_observation(
    source: SourceQueryView<'_>,
) -> Result<InlineSourceObservation, InlineLeafMaterializationError> {
    Ok(InlineSourceObservation {
        revision: source.revision(),
        root: source.identity(),
        bytes: u64::try_from(source.len_bytes())
            .map_err(|_| InlineLeafMaterializationError::Invariant("source bytes exceed u64"))?,
        utf16: u64::try_from(source.len_utf16())
            .map_err(|_| InlineLeafMaterializationError::Invariant("source UTF-16 exceeds u64"))?,
    })
}

enum InputKindError {
    Green(SerializedGreenError),
    Unsupported(GreenKind),
}

fn input_kind(frame: &crate::GreenOpenFrame) -> Result<InlineInputKind, InputKindError> {
    match frame.kind {
        GreenKind::HEADING => {
            let heading = GreenHeadingOpenFacts::try_from_envelope(&frame.facts)
                .map_err(InputKindError::Green)?;
            Ok(InlineInputKind::Heading {
                level: heading.level(),
                setext: heading.style() == GreenHeadingStyle::Setext,
            })
        }
        GreenKind::PARAGRAPH => Ok(InlineInputKind::Paragraph),
        GreenKind::TABLE_CELL => Ok(InlineInputKind::TableCell),
        other => Err(InputKindError::Unsupported(other)),
    }
}

fn validate_segment_prefix(
    segment: &crate::GreenLogicalSegment,
    target: GreenEnterCapability,
    logical_bytes: usize,
    logical_utf16: u64,
) -> Result<(), InlineLeafMaterializationError> {
    if segment.channel != LogicalChannel::Inline || segment.consumer.enter != target {
        return Err(InlineLeafMaterializationError::Invariant(
            "logical segment belongs to a different terminal",
        ));
    }
    if u64::try_from(logical_bytes).ok() != Some(segment.logical_byte_range.start)
        || logical_utf16 != segment.logical_utf16_range.start
    {
        return Err(InlineLeafMaterializationError::Invariant(
            "logical segment does not continue the materialized prefix",
        ));
    }
    Ok(())
}

fn usize_range(range: &Range<u64>) -> Result<Range<usize>, InlineLeafMaterializationError> {
    Ok(usize::try_from(range.start)
        .map_err(|_| InlineLeafMaterializationError::Invariant("physical start exceeds usize"))?
        ..usize::try_from(range.end)
            .map_err(|_| InlineLeafMaterializationError::Invariant("physical end exceeds usize"))?)
}

fn logical_range_u32(range: &Range<u64>) -> Result<Range<u32>, InlineLeafMaterializationError> {
    Ok(u32::try_from(range.start)
        .map_err(|_| InlineLeafMaterializationError::Invariant("logical start exceeds u32"))?
        ..u32::try_from(range.end)
            .map_err(|_| InlineLeafMaterializationError::Invariant("logical end exceeds u32"))?)
}

fn append_atomic_replacement(
    transform: AtomicProjectionKind,
    physical: &[u8],
    output: &mut Vec<u8>,
) -> Result<u64, InlineLeafMaterializationError> {
    match transform {
        AtomicProjectionKind::TabToSpaces { spaces } if physical == b"\t" => {
            let end = output.len().checked_add(usize::from(spaces)).ok_or(
                InlineLeafMaterializationError::Invariant("atomic tab expansion length overflow"),
            )?;
            output.resize(end, b' ');
            Ok(u64::from(spaces))
        }
        AtomicProjectionKind::CrLfToLf if physical == b"\r\n" => {
            output.push(b'\n');
            Ok(1)
        }
        AtomicProjectionKind::LoneCrToLf if physical == b"\r" => {
            output.push(b'\n');
            Ok(1)
        }
        AtomicProjectionKind::NulToReplacement if physical == b"\0" => {
            output.extend_from_slice("\u{fffd}".as_bytes());
            Ok(1)
        }
        AtomicProjectionKind::TabToSpaces { .. }
        | AtomicProjectionKind::CrLfToLf
        | AtomicProjectionKind::LoneCrToLf
        | AtomicProjectionKind::NulToReplacement => Err(InlineLeafMaterializationError::Invariant(
            "typed atomic projection disagrees with current source bytes",
        )),
    }
}

fn push_origin_run(
    runs: &mut Vec<LogicalOriginRun>,
    logical: Range<u32>,
    physical: Range<u64>,
    kind: OriginRunKind,
) {
    if kind == OriginRunKind::Identity
        && let Some(previous) = runs.last_mut()
        && previous.kind == OriginRunKind::Identity
        && previous.logical.end == logical.start
        && previous.physical.end == physical.start
    {
        previous.logical.end = logical.end;
        previous.physical.end = physical.end;
        return;
    }
    runs.push(LogicalOriginRun {
        logical,
        physical,
        kind,
    });
}

fn merge_source_receipt(
    target: &mut InlineLeafMaterializationReceipt,
    source: DerivedSourceReadReceipt,
) -> Result<(), InlineLeafMaterializationError> {
    target.source_ranges_read = target.source_ranges_read.checked_add(1).ok_or(
        InlineLeafMaterializationError::Invariant("source range count overflow"),
    )?;
    target.source_chunks_visited = target
        .source_chunks_visited
        .checked_add(source.chunks_visited)
        .ok_or(InlineLeafMaterializationError::Invariant(
            "source chunk count overflow",
        ))?;
    target.source_bytes_copied = target
        .source_bytes_copied
        .checked_add(source.bytes_copied)
        .ok_or(InlineLeafMaterializationError::Invariant(
            "source byte count overflow",
        ))?;
    target.maximum_source_chunk_bytes = target
        .maximum_source_chunk_bytes
        .max(source.maximum_chunk_bytes);
    Ok(())
}

fn apply_green_receipt(
    target: &mut InlineLeafMaterializationReceipt,
    green: crate::GreenLogicalReceipt,
) {
    target.green_coverage_runs_visited = green.coverage_runs_visited;
    target.projection_program_pages_decoded = green.projection_program_pages_decoded;
}

fn map_source_read_error(error: DerivedSourceReadError) -> InlineLeafMaterializationError {
    match error {
        DerivedSourceReadError::Source(error) => InlineLeafMaterializationError::Source(error),
        DerivedSourceReadError::CapExceeded => InlineLeafMaterializationError::Invariant(
            "bounded source read escaped the preflighted inline cap",
        ),
        DerivedSourceReadError::Overflow => {
            InlineLeafMaterializationError::Invariant("bounded source read overflow")
        }
    }
}

//! End-to-end exact-source inline projection derivation.
//!
//! This private job is the only promotion boundary between fallible inline
//! candidates and one atomic, source-stamped typed capture. Raw
//! backtick runs and syntactic angle autolinks first resolve into one
//! source-ordered opaque stream. The whole-leaf lexical hazard gate and
//! emphasis resolver both consume that same shielding map; any unshielded
//! hazard or ambiguous delimiter remainder fails the supplied range closed.
//! Candidate scratch is reclaimed before completion. Successful transfer moves
//! all facts, cooked link values, and edit components together; discarded or
//! faulted work follows the explicit fuelled release path.

use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;

use flark_engine::parser_internal::{
    M11ParserRangeCursor, M11ParserRangeError, M11ParserRangeStatus, M11ParserSourceRangeAuthority,
    M11ReferenceResolver, M11_PARSER_RANGE_MAX_POLL_BYTES,
};
use flark_engine::{DocumentRuntime, ParserProfileId, SourceVersion};

use crate::block_core::M11RecursiveGreenInlineLeafFence;
use crate::inline_autolink::{
    M11InlineAutolinkError, M11InlineAutolinkJob, M11InlineAutolinkPollStatus,
    M11InlineOpaqueCandidate, M11InlineOpaqueCandidates, M11InlineOpaqueKind,
    M11InlineOpaquePollStatus, M11InlineOpaqueResolveJob,
};
use crate::inline_bare_autolink::{
    M11InlineBareAutolinkJob, M11InlineBareAutolinkJobError, M11InlineBareAutolinkPollStatus,
};
use crate::inline_code::{
    M11InlineCodeError, M11InlineCodeJob, M11InlineCodePollStatus, M11InlineCodeRuns,
};
use crate::inline_direct::{
    M11InlineDirectCandidates, M11InlineDirectError, M11InlineDirectFact, M11InlineDirectJob,
    M11InlineDirectKind, M11InlineDirectPollStatus,
};
use crate::inline_edit_component::{
    derive_inline_edit_components, M11InlineEditComponent,
    M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES,
};
use crate::inline_emphasis::{
    M11EmphasisCandidate, M11EmphasisCandidateKind, M11InlineCandidates, M11InlineEmphasisError,
    M11InlineEmphasisJob, M11InlineEmphasisPollStatus,
};
use crate::inline_hazard::{
    M11InlineHazardDisposition, M11InlineHazardError, M11InlineHazardJob, M11InlineHazardPollStatus,
};
use crate::inline_lex::{
    M11InlineLexError, M11InlineLexEvent, M11InlineLexEventKind, M11InlineLexHazardKind,
    M11InlineLexPollStatus, M11InlineLexScanner,
};
use crate::inline_projection::{
    M11InlineLinkValue, M11InlineProjectionCaptureValidator, M11InlineProjectionError,
    M11InlineProjectionFact, M11InlineProjectionKind,
};
use crate::parser_binding::{M11ParserBinding, M11_GRAMMAR_REVISION};

pub const M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineProjectionUnsupportedReason {
    LexicalHazard(M11InlineLexHazardKind),
    AmbiguousEmphasisRemainder { marker: u8 },
}

/// Fail-closed result for the entire exact source range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineProjectionUnsupported {
    source_range: Range<u32>,
    first_blocker_range: Range<u32>,
    reason: M11InlineProjectionUnsupportedReason,
}

impl M11InlineProjectionUnsupported {
    #[cfg(test)]
    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    #[cfg(test)]
    pub(crate) fn first_blocker_range(&self) -> Range<u32> {
        self.first_blocker_range.clone()
    }

    #[cfg(test)]
    pub(crate) const fn reason(&self) -> M11InlineProjectionUnsupportedReason {
        self.reason
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineProjectionDisposition {
    Authoritative,
    Unsupported(M11InlineProjectionUnsupported),
}

/// One complete authoritative inline result.
///
/// These three vectors are transferred together so a caller can never observe
/// facts without the cooked values or edit components derived from the same
/// exact source range.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct M11InlineProjectionCapture {
    facts: Vec<M11InlineProjectionFact>,
    link_values: Vec<M11InlineLinkValue>,
    edit_components: Vec<M11InlineEditComponent>,
}

impl M11InlineProjectionCapture {
    #[must_use]
    pub fn facts(&self) -> &[M11InlineProjectionFact] {
        &self.facts
    }

    #[must_use]
    pub fn link_values(&self) -> &[M11InlineLinkValue] {
        &self.link_values
    }

    #[must_use]
    pub fn edit_components(&self) -> &[M11InlineEditComponent] {
        &self.edit_components
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<M11InlineProjectionFact>,
        Vec<M11InlineLinkValue>,
        Vec<M11InlineEditComponent>,
    ) {
        (self.facts, self.link_values, self.edit_components)
    }
}

/// Source-stamped terminal result from one exact inline leaf.
#[derive(Debug, Eq, PartialEq)]
pub enum M11InlineProjectionOutcome {
    Authoritative {
        source: SourceVersion,
        source_range: Range<u32>,
        parser_profile: ParserProfileId,
        capture: M11InlineProjectionCapture,
    },
    Unsupported {
        source: SourceVersion,
        source_range: Range<u32>,
        parser_profile: ParserProfileId,
    },
}

impl M11InlineProjectionOutcome {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        match self {
            Self::Authoritative { source, .. } | Self::Unsupported { source, .. } => *source,
        }
    }

    #[must_use]
    pub fn source_range(&self) -> Range<u32> {
        match self {
            Self::Authoritative { source_range, .. } | Self::Unsupported { source_range, .. } => {
                source_range.clone()
            }
        }
    }

    #[must_use]
    pub const fn parser_profile(&self) -> ParserProfileId {
        match self {
            Self::Authoritative { parser_profile, .. }
            | Self::Unsupported { parser_profile, .. } => *parser_profile,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineProjectionJobPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InlineProjectionJobPoll {
    status: M11InlineProjectionJobPollStatus,
    transitions: usize,
}

impl M11InlineProjectionJobPoll {
    pub const fn status(self) -> M11InlineProjectionJobPollStatus {
        self.status
    }

    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InlineProjectionJobReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11InlineProjectionJobReleasePoll {
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    pub const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Debug)]
enum M11InlineProjectionJobErrorInner {
    Code(M11InlineCodeError),
    Autolink(M11InlineAutolinkError),
    Direct(M11InlineDirectError),
    BareAutolink(M11InlineBareAutolinkJobError),
    Hazard(M11InlineHazardError),
    Emphasis(M11InlineEmphasisError),
    Lex(M11InlineLexError),
    Projection(M11InlineProjectionError),
    Page(M11ParserRangeError),
    ZeroFuel,
    PollLimitExceeded,
    CoordinateOverflow,
    CandidateOrder,
    BlockFenceRangeMismatch,
    UnsupportedGrammarRevision { actual: u32 },
    InvalidState,
}

/// Opaque failure from resumable inline Projection derivation or cleanup.
///
/// Stage-private candidate errors remain implementation details while their
/// full diagnostic chain is preserved by [`std::error::Error`].
#[derive(Debug)]
pub struct M11InlineProjectionJobError(M11InlineProjectionJobErrorInner);

#[allow(non_upper_case_globals)]
impl M11InlineProjectionJobError {
    const ZeroFuel: Self = Self(M11InlineProjectionJobErrorInner::ZeroFuel);
    const PollLimitExceeded: Self = Self(M11InlineProjectionJobErrorInner::PollLimitExceeded);
    const CoordinateOverflow: Self = Self(M11InlineProjectionJobErrorInner::CoordinateOverflow);
    const CandidateOrder: Self = Self(M11InlineProjectionJobErrorInner::CandidateOrder);
    const BlockFenceRangeMismatch: Self =
        Self(M11InlineProjectionJobErrorInner::BlockFenceRangeMismatch);
    const InvalidState: Self = Self(M11InlineProjectionJobErrorInner::InvalidState);

    fn unsupported_grammar_revision(actual: u32) -> Self {
        Self(M11InlineProjectionJobErrorInner::UnsupportedGrammarRevision { actual })
    }
}

impl fmt::Display for M11InlineProjectionJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            M11InlineProjectionJobErrorInner::Code(error) => {
                write!(formatter, "inline Projection code stage failed: {error}")
            }
            M11InlineProjectionJobErrorInner::Autolink(error) => {
                write!(
                    formatter,
                    "inline Projection angle-autolink stage failed: {error}"
                )
            }
            M11InlineProjectionJobErrorInner::Direct(error) => {
                write!(
                    formatter,
                    "inline Projection direct-link stage failed: {error}"
                )
            }
            M11InlineProjectionJobErrorInner::BareAutolink(error) => {
                write!(
                    formatter,
                    "inline Projection bare-autolink stage failed: {error}"
                )
            }
            M11InlineProjectionJobErrorInner::Hazard(error) => {
                write!(formatter, "inline Projection hazard stage failed: {error}")
            }
            M11InlineProjectionJobErrorInner::Emphasis(error) => {
                write!(
                    formatter,
                    "inline Projection emphasis stage failed: {error}"
                )
            }
            M11InlineProjectionJobErrorInner::Lex(error) => {
                write!(formatter, "inline Projection emission scan failed: {error}")
            }
            M11InlineProjectionJobErrorInner::Projection(error) => {
                write!(
                    formatter,
                    "inline Projection capture validation failed: {error}"
                )
            }
            M11InlineProjectionJobErrorInner::Page(error) => {
                write!(
                    formatter,
                    "inline edit-component source capture failed: {error}"
                )
            }
            M11InlineProjectionJobErrorInner::ZeroFuel => {
                formatter.write_str("inline Projection poll requires nonzero fuel")
            }
            M11InlineProjectionJobErrorInner::PollLimitExceeded => {
                formatter.write_str("inline Projection poll exceeds its transition limit")
            }
            M11InlineProjectionJobErrorInner::CoordinateOverflow => {
                formatter.write_str("inline Projection coordinate or counter overflow")
            }
            M11InlineProjectionJobErrorInner::CandidateOrder => {
                formatter.write_str("inline Projection candidates are not in source preorder")
            }
            M11InlineProjectionJobErrorInner::BlockFenceRangeMismatch => {
                formatter.write_str("inline Projection range differs from the fenced Paragraph")
            }
            M11InlineProjectionJobErrorInner::UnsupportedGrammarRevision { actual } => {
                write!(
                    formatter,
                    "unsupported inline Projection grammar revision {actual}"
                )
            }
            M11InlineProjectionJobErrorInner::InvalidState => {
                formatter.write_str("inline Projection job is in an invalid state")
            }
        }
    }
}

impl std::error::Error for M11InlineProjectionJobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            M11InlineProjectionJobErrorInner::Code(error) => Some(error),
            M11InlineProjectionJobErrorInner::Autolink(error) => Some(error),
            M11InlineProjectionJobErrorInner::Direct(error) => Some(error),
            M11InlineProjectionJobErrorInner::BareAutolink(error) => Some(error),
            M11InlineProjectionJobErrorInner::Hazard(error) => Some(error),
            M11InlineProjectionJobErrorInner::Emphasis(error) => Some(error),
            M11InlineProjectionJobErrorInner::Lex(error) => Some(error),
            M11InlineProjectionJobErrorInner::Projection(error) => Some(error),
            M11InlineProjectionJobErrorInner::Page(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11InlineCodeError> for M11InlineProjectionJobError {
    fn from(value: M11InlineCodeError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Code(value))
    }
}

impl From<M11InlineAutolinkError> for M11InlineProjectionJobError {
    fn from(value: M11InlineAutolinkError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Autolink(value))
    }
}

impl From<M11InlineDirectError> for M11InlineProjectionJobError {
    fn from(value: M11InlineDirectError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Direct(value))
    }
}

impl From<M11InlineBareAutolinkJobError> for M11InlineProjectionJobError {
    fn from(value: M11InlineBareAutolinkJobError) -> Self {
        Self(M11InlineProjectionJobErrorInner::BareAutolink(value))
    }
}

impl From<M11InlineHazardError> for M11InlineProjectionJobError {
    fn from(value: M11InlineHazardError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Hazard(value))
    }
}

impl From<M11InlineEmphasisError> for M11InlineProjectionJobError {
    fn from(value: M11InlineEmphasisError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Emphasis(value))
    }
}

impl From<M11InlineLexError> for M11InlineProjectionJobError {
    fn from(value: M11InlineLexError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Lex(value))
    }
}

impl From<M11InlineProjectionError> for M11InlineProjectionJobError {
    fn from(value: M11InlineProjectionError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Projection(value))
    }
}

impl From<M11ParserRangeError> for M11InlineProjectionJobError {
    fn from(value: M11ParserRangeError) -> Self {
        Self(M11InlineProjectionJobErrorInner::Page(value))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionJobPhase {
    Code,
    TakeCode,
    Autolink,
    TakeAutolink,
    ResolveOpaque,
    TakeOpaque,
    Direct,
    TakeDirect,
    BareAutolink,
    TakeBareAutolink,
    Hazard,
    TakeHazard,
    Emphasis,
    TakeEmphasis,
    CheckRemainder,
    BeginCapture,
    Emit,
    BeginEditComponents,
    CaptureEditComponentSource,
    BuildEditComponents,
    BeginCleanup,
    CleanupCode,
    CleanupOpaque,
    CleanupCandidates,
    Complete,
    OutcomeTaken,
    Faulted,
    Releasing,
    Released,
}

/// Resumable exact-source promotion from inline candidates to an atomic capture.
pub struct M11InlineProjectionJob {
    source: SourceVersion,
    source_range: Range<u32>,
    parser_profile: ParserProfileId,
    phase: ProjectionJobPhase,
    code_job: Option<Box<M11InlineCodeJob>>,
    code: Option<M11InlineCodeRuns>,
    autolink_job: Option<Box<M11InlineAutolinkJob>>,
    opaque_job: Option<Box<M11InlineOpaqueResolveJob>>,
    opaque: Option<Box<M11InlineOpaqueCandidates>>,
    direct_job: Option<Box<M11InlineDirectJob>>,
    reference_resolver: Option<M11ReferenceResolver>,
    #[cfg(any(test, feature = "m11-compact-probe"))]
    compact_reference_resolver: Option<crate::block_core::M11CompactReferenceResolver>,
    direct: Option<M11InlineDirectCandidates>,
    bare_autolink_job: Option<M11InlineBareAutolinkJob>,
    hazard_job: Option<Box<M11InlineHazardJob>>,
    emphasis_job: Option<Box<M11InlineEmphasisJob>>,
    candidates: Option<Box<M11InlineCandidates>>,
    leaf_scanner: Option<Box<M11InlineLexScanner>>,
    capture_validator: Option<M11InlineProjectionCaptureValidator>,
    unsupported: Option<M11InlineProjectionUnsupported>,
    final_authority: Option<M11ParserSourceRangeAuthority>,
    code_job_abort_started: bool,
    code_release_started: bool,
    autolink_job_abort_started: bool,
    opaque_job_abort_started: bool,
    opaque_release_started: bool,
    emphasis_abort_started: bool,
    candidate_release_started: bool,
    opaque_index: u32,
    direct_index: u32,
    delimiter_index: u32,
    emphasis_chain: Option<u32>,
    pending_opaque: Option<M11InlineOpaqueCandidate>,
    pending_direct: Option<u32>,
    pending_emphasis: Option<M11EmphasisCandidate>,
    pending_leaf_event: Option<M11InlineLexEvent>,
    pending_leaf: Option<M11InlineLexEvent>,
    leaf_scan_complete: bool,
    leaf_opaque_index: u32,
    leaf_direct_syntax_index: u32,
    emphasis_visited: u32,
    capture: Option<M11InlineProjectionCapture>,
    edit_component_cursor: Option<M11ParserRangeCursor>,
    edit_component_source: Vec<u8>,
    edit_component_source_written: usize,
    last_order_key: Option<(u32, u32)>,
    initial_lexical_source_bytes_read: u64,
}

impl fmt::Debug for M11InlineProjectionJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineProjectionJob")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("parser_profile", &self.parser_profile)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl M11InlineProjectionJob {
    /// Starts one recursive-Green inline-bearing leaf and retains the emitted
    /// typed facts for a bounded viewport consumer.
    pub fn new_for_recursive_green_inline_leaf(
        runtime: &DocumentRuntime,
        fence: M11RecursiveGreenInlineLeafFence,
        binding: M11ParserBinding,
    ) -> Result<Self, M11InlineProjectionJobError> {
        Self::new_for_recursive_green_inline_leaf_with_optional_reference_resolver(
            runtime, fence, binding, None,
        )
    }

    /// Starts one recursive-Green inline leaf with both definitive reference
    /// winners and bounded typed-fact capture for a viewport consumer.
    pub fn new_for_recursive_green_inline_leaf_with_reference_resolver(
        runtime: &DocumentRuntime,
        fence: M11RecursiveGreenInlineLeafFence,
        binding: M11ParserBinding,
        reference_resolver: M11ReferenceResolver,
    ) -> Result<Self, M11InlineProjectionJobError> {
        Self::new_for_recursive_green_inline_leaf_with_optional_reference_resolver(
            runtime,
            fence,
            binding,
            Some(reference_resolver),
        )
    }

    #[cfg(any(test, feature = "m11-compact-probe"))]
    pub(crate) fn new_for_recursive_green_inline_leaf_with_compact_reference_resolver(
        runtime: &DocumentRuntime,
        fence: M11RecursiveGreenInlineLeafFence,
        binding: M11ParserBinding,
        reference_resolver: crate::block_core::M11CompactReferenceResolver,
    ) -> Result<Self, M11InlineProjectionJobError> {
        let mut job = Self::new_for_recursive_green_inline_leaf_with_optional_reference_resolver(
            runtime, fence, binding, None,
        )?;
        job.compact_reference_resolver = Some(reference_resolver);
        Ok(job)
    }

    fn new_for_recursive_green_inline_leaf_with_optional_reference_resolver(
        runtime: &DocumentRuntime,
        fence: M11RecursiveGreenInlineLeafFence,
        binding: M11ParserBinding,
        reference_resolver: Option<M11ReferenceResolver>,
    ) -> Result<Self, M11InlineProjectionJobError> {
        let (authority, expected_range) = fence.into_inline_authority();
        let expected_range = u32::try_from(expected_range.start)
            .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?
            ..u32::try_from(expected_range.end)
                .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?;
        let actual_range = authority.source_range();
        let actual_range = u32::try_from(actual_range.start)
            .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?
            ..u32::try_from(actual_range.end)
                .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?;
        if actual_range != expected_range {
            return Err(M11InlineProjectionJobError::BlockFenceRangeMismatch);
        }
        Self::new_from_exact_authority(runtime, authority, binding, reference_resolver)
    }

    fn new_from_exact_authority(
        runtime: &DocumentRuntime,
        authority: M11ParserSourceRangeAuthority,
        binding: M11ParserBinding,
        reference_resolver: Option<M11ReferenceResolver>,
    ) -> Result<Self, M11InlineProjectionJobError> {
        authority
            .validate(runtime)
            .map_err(M11InlineCodeError::from)?;
        let source = authority.source();
        let authority_range = authority.source_range();
        let source_range = u32::try_from(authority_range.start)
            .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?
            ..u32::try_from(authority_range.end)
                .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?;
        if binding.grammar_revision() != M11_GRAMMAR_REVISION {
            return Err(M11InlineProjectionJobError::unsupported_grammar_revision(
                binding.grammar_revision(),
            ));
        }
        let parser_profile = binding.syntax_profile();
        let code_job = M11InlineCodeJob::new(runtime, authority)?;
        Ok(Self {
            source,
            source_range,
            parser_profile,
            phase: ProjectionJobPhase::Code,
            code_job: Some(Box::new(code_job)),
            code: None,
            autolink_job: None,
            opaque_job: None,
            opaque: None,
            direct_job: None,
            reference_resolver,
            #[cfg(any(test, feature = "m11-compact-probe"))]
            compact_reference_resolver: None,
            direct: None,
            bare_autolink_job: None,
            hazard_job: None,
            emphasis_job: None,
            candidates: None,
            leaf_scanner: None,
            capture_validator: None,
            unsupported: None,
            final_authority: None,
            code_job_abort_started: false,
            code_release_started: false,
            autolink_job_abort_started: false,
            opaque_job_abort_started: false,
            opaque_release_started: false,
            emphasis_abort_started: false,
            candidate_release_started: false,
            opaque_index: 0,
            direct_index: 0,
            delimiter_index: 0,
            emphasis_chain: None,
            pending_opaque: None,
            pending_direct: None,
            pending_emphasis: None,
            pending_leaf_event: None,
            pending_leaf: None,
            leaf_scan_complete: false,
            leaf_opaque_index: 0,
            leaf_direct_syntax_index: 0,
            emphasis_visited: 0,
            capture: Some(M11InlineProjectionCapture::default()),
            edit_component_cursor: None,
            edit_component_source: Vec::new(),
            edit_component_source_written: 0,
            last_order_key: None,
            initial_lexical_source_bytes_read: 0,
        })
    }

    /// Exact source bytes read by the initial lexical pass.
    ///
    /// Later source-backed stages receive cursors from the same range
    /// authority. This receipt makes the first complete range traversal
    /// directly observable without claiming to total repeated stage reads.
    #[must_use]
    pub const fn initial_lexical_source_bytes_read(&self) -> u64 {
        self.initial_lexical_source_bytes_read
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineProjectionJobPoll, M11InlineProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase == ProjectionJobPhase::Complete {
            return Ok(M11InlineProjectionJobPoll {
                status: M11InlineProjectionJobPollStatus::Complete,
                transitions: 0,
            });
        }
        if matches!(
            self.phase,
            ProjectionJobPhase::Faulted
                | ProjectionJobPhase::OutcomeTaken
                | ProjectionJobPhase::Releasing
                | ProjectionJobPhase::Released
        ) {
            return Err(M11InlineProjectionJobError::InvalidState);
        }

        let mut transitions = 0;
        while transitions < fuel {
            let before = transitions;
            let phase_before = self.phase;
            let step = match self.phase {
                ProjectionJobPhase::Code => self.poll_code(runtime, fuel, &mut transitions),
                ProjectionJobPhase::TakeCode => self.take_code(runtime, &mut transitions),
                ProjectionJobPhase::Autolink => self.poll_autolink(runtime, fuel, &mut transitions),
                ProjectionJobPhase::TakeAutolink => self.take_autolink(runtime, &mut transitions),
                ProjectionJobPhase::ResolveOpaque => {
                    self.poll_opaque_resolver(runtime, fuel, &mut transitions)
                }
                ProjectionJobPhase::TakeOpaque => self.take_opaque(runtime, &mut transitions),
                ProjectionJobPhase::Direct => self.poll_direct(runtime, fuel, &mut transitions),
                ProjectionJobPhase::TakeDirect => self.take_direct(runtime, &mut transitions),
                ProjectionJobPhase::BareAutolink => {
                    self.poll_bare_autolink(runtime, fuel, &mut transitions)
                }
                ProjectionJobPhase::TakeBareAutolink => {
                    self.take_bare_autolink(runtime, &mut transitions)
                }
                ProjectionJobPhase::Hazard => self.poll_hazard(runtime, fuel, &mut transitions),
                ProjectionJobPhase::TakeHazard => self.take_hazard(&mut transitions),
                ProjectionJobPhase::Emphasis => self.poll_emphasis(runtime, fuel, &mut transitions),
                ProjectionJobPhase::TakeEmphasis => self.take_emphasis(&mut transitions),
                ProjectionJobPhase::CheckRemainder => self.check_remainder(&mut transitions),
                ProjectionJobPhase::BeginCapture => self.begin_capture(runtime, &mut transitions),
                ProjectionJobPhase::Emit => self.poll_emit(fuel, &mut transitions),
                ProjectionJobPhase::BeginEditComponents => {
                    self.begin_edit_components(runtime, &mut transitions)
                }
                ProjectionJobPhase::CaptureEditComponentSource => {
                    self.poll_edit_component_source(fuel, &mut transitions)
                }
                ProjectionJobPhase::BuildEditComponents => {
                    self.build_edit_components(&mut transitions)
                }
                ProjectionJobPhase::BeginCleanup => self.begin_cleanup(&mut transitions),
                ProjectionJobPhase::CleanupCode => {
                    self.poll_code_cleanup(runtime, fuel, &mut transitions)
                }
                ProjectionJobPhase::CleanupOpaque => {
                    self.poll_opaque_cleanup(runtime, fuel, &mut transitions)
                }
                ProjectionJobPhase::CleanupCandidates => {
                    self.poll_candidate_cleanup(runtime, fuel, &mut transitions)
                }
                ProjectionJobPhase::Complete => break,
                ProjectionJobPhase::OutcomeTaken
                | ProjectionJobPhase::Faulted
                | ProjectionJobPhase::Releasing
                | ProjectionJobPhase::Released => Err(M11InlineProjectionJobError::InvalidState),
            };
            if let Err(error) = step {
                if let Some(hazard) = self.hazard_job.as_mut() {
                    hazard.cancel();
                }
                if let Some(scanner) = self.leaf_scanner.as_mut() {
                    scanner.cancel();
                }
                drop(self.leaf_scanner.take());
                self.phase = ProjectionJobPhase::Faulted;
                return Err(error);
            }
            if transitions == before && self.phase != phase_before {
                // Advancing the orchestration state is itself one bounded
                // transition even when a completed child has no arena work
                // left to report. Without this charge a ready job can return
                // Pending(0), causing edge-triggered platform executors to
                // conclude that no further poll is required.
                transitions = transitions
                    .checked_add(1)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
            }
            if self.phase == ProjectionJobPhase::Complete {
                return Ok(M11InlineProjectionJobPoll {
                    status: M11InlineProjectionJobPollStatus::Complete,
                    transitions,
                });
            }
            if transitions == before {
                break;
            }
        }
        Ok(M11InlineProjectionJobPoll {
            status: M11InlineProjectionJobPollStatus::Pending,
            transitions,
        })
    }

    fn poll_code(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .code_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineCodePollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeCode;
        }
        Ok(())
    }

    fn take_code(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        self.initial_lexical_source_bytes_read = self
            .code_job
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .lexical_receipt()
            .source_bytes();
        if self.initial_lexical_source_bytes_read
            != u64::from(
                self.source_range
                    .end
                    .checked_sub(self.source_range.start)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?,
            )
        {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let code = self
            .code_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .take_output()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        drop(self.code_job.take());
        self.code = Some(code);
        self.autolink_job = Some(Box::new(M11InlineAutolinkJob::new(
            runtime,
            self.code
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
        )?));
        self.phase = ProjectionJobPhase::Autolink;
        *transitions += 1;
        Ok(())
    }

    fn poll_autolink(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .autolink_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineAutolinkPollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeAutolink;
        }
        Ok(())
    }

    fn take_autolink(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let opaque_job = M11InlineOpaqueResolveJob::take_new(
            runtime,
            &mut self.code,
            self.autolink_job
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
        )?;
        drop(self.autolink_job.take());
        self.opaque_job = Some(Box::new(opaque_job));
        self.phase = ProjectionJobPhase::ResolveOpaque;
        *transitions += 1;
        Ok(())
    }

    fn poll_opaque_resolver(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .opaque_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineOpaquePollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeOpaque;
        }
        Ok(())
    }

    fn take_opaque(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        self.opaque = Some(Box::new(
            self.opaque_job
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .take_output()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
        ));
        drop(self.opaque_job.take());
        let opaque = self
            .opaque
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        #[cfg(any(test, feature = "m11-compact-probe"))]
        let direct = if let Some(resolver) = self.compact_reference_resolver.take() {
            M11InlineDirectJob::new_with_compact_reference_resolver(runtime, opaque, resolver)?
        } else if let Some(resolver) = self.reference_resolver.take() {
            M11InlineDirectJob::new_with_reference_resolver(runtime, opaque, resolver)?
        } else {
            M11InlineDirectJob::new(runtime, opaque)?
        };
        #[cfg(not(any(test, feature = "m11-compact-probe")))]
        let direct = if let Some(resolver) = self.reference_resolver.take() {
            M11InlineDirectJob::new_with_reference_resolver(runtime, opaque, resolver)?
        } else {
            M11InlineDirectJob::new(runtime, opaque)?
        };
        self.direct_job = Some(Box::new(direct));
        self.phase = ProjectionJobPhase::Direct;
        *transitions += 1;
        Ok(())
    }

    fn poll_direct(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .direct_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(
                runtime,
                self.opaque
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?,
                fuel - *transitions,
            )?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineDirectPollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeDirect;
        }
        Ok(())
    }

    fn take_direct(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        self.direct = Some(
            self.direct_job
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .take_output()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
        );
        drop(self.direct_job.take());
        self.bare_autolink_job = Some(M11InlineBareAutolinkJob::new(
            runtime,
            self.opaque
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
            self.direct
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
        )?);
        self.phase = ProjectionJobPhase::BareAutolink;
        *transitions += 1;
        Ok(())
    }

    fn poll_bare_autolink(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .bare_autolink_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(
                runtime,
                self.opaque
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?,
                self.direct
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?,
                fuel - *transitions,
            )?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineBareAutolinkPollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeBareAutolink;
        }
        Ok(())
    }

    fn take_bare_autolink(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let augmented = self
            .bare_autolink_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .take_output()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        drop(self.bare_autolink_job.take());
        self.opaque
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .install_augmented(augmented)?;
        self.hazard_job = Some(Box::new(M11InlineHazardJob::new_with_direct(
            runtime,
            self.opaque
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
            self.direct
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?,
        )?));
        self.phase = ProjectionJobPhase::Hazard;
        *transitions += 1;
        Ok(())
    }

    fn poll_hazard(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .hazard_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(
                runtime,
                self.opaque
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?,
                fuel - *transitions,
            )?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineHazardPollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeHazard;
        }
        Ok(())
    }

    fn take_hazard(&mut self, transitions: &mut usize) -> Result<(), M11InlineProjectionJobError> {
        let result = self
            .hazard_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .take_result()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        drop(self.hazard_job.take());
        if result.source() != self.source || result.source_range() != self.source_range {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        match result.disposition() {
            M11InlineHazardDisposition::Clean => {
                let opaque = self
                    .opaque
                    .take()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?;
                self.emphasis_job = Some(Box::new(M11InlineEmphasisJob::new_with_direct(
                    *opaque,
                    self.direct
                        .as_ref()
                        .ok_or(M11InlineProjectionJobError::InvalidState)?,
                )?));
                self.phase = ProjectionJobPhase::Emphasis;
            }
            M11InlineHazardDisposition::Unsupported { kind, start, end } => {
                self.unsupported = Some(M11InlineProjectionUnsupported {
                    source_range: self.source_range.clone(),
                    first_blocker_range: start..end,
                    reason: M11InlineProjectionUnsupportedReason::LexicalHazard(kind),
                });
                self.phase = ProjectionJobPhase::BeginCleanup;
            }
        }
        *transitions += 1;
        Ok(())
    }

    fn poll_emphasis(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let poll = self
            .emphasis_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11InlineEmphasisPollStatus::Complete {
            self.phase = ProjectionJobPhase::TakeEmphasis;
        }
        Ok(())
    }

    fn take_emphasis(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let candidates = self
            .emphasis_job
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .take_output()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        drop(self.emphasis_job.take());
        self.candidates = Some(Box::new(candidates));
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if candidates.source() != self.source || candidates.source_range() != self.source_range {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        self.phase = ProjectionJobPhase::CheckRemainder;
        *transitions += 1;
        Ok(())
    }

    fn check_remainder(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if candidates.remainder_len() != 0 {
            let remainder = candidates
                .remainder_candidate(0)?
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            self.unsupported = Some(M11InlineProjectionUnsupported {
                source_range: self.source_range.clone(),
                first_blocker_range: remainder.relative_range(),
                reason: M11InlineProjectionUnsupportedReason::AmbiguousEmphasisRemainder {
                    marker: remainder.marker(),
                },
            });
            self.phase = ProjectionJobPhase::BeginCleanup;
        } else {
            self.phase = ProjectionJobPhase::BeginCapture;
        }
        *transitions += 1;
        Ok(())
    }

    fn begin_capture(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        candidates.validate_source(runtime)?;
        self.capture_validator = Some(M11InlineProjectionCaptureValidator::new(
            runtime,
            candidates.source_authority()?,
            self.parser_profile,
        )?);
        self.leaf_scanner = Some(Box::new(M11InlineLexScanner::new(
            candidates.source_cursor(runtime)?,
        )));
        self.phase = ProjectionJobPhase::Emit;
        *transitions += 1;
        Ok(())
    }

    fn poll_emit(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        if self.pending_opaque.is_none() {
            let candidates = self
                .candidates
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            if self.opaque_index < candidates.opaque_len() {
                let candidate = candidates
                    .opaque_candidate(self.opaque_index)?
                    .ok_or(M11InlineProjectionJobError::InvalidState)?;
                self.opaque_index = self
                    .opaque_index
                    .checked_add(1)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if self
                    .direct
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?
                    .suppresses_opaque(candidate)
                {
                    *transitions += 1;
                    return Ok(());
                }
                self.pending_opaque = Some(candidate);
                *transitions += 1;
                return Ok(());
            }
        }
        if self.pending_direct.is_none() {
            let direct = self
                .direct
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            if self.direct_index < direct.len() {
                self.pending_direct = Some(self.direct_index);
                self.direct_index = self
                    .direct_index
                    .checked_add(1)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
        }
        if self.pending_emphasis.is_none() {
            if let Some(index) = self.emphasis_chain.take() {
                let candidates = self
                    .candidates
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?;
                let candidate = candidates
                    .emphasis_candidate(index)?
                    .ok_or(M11InlineProjectionJobError::InvalidState)?;
                self.emphasis_chain = candidate.next_same_opener();
                self.emphasis_visited = self
                    .emphasis_visited
                    .checked_add(1)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if self.emphasis_visited > candidates.emphasis_len() {
                    return Err(M11InlineProjectionJobError::CandidateOrder);
                }
                if self
                    .direct
                    .as_ref()
                    .ok_or(M11InlineProjectionJobError::InvalidState)?
                    .intersects_syntax(candidate.relative_range())
                {
                    *transitions += 1;
                    return Ok(());
                }
                self.pending_emphasis = Some(candidate);
                *transitions += 1;
                return Ok(());
            }
            let candidates = self
                .candidates
                .as_ref()
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            if self.delimiter_index < candidates.delimiter_len() {
                self.emphasis_chain = candidates.delimiter_candidate_head(self.delimiter_index)?;
                self.delimiter_index = self
                    .delimiter_index
                    .checked_add(1)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
        }

        if self.pending_leaf.is_none() && !self.leaf_scan_complete {
            if self.pending_leaf_event.is_some() {
                self.process_pending_leaf(transitions)?;
                return Ok(());
            }
            let poll = self
                .leaf_scanner
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .poll(fuel - *transitions)?;
            *transitions = transitions
                .checked_add(poll.transitions())
                .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
            match poll.status() {
                M11InlineLexPollStatus::Pending => {}
                M11InlineLexPollStatus::Event(event) => {
                    if matches!(
                        event.kind(),
                        M11InlineLexEventKind::BackslashEscape
                            | M11InlineLexEventKind::CharacterReference { .. }
                            | M11InlineLexEventKind::HardLineBreak {
                                continuation_indented: false,
                                ..
                            }
                    ) {
                        self.pending_leaf_event = Some(event);
                    }
                }
                M11InlineLexPollStatus::Complete => {
                    self.leaf_scan_complete = true;
                    drop(self.leaf_scanner.take());
                }
            }
            return Ok(());
        }

        let candidates = self
            .candidates
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if self.pending_opaque.is_none()
            && self.pending_direct.is_none()
            && self.pending_emphasis.is_none()
            && self.pending_leaf.is_none()
        {
            if self.opaque_index != candidates.opaque_len()
                || self.direct_index
                    != self
                        .direct
                        .as_ref()
                        .ok_or(M11InlineProjectionJobError::InvalidState)?
                        .len()
                || self.emphasis_visited != candidates.emphasis_len()
                || !self.leaf_scan_complete
            {
                return Err(M11InlineProjectionJobError::CandidateOrder);
            }
            self.phase = ProjectionJobPhase::BeginEditComponents;
            *transitions += 1;
            return Ok(());
        }

        #[derive(Clone, Copy)]
        enum Choice {
            Opaque(M11InlineOpaqueCandidate),
            Direct { index: u32, start: u32, end: u32 },
            Emphasis(M11EmphasisCandidate),
            Leaf(M11InlineLexEvent),
        }
        impl Choice {
            fn range(self) -> Range<u32> {
                match self {
                    Self::Opaque(candidate) => candidate.relative_range(),
                    Self::Direct { start, end, .. } => start..end,
                    Self::Emphasis(candidate) => candidate.relative_range(),
                    Self::Leaf(event) => event.start()..event.end(),
                }
            }
        }

        let mut choice = self.pending_opaque.map(Choice::Opaque);
        if let Some(index) = self.pending_direct {
            let range = self
                .direct
                .as_ref()
                .and_then(|direct| direct.fact(index))
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .source();
            let candidate = Choice::Direct {
                index,
                start: range.start,
                end: range.end,
            };
            if choice.is_none_or(|current| {
                compare_ranges_preorder(candidate.range(), current.range()) == Ordering::Less
            }) {
                choice = Some(candidate);
            }
        }
        if let Some(emphasis) = self.pending_emphasis {
            let candidate = Choice::Emphasis(emphasis);
            if choice.is_none_or(|current| {
                compare_ranges_preorder(candidate.range(), current.range()) == Ordering::Less
            }) {
                choice = Some(candidate);
            }
        }
        if let Some(leaf) = self.pending_leaf {
            let candidate = Choice::Leaf(leaf);
            if choice.is_none_or(|current| {
                compare_ranges_preorder(candidate.range(), current.range()) == Ordering::Less
            }) {
                choice = Some(candidate);
            }
        }
        let (fact, link_value) = match choice.ok_or(M11InlineProjectionJobError::InvalidState)? {
            Choice::Opaque(opaque) => {
                self.pending_opaque = None;
                (opaque_fact(opaque)?, None)
            }
            Choice::Direct { index, .. } => {
                self.pending_direct = None;
                let ordinal = u32::try_from(
                    self.capture
                        .as_ref()
                        .ok_or(M11InlineProjectionJobError::InvalidState)?
                        .facts
                        .len(),
                )
                .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?;
                let direct = self
                    .direct
                    .as_ref()
                    .and_then(|direct| direct.fact(index))
                    .ok_or(M11InlineProjectionJobError::InvalidState)?;
                let (fact, value) = direct_fact(direct, ordinal)?;
                (fact, Some(value))
            }
            Choice::Emphasis(emphasis) => {
                self.pending_emphasis = None;
                (emphasis_fact(emphasis)?, None)
            }
            Choice::Leaf(event) => {
                self.pending_leaf = None;
                (leaf_fact(event)?, None)
            }
        };
        self.validate_next_fact(fact)?;
        let validator = self
            .capture_validator
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if let Some(value) = link_value.as_ref() {
            validator.offer(&[fact], std::slice::from_ref(value))?;
        } else {
            validator.offer(&[fact], &[])?;
        }
        let capture = self
            .capture
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        capture.facts.push(fact);
        if let Some(value) = link_value {
            capture.link_values.push(value);
        }
        self.phase = ProjectionJobPhase::Emit;
        *transitions += 1;
        Ok(())
    }

    fn process_pending_leaf(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let event = self
            .pending_leaf_event
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if !matches!(
            event.kind(),
            M11InlineLexEventKind::BackslashEscape
                | M11InlineLexEventKind::CharacterReference { .. }
                | M11InlineLexEventKind::HardLineBreak {
                    continuation_indented: false,
                    ..
                }
        ) {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if let Some(candidate) = candidates.opaque_candidate(self.leaf_opaque_index)? {
            let range = candidate.relative_range();
            if range.end <= event.start() {
                self.leaf_opaque_index = self
                    .leaf_opaque_index
                    .checked_add(1)
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
        }
        let direct = self
            .direct
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        if direct
            .syntax_range(self.leaf_direct_syntax_index)
            .is_some_and(|range| range.end <= event.start())
        {
            self.leaf_direct_syntax_index = self
                .leaf_direct_syntax_index
                .checked_add(1)
                .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
            *transitions += 1;
            return Ok(());
        }
        let opaque_candidate = candidates.opaque_candidate(self.leaf_opaque_index)?;
        let opaque_range = opaque_candidate.map(M11InlineOpaqueCandidate::relative_range);
        let direct_range = direct.syntax_range(self.leaf_direct_syntax_index);
        let opaque_shielded = opaque_range
            .as_ref()
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        if opaque_range.as_ref().is_some_and(|range| {
            range.start < event.end() && event.start() < range.end && !opaque_shielded
        }) {
            return Err(M11InlineProjectionJobError::CandidateOrder);
        }
        let direct_shielded = direct_range
            .as_ref()
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        if direct_range.as_ref().is_some_and(|range| {
            range.start < event.end() && event.start() < range.end && !direct_shielded
        }) {
            return Err(M11InlineProjectionJobError::CandidateOrder);
        }
        self.pending_leaf_event = None;
        let retained_inside_uri_autolink = opaque_shielded
            && matches!(
                event.kind(),
                M11InlineLexEventKind::CharacterReference { .. }
            )
            && opaque_candidate
                .is_some_and(|candidate| candidate.kind() == M11InlineOpaqueKind::AutolinkUri);
        if !direct_shielded && (!opaque_shielded || retained_inside_uri_autolink) {
            self.pending_leaf = Some(event);
        }
        *transitions += 1;
        Ok(())
    }

    fn validate_next_fact(
        &mut self,
        fact: M11InlineProjectionFact,
    ) -> Result<(), M11InlineProjectionJobError> {
        let range = fact.relative_range();
        let key = (range.start, u32::MAX - range.end);
        if self.last_order_key.is_some_and(|last| key < last) {
            return Err(M11InlineProjectionJobError::CandidateOrder);
        }
        self.last_order_key = Some(key);
        Ok(())
    }

    fn begin_edit_components(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let facts = self
            .capture
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .facts();
        let exhaustive_brackets = self
            .direct
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .exhaustive_bracket_classification();
        if facts.len() != 1
            || !matches!(
                facts[0].kind(),
                M11InlineProjectionKind::Strong | M11InlineProjectionKind::Emphasis
            )
            || !exhaustive_brackets
        {
            self.phase = ProjectionJobPhase::BeginCleanup;
            *transitions += 1;
            return Ok(());
        }
        let source_len = usize::try_from(
            self.source_range
                .end
                .checked_sub(self.source_range.start)
                .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?,
        )
        .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?;
        if source_len == 0 || source_len > M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES {
            self.phase = ProjectionJobPhase::BeginCleanup;
            *transitions += 1;
            return Ok(());
        }
        let candidates = self
            .candidates
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?;
        self.edit_component_cursor = Some(candidates.source_cursor(runtime)?);
        self.edit_component_source = vec![0; source_len];
        self.edit_component_source_written = 0;
        self.phase = ProjectionJobPhase::CaptureEditComponentSource;
        *transitions += 1;
        Ok(())
    }

    fn poll_edit_component_source(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let remaining_fuel = fuel.saturating_sub(*transitions);
        if remaining_fuel == 0 {
            return Ok(());
        }
        let remaining_bytes = self
            .edit_component_source
            .len()
            .saturating_sub(self.edit_component_source_written);
        if remaining_bytes == 0 {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let chunk = remaining_bytes.min(M11_PARSER_RANGE_MAX_POLL_BYTES);
        let end = self
            .edit_component_source_written
            .checked_add(chunk)
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        let poll = self
            .edit_component_cursor
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll(
                remaining_fuel.min(M11_PARSER_RANGE_MAX_POLL_BYTES),
                &mut self.edit_component_source[self.edit_component_source_written..end],
            )?;
        self.edit_component_source_written = self
            .edit_component_source_written
            .checked_add(poll.bytes_read())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.status() == M11ParserRangeStatus::Complete {
            drop(self.edit_component_cursor.take());
            if self.edit_component_source_written != self.edit_component_source.len() {
                return Err(M11InlineProjectionJobError::InvalidState);
            }
            self.phase = ProjectionJobPhase::BuildEditComponents;
        }
        Ok(())
    }

    fn build_edit_components(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        let facts = self
            .capture
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .facts();
        let exhaustive_brackets = self
            .direct
            .as_ref()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .exhaustive_bracket_classification();
        let components =
            derive_inline_edit_components(&self.edit_component_source, facts, exhaustive_brackets);
        self.capture
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .edit_components = components;
        self.edit_component_source.clear();
        self.edit_component_source_written = 0;
        self.phase = ProjectionJobPhase::BeginCleanup;
        *transitions += 1;
        Ok(())
    }

    fn begin_cleanup(
        &mut self,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        if let Some(code) = self.code.as_mut() {
            code.begin_release()?;
            self.code_release_started = true;
            self.phase = ProjectionJobPhase::CleanupCode;
        } else if let Some(opaque) = self.opaque.as_mut() {
            opaque.begin_release()?;
            self.opaque_release_started = true;
            self.phase = ProjectionJobPhase::CleanupOpaque;
        } else if let Some(candidates) = self.candidates.as_mut() {
            candidates.begin_release()?;
            self.candidate_release_started = true;
            self.phase = ProjectionJobPhase::CleanupCandidates;
        } else {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        *transitions += 1;
        Ok(())
    }

    fn poll_opaque_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        if !self.opaque_release_started {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let poll = self
            .opaque
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll_release(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.complete() {
            let authority = self
                .opaque
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .take_source_authority()
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            drop(self.opaque.take());
            self.complete(authority)?;
        }
        Ok(())
    }

    fn poll_code_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        if !self.code_release_started {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let poll = self
            .code
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll_release(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.complete() {
            let authority = self
                .code
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .take_source_authority()
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            drop(self.code.take());
            self.complete(authority)?;
        }
        Ok(())
    }

    fn poll_candidate_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineProjectionJobError> {
        if !self.candidate_release_started {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let poll = self
            .candidates
            .as_mut()
            .ok_or(M11InlineProjectionJobError::InvalidState)?
            .poll_release(runtime, fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
        if poll.complete() {
            let authority = self
                .candidates
                .as_mut()
                .ok_or(M11InlineProjectionJobError::InvalidState)?
                .take_source_authority()
                .ok_or(M11InlineProjectionJobError::InvalidState)?;
            drop(self.candidates.take());
            self.complete(authority)?;
        }
        Ok(())
    }

    fn complete(
        &mut self,
        authority: M11ParserSourceRangeAuthority,
    ) -> Result<(), M11InlineProjectionJobError> {
        if authority.source() != self.source {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let authority_range = authority.source_range();
        if authority_range.start != self.source_range.start as usize
            || authority_range.end != self.source_range.end as usize
        {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        if self.final_authority.is_some() || self.capture.is_none() {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let authority = match (self.capture_validator.take(), self.unsupported.is_some()) {
            (Some(validator), false) => validator.finish(
                authority,
                self.source,
                self.source_range.clone(),
                self.parser_profile,
            )?,
            (None, true)
                if self.capture.as_ref().is_some_and(|capture| {
                    capture.facts.is_empty()
                        && capture.link_values.is_empty()
                        && capture.edit_components.is_empty()
                }) =>
            {
                authority
            }
            _ => return Err(M11InlineProjectionJobError::InvalidState),
        };
        self.final_authority = Some(authority);
        self.phase = ProjectionJobPhase::Complete;
        Ok(())
    }

    /// Atomically transfers the completed, source-stamped result.
    ///
    /// Authoritative capture is the normal terminal path and needs no release
    /// ceremony. A completed result may instead be discarded through
    /// [`Self::begin_release`].
    #[must_use]
    pub fn take_outcome(&mut self) -> Option<M11InlineProjectionOutcome> {
        if self.phase != ProjectionJobPhase::Complete
            || self.capture.is_none()
            || self.final_authority.is_none()
        {
            return None;
        }
        let capture = self.capture.take().expect("preflighted capture");
        drop(
            self.final_authority
                .take()
                .expect("preflighted final authority"),
        );
        let outcome = if self.unsupported.take().is_some() {
            M11InlineProjectionOutcome::Unsupported {
                source: self.source,
                source_range: self.source_range.clone(),
                parser_profile: self.parser_profile,
            }
        } else {
            M11InlineProjectionOutcome::Authoritative {
                source: self.source,
                source_range: self.source_range.clone(),
                parser_profile: self.parser_profile,
                capture,
            }
        };
        self.phase = ProjectionJobPhase::OutcomeTaken;
        Some(outcome)
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11InlineProjectionJobError> {
        if matches!(
            self.phase,
            ProjectionJobPhase::OutcomeTaken | ProjectionJobPhase::Released
        ) {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        // Cleanup initialization is deliberately retryable. A child begin
        // operation is fallible, and a caller must be able to resume the same
        // move-only owners rather than strand a partially initialized release.
        self.phase = ProjectionJobPhase::Releasing;

        if let Some(cursor) = self.edit_component_cursor.as_mut() {
            cursor.cancel();
        }
        drop(self.edit_component_cursor.take());

        if let Some(scanner) = self.leaf_scanner.as_mut() {
            scanner.cancel();
        }
        drop(self.leaf_scanner.take());
        if let Some(mut hazard) = self.hazard_job.take() {
            hazard.cancel();
            drop(hazard);
        }
        if let Some(mut direct_job) = self.direct_job.take() {
            direct_job.cancel();
            drop(direct_job);
        }
        if let Some(mut bare_autolink_job) = self.bare_autolink_job.take() {
            bare_autolink_job.cancel();
            drop(bare_autolink_job);
        }
        drop(self.direct.take());
        if let Some(code_job) = self.code_job.as_mut() {
            if !self.code_job_abort_started {
                code_job.begin_abort()?;
                self.code_job_abort_started = true;
            }
        }
        if let Some(autolink_job) = self.autolink_job.as_mut() {
            if !self.autolink_job_abort_started {
                autolink_job.begin_abort()?;
                self.autolink_job_abort_started = true;
            }
        }
        if let Some(opaque_job) = self.opaque_job.as_mut() {
            if !self.opaque_job_abort_started {
                opaque_job.begin_abort()?;
                self.opaque_job_abort_started = true;
            }
        }
        if let Some(code) = self.code.as_mut() {
            if !self.code_release_started {
                code.begin_release()?;
                self.code_release_started = true;
            }
        }
        if let Some(opaque) = self.opaque.as_mut() {
            if !self.opaque_release_started {
                opaque.begin_release()?;
                self.opaque_release_started = true;
            }
        }
        if let Some(emphasis_job) = self.emphasis_job.as_mut() {
            if !self.emphasis_abort_started {
                emphasis_job.begin_abort()?;
                self.emphasis_abort_started = true;
            }
        }
        if let Some(candidates) = self.candidates.as_mut() {
            if !self.candidate_release_started {
                candidates.begin_release()?;
                self.candidate_release_started = true;
            }
        }
        let _ = runtime;
        Ok(())
    }

    pub fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineProjectionJobReleasePoll, M11InlineProjectionJobError> {
        validate_fuel(fuel)?;
        if self.phase != ProjectionJobPhase::Releasing {
            return Err(M11InlineProjectionJobError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if let Some(code_job) = self.code_job.as_mut() {
                if !self.code_job_abort_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = code_job.poll_abort(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.code_job.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(autolink_job) = self.autolink_job.as_mut() {
                if !self.autolink_job_abort_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = autolink_job.poll_abort(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.autolink_job.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(opaque_job) = self.opaque_job.as_mut() {
                if !self.opaque_job_abort_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = opaque_job.poll_abort(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.opaque_job.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(code) = self.code.as_mut() {
                if !self.code_release_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = code.poll_release(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.code.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(opaque) = self.opaque.as_mut() {
                if !self.opaque_release_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = opaque.poll_release(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.opaque.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(emphasis_job) = self.emphasis_job.as_mut() {
                if !self.emphasis_abort_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = emphasis_job.poll_abort(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.emphasis_job.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(candidates) = self.candidates.as_mut() {
                if !self.candidate_release_started {
                    return Err(M11InlineProjectionJobError::InvalidState);
                }
                let poll = candidates.poll_release(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.candidates.take());
                    continue;
                }
                return Ok(M11InlineProjectionJobReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            drop(self.capture_validator.take());
            drop(self.final_authority.take());
            drop(self.unsupported.take());
            drop(self.capture.take());
            self.phase = ProjectionJobPhase::Released;
            return Ok(M11InlineProjectionJobReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11InlineProjectionJobReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11InlineProjectionJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    ProjectionJobPhase::OutcomeTaken | ProjectionJobPhase::Released
                ),
                "inline projection jobs require outcome transfer or explicit fuelled release"
            );
        }
    }
}

fn compare_ranges_preorder(left: Range<u32>, right: Range<u32>) -> Ordering {
    left.start
        .cmp(&right.start)
        .then_with(|| right.end.cmp(&left.end))
}

fn opaque_fact(
    candidate: M11InlineOpaqueCandidate,
) -> Result<M11InlineProjectionFact, M11InlineProjectionJobError> {
    let kind = match candidate.kind() {
        M11InlineOpaqueKind::Code => M11InlineProjectionKind::Code,
        M11InlineOpaqueKind::AutolinkUri => M11InlineProjectionKind::AutolinkUri,
        M11InlineOpaqueKind::AutolinkEmail => M11InlineProjectionKind::AutolinkEmail,
    };
    let source = candidate.relative_range();
    let content = candidate.relative_content_range();
    Ok(
        if source == content
            && matches!(
                kind,
                M11InlineProjectionKind::AutolinkUri | M11InlineProjectionKind::AutolinkEmail
            )
        {
            M11InlineProjectionFact::new_bare_autolink(kind, candidate.flags(), source)?
        } else {
            M11InlineProjectionFact::new(kind, candidate.flags(), source, content)?
        },
    )
}

fn direct_fact(
    candidate: &M11InlineDirectFact,
    ordinal: u32,
) -> Result<(M11InlineProjectionFact, M11InlineLinkValue), M11InlineProjectionJobError> {
    let kind = match candidate.kind() {
        M11InlineDirectKind::Link => M11InlineProjectionKind::DirectLink,
        M11InlineDirectKind::Image => M11InlineProjectionKind::DirectImage,
        M11InlineDirectKind::ReferenceLink => M11InlineProjectionKind::ReferenceLink,
        M11InlineDirectKind::ReferenceImage => M11InlineProjectionKind::ReferenceImage,
    };
    let fact = M11InlineProjectionFact::new(kind, 0, candidate.source(), candidate.label_source())?;
    let destination_source = candidate.destination_source();
    let destination_source = u32::try_from(destination_source.start)
        .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?
        ..u32::try_from(destination_source.end)
            .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?;
    let title_source = candidate
        .title_source()
        .map(|range| -> Result<Range<u32>, M11InlineProjectionJobError> {
            Ok(u32::try_from(range.start)
                .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?
                ..u32::try_from(range.end)
                    .map_err(|_| M11InlineProjectionJobError::CoordinateOverflow)?)
        })
        .transpose()?;
    let value = M11InlineLinkValue::new(
        ordinal,
        destination_source,
        title_source,
        candidate.cooked_destination().to_owned().into_boxed_str(),
        candidate
            .cooked_title()
            .map(|title| title.to_owned().into_boxed_str()),
    )?;
    Ok((fact, value))
}

fn emphasis_fact(
    candidate: M11EmphasisCandidate,
) -> Result<M11InlineProjectionFact, M11InlineProjectionJobError> {
    let kind = match candidate.kind() {
        M11EmphasisCandidateKind::Emphasis => M11InlineProjectionKind::Emphasis,
        M11EmphasisCandidateKind::Strong => M11InlineProjectionKind::Strong,
        M11EmphasisCandidateKind::Strikethrough => M11InlineProjectionKind::Strikethrough,
    };
    Ok(M11InlineProjectionFact::new(
        kind,
        0,
        candidate.relative_range(),
        candidate.relative_content_range(),
    )?)
}

fn leaf_fact(
    event: M11InlineLexEvent,
) -> Result<M11InlineProjectionFact, M11InlineProjectionJobError> {
    if let M11InlineLexEventKind::CharacterReference { first, second } = event.kind() {
        return Ok(M11InlineProjectionFact::new_character_reference(
            event.start()..event.end(),
            first,
            second,
        )?);
    }
    let (kind, content_start) = match event.kind() {
        M11InlineLexEventKind::BackslashEscape => (
            M11InlineProjectionKind::BackslashEscape,
            event
                .start()
                .checked_add(1)
                .ok_or(M11InlineProjectionJobError::CoordinateOverflow)?,
        ),
        M11InlineLexEventKind::HardLineBreak {
            content_start,
            continuation_indented: false,
        } => (M11InlineProjectionKind::HardLineBreak, content_start),
        _ => return Err(M11InlineProjectionJobError::InvalidState),
    };
    Ok(M11InlineProjectionFact::new(
        kind,
        0,
        event.start()..event.end(),
        content_start..event.end(),
    )?)
}

fn validate_fuel(fuel: usize) -> Result<(), M11InlineProjectionJobError> {
    if fuel == 0 {
        return Err(M11InlineProjectionJobError::ZeroFuel);
    }
    if fuel > M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS {
        return Err(M11InlineProjectionJobError::PollLimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use comrak::{markdown_to_html, Options as ComrakOptions};
    use flark_engine::parser_internal::{
        M11ReferenceJournal, M11ReferenceJournalOccurrence, M11ReferenceJournalRange,
        M11ReferenceJournalRoot, M11ReferenceJournalStatus,
    };
    use flark_engine::DocumentRuntimeConfig;

    const TEST_PROFILE: u64 = 0x1703;

    #[test]
    fn phase_scratch_is_heap_owned_and_job_stays_stack_bounded() {
        assert!(std::mem::size_of::<M11InlineProjectionJob>() <= 8 * 1024);
    }

    #[test]
    fn successful_capture_never_leaves_a_transient_projection_root() {
        let source = "[**link**](/target \"title\") and `code`";
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let baseline = runtime.arena_metrics();

        for attempt in 0..3 {
            let authority = M11ParserSourceRangeAuthority::new(
                &runtime,
                runtime.snapshot_current_source().expect("authority lease"),
                0..source.len(),
            )
            .expect("range authority");
            let mut job = M11InlineProjectionJob::new_from_exact_authority(
                &runtime,
                authority,
                binding(),
                None,
            )
            .expect("capture job");
            loop {
                let poll = job.poll(&mut runtime, 1).expect("capture poll");
                assert!(poll.transitions() <= 1);
                if poll.status() == M11InlineProjectionJobPollStatus::Complete {
                    break;
                }
                assert_ne!(poll.transitions(), 0, "ready capture must progress");
            }

            let complete = runtime.arena_metrics();
            assert_eq!(complete.resident_nodes, baseline.resident_nodes);
            assert_eq!(complete.live_payload_bytes, baseline.live_payload_bytes);
            assert_eq!(
                complete.reserved_external_payload_bytes,
                baseline.reserved_external_payload_bytes
            );
            assert_eq!(complete.live_builds, baseline.live_builds);
            assert_eq!(complete.pending_reclaims, baseline.pending_reclaims);
            assert_eq!(complete.pending_build_aborts, baseline.pending_build_aborts);

            if attempt == 0 {
                let authority = job.final_authority.take().expect("completed authority");
                assert!(job.take_outcome().is_none());
                assert!(job.capture.is_some(), "failed transfer retains capture");
                job.final_authority = Some(authority);
            }

            let capture = take_authoritative_capture(&mut job);
            assert_eq!(capture.facts().len(), 3);
            assert_eq!(capture.link_values().len(), 1);
            drop(job);

            let transferred = runtime.arena_metrics();
            assert_eq!(transferred.resident_nodes, baseline.resident_nodes);
            assert_eq!(transferred.live_payload_bytes, baseline.live_payload_bytes);
            assert_eq!(
                transferred.reserved_external_payload_bytes,
                baseline.reserved_external_payload_bytes
            );
            assert_eq!(transferred.live_builds, baseline.live_builds);
            assert_eq!(transferred.pending_reclaims, baseline.pending_reclaims);
            assert_eq!(
                transferred.pending_build_aborts,
                baseline.pending_build_aborts
            );
        }

        close_runtime(runtime);
    }

    #[derive(Debug, Eq, PartialEq)]
    struct Resolution {
        source_range: Range<u32>,
        parser_profile: ParserProfileId,
        disposition: M11InlineProjectionDisposition,
        facts: Vec<M11InlineProjectionFact>,
        link_values: Vec<M11InlineLinkValue>,
        maximum_poll_transitions: usize,
    }

    fn binding() -> M11ParserBinding {
        M11ParserBinding::current(
            ParserProfileId::new(TEST_PROFILE).expect("nonzero parser profile"),
        )
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("runtime close").complete {}
        let metrics = runtime.arena_metrics();
        assert_eq!(metrics.reserved_external_payload_bytes, 0);
        assert_eq!(metrics.resident_nodes, 0);
        assert_eq!(metrics.live_builds, 0);
    }

    fn settle_reference_input(journal: &mut M11ReferenceJournal, runtime: &mut DocumentRuntime) {
        loop {
            let poll = journal.poll(runtime, 1).expect("reference journal poll");
            assert!(poll.transitions() <= 1);
            if poll.status() == M11ReferenceJournalStatus::NeedsInput {
                break;
            }
            assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn offer_reference(
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
        source: Range<u64>,
        label_source: Range<u64>,
        destination_source: Range<u64>,
        title_source: Option<Range<u64>>,
        normalized_label: &[u8],
        destination: &[u8],
        title: Option<&[u8]>,
    ) {
        let range = |range: Range<u64>| M11ReferenceJournalRange::new(range.clone(), range);
        journal
            .offer_occurrence(
                runtime,
                M11ReferenceJournalOccurrence::new(
                    range(source),
                    range(label_source),
                    range(destination_source),
                    title_source.map(range),
                    normalized_label,
                    destination,
                    title.map(|title| title.into()),
                ),
            )
            .expect("offer reference");
        settle_reference_input(journal, runtime);
    }

    fn finish_reference_journal(
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
    ) -> M11ReferenceJournalRoot {
        journal.finish_input(runtime).expect("finish references");
        loop {
            let poll = journal.poll(runtime, 1).expect("finish reference poll");
            assert!(poll.transitions() <= 1);
            if poll.status() == M11ReferenceJournalStatus::Complete {
                return journal.take_root().expect("reference root");
            }
            assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
        }
    }

    fn release_reference_root(root: &mut M11ReferenceJournalRoot, runtime: &mut DocumentRuntime) {
        root.begin_release(runtime)
            .expect("begin reference release");
        loop {
            let poll = root
                .poll_release(runtime, 1)
                .expect("reference release poll");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
    }

    fn resolve_in(source_text: &str, source_range: Range<usize>, fuel: usize) -> Resolution {
        let mut runtime =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            source_range.clone(),
        )
        .expect("range authority");
        let mut job =
            M11InlineProjectionJob::new_from_exact_authority(&runtime, authority, binding(), None)
                .expect("capture job");
        let mut maximum_poll_transitions = 0;
        loop {
            let poll = job.poll(&mut runtime, fuel).expect("Projection poll");
            maximum_poll_transitions = maximum_poll_transitions.max(poll.transitions());
            assert!(poll.transitions() <= fuel);
            if poll.status() == M11InlineProjectionJobPollStatus::Pending {
                assert_ne!(
                    poll.transitions(),
                    0,
                    "ready inline Projection stalled in phase {:?} for {source_text:?}",
                    job.phase
                );
            }
            if poll.status() == M11InlineProjectionJobPollStatus::Complete {
                break;
            }
        }
        assert_eq!(job.source, source);
        assert_eq!(job.parser_profile, binding().syntax_profile());
        let source_range =
            u32::try_from(source_range.start).unwrap()..u32::try_from(source_range.end).unwrap();
        assert_eq!(job.source_range, source_range);
        let unsupported = job.unsupported.clone();
        let (disposition, facts, link_values) = match job
            .take_outcome()
            .expect("completed capture has an outcome")
        {
            M11InlineProjectionOutcome::Authoritative {
                source: outcome_source,
                source_range: outcome_range,
                parser_profile,
                capture,
            } => {
                assert_eq!(outcome_source, source);
                assert_eq!(outcome_range, source_range);
                assert_eq!(parser_profile, binding().syntax_profile());
                let (facts, link_values, _) = capture.into_parts();
                (
                    M11InlineProjectionDisposition::Authoritative,
                    facts,
                    link_values,
                )
            }
            M11InlineProjectionOutcome::Unsupported {
                source: outcome_source,
                source_range: outcome_range,
                parser_profile,
            } => {
                assert_eq!(outcome_source, source);
                assert_eq!(outcome_range, source_range);
                assert_eq!(parser_profile, binding().syntax_profile());
                (
                    M11InlineProjectionDisposition::Unsupported(
                        unsupported.expect("fail-closed record"),
                    ),
                    Vec::new(),
                    Vec::new(),
                )
            }
        };
        drop(job);
        close_runtime(runtime);

        Resolution {
            source_range,
            parser_profile: binding().syntax_profile(),
            disposition,
            facts,
            link_values,
            maximum_poll_transitions,
        }
    }

    fn resolve(source_text: &str, fuel: usize) -> Resolution {
        resolve_in(source_text, 0..source_text.len(), fuel)
    }

    fn take_authoritative_capture(job: &mut M11InlineProjectionJob) -> M11InlineProjectionCapture {
        match job.take_outcome().expect("completed inline outcome") {
            M11InlineProjectionOutcome::Authoritative { capture, .. } => capture,
            M11InlineProjectionOutcome::Unsupported { .. } => {
                panic!("expected authoritative inline capture")
            }
        }
    }

    fn assert_single_autolink(example: u32, source: &str, expected_kind: M11InlineProjectionKind) {
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative,
            "CommonMark example {example}"
        );
        assert_eq!(result.facts.len(), 1, "CommonMark example {example}");
        let fact = result.facts[0];
        assert_eq!(fact.kind(), expected_kind, "CommonMark example {example}");
        assert_eq!(fact.flags(), 0, "CommonMark example {example}");
        let outer_end = u32::try_from(source.trim_end_matches('\n').len()).unwrap();
        assert_eq!(
            fact.relative_range(),
            0..outer_end,
            "CommonMark example {example}"
        );
        assert_eq!(
            fact.relative_content_range(),
            1..outer_end - 1,
            "CommonMark example {example}"
        );
    }

    fn assert_no_autolink(example: u32, source: &str) {
        let result = resolve(source, 1);
        assert!(
            matches!(
                result.disposition,
                M11InlineProjectionDisposition::Unsupported(_)
            ),
            "CommonMark example {example} must remain fail-closed"
        );
        assert!(result.facts.is_empty(), "CommonMark example {example}");
    }

    fn assert_single_bare_autolink(
        example: u32,
        source: &str,
        expected_kind: M11InlineProjectionKind,
        expected_flags: u8,
    ) {
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative,
            "GFM example {example}"
        );
        assert_eq!(result.facts.len(), 1, "GFM example {example}");
        let fact = result.facts[0];
        let end = u32::try_from(source.trim_end_matches('\n').len()).unwrap();
        assert_eq!(fact.kind(), expected_kind, "GFM example {example}");
        assert_eq!(fact.flags(), expected_flags, "GFM example {example}");
        assert_eq!(fact.relative_range(), 0..end, "GFM example {example}");
        assert_eq!(
            fact.relative_content_range(),
            0..end,
            "GFM example {example}"
        );
    }

    #[test]
    fn direct_link_fact_and_value_lane_survive_parser_to_engine_projection() {
        let source = "[link](/uri \"title\")";
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(result.facts.len(), 1);
        let fact = result.facts[0];
        assert_eq!(fact.kind(), M11InlineProjectionKind::DirectLink);
        assert_eq!(fact.relative_range(), 0..source.len() as u32);
        assert_eq!(fact.relative_content_range(), 1..5);
        assert_eq!(fact.flags(), 0);
        assert_eq!(result.link_values.len(), 1);
    }

    #[test]
    fn commonmark_direct_link_and_image_forms_are_authoritative() {
        for (example, source, kind, content) in [
            (
                482_u32,
                "[link](/uri \"title\")",
                M11InlineProjectionKind::DirectLink,
                1..5,
            ),
            (485, "[link]()", M11InlineProjectionKind::DirectLink, 1..5),
            (
                489,
                "[link](</my uri>)",
                M11InlineProjectionKind::DirectLink,
                1..5,
            ),
            (
                496,
                "[link](foo(and(bar)))",
                M11InlineProjectionKind::DirectLink,
                1..5,
            ),
            (
                510,
                "[link](   /uri\n  \"title\"  )",
                M11InlineProjectionKind::DirectLink,
                1..5,
            ),
            (
                572,
                "![foo](/url \"title\")",
                M11InlineProjectionKind::DirectImage,
                2..5,
            ),
        ] {
            let result = resolve(source, 1);
            assert_eq!(
                result.disposition,
                M11InlineProjectionDisposition::Authoritative,
                "CommonMark example {example}"
            );
            assert_eq!(result.facts.len(), 1, "CommonMark example {example}");
            let fact = result.facts[0];
            assert_eq!(fact.kind(), kind, "CommonMark example {example}");
            assert_eq!(
                fact.relative_range(),
                0..source.len() as u32,
                "CommonMark example {example}"
            );
            assert_eq!(
                fact.relative_content_range(),
                content,
                "CommonMark example {example}"
            );
            assert_eq!(result.link_values.len(), 1, "CommonMark example {example}");
        }
    }

    #[test]
    fn commonmark_direct_labels_preserve_nested_inline_facts_and_link_precedence() {
        for (example, source, expected) in [
            (
                516_u32,
                "[link *foo **bar** `#`*](/uri)",
                vec![
                    (M11InlineProjectionKind::DirectLink, 0..30, 1..23),
                    (M11InlineProjectionKind::Emphasis, 6..23, 7..22),
                    (M11InlineProjectionKind::Strong, 11..18, 13..16),
                    (M11InlineProjectionKind::Code, 19..22, 20..21),
                ],
            ),
            (
                517,
                "[![moon](moon.jpg)](/uri)",
                vec![
                    (M11InlineProjectionKind::DirectLink, 0..25, 1..18),
                    (M11InlineProjectionKind::DirectImage, 1..18, 3..7),
                ],
            ),
            (
                518,
                "[foo [bar](/uri)](/uri)",
                vec![(M11InlineProjectionKind::DirectLink, 5..16, 6..9)],
            ),
            (
                574,
                "![foo ![bar](/url)](/url2)",
                vec![
                    (M11InlineProjectionKind::DirectImage, 0..26, 2..18),
                    (M11InlineProjectionKind::DirectImage, 6..18, 8..11),
                ],
            ),
            (
                575,
                "![foo [bar](/url)](/url2)",
                vec![
                    (M11InlineProjectionKind::DirectImage, 0..25, 2..17),
                    (M11InlineProjectionKind::DirectLink, 6..17, 7..10),
                ],
            ),
        ] {
            let result = resolve(source, 1);
            assert_eq!(
                result.disposition,
                M11InlineProjectionDisposition::Authoritative,
                "CommonMark example {example}"
            );
            let actual = result
                .facts
                .iter()
                .map(|fact| {
                    (
                        fact.kind(),
                        fact.relative_range(),
                        fact.relative_content_range(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "CommonMark example {example}");
            assert_eq!(
                result.link_values.len(),
                expected
                    .iter()
                    .filter(|(kind, _, _)| {
                        matches!(
                            kind,
                            M11InlineProjectionKind::DirectLink
                                | M11InlineProjectionKind::DirectImage
                        )
                    })
                    .count(),
                "CommonMark example {example}"
            );
        }
    }

    #[test]
    fn reference_shortcut_collapsed_and_incomplete_links_remain_fail_closed() {
        for source in [
            "![foo *bar*]",
            "[foo]",
            "[foo][]",
            "[foo][bar]",
            "[foo](",
            "![foo](",
        ] {
            let result = resolve(source, 1);
            assert!(
                matches!(
                    result.disposition,
                    M11InlineProjectionDisposition::Unsupported(_)
                ),
                "{source:?} must remain fail-closed"
            );
            assert!(result.facts.is_empty(), "{source:?}");
            assert!(result.link_values.is_empty(), "{source:?}");
        }
    }

    #[test]
    fn live_reference_resolver_projects_reference_links_and_images() {
        let source = "[text][BAR] and ![foo][] and [missing]\n\n[bar]: /bar \"B\"\n[foo]: /foo\n";
        let link_source = source.find("[text][BAR]").expect("reference link");
        let image_source = source.find("![foo][]").expect("reference image");
        let paragraph_end = source.find("\n\n").expect("Paragraph boundary") + 1;
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let source_version = runtime.current_source_version().expect("source version");
        let profile = u32::try_from(binding().syntax_profile().get()).expect("u32 profile");
        let mut journal =
            M11ReferenceJournal::new(&mut runtime, source_version, profile).expect("journal");
        let bar_start = source.find("[bar]:").expect("bar definition");
        let bar_end = source[bar_start..].find('\n').unwrap() + bar_start;
        let bar_destination = source[bar_start..].find("/bar").unwrap() + bar_start;
        let bar_title = source[bar_start..].find("\"B\"").unwrap() + bar_start;
        offer_reference(
            &mut journal,
            &mut runtime,
            bar_start as u64..bar_end as u64,
            (bar_start + 1) as u64..(bar_start + 4) as u64,
            bar_destination as u64..(bar_destination + 4) as u64,
            Some(bar_title as u64..(bar_title + 3) as u64),
            b"bar",
            b"/bar",
            Some(b"B"),
        );
        let foo_start = source.find("[foo]:").expect("foo definition");
        let foo_end = source[foo_start..].find('\n').unwrap() + foo_start;
        let foo_destination = source[foo_start..].find("/foo").unwrap() + foo_start;
        offer_reference(
            &mut journal,
            &mut runtime,
            foo_start as u64..foo_end as u64,
            (foo_start + 1) as u64..(foo_start + 4) as u64,
            foo_destination as u64..(foo_destination + 4) as u64,
            None,
            b"foo",
            b"/foo",
            None,
        );
        let mut references = finish_reference_journal(&mut journal, &mut runtime);
        let resolver = M11ReferenceResolver::from_live_reference_journal(&runtime, &references)
            .expect("live reference resolver");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            0..paragraph_end,
        )
        .expect("paragraph authority");
        let mut job = M11InlineProjectionJob::new_from_exact_authority(
            &runtime,
            authority,
            binding(),
            Some(resolver),
        )
        .expect("reference-aware capture job");
        assert!(job.reference_resolver.is_some());
        while job.phase != ProjectionJobPhase::TakeOpaque {
            let poll = job.poll(&mut runtime, 1).expect("Projection poll");
            assert!(poll.transitions() <= 1);
            assert_eq!(poll.status(), M11InlineProjectionJobPollStatus::Pending);
        }
        assert!(job.reference_resolver.is_some());
        assert!(job.direct_job.is_none());
        let take_poll = job.poll(&mut runtime, 1).expect("take opaque");
        assert_eq!(take_poll.transitions(), 1);
        assert_eq!(job.phase, ProjectionJobPhase::Direct);
        assert!(job.reference_resolver.is_none());
        assert!(job.direct_job.is_some());

        loop {
            let poll = job.poll(&mut runtime, 1).expect("Projection poll");
            assert!(poll.transitions() <= 1);
            if poll.status() == M11InlineProjectionJobPollStatus::Complete {
                break;
            }
        }
        let capture = take_authoritative_capture(&mut job);
        let (facts, link_values, _) = capture.into_parts();
        assert_eq!(link_values.len(), 2);
        assert_eq!(
            facts
                .iter()
                .map(|fact| (
                    fact.kind(),
                    fact.relative_range(),
                    fact.relative_content_range(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    M11InlineProjectionKind::ReferenceLink,
                    link_source as u32..(link_source + "[text][BAR]".len()) as u32,
                    (link_source + 1) as u32..(link_source + 5) as u32,
                ),
                (
                    M11InlineProjectionKind::ReferenceImage,
                    image_source as u32..(image_source + "![foo][]".len()) as u32,
                    (image_source + 2) as u32..(image_source + 5) as u32,
                ),
            ]
        );
        drop(job);
        release_reference_root(&mut references, &mut runtime);
        drop(references);
        close_runtime(runtime);
    }

    #[test]
    fn live_reference_resolver_accepts_definition_value_before_leaf() {
        let source = "[foo]: /destination \"title\"\n\n[text][foo]";
        let paragraph_start = source.find("[text][foo]").expect("Paragraph");
        let mut runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let source_version = runtime.current_source_version().expect("current source");
        let profile = u32::try_from(binding().syntax_profile().get()).expect("u32 profile");
        let mut journal =
            M11ReferenceJournal::new(&mut runtime, source_version, profile).expect("journal");
        let definition_end = source.find('\n').expect("definition end");
        let destination_start = source.find("/destination").expect("destination");
        let title_start = source.find("\"title\"").expect("title");
        offer_reference(
            &mut journal,
            &mut runtime,
            0..definition_end as u64,
            1..4,
            destination_start as u64..(destination_start + "/destination".len()) as u64,
            Some(title_start as u64..(title_start + "\"title\"".len()) as u64),
            b"foo",
            b"/destination",
            Some(b"title"),
        );
        let mut references = finish_reference_journal(&mut journal, &mut runtime);
        let resolver = M11ReferenceResolver::from_live_reference_journal(&runtime, &references)
            .expect("live reference resolver");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            paragraph_start..source.len(),
        )
        .expect("Paragraph authority");
        let mut job = M11InlineProjectionJob::new_from_exact_authority(
            &runtime,
            authority,
            binding(),
            Some(resolver),
        )
        .expect("reference-aware capture job");
        loop {
            let poll = job.poll(&mut runtime, 1).expect("Projection poll");
            assert!(poll.transitions() <= 1);
            if poll.status() == M11InlineProjectionJobPollStatus::Complete {
                break;
            }
        }
        let capture = take_authoritative_capture(&mut job);
        let (facts, link_values, _) = capture.into_parts();
        assert_eq!(facts.len(), 1);
        assert_eq!(link_values.len(), 1);
        let fact = facts[0];
        assert_eq!(fact.kind(), M11InlineProjectionKind::ReferenceLink);
        assert_eq!(fact.relative_range(), 0.."[text][foo]".len() as u32);
        assert_eq!(fact.relative_content_range(), 1..5);
        drop(job);
        release_reference_root(&mut references, &mut runtime);
        drop(references);
        close_runtime(runtime);
    }

    #[test]
    fn code_shields_link_spelling_while_label_facts_remain_visible() {
        let source = "`[x](/not)` and [a&amp;b](/yes)";
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        let actual = result
            .facts
            .iter()
            .map(|fact| {
                (
                    fact.kind(),
                    fact.relative_range(),
                    fact.relative_content_range(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (M11InlineProjectionKind::Code, 0..11, 1..10),
                (M11InlineProjectionKind::DirectLink, 16..31, 17..24),
                (M11InlineProjectionKind::CharacterReference, 18..23, 18..23),
            ]
        );
        assert_eq!(result.link_values.len(), 1);
    }

    #[test]
    fn autolink_inside_direct_link_uses_inner_link_wins_policy() {
        let source = "[<https://example.com>](/outer)";
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(result.facts.len(), 1);
        let fact = result.facts[0];
        assert_eq!(fact.kind(), M11InlineProjectionKind::AutolinkUri);
        assert_eq!(fact.relative_range(), 1..22);
        assert_eq!(fact.relative_content_range(), 2..21);
        assert!(result.link_values.is_empty());
    }

    #[test]
    fn commonmark_12_projects_all_32_ascii_punctuation_escapes_exactly() {
        let punctuation = (b'!'..=b'~')
            .filter(|byte| char::from(*byte).is_ascii_punctuation())
            .collect::<Vec<_>>();
        assert_eq!(punctuation.len(), 32);
        let mut source = String::with_capacity(punctuation.len() * 2 + 1);
        for byte in &punctuation {
            source.push('\\');
            source.push(char::from(*byte));
        }
        source.push('\n');
        assert_eq!(
            markdown_to_html(&source, &ComrakOptions::default()),
            "<p>!&quot;#$%&amp;'()*+,-./:;&lt;=&gt;?@[\\]^_`{|}~</p>\n",
            "vendored Comrak CommonMark example 12 oracle"
        );

        let result = resolve(&source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(result.facts.len(), punctuation.len());
        for (index, (fact, punctuation)) in result.facts.iter().zip(punctuation.iter()).enumerate()
        {
            let start = u32::try_from(index * 2).expect("escape start");
            assert_eq!(fact.kind(), M11InlineProjectionKind::BackslashEscape);
            assert_eq!(fact.flags(), 0);
            assert_eq!(fact.relative_range(), start..start + 2);
            assert_eq!(fact.relative_content_range(), start + 1..start + 2);
            let range = fact.relative_range();
            let content = fact.relative_content_range();
            assert_eq!(
                &source[usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()],
                format!("\\{}", char::from(*punctuation))
            );
            assert_eq!(
                &source[usize::try_from(content.start).unwrap()
                    ..usize::try_from(content.end).unwrap()],
                char::from(*punctuation).to_string()
            );
        }
        assert_eq!(result.facts.len(), 32);
    }

    #[test]
    fn commonmark_13_and_16_keep_non_punctuation_literal_and_project_hard_break() {
        let literal = "\\\t\\A\\a\\ \\3\\φ\\«\n";
        assert_eq!(
            markdown_to_html(literal, &ComrakOptions::default()),
            "<p>\\\t\\A\\a\\ \\3\\φ\\«</p>\n",
            "vendored Comrak CommonMark example 13 oracle"
        );
        let result = resolve(literal, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert!(result.facts.is_empty());

        let hard_break = "foo\\\nbar\n";
        assert_eq!(
            markdown_to_html(hard_break, &ComrakOptions::default()),
            "<p>foo<br />\nbar</p>\n",
            "vendored Comrak CommonMark example 16 oracle"
        );
        let result = resolve(hard_break, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(result.facts.len(), 1);
        let fact = result.facts[0];
        assert_eq!(fact.kind(), M11InlineProjectionKind::HardLineBreak);
        assert_eq!(fact.relative_range(), 3..5);
        assert_eq!(fact.relative_content_range(), 4..5);
        assert_eq!(fact.flags(), 0);
    }

    #[test]
    fn hard_line_break_forms_preserve_exact_physical_eol_and_are_fuel_invariant() {
        for ending in ["\n", "\r", "\r\n"] {
            for marker in ["\\", "  ", "   "] {
                let source = format!("before{marker}{ending}after");
                let oracle = markdown_to_html(&source, &ComrakOptions::default());
                assert!(
                    oracle.contains("<br />"),
                    "vendored Comrak oracle for marker={marker:?}, ending={ending:?}"
                );
                let marker_start = u32::try_from("before".len()).unwrap();
                let content_start =
                    marker_start + u32::try_from(marker.len()).expect("marker width");
                let expected_range =
                    marker_start..content_start + u32::try_from(ending.len()).unwrap();
                let expected = resolve(&source, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS);
                assert_eq!(
                    expected.disposition,
                    M11InlineProjectionDisposition::Authoritative
                );
                assert_eq!(expected.facts.len(), 1);
                assert_eq!(
                    expected.facts[0].kind(),
                    M11InlineProjectionKind::HardLineBreak
                );
                assert_eq!(expected.facts[0].relative_range(), expected_range);
                assert_eq!(
                    expected.facts[0].relative_content_range(),
                    content_start..content_start + u32::try_from(ending.len()).unwrap()
                );
                for fuel in [1, 2, 7, 31, 257] {
                    let actual = resolve(&source, fuel);
                    assert_eq!(actual.disposition, expected.disposition, "fuel={fuel}");
                    assert_eq!(actual.facts, expected.facts, "fuel={fuel}");
                    assert!(actual.maximum_poll_transitions <= fuel);
                }
            }
        }
    }

    #[test]
    fn hard_line_breaks_nest_under_emphasis_and_opaque_code_keeps_markers_literal() {
        let source = "*foo\\\nbar*";
        assert_eq!(
            markdown_to_html(source, &ComrakOptions::default()),
            "<p><em>foo<br />\nbar</em></p>\n"
        );
        let result = resolve(source, 1);
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| (
                    fact.kind(),
                    fact.relative_range(),
                    fact.relative_content_range()
                ))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineProjectionKind::Emphasis, 0..10, 1..9),
                (M11InlineProjectionKind::HardLineBreak, 4..6, 5..6),
            ]
        );

        let code = "`a\\\nb`";
        assert_eq!(
            markdown_to_html(code, &ComrakOptions::default()),
            "<p><code>a\\ b</code></p>\n"
        );
        let result = resolve(code, 1);
        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].kind(), M11InlineProjectionKind::Code);
    }

    #[test]
    fn indented_hard_break_continuations_remain_fail_closed() {
        for source in ["foo\\\n bar", "foo  \n\tbar", "foo\\\r\n  bar"] {
            assert!(
                markdown_to_html(source, &ComrakOptions::default()).contains("<br />"),
                "vendored Comrak must still recognize the hard break for {source:?}"
            );
            for fuel in [1, 2, 7, 31] {
                let result = resolve(source, fuel);
                let M11InlineProjectionDisposition::Unsupported(unsupported) = result.disposition
                else {
                    panic!("indented continuation must fail closed for {source:?}");
                };
                assert_eq!(
                    unsupported.reason(),
                    M11InlineProjectionUnsupportedReason::LexicalHazard(
                        M11InlineLexHazardKind::HardBreakCandidate
                    )
                );
                assert!(result.facts.is_empty());
            }
        }
    }

    #[test]
    fn terminal_hard_break_markers_remain_literal_without_a_fact() {
        for source in ["foo\\\n", "foo  \n", "foo\\\r", "foo  \r\n"] {
            let result = resolve(source, 1);
            assert_eq!(
                result.disposition,
                M11InlineProjectionDisposition::Authoritative,
                "{source:?}"
            );
            assert!(result.facts.is_empty(), "{source:?}");
        }
    }

    #[test]
    fn commonmark_25_30_character_references_publish_exact_cooked_replacements() {
        for (source, expected_html, expected_replacement) in [
            ("&nbsp;", "<p>\u{A0}</p>\n", ('\u{A0}', None)),
            ("&amp;", "<p>&amp;</p>\n", ('&', None)),
            ("&ngE;", "<p>≧̸</p>\n", ('≧', Some('\u{338}'))),
            ("&#35;", "<p>#</p>\n", ('#', None)),
            ("&#X22;", "<p>&quot;</p>\n", ('\"', None)),
            ("&#0;", "<p>�</p>\n", ('\u{FFFD}', None)),
        ] {
            assert_eq!(
                markdown_to_html(source, &ComrakOptions::default()),
                expected_html,
                "vendored Comrak oracle for {source:?}"
            );
            for fuel in [1, 2, 7, 31, 257] {
                let result = resolve(source, fuel);
                assert_eq!(
                    result.disposition,
                    M11InlineProjectionDisposition::Authoritative,
                    "source={source:?}, fuel={fuel}"
                );
                assert_eq!(result.facts.len(), 1, "source={source:?}, fuel={fuel}");
                let fact = result.facts[0];
                assert_eq!(
                    fact.kind(),
                    M11InlineProjectionKind::CharacterReference,
                    "source={source:?}, fuel={fuel}"
                );
                assert_eq!(
                    fact.relative_range(),
                    0..u32::try_from(source.len()).expect("source length"),
                    "source={source:?}, fuel={fuel}"
                );
                assert_eq!(
                    fact.character_reference(),
                    Some(expected_replacement),
                    "source={source:?}, fuel={fuel}"
                );
            }
        }
    }

    #[test]
    fn commonmark_28_30_invalid_or_unterminated_entities_remain_literal() {
        for source in [
            "&nbsp",
            "&x;",
            "&#;",
            "&#x;",
            "&#87654321;",
            "&#abcdef0;",
            "&ThisIsNotDefined;",
            "&hi?;",
        ] {
            assert!(
                markdown_to_html(source, &ComrakOptions::default()).contains("&amp;"),
                "vendored Comrak must preserve the leading ampersand for {source:?}"
            );
            for fuel in [1, 2, 7, 31] {
                let result = resolve(source, fuel);
                assert_eq!(
                    result.disposition,
                    M11InlineProjectionDisposition::Authoritative,
                    "source={source:?}, fuel={fuel}"
                );
                assert!(result.facts.is_empty(), "source={source:?}, fuel={fuel}");
            }
        }
    }

    #[test]
    fn character_references_nest_under_emphasis_and_remain_opaque_in_code() {
        let source = "*&copy; &ngE;*";
        assert_eq!(
            markdown_to_html(source, &ComrakOptions::default()),
            "<p><em>© ≧̸</em></p>\n"
        );
        let result = resolve(source, 1);
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.relative_range()))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineProjectionKind::Emphasis, 0..14),
                (M11InlineProjectionKind::CharacterReference, 1..7),
                (M11InlineProjectionKind::CharacterReference, 8..13),
            ]
        );

        let code = "`&amp;`";
        assert_eq!(
            markdown_to_html(code, &ComrakOptions::default()),
            "<p><code>&amp;amp;</code></p>\n"
        );
        let result = resolve(code, 1);
        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].kind(), M11InlineProjectionKind::Code);
    }

    #[test]
    fn pinned_comrak_e000_boundary_is_explicit_differential_evidence() {
        let source = "&#xE000;";
        assert_eq!(
            markdown_to_html(source, &ComrakOptions::default()),
            "<p>�</p>\n",
            "pinned Comrak 0.54 includes U+E000 in its replacement range"
        );
        let result = resolve(source, 1);
        assert_eq!(
            result.facts[0].character_reference(),
            Some(('\u{FFFD}', None))
        );
    }

    #[test]
    fn commonmark_14_block_openers_are_literal_without_weakening_link_hazards() {
        for (source, expected_html, escape_start) in [
            ("1\\. not a list\n", "<p>1. not a list</p>\n", 1_u32),
            ("\\* not a list\n", "<p>* not a list</p>\n", 0),
            ("\\# not a heading\n", "<p># not a heading</p>\n", 0),
        ] {
            assert_eq!(
                markdown_to_html(source, &ComrakOptions::default()),
                expected_html,
                "vendored Comrak CommonMark example 14 oracle for {source:?}"
            );
            let result = resolve(source, 1);
            assert_eq!(
                result.disposition,
                M11InlineProjectionDisposition::Authoritative,
                "{source:?}"
            );
            assert_eq!(result.facts.len(), 1, "{source:?}");
            assert_eq!(
                result.facts[0].kind(),
                M11InlineProjectionKind::BackslashEscape
            );
            assert_eq!(
                result.facts[0].relative_range(),
                escape_start..escape_start + 2
            );
        }

        let full_example = concat!(
            "\\*not emphasized*\n",
            "\\<br/> not a tag\n",
            "\\[not a link](/foo)\n",
            "\\`not code`\n",
            "1\\. not a list\n",
            "\\* not a list\n",
            "\\# not a heading\n",
            "\\[foo]: /url \"not a reference\"\n",
            "\\&ouml; not a character entity\n",
        );
        let result = resolve(full_example, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative,
            "escaped bracket openers are definitively literal without a reference index"
        );
        assert!(result.facts.iter().all(|fact| {
            !matches!(
                fact.kind(),
                M11InlineProjectionKind::DirectLink
                    | M11InlineProjectionKind::DirectImage
                    | M11InlineProjectionKind::ReferenceLink
                    | M11InlineProjectionKind::ReferenceImage
            )
        }));
    }

    #[test]
    fn commonmark_15_17_and_20_preserve_precedence_and_opaque_content() {
        let even_backslashes = "\\\\*emphasis*\n";
        assert_eq!(
            markdown_to_html(even_backslashes, &ComrakOptions::default()),
            "<p>\\<em>emphasis</em></p>\n",
            "vendored Comrak CommonMark example 15 oracle"
        );
        let result = resolve(even_backslashes, 1);
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.relative_range()))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineProjectionKind::BackslashEscape, 0..2),
                (M11InlineProjectionKind::Emphasis, 2..12),
            ]
        );

        let code = "`` \\[\\` ``\n";
        assert_eq!(
            markdown_to_html(code, &ComrakOptions::default()),
            "<p><code>\\[\\`</code></p>\n",
            "vendored Comrak CommonMark example 17 oracle"
        );
        let result = resolve(code, 1);
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| fact.kind())
                .collect::<Vec<_>>(),
            vec![M11InlineProjectionKind::Code],
            "backslashes have no escaping role inside code spans"
        );

        let autolink = "<https://example.com?find=\\*>\n";
        assert_eq!(
            markdown_to_html(autolink, &ComrakOptions::default()),
            "<p><a href=\"https://example.com?find=%5C*\">https://example.com?find=\\*</a></p>\n",
            "vendored Comrak CommonMark example 20 oracle"
        );
        let result = resolve(autolink, 1);
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| fact.kind())
                .collect::<Vec<_>>(),
            vec![M11InlineProjectionKind::AutolinkUri],
            "accepted angle autolinks own their internal backslashes"
        );
    }

    #[test]
    fn escapes_merge_in_source_preorder_inside_emphasis() {
        let source = "*a \\* b* and **c \\_ d**";
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.relative_range()))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineProjectionKind::Emphasis, 0..8),
                (M11InlineProjectionKind::BackslashEscape, 3..5),
                (M11InlineProjectionKind::Strong, 13..23),
                (M11InlineProjectionKind::BackslashEscape, 17..19),
            ]
        );
    }

    #[test]
    fn escape_projection_is_fuel_partition_invariant() {
        let source = format!(
            "{}*a \\* b* {}",
            "x".repeat(crate::inline_lex::M11_INLINE_LEX_MAX_POLL_TRANSITIONS - 1),
            "\\!".repeat(1_024)
        );
        let expected = resolve(&source, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS);
        for fuel in [1, 2, 7, 31, 257] {
            let actual = resolve(&source, fuel);
            assert_eq!(actual.disposition, expected.disposition, "fuel={fuel}");
            assert_eq!(actual.facts, expected.facts, "fuel={fuel}");
            assert_eq!(actual.facts.len(), expected.facts.len(), "fuel={fuel}");
            assert!(actual.maximum_poll_transitions <= fuel);
        }
    }

    #[test]
    fn commonmark_angle_and_gfm_bare_autolink_profiles_are_pinned_by_number() {
        // Source authority is the local CommonMark fixture
        // test/fixtures/commonmark/upstream/common_mark_tests.json. The GFM
        // fixture numbers this same section eight examples later.
        let uri = [
            (594, "<http://foo.bar.baz>\n"),
            (595, "<https://foo.bar.baz/test?q=hello&id=22&boolean>\n"),
            (596, "<irc://foo.bar:2233/baz>\n"),
            (597, "<MAILTO:FOO@BAR.BAZ>\n"),
            (598, "<a+b+c:d>\n"),
            (599, "<made-up-scheme://foo,bar>\n"),
            (600, "<https://../>\n"),
            (601, "<localhost:5001/foo>\n"),
            // CommonMark's semantic link destination retains these source
            // backslashes. Percent-encoding belongs to HTML serialization,
            // not to the parser-owned inline fact.
            (603, "<https://example.com/\\[\\>\n"),
        ];
        for (example, source) in uri {
            assert_single_autolink(example, source, M11InlineProjectionKind::AutolinkUri);
        }
        for (example, source) in [
            (604, "<foo@bar.example.com>\n"),
            (605, "<foo+special@Bar.baz-bar0.com>\n"),
        ] {
            assert_single_autolink(example, source, M11InlineProjectionKind::AutolinkEmail);
        }

        for (example, source) in [
            (602, "<https://foo.bar/baz bim>\n"),
            (606, "<foo\\+@bar.example.com>\n"),
            (607, "<>\n"),
            (608, "< https://foo.bar >\n"),
            (609, "<m:abc>\n"),
            (610, "<foo.bar.baz>\n"),
        ] {
            assert_no_autolink(example, source);
        }
        assert_single_bare_autolink(
            611,
            "https://example.com\n",
            M11InlineProjectionKind::AutolinkUri,
            0,
        );
        assert_single_bare_autolink(
            612,
            "foo@bar.example.com\n",
            M11InlineProjectionKind::AutolinkEmail,
            0,
        );
    }

    #[test]
    fn gfm_bare_autolinks_publish_markerless_source_order_flags_and_fuel_invariance() {
        let source =
            "before https://scheme.example/a www.commonmark.org/help me@example.test after";
        let expected = resolve(source, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS);
        assert_eq!(
            expected.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        let expected_ranges = [
            source.find("https://scheme.example/a").unwrap()
                ..source.find("https://scheme.example/a").unwrap()
                    + "https://scheme.example/a".len(),
            source.find("www.commonmark.org/help").unwrap()
                ..source.find("www.commonmark.org/help").unwrap() + "www.commonmark.org/help".len(),
            source.find("me@example.test").unwrap()
                ..source.find("me@example.test").unwrap() + "me@example.test".len(),
        ];
        assert_eq!(
            expected
                .facts
                .iter()
                .map(|fact| (
                    fact.kind(),
                    fact.flags(),
                    fact.relative_range(),
                    fact.relative_content_range(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    M11InlineProjectionKind::AutolinkUri,
                    0,
                    u32::try_from(expected_ranges[0].start).unwrap()
                        ..u32::try_from(expected_ranges[0].end).unwrap(),
                    u32::try_from(expected_ranges[0].start).unwrap()
                        ..u32::try_from(expected_ranges[0].end).unwrap(),
                ),
                (
                    M11InlineProjectionKind::AutolinkUri,
                    crate::inline_bare_autolink::M11_BARE_AUTOLINK_WWW_FLAG,
                    u32::try_from(expected_ranges[1].start).unwrap()
                        ..u32::try_from(expected_ranges[1].end).unwrap(),
                    u32::try_from(expected_ranges[1].start).unwrap()
                        ..u32::try_from(expected_ranges[1].end).unwrap(),
                ),
                (
                    M11InlineProjectionKind::AutolinkEmail,
                    0,
                    u32::try_from(expected_ranges[2].start).unwrap()
                        ..u32::try_from(expected_ranges[2].end).unwrap(),
                    u32::try_from(expected_ranges[2].start).unwrap()
                        ..u32::try_from(expected_ranges[2].end).unwrap(),
                ),
            ]
        );
        for fuel in [1, 2, 7, 31, 257] {
            let actual = resolve(source, fuel);
            assert_eq!(actual.disposition, expected.disposition, "fuel={fuel}");
            assert_eq!(actual.facts, expected.facts, "fuel={fuel}");
            assert!(actual.maximum_poll_transitions <= fuel);
        }
    }

    #[test]
    fn code_angle_direct_and_bracket_context_precede_gfm_bare_autolinks() {
        let source = concat!(
            "`https://code.example/a` ",
            "<https://angle.example/a> ",
            "[www.label.example](destination) ",
            r"\[ www.escaped-bracket.example ",
            "https://outside.example/a",
        );
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.flags(), fact.relative_range()))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineProjectionKind::Code, 0, 0..24),
                (M11InlineProjectionKind::AutolinkUri, 0, 25..50),
                (M11InlineProjectionKind::DirectLink, 0, 51..83),
                (M11InlineProjectionKind::BackslashEscape, 0, 84..86),
                (
                    M11InlineProjectionKind::AutolinkUri,
                    crate::inline_bare_autolink::M11_BARE_AUTOLINK_WWW_FLAG,
                    87..114,
                ),
                (M11InlineProjectionKind::AutolinkUri, 0, 115..140),
            ]
        );

        let unresolved = resolve("[www.hidden.example] https://outside.example", 1);
        assert!(matches!(
            unresolved.disposition,
            M11InlineProjectionDisposition::Unsupported(_)
        ));
        assert!(unresolved.facts.is_empty());
    }

    #[test]
    fn large_paragraph_bare_scan_is_token_bounded_and_crosses_source_pages() {
        let mut source = "ordinary ".repeat(2_000);
        source.push_str("https://large.example/path");
        let result = resolve(&source, 7);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(result.facts.len(), 1);
        assert_eq!(
            result.facts[0].relative_range().end,
            u32::try_from(source.len()).unwrap()
        );
        assert!(result.maximum_poll_transitions <= 7);

        let overlong = format!(
            "https://large.example/{}",
            "x".repeat(crate::inline_bare_autolink::M11_BARE_AUTOLINK_MAX_TOKEN_BYTES)
        );
        let result = resolve(&overlong, 7);
        assert!(matches!(
            result.disposition,
            M11InlineProjectionDisposition::Unsupported(_)
        ));
        assert!(result.facts.is_empty());
    }

    #[test]
    fn angle_autolinks_obey_code_and_delimiter_precedence_in_both_directions() {
        let code_owned = resolve("`<http://example.test>`", 1);
        assert_eq!(
            code_owned
                .facts
                .iter()
                .map(|fact| fact.kind())
                .collect::<Vec<_>>(),
            vec![M11InlineProjectionKind::Code]
        );

        let angle_owned = resolve("<http://example.test/`tick`>", 1);
        assert_eq!(
            angle_owned
                .facts
                .iter()
                .map(|fact| fact.kind())
                .collect::<Vec<_>>(),
            vec![M11InlineProjectionKind::AutolinkUri],
            "accepted angle ownership must consume overlapping raw runs"
        );

        let composed = resolve("*<http://example.test>*", 1);
        assert_eq!(
            composed
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.relative_range()))
                .collect::<Vec<_>>(),
            vec![
                (M11InlineProjectionKind::Emphasis, 0..23),
                (M11InlineProjectionKind::AutolinkUri, 1..22),
            ]
        );
    }

    #[test]
    fn earlier_angle_ownership_repairs_raw_backtick_pairing_before_hazard_shielding() {
        let source = "<http://x/`> &amp; `";
        for fuel in [1, 2, 7, 31, 257] {
            let result = resolve(source, fuel);
            assert_eq!(
                result.disposition,
                M11InlineProjectionDisposition::Authoritative,
                "fuel={fuel}"
            );
            assert_eq!(
                result
                    .facts
                    .iter()
                    .map(|fact| (fact.kind(), fact.relative_range()))
                    .collect::<Vec<_>>(),
                vec![
                    (M11InlineProjectionKind::AutolinkUri, 0..12),
                    (M11InlineProjectionKind::CharacterReference, 13..18),
                ],
                "fuel={fuel}"
            );
            assert_eq!(
                result.facts[1].character_reference(),
                Some(('&', None)),
                "fuel={fuel}"
            );
        }
    }

    #[test]
    fn earlier_angle_ownership_repairs_cross_angle_and_later_run_pairing() {
        let cross_angle = "<ab:`> <cd:`>";
        let repaired_later_code = "<ab:`> `code`";
        for fuel in [1, 2, 7, 31, 257] {
            let result = resolve(cross_angle, fuel);
            assert_eq!(
                result
                    .facts
                    .iter()
                    .map(|fact| (fact.kind(), fact.relative_range()))
                    .collect::<Vec<_>>(),
                vec![
                    (M11InlineProjectionKind::AutolinkUri, 0..6),
                    (M11InlineProjectionKind::AutolinkUri, 7..13),
                ],
                "cross-angle fuel={fuel}"
            );

            let result = resolve(repaired_later_code, fuel);
            assert_eq!(
                result
                    .facts
                    .iter()
                    .map(|fact| (fact.kind(), fact.relative_range()))
                    .collect::<Vec<_>>(),
                vec![
                    (M11InlineProjectionKind::AutolinkUri, 0..6),
                    (M11InlineProjectionKind::Code, 7..13),
                ],
                "the two later single runs must repair into outside code, fuel={fuel}"
            );
        }
    }

    #[test]
    fn commonmark_480_and_481_autolinks_shield_internal_emphasis_closers() {
        for (example, source) in [
            (480, "**a<https://foo.bar/?q=**>\n"),
            (481, "__a<https://foo.bar/?q=__>\n"),
        ] {
            let result = resolve(source, 1);
            assert_eq!(
                result
                    .facts
                    .iter()
                    .map(|fact| fact.kind())
                    .collect::<Vec<_>>(),
                vec![M11InlineProjectionKind::AutolinkUri],
                "CommonMark example {example}"
            );
            assert_eq!(
                result.facts[0].relative_range(),
                3..u32::try_from(source.trim_end().len()).unwrap(),
                "CommonMark example {example}"
            );
        }
    }

    #[test]
    fn character_references_use_one_lexer_authority_inside_angle_autolinks() {
        let source = "<http://example.test/?x=&amp;>\n";
        assert_eq!(
            markdown_to_html(source, &ComrakOptions::default()),
            concat!(
                "<p><a href=\"http://example.test/?x=&amp;\">",
                "http://example.test/?x=&amp;</a></p>\n"
            ),
            "vendored Comrak decodes the reference in both destination and label"
        );
        let entity_start = u32::try_from(source.find("&amp;").expect("entity")).expect("offset");
        let entity_end = entity_start + 5;
        for fuel in [1, 2, 7, 31, 257] {
            let result = resolve(source, fuel);
            assert_eq!(
                result.disposition,
                M11InlineProjectionDisposition::Authoritative,
                "fuel={fuel}"
            );
            assert_eq!(result.facts.len(), 2, "fuel={fuel}");
            assert_eq!(
                result.facts[0].kind(),
                M11InlineProjectionKind::AutolinkUri,
                "fuel={fuel}"
            );
            assert_eq!(
                result.facts[0].relative_range(),
                0..u32::try_from(source.trim_end().len()).expect("source length"),
                "fuel={fuel}"
            );
            assert_eq!(
                result.facts[1].kind(),
                M11InlineProjectionKind::CharacterReference,
                "fuel={fuel}"
            );
            assert_eq!(
                result.facts[1].relative_range(),
                entity_start..entity_end,
                "fuel={fuel}"
            );
            assert_eq!(
                result.facts[1].character_reference(),
                Some(('&', None)),
                "fuel={fuel}"
            );
        }

        let invalid = resolve("<http://example.test/?x=&not-an-entity;>\n", 1);
        assert_eq!(invalid.facts.len(), 1);
        assert_eq!(
            invalid.facts[0].kind(),
            M11InlineProjectionKind::AutolinkUri
        );
    }

    #[test]
    fn email_domain_edges_match_the_vendored_comrak_differential_oracle() {
        let label_63 = "a".repeat(63);
        let label_64 = "a".repeat(64);
        let cases = [
            ("a@b".to_owned(), true),
            (".a@b".to_owned(), true),
            ("a@.b".to_owned(), false),
            ("a@b.".to_owned(), false),
            ("a@b..c".to_owned(), false),
            ("a@-b".to_owned(), false),
            ("a@b-".to_owned(), false),
            (format!("a@{label_63}"), true),
            (format!("a@{label_64}"), false),
        ];
        for (address, expected) in cases {
            let source = format!("<{address}>\n");
            let oracle =
                markdown_to_html(&source, &ComrakOptions::default()).contains("<a href=\"mailto:");
            assert_eq!(oracle, expected, "vendored Comrak oracle for {address:?}");
            let actual = resolve(&source, 1);
            let admitted = actual.facts.len() == 1
                && actual.facts[0].kind() == M11InlineProjectionKind::AutolinkEmail;
            assert_eq!(admitted, oracle, "streaming resolver for {address:?}");
        }
    }

    #[test]
    fn angle_autolink_results_are_fuel_partition_invariant() {
        let source = "*<http://a.test/?q=&unknown;>* and <me@b.test>";
        let expected = resolve(source, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS);
        for fuel in [1, 2, 7, 31, 257] {
            let actual = resolve(source, fuel);
            assert_eq!(actual.disposition, expected.disposition, "fuel={fuel}");
            assert_eq!(actual.facts, expected.facts, "fuel={fuel}");
            assert!(actual.maximum_poll_transitions <= fuel);
        }
    }

    #[test]
    fn long_angle_autolink_crossing_a_utf8_window_boundary_is_partition_invariant() {
        let window = crate::inline_autolink::M11_INLINE_AUTOLINK_SOURCE_WINDOW_BYTES;
        let prefix = "x".repeat(window - "<xy:".len() - 1);
        let source = format!("{prefix}<xy:é{}>", "a".repeat(window * 3));
        let expected = resolve(&source, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS);
        assert_eq!(
            expected
                .facts
                .iter()
                .map(|fact| (fact.kind(), fact.relative_range()))
                .collect::<Vec<_>>(),
            vec![(
                M11InlineProjectionKind::AutolinkUri,
                u32::try_from(prefix.len()).unwrap()..u32::try_from(source.len()).unwrap()
            )]
        );
        for fuel in [1, 2, 31, 257] {
            let actual = resolve(&source, fuel);
            assert_eq!(actual.disposition, expected.disposition, "fuel={fuel}");
            assert_eq!(actual.facts, expected.facts, "fuel={fuel}");
            assert!(actual.maximum_poll_transitions <= fuel);
        }
    }

    #[test]
    fn mixed_code_and_nested_emphasis_emit_one_source_preordered_fact_per_transition() {
        let source = "***bold*** and `code`";
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| fact.kind())
                .collect::<Vec<_>>(),
            vec![
                M11InlineProjectionKind::Emphasis,
                M11InlineProjectionKind::Strong,
                M11InlineProjectionKind::Code,
            ]
        );
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| fact.relative_range())
                .collect::<Vec<_>>(),
            vec![0..10, 1..9, 15..21]
        );
        assert_eq!(result.facts.len(), 3);
    }

    #[test]
    fn gfm_strikethrough_is_authoritative_and_composes_with_emphasis_and_code() {
        let source = "~~*gone*~~ and ~also~ and `~~literal~~`";
        let result = resolve(source, 1);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| fact.kind())
                .collect::<Vec<_>>(),
            vec![
                M11InlineProjectionKind::Strikethrough,
                M11InlineProjectionKind::Emphasis,
                M11InlineProjectionKind::Strikethrough,
                M11InlineProjectionKind::Code,
            ]
        );
        assert_eq!(
            result
                .facts
                .iter()
                .map(|fact| fact.relative_range())
                .collect::<Vec<_>>(),
            vec![0..10, 2..8, 15..21, 26..39]
        );
        assert!(result.facts.iter().all(|fact| fact.flags() == 0));

        let incomplete = resolve("~~still typing", 1);
        assert_eq!(
            incomplete.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert!(incomplete.facts.is_empty());
    }

    #[test]
    fn plain_paragraph_produces_an_authoritative_empty_capture() {
        let result = resolve("plain paragraph", 3);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert!(result.facts.is_empty());
    }

    #[test]
    fn strong_paragraph_never_yields_a_zero_progress_ready_poll() {
        let result = resolve("**plain**", 32);
        assert_eq!(
            result.disposition,
            M11InlineProjectionDisposition::Authoritative
        );
        assert_eq!(result.facts.len(), 1);
    }

    #[test]
    fn supported_bare_email_retires_its_hazard_before_later_html_fails_closed() {
        let source = "before name@example.test and <tag> after";
        let result = resolve(source, 2);
        let M11InlineProjectionDisposition::Unsupported(unsupported) = result.disposition else {
            panic!("expected unsupported");
        };
        assert_eq!(unsupported.source_range(), 0..source.len() as u32);
        assert_eq!(unsupported.first_blocker_range(), 29..30);
        assert_eq!(
            unsupported.reason(),
            M11InlineProjectionUnsupportedReason::LexicalHazard(
                M11InlineLexHazardKind::HtmlCandidate
            )
        );
        assert!(result.facts.is_empty());
    }

    #[test]
    fn ambiguous_emphasis_remainder_fails_the_whole_range_closed() {
        let source = "**wow*";
        let result = resolve(source, 1);
        let M11InlineProjectionDisposition::Unsupported(unsupported) = result.disposition else {
            panic!("expected unsupported");
        };
        assert_eq!(unsupported.source_range(), 0..source.len() as u32);
        assert_eq!(unsupported.first_blocker_range(), 0..1);
        assert_eq!(
            unsupported.reason(),
            M11InlineProjectionUnsupportedReason::AmbiguousEmphasisRemainder { marker: b'*' }
        );
        assert!(result.facts.is_empty());
    }

    #[test]
    fn poll_partition_and_nonzero_visible_range_do_not_change_projection() {
        let prefix = "OUT:";
        let visible = "***bold*** and `code`";
        let source = format!("{prefix}{visible}");
        let expected = resolve_in(
            &source,
            prefix.len()..source.len(),
            M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
        );
        assert_eq!(
            expected.source_range,
            prefix.len() as u32..source.len() as u32
        );
        for fuel in [1, 2, 7, 31, 257] {
            let actual = resolve_in(&source, prefix.len()..source.len(), fuel);
            assert_eq!(actual.disposition, expected.disposition, "fuel={fuel}");
            assert_eq!(actual.facts, expected.facts, "fuel={fuel}");
            assert_eq!(actual.link_values, expected.link_values, "fuel={fuel}");
            assert!(actual.maximum_poll_transitions <= fuel);
        }
    }

    #[test]
    fn grammar_revision_three_binding_is_rejected_before_escape_derivation() {
        let source = "\\*literal*";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            0..source.len(),
        )
        .expect("authority");
        let stale = M11ParserBinding::new(binding().syntax_profile(), 3);
        let error =
            M11InlineProjectionJob::new_from_exact_authority(&runtime, authority, stale, None)
                .expect_err("grammar revision 3 must not reuse revision 5 inline semantics");
        assert!(matches!(
            error.0,
            M11InlineProjectionJobErrorInner::UnsupportedGrammarRevision { actual: 3 }
        ));
        close_runtime(runtime);
    }

    #[test]
    fn partial_work_aborts_and_reclaims_every_owner_with_fuel_one() {
        let source = "*x* ".repeat(20_000);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            0..source.len(),
        )
        .expect("authority");
        let mut job =
            M11InlineProjectionJob::new_from_exact_authority(&runtime, authority, binding(), None)
                .expect("capture job");
        while runtime.arena_metrics().reserved_external_payload_bytes == 0 {
            let poll = job.poll(&mut runtime, 257).expect("partial work");
            assert_eq!(poll.status(), M11InlineProjectionJobPollStatus::Pending);
        }
        job.begin_release(&mut runtime).expect("begin release");
        job.begin_release(&mut runtime)
            .expect("retry release initialization");
        loop {
            let poll = job.poll_release(&mut runtime, 1).expect("release poll");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        drop(job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }

    #[test]
    fn partial_final_escape_and_hard_break_scan_aborts_and_reclaims_every_owner() {
        let source = "\\* x\\\ny ".repeat(10_000);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            0..source.len(),
        )
        .expect("authority");
        let mut job =
            M11InlineProjectionJob::new_from_exact_authority(&runtime, authority, binding(), None)
                .expect("capture job");
        loop {
            let poll = job.poll(&mut runtime, 257).expect("partial work");
            assert_eq!(poll.status(), M11InlineProjectionJobPollStatus::Pending);
            if job.phase == ProjectionJobPhase::Emit && job.leaf_scanner.is_some() {
                break;
            }
        }
        let poll = job
            .poll(&mut runtime, 1)
            .expect("begin final atomic-leaf scan");
        assert_eq!(poll.status(), M11InlineProjectionJobPollStatus::Pending);
        assert!(job.leaf_scanner.is_some());

        job.begin_release(&mut runtime).expect("begin release");
        loop {
            let poll = job.poll_release(&mut runtime, 1).expect("release poll");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        drop(job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }

    #[test]
    fn partial_angle_autolink_radix_work_aborts_and_reclaims_with_fuel_one() {
        let source = "<http://example.test/a> ".repeat(2_000);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = M11ParserSourceRangeAuthority::new(
            &runtime,
            runtime.snapshot_current_source().expect("authority lease"),
            0..source.len(),
        )
        .expect("authority");
        let mut job =
            M11InlineProjectionJob::new_from_exact_authority(&runtime, authority, binding(), None)
                .expect("capture job");
        loop {
            let poll = job.poll(&mut runtime, 257).expect("partial work");
            assert_eq!(poll.status(), M11InlineProjectionJobPollStatus::Pending);
            if job.phase == ProjectionJobPhase::Autolink
                && runtime.arena_metrics().reserved_external_payload_bytes > 0
            {
                break;
            }
        }
        assert!(job.autolink_job.is_some());
        job.begin_release(&mut runtime).expect("begin release");
        loop {
            let poll = job.poll_release(&mut runtime, 1).expect("release poll");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        drop(job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }
}

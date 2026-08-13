use std::fmt;
use std::mem;
use std::ops::Range;

use flark_engine::parser_internal::{
    M11InlineProjectionFact, M11InlineProjectionKind, M11RecursiveGreenPoint,
    M11RecursiveGreenRenderableRow, M11RecursiveGreenRowEditCapability,
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowQueryOutcome,
    M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
    M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE,
};
use flark_engine::{
    ArenaMetrics, DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, ParserProfileId,
    SourceBoundaryAffinity, SourceEditError, SourceSnapshotLease, SourceVersion,
};
use flark_parser::{
    block_core::{
        m11_recursive_green_row_presentation, BulletMarker, FenceCharacter, HeadingStyle,
        ListDelimiter, M11RecursiveGreenCodeBlockStyle, M11RecursiveGreenInlineLeafKind,
        M11RecursiveGreenListMarker, M11RecursiveGreenRowPresentation,
    },
    classify_m11_simple_edit_line, project_m11_gfm_table, M11GfmTableAlignment,
    M11InlineProjectionJob, M11InlineProjectionJobError, M11InlineProjectionJobPollStatus,
    M11ParserBinding, M11PersistentRecursiveGreenAdoption,
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenAdoptionWork,
    M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanBuild,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
    M11PersistentRecursiveGreenSessionError, M11SimpleEditLineKind, M11SimpleEditListMarker,
    M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS, M11_SIMPLE_EDIT_LINE_MAX_BYTES,
};

use crate::edit_intent::{
    resolve_document_edit_intent_v1, DocumentEditLineEnding, DocumentListOutdent,
    DocumentParagraphMerge, DocumentSimpleEditContext, DocumentSimpleEditRow,
};
use crate::{
    DocumentEditIntentDispositionV1, DocumentEditIntentReceiptV1, DocumentEditIntentV1,
    DocumentEditPresentationTransitionV1, DocumentSourceTransactionReceiptV1,
};

const SYNTAX_PROFILE_GFM_V1: u32 = 1;
const QUERY_OPEN_DEPTH_LIMIT: usize = 256;
const VIEWPORT_INLINE_LEAF_MAX_BYTES: u64 = 8 * 1024;
const VIEWPORT_INLINE_FACTS_PER_ROW_MAX: usize = 512;
const VIEWPORT_INLINE_TOTAL_TRANSITIONS_MAX: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DocumentSessionPhase {
    #[default]
    Building,
    Ready,
    Closing,
    Closed,
    Faulted,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentPumpReceipt {
    pub revision: u64,
    pub work_units: usize,
    pub phase: DocumentSessionPhase,
    pub last_edit_work: M11PersistentRecursiveGreenAdoptionWork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentEditReceipt {
    pub revision: u64,
    pub parser_pending: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentCloseReceipt {
    pub work_units: usize,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentViewportRowEditCapability {
    Contiguous,
    ProjectedReserved,
    Unavailable,
}

/// Parser-authored authority for retaining one row's presentation while an
/// exact source transaction is waiting for current-revision certification.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DocumentViewportRowContinuityPolicy {
    #[default]
    None,
    /// A conservative plain-text edit inside the parser's contiguous editable
    /// range can retain identity when the host's bounded validator approves
    /// the exact transaction.
    PlainTextEdit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentHeadingStyle {
    Atx,
    Setext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentBulletMarker {
    Hyphen,
    Plus,
    Asterisk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentListDelimiter {
    Period,
    Parenthesis,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentListMarker {
    Bullet(DocumentBulletMarker),
    Ordered {
        value: u32,
        delimiter: DocumentListDelimiter,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentFenceCharacter {
    Backtick,
    Tilde,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentCodeBlockStyle {
    Indented,
    Fenced {
        fence: DocumentFenceCharacter,
        minimum_closing_length: u32,
        fence_offset: u8,
        closed: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentInlineFactKind {
    Emphasis,
    Strong,
    Code,
    Strikethrough,
    AutolinkUri,
    AutolinkEmail,
    BackslashEscape,
    HardLineBreak,
    Replacement,
    DirectLink,
    DirectImage,
    ReferenceLink,
    ReferenceImage,
    TableCell,
}

pub const DOCUMENT_TABLE_CELL_ALIGNMENT_MASK: u8 = 0x03;
pub const DOCUMENT_TABLE_CELL_HEADER: u8 = 1 << 2;
pub const DOCUMENT_TABLE_CELL_ROW_START: u8 = 1 << 3;
pub const DOCUMENT_TABLE_CELL_AUTOCOMPLETED: u8 = 1 << 4;
/// The parser authorizes transaction-bound continuity for conservative plain
/// text edits wholly inside this fact's visible content range.
pub const DOCUMENT_INLINE_FACT_CONTINUITY_PLAIN_TEXT: u8 = 1 << 7;

/// Parser-cooked visible text replacing one exact source range.
///
/// The selected grammar currently needs at most two Unicode scalar values for
/// a character reference and one scalar for a normalized code line ending.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentInlineReplacement {
    pub first: char,
    pub second: Option<char>,
}

/// One parser-authored inline semantic in absolute document coordinates.
///
/// `source_range` contains the complete Markdown form. `content_range` names
/// the visible content after its opening and closing marker cuts are removed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentInlineFact {
    pub kind: DocumentInlineFactKind,
    pub flags: u8,
    pub source_range: Range<u64>,
    pub source_utf16_range: Range<u64>,
    pub content_range: Range<u64>,
    pub content_utf16_range: Range<u64>,
    pub replacement: Option<DocumentInlineReplacement>,
}

/// One ordered identity-source cut in a parser-certified projected row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentProjectionSegment {
    pub source_range: Range<u64>,
    pub source_utf16_range: Range<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DocumentViewportRowPresentation {
    #[default]
    Plain,
    Heading {
        level: u8,
        style: DocumentHeadingStyle,
    },
    ListItem {
        marker: DocumentListMarker,
        prefix_start_byte: u64,
        prefix_end_byte: u64,
        prefix_start_utf16: u64,
        prefix_end_utf16: u64,
        nesting_depth: u8,
        marker_offset: u8,
        container_widths: u64,
        container_count: u8,
        marker_column: u8,
        simple_continuation: bool,
        starts_list: bool,
        task_checked: Option<bool>,
    },
    BlockQuote {
        prefix_start_byte: u64,
        prefix_end_byte: u64,
        prefix_start_utf16: u64,
        prefix_end_utf16: u64,
        nesting_depth: u8,
        simple_continuation: bool,
    },
    CodeBlock {
        style: DocumentCodeBlockStyle,
    },
    ThematicBreak,
    Table,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentViewportRow {
    pub ordinal: u64,
    pub kind: u16,
    pub source_range: Range<u64>,
    pub source_utf16_range: Range<u64>,
    pub editable_range: Option<Range<u64>>,
    pub editable_utf16_range: Option<Range<u64>>,
    pub edit_capability: DocumentViewportRowEditCapability,
    pub continuity_policy: DocumentViewportRowContinuityPolicy,
    pub presentation: DocumentViewportRowPresentation,
    /// `Some` means the complete bounded inline leaf is authoritative. Empty
    /// is distinct from `None`, which requires exact-source neutral display.
    pub inline_facts: Option<Vec<DocumentInlineFact>>,
    /// Present only for `ProjectedReserved` rows. Every segment is exact
    /// source; gaps are parser-certified hidden container material.
    pub projection_segments: Option<Vec<DocumentProjectionSegment>>,
    pub path_depth: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentQueryReceipt {
    pub storage_pages_visited: u64,
    pub events_scanned: u64,
    pub tree_nodes_visited: u64,
    pub maximum_open_depth: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentViewport {
    pub revision: u64,
    pub requested_range: Range<u64>,
    pub start_ordinal: u64,
    pub total_rows: u64,
    pub complete: bool,
    pub rows: Vec<DocumentViewportRow>,
    pub receipt: DocumentQueryReceipt,
}

/// One current-revision item in a mixed live viewport. Certified rows carry
/// parser-authored structural facts; pending spans carry only exact source
/// coordinates and must be painted neutrally.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentLiveViewportSpan {
    Pending {
        source_range: Range<u64>,
        source_utf16_range: Range<u64>,
    },
    CertifiedUnchanged {
        source_range: Range<u64>,
        source_utf16_range: Range<u64>,
    },
}

impl DocumentLiveViewportSpan {
    #[must_use]
    pub fn source_range(&self) -> Range<u64> {
        match self {
            Self::Pending { source_range, .. } | Self::CertifiedUnchanged { source_range, .. } => {
                source_range.clone()
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentLiveViewport {
    pub revision: u64,
    pub requested_range: Range<u64>,
    pub covered_range: Range<u64>,
    pub complete: bool,
    pub spans: Vec<DocumentLiveViewportSpan>,
    pub receipt: DocumentQueryReceipt,
}

impl DocumentLiveViewport {
    #[must_use]
    pub fn is_fully_certified(&self) -> bool {
        self.spans
            .iter()
            .all(|span| matches!(span, DocumentLiveViewportSpan::CertifiedUnchanged { .. }))
    }
}

#[derive(Debug)]
pub enum DocumentSessionError {
    ZeroWorkBudget,
    Busy,
    NotReady,
    Faulted,
    StaleRevision { expected: u64, actual: u64 },
    RangeOutOfBounds,
    EditIntentLimitExceeded,
    UnsupportedEditIntentSelection,
    QueryBudgetExceeded,
    Engine(DocumentRuntimeError),
    Source(SourceEditError),
    Parser(M11PersistentRecursiveGreenSessionError),
    Inline(M11InlineProjectionJobError),
}

impl DocumentSessionError {
    #[must_use]
    pub const fn is_backpressure(&self) -> bool {
        matches!(
            self,
            Self::Engine(DocumentRuntimeError::RetirementBackpressure { .. })
        )
    }
}

impl fmt::Display for DocumentSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWorkBudget => formatter.write_str("document work budget must be nonzero"),
            Self::Busy => formatter.write_str("document has parser work in flight"),
            Self::NotReady => formatter.write_str("document semantics are not ready"),
            Self::Faulted => formatter.write_str("document parser is faulted"),
            Self::StaleRevision { expected, actual } => {
                write!(
                    formatter,
                    "stale revision {expected}; current revision is {actual}"
                )
            }
            Self::RangeOutOfBounds => formatter.write_str("source range is out of bounds"),
            Self::EditIntentLimitExceeded => {
                formatter.write_str("semantic edit exceeds the bounded small-edit envelope")
            }
            Self::UnsupportedEditIntentSelection => {
                formatter.write_str("semantic edit result cannot use mechanical anchor mapping")
            }
            Self::QueryBudgetExceeded => formatter.write_str("viewport query budget exhausted"),
            Self::Engine(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
            Self::Parser(error) => error.fmt(formatter),
            Self::Inline(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DocumentSessionError {}

impl From<DocumentRuntimeError> for DocumentSessionError {
    fn from(value: DocumentRuntimeError) -> Self {
        Self::Engine(value)
    }
}

impl From<SourceEditError> for DocumentSessionError {
    fn from(value: SourceEditError) -> Self {
        Self::Source(value)
    }
}

impl From<M11PersistentRecursiveGreenSessionError> for DocumentSessionError {
    fn from(value: M11PersistentRecursiveGreenSessionError) -> Self {
        Self::Parser(value)
    }
}

impl From<M11InlineProjectionJobError> for DocumentSessionError {
    fn from(value: M11InlineProjectionJobError) -> Self {
        Self::Inline(value)
    }
}

enum ParseState {
    Clean(Box<M11PersistentRecursiveGreenCleanBuild>),
    CancellingClean(Box<M11PersistentRecursiveGreenCleanBuild>),
    Ready(Box<M11PersistentRecursiveGreenSession>),
    Adopting(Box<M11PersistentRecursiveGreenAdoption>),
    CancellingAdoption(Box<M11PersistentRecursiveGreenAdoption>),
    ReleasingBaseForTarget {
        base: Box<M11PersistentRecursiveGreenSession>,
        target: Box<M11PersistentRecursiveGreenSession>,
    },
    ReleasingBaseForClean(Box<M11PersistentRecursiveGreenSession>),
    ClosingClean(Box<M11PersistentRecursiveGreenCleanBuild>),
    ClosingAdoption(Box<M11PersistentRecursiveGreenAdoption>),
    ClosingSession {
        current: Box<M11PersistentRecursiveGreenSession>,
        next: Option<Box<M11PersistentRecursiveGreenSession>>,
    },
    ClosingRuntime,
    Closed,
    Faulted,
    Transition,
}

pub struct DocumentSession {
    runtime: DocumentRuntime,
    parser: ParseState,
    last_edit_work: M11PersistentRecursiveGreenAdoptionWork,
    fault_arena_metrics: Option<ArenaMetrics>,
    edit_context: Option<DocumentSimpleEditContext>,
    fallback_line_ending: DocumentEditLineEnding,
}

impl fmt::Debug for DocumentSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentSession")
            .field("revision", &self.revision())
            .field("phase", &self.phase())
            .finish_non_exhaustive()
    }
}

impl DocumentSession {
    pub fn begin(source: &str) -> Result<Self, DocumentSessionError> {
        Self::begin_with_config(source, DocumentRuntimeConfig::default())
    }

    fn begin_with_config(
        source: &str,
        config: DocumentRuntimeConfig,
    ) -> Result<Self, DocumentSessionError> {
        let fallback_line_ending = dominant_edit_line_ending(source.as_bytes());
        let mut runtime = DocumentRuntime::new(source, config)?;
        let parser = ParseState::Clean(Box::new(begin_clean_build(&mut runtime)?));
        Ok(Self {
            runtime,
            parser,
            last_edit_work: M11PersistentRecursiveGreenAdoptionWork::default(),
            fault_arena_metrics: None,
            edit_context: None,
            fallback_line_ending,
        })
    }

    #[must_use]
    pub fn revision(&self) -> u64 {
        self.runtime
            .current_source_version()
            .and_then(|source| source.revision().get().checked_add(1))
            .unwrap_or(0)
    }

    #[must_use]
    pub fn source_byte_len(&self) -> usize {
        self.runtime
            .current_source_version()
            .map_or(0, SourceVersion::byte_len)
    }

    #[must_use]
    pub fn source_utf16_len(&self) -> usize {
        self.runtime
            .current_source_version()
            .map_or(0, SourceVersion::utf16_len)
    }

    /// Current persistent-arena residency for capacity evidence and fault
    /// diagnosis. These counters are logical admitted bytes, not process RSS.
    #[must_use]
    pub const fn arena_metrics(&self) -> ArenaMetrics {
        self.runtime.arena_metrics()
    }

    /// Arena residency captured before fault cleanup releases the failed
    /// candidate. This is absent for sessions that have not faulted.
    #[must_use]
    pub const fn fault_arena_metrics(&self) -> Option<ArenaMetrics> {
        self.fault_arena_metrics
    }

    #[must_use]
    pub const fn phase(&self) -> DocumentSessionPhase {
        match self.parser {
            ParseState::Ready(_) => DocumentSessionPhase::Ready,
            ParseState::ClosingClean(_)
            | ParseState::ClosingAdoption(_)
            | ParseState::ClosingSession { .. }
            | ParseState::ClosingRuntime => DocumentSessionPhase::Closing,
            ParseState::Closed => DocumentSessionPhase::Closed,
            ParseState::Faulted => DocumentSessionPhase::Faulted,
            ParseState::Clean(_)
            | ParseState::CancellingClean(_)
            | ParseState::Adopting(_)
            | ParseState::CancellingAdoption(_)
            | ParseState::ReleasingBaseForTarget { .. }
            | ParseState::ReleasingBaseForClean(_)
            | ParseState::Transition => DocumentSessionPhase::Building,
        }
    }

    pub fn pump(
        &mut self,
        max_work_units: usize,
    ) -> Result<DocumentPumpReceipt, DocumentSessionError> {
        if max_work_units == 0 {
            return Err(DocumentSessionError::ZeroWorkBudget);
        }
        if matches!(self.parser, ParseState::Faulted) {
            return Err(DocumentSessionError::Faulted);
        }

        let mut consumed = 0;
        while consumed < max_work_units {
            let retirement = self.runtime.poll_retirement(1);
            if retirement.released_source_leases > 0 || retirement.arena_transitions > 0 {
                consumed += 1;
                continue;
            }
            if matches!(self.parser, ParseState::Ready(_)) {
                break;
            }
            let state = mem::replace(&mut self.parser, ParseState::Transition);
            let next = self.advance_one(state);
            match next {
                Ok(state) => self.parser = state,
                Err(error) => {
                    if self.fault_arena_metrics.is_none() {
                        self.fault_arena_metrics = Some(self.runtime.arena_metrics());
                    }
                    self.parser = ParseState::Faulted;
                    return Err(error);
                }
            }
            consumed += 1;
        }

        Ok(DocumentPumpReceipt {
            revision: self.revision(),
            work_units: consumed,
            phase: self.phase(),
            last_edit_work: self.last_edit_work,
        })
    }

    fn advance_one(&mut self, state: ParseState) -> Result<ParseState, DocumentSessionError> {
        match state {
            ParseState::Clean(mut build) => {
                // A recursive-green build asserts on drop unless its root was
                // transferred or it was explicitly cancelled, so an error here
                // must release the build rather than let it fall out of scope:
                // otherwise the assertion kills the document actor thread and
                // every later call reports an opaque internal fault instead of
                // this typed parser error.
                let poll = match build.poll(&mut self.runtime, 1) {
                    Ok(poll) => poll,
                    Err(error) => {
                        self.fault_arena_metrics = Some(self.runtime.arena_metrics());
                        release_failed_clean_build(&mut self.runtime, build);
                        return Err(error.into());
                    }
                };
                if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                    match build.take_session() {
                        Some(session) => Ok(ParseState::Ready(Box::new(session))),
                        None => {
                            release_failed_clean_build(&mut self.runtime, build);
                            Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                                "completed clean build omitted its session",
                            )
                            .into())
                        }
                    }
                } else {
                    Ok(ParseState::Clean(build))
                }
            }
            ParseState::CancellingClean(mut build) => {
                let poll = build.poll_cancel(&mut self.runtime, 1)?;
                if poll.status() == M11PersistentRecursiveGreenBuildStatus::Cancelled {
                    Ok(ParseState::Clean(Box::new(begin_clean_build(
                        &mut self.runtime,
                    )?)))
                } else {
                    Ok(ParseState::CancellingClean(build))
                }
            }
            ParseState::Adopting(mut adoption) => {
                let poll = adoption.poll(&mut self.runtime, 1)?;
                match poll.status() {
                    M11PersistentRecursiveGreenAdoptionStatus::Pending => {
                        Ok(ParseState::Adopting(adoption))
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                        let mut update = adoption.take_update().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "completed adoption omitted its update",
                            ),
                        )?;
                        self.last_edit_work = update.work();
                        let target = update.take_target().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "completed adoption omitted its target",
                            ),
                        )?;
                        let mut base = update.take_base().ok_or(
                            M11PersistentRecursiveGreenSessionError::InvalidState(
                                "completed adoption omitted its base",
                            ),
                        )?;
                        base.begin_release(&mut self.runtime)?;
                        Ok(ParseState::ReleasingBaseForTarget {
                            base: Box::new(base),
                            target: Box::new(target),
                        })
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                        adoption.begin_cancel(&mut self.runtime)?;
                        Ok(ParseState::CancellingAdoption(adoption))
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                        Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                            "active adoption reported cancelled",
                        )
                        .into())
                    }
                }
            }
            ParseState::CancellingAdoption(mut adoption) => {
                if adoption.poll_cancel(&mut self.runtime, 1)? {
                    let mut base = adoption.take_base_after_cancel().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "cancelled adoption omitted its base",
                        ),
                    )?;
                    base.begin_release(&mut self.runtime)?;
                    Ok(ParseState::ReleasingBaseForClean(Box::new(base)))
                } else {
                    Ok(ParseState::CancellingAdoption(adoption))
                }
            }
            ParseState::ReleasingBaseForTarget { mut base, target } => {
                if base.poll_release(&mut self.runtime, 1)? {
                    Ok(ParseState::Ready(target))
                } else {
                    Ok(ParseState::ReleasingBaseForTarget { base, target })
                }
            }
            ParseState::ReleasingBaseForClean(mut base) => {
                if base.poll_release(&mut self.runtime, 1)? {
                    Ok(ParseState::Clean(Box::new(begin_clean_build(
                        &mut self.runtime,
                    )?)))
                } else {
                    Ok(ParseState::ReleasingBaseForClean(base))
                }
            }
            ParseState::Ready(session) => Ok(ParseState::Ready(session)),
            ParseState::ClosingClean(_)
            | ParseState::ClosingAdoption(_)
            | ParseState::ClosingSession { .. }
            | ParseState::ClosingRuntime
            | ParseState::Closed => Err(DocumentSessionError::Busy),
            ParseState::Faulted | ParseState::Transition => Err(DocumentSessionError::Faulted),
        }
    }

    pub fn apply_edit(
        &mut self,
        expected_revision: u64,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<DocumentEditReceipt, DocumentSessionError> {
        let actual_revision = self.revision();
        if expected_revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        let base_utf16_range = self
            .runtime
            .snapshot_current_source()
            .ok()
            .and_then(|lease| {
                Some(
                    lease.utf16_offset_for_byte(range.start).ok()?
                        ..lease.utf16_offset_for_byte(range.end).ok()?,
                )
            });
        let parser_is_ready = matches!(&self.parser, ParseState::Ready(_));
        let base_edit_context = self
            .edit_context
            .as_ref()
            .filter(|context| context.revision == expected_revision)
            .cloned()
            .or_else(|| self.capture_ready_edit_context(range.start))
            .or_else(|| {
                (!parser_is_ready)
                    .then(|| self.capture_exact_edit_context(range.start))
                    .flatten()
            });
        let state = mem::replace(&mut self.parser, ParseState::Transition);
        let result = match state {
            ParseState::Ready(base) => {
                self.apply_edit_to_ready_base(base, range.clone(), replacement)
            }
            ParseState::Faulted => {
                self.parser = ParseState::Faulted;
                Err(DocumentSessionError::Faulted)
            }
            ParseState::Clean(mut build) => {
                if let Err(error) = build.begin_cancel(&mut self.runtime) {
                    self.parser = ParseState::Clean(build);
                    return Err(error.into());
                }
                let result = self.apply_edit_while_building(range.clone(), replacement);
                self.parser = ParseState::CancellingClean(build);
                result
            }
            ParseState::CancellingClean(build) => {
                let result = self.apply_edit_while_building(range.clone(), replacement);
                self.parser = ParseState::CancellingClean(build);
                result
            }
            ParseState::Adopting(mut adoption) => {
                if let Err(error) = adoption.begin_cancel(&mut self.runtime) {
                    self.parser = ParseState::Adopting(adoption);
                    return Err(error.into());
                }
                let result = self.apply_edit_while_building(range.clone(), replacement);
                self.parser = ParseState::CancellingAdoption(adoption);
                result
            }
            ParseState::CancellingAdoption(adoption) => {
                let result = self.apply_edit_while_building(range.clone(), replacement);
                self.parser = ParseState::CancellingAdoption(adoption);
                result
            }
            ParseState::ReleasingBaseForClean(base) => {
                let result = self.apply_edit_while_building(range.clone(), replacement);
                self.parser = ParseState::ReleasingBaseForClean(base);
                result
            }
            closing @ (ParseState::ClosingClean(_)
            | ParseState::ClosingAdoption(_)
            | ParseState::ClosingSession { .. }
            | ParseState::ClosingRuntime
            | ParseState::Closed) => {
                self.parser = closing;
                Err(DocumentSessionError::Busy)
            }
            other => {
                self.parser = other;
                Err(DocumentSessionError::Busy)
            }
        };
        if let Ok(receipt) = result {
            self.edit_context = match (base_edit_context, base_utf16_range) {
                (Some(context), Some(utf16_range)) => self.transform_edit_context(
                    context,
                    range,
                    utf16_range,
                    replacement,
                    receipt.revision,
                ),
                _ => None,
            };
        }
        result
    }

    /// Validates and commits one caller-known literal splice while computing
    /// every coordinate needed for atomic selection-anchor retargeting before
    /// the source linearization point.
    pub fn apply_source_transaction_v1(
        &mut self,
        expected_revision: u64,
        base_utf16_range: Range<usize>,
        replacement: &str,
        result_selection_base_utf16: usize,
        result_selection_extent_utf16: usize,
    ) -> Result<DocumentSourceTransactionReceiptV1, DocumentSessionError> {
        let actual_revision = self.revision();
        if expected_revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        if base_utf16_range.start > base_utf16_range.end {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let base_byte_range = self.byte_offset_for_utf16(base_utf16_range.start)?
            ..self.byte_offset_for_utf16(base_utf16_range.end)?;
        let replacement_utf16_length = replacement.encode_utf16().count();
        let result_source_utf16_length = self
            .source_utf16_len()
            .checked_sub(base_utf16_range.len())
            .and_then(|length| length.checked_add(replacement_utf16_length))
            .ok_or(DocumentSessionError::RangeOutOfBounds)?;
        if result_selection_base_utf16 > result_source_utf16_length
            || result_selection_extent_utf16 > result_source_utf16_length
        {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let result_utf16_range = base_utf16_range.start
            ..base_utf16_range
                .start
                .checked_add(replacement_utf16_length)
                .ok_or(DocumentSessionError::RangeOutOfBounds)?;
        let result_byte_range = base_byte_range.start
            ..base_byte_range
                .start
                .checked_add(replacement.len())
                .ok_or(DocumentSessionError::RangeOutOfBounds)?;

        // Complete these conversions before apply_edit. ABI finalization can
        // then transform all anchors and retarget the canonical pair without
        // another fallible actor call after the source has changed.
        let result_selection_base_byte = self.result_byte_for_source_transaction_utf16(
            &base_byte_range,
            &base_utf16_range,
            replacement,
            result_selection_base_utf16,
        )?;
        let result_selection_extent_byte = self.result_byte_for_source_transaction_utf16(
            &base_byte_range,
            &base_utf16_range,
            replacement,
            result_selection_extent_utf16,
        )?;
        let inverse = self.source_bytes(base_byte_range.clone())?;
        let edit = self.apply_edit(expected_revision, base_byte_range.clone(), replacement)?;
        Ok(DocumentSourceTransactionReceiptV1 {
            base_revision: expected_revision,
            result_revision: edit.revision,
            committed_splice: crate::DocumentCommittedSpliceV1 {
                base_byte_range,
                base_utf16_range,
                replacement: replacement.to_owned(),
                result_byte_range,
                result_utf16_range,
            },
            inverse,
            result_selection_base_utf16,
            result_selection_extent_utf16,
            result_selection_base_byte,
            result_selection_extent_byte,
            result_source_byte_length: self.source_byte_len(),
            result_source_utf16_length: self.source_utf16_len(),
            parser_pending: edit.parser_pending,
        })
    }

    fn result_byte_for_source_transaction_utf16(
        &self,
        base_byte_range: &Range<usize>,
        base_utf16_range: &Range<usize>,
        replacement: &str,
        result_utf16: usize,
    ) -> Result<usize, DocumentSessionError> {
        let replacement_utf16_length = replacement.encode_utf16().count();
        let result_replacement_end = base_utf16_range
            .start
            .checked_add(replacement_utf16_length)
            .ok_or(DocumentSessionError::RangeOutOfBounds)?;
        if result_utf16 <= base_utf16_range.start {
            return self.byte_offset_for_utf16(result_utf16);
        }
        if result_utf16 >= result_replacement_end {
            let original_utf16 = base_utf16_range
                .end
                .checked_add(result_utf16 - result_replacement_end)
                .ok_or(DocumentSessionError::RangeOutOfBounds)?;
            let original_byte = self.byte_offset_for_utf16(original_utf16)?;
            return original_byte
                .checked_sub(base_byte_range.len())
                .and_then(|byte| byte.checked_add(replacement.len()))
                .ok_or(DocumentSessionError::RangeOutOfBounds);
        }

        let wanted = result_utf16 - base_utf16_range.start;
        let mut utf16 = 0usize;
        for (byte, scalar) in replacement.char_indices() {
            if utf16 == wanted {
                return base_byte_range
                    .start
                    .checked_add(byte)
                    .ok_or(DocumentSessionError::RangeOutOfBounds);
            }
            utf16 += scalar.len_utf16();
            if utf16 > wanted {
                return Err(DocumentSessionError::RangeOutOfBounds);
            }
        }
        if utf16 == wanted {
            return base_byte_range
                .start
                .checked_add(replacement.len())
                .ok_or(DocumentSessionError::RangeOutOfBounds);
        }
        Err(DocumentSessionError::RangeOutOfBounds)
    }

    /// Resolves and commits the collapsed-caret `flark-edit-v1` subset without
    /// waiting for parser certification. The semantic context is either read
    /// from the current certified row or carried through exact local lineage.
    pub fn try_apply_edit_intent_v1(
        &mut self,
        expected_revision: u64,
        intent: DocumentEditIntentV1,
        selection_utf16: usize,
        composition_active: bool,
    ) -> Result<DocumentEditIntentReceiptV1, DocumentSessionError> {
        let selection_byte = self.byte_offset_for_utf16(selection_utf16)?;
        self.try_apply_edit_intent_v1_at_byte(
            expected_revision,
            intent,
            selection_byte,
            composition_active,
        )
    }

    pub fn try_apply_edit_intent_v1_at_byte(
        &mut self,
        expected_revision: u64,
        intent: DocumentEditIntentV1,
        selection_byte: usize,
        composition_active: bool,
    ) -> Result<DocumentEditIntentReceiptV1, DocumentSessionError> {
        let actual_revision = self.revision();
        if expected_revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        if selection_byte > self.source_byte_len() {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let selection_utf16 = self.utf16_offset_for_byte(selection_byte)?;
        if composition_active {
            return Ok(DocumentEditIntentReceiptV1 {
                disposition: DocumentEditIntentDispositionV1::NotApplicable,
                base_revision: expected_revision,
                result_revision: expected_revision,
                committed_splice: None,
                inverse: Vec::new(),
                result_selection_byte: selection_byte,
                result_selection_utf16: selection_utf16,
                result_source_byte_length: self.source_byte_len(),
                result_source_utf16_length: self.source_utf16_len(),
                parser_pending: self.phase() != DocumentSessionPhase::Ready,
                presentation_transition: DocumentEditPresentationTransitionV1::None,
            });
        }

        let parser_is_ready = matches!(&self.parser, ParseState::Ready(_));
        let context = self
            .edit_context
            .as_ref()
            .filter(|context| {
                context.revision == expected_revision
                    && selection_byte >= context.editable_bytes.start
                    && selection_byte <= context.editable_bytes.end
            })
            .cloned()
            .or_else(|| self.capture_ready_edit_context(selection_byte))
            .or_else(|| {
                (!parser_is_ready)
                    .then(|| self.capture_exact_edit_context(selection_byte))
                    .flatten()
            });
        let Some(context) = context else {
            return Ok(DocumentEditIntentReceiptV1 {
                disposition: DocumentEditIntentDispositionV1::NeedsCurrentSemantics,
                base_revision: expected_revision,
                result_revision: expected_revision,
                committed_splice: None,
                inverse: Vec::new(),
                result_selection_byte: selection_byte,
                result_selection_utf16: selection_utf16,
                result_source_byte_length: self.source_byte_len(),
                result_source_utf16_length: self.source_utf16_len(),
                parser_pending: self.phase() != DocumentSessionPhase::Ready,
                presentation_transition: DocumentEditPresentationTransitionV1::None,
            });
        };
        let resolved =
            resolve_document_edit_intent_v1(intent, selection_byte, selection_utf16, &context);
        let Some(splice) = resolved.splice.clone() else {
            return Ok(DocumentEditIntentReceiptV1 {
                disposition: resolved.disposition,
                base_revision: expected_revision,
                result_revision: expected_revision,
                committed_splice: None,
                inverse: Vec::new(),
                result_selection_byte: selection_byte,
                result_selection_utf16: resolved.result_selection_utf16,
                result_source_byte_length: self.source_byte_len(),
                result_source_utf16_length: self.source_utf16_len(),
                parser_pending: self.phase() != DocumentSessionPhase::Ready,
                presentation_transition: DocumentEditPresentationTransitionV1::None,
            });
        };
        let expected_result_selection_utf16 = transformed_collapsed_selection(
            selection_utf16,
            &splice.base_utf16_range,
            splice.replacement.encode_utf16().count(),
        )
        .ok_or(DocumentSessionError::UnsupportedEditIntentSelection)?;
        if resolved.result_selection_utf16 != expected_result_selection_utf16 {
            return Err(DocumentSessionError::UnsupportedEditIntentSelection);
        }
        let result_selection_byte = transformed_collapsed_selection(
            selection_byte,
            &splice.base_byte_range,
            splice.replacement.len(),
        )
        .ok_or(DocumentSessionError::UnsupportedEditIntentSelection)?;

        // Reuse the context already resolved above. Without this handoff the
        // generic one-splice commit path performs a redundant current-row
        // query before applying the exact semantic splice.
        self.edit_context = Some(context);
        let semantic_bytes = 32usize
            .checked_add(splice.base_byte_range.len())
            .and_then(|bytes| bytes.checked_add(splice.replacement.len()))
            .ok_or(DocumentSessionError::EditIntentLimitExceeded)?;
        if semantic_bytes > crate::MAX_SMALL_EDIT_BYTES as usize {
            return Err(DocumentSessionError::EditIntentLimitExceeded);
        }
        let inverse = self.source_bytes(splice.base_byte_range.clone())?;
        let edit = self.apply_edit(
            expected_revision,
            splice.base_byte_range.clone(),
            &splice.replacement,
        )?;
        self.edit_context = resolved.result_context;
        Ok(DocumentEditIntentReceiptV1 {
            disposition: DocumentEditIntentDispositionV1::Applied,
            base_revision: expected_revision,
            result_revision: edit.revision,
            committed_splice: Some(splice),
            inverse,
            result_selection_byte,
            result_selection_utf16: resolved.result_selection_utf16,
            result_source_byte_length: self.source_byte_len(),
            result_source_utf16_length: self.source_utf16_len(),
            parser_pending: edit.parser_pending,
            presentation_transition: resolved.presentation_transition,
        })
    }

    fn capture_ready_edit_context(
        &mut self,
        selection_byte: usize,
    ) -> Option<DocumentSimpleEditContext> {
        if selection_byte > self.source_byte_len() {
            return None;
        }
        let requested_start = self
            .snapped_to_scalar_boundary(selection_byte.saturating_sub(16))
            .ok()?;
        // A parser-authored zero-length row is positioned after its complete
        // physical line ending. Include both bytes of CRLF when the caret is
        // immediately before that ending.
        let requested_end = selection_byte
            .checked_add(2)
            .unwrap_or(selection_byte)
            .min(self.source_byte_len());
        let revision = self.revision();
        let session = match &self.parser {
            ParseState::Ready(session) => session,
            _ => return None,
        };
        let viewport = query_session_viewport(
            &mut self.runtime,
            session,
            revision,
            requested_start..requested_end,
            8,
        )
        .ok()?;
        let current = viewport.rows.iter().find(|row| {
            row.editable_range.as_ref().is_some_and(|range| {
                selection_byte >= range.start as usize && selection_byte <= range.end as usize
            })
        });
        let Some(current) = current else {
            // Empty structural markers may have no renderable certified row.
            // Admit only an exact, isolated empty construct in that case.
            let mut exact = self.capture_exact_edit_context(selection_byte)?;
            if matches!(
                exact.row,
                DocumentSimpleEditRow::ListItem { empty: true, .. }
            ) {
                self.certify_absent_empty_list(&mut exact, &viewport.rows)?;
            }
            return Some(exact).filter(|context| {
                matches!(
                    context.row,
                    DocumentSimpleEditRow::AtxHeading { empty: true, .. }
                        | DocumentSimpleEditRow::BlockQuote { empty: true, .. }
                        | DocumentSimpleEditRow::ListItem { empty: true, .. }
                )
            });
        };
        if matches!(
            current.presentation,
            DocumentViewportRowPresentation::BlockQuote {
                nesting_depth: 1,
                simple_continuation: false,
                ..
            }
        ) && current.edit_capability == DocumentViewportRowEditCapability::ProjectedReserved
        {
            return self.capture_projected_block_quote_edit_context(
                current,
                selection_byte,
                viewport.revision,
            );
        }
        if matches!(
            current.presentation,
            DocumentViewportRowPresentation::CodeBlock {
                style: DocumentCodeBlockStyle::Indented,
            }
        ) && current.edit_capability != DocumentViewportRowEditCapability::Unavailable
        {
            return self.capture_projected_indented_code_edit_context(
                current,
                selection_byte,
                viewport.revision,
            );
        }
        if current.presentation == DocumentViewportRowPresentation::ThematicBreak
            && current.path_depth == 1
        {
            let source_bytes = u64_range_to_usize(&current.source_range)?;
            let source_utf16 = u64_range_to_usize(&current.source_utf16_range)?;
            let editable_bytes = u64_range_to_usize(current.editable_range.as_ref()?)?;
            let editable_utf16 = u64_range_to_usize(current.editable_utf16_range.as_ref()?)?;
            if !editable_bytes.is_empty()
                || !editable_utf16.is_empty()
                || selection_byte != editable_bytes.start
            {
                return None;
            }
            let (atom_bytes, atom_utf16) =
                self.source_range_without_bof_bom(source_bytes.clone(), source_utf16.clone())?;
            return Some(DocumentSimpleEditContext {
                revision: viewport.revision,
                source_bytes,
                source_utf16,
                editable_bytes,
                editable_utf16,
                ending: self.fallback_line_ending,
                row: DocumentSimpleEditRow::ThematicBreak {
                    atom_bytes,
                    atom_utf16,
                },
                paragraph_merge: None,
            });
        }
        if matches!(
            current.presentation,
            DocumentViewportRowPresentation::Heading {
                style: DocumentHeadingStyle::Atx,
                ..
            }
        ) {
            // Level/style is certified above. Only marker geometry comes from
            // the bounded exact classifier, so it cannot override Setext or a
            // different certified construct.
            return self
                .capture_exact_edit_context(selection_byte)
                .filter(|context| matches!(context.row, DocumentSimpleEditRow::AtxHeading { .. }));
        }
        let source_bytes = u64_range_to_usize(&current.source_range)?;
        let source_utf16 = u64_range_to_usize(&current.source_utf16_range)?;
        let editable_bytes = u64_range_to_usize(current.editable_range.as_ref()?)?;
        let editable_utf16 = u64_range_to_usize(current.editable_utf16_range.as_ref()?)?;
        let ending = self
            .edit_line_ending_at(source_bytes.end)
            .unwrap_or(self.fallback_line_ending);

        let row = match current.presentation {
            DocumentViewportRowPresentation::Plain if current.kind == 5 => {
                DocumentSimpleEditRow::Plain
            }
            DocumentViewportRowPresentation::ListItem {
                marker,
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                nesting_depth,
                marker_offset,
                container_widths,
                container_count,
                marker_column,
                simple_continuation: true,
                starts_list,
                task_checked,
            } if nesting_depth >= 1 => {
                let prefix_bytes = usize::try_from(prefix_start_byte).ok()?
                    ..usize::try_from(prefix_end_byte).ok()?;
                let prefix_utf16 = usize::try_from(prefix_start_utf16).ok()?
                    ..usize::try_from(prefix_end_utf16).ok()?;
                let outdent = (nesting_depth > 1)
                    .then(|| {
                        self.capture_nested_list_outdent(
                            prefix_bytes.start,
                            prefix_utf16.start,
                            nesting_depth,
                            marker_offset,
                            marker_column,
                            container_widths,
                            container_count,
                        )
                    })
                    .flatten();
                if nesting_depth > 1 && outdent.is_none() {
                    return None;
                }
                DocumentSimpleEditRow::ListItem {
                    marker,
                    prefix_bytes,
                    prefix_utf16,
                    nesting_depth,
                    marker_offset,
                    container_widths,
                    container_count,
                    marker_column,
                    starts_list,
                    task_checked,
                    empty: current.kind == 14 || editable_bytes.is_empty(),
                    outdent,
                }
            }
            DocumentViewportRowPresentation::BlockQuote {
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                nesting_depth: 1,
                simple_continuation: true,
            } => {
                let prefix_bytes = usize::try_from(prefix_start_byte).ok()?
                    ..usize::try_from(prefix_end_byte).ok()?;
                let prefix_text =
                    String::from_utf8(self.source_bytes(prefix_bytes.clone()).ok()?).ok()?;
                DocumentSimpleEditRow::BlockQuote {
                    prefix_bytes,
                    prefix_utf16: usize::try_from(prefix_start_utf16).ok()?
                        ..usize::try_from(prefix_end_utf16).ok()?,
                    prefix_text,
                    starts_quote: true,
                    empty: editable_bytes.is_empty(),
                }
            }
            _ => return None,
        };

        let paragraph_merge = matches!(row, DocumentSimpleEditRow::Plain)
            .then(|| {
                self.capture_plain_paragraph_merge(&viewport.rows, current.ordinal, &source_bytes)
            })
            .flatten();
        Some(DocumentSimpleEditContext {
            revision: viewport.revision,
            source_bytes,
            source_utf16,
            editable_bytes,
            editable_utf16,
            ending,
            row,
            paragraph_merge,
        })
    }

    /// Resolves one physical line inside a parser-certified multiline quote.
    ///
    /// The logical row exposes exact content segments separated by hidden
    /// quote prefixes. Semantic edit resolution still operates on one bounded
    /// physical line, so this derives that line only from the certified
    /// segment geometry and exact current source. No Markdown is reclassified
    /// here.
    fn capture_projected_block_quote_edit_context(
        &self,
        row: &DocumentViewportRow,
        selection_byte: usize,
        revision: u64,
    ) -> Option<DocumentSimpleEditContext> {
        let segments = row.projection_segments.as_ref()?;
        let (segment_index, segment) = segments.iter().enumerate().find(|(_, segment)| {
            let range = &segment.source_range;
            usize::try_from(range.start).is_ok_and(|start| selection_byte >= start)
                && usize::try_from(range.end).is_ok_and(|end| selection_byte <= end)
        })?;
        let segment_bytes = u64_range_to_usize(&segment.source_range)?;
        let physical_start = if segment_index == 0 {
            match row.presentation {
                DocumentViewportRowPresentation::BlockQuote {
                    prefix_start_byte, ..
                } => usize::try_from(prefix_start_byte).ok()?,
                _ => return None,
            }
        } else {
            usize::try_from(segments.get(segment_index - 1)?.source_range.end).ok()?
        };
        let physical_end = if segment_index + 1 < segments.len() {
            segment_bytes.end
        } else {
            usize::try_from(row.source_range.end).ok()?
        };
        if physical_start > segment_bytes.start || segment_bytes.end > physical_end {
            return None;
        }
        let observed_ending = self.edit_line_ending_at(physical_end);
        let ending = observed_ending.unwrap_or(self.fallback_line_ending);
        let editable_end = match observed_ending {
            Some(ending) => physical_end.checked_sub(ending.text().len())?,
            None => physical_end,
        }
        .min(segment_bytes.end);
        let editable_bytes = segment_bytes.start..editable_end;
        if selection_byte < editable_bytes.start || selection_byte > editable_bytes.end {
            return None;
        }

        let lease = self.runtime.snapshot_current_source().ok()?;
        let physical_start_utf16 = lease.utf16_offset_for_byte(physical_start).ok()?;
        let physical_end_utf16 = lease.utf16_offset_for_byte(physical_end).ok()?;
        let editable_start_utf16 = lease.utf16_offset_for_byte(editable_bytes.start).ok()?;
        let editable_end_utf16 = lease.utf16_offset_for_byte(editable_bytes.end).ok()?;
        let prefix_bytes = physical_start..segment_bytes.start;
        let prefix_utf16 = physical_start_utf16..editable_start_utf16;
        let prefix_text = String::from_utf8(self.source_bytes(prefix_bytes.clone()).ok()?).ok()?;

        Some(DocumentSimpleEditContext {
            revision,
            source_bytes: physical_start..physical_end,
            source_utf16: physical_start_utf16..physical_end_utf16,
            editable_bytes,
            editable_utf16: editable_start_utf16..editable_end_utf16,
            ending,
            row: DocumentSimpleEditRow::BlockQuote {
                prefix_bytes,
                prefix_utf16,
                prefix_text,
                starts_quote: segment_index == 0,
                empty: editable_start_utf16 == editable_end_utf16,
            },
            paragraph_merge: None,
        })
    }

    /// Resolves one physical line inside a parser-certified indented-code row.
    /// The projection segments are the visible code; every gap is the exact
    /// four-column prefix owned by the parser, never a Dart/Rust heuristic.
    fn capture_projected_indented_code_edit_context(
        &self,
        row: &DocumentViewportRow,
        selection_byte: usize,
        revision: u64,
    ) -> Option<DocumentSimpleEditContext> {
        let Some(segments) = row.projection_segments.as_ref() else {
            let source_bytes = u64_range_to_usize(&row.source_range)?;
            let source_utf16 = u64_range_to_usize(&row.source_utf16_range)?;
            let editable_bytes = u64_range_to_usize(row.editable_range.as_ref()?)?;
            let editable_utf16 = u64_range_to_usize(row.editable_utf16_range.as_ref()?)?;
            if selection_byte < editable_bytes.start || selection_byte > editable_bytes.end {
                return None;
            }
            let ending = self
                .edit_line_ending_at(source_bytes.end)
                .unwrap_or(self.fallback_line_ending);
            let (prefix_bytes, prefix_utf16) = self.indented_code_prefix_ranges(
                source_bytes.start..editable_bytes.start,
                source_utf16.start..editable_utf16.start,
            )?;
            let prefix_text = self.indented_code_continuation_prefix(&prefix_bytes)?;
            return Some(DocumentSimpleEditContext {
                revision,
                source_bytes,
                source_utf16,
                editable_bytes,
                editable_utf16,
                ending,
                row: DocumentSimpleEditRow::IndentedCode {
                    prefix_bytes,
                    prefix_utf16,
                    prefix_text,
                    join_bytes: None,
                    join_utf16: None,
                },
                paragraph_merge: None,
            });
        };
        let (segment_index, segment) = segments.iter().enumerate().find(|(_, segment)| {
            let range = &segment.source_range;
            usize::try_from(range.start).is_ok_and(|start| selection_byte >= start)
                && usize::try_from(range.end).is_ok_and(|end| selection_byte <= end)
        })?;
        let segment_bytes = u64_range_to_usize(&segment.source_range)?;
        let physical_start = if segment_index == 0 {
            usize::try_from(row.source_range.start).ok()?
        } else {
            usize::try_from(segments.get(segment_index - 1)?.source_range.end).ok()?
        };
        let physical_end = if segment_index + 1 < segments.len() {
            segment_bytes.end
        } else {
            usize::try_from(row.source_range.end).ok()?
        };
        if physical_start > segment_bytes.start || segment_bytes.end > physical_end {
            return None;
        }
        let observed_ending = self.edit_line_ending_at(physical_end);
        let ending = observed_ending.unwrap_or(self.fallback_line_ending);
        let editable_end = match observed_ending {
            Some(ending) => physical_end.checked_sub(ending.text().len())?,
            None => physical_end,
        }
        .min(segment_bytes.end);
        let editable_bytes = segment_bytes.start..editable_end;
        if selection_byte < editable_bytes.start || selection_byte > editable_bytes.end {
            return None;
        }

        let lease = self.runtime.snapshot_current_source().ok()?;
        let physical_start_utf16 = lease.utf16_offset_for_byte(physical_start).ok()?;
        let physical_end_utf16 = lease.utf16_offset_for_byte(physical_end).ok()?;
        let editable_start_utf16 = lease.utf16_offset_for_byte(editable_bytes.start).ok()?;
        let editable_end_utf16 = lease.utf16_offset_for_byte(editable_bytes.end).ok()?;
        let (prefix_bytes, prefix_utf16) = self.indented_code_prefix_ranges(
            physical_start..segment_bytes.start,
            physical_start_utf16..editable_start_utf16,
        )?;
        let prefix_text = self.indented_code_continuation_prefix(&prefix_bytes)?;

        let (join_bytes, join_utf16) = if segment_index == 0 {
            (None, None)
        } else {
            let previous_ending = self.edit_line_ending_at(physical_start)?;
            let join_start = physical_start.checked_sub(previous_ending.text().len())?;
            let join_start_utf16 = lease.utf16_offset_for_byte(join_start).ok()?;
            (
                Some(join_start..segment_bytes.start),
                Some(join_start_utf16..editable_start_utf16),
            )
        };

        Some(DocumentSimpleEditContext {
            revision,
            source_bytes: physical_start..physical_end,
            source_utf16: physical_start_utf16..physical_end_utf16,
            editable_bytes,
            editable_utf16: editable_start_utf16..editable_end_utf16,
            ending,
            row: DocumentSimpleEditRow::IndentedCode {
                prefix_bytes,
                prefix_utf16,
                prefix_text,
                join_bytes,
                join_utf16,
            },
            paragraph_merge: None,
        })
    }

    fn indented_code_continuation_prefix(&self, prefix: &Range<usize>) -> Option<String> {
        let prefix = String::from_utf8(self.source_bytes(prefix.clone()).ok()?).ok()?;
        (!prefix.is_empty() && !prefix.contains(['\r', '\n'])).then_some(prefix)
    }

    fn indented_code_prefix_ranges(
        &self,
        bytes: Range<usize>,
        utf16: Range<usize>,
    ) -> Option<(Range<usize>, Range<usize>)> {
        // The parser hides a BOF BOM with the first code prefix. It remains
        // document metadata, not repeatable indentation and not part of a
        // prefix-lift deletion.
        self.source_range_without_bof_bom(bytes, utf16)
    }

    fn source_range_without_bof_bom(
        &self,
        mut bytes: Range<usize>,
        mut utf16: Range<usize>,
    ) -> Option<(Range<usize>, Range<usize>)> {
        if bytes.start == 0
            && self.source_bytes(0..bytes.end.min(3)).ok()?.as_slice() == [0xef, 0xbb, 0xbf]
        {
            bytes.start = 3;
            utf16.start = utf16.start.checked_add(1)?;
        }
        (bytes.start < bytes.end && utf16.start < utf16.end).then_some((bytes, utf16))
    }

    fn capture_nested_list_outdent(
        &self,
        marker_start_byte: usize,
        marker_start_utf16: usize,
        nesting_depth: u8,
        marker_offset: u8,
        marker_column: u8,
        container_widths: u64,
        container_count: u8,
    ) -> Option<DocumentListOutdent> {
        let indentation =
            self.capture_list_marker_indentation(marker_start_byte, marker_start_utf16)?;
        let container_column = marker_column.checked_sub(marker_offset)?;
        if container_count != nesting_depth.checked_sub(1)? || container_count == 0 {
            return None;
        }
        let shift = u32::from(container_count - 1) * 4;
        let width = usize::try_from((container_widths >> shift) & 0x0f).ok()?;
        if width == 0
            || indentation.bytes.len() != usize::from(container_column)
            || indentation.utf16.len() != usize::from(container_column)
            || width > indentation.bytes.len()
        {
            return None;
        }
        Some(DocumentListOutdent {
            bytes: marker_start_byte.checked_sub(width)?..marker_start_byte,
            utf16: marker_start_utf16.checked_sub(width)?..marker_start_utf16,
            indentation: indentation.indentation,
        })
    }

    fn capture_list_marker_indentation(
        &self,
        marker_start_byte: usize,
        marker_start_utf16: usize,
    ) -> Option<DocumentListOutdent> {
        // One byte beyond the maximum published marker column preserves the
        // preceding line-boundary proof even at the contract ceiling.
        const MAX_OUTDENT_PREFIX_BYTES: usize = 256;
        let window_start = self
            .snapped_to_scalar_boundary(marker_start_byte.saturating_sub(MAX_OUTDENT_PREFIX_BYTES))
            .ok()?;
        let window = self.source_bytes(window_start..marker_start_byte).ok()?;
        let local_line_start = window
            .iter()
            .rposition(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(0, |index| index + 1);
        if local_line_start == 0 && window_start != 0 {
            return None;
        }
        let indentation = window.get(local_line_start..)?;
        if indentation.iter().any(|byte| !matches!(byte, b' ')) {
            return None;
        }
        let indentation_utf16 = std::str::from_utf8(indentation)
            .ok()?
            .encode_utf16()
            .count();
        Some(DocumentListOutdent {
            bytes: window_start + local_line_start..marker_start_byte,
            utf16: marker_start_utf16.checked_sub(indentation_utf16)?..marker_start_utf16,
            indentation: String::from_utf8(indentation.to_vec()).ok()?,
        })
    }

    fn certify_absent_empty_list(
        &self,
        context: &mut DocumentSimpleEditContext,
        rows: &[DocumentViewportRow],
    ) -> Option<()> {
        let DocumentSimpleEditRow::ListItem {
            prefix_bytes,
            prefix_utf16,
            nesting_depth,
            marker_offset,
            container_widths,
            container_count,
            marker_column,
            starts_list,
            outdent,
            ..
        } = &mut context.row
        else {
            return None;
        };
        let previous = rows.iter().rev().find_map(|row| match row.presentation {
            DocumentViewportRowPresentation::ListItem {
                prefix_start_byte,
                prefix_start_utf16,
                nesting_depth,
                simple_continuation: true,
                ..
            } if row.source_range.end as usize <= context.source_bytes.start => Some((
                usize::try_from(prefix_start_byte).ok()?,
                usize::try_from(prefix_start_utf16).ok()?,
                nesting_depth,
            )),
            _ => None,
        })?;
        let previous_indentation = self.capture_list_marker_indentation(previous.0, previous.1)?;
        let current_column = usize::from(*marker_offset);
        let previous_column = previous_indentation.bytes.len();
        let certified_depth = if current_column == previous_column {
            previous.2
        } else if current_column > previous_column && previous.2 == 1 {
            2
        } else {
            return None;
        };
        if certified_depth == 1 {
            return Some(());
        }
        if certified_depth != 2 || current_column != 2 {
            return None;
        }
        let indentation_end_byte = prefix_bytes.start.checked_add(current_column)?;
        let indentation_end_utf16 = prefix_utf16.start.checked_add(current_column)?;
        let indentation = self
            .source_bytes(prefix_bytes.start..indentation_end_byte)
            .ok()?;
        if indentation.iter().any(|byte| *byte != b' ') {
            return None;
        }
        *outdent = Some(DocumentListOutdent {
            bytes: prefix_bytes.start..indentation_end_byte,
            utf16: prefix_utf16.start..indentation_end_utf16,
            indentation: String::from_utf8(indentation).ok()?,
        });
        prefix_bytes.start = indentation_end_byte;
        prefix_utf16.start = indentation_end_utf16;
        *nesting_depth = 2;
        *marker_offset = 0;
        *container_widths = 2;
        *container_count = 1;
        *marker_column = 2;
        *starts_list = true;
        Some(())
    }

    /// Builds the same bounded edit facts directly from exact current source
    /// when no certified row has existed yet. Classification still belongs to
    /// `flark_parser`; this method only locates one physical line and converts
    /// its parser-authored facts into current document coordinates.
    fn capture_exact_edit_context(
        &self,
        selection_byte: usize,
    ) -> Option<DocumentSimpleEditContext> {
        if selection_byte > self.source_byte_len() {
            return None;
        }
        let window_start = self
            .snapped_to_scalar_boundary(
                selection_byte.saturating_sub(M11_SIMPLE_EDIT_LINE_MAX_BYTES),
            )
            .ok()?;
        let window_end = self
            .snapped_to_scalar_boundary(
                selection_byte
                    .saturating_add(M11_SIMPLE_EDIT_LINE_MAX_BYTES)
                    .min(self.source_byte_len()),
            )
            .ok()?;
        let window = self.source_bytes(window_start..window_end).ok()?;
        let local_selection = selection_byte - window_start;
        let local_line_start = line_start_in_window(&window, local_selection)?;
        if local_line_start == 0 && window_start != 0 {
            return None;
        }
        let local_line_end = line_end_in_window(&window, local_selection);
        let line_start = window_start + local_line_start;
        let line_end = window_start + local_line_end;
        let classified = classify_m11_simple_edit_line(
            &window[local_line_start..local_line_end],
            line_start == 0,
        );
        let lease = self.runtime.snapshot_current_source().ok()?;
        let source_utf16 = lease.utf16_offset_for_byte(line_start).ok()?
            ..lease.utf16_offset_for_byte(line_end).ok()?;
        let ending = match classified.ending {
            flark_parser::M11LineEnding::Lf => DocumentEditLineEnding::Lf,
            flark_parser::M11LineEnding::CrLf => DocumentEditLineEnding::CrLf,
            flark_parser::M11LineEnding::Cr => DocumentEditLineEnding::Cr,
            flark_parser::M11LineEnding::Eof => self.fallback_line_ending,
        };
        let (row, editable_bytes) = match classified.kind {
            M11SimpleEditLineKind::Plain => (
                DocumentSimpleEditRow::Plain,
                line_start..line_start + classified.content_end,
            ),
            M11SimpleEditLineKind::ListItem {
                marker,
                prefix,
                content,
                marker_offset,
                task_checked,
                empty,
            } => {
                let starts_list =
                    exact_simple_list_starts_list(&window, window_start, local_line_start, marker);
                let prefix_bytes = line_start + prefix.start..line_start + prefix.end;
                let prefix_utf16 = lease.utf16_offset_for_byte(prefix_bytes.start).ok()?
                    ..lease.utf16_offset_for_byte(prefix_bytes.end).ok()?;
                let editable = line_start + content.start..line_start + content.end;
                (
                    DocumentSimpleEditRow::ListItem {
                        marker: document_marker_from_parser(marker),
                        prefix_bytes,
                        prefix_utf16,
                        nesting_depth: 1,
                        marker_offset,
                        container_widths: 0,
                        container_count: 0,
                        marker_column: marker_offset,
                        starts_list,
                        task_checked,
                        empty,
                        outdent: None,
                    },
                    editable,
                )
            }
            M11SimpleEditLineKind::AtxHeading {
                prefix,
                content,
                empty,
            } => {
                let prefix_bytes = line_start + prefix.start..line_start + prefix.end;
                let prefix_utf16 = lease.utf16_offset_for_byte(prefix_bytes.start).ok()?
                    ..lease.utf16_offset_for_byte(prefix_bytes.end).ok()?;
                (
                    DocumentSimpleEditRow::AtxHeading {
                        prefix_bytes,
                        prefix_utf16,
                        empty,
                    },
                    line_start + content.start..line_start + content.end,
                )
            }
            M11SimpleEditLineKind::BlockQuote {
                prefix,
                content,
                empty,
            } => {
                if !exact_simple_block_quote_is_isolated(
                    &window,
                    window_start,
                    window_end,
                    self.source_byte_len(),
                    local_line_start,
                    local_line_end,
                ) {
                    return None;
                }
                let prefix_bytes = line_start + prefix.start..line_start + prefix.end;
                let prefix_utf16 = lease.utf16_offset_for_byte(prefix_bytes.start).ok()?
                    ..lease.utf16_offset_for_byte(prefix_bytes.end).ok()?;
                let prefix_text = String::from_utf8(
                    window[local_line_start + prefix.start..local_line_start + prefix.end].to_vec(),
                )
                .ok()?;
                (
                    DocumentSimpleEditRow::BlockQuote {
                        prefix_bytes,
                        prefix_utf16,
                        prefix_text,
                        starts_quote: true,
                        empty,
                    },
                    line_start + content.start..line_start + content.end,
                )
            }
            M11SimpleEditLineKind::Unsupported => return None,
        };
        if selection_byte < editable_bytes.start || selection_byte > editable_bytes.end {
            return None;
        }
        let editable_utf16 = lease.utf16_offset_for_byte(editable_bytes.start).ok()?
            ..lease.utf16_offset_for_byte(editable_bytes.end).ok()?;
        let paragraph_merge = matches!(row, DocumentSimpleEditRow::Plain)
            .then(|| exact_plain_paragraph_merge(&lease, &window, window_start, local_line_start))
            .flatten();
        Some(DocumentSimpleEditContext {
            revision: self.revision(),
            source_bytes: line_start..line_end,
            source_utf16,
            editable_bytes,
            editable_utf16,
            ending,
            row,
            paragraph_merge,
        })
    }

    fn capture_plain_paragraph_merge(
        &self,
        rows: &[DocumentViewportRow],
        current_ordinal: u64,
        current_source: &Range<usize>,
    ) -> Option<DocumentParagraphMerge> {
        let previous = rows
            .iter()
            .filter(|row| {
                row.ordinal < current_ordinal
                    && row.kind == 5
                    && row.presentation == DocumentViewportRowPresentation::Plain
                    && row.source_range.end <= current_source.start as u64
            })
            .max_by_key(|row| row.ordinal)?;
        let previous_source_bytes = u64_range_to_usize(&previous.source_range)?;
        let previous_source_utf16 = u64_range_to_usize(&previous.source_utf16_range)?;
        let previous_content_end = self
            .edit_line_ending_at(previous_source_bytes.end)
            .map_or(previous_source_bytes.end, |ending| {
                previous_source_bytes.end - ending.text().len()
            });
        if previous_content_end > current_source.start {
            return None;
        }
        let separator_bytes = previous_content_end..current_source.start;
        let separator = self.source_bytes(separator_bytes.clone()).ok()?;
        if !contains_exactly_two_line_endings(&separator) {
            return None;
        }
        let lease = self.runtime.snapshot_current_source().ok()?;
        let separator_utf16 = lease.utf16_offset_for_byte(separator_bytes.start).ok()?
            ..lease.utf16_offset_for_byte(separator_bytes.end).ok()?;
        Some(DocumentParagraphMerge {
            previous_source_bytes,
            previous_source_utf16,
            separator_bytes,
            separator_utf16,
        })
    }

    fn edit_line_ending_at(&self, physical_end: usize) -> Option<DocumentEditLineEnding> {
        if physical_end == 0 {
            return None;
        }
        match self
            .source_bytes(physical_end - 1..physical_end)
            .ok()?
            .as_slice()
        {
            b"\n" => {
                if physical_end >= 2
                    && self
                        .source_bytes(physical_end - 2..physical_end - 1)
                        .ok()
                        .as_deref()
                        == Some(b"\r")
                {
                    Some(DocumentEditLineEnding::CrLf)
                } else {
                    Some(DocumentEditLineEnding::Lf)
                }
            }
            b"\r" => Some(DocumentEditLineEnding::Cr),
            _ => None,
        }
    }

    fn transform_edit_context(
        &self,
        mut context: DocumentSimpleEditContext,
        base_bytes: Range<usize>,
        base_utf16: Range<usize>,
        replacement: &str,
        result_revision: u64,
    ) -> Option<DocumentSimpleEditContext> {
        if replacement.contains(['\r', '\n'])
            || base_bytes.start < context.editable_bytes.start
            || base_bytes.end > context.editable_bytes.end
            || base_utf16.start < context.editable_utf16.start
            || base_utf16.end > context.editable_utf16.end
        {
            return None;
        }
        let replacement_utf16 = replacement.encode_utf16().count();
        let byte_delta = replacement.len() as isize - base_bytes.len() as isize;
        let utf16_delta = replacement_utf16 as isize - base_utf16.len() as isize;
        context.revision = result_revision;
        context.source_bytes.end = add_signed(context.source_bytes.end, byte_delta)?;
        context.source_utf16.end = add_signed(context.source_utf16.end, utf16_delta)?;
        context.editable_bytes.end = add_signed(context.editable_bytes.end, byte_delta)?;
        context.editable_utf16.end = add_signed(context.editable_utf16.end, utf16_delta)?;
        match &mut context.row {
            DocumentSimpleEditRow::ListItem { empty, .. }
            | DocumentSimpleEditRow::AtxHeading { empty, .. }
            | DocumentSimpleEditRow::BlockQuote { empty, .. } => {
                *empty = context.editable_bytes.is_empty();
            }
            DocumentSimpleEditRow::Plain
            | DocumentSimpleEditRow::IndentedCode { .. }
            | DocumentSimpleEditRow::ThematicBreak { .. } => {}
        }
        self.validate_transformed_edit_context(&context)
            .then_some(context)
    }

    fn validate_transformed_edit_context(&self, context: &DocumentSimpleEditContext) -> bool {
        let line_start = match &context.row {
            DocumentSimpleEditRow::Plain => context.source_bytes.start,
            DocumentSimpleEditRow::ListItem { prefix_bytes, .. }
            | DocumentSimpleEditRow::AtxHeading { prefix_bytes, .. }
            | DocumentSimpleEditRow::BlockQuote { prefix_bytes, .. }
            | DocumentSimpleEditRow::IndentedCode { prefix_bytes, .. } => prefix_bytes.start,
            DocumentSimpleEditRow::ThematicBreak { atom_bytes, .. } => atom_bytes.start,
        };
        if line_start > context.source_bytes.end {
            return false;
        }
        let requested_end = context
            .source_bytes
            .end
            .min(line_start.saturating_add(M11_SIMPLE_EDIT_LINE_MAX_BYTES));
        let Ok(requested_end) = self.snapped_to_scalar_boundary(requested_end) else {
            return false;
        };
        let Ok(source) = self.source_bytes(line_start..requested_end) else {
            return false;
        };
        let classified = classify_m11_simple_edit_line(&source, line_start == 0);
        match (&context.row, classified.kind) {
            (DocumentSimpleEditRow::Plain, M11SimpleEditLineKind::Plain) => true,
            (
                DocumentSimpleEditRow::ListItem {
                    marker,
                    prefix_bytes,
                    marker_offset,
                    task_checked,
                    ..
                },
                M11SimpleEditLineKind::ListItem {
                    marker: classified_marker,
                    prefix,
                    marker_offset: classified_offset,
                    task_checked: classified_task_checked,
                    ..
                },
            ) => {
                document_marker_matches_parser(*marker, classified_marker)
                    && prefix.end == prefix_bytes.end.saturating_sub(line_start)
                    && classified_offset == *marker_offset
                    && classified_task_checked == *task_checked
            }
            (
                DocumentSimpleEditRow::AtxHeading { prefix_bytes, .. },
                M11SimpleEditLineKind::AtxHeading { prefix, .. },
            ) => prefix.end == prefix_bytes.end.saturating_sub(line_start),
            (
                DocumentSimpleEditRow::BlockQuote {
                    prefix_bytes,
                    prefix_text,
                    ..
                },
                M11SimpleEditLineKind::BlockQuote { prefix, .. },
            ) => {
                prefix.end == prefix_bytes.end.saturating_sub(line_start)
                    && source
                        .get(prefix)
                        .is_some_and(|bytes| bytes == prefix_text.as_bytes())
            }
            (
                DocumentSimpleEditRow::IndentedCode {
                    prefix_bytes,
                    prefix_text,
                    ..
                },
                _,
            ) => {
                let relative_end = prefix_bytes.end.saturating_sub(line_start);
                source.get(..relative_end).is_some_and(|bytes| {
                    bytes == prefix_text.as_bytes()
                        || String::from_utf8_lossy(bytes)
                            .strip_prefix('\u{feff}')
                            .is_some_and(|without_bom| {
                                without_bom.as_bytes() == prefix_text.as_bytes()
                            })
                })
            }
            (DocumentSimpleEditRow::ThematicBreak { atom_bytes, .. }, _) => {
                atom_bytes.start >= line_start && atom_bytes.end <= context.source_bytes.end
            }
            _ => false,
        }
    }

    fn apply_edit_to_ready_base(
        &mut self,
        base: Box<M11PersistentRecursiveGreenSession>,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<DocumentEditReceipt, DocumentSessionError> {
        let expected = base.source();
        let edit = match self
            .runtime
            .apply_edit(expected, range.clone(), replacement)
        {
            Ok(edit) => edit,
            Err(error) => {
                self.parser = ParseState::Ready(base);
                return Err(error.into());
            }
        };
        let revision = edit
            .source()
            .current()
            .revision()
            .get()
            .checked_add(1)
            .ok_or(DocumentSessionError::Faulted)?;
        let target = self.runtime.snapshot_current_source()?;
        self.last_edit_work = M11PersistentRecursiveGreenAdoptionWork::default();
        self.parser = match (*base).begin_local_adoption(&self.runtime, target, range) {
            Ok(adoption) => ParseState::Adopting(Box::new(adoption)),
            Err(failure) => {
                let mut base = failure.into_base();
                base.begin_release(&mut self.runtime)?;
                ParseState::ReleasingBaseForClean(Box::new(base))
            }
        };
        Ok(DocumentEditReceipt {
            revision,
            parser_pending: true,
        })
    }

    fn apply_edit_while_building(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<DocumentEditReceipt, DocumentSessionError> {
        let expected = self
            .runtime
            .current_source_version()
            .ok_or(DocumentSessionError::Faulted)?;
        let edit = self.runtime.apply_edit(expected, range, replacement)?;
        let revision = edit
            .source()
            .current()
            .revision()
            .get()
            .checked_add(1)
            .ok_or(DocumentSessionError::Faulted)?;
        self.last_edit_work = M11PersistentRecursiveGreenAdoptionWork::default();
        Ok(DocumentEditReceipt {
            revision,
            parser_pending: true,
        })
    }

    pub fn source_bytes(&self, range: Range<usize>) -> Result<Vec<u8>, DocumentSessionError> {
        if range.start > range.end || range.end > self.source_byte_len() {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let lease = self.runtime.snapshot_current_source()?;
        let mut cursor = lease.cursor_in(range.clone())?;
        let mut bytes = vec![0_u8; range.end - range.start];
        let mut written = 0;
        while written < bytes.len() {
            let count = cursor.read(&mut bytes[written..]);
            if count == 0 {
                return Err(DocumentSessionError::RangeOutOfBounds);
            }
            written += count;
        }
        let _lease = cursor.finish()?;
        Ok(bytes)
    }

    pub fn byte_offset_for_utf16(&self, offset: usize) -> Result<usize, DocumentSessionError> {
        Ok(self
            .runtime
            .snapshot_current_source()?
            .byte_offset_for_utf16(offset)?)
    }

    pub fn utf16_offset_for_byte(&self, offset: usize) -> Result<usize, DocumentSessionError> {
        Ok(self
            .runtime
            .snapshot_current_source()?
            .utf16_offset_for_byte(offset)?)
    }

    /// Moves `offset` back to the nearest UTF-8 scalar boundary.
    ///
    /// A viewport request is a coverage hint expressed in bytes, and a host
    /// that caps it against a byte budget cannot know where scalars end —
    /// that knowledge is the runtime's. Snapping can only cover slightly
    /// less, never produce a wrong result, so it is preferred to rejecting a
    /// request whose only fault is landing inside a multi-byte scalar.
    pub fn snapped_to_scalar_boundary(&self, offset: usize) -> Result<usize, DocumentSessionError> {
        let length = self.source_byte_len();
        if offset == 0 || offset >= length {
            return Ok(offset.min(length));
        }
        // Coordinate conversion is itself the boundary test: it rejects an
        // offset inside a scalar. A scalar is at most four bytes, so at most
        // four probes are needed, and reading raw bytes would not work here
        // because a byte range must itself be boundary-aligned.
        let lease = self.runtime.snapshot_current_source()?;
        let floor = offset.saturating_sub(3);
        let mut candidate = offset;
        loop {
            if lease.utf16_offset_for_byte(candidate).is_ok() {
                return Ok(candidate);
            }
            if candidate == floor {
                return Ok(floor);
            }
            candidate -= 1;
        }
    }

    pub fn query_viewport(
        &mut self,
        revision: u64,
        requested_range: Range<usize>,
        maximum_rows: u32,
    ) -> Result<DocumentViewport, DocumentSessionError> {
        let actual_revision = self.revision();
        if revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: revision,
                actual: actual_revision,
            });
        }
        if requested_range.start > requested_range.end
            || requested_range.end > self.source_byte_len()
            || maximum_rows == 0
        {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let requested_range = self.snapped_to_scalar_boundary(requested_range.start)?
            ..self.snapped_to_scalar_boundary(requested_range.end)?;
        let session = match &self.parser {
            ParseState::Ready(session) => session,
            ParseState::Faulted => return Err(DocumentSessionError::Faulted),
            _ => return Err(DocumentSessionError::NotReady),
        };
        query_session_viewport(
            &mut self.runtime,
            session,
            revision,
            requested_range,
            maximum_rows,
        )
    }

    /// Returns a current-revision live projection whose pending spans contain
    /// no semantic facts. During an incremental adoption, only ranges backed
    /// by parser-authenticated restart/convergence authority are queried from
    /// the retained base; every other byte remains explicitly pending.
    pub fn query_live_viewport(
        &self,
        revision: u64,
        requested_range: Range<usize>,
        maximum_spans: u32,
    ) -> Result<DocumentLiveViewport, DocumentSessionError> {
        let actual_revision = self.revision();
        if revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: revision,
                actual: actual_revision,
            });
        }
        if requested_range.start > requested_range.end
            || requested_range.end > self.source_byte_len()
            || maximum_spans == 0
        {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let requested_range = self.snapped_to_scalar_boundary(requested_range.start)?
            ..self.snapped_to_scalar_boundary(requested_range.end)?;
        match &self.parser {
            ParseState::Ready(_) => certified_range_live_viewport(
                &self.runtime,
                revision,
                requested_range,
                maximum_spans,
            ),
            ParseState::Adopting(adoption) => query_adopting_live_viewport(
                &self.runtime,
                adoption,
                revision,
                requested_range,
                maximum_spans,
            ),
            ParseState::Faulted => Err(DocumentSessionError::Faulted),
            _ => pending_live_viewport(&self.runtime, revision, requested_range, maximum_spans),
        }
    }

    /// Makes the document terminally non-writable and starts bounded release.
    pub fn begin_close(&mut self) -> Result<(), DocumentSessionError> {
        let state = mem::replace(&mut self.parser, ParseState::Transition);
        self.parser = match state {
            ParseState::Clean(mut build) => {
                if let Err(error) = build.begin_cancel(&mut self.runtime) {
                    self.parser = ParseState::Clean(build);
                    return Err(error.into());
                }
                ParseState::ClosingClean(build)
            }
            ParseState::CancellingClean(build) => ParseState::ClosingClean(build),
            ParseState::Ready(mut session) => {
                if let Err(error) = session.begin_release(&mut self.runtime) {
                    self.parser = ParseState::Ready(session);
                    return Err(error.into());
                }
                ParseState::ClosingSession {
                    current: session,
                    next: None,
                }
            }
            ParseState::Adopting(mut adoption) => {
                if let Err(error) = adoption.begin_cancel(&mut self.runtime) {
                    self.parser = ParseState::Adopting(adoption);
                    return Err(error.into());
                }
                ParseState::ClosingAdoption(adoption)
            }
            ParseState::CancellingAdoption(adoption) => ParseState::ClosingAdoption(adoption),
            ParseState::ReleasingBaseForTarget { base, target } => ParseState::ClosingSession {
                current: base,
                next: Some(target),
            },
            ParseState::ReleasingBaseForClean(base) => ParseState::ClosingSession {
                current: base,
                next: None,
            },
            ParseState::Faulted => {
                self.runtime.begin_close()?;
                ParseState::ClosingRuntime
            }
            closing @ (ParseState::ClosingClean(_)
            | ParseState::ClosingAdoption(_)
            | ParseState::ClosingSession { .. }
            | ParseState::ClosingRuntime
            | ParseState::Closed) => {
                self.parser = closing;
                return Err(DocumentSessionError::Busy);
            }
            ParseState::Transition => {
                self.parser = ParseState::Faulted;
                return Err(DocumentSessionError::Faulted);
            }
        };
        Ok(())
    }

    /// Releases at most `max_work_units` units of parser/source state.
    pub fn pump_close(
        &mut self,
        max_work_units: usize,
    ) -> Result<DocumentCloseReceipt, DocumentSessionError> {
        if max_work_units == 0 {
            return Err(DocumentSessionError::ZeroWorkBudget);
        }
        if !matches!(
            self.parser,
            ParseState::ClosingClean(_)
                | ParseState::ClosingAdoption(_)
                | ParseState::ClosingSession { .. }
                | ParseState::ClosingRuntime
                | ParseState::Closed
        ) {
            return Err(DocumentSessionError::Busy);
        }

        let mut consumed = 0;
        while consumed < max_work_units && !matches!(self.parser, ParseState::Closed) {
            let state = mem::replace(&mut self.parser, ParseState::Transition);
            let next = self.advance_close_one(state);
            match next {
                Ok(state) => self.parser = state,
                Err(error) => {
                    self.parser = ParseState::Faulted;
                    return Err(error);
                }
            }
            consumed += 1;
        }
        Ok(DocumentCloseReceipt {
            work_units: consumed,
            complete: matches!(self.parser, ParseState::Closed),
        })
    }

    fn advance_close_one(&mut self, state: ParseState) -> Result<ParseState, DocumentSessionError> {
        match state {
            ParseState::ClosingClean(mut build) => {
                let poll = build.poll_cancel(&mut self.runtime, 1)?;
                if poll.status() == M11PersistentRecursiveGreenBuildStatus::Cancelled {
                    drop(build);
                    self.runtime.begin_close()?;
                    Ok(ParseState::ClosingRuntime)
                } else {
                    Ok(ParseState::ClosingClean(build))
                }
            }
            ParseState::ClosingAdoption(mut adoption) => {
                if adoption.poll_cancel(&mut self.runtime, 1)? {
                    let mut base = adoption.take_base_after_cancel().ok_or(
                        M11PersistentRecursiveGreenSessionError::InvalidState(
                            "cancelled close adoption omitted its base",
                        ),
                    )?;
                    base.begin_release(&mut self.runtime)?;
                    Ok(ParseState::ClosingSession {
                        current: Box::new(base),
                        next: None,
                    })
                } else {
                    Ok(ParseState::ClosingAdoption(adoption))
                }
            }
            ParseState::ClosingSession {
                mut current,
                mut next,
            } => {
                if current.poll_release(&mut self.runtime, 1)? {
                    drop(current);
                    if let Some(mut session) = next.take() {
                        session.begin_release(&mut self.runtime)?;
                        Ok(ParseState::ClosingSession {
                            current: session,
                            next: None,
                        })
                    } else {
                        self.runtime.begin_close()?;
                        Ok(ParseState::ClosingRuntime)
                    }
                } else {
                    Ok(ParseState::ClosingSession { current, next })
                }
            }
            ParseState::ClosingRuntime => {
                if self.runtime.poll_close(1)?.complete {
                    Ok(ParseState::Closed)
                } else {
                    Ok(ParseState::ClosingRuntime)
                }
            }
            ParseState::Closed => Ok(ParseState::Closed),
            _ => Err(DocumentSessionError::Busy),
        }
    }

    /// Consuming convenience for native hosts outside a foreground thread.
    pub fn close(mut self) -> Result<(), DocumentSessionError> {
        self.begin_close()?;
        while !self.pump_close(256)?.complete {}
        Ok(())
    }
}

fn dominant_edit_line_ending(source: &[u8]) -> DocumentEditLineEnding {
    let mut lf = 0_u64;
    let mut crlf = 0_u64;
    let mut cr = 0_u64;
    let mut index = 0;
    while index < source.len() {
        match source[index] {
            b'\r' if source.get(index + 1) == Some(&b'\n') => {
                crlf += 1;
                index += 2;
            }
            b'\r' => {
                cr += 1;
                index += 1;
            }
            b'\n' => {
                lf += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    if crlf > lf && crlf >= cr {
        DocumentEditLineEnding::CrLf
    } else if cr > lf && cr > crlf {
        DocumentEditLineEnding::Cr
    } else {
        DocumentEditLineEnding::Lf
    }
}

fn contains_exactly_two_line_endings(source: &[u8]) -> bool {
    let mut endings = 0;
    let mut index = 0;
    while index < source.len() {
        match source[index] {
            b'\r' if source.get(index + 1) == Some(&b'\n') => {
                endings += 1;
                index += 2;
            }
            b'\r' | b'\n' => {
                endings += 1;
                index += 1;
            }
            _ => return false,
        }
    }
    endings == 2
}

fn line_start_in_window(source: &[u8], selection: usize) -> Option<usize> {
    if selection > source.len()
        || (selection > 0
            && selection < source.len()
            && source[selection - 1] == b'\r'
            && source[selection] == b'\n')
    {
        return None;
    }
    Some(
        source[..selection]
            .iter()
            .rposition(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(0, |index| index + 1),
    )
}

fn line_end_in_window(source: &[u8], selection: usize) -> usize {
    let Some(relative) = source[selection..]
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
    else {
        return source.len();
    };
    let ending = selection + relative;
    if source[ending] == b'\r' && source.get(ending + 1) == Some(&b'\n') {
        ending + 2
    } else {
        ending + 1
    }
}

fn previous_line_ending_start(source: &[u8], end: usize) -> Option<usize> {
    match source.get(end.checked_sub(1)?)? {
        b'\n' if end >= 2 && source[end - 2] == b'\r' => Some(end - 2),
        b'\n' | b'\r' => Some(end - 1),
        _ => None,
    }
}

fn transformed_collapsed_selection(
    offset: usize,
    base: &Range<usize>,
    replacement_len: usize,
) -> Option<usize> {
    if offset < base.start {
        return Some(offset);
    }
    if offset <= base.end {
        return base.start.checked_add(replacement_len);
    }
    offset.checked_sub(base.len())?.checked_add(replacement_len)
}

fn exact_simple_block_quote_is_isolated(
    window: &[u8],
    window_start: usize,
    window_end: usize,
    source_len: usize,
    line_start: usize,
    line_end: usize,
) -> bool {
    let separated_before = if line_start == 0 {
        window_start == 0
    } else {
        let Some(previous_ending) = previous_line_ending_start(window, line_start) else {
            return false;
        };
        let previous_start = window[..previous_ending]
            .iter()
            .rposition(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(0, |index| index + 1);
        (previous_start != 0 || window_start == 0)
            && physical_line_is_blank(&window[previous_start..line_start])
    };
    if !separated_before {
        return false;
    }
    if line_end == window.len() {
        return window_end == source_len;
    }
    let next_end = line_end_in_window(window, line_end);
    physical_line_is_blank(&window[line_end..next_end])
}

fn physical_line_is_blank(source: &[u8]) -> bool {
    source
        .iter()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

fn exact_plain_paragraph_merge(
    lease: &SourceSnapshotLease,
    window: &[u8],
    window_start: usize,
    current_line_start: usize,
) -> Option<DocumentParagraphMerge> {
    let last_ending = previous_line_ending_start(window, current_line_start)?;
    let previous_ending = previous_line_ending_start(window, last_ending)?;
    let previous_line_start = window[..previous_ending]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |index| index + 1);
    if previous_line_start == 0 && window_start != 0 {
        return None;
    }
    let previous_source = &window[previous_line_start..last_ending];
    if !matches!(
        classify_m11_simple_edit_line(previous_source, window_start + previous_line_start == 0)
            .kind,
        M11SimpleEditLineKind::Plain
    ) {
        return None;
    }
    let previous_source_bytes = window_start + previous_line_start..window_start + last_ending;
    let separator_bytes = window_start + previous_ending..window_start + current_line_start;
    let previous_source_utf16 = lease
        .utf16_offset_for_byte(previous_source_bytes.start)
        .ok()?
        ..lease
            .utf16_offset_for_byte(previous_source_bytes.end)
            .ok()?;
    let separator_utf16 = lease.utf16_offset_for_byte(separator_bytes.start).ok()?
        ..lease.utf16_offset_for_byte(separator_bytes.end).ok()?;
    Some(DocumentParagraphMerge {
        previous_source_bytes,
        previous_source_utf16,
        separator_bytes,
        separator_utf16,
    })
}

fn exact_simple_list_starts_list(
    window: &[u8],
    window_start: usize,
    current_line_start: usize,
    current_marker: M11SimpleEditListMarker,
) -> bool {
    let Some(previous_ending_start) = previous_line_ending_start(window, current_line_start) else {
        return true;
    };
    let previous_line_start = window[..previous_ending_start]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |index| index + 1);
    if previous_line_start == 0 && window_start != 0 {
        return true;
    }
    let previous = classify_m11_simple_edit_line(
        &window[previous_line_start..current_line_start],
        window_start + previous_line_start == 0,
    );
    let M11SimpleEditLineKind::ListItem {
        marker: previous_marker,
        ..
    } = previous.kind
    else {
        return true;
    };
    !simple_list_markers_are_compatible(previous_marker, current_marker)
}

fn simple_list_markers_are_compatible(
    left: M11SimpleEditListMarker,
    right: M11SimpleEditListMarker,
) -> bool {
    match (left, right) {
        (M11SimpleEditListMarker::Bullet(left), M11SimpleEditListMarker::Bullet(right)) => {
            left == right
        }
        (
            M11SimpleEditListMarker::Ordered {
                delimiter: left, ..
            },
            M11SimpleEditListMarker::Ordered {
                delimiter: right, ..
            },
        ) => left == right,
        _ => false,
    }
}

fn u64_range_to_usize(range: &Range<u64>) -> Option<Range<usize>> {
    Some(usize::try_from(range.start).ok()?..usize::try_from(range.end).ok()?)
}

fn add_signed(value: usize, delta: isize) -> Option<usize> {
    if delta >= 0 {
        value.checked_add(delta as usize)
    } else {
        value.checked_sub(delta.unsigned_abs())
    }
}

fn document_marker_matches_parser(
    document: DocumentListMarker,
    parser: M11SimpleEditListMarker,
) -> bool {
    matches!(
        (document, parser),
        (
            DocumentListMarker::Bullet(DocumentBulletMarker::Hyphen),
            M11SimpleEditListMarker::Bullet(BulletMarker::Hyphen)
        ) | (
            DocumentListMarker::Bullet(DocumentBulletMarker::Plus),
            M11SimpleEditListMarker::Bullet(BulletMarker::Plus)
        ) | (
            DocumentListMarker::Bullet(DocumentBulletMarker::Asterisk),
            M11SimpleEditListMarker::Bullet(BulletMarker::Asterisk)
        ) | (
            DocumentListMarker::Ordered {
                value: _,
                delimiter: DocumentListDelimiter::Period,
            },
            M11SimpleEditListMarker::Ordered {
                value: _,
                delimiter: ListDelimiter::Period,
            }
        ) | (
            DocumentListMarker::Ordered {
                value: _,
                delimiter: DocumentListDelimiter::Parenthesis,
            },
            M11SimpleEditListMarker::Ordered {
                value: _,
                delimiter: ListDelimiter::Parenthesis,
            }
        )
    ) && match (document, parser) {
        (
            DocumentListMarker::Ordered { value: left, .. },
            M11SimpleEditListMarker::Ordered { value: right, .. },
        ) => left == right,
        _ => true,
    }
}

fn document_marker_from_parser(marker: M11SimpleEditListMarker) -> DocumentListMarker {
    match marker {
        M11SimpleEditListMarker::Bullet(BulletMarker::Hyphen) => {
            DocumentListMarker::Bullet(DocumentBulletMarker::Hyphen)
        }
        M11SimpleEditListMarker::Bullet(BulletMarker::Plus) => {
            DocumentListMarker::Bullet(DocumentBulletMarker::Plus)
        }
        M11SimpleEditListMarker::Bullet(BulletMarker::Asterisk) => {
            DocumentListMarker::Bullet(DocumentBulletMarker::Asterisk)
        }
        M11SimpleEditListMarker::Ordered {
            value,
            delimiter: ListDelimiter::Period,
        } => DocumentListMarker::Ordered {
            value,
            delimiter: DocumentListDelimiter::Period,
        },
        M11SimpleEditListMarker::Ordered {
            value,
            delimiter: ListDelimiter::Parenthesis,
        } => DocumentListMarker::Ordered {
            value,
            delimiter: DocumentListDelimiter::Parenthesis,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gfm_table_projection_reaches_the_viewport_as_typed_cells() {
        let source = "| f\\|oo | bar |\n| :--- | ---: |\n| `x\\|y` | baz |\n";
        let mut document = DocumentSession::begin(source).expect("begin table document");
        while document.pump(512).expect("pump table").phase != DocumentSessionPhase::Ready {}
        let viewport = document
            .query_viewport(document.revision(), 0..source.len(), 8)
            .expect("query table viewport");
        let row = viewport.rows.first().expect("table row");
        assert_eq!(row.presentation, DocumentViewportRowPresentation::Table);
        let facts = row.inline_facts.as_ref().expect("table inline facts");
        let cells = facts
            .iter()
            .filter(|fact| fact.kind == DocumentInlineFactKind::TableCell)
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), 4);
        assert_eq!(
            cells[0].flags,
            1 | DOCUMENT_TABLE_CELL_HEADER | DOCUMENT_TABLE_CELL_ROW_START
        );
        assert_eq!(cells[1].flags, 3 | DOCUMENT_TABLE_CELL_HEADER);
        assert_eq!(cells[2].flags, 1 | DOCUMENT_TABLE_CELL_ROW_START);
        assert_eq!(cells[3].flags, 3);
        assert_eq!(
            facts
                .iter()
                .filter(|fact| fact.kind == DocumentInlineFactKind::Replacement)
                .count(),
            2
        );
        document.close().expect("close table document");
    }

    #[test]
    fn dense_capacity_fault_has_a_fast_bounded_reclamation_probe() {
        const PAYLOAD_BUDGET: usize = 1024 * 1024;
        let source = "x.\n\n".repeat(32 * 1024);
        let config = DocumentRuntimeConfig {
            arena_limits: flark_engine::ArenaLimits {
                max_live_payload_bytes: PAYLOAD_BUDGET,
                ..flark_engine::ArenaLimits::default()
            },
            ..DocumentRuntimeConfig::default()
        };
        let mut document =
            DocumentSession::begin_with_config(&source, config).expect("begin dense probe");
        let error = loop {
            match document.pump(512) {
                Ok(receipt) if receipt.phase == DocumentSessionPhase::Ready => {
                    panic!("dense probe unexpectedly fit its reduced payload budget")
                }
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert!(
            matches!(error, DocumentSessionError::Parser(_)),
            "expected parser capacity fault, got {error:?}"
        );
        let metrics = document
            .fault_arena_metrics()
            .expect("pre-cleanup fault metrics");
        eprintln!("dense fast probe: {error:?}; arena={metrics:?}");
        assert!(
            metrics.live_payload_bytes + metrics.reserved_external_payload_bytes <= PAYLOAD_BUDGET
        );
        assert!(metrics.live_payload_bytes > PAYLOAD_BUDGET / 2);

        document.begin_close().expect("begin faulted close");
        while !document
            .pump_close(256)
            .expect("pump faulted close")
            .complete
        {}
        assert_eq!(document.phase(), DocumentSessionPhase::Closed);
        assert_eq!(document.source_byte_len(), 0);
    }
}

fn row_query_limits(maximum_rows: u32) -> Option<M11RecursiveGreenRowQueryLimits> {
    let maximum_rows_u64 = u64::from(maximum_rows);
    M11RecursiveGreenRowQueryLimits::new(
        maximum_rows,
        maximum_rows_u64.saturating_mul(16).saturating_add(64),
        maximum_rows_u64.saturating_mul(512).saturating_add(4096),
        QUERY_OPEN_DEPTH_LIMIT,
        maximum_rows_u64.saturating_mul(512).saturating_add(4096),
    )
}

fn query_session_viewport(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    revision: u64,
    requested_range: Range<usize>,
    maximum_rows: u32,
) -> Result<DocumentViewport, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let utf16 = lease.utf16_offset_for_byte(requested_range.start)?;
    let limits = row_query_limits(maximum_rows).ok_or(DocumentSessionError::RangeOutOfBounds)?;
    let outcome = session.query_renderable_rows_bounded(
        runtime,
        M11RecursiveGreenPoint::new(requested_range.start, utf16, SourceBoundaryAffinity::After),
        requested_range.end as u64,
        limits,
    )?;
    let window = match outcome {
        M11RecursiveGreenRowQueryOutcome::Window(window) => window,
        M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
            return Err(DocumentSessionError::QueryBudgetExceeded)
        }
    };
    let receipt = window.receipt();
    let rows = window
        .rows()
        .iter()
        .map(|row| document_viewport_row(runtime, session, row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DocumentViewport {
        revision,
        requested_range: requested_range.start as u64..requested_range.end as u64,
        start_ordinal: window.start_ordinal(),
        total_rows: window.total_rows(),
        complete: window.complete(),
        rows,
        receipt: DocumentQueryReceipt {
            storage_pages_visited: receipt.storage_pages_visited(),
            events_scanned: receipt.events_scanned(),
            tree_nodes_visited: receipt.node_headers_decoded(),
            maximum_open_depth: receipt.maximum_open_depth(),
        },
    })
}

fn document_viewport_row(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<DocumentViewportRow, DocumentSessionError> {
    let mut presentation = match m11_recursive_green_row_presentation(runtime, row)
        .map_err(M11PersistentRecursiveGreenSessionError::from)?
    {
        M11RecursiveGreenRowPresentation::Plain => DocumentViewportRowPresentation::Plain,
        M11RecursiveGreenRowPresentation::Heading { level, style } => {
            DocumentViewportRowPresentation::Heading {
                level,
                style: match style {
                    HeadingStyle::Atx => DocumentHeadingStyle::Atx,
                    HeadingStyle::Setext => DocumentHeadingStyle::Setext,
                },
            }
        }
        M11RecursiveGreenRowPresentation::ListItem {
            marker,
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            marker_offset,
            container_widths,
            container_count,
            marker_column,
            simple_continuation,
            starts_list,
            task_checked,
        } => DocumentViewportRowPresentation::ListItem {
            marker: match marker {
                M11RecursiveGreenListMarker::Bullet(marker) => {
                    DocumentListMarker::Bullet(match marker {
                        BulletMarker::Hyphen => DocumentBulletMarker::Hyphen,
                        BulletMarker::Plus => DocumentBulletMarker::Plus,
                        BulletMarker::Asterisk => DocumentBulletMarker::Asterisk,
                    })
                }
                M11RecursiveGreenListMarker::Ordered { value, delimiter } => {
                    DocumentListMarker::Ordered {
                        value,
                        delimiter: match delimiter {
                            ListDelimiter::Period => DocumentListDelimiter::Period,
                            ListDelimiter::Parenthesis => DocumentListDelimiter::Parenthesis,
                        },
                    }
                }
            },
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            marker_offset,
            container_widths,
            container_count,
            marker_column,
            simple_continuation,
            starts_list,
            task_checked,
        },
        M11RecursiveGreenRowPresentation::BlockQuote {
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            simple_continuation,
        } => DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            simple_continuation,
        },
        M11RecursiveGreenRowPresentation::CodeBlock { style } => {
            DocumentViewportRowPresentation::CodeBlock {
                style: match style {
                    M11RecursiveGreenCodeBlockStyle::Indented => DocumentCodeBlockStyle::Indented,
                    M11RecursiveGreenCodeBlockStyle::Fenced {
                        fence,
                        minimum_closing_length,
                        fence_offset,
                        closed,
                    } => DocumentCodeBlockStyle::Fenced {
                        fence: match fence {
                            FenceCharacter::Backtick => DocumentFenceCharacter::Backtick,
                            FenceCharacter::Tilde => DocumentFenceCharacter::Tilde,
                        },
                        minimum_closing_length,
                        fence_offset,
                        closed,
                    },
                },
            }
        }
        M11RecursiveGreenRowPresentation::ThematicBreak => {
            DocumentViewportRowPresentation::ThematicBreak
        }
    };
    let source_range = row.physical_range();
    let source_utf16_range = row.physical_utf16_range();
    let mut editable_range = row.editable_range();
    let mut editable_utf16_range = row.editable_utf16_range();
    if matches!(
        presentation,
        DocumentViewportRowPresentation::Heading {
            style: DocumentHeadingStyle::Atx,
            ..
        }
    ) && editable_range.as_ref().is_some_and(Range::is_empty)
    {
        if let Some((exact_bytes, exact_utf16)) = certified_empty_atx_heading_editable(runtime, row)
        {
            editable_range = Some(exact_bytes);
            editable_utf16_range = Some(exact_utf16);
        }
    }
    let empty_container_prefix_end = match presentation {
        DocumentViewportRowPresentation::ListItem {
            prefix_end_byte,
            prefix_end_utf16,
            ..
        }
        | DocumentViewportRowPresentation::BlockQuote {
            prefix_end_byte,
            prefix_end_utf16,
            ..
        } => Some((prefix_end_byte, prefix_end_utf16)),
        _ => None,
    };
    if let Some((prefix_end_byte, prefix_end_utf16)) = empty_container_prefix_end {
        if prefix_end_byte < source_range.start && prefix_end_utf16 < source_utf16_range.start {
            editable_range = Some(prefix_end_byte..prefix_end_byte);
            editable_utf16_range = Some(prefix_end_utf16..prefix_end_utf16);
        }
    }
    let inline_facts = document_inline_facts(runtime, session, row)?;
    if presentation == DocumentViewportRowPresentation::Plain
        && inline_facts.as_ref().is_some_and(|facts| {
            facts
                .iter()
                .any(|fact| fact.kind == DocumentInlineFactKind::TableCell)
        })
    {
        presentation = DocumentViewportRowPresentation::Table;
    }
    let edit_capability = match row.edit_capability() {
        M11RecursiveGreenRowEditCapability::ProjectedReserved
            if !matches!(
                presentation,
                DocumentViewportRowPresentation::BlockQuote {
                    nesting_depth: 1,
                    ..
                } | DocumentViewportRowPresentation::CodeBlock {
                    style: DocumentCodeBlockStyle::Indented,
                }
            ) =>
        {
            editable_range = None;
            editable_utf16_range = None;
            DocumentViewportRowEditCapability::Unavailable
        }
        capability => document_edit_capability(capability),
    };
    let projection_segments =
        (edit_capability == DocumentViewportRowEditCapability::ProjectedReserved).then(|| {
            row.editable_segments()
                .iter()
                .map(|segment| DocumentProjectionSegment {
                    source_range: segment.byte_range(),
                    source_utf16_range: segment.utf16_range(),
                })
                .collect::<Vec<_>>()
        });
    let continuity_policy = if matches!(
        edit_capability,
        DocumentViewportRowEditCapability::Contiguous
            | DocumentViewportRowEditCapability::ProjectedReserved
    ) && editable_utf16_range
        .as_ref()
        .is_some_and(|range| range.start < range.end)
        && !matches!(
            presentation,
            DocumentViewportRowPresentation::ThematicBreak | DocumentViewportRowPresentation::Table
        ) {
        DocumentViewportRowContinuityPolicy::PlainTextEdit
    } else {
        DocumentViewportRowContinuityPolicy::None
    };
    Ok(DocumentViewportRow {
        ordinal: row.ordinal(),
        kind: row.kind().get(),
        source_range,
        source_utf16_range,
        editable_range,
        editable_utf16_range,
        edit_capability,
        continuity_policy,
        presentation,
        inline_facts,
        projection_segments,
        path_depth: u32::try_from(row.path().len()).unwrap_or(u32::MAX),
    })
}

fn certified_empty_atx_heading_editable(
    runtime: &DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
) -> Option<(Range<u64>, Range<u64>)> {
    let frame = row.path().last()?;
    let frame_bytes = frame.physical_range();
    let frame_utf16 = frame.physical_utf16_range();
    let length = usize::try_from(frame_bytes.end.checked_sub(frame_bytes.start)?).ok()?;
    if length > M11_SIMPLE_EDIT_LINE_MAX_BYTES {
        return None;
    }
    let start = usize::try_from(frame_bytes.start).ok()?;
    let end = usize::try_from(frame_bytes.end).ok()?;
    let mut source = vec![0_u8; length];
    if runtime
        .read_current_source_window(start..end, &mut source)
        .ok()?
        != length
    {
        return None;
    }
    let classified = classify_m11_simple_edit_line(&source, start == 0);
    let M11SimpleEditLineKind::AtxHeading { content, empty, .. } = classified.kind else {
        return None;
    };
    if !empty {
        return None;
    }
    let content_start_utf16 = std::str::from_utf8(source.get(..content.start)?)
        .ok()?
        .encode_utf16()
        .count();
    let content_end_utf16 = std::str::from_utf8(source.get(..content.end)?)
        .ok()?
        .encode_utf16()
        .count();
    Some((
        frame_bytes.start + content.start as u64..frame_bytes.start + content.end as u64,
        frame_utf16.start + content_start_utf16 as u64
            ..frame_utf16.start + content_end_utf16 as u64,
    ))
}

fn document_inline_facts(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<Vec<DocumentInlineFact>>, DocumentSessionError> {
    if M11RecursiveGreenInlineLeafKind::from_green_kind(row.kind()).is_none()
        || row.edit_capability() != M11RecursiveGreenRowEditCapability::Contiguous
    {
        return Ok(None);
    }
    let (Some(editable), Some(editable_utf16)) = (row.editable_range(), row.editable_utf16_range())
    else {
        return Ok(None);
    };
    if editable.end.saturating_sub(editable.start) > VIEWPORT_INLINE_LEAF_MAX_BYTES {
        return Ok(None);
    }

    let prepared = session.prepare_inline_leaf(
        runtime,
        M11RecursiveGreenPoint::new(
            usize::try_from(row.physical_range().start)
                .map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
            usize::try_from(row.physical_utf16_range().start)
                .map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
            SourceBoundaryAffinity::After,
        ),
    )?;
    let inline_source = prepared.inline_source_range();
    let inline_source_utf16 = prepared.inline_source_utf16_range();
    if u64::from(inline_source.start) != editable.start
        || u64::from(inline_source.end) != editable.end
        || u64::from(inline_source_utf16.start) != editable_utf16.start
        || u64::from(inline_source_utf16.end) != editable_utf16.end
    {
        return Ok(None);
    }
    let parser_profile = ParserProfileId::new(u64::from(session.syntax_profile()))
        .ok_or(DocumentSessionError::Faulted)?;
    let reference_resolver = session.reference_resolver(runtime)?;
    let mut job =
        M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_reference_resolver_and_fact_capture(
            runtime,
            prepared.into_fence(),
            M11ParserBinding::current(parser_profile),
            reference_resolver,
        )?;
    let mut transitions = 0_usize;
    loop {
        let remaining = VIEWPORT_INLINE_TOTAL_TRANSITIONS_MAX.saturating_sub(transitions);
        if remaining == 0 {
            abort_inline_fact_job(runtime, &mut job)?;
            return Ok(None);
        }
        let poll = match job.poll(
            runtime,
            remaining.min(M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS),
        ) {
            Ok(poll) => poll,
            Err(error) => {
                abort_inline_fact_job(runtime, &mut job)?;
                return Err(error.into());
            }
        };
        transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(DocumentSessionError::Faulted)?;
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            break;
        }
        if poll.transitions() == 0 {
            abort_inline_fact_job(runtime, &mut job)?;
            return Ok(None);
        }
    }

    let facts = job
        .take_projected_facts()
        .ok_or(DocumentSessionError::Faulted)?;
    abort_inline_fact_job(runtime, &mut job)?;
    map_document_inline_facts(runtime, inline_source, editable, facts)
}

fn abort_inline_fact_job(
    runtime: &mut DocumentRuntime,
    job: &mut M11InlineProjectionJob,
) -> Result<(), M11InlineProjectionJobError> {
    job.begin_abort(runtime)?;
    loop {
        let poll = job.poll_abort(runtime, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS)?;
        if poll.complete() {
            return Ok(());
        }
    }
}

fn map_document_inline_facts(
    runtime: &DocumentRuntime,
    inline_source: Range<u32>,
    editable: Range<u64>,
    facts: Vec<M11InlineProjectionFact>,
) -> Result<Option<Vec<DocumentInlineFact>>, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let inline_start =
        usize::try_from(inline_source.start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let inline_end =
        usize::try_from(inline_source.end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let mut cursor = lease.cursor_in(inline_start..inline_end)?;
    let mut inline_bytes = vec![0_u8; inline_end.saturating_sub(inline_start)];
    let mut written = 0;
    while written < inline_bytes.len() {
        let count = cursor.read(&mut inline_bytes[written..]);
        if count == 0 {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        written += count;
    }
    let lease = cursor.finish()?;

    let mut mapped = Vec::with_capacity(facts.len());
    for fact in facts {
        let mut relative_content = fact.relative_content_range();
        let mut replacement = None;
        let kind = match fact.kind() {
            M11InlineProjectionKind::Emphasis => DocumentInlineFactKind::Emphasis,
            M11InlineProjectionKind::Strong => DocumentInlineFactKind::Strong,
            M11InlineProjectionKind::Code => {
                if fact.flags() & M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE != 0 {
                    relative_content = trim_code_content_range(&inline_bytes, relative_content)?;
                }
                DocumentInlineFactKind::Code
            }
            M11InlineProjectionKind::Strikethrough => DocumentInlineFactKind::Strikethrough,
            M11InlineProjectionKind::AutolinkUri => DocumentInlineFactKind::AutolinkUri,
            M11InlineProjectionKind::AutolinkEmail => DocumentInlineFactKind::AutolinkEmail,
            M11InlineProjectionKind::BackslashEscape => DocumentInlineFactKind::BackslashEscape,
            M11InlineProjectionKind::HardLineBreak => DocumentInlineFactKind::HardLineBreak,
            M11InlineProjectionKind::CharacterReference => {
                let (first, second) = fact
                    .character_reference()
                    .ok_or(DocumentSessionError::Faulted)?;
                let source = fact.relative_range();
                relative_content = source;
                replacement = Some(DocumentInlineReplacement { first, second });
                DocumentInlineFactKind::Replacement
            }
            M11InlineProjectionKind::DirectLink => DocumentInlineFactKind::DirectLink,
            M11InlineProjectionKind::DirectImage => DocumentInlineFactKind::DirectImage,
            M11InlineProjectionKind::ReferenceLink => DocumentInlineFactKind::ReferenceLink,
            M11InlineProjectionKind::ReferenceImage => DocumentInlineFactKind::ReferenceImage,
        };
        let source = absolute_inline_range(inline_source.start, fact.relative_range())?;
        let content = absolute_inline_range(inline_source.start, relative_content.clone())?;
        if source.start < editable.start
            || source.end > editable.end
            || content.start < source.start
            || content.end > source.end
        {
            return Err(DocumentSessionError::Faulted);
        }
        let continuity = match kind {
            DocumentInlineFactKind::Emphasis
            | DocumentInlineFactKind::Strong
            | DocumentInlineFactKind::Code
            | DocumentInlineFactKind::Strikethrough
            | DocumentInlineFactKind::DirectLink => DOCUMENT_INLINE_FACT_CONTINUITY_PLAIN_TEXT,
            _ => 0,
        };
        push_document_inline_fact(
            &mut mapped,
            &lease,
            kind,
            fact.flags() | continuity,
            source,
            content,
            replacement,
        )?;

        if fact.kind() == M11InlineProjectionKind::Code
            && fact.flags() & M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS != 0
        {
            append_code_line_ending_replacements(
                &mut mapped,
                &lease,
                inline_source.start,
                &inline_bytes,
                relative_content,
            )?;
        }
        if mapped.len() > VIEWPORT_INLINE_FACTS_PER_ROW_MAX {
            return Ok(None);
        }
    }
    append_document_table_facts(&mut mapped, &lease, inline_source.start, &inline_bytes)?;
    if mapped.len() > VIEWPORT_INLINE_FACTS_PER_ROW_MAX {
        return Ok(None);
    }
    Ok(Some(mapped))
}

fn append_document_table_facts(
    mapped: &mut Vec<DocumentInlineFact>,
    lease: &SourceSnapshotLease,
    inline_base: u32,
    inline_bytes: &[u8],
) -> Result<(), DocumentSessionError> {
    let source = str::from_utf8(inline_bytes).map_err(|_| DocumentSessionError::Faulted)?;
    let table = match project_m11_gfm_table(source) {
        Ok(Some(table)) if table.preface_range.is_none() => table,
        Ok(None) | Err(_) => return Ok(()),
        Ok(Some(_)) => return Ok(()),
    };
    for (header, row) in
        std::iter::once((true, &table.header)).chain(table.body.iter().map(|row| (false, row)))
    {
        for (column, cell) in row.cells.iter().enumerate() {
            let alignment = match table
                .alignments
                .get(column)
                .copied()
                .unwrap_or(M11GfmTableAlignment::None)
            {
                M11GfmTableAlignment::None => 0,
                M11GfmTableAlignment::Left => 1,
                M11GfmTableAlignment::Center => 2,
                M11GfmTableAlignment::Right => 3,
            };
            let flags = alignment
                | if header {
                    DOCUMENT_TABLE_CELL_HEADER
                } else {
                    0
                }
                | if column == 0 {
                    DOCUMENT_TABLE_CELL_ROW_START
                } else {
                    0
                }
                | if cell.autocompleted {
                    DOCUMENT_TABLE_CELL_AUTOCOMPLETED
                } else {
                    0
                };
            let source = absolute_inline_range(inline_base, cell.source_range.clone())?;
            let content = absolute_inline_range(inline_base, cell.content_range.clone())?;
            push_document_inline_fact(
                mapped,
                lease,
                DocumentInlineFactKind::TableCell,
                flags,
                source,
                content,
                None,
            )?;
            for escape in &cell.pipe_escape_ranges {
                let source = absolute_inline_range(inline_base, escape.clone())?;
                push_document_inline_fact(
                    mapped,
                    lease,
                    DocumentInlineFactKind::Replacement,
                    0,
                    source.clone(),
                    source,
                    Some(DocumentInlineReplacement {
                        first: '|',
                        second: None,
                    }),
                )?;
            }
        }
    }
    Ok(())
}

fn trim_code_content_range(
    inline_bytes: &[u8],
    content: Range<u32>,
) -> Result<Range<u32>, DocumentSessionError> {
    let start =
        usize::try_from(content.start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let end = usize::try_from(content.end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let bytes = inline_bytes
        .get(start..end)
        .ok_or(DocumentSessionError::RangeOutOfBounds)?;
    let opener = normalized_space_width_at_start(bytes).ok_or(DocumentSessionError::Faulted)?;
    let closer = normalized_space_width_at_end(bytes).ok_or(DocumentSessionError::Faulted)?;
    if opener
        .checked_add(closer)
        .is_none_or(|cut| cut >= bytes.len())
    {
        return Err(DocumentSessionError::Faulted);
    }
    Ok(content.start + opener as u32..content.end - closer as u32)
}

fn normalized_space_width_at_start(bytes: &[u8]) -> Option<usize> {
    match bytes {
        [b' ', ..] | [b'\n', ..] | [b'\r'] => Some(1),
        [b'\r', b'\n', ..] => Some(2),
        [b'\r', ..] => Some(1),
        _ => None,
    }
}

fn normalized_space_width_at_end(bytes: &[u8]) -> Option<usize> {
    match bytes {
        [.., b'\r', b'\n'] => Some(2),
        [.., b' '] | [.., b'\n'] | [.., b'\r'] => Some(1),
        _ => None,
    }
}

fn append_code_line_ending_replacements(
    mapped: &mut Vec<DocumentInlineFact>,
    lease: &SourceSnapshotLease,
    inline_base: u32,
    inline_bytes: &[u8],
    content: Range<u32>,
) -> Result<(), DocumentSessionError> {
    let start =
        usize::try_from(content.start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let end = usize::try_from(content.end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let bytes = inline_bytes
        .get(start..end)
        .ok_or(DocumentSessionError::RangeOutOfBounds)?;
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let width = match bytes[offset] {
            b'\r' if bytes.get(offset + 1) == Some(&b'\n') => 2,
            b'\r' | b'\n' => 1,
            _ => {
                offset += 1;
                continue;
            }
        };
        let relative_start = content
            .start
            .checked_add(u32::try_from(offset).map_err(|_| DocumentSessionError::Faulted)?)
            .ok_or(DocumentSessionError::Faulted)?;
        let relative_end = relative_start
            .checked_add(u32::try_from(width).map_err(|_| DocumentSessionError::Faulted)?)
            .ok_or(DocumentSessionError::Faulted)?;
        let source = absolute_inline_range(inline_base, relative_start..relative_end)?;
        push_document_inline_fact(
            mapped,
            lease,
            DocumentInlineFactKind::Replacement,
            0,
            source.clone(),
            source,
            Some(DocumentInlineReplacement {
                first: ' ',
                second: None,
            }),
        )?;
        offset += width;
    }
    Ok(())
}

fn push_document_inline_fact(
    mapped: &mut Vec<DocumentInlineFact>,
    lease: &SourceSnapshotLease,
    kind: DocumentInlineFactKind,
    flags: u8,
    source: Range<u64>,
    content: Range<u64>,
    replacement: Option<DocumentInlineReplacement>,
) -> Result<(), DocumentSessionError> {
    let source_utf16 = source_utf16_range(lease, &source)?;
    let content_utf16 = source_utf16_range(lease, &content)?;
    mapped.push(DocumentInlineFact {
        kind,
        flags,
        source_range: source,
        source_utf16_range: source_utf16,
        content_range: content,
        content_utf16_range: content_utf16,
        replacement,
    });
    Ok(())
}

fn source_utf16_range(
    lease: &SourceSnapshotLease,
    range: &Range<u64>,
) -> Result<Range<u64>, DocumentSessionError> {
    Ok(u64::try_from(lease.utf16_offset_for_byte(
        usize::try_from(range.start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
    )?)
    .map_err(|_| DocumentSessionError::RangeOutOfBounds)?
        ..u64::try_from(lease.utf16_offset_for_byte(
            usize::try_from(range.end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
        )?)
        .map_err(|_| DocumentSessionError::RangeOutOfBounds)?)
}

fn absolute_inline_range(
    base: u32,
    relative: Range<u32>,
) -> Result<Range<u64>, DocumentSessionError> {
    let start = base
        .checked_add(relative.start)
        .ok_or(DocumentSessionError::RangeOutOfBounds)?;
    let end = base
        .checked_add(relative.end)
        .ok_or(DocumentSessionError::RangeOutOfBounds)?;
    Ok(u64::from(start)..u64::from(end))
}

const fn document_edit_capability(
    capability: M11RecursiveGreenRowEditCapability,
) -> DocumentViewportRowEditCapability {
    match capability {
        M11RecursiveGreenRowEditCapability::Contiguous => {
            DocumentViewportRowEditCapability::Contiguous
        }
        M11RecursiveGreenRowEditCapability::ProjectedReserved => {
            DocumentViewportRowEditCapability::ProjectedReserved
        }
        M11RecursiveGreenRowEditCapability::Unavailable => {
            DocumentViewportRowEditCapability::Unavailable
        }
    }
}

fn certified_range_live_viewport(
    runtime: &DocumentRuntime,
    revision: u64,
    requested_range: Range<usize>,
    _maximum_spans: u32,
) -> Result<DocumentLiveViewport, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let spans = if requested_range.is_empty() {
        Vec::new()
    } else {
        vec![DocumentLiveViewportSpan::CertifiedUnchanged {
            source_range: requested_range.start as u64..requested_range.end as u64,
            source_utf16_range: lease.utf16_offset_for_byte(requested_range.start)? as u64
                ..lease.utf16_offset_for_byte(requested_range.end)? as u64,
        }]
    };
    Ok(DocumentLiveViewport {
        revision,
        requested_range: requested_range.start as u64..requested_range.end as u64,
        covered_range: requested_range.start as u64..requested_range.end as u64,
        complete: true,
        spans,
        receipt: DocumentQueryReceipt::default(),
    })
}

fn pending_live_viewport(
    runtime: &DocumentRuntime,
    revision: u64,
    requested_range: Range<usize>,
    _maximum_spans: u32,
) -> Result<DocumentLiveViewport, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let spans = if requested_range.is_empty() {
        Vec::new()
    } else {
        vec![DocumentLiveViewportSpan::Pending {
            source_range: requested_range.start as u64..requested_range.end as u64,
            source_utf16_range: lease.utf16_offset_for_byte(requested_range.start)? as u64
                ..lease.utf16_offset_for_byte(requested_range.end)? as u64,
        }]
    };
    Ok(DocumentLiveViewport {
        revision,
        requested_range: requested_range.start as u64..requested_range.end as u64,
        covered_range: requested_range.start as u64..requested_range.end as u64,
        complete: true,
        spans,
        receipt: DocumentQueryReceipt::default(),
    })
}

fn query_adopting_live_viewport(
    runtime: &DocumentRuntime,
    adoption: &M11PersistentRecursiveGreenAdoption,
    revision: u64,
    requested_range: Range<usize>,
    maximum_spans: u32,
) -> Result<DocumentLiveViewport, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let mut spans = Vec::with_capacity(maximum_spans as usize);
    let mut cursor = requested_range.start;
    let maximum_spans = maximum_spans as usize;

    for region in adoption.live_projection_regions().into_iter().flatten() {
        let target = region.target_byte_range();
        let start = cursor.max(requested_range.start).max(target.start);
        let end = requested_range.end.min(target.end);
        if start >= end {
            continue;
        }
        if cursor < start {
            if spans.len() == maximum_spans {
                break;
            }
            spans.push(DocumentLiveViewportSpan::Pending {
                source_range: cursor as u64..start as u64,
                source_utf16_range: lease.utf16_offset_for_byte(cursor)? as u64
                    ..lease.utf16_offset_for_byte(start)? as u64,
            });
            cursor = start;
        }
        if spans.len() == maximum_spans {
            break;
        }
        spans.push(DocumentLiveViewportSpan::CertifiedUnchanged {
            source_range: start as u64..end as u64,
            source_utf16_range: lease.utf16_offset_for_byte(start)? as u64
                ..lease.utf16_offset_for_byte(end)? as u64,
        });
        cursor = end;
    }

    if cursor < requested_range.end && spans.len() < maximum_spans {
        spans.push(DocumentLiveViewportSpan::Pending {
            source_range: cursor as u64..requested_range.end as u64,
            source_utf16_range: lease.utf16_offset_for_byte(cursor)? as u64
                ..lease.utf16_offset_for_byte(requested_range.end)? as u64,
        });
        cursor = requested_range.end;
    }
    Ok(DocumentLiveViewport {
        revision,
        requested_range: requested_range.start as u64..requested_range.end as u64,
        covered_range: requested_range.start as u64..cursor as u64,
        complete: cursor == requested_range.end,
        spans,
        receipt: DocumentQueryReceipt::default(),
    })
}

/// Releases a clean build whose poll failed.
///
/// Cancellation is drained in bounded turns. If it cannot be drained the
/// build is deliberately leaked: retaining its arena state is strictly
/// better than the drop assertion, which would stop the document actor and
/// erase the typed fault that caused this path.
fn release_failed_clean_build(
    runtime: &mut DocumentRuntime,
    mut build: Box<M11PersistentRecursiveGreenCleanBuild>,
) {
    const RELEASE_FUEL_PER_TURN: usize = 256;
    const MAX_RELEASE_TURNS: usize = 4096;
    if build.begin_cancel(runtime).is_ok() {
        for _ in 0..MAX_RELEASE_TURNS {
            match build.poll_cancel(runtime, RELEASE_FUEL_PER_TURN) {
                Ok(poll) if poll.status() == M11PersistentRecursiveGreenBuildStatus::Cancelled => {
                    return;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
    mem::forget(build);
}

fn begin_clean_build(
    runtime: &mut DocumentRuntime,
) -> Result<M11PersistentRecursiveGreenCleanBuild, DocumentSessionError> {
    let scanner = runtime.snapshot_current_source()?;
    let writer = runtime.snapshot_current_source()?;
    Ok(
        M11PersistentRecursiveGreenCleanPlan::new(scanner, writer, SYNTAX_PROFILE_GFM_V1)?
            .begin(runtime)?,
    )
}

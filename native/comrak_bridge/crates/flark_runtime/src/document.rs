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
    M11InlineProjectionJob, M11InlineProjectionJobError, M11InlineProjectionJobPollStatus,
    M11ParserBinding, M11PersistentRecursiveGreenAdoption,
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenAdoptionWork,
    M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanBuild,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
    M11PersistentRecursiveGreenSessionError, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
};

const SYNTAX_PROFILE_GFM_V1: u32 = 1;
const QUERY_OPEN_DEPTH_LIMIT: usize = 256;
const VIEWPORT_INLINE_LEAF_MAX_BYTES: u64 = 8 * 1024;
const VIEWPORT_INLINE_FACTS_PER_ROW_MAX: usize = 64;
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
}

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
    pub presentation: DocumentViewportRowPresentation,
    /// `Some` means the complete bounded inline leaf is authoritative. Empty
    /// is distinct from `None`, which requires exact-source neutral display.
    pub inline_facts: Option<Vec<DocumentInlineFact>>,
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
        let mut runtime = DocumentRuntime::new(source, config)?;
        let parser = ParseState::Clean(Box::new(begin_clean_build(&mut runtime)?));
        Ok(Self {
            runtime,
            parser,
            last_edit_work: M11PersistentRecursiveGreenAdoptionWork::default(),
            fault_arena_metrics: None,
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
        let state = mem::replace(&mut self.parser, ParseState::Transition);
        match state {
            ParseState::Ready(base) => self.apply_edit_to_ready_base(base, range, replacement),
            ParseState::Faulted => {
                self.parser = ParseState::Faulted;
                Err(DocumentSessionError::Faulted)
            }
            ParseState::Clean(mut build) => {
                if let Err(error) = build.begin_cancel(&mut self.runtime) {
                    self.parser = ParseState::Clean(build);
                    return Err(error.into());
                }
                let result = self.apply_edit_while_building(range, replacement);
                self.parser = ParseState::CancellingClean(build);
                result
            }
            ParseState::CancellingClean(build) => {
                let result = self.apply_edit_while_building(range, replacement);
                self.parser = ParseState::CancellingClean(build);
                result
            }
            ParseState::Adopting(mut adoption) => {
                if let Err(error) = adoption.begin_cancel(&mut self.runtime) {
                    self.parser = ParseState::Adopting(adoption);
                    return Err(error.into());
                }
                let result = self.apply_edit_while_building(range, replacement);
                self.parser = ParseState::CancellingAdoption(adoption);
                result
            }
            ParseState::CancellingAdoption(adoption) => {
                let result = self.apply_edit_while_building(range, replacement);
                self.parser = ParseState::CancellingAdoption(adoption);
                result
            }
            ParseState::ReleasingBaseForClean(base) => {
                let result = self.apply_edit_while_building(range, replacement);
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

#[cfg(test)]
mod tests {
    use super::*;

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
    let presentation = match m11_recursive_green_row_presentation(runtime, row)
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
    if let DocumentViewportRowPresentation::ListItem {
        prefix_end_byte,
        prefix_end_utf16,
        ..
    } = presentation
    {
        if prefix_end_byte < source_range.start && prefix_end_utf16 < source_utf16_range.start {
            editable_range = Some(prefix_end_byte..prefix_end_byte);
            editable_utf16_range = Some(prefix_end_utf16..prefix_end_utf16);
        }
    }
    let inline_facts = document_inline_facts(runtime, session, row)?;
    Ok(DocumentViewportRow {
        ordinal: row.ordinal(),
        kind: row.kind().get(),
        source_range,
        source_utf16_range,
        editable_range,
        editable_utf16_range,
        edit_capability: document_edit_capability(row.edit_capability()),
        presentation,
        inline_facts,
        path_depth: u32::try_from(row.path().len()).unwrap_or(u32::MAX),
    })
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
        push_document_inline_fact(
            &mut mapped,
            &lease,
            kind,
            fact.flags(),
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
    Ok(Some(mapped))
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

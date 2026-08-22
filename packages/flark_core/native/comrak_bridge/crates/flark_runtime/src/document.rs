use std::fmt;
use std::mem;
use std::ops::Range;

use flark_engine::parser_internal::{
    M11InlineLinkValue, M11InlineProjectionFact, M11InlineProjectionKind, M11RecursiveGreenPoint,
    M11RecursiveGreenRenderableRow, M11RecursiveGreenRowEditCapability,
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowQueryOutcome, M11ReferenceResolver,
    M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
    M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
    M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE,
};
use flark_engine::{
    ArenaMetrics, DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, ParserProfileId,
    SourceBoundaryAffinity, SourceEditError, SourceSnapshotLease, SourceVersion,
};
#[cfg(feature = "opening-session")]
use flark_engine::{OpeningSourceError, OpeningSourceStore, OpeningSourceVersion, SourceRevision};
use flark_parser::{
    block_core::{
        m11_block_quote_prefix_lineage, m11_recursive_green_row_presentation, BulletMarker,
        FenceCharacter, HeadingStyle, ListDelimiter, M11RecursiveGreenCodeBlockStyle,
        M11RecursiveGreenInlineLeafKind, M11RecursiveGreenListMarker,
        M11RecursiveGreenRowPresentation,
    },
    classify_m11_simple_edit_line, project_m11_gfm_inline, project_m11_gfm_table, M11GfmInlineNode,
    M11GfmInlineOptions, M11GfmTableAlignment, M11InlineEditComponent,
    M11InlineEditComponentMatcher, M11InlineProjectionJob, M11InlineProjectionJobError,
    M11InlineProjectionJobPollStatus, M11ParserBinding, M11PersistentRecursiveGreenAdoption,
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenAdoptionWork,
    M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanBuild,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
    M11PersistentRecursiveGreenSessionError, M11RecursiveGreenInlineLeafPreparation,
    M11SimpleEditLineKind, M11SimpleEditListMarker, M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
    M11_SIMPLE_EDIT_LINE_MAX_BYTES,
};

#[cfg(feature = "opening-session")]
use flark_parser::{
    M11CompactViewportProbeError, M11ProgressiveOpenSession, M11ProgressiveOpenSessionPoll,
};

use crate::edit_intent::{
    resolve_document_edit_intent_v1, DocumentBlockQuoteOutdent, DocumentEditLineEnding,
    DocumentListIndent, DocumentListOutdent, DocumentParagraphMerge, DocumentSimpleEditContext,
    DocumentSimpleEditRow, DocumentTaskCheck, ResolvedDocumentEditIntentV1,
};
use crate::{
    DocumentEditIntentDispositionV1, DocumentEditIntentReceiptV1, DocumentEditIntentV1,
    DocumentEditPresentationTransitionV1, DocumentSourceTransactionReceiptV1,
    DocumentStagedSourceTransactionReceiptV1,
};

const SYNTAX_PROFILE_GFM_V1: u32 = 1;
const QUERY_OPEN_DEPTH_LIMIT: usize = 256;
const VIEWPORT_INLINE_LEAF_MAX_BYTES: u64 = 8 * 1024;
const VIEWPORT_INLINE_FACTS_PER_ROW_MAX: usize = 512;
// Kind-15 envelopes share the ABI's 64 KiB semantic payload with ordinary
// facts and edit cells. Keep the parser-authored optimization bounded so a
// word-dense fact cannot evict the row's authoritative inline fact set.
const VIEWPORT_LITERAL_SAFE_ENVELOPES_PER_ROW_MAX: usize = 128;
const VIEWPORT_INLINE_TOTAL_TRANSITIONS_MAX: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
enum StructuralInlineProofToken {
    Text(String),
    Enter(u8, String, String),
    Exit(u8),
    SoftBreak,
    HardBreak,
}

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

/// Parser-declared literal edit class for one certified safe envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentLiteralEditClass {
    /// A non-empty insertion containing only ASCII letters and digits.
    AsciiWordInsertion,
    /// One U+0020 insertion. This deliberately does not authorize a second
    /// edit before the parser certifies the first one.
    SingleAsciiSpaceInsertion,
    /// One U+002A insertion inside a parser-proved isolated flat Strong
    /// content range. The proof is consumed by the matching edit.
    SingleAsciiAsteriskInsertion,
}

/// Parser-authored positional proof that one declared literal edit class
/// cannot change the row's published facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentLiteralSafeEnvelope {
    pub edit_class: DocumentLiteralEditClass,
    pub source_range: Range<u64>,
    pub source_utf16_range: Range<u64>,
}

pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCH_ANY_NO_CRLF_SPLICE: u32 = 0x0001;
pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCH_ASCII_LITERAL_SPLICE_IN_LITERAL: u32 = 0x0002;
pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCH_INSERT_SINGLE_ASCII_SPACE_AT_POINT: u32 = 0x0003;
pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL: u32 = 0x0004;
pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCH_APPEND_ASCII_LITERAL_AT_LINE_END: u32 = 0x0005;
pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCH_INSERT_EXACT_SCALAR_AT_POINT: u32 = 0x0006;
pub const DOCUMENT_PROJECTION_EDIT_CELL_MATCHER_MASK: u32 = 0x00ff;
pub const DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL: u32 = 0x0100;
pub const DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_OUTSIDE: u32 = 0x0200;
pub const DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT: u32 = 0x0400;
pub const DOCUMENT_PROJECTION_EDIT_CELL_CHAIN_RESULT: u32 = 0x0800;
pub const DOCUMENT_PROJECTION_EDIT_CELL_RETENTION_MASK: u32 = 0x0f00;
pub const DOCUMENT_PROJECTION_EDIT_CELL_TERMINAL_SPACE_BLOCKED: u32 = 0x1000;
pub const DOCUMENT_PROJECTION_EDIT_CELL_KNOWN_FLAGS_MASK: u32 = 0x1fff;
pub const DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS: u32 =
    DOCUMENT_PROJECTION_EDIT_CELL_MATCH_ANY_NO_CRLF_SPLICE
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
        | DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT
        | DOCUMENT_PROJECTION_EDIT_CELL_CHAIN_RESULT;
pub const DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS: u32 =
    DOCUMENT_PROJECTION_EDIT_CELL_MATCH_ASCII_LITERAL_SPLICE_IN_LITERAL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_OUTSIDE
        | DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT
        | DOCUMENT_PROJECTION_EDIT_CELL_CHAIN_RESULT;
pub const DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS: u32 =
    DOCUMENT_PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_OUTSIDE
        | DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT;
pub const DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS: u32 =
    DOCUMENT_PROJECTION_EDIT_CELL_MATCH_APPEND_ASCII_LITERAL_AT_LINE_END
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_OUTSIDE
        | DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT
        | DOCUMENT_PROJECTION_EDIT_CELL_CHAIN_RESULT;
pub const DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS: u32 =
    DOCUMENT_PROJECTION_EDIT_CELL_MATCH_INSERT_SINGLE_ASCII_SPACE_AT_POINT
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_OUTSIDE
        | DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT;
pub const DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS: u32 =
    DOCUMENT_PROJECTION_EDIT_CELL_MATCH_INSERT_EXACT_SCALAR_AT_POINT
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
        | DOCUMENT_PROJECTION_EDIT_CELL_RETAIN_OUTSIDE
        | DOCUMENT_PROJECTION_EDIT_CELL_PRESENT_EXACT;

/// Parser-authored pre-edit geometry for one bounded projection edit cell.
///
/// The first contract admits arbitrary non-newline edits within one complete
/// ATX heading content range. The host may retain only the parser-certified
/// block shell, presents the transformed cell as exact source, and may chain
/// the transformed range while current-revision parsing catches up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentProjectionEditCell {
    /// The pre-edit dependency closure that becomes exact while parsing is
    /// pending. This maps to kind 16's source geometry at the ABI boundary.
    pub source_range: Range<u64>,
    pub source_utf16_range: Range<u64>,
    /// The pre-edit admission range. This maps to kind 16's content geometry
    /// at the ABI boundary and may be narrower than the dependency closure.
    pub trigger_range: Range<u64>,
    pub trigger_utf16_range: Range<u64>,
    pub flags: u32,
    pub replacement_first: u32,
    pub replacement_second: u32,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentSemanticTargetKind {
    Link,
    Image,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentSemanticTargetSyntax {
    AutolinkUri,
    AutolinkEmail,
    Direct,
    Reference,
}

/// One parser-certified link or image target resolved on demand.
///
/// Target values deliberately do not ride every viewport receipt: activation
/// is outside the frame-critical path, while the exact source cuts and cooked
/// values remain native-authoritative at the revision where they are used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentSemanticTarget {
    pub kind: DocumentSemanticTargetKind,
    pub syntax: DocumentSemanticTargetSyntax,
    pub source_range: Range<u64>,
    pub source_utf16_range: Range<u64>,
    pub content_range: Range<u64>,
    pub content_utf16_range: Range<u64>,
    pub destination_source_range: Range<u64>,
    pub destination_source_utf16_range: Range<u64>,
    pub title_source_range: Option<Range<u64>>,
    pub title_source_utf16_range: Option<Range<u64>>,
    pub destination: String,
    pub title: Option<String>,
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
        item_end_byte: u64,
        item_end_utf16: u64,
        nesting_depth: u8,
        marker_offset: u8,
        item_padding: u8,
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
        container_widths: u64,
        container_count: u8,
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
    pub presentation: DocumentViewportRowPresentation,
    /// `Some` means the complete bounded inline leaf is authoritative. Empty
    /// is distinct from `None`, which requires exact-source neutral display.
    pub inline_facts: Option<Vec<DocumentInlineFact>>,
    /// Exact parser-authored ranges for typed literal edits that may retain
    /// this row's presentation until current-revision certification arrives.
    pub literal_safe_envelopes: Vec<DocumentLiteralSafeEnvelope>,
    /// Bounded parser-authored cells whose declared presentation policies may
    /// bridge a pending parser revision. An empty list fails closed.
    pub projection_edit_cells: Vec<DocumentProjectionEditCell>,
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
        self.complete
            && self.covered_range == self.requested_range
            && self
                .spans
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
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    RangeOutOfBounds,
    EditIntentLimitExceeded,
    UnsupportedEditIntentSelection,
    QueryBudgetExceeded,
    Engine(DocumentRuntimeError),
    Source(SourceEditError),
    Parser(M11PersistentRecursiveGreenSessionError),
    Inline(M11InlineProjectionJobError),
    #[cfg(feature = "opening-session")]
    Opening(OpeningSourceError),
    #[cfg(feature = "opening-session")]
    Compact(M11CompactViewportProbeError),
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
            #[cfg(feature = "opening-session")]
            Self::Opening(error) => error.fmt(formatter),
            #[cfg(feature = "opening-session")]
            Self::Compact(error) => error.fmt(formatter),
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

#[cfg(feature = "opening-session")]
impl From<OpeningSourceError> for DocumentSessionError {
    fn from(value: OpeningSourceError) -> Self {
        Self::Opening(value)
    }
}

#[cfg(feature = "opening-session")]
impl From<M11CompactViewportProbeError> for DocumentSessionError {
    fn from(value: M11CompactViewportProbeError) -> Self {
        Self::Compact(value)
    }
}

impl From<M11InlineProjectionJobError> for DocumentSessionError {
    fn from(value: M11InlineProjectionJobError) -> Self {
        Self::Inline(value)
    }
}

#[cfg(feature = "opening-session")]
/// Progressive-open authority: the session owns the store (sole mutation
/// authority during load), the incremental parser session over the replica,
/// and the last store version adopted into runtime and parser.
struct OpeningState {
    store: OpeningSourceStore,
    session: M11ProgressiveOpenSession,
    adopted: OpeningSourceVersion,
    seal_requested: bool,
    finalizing: bool,
}

enum ParseState {
    #[cfg(feature = "opening-session")]
    Opening(Box<OpeningState>),
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

    #[cfg(feature = "opening-session")]
    /// Begins one progressive open: no source is required up front, pages
    /// are admitted through [`Self::opening_append_page`], and the load ends
    /// only at [`Self::seal_opening`]. The first certified viewport becomes
    /// queryable before EOF.
    pub fn begin_opening() -> Result<Self, DocumentSessionError> {
        let store = OpeningSourceStore::new(SourceRevision::new(0), None)?;
        let mut runtime = DocumentRuntime::from_opening_snapshot(
            store.snapshot(),
            DocumentRuntimeConfig::default(),
        )?;
        let session = M11ProgressiveOpenSession::begin(&mut runtime, SYNTAX_PROFILE_GFM_V1)?;
        let adopted = store.version();
        Ok(Self {
            runtime,
            parser: ParseState::Opening(Box::new(OpeningState {
                store,
                session,
                adopted,
                seal_requested: false,
                finalizing: false,
            })),
            last_edit_work: M11PersistentRecursiveGreenAdoptionWork::default(),
            fault_arena_metrics: None,
            edit_context: None,
            fallback_line_ending: dominant_edit_line_ending(b""),
        })
    }

    #[cfg(feature = "opening-session")]
    /// Admits one bounded transport page during a progressive open. The
    /// parser adopts it at its next starvation inside [`Self::pump`].
    pub fn opening_append_page(&mut self, text: &str) -> Result<(), DocumentSessionError> {
        let ParseState::Opening(state) = &mut self.parser else {
            return Err(DocumentSessionError::NotReady);
        };
        if state.seal_requested {
            return Err(DocumentSessionError::Busy);
        }
        let version = state.store.version();
        let start = version.admitted_input_utf16();
        let page_utf16 = text.encode_utf16().count();
        state
            .store
            .append_page(version, start..start + page_utf16, text)?;
        Ok(())
    }

    /// Returns whether a progressive open currently holds a certified early
    /// viewport at the live generation. False for every non-opening state:
    /// callers use this to route pre-certification queries to exact pending
    /// source rather than semantic paths.
    #[cfg(feature = "opening-session")]
    #[must_use]
    pub fn opening_certified(&self) -> bool {
        let ParseState::Opening(state) = &self.parser else {
            return false;
        };
        state
            .session
            .certified_early()
            .is_some_and(|(_, source)| self.runtime.current_source_version() == Some(source))
    }

    #[cfg(feature = "opening-session")]
    /// Declares transport end: after every admitted page is adopted, the
    /// load seals at exactly the admitted text and parsing runs to EOF
    /// authority.
    pub fn seal_opening(&mut self) -> Result<(), DocumentSessionError> {
        let ParseState::Opening(state) = &mut self.parser else {
            return Err(DocumentSessionError::NotReady);
        };
        state.seal_requested = true;
        Ok(())
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
            #[cfg(feature = "opening-session")]
            ParseState::Opening(_) => DocumentSessionPhase::Building,
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
            let remaining = max_work_units - consumed;
            let next = match state {
                // A clean build already owns a fuel-bounded inner state
                // machine. Let it consume the remaining grant directly
                // instead of moving its large state through `ParseState` once
                // per transition. Retirement is still polled before each
                // bounded grant, and the caller's work-unit ceiling remains
                // exact.
                ParseState::Clean(build) => self.advance_clean(build, remaining),
                #[cfg(feature = "opening-session")]
                ParseState::Opening(state) => self.advance_opening(state, remaining),
                other => self.advance_one(other).map(|state| (state, 1)),
            };
            match next {
                Ok((state, work_units)) => {
                    self.parser = state;
                    consumed += work_units;
                }
                Err(error) => {
                    if self.fault_arena_metrics.is_none() {
                        self.fault_arena_metrics = Some(self.runtime.arena_metrics());
                    }
                    self.parser = ParseState::Faulted;
                    return Err(error);
                }
            }
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
            #[cfg(feature = "opening-session")]
            ParseState::Opening(state) => self.advance_opening(state, 1).map(|(state, _)| state),
            ParseState::Clean(build) => self.advance_clean(build, 1).map(|(state, _)| state),
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

    fn advance_clean(
        &mut self,
        mut build: Box<M11PersistentRecursiveGreenCleanBuild>,
        max_work_units: usize,
    ) -> Result<(ParseState, usize), DocumentSessionError> {
        // A recursive-green build asserts on drop unless its root was
        // transferred or it was explicitly cancelled, so an error here must
        // release the build rather than let it fall out of scope: otherwise
        // the assertion kills the document actor thread and every later call
        // reports an opaque internal fault instead of this typed parser error.
        let poll = match build.poll(&mut self.runtime, max_work_units) {
            Ok(poll) => poll,
            Err(error) => {
                self.fault_arena_metrics = Some(self.runtime.arena_metrics());
                release_failed_clean_build(&mut self.runtime, build);
                return Err(error.into());
            }
        };
        let work_units = poll.transitions();
        if work_units == 0 || work_units > max_work_units {
            release_failed_clean_build(&mut self.runtime, build);
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "clean build violated its bounded work grant",
            )
            .into());
        }
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            match build.take_session() {
                Some(session) => Ok((ParseState::Ready(Box::new(session)), work_units)),
                None => {
                    release_failed_clean_build(&mut self.runtime, build);
                    Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                        "completed clean build omitted its session",
                    )
                    .into())
                }
            }
        } else {
            Ok((ParseState::Clean(build), work_units))
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
        #[cfg(feature = "opening-session")]
        if matches!(&self.parser, ParseState::Opening(_)) {
            return self.apply_opening_edit(range, replacement);
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
            .or_else(|| self.capture_ready_edit_context(range.start, false))
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

    /// Commits replacement bytes that the ABI already validated and staged.
    ///
    /// Staged v1 intentionally places one collapsed result caret at the end
    /// of the inserted range. This keeps the actor linearization independent
    /// of replacement size: no second scan or document-sized receipt is
    /// needed for the large paste/delete behavior that v1 admits.
    pub fn apply_staged_source_transaction_v1(
        &mut self,
        expected_revision: u64,
        base_byte_range: Range<usize>,
        replacement: &str,
        replacement_utf16_length: usize,
    ) -> Result<DocumentStagedSourceTransactionReceiptV1, DocumentSessionError> {
        let actual_revision = self.revision();
        if expected_revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        if base_byte_range.start > base_byte_range.end {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let base_utf16_range = self.utf16_offset_for_byte(base_byte_range.start)?
            ..self.utf16_offset_for_byte(base_byte_range.end)?;
        let result_source_utf16_length = self
            .source_utf16_len()
            .checked_sub(base_utf16_range.len())
            .and_then(|length| length.checked_add(replacement_utf16_length))
            .ok_or(DocumentSessionError::RangeOutOfBounds)?;
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
        let result_selection_utf16 = result_utf16_range.end;
        let result_selection_byte = result_byte_range.end;
        let edit = self.apply_edit(expected_revision, base_byte_range.clone(), replacement)?;
        Ok(DocumentStagedSourceTransactionReceiptV1 {
            base_revision: expected_revision,
            result_revision: edit.revision,
            base_byte_range,
            base_utf16_range,
            result_byte_range,
            result_utf16_range,
            result_selection_utf16,
            result_selection_byte,
            result_source_byte_length: self.source_byte_len(),
            result_source_utf16_length,
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
        self.try_apply_edit_intent_v1_at_bytes(
            expected_revision,
            intent,
            selection_byte,
            selection_byte,
            composition_active,
        )
    }

    /// Resolves a semantic command at [target_byte] while preserving the
    /// independently anchored selection represented by [selection_byte].
    /// Keyboard intents pass the same byte for both. Selection-independent
    /// actions may differ only when their committed splice is length-neutral.
    pub fn try_apply_edit_intent_v1_at_bytes(
        &mut self,
        expected_revision: u64,
        intent: DocumentEditIntentV1,
        selection_byte: usize,
        target_byte: usize,
        composition_active: bool,
    ) -> Result<DocumentEditIntentReceiptV1, DocumentSessionError> {
        let actual_revision = self.revision();
        if expected_revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        if selection_byte > self.source_byte_len() || target_byte > self.source_byte_len() {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let selection_utf16 = self.utf16_offset_for_byte(selection_byte)?;
        let target_utf16 = self.utf16_offset_for_byte(target_byte)?;
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
                presentation_proven: false,
            });
        }

        let parser_is_ready = matches!(&self.parser, ParseState::Ready(_));
        let context = self
            .edit_context
            .as_ref()
            .filter(|context| {
                context.revision == expected_revision
                    && target_byte >= context.editable_bytes.start
                    && target_byte <= context.editable_bytes.end
                    && (intent != DocumentEditIntentV1::IndentListItem
                        || matches!(
                            context.row,
                            DocumentSimpleEditRow::ListItem {
                                starts_list: true,
                                ..
                            } | DocumentSimpleEditRow::ListItem {
                                indent: Some(_),
                                ..
                            }
                        ))
            })
            .cloned()
            .or_else(|| {
                self.capture_ready_edit_context(
                    target_byte,
                    intent == DocumentEditIntentV1::IndentListItem,
                )
            })
            .or_else(|| {
                (!parser_is_ready)
                    .then(|| self.capture_exact_edit_context(target_byte))
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
                presentation_proven: false,
            });
        };
        let resolved = resolve_document_edit_intent_v1(intent, target_byte, target_utf16, &context);
        let Some(splice) = resolved.splice.clone() else {
            return Ok(DocumentEditIntentReceiptV1 {
                disposition: resolved.disposition,
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
                presentation_proven: false,
            });
        };
        let expected_result_selection_utf16 = transformed_collapsed_selection(
            selection_utf16,
            &splice.base_utf16_range,
            splice.replacement.encode_utf16().count(),
        )
        .ok_or(DocumentSessionError::UnsupportedEditIntentSelection)?;
        if intent != DocumentEditIntentV1::ToggleTaskChecked
            && resolved.result_selection_utf16 != expected_result_selection_utf16
        {
            return Err(DocumentSessionError::UnsupportedEditIntentSelection);
        }
        if intent == DocumentEditIntentV1::ToggleTaskChecked
            && (splice.base_byte_range.len() != splice.replacement.len()
                || splice.base_utf16_range.len() != splice.replacement.encode_utf16().count())
        {
            return Err(DocumentSessionError::UnsupportedEditIntentSelection);
        }
        let result_selection_byte = transformed_collapsed_selection(
            selection_byte,
            &splice.base_byte_range,
            splice.replacement.len(),
        )
        .ok_or(DocumentSessionError::UnsupportedEditIntentSelection)?;

        let semantic_bytes = 32usize
            .checked_add(splice.base_byte_range.len())
            .and_then(|bytes| bytes.checked_add(splice.replacement.len()))
            .ok_or(DocumentSessionError::EditIntentLimitExceeded)?;
        if semantic_bytes > crate::MAX_SMALL_EDIT_BYTES as usize {
            return Err(DocumentSessionError::EditIntentLimitExceeded);
        }
        let presentation_proven =
            parser_is_ready && self.proves_bounded_structural_presentation(&context, &resolved)?;
        // Reuse the context already resolved above. Without this handoff the
        // generic one-splice commit path performs a redundant current-row
        // query before applying the exact semantic splice.
        self.edit_context = Some(context);
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
            result_selection_utf16: expected_result_selection_utf16,
            result_source_byte_length: self.source_byte_len(),
            result_source_utf16_length: self.source_utf16_len(),
            parser_pending: edit.parser_pending,
            presentation_transition: resolved.presentation_transition,
            presentation_proven,
        })
    }

    fn proves_bounded_structural_presentation(
        &self,
        context: &DocumentSimpleEditContext,
        resolved: &ResolvedDocumentEditIntentV1,
    ) -> Result<bool, DocumentSessionError> {
        let Some(splice) = resolved.splice.as_ref() else {
            return Ok(false);
        };
        if !matches!(context.row, DocumentSimpleEditRow::Plain) {
            return Ok(false);
        }
        match resolved.presentation_transition {
            DocumentEditPresentationTransitionV1::SplitParagraph
                if splice.base_byte_range.is_empty()
                    && splice.base_byte_range.start == context.editable_bytes.end
                    && splice.base_utf16_range.start == context.editable_utf16.end =>
            {
                let content = self.source_bytes(context.editable_bytes.clone())?;
                Ok(structural_inline_source_is_bounded(&content)
                    && content
                        .last()
                        .is_some_and(|byte| !matches!(byte, b' ' | b'\t' | b'\\' | b'\r' | b'\n'))
                    && structural_inline_tokens(&content).is_some())
            }
            DocumentEditPresentationTransitionV1::MergeParagraph => {
                let Some(merge) = context.paragraph_merge.as_ref() else {
                    return Ok(false);
                };
                let previous = self.source_bytes(merge.previous_source_bytes.clone())?;
                let current = self.source_bytes(context.editable_bytes.clone())?;
                let previous = trim_structural_line_endings(&previous);
                if previous.is_empty() || current.is_empty() {
                    return Ok(false);
                }
                Ok(structural_inline_partition_is_stable(previous, &current))
            }
            _ => Ok(false),
        }
    }

    fn capture_ready_edit_context(
        &mut self,
        selection_byte: usize,
        include_list_indent: bool,
    ) -> Option<DocumentSimpleEditContext> {
        if selection_byte > self.source_byte_len() {
            return None;
        }
        let ordinary_requested_start = self
            .snapped_to_scalar_boundary(selection_byte.saturating_sub(16))
            .ok()?;
        let list_indent_requested_start = include_list_indent
            .then(|| self.bounded_physical_line_window_start(selection_byte, 32, 4 * 1024))
            .flatten();
        let requested_start = list_indent_requested_start.unwrap_or(ordinary_requested_start);
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
            if list_indent_requested_start.is_some() {
                40
            } else {
                8
            },
        )
        .ok()?;
        let current = viewport
            .rows
            .iter()
            .find(|row| {
                row.editable_range.as_ref().is_some_and(|range| {
                    selection_byte >= range.start as usize && selection_byte <= range.end as usize
                })
            })
            .cloned();
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
                ) || (matches!(context.row, DocumentSimpleEditRow::Plain)
                    && context.editable_bytes.is_empty()
                    && context.paragraph_merge.is_some())
            });
        };
        if matches!(
            current.presentation,
            DocumentViewportRowPresentation::BlockQuote {
                nesting_depth,
                container_count,
                simple_continuation: false,
                ..
            } if nesting_depth == container_count
        ) && current.edit_capability == DocumentViewportRowEditCapability::ProjectedReserved
        {
            return self.capture_projected_block_quote_edit_context(
                &current,
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
                &current,
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
                item_end_byte,
                item_end_utf16,
                nesting_depth,
                marker_offset,
                item_padding,
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
                let task_check = match task_checked {
                    Some(checked) => {
                        Some(self.capture_task_check(&prefix_bytes, &prefix_utf16, checked)?)
                    }
                    None => None,
                };
                let complete_item_is_active_row = item_end_byte == current.source_range.end
                    && item_end_utf16 == current.source_utf16_range.end;
                let indent = (include_list_indent && complete_item_is_active_row)
                    .then(|| {
                        self.capture_list_indent(
                            &viewport.rows,
                            current.ordinal,
                            &prefix_bytes,
                            &prefix_utf16,
                            nesting_depth,
                            marker_offset,
                            marker_column,
                            starts_list,
                        )
                    })
                    .flatten();
                DocumentSimpleEditRow::ListItem {
                    marker,
                    prefix_bytes,
                    prefix_utf16,
                    nesting_depth,
                    marker_offset,
                    item_padding,
                    container_widths,
                    container_count,
                    marker_column,
                    starts_list,
                    task_checked,
                    task_check,
                    empty: current.kind == 14 || editable_bytes.is_empty(),
                    indent,
                    outdent,
                }
            }
            DocumentViewportRowPresentation::BlockQuote {
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                nesting_depth,
                container_widths,
                container_count,
                simple_continuation: true,
            } if container_count == nesting_depth => {
                let prefix_bytes = usize::try_from(prefix_start_byte).ok()?
                    ..usize::try_from(prefix_end_byte).ok()?;
                let prefix_utf16 = usize::try_from(prefix_start_utf16).ok()?
                    ..usize::try_from(prefix_end_utf16).ok()?;
                let prefix_text =
                    String::from_utf8(self.source_bytes(prefix_bytes.clone()).ok()?).ok()?;
                let outdent = self.capture_block_quote_outdent(
                    &prefix_bytes,
                    &prefix_utf16,
                    nesting_depth,
                    container_widths,
                    container_count,
                );
                if nesting_depth > 1 && outdent.is_none() {
                    return None;
                }
                DocumentSimpleEditRow::BlockQuote {
                    prefix_bytes,
                    prefix_utf16,
                    prefix_text,
                    nesting_depth,
                    container_widths,
                    container_count,
                    starts_quote: true,
                    empty: editable_bytes.is_empty(),
                    outdent,
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

    /// Chooses a source window containing at most `maximum_previous_lines`
    /// complete physical predecessors of the caret line. This is source
    /// geometry only; Markdown ancestry still comes exclusively from the
    /// parser-authored rows returned for the window.
    fn bounded_physical_line_window_start(
        &self,
        selection_byte: usize,
        maximum_previous_lines: usize,
        maximum_source_bytes: usize,
    ) -> Option<usize> {
        let window_start = self
            .snapped_to_scalar_boundary(selection_byte.saturating_sub(maximum_source_bytes))
            .ok()?;
        let source = self.source_bytes(window_start..selection_byte).ok()?;
        let mut line_starts = Vec::with_capacity(maximum_previous_lines.saturating_add(1));
        if window_start == 0 {
            line_starts.push(0);
        }
        let mut cursor = 0;
        while cursor < source.len() {
            match source[cursor] {
                b'\r' if source.get(cursor + 1) == Some(&b'\n') => cursor += 2,
                b'\r' | b'\n' => cursor += 1,
                _ => {
                    cursor += 1;
                    continue;
                }
            }
            line_starts.push(window_start + cursor);
        }
        let current_line_start = *line_starts.last()?;
        if current_line_start > selection_byte {
            return None;
        }
        let retained = maximum_previous_lines.saturating_add(1);
        Some(line_starts[line_starts.len().saturating_sub(retained)])
    }

    /// Certifies the one insertion width that makes the current list item a
    /// child of its preceding sibling. `starts_list` is the parser's proof
    /// that no such sibling exists; every other absence means the bounded
    /// ancestry window was insufficient and the command must fail closed.
    #[allow(clippy::too_many_arguments)]
    fn capture_list_indent(
        &self,
        rows: &[DocumentViewportRow],
        current_ordinal: u64,
        prefix_bytes: &Range<usize>,
        prefix_utf16: &Range<usize>,
        nesting_depth: u8,
        marker_offset: u8,
        marker_column: u8,
        starts_list: bool,
    ) -> Option<DocumentListIndent> {
        if starts_list {
            return None;
        }
        let mut preceding_padding = None;
        for row in rows
            .iter()
            .rev()
            .filter(|row| row.ordinal < current_ordinal)
        {
            match row.presentation {
                DocumentViewportRowPresentation::ListItem {
                    nesting_depth: preceding_depth,
                    item_padding,
                    simple_continuation: true,
                    ..
                } if preceding_depth == nesting_depth => {
                    preceding_padding = Some(item_padding);
                    break;
                }
                DocumentViewportRowPresentation::ListItem {
                    nesting_depth: preceding_depth,
                    ..
                } if preceding_depth < nesting_depth => return None,
                _ => {}
            }
        }
        let width = preceding_padding.filter(|width| (2..=14).contains(width))?;
        let container_column = marker_column.checked_sub(marker_offset)?;
        let byte_offset = prefix_bytes
            .start
            .checked_sub(usize::from(container_column))?;
        let utf16_offset = prefix_utf16
            .start
            .checked_sub(usize::from(container_column))?;
        let indentation = self.source_bytes(byte_offset..prefix_bytes.start).ok()?;
        if indentation.len() != usize::from(container_column)
            || indentation.iter().any(|byte| *byte != b' ')
        {
            return None;
        }
        Some(DocumentListIndent {
            byte_offset,
            utf16_offset,
            width,
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
        let prefix_source = self.source_bytes(prefix_bytes.clone()).ok()?;
        let prefix_text = String::from_utf8(prefix_source.clone()).ok()?;
        let nesting_depth = match row.presentation {
            DocumentViewportRowPresentation::BlockQuote {
                nesting_depth,
                container_count,
                ..
            } if container_count == nesting_depth => nesting_depth,
            _ => return None,
        };
        let (container_widths, container_count) =
            m11_block_quote_prefix_lineage(&prefix_source, nesting_depth)?;
        let outdent = self.capture_block_quote_outdent(
            &prefix_bytes,
            &prefix_utf16,
            nesting_depth,
            container_widths,
            container_count,
        );
        if nesting_depth > 1 && outdent.is_none() {
            return None;
        }

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
                nesting_depth,
                container_widths,
                container_count,
                starts_quote: segment_index == 0,
                empty: editable_start_utf16 == editable_end_utf16,
                outdent,
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

    fn capture_block_quote_outdent(
        &self,
        prefix_bytes: &Range<usize>,
        prefix_utf16: &Range<usize>,
        nesting_depth: u8,
        container_widths: u64,
        container_count: u8,
    ) -> Option<DocumentBlockQuoteOutdent> {
        if nesting_depth <= 1 || container_count != nesting_depth || container_count > 16 {
            return None;
        }
        let shift = u32::from(container_count - 1) * 4;
        let width = usize::try_from((container_widths >> shift) & 0x0f).ok()?;
        if !(1..=15).contains(&width) || width > prefix_bytes.len() || width > prefix_utf16.len() {
            return None;
        }
        Some(DocumentBlockQuoteOutdent {
            bytes: prefix_bytes.end.checked_sub(width)?..prefix_bytes.end,
            utf16: prefix_utf16.end.checked_sub(width)?..prefix_utf16.end,
        })
    }

    fn capture_task_check(
        &self,
        prefix_bytes: &Range<usize>,
        prefix_utf16: &Range<usize>,
        checked: bool,
    ) -> Option<DocumentTaskCheck> {
        if prefix_bytes.len() != prefix_utf16.len() || prefix_bytes.len() > 64 {
            return None;
        }
        let prefix = self.source_bytes(prefix_bytes.clone()).ok()?;
        let marker_start = prefix.windows(3).rposition(|window| {
            window[0] == b'['
                && window[2] == b']'
                && if checked {
                    matches!(window[1], b'x' | b'X')
                } else {
                    window[1] == b' '
                }
        })?;
        if !prefix[marker_start + 3..]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
        {
            return None;
        }
        let byte_start = prefix_bytes.start.checked_add(marker_start + 1)?;
        let utf16_start = prefix_utf16.start.checked_add(marker_start + 1)?;
        Some(DocumentTaskCheck {
            bytes: byte_start..byte_start + 1,
            utf16: utf16_start..utf16_start + 1,
            checked,
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
                item_padding,
                task_checked,
                empty,
            } => {
                let starts_list =
                    exact_simple_list_starts_list(&window, window_start, local_line_start, marker);
                let prefix_bytes = line_start + prefix.start..line_start + prefix.end;
                let prefix_utf16 = lease.utf16_offset_for_byte(prefix_bytes.start).ok()?
                    ..lease.utf16_offset_for_byte(prefix_bytes.end).ok()?;
                let editable = line_start + content.start..line_start + content.end;
                let task_check = match task_checked {
                    Some(checked) => {
                        Some(self.capture_task_check(&prefix_bytes, &prefix_utf16, checked)?)
                    }
                    None => None,
                };
                (
                    DocumentSimpleEditRow::ListItem {
                        marker: document_marker_from_parser(marker),
                        prefix_bytes,
                        prefix_utf16,
                        nesting_depth: 1,
                        marker_offset,
                        item_padding,
                        container_widths: 0,
                        container_count: 0,
                        marker_column: marker_offset,
                        starts_list,
                        task_checked,
                        task_check,
                        empty,
                        indent: None,
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
                let prefix_source = prefix_text.as_bytes();
                let (container_widths, container_count) =
                    m11_block_quote_prefix_lineage(prefix_source, 1)?;
                (
                    DocumentSimpleEditRow::BlockQuote {
                        prefix_bytes,
                        prefix_utf16,
                        prefix_text,
                        nesting_depth: 1,
                        container_widths,
                        container_count,
                        starts_quote: true,
                        empty,
                        outdent: None,
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
            .then(|| {
                exact_plain_paragraph_merge(&lease, &window, window_start, local_line_start)
                    .or_else(|| {
                        editable_bytes.is_empty().then(|| {
                            exact_empty_plain_backspace(
                                &lease,
                                &window,
                                window_start,
                                local_line_start,
                            )
                        })?
                    })
            })
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
            restore_context: None,
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
            #[cfg(feature = "opening-session")]
            ParseState::Opening(state) => {
                return query_opening_viewport(
                    &mut self.runtime,
                    state,
                    revision,
                    requested_range,
                    maximum_rows,
                )
            }
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

    /// Resolves one exact parser-authored link or image fact at the current
    /// revision. This work is bounded to one inline leaf and is intentionally
    /// performed on activation rather than on every viewport query.
    pub fn query_semantic_target(
        &mut self,
        revision: u64,
        source_range: Range<usize>,
    ) -> Result<Option<DocumentSemanticTarget>, DocumentSessionError> {
        let actual_revision = self.revision();
        if revision != actual_revision {
            return Err(DocumentSessionError::StaleRevision {
                expected: revision,
                actual: actual_revision,
            });
        }
        if source_range.start >= source_range.end || source_range.end > self.source_byte_len() {
            return Err(DocumentSessionError::RangeOutOfBounds);
        }
        let session = match &self.parser {
            ParseState::Ready(session) => session,
            ParseState::Faulted => return Err(DocumentSessionError::Faulted),
            _ => return Err(DocumentSessionError::NotReady),
        };
        query_session_semantic_target(&mut self.runtime, session, source_range)
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
            #[cfg(feature = "opening-session")]
            ParseState::Opening(state) => opening_live_viewport(
                &self.runtime,
                state,
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
            #[cfg(feature = "opening-session")]
            ParseState::Opening(mut state) => {
                // The open session's release drains its cancel and viewport
                // loops synchronously; the store simply drops, because the
                // replica never outlives its authority.
                state.session.release(&mut self.runtime);
                drop(state);
                self.runtime.begin_close()?;
                ParseState::ClosingRuntime
            }
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
        restore_context: None,
    })
}

fn exact_empty_plain_backspace(
    lease: &SourceSnapshotLease,
    window: &[u8],
    window_start: usize,
    current_line_start: usize,
) -> Option<DocumentParagraphMerge> {
    let separator_start = previous_line_ending_start(window, current_line_start)?;
    let previous_line_start = window[..separator_start]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |index| index + 1);
    if previous_line_start == 0 && window_start != 0 {
        return None;
    }
    if !matches!(
        classify_m11_simple_edit_line(
            &window[previous_line_start..separator_start],
            window_start + previous_line_start == 0,
        )
        .kind,
        M11SimpleEditLineKind::Plain
    ) {
        return None;
    }
    let previous_source_bytes =
        window_start + previous_line_start..window_start + current_line_start;
    let separator_bytes = window_start + separator_start..window_start + current_line_start;
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
        restore_context: None,
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

fn trim_structural_line_endings(mut source: &[u8]) -> &[u8] {
    while source
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
    {
        source = &source[..source.len() - 1];
    }
    source
}

fn structural_inline_source_is_bounded(source: &[u8]) -> bool {
    !source.is_empty()
        && source.len() <= M11_SIMPLE_EDIT_LINE_MAX_BYTES
        && source.iter().all(|byte| {
            byte.is_ascii()
                && !matches!(
                    byte,
                    b'[' | b']' | b'<' | b'>' | b'\\' | b'&' | b'`' | b'_' | b'~' | b'\r' | b'\n'
                )
        })
}

fn structural_inline_partition_is_stable(left: &[u8], right: &[u8]) -> bool {
    if left.len().saturating_add(right.len()) > M11_SIMPLE_EDIT_LINE_MAX_BYTES
        || !structural_inline_source_is_bounded(left)
        || !structural_inline_source_is_bounded(right)
    {
        return false;
    }
    let Some(mut partitioned) = structural_inline_tokens(left) else {
        return false;
    };
    let Some(right_tokens) = structural_inline_tokens(right) else {
        return false;
    };
    append_structural_tokens(&mut partitioned, right_tokens);
    let mut merged = Vec::with_capacity(left.len() + right.len());
    merged.extend_from_slice(left);
    merged.extend_from_slice(right);
    structural_inline_tokens(&merged).is_some_and(|tokens| tokens == partitioned)
}

fn structural_inline_tokens(source: &[u8]) -> Option<Vec<StructuralInlineProofToken>> {
    if !structural_inline_source_is_bounded(source) {
        return None;
    }
    let source = std::str::from_utf8(source).ok()?;
    let nodes = project_m11_gfm_inline(
        source,
        M11GfmInlineOptions {
            strikethrough: true,
            autolink: true,
        },
        &[],
    )
    .ok()?;
    let mut tokens = Vec::new();
    append_structural_inline_nodes(&mut tokens, &nodes);
    Some(tokens)
}

fn append_structural_tokens(
    target: &mut Vec<StructuralInlineProofToken>,
    tokens: Vec<StructuralInlineProofToken>,
) {
    for token in tokens {
        push_structural_token(target, token);
    }
}

fn push_structural_token(
    target: &mut Vec<StructuralInlineProofToken>,
    token: StructuralInlineProofToken,
) {
    if let StructuralInlineProofToken::Text(text) = token {
        if let Some(StructuralInlineProofToken::Text(previous)) = target.last_mut() {
            previous.push_str(&text);
        } else {
            target.push(StructuralInlineProofToken::Text(text));
        }
    } else {
        target.push(token);
    }
}

fn append_structural_inline_nodes(
    target: &mut Vec<StructuralInlineProofToken>,
    nodes: &[M11GfmInlineNode],
) {
    for node in nodes {
        match node {
            M11GfmInlineNode::Text(text) => {
                push_structural_token(target, StructuralInlineProofToken::Text(text.clone()));
            }
            M11GfmInlineNode::SoftBreak => {
                target.push(StructuralInlineProofToken::SoftBreak);
            }
            M11GfmInlineNode::LineBreak => {
                target.push(StructuralInlineProofToken::HardBreak);
            }
            M11GfmInlineNode::Transparent(children) => {
                append_structural_inline_nodes(target, children);
            }
            M11GfmInlineNode::Emphasis(children) => {
                append_structural_container(target, 1, "", "", children);
            }
            M11GfmInlineNode::Strong(children) => {
                append_structural_container(target, 2, "", "", children);
            }
            M11GfmInlineNode::Code(text) => {
                target.push(StructuralInlineProofToken::Enter(
                    3,
                    String::new(),
                    String::new(),
                ));
                push_structural_token(target, StructuralInlineProofToken::Text(text.clone()));
                target.push(StructuralInlineProofToken::Exit(3));
            }
            M11GfmInlineNode::Strikethrough(children) => {
                append_structural_container(target, 4, "", "", children);
            }
            M11GfmInlineNode::Link {
                destination,
                title,
                children,
            } => append_structural_container(target, 5, destination, title, children),
            M11GfmInlineNode::Image {
                destination,
                title,
                children,
            } => append_structural_container(target, 6, destination, title, children),
            M11GfmInlineNode::Html(text) => {
                target.push(StructuralInlineProofToken::Enter(
                    7,
                    String::new(),
                    String::new(),
                ));
                push_structural_token(target, StructuralInlineProofToken::Text(text.clone()));
                target.push(StructuralInlineProofToken::Exit(7));
            }
        }
    }
}

fn append_structural_container(
    target: &mut Vec<StructuralInlineProofToken>,
    kind: u8,
    first: &str,
    second: &str,
    children: &[M11GfmInlineNode],
) {
    target.push(StructuralInlineProofToken::Enter(
        kind,
        first.to_owned(),
        second.to_owned(),
    ));
    append_structural_inline_nodes(target, children);
    target.push(StructuralInlineProofToken::Exit(kind));
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

    fn ready_viewport_rows(
        source: &str,
        requested_range: Range<usize>,
        maximum_rows: u32,
    ) -> Vec<DocumentViewportRow> {
        let mut document = DocumentSession::begin(source).expect("begin viewport oracle");
        while document.pump(512).expect("pump viewport oracle").phase != DocumentSessionPhase::Ready
        {
        }
        let rows = document
            .query_viewport(document.revision(), requested_range, maximum_rows)
            .expect("query viewport oracle")
            .rows;
        document.close().expect("close viewport oracle");
        rows
    }

    #[test]
    fn closed_prefix_rows_match_complete_public_viewport_output() {
        let prefix = (0..40)
            .map(|index| {
                format!(
                    "Paragraph {index} has **strong**, _emphasis_, `code`, and a [direct link](https://example.invalid/{index}).\n\n"
                )
            })
            .collect::<String>();
        let suffix = (40..80)
            .map(|index| format!("Later paragraph {index} cannot change the prefix.\n\n"))
            .collect::<String>();
        let full = format!("{prefix}{suffix}");

        let prefix_rows = ready_viewport_rows(&prefix, 0..prefix.len(), 24);
        let full_rows = ready_viewport_rows(&full, 0..prefix.len(), 24);

        assert_eq!(prefix_rows.len(), 24);
        assert_eq!(full_rows, prefix_rows);
    }

    #[test]
    fn closed_block_is_not_by_itself_a_publication_proof() {
        let cases = [
            ("[later]\n\n", "[later]: /resolved\n"),
            ("| heading |\n", "| --- |\n| cell |\n"),
            ("setext candidate\n", "---\n"),
        ];

        for (prefix, suffix) in cases {
            let full = format!("{prefix}{suffix}");
            let prefix_rows = ready_viewport_rows(prefix, 0..prefix.len(), 8);
            let full_rows = ready_viewport_rows(&full, 0..prefix.len(), 8);
            assert_ne!(
                full_rows.first(),
                prefix_rows.first(),
                "later source must be visible to the publication classifier for {prefix:?}"
            );
        }
    }

    #[test]
    fn unsupported_inline_projection_is_neutral_not_authoritative_empty() {
        let plain = "ordinary text\n";
        let plain_rows = ready_viewport_rows(plain, 0..plain.len(), 1);
        assert_eq!(
            plain_rows[0].inline_facts,
            Some(Vec::new()),
            "an exhaustively classified plain leaf is authoritative"
        );

        let unsupported = "ordinary <span>inline HTML</span> text\n";
        let unsupported_rows = ready_viewport_rows(unsupported, 0..unsupported.len(), 1);
        assert_eq!(
            unsupported_rows[0].inline_facts, None,
            "a failed-closed inline leaf must render from exact source"
        );
    }

    #[test]
    fn reference_dependent_slice_fails_closed_without_final_winners() {
        let source = "[later]\n\n[later]: /resolved\n";
        let mut document = DocumentSession::begin(source).expect("begin reference document");
        while document.pump(512).expect("pump reference document").phase
            != DocumentSessionPhase::Ready
        {}

        let prepared = match &document.parser {
            ParseState::Ready(session) => session
                .prepare_inline_leaf(
                    &document.runtime,
                    M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
                )
                .expect("prepare reference-shaped leaf"),
            _ => panic!("reference document must be ready"),
        };
        let parser_profile =
            ParserProfileId::new(u64::from(SYNTAX_PROFILE_GFM_V1)).expect("GFM profile identity");
        assert!(
            capture_prepared_inline_projection(
                &mut document.runtime,
                prepared,
                0..7,
                0..7,
                parser_profile,
                None,
            )
            .expect("capture reference-shaped slice")
            .is_none(),
            "a progressive slice without final reference winners must remain neutral"
        );

        let viewport = document
            .query_viewport(document.revision(), 0..7, 1)
            .expect("query final reference row");
        assert!(viewport.rows[0]
            .inline_facts
            .as_ref()
            .is_some_and(|facts| facts
                .iter()
                .any(|fact| fact.kind == DocumentInlineFactKind::ReferenceLink)));
        document.close().expect("close reference document");
    }

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
    fn slice_style_mapping_matches_public_rows_tables_and_activated_targets() {
        let source = concat!(
            "# Mixed viewport\n\n",
            "Paragraph with **strong**, &amp;, [direct](https://example.com/path \"title\"), and <me@example.com>.\n\n",
            "| f\\|oo | bar |\n",
            "| :--- | ---: |\n",
            "| `x\\|y` | baz |\n\n",
            "- [x] task\n",
        );
        let mut document = DocumentSession::begin(source).expect("begin mixed viewport");
        while document.pump(512).expect("pump mixed viewport").phase != DocumentSessionPhase::Ready
        {
        }
        let revision = document.revision();
        let oracle = document
            .query_viewport(revision, 0..source.len(), 16)
            .expect("query complete public viewport");

        let rows = {
            let session = match &document.parser {
                ParseState::Ready(session) => session,
                _ => panic!("mixed viewport must be ready"),
            };
            let limits = row_query_limits(16).expect("mixed viewport limits");
            let outcome = session
                .query_renderable_rows_bounded(
                    &document.runtime,
                    M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
                    source.len() as u64,
                    limits,
                )
                .expect("query mixed Green rows");
            match outcome {
                M11RecursiveGreenRowQueryOutcome::Window(window) => window.rows().to_vec(),
                M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
                    panic!("mixed viewport query exceeded its frozen budget")
                }
            }
        };

        let mapped = {
            let session = match &document.parser {
                ParseState::Ready(session) => session,
                _ => panic!("mixed viewport must be ready"),
            };
            map_document_viewport_rows(&mut document.runtime, &rows, |runtime, row| {
                document_inline_facts_without_reference_authority(runtime, session, row)
            })
            .expect("map slice-style public rows")
        };
        assert_eq!(mapped, oracle.rows);
        assert!(mapped
            .iter()
            .any(|row| row.presentation == DocumentViewportRowPresentation::Table));
        assert!(mapped.iter().any(|row| {
            row.inline_facts.as_ref().is_some_and(|facts| {
                facts
                    .iter()
                    .any(|fact| fact.kind == DocumentInlineFactKind::TableCell)
            })
        }));

        let direct_start = source.find("[direct]").expect("direct link start");
        let direct_end = source[direct_start..]
            .find(')')
            .map(|offset| direct_start + offset + 1)
            .expect("direct link end");
        let requested = direct_start as u64..direct_end as u64;
        let slice_target = {
            let row = rows
                .iter()
                .find(|row| {
                    let physical = row.physical_range();
                    physical.start <= requested.start && physical.end >= requested.end
                })
                .expect("direct link row");
            let session = match &document.parser {
                ParseState::Ready(session) => session,
                _ => panic!("mixed viewport must be ready"),
            };
            let captured = capture_document_inline_projection_without_reference_authority(
                &mut document.runtime,
                session,
                row,
            )
            .expect("capture slice-style direct target")
            .expect("direct target facts are authoritative");
            map_document_semantic_target(&document.runtime, captured, requested.clone())
                .expect("map slice-style direct target")
        };
        let oracle_target = document
            .query_semantic_target(revision, direct_start..direct_end)
            .expect("query complete direct target");
        assert_eq!(slice_target, oracle_target);
        assert_eq!(
            slice_target
                .as_ref()
                .map(|target| target.destination.as_str()),
            Some("https://example.com/path")
        );

        document.close().expect("close mixed viewport");
    }

    #[test]
    fn compact_slice_matches_complete_product_rows_and_reference_target() {
        let mut source = String::from(concat!(
            "# Mixed cold viewport\n\n",
            "Paragraph with **strong**, &amp;, and [direct](https://example.com/path \"title\").\n\n",
            "| f\\|oo | bar |\n",
            "| :--- | ---: |\n",
            "| `x\\|y` | baz |\n\n",
            "- [x] task\n\n",
            "> quoted row\n\n",
            "Forward [late] and [missing].\n\n",
        ));
        for index in 0..48 {
            source.push_str(&format!(
                "Tail paragraph {index:02} keeps the reference definition beyond the first slice.\n\n"
            ));
        }
        let definition_start = source.len();
        source.push_str("[late]: /resolved \"late title\"\n");

        let mut compact_runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("compact product runtime");
        let mut compact =
            flark_parser::build_m11_compact_first_viewport_probe(&mut compact_runtime, 1)
                .expect("build compact first viewport");
        let slice_range = compact.root().source_range();
        assert_eq!(slice_range.start, 0);
        assert!(
            slice_range.end < definition_start as u64,
            "final reference authority must come from outside the cold slice"
        );
        let limits = row_query_limits(64).expect("compact product limits");
        let outcome = compact
            .root()
            .locate_renderable_rows_bounded(
                &compact_runtime,
                M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
                slice_range.end,
                limits,
            )
            .expect("query compact product rows");
        let compact_rows = match outcome {
            M11RecursiveGreenRowQueryOutcome::Window(window) => window.rows().to_vec(),
            M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
                panic!("compact product query exceeded its frozen budget")
            }
        };
        let mapped =
            map_document_viewport_rows(&mut compact_runtime, &compact_rows, |runtime, row| {
                document_inline_facts_from_compact_probe(runtime, &compact, row)
            })
            .expect("map compact product rows");

        let mut oracle = DocumentSession::begin(&source).expect("begin product oracle");
        while oracle.pump(512).expect("pump product oracle").phase != DocumentSessionPhase::Ready {}
        let oracle_rows = oracle
            .query_viewport(
                oracle.revision(),
                0..usize::try_from(slice_range.end).expect("slice end fits usize"),
                64,
            )
            .expect("query complete product oracle")
            .rows;
        assert_eq!(mapped, oracle_rows);
        assert_eq!(mapped.len(), 32);
        assert!(mapped
            .iter()
            .any(|row| row.presentation == DocumentViewportRowPresentation::Table));

        let late_start = source.find("[late]").expect("late reference start");
        let late_end = late_start + "[late]".len();
        let requested = late_start as u64..late_end as u64;
        let compact_target = {
            let row = compact_rows
                .iter()
                .find(|row| {
                    let physical = row.physical_range();
                    physical.start <= requested.start && physical.end >= requested.end
                })
                .expect("late reference row");
            let captured = capture_document_inline_projection_from_compact_probe(
                &mut compact_runtime,
                &compact,
                row,
            )
            .expect("capture compact target")
            .expect("compact reference facts are authoritative");
            map_document_semantic_target(&compact_runtime, captured, requested)
                .expect("map compact target")
        };
        let oracle_target = oracle
            .query_semantic_target(oracle.revision(), late_start..late_end)
            .expect("query oracle target");
        assert_eq!(compact_target, oracle_target);
        assert_eq!(
            compact_target
                .as_ref()
                .map(|target| (target.destination.as_str(), target.title.as_deref())),
            Some(("/resolved", Some("late title")))
        );

        oracle.close().expect("close product oracle");
        compact
            .begin_release(&mut compact_runtime)
            .expect("begin compact product release");
        while !compact
            .poll_release(&mut compact_runtime, 256)
            .expect("poll compact product release")
        {}
        compact_runtime
            .begin_close()
            .expect("begin compact runtime close");
        while !compact_runtime
            .poll_close(256)
            .expect("poll compact runtime close")
            .complete
        {}
    }

    #[test]
    fn semantic_targets_are_parser_cooked_and_resolved_on_demand() {
        let source = "[direct](https://example.com \"title\") <me@example.com> www.example.com ![alt][img]\n\n[img]: /asset.png 'cap'\n";
        let mut document = DocumentSession::begin(source).expect("begin target document");
        while document.pump(512).expect("pump targets").phase != DocumentSessionPhase::Ready {}
        let revision = document.revision();

        let direct_start = source.find("[direct]").expect("direct start");
        let direct_end = source[direct_start..]
            .find(')')
            .map(|offset| direct_start + offset + 1)
            .expect("direct end");
        let direct = document
            .query_semantic_target(revision, direct_start..direct_end)
            .expect("query direct")
            .expect("direct target");
        assert_eq!(direct.kind, DocumentSemanticTargetKind::Link);
        assert_eq!(direct.syntax, DocumentSemanticTargetSyntax::Direct);
        assert_eq!(direct.destination, "https://example.com");
        assert_eq!(direct.title.as_deref(), Some("title"));
        assert_eq!(
            &source[direct.destination_source_range.start as usize
                ..direct.destination_source_range.end as usize],
            "https://example.com"
        );

        let email_start = source.find("<me@example.com>").expect("email start");
        let email_end = email_start + "<me@example.com>".len();
        let email = document
            .query_semantic_target(revision, email_start..email_end)
            .expect("query email")
            .expect("email target");
        assert_eq!(email.syntax, DocumentSemanticTargetSyntax::AutolinkEmail);
        assert_eq!(email.destination, "mailto:me@example.com");

        let www_start = source.find("www.example.com").expect("www start");
        let www_end = www_start + "www.example.com".len();
        let www = document
            .query_semantic_target(revision, www_start..www_end)
            .expect("query www")
            .expect("www target");
        assert_eq!(www.syntax, DocumentSemanticTargetSyntax::AutolinkUri);
        assert_eq!(www.destination, "http://www.example.com");

        let image_start = source.find("![alt][img]").expect("image start");
        let image_end = image_start + "![alt][img]".len();
        let image = document
            .query_semantic_target(revision, image_start..image_end)
            .expect("query image")
            .expect("image target");
        assert_eq!(image.kind, DocumentSemanticTargetKind::Image);
        assert_eq!(image.syntax, DocumentSemanticTargetSyntax::Reference);
        assert_eq!(image.destination, "/asset.png");
        assert_eq!(image.title.as_deref(), Some("cap"));
        assert_eq!(
            &source[image.destination_source_range.start as usize
                ..image.destination_source_range.end as usize],
            "/asset.png"
        );

        document.close().expect("close target document");
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
    let rows = map_document_viewport_rows(runtime, window.rows(), |runtime, row| {
        document_inline_facts(runtime, session, row)
    })?;
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

fn map_document_viewport_rows(
    runtime: &mut DocumentRuntime,
    rows: &[M11RecursiveGreenRenderableRow],
    mut inline_projection: impl FnMut(
        &mut DocumentRuntime,
        &M11RecursiveGreenRenderableRow,
    ) -> Result<
        Option<DocumentInlineProjectionAuthority>,
        DocumentSessionError,
    >,
) -> Result<Vec<DocumentViewportRow>, DocumentSessionError> {
    rows.iter()
        .map(|row| {
            let inline_projection = inline_projection(runtime, row)?;
            document_viewport_row_with_inline_facts(runtime, row, inline_projection)
        })
        .collect()
}

fn query_session_semantic_target(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    source_range: Range<usize>,
) -> Result<Option<DocumentSemanticTarget>, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let utf16 = lease.utf16_offset_for_byte(source_range.start)?;
    let limits = row_query_limits(1).ok_or(DocumentSessionError::RangeOutOfBounds)?;
    let outcome = session.query_renderable_rows_bounded(
        runtime,
        M11RecursiveGreenPoint::new(source_range.start, utf16, SourceBoundaryAffinity::After),
        source_range.end as u64,
        limits,
    )?;
    let window = match outcome {
        M11RecursiveGreenRowQueryOutcome::Window(window) => window,
        M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
            return Err(DocumentSessionError::QueryBudgetExceeded)
        }
    };
    let requested = source_range.start as u64..source_range.end as u64;
    let Some(row) = window.rows().iter().find(|row| {
        let physical = row.physical_range();
        physical.start <= requested.start && physical.end >= requested.end
    }) else {
        return Ok(None);
    };
    let Some(captured) = capture_document_inline_projection(runtime, session, row)? else {
        return Ok(None);
    };
    map_document_semantic_target(runtime, captured, requested)
}

fn map_document_semantic_target(
    runtime: &DocumentRuntime,
    captured: CapturedDocumentInlineProjection,
    requested: Range<u64>,
) -> Result<Option<DocumentSemanticTarget>, DocumentSessionError> {
    let Some((ordinal, fact)) = captured.facts.iter().enumerate().find(|(_, fact)| {
        absolute_inline_range(captured.inline_source.start, fact.relative_range())
            .is_ok_and(|range| range == requested)
    }) else {
        return Ok(None);
    };
    let (kind, syntax) = match fact.kind() {
        M11InlineProjectionKind::AutolinkUri => (
            DocumentSemanticTargetKind::Link,
            DocumentSemanticTargetSyntax::AutolinkUri,
        ),
        M11InlineProjectionKind::AutolinkEmail => (
            DocumentSemanticTargetKind::Link,
            DocumentSemanticTargetSyntax::AutolinkEmail,
        ),
        M11InlineProjectionKind::DirectLink => (
            DocumentSemanticTargetKind::Link,
            DocumentSemanticTargetSyntax::Direct,
        ),
        M11InlineProjectionKind::DirectImage => (
            DocumentSemanticTargetKind::Image,
            DocumentSemanticTargetSyntax::Direct,
        ),
        M11InlineProjectionKind::ReferenceLink => (
            DocumentSemanticTargetKind::Link,
            DocumentSemanticTargetSyntax::Reference,
        ),
        M11InlineProjectionKind::ReferenceImage => (
            DocumentSemanticTargetKind::Image,
            DocumentSemanticTargetSyntax::Reference,
        ),
        _ => return Ok(None),
    };
    let source_range = requested;
    let content_range =
        absolute_inline_range(captured.inline_source.start, fact.relative_content_range())?;
    let (destination_source_range, title_source_range, destination, title) = match syntax {
        DocumentSemanticTargetSyntax::AutolinkUri | DocumentSemanticTargetSyntax::AutolinkEmail => {
            let visible = read_utf8_source_range(runtime, &content_range)?;
            let destination = match syntax {
                DocumentSemanticTargetSyntax::AutolinkUri
                    if fact.flags() & M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW != 0 =>
                {
                    format!("http://{visible}")
                }
                DocumentSemanticTargetSyntax::AutolinkEmail => format!("mailto:{visible}"),
                _ => visible,
            };
            (content_range.clone(), None, destination, None)
        }
        DocumentSemanticTargetSyntax::Direct | DocumentSemanticTargetSyntax::Reference => {
            let value = captured
                .link_values
                .iter()
                .find(|value| value.parent_fact_ordinal() as usize == ordinal)
                .ok_or(DocumentSessionError::Faulted)?;
            let direct = syntax == DocumentSemanticTargetSyntax::Direct;
            let destination_source_range = if direct {
                absolute_inline_range(
                    captured.inline_source.start,
                    value.destination_source_range().clone(),
                )?
            } else {
                u64::from(value.destination_source_range().start)
                    ..u64::from(value.destination_source_range().end)
            };
            let title_source_range = value
                .title_source_range()
                .map(|range| {
                    if direct {
                        absolute_inline_range(captured.inline_source.start, range.clone())
                    } else {
                        Ok(u64::from(range.start)..u64::from(range.end))
                    }
                })
                .transpose()?;
            (
                destination_source_range,
                title_source_range,
                value.cooked_destination().to_owned(),
                value.cooked_title().map(str::to_owned),
            )
        }
    };
    let lease = runtime.snapshot_current_source()?;
    Ok(Some(DocumentSemanticTarget {
        kind,
        syntax,
        source_utf16_range: source_utf16_range(&lease, &source_range)?,
        content_utf16_range: source_utf16_range(&lease, &content_range)?,
        destination_source_utf16_range: source_utf16_range(&lease, &destination_source_range)?,
        title_source_utf16_range: title_source_range
            .as_ref()
            .map(|range| source_utf16_range(&lease, range))
            .transpose()?,
        source_range,
        content_range,
        destination_source_range,
        title_source_range,
        destination,
        title,
    }))
}

fn read_utf8_source_range(
    runtime: &DocumentRuntime,
    range: &Range<u64>,
) -> Result<String, DocumentSessionError> {
    let start = usize::try_from(range.start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let end = usize::try_from(range.end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let mut bytes = vec![0_u8; end.saturating_sub(start)];
    if runtime.read_current_source_window(start..end, &mut bytes)? != bytes.len() {
        return Err(DocumentSessionError::RangeOutOfBounds);
    }
    String::from_utf8(bytes).map_err(|_| DocumentSessionError::Faulted)
}

fn document_viewport_row_with_inline_facts(
    runtime: &mut DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
    inline_projection: Option<DocumentInlineProjectionAuthority>,
) -> Result<DocumentViewportRow, DocumentSessionError> {
    let (inline_facts, parser_edit_cells) = inline_projection.map_or_else(
        || (None, Vec::new()),
        |projection| {
            (
                Some(projection.inline_facts),
                projection.projection_edit_cells,
            )
        },
    );
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
            item_end_byte,
            item_end_utf16,
            nesting_depth,
            marker_offset,
            item_padding,
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
            item_end_byte,
            item_end_utf16,
            nesting_depth,
            marker_offset,
            item_padding,
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
            container_widths,
            container_count,
            simple_continuation,
        } => DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            container_widths,
            container_count,
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
                    nesting_depth,
                    container_count,
                    ..
                } if container_count == nesting_depth
            ) && !matches!(
                presentation,
                DocumentViewportRowPresentation::CodeBlock {
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
    let projection_edit_cells = if projection_segments.is_none() {
        let mut cells = document_projection_edit_cells(
            runtime,
            row,
            presentation,
            &source_range,
            &source_utf16_range,
            inline_facts.as_deref(),
            editable_range.as_ref(),
            editable_utf16_range.as_ref(),
            edit_capability,
        )?;
        if presentation == DocumentViewportRowPresentation::Plain {
            cells.extend(parser_edit_cells);
        }
        cells
    } else {
        Vec::new()
    };
    let replaces_whole_heading_envelopes = matches!(
        presentation,
        DocumentViewportRowPresentation::Heading {
            style: DocumentHeadingStyle::Atx,
            ..
        }
    ) && projection_edit_cells
        .iter()
        .any(|cell| cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS);
    let mut literal_safe_envelopes = if replaces_whole_heading_envelopes {
        Vec::new()
    } else if matches!(
        edit_capability,
        DocumentViewportRowEditCapability::Contiguous
            | DocumentViewportRowEditCapability::ProjectedReserved
    ) && !matches!(
        presentation,
        DocumentViewportRowPresentation::ThematicBreak | DocumentViewportRowPresentation::Table
    ) {
        document_literal_safe_envelopes(
            runtime,
            presentation,
            &source_range,
            inline_facts.as_deref(),
            editable_range.as_ref(),
            editable_utf16_range.as_ref(),
        )?
    } else {
        Vec::new()
    };
    for cell in projection_edit_cells
        .iter()
        .filter(|cell| cell.flags == DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS)
    {
        if literal_safe_envelopes.len() >= VIEWPORT_LITERAL_SAFE_ENVELOPES_PER_ROW_MAX {
            break;
        }
        let (Some(byte_start), Some(byte_end), Some(utf16_start), Some(utf16_end)) = (
            cell.source_range.start.checked_add(2),
            cell.source_range.end.checked_sub(2),
            cell.source_utf16_range.start.checked_add(2),
            cell.source_utf16_range.end.checked_sub(2),
        ) else {
            continue;
        };
        if byte_start >= byte_end || utf16_start >= utf16_end {
            continue;
        }
        literal_safe_envelopes.push(DocumentLiteralSafeEnvelope {
            edit_class: DocumentLiteralEditClass::SingleAsciiAsteriskInsertion,
            source_range: byte_start..byte_end,
            source_utf16_range: utf16_start..utf16_end,
        });
    }
    Ok(DocumentViewportRow {
        ordinal: row.ordinal(),
        kind: row.kind().get(),
        source_range,
        source_utf16_range,
        editable_range,
        editable_utf16_range,
        edit_capability,
        presentation,
        inline_facts,
        literal_safe_envelopes,
        projection_edit_cells,
        projection_segments,
        path_depth: u32::try_from(row.path().len()).unwrap_or(u32::MAX),
    })
}

#[allow(clippy::too_many_arguments)]
fn document_projection_edit_cells(
    runtime: &DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
    presentation: DocumentViewportRowPresentation,
    row_source_range: &Range<u64>,
    row_source_utf16_range: &Range<u64>,
    facts: Option<&[DocumentInlineFact]>,
    editable_range: Option<&Range<u64>>,
    editable_utf16_range: Option<&Range<u64>>,
    edit_capability: DocumentViewportRowEditCapability,
) -> Result<Vec<DocumentProjectionEditCell>, DocumentSessionError> {
    let (Some(editable), Some(editable_utf16), Some(facts)) =
        (editable_range, editable_utf16_range, facts)
    else {
        return Ok(Vec::new());
    };
    let physical_line_start = match presentation {
        DocumentViewportRowPresentation::ListItem {
            prefix_start_byte, ..
        }
        | DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte, ..
        } => prefix_start_byte,
        _ => row_source_range.start,
    };
    let canonical_path_depth = match presentation {
        DocumentViewportRowPresentation::ListItem {
            nesting_depth: 1, ..
        } => row.path().len() == 4,
        DocumentViewportRowPresentation::BlockQuote {
            nesting_depth: 1, ..
        } => row.path().len() == 3,
        _ => (1..=2).contains(&row.path().len()),
    };
    if edit_capability != DocumentViewportRowEditCapability::Contiguous
        || !canonical_path_depth
        || !document_row_starts_at_physical_line_start(runtime, physical_line_start)
        || editable.start > editable.end
        || editable_utf16.start > editable_utf16.end
        || editable.start < row_source_range.start
        || editable.end > row_source_range.end
        || editable_utf16.start < row_source_utf16_range.start
        || editable_utf16.end > row_source_utf16_range.end
    {
        return Ok(Vec::new());
    }

    let Some(row_byte_len) = row_source_range.end.checked_sub(row_source_range.start) else {
        return Ok(Vec::new());
    };
    let Some(row_utf16_len) = row_source_utf16_range
        .end
        .checked_sub(row_source_utf16_range.start)
    else {
        return Ok(Vec::new());
    };
    if row_byte_len > M11_SIMPLE_EDIT_LINE_MAX_BYTES as u64 {
        return Ok(Vec::new());
    }
    let row_source = read_utf8_source_range(runtime, row_source_range)?;
    if row_source.len() as u64 != row_byte_len
        || row_source.encode_utf16().count() as u64 != row_utf16_len
    {
        return Ok(Vec::new());
    }

    let editable_start = usize::try_from(editable.start - row_source_range.start)
        .map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let editable_end = usize::try_from(editable.end - row_source_range.start)
        .map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let Some(prefix) = row_source.get(..editable_start) else {
        return Ok(Vec::new());
    };
    let Some(content) = row_source.get(editable_start..editable_end) else {
        return Ok(Vec::new());
    };
    let Some(suffix) = row_source.get(editable_end..) else {
        return Ok(Vec::new());
    };
    if presentation == DocumentViewportRowPresentation::Table {
        return Ok(document_table_literal_word_edit_cells(
            &row_source,
            row_source_range,
            editable,
            editable_utf16,
            facts,
        ));
    }
    let literal_segment_shell = match presentation {
        DocumentViewportRowPresentation::Plain => prefix.is_empty(),
        DocumentViewportRowPresentation::ListItem {
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            container_count,
            simple_continuation,
            ..
        } => {
            prefix.is_empty()
                && nesting_depth == 1
                && container_count == 0
                && simple_continuation
                && prefix_start_byte == physical_line_start
                && prefix_end_byte == row_source_range.start
                && prefix_start_utf16 <= prefix_end_utf16
                && prefix_end_utf16 == row_source_utf16_range.start
        }
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            container_count,
            simple_continuation,
            ..
        } => {
            prefix.is_empty()
                && nesting_depth == 1
                && container_count == 1
                && simple_continuation
                && prefix_start_byte == physical_line_start
                && prefix_end_byte == row_source_range.start
                && prefix_start_utf16 <= prefix_end_utf16
                && prefix_end_utf16 == row_source_utf16_range.start
        }
        _ => false,
    };
    if literal_segment_shell {
        if !matches!(suffix, "" | "\n" | "\r\n") {
            return Ok(Vec::new());
        }
        if presentation != DocumentViewportRowPresentation::Plain
            && content.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
        {
            return Ok(Vec::new());
        }
        let mut cells = document_plain_literal_word_edit_cells(
            content,
            editable,
            editable_utf16,
            facts,
            presentation == DocumentViewportRowPresentation::Plain,
        );
        cells.extend(document_flat_strong_opening_space_edit_cells(
            content,
            editable,
            editable_utf16,
            facts,
        ));
        return Ok(cells);
    }

    let level = match presentation {
        DocumentViewportRowPresentation::Heading {
            level,
            style: DocumentHeadingStyle::Atx,
        } => level,
        _ => return Ok(Vec::new()),
    };
    let canonical_prefix = format!("{} ", "#".repeat(usize::from(level)));
    let byte_geometry_matches = content.len() as u64 == editable.end - editable.start;
    let utf16_geometry_matches = prefix.encode_utf16().count() as u64
        == editable_utf16.start - row_source_utf16_range.start
        && content.encode_utf16().count() as u64 == editable_utf16.end - editable_utf16.start
        && suffix.encode_utf16().count() as u64 == row_source_utf16_range.end - editable_utf16.end;
    if prefix != canonical_prefix
        || !matches!(suffix, "" | "\n" | "\r\n")
        || content.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
        || !byte_geometry_matches
        || !utf16_geometry_matches
    {
        return Ok(Vec::new());
    }

    if facts.is_empty() {
        return Ok(vec![DocumentProjectionEditCell {
            source_range: editable.clone(),
            source_utf16_range: editable_utf16.clone(),
            trigger_range: editable.clone(),
            trigger_utf16_range: editable_utf16.clone(),
            flags: DOCUMENT_PROJECTION_EDIT_CELL_PLAIN_ATX_FLAGS,
            replacement_first: 0,
            replacement_second: 0,
        }]);
    }

    Ok(document_flat_strong_opening_space_edit_cells(
        content,
        editable,
        editable_utf16,
        facts,
    ))
}

fn document_table_literal_word_edit_cells(
    row_source: &str,
    row_source_range: &Range<u64>,
    editable: &Range<u64>,
    editable_utf16: &Range<u64>,
    facts: &[DocumentInlineFact],
) -> Vec<DocumentProjectionEditCell> {
    facts
        .iter()
        .filter(|cell| cell.kind == DocumentInlineFactKind::TableCell)
        .filter_map(|cell| {
            if cell.content_range.start >= cell.content_range.end
                || cell.content_utf16_range.start >= cell.content_utf16_range.end
                || cell.source_range.start > cell.content_range.start
                || cell.content_range.end > cell.source_range.end
                || cell.source_utf16_range.start > cell.content_utf16_range.start
                || cell.content_utf16_range.end > cell.source_utf16_range.end
                || cell.content_range.start < editable.start
                || cell.content_range.end > editable.end
                || cell.content_utf16_range.start < editable_utf16.start
                || cell.content_utf16_range.end > editable_utf16.end
            {
                return None;
            }
            let intersects_other_fact = facts.iter().any(|fact| {
                fact.kind != DocumentInlineFactKind::TableCell
                    && fact.source_range.start < cell.content_range.end
                    && cell.content_range.start < fact.source_range.end
            });
            if intersects_other_fact {
                return None;
            }
            let relative_start =
                usize::try_from(cell.content_range.start - row_source_range.start).ok()?;
            let relative_end =
                usize::try_from(cell.content_range.end - row_source_range.start).ok()?;
            let source = row_source.get(relative_start..relative_end)?;
            if source.is_empty()
                || !source
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ')
                || !source.bytes().any(|byte| byte.is_ascii_alphanumeric())
                || source.len() as u64 != cell.content_range.end - cell.content_range.start
                || source.encode_utf16().count() as u64
                    != cell.content_utf16_range.end - cell.content_utf16_range.start
            {
                return None;
            }
            Some(document_literal_word_edit_cells(
                cell.content_range.clone(),
                cell.content_utf16_range.clone(),
                cell.content_range.clone(),
                cell.content_utf16_range.clone(),
                source.bytes().filter(u8::is_ascii_alphanumeric).count(),
                true,
            ))
        })
        .flatten()
        .collect()
}

fn document_literal_word_edit_cells(
    source_range: Range<u64>,
    source_utf16_range: Range<u64>,
    trigger_range: Range<u64>,
    trigger_utf16_range: Range<u64>,
    ascii_alphanumeric_count: usize,
    allow_one_unit_delete: bool,
) -> Vec<DocumentProjectionEditCell> {
    let mut cells = vec![DocumentProjectionEditCell {
        source_range: source_range.clone(),
        source_utf16_range: source_utf16_range.clone(),
        trigger_range: trigger_range.clone(),
        trigger_utf16_range: trigger_utf16_range.clone(),
        flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_WORD_FLAGS,
        replacement_first: 0,
        replacement_second: 0,
    }];
    // A one-unit deletion is safe only when every admitted deletion leaves at
    // least one alphanumeric source unit in the cell. It remains one-shot so
    // the host never infers that this count survived another deletion.
    if allow_one_unit_delete && ascii_alphanumeric_count >= 2 {
        cells.push(DocumentProjectionEditCell {
            source_range,
            source_utf16_range,
            trigger_range,
            trigger_utf16_range,
            flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_DELETE_ONE_FLAGS,
            replacement_first: 0,
            replacement_second: 0,
        });
    }
    cells
}

fn document_plain_literal_word_edit_cells(
    editable_source: &str,
    editable: &Range<u64>,
    editable_utf16: &Range<u64>,
    facts: &[DocumentInlineFact],
    allow_terminal_append: bool,
) -> Vec<DocumentProjectionEditCell> {
    let mut occupied = facts
        .iter()
        .filter_map(|fact| {
            (fact.source_range.start >= editable.start
                && fact.source_range.end <= editable.end
                && fact.source_range.start <= fact.source_range.end
                && fact.source_utf16_range.start >= editable_utf16.start
                && fact.source_utf16_range.end <= editable_utf16.end
                && fact.source_utf16_range.start <= fact.source_utf16_range.end)
                .then(|| (fact.source_range.clone(), fact.source_utf16_range.clone()))
        })
        .collect::<Vec<_>>();
    if occupied.len() != facts.len() {
        return Vec::new();
    }
    occupied.sort_by_key(|(bytes, utf16)| (bytes.start, bytes.end, utf16.start, utf16.end));

    let mut merged = Vec::<(Range<u64>, Range<u64>)>::new();
    for (bytes, utf16) in occupied {
        if let Some((last_bytes, last_utf16)) = merged.last_mut() {
            if bytes.start < last_bytes.end {
                last_bytes.end = last_bytes.end.max(bytes.end);
                last_utf16.end = last_utf16.end.max(utf16.end);
                continue;
            }
        }
        merged.push((bytes, utf16));
    }

    let mut gaps = Vec::new();
    let mut byte_cursor = editable.start;
    let mut utf16_cursor = editable_utf16.start;
    for (bytes, utf16) in &merged {
        if byte_cursor < bytes.start && utf16_cursor < utf16.start {
            gaps.push((
                byte_cursor..bytes.start,
                utf16_cursor..utf16.start,
                byte_cursor == editable.start,
                false,
            ));
        }
        byte_cursor = byte_cursor.max(bytes.end);
        utf16_cursor = utf16_cursor.max(utf16.end);
    }
    if byte_cursor < editable.end && utf16_cursor < editable_utf16.end {
        gaps.push((
            byte_cursor..editable.end,
            utf16_cursor..editable_utf16.end,
            byte_cursor == editable.start,
            true,
        ));
    }
    if merged.is_empty() && editable.start == editable.end {
        return Vec::new();
    }

    let mut cells = gaps
        .iter()
        .flat_map(|(bytes, utf16, starts_row, ends_row)| {
            document_ascii_line_literal_cells(
                editable_source,
                editable,
                bytes,
                utf16,
                *starts_row,
                *ends_row,
            )
        })
        .collect::<Vec<_>>();
    if allow_terminal_append {
        if let Some((bytes, utf16, _, true)) = gaps.last() {
            if let Some(cell) =
                document_terminal_literal_append_cell(editable_source, editable, bytes, utf16)
            {
                cells.push(cell);
            }
        }
    }
    cells
}

fn document_terminal_literal_append_cell(
    editable_source: &str,
    editable: &Range<u64>,
    gap_bytes: &Range<u64>,
    gap_utf16: &Range<u64>,
) -> Option<DocumentProjectionEditCell> {
    if gap_bytes.end != editable.end || gap_bytes.start >= gap_bytes.end {
        return None;
    }
    let relative_start = usize::try_from(gap_bytes.start - editable.start).ok()?;
    let relative_end = usize::try_from(gap_bytes.end - editable.start).ok()?;
    let gap_source = editable_source.get(relative_start..relative_end)?;
    if gap_source.is_empty() {
        return None;
    }
    let trailing_spaces = gap_source
        .bytes()
        .rev()
        .take_while(|byte| *byte == b' ')
        .count();
    if trailing_spaces > 1
        || (trailing_spaces == 0
            && gap_source
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_whitespace))
    {
        return None;
    }
    let line_start = gap_source
        .char_indices()
        .rev()
        .find_map(|(index, character)| matches!(character, '\r' | '\n').then_some(index + 1))
        .unwrap_or(0);
    let line_source = gap_source.get(line_start..)?;
    if line_source.is_empty() {
        return None;
    }
    // Appending U+0020 can itself complete a block opener (`- `, `1. `,
    // `# `, ...). Keep this first terminal tranche on ordinary prose lines:
    // the parser-certified current row is Plain and the physical line begins,
    // after at most ordinary paragraph padding, with an ASCII letter. This is
    // deliberately narrower than reproducing block grammar in the host.
    let physical_line_end = relative_end;
    let physical_line_start = editable_source
        .get(..physical_line_end)?
        .char_indices()
        .rev()
        .find_map(|(index, character)| matches!(character, '\r' | '\n').then_some(index + 1))
        .unwrap_or(0);
    let physical_line = editable_source.get(physical_line_start..physical_line_end)?;
    if gap_bytes
        .start
        .checked_add(u64::try_from(line_start).ok()?)?
        != editable
            .start
            .checked_add(u64::try_from(physical_line_start).ok()?)?
    {
        // A fact earlier on this physical line can absorb a terminal suffix
        // after the edit (for example, GFM bare-autolink punctuation). The
        // first terminal tranche retains outside facts, so it may authorize
        // only a final line whose complete source belongs to this plain gap.
        return None;
    }
    let leading_spaces = physical_line
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    if leading_spaces > 3
        || !physical_line
            .as_bytes()
            .get(leading_spaces)
            .is_some_and(u8::is_ascii_alphabetic)
    {
        return None;
    }
    let line_utf16_start = gap_utf16.start
        + u64::try_from(gap_source.get(..line_start)?.encode_utf16().count()).ok()?;
    let source_range = gap_bytes.start + u64::try_from(line_start).ok()?..gap_bytes.end;
    let source_utf16_range = line_utf16_start..gap_utf16.end;
    Some(DocumentProjectionEditCell {
        source_range,
        source_utf16_range,
        trigger_range: gap_bytes.end..gap_bytes.end,
        trigger_utf16_range: gap_utf16.end..gap_utf16.end,
        flags: DOCUMENT_PROJECTION_EDIT_CELL_LITERAL_APPEND_FLAGS
            | if trailing_spaces == 1 {
                DOCUMENT_PROJECTION_EDIT_CELL_TERMINAL_SPACE_BLOCKED
            } else {
                0
            },
        replacement_first: 0,
        replacement_second: 0,
    })
}

fn document_ascii_line_literal_cells(
    editable_source: &str,
    editable: &Range<u64>,
    bytes: &Range<u64>,
    utf16: &Range<u64>,
    starts_row: bool,
    ends_row: bool,
) -> Vec<DocumentProjectionEditCell> {
    let Ok(relative_start) = usize::try_from(bytes.start - editable.start) else {
        return Vec::new();
    };
    let Ok(relative_end) = usize::try_from(bytes.end - editable.start) else {
        return Vec::new();
    };
    let Some(gap_source) = editable_source.get(relative_start..relative_end) else {
        return Vec::new();
    };
    let Some(byte_len) = bytes.end.checked_sub(bytes.start) else {
        return Vec::new();
    };
    let Some(utf16_len) = utf16.end.checked_sub(utf16.start) else {
        return Vec::new();
    };
    if gap_source.len() as u64 != byte_len || gap_source.encode_utf16().count() as u64 != utf16_len
    {
        return Vec::new();
    }
    let mut cells = Vec::new();
    let mut segment_start = 0;
    let gap_bytes = gap_source.as_bytes();
    while segment_start <= gap_bytes.len() {
        let segment_end = gap_bytes[segment_start..]
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(gap_bytes.len(), |offset| segment_start + offset);
        let raw_source = &gap_source[segment_start..segment_end];
        let leading_spaces = raw_source.bytes().take_while(|byte| *byte == b' ').count();
        let trailing_spaces = raw_source
            .bytes()
            .rev()
            .take_while(|byte| *byte == b' ')
            .count();
        let literal_start = segment_start + leading_spaces;
        let literal_end = segment_end.saturating_sub(trailing_spaces);
        let source = gap_source
            .get(literal_start..literal_end)
            .unwrap_or_default();
        if !source.is_empty()
            && source
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ')
            && source.bytes().any(|byte| byte.is_ascii_alphanumeric())
        {
            let raw_utf16_prefix = gap_source[..segment_start].encode_utf16().count() as u64;
            let raw_utf16_len = raw_source.encode_utf16().count() as u64;
            let literal_utf16_prefix = gap_source[..literal_start].encode_utf16().count() as u64;
            let literal_utf16_len = source.encode_utf16().count() as u64;
            // The affected closure includes harmless surrounding U+0020 so
            // every matcher for the final physical-line gap shares the same
            // parser-authored partition. The trigger remains the trimmed
            // literal, so neither leading indentation nor a trailing hard-
            // break boundary is admitted by the generic matcher.
            let segment_bytes =
                bytes.start + segment_start as u64..bytes.start + segment_end as u64;
            let segment_utf16 =
                utf16.start + raw_utf16_prefix..utf16.start + raw_utf16_prefix + raw_utf16_len;
            let trigger_bytes =
                bytes.start + literal_start as u64..bytes.start + literal_end as u64;
            let trigger_utf16 = utf16.start + literal_utf16_prefix
                ..utf16.start + literal_utf16_prefix + literal_utf16_len;
            let starts_physical_line = (segment_start == 0 && starts_row) || segment_start > 0;
            let ends_physical_line =
                (segment_end == gap_bytes.len() && ends_row) || segment_end < gap_bytes.len();
            let byte_trigger_start = if starts_physical_line || leading_spaces > 0 {
                trigger_bytes.start
            } else {
                trigger_bytes.start + 1
            };
            let ends_editable = ends_row && segment_end == gap_bytes.len();
            let byte_trigger_end = if ends_editable {
                trigger_bytes.end.saturating_sub(1)
            } else if ends_physical_line || trailing_spaces > 0 {
                trigger_bytes.end
            } else {
                trigger_bytes.end.saturating_sub(1)
            };
            let trigger_start = if starts_physical_line || leading_spaces > 0 {
                trigger_utf16.start
            } else {
                trigger_utf16.start + 1
            };
            let trigger_end = if ends_editable {
                trigger_utf16.end.saturating_sub(1)
            } else if ends_physical_line || trailing_spaces > 0 {
                trigger_utf16.end
            } else {
                trigger_utf16.end.saturating_sub(1)
            };
            if byte_trigger_start <= byte_trigger_end && trigger_start <= trigger_end {
                cells.extend(document_literal_word_edit_cells(
                    segment_bytes,
                    segment_utf16,
                    byte_trigger_start..byte_trigger_end,
                    trigger_start..trigger_end,
                    source.bytes().filter(u8::is_ascii_alphanumeric).count(),
                    !(starts_physical_line && leading_spaces > 0)
                        && !(ends_physical_line && trailing_spaces > 0),
                ));
            }
        }
        if segment_end == gap_bytes.len() {
            break;
        }
        segment_start = segment_end + 1;
        if gap_bytes[segment_end] == b'\r'
            && segment_start < gap_bytes.len()
            && gap_bytes[segment_start] == b'\n'
        {
            segment_start += 1;
        }
    }
    cells
}

fn document_flat_strong_opening_space_edit_cells(
    editable_source: &str,
    editable: &Range<u64>,
    editable_utf16: &Range<u64>,
    facts: &[DocumentInlineFact],
) -> Vec<DocumentProjectionEditCell> {
    let mut cells = Vec::new();
    for (candidate_index, candidate) in facts.iter().enumerate() {
        if candidate.kind != DocumentInlineFactKind::Strong
            || candidate.flags != 0
            || candidate.replacement.is_some()
            || candidate.source_range.start < editable.start
            || candidate.source_range.end > editable.end
            || candidate.source_utf16_range.start < editable_utf16.start
            || candidate.source_utf16_range.end > editable_utf16.end
            || candidate.source_range.start > candidate.source_range.end
            || candidate.source_utf16_range.start > candidate.source_utf16_range.end
        {
            continue;
        }
        let Some(expected_content_start) = candidate.source_range.start.checked_add(2) else {
            continue;
        };
        let Some(expected_content_end) = candidate.source_range.end.checked_sub(2) else {
            continue;
        };
        let Some(expected_content_utf16_start) = candidate.source_utf16_range.start.checked_add(2)
        else {
            continue;
        };
        let Some(expected_content_utf16_end) = candidate.source_utf16_range.end.checked_sub(2)
        else {
            continue;
        };
        if candidate.content_range != (expected_content_start..expected_content_end)
            || candidate.content_utf16_range
                != (expected_content_utf16_start..expected_content_utf16_end)
        {
            continue;
        }

        let Ok(relative_start) = usize::try_from(candidate.source_range.start - editable.start)
        else {
            continue;
        };
        let Ok(relative_end) = usize::try_from(candidate.source_range.end - editable.start) else {
            continue;
        };
        let Some(candidate_source) = editable_source.get(relative_start..relative_end) else {
            continue;
        };
        let Some(word) = candidate_source
            .strip_prefix("**")
            .and_then(|source| source.strip_suffix("**"))
        else {
            continue;
        };
        if word.is_empty() || !word.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            continue;
        }
        let outside_contains_asterisk = editable_source
            .get(..relative_start)
            .into_iter()
            .chain(editable_source.get(relative_end..))
            .flat_map(str::bytes)
            .any(|byte| byte == b'*');
        if outside_contains_asterisk {
            continue;
        }
        let overlaps_or_nests = facts.iter().enumerate().any(|(index, fact)| {
            index != candidate_index
                && fact.source_range.start < candidate.source_range.end
                && candidate.source_range.start < fact.source_range.end
        });
        if overlaps_or_nests {
            continue;
        }

        cells.push(DocumentProjectionEditCell {
            source_range: candidate.source_range.clone(),
            source_utf16_range: candidate.source_utf16_range.clone(),
            trigger_range: candidate.content_range.start..candidate.content_range.start,
            trigger_utf16_range: candidate.content_utf16_range.start
                ..candidate.content_utf16_range.start,
            flags: DOCUMENT_PROJECTION_EDIT_CELL_STRONG_OPENING_SPACE_FLAGS,
            replacement_first: 0,
            replacement_second: 0,
        });
    }
    cells
}

fn document_row_starts_at_physical_line_start(runtime: &DocumentRuntime, start: u64) -> bool {
    if start == 0 {
        return true;
    }
    let Ok(start) = usize::try_from(start) else {
        return false;
    };
    let mut previous = [0_u8; 1];
    if runtime
        .read_current_source_window(start - 1..start, &mut previous)
        .ok()
        != Some(previous.len())
    {
        return false;
    }
    previous[0] == b'\n'
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

struct CapturedDocumentInlineProjection {
    inline_source: Range<u32>,
    editable: Range<u64>,
    facts: Vec<M11InlineProjectionFact>,
    link_values: Vec<M11InlineLinkValue>,
    edit_components: Vec<M11InlineEditComponent>,
}

struct DocumentInlineProjectionAuthority {
    inline_facts: Vec<DocumentInlineFact>,
    projection_edit_cells: Vec<DocumentProjectionEditCell>,
}

fn document_inline_facts(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<DocumentInlineProjectionAuthority>, DocumentSessionError> {
    let Some(captured) = capture_document_inline_projection(runtime, session, row)? else {
        return Ok(None);
    };
    map_document_inline_projection(runtime, captured)
}

#[cfg(test)]
fn document_inline_facts_without_reference_authority(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<DocumentInlineProjectionAuthority>, DocumentSessionError> {
    let Some(captured) =
        capture_document_inline_projection_without_reference_authority(runtime, session, row)?
    else {
        return Ok(None);
    };
    map_document_inline_projection(runtime, captured)
}

#[cfg(test)]
fn capture_document_inline_projection_without_reference_authority(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<CapturedDocumentInlineProjection>, DocumentSessionError> {
    let Some((prepared, editable, editable_utf16, parser_profile)) =
        prepare_document_inline_projection(runtime, session, row)?
    else {
        return Ok(None);
    };
    capture_prepared_inline_projection(
        runtime,
        prepared,
        editable,
        editable_utf16,
        parser_profile,
        None,
    )
}

#[cfg(any(test, feature = "opening-session"))]
fn capture_document_inline_projection_from_compact_probe(
    runtime: &mut DocumentRuntime,
    probe: &flark_parser::M11CompactViewportProbe,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<CapturedDocumentInlineProjection>, DocumentSessionError> {
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
    let point = M11RecursiveGreenPoint::new(
        usize::try_from(row.physical_range().start)
            .map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
        usize::try_from(row.physical_utf16_range().start)
            .map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
        SourceBoundaryAffinity::After,
    );
    let Some(captured) = probe
        .capture_inline_projection(runtime, point)
        .map_err(|_| DocumentSessionError::Faulted)?
    else {
        return Ok(None);
    };
    if u64::from(captured.inline_source.start) != editable.start
        || u64::from(captured.inline_source.end) != editable.end
        || u64::from(captured.inline_source_utf16.start) != editable_utf16.start
        || u64::from(captured.inline_source_utf16.end) != editable_utf16.end
    {
        return Ok(None);
    }
    Ok(Some(CapturedDocumentInlineProjection {
        inline_source: captured.inline_source,
        editable,
        facts: captured.facts,
        link_values: captured.link_values,
        edit_components: captured.edit_components,
    }))
}

#[cfg(any(test, feature = "opening-session"))]
fn document_inline_facts_from_compact_probe(
    runtime: &mut DocumentRuntime,
    probe: &flark_parser::M11CompactViewportProbe,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<DocumentInlineProjectionAuthority>, DocumentSessionError> {
    let Some(captured) =
        capture_document_inline_projection_from_compact_probe(runtime, probe, row)?
    else {
        return Ok(None);
    };
    map_document_inline_projection(runtime, captured)
}

fn capture_document_inline_projection(
    runtime: &mut DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<CapturedDocumentInlineProjection>, DocumentSessionError> {
    let Some((prepared, editable, editable_utf16, parser_profile)) =
        prepare_document_inline_projection(runtime, session, row)?
    else {
        return Ok(None);
    };
    let reference_resolver = session.reference_resolver(runtime)?;
    capture_prepared_inline_projection(
        runtime,
        prepared,
        editable,
        editable_utf16,
        parser_profile,
        Some(reference_resolver),
    )
}

fn prepare_document_inline_projection(
    runtime: &DocumentRuntime,
    session: &M11PersistentRecursiveGreenSession,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<
    Option<(
        M11RecursiveGreenInlineLeafPreparation,
        Range<u64>,
        Range<u64>,
        ParserProfileId,
    )>,
    DocumentSessionError,
> {
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
    let parser_profile = ParserProfileId::new(u64::from(session.syntax_profile()))
        .ok_or(DocumentSessionError::Faulted)?;
    Ok(Some((prepared, editable, editable_utf16, parser_profile)))
}

fn capture_prepared_inline_projection(
    runtime: &mut DocumentRuntime,
    prepared: M11RecursiveGreenInlineLeafPreparation,
    editable: Range<u64>,
    editable_utf16: Range<u64>,
    parser_profile: ParserProfileId,
    reference_resolver: Option<M11ReferenceResolver>,
) -> Result<Option<CapturedDocumentInlineProjection>, DocumentSessionError> {
    let inline_source = prepared.inline_source_range();
    let inline_source_utf16 = prepared.inline_source_utf16_range();
    if u64::from(inline_source.start) != editable.start
        || u64::from(inline_source.end) != editable.end
        || u64::from(inline_source_utf16.start) != editable_utf16.start
        || u64::from(inline_source_utf16.end) != editable_utf16.end
    {
        return Ok(None);
    }
    let binding = M11ParserBinding::current(parser_profile);
    let mut job = match reference_resolver {
        Some(reference_resolver) => {
            M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_reference_resolver_and_fact_capture(
                runtime,
                prepared.into_fence(),
                binding,
                reference_resolver,
            )?
        }
        None => M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_fact_capture(
            runtime,
            prepared.into_fence(),
            binding,
        )?,
    };
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

    if job.projected_facts_are_authoritative() != Some(true) {
        abort_inline_fact_job(runtime, &mut job)?;
        return Ok(None);
    }

    let facts = job
        .take_projected_facts()
        .ok_or(DocumentSessionError::Faulted)?;
    let link_values = job
        .take_projected_link_values()
        .ok_or(DocumentSessionError::Faulted)?;
    let edit_components = job
        .take_projected_edit_components()
        .ok_or(DocumentSessionError::Faulted)?;
    abort_inline_fact_job(runtime, &mut job)?;
    Ok(Some(CapturedDocumentInlineProjection {
        inline_source,
        editable,
        facts,
        link_values,
        edit_components,
    }))
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

fn map_document_inline_projection(
    runtime: &DocumentRuntime,
    captured: CapturedDocumentInlineProjection,
) -> Result<Option<DocumentInlineProjectionAuthority>, DocumentSessionError> {
    let projection_edit_cells = map_parser_projection_edit_cells(
        runtime,
        captured.inline_source.clone(),
        &captured.editable,
        captured.edit_components,
    )?;
    let Some(inline_facts) = map_document_inline_facts(
        runtime,
        captured.inline_source,
        captured.editable,
        captured.facts,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(DocumentInlineProjectionAuthority {
        inline_facts,
        projection_edit_cells,
    }))
}

fn map_parser_projection_edit_cells(
    runtime: &DocumentRuntime,
    inline_source: Range<u32>,
    editable: &Range<u64>,
    components: Vec<M11InlineEditComponent>,
) -> Result<Vec<DocumentProjectionEditCell>, DocumentSessionError> {
    let lease = runtime.snapshot_current_source()?;
    let mut mapped = Vec::with_capacity(components.len());
    for component in components {
        let affected = absolute_inline_range(inline_source.start, component.affected())?;
        let trigger = absolute_inline_range(inline_source.start, component.trigger())?;
        if affected.start < editable.start
            || affected.end > editable.end
            || trigger.start < affected.start
            || trigger.end > affected.end
        {
            return Err(DocumentSessionError::Faulted);
        }
        let (flags, replacement_first, replacement_second) = match component.matcher() {
            M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar } => (
                DOCUMENT_PROJECTION_EDIT_CELL_EXACT_SCALAR_FLAGS,
                u32::from(scalar),
                0,
            ),
        };
        mapped.push(DocumentProjectionEditCell {
            source_utf16_range: source_utf16_range(&lease, &affected)?,
            trigger_utf16_range: source_utf16_range(&lease, &trigger)?,
            source_range: affected,
            trigger_range: trigger,
            flags,
            replacement_first,
            replacement_second,
        });
    }
    Ok(mapped)
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
    append_document_table_facts(&mut mapped, &lease, inline_source.start, &inline_bytes)?;
    if mapped.len() > VIEWPORT_INLINE_FACTS_PER_ROW_MAX {
        return Ok(None);
    }
    Ok(Some(mapped))
}

fn document_literal_safe_envelopes(
    runtime: &DocumentRuntime,
    presentation: DocumentViewportRowPresentation,
    row_source_range: &Range<u64>,
    facts: Option<&[DocumentInlineFact]>,
    editable_range: Option<&Range<u64>>,
    editable_utf16_range: Option<&Range<u64>>,
) -> Result<Vec<DocumentLiteralSafeEnvelope>, DocumentSessionError> {
    let (Some(facts), Some(editable), Some(editable_utf16)) =
        (facts, editable_range, editable_utf16_range)
    else {
        return Ok(Vec::new());
    };
    if editable.is_empty() || editable_utf16.is_empty() {
        return Ok(Vec::new());
    }

    // The ATX marker is outside this editable slice. Once the parser has also
    // authored the complete absence of inline facts, a slice bounded by word
    // bytes and containing only word bytes and ordinary spaces has no latent
    // block or inline delimiter. ABI 4.27 interprets the non-empty space
    // envelope as strict-interior authority; the separate zero-width endpoint
    // authorizes one trailing space and is consumed by Core. Requiring word
    // bytes at both edges avoids carrying an identity-projection proof across
    // Markdown's leading/trailing whitespace normalization.
    let atx_level = match presentation {
        DocumentViewportRowPresentation::Heading {
            level,
            style: DocumentHeadingStyle::Atx,
        } => Some(level),
        _ => None,
    };
    if let Some(level) = atx_level.filter(|_| facts.is_empty()) {
        let content = read_utf8_source_range(runtime, editable)?;
        let prefix = read_utf8_source_range(runtime, &(row_source_range.start..editable.start))?;
        let suffix = read_utf8_source_range(runtime, &(editable.end..row_source_range.end))?;
        let content_byte_len = editable.end - editable.start;
        let content_utf16_len = editable_utf16.end - editable_utf16.start;
        let canonical_prefix = format!("{} ", "#".repeat(usize::from(level)));
        let canonical_edges =
            prefix == canonical_prefix && matches!(suffix.as_str(), "" | "\n" | "\r\n");
        let edge_word_bounded = content
            .as_bytes()
            .first()
            .zip(content.as_bytes().last())
            .is_some_and(|(first, last)| {
                first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric()
            });
        if content.len() as u64 == content_byte_len
            && content_byte_len == content_utf16_len
            && canonical_edges
            && edge_word_bounded
            && content
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ')
        {
            return Ok(vec![
                DocumentLiteralSafeEnvelope {
                    edit_class: DocumentLiteralEditClass::AsciiWordInsertion,
                    source_range: editable.clone(),
                    source_utf16_range: editable_utf16.clone(),
                },
                DocumentLiteralSafeEnvelope {
                    edit_class: DocumentLiteralEditClass::SingleAsciiSpaceInsertion,
                    source_range: editable.clone(),
                    source_utf16_range: editable_utf16.clone(),
                },
                DocumentLiteralSafeEnvelope {
                    edit_class: DocumentLiteralEditClass::SingleAsciiSpaceInsertion,
                    source_range: editable.end..editable.end,
                    source_utf16_range: editable_utf16.end..editable_utf16.end,
                },
            ]);
        }
    }

    let eligible = |fact: &&DocumentInlineFact| {
        matches!(
            fact.kind,
            DocumentInlineFactKind::Emphasis
                | DocumentInlineFactKind::Strong
                | DocumentInlineFactKind::Code
                | DocumentInlineFactKind::Strikethrough
                | DocumentInlineFactKind::DirectLink
        ) && editable.start <= fact.source_range.start
            && fact.source_range.end <= editable.end
            && editable_utf16.start <= fact.source_utf16_range.start
            && fact.source_utf16_range.end <= editable_utf16.end
    };

    let mut envelopes = Vec::new();
    'facts: for fact in facts.iter().filter(eligible) {
        if fact.content_range.is_empty() || fact.content_utf16_range.is_empty() {
            continue;
        }
        // Publish only maximal ASCII-word leaves whose neighboring source is
        // the fact boundary or ordinary whitespace and whose bytes do not
        // intersect another inline fact. This keeps the proof local around
        // entities, escapes, nested syntax, and Unicode adjacency while also
        // covering ordinary multiword styled content such as `**bold text**`.
        // The host still admits ASCII word insertion only; it never infers
        // Markdown or widens these parser-authored leaf boundaries.
        let content = read_utf8_source_range(runtime, &fact.content_range)?;
        let content_byte_len = fact.content_range.end - fact.content_range.start;
        let content_utf16_len = fact.content_utf16_range.end - fact.content_utf16_range.start;
        if content.len() as u64 != content_byte_len
            || (fact.kind == DocumentInlineFactKind::Code && fact.flags != 0)
        {
            continue;
        }
        let bytes = content.as_bytes();
        let mut cursor = 0usize;
        let mut utf16_prefix_bytes = 0usize;
        let mut utf16_prefix_units = 0u64;
        while cursor < bytes.len() {
            if !bytes[cursor].is_ascii_alphanumeric() {
                cursor += 1;
                continue;
            }
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_alphanumeric() {
                cursor += 1;
            }
            let end = cursor;
            let left_guarded = start == 0 || bytes[start - 1] == b' ';
            let right_guarded = end == bytes.len() || bytes[end] == b' ';
            if !left_guarded || !right_guarded {
                continue;
            }
            let start = u64::try_from(start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
            let end = u64::try_from(end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
            let source_range = fact.content_range.start + start..fact.content_range.start + end;
            if facts.iter().any(|other| {
                !std::ptr::eq(other, fact)
                    && other.source_range.start < source_range.end
                    && source_range.start < other.source_range.end
                    && !(other.content_range.start <= source_range.start
                        && source_range.end <= other.content_range.end)
            }) {
                continue;
            }
            let start_usize =
                usize::try_from(start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
            let end_usize =
                usize::try_from(end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
            utf16_prefix_units = utf16_prefix_units
                .checked_add(
                    u64::try_from(
                        content[utf16_prefix_bytes..start_usize]
                            .encode_utf16()
                            .count(),
                    )
                    .map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
                )
                .ok_or(DocumentSessionError::RangeOutOfBounds)?;
            utf16_prefix_bytes = start_usize;
            let utf16_start = fact
                .content_utf16_range
                .start
                .checked_add(utf16_prefix_units)
                .ok_or(DocumentSessionError::RangeOutOfBounds)?;
            let utf16_end = utf16_start
                .checked_add(
                    u64::try_from(end_usize - start_usize)
                        .map_err(|_| DocumentSessionError::RangeOutOfBounds)?,
                )
                .ok_or(DocumentSessionError::RangeOutOfBounds)?;
            if utf16_end > fact.content_utf16_range.end || content_utf16_len == 0 {
                continue;
            }
            envelopes.push(DocumentLiteralSafeEnvelope {
                edit_class: DocumentLiteralEditClass::AsciiWordInsertion,
                source_range,
                source_utf16_range: utf16_start..utf16_end,
            });
            if envelopes.len() >= VIEWPORT_LITERAL_SAFE_ENVELOPES_PER_ROW_MAX {
                break 'facts;
            }
        }
    }

    // A construct ending at the editable row boundary has no following
    // source that a single inserted space can reclassify. The zero-width
    // envelope is intentionally one-shot: after the insertion, the host waits
    // for fresh parser authority instead of assuming a second trailing space
    // is harmless (two spaces can create a hard line break).
    if envelopes.len() < VIEWPORT_LITERAL_SAFE_ENVELOPES_PER_ROW_MAX
        && facts.iter().filter(eligible).any(|fact| {
            fact.source_range.end == editable.end
                && fact.source_utf16_range.end == editable_utf16.end
        })
    {
        envelopes.push(DocumentLiteralSafeEnvelope {
            edit_class: DocumentLiteralEditClass::SingleAsciiSpaceInsertion,
            source_range: editable.end..editable.end,
            source_utf16_range: editable_utf16.end..editable_utf16.end,
        });
    }
    Ok(envelopes)
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

/// Serves live-projection spans during a progressive open: the certified
/// early viewport's range (when bound to the current generation) is
/// certified, and everything else is exact pending source.
#[cfg(feature = "opening-session")]
fn opening_live_viewport(
    runtime: &DocumentRuntime,
    state: &OpeningState,
    revision: u64,
    requested_range: Range<usize>,
    maximum_spans: u32,
) -> Result<DocumentLiveViewport, DocumentSessionError> {
    let certified = state.session.certified_early().and_then(|(early, source)| {
        if runtime.current_source_version() != Some(source) {
            return None;
        }
        let range = early.root().source_range();
        let start = usize::try_from(range.start).ok()?;
        let end = usize::try_from(range.end).ok()?;
        let start = start.max(requested_range.start);
        let end = end.min(requested_range.end);
        (start < end).then_some(start..end)
    });
    let Some(certified) = certified else {
        return pending_live_viewport(runtime, revision, requested_range, maximum_spans);
    };
    let lease = runtime.snapshot_current_source()?;
    let utf16 = |range: &Range<usize>| -> Result<Range<u64>, DocumentSessionError> {
        Ok(lease.utf16_offset_for_byte(range.start)? as u64
            ..lease.utf16_offset_for_byte(range.end)? as u64)
    };
    let mut spans = Vec::new();
    let mut cursor = requested_range.start;
    let maximum_spans = maximum_spans as usize;
    if cursor < certified.start && spans.len() < maximum_spans {
        let range = cursor..certified.start;
        spans.push(DocumentLiveViewportSpan::Pending {
            source_range: range.start as u64..range.end as u64,
            source_utf16_range: utf16(&range)?,
        });
        cursor = certified.start;
    }
    if cursor < certified.end && spans.len() < maximum_spans {
        spans.push(DocumentLiveViewportSpan::CertifiedUnchanged {
            source_range: cursor as u64..certified.end as u64,
            source_utf16_range: utf16(&(cursor..certified.end))?,
        });
        cursor = certified.end;
    }
    if cursor < requested_range.end && spans.len() < maximum_spans {
        let range = cursor..requested_range.end;
        spans.push(DocumentLiveViewportSpan::Pending {
            source_range: range.start as u64..range.end as u64,
            source_utf16_range: utf16(&range)?,
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

/// Serves complete certified viewport rows during a progressive open,
/// clamped to the certified early viewport's range at its bound generation.
/// `total_rows` reports only the known certified prefix and `complete` stays
/// false: a pre-EOF row count is never an exact total.
#[cfg(feature = "opening-session")]
fn query_opening_viewport(
    runtime: &mut DocumentRuntime,
    state: &OpeningState,
    revision: u64,
    requested_range: Range<usize>,
    maximum_rows: u32,
) -> Result<DocumentViewport, DocumentSessionError> {
    let Some((early, source)) = state.session.certified_early() else {
        return Err(DocumentSessionError::NotReady);
    };
    if runtime.current_source_version() != Some(source) {
        return Err(DocumentSessionError::NotReady);
    }
    let slice = early.root().source_range();
    let slice_start =
        usize::try_from(slice.start).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let slice_end =
        usize::try_from(slice.end).map_err(|_| DocumentSessionError::RangeOutOfBounds)?;
    let start = requested_range.start.max(slice_start);
    let end = requested_range.end.min(slice_end);
    if start >= end {
        return Ok(DocumentViewport {
            revision,
            requested_range: requested_range.start as u64..requested_range.end as u64,
            start_ordinal: 0,
            total_rows: 0,
            complete: false,
            rows: Vec::new(),
            receipt: DocumentQueryReceipt::default(),
        });
    }
    let limits = row_query_limits(maximum_rows).ok_or(DocumentSessionError::QueryBudgetExceeded)?;
    let lease = runtime.snapshot_current_source()?;
    let start_utf16 = lease.utf16_offset_for_byte(start)?;
    drop(lease);
    let outcome = early
        .root()
        .locate_renderable_rows_bounded(
            runtime,
            M11RecursiveGreenPoint::new(start, start_utf16, SourceBoundaryAffinity::After),
            end as u64,
            limits,
        )
        .map_err(|error| {
            DocumentSessionError::Parser(M11PersistentRecursiveGreenSessionError::Green(error))
        })?;
    let window = match outcome {
        M11RecursiveGreenRowQueryOutcome::Window(window) => window,
        M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
            return Err(DocumentSessionError::QueryBudgetExceeded)
        }
    };
    let start_ordinal = window.start_ordinal();
    let rows = window.rows().to_vec();
    let mapped = map_document_viewport_rows(runtime, &rows, |runtime, row| {
        document_inline_facts_from_compact_probe(runtime, early, row)
    })?;
    let total_rows = start_ordinal.saturating_add(mapped.len() as u64);
    Ok(DocumentViewport {
        revision,
        requested_range: requested_range.start as u64..requested_range.end as u64,
        start_ordinal,
        total_rows,
        complete: false,
        rows: mapped,
        receipt: DocumentQueryReceipt::default(),
    })
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

#[cfg(feature = "opening-session")]
impl DocumentSession {
    /// Advances one progressive open by a bounded grant: parser work first,
    /// then at starvation the outstanding transport pages adopt in one
    /// authenticated step, an exhaustion seal applies, or the pump yields to
    /// await transport. Completion releases the final compact viewport and
    /// starts the ordinary clean build over the sealed source.
    fn advance_opening(
        &mut self,
        mut state: Box<OpeningState>,
        fuel: usize,
    ) -> Result<(ParseState, usize), DocumentSessionError> {
        if state.finalizing {
            let complete = state.session.poll_final_release(&mut self.runtime, 1)?;
            return if complete {
                let build = begin_clean_build(&mut self.runtime)?;
                Ok((ParseState::Clean(Box::new(build)), 1))
            } else {
                Ok((ParseState::Opening(state), 1))
            };
        }
        let poll = state.session.poll(&mut self.runtime, fuel)?;
        let parser_transitions = state.session.last_poll_transitions();
        if parser_transitions > fuel {
            return Err(M11PersistentRecursiveGreenSessionError::InvalidState(
                "progressive open parser violated its bounded work grant",
            )
            .into());
        }
        // An already-starved frontier legitimately reports zero inner parser
        // transitions. Polling it and applying the associated adopt/seal/yield
        // decision is still one outer session work unit, so the pump always
        // makes bounded accounting progress.
        let work_units = parser_transitions.max(1);
        match poll {
            M11ProgressiveOpenSessionPoll::Pending => Ok((ParseState::Opening(state), work_units)),
            M11ProgressiveOpenSessionPoll::Starved => {
                if state.store.version() != state.adopted {
                    let proof = state.store.prove_append_since(state.adopted)?;
                    let current = proof.current();
                    state
                        .session
                        .adopt_append(&mut self.runtime, proof, state.seal_requested)?;
                    state.adopted = current;
                    Ok((ParseState::Opening(state), work_units))
                } else if state.seal_requested {
                    state.session.seal_exhausted(&mut self.runtime)?;
                    Ok((ParseState::Opening(state), work_units))
                } else {
                    // Awaiting transport: conservatively consume the unused
                    // grant so the bounded outer pump yields instead of
                    // repeatedly polling the same zero-transition frontier.
                    Ok((ParseState::Opening(state), fuel))
                }
            }
            M11ProgressiveOpenSessionPoll::Complete => {
                state.session.begin_final_release(&mut self.runtime)?;
                state.finalizing = true;
                Ok((ParseState::Opening(state), work_units))
            }
        }
    }

    /// Applies one literal edit during a progressive open. The store is the
    /// mutation authority: the edit advances the edit revision, and the
    /// replica, parser session, and certified viewport rebuild from the
    /// post-edit snapshot. Load-time edits trade locality for correctness;
    /// Experiment B convergence replaces the restart later.
    fn apply_opening_edit(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<DocumentEditReceipt, DocumentSessionError> {
        let lease = self.runtime.snapshot_current_source()?;
        let utf16_range =
            lease.utf16_offset_for_byte(range.start)?..lease.utf16_offset_for_byte(range.end)?;
        drop(lease);
        let ParseState::Opening(state) = mem::replace(&mut self.parser, ParseState::Transition)
        else {
            return Err(DocumentSessionError::NotReady);
        };
        let OpeningState {
            mut store,
            mut session,
            seal_requested,
            ..
        } = *state;
        let restart = (|| -> Result<OpeningState, DocumentSessionError> {
            session.release(&mut self.runtime);
            let version = store.version();
            store.apply_utf16_edit(version, utf16_range, replacement)?;
            let mut old = mem::replace(
                &mut self.runtime,
                DocumentRuntime::from_opening_snapshot(
                    store.snapshot(),
                    DocumentRuntimeConfig::default(),
                )?,
            );
            old.begin_close()?;
            while !old.poll_close(4_096)?.complete {}
            let session =
                M11ProgressiveOpenSession::begin(&mut self.runtime, SYNTAX_PROFILE_GFM_V1)?;
            let adopted = store.version();
            Ok(OpeningState {
                store,
                session,
                adopted,
                seal_requested,
                finalizing: false,
            })
        })();
        match restart {
            Ok(state) => {
                self.parser = ParseState::Opening(Box::new(state));
                self.edit_context = None;
                Ok(DocumentEditReceipt {
                    revision: self.revision(),
                    parser_pending: true,
                })
            }
            Err(error) => {
                if self.fault_arena_metrics.is_none() {
                    self.fault_arena_metrics = Some(self.runtime.arena_metrics());
                }
                self.parser = ParseState::Faulted;
                Err(error)
            }
        }
    }
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

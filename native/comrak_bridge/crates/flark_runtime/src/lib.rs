#![forbid(unsafe_code)]
//! Flark v4 host-neutral runtime and its fixed contract vocabulary.
//!
//! The contract types remain independent of any host language. The document
//! actor at the bottom of this module joins them to Flark's persistent parser
//! and source engine without exposing either implementation across the ABI.

mod actor;
mod document;
mod edit_intent;

pub use actor::{DocumentActor, DocumentActorError, DocumentActorInspection};

pub use document::{
    DocumentBulletMarker, DocumentCloseReceipt, DocumentCodeBlockStyle, DocumentEditReceipt,
    DocumentFenceCharacter, DocumentHeadingStyle, DocumentInlineFact, DocumentInlineFactKind,
    DocumentInlineReplacement, DocumentListDelimiter, DocumentListMarker, DocumentLiveViewport,
    DocumentLiveViewportSpan, DocumentPumpReceipt, DocumentQueryReceipt, DocumentSession,
    DocumentSessionError, DocumentSessionPhase, DocumentViewport, DocumentViewportRow,
    DocumentViewportRowContinuityPolicy, DocumentViewportRowEditCapability,
    DocumentViewportRowPresentation, DOCUMENT_INLINE_FACT_CONTINUITY_PLAIN_TEXT,
    DOCUMENT_TABLE_CELL_ALIGNMENT_MASK, DOCUMENT_TABLE_CELL_AUTOCOMPLETED,
    DOCUMENT_TABLE_CELL_HEADER, DOCUMENT_TABLE_CELL_ROW_START,
};
pub use edit_intent::{
    DocumentCommittedSpliceV1, DocumentEditIntentDispositionV1, DocumentEditIntentReceiptV1,
    DocumentEditIntentV1, DocumentEditPresentationTransitionV1, DocumentSourceTransactionReceiptV1,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum StatusCode {
    Ok = 0x0000,
    NeedsInput = 0x0001,
    NeedsOutputBuffer = 0x0002,
    NotCertified = 0x0003,
    Cancelled = 0x0004,
    Superseded = 0x0005,
    BudgetExhausted = 0x0006,
    ResultCapReached = 0x0007,
    NotReadySourceGap = 0x0008,
    Backpressure = 0x0009,
    InvalidArgument = 0x0100,
    UnsupportedAbiVersion = 0x0101,
    UnsupportedCapability = 0x0102,
    InvalidHandle = 0x0103,
    StaleHandle = 0x0104,
    WrongHandleKind = 0x0105,
    OwnerMismatch = 0x0106,
    SessionBusy = 0x0107,
    SessionClosing = 0x0108,
    SessionClosed = 0x0109,
    MigrationWhileActive = 0x010a,
    StaleRevision = 0x0200,
    StaleSnapshot = 0x0201,
    StaleContinuation = 0x0202,
    InvalidUtf8 = 0x0203,
    RangeOutOfBounds = 0x0204,
    RangeNotScalarBoundary = 0x0205,
    CoordinateOutOfRange = 0x0206,
    EditTooLarge = 0x0207,
    QueryLimitExceeded = 0x0208,
    BufferTooSmall = 0x0209,
    StaleProgressToken = 0x020a,
    InvalidUtf16HostInput = 0x020b,
    TransactionConflict = 0x0300,
    TransactionIncomplete = 0x0301,
    TransactionAlreadyCommitted = 0x0302,
    TransactionAlreadyAborted = 0x0303,
    HistoryBudgetExceeded = 0x0304,
    HistoryTokenEvicted = 0x0305,
    HistoryTokenStale = 0x0306,
    ContinuationLimitExceeded = 0x0307,
    ProgressStalled = 0x0400,
    PanicContained = 0x0401,
    InternalFault = 0x0402,
    ResourceLimitExceeded = 0x0403,
    CloseIncomplete = 0x0404,
    ParserFault = 0x0405,
    AllocationFailure = 0x0406,
}

pub const STATUS_CODES: &[(&str, u32)] = &[
    ("OK", StatusCode::Ok as u32),
    ("NEEDS_INPUT", StatusCode::NeedsInput as u32),
    ("NEEDS_OUTPUT_BUFFER", StatusCode::NeedsOutputBuffer as u32),
    ("NOT_CERTIFIED", StatusCode::NotCertified as u32),
    ("CANCELLED", StatusCode::Cancelled as u32),
    ("SUPERSEDED", StatusCode::Superseded as u32),
    ("BUDGET_EXHAUSTED", StatusCode::BudgetExhausted as u32),
    ("RESULT_CAP_REACHED", StatusCode::ResultCapReached as u32),
    ("NOT_READY_SOURCE_GAP", StatusCode::NotReadySourceGap as u32),
    ("BACKPRESSURE", StatusCode::Backpressure as u32),
    ("INVALID_ARGUMENT", StatusCode::InvalidArgument as u32),
    (
        "UNSUPPORTED_ABI_VERSION",
        StatusCode::UnsupportedAbiVersion as u32,
    ),
    (
        "UNSUPPORTED_CAPABILITY",
        StatusCode::UnsupportedCapability as u32,
    ),
    ("INVALID_HANDLE", StatusCode::InvalidHandle as u32),
    ("STALE_HANDLE", StatusCode::StaleHandle as u32),
    ("WRONG_HANDLE_KIND", StatusCode::WrongHandleKind as u32),
    ("OWNER_MISMATCH", StatusCode::OwnerMismatch as u32),
    ("SESSION_BUSY", StatusCode::SessionBusy as u32),
    ("SESSION_CLOSING", StatusCode::SessionClosing as u32),
    ("SESSION_CLOSED", StatusCode::SessionClosed as u32),
    (
        "MIGRATION_WHILE_ACTIVE",
        StatusCode::MigrationWhileActive as u32,
    ),
    ("STALE_REVISION", StatusCode::StaleRevision as u32),
    ("STALE_SNAPSHOT", StatusCode::StaleSnapshot as u32),
    ("STALE_CONTINUATION", StatusCode::StaleContinuation as u32),
    ("INVALID_UTF8", StatusCode::InvalidUtf8 as u32),
    ("RANGE_OUT_OF_BOUNDS", StatusCode::RangeOutOfBounds as u32),
    (
        "RANGE_NOT_SCALAR_BOUNDARY",
        StatusCode::RangeNotScalarBoundary as u32,
    ),
    (
        "COORDINATE_OUT_OF_RANGE",
        StatusCode::CoordinateOutOfRange as u32,
    ),
    ("EDIT_TOO_LARGE", StatusCode::EditTooLarge as u32),
    (
        "QUERY_LIMIT_EXCEEDED",
        StatusCode::QueryLimitExceeded as u32,
    ),
    ("BUFFER_TOO_SMALL", StatusCode::BufferTooSmall as u32),
    (
        "STALE_PROGRESS_TOKEN",
        StatusCode::StaleProgressToken as u32,
    ),
    (
        "INVALID_UTF16_HOST_INPUT",
        StatusCode::InvalidUtf16HostInput as u32,
    ),
    (
        "TRANSACTION_CONFLICT",
        StatusCode::TransactionConflict as u32,
    ),
    (
        "TRANSACTION_INCOMPLETE",
        StatusCode::TransactionIncomplete as u32,
    ),
    (
        "TRANSACTION_ALREADY_COMMITTED",
        StatusCode::TransactionAlreadyCommitted as u32,
    ),
    (
        "TRANSACTION_ALREADY_ABORTED",
        StatusCode::TransactionAlreadyAborted as u32,
    ),
    (
        "HISTORY_BUDGET_EXCEEDED",
        StatusCode::HistoryBudgetExceeded as u32,
    ),
    (
        "HISTORY_TOKEN_EVICTED",
        StatusCode::HistoryTokenEvicted as u32,
    ),
    ("HISTORY_TOKEN_STALE", StatusCode::HistoryTokenStale as u32),
    (
        "CONTINUATION_LIMIT_EXCEEDED",
        StatusCode::ContinuationLimitExceeded as u32,
    ),
    ("PROGRESS_STALLED", StatusCode::ProgressStalled as u32),
    ("PANIC_CONTAINED", StatusCode::PanicContained as u32),
    ("INTERNAL_FAULT", StatusCode::InternalFault as u32),
    (
        "RESOURCE_LIMIT_EXCEEDED",
        StatusCode::ResourceLimitExceeded as u32,
    ),
    ("CLOSE_INCOMPLETE", StatusCode::CloseIncomplete as u32),
    ("PARSER_FAULT", StatusCode::ParserFault as u32),
    ("ALLOCATION_FAILURE", StatusCode::AllocationFailure as u32),
];

pub const fn status_allows_progress(status: StatusCode, progress: ProgressState) -> bool {
    match status {
        StatusCode::Ok => matches!(progress, ProgressState::Advanced | ProgressState::Complete),
        StatusCode::NeedsInput | StatusCode::NeedsOutputBuffer => false,
        StatusCode::NotCertified => matches!(progress, ProgressState::Complete),
        StatusCode::Cancelled => matches!(progress, ProgressState::Cancelled),
        StatusCode::Superseded => matches!(progress, ProgressState::Superseded),
        StatusCode::BudgetExhausted => matches!(progress, ProgressState::BudgetExhausted),
        StatusCode::ResultCapReached => matches!(progress, ProgressState::ResultCapReached),
        StatusCode::NotReadySourceGap => matches!(progress, ProgressState::PendingSourceGap),
        StatusCode::Backpressure => matches!(progress, ProgressState::Backpressured),
        StatusCode::ProgressStalled
        | StatusCode::PanicContained
        | StatusCode::InternalFault
        | StatusCode::ParserFault
        | StatusCode::AllocationFailure => matches!(progress, ProgressState::Fault),
        StatusCode::InvalidArgument
        | StatusCode::UnsupportedAbiVersion
        | StatusCode::UnsupportedCapability
        | StatusCode::InvalidHandle
        | StatusCode::StaleHandle
        | StatusCode::WrongHandleKind
        | StatusCode::OwnerMismatch
        | StatusCode::SessionBusy
        | StatusCode::SessionClosing
        | StatusCode::MigrationWhileActive
        | StatusCode::StaleRevision
        | StatusCode::StaleSnapshot
        | StatusCode::StaleContinuation
        | StatusCode::InvalidUtf8
        | StatusCode::RangeOutOfBounds
        | StatusCode::RangeNotScalarBoundary
        | StatusCode::CoordinateOutOfRange
        | StatusCode::EditTooLarge
        | StatusCode::QueryLimitExceeded
        | StatusCode::BufferTooSmall
        | StatusCode::StaleProgressToken
        | StatusCode::InvalidUtf16HostInput
        | StatusCode::TransactionConflict
        | StatusCode::TransactionIncomplete
        | StatusCode::TransactionAlreadyCommitted
        | StatusCode::TransactionAlreadyAborted
        | StatusCode::HistoryTokenEvicted
        | StatusCode::HistoryTokenStale
        | StatusCode::ContinuationLimitExceeded
        | StatusCode::ResourceLimitExceeded
        | StatusCode::CloseIncomplete => matches!(progress, ProgressState::None),
        // These numeric codes remain reserved for ABI stability, but v4.0 has
        // no legal operation that returns them. Closed handles are stale or
        // invalid, and unavailable history storage is a successful commit
        // disposition rather than a source-edit failure.
        StatusCode::SessionClosed | StatusCode::HistoryBudgetExceeded => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum OperationCode {
    Negotiate = 0,
    CreateBegin = 1,
    CreateAppend = 2,
    CreateCommit = 3,
    CreateAbort = 4,
    SourceRead = 5,
    SmallEdit = 6,
    BulkBegin = 7,
    BulkAppend = 8,
    BulkCommit = 9,
    BulkAbort = 10,
    Pump = 11,
    QueryViewport = 12,
    ContinuationNext = 13,
    ContinuationRelease = 14,
    AnchorCreate = 15,
    AnchorTransform = 16,
    AnchorResolve = 17,
    AnchorRelease = 18,
    CoordinateConvert = 19,
    HistoryReplay = 20,
    HistoryRelease = 21,
    Cancel = 22,
    CloseBegin = 23,
    ClosePump = 24,
    CloseFinish = 25,
    SessionTransferOwner = 26,
    SessionInspect = 27,
    EditIntentV1 = 28,
    SourceTransactionV1 = 29,
}

impl OperationCode {
    /// Operations whose caller-owned output begins with the fixed ABI result
    /// page header, including a header-only empty page.
    pub const fn produces_result_page(self) -> bool {
        matches!(
            self,
            Self::SourceRead | Self::QueryViewport | Self::ContinuationNext
        )
    }
}

pub const OPERATION_CODES: &[(&str, u32)] = &[
    ("NEGOTIATE", OperationCode::Negotiate as u32),
    ("CREATE_BEGIN", OperationCode::CreateBegin as u32),
    ("CREATE_APPEND", OperationCode::CreateAppend as u32),
    ("CREATE_COMMIT", OperationCode::CreateCommit as u32),
    ("CREATE_ABORT", OperationCode::CreateAbort as u32),
    ("SOURCE_READ", OperationCode::SourceRead as u32),
    ("SMALL_EDIT", OperationCode::SmallEdit as u32),
    ("BULK_BEGIN", OperationCode::BulkBegin as u32),
    ("BULK_APPEND", OperationCode::BulkAppend as u32),
    ("BULK_COMMIT", OperationCode::BulkCommit as u32),
    ("BULK_ABORT", OperationCode::BulkAbort as u32),
    ("PUMP", OperationCode::Pump as u32),
    ("QUERY_VIEWPORT", OperationCode::QueryViewport as u32),
    ("CONTINUATION_NEXT", OperationCode::ContinuationNext as u32),
    (
        "CONTINUATION_RELEASE",
        OperationCode::ContinuationRelease as u32,
    ),
    ("ANCHOR_CREATE", OperationCode::AnchorCreate as u32),
    ("ANCHOR_TRANSFORM", OperationCode::AnchorTransform as u32),
    ("ANCHOR_RESOLVE", OperationCode::AnchorResolve as u32),
    ("ANCHOR_RELEASE", OperationCode::AnchorRelease as u32),
    (
        "COORDINATE_CONVERT",
        OperationCode::CoordinateConvert as u32,
    ),
    ("HISTORY_REPLAY", OperationCode::HistoryReplay as u32),
    ("HISTORY_RELEASE", OperationCode::HistoryRelease as u32),
    ("CANCEL", OperationCode::Cancel as u32),
    ("CLOSE_BEGIN", OperationCode::CloseBegin as u32),
    ("CLOSE_PUMP", OperationCode::ClosePump as u32),
    ("CLOSE_FINISH", OperationCode::CloseFinish as u32),
    (
        "SESSION_TRANSFER_OWNER",
        OperationCode::SessionTransferOwner as u32,
    ),
    ("SESSION_INSPECT", OperationCode::SessionInspect as u32),
    ("EDIT_INTENT_V1", OperationCode::EditIntentV1 as u32),
    (
        "SOURCE_TRANSACTION_V1",
        OperationCode::SourceTransactionV1 as u32,
    ),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum ProgressState {
    None = 0,
    Advanced = 1,
    BudgetExhausted = 2,
    ResultCapReached = 3,
    PendingSourceGap = 4,
    Backpressured = 5,
    Complete = 6,
    Cancelled = 7,
    Superseded = 8,
    Fault = 9,
}

pub const PROGRESS_STATES: &[(&str, u32)] = &[
    ("NONE", ProgressState::None as u32),
    ("ADVANCED", ProgressState::Advanced as u32),
    ("BUDGET_EXHAUSTED", ProgressState::BudgetExhausted as u32),
    ("RESULT_CAP_REACHED", ProgressState::ResultCapReached as u32),
    ("PENDING_SOURCE_GAP", ProgressState::PendingSourceGap as u32),
    ("BACKPRESSURED", ProgressState::Backpressured as u32),
    ("COMPLETE", ProgressState::Complete as u32),
    ("CANCELLED", ProgressState::Cancelled as u32),
    ("SUPERSEDED", ProgressState::Superseded as u32),
    ("FAULT", ProgressState::Fault as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum SessionState {
    Creating = 1,
    Open = 2,
    Closing = 3,
    Closed = 4,
    Faulted = 5,
}

pub const SESSION_STATES: &[(&str, u32)] = &[
    ("CREATING", SessionState::Creating as u32),
    ("OPEN", SessionState::Open as u32),
    ("CLOSING", SessionState::Closing as u32),
    ("CLOSED", SessionState::Closed as u32),
    ("FAULTED", SessionState::Faulted as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum TransactionState {
    Staging = 1,
    Committed = 2,
    Aborted = 3,
}

pub const TRANSACTION_STATES: &[(&str, u32)] = &[
    ("STAGING", TransactionState::Staging as u32),
    ("COMMITTED", TransactionState::Committed as u32),
    ("ABORTED", TransactionState::Aborted as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum CertificationState {
    NotApplicable = 0,
    PendingNeutral = 1,
    CurrentCertified = 2,
    MixedCurrent = 3,
}

pub const CERTIFICATION_STATES: &[(&str, u32)] = &[
    ("NOT_APPLICABLE", CertificationState::NotApplicable as u32),
    ("PENDING_NEUTRAL", CertificationState::PendingNeutral as u32),
    (
        "CURRENT_CERTIFIED",
        CertificationState::CurrentCertified as u32,
    ),
    ("MIXED_CURRENT", CertificationState::MixedCurrent as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum ParserProfile {
    CommonMark0312 = 1,
    FlarkGfm029V1 = 2,
}

impl Default for ParserProfile {
    fn default() -> Self {
        Self::FlarkGfm029V1
    }
}

pub const PARSER_PROFILES: &[(&str, u32)] = &[
    ("COMMONMARK_0_31_2", ParserProfile::CommonMark0312 as u32),
    ("FLARK_GFM_0_29_V1", ParserProfile::FlarkGfm029V1 as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum CoordinateKind {
    SourceByte = 1,
    Utf16CodeUnit = 2,
}

pub const COORDINATE_KINDS: &[(&str, u32)] = &[
    ("SOURCE_BYTE", CoordinateKind::SourceByte as u32),
    ("UTF16_CODE_UNIT", CoordinateKind::Utf16CodeUnit as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum Affinity {
    Upstream = 1,
    Downstream = 2,
}

pub const AFFINITIES: &[(&str, u32)] = &[
    ("UPSTREAM", Affinity::Upstream as u32),
    ("DOWNSTREAM", Affinity::Downstream as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum QueryKind {
    Source = 1,
    Semantic = 2,
    SourceAndSemantic = 3,
}

pub const QUERY_KINDS: &[(&str, u32)] = &[
    ("SOURCE", QueryKind::Source as u32),
    ("SEMANTIC", QueryKind::Semantic as u32),
    ("SOURCE_AND_SEMANTIC", QueryKind::SourceAndSemantic as u32),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum ResultRecordKind {
    SourceBytes = 1,
    SemanticFacts = 2,
    SourceAndSemantic = 3,
}

pub const RESULT_RECORD_KINDS: &[(&str, u32)] = &[
    ("SOURCE_BYTES", ResultRecordKind::SourceBytes as u32),
    ("SEMANTIC_FACTS", ResultRecordKind::SemanticFacts as u32),
    (
        "SOURCE_AND_SEMANTIC",
        ResultRecordKind::SourceAndSemantic as u32,
    ),
];

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum HistoryDisposition {
    NotApplicable = 0,
    Retained = 1,
    Disabled = 2,
    OverBudget = 3,
}

pub const HISTORY_DISPOSITIONS: &[(&str, u32)] = &[
    ("NOT_APPLICABLE", HistoryDisposition::NotApplicable as u32),
    ("RETAINED", HistoryDisposition::Retained as u32),
    ("DISABLED", HistoryDisposition::Disabled as u32),
    ("OVER_BUDGET", HistoryDisposition::OverBudget as u32),
];

pub const HANDLE_KINDS: &[(&str, u32)] = &[
    ("NONE", 0),
    ("SESSION", 1),
    ("TRANSACTION", 2),
    ("CONTINUATION", 3),
    ("ANCHOR", 4),
    ("HISTORY_TOKEN", 5),
];

pub const OWNERSHIP_KINDS: &[(&str, u32)] = &[
    ("BORROWED_INPUT", 1),
    ("CALLER_OUTPUT", 2),
    ("RUNTIME_HANDLE", 3),
    ("RUNTIME_STAGED_BYTES", 4),
];

pub const CAPABILITY_BITS: &[(&str, u64)] = &[
    ("SOURCE_STREAMING", 1 << 0),
    ("SMALL_EDITS", 1 << 1),
    ("BULK_TRANSACTIONS", 1 << 2),
    ("BOUNDED_PUMP", 1 << 3),
    ("RANGE_CERTIFICATION", 1 << 4),
    ("VIEWPORT_QUERIES", 1 << 5),
    ("STABLE_ANCHORS", 1 << 6),
    ("COORDINATE_CONVERSION", 1 << 7),
    ("REVERSIBLE_HISTORY", 1 << 8),
    ("RESUMABLE_CLOSE", 1 << 9),
    ("SNAPSHOT_CONTINUATIONS", 1 << 10),
    ("CANCELLATION", 1 << 11),
    ("PANIC_CONTAINMENT", 1 << 12),
    ("COMMONMARK_0_31_2", 1 << 13),
    ("SELECTED_GFM_V1", 1 << 14),
    ("EDIT_INTENTS_V1", 1 << 15),
    ("SOURCE_TRANSACTIONS_V1", 1 << 16),
    ("NATIVE_COMPOSITE_HISTORY_V1", 1 << 17),
];

pub const MAX_SMALL_EDIT_BYTES: u32 = 4096;
pub const MAX_BULK_CHUNK_BYTES: u32 = 65_536;
pub const MAX_SOURCE_CHUNK_BYTES: u32 = 65_536;
pub const MAX_RESULT_BYTES: u32 = 262_144;
pub const MAX_QUERY_ITEMS: u32 = 4096;
pub const MAX_TRANSACTION_EDITS: u32 = 64;
/// Hard cap on live anchors per session. Anchors are transformed eagerly on
/// every committed edit, so this bound is what keeps edit admission's anchor
/// maintenance a bounded synchronous cost.
pub const MAX_LIVE_ANCHORS: u32 = 4096;

macro_rules! opaque_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(pub u64);
    };
}

opaque_id!(SessionHandle);
opaque_id!(TransactionHandle);
opaque_id!(ContinuationHandle);
opaque_id!(AnchorHandle);
opaque_id!(HistoryToken);
opaque_id!(SnapshotId);
opaque_id!(Revision);
opaque_id!(ProgressToken);
opaque_id!(OwnerToken);

impl ContinuationHandle {
    /// No retained result-page continuation. Legal only in fields whose
    /// operation-specific rule names zero as `NONE`.
    pub const NONE: Self = Self(0);
}

impl AnchorHandle {
    /// No pre-existing anchor. Used only by `ANCHOR_CREATE` and fields marked
    /// unused by an operation-specific request rule.
    pub const NONE: Self = Self(0);
}

impl HistoryToken {
    /// No reversible history payload was retained for this committed edit.
    pub const NONE: Self = Self(0);
}

impl SnapshotId {
    /// Select and pin the latest snapshot for an initial viewport query.
    pub const LATEST: Self = Self(0);
    /// No snapshot participates in this operation, including `SOURCE_READ`.
    pub const NOT_APPLICABLE: Self = Self(0);
}

impl Revision {
    /// No committed revision exists yet. Legal only for provisional document
    /// creation commit/abort requests.
    pub const UNCOMMITTED: Self = Self(0);
}

impl ProgressToken {
    /// Begin a progress state machine. Pump/finalize requests require the
    /// latest nonzero token returned by the preceding receipt.
    pub const NONE: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionRef {
    pub session: SessionHandle,
    pub owner: OwnerToken,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceRange {
    pub start_byte: u64,
    pub end_byte: u64,
}

impl SourceRange {
    pub const fn is_well_formed(self) -> bool {
        self.start_byte <= self.end_byte
    }

    pub const fn contains(self, other: Self) -> bool {
        self.is_well_formed()
            && other.is_well_formed()
            && self.start_byte <= other.start_byte
            && other.end_byte <= self.end_byte
    }

    pub const fn byte_len(self) -> u64 {
        self.end_byte.saturating_sub(self.start_byte)
    }
}

/// One splice in a revision-checked atomic small-edit transaction. Descriptor
/// ranges are sorted and non-overlapping in the base revision. Replacement
/// offsets address the transaction's packed replacement byte slice.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditDescriptor {
    pub start_byte: u64,
    pub end_byte: u64,
    pub replacement_offset: u64,
    pub replacement_len: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkBudget {
    pub max_work_units: u64,
    /// Host scheduling hint recorded by the host in its call receipt. The
    /// runtime-enforced bound is `max_work_units`; this value is not echoed in
    /// the runtime `Outcome`.
    pub advisory_max_micros: u64,
    pub max_result_items: u32,
    pub max_result_bytes: u32,
}

impl WorkBudget {
    /// Every budgeted call must admit at least one bounded work unit. Result
    /// caps are always bounded by the frozen runtime maxima, even when a
    /// particular operation does not consume them.
    pub const fn is_contract_valid(self) -> bool {
        self.max_work_units != 0
            && self.max_result_items <= MAX_QUERY_ITEMS
            && self.max_result_bytes <= MAX_RESULT_BYTES
    }

    /// Page-producing pump calls must admit at least one item and one payload
    /// byte. Empty result pages remain legal; these nonzero caps prevent a
    /// nonempty query from entering a zero-progress result-cap loop.
    pub const fn is_page_contract_valid(self) -> bool {
        self.is_contract_valid() && self.max_result_items != 0 && self.max_result_bytes != 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionConfig {
    pub parser_profile: ParserProfile,
    pub history_budget_bytes: u64,
    pub max_document_bytes: u64,
    pub flags: u64,
}

/// Host-neutral authority for the fixed result-page header. The runtime owns
/// the values and payload semantics; `flark-abi` alone encodes them into the C
/// record that precedes the caller-owned payload buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResultPageReceipt {
    pub record_kind: ResultRecordKind,
    pub certification: CertificationState,
    pub revision: Revision,
    pub snapshot: SnapshotId,
    pub requested_range: SourceRange,
    pub covered_range: SourceRange,
    pub item_count: u32,
    pub payload_bytes: u32,
    pub continuation: ContinuationHandle,
}

impl ResultPageReceipt {
    /// Checks the cross-layer invariants that can be proven without decoding
    /// the payload. Kind-specific semantic record validation remains mandatory
    /// while the payload is produced.
    pub const fn is_contract_valid(self) -> bool {
        if self.revision.0 == Revision::UNCOMMITTED.0
            || !self.requested_range.contains(self.covered_range)
            || self.item_count > MAX_QUERY_ITEMS
            || self.payload_bytes > MAX_RESULT_BYTES
        {
            return false;
        }

        match (self.record_kind, self.certification) {
            (ResultRecordKind::SourceBytes, CertificationState::NotApplicable) => {
                let payload_matches_source =
                    self.covered_range.byte_len() == self.payload_bytes as u64;
                let source_read_shape = self.snapshot.0 == SnapshotId::NOT_APPLICABLE.0
                    && self.continuation.0 == ContinuationHandle::NONE.0
                    && self.payload_bytes <= MAX_SOURCE_CHUNK_BYTES;
                let pinned_query_shape = self.snapshot.0 != SnapshotId::NOT_APPLICABLE.0;
                self.item_count == 0
                    && payload_matches_source
                    && (source_read_shape || pinned_query_shape)
            }
            (ResultRecordKind::SourceBytes, CertificationState::PendingNeutral) => {
                self.snapshot.0 != SnapshotId::NOT_APPLICABLE.0
                    && self.item_count == 0
                    && self.covered_range.byte_len() == self.payload_bytes as u64
            }
            (ResultRecordKind::SemanticFacts, CertificationState::CurrentCertified)
            | (ResultRecordKind::SourceAndSemantic, CertificationState::PendingNeutral)
            | (ResultRecordKind::SourceAndSemantic, CertificationState::CurrentCertified)
            | (ResultRecordKind::SourceAndSemantic, CertificationState::MixedCurrent) => {
                self.snapshot.0 != SnapshotId::NOT_APPLICABLE.0
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionInspectionReceipt {
    pub session: SessionHandle,
    pub state: SessionState,
    pub revision: Revision,
    pub live_transactions: u32,
    pub live_continuations: u32,
    pub live_anchors: u32,
    pub live_history_tokens: u32,
}

/// Typed runtime result authority. `flark-abi` maps these variants into the
/// generic fixed-width C `Outcome` fields according to the manifest's
/// `outcomeFieldRoles`; runtime implementations never invent raw field roles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationResult {
    None,
    SessionCreated {
        session: SessionHandle,
        transaction: TransactionHandle,
    },
    TransactionStaged {
        transaction: TransactionHandle,
    },
    RevisionCreated {
        session: SessionHandle,
        revision: Revision,
    },
    RevisionCommitted {
        revision: Revision,
        history_token: HistoryToken,
        history: HistoryDisposition,
    },
    Page(ResultPageReceipt),
    Progress {
        revision: Revision,
        token: ProgressToken,
    },
    CloseProgress {
        token: ProgressToken,
    },
    Anchor {
        anchor: AnchorHandle,
        revision: Revision,
    },
    AnchorPosition {
        anchor: AnchorHandle,
        revision: Revision,
        coordinate: CoordinateKind,
        position: u64,
    },
    ConvertedPosition {
        revision: Revision,
        coordinate: CoordinateKind,
        position: u64,
    },
    OwnerTransferred {
        session: SessionHandle,
    },
    SessionInspection(SessionInspectionReceipt),
}

impl OperationResult {
    pub const fn is_valid_for(self, operation: OperationCode) -> bool {
        match self {
            Self::None => true,
            Self::SessionCreated {
                session,
                transaction,
            } => {
                matches!(operation, OperationCode::CreateBegin)
                    && session.0 != 0
                    && transaction.0 != 0
            }
            Self::TransactionStaged { transaction } => {
                matches!(
                    operation,
                    OperationCode::CreateAppend
                        | OperationCode::BulkBegin
                        | OperationCode::BulkAppend
                ) && transaction.0 != 0
            }
            Self::RevisionCreated { session, revision } => {
                matches!(operation, OperationCode::CreateCommit)
                    && session.0 != 0
                    && revision.0 != Revision::UNCOMMITTED.0
            }
            Self::RevisionCommitted {
                revision,
                history_token,
                history,
            } => {
                let history_is_coherent = match history {
                    HistoryDisposition::Retained => history_token.0 != HistoryToken::NONE.0,
                    HistoryDisposition::Disabled | HistoryDisposition::OverBudget => {
                        history_token.0 == HistoryToken::NONE.0
                    }
                    HistoryDisposition::NotApplicable => false,
                };
                matches!(
                    operation,
                    OperationCode::SmallEdit
                        | OperationCode::BulkCommit
                        | OperationCode::HistoryReplay
                        | OperationCode::EditIntentV1
                        | OperationCode::SourceTransactionV1
                ) && revision.0 != Revision::UNCOMMITTED.0
                    && history_is_coherent
            }
            Self::Page(page) => {
                matches!(
                    operation,
                    OperationCode::SourceRead
                        | OperationCode::QueryViewport
                        | OperationCode::ContinuationNext
                ) && page.is_contract_valid()
            }
            Self::Progress { revision, token } => {
                matches!(
                    operation,
                    OperationCode::CreateCommit
                        | OperationCode::BulkCommit
                        | OperationCode::Pump
                        | OperationCode::AnchorCreate
                        | OperationCode::AnchorTransform
                        | OperationCode::AnchorResolve
                        | OperationCode::CoordinateConvert
                        | OperationCode::HistoryReplay
                        | OperationCode::Cancel
                ) && token.0 != ProgressToken::NONE.0
                    && (matches!(operation, OperationCode::CreateCommit)
                        || revision.0 != Revision::UNCOMMITTED.0)
            }
            Self::CloseProgress { token } => {
                matches!(
                    operation,
                    OperationCode::CloseBegin
                        | OperationCode::ClosePump
                        | OperationCode::CloseFinish
                ) && token.0 != ProgressToken::NONE.0
            }
            Self::Anchor { anchor, revision } => {
                matches!(
                    operation,
                    OperationCode::AnchorCreate | OperationCode::AnchorTransform
                ) && anchor.0 != AnchorHandle::NONE.0
                    && revision.0 != Revision::UNCOMMITTED.0
            }
            Self::AnchorPosition {
                anchor, revision, ..
            } => {
                matches!(operation, OperationCode::AnchorResolve)
                    && anchor.0 != AnchorHandle::NONE.0
                    && revision.0 != Revision::UNCOMMITTED.0
            }
            Self::ConvertedPosition { revision, .. } => {
                matches!(operation, OperationCode::CoordinateConvert)
                    && revision.0 != Revision::UNCOMMITTED.0
            }
            Self::OwnerTransferred { session } => {
                matches!(operation, OperationCode::SessionTransferOwner) && session.0 != 0
            }
            Self::SessionInspection(inspection) => {
                matches!(operation, OperationCode::SessionInspect)
                    && inspection.session.0 != 0
                    && (matches!(inspection.state, SessionState::Creating)
                        || inspection.revision.0 != Revision::UNCOMMITTED.0)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub operation: OperationCode,
    pub status: StatusCode,
    pub progress: ProgressState,
    /// Host-neutral payload requirement, excluding any C ABI record/header.
    pub required_payload_bytes: u64,
    /// Host-neutral payload bytes produced, excluding the C result-page header.
    pub written_payload_bytes: u64,
    pub result: OperationResult,
}

impl Outcome {
    pub const fn is_contract_valid(self) -> bool {
        if !status_allows_progress(self.status, self.progress)
            || !self.result.is_valid_for(self.operation)
            || (self.required_payload_bytes != 0
                && !matches!(self.status, StatusCode::BufferTooSmall))
            || (matches!(self.status, StatusCode::BufferTooSmall)
                && self.required_payload_bytes == 0
                && !self.operation.produces_result_page())
        {
            return false;
        }
        if !outcome_shape_is_contract_valid(self.operation, self.status, self.progress, self.result)
        {
            return false;
        }
        match self.result {
            OperationResult::Page(page) => self.written_payload_bytes == page.payload_bytes as u64,
            _ if matches!(
                self.operation,
                OperationCode::EditIntentV1 | OperationCode::SourceTransactionV1
            ) && matches!(self.status, StatusCode::Ok) =>
            {
                self.written_payload_bytes != 0
                    && self.written_payload_bytes <= MAX_RESULT_BYTES as u64
            }
            _ => self.written_payload_bytes == 0,
        }
    }
}

const fn outcome_shape_is_contract_valid(
    operation: OperationCode,
    status: StatusCode,
    progress: ProgressState,
    result: OperationResult,
) -> bool {
    match status {
        StatusCode::NeedsInput
        | StatusCode::NeedsOutputBuffer
        | StatusCode::SessionClosed
        | StatusCode::HistoryBudgetExceeded => false,
        StatusCode::Ok => match progress {
            ProgressState::Advanced => operation_accepts_progress(operation, result),
            ProgressState::Complete => operation_accepts_terminal_success(operation, result),
            _ => false,
        },
        StatusCode::NotCertified => {
            matches!(progress, ProgressState::Complete)
                && page_result_is_contract_valid_for(operation, status, result)
        }
        StatusCode::Cancelled => {
            matches!(progress, ProgressState::Cancelled)
                && operation_accepts_progress(operation, result)
        }
        StatusCode::Superseded => {
            matches!(progress, ProgressState::Superseded)
                && operation_accepts_progress(operation, result)
        }
        StatusCode::BudgetExhausted => {
            matches!(progress, ProgressState::BudgetExhausted)
                && (operation_accepts_progress(operation, result)
                    || operation_accepts_close_progress(operation, result))
        }
        StatusCode::ResultCapReached => {
            matches!(progress, ProgressState::ResultCapReached)
                && page_result_is_contract_valid_for(operation, status, result)
        }
        StatusCode::NotReadySourceGap => {
            matches!(progress, ProgressState::PendingSourceGap)
                && matches!(
                    operation,
                    OperationCode::CreateCommit | OperationCode::BulkCommit
                )
                && operation_accepts_progress(operation, result)
        }
        StatusCode::Backpressure => {
            matches!(progress, ProgressState::Backpressured)
                && matches!(
                    operation,
                    OperationCode::SmallEdit
                        | OperationCode::EditIntentV1
                        | OperationCode::SourceTransactionV1
                        | OperationCode::HistoryReplay
                        | OperationCode::QueryViewport
                        | OperationCode::ContinuationNext
                )
                && matches!(result, OperationResult::None)
        }
        StatusCode::BufferTooSmall => {
            matches!(progress, ProgressState::None)
                && operation_has_caller_output(operation)
                && matches!(result, OperationResult::None)
        }
        StatusCode::CloseIncomplete => {
            matches!(operation, OperationCode::CloseFinish)
                && matches!(progress, ProgressState::None)
                && matches!(result, OperationResult::CloseProgress { .. })
        }
        StatusCode::ProgressStalled
        | StatusCode::PanicContained
        | StatusCode::InternalFault
        | StatusCode::ParserFault
        | StatusCode::AllocationFailure => {
            matches!(progress, ProgressState::Fault)
                && (matches!(result, OperationResult::None)
                    || operation_accepts_progress(operation, result)
                    || operation_accepts_close_progress(operation, result))
        }
        StatusCode::InvalidArgument
        | StatusCode::UnsupportedAbiVersion
        | StatusCode::UnsupportedCapability
        | StatusCode::InvalidHandle
        | StatusCode::StaleHandle
        | StatusCode::WrongHandleKind
        | StatusCode::OwnerMismatch
        | StatusCode::SessionBusy
        | StatusCode::SessionClosing
        | StatusCode::MigrationWhileActive
        | StatusCode::StaleRevision
        | StatusCode::StaleSnapshot
        | StatusCode::StaleContinuation
        | StatusCode::InvalidUtf8
        | StatusCode::RangeOutOfBounds
        | StatusCode::RangeNotScalarBoundary
        | StatusCode::CoordinateOutOfRange
        | StatusCode::EditTooLarge
        | StatusCode::QueryLimitExceeded
        | StatusCode::StaleProgressToken
        | StatusCode::InvalidUtf16HostInput
        | StatusCode::TransactionConflict
        | StatusCode::TransactionIncomplete
        | StatusCode::TransactionAlreadyCommitted
        | StatusCode::TransactionAlreadyAborted
        | StatusCode::HistoryTokenEvicted
        | StatusCode::HistoryTokenStale
        | StatusCode::ContinuationLimitExceeded
        | StatusCode::ResourceLimitExceeded => {
            matches!(progress, ProgressState::None) && matches!(result, OperationResult::None)
        }
    }
}

const fn operation_accepts_terminal_success(
    operation: OperationCode,
    result: OperationResult,
) -> bool {
    matches!(
        (operation, result),
        (OperationCode::Negotiate, OperationResult::None)
            | (
                OperationCode::CreateBegin,
                OperationResult::SessionCreated { .. }
            )
            | (
                OperationCode::CreateAppend | OperationCode::BulkBegin | OperationCode::BulkAppend,
                OperationResult::TransactionStaged { .. }
            )
            | (
                OperationCode::CreateCommit,
                OperationResult::RevisionCreated { .. }
            )
            | (
                OperationCode::CreateAbort
                    | OperationCode::BulkAbort
                    | OperationCode::ContinuationRelease
                    | OperationCode::AnchorRelease
                    | OperationCode::HistoryRelease
                    | OperationCode::EditIntentV1,
                OperationResult::None
            )
            | (
                OperationCode::SourceRead
                    | OperationCode::QueryViewport
                    | OperationCode::ContinuationNext,
                OperationResult::Page(_)
            )
            | (
                OperationCode::SmallEdit
                    | OperationCode::BulkCommit
                    | OperationCode::HistoryReplay
                    | OperationCode::EditIntentV1
                    | OperationCode::SourceTransactionV1,
                OperationResult::RevisionCommitted { .. }
            )
            | (OperationCode::Pump, OperationResult::Progress { .. })
            | (
                OperationCode::AnchorCreate | OperationCode::AnchorTransform,
                OperationResult::Anchor { .. }
            )
            | (
                OperationCode::AnchorResolve,
                OperationResult::AnchorPosition { .. }
            )
            | (
                OperationCode::CoordinateConvert,
                OperationResult::ConvertedPosition { .. }
            )
            | (
                OperationCode::CloseBegin | OperationCode::ClosePump,
                OperationResult::CloseProgress { .. }
            )
            | (OperationCode::CloseFinish, OperationResult::None)
            | (
                OperationCode::SessionTransferOwner,
                OperationResult::OwnerTransferred { .. }
            )
            | (
                OperationCode::SessionInspect,
                OperationResult::SessionInspection(_)
            )
    ) && match result {
        OperationResult::Page(_) => {
            page_result_is_contract_valid_for(operation, StatusCode::Ok, result)
        }
        _ => true,
    }
}

const fn operation_accepts_progress(operation: OperationCode, result: OperationResult) -> bool {
    matches!(result, OperationResult::Progress { .. })
        && matches!(
            operation,
            OperationCode::CreateCommit
                | OperationCode::BulkCommit
                | OperationCode::Pump
                | OperationCode::AnchorCreate
                | OperationCode::AnchorTransform
                | OperationCode::AnchorResolve
                | OperationCode::CoordinateConvert
                | OperationCode::HistoryReplay
                | OperationCode::Cancel
        )
}

const fn operation_accepts_close_progress(
    operation: OperationCode,
    result: OperationResult,
) -> bool {
    matches!(result, OperationResult::CloseProgress { .. })
        && matches!(
            operation,
            OperationCode::CloseBegin | OperationCode::ClosePump | OperationCode::CloseFinish
        )
}

const fn operation_has_caller_output(operation: OperationCode) -> bool {
    matches!(
        operation,
        OperationCode::Negotiate
            | OperationCode::EditIntentV1
            | OperationCode::SourceTransactionV1
            | OperationCode::SourceRead
            | OperationCode::QueryViewport
            | OperationCode::ContinuationNext
            | OperationCode::SessionInspect
    )
}

const fn page_result_is_contract_valid_for(
    operation: OperationCode,
    status: StatusCode,
    result: OperationResult,
) -> bool {
    let OperationResult::Page(page) = result else {
        return false;
    };
    if !page.is_contract_valid() {
        return false;
    }
    match operation {
        OperationCode::SourceRead => {
            matches!(status, StatusCode::Ok)
                && matches!(page.record_kind, ResultRecordKind::SourceBytes)
                && matches!(page.certification, CertificationState::NotApplicable)
                && page.snapshot.0 == SnapshotId::NOT_APPLICABLE.0
                && page.continuation.0 == ContinuationHandle::NONE.0
        }
        OperationCode::QueryViewport | OperationCode::ContinuationNext => {
            if page.snapshot.0 == SnapshotId::NOT_APPLICABLE.0 {
                return false;
            }
            match status {
                StatusCode::Ok => {
                    page.continuation.0 == ContinuationHandle::NONE.0
                        && matches!(page.certification, CertificationState::CurrentCertified)
                }
                StatusCode::NotCertified => {
                    page.continuation.0 == ContinuationHandle::NONE.0
                        && matches!(
                            (page.record_kind, page.certification),
                            (
                                ResultRecordKind::SourceBytes,
                                CertificationState::PendingNeutral
                            ) | (
                                ResultRecordKind::SourceAndSemantic,
                                CertificationState::PendingNeutral
                                    | CertificationState::MixedCurrent
                            )
                        )
                }
                StatusCode::ResultCapReached => page.continuation.0 != ContinuationHandle::NONE.0,
                _ => false,
            }
        }
        _ => false,
    }
}

/// Typed, exhaustive runtime requests. Input slices are borrowed only for the
/// duration of `dispatch`; the caller owns the output slice for the entire call.
pub enum RuntimeRequest<'a> {
    CreateBegin {
        owner: OwnerToken,
        config: SessionConfig,
        expected_total_bytes: u64,
        first_chunk: &'a [u8],
    },
    CreateAppend {
        session: SessionRef,
        transaction: TransactionHandle,
        chunk_offset: u64,
        chunk: &'a [u8],
    },
    CreateCommit {
        session: SessionRef,
        transaction: TransactionHandle,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    CreateAbort {
        session: SessionRef,
        transaction: TransactionHandle,
        budget: WorkBudget,
    },
    SourceRead {
        session: SessionRef,
        revision: Revision,
        range: SourceRange,
    },
    SmallEdit {
        session: SessionRef,
        expected_revision: Revision,
        edits: &'a [EditDescriptor],
        replacement_bytes: &'a [u8],
        budget: WorkBudget,
    },
    BulkBegin {
        session: SessionRef,
        expected_revision: Revision,
        range: SourceRange,
        expected_total_bytes: u64,
    },
    BulkAppend {
        session: SessionRef,
        transaction: TransactionHandle,
        chunk_offset: u64,
        chunk: &'a [u8],
    },
    BulkCommit {
        session: SessionRef,
        transaction: TransactionHandle,
        expected_revision: Revision,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    BulkAbort {
        session: SessionRef,
        transaction: TransactionHandle,
        expected_revision: Revision,
        budget: WorkBudget,
    },
    Pump {
        session: SessionRef,
        expected_revision: Revision,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    QueryViewport {
        session: SessionRef,
        revision: Revision,
        snapshot: SnapshotId,
        range: SourceRange,
        kind: QueryKind,
        budget: WorkBudget,
    },
    ContinuationNext {
        session: SessionRef,
        revision: Revision,
        snapshot: SnapshotId,
        continuation: ContinuationHandle,
        budget: WorkBudget,
    },
    ContinuationRelease {
        session: SessionRef,
        revision: Revision,
        snapshot: SnapshotId,
        continuation: ContinuationHandle,
        budget: WorkBudget,
    },
    AnchorCreate {
        session: SessionRef,
        revision: Revision,
        position: u64,
        coordinate: CoordinateKind,
        affinity: Affinity,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    AnchorTransform {
        session: SessionRef,
        anchor: AnchorHandle,
        target_revision: Revision,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    AnchorResolve {
        session: SessionRef,
        anchor: AnchorHandle,
        revision: Revision,
        coordinate: CoordinateKind,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    AnchorRelease {
        session: SessionRef,
        anchor: AnchorHandle,
        budget: WorkBudget,
    },
    CoordinateConvert {
        session: SessionRef,
        revision: Revision,
        position: u64,
        from: CoordinateKind,
        to: CoordinateKind,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    HistoryReplay {
        session: SessionRef,
        expected_revision: Revision,
        token: HistoryToken,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    HistoryRelease {
        session: SessionRef,
        expected_revision: Revision,
        token: HistoryToken,
        budget: WorkBudget,
    },
    Cancel {
        session: SessionRef,
        progress_token: ProgressToken,
    },
    CloseBegin {
        session: SessionRef,
        budget: WorkBudget,
    },
    ClosePump {
        session: SessionRef,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    CloseFinish {
        session: SessionRef,
        progress_token: ProgressToken,
        budget: WorkBudget,
    },
    SessionTransferOwner {
        session: SessionRef,
        new_owner: OwnerToken,
    },
    SessionInspect {
        session: SessionRef,
    },
}

impl RuntimeRequest<'_> {
    pub const fn operation(&self) -> OperationCode {
        match self {
            Self::CreateBegin { .. } => OperationCode::CreateBegin,
            Self::CreateAppend { .. } => OperationCode::CreateAppend,
            Self::CreateCommit { .. } => OperationCode::CreateCommit,
            Self::CreateAbort { .. } => OperationCode::CreateAbort,
            Self::SourceRead { .. } => OperationCode::SourceRead,
            Self::SmallEdit { .. } => OperationCode::SmallEdit,
            Self::BulkBegin { .. } => OperationCode::BulkBegin,
            Self::BulkAppend { .. } => OperationCode::BulkAppend,
            Self::BulkCommit { .. } => OperationCode::BulkCommit,
            Self::BulkAbort { .. } => OperationCode::BulkAbort,
            Self::Pump { .. } => OperationCode::Pump,
            Self::QueryViewport { .. } => OperationCode::QueryViewport,
            Self::ContinuationNext { .. } => OperationCode::ContinuationNext,
            Self::ContinuationRelease { .. } => OperationCode::ContinuationRelease,
            Self::AnchorCreate { .. } => OperationCode::AnchorCreate,
            Self::AnchorTransform { .. } => OperationCode::AnchorTransform,
            Self::AnchorResolve { .. } => OperationCode::AnchorResolve,
            Self::AnchorRelease { .. } => OperationCode::AnchorRelease,
            Self::CoordinateConvert { .. } => OperationCode::CoordinateConvert,
            Self::HistoryReplay { .. } => OperationCode::HistoryReplay,
            Self::HistoryRelease { .. } => OperationCode::HistoryRelease,
            Self::Cancel { .. } => OperationCode::Cancel,
            Self::CloseBegin { .. } => OperationCode::CloseBegin,
            Self::ClosePump { .. } => OperationCode::ClosePump,
            Self::CloseFinish { .. } => OperationCode::CloseFinish,
            Self::SessionTransferOwner { .. } => OperationCode::SessionTransferOwner,
            Self::SessionInspect { .. } => OperationCode::SessionInspect,
        }
    }

    /// Proves that an atomic `SMALL_EDIT` cannot hide document-sized source
    /// deletion or inverse-history work behind a tiny descriptor buffer. The
    /// complete synchronous envelope includes descriptor records,
    /// replacement bytes, and every deleted source byte. Larger work must use
    /// the resumable bulk transaction path.
    pub fn small_edit_envelope_is_contract_valid(&self) -> bool {
        let Self::SmallEdit {
            edits,
            replacement_bytes,
            ..
        } = self
        else {
            return true;
        };
        if edits.is_empty() || edits.len() > MAX_TRANSACTION_EDITS as usize {
            return false;
        }

        let mut previous_end = 0_u64;
        let mut next_replacement_offset = 0_u64;
        let mut deleted_source_bytes = 0_u64;
        for (index, edit) in edits.iter().enumerate() {
            if edit.start_byte > edit.end_byte || (index != 0 && edit.start_byte < previous_end) {
                return false;
            }
            previous_end = edit.end_byte;
            let Some(next_deleted_bytes) =
                deleted_source_bytes.checked_add(edit.end_byte - edit.start_byte)
            else {
                return false;
            };
            deleted_source_bytes = next_deleted_bytes;

            // Replacement storage is one packed descriptor-order partition.
            // Aliasing, gaps, and out-of-order slices would otherwise let many
            // descriptors amplify the same small backing buffer into work far
            // beyond the synchronous envelope.
            if edit.replacement_offset != next_replacement_offset {
                return false;
            }
            let Some(replacement_end) = edit.replacement_offset.checked_add(edit.replacement_len)
            else {
                return false;
            };
            if replacement_end > replacement_bytes.len() as u64 {
                return false;
            }
            next_replacement_offset = replacement_end;
        }
        if next_replacement_offset != replacement_bytes.len() as u64 {
            return false;
        }

        let Some(descriptor_bytes) = (edits.len() as u64).checked_mul(32) else {
            return false;
        };
        descriptor_bytes
            .checked_add(replacement_bytes.len() as u64)
            .and_then(|bytes| bytes.checked_add(deleted_source_bytes))
            .is_some_and(|bytes| bytes <= u64::from(MAX_SMALL_EDIT_BYTES))
    }

    /// Freezes budget admission independently of the M2 implementation. A
    /// request with an invalid budget is rejected as `INVALID_ARGUMENT`
    /// before any session state or borrowed input is consumed.
    pub const fn budget_is_contract_valid(&self) -> bool {
        match self {
            Self::CreateCommit { budget, .. }
            | Self::CreateAbort { budget, .. }
            | Self::SmallEdit { budget, .. }
            | Self::BulkCommit { budget, .. }
            | Self::BulkAbort { budget, .. }
            | Self::Pump { budget, .. }
            | Self::ContinuationRelease { budget, .. }
            | Self::AnchorCreate { budget, .. }
            | Self::AnchorTransform { budget, .. }
            | Self::AnchorResolve { budget, .. }
            | Self::AnchorRelease { budget, .. }
            | Self::CoordinateConvert { budget, .. }
            | Self::HistoryReplay { budget, .. }
            | Self::HistoryRelease { budget, .. }
            | Self::CloseBegin { budget, .. }
            | Self::ClosePump { budget, .. }
            | Self::CloseFinish { budget, .. } => budget.is_contract_valid(),
            Self::QueryViewport { budget, .. } | Self::ContinuationNext { budget, .. } => {
                budget.is_page_contract_valid()
            }
            Self::CreateBegin { .. }
            | Self::CreateAppend { .. }
            | Self::SourceRead { .. }
            | Self::BulkBegin { .. }
            | Self::BulkAppend { .. }
            | Self::Cancel { .. }
            | Self::SessionTransferOwner { .. }
            | Self::SessionInspect { .. } => true,
        }
    }
}

/// Contract implemented by the M2 runtime. No implementation is supplied in M0.
pub trait RuntimeContract {
    fn dispatch(&mut self, request: RuntimeRequest<'_>, output: &mut [u8]) -> Outcome;
}

//! Flark v4 C-compatible ABI contract and native implementation.
//!
//! Fixed-width records remain the complete language boundary. The implementation
//! delegates document authority to `flark-runtime` and keeps raw-pointer work in
//! this crate only.

mod implementation;

pub use implementation::{
    flark_v4_anchor_create, flark_v4_anchor_release, flark_v4_anchor_resolve,
    flark_v4_anchor_transform, flark_v4_bulk_abort, flark_v4_bulk_append, flark_v4_bulk_begin,
    flark_v4_bulk_commit, flark_v4_cancel, flark_v4_close_begin, flark_v4_close_finish,
    flark_v4_close_pump, flark_v4_continuation_next, flark_v4_continuation_release,
    flark_v4_coordinate_convert, flark_v4_create_abort, flark_v4_create_append,
    flark_v4_create_begin, flark_v4_create_commit, flark_v4_history_release,
    flark_v4_history_replay, flark_v4_negotiate, flark_v4_pump, flark_v4_query_viewport,
    flark_v4_session_inspect, flark_v4_session_transfer_owner, flark_v4_small_edit,
    flark_v4_source_read,
};

pub use flark_runtime::{
    AFFINITIES, CAPABILITY_BITS, CERTIFICATION_STATES, COORDINATE_KINDS, HANDLE_KINDS,
    HISTORY_DISPOSITIONS, OPERATION_CODES, OWNERSHIP_KINDS, PARSER_PROFILES, PROGRESS_STATES,
    QUERY_KINDS, RESULT_RECORD_KINDS, SESSION_STATES, STATUS_CODES, TRANSACTION_STATES,
};

pub const ABI_MAJOR: u16 = 4;
pub const ABI_MINOR: u16 = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct AbiInfo {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub capability_bits: u64,
    pub max_small_edit_bytes: u32,
    pub max_bulk_chunk_bytes: u32,
    pub max_source_chunk_bytes: u32,
    pub max_result_bytes: u32,
    pub max_query_items: u32,
    pub max_transaction_edits: u32,
    pub reserved: [u64; 3],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct Outcome {
    pub struct_size: u32,
    pub operation: u32,
    pub status: u32,
    pub progress_state: u32,
    pub primary_handle: u64,
    pub secondary_handle: u64,
    pub revision: u64,
    pub snapshot: u64,
    pub progress_token: u64,
    pub required_bytes: u64,
    pub written_bytes: u64,
    pub detail_code: u64,
    pub reserved: [u64; 4],
}

impl Outcome {
    /// Maps one typed runtime result into the generic fixed-width C receipt.
    /// The mapping is duplicated in the machine-readable `outcomeFieldRoles`
    /// table and contract-tested there.
    pub fn from_runtime(value: flark_runtime::Outcome) -> Option<Self> {
        if !value.is_contract_valid() {
            return None;
        }
        let page_operation = value.operation.produces_result_page();
        let page_header_bytes = core::mem::size_of::<ResultPageHeader>() as u64;
        let mut outcome = Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            operation: value.operation as u32,
            status: value.status as u32,
            progress_state: value.progress as u32,
            required_bytes: if page_operation
                && matches!(value.status, flark_runtime::StatusCode::BufferTooSmall)
            {
                page_header_bytes + value.required_payload_bytes
            } else {
                value.required_payload_bytes
            },
            written_bytes: if matches!(value.result, flark_runtime::OperationResult::Page(_)) {
                page_header_bytes + value.written_payload_bytes
            } else {
                value.written_payload_bytes
            },
            ..Self::default()
        };
        match value.result {
            flark_runtime::OperationResult::None => {}
            flark_runtime::OperationResult::SessionCreated {
                session,
                transaction,
            } => {
                outcome.primary_handle = session.0;
                outcome.secondary_handle = transaction.0;
            }
            flark_runtime::OperationResult::TransactionStaged { transaction } => {
                outcome.primary_handle = transaction.0;
            }
            flark_runtime::OperationResult::RevisionCreated { session, revision } => {
                outcome.primary_handle = session.0;
                outcome.revision = revision.0;
            }
            flark_runtime::OperationResult::RevisionCommitted {
                revision,
                history_token,
                history,
            } => {
                outcome.primary_handle = history_token.0;
                outcome.revision = revision.0;
                outcome.detail_code = history as u64;
            }
            flark_runtime::OperationResult::Page(page) => {
                outcome.revision = page.revision.0;
                outcome.snapshot = page.snapshot.0;
            }
            flark_runtime::OperationResult::Progress { revision, token } => {
                outcome.revision = revision.0;
                outcome.progress_token = token.0;
            }
            flark_runtime::OperationResult::CloseProgress { token } => {
                outcome.progress_token = token.0;
            }
            flark_runtime::OperationResult::Anchor { anchor, revision } => {
                outcome.primary_handle = anchor.0;
                outcome.revision = revision.0;
            }
            flark_runtime::OperationResult::AnchorPosition {
                anchor,
                revision,
                coordinate: _,
                position,
            } => {
                outcome.primary_handle = anchor.0;
                outcome.revision = revision.0;
                outcome.detail_code = position;
            }
            flark_runtime::OperationResult::ConvertedPosition {
                revision,
                coordinate: _,
                position,
            } => {
                outcome.revision = revision.0;
                outcome.detail_code = position;
            }
            flark_runtime::OperationResult::OwnerTransferred { session } => {
                outcome.primary_handle = session.0;
            }
            flark_runtime::OperationResult::SessionInspection(inspection) => {
                outcome.primary_handle = inspection.session.0;
                outcome.revision = inspection.revision.0;
                outcome.detail_code = inspection.state as u64;
            }
        }
        Some(outcome)
    }
}

/// Prefix of every `SOURCE_READ`, `QUERY_VIEWPORT`, and `CONTINUATION_NEXT`
/// output buffer. Payload bytes begin at `struct_size`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ResultPageHeader {
    pub struct_size: u32,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub record_kind: u32,
    pub certification_state: u32,
    pub revision: u64,
    pub snapshot: u64,
    pub requested_range: SourceRange,
    pub covered_range: SourceRange,
    pub item_count: u32,
    pub payload_bytes: u32,
    pub continuation: u64,
    pub reserved: [u64; 2],
}

impl ResultPageHeader {
    /// Encodes a validated host-neutral runtime receipt into the frozen C page
    /// prefix. Invalid runtime receipts never cross the ABI boundary.
    pub fn from_runtime(receipt: flark_runtime::ResultPageReceipt) -> Option<Self> {
        if !receipt.is_contract_valid() {
            return None;
        }
        Some(Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
            record_kind: receipt.record_kind as u32,
            certification_state: receipt.certification as u32,
            revision: receipt.revision.0,
            snapshot: receipt.snapshot.0,
            requested_range: SourceRange {
                start_byte: receipt.requested_range.start_byte,
                end_byte: receipt.requested_range.end_byte,
            },
            covered_range: SourceRange {
                start_byte: receipt.covered_range.start_byte,
                end_byte: receipt.covered_range.end_byte,
            },
            item_count: receipt.item_count,
            payload_bytes: receipt.payload_bytes,
            continuation: receipt.continuation.0,
            reserved: [0; 2],
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SourceRange {
    pub start_byte: u64,
    pub end_byte: u64,
}

/// One ordered current-revision certification span in a
/// `SOURCE_AND_SEMANTIC` live viewport payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct CertificationRangeRecord {
    pub certification_state: u32,
    pub reserved: u32,
    pub source_range: SourceRange,
    pub source_utf16_range: SourceRange,
}

/// One fixed semantic payload record emitted by the v4 viewport query.
///
/// The result-page header remains the authority for revision, snapshot, and
/// record count. `u64::MAX` in an editable endpoint means that row has no
/// contiguous editable cut and must be painted from exact source neutrally.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ViewportRowRecord {
    pub ordinal: u64,
    pub kind: u32,
    pub flags: u32,
    pub source_start_byte: u64,
    pub source_end_byte: u64,
    pub source_start_utf16: u64,
    pub source_end_utf16: u64,
    pub editable_start_byte: u64,
    pub editable_end_byte: u64,
    pub editable_start_utf16: u64,
    pub editable_end_utf16: u64,
    pub presentation_prefix_start_byte: u64,
    pub presentation_prefix_end_byte: u64,
    pub presentation_prefix_start_utf16: u64,
    pub presentation_prefix_end_utf16: u64,
    pub path_depth: u32,
    pub semantic_variant: u32,
    pub semantic_value: u32,
    pub inline_fact_count: u32,
}

pub const VIEWPORT_ROW_FLAG_CONTIGUOUS_EDIT: u32 = 1 << 0;
pub const VIEWPORT_ROW_FLAG_PROJECTED_RESERVED: u32 = 1 << 1;
pub const VIEWPORT_ROW_FLAG_EDIT_UNAVAILABLE: u32 = 1 << 2;
pub const VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE: u32 = 1 << 3;
pub const VIEWPORT_ROW_HEADING_LEVEL_MASK: u32 = 0xff;
pub const VIEWPORT_ROW_HEADING_SETEXT: u32 = 1 << 8;
pub const VIEWPORT_ROW_LIST_MARKER_MASK: u32 = 0x7;
pub const VIEWPORT_ROW_LIST_HYPHEN: u32 = 1;
pub const VIEWPORT_ROW_LIST_PLUS: u32 = 2;
pub const VIEWPORT_ROW_LIST_ASTERISK: u32 = 3;
pub const VIEWPORT_ROW_LIST_ORDERED_PERIOD: u32 = 4;
pub const VIEWPORT_ROW_LIST_ORDERED_PARENTHESIS: u32 = 5;
pub const VIEWPORT_ROW_LIST_DEPTH_SHIFT: u32 = 3;
pub const VIEWPORT_ROW_LIST_DEPTH_MASK: u32 = 0xff << VIEWPORT_ROW_LIST_DEPTH_SHIFT;
pub const VIEWPORT_ROW_LIST_MARKER_OFFSET_SHIFT: u32 = 11;
pub const VIEWPORT_ROW_LIST_MARKER_OFFSET_MASK: u32 = 0x3 << VIEWPORT_ROW_LIST_MARKER_OFFSET_SHIFT;
pub const VIEWPORT_ROW_LIST_SIMPLE_CONTINUATION: u32 = 1 << 13;
pub const VIEWPORT_ROW_LIST_STARTS_LIST: u32 = 1 << 14;
pub const VIEWPORT_ROW_LIST_TASK: u32 = 1 << 15;
pub const VIEWPORT_ROW_LIST_TASK_CHECKED: u32 = 1 << 16;
pub const VIEWPORT_ROW_BLOCK_QUOTE_PRESENTATION: u32 = 1 << 16;
pub const VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_SHIFT: u32 = 17;
pub const VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_MASK: u32 = 0xff << VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_SHIFT;
pub const VIEWPORT_ROW_BLOCK_QUOTE_SIMPLE_CONTINUATION: u32 = 1 << 25;
pub const VIEWPORT_ROW_CODE_PRESENTATION: u32 = 1 << 16;
pub const VIEWPORT_ROW_CODE_FENCED: u32 = 1 << 17;
pub const VIEWPORT_ROW_CODE_TILDE: u32 = 1 << 18;
pub const VIEWPORT_ROW_CODE_CLOSED: u32 = 1 << 19;
pub const VIEWPORT_ROW_CODE_FENCE_OFFSET_SHIFT: u32 = 20;
pub const VIEWPORT_ROW_CODE_FENCE_OFFSET_MASK: u32 = 0x3 << VIEWPORT_ROW_CODE_FENCE_OFFSET_SHIFT;
pub const VIEWPORT_ROW_THEMATIC_BREAK_PRESENTATION: u32 = 1 << 16;

/// One parser-authored inline semantic following the viewport-row array in a
/// `SEMANTIC_FACTS` payload. Records are grouped in row order; each row's
/// `inline_fact_count` declares the size of its group.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct InlineFactRecord {
    pub kind: u32,
    pub flags: u32,
    pub source_start_byte: u64,
    pub source_end_byte: u64,
    pub source_start_utf16: u64,
    pub source_end_utf16: u64,
    pub content_start_byte: u64,
    pub content_end_byte: u64,
    pub content_start_utf16: u64,
    pub content_end_utf16: u64,
    /// First Unicode scalar for a parser-authored replacement, or zero.
    pub replacement_first: u32,
    /// Optional second replacement scalar, or zero.
    pub replacement_second: u32,
}

pub const INLINE_FACT_EMPHASIS: u32 = 1;
pub const INLINE_FACT_STRONG: u32 = 2;
pub const INLINE_FACT_CODE: u32 = 3;
pub const INLINE_FACT_STRIKETHROUGH: u32 = 4;
pub const INLINE_FACT_AUTOLINK_URI: u32 = 5;
pub const INLINE_FACT_AUTOLINK_EMAIL: u32 = 6;
pub const INLINE_FACT_BACKSLASH_ESCAPE: u32 = 7;
pub const INLINE_FACT_HARD_LINE_BREAK: u32 = 8;
pub const INLINE_FACT_REPLACEMENT: u32 = 9;
pub const INLINE_FACT_DIRECT_LINK: u32 = 10;
pub const INLINE_FACT_DIRECT_IMAGE: u32 = 11;
pub const INLINE_FACT_REFERENCE_LINK: u32 = 12;
pub const INLINE_FACT_REFERENCE_IMAGE: u32 = 13;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct EditDescriptor {
    pub start_byte: u64,
    pub end_byte: u64,
    pub replacement_offset: u64,
    pub replacement_len: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct WorkBudget {
    pub max_work_units: u64,
    pub advisory_max_micros: u64,
    pub max_result_items: u32,
    pub max_result_bytes: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SessionRef {
    pub session: u64,
    pub owner_token: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct NegotiateRequest {
    pub struct_size: u32,
    pub requested_major: u16,
    pub requested_minor: u16,
    pub required_capability_bits: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SessionConfig {
    pub struct_size: u32,
    pub parser_profile: u32,
    pub history_budget_bytes: u64,
    pub max_document_bytes: u64,
    pub flags: u64,
    pub reserved: [u64; 4],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct CreateRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub owner_token: u64,
    pub expected_total_bytes: u64,
    pub config: SessionConfig,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct StageRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub transaction: u64,
    pub chunk_offset: u64,
    pub chunk_len: u64,
    pub reserved: [u64; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SourceReadRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub revision: u64,
    pub range: SourceRange,
    pub reserved: [u64; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SmallEditRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub expected_revision: u64,
    pub edit_count: u32,
    pub reserved_u32: u32,
    pub replacement_bytes_len: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct BulkBeginRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub expected_revision: u64,
    pub range: SourceRange,
    pub expected_total_bytes: u64,
    pub reserved: [u64; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct TransactionRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub transaction: u64,
    pub expected_revision: u64,
    pub progress_token: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 1],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct PumpRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub expected_revision: u64,
    pub progress_token: u64,
    pub budget: WorkBudget,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct QueryRequest {
    pub struct_size: u32,
    pub query_kind: u32,
    pub session: SessionRef,
    pub revision: u64,
    pub snapshot: u64,
    pub range: SourceRange,
    pub continuation: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 1],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ContinuationRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub revision: u64,
    pub snapshot: u64,
    pub continuation: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 1],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct AnchorRequest {
    pub struct_size: u32,
    pub coordinate_kind: u32,
    pub session: SessionRef,
    pub revision: u64,
    pub snapshot: u64,
    pub anchor: u64,
    pub position: u64,
    pub affinity: u32,
    pub reserved_u32: u32,
    pub progress_token: u64,
    pub budget: WorkBudget,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct CoordinateRequest {
    pub struct_size: u32,
    pub from_kind: u32,
    pub to_kind: u32,
    pub reserved_u32: u32,
    pub session: SessionRef,
    pub revision: u64,
    pub snapshot: u64,
    pub position: u64,
    pub progress_token: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 1],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct HistoryRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub expected_revision: u64,
    pub history_token: u64,
    pub progress_token: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 1],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct CancelRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub progress_token: u64,
    pub reserved: [u64; 4],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct CloseRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub progress_token: u64,
    pub budget: WorkBudget,
    pub reserved: [u64; 1],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct OwnerTransferRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub new_owner_token: u64,
    pub reserved: [u64; 4],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct InspectRequest {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub reserved: [u64; 5],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SessionInspection {
    pub struct_size: u32,
    pub session_state: u32,
    pub session: u64,
    pub revision: u64,
    pub live_transactions: u32,
    pub live_continuations: u32,
    pub live_anchors: u32,
    pub live_history_tokens: u32,
    pub reserved: [u64; 3],
}

impl SessionInspection {
    pub fn from_runtime(value: flark_runtime::SessionInspectionReceipt) -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            session_state: value.state as u32,
            session: value.session.0,
            revision: value.revision.0,
            live_transactions: value.live_transactions,
            live_continuations: value.live_continuations,
            live_anchors: value.live_anchors,
            live_history_tokens: value.live_history_tokens,
            reserved: [0; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestFieldRuleKind {
    MustBeZero,
    MustBeNonZero,
    ZeroSelectsLatest,
    ZeroBeginsProgress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestFieldRule {
    pub operation: flark_runtime::OperationCode,
    pub field: &'static str,
    pub rule: RequestFieldRuleKind,
}

/// Operation-specific exceptions to the default rule that IDs, revisions,
/// snapshots, and tokens are nonzero. Fields not named here follow their
/// record's ordinary required/nonzero contract.
pub const REQUEST_FIELD_RULES: &[RequestFieldRule] = &[
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CreateCommit,
        field: "expected_revision",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CreateCommit,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CreateAbort,
        field: "expected_revision",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CreateAbort,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::BulkCommit,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::BulkAbort,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::Pump,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::QueryViewport,
        field: "continuation",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::QueryViewport,
        field: "snapshot",
        rule: RequestFieldRuleKind::ZeroSelectsLatest,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorCreate,
        field: "anchor",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorCreate,
        field: "snapshot",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorCreate,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorTransform,
        field: "coordinate_kind",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorTransform,
        field: "snapshot",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorTransform,
        field: "position",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorTransform,
        field: "affinity",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorTransform,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorResolve,
        field: "snapshot",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorResolve,
        field: "position",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorResolve,
        field: "affinity",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorResolve,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorRelease,
        field: "coordinate_kind",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorRelease,
        field: "revision",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorRelease,
        field: "snapshot",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorRelease,
        field: "position",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorRelease,
        field: "affinity",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::AnchorRelease,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CoordinateConvert,
        field: "snapshot",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CoordinateConvert,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::HistoryReplay,
        field: "progress_token",
        rule: RequestFieldRuleKind::ZeroBeginsProgress,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::HistoryRelease,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CloseBegin,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::ClosePump,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeNonZero,
    },
    RequestFieldRule {
        operation: flark_runtime::OperationCode::CloseFinish,
        field: "progress_token",
        rule: RequestFieldRuleKind::MustBeNonZero,
    },
];

pub const RECORD_LAYOUTS: &[(&str, usize)] = &[
    ("ABI_INFO", core::mem::size_of::<AbiInfo>()),
    ("OUTCOME", core::mem::size_of::<Outcome>()),
    (
        "RESULT_PAGE_HEADER",
        core::mem::size_of::<ResultPageHeader>(),
    ),
    (
        "CERTIFICATION_RANGE_RECORD",
        core::mem::size_of::<CertificationRangeRecord>(),
    ),
    (
        "VIEWPORT_ROW_RECORD",
        core::mem::size_of::<ViewportRowRecord>(),
    ),
    (
        "INLINE_FACT_RECORD",
        core::mem::size_of::<InlineFactRecord>(),
    ),
    ("SOURCE_RANGE", core::mem::size_of::<SourceRange>()),
    ("EDIT_DESCRIPTOR", core::mem::size_of::<EditDescriptor>()),
    ("WORK_BUDGET", core::mem::size_of::<WorkBudget>()),
    ("SESSION_REF", core::mem::size_of::<SessionRef>()),
    (
        "NEGOTIATE_REQUEST",
        core::mem::size_of::<NegotiateRequest>(),
    ),
    ("SESSION_CONFIG", core::mem::size_of::<SessionConfig>()),
    ("CREATE_REQUEST", core::mem::size_of::<CreateRequest>()),
    ("STAGE_REQUEST", core::mem::size_of::<StageRequest>()),
    (
        "SOURCE_READ_REQUEST",
        core::mem::size_of::<SourceReadRequest>(),
    ),
    (
        "SMALL_EDIT_REQUEST",
        core::mem::size_of::<SmallEditRequest>(),
    ),
    (
        "BULK_BEGIN_REQUEST",
        core::mem::size_of::<BulkBeginRequest>(),
    ),
    (
        "TRANSACTION_REQUEST",
        core::mem::size_of::<TransactionRequest>(),
    ),
    ("PUMP_REQUEST", core::mem::size_of::<PumpRequest>()),
    ("QUERY_REQUEST", core::mem::size_of::<QueryRequest>()),
    (
        "CONTINUATION_REQUEST",
        core::mem::size_of::<ContinuationRequest>(),
    ),
    ("ANCHOR_REQUEST", core::mem::size_of::<AnchorRequest>()),
    (
        "COORDINATE_REQUEST",
        core::mem::size_of::<CoordinateRequest>(),
    ),
    ("HISTORY_REQUEST", core::mem::size_of::<HistoryRequest>()),
    ("CANCEL_REQUEST", core::mem::size_of::<CancelRequest>()),
    ("CLOSE_REQUEST", core::mem::size_of::<CloseRequest>()),
    (
        "OWNER_TRANSFER_REQUEST",
        core::mem::size_of::<OwnerTransferRequest>(),
    ),
    ("INSPECT_REQUEST", core::mem::size_of::<InspectRequest>()),
    (
        "SESSION_INSPECTION",
        core::mem::size_of::<SessionInspection>(),
    ),
];

#[cfg(feature = "declarations")]
extern "C" {
    pub fn flark_v4_negotiate(
        request: *const NegotiateRequest,
        info: *mut AbiInfo,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_create_begin(
        request: *const CreateRequest,
        input: *const u8,
        input_len: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_create_append(
        request: *const StageRequest,
        input: *const u8,
        input_len: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_create_commit(request: *const TransactionRequest, outcome: *mut Outcome)
        -> u32;
    pub fn flark_v4_create_abort(request: *const TransactionRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_source_read(
        request: *const SourceReadRequest,
        output: *mut u8,
        output_capacity: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_small_edit(
        request: *const SmallEditRequest,
        edits: *const EditDescriptor,
        edit_count: u32,
        replacement_bytes: *const u8,
        replacement_bytes_len: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_bulk_begin(request: *const BulkBeginRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_bulk_append(
        request: *const StageRequest,
        input: *const u8,
        input_len: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_bulk_commit(request: *const TransactionRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_bulk_abort(request: *const TransactionRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_pump(request: *const PumpRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_query_viewport(
        request: *const QueryRequest,
        output: *mut u8,
        output_capacity: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_continuation_next(
        request: *const ContinuationRequest,
        output: *mut u8,
        output_capacity: u64,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_continuation_release(
        request: *const ContinuationRequest,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_anchor_create(request: *const AnchorRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_anchor_transform(request: *const AnchorRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_anchor_resolve(request: *const AnchorRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_anchor_release(request: *const AnchorRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_coordinate_convert(
        request: *const CoordinateRequest,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_history_replay(request: *const HistoryRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_history_release(request: *const HistoryRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_cancel(request: *const CancelRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_close_begin(request: *const CloseRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_close_pump(request: *const CloseRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_close_finish(request: *const CloseRequest, outcome: *mut Outcome) -> u32;
    pub fn flark_v4_session_transfer_owner(
        request: *const OwnerTransferRequest,
        outcome: *mut Outcome,
    ) -> u32;
    pub fn flark_v4_session_inspect(
        request: *const InspectRequest,
        inspection: *mut SessionInspection,
        outcome: *mut Outcome,
    ) -> u32;
}

//! Flark v4 C-compatible ABI contract and native implementation.
//!
//! Fixed-width records remain the complete language boundary. The implementation
//! delegates document authority to `flark-runtime` and keeps raw-pointer work in
//! this crate only.

#[cfg(not(feature = "declarations"))]
mod implementation;

#[cfg(not(feature = "declarations"))]
pub use implementation::{
    flark_v4_anchor_create, flark_v4_anchor_release, flark_v4_anchor_resolve,
    flark_v4_anchor_transform, flark_v4_bulk_abort, flark_v4_bulk_append, flark_v4_bulk_begin,
    flark_v4_bulk_commit, flark_v4_cancel, flark_v4_close_begin, flark_v4_close_finish,
    flark_v4_close_pump, flark_v4_continuation_next, flark_v4_continuation_release,
    flark_v4_coordinate_convert, flark_v4_create_abort, flark_v4_create_append,
    flark_v4_create_begin, flark_v4_create_commit, flark_v4_edit_intent_v1,
    flark_v4_history_release, flark_v4_history_replay, flark_v4_negotiate, flark_v4_pump,
    flark_v4_query_viewport, flark_v4_session_inspect, flark_v4_session_transfer_owner,
    flark_v4_small_edit, flark_v4_source_read, flark_v4_source_transaction_v1,
    flark_v4_staged_source_transaction_v1,
};

pub use flark_runtime::{
    AFFINITIES, CAPABILITY_BITS, CERTIFICATION_STATES, COORDINATE_KINDS, HANDLE_KINDS,
    HISTORY_DISPOSITIONS, OPERATION_CODES, OWNERSHIP_KINDS, PARSER_PROFILES, PROGRESS_STATES,
    QUERY_KINDS, RESULT_RECORD_KINDS, SESSION_STATES, STATUS_CODES, TRANSACTION_STATES,
};

pub const ABI_MAJOR: u16 = 4;
pub const ABI_MINOR: u16 = 38;

/// Requests process-global registry counts from `SESSION_INSPECT`. This mode
/// requires a zero session reference and capability
/// `GLOBAL_LIVE_STATE_INSPECTION_V1`.
pub const INSPECT_FLAG_GLOBAL_LIVE_STATE: u32 = 1;

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
            flark_runtime::OperationResult::GlobalLiveStateInspection(_) => {}
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

/// One exact identity-source segment in a projected viewport row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct ProjectionSegmentRecord {
    pub source_range: SourceRange,
    pub source_utf16_range: SourceRange,
}

/// CreateRequest.flags bit selecting the progressive opening-query mode:
/// the session exists immediately, pages stream through create_append with
/// incremental UTF-8 validation, certified rows are queryable before EOF,
/// and create_commit seals the load instead of parsing a buffered copy.
pub const CREATE_FLAG_OPENING: u32 = 1 << 0;

pub const VIEWPORT_ROW_FLAG_CONTIGUOUS_EDIT: u32 = 1 << 0;
pub const VIEWPORT_ROW_FLAG_PROJECTED_RESERVED: u32 = 1 << 1;
pub const VIEWPORT_ROW_FLAG_EDIT_UNAVAILABLE: u32 = 1 << 2;
pub const VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE: u32 = 1 << 3;
pub const VIEWPORT_ROW_FLAG_INSERT_PARAGRAPH_BREAK: u32 = 1 << 4;
pub const VIEWPORT_ROW_FLAG_INSERT_PARAGRAPH_BREAK_AT_PHYSICAL_LINE_START: u32 = 1 << 5;
pub const VIEWPORT_ROW_FLAG_DELETE_BACKWARD_AT_EDITABLE_START: u32 = 1 << 6;
pub const VIEWPORT_ROW_FLAG_DELETE_BACKWARD_AT_PROJECTION_START: u32 = 1 << 7;
pub const VIEWPORT_ROW_FLAG_DELETE_BACKWARD_AT_PHYSICAL_LINE_START: u32 = 1 << 8;
pub const VIEWPORT_ROW_FLAG_DELETE_FORWARD_AT_EDITABLE_START: u32 = 1 << 9;
pub const VIEWPORT_ROW_FLAG_INSERT_PARAGRAPH_BREAK_AS_LITERAL: u32 = 1 << 10;
pub const VIEWPORT_ROW_INLINE_FACT_COUNT_MASK: u32 = 0x0000_ffff;
pub const VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT: u32 = 16;
pub const VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_MASK: u32 = 0xffff_0000;
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
pub const VIEWPORT_ROW_LIST_MARKER_COLUMN_SHIFT: u32 = 17;
pub const VIEWPORT_ROW_LIST_MARKER_COLUMN_MASK: u32 = 0xff << VIEWPORT_ROW_LIST_MARKER_COLUMN_SHIFT;
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
pub const VIEWPORT_ROW_TABLE_PRESENTATION: u32 = 1 << 26;

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SemanticTargetRecord {
    pub kind: u32,
    pub syntax: u32,
    pub source_range: SourceRange,
    pub source_utf16_range: SourceRange,
    pub content_range: SourceRange,
    pub content_utf16_range: SourceRange,
    pub destination_source_range: SourceRange,
    pub destination_source_utf16_range: SourceRange,
    pub title_source_range: SourceRange,
    pub title_source_utf16_range: SourceRange,
    pub destination_bytes: u32,
    pub title_bytes: u32,
    pub reserved: [u64; 2],
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
pub const INLINE_FACT_TABLE_CELL: u32 = 14;
/// A parser-authored literal-safe envelope transported in the semantic-record
/// stream. The source ranges carry the envelope and `flags` carries one
/// `LITERAL_EDIT_CLASS_*` value; content/replacement fields are zero.
pub const INLINE_FACT_LITERAL_SAFE_ENVELOPE: u32 = 15;
/// A parser-authored bounded projection edit cell. Source fields carry the
/// affected closure and content fields carry the admission trigger.
pub const INLINE_FACT_PROJECTION_EDIT_CELL: u32 = 16;
pub const INLINE_FACT_PENDING_PRESENTATION_PLAN: u32 = 17;
pub const INLINE_FACT_PENDING_PRESENTATION_STEP: u32 = 18;
pub const INLINE_FACT_PENDING_PRESENTATION_ROW: u32 = 19;
pub const PENDING_PRESENTATION_PLAN_SEQUENCE_LENGTH_MASK: u32 = 0x0000_00ff;
pub const PENDING_PRESENTATION_PLAN_STEP_COUNT_SHIFT: u32 = 8;
pub const PENDING_PRESENTATION_PLAN_REPLACED_ROW_COUNT_SHIFT: u32 = 16;
pub const PENDING_PRESENTATION_STEP_PREFIX_LENGTH_MASK: u32 = 0x0000_00ff;
pub const PENDING_PRESENTATION_STEP_ROW_COUNT_SHIFT: u32 = 8;
pub const PENDING_PRESENTATION_ROW_KIND_MASK: u32 = 0x0000_ffff;
pub const PENDING_PRESENTATION_ROW_FACT_COUNT_SHIFT: u32 = 16;
pub const LITERAL_EDIT_CLASS_ASCII_WORD_INSERTION: u32 = 1;
pub const LITERAL_EDIT_CLASS_SINGLE_ASCII_SPACE_INSERTION: u32 = 2;
pub const LITERAL_EDIT_CLASS_SINGLE_ASCII_ASTERISK_INSERTION: u32 = 3;
pub const LITERAL_EDIT_CLASS_SINGLE_ASCII_LITERAL_UNIT_DELETION: u32 = 4;
pub const PROJECTION_EDIT_CELL_MATCHER_MASK: u32 = 0x00ff;
pub const PROJECTION_EDIT_CELL_MATCH_ANY_NO_CRLF_SPLICE: u32 = 0x0001;
pub const PROJECTION_EDIT_CELL_MATCH_ASCII_LITERAL_SPLICE_IN_LITERAL: u32 = 0x0002;
pub const PROJECTION_EDIT_CELL_MATCH_INSERT_SINGLE_ASCII_SPACE_AT_POINT: u32 = 0x0003;
pub const PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL: u32 = 0x0004;
pub const PROJECTION_EDIT_CELL_MATCH_APPEND_ASCII_LITERAL_AT_LINE_END: u32 = 0x0005;
pub const PROJECTION_EDIT_CELL_MATCH_INSERT_EXACT_SCALAR_AT_POINT: u32 = 0x0006;
pub const PROJECTION_EDIT_CELL_MATCH_EXACT_SPLICE_REPLACE_BLOCK_SHELL: u32 = 0x0007;
pub const PROJECTION_EDIT_CELL_MATCH_SIMPLE_BLOCK_PREFIX_PLAN: u32 = 0x0008;
pub const PROJECTION_EDIT_CELL_SIMPLE_BLOCK_PLAN_BYTES_MASK: u32 = 0x00ff_ffff;
pub const PROJECTION_EDIT_CELL_SIMPLE_BLOCK_PLAN_ACTIVATION_SHIFT: u32 = 24;
pub const PROJECTION_EDIT_CELL_SIMPLE_BLOCK_PLAN_ACTIVATION_MASK: u32 = 0xff00_0000;
pub const PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL: u32 = 0x0100;
pub const PROJECTION_EDIT_CELL_RETAIN_OUTSIDE: u32 = 0x0200;
pub const PROJECTION_EDIT_CELL_PRESENT_EXACT: u32 = 0x0400;
pub const PROJECTION_EDIT_CELL_CHAIN_RESULT: u32 = 0x0800;
pub const PROJECTION_EDIT_CELL_TERMINAL_SPACE_BLOCKED: u32 = 0x1000;
pub const PROJECTION_EDIT_CELL_REPLACE_BLOCK_SHELL: u32 = 0x2000;
pub const PROJECTION_EDIT_CELL_EMPTY_LITERAL_RESULT: u32 = 0x4000;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_KIND_MASK: u32 = 0x0000_000f;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_PREFIX_SHIFT: u32 = 4;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_PREFIX_MASK: u32 = 0x0000_0ff0;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_PARAMETER_SHIFT: u32 = 12;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_PLAIN: u32 = 1;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_ATX_HEADING: u32 = 2;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_BLOCK_QUOTE: u32 = 3;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_LIST_ITEM: u32 = 4;
pub const PROJECTION_EDIT_CELL_RESULT_SHELL_REMOVED: u32 = 5;
pub const INLINE_FACT_TABLE_ALIGNMENT_MASK: u32 = 0x03;
pub const INLINE_FACT_TABLE_HEADER: u32 = 1 << 2;
pub const INLINE_FACT_TABLE_ROW_START: u32 = 1 << 3;
pub const INLINE_FACT_TABLE_AUTOCOMPLETED: u32 = 1 << 4;

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

pub const EDIT_PROFILE_FLARK_V1: u32 = 1;
pub const EDIT_INTENT_INSERT_PARAGRAPH_BREAK: u32 = 1;
pub const EDIT_INTENT_DELETE_BACKWARD: u32 = 2;
pub const EDIT_INTENT_DELETE_FORWARD: u32 = 3;
pub const EDIT_INTENT_TOGGLE_TASK_CHECKED: u32 = 4;
pub const EDIT_INTENT_INDENT_LIST_ITEM: u32 = 5;
pub const EDIT_INTENT_OUTDENT_LIST_ITEM: u32 = 6;
pub const EDIT_INTENT_DISPOSITION_APPLIED: u32 = 1;
pub const EDIT_INTENT_DISPOSITION_HANDLED_NO_CHANGE: u32 = 2;
pub const EDIT_INTENT_DISPOSITION_NOT_APPLICABLE: u32 = 3;
pub const EDIT_INTENT_DISPOSITION_NEEDS_CURRENT_SEMANTICS: u32 = 4;
pub const EDIT_INTENT_RECEIPT_HAS_COMMIT: u32 = 1 << 0;
pub const EDIT_INTENT_RECEIPT_PARSER_PENDING: u32 = 1 << 1;
pub const EDIT_INTENT_RECEIPT_SEMANTIC_BYTES: u32 = 1 << 2;
pub const EDIT_INTENT_RECEIPT_PRESENTATION_PROVEN: u32 = 1 << 3;
pub const EDIT_INTENT_RECEIPT_HAS_INLINE_CONTINUATION: u32 = 1 << 4;
pub const INLINE_CONTINUATION_RECIPE_VERSION_V1: u16 = 1;
pub const INLINE_CONTINUATION_SCALAR_STABLE_NON_WHITESPACE: u16 = 1;
pub const INLINE_CONTINUATION_SCALAR_COMMONMARK_ORDINARY_ONLY: u16 = 2;
pub const EDIT_PRESENTATION_NONE: u32 = 0;
pub const EDIT_PRESENTATION_SPLIT_PARAGRAPH: u32 = 1;
pub const EDIT_PRESENTATION_CONTINUE_LIST: u32 = 2;
pub const EDIT_PRESENTATION_EXIT_LIST: u32 = 3;
pub const EDIT_PRESENTATION_MERGE_PARAGRAPH: u32 = 4;
pub const EDIT_PRESENTATION_LIFT_LIST: u32 = 5;
pub const EDIT_PRESENTATION_CONTINUE_BLOCK_QUOTE: u32 = 6;
pub const EDIT_PRESENTATION_EXIT_BLOCK_QUOTE: u32 = 7;
pub const EDIT_PRESENTATION_LIFT_BLOCK_QUOTE: u32 = 8;
pub const EDIT_PRESENTATION_EXIT_HEADING: u32 = 9;
pub const EDIT_PRESENTATION_LIFT_HEADING: u32 = 10;
pub const EDIT_PRESENTATION_OUTDENT_LIST: u32 = 11;
pub const EDIT_PRESENTATION_CONTINUE_INDENTED_CODE: u32 = 12;
pub const EDIT_PRESENTATION_JOIN_INDENTED_CODE: u32 = 13;
pub const EDIT_PRESENTATION_LIFT_INDENTED_CODE: u32 = 14;
pub const EDIT_PRESENTATION_DELETE_THEMATIC_BREAK: u32 = 15;
pub const EDIT_PRESENTATION_OUTDENT_BLOCK_QUOTE: u32 = 16;
pub const EDIT_PRESENTATION_TOGGLE_TASK_CHECKED: u32 = 17;
pub const EDIT_PRESENTATION_INDENT_LIST: u32 = 18;
pub const EDIT_PRESENTATION_RETAIN_PARAGRAPH_GAP: u32 = 19;
pub const EDIT_PRESENTATION_JOIN_FENCED_CODE: u32 = 20;
pub const EDIT_PRESENTATION_DELETE_INLINE_OWNER: u32 = 21;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct EditIntentRequestV1 {
    pub struct_size: u32,
    pub profile_id: u32,
    pub session: SessionRef,
    pub expected_revision: u64,
    pub selection_base_anchor: u64,
    pub selection_extent_anchor: u64,
    pub logical_edit_id: u64,
    pub request_digest: u64,
    pub acknowledge_previous_logical_edit_id: u64,
    pub selection_generation: u64,
    pub intent: u32,
    pub selection_affinity: u32,
    pub selection_direction: u32,
    pub composition_active: u32,
    pub budget: WorkBudget,
    /// Zero for selection-bound keyboard intents. A selection-independent
    /// semantic action supplies one owned anchor inside its certified target
    /// row; the action preserves both canonical selection anchors.
    pub target_anchor: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct EditIntentReceiptV1 {
    pub struct_size: u32,
    pub semantic_disposition: u32,
    pub history_disposition: u32,
    pub flags: u32,
    pub logical_edit_id: u64,
    pub request_digest: u64,
    pub base_revision: u64,
    pub result_revision: u64,
    pub base_byte_range: SourceRange,
    pub base_utf16_range: SourceRange,
    pub result_byte_range: SourceRange,
    pub result_utf16_range: SourceRange,
    pub result_selection_utf16: u64,
    pub result_selection_affinity: u32,
    pub result_selection_direction: u32,
    pub result_source_byte_length: u64,
    pub result_source_utf16_length: u64,
    pub affected_result_utf16_range: SourceRange,
    pub history_token: u64,
    pub replacement_bytes: u32,
    pub presentation_transition: u32,
    pub reserved: [u64; 2],
}

pub const SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT: u32 = 1 << 0;
pub const SOURCE_TRANSACTION_RECEIPT_PARSER_PENDING: u32 = 1 << 1;
pub const SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES: u32 = 1 << 2;
pub const SOURCE_TRANSACTION_RECEIPT_COMPOSITE_HISTORY_EXTENDED: u32 = 1 << 3;
pub const SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES: u32 = 1 << 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SourceTransactionRequestV1 {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub expected_revision: u64,
    pub selection_base_anchor: u64,
    pub selection_extent_anchor: u64,
    pub logical_edit_id: u64,
    pub request_digest: u64,
    pub acknowledge_previous_logical_edit_id: u64,
    pub selection_generation: u64,
    pub base_utf16_range: SourceRange,
    pub result_selection_base_utf16: u64,
    pub result_selection_extent_utf16: u64,
    pub selection_affinity: u32,
    pub selection_direction: u32,
    pub replacement_bytes_len: u64,
    pub budget: WorkBudget,
    /// Zero creates a standalone history token. A nonzero ID lets native
    /// extend the current tail when it names the same adjacent edit group.
    pub history_group_id: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct SourceTransactionReceiptV1 {
    pub struct_size: u32,
    pub history_disposition: u32,
    pub flags: u32,
    pub reserved_u32: u32,
    pub logical_edit_id: u64,
    pub request_digest: u64,
    pub base_revision: u64,
    pub result_revision: u64,
    pub base_byte_range: SourceRange,
    pub base_utf16_range: SourceRange,
    pub result_byte_range: SourceRange,
    pub result_utf16_range: SourceRange,
    pub result_selection_base_utf16: u64,
    pub result_selection_extent_utf16: u64,
    pub result_selection_affinity: u32,
    pub result_selection_direction: u32,
    pub result_source_byte_length: u64,
    pub result_source_utf16_length: u64,
    pub affected_result_utf16_range: SourceRange,
    pub history_token: u64,
    pub replacement_bytes: u64,
    pub reserved: [u64; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct StagedSourceTransactionRequestV1 {
    pub struct_size: u32,
    pub flags: u32,
    pub session: SessionRef,
    pub transaction: u64,
    pub expected_revision: u64,
    pub progress_token: u64,
    pub selection_base_anchor: u64,
    pub selection_extent_anchor: u64,
    pub logical_edit_id: u64,
    pub request_digest: u64,
    pub acknowledge_previous_logical_edit_id: u64,
    pub selection_generation: u64,
    /// V1 admits only a collapsed result caret at the inserted range end.
    pub result_selection_utf16: u64,
    pub selection_affinity: u32,
    pub selection_direction: u32,
    pub budget: WorkBudget,
    pub history_group_id: u64,
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

    pub fn from_global(value: flark_runtime::GlobalLiveStateInspectionReceipt) -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            session_state: 0,
            session: 0,
            revision: 0,
            live_transactions: value.live_transactions,
            live_continuations: value.live_continuations,
            live_anchors: value.live_anchors,
            live_history_tokens: value.live_history_tokens,
            reserved: [value.live_sessions, 0, 0],
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
        "PROJECTION_SEGMENT_RECORD",
        core::mem::size_of::<ProjectionSegmentRecord>(),
    ),
    (
        "INLINE_FACT_RECORD",
        core::mem::size_of::<InlineFactRecord>(),
    ),
    (
        "SEMANTIC_TARGET_RECORD",
        core::mem::size_of::<SemanticTargetRecord>(),
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
        "EDIT_INTENT_REQUEST_V1",
        core::mem::size_of::<EditIntentRequestV1>(),
    ),
    (
        "EDIT_INTENT_RECEIPT_V1",
        core::mem::size_of::<EditIntentReceiptV1>(),
    ),
    (
        "SOURCE_TRANSACTION_REQUEST_V1",
        core::mem::size_of::<SourceTransactionRequestV1>(),
    ),
    (
        "SOURCE_TRANSACTION_RECEIPT_V1",
        core::mem::size_of::<SourceTransactionReceiptV1>(),
    ),
    (
        "STAGED_SOURCE_TRANSACTION_REQUEST_V1",
        core::mem::size_of::<StagedSourceTransactionRequestV1>(),
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

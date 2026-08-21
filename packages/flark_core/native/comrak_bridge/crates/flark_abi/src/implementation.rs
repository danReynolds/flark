use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;
use std::str;
use std::sync::{Mutex, OnceLock};

use flark_runtime::{
    Affinity, AnchorHandle, CertificationState, ContinuationHandle, CoordinateKind, DocumentActor,
    DocumentActorError, DocumentBulletMarker, DocumentCodeBlockStyle,
    DocumentEditIntentDispositionV1, DocumentEditIntentV1, DocumentEditPresentationTransitionV1,
    DocumentFenceCharacter, DocumentHeadingStyle, DocumentInlineFact, DocumentInlineFactKind,
    DocumentListDelimiter, DocumentListMarker, DocumentLiteralEditClass,
    DocumentLiteralSafeEnvelope, DocumentLiveViewportSpan, DocumentProjectionEditCell,
    DocumentProjectionSegment, DocumentSemanticTargetKind, DocumentSemanticTargetSyntax,
    DocumentSessionError, DocumentSessionPhase, DocumentViewportRowEditCapability,
    DocumentViewportRowPresentation, HistoryDisposition, HistoryToken, OperationCode,
    OperationResult, Outcome as RuntimeOutcome, ProgressState, ProgressToken, QueryKind,
    ResultPageReceipt, ResultRecordKind, Revision, SessionHandle, SessionInspectionReceipt,
    SessionState, SnapshotId, SourceRange as RuntimeSourceRange, StatusCode, TransactionHandle,
    MAX_BULK_CHUNK_BYTES, MAX_LIVE_ANCHORS, MAX_QUERY_ITEMS, MAX_RESULT_BYTES,
    MAX_SMALL_EDIT_BYTES, MAX_SOURCE_CHUNK_BYTES, MAX_TRANSACTION_EDITS,
};

use crate::{
    AbiInfo, AnchorRequest, BulkBeginRequest, CancelRequest, CertificationRangeRecord,
    CloseRequest, ContinuationRequest, CoordinateRequest, CreateRequest, EditDescriptor,
    EditIntentReceiptV1, EditIntentRequestV1, HistoryRequest, InlineFactRecord, InspectRequest,
    NegotiateRequest, Outcome, OwnerTransferRequest, ProjectionSegmentRecord, PumpRequest,
    QueryRequest, ResultPageHeader, SemanticTargetRecord, SessionInspection, SessionRef,
    SmallEditRequest, SourceRange, SourceReadRequest, SourceTransactionReceiptV1,
    SourceTransactionRequestV1, StageRequest, StagedSourceTransactionRequestV1, TransactionRequest,
    ViewportRowRecord, WorkBudget, ABI_MAJOR, ABI_MINOR, EDIT_INTENT_DELETE_BACKWARD,
    EDIT_INTENT_DELETE_FORWARD, EDIT_INTENT_DISPOSITION_APPLIED,
    EDIT_INTENT_DISPOSITION_HANDLED_NO_CHANGE, EDIT_INTENT_DISPOSITION_NEEDS_CURRENT_SEMANTICS,
    EDIT_INTENT_DISPOSITION_NOT_APPLICABLE, EDIT_INTENT_INDENT_LIST_ITEM,
    EDIT_INTENT_INSERT_PARAGRAPH_BREAK, EDIT_INTENT_OUTDENT_LIST_ITEM,
    EDIT_INTENT_RECEIPT_HAS_COMMIT, EDIT_INTENT_RECEIPT_PARSER_PENDING,
    EDIT_INTENT_RECEIPT_SEMANTIC_BYTES, EDIT_INTENT_TOGGLE_TASK_CHECKED,
    EDIT_PRESENTATION_CONTINUE_BLOCK_QUOTE, EDIT_PRESENTATION_CONTINUE_INDENTED_CODE,
    EDIT_PRESENTATION_CONTINUE_LIST, EDIT_PRESENTATION_DELETE_THEMATIC_BREAK,
    EDIT_PRESENTATION_EXIT_BLOCK_QUOTE, EDIT_PRESENTATION_EXIT_HEADING,
    EDIT_PRESENTATION_EXIT_LIST, EDIT_PRESENTATION_INDENT_LIST,
    EDIT_PRESENTATION_JOIN_INDENTED_CODE, EDIT_PRESENTATION_LIFT_BLOCK_QUOTE,
    EDIT_PRESENTATION_LIFT_HEADING, EDIT_PRESENTATION_LIFT_INDENTED_CODE,
    EDIT_PRESENTATION_LIFT_LIST, EDIT_PRESENTATION_MERGE_PARAGRAPH, EDIT_PRESENTATION_NONE,
    EDIT_PRESENTATION_OUTDENT_BLOCK_QUOTE, EDIT_PRESENTATION_OUTDENT_LIST,
    EDIT_PRESENTATION_RETAIN_PARAGRAPH_GAP, EDIT_PRESENTATION_SPLIT_PARAGRAPH,
    EDIT_PRESENTATION_TOGGLE_TASK_CHECKED, EDIT_PROFILE_FLARK_V1, INLINE_FACT_AUTOLINK_EMAIL,
    INLINE_FACT_AUTOLINK_URI, INLINE_FACT_BACKSLASH_ESCAPE, INLINE_FACT_CODE,
    INLINE_FACT_DIRECT_IMAGE, INLINE_FACT_DIRECT_LINK, INLINE_FACT_EMPHASIS,
    INLINE_FACT_HARD_LINE_BREAK, INLINE_FACT_LITERAL_SAFE_ENVELOPE,
    INLINE_FACT_PROJECTION_EDIT_CELL, INLINE_FACT_REFERENCE_IMAGE, INLINE_FACT_REFERENCE_LINK,
    INLINE_FACT_REPLACEMENT, INLINE_FACT_STRIKETHROUGH, INLINE_FACT_STRONG, INLINE_FACT_TABLE_CELL,
    LITERAL_EDIT_CLASS_ASCII_WORD_INSERTION, LITERAL_EDIT_CLASS_SINGLE_ASCII_SPACE_INSERTION,
    SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES,
    SOURCE_TRANSACTION_RECEIPT_COMPOSITE_HISTORY_EXTENDED, SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT,
    SOURCE_TRANSACTION_RECEIPT_PARSER_PENDING, SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES,
    VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_SHIFT, VIEWPORT_ROW_BLOCK_QUOTE_PRESENTATION,
    VIEWPORT_ROW_BLOCK_QUOTE_SIMPLE_CONTINUATION, VIEWPORT_ROW_CODE_CLOSED,
    VIEWPORT_ROW_CODE_FENCED, VIEWPORT_ROW_CODE_FENCE_OFFSET_SHIFT, VIEWPORT_ROW_CODE_PRESENTATION,
    VIEWPORT_ROW_CODE_TILDE, VIEWPORT_ROW_FLAG_CONTIGUOUS_EDIT, VIEWPORT_ROW_FLAG_EDIT_UNAVAILABLE,
    VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE, VIEWPORT_ROW_FLAG_PROJECTED_RESERVED,
    VIEWPORT_ROW_HEADING_LEVEL_MASK, VIEWPORT_ROW_HEADING_SETEXT,
    VIEWPORT_ROW_INLINE_FACT_COUNT_MASK, VIEWPORT_ROW_LIST_ASTERISK, VIEWPORT_ROW_LIST_DEPTH_SHIFT,
    VIEWPORT_ROW_LIST_HYPHEN, VIEWPORT_ROW_LIST_MARKER_COLUMN_SHIFT,
    VIEWPORT_ROW_LIST_MARKER_OFFSET_SHIFT, VIEWPORT_ROW_LIST_ORDERED_PARENTHESIS,
    VIEWPORT_ROW_LIST_ORDERED_PERIOD, VIEWPORT_ROW_LIST_PLUS,
    VIEWPORT_ROW_LIST_SIMPLE_CONTINUATION, VIEWPORT_ROW_LIST_STARTS_LIST, VIEWPORT_ROW_LIST_TASK,
    VIEWPORT_ROW_LIST_TASK_CHECKED, VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT,
    VIEWPORT_ROW_TABLE_PRESENTATION, VIEWPORT_ROW_THEMATIC_BREAK_PRESENTATION,
};

const IMPLEMENTED_CAPABILITIES: u64 = (1 << 0)
    | (1 << 1)
    | (1 << 2)
    | (1 << 3)
    | (1 << 4)
    | (1 << 5)
    | (1 << 6)
    | (1 << 7)
    | (1 << 8)
    | (1 << 9)
    | (1 << 10)
    | (1 << 11)
    | (1 << 12)
    | (1 << 13)
    | (1 << 14)
    | (1 << 15)
    | (1 << 16)
    | (1 << 17)
    | (1 << 18)
    | (1 << 19)
    | (1 << 20)
    | (1 << 21)
    | (1 << 22)
    | (1 << 23)
    | (1 << 24)
    | (1 << 25)
    | (1 << 26)
    | (1 << 27)
    | (1 << 28)
    | (1 << 29);

struct Registry {
    next_handle: u64,
    sessions: BTreeMap<u64, StoredSession>,
    transactions: BTreeMap<u64, StoredBulkTransaction>,
    continuations: BTreeMap<u64, StoredContinuation>,
    histories: BTreeMap<u64, StoredHistory>,
    anchors: BTreeMap<u64, StoredAnchor>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            next_handle: 1,
            sessions: BTreeMap::new(),
            transactions: BTreeMap::new(),
            continuations: BTreeMap::new(),
            histories: BTreeMap::new(),
            anchors: BTreeMap::new(),
        }
    }
}

impl Registry {
    fn allocate_handle(&mut self) -> Result<u64, StatusCode> {
        let handle = self.next_handle;
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        Ok(handle)
    }
}

/// Bookkeeping for one progressive opening-query transaction: the staged
/// transaction handle, the raw-byte offset the transport has delivered, the
/// declared total (zero when the stream length is unknown), and at most three
/// carried bytes of one UTF-8 scalar split across transport chunks.
#[cfg(feature = "opening-session")]
struct OpeningTransaction {
    transaction: u64,
    received_bytes: u64,
    expected_bytes: u64,
    utf8_carry: Vec<u8>,
}

struct StoredSession {
    owner: u64,
    state: StoredSessionState,
    #[cfg(feature = "opening-session")]
    opening: Option<OpeningTransaction>,
    transactions: BTreeSet<u64>,
    max_document_bytes: u64,
    progress_token: u64,
    continuations: BTreeSet<u64>,
    anchors: BTreeSet<u64>,
    history_budget_bytes: u64,
    history_used_bytes: u64,
    history_head: u64,
    history_tail: u64,
    history_state: u64,
    next_history_state: u64,
    history_token_count: u32,
    evicted_history_tokens: BTreeSet<u64>,
    terminal_edit_intent: Option<StoredEditIntentTerminal>,
    terminal_source_transaction: Option<StoredSourceTransactionTerminal>,
    close_token: u64,
    close_complete: bool,
}

/// A source-stable position. Anchors are transformed eagerly on every
/// committed edit, so a stored offset is always a scalar boundary at the
/// session's current revision; no per-anchor revision or edit journal exists.
struct StoredAnchor {
    session: u64,
    byte_offset: u64,
    affinity: Affinity,
}

enum StoredSessionState {
    Creating {
        transaction: u64,
        expected_bytes: usize,
        bytes: Vec<u8>,
    },
    Open(DocumentActor),
}

enum BulkHistoryCapture {
    Capturing(Vec<u8>),
    Disabled,
    OverBudget,
}

struct StoredBulkTransaction {
    session: u64,
    owner: u64,
    expected_revision: u64,
    start_byte: u64,
    end_byte: u64,
    expected_bytes: usize,
    replacement: Vec<u8>,
    validated_bytes: usize,
    validated_utf16: usize,
    inverse_next_byte: u64,
    history: BulkHistoryCapture,
    progress_token: u64,
    source_request: Option<StoredStagedSourceRequest>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StoredStagedSourceRequest {
    selection_base_anchor: u64,
    selection_extent_anchor: u64,
    logical_edit_id: u64,
    request_digest: u64,
    acknowledge_previous_logical_edit_id: u64,
    selection_generation: u64,
    result_selection_utf16: u64,
    selection_affinity: u32,
    selection_direction: u32,
    history_group_id: u64,
}

#[derive(Clone, Copy)]
struct StoredContinuation {
    session: u64,
    owner: u64,
    revision: u64,
    snapshot: u64,
    requested_start: u64,
    requested_end: u64,
    next_start: u64,
    query_kind: u32,
}

const HISTORY_TOKEN_OVERHEAD_BYTES: u64 = 64;
const HISTORY_SPLICE_OVERHEAD_BYTES: u64 = 32;
const MAX_COMPOSITE_HISTORY_STEPS: usize = 256;
const MAX_COMPOSITE_HISTORY_MATERIALIZED_BYTES: u64 = 1 << 20;
const MAX_EVICTED_HISTORY_TOMBSTONES: usize = 4096;

#[derive(Clone)]
struct StoredHistorySplice {
    start_byte: u64,
    end_byte: u64,
    replacement: Vec<u8>,
}

#[derive(Clone)]
struct StoredHistory {
    session: u64,
    owner: u64,
    applies_state: u64,
    target_state: u64,
    group_id: u64,
    splices: Vec<StoredHistorySplice>,
    retained_bytes: u64,
    previous: u64,
    next: u64,
}

struct StoredEditIntentTerminal {
    receipt: EditIntentReceiptV1,
    replacement: Vec<u8>,
}

#[derive(Clone, Copy)]
struct StoredSourceTransactionTerminal {
    receipt: SourceTransactionReceiptV1,
}

fn record_evicted_history_token(entry: &mut StoredSession, token: u64) {
    let maximum = usize::try_from(entry.history_budget_bytes / HISTORY_TOKEN_OVERHEAD_BYTES)
        .unwrap_or(MAX_EVICTED_HISTORY_TOMBSTONES)
        .min(MAX_EVICTED_HISTORY_TOMBSTONES);
    if maximum == 0 {
        return;
    }
    entry.evicted_history_tokens.insert(token);
    while entry.evicted_history_tokens.len() > maximum {
        let Some(oldest) = entry.evicted_history_tokens.first().copied() else {
            break;
        };
        entry.evicted_history_tokens.remove(&oldest);
    }
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

fn runtime_error(operation: OperationCode, status: StatusCode) -> RuntimeOutcome {
    RuntimeOutcome {
        operation,
        status,
        progress: match status {
            StatusCode::Backpressure => ProgressState::Backpressured,
            // Outcome coherence pairs this status with PendingSourceGap;
            // leaving None here collapsed a typed not-ready reply into an
            // anonymous internal fault at the encoding boundary.
            StatusCode::NotReadySourceGap => ProgressState::PendingSourceGap,
            StatusCode::ProgressStalled
            | StatusCode::PanicContained
            | StatusCode::InternalFault
            | StatusCode::ParserFault
            | StatusCode::AllocationFailure => ProgressState::Fault,
            _ => ProgressState::None,
        },
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::None,
    }
}

fn map_document_error(error: &DocumentSessionError) -> StatusCode {
    match error {
        DocumentSessionError::ZeroWorkBudget => StatusCode::InvalidArgument,
        DocumentSessionError::Busy => StatusCode::SessionBusy,
        DocumentSessionError::NotReady => StatusCode::NotReadySourceGap,
        DocumentSessionError::Faulted
        | DocumentSessionError::Parser(_)
        | DocumentSessionError::Inline(_) => StatusCode::ParserFault,
        DocumentSessionError::StaleRevision { .. } => StatusCode::StaleRevision,
        DocumentSessionError::RangeOutOfBounds | DocumentSessionError::Source(_) => {
            StatusCode::RangeOutOfBounds
        }
        DocumentSessionError::EditIntentLimitExceeded => StatusCode::EditTooLarge,
        DocumentSessionError::UnsupportedEditIntentSelection => StatusCode::InvalidArgument,
        DocumentSessionError::QueryBudgetExceeded => StatusCode::QueryLimitExceeded,
        #[cfg(feature = "opening-session")]
        DocumentSessionError::Opening(_) => StatusCode::TransactionConflict,
        #[cfg(feature = "opening-session")]
        DocumentSessionError::Compact(_) => StatusCode::ParserFault,
        error if error.is_backpressure() => StatusCode::Backpressure,
        DocumentSessionError::Engine(_) => StatusCode::InternalFault,
    }
}

fn map_actor_error(error: &DocumentActorError) -> StatusCode {
    match error {
        DocumentActorError::Session(error) => map_document_error(error),
        DocumentActorError::Spawn(_) => StatusCode::ResourceLimitExceeded,
        DocumentActorError::Closed => StatusCode::InternalFault,
        // The contract requires a contained unwind to be reported as such
        // rather than collapsing into an anonymous internal fault.
        DocumentActorError::Panicked => StatusCode::PanicContained,
    }
}

/// Begins one progressive opening-query session: the document actor exists
/// immediately over an empty admitted source, the staged transaction tracks
/// raw transport bytes, and any initial chunk stages through the same
/// incremental UTF-8 path as later appends. A declared total of zero means
/// the stream length is unknown and only commit ends it.
#[cfg(feature = "opening-session")]
fn opening_create_begin(
    request: &crate::CreateRequest,
    input: &[u8],
) -> Result<RuntimeOutcome, StatusCode> {
    if request.owner_token == 0
        || input.len() > MAX_SOURCE_CHUNK_BYTES as usize
        || (request.expected_total_bytes != 0 && input.len() as u64 > request.expected_total_bytes)
        || request.config.struct_size != size_of::<crate::SessionConfig>() as u32
    {
        return Err(StatusCode::InvalidArgument);
    }
    if request.config.max_document_bytes != 0
        && (request.expected_total_bytes > request.config.max_document_bytes
            || input.len() as u64 > request.config.max_document_bytes)
    {
        return Err(StatusCode::ResourceLimitExceeded);
    }
    let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
    let session = registry.allocate_handle()?;
    let transaction = registry.allocate_handle()?;
    let document = DocumentActor::begin_opening().map_err(|error| map_actor_error(&error))?;
    let mut opening = OpeningTransaction {
        transaction,
        received_bytes: 0,
        expected_bytes: request.expected_total_bytes,
        utf8_carry: Vec::new(),
    };
    opening_stage_bytes(&document, &mut opening, input)?;
    registry.sessions.insert(
        session,
        StoredSession {
            owner: request.owner_token,
            state: StoredSessionState::Open(document),
            opening: Some(opening),
            transactions: BTreeSet::new(),
            max_document_bytes: request.config.max_document_bytes,
            progress_token: 0,
            continuations: BTreeSet::new(),
            anchors: BTreeSet::new(),
            history_budget_bytes: request.config.history_budget_bytes,
            history_used_bytes: 0,
            history_head: 0,
            history_tail: 0,
            history_state: 1,
            next_history_state: 2,
            history_token_count: 0,
            evicted_history_tokens: BTreeSet::new(),
            terminal_edit_intent: None,
            terminal_source_transaction: None,
            close_token: 0,
            close_complete: false,
        },
    );
    Ok(RuntimeOutcome {
        operation: OperationCode::CreateBegin,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::SessionCreated {
            session: SessionHandle(session),
            transaction: TransactionHandle(transaction),
        },
    })
}

/// Stages raw transport bytes into the opening actor: at most three carried
/// bytes join the chunk, the longest valid UTF-8 prefix splits into pages
/// bounded by the store's UTF-16 page cap, an incomplete trailing scalar
/// carries to the next chunk, and an actually invalid sequence fails typed.
#[cfg(feature = "opening-session")]
fn opening_stage_bytes(
    document: &DocumentActor,
    opening: &mut OpeningTransaction,
    input: &[u8],
) -> Result<(), StatusCode> {
    let next_received = opening
        .received_bytes
        .checked_add(input.len() as u64)
        .ok_or(StatusCode::ResourceLimitExceeded)?;
    let joined;
    let bytes: &[u8] = if opening.utf8_carry.is_empty() {
        input
    } else {
        let mut buffer = Vec::with_capacity(opening.utf8_carry.len().saturating_add(input.len()));
        buffer.extend_from_slice(&opening.utf8_carry);
        buffer.extend_from_slice(input);
        joined = buffer;
        &joined
    };
    let (text, carry) = match std::str::from_utf8(bytes) {
        Ok(text) => (text, &[][..]),
        Err(error) => {
            if error.error_len().is_some() {
                return Err(StatusCode::InvalidUtf8);
            }
            let (head, tail) = bytes.split_at(error.valid_up_to());
            let head = std::str::from_utf8(head).map_err(|_| StatusCode::InternalFault)?;
            (head, tail)
        }
    };
    let mut remaining = text;
    while !remaining.is_empty() {
        let cut = utf16_bounded_prefix(remaining, flark_runtime::SOURCE_SEED_PAGE_MAX_UTF16);
        document
            .opening_append_page(remaining[..cut].to_owned())
            .map_err(|error| map_actor_error(&error))?;
        remaining = &remaining[cut..];
    }
    opening.utf8_carry = carry.to_vec();
    opening.received_bytes = next_received;
    Ok(())
}

/// Returns the longest prefix byte length whose UTF-16 width fits the cap.
#[cfg(feature = "opening-session")]
fn utf16_bounded_prefix(text: &str, max_utf16: usize) -> usize {
    let mut units = 0;
    for (offset, character) in text.char_indices() {
        let width = character.len_utf16();
        if units + width > max_utf16 {
            return offset;
        }
        units += width;
    }
    text.len()
}

fn emit<F>(operation: OperationCode, outcome: *mut Outcome, call: F) -> u32
where
    F: FnOnce() -> Result<RuntimeOutcome, StatusCode>,
{
    if outcome.is_null() {
        return StatusCode::InvalidArgument as u32;
    }
    let runtime = match catch_unwind(AssertUnwindSafe(call)) {
        Ok(Ok(value)) => value,
        Ok(Err(status)) => runtime_error(operation, status),
        Err(_) => runtime_error(operation, StatusCode::PanicContained),
    };
    let encoded = Outcome::from_runtime(runtime).unwrap_or_else(|| {
        Outcome::from_runtime(runtime_error(operation, StatusCode::InternalFault))
            .expect("internal fault outcome is contract valid")
    });
    unsafe {
        ptr::write_unaligned(outcome, encoded);
    }
    encoded.status
}

unsafe fn read_record<T: Copy>(value: *const T, expected_size: u32) -> Result<T, StatusCode> {
    if value.is_null() {
        return Err(StatusCode::InvalidArgument);
    }
    let record = unsafe { ptr::read_unaligned(value) };
    let struct_size = unsafe { ptr::read_unaligned(value.cast::<u32>()) };
    if struct_size != expected_size {
        return Err(StatusCode::InvalidArgument);
    }
    Ok(record)
}

unsafe fn borrowed_bytes<'a>(input: *const u8, len: u64) -> Result<&'a [u8], StatusCode> {
    let len = usize::try_from(len).map_err(|_| StatusCode::InvalidArgument)?;
    if len == 0 {
        return Ok(&[]);
    }
    if input.is_null() {
        return Err(StatusCode::InvalidArgument);
    }
    Ok(unsafe { slice::from_raw_parts(input, len) })
}

fn valid_budget(budget: WorkBudget, page: bool) -> bool {
    budget.max_work_units != 0
        && budget.max_result_items <= MAX_QUERY_ITEMS
        && budget.max_result_bytes <= MAX_RESULT_BYTES
        && (!page || (budget.max_result_items != 0 && budget.max_result_bytes != 0))
}

fn owned_session_entry<'a>(
    registry: &'a mut Registry,
    session: SessionRef,
) -> Result<&'a mut StoredSession, StatusCode> {
    if session.session == 0 || session.owner_token == 0 {
        return Err(StatusCode::InvalidHandle);
    }
    let entry = registry
        .sessions
        .get_mut(&session.session)
        .ok_or(StatusCode::InvalidHandle)?;
    if entry.owner != session.owner_token {
        return Err(StatusCode::OwnerMismatch);
    }
    Ok(entry)
}

fn session_entry<'a>(
    registry: &'a mut Registry,
    session: SessionRef,
) -> Result<&'a mut StoredSession, StatusCode> {
    let entry = owned_session_entry(registry, session)?;
    if entry.close_token != 0 {
        return Err(StatusCode::SessionClosing);
    }
    Ok(entry)
}

/// H1 keeps the measured legacy literal lane temporarily. A later ordered
/// mutation at the terminal's result revision proves that Core received and
/// adopted that terminal, so it is a safe implicit acknowledgement. A caller
/// that lost the reply cannot know or submit the result revision.
fn acknowledge_edit_terminal_for_ordered_mutation(
    entry: &mut StoredSession,
    expected_revision: u64,
) {
    if entry
        .terminal_edit_intent
        .as_ref()
        .is_some_and(|terminal| terminal.receipt.result_revision == expected_revision)
    {
        entry.terminal_edit_intent = None;
    }
    if entry
        .terminal_source_transaction
        .as_ref()
        .is_some_and(|terminal| terminal.receipt.result_revision == expected_revision)
    {
        entry.terminal_source_transaction = None;
    }
}

fn detach_history(registry: &mut Registry, token: u64) -> Option<StoredHistory> {
    let history = registry.histories.remove(&token)?;
    if history.previous != 0 {
        if let Some(previous) = registry.histories.get_mut(&history.previous) {
            previous.next = history.next;
        }
    }
    if history.next != 0 {
        if let Some(next) = registry.histories.get_mut(&history.next) {
            next.previous = history.previous;
        }
    }
    if let Some(entry) = registry.sessions.get_mut(&history.session) {
        if entry.history_head == token {
            entry.history_head = history.next;
        }
        if entry.history_tail == token {
            entry.history_tail = history.previous;
        }
        entry.history_used_bytes = entry
            .history_used_bytes
            .saturating_sub(history.retained_bytes);
        entry.history_token_count = entry.history_token_count.saturating_sub(1);
    }
    Some(history)
}

fn retain_history(
    registry: &mut Registry,
    session: SessionRef,
    applies_state: u64,
    target_state: u64,
    start_byte: u64,
    end_byte: u64,
    replacement: Vec<u8>,
    group_id: u64,
) -> Result<(HistoryToken, HistoryDisposition), StatusCode> {
    let retained_bytes = HISTORY_TOKEN_OVERHEAD_BYTES
        .checked_add(replacement.len() as u64)
        .ok_or(StatusCode::ResourceLimitExceeded)?;
    let history_budget_bytes = session_entry(registry, session)?.history_budget_bytes;
    if history_budget_bytes == 0 {
        return Ok((HistoryToken::NONE, HistoryDisposition::Disabled));
    }
    if retained_bytes > history_budget_bytes {
        return Ok((HistoryToken::NONE, HistoryDisposition::OverBudget));
    }

    loop {
        let entry = session_entry(registry, session)?;
        if entry
            .history_used_bytes
            .checked_add(retained_bytes)
            .is_some_and(|used| used <= history_budget_bytes)
        {
            break;
        }
        let oldest = entry.history_head;
        if oldest == 0 {
            return Err(StatusCode::InternalFault);
        }
        let evicted = detach_history(registry, oldest).ok_or(StatusCode::InternalFault)?;
        if evicted.session != session.session || evicted.owner != session.owner_token {
            return Err(StatusCode::InternalFault);
        }
        record_evicted_history_token(session_entry(registry, session)?, oldest);
    }

    let token = registry.allocate_handle()?;
    let previous = session_entry(registry, session)?.history_tail;
    if previous != 0 {
        registry
            .histories
            .get_mut(&previous)
            .ok_or(StatusCode::InternalFault)?
            .next = token;
    }
    registry.histories.insert(
        token,
        StoredHistory {
            session: session.session,
            owner: session.owner_token,
            applies_state,
            target_state,
            group_id,
            splices: vec![StoredHistorySplice {
                start_byte,
                end_byte,
                replacement,
            }],
            retained_bytes,
            previous,
            next: 0,
        },
    );
    let entry = session_entry(registry, session)?;
    if entry.history_head == 0 {
        entry.history_head = token;
    }
    entry.history_tail = token;
    entry.history_used_bytes = entry
        .history_used_bytes
        .checked_add(retained_bytes)
        .ok_or(StatusCode::ResourceLimitExceeded)?;
    entry.history_token_count = entry
        .history_token_count
        .checked_add(1)
        .ok_or(StatusCode::ResourceLimitExceeded)?;
    Ok((HistoryToken(token), HistoryDisposition::Retained))
}

/// Reserves one exact-capacity interactive inverse without evicting existing
/// history. Authoritative editor mutations are history-required: insufficient
/// maintained headroom is a mutation-free rejection rather than a committed
/// edit with a missing undo unit.
fn reserve_edit_intent_history(
    registry: &mut Registry,
    session: SessionRef,
    applies_state: u64,
    target_state: u64,
    inverse_capacity: usize,
    group_id: u64,
) -> Result<u64, StatusCode> {
    let retained_bytes = HISTORY_TOKEN_OVERHEAD_BYTES
        .checked_add(
            u64::try_from(inverse_capacity).map_err(|_| StatusCode::ResourceLimitExceeded)?,
        )
        .ok_or(StatusCode::ResourceLimitExceeded)?;
    let entry = session_entry(registry, session)?;
    if entry.history_budget_bytes < retained_bytes
        || entry
            .history_used_bytes
            .checked_add(retained_bytes)
            .is_none_or(|used| used > entry.history_budget_bytes)
    {
        return Err(StatusCode::ResourceLimitExceeded);
    }

    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(inverse_capacity)
        .map_err(|_| StatusCode::AllocationFailure)?;
    let mut splices = Vec::new();
    splices
        .try_reserve_exact(1)
        .map_err(|_| StatusCode::AllocationFailure)?;
    splices.push(StoredHistorySplice {
        start_byte: 0,
        end_byte: 0,
        replacement,
    });
    let token = registry.allocate_handle()?;
    let previous = session_entry(registry, session)?.history_tail;
    registry.histories.insert(
        token,
        StoredHistory {
            session: session.session,
            owner: session.owner_token,
            applies_state,
            target_state,
            group_id,
            splices,
            retained_bytes,
            previous,
            next: 0,
        },
    );
    if previous != 0 {
        registry
            .histories
            .get_mut(&previous)
            .expect("reserved history predecessor must remain live")
            .next = token;
    }
    let entry = session_entry(registry, session)?;
    if entry.history_head == 0 {
        entry.history_head = token;
    }
    entry.history_tail = token;
    entry.history_used_bytes += retained_bytes;
    entry.history_token_count += 1;
    Ok(token)
}

/// Completes a preallocated semantic-history slot after source commit. Every
/// operation here is bounded and allocation-free.
fn finalize_edit_intent_history(
    registry: &mut Registry,
    token: u64,
    start_byte: u64,
    end_byte: u64,
    inverse: &[u8],
) {
    let actual_retained = HISTORY_TOKEN_OVERHEAD_BYTES + inverse.len() as u64;
    let (session, released_reservation) = {
        let history = registry
            .histories
            .get_mut(&token)
            .expect("semantic history reservation must remain live");
        let splice = history
            .splices
            .first_mut()
            .expect("reserved history must own one splice");
        debug_assert!(inverse.len() <= splice.replacement.capacity());
        splice.start_byte = start_byte;
        splice.end_byte = end_byte;
        splice.replacement.extend_from_slice(inverse);
        let released = history.retained_bytes - actual_retained;
        history.retained_bytes = actual_retained;
        (history.session, released)
    };
    let entry = registry
        .sessions
        .get_mut(&session)
        .expect("semantic history session must remain live");
    entry.history_used_bytes -= released_reservation;
}

#[derive(Clone, Copy)]
enum SourceHistoryReservation {
    New(u64),
    Extend {
        token: u64,
        reserved_bytes: u64,
        applies_state: u64,
    },
}

impl SourceHistoryReservation {
    const fn token(self) -> u64 {
        match self {
            Self::New(token) | Self::Extend { token, .. } => token,
        }
    }

    const fn extended(self) -> bool {
        matches!(self, Self::Extend { .. })
    }
}

fn reserve_source_transaction_history(
    registry: &mut Registry,
    session: SessionRef,
    applies_state: u64,
    target_state: u64,
    inverse_capacity: usize,
    group_id: u64,
) -> Result<SourceHistoryReservation, StatusCode> {
    if group_id == 0 {
        return reserve_edit_intent_history(
            registry,
            session,
            applies_state,
            target_state,
            inverse_capacity,
            0,
        )
        .map(SourceHistoryReservation::New);
    }

    let candidate = session_entry(registry, session)?.history_tail;
    let can_extend = candidate != 0
        && registry.histories.get(&candidate).is_some_and(|history| {
            history.session == session.session
                && history.owner == session.owner_token
                && history.group_id == group_id
                && history.applies_state == target_state
                && history.next == 0
                && history.splices.len() < MAX_COMPOSITE_HISTORY_STEPS
        });
    if !can_extend {
        return reserve_edit_intent_history(
            registry,
            session,
            applies_state,
            target_state,
            inverse_capacity,
            group_id,
        )
        .map(SourceHistoryReservation::New);
    }

    let reserved_bytes = HISTORY_SPLICE_OVERHEAD_BYTES
        .checked_add(
            u64::try_from(inverse_capacity).map_err(|_| StatusCode::ResourceLimitExceeded)?,
        )
        .ok_or(StatusCode::ResourceLimitExceeded)?;
    {
        let entry = session_entry(registry, session)?;
        if entry
            .history_used_bytes
            .checked_add(reserved_bytes)
            .is_none_or(|used| used > entry.history_budget_bytes)
        {
            return Err(StatusCode::ResourceLimitExceeded);
        }
    }
    registry
        .histories
        .get_mut(&candidate)
        .expect("validated composite history tail must remain live")
        .splices
        .try_reserve(1)
        .map_err(|_| StatusCode::AllocationFailure)?;
    let history = registry
        .histories
        .get_mut(&candidate)
        .expect("reserved composite history tail must remain live");
    history.retained_bytes += reserved_bytes;
    session_entry(registry, session)?.history_used_bytes += reserved_bytes;
    Ok(SourceHistoryReservation::Extend {
        token: candidate,
        reserved_bytes,
        applies_state,
    })
}

fn rollback_source_history_reservation(
    registry: &mut Registry,
    reservation: SourceHistoryReservation,
) {
    match reservation {
        SourceHistoryReservation::New(token) => {
            let _ = detach_history(registry, token);
        }
        SourceHistoryReservation::Extend {
            token,
            reserved_bytes,
            ..
        } => {
            let Some(history) = registry.histories.get_mut(&token) else {
                return;
            };
            history.retained_bytes = history.retained_bytes.saturating_sub(reserved_bytes);
            if let Some(entry) = registry.sessions.get_mut(&history.session) {
                entry.history_used_bytes = entry.history_used_bytes.saturating_sub(reserved_bytes);
            }
        }
    }
}

fn finalize_source_transaction_history(
    registry: &mut Registry,
    reservation: SourceHistoryReservation,
    start_byte: u64,
    end_byte: u64,
    inverse: Vec<u8>,
) {
    match reservation {
        SourceHistoryReservation::New(token) => {
            finalize_edit_intent_history(registry, token, start_byte, end_byte, &inverse);
        }
        SourceHistoryReservation::Extend {
            token,
            reserved_bytes,
            applies_state,
        } => {
            let actual_retained = HISTORY_SPLICE_OVERHEAD_BYTES + inverse.len() as u64;
            let history = registry
                .histories
                .get_mut(&token)
                .expect("reserved composite history tail must remain live");
            history.applies_state = applies_state;
            history.splices.push(StoredHistorySplice {
                start_byte,
                end_byte,
                replacement: inverse,
            });
            let released = reserved_bytes - actual_retained;
            history.retained_bytes -= released;
            registry
                .sessions
                .get_mut(&history.session)
                .expect("composite history session must remain live")
                .history_used_bytes -= released;
        }
    }
}

/// Installs an already-staged inverse as a required standalone undo unit.
/// All allocation and history-headroom checks complete before source mutation;
/// finalization after the actor receipt only changes scalar fields.
fn reserve_staged_source_history(
    registry: &mut Registry,
    session: SessionRef,
    applies_state: u64,
    target_state: u64,
    start_byte: u64,
    replay_end: u64,
    inverse: Vec<u8>,
) -> Result<u64, (StatusCode, Vec<u8>)> {
    let retained_bytes = match HISTORY_TOKEN_OVERHEAD_BYTES.checked_add(inverse.len() as u64) {
        Some(bytes) => bytes,
        None => return Err((StatusCode::ResourceLimitExceeded, inverse)),
    };
    let (history_budget_bytes, history_used_bytes, previous) =
        match session_entry(registry, session) {
            Ok(entry) => (
                entry.history_budget_bytes,
                entry.history_used_bytes,
                entry.history_tail,
            ),
            Err(error) => return Err((error, inverse)),
        };
    if history_budget_bytes < retained_bytes
        || history_used_bytes
            .checked_add(retained_bytes)
            .is_none_or(|used| used > history_budget_bytes)
    {
        return Err((StatusCode::ResourceLimitExceeded, inverse));
    }
    let mut splices = Vec::new();
    if splices.try_reserve_exact(1).is_err() {
        return Err((StatusCode::AllocationFailure, inverse));
    }
    splices.push(StoredHistorySplice {
        start_byte,
        end_byte: replay_end,
        replacement: inverse,
    });
    let token = match registry.allocate_handle() {
        Ok(token) => token,
        Err(error) => return Err((error, splices.pop().unwrap().replacement)),
    };
    registry.histories.insert(
        token,
        StoredHistory {
            session: session.session,
            owner: session.owner_token,
            applies_state,
            target_state,
            group_id: 0,
            splices,
            retained_bytes,
            previous,
            next: 0,
        },
    );
    if previous != 0 {
        registry
            .histories
            .get_mut(&previous)
            .expect("reserved staged-history predecessor must remain live")
            .next = token;
    }
    let entry = registry
        .sessions
        .get_mut(&session.session)
        .expect("reserved staged-history session must remain live");
    if entry.history_head == 0 {
        entry.history_head = token;
    }
    entry.history_tail = token;
    entry.history_used_bytes += retained_bytes;
    entry.history_token_count += 1;
    Ok(token)
}

fn rollback_staged_source_history(registry: &mut Registry, token: u64) -> Vec<u8> {
    let mut history = detach_history(registry, token)
        .expect("staged source history reservation must remain live");
    history
        .splices
        .pop()
        .expect("staged source history must own one splice")
        .replacement
}

fn edit_intent_output_requirement() -> u64 {
    size_of::<EditIntentReceiptV1>() as u64 + u64::from(MAX_SMALL_EDIT_BYTES)
}

unsafe fn write_edit_intent_terminal(terminal: &StoredEditIntentTerminal, output: *mut u8) {
    unsafe {
        ptr::write_unaligned(output.cast::<EditIntentReceiptV1>(), terminal.receipt);
        ptr::copy_nonoverlapping(
            terminal.replacement.as_ptr(),
            output.add(size_of::<EditIntentReceiptV1>()),
            terminal.replacement.len(),
        );
    }
}

fn edit_intent_terminal_outcome(terminal: &StoredEditIntentTerminal) -> RuntimeOutcome {
    let written = size_of::<EditIntentReceiptV1>() as u64 + terminal.replacement.len() as u64;
    let has_commit = terminal.receipt.flags & EDIT_INTENT_RECEIPT_HAS_COMMIT != 0;
    RuntimeOutcome {
        operation: OperationCode::EditIntentV1,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: written,
        result: if has_commit {
            OperationResult::RevisionCommitted {
                revision: Revision(terminal.receipt.result_revision),
                history_token: HistoryToken(terminal.receipt.history_token),
                history: HistoryDisposition::Retained,
            }
        } else {
            OperationResult::None
        },
    }
}

fn source_transaction_output_requirement() -> u64 {
    size_of::<SourceTransactionReceiptV1>() as u64
}

unsafe fn write_source_transaction_terminal(
    terminal: &StoredSourceTransactionTerminal,
    output: *mut u8,
) {
    unsafe {
        ptr::write_unaligned(
            output.cast::<SourceTransactionReceiptV1>(),
            terminal.receipt,
        );
    }
}

fn source_transaction_terminal_outcome(
    terminal: &StoredSourceTransactionTerminal,
) -> RuntimeOutcome {
    source_transaction_terminal_outcome_for(terminal, OperationCode::SourceTransactionV1)
}

fn source_transaction_terminal_outcome_for(
    terminal: &StoredSourceTransactionTerminal,
    operation: OperationCode,
) -> RuntimeOutcome {
    RuntimeOutcome {
        operation,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: size_of::<SourceTransactionReceiptV1>() as u64,
        result: OperationResult::RevisionCommitted {
            revision: Revision(terminal.receipt.result_revision),
            history_token: HistoryToken(terminal.receipt.history_token),
            history: HistoryDisposition::Retained,
        },
    }
}

/// Maps one anchored byte offset through a committed splice of
/// `deleted_len` bytes replaced by `inserted_len` bytes at `start`.
///
/// Offsets strictly inside the deleted span collapse to the splice edge named
/// by the anchor's affinity; an offset exactly at `start` moves with the
/// insertion only for `Downstream`. Every input offset is a scalar boundary
/// and every produced offset is one, because splices only occur at validated
/// scalar boundaries.
fn map_offset_through_edit(
    offset: u64,
    affinity: Affinity,
    start: u64,
    deleted_len: u64,
    inserted_len: u64,
) -> u64 {
    let deleted_end = start.saturating_add(deleted_len);
    if offset < start {
        return offset;
    }
    if offset == start {
        return match affinity {
            Affinity::Upstream => start,
            Affinity::Downstream => start.saturating_add(inserted_len),
        };
    }
    if offset >= deleted_end {
        return offset - deleted_len + inserted_len;
    }
    match affinity {
        Affinity::Upstream => start,
        Affinity::Downstream => start.saturating_add(inserted_len),
    }
}

/// Eagerly transforms every live anchor of `session` through one committed
/// splice, keeping all anchors at the current revision. Work is bounded by
/// `MAX_LIVE_ANCHORS`.
fn transform_session_anchors(
    registry: &mut Registry,
    session: u64,
    start: u64,
    deleted_len: u64,
    inserted_len: u64,
) {
    for anchor in registry.anchors.values_mut() {
        if anchor.session == session {
            anchor.byte_offset = map_offset_through_edit(
                anchor.byte_offset,
                anchor.affinity,
                start,
                deleted_len,
                inserted_len,
            );
        }
    }
}

/// Resolves one anchor handle for a validated owned session, distinguishing a
/// handle of another kind from an unknown or consumed one.
fn anchor_for_request(
    registry: &Registry,
    session: SessionRef,
    anchor: u64,
) -> Result<&StoredAnchor, StatusCode> {
    if anchor == 0 {
        return Err(StatusCode::InvalidArgument);
    }
    match registry.anchors.get(&anchor) {
        Some(stored) if stored.session == session.session => Ok(stored),
        Some(_) => Err(StatusCode::InvalidHandle),
        None => {
            if registry.sessions.contains_key(&anchor)
                || registry.transactions.contains_key(&anchor)
                || registry.continuations.contains_key(&anchor)
                || registry.histories.contains_key(&anchor)
            {
                Err(StatusCode::WrongHandleKind)
            } else {
                Err(StatusCode::InvalidHandle)
            }
        }
    }
}

fn history_for_request(
    registry: &mut Registry,
    session: SessionRef,
    token: u64,
) -> Result<StoredHistory, StatusCode> {
    let entry = session_entry(registry, session)?;
    if entry.evicted_history_tokens.contains(&token) {
        return Err(StatusCode::HistoryTokenEvicted);
    }
    let history = registry
        .histories
        .get(&token)
        .cloned()
        .ok_or(StatusCode::HistoryTokenStale)?;
    if history.owner != session.owner_token {
        return Err(StatusCode::OwnerMismatch);
    }
    if history.session != session.session {
        return Err(StatusCode::HistoryTokenStale);
    }
    Ok(history)
}

#[no_mangle]
pub extern "C" fn flark_v4_negotiate(
    request: *const NegotiateRequest,
    info: *mut AbiInfo,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::Negotiate, outcome, || {
        let request = unsafe { read_record(request, size_of::<NegotiateRequest>() as u32)? };
        if info.is_null() {
            return Err(StatusCode::InvalidArgument);
        }
        // Negotiation is intentionally exact-minor. The ABI has no retained
        // per-client negotiation state, so accepting an older minor could not
        // tailor later row flags or record vocabulary safely for that client.
        if request.requested_major != ABI_MAJOR || request.requested_minor != ABI_MINOR {
            return Err(StatusCode::UnsupportedAbiVersion);
        }
        if request.required_capability_bits & !IMPLEMENTED_CAPABILITIES != 0 {
            return Err(StatusCode::UnsupportedCapability);
        }
        let value = AbiInfo {
            struct_size: size_of::<AbiInfo>() as u32,
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
            capability_bits: IMPLEMENTED_CAPABILITIES,
            max_small_edit_bytes: MAX_SMALL_EDIT_BYTES,
            max_bulk_chunk_bytes: MAX_BULK_CHUNK_BYTES,
            max_source_chunk_bytes: MAX_SOURCE_CHUNK_BYTES,
            max_result_bytes: MAX_RESULT_BYTES,
            max_query_items: MAX_QUERY_ITEMS,
            max_transaction_edits: MAX_TRANSACTION_EDITS,
            reserved: [0; 3],
        };
        unsafe {
            ptr::write_unaligned(info, value);
        }
        Ok(RuntimeOutcome {
            operation: OperationCode::Negotiate,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_create_begin(
    request: *const CreateRequest,
    input: *const u8,
    input_len: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::CreateBegin, outcome, || {
        let request = unsafe { read_record(request, size_of::<CreateRequest>() as u32)? };
        let input = unsafe { borrowed_bytes(input, input_len)? };
        if request.flags & crate::CREATE_FLAG_OPENING != 0 {
            #[cfg(not(feature = "opening-session"))]
            return Err(StatusCode::InvalidArgument);
            #[cfg(feature = "opening-session")]
            return opening_create_begin(&request, input);
        }
        if request.owner_token == 0
            || input.len() > MAX_SOURCE_CHUNK_BYTES as usize
            || input.len() as u64 > request.expected_total_bytes
            || request.config.struct_size != size_of::<crate::SessionConfig>() as u32
        {
            return Err(StatusCode::InvalidArgument);
        }
        let expected_bytes = usize::try_from(request.expected_total_bytes)
            .map_err(|_| StatusCode::ResourceLimitExceeded)?;
        if request.config.max_document_bytes != 0
            && request.expected_total_bytes > request.config.max_document_bytes
        {
            return Err(StatusCode::ResourceLimitExceeded);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let session = registry.allocate_handle()?;
        let transaction = registry.allocate_handle()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(expected_bytes)
            .map_err(|_| StatusCode::AllocationFailure)?;
        bytes.extend_from_slice(input);
        registry.sessions.insert(
            session,
            StoredSession {
                owner: request.owner_token,
                state: StoredSessionState::Creating {
                    transaction,
                    expected_bytes,
                    bytes,
                },
                #[cfg(feature = "opening-session")]
                opening: None,
                transactions: BTreeSet::new(),
                max_document_bytes: request.config.max_document_bytes,
                progress_token: 0,
                continuations: BTreeSet::new(),
                anchors: BTreeSet::new(),
                history_budget_bytes: request.config.history_budget_bytes,
                history_used_bytes: 0,
                history_head: 0,
                history_tail: 0,
                history_state: 1,
                next_history_state: 2,
                history_token_count: 0,
                evicted_history_tokens: BTreeSet::new(),
                terminal_edit_intent: None,
                terminal_source_transaction: None,
                close_token: 0,
                close_complete: false,
            },
        );
        Ok(RuntimeOutcome {
            operation: OperationCode::CreateBegin,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::SessionCreated {
                session: SessionHandle(session),
                transaction: TransactionHandle(transaction),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_create_append(
    request: *const StageRequest,
    input: *const u8,
    input_len: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::CreateAppend, outcome, || {
        let request = unsafe { read_record(request, size_of::<StageRequest>() as u32)? };
        let input = unsafe { borrowed_bytes(input, input_len)? };
        if input.len() > MAX_SOURCE_CHUNK_BYTES as usize || request.chunk_len != input_len {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = session_entry(&mut registry, request.session)?;
        #[cfg(feature = "opening-session")]
        let max_document_bytes = entry.max_document_bytes;
        #[cfg(feature = "opening-session")]
        if let Some(opening) = entry.opening.as_mut() {
            if opening.transaction != request.transaction
                || request.chunk_offset != opening.received_bytes
            {
                return Err(StatusCode::TransactionConflict);
            }
            let next_received = opening
                .received_bytes
                .checked_add(input.len() as u64)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            if opening.expected_bytes != 0 && next_received > opening.expected_bytes {
                return Err(StatusCode::TransactionConflict);
            }
            if max_document_bytes != 0 && next_received > max_document_bytes {
                return Err(StatusCode::ResourceLimitExceeded);
            }
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::InternalFault);
            };
            opening_stage_bytes(document, opening, input)?;
            return Ok(RuntimeOutcome {
                operation: OperationCode::CreateAppend,
                status: StatusCode::Ok,
                progress: ProgressState::Complete,
                required_payload_bytes: 0,
                written_payload_bytes: 0,
                result: OperationResult::TransactionStaged {
                    transaction: TransactionHandle(request.transaction),
                },
            });
        }
        let StoredSessionState::Creating {
            transaction,
            expected_bytes,
            bytes,
        } = &mut entry.state
        else {
            return Err(StatusCode::TransactionAlreadyCommitted);
        };
        if *transaction != request.transaction
            || request.chunk_offset != bytes.len() as u64
            || bytes.len().saturating_add(input.len()) > *expected_bytes
        {
            return Err(StatusCode::TransactionConflict);
        }
        bytes.extend_from_slice(input);
        Ok(RuntimeOutcome {
            operation: OperationCode::CreateAppend,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::TransactionStaged {
                transaction: TransactionHandle(*transaction),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_create_commit(
    request: *const TransactionRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::CreateCommit, outcome, || {
        let request = unsafe { read_record(request, size_of::<TransactionRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.expected_revision != 0
            || request.progress_token != 0
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = session_entry(&mut registry, request.session)?;
        #[cfg(feature = "opening-session")]
        if let Some(opening) = entry.opening.as_ref() {
            if opening.transaction != request.transaction {
                return Err(StatusCode::TransactionConflict);
            }
            if !opening.utf8_carry.is_empty() {
                return Err(StatusCode::InvalidUtf8);
            }
            if opening.expected_bytes != 0 && opening.received_bytes != opening.expected_bytes {
                return Err(StatusCode::TransactionIncomplete);
            }
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::InternalFault);
            };
            document
                .seal_opening()
                .map_err(|error| map_actor_error(&error))?;
            let receipt = document
                .pump(usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX))
                .map_err(|error| map_actor_error(&error))?;
            let revision = document
                .inspect()
                .map_err(|error| map_actor_error(&error))?
                .revision;
            entry.opening = None;
            entry.progress_token = entry.progress_token.max(1);
            return Ok(if receipt.phase == DocumentSessionPhase::Ready {
                RuntimeOutcome {
                    operation: OperationCode::CreateCommit,
                    status: StatusCode::Ok,
                    progress: ProgressState::Complete,
                    required_payload_bytes: 0,
                    written_payload_bytes: 0,
                    result: OperationResult::RevisionCreated {
                        session: SessionHandle(request.session.session),
                        revision: Revision(revision),
                    },
                }
            } else {
                RuntimeOutcome {
                    operation: OperationCode::CreateCommit,
                    status: StatusCode::BudgetExhausted,
                    progress: ProgressState::BudgetExhausted,
                    required_payload_bytes: 0,
                    written_payload_bytes: 0,
                    result: OperationResult::Progress {
                        revision: Revision(revision),
                        token: ProgressToken(entry.progress_token),
                    },
                }
            });
        }
        let state = std::mem::replace(
            &mut entry.state,
            StoredSessionState::Creating {
                transaction: 0,
                expected_bytes: 0,
                bytes: Vec::new(),
            },
        );
        let StoredSessionState::Creating {
            transaction,
            expected_bytes,
            bytes,
        } = state
        else {
            entry.state = state;
            return Err(StatusCode::TransactionAlreadyCommitted);
        };
        if transaction != request.transaction || bytes.len() != expected_bytes {
            entry.state = StoredSessionState::Creating {
                transaction,
                expected_bytes,
                bytes,
            };
            return Err(StatusCode::TransactionIncomplete);
        }
        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            Err(error) => {
                entry.state = StoredSessionState::Creating {
                    transaction,
                    expected_bytes,
                    bytes: error.into_bytes(),
                };
                return Err(StatusCode::InvalidUtf8);
            }
        };
        let document = DocumentActor::begin(source).map_err(|error| map_actor_error(&error))?;
        let receipt = document
            .pump(usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX))
            .map_err(|error| map_actor_error(&error))?;
        let ready = receipt.phase == DocumentSessionPhase::Ready;
        entry.state = StoredSessionState::Open(document);
        entry.progress_token = 1;
        Ok(if ready {
            RuntimeOutcome {
                operation: OperationCode::CreateCommit,
                status: StatusCode::Ok,
                progress: ProgressState::Complete,
                required_payload_bytes: 0,
                written_payload_bytes: 0,
                result: OperationResult::RevisionCreated {
                    session: SessionHandle(request.session.session),
                    revision: Revision(1),
                },
            }
        } else {
            RuntimeOutcome {
                operation: OperationCode::CreateCommit,
                status: StatusCode::BudgetExhausted,
                progress: ProgressState::BudgetExhausted,
                required_payload_bytes: 0,
                written_payload_bytes: 0,
                result: OperationResult::Progress {
                    revision: Revision(1),
                    token: ProgressToken(entry.progress_token),
                },
            }
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_create_abort(
    request: *const TransactionRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::CreateAbort, outcome, || {
        let request = unsafe { read_record(request, size_of::<TransactionRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.transaction == 0
            || request.expected_revision != 0
            || request.progress_token != 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Creating { transaction, .. } = &entry.state else {
                return Err(StatusCode::TransactionAlreadyCommitted);
            };
            if *transaction != request.transaction {
                return Err(StatusCode::TransactionConflict);
            }
        }
        registry.sessions.remove(&request.session.session);
        Ok(RuntimeOutcome {
            operation: OperationCode::CreateAbort,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_pump(request: *const PumpRequest, outcome: *mut Outcome) -> u32 {
    emit(OperationCode::Pump, outcome, || {
        let request = unsafe { read_record(request, size_of::<PumpRequest>() as u32)? };
        if !valid_budget(request.budget, false) {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = session_entry(&mut registry, request.session)?;
        if entry.progress_token == 0 {
            if request.progress_token != 0 {
                return Err(StatusCode::StaleProgressToken);
            }
            entry.progress_token = 1;
        } else if request.progress_token != entry.progress_token {
            return Err(StatusCode::StaleProgressToken);
        }
        let StoredSessionState::Open(document) = &mut entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if request.expected_revision != inspection.revision {
            return Err(StatusCode::StaleRevision);
        }
        let receipt = document
            .pump(usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX))
            .map_err(|error| map_actor_error(&error))?;
        let revision = receipt.revision;
        let ready = receipt.phase == DocumentSessionPhase::Ready;
        // Completed progress is not active work: the receipt echoes the final
        // token, but the stored token clears so the session reads as idle for
        // owner migration and a later pump chain starts from zero.
        let result_token = if ready {
            std::mem::take(&mut entry.progress_token)
        } else {
            entry.progress_token = entry
                .progress_token
                .checked_add(1)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            entry.progress_token
        };
        Ok(RuntimeOutcome {
            operation: OperationCode::Pump,
            status: if ready {
                StatusCode::Ok
            } else {
                StatusCode::BudgetExhausted
            },
            progress: if ready {
                ProgressState::Complete
            } else {
                ProgressState::BudgetExhausted
            },
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::Progress {
                revision: Revision(revision),
                token: ProgressToken(result_token),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_small_edit(
    request: *const SmallEditRequest,
    edits: *const EditDescriptor,
    edit_count: u32,
    replacement_bytes: *const u8,
    replacement_bytes_len: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::SmallEdit, outcome, || {
        let request = unsafe { read_record(request, size_of::<SmallEditRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.edit_count != edit_count
            || request.replacement_bytes_len != replacement_bytes_len
            || edit_count != 1
            || edits.is_null()
            || request.reserved_u32 != 0
            || request.reserved != [0; 2]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let replacement = unsafe { borrowed_bytes(replacement_bytes, replacement_bytes_len)? };
        let replacement = str::from_utf8(replacement).map_err(|_| StatusCode::InvalidUtf8)?;
        let edit = unsafe { ptr::read_unaligned(edits) };
        let deleted = edit.end_byte.saturating_sub(edit.start_byte);
        let aggregate = size_of::<EditDescriptor>() as u64 + replacement_bytes_len + deleted;
        if edit.start_byte > edit.end_byte
            || edit.replacement_offset != 0
            || edit.replacement_len != replacement_bytes_len
        {
            return Err(StatusCode::InvalidArgument);
        }
        if aggregate > u64::from(MAX_SMALL_EDIT_BYTES) {
            return Err(StatusCode::EditTooLarge);
        }
        let start = usize::try_from(edit.start_byte).map_err(|_| StatusCode::RangeOutOfBounds)?;
        let end = usize::try_from(edit.end_byte).map_err(|_| StatusCode::RangeOutOfBounds)?;
        let replay_end = edit
            .start_byte
            .checked_add(replacement_bytes_len)
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let (receipt, inverse, applies_state, target_state) = {
            let entry = session_entry(&mut registry, request.session)?;
            acknowledge_edit_terminal_for_ordered_mutation(entry, request.expected_revision);
            let StoredSessionState::Open(document) = &mut entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            let target_state = entry.history_state;
            let applies_state = entry.next_history_state;
            let next_history_state = applies_state
                .checked_add(1)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            let inverse = document
                .source_bytes(start..end)
                .map_err(|error| map_actor_error(&error))?;
            let receipt = document
                .apply_edit(
                    request.expected_revision,
                    start..end,
                    replacement.to_owned(),
                )
                .map_err(|error| map_actor_error(&error))?;
            entry.history_state = applies_state;
            entry.next_history_state = next_history_state;
            entry.progress_token = 0;
            entry.continuations.clear();
            (receipt, inverse, applies_state, target_state)
        };
        registry
            .continuations
            .retain(|_, continuation| continuation.session != request.session.session);
        transform_session_anchors(
            &mut registry,
            request.session.session,
            edit.start_byte,
            deleted,
            replacement_bytes_len,
        );
        let (history_token, history) = retain_history(
            &mut registry,
            request.session,
            applies_state,
            target_state,
            edit.start_byte,
            replay_end,
            inverse,
            0,
        )
        .unwrap_or((HistoryToken::NONE, HistoryDisposition::OverBudget));
        Ok(RuntimeOutcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::RevisionCommitted {
                revision: Revision(receipt.revision),
                history_token,
                history,
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_source_transaction_v1(
    request: *const SourceTransactionRequestV1,
    replacement_bytes: *const u8,
    replacement_bytes_len: u64,
    output: *mut u8,
    output_capacity: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::SourceTransactionV1, outcome, || {
        let request =
            unsafe { read_record(request, size_of::<SourceTransactionRequestV1>() as u32)? };
        let required_output = source_transaction_output_requirement();
        if output.is_null() || output_capacity < required_output {
            return Ok(RuntimeOutcome {
                operation: OperationCode::SourceTransactionV1,
                status: StatusCode::BufferTooSmall,
                progress: ProgressState::None,
                required_payload_bytes: required_output,
                written_payload_bytes: 0,
                result: OperationResult::None,
            });
        }
        if !valid_budget(request.budget, false)
            || u64::from(request.budget.max_result_bytes) < required_output
            || request.flags != 0
            || request.expected_revision == 0
            || request.logical_edit_id == 0
            || request.request_digest == 0
            || request.base_utf16_range.start_byte > request.base_utf16_range.end_byte
            || request.replacement_bytes_len != replacement_bytes_len
            || !matches!(
                request.selection_affinity,
                value if value == Affinity::Upstream as u32
                    || value == Affinity::Downstream as u32
            )
            || request.selection_direction > 1
        {
            return Err(StatusCode::InvalidArgument);
        }
        // V1 accepts one bounded ingress chunk. Larger replacements use the
        // staged transaction lane until a receipt-bearing staged commit is
        // added. Deleted source is bounded to one inverse chunk below and its
        // exact capacity is reserved before source mutation.
        if replacement_bytes_len > u64::from(MAX_BULK_CHUNK_BYTES) {
            return Err(StatusCode::EditTooLarge);
        }
        let replacement = unsafe { borrowed_bytes(replacement_bytes, replacement_bytes_len)? };
        let replacement = str::from_utf8(replacement).map_err(|_| StatusCode::InvalidUtf8)?;

        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        {
            let entry = session_entry(&mut registry, request.session)?;
            if let Some(terminal) = entry.terminal_source_transaction.as_ref() {
                if terminal.receipt.logical_edit_id == request.logical_edit_id {
                    if terminal.receipt.request_digest != request.request_digest
                        || terminal.receipt.flags & SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES != 0
                    {
                        return Err(StatusCode::TransactionConflict);
                    }
                    unsafe { write_source_transaction_terminal(terminal, output) };
                    return Ok(source_transaction_terminal_outcome(terminal));
                }
            }
            if entry
                .terminal_edit_intent
                .as_ref()
                .is_some_and(|terminal| terminal.receipt.logical_edit_id == request.logical_edit_id)
            {
                return Err(StatusCode::TransactionConflict);
            }
            let pending_logical_edit_id = entry
                .terminal_source_transaction
                .as_ref()
                .map(|terminal| terminal.receipt.logical_edit_id)
                .or_else(|| {
                    entry
                        .terminal_edit_intent
                        .as_ref()
                        .map(|terminal| terminal.receipt.logical_edit_id)
                });
            match pending_logical_edit_id {
                Some(id) if request.acknowledge_previous_logical_edit_id != id => {
                    return Err(StatusCode::Backpressure)
                }
                None if request.acknowledge_previous_logical_edit_id != 0 => {
                    return Err(StatusCode::InvalidArgument)
                }
                _ => {}
            }
        }

        let same_anchor = request.selection_base_anchor == request.selection_extent_anchor;
        if same_anchor
            && request.result_selection_base_utf16 != request.result_selection_extent_utf16
        {
            return Err(StatusCode::InvalidArgument);
        }
        anchor_for_request(&registry, request.session, request.selection_base_anchor)?;
        anchor_for_request(&registry, request.session, request.selection_extent_anchor)?;

        let (
            deleted_bytes,
            applies_state,
            target_state,
            start_utf16,
            end_utf16,
            result_base,
            result_extent,
        ) = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            let inspection = document
                .inspect()
                .map_err(|error| map_actor_error(&error))?;
            if inspection.revision != request.expected_revision {
                return Err(StatusCode::StaleRevision);
            }
            let start_utf16 = usize::try_from(request.base_utf16_range.start_byte)
                .map_err(|_| StatusCode::RangeOutOfBounds)?;
            let end_utf16 = usize::try_from(request.base_utf16_range.end_byte)
                .map_err(|_| StatusCode::RangeOutOfBounds)?;
            let result_base = usize::try_from(request.result_selection_base_utf16)
                .map_err(|_| StatusCode::RangeOutOfBounds)?;
            let result_extent = usize::try_from(request.result_selection_extent_utf16)
                .map_err(|_| StatusCode::RangeOutOfBounds)?;
            let start_byte = document
                .byte_offset_for_utf16(start_utf16)
                .map_err(|error| map_actor_error(&error))?;
            let end_byte = document
                .byte_offset_for_utf16(end_utf16)
                .map_err(|error| map_actor_error(&error))?;
            let deleted_bytes = end_byte
                .checked_sub(start_byte)
                .ok_or(StatusCode::RangeOutOfBounds)?;
            if deleted_bytes > MAX_BULK_CHUNK_BYTES as usize {
                return Err(StatusCode::EditTooLarge);
            }
            let result_bytes = inspection
                .source_byte_len
                .checked_sub(deleted_bytes)
                .and_then(|bytes| bytes.checked_add(replacement.len()))
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            if entry.max_document_bytes != 0 && result_bytes as u64 > entry.max_document_bytes {
                return Err(StatusCode::ResourceLimitExceeded);
            }
            let applies_state = entry.next_history_state;
            applies_state
                .checked_add(1)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            (
                deleted_bytes,
                applies_state,
                entry.history_state,
                start_utf16,
                end_utf16,
                result_base,
                result_extent,
            )
        };
        let reserved_history = reserve_source_transaction_history(
            &mut registry,
            request.session,
            applies_state,
            target_state,
            deleted_bytes,
            request.history_group_id,
        )?;

        let actor_result = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &mut entry.state else {
                rollback_source_history_reservation(&mut registry, reserved_history);
                return Err(StatusCode::SessionBusy);
            };
            document.apply_source_transaction_v1(
                request.expected_revision,
                start_utf16..end_utf16,
                replacement.to_owned(),
                result_base,
                result_extent,
            )
        };
        let receipt = match actor_result {
            Ok(receipt) => receipt,
            Err(error) => {
                rollback_source_history_reservation(&mut registry, reserved_history);
                return Err(map_actor_error(&error));
            }
        };

        let splice = receipt.committed_splice;
        let start_byte = splice.base_byte_range.start as u64;
        let deleted_bytes = splice.base_byte_range.len() as u64;
        let inserted_bytes = splice.replacement.len() as u64;
        transform_session_anchors(
            &mut registry,
            request.session.session,
            start_byte,
            deleted_bytes,
            inserted_bytes,
        );
        registry
            .anchors
            .get_mut(&request.selection_base_anchor)
            .expect("validated source-transaction base anchor must remain live")
            .byte_offset = receipt.result_selection_base_byte as u64;
        registry
            .anchors
            .get_mut(&request.selection_base_anchor)
            .expect("validated source-transaction base anchor must remain live")
            .affinity =
            if receipt.result_selection_base_byte <= receipt.result_selection_extent_byte {
                Affinity::Downstream
            } else {
                Affinity::Upstream
            };
        if !same_anchor {
            let extent = registry
                .anchors
                .get_mut(&request.selection_extent_anchor)
                .expect("validated source-transaction extent anchor must remain live");
            extent.byte_offset = receipt.result_selection_extent_byte as u64;
            extent.affinity =
                if receipt.result_selection_base_byte < receipt.result_selection_extent_byte {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                };
        }

        let result_end_byte = splice.result_byte_range.end as u64;
        finalize_source_transaction_history(
            &mut registry,
            reserved_history,
            start_byte,
            result_end_byte,
            receipt.inverse,
        );
        {
            let entry = registry
                .sessions
                .get_mut(&request.session.session)
                .expect("committed source-transaction session must remain live");
            entry.history_state = applies_state;
            entry.next_history_state = applies_state + 1;
            entry.progress_token = 0;
            entry.continuations.clear();
        }
        registry
            .continuations
            .retain(|_, continuation| continuation.session != request.session.session);

        let terminal = StoredSourceTransactionTerminal {
            receipt: SourceTransactionReceiptV1 {
                struct_size: size_of::<SourceTransactionReceiptV1>() as u32,
                history_disposition: HistoryDisposition::Retained as u32,
                flags: SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT
                    | SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES
                    | if reserved_history.extended() {
                        SOURCE_TRANSACTION_RECEIPT_COMPOSITE_HISTORY_EXTENDED
                    } else {
                        0
                    }
                    | if receipt.parser_pending {
                        SOURCE_TRANSACTION_RECEIPT_PARSER_PENDING
                    } else {
                        0
                    },
                reserved_u32: 0,
                logical_edit_id: request.logical_edit_id,
                request_digest: request.request_digest,
                base_revision: receipt.base_revision,
                result_revision: receipt.result_revision,
                base_byte_range: SourceRange {
                    start_byte,
                    end_byte: splice.base_byte_range.end as u64,
                },
                base_utf16_range: SourceRange {
                    start_byte: splice.base_utf16_range.start as u64,
                    end_byte: splice.base_utf16_range.end as u64,
                },
                result_byte_range: SourceRange {
                    start_byte: splice.result_byte_range.start as u64,
                    end_byte: splice.result_byte_range.end as u64,
                },
                result_utf16_range: SourceRange {
                    start_byte: splice.result_utf16_range.start as u64,
                    end_byte: splice.result_utf16_range.end as u64,
                },
                result_selection_base_utf16: receipt.result_selection_base_utf16 as u64,
                result_selection_extent_utf16: receipt.result_selection_extent_utf16 as u64,
                result_selection_affinity: request.selection_affinity,
                result_selection_direction: request.selection_direction,
                result_source_byte_length: receipt.result_source_byte_length as u64,
                result_source_utf16_length: receipt.result_source_utf16_length as u64,
                affected_result_utf16_range: SourceRange {
                    start_byte: splice.result_utf16_range.start as u64,
                    end_byte: splice.result_utf16_range.end as u64,
                },
                history_token: reserved_history.token(),
                replacement_bytes: replacement_bytes_len,
                reserved: [0; 2],
            },
        };
        let outcome = source_transaction_terminal_outcome(&terminal);
        let entry = registry
            .sessions
            .get_mut(&request.session.session)
            .expect("committed source-transaction session must remain live");
        entry.terminal_edit_intent = None;
        entry.terminal_source_transaction = Some(terminal);
        unsafe {
            write_source_transaction_terminal(
                entry
                    .terminal_source_transaction
                    .as_ref()
                    .expect("stored source terminal must remain live"),
                output,
            )
        };
        Ok(outcome)
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_edit_intent_v1(
    request: *const EditIntentRequestV1,
    output: *mut u8,
    output_capacity: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::EditIntentV1, outcome, || {
        let request = unsafe { read_record(request, size_of::<EditIntentRequestV1>() as u32)? };
        let maximum_output = edit_intent_output_requirement();
        if output.is_null() || output_capacity < maximum_output {
            return Ok(RuntimeOutcome {
                operation: OperationCode::EditIntentV1,
                status: StatusCode::BufferTooSmall,
                progress: ProgressState::None,
                required_payload_bytes: maximum_output,
                written_payload_bytes: 0,
                result: OperationResult::None,
            });
        }
        if !valid_budget(request.budget, false)
            || u64::from(request.budget.max_result_bytes) < maximum_output
            || request.profile_id != EDIT_PROFILE_FLARK_V1
            || request.expected_revision == 0
            || request.logical_edit_id == 0
            || request.request_digest == 0
            || request.composition_active > 1
        {
            return Err(StatusCode::InvalidArgument);
        }
        let intent = match request.intent {
            EDIT_INTENT_INSERT_PARAGRAPH_BREAK => DocumentEditIntentV1::InsertParagraphBreak,
            EDIT_INTENT_DELETE_BACKWARD => DocumentEditIntentV1::DeleteBackward,
            EDIT_INTENT_DELETE_FORWARD => DocumentEditIntentV1::DeleteForward,
            EDIT_INTENT_TOGGLE_TASK_CHECKED => DocumentEditIntentV1::ToggleTaskChecked,
            EDIT_INTENT_INDENT_LIST_ITEM => DocumentEditIntentV1::IndentListItem,
            EDIT_INTENT_OUTDENT_LIST_ITEM => DocumentEditIntentV1::OutdentListItem,
            _ => return Err(StatusCode::InvalidArgument),
        };
        let is_target_action = intent == DocumentEditIntentV1::ToggleTaskChecked;
        if if is_target_action {
            request.target_anchor == 0
                || !matches!(
                    request.selection_affinity,
                    value if value == Affinity::Upstream as u32
                        || value == Affinity::Downstream as u32
                )
                || request.selection_direction > 1
        } else {
            request.target_anchor != 0
                || request.selection_affinity != Affinity::Downstream as u32
                || request.selection_direction != 0
        } {
            return Err(StatusCode::InvalidArgument);
        }

        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        {
            let entry = session_entry(&mut registry, request.session)?;
            if let Some(terminal) = entry.terminal_edit_intent.as_ref() {
                if terminal.receipt.logical_edit_id == request.logical_edit_id {
                    if terminal.receipt.request_digest != request.request_digest {
                        return Err(StatusCode::TransactionConflict);
                    }
                    unsafe { write_edit_intent_terminal(terminal, output) };
                    return Ok(edit_intent_terminal_outcome(terminal));
                }
            }
            if entry
                .terminal_source_transaction
                .as_ref()
                .is_some_and(|terminal| terminal.receipt.logical_edit_id == request.logical_edit_id)
            {
                return Err(StatusCode::TransactionConflict);
            }
            let pending_logical_edit_id = entry
                .terminal_edit_intent
                .as_ref()
                .map(|terminal| terminal.receipt.logical_edit_id)
                .or_else(|| {
                    entry
                        .terminal_source_transaction
                        .as_ref()
                        .map(|terminal| terminal.receipt.logical_edit_id)
                });
            match pending_logical_edit_id {
                Some(id) if request.acknowledge_previous_logical_edit_id != id => {
                    return Err(StatusCode::Backpressure)
                }
                None if request.acknowledge_previous_logical_edit_id != 0 => {
                    return Err(StatusCode::InvalidArgument)
                }
                _ => {}
            }
            // Keep the acknowledged terminal until this request itself
            // reaches a terminal. A bounded pre-commit rejection must be
            // retryable with the same request and acknowledgement.
        }

        let (selection_byte, selection_extent_byte, target_byte) = {
            let base =
                anchor_for_request(&registry, request.session, request.selection_base_anchor)?;
            let extent =
                anchor_for_request(&registry, request.session, request.selection_extent_anchor)?;
            if !is_target_action
                && (base.byte_offset != extent.byte_offset
                    || base.affinity != Affinity::Downstream
                    || extent.affinity != Affinity::Downstream)
            {
                return Err(StatusCode::InvalidArgument);
            }
            let selection_byte =
                usize::try_from(base.byte_offset).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let selection_extent_byte =
                usize::try_from(extent.byte_offset).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let target_byte = if is_target_action {
                usize::try_from(
                    anchor_for_request(&registry, request.session, request.target_anchor)?
                        .byte_offset,
                )
                .map_err(|_| StatusCode::RangeOutOfBounds)?
            } else {
                selection_byte
            };
            (selection_byte, selection_extent_byte, target_byte)
        };

        let (reserved_history, applies_state) = if request.composition_active == 0 {
            let (applies_state, target_state) = {
                let entry = session_entry(&mut registry, request.session)?;
                let applies_state = entry.next_history_state;
                applies_state
                    .checked_add(1)
                    .ok_or(StatusCode::ResourceLimitExceeded)?;
                (applies_state, entry.history_state)
            };
            let token = reserve_edit_intent_history(
                &mut registry,
                request.session,
                applies_state,
                target_state,
                MAX_SMALL_EDIT_BYTES as usize,
                0,
            )?;
            (Some(token), applies_state)
        } else {
            (None, 0)
        };

        let actor_result = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &mut entry.state else {
                if let Some(token) = reserved_history {
                    detach_history(&mut registry, token);
                }
                return Err(StatusCode::SessionBusy);
            };
            document.try_apply_edit_intent_v1_at_bytes(
                request.expected_revision,
                intent,
                selection_byte,
                target_byte,
                request.composition_active != 0,
            )
        };
        let receipt = match actor_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(token) = reserved_history {
                    detach_history(&mut registry, token);
                }
                return Err(map_actor_error(&error));
            }
        };

        let semantic_disposition = match receipt.disposition {
            DocumentEditIntentDispositionV1::Applied => EDIT_INTENT_DISPOSITION_APPLIED,
            DocumentEditIntentDispositionV1::HandledNoChange => {
                EDIT_INTENT_DISPOSITION_HANDLED_NO_CHANGE
            }
            DocumentEditIntentDispositionV1::NotApplicable => {
                EDIT_INTENT_DISPOSITION_NOT_APPLICABLE
            }
            DocumentEditIntentDispositionV1::NeedsCurrentSemantics => {
                EDIT_INTENT_DISPOSITION_NEEDS_CURRENT_SEMANTICS
            }
        };
        let presentation_transition = match receipt.presentation_transition {
            DocumentEditPresentationTransitionV1::None => EDIT_PRESENTATION_NONE,
            DocumentEditPresentationTransitionV1::SplitParagraph => {
                EDIT_PRESENTATION_SPLIT_PARAGRAPH
            }
            DocumentEditPresentationTransitionV1::ContinueList => EDIT_PRESENTATION_CONTINUE_LIST,
            DocumentEditPresentationTransitionV1::ExitList => EDIT_PRESENTATION_EXIT_LIST,
            DocumentEditPresentationTransitionV1::MergeParagraph => {
                EDIT_PRESENTATION_MERGE_PARAGRAPH
            }
            DocumentEditPresentationTransitionV1::LiftList => EDIT_PRESENTATION_LIFT_LIST,
            DocumentEditPresentationTransitionV1::ContinueBlockQuote => {
                EDIT_PRESENTATION_CONTINUE_BLOCK_QUOTE
            }
            DocumentEditPresentationTransitionV1::ExitBlockQuote => {
                EDIT_PRESENTATION_EXIT_BLOCK_QUOTE
            }
            DocumentEditPresentationTransitionV1::LiftBlockQuote => {
                EDIT_PRESENTATION_LIFT_BLOCK_QUOTE
            }
            DocumentEditPresentationTransitionV1::ExitHeading => EDIT_PRESENTATION_EXIT_HEADING,
            DocumentEditPresentationTransitionV1::LiftHeading => EDIT_PRESENTATION_LIFT_HEADING,
            DocumentEditPresentationTransitionV1::OutdentList => EDIT_PRESENTATION_OUTDENT_LIST,
            DocumentEditPresentationTransitionV1::ContinueIndentedCode => {
                EDIT_PRESENTATION_CONTINUE_INDENTED_CODE
            }
            DocumentEditPresentationTransitionV1::JoinIndentedCode => {
                EDIT_PRESENTATION_JOIN_INDENTED_CODE
            }
            DocumentEditPresentationTransitionV1::LiftIndentedCode => {
                EDIT_PRESENTATION_LIFT_INDENTED_CODE
            }
            DocumentEditPresentationTransitionV1::DeleteThematicBreak => {
                EDIT_PRESENTATION_DELETE_THEMATIC_BREAK
            }
            DocumentEditPresentationTransitionV1::OutdentBlockQuote => {
                EDIT_PRESENTATION_OUTDENT_BLOCK_QUOTE
            }
            DocumentEditPresentationTransitionV1::ToggleTaskChecked => {
                EDIT_PRESENTATION_TOGGLE_TASK_CHECKED
            }
            DocumentEditPresentationTransitionV1::IndentList => EDIT_PRESENTATION_INDENT_LIST,
            DocumentEditPresentationTransitionV1::RetainParagraphGap => {
                EDIT_PRESENTATION_RETAIN_PARAGRAPH_GAP
            }
        };

        if receipt.disposition != DocumentEditIntentDispositionV1::Applied {
            debug_assert_eq!(presentation_transition, EDIT_PRESENTATION_NONE);
            if let Some(token) = reserved_history {
                detach_history(&mut registry, token);
            }
            let terminal = StoredEditIntentTerminal {
                receipt: EditIntentReceiptV1 {
                    struct_size: size_of::<EditIntentReceiptV1>() as u32,
                    semantic_disposition,
                    history_disposition: HistoryDisposition::NotApplicable as u32,
                    flags: if receipt.parser_pending {
                        EDIT_INTENT_RECEIPT_PARSER_PENDING
                    } else {
                        0
                    },
                    logical_edit_id: request.logical_edit_id,
                    request_digest: request.request_digest,
                    base_revision: receipt.base_revision,
                    result_revision: receipt.result_revision,
                    result_selection_utf16: receipt.result_selection_utf16 as u64,
                    result_selection_affinity: request.selection_affinity,
                    result_selection_direction: request.selection_direction,
                    result_source_byte_length: receipt.result_source_byte_length as u64,
                    result_source_utf16_length: receipt.result_source_utf16_length as u64,
                    ..EditIntentReceiptV1::default()
                },
                replacement: Vec::new(),
            };
            let outcome = edit_intent_terminal_outcome(&terminal);
            let entry = session_entry(&mut registry, request.session)?;
            entry.terminal_source_transaction = None;
            entry.terminal_edit_intent = Some(terminal);
            unsafe {
                write_edit_intent_terminal(
                    entry
                        .terminal_edit_intent
                        .as_ref()
                        .expect("stored semantic terminal must remain live"),
                    output,
                )
            };
            return Ok(outcome);
        }

        let history_token = reserved_history
            .expect("applied semantic edit must have a preallocated history reservation");
        let splice = receipt
            .committed_splice
            .expect("applied semantic edit must return its committed splice");
        let start_byte = splice.base_byte_range.start as u64;
        let deleted_bytes = splice.base_byte_range.len() as u64;
        let replacement = splice.replacement.into_bytes();
        let inserted_bytes = replacement.len() as u64;
        let result_end_byte = start_byte + inserted_bytes;

        transform_session_anchors(
            &mut registry,
            request.session.session,
            start_byte,
            deleted_bytes,
            inserted_bytes,
        );
        debug_assert_eq!(
            anchor_for_request(&registry, request.session, request.selection_base_anchor)
                .map(|anchor| anchor.byte_offset),
            Ok(receipt.result_selection_byte as u64)
        );
        debug_assert_eq!(
            anchor_for_request(&registry, request.session, request.selection_extent_anchor)
                .map(|anchor| anchor.byte_offset),
            Ok(if is_target_action {
                selection_extent_byte as u64
            } else {
                receipt.result_selection_byte as u64
            })
        );
        finalize_edit_intent_history(
            &mut registry,
            history_token,
            start_byte,
            result_end_byte,
            &receipt.inverse,
        );
        {
            let entry = registry
                .sessions
                .get_mut(&request.session.session)
                .expect("committed semantic session must remain live");
            entry.history_state = applies_state;
            entry.next_history_state = applies_state + 1;
            entry.progress_token = 0;
            entry.continuations.clear();
        }
        registry
            .continuations
            .retain(|_, continuation| continuation.session != request.session.session);

        let terminal = StoredEditIntentTerminal {
            receipt: EditIntentReceiptV1 {
                struct_size: size_of::<EditIntentReceiptV1>() as u32,
                semantic_disposition,
                history_disposition: HistoryDisposition::Retained as u32,
                flags: EDIT_INTENT_RECEIPT_HAS_COMMIT
                    | EDIT_INTENT_RECEIPT_SEMANTIC_BYTES
                    | if receipt.parser_pending {
                        EDIT_INTENT_RECEIPT_PARSER_PENDING
                    } else {
                        0
                    },
                logical_edit_id: request.logical_edit_id,
                request_digest: request.request_digest,
                base_revision: receipt.base_revision,
                result_revision: receipt.result_revision,
                base_byte_range: SourceRange {
                    start_byte,
                    end_byte: start_byte + deleted_bytes,
                },
                base_utf16_range: SourceRange {
                    start_byte: splice.base_utf16_range.start as u64,
                    end_byte: splice.base_utf16_range.end as u64,
                },
                result_byte_range: SourceRange {
                    start_byte,
                    end_byte: result_end_byte,
                },
                result_utf16_range: SourceRange {
                    start_byte: splice.result_utf16_range.start as u64,
                    end_byte: splice.result_utf16_range.end as u64,
                },
                result_selection_utf16: receipt.result_selection_utf16 as u64,
                result_selection_affinity: request.selection_affinity,
                result_selection_direction: request.selection_direction,
                result_source_byte_length: receipt.result_source_byte_length as u64,
                result_source_utf16_length: receipt.result_source_utf16_length as u64,
                affected_result_utf16_range: SourceRange {
                    start_byte: splice.result_utf16_range.start as u64,
                    end_byte: splice.result_utf16_range.end as u64,
                },
                history_token,
                replacement_bytes: replacement.len() as u32,
                presentation_transition,
                reserved: [0; 2],
            },
            replacement,
        };
        let outcome = edit_intent_terminal_outcome(&terminal);
        let entry = registry
            .sessions
            .get_mut(&request.session.session)
            .expect("committed semantic session must remain live");
        entry.terminal_source_transaction = None;
        entry.terminal_edit_intent = Some(terminal);
        unsafe {
            write_edit_intent_terminal(
                entry
                    .terminal_edit_intent
                    .as_ref()
                    .expect("stored semantic terminal must remain live"),
                output,
            )
        };
        Ok(outcome)
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_bulk_begin(
    request: *const BulkBeginRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::BulkBegin, outcome, || {
        let request = unsafe { read_record(request, size_of::<BulkBeginRequest>() as u32)? };
        if request.flags != 0
            || request.expected_revision == 0
            || request.range.start_byte > request.range.end_byte
            || request.reserved != [0; 2]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let expected_bytes = usize::try_from(request.expected_total_bytes)
            .map_err(|_| StatusCode::ResourceLimitExceeded)?;
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let (history_budget_bytes, deleted_bytes) = {
            let entry = session_entry(&mut registry, request.session)?;
            acknowledge_edit_terminal_for_ordered_mutation(entry, request.expected_revision);
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            let inspection = document
                .inspect()
                .map_err(|error| map_actor_error(&error))?;
            if inspection.revision != request.expected_revision {
                return Err(StatusCode::StaleRevision);
            }
            if request.range.end_byte > inspection.source_byte_len as u64 {
                return Err(StatusCode::RangeOutOfBounds);
            }
            let next_source_bytes = (inspection.source_byte_len as u64)
                .checked_sub(request.range.end_byte - request.range.start_byte)
                .and_then(|bytes| bytes.checked_add(request.expected_total_bytes))
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            if entry.max_document_bytes != 0 && next_source_bytes > entry.max_document_bytes {
                return Err(StatusCode::ResourceLimitExceeded);
            }
            // Bounds were checked above, so a failed conversion identifies a
            // non-scalar UTF-8 cut rather than an out-of-document position.
            document
                .utf16_offset_for_byte(request.range.start_byte as usize)
                .map_err(|_| StatusCode::RangeNotScalarBoundary)?;
            document
                .utf16_offset_for_byte(request.range.end_byte as usize)
                .map_err(|_| StatusCode::RangeNotScalarBoundary)?;
            (
                entry.history_budget_bytes,
                request.range.end_byte - request.range.start_byte,
            )
        };

        let mut replacement = Vec::new();
        replacement
            .try_reserve_exact(expected_bytes)
            .map_err(|_| StatusCode::AllocationFailure)?;
        let retained_bytes = HISTORY_TOKEN_OVERHEAD_BYTES
            .checked_add(deleted_bytes)
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        let history = if history_budget_bytes == 0 {
            BulkHistoryCapture::Disabled
        } else if retained_bytes > history_budget_bytes {
            BulkHistoryCapture::OverBudget
        } else {
            let deleted_bytes =
                usize::try_from(deleted_bytes).map_err(|_| StatusCode::ResourceLimitExceeded)?;
            let mut inverse = Vec::new();
            if inverse.try_reserve_exact(deleted_bytes).is_err() {
                // Undo retention is best-effort and must never reject source.
                BulkHistoryCapture::OverBudget
            } else {
                BulkHistoryCapture::Capturing(inverse)
            }
        };
        let transaction = registry.allocate_handle()?;
        registry.transactions.insert(
            transaction,
            StoredBulkTransaction {
                session: request.session.session,
                owner: request.session.owner_token,
                expected_revision: request.expected_revision,
                start_byte: request.range.start_byte,
                end_byte: request.range.end_byte,
                expected_bytes,
                replacement,
                validated_bytes: 0,
                validated_utf16: 0,
                inverse_next_byte: request.range.start_byte,
                history,
                progress_token: 0,
                source_request: None,
            },
        );
        session_entry(&mut registry, request.session)?
            .transactions
            .insert(transaction);
        Ok(RuntimeOutcome {
            operation: OperationCode::BulkBegin,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::TransactionStaged {
                transaction: TransactionHandle(transaction),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_bulk_append(
    request: *const StageRequest,
    input: *const u8,
    input_len: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::BulkAppend, outcome, || {
        let request = unsafe { read_record(request, size_of::<StageRequest>() as u32)? };
        let input = unsafe { borrowed_bytes(input, input_len)? };
        if request.flags != 0
            || request.transaction == 0
            || request.chunk_len != input_len
            || input.is_empty()
            || input.len() > MAX_BULK_CHUNK_BYTES as usize
            || request.reserved != [0; 2]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        session_entry(&mut registry, request.session)?;
        let transaction = registry
            .transactions
            .get_mut(&request.transaction)
            .ok_or(StatusCode::InvalidHandle)?;
        if transaction.session != request.session.session
            || transaction.owner != request.session.owner_token
            || request.chunk_offset != transaction.replacement.len() as u64
            || transaction.replacement.len().saturating_add(input.len())
                > transaction.expected_bytes
        {
            return Err(StatusCode::TransactionConflict);
        }
        transaction.replacement.extend_from_slice(input);
        Ok(RuntimeOutcome {
            operation: OperationCode::BulkAppend,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::TransactionStaged {
                transaction: TransactionHandle(request.transaction),
            },
        })
    })
}

fn next_utf8_validation_end(bytes: &[u8], cursor: usize) -> usize {
    let tentative = cursor
        .saturating_add(MAX_BULK_CHUNK_BYTES as usize)
        .min(bytes.len());
    if tentative == bytes.len() {
        return tentative;
    }
    let mut end = tentative;
    while end > cursor && bytes[end] & 0xc0 == 0x80 {
        end -= 1;
    }
    if end == cursor {
        tentative
    } else {
        end
    }
}

/// Advances bounded UTF-8/UTF-16 validation and inverse capture shared by the
/// legacy and receipt-bearing staged commit entrypoints.
fn advance_bulk_commit_work(
    registry: &mut Registry,
    session: SessionRef,
    transaction: &mut StoredBulkTransaction,
    mut remaining: usize,
) -> Result<usize, StatusCode> {
    while remaining != 0 && transaction.validated_bytes < transaction.replacement.len() {
        let end = next_utf8_validation_end(&transaction.replacement, transaction.validated_bytes);
        let chunk = str::from_utf8(&transaction.replacement[transaction.validated_bytes..end])
            .map_err(|_| StatusCode::InvalidUtf8)?;
        transaction.validated_utf16 = transaction
            .validated_utf16
            .checked_add(chunk.encode_utf16().count())
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        transaction.validated_bytes = end;
        remaining -= 1;
    }
    while remaining != 0 && transaction.inverse_next_byte < transaction.end_byte {
        let BulkHistoryCapture::Capturing(inverse) = &mut transaction.history else {
            transaction.inverse_next_byte = transaction.end_byte;
            break;
        };
        let chunk_end = transaction
            .inverse_next_byte
            .saturating_add(u64::from(MAX_BULK_CHUNK_BYTES))
            .min(transaction.end_byte);
        let bytes = {
            let entry = session_entry(registry, session)?;
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            document
                .source_bytes(transaction.inverse_next_byte as usize..chunk_end as usize)
                .map_err(|error| map_actor_error(&error))?
        };
        inverse.extend_from_slice(&bytes);
        transaction.inverse_next_byte = chunk_end;
        remaining -= 1;
    }
    Ok(remaining)
}

fn pending_bulk_commit(
    registry: &mut Registry,
    handle: u64,
    mut transaction: StoredBulkTransaction,
    operation: OperationCode,
) -> Result<RuntimeOutcome, StatusCode> {
    let token = match registry.allocate_handle() {
        Ok(token) => token,
        Err(error) => {
            registry.transactions.insert(handle, transaction);
            return Err(error);
        }
    };
    transaction.progress_token = token;
    let revision = transaction.expected_revision;
    registry.transactions.insert(handle, transaction);
    Ok(RuntimeOutcome {
        operation,
        status: StatusCode::BudgetExhausted,
        progress: ProgressState::BudgetExhausted,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::Progress {
            revision: Revision(revision),
            token: ProgressToken(token),
        },
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_bulk_commit(
    request: *const TransactionRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::BulkCommit, outcome, || {
        let request = unsafe { read_record(request, size_of::<TransactionRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.transaction == 0
            || request.expected_revision == 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let inspection = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            document
                .inspect()
                .map_err(|error| map_actor_error(&error))?
        };
        let transaction = registry
            .transactions
            .get(&request.transaction)
            .ok_or(StatusCode::InvalidHandle)?;
        if transaction.session != request.session.session
            || transaction.owner != request.session.owner_token
            || transaction.expected_revision != request.expected_revision
            || transaction.source_request.is_some()
        {
            return Err(StatusCode::TransactionConflict);
        }
        if inspection.revision != request.expected_revision {
            return Err(StatusCode::StaleRevision);
        }
        if transaction.replacement.len() != transaction.expected_bytes {
            return Err(StatusCode::TransactionIncomplete);
        }
        if (transaction.progress_token == 0 && request.progress_token != 0)
            || (transaction.progress_token != 0
                && request.progress_token != transaction.progress_token)
        {
            return Err(StatusCode::StaleProgressToken);
        }

        let mut transaction = registry
            .transactions
            .remove(&request.transaction)
            .ok_or(StatusCode::InternalFault)?;
        let remaining = match advance_bulk_commit_work(
            &mut registry,
            request.session,
            &mut transaction,
            usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX),
        ) {
            Ok(remaining) => remaining,
            Err(error) => {
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(error);
            }
        };
        if transaction.validated_bytes != transaction.replacement.len()
            || transaction.inverse_next_byte != transaction.end_byte
            || remaining == 0
        {
            return pending_bulk_commit(
                &mut registry,
                request.transaction,
                transaction,
                OperationCode::BulkCommit,
            );
        }

        let replacement_len = transaction.replacement.len() as u64;
        let replacement = unsafe {
            // Every byte was validated in bounded chunks above.
            String::from_utf8_unchecked(std::mem::take(&mut transaction.replacement))
        };
        let history = std::mem::replace(&mut transaction.history, BulkHistoryCapture::OverBudget);
        let replay_end = transaction
            .start_byte
            .checked_add(replacement_len)
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        let (receipt, applies_state, target_state) = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &mut entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            let target_state = entry.history_state;
            let applies_state = entry.next_history_state;
            let next_history_state = applies_state
                .checked_add(1)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            let receipt = document
                .apply_edit(
                    request.expected_revision,
                    transaction.start_byte as usize..transaction.end_byte as usize,
                    replacement,
                )
                .map_err(|error| map_actor_error(&error))?;
            entry.history_state = applies_state;
            entry.next_history_state = next_history_state;
            entry.progress_token = 0;
            entry.continuations.clear();
            entry.transactions.remove(&request.transaction);
            (receipt, applies_state, target_state)
        };
        registry
            .continuations
            .retain(|_, continuation| continuation.session != request.session.session);
        transform_session_anchors(
            &mut registry,
            request.session.session,
            transaction.start_byte,
            transaction.end_byte - transaction.start_byte,
            replacement_len,
        );
        let (history_token, disposition) = match history {
            BulkHistoryCapture::Capturing(inverse) => retain_history(
                &mut registry,
                request.session,
                applies_state,
                target_state,
                transaction.start_byte,
                replay_end,
                inverse,
                0,
            )
            .unwrap_or((HistoryToken::NONE, HistoryDisposition::OverBudget)),
            BulkHistoryCapture::Disabled => (HistoryToken::NONE, HistoryDisposition::Disabled),
            BulkHistoryCapture::OverBudget => (HistoryToken::NONE, HistoryDisposition::OverBudget),
        };
        Ok(RuntimeOutcome {
            operation: OperationCode::BulkCommit,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::RevisionCommitted {
                revision: Revision(receipt.revision),
                history_token,
                history: disposition,
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_staged_source_transaction_v1(
    request: *const StagedSourceTransactionRequestV1,
    output: *mut u8,
    output_capacity: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::StagedSourceTransactionV1, outcome, || {
        let request = unsafe {
            read_record(
                request,
                size_of::<StagedSourceTransactionRequestV1>() as u32,
            )?
        };
        let required_output = source_transaction_output_requirement();
        if output.is_null() || output_capacity < required_output {
            return Ok(RuntimeOutcome {
                operation: OperationCode::StagedSourceTransactionV1,
                status: StatusCode::BufferTooSmall,
                progress: ProgressState::None,
                required_payload_bytes: required_output,
                written_payload_bytes: 0,
                result: OperationResult::None,
            });
        }
        if !valid_budget(request.budget, false)
            || u64::from(request.budget.max_result_bytes) < required_output
            || request.flags != 0
            || request.expected_revision == 0
            || request.logical_edit_id == 0
            || request.request_digest == 0
            || request.selection_affinity != Affinity::Downstream as u32
            || request.selection_direction != 0
            || request.history_group_id != 0
            || request.reserved != [0; 2]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let source_request = StoredStagedSourceRequest {
            selection_base_anchor: request.selection_base_anchor,
            selection_extent_anchor: request.selection_extent_anchor,
            logical_edit_id: request.logical_edit_id,
            request_digest: request.request_digest,
            acknowledge_previous_logical_edit_id: request.acknowledge_previous_logical_edit_id,
            selection_generation: request.selection_generation,
            result_selection_utf16: request.result_selection_utf16,
            selection_affinity: request.selection_affinity,
            selection_direction: request.selection_direction,
            history_group_id: request.history_group_id,
        };

        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        {
            let entry = session_entry(&mut registry, request.session)?;
            if let Some(terminal) = entry.terminal_source_transaction.as_ref() {
                if terminal.receipt.logical_edit_id == request.logical_edit_id {
                    if terminal.receipt.request_digest != request.request_digest
                        || terminal.receipt.flags & SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES == 0
                    {
                        return Err(StatusCode::TransactionConflict);
                    }
                    unsafe { write_source_transaction_terminal(terminal, output) };
                    return Ok(source_transaction_terminal_outcome_for(
                        terminal,
                        OperationCode::StagedSourceTransactionV1,
                    ));
                }
            }
            if entry
                .terminal_edit_intent
                .as_ref()
                .is_some_and(|terminal| terminal.receipt.logical_edit_id == request.logical_edit_id)
            {
                return Err(StatusCode::TransactionConflict);
            }
            let pending_logical_edit_id = entry
                .terminal_source_transaction
                .as_ref()
                .map(|terminal| terminal.receipt.logical_edit_id)
                .or_else(|| {
                    entry
                        .terminal_edit_intent
                        .as_ref()
                        .map(|terminal| terminal.receipt.logical_edit_id)
                });
            match pending_logical_edit_id {
                Some(id) if request.acknowledge_previous_logical_edit_id != id => {
                    return Err(StatusCode::Backpressure)
                }
                None if request.acknowledge_previous_logical_edit_id != 0 => {
                    return Err(StatusCode::InvalidArgument)
                }
                _ => {}
            }
        }

        // Zero is a terminal-recovery probe only. A first admission still
        // requires one live BULK_BEGIN/BULK_APPEND staging handle.
        if request.transaction == 0 {
            return Err(StatusCode::InvalidHandle);
        }

        anchor_for_request(&registry, request.session, request.selection_base_anchor)?;
        anchor_for_request(&registry, request.session, request.selection_extent_anchor)?;
        let inspection = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            document
                .inspect()
                .map_err(|error| map_actor_error(&error))?
        };
        if inspection.revision != request.expected_revision {
            return Err(StatusCode::StaleRevision);
        }
        {
            let transaction = registry
                .transactions
                .get_mut(&request.transaction)
                .ok_or(StatusCode::InvalidHandle)?;
            if transaction.session != request.session.session
                || transaction.owner != request.session.owner_token
                || transaction.expected_revision != request.expected_revision
            {
                return Err(StatusCode::TransactionConflict);
            }
            if transaction.replacement.len() != transaction.expected_bytes {
                return Err(StatusCode::TransactionIncomplete);
            }
            if (transaction.progress_token == 0 && request.progress_token != 0)
                || (transaction.progress_token != 0
                    && request.progress_token != transaction.progress_token)
            {
                return Err(StatusCode::StaleProgressToken);
            }
            match transaction.source_request {
                Some(existing) if existing != source_request => {
                    return Err(StatusCode::TransactionConflict)
                }
                None => transaction.source_request = Some(source_request),
                _ => {}
            }
        }

        let mut transaction = registry
            .transactions
            .remove(&request.transaction)
            .ok_or(StatusCode::InternalFault)?;
        let remaining = match advance_bulk_commit_work(
            &mut registry,
            request.session,
            &mut transaction,
            usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX),
        ) {
            Ok(remaining) => remaining,
            Err(error) => {
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(error);
            }
        };
        if transaction.validated_bytes != transaction.replacement.len()
            || transaction.inverse_next_byte != transaction.end_byte
            || remaining == 0
        {
            return pending_bulk_commit(
                &mut registry,
                request.transaction,
                transaction,
                OperationCode::StagedSourceTransactionV1,
            );
        }

        let (start_utf16, applies_state, target_state) = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &entry.state else {
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(StatusCode::SessionBusy);
            };
            let start_utf16 = document
                .utf16_offset_for_byte(transaction.start_byte as usize)
                .map_err(|error| map_actor_error(&error))?;
            let expected_result_selection = start_utf16
                .checked_add(transaction.validated_utf16)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            if request.result_selection_utf16 != expected_result_selection as u64 {
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(StatusCode::InvalidArgument);
            }
            let applies_state = entry.next_history_state;
            applies_state
                .checked_add(1)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            (start_utf16, applies_state, entry.history_state)
        };
        let replacement_len = transaction.replacement.len() as u64;
        let replay_end = transaction
            .start_byte
            .checked_add(replacement_len)
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        let inverse =
            match std::mem::replace(&mut transaction.history, BulkHistoryCapture::OverBudget) {
                BulkHistoryCapture::Capturing(inverse) => inverse,
                other => {
                    transaction.history = other;
                    registry
                        .transactions
                        .insert(request.transaction, transaction);
                    return Err(StatusCode::ResourceLimitExceeded);
                }
            };
        let history_token = match reserve_staged_source_history(
            &mut registry,
            request.session,
            applies_state,
            target_state,
            transaction.start_byte,
            replay_end,
            inverse,
        ) {
            Ok(token) => token,
            Err((error, inverse)) => {
                transaction.history = BulkHistoryCapture::Capturing(inverse);
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(error);
            }
        };
        let replacement = unsafe {
            // Every byte was validated in bounded chunks above.
            String::from_utf8_unchecked(std::mem::take(&mut transaction.replacement))
        };
        let actor_result = {
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(document) = &mut entry.state else {
                let inverse = rollback_staged_source_history(&mut registry, history_token);
                transaction.history = BulkHistoryCapture::Capturing(inverse);
                transaction.replacement = replacement.into_bytes();
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(StatusCode::SessionBusy);
            };
            document.apply_staged_source_transaction_v1(
                request.expected_revision,
                transaction.start_byte as usize..transaction.end_byte as usize,
                replacement,
                transaction.validated_utf16,
            )
        };
        let (replacement, receipt) = match actor_result {
            Ok(result) => result,
            Err(error) => {
                rollback_staged_source_history(&mut registry, history_token);
                return Err(map_actor_error(&error));
            }
        };
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(error) => {
                let inverse = rollback_staged_source_history(&mut registry, history_token);
                transaction.history = BulkHistoryCapture::Capturing(inverse);
                transaction.replacement = replacement.into_bytes();
                registry
                    .transactions
                    .insert(request.transaction, transaction);
                return Err(map_document_error(&error));
            }
        };
        drop(replacement);

        let start_byte = receipt.base_byte_range.start as u64;
        let deleted_bytes = receipt.base_byte_range.len() as u64;
        transform_session_anchors(
            &mut registry,
            request.session.session,
            start_byte,
            deleted_bytes,
            replacement_len,
        );
        let same_anchor = request.selection_base_anchor == request.selection_extent_anchor;
        {
            let base = registry
                .anchors
                .get_mut(&request.selection_base_anchor)
                .expect("validated staged base anchor must remain live");
            base.byte_offset = receipt.result_selection_byte as u64;
            base.affinity = Affinity::Downstream;
        }
        if !same_anchor {
            let extent = registry
                .anchors
                .get_mut(&request.selection_extent_anchor)
                .expect("validated staged extent anchor must remain live");
            extent.byte_offset = receipt.result_selection_byte as u64;
            extent.affinity = Affinity::Downstream;
        }
        {
            let entry = registry
                .sessions
                .get_mut(&request.session.session)
                .expect("committed staged source session must remain live");
            entry.history_state = applies_state;
            entry.next_history_state = applies_state + 1;
            entry.progress_token = 0;
            entry.continuations.clear();
            entry.transactions.remove(&request.transaction);
        }
        registry
            .continuations
            .retain(|_, continuation| continuation.session != request.session.session);

        let terminal = StoredSourceTransactionTerminal {
            receipt: SourceTransactionReceiptV1 {
                struct_size: size_of::<SourceTransactionReceiptV1>() as u32,
                history_disposition: HistoryDisposition::Retained as u32,
                flags: SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT
                    | SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES
                    | if receipt.parser_pending {
                        SOURCE_TRANSACTION_RECEIPT_PARSER_PENDING
                    } else {
                        0
                    },
                reserved_u32: 0,
                logical_edit_id: request.logical_edit_id,
                request_digest: request.request_digest,
                base_revision: receipt.base_revision,
                result_revision: receipt.result_revision,
                base_byte_range: SourceRange {
                    start_byte,
                    end_byte: receipt.base_byte_range.end as u64,
                },
                base_utf16_range: SourceRange {
                    start_byte: receipt.base_utf16_range.start as u64,
                    end_byte: receipt.base_utf16_range.end as u64,
                },
                result_byte_range: SourceRange {
                    start_byte: receipt.result_byte_range.start as u64,
                    end_byte: receipt.result_byte_range.end as u64,
                },
                result_utf16_range: SourceRange {
                    start_byte: receipt.result_utf16_range.start as u64,
                    end_byte: receipt.result_utf16_range.end as u64,
                },
                result_selection_base_utf16: receipt.result_selection_utf16 as u64,
                result_selection_extent_utf16: receipt.result_selection_utf16 as u64,
                result_selection_affinity: Affinity::Downstream as u32,
                result_selection_direction: 0,
                result_source_byte_length: receipt.result_source_byte_length as u64,
                result_source_utf16_length: receipt.result_source_utf16_length as u64,
                affected_result_utf16_range: SourceRange {
                    start_byte: start_utf16 as u64,
                    end_byte: receipt.result_utf16_range.end as u64,
                },
                history_token,
                replacement_bytes: replacement_len,
                reserved: [0; 2],
            },
        };
        let result = source_transaction_terminal_outcome_for(
            &terminal,
            OperationCode::StagedSourceTransactionV1,
        );
        let entry = registry
            .sessions
            .get_mut(&request.session.session)
            .expect("committed staged source session must remain live");
        entry.terminal_edit_intent = None;
        entry.terminal_source_transaction = Some(terminal);
        unsafe {
            write_source_transaction_terminal(
                entry
                    .terminal_source_transaction
                    .as_ref()
                    .expect("stored staged terminal must remain live"),
                output,
            )
        };
        Ok(result)
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_bulk_abort(
    request: *const TransactionRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::BulkAbort, outcome, || {
        let request = unsafe { read_record(request, size_of::<TransactionRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.transaction == 0
            || request.expected_revision == 0
            || request.progress_token != 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        session_entry(&mut registry, request.session)?;
        let transaction = registry
            .transactions
            .get(&request.transaction)
            .ok_or(StatusCode::InvalidHandle)?;
        if transaction.session != request.session.session
            || transaction.owner != request.session.owner_token
            || transaction.expected_revision != request.expected_revision
        {
            return Err(StatusCode::TransactionConflict);
        }
        registry.transactions.remove(&request.transaction);
        session_entry(&mut registry, request.session)?
            .transactions
            .remove(&request.transaction);
        Ok(RuntimeOutcome {
            operation: OperationCode::BulkAbort,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

#[derive(Clone, Copy)]
enum HistoryPiece {
    Original { start: u64, end: u64 },
    Replacement { step: usize, start: u64, end: u64 },
}

fn history_piece_len(piece: HistoryPiece) -> u64 {
    match piece {
        HistoryPiece::Original { start, end } | HistoryPiece::Replacement { start, end, .. } => {
            end - start
        }
    }
}

fn history_piece_slice(piece: HistoryPiece, start: u64, end: u64) -> HistoryPiece {
    match piece {
        HistoryPiece::Original {
            start: source_start,
            ..
        } => HistoryPiece::Original {
            start: source_start + start,
            end: source_start + end,
        },
        HistoryPiece::Replacement {
            step,
            start: source_start,
            ..
        } => HistoryPiece::Replacement {
            step,
            start: source_start + start,
            end: source_start + end,
        },
    }
}

fn split_history_pieces_at(
    pieces: &mut Vec<HistoryPiece>,
    position: u64,
) -> Result<usize, StatusCode> {
    let mut cursor = 0u64;
    for index in 0..pieces.len() {
        let length = history_piece_len(pieces[index]);
        let end = cursor
            .checked_add(length)
            .ok_or(StatusCode::ResourceLimitExceeded)?;
        if position == cursor {
            return Ok(index);
        }
        if position < end {
            let cut = position - cursor;
            let piece = pieces[index];
            let left = history_piece_slice(piece, 0, cut);
            let right = history_piece_slice(piece, cut, length);
            pieces[index] = left;
            pieces
                .try_reserve_exact(1)
                .map_err(|_| StatusCode::AllocationFailure)?;
            pieces.insert(index + 1, right);
            return Ok(index + 1);
        }
        cursor = end;
    }
    if position == cursor {
        Ok(pieces.len())
    } else {
        Err(StatusCode::HistoryTokenStale)
    }
}

/// Reduces a chronological composite inverse to one bounded splice against
/// the current source. All allocation, coordinate validation, and source
/// materialization complete before the document's single replay commit.
fn materialize_composite_history(
    document: &DocumentActor,
    history: &StoredHistory,
    source_byte_length: u64,
) -> Result<(u64, u64, String), StatusCode> {
    let mut pieces = Vec::new();
    pieces
        .try_reserve_exact(history.splices.len().saturating_mul(2).saturating_add(1))
        .map_err(|_| StatusCode::AllocationFailure)?;
    if source_byte_length != 0 {
        pieces.push(HistoryPiece::Original {
            start: 0,
            end: source_byte_length,
        });
    }
    for step in (0..history.splices.len()).rev() {
        let splice = &history.splices[step];
        let start = split_history_pieces_at(&mut pieces, splice.start_byte)?;
        let end = split_history_pieces_at(&mut pieces, splice.end_byte)?;
        if start > end {
            return Err(StatusCode::HistoryTokenStale);
        }
        let replacement = if splice.replacement.is_empty() {
            None
        } else {
            Some(HistoryPiece::Replacement {
                step,
                start: 0,
                end: splice.replacement.len() as u64,
            })
        };
        pieces.splice(start..end, replacement);
    }

    let mut prefix = 0u64;
    let mut prefix_count = 0usize;
    for piece in &pieces {
        match *piece {
            HistoryPiece::Original { start, end } if start == prefix => {
                prefix = end;
                prefix_count += 1;
            }
            _ => break,
        }
    }
    let mut suffix = source_byte_length;
    let mut suffix_count = 0usize;
    for piece in pieces[prefix_count..].iter().rev() {
        match *piece {
            HistoryPiece::Original { start, end } if end == suffix => {
                suffix = start;
                suffix_count += 1;
            }
            _ => break,
        }
    }
    if prefix > suffix {
        return Err(StatusCode::HistoryTokenStale);
    }
    let middle_end = pieces.len().saturating_sub(suffix_count);
    let middle = &pieces[prefix_count..middle_end];
    let replacement_bytes = middle.iter().try_fold(0u64, |total, piece| {
        total
            .checked_add(history_piece_len(*piece))
            .ok_or(StatusCode::ResourceLimitExceeded)
    })?;
    if suffix - prefix > MAX_COMPOSITE_HISTORY_MATERIALIZED_BYTES
        || replacement_bytes > MAX_COMPOSITE_HISTORY_MATERIALIZED_BYTES
    {
        return Err(StatusCode::ResourceLimitExceeded);
    }
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(
            usize::try_from(replacement_bytes).map_err(|_| StatusCode::ResourceLimitExceeded)?,
        )
        .map_err(|_| StatusCode::AllocationFailure)?;
    for piece in middle {
        match *piece {
            HistoryPiece::Original { start, end } => {
                let bytes = document
                    .source_bytes(start as usize..end as usize)
                    .map_err(|error| map_actor_error(&error))?;
                replacement.extend_from_slice(&bytes);
            }
            HistoryPiece::Replacement { step, start, end } => {
                replacement.extend_from_slice(
                    &history.splices[step].replacement[start as usize..end as usize],
                );
            }
        }
    }
    let replacement = String::from_utf8(replacement).map_err(|_| StatusCode::InternalFault)?;
    Ok((prefix, suffix, replacement))
}

#[no_mangle]
pub extern "C" fn flark_v4_history_replay(
    request: *const HistoryRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::HistoryReplay, outcome, || {
        let request = unsafe { read_record(request, size_of::<HistoryRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.expected_revision == 0
            || request.history_token == 0
            || request.progress_token != 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let history = history_for_request(&mut registry, request.session, request.history_token)?;
        if session_entry(&mut registry, request.session)?.history_state != history.applies_state {
            return Err(StatusCode::HistoryTokenStale);
        }
        let (receipt, inverse, start_byte, end_byte, replay_end) = {
            let entry = session_entry(&mut registry, request.session)?;
            acknowledge_edit_terminal_for_ordered_mutation(entry, request.expected_revision);
            let StoredSessionState::Open(document) = &mut entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            let inspection = document
                .inspect()
                .map_err(|error| map_actor_error(&error))?;
            if inspection.revision != request.expected_revision {
                return Err(StatusCode::StaleRevision);
            }
            let (start_byte, end_byte, replacement) = if history.splices.len() == 1 {
                let splice = &history.splices[0];
                let replacement = str::from_utf8(&splice.replacement)
                    .map_err(|_| StatusCode::InternalFault)?
                    .to_owned();
                (splice.start_byte, splice.end_byte, replacement)
            } else {
                materialize_composite_history(
                    document,
                    &history,
                    inspection.source_byte_len as u64,
                )?
            };
            let start = usize::try_from(start_byte).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let end = usize::try_from(end_byte).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let replay_end = start_byte
                .checked_add(replacement.len() as u64)
                .ok_or(StatusCode::ResourceLimitExceeded)?;
            let inverse = document
                .source_bytes(start..end)
                .map_err(|error| map_actor_error(&error))?;
            let receipt = document
                .apply_edit(request.expected_revision, start..end, replacement)
                .map_err(|error| map_actor_error(&error))?;
            entry.history_state = history.target_state;
            entry.progress_token = 0;
            entry.continuations.clear();
            (receipt, inverse, start_byte, end_byte, replay_end)
        };
        registry
            .continuations
            .retain(|_, continuation| continuation.session != request.session.session);
        transform_session_anchors(
            &mut registry,
            request.session.session,
            start_byte,
            end_byte - start_byte,
            replay_end - start_byte,
        );
        let _ = detach_history(&mut registry, request.history_token);
        let (history_token, history) = retain_history(
            &mut registry,
            request.session,
            history.target_state,
            history.applies_state,
            start_byte,
            replay_end,
            inverse,
            0,
        )
        .unwrap_or((HistoryToken::NONE, HistoryDisposition::OverBudget));
        Ok(RuntimeOutcome {
            operation: OperationCode::HistoryReplay,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::RevisionCommitted {
                revision: Revision(receipt.revision),
                history_token,
                history,
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_history_release(
    request: *const HistoryRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::HistoryRelease, outcome, || {
        let request = unsafe { read_record(request, size_of::<HistoryRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.expected_revision == 0
            || request.history_token == 0
            || request.progress_token != 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = owned_session_entry(&mut registry, request.session)?;
        if entry.evicted_history_tokens.remove(&request.history_token) {
            return Err(StatusCode::HistoryTokenEvicted);
        }
        {
            let history = registry
                .histories
                .get(&request.history_token)
                .ok_or(StatusCode::HistoryTokenStale)?;
            if history.owner != request.session.owner_token {
                return Err(StatusCode::OwnerMismatch);
            }
            if history.session != request.session.session {
                return Err(StatusCode::HistoryTokenStale);
            }
        }
        let entry = owned_session_entry(&mut registry, request.session)?;
        let StoredSessionState::Open(document) = &entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if inspection.revision != request.expected_revision {
            return Err(StatusCode::StaleRevision);
        }
        detach_history(&mut registry, request.history_token)
            .ok_or(StatusCode::HistoryTokenStale)?;
        Ok(RuntimeOutcome {
            operation: OperationCode::HistoryRelease,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_coordinate_convert(
    request: *const CoordinateRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::CoordinateConvert, outcome, || {
        let request = unsafe { read_record(request, size_of::<CoordinateRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.snapshot != 0
            || request.progress_token != 0
        {
            return Err(StatusCode::InvalidArgument);
        }
        let from = match request.from_kind {
            1 => CoordinateKind::SourceByte,
            2 => CoordinateKind::Utf16CodeUnit,
            _ => return Err(StatusCode::InvalidArgument),
        };
        let to = match request.to_kind {
            1 => CoordinateKind::SourceByte,
            2 => CoordinateKind::Utf16CodeUnit,
            _ => return Err(StatusCode::InvalidArgument),
        };
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = session_entry(&mut registry, request.session)?;
        let StoredSessionState::Open(document) = &mut entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if request.revision != inspection.revision {
            return Err(StatusCode::StaleRevision);
        }
        let position =
            usize::try_from(request.position).map_err(|_| StatusCode::CoordinateOutOfRange)?;
        let converted = match (from, to) {
            (CoordinateKind::SourceByte, CoordinateKind::Utf16CodeUnit) => document
                .utf16_offset_for_byte(position)
                .map_err(|_| StatusCode::CoordinateOutOfRange)?,
            (CoordinateKind::Utf16CodeUnit, CoordinateKind::SourceByte) => document
                .byte_offset_for_utf16(position)
                .map_err(|_| StatusCode::CoordinateOutOfRange)?,
            (CoordinateKind::SourceByte, CoordinateKind::SourceByte)
                if position <= inspection.source_byte_len =>
            {
                position
            }
            (CoordinateKind::Utf16CodeUnit, CoordinateKind::Utf16CodeUnit)
                if position <= inspection.source_utf16_len =>
            {
                position
            }
            _ => return Err(StatusCode::CoordinateOutOfRange),
        };
        Ok(RuntimeOutcome {
            operation: OperationCode::CoordinateConvert,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::ConvertedPosition {
                revision: Revision(request.revision),
                coordinate: to,
                position: converted as u64,
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_anchor_create(
    request: *const AnchorRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::AnchorCreate, outcome, || {
        let request = unsafe { read_record(request, size_of::<AnchorRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.anchor != 0
            || request.snapshot != 0
            || request.progress_token != 0
            || request.reserved_u32 != 0
        {
            return Err(StatusCode::InvalidArgument);
        }
        let coordinate = match request.coordinate_kind {
            1 => CoordinateKind::SourceByte,
            2 => CoordinateKind::Utf16CodeUnit,
            _ => return Err(StatusCode::InvalidArgument),
        };
        let affinity = match request.affinity {
            1 => Affinity::Upstream,
            2 => Affinity::Downstream,
            _ => return Err(StatusCode::InvalidArgument),
        };
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let (byte_offset, revision) = {
            let entry = session_entry(&mut registry, request.session)?;
            if entry.anchors.len() >= MAX_LIVE_ANCHORS as usize {
                return Err(StatusCode::ResourceLimitExceeded);
            }
            let StoredSessionState::Open(document) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            let inspection = document
                .inspect()
                .map_err(|error| map_actor_error(&error))?;
            if request.revision != inspection.revision {
                return Err(StatusCode::StaleRevision);
            }
            let position =
                usize::try_from(request.position).map_err(|_| StatusCode::CoordinateOutOfRange)?;
            let byte_offset = match coordinate {
                CoordinateKind::SourceByte => {
                    if position > inspection.source_byte_len {
                        return Err(StatusCode::CoordinateOutOfRange);
                    }
                    document
                        .utf16_offset_for_byte(position)
                        .map_err(|_| StatusCode::RangeNotScalarBoundary)?;
                    request.position
                }
                CoordinateKind::Utf16CodeUnit => {
                    if position > inspection.source_utf16_len {
                        return Err(StatusCode::CoordinateOutOfRange);
                    }
                    let byte = document
                        .byte_offset_for_utf16(position)
                        .map_err(|_| StatusCode::RangeNotScalarBoundary)?;
                    byte as u64
                }
            };
            (byte_offset, inspection.revision)
        };
        let handle = registry.allocate_handle()?;
        registry.anchors.insert(
            handle,
            StoredAnchor {
                session: request.session.session,
                byte_offset,
                affinity,
            },
        );
        owned_session_entry(&mut registry, request.session)?
            .anchors
            .insert(handle);
        Ok(RuntimeOutcome {
            operation: OperationCode::AnchorCreate,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::Anchor {
                anchor: AnchorHandle(handle),
                revision: Revision(revision),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_anchor_transform(
    request: *const AnchorRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::AnchorTransform, outcome, || {
        let request = unsafe { read_record(request, size_of::<AnchorRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.coordinate_kind != 0
            || request.snapshot != 0
            || request.position != 0
            || request.affinity != 0
            || request.progress_token != 0
            || request.reserved_u32 != 0
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        session_entry(&mut registry, request.session)?;
        anchor_for_request(&registry, request.session, request.anchor)?;
        let entry = session_entry(&mut registry, request.session)?;
        let StoredSessionState::Open(document) = &entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if request.revision != inspection.revision {
            return Err(StatusCode::StaleRevision);
        }
        Ok(RuntimeOutcome {
            operation: OperationCode::AnchorTransform,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::Anchor {
                anchor: AnchorHandle(request.anchor),
                revision: Revision(inspection.revision),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_anchor_resolve(
    request: *const AnchorRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::AnchorResolve, outcome, || {
        let request = unsafe { read_record(request, size_of::<AnchorRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.snapshot != 0
            || request.position != 0
            || request.affinity != 0
            || request.progress_token != 0
            || request.reserved_u32 != 0
        {
            return Err(StatusCode::InvalidArgument);
        }
        let coordinate = match request.coordinate_kind {
            1 => CoordinateKind::SourceByte,
            2 => CoordinateKind::Utf16CodeUnit,
            _ => return Err(StatusCode::InvalidArgument),
        };
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        session_entry(&mut registry, request.session)?;
        let byte_offset =
            anchor_for_request(&registry, request.session, request.anchor)?.byte_offset;
        let entry = session_entry(&mut registry, request.session)?;
        let StoredSessionState::Open(document) = &entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if request.revision != inspection.revision {
            return Err(StatusCode::StaleRevision);
        }
        let position = match coordinate {
            CoordinateKind::SourceByte => byte_offset,
            CoordinateKind::Utf16CodeUnit => {
                let offset = usize::try_from(byte_offset).map_err(|_| StatusCode::InternalFault)?;
                document
                    .utf16_offset_for_byte(offset)
                    .map_err(|_| StatusCode::InternalFault)? as u64
            }
        };
        Ok(RuntimeOutcome {
            operation: OperationCode::AnchorResolve,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::AnchorPosition {
                anchor: AnchorHandle(request.anchor),
                revision: Revision(inspection.revision),
                coordinate,
                position,
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_anchor_release(
    request: *const AnchorRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::AnchorRelease, outcome, || {
        let request = unsafe { read_record(request, size_of::<AnchorRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.coordinate_kind != 0
            || request.revision != 0
            || request.snapshot != 0
            || request.position != 0
            || request.affinity != 0
            || request.progress_token != 0
            || request.reserved_u32 != 0
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        owned_session_entry(&mut registry, request.session)?;
        anchor_for_request(&registry, request.session, request.anchor)?;
        registry.anchors.remove(&request.anchor);
        owned_session_entry(&mut registry, request.session)?
            .anchors
            .remove(&request.anchor);
        Ok(RuntimeOutcome {
            operation: OperationCode::AnchorRelease,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_cancel(request: *const CancelRequest, outcome: *mut Outcome) -> u32 {
    emit(OperationCode::Cancel, outcome, || {
        let request = unsafe { read_record(request, size_of::<CancelRequest>() as u32)? };
        if request.flags != 0 || request.progress_token == 0 || request.reserved != [0; 4] {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = owned_session_entry(&mut registry, request.session)?;
        let StoredSessionState::Open(document) = &entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        if entry.progress_token == 0 || entry.progress_token != request.progress_token {
            return Err(StatusCode::StaleProgressToken);
        }
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        entry.progress_token = 0;
        Ok(RuntimeOutcome {
            operation: OperationCode::Cancel,
            status: StatusCode::Cancelled,
            progress: ProgressState::Cancelled,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::Progress {
                revision: Revision(inspection.revision),
                token: ProgressToken(request.progress_token),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_source_read(
    request: *const SourceReadRequest,
    output: *mut u8,
    output_capacity: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::SourceRead, outcome, || {
        let request = unsafe { read_record(request, size_of::<SourceReadRequest>() as u32)? };
        let length = request
            .range
            .end_byte
            .saturating_sub(request.range.start_byte);
        if request.range.start_byte > request.range.end_byte
            || length > u64::from(MAX_SOURCE_CHUNK_BYTES)
        {
            return Err(StatusCode::RangeOutOfBounds);
        }
        let required = size_of::<ResultPageHeader>() as u64 + length;
        if output.is_null() || output_capacity < required {
            return Ok(RuntimeOutcome {
                operation: OperationCode::SourceRead,
                status: StatusCode::BufferTooSmall,
                progress: ProgressState::None,
                required_payload_bytes: length,
                written_payload_bytes: 0,
                result: OperationResult::None,
            });
        }
        let start =
            usize::try_from(request.range.start_byte).map_err(|_| StatusCode::RangeOutOfBounds)?;
        let end =
            usize::try_from(request.range.end_byte).map_err(|_| StatusCode::RangeOutOfBounds)?;
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = session_entry(&mut registry, request.session)?;
        let StoredSessionState::Open(document) = &mut entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if request.revision != inspection.revision {
            return Err(StatusCode::StaleRevision);
        }
        let bytes = document
            .source_bytes(start..end)
            .map_err(|error| map_actor_error(&error))?;
        let page = ResultPageReceipt {
            record_kind: ResultRecordKind::SourceBytes,
            certification: CertificationState::NotApplicable,
            revision: Revision(request.revision),
            snapshot: SnapshotId::NOT_APPLICABLE,
            requested_range: RuntimeSourceRange {
                start_byte: request.range.start_byte,
                end_byte: request.range.end_byte,
            },
            covered_range: RuntimeSourceRange {
                start_byte: request.range.start_byte,
                end_byte: request.range.end_byte,
            },
            item_count: 0,
            payload_bytes: bytes.len() as u32,
            continuation: ContinuationHandle::NONE,
        };
        write_page(output, page, |payload| payload.copy_from_slice(&bytes))?;
        Ok(RuntimeOutcome {
            operation: OperationCode::SourceRead,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: bytes.len() as u64,
            result: OperationResult::Page(page),
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_query_viewport(
    request: *const QueryRequest,
    output: *mut u8,
    output_capacity: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::QueryViewport, outcome, || {
        let request = unsafe { read_record(request, size_of::<QueryRequest>() as u32)? };
        if !valid_budget(request.budget, true)
            || request.continuation != 0
            || request.range.start_byte > request.range.end_byte
            || !matches!(request.query_kind, 1 | 2 | 3 | 4 | 5 | 6)
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        query_page(
            OperationCode::QueryViewport,
            &mut registry,
            request.session,
            request.revision,
            if request.snapshot == 0 {
                request.revision
            } else {
                request.snapshot
            },
            request.range.start_byte,
            request.range.end_byte,
            request.range.start_byte,
            request.query_kind,
            request.budget,
            None,
            output,
            output_capacity,
        )
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_continuation_next(
    request: *const ContinuationRequest,
    output: *mut u8,
    output_capacity: u64,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::ContinuationNext, outcome, || {
        let request = unsafe { read_record(request, size_of::<ContinuationRequest>() as u32)? };
        if !valid_budget(request.budget, true)
            || request.flags != 0
            || request.revision == 0
            || request.snapshot == 0
            || request.continuation == 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        session_entry(&mut registry, request.session)?;
        let continuation = registry
            .continuations
            .get(&request.continuation)
            .copied()
            .ok_or(StatusCode::StaleContinuation)?;
        if continuation.owner != request.session.owner_token {
            return Err(StatusCode::OwnerMismatch);
        }
        if continuation.session != request.session.session
            || continuation.revision != request.revision
            || continuation.snapshot != request.snapshot
        {
            return Err(StatusCode::StaleContinuation);
        }
        query_page(
            OperationCode::ContinuationNext,
            &mut registry,
            request.session,
            request.revision,
            request.snapshot,
            continuation.requested_start,
            continuation.requested_end,
            continuation.next_start,
            continuation.query_kind,
            request.budget,
            Some(request.continuation),
            output,
            output_capacity,
        )
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_continuation_release(
    request: *const ContinuationRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::ContinuationRelease, outcome, || {
        let request = unsafe { read_record(request, size_of::<ContinuationRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.revision == 0
            || request.snapshot == 0
            || request.continuation == 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        session_entry(&mut registry, request.session)?;
        let continuation = registry
            .continuations
            .get(&request.continuation)
            .copied()
            .ok_or(StatusCode::StaleContinuation)?;
        if continuation.owner != request.session.owner_token {
            return Err(StatusCode::OwnerMismatch);
        }
        if continuation.session != request.session.session
            || continuation.revision != request.revision
            || continuation.snapshot != request.snapshot
        {
            return Err(StatusCode::StaleContinuation);
        }
        registry.continuations.remove(&request.continuation);
        let entry = owned_session_entry(&mut registry, request.session)?;
        entry.continuations.remove(&request.continuation);
        Ok(RuntimeOutcome {
            operation: OperationCode::ContinuationRelease,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_close_begin(request: *const CloseRequest, outcome: *mut Outcome) -> u32 {
    emit(OperationCode::CloseBegin, outcome, || {
        let request = unsafe { read_record(request, size_of::<CloseRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.progress_token != 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let close_token = registry.allocate_handle()?;
        {
            let entry = owned_session_entry(&mut registry, request.session)?;
            if entry.close_token != 0 {
                return Err(StatusCode::SessionClosing);
            }
            match &mut entry.state {
                StoredSessionState::Creating { .. } => {}
                StoredSessionState::Open(document) => document
                    .begin_close()
                    .map_err(|error| map_actor_error(&error))?,
            }
            entry.close_token = close_token;
            entry.close_complete = false;
        }
        let complete = pump_session_close(
            &mut registry,
            request.session,
            usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX),
        )?;
        Ok(RuntimeOutcome {
            operation: OperationCode::CloseBegin,
            status: if complete {
                StatusCode::Ok
            } else {
                StatusCode::BudgetExhausted
            },
            progress: if complete {
                ProgressState::Complete
            } else {
                ProgressState::BudgetExhausted
            },
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::CloseProgress {
                token: ProgressToken(close_token),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_close_pump(request: *const CloseRequest, outcome: *mut Outcome) -> u32 {
    emit(OperationCode::ClosePump, outcome, || {
        let request = unsafe { read_record(request, size_of::<CloseRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.progress_token == 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        {
            let entry = owned_session_entry(&mut registry, request.session)?;
            if entry.close_token == 0 || entry.close_token != request.progress_token {
                return Err(StatusCode::StaleProgressToken);
            }
        }
        let complete = pump_session_close(
            &mut registry,
            request.session,
            usize::try_from(request.budget.max_work_units).unwrap_or(usize::MAX),
        )?;
        let next_token = registry.allocate_handle()?;
        owned_session_entry(&mut registry, request.session)?.close_token = next_token;
        Ok(RuntimeOutcome {
            operation: OperationCode::ClosePump,
            status: if complete {
                StatusCode::Ok
            } else {
                StatusCode::BudgetExhausted
            },
            progress: if complete {
                ProgressState::Complete
            } else {
                ProgressState::BudgetExhausted
            },
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::CloseProgress {
                token: ProgressToken(next_token),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_close_finish(
    request: *const CloseRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::CloseFinish, outcome, || {
        let request = unsafe { read_record(request, size_of::<CloseRequest>() as u32)? };
        if !valid_budget(request.budget, false)
            || request.flags != 0
            || request.progress_token == 0
            || request.reserved != [0; 1]
        {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let complete = {
            let entry = owned_session_entry(&mut registry, request.session)?;
            if entry.close_token == 0 || entry.close_token != request.progress_token {
                return Err(StatusCode::StaleProgressToken);
            }
            entry.close_complete
                && entry.transactions.is_empty()
                && entry.continuations.is_empty()
                && entry.anchors.is_empty()
                && entry.history_head == 0
                && entry.evicted_history_tokens.is_empty()
        };
        if !complete {
            return Ok(RuntimeOutcome {
                operation: OperationCode::CloseFinish,
                status: StatusCode::CloseIncomplete,
                progress: ProgressState::None,
                required_payload_bytes: 0,
                written_payload_bytes: 0,
                result: OperationResult::CloseProgress {
                    token: ProgressToken(request.progress_token),
                },
            });
        }
        let removed = registry.sessions.remove(&request.session.session);
        if removed.is_none() {
            return Err(StatusCode::InvalidHandle);
        }
        Ok(RuntimeOutcome {
            operation: OperationCode::CloseFinish,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        })
    })
}

fn pump_session_close(
    registry: &mut Registry,
    session: SessionRef,
    max_work_units: usize,
) -> Result<bool, StatusCode> {
    let mut remaining = max_work_units;
    while remaining != 0 {
        let transaction = owned_session_entry(registry, session)?
            .transactions
            .first()
            .copied();
        if let Some(transaction) = transaction {
            registry.transactions.remove(&transaction);
            owned_session_entry(registry, session)?
                .transactions
                .remove(&transaction);
            remaining -= 1;
            continue;
        }

        let continuation = owned_session_entry(registry, session)?
            .continuations
            .first()
            .copied();
        if let Some(continuation) = continuation {
            registry.continuations.remove(&continuation);
            owned_session_entry(registry, session)?
                .continuations
                .remove(&continuation);
            remaining -= 1;
            continue;
        }

        let anchor = owned_session_entry(registry, session)?
            .anchors
            .first()
            .copied();
        if let Some(anchor) = anchor {
            registry.anchors.remove(&anchor);
            owned_session_entry(registry, session)?
                .anchors
                .remove(&anchor);
            remaining -= 1;
            continue;
        }

        let history = owned_session_entry(registry, session)?.history_head;
        if history != 0 {
            detach_history(registry, history).ok_or(StatusCode::InternalFault)?;
            remaining -= 1;
            continue;
        }

        let evicted_history = owned_session_entry(registry, session)?
            .evicted_history_tokens
            .first()
            .copied();
        if let Some(history) = evicted_history {
            owned_session_entry(registry, session)?
                .evicted_history_tokens
                .remove(&history);
            remaining -= 1;
            continue;
        }

        let entry = owned_session_entry(registry, session)?;
        match &mut entry.state {
            StoredSessionState::Creating { bytes, .. } => {
                *bytes = Vec::new();
                entry.close_complete = true;
            }
            StoredSessionState::Open(document) => {
                let receipt = document
                    .pump_close(remaining)
                    .map_err(|error| map_actor_error(&error))?;
                entry.close_complete = receipt.complete;
            }
        }
        break;
    }
    let entry = owned_session_entry(registry, session)?;
    Ok(entry.close_complete
        && entry.transactions.is_empty()
        && entry.continuations.is_empty()
        && entry.anchors.is_empty()
        && entry.history_head == 0
        && entry.evicted_history_tokens.is_empty())
}

#[no_mangle]
pub extern "C" fn flark_v4_session_transfer_owner(
    request: *const OwnerTransferRequest,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::SessionTransferOwner, outcome, || {
        let request = unsafe { read_record(request, size_of::<OwnerTransferRequest>() as u32)? };
        if request.flags != 0 || request.new_owner_token == 0 || request.reserved != [0; 4] {
            return Err(StatusCode::InvalidArgument);
        }
        {
            let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
            let entry = session_entry(&mut registry, request.session)?;
            let StoredSessionState::Open(_) = &entry.state else {
                return Err(StatusCode::SessionBusy);
            };
            if entry.progress_token != 0
                || !entry.transactions.is_empty()
                || !entry.continuations.is_empty()
            {
                return Err(StatusCode::MigrationWhileActive);
            }
            entry.owner = request.new_owner_token;
            // Retained history tokens survive an idle owner migration, so
            // their stored owner authority must follow the session's.
            for history in registry.histories.values_mut() {
                if history.session == request.session.session {
                    history.owner = request.new_owner_token;
                }
            }
        }
        Ok(RuntimeOutcome {
            operation: OperationCode::SessionTransferOwner,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::OwnerTransferred {
                session: SessionHandle(request.session.session),
            },
        })
    })
}

#[no_mangle]
pub extern "C" fn flark_v4_session_inspect(
    request: *const InspectRequest,
    inspection: *mut SessionInspection,
    outcome: *mut Outcome,
) -> u32 {
    emit(OperationCode::SessionInspect, outcome, || {
        let request = unsafe { read_record(request, size_of::<InspectRequest>() as u32)? };
        if request.flags != 0 || request.reserved != [0; 5] || inspection.is_null() {
            return Err(StatusCode::InvalidArgument);
        }
        let mut registry = registry().lock().map_err(|_| StatusCode::InternalFault)?;
        let entry = owned_session_entry(&mut registry, request.session)?;
        let (state, revision, live_transactions) = match &entry.state {
            StoredSessionState::Creating { .. } => (SessionState::Creating, 0, 1),
            StoredSessionState::Open(document) => {
                let observed = document
                    .inspect()
                    .map_err(|error| map_actor_error(&error))?;
                let state = if observed.phase == DocumentSessionPhase::Faulted {
                    SessionState::Faulted
                } else if entry.close_token != 0 {
                    SessionState::Closing
                } else {
                    SessionState::Open
                };
                let transactions = u32::try_from(entry.transactions.len())
                    .map_err(|_| StatusCode::InternalFault)?;
                (state, observed.revision, transactions)
            }
        };
        let receipt = SessionInspectionReceipt {
            session: SessionHandle(request.session.session),
            state,
            revision: Revision(revision),
            live_transactions,
            live_continuations: u32::try_from(entry.continuations.len())
                .map_err(|_| StatusCode::InternalFault)?,
            live_anchors: u32::try_from(entry.anchors.len())
                .map_err(|_| StatusCode::InternalFault)?,
            live_history_tokens: entry.history_token_count,
        };
        unsafe {
            ptr::write_unaligned(inspection, SessionInspection::from_runtime(receipt));
        }
        Ok(RuntimeOutcome {
            operation: OperationCode::SessionInspect,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::SessionInspection(receipt),
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn query_page(
    operation: OperationCode,
    registry: &mut Registry,
    session: SessionRef,
    revision: u64,
    snapshot: u64,
    mut requested_start: u64,
    mut requested_end: u64,
    mut page_start: u64,
    query_kind: u32,
    budget: WorkBudget,
    prior_continuation: Option<u64>,
    output: *mut u8,
    output_capacity: u64,
) -> Result<RuntimeOutcome, StatusCode> {
    if output.is_null() || output_capacity < size_of::<ResultPageHeader>() as u64 {
        return Ok(RuntimeOutcome {
            operation,
            status: StatusCode::BufferTooSmall,
            progress: ProgressState::None,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        });
    }
    let continuation_candidate = registry.allocate_handle()?;
    let owner = session.owner_token;
    let (page, status, payload) = {
        let entry = session_entry(registry, session)?;
        #[cfg(feature = "opening-session")]
        let opening = entry.opening.is_some();
        let StoredSessionState::Open(document) = &mut entry.state else {
            return Err(StatusCode::SessionBusy);
        };
        // An opening-query session serves certified semantic rows before it
        // is Ready, and exact pending source until certification exists —
        // the contract's honest pre-certification reply.
        #[cfg(feature = "opening-session")]
        let opening_semantic_query = opening
            && document
                .opening_certified()
                .map_err(|error| map_actor_error(&error))?;
        let inspection = document
            .inspect()
            .map_err(|error| map_actor_error(&error))?;
        if revision != inspection.revision {
            return Err(if operation == OperationCode::ContinuationNext {
                StatusCode::StaleContinuation
            } else {
                StatusCode::StaleRevision
            });
        }
        if snapshot != revision {
            return Err(StatusCode::StaleSnapshot);
        }
        if requested_start > page_start
            || page_start > requested_end
            || requested_end > inspection.source_byte_len as u64
        {
            return Err(StatusCode::RangeOutOfBounds);
        }
        // Viewport ranges are byte-budget hints, so the host cannot be
        // required to place them on UTF-8 scalar boundaries. Resolve the
        // effective interval once at the ABI boundary and use it for the
        // runtime query, returned page header, and any stored continuation.
        // Semantic-target activation is an exact parser-authored range and
        // remains strict instead of silently changing the selected fact.
        if query_kind != QueryKind::SemanticTarget as u32 {
            requested_start = document
                .snapped_to_scalar_boundary(
                    usize::try_from(requested_start).map_err(|_| StatusCode::RangeOutOfBounds)?,
                )
                .map_err(|error| map_actor_error(&error))? as u64;
            requested_end = document
                .snapped_to_scalar_boundary(
                    usize::try_from(requested_end).map_err(|_| StatusCode::RangeOutOfBounds)?,
                )
                .map_err(|error| map_actor_error(&error))? as u64;
            page_start = document
                .snapped_to_scalar_boundary(
                    usize::try_from(page_start).map_err(|_| StatusCode::RangeOutOfBounds)?,
                )
                .map_err(|error| map_actor_error(&error))? as u64;
        }
        let payload_capacity = usize::try_from(
            output_capacity
                .saturating_sub(size_of::<ResultPageHeader>() as u64)
                .min(u64::from(budget.max_result_bytes)),
        )
        .unwrap_or(usize::MAX);

        // During a progressive open, semantic rows may only cover the
        // contiguous certified span beginning at this page. A continuation
        // that has reached pending source therefore falls back to the exact
        // pending-source page below instead of claiming empty certified
        // semantics for the remainder of the request.
        #[cfg(feature = "opening-session")]
        let opening_semantic_end = if opening_semantic_query
            && matches!(query_kind, 2 | 4 | 6)
            && page_start < requested_end
        {
            let start = usize::try_from(page_start).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let end = usize::try_from(requested_end).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let live = document
                .query_live_viewport(revision, start..end, 3)
                .map_err(|error| map_actor_error(&error))?;
            live.spans.first().and_then(|span| match span {
                DocumentLiveViewportSpan::CertifiedUnchanged { source_range, .. }
                    if source_range.start == page_start =>
                {
                    Some(source_range.end)
                }
                _ => None,
            })
        } else {
            None
        };
        #[cfg(not(feature = "opening-session"))]
        let opening_semantic_end: Option<u64> = None;
        let opening_semantic_page = opening_semantic_end.is_some();

        if query_kind == 5 {
            if page_start != requested_start || requested_start >= requested_end {
                return Err(StatusCode::InvalidArgument);
            }
            let start =
                usize::try_from(requested_start).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let end = usize::try_from(requested_end).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let target = match document.query_semantic_target(revision, start..end) {
                Ok(target) => target,
                // Target activation is an optional presentation query. While
                // the parser is retiring old facts, expose an empty current
                // answer instead of turning normal incremental work into an
                // exceptional UI path.
                Err(DocumentActorError::Session(DocumentSessionError::NotReady)) => None,
                Err(error) => return Err(map_actor_error(&error)),
            };
            let mut payload_bytes = 0_usize;
            let payload = if let Some(target) = target {
                let title_bytes = target.title.as_deref().map_or(&[][..], str::as_bytes);
                payload_bytes = size_of::<SemanticTargetRecord>()
                    .saturating_add(target.destination.len())
                    .saturating_add(title_bytes.len());
                if payload_bytes > payload_capacity {
                    return Ok(RuntimeOutcome {
                        operation,
                        status: StatusCode::BufferTooSmall,
                        progress: ProgressState::None,
                        required_payload_bytes: payload_bytes as u64,
                        written_payload_bytes: 0,
                        result: OperationResult::None,
                    });
                }
                QueryPayload::SemanticTarget {
                    record: semantic_target_record(&target)?,
                    destination: target.destination.into_bytes(),
                    title: target.title.map_or_else(Vec::new, String::into_bytes),
                }
            } else {
                QueryPayload::SemanticTarget {
                    record: SemanticTargetRecord::default(),
                    destination: Vec::new(),
                    title: Vec::new(),
                }
            };
            let page = ResultPageReceipt {
                record_kind: ResultRecordKind::SemanticTarget,
                certification: CertificationState::CurrentCertified,
                revision: Revision(revision),
                snapshot: SnapshotId(snapshot),
                requested_range: RuntimeSourceRange {
                    start_byte: requested_start,
                    end_byte: requested_end,
                },
                covered_range: RuntimeSourceRange {
                    start_byte: requested_start,
                    end_byte: requested_end,
                },
                item_count: u32::from(payload_bytes != 0),
                payload_bytes: payload_bytes as u32,
                continuation: ContinuationHandle::NONE,
            };
            (page, StatusCode::Ok, payload)
        } else if query_kind == 3 {
            let maximum_spans = budget.max_result_items.min(MAX_QUERY_ITEMS).min(3);
            if maximum_spans == 0 {
                return Ok(RuntimeOutcome {
                    operation,
                    status: StatusCode::BufferTooSmall,
                    progress: ProgressState::None,
                    required_payload_bytes: size_of::<CertificationRangeRecord>() as u64,
                    written_payload_bytes: 0,
                    result: OperationResult::None,
                });
            }
            let start = usize::try_from(page_start).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let end = usize::try_from(requested_end).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let viewport = document
                .query_live_viewport(revision, start..end, maximum_spans)
                .map_err(|error| map_actor_error(&error))?;
            let records = viewport
                .spans
                .iter()
                .map(certification_range_record)
                .collect::<Vec<_>>();
            let covered_start = usize::try_from(viewport.covered_range.start)
                .map_err(|_| StatusCode::RangeOutOfBounds)?;
            let covered_end = usize::try_from(viewport.covered_range.end)
                .map_err(|_| StatusCode::RangeOutOfBounds)?;
            let source = document
                .source_bytes(covered_start..covered_end)
                .map_err(|error| map_actor_error(&error))?;
            if !viewport.complete && covered_end <= covered_start {
                return Err(StatusCode::InternalFault);
            }
            let required_payload_bytes = records
                .len()
                .saturating_mul(size_of::<CertificationRangeRecord>())
                .saturating_add(source.len());
            if required_payload_bytes > payload_capacity {
                return Ok(RuntimeOutcome {
                    operation,
                    status: StatusCode::BufferTooSmall,
                    progress: ProgressState::None,
                    required_payload_bytes: required_payload_bytes as u64,
                    written_payload_bytes: 0,
                    result: OperationResult::None,
                });
            }
            let certification = if viewport.is_fully_certified() {
                CertificationState::CurrentCertified
            } else if records.iter().any(|record| {
                record.certification_state == CertificationState::CurrentCertified as u32
            }) {
                CertificationState::MixedCurrent
            } else {
                CertificationState::PendingNeutral
            };
            let page = ResultPageReceipt {
                record_kind: ResultRecordKind::SourceAndSemantic,
                certification,
                revision: Revision(revision),
                snapshot: SnapshotId(snapshot),
                requested_range: RuntimeSourceRange {
                    start_byte: requested_start,
                    end_byte: requested_end,
                },
                covered_range: RuntimeSourceRange {
                    start_byte: viewport.covered_range.start,
                    end_byte: viewport.covered_range.end,
                },
                item_count: records.len() as u32,
                payload_bytes: required_payload_bytes as u32,
                continuation: if viewport.complete {
                    ContinuationHandle::NONE
                } else {
                    ContinuationHandle(continuation_candidate)
                },
            };
            (
                page,
                if !viewport.complete {
                    StatusCode::ResultCapReached
                } else if certification == CertificationState::CurrentCertified {
                    StatusCode::Ok
                } else {
                    StatusCode::NotCertified
                },
                QueryPayload::CertificationRanges { records, source },
            )
        } else if (inspection.phase != DocumentSessionPhase::Ready && !opening_semantic_page)
            || query_kind == 1
        {
            let maximum = payload_capacity
                .min(MAX_SOURCE_CHUNK_BYTES as usize)
                .min(budget.max_result_bytes as usize);
            let start = usize::try_from(page_start).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let requested_end_usize =
                usize::try_from(requested_end).map_err(|_| StatusCode::RangeOutOfBounds)?;
            // Buffer and budget caps produce an arbitrary byte cut, which a
            // multi-byte scalar can straddle; only the runtime knows where a
            // legal cut is, and a page may cover less than requested.
            let end = document
                .snapped_to_scalar_boundary(start.saturating_add(maximum).min(requested_end_usize))
                .map_err(|error| map_actor_error(&error))?;
            let bytes = document
                .source_bytes(start..end)
                .map_err(|error| map_actor_error(&error))?;
            let has_more = end < requested_end_usize;
            let page = ResultPageReceipt {
                record_kind: ResultRecordKind::SourceBytes,
                certification: if query_kind == 1 {
                    CertificationState::NotApplicable
                } else {
                    CertificationState::PendingNeutral
                },
                revision: Revision(revision),
                snapshot: SnapshotId(snapshot),
                requested_range: RuntimeSourceRange {
                    start_byte: requested_start,
                    end_byte: requested_end,
                },
                covered_range: RuntimeSourceRange {
                    start_byte: page_start,
                    end_byte: end as u64,
                },
                item_count: 0,
                payload_bytes: bytes.len() as u32,
                continuation: if has_more {
                    ContinuationHandle(continuation_candidate)
                } else {
                    ContinuationHandle::NONE
                },
            };
            (
                page,
                if has_more {
                    StatusCode::ResultCapReached
                } else if query_kind == 1 {
                    StatusCode::Ok
                } else {
                    StatusCode::NotCertified
                },
                QueryPayload::Source(bytes),
            )
        } else {
            let maximum_rows_by_bytes = payload_capacity / size_of::<ViewportRowRecord>();
            let maximum_rows = maximum_rows_by_bytes
                .min(budget.max_result_items as usize)
                .min(MAX_QUERY_ITEMS as usize);
            if maximum_rows == 0 {
                return Ok(RuntimeOutcome {
                    operation,
                    status: StatusCode::BufferTooSmall,
                    progress: ProgressState::None,
                    required_payload_bytes: size_of::<ViewportRowRecord>() as u64,
                    written_payload_bytes: 0,
                    result: OperationResult::None,
                });
            }
            let start = usize::try_from(page_start).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let semantic_end = opening_semantic_end.unwrap_or(requested_end);
            let end = usize::try_from(semantic_end).map_err(|_| StatusCode::RangeOutOfBounds)?;
            let query_maximum_rows =
                maximum_rows.saturating_add(usize::from(opening_semantic_page));
            let viewport = document
                .query_viewport(
                    revision,
                    start..end,
                    u32::try_from(query_maximum_rows).map_err(|_| StatusCode::InternalFault)?,
                )
                .map_err(|error| map_actor_error(&error))?;
            let encoded_row_count = viewport.rows.len().min(maximum_rows);
            let opening_rows_capped =
                opening_semantic_page && viewport.rows.len() > encoded_row_count;
            let row_payload_bytes = encoded_row_count * size_of::<ViewportRowRecord>();
            let mut remaining_payload_bytes = payload_capacity.saturating_sub(row_payload_bytes);
            let mut records = Vec::with_capacity(encoded_row_count);
            let mut inline_facts = Vec::new();
            let mut projection_segments = Vec::new();
            for row in viewport.rows.iter().take(encoded_row_count) {
                let projection_segment_count = match &row.projection_segments {
                    Some(segments)
                        if matches!(query_kind, 4 | 6)
                            && segments.len() > 1
                            && segments.len() <= u16::MAX as usize
                            && segments.len() * size_of::<ProjectionSegmentRecord>()
                                <= remaining_payload_bytes =>
                    {
                        remaining_payload_bytes -=
                            segments.len() * size_of::<ProjectionSegmentRecord>();
                        projection_segments.extend(segments.iter().map(projection_segment_record));
                        u32::try_from(segments.len()).map_err(|_| StatusCode::InternalFault)?
                    }
                    _ => 0,
                };
                let literal_safe_envelope_count =
                    if query_kind == QueryKind::SemanticProjectedLiteralSafe as u32 {
                        row.literal_safe_envelopes.len()
                    } else {
                        0
                    };
                let projection_edit_cell_count =
                    if query_kind == QueryKind::SemanticProjectedLiteralSafe as u32 {
                        row.projection_edit_cells.len()
                    } else {
                        0
                    };
                let (inline_authoritative, inline_fact_count) = match &row.inline_facts {
                    Some(facts)
                        if facts.len()
                            + literal_safe_envelope_count
                            + projection_edit_cell_count
                            <= VIEWPORT_ROW_INLINE_FACT_COUNT_MASK as usize
                            && (facts.len()
                                + literal_safe_envelope_count
                                + projection_edit_cell_count)
                                * size_of::<InlineFactRecord>()
                                <= remaining_payload_bytes =>
                    {
                        let semantic_record_count =
                            facts.len() + literal_safe_envelope_count + projection_edit_cell_count;
                        remaining_payload_bytes -=
                            semantic_record_count * size_of::<InlineFactRecord>();
                        inline_facts.extend(facts.iter().map(inline_fact_record));
                        if literal_safe_envelope_count != 0 {
                            inline_facts.extend(
                                row.literal_safe_envelopes
                                    .iter()
                                    .map(literal_safe_envelope_record),
                            );
                        }
                        if projection_edit_cell_count != 0 {
                            inline_facts.extend(
                                row.projection_edit_cells
                                    .iter()
                                    .map(projection_edit_cell_record),
                            );
                        }
                        (
                            true,
                            u32::try_from(semantic_record_count)
                                .map_err(|_| StatusCode::InternalFault)?,
                        )
                    }
                    _ => (false, 0),
                };
                records.push(viewport_record(
                    row,
                    inline_authoritative,
                    inline_fact_count,
                    projection_segment_count,
                ));
            }
            let semantic_rows_complete = if opening_semantic_page {
                !opening_rows_capped
            } else {
                viewport.complete
            };
            let has_more = !semantic_rows_complete || semantic_end < requested_end;
            let covered_end = if !semantic_rows_complete {
                viewport
                    .rows
                    .iter()
                    .take(encoded_row_count)
                    .last()
                    .map_or(page_start, |row| row.source_range.end)
                    .min(semantic_end)
            } else {
                semantic_end
            };
            if has_more && covered_end <= page_start {
                return Err(StatusCode::InternalFault);
            }
            let page = ResultPageReceipt {
                record_kind: ResultRecordKind::SemanticFacts,
                certification: CertificationState::CurrentCertified,
                revision: Revision(revision),
                snapshot: SnapshotId(snapshot),
                requested_range: RuntimeSourceRange {
                    start_byte: requested_start,
                    end_byte: requested_end,
                },
                covered_range: RuntimeSourceRange {
                    start_byte: page_start,
                    end_byte: covered_end,
                },
                item_count: records.len() as u32,
                payload_bytes: (row_payload_bytes
                    + inline_facts.len() * size_of::<InlineFactRecord>()
                    + projection_segments.len() * size_of::<ProjectionSegmentRecord>())
                    as u32,
                continuation: if has_more {
                    ContinuationHandle(continuation_candidate)
                } else {
                    ContinuationHandle::NONE
                },
            };
            (
                page,
                if has_more {
                    StatusCode::ResultCapReached
                } else {
                    StatusCode::Ok
                },
                QueryPayload::Rows {
                    rows: records,
                    inline_facts,
                    projection_segments,
                },
            )
        }
    };

    write_page(output, page, |output| match &payload {
        QueryPayload::Source(bytes) => output.copy_from_slice(bytes),
        QueryPayload::Rows {
            rows,
            inline_facts,
            projection_segments,
        } => {
            let row_bytes = rows.len() * size_of::<ViewportRowRecord>();
            let inline_bytes = inline_facts.len() * size_of::<InlineFactRecord>();
            let (row_output, trailing_output) = output.split_at_mut(row_bytes);
            let (inline_output, segment_output) = trailing_output.split_at_mut(inline_bytes);
            write_row_records(row_output, rows);
            write_inline_fact_records(inline_output, inline_facts);
            write_projection_segment_records(segment_output, projection_segments);
        }
        QueryPayload::CertificationRanges { records, source } => {
            let record_bytes = records.len() * size_of::<CertificationRangeRecord>();
            let (record_output, source_output) = output.split_at_mut(record_bytes);
            write_certification_range_records(record_output, records);
            source_output.copy_from_slice(source);
        }
        QueryPayload::SemanticTarget {
            record,
            destination,
            title,
        } => {
            if destination.is_empty() && title.is_empty() && record.kind == 0 {
                return;
            }
            let (record_output, value_output) =
                output.split_at_mut(size_of::<SemanticTargetRecord>());
            unsafe {
                ptr::write_unaligned(
                    record_output.as_mut_ptr().cast::<SemanticTargetRecord>(),
                    *record,
                );
            }
            let (destination_output, title_output) = value_output.split_at_mut(destination.len());
            destination_output.copy_from_slice(destination);
            title_output.copy_from_slice(title);
        }
    })?;

    if let Some(prior) = prior_continuation {
        registry.continuations.remove(&prior);
        let entry = owned_session_entry(registry, session)?;
        entry.continuations.remove(&prior);
    }
    if page.continuation.0 != 0 {
        registry.continuations.insert(
            page.continuation.0,
            StoredContinuation {
                session: session.session,
                owner,
                revision,
                snapshot,
                requested_start,
                requested_end,
                next_start: page.covered_range.end_byte,
                query_kind,
            },
        );
        let entry = owned_session_entry(registry, session)?;
        entry.continuations.insert(page.continuation.0);
    }
    Ok(RuntimeOutcome {
        operation,
        status,
        progress: if status == StatusCode::ResultCapReached {
            ProgressState::ResultCapReached
        } else {
            ProgressState::Complete
        },
        required_payload_bytes: 0,
        written_payload_bytes: page.payload_bytes as u64,
        result: OperationResult::Page(page),
    })
}

enum QueryPayload {
    Source(Vec<u8>),
    Rows {
        rows: Vec<ViewportRowRecord>,
        inline_facts: Vec<InlineFactRecord>,
        projection_segments: Vec<ProjectionSegmentRecord>,
    },
    CertificationRanges {
        records: Vec<CertificationRangeRecord>,
        source: Vec<u8>,
    },
    SemanticTarget {
        record: SemanticTargetRecord,
        destination: Vec<u8>,
        title: Vec<u8>,
    },
}

fn semantic_target_record(
    target: &flark_runtime::DocumentSemanticTarget,
) -> Result<SemanticTargetRecord, StatusCode> {
    fn range(value: &std::ops::Range<u64>) -> SourceRange {
        SourceRange {
            start_byte: value.start,
            end_byte: value.end,
        }
    }
    let empty = SourceRange::default();
    Ok(SemanticTargetRecord {
        kind: match target.kind {
            DocumentSemanticTargetKind::Link => 1,
            DocumentSemanticTargetKind::Image => 2,
        },
        syntax: match target.syntax {
            DocumentSemanticTargetSyntax::AutolinkUri => 1,
            DocumentSemanticTargetSyntax::AutolinkEmail => 2,
            DocumentSemanticTargetSyntax::Direct => 3,
            DocumentSemanticTargetSyntax::Reference => 4,
        },
        source_range: range(&target.source_range),
        source_utf16_range: range(&target.source_utf16_range),
        content_range: range(&target.content_range),
        content_utf16_range: range(&target.content_utf16_range),
        destination_source_range: range(&target.destination_source_range),
        destination_source_utf16_range: range(&target.destination_source_utf16_range),
        title_source_range: target.title_source_range.as_ref().map_or(empty, range),
        title_source_utf16_range: target
            .title_source_utf16_range
            .as_ref()
            .map_or(empty, range),
        destination_bytes: u32::try_from(target.destination.len())
            .map_err(|_| StatusCode::ResultCapReached)?,
        title_bytes: u32::try_from(target.title.as_ref().map_or(0, String::len))
            .map_err(|_| StatusCode::ResultCapReached)?,
        reserved: [0; 2],
    })
}

fn certification_range_record(span: &DocumentLiveViewportSpan) -> CertificationRangeRecord {
    let (certification_state, source_range, source_utf16_range) = match span {
        DocumentLiveViewportSpan::Pending {
            source_range,
            source_utf16_range,
        } => (
            CertificationState::PendingNeutral,
            source_range,
            source_utf16_range,
        ),
        DocumentLiveViewportSpan::CertifiedUnchanged {
            source_range,
            source_utf16_range,
        } => (
            CertificationState::CurrentCertified,
            source_range,
            source_utf16_range,
        ),
    };
    CertificationRangeRecord {
        certification_state: certification_state as u32,
        reserved: 0,
        source_range: SourceRange {
            start_byte: source_range.start,
            end_byte: source_range.end,
        },
        source_utf16_range: SourceRange {
            start_byte: source_utf16_range.start,
            end_byte: source_utf16_range.end,
        },
    }
}

fn viewport_record(
    row: &flark_runtime::DocumentViewportRow,
    inline_authoritative: bool,
    inline_fact_count: u32,
    projection_segment_count: u32,
) -> ViewportRowRecord {
    let projected_authoritative = projection_segment_count > 1;
    let has_editable = row.edit_capability == DocumentViewportRowEditCapability::Contiguous
        || (row.edit_capability == DocumentViewportRowEditCapability::ProjectedReserved
            && projected_authoritative);
    let (editable_start_byte, editable_end_byte) = if has_editable {
        row.editable_range
            .as_ref()
            .map_or((u64::MAX, u64::MAX), |range| (range.start, range.end))
    } else {
        (u64::MAX, u64::MAX)
    };
    let (editable_start_utf16, editable_end_utf16) = if has_editable {
        row.editable_utf16_range
            .as_ref()
            .map_or((u64::MAX, u64::MAX), |range| (range.start, range.end))
    } else {
        (u64::MAX, u64::MAX)
    };
    let mut flags = match row.edit_capability {
        DocumentViewportRowEditCapability::Contiguous => VIEWPORT_ROW_FLAG_CONTIGUOUS_EDIT,
        DocumentViewportRowEditCapability::ProjectedReserved if projected_authoritative => {
            VIEWPORT_ROW_FLAG_PROJECTED_RESERVED
        }
        DocumentViewportRowEditCapability::ProjectedReserved => VIEWPORT_ROW_FLAG_EDIT_UNAVAILABLE,
        DocumentViewportRowEditCapability::Unavailable => VIEWPORT_ROW_FLAG_EDIT_UNAVAILABLE,
    };
    if inline_authoritative {
        flags |= VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE;
    }
    let (
        presentation_prefix_start_byte,
        presentation_prefix_end_byte,
        presentation_prefix_start_utf16,
        presentation_prefix_end_utf16,
        semantic_variant,
        semantic_value,
    ) = match row.presentation {
        DocumentViewportRowPresentation::Plain => (u64::MAX, u64::MAX, u64::MAX, u64::MAX, 0, 0),
        DocumentViewportRowPresentation::Heading { level, style } => (
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u32::from(level) & VIEWPORT_ROW_HEADING_LEVEL_MASK
                | if style == DocumentHeadingStyle::Setext {
                    VIEWPORT_ROW_HEADING_SETEXT
                } else {
                    0
                },
            0,
        ),
        DocumentViewportRowPresentation::ListItem {
            marker,
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            marker_offset,
            marker_column,
            simple_continuation,
            starts_list,
            task_checked,
            ..
        } => {
            let (marker_variant, marker_value) = match marker {
                DocumentListMarker::Bullet(marker) => (
                    match marker {
                        DocumentBulletMarker::Hyphen => VIEWPORT_ROW_LIST_HYPHEN,
                        DocumentBulletMarker::Plus => VIEWPORT_ROW_LIST_PLUS,
                        DocumentBulletMarker::Asterisk => VIEWPORT_ROW_LIST_ASTERISK,
                    },
                    0,
                ),
                DocumentListMarker::Ordered { value, delimiter } => (
                    match delimiter {
                        DocumentListDelimiter::Period => VIEWPORT_ROW_LIST_ORDERED_PERIOD,
                        DocumentListDelimiter::Parenthesis => VIEWPORT_ROW_LIST_ORDERED_PARENTHESIS,
                    },
                    value,
                ),
            };
            (
                prefix_start_byte,
                prefix_end_byte,
                prefix_start_utf16,
                prefix_end_utf16,
                marker_variant
                    | u32::from(nesting_depth) << VIEWPORT_ROW_LIST_DEPTH_SHIFT
                    | u32::from(marker_offset) << VIEWPORT_ROW_LIST_MARKER_OFFSET_SHIFT
                    | u32::from(marker_column) << VIEWPORT_ROW_LIST_MARKER_COLUMN_SHIFT
                    | if simple_continuation {
                        VIEWPORT_ROW_LIST_SIMPLE_CONTINUATION
                    } else {
                        0
                    }
                    | if starts_list {
                        VIEWPORT_ROW_LIST_STARTS_LIST
                    } else {
                        0
                    }
                    | if task_checked.is_some() {
                        VIEWPORT_ROW_LIST_TASK
                    } else {
                        0
                    }
                    | if task_checked == Some(true) {
                        VIEWPORT_ROW_LIST_TASK_CHECKED
                    } else {
                        0
                    },
                marker_value,
            )
        }
        DocumentViewportRowPresentation::BlockQuote {
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            nesting_depth,
            simple_continuation,
            ..
        } => (
            prefix_start_byte,
            prefix_end_byte,
            prefix_start_utf16,
            prefix_end_utf16,
            VIEWPORT_ROW_BLOCK_QUOTE_PRESENTATION
                | u32::from(nesting_depth) << VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_SHIFT
                | if simple_continuation {
                    VIEWPORT_ROW_BLOCK_QUOTE_SIMPLE_CONTINUATION
                } else {
                    0
                },
            0,
        ),
        DocumentViewportRowPresentation::CodeBlock { style } => {
            let (semantic_variant, semantic_value) = match style {
                DocumentCodeBlockStyle::Indented => (VIEWPORT_ROW_CODE_PRESENTATION, 0),
                DocumentCodeBlockStyle::Fenced {
                    fence,
                    minimum_closing_length,
                    fence_offset,
                    closed,
                } => (
                    VIEWPORT_ROW_CODE_PRESENTATION
                        | VIEWPORT_ROW_CODE_FENCED
                        | if fence == DocumentFenceCharacter::Tilde {
                            VIEWPORT_ROW_CODE_TILDE
                        } else {
                            0
                        }
                        | if closed { VIEWPORT_ROW_CODE_CLOSED } else { 0 }
                        | u32::from(fence_offset) << VIEWPORT_ROW_CODE_FENCE_OFFSET_SHIFT,
                    minimum_closing_length,
                ),
            };
            (
                u64::MAX,
                u64::MAX,
                u64::MAX,
                u64::MAX,
                semantic_variant,
                semantic_value,
            )
        }
        DocumentViewportRowPresentation::ThematicBreak => (
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            VIEWPORT_ROW_THEMATIC_BREAK_PRESENTATION,
            0,
        ),
        DocumentViewportRowPresentation::Table => (
            u64::MAX,
            u64::MAX,
            u64::MAX,
            u64::MAX,
            if inline_authoritative {
                VIEWPORT_ROW_TABLE_PRESENTATION
            } else {
                0
            },
            0,
        ),
    };
    ViewportRowRecord {
        ordinal: row.ordinal,
        kind: u32::from(row.kind),
        flags,
        source_start_byte: row.source_range.start,
        source_end_byte: row.source_range.end,
        source_start_utf16: row.source_utf16_range.start,
        source_end_utf16: row.source_utf16_range.end,
        editable_start_byte,
        editable_end_byte,
        editable_start_utf16,
        editable_end_utf16,
        presentation_prefix_start_byte,
        presentation_prefix_end_byte,
        presentation_prefix_start_utf16,
        presentation_prefix_end_utf16,
        path_depth: row.path_depth,
        semantic_variant,
        semantic_value,
        inline_fact_count: (inline_fact_count & VIEWPORT_ROW_INLINE_FACT_COUNT_MASK)
            | projection_segment_count << VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT,
    }
}

fn projection_segment_record(segment: &DocumentProjectionSegment) -> ProjectionSegmentRecord {
    ProjectionSegmentRecord {
        source_range: SourceRange {
            start_byte: segment.source_range.start,
            end_byte: segment.source_range.end,
        },
        source_utf16_range: SourceRange {
            start_byte: segment.source_utf16_range.start,
            end_byte: segment.source_utf16_range.end,
        },
    }
}

fn inline_fact_record(fact: &DocumentInlineFact) -> InlineFactRecord {
    InlineFactRecord {
        kind: match fact.kind {
            DocumentInlineFactKind::Emphasis => INLINE_FACT_EMPHASIS,
            DocumentInlineFactKind::Strong => INLINE_FACT_STRONG,
            DocumentInlineFactKind::Code => INLINE_FACT_CODE,
            DocumentInlineFactKind::Strikethrough => INLINE_FACT_STRIKETHROUGH,
            DocumentInlineFactKind::AutolinkUri => INLINE_FACT_AUTOLINK_URI,
            DocumentInlineFactKind::AutolinkEmail => INLINE_FACT_AUTOLINK_EMAIL,
            DocumentInlineFactKind::BackslashEscape => INLINE_FACT_BACKSLASH_ESCAPE,
            DocumentInlineFactKind::HardLineBreak => INLINE_FACT_HARD_LINE_BREAK,
            DocumentInlineFactKind::Replacement => INLINE_FACT_REPLACEMENT,
            DocumentInlineFactKind::DirectLink => INLINE_FACT_DIRECT_LINK,
            DocumentInlineFactKind::DirectImage => INLINE_FACT_DIRECT_IMAGE,
            DocumentInlineFactKind::ReferenceLink => INLINE_FACT_REFERENCE_LINK,
            DocumentInlineFactKind::ReferenceImage => INLINE_FACT_REFERENCE_IMAGE,
            DocumentInlineFactKind::TableCell => INLINE_FACT_TABLE_CELL,
        },
        flags: u32::from(fact.flags),
        source_start_byte: fact.source_range.start,
        source_end_byte: fact.source_range.end,
        source_start_utf16: fact.source_utf16_range.start,
        source_end_utf16: fact.source_utf16_range.end,
        content_start_byte: fact.content_range.start,
        content_end_byte: fact.content_range.end,
        content_start_utf16: fact.content_utf16_range.start,
        content_end_utf16: fact.content_utf16_range.end,
        replacement_first: fact.replacement.map_or(0, |value| u32::from(value.first)),
        replacement_second: fact
            .replacement
            .and_then(|value| value.second)
            .map_or(0, u32::from),
    }
}

fn literal_safe_envelope_record(envelope: &DocumentLiteralSafeEnvelope) -> InlineFactRecord {
    InlineFactRecord {
        kind: INLINE_FACT_LITERAL_SAFE_ENVELOPE,
        flags: match envelope.edit_class {
            DocumentLiteralEditClass::AsciiWordInsertion => LITERAL_EDIT_CLASS_ASCII_WORD_INSERTION,
            DocumentLiteralEditClass::SingleAsciiSpaceInsertion => {
                LITERAL_EDIT_CLASS_SINGLE_ASCII_SPACE_INSERTION
            }
        },
        source_start_byte: envelope.source_range.start,
        source_end_byte: envelope.source_range.end,
        source_start_utf16: envelope.source_utf16_range.start,
        source_end_utf16: envelope.source_utf16_range.end,
        ..InlineFactRecord::default()
    }
}

fn projection_edit_cell_record(cell: &DocumentProjectionEditCell) -> InlineFactRecord {
    InlineFactRecord {
        kind: INLINE_FACT_PROJECTION_EDIT_CELL,
        flags: cell.flags,
        source_start_byte: cell.source_range.start,
        source_end_byte: cell.source_range.end,
        source_start_utf16: cell.source_utf16_range.start,
        source_end_utf16: cell.source_utf16_range.end,
        content_start_byte: cell.trigger_range.start,
        content_end_byte: cell.trigger_range.end,
        content_start_utf16: cell.trigger_utf16_range.start,
        content_end_utf16: cell.trigger_utf16_range.end,
        ..InlineFactRecord::default()
    }
}

fn write_page<F>(
    output: *mut u8,
    receipt: ResultPageReceipt,
    payload_writer: F,
) -> Result<(), StatusCode>
where
    F: FnOnce(&mut [u8]),
{
    let header = ResultPageHeader::from_runtime(receipt).ok_or(StatusCode::InternalFault)?;
    unsafe {
        ptr::write_unaligned(output.cast::<ResultPageHeader>(), header);
        let payload = slice::from_raw_parts_mut(
            output.add(size_of::<ResultPageHeader>()),
            receipt.payload_bytes as usize,
        );
        payload_writer(payload);
    }
    Ok(())
}

fn write_row_records(output: &mut [u8], rows: &[ViewportRowRecord]) {
    debug_assert_eq!(output.len(), rows.len() * size_of::<ViewportRowRecord>());
    for (index, row) in rows.iter().copied().enumerate() {
        unsafe {
            ptr::write_unaligned(
                output
                    .as_mut_ptr()
                    .add(index * size_of::<ViewportRowRecord>())
                    .cast::<ViewportRowRecord>(),
                row,
            );
        }
    }
}

fn write_inline_fact_records(output: &mut [u8], records: &[InlineFactRecord]) {
    debug_assert_eq!(output.len(), records.len() * size_of::<InlineFactRecord>());
    for (index, record) in records.iter().copied().enumerate() {
        unsafe {
            ptr::write_unaligned(
                output
                    .as_mut_ptr()
                    .add(index * size_of::<InlineFactRecord>())
                    .cast::<InlineFactRecord>(),
                record,
            );
        }
    }
}

fn write_projection_segment_records(output: &mut [u8], records: &[ProjectionSegmentRecord]) {
    debug_assert_eq!(
        output.len(),
        records.len() * size_of::<ProjectionSegmentRecord>()
    );
    for (index, record) in records.iter().copied().enumerate() {
        unsafe {
            ptr::write_unaligned(
                output
                    .as_mut_ptr()
                    .add(index * size_of::<ProjectionSegmentRecord>())
                    .cast::<ProjectionSegmentRecord>(),
                record,
            );
        }
    }
}

fn write_certification_range_records(output: &mut [u8], records: &[CertificationRangeRecord]) {
    debug_assert_eq!(
        output.len(),
        records.len() * size_of::<CertificationRangeRecord>()
    );
    for (index, record) in records.iter().copied().enumerate() {
        unsafe {
            ptr::write_unaligned(
                output
                    .as_mut_ptr()
                    .add(index * size_of::<CertificationRangeRecord>())
                    .cast::<CertificationRangeRecord>(),
                record,
            );
        }
    }
}

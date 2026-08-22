use std::mem::size_of;

use flark_abi::{
    flark_v4_anchor_create, flark_v4_anchor_release, flark_v4_anchor_resolve,
    flark_v4_anchor_transform, flark_v4_bulk_abort, flark_v4_bulk_append, flark_v4_bulk_begin,
    flark_v4_cancel, flark_v4_close_begin, flark_v4_close_finish, flark_v4_close_pump,
    flark_v4_create_abort, flark_v4_create_begin, flark_v4_create_commit, flark_v4_edit_intent_v1,
    flark_v4_history_release, flark_v4_history_replay, flark_v4_pump, flark_v4_query_viewport,
    flark_v4_session_inspect, flark_v4_session_transfer_owner, flark_v4_small_edit,
    flark_v4_source_transaction_v1, flark_v4_staged_source_transaction_v1, AnchorRequest,
    BulkBeginRequest, CancelRequest, CloseRequest, CreateRequest, EditDescriptor,
    EditIntentReceiptV1, EditIntentRequestV1, InspectRequest, Outcome, OwnerTransferRequest,
    PumpRequest, QueryRequest, ResultPageHeader, SessionConfig, SessionInspection, SessionRef,
    SmallEditRequest, SourceRange, SourceReadRequest, SourceTransactionReceiptV1,
    SourceTransactionRequestV1, StageRequest, StagedSourceTransactionRequestV1, TransactionRequest,
    WorkBudget, EDIT_INTENT_DISPOSITION_APPLIED, EDIT_INTENT_INDENT_LIST_ITEM,
    EDIT_INTENT_INSERT_PARAGRAPH_BREAK, EDIT_INTENT_OUTDENT_LIST_ITEM,
    EDIT_INTENT_RECEIPT_HAS_COMMIT, EDIT_INTENT_RECEIPT_PRESENTATION_PROVEN,
    EDIT_INTENT_RECEIPT_SEMANTIC_BYTES, EDIT_INTENT_TOGGLE_TASK_CHECKED,
    EDIT_PRESENTATION_CONTINUE_LIST, EDIT_PRESENTATION_EXIT_LIST, EDIT_PRESENTATION_INDENT_LIST,
    EDIT_PRESENTATION_OUTDENT_LIST, EDIT_PRESENTATION_SPLIT_PARAGRAPH,
    EDIT_PRESENTATION_TOGGLE_TASK_CHECKED, EDIT_PROFILE_FLARK_V1,
    SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES, SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT,
    SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES,
};
use flark_runtime::{HistoryDisposition, SessionState, StatusCode, MAX_LIVE_ANCHORS};

fn budget(work: u64) -> WorkBudget {
    WorkBudget {
        max_work_units: work,
        advisory_max_micros: 0,
        max_result_items: 256,
        max_result_bytes: 64 * 1024,
    }
}

fn open_session(source: &[u8], owner: u64) -> SessionRef {
    open_session_with_history(source, owner, 8 * 1024)
}

fn open_session_with_history(source: &[u8], owner: u64, history_budget_bytes: u64) -> SessionRef {
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: 0,
        owner_token: owner,
        expected_total_bytes: source.len() as u64,
        config: SessionConfig {
            struct_size: size_of::<SessionConfig>() as u32,
            parser_profile: 2,
            history_budget_bytes,
            max_document_bytes: 16 * 1024 * 1024,
            flags: 0,
            reserved: [0; 4],
        },
    };
    let mut outcome = Outcome::default();
    let status = flark_v4_create_begin(&create, source.as_ptr(), source.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: owner,
    };
    let commit = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction: outcome.secondary_handle,
        expected_revision: 0,
        progress_token: 0,
        budget: budget(8),
        reserved: [0; 1],
    };
    let mut status = flark_v4_create_commit(&commit, &mut outcome);
    let mut token = outcome.progress_token;
    while status == StatusCode::BudgetExhausted as u32 {
        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session,
            expected_revision: 1,
            progress_token: token,
            budget: budget(64),
        };
        status = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;
    }
    assert_eq!(status, StatusCode::Ok as u32);
    session
}

fn pump_to_ready(session: SessionRef, revision: u64) {
    let mut outcome = Outcome::default();
    let mut token = 0;
    loop {
        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session,
            expected_revision: revision,
            progress_token: token,
            budget: budget(64),
        };
        let status = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;
        if status != StatusCode::BudgetExhausted as u32 {
            assert_eq!(status, StatusCode::Ok as u32);
            return;
        }
    }
}

fn small_edit(
    session: SessionRef,
    expected_revision: u64,
    start: u64,
    end: u64,
    replacement: &[u8],
) -> Outcome {
    let descriptor = EditDescriptor {
        start_byte: start,
        end_byte: end,
        replacement_offset: 0,
        replacement_len: replacement.len() as u64,
    };
    let edit = SmallEditRequest {
        struct_size: size_of::<SmallEditRequest>() as u32,
        flags: 0,
        session,
        expected_revision,
        edit_count: 1,
        reserved_u32: 0,
        replacement_bytes_len: replacement.len() as u64,
        budget: budget(1),
        reserved: [0; 2],
    };
    let mut outcome = Outcome::default();
    let status = flark_v4_small_edit(
        &edit,
        &descriptor,
        1,
        replacement.as_ptr(),
        replacement.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    outcome
}

fn anchor_request(session: SessionRef) -> AnchorRequest {
    AnchorRequest {
        struct_size: size_of::<AnchorRequest>() as u32,
        coordinate_kind: 0,
        session,
        revision: 0,
        snapshot: 0,
        anchor: 0,
        position: 0,
        affinity: 0,
        reserved_u32: 0,
        progress_token: 0,
        budget: budget(1),
    }
}

fn create_anchor(
    session: SessionRef,
    revision: u64,
    coordinate_kind: u32,
    position: u64,
    affinity: u32,
) -> u64 {
    let request = AnchorRequest {
        coordinate_kind,
        revision,
        position,
        affinity,
        ..anchor_request(session)
    };
    let mut outcome = Outcome::default();
    let status = flark_v4_anchor_create(&request, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    assert_ne!(outcome.primary_handle, 0);
    assert_eq!(outcome.revision, revision);
    outcome.primary_handle
}

fn resolve_anchor(session: SessionRef, anchor: u64, revision: u64, coordinate_kind: u32) -> u64 {
    let request = AnchorRequest {
        coordinate_kind,
        revision,
        anchor,
        ..anchor_request(session)
    };
    let mut outcome = Outcome::default();
    let status = flark_v4_anchor_resolve(&request, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    assert_eq!(outcome.primary_handle, anchor);
    assert_eq!(outcome.revision, revision);
    outcome.detail_code
}

fn close_session(session: SessionRef) {
    let mut outcome = Outcome::default();
    let mut close = CloseRequest {
        struct_size: size_of::<CloseRequest>() as u32,
        flags: 0,
        session,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    let mut status = flark_v4_close_begin(&close, &mut outcome);
    close.progress_token = outcome.progress_token;
    let mut turns = 0;
    while status == StatusCode::BudgetExhausted as u32 {
        status = flark_v4_close_pump(&close, &mut outcome);
        close.progress_token = outcome.progress_token;
        turns += 1;
        assert!(turns < 10_000, "bounded close should converge");
    }
    assert_eq!(status, StatusCode::Ok as u32);
    status = flark_v4_close_finish(&close, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
}

const SOURCE_BYTE: u32 = 1;
const UTF16: u32 = 2;
const UPSTREAM: u32 = 1;
const DOWNSTREAM: u32 = 2;

fn edit_intent_request(
    session: SessionRef,
    revision: u64,
    anchor: u64,
    logical_edit_id: u64,
    request_digest: u64,
) -> EditIntentRequestV1 {
    EditIntentRequestV1 {
        struct_size: size_of::<EditIntentRequestV1>() as u32,
        profile_id: EDIT_PROFILE_FLARK_V1,
        session,
        expected_revision: revision,
        selection_base_anchor: anchor,
        selection_extent_anchor: anchor,
        logical_edit_id,
        request_digest,
        acknowledge_previous_logical_edit_id: 0,
        selection_generation: logical_edit_id,
        intent: EDIT_INTENT_INSERT_PARAGRAPH_BREAK,
        selection_affinity: DOWNSTREAM,
        selection_direction: 0,
        composition_active: 0,
        budget: budget(1),
        target_anchor: 0,
    }
}

fn read_source(session: SessionRef, revision: u64, length: usize) -> Vec<u8> {
    let request = SourceReadRequest {
        struct_size: size_of::<SourceReadRequest>() as u32,
        flags: 0,
        session,
        revision,
        range: SourceRange {
            start_byte: 0,
            end_byte: length as u64,
        },
        reserved: [0; 2],
    };
    let mut output = vec![0_u8; size_of::<ResultPageHeader>() + length];
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_abi::flark_v4_source_read(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    output[size_of::<ResultPageHeader>()..].to_vec()
}

#[test]
fn semantic_edit_is_one_commit_with_required_history_and_recoverable_terminal() {
    let source = b"- one\n- two\n";
    let session = open_session(source, 451);
    let anchor = create_anchor(session, 1, SOURCE_BYTE, 5, DOWNSTREAM);
    let mut request = edit_intent_request(session, 1, anchor, 1, 0xA11CE);
    let mut output = vec![0_u8; size_of::<EditIntentReceiptV1>() + 4096];
    let mut outcome = Outcome::default();

    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let first = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_eq!(first.semantic_disposition, EDIT_INTENT_DISPOSITION_APPLIED);
    assert_eq!(
        first.flags & (EDIT_INTENT_RECEIPT_HAS_COMMIT | EDIT_INTENT_RECEIPT_SEMANTIC_BYTES),
        EDIT_INTENT_RECEIPT_HAS_COMMIT | EDIT_INTENT_RECEIPT_SEMANTIC_BYTES
    );
    assert_eq!(first.base_revision, 1);
    assert_eq!(first.result_revision, 2);
    assert_eq!(
        first.base_byte_range,
        SourceRange {
            start_byte: 5,
            end_byte: 5
        }
    );
    assert_eq!(first.result_selection_utf16, 8);
    assert_ne!(first.history_token, 0);
    assert_eq!(outcome.primary_handle, first.history_token);
    assert_eq!(outcome.detail_code, HistoryDisposition::Retained as u64);
    assert_eq!(first.replacement_bytes, 3);
    assert_eq!(
        first.presentation_transition,
        EDIT_PRESENTATION_CONTINUE_LIST
    );
    assert_eq!(
        &output[size_of::<EditIntentReceiptV1>()..size_of::<EditIntentReceiptV1>() + 3],
        b"\n- "
    );
    let after_first = b"- one\n- \n- two\n";
    assert_eq!(read_source(session, 2, after_first.len()), after_first);
    assert_eq!(resolve_anchor(session, anchor, 2, SOURCE_BYTE), 8);

    // A lost caller reply retries the same logical ID and digest. Native
    // returns the retained terminal receipt without replaying the source edit.
    output.fill(0);
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let recovered = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_eq!(recovered, first);
    assert_eq!(read_source(session, 2, after_first.len()), after_first);

    // A different command cannot pass the unacknowledged terminal.
    request.expected_revision = 2;
    request.logical_edit_id = 2;
    request.request_digest = 0xB0B;
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Backpressure as u32
    );

    // Acknowledging it admits the next command. The empty list row exits the
    // list with one source commit and a second required history token.
    request.acknowledge_previous_logical_edit_id = 1;
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let second = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_eq!(second.result_revision, 3);
    assert_eq!(second.presentation_transition, EDIT_PRESENTATION_EXIT_LIST);
    assert_ne!(second.history_token, first.history_token);
    let after_second = b"- one\n\n\n- two\n";
    assert_eq!(read_source(session, 3, after_second.len()), after_second);

    // During H1 the ordered legacy literal lane implicitly acknowledges the
    // terminal at its result revision. A later duplicate therefore cannot
    // recover and replay an already superseded receipt.
    small_edit(
        session,
        3,
        after_second.len() as u64,
        after_second.len() as u64,
        b"x",
    );
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::InvalidArgument as u32
    );
    assert_eq!(
        read_source(session, 4, after_second.len() + 1),
        b"- one\n\n\n- two\nx"
    );
    close_session(session);
}

#[test]
fn ready_terminal_paragraph_split_carries_parser_presentation_proof() {
    let source = b"Before **bold**.\n";
    let session = open_session(source, 458);
    pump_to_ready(session, 1);
    let anchor = create_anchor(session, 1, SOURCE_BYTE, 16, DOWNSTREAM);
    let request = edit_intent_request(session, 1, anchor, 1, 0x51A17);
    let mut output = vec![0_u8; size_of::<EditIntentReceiptV1>() + 4096];
    let mut outcome = Outcome::default();

    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let receipt = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_ne!(receipt.flags & EDIT_INTENT_RECEIPT_PRESENTATION_PROVEN, 0);
    assert_eq!(
        receipt.presentation_transition,
        EDIT_PRESENTATION_SPLIT_PARAGRAPH
    );
    close_session(session);
}

#[test]
fn list_indent_and_outdent_are_receipted_anchor_stable_transactions() {
    let initial = b"- parent\n- child\n";
    let session = open_session(initial, 454);
    pump_to_ready(session, 1);
    let selection = create_anchor(session, 1, SOURCE_BYTE, 16, DOWNSTREAM);
    let mut request = edit_intent_request(session, 1, selection, 1, 0x1ADE17);
    request.intent = EDIT_INTENT_INDENT_LIST_ITEM;
    let mut output = vec![0_u8; size_of::<EditIntentReceiptV1>() + 4096];
    let mut outcome = Outcome::default();

    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let indented = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_eq!(
        indented.semantic_disposition,
        EDIT_INTENT_DISPOSITION_APPLIED
    );
    assert_eq!(
        indented.presentation_transition,
        EDIT_PRESENTATION_INDENT_LIST
    );
    assert_eq!(
        indented.base_byte_range,
        SourceRange {
            start_byte: 9,
            end_byte: 9
        }
    );
    assert_eq!(indented.result_selection_utf16, 18);
    assert_eq!(
        &output[size_of::<EditIntentReceiptV1>()..size_of::<EditIntentReceiptV1>() + 2],
        b"  "
    );
    assert_eq!(resolve_anchor(session, selection, 2, SOURCE_BYTE), 18);
    assert_eq!(
        read_source(session, 2, initial.len() + 2),
        b"- parent\n  - child\n"
    );

    output.fill(0);
    request.expected_revision = 2;
    request.logical_edit_id = 2;
    request.request_digest = 0x0A7DE17;
    request.acknowledge_previous_logical_edit_id = 1;
    request.selection_generation = 2;
    request.intent = EDIT_INTENT_OUTDENT_LIST_ITEM;
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let outdented = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_eq!(
        outdented.semantic_disposition,
        EDIT_INTENT_DISPOSITION_APPLIED
    );
    assert_eq!(
        outdented.presentation_transition,
        EDIT_PRESENTATION_OUTDENT_LIST
    );
    assert_eq!(
        outdented.base_byte_range,
        SourceRange {
            start_byte: 9,
            end_byte: 11
        }
    );
    assert_eq!(outdented.result_selection_utf16, 16);
    assert_eq!(resolve_anchor(session, selection, 3, SOURCE_BYTE), 16);
    assert_eq!(read_source(session, 3, initial.len()), initial);
    close_session(session);
}

#[test]
fn semantic_action_targets_a_task_and_preserves_a_range_selection() {
    let source = b"- [ ] task\n\nselection\n";
    let session = open_session(source, 453);
    let base = create_anchor(session, 1, SOURCE_BYTE, 12, DOWNSTREAM);
    let extent = create_anchor(session, 1, SOURCE_BYTE, 21, UPSTREAM);
    let target = create_anchor(session, 1, SOURCE_BYTE, 6, DOWNSTREAM);
    let mut request = edit_intent_request(session, 1, base, 1, 0xAC710);
    request.intent = EDIT_INTENT_TOGGLE_TASK_CHECKED;
    request.selection_extent_anchor = extent;
    request.target_anchor = target;
    let mut output = vec![0_u8; size_of::<EditIntentReceiptV1>() + 4096];
    let mut outcome = Outcome::default();

    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let receipt = unsafe {
        output
            .as_ptr()
            .cast::<EditIntentReceiptV1>()
            .read_unaligned()
    };
    assert_eq!(
        receipt.semantic_disposition,
        EDIT_INTENT_DISPOSITION_APPLIED
    );
    assert_eq!(
        receipt.presentation_transition,
        EDIT_PRESENTATION_TOGGLE_TASK_CHECKED
    );
    assert_eq!(
        receipt.base_byte_range,
        SourceRange {
            start_byte: 3,
            end_byte: 4
        }
    );
    assert_eq!(receipt.result_selection_utf16, 12);
    assert_ne!(receipt.history_token, 0);
    assert_eq!(
        &output[size_of::<EditIntentReceiptV1>()..size_of::<EditIntentReceiptV1>() + 1],
        b"x"
    );
    assert_eq!(resolve_anchor(session, base, 2, SOURCE_BYTE), 12);
    assert_eq!(resolve_anchor(session, extent, 2, SOURCE_BYTE), 21);
    assert_eq!(resolve_anchor(session, target, 2, SOURCE_BYTE), 6);
    assert_eq!(
        read_source(session, 2, source.len()),
        b"- [x] task\n\nselection\n"
    );
    close_session(session);
}

#[test]
fn semantic_edit_transforms_the_maximum_live_anchor_set() {
    let session = open_session(b"- one\n", 452);
    let selection = create_anchor(session, 1, SOURCE_BYTE, 5, DOWNSTREAM);
    let mut anchors = Vec::with_capacity(MAX_LIVE_ANCHORS as usize);
    anchors.push(selection);
    for _ in 1..MAX_LIVE_ANCHORS {
        anchors.push(create_anchor(session, 1, SOURCE_BYTE, 5, DOWNSTREAM));
    }

    let overflow = AnchorRequest {
        coordinate_kind: SOURCE_BYTE,
        revision: 1,
        position: 5,
        affinity: DOWNSTREAM,
        ..anchor_request(session)
    };
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_v4_anchor_create(&overflow, &mut outcome),
        StatusCode::ResourceLimitExceeded as u32
    );

    let request = edit_intent_request(session, 1, selection, 1, 0xA11CF);
    let mut output = vec![0_u8; size_of::<EditIntentReceiptV1>() + 4096];
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    assert_eq!(resolve_anchor(session, anchors[0], 2, SOURCE_BYTE), 8);
    assert_eq!(
        resolve_anchor(session, *anchors.last().unwrap(), 2, SOURCE_BYTE),
        8
    );
    close_session(session);
}

#[test]
fn semantic_edit_rejects_before_commit_without_required_history_headroom() {
    let source = b"- one\n";
    let session = open_session_with_history(source, 452, 0);
    let anchor = create_anchor(session, 1, SOURCE_BYTE, 5, DOWNSTREAM);
    let request = edit_intent_request(session, 1, anchor, 1, 0xCAFE);
    let mut output = vec![0_u8; size_of::<EditIntentReceiptV1>() + 4096];
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_v4_edit_intent_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::ResourceLimitExceeded as u32
    );
    assert_eq!(read_source(session, 1, source.len()), source);
    assert_eq!(resolve_anchor(session, anchor, 1, SOURCE_BYTE), 5);
    close_session(session);
}

#[test]
fn anchors_stay_source_stable_through_edits_and_replay() {
    // "Hello 🌍 world!\n": the earth emoji is 4 UTF-8 bytes / 2 UTF-16 units,
    // so byte and UTF-16 coordinates diverge after byte 6.
    let source = "Hello \u{1F30D} world!\n".as_bytes();
    let session = open_session(source, 501);
    let mut outcome = Outcome::default();

    // Invalid creations mutate nothing and return typed statuses.
    let mid_scalar = AnchorRequest {
        coordinate_kind: SOURCE_BYTE,
        revision: 1,
        position: 8,
        affinity: UPSTREAM,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_create(&mid_scalar, &mut outcome),
        StatusCode::RangeNotScalarBoundary as u32
    );
    let mid_surrogate = AnchorRequest {
        coordinate_kind: UTF16,
        revision: 1,
        position: 7,
        affinity: UPSTREAM,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_create(&mid_surrogate, &mut outcome),
        StatusCode::RangeNotScalarBoundary as u32
    );
    let out_of_range = AnchorRequest {
        coordinate_kind: SOURCE_BYTE,
        revision: 1,
        position: 999,
        affinity: UPSTREAM,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_create(&out_of_range, &mut outcome),
        StatusCode::CoordinateOutOfRange as u32
    );
    let stale = AnchorRequest {
        coordinate_kind: SOURCE_BYTE,
        revision: 5,
        position: 0,
        affinity: UPSTREAM,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_create(&stale, &mut outcome),
        StatusCode::StaleRevision as u32
    );
    let bad_affinity = AnchorRequest {
        coordinate_kind: SOURCE_BYTE,
        revision: 1,
        position: 0,
        affinity: 3,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_create(&bad_affinity, &mut outcome),
        StatusCode::InvalidArgument as u32
    );

    // The same position through both coordinate kinds: byte 11 == UTF-16 9.
    let by_byte = create_anchor(session, 1, SOURCE_BYTE, 11, DOWNSTREAM);
    let by_utf16 = create_anchor(session, 1, UTF16, 9, UPSTREAM);
    assert_eq!(resolve_anchor(session, by_byte, 1, SOURCE_BYTE), 11);
    assert_eq!(resolve_anchor(session, by_byte, 1, UTF16), 9);
    assert_eq!(resolve_anchor(session, by_utf16, 1, SOURCE_BYTE), 11);

    // An insertion before both anchors shifts them eagerly.
    assert_eq!(small_edit(session, 1, 6, 6, b"big ").revision, 2);
    assert_eq!(resolve_anchor(session, by_byte, 2, SOURCE_BYTE), 15);
    assert_eq!(resolve_anchor(session, by_byte, 2, UTF16), 13);
    assert_eq!(resolve_anchor(session, by_utf16, 2, SOURCE_BYTE), 15);

    // An insertion exactly at an anchor follows its affinity.
    let stay = create_anchor(session, 2, SOURCE_BYTE, 6, UPSTREAM);
    let follow = create_anchor(session, 2, SOURCE_BYTE, 6, DOWNSTREAM);
    assert_eq!(small_edit(session, 2, 6, 6, b"!").revision, 3);
    assert_eq!(resolve_anchor(session, stay, 3, SOURCE_BYTE), 6);
    assert_eq!(resolve_anchor(session, follow, 3, SOURCE_BYTE), 7);
    assert_eq!(resolve_anchor(session, by_byte, 3, SOURCE_BYTE), 16);

    // Anchors inside a replaced span collapse to the edge their affinity
    // names, and the replay of that edit transforms them back through the
    // inverse splice.
    let interior_up = create_anchor(session, 3, SOURCE_BYTE, 2, UPSTREAM);
    let interior_down = create_anchor(session, 3, SOURCE_BYTE, 2, DOWNSTREAM);
    let replace = small_edit(session, 3, 0, 5, b"Hi");
    assert_eq!(replace.revision, 4);
    assert_eq!(replace.detail_code, HistoryDisposition::Retained as u64);
    let undo_token = replace.primary_handle;
    assert_eq!(resolve_anchor(session, interior_up, 4, SOURCE_BYTE), 0);
    assert_eq!(resolve_anchor(session, interior_down, 4, SOURCE_BYTE), 2);
    assert_eq!(resolve_anchor(session, by_byte, 4, SOURCE_BYTE), 13);

    let history = flark_abi::HistoryRequest {
        struct_size: size_of::<flark_abi::HistoryRequest>() as u32,
        flags: 0,
        session,
        expected_revision: 4,
        history_token: undo_token,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    assert_eq!(
        flark_v4_history_replay(&history, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(outcome.revision, 5);
    assert_eq!(resolve_anchor(session, interior_up, 5, SOURCE_BYTE), 0);
    assert_eq!(resolve_anchor(session, interior_down, 5, SOURCE_BYTE), 5);
    assert_eq!(resolve_anchor(session, by_byte, 5, SOURCE_BYTE), 16);
    let redo_token = outcome.primary_handle;

    // A live history token is a different handle kind for anchor operations.
    let wrong_kind = AnchorRequest {
        revision: 5,
        anchor: redo_token,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_transform(&wrong_kind, &mut outcome),
        StatusCode::WrongHandleKind as u32
    );

    let release_redo = flark_abi::HistoryRequest {
        expected_revision: 5,
        history_token: redo_token,
        ..history
    };
    assert_eq!(
        flark_v4_history_release(&release_redo, &mut outcome),
        StatusCode::Ok as u32
    );

    // Transform revalidates the always-current anchor at the named revision.
    let transform = AnchorRequest {
        revision: 5,
        anchor: by_byte,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_transform(&transform, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(outcome.primary_handle, by_byte);
    assert_eq!(outcome.revision, 5);
    let stale_transform = AnchorRequest {
        revision: 4,
        anchor: by_byte,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_transform(&stale_transform, &mut outcome),
        StatusCode::StaleRevision as u32
    );

    // A released anchor handle is no longer resolvable.
    let release = AnchorRequest {
        anchor: by_byte,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_release(&release, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(
        flark_v4_anchor_release(&release, &mut outcome),
        StatusCode::InvalidHandle as u32
    );
    let resolve_released = AnchorRequest {
        coordinate_kind: SOURCE_BYTE,
        revision: 5,
        anchor: by_byte,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_resolve(&resolve_released, &mut outcome),
        StatusCode::InvalidHandle as u32
    );

    // Close drains the remaining live anchors without explicit releases.
    close_session(session);
}

#[test]
fn cancel_names_only_the_current_progress_token() {
    let source = "paragraph one\n\n".repeat(512);
    let session = open_session(source.as_bytes(), 601);
    let mut outcome = Outcome::default();

    // Force an in-flight pump progress by reparsing after an edit with a
    // one-unit budget.
    assert_eq!(small_edit(session, 1, 0, 1, b"P").revision, 2);
    let pump = PumpRequest {
        struct_size: size_of::<PumpRequest>() as u32,
        flags: 0,
        session,
        expected_revision: 2,
        progress_token: 0,
        budget: budget(1),
    };
    assert_eq!(
        flark_v4_pump(&pump, &mut outcome),
        StatusCode::BudgetExhausted as u32
    );
    let live_token = outcome.progress_token;
    assert_ne!(live_token, 0);

    // A fresh pump cannot silently replace the live progress.
    assert_eq!(
        flark_v4_pump(&pump, &mut outcome),
        StatusCode::StaleProgressToken as u32
    );

    let mut cancel = CancelRequest {
        struct_size: size_of::<CancelRequest>() as u32,
        flags: 0,
        session,
        progress_token: 0,
        reserved: [0; 4],
    };
    assert_eq!(
        flark_v4_cancel(&cancel, &mut outcome),
        StatusCode::InvalidArgument as u32
    );
    cancel.progress_token = live_token + 999;
    assert_eq!(
        flark_v4_cancel(&cancel, &mut outcome),
        StatusCode::StaleProgressToken as u32
    );
    cancel.progress_token = live_token;
    assert_eq!(
        flark_v4_cancel(&cancel, &mut outcome),
        StatusCode::Cancelled as u32
    );
    assert_eq!(outcome.progress_token, live_token);
    assert_eq!(outcome.revision, 2);

    // After cancellation a zero-token pump starts fresh progress.
    let status = flark_v4_pump(&pump, &mut outcome);
    assert!(
        status == StatusCode::Ok as u32 || status == StatusCode::BudgetExhausted as u32,
        "fresh pump after cancel, got {status}"
    );
    if status == StatusCode::BudgetExhausted as u32 {
        let follow = CancelRequest {
            progress_token: outcome.progress_token,
            ..cancel
        };
        assert_eq!(
            flark_v4_cancel(&follow, &mut outcome),
            StatusCode::Cancelled as u32
        );
    }
    close_session(session);
}

#[test]
fn owner_transfer_requires_idle_and_carries_history_authority() {
    let source = "alpha beta gamma\n\n".repeat(64);
    let session = open_session(source.as_bytes(), 701);
    let mut outcome = Outcome::default();

    let edited = small_edit(session, 1, 0, 5, b"delta");
    assert_eq!(edited.revision, 2);
    let history_token = edited.primary_handle;
    assert_ne!(history_token, 0);
    pump_to_ready(session, 2);

    // A live bulk transaction blocks migration.
    let bulk = BulkBeginRequest {
        struct_size: size_of::<BulkBeginRequest>() as u32,
        flags: 0,
        session,
        expected_revision: 2,
        range: SourceRange {
            start_byte: 0,
            end_byte: 5,
        },
        expected_total_bytes: 5,
        reserved: [0; 2],
    };
    assert_eq!(
        flark_v4_bulk_begin(&bulk, &mut outcome),
        StatusCode::Ok as u32
    );
    let bulk_transaction = outcome.primary_handle;
    let transfer = OwnerTransferRequest {
        struct_size: size_of::<OwnerTransferRequest>() as u32,
        flags: 0,
        session,
        new_owner_token: 702,
        reserved: [0; 4],
    };
    assert_eq!(
        flark_v4_session_transfer_owner(&transfer, &mut outcome),
        StatusCode::MigrationWhileActive as u32
    );
    let abort = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction: bulk_transaction,
        expected_revision: 2,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    assert_eq!(
        flark_v4_bulk_abort(&abort, &mut outcome),
        StatusCode::Ok as u32
    );

    // A live continuation blocks migration.
    let mut page = vec![0_u8; 8 * 1024];
    let query = QueryRequest {
        struct_size: size_of::<QueryRequest>() as u32,
        query_kind: 1,
        session,
        revision: 2,
        snapshot: 0,
        range: SourceRange {
            start_byte: 0,
            end_byte: source.len() as u64,
        },
        continuation: 0,
        budget: WorkBudget {
            max_result_items: 1,
            max_result_bytes: 128,
            ..budget(64)
        },
        reserved: [0; 1],
    };
    let status =
        flark_v4_query_viewport(&query, page.as_mut_ptr(), page.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::ResultCapReached as u32);
    let header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_ne!(header.continuation, 0);
    assert_eq!(
        flark_v4_session_transfer_owner(&transfer, &mut outcome),
        StatusCode::MigrationWhileActive as u32
    );
    let release = flark_abi::ContinuationRequest {
        struct_size: size_of::<flark_abi::ContinuationRequest>() as u32,
        flags: 0,
        session,
        revision: header.revision,
        snapshot: header.snapshot,
        continuation: header.continuation,
        budget: query.budget,
        reserved: [0; 1],
    };
    assert_eq!(
        flark_abi::flark_v4_continuation_release(&release, &mut outcome),
        StatusCode::Ok as u32
    );

    let zero_owner = OwnerTransferRequest {
        new_owner_token: 0,
        ..transfer
    };
    assert_eq!(
        flark_v4_session_transfer_owner(&zero_owner, &mut outcome),
        StatusCode::InvalidArgument as u32
    );

    // Idle migration succeeds; the old owner loses authority and retained
    // history follows the new owner.
    assert_eq!(
        flark_v4_session_transfer_owner(&transfer, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(outcome.primary_handle, session.session);
    let stale_owner_pump = PumpRequest {
        struct_size: size_of::<PumpRequest>() as u32,
        flags: 0,
        session,
        expected_revision: 2,
        progress_token: 0,
        budget: budget(1),
    };
    assert_eq!(
        flark_v4_pump(&stale_owner_pump, &mut outcome),
        StatusCode::OwnerMismatch as u32
    );
    let new_session = SessionRef {
        session: session.session,
        owner_token: 702,
    };
    let release_history = flark_abi::HistoryRequest {
        struct_size: size_of::<flark_abi::HistoryRequest>() as u32,
        flags: 0,
        session: new_session,
        expected_revision: 2,
        history_token,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    assert_eq!(
        flark_v4_history_release(&release_history, &mut outcome),
        StatusCode::Ok as u32
    );
    close_session(new_session);
}

#[test]
fn create_abort_releases_the_provisional_session() {
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: 0,
        owner_token: 801,
        expected_total_bytes: 64,
        config: SessionConfig {
            struct_size: size_of::<SessionConfig>() as u32,
            parser_profile: 2,
            history_budget_bytes: 0,
            max_document_bytes: 1024,
            flags: 0,
            reserved: [0; 4],
        },
    };
    let mut outcome = Outcome::default();
    let chunk = b"partial";
    let status = flark_v4_create_begin(&create, chunk.as_ptr(), chunk.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: 801,
    };
    let transaction = outcome.secondary_handle;

    let mut abort = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction: transaction + 1,
        expected_revision: 0,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    assert_eq!(
        flark_v4_create_abort(&abort, &mut outcome),
        StatusCode::TransactionConflict as u32
    );
    abort.transaction = transaction;
    assert_eq!(
        flark_v4_create_abort(&abort, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(
        flark_v4_create_abort(&abort, &mut outcome),
        StatusCode::InvalidHandle as u32
    );

    // An open session rejects create-abort as already committed.
    let open = open_session(b"ready\n", 802);
    let committed_abort = TransactionRequest {
        session: open,
        transaction: 1,
        ..abort
    };
    assert_eq!(
        flark_v4_create_abort(&committed_abort, &mut outcome),
        StatusCode::TransactionAlreadyCommitted as u32
    );
    close_session(open);
}

#[test]
fn invalid_utf8_commit_preserves_the_abortable_transaction() {
    let bytes = [0xff, 0xfe];
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: 0,
        owner_token: 803,
        expected_total_bytes: bytes.len() as u64,
        config: SessionConfig {
            struct_size: size_of::<SessionConfig>() as u32,
            parser_profile: 2,
            history_budget_bytes: 0,
            max_document_bytes: 1024,
            flags: 0,
            reserved: [0; 4],
        },
    };
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_v4_create_begin(&create, bytes.as_ptr(), bytes.len() as u64, &mut outcome),
        StatusCode::Ok as u32
    );
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: create.owner_token,
    };
    let transaction = outcome.secondary_handle;
    let request = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction,
        expected_revision: 0,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    assert_eq!(
        flark_v4_create_commit(&request, &mut outcome),
        StatusCode::InvalidUtf8 as u32
    );
    assert_eq!(
        flark_v4_create_abort(&request, &mut outcome),
        StatusCode::Ok as u32
    );
}

#[test]
fn session_inspect_reports_state_and_live_handles() {
    let mut outcome = Outcome::default();
    let mut inspection = SessionInspection::default();

    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: 0,
        owner_token: 901,
        expected_total_bytes: 6,
        config: SessionConfig {
            struct_size: size_of::<SessionConfig>() as u32,
            parser_profile: 2,
            history_budget_bytes: 8 * 1024,
            max_document_bytes: 1024 * 1024,
            flags: 0,
            reserved: [0; 4],
        },
    };
    let chunk = b"ab\n";
    let status = flark_v4_create_begin(&create, chunk.as_ptr(), chunk.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: 901,
    };
    let transaction = outcome.secondary_handle;

    let inspect = InspectRequest {
        struct_size: size_of::<InspectRequest>() as u32,
        flags: 0,
        session,
        reserved: [0; 5],
    };
    assert_eq!(
        flark_v4_session_inspect(&inspect, &mut inspection, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(inspection.session_state, SessionState::Creating as u32);
    assert_eq!(inspection.revision, 0);
    assert_eq!(inspection.live_transactions, 1);
    assert_eq!(outcome.detail_code, SessionState::Creating as u64);

    // Null output is rejected before any session state is consumed.
    assert_eq!(
        flark_v4_session_inspect(&inspect, std::ptr::null_mut(), &mut outcome),
        StatusCode::InvalidArgument as u32
    );

    // Finish creation and accumulate one of each live handle kind.
    let append = flark_abi::StageRequest {
        struct_size: size_of::<flark_abi::StageRequest>() as u32,
        flags: 0,
        session,
        transaction,
        chunk_offset: 3,
        chunk_len: 3,
        reserved: [0; 2],
    };
    let tail = b"cd\n";
    assert_eq!(
        flark_abi::flark_v4_create_append(&append, tail.as_ptr(), tail.len() as u64, &mut outcome),
        StatusCode::Ok as u32
    );
    let commit = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction,
        expected_revision: 0,
        progress_token: 0,
        budget: budget(64),
        reserved: [0; 1],
    };
    let mut status = flark_v4_create_commit(&commit, &mut outcome);
    while status == StatusCode::BudgetExhausted as u32 {
        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session,
            expected_revision: 1,
            progress_token: outcome.progress_token,
            budget: budget(64),
        };
        status = flark_v4_pump(&pump, &mut outcome);
    }
    assert_eq!(status, StatusCode::Ok as u32);

    assert_eq!(
        flark_v4_session_inspect(&inspect, &mut inspection, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(inspection.session_state, SessionState::Open as u32);
    assert_eq!(inspection.revision, 1);
    assert_eq!(inspection.live_transactions, 0);
    assert_eq!(inspection.live_continuations, 0);
    assert_eq!(inspection.live_anchors, 0);
    assert_eq!(inspection.live_history_tokens, 0);

    let anchor_a = create_anchor(session, 1, SOURCE_BYTE, 0, UPSTREAM);
    let _anchor_b = create_anchor(session, 1, SOURCE_BYTE, 3, DOWNSTREAM);
    let edited = small_edit(session, 1, 0, 1, b"A");
    assert_ne!(edited.primary_handle, 0);
    assert_eq!(
        flark_v4_session_inspect(&inspect, &mut inspection, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(inspection.revision, 2);
    assert_eq!(inspection.live_anchors, 2);
    assert_eq!(inspection.live_history_tokens, 1);

    let release = AnchorRequest {
        anchor: anchor_a,
        ..anchor_request(session)
    };
    assert_eq!(
        flark_v4_anchor_release(&release, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(
        flark_v4_session_inspect(&inspect, &mut inspection, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(inspection.live_anchors, 1);

    // Closing is visible through inspection, close pump drains the remaining
    // anchor and history token, and a finished session is no longer
    // inspectable.
    let mut close = CloseRequest {
        struct_size: size_of::<CloseRequest>() as u32,
        flags: 0,
        session,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    let mut close_status = flark_v4_close_begin(&close, &mut outcome);
    close.progress_token = outcome.progress_token;
    if close_status == StatusCode::BudgetExhausted as u32 {
        assert_eq!(
            flark_v4_session_inspect(&inspect, &mut inspection, &mut outcome),
            StatusCode::Ok as u32
        );
        assert_eq!(inspection.session_state, SessionState::Closing as u32);
        let finish_early = flark_v4_close_finish(&close, &mut outcome);
        assert_eq!(finish_early, StatusCode::CloseIncomplete as u32);
        while close_status == StatusCode::BudgetExhausted as u32 {
            close_status = flark_v4_close_pump(&close, &mut outcome);
            close.progress_token = outcome.progress_token;
        }
    }
    assert_eq!(close_status, StatusCode::Ok as u32);
    assert_eq!(
        flark_v4_close_finish(&close, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(
        flark_v4_session_inspect(&inspect, &mut inspection, &mut outcome),
        StatusCode::InvalidHandle as u32
    );
}

#[test]
fn source_transaction_commits_history_and_retargets_selection_atomically() {
    const UTF16: u32 = 2;
    const DOWNSTREAM: u32 = 2;
    const UPSTREAM: u32 = 1;

    let initial = "a🌍bc\n";
    let session = open_session(initial.as_bytes(), 0x5151);
    let base = create_anchor(session, 1, UTF16, 1, DOWNSTREAM);
    let extent = create_anchor(session, 1, UTF16, 3, UPSTREAM);
    let replacement = "éx".as_bytes();
    let request = SourceTransactionRequestV1 {
        struct_size: size_of::<SourceTransactionRequestV1>() as u32,
        flags: 0,
        session,
        expected_revision: 1,
        selection_base_anchor: base,
        selection_extent_anchor: extent,
        logical_edit_id: 1,
        request_digest: 0x5151_0001,
        acknowledge_previous_logical_edit_id: 0,
        selection_generation: 1,
        base_utf16_range: SourceRange {
            start_byte: 1,
            end_byte: 3,
        },
        result_selection_base_utf16: 1,
        result_selection_extent_utf16: 3,
        selection_affinity: DOWNSTREAM,
        selection_direction: 0,
        replacement_bytes_len: replacement.len() as u64,
        budget: budget(1),
        history_group_id: 0,
    };
    let mut output = vec![0u8; size_of::<SourceTransactionReceiptV1>()];
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_v4_source_transaction_v1(
            &request,
            replacement.as_ptr(),
            replacement.len() as u64,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let receipt =
        unsafe { std::ptr::read_unaligned(output.as_ptr().cast::<SourceTransactionReceiptV1>()) };
    assert_eq!(
        receipt.struct_size as usize,
        size_of::<SourceTransactionReceiptV1>()
    );
    assert_eq!(receipt.base_revision, 1);
    assert_eq!(receipt.result_revision, 2);
    assert_eq!(
        receipt.history_disposition,
        HistoryDisposition::Retained as u32
    );
    assert_eq!(receipt.history_token, outcome.primary_handle);
    assert_ne!(receipt.history_token, 0);
    assert_eq!(receipt.replacement_bytes, replacement.len() as u64);
    assert_eq!(
        receipt.flags
            & (SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT
                | SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES),
        SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT | SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES
    );
    assert_eq!(
        receipt.base_byte_range,
        SourceRange {
            start_byte: 1,
            end_byte: 5,
        }
    );
    assert_eq!(
        receipt.result_byte_range,
        SourceRange {
            start_byte: 1,
            end_byte: 4,
        }
    );
    assert_eq!(resolve_anchor(session, base, 2, 1), 1);
    assert_eq!(resolve_anchor(session, extent, 2, 1), 4);
    assert_eq!(read_source(session, 2, "aéxbc\n".len()), b"a\xc3\xa9xbc\n");

    // A lost-reply retry returns the exact terminal without another commit.
    let mut retry_output = vec![0u8; size_of::<SourceTransactionReceiptV1>()];
    assert_eq!(
        flark_v4_source_transaction_v1(
            &request,
            replacement.as_ptr(),
            replacement.len() as u64,
            retry_output.as_mut_ptr(),
            retry_output.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    assert_eq!(outcome.revision, 2);
    assert_eq!(output, retry_output);
    close_session(session);
}

#[test]
fn staged_source_transaction_is_resumable_receipted_and_replayable() {
    let session = open_session(b"hello", 0x5252);
    let base = create_anchor(session, 1, SOURCE_BYTE, 1, DOWNSTREAM);
    let extent = create_anchor(session, 1, SOURCE_BYTE, 4, UPSTREAM);
    let replacement = vec![b'x'; 90_000];
    let begin = BulkBeginRequest {
        struct_size: size_of::<BulkBeginRequest>() as u32,
        flags: 0,
        session,
        expected_revision: 1,
        range: SourceRange {
            start_byte: 1,
            end_byte: 4,
        },
        expected_total_bytes: replacement.len() as u64,
        reserved: [0; 2],
    };
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_v4_bulk_begin(&begin, &mut outcome),
        StatusCode::Ok as u32
    );
    let transaction = outcome.primary_handle;
    for (index, chunk) in replacement.chunks(65_536).enumerate() {
        let stage = StageRequest {
            struct_size: size_of::<StageRequest>() as u32,
            flags: 0,
            session,
            transaction,
            chunk_offset: if index == 0 { 0 } else { 65_536 },
            chunk_len: chunk.len() as u64,
            reserved: [0; 2],
        };
        assert_eq!(
            flark_v4_bulk_append(&stage, chunk.as_ptr(), chunk.len() as u64, &mut outcome,),
            StatusCode::Ok as u32
        );
    }

    let mut request = StagedSourceTransactionRequestV1 {
        struct_size: size_of::<StagedSourceTransactionRequestV1>() as u32,
        flags: 0,
        session,
        transaction,
        expected_revision: 1,
        progress_token: 0,
        selection_base_anchor: base,
        selection_extent_anchor: extent,
        logical_edit_id: 1,
        request_digest: 0x5252_0001,
        acknowledge_previous_logical_edit_id: 0,
        selection_generation: 1,
        result_selection_utf16: 90_001,
        selection_affinity: DOWNSTREAM,
        selection_direction: 0,
        budget: budget(1),
        history_group_id: 0,
        reserved: [0; 2],
    };
    let mut output = vec![0u8; size_of::<SourceTransactionReceiptV1>()];
    let mut status = flark_v4_staged_source_transaction_v1(
        &request,
        output.as_mut_ptr(),
        output.len() as u64,
        &mut outcome,
    );
    let mut turns = 0;
    while status == StatusCode::BudgetExhausted as u32 {
        request.progress_token = outcome.progress_token;
        status = flark_v4_staged_source_transaction_v1(
            &request,
            output.as_mut_ptr(),
            output.len() as u64,
            &mut outcome,
        );
        turns += 1;
        assert!(turns < 10, "bounded staged commit should converge");
    }
    assert_eq!(status, StatusCode::Ok as u32);
    let receipt =
        unsafe { std::ptr::read_unaligned(output.as_ptr().cast::<SourceTransactionReceiptV1>()) };
    assert_eq!(receipt.result_revision, 2);
    assert_eq!(receipt.result_source_byte_length, 90_002);
    assert_eq!(receipt.result_source_utf16_length, 90_002);
    assert_eq!(receipt.result_selection_base_utf16, 90_001);
    assert_eq!(receipt.result_selection_extent_utf16, 90_001);
    assert_eq!(receipt.replacement_bytes, replacement.len() as u64);
    assert_ne!(receipt.history_token, 0);
    assert_eq!(
        receipt.flags
            & (SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT | SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES),
        SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT | SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES
    );
    assert_eq!(resolve_anchor(session, base, 2, UTF16), 90_001);
    assert_eq!(resolve_anchor(session, extent, 2, UTF16), 90_001);

    // A lost terminal reply remains recoverable even though the staging
    // handle was consumed by the source commit.
    let mut replay = vec![0u8; size_of::<SourceTransactionReceiptV1>()];
    assert_eq!(
        flark_v4_staged_source_transaction_v1(
            &request,
            replay.as_mut_ptr(),
            replay.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    assert_eq!(output, replay);
    close_session(session);
}

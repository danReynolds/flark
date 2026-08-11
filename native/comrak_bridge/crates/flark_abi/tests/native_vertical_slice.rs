use std::mem::size_of;

use flark_abi::{
    flark_v4_close_begin, flark_v4_close_finish, flark_v4_close_pump, flark_v4_continuation_next,
    flark_v4_continuation_release, flark_v4_create_begin, flark_v4_create_commit,
    flark_v4_history_release, flark_v4_history_replay, flark_v4_pump, flark_v4_query_viewport,
    flark_v4_small_edit, flark_v4_source_read, CloseRequest, ContinuationRequest, CreateRequest,
    EditDescriptor, HistoryRequest, InlineFactRecord, Outcome, PumpRequest, QueryRequest,
    ResultPageHeader, SessionConfig, SessionRef, SmallEditRequest, SourceRange, SourceReadRequest,
    TransactionRequest, ViewportRowRecord, WorkBudget, INLINE_FACT_EMPHASIS,
    VIEWPORT_ROW_FLAG_CONTINUITY_PLAIN_TEXT_EDIT, VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE,
};
use flark_runtime::{HistoryDisposition, StatusCode};

fn budget(work: u64) -> WorkBudget {
    WorkBudget {
        max_work_units: work,
        advisory_max_micros: 0,
        max_result_items: 256,
        max_result_bytes: 64 * 1024,
    }
}

#[test]
fn fixed_abi_drives_open_edit_source_and_semantic_viewport() {
    assert_eq!(size_of::<ViewportRowRecord>(), 128);
    assert_eq!(size_of::<InlineFactRecord>(), 80);
    let source = b"# *Flark*\n\nA quick paragraph.\n\n- one\n- two\n";
    let owner = 71;
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: 0,
        owner_token: owner,
        expected_total_bytes: source.len() as u64,
        config: SessionConfig {
            struct_size: size_of::<SessionConfig>() as u32,
            parser_profile: 2,
            history_budget_bytes: 1024,
            max_document_bytes: 16 * 1024 * 1024,
            flags: 0,
            reserved: [0; 4],
        },
    };
    let mut outcome = Outcome::default();
    let status = flark_v4_create_begin(&create, source.as_ptr(), source.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    let session = outcome.primary_handle;
    let transaction = outcome.secondary_handle;
    assert_ne!(session, 0);
    assert_ne!(transaction, 0);

    let commit = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session: SessionRef {
            session,
            owner_token: owner,
        },
        transaction,
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
            session: commit.session,
            expected_revision: 1,
            progress_token: token,
            budget: budget(64),
        };
        status = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;
    }
    assert_eq!(status, StatusCode::Ok as u32);
    assert_eq!(outcome.revision, 1);

    let mut page = vec![0_u8; 64 * 1024];
    let query = QueryRequest {
        struct_size: size_of::<QueryRequest>() as u32,
        query_kind: 2,
        session: commit.session,
        revision: 1,
        snapshot: 0,
        range: SourceRange {
            start_byte: 0,
            end_byte: source.len() as u64,
        },
        continuation: 0,
        budget: WorkBudget {
            max_result_items: 1,
            ..budget(64)
        },
        reserved: [0; 1],
    };
    status = flark_v4_query_viewport(&query, page.as_mut_ptr(), page.len() as u64, &mut outcome);
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32);
    let header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(header.revision, 1);
    assert!(header.item_count > 0);
    assert_ne!(header.continuation, 0);
    let first = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_eq!(first.kind, 12, "ATX heading kind");
    assert_eq!(first.semantic_variant, 1, "parser-authored H1 variant");
    assert_ne!(
        first.flags & VIEWPORT_ROW_FLAG_CONTINUITY_PLAIN_TEXT_EDIT,
        0,
        "heading content authorizes stable plain-text edit presentation"
    );
    assert_ne!(first.flags & VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE, 0);
    assert_eq!(first.inline_fact_count, 1);
    assert_eq!(header.payload_bytes, 128 + 80);
    let inline = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>() + size_of::<ViewportRowRecord>())
            .cast::<InlineFactRecord>()
            .read_unaligned()
    };
    assert_eq!(inline.kind, INLINE_FACT_EMPHASIS);
    assert_eq!(inline.source_start_byte, 2);
    assert_eq!(inline.source_end_byte, 9);
    assert_eq!(inline.content_start_byte, 3);
    assert_eq!(inline.content_end_byte, 8);

    let continuation = ContinuationRequest {
        struct_size: size_of::<ContinuationRequest>() as u32,
        flags: 0,
        session: commit.session,
        revision: header.revision,
        snapshot: header.snapshot,
        continuation: header.continuation,
        budget: query.budget,
        reserved: [0; 1],
    };
    let previous_end = header.covered_range.end_byte;
    page.fill(0);
    status = flark_v4_continuation_next(
        &continuation,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32);
    let next_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(next_header.requested_range, header.requested_range);
    assert_eq!(next_header.covered_range.start_byte, previous_end);
    assert!(next_header.covered_range.end_byte > previous_end);
    if next_header.continuation != 0 {
        let release = ContinuationRequest {
            continuation: next_header.continuation,
            snapshot: next_header.snapshot,
            ..continuation
        };
        status = flark_v4_continuation_release(&release, &mut outcome);
        assert_eq!(status, StatusCode::Ok as u32);
    }

    let quick = source
        .windows(b"quick".len())
        .position(|window| window == b"quick")
        .expect("quick offset");
    let descriptor = EditDescriptor {
        start_byte: quick as u64,
        end_byte: (quick + b"quick".len()) as u64,
        replacement_offset: 0,
        replacement_len: 4,
    };
    let replacement = b"fast";
    let edit = SmallEditRequest {
        struct_size: size_of::<SmallEditRequest>() as u32,
        flags: 0,
        session: commit.session,
        expected_revision: 1,
        edit_count: 1,
        reserved_u32: 0,
        replacement_bytes_len: replacement.len() as u64,
        budget: budget(1),
        reserved: [0; 2],
    };
    status = flark_v4_small_edit(
        &edit,
        &descriptor,
        1,
        replacement.as_ptr(),
        replacement.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    assert_eq!(outcome.revision, 2);
    assert_ne!(outcome.primary_handle, 0);
    assert_eq!(outcome.detail_code, HistoryDisposition::Retained as u64);
    let undo_token = outcome.primary_handle;

    token = 0;
    loop {
        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session: commit.session,
            expected_revision: 2,
            progress_token: token,
            budget: budget(64),
        };
        status = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;
        if status != StatusCode::BudgetExhausted as u32 {
            break;
        }
    }
    assert_eq!(status, StatusCode::Ok as u32);

    let expected = String::from_utf8(source.to_vec())
        .expect("source")
        .replacen("quick", "fast", 1);
    let read = SourceReadRequest {
        struct_size: size_of::<SourceReadRequest>() as u32,
        flags: 0,
        session: commit.session,
        revision: 2,
        range: SourceRange {
            start_byte: 0,
            end_byte: expected.len() as u64,
        },
        reserved: [0; 2],
    };
    page.fill(0);
    status = flark_v4_source_read(&read, page.as_mut_ptr(), page.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    let payload =
        &page[size_of::<ResultPageHeader>()..size_of::<ResultPageHeader>() + expected.len()];
    assert_eq!(payload, expected.as_bytes());

    let mut history = HistoryRequest {
        struct_size: size_of::<HistoryRequest>() as u32,
        flags: 0,
        session: commit.session,
        expected_revision: 2,
        history_token: undo_token,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    status = flark_v4_history_replay(&history, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    assert_eq!(outcome.revision, 3);
    assert_ne!(outcome.primary_handle, 0);
    let redo_token = outcome.primary_handle;

    let restored_read = SourceReadRequest {
        revision: 3,
        range: SourceRange {
            start_byte: 0,
            end_byte: source.len() as u64,
        },
        ..read
    };
    page.fill(0);
    status = flark_v4_source_read(
        &restored_read,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let payload =
        &page[size_of::<ResultPageHeader>()..size_of::<ResultPageHeader>() + source.len()];
    assert_eq!(payload, source);

    history.expected_revision = 3;
    history.history_token = redo_token;
    status = flark_v4_history_replay(&history, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    assert_eq!(outcome.revision, 4);
    assert_ne!(outcome.primary_handle, 0);
    let final_undo_token = outcome.primary_handle;

    let redone_read = SourceReadRequest {
        revision: 4,
        range: SourceRange {
            start_byte: 0,
            end_byte: expected.len() as u64,
        },
        ..read
    };
    page.fill(0);
    status = flark_v4_source_read(
        &redone_read,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let payload =
        &page[size_of::<ResultPageHeader>()..size_of::<ResultPageHeader>() + expected.len()];
    assert_eq!(payload, expected.as_bytes());

    history.expected_revision = 4;
    history.history_token = final_undo_token;
    status = flark_v4_history_release(&history, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);

    let mut close = CloseRequest {
        struct_size: size_of::<CloseRequest>() as u32,
        flags: 0,
        session: commit.session,
        progress_token: 0,
        budget: budget(1),
        reserved: [0; 1],
    };
    status = flark_v4_close_begin(&close, &mut outcome);
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::BudgetExhausted as u32);
    close.progress_token = outcome.progress_token;
    let mut close_turns = 0;
    while status == StatusCode::BudgetExhausted as u32 {
        status = flark_v4_close_pump(&close, &mut outcome);
        close.progress_token = outcome.progress_token;
        close_turns += 1;
        assert!(close_turns < 10_000, "bounded close should converge");
    }
    assert_eq!(status, StatusCode::Ok as u32);
    status = flark_v4_close_finish(&close, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);

    status = flark_v4_source_read(&read, page.as_mut_ptr(), page.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::InvalidHandle as u32);
}

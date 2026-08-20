//! The RFC 029 opening-query mode through the fixed C ABI: an unknown-length
//! stream admits in chunks (one deliberately splitting a UTF-8 scalar),
//! certified viewport rows are queryable before the transaction commits, and
//! commit seals the load instead of parsing a buffered copy.

use std::mem::size_of;

use flark_abi::{
    flark_v4_close_begin, flark_v4_close_finish, flark_v4_close_pump, flark_v4_continuation_next,
    flark_v4_create_append, flark_v4_create_begin, flark_v4_create_commit, flark_v4_pump,
    flark_v4_query_viewport, CloseRequest, ContinuationRequest, CreateRequest, Outcome,
    PumpRequest, QueryRequest, ResultPageHeader, SessionConfig, SessionRef, SourceRange,
    StageRequest, TransactionRequest, ViewportRowRecord, WorkBudget, CREATE_FLAG_OPENING,
};
use flark_runtime::{CertificationState, StatusCode};

fn budget(work: u64) -> WorkBudget {
    WorkBudget {
        max_work_units: work,
        advisory_max_micros: 0,
        max_result_items: 256,
        max_result_bytes: 64 * 1024,
    }
}

fn budget_with_items(work: u64, items: u32) -> WorkBudget {
    WorkBudget {
        max_result_items: items,
        ..budget(work)
    }
}

fn finish_opening(session: SessionRef, transaction: u64) {
    let commit = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction,
        expected_revision: 0,
        progress_token: 0,
        budget: budget(8),
        reserved: [0; 1],
    };
    let mut outcome = Outcome::default();
    let mut status = flark_v4_create_commit(&commit, &mut outcome);
    let mut token = outcome.progress_token;
    while status == StatusCode::BudgetExhausted as u32 {
        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session,
            expected_revision: 1,
            progress_token: token,
            budget: budget(4_096),
        };
        status = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;
    }
    assert_eq!(status, StatusCode::Ok as u32);
}

fn close_session(session: SessionRef) {
    let mut outcome = Outcome::default();
    let mut close = CloseRequest {
        struct_size: size_of::<CloseRequest>() as u32,
        flags: 0,
        session,
        progress_token: 0,
        budget: budget(4_096),
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
    assert_eq!(
        flark_v4_close_finish(&close, &mut outcome),
        StatusCode::Ok as u32
    );
}

fn fixture() -> Vec<u8> {
    let mut source = String::new();
    for index in 0..200 {
        source.push_str(&format!(
            "Paragraph {index} on the café terrace has **bold** words.\n\n"
        ));
    }
    source.into_bytes()
}

#[test]
fn opening_mode_serves_certified_rows_before_commit_and_seals_on_commit() {
    let source = fixture();
    let owner = 88;
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: CREATE_FLAG_OPENING,
        owner_token: owner,
        // Zero declares an unknown-length stream: only commit ends it.
        expected_total_bytes: 0,
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
    // Begin with an empty initial chunk: the session exists before any byte.
    let status = flark_v4_create_begin(&create, std::ptr::null(), 0, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: owner,
    };
    let transaction = outcome.secondary_handle;

    // A pre-certification semantic query is a typed not-ready reply through
    // the C encoding, never an anonymous internal fault.
    let mut probe_page = vec![0_u8; 4096];
    let early_query = QueryRequest {
        struct_size: size_of::<QueryRequest>() as u32,
        query_kind: 2,
        session,
        revision: 1,
        snapshot: 0,
        range: SourceRange {
            start_byte: 0,
            end_byte: 0,
        },
        continuation: 0,
        budget: budget(64),
        reserved: [0; 1],
    };
    let status = flark_v4_query_viewport(
        &early_query,
        probe_page.as_mut_ptr(),
        probe_page.len() as u64,
        &mut outcome,
    );
    // Exact pending source with NotCertified — never an anonymous fault.
    assert_eq!(status, StatusCode::NotCertified as u32);

    // Chunk so one boundary lands inside the two-byte 'é' of "café".
    let split_at = source
        .iter()
        .position(|byte| *byte == 0xC3)
        .expect("multi-byte scalar")
        + 1;
    let chunks = [
        &source[..split_at],
        &source[split_at..source.len() / 2],
        &source[source.len() / 2..],
    ];
    let mut offset = 0_u64;
    let mut token = 0_u64;
    let mut queried_before_commit = false;
    for chunk in chunks {
        let stage = StageRequest {
            struct_size: size_of::<StageRequest>() as u32,
            flags: 0,
            session,
            transaction,
            chunk_offset: offset,
            chunk_len: chunk.len() as u64,
            reserved: [0; 2],
        };
        let status =
            flark_v4_create_append(&stage, chunk.as_ptr(), chunk.len() as u64, &mut outcome);
        assert_eq!(status, StatusCode::Ok as u32);
        offset += chunk.len() as u64;

        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session,
            expected_revision: 1,
            progress_token: token,
            budget: budget(2_048),
        };
        let _ = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;

        // Once certified rows exist they are queryable mid-transaction.
        let mut page = vec![0_u8; 64 * 1024];
        let query = QueryRequest {
            struct_size: size_of::<QueryRequest>() as u32,
            query_kind: 2,
            session,
            revision: 1,
            snapshot: 0,
            range: SourceRange {
                start_byte: 0,
                // Up to three raw bytes may be carried as a split scalar and
                // are not yet admitted source.
                end_byte: offset.saturating_sub(3),
            },
            continuation: 0,
            budget: budget(64),
            reserved: [0; 1],
        };
        let status =
            flark_v4_query_viewport(&query, page.as_mut_ptr(), page.len() as u64, &mut outcome);
        if status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32 {
            let header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
            if header.item_count > 0 {
                let first = unsafe {
                    page.as_ptr()
                        .add(size_of::<ResultPageHeader>())
                        .cast::<ViewportRowRecord>()
                        .read_unaligned()
                };
                assert_eq!(first.source_start_byte, 0, "certified rows begin at BOF");
                queried_before_commit = true;
            }
        }
    }
    assert!(
        queried_before_commit,
        "certified viewport rows were served before the creation transaction committed"
    );

    // Drain the parser to the currently admitted frontier. The stream remains
    // unsealed, so this yields BudgetExhausted after adopting the last page.
    let pump = PumpRequest {
        struct_size: size_of::<PumpRequest>() as u32,
        flags: 0,
        session,
        expected_revision: 1,
        progress_token: token,
        budget: budget(1_000_000),
    };
    assert_eq!(
        flark_v4_pump(&pump, &mut outcome),
        StatusCode::BudgetExhausted as u32
    );

    // A one-item certification page can expose only the certified head. Its
    // coverage and continuation must say so; the next page is the pending
    // tail, never an implicit extension of the head's certification.
    let precommit_end = source.len().saturating_sub(3) as u64;
    let mut certification_page = vec![0_u8; 64 * 1024];
    let certification_query = QueryRequest {
        struct_size: size_of::<QueryRequest>() as u32,
        query_kind: 3,
        session,
        revision: 1,
        snapshot: 0,
        range: SourceRange {
            start_byte: 0,
            end_byte: precommit_end,
        },
        continuation: 0,
        budget: budget_with_items(64, 1),
        reserved: [0; 1],
    };
    let status = flark_v4_query_viewport(
        &certification_query,
        certification_page.as_mut_ptr(),
        certification_page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::ResultCapReached as u32);
    let head = unsafe {
        certification_page
            .as_ptr()
            .cast::<ResultPageHeader>()
            .read_unaligned()
    };
    assert_eq!(head.covered_range.start_byte, 0);
    assert!(head.covered_range.end_byte < precommit_end);
    assert_ne!(head.continuation, 0);
    assert_ne!(
        head.certification_state,
        CertificationState::CurrentCertified as u32,
        "an incomplete page cannot certify the full requested range"
    );
    let next = ContinuationRequest {
        struct_size: size_of::<ContinuationRequest>() as u32,
        flags: 0,
        session,
        revision: 1,
        snapshot: 1,
        continuation: head.continuation,
        budget: budget_with_items(64, 3),
        reserved: [0; 1],
    };
    let status = flark_v4_continuation_next(
        &next,
        certification_page.as_mut_ptr(),
        certification_page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::NotCertified as u32);
    let tail = unsafe {
        certification_page
            .as_ptr()
            .cast::<ResultPageHeader>()
            .read_unaligned()
    };
    assert_eq!(tail.covered_range.start_byte, head.covered_range.end_byte);
    assert_eq!(tail.covered_range.end_byte, precommit_end);
    assert_eq!(
        tail.certification_state,
        CertificationState::PendingNeutral as u32
    );
    assert_eq!(tail.continuation, 0);

    // A row-capped semantic page covers exactly its encoded row. The hidden
    // lookahead row proves that another row exists but must not advance the
    // continuation beyond bytes the page did not return.
    let mut capped_semantic_page = vec![0_u8; 64 * 1024];
    let capped_semantic_query = QueryRequest {
        query_kind: 4,
        budget: budget_with_items(64, 1),
        ..certification_query
    };
    let status = flark_v4_query_viewport(
        &capped_semantic_query,
        capped_semantic_page.as_mut_ptr(),
        capped_semantic_page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::ResultCapReached as u32);
    let capped_semantic_head = unsafe {
        capped_semantic_page
            .as_ptr()
            .cast::<ResultPageHeader>()
            .read_unaligned()
    };
    assert_eq!(capped_semantic_head.item_count, 1);
    let capped_semantic_row = unsafe {
        capped_semantic_page
            .as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_eq!(
        capped_semantic_head.covered_range.end_byte, capped_semantic_row.source_end_byte,
        "lookahead rows must not be skipped by the continuation"
    );
    assert_ne!(capped_semantic_head.continuation, 0);

    // The semantic page is similarly bounded to the certified prefix. Its
    // continuation reaches an exact pending-source page rather than an empty
    // semantic page falsely marked current-certified.
    let mut semantic_page = vec![0_u8; 64 * 1024];
    let semantic_query = QueryRequest {
        query_kind: 4,
        budget: budget_with_items(64, 256),
        ..certification_query
    };
    let status = flark_v4_query_viewport(
        &semantic_query,
        semantic_page.as_mut_ptr(),
        semantic_page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::ResultCapReached as u32);
    let semantic_head = unsafe {
        semantic_page
            .as_ptr()
            .cast::<ResultPageHeader>()
            .read_unaligned()
    };
    assert_eq!(
        semantic_head.certification_state,
        CertificationState::CurrentCertified as u32
    );
    assert!(semantic_head.covered_range.end_byte < precommit_end);
    assert_ne!(semantic_head.continuation, 0);
    let semantic_next = ContinuationRequest {
        continuation: semantic_head.continuation,
        ..next
    };
    let status = flark_v4_continuation_next(
        &semantic_next,
        semantic_page.as_mut_ptr(),
        semantic_page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::NotCertified as u32);
    let semantic_tail = unsafe {
        semantic_page
            .as_ptr()
            .cast::<ResultPageHeader>()
            .read_unaligned()
    };
    assert_eq!(
        semantic_tail.covered_range.start_byte,
        semantic_head.covered_range.end_byte
    );
    assert_eq!(semantic_tail.covered_range.end_byte, precommit_end);
    assert_eq!(
        semantic_tail.certification_state,
        CertificationState::PendingNeutral as u32
    );

    let commit = TransactionRequest {
        struct_size: size_of::<TransactionRequest>() as u32,
        flags: 0,
        session,
        transaction,
        expected_revision: 0,
        progress_token: 0,
        budget: budget(8),
        reserved: [0; 1],
    };
    let mut status = flark_v4_create_commit(&commit, &mut outcome);
    token = outcome.progress_token;
    while status == StatusCode::BudgetExhausted as u32 {
        let pump = PumpRequest {
            struct_size: size_of::<PumpRequest>() as u32,
            flags: 0,
            session,
            expected_revision: 1,
            progress_token: token,
            budget: budget(4_096),
        };
        status = flark_v4_pump(&pump, &mut outcome);
        token = outcome.progress_token;
    }
    assert_eq!(status, StatusCode::Ok as u32);

    let mut page = vec![0_u8; 64 * 1024];
    let query = QueryRequest {
        struct_size: size_of::<QueryRequest>() as u32,
        query_kind: 2,
        session,
        revision: 1,
        snapshot: 0,
        range: SourceRange {
            start_byte: 0,
            end_byte: source.len() as u64,
        },
        continuation: 0,
        budget: budget(64),
        reserved: [0; 1],
    };
    status = flark_v4_query_viewport(&query, page.as_mut_ptr(), page.len() as u64, &mut outcome);
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32);
    let header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert!(header.item_count > 0);

    let mut close = CloseRequest {
        struct_size: size_of::<CloseRequest>() as u32,
        flags: 0,
        session,
        progress_token: 0,
        budget: budget(4_096),
        reserved: [0; 1],
    };
    let mut status = flark_v4_close_begin(&close, &mut outcome);
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
    assert_eq!(
        flark_v4_close_finish(&close, &mut outcome),
        StatusCode::Ok as u32
    );
}

#[test]
fn rejected_utf8_continuation_preserves_carry_for_same_offset_retry() {
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: CREATE_FLAG_OPENING,
        owner_token: 89,
        expected_total_bytes: 4,
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
        flark_v4_create_begin(&create, std::ptr::null(), 0, &mut outcome),
        StatusCode::Ok as u32
    );
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: create.owner_token,
    };
    let transaction = outcome.secondary_handle;

    let lead = [0xF0_u8];
    let first = StageRequest {
        struct_size: size_of::<StageRequest>() as u32,
        flags: 0,
        session,
        transaction,
        chunk_offset: 0,
        chunk_len: lead.len() as u64,
        reserved: [0; 2],
    };
    assert_eq!(
        flark_v4_create_append(&first, lead.as_ptr(), lead.len() as u64, &mut outcome),
        StatusCode::Ok as u32
    );

    let invalid = [0_u8];
    let retry = StageRequest {
        chunk_offset: 1,
        chunk_len: invalid.len() as u64,
        ..first
    };
    assert_eq!(
        flark_v4_create_append(&retry, invalid.as_ptr(), invalid.len() as u64, &mut outcome,),
        StatusCode::InvalidUtf8 as u32
    );

    let ascii = b"a";
    assert_eq!(
        flark_v4_create_append(&retry, ascii.as_ptr(), ascii.len() as u64, &mut outcome,),
        StatusCode::InvalidUtf8 as u32,
        "same-offset ASCII retry must not discard the accepted lead byte"
    );

    // The rejected byte did not advance the offset or discard the accepted
    // lead byte, so the genuine continuation completes the original scalar.
    let continuation = [0x9F_u8, 0x92, 0xA9];
    let retry = StageRequest {
        chunk_len: continuation.len() as u64,
        ..retry
    };
    assert_eq!(
        flark_v4_create_append(
            &retry,
            continuation.as_ptr(),
            continuation.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    finish_opening(session, transaction);
    close_session(session);
}

#[test]
fn unknown_length_opening_enforces_max_document_bytes_incrementally() {
    let create = CreateRequest {
        struct_size: size_of::<CreateRequest>() as u32,
        flags: CREATE_FLAG_OPENING,
        owner_token: 90,
        expected_total_bytes: 0,
        config: SessionConfig {
            struct_size: size_of::<SessionConfig>() as u32,
            parser_profile: 2,
            history_budget_bytes: 0,
            max_document_bytes: 4,
            flags: 0,
            reserved: [0; 4],
        },
    };
    let mut outcome = Outcome::default();
    let initial = b"abcd";
    assert_eq!(
        flark_v4_create_begin(
            &create,
            initial.as_ptr(),
            initial.len() as u64,
            &mut outcome,
        ),
        StatusCode::Ok as u32
    );
    let session = SessionRef {
        session: outcome.primary_handle,
        owner_token: create.owner_token,
    };
    let transaction = outcome.secondary_handle;
    let overflow = b"e";
    let append = StageRequest {
        struct_size: size_of::<StageRequest>() as u32,
        flags: 0,
        session,
        transaction,
        chunk_offset: initial.len() as u64,
        chunk_len: overflow.len() as u64,
        reserved: [0; 2],
    };
    assert_eq!(
        flark_v4_create_append(
            &append,
            overflow.as_ptr(),
            overflow.len() as u64,
            &mut outcome,
        ),
        StatusCode::ResourceLimitExceeded as u32
    );
    finish_opening(session, transaction);
    close_session(session);

    // The same cap applies to an initial chunk even when the total is unknown.
    let oversized = b"abcde";
    assert_eq!(
        flark_v4_create_begin(
            &CreateRequest {
                owner_token: 91,
                ..create
            },
            oversized.as_ptr(),
            oversized.len() as u64,
            &mut outcome,
        ),
        StatusCode::ResourceLimitExceeded as u32
    );
}

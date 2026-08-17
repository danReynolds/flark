//! The RFC 029 opening-query mode through the fixed C ABI: an unknown-length
//! stream admits in chunks (one deliberately splitting a UTF-8 scalar),
//! certified viewport rows are queryable before the transaction commits, and
//! commit seals the load instead of parsing a buffered copy.

use std::mem::size_of;

use flark_abi::{
    flark_v4_close_begin, flark_v4_close_finish, flark_v4_close_pump, flark_v4_create_append,
    flark_v4_create_begin, flark_v4_create_commit, flark_v4_pump, flark_v4_query_viewport,
    CloseRequest, CreateRequest, Outcome, PumpRequest, QueryRequest, ResultPageHeader,
    SessionConfig, SessionRef, SourceRange, StageRequest, TransactionRequest, ViewportRowRecord,
    WorkBudget, CREATE_FLAG_OPENING,
};
use flark_runtime::StatusCode;

fn budget(work: u64) -> WorkBudget {
    WorkBudget {
        max_work_units: work,
        advisory_max_micros: 0,
        max_result_items: 256,
        max_result_bytes: 64 * 1024,
    }
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

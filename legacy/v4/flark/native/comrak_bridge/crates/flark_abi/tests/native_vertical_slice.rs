use std::mem::size_of;

use flark_abi::{
    flark_v4_close_begin, flark_v4_close_finish, flark_v4_close_pump, flark_v4_continuation_next,
    flark_v4_continuation_release, flark_v4_create_begin, flark_v4_create_commit,
    flark_v4_history_release, flark_v4_history_replay, flark_v4_negotiate, flark_v4_pump,
    flark_v4_query_viewport, flark_v4_session_inspect, flark_v4_small_edit, flark_v4_source_read,
    AbiInfo, CloseRequest, ContinuationRequest, CreateRequest, EditDescriptor, HistoryRequest,
    InlineFactRecord, InspectRequest, NegotiateRequest, Outcome, ProjectionSegmentRecord,
    PumpRequest, QueryRequest, ResultPageHeader, SemanticTargetRecord, SessionConfig,
    SessionInspection, SessionRef, SmallEditRequest, SourceRange, SourceReadRequest,
    TransactionRequest, ViewportRowRecord, WorkBudget, ABI_MAJOR, ABI_MINOR, INLINE_FACT_EMPHASIS,
    INLINE_FACT_LITERAL_SAFE_ENVELOPE, INLINE_FACT_PENDING_PRESENTATION_PLAN,
    INLINE_FACT_PENDING_PRESENTATION_ROW, INLINE_FACT_PENDING_PRESENTATION_STEP,
    INLINE_FACT_PROJECTION_EDIT_CELL, INLINE_FACT_TABLE_CELL, INSPECT_FLAG_GLOBAL_LIVE_STATE,
    LITERAL_EDIT_CLASS_ASCII_WORD_INSERTION, LITERAL_EDIT_CLASS_SINGLE_ASCII_LITERAL_UNIT_DELETION,
    LITERAL_EDIT_CLASS_SINGLE_ASCII_SPACE_INSERTION,
    PENDING_PRESENTATION_PLAN_REPLACED_ROW_COUNT_SHIFT,
    PENDING_PRESENTATION_PLAN_SEQUENCE_LENGTH_MASK, PENDING_PRESENTATION_PLAN_STEP_COUNT_SHIFT,
    PENDING_PRESENTATION_ROW_FACT_COUNT_SHIFT, PENDING_PRESENTATION_STEP_PREFIX_LENGTH_MASK,
    PENDING_PRESENTATION_STEP_ROW_COUNT_SHIFT, PROJECTION_EDIT_CELL_CHAIN_RESULT,
    PROJECTION_EDIT_CELL_EMPTY_LITERAL_RESULT, PROJECTION_EDIT_CELL_MATCHER_MASK,
    PROJECTION_EDIT_CELL_MATCH_ANY_NO_CRLF_SPLICE,
    PROJECTION_EDIT_CELL_MATCH_ASCII_LITERAL_SPLICE_IN_LITERAL,
    PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL,
    PROJECTION_EDIT_CELL_MATCH_EXACT_SPLICE_REPLACE_BLOCK_SHELL,
    PROJECTION_EDIT_CELL_MATCH_SIMPLE_BLOCK_PREFIX_PLAN, PROJECTION_EDIT_CELL_PRESENT_EXACT,
    PROJECTION_EDIT_CELL_RESULT_SHELL_ATX_HEADING, PROJECTION_EDIT_CELL_RESULT_SHELL_KIND_MASK,
    PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL, PROJECTION_EDIT_CELL_RETAIN_OUTSIDE,
    VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE, VIEWPORT_ROW_FLAG_PROJECTED_RESERVED,
    VIEWPORT_ROW_INLINE_FACT_COUNT_MASK, VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT,
    VIEWPORT_ROW_TABLE_PRESENTATION,
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

fn read_inline_facts(page: &[u8], count: u32) -> Vec<InlineFactRecord> {
    (0..count as usize)
        .map(|index| unsafe {
            page.as_ptr()
                .add(
                    size_of::<ResultPageHeader>()
                        + size_of::<ViewportRowRecord>()
                        + index * size_of::<InlineFactRecord>(),
                )
                .cast::<InlineFactRecord>()
                .read_unaligned()
        })
        .collect()
}

#[test]
fn fixed_abi_drives_open_edit_source_and_semantic_viewport() {
    assert_eq!(size_of::<ViewportRowRecord>(), 128);
    assert_eq!(size_of::<ProjectionSegmentRecord>(), 32);
    assert_eq!(size_of::<InlineFactRecord>(), 80);
    let preceding_minor = NegotiateRequest {
        struct_size: size_of::<NegotiateRequest>() as u32,
        requested_major: ABI_MAJOR,
        requested_minor: 33,
        required_capability_bits: (1_u64 << 35) - 1,
    };
    let mut info = AbiInfo::default();
    let mut outcome = Outcome::default();
    assert_eq!(
        flark_v4_negotiate(&preceding_minor, &mut info, &mut outcome),
        StatusCode::UnsupportedAbiVersion as u32,
        "the stateless ABI must reject a preceding minor it cannot tailor"
    );
    let subsequent_minor = NegotiateRequest {
        requested_minor: ABI_MINOR + 1,
        required_capability_bits: (1_u64 << 38) - 1,
        ..preceding_minor
    };
    assert_eq!(
        flark_v4_negotiate(&subsequent_minor, &mut info, &mut outcome),
        StatusCode::UnsupportedAbiVersion as u32,
        "the stateless ABI must reject a future minor it does not implement"
    );
    let negotiate = NegotiateRequest {
        requested_minor: ABI_MINOR,
        required_capability_bits: (1_u64 << 38) - 1,
        ..preceding_minor
    };
    assert_eq!(
        flark_v4_negotiate(&negotiate, &mut info, &mut outcome),
        StatusCode::Ok as u32
    );
    assert_eq!(info.abi_minor, ABI_MINOR);
    assert_eq!(info.capability_bits, (1_u64 << 38) - 1);

    let base_source = concat!(
        "# *Flark*\n\n",
        "# Plain heading\n\n",
        "A quick paragraph.\n\n",
        "- one\n- two\n\n",
        "> first\n> second\n\n",
        "| left | right |\n| :--- | ---: |\n| a | b |\n\n",
        "A [target](https://example.com/path \"title\").\n\n",
        "😀 tail.\n",
    );
    let dense_content = std::iter::repeat("a")
        .take(200)
        .collect::<Vec<_>>()
        .join(" ");
    let dense_rows = std::iter::repeat_with(|| format!("**{dense_content}**\n\n"))
        .take(7)
        .collect::<String>();
    let tail_quote = "> tail first\n> tail second\n";
    let dense_start = base_source.len();
    let source_text = format!("{base_source}{dense_rows}{tail_quote}");
    let source = source_text.as_bytes();
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
    let legacy_query = QueryRequest {
        struct_size: size_of::<QueryRequest>() as u32,
        query_kind: 4,
        session: commit.session,
        revision: 1,
        snapshot: 0,
        range: SourceRange {
            start_byte: 0,
            end_byte: b"# *Flark*\n".len() as u64,
        },
        continuation: 0,
        budget: WorkBudget {
            max_result_items: 1,
            ..budget(64)
        },
        reserved: [0; 1],
    };
    status = flark_v4_query_viewport(
        &legacy_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let legacy_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    let legacy_row = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_eq!(legacy_header.payload_bytes, 128 + 80);
    assert_eq!(legacy_row.inline_fact_count, 1);
    let legacy_fact = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>() + size_of::<ViewportRowRecord>())
            .cast::<InlineFactRecord>()
            .read_unaligned()
    };
    assert_eq!(legacy_fact.kind, INLINE_FACT_EMPHASIS);

    let query = QueryRequest {
        query_kind: 6,
        range: SourceRange {
            start_byte: 0,
            end_byte: source.len() as u64,
        },
        ..legacy_query
    };
    page.fill(0);
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
    assert_ne!(first.flags & VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE, 0);
    assert!(first.inline_fact_count >= 3);
    assert_eq!(header.payload_bytes, 128 + first.inline_fact_count * 80);
    let inline_facts = read_inline_facts(&page, first.inline_fact_count);
    let inline = &inline_facts[0];
    assert_eq!(inline.kind, INLINE_FACT_EMPHASIS);
    assert_eq!(inline.source_start_byte, 2);
    assert_eq!(inline.source_end_byte, 9);
    assert_eq!(inline.content_start_byte, 3);
    assert_eq!(inline.content_end_byte, 8);
    let word_envelope = inline_facts
        .iter()
        .find(|fact| {
            fact.kind == INLINE_FACT_LITERAL_SAFE_ENVELOPE
                && fact.flags == LITERAL_EDIT_CLASS_ASCII_WORD_INSERTION
        })
        .expect("word insertion envelope");
    assert_eq!(word_envelope.source_start_utf16, 3);
    assert_eq!(word_envelope.source_end_utf16, 8);
    let space_envelope = inline_facts
        .iter()
        .find(|fact| {
            fact.kind == INLINE_FACT_LITERAL_SAFE_ENVELOPE
                && fact.flags == LITERAL_EDIT_CLASS_SINGLE_ASCII_SPACE_INSERTION
        })
        .expect("space insertion envelope");
    assert!(inline_facts.iter().any(|fact| {
        fact.kind == INLINE_FACT_LITERAL_SAFE_ENVELOPE
            && fact.flags == LITERAL_EDIT_CLASS_SINGLE_ASCII_LITERAL_UNIT_DELETION
            && 3 <= fact.source_start_utf16
            && fact.source_end_utf16 <= 8
    }));
    assert!(inline_facts.iter().any(|fact| {
        fact.kind == INLINE_FACT_PROJECTION_EDIT_CELL
            && fact.flags & PROJECTION_EDIT_CELL_MATCHER_MASK
                == PROJECTION_EDIT_CELL_MATCH_EXACT_SPLICE_REPLACE_BLOCK_SHELL
    }));

    let plain_heading_start = source_text.find("# Plain heading").unwrap() as u64;
    let plain_heading_end = plain_heading_start + "# Plain heading\n".len() as u64;
    let plain_query = QueryRequest {
        range: SourceRange {
            start_byte: plain_heading_start,
            end_byte: plain_heading_end,
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &plain_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let plain_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    let plain_row = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert!(plain_row.inline_fact_count >= 1);
    assert_eq!(
        plain_header.payload_bytes,
        128 + plain_row.inline_fact_count * 80
    );
    let plain_facts = read_inline_facts(&page, plain_row.inline_fact_count);
    let plain_cell = plain_facts
        .iter()
        .find(|fact| {
            fact.kind == INLINE_FACT_PROJECTION_EDIT_CELL
                && fact.flags & PROJECTION_EDIT_CELL_MATCHER_MASK
                    == PROJECTION_EDIT_CELL_MATCH_ANY_NO_CRLF_SPLICE
        })
        .expect("plain heading content cell");
    assert_eq!(plain_cell.kind, INLINE_FACT_PROJECTION_EDIT_CELL);
    assert_eq!(
        plain_cell.flags,
        PROJECTION_EDIT_CELL_MATCH_ANY_NO_CRLF_SPLICE
            | PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL
            | PROJECTION_EDIT_CELL_PRESENT_EXACT
            | PROJECTION_EDIT_CELL_CHAIN_RESULT
    );

    let paragraph_start = source_text.find("A quick paragraph.").unwrap() as u64;
    let paragraph_end = paragraph_start + "A quick paragraph.\n".len() as u64;
    let paragraph_query = QueryRequest {
        range: SourceRange {
            start_byte: paragraph_start,
            end_byte: paragraph_end,
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &paragraph_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let paragraph_row = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    let paragraph_facts = read_inline_facts(&page, paragraph_row.inline_fact_count);
    let heading_plan = paragraph_facts
        .iter()
        .find(|fact| {
            fact.kind == INLINE_FACT_PROJECTION_EDIT_CELL
                && fact.flags & PROJECTION_EDIT_CELL_MATCHER_MASK
                    == PROJECTION_EDIT_CELL_MATCH_SIMPLE_BLOCK_PREFIX_PLAN
                && fact.replacement_first
                    == ((2_u32 << 24) | u32::from(b'#') | (u32::from(b' ') << 8))
        })
        .expect("plain paragraph heading prefix plan");
    assert_eq!(
        heading_plan.replacement_second & PROJECTION_EDIT_CELL_RESULT_SHELL_KIND_MASK,
        PROJECTION_EDIT_CELL_RESULT_SHELL_ATX_HEADING
    );
    assert_eq!(plain_cell.source_start_byte, plain_heading_start + 2);
    assert_eq!(plain_cell.source_end_byte, plain_heading_end - 1);
    assert_eq!(plain_cell.content_start_byte, plain_cell.source_start_byte);
    assert_eq!(plain_cell.content_end_byte, plain_cell.source_end_byte);
    assert_eq!(space_envelope.source_start_utf16, 9);
    assert_eq!(space_envelope.source_end_utf16, 9);

    let dense_query = QueryRequest {
        range: SourceRange {
            start_byte: dense_start as u64,
            end_byte: source.len() as u64,
        },
        budget: WorkBudget {
            max_result_items: 8,
            ..budget(64)
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &dense_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let dense_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(dense_header.item_count, 8);
    for index in 0..7 {
        let row = unsafe {
            page.as_ptr()
                .add(size_of::<ResultPageHeader>() + index * size_of::<ViewportRowRecord>())
                .cast::<ViewportRowRecord>()
                .read_unaligned()
        };
        assert_ne!(
            row.flags & VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE,
            0,
            "optional proof records must not evict row {index}'s Strong fact",
        );
        let semantic_record_count = row.inline_fact_count & VIEWPORT_ROW_INLINE_FACT_COUNT_MASK;
        assert!(
            semantic_record_count >= 1,
            "row {index} must retain at least its ordinary Strong fact",
        );
        if index == 0 {
            assert_eq!(
                semantic_record_count, 130,
                "the first dense row should retain its Strong fact, all 128 optional envelopes, and one bounded prefix-plan record",
            );
        }
        if index == 6 {
            assert!(
                semantic_record_count < 130,
                "the final dense row must prove optional records were truncated instead of evicting its Strong fact",
            );
        }
    }
    let tail_quote_row = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>() + 7 * size_of::<ViewportRowRecord>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_ne!(
        tail_quote_row.flags & VIEWPORT_ROW_FLAG_PROJECTED_RESERVED,
        0,
        "optional proof records must not evict a later row's required projection segments",
    );
    assert_eq!(
        tail_quote_row.inline_fact_count >> VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT,
        2,
    );

    let emoji_start = source_text.find('😀').expect("emoji byte offset");
    let unsnapped_end = emoji_start + 1;
    assert!(!source_text.is_char_boundary(unsnapped_end));
    let scalar_query = QueryRequest {
        range: SourceRange {
            start_byte: 0,
            end_byte: unsnapped_end as u64,
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &scalar_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::ResultCapReached as u32);
    let scalar_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(scalar_header.requested_range.start_byte, 0);
    assert_eq!(
        scalar_header.requested_range.end_byte, emoji_start as u64,
        "the ABI must publish the runtime's scalar-aligned effective end"
    );
    assert!(source_text.is_char_boundary(scalar_header.covered_range.end_byte as usize));
    assert_ne!(scalar_header.continuation, 0);

    let scalar_continuation = ContinuationRequest {
        struct_size: size_of::<ContinuationRequest>() as u32,
        flags: 0,
        session: commit.session,
        revision: scalar_header.revision,
        snapshot: scalar_header.snapshot,
        continuation: scalar_header.continuation,
        budget: scalar_query.budget,
        reserved: [0; 1],
    };
    page.fill(0);
    status = flark_v4_continuation_next(
        &scalar_continuation,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32);
    let scalar_next = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(scalar_next.requested_range, scalar_header.requested_range);
    assert!(source_text.is_char_boundary(scalar_next.covered_range.start_byte as usize));
    assert!(source_text.is_char_boundary(scalar_next.covered_range.end_byte as usize));
    if scalar_next.continuation != 0 {
        let release = ContinuationRequest {
            continuation: scalar_next.continuation,
            ..scalar_continuation
        };
        status = flark_v4_continuation_release(&release, &mut outcome);
        assert_eq!(status, StatusCode::Ok as u32);
    }

    let quote_start = source
        .windows(b"> first".len())
        .position(|window| window == b"> first")
        .expect("multiline quote offset");
    let quote_query = QueryRequest {
        query_kind: 4,
        range: SourceRange {
            start_byte: quote_start as u64,
            end_byte: source.len() as u64,
        },
        budget: WorkBudget {
            max_result_items: 1,
            ..budget(64)
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &quote_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32);
    let quote_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(quote_header.item_count, 1);
    assert_eq!(quote_header.payload_bytes, 128 + 2 * 32);
    let quote = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_ne!(quote.flags & VIEWPORT_ROW_FLAG_PROJECTED_RESERVED, 0);
    assert_eq!(
        quote.inline_fact_count & VIEWPORT_ROW_INLINE_FACT_COUNT_MASK,
        0
    );
    assert_eq!(
        quote.inline_fact_count >> VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT,
        2,
    );
    assert_eq!(quote.editable_start_byte, (quote_start + 2) as u64);
    assert_eq!(quote.editable_end_byte, (quote_start + 16) as u64);
    let segment_base = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>() + size_of::<ViewportRowRecord>())
            .cast::<ProjectionSegmentRecord>()
    };
    let first_segment = unsafe { segment_base.read_unaligned() };
    let second_segment = unsafe { segment_base.add(1).read_unaligned() };
    assert_eq!(
        first_segment.source_range,
        SourceRange {
            start_byte: (quote_start + 2) as u64,
            end_byte: (quote_start + 8) as u64,
        },
    );
    assert_eq!(
        second_segment.source_range,
        SourceRange {
            start_byte: (quote_start + 10) as u64,
            end_byte: (quote_start + 16) as u64,
        },
    );

    let table_start = source
        .windows(b"| left |".len())
        .position(|window| window == b"| left |")
        .expect("table offset");
    let table_query = QueryRequest {
        range: SourceRange {
            start_byte: table_start as u64,
            end_byte: source.len() as u64,
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &table_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert!(status == StatusCode::Ok as u32 || status == StatusCode::ResultCapReached as u32);
    let table_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(table_header.item_count, 1);
    let table = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_ne!(table.semantic_variant & VIEWPORT_ROW_TABLE_PRESENTATION, 0);
    assert_ne!(table.flags & VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE, 0);
    let table_fact_count = table.inline_fact_count & VIEWPORT_ROW_INLINE_FACT_COUNT_MASK;
    assert_eq!(table_fact_count, 12);
    let table_facts = unsafe {
        std::slice::from_raw_parts(
            page.as_ptr()
                .add(size_of::<ResultPageHeader>() + size_of::<ViewportRowRecord>())
                .cast::<InlineFactRecord>(),
            table_fact_count as usize,
        )
    };
    let semantic_cells = table_facts
        .iter()
        .filter(|fact| fact.kind == INLINE_FACT_TABLE_CELL)
        .collect::<Vec<_>>();
    assert_eq!(semantic_cells.len(), 4);
    let edit_cells = table_facts
        .iter()
        .filter(|fact| fact.kind == INLINE_FACT_PROJECTION_EDIT_CELL)
        .collect::<Vec<_>>();
    assert_eq!(edit_cells.len(), 8);
    assert_eq!(
        edit_cells
            .iter()
            .filter(|fact| {
                fact.flags & PROJECTION_EDIT_CELL_MATCHER_MASK
                    == PROJECTION_EDIT_CELL_MATCH_ASCII_LITERAL_SPLICE_IN_LITERAL
            })
            .count(),
        4
    );
    assert_eq!(
        edit_cells
            .iter()
            .filter(|fact| {
                fact.flags & PROJECTION_EDIT_CELL_MATCHER_MASK
                    == PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL
            })
            .count(),
        4
    );
    assert_eq!(
        edit_cells
            .iter()
            .filter(|fact| fact.flags & PROJECTION_EDIT_CELL_EMPTY_LITERAL_RESULT != 0)
            .count(),
        2
    );
    for fact in edit_cells {
        assert_ne!(fact.flags & PROJECTION_EDIT_CELL_RETAIN_BLOCK_SHELL, 0);
        assert_ne!(fact.flags & PROJECTION_EDIT_CELL_RETAIN_OUTSIDE, 0);
        assert_ne!(fact.flags & PROJECTION_EDIT_CELL_PRESENT_EXACT, 0);
        assert!(fact.source_start_byte < fact.source_end_byte);
        assert!(fact.content_start_byte >= fact.source_start_byte);
        assert!(fact.content_end_byte <= fact.source_end_byte);
        if fact.flags & PROJECTION_EDIT_CELL_EMPTY_LITERAL_RESULT != 0 {
            assert_eq!(
                fact.flags & PROJECTION_EDIT_CELL_MATCHER_MASK,
                PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL
            );
            assert_eq!(fact.replacement_first, 1);
        } else {
            assert_eq!(fact.replacement_first, 0);
        }
        assert_eq!(fact.replacement_second, 0);
    }

    let target_start = source
        .windows(b"[target]".len())
        .position(|window| window == b"[target]")
        .expect("target offset");
    let target_end = source[target_start..]
        .iter()
        .position(|byte| *byte == b')')
        .map(|offset| target_start + offset + 1)
        .expect("target end");
    let target_query = QueryRequest {
        query_kind: 5,
        range: SourceRange {
            start_byte: target_start as u64,
            end_byte: target_end as u64,
        },
        budget: budget(64),
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &target_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let target_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(target_header.item_count, 1);
    let target = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<SemanticTargetRecord>()
            .read_unaligned()
    };
    assert_eq!(target.kind, 1, "link target");
    assert_eq!(target.syntax, 3, "direct-link syntax");
    let value_start = size_of::<ResultPageHeader>() + size_of::<SemanticTargetRecord>();
    let destination = &page[value_start..value_start + target.destination_bytes as usize];
    let title = &page[value_start + target.destination_bytes as usize
        ..value_start + target.destination_bytes as usize + target.title_bytes as usize];
    assert_eq!(destination, b"https://example.com/path");
    assert_eq!(title, b"title");

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

    let global_request = InspectRequest {
        struct_size: size_of::<InspectRequest>() as u32,
        flags: INSPECT_FLAG_GLOBAL_LIVE_STATE,
        session: SessionRef::default(),
        reserved: [0; 5],
    };
    let mut global = SessionInspection::default();
    status = flark_v4_session_inspect(&global_request, &mut global, &mut outcome);
    assert_eq!(status, StatusCode::Ok as u32);
    assert_eq!(global.session_state, 0);
    assert_eq!(global.session, 0);
    assert_eq!(global.revision, 0);
    assert_eq!(global.live_transactions, 0);
    assert_eq!(global.live_continuations, 0);
    assert_eq!(global.live_anchors, 0);
    assert_eq!(global.live_history_tokens, 0);
    assert_eq!(global.reserved, [0; 3]);

    status = flark_v4_source_read(&read, page.as_mut_ptr(), page.len() as u64, &mut outcome);
    assert_eq!(status, StatusCode::InvalidHandle as u32);

    // The final D0 ABI carries a complete, nested plan as one optional record
    // group. Query the exact bounded seed through the raw C boundary so a
    // partial step/row stream cannot false-green via the higher-level decoder.
    let plan_source = b"change this line\n\n**sentinel**\n";
    let plan_owner = 72;
    let plan_create = CreateRequest {
        owner_token: plan_owner,
        expected_total_bytes: plan_source.len() as u64,
        ..create
    };
    status = flark_v4_create_begin(
        &plan_create,
        plan_source.as_ptr(),
        plan_source.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let plan_session = SessionRef {
        session: outcome.primary_handle,
        owner_token: plan_owner,
    };
    let plan_commit = TransactionRequest {
        session: plan_session,
        transaction: outcome.secondary_handle,
        budget: budget(64),
        ..commit
    };
    status = flark_v4_create_commit(&plan_commit, &mut outcome);
    let mut plan_token = outcome.progress_token;
    while status == StatusCode::BudgetExhausted as u32 {
        status = flark_v4_pump(
            &PumpRequest {
                struct_size: size_of::<PumpRequest>() as u32,
                flags: 0,
                session: plan_session,
                expected_revision: 1,
                progress_token: plan_token,
                budget: budget(64),
            },
            &mut outcome,
        );
        plan_token = outcome.progress_token;
    }
    assert_eq!(status, StatusCode::Ok as u32);

    let plan_query = QueryRequest {
        session: plan_session,
        revision: 1,
        range: SourceRange {
            start_byte: 0,
            end_byte: plan_source.len() as u64,
        },
        budget: WorkBudget {
            max_result_items: 8,
            ..budget(64)
        },
        ..query
    };
    page.fill(0);
    status = flark_v4_query_viewport(
        &plan_query,
        page.as_mut_ptr(),
        page.len() as u64,
        &mut outcome,
    );
    assert_eq!(status, StatusCode::Ok as u32);
    let plan_header = unsafe { page.as_ptr().cast::<ResultPageHeader>().read_unaligned() };
    assert_eq!(plan_header.item_count, 2);
    let plan_owner_row = unsafe {
        page.as_ptr()
            .add(size_of::<ResultPageHeader>())
            .cast::<ViewportRowRecord>()
            .read_unaligned()
    };
    assert_ne!(
        plan_owner_row.flags & VIEWPORT_ROW_FLAG_INLINE_AUTHORITATIVE,
        0
    );
    let plan_record_count = plan_owner_row.inline_fact_count & VIEWPORT_ROW_INLINE_FACT_COUNT_MASK;
    let plan_record_start = size_of::<ResultPageHeader>()
        + plan_header.item_count as usize * size_of::<ViewportRowRecord>();
    let plan_records = (0..plan_record_count as usize)
        .map(|index| unsafe {
            page.as_ptr()
                .add(plan_record_start + index * size_of::<InlineFactRecord>())
                .cast::<InlineFactRecord>()
                .read_unaligned()
        })
        .collect::<Vec<_>>();
    let plan_index = plan_records
        .iter()
        .position(|record| record.kind == INLINE_FACT_PENDING_PRESENTATION_PLAN)
        .expect("bounded plan header");
    let plan_record = &plan_records[plan_index];
    assert_eq!(
        plan_record.flags & PENDING_PRESENTATION_PLAN_SEQUENCE_LENGTH_MASK,
        8
    );
    assert_eq!(
        (plan_record.flags >> PENDING_PRESENTATION_PLAN_STEP_COUNT_SHIFT) & 0xff,
        8
    );
    assert_eq!(
        (plan_record.flags >> PENDING_PRESENTATION_PLAN_REPLACED_ROW_COUNT_SHIFT) & 0xff,
        2
    );
    assert_eq!(plan_record.replacement_first, u32::from_le_bytes(*b"```d"));
    assert_eq!(
        plan_record.replacement_second,
        u32::from_le_bytes(*b"art\n")
    );

    let mut cursor = plan_index + 1;
    for expected_prefix in 1..=8_u32 {
        let step = plan_records.get(cursor).expect("complete plan step");
        assert_eq!(step.kind, INLINE_FACT_PENDING_PRESENTATION_STEP);
        assert_eq!(
            step.flags & PENDING_PRESENTATION_STEP_PREFIX_LENGTH_MASK,
            expected_prefix
        );
        let row_count = (step.flags >> PENDING_PRESENTATION_STEP_ROW_COUNT_SHIFT) & 0xff;
        assert!((1..=4).contains(&row_count));
        cursor += 1;
        for _ in 0..row_count {
            let row = plan_records.get(cursor).expect("complete plan row");
            assert_eq!(row.kind, INLINE_FACT_PENDING_PRESENTATION_ROW);
            cursor += 1 + (row.flags >> PENDING_PRESENTATION_ROW_FACT_COUNT_SHIFT) as usize;
        }
    }
    assert!(cursor <= plan_records.len());

    let mut plan_close = CloseRequest {
        session: plan_session,
        progress_token: 0,
        budget: budget(64),
        ..close
    };
    status = flark_v4_close_begin(&plan_close, &mut outcome);
    while status == StatusCode::BudgetExhausted as u32 {
        plan_close.progress_token = outcome.progress_token;
        status = flark_v4_close_pump(&plan_close, &mut outcome);
    }
    assert_eq!(status, StatusCode::Ok as u32);
    plan_close.progress_token = outcome.progress_token;
    assert_eq!(
        flark_v4_close_finish(&plan_close, &mut outcome),
        StatusCode::Ok as u32
    );
}

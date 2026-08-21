use std::collections::BTreeSet;

use flark_runtime::{
    status_allows_progress, Affinity, CertificationState, ContinuationHandle, CoordinateKind,
    EditDescriptor, HistoryDisposition, HistoryToken, OperationCode, OperationResult, Outcome,
    OwnerToken, ProgressState, ProgressToken, QueryKind, ResultPageReceipt, ResultRecordKind,
    Revision, RuntimeContract, RuntimeRequest, SessionConfig, SessionHandle, SessionRef,
    SnapshotId, SourceRange, StatusCode, TransactionHandle, WorkBudget, CAPABILITY_BITS,
    HISTORY_DISPOSITIONS, MAX_BULK_CHUNK_BYTES, MAX_LIVE_ANCHORS, MAX_QUERY_ITEMS,
    MAX_RESULT_BYTES, MAX_SMALL_EDIT_BYTES, MAX_SOURCE_CHUNK_BYTES, MAX_TRANSACTION_EDITS,
    OPERATION_CODES, PARSER_PROFILES, PROGRESS_STATES, RESULT_RECORD_KINDS, STATUS_CODES,
};

fn assert_runtime_implementation_can_only_enter_through_the_contract<T: RuntimeContract>() {
    let _: fn(&mut T, RuntimeRequest<'_>, &mut [u8]) -> flark_runtime::Outcome = T::dispatch;
}

#[test]
fn contract_is_host_neutral_typed_and_bounded() {
    assert_eq!(MAX_SMALL_EDIT_BYTES, 4096);
    assert_eq!(MAX_BULK_CHUNK_BYTES, 65_536);
    assert_eq!(MAX_SOURCE_CHUNK_BYTES, 65_536);
    assert_eq!(MAX_RESULT_BYTES, 262_144);
    assert_eq!(MAX_QUERY_ITEMS, 4096);
    assert_eq!(MAX_TRANSACTION_EDITS, 64);
    assert_eq!(MAX_LIVE_ANCHORS, 4096);
    assert_eq!(CoordinateKind::SourceByte as u32, 1);
    assert_eq!(CoordinateKind::Utf16CodeUnit as u32, 2);
    assert_ne!(Affinity::Upstream as u32, Affinity::Downstream as u32);
    assert_ne!(OwnerToken(1), OwnerToken(2));

    let request = RuntimeRequest::CreateBegin {
        owner: OwnerToken(1),
        config: SessionConfig::default(),
        expected_total_bytes: 0,
        first_chunk: b"",
    };
    assert_eq!(request.operation(), OperationCode::CreateBegin);

    let pure_deletion = RuntimeRequest::BulkBegin {
        session: SessionRef {
            session: SessionHandle(1),
            owner: OwnerToken(1),
        },
        expected_revision: Revision(1),
        range: SourceRange {
            start_byte: 0,
            end_byte: 1,
        },
        expected_total_bytes: 0,
    };
    assert_eq!(pure_deletion.operation(), OperationCode::BulkBegin);
    let RuntimeRequest::BulkBegin {
        expected_total_bytes,
        ..
    } = pure_deletion
    else {
        unreachable!("constructed a BULK_BEGIN request")
    };
    assert_eq!(expected_total_bytes, 0);

    let _ = assert_runtime_implementation_can_only_enter_through_the_contract::<
        ContractIsIntentionallyUnimplemented,
    >;
}

enum ContractIsIntentionallyUnimplemented {}

impl RuntimeContract for ContractIsIntentionallyUnimplemented {
    fn dispatch(
        &mut self,
        _request: RuntimeRequest<'_>,
        _output: &mut [u8],
    ) -> flark_runtime::Outcome {
        match *self {}
    }
}

#[test]
fn code_tables_are_unique_and_exhaustive_snapshots() {
    assert_eq!(OPERATION_CODES.len(), 31);
    assert_eq!(STATUS_CODES.len(), 48);
    assert_eq!(CAPABILITY_BITS.len(), 28);
    assert_unique_u32(OPERATION_CODES);
    assert_unique_u32(STATUS_CODES);
    assert_unique_u32(PROGRESS_STATES);
    assert_unique_u32(PARSER_PROFILES);
    assert_unique_u32(RESULT_RECORD_KINDS);
    assert_unique_u32(HISTORY_DISPOSITIONS);
    assert_unique_u64(CAPABILITY_BITS);

    assert_eq!(OPERATION_CODES.first(), Some(&("NEGOTIATE", 0)));
    assert_eq!(
        OPERATION_CODES.last(),
        Some(&("STAGED_SOURCE_TRANSACTION_V1", 30))
    );
    assert_eq!(
        CAPABILITY_BITS.last(),
        Some(&("LITERAL_SAFE_ENVELOPE_CLOSURE_V1", 1 << 27))
    );
    assert_eq!(StatusCode::Ok as u32, 0);
    assert_eq!(StatusCode::ProgressStalled as u32, 0x0400);
    assert_eq!(StatusCode::PanicContained as u32, 0x0401);
}

#[test]
fn status_progress_pairs_are_runtime_enforced() {
    assert!(status_allows_progress(
        StatusCode::Ok,
        ProgressState::Complete
    ));
    assert!(status_allows_progress(
        StatusCode::NotCertified,
        ProgressState::Complete
    ));
    assert!(status_allows_progress(
        StatusCode::BufferTooSmall,
        ProgressState::None
    ));
    assert!(status_allows_progress(
        StatusCode::CloseIncomplete,
        ProgressState::None
    ));
    assert!(!status_allows_progress(
        StatusCode::NeedsInput,
        ProgressState::PendingSourceGap
    ));
    assert!(!status_allows_progress(
        StatusCode::CloseIncomplete,
        ProgressState::Advanced
    ));
    assert!(!status_allows_progress(
        StatusCode::SessionClosed,
        ProgressState::None
    ));
    assert!(!status_allows_progress(
        StatusCode::HistoryBudgetExceeded,
        ProgressState::None
    ));
}

#[test]
fn budget_admission_is_nonzero_bounded_and_page_safe() {
    let valid = WorkBudget {
        max_work_units: 1,
        advisory_max_micros: 0,
        max_result_items: 1,
        max_result_bytes: 1,
    };
    assert!(valid.is_contract_valid());
    assert!(valid.is_page_contract_valid());
    assert!(!WorkBudget {
        max_work_units: 0,
        ..valid
    }
    .is_contract_valid());
    assert!(!WorkBudget {
        max_result_items: MAX_QUERY_ITEMS + 1,
        ..valid
    }
    .is_contract_valid());
    assert!(!WorkBudget {
        max_result_bytes: MAX_RESULT_BYTES + 1,
        ..valid
    }
    .is_contract_valid());
    assert!(!WorkBudget {
        max_result_items: 0,
        ..valid
    }
    .is_page_contract_valid());
    assert!(!WorkBudget {
        max_result_bytes: 0,
        ..valid
    }
    .is_page_contract_valid());

    let session = SessionRef {
        session: SessionHandle(1),
        owner: OwnerToken(2),
    };
    let query = RuntimeRequest::QueryViewport {
        session,
        revision: Revision(3),
        snapshot: SnapshotId::NOT_APPLICABLE,
        range: SourceRange {
            start_byte: 0,
            end_byte: 0,
        },
        kind: QueryKind::Source,
        budget: valid,
    };
    assert!(query.budget_is_contract_valid());
    let zero_work_query = RuntimeRequest::QueryViewport {
        session,
        revision: Revision(3),
        snapshot: SnapshotId::NOT_APPLICABLE,
        range: SourceRange {
            start_byte: 0,
            end_byte: 0,
        },
        kind: QueryKind::Source,
        budget: WorkBudget {
            max_work_units: 0,
            ..valid
        },
    };
    assert!(!zero_work_query.budget_is_contract_valid());
}

#[test]
fn small_edit_envelope_counts_deleted_source_and_rejects_hidden_bulk_work() {
    let session = SessionRef {
        session: SessionHandle(1),
        owner: OwnerToken(2),
    };
    let budget = WorkBudget {
        max_work_units: 1,
        advisory_max_micros: 0,
        max_result_items: 0,
        max_result_bytes: 0,
    };
    let replacement = b"new";
    let valid_edit = [EditDescriptor {
        start_byte: 10,
        end_byte: 13,
        replacement_offset: 0,
        replacement_len: replacement.len() as u64,
    }];
    let valid = RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &valid_edit,
        replacement_bytes: replacement,
        budget,
    };
    assert!(valid.small_edit_envelope_is_contract_valid());

    let exact_cap = [EditDescriptor {
        start_byte: 0,
        end_byte: u64::from(MAX_SMALL_EDIT_BYTES) - 32,
        replacement_offset: 0,
        replacement_len: 0,
    }];
    let exact_cap_request = RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &exact_cap,
        replacement_bytes: b"",
        budget,
    };
    assert!(exact_cap_request.small_edit_envelope_is_contract_valid());

    let over_cap = [EditDescriptor {
        end_byte: u64::from(MAX_SMALL_EDIT_BYTES) - 31,
        ..exact_cap[0]
    }];
    let over_cap_request = RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &over_cap,
        replacement_bytes: b"",
        budget,
    };
    assert!(!over_cap_request.small_edit_envelope_is_contract_valid());

    let document_sized_delete = [EditDescriptor {
        start_byte: 0,
        end_byte: 10 * 1024 * 1024,
        replacement_offset: 0,
        replacement_len: 0,
    }];
    let invalid = RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &document_sized_delete,
        replacement_bytes: b"",
        budget,
    };
    assert!(!invalid.small_edit_envelope_is_contract_valid());

    let unsorted = [
        EditDescriptor {
            start_byte: 10,
            end_byte: 11,
            replacement_offset: 0,
            replacement_len: 0,
        },
        EditDescriptor {
            start_byte: 9,
            end_byte: 10,
            replacement_offset: 0,
            replacement_len: 0,
        },
    ];
    let invalid = RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &unsorted,
        replacement_bytes: b"",
        budget,
    };
    assert!(!invalid.small_edit_envelope_is_contract_valid());

    let packed_replacement = b"abcdef";
    let packed_edits = [
        EditDescriptor {
            start_byte: 0,
            end_byte: 0,
            replacement_offset: 0,
            replacement_len: 2,
        },
        EditDescriptor {
            start_byte: 1,
            end_byte: 1,
            replacement_offset: 2,
            replacement_len: 4,
        },
    ];
    assert!(RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &packed_edits,
        replacement_bytes: packed_replacement,
        budget,
    }
    .small_edit_envelope_is_contract_valid());

    let reused_slice = [
        EditDescriptor {
            replacement_offset: 0,
            replacement_len: 3,
            ..packed_edits[0]
        },
        EditDescriptor {
            replacement_offset: 0,
            replacement_len: 3,
            ..packed_edits[1]
        },
    ];
    assert!(!RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &reused_slice,
        replacement_bytes: packed_replacement,
        budget,
    }
    .small_edit_envelope_is_contract_valid());

    let gapped_slices = [
        EditDescriptor {
            replacement_offset: 0,
            replacement_len: 2,
            ..packed_edits[0]
        },
        EditDescriptor {
            replacement_offset: 3,
            replacement_len: 3,
            ..packed_edits[1]
        },
    ];
    assert!(!RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &gapped_slices,
        replacement_bytes: packed_replacement,
        budget,
    }
    .small_edit_envelope_is_contract_valid());

    let trailing_unreferenced_bytes = [EditDescriptor {
        replacement_offset: 0,
        replacement_len: 5,
        ..packed_edits[0]
    }];
    assert!(!RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &trailing_unreferenced_bytes,
        replacement_bytes: packed_replacement,
        budget,
    }
    .small_edit_envelope_is_contract_valid());

    let reused_large_slice = [EditDescriptor {
        start_byte: 0,
        end_byte: 0,
        replacement_offset: 0,
        replacement_len: 2048,
    }; MAX_TRANSACTION_EDITS as usize];
    assert!(!RuntimeRequest::SmallEdit {
        session,
        expected_revision: Revision(3),
        edits: &reused_large_slice,
        replacement_bytes: &[b'x'; 2048],
        budget,
    }
    .small_edit_envelope_is_contract_valid());
}

#[test]
fn outcome_validation_rejects_every_impossible_status_progress_and_result_shape() {
    let committed = OperationResult::RevisionCommitted {
        revision: Revision(2),
        history_token: HistoryToken::NONE,
        history: HistoryDisposition::Disabled,
    };
    let progress = OperationResult::Progress {
        revision: Revision(1),
        token: ProgressToken(10),
    };
    let query_page = ResultPageReceipt {
        record_kind: ResultRecordKind::SemanticFacts,
        certification: CertificationState::CurrentCertified,
        revision: Revision(1),
        snapshot: SnapshotId(2),
        requested_range: SourceRange {
            start_byte: 0,
            end_byte: 1,
        },
        covered_range: SourceRange {
            start_byte: 0,
            end_byte: 1,
        },
        item_count: 1,
        payload_bytes: 1,
        continuation: ContinuationHandle::NONE,
    };
    let pending_page = ResultPageReceipt {
        record_kind: ResultRecordKind::SourceBytes,
        certification: CertificationState::PendingNeutral,
        item_count: 0,
        ..query_page
    };
    let source_page = ResultPageReceipt {
        record_kind: ResultRecordKind::SourceBytes,
        certification: CertificationState::NotApplicable,
        snapshot: SnapshotId::NOT_APPLICABLE,
        item_count: 0,
        ..query_page
    };

    let valid = [
        Outcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: committed,
        },
        Outcome {
            operation: OperationCode::HistoryReplay,
            status: StatusCode::Backpressure,
            progress: ProgressState::Backpressured,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        },
        Outcome {
            operation: OperationCode::BulkCommit,
            status: StatusCode::BudgetExhausted,
            progress: ProgressState::BudgetExhausted,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: progress,
        },
        Outcome {
            operation: OperationCode::QueryViewport,
            status: StatusCode::NotCertified,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(pending_page),
        },
        Outcome {
            operation: OperationCode::SourceRead,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(source_page),
        },
        Outcome {
            operation: OperationCode::QueryViewport,
            status: StatusCode::ResultCapReached,
            progress: ProgressState::ResultCapReached,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(ResultPageReceipt {
                continuation: ContinuationHandle(3),
                ..query_page
            }),
        },
    ];
    assert!(valid.into_iter().all(Outcome::is_contract_valid));

    let impossible = [
        // Success must carry the operation's terminal receipt.
        Outcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        },
        // Error outcomes cannot smuggle a committed revision.
        Outcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::InvalidArgument,
            progress: ProgressState::None,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: committed,
        },
        // A non-create progress result must name its active revision.
        Outcome {
            operation: OperationCode::BulkCommit,
            status: StatusCode::BudgetExhausted,
            progress: ProgressState::BudgetExhausted,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::Progress {
                revision: Revision::UNCOMMITTED,
                token: ProgressToken(10),
            },
        },
        // OK query pages must not claim pending-neutral certification.
        Outcome {
            operation: OperationCode::QueryViewport,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(pending_page),
        },
        // NOT_CERTIFIED must be the precise pending-neutral source page.
        Outcome {
            operation: OperationCode::QueryViewport,
            status: StatusCode::NotCertified,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(query_page),
        },
        // SOURCE_READ cannot borrow a pinned query snapshot shape.
        Outcome {
            operation: OperationCode::SourceRead,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(ResultPageReceipt {
                record_kind: ResultRecordKind::SourceBytes,
                certification: CertificationState::NotApplicable,
                item_count: 0,
                ..query_page
            }),
        },
        // A capped page must return a resumable continuation.
        Outcome {
            operation: OperationCode::QueryViewport,
            status: StatusCode::ResultCapReached,
            progress: ProgressState::ResultCapReached,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(query_page),
        },
        // An ordinary OK page must not retain a continuation.
        Outcome {
            operation: OperationCode::QueryViewport,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 1,
            result: OperationResult::Page(ResultPageReceipt {
                continuation: ContinuationHandle(3),
                ..query_page
            }),
        },
        // Close completion consumes its token instead of returning it.
        Outcome {
            operation: OperationCode::CloseFinish,
            status: StatusCode::Ok,
            progress: ProgressState::Complete,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::CloseProgress {
                token: ProgressToken(10),
            },
        },
        // ADVANCED means resumable progress, never a terminal commit receipt.
        Outcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::Ok,
            progress: ProgressState::Advanced,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: committed,
        },
        // Reserved numeric codes have no v1 runtime outcome.
        Outcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::SessionClosed,
            progress: ProgressState::None,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        },
        Outcome {
            operation: OperationCode::SmallEdit,
            status: StatusCode::HistoryBudgetExceeded,
            progress: ProgressState::None,
            required_payload_bytes: 0,
            written_payload_bytes: 0,
            result: OperationResult::None,
        },
    ];
    assert!(impossible
        .into_iter()
        .all(|outcome| !outcome.is_contract_valid()));
}

#[test]
fn header_only_page_buffer_requirements_are_contract_valid() {
    let header_only = flark_runtime::Outcome {
        operation: OperationCode::SourceRead,
        status: StatusCode::BufferTooSmall,
        progress: ProgressState::None,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: flark_runtime::OperationResult::None,
    };
    assert!(header_only.is_contract_valid());
    assert!(!flark_runtime::Outcome {
        operation: OperationCode::SmallEdit,
        ..header_only
    }
    .is_contract_valid());
    assert!(OperationCode::SourceRead.produces_result_page());
    assert!(!OperationCode::SmallEdit.produces_result_page());
}

#[test]
fn page_receipts_are_typed_and_reject_cross_layer_mismatches() {
    let source = ResultPageReceipt {
        record_kind: ResultRecordKind::SourceBytes,
        certification: CertificationState::NotApplicable,
        revision: Revision(7),
        snapshot: SnapshotId::NOT_APPLICABLE,
        requested_range: SourceRange {
            start_byte: 100,
            end_byte: 104,
        },
        covered_range: SourceRange {
            start_byte: 100,
            end_byte: 104,
        },
        item_count: 0,
        payload_bytes: 4,
        continuation: ContinuationHandle::NONE,
    };
    assert!(source.is_contract_valid());

    let pending = ResultPageReceipt {
        certification: CertificationState::PendingNeutral,
        snapshot: SnapshotId(9),
        ..source
    };
    assert!(pending.is_contract_valid());

    let certified = ResultPageReceipt {
        record_kind: ResultRecordKind::SemanticFacts,
        certification: CertificationState::CurrentCertified,
        snapshot: SnapshotId(9),
        item_count: 2,
        payload_bytes: 48,
        ..source
    };
    assert!(certified.is_contract_valid());

    assert!(!ResultPageReceipt {
        revision: Revision::UNCOMMITTED,
        ..certified
    }
    .is_contract_valid());
    assert!(!ResultPageReceipt {
        snapshot: SnapshotId::NOT_APPLICABLE,
        ..certified
    }
    .is_contract_valid());
    assert!(!ResultPageReceipt {
        covered_range: SourceRange {
            start_byte: 99,
            end_byte: 104,
        },
        ..source
    }
    .is_contract_valid());
    assert!(!ResultPageReceipt {
        continuation: ContinuationHandle(12),
        ..source
    }
    .is_contract_valid());
}

#[test]
fn exhaustive_runtime_requests_preserve_bounded_commit_release_and_close_fields() {
    let session = SessionRef {
        session: SessionHandle(11),
        owner: OwnerToken(12),
    };
    let transaction = TransactionHandle(13);
    let revision = Revision(14);
    let snapshot = SnapshotId(15);
    let budget = WorkBudget {
        max_work_units: 16,
        advisory_max_micros: 17,
        max_result_items: 18,
        max_result_bytes: 19,
    };

    let RuntimeRequest::CreateCommit {
        session: preserved_session,
        transaction: preserved_transaction,
        progress_token,
        budget: preserved_budget,
    } = (RuntimeRequest::CreateCommit {
        session,
        transaction,
        progress_token: ProgressToken::NONE,
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(preserved_session, session);
    assert_eq!(preserved_transaction, transaction);
    assert_eq!(progress_token, ProgressToken::NONE);
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::BulkAbort {
        expected_revision,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::BulkAbort {
        session,
        transaction,
        expected_revision: revision,
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(expected_revision, revision);
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::ContinuationRelease {
        revision: preserved_revision,
        snapshot: preserved_snapshot,
        continuation,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::ContinuationRelease {
        session,
        revision,
        snapshot,
        continuation: ContinuationHandle(20),
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(preserved_revision, revision);
    assert_eq!(preserved_snapshot, snapshot);
    assert_eq!(continuation, ContinuationHandle(20));
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::HistoryRelease {
        expected_revision,
        token,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::HistoryRelease {
        session,
        expected_revision: revision,
        token: HistoryToken(21),
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(expected_revision, revision);
    assert_eq!(token, HistoryToken(21));
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::CloseFinish {
        progress_token,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::CloseFinish {
        session,
        progress_token: ProgressToken(22),
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(progress_token, ProgressToken(22));
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::Pump {
        expected_revision,
        progress_token,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::Pump {
        session,
        expected_revision: revision,
        progress_token: ProgressToken(23),
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(expected_revision, revision);
    assert_eq!(progress_token, ProgressToken(23));
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::AnchorCreate {
        progress_token,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::AnchorCreate {
        session,
        revision,
        position: 24,
        coordinate: CoordinateKind::Utf16CodeUnit,
        affinity: Affinity::Downstream,
        progress_token: ProgressToken(25),
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(progress_token, ProgressToken(25));
    assert_eq!(preserved_budget, budget);

    let RuntimeRequest::CoordinateConvert {
        progress_token,
        budget: preserved_budget,
        ..
    } = (RuntimeRequest::CoordinateConvert {
        session,
        revision,
        position: 26,
        from: CoordinateKind::SourceByte,
        to: CoordinateKind::Utf16CodeUnit,
        progress_token: ProgressToken(27),
        budget,
    })
    else {
        unreachable!()
    };
    assert_eq!(progress_token, ProgressToken(27));
    assert_eq!(preserved_budget, budget);
}

fn assert_unique_u32(values: &[(&str, u32)]) {
    let names = values
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    let codes = values
        .iter()
        .map(|(_, code)| *code)
        .collect::<BTreeSet<_>>();
    assert_eq!(names.len(), values.len());
    assert_eq!(codes.len(), values.len());
}

fn assert_unique_u64(values: &[(&str, u64)]) {
    let names = values
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    let codes = values
        .iter()
        .map(|(_, code)| *code)
        .collect::<BTreeSet<_>>();
    assert_eq!(names.len(), values.len());
    assert_eq!(codes.len(), values.len());
}

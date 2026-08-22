use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    mem::{align_of, offset_of, size_of},
    path::PathBuf,
    process::Command,
};

use flark_abi::{
    AbiInfo, AnchorRequest, CertificationRangeRecord, EditDescriptor, EditIntentReceiptV1,
    EditIntentRequestV1, InlineFactRecord, Outcome, QueryRequest, RequestFieldRuleKind,
    ResultPageHeader, SessionInspection, SmallEditRequest, StagedSourceTransactionRequestV1,
    ABI_MAJOR, ABI_MINOR, AFFINITIES, CAPABILITY_BITS, CERTIFICATION_STATES, COORDINATE_KINDS,
    HANDLE_KINDS, HISTORY_DISPOSITIONS, OPERATION_CODES, OWNERSHIP_KINDS, PARSER_PROFILES,
    PROGRESS_STATES, QUERY_KINDS, RECORD_LAYOUTS, REQUEST_FIELD_RULES, RESULT_RECORD_KINDS,
    SESSION_STATES, STATUS_CODES, TRANSACTION_STATES,
};
use flark_runtime::{
    AnchorHandle, CertificationState, ContinuationHandle, CoordinateKind, HistoryDisposition,
    HistoryToken, OperationCode, OperationResult, ProgressState, ResultPageReceipt,
    ResultRecordKind, Revision, SessionHandle, SessionInspectionReceipt, SessionState, SnapshotId,
    SourceRange as RuntimeSourceRange, StatusCode,
};
use serde_json::Value;

const HEADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../include/flark_v4.h"
));
const MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../test/fixtures/v4/runtime_abi_v1.json"
));

#[test]
fn fixed_width_records_match_the_frozen_layout() {
    assert_eq!(align_of::<AbiInfo>(), 8);
    assert_eq!(size_of::<AbiInfo>(), 64);
    assert_eq!(offset_of!(AbiInfo, capability_bits), 8);
    assert_eq!(size_of::<Outcome>(), 112);
    assert_eq!(offset_of!(Outcome, status), 8);
    assert_eq!(offset_of!(Outcome, primary_handle), 16);
    assert_eq!(offset_of!(Outcome, reserved), 80);
    assert_eq!(size_of::<ResultPageHeader>(), 96);
    assert_eq!(offset_of!(ResultPageHeader, revision), 16);
    assert_eq!(offset_of!(ResultPageHeader, requested_range), 32);
    assert_eq!(offset_of!(ResultPageHeader, covered_range), 48);
    assert_eq!(offset_of!(ResultPageHeader, item_count), 64);
    assert_eq!(offset_of!(ResultPageHeader, continuation), 72);
    assert_eq!(size_of::<CertificationRangeRecord>(), 40);
    assert_eq!(offset_of!(CertificationRangeRecord, source_range), 8);
    assert_eq!(size_of::<InlineFactRecord>(), 80);
    assert_eq!(offset_of!(InlineFactRecord, source_start_byte), 8);
    assert_eq!(offset_of!(InlineFactRecord, content_start_byte), 40);
    assert_eq!(offset_of!(InlineFactRecord, replacement_first), 72);
    assert_eq!(offset_of!(InlineFactRecord, replacement_second), 76);
    assert_eq!(size_of::<EditDescriptor>(), 32);
    assert_eq!(size_of::<SmallEditRequest>(), 88);
    assert_eq!(offset_of!(SmallEditRequest, edit_count), 32);
    assert_eq!(offset_of!(SmallEditRequest, replacement_bytes_len), 40);
    assert_eq!(offset_of!(SmallEditRequest, budget), 48);
    assert_eq!(size_of::<EditIntentRequestV1>(), 128);
    assert_eq!(offset_of!(EditIntentRequestV1, logical_edit_id), 48);
    assert_eq!(offset_of!(EditIntentRequestV1, budget), 96);
    assert_eq!(size_of::<EditIntentReceiptV1>(), 192);
    assert_eq!(offset_of!(EditIntentReceiptV1, base_byte_range), 48);
    assert_eq!(offset_of!(EditIntentReceiptV1, history_token), 160);
    assert_eq!(size_of::<StagedSourceTransactionRequestV1>(), 160);
    assert_eq!(
        offset_of!(StagedSourceTransactionRequestV1, result_selection_utf16),
        96
    );
    assert_eq!(size_of::<QueryRequest>(), 96);
    assert_eq!(offset_of!(QueryRequest, budget), 64);
    assert_eq!(size_of::<AnchorRequest>(), 96);
    assert_eq!(offset_of!(AnchorRequest, progress_token), 64);
    assert_eq!(offset_of!(AnchorRequest, budget), 72);
    assert_eq!(size_of::<SessionInspection>(), 64);
    assert_eq!(offset_of!(SessionInspection, live_transactions), 24);
}

#[test]
fn manifest_rust_and_header_code_tables_are_identical() {
    let manifest = manifest();
    assert_eq!(
        manifest["abi"]["major"].as_u64(),
        Some(u64::from(ABI_MAJOR))
    );
    assert_eq!(
        manifest["abi"]["minor"].as_u64(),
        Some(u64::from(ABI_MINOR))
    );

    assert_u32_table(&manifest, "statuses", STATUS_CODES, "FLARK_V4_STATUS_");
    assert_u32_table(
        &manifest,
        "operations",
        OPERATION_CODES,
        "FLARK_V4_OPERATION_",
    );
    assert_u32_table(
        &manifest,
        "progressStates",
        PROGRESS_STATES,
        "FLARK_V4_PROGRESS_",
    );
    assert_u32_table(
        &manifest,
        "sessionStates",
        SESSION_STATES,
        "FLARK_V4_SESSION_",
    );
    assert_u32_table(
        &manifest,
        "transactionStates",
        TRANSACTION_STATES,
        "FLARK_V4_TRANSACTION_",
    );
    assert_u32_table(
        &manifest,
        "certificationStates",
        CERTIFICATION_STATES,
        "FLARK_V4_CERTIFICATION_",
    );
    assert_u32_table(
        &manifest,
        "parserProfiles",
        PARSER_PROFILES,
        "FLARK_V4_PARSER_PROFILE_",
    );
    assert_u32_table(
        &manifest,
        "coordinateKinds",
        COORDINATE_KINDS,
        "FLARK_V4_COORDINATE_",
    );
    assert_u32_table(&manifest, "affinities", AFFINITIES, "FLARK_V4_AFFINITY_");
    assert_u32_table(&manifest, "queryKinds", QUERY_KINDS, "FLARK_V4_QUERY_");
    assert_u32_table(
        &manifest,
        "resultRecordKinds",
        RESULT_RECORD_KINDS,
        "FLARK_V4_RESULT_RECORD_",
    );
    assert_u32_table(
        &manifest,
        "historyDispositions",
        HISTORY_DISPOSITIONS,
        "FLARK_V4_HISTORY_",
    );
    assert_u32_table(&manifest, "handleKinds", HANDLE_KINDS, "FLARK_V4_HANDLE_");
    assert_u32_table(
        &manifest,
        "ownershipKinds",
        OWNERSHIP_KINDS,
        "FLARK_V4_OWNERSHIP_",
    );
    assert_u64_table(
        &manifest,
        "capabilities",
        CAPABILITY_BITS,
        "FLARK_V4_CAPABILITY_",
        "bit",
    );

    let record_manifest = named_numbers(&manifest, "records", "sizeBytes");
    let rust_records = RECORD_LAYOUTS
        .iter()
        .map(|(name, size)| ((*name).to_owned(), *size as u64))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(record_manifest, rust_records);
    for (name, size) in &record_manifest {
        assert_eq!(
            header_macro(&format!("FLARK_V4_SIZEOF_{name}")),
            *size,
            "header size for {name}"
        );
    }
}

#[test]
fn operations_name_real_header_symbols_and_known_request_records() {
    let manifest = manifest();
    let records = named_numbers(&manifest, "records", "sizeBytes")
        .into_keys()
        .collect::<BTreeSet<_>>();
    let operations = manifest["operations"].as_array().expect("operations array");
    assert_eq!(operations.len(), 31);
    for operation in operations {
        let symbol = operation["symbol"].as_str().expect("operation symbol");
        let request = operation["requestRecord"]
            .as_str()
            .expect("operation request record");
        assert!(HEADER.contains(&format!("{symbol}(")), "missing {symbol}");
        assert!(
            records.contains(request),
            "unknown request record {request}"
        );
        assert!(operation["allowedSessionStates"]
            .as_array()
            .is_some_and(|states| !states.is_empty()));
        if let Some(output) = operation["outputRecord"].as_str() {
            assert!(records.contains(output), "unknown output record {output}");
        }
    }

    for name in ["SOURCE_READ", "QUERY_VIEWPORT", "CONTINUATION_NEXT"] {
        let operation = operations
            .iter()
            .find(|operation| operation["name"].as_str() == Some(name))
            .expect("page-producing operation");
        assert_eq!(
            operation["outputRecord"].as_str(),
            Some("RESULT_PAGE_HEADER")
        );
    }
}

#[test]
fn every_previously_ambiguous_outcome_has_one_explicit_discriminant() {
    let manifest = manifest();
    let statuses = named_numbers(&manifest, "statuses", "code")
        .into_keys()
        .collect::<BTreeSet<_>>();
    let required = BTreeSet::from([
        "ALLOCATION_FAILURE",
        "BACKPRESSURE",
        "BUDGET_EXHAUSTED",
        "BUFFER_TOO_SMALL",
        "INTERNAL_FAULT",
        "INVALID_UTF16_HOST_INPUT",
        "NOT_READY_SOURCE_GAP",
        "PANIC_CONTAINED",
        "PARSER_FAULT",
        "PROGRESS_STALLED",
        "RESULT_CAP_REACHED",
        "STALE_PROGRESS_TOKEN",
    ]);
    assert!(required.iter().all(|name| statuses.contains(*name)));

    let coverage = manifest["ambiguousOutcomeCoverage"]
        .as_array()
        .expect("ambiguous outcome coverage");
    let covered = coverage
        .iter()
        .map(|entry| entry["status"].as_str().expect("coverage status"))
        .collect::<BTreeSet<_>>();
    assert!(required.is_subset(&covered));
}

#[test]
fn host_neutral_page_receipt_is_the_only_page_header_authority() {
    let receipt = ResultPageReceipt {
        record_kind: ResultRecordKind::SourceBytes,
        certification: CertificationState::PendingNeutral,
        revision: Revision(41),
        snapshot: SnapshotId(42),
        requested_range: RuntimeSourceRange {
            start_byte: 10,
            end_byte: 20,
        },
        covered_range: RuntimeSourceRange {
            start_byte: 12,
            end_byte: 16,
        },
        item_count: 0,
        payload_bytes: 4,
        continuation: ContinuationHandle(43),
    };
    let header = ResultPageHeader::from_runtime(receipt).expect("valid runtime page receipt");
    assert_eq!(header.struct_size as usize, size_of::<ResultPageHeader>());
    assert_eq!(header.abi_major, ABI_MAJOR);
    assert_eq!(header.abi_minor, ABI_MINOR);
    assert_eq!(header.record_kind, ResultRecordKind::SourceBytes as u32);
    assert_eq!(
        header.certification_state,
        CertificationState::PendingNeutral as u32
    );
    assert_eq!(header.revision, 41);
    assert_eq!(header.snapshot, 42);
    assert_eq!(header.requested_range.start_byte, 10);
    assert_eq!(header.requested_range.end_byte, 20);
    assert_eq!(header.covered_range.start_byte, 12);
    assert_eq!(header.covered_range.end_byte, 16);
    assert_eq!(header.item_count, 0);
    assert_eq!(header.payload_bytes, 4);
    assert_eq!(header.continuation, 43);
    assert_eq!(header.reserved, [0; 2]);

    assert!(ResultPageHeader::from_runtime(ResultPageReceipt {
        revision: Revision::UNCOMMITTED,
        ..receipt
    })
    .is_none());
}

#[test]
fn typed_runtime_results_have_one_frozen_c_outcome_mapping() {
    let committed = flark_runtime::Outcome {
        operation: OperationCode::SmallEdit,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::RevisionCommitted {
            revision: Revision(51),
            history_token: HistoryToken(52),
            history: HistoryDisposition::Retained,
        },
    };
    let encoded = Outcome::from_runtime(committed).expect("valid committed result");
    assert_eq!(encoded.struct_size as usize, size_of::<Outcome>());
    assert_eq!(encoded.operation, OperationCode::SmallEdit as u32);
    assert_eq!(encoded.status, StatusCode::Ok as u32);
    assert_eq!(encoded.progress_state, ProgressState::Complete as u32);
    assert_eq!(encoded.primary_handle, 52);
    assert_eq!(encoded.secondary_handle, 0);
    assert_eq!(encoded.revision, 51);
    assert_eq!(encoded.snapshot, 0);
    assert_eq!(encoded.progress_token, 0);
    assert_eq!(encoded.detail_code, HistoryDisposition::Retained as u64);
    assert_eq!(encoded.reserved, [0; 4]);

    let resolved = Outcome::from_runtime(flark_runtime::Outcome {
        operation: OperationCode::AnchorResolve,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::AnchorPosition {
            anchor: AnchorHandle(61),
            revision: Revision(62),
            coordinate: CoordinateKind::Utf16CodeUnit,
            position: 63,
        },
    })
    .expect("valid anchor result");
    assert_eq!(resolved.primary_handle, 61);
    assert_eq!(resolved.revision, 62);
    assert_eq!(resolved.detail_code, 63);

    let inspection_receipt = SessionInspectionReceipt {
        session: SessionHandle(71),
        state: SessionState::Closing,
        revision: Revision(72),
        live_transactions: 1,
        live_continuations: 2,
        live_anchors: 3,
        live_history_tokens: 4,
    };
    let inspection = SessionInspection::from_runtime(inspection_receipt);
    assert_eq!(
        inspection.struct_size as usize,
        size_of::<SessionInspection>()
    );
    assert_eq!(inspection.session_state, SessionState::Closing as u32);
    assert_eq!(inspection.session, 71);
    assert_eq!(inspection.revision, 72);
    assert_eq!(inspection.live_transactions, 1);
    assert_eq!(inspection.live_continuations, 2);
    assert_eq!(inspection.live_anchors, 3);
    assert_eq!(inspection.live_history_tokens, 4);
    assert_eq!(inspection.reserved, [0; 3]);

    let source_page = ResultPageReceipt {
        record_kind: ResultRecordKind::SourceBytes,
        certification: CertificationState::NotApplicable,
        revision: Revision(81),
        snapshot: SnapshotId::NOT_APPLICABLE,
        requested_range: RuntimeSourceRange {
            start_byte: 0,
            end_byte: 4,
        },
        covered_range: RuntimeSourceRange {
            start_byte: 0,
            end_byte: 4,
        },
        item_count: 0,
        payload_bytes: 4,
        continuation: ContinuationHandle::NONE,
    };
    let page_outcome = Outcome::from_runtime(flark_runtime::Outcome {
        operation: OperationCode::SourceRead,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: 4,
        result: OperationResult::Page(source_page),
    })
    .expect("valid page outcome");
    assert_eq!(page_outcome.written_bytes, 100);
    assert_eq!(page_outcome.revision, 81);
    assert_eq!(page_outcome.snapshot, 0);

    let header_only_requirement = Outcome::from_runtime(flark_runtime::Outcome {
        operation: OperationCode::SourceRead,
        status: StatusCode::BufferTooSmall,
        progress: ProgressState::None,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::None,
    })
    .expect("valid header-only page requirement");
    assert_eq!(header_only_requirement.required_bytes, 96);
    assert_eq!(header_only_requirement.written_bytes, 0);

    assert!(Outcome::from_runtime(flark_runtime::Outcome {
        operation: OperationCode::SmallEdit,
        status: StatusCode::BufferTooSmall,
        progress: ProgressState::None,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::None,
    })
    .is_none());

    assert!(Outcome::from_runtime(flark_runtime::Outcome {
        operation: OperationCode::SourceRead,
        status: StatusCode::Ok,
        progress: ProgressState::Complete,
        required_payload_bytes: 0,
        written_payload_bytes: 0,
        result: OperationResult::Anchor {
            anchor: AnchorHandle(91),
            revision: Revision(92),
        },
    })
    .is_none());

    let manifest = manifest();
    let rows = manifest["outcomeFieldRoles"]
        .as_array()
        .expect("outcomeFieldRoles array");
    let operations = rows
        .iter()
        .map(|row| row["operation"].as_str().expect("outcome operation"))
        .collect::<BTreeSet<_>>();
    assert_eq!(operations.len(), OPERATION_CODES.len());
    assert!(OPERATION_CODES
        .iter()
        .all(|(name, _)| operations.contains(name)));
}

#[test]
fn operation_specific_zero_rules_match_the_manifest() {
    let manifest = manifest();
    let manifest_rules = manifest["requestFieldRules"]
        .as_array()
        .expect("requestFieldRules array")
        .iter()
        .map(|entry| {
            (
                entry["operation"].as_str().expect("rule operation"),
                entry["field"].as_str().expect("rule field"),
                entry["rule"].as_str().expect("rule kind"),
            )
        })
        .collect::<BTreeSet<_>>();
    let rust_rules = REQUEST_FIELD_RULES
        .iter()
        .map(|entry| {
            let operation = OPERATION_CODES
                .iter()
                .find_map(|(name, code)| (*code == entry.operation as u32).then_some(*name))
                .expect("operation name for field rule");
            let rule = match entry.rule {
                RequestFieldRuleKind::MustBeZero => "MUST_BE_ZERO",
                RequestFieldRuleKind::MustBeNonZero => "MUST_BE_NONZERO",
                RequestFieldRuleKind::ZeroSelectsLatest => "ZERO_SELECTS_LATEST",
                RequestFieldRuleKind::ZeroBeginsProgress => "ZERO_BEGINS_PROGRESS",
            };
            (operation, entry.field, rule)
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(manifest_rules, rust_rules);

    assert!(REQUEST_FIELD_RULES.iter().any(|entry| {
        entry.operation == OperationCode::QueryViewport
            && entry.field == "snapshot"
            && entry.rule == RequestFieldRuleKind::ZeroSelectsLatest
    }));
}

#[test]
fn status_progress_and_close_source_lifecycle_rules_are_exhaustive() {
    let manifest = manifest();
    let status_names = named_numbers(&manifest, "statuses", "code")
        .into_keys()
        .collect::<BTreeSet<_>>();
    let progress_names = named_numbers(&manifest, "progressStates", "code")
        .into_keys()
        .collect::<BTreeSet<_>>();
    let rules = manifest["statusProgressRules"]
        .as_array()
        .expect("statusProgressRules array");
    let ruled_statuses = rules
        .iter()
        .map(|entry| {
            entry["status"]
                .as_str()
                .expect("status rule name")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(ruled_statuses, status_names);
    for rule in rules {
        for progress in rule["allowedProgress"]
            .as_array()
            .expect("allowed progress array")
        {
            assert!(progress_names.contains(progress.as_str().expect("allowed progress name")));
        }
    }
    let progress_for = |status: &str| {
        rules
            .iter()
            .find(|entry| entry["status"].as_str() == Some(status))
            .expect("named status rule")["allowedProgress"]
            .as_array()
            .expect("allowed progress")
            .iter()
            .map(|value| value.as_str().expect("progress name"))
            .collect::<Vec<_>>()
    };
    assert_eq!(progress_for("NOT_CERTIFIED"), ["COMPLETE"]);
    assert_eq!(progress_for("BUFFER_TOO_SMALL"), ["NONE"]);
    assert_eq!(progress_for("CLOSE_INCOMPLETE"), ["NONE"]);
    assert!(progress_for("NEEDS_INPUT").is_empty());
    assert!(progress_for("NEEDS_OUTPUT_BUFFER").is_empty());
    assert!(progress_for("SESSION_CLOSED").is_empty());
    assert!(progress_for("HISTORY_BUDGET_EXCEEDED").is_empty());

    let reserved = manifest["statuses"]
        .as_array()
        .expect("statuses")
        .iter()
        .filter_map(|entry| {
            (entry["v1Disposition"].as_str() == Some("RESERVED_NEVER_RETURN"))
                .then(|| entry["name"].as_str().expect("reserved status name"))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        reserved,
        BTreeSet::from([
            "HISTORY_BUDGET_EXCEEDED",
            "NEEDS_INPUT",
            "NEEDS_OUTPUT_BUFFER",
            "SESSION_CLOSED",
        ])
    );
    assert!(HEADER.contains("NEEDS_INPUT, NEEDS_OUTPUT_BUFFER, SESSION_CLOSED, and"));

    let operations = manifest["operations"].as_array().expect("operations");
    let states_for = |name: &str| {
        operations
            .iter()
            .find(|entry| entry["name"].as_str() == Some(name))
            .expect("named operation")["allowedSessionStates"]
            .as_array()
            .expect("allowed states")
            .iter()
            .map(|value| value.as_str().expect("state name"))
            .collect::<Vec<_>>()
    };
    assert_eq!(states_for("SOURCE_READ"), ["OPEN"]);
    assert!(!states_for("SESSION_INSPECT").contains(&"CLOSED"));

    let budgets = manifest["budgetAdmissionRules"]
        .as_object()
        .expect("budget admission rules");
    assert!(budgets["allBudgetedCalls"]
        .as_str()
        .expect("all-budget rule")
        .contains("at least 1"));
    assert!(budgets["pagePumpCalls"]
        .as_str()
        .expect("page-budget rule")
        .contains("nonzero"));
    assert!(HEADER.contains("max_work_units must be nonzero"));
    assert!(HEADER.contains("including a header-only empty page"));
}

#[test]
fn duplicate_abi_length_authorities_must_match() {
    let manifest = manifest();
    let rules = manifest["lengthAuthorityRules"]
        .as_array()
        .expect("lengthAuthorityRules array");
    assert_eq!(rules.len(), 4);
    for rule in rules {
        assert_eq!(
            rule["rule"].as_str(),
            Some("MUST_MATCH_OR_INVALID_ARGUMENT")
        );
    }
    assert!(rules.iter().any(|rule| {
        rule["operation"].as_str() == Some("SMALL_EDIT")
            && rule["recordField"].as_str() == Some("edit_count")
            && rule["callArgument"].as_str() == Some("edit_count")
    }));
}

#[test]
fn close_initial_admission_snapshot_and_flag_policies_are_frozen() {
    let manifest = manifest();
    let close = manifest["closeStatusRules"]
        .as_array()
        .expect("closeStatusRules array");
    assert!(close.iter().any(|row| {
        row["operation"].as_str() == Some("CLOSE_PUMP")
            && row["status"].as_str() == Some("BUDGET_EXHAUSTED")
            && row["progress"].as_str() == Some("BUDGET_EXHAUSTED")
            && row["token"].as_str() == Some("CHANGED_NONZERO")
    }));
    assert!(close.iter().any(|row| {
        row["operation"].as_str() == Some("CLOSE_FINISH")
            && row["status"].as_str() == Some("CLOSE_INCOMPLETE")
            && row["progress"].as_str() == Some("NONE")
            && row["token"].as_str() == Some("ECHO_UNCHANGED_NONZERO")
    }));

    let admission = manifest["admissionRules"]
        .as_array()
        .expect("admissionRules array");
    assert!(admission.iter().any(|row| {
        row["operation"].as_str() == Some("CREATE_BEGIN")
            && row["hardCapBytes"].as_u64() == Some(65_536)
    }));
    assert!(admission.iter().any(|row| {
        row["operation"].as_str() == Some("SMALL_EDIT")
            && row["hardCapBytes"].as_u64() == Some(4096)
            && row["workUnitsOnSuccess"].as_u64() == Some(1)
            && row["resumable"].as_bool() == Some(false)
    }));
    assert!(HEADER.contains("total deleted source bytes together must not exceed"));
    assert!(HEADER.contains("exact descriptor-order partition"));
    assert!(HEADER.contains("expected_total_bytes may be zero for a pure deletion"));
    assert_eq!(
        manifest["snapshotLifetime"]["ownership"].as_str(),
        Some("NON_OWNING_EPOCH")
    );
    assert_eq!(manifest["inputFlagRules"]["abiMinor"].as_u64(), Some(0));
    assert!(manifest["inputFlagRules"]["rule"]
        .as_str()
        .expect("flag rule")
        .contains("must be zero"));
}

#[test]
fn projection_edit_cell_vocabulary_remains_bound_to_minor_29() {
    let manifest = manifest();
    let cells = &manifest["viewportProjectionEditCells"];
    assert_eq!(cells["abiMinor"].as_u64(), Some(29));
    assert_eq!(
        cells["capability"].as_str(),
        Some("PROJECTION_EDIT_CELLS_V2")
    );
    assert_eq!(cells["matchers"]["ANY_NO_CRLF_SPLICE"].as_u64(), Some(1));
    assert_eq!(
        cells["matchers"]["ASCII_LITERAL_SPLICE_IN_LITERAL"].as_u64(),
        Some(2)
    );
    assert_eq!(
        cells["matchers"]["INSERT_SINGLE_ASCII_SPACE_AT_POINT"].as_u64(),
        Some(3)
    );
    assert_eq!(
        cells["matchers"]["DELETE_ONE_ASCII_UNIT_IN_LITERAL"].as_u64(),
        Some(4)
    );
    assert_eq!(
        cells["matchers"]["APPEND_ASCII_LITERAL_AT_LINE_END"].as_u64(),
        Some(5)
    );
    assert_eq!(
        cells["stateFlags"]["TERMINAL_SPACE_BLOCKED"].as_u64(),
        Some(4096)
    );
    assert!(HEADER.contains("FLARK_V4_CAPABILITY_PROJECTION_EDIT_CELLS_V2"));
    assert!(HEADER.contains("FLARK_V4_PROJECTION_EDIT_CELL_MATCH_ASCII_LITERAL_SPLICE_IN_LITERAL"));
    assert!(HEADER.contains("FLARK_V4_PROJECTION_EDIT_CELL_MATCH_DELETE_ONE_ASCII_UNIT_IN_LITERAL"));
    assert!(HEADER.contains("FLARK_V4_PROJECTION_EDIT_CELL_MATCH_APPEND_ASCII_LITERAL_AT_LINE_END"));
    assert!(HEADER.contains("FLARK_V4_PROJECTION_EDIT_CELL_TERMINAL_SPACE_BLOCKED"));
}

#[test]
fn literal_safe_envelope_v2_remains_bound_to_minor_30() {
    let manifest = manifest();
    let envelopes = &manifest["viewportLiteralSafeEnvelopesV2"];
    assert_eq!(envelopes["abiMinor"].as_u64(), Some(30));
    assert_eq!(
        envelopes["capability"].as_str(),
        Some("LITERAL_SAFE_ENVELOPES_V2")
    );
    assert_eq!(
        envelopes["editClasses"]["SINGLE_ASCII_ASTERISK_INSERTION"].as_u64(),
        Some(3)
    );
    assert!(HEADER.contains("FLARK_V4_CAPABILITY_LITERAL_SAFE_ENVELOPES_V2"));
    assert!(HEADER.contains("FLARK_V4_LITERAL_EDIT_CLASS_SINGLE_ASCII_ASTERISK_INSERTION"));
}

#[test]
fn structural_presentation_proof_is_bound_to_minor_31() {
    let manifest = manifest();
    let proof = &manifest["structuralPresentationProofsV1"];
    assert_eq!(ABI_MINOR, 31);
    assert_eq!(proof["abiMinor"].as_u64(), Some(31));
    assert_eq!(
        proof["capability"].as_str(),
        Some("STRUCTURAL_PRESENTATION_PROOFS_V1")
    );
    assert_eq!(
        proof["receiptFlag"]["PRESENTATION_PROVEN"].as_u64(),
        Some(8)
    );
    assert!(HEADER.contains("FLARK_V4_CAPABILITY_STRUCTURAL_PRESENTATION_PROOFS_V1"));
    assert!(HEADER.contains("FLARK_V4_EDIT_INTENT_RECEIPT_PRESENTATION_PROVEN"));
}

#[test]
fn c11_compiler_accepts_the_public_header() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include_dir = manifest_dir.join("../../include");
    let smoke = manifest_dir.join("tests/header_smoke.c");
    let compiler = env::var_os("CC").unwrap_or_else(|| "cc".into());
    let output = Command::new(compiler)
        .arg("-std=c11")
        .arg("-Werror")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(include_dir)
        .arg(smoke)
        .output()
        .expect("run C compiler for flark_v4.h");
    assert!(
        output.status.success(),
        "C header did not compile:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("runtime ABI manifest JSON")
}

fn assert_u32_table(manifest: &Value, key: &str, rust: &[(&str, u32)], header_prefix: &str) {
    let manifest_values = named_numbers(manifest, key, "code");
    let rust_values = rust
        .iter()
        .map(|(name, code)| ((*name).to_owned(), u64::from(*code)))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_values, rust_values, "Rust {key} table drifted");
    for (name, code) in manifest_values {
        assert_eq!(
            header_macro(&format!("{header_prefix}{name}")),
            code,
            "header {key} value for {name}"
        );
    }
}

fn assert_u64_table(
    manifest: &Value,
    key: &str,
    rust: &[(&str, u64)],
    header_prefix: &str,
    value_key: &str,
) {
    let manifest_values = named_numbers(manifest, key, value_key);
    let rust_values = rust
        .iter()
        .map(|(name, code)| ((*name).to_owned(), *code))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(manifest_values, rust_values, "Rust {key} table drifted");
    for (name, code) in manifest_values {
        assert_eq!(
            header_macro(&format!("{header_prefix}{name}")),
            code,
            "header {key} value for {name}"
        );
    }
}

fn named_numbers(manifest: &Value, key: &str, value_key: &str) -> BTreeMap<String, u64> {
    manifest[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} array"))
        .iter()
        .map(|entry| {
            (
                entry["name"].as_str().expect("entry name").to_owned(),
                entry[value_key].as_u64().expect("entry numeric value"),
            )
        })
        .collect()
}

fn header_macro(name: &str) -> u64 {
    let prefix = format!("#define {name} ");
    let line = HEADER
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing header macro {name}"));
    let wrapped = line[prefix.len()..].trim();
    let value = wrapped
        .strip_prefix("UINT32_C(")
        .or_else(|| wrapped.strip_prefix("UINT64_C("))
        .or_else(|| wrapped.strip_prefix("UINT16_C("))
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or_else(|| panic!("unsupported header macro {line}"));
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).expect("hex macro")
    } else {
        value.parse().expect("decimal macro")
    }
}

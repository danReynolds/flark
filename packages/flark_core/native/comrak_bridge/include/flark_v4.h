#ifndef FLARK_V4_H
#define FLARK_V4_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define FLARK_V4_ABI_MAJOR UINT16_C(4)
#define FLARK_V4_ABI_MINOR UINT16_C(25)

/* Zero sentinels are legal only where the operation rules below say so. */
#define FLARK_V4_CONTINUATION_NONE UINT64_C(0)
#define FLARK_V4_ANCHOR_NONE UINT64_C(0)
#define FLARK_V4_HISTORY_TOKEN_NONE UINT64_C(0)
#define FLARK_V4_SNAPSHOT_LATEST UINT64_C(0)
#define FLARK_V4_SNAPSHOT_NOT_APPLICABLE UINT64_C(0)
#define FLARK_V4_REVISION_UNCOMMITTED UINT64_C(0)
#define FLARK_V4_PROGRESS_TOKEN_NONE UINT64_C(0)

typedef uint32_t FlarkV4StatusCode;
typedef uint32_t FlarkV4OperationCode;
typedef uint32_t FlarkV4ProgressState;
typedef uint32_t FlarkV4SessionState;
typedef uint32_t FlarkV4TransactionState;
typedef uint32_t FlarkV4CertificationState;
typedef uint32_t FlarkV4CoordinateKind;
typedef uint32_t FlarkV4Affinity;
typedef uint32_t FlarkV4QueryKind;
typedef uint32_t FlarkV4ParserProfile;
typedef uint32_t FlarkV4ResultRecordKind;
typedef uint32_t FlarkV4HistoryDisposition;
typedef uint32_t FlarkV4HandleKind;
typedef uint32_t FlarkV4OwnershipKind;
typedef uint64_t FlarkV4CapabilityBits;
typedef uint64_t FlarkV4SessionHandle;
typedef uint64_t FlarkV4TransactionHandle;
typedef uint64_t FlarkV4ContinuationHandle;
typedef uint64_t FlarkV4AnchorHandle;
typedef uint64_t FlarkV4HistoryToken;
typedef uint64_t FlarkV4Revision;
typedef uint64_t FlarkV4SnapshotId;
typedef uint64_t FlarkV4ProgressToken;
typedef uint64_t FlarkV4OwnerToken;

#define FLARK_V4_STATUS_OK UINT32_C(0x0000)
#define FLARK_V4_STATUS_NEEDS_INPUT UINT32_C(0x0001)
#define FLARK_V4_STATUS_NEEDS_OUTPUT_BUFFER UINT32_C(0x0002)
#define FLARK_V4_STATUS_NOT_CERTIFIED UINT32_C(0x0003)
#define FLARK_V4_STATUS_CANCELLED UINT32_C(0x0004)
#define FLARK_V4_STATUS_SUPERSEDED UINT32_C(0x0005)
#define FLARK_V4_STATUS_BUDGET_EXHAUSTED UINT32_C(0x0006)
#define FLARK_V4_STATUS_RESULT_CAP_REACHED UINT32_C(0x0007)
#define FLARK_V4_STATUS_NOT_READY_SOURCE_GAP UINT32_C(0x0008)
#define FLARK_V4_STATUS_BACKPRESSURE UINT32_C(0x0009)
#define FLARK_V4_STATUS_INVALID_ARGUMENT UINT32_C(0x0100)
#define FLARK_V4_STATUS_UNSUPPORTED_ABI_VERSION UINT32_C(0x0101)
#define FLARK_V4_STATUS_UNSUPPORTED_CAPABILITY UINT32_C(0x0102)
#define FLARK_V4_STATUS_INVALID_HANDLE UINT32_C(0x0103)
#define FLARK_V4_STATUS_STALE_HANDLE UINT32_C(0x0104)
#define FLARK_V4_STATUS_WRONG_HANDLE_KIND UINT32_C(0x0105)
#define FLARK_V4_STATUS_OWNER_MISMATCH UINT32_C(0x0106)
#define FLARK_V4_STATUS_SESSION_BUSY UINT32_C(0x0107)
#define FLARK_V4_STATUS_SESSION_CLOSING UINT32_C(0x0108)
#define FLARK_V4_STATUS_SESSION_CLOSED UINT32_C(0x0109)
#define FLARK_V4_STATUS_MIGRATION_WHILE_ACTIVE UINT32_C(0x010a)
#define FLARK_V4_STATUS_STALE_REVISION UINT32_C(0x0200)
#define FLARK_V4_STATUS_STALE_SNAPSHOT UINT32_C(0x0201)
#define FLARK_V4_STATUS_STALE_CONTINUATION UINT32_C(0x0202)
#define FLARK_V4_STATUS_INVALID_UTF8 UINT32_C(0x0203)
#define FLARK_V4_STATUS_RANGE_OUT_OF_BOUNDS UINT32_C(0x0204)
#define FLARK_V4_STATUS_RANGE_NOT_SCALAR_BOUNDARY UINT32_C(0x0205)
#define FLARK_V4_STATUS_COORDINATE_OUT_OF_RANGE UINT32_C(0x0206)
#define FLARK_V4_STATUS_EDIT_TOO_LARGE UINT32_C(0x0207)
#define FLARK_V4_STATUS_QUERY_LIMIT_EXCEEDED UINT32_C(0x0208)
#define FLARK_V4_STATUS_BUFFER_TOO_SMALL UINT32_C(0x0209)
#define FLARK_V4_STATUS_STALE_PROGRESS_TOKEN UINT32_C(0x020a)
#define FLARK_V4_STATUS_INVALID_UTF16_HOST_INPUT UINT32_C(0x020b)
#define FLARK_V4_STATUS_TRANSACTION_CONFLICT UINT32_C(0x0300)
#define FLARK_V4_STATUS_TRANSACTION_INCOMPLETE UINT32_C(0x0301)
#define FLARK_V4_STATUS_TRANSACTION_ALREADY_COMMITTED UINT32_C(0x0302)
#define FLARK_V4_STATUS_TRANSACTION_ALREADY_ABORTED UINT32_C(0x0303)
#define FLARK_V4_STATUS_HISTORY_BUDGET_EXCEEDED UINT32_C(0x0304)
#define FLARK_V4_STATUS_HISTORY_TOKEN_EVICTED UINT32_C(0x0305)
#define FLARK_V4_STATUS_HISTORY_TOKEN_STALE UINT32_C(0x0306)
#define FLARK_V4_STATUS_CONTINUATION_LIMIT_EXCEEDED UINT32_C(0x0307)
#define FLARK_V4_STATUS_PROGRESS_STALLED UINT32_C(0x0400)
#define FLARK_V4_STATUS_PANIC_CONTAINED UINT32_C(0x0401)
#define FLARK_V4_STATUS_INTERNAL_FAULT UINT32_C(0x0402)
#define FLARK_V4_STATUS_RESOURCE_LIMIT_EXCEEDED UINT32_C(0x0403)
#define FLARK_V4_STATUS_CLOSE_INCOMPLETE UINT32_C(0x0404)
#define FLARK_V4_STATUS_PARSER_FAULT UINT32_C(0x0405)
#define FLARK_V4_STATUS_ALLOCATION_FAILURE UINT32_C(0x0406)

/* NEEDS_INPUT, NEEDS_OUTPUT_BUFFER, SESSION_CLOSED, and
 * HISTORY_BUDGET_EXCEEDED retain numeric values for ABI stability but are
 * reserved and never returned by 4.0. The function return value always equals
 * FlarkV4Outcome.status. */

#define FLARK_V4_OPERATION_NEGOTIATE UINT32_C(0)
#define FLARK_V4_OPERATION_CREATE_BEGIN UINT32_C(1)
#define FLARK_V4_OPERATION_CREATE_APPEND UINT32_C(2)
#define FLARK_V4_OPERATION_CREATE_COMMIT UINT32_C(3)
#define FLARK_V4_OPERATION_CREATE_ABORT UINT32_C(4)
#define FLARK_V4_OPERATION_SOURCE_READ UINT32_C(5)
#define FLARK_V4_OPERATION_SMALL_EDIT UINT32_C(6)
#define FLARK_V4_OPERATION_BULK_BEGIN UINT32_C(7)
#define FLARK_V4_OPERATION_BULK_APPEND UINT32_C(8)
#define FLARK_V4_OPERATION_BULK_COMMIT UINT32_C(9)
#define FLARK_V4_OPERATION_BULK_ABORT UINT32_C(10)
#define FLARK_V4_OPERATION_PUMP UINT32_C(11)
#define FLARK_V4_OPERATION_QUERY_VIEWPORT UINT32_C(12)
#define FLARK_V4_OPERATION_CONTINUATION_NEXT UINT32_C(13)
#define FLARK_V4_OPERATION_CONTINUATION_RELEASE UINT32_C(14)
#define FLARK_V4_OPERATION_ANCHOR_CREATE UINT32_C(15)
#define FLARK_V4_OPERATION_ANCHOR_TRANSFORM UINT32_C(16)
#define FLARK_V4_OPERATION_ANCHOR_RESOLVE UINT32_C(17)
#define FLARK_V4_OPERATION_ANCHOR_RELEASE UINT32_C(18)
#define FLARK_V4_OPERATION_COORDINATE_CONVERT UINT32_C(19)
#define FLARK_V4_OPERATION_HISTORY_REPLAY UINT32_C(20)
#define FLARK_V4_OPERATION_HISTORY_RELEASE UINT32_C(21)
#define FLARK_V4_OPERATION_CANCEL UINT32_C(22)
#define FLARK_V4_OPERATION_CLOSE_BEGIN UINT32_C(23)
#define FLARK_V4_OPERATION_CLOSE_PUMP UINT32_C(24)
#define FLARK_V4_OPERATION_CLOSE_FINISH UINT32_C(25)
#define FLARK_V4_OPERATION_SESSION_TRANSFER_OWNER UINT32_C(26)
#define FLARK_V4_OPERATION_SESSION_INSPECT UINT32_C(27)
#define FLARK_V4_OPERATION_EDIT_INTENT_V1 UINT32_C(28)
#define FLARK_V4_OPERATION_SOURCE_TRANSACTION_V1 UINT32_C(29)
#define FLARK_V4_OPERATION_STAGED_SOURCE_TRANSACTION_V1 UINT32_C(30)

#define FLARK_V4_PROGRESS_NONE UINT32_C(0)
#define FLARK_V4_PROGRESS_ADVANCED UINT32_C(1)
#define FLARK_V4_PROGRESS_BUDGET_EXHAUSTED UINT32_C(2)
#define FLARK_V4_PROGRESS_RESULT_CAP_REACHED UINT32_C(3)
#define FLARK_V4_PROGRESS_PENDING_SOURCE_GAP UINT32_C(4)
#define FLARK_V4_PROGRESS_BACKPRESSURED UINT32_C(5)
#define FLARK_V4_PROGRESS_COMPLETE UINT32_C(6)
#define FLARK_V4_PROGRESS_CANCELLED UINT32_C(7)
#define FLARK_V4_PROGRESS_SUPERSEDED UINT32_C(8)
#define FLARK_V4_PROGRESS_FAULT UINT32_C(9)

#define FLARK_V4_SESSION_CREATING UINT32_C(1)
#define FLARK_V4_SESSION_OPEN UINT32_C(2)
#define FLARK_V4_SESSION_CLOSING UINT32_C(3)
#define FLARK_V4_SESSION_CLOSED UINT32_C(4)
#define FLARK_V4_SESSION_FAULTED UINT32_C(5)

#define FLARK_V4_TRANSACTION_STAGING UINT32_C(1)
#define FLARK_V4_TRANSACTION_COMMITTED UINT32_C(2)
#define FLARK_V4_TRANSACTION_ABORTED UINT32_C(3)

#define FLARK_V4_CERTIFICATION_NOT_APPLICABLE UINT32_C(0)
#define FLARK_V4_CERTIFICATION_PENDING_NEUTRAL UINT32_C(1)
#define FLARK_V4_CERTIFICATION_CURRENT_CERTIFIED UINT32_C(2)
#define FLARK_V4_CERTIFICATION_MIXED_CURRENT UINT32_C(3)

#define FLARK_V4_PARSER_PROFILE_COMMONMARK_0_31_2 UINT32_C(1)
#define FLARK_V4_PARSER_PROFILE_FLARK_GFM_0_29_V1 UINT32_C(2)

#define FLARK_V4_COORDINATE_SOURCE_BYTE UINT32_C(1)
#define FLARK_V4_COORDINATE_UTF16_CODE_UNIT UINT32_C(2)

#define FLARK_V4_AFFINITY_UPSTREAM UINT32_C(1)
#define FLARK_V4_AFFINITY_DOWNSTREAM UINT32_C(2)

#define FLARK_V4_QUERY_SOURCE UINT32_C(1)
#define FLARK_V4_QUERY_SEMANTIC UINT32_C(2)
#define FLARK_V4_QUERY_SOURCE_AND_SEMANTIC UINT32_C(3)
#define FLARK_V4_QUERY_SEMANTIC_PROJECTED UINT32_C(4)
#define FLARK_V4_QUERY_SEMANTIC_TARGET UINT32_C(5)

#define FLARK_V4_RESULT_RECORD_SOURCE_BYTES UINT32_C(1)
#define FLARK_V4_RESULT_RECORD_SEMANTIC_FACTS UINT32_C(2)
#define FLARK_V4_RESULT_RECORD_SOURCE_AND_SEMANTIC UINT32_C(3)
#define FLARK_V4_RESULT_RECORD_SEMANTIC_TARGET UINT32_C(4)

#define FLARK_V4_HISTORY_NOT_APPLICABLE UINT32_C(0)
#define FLARK_V4_HISTORY_RETAINED UINT32_C(1)
#define FLARK_V4_HISTORY_DISABLED UINT32_C(2)
#define FLARK_V4_HISTORY_OVER_BUDGET UINT32_C(3)

#define FLARK_V4_HANDLE_NONE UINT32_C(0)
#define FLARK_V4_HANDLE_SESSION UINT32_C(1)
#define FLARK_V4_HANDLE_TRANSACTION UINT32_C(2)
#define FLARK_V4_HANDLE_CONTINUATION UINT32_C(3)
#define FLARK_V4_HANDLE_ANCHOR UINT32_C(4)
#define FLARK_V4_HANDLE_HISTORY_TOKEN UINT32_C(5)

#define FLARK_V4_OWNERSHIP_BORROWED_INPUT UINT32_C(1)
#define FLARK_V4_OWNERSHIP_CALLER_OUTPUT UINT32_C(2)
#define FLARK_V4_OWNERSHIP_RUNTIME_HANDLE UINT32_C(3)
#define FLARK_V4_OWNERSHIP_RUNTIME_STAGED_BYTES UINT32_C(4)

#define FLARK_V4_CAPABILITY_SOURCE_STREAMING UINT64_C(0x0000000000000001)
#define FLARK_V4_CAPABILITY_SMALL_EDITS UINT64_C(0x0000000000000002)
#define FLARK_V4_CAPABILITY_BULK_TRANSACTIONS UINT64_C(0x0000000000000004)
#define FLARK_V4_CAPABILITY_BOUNDED_PUMP UINT64_C(0x0000000000000008)
#define FLARK_V4_CAPABILITY_RANGE_CERTIFICATION UINT64_C(0x0000000000000010)
#define FLARK_V4_CAPABILITY_VIEWPORT_QUERIES UINT64_C(0x0000000000000020)
#define FLARK_V4_CAPABILITY_STABLE_ANCHORS UINT64_C(0x0000000000000040)
#define FLARK_V4_CAPABILITY_COORDINATE_CONVERSION UINT64_C(0x0000000000000080)
#define FLARK_V4_CAPABILITY_REVERSIBLE_HISTORY UINT64_C(0x0000000000000100)
#define FLARK_V4_CAPABILITY_RESUMABLE_CLOSE UINT64_C(0x0000000000000200)
#define FLARK_V4_CAPABILITY_SNAPSHOT_CONTINUATIONS UINT64_C(0x0000000000000400)
#define FLARK_V4_CAPABILITY_CANCELLATION UINT64_C(0x0000000000000800)
#define FLARK_V4_CAPABILITY_PANIC_CONTAINMENT UINT64_C(0x0000000000001000)
#define FLARK_V4_CAPABILITY_COMMONMARK_0_31_2 UINT64_C(0x0000000000002000)
#define FLARK_V4_CAPABILITY_SELECTED_GFM_V1 UINT64_C(0x0000000000004000)
#define FLARK_V4_CAPABILITY_EDIT_INTENTS_V1 UINT64_C(0x0000000000008000)
#define FLARK_V4_CAPABILITY_SOURCE_TRANSACTIONS_V1 UINT64_C(0x0000000000010000)
#define FLARK_V4_CAPABILITY_NATIVE_COMPOSITE_HISTORY_V1 UINT64_C(0x0000000000020000)
#define FLARK_V4_CAPABILITY_LIST_CONTAINER_COLUMNS_V1 UINT64_C(0x0000000000040000)
#define FLARK_V4_CAPABILITY_INDENTED_CODE_EDITING_V1 UINT64_C(0x0000000000080000)
#define FLARK_V4_CAPABILITY_SEMANTIC_ATOMS_V1 UINT64_C(0x0000000000100000)
#define FLARK_V4_CAPABILITY_NESTED_BLOCK_QUOTE_EDITING_V1 UINT64_C(0x0000000000200000)
#define FLARK_V4_CAPABILITY_SEMANTIC_ACTIONS_V1 UINT64_C(0x0000000000400000)
#define FLARK_V4_CAPABILITY_STAGED_SOURCE_TRANSACTIONS_V1 UINT64_C(0x0000000000800000)
#define FLARK_V4_CAPABILITY_LIST_INDENTATION_V1 UINT64_C(0x0000000001000000)
#define FLARK_V4_CAPABILITY_SEMANTIC_TARGETS_V1 UINT64_C(0x0000000002000000)

#define FLARK_V4_SEMANTIC_TARGET_LINK UINT32_C(1)
#define FLARK_V4_SEMANTIC_TARGET_IMAGE UINT32_C(2)
#define FLARK_V4_SEMANTIC_TARGET_AUTOLINK_URI UINT32_C(1)
#define FLARK_V4_SEMANTIC_TARGET_AUTOLINK_EMAIL UINT32_C(2)
#define FLARK_V4_SEMANTIC_TARGET_DIRECT UINT32_C(3)
#define FLARK_V4_SEMANTIC_TARGET_REFERENCE UINT32_C(4)

#define FLARK_V4_EDIT_PROFILE_FLARK_V1 UINT32_C(1)
#define FLARK_V4_EDIT_INTENT_INSERT_PARAGRAPH_BREAK UINT32_C(1)
#define FLARK_V4_EDIT_INTENT_DELETE_BACKWARD UINT32_C(2)
#define FLARK_V4_EDIT_INTENT_DELETE_FORWARD UINT32_C(3)
#define FLARK_V4_EDIT_INTENT_TOGGLE_TASK_CHECKED UINT32_C(4)
#define FLARK_V4_EDIT_INTENT_INDENT_LIST_ITEM UINT32_C(5)
#define FLARK_V4_EDIT_INTENT_OUTDENT_LIST_ITEM UINT32_C(6)
#define FLARK_V4_EDIT_INTENT_DISPOSITION_APPLIED UINT32_C(1)
#define FLARK_V4_EDIT_INTENT_DISPOSITION_HANDLED_NO_CHANGE UINT32_C(2)
#define FLARK_V4_EDIT_INTENT_DISPOSITION_NOT_APPLICABLE UINT32_C(3)
#define FLARK_V4_EDIT_INTENT_DISPOSITION_NEEDS_CURRENT_SEMANTICS UINT32_C(4)
#define FLARK_V4_EDIT_INTENT_RECEIPT_HAS_COMMIT UINT32_C(0x1)
#define FLARK_V4_EDIT_INTENT_RECEIPT_PARSER_PENDING UINT32_C(0x2)
#define FLARK_V4_EDIT_INTENT_RECEIPT_SEMANTIC_BYTES UINT32_C(0x4)
#define FLARK_V4_SOURCE_TRANSACTION_RECEIPT_HAS_COMMIT UINT32_C(0x1)
#define FLARK_V4_SOURCE_TRANSACTION_RECEIPT_PARSER_PENDING UINT32_C(0x2)
#define FLARK_V4_SOURCE_TRANSACTION_RECEIPT_CALLER_KNOWN_BYTES UINT32_C(0x4)
#define FLARK_V4_SOURCE_TRANSACTION_RECEIPT_COMPOSITE_HISTORY_EXTENDED UINT32_C(0x8)
#define FLARK_V4_SOURCE_TRANSACTION_RECEIPT_STAGED_BYTES UINT32_C(0x10)
#define FLARK_V4_EDIT_PRESENTATION_NONE UINT32_C(0)
#define FLARK_V4_EDIT_PRESENTATION_SPLIT_PARAGRAPH UINT32_C(1)
#define FLARK_V4_EDIT_PRESENTATION_CONTINUE_LIST UINT32_C(2)
#define FLARK_V4_EDIT_PRESENTATION_EXIT_LIST UINT32_C(3)
#define FLARK_V4_EDIT_PRESENTATION_MERGE_PARAGRAPH UINT32_C(4)
#define FLARK_V4_EDIT_PRESENTATION_LIFT_LIST UINT32_C(5)
#define FLARK_V4_EDIT_PRESENTATION_CONTINUE_BLOCK_QUOTE UINT32_C(6)
#define FLARK_V4_EDIT_PRESENTATION_EXIT_BLOCK_QUOTE UINT32_C(7)
#define FLARK_V4_EDIT_PRESENTATION_LIFT_BLOCK_QUOTE UINT32_C(8)
#define FLARK_V4_EDIT_PRESENTATION_EXIT_HEADING UINT32_C(9)
#define FLARK_V4_EDIT_PRESENTATION_LIFT_HEADING UINT32_C(10)
#define FLARK_V4_EDIT_PRESENTATION_OUTDENT_LIST UINT32_C(11)
#define FLARK_V4_EDIT_PRESENTATION_CONTINUE_INDENTED_CODE UINT32_C(12)
#define FLARK_V4_EDIT_PRESENTATION_JOIN_INDENTED_CODE UINT32_C(13)
#define FLARK_V4_EDIT_PRESENTATION_LIFT_INDENTED_CODE UINT32_C(14)
#define FLARK_V4_EDIT_PRESENTATION_DELETE_THEMATIC_BREAK UINT32_C(15)
#define FLARK_V4_EDIT_PRESENTATION_OUTDENT_BLOCK_QUOTE UINT32_C(16)
#define FLARK_V4_EDIT_PRESENTATION_TOGGLE_TASK_CHECKED UINT32_C(17)
#define FLARK_V4_EDIT_PRESENTATION_INDENT_LIST UINT32_C(18)
#define FLARK_V4_EDIT_PRESENTATION_RETAIN_PARAGRAPH_GAP UINT32_C(19)

#define FLARK_V4_MAX_SMALL_EDIT_BYTES UINT32_C(4096)
#define FLARK_V4_MAX_BULK_CHUNK_BYTES UINT32_C(65536)
#define FLARK_V4_MAX_SOURCE_CHUNK_BYTES UINT32_C(65536)
#define FLARK_V4_MAX_RESULT_BYTES UINT32_C(262144)
#define FLARK_V4_MAX_QUERY_ITEMS UINT32_C(4096)
#define FLARK_V4_MAX_TRANSACTION_EDITS UINT32_C(64)
/* Live anchors are transformed eagerly on every committed edit; this cap
 * bounds that per-edit maintenance. Not yet surfaced in FlarkV4AbiInfo. */
#define FLARK_V4_MAX_LIVE_ANCHORS UINT32_C(4096)

typedef struct FlarkV4AbiInfo {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  uint64_t capability_bits;
  uint32_t max_small_edit_bytes;
  uint32_t max_bulk_chunk_bytes;
  uint32_t max_source_chunk_bytes;
  uint32_t max_result_bytes;
  uint32_t max_query_items;
  uint32_t max_transaction_edits;
  uint64_t reserved[3];
} FlarkV4AbiInfo;

typedef struct FlarkV4Outcome {
  uint32_t struct_size;
  uint32_t operation;
  uint32_t status;
  uint32_t progress_state;
  uint64_t primary_handle;
  uint64_t secondary_handle;
  uint64_t revision;
  uint64_t snapshot;
  uint64_t progress_token;
  uint64_t required_bytes;
  uint64_t written_bytes;
  uint64_t detail_code;
  uint64_t reserved[4];
} FlarkV4Outcome;
/* required_bytes is nonzero only for BUFFER_TOO_SMALL. For SOURCE_READ,
 * QUERY_VIEWPORT, and CONTINUATION_NEXT it always includes the fixed result
 * page header, including a header-only empty page. */

typedef struct FlarkV4SourceRange {
  uint64_t start_byte;
  uint64_t end_byte;
} FlarkV4SourceRange;

/* Ordered partition record for SOURCE_AND_SEMANTIC live viewport pages.
 * certification_state is PENDING_NEUTRAL or CURRENT_CERTIFIED. Records are
 * followed in the payload by exact current source bytes for covered_range. */
typedef struct FlarkV4CertificationRangeRecord {
  FlarkV4CertificationState certification_state;
  uint32_t reserved;
  FlarkV4SourceRange source_range;
  FlarkV4SourceRange source_utf16_range;
} FlarkV4CertificationRangeRecord;

/* Prefix of every source/query/continuation output; payload starts at struct_size. */
typedef struct FlarkV4ResultPageHeader {
  uint32_t struct_size;
  uint16_t abi_major;
  uint16_t abi_minor;
  FlarkV4ResultRecordKind record_kind;
  FlarkV4CertificationState certification_state;
  FlarkV4Revision revision;
  FlarkV4SnapshotId snapshot;
  FlarkV4SourceRange requested_range;
  FlarkV4SourceRange covered_range;
  uint32_t item_count;
  uint32_t payload_bytes;
  FlarkV4ContinuationHandle continuation;
  uint64_t reserved[2];
} FlarkV4ResultPageHeader;

/* Fixed payload record for SEMANTIC_FACTS viewport pages. UINT64_MAX in an
 * editable endpoint means the row has no contiguous active-edit projection.
 * ABI 4.5 appends grouped inline-fact records after the row array. ABI 4.6
 * adds bounded parser-cooked replacement scalars to those records. ABI 4.7
 * adds parser-owned bounded GFM table cells in the same grouped stream. ABI
 * 4.15 packs the inline-fact count in the low 16 bits and the grouped
 * projected-segment count in the high 16 bits. ABI 4.16 admits synthetic
 * zero-length row kind 15 for parser-certified empty BlockQuote lines. */
typedef struct FlarkV4ViewportRowRecord {
  uint64_t ordinal;
  uint32_t kind;
  uint32_t flags;
  uint64_t source_start_byte;
  uint64_t source_end_byte;
  uint64_t source_start_utf16;
  uint64_t source_end_utf16;
  uint64_t editable_start_byte;
  uint64_t editable_end_byte;
  uint64_t editable_start_utf16;
  uint64_t editable_end_utf16;
  uint64_t presentation_prefix_start_byte;
  uint64_t presentation_prefix_end_byte;
  uint64_t presentation_prefix_start_utf16;
  uint64_t presentation_prefix_end_utf16;
  uint32_t path_depth;
  uint32_t semantic_variant;
  uint32_t semantic_value;
  uint32_t inline_fact_count;
} FlarkV4ViewportRowRecord;

/* One exact identity-source segment in a PROJECTED_RESERVED row. Segment
 * records follow all grouped inline facts in row order. */
typedef struct FlarkV4ProjectionSegmentRecord {
  FlarkV4SourceRange source_range;
  FlarkV4SourceRange source_utf16_range;
} FlarkV4ProjectionSegmentRecord;

#define FLARK_V4_VIEWPORT_ROW_CONTIGUOUS_EDIT UINT32_C(0x1)
#define FLARK_V4_VIEWPORT_ROW_PROJECTED_RESERVED UINT32_C(0x2)
#define FLARK_V4_VIEWPORT_ROW_EDIT_UNAVAILABLE UINT32_C(0x4)
#define FLARK_V4_VIEWPORT_ROW_INLINE_AUTHORITATIVE UINT32_C(0x8)
#define FLARK_V4_VIEWPORT_ROW_INLINE_FACT_COUNT_MASK UINT32_C(0x0000ffff)
#define FLARK_V4_VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_SHIFT UINT32_C(16)
#define FLARK_V4_VIEWPORT_ROW_PROJECTION_SEGMENT_COUNT_MASK UINT32_C(0xffff0000)
#define FLARK_V4_VIEWPORT_ROW_HEADING_LEVEL_MASK UINT32_C(0xff)
#define FLARK_V4_VIEWPORT_ROW_HEADING_SETEXT UINT32_C(0x100)
#define FLARK_V4_VIEWPORT_ROW_LIST_MARKER_MASK UINT32_C(0x7)
#define FLARK_V4_VIEWPORT_ROW_LIST_HYPHEN UINT32_C(0x1)
#define FLARK_V4_VIEWPORT_ROW_LIST_PLUS UINT32_C(0x2)
#define FLARK_V4_VIEWPORT_ROW_LIST_ASTERISK UINT32_C(0x3)
#define FLARK_V4_VIEWPORT_ROW_LIST_ORDERED_PERIOD UINT32_C(0x4)
#define FLARK_V4_VIEWPORT_ROW_LIST_ORDERED_PARENTHESIS UINT32_C(0x5)
#define FLARK_V4_VIEWPORT_ROW_LIST_DEPTH_SHIFT UINT32_C(3)
#define FLARK_V4_VIEWPORT_ROW_LIST_DEPTH_MASK UINT32_C(0x7f8)
#define FLARK_V4_VIEWPORT_ROW_LIST_MARKER_OFFSET_SHIFT UINT32_C(11)
#define FLARK_V4_VIEWPORT_ROW_LIST_MARKER_OFFSET_MASK UINT32_C(0x1800)
#define FLARK_V4_VIEWPORT_ROW_LIST_SIMPLE_CONTINUATION UINT32_C(0x2000)
#define FLARK_V4_VIEWPORT_ROW_LIST_STARTS_LIST UINT32_C(0x4000)
#define FLARK_V4_VIEWPORT_ROW_LIST_TASK UINT32_C(0x8000)
#define FLARK_V4_VIEWPORT_ROW_LIST_TASK_CHECKED UINT32_C(0x10000)
#define FLARK_V4_VIEWPORT_ROW_LIST_MARKER_COLUMN_SHIFT UINT32_C(17)
#define FLARK_V4_VIEWPORT_ROW_LIST_MARKER_COLUMN_MASK UINT32_C(0x1fe0000)
#define FLARK_V4_VIEWPORT_ROW_BLOCK_QUOTE_PRESENTATION UINT32_C(0x10000)
#define FLARK_V4_VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_SHIFT UINT32_C(17)
#define FLARK_V4_VIEWPORT_ROW_BLOCK_QUOTE_DEPTH_MASK UINT32_C(0x1fe0000)
#define FLARK_V4_VIEWPORT_ROW_BLOCK_QUOTE_SIMPLE_CONTINUATION UINT32_C(0x2000000)
#define FLARK_V4_VIEWPORT_ROW_CODE_PRESENTATION UINT32_C(0x10000)
#define FLARK_V4_VIEWPORT_ROW_CODE_FENCED UINT32_C(0x20000)
#define FLARK_V4_VIEWPORT_ROW_CODE_TILDE UINT32_C(0x40000)
#define FLARK_V4_VIEWPORT_ROW_CODE_CLOSED UINT32_C(0x80000)
#define FLARK_V4_VIEWPORT_ROW_CODE_FENCE_OFFSET_SHIFT UINT32_C(20)
#define FLARK_V4_VIEWPORT_ROW_CODE_FENCE_OFFSET_MASK UINT32_C(0x300000)
#define FLARK_V4_VIEWPORT_ROW_THEMATIC_BREAK_PRESENTATION UINT32_C(0x10000)
#define FLARK_V4_VIEWPORT_ROW_TABLE_PRESENTATION UINT32_C(0x4000000)

/* Parser-authored inline facts grouped in row order after the fixed row array.
 * source_* spans the complete Markdown form; content_* spans visible content. */
typedef struct FlarkV4InlineFactRecord {
  uint32_t kind;
  uint32_t flags;
  uint64_t source_start_byte;
  uint64_t source_end_byte;
  uint64_t source_start_utf16;
  uint64_t source_end_utf16;
  uint64_t content_start_byte;
  uint64_t content_end_byte;
  uint64_t content_start_utf16;
  uint64_t content_end_utf16;
  uint32_t replacement_first;
  uint32_t replacement_second;
} FlarkV4InlineFactRecord;

/* One on-demand parser-certified link/image target. The query range must be
 * the exact source range of an authoritative inline fact. Variable UTF-8
 * destination bytes followed by optional title bytes trail this record. */
typedef struct FlarkV4SemanticTargetRecord {
  uint32_t kind;
  uint32_t syntax;
  FlarkV4SourceRange source_range;
  FlarkV4SourceRange source_utf16_range;
  FlarkV4SourceRange content_range;
  FlarkV4SourceRange content_utf16_range;
  FlarkV4SourceRange destination_source_range;
  FlarkV4SourceRange destination_source_utf16_range;
  FlarkV4SourceRange title_source_range;
  FlarkV4SourceRange title_source_utf16_range;
  uint32_t destination_bytes;
  uint32_t title_bytes;
  uint64_t reserved[2];
} FlarkV4SemanticTargetRecord;

#define FLARK_V4_INLINE_FACT_EMPHASIS UINT32_C(1)
#define FLARK_V4_INLINE_FACT_STRONG UINT32_C(2)
#define FLARK_V4_INLINE_FACT_CODE UINT32_C(3)
#define FLARK_V4_INLINE_FACT_STRIKETHROUGH UINT32_C(4)
#define FLARK_V4_INLINE_FACT_AUTOLINK_URI UINT32_C(5)
#define FLARK_V4_INLINE_FACT_AUTOLINK_EMAIL UINT32_C(6)
#define FLARK_V4_INLINE_FACT_BACKSLASH_ESCAPE UINT32_C(7)
#define FLARK_V4_INLINE_FACT_HARD_LINE_BREAK UINT32_C(8)
#define FLARK_V4_INLINE_FACT_REPLACEMENT UINT32_C(9)
#define FLARK_V4_INLINE_FACT_DIRECT_LINK UINT32_C(10)
#define FLARK_V4_INLINE_FACT_DIRECT_IMAGE UINT32_C(11)
#define FLARK_V4_INLINE_FACT_REFERENCE_LINK UINT32_C(12)
#define FLARK_V4_INLINE_FACT_REFERENCE_IMAGE UINT32_C(13)
#define FLARK_V4_INLINE_FACT_TABLE_CELL UINT32_C(14)
#define FLARK_V4_INLINE_FACT_TABLE_ALIGNMENT_MASK UINT32_C(0x3)
#define FLARK_V4_INLINE_FACT_TABLE_HEADER UINT32_C(0x4)
#define FLARK_V4_INLINE_FACT_TABLE_ROW_START UINT32_C(0x8)
#define FLARK_V4_INLINE_FACT_TABLE_AUTOCOMPLETED UINT32_C(0x10)

typedef struct FlarkV4EditDescriptor {
  uint64_t start_byte;
  uint64_t end_byte;
  uint64_t replacement_offset;
  uint64_t replacement_len;
} FlarkV4EditDescriptor;

typedef struct FlarkV4WorkBudget {
  uint64_t max_work_units;
  uint64_t advisory_max_micros;
  uint32_t max_result_items;
  uint32_t max_result_bytes;
} FlarkV4WorkBudget;
/* max_work_units must be nonzero on every request carrying this record.
 * QUERY_VIEWPORT and CONTINUATION_NEXT additionally require both result caps
 * to be nonzero. Every cap must remain within the negotiated maxima. */

typedef struct FlarkV4SessionRef {
  uint64_t session;
  uint64_t owner_token;
} FlarkV4SessionRef;

typedef struct FlarkV4NegotiateRequest {
  uint32_t struct_size;
  uint16_t requested_major;
  uint16_t requested_minor;
  uint64_t required_capability_bits;
} FlarkV4NegotiateRequest;

typedef struct FlarkV4SessionConfig {
  uint32_t struct_size;
  uint32_t parser_profile;
  uint64_t history_budget_bytes;
  uint64_t max_document_bytes;
  uint64_t flags;
  uint64_t reserved[4];
} FlarkV4SessionConfig;

typedef struct FlarkV4CreateRequest {
  uint32_t struct_size;
  uint32_t flags;
  uint64_t owner_token;
  uint64_t expected_total_bytes;
  FlarkV4SessionConfig config;
} FlarkV4CreateRequest;
/* CREATE_BEGIN input_len is at most MAX_SOURCE_CHUNK_BYTES. Larger initial
 * source must continue through CREATE_APPEND. */

typedef struct FlarkV4StageRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t transaction;
  uint64_t chunk_offset;
  uint64_t chunk_len;
  uint64_t reserved[2];
} FlarkV4StageRequest;
/* chunk_len must equal the entrypoint input_len or the call is INVALID_ARGUMENT. */

typedef struct FlarkV4SourceReadRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t revision;
  FlarkV4SourceRange range;
  uint64_t reserved[2];
} FlarkV4SourceReadRequest;
/* SOURCE_READ is OPEN-only, range length is at most MAX_SOURCE_CHUNK_BYTES,
 * and its SOURCE_BYTES page uses snapshot/continuation zero. Callers advance
 * explicit ranges; SOURCE_READ never creates a continuation. */

typedef struct FlarkV4SmallEditRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t expected_revision;
  uint32_t edit_count;
  uint32_t reserved_u32;
  uint64_t replacement_bytes_len;
  FlarkV4WorkBudget budget;
  uint64_t reserved[2];
} FlarkV4SmallEditRequest;
/* edit_count and replacement_bytes_len must equal the duplicate entrypoint
 * arguments or the call is INVALID_ARGUMENT. Descriptor replacement ranges
 * form an exact descriptor-order partition of the packed byte slice: no gaps,
 * overlap/reuse, reordering, or unreferenced tail. Descriptor bytes,
 * replacement bytes, and total deleted source bytes together must not exceed
 * FLARK_V4_MAX_SMALL_EDIT_BYTES; larger edits use the resumable bulk path. */

typedef struct FlarkV4EditIntentRequestV1 {
  uint32_t struct_size;
  uint32_t profile_id;
  FlarkV4SessionRef session;
  uint64_t expected_revision;
  FlarkV4AnchorHandle selection_base_anchor;
  FlarkV4AnchorHandle selection_extent_anchor;
  uint64_t logical_edit_id;
  uint64_t request_digest;
  uint64_t acknowledge_previous_logical_edit_id;
  uint64_t selection_generation;
  uint32_t intent;
  uint32_t selection_affinity;
  uint32_t selection_direction;
  uint32_t composition_active;
  FlarkV4WorkBudget budget;
  FlarkV4AnchorHandle target_anchor;
} FlarkV4EditIntentRequestV1;

typedef struct FlarkV4EditIntentReceiptV1 {
  uint32_t struct_size;
  uint32_t semantic_disposition;
  FlarkV4HistoryDisposition history_disposition;
  uint32_t flags;
  uint64_t logical_edit_id;
  uint64_t request_digest;
  FlarkV4Revision base_revision;
  FlarkV4Revision result_revision;
  FlarkV4SourceRange base_byte_range;
  FlarkV4SourceRange base_utf16_range;
  FlarkV4SourceRange result_byte_range;
  FlarkV4SourceRange result_utf16_range;
  uint64_t result_selection_utf16;
  uint32_t result_selection_affinity;
  uint32_t result_selection_direction;
  uint64_t result_source_byte_length;
  uint64_t result_source_utf16_length;
  FlarkV4SourceRange affected_result_utf16_range;
  FlarkV4HistoryToken history_token;
  uint32_t replacement_bytes;
  uint32_t presentation_transition;
  uint64_t reserved[2];
} FlarkV4EditIntentReceiptV1;
/* The replacement payload begins immediately after the fixed receipt.
 * Selection-bound keyboard intents require two collapsed downstream anchors
 * at the same current byte and target_anchor zero. Selection-independent
 * semantic actions require an owned target_anchor in the certified row and
 * preserve both canonical selection anchors. */

typedef struct FlarkV4SourceTransactionRequestV1 {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  FlarkV4Revision expected_revision;
  FlarkV4AnchorHandle selection_base_anchor;
  FlarkV4AnchorHandle selection_extent_anchor;
  uint64_t logical_edit_id;
  uint64_t request_digest;
  uint64_t acknowledge_previous_logical_edit_id;
  uint64_t selection_generation;
  FlarkV4SourceRange base_utf16_range;
  uint64_t result_selection_base_utf16;
  uint64_t result_selection_extent_utf16;
  FlarkV4Affinity selection_affinity;
  uint32_t selection_direction;
  uint64_t replacement_bytes_len;
  FlarkV4WorkBudget budget;
  /* Zero creates a standalone token. A nonzero ID may extend only the
   * adjacent native history tail carrying the same group identity. */
  uint64_t history_group_id;
} FlarkV4SourceTransactionRequestV1;

typedef struct FlarkV4SourceTransactionReceiptV1 {
  uint32_t struct_size;
  FlarkV4HistoryDisposition history_disposition;
  uint32_t flags;
  uint32_t reserved_u32;
  uint64_t logical_edit_id;
  uint64_t request_digest;
  FlarkV4Revision base_revision;
  FlarkV4Revision result_revision;
  FlarkV4SourceRange base_byte_range;
  FlarkV4SourceRange base_utf16_range;
  FlarkV4SourceRange result_byte_range;
  FlarkV4SourceRange result_utf16_range;
  uint64_t result_selection_base_utf16;
  uint64_t result_selection_extent_utf16;
  FlarkV4Affinity result_selection_affinity;
  uint32_t result_selection_direction;
  uint64_t result_source_byte_length;
  uint64_t result_source_utf16_length;
  FlarkV4SourceRange affected_result_utf16_range;
  FlarkV4HistoryToken history_token;
  uint64_t replacement_bytes;
  uint64_t reserved[2];
} FlarkV4SourceTransactionReceiptV1;

typedef struct FlarkV4StagedSourceTransactionRequestV1 {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  FlarkV4TransactionHandle transaction;
  FlarkV4Revision expected_revision;
  FlarkV4ProgressToken progress_token;
  FlarkV4AnchorHandle selection_base_anchor;
  FlarkV4AnchorHandle selection_extent_anchor;
  uint64_t logical_edit_id;
  uint64_t request_digest;
  uint64_t acknowledge_previous_logical_edit_id;
  uint64_t selection_generation;
  uint64_t result_selection_utf16;
  FlarkV4Affinity selection_affinity;
  uint32_t selection_direction;
  FlarkV4WorkBudget budget;
  uint64_t history_group_id;
  uint64_t reserved[2];
} FlarkV4StagedSourceTransactionRequestV1;
/* V1 requires a collapsed downstream result caret at the inserted range end
 * and history_group_id zero. BULK_BEGIN/BULK_APPEND stage the replacement;
 * this resumable commit returns FlarkV4SourceTransactionReceiptV1. A zero
 * transaction is accepted only to recover a matching stored terminal. */

typedef struct FlarkV4BulkBeginRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t expected_revision;
  FlarkV4SourceRange range;
  uint64_t expected_total_bytes;
  uint64_t reserved[2];
} FlarkV4BulkBeginRequest;
/* expected_total_bytes may be zero for a pure deletion. The named source
 * range may exceed the small-edit envelope because BULK_COMMIT is resumable. */

typedef struct FlarkV4TransactionRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t transaction;
  uint64_t expected_revision;
  FlarkV4ProgressToken progress_token;
  FlarkV4WorkBudget budget;
  uint64_t reserved[1];
} FlarkV4TransactionRequest;
/* COMMIT uses progress token zero to begin and echoes each nonzero returned
 * token to resume. ABORT requires progress token zero and detaches staged
 * storage into the session reclamation queue in one work unit. */

typedef struct FlarkV4PumpRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t expected_revision;
  FlarkV4ProgressToken progress_token;
  FlarkV4WorkBudget budget;
} FlarkV4PumpRequest;
/* progress_token zero begins pump work; a nonzero value must echo the latest
 * token returned for the same revision. */

typedef struct FlarkV4QueryRequest {
  uint32_t struct_size;
  uint32_t query_kind;
  FlarkV4SessionRef session;
  uint64_t revision;
  uint64_t snapshot;
  FlarkV4SourceRange range;
  uint64_t continuation;
  FlarkV4WorkBudget budget;
  uint64_t reserved[1];
} FlarkV4QueryRequest;
/* QUERY_VIEWPORT requires continuation == NONE. snapshot == LATEST selects and
 * pins the latest snapshot; every returned page names the resulting nonzero ID. */

typedef struct FlarkV4ContinuationRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t revision;
  uint64_t snapshot;
  uint64_t continuation;
  FlarkV4WorkBudget budget;
  uint64_t reserved[1];
} FlarkV4ContinuationRequest;
/* NEXT and RELEASE echo the exact nonzero revision, snapshot, and continuation
 * authority returned by the preceding result page. */

typedef struct FlarkV4AnchorRequest {
  uint32_t struct_size;
  uint32_t coordinate_kind;
  FlarkV4SessionRef session;
  uint64_t revision;
  uint64_t snapshot;
  uint64_t anchor;
  uint64_t position;
  uint32_t affinity;
  uint32_t reserved_u32;
  FlarkV4ProgressToken progress_token;
  FlarkV4WorkBudget budget;
} FlarkV4AnchorRequest;
/* CREATE requires anchor and snapshot zero. TRANSFORM requires coordinate_kind,
 * snapshot, position, and affinity zero. RESOLVE requires snapshot, position,
 * and affinity zero. RELEASE requires every semantic field except anchor zero.
 * Create/transform/resolve use token zero to begin and echo it to resume;
 * release requires token zero and detaches in at most one work unit. */

typedef struct FlarkV4CoordinateRequest {
  uint32_t struct_size;
  uint32_t from_kind;
  uint32_t to_kind;
  uint32_t reserved_u32;
  FlarkV4SessionRef session;
  uint64_t revision;
  uint64_t snapshot;
  uint64_t position;
  FlarkV4ProgressToken progress_token;
  FlarkV4WorkBudget budget;
  uint64_t reserved[1];
} FlarkV4CoordinateRequest;
/* COORDINATE_CONVERT is revision-bound, requires snapshot == 0, and uses token
 * zero to begin followed by the latest returned token to resume. */

typedef struct FlarkV4HistoryRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t expected_revision;
  uint64_t history_token;
  FlarkV4ProgressToken progress_token;
  FlarkV4WorkBudget budget;
  uint64_t reserved[1];
} FlarkV4HistoryRequest;
/* HISTORY_REPLAY uses token zero to begin bounded work and echoes it to resume.
 * HISTORY_RELEASE requires progress_token zero and detaches in one work unit. */

typedef struct FlarkV4CancelRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t progress_token;
  uint64_t reserved[4];
} FlarkV4CancelRequest;

typedef struct FlarkV4CloseRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t progress_token;
  FlarkV4WorkBudget budget;
  uint64_t reserved[1];
} FlarkV4CloseRequest;
/* BEGIN requires progress_token == NONE. PUMP and FINISH require the latest
 * nonzero close token. FINISH atomically consumes the sole session handle. */

typedef struct FlarkV4OwnerTransferRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t new_owner_token;
  uint64_t reserved[4];
} FlarkV4OwnerTransferRequest;

typedef struct FlarkV4InspectRequest {
  uint32_t struct_size;
  uint32_t flags;
  FlarkV4SessionRef session;
  uint64_t reserved[5];
} FlarkV4InspectRequest;

typedef struct FlarkV4SessionInspection {
  uint32_t struct_size;
  FlarkV4SessionState session_state;
  FlarkV4SessionHandle session;
  FlarkV4Revision revision;
  uint32_t live_transactions;
  uint32_t live_continuations;
  uint32_t live_anchors;
  uint32_t live_history_tokens;
  uint64_t reserved[3];
} FlarkV4SessionInspection;

#define FLARK_V4_SIZEOF_ABI_INFO UINT32_C(64)
#define FLARK_V4_SIZEOF_OUTCOME UINT32_C(112)
#define FLARK_V4_SIZEOF_RESULT_PAGE_HEADER UINT32_C(96)
#define FLARK_V4_SIZEOF_VIEWPORT_ROW_RECORD UINT32_C(128)
#define FLARK_V4_SIZEOF_PROJECTION_SEGMENT_RECORD UINT32_C(32)
#define FLARK_V4_SIZEOF_INLINE_FACT_RECORD UINT32_C(80)
#define FLARK_V4_SIZEOF_SEMANTIC_TARGET_RECORD UINT32_C(160)
#define FLARK_V4_SIZEOF_CERTIFICATION_RANGE_RECORD UINT32_C(40)
#define FLARK_V4_SIZEOF_SOURCE_RANGE UINT32_C(16)
#define FLARK_V4_SIZEOF_EDIT_DESCRIPTOR UINT32_C(32)
#define FLARK_V4_SIZEOF_WORK_BUDGET UINT32_C(24)
#define FLARK_V4_SIZEOF_SESSION_REF UINT32_C(16)
#define FLARK_V4_SIZEOF_NEGOTIATE_REQUEST UINT32_C(16)
#define FLARK_V4_SIZEOF_SESSION_CONFIG UINT32_C(64)
#define FLARK_V4_SIZEOF_CREATE_REQUEST UINT32_C(88)
#define FLARK_V4_SIZEOF_STAGE_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_SOURCE_READ_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_SMALL_EDIT_REQUEST UINT32_C(88)
#define FLARK_V4_SIZEOF_EDIT_INTENT_REQUEST_V1 UINT32_C(128)
#define FLARK_V4_SIZEOF_EDIT_INTENT_RECEIPT_V1 UINT32_C(192)
#define FLARK_V4_SIZEOF_SOURCE_TRANSACTION_REQUEST_V1 UINT32_C(160)
#define FLARK_V4_SIZEOF_SOURCE_TRANSACTION_RECEIPT_V1 UINT32_C(200)
#define FLARK_V4_SIZEOF_STAGED_SOURCE_TRANSACTION_REQUEST_V1 UINT32_C(160)
#define FLARK_V4_SIZEOF_BULK_BEGIN_REQUEST UINT32_C(72)
#define FLARK_V4_SIZEOF_TRANSACTION_REQUEST UINT32_C(80)
#define FLARK_V4_SIZEOF_PUMP_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_QUERY_REQUEST UINT32_C(96)
#define FLARK_V4_SIZEOF_CONTINUATION_REQUEST UINT32_C(80)
#define FLARK_V4_SIZEOF_ANCHOR_REQUEST UINT32_C(96)
#define FLARK_V4_SIZEOF_COORDINATE_REQUEST UINT32_C(96)
#define FLARK_V4_SIZEOF_HISTORY_REQUEST UINT32_C(80)
#define FLARK_V4_SIZEOF_CANCEL_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_CLOSE_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_OWNER_TRANSFER_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_INSPECT_REQUEST UINT32_C(64)
#define FLARK_V4_SIZEOF_SESSION_INSPECTION UINT32_C(64)

#if defined(__cplusplus)
#define FLARK_V4_STATIC_ASSERT(condition, message) static_assert(condition, message)
#else
#define FLARK_V4_STATIC_ASSERT(condition, message) _Static_assert(condition, message)
#endif

FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4AbiInfo) == FLARK_V4_SIZEOF_ABI_INFO, "FlarkV4AbiInfo layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4Outcome) == FLARK_V4_SIZEOF_OUTCOME, "FlarkV4Outcome layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4ResultPageHeader) == FLARK_V4_SIZEOF_RESULT_PAGE_HEADER, "FlarkV4ResultPageHeader layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4ViewportRowRecord) == FLARK_V4_SIZEOF_VIEWPORT_ROW_RECORD, "FlarkV4ViewportRowRecord layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4ProjectionSegmentRecord) == FLARK_V4_SIZEOF_PROJECTION_SEGMENT_RECORD, "FlarkV4ProjectionSegmentRecord layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4InlineFactRecord) == FLARK_V4_SIZEOF_INLINE_FACT_RECORD, "FlarkV4InlineFactRecord layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SemanticTargetRecord) == FLARK_V4_SIZEOF_SEMANTIC_TARGET_RECORD, "FlarkV4SemanticTargetRecord layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4CertificationRangeRecord) == FLARK_V4_SIZEOF_CERTIFICATION_RANGE_RECORD, "FlarkV4CertificationRangeRecord layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SourceRange) == FLARK_V4_SIZEOF_SOURCE_RANGE, "FlarkV4SourceRange layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4EditDescriptor) == FLARK_V4_SIZEOF_EDIT_DESCRIPTOR, "FlarkV4EditDescriptor layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4WorkBudget) == FLARK_V4_SIZEOF_WORK_BUDGET, "FlarkV4WorkBudget layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SessionRef) == FLARK_V4_SIZEOF_SESSION_REF, "FlarkV4SessionRef layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4NegotiateRequest) == FLARK_V4_SIZEOF_NEGOTIATE_REQUEST, "FlarkV4NegotiateRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SessionConfig) == FLARK_V4_SIZEOF_SESSION_CONFIG, "FlarkV4SessionConfig layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4CreateRequest) == FLARK_V4_SIZEOF_CREATE_REQUEST, "FlarkV4CreateRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4StageRequest) == FLARK_V4_SIZEOF_STAGE_REQUEST, "FlarkV4StageRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SourceReadRequest) == FLARK_V4_SIZEOF_SOURCE_READ_REQUEST, "FlarkV4SourceReadRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SmallEditRequest) == FLARK_V4_SIZEOF_SMALL_EDIT_REQUEST, "FlarkV4SmallEditRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4EditIntentRequestV1) == FLARK_V4_SIZEOF_EDIT_INTENT_REQUEST_V1, "FlarkV4EditIntentRequestV1 layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4EditIntentReceiptV1) == FLARK_V4_SIZEOF_EDIT_INTENT_RECEIPT_V1, "FlarkV4EditIntentReceiptV1 layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SourceTransactionRequestV1) == FLARK_V4_SIZEOF_SOURCE_TRANSACTION_REQUEST_V1, "FlarkV4SourceTransactionRequestV1 layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SourceTransactionReceiptV1) == FLARK_V4_SIZEOF_SOURCE_TRANSACTION_RECEIPT_V1, "FlarkV4SourceTransactionReceiptV1 layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4StagedSourceTransactionRequestV1) == FLARK_V4_SIZEOF_STAGED_SOURCE_TRANSACTION_REQUEST_V1, "FlarkV4StagedSourceTransactionRequestV1 layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4BulkBeginRequest) == FLARK_V4_SIZEOF_BULK_BEGIN_REQUEST, "FlarkV4BulkBeginRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4TransactionRequest) == FLARK_V4_SIZEOF_TRANSACTION_REQUEST, "FlarkV4TransactionRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4PumpRequest) == FLARK_V4_SIZEOF_PUMP_REQUEST, "FlarkV4PumpRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4QueryRequest) == FLARK_V4_SIZEOF_QUERY_REQUEST, "FlarkV4QueryRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4ContinuationRequest) == FLARK_V4_SIZEOF_CONTINUATION_REQUEST, "FlarkV4ContinuationRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4AnchorRequest) == FLARK_V4_SIZEOF_ANCHOR_REQUEST, "FlarkV4AnchorRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4CoordinateRequest) == FLARK_V4_SIZEOF_COORDINATE_REQUEST, "FlarkV4CoordinateRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4HistoryRequest) == FLARK_V4_SIZEOF_HISTORY_REQUEST, "FlarkV4HistoryRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4CancelRequest) == FLARK_V4_SIZEOF_CANCEL_REQUEST, "FlarkV4CancelRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4CloseRequest) == FLARK_V4_SIZEOF_CLOSE_REQUEST, "FlarkV4CloseRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4OwnerTransferRequest) == FLARK_V4_SIZEOF_OWNER_TRANSFER_REQUEST, "FlarkV4OwnerTransferRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4InspectRequest) == FLARK_V4_SIZEOF_INSPECT_REQUEST, "FlarkV4InspectRequest layout");
FLARK_V4_STATIC_ASSERT(sizeof(FlarkV4SessionInspection) == FLARK_V4_SIZEOF_SESSION_INSPECTION, "FlarkV4SessionInspection layout");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4AbiInfo, capability_bits) == 8, "FlarkV4AbiInfo capability offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4Outcome, status) == 8, "FlarkV4Outcome status offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4Outcome, primary_handle) == 16, "FlarkV4Outcome handle offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4Outcome, reserved) == 80, "FlarkV4Outcome reserved offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4ResultPageHeader, revision) == 16, "FlarkV4ResultPageHeader revision offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4ResultPageHeader, requested_range) == 32, "FlarkV4ResultPageHeader requested range offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4ResultPageHeader, covered_range) == 48, "FlarkV4ResultPageHeader covered range offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4ResultPageHeader, item_count) == 64, "FlarkV4ResultPageHeader item count offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4ResultPageHeader, continuation) == 72, "FlarkV4ResultPageHeader continuation offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4SmallEditRequest, edit_count) == 32, "FlarkV4SmallEditRequest edit count offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4SmallEditRequest, replacement_bytes_len) == 40, "FlarkV4SmallEditRequest replacement offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4SmallEditRequest, budget) == 48, "FlarkV4SmallEditRequest budget offset");
FLARK_V4_STATIC_ASSERT(offsetof(FlarkV4QueryRequest, budget) == 64, "FlarkV4QueryRequest budget offset");

FlarkV4StatusCode flark_v4_negotiate(const FlarkV4NegotiateRequest *request, FlarkV4AbiInfo *info, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_create_begin(const FlarkV4CreateRequest *request, const uint8_t *input, uint64_t input_len, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_create_append(const FlarkV4StageRequest *request, const uint8_t *input, uint64_t input_len, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_create_commit(const FlarkV4TransactionRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_create_abort(const FlarkV4TransactionRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_source_read(const FlarkV4SourceReadRequest *request, uint8_t *output, uint64_t output_capacity, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_small_edit(const FlarkV4SmallEditRequest *request, const FlarkV4EditDescriptor *edits, uint32_t edit_count, const uint8_t *replacement_bytes, uint64_t replacement_bytes_len, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_edit_intent_v1(const FlarkV4EditIntentRequestV1 *request, uint8_t *output, uint64_t output_capacity, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_source_transaction_v1(const FlarkV4SourceTransactionRequestV1 *request, const uint8_t *replacement_bytes, uint64_t replacement_bytes_len, uint8_t *output, uint64_t output_capacity, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_staged_source_transaction_v1(const FlarkV4StagedSourceTransactionRequestV1 *request, uint8_t *output, uint64_t output_capacity, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_bulk_begin(const FlarkV4BulkBeginRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_bulk_append(const FlarkV4StageRequest *request, const uint8_t *input, uint64_t input_len, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_bulk_commit(const FlarkV4TransactionRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_bulk_abort(const FlarkV4TransactionRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_pump(const FlarkV4PumpRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_query_viewport(const FlarkV4QueryRequest *request, uint8_t *output, uint64_t output_capacity, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_continuation_next(const FlarkV4ContinuationRequest *request, uint8_t *output, uint64_t output_capacity, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_continuation_release(const FlarkV4ContinuationRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_anchor_create(const FlarkV4AnchorRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_anchor_transform(const FlarkV4AnchorRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_anchor_resolve(const FlarkV4AnchorRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_anchor_release(const FlarkV4AnchorRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_coordinate_convert(const FlarkV4CoordinateRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_history_replay(const FlarkV4HistoryRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_history_release(const FlarkV4HistoryRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_cancel(const FlarkV4CancelRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_close_begin(const FlarkV4CloseRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_close_pump(const FlarkV4CloseRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_close_finish(const FlarkV4CloseRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_session_transfer_owner(const FlarkV4OwnerTransferRequest *request, FlarkV4Outcome *outcome);
FlarkV4StatusCode flark_v4_session_inspect(const FlarkV4InspectRequest *request, FlarkV4SessionInspection *inspection, FlarkV4Outcome *outcome);

#ifdef __cplusplus
}
#endif

#endif

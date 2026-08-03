#ifndef FLARK_COMRAK_BRIDGE_H_
#define FLARK_COMRAK_BRIDGE_H_

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct FlarkComrakResponse {
  uint32_t abi_version;
  uint32_t revision;
  uint16_t status_code;
  uint16_t reserved;
  uint8_t* payload_ptr;
  uint32_t payload_len;
} FlarkComrakResponse;

uint32_t flark_comrak_bridge_version(void);

uint8_t* flark_comrak_input_alloc(uint32_t len);

void flark_comrak_input_free(uint8_t* ptr, uint32_t len);

FlarkComrakResponse* flark_comrak_parse(
    uint32_t revision,
    uint8_t profile,
    const uint8_t* text_ptr,
    uint32_t text_len);

void flark_comrak_response_free(FlarkComrakResponse* response);

/*
 * Flark v3 endpoint registry ABI. This version is independent of both the
 * legacy response ABI above and the FLK3 wire-envelope version.
 *
 * Pointer contract for every v3 call below:
 * - typed pointers are naturally aligned and valid for the complete call;
 * - const inputs remain immutable and outputs are exclusively writable;
 * - every input, output, and byte-buffer range in one call is pairwise
 *   disjoint; and
 * - null is permitted only where a declaration comment explicitly says so.
 * Violating this contract is undefined behavior; a status code is not a
 * substitute for valid C pointer provenance.
 *
 * Dispatch, close, and poll have two result channels. A non-OK status may
 * still mean the endpoint mutated and credited an event (most importantly
 * Failed). Callers must always inspect the initialized receipt and service any
 * nonzero outstanding_event_id before deciding how to proceed.
 */
#define FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION UINT32_C(0x00020002)
#define FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES UINT32_C(262168)
#define FLARK_V3_ENDPOINT_MAXIMUM_POLL_SOURCE_BYTES UINT32_C(65536)
#define FLARK_V3_ENDPOINT_MAXIMUM_POLL_CHECKPOINTS UINT32_C(64)
#define FLARK_V3_ENDPOINT_MAXIMUM_RETIREMENT_TRANSITIONS UINT32_C(256)
#define FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES UINT32_C(65536)
/* Fresh admissions are capped at half the resident-slot ceiling. The other
 * half is reserved so all admitted endpoints may hold one create-before-
 * revoke recovery replacement concurrently. */
#define FLARK_V3_ENDPOINT_MAXIMUM_FRESH_ENDPOINTS UINT32_C(2048)
#define FLARK_V3_ENDPOINT_MAXIMUM_RESIDENT_SLOTS UINT32_C(4096)
#define FLARK_V3_ENDPOINT_CONFIG_BYTES UINT32_C(64)

/* Return codes reuse the stable FLK3 status namespace. */
#define FLARK_V3_STATUS_OK UINT32_C(0x0000)
#define FLARK_V3_STATUS_UNSUPPORTED_MAJOR UINT32_C(0x0010)
#define FLARK_V3_STATUS_UNSUPPORTED_MINOR UINT32_C(0x0011)
#define FLARK_V3_STATUS_MALFORMED_FRAME UINT32_C(0x0012)
#define FLARK_V3_STATUS_UNKNOWN_OPCODE UINT32_C(0x0013)
#define FLARK_V3_STATUS_FORBIDDEN_FLAGS UINT32_C(0x0014)
#define FLARK_V3_STATUS_INVALID UINT32_C(0x0100)
#define FLARK_V3_STATUS_INVALID_STATE UINT32_C(0x0101)
#define FLARK_V3_STATUS_BACKPRESSURE UINT32_C(0x0102)
#define FLARK_V3_STATUS_FOREGROUND_BOUND_EXCEEDED UINT32_C(0x010a)
#define FLARK_V3_STATUS_CLOSED UINT32_C(0x010c)
#define FLARK_V3_STATUS_NOT_READY UINT32_C(0x010d)
#define FLARK_V3_STATUS_UNSUPPORTED_SCHEMA UINT32_C(0x010e)
#define FLARK_V3_STATUS_ALLOCATION_FAILED UINT32_C(0x010f)
#define FLARK_V3_STATUS_INVALID_UTF8 UINT32_C(0x0110)
#define FLARK_V3_STATUS_INTERNAL_FAULT UINT32_C(0x0111)

#define FLARK_V3_ACTION_OPENED UINT32_C(1)
#define FLARK_V3_ACTION_SOURCE_PAGE_ACCEPTED UINT32_C(2)
#define FLARK_V3_ACTION_SOURCE_BATCH_ACCEPTED UINT32_C(3)
#define FLARK_V3_ACTION_SUPERSEDED UINT32_C(4)
#define FLARK_V3_ACTION_EVENT_RECEIPT_ACCEPTED UINT32_C(5)
#define FLARK_V3_ACTION_CLOSE_LATCHED UINT32_C(6)
#define FLARK_V3_ACTION_DRAIN_ADVANCED UINT32_C(7)

#define FLARK_V3_HOST_POLL_ACTION_CREDIT_ACCEPTED UINT32_C(1)
#define FLARK_V3_HOST_POLL_ACTION_CANDIDATE_CANCELLED UINT32_C(2)
#define FLARK_V3_HOST_POLL_ACTION_DELIVERY_EMITTED UINT32_C(3)

#define FLARK_V3_EVENT_NONE UINT32_C(0)
#define FLARK_V3_EVENT_OPENED UINT32_C(1)
#define FLARK_V3_EVENT_SOURCE_SYNCHRONIZED UINT32_C(2)
#define FLARK_V3_EVENT_SOURCE_FACTS_PAGE UINT32_C(3)
#define FLARK_V3_EVENT_SOURCE_FACTS_COMPLETED UINT32_C(4)
#define FLARK_V3_EVENT_FAILED UINT32_C(5)
#define FLARK_V3_EVENT_DRAIN_PROGRESS UINT32_C(6)
#define FLARK_V3_EVENT_CLOSED UINT32_C(7)
#define FLARK_V3_EVENT_PUBLICATION_BEGIN UINT32_C(8)
#define FLARK_V3_EVENT_PUBLICATION_PACKET UINT32_C(9)
#define FLARK_V3_EVENT_PUBLICATION_COMMIT UINT32_C(10)
#define FLARK_V3_EVENT_PUBLICATION_DELIVERY_ACKNOWLEDGED UINT32_C(11)
#define FLARK_V3_EVENT_SOURCE_FACTS_DELTA_BEGIN UINT32_C(12)
#define FLARK_V3_EVENT_SOURCE_FACTS_DELTA_PAGE UINT32_C(13)
#define FLARK_V3_EVENT_SOURCE_FACTS_DELTA_COMPLETED UINT32_C(14)

#define FLARK_V3_CERTIFICATION_NONE UINT32_C(0)
#define FLARK_V3_CERTIFICATION_NOT_STARTED UINT32_C(1)
#define FLARK_V3_CERTIFICATION_SCANNING UINT32_C(2)
#define FLARK_V3_CERTIFICATION_PUBLISHING UINT32_C(3)
#define FLARK_V3_CERTIFICATION_AWAITING_PROMOTION UINT32_C(4)
#define FLARK_V3_CERTIFICATION_EXTERNALLY_ELIGIBLE UINT32_C(5)

typedef struct FlarkV3EndpointHandle {
  uint32_t slot;
  uint32_t generation;
} FlarkV3EndpointHandle;

typedef struct FlarkV3EndpointConfig {
  uint32_t abi_version;
  uint32_t struct_size;
  uint32_t max_retired_sources;
  uint32_t max_retired_source_bytes;
  uint32_t arena_max_slots;
  uint32_t arena_max_live_payload_bytes;
  uint32_t arena_max_children_per_node;
  uint32_t checkpoint_spacing_utf16;
  uint32_t source_facts_max_checkpoints;
  uint32_t source_facts_max_pages;
  uint32_t source_facts_max_resident_bytes;
  uint32_t parser_profile;
  uint32_t reserved[4];
} FlarkV3EndpointConfig;

typedef struct FlarkV3SessionBinding {
  uint32_t document_session[4];
  uint32_t source_session_identity;
  uint32_t worker_generation;
} FlarkV3SessionBinding;

typedef struct FlarkV3DispatchReceipt {
  uint32_t correlation_id;
  uint32_t action;
  uint32_t outstanding_event_id;
  uint32_t outstanding_event_kind;
} FlarkV3DispatchReceipt;

typedef struct FlarkV3PollFuel {
  uint32_t maximum_source_bytes;
  uint32_t maximum_checkpoints;
  uint32_t maximum_retirement_transitions;
} FlarkV3PollFuel;

typedef struct FlarkV3PollReceipt {
  uint32_t source_bytes_examined;
  uint32_t source_bytes_buffered;
  uint32_t cursor_refills;
  uint32_t cursor_copy_bytes_upper_bound;
  uint32_t checkpoints_emitted;
  uint32_t source_fact_transitions;
  uint32_t released_source_leases;
  uint32_t released_source_bytes;
  uint32_t arena_transitions;
  uint32_t arena_nodes_reclaimed;
  uint32_t scan_complete;
  uint32_t certification;
  uint32_t cleanup_complete;
  uint32_t outstanding_event_id;
  uint32_t outstanding_event_kind;
} FlarkV3PollReceipt;

typedef struct FlarkV3CandidatePollReceipt {
  uint32_t transitions;
  uint32_t cleanup_complete;
  uint32_t outstanding_event_id;
  uint32_t outstanding_event_kind;
} FlarkV3CandidatePollReceipt;

#if defined(__cplusplus)
static_assert(sizeof(FlarkV3EndpointHandle) == 8,
              "FlarkV3EndpointHandle ABI drift");
static_assert(offsetof(FlarkV3EndpointHandle, generation) == 4,
              "FlarkV3EndpointHandle field drift");
static_assert(sizeof(FlarkV3EndpointConfig) == 64,
              "FlarkV3EndpointConfig ABI drift");
static_assert(offsetof(FlarkV3EndpointConfig, reserved) == 48,
              "FlarkV3EndpointConfig field drift");
static_assert(sizeof(FlarkV3SessionBinding) == 24,
              "FlarkV3SessionBinding ABI drift");
static_assert(offsetof(FlarkV3SessionBinding, source_session_identity) == 16,
              "FlarkV3SessionBinding field drift");
static_assert(offsetof(FlarkV3SessionBinding, worker_generation) == 20,
              "FlarkV3SessionBinding generation drift");
static_assert(sizeof(FlarkV3DispatchReceipt) == 16,
              "FlarkV3DispatchReceipt ABI drift");
static_assert(offsetof(FlarkV3DispatchReceipt, outstanding_event_id) == 8,
              "FlarkV3DispatchReceipt field drift");
static_assert(sizeof(FlarkV3PollFuel) == 12,
              "FlarkV3PollFuel ABI drift");
static_assert(sizeof(FlarkV3PollReceipt) == 60,
              "FlarkV3PollReceipt ABI drift");
static_assert(offsetof(FlarkV3PollReceipt, outstanding_event_id) == 52,
              "FlarkV3PollReceipt field drift");
static_assert(sizeof(FlarkV3CandidatePollReceipt) == 16,
              "FlarkV3CandidatePollReceipt ABI drift");
static_assert(offsetof(FlarkV3CandidatePollReceipt, outstanding_event_id) == 8,
              "FlarkV3CandidatePollReceipt field drift");
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(sizeof(FlarkV3EndpointHandle) == 8,
               "FlarkV3EndpointHandle ABI drift");
_Static_assert(offsetof(FlarkV3EndpointHandle, generation) == 4,
               "FlarkV3EndpointHandle field drift");
_Static_assert(sizeof(FlarkV3EndpointConfig) == 64,
               "FlarkV3EndpointConfig ABI drift");
_Static_assert(offsetof(FlarkV3EndpointConfig, reserved) == 48,
               "FlarkV3EndpointConfig field drift");
_Static_assert(sizeof(FlarkV3SessionBinding) == 24,
               "FlarkV3SessionBinding ABI drift");
_Static_assert(offsetof(FlarkV3SessionBinding, source_session_identity) == 16,
               "FlarkV3SessionBinding field drift");
_Static_assert(offsetof(FlarkV3SessionBinding, worker_generation) == 20,
               "FlarkV3SessionBinding generation drift");
_Static_assert(sizeof(FlarkV3DispatchReceipt) == 16,
               "FlarkV3DispatchReceipt ABI drift");
_Static_assert(offsetof(FlarkV3DispatchReceipt, outstanding_event_id) == 8,
               "FlarkV3DispatchReceipt field drift");
_Static_assert(sizeof(FlarkV3PollFuel) == 12,
               "FlarkV3PollFuel ABI drift");
_Static_assert(sizeof(FlarkV3PollReceipt) == 60,
               "FlarkV3PollReceipt ABI drift");
_Static_assert(offsetof(FlarkV3PollReceipt, outstanding_event_id) == 52,
               "FlarkV3PollReceipt field drift");
_Static_assert(sizeof(FlarkV3CandidatePollReceipt) == 16,
               "FlarkV3CandidatePollReceipt ABI drift");
_Static_assert(offsetof(FlarkV3CandidatePollReceipt, outstanding_event_id) == 8,
               "FlarkV3CandidatePollReceipt field drift");
#endif

uint32_t flark_v3_endpoint_native_abi_version(void);

/* Private, one-shot Feedback Checkpoint B evidence probe. This is diagnostic
 * JSON, not part of the product document protocol. Callers allocate the
 * declared maximum; an undersized call returns FOREGROUND_BOUND_EXCEEDED and
 * leaves out_written at zero rather than exposing a partial receipt. */
uint32_t flark_v3_checkpoint_b_probe(
    uint8_t* output_ptr,
    uint32_t output_capacity,
    uint32_t* out_written);

uint32_t flark_v3_endpoint_config_standard(
    FlarkV3EndpointConfig* out_config);

uint32_t flark_v3_endpoint_create(
    const FlarkV3EndpointConfig* config,
    FlarkV3EndpointHandle* out_handle);

uint32_t flark_v3_endpoint_recover(
    const FlarkV3EndpointConfig* config,
    const FlarkV3SessionBinding* previous_binding,
    FlarkV3EndpointHandle* out_handle);

/* input_ptr may be null only when input_len is zero. */
uint32_t flark_v3_endpoint_dispatch(
    FlarkV3EndpointHandle handle,
    const uint8_t* input_ptr,
    uint32_t input_len,
    FlarkV3DispatchReceipt* out_receipt);

/* Accepts only a terminal schema-2 HostPoll response. Parser-event receipts
 * remain schema-3 commands handled by flark_v3_endpoint_dispatch. */
uint32_t flark_v3_endpoint_dispatch_host_poll(
    FlarkV3EndpointHandle handle,
    const uint8_t* input_ptr,
    uint32_t input_len,
    FlarkV3DispatchReceipt* out_receipt);

/* Accepts only a terminal schema-4 inline-sidecar HostPoll response. */
uint32_t flark_v3_endpoint_dispatch_inline_sidecar_host_poll(
    FlarkV3EndpointHandle handle,
    const uint8_t* input_ptr,
    uint32_t input_len,
    FlarkV3DispatchReceipt* out_receipt);

/* Accepts only a terminal VPB1 viewport-presentation HostPoll response. */
uint32_t flark_v3_endpoint_dispatch_viewport_presentation_host_poll(
    FlarkV3EndpointHandle handle,
    const uint8_t* input_ptr,
    uint32_t input_len,
    FlarkV3DispatchReceipt* out_receipt);

/* Source-byte and checkpoint fuel must both be nonzero and no greater than
 * their declared maxima. Retirement fuel may be zero. */
uint32_t flark_v3_endpoint_poll(
    FlarkV3EndpointHandle handle,
    FlarkV3PollFuel fuel,
    FlarkV3PollReceipt* out_receipt);

/* Candidate work is independently fuelled and never consumes source-byte or
 * checkpoint grants. maximum_transitions must be in [1, 256]. */
uint32_t flark_v3_endpoint_poll_candidate(
    FlarkV3EndpointHandle handle,
    uint32_t maximum_transitions,
    FlarkV3CandidatePollReceipt* out_receipt);

/* On a too-small buffer, out_written receives the required size and the
 * outstanding event remains replayable. A null buffer is valid only at zero
 * capacity, which provides an allocation-free size query. */
uint32_t flark_v3_endpoint_encode(
    FlarkV3EndpointHandle handle,
    uint8_t* output_ptr,
    uint32_t output_capacity,
    uint32_t* out_written);

/* Accepts only an exact FLK3 close command; other opcodes do not mutate.
 * input_ptr may be null only when input_len is zero. */
uint32_t flark_v3_endpoint_close(
    FlarkV3EndpointHandle handle,
    const uint8_t* input_ptr,
    uint32_t input_len,
    FlarkV3DispatchReceipt* out_receipt);

/* Normal removal succeeds only after the accepted Closed receipt. */
uint32_t flark_v3_endpoint_remove(FlarkV3EndpointHandle handle);

/* Abnormal finalization only; this may perform unmetered containment work.
 * BACKPRESSURE means an operation still owns the endpoint: the handle remains
 * live and no slot was recycled, so an explicit caller may retry later. */
uint32_t flark_v3_endpoint_emergency_destroy(FlarkV3EndpointHandle handle);

/* NativeFinalizer integration uses an opaque heap token that contains only the
 * generation-checked handle, never an Endpoint pointer. The endpoint itself is
 * resolved through the registry when the callback runs.
 *
 * A successful create transfers exactly-once token ownership to the caller.
 * If the finalizer is detached, consume the token with token_release. If it
 * remains attached, NativeFinalizer calls emergency_finalize, whose null token
 * case is an explicit no-op. Dart wrappers must keep their owner reachable
 * through each FFI operation (for example with reachabilityFence) so the
 * callback cannot race an active endpoint operation. A callback race fails
 * closed without recycling the still-live slot. */
uint32_t flark_v3_endpoint_finalizer_token_create(
    FlarkV3EndpointHandle handle,
    void** out_token);

uint32_t flark_v3_endpoint_finalizer_token_release(void* token);

void flark_v3_endpoint_emergency_finalize(void* token);

/*
 * Independent M1.1 publication host. This handle owns a host arena in the
 * caller isolate/address space; it never aliases a parser endpoint arena.
 * HostClosed does not recycle the handle. After observing HOST_OUTCOME_CLOSED,
 * detach the finalizer, prove flark_v3_host_remove (or emergency_destroy)
 * succeeded, and only then release the token. Reattach the finalizer if
 * reclamation cannot be proven.
 */
#define FLARK_V3_HOST_NATIVE_ABI_VERSION UINT32_C(0x00030006)
#define FLARK_V3_HOST_MAXIMUM_FRAME_BYTES UINT32_C(5140)
#define FLARK_V3_HOST_MAXIMUM_PACKET_BYTES UINT32_C(71724)
#define FLARK_V3_HOST_MAXIMUM_QUERY_BYTES UINT32_C(65536)
#define FLARK_V3_HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES UINT32_C(131072)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES UINT32_C(262144)
#define FLARK_V3_HOST_MAXIMUM_RESIDENT_HOSTS UINT32_C(2048)
#define FLARK_V3_HOST_M11_GREEN_RECORD_BYTES UINT32_C(80)
#define FLARK_V3_HOST_M11_PROJECTION_RECORD_BYTES UINT32_C(56)
#define FLARK_V3_HOST_POINT_QUERY_SCHEMA UINT32_C(1)
#define FLARK_V3_HOST_BLOCK_RANGE_QUERY_SCHEMA UINT32_C(1)
#define FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES UINT32_C(64)
#define FLARK_V3_HOST_BLOCK_RANGE_HEADER_BYTES UINT32_C(32)
#define FLARK_V3_HOST_BLOCK_RANGE_RECORD_BYTES UINT32_C(160)
#define FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_QUERY_SCHEMA UINT32_C(1)
#define FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES UINT32_C(4096)
#define FLARK_V3_HOST_M11_VIEWPORT_BYTES UINT32_C(156)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_MODE UINT32_C(1)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_SCHEMA UINT32_C(1)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_PAGE_SCHEMA UINT32_C(10)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_PAGE_HEADER_BYTES UINT32_C(160)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES UINT32_C(152)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_FRAME_BYTES UINT32_C(65536)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_BEGIN_BYTES UINT32_C(348)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_COMMIT_REQUEST_BYTES UINT32_C(56)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_ACK_BYTES UINT32_C(296)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES UINT32_C(324)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES UINT32_C(320)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES UINT32_C(32)

#define FLARK_V3_HOST_REJECT_NONE UINT32_C(0)
#define FLARK_V3_HOST_REJECT_INVALID UINT32_C(1)
#define FLARK_V3_HOST_REJECT_BACKPRESSURE UINT32_C(2)
#define FLARK_V3_HOST_REJECT_STALE_SOURCE UINT32_C(3)
#define FLARK_V3_HOST_REJECT_EXACT_SOURCE_MISMATCH UINT32_C(4)
/* Code 5 is reserved for the shared publication protocol's
 * SESSION_SNAPSHOT_REQUIRED value. This native M1.1 host does not emit it. */
#define FLARK_V3_HOST_REJECT_SESSION_SNAPSHOT_REQUIRED UINT32_C(5)
#define FLARK_V3_HOST_REJECT_BASE_MISMATCH UINT32_C(6)
#define FLARK_V3_HOST_REJECT_WRONG_OFFER UINT32_C(7)
#define FLARK_V3_HOST_REJECT_CORRUPT_PUBLICATION UINT32_C(8)
#define FLARK_V3_HOST_REJECT_QUERY_BOUND_EXCEEDED UINT32_C(9)
#define FLARK_V3_HOST_REJECT_FOREGROUND_BOUND_EXCEEDED UINT32_C(10)
#define FLARK_V3_HOST_REJECT_SUPERSEDED UINT32_C(11)
#define FLARK_V3_HOST_REJECT_CLOSED UINT32_C(12)
/* NotReady, AllocationFailed, and InternalFault use their non-OK native
 * status and leave rejection_reason at NONE; they are not expected races. */

#define FLARK_V3_HOST_OUTCOME_PENDING UINT32_C(0)
#define FLARK_V3_HOST_OUTCOME_PACKET_CREDIT UINT32_C(1)
#define FLARK_V3_HOST_OUTCOME_COMMITTED UINT32_C(2)
#define FLARK_V3_HOST_OUTCOME_ABORT_COMPLETE UINT32_C(3)
#define FLARK_V3_HOST_OUTCOME_CLOSED UINT32_C(4)

#define FLARK_V3_HOST_INLINE_SIDECAR_MODE UINT32_C(1)
#define FLARK_V3_HOST_INLINE_SIDECAR_DISPOSITION_AUTHORITATIVE UINT32_C(1)
#define FLARK_V3_HOST_INLINE_SIDECAR_DISPOSITION_UNSUPPORTED UINT32_C(2)
#define FLARK_V3_HOST_INLINE_SIDECAR_OUTCOME_PENDING UINT32_C(0)
#define FLARK_V3_HOST_INLINE_SIDECAR_OUTCOME_PACKET_CREDIT UINT32_C(1)
#define FLARK_V3_HOST_INLINE_SIDECAR_OUTCOME_COMMITTED UINT32_C(2)
#define FLARK_V3_HOST_INLINE_SIDECAR_OUTCOME_ABORT_COMPLETE UINT32_C(3)
#define FLARK_V3_HOST_INLINE_SIDECAR_OUTCOME_CLOSED UINT32_C(4)
#define FLARK_V3_HOST_INLINE_SIDECAR_QUERY_SCHEMA UINT32_C(3)
#define FLARK_V3_HOST_INLINE_SIDECAR_QUERY_UNAVAILABLE UINT32_C(0)
#define FLARK_V3_HOST_INLINE_SIDECAR_QUERY_AUTHORITATIVE UINT32_C(1)
#define FLARK_V3_HOST_INLINE_SIDECAR_QUERY_UNSUPPORTED UINT32_C(2)
#define FLARK_V3_HOST_INLINE_SIDECAR_PAYLOAD_INLINE UINT32_C(1)
#define FLARK_V3_HOST_INLINE_SIDECAR_PAYLOAD_INDENTED_CODE UINT32_C(2)
#define FLARK_V3_HOST_INLINE_SIDECAR_PAYLOAD_BLOCK_QUOTE UINT32_C(3)
#define FLARK_V3_HOST_INLINE_SIDECAR_PAYLOAD_BULLET_LIST UINT32_C(4)
#define FLARK_V3_HOST_INLINE_SIDECAR_PAYLOAD_ORDERED_LIST_ITEM UINT32_C(6)

#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_OUTCOME_PENDING UINT32_C(0)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_OUTCOME_PACKET_CREDIT UINT32_C(1)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_OUTCOME_COMMITTED UINT32_C(2)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_OUTCOME_ABORT_COMPLETE UINT32_C(3)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_OUTCOME_CLOSED UINT32_C(4)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_UNAVAILABLE UINT32_C(0)
#define FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_AVAILABLE UINT32_C(1)

#define FLARK_V3_HOST_QUERY_AFFINITY_UPSTREAM UINT32_C(0)
#define FLARK_V3_HOST_QUERY_AFFINITY_DOWNSTREAM UINT32_C(1)
#define FLARK_V3_HOST_QUERY_OUTCOME_VIEWPORT UINT32_C(1)
#define FLARK_V3_HOST_QUERY_OUTCOME_SOURCE_GAP UINT32_C(2)
#define FLARK_V3_HOST_QUERY_GAP_OPEN_DEPTH_LIMIT UINT32_C(1)
#define FLARK_V3_HOST_QUERY_GAP_ENCODED_BYTE_LIMIT UINT32_C(2)
#define FLARK_V3_HOST_QUERY_GAP_LEAF_LIMIT UINT32_C(3)
#define FLARK_V3_HOST_QUERY_GAP_TREE_NODE_LIMIT UINT32_C(4)
#define FLARK_V3_HOST_QUERY_GAP_UNDECODABLE_CLOSURE UINT32_C(5)
#define FLARK_V3_HOST_QUERY_GAP_UNAVAILABLE_FACTS UINT32_C(6)

#define FLARK_V3_HOST_ORDINAL_WINDOW_OUTCOME_WINDOW UINT32_C(1)
#define FLARK_V3_HOST_ORDINAL_WINDOW_OUTCOME_FAILURE UINT32_C(2)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_UNAVAILABLE_FACTS UINT32_C(1)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_ENTRY_WINDOW_LIMIT UINT32_C(2)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_STORAGE_PAGE_LIMIT UINT32_C(3)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_TREE_NODE_LIMIT UINT32_C(4)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_PACKED_ENTRY_LIMIT UINT32_C(5)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_ORDINAL_OUT_OF_RANGE UINT32_C(6)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FAILURE_UNDECODABLE_CLOSURE UINT32_C(7)
#define FLARK_V3_HOST_ORDINAL_WINDOW_FLAG_COMPLETE UINT32_C(1)

typedef struct FlarkV3HostHandle {
  uint32_t slot;
  uint32_t generation;
} FlarkV3HostHandle;

typedef struct FlarkV3HostConfig {
  uint32_t abi_version;
  uint32_t struct_size;
  uint32_t document_session[4];
  uint32_t grammar_revision;
  uint32_t syntax_profile;
  uint32_t authority_mask;
  uint32_t maximum_query_bytes;
  uint32_t reserved[4];
} FlarkV3HostConfig;

typedef struct FlarkV3HostSourceVersion {
  uint32_t document_session[4];
  uint32_t revision;
  uint32_t utf8_length;
  uint32_t utf16_length;
  uint32_t content_hash128[4];
} FlarkV3HostSourceVersion;

typedef struct FlarkV3HostOfferBegin {
  uint32_t offer_id[4];
  uint32_t publication_session[4];
  uint32_t target_host_revision;
  FlarkV3HostSourceVersion source_version;
  /* Canonical Dart word order: highWord, lowWord. */
  uint32_t source_root[2];
  uint32_t parse_generation;
  uint32_t grammar_revision;
  uint32_t syntax_profile;
  uint32_t authority_mask;
  uint32_t transferred_record_count;
  uint32_t target_record_count;
  uint32_t maximum_frame_count;
  uint32_t maximum_encoded_frame_bytes;
  uint32_t maximum_packet_bytes;
  uint32_t maximum_frame_bytes;
  uint32_t maximum_program_children;
  uint32_t reserved[3];
} FlarkV3HostOfferBegin;

typedef struct FlarkV3HostCommitRequest {
  uint32_t offer_id[4];
  uint32_t actual_frame_count;
  uint32_t actual_encoded_frame_bytes;
  uint32_t rolling_transport_digest[4];
  uint32_t canonical_stream_digest[4];
} FlarkV3HostCommitRequest;

typedef struct FlarkV3HostStructuralAck {
  uint32_t publication_session[4];
  uint32_t host_revision;
  FlarkV3HostSourceVersion source_version;
  /* Canonical Dart word order: highWord, lowWord. */
  uint32_t source_root[2];
  uint32_t parse_generation;
  uint32_t grammar_revision;
  uint32_t syntax_profile;
  uint32_t authority_mask;
  uint32_t record_count;
  uint32_t sequence_digest[4];
  uint32_t manifest_digest[4];
} FlarkV3HostStructuralAck;

/* Exact little-endian u64 lanes. This layout is lossless in Dart2JS. */
typedef struct FlarkV3HostU64 {
  uint32_t low_word;
  uint32_t high_word;
} FlarkV3HostU64;

typedef struct FlarkV3HostInlineSidecarBinding {
  FlarkV3HostU64 parser_profile;
  FlarkV3HostU64 refinement_generation;
  FlarkV3HostU64 block_ordinal;
  uint32_t physical_start_utf8;
  uint32_t physical_end_utf8;
  uint32_t visible_start_utf8;
  uint32_t visible_end_utf8;
  uint32_t physical_start_utf16;
  uint32_t physical_end_utf16;
  uint32_t visible_start_utf16;
  uint32_t visible_end_utf16;
} FlarkV3HostInlineSidecarBinding;

typedef struct FlarkV3HostInlineSidecarDisposition {
  uint32_t disposition;
  uint32_t reason;
  FlarkV3HostU64 logical_page_count;
  FlarkV3HostU64 fact_count;
  FlarkV3HostU64 storage_page_count;
  uint32_t link_value_entry_count;
  uint32_t link_value_encoded_bytes;
  FlarkV3HostU64 link_value_storage_page_count;
  uint32_t commitment256[8];
} FlarkV3HostInlineSidecarDisposition;

typedef struct FlarkV3HostInlineSidecarBegin {
  uint32_t schema;
  uint32_t mode;
  uint32_t offer_id[4];
  uint32_t publication_session[4];
  FlarkV3HostStructuralAck base_ack;
  FlarkV3HostInlineSidecarBinding binding;
  uint32_t hio1_encoded_bytes;
  uint32_t ipr2_descriptor_bytes;
  uint32_t transferred_node_count;
  FlarkV3HostInlineSidecarDisposition sidecar_disposition;
  uint32_t hio1_envelope_digest256[8];
  uint32_t maximum_frame_count;
  uint32_t maximum_encoded_frame_bytes;
  uint32_t maximum_packet_bytes;
  uint32_t maximum_frame_bytes;
  uint32_t maximum_program_children;
} FlarkV3HostInlineSidecarBegin;

typedef struct FlarkV3HostInlineSidecarCommitRequest {
  uint32_t offer_id[4];
  uint32_t actual_frame_count;
  uint32_t actual_encoded_frame_bytes;
  uint32_t rolling_transport_digest[4];
  uint32_t root_stream_digest[4];
} FlarkV3HostInlineSidecarCommitRequest;

typedef struct FlarkV3HostInlineSidecarAck {
  uint32_t publication_session[4];
  FlarkV3HostStructuralAck base_ack;
  FlarkV3HostU64 refinement_generation;
  FlarkV3HostU64 block_ordinal;
  uint32_t transferred_node_count;
  uint32_t disposition;
  uint32_t hio1_envelope_digest256[8];
  uint32_t root_stream_digest[4];
} FlarkV3HostInlineSidecarAck;

typedef struct FlarkV3HostInlineSidecarPollReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t offer_id[4];
  uint32_t next_frame_ordinal;
  FlarkV3HostInlineSidecarAck ack;
} FlarkV3HostInlineSidecarPollReceipt;

typedef struct FlarkV3HostInlineSidecarQuery {
  uint32_t schema;
  uint32_t struct_size;
  FlarkV3HostInlineSidecarBinding binding;
  uint32_t maximum_encoded_bytes;
  uint32_t reserved[3];
} FlarkV3HostInlineSidecarQuery;

typedef struct FlarkV3HostInlineSidecarQueryReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t reason;
  uint32_t encoded_bytes;
  uint32_t fact_count;
  uint32_t tree_nodes_visited;
  uint32_t value_entry_count;
  uint32_t value_encoded_bytes;
  uint32_t payload_kind;
} FlarkV3HostInlineSidecarQueryReceipt;

typedef struct FlarkV3HostViewportPresentationMetricRange {
  uint32_t start_utf8;
  uint32_t start_utf16;
  uint32_t end_utf8;
  uint32_t end_utf16;
} FlarkV3HostViewportPresentationMetricRange;

typedef struct FlarkV3HostViewportPresentationVisitStart {
  FlarkV3HostU64 block_ordinal;
  uint32_t utf8_offset;
  uint32_t utf16_offset;
} FlarkV3HostViewportPresentationVisitStart;

typedef struct FlarkV3HostViewportPresentationBinding {
  uint32_t viewport_generation;
  FlarkV3HostViewportPresentationMetricRange requested_range;
  FlarkV3HostViewportPresentationMetricRange covered_range;
  FlarkV3HostViewportPresentationVisitStart start;
  FlarkV3HostViewportPresentationVisitStart next;
  /* Zero is false and one is true. */
  uint32_t complete;
} FlarkV3HostViewportPresentationBinding;

typedef struct FlarkV3HostViewportPresentationEnvelope {
  uint32_t visited_structural_entries;
  uint32_t visited_storage_pages;
  uint32_t ordered_leaf_count;
  uint32_t inline_source_bytes;
  uint32_t fact_count;
  uint32_t transferred_node_count;
  uint32_t parser_transitions;
  uint32_t aggregate_envelope_digest256[8];
} FlarkV3HostViewportPresentationEnvelope;

typedef struct FlarkV3HostViewportPresentationQueryLimits {
  uint32_t maximum_structural_entries;
  uint32_t maximum_storage_pages;
  uint32_t maximum_inline_leaves;
  uint32_t maximum_inline_leaf_source_bytes;
  uint32_t maximum_inline_source_bytes;
  uint32_t maximum_fact_records;
  uint32_t maximum_encoded_frame_bytes;
  uint32_t maximum_parser_transitions;
} FlarkV3HostViewportPresentationQueryLimits;

typedef struct FlarkV3HostViewportPresentationOfferLimits {
  uint32_t maximum_frame_count;
  uint32_t maximum_encoded_frame_bytes;
  uint32_t maximum_packet_bytes;
  uint32_t maximum_frame_bytes;
  uint32_t maximum_program_children;
} FlarkV3HostViewportPresentationOfferLimits;

typedef struct FlarkV3HostViewportPresentationBegin {
  uint32_t schema;
  uint32_t mode;
  uint32_t offer_id[4];
  uint32_t publication_session[4];
  FlarkV3HostStructuralAck base_ack;
  FlarkV3HostViewportPresentationBinding binding;
  FlarkV3HostViewportPresentationEnvelope envelope;
  FlarkV3HostViewportPresentationQueryLimits query_limits;
  FlarkV3HostViewportPresentationOfferLimits limits;
} FlarkV3HostViewportPresentationBegin;

typedef struct FlarkV3HostViewportPresentationCommitRequest {
  uint32_t offer_id[4];
  uint32_t actual_frame_count;
  uint32_t actual_encoded_frame_bytes;
  uint32_t rolling_transport_digest[4];
  uint32_t aggregate_root_stream_digest[4];
} FlarkV3HostViewportPresentationCommitRequest;

typedef struct FlarkV3HostViewportPresentationAck {
  uint32_t publication_session[4];
  FlarkV3HostStructuralAck base_ack;
  FlarkV3HostViewportPresentationBinding binding;
  FlarkV3HostViewportPresentationEnvelope envelope;
  uint32_t actual_frame_count;
  uint32_t actual_encoded_frame_bytes;
  uint32_t aggregate_root_stream_digest[4];
} FlarkV3HostViewportPresentationAck;

typedef struct FlarkV3HostViewportPresentationPollReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t offer_id[4];
  uint32_t next_frame_ordinal;
  FlarkV3HostViewportPresentationAck ack;
} FlarkV3HostViewportPresentationPollReceipt;

typedef struct FlarkV3HostViewportPresentationQuery {
  uint32_t schema;
  uint32_t struct_size;
  FlarkV3HostViewportPresentationAck ack;
  uint32_t maximum_encoded_bytes;
  uint32_t reserved[3];
} FlarkV3HostViewportPresentationQuery;

typedef struct FlarkV3HostViewportPresentationQueryReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t encoded_bytes;
  uint32_t entry_count;
  uint32_t reserved[4];
} FlarkV3HostViewportPresentationQueryReceipt;

typedef struct FlarkV3HostCallReceipt {
  uint32_t rejection_reason;
} FlarkV3HostCallReceipt;

typedef struct FlarkV3HostWorkGrant {
  uint32_t inspect_bytes;
  uint32_t copy_bytes;
  uint32_t transitions;
} FlarkV3HostWorkGrant;

typedef struct FlarkV3HostPollReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t offer_id[4];
  uint32_t next_frame_ordinal;
  FlarkV3HostStructuralAck ack;
} FlarkV3HostPollReceipt;

typedef struct FlarkV3HostId128 {
  uint32_t words[4];
} FlarkV3HostId128;

typedef struct FlarkV3HostPointQuery {
  uint32_t schema;
  uint32_t struct_size;
  FlarkV3HostSourceVersion source_version;
  uint32_t position_utf8;
  uint32_t position_utf16;
  uint32_t affinity;
  uint32_t maximum_encoded_bytes;
  uint32_t maximum_open_depth;
  uint32_t maximum_leaf_count;
  uint32_t maximum_tree_nodes_visited;
  uint32_t reserved[4];
} FlarkV3HostPointQuery;

typedef struct FlarkV3HostPointQueryReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t gap_reason;
  uint32_t encoded_bytes;
  uint32_t leaf_count;
  uint32_t open_depth;
  uint32_t tree_nodes_visited;
  uint32_t summary_nodes_skipped;
  uint32_t range_start_utf8;
  uint32_t range_start_utf16;
  uint32_t range_end_utf8;
  uint32_t range_end_utf16;
  FlarkV3HostSourceVersion source_version;
  uint32_t reserved[5];
} FlarkV3HostPointQueryReceipt;

typedef struct FlarkV3HostBlockRangeQuery {
  uint32_t schema;
  uint32_t struct_size;
  FlarkV3HostSourceVersion source_version;
  uint32_t requested_start_utf8;
  uint32_t requested_start_utf16;
  uint32_t requested_end_utf8;
  uint32_t requested_end_utf16;
  uint32_t maximum_encoded_bytes;
  uint32_t maximum_block_count;
  uint32_t maximum_storage_pages_visited;
  uint32_t maximum_open_depth;
  uint32_t maximum_tree_nodes_visited;
  uint32_t continuation_length;
  uint8_t continuation[FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES];
  uint32_t reserved[4];
} FlarkV3HostBlockRangeQuery;

typedef struct FlarkV3HostBlockRangeQueryReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t gap_reason;
  uint32_t encoded_bytes;
  uint32_t block_count;
  uint32_t storage_pages_visited;
  uint32_t open_depth;
  uint32_t tree_nodes_visited;
  uint32_t packed_entries_inspected;
  uint32_t summary_nodes_skipped;
  uint32_t flags;
  uint32_t coverage_start_utf8;
  uint32_t coverage_start_utf16;
  uint32_t coverage_end_utf8;
  uint32_t coverage_end_utf16;
  FlarkV3HostSourceVersion source_version;
  uint32_t continuation_length;
  uint8_t continuation[FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES];
  uint32_t reserved[4];
} FlarkV3HostBlockRangeQueryReceipt;

typedef struct FlarkV3HostStructuralOrdinalWindowQuery {
  uint32_t schema;
  uint32_t struct_size;
  FlarkV3HostSourceVersion source_version;
  FlarkV3HostU64 start_entry_ordinal;
  uint32_t maximum_entries;
  uint32_t maximum_storage_pages_visited;
  uint32_t maximum_tree_nodes_visited;
  uint32_t maximum_packed_entries_inspected;
  uint32_t reserved[4];
} FlarkV3HostStructuralOrdinalWindowQuery;

typedef struct FlarkV3HostStructuralOrdinalWindowQueryReceipt {
  uint32_t rejection_reason;
  uint32_t outcome;
  uint32_t failure_reason;
  uint32_t flags;
  FlarkV3HostU64 total_entry_count;
  FlarkV3HostU64 start_entry_ordinal;
  FlarkV3HostU64 next_entry_ordinal;
  uint32_t start_utf8;
  uint32_t start_utf16;
  uint32_t next_utf8;
  uint32_t next_utf16;
  uint32_t storage_pages_visited;
  uint32_t tree_nodes_visited;
  uint32_t packed_entries_inspected;
  uint32_t summary_nodes_skipped;
  FlarkV3HostSourceVersion source_version;
  uint32_t reserved[4];
} FlarkV3HostStructuralOrdinalWindowQueryReceipt;

#if defined(__cplusplus)
static_assert(sizeof(FlarkV3HostHandle) == 8, "FlarkV3HostHandle ABI drift");
static_assert(sizeof(FlarkV3HostConfig) == 56, "FlarkV3HostConfig ABI drift");
static_assert(offsetof(FlarkV3HostConfig, reserved) == 40,
              "FlarkV3HostConfig field drift");
static_assert(sizeof(FlarkV3HostSourceVersion) == 44,
              "FlarkV3HostSourceVersion ABI drift");
static_assert(sizeof(FlarkV3HostOfferBegin) == 144,
              "FlarkV3HostOfferBegin ABI drift");
static_assert(sizeof(FlarkV3HostCommitRequest) == 56,
              "FlarkV3HostCommitRequest ABI drift");
static_assert(sizeof(FlarkV3HostStructuralAck) == 124,
              "FlarkV3HostStructuralAck ABI drift");
static_assert(sizeof(FlarkV3HostU64) == 8, "FlarkV3HostU64 ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarBinding) == 56,
              "FlarkV3HostInlineSidecarBinding ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarDisposition) == 80,
              "FlarkV3HostInlineSidecarDisposition ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarBegin) == 364,
              "FlarkV3HostInlineSidecarBegin ABI drift");
static_assert(offsetof(FlarkV3HostInlineSidecarBegin, base_ack) == 40,
              "FlarkV3HostInlineSidecarBegin base ACK drift");
static_assert(offsetof(FlarkV3HostInlineSidecarBegin, binding) == 164,
              "FlarkV3HostInlineSidecarBegin binding drift");
static_assert(sizeof(FlarkV3HostInlineSidecarCommitRequest) == 56,
              "FlarkV3HostInlineSidecarCommitRequest ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarAck) == 212,
              "FlarkV3HostInlineSidecarAck ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarPollReceipt) == 240,
              "FlarkV3HostInlineSidecarPollReceipt ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarQuery) == 80,
              "FlarkV3HostInlineSidecarQuery ABI drift");
static_assert(sizeof(FlarkV3HostInlineSidecarQueryReceipt) == 36,
              "FlarkV3HostInlineSidecarQueryReceipt ABI drift");
static_assert(offsetof(FlarkV3HostInlineSidecarQueryReceipt, payload_kind) == 32,
              "FlarkV3HostInlineSidecarQueryReceipt payload kind drift");
static_assert(sizeof(FlarkV3HostViewportPresentationMetricRange) == 16,
              "FlarkV3HostViewportPresentationMetricRange ABI drift");
static_assert(sizeof(FlarkV3HostViewportPresentationVisitStart) == 16,
              "FlarkV3HostViewportPresentationVisitStart ABI drift");
static_assert(sizeof(FlarkV3HostViewportPresentationBinding) == 72,
              "FlarkV3HostViewportPresentationBinding ABI drift");
static_assert(offsetof(FlarkV3HostViewportPresentationBinding, complete) == 68,
              "FlarkV3HostViewportPresentationBinding field drift");
static_assert(sizeof(FlarkV3HostViewportPresentationEnvelope) == 60,
              "FlarkV3HostViewportPresentationEnvelope ABI drift");
static_assert(sizeof(FlarkV3HostViewportPresentationQueryLimits) == 32,
              "FlarkV3HostViewportPresentationQueryLimits ABI drift");
static_assert(sizeof(FlarkV3HostViewportPresentationOfferLimits) == 20,
              "FlarkV3HostViewportPresentationOfferLimits ABI drift");
static_assert(sizeof(FlarkV3HostViewportPresentationBegin) ==
                  FLARK_V3_HOST_VIEWPORT_PRESENTATION_BEGIN_BYTES,
              "FlarkV3HostViewportPresentationBegin ABI drift");
static_assert(offsetof(FlarkV3HostViewportPresentationBegin, base_ack) == 40,
              "FlarkV3HostViewportPresentationBegin base ACK drift");
static_assert(offsetof(FlarkV3HostViewportPresentationBegin, binding) == 164,
              "FlarkV3HostViewportPresentationBegin binding drift");
static_assert(offsetof(FlarkV3HostViewportPresentationBegin, envelope) == 236,
              "FlarkV3HostViewportPresentationBegin envelope drift");
static_assert(offsetof(FlarkV3HostViewportPresentationBegin, query_limits) == 296,
              "FlarkV3HostViewportPresentationBegin query limits drift");
static_assert(offsetof(FlarkV3HostViewportPresentationBegin, limits) == 328,
              "FlarkV3HostViewportPresentationBegin transport limits drift");
static_assert(sizeof(FlarkV3HostViewportPresentationCommitRequest) ==
                  FLARK_V3_HOST_VIEWPORT_PRESENTATION_COMMIT_REQUEST_BYTES,
              "FlarkV3HostViewportPresentationCommitRequest ABI drift");
static_assert(sizeof(FlarkV3HostViewportPresentationAck) ==
                  FLARK_V3_HOST_VIEWPORT_PRESENTATION_ACK_BYTES,
              "FlarkV3HostViewportPresentationAck ABI drift");
static_assert(offsetof(FlarkV3HostViewportPresentationAck, binding) == 140,
              "FlarkV3HostViewportPresentationAck binding drift");
static_assert(offsetof(FlarkV3HostViewportPresentationAck, envelope) == 212,
              "FlarkV3HostViewportPresentationAck envelope drift");
static_assert(sizeof(FlarkV3HostViewportPresentationPollReceipt) ==
                  FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES,
              "FlarkV3HostViewportPresentationPollReceipt ABI drift");
static_assert(offsetof(FlarkV3HostViewportPresentationPollReceipt, ack) == 28,
              "FlarkV3HostViewportPresentationPollReceipt ACK drift");
static_assert(sizeof(FlarkV3HostViewportPresentationQuery) ==
                  FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES,
              "FlarkV3HostViewportPresentationQuery ABI drift");
static_assert(offsetof(FlarkV3HostViewportPresentationQuery,
                       maximum_encoded_bytes) == 304,
              "FlarkV3HostViewportPresentationQuery bound drift");
static_assert(offsetof(FlarkV3HostViewportPresentationQuery, reserved) == 308,
              "FlarkV3HostViewportPresentationQuery reserved drift");
static_assert(sizeof(FlarkV3HostViewportPresentationQueryReceipt) ==
                  FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES,
              "FlarkV3HostViewportPresentationQueryReceipt ABI drift");
static_assert(sizeof(FlarkV3HostPollReceipt) == 152,
              "FlarkV3HostPollReceipt ABI drift");
static_assert(sizeof(FlarkV3HostPointQuery) == 96,
              "FlarkV3HostPointQuery ABI drift");
static_assert(offsetof(FlarkV3HostPointQuery, source_version) == 8,
              "FlarkV3HostPointQuery source field drift");
static_assert(offsetof(FlarkV3HostPointQuery, position_utf8) == 52,
              "FlarkV3HostPointQuery position field drift");
static_assert(offsetof(FlarkV3HostPointQuery, reserved) == 80,
              "FlarkV3HostPointQuery reserved field drift");
static_assert(sizeof(FlarkV3HostPointQueryReceipt) == 112,
              "FlarkV3HostPointQueryReceipt ABI drift");
static_assert(offsetof(FlarkV3HostPointQueryReceipt, source_version) == 48,
              "FlarkV3HostPointQueryReceipt source field drift");
static_assert(offsetof(FlarkV3HostPointQueryReceipt, reserved) == 92,
              "FlarkV3HostPointQueryReceipt reserved field drift");
static_assert(sizeof(FlarkV3HostBlockRangeQuery) == 172,
              "FlarkV3HostBlockRangeQuery ABI drift");
static_assert(offsetof(FlarkV3HostBlockRangeQuery, continuation) == 92,
              "FlarkV3HostBlockRangeQuery continuation field drift");
static_assert(offsetof(FlarkV3HostBlockRangeQuery, reserved) == 156,
              "FlarkV3HostBlockRangeQuery reserved field drift");
static_assert(sizeof(FlarkV3HostBlockRangeQueryReceipt) == 188,
              "FlarkV3HostBlockRangeQueryReceipt ABI drift");
static_assert(offsetof(FlarkV3HostBlockRangeQueryReceipt, source_version) == 60,
              "FlarkV3HostBlockRangeQueryReceipt source field drift");
static_assert(offsetof(FlarkV3HostBlockRangeQueryReceipt, continuation) == 108,
              "FlarkV3HostBlockRangeQueryReceipt continuation field drift");
static_assert(offsetof(FlarkV3HostBlockRangeQueryReceipt, reserved) == 172,
              "FlarkV3HostBlockRangeQueryReceipt reserved field drift");
static_assert(sizeof(FlarkV3HostStructuralOrdinalWindowQuery) == 92,
              "FlarkV3HostStructuralOrdinalWindowQuery ABI drift");
static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQuery,
                       start_entry_ordinal) == 52,
              "FlarkV3HostStructuralOrdinalWindowQuery start field drift");
static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQuery, reserved) == 76,
              "FlarkV3HostStructuralOrdinalWindowQuery reserved field drift");
static_assert(sizeof(FlarkV3HostStructuralOrdinalWindowQueryReceipt) == 132,
              "FlarkV3HostStructuralOrdinalWindowQueryReceipt ABI drift");
static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                       total_entry_count) == 16,
              "FlarkV3HostStructuralOrdinalWindowQueryReceipt total field drift");
static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                       start_utf8) == 40,
              "FlarkV3HostStructuralOrdinalWindowQueryReceipt cut field drift");
static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                       source_version) == 72,
              "FlarkV3HostStructuralOrdinalWindowQueryReceipt source field drift");
static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                       reserved) == 116,
              "FlarkV3HostStructuralOrdinalWindowQueryReceipt reserved field drift");
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(sizeof(FlarkV3HostHandle) == 8, "FlarkV3HostHandle ABI drift");
_Static_assert(sizeof(FlarkV3HostConfig) == 56, "FlarkV3HostConfig ABI drift");
_Static_assert(offsetof(FlarkV3HostConfig, reserved) == 40,
               "FlarkV3HostConfig field drift");
_Static_assert(sizeof(FlarkV3HostSourceVersion) == 44,
               "FlarkV3HostSourceVersion ABI drift");
_Static_assert(sizeof(FlarkV3HostOfferBegin) == 144,
               "FlarkV3HostOfferBegin ABI drift");
_Static_assert(sizeof(FlarkV3HostCommitRequest) == 56,
               "FlarkV3HostCommitRequest ABI drift");
_Static_assert(sizeof(FlarkV3HostStructuralAck) == 124,
               "FlarkV3HostStructuralAck ABI drift");
_Static_assert(sizeof(FlarkV3HostU64) == 8, "FlarkV3HostU64 ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarBinding) == 56,
               "FlarkV3HostInlineSidecarBinding ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarDisposition) == 80,
               "FlarkV3HostInlineSidecarDisposition ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarBegin) == 364,
               "FlarkV3HostInlineSidecarBegin ABI drift");
_Static_assert(offsetof(FlarkV3HostInlineSidecarBegin, base_ack) == 40,
               "FlarkV3HostInlineSidecarBegin base ACK drift");
_Static_assert(offsetof(FlarkV3HostInlineSidecarBegin, binding) == 164,
               "FlarkV3HostInlineSidecarBegin binding drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarCommitRequest) == 56,
               "FlarkV3HostInlineSidecarCommitRequest ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarAck) == 212,
               "FlarkV3HostInlineSidecarAck ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarPollReceipt) == 240,
               "FlarkV3HostInlineSidecarPollReceipt ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarQuery) == 80,
               "FlarkV3HostInlineSidecarQuery ABI drift");
_Static_assert(sizeof(FlarkV3HostInlineSidecarQueryReceipt) == 36,
               "FlarkV3HostInlineSidecarQueryReceipt ABI drift");
_Static_assert(offsetof(FlarkV3HostInlineSidecarQueryReceipt, payload_kind) == 32,
               "FlarkV3HostInlineSidecarQueryReceipt payload kind drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationMetricRange) == 16,
               "FlarkV3HostViewportPresentationMetricRange ABI drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationVisitStart) == 16,
               "FlarkV3HostViewportPresentationVisitStart ABI drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationBinding) == 72,
               "FlarkV3HostViewportPresentationBinding ABI drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationBinding, complete) == 68,
               "FlarkV3HostViewportPresentationBinding field drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationEnvelope) == 60,
               "FlarkV3HostViewportPresentationEnvelope ABI drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationQueryLimits) == 32,
               "FlarkV3HostViewportPresentationQueryLimits ABI drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationOfferLimits) == 20,
               "FlarkV3HostViewportPresentationOfferLimits ABI drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationBegin) ==
                   FLARK_V3_HOST_VIEWPORT_PRESENTATION_BEGIN_BYTES,
               "FlarkV3HostViewportPresentationBegin ABI drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationBegin, base_ack) == 40,
               "FlarkV3HostViewportPresentationBegin base ACK drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationBegin, binding) == 164,
               "FlarkV3HostViewportPresentationBegin binding drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationBegin, envelope) == 236,
               "FlarkV3HostViewportPresentationBegin envelope drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationBegin, query_limits) == 296,
               "FlarkV3HostViewportPresentationBegin query limits drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationBegin, limits) == 328,
               "FlarkV3HostViewportPresentationBegin transport limits drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationCommitRequest) ==
                   FLARK_V3_HOST_VIEWPORT_PRESENTATION_COMMIT_REQUEST_BYTES,
               "FlarkV3HostViewportPresentationCommitRequest ABI drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationAck) ==
                   FLARK_V3_HOST_VIEWPORT_PRESENTATION_ACK_BYTES,
               "FlarkV3HostViewportPresentationAck ABI drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationAck, binding) == 140,
               "FlarkV3HostViewportPresentationAck binding drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationAck, envelope) == 212,
               "FlarkV3HostViewportPresentationAck envelope drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationPollReceipt) ==
                   FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES,
               "FlarkV3HostViewportPresentationPollReceipt ABI drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationPollReceipt, ack) == 28,
               "FlarkV3HostViewportPresentationPollReceipt ACK drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationQuery) ==
                   FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES,
               "FlarkV3HostViewportPresentationQuery ABI drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationQuery,
                        maximum_encoded_bytes) == 304,
               "FlarkV3HostViewportPresentationQuery bound drift");
_Static_assert(offsetof(FlarkV3HostViewportPresentationQuery, reserved) == 308,
               "FlarkV3HostViewportPresentationQuery reserved drift");
_Static_assert(sizeof(FlarkV3HostViewportPresentationQueryReceipt) ==
                   FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES,
               "FlarkV3HostViewportPresentationQueryReceipt ABI drift");
_Static_assert(sizeof(FlarkV3HostPollReceipt) == 152,
               "FlarkV3HostPollReceipt ABI drift");
_Static_assert(sizeof(FlarkV3HostPointQuery) == 96,
               "FlarkV3HostPointQuery ABI drift");
_Static_assert(offsetof(FlarkV3HostPointQuery, source_version) == 8,
               "FlarkV3HostPointQuery source field drift");
_Static_assert(offsetof(FlarkV3HostPointQuery, position_utf8) == 52,
               "FlarkV3HostPointQuery position field drift");
_Static_assert(offsetof(FlarkV3HostPointQuery, reserved) == 80,
               "FlarkV3HostPointQuery reserved field drift");
_Static_assert(sizeof(FlarkV3HostPointQueryReceipt) == 112,
               "FlarkV3HostPointQueryReceipt ABI drift");
_Static_assert(offsetof(FlarkV3HostPointQueryReceipt, source_version) == 48,
               "FlarkV3HostPointQueryReceipt source field drift");
_Static_assert(offsetof(FlarkV3HostPointQueryReceipt, reserved) == 92,
               "FlarkV3HostPointQueryReceipt reserved field drift");
_Static_assert(sizeof(FlarkV3HostBlockRangeQuery) == 172,
               "FlarkV3HostBlockRangeQuery ABI drift");
_Static_assert(offsetof(FlarkV3HostBlockRangeQuery, continuation) == 92,
               "FlarkV3HostBlockRangeQuery continuation field drift");
_Static_assert(offsetof(FlarkV3HostBlockRangeQuery, reserved) == 156,
               "FlarkV3HostBlockRangeQuery reserved field drift");
_Static_assert(sizeof(FlarkV3HostBlockRangeQueryReceipt) == 188,
               "FlarkV3HostBlockRangeQueryReceipt ABI drift");
_Static_assert(offsetof(FlarkV3HostBlockRangeQueryReceipt, source_version) == 60,
               "FlarkV3HostBlockRangeQueryReceipt source field drift");
_Static_assert(offsetof(FlarkV3HostBlockRangeQueryReceipt, continuation) == 108,
               "FlarkV3HostBlockRangeQueryReceipt continuation field drift");
_Static_assert(offsetof(FlarkV3HostBlockRangeQueryReceipt, reserved) == 172,
               "FlarkV3HostBlockRangeQueryReceipt reserved field drift");
_Static_assert(sizeof(FlarkV3HostStructuralOrdinalWindowQuery) == 92,
               "FlarkV3HostStructuralOrdinalWindowQuery ABI drift");
_Static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQuery,
                        start_entry_ordinal) == 52,
               "FlarkV3HostStructuralOrdinalWindowQuery start field drift");
_Static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQuery, reserved) == 76,
               "FlarkV3HostStructuralOrdinalWindowQuery reserved field drift");
_Static_assert(sizeof(FlarkV3HostStructuralOrdinalWindowQueryReceipt) == 132,
               "FlarkV3HostStructuralOrdinalWindowQueryReceipt ABI drift");
_Static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                        total_entry_count) == 16,
               "FlarkV3HostStructuralOrdinalWindowQueryReceipt total field drift");
_Static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                        start_utf8) == 40,
               "FlarkV3HostStructuralOrdinalWindowQueryReceipt cut field drift");
_Static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                        source_version) == 72,
               "FlarkV3HostStructuralOrdinalWindowQueryReceipt source field drift");
_Static_assert(offsetof(FlarkV3HostStructuralOrdinalWindowQueryReceipt,
                        reserved) == 116,
               "FlarkV3HostStructuralOrdinalWindowQueryReceipt reserved field drift");
#endif

uint32_t flark_v3_host_native_abi_version(void);

uint32_t flark_v3_host_config_standard(FlarkV3HostConfig* out_config);

uint32_t flark_v3_host_create(
    const FlarkV3HostConfig* config,
    FlarkV3HostHandle* out_handle);

uint32_t flark_v3_host_observe_source(
    FlarkV3HostHandle handle,
    const FlarkV3HostSourceVersion* source,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_begin_offer(
    FlarkV3HostHandle handle,
    const FlarkV3HostOfferBegin* begin,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_begin_inline_sidecar_offer(
    FlarkV3HostHandle handle,
    const FlarkV3HostInlineSidecarBegin* begin,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_begin_viewport_presentation_offer(
    FlarkV3HostHandle handle,
    const FlarkV3HostViewportPresentationBegin* begin,
    FlarkV3HostCallReceipt* out_receipt);

/* Begins an exact-base delta that reuses only the canonical References root
 * authenticated by base_ack. All target wrappers and the manifest are fresh. */
uint32_t flark_v3_host_begin_references_delta(
    FlarkV3HostHandle handle,
    const FlarkV3HostOfferBegin* begin,
    const FlarkV3HostStructuralAck* base_ack,
    FlarkV3HostCallReceipt* out_receipt);

/* Begins a typed exact-base e4/e5 delta authenticated by base_ack. Canonical
 * References are reused and persistent SourceFacts are replayed before the
 * host admits ordinary target nodes. */
uint32_t flark_v3_host_begin_exact_base_delta(
    FlarkV3HostHandle handle,
    const FlarkV3HostOfferBegin* begin,
    const FlarkV3HostStructuralAck* base_ack,
    FlarkV3HostCallReceipt* out_receipt);

/* packet points at exact FPK3 bytes and may be null only when packet_length is
 * zero. The packet and receipt ranges must be disjoint. */
uint32_t flark_v3_host_admit_packet(
    FlarkV3HostHandle handle,
    const uint8_t* packet,
    uint32_t packet_length,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_admit_inline_sidecar_packet(
    FlarkV3HostHandle handle,
    const uint8_t* packet,
    uint32_t packet_length,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_admit_viewport_presentation_packet(
    FlarkV3HostHandle handle,
    const uint8_t* packet,
    uint32_t packet_length,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_request_commit(
    FlarkV3HostHandle handle,
    const FlarkV3HostCommitRequest* request,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_request_inline_sidecar_commit(
    FlarkV3HostHandle handle,
    const FlarkV3HostInlineSidecarCommitRequest* request,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_request_viewport_presentation_commit(
    FlarkV3HostHandle handle,
    const FlarkV3HostViewportPresentationCommitRequest* request,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_abort_offer(
    FlarkV3HostHandle handle,
    const FlarkV3HostId128* offer_id,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_abort_inline_sidecar_offer(
    FlarkV3HostHandle handle,
    const FlarkV3HostId128* offer_id,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_abort_viewport_presentation_offer(
    FlarkV3HostHandle handle,
    const FlarkV3HostId128* offer_id,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_poll(
    FlarkV3HostHandle handle,
    FlarkV3HostWorkGrant grant,
    FlarkV3HostPollReceipt* out_receipt);

uint32_t flark_v3_host_poll_inline_sidecar(
    FlarkV3HostHandle handle,
    FlarkV3HostWorkGrant grant,
    FlarkV3HostInlineSidecarPollReceipt* out_receipt);

uint32_t flark_v3_host_poll_viewport_presentation(
    FlarkV3HostHandle handle,
    FlarkV3HostWorkGrant grant,
    FlarkV3HostViewportPresentationPollReceipt* out_receipt);

uint32_t flark_v3_host_acknowledge_delivery(
    FlarkV3HostHandle handle,
    const FlarkV3HostStructuralAck* ack,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_acknowledge_inline_sidecar_delivery(
    FlarkV3HostHandle handle,
    const FlarkV3HostInlineSidecarAck* ack,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_acknowledge_viewport_presentation_delivery(
    FlarkV3HostHandle handle,
    const FlarkV3HostViewportPresentationAck* ack,
    FlarkV3HostCallReceipt* out_receipt);

/* Authors FLKVP001 bytes directly from the installed host root. The request
 * and receipt remain stable when the current fixed M1.1 lookup is replaced by
 * a multi-record point index. output may be null only when capacity is zero;
 * query, output, and receipt ranges must be pairwise disjoint. */
uint32_t flark_v3_host_query_structural(
    FlarkV3HostHandle handle,
    const FlarkV3HostPointQuery* query,
    uint8_t* output,
    uint32_t capacity,
    FlarkV3HostPointQueryReceipt* out_receipt);

uint32_t flark_v3_host_query_structural_range(
    FlarkV3HostHandle handle,
    const FlarkV3HostBlockRangeQuery* query,
    uint8_t* output,
    uint32_t capacity,
    FlarkV3HostBlockRangeQueryReceipt* out_receipt);

uint32_t flark_v3_host_query_structural_ordinal_window(
    FlarkV3HostHandle handle,
    const FlarkV3HostStructuralOrdinalWindowQuery* query,
    FlarkV3HostStructuralOrdinalWindowQueryReceipt* out_receipt);

uint32_t flark_v3_host_query_inline_sidecar(
    FlarkV3HostHandle handle,
    const FlarkV3HostInlineSidecarQuery* query,
    uint8_t* output,
    uint32_t capacity,
    FlarkV3HostInlineSidecarQueryReceipt* out_receipt);

/* Copies one exact installed FLKVP001 aggregate page into caller-owned output.
 * query, output, and receipt ranges must be pairwise disjoint. A too-small
 * output returns QUERY_BOUND_EXCEEDED with zero encoded_bytes/entry_count. */
uint32_t flark_v3_host_query_viewport_presentation(
    FlarkV3HostHandle handle,
    const FlarkV3HostViewportPresentationQuery* query,
    uint8_t* output,
    uint32_t capacity,
    FlarkV3HostViewportPresentationQueryReceipt* out_receipt);

/* Begins bounded retirement. Continue flark_v3_host_poll until CLOSED. Then
 * detach the finalizer, prove host_remove (or emergency_destroy) succeeded,
 * and only then release its token; reattach on reclamation failure. */
uint32_t flark_v3_host_close(
    FlarkV3HostHandle handle,
    FlarkV3HostCallReceipt* out_receipt);

uint32_t flark_v3_host_remove(FlarkV3HostHandle handle);

uint32_t flark_v3_host_emergency_destroy(FlarkV3HostHandle handle);

uint32_t flark_v3_host_finalizer_token_create(
    FlarkV3HostHandle handle,
    void** out_token);

uint32_t flark_v3_host_finalizer_token_release(void* token);

void flark_v3_host_emergency_finalize(void* token);

#ifdef __cplusplus
}  // extern "C"
#endif

#endif  // FLARK_COMRAK_BRIDGE_H_

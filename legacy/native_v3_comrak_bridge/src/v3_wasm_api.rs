//! JavaScript-safe Wasm ABI for the persistent v3 endpoint and host.
//!
//! Rust's C ABI is allowed to lower small structs by value differently across
//! targets. JavaScript callers must therefore use only these wrappers: every
//! argument and result is one `u32`, pointers are linear-memory byte offsets,
//! and opaque handles travel as separate slot/generation words. The wrappers
//! deliberately delegate the native endpoint and host entry points so status
//! mapping, receipt initialization, bounds, and registry ownership stay
//! identical on native and Web.

use std::alloc::{alloc, dealloc, Layout};
use std::mem::offset_of;

use crate::v3_host_native_api::{
    flark_v3_host_abort_inline_sidecar_offer, flark_v3_host_abort_offer,
    flark_v3_host_abort_viewport_presentation_offer, flark_v3_host_acknowledge_delivery,
    flark_v3_host_acknowledge_inline_sidecar_delivery,
    flark_v3_host_acknowledge_viewport_presentation_delivery,
    flark_v3_host_admit_inline_sidecar_packet, flark_v3_host_admit_packet,
    flark_v3_host_admit_viewport_presentation_packet, flark_v3_host_begin_exact_base_delta,
    flark_v3_host_begin_inline_sidecar_offer, flark_v3_host_begin_offer,
    flark_v3_host_begin_references_delta, flark_v3_host_begin_viewport_presentation_offer,
    flark_v3_host_close, flark_v3_host_config_standard, flark_v3_host_create,
    flark_v3_host_emergency_destroy, flark_v3_host_native_abi_version,
    flark_v3_host_observe_source, flark_v3_host_poll, flark_v3_host_poll_inline_sidecar,
    flark_v3_host_poll_viewport_presentation, flark_v3_host_query_inline_sidecar,
    flark_v3_host_query_structural, flark_v3_host_query_structural_ordinal_window,
    flark_v3_host_query_structural_range, flark_v3_host_query_viewport_presentation,
    flark_v3_host_remove, flark_v3_host_request_commit,
    flark_v3_host_request_inline_sidecar_commit,
    flark_v3_host_request_viewport_presentation_commit, FlarkV3HostBlockRangeQuery,
    FlarkV3HostBlockRangeQueryReceipt, FlarkV3HostCallReceipt, FlarkV3HostCommitRequest,
    FlarkV3HostConfig, FlarkV3HostHandle, FlarkV3HostId128, FlarkV3HostInlineSidecarAck,
    FlarkV3HostInlineSidecarBegin, FlarkV3HostInlineSidecarBinding,
    FlarkV3HostInlineSidecarCommitRequest, FlarkV3HostInlineSidecarDisposition,
    FlarkV3HostInlineSidecarPollReceipt, FlarkV3HostInlineSidecarQuery,
    FlarkV3HostInlineSidecarQueryReceipt, FlarkV3HostOfferBegin, FlarkV3HostPointQuery,
    FlarkV3HostPointQueryReceipt, FlarkV3HostPollReceipt, FlarkV3HostSourceVersion,
    FlarkV3HostStructuralAck, FlarkV3HostStructuralOrdinalWindowQuery,
    FlarkV3HostStructuralOrdinalWindowQueryReceipt, FlarkV3HostU64,
    FlarkV3HostViewportPresentationAck, FlarkV3HostViewportPresentationBegin,
    FlarkV3HostViewportPresentationBinding, FlarkV3HostViewportPresentationCommitRequest,
    FlarkV3HostViewportPresentationEnvelope, FlarkV3HostViewportPresentationMetricRange,
    FlarkV3HostViewportPresentationOfferLimits, FlarkV3HostViewportPresentationPollReceipt,
    FlarkV3HostViewportPresentationQuery, FlarkV3HostViewportPresentationQueryLimits,
    FlarkV3HostViewportPresentationQueryReceipt, FlarkV3HostViewportPresentationVisitStart,
    FlarkV3HostWorkGrant,
};
use crate::v3_native_api::{
    flark_v3_checkpoint_b_probe, flark_v3_endpoint_close, flark_v3_endpoint_config_standard,
    flark_v3_endpoint_create, flark_v3_endpoint_dispatch, flark_v3_endpoint_dispatch_host_poll,
    flark_v3_endpoint_dispatch_inline_sidecar_host_poll,
    flark_v3_endpoint_dispatch_viewport_presentation_host_poll,
    flark_v3_endpoint_emergency_destroy, flark_v3_endpoint_encode,
    flark_v3_endpoint_native_abi_version, flark_v3_endpoint_poll, flark_v3_endpoint_poll_candidate,
    flark_v3_endpoint_recover, flark_v3_endpoint_remove, FlarkV3CandidatePollReceipt,
    FlarkV3DispatchReceipt, FlarkV3EndpointConfig, FlarkV3EndpointHandle, FlarkV3PollFuel,
    FlarkV3PollReceipt, FlarkV3SessionBinding,
};
use crate::v3_wire::Status;

/// Wasm-side allocations are suitable for every v3 ABI struct, including the
/// native host's temporary `u64` query count.
const WASM_ALLOCATION_ALIGNMENT: usize = 8;

#[inline]
fn const_pointer<T>(offset: u32) -> *const T {
    offset as usize as *const T
}

#[inline]
fn mut_pointer<T>(offset: u32) -> *mut T {
    offset as usize as *mut T
}

#[no_mangle]
/// # Safety
/// Output and written-count regions must satisfy the delegated diagnostic ABI.
pub unsafe extern "C" fn flark_v3_wasm_checkpoint_b_probe(
    output: u32,
    output_capacity: u32,
    out_written: u32,
) -> u32 {
    unsafe {
        flark_v3_checkpoint_b_probe(
            mut_pointer(output),
            output_capacity,
            mut_pointer(out_written),
        )
    }
}

#[inline]
const fn endpoint_handle(slot: u32, generation: u32) -> FlarkV3EndpointHandle {
    FlarkV3EndpointHandle { slot, generation }
}

#[inline]
const fn host_handle(slot: u32, generation: u32) -> FlarkV3HostHandle {
    FlarkV3HostHandle { slot, generation }
}

#[inline]
const fn status(status: Status) -> u32 {
    status.code() as u32
}

#[no_mangle]
/// Allocates one caller-owned linear-memory region aligned to eight bytes.
/// Zero means either a zero-length request or allocation failure.
pub extern "C" fn flark_v3_wasm_alloc(length: u32) -> u32 {
    if length == 0 {
        return 0;
    }
    let Ok(layout) = Layout::from_size_align(length as usize, WASM_ALLOCATION_ALIGNMENT) else {
        return 0;
    };
    // SAFETY: `layout` is non-zero and valid. Ownership is transferred to the
    // caller until one matching `flark_v3_wasm_free`.
    let pointer = unsafe { alloc(layout) };
    pointer as usize as u32
}

#[no_mangle]
/// Frees one region returned by `flark_v3_wasm_alloc`.
///
/// # Safety
/// `pointer` and `length` must be the exact still-live allocation pair. A null
/// pointer or zero length is accepted as a no-op.
pub unsafe extern "C" fn flark_v3_wasm_free(pointer: u32, length: u32) -> u32 {
    if pointer == 0 || length == 0 {
        return status(Status::Ok);
    }
    let Ok(layout) = Layout::from_size_align(length as usize, WASM_ALLOCATION_ALIGNMENT) else {
        return status(Status::Invalid);
    };
    // SAFETY: exact provenance, layout, and single release are the caller
    // contract above.
    unsafe { dealloc(mut_pointer(pointer), layout) };
    status(Status::Ok)
}

#[no_mangle]
pub extern "C" fn flark_v3_wasm_endpoint_native_abi_version() -> u32 {
    flark_v3_endpoint_native_abi_version()
}

#[no_mangle]
/// # Safety
/// `out_config` names writable linear memory for one endpoint config.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_config_standard(out_config: u32) -> u32 {
    unsafe { flark_v3_endpoint_config_standard(mut_pointer(out_config)) }
}

#[no_mangle]
/// # Safety
/// The input config and output handle regions must satisfy the delegated ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_create(config: u32, out_handle: u32) -> u32 {
    unsafe { flark_v3_endpoint_create(const_pointer(config), mut_pointer(out_handle)) }
}

#[no_mangle]
/// # Safety
/// Config/binding inputs and the output handle must satisfy the delegated ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_recover(
    config: u32,
    previous_binding: u32,
    out_handle: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_recover(
            const_pointer(config),
            const_pointer(previous_binding),
            mut_pointer(out_handle),
        )
    }
}

#[no_mangle]
/// # Safety
/// Input frame and receipt regions must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_dispatch(
    slot: u32,
    generation: u32,
    input: u32,
    input_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_dispatch(
            endpoint_handle(slot, generation),
            const_pointer(input),
            input_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Input frame and receipt regions must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_dispatch_host_poll(
    slot: u32,
    generation: u32,
    input: u32,
    input_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_dispatch_host_poll(
            endpoint_handle(slot, generation),
            const_pointer(input),
            input_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Input frame and receipt regions must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_dispatch_inline_sidecar_host_poll(
    slot: u32,
    generation: u32,
    input: u32,
    input_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_dispatch_inline_sidecar_host_poll(
            endpoint_handle(slot, generation),
            const_pointer(input),
            input_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Input frame and receipt regions must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_dispatch_viewport_presentation_host_poll(
    slot: u32,
    generation: u32,
    input: u32,
    input_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_dispatch_viewport_presentation_host_poll(
            endpoint_handle(slot, generation),
            const_pointer(input),
            input_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// The receipt region must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_poll(
    slot: u32,
    generation: u32,
    maximum_source_bytes: u32,
    maximum_checkpoints: u32,
    maximum_retirement_transitions: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_poll(
            endpoint_handle(slot, generation),
            FlarkV3PollFuel {
                maximum_source_bytes,
                maximum_checkpoints,
                maximum_retirement_transitions,
            },
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// The receipt region must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_poll_candidate(
    slot: u32,
    generation: u32,
    maximum_transitions: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_poll_candidate(
            endpoint_handle(slot, generation),
            maximum_transitions,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Output and written-count regions must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_encode(
    slot: u32,
    generation: u32,
    output: u32,
    output_capacity: u32,
    out_written: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_encode(
            endpoint_handle(slot, generation),
            mut_pointer(output),
            output_capacity,
            mut_pointer(out_written),
        )
    }
}

#[no_mangle]
/// # Safety
/// Input frame and receipt regions must satisfy the delegated endpoint ABI.
pub unsafe extern "C" fn flark_v3_wasm_endpoint_close(
    slot: u32,
    generation: u32,
    input: u32,
    input_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_endpoint_close(
            endpoint_handle(slot, generation),
            const_pointer(input),
            input_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
pub extern "C" fn flark_v3_wasm_endpoint_remove(slot: u32, generation: u32) -> u32 {
    flark_v3_endpoint_remove(endpoint_handle(slot, generation))
}

#[no_mangle]
pub extern "C" fn flark_v3_wasm_endpoint_emergency_destroy(slot: u32, generation: u32) -> u32 {
    flark_v3_endpoint_emergency_destroy(endpoint_handle(slot, generation))
}

#[no_mangle]
pub extern "C" fn flark_v3_wasm_host_native_abi_version() -> u32 {
    flark_v3_host_native_abi_version()
}

#[no_mangle]
/// # Safety
/// `out_config` names writable linear memory for one host config.
pub unsafe extern "C" fn flark_v3_wasm_host_config_standard(out_config: u32) -> u32 {
    unsafe { flark_v3_host_config_standard(mut_pointer(out_config)) }
}

#[no_mangle]
/// # Safety
/// The input config and output handle regions must satisfy the delegated ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_create(config: u32, out_handle: u32) -> u32 {
    unsafe { flark_v3_host_create(const_pointer(config), mut_pointer(out_handle)) }
}

#[no_mangle]
/// # Safety
/// Source and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_observe_source(
    slot: u32,
    generation: u32,
    source: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_observe_source(
            host_handle(slot, generation),
            const_pointer(source),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Offer-begin and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_begin_offer(
    slot: u32,
    generation: u32,
    begin: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_begin_offer(
            host_handle(slot, generation),
            const_pointer(begin),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Sidecar offer-begin and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_begin_inline_sidecar_offer(
    slot: u32,
    generation: u32,
    begin: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_begin_inline_sidecar_offer(
            host_handle(slot, generation),
            const_pointer(begin),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// VPB1 offer-begin and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_begin_viewport_presentation_offer(
    slot: u32,
    generation: u32,
    begin: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_begin_viewport_presentation_offer(
            host_handle(slot, generation),
            const_pointer(begin),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Offer-begin, exact base ACK, and receipt regions must satisfy the delegated
/// host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_begin_references_delta(
    slot: u32,
    generation: u32,
    begin: u32,
    base_ack: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_begin_references_delta(
            host_handle(slot, generation),
            const_pointer(begin),
            const_pointer(base_ack),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Offer-begin, exact base ACK, and receipt regions must satisfy the delegated
/// host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_begin_exact_base_delta(
    slot: u32,
    generation: u32,
    begin: u32,
    base_ack: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_begin_exact_base_delta(
            host_handle(slot, generation),
            const_pointer(begin),
            const_pointer(base_ack),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Exact FPK3 packet bytes and the receipt region must satisfy the delegated
/// host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_admit_packet(
    slot: u32,
    generation: u32,
    packet: u32,
    packet_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_admit_packet(
            host_handle(slot, generation),
            const_pointer(packet),
            packet_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Exact FPK3 sidecar packet bytes and the receipt region must satisfy the
/// delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_admit_inline_sidecar_packet(
    slot: u32,
    generation: u32,
    packet: u32,
    packet_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_admit_inline_sidecar_packet(
            host_handle(slot, generation),
            const_pointer(packet),
            packet_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Exact FPK3 VPB1 packet bytes and the receipt region must satisfy the
/// delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_admit_viewport_presentation_packet(
    slot: u32,
    generation: u32,
    packet: u32,
    packet_length: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_admit_viewport_presentation_packet(
            host_handle(slot, generation),
            const_pointer(packet),
            packet_length,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Commit request and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_request_commit(
    slot: u32,
    generation: u32,
    request: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_request_commit(
            host_handle(slot, generation),
            const_pointer(request),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Sidecar commit request and receipt regions must satisfy the delegated host
/// ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_request_inline_sidecar_commit(
    slot: u32,
    generation: u32,
    request: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_request_inline_sidecar_commit(
            host_handle(slot, generation),
            const_pointer(request),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// VPB1 commit request and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_request_viewport_presentation_commit(
    slot: u32,
    generation: u32,
    request: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_request_viewport_presentation_commit(
            host_handle(slot, generation),
            const_pointer(request),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Offer-id and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_abort_offer(
    slot: u32,
    generation: u32,
    offer_id: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_abort_offer(
            host_handle(slot, generation),
            const_pointer(offer_id),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Sidecar offer-id and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_abort_inline_sidecar_offer(
    slot: u32,
    generation: u32,
    offer_id: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_abort_inline_sidecar_offer(
            host_handle(slot, generation),
            const_pointer(offer_id),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// VPB1 offer-id and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_abort_viewport_presentation_offer(
    slot: u32,
    generation: u32,
    offer_id: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_abort_viewport_presentation_offer(
            host_handle(slot, generation),
            const_pointer(offer_id),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// The poll receipt region must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_poll(
    slot: u32,
    generation: u32,
    inspect_bytes: u32,
    copy_bytes: u32,
    transitions: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_poll(
            host_handle(slot, generation),
            FlarkV3HostWorkGrant {
                inspect_bytes,
                copy_bytes,
                transitions,
            },
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// The sidecar poll receipt region must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_poll_inline_sidecar(
    slot: u32,
    generation: u32,
    inspect_bytes: u32,
    copy_bytes: u32,
    transitions: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_poll_inline_sidecar(
            host_handle(slot, generation),
            FlarkV3HostWorkGrant {
                inspect_bytes,
                copy_bytes,
                transitions,
            },
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// The VPB1 poll receipt region must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_poll_viewport_presentation(
    slot: u32,
    generation: u32,
    inspect_bytes: u32,
    copy_bytes: u32,
    transitions: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_poll_viewport_presentation(
            host_handle(slot, generation),
            FlarkV3HostWorkGrant {
                inspect_bytes,
                copy_bytes,
                transitions,
            },
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Acknowledgement and receipt regions must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_acknowledge_delivery(
    slot: u32,
    generation: u32,
    acknowledgement: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_acknowledge_delivery(
            host_handle(slot, generation),
            const_pointer(acknowledgement),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Sidecar acknowledgement and receipt regions must satisfy the delegated host
/// ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_acknowledge_inline_sidecar_delivery(
    slot: u32,
    generation: u32,
    acknowledgement: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_acknowledge_inline_sidecar_delivery(
            host_handle(slot, generation),
            const_pointer(acknowledgement),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// VPB1 acknowledgement and receipt regions must satisfy the delegated host
/// ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_acknowledge_viewport_presentation_delivery(
    slot: u32,
    generation: u32,
    acknowledgement: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_acknowledge_viewport_presentation_delivery(
            host_handle(slot, generation),
            const_pointer(acknowledgement),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Query, output, and receipt regions must satisfy the delegated host ABI.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn flark_v3_wasm_host_query_structural(
    slot: u32,
    generation: u32,
    query: u32,
    output: u32,
    capacity: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_query_structural(
            host_handle(slot, generation),
            const_pointer(query),
            mut_pointer(output),
            capacity,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Range query, output, and receipt regions must satisfy the delegated host
/// ABI.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn flark_v3_wasm_host_query_structural_range(
    slot: u32,
    generation: u32,
    query: u32,
    output: u32,
    capacity: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_query_structural_range(
            host_handle(slot, generation),
            const_pointer(query),
            mut_pointer(output),
            capacity,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Ordinal-window query and receipt regions must satisfy the delegated host
/// ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_query_structural_ordinal_window(
    slot: u32,
    generation: u32,
    query: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_query_structural_ordinal_window(
            host_handle(slot, generation),
            const_pointer(query),
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// Sidecar query, output, and receipt regions must satisfy the delegated host
/// ABI.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn flark_v3_wasm_host_query_inline_sidecar(
    slot: u32,
    generation: u32,
    query: u32,
    output: u32,
    capacity: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_query_inline_sidecar(
            host_handle(slot, generation),
            const_pointer(query),
            mut_pointer(output),
            capacity,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// VPB1 query, output, and receipt regions must satisfy the delegated host ABI.
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn flark_v3_wasm_host_query_viewport_presentation(
    slot: u32,
    generation: u32,
    query: u32,
    output: u32,
    capacity: u32,
    out_receipt: u32,
) -> u32 {
    unsafe {
        flark_v3_host_query_viewport_presentation(
            host_handle(slot, generation),
            const_pointer(query),
            mut_pointer(output),
            capacity,
            mut_pointer(out_receipt),
        )
    }
}

#[no_mangle]
/// # Safety
/// The receipt region must satisfy the delegated host ABI.
pub unsafe extern "C" fn flark_v3_wasm_host_close(
    slot: u32,
    generation: u32,
    out_receipt: u32,
) -> u32 {
    unsafe { flark_v3_host_close(host_handle(slot, generation), mut_pointer(out_receipt)) }
}

#[no_mangle]
pub extern "C" fn flark_v3_wasm_host_remove(slot: u32, generation: u32) -> u32 {
    flark_v3_host_remove(host_handle(slot, generation))
}

#[no_mangle]
pub extern "C" fn flark_v3_wasm_host_emergency_destroy(slot: u32, generation: u32) -> u32 {
    flark_v3_host_emergency_destroy(host_handle(slot, generation))
}

// Compile-time guards: every JavaScript-visible parameter and return is one
// Wasm `i32`, represented here as `u32`. Any signature drift is intentional ABI
// work rather than an accidental Rust aggregate lowering change.
const _: extern "C" fn(u32) -> u32 = flark_v3_wasm_alloc;
const _: unsafe extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_free;
const _: unsafe extern "C" fn(u32, u32, u32) -> u32 = flark_v3_wasm_checkpoint_b_probe;
const _: extern "C" fn() -> u32 = flark_v3_wasm_endpoint_native_abi_version;
const _: unsafe extern "C" fn(u32) -> u32 = flark_v3_wasm_endpoint_config_standard;
const _: unsafe extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_endpoint_create;
const _: unsafe extern "C" fn(u32, u32, u32) -> u32 = flark_v3_wasm_endpoint_recover;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 = flark_v3_wasm_endpoint_dispatch;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_endpoint_dispatch_host_poll;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_endpoint_dispatch_inline_sidecar_host_poll;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_endpoint_dispatch_viewport_presentation_host_poll;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 = flark_v3_wasm_endpoint_poll;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = flark_v3_wasm_endpoint_poll_candidate;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 = flark_v3_wasm_endpoint_encode;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 = flark_v3_wasm_endpoint_close;
const _: extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_endpoint_remove;
const _: extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_endpoint_emergency_destroy;
const _: extern "C" fn() -> u32 = flark_v3_wasm_host_native_abi_version;
const _: unsafe extern "C" fn(u32) -> u32 = flark_v3_wasm_host_config_standard;
const _: unsafe extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_host_create;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_observe_source;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_begin_offer;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_begin_inline_sidecar_offer;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_begin_viewport_presentation_offer;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_begin_references_delta;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_begin_exact_base_delta;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_admit_packet;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_admit_inline_sidecar_packet;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_admit_viewport_presentation_packet;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_request_commit;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_request_inline_sidecar_commit;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_request_viewport_presentation_commit;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_abort_offer;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_abort_inline_sidecar_offer;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_abort_viewport_presentation_offer;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_poll;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_poll_inline_sidecar;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_poll_viewport_presentation;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 = flark_v3_wasm_host_acknowledge_delivery;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_acknowledge_inline_sidecar_delivery;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_acknowledge_viewport_presentation_delivery;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_query_structural;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_query_structural_range;
const _: unsafe extern "C" fn(u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_query_structural_ordinal_window;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_query_inline_sidecar;
const _: unsafe extern "C" fn(u32, u32, u32, u32, u32, u32) -> u32 =
    flark_v3_wasm_host_query_viewport_presentation;
const _: unsafe extern "C" fn(u32, u32, u32) -> u32 = flark_v3_wasm_host_close;
const _: extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_host_remove;
const _: extern "C" fn(u32, u32) -> u32 = flark_v3_wasm_host_emergency_destroy;

// The JS adapter allocates these fixed layouts directly in linear memory.
// Drift must fail compilation instead of silently changing a pointer contract.
const _: () = {
    assert!(std::mem::size_of::<FlarkV3EndpointHandle>() == 8);
    assert!(offset_of!(FlarkV3EndpointHandle, slot) == 0);
    assert!(offset_of!(FlarkV3EndpointHandle, generation) == 4);
    assert!(std::mem::size_of::<FlarkV3EndpointConfig>() == 64);
    assert!(offset_of!(FlarkV3EndpointConfig, abi_version) == 0);
    assert!(offset_of!(FlarkV3EndpointConfig, struct_size) == 4);
    assert!(offset_of!(FlarkV3EndpointConfig, max_retired_sources) == 8);
    assert!(offset_of!(FlarkV3EndpointConfig, max_retired_source_bytes) == 12);
    assert!(offset_of!(FlarkV3EndpointConfig, arena_max_slots) == 16);
    assert!(offset_of!(FlarkV3EndpointConfig, arena_max_live_payload_bytes) == 20);
    assert!(offset_of!(FlarkV3EndpointConfig, arena_max_children_per_node) == 24);
    assert!(offset_of!(FlarkV3EndpointConfig, checkpoint_spacing_utf16) == 28);
    assert!(offset_of!(FlarkV3EndpointConfig, source_facts_max_checkpoints) == 32);
    assert!(offset_of!(FlarkV3EndpointConfig, source_facts_max_pages) == 36);
    assert!(offset_of!(FlarkV3EndpointConfig, source_facts_max_resident_bytes) == 40);
    assert!(offset_of!(FlarkV3EndpointConfig, parser_profile) == 44);
    assert!(offset_of!(FlarkV3EndpointConfig, reserved) == 48);
    assert!(std::mem::size_of::<FlarkV3SessionBinding>() == 24);
    assert!(offset_of!(FlarkV3SessionBinding, document_session) == 0);
    assert!(offset_of!(FlarkV3SessionBinding, source_session_identity) == 16);
    assert!(offset_of!(FlarkV3SessionBinding, worker_generation) == 20);
    assert!(std::mem::size_of::<FlarkV3DispatchReceipt>() == 16);
    assert!(offset_of!(FlarkV3DispatchReceipt, correlation_id) == 0);
    assert!(offset_of!(FlarkV3DispatchReceipt, action) == 4);
    assert!(offset_of!(FlarkV3DispatchReceipt, outstanding_event_id) == 8);
    assert!(offset_of!(FlarkV3DispatchReceipt, outstanding_event_kind) == 12);
    assert!(std::mem::size_of::<FlarkV3PollReceipt>() == 60);
    assert!(offset_of!(FlarkV3PollReceipt, source_bytes_examined) == 0);
    assert!(offset_of!(FlarkV3PollReceipt, source_bytes_buffered) == 4);
    assert!(offset_of!(FlarkV3PollReceipt, cursor_refills) == 8);
    assert!(offset_of!(FlarkV3PollReceipt, cursor_copy_bytes_upper_bound) == 12);
    assert!(offset_of!(FlarkV3PollReceipt, checkpoints_emitted) == 16);
    assert!(offset_of!(FlarkV3PollReceipt, source_fact_transitions) == 20);
    assert!(offset_of!(FlarkV3PollReceipt, released_source_leases) == 24);
    assert!(offset_of!(FlarkV3PollReceipt, released_source_bytes) == 28);
    assert!(offset_of!(FlarkV3PollReceipt, arena_transitions) == 32);
    assert!(offset_of!(FlarkV3PollReceipt, arena_nodes_reclaimed) == 36);
    assert!(offset_of!(FlarkV3PollReceipt, scan_complete) == 40);
    assert!(offset_of!(FlarkV3PollReceipt, certification) == 44);
    assert!(offset_of!(FlarkV3PollReceipt, cleanup_complete) == 48);
    assert!(offset_of!(FlarkV3PollReceipt, outstanding_event_id) == 52);
    assert!(offset_of!(FlarkV3PollReceipt, outstanding_event_kind) == 56);
    assert!(std::mem::size_of::<FlarkV3CandidatePollReceipt>() == 16);
    assert!(offset_of!(FlarkV3CandidatePollReceipt, transitions) == 0);
    assert!(offset_of!(FlarkV3CandidatePollReceipt, cleanup_complete) == 4);
    assert!(offset_of!(FlarkV3CandidatePollReceipt, outstanding_event_id) == 8);
    assert!(offset_of!(FlarkV3CandidatePollReceipt, outstanding_event_kind) == 12);
    assert!(std::mem::size_of::<FlarkV3HostHandle>() == 8);
    assert!(offset_of!(FlarkV3HostHandle, slot) == 0);
    assert!(offset_of!(FlarkV3HostHandle, generation) == 4);
    assert!(std::mem::size_of::<FlarkV3HostConfig>() == 56);
    assert!(offset_of!(FlarkV3HostConfig, abi_version) == 0);
    assert!(offset_of!(FlarkV3HostConfig, struct_size) == 4);
    assert!(offset_of!(FlarkV3HostConfig, document_session) == 8);
    assert!(offset_of!(FlarkV3HostConfig, grammar_revision) == 24);
    assert!(offset_of!(FlarkV3HostConfig, syntax_profile) == 28);
    assert!(offset_of!(FlarkV3HostConfig, authority_mask) == 32);
    assert!(offset_of!(FlarkV3HostConfig, maximum_query_bytes) == 36);
    assert!(offset_of!(FlarkV3HostConfig, reserved) == 40);
    assert!(std::mem::size_of::<FlarkV3HostSourceVersion>() == 44);
    assert!(offset_of!(FlarkV3HostSourceVersion, document_session) == 0);
    assert!(offset_of!(FlarkV3HostSourceVersion, revision) == 16);
    assert!(offset_of!(FlarkV3HostSourceVersion, utf8_length) == 20);
    assert!(offset_of!(FlarkV3HostSourceVersion, utf16_length) == 24);
    assert!(offset_of!(FlarkV3HostSourceVersion, content_hash128) == 28);
    assert!(std::mem::size_of::<FlarkV3HostOfferBegin>() == 144);
    assert!(offset_of!(FlarkV3HostOfferBegin, offer_id) == 0);
    assert!(offset_of!(FlarkV3HostOfferBegin, publication_session) == 16);
    assert!(offset_of!(FlarkV3HostOfferBegin, target_host_revision) == 32);
    assert!(offset_of!(FlarkV3HostOfferBegin, source_version) == 36);
    assert!(offset_of!(FlarkV3HostOfferBegin, source_root) == 80);
    assert!(offset_of!(FlarkV3HostOfferBegin, parse_generation) == 88);
    assert!(offset_of!(FlarkV3HostOfferBegin, grammar_revision) == 92);
    assert!(offset_of!(FlarkV3HostOfferBegin, syntax_profile) == 96);
    assert!(offset_of!(FlarkV3HostOfferBegin, authority_mask) == 100);
    assert!(offset_of!(FlarkV3HostOfferBegin, transferred_record_count) == 104);
    assert!(offset_of!(FlarkV3HostOfferBegin, target_record_count) == 108);
    assert!(offset_of!(FlarkV3HostOfferBegin, maximum_frame_count) == 112);
    assert!(offset_of!(FlarkV3HostOfferBegin, maximum_encoded_frame_bytes) == 116);
    assert!(offset_of!(FlarkV3HostOfferBegin, maximum_packet_bytes) == 120);
    assert!(offset_of!(FlarkV3HostOfferBegin, maximum_frame_bytes) == 124);
    assert!(offset_of!(FlarkV3HostOfferBegin, maximum_program_children) == 128);
    assert!(offset_of!(FlarkV3HostOfferBegin, reserved) == 132);
    assert!(std::mem::size_of::<FlarkV3HostCommitRequest>() == 56);
    assert!(offset_of!(FlarkV3HostCommitRequest, offer_id) == 0);
    assert!(offset_of!(FlarkV3HostCommitRequest, actual_frame_count) == 16);
    assert!(offset_of!(FlarkV3HostCommitRequest, actual_encoded_frame_bytes) == 20);
    assert!(offset_of!(FlarkV3HostCommitRequest, rolling_transport_digest) == 24);
    assert!(offset_of!(FlarkV3HostCommitRequest, canonical_stream_digest) == 40);
    assert!(std::mem::size_of::<FlarkV3HostId128>() == 16);
    assert!(offset_of!(FlarkV3HostId128, words) == 0);
    assert!(std::mem::size_of::<FlarkV3HostStructuralAck>() == 124);
    assert!(offset_of!(FlarkV3HostStructuralAck, publication_session) == 0);
    assert!(offset_of!(FlarkV3HostStructuralAck, host_revision) == 16);
    assert!(offset_of!(FlarkV3HostStructuralAck, source_version) == 20);
    assert!(offset_of!(FlarkV3HostStructuralAck, source_root) == 64);
    assert!(offset_of!(FlarkV3HostStructuralAck, parse_generation) == 72);
    assert!(offset_of!(FlarkV3HostStructuralAck, grammar_revision) == 76);
    assert!(offset_of!(FlarkV3HostStructuralAck, syntax_profile) == 80);
    assert!(offset_of!(FlarkV3HostStructuralAck, authority_mask) == 84);
    assert!(offset_of!(FlarkV3HostStructuralAck, record_count) == 88);
    assert!(offset_of!(FlarkV3HostStructuralAck, sequence_digest) == 92);
    assert!(offset_of!(FlarkV3HostStructuralAck, manifest_digest) == 108);
    assert!(std::mem::size_of::<FlarkV3HostU64>() == 8);
    assert!(offset_of!(FlarkV3HostU64, low_word) == 0);
    assert!(offset_of!(FlarkV3HostU64, high_word) == 4);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarBinding>() == 56);
    assert!(offset_of!(FlarkV3HostInlineSidecarBinding, parser_profile) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarBinding, refinement_generation) == 8);
    assert!(offset_of!(FlarkV3HostInlineSidecarBinding, block_ordinal) == 16);
    assert!(offset_of!(FlarkV3HostInlineSidecarBinding, physical_start_utf8) == 24);
    assert!(offset_of!(FlarkV3HostInlineSidecarBinding, visible_start_utf16) == 48);
    assert!(offset_of!(FlarkV3HostInlineSidecarBinding, visible_end_utf16) == 52);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarDisposition>() == 80);
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, disposition) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, reason) == 4);
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, logical_page_count) == 8);
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, fact_count) == 16);
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, storage_page_count) == 24);
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, link_value_entry_count) == 32);
    assert!(
        offset_of!(
            FlarkV3HostInlineSidecarDisposition,
            link_value_encoded_bytes
        ) == 36
    );
    assert!(
        offset_of!(
            FlarkV3HostInlineSidecarDisposition,
            link_value_storage_page_count
        ) == 40
    );
    assert!(offset_of!(FlarkV3HostInlineSidecarDisposition, commitment256) == 48);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarBegin>() == 364);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, schema) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, offer_id) == 8);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, base_ack) == 40);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, binding) == 164);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, hio1_encoded_bytes) == 220);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, sidecar_disposition) == 232);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, hio1_envelope_digest256) == 312);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, maximum_frame_count) == 344);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarCommitRequest>() == 56);
    assert!(offset_of!(FlarkV3HostInlineSidecarCommitRequest, offer_id) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarCommitRequest, root_stream_digest) == 40);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarAck>() == 212);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, publication_session) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, base_ack) == 16);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, refinement_generation) == 140);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, block_ordinal) == 148);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, disposition) == 160);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, hio1_envelope_digest256) == 164);
    assert!(offset_of!(FlarkV3HostInlineSidecarAck, root_stream_digest) == 196);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarPollReceipt>() == 240);
    assert!(offset_of!(FlarkV3HostInlineSidecarPollReceipt, rejection_reason) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarPollReceipt, outcome) == 4);
    assert!(offset_of!(FlarkV3HostInlineSidecarPollReceipt, offer_id) == 8);
    assert!(offset_of!(FlarkV3HostInlineSidecarPollReceipt, next_frame_ordinal) == 24);
    assert!(offset_of!(FlarkV3HostInlineSidecarPollReceipt, ack) == 28);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarQuery>() == 80);
    assert!(offset_of!(FlarkV3HostInlineSidecarQuery, binding) == 8);
    assert!(offset_of!(FlarkV3HostInlineSidecarQuery, maximum_encoded_bytes) == 64);
    assert!(offset_of!(FlarkV3HostInlineSidecarQuery, reserved) == 68);
    assert!(std::mem::size_of::<FlarkV3HostInlineSidecarQueryReceipt>() == 36);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, rejection_reason) == 0);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, outcome) == 4);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, reason) == 8);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, encoded_bytes) == 12);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, fact_count) == 16);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, tree_nodes_visited) == 20);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, value_entry_count) == 24);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, value_encoded_bytes) == 28);
    assert!(offset_of!(FlarkV3HostInlineSidecarQueryReceipt, payload_kind) == 32);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationMetricRange>() == 16);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationVisitStart>() == 16);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationBinding>() == 72);
    assert!(offset_of!(FlarkV3HostViewportPresentationBinding, complete) == 68);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationEnvelope>() == 60);
    assert!(
        offset_of!(
            FlarkV3HostViewportPresentationEnvelope,
            aggregate_envelope_digest256
        ) == 28
    );
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationQueryLimits>() == 32);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationOfferLimits>() == 20);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationBegin>() == 348);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, base_ack) == 40);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, binding) == 164);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, envelope) == 236);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, query_limits) == 296);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, limits) == 328);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationCommitRequest>() == 56);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationAck>() == 296);
    assert!(offset_of!(FlarkV3HostViewportPresentationAck, base_ack) == 16);
    assert!(offset_of!(FlarkV3HostViewportPresentationAck, binding) == 140);
    assert!(offset_of!(FlarkV3HostViewportPresentationAck, envelope) == 212);
    assert!(
        offset_of!(
            FlarkV3HostViewportPresentationAck,
            aggregate_root_stream_digest
        ) == 280
    );
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationPollReceipt>() == 324);
    assert!(offset_of!(FlarkV3HostViewportPresentationPollReceipt, ack) == 28);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationQuery>() == 320);
    assert!(offset_of!(FlarkV3HostViewportPresentationQuery, ack) == 8);
    assert!(offset_of!(FlarkV3HostViewportPresentationQuery, maximum_encoded_bytes) == 304);
    assert!(offset_of!(FlarkV3HostViewportPresentationQuery, reserved) == 308);
    assert!(std::mem::size_of::<FlarkV3HostViewportPresentationQueryReceipt>() == 32);
    assert!(offset_of!(FlarkV3HostViewportPresentationQueryReceipt, reserved) == 16);
    assert!(std::mem::size_of::<FlarkV3HostCallReceipt>() == 4);
    assert!(offset_of!(FlarkV3HostCallReceipt, rejection_reason) == 0);
    assert!(std::mem::size_of::<FlarkV3HostPollReceipt>() == 152);
    assert!(offset_of!(FlarkV3HostPollReceipt, rejection_reason) == 0);
    assert!(offset_of!(FlarkV3HostPollReceipt, outcome) == 4);
    assert!(offset_of!(FlarkV3HostPollReceipt, offer_id) == 8);
    assert!(offset_of!(FlarkV3HostPollReceipt, next_frame_ordinal) == 24);
    assert!(offset_of!(FlarkV3HostPollReceipt, ack) == 28);
    assert!(std::mem::size_of::<FlarkV3HostPointQuery>() == 96);
    assert!(offset_of!(FlarkV3HostPointQuery, schema) == 0);
    assert!(offset_of!(FlarkV3HostPointQuery, struct_size) == 4);
    assert!(offset_of!(FlarkV3HostPointQuery, source_version) == 8);
    assert!(offset_of!(FlarkV3HostPointQuery, position_utf8) == 52);
    assert!(offset_of!(FlarkV3HostPointQuery, position_utf16) == 56);
    assert!(offset_of!(FlarkV3HostPointQuery, affinity) == 60);
    assert!(offset_of!(FlarkV3HostPointQuery, maximum_encoded_bytes) == 64);
    assert!(offset_of!(FlarkV3HostPointQuery, maximum_open_depth) == 68);
    assert!(offset_of!(FlarkV3HostPointQuery, maximum_leaf_count) == 72);
    assert!(offset_of!(FlarkV3HostPointQuery, maximum_tree_nodes_visited) == 76);
    assert!(offset_of!(FlarkV3HostPointQuery, reserved) == 80);
    assert!(std::mem::size_of::<FlarkV3HostPointQueryReceipt>() == 112);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, rejection_reason) == 0);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, outcome) == 4);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, gap_reason) == 8);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, encoded_bytes) == 12);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, leaf_count) == 16);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, open_depth) == 20);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, tree_nodes_visited) == 24);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, summary_nodes_skipped) == 28);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, range_start_utf8) == 32);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, range_start_utf16) == 36);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, range_end_utf8) == 40);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, range_end_utf16) == 44);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, source_version) == 48);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, reserved) == 92);
    assert!(std::mem::size_of::<FlarkV3HostBlockRangeQuery>() == 172);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, source_version) == 8);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, requested_start_utf8) == 52);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, continuation) == 92);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, reserved) == 156);
    assert!(std::mem::size_of::<FlarkV3HostBlockRangeQueryReceipt>() == 188);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, source_version) == 60);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, continuation) == 108);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, reserved) == 172);
    assert!(std::mem::size_of::<FlarkV3HostStructuralOrdinalWindowQuery>() == 92);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, source_version) == 8);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, start_entry_ordinal) == 52);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, maximum_entries) == 60);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, reserved) == 76);
    assert!(std::mem::size_of::<FlarkV3HostStructuralOrdinalWindowQueryReceipt>() == 132);
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            total_entry_count
        ) == 16
    );
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            start_entry_ordinal
        ) == 24
    );
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            next_entry_ordinal
        ) == 32
    );
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQueryReceipt, start_utf8) == 40);
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            storage_pages_visited
        ) == 56
    );
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            source_version
        ) == 72
    );
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQueryReceipt, reserved) == 116);
};

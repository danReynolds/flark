//! Caller-owned native C ABI for the generation-checked Flark v3 endpoint.
//!
//! Every exported call catches panics, resolves an opaque slot/generation pair,
//! and operates under one endpoint mutex. No Endpoint pointer crosses the
//! boundary. The sole persistent native pointer is a NativeFinalizer token
//! containing only the generation-checked handle. Normal removal is possible
//! only after the credited close protocol reaches `Removable`; emergency
//! destruction is a separate, explicitly abnormal symbol.

use std::ffi::c_void;
use std::mem::{align_of, offset_of, size_of};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use flark_engine::{
    ArenaLimits, DocumentRuntimeConfig, ParserProfileId, SourceFactsRootLimits,
    SourceFactsScanProfile, SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX,
    SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX,
};

use crate::v3_endpoint::{
    standard_document_runtime_config, CertificationStatus, EndpointCandidatePollReceipt,
    EndpointCommandAction, EndpointCommandReceipt, EndpointConfig, EndpointError,
    EndpointEventKind, EndpointHostPollAction, EndpointHostPollReceipt, EndpointPollFuel,
    EndpointPollReceipt, MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS,
};
use crate::v3_publication_wire::EncodeError as PublicationEncodeError;
use crate::v3_registry::{EndpointHandle, EndpointRegistry, RegistryError};
use crate::v3_session_wire::{
    decode_command, Command, DecodeFailure as SessionDecodeFailure,
    EncodeError as SessionEncodeError, SessionBinding,
};
use crate::v3_wire::{
    DecodeFailure as EnvelopeDecodeFailure, EncodeError as EnvelopeEncodeError, Status,
    HEADER_BYTES, MAXIMUM_PAYLOAD_BYTES,
};

pub const FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION: u32 = 0x0002_0002;
pub const FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES: u32 =
    (HEADER_BYTES + MAXIMUM_PAYLOAD_BYTES) as u32;
pub const FLARK_V3_ENDPOINT_MAXIMUM_POLL_SOURCE_BYTES: u32 =
    SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX as u32;
pub const FLARK_V3_ENDPOINT_MAXIMUM_POLL_CHECKPOINTS: u32 =
    SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u32;
pub const FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES: u32 = 64 * 1024;

const STANDARD_CHECKPOINT_SPACING_UTF16: u32 = 4 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3EndpointHandle {
    pub slot: u32,
    pub generation: u32,
}

impl From<FlarkV3EndpointHandle> for EndpointHandle {
    fn from(handle: FlarkV3EndpointHandle) -> Self {
        Self::from_parts(handle.slot, handle.generation)
    }
}

impl From<EndpointHandle> for FlarkV3EndpointHandle {
    fn from(handle: EndpointHandle) -> Self {
        Self {
            slot: handle.slot(),
            generation: handle.generation(),
        }
    }
}

/// Versioned, fixed-width admission policy for one native endpoint.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3EndpointConfig {
    pub abi_version: u32,
    pub struct_size: u32,
    pub max_retired_sources: u32,
    pub max_retired_source_bytes: u32,
    pub arena_max_slots: u32,
    pub arena_max_live_payload_bytes: u32,
    pub arena_max_children_per_node: u32,
    pub checkpoint_spacing_utf16: u32,
    pub source_facts_max_checkpoints: u32,
    pub source_facts_max_pages: u32,
    pub source_facts_max_resident_bytes: u32,
    pub parser_profile: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3SessionBinding {
    pub document_session: [u32; 4],
    pub source_session_identity: u32,
    pub worker_generation: u32,
}

impl From<FlarkV3SessionBinding> for SessionBinding {
    fn from(binding: FlarkV3SessionBinding) -> Self {
        Self {
            document_session: binding.document_session,
            source_session_identity: binding.source_session_identity,
            worker_generation: binding.worker_generation,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3DispatchReceipt {
    pub correlation_id: u32,
    pub action: u32,
    /// Zero when the call leaves no globally credited event outstanding.
    pub outstanding_event_id: u32,
    /// Zero when `outstanding_event_id` is zero.
    pub outstanding_event_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3PollFuel {
    pub maximum_source_bytes: u32,
    pub maximum_checkpoints: u32,
    pub maximum_retirement_transitions: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3PollReceipt {
    pub source_bytes_examined: u32,
    pub source_bytes_buffered: u32,
    pub cursor_refills: u32,
    pub cursor_copy_bytes_upper_bound: u32,
    pub checkpoints_emitted: u32,
    pub source_fact_transitions: u32,
    pub released_source_leases: u32,
    pub released_source_bytes: u32,
    pub arena_transitions: u32,
    pub arena_nodes_reclaimed: u32,
    pub scan_complete: u32,
    /// Zero means no certification transition in this poll.
    pub certification: u32,
    pub cleanup_complete: u32,
    /// Zero when the call leaves no globally credited event outstanding.
    pub outstanding_event_id: u32,
    /// Zero when `outstanding_event_id` is zero.
    pub outstanding_event_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3CandidatePollReceipt {
    pub transitions: u32,
    pub cleanup_complete: u32,
    pub outstanding_event_id: u32,
    pub outstanding_event_kind: u32,
}

static ENDPOINT_REGISTRY: OnceLock<EndpointRegistry> = OnceLock::new();

struct FinalizerToken {
    handle: EndpointHandle,
}

fn registry() -> &'static EndpointRegistry {
    ENDPOINT_REGISTRY.get_or_init(EndpointRegistry::production)
}

#[derive(Debug)]
enum NativeError {
    InvalidArgument,
    InvalidConfig,
    BoundExceeded,
    NumericOverflow,
    CheckpointB,
    Registry(RegistryError),
    Endpoint(EndpointError),
}

impl From<RegistryError> for NativeError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<EndpointError> for NativeError {
    fn from(error: EndpointError) -> Self {
        Self::Endpoint(error)
    }
}

type NativeResult<T = ()> = Result<T, NativeError>;

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
pub extern "C" fn flark_v3_endpoint_native_abi_version() -> u32 {
    FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Runs the private, one-shot Feedback Checkpoint B evidence probe and writes
/// its bounded JSON receipt into caller-owned storage.
///
/// This is diagnostic evidence, not part of the product document protocol.
///
/// # Safety
///
/// `output_ptr` must be uniquely writable for `output_capacity` bytes when
/// nonzero and `out_written` must be aligned, uniquely writable, and outside
/// that output range.
pub unsafe extern "C" fn flark_v3_checkpoint_b_probe(
    output_ptr: *mut u8,
    output_capacity: u32,
    out_written: *mut u32,
) -> u32 {
    ffi_guard(|| {
        write_output(out_written, 0)?;
        let encoded = crate::v3_checkpoint_b_probe::encode_probe_json()
            .map_err(|_| NativeError::CheckpointB)?;
        if encoded.len() > FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES as usize {
            return Err(NativeError::CheckpointB);
        }
        let output = write_buffer(
            output_ptr,
            output_capacity,
            FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES,
        )?;
        if output.len() < encoded.len() {
            return Err(NativeError::BoundExceeded);
        }
        output[..encoded.len()].copy_from_slice(&encoded);
        write_output(out_written, bounded_u32(encoded.len())?)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Writes the validated production admission profile.
///
/// # Safety
///
/// `out_config` must be a valid, aligned, uniquely writable pointer to one
/// [`FlarkV3EndpointConfig`].
pub unsafe extern "C" fn flark_v3_endpoint_config_standard(
    out_config: *mut FlarkV3EndpointConfig,
) -> u32 {
    ffi_guard(|| {
        write_output(out_config, FlarkV3EndpointConfig::default())?;
        let config = standard_native_config()?;
        let _ = endpoint_config(config)?;
        write_output(out_config, config)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Creates an endpoint that accepts one fresh parser-open command.
///
/// # Safety
///
/// `config` must point to one readable config and `out_handle` to one aligned,
/// uniquely writable, non-overlapping handle for the duration of this call.
pub unsafe extern "C" fn flark_v3_endpoint_create(
    config: *const FlarkV3EndpointConfig,
    out_handle: *mut FlarkV3EndpointHandle,
) -> u32 {
    ffi_guard(|| {
        write_output(out_handle, FlarkV3EndpointHandle::default())?;
        let config = endpoint_config(read_input(config)?)?;
        let handle = registry().create_fresh(config)?;
        write_output(out_handle, handle.into())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Creates an endpoint bound to the exact next recovery generation.
///
/// # Safety
///
/// `config` and `previous_binding` must be readable for this call and
/// `out_handle` must be aligned, uniquely writable, and not overlap either
/// input.
pub unsafe extern "C" fn flark_v3_endpoint_recover(
    config: *const FlarkV3EndpointConfig,
    previous_binding: *const FlarkV3SessionBinding,
    out_handle: *mut FlarkV3EndpointHandle,
) -> u32 {
    ffi_guard(|| {
        write_output(out_handle, FlarkV3EndpointHandle::default())?;
        let config = endpoint_config(read_input(config)?)?;
        let previous = SessionBinding::from(read_input(previous_binding)?);
        let handle = registry().create_recovery(previous, config)?;
        write_output(out_handle, handle.into())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Dispatches one exact caller-owned schema-v3 command frame.
///
/// # Safety
///
/// `input_ptr` must be readable for `input_len` bytes when nonzero and
/// `out_receipt` must be aligned, uniquely writable, and non-overlapping.
pub unsafe extern "C" fn flark_v3_endpoint_dispatch(
    handle: FlarkV3EndpointHandle,
    input_ptr: *const u8,
    input_len: u32,
    out_receipt: *mut FlarkV3DispatchReceipt,
) -> u32 {
    // SAFETY: this export's caller contract is forwarded unchanged.
    ffi_guard(|| unsafe { dispatch_call(handle, input_ptr, input_len, out_receipt, false) })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Dispatches one terminal schema-2 host-poll response. Schema-3 parser event
/// receipts remain exclusive to `flark_v3_endpoint_dispatch`.
///
/// # Safety
///
/// `input_ptr` must be readable for `input_len` bytes when nonzero and
/// `out_receipt` must be aligned, uniquely writable, and non-overlapping.
pub unsafe extern "C" fn flark_v3_endpoint_dispatch_host_poll(
    handle: FlarkV3EndpointHandle,
    input_ptr: *const u8,
    input_len: u32,
    out_receipt: *mut FlarkV3DispatchReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3DispatchReceipt::default())?;
        let input = read_buffer(input_ptr, input_len, FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES)?;
        let (result, outstanding) = registry().with_endpoint(handle.into(), |endpoint| {
            let result = endpoint.dispatch_host_poll(input);
            (result, endpoint.outstanding_status())
        })?;
        match result {
            Ok(receipt) => write_output(out_receipt, host_poll_receipt(receipt)),
            Err(error) => {
                if let Some(outstanding) = outstanding {
                    write_output(
                        out_receipt,
                        FlarkV3DispatchReceipt {
                            outstanding_event_id: outstanding.event_id,
                            outstanding_event_kind: event_kind(outstanding.kind),
                            ..FlarkV3DispatchReceipt::default()
                        },
                    )?;
                }
                Err(NativeError::Endpoint(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Dispatches one terminal hot-inline sidecar host-poll response. Its ticket
/// phases remain disjoint from structural host-poll responses.
///
/// # Safety
///
/// `input_ptr` must be readable for `input_len` bytes when nonzero and
/// `out_receipt` must be aligned, uniquely writable, and non-overlapping.
pub unsafe extern "C" fn flark_v3_endpoint_dispatch_inline_sidecar_host_poll(
    handle: FlarkV3EndpointHandle,
    input_ptr: *const u8,
    input_len: u32,
    out_receipt: *mut FlarkV3DispatchReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3DispatchReceipt::default())?;
        let input = read_buffer(input_ptr, input_len, FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES)?;
        let (result, outstanding) = registry().with_endpoint(handle.into(), |endpoint| {
            let result = endpoint.dispatch_inline_sidecar_host_poll(input);
            (result, endpoint.outstanding_status())
        })?;
        match result {
            Ok(receipt) => write_output(out_receipt, host_poll_receipt(receipt)),
            Err(error) => {
                if let Some(outstanding) = outstanding {
                    write_output(
                        out_receipt,
                        FlarkV3DispatchReceipt {
                            outstanding_event_id: outstanding.event_id,
                            outstanding_event_kind: event_kind(outstanding.kind),
                            ..FlarkV3DispatchReceipt::default()
                        },
                    )?;
                }
                Err(NativeError::Endpoint(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Dispatches one terminal viewport-presentation host-poll response. Its
/// ticket phases and ACK are disjoint from structural and point-sidecar
/// publication.
///
/// # Safety
///
/// `input_ptr` must be readable for `input_len` bytes when nonzero and
/// `out_receipt` must be aligned, uniquely writable, and non-overlapping.
pub unsafe extern "C" fn flark_v3_endpoint_dispatch_viewport_presentation_host_poll(
    handle: FlarkV3EndpointHandle,
    input_ptr: *const u8,
    input_len: u32,
    out_receipt: *mut FlarkV3DispatchReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3DispatchReceipt::default())?;
        let input = read_buffer(input_ptr, input_len, FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES)?;
        let (result, outstanding) = registry().with_endpoint(handle.into(), |endpoint| {
            let result = endpoint.dispatch_viewport_presentation_host_poll(input);
            (result, endpoint.outstanding_status())
        })?;
        match result {
            Ok(receipt) => write_output(out_receipt, host_poll_receipt(receipt)),
            Err(error) => {
                if let Some(outstanding) = outstanding {
                    write_output(
                        out_receipt,
                        FlarkV3DispatchReceipt {
                            outstanding_event_id: outstanding.event_id,
                            outstanding_event_kind: event_kind(outstanding.kind),
                            ..FlarkV3DispatchReceipt::default()
                        },
                    )?;
                }
                Err(NativeError::Endpoint(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Dispatches only an exact close command frame. Other opcodes are rejected
/// before the endpoint is mutated.
///
/// # Safety
///
/// `input_ptr` must be readable for `input_len` bytes when nonzero and
/// `out_receipt` must be aligned, uniquely writable, and non-overlapping.
pub unsafe extern "C" fn flark_v3_endpoint_close(
    handle: FlarkV3EndpointHandle,
    input_ptr: *const u8,
    input_len: u32,
    out_receipt: *mut FlarkV3DispatchReceipt,
) -> u32 {
    // SAFETY: this export's caller contract is forwarded unchanged.
    ffi_guard(|| unsafe { dispatch_call(handle, input_ptr, input_len, out_receipt, true) })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Advances source facts or quarantined cleanup under explicit bounded fuel.
///
/// # Safety
///
/// `out_receipt` must be aligned and uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_endpoint_poll(
    handle: FlarkV3EndpointHandle,
    fuel: FlarkV3PollFuel,
    out_receipt: *mut FlarkV3PollReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3PollReceipt::default())?;
        if fuel.maximum_source_bytes == 0 || fuel.maximum_checkpoints == 0 {
            return Err(NativeError::InvalidArgument);
        }
        if fuel.maximum_source_bytes > FLARK_V3_ENDPOINT_MAXIMUM_POLL_SOURCE_BYTES
            || fuel.maximum_checkpoints > FLARK_V3_ENDPOINT_MAXIMUM_POLL_CHECKPOINTS
            || fuel.maximum_retirement_transitions > MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS as u32
        {
            return Err(NativeError::BoundExceeded);
        }
        let (result, outstanding) = registry().with_endpoint(handle.into(), |endpoint| {
            let result = endpoint.poll_source_facts(EndpointPollFuel {
                maximum_source_bytes: fuel.maximum_source_bytes as usize,
                maximum_checkpoints: fuel.maximum_checkpoints as usize,
                maximum_retirement_transitions: fuel.maximum_retirement_transitions as usize,
            });
            (result, endpoint.outstanding_status())
        })?;
        match result {
            Ok(receipt) => write_output(out_receipt, poll_receipt(receipt, outstanding)?),
            Err(error) => {
                if let Some(outstanding) = outstanding {
                    write_output(
                        out_receipt,
                        FlarkV3PollReceipt {
                            outstanding_event_id: outstanding.event_id,
                            outstanding_event_kind: event_kind(outstanding.kind),
                            ..FlarkV3PollReceipt::default()
                        },
                    )?;
                }
                Err(NativeError::Endpoint(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Advances exact parsing, candidate sealing, publication, or producer
/// reclamation under one explicit transition grant.
///
/// # Safety
///
/// `out_receipt` must be aligned and uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_endpoint_poll_candidate(
    handle: FlarkV3EndpointHandle,
    maximum_transitions: u32,
    out_receipt: *mut FlarkV3CandidatePollReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3CandidatePollReceipt::default())?;
        if maximum_transitions == 0 {
            return Err(NativeError::InvalidArgument);
        }
        if maximum_transitions > MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS as u32 {
            return Err(NativeError::BoundExceeded);
        }
        let result = registry().with_endpoint(handle.into(), |endpoint| {
            endpoint.poll_candidate(maximum_transitions as usize)
        })?;
        match result {
            Ok(receipt) => write_output(out_receipt, candidate_poll_receipt(receipt)?),
            Err(error) => Err(NativeError::Endpoint(error)),
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Encodes the currently credited event directly into caller-owned storage.
///
/// # Safety
///
/// `output_ptr` must be uniquely writable for `output_capacity` bytes when
/// nonzero and `out_written` must be aligned, uniquely writable, and outside
/// that output range.
pub unsafe extern "C" fn flark_v3_endpoint_encode(
    handle: FlarkV3EndpointHandle,
    output_ptr: *mut u8,
    output_capacity: u32,
    out_written: *mut u32,
) -> u32 {
    ffi_guard(|| {
        write_output(out_written, 0)?;
        let output = write_buffer(
            output_ptr,
            output_capacity,
            FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES,
        )?;
        let encoded = registry().with_endpoint(handle.into(), |endpoint| {
            endpoint.encode_outstanding_event(output)
        })?;
        match encoded {
            Ok(written) => {
                let written = u32::try_from(written).map_err(|_| NativeError::NumericOverflow)?;
                write_output(out_written, written)
            }
            Err(EndpointError::Encode(SessionEncodeError::Envelope(
                EnvelopeEncodeError::BufferTooSmall { required, .. },
            ))) => {
                let required = u32::try_from(required).map_err(|_| NativeError::NumericOverflow)?;
                write_output(out_written, required)?;
                Err(NativeError::Endpoint(EndpointError::Encode(
                    SessionEncodeError::Envelope(EnvelopeEncodeError::BufferTooSmall {
                        required: required as usize,
                        available: output_capacity as usize,
                    }),
                )))
            }
            Err(EndpointError::PublicationEncode(PublicationEncodeError::Envelope(
                EnvelopeEncodeError::BufferTooSmall { required, .. },
            ))) => {
                let required = u32::try_from(required).map_err(|_| NativeError::NumericOverflow)?;
                write_output(out_written, required)?;
                Err(NativeError::Endpoint(EndpointError::PublicationEncode(
                    PublicationEncodeError::Envelope(EnvelopeEncodeError::BufferTooSmall {
                        required: required as usize,
                        available: output_capacity as usize,
                    }),
                )))
            }
            Err(error) => Err(NativeError::Endpoint(error)),
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Removes only an endpoint whose accepted `Closed` event receipt made it
/// `Removable`.
pub extern "C" fn flark_v3_endpoint_remove(handle: FlarkV3EndpointHandle) -> u32 {
    ffi_guard(|| registry().remove(handle.into()).map_err(NativeError::from))
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Abnormal finalization path. When no operation is in flight, this revokes the
/// handle and may run Endpoint's unmetered emergency containment Drop. If an
/// operation lease exists, it returns backpressure without revoking or
/// recycling the handle. It is never normal close.
pub extern "C" fn flark_v3_endpoint_emergency_destroy(handle: FlarkV3EndpointHandle) -> u32 {
    ffi_guard(|| {
        registry().emergency_destroy(handle.into())?;
        Ok(())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Allocates an opaque callback token containing only the generation-checked
/// handle, never an endpoint pointer.
///
/// # Safety
///
/// `out_token` must be aligned and uniquely writable. The returned token must
/// be consumed exactly once by `flark_v3_endpoint_finalizer_token_release` or
/// `flark_v3_endpoint_emergency_finalize`.
pub unsafe extern "C" fn flark_v3_endpoint_finalizer_token_create(
    handle: FlarkV3EndpointHandle,
    out_token: *mut *mut c_void,
) -> u32 {
    ffi_guard(|| {
        write_output(out_token, std::ptr::null_mut())?;
        let handle = EndpointHandle::from(handle);
        registry().validate_live(handle)?;
        let token = Box::into_raw(Box::new(FinalizerToken { handle })).cast::<c_void>();
        write_output(out_token, token)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Releases a detached finalizer token without touching its endpoint.
///
/// # Safety
///
/// `token` must be a still-live token returned by
/// `flark_v3_endpoint_finalizer_token_create` and must be consumed once.
pub unsafe extern "C" fn flark_v3_endpoint_finalizer_token_release(token: *mut c_void) -> u32 {
    ffi_guard(|| {
        if token.is_null() {
            return Err(NativeError::InvalidArgument);
        }
        // SAFETY: exact provenance and single consumption are the caller contract.
        unsafe { drop(Box::from_raw(token.cast::<FinalizerToken>())) };
        Ok(())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Dart `NativeFinalizer` compatible emergency trampoline. It consumes the
/// opaque token and never unwinds across the callback edge.
///
/// # Safety
///
/// `token` must be null or a still-live token returned by
/// `flark_v3_endpoint_finalizer_token_create`, consumed exactly once here.
pub unsafe extern "C" fn flark_v3_endpoint_emergency_finalize(token: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if token.is_null() {
            return;
        }
        // SAFETY: exact provenance and single consumption are the callback contract.
        let token = unsafe { Box::from_raw(token.cast::<FinalizerToken>()) };
        let _ = registry().emergency_destroy(token.handle);
    }));
}

fn ffi_guard(operation: impl FnOnce() -> NativeResult) -> u32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => u32::from(Status::Ok.code()),
        Ok(Err(error)) => u32::from(native_status(&error).code()),
        Err(_) => u32::from(Status::InternalFault.code()),
    }
}

unsafe fn dispatch_call(
    handle: FlarkV3EndpointHandle,
    input_ptr: *const u8,
    input_len: u32,
    out_receipt: *mut FlarkV3DispatchReceipt,
    require_close: bool,
) -> NativeResult {
    // SAFETY: the exported caller contract is forwarded unchanged.
    unsafe { write_output(out_receipt, FlarkV3DispatchReceipt::default())? };
    // SAFETY: the exported caller contract is forwarded unchanged.
    let input =
        unsafe { read_buffer(input_ptr, input_len, FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES)? };
    let (result, outstanding) = registry().with_endpoint(handle.into(), |endpoint| {
        let result = if require_close {
            let binding = endpoint.status().binding;
            match decode_command(input, binding) {
                Ok(decoded) if matches!(decoded.command, Command::BeginClose { .. }) => {
                    endpoint.dispatch(input)
                }
                Ok(_) => Err(EndpointError::InvalidLifecycle),
                Err(error) => Err(EndpointError::Decode(error)),
            }
        } else {
            endpoint.dispatch(input)
        };
        (result, endpoint.outstanding_status())
    })?;
    match result {
        Ok(receipt) => {
            // SAFETY: validated above and held only for this synchronous call.
            unsafe { write_output(out_receipt, dispatch_receipt(receipt)) }
        }
        Err(error) => {
            if let Some(outstanding) = outstanding {
                // SAFETY: validated above and held only for this synchronous call.
                unsafe {
                    write_output(
                        out_receipt,
                        FlarkV3DispatchReceipt {
                            correlation_id: 0,
                            action: 0,
                            outstanding_event_id: outstanding.event_id,
                            outstanding_event_kind: event_kind(outstanding.kind),
                        },
                    )?
                };
            }
            Err(NativeError::Endpoint(error))
        }
    }
}

fn standard_native_config() -> NativeResult<FlarkV3EndpointConfig> {
    let runtime = standard_document_runtime_config();
    let source_facts = SourceFactsRootLimits::default();
    Ok(FlarkV3EndpointConfig {
        abi_version: FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION,
        struct_size: u32::try_from(size_of::<FlarkV3EndpointConfig>())
            .map_err(|_| NativeError::NumericOverflow)?,
        max_retired_sources: bounded_u32(runtime.max_retired_sources)?,
        max_retired_source_bytes: bounded_u32(runtime.max_retired_source_bytes)?,
        arena_max_slots: bounded_u32(runtime.arena_limits.max_slots)?,
        arena_max_live_payload_bytes: bounded_u32(runtime.arena_limits.max_live_payload_bytes)?,
        arena_max_children_per_node: bounded_u32(runtime.arena_limits.max_children_per_node)?,
        checkpoint_spacing_utf16: STANDARD_CHECKPOINT_SPACING_UTF16,
        source_facts_max_checkpoints: bounded_u32(source_facts.max_checkpoints())?,
        source_facts_max_pages: bounded_u32(source_facts.max_pages())?,
        source_facts_max_resident_bytes: bounded_u32(source_facts.max_resident_bytes())?,
        parser_profile: 1,
        reserved: [0; 4],
    })
}

fn endpoint_config(config: FlarkV3EndpointConfig) -> NativeResult<EndpointConfig> {
    if config.abi_version != FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION
        || config.struct_size as usize != size_of::<FlarkV3EndpointConfig>()
        || config.parser_profile != 1
        || config.reserved != [0; 4]
    {
        return Err(NativeError::InvalidConfig);
    }
    let scan = SourceFactsScanProfile::new(config.checkpoint_spacing_utf16 as usize)
        .map_err(|_| NativeError::InvalidConfig)?;
    let root_limits = SourceFactsRootLimits::new(
        config.source_facts_max_checkpoints as usize,
        config.source_facts_max_pages as usize,
        config.source_facts_max_resident_bytes as usize,
    )
    .ok_or(NativeError::InvalidConfig)?;
    let parser_profile =
        ParserProfileId::new(u64::from(config.parser_profile)).ok_or(NativeError::InvalidConfig)?;
    EndpointConfig::new(
        DocumentRuntimeConfig {
            max_retired_sources: config.max_retired_sources as usize,
            max_retired_source_bytes: config.max_retired_source_bytes as usize,
            max_retained_source_edit_lineages: DocumentRuntimeConfig::default()
                .max_retained_source_edit_lineages,
            arena_limits: ArenaLimits {
                max_slots: config.arena_max_slots as usize,
                max_live_payload_bytes: config.arena_max_live_payload_bytes as usize,
                max_children_per_node: config.arena_max_children_per_node as usize,
            },
        },
        scan,
        root_limits,
        parser_profile,
    )
    .map_err(|_| NativeError::InvalidConfig)
}

fn dispatch_receipt(receipt: EndpointCommandReceipt) -> FlarkV3DispatchReceipt {
    let (outstanding_event_id, outstanding_event_kind) = receipt
        .outstanding_event
        .map(|event| (event.event_id, event_kind(event.kind)))
        .unwrap_or((0, 0));
    FlarkV3DispatchReceipt {
        correlation_id: receipt.correlation_id,
        action: command_action(receipt.action),
        outstanding_event_id,
        outstanding_event_kind,
    }
}

fn host_poll_receipt(receipt: EndpointHostPollReceipt) -> FlarkV3DispatchReceipt {
    let (outstanding_event_id, outstanding_event_kind) = receipt
        .outstanding_event
        .map(|event| (event.event_id, event_kind(event.kind)))
        .unwrap_or((0, 0));
    FlarkV3DispatchReceipt {
        correlation_id: receipt.correlation_id,
        action: match receipt.action {
            EndpointHostPollAction::CreditAccepted => 1,
            EndpointHostPollAction::CandidateCancelled => 2,
            EndpointHostPollAction::DeliveryEmitted => 3,
        },
        outstanding_event_id,
        outstanding_event_kind,
    }
}

fn candidate_poll_receipt(
    receipt: EndpointCandidatePollReceipt,
) -> NativeResult<FlarkV3CandidatePollReceipt> {
    let (outstanding_event_id, outstanding_event_kind) = receipt
        .outstanding_event
        .map(|event| (event.event_id, event_kind(event.kind)))
        .unwrap_or((0, 0));
    Ok(FlarkV3CandidatePollReceipt {
        transitions: bounded_u32(receipt.transitions)?,
        cleanup_complete: u32::from(receipt.cleanup_complete),
        outstanding_event_id,
        outstanding_event_kind,
    })
}

fn poll_receipt(
    receipt: EndpointPollReceipt,
    outstanding: Option<crate::v3_endpoint::OutstandingEventStatus>,
) -> NativeResult<FlarkV3PollReceipt> {
    let (outstanding_event_id, outstanding_event_kind) = outstanding
        .map(|event| (event.event_id, event_kind(event.kind)))
        .unwrap_or((0, 0));
    Ok(FlarkV3PollReceipt {
        source_bytes_examined: bounded_u32(receipt.source_bytes_examined)?,
        source_bytes_buffered: bounded_u32(receipt.source_bytes_buffered)?,
        cursor_refills: bounded_u32(receipt.cursor_refills)?,
        cursor_copy_bytes_upper_bound: bounded_u32(receipt.cursor_copy_bytes_upper_bound)?,
        checkpoints_emitted: bounded_u32(receipt.checkpoints_emitted)?,
        source_fact_transitions: bounded_u32(receipt.source_fact_transitions)?,
        released_source_leases: bounded_u32(receipt.released_source_leases)?,
        released_source_bytes: bounded_u32(receipt.released_source_bytes)?,
        arena_transitions: bounded_u32(receipt.arena_transitions)?,
        arena_nodes_reclaimed: bounded_u32(receipt.arena_nodes_reclaimed)?,
        scan_complete: u32::from(receipt.scan_complete),
        certification: receipt.certification.map(certification).unwrap_or(0),
        cleanup_complete: u32::from(receipt.cleanup_complete),
        outstanding_event_id,
        outstanding_event_kind,
    })
}

const fn command_action(action: EndpointCommandAction) -> u32 {
    match action {
        EndpointCommandAction::Opened => 1,
        EndpointCommandAction::SourcePageAccepted => 2,
        EndpointCommandAction::SourceBatchAccepted => 3,
        EndpointCommandAction::Superseded => 4,
        EndpointCommandAction::EventReceiptAccepted => 5,
        EndpointCommandAction::CloseLatched => 6,
        EndpointCommandAction::DrainAdvanced => 7,
        EndpointCommandAction::InlineRefinementAccepted => 8,
        EndpointCommandAction::ViewportPresentationAccepted => 9,
    }
}

const fn event_kind(kind: EndpointEventKind) -> u32 {
    match kind {
        EndpointEventKind::Opened => 1,
        EndpointEventKind::SourceSynchronized => 2,
        EndpointEventKind::SourceFactsPage => 3,
        EndpointEventKind::SourceFactsCompleted => 4,
        EndpointEventKind::Failed => 5,
        EndpointEventKind::DrainProgress => 6,
        EndpointEventKind::Closed => 7,
        EndpointEventKind::PublicationBegin => 8,
        EndpointEventKind::PublicationPacket => 9,
        EndpointEventKind::PublicationCommit => 10,
        EndpointEventKind::PublicationDeliveryAcknowledged => 11,
        EndpointEventKind::SourceFactsDeltaBegin => 12,
        EndpointEventKind::SourceFactsDeltaPage => 13,
        EndpointEventKind::SourceFactsDeltaCompleted => 14,
        EndpointEventKind::InlinePublicationBegin => 15,
        EndpointEventKind::InlinePublicationPacket => 16,
        EndpointEventKind::InlinePublicationCommit => 17,
        EndpointEventKind::InlinePublicationDeliveryAcknowledged => 18,
        EndpointEventKind::InlineRefinementUnavailable => 19,
        EndpointEventKind::ViewportPresentationUnavailable => 20,
        EndpointEventKind::ViewportPublicationBegin => 21,
        EndpointEventKind::ViewportPublicationPacket => 22,
        EndpointEventKind::ViewportPublicationCommit => 23,
        EndpointEventKind::ViewportPublicationDeliveryAcknowledged => 24,
    }
}

const fn certification(certification: CertificationStatus) -> u32 {
    match certification {
        CertificationStatus::NotStarted => 1,
        CertificationStatus::Scanning => 2,
        CertificationStatus::Publishing => 3,
        CertificationStatus::AwaitingPromotion => 4,
        CertificationStatus::ExternallyEligible => 5,
    }
}

fn bounded_u32(value: usize) -> NativeResult<u32> {
    u32::try_from(value).map_err(|_| NativeError::NumericOverflow)
}

fn native_status(error: &NativeError) -> Status {
    match error {
        NativeError::InvalidArgument | NativeError::InvalidConfig => Status::Invalid,
        NativeError::BoundExceeded => Status::ForegroundBoundExceeded,
        NativeError::NumericOverflow | NativeError::CheckpointB => Status::InternalFault,
        NativeError::Registry(error) => match error {
            RegistryError::InvalidLimit
            | RegistryError::InvalidHandle
            | RegistryError::InvalidRecovery => Status::Invalid,
            RegistryError::StaleHandle => Status::Closed,
            RegistryError::CapacityExceeded | RegistryError::AllocationFailed => {
                Status::AllocationFailed
            }
            RegistryError::Poisoned => Status::InternalFault,
            RegistryError::InUse => Status::Backpressure,
            RegistryError::NotRemovable => Status::InvalidState,
        },
        NativeError::Endpoint(error) => endpoint_status(error),
    }
}

fn endpoint_status(error: &EndpointError) -> Status {
    match error {
        EndpointError::InvalidConfig => Status::Invalid,
        EndpointError::Decode(error) => match error.failure {
            SessionDecodeFailure::Envelope(failure) => match failure {
                EnvelopeDecodeFailure::UnsupportedMajor => Status::UnsupportedMajor,
                EnvelopeDecodeFailure::UnsupportedMinor => Status::UnsupportedMinor,
                EnvelopeDecodeFailure::UnknownOpcode => Status::UnknownOpcode,
                EnvelopeDecodeFailure::ForbiddenFlags => Status::ForbiddenFlags,
                EnvelopeDecodeFailure::PayloadTooLarge => Status::ForegroundBoundExceeded,
                _ => Status::MalformedFrame,
            },
            SessionDecodeFailure::UnsupportedSchema => Status::UnsupportedSchema,
            SessionDecodeFailure::UnexpectedOpcode | SessionDecodeFailure::UnknownVariant => {
                Status::UnknownOpcode
            }
            SessionDecodeFailure::OversizedValue => Status::ForegroundBoundExceeded,
            SessionDecodeFailure::InvalidUtf8 => Status::InvalidUtf8,
            SessionDecodeFailure::IdentityMismatch => Status::InvalidState,
            SessionDecodeFailure::TruncatedPayload
            | SessionDecodeFailure::TrailingPayload
            | SessionDecodeFailure::InvalidValue => Status::MalformedFrame,
        },
        EndpointError::Encode(error) => match error {
            SessionEncodeError::Envelope(EnvelopeEncodeError::BufferTooSmall { .. }) => {
                Status::ForegroundBoundExceeded
            }
            _ => Status::InternalFault,
        },
        EndpointError::PublicationDecode(_) => Status::MalformedFrame,
        EndpointError::PublicationEncode(error) => match error {
            PublicationEncodeError::Envelope(EnvelopeEncodeError::BufferTooSmall { .. }) => {
                Status::ForegroundBoundExceeded
            }
            _ => Status::InternalFault,
        },
        EndpointError::Candidate => Status::InternalFault,
        EndpointError::EventCreditOccupied { .. } => Status::Backpressure,
        EndpointError::NoOutstandingEvent => Status::NotReady,
        EndpointError::ReceiptMismatch { .. }
        | EndpointError::InvalidLifecycle
        | EndpointError::InvalidSeed
        | EndpointError::InvalidEdit
        | EndpointError::InvalidReceipt => Status::InvalidState,
        EndpointError::InvalidPollFuel => Status::Invalid,
        EndpointError::SourceFacts | EndpointError::EventIdentityExhausted => Status::InternalFault,
    }
}

unsafe fn read_input<T: Copy>(pointer: *const T) -> NativeResult<T> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(NativeError::InvalidArgument);
    }
    // SAFETY: checked for null/alignment; validity is the exported caller contract.
    Ok(unsafe { pointer.read() })
}

unsafe fn write_output<T>(pointer: *mut T, value: T) -> NativeResult {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(NativeError::InvalidArgument);
    }
    // SAFETY: checked for null/alignment; writability is the caller contract.
    unsafe { pointer.write(value) };
    Ok(())
}

unsafe fn read_buffer<'buffer>(
    pointer: *const u8,
    length: u32,
    maximum: u32,
) -> NativeResult<&'buffer [u8]> {
    if length > maximum {
        return Err(NativeError::BoundExceeded);
    }
    if length == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err(NativeError::InvalidArgument);
    }
    // SAFETY: non-null, globally bounded, and readable by the caller contract.
    Ok(unsafe { std::slice::from_raw_parts(pointer, length as usize) })
}

unsafe fn write_buffer<'buffer>(
    pointer: *mut u8,
    length: u32,
    maximum: u32,
) -> NativeResult<&'buffer mut [u8]> {
    if length > maximum {
        return Err(NativeError::BoundExceeded);
    }
    if length == 0 {
        return Ok(&mut []);
    }
    if pointer.is_null() {
        return Err(NativeError::InvalidArgument);
    }
    // SAFETY: non-null, globally bounded, uniquely writable caller storage.
    Ok(unsafe { std::slice::from_raw_parts_mut(pointer, length as usize) })
}

// Compile-time C ABI signature guards. Changing an exported signature requires
// an intentional ABI version/header update.
const _: extern "C" fn() -> u32 = flark_v3_endpoint_native_abi_version;
const _: unsafe extern "C" fn(*mut u8, u32, *mut u32) -> u32 = flark_v3_checkpoint_b_probe;
const _: unsafe extern "C" fn(*mut FlarkV3EndpointConfig) -> u32 =
    flark_v3_endpoint_config_standard;
const _: unsafe extern "C" fn(*const FlarkV3EndpointConfig, *mut FlarkV3EndpointHandle) -> u32 =
    flark_v3_endpoint_create;
const _: unsafe extern "C" fn(
    *const FlarkV3EndpointConfig,
    *const FlarkV3SessionBinding,
    *mut FlarkV3EndpointHandle,
) -> u32 = flark_v3_endpoint_recover;
const _: unsafe extern "C" fn(
    FlarkV3EndpointHandle,
    *const u8,
    u32,
    *mut FlarkV3DispatchReceipt,
) -> u32 = flark_v3_endpoint_dispatch;
const _: unsafe extern "C" fn(
    FlarkV3EndpointHandle,
    *const u8,
    u32,
    *mut FlarkV3DispatchReceipt,
) -> u32 = flark_v3_endpoint_dispatch_host_poll;
const _: unsafe extern "C" fn(
    FlarkV3EndpointHandle,
    *const u8,
    u32,
    *mut FlarkV3DispatchReceipt,
) -> u32 = flark_v3_endpoint_dispatch_inline_sidecar_host_poll;
const _: unsafe extern "C" fn(
    FlarkV3EndpointHandle,
    *const u8,
    u32,
    *mut FlarkV3DispatchReceipt,
) -> u32 = flark_v3_endpoint_dispatch_viewport_presentation_host_poll;
const _: unsafe extern "C" fn(
    FlarkV3EndpointHandle,
    FlarkV3PollFuel,
    *mut FlarkV3PollReceipt,
) -> u32 = flark_v3_endpoint_poll;
const _: unsafe extern "C" fn(FlarkV3EndpointHandle, u32, *mut FlarkV3CandidatePollReceipt) -> u32 =
    flark_v3_endpoint_poll_candidate;
const _: unsafe extern "C" fn(FlarkV3EndpointHandle, *mut u8, u32, *mut u32) -> u32 =
    flark_v3_endpoint_encode;
const _: unsafe extern "C" fn(
    FlarkV3EndpointHandle,
    *const u8,
    u32,
    *mut FlarkV3DispatchReceipt,
) -> u32 = flark_v3_endpoint_close;
const _: extern "C" fn(FlarkV3EndpointHandle) -> u32 = flark_v3_endpoint_remove;
const _: extern "C" fn(FlarkV3EndpointHandle) -> u32 = flark_v3_endpoint_emergency_destroy;
const _: unsafe extern "C" fn(FlarkV3EndpointHandle, *mut *mut c_void) -> u32 =
    flark_v3_endpoint_finalizer_token_create;
const _: unsafe extern "C" fn(*mut c_void) -> u32 = flark_v3_endpoint_finalizer_token_release;
const _: unsafe extern "C" fn(*mut c_void) = flark_v3_endpoint_emergency_finalize;

const _: () = {
    assert!(SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX <= u32::MAX as usize);
    assert!(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX <= u32::MAX as usize);
    assert!(size_of::<FlarkV3EndpointHandle>() == 8);
    assert!(align_of::<FlarkV3EndpointHandle>() == 4);
    assert!(offset_of!(FlarkV3EndpointHandle, generation) == 4);
    assert!(size_of::<FlarkV3EndpointConfig>() == 64);
    assert!(align_of::<FlarkV3EndpointConfig>() == 4);
    assert!(offset_of!(FlarkV3EndpointConfig, reserved) == 48);
    assert!(size_of::<FlarkV3SessionBinding>() == 24);
    assert!(offset_of!(FlarkV3SessionBinding, source_session_identity) == 16);
    assert!(offset_of!(FlarkV3SessionBinding, worker_generation) == 20);
    assert!(size_of::<FlarkV3DispatchReceipt>() == 16);
    assert!(offset_of!(FlarkV3DispatchReceipt, outstanding_event_id) == 8);
    assert!(size_of::<FlarkV3PollFuel>() == 12);
    assert!(size_of::<FlarkV3PollReceipt>() == 60);
    assert!(offset_of!(FlarkV3PollReceipt, source_fact_transitions) == 20);
    assert!(offset_of!(FlarkV3PollReceipt, outstanding_event_id) == 52);
    assert!(size_of::<FlarkV3CandidatePollReceipt>() == 16);
    assert!(offset_of!(FlarkV3CandidatePollReceipt, outstanding_event_id) == 8);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3_registry::{MAXIMUM_FRESH_ENDPOINTS, MAXIMUM_RESIDENT_ENDPOINT_SLOTS};
    use crate::v3_session_wire::PAYLOAD_SCHEMA;
    use crate::v3_wire::{self, FrameKind, Header, Opcode};
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn binding() -> FlarkV3SessionBinding {
        FlarkV3SessionBinding {
            document_session: [11, 22, 33, 44],
            source_session_identity: 55,
            worker_generation: 1,
        }
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_common(bytes: &mut Vec<u8>, variant: u16, binding: FlarkV3SessionBinding) {
        push_u16(bytes, PAYLOAD_SCHEMA);
        push_u16(bytes, variant);
        push_u32(bytes, binding.worker_generation);
        for word in binding.document_session {
            push_u32(bytes, word);
        }
        push_u32(bytes, binding.source_session_identity);
    }

    fn frame(opcode: Opcode, correlation_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + payload.len()];
        let written = v3_wire::encode_into(
            FrameKind::Request,
            Header {
                opcode,
                status: Status::Ok,
                flags: 0,
                correlation_id,
            },
            payload,
            &mut output,
        )
        .unwrap();
        output.truncate(written);
        output
    }

    fn open_frame(binding: FlarkV3SessionBinding) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        frame(Opcode::ParserOpen, 100, &payload)
    }

    fn recovery_open_frame(binding: FlarkV3SessionBinding) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 1, binding);
        frame(Opcode::ParserOpen, 101, &payload)
    }

    fn accepted_event_frame(binding: FlarkV3SessionBinding, event_id: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        frame(Opcode::ParserAcknowledge, event_id, &payload)
    }

    fn accepted_empty_snapshot_frame(binding: FlarkV3SessionBinding, event_id: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, 1);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 1);
        frame(Opcode::ParserAcknowledge, event_id, &payload)
    }

    fn empty_snapshot_frame(binding: FlarkV3SessionBinding) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, 1);
        push_u32(&mut payload, 1);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 1);
        push_u32(&mut payload, 0);
        for _ in 0..5 {
            push_u32(&mut payload, 0);
        }
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
        frame(Opcode::SnapshotPage, 1, &payload)
    }

    fn mismatched_known_snapshot_frame(binding: FlarkV3SessionBinding) -> Vec<u8> {
        const SOURCE: &str = "abc";
        const HASH_BASES: [u32; 4] = [0x0010_0193, 0x9e37_79b1, 0x85eb_ca77, 0xc2b2_ae3d];
        let mut content_hash128 = [0_u32; 4];
        for byte in SOURCE.bytes() {
            for (word, base) in content_hash128.iter_mut().zip(HASH_BASES) {
                *word = word.wrapping_mul(base).wrapping_add(u32::from(byte) + 1);
            }
        }
        content_hash128[0] ^= 1;

        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, 1); // lease id
        push_u32(&mut payload, 1); // base UI revision
        push_u32(&mut payload, 0); // start UTF-16
        push_u32(&mut payload, 3); // end UTF-16
        push_u32(&mut payload, 3); // total UTF-16
        push_u32(&mut payload, 0); // intent high-water
        push_u32(&mut payload, 1); // known stamp
        push_u32(&mut payload, 1); // target revision
        push_u32(&mut payload, 3); // target UTF-16
        push_u32(&mut payload, 3); // target UTF-8
        for word in content_hash128 {
            push_u32(&mut payload, word);
        }
        push_u32(&mut payload, 3); // page UTF-16
        push_u32(&mut payload, 3); // page UTF-8
        payload.extend_from_slice(SOURCE.as_bytes());
        frame(Opcode::SnapshotPage, 1, &payload)
    }

    fn close_frame(binding: FlarkV3SessionBinding) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, binding.worker_generation);
        frame(Opcode::Close, 200, &payload)
    }

    fn drain_frame(binding: FlarkV3SessionBinding, drain_id: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, drain_id);
        push_u32(&mut payload, 1);
        frame(Opcode::Drain, drain_id, &payload)
    }

    fn supersede_frame(binding: FlarkV3SessionBinding) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, 0);
        frame(Opcode::Supersede, 300, &payload)
    }

    fn dispatch(handle: FlarkV3EndpointHandle, command: &[u8]) -> (u32, FlarkV3DispatchReceipt) {
        let mut receipt = FlarkV3DispatchReceipt::default();
        // SAFETY: command and receipt remain valid for the synchronous call.
        let status = unsafe {
            flark_v3_endpoint_dispatch(handle, command.as_ptr(), command.len() as u32, &mut receipt)
        };
        (status, receipt)
    }

    fn open(handle: FlarkV3EndpointHandle, binding: FlarkV3SessionBinding) {
        let (status, opened) = dispatch(handle, &open_frame(binding));
        assert_eq!(status, ok());
        assert_eq!(opened.action, 1);
        assert_eq!(opened.outstanding_event_kind, 1);
        let (status, accepted) = dispatch(
            handle,
            &accepted_event_frame(binding, opened.outstanding_event_id),
        );
        assert_eq!(status, ok());
        assert_eq!(accepted.action, 5);
        assert_eq!(accepted.outstanding_event_id, 0);
    }

    fn ok() -> u32 {
        u32::from(Status::Ok.code())
    }

    fn standard_config() -> FlarkV3EndpointConfig {
        let mut config = FlarkV3EndpointConfig::default();
        // SAFETY: valid unique output pointer.
        assert_eq!(
            unsafe { flark_v3_endpoint_config_standard(&mut config) },
            ok()
        );
        config
    }

    fn create() -> FlarkV3EndpointHandle {
        let config = standard_config();
        let mut handle = FlarkV3EndpointHandle::default();
        // SAFETY: valid input/output pointers for this call.
        assert_eq!(
            unsafe { flark_v3_endpoint_create(&config, &mut handle) },
            ok()
        );
        assert_ne!(handle, FlarkV3EndpointHandle::default());
        handle
    }

    #[test]
    fn standard_config_is_explicit_versioned_and_revalidated() {
        let config = standard_config();
        assert_eq!(config.abi_version, FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION);
        assert_eq!(
            config.struct_size as usize,
            size_of::<FlarkV3EndpointConfig>()
        );
        assert_eq!(
            config.arena_max_slots as usize,
            flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS
        );
        assert_eq!(
            config.arena_max_live_payload_bytes as usize,
            crate::v3_endpoint::V3_PRODUCER_ARENA_MAX_LIVE_PAYLOAD_BYTES
        );
        assert!(endpoint_config(config).is_ok());

        let mut bad = config;
        bad.reserved[2] = 1;
        let mut handle = FlarkV3EndpointHandle {
            slot: 99,
            generation: 99,
        };
        // SAFETY: valid input/output pointers for this call.
        let status = unsafe { flark_v3_endpoint_create(&bad, &mut handle) };
        assert_eq!(status, u32::from(Status::Invalid.code()));
        assert_eq!(handle, FlarkV3EndpointHandle::default());
    }

    #[test]
    fn recovery_ffi_binds_the_exact_previous_and_next_generation() {
        let config = standard_config();
        let previous = binding();
        let mut handle = FlarkV3EndpointHandle::default();
        // SAFETY: valid input and output pointers.
        assert_eq!(
            unsafe { flark_v3_endpoint_recover(&config, &previous, &mut handle) },
            ok()
        );
        let next = FlarkV3SessionBinding {
            worker_generation: previous.worker_generation + 1,
            ..previous
        };
        let (status, opened) = dispatch(handle, &recovery_open_frame(next));
        assert_eq!(status, ok());
        assert_eq!(opened.action, 1);
        let (status, _) = dispatch(
            handle,
            &accepted_event_frame(next, opened.outstanding_event_id),
        );
        assert_eq!(status, ok());
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());

        let invalid = FlarkV3SessionBinding {
            source_session_identity: 0,
            ..previous
        };
        let mut rejected = FlarkV3EndpointHandle {
            slot: 99,
            generation: 99,
        };
        // SAFETY: valid input and output pointers.
        assert_eq!(
            unsafe { flark_v3_endpoint_recover(&config, &invalid, &mut rejected) },
            u32::from(Status::Invalid.code())
        );
        assert_eq!(rejected, FlarkV3EndpointHandle::default());
    }

    #[test]
    fn null_unaligned_and_oversized_buffers_fail_before_endpoint_access() {
        let handle = create();
        let mut receipt = FlarkV3DispatchReceipt::default();
        // SAFETY: null is deliberately exercised and rejected.
        assert_eq!(
            unsafe { flark_v3_endpoint_dispatch(handle, std::ptr::null(), 1, &mut receipt) },
            u32::from(Status::Invalid.code())
        );
        let mut unaligned_storage =
            vec![0_u8; size_of::<FlarkV3DispatchReceipt>() + align_of::<FlarkV3DispatchReceipt>()];
        let unaligned_offset = (0..align_of::<FlarkV3DispatchReceipt>())
            .find(|offset| {
                !(unaligned_storage.as_ptr() as usize + offset)
                    .is_multiple_of(align_of::<FlarkV3DispatchReceipt>())
            })
            .unwrap();
        let unaligned_receipt =
            // SAFETY: the pointer remains inside the allocation and is never dereferenced.
            unsafe { unaligned_storage.as_mut_ptr().add(unaligned_offset) }
                .cast::<FlarkV3DispatchReceipt>();
        // SAFETY: the deliberately unaligned output is rejected before access.
        assert_eq!(
            unsafe { flark_v3_endpoint_dispatch(handle, std::ptr::null(), 0, unaligned_receipt) },
            u32::from(Status::Invalid.code())
        );
        // SAFETY: the length is rejected before the pointer is read.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_dispatch(
                    handle,
                    std::ptr::null(),
                    FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES + 1,
                    &mut receipt,
                )
            },
            u32::from(Status::ForegroundBoundExceeded.code())
        );
        let mut written = 99;
        // SAFETY: the capacity is rejected before the pointer is written.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_encode(
                    handle,
                    std::ptr::null_mut(),
                    FLARK_V3_ENDPOINT_MAXIMUM_FRAME_BYTES + 1,
                    &mut written,
                )
            },
            u32::from(Status::ForegroundBoundExceeded.code())
        );
        assert_eq!(written, 0);
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn poll_bounds_are_checked_before_endpoint_work() {
        let handle = create();
        let mut receipt = FlarkV3PollReceipt::default();
        // SAFETY: valid output pointer.
        let status = unsafe {
            flark_v3_endpoint_poll(
                handle,
                FlarkV3PollFuel {
                    maximum_source_bytes: FLARK_V3_ENDPOINT_MAXIMUM_POLL_SOURCE_BYTES + 1,
                    maximum_checkpoints: 1,
                    maximum_retirement_transitions: 0,
                },
                &mut receipt,
            )
        };
        assert_eq!(status, u32::from(Status::ForegroundBoundExceeded.code()));
        assert_eq!(receipt, FlarkV3PollReceipt::default());

        // SAFETY: valid output pointer and exact declared maxima.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: FLARK_V3_ENDPOINT_MAXIMUM_POLL_SOURCE_BYTES,
                        maximum_checkpoints: FLARK_V3_ENDPOINT_MAXIMUM_POLL_CHECKPOINTS,
                        maximum_retirement_transitions: 0,
                    },
                    &mut receipt,
                )
            },
            ok()
        );
        // SAFETY: valid output pointer; oversized checkpoint fuel is rejected.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: 1,
                        maximum_checkpoints: FLARK_V3_ENDPOINT_MAXIMUM_POLL_CHECKPOINTS + 1,
                        maximum_retirement_transitions: 0,
                    },
                    &mut receipt,
                )
            },
            u32::from(Status::ForegroundBoundExceeded.code())
        );
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn publication_size_query_uses_the_same_bounded_retry_status_as_session_events() {
        let error = EndpointError::PublicationEncode(PublicationEncodeError::Envelope(
            EnvelopeEncodeError::BufferTooSmall {
                required: 240,
                available: 0,
            },
        ));
        assert_eq!(endpoint_status(&error), Status::ForegroundBoundExceeded);
    }

    #[test]
    fn zero_poll_fuel_is_rejected_without_faulting_an_active_scan() {
        let handle = create();
        let binding = binding();
        open(handle, binding);
        let (status, synchronized) = dispatch(handle, &empty_snapshot_frame(binding));
        assert_eq!(status, ok());
        assert_eq!(synchronized.outstanding_event_kind, 2);
        let (status, _) = dispatch(
            handle,
            &accepted_empty_snapshot_frame(binding, synchronized.outstanding_event_id),
        );
        assert_eq!(status, ok());

        let mut receipt = FlarkV3PollReceipt::default();
        // SAFETY: valid output pointer; zero fuel is intentionally rejected.
        assert_eq!(
            unsafe { flark_v3_endpoint_poll(handle, FlarkV3PollFuel::default(), &mut receipt) },
            u32::from(Status::Invalid.code())
        );
        assert_eq!(receipt, FlarkV3PollReceipt::default());

        // SAFETY: valid bounded fuel/output. Success proves zero fuel did not
        // poison or close the active scan.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: 1,
                        maximum_checkpoints: 1,
                        maximum_retirement_transitions: 0,
                    },
                    &mut receipt,
                )
            },
            ok()
        );
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn mutating_poll_error_still_reports_the_failed_event_credit() {
        let handle = create();
        let binding = binding();
        open(handle, binding);
        let (status, synchronized) = dispatch(handle, &mismatched_known_snapshot_frame(binding));
        assert_eq!(status, ok());
        assert_eq!(synchronized.outstanding_event_kind, 2);
        let (status, _) = dispatch(
            handle,
            &accepted_empty_snapshot_frame(binding, synchronized.outstanding_event_id),
        );
        assert_eq!(status, ok());

        let mut failed = None;
        for _ in 0..4 {
            let mut receipt = FlarkV3PollReceipt::default();
            // SAFETY: valid bounded fuel and unique output storage.
            let status = unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: 64,
                        maximum_checkpoints: 8,
                        maximum_retirement_transitions: 1,
                    },
                    &mut receipt,
                )
            };
            if status != ok() {
                failed = Some((status, receipt));
                break;
            }
        }
        let (status, receipt) = failed.expect("known hash mismatch must fail bounded polling");
        assert_eq!(status, u32::from(Status::InternalFault.code()));
        assert_ne!(receipt.outstanding_event_id, 0);
        assert_eq!(receipt.outstanding_event_kind, 5);
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn encode_size_query_and_replay_do_not_consume_event_credit() {
        let handle = create();
        let binding = binding();
        let (status, opened) = dispatch(handle, &open_frame(binding));
        assert_eq!(status, ok());
        assert_ne!(opened.outstanding_event_id, 0);

        let mut required = 0;
        // SAFETY: a null buffer is permitted at exactly zero capacity.
        let status =
            unsafe { flark_v3_endpoint_encode(handle, std::ptr::null_mut(), 0, &mut required) };
        assert_eq!(status, u32::from(Status::ForegroundBoundExceeded.code()));
        assert!(required >= v3_wire::HEADER_BYTES as u32);

        let mut first = vec![0; required as usize];
        let mut first_written = 0;
        // SAFETY: caller-owned output and length are exact.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_encode(
                    handle,
                    first.as_mut_ptr(),
                    first.len() as u32,
                    &mut first_written,
                )
            },
            ok()
        );
        assert_eq!(first_written, required);

        let mut replay = vec![0; required as usize];
        let mut replay_written = 0;
        // SAFETY: caller-owned output and length are exact.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_encode(
                    handle,
                    replay.as_mut_ptr(),
                    replay.len() as u32,
                    &mut replay_written,
                )
            },
            ok()
        );
        assert_eq!(replay_written, required);
        assert_eq!(replay, first);

        let (status, _) = dispatch(
            handle,
            &accepted_event_frame(binding, opened.outstanding_event_id),
        );
        assert_eq!(status, ok());
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn graceful_close_requires_accepted_closed_receipt_before_remove() {
        let handle = create();
        let binding = binding();
        assert_eq!(
            flark_v3_endpoint_remove(handle),
            u32::from(Status::InvalidState.code())
        );
        open(handle, binding);

        let close = close_frame(binding);
        let mut close_receipt = FlarkV3DispatchReceipt::default();
        // SAFETY: command and receipt remain valid for the synchronous call.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_close(
                    handle,
                    close.as_ptr(),
                    close.len() as u32,
                    &mut close_receipt,
                )
            },
            ok()
        );
        assert_eq!(close_receipt.action, 6);
        assert_eq!(
            flark_v3_endpoint_remove(handle),
            u32::from(Status::InvalidState.code())
        );

        let (status, drain) = dispatch(handle, &drain_frame(binding, 201));
        assert_eq!(status, ok());
        assert_eq!(drain.action, 7);
        assert_eq!(drain.outstanding_event_kind, 6);
        let (status, closed) = dispatch(
            handle,
            &accepted_event_frame(binding, drain.outstanding_event_id),
        );
        assert_eq!(status, ok());
        assert_eq!(closed.outstanding_event_kind, 7);
        assert_eq!(
            flark_v3_endpoint_remove(handle),
            u32::from(Status::InvalidState.code())
        );

        let (status, final_receipt) = dispatch(
            handle,
            &accepted_event_frame(binding, closed.outstanding_event_id),
        );
        assert_eq!(status, ok());
        assert_eq!(final_receipt.outstanding_event_id, 0);
        assert_eq!(flark_v3_endpoint_remove(handle), ok());
        assert_eq!(
            flark_v3_endpoint_remove(handle),
            u32::from(Status::Closed.code())
        );
    }

    #[test]
    fn close_entrypoint_rejects_other_opcodes_before_mutation() {
        let handle = create();
        let binding = binding();
        open(handle, binding);
        let supersede = supersede_frame(binding);
        let mut receipt = FlarkV3DispatchReceipt::default();
        // SAFETY: command and receipt remain valid for the synchronous call.
        let status = unsafe {
            flark_v3_endpoint_close(
                handle,
                supersede.as_ptr(),
                supersede.len() as u32,
                &mut receipt,
            )
        };
        assert_eq!(status, u32::from(Status::InvalidState.code()));
        assert_eq!(receipt, FlarkV3DispatchReceipt::default());
        let (status, dispatched) = dispatch(handle, &supersede);
        assert_eq!(status, ok());
        assert_eq!(dispatched.action, 4);
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn mutating_dispatch_error_still_reports_the_failed_event_credit() {
        let handle = create();
        let binding = binding();
        open(handle, binding);
        let malformed = vec![0; v3_wire::HEADER_BYTES];
        let (status, receipt) = dispatch(handle, &malformed);
        assert_eq!(status, u32::from(Status::MalformedFrame.code()));
        assert_ne!(receipt.outstanding_event_id, 0);
        assert_eq!(receipt.outstanding_event_kind, 5);

        let mut required = 0;
        // SAFETY: zero-capacity size query has no output buffer.
        assert_eq!(
            unsafe { flark_v3_endpoint_encode(handle, std::ptr::null_mut(), 0, &mut required) },
            u32::from(Status::ForegroundBoundExceeded.code())
        );
        assert_ne!(required, 0);
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn panic_guard_returns_internal_fault_without_crossing_ffi() {
        let status = ffi_guard(|| -> NativeResult { panic!("synthetic native panic") });
        assert_eq!(status, u32::from(Status::InternalFault.code()));
    }

    #[test]
    fn removed_handle_is_closed_and_emergency_symbol_is_distinct() {
        let handle = create();
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
        let mut receipt = FlarkV3PollReceipt::default();
        // SAFETY: valid output pointer.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: 1,
                        maximum_checkpoints: 1,
                        maximum_retirement_transitions: 0,
                    },
                    &mut receipt,
                )
            },
            u32::from(Status::Closed.code())
        );
        assert_eq!(
            flark_v3_endpoint_remove(handle),
            u32::from(Status::Closed.code())
        );
    }

    #[test]
    fn opaque_native_finalizer_token_contains_no_endpoint_pointer() {
        let handle = create();
        let mut token = std::ptr::null_mut();
        // SAFETY: valid unique token output pointer.
        assert_eq!(
            unsafe { flark_v3_endpoint_finalizer_token_create(handle, &mut token) },
            ok()
        );
        assert!(!token.is_null());
        // SAFETY: token has exact provenance and is consumed once.
        unsafe { flark_v3_endpoint_emergency_finalize(token) };

        let mut receipt = FlarkV3PollReceipt::default();
        // SAFETY: valid bounded fuel/output.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: 1,
                        maximum_checkpoints: 1,
                        maximum_retirement_transitions: 0,
                    },
                    &mut receipt,
                )
            },
            u32::from(Status::Closed.code())
        );

        let handle = create();
        let mut detached = std::ptr::null_mut();
        // SAFETY: valid unique token output pointer.
        assert_eq!(
            unsafe { flark_v3_endpoint_finalizer_token_create(handle, &mut detached) },
            ok()
        );
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
        // SAFETY: the detached token has exact provenance and is consumed once.
        assert_eq!(
            unsafe { flark_v3_endpoint_finalizer_token_release(detached) },
            ok()
        );
        // SAFETY: null is an explicit no-op for the callback trampoline.
        unsafe { flark_v3_endpoint_emergency_finalize(std::ptr::null_mut()) };
    }

    #[test]
    fn racing_finalizer_never_recycles_an_endpoint_owned_by_an_active_call() {
        let handle = create();
        let mut token = std::ptr::null_mut();
        // SAFETY: valid unique token output pointer.
        assert_eq!(
            unsafe { flark_v3_endpoint_finalizer_token_create(handle, &mut token) },
            ok()
        );
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let thread_entered = Arc::clone(&entered);
        let thread_release = Arc::clone(&release);
        let registry_handle = EndpointHandle::from(handle);
        let in_flight = thread::spawn(move || {
            registry()
                .with_endpoint(registry_handle, |_| {
                    thread_entered.wait();
                    thread_release.wait();
                })
                .unwrap();
        });
        entered.wait();

        // SAFETY: token has exact provenance and is consumed once. The
        // intentionally racing callback must fail closed because the endpoint
        // Arc is still leased by `with_endpoint`.
        unsafe { flark_v3_endpoint_emergency_finalize(token) };
        release.wait();
        in_flight.join().unwrap();

        let mut receipt = FlarkV3PollReceipt::default();
        // SAFETY: valid bounded fuel/output. The live result proves the slot
        // was not recycled under the active operation.
        assert_eq!(
            unsafe {
                flark_v3_endpoint_poll(
                    handle,
                    FlarkV3PollFuel {
                        maximum_source_bytes: 1,
                        maximum_checkpoints: 1,
                        maximum_retirement_transitions: 0,
                    },
                    &mut receipt,
                )
            },
            ok()
        );
        assert_eq!(flark_v3_endpoint_emergency_destroy(handle), ok());
    }

    #[test]
    fn header_and_rust_export_guards_cover_every_native_symbol() {
        let header = include_str!("../flark_comrak_bridge.h");
        for symbol in [
            "flark_v3_checkpoint_b_probe",
            "flark_v3_endpoint_native_abi_version",
            "flark_v3_endpoint_config_standard",
            "flark_v3_endpoint_create",
            "flark_v3_endpoint_recover",
            "flark_v3_endpoint_dispatch",
            "flark_v3_endpoint_dispatch_host_poll",
            "flark_v3_endpoint_dispatch_inline_sidecar_host_poll",
            "flark_v3_endpoint_dispatch_viewport_presentation_host_poll",
            "flark_v3_endpoint_poll",
            "flark_v3_endpoint_poll_candidate",
            "flark_v3_endpoint_encode",
            "flark_v3_endpoint_close",
            "flark_v3_endpoint_remove",
            "flark_v3_endpoint_emergency_destroy",
            "flark_v3_endpoint_finalizer_token_create",
            "flark_v3_endpoint_finalizer_token_release",
            "flark_v3_endpoint_emergency_finalize",
        ] {
            assert!(
                header.contains(symbol),
                "missing C declaration for {symbol}"
            );
        }
        assert_eq!(size_of::<FlarkV3EndpointHandle>(), 8);
        assert_eq!(align_of::<FlarkV3EndpointHandle>(), 4);
        assert!(header.contains("FLARK_V3_ENDPOINT_NATIVE_ABI_VERSION UINT32_C(0x00020002)"));
        assert!(header.contains("uint32_t source_fact_transitions;"));
        assert!(header.contains("FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES UINT32_C(65536)"));
    }

    #[test]
    fn checkpoint_b_probe_exports_bounded_valid_json() {
        let mut output = vec![0_u8; FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES as usize];
        let mut written = 0_u32;
        // SAFETY: the output and written-count regions are valid, unique, and
        // bounded by the declared diagnostic maximum.
        assert_eq!(
            unsafe {
                flark_v3_checkpoint_b_probe(
                    output.as_mut_ptr(),
                    FLARK_V3_CHECKPOINT_B_MAXIMUM_JSON_BYTES,
                    &mut written,
                )
            },
            ok()
        );
        let receipt: serde_json::Value =
            serde_json::from_slice(&output[..written as usize]).unwrap();
        assert_eq!(receipt["schema"], 1);
        assert_eq!(receipt["platform"], "native");
        assert_eq!(receipt["allChecksPassed"], true);

        let mut rejected_written = u32::MAX;
        // SAFETY: a null zero-capacity output is valid for checking that an
        // undersized call fails without exposing a partial receipt.
        assert_eq!(
            unsafe { flark_v3_checkpoint_b_probe(std::ptr::null_mut(), 0, &mut rejected_written,) },
            u32::from(Status::ForegroundBoundExceeded.code())
        );
        assert_eq!(rejected_written, 0);
    }

    #[test]
    fn registry_limits_match_native_admission_and_recovery_contract() {
        assert_eq!(MAXIMUM_RESIDENT_ENDPOINT_SLOTS, 4_096);
        assert_eq!(MAXIMUM_FRESH_ENDPOINTS, 2_048);
        let header = include_str!("../flark_comrak_bridge.h");
        assert!(header.contains("FLARK_V3_ENDPOINT_MAXIMUM_FRESH_ENDPOINTS UINT32_C(2048)"));
        assert!(header.contains("FLARK_V3_ENDPOINT_MAXIMUM_RESIDENT_SLOTS UINT32_C(4096)"));
    }
}

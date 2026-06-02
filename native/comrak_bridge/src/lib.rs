mod abi;
mod marker_mapping;
mod parser;
mod payload;
mod source_ranges;

use abi::{
    allocate_response, free_response, FlarkComrakResponse, ABI_VERSION, STATUS_ERROR, STATUS_OK,
};
use parser::parse_to_payload;
use payload::diagnostic_payload;

#[no_mangle]
pub extern "C" fn flark_comrak_bridge_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn flark_comrak_input_alloc(len: u32) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }

    let mut bytes = Vec::<u8>::with_capacity(len as usize);
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

#[no_mangle]
pub extern "C" fn flark_comrak_input_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() || len == 0 {
        return;
    }

    // SAFETY: pointer/capacity originate from `flark_comrak_input_alloc`.
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, len as usize);
    }
}

#[no_mangle]
pub extern "C" fn flark_comrak_parse(
    revision: u32,
    profile: u8,
    text_ptr: *const u8,
    text_len: u32,
) -> *mut FlarkComrakResponse {
    if profile > 1 {
        return allocate_response(
            revision,
            STATUS_ERROR,
            diagnostic_payload("Unsupported profile in comrak bridge."),
        );
    }

    if text_len > 0 && text_ptr.is_null() {
        return allocate_response(
            revision,
            STATUS_ERROR,
            diagnostic_payload("Received null text pointer with non-zero length."),
        );
    }

    let input_bytes = if text_ptr.is_null() || text_len == 0 {
        &[][..]
    } else {
        // SAFETY: pointer/length are validated above and only read for this call.
        unsafe { std::slice::from_raw_parts(text_ptr, text_len as usize) }
    };

    let text = match std::str::from_utf8(input_bytes) {
        Ok(text) => text,
        Err(_) => {
            return allocate_response(
                revision,
                STATUS_ERROR,
                diagnostic_payload("Invalid UTF-8 input."),
            )
        }
    };

    match parse_to_payload(text, profile) {
        Ok(payload) => allocate_response(revision, STATUS_OK, payload),
        Err(message) => allocate_response(revision, STATUS_ERROR, diagnostic_payload(&message)),
    }
}

#[no_mangle]
pub extern "C" fn flark_comrak_response_free(response_ptr: *mut FlarkComrakResponse) {
    free_response(response_ptr);
}

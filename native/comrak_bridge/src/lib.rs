mod abi;
mod marker_mapping;
mod parser;
mod payload;
mod reference_definitions;
mod source_ranges;

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

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
    // A panic must never unwind across the `extern "C"` boundary: that is
    // undefined behavior and aborts the whole host application. The range
    // mapping does careful byte-offset arithmetic over comrak sourcepos data,
    // and comrak itself is third-party code, so any residual or future panic
    // (a mis-slice, an unexpected offset, an allocation the parser makes) is
    // caught here and surfaced as an ordinary STATUS_ERROR response. The Dart
    // backend already routes STATUS_ERROR to its parse-error callback and
    // recovers, so a bad document degrades to "not parsed this frame" instead
    // of crashing the app. AssertUnwindSafe is sound because the only captured
    // state is the caller-owned input buffer, which we merely read.
    let parsed = catch_unwind(AssertUnwindSafe(|| {
        parse_current_revision(revision, profile, text_ptr, text_len)
    }));
    match parsed {
        Ok(response) => response,
        Err(panic) => allocate_response(
            revision,
            STATUS_ERROR,
            diagnostic_payload(&format!(
                "comrak bridge panicked while parsing: {}",
                panic_message(panic.as_ref()),
            )),
        ),
    }
}

fn parse_current_revision(
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

fn panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A panic raised inside the parse boundary must be caught and reported as
    /// STATUS_ERROR, never allowed to unwind across the FFI edge and abort the
    /// host. This exercises the same `catch_unwind` wrapper the real entry
    /// point uses.
    #[test]
    fn catch_unwind_converts_a_panic_into_an_error_response() {
        let parsed = catch_unwind(AssertUnwindSafe(|| -> *mut FlarkComrakResponse {
            panic!("synthetic parse failure");
        }));
        let response_ptr = match parsed {
            Ok(response) => response,
            Err(panic) => allocate_response(
                7,
                STATUS_ERROR,
                diagnostic_payload(&format!(
                    "comrak bridge panicked while parsing: {}",
                    panic_message(panic.as_ref()),
                )),
            ),
        };
        assert!(!response_ptr.is_null());
        // SAFETY: the pointer originates from allocate_response above.
        let response = unsafe { &*response_ptr };
        assert_eq!(response.status_code, STATUS_ERROR);
        assert_eq!(response.revision, 7);
        assert_eq!(response.abi_version, ABI_VERSION);
        flark_comrak_response_free(response_ptr);
    }

    /// A well-formed document still parses to a STATUS_OK response with a
    /// non-empty payload after the wrapper change.
    #[test]
    fn normal_document_parses_to_ok() {
        let text = "# Title\n\nHello **world**.\n";
        let response_ptr =
            flark_comrak_parse(1, 1, text.as_ptr(), text.len() as u32);
        assert!(!response_ptr.is_null());
        // SAFETY: the pointer originates from flark_comrak_parse.
        let response = unsafe { &*response_ptr };
        assert_eq!(response.status_code, STATUS_OK);
        assert!(response.payload_len > 0);
        flark_comrak_response_free(response_ptr);
    }
}

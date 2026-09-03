//! flark_parse: unmodified comrak plus a flat render-model extraction, with a
//! three-function C ABI shared by the native (FFI) and wasm32 transports.
pub mod lines;
pub mod model;
pub mod reference_definitions;
pub mod schema;

use std::slice;

/// Parse `len` UTF-8 bytes at `src` and hand back a freshly allocated render
/// model in `*out` / `*out_len`. Returns 0 on success, 1 for a null argument,
/// 2 for invalid UTF-8, and 3 when the extraction panicked (contained).
#[no_mangle]
pub extern "C" fn flark_parse(src: *const u8, len: u32, out: *mut *mut u8, out_len: *mut u32) -> i32 {
    if src.is_null() || out.is_null() || out_len.is_null() { return 1; }
    let bytes = unsafe { slice::from_raw_parts(src, len as usize) };
    let Ok(text) = std::str::from_utf8(bytes) else { return 2; };
    match std::panic::catch_unwind(|| model::Extractor::extract(text)) {
        Ok(buf) => {
            let mut buf = buf.into_boxed_slice();
            let ptr = buf.as_mut_ptr();
            let n = buf.len() as u32;
            std::mem::forget(buf);
            unsafe { *out = ptr; *out_len = n; }
            0
        }
        Err(_) => 3,
    }
}

/// Allocate `len` zeroed bytes the caller may fill with source text.
#[no_mangle]
pub extern "C" fn flark_parse_alloc(len: u32) -> *mut u8 {
    let mut v = vec![0u8; len as usize].into_boxed_slice();
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Free a buffer returned by `flark_parse` or `flark_parse_alloc`.
#[no_mangle]
pub extern "C" fn flark_parse_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(slice::from_raw_parts_mut(ptr, len as usize))); }
}

/// Render-model schema version this library writes.
#[no_mangle]
pub extern "C" fn flark_parse_schema_version() -> u32 { schema::VERSION }

//! flark_parse: unmodified comrak plus a flat render-model extraction, with a
//! three-function C ABI shared by the native (FFI) and wasm32 transports.
pub mod lines;
pub mod model;
pub mod reference_definitions;
pub mod schema;
mod text_pieces;

use std::slice;

/// Parse `len` UTF-8 bytes at `src` and hand back a freshly allocated render
/// model in `*out` / `*out_len` (bytes). A null `src` with `len == 0` is the
/// empty document. Returns 0 on success, 1 for a null output argument or a
/// null source with a non-zero length, 2 for invalid UTF-8, and 3 when the
/// extraction panicked.
///
/// Panic containment is native-only: `wasm32-unknown-unknown` aborts on
/// panic, so on the web a panic traps out of this call and the host must
/// discard the instance and re-instantiate the module.
#[no_mangle]
pub extern "C" fn flark_parse(src: *const u8, len: u32, out: *mut *mut u8, out_len: *mut u32) -> i32 {
    if out.is_null() || out_len.is_null() { return 1; }
    if src.is_null() && len != 0 { return 1; }
    let bytes: &[u8] = if src.is_null() { &[] } else { unsafe { slice::from_raw_parts(src, len as usize) } };
    let Ok(text) = std::str::from_utf8(bytes) else { return 2; };
    match std::panic::catch_unwind(|| model::Extractor::extract(text)) {
        Ok(words) => {
            let mut words = words.into_boxed_slice();
            let ptr = words.as_mut_ptr() as *mut u8;
            let n = (words.len() * 4) as u32;
            std::mem::forget(words);
            unsafe { *out = ptr; *out_len = n; }
            0
        }
        Err(_) => 3,
    }
}

/// Allocate `len` zeroed bytes (rounded up to whole 32-bit words, so the
/// pointer is 4-byte aligned) for source text or an out-parameter cell.
#[no_mangle]
pub extern "C" fn flark_parse_alloc(len: u32) -> *mut u8 {
    let mut v = vec![0u32; (len as usize + 3) / 4].into_boxed_slice();
    let p = v.as_mut_ptr() as *mut u8;
    std::mem::forget(v);
    p
}

/// Free a buffer returned by `flark_parse` or `flark_parse_alloc`, passing
/// the same `len` that produced it. Every buffer is a whole-word allocation.
#[no_mangle]
pub extern "C" fn flark_parse_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(slice::from_raw_parts_mut(ptr as *mut u32, (len as usize + 3) / 4))); }
}

/// Render-model schema version this library writes.
#[no_mangle]
pub extern "C" fn flark_parse_schema_version() -> u32 { schema::VERSION }

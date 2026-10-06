//! flark_parse: unmodified comrak plus a flat render-model extraction, with a
//! three-function C ABI shared by the native (FFI) and wasm32 transports.
pub mod lines;
pub mod model;
pub mod records;
pub mod reference_definitions;
pub mod schema;
mod text_pieces;

use std::slice;

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub const PARSE_OK: i32 = 0;
pub const PARSE_INVALID_ARGUMENT: i32 = 1;
pub const PARSE_INVALID_UTF8: i32 = 2;
pub const PARSE_PANIC: i32 = 3;
pub const PARSE_EXTRACTION_DEVIATION: i32 = 4;

/// Parse `len` UTF-8 bytes at `src` and hand back a freshly allocated render
/// model in `*out` / `*out_len` (bytes). A null `src` with `len == 0` is the
/// empty document. Returns 0 on success, 1 for a null output argument or a
/// null source with a non-zero length, 2 for invalid UTF-8, 3 when the
/// extraction panicked, and 4 when it refused to publish: a derived range or
/// value failed validation, a line holds more possible email addresses than
/// comrak can link without overflowing the stack
/// (`model::MAX_EMAIL_AUTOLINKS_PER_LINE`), tables could gain more cells
/// than comrak creates in bounded time and memory
/// (`model::MAX_FILLED_TABLE_CELLS`), or the model would not fit the 32-bit
/// `out_len`.
///
/// Panic containment is native-only: `wasm32-unknown-unknown` aborts on
/// panic, so on the web a panic traps out of this call and the host must
/// discard the instance and re-instantiate the module.
#[no_mangle]
pub extern "C" fn flark_parse(src: *const u8, len: u32, out: *mut *mut u8, out_len: *mut u32) -> i32 {
    if out.is_null() || out_len.is_null() { return PARSE_INVALID_ARGUMENT; }
    unsafe { *out = std::ptr::null_mut(); *out_len = 0; }
    if src.is_null() && len != 0 { return PARSE_INVALID_ARGUMENT; }
    let bytes: &[u8] = if src.is_null() { &[] } else { unsafe { slice::from_raw_parts(src, len as usize) } };
    let Ok(text) = std::str::from_utf8(bytes) else { return PARSE_INVALID_UTF8; };
    match std::panic::catch_unwind(|| model::Extractor::extract(text)) {
        Ok(Ok(words)) => {
            // Truncating the length would hand the host a prefix of the model
            // and a size that no longer frees the allocation.
            let Some(n) = words.len().checked_mul(4).and_then(|n| u32::try_from(n).ok()) else { return PARSE_EXTRACTION_DEVIATION; };
            let mut words = words.into_boxed_slice();
            let ptr = words.as_mut_ptr() as *mut u8;
            std::mem::forget(words);
            unsafe { *out = ptr; *out_len = n; }
            PARSE_OK
        }
        Ok(Err(_)) => PARSE_EXTRACTION_DEVIATION,
        Err(_) => PARSE_PANIC,
    }
}

/// Allocate `len` zeroed bytes (rounded up to whole 32-bit words, so the
/// pointer is 4-byte aligned) for source text or an out-parameter cell. The
/// word count is taken in 32 bits: `len + 3` in a 32-bit `usize` (wasm32)
/// wrapped a size near `u32::MAX` to an empty allocation.
#[no_mangle]
pub extern "C" fn flark_parse_alloc(len: u32) -> *mut u8 {
    let mut v = vec![0u32; len.div_ceil(4) as usize].into_boxed_slice();
    let p = v.as_mut_ptr() as *mut u8;
    std::mem::forget(v);
    p
}

/// Free a buffer returned by `flark_parse` or `flark_parse_alloc`, passing
/// the same `len` that produced it. Every buffer is a whole-word allocation.
#[no_mangle]
pub extern "C" fn flark_parse_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(slice::from_raw_parts_mut(ptr as *mut u32, len.div_ceil(4) as usize))); }
}

/// Render-model schema version this library writes.
#[no_mangle]
pub extern "C" fn flark_parse_schema_version() -> u32 { schema::VERSION }

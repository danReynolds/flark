pub mod model;

use std::slice;

/// Parse `len` bytes at `src` and write the render model to a freshly allocated
/// buffer. Returns 0 on success; `*out`/`*out_len` receive the buffer.
#[no_mangle]
pub extern "C" fn flark_spike_parse(src: *const u8, len: u32, out: *mut *mut u8, out_len: *mut u32) -> i32 {
    if src.is_null() || out.is_null() || out_len.is_null() { return 1; }
    let bytes = unsafe { slice::from_raw_parts(src, len as usize) };
    let Ok(text) = std::str::from_utf8(bytes) else { return 2; };
    let result = std::panic::catch_unwind(|| model::Extractor::extract(text, false));
    match result {
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

#[no_mangle]
pub extern "C" fn flark_spike_alloc(len: u32) -> *mut u8 {
    let mut v = vec![0u8; len as usize].into_boxed_slice();
    let p = v.as_mut_ptr(); std::mem::forget(v); p
}

#[no_mangle]
pub extern "C" fn flark_spike_free(ptr: *mut u8, len: u32) {
    if ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(slice::from_raw_parts_mut(ptr, len as usize))); }
}

#[no_mangle]
pub extern "C" fn flark_spike_version() -> u32 { model::VERSION }

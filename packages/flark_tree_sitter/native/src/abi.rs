//! The caller owns all buffers and must pass their original lengths to free.
use std::slice;

#[no_mangle]
pub extern "C" fn flark_tree_sitter_version() -> u32 {
    crate::VERSION
}

#[no_mangle]
pub extern "C" fn flark_tree_sitter_alloc(len: u32) -> *mut u8 {
    let mut words = vec![0u64; (len as usize).div_ceil(8)].into_boxed_slice();
    let ptr = words.as_mut_ptr().cast();
    std::mem::forget(words);
    ptr
}

/// # Safety
/// `ptr` must be a live allocation from this ABI, with its original `len`.
#[no_mangle]
pub unsafe extern "C" fn flark_tree_sitter_free(ptr: *mut u8, len: u32) {
    if !ptr.is_null() {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                ptr.cast::<u64>(),
                (len as usize).div_ceil(8),
            )));
        }
    }
}

/// Writes UTF-8 JSON, versioned independently from Flark's Markdown schema.
/// Returns 0=success, 1=argument, 2=UTF-8, 3=engine, 4=native panic, 5=byte limit.
///
/// # Safety
/// Inputs must describe live buffers; output cells must be writable and aligned.
/// A null input with zero length is accepted. On any error output cells are zero.
#[no_mangle]
pub unsafe extern "C" fn flark_tree_sitter_analyze(
    src: *const u8,
    len: u32,
    language: u32,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> i32 {
    unsafe { exchange(src, len, language, out, out_len, 0) }
}

/// Propose a snippet edit from a UTF-8 JSON request. Same ownership/error ABI.
/// # Safety
/// Inputs must describe live buffers; output cells must be writable and aligned.
#[no_mangle]
pub unsafe extern "C" fn flark_tree_sitter_edit(
    src: *const u8,
    len: u32,
    language: u32,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> i32 {
    unsafe { exchange(src, len, language, out, out_len, 1) }
}

/// Infer a language from at most 128 UTF-16 units. Uses the same buffer ABI.
/// # Safety
/// Inputs/outputs must be live buffers as for `flark_tree_sitter_analyze`.
#[no_mangle]
pub unsafe extern "C" fn flark_tree_sitter_detect(
    src: *const u8,
    len: u32,
    _language: u32,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> i32 {
    unsafe { exchange(src, len, 0, out, out_len, 2) }
}

unsafe fn exchange(
    src: *const u8,
    len: u32,
    language: u32,
    out: *mut *mut u8,
    out_len: *mut u32,
    operation: u8,
) -> i32 {
    if out.is_null() || out_len.is_null() {
        return 1;
    }
    unsafe {
        *out = std::ptr::null_mut();
        *out_len = 0;
    }
    if len as usize
        > if operation == 1 {
            crate::MAX_UTF16 * 12 + 1024
        } else {
            crate::MAX_UTF16 * 4
        }
    {
        return 5;
    }
    if src.is_null() && len != 0 {
        return 1;
    }
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(src, len as usize) }
    };
    let Ok(source) = std::str::from_utf8(bytes) else {
        return 2;
    };
    match std::panic::catch_unwind(|| {
        if operation == 1 {
            serde_json::from_str::<crate::edit::Request>(source)
                .map_err(|e| e.to_string())
                .and_then(|r| crate::edit::propose(r, language))
                .and_then(|a| serde_json::to_vec(&a).map_err(|e| e.to_string()))
        } else if operation == 2 {
            crate::detect::detect(source).and_then(|language| {
                serde_json::to_vec(
                    &serde_json::json!({"version":crate::VERSION,"language":language}),
                )
                .map_err(|e| e.to_string())
            })
        } else {
            crate::analyze(source, language).and_then(|a| a.encode())
        }
    }) {
        Ok(Ok(bytes)) => {
            let len = bytes.len() as u32;
            let ptr = flark_tree_sitter_alloc(len);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
                *out = ptr;
                *out_len = len;
            }
            0
        }
        Ok(Err(_)) => 3,
        Err(_) => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_validates_inputs_and_clears_outputs() {
        unsafe {
            let mut out = std::ptr::null_mut();
            let mut len = 999;
            assert_eq!(
                flark_tree_sitter_analyze(std::ptr::null(), 1, 1, &mut out, &mut len),
                1
            );
            assert_eq!(len, 0);
            assert!(out.is_null());
            assert_eq!(
                flark_tree_sitter_analyze([255].as_ptr(), 1, 1, &mut out, &mut len),
                2
            );
            assert_eq!(
                flark_tree_sitter_analyze(b"x".as_ptr(), 1, 99, &mut out, &mut len),
                3
            );
            assert_eq!(
                flark_tree_sitter_analyze(std::ptr::null(), 0, 1, &mut out, &mut len),
                0
            );
            let value: serde_json::Value =
                serde_json::from_slice(slice::from_raw_parts(out, len as usize)).unwrap();
            assert_eq!(value["version"], crate::VERSION);
            flark_tree_sitter_free(out, len);
        }
    }
}

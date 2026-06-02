pub(crate) const ABI_VERSION: u32 = 1;
pub(crate) const STATUS_OK: u16 = 0;
pub(crate) const STATUS_ERROR: u16 = 1;

#[repr(C)]
pub struct FlarkComrakResponse {
    pub abi_version: u32,
    pub revision: u32,
    pub status_code: u16,
    pub reserved: u16,
    pub payload_ptr: *mut u8,
    pub payload_len: u32,
}

pub(crate) fn allocate_response(
    revision: u32,
    status_code: u16,
    payload: Vec<u8>,
) -> *mut FlarkComrakResponse {
    let payload_len = payload.len() as u32;
    let payload_ptr = if payload.is_empty() {
        std::ptr::null_mut()
    } else {
        let payload = payload.into_boxed_slice();
        Box::into_raw(payload) as *mut u8
    };
    let payload_len = if payload_ptr.is_null() {
        0
    } else {
        payload_len
    };

    let response = FlarkComrakResponse {
        abi_version: ABI_VERSION,
        revision,
        status_code,
        reserved: 0,
        payload_ptr,
        payload_len,
    };
    Box::into_raw(Box::new(response))
}

pub(crate) fn free_response(response_ptr: *mut FlarkComrakResponse) {
    if response_ptr.is_null() {
        return;
    }

    // SAFETY: Caller guarantees pointer originates from `sovereign_comrak_parse`.
    let response = unsafe { Box::from_raw(response_ptr) };

    if !response.payload_ptr.is_null() && response.payload_len > 0 {
        // SAFETY: payload pointer/len originate from `allocate_response`.
        unsafe {
            let payload = std::ptr::slice_from_raw_parts_mut(
                response.payload_ptr,
                response.payload_len as usize,
            );
            let _ = Box::from_raw(payload);
        }
    }
}

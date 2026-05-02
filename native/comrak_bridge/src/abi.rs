pub(crate) const ABI_VERSION: u32 = 1;
pub(crate) const STATUS_OK: u16 = 0;
pub(crate) const STATUS_ERROR: u16 = 1;

#[repr(C)]
pub struct SovereignComrakResponse {
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
    mut payload: Vec<u8>,
) -> *mut SovereignComrakResponse {
    let payload_len = payload.len() as u32;
    let payload_ptr = if payload.is_empty() {
        std::ptr::null_mut()
    } else {
        let ptr = payload.as_mut_ptr();
        std::mem::forget(payload);
        ptr
    };
    let payload_len = if payload_ptr.is_null() {
        0
    } else {
        payload_len
    };

    let response = SovereignComrakResponse {
        abi_version: ABI_VERSION,
        revision,
        status_code,
        reserved: 0,
        payload_ptr,
        payload_len,
    };
    Box::into_raw(Box::new(response))
}

pub(crate) fn free_response(response_ptr: *mut SovereignComrakResponse) {
    if response_ptr.is_null() {
        return;
    }

    // SAFETY: Caller guarantees pointer originates from `sovereign_comrak_parse`.
    let response = unsafe { Box::from_raw(response_ptr) };

    if !response.payload_ptr.is_null() && response.payload_len > 0 {
        // SAFETY: payload pointer/len originate from `allocate_response`.
        unsafe {
            let _ = Vec::from_raw_parts(
                response.payload_ptr,
                response.payload_len as usize,
                response.payload_len as usize,
            );
        }
    }
}

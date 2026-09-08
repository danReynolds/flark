use flark_parse::schema::{self, header};
use flark_parse::{
    flark_parse, flark_parse_free, flark_parse_schema_version, PARSE_EXTRACTION_DEVIATION, PARSE_OK,
};

#[test]
fn abi_and_encoded_header_publish_schema_v4() {
    assert_eq!(flark_parse_schema_version(), 4);

    let src = b"**ok**";
    let mut out = std::ptr::null_mut();
    let mut out_len = 0;
    let rc = flark_parse(src.as_ptr(), src.len() as u32, &mut out, &mut out_len);
    assert_eq!(rc, PARSE_OK);
    assert!(!out.is_null());
    assert_eq!(out_len as usize % std::mem::size_of::<u32>(), 0);

    let words = unsafe {
        std::slice::from_raw_parts(
            out.cast::<u32>(),
            out_len as usize / std::mem::size_of::<u32>(),
        )
    };
    assert_eq!(words[header::MAGIC], schema::MAGIC);
    assert_eq!(words[header::VERSION], 4);
    flark_parse_free(out, out_len);
}

#[test]
fn abi_returns_a_typed_error_and_no_model_for_extraction_deviations() {
    // The packed schema supports alignment for at most 16 columns.
    let src = "| a ".repeat(17) + "|\n" + &"|---".repeat(16) + "|-:|\n";
    let mut out = 1usize as *mut u8;
    let mut out_len = u32::MAX;
    let rc = flark_parse(src.as_ptr(), src.len() as u32, &mut out, &mut out_len);
    assert_eq!(rc, PARSE_EXTRACTION_DEVIATION);
    assert!(out.is_null());
    assert_eq!(out_len, 0);
}

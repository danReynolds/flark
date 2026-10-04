use flark_parse::schema::{self, header};
use flark_parse::{
    flark_parse, flark_parse_free, flark_parse_schema_version, model, PARSE_EXTRACTION_DEVIATION, PARSE_INVALID_ARGUMENT,
    PARSE_INVALID_UTF8, PARSE_OK,
};

#[test]
fn abi_and_encoded_header_publish_schema_v5() {
    assert_eq!(flark_parse_schema_version(), 5);

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
    assert_eq!(words[header::VERSION], 5);
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

#[test]
fn abi_refuses_a_line_of_more_addresses_than_comrak_links_safely() {
    // Refused before comrak parses: linking them recursively could overflow
    // the caller's stack, which no return code could report.
    let src = "a@b.c ".repeat(model::MAX_EMAIL_AUTOLINKS_PER_LINE + 1);
    let mut out = 1usize as *mut u8;
    let mut out_len = u32::MAX;
    let rc = flark_parse(src.as_ptr(), src.len() as u32, &mut out, &mut out_len);
    assert_eq!(rc, PARSE_EXTRACTION_DEVIATION);
    assert!(out.is_null());
    assert_eq!(out_len, 0);
}

#[test]
fn abi_refuses_tables_that_could_gain_more_cells_than_comrak_creates_safely() {
    // Refused before comrak parses: it would create a cell for every column
    // every row lacks, here 4.2 million, without the cap it means to apply.
    let src = "a|".repeat(2048) + "\n" + &"-|".repeat(2048) + &"\nx".repeat(2046);
    assert!(model::filled_table_cells(&src) > model::MAX_FILLED_TABLE_CELLS);
    let mut out = 1usize as *mut u8;
    let mut out_len = u32::MAX;
    let rc = flark_parse(src.as_ptr(), src.len() as u32, &mut out, &mut out_len);
    assert_eq!(rc, PARSE_EXTRACTION_DEVIATION);
    assert!(out.is_null());
    assert_eq!(out_len, 0);
}

#[test]
fn abi_refuses_bytes_that_are_not_utf8_and_missing_arguments() {
    // A host hands the parser bytes: surrogates encoded on their own (what a
    // lone UTF-16 surrogate becomes in CESU-8), overlong forms, stray
    // continuation bytes, sequences cut short and bytes UTF-8 never holds
    // are refused with no model, wherever they fall.
    let bad: [&[u8]; 9] = [b"\xed\xa0\x80", b"a\xed\xbf\xbfb", b"\xc0\xaf", b"\xe0\x80\xaf", b"\x80", b"*a* \xbf", b"| a |\n|---|\n| \xe2\x82 |\n", b"\xf4\x90\x80\x80", b"\xff"];
    for bytes in bad {
        let mut out = 1usize as *mut u8;
        let mut out_len = u32::MAX;
        assert_eq!(flark_parse(bytes.as_ptr(), bytes.len() as u32, &mut out, &mut out_len), PARSE_INVALID_UTF8, "{bytes:?}");
        assert!(out.is_null());
        assert_eq!(out_len, 0);
    }
    let (mut out, mut out_len) = (std::ptr::null_mut(), 0u32);
    assert_eq!(flark_parse(std::ptr::null(), 1, &mut out, &mut out_len), PARSE_INVALID_ARGUMENT);
    assert_eq!(flark_parse(b"x".as_ptr(), 1, std::ptr::null_mut(), &mut out_len), PARSE_INVALID_ARGUMENT);
    assert_eq!(flark_parse(b"x".as_ptr(), 1, &mut out, std::ptr::null_mut()), PARSE_INVALID_ARGUMENT);
    // No source and no length is the empty document.
    assert_eq!(flark_parse(std::ptr::null(), 0, &mut out, &mut out_len), PARSE_OK);
    flark_parse_free(out, out_len);
}

use std::path::Path;

#[test]
fn annotations_disabled_full_parser_is_byte_exact_to_pristine_comrak() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test/fixtures/commonmark/upstream");
    let mut compared = 0;
    for (filename, gfm) in [("common_mark_tests.json", false), ("gfm_tests.json", true)] {
        let fixtures: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(filename)).unwrap()).unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let source = fixture["markdown"].as_str().unwrap();
            let patched = patched_options(gfm);
            let pristine = pristine_options(gfm);
            assert_eq!(
                comrak::markdown_to_html(source, &patched),
                comrak_pristine::markdown_to_html(source, &pristine),
                "HTML mismatch in {filename} example {}",
                fixture["example"]
            );
            assert_eq!(
                comrak::markdown_to_commonmark_xml(source, &patched),
                comrak_pristine::markdown_to_commonmark_xml(source, &pristine),
                "AST/sourcepos mismatch in {filename} example {}",
                fixture["example"]
            );
            compared += 1;
        }
    }
    eprintln!("pristine full-parser differentials: {compared}");
}

fn patched_options(gfm: bool) -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    if gfm {
        options.extension.strikethrough = true;
        options.extension.tagfilter = true;
        options.extension.table = true;
        options.extension.autolink = true;
        options.extension.tasklist = true;
    }
    options
}

fn pristine_options(gfm: bool) -> comrak_pristine::Options<'static> {
    let mut options = comrak_pristine::Options::default();
    if gfm {
        options.extension.strikethrough = true;
        options.extension.tagfilter = true;
        options.extension.table = true;
        options.extension.autolink = true;
        options.extension.tasklist = true;
    }
    options
}

use flark_runtime::{DocumentSession, DocumentSessionError};

fn source(document: &DocumentSession) -> String {
    String::from_utf8(
        document
            .source_bytes(0..document.source_byte_len())
            .expect("source bytes"),
    )
    .expect("UTF-8 source")
}

#[test]
fn literal_transaction_commits_exact_source_and_result_coordinates() {
    let mut document = DocumentSession::begin("a🌍bc\n").expect("begin document");
    let receipt = document
        .apply_source_transaction_v1(1, 1..3, "éx", 1, 3)
        .expect("replace non-BMP scalar");

    assert_eq!(receipt.base_revision, 1);
    assert_eq!(receipt.result_revision, 2);
    assert_eq!(receipt.committed_splice.base_byte_range, 1..5);
    assert_eq!(receipt.committed_splice.base_utf16_range, 1..3);
    assert_eq!(receipt.committed_splice.result_byte_range, 1..4);
    assert_eq!(receipt.committed_splice.result_utf16_range, 1..3);
    assert_eq!(receipt.inverse, "🌍".as_bytes());
    assert_eq!(receipt.result_selection_base_utf16, 1);
    assert_eq!(receipt.result_selection_extent_utf16, 3);
    assert_eq!(receipt.result_selection_base_byte, 1);
    assert_eq!(receipt.result_selection_extent_byte, 4);
    assert_eq!(receipt.result_source_byte_length, "aéxbc\n".len());
    assert_eq!(
        receipt.result_source_utf16_length,
        "aéxbc\n".encode_utf16().count()
    );
    assert_eq!(source(&document), "aéxbc\n");
    document.close().expect("close document");
}

#[test]
fn result_selection_after_splice_is_mapped_before_commit() {
    let mut document = DocumentSession::begin("ab🌍cd").expect("begin document");
    let receipt = document
        .apply_source_transaction_v1(1, 0..1, "XYZ", 6, 7)
        .expect("replace prefix");

    assert_eq!(source(&document), "XYZb🌍cd");
    assert_eq!(receipt.result_selection_base_utf16, 6);
    assert_eq!(receipt.result_selection_extent_utf16, 7);
    assert_eq!(receipt.result_selection_base_byte, 8);
    assert_eq!(receipt.result_selection_extent_byte, 9);
    document.close().expect("close document");
}

#[test]
fn invalid_result_selection_never_mutates() {
    let mut document = DocumentSession::begin("abc").expect("begin document");
    let error = document
        .apply_source_transaction_v1(1, 1..1, "🌍", 2, 2)
        .expect_err("selection inside surrogate pair must fail");

    assert!(matches!(error, DocumentSessionError::RangeOutOfBounds));
    assert_eq!(document.revision(), 1);
    assert_eq!(source(&document), "abc");
    document.close().expect("close document");
}

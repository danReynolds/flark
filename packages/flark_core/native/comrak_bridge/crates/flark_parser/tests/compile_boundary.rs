const MANIFEST: &str = include_str!("../Cargo.toml");
const WORKSPACE_MANIFEST: &str = include_str!("../../../Cargo.toml");
const PUBLIC_ROOT: &str = include_str!("../src/lib.rs");
const CONTROLLER: &str = include_str!("../src/exact_clean.rs");

#[test]
fn production_crate_has_one_pinned_lexical_donor_and_no_research_dependency() {
    assert!(MANIFEST.contains("comrak = { version = \"=0.54.0\", default-features = false }"));
    assert!(
        WORKSPACE_MANIFEST.contains("comrak = { path = \"crates/flark_parser/vendor/comrak\" }")
    );
    assert!(MANIFEST.contains(
        "flark-engine = { path = \"../flark_engine\", features = [\"parser-internal\"] }"
    ));
    for forbidden in [
        "tool/parser_research",
        "generated_scanner_gate",
        "flark-reference-label-service",
        "regex =",
        "serde =",
    ] {
        assert!(
            !MANIFEST.contains(forbidden),
            "forbidden dependency: {forbidden}"
        );
    }
}

#[test]
fn vendored_lexical_facade_contains_no_full_parser_or_research_oracle() {
    const DONOR_ROOT: &str = include_str!("../vendor/comrak/src/lib.rs");
    const FACADE: &str = include_str!("../vendor/comrak/src/parser/block_spine_facade.rs");

    assert!(DONOR_ROOT.contains("block_spine_facade"));
    assert!(!FACADE.contains("oracle_block_projection"));
    assert!(!FACADE.contains("FacadeOracleBlock"));
    assert!(!FACADE.contains("Parser::new"));
    assert!(!FACADE.contains("parse_document"));
}

#[test]
fn production_controller_consumes_only_the_lexical_facade() {
    assert!(CONTROLLER.contains("use comrak::block_spine_facade"));
    for source in [
        PUBLIC_ROOT,
        CONTROLLER,
        include_str!("../src/contract.rs"),
        include_str!("../src/source_adapter.rs"),
    ] {
        assert!(!source.contains("parse_document"));
        assert!(!source.contains("markdown_to_html"));
        assert!(!source.contains("oracle_block_projection"));
    }
}

#[test]
fn public_root_does_not_expose_donor_storage_or_a_classifier() {
    for forbidden in [
        "BlockTree",
        "BlockNode",
        "Arena",
        "classify_paragraph",
        "looks_like_paragraph",
        "Regex",
    ] {
        assert!(
            !PUBLIC_ROOT.contains(forbidden),
            "forbidden public seam: {forbidden}"
        );
    }
}

use std::{
    fs,
    path::{Path, PathBuf},
};

const MANIFEST: &str = include_str!("../Cargo.toml");
const WORKSPACE_MANIFEST: &str = include_str!("../../../Cargo.toml");
const PUBLIC_ROOT: &str = include_str!("../src/lib.rs");
const CONTROLLER: &str = include_str!("../src/block_core/donor/core/src/parser.rs");

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
fn production_direct_controller_consumes_only_the_lexical_facade() {
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

#[test]
fn retired_candidate_transport_stays_out_of_the_parser_boundary() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/block_quote_projection_job.rs",
        "src/bullet_list_local_delta.rs",
        "src/bullet_list_projection_job.rs",
        "src/exact_clean.rs",
        "src/indented_code_projection_job.rs",
        "src/projected_inline_projection_job.rs",
        "src/publication.rs",
        "src/recursive_green_block_quote_projection.rs",
        "src/reference_cook.rs",
        "tests/block_quote_projection_job.rs",
        "tests/exact_clean.rs",
        "tests/fenced_code_clean.rs",
        "tests/indented_code_projection_job.rs",
        "tests/large_candidate_scaling.rs",
        "tests/leading_references_crop.rs",
        "tests/ordinary_paragraph_block_splice.rs",
        "tests/ordinary_paragraph_restart.rs",
        "tests/publication_vertical.rs",
    ] {
        assert!(
            !manifest.join(relative).exists(),
            "retired parser transport file returned: {relative}"
        );
    }

    let mut sources = Vec::new();
    collect_first_party_rust_sources(&manifest.join("src"), &mut sources);
    sources.sort();
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for forbidden in [
            "M11ParserCandidate",
            "M11ParserCandidateWriterPoll",
            "M11CandidateBuild",
            "M11CandidatePublication",
            "M11CandidateRoleBytes",
            "M11CleanParseJob",
            "M11CleanBlockController",
            "M11Published",
            "M11RetainedCandidatePublication",
            "M11OwnedSnapshotPoll",
            "M11RoleRecords",
            "M11SnapshotFrameKind",
            "M11InlineProjectionPublication",
            "M11InlineProjectionBuild",
            "M11InlineProjectionBuildStatus",
            "M11InlineProjectionRoot",
            "M11ReferenceCookReceipt",
            "M11RecursiveGreenBlockQuoteProjection",
            "prepare_block_quote_projection",
            "into_publication_parts",
        ] {
            assert!(
                !source.contains(forbidden),
                "retired parser transport identifier {forbidden} returned in {}",
                path.display()
            );
        }
    }
}

fn collect_first_party_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("read parser source entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "donor") {
                continue;
            }
            collect_first_party_rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

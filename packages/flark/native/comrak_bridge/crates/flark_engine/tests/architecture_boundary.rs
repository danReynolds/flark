use std::{fs, path::Path};

#[test]
fn retired_candidate_transport_stays_deleted() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/block_quote_projection.rs",
        "src/block_sequence.rs",
        "src/candidate_manifest.rs",
        "src/host_store.rs",
        "src/indented_code_projection.rs",
        "src/inline_overlay.rs",
        "src/m11_host.rs",
        "src/recursive_green/host_replay.rs",
        "src/recursive_green/publication.rs",
        "tests/inline_projection.rs",
        "tests/parser_pages.rs",
    ] {
        assert!(
            !manifest.join(relative).exists(),
            "retired candidate transport file returned: {relative}"
        );
    }
}

#[test]
fn parser_seam_contains_only_live_editing_services() {
    const PARSER_SEAM: &str = include_str!("../src/parser_internal.rs");
    for forbidden in [
        "CandidateManifest",
        "CandidateHost",
        "SnapshotEncoder",
        "InlineOverlay",
        "BlockQuoteProjection",
        "IndentedCodeProjection",
        "BlockSequence",
        "M11InlineProjectionBuild",
        "M11InlineProjectionRoot",
        "M11InlineProjectionDescriptor",
        "M11InlineProjectionCursor",
        "M11InlineProjectionCheckpointQuery",
    ] {
        assert!(
            !PARSER_SEAM.contains(forbidden),
            "retired parser seam identifier returned: {forbidden}"
        );
    }
}

#[test]
fn persistent_inline_transport_symbols_stay_out_of_engine_source() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    collect_rust_sources(&source_root, &mut sources);
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for forbidden in [
            "PersistentM11InlineProjection",
            "PersistentM11InlineLinkValue",
            "PERSISTENT_INLINE_PROJECTION",
            "PERSISTENT_PROJECTED_INLINE_PROJECTION",
            "RetainedM11InlineProjectionRole",
        ] {
            assert!(
                !source.contains(forbidden),
                "retired inline transport identifier {forbidden} returned in {}",
                path.display()
            );
        }
    }
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
    {
        let path = entry.expect("read engine source entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

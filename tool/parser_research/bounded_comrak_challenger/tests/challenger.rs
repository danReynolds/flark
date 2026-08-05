use std::sync::Arc;

use comrak::markdown_to_html;
use flark_bounded_comrak_challenger::{
    build_bounded_index, build_bounded_leaf_index, gfm_options, is_certified_payload_edit,
    naive_fragment_html, RegionDisposition, RegionShape, UnsupportedRisk,
};

#[test]
fn oversized_paragraph_never_enters_comrak_and_suffix_remains_available() {
    let source: Arc<str> = format!(
        "prefix\n\n{}\n\nsuffix paragraph\n",
        "ordinary payload ".repeat(20_000)
    )
    .into();
    let index = build_bounded_index(Arc::clone(&source), 8_192, &gfm_options());

    assert_eq!(index.metrics.premature_comrak_calls, 0);
    assert_eq!(index.metrics.opaque_regions, 1);
    assert!(index.metrics.opaque_source_bytes > 300_000);
    assert!(index.regions.iter().any(|region| {
        matches!(region.disposition, RegionDisposition::OpaqueOverCap)
            && source[region.range.clone()].contains("ordinary payload")
    }));
    assert!(index.regions.iter().any(|region| {
        region.is_delegated() && source[region.range.clone()].contains("suffix paragraph")
    }));
}

#[test]
#[cfg(not(debug_assertions))]
fn closed_fence_can_recover_a_later_suffix_in_the_subset() {
    let source = "```\ncode\n```\n\nsuffix\n";
    let index = build_bounded_index(source, 8, &gfm_options());
    assert!(index
        .regions
        .iter()
        .any(|region| matches!(region.disposition, RegionDisposition::OpaqueOverCap)));
    assert!(index.regions.iter().any(|region| {
        region.is_delegated() && index.source()[region.range.clone()].contains("suffix")
    }));
}

#[test]
fn current_subset_does_not_prove_exact_suffix_recovery_after_a_loose_list() {
    let source = format!("{}\nsuffix\n", "- list payload\n".repeat(100));
    let suffix = source.find("suffix").unwrap();
    let index = build_bounded_index(source.as_str(), 64, &gfm_options());
    let region = index
        .region_containing(suffix)
        .expect("suffix must be covered");

    // Comrak ends the list before this unindented paragraph. The research
    // subset keeps the line in the list envelope. That is a useful
    // falsification: exact suffix recovery needs the complete block grammar,
    // not a convenient boundary heuristic.
    assert_eq!(region.shape, RegionShape::List);
    assert!(matches!(
        region.disposition,
        RegionDisposition::OpaqueOverCap
    ));
}

#[test]
fn inline_leaf_mode_does_not_make_a_large_list_one_opaque_leaf() {
    let source = format!("{}\nsuffix\n", "- list payload\n".repeat(10_000));
    let index = build_bounded_leaf_index(source.as_str(), 64, &gfm_options());

    assert_eq!(index.metrics.opaque_regions, 0);
    assert!(index.metrics.delegated_regions >= 10_000);
    assert!(index
        .regions
        .iter()
        .filter(|region| region.shape == RegionShape::List)
        .all(|region| region.len() <= 64 && region.is_delegated()));
}

#[test]
#[cfg(not(debug_assertions))]
fn inline_leaf_mode_keeps_large_fence_payload_source_backed() {
    let source = format!("```\n{}\n```\n\nsuffix\n", "code payload\n".repeat(10_000));
    let index = build_bounded_leaf_index(source.as_str(), 64, &gfm_options());
    assert_eq!(index.metrics.source_backed_raw_regions, 1);
    assert!(index
        .regions
        .iter()
        .any(|region| matches!(region.disposition, RegionDisposition::SourceBackedRaw)));
}

#[test]
fn known_unsupported_block_grammar_fails_closed() {
    let source =
        "| head | value |\n| --- | ---: |\n| a | b |\n\n<script>\nx\n</script>\n\n[ref]: /url\n";
    let index = build_bounded_index(source, 65_536, &gfm_options());
    assert!(index.regions.iter().any(|region| matches!(
        region.disposition,
        RegionDisposition::OpaqueUnsupported(UnsupportedRisk::Table)
    )));
    assert!(index.regions.iter().any(|region| matches!(
        region.disposition,
        RegionDisposition::OpaqueUnsupported(UnsupportedRisk::HtmlBlock)
    )));
    assert!(index.regions.iter().any(|region| matches!(
        region.disposition,
        RegionDisposition::OpaqueUnsupported(UnsupportedRisk::ReferenceOrFootnote)
    )));
}

#[test]
fn setext_and_whole_list_envelopes_delegate_as_one_semantic_unit() {
    let source = "title\n===\n\n- one\n- two\n\nlast\n";
    let options = gfm_options();
    let index = build_bounded_index(source, 65_536, &options);

    assert!(index
        .regions
        .iter()
        .any(|region| { region.shape == RegionShape::SetextHeading && region.is_delegated() }));
    let lists = index
        .regions
        .iter()
        .filter(|region| region.shape == RegionShape::List)
        .collect::<Vec<_>>();
    assert_eq!(
        lists.len(),
        1,
        "list tightness requires the whole list envelope"
    );
    assert!(lists[0].is_delegated());
    assert_eq!(
        index.delegated_html(&options),
        markdown_to_html(source, &options)
    );
}

#[test]
fn naive_leaf_local_reference_and_footnote_delegation_is_not_exact() {
    let options = gfm_options();
    let reference_source = "A [link][shared].\n\n[shared]: /target\n";
    let reference_split = reference_source.find("\n\n").unwrap();
    let reference_ranges = [
        0..reference_split + 1,
        reference_split + 2..reference_source.len(),
    ];
    assert_ne!(
        naive_fragment_html(reference_source, &reference_ranges, &options),
        markdown_to_html(reference_source, &options)
    );

    let footnote_source = "Use note[^a].\n\n[^a]: Footnote body.\n";
    let footnote_split = footnote_source.find("\n\n").unwrap();
    let footnote_ranges = [
        0..footnote_split + 1,
        footnote_split + 2..footnote_source.len(),
    ];
    assert_ne!(
        naive_fragment_html(footnote_source, &footnote_ranges, &options),
        markdown_to_html(footnote_source, &options)
    );
}

#[test]
fn html_terminator_without_blank_exposes_opaque_suffix_risk_in_subset_spine() {
    let body = "x\n".repeat(10_000);
    let source = format!("<script>\n{body}</script>\nsuffix paragraph\n");
    let index = build_bounded_index(source.as_str(), 8_192, &gfm_options());

    // The subset spine does not know HTML block class terminators, so its only
    // honest behavior is one opaque envelope that includes the later suffix.
    assert_eq!(index.regions.len(), 1);
    assert!(matches!(
        index.regions[0].disposition,
        RegionDisposition::OpaqueUnsupported(UnsupportedRisk::HtmlBlock)
    ));
    assert!(index.source()[index.regions[0].range.clone()].contains("suffix paragraph"));
}

#[test]
fn certified_opaque_fast_path_is_intentionally_narrow() {
    let source = "ordinary payload in the middle of a long line\n";
    let payload = source.find("payload").unwrap();
    assert!(is_certified_payload_edit(
        source,
        payload + 4..payload + 5,
        "x"
    ));
    assert!(!is_certified_payload_edit(source, 0..1, "x"));
    assert!(!is_certified_payload_edit(
        source,
        payload..payload + 1,
        "\n"
    ));
}

use flark_engine::parser_internal::{
    M11RecursiveGreenPoint, M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowQueryOutcome,
    M11RecursiveGreenRowWindow,
};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity};
use flark_parser::{
    build_m11_compact_first_viewport_probe, build_m11_progressive_compact_probe,
    M11CompactViewportProbe,
};

fn rows(probe: &M11CompactViewportProbe, runtime: &DocumentRuntime) -> M11RecursiveGreenRowWindow {
    let root = probe.root();
    let source = root.source_range();
    let source_utf16 = root.source_utf16_range();
    let limits = M11RecursiveGreenRowQueryLimits::new(64, 1_024, 64 * 1_024, 256, 64 * 1_024)
        .expect("row limits");
    match root
        .locate_renderable_rows_bounded(
            runtime,
            M11RecursiveGreenPoint::new(
                usize::try_from(source.start).expect("source start"),
                usize::try_from(source_utf16.start).expect("UTF-16 start"),
                SourceBoundaryAffinity::After,
            ),
            source.end,
            limits,
        )
        .expect("row query")
    {
        M11RecursiveGreenRowQueryOutcome::Window(window) => window,
        M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
            panic!("row query exceeded {:?}", exceeded.limit())
        }
    }
}

fn release(mut probe: M11CompactViewportProbe, runtime: &mut DocumentRuntime) {
    probe.begin_release(runtime).expect("begin probe release");
    while !probe
        .poll_release(runtime, 4_096)
        .expect("poll probe release")
    {}
}

fn close(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(4_096).expect("poll close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

fn assert_progressive_matches_clean(
    source: &str,
    frontiers: &[usize],
    inline_point: usize,
) -> (usize, bool, bool) {
    let mut progressive_runtime = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
        .expect("progressive runtime");
    let progressive = build_m11_progressive_compact_probe(&mut progressive_runtime, 1, frontiers)
        .expect("progressive compact probe");
    let starvation_count = progressive.starvation_count();
    assert_eq!(starvation_count, frontiers.len() - 1);
    let structural_before_eof = progressive.first_structural_slice_before_eof();
    let structural_frontier = progressive.first_structural_slice_admitted_bytes();
    let early_certified = progressive.early_plain_closed_prefix_certified();
    let (early_viewport, progressive_viewport) = progressive.into_viewports();

    let mut clean_runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("clean runtime");
    let clean_viewport =
        build_m11_compact_first_viewport_probe(&mut clean_runtime, 1).expect("clean compact probe");

    assert_eq!(
        progressive_viewport.root().build_receipt(),
        clean_viewport.root().build_receipt(),
    );
    assert_eq!(
        progressive_viewport.root().source_range(),
        clean_viewport.root().source_range(),
    );
    let progressive_rows = rows(&progressive_viewport, &progressive_runtime);
    let clean_rows = rows(&clean_viewport, &clean_runtime);
    assert_eq!(progressive_rows.start_ordinal(), clean_rows.start_ordinal());
    assert_eq!(progressive_rows.total_rows(), clean_rows.total_rows());
    assert_eq!(progressive_rows.complete(), clean_rows.complete());
    assert_eq!(progressive_rows.rows(), clean_rows.rows());

    let point =
        M11RecursiveGreenPoint::new(inline_point, inline_point, SourceBoundaryAffinity::After);
    let progressive_inline = progressive_viewport
        .capture_inline_projection(&mut progressive_runtime, point)
        .expect("progressive inline")
        .expect("progressive paragraph");
    let clean_inline = clean_viewport
        .capture_inline_projection(&mut clean_runtime, point)
        .expect("clean inline")
        .expect("clean paragraph");
    assert_eq!(progressive_inline.inline_source, clean_inline.inline_source);
    assert_eq!(
        progressive_inline.inline_source_utf16,
        clean_inline.inline_source_utf16,
    );
    assert_eq!(progressive_inline.facts, clean_inline.facts);
    assert_eq!(progressive_inline.link_values, clean_inline.link_values);

    if let Some(early) = early_viewport {
        let early_rows = rows(&early, &progressive_runtime);
        assert_eq!(early_rows.rows(), progressive_rows.rows());
        let early_inline = early
            .capture_inline_projection(&mut progressive_runtime, point)
            .expect("early inline")
            .expect("early paragraph");
        assert_eq!(early_inline.inline_source, progressive_inline.inline_source);
        assert_eq!(early_inline.facts, progressive_inline.facts);
        assert_eq!(early_inline.link_values, progressive_inline.link_values);
        release(early, &mut progressive_runtime);
    }

    release(progressive_viewport, &mut progressive_runtime);
    release(clean_viewport, &mut clean_runtime);
    close(progressive_runtime);
    close(clean_runtime);
    (structural_frontier, structural_before_eof, early_certified)
}

#[test]
fn starvation_is_not_eof_across_setext_lookahead_and_late_reference_authority() {
    let mut source = String::from("Heading\n");
    let after_heading_line = source.len();
    source.push_str("---\n\n");
    let after_setext = source.len();
    for index in 0..48 {
        source.push_str(&format!("Paragraph {index} with **bold** and [late].\n\n"));
    }
    let after_visible_rows = source.len();
    source.push_str("[late]: https://example.com \"Late winner\"\n");
    let frontiers = [
        after_heading_line,
        after_setext,
        after_visible_rows,
        source.len(),
    ];

    let (structural_frontier, structural_before_eof, early_certified) =
        assert_progressive_matches_clean(&source, &frontiers, after_setext + 1);
    assert!(structural_before_eof);
    assert_eq!(structural_frontier, after_visible_rows);
    assert!(!early_certified);
}

#[test]
fn open_fence_survives_multiple_starvations_and_closes_only_on_real_input() {
    let mut source = String::from("```text\n");
    for index in 0..12 {
        source.push_str(&format!("code line {index}\n"));
    }
    let inside_fence = source.len();
    for index in 12..24 {
        source.push_str(&format!("code line {index}\n"));
    }
    let later_inside_fence = source.len();
    source.push_str("```\n\n");
    let closed_fence = source.len();
    for index in 0..40 {
        source.push_str(&format!("After fence {index}.\n\n"));
    }
    let visible_rows = source.len();
    source.push_str("Tail.\n");
    let frontiers = [
        inside_fence,
        later_inside_fence,
        closed_fence,
        visible_rows,
        source.len(),
    ];

    let (structural_frontier, structural_before_eof, early_certified) =
        assert_progressive_matches_clean(&source, &frontiers, closed_fence + 1);
    assert!(structural_before_eof);
    assert!(structural_frontier >= closed_fence);
    assert!(early_certified);
}

#[test]
fn an_unsealed_frontier_inside_a_physical_line_is_rejected() {
    let source = "first line\nsecond line\nthird line\n";
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let result = build_m11_progressive_compact_probe(
        &mut runtime,
        1,
        &[
            "first".len(),
            "first line\nsecond line\n".len(),
            source.len(),
        ],
    );
    assert!(result.is_err());
    close(runtime);
}

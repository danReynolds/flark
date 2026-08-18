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
    assert_eq!(progressive.starvation_count(), frontiers.len() - 1);
    assert_probe_matches_clean(progressive, progressive_runtime, source, inline_point, true)
}

fn assert_probe_matches_clean(
    progressive: flark_parser::M11ProgressiveCompactProbe,
    mut progressive_runtime: DocumentRuntime,
    source: &str,
    inline_point: usize,
    early_queryable: bool,
) -> (usize, bool, bool) {
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

    // An early viewport is bound to its mid-load generation; once later
    // appends advanced the source, querying it must fail closed, so the
    // equality comparison only runs when the generation still matches.
    if let Some(early) = early_viewport {
        if !early_queryable {
            release(early, &mut progressive_runtime);
        } else {
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
fn direct_links_escapes_and_code_spans_certify_before_eof() {
    // Bracket-bearing prose whose brackets are not reference uses — direct
    // links, escaped brackets, code spans — has no suffix dependency: the
    // audited capture consumes no Unknown lookup, so the slice certifies
    // before EOF with cooked link values equal to the eventual viewport.
    let mut source = String::new();
    for index in 0..48 {
        source.push_str(&format!(
            "Paragraph {index} has a [direct link](https://example.invalid/{index} \"T{index}\"), \\[escaped\\], and `arr[{index}]`.\n\n"
        ));
    }
    let after_visible = source.len();
    source.push_str("Tail paragraph.\n");
    let frontiers = [after_visible, source.len()];
    let inline_point = source.find("Paragraph 2").expect("linked paragraph") + 2;

    let (structural_frontier, structural_before_eof, early_certified) =
        assert_progressive_matches_clean(&source, &frontiers, inline_point);
    assert!(structural_before_eof);
    assert!(structural_frontier <= after_visible);
    assert!(early_certified);
}

#[test]
fn admitted_definitions_certify_linked_slices_before_eof() {
    // The removed reference window's rewritten journal — hidden coverage
    // for the definition bytes — flushes into the first-slice candidate, so
    // a document opening with definitions builds its slice; the committed
    // definition is the final GFM first winner (the later duplicate loses),
    // and the audited slice certifies before EOF with cooked link values
    // equal to the eventual full-authority viewport.
    let mut source = String::from("[top]: /admitted \"Top title\"\n\n");
    for index in 0..48 {
        source.push_str(&format!(
            "Paragraph {index} links [top] and [top][] with **bold**.\n\n"
        ));
    }
    let after_visible = source.len();
    source.push_str("[top]: /displaced \"Loser\"\n");
    let frontiers = [after_visible, source.len()];
    let inline_point = source.find("Paragraph 2").expect("linked paragraph") + 2;

    let (structural_frontier, structural_before_eof, early_certified) =
        assert_progressive_matches_clean(&source, &frontiers, inline_point);
    assert!(structural_before_eof);
    assert!(structural_frontier <= after_visible);
    assert!(early_certified);
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
fn opening_store_appends_drive_the_live_parser_to_clean_equality() {
    use flark_engine::{OpeningSourceStore, SourceRevision};
    use flark_parser::{build_m11_progressive_open_probe, M11ProgressiveOpenFeed};

    // CRLF paragraphs with direct links (audited early certification), a
    // fence spanning pages, late reference uses resolved by a definition in
    // the final page, and an unterminated tail sealed by exhaustion.
    let mut source = String::from("Heading\n---\n\n");
    for index in 0..40 {
        source.push_str(&format!(
            "Paragraph {index} has [a link](https://example.invalid/{index}) and **bold**.\r\n\r\n"
        ));
    }
    source.push_str("```text\n");
    for index in 0..24 {
        source.push_str(&format!("code line {index}\n"));
    }
    source.push_str("```\n\n");
    for index in 0..24 {
        source.push_str(&format!("After fence {index} uses [late].\n\n"));
    }
    source.push_str("[late]: https://example.com \"Late winner\"\n\nTail without newline");
    assert!(source.is_ascii(), "byte offsets double as UTF-16 offsets");

    // Hostile page cuts: between the CR and LF of one ending, mid-word,
    // inside the open fence, and at the late definition.
    let cuts = [
        source.find("\r\n").expect("first CRLF") + 1,
        source.find("Paragraph 7").expect("mid paragraph") + 6,
        source.find("code line 10").expect("inside fence") + 5,
        source.find("[late]:").expect("late definition"),
    ];
    assert!(cuts.windows(2).all(|window| window[0] < window[1]));
    let mut pages = Vec::new();
    let mut start = 0;
    for cut in cuts.into_iter().chain([source.len()]) {
        pages.push((start..cut, source[start..cut].to_string()));
        start = cut;
    }

    let mut store = OpeningSourceStore::new(SourceRevision::new(7), None).expect("opening store");
    let mut progressive_runtime = DocumentRuntime::from_opening_snapshot(
        store.snapshot(),
        DocumentRuntimeConfig::default(),
    )
    .expect("opening runtime");
    let admitted = store.version();
    let mut remaining = pages.into_iter();
    let probe = build_m11_progressive_open_probe(
        &mut progressive_runtime,
        1,
        &mut store,
        admitted,
        |store| match remaining.next() {
            Some((range, text)) => {
                let version = store.version();
                store
                    .append_page(version, range, &text)
                    .expect("transport page append");
                Ok(M11ProgressiveOpenFeed::Appended)
            }
            None => Ok(M11ProgressiveOpenFeed::Exhausted),
        },
    )
    .expect("opening-store progressive probe");

    assert!(probe.first_structural_slice_before_eof());
    assert!(probe.early_plain_closed_prefix_certified());
    assert!(probe.starvation_count() >= 5);
    let inline_point = source.find("Paragraph 3").expect("early paragraph") + 2;
    assert_probe_matches_clean(probe, progressive_runtime, &source, inline_point, false);

    let sealed = store.seal().expect("seal exhausted opening store");
    assert_eq!(sealed.version().byte_len(), source.len());
}

#[test]
fn hostile_shape_heads_certify_before_eof() {
    // Nested containers and tables in the first screenful must certify
    // through the audit (their bracket-free rows are suffix-independent
    // once closed) with early facts equal to the eventual viewport.
    let mut nested = String::new();
    for index in 0..24 {
        nested.push_str(&format!(
            "> quote {index} with **bold** content.\n\n- item {index} one\n- item {index} two\n\n"
        ));
    }
    let nested_visible = nested.len();
    nested.push_str("Tail paragraph.\n");
    let (_, before_eof, certified) = assert_progressive_matches_clean(
        &nested,
        &[nested_visible, nested.len()],
        nested.find("quote 2").expect("nested paragraph") + 1,
    );
    assert!(before_eof);
    assert!(certified, "nested heads certify");

    let mut tables = String::new();
    for index in 0..24 {
        tables.push_str(&format!(
            "| left {index} | right {index} |\n| :--- | ---: |\n| alpha | beta |\n\nParagraph {index} with **bold** words.\n\n"
        ));
    }
    let tables_visible = tables.len();
    tables.push_str("Tail paragraph.\n");
    let (_, before_eof, certified) = assert_progressive_matches_clean(
        &tables,
        &[tables_visible, tables.len()],
        tables.find("Paragraph 2").expect("table-adjacent paragraph") + 1,
    );
    assert!(before_eof);
    assert!(certified, "table heads certify");
}

#[test]
fn open_session_regenerates_certification_as_definitions_arrive() {
    use flark_engine::{OpeningSourceStore, SourceRevision};
    use flark_parser::{M11ProgressiveOpenSession, M11ProgressiveOpenSessionPoll};

    fn drive_to_starvation(
        session: &mut M11ProgressiveOpenSession,
        runtime: &mut DocumentRuntime,
    ) -> M11ProgressiveOpenSessionPoll {
        loop {
            match session.poll(runtime, 256).expect("session poll") {
                M11ProgressiveOpenSessionPoll::Pending => {}
                status => return status,
            }
        }
    }

    // Page 1: linked paragraphs whose label has no admitted definition, so
    // the captured slice cannot certify. Page 2 delivers the definition;
    // certification must appear at the following starvation, bound to the
    // page-2 generation, with rows queryable there. Page 3 rebinds it again.
    let mut page1 = String::new();
    for index in 0..40 {
        page1.push_str(&format!(
            "Paragraph {index} links [late] with **bold** text.\n\n"
        ));
    }
    let page2 = "[late]: /resolved \"Late title\"\n\nMiddle paragraph.\n\n".to_string();
    let page3 = "Tail paragraph.\n".to_string();

    let mut store = OpeningSourceStore::new(SourceRevision::new(3), None).expect("opening store");
    let v0 = store.version();
    store
        .append_page(v0, 0..page1.len(), &page1)
        .expect("page 1");
    let mut runtime = DocumentRuntime::from_opening_snapshot(
        store.snapshot(),
        DocumentRuntimeConfig::default(),
    )
    .expect("opening runtime");
    let mut session = M11ProgressiveOpenSession::begin(&mut runtime, 1).expect("open session");

    assert_eq!(
        drive_to_starvation(&mut session, &mut runtime),
        M11ProgressiveOpenSessionPoll::Starved
    );
    assert!(
        session.certified_early().is_none(),
        "no committed definition can certify [late] uses yet"
    );

    let before = store.version();
    store
        .append_page(before, page1.len()..page1.len() + page2.len(), &page2)
        .expect("page 2");
    session
        .adopt_append(
            &mut runtime,
            store.prove_append_since(before).expect("page 2 proof"),
            false,
        )
        .expect("adopt page 2");
    assert_eq!(
        drive_to_starvation(&mut session, &mut runtime),
        M11ProgressiveOpenSessionPoll::Starved
    );
    let (early, bound_source) = session
        .certified_early()
        .expect("definition arrival upgrades certification");
    assert_eq!(runtime.current_source_version(), Some(bound_source));
    let early_rows = rows(early, &runtime);
    assert_eq!(early_rows.rows().len(), 32);
    let inline_point = page1.find("Paragraph 2").expect("linked paragraph") + 2;
    let early_inline = early
        .capture_inline_projection(
            &mut runtime,
            M11RecursiveGreenPoint::new(inline_point, inline_point, SourceBoundaryAffinity::After),
        )
        .expect("early inline")
        .expect("early paragraph");
    assert!(
        format!("{:?}", early_inline.link_values).contains("/resolved"),
        "certified early rows resolve [late] through the committed winner"
    );

    // The next append rebinds the certified viewport to the new generation.
    let before = store.version();
    let page3_start = page1.len() + page2.len();
    store
        .append_page(before, page3_start..page3_start + page3.len(), &page3)
        .expect("page 3");
    session
        .adopt_append(
            &mut runtime,
            store.prove_append_since(before).expect("page 3 proof"),
            true,
        )
        .expect("adopt page 3");
    let (_, rebound_source) = session
        .certified_early()
        .expect("certification survives the next generation by regeneration");
    assert_ne!(rebound_source, bound_source);
    assert_eq!(runtime.current_source_version(), Some(rebound_source));

    while session.poll(&mut runtime, 4_096).expect("seal poll")
        != M11ProgressiveOpenSessionPoll::Complete
    {}
    let final_viewport = session.take_final(&mut runtime).expect("final viewport");
    let final_rows = rows(&final_viewport, &runtime);
    assert_eq!(final_rows.rows().len(), 32);
    release(final_viewport, &mut runtime);
    close(runtime);

    let _ = store.seal().expect("seal exhausted store");
}

#[test]
fn a_frontier_between_carriage_return_and_line_feed_is_rejected() {
    let source = "alpha\r\nbeta\r\ngamma\r\n";
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let result = build_m11_progressive_compact_probe(
        &mut runtime,
        1,
        &[
            "alpha\r".len(),
            "alpha\r\nbeta\r\n".len(),
            source.len(),
        ],
    );
    assert!(result.is_err());
    close(runtime);
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

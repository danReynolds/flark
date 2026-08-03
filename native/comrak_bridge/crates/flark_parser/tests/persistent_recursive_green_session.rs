use flark_engine::parser_internal::{
    M11RecursiveGreenPoint, M11RecursiveGreenRenderableRow, M11RecursiveGreenRowEditCapability,
    M11RecursiveGreenRowQueryLimits, M11RecursiveGreenRowQueryOutcome,
};
use flark_engine::{ArenaLimits, DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity};
use flark_parser::{
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
};

fn build_session(runtime: &mut DocumentRuntime) -> M11PersistentRecursiveGreenSession {
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime.snapshot_current_source().expect("scanner lease"),
        runtime.snapshot_current_source().expect("writer lease"),
        1,
    )
    .expect("clean plan");
    let mut build = plan.begin(runtime).expect("clean build");
    loop {
        let poll = build.poll(runtime, 64).expect("poll clean build");
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            return build.take_session().expect("persistent session");
        }
    }
}

fn utf16_offset(source: &str, byte: usize) -> usize {
    source[..byte].encode_utf16().count()
}

fn list_tightness(row: &M11RecursiveGreenRenderableRow) -> Vec<u8> {
    row.path()
        .iter()
        .filter(|frame| frame.kind().get() == 3)
        .map(|frame| {
            let close = frame.close().expect("List close facts");
            assert_eq!(close.tag().get(), 1);
            close.as_bytes()[0]
        })
        .collect()
}

fn release_session(
    runtime: &mut DocumentRuntime,
    session: &mut M11PersistentRecursiveGreenSession,
) {
    session.begin_release(runtime).expect("begin release");
    while !session.poll_release(runtime, 64).expect("poll release") {}
}

fn cancellation_fixture() -> (String, usize) {
    let mut source = String::from("[a]: /target\n\n");
    for ordinal in 0..240 {
        source.push_str(&format!(
            "Paragraph {ordinal:03} carries enough ordinary source for sparse restart spacing.\n\n"
        ));
    }
    let edit_start = source
        .find("Paragraph 140")
        .and_then(|paragraph| {
            source[paragraph..]
                .find("ordinary")
                .map(|word| paragraph + word)
        })
        .expect("late cancellation edit");
    (source, edit_start)
}

fn assert_original_base(
    base: &M11PersistentRecursiveGreenSession,
    source: flark_engine::SourceVersion,
    checkpoints: usize,
    references: u64,
) {
    assert_eq!(base.source(), source);
    assert_eq!(base.checkpoint_count(), checkpoints);
    assert_eq!(base.reference_occurrence_count(), references);
}

#[test]
fn clean_session_retains_green_and_reference_authority_for_late_queries() {
    let source = "[a]: /target\n\nbefore\n\nLate **bold** [a].\n";
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("document runtime");
    let mut session = build_session(&mut runtime);
    assert_eq!(session.reference_occurrence_count(), 1);
    assert!(session.checkpoint_count() >= 1);

    let point = source.find("bold").expect("late paragraph point");
    let prepared = session
        .prepare_paragraph_inline(
            &runtime,
            M11RecursiveGreenPoint::new(point, point, SourceBoundaryAffinity::After),
        )
        .expect("retained Green query");
    assert_eq!(
        &source[prepared.block_source_range().start as usize
            ..prepared.block_source_range().end as usize],
        "Late **bold** [a].\n"
    );
    assert_eq!(prepared.driver_work(), 0);

    let prepared_leaf = session
        .prepare_inline_leaf(
            &runtime,
            M11RecursiveGreenPoint::new(point, point, SourceBoundaryAffinity::After),
        )
        .expect("bounded retained Green row query");
    assert_eq!(
        prepared_leaf.block_source_range(),
        prepared.block_source_range()
    );
    assert_eq!(
        prepared_leaf.inline_source_range(),
        prepared.inline_source_range()
    );

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn eof_paragraph_append_retains_contiguous_editable_geometry() {
    const BASE: &str = "alpha";
    const TARGET: &str = "alpha beta";

    let mut runtime =
        DocumentRuntime::new(BASE, DocumentRuntimeConfig::default()).expect("base runtime");
    let session = build_session(&mut runtime);
    let base = session.source();
    runtime
        .apply_edit(base, BASE.len()..BASE.len(), " beta")
        .expect("append to EOF paragraph");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let mut adoption = session
        .begin_local_adoption(&runtime, target_lease, BASE.len()..BASE.len())
        .unwrap_or_else(|failure| panic!("EOF append adoption start: {}", failure.error()));
    loop {
        match adoption
            .poll(&mut runtime, 64)
            .expect("poll EOF append")
            .status()
        {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("EOF paragraph append required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("EOF paragraph append was cancelled")
            }
        }
    }
    let mut update = adoption.take_update().expect("EOF append update");
    let mut superseded = update.take_base().expect("superseded base");
    let mut target = update.take_target().expect("target session");
    let window = target
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
            TARGET.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(1, 8, 512, 16, 512).expect("EOF row limits"),
        )
        .expect("incremental EOF row query");
    let row = window
        .rows()
        .first()
        .expect("incremental EOF paragraph row");
    assert_eq!(row.kind().get(), 5);
    assert_eq!(
        row.edit_capability(),
        M11RecursiveGreenRowEditCapability::Contiguous
    );
    assert_eq!(row.editable_range(), Some(0..TARGET.len() as u64));

    let mut clean_runtime =
        DocumentRuntime::new(TARGET, DocumentRuntimeConfig::default()).expect("clean runtime");
    let mut clean = build_session(&mut clean_runtime);
    let clean_window = clean
        .query_renderable_rows(
            &clean_runtime,
            M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
            TARGET.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(1, 8, 512, 16, 512).expect("clean EOF row limits"),
        )
        .expect("clean EOF row query");
    let clean_row = clean_window
        .rows()
        .first()
        .expect("clean EOF paragraph row");
    assert_eq!(
        clean_row.edit_capability(),
        M11RecursiveGreenRowEditCapability::Contiguous
    );
    assert_eq!(clean_row.editable_range(), Some(0..TARGET.len() as u64));

    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
    release_session(&mut clean_runtime, &mut clean);
    clean_runtime.begin_close().expect("begin clean close");
    while !clean_runtime
        .poll_close(64)
        .expect("poll clean close")
        .complete
    {}
}

#[test]
fn terminal_empty_list_items_emit_one_editable_row_without_duplicating_nonempty_items() {
    for (source, item_start, list_style, marker) in [
        ("- alpha\n-   ", 8_usize, 1_u8, b'-'),
        ("7) alpha\n0)   ", 9_usize, 2_u8, b')'),
    ] {
        let mut runtime = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .expect("terminal-empty list runtime");
        let mut session = build_session(&mut runtime);
        let window = session
            .query_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
                source.len() as u64,
                M11RecursiveGreenRowQueryLimits::new(8, 8, 512, 16, 512)
                    .expect("terminal-empty row limits"),
            )
            .expect("terminal-empty row query");
        assert!(window.complete(), "source={source:?}");
        assert_eq!(window.rows().len(), 2, "source={source:?}");
        assert_eq!(window.rows()[0].kind().get(), 5, "source={source:?}");

        let empty = &window.rows()[1];
        assert_eq!(empty.kind().get(), 14, "source={source:?}");
        assert_eq!(
            empty.physical_range(),
            source.len() as u64..source.len() as u64,
            "source={source:?}",
        );
        assert_eq!(
            empty.edit_capability(),
            M11RecursiveGreenRowEditCapability::Contiguous,
        );
        assert_eq!(
            empty.editable_range(),
            Some(source.len() as u64..source.len() as u64),
            "source={source:?}",
        );
        assert_eq!(
            empty.editable_utf16_range(),
            Some(source.encode_utf16().count() as u64..source.encode_utf16().count() as u64,),
            "source={source:?}",
        );
        assert_eq!(
            empty
                .path()
                .iter()
                .map(|frame| frame.kind().get())
                .collect::<Vec<_>>(),
            vec![1, 3, 4, 14],
            "source={source:?}",
        );
        let list = empty.path()[1]
            .property()
            .expect("empty row retains List facts");
        assert_eq!(list.tag().get(), 1);
        assert_eq!(list.as_bytes()[0], list_style);
        assert_eq!(list.as_bytes()[if list_style == 1 { 1 } else { 2 }], marker,);
        let item = empty.path()[2]
            .property()
            .expect("empty row retains Item facts");
        assert_eq!(item.tag().get(), 2);
        assert_eq!(
            empty.path()[2].physical_range(),
            item_start as u64..source.len() as u64,
        );

        let suffix_start = source.len() - 1;
        let suffix = session
            .query_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(
                    suffix_start,
                    utf16_offset(source, suffix_start),
                    SourceBoundaryAffinity::After,
                ),
                source.len() as u64,
                M11RecursiveGreenRowQueryLimits::new(1, 8, 512, 16, 512)
                    .expect("terminal-empty suffix row limits"),
            )
            .expect("terminal-empty nonempty suffix range");
        assert!(suffix.complete(), "source={source:?}");
        assert_eq!(suffix.rows().len(), 1, "source={source:?}");
        assert_eq!(suffix.rows()[0].kind().get(), 14, "source={source:?}");
        assert_eq!(
            suffix.rows()[0].editable_range(),
            Some(source.len() as u64..source.len() as u64),
            "source={source:?}",
        );

        let eof = session
            .locate_point(
                &runtime,
                M11RecursiveGreenPoint::new(
                    source.len(),
                    source.encode_utf16().count(),
                    SourceBoundaryAffinity::After,
                ),
            )
            .expect("query terminal-empty EOF")
            .expect("terminal-empty EOF location");
        assert_eq!(
            eof.byte_range(),
            source.len() as u64..source.len() as u64,
            "source={source:?}",
        );
        assert_eq!(
            eof.utf16_range(),
            source.encode_utf16().count() as u64..source.encode_utf16().count() as u64,
            "source={source:?}",
        );
        assert_eq!(eof.owner().kind().get(), 14, "source={source:?}");
        assert_eq!(
            eof.ancestry()
                .iter()
                .map(|ancestor| ancestor.kind().get())
                .collect::<Vec<_>>(),
            vec![1, 3, 4, 14],
            "source={source:?}",
        );

        let before_eof = session
            .locate_point(
                &runtime,
                M11RecursiveGreenPoint::new(
                    source.len(),
                    source.encode_utf16().count(),
                    SourceBoundaryAffinity::Before,
                ),
            )
            .expect("query terminal-empty EOF with upstream affinity")
            .expect("terminal-empty upstream EOF location");
        assert_eq!(before_eof.owner().kind().get(), 4, "source={source:?}");
        assert_ne!(before_eof.byte_range().start, before_eof.byte_range().end);

        release_session(&mut runtime, &mut session);
        runtime.begin_close().expect("begin terminal-empty close");
        while !runtime
            .poll_close(64)
            .expect("poll terminal-empty close")
            .complete
        {}
    }

    let source = "- alpha\n- beta";
    let mut runtime = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
        .expect("nonempty list runtime");
    let mut session = build_session(&mut runtime);
    let window = session
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
            source.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(8, 8, 512, 16, 512).expect("nonempty row limits"),
        )
        .expect("nonempty row query");
    assert_eq!(
        window
            .rows()
            .iter()
            .map(|row| row.kind().get())
            .collect::<Vec<_>>(),
        vec![5, 5],
    );
    assert!(window.rows().iter().all(|row| {
        row.path()
            .iter()
            .map(|frame| frame.kind().get())
            .eq([1, 3, 4, 5])
    }));

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin nonempty close");
    while !runtime
        .poll_close(64)
        .expect("poll nonempty close")
        .complete
    {}
}

#[test]
fn leading_reference_visible_remainder_edit_uses_local_recursive_green_adoption() {
    const REFERENCES: usize = 128;
    let mut source = String::new();
    for ordinal in 0..REFERENCES {
        source.push_str(&format!("[r{ordinal}]: /target/{ordinal}\n"));
    }
    source.push_str("visible **bold** tail\n");
    let edit_start = source.rfind("bold").expect("visible tail edit");
    let mut target_source = source.clone();
    target_source.replace_range(edit_start..edit_start + 4, "strong");

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("leading-reference runtime");
    let session = build_session(&mut runtime);
    assert_eq!(session.reference_occurrence_count(), REFERENCES as u64);
    assert!(session.checkpoint_count() >= 2);
    let base = session.source();
    runtime
        .apply_edit(base, edit_start..edit_start + 4, "strong")
        .expect("visible tail edit");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let mut adoption = session
        .begin_local_adoption(&runtime, target_lease, edit_start..edit_start + 4)
        .unwrap_or_else(|failure| panic!("remainder adoption start: {}", failure.error()));
    loop {
        match adoption
            .poll(&mut runtime, 64)
            .expect("poll remainder adoption")
            .status()
        {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("visible remainder edit required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("visible remainder edit was cancelled")
            }
        }
    }
    let mut update = adoption.take_update().expect("local remainder update");
    assert!(update.work().source_bytes_read() < 256);
    assert!(update.work().green_tree_nodes_rebuilt() < 64);
    assert!(update.work().reference_rebind_transitions() <= 4);
    let mut superseded = update.take_base().expect("superseded base");
    let mut target = update.take_target().expect("target session");
    assert_eq!(target.reference_occurrence_count(), REFERENCES as u64);
    let visible_start = target_source.rfind("visible").expect("visible suffix");
    let row_window = target
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(edit_start, edit_start, SourceBoundaryAffinity::After),
            target_source.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 512)
                .expect("nonzero row limits"),
        )
        .expect("retained visible-suffix row query");
    let row = row_window.rows().first().expect("retained visible row");
    assert_eq!(row.kind().get(), 5);
    assert_eq!(
        row.edit_capability(),
        M11RecursiveGreenRowEditCapability::Contiguous
    );
    assert_eq!(
        row.editable_range(),
        Some(visible_start as u64..(target_source.len() - 1) as u64),
    );

    let incremental_digest = target
        .semantic_digest_for_diagnostics(&runtime)
        .expect("incremental digest");
    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean oracle runtime");
    let mut clean = build_session(&mut clean_runtime);
    assert_eq!(
        incremental_digest,
        clean
            .semantic_digest_for_diagnostics(&clean_runtime)
            .expect("clean digest")
    );

    release_session(&mut clean_runtime, &mut clean);
    clean_runtime.begin_close().expect("begin clean close");
    while !clean_runtime
        .poll_close(64)
        .expect("poll clean close")
        .complete
    {}
    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
#[ignore = "large-scale bounded inline preparation gate"]
fn leading_reference_visible_tail_inline_preparation_uses_bounded_row_geometry() {
    const REFERENCES: usize = 9_000;
    let mut source = String::new();
    for ordinal in 0..REFERENCES {
        source.push_str(&format!("[r{ordinal}]: /target/{ordinal}\n"));
    }
    source.push_str("visible **bold** tail\n");
    let point = source.rfind("bold").expect("visible tail point");
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("leading-reference runtime");
    let mut session = build_session(&mut runtime);

    let prepared = session
        .prepare_inline_leaf(
            &runtime,
            M11RecursiveGreenPoint::new(
                point,
                utf16_offset(&source, point),
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("bounded visible-tail inline preparation");
    assert_eq!(prepared.block_source_range(), 0..source.len() as u32);
    assert_eq!(
        prepared.inline_source_range(),
        (source.len() - "visible **bold** tail\n".len()) as u32..(source.len() - 1) as u32,
    );
    assert!(prepared.query_receipt().storage_pages_visited() <= 25);
    assert!(prepared.query_receipt().node_headers_decoded() <= 512);

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn bounded_inline_preparation_rejects_unrendered_separator_points() {
    let source = "\nfirst\n\nsecond\n";
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("separator runtime");
    let mut session = build_session(&mut runtime);
    for point in [0, source.find("\n\n").expect("middle separator") + 1] {
        assert!(
            session
                .prepare_inline_leaf(
                    &runtime,
                    M11RecursiveGreenPoint::new(
                        point,
                        utf16_offset(source, point),
                        SourceBoundaryAffinity::After,
                    ),
                )
                .is_err(),
            "unrendered separator at {point} must not borrow the next row",
        );
    }

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn local_adoption_cancel_before_splice_returns_complete_original_base() {
    let (source, edit_start) = cancellation_fixture();
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("cancellation runtime");
    let session = build_session(&mut runtime);
    let base_source = session.source();
    let base_checkpoints = session.checkpoint_count();
    let base_references = session.reference_occurrence_count();
    let base_resident_nodes = runtime.arena_metrics().resident_nodes;
    assert!(base_checkpoints >= 3);

    runtime
        .apply_edit(
            base_source,
            edit_start..edit_start + "ordinary".len(),
            "precise_",
        )
        .expect("cancellation edit");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("cancellation target lease");
    let mut adoption = session
        .begin_local_adoption(
            &runtime,
            target_lease,
            edit_start..edit_start + "ordinary".len(),
        )
        .unwrap_or_else(|failure| panic!("cancellation adoption start: {}", failure.error()));

    adoption
        .begin_cancel(&mut runtime)
        .expect("begin pre-splice cancellation");
    while !adoption
        .poll_cancel(&mut runtime, 1)
        .expect("poll pre-splice cancellation")
    {}
    let mut base = adoption
        .take_base_after_cancel()
        .expect("complete base after pre-splice cancellation");
    assert_original_base(&base, base_source, base_checkpoints, base_references);
    assert_eq!(runtime.arena_metrics().resident_nodes, base_resident_nodes);

    release_session(&mut runtime, &mut base);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

#[test]
fn local_adoption_cancel_after_splice_releases_target_and_returns_complete_original_base() {
    let (source, edit_start) = cancellation_fixture();
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("cancellation runtime");
    let session = build_session(&mut runtime);
    while !runtime.poll_retirement(1).complete {}
    let base_source = session.source();
    let base_checkpoints = session.checkpoint_count();
    let base_references = session.reference_occurrence_count();
    let base_arena_metrics = runtime.arena_metrics();
    let base_resident_nodes = base_arena_metrics.resident_nodes;
    assert!(base_checkpoints >= 3);
    let base_page_probe_offsets = [
        source.find("Paragraph 010").expect("prefix page probe"),
        source.find("Paragraph 140").expect("edited page probe"),
        source.find("Paragraph 230").expect("suffix page probe"),
    ];
    let base_page_probes = base_page_probe_offsets.map(|probe| {
        session
            .storage_page_identity_at_source_byte_for_diagnostics(&runtime, probe)
            .expect("base page identity")
    });

    runtime
        .apply_edit(
            base_source,
            edit_start..edit_start + "ordinary".len(),
            "precise_",
        )
        .expect("cancellation edit");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("cancellation target lease");
    let mut adoption = session
        .begin_local_adoption(
            &runtime,
            target_lease,
            edit_start..edit_start + "ordinary".len(),
        )
        .unwrap_or_else(|failure| panic!("cancellation adoption start: {}", failure.error()));

    let mut target_resident_nodes = None;
    for _ in 0..20_000 {
        let poll = adoption
            .poll(&mut runtime, 1)
            .expect("poll adoption through structural splice");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                panic!("adoption completed before the post-splice cancellation checkpoint")
            }
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("post-splice cancellation fixture required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("adoption cancelled before cancellation was requested")
            }
        }
        if runtime.arena_metrics().resident_nodes > base_resident_nodes {
            target_resident_nodes = Some(runtime.arena_metrics().resident_nodes);
            break;
        }
    }
    let target_resident_nodes =
        target_resident_nodes.expect("target Green root was never installed");

    adoption
        .begin_cancel(&mut runtime)
        .expect("begin post-splice cancellation");
    while !adoption
        .poll_cancel(&mut runtime, 1)
        .expect("poll post-splice cancellation")
    {}
    let mut base = adoption
        .take_base_after_cancel()
        .expect("complete base after post-splice cancellation");
    assert_original_base(&base, base_source, base_checkpoints, base_references);
    assert_eq!(
        base_page_probes,
        base_page_probe_offsets.map(|probe| {
            base.storage_page_identity_at_source_byte_for_diagnostics(&runtime, probe)
                .expect("restored base page identity")
        })
    );
    assert!(target_resident_nodes > base_resident_nodes);
    let restored_arena_metrics = runtime.arena_metrics();
    assert_eq!(
        (
            restored_arena_metrics.resident_nodes,
            restored_arena_metrics.live_payload_bytes,
            restored_arena_metrics.reserved_external_payload_bytes,
            restored_arena_metrics.pending_reclaims,
            restored_arena_metrics.live_builds,
            restored_arena_metrics.pending_build_aborts,
        ),
        (
            base_arena_metrics.resident_nodes,
            base_arena_metrics.live_payload_bytes,
            base_arena_metrics.reserved_external_payload_bytes,
            base_arena_metrics.pending_reclaims,
            base_arena_metrics.live_builds,
            base_arena_metrics.pending_build_aborts,
        ),
        "post-splice cancellation must restore every live arena metric",
    );
    assert!(
        restored_arena_metrics.allocated_slots >= base_arena_metrics.allocated_slots,
        "allocator slot high-water must remain monotonic",
    );

    release_session(&mut runtime, &mut base);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

#[test]
fn large_session_keeps_two_late_edits_local_and_reference_rebind_constant() {
    let mut source = String::from("[a]: /target\n\n");
    for ordinal in 0..9_000 {
        source.push_str(&format!(
            "Paragraph {ordinal:05} carries enough ordinary source for sparse restart spacing.\n\n"
        ));
    }
    source.push_str("Late **bold** and _live_ [a].\n\n");
    for ordinal in 0..100 {
        source.push_str(&format!(
            "Trailing convergence paragraph {ordinal:03} keeps an unchanged suffix available.\n\n"
        ));
    }
    assert!(source.len() > 512 * 1024);

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("large document runtime");
    let session0 = build_session(&mut runtime);
    assert!(session0.checkpoint_count() < source.len() / 2048);

    let first_start = source.rfind("bold").expect("first edit");
    let base0 = session0.source();
    runtime
        .apply_edit(base0, first_start..first_start + 4, "strong")
        .expect("first edit");
    let target1_lease = runtime
        .snapshot_current_source()
        .expect("first target lease");
    let mut adoption1 = session0
        .begin_local_adoption(&runtime, target1_lease, first_start..first_start + 4)
        .unwrap_or_else(|failure| panic!("first adoption start: {}", failure.error()));
    loop {
        let poll = adoption1.poll(&mut runtime, 64).expect("first adoption");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("first edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("first edit adoption was not cancelled")
            }
        }
    }
    let mut update1 = adoption1.take_update().expect("first update");
    let work1 = update1.work();
    assert!(work1.source_bytes_read() < 16 * 1024);
    assert!(work1.green_tree_nodes_rebuilt() < 256);
    assert!(work1.reference_rebind_transitions() <= 4);
    let mut superseded0 = update1.take_base().expect("first base");
    let session1 = update1.take_target().expect("first target");
    release_session(&mut runtime, &mut superseded0);

    let target1_source = runtime.current_source_version().expect("target1 source");
    let target1_text = "Late **strong** and _live_ [a].\n";
    let target1_start = source.rfind("Late **bold**").expect("late island start");
    let second_start = target1_start + target1_text.find("live").expect("second edit");
    runtime
        .apply_edit(target1_source, second_start..second_start + 4, "fluid")
        .expect("second edit");
    let target2_lease = runtime
        .snapshot_current_source()
        .expect("second target lease");
    let mut adoption2 = session1
        .begin_local_adoption(&runtime, target2_lease, second_start..second_start + 4)
        .unwrap_or_else(|failure| panic!("second adoption start: {}", failure.error()));
    loop {
        let poll = adoption2.poll(&mut runtime, 64).expect("second adoption");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("second edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("second edit adoption was not cancelled")
            }
        }
    }
    let mut update2 = adoption2.take_update().expect("second update");
    let work2 = update2.work();
    assert!(work2.source_bytes_read() < 16 * 1024);
    assert!(work2.green_tree_nodes_rebuilt() < 256);
    assert!(work2.reference_rebind_transitions() <= 4);
    let mut superseded1 = update2.take_base().expect("second base");
    let mut session2 = update2.take_target().expect("second target");
    release_session(&mut runtime, &mut superseded1);

    let target2_text = "Late **strong** and _fluid_ [a].\n";
    let target2_start = target1_start;
    let prepared = session2
        .prepare_paragraph_inline(
            &runtime,
            M11RecursiveGreenPoint::new(
                target2_start + 10,
                target2_start + 10,
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("late retained query");
    assert_eq!(
        prepared.block_source_range(),
        target2_start as u32..(target2_start + target2_text.len()) as u32
    );
    assert_eq!(prepared.driver_work(), 0);

    release_session(&mut runtime, &mut session2);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn large_block_quote_edit_restarts_inside_the_open_container_and_reuses_distant_pages() {
    const QUOTE_LINES: usize = 2_048;
    const EDIT_LINE: usize = QUOTE_LINES / 2;
    let mut source = String::new();
    for ordinal in 0..128 {
        source.push_str(&format!(
            "Prefix paragraph {ordinal:03} keeps a distant Green page reusable.\n\n"
        ));
    }
    for ordinal in 0..QUOTE_LINES {
        source.push_str(&format!(
            "> quoted line {ordinal:04} carries alpha through one open paragraph.\n"
        ));
    }
    source.push('\n');
    let suffix_start = source.len();
    for ordinal in 0..128 {
        source.push_str(&format!(
            "Suffix paragraph {ordinal:03} keeps another distant Green page reusable.\n\n"
        ));
    }

    let edit_line = format!("> quoted line {EDIT_LINE:04} carries alpha");
    let edit_start = source
        .find(&edit_line)
        .map(|line| line + edit_line.find("alpha").expect("alpha in edit line"))
        .expect("middle quote edit");
    let edit_end = edit_start + "alpha".len();
    let mut target_source = source.clone();
    target_source.replace_range(edit_start..edit_end, "βeta");
    assert_eq!(target_source.len(), source.len());
    assert_eq!(
        target_source.encode_utf16().count() + 1,
        source.encode_utf16().count(),
    );

    let prefix_probe = source.find("Prefix paragraph 000").expect("prefix probe");
    let suffix_probe = suffix_start
        + source[suffix_start..]
            .find("Suffix paragraph 127")
            .expect("suffix probe");
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("large block-quote runtime");
    let session = build_session(&mut runtime);
    let base_prefix_page = session
        .storage_page_identity_at_source_byte_for_diagnostics(&runtime, prefix_probe)
        .expect("base prefix page");
    let base_suffix_page = session
        .storage_page_identity_at_source_byte_for_diagnostics(&runtime, suffix_probe)
        .expect("base suffix page");

    let base = session.source();
    runtime
        .apply_edit(base, edit_start..edit_end, "βeta")
        .expect("edit inside block quote");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("block-quote target lease");
    let mut adoption = session
        .begin_local_adoption(&runtime, target_lease, edit_start..edit_end)
        .unwrap_or_else(|failure| panic!("block-quote adoption start: {}", failure.error()));
    loop {
        match adoption
            .poll(&mut runtime, 64)
            .expect("block-quote adoption")
            .status()
        {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("block-quote content edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("block-quote content edit was cancelled")
            }
        }
    }

    let mut update = adoption.take_update().expect("block-quote update");
    let work = update.work();
    assert!(
        work.source_bytes_read() < 16 * 1024,
        "an edit halfway through one large quote must not reparse the whole container: {work:?}",
    );
    assert!(work.green_tree_nodes_rebuilt() < 512, "work={work:?}");
    let selected_segments = update.recursive_green_splice_selection().segments();
    assert_eq!(
        selected_segments.len(),
        2,
        "the primary local splice and the far repaired Paragraph Exit must both be published",
    );
    let primary_base = selected_segments[0].base_event_range();
    let primary_target = selected_segments[0].target_event_range();
    let far_base = selected_segments[1].base_event_range();
    let far_target = selected_segments[1].target_event_range();
    assert!(primary_base.end <= far_base.start);
    assert_eq!(
        far_base.end - far_base.start,
        far_target.end - far_target.start
    );
    assert_eq!(
        far_base.start - primary_base.end,
        far_target.start - primary_target.end,
        "the unchanged event gap must carry the primary splice delta into target coordinates",
    );
    let mut superseded = update.take_base().expect("block-quote base");
    let mut target = update.take_target().expect("block-quote target");
    assert_eq!(
        target
            .storage_page_identity_at_source_byte_for_diagnostics(&runtime, prefix_probe)
            .expect("target prefix page"),
        base_prefix_page,
    );
    assert_eq!(
        target
            .storage_page_identity_at_source_byte_for_diagnostics(&runtime, suffix_probe)
            .expect("target suffix page"),
        base_suffix_page,
    );
    let edited_utf16 = utf16_offset(&target_source, edit_start);
    let location = target
        .locate_point(
            &runtime,
            M11RecursiveGreenPoint::new(edit_start, edited_utf16, SourceBoundaryAffinity::After),
        )
        .expect("incremental quote query")
        .expect("incremental quote location");
    assert_eq!(
        location
            .ancestry()
            .iter()
            .map(|frame| frame.kind().get())
            .collect::<Vec<_>>(),
        vec![1, 2, 5],
    );

    let incremental_digest = target
        .semantic_digest_for_diagnostics(&runtime)
        .expect("incremental quote digest");
    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean quote runtime");
    let mut clean = build_session(&mut clean_runtime);
    assert_eq!(
        clean
            .semantic_digest_for_diagnostics(&clean_runtime)
            .expect("clean quote digest"),
        incremental_digest,
    );

    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    runtime.begin_close().expect("begin quote runtime close");
    while !runtime
        .poll_close(64)
        .expect("poll quote runtime close")
        .complete
    {}
    release_session(&mut clean_runtime, &mut clean);
    clean_runtime
        .begin_close()
        .expect("begin clean quote runtime close");
    while !clean_runtime
        .poll_close(64)
        .expect("poll clean quote runtime close")
        .complete
    {}
}

#[test]
fn commonmark_321_and_structure_changing_325_stay_generic_exact_and_local_in_a_large_session() {
    const CM321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
    const CM325: &str = "* foo\n  * bar\n\n  baz\n";

    let mut source = String::new();
    for ordinal in 0..9_000 {
        source.push_str(&format!(
            "Prefix paragraph {ordinal:05} carries enough ordinary source for sparse restart spacing.\n\n"
        ));
    }
    let cm321_start = source.len();
    source.push_str(CM321);
    source.push('\n');
    let cm325_start = source.len();
    source.push_str(CM325);
    source.push('\n');
    for ordinal in 0..1_000 {
        source.push_str(&format!(
            "Trailing paragraph {ordinal:04} remains an unchanged serialized-Green sibling.\n\n"
        ));
    }
    assert!(source.len() > 512 * 1024);

    let edit_start = cm325_start + CM325.find("baz").expect("lazy outer-item Paragraph");
    let edit_end = edit_start + "baz".len();
    let prefix_probe = source.find("Prefix paragraph 00000").expect("prefix probe");
    let suffix_probe = source
        .rfind("Trailing paragraph 0999")
        .expect("suffix probe");
    let mut target_source = source.clone();
    target_source.replace_range(edit_start..edit_end, "* βaz");
    const BYTE_DELTA: usize = "* βaz".len() - "baz".len();
    const UTF16_DELTA: usize = 5 - 3;
    assert_eq!(target_source.len(), source.len() + BYTE_DELTA);
    assert_eq!(
        target_source.encode_utf16().count(),
        source.encode_utf16().count() + UTF16_DELTA,
    );

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("large recursive-container runtime");
    let session0 = build_session(&mut runtime);
    let base_prefix_page = session0
        .storage_page_identity_at_source_byte_for_diagnostics(&runtime, prefix_probe)
        .expect("base prefix page");
    let base_suffix_page = session0
        .storage_page_identity_at_source_byte_for_diagnostics(&runtime, suffix_probe)
        .expect("base suffix page");
    let base_bar = cm325_start + CM325.find("bar").expect("nested List Paragraph");
    let base_rows = session0
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(base_bar, base_bar, SourceBoundaryAffinity::After),
            u64::try_from(cm325_start + CM325.len()).expect("CM325 base end fits"),
            M11RecursiveGreenRowQueryLimits::new(4, 32, 4096, 16, 4096)
                .expect("bounded CM325 base row limits"),
        )
        .expect("base nested-list rows");
    let base_bar_row = base_rows
        .rows()
        .iter()
        .find(|row| {
            row.editable_range().is_some_and(|range| {
                range.start <= base_bar as u64 && (base_bar as u64) < range.end
            })
        })
        .expect("base nested-list Paragraph row");
    assert_eq!(
        base_bar_row
            .path()
            .iter()
            .map(|frame| frame.kind().get())
            .collect::<Vec<_>>(),
        vec![1, 3, 4, 3, 4, 5],
    );
    assert_eq!(
        list_tightness(base_bar_row),
        vec![0, 1],
        "CM325 starts with a loose outer List and tight nested List",
    );

    let base = session0.source();
    runtime
        .apply_edit(base, edit_start..edit_end, "* βaz")
        .expect("turn the lazy outer-item Paragraph into a second nested-list Item");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("recursive-container target lease");
    let mut adoption = session0
        .begin_local_adoption(&runtime, target_lease, edit_start..edit_end)
        .unwrap_or_else(|failure| panic!("container adoption start: {}", failure.error()));
    let mut fuel_index = 0_usize;
    const FUEL_PATTERN: [usize; 4] = [1, 7, 3, 64];
    loop {
        let fuel = FUEL_PATTERN[fuel_index % FUEL_PATTERN.len()];
        fuel_index += 1;
        let poll = adoption
            .poll(&mut runtime, fuel)
            .expect("container adoption");
        assert!(poll.transitions() <= fuel);
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("structure-changing CM325 edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("nested container adoption was cancelled")
            }
        }
    }

    let mut update = adoption.take_update().expect("container update");
    let work = update.work();
    assert!(work.source_bytes_read() < 16 * 1024);
    assert!(work.green_tree_nodes_rebuilt() < 256);
    let mut superseded = update.take_base().expect("container base");
    let mut target = update.take_target().expect("container target");

    assert_eq!(
        target
            .storage_page_identity_at_source_byte_for_diagnostics(&runtime, prefix_probe)
            .expect("target prefix page"),
        base_prefix_page,
    );
    assert_eq!(
        target
            .storage_page_identity_at_source_byte_for_diagnostics(
                &runtime,
                suffix_probe + BYTE_DELTA,
            )
            .expect("target suffix page"),
        base_suffix_page,
    );
    let cm321_probe = cm321_start + CM321.find("> b").expect("retained quote child") + 2;
    let cm321_location = target
        .locate_point(
            &runtime,
            M11RecursiveGreenPoint::new(cm321_probe, cm321_probe, SourceBoundaryAffinity::After),
        )
        .expect("retained CM321 query")
        .expect("retained CM321 location");
    assert_eq!(
        cm321_location
            .ancestry()
            .iter()
            .map(|ancestor| ancestor.kind().get())
            .collect::<Vec<_>>(),
        vec![1, 3, 4, 2, 5],
    );
    let edited_content_byte = edit_start + "* β".len();
    let edited_content_utf16 = utf16_offset(&target_source, edited_content_byte);
    assert_ne!(edited_content_byte, edited_content_utf16);
    let row_window = target
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(
                edited_content_byte,
                edited_content_utf16,
                SourceBoundaryAffinity::After,
            ),
            u64::try_from(cm325_start + CM325.len() + BYTE_DELTA).expect("CM325 target end fits"),
            M11RecursiveGreenRowQueryLimits::new(4, 32, 4096, 16, 4096)
                .expect("bounded CM325 row limits"),
        )
        .expect("edited nested-list Paragraph rows");
    let edited_row = row_window
        .rows()
        .iter()
        .find(|row| {
            row.editable_range().is_some_and(|range| {
                range.start <= edited_content_byte as u64
                    && (edited_content_byte as u64) < range.end
            })
        })
        .expect("edited nested-list Paragraph row");
    assert_eq!(edited_row.kind().get(), 5);
    assert_eq!(
        edited_row
            .path()
            .iter()
            .map(|frame| frame.kind().get())
            .collect::<Vec<_>>(),
        vec![1, 3, 4, 3, 4, 5],
        "the former outer-item Paragraph must become a second nested-list Item",
    );
    assert_eq!(
        edited_row.physical_range(),
        (edit_start + 2) as u64..(edit_start + 7) as u64,
    );
    assert_eq!(
        edited_row.physical_utf16_range(),
        (edit_start + 2) as u64..(edit_start + 6) as u64,
    );
    assert_eq!(
        edited_row.editable_range(),
        Some((edit_start + 2) as u64..(edit_start + 6) as u64),
    );
    assert_eq!(
        edited_row.editable_utf16_range(),
        Some((edit_start + 2) as u64..(edit_start + 5) as u64),
    );
    assert_eq!(
        list_tightness(edited_row),
        vec![1, 0],
        "the edit makes the nested list loose while the outer list becomes tight",
    );

    let incremental_digest = target
        .semantic_digest_for_diagnostics(&runtime)
        .expect("incremental semantic digest");
    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean target runtime");
    let mut clean_target = build_session(&mut clean_runtime);
    assert_eq!(
        clean_target
            .semantic_digest_for_diagnostics(&clean_runtime)
            .expect("clean semantic digest"),
        incremental_digest,
    );

    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    runtime
        .begin_close()
        .expect("begin container runtime close");
    while !runtime
        .poll_close(64)
        .expect("poll container runtime close")
        .complete
    {}
    release_session(&mut clean_runtime, &mut clean_target);
    clean_runtime
        .begin_close()
        .expect("begin clean runtime close");
    while !clean_runtime
        .poll_close(64)
        .expect("poll clean runtime close")
        .complete
    {}
}

#[test]
fn large_fenced_row_query_uses_cached_literal_geometry_within_normal_budget() {
    const BODY_LINES: usize = 2_500;
    let mut source = String::from("before\r\n\r\n```rust\r\n");
    let literal_start = source.len();
    for ordinal in 0..BODY_LINES {
        source.push_str(&format!("body {ordinal:04} α\r\n"));
    }
    let literal_end = source.len();
    source.push_str("```\r\n");
    let mut point = literal_start + (literal_end - literal_start) / 2;
    while !source.is_char_boundary(point) {
        point -= 1;
    }
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("fence runtime");
    let mut session = build_session(&mut runtime);
    let window = session
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(
                point,
                utf16_offset(&source, point),
                SourceBoundaryAffinity::After,
            ),
            source.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 320)
                .expect("nonzero row limits"),
        )
        .expect("large fence row query");
    let row = window.rows().first().expect("large fence row");
    assert_eq!(row.kind().get(), 7);
    assert_eq!(
        row.edit_capability(),
        M11RecursiveGreenRowEditCapability::Contiguous
    );
    assert_eq!(
        row.editable_range(),
        Some(literal_start as u64..literal_end as u64)
    );
    assert_eq!(
        row.editable_utf16_range(),
        Some(
            utf16_offset(&source, literal_start) as u64..utf16_offset(&source, literal_end) as u64
        )
    );
    assert!(window.receipt().events_scanned() < 3_200);
    assert!(window.receipt().storage_pages_visited() <= 25);
    assert!(window.receipt().node_headers_decoded() <= 320);

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin fence close");
    while !runtime.poll_close(64).expect("poll fence close").complete {}
}

#[test]
fn fenced_cached_geometry_is_exact_for_empty_crlf_and_unclosed_literals() {
    let cases = [
        ("```\n```\n", 4_usize, 4_usize),
        ("~~~lang\r\n~~~\r\n", 9_usize, 9_usize),
        ("```\r\nalpha\r\nbeta", 5_usize, 16_usize),
    ];
    for (source, literal_start, literal_end) in cases {
        let mut runtime = DocumentRuntime::new(source, DocumentRuntimeConfig::default())
            .expect("fence edge runtime");
        let mut session = build_session(&mut runtime);
        let window = session
            .query_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
                source.len() as u64,
                M11RecursiveGreenRowQueryLimits::new(1, 8, 128, 16, 1_024)
                    .expect("nonzero row limits"),
            )
            .expect("fence edge row query");
        let row = window.rows().first().expect("fence edge row");
        assert_eq!(row.kind().get(), 7);
        assert_eq!(
            row.edit_capability(),
            M11RecursiveGreenRowEditCapability::Contiguous
        );
        assert_eq!(
            row.editable_range(),
            Some(literal_start as u64..literal_end as u64),
            "source={source:?}"
        );
        assert_eq!(
            row.editable_utf16_range(),
            Some(
                utf16_offset(source, literal_start) as u64
                    ..utf16_offset(source, literal_end) as u64
            ),
            "source={source:?}"
        );

        release_session(&mut runtime, &mut session);
        runtime.begin_close().expect("begin fence edge close");
        while !runtime
            .poll_close(64)
            .expect("poll fence edge close")
            .complete
        {}
    }
}

#[test]
#[ignore = "large-scale cached-row release gate"]
fn one_hundred_thousand_leading_references_query_visible_tail_within_normal_budget() {
    const REFERENCES: usize = 100_000;
    let mut source = String::new();
    source.reserve(REFERENCES * 32 + 32);
    for ordinal in 0..REFERENCES {
        source.push_str(&format!("[r{ordinal}]: /target/{ordinal}\n"));
    }
    let literal_start = source.len();
    source.push_str("visible **bold** tail\n");
    let literal_end = source.len() - 1;
    let point = literal_start + "visible **".len();
    let mut runtime = DocumentRuntime::new(
        &source,
        DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_slots: 131_072,
                ..ArenaLimits::default()
            },
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("large reference runtime");
    let build_started = std::time::Instant::now();
    let mut session = build_session(&mut runtime);
    let build_elapsed = build_started.elapsed();
    assert_eq!(session.reference_occurrence_count(), REFERENCES as u64);
    let query_started = std::time::Instant::now();
    let outcome = session
        .query_renderable_rows_bounded(
            &runtime,
            M11RecursiveGreenPoint::new(
                point,
                utf16_offset(&source, point),
                SourceBoundaryAffinity::After,
            ),
            source.len() as u64,
            M11RecursiveGreenRowQueryLimits::new(1, 25, 3_200, 64, 512)
                .expect("nonzero row limits"),
        )
        .expect("large reference visible-tail row query outcome");
    let query_elapsed = query_started.elapsed();
    let window = match outcome {
        M11RecursiveGreenRowQueryOutcome::Window(window) => window,
        M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => panic!(
            "large reference row query exhausted {:?}: {:?}",
            exceeded.limit(),
            exceeded.receipt(),
        ),
    };
    let row = window.rows().first().expect("visible-tail Paragraph row");
    assert_eq!(row.kind().get(), 5);
    assert_eq!(
        row.edit_capability(),
        M11RecursiveGreenRowEditCapability::Contiguous
    );
    assert_eq!(
        row.editable_range(),
        Some(literal_start as u64..literal_end as u64)
    );
    assert!(window.receipt().events_scanned() < 3_200);
    assert!(window.receipt().storage_pages_visited() <= 25);
    assert!(window.receipt().node_headers_decoded() <= 512);
    eprintln!(
        "reference_100k build={build_elapsed:?} query={query_elapsed:?} events={} pages={} nodes={}",
        window.receipt().events_scanned(),
        window.receipt().storage_pages_visited(),
        window.receipt().node_headers_decoded(),
    );

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin reference close");
    while !runtime
        .poll_close(64)
        .expect("poll reference close")
        .complete
    {}
}

use flark_comrak_inline_fragment_gate::InlineFragmentError;
use flark_lazy_inline_fact_cache_gate::{
    Adoption, IndexedDocument, LazyInlineController, LeafInlineContext, LeafPresentation,
    ReferenceSnapshot, DEFAULT_LOGICAL_LEAF_BYTES,
};

fn references() -> ReferenceSnapshot {
    ReferenceSnapshot::default()
        .with_symbol("label", true, "/one")
        .with_symbol("x", false, "")
}

fn edited_leaf() -> String {
    let prefix = "Edited **now** with `code`, *style*, and [label]. ";
    let mut result = prefix.to_owned();
    result.push_str(&"x".repeat(DEFAULT_LOGICAL_LEAF_BYTES - result.len()));
    result
}

fn task_leaf() -> String {
    let prefix = "[x] same bytes with **strong**, `code`, and [label]. ";
    let mut result = prefix.to_owned();
    result.push_str(&"t".repeat(DEFAULT_LOGICAL_LEAF_BYTES - result.len()));
    result
}

fn assert_exact_task_markers(
    controller: &mut LazyInlineController,
    document: &IndexedDocument,
    references: &ReferenceSnapshot,
    expected: usize,
) {
    let descriptor = document.directory().descriptor(0).unwrap();
    assert!(matches!(
        controller.presentation(&descriptor, references),
        LeafPresentation::Exact {
            task_list_markers,
            ..
        } if task_list_markers == expected
    ));
}

#[test]
fn ten_mib_document_retains_zero_eager_facts_and_parses_only_the_window() {
    let document = IndexedDocument::ordinary(10 * 1024 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let synthetic = IndexedDocument::synthetic(100 * 1024 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let references = references();
    let mut controller = LazyInlineController::new(512 * 1024, 256);

    let initial = controller.cache().stats();
    assert_eq!(initial.entries, 0);
    assert_eq!(initial.facts, 0);
    assert_eq!(initial.projection_facts, 0);
    assert!(document.directory().len() > 100_000);
    assert!(synthetic.directory().len() > 1_000_000);
    assert!(document.directory().accounted_retained_bytes() < 2 * 1024 * 1024);
    assert!(synthetic.directory().accounted_retained_bytes() < 16 * 1024 * 1024);

    let schedule =
        controller.schedule_window(&document, 50_000..50_040, 20, Some(50_005), &references);
    assert_eq!(schedule.desired_leaves, 80);
    assert_eq!(schedule.queued, 80);
    let pending = document.directory().descriptor(50_005).unwrap();
    assert_eq!(
        controller.presentation(&pending, &references),
        LeafPresentation::SourceVisible { pending: true }
    );
    assert_eq!(
        document.leaf_source(&pending).unwrap().len(),
        DEFAULT_LOGICAL_LEAF_BYTES
    );

    let batch = controller.drain(&document, &references).unwrap();
    assert_eq!(batch.parsed_leaves, 80);
    assert_eq!(batch.parsed_bytes, 80 * DEFAULT_LOGICAL_LEAF_BYTES);
    assert_eq!(batch.adopted, 80);
    assert_eq!(batch.rejected, 0);
    assert!(batch.facts > 80);
    assert!(batch.protocol_output_bytes > batch.parsed_bytes);
    let retained = controller.cache().stats();
    assert_eq!(retained.entries, 80);
    assert!(retained.facts > 0);
    assert!(retained.accounted_bytes <= controller.cache().maximum_bytes());
    assert!(batch.parsed_bytes * 1000 < document.source_len().unwrap());
    assert!(synthetic.directory().coverage().byte >= 100 * 1024 * 1024 - 100);
    assert_eq!(synthetic.source_len(), None);
}

#[test]
fn scroll_and_byte_bounded_eviction_leave_source_as_the_valid_fallback() {
    let document = IndexedDocument::ordinary(2 * 1024 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let references = references();
    let mut controller = LazyInlineController::new(64 * 1024, 128);

    let first = controller.schedule_window(&document, 100..132, 8, None, &references);
    assert_eq!(first.desired_leaves, 48);
    controller.drain(&document, &references).unwrap();
    let old = document.directory().descriptor(100).unwrap();
    let eviction_probe = document.directory().descriptor(101).unwrap();
    assert!(matches!(
        controller.presentation(&old, &references),
        LeafPresentation::Exact { .. }
    ));

    let second = controller.schedule_window(&document, 10_000..10_032, 8, None, &references);
    assert_eq!(second.prior_queue_collapsed, 0);
    assert_eq!(second.queued, 48);
    let scrolled = controller.drain(&document, &references).unwrap();
    assert!(scrolled.evicted > 0);
    let stats = controller.cache().stats();
    assert!(stats.entries < 96);
    assert!(stats.accounted_bytes <= controller.cache().maximum_bytes());
    assert!(stats.evictions > 0);
    assert_eq!(
        controller.presentation(&eviction_probe, &references),
        LeafPresentation::SourceVisible { pending: false }
    );
    assert!(document
        .leaf_source(&eviction_probe)
        .unwrap()
        .starts_with("An ordinary"));
}

#[test]
fn local_edit_rejects_stale_completion_and_latest_window_collapses_the_queue() {
    let mut document = IndexedDocument::ordinary(512 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let references = references();
    let mut controller = LazyInlineController::new(256 * 1024, 128);
    let active_ordinal = 300;

    let first = controller.schedule_window(&document, 0..20, 5, Some(active_ordinal), &references);
    assert_eq!(first.queued, 26);
    let old_job = controller.prepare_next(&document).unwrap().unwrap();
    assert_eq!(
        old_job.version,
        document
            .directory()
            .descriptor(active_ordinal)
            .unwrap()
            .version
    );
    let old_revision = document.revision();

    let replacement = edited_leaf();
    let edited_version = document
        .edit_leaf_same_metrics(active_ordinal, &replacement)
        .unwrap();
    assert_eq!(edited_version.id, old_job.version.id);
    assert_ne!(
        edited_version.content_generation,
        old_job.version.content_generation
    );
    let latest = controller.schedule_window(&document, 0..20, 5, Some(active_ordinal), &references);
    assert_eq!(latest.prior_queue_collapsed, 25);
    assert_eq!(controller.queue_len(), 26);

    let preflight = old_job.clone().run(&references, document.revision());
    assert!(matches!(
        preflight,
        Err(InlineFragmentError::StaleRevision { .. })
    ));
    let stale_completion = old_job.run(&references, old_revision).unwrap();
    assert_eq!(
        controller.adopt(stale_completion, &document, &references),
        Adoption::StaleRevision
    );

    let active = controller.prepare_next(&document).unwrap().unwrap();
    assert_eq!(active.version, edited_version);
    let active_completion = active.run(&references, document.revision()).unwrap();
    assert!(matches!(
        controller.adopt(active_completion, &document, &references),
        Adoption::Adopted { .. }
    ));
    let active_descriptor = document.directory().descriptor(active_ordinal).unwrap();
    assert!(matches!(
        controller.presentation(&active_descriptor, &references),
        LeafPresentation::Exact { .. }
    ));

    for scroll in 0..100 {
        let start = 100 + scroll * 10;
        let receipt =
            controller.schedule_window(&document, start..start + 20, 5, None, &references);
        assert!(receipt.queued <= 30);
        assert!(controller.queue_len() <= 30);
    }
}

#[test]
fn reference_validation_is_lazy_per_cached_leaf_and_never_needs_all_consumers() {
    let mut document = IndexedDocument::ordinary(2 * 1024 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let mut references = references();
    let mut controller = LazyInlineController::new(256 * 1024, 128);
    controller.schedule_window(&document, 0..2, 0, None, &references);
    let batch = controller.drain(&document, &references).unwrap();
    assert_eq!(batch.parsed_leaves, 2);
    assert_eq!(controller.cache().stats().dependencies, 2);
    assert!(document.directory().len() > 20_000);

    references.reset_resolve_calls();
    assert!(references.set_value("label", "/changed-value"));
    document.advance_revision().unwrap();
    let first = document.directory().descriptor(0).unwrap();
    assert!(matches!(
        controller.presentation(&first, &references),
        LeafPresentation::Exact { .. }
    ));
    assert_eq!(references.resolve_calls(), 0);
    assert_eq!(references.value("label"), Some("/changed-value"));

    assert!(references.set_defined("label", false));
    let before = controller.cache().stats();
    assert_eq!(before.entries, 2);
    assert_eq!(
        controller.presentation(&first, &references),
        LeafPresentation::SourceVisible { pending: false }
    );
    let after_one_lookup = controller.cache().stats();
    assert_eq!(after_one_lookup.dependency_invalidations, 1);
    assert_eq!(
        after_one_lookup.entries, 1,
        "other cached consumers were not enumerated"
    );

    let current = controller.schedule_window(&document, 0..1, 0, None, &references);
    assert_eq!(current.queued, 1);
    let reparsed = controller.drain(&document, &references).unwrap();
    assert_eq!(reparsed.parsed_leaves, 1);
    assert_eq!(reparsed.adopted, 1);

    let far = 10_000;
    assert!(far < document.directory().len());
    assert!(references.set_defined("label", true));
    document.advance_revision().unwrap();
    references.reset_resolve_calls();
    let missed = controller.schedule_window(&document, far..far + 1, 0, None, &references);
    assert_eq!(missed.queued, 1);
    assert_eq!(
        references.resolve_calls(),
        0,
        "a cache miss stores no dependency to scan"
    );
    let far_batch = controller.drain(&document, &references).unwrap();
    assert_eq!(far_batch.parsed_leaves, 1);
    assert_eq!(references.resolve_calls(), 1);
}

#[test]
fn completion_dependency_generation_is_checked_again_at_adoption() {
    let document = IndexedDocument::ordinary(64 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let mut references = references();
    let mut controller = LazyInlineController::new(64 * 1024, 64);
    controller.schedule_window(&document, 0..1, 0, None, &references);
    let job = controller.prepare_next(&document).unwrap().unwrap();
    let completion = job.run(&references, document.revision()).unwrap();
    assert!(references.set_defined("label", false));
    assert_eq!(
        controller.adopt(completion, &document, &references),
        Adoption::StaleDependency
    );
    assert_eq!(controller.cache().stats().entries, 0);
}

#[test]
fn unchanged_bytes_never_reuse_facts_across_block_owned_inline_contexts() {
    let mut document =
        IndexedDocument::ordinary(DEFAULT_LOGICAL_LEAF_BYTES, DEFAULT_LOGICAL_LEAF_BYTES);
    let references = references();
    let mut controller = LazyInlineController::new(64 * 1024, 16);
    let task_source = task_leaf();
    let initial = document.edit_leaf_same_metrics(0, &task_source).unwrap();
    let content_generation = initial.content_generation;
    let leaf_id = initial.id;

    let history = [
        (LeafInlineContext::Paragraph, 0),
        (
            LeafInlineContext::ListItemParagraph {
                task_list_certified: true,
            },
            1,
        ),
        (
            LeafInlineContext::ListItemParagraph {
                task_list_certified: false,
            },
            0,
        ),
        (
            LeafInlineContext::Heading {
                level: 2,
                setext: false,
            },
            0,
        ),
        (LeafInlineContext::TableCell, 0),
        (
            LeafInlineContext::ListItemParagraph {
                task_list_certified: true,
            },
            1,
        ),
        (LeafInlineContext::Paragraph, 0),
    ];

    for (index, (context, expected_task_markers)) in history.into_iter().enumerate() {
        if index > 0 {
            let before = controller.cache().stats();
            let version = document.set_leaf_inline_context(0, context).unwrap();
            assert_eq!(version.id, leaf_id);
            assert_eq!(version.content_generation, content_generation);
            assert_eq!(
                document
                    .leaf_source(&document.directory().descriptor(0).unwrap())
                    .unwrap(),
                task_source
            );

            let descriptor = document.directory().descriptor(0).unwrap();
            assert_eq!(
                controller.presentation(&descriptor, &references),
                LeafPresentation::SourceVisible { pending: false },
                "old facts must be withdrawn before replacement parsing for {context:?}",
            );
            assert_eq!(
                controller.queue_len(),
                0,
                "context changes do not parse eagerly"
            );
            assert_eq!(
                controller.cache().stats().context_invalidations,
                before.context_invalidations + 1,
            );
        }

        let schedule = controller.schedule_window(&document, 0..1, 0, None, &references);
        assert_eq!(schedule.cache_hits, 0);
        assert_eq!(schedule.queued, 1);
        let descriptor = document.directory().descriptor(0).unwrap();
        assert_eq!(
            controller.presentation(&descriptor, &references),
            LeafPresentation::SourceVisible { pending: true },
        );
        let job = controller.prepare_next(&document).unwrap().unwrap();
        let completion = job.run(&references, document.revision()).unwrap();
        let adoption = controller.adopt(completion, &document, &references);
        assert!(
            matches!(adoption, Adoption::Adopted { .. }),
            "failed to adopt history step {index} for {context:?}: {adoption:?}"
        );
        assert_exact_task_markers(
            &mut controller,
            &document,
            &references,
            expected_task_markers,
        );
    }
}

#[test]
fn structural_context_change_rejects_in_flight_task_completion() {
    let mut document =
        IndexedDocument::ordinary(DEFAULT_LOGICAL_LEAF_BYTES, DEFAULT_LOGICAL_LEAF_BYTES);
    let references = references();
    let mut controller = LazyInlineController::new(64 * 1024, 16);
    let task_source = task_leaf();
    document.edit_leaf_same_metrics(0, &task_source).unwrap();
    document
        .set_leaf_inline_context(
            0,
            LeafInlineContext::ListItemParagraph {
                task_list_certified: true,
            },
        )
        .unwrap();
    controller.schedule_window(&document, 0..1, 0, None, &references);
    let old_revision = document.revision();
    let old_job = controller.prepare_next(&document).unwrap().unwrap();
    let completion = old_job.run(&references, old_revision).unwrap();

    document
        .set_leaf_inline_context(
            0,
            LeafInlineContext::ListItemParagraph {
                task_list_certified: false,
            },
        )
        .unwrap();
    assert_eq!(
        controller.adopt(completion, &document, &references),
        Adoption::StaleRevision,
    );
    assert_eq!(controller.cache().stats().entries, 0);

    let schedule = controller.schedule_window(&document, 0..1, 0, None, &references);
    assert_eq!(schedule.queued, 1);
    controller.drain(&document, &references).unwrap();
    assert_exact_task_markers(&mut controller, &document, &references, 0);
}

use flark_lazy_inline_fact_cache_gate::{
    run_window_probe, Adoption, IndexedDocument, LazyInlineController, LeafInlineContext,
    LeafPresentation, ReferenceSnapshot, DEFAULT_LOGICAL_LEAF_BYTES,
};

#[unsafe(no_mangle)]
pub extern "C" fn lazy_window_probe(leaves: u32) -> u64 {
    match run_window_probe(leaves as usize) {
        Ok(receipt) => receipt
            .checksum
            .wrapping_add((receipt.facts as u64).rotate_left(17))
            .wrapping_add((receipt.cache_entries as u64).rotate_left(31)),
        Err(_) => u64::MAX,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn lazy_behavior_probe() -> u32 {
    let mut result = 0_u32;
    let mut document = IndexedDocument::ordinary(64 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let mut references = ReferenceSnapshot::default()
        .with_symbol("label", true, "/one")
        .with_symbol("x", false, "");
    let mut controller = LazyInlineController::new(128 * 1024, 64);
    controller.schedule_window(&document, 0..1, 0, None, &references);
    if controller.drain(&document, &references).is_ok() {
        let descriptor = document.directory().descriptor(0).unwrap();
        if matches!(
            controller.presentation(&descriptor, &references),
            LeafPresentation::Exact { .. }
        ) {
            result |= 1;
        }
        references.set_value("label", "/two");
        let _ = document.advance_revision();
        let descriptor = document.directory().descriptor(0).unwrap();
        if matches!(
            controller.presentation(&descriptor, &references),
            LeafPresentation::Exact { .. }
        ) {
            result |= 2;
        }
        references.set_defined("label", false);
        if matches!(
            controller.presentation(&descriptor, &references),
            LeafPresentation::SourceVisible { .. }
        ) {
            result |= 4;
        }
    }

    controller.schedule_window(&document, 10..11, 0, Some(10), &references);
    if let Ok(Some(job)) = controller.prepare_next(&document) {
        let old_revision = document.revision();
        let mut replacement = "Edited [label] ".to_owned();
        replacement.push_str(&"x".repeat(DEFAULT_LOGICAL_LEAF_BYTES - replacement.len()));
        if document.edit_leaf_same_metrics(10, &replacement).is_ok() {
            if let Ok(completion) = job.run(&references, old_revision) {
                if controller.adopt(completion, &document, &references) == Adoption::StaleRevision {
                    result |= 8;
                }
            }
        }
    }

    let mut task_source = "[x] same bytes with **strong**, `code`, and [label]. ".to_owned();
    task_source.push_str(&"t".repeat(DEFAULT_LOGICAL_LEAF_BYTES - task_source.len()));
    if document.edit_leaf_same_metrics(0, &task_source).is_ok()
        && document
            .set_leaf_inline_context(
                0,
                LeafInlineContext::ListItemParagraph {
                    task_list_certified: true,
                },
            )
            .is_ok()
    {
        controller.schedule_window(&document, 0..1, 0, None, &references);
        if controller.drain(&document, &references).is_ok() {
            let first = document.directory().descriptor(0).unwrap();
            let task_was_exact = matches!(
                controller.presentation(&first, &references),
                LeafPresentation::Exact {
                    task_list_markers: 1,
                    ..
                }
            );
            let later_version = document.set_leaf_inline_context(
                0,
                LeafInlineContext::ListItemParagraph {
                    task_list_certified: false,
                },
            );
            let later = document.directory().descriptor(0).unwrap();
            let stale_task_withdrawn = later_version.is_ok()
                && matches!(
                    controller.presentation(&later, &references),
                    LeafPresentation::SourceVisible { pending: false }
                );
            controller.schedule_window(&document, 0..1, 0, None, &references);
            if controller.drain(&document, &references).is_ok() {
                let later = document.directory().descriptor(0).unwrap();
                let later_was_exact = matches!(
                    controller.presentation(&later, &references),
                    LeafPresentation::Exact {
                        task_list_markers: 0,
                        ..
                    }
                );
                if task_was_exact && stale_task_withdrawn && later_was_exact {
                    result |= 16;
                }
            }
        }
    }
    result
}

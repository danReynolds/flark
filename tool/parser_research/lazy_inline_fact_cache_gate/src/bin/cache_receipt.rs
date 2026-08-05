use std::error::Error;
use std::time::Instant;

use flark_lazy_inline_fact_cache_gate::{
    Adoption, CacheGateError, IndexedDocument, LazyInlineController, LeafPresentation,
    ReferenceSnapshot, DEFAULT_LOGICAL_LEAF_BYTES,
};

fn percentile(samples: &mut [u128], percentile: usize) -> u128 {
    samples.sort_unstable();
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

fn replacement() -> String {
    let prefix = "Edited **now** with `code`, *style*, and [label]. ";
    let mut result = prefix.to_owned();
    result.push_str(&"x".repeat(DEFAULT_LOGICAL_LEAF_BYTES - result.len()));
    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let references = ReferenceSnapshot::default().with_symbol("label", true, "/one");
    let build_started = Instant::now();
    let document = IndexedDocument::ordinary(10 * 1024 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let build_elapsed = build_started.elapsed();
    let mut controller = LazyInlineController::new(96 * 1024, 128);
    let initial = controller.cache().stats();
    let schedule_started = Instant::now();
    let schedule =
        controller.schedule_window(&document, 50_000..50_040, 20, Some(75_000), &references);
    let schedule_elapsed = schedule_started.elapsed();

    let mut parse_ns = Vec::new();
    let mut adoption_ns = Vec::new();
    let mut parsed_bytes = 0;
    let mut facts = 0;
    let mut output_bytes = 0;
    let mut adopted = 0;
    while let Some(job) = controller.prepare_next(&document)? {
        let started = Instant::now();
        let completion = job
            .run(&references, document.revision())
            .map_err(CacheGateError::from)?;
        parse_ns.push(started.elapsed().as_nanos());
        parsed_bytes += completion.logical_bytes;
        facts += completion.fragment.facts.len() + completion.fragment.projection_facts.len();
        output_bytes += completion.fragment.output_bytes();
        let started = Instant::now();
        if matches!(
            controller.adopt(completion, &document, &references),
            Adoption::Adopted { .. }
        ) {
            adopted += 1;
        }
        adoption_ns.push(started.elapsed().as_nanos());
    }
    let retained = controller.cache().stats();

    println!("lazy-inline-fact-cache native receipt");
    println!(
        "document source_bytes={} leaves={} descriptor_pages={} descriptor_bytes={} build_us={}",
        document.source_len().unwrap_or(0),
        document.directory().len(),
        document.directory().descriptor_page_count(),
        document.directory().accounted_retained_bytes(),
        build_elapsed.as_micros()
    );
    println!(
        "initial eager_cache_entries={} eager_facts={} eager_projection_facts={}",
        initial.entries, initial.facts, initial.projection_facts
    );
    println!(
        "window desired={} queued={} active_outside_visible=true schedule_ns={} parsed={} adopted={} parsed_bytes={}",
        schedule.desired_leaves,
        schedule.queued,
        schedule_elapsed.as_nanos(),
        parse_ns.len(),
        adopted,
        parsed_bytes
    );
    println!(
        "latency parse_p50_ns={} parse_p99_ns={} adoption_p50_ns={} adoption_p99_ns={}",
        percentile(&mut parse_ns, 50),
        percentile(&mut parse_ns, 99),
        percentile(&mut adoption_ns, 50),
        percentile(&mut adoption_ns, 99)
    );
    println!(
        "density facts={} facts_per_leaf={:.2} facts_per_kib={:.2} protocol_bytes={} protocol_per_source_byte={:.2}",
        facts,
        facts as f64 / adopted as f64,
        facts as f64 * 1024.0 / parsed_bytes as f64,
        output_bytes,
        output_bytes as f64 / parsed_bytes as f64
    );
    println!(
        "retained cache_entries={} cache_bytes={} byte_cap={} facts={} projection_facts={} payload_bytes={} dependencies={}",
        retained.entries,
        retained.accounted_bytes,
        controller.cache().maximum_bytes(),
        retained.facts,
        retained.projection_facts,
        retained.payload_bytes,
        retained.dependencies
    );

    let scroll = controller.schedule_window(&document, 90_000..90_040, 20, None, &references);
    let scroll_batch = controller.drain(&document, &references)?;
    let after_scroll = controller.cache().stats();
    let old = document.directory().descriptor(50_000).unwrap();
    println!(
        "scroll queued={} parsed={} adopted={} evicted={} cache_entries={} old_source_visible={}",
        scroll.queued,
        scroll_batch.parsed_leaves,
        scroll_batch.adopted,
        scroll_batch.evicted,
        after_scroll.entries,
        matches!(
            controller.presentation(&old, &references),
            LeafPresentation::SourceVisible { .. }
        )
    );

    let synthetic_started = Instant::now();
    let synthetic = IndexedDocument::synthetic(100 * 1024 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    println!(
        "synthetic_100mib coverage_bytes={} leaves={} descriptor_pages={} descriptor_bytes={} build_us={} eager_inline_facts=0",
        synthetic.directory().coverage().byte,
        synthetic.directory().len(),
        synthetic.directory().descriptor_page_count(),
        synthetic.directory().accounted_retained_bytes(),
        synthetic_started.elapsed().as_micros()
    );

    let mut edit_document = IndexedDocument::ordinary(64 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let mut edit_controller = LazyInlineController::new(128 * 1024, 64);
    edit_controller.schedule_window(&edit_document, 0..20, 5, Some(300), &references);
    let stale_job = edit_controller.prepare_next(&edit_document)?.unwrap();
    let old_revision = edit_document.revision();
    edit_document.edit_leaf_same_metrics(300, &replacement())?;
    let latest = edit_controller.schedule_window(&edit_document, 0..20, 5, Some(300), &references);
    let stale_completion = stale_job
        .run(&references, old_revision)
        .map_err(CacheGateError::from)?;
    let stale_adoption = edit_controller.adopt(stale_completion, &edit_document, &references);
    println!(
        "latest_wins collapsed={} queue={} stale_adoption={stale_adoption:?}",
        latest.prior_queue_collapsed,
        edit_controller.queue_len()
    );

    let mut reference_document = IndexedDocument::ordinary(64 * 1024, DEFAULT_LOGICAL_LEAF_BYTES);
    let mut reference_snapshot = references.clone();
    let mut reference_controller = LazyInlineController::new(128 * 1024, 64);
    reference_controller.schedule_window(&reference_document, 0..1, 0, None, &reference_snapshot);
    reference_controller.drain(&reference_document, &reference_snapshot)?;
    reference_snapshot.set_value("label", "/value-only");
    reference_document.advance_revision()?;
    let descriptor = reference_document.directory().descriptor(0).unwrap();
    let value_hit = matches!(
        reference_controller.presentation(&descriptor, &reference_snapshot),
        LeafPresentation::Exact { .. }
    );
    reference_snapshot.set_defined("label", false);
    let presence_miss = matches!(
        reference_controller.presentation(&descriptor, &reference_snapshot),
        LeafPresentation::SourceVisible { .. }
    );
    println!(
        "references value_only_cache_hit={value_hit} presence_change_cache_miss={presence_miss} invalidated_cached_leaves={} document_consumers_enumerated=0",
        reference_controller.cache().stats().dependency_invalidations
    );
    Ok(())
}

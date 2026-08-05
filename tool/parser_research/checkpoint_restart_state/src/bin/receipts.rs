use std::env;
use std::sync::Arc;
use std::time::Instant;

use checkpoint_restart_state::{
    run_to_completion, Document, ParseJob, ParseResult, SOURCE_PAGE_BYTES,
};

const TEN_MIB: usize = 10 * 1024 * 1024;
const FUEL: u64 = 4 * 1024;

fn main() {
    let shape = env::args().nth(1).unwrap_or_else(|| "balanced".to_owned());
    match shape.as_str() {
        "balanced" => balanced(),
        "open" => all_open(),
        other => panic!("unknown shape {other:?}; expected balanced or open"),
    }
}

fn balanced() {
    let mut bytes = vec![b'a'; TEN_MIB];
    for page in 0..TEN_MIB / SOURCE_PAGE_BYTES {
        let base = page * SOURCE_PAGE_BYTES;
        bytes[base] = b'[';
        bytes[base + SOURCE_PAGE_BYTES - 1] = b']';
    }
    let old = timed_clean(&bytes, "balanced_clean");
    let edit_at = TEN_MIB / 2 + 127;
    let edit = old.document().edit(edit_at..edit_at + 1, b"z");
    drop(bytes);
    let started = Instant::now();
    let resumed = run_to_completion(ParseJob::incremental(Arc::clone(&old), edit), FUEL);
    let elapsed = started.elapsed();
    print_receipt("balanced_resumed", &resumed, elapsed.as_micros());
    let clean = run_to_completion(ParseJob::clean(resumed.document().clone()), FUEL);
    assert_eq!(
        resumed.canonical_output_bytes(),
        clean.canonical_output_bytes()
    );
    assert!(resumed.checkpoints_exactly_equal(&clean));
    println!("balanced_exact_clean_match=true");
}

fn all_open() {
    let mut bytes = vec![b'a'; TEN_MIB];
    for page in 0..TEN_MIB / SOURCE_PAGE_BYTES {
        bytes[page * SOURCE_PAGE_BYTES] = b'[';
    }
    let old = timed_clean(&bytes, "open_clean");
    let edit = old.document().edit(0..1, b"a");
    drop(bytes);
    let started = Instant::now();
    let resumed = run_to_completion(ParseJob::incremental(Arc::clone(&old), edit), FUEL);
    let elapsed = started.elapsed();
    print_receipt("open_resumed", &resumed, elapsed.as_micros());
    let clean = run_to_completion(ParseJob::clean(resumed.document().clone()), FUEL);
    assert_eq!(
        resumed.canonical_output_bytes(),
        clean.canonical_output_bytes()
    );
    assert!(resumed.checkpoints_exactly_equal(&clean));
    println!("open_exact_clean_match=true");
}

fn timed_clean(bytes: &[u8], label: &str) -> Arc<ParseResult> {
    let started = Instant::now();
    let result = run_to_completion(ParseJob::clean(Document::from_bytes(bytes)), FUEL);
    print_receipt(label, &result, started.elapsed().as_micros());
    result
}

fn print_receipt(label: &str, result: &ParseResult, elapsed_us: u128) {
    let metrics = result.metrics();
    let retained = result.retained_estimate();
    println!(
        "{label}: bytes={} pages={} elapsed_us={elapsed_us} scanned_bytes={} scanned_pages={} \
         prefix_pages={} attached_suffix_pages={} convergence_page={:?} eof_frames={} ticks={} \
         max_work_per_tick={} max_scan_per_tick={} max_eof_per_tick={} retained_graph_bytes={} \
         retained_source_bytes={} retained_stack_bytes={} retained_output_bytes={}",
        result.document().len(),
        result.document().page_count(),
        metrics.source_bytes_scanned,
        metrics.pages_scanned,
        metrics.prefix_pages_copied,
        metrics.suffix_pages_attached,
        metrics.convergence_page,
        metrics.eof_frames_finalized,
        metrics.ticks,
        metrics.max_work_units_per_tick,
        metrics.max_source_bytes_per_tick,
        metrics.max_eof_frames_per_tick,
        retained.total,
        retained.source_bytes,
        retained.persistent_stack,
        retained.output_payload,
    );
}

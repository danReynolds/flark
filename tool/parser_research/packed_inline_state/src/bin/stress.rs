use flark_packed_inline_state::{Engine, Phase, SegmentedSource};
use std::env;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_BYTES: usize = 10 * 1024 * 1024;
const SEGMENT_BYTES: usize = 64 * 1024;

fn main() {
    let pattern = env::args()
        .nth(1)
        .unwrap_or_else(|| "alternating".to_owned());
    let bytes = env::args()
        .nth(2)
        .map_or(DEFAULT_BYTES, |value| value.parse().expect("byte count"));
    let source = patterned_source(&pattern, bytes);

    let started = Instant::now();
    let (old, old_metrics) = parse(source.clone(), 0);
    let old_elapsed = started.elapsed();

    let midpoint = source.len() / 2;
    let edited = source.edit(midpoint..midpoint, "x");
    let shared_segments = edited.shared_segment_count(&source);
    let started = Instant::now();
    let (candidate, candidate_metrics) = parse(edited, old.retained_bytes());
    let candidate_elapsed = started.elapsed();
    let (candidate, reuse) = candidate.with_reusable_suffix(&old);

    println!(
        "pattern={pattern} source_bytes={} old_ms={} candidate_ms={} shared_segments={shared_segments}",
        source.len(),
        old_elapsed.as_millis(),
        candidate_elapsed.as_millis()
    );
    println!(
        "old lexical_events={} lexical_bytes={} lexical_B_per_event={:.3} facts={} fact_bytes={} fact_B_per_event={:.3} stack_payload_peak={} stack_alloc_peak={} checkpoint_root_payload={} checkpoint_root_alloc={} retained={} accounted_peak={}",
        old_metrics.lexical_events,
        old_metrics.lexical_payload_bytes,
        old_metrics.lexical_bytes_per_event(),
        old_metrics.fact_count,
        old_metrics.fact_payload_bytes,
        old_metrics.fact_bytes_per_event(),
        old_metrics.max_stack_payload_bytes,
        old_metrics.max_stack_allocated_bytes,
        old.state_root_payload_bytes(),
        old.state_root_allocated_bytes(),
        old.retained_bytes(),
        old_metrics.peak_accounted_bytes,
    );
    println!(
        "candidate lexical_events={} lexical_bytes={} pending_bytes={} facts={} fact_bytes={} stack_payload_peak={} stack_alloc_peak={} checkpoint_root_payload={} checkpoint_root_alloc={} retained_old={} total_accounted_peak={}",
        candidate_metrics.lexical_events,
        candidate_metrics.lexical_payload_bytes,
        candidate_metrics.pending_payload_bytes,
        candidate_metrics.fact_count,
        candidate_metrics.fact_payload_bytes,
        candidate_metrics.max_stack_payload_bytes,
        candidate_metrics.max_stack_allocated_bytes,
        candidate.state_root_payload_bytes(),
        candidate.state_root_allocated_bytes(),
        candidate_metrics.retained_old_bytes,
        candidate_metrics.peak_accounted_bytes,
    );
    println!(
        "reuse lex_suffix_pages={} lex_payload_bytes={} fact_suffix_pages={} fact_payload_bytes={} state_equal={}",
        reuse.reused_lex_suffix_pages,
        reuse.reused_lex_payload_bytes,
        reuse.reused_fact_suffix_pages,
        reuse.reused_fact_payload_bytes,
        old.state_fingerprint == candidate.state_fingerprint,
    );
}

fn parse(
    source: SegmentedSource,
    retained_old: usize,
) -> (
    flark_packed_inline_state::Checkpoint,
    flark_packed_inline_state::Metrics,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut engine = Engine::new(source, cancel, retained_old);
    while engine.phase() != Phase::Done {
        engine.advance(4096);
    }
    engine.finish().expect("completed")
}

fn patterned_source(pattern: &str, bytes: usize) -> SegmentedSource {
    let unit: &[u8] = match pattern {
        "alternating" => b"*_",
        "emphasis" => b"*a*",
        "brackets" => b"[",
        "run" => b"*",
        _ => panic!("pattern must be alternating, emphasis, brackets, or run"),
    };
    let mut segments = Vec::with_capacity(bytes.div_ceil(SEGMENT_BYTES));
    let mut produced = 0usize;
    while produced < bytes {
        let len = (bytes - produced).min(SEGMENT_BYTES);
        let mut segment = Vec::with_capacity(len);
        for offset in 0..len {
            segment.push(unit[(produced + offset) % unit.len()]);
        }
        segments.push(String::from_utf8(segment).expect("ASCII stress input"));
        produced += len;
    }
    SegmentedSource::from_owned_segments(segments)
}

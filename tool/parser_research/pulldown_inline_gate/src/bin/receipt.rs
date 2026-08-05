use flark_pulldown_inline_gate::{
    CancellationToken, InlineEngine, LogicalLeaf, ParsePoll, ReferenceTable,
};
use std::sync::Arc;
use std::time::Instant;

fn run(name: &str, source: String, fuel: usize) {
    let source_bytes = source.len();
    let mut engine = InlineEngine::new(
        LogicalLeaf::contiguous(source),
        Arc::new(ReferenceTable::new()),
    );
    let cancellation = CancellationToken::default();
    let started = Instant::now();
    loop {
        if matches!(engine.resume(fuel, &cancellation), ParsePoll::Ready { .. }) {
            break;
        }
    }
    let elapsed = started.elapsed();
    let receipt = engine.memory_receipt();
    println!(
        "case={name} source_bytes={source_bytes} elapsed_ms={} polls={} max_poll_work={} tokens={} facts={} plain_runs={} token_capacity_bytes={} fact_capacity_bytes={} total_aux_bytes={}",
        elapsed.as_millis(),
        receipt.polls,
        receipt.max_poll_work,
        receipt.token_count,
        receipt.fact_count,
        engine.plain_run_count(),
        receipt.token_capacity_bytes,
        receipt.fact_capacity_bytes,
        receipt.total_retained_auxiliary_bytes,
    );
}

fn repeat_to_len(pattern: &str, len: usize) -> String {
    let repetitions = len.div_ceil(pattern.len());
    pattern.repeat(repetitions)[..len].to_owned()
}

fn main() {
    const MIB: usize = 1024 * 1024;
    const FUEL: usize = 4096;
    run("plain_10mib", "a".repeat(10 * MIB), FUEL);
    run(
        "giant_code_10mib",
        format!("`{}`", "a".repeat(10 * MIB)),
        FUEL,
    );
    run(
        "delimiter_dense_unmatched_10mib",
        repeat_to_len("_ ", 10 * MIB),
        FUEL,
    );
    run("styled_dense_1mib", repeat_to_len("*a* ", MIB), FUEL);
}

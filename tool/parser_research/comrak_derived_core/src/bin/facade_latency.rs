use std::hint::black_box;
use std::time::Instant;

use comrak::block_spine_facade::{self as facade, MAX_CLASSIFICATION_BYTES};

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map_or(10_000, |value| value.parse::<usize>().expect("iterations"));
    let table = format!("| {} |\n", "x".repeat(MAX_CLASSIFICATION_BYTES - 5));
    let html = format!(
        "<x-tag a=\"{}\">\n",
        "x".repeat(MAX_CLASSIFICATION_BYTES - 13)
    );
    let reference = format!("[x]: /{}\n", "x".repeat(MAX_CLASSIFICATION_BYTES - 7));
    assert_eq!(table.len(), MAX_CLASSIFICATION_BYTES);
    assert_eq!(html.len(), MAX_CLASSIFICATION_BYTES);
    assert_eq!(reference.len(), MAX_CLASSIFICATION_BYTES);

    measure("table-row", iterations, || {
        black_box(facade::table_row(black_box(&table), false).unwrap());
    });
    measure("html-type7-start", iterations, || {
        black_box(facade::html_block_start(black_box(&html), true).unwrap());
    });
    measure("reference-definition", iterations, || {
        black_box(facade::reference_definitions(black_box(&reference)).unwrap());
    });
}

fn measure(name: &str, iterations: usize, mut operation: impl FnMut()) {
    for _ in 0..128 {
        operation();
    }
    let mut samples = Vec::with_capacity(iterations);
    let total = Instant::now();
    for _ in 0..iterations {
        let started = Instant::now();
        operation();
        samples.push(started.elapsed().as_nanos() as u64);
    }
    let total = total.elapsed();
    samples.sort_unstable();
    let percentile = |value: usize| samples[value * (samples.len() - 1) / 1_000];
    println!(
        "name={name} bytes={MAX_CLASSIFICATION_BYTES} iterations={iterations} total_us={} p50_ns={} p95_ns={} p99_ns={} p999_ns={} max_ns={}",
        total.as_micros(),
        percentile(500),
        percentile(950),
        percentile(990),
        percentile(999),
        samples[samples.len() - 1],
    );
}

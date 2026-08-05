use std::time::Instant;

use comrak::{parse_document, Arena};
use flark_bounded_comrak_challenger::gfm_options;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let shape = args.get(1).map(String::as_str).unwrap_or("dense");
    let bytes = args
        .get(2)
        .map(|value| value.parse::<usize>().expect("bytes must be an integer"))
        .unwrap_or(8_192);
    let iterations = args
        .get(3)
        .map(|value| {
            value
                .parse::<usize>()
                .expect("iterations must be an integer")
        })
        .unwrap_or(500);
    let source = generate(shape, bytes);
    let options = gfm_options();

    // Warm code and allocator paths before collecting tails.
    for _ in 0..20 {
        let arena = Arena::new();
        std::hint::black_box(parse_document(&arena, &source, &options));
    }

    let mut elapsed = Vec::with_capacity(iterations);
    let mut nodes = 0usize;
    for _ in 0..iterations {
        let arena = Arena::new();
        let started = Instant::now();
        let root = parse_document(&arena, &source, &options);
        elapsed.push(started.elapsed().as_nanos() as usize);
        nodes = root.descendants().count();
    }
    elapsed.sort_unstable();
    println!(
        "inline-cap shape={shape} requested_bytes={bytes} source_bytes={} iterations={iterations} nodes={nodes} p50_ns={} p95_ns={} p99_ns={} max_ns={}",
        source.len(),
        percentile(&elapsed, 50),
        percentile(&elapsed, 95),
        percentile(&elapsed, 99),
        elapsed.last().copied().unwrap_or_default(),
    );
}

fn generate(shape: &str, target: usize) -> String {
    let atom = match shape {
        "dense" => "a **strong** b *emph* c `code` ",
        "unmatched" => "*a_ [b ![c ~~d ",
        "links" => "[label](https://example.com) [use][missing] ",
        other => panic!("unknown shape {other:?}"),
    };
    let mut source = String::with_capacity(target + atom.len());
    while source.len() < target {
        source.push_str(atom);
    }
    source.push('\n');
    source
}

fn percentile(values: &[usize], percentile: usize) -> usize {
    values[(values.len() - 1) * percentile / 100]
}

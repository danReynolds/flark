use std::time::{Duration, Instant};

use flark_engine::{OpeningSourceStore, SourceRevision, SOURCE_SEED_PAGE_MAX_UTF16};

const FIRST_FRONTIER_BYTES: usize = 512 * 1024;

struct Receipt {
    size_mib: usize,
    first_frontier_bytes: usize,
    first_frontier: Duration,
    seal: Duration,
    mib_per_second: f64,
    generations: u64,
    seal_reused_root: bool,
}

fn run(size_mib: usize) -> Receipt {
    let total = size_mib * 1024 * 1024;
    let page = "x".repeat(SOURCE_SEED_PAGE_MAX_UTF16);
    let mut opening = OpeningSourceStore::new(SourceRevision::new(1), total).expect("opening");
    let started = Instant::now();
    let mut first_frontier = None;
    let mut edited = false;

    while opening.version().admitted_input_utf16() < total {
        let version = opening.version();
        let start = version.admitted_input_utf16();
        let count = SOURCE_SEED_PAGE_MAX_UTF16.min(total - start);
        opening
            .append_page(version, start..start + count, &page[..count])
            .expect("append page");
        if first_frontier.is_none()
            && opening.version().admitted_input_bytes() >= FIRST_FRONTIER_BYTES
        {
            first_frontier = Some(started.elapsed());
        }
        if !edited && opening.version().admitted_input_bytes() >= FIRST_FRONTIER_BYTES {
            let version = opening.version();
            opening
                .apply_utf16_edit(version, 0..1, "z")
                .expect("edit admitted prefix");
            edited = true;
        }
    }

    let before_seal = opening.version();
    let source = opening.seal().expect("seal");
    let seal = started.elapsed();
    assert_eq!(source.version().byte_len(), total);
    assert_eq!(source.version().revision(), SourceRevision::new(2));
    Receipt {
        size_mib,
        first_frontier_bytes: FIRST_FRONTIER_BYTES.min(total),
        first_frontier: first_frontier.unwrap_or(seal),
        seal,
        mib_per_second: size_mib as f64 / seal.as_secs_f64(),
        generations: before_seal.generation(),
        seal_reused_root: source.version().root() == before_seal.root(),
    }
}

fn main() {
    let sizes = std::env::args()
        .skip(1)
        .map(|argument| argument.parse::<usize>().expect("size is an integer MiB"))
        .collect::<Vec<_>>();
    let sizes = if sizes.is_empty() {
        vec![1, 10, 40]
    } else {
        sizes
    };
    let receipts = sizes.into_iter().map(run).collect::<Vec<_>>();
    println!("[");
    for (index, receipt) in receipts.iter().enumerate() {
        println!(
            "  {{\"size_mib\":{},\"first_frontier_bytes\":{},\"first_frontier_ms\":{:.3},\"seal_ms\":{:.3},\"mib_per_second\":{:.3},\"generations\":{},\"seal_reused_root\":{}}}{}",
            receipt.size_mib,
            receipt.first_frontier_bytes,
            receipt.first_frontier.as_secs_f64() * 1000.0,
            receipt.seal.as_secs_f64() * 1000.0,
            receipt.mib_per_second,
            receipt.generations,
            receipt.seal_reused_root,
            if index + 1 == receipts.len() { "" } else { "," },
        );
    }
    println!("]");
}

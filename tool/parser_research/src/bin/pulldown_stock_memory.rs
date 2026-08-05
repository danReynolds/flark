//! External-RSS receipt for stock Pulldown 0.13.4 representations.
//!
//! Run each shape in a fresh process so `/usr/bin/time -l` supplies an
//! allocator-independent maximum resident-set measurement:
//!
//! ```sh
//! /usr/bin/time -l cargo run --release --bin pulldown_stock_memory -- dense-lines
//! /usr/bin/time -l cargo run --release --bin pulldown_stock_memory -- dense-inline
//! /usr/bin/time -l cargo run --release --bin pulldown_stock_memory -- giant-line
//! ```

use std::hint::black_box;
use std::time::Instant;

use pulldown_cmark::{Options, Parser};

fn main() {
    let shape = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dense-lines".to_owned());
    let source = match shape.as_str() {
        "dense-lines" => "a\n".repeat(1_000_000),
        "dense-inline" => {
            let pattern = "*a_ `code` [x](https://example.com) ";
            pattern.repeat(28_000)
        }
        "giant-line" => format!("{}\n", "x".repeat(10 * 1024 * 1024)),
        _ => panic!("unknown shape {shape:?}"),
    };

    let started = Instant::now();
    let parser = Parser::new_ext(
        black_box(source.as_str()),
        Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_GFM,
    );
    let constructed = started.elapsed();
    let iter_started = Instant::now();
    let mut events = 0usize;
    let mut covered_bytes = 0usize;
    for (event, range) in parser.into_offset_iter() {
        black_box(event);
        covered_bytes = covered_bytes.wrapping_add(range.len());
        events += 1;
    }
    println!(
        "pulldown_stock shape={shape} source_bytes={} events={events} range_bytes={covered_bytes} construct_us={} iterate_us={}",
        source.len(),
        constructed.as_micros(),
        iter_started.elapsed().as_micros(),
    );
}

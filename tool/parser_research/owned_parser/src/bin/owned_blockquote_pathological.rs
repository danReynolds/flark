use std::hint::black_box;
use std::time::Instant;

use flark_owned_parser_trial::parse;

fn main() {
    for depth in [100usize, 500, 1_000, 2_000, 5_000] {
        let input = format!("{}a", "> ".repeat(depth));
        let started = Instant::now();
        let document = parse(black_box(&input));
        println!(
            "owned_pathological case=nested_block_quotes depth={depth} bytes={} elapsed_us={}",
            input.len(),
            started.elapsed().as_micros(),
        );
        std::mem::forget(document);
    }
}

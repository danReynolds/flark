use std::hint::black_box;
use std::time::Instant;

use flark_owned_parser_trial::parse;

fn main() {
    for n in [5_000usize, 50_000, 250_000] {
        let input = "*a_ ".repeat(n);
        let started = Instant::now();
        let document = parse(black_box(&input));
        println!(
            "owned_pathological case=unmatched_mixed_emphasis bytes={} blocks={} root_inlines={} elapsed_us={}",
            input.len(),
            document.blocks.len(),
            document.blocks.first().map_or(0, |block| block.inlines.len()),
            started.elapsed().as_micros()
        );
    }
}

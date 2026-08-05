use std::hint::black_box;
use std::time::Instant;

use flark_owned_parser_trial::parse;

fn main() {
    for runs in [50usize, 100, 200, 400, 800, 1_600, 2_500] {
        let mut input = String::new();
        for length in 1..runs {
            input.push('e');
            input.extend(std::iter::repeat_n('`', length));
        }
        let started = Instant::now();
        black_box(parse(black_box(&input)));
        println!(
            "owned_pathological case=increasing_backtick_runs runs={runs} bytes={} elapsed_us={}",
            input.len(),
            started.elapsed().as_micros(),
        );
    }
}

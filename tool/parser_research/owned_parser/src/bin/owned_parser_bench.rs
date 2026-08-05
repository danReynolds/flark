use std::hint::black_box;
use std::time::Instant;

use flark_owned_parser_trial::parse;

fn main() {
    for target in [100 * 1024usize, 1024 * 1024] {
        for (name, source) in [
            ("typical", typical_document(target)),
            ("giant_inline", giant_inline(target)),
            ("fenced_code", giant_fenced_code(target)),
        ] {
            let count = if target < 1024 * 1024 { 21 } else { 9 };
            let samples = samples(count, || {
                black_box(parse(black_box(&source)));
            });
            println!(
                "owned_full_parse shape={name} bytes={} p50_us={} p95_us={}",
                source.len(),
                percentile(&samples, 50),
                percentile(&samples, 95),
            );
        }
    }
}

fn samples(mut count: usize, mut operation: impl FnMut()) -> Vec<u128> {
    operation();
    let mut values = Vec::with_capacity(count);
    while count > 0 {
        let started = Instant::now();
        operation();
        values.push(started.elapsed().as_micros());
        count -= 1;
    }
    values.sort_unstable();
    values
}

fn percentile(values: &[u128], percentile: usize) -> u128 {
    values[(values.len() - 1) * percentile / 100]
}

fn typical_document(target: usize) -> String {
    let mut output = String::with_capacity(target + 256);
    let mut section = 0;
    while output.len() < target {
        output.push_str(&format!(
            "## Section {section}\n\nParagraph with **bold**, *emphasis*, `code`, and [link][shared].\n\n- [ ] item one\n- item two\n\n| a | b |\n| - | - |\n| x | y |\n\n"
        ));
        section += 1;
    }
    output.push_str("[shared]: https://example.com\n");
    output
}

fn giant_inline(target: usize) -> String {
    let mut output = String::with_capacity(target + 128);
    while output.len() < target {
        output.push_str("words **bold** *em* `code` [link](https://example.com) ");
    }
    output
}

fn giant_fenced_code(target: usize) -> String {
    let mut output = String::with_capacity(target + 64);
    output.push_str("```text\n");
    while output.len() + 5 < target {
        output.push_str("code text that is deliberately not parsed as inline markdown\n");
    }
    output.push_str("```\n");
    output
}

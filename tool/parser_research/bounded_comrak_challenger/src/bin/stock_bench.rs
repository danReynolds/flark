use std::fmt::Write as _;
use std::time::Instant;

use comrak::{parse_document, Arena};
use flark_bounded_comrak_challenger::gfm_options;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let shape = args.get(1).map(String::as_str).unwrap_or("paragraph");
    let mib = args
        .get(2)
        .map(|value| value.parse::<usize>().expect("MiB must be an integer"))
        .unwrap_or(1);
    let source = generate(shape, mib * 1_048_576);
    let arena = Arena::new();
    let started = Instant::now();
    let root = parse_document(&arena, &source, &gfm_options());
    let parse_us = started.elapsed().as_micros();
    let nodes = root.descendants().count();
    println!(
        "stock shape={shape} target_mib={mib} source_bytes={} parse_us={parse_us} nodes={nodes}",
        source.len()
    );
    std::hint::black_box(root);
}

fn generate(shape: &str, target: usize) -> String {
    let mut source = String::with_capacity(target + 4_096);
    match shape {
        "many-small" => {
            let mut block = 0usize;
            while source.len() < target {
                write!(source, "ordinary block {block} has **strong** words ").unwrap();
                for _ in 0..128 {
                    source.push_str("and plain payload that stays comfortably bounded ");
                }
                source.push_str("\n\n");
                block += 1;
            }
        }
        "paragraph" => {
            while source.len() < target {
                source.push_str("ordinary words with **strong** emphasis and plain payload ");
            }
            source.push('\n');
        }
        "fence" => {
            source.push_str("```text\n");
            while source.len() < target {
                source.push_str("code payload with no closing fence marker on this line\n");
            }
            source.push_str("```\n");
        }
        "list" => {
            let mut item = 0usize;
            while source.len() < target {
                writeln!(
                    source,
                    "- item {item} has **strong** words and ordinary payload"
                )
                .unwrap();
                item += 1;
            }
        }
        "table" => {
            source.push_str("| name | value |\n| --- | ---: |\n");
            let mut row = 0usize;
            while source.len() < target {
                writeln!(source, "| ordinary row {row} | **strong** payload {row} |").unwrap();
                row += 1;
            }
        }
        other => panic!("unknown shape {other:?}"),
    }
    source
}

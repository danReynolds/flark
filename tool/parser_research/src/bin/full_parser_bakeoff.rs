//! Coarse full-AST cost comparison for parser-fork triage.
//!
//! Incremental behavior remains the deciding property. This receipt only
//! checks that a candidate does not introduce an obviously unacceptable cold
//! or pathological full-parse baseline.

use std::hint::black_box;
use std::time::Instant;

use comrak::{parse_document, Arena, Options as ComrakOptions};
use markdown::{to_mdast, ParseOptions};
use pulldown_cmark::{Options as PulldownOptions, Parser as PulldownParser};

fn main() {
    for (name, source) in [
        ("typical", typical_document(100 * 1024)),
        ("giant_inline", giant_inline(100 * 1024)),
    ] {
        let comrak = samples(11, || {
            let arena = Arena::new();
            let root = parse_document(&arena, black_box(&source), &comrak_options());
            black_box(root.descendants().count());
        });
        let markdown_rs = samples(11, || {
            black_box(
                to_mdast(black_box(&source), &ParseOptions::gfm()).expect("markdown-rs parse"),
            );
        });
        let pulldown = samples(11, || {
            black_box(PulldownParser::new_ext(black_box(&source), pulldown_options()).count());
        });
        println!(
            "full_parser shape={name} bytes={} comrak_p50_us={} comrak_p95_us={} pulldown_p50_us={} pulldown_p95_us={} markdown_rs_p50_us={} markdown_rs_p95_us={}",
            source.len(),
            percentile(&comrak, 50),
            percentile(&comrak, 95),
            percentile(&pulldown, 50),
            percentile(&pulldown, 95),
            percentile(&markdown_rs, 50),
            percentile(&markdown_rs, 95),
        );
    }
}

fn pulldown_options() -> PulldownOptions {
    PulldownOptions::ENABLE_TABLES
        | PulldownOptions::ENABLE_STRIKETHROUGH
        | PulldownOptions::ENABLE_TASKLISTS
}

fn samples(mut count: usize, mut operation: impl FnMut()) -> Vec<u128> {
    for _ in 0..1 {
        operation();
    }
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

fn comrak_options() -> ComrakOptions<'static> {
    let mut options = ComrakOptions::default();
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;
    options
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

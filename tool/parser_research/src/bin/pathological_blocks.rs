use std::time::Instant;

use comrak::{parse_document, Arena, Options};

fn main() {
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;

    probe("nested_emphasis_5000", nested_emphasis(5_000), &options);

    for target in [1_000_000usize, 5_000_000, 10_000_000] {
        probe("plain_paragraph", giant_plain_paragraph(target), &options);
        probe("fenced_code", giant_fenced_code(target), &options);
        probe("single_list", giant_list(target), &options);
        probe(
            "unmatched_mixed_emphasis",
            "*a_ ".repeat(target / 4),
            &options,
        );
    }
}

fn nested_emphasis(depth: usize) -> String {
    format!("{}{}", "*a **a ".repeat(depth), " a** a*".repeat(depth))
}

fn probe(name: &str, markdown: String, options: &Options<'_>) {
    // One untimed warmup at 1KB keeps dynamic library/page startup noise out
    // without doubling the memory of the pathological input.
    let warmup_arena = Arena::new();
    let _ = parse_document(&warmup_arena, "warmup **text**\n", options);

    let arena = Arena::new();
    let started = Instant::now();
    let root = parse_document(&arena, &markdown, options);
    let elapsed = started.elapsed();
    let nodes = root.descendants().count();
    println!(
        "pathological_parse case={name} bytes={} nodes={nodes} elapsed_us={}",
        markdown.len(),
        elapsed.as_micros(),
    );
}

fn giant_plain_paragraph(target: usize) -> String {
    let mut output = String::with_capacity(target + 64);
    while output.len() < target {
        output.push_str("ordinary words with occasional **bold** and [link](https://example.com) ");
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

fn giant_list(target: usize) -> String {
    let mut output = String::with_capacity(target + 64);
    let mut index = 0;
    while output.len() < target {
        output.push_str("- item ");
        output.push_str(&index.to_string());
        output.push_str(" with **bold** text and [shared][shared]\n");
        index += 1;
    }
    output.push_str("\n[shared]: https://example.com\n");
    output
}

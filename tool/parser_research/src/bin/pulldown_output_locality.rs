//! Measures semantic output propagation in stock pulldown-cmark.
//!
//! This is not an incremental implementation. It uses clean parses to define
//! the smallest top-level output envelope that a persistent derivative would
//! have to replace or invalidate after representative edits.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::Instant;

use pulldown_cmark::{Event, Options, Parser};

#[derive(Debug)]
struct Receipt {
    range: Range<usize>,
    semantic: u64,
}

fn main() {
    let original = large_document(1_000_000);
    let (baseline, baseline_us) = snapshot(&original);
    println!(
        "pulldown_output_baseline bytes={} blocks={} clean_us={baseline_us}",
        original.len(),
        baseline.len(),
    );

    let middle = section_count(&original) / 2;
    let cases = [
        (
            "paragraph_character",
            format!("Paragraph {middle} has"),
            format!("Paragraph {middle} now has"),
        ),
        (
            "inline_delimiter",
            format!("unique words {middle}"),
            format!("unique **words** {middle}"),
        ),
        (
            "remove_middle_fence_close",
            format!("code line {middle}\n```\n"),
            format!("code line {middle}\n   \n"),
        ),
        (
            "change_list_indentation",
            format!("  - nested item {middle}"),
            format!("    - nested item {middle}"),
        ),
        (
            "break_table_delimiter",
            format!("| --- | ---: |\n| cell {middle}"),
            format!("| -- | ---: |\n| cell {middle}"),
        ),
        (
            "open_html_comment_near_start",
            "<!-- bounded comment -->".to_owned(),
            "<!-- unbounded comment".to_owned(),
        ),
        (
            "change_global_reference",
            "[shared]: https://example.com/old".to_owned(),
            "[shared]: https://example.com/new".to_owned(),
        ),
    ];

    for (name, needle, replacement) in cases {
        let edited = original.replacen(&needle, &replacement, 1);
        assert_ne!(edited, original, "missing edit needle for {name}");
        let (current, clean_us) = snapshot(&edited);
        let delta = delta(&baseline, &current);
        println!(
            "pulldown_output case={name} delta_bytes={} clean_us={clean_us} changed_blocks={} envelope_bytes={}",
            edited.len() as isize - original.len() as isize,
            delta.0,
            delta.1,
        );
    }
}

fn snapshot(markdown: &str) -> (Vec<Receipt>, u128) {
    let started = Instant::now();
    let parser = Parser::new_ext(markdown, options()).into_offset_iter();
    let mut receipts = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<(usize, usize, DefaultHasher)> = None;

    for (event, range) in parser {
        let is_start = matches!(event, Event::Start(_));
        let is_end = matches!(event, Event::End(_));
        if depth == 0 && current.is_none() {
            current = Some((range.start, range.end, DefaultHasher::new()));
        }
        if let Some((start, end, hasher)) = current.as_mut() {
            *start = (*start).min(range.start);
            *end = (*end).max(range.end);
            format!("{event:?}").hash(hasher);
        }
        if is_start {
            depth += 1;
        }
        if is_end {
            depth = depth.saturating_sub(1);
        }
        if depth == 0 {
            let (start, end, hasher) = current.take().expect("active receipt");
            receipts.push(Receipt {
                range: start..end,
                semantic: hasher.finish(),
            });
        }
    }
    (receipts, started.elapsed().as_micros())
}

fn delta(before: &[Receipt], after: &[Receipt]) -> (usize, usize) {
    let mut prefix = 0;
    while prefix < before.len()
        && prefix < after.len()
        && before[prefix].semantic == after[prefix].semantic
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < before.len().saturating_sub(prefix)
        && suffix < after.len().saturating_sub(prefix)
        && before[before.len() - 1 - suffix].semantic == after[after.len() - 1 - suffix].semantic
    {
        suffix += 1;
    }
    let changed = after.len().saturating_sub(prefix + suffix);
    let envelope = if changed == 0 {
        0
    } else {
        let first = &after[prefix];
        let last = &after[after.len() - suffix - 1];
        last.range.end.saturating_sub(first.range.start)
    };
    (changed, envelope)
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

fn large_document(target: usize) -> String {
    let mut output = String::from("<!-- bounded comment -->\n\n");
    let mut section = 0;
    while output.len() < target.saturating_sub(128) {
        output.push_str(&format!(
            concat!(
                "## Section {section}\n\n",
                "Paragraph {section} has *emphasis*, [a shared link][shared], and unique words {section}.\n\n",
                "- list item {section}\n",
                "  - nested item {section}\n\n",
                "| key | value |\n",
                "| --- | ---: |\n",
                "| cell {section} | {section} |\n\n",
                "```rust\n",
                "let section = {section}; // code line {section}\n",
                "```\n\n",
            ),
            section = section,
        ));
        section += 1;
    }
    output.push_str("[shared]: https://example.com/old\n");
    output
}

fn section_count(markdown: &str) -> usize {
    markdown.matches("## Section ").count()
}

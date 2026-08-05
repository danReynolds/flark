//! Measures how edits propagate through an authoritative full Comrak parse.
//!
//! This is deliberately not an incremental parser. Full parses are used as an
//! oracle so we can distinguish block-structure invalidation from semantic
//! invalidation before choosing an incremental representation.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::{Duration, Instant};

use comrak::nodes::AstNode;
use comrak::{parse_document, Arena, Options};

const TARGET_BYTES: usize = 1_000_000;

#[derive(Clone, Debug)]
struct BlockReceipt {
    range: Range<usize>,
    structural: u64,
    semantic: u64,
}

#[derive(Debug)]
struct Snapshot {
    blocks: Vec<BlockReceipt>,
    elapsed: Duration,
}

#[derive(Clone)]
struct EditCase {
    name: &'static str,
    needle: String,
    replacement: String,
}

fn main() {
    let markdown = large_document(TARGET_BYTES);
    let baseline = snapshot(&markdown);
    println!(
        "baseline bytes={} blocks={} snapshot_us={}",
        markdown.len(),
        baseline.blocks.len(),
        baseline.elapsed.as_micros(),
    );

    for case in edit_cases(&markdown) {
        let edited = replace_once(&markdown, &case.needle, &case.replacement);
        let current = snapshot(&edited);
        let structural = delta(&baseline.blocks, &current.blocks, |block| block.structural);
        let semantic = delta(&baseline.blocks, &current.blocks, |block| block.semantic);
        println!(
            "case={} delta_bytes={} snapshot_us={} structural_changed={} structural_envelope_bytes={} semantic_changed={} semantic_envelope_bytes={}",
            case.name,
            edited.len() as isize - markdown.len() as isize,
            current.elapsed.as_micros(),
            structural.changed_blocks,
            structural.envelope_bytes,
            semantic.changed_blocks,
            semantic.envelope_bytes,
        );
    }
}

#[derive(Debug)]
struct Delta {
    changed_blocks: usize,
    envelope_bytes: usize,
}

fn delta(
    before: &[BlockReceipt],
    after: &[BlockReceipt],
    fingerprint: impl Fn(&BlockReceipt) -> u64,
) -> Delta {
    let mut prefix = 0;
    while prefix < before.len()
        && prefix < after.len()
        && fingerprint(&before[prefix]) == fingerprint(&after[prefix])
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < before.len().saturating_sub(prefix)
        && suffix < after.len().saturating_sub(prefix)
        && fingerprint(&before[before.len() - 1 - suffix])
            == fingerprint(&after[after.len() - 1 - suffix])
    {
        suffix += 1;
    }

    let changed_blocks = after.len().saturating_sub(prefix + suffix);
    let envelope_bytes = if changed_blocks == 0 {
        0
    } else {
        let first = &after[prefix];
        let last = &after[after.len() - suffix - 1];
        last.range.end.saturating_sub(first.range.start)
    };
    Delta {
        changed_blocks,
        envelope_bytes,
    }
}

fn snapshot(markdown: &str) -> Snapshot {
    let started = Instant::now();
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &gfm_options());
    let line_starts = line_starts(markdown);
    let blocks = root
        .children()
        .map(|node| block_receipt(node, markdown, &line_starts))
        .collect();
    Snapshot {
        blocks,
        elapsed: started.elapsed(),
    }
}

fn block_receipt<'a>(node: &'a AstNode<'a>, markdown: &str, line_starts: &[usize]) -> BlockReceipt {
    let range = node_range(node, markdown.len(), line_starts);

    let mut structural_hasher = DefaultHasher::new();
    hash_structure(node, markdown, line_starts, &mut structural_hasher);

    let mut semantic_hasher = DefaultHasher::new();
    hash_semantics(node, &mut semantic_hasher);

    BlockReceipt {
        range,
        structural: structural_hasher.finish(),
        semantic: semantic_hasher.finish(),
    }
}

fn hash_structure<'a>(
    node: &'a AstNode<'a>,
    markdown: &str,
    line_starts: &[usize],
    hasher: &mut DefaultHasher,
) {
    let data = node.data.borrow();
    std::mem::discriminant(&data.value).hash(hasher);
    let range = sourcepos_range(data.sourcepos, markdown.len(), line_starts);
    markdown[range].hash(hasher);
    drop(data);
    for child in node.children() {
        hash_structure(child, markdown, line_starts, hasher);
    }
}

fn hash_semantics<'a>(node: &'a AstNode<'a>, hasher: &mut DefaultHasher) {
    format!("{:?}", node.data.borrow().value).hash(hasher);
    for child in node.children() {
        hash_semantics(child, hasher);
    }
}

fn node_range<'a>(node: &'a AstNode<'a>, len: usize, line_starts: &[usize]) -> Range<usize> {
    sourcepos_range(node.data.borrow().sourcepos, len, line_starts)
}

fn sourcepos_range(
    sourcepos: comrak::nodes::Sourcepos,
    len: usize,
    line_starts: &[usize],
) -> Range<usize> {
    let start_line = sourcepos.start.line.saturating_sub(1);
    let end_line = sourcepos.end.line.saturating_sub(1);
    let start = line_starts
        .get(start_line)
        .copied()
        .unwrap_or(len)
        .saturating_add(sourcepos.start.column.saturating_sub(1))
        .min(len);
    let end = line_starts
        .get(end_line)
        .copied()
        .unwrap_or(len)
        .saturating_add(sourcepos.end.column)
        .min(len);
    start..end.max(start)
}

fn line_starts(markdown: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (offset, byte) in markdown.bytes().enumerate() {
        if byte == b'\n' && offset + 1 < markdown.len() {
            starts.push(offset + 1);
        }
    }
    starts
}

fn gfm_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;
    options
}

fn edit_cases(markdown: &str) -> Vec<EditCase> {
    let middle_section = section_count(markdown) / 2;
    vec![
        EditCase {
            name: "paragraph_character",
            needle: format!("Paragraph {middle_section} has"),
            replacement: format!("Paragraph {middle_section} now has"),
        },
        EditCase {
            name: "inline_delimiter",
            needle: format!("unique words {middle_section}"),
            replacement: format!("unique **words** {middle_section}"),
        },
        EditCase {
            name: "paragraph_newline",
            needle: format!("and unique words {middle_section}."),
            replacement: format!("and unique\nwords {middle_section}."),
        },
        EditCase {
            name: "remove_middle_fence_close",
            needle: format!("code line {middle_section}\n```\n"),
            replacement: format!("code line {middle_section}\n   \n"),
        },
        EditCase {
            name: "change_list_indentation",
            needle: format!("  - nested item {middle_section}"),
            replacement: format!("    - nested item {middle_section}"),
        },
        EditCase {
            name: "break_table_delimiter",
            needle: format!("| --- | ---: |\n| cell {middle_section}"),
            replacement: format!("| -- | ---: |\n| cell {middle_section}"),
        },
        EditCase {
            name: "open_html_comment_near_start",
            needle: "<!-- bounded comment -->".to_string(),
            replacement: "<!-- unbounded comment".to_string(),
        },
        EditCase {
            name: "change_global_reference",
            needle: "[shared]: https://example.com/old".to_string(),
            replacement: "[shared]: https://example.com/new".to_string(),
        },
    ]
}

fn large_document(target_bytes: usize) -> String {
    let mut output = String::from("<!-- bounded comment -->\n\n");
    let mut section = 0;
    while output.len() < target_bytes.saturating_sub(128) {
        output.push_str(&format!(
            concat!(
                "## Section {section}\n\n",
                "Paragraph {section} has *emphasis*, [a shared link][shared], ",
                "and unique words {section}.\n\n",
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

fn replace_once(markdown: &str, needle: &str, replacement: &str) -> String {
    let offset = markdown
        .find(needle)
        .unwrap_or_else(|| panic!("missing edit needle: {needle:?}"));
    let mut edited = String::with_capacity(markdown.len() + replacement.len());
    edited.push_str(&markdown[..offset]);
    edited.push_str(replacement);
    edited.push_str(&markdown[offset + needle.len()..]);
    edited
}

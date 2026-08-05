use flark_pulldown_derived_core::{Document, Fuel};

fn main() {
    let giant = format!("{}\n", "x".repeat(10_000_000));
    let giant = Document::parse(giant, Fuel::bytes(4096)).expect("giant line");
    println!("giant_line_10mb {:?}", giant.memory_receipt());

    let dense = Document::parse("a\n".repeat(1_000_000), Fuel::bytes(64 * 1024))
        .expect("million-line paragraph");
    println!("dense_million_lines {:?}", dense.memory_receipt());

    let mut markdown = String::new();
    for section in 0..1_200 {
        markdown.push_str(&format!(
            "Section {section:04} word-{section:04}\n----\n\n> quote {section:04}\n\n- item {section:04}\n\n```txt\ncode {section:04}\n```\n\n"
        ));
    }
    let mut document = Document::parse(markdown, Fuel::bytes(1024)).expect("edit-history fixture");
    let mut converged = 0;
    let mut reused_suffix_chunks = 0;
    let mut max_reparsed = 0;
    let mut max_delta_chunks = 0;
    for edit in 0..250 {
        let section = (edit * 37) % 1_200;
        let needle = format!("word-{section:04}");
        let start = document.source().find(&needle).expect("edit target") + 5;
        let replacement = if edit % 3 == 0 { "X" } else { "YY" };
        let receipt = document
            .apply_edit(start..start + 1, replacement, Fuel::bytes(257))
            .expect("resumed edit");
        converged += usize::from(receipt.converged);
        reused_suffix_chunks += receipt.reused_suffix_chunks;
        max_reparsed = max_reparsed.max(receipt.reparsed_bytes);
        max_delta_chunks = max_delta_chunks.max(receipt.delta.new_chunks.len());
        let clean = Document::parse(document.source(), Fuel::bytes(8192)).expect("clean oracle");
        assert_eq!(document.semantic_snapshot(), clean.semantic_snapshot());
    }
    println!(
        "edit_history edits=250 exact=250 converged={converged} reused_suffix_chunks={reused_suffix_chunks} max_reparsed_bytes={max_reparsed} max_delta_chunks={max_delta_chunks}"
    );
}

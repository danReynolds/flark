use std::error::Error;
use std::sync::Arc;

use flark_integrated_parser_slice::crop_source::CropSnapshotLease;
use flark_relative_output_reuse_gate::{
    detach_crop_blocks, AllocationReceipt, BlockFact, BlockKind, LocalRange, OutputPage,
    OutputTree, PageId, Position,
};

fn main() -> Result<(), Box<dyn Error>> {
    const PAGE_COUNT: usize = 65_536;
    let mut build = AllocationReceipt::default();
    let mut pages = Vec::with_capacity(PAGE_COUNT);
    for index in 0..PAGE_COUNT {
        pages.push(OutputPage::from_metrics(
            PageId(index as u64),
            Position { byte: 1, utf16: 1 },
            vec![BlockFact {
                kind: BlockKind::Paragraph,
                range: LocalRange {
                    start: Position::default(),
                    end: Position { byte: 1, utf16: 1 },
                },
                container_depth: 0,
                list_property: None,
            }],
            Vec::new(),
            &mut build,
        )?);
    }
    let original = OutputTree::from_pages(&pages, &mut build);
    let old_suffix_root = original.right_partition().expect("large tree has a branch");
    let old_suffix_page = original
        .locate_page(60_000)
        .expect("probe page exists")
        .page;

    let mut mutation = AllocationReceipt::default();
    let inserted = OutputPage::from_fragment(
        PageId(100_000),
        "🦀\n",
        vec![BlockFact {
            kind: BlockKind::Paragraph,
            range: LocalRange {
                start: Position::default(),
                end: Position::of("🦀\n"),
            },
            container_depth: 0,
            list_property: None,
        }],
        Vec::new(),
        &mut mutation,
    )?;
    let edited = original.splice_pages(0..0, &[inserted], &mut mutation);
    let suffix_root_shared = old_suffix_root.shares_root_with(
        &edited
            .right_partition()
            .expect("edited large tree has a branch"),
    );
    let suffix_page_shared = Arc::ptr_eq(
        &old_suffix_page,
        &edited
            .locate_page(60_001)
            .expect("shifted probe page exists")
            .page,
    );
    let absolute = edited
        .absolute_fact(60_001, 0)
        .expect("shifted probe fact exists");

    println!("relative-output-reuse receipt");
    println!(
        "initial pages={} height={} page_allocations={} fact_records={} tree_nodes={}",
        original.page_count(),
        original.height(),
        build.output_pages_allocated,
        build.fact_records_allocated,
        build.tree_nodes_allocated()
    );
    println!(
        "prefix insertion pages_created={} facts_created={} leaf_nodes={} branch_nodes={} nodes_visited={}",
        mutation.output_pages_allocated,
        mutation.fact_records_allocated,
        mutation.leaf_nodes_allocated,
        mutation.branch_nodes_allocated,
        mutation.tree_nodes_visited
    );
    println!(
        "suffix subtree_pages={} root_shared={} probe_page_shared={}",
        old_suffix_root.page_count(),
        suffix_root_shared,
        suffix_page_shared
    );
    println!(
        "probe absolute_byte={} absolute_utf16={} query_nodes={}",
        absolute.range.start.byte, absolute.range.start.utf16, absolute.nodes_visited
    );

    let source = CropSnapshotLease::from_text("old λ\n\nsecond 🦀\n\nthird");
    let (detached, detach) = detach_crop_blocks(Arc::clone(&source))?;
    let observer = Arc::downgrade(&source);
    println!(
        "crop detach leaves={} materialized_bytes={} retained_strong={} retained_weak={}",
        detach.parsed_block_leaves,
        detach.source_bytes_materialized,
        detach.retained_strong_source_leases,
        detach.retained_weak_source_leases
    );
    drop(source);
    println!(
        "crop lease_dropped={} output_still_queryable={}",
        observer.upgrade().is_none(),
        detached.absolute_fact(1, 0).is_some()
    );
    Ok(())
}

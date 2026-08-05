use euler_structure_challenger::{
    BlockId, ClosedChildSummary, EulerSequence, MutationReceipt, QueryReceipt,
    StableBoundaryDirectory, StableDirectoryBuildReceipt, Token,
};

const ITEMS: u64 = 100_000;
const PAGE_TOKENS: usize = 256;
const ARENA_PAGE_BYTES: usize = 4_096;
const SEQUENCE_BRANCH_BYTES: usize = 56;

fn enter(block: BlockId, closed: ClosedChildSummary) -> Token {
    Token::Enter { block, closed }
}

fn tokens() -> Vec<Token> {
    let mut tokens = Vec::with_capacity((ITEMS as usize) * 4 + 4);
    tokens.push(enter(BlockId(1), ClosedChildSummary::default()));
    tokens.push(enter(BlockId(2), ClosedChildSummary::default()));
    for index in 0..ITEMS {
        tokens.push(enter(
            BlockId(10 + index * 2),
            ClosedChildSummary::default(),
        ));
        tokens.push(enter(
            BlockId(11 + index * 2),
            ClosedChildSummary::default(),
        ));
        tokens.push(Token::Exit);
        tokens.push(Token::Exit);
    }
    tokens.push(Token::Exit);
    tokens.push(Token::Exit);
    tokens
}

fn sparse_fallback_payload(blocks: usize) -> usize {
    // Clean comparison, not the naive all-container index:
    // * one 8-byte BlockOrder entry for every block;
    // * 0/1 child containers inline child identity (u64), its three-bit
    //   summary, and one semantics/count byte: optimistic packed 10 B/item;
    // * only the 100k-child list allocates 16-byte direct-child entries;
    // * both persistent sequences use the current 56-byte branch payload.
    let order_capacity = (ARENA_PAGE_BYTES - 4) / 8;
    let order_pages = blocks.div_ceil(order_capacity);
    let order =
        blocks * 8 + order_pages * 4 + order_pages.saturating_sub(1) * SEQUENCE_BRANCH_BYTES;
    let child_capacity = (ARENA_PAGE_BYTES - 12) / 16;
    let child_pages = (ITEMS as usize).div_ceil(child_capacity);
    let large_child_sequence = ITEMS as usize * 16
        + child_pages * 12
        + child_pages.saturating_sub(1) * SEQUENCE_BRANCH_BYTES;
    let large_container_binding = 28;
    let inline_one_child_items = ITEMS as usize * 10;
    order + large_child_sequence + large_container_binding + inline_one_child_items
}

fn main() {
    let tokens = tokens();
    let blocks = ITEMS as usize * 2 + 2;
    let sequence = EulerSequence::from_tokens(&tokens, PAGE_TOKENS);
    let list_interior = 2..2 + ITEMS * 4;
    let changed_item = 50_001_u64;
    let item_enter = list_interior.start + changed_item * 4;
    let paragraph_enter = item_enter + 1;
    let far_rank = list_interior.end - 3;
    let far = sequence
        .page_identity_at(far_rank)
        .expect("far suffix page");
    let item = BlockId(10 + changed_item * 2);
    let paragraph = BlockId(item.0 + 1);
    let mut mutation = MutationReceipt::default();
    let with_paragraph = sequence
        .replace_token(
            paragraph_enter,
            enter(
                paragraph,
                ClosedChildSummary {
                    ends_blank: true,
                    ..ClosedChildSummary::default()
                },
            ),
            &mut mutation,
        )
        .expect("paragraph fact replacement");
    let changed = with_paragraph
        .replace_token(
            item_enter,
            enter(
                item,
                ClosedChildSummary {
                    item_loose_if_nonlast: true,
                    ..ClosedChildSummary::default()
                },
            ),
            &mut mutation,
        )
        .expect("item fact replacement");
    let mut query = QueryReceipt::default();
    let fold = changed
        .direct_child_summary(list_interior.clone(), &mut query)
        .expect("balanced list interior");
    let retained = changed.memory_stats();
    let shared = EulerSequence::shared_memory_stats(&[&sequence, &with_paragraph, &changed]);
    let far_after = changed
        .page_identity_at(far_rank)
        .expect("far suffix page after edit");
    let sparse = sparse_fallback_payload(blocks);
    let mutation_packed_allocation = mutation.packed_token_bytes_copied
        + mutation.token_pages_allocated * 16
        + mutation.nodes_allocated * SEQUENCE_BRANCH_BYTES;

    let mut directory_build = StableDirectoryBuildReceipt::default();
    let directory = StableBoundaryDirectory::build(&changed, &mut directory_build)
        .expect("stable boundary directory");
    let directory_memory = directory.memory();
    let mut boundary_query = QueryReceipt::default();
    let item_range = directory
        .block_range(&changed, item, &mut boundary_query)
        .expect("exact item range");

    println!(
        "euler_realistic_spanning_list items={ITEMS} blocks={blocks} tight={} pages={} packed_payload={} packed_bytes_per_block={:.3} runtime_heap={} shared_three_roots_runtime_heap={} mutation_nodes_visited={} mutation_nodes_allocated={} mutation_token_pages={} mutation_packed_token_bytes={} mutation_modeled_packed_allocation={} query_nodes={} query_whole_nodes={} query_boundary_tokens={} far_suffix_same={} sparse_fallback_payload={} sparse_fallback_bytes_per_block={:.3} raw_euler_saving_per_block={:.3} stable_directory_build_tokens={} stable_directory_standalone_lower_bound={} stable_directory_integrated_record_net={} stable_directory_navigation_lower_bound={} stable_directory_runtime_map_payload_lower_bound={} item_range={:?} boundary_query_nodes={} boundary_query_tokens={} rust_token_bytes={}",
        fold.list_is_tight(),
        retained.token_pages,
        retained.packed_payload_bytes,
        retained.packed_payload_bytes as f64 / blocks as f64,
        retained.runtime_heap_bytes,
        shared.runtime_heap_bytes,
        mutation.nodes_visited,
        mutation.nodes_allocated,
        mutation.token_pages_allocated,
        mutation.packed_token_bytes_copied,
        mutation_packed_allocation,
        query.nodes_visited,
        query.whole_nodes_folded,
        query.tokens_scanned,
        far == far_after,
        sparse,
        sparse as f64 / blocks as f64,
        (sparse - retained.packed_payload_bytes) as f64 / blocks as f64,
        directory_build.tokens_scanned,
        directory_memory.standalone_packed_lower_bound,
        directory_memory.integrated_record_net_bytes,
        directory_memory.root_navigation_lower_bound,
        directory_memory.runtime_map_payload_lower_bound,
        item_range,
        boundary_query.nodes_visited,
        boundary_query.tokens_scanned,
        std::mem::size_of::<Token>(),
    );
}

use euler_structure_challenger::{
    AbsoluteBoundaryOracle, BlockId, ChildSequenceSummary, ClosedChildSummary, ContainerSemantics,
    EnumerationReceipt, EulerSequence, EulerSummary, MutationReceipt, OpenContainerPrefix,
    OpenProperty, OracleTree, PageIdentity, QueryReceipt, StableBoundaryDirectory,
    StableDirectoryBuildReceipt, Token,
};

fn closed(bits: u8) -> ClosedChildSummary {
    ClosedChildSummary::from_bits(bits)
}

fn enter(id: u64, bits: u8) -> Token {
    Token::Enter {
        block: BlockId(id),
        closed: closed(bits),
    }
}

fn oracle_fold(children: &[(BlockId, ClosedChildSummary)]) -> ChildSequenceSummary {
    children
        .iter()
        .fold(ChildSequenceSummary::default(), |summary, (_, child)| {
            summary.followed_by(ChildSequenceSummary::singleton(*child))
        })
}

fn brute_summary(tokens: &[Token]) -> EulerSummary {
    let mut depth = 0_i64;
    let mut minimum_prefix = 0_i64;
    let mut maximum_prefix = 0_i64;
    let mut minimum_enter = None;
    let mut outermost = Vec::new();
    let mut enters = 0_u64;
    for token in tokens {
        match *token {
            Token::Enter { block, closed } => {
                enters += 1;
                match minimum_enter {
                    None => {
                        minimum_enter = Some(depth);
                        outermost.push((block, closed));
                    }
                    Some(minimum) if depth < minimum => {
                        minimum_enter = Some(depth);
                        outermost.clear();
                        outermost.push((block, closed));
                    }
                    Some(minimum) if depth == minimum => outermost.push((block, closed)),
                    Some(_) => {}
                }
                depth += 1;
            }
            Token::Exit => depth -= 1,
        }
        minimum_prefix = minimum_prefix.min(depth);
        maximum_prefix = maximum_prefix.max(depth);
    }
    EulerSummary {
        tokens: tokens.len() as u64,
        enters,
        balance: depth,
        minimum_prefix,
        maximum_prefix,
        minimum_enter_depth: minimum_enter,
        outermost: oracle_fold(&outermost),
        outermost_count: outermost.len() as u64,
        first_outermost: outermost.first().map(|entry| entry.0),
        last_outermost: outermost.last().map(|entry| entry.0),
    }
}

#[test]
fn minimum_enter_monoid_is_exhaustive_and_associative() {
    let alphabet = [Token::Exit, enter(1, 0), enter(2, 7)];
    let mut sequences = vec![Vec::new()];
    for length in 1..=8 {
        let previous = sequences
            .iter()
            .filter(|sequence| sequence.len() == length - 1)
            .cloned()
            .collect::<Vec<_>>();
        for prefix in previous {
            for token in alphabet {
                let mut sequence = prefix.clone();
                sequence.push(token);
                sequences.push(sequence);
            }
        }
    }
    let mut split_cases = 0_usize;
    for tokens in sequences {
        let exact = brute_summary(&tokens);
        assert_eq!(EulerSummary::from_tokens(&tokens), exact, "{tokens:?}");
        for first in 0..=tokens.len() {
            for second in first..=tokens.len() {
                let a = EulerSummary::from_tokens(&tokens[..first]);
                let b = EulerSummary::from_tokens(&tokens[first..second]);
                let c = EulerSummary::from_tokens(&tokens[second..]);
                assert_eq!(
                    a.followed_by(b).followed_by(c),
                    a.followed_by(b.followed_by(c))
                );
                assert_eq!(a.followed_by(b).followed_by(c), exact);
                split_cases += 1;
            }
        }
    }
    eprintln!("minimum_enter_exhaustive split_cases={split_cases}");
    assert!(split_cases > 400_000);
}

#[derive(Clone, Debug)]
struct Shape(Vec<Shape>);

fn forests(nodes: usize, memo: &mut Vec<Option<Vec<Vec<Shape>>>>) -> Vec<Vec<Shape>> {
    if let Some(cached) = &memo[nodes] {
        return cached.clone();
    }
    let result = if nodes == 0 {
        vec![Vec::new()]
    } else {
        let mut output = Vec::new();
        for first_nodes in 1..=nodes {
            for first in trees(first_nodes, memo) {
                for rest in forests(nodes - first_nodes, memo) {
                    let mut forest = Vec::with_capacity(rest.len() + 1);
                    forest.push(first.clone());
                    forest.extend(rest);
                    output.push(forest);
                }
            }
        }
        output
    };
    memo[nodes] = Some(result.clone());
    result
}

fn trees(nodes: usize, memo: &mut Vec<Option<Vec<Vec<Shape>>>>) -> Vec<Shape> {
    assert!(nodes > 0);
    forests(nodes - 1, memo).into_iter().map(Shape).collect()
}

fn materialize(shape: &Shape, next: &mut u64, salt: u64) -> OracleTree {
    let id = *next;
    *next += 1;
    OracleTree {
        block: BlockId(id),
        closed: closed(((id * 5 + salt * 3) & 7) as u8),
        children: shape
            .0
            .iter()
            .map(|child| materialize(child, next, salt))
            .collect(),
    }
}

fn assert_tree_oracle(tree: &OracleTree, page_tokens: usize) {
    let tokens = tree.encoded();
    let sequence = EulerSequence::from_tokens(&tokens, page_tokens);
    let boundaries = AbsoluteBoundaryOracle::build(&tokens).unwrap();
    let mut nodes = Vec::new();
    tree.walk(&mut nodes);
    for node in nodes {
        let interior = boundaries.interior(node.block).unwrap();
        let mut query = QueryReceipt::default();
        assert_eq!(
            sequence
                .direct_child_summary(interior.clone(), &mut query)
                .unwrap(),
            node.direct_child_summary(),
            "block {:?}",
            node.block
        );
        let mut enumeration = EnumerationReceipt::default();
        let actual = sequence
            .collect_outermost(interior, &mut enumeration)
            .unwrap();
        let expected = node
            .children
            .iter()
            .map(|child| (child.block, child.closed))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "block {:?}", node.block);
    }
}

#[test]
fn every_ordered_tree_shape_through_eight_nodes_matches_direct_child_oracle() {
    let mut memo = vec![None; 9];
    memo[0] = Some(vec![Vec::new()]);
    let mut shapes = 0_usize;
    let mut container_queries = 0_usize;
    for nodes in 1..=8 {
        for (salt, shape) in trees(nodes, &mut memo).iter().enumerate() {
            let mut next = 1;
            let tree = materialize(shape, &mut next, salt as u64);
            assert_tree_oracle(&tree, 1 + (salt % 7));
            shapes += 1;
            container_queries += nodes;
        }
    }
    eprintln!("ordered_tree_exhaustive shapes={shapes} container_queries={container_queries}");
    assert_eq!(shapes, 626);
    assert_eq!(container_queries, 4_707);
}

#[derive(Clone, Copy)]
struct Prng(u64);

impl Prng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() as usize) % limit
    }
}

fn random_tree(nodes: usize, random: &mut Prng) -> OracleTree {
    let mut child_ids = vec![Vec::<usize>::new(); nodes];
    for child in 1..nodes {
        // Bias toward recent parents for depth, while retaining wide trees.
        let parent = if random.below(4) == 0 {
            random.below(child)
        } else {
            child.saturating_sub(1 + random.below(child.min(12)))
        };
        child_ids[parent].push(child);
    }
    fn build(index: usize, child_ids: &[Vec<usize>], random: &mut Prng) -> OracleTree {
        OracleTree {
            block: BlockId(index as u64 + 1),
            closed: closed((random.next() & 7) as u8),
            children: child_ids[index]
                .iter()
                .copied()
                .map(|child| build(child, child_ids, random))
                .collect(),
        }
    }
    build(0, &child_ids, random)
}

#[test]
fn randomized_deep_and_wide_trees_match_every_container_oracle() {
    let mut random = Prng(0x2a8f_1d77_c915_64e3);
    let mut trees = 0_usize;
    let mut nodes = 0_usize;
    for case in 0..250 {
        let count = 2 + random.below(499);
        let tree = random_tree(count, &mut random);
        assert_tree_oracle(&tree, 1 + random.below(47));
        trees += 1;
        nodes += count;
        if case % 31 == 0 {
            let mut chain = OracleTree {
                block: BlockId(10_000),
                closed: closed(0),
                children: Vec::new(),
            };
            for depth in 0..384 {
                chain = OracleTree {
                    block: BlockId(10_001 + depth),
                    closed: closed((depth & 7) as u8),
                    children: vec![chain],
                };
            }
            assert_tree_oracle(&chain, 31);
        }
    }
    eprintln!("random_tree_oracle trees={trees} nodes={nodes}");
    assert!(nodes > 50_000);
}

#[test]
fn nested_list_folds_propagate_exactly_without_descendant_pollution() {
    let paragraph_changed = closed(1);
    let item_semantics = ContainerSemantics {
        descends_through_last_child: true,
        is_item: true,
        last_line_blank: false,
    };
    let list_semantics = ContainerSemantics {
        descends_through_last_child: true,
        is_item: false,
        last_line_blank: false,
    };
    let inner_item =
        item_semantics.closed_summary(ChildSequenceSummary::singleton(paragraph_changed));
    let inner_list_children = ChildSequenceSummary::singleton(inner_item);
    let inner_list = list_semantics.closed_summary(inner_list_children);
    let outer_item = item_semantics.closed_summary(ChildSequenceSummary::singleton(inner_list));
    let second_item = closed(0);
    let tree = OracleTree {
        block: BlockId(1),
        closed: closed(0),
        children: vec![OracleTree {
            block: BlockId(100),
            closed: list_semantics.closed_summary(
                ChildSequenceSummary::singleton(outer_item)
                    .followed_by(ChildSequenceSummary::singleton(second_item)),
            ),
            children: vec![
                OracleTree {
                    block: BlockId(101),
                    closed: outer_item,
                    children: vec![OracleTree {
                        block: BlockId(102),
                        closed: inner_list,
                        children: vec![OracleTree {
                            block: BlockId(103),
                            closed: inner_item,
                            children: vec![OracleTree {
                                block: BlockId(104),
                                closed: paragraph_changed,
                                children: vec![],
                            }],
                        }],
                    }],
                },
                OracleTree {
                    block: BlockId(105),
                    closed: second_item,
                    children: vec![],
                },
            ],
        }],
    };
    let tokens = tree.encoded();
    let sequence = EulerSequence::from_tokens(&tokens, 3);
    let boundaries = AbsoluteBoundaryOracle::build(&tokens).unwrap();
    let mut query = QueryReceipt::default();
    let outer = sequence
        .direct_child_summary(boundaries.interior(BlockId(100)).unwrap(), &mut query)
        .unwrap();
    let inner = sequence
        .direct_child_summary(boundaries.interior(BlockId(102)).unwrap(), &mut query)
        .unwrap();
    assert_eq!(outer, tree.children[0].direct_child_summary());
    assert_eq!(inner, inner_list_children);
    assert_eq!(
        sequence
            .range_summary(boundaries.interior(BlockId(100)).unwrap(), &mut query)
            .unwrap()
            .outermost_count,
        2
    );
    assert_eq!(
        sequence
            .range_summary(boundaries.interior(BlockId(102)).unwrap(), &mut query)
            .unwrap()
            .outermost_count,
        1
    );
}

#[test]
fn open_container_keeps_unfinalized_child_out_of_persistent_authority() {
    let tokens = vec![enter(10, 0), Token::Exit, enter(11, 2), Token::Exit];
    let sequence = EulerSequence::from_tokens(&tokens, 2);
    let open = OpenContainerPrefix {
        container: BlockId(1),
        committed_children: 0..tokens.len() as u64,
        active_direct_child: Some(BlockId(12)),
    };
    let mut query = QueryReceipt::default();
    assert_eq!(
        open.committed_summary(&sequence, &mut query).unwrap(),
        oracle_fold(&[(BlockId(10), closed(0)), (BlockId(11), closed(2))])
    );
    assert_eq!(
        open.current_property(&sequence, &mut query).unwrap(),
        OpenProperty::UnknownActiveChild(BlockId(12))
    );
    let closed_prefix = OpenContainerPrefix {
        active_direct_child: None,
        ..open
    };
    assert!(matches!(
        closed_prefix
            .current_property(&sequence, &mut query)
            .unwrap(),
        OpenProperty::Exact(_)
    ));
}

fn page(sequence: &EulerSequence, rank: u64) -> PageIdentity {
    sequence.page_identity_at(rank).unwrap()
}

#[test]
fn subtree_reparent_is_one_contiguous_cut_splice_and_preserves_far_page() {
    let leaves = (0..64)
        .map(|index| OracleTree {
            block: BlockId(1_000 + index),
            closed: closed((index & 7) as u8),
            children: vec![],
        })
        .collect::<Vec<_>>();
    let tree = OracleTree {
        block: BlockId(1),
        closed: closed(0),
        children: vec![
            OracleTree {
                block: BlockId(10),
                closed: closed(0),
                children: vec![
                    OracleTree {
                        block: BlockId(11),
                        closed: closed(1),
                        children: vec![],
                    },
                    OracleTree {
                        block: BlockId(12),
                        closed: closed(2),
                        children: vec![OracleTree {
                            block: BlockId(13),
                            closed: closed(3),
                            children: vec![],
                        }],
                    },
                ],
            },
            OracleTree {
                block: BlockId(20),
                closed: closed(0),
                children: vec![OracleTree {
                    block: BlockId(21),
                    closed: closed(4),
                    children: vec![],
                }],
            },
        ]
        .into_iter()
        .chain(leaves)
        .collect(),
    };
    let tokens = tree.encoded();
    let boundaries = AbsoluteBoundaryOracle::build(&tokens).unwrap();
    let moved_range = boundaries.block_range(BlockId(12)).unwrap();
    let far_original_rank = boundaries.block_range(BlockId(1_063)).unwrap().start;
    let sequence = EulerSequence::from_tokens(&tokens, 5);
    let far_before = page(&sequence, far_original_rank);

    let mut without = tokens.clone();
    let moved = without
        .drain(moved_range.start as usize..moved_range.end as usize)
        .collect::<Vec<_>>();
    let without_boundaries = AbsoluteBoundaryOracle::build(&without).unwrap();
    let destination = without_boundaries.block_range(BlockId(20)).unwrap().end - 1;
    let mut expected = without.clone();
    expected.splice(destination as usize..destination as usize, moved);

    let mut receipt = MutationReceipt::default();
    let reparented = sequence
        .cut_and_insert(moved_range.clone(), destination, &mut receipt)
        .unwrap();
    assert_eq!(reparented.tokens(), expected);
    let expected_boundaries = AbsoluteBoundaryOracle::build(&expected).unwrap();
    let far_after_rank = expected_boundaries
        .block_range(BlockId(1_063))
        .unwrap()
        .start;
    let far_after = page(&reparented, far_after_rank);
    assert_eq!(far_before.page_id, far_after.page_id);
    assert_eq!(far_before.allocation, far_after.allocation);

    let expected_tree = OracleTree {
        children: tree
            .children
            .iter()
            .map(|child| match child.block {
                BlockId(10) => OracleTree {
                    children: vec![child.children[0].clone()],
                    ..child.clone()
                },
                BlockId(20) => OracleTree {
                    children: vec![
                        child.children[0].clone(),
                        tree.children[0].children[1].clone(),
                    ],
                    ..child.clone()
                },
                _ => child.clone(),
            })
            .collect(),
        ..tree
    };
    let mut expected_nodes = Vec::new();
    expected_tree.walk(&mut expected_nodes);
    for node in expected_nodes {
        let mut query = QueryReceipt::default();
        assert_eq!(
            reparented
                .direct_child_summary(
                    expected_boundaries.interior(node.block).unwrap(),
                    &mut query,
                )
                .unwrap(),
            node.direct_child_summary(),
            "reparented block {:?}",
            node.block
        );
    }
    assert!(receipt.nodes_visited < 256, "{receipt:?}");
}

fn realistic_spanning_list_tokens(items: u64) -> Vec<Token> {
    let mut tokens = Vec::with_capacity((items as usize) * 4 + 4);
    tokens.push(enter(1, 0));
    tokens.push(enter(2, 0));
    for index in 0..items {
        tokens.push(enter(10 + index * 2, 0));
        tokens.push(enter(11 + index * 2, 0));
        tokens.push(Token::Exit);
        tokens.push(Token::Exit);
    }
    tokens.push(Token::Exit);
    tokens.push(Token::Exit);
    tokens
}

#[test]
fn realistic_hundred_thousand_item_list_updates_locally_and_reuses_suffix() {
    const ITEMS: u64 = 100_000;
    let tokens = realistic_spanning_list_tokens(ITEMS);
    let sequence = EulerSequence::from_tokens(&tokens, 256);
    let list_interior = 2..2 + ITEMS * 4;
    let changed_item = 50_001_u64;
    let item_enter = list_interior.start + changed_item * 4;
    let paragraph_enter = item_enter + 1;
    let far_rank = list_interior.end - 3;
    let far_before = page(&sequence, far_rank);

    let changed_item_id = 10 + changed_item * 2;
    let changed_paragraph_id = changed_item_id + 1;
    let mut mutation = MutationReceipt::default();
    let with_paragraph = sequence
        .replace_token(
            paragraph_enter,
            enter(changed_paragraph_id, 1),
            &mut mutation,
        )
        .unwrap();
    let changed = with_paragraph
        .replace_token(item_enter, enter(changed_item_id, 2), &mut mutation)
        .unwrap();
    let mut expected = tokens.clone();
    expected[paragraph_enter as usize] = enter(changed_paragraph_id, 1);
    expected[item_enter as usize] = enter(changed_item_id, 2);
    assert_eq!(changed.tokens(), expected);

    let mut query = QueryReceipt::default();
    let list = changed
        .direct_child_summary(list_interior.clone(), &mut query)
        .unwrap();
    assert!(!list.list_is_tight());
    assert_eq!(
        list,
        brute_summary(&expected[list_interior.start as usize..list_interior.end as usize])
            .outermost
    );
    let far_after = page(&changed, far_rank);
    assert_eq!(far_before.page_id, far_after.page_id);
    assert_eq!(far_before.allocation, far_after.allocation);
    assert!(mutation.nodes_visited < 512, "{mutation:?}");
    assert!(query.nodes_visited < 256, "{query:?}");
    assert!(query.tokens_scanned <= 512, "{query:?}");

    let retained = changed.memory_stats();
    let shared = EulerSequence::shared_memory_stats(&[&sequence, &with_paragraph, &changed]);
    eprintln!(
        "realistic_spanning_list items={ITEMS} blocks={} pages={} packed_payload={} packed_bytes_per_block={:.3} runtime_heap={} shared_three_roots={} mutation_nodes={} mutation_pages={} mutation_token_bytes={} query_nodes={} query_boundary_tokens={}",
        ITEMS * 2 + 2,
        retained.token_pages,
        retained.packed_payload_bytes,
        retained.packed_payload_bytes as f64 / (ITEMS * 2 + 2) as f64,
        retained.runtime_heap_bytes,
        shared.runtime_heap_bytes,
        mutation.nodes_visited,
        mutation.token_pages_allocated,
        mutation.packed_token_bytes_copied,
        query.nodes_visited,
        query.tokens_scanned,
    );
    assert!(retained.packed_payload_bytes < 2_400_000);
}

#[test]
fn absolute_boundary_ranks_are_exact_but_prefix_rebase_proves_them_disallowed() {
    let tokens = realistic_spanning_list_tokens(1_000);
    let original = AbsoluteBoundaryOracle::build(&tokens).unwrap();
    let suffix = BlockId(10 + 999 * 2);
    let before = original.block_range(suffix).unwrap();
    let mut prefixed = vec![enter(9_000_000, 0), Token::Exit];
    prefixed.extend_from_slice(&tokens);
    let after = AbsoluteBoundaryOracle::build(&prefixed)
        .unwrap()
        .block_range(suffix)
        .unwrap();
    assert_eq!(after.start, before.start + 2);
    assert_eq!(after.end, before.end + 2);
    assert_eq!(original.packed_lower_bound_bytes(), (1_000 * 2 + 2) * 24);
}

#[test]
fn stable_page_cursor_resolves_exact_boundary_after_prefix_edit_without_rebasing() {
    const ITEMS: u64 = 10_000;
    let tokens = realistic_spanning_list_tokens(ITEMS);
    let sequence = EulerSequence::from_tokens(&tokens, 256);
    let block = BlockId(10 + (ITEMS - 1) * 2);
    let absolute = AbsoluteBoundaryOracle::build(&tokens).unwrap();
    let mut original_build = StableDirectoryBuildReceipt::default();
    let original = StableBoundaryDirectory::build(&sequence, &mut original_build).unwrap();
    let original_cursor = original.cursor(block).unwrap();
    let original_rank = original.enter_rank(block).unwrap();
    let mut matching = QueryReceipt::default();
    assert_eq!(
        original
            .block_range(&sequence, block, &mut matching)
            .unwrap(),
        absolute.block_range(block).unwrap()
    );

    let mut mutation = MutationReceipt::default();
    let prefixed = sequence
        .splice(0..0, &[enter(9_000_000, 0), Token::Exit], &mut mutation)
        .unwrap();
    let mut prefixed_build = StableDirectoryBuildReceipt::default();
    let current = StableBoundaryDirectory::build(&prefixed, &mut prefixed_build).unwrap();
    assert_eq!(current.cursor(block).unwrap(), original_cursor);
    assert_eq!(current.enter_rank(block).unwrap(), original_rank + 2);
    let mut current_matching = QueryReceipt::default();
    assert_eq!(
        current
            .block_range(&prefixed, block, &mut current_matching)
            .unwrap(),
        original_rank + 2..original_rank + 6
    );
    assert_eq!(original_build.tokens_scanned, tokens.len());
    assert_eq!(prefixed_build.tokens_scanned, tokens.len() + 2);
    assert!(mutation.nodes_visited < 128);

    let memory = current.memory();
    eprintln!(
        "stable_boundary blocks={} build_tokens={} standalone_packed_lower_bound={} integrated_record_net={} root_navigation_lower_bound={} runtime_map_payload_lower_bound={} match_nodes={} match_boundary_tokens={}",
        prefixed_build.block_locators,
        prefixed_build.tokens_scanned,
        memory.standalone_packed_lower_bound,
        memory.integrated_record_net_bytes,
        memory.root_navigation_lower_bound,
        memory.runtime_map_payload_lower_bound,
        current_matching.nodes_visited,
        current_matching.tokens_scanned,
    );
    assert_eq!(
        memory.standalone_packed_lower_bound,
        prefixed_build.block_locators * 18
    );
    // The cursor itself is stable, but rebuilding its root navigation scanned
    // the entire document. This receipt is a correctness model, not a GO.
    assert!(prefixed_build.tokens_scanned > 40_000);
}

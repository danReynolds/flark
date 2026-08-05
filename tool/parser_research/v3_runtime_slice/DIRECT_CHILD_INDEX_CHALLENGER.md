# Per-container direct-child index challenger

Status: **mechanism GREEN; representation not selected** (2026-07-15).

This challenger answers one narrow question left open by a preorder-only
`BlockOrder`: can a spanning list's exact CommonMark child-output fold be
updated without walking descendants or skipping over each direct child's
subtree?

The executable answer is yes. Each container owns a packed persistent sequence
of `{direct_child_block_id, ClosedChildAggregate}` entries. Its branch monoid is
the exact associative `ChildSequenceAggregate::followed_by` fold. A separate
stable-ID-sorted binding sequence maps a container to that child-sequence root
and stores its fold plus the three container semantics needed to derive the
container's own closed-child contribution.

The focused test is:

```sh
cargo test --test record_forest \
  direct_child_fold_index_updates_spanning_and_nested_list_properties_locally \
  -- --nocapture
```

Debug receipt:

```text
direct_child_index items=100000 build_payload=1627048 live_payload=1632864 \
bytes_per_large_item=16.27 large_pages=393 large_edit_nodes=59 nested_edit_nodes=44
```

The test proves all of the following at the representation layer:

- one changed contribution in a 100,000-item list copies one packed child page,
  one container binding, and logarithmic paths in the two persistent trees;
- the last child page keeps exact arena identity after the interior edit;
- the list property changes from tight to loose from the exact range monoid;
- propagating newly derived closed-child contributions through an inner list,
  its enclosing item, and an outer list takes 44 recorded node visits; and
- releasing every old/intermediate/new root reclaims the arena to zero live
  nodes.

The 16.27 bytes/item figure is retained arena payload for this index alone. It
excludes the normalized block table, structural order, source coverage,
branches owned by other components, allocator metadata, build vectors, and the
coverage-order oracle. It is not total runtime memory.

## Why it is not selected yet

This shape is clean locally but may be redundant globally. It duplicates every
direct-child ID beside the existing preorder and creates one individually owned
sequence root per container. A unified balanced-parentheses structural
sequence could potentially replace both plain `BlockOrder` and this index:
`Enter(BlockId, ClosedChildAggregate)` / `Exit` tokens plus a range monoid over
depth delta, minimum relative depth, and the ordered aggregate at that depth
can recover a container's direct-child fold.

That unified alternative still has to prove several facts before it is called
cleaner: stable and bounded `BlockId -> Enter/interior/Exit` lookup; exact
behavior for unfinished containers represented by the open overlay; packed
bytes rather than an assumed token size; token-level local splices without page
fragmentation; and cut/splice correctness for subtree reparenting. Until that
comparison is executable, `ContainerChildFoldIndex` is a feasibility baseline,
not part of `RecordForestManifest` and not an architecture selection.

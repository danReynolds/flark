# Generic hierarchical-green gate

Status: **shared mechanics GO; final-C representation REJECT**,
2026-07-16.

This was the one final discriminator for candidate C. It asked whether the
generic hierarchical-green idea could become a real persistent large-document
representation using the shared arena, persistent sequence, and transaction,
without a Markdown-pattern codec, proxy edge nodes, rebased ranks, a global
`BlockId` directory, or a second sequence/transaction primitive.

## Hard verdict

**Reject this final-C representation. Preserve its generic arena mechanics.**

The implementation proves that bounded page-local fanout, the existing
persistent sibling sequence, exact page sharing, and one arena transaction fit
together. It does **not** prove the semantic hierarchical representation that
the editor needs. The passing 100,000-block receipts exercise a simpler flat
shape which cannot encode the required nested source order.

Fixing the mismatch is not a codec optimization. It requires mixed semantic
children and independently owned source atoms inside every local sequence,
nested source-route mutation, and nested viewport traversal. That is a
substantial new representation and moves candidate C toward the serialized
green model it was meant to beat. Per the final-bakeoff stop rule, do not open
that redesign now.

## What the mechanism did prove

The narrow physical mechanisms are useful and remain reusable:

- one arena page can contain its payload plus up to 128 bounded child IDs;
- a 4 KiB leaf can own 101 external roots without one proxy allocation per
  edge;
- all large sibling ranges use the crate's existing persistent sequence;
- leaf, branch, root, and manifest allocations share one `PageArena` and one
  `ArenaBuildTransaction`;
- source-derived local replacement path-copies a bounded leaf/branch path and
  retains the exact distant suffix page; and
- fixed-size direct-child fold contributions propagate through the existing
  associative sequence summaries without a sibling scan.

The arena capability is grammar-neutral. Existing two-child callers retain
their old API and behavior. It can be used by another representation if its
packed pages need many owned edges.

## Fatal semantic mismatch

### 1. One local run followed by one child is not Markdown source order

`HierarchicalSiblingEntry` contains exactly one `local_metric`, one coverage
identity/source kind, and one optional child whose source is defined to follow
that local run:

```text
entry := local-source-run, optional-child-document
```

That cannot represent a continuation-line quote/list marker owned by an
ancestor while a descendant Paragraph remains open, or an Item marker before a
Paragraph followed by an Item-owned editable gap. The earlier isolated generic
page proved arbitrary ordered source pieces, but this final composed module did
not carry that model forward.

### 2. Ordinary leaf blocks require a dummy or duplicate entry

Every `HierarchicalGreenDocument` build rejects an empty sibling sequence. A
leaf Paragraph therefore cannot be represented solely by its root facts and
source; it needs another sibling entry beneath it. Consequently the required
`Item -> Paragraph` shape cannot be encoded without a dummy/duplicate semantic
node or another local storage form. Either escape invalidates this receipt.

### 3. Nested source lookup is not globally exact

Nested lookup descends the child using a child-local offset and prepends
enclosing block IDs, but returns the child's byte and UTF-16 ranges without
adding the parent base. Its cursor names the child manifest. Calling
`replace_at_cursor` on the top-level document with that nested hit therefore
fails as stale instead of path-copying the enclosing hierarchy.

The reported query receipt counts sibling nodes/entries, but does not fully
account the repeated child-document summary reads and bounded leaf decode work
that a real nested query performs. `decode_leaf` also allocates decoded-entry
storage and re-encodes the full bounded page for canonicality on every query;
that 4 KiB-class scratch/copy work is absent from the query receipt.

### 4. Viewport and document totals stop at the local sibling layer

Viewport traversal emits the intersecting parent's local range but never
descends an optional child document. `block_count` adds only the current root
and current sibling entries; descendants are omitted. The flat byte/UTF-16
viewport receipt is therefore not evidence for the hierarchical product
query.

### 5. The fold test uses separately retained child truth

The nested test first mutates a separately retained 100,000-Item inner
document, then manually installs its new root and a new fold contribution into
an outer Item. That validates the shared sequence algebra, but it does not
validate one source-derived nested edit through a single current document
root. The child root acts as an external oracle during the two-step test.

This is not independently mutable *semantic* truth in the immutable arena, but
it leaves the required top-level mutation route and atomic parser adoption
unproved.

## Why the 43.91 B/block receipt is not selectable

The passing large fixture is 100,000 flat entries with `child == None`, cycling
through Paragraph, Heading, fenced/indented code, HTML, Table/Row/Cell,
thematic break, BlockQuote, List, and Item. Naming those kinds does not make
them a valid heterogeneous tree. The fold fixture is a List with 100,000 Item
entries, not 100,000 Items each containing a Paragraph with marker, content,
and gap ownership.

The measured narrow receipt remains useful only as a physical upper bound:

| Narrow flat fixture receipt | Value |
| --- | ---: |
| named semantic entries plus Document | 100,001 |
| leaf pages | 991 |
| live nodes across old/candidate/current roots | 2,016 |
| live encoded payload | 4,080,872 B |
| live page-local edge tables | 16,320 B |
| arena slot capacity | 4,096 slots / 262,144 B |
| modeled allocator metadata | 32,256 B |
| three root handles | 24 B |
| fully accounted narrow retained total | 4,391,616 B |
| narrow bytes per named entry | 43.91 B |

The allocator model is 16 bytes per live boxed page; it is not a portable
allocator measurement. Payload, edges, slots, branches, roots, manifests, and
handles are included. What is missing is more important: the valid nested
semantic/source encoding itself.

The narrow performance receipts were:

| Operation | Receipt |
| --- | --- |
| flat byte/UTF-16 lookup | 10 sequence nodes, 10 entries |
| flat local viewport | 21 nodes, 1 leaf, 101 entries |
| flat prefix replacement | 49 nodes, 4,944 payload bytes |
| flat interior replacement | 82 nodes, 5,464 payload bytes |
| distant flat suffix | exact page identity retained |
| inner fold replacement | 82 nodes, 101 decoded entries |
| outer manual fold installation | 14 nodes, 101 decoded entries |

Peak narrow build state was an 8,192-byte typed page buffer, 4,072-byte
encoded page buffer, nine simultaneous streaming roots/384 bytes of bins, and
a 16-entry/512-byte transaction journal.

None of those numbers should be compared as a candidate-C total against the
serialized candidate's exact 200,002-block, 300,000-source-atom
Item-to-Paragraph workload.

## Required-workload audit

| Required workload/operation | Final-C result |
| --- | --- |
| 100,000 top-level ordinary Paragraphs | physical flat sequence only |
| 100,000 Item-to-Paragraph List | **fail: shape not representable honestly** |
| nested tightness-changing List ancestry | sequence algebra only; nested top-root mutation absent |
| 10 MiB/100,000-line Paragraph and fence | not run through this composed codec |
| nested marker-only lines and editable gaps | **fail in composed codec**; isolated local-page witness only |
| table/setext promotion, detach, reparent | not run through this composed codec |
| 10,000 insertions in one gap | not run |
| prefix edit with exact distant suffix | pass for flat local entries |
| exact nested byte/UTF-16 ranges | **fail: child-local ranges are not rebased** |
| hierarchical viewport | **fail: child documents are not traversed** |

The source contract explicitly forbids winning from a structure-only or
easier-shape receipt. This gate therefore fails regardless of its green test
suite.

## Stop-condition judgment

The current code did avoid the explicit mechanical anti-patterns:

- no pattern-specific List/Item/Table codec;
- no proxy allocation per edge;
- no global `BlockId -> location` map;
- no persistent cursor rebasing;
- no second sibling sequence; and
- no second transaction type.

But it avoided them by not implementing the required semantic/source shape.
Closing that gap would require a new mixed local token model, route-aware
nested updates, and source-aware nested traversal. The isolated generic page
suggests this is theoretically possible, but the final discriminator was
specifically the point at which those mechanisms had to compose. They did not.

Retiring a packed page also currently decodes one bounded page of child edges
inside one reclaim transition before the already-fuelled release queue drains
them. If this arena facility is reused, move the earlier one-edge-per-poll
cursor witness into the shared arena when strict per-edge scheduler fuel is a
production requirement.

## Validation

The implementation is mechanically sound for what it actually models:

- 63 debug all-target tests pass;
- 63 release all-target tests pass;
- `cargo fmt --all -- --check` passes;
- strict all-target Clippy passes with warnings denied; and
- `wasm32-unknown-unknown` all-target checking passes.

Green validation does not override the semantic workload mismatch.

Executable rejected artifact:

- `v3_runtime_slice/src/arena.rs`
- `v3_runtime_slice/src/generic_green.rs`
- `v3_runtime_slice/src/hierarchical_green_sequence.rs`
- `v3_runtime_slice/tests/generic_green.rs`
- `v3_runtime_slice/tests/hierarchical_green_sequence.rs`

## Selection consequence

Remove this form of hierarchical green from the physical-representation
bakeoff. Keep the authority/lifetime architecture and the generic packed-edge
arena mechanism. If serialized green passes its final shared-transaction and
composition audit, select it. If serialized green fails, redesign the
representation seam from the concrete failures; do not treat this flat
hierarchical receipt as the fallback and do not open another speculative
candidate merely to preserve C.

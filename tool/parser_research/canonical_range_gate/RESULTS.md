# Canonical range and coverage gate

Status: **semantic GO for Flark-canonical ranges; production integration HOLD**, 2026-07-15.

This gate asks one narrow architecture question: must Flark retain Comrak's
chronological list-source-position repair protocol in its persistent output, or
can product coordinates be derived from final source ownership and ancestry?

The tested answer is the latter. Select **B: Flark-canonical ranges**. Reject
**A: persistent repair overlay**. Do not select **C: a hybrid persistent
model**; a donor compatibility projection may remain as a test-only/parser-seam
adapter if a real consumer is later found, but it is not ordinary semantic
state.

This is a semantic decision, not a production-readiness claim. The prototype is
an intentionally batch, heap-backed adjudicator. It does not yet prove the
persistent packed-page implementation, local splice complexity, bounded
allocation, or Flutter integration.

## Why this is the cleaner authority boundary

`RepairListSourcePositions` reproduces a donor AST mutation sequence. It is not
a Markdown transition that can affect continuation, rendering, or the meaning
of a later line. Persisting it would force the record forest to remember:

- which descendant ended with a zero-column sentinel;
- which close/repair occurred first;
- prior position snapshots or stamps; and
- an eager descendant rewrite whose only purpose is donor source-position
  compatibility.

Those facts violate the compact-output contract without adding product
authority. The editor needs exact current source ownership, stable ancestry,
strict parent containment, byte/UTF-16 coordinate queries, explicit gaps, and
exact presentation subtargets. All are expressible as functions of the current
source, final tree, and total coverage partition.

The decisive small counterexample is CommonMark fixture 257:

```markdown
 -    one

     two
```

Pristine Comrak reports the list as `[1:2, 1:9]`, the item as
`[1:2, 2:0]`, and its paragraph as `[1:7, 1:9]`. In half-open byte offsets the
item is `1..10` while its parent list is `1..9`: the child escapes its parent.
Fixture 255 routes an equivalent zero-column repair to a different node. Both
event streams contain the sentinel and one repair event; chronology determines
which donor field is overwritten.

The canonical result uses one rule for both fixtures: meaningful marker/leaf
facts end at the final child on the first line, so list and item are both the
contained extent (`1..9` for fixture 257). The following blank line is an
explicit root-owned gap. No historical repair state survives.

## Normative product model

The following is the proposed contract, independent of this prototype's data
structures:

1. The current source and its stable physical coverage identities are the
   coordinate authority. Persistent records do not store rebased absolute
   suffix offsets.
2. The exact block machine emits final structural facts, terminal syntax facts,
   container-marker facts, detach/reparent operations, and sparse auxiliary
   facts such as reference definitions. These are grammar/product facts.
3. `BlockOrder` is the one preorder sequence. Each block has a stable ID,
   parent ID, and a subtree boundary/aggregate; it has no copied child vector.
4. `CoveragePartition` covers the exact source once and in order. Each segment
   is owned by a terminal leaf, an explicit editable gap, or the innermost
   container marker. The document root is real and owns root-level gaps.
5. A terminal syntax extent is its exact syntax, excluding a semantically free
   terminal line delimiter. A marker extent is the exact marker syntax. Every
   excluded byte still belongs to a coverage segment; nothing disappears.
6. A container's query range is the hull aggregate of its own marker facts and
   its current preorder subtree. Consequently every child is contained by its
   parent by construction. An internal blank line in a continuing list/quote
   remains a gap owned by that innermost container; a trailing inter-block blank
   line falls back to the appropriate ancestor/root gap.
7. The document byte range is the full source `[0, source_byte_length)`. Byte,
   UTF-16, and line coordinates are aggregate queries over the source coverage
   structure. Grapheme-safe editing uses the source coordinate service plus
   boundary summaries. None are independently mutable parser-position facts.
8. Reference definitions and similar auxiliary syntax may have a sparse
   semantic fact/index while their physical bytes participate in ordinary
   coverage. Calling a coverage segment a `Gap` does not declare those bytes
   semantically irrelevant; coverage kind is ownership/layout vocabulary, not
   a second grammar.
9. Table cells, task markers, fence opener/info/body/closer, inline marker and
   replacement ranges, ambiguity, and command capabilities remain exact
   requested presentation facts. A broad block hull is never substituted for
   an interaction target.
10. `PositionStamp`, `end_stamp`, repair ordering, repair scope, and sparse
    prior descendant snapshots are absent from committed records and deltas.
    `RepairListSourcePositions` should disappear from the production semantic
    protocol. It may exist only inside a donor oracle/compatibility adapter.

The intended persistent implementation is a preorder/subtree range aggregate
over packed persistent pages plus the total coverage sequence. A local edit
path-copies touched coverage/order pages and recomputes aggregates on the
changed paths. Container extent lookup is then a sequence range query; it does
not scan or rewrite descendants. The intended bound is
`O(depth * log(pages))` adoption work for ancestry/range aggregates, subject to
the composed gate proving it.

## Prototype

The temporary crate is:

- `src/lib.rs`: consumes the exact `ResumableValueBlockParser` event stream,
  deliberately counts and ignores every `RepairListSourcePositions`, derives
  terminal/marker extents and pure subtree hulls, builds total byte/UTF-16
  coverage, and validates containment.
- `tests/canonical_ranges.rs`: checks the repair counterexamples, nested-list
  gap ownership, reference-definition detach plus setext promotion, GFM table
  promotion/marker ownership, Unicode + CRLF boundaries, and the complete
  CommonMark/GFM fixture corpora.

The corpus gate also compares the exact block kind/tree order against the clean
value parser and normalized HTML against pristine Comrak under the matching
CommonMark/GFM options. Range canonicalization therefore did not create a
second Markdown interpretation in the exercised corpus.

## Receipts

Strict focused verification:

```console
$ cargo fmt --package flark-canonical-range-gate -- --check
$ cargo clippy --all-targets -- -D warnings
$ cargo test --release --all-targets
running 3 tests
test fixtures_255_and_257_expose_donor_chronology_but_have_one_canonical_rule ... ok
test focused_promotion_detach_nested_list_and_table_ranges_are_product_coherent ... ok
test full_commonmark_and_gfm_corpora_keep_exact_tree_html_and_canonical_range_invariants ... ok
```

Full differential corpus receipt:

```console
$ cargo test --release --test canonical_ranges \
    full_commonmark_and_gfm_corpora -- --nocapture
CANONICAL_RANGE_CORPUS fixtures=1322 nodes=3841 \
  donor_range_deltas=1362 non_document_range_deltas=40 \
  repair_scope_range_deltas=22 donor_parent_containment_failures=8 \
  canonical_parent_containment_failures=0 ignored_repair_events=208 \
  detached_nodes=156 \
  delta_by_kind={"code": 24, "document": 1322, "heading": 2, \
                 "item": 8, "paragraph": 6}
test full_commonmark_and_gfm_corpora_keep_exact_tree_html_and_canonical_range_invariants ... ok
```

The 1,322 document deltas are expected: Flark's document owner covers the full
source, including its terminal delimiter. The 40 non-document deltas are 20
unique CommonMark cases mirrored by GFM:

- 8 item deltas are the four donor parent-containment failures in each corpus:
  CommonMark 257/308/309/313 and GFM 235/288/289/293.
- 6 paragraph and 2 heading deltas begin after detached reference definitions,
  at the surviving product-visible leaf rather than historical paragraph
  scratch.
- 24 code deltas normalize zero-column/trailing-line behavior using surviving
  source-backed content and exact syntax delimiters.

The donor has eight parent-containment failures. The canonical result has zero
over all 1,322 fixtures. All 208 repair events were ignored, while 156 detach
events were still applied because detach is a real final-tree transition.

The current v2 contracts were audited separately with these baselines:

```console
$ flutter test \
    test/v2/markdown/flark_native_comrak_parse_backend_test.dart \
    test/v2/render_plan/flark_render_plan_test.dart \
    test/v2/flutter/flark_cross_block_selection_test.dart \
    test/v2/flutter/flark_selection_gesture_test.dart
00:00 +61: All tests passed!

$ flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart \
    --name 'source-host structural transition matrix|existing block surfaces stay mounted|blank line|terminal.*exit'
00:00 +10: All tests passed!
```

Those Flutter runs are baseline/audit receipts, not candidate integration
tests. They identify the migration obligations below.

## Product-observable consequences

The v2 bridge and Flutter layer already show that raw donor source positions
are not the product contract:

- the native backend expands several AST positions to full-line block ranges
  and trims leading reference-definition lines;
- the Dart backend synthesizes/normalizes list, fence, table, and related
  ranges; and
- the editor uses some block endpoints as proxies for omitted blank gaps,
  terminal exit separators, and focus-host bounds.

Therefore replacing those endpoint values mechanically would break behavior.
The v3 port should instead make the missing concepts explicit:

- canonical source anchors remain the selection/history authority;
- total coverage supplies editable gap hosts and global source/display maps;
- gap adjacency/exit-separator facts replace block-end heuristics;
- stable input/mounted-host leases preserve focus and IME independently of
  semantic authority;
- exact projection/compound-target records drive hidden markers, table cells,
  tasks, fences, links, and commands; and
- reference-definition generations invalidate affected inline facts even when
  a block's structural range is unchanged.

Add a v3 integration version of fixture 257, mixed known/unknown selection and
edit-through-gap cases, terminal fence/list exit cases, and nested
container-marker-only/blank lines. The current v2 tests are a behavior record,
not evidence that their internal range proxies should be preserved.

## What this gate does not prove

The prototype deliberately uses operations that are unacceptable in the
production hot path:

- batch parsing and materialization of all event nodes;
- heap `BTreeMap`/`BTreeSet` collections and absolute offsets;
- an `O(nodes * atomic intervals)` ownership scan (quadratic worst case);
- repeated UTF-16 prefix counting while building coverage;
- recursive full-tree hull construction;
- no materialized reference-definition/property index (the gate exercises
  detach and physical coverage, while clean-parser HTML supplies the semantic
  oracle); and
- no persistent splice/identity, memory accounting, allocation-failure,
  cancellation, scheduling, urgent-overlay, or physical-device latency test.

It is a semantic adjudicator: it proves that a deterministic, chronology-free
range contract exists for the exercised grammar. It does **not** prove that the
current `record_forest` implements that contract efficiently or transactionally.

## Decision and next gate

Adopt Flark-canonical ranges in the architecture contract and delete repair
chronology from the persistent schema before composing the parser and record
forest. Keep a tiny donor source-position adapter only in differential tests
unless a concrete external compatibility promise is discovered.

The next composed gate must feed the exact parser events into the production
candidate forest and reproduce this gate's tree, HTML, range, coverage, and
UTF-16 receipts across clean parses and edit histories. It must additionally
prove bounded packed-page splice/adoption, stable distant suffix identity,
reference/property aggregate updates, cancellation, and current-root coordinate
queries. Only then does this move from semantic **GO** to production **GO**.

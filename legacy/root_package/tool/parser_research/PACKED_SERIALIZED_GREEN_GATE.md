# Packed serialized-green gate

Status: **selected integration direction; mechanism/integration progress GO;
production HOLD**, 2026-07-16.

This gate records the shared-arena port of the source-ordered serialized-green
candidate in `v3_runtime_slice`. It supersedes the earlier typed-`Arc` memory
model as the useful executable receipt for this representation. The selected
direction for the next integrated parser slice is now:

```text
one immutable packed source-order sequence
  Enter(BlockId, kind, bounded facts)
  Coverage(CoverageId, byte/UTF-16 lengths, owner depth, part)
  Exit(close-time child contribution)

owned by one revisioned manifest in the shared PageArena
```

This selects a direction for continued integration, not a frozen production
codec. The lane still loses if the remaining parser/composer work requires a
global `BlockId` directory, persistent token ranks, cursor repair after each
mutation, or a second independently mutable structure/source authority.

## What is now executable

The port retains only encoded arena payloads and ownership edges. Typed
`GreenEvent`s and decoded leaf records are bounded build/query scratch; there
is no typed token mirror beside every packed leaf.

The root manifest binds syntax profile, source revision, grammar revision,
parse generation, semantic epoch, known byte range, sequence counts, and
byte/UTF-16 metrics. It owns one persistent sequence root through an arena
edge. Build and Enter rewrites allocate through one `ArenaBuildTransaction`.

The focused release commands are:

```sh
cargo test --release --test serialized_green -- --nocapture
cargo test --release \
  close_time_fold_selects_only_direct_children_across_every_split \
  -- --nocapture
```

They currently pass five integration tests and the close-time-fold unit gate.

## Close-time direct-child fold correction

The earlier serialized challenger placed a `ClosedChildAggregate` on Enter.
That is not a clean parser sink: list blankness/tightness contribution is known
only when the block closes, so direct parser construction would have to
backpatch every affected Enter.

The packed port uses the equal-order streaming dual:

```text
Enter { block, kind, facts }
...
Exit { closed_child_bits }
```

An Exit remains a one-byte common record. Its three low bits encode
`ends_blank`, `item_loose_if_nonlast`, and `item_loose_if_last`.

The associative branch summary stores `minimum_closed_depth` and the
corresponding `outermost` child fold. Nested descendant Exits occur above the
minimum closing depth; direct-child Exits occur at the minimum. Concatenation
therefore selects and composes the same direct-child sequence without knowing
the contribution at Enter time.

The unit gate enumerates all 512 triples of three-bit child contributions and
all 13 split points in the twelve-event nested witness: 6,656 recompositions.
Every split produces the same balance, minimum depth, and ordered child fold.
This removes a real integration backpatch, but parser-direct emission is still
a remaining end-to-end gate.

## Atomic bounded facts

Facts are no longer free-standing property chunks. Enter has either a
no-facts or with-facts tag; the latter contains one length-delimited
`FactsEnvelope`:

```text
schema_version varint
field_count varint
repeated {
  (FactId << 1 | critical) varint
  value_length varint
  value bytes
}
```

The current codec enforces:

- schema version 1;
- nonzero, strictly increasing, unique fact IDs;
- minimal varints and exact envelope-length consumption;
- rejection of unknown critical fields;
- a 256-byte maximum inline envelope;
- required facts for List, Item, Heading, fenced code, HTML, Table, Row, Cell,
  and ThematicBreak; and
- initial kind, length, enum, and scalar-value checks for the implemented fact
  families.

An Enter and its facts are one encoded event, so page replacement cannot
separate or partially rewrite them. The depth-100 batch rewrites complete
Enter+facts records.

This is a credible framing and validation mechanism, not yet a complete
product fact schema. Several field payloads remain compact opaque byte layouts
with only shallow validation. Unbounded facts such as large table-alignment
vectors and source-backed value runs do not yet have typed arena edges; packed
green leaves currently reject ownership edges entirely. Inline projection,
semantic targets, reference resolution, and command capabilities remain
separate revision-scoped presentation facts and must not be copied into this
structural envelope.

## Mandatory source/projection schema correction

The projection-sufficiency audit found that the current
`CoverageRun { id, physical metrics, owner depth, part }` is exact for physical
coverage/navigation but cannot reconstruct parser-logical input. It omits
partial-tab expansion length, hidden reference prefixes, table-cell
trim/unescape, synthetic output, and code/HTML logical slice boundaries.

Serialized green remains selected, but the production token must become one
unified `SourceProjectionRun`: the same physical ownership record carries an
orthogonal `None`, `Identity`, typed `Atomic`, or typed relative `Program`
logical contribution. Uncommon Program pages are ownership edges of the same
green transaction; there is no CoverageId lookup directory or aggregate
logical String. The 34.98-byte receipt remains honest for its measured
structural/physical facts but is not a complete semantic-root memory receipt
until projection pages are included.

## Stable source coverage and streaming query

Every coverage run now carries a nonzero stable `CoverageId` in addition to
byte/UTF-16 length, owner-relative depth, and source part. IDs are retained in
the packed receipt rather than assumed to exist elsewhere. The current codec
stores each ID as a fixed eight-byte scalar.

The query API starts from source, not from arbitrary block identity:

1. `seek` descends byte or UTF-16 prefix metrics once, respecting
   upstream/downstream affinity.
2. A reverse balanced-parentheses traversal uses summaries to skip closed
   subtrees and reconstructs exact open frames. Each frame contains BlockId,
   kind, decoded facts, and a current-manifest Enter capability.
3. The returned `GreenStreamCursor` retains a branch zipper. Successor leaves
   are reached from that zipper rather than by descending from the root again.
4. `next_coverage` yields CoverageId, part, exact byte and UTF-16 ranges,
   semantic owner including kind/facts, semantic ancestry, full structural
   open path, and a current-manifest coverage capability.

The interleaved witness covers a quote-owned continuation marker while a
Paragraph remains structurally open, unequal byte/UTF-16 coordinates,
quote-owned and document-owned gaps, and sequential transition between those
owners.

The far-viewport witness seeks at run 1,500 in a 2,000-run paragraph and then
streams 400 runs across multiple leaves. Its receipt has exactly one root
descent and zero successor root descents.

This proves the indexless query shape. It does not yet prove stable-ID lineage
through arbitrary source edits, every affinity boundary, or the Dart bridge
shape. `GreenCoverageView` currently clones the owner frame and allocates
ancestry/open-path vectors per yielded run; production viewport output should
encode the initial stack once and then source-ordered push/pop/coverage deltas.

## One-base-root depth-100 rewrite

`seek` returns Enter capabilities stamped with manifest, expected leaf,
base-leaf index, byte offset, BlockId, and kind. `rewrite_enters` resolves every
capability against the same immutable base root before constructing the new
manifest. It groups targets by leaf, validates the expected ArenaId, decodes an
affected leaf once, replaces complete Enter+facts events, and applies all leaf
replacements through one transaction.

The executable witness builds 100 nested BlockQuotes followed by a 2,000-run
distant paragraph, rewrites all 100 ancestor fact envelopes in one batch, and
proves:

- the distant final leaf keeps the exact same ArenaId;
- every unchanged base leaf is reported reused;
- the candidate exposes the new facts while the retained base root still
  exposes the old facts; and
- maximum affected-page decode scratch remains below 64 KiB.

This closes the previous repeated-query/stale-cursor shape for multi-ancestor
fact propagation. It is not yet the generic mutation API: the proven batch
replaces affected leaves containing known Enter records and requires the total
structural/source summary to remain unchanged.

## 100,000 Item-to-Paragraph receipt

The workload is:

```text
Document
  List + 8-byte critical List fact
    100,000 x Item + 4-byte critical Item fact
      container-marker CoverageId
      Paragraph
        content CoverageId
      gap CoverageId
```

It contains 200,002 semantic blocks and 300,000 stable coverage runs. The
release receipt is:

```text
packed serialized green
blocks                                      200,002
leaf pages                                     1,596
live arena nodes                               3,214
live payload bytes                         6,657,179
live edge bytes                               25,880
slot capacity                                  4,096
slot-vector bytes                            262,144
modeled allocator bytes                       51,424
one root handle                                    8
accounted retained bytes                   6,996,635
accounted bytes/block                           34.98

maximum encoded page scratch                   4,096
transaction owner-journal bytes                  512
```

This is an executable packed-only representation receipt, not structure-only
arithmetic. It is also not process RSS or total editor memory.

### Included

The 6,996,635-byte retained total includes:

- packed Enter, facts, CoverageId/metrics/owner/part, and Exit records;
- list/item fact envelopes used by the workload;
- 80-byte leaf and branch summaries, including the close-time child fold;
- persistent-sequence ownership edges and their packed bytes;
- the 104-byte revisioned manifest and its root edge;
- the arena slot vector at allocated capacity, including unused capacity;
- a 16-byte-per-live-node allocator model; and
- one retained root handle.

The 4 KiB encoded-page buffer and 512-byte transaction journal are reported as
temporary build capacities, not added to the retained total.

### Excluded

The receipt excludes:

- Crop/source storage, coordinate/grapheme indexes, and edit-lineage history;
- parser continuation, restart samples, open overlay, UnknownRange, and
  convergence state;
- reference symbol/occurrence indexes;
- external large-fact roots and source-backed value-run pages;
- inline/projection facts, semantic targets, command capabilities, and the
  composite presentation manifest;
- additional retained history/candidate roots;
- allocator headers, size-class rounding, arena/Vec object headers, and
  process-runtime overhead beyond the explicit 16-byte node model; and
- Dart source mirrors, bridge buffers, viewport records, Flutter layout,
  painting, semantics, selection, IME, and host state.

The number therefore selects neither a complete worker memory budget nor a
launch ceiling. It is the first honest packed structural/source representation
number for this candidate.

## Remaining hard gates

Production remains HOLD until all of the following compose in this same lane.

1. **Unified logical-projection runs and typed large-fact edges.** Extend the
   physical coverage token with exact None/Identity/Atomic/Program logical
   contribution, add a streaming logical cursor, and add bounded,
   schema-checked arena children for relative transform programs, table
   alignments, and raw logical slices. Prove the full 1,322-fixture logical and
   origin differential plus count/type/generation validation, lazy/fuelled
   query, atomic ownership, cancellation, corruption handling, and memory
   receipts. A large fact may not expand the inline envelope or block an
   ordinary viewport query.
2. **Token/range mutation and boundary repacking.** Implement one-base-root
   insert, delete, promotion, reference-only detach, subtree move/reparent,
   suffix attachment, and exact cuts inside coalesced Coverage runs. The
   current leaf-range helpers are not wired into serialized green and do not
   prove token/byte/UTF-16 boundary cuts. Replacement must repack predecessor
   and successor boundary leaves; 10,000 insertions in one gap may not retain
   thousands of tiny leaves or a growing branch seam.
3. **Resumable ownership journal.** The transaction provides one rollback
   boundary and PageArena reclamation is fuelled, but transaction Drop still
   scans its owner journal synchronously. Cancellation, supersession, and
   allocation failure need a bounded resumable journal/reclaim path with
   replay/stale-handle tests and an honest peak receipt.
4. **Parser and composer integration.** Stream the exact block parser directly
   into Enter/Coverage/Exit pages, consume restart/composer bindings as one
   capability-bound base-root mutation program, attach a certified suffix, and
   adopt structure, coverage, large facts, references, restart state, and the
   manifest atomically. Prove canonical CommonMark/GFM structure, ranges,
   markers, list folds, tables, HTML, references, stable-ID lineage, open and
   partial roots, cancellation, and fresh-parse equality after every revision.
5. **Corruption, failure, and randomized mutation.** Fuzz packed event/fact
   lengths, minimal varints, summaries, arena edges, manifests, owner depth,
   stale/replayed capabilities, overlapping ranges, allocation failures, and
   repeated AVL/sequence rebalancing. Every failure must preserve the old root
   and reclaim the entire candidate under bounded fuel.
6. **Native/WASM and device proof.** Measure actual worker retained/peak RSS,
   edit-to-facts and far-viewport latency, bridge bytes, Dart adoption work,
   worker-to-paint frames, cancellation tails, and the current query's cloning
   costs on representative iOS, Android, desktop, and web devices. Parser
   correctness and a 34.98-byte structural receipt do not certify jank-free
   input, shaping, layout, accessibility, or live feel.

## Verdict

The packed shared-arena port materially strengthens serialized green. It now
has a parser-streamable close-time fold, atomic bounded Enter facts, explicit
coverage identity, packed-only retention, a source-first zipper query, and one
base-root multi-ancestor rewrite with exact distant sharing. Those pieces fit
together as one authority rather than as compensating indexes.

That is enough to select packed serialized green as the **integration
direction** and to mark mechanism/integration progress GO. It is not enough to
freeze the physical representation or claim launch readiness. Production
selection remains HOLD until generic token/range mutation, large facts,
resumable ownership, parser/composer composition, adversarial correctness, and
device receipts pass without introducing hidden global lookup or a second
semantic truth.

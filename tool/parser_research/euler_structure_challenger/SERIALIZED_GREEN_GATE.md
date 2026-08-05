# Source-driven serialized-green gate

Status: **architectural direction selected; production implementation HOLD**,
2026-07-16.

This gate tests the strongest Euler candidate rather than the earlier
Enter/Exit-only estimate. One immutable source-order rope now carries:

- `Enter(BlockId, kind, closed-child contribution)` and `Exit` structure;
- generic, length-tagged structural property chunks adjacent to their owning
  Enter;
- coalesced coverage atoms with byte/UTF-16 length, semantic part, and
  owner-relative ancestor depth; and
- exact branch summaries for source descent, balanced-parenthesis navigation,
  direct-child folds, and a fixed four-label unmatched-Enter witness.

No persistent token rank, document-wide `BlockId -> node` directory, parent
map, or separately keyed structural-property map is used by this lane.
Source lookup returns a current-root route cursor. The cursor is deliberately
ephemeral: a newer root rejects it even when its leaf page survived, and a
fresh source query recovers the unchanged block and leaf identity.

## Executable receipts

The release command is:

```sh
cargo test --release --all-targets -- --nocapture
```

The final gate matrix is green for formatting, warning-free Clippy, debug and
release all-target tests, and `wasm32-unknown-unknown` compilation. The fixed
four-label witness is also checked exhaustively across 167,481 short
fragment/split combinations; deeper paths are covered by an exact-fallback
equivalence test.

The 100,000-item workload is:

```text
Document
  List + list metadata
    100,000 x Item + item metadata
      marker coverage
      Paragraph
        content coverage
      gap coverage
```

Its current receipt is:

```text
semantic blocks                         200,002
adjacent property records               100,001
coverage atoms                          300,000
packed leaf pages                           908
packed branch nodes                         907
encoded token bytes                   3,700,029
retained encoded payload              3,801,629
modeled 80-byte arena slots               1,815 / 145,200 bytes
payload + slots                       3,946,829 = 19.73 bytes/block
three shared revision roots           3,955,587 / 1,843 unique slots

full source hit with Enter cursors        32 tree nodes / 2,015 leaf tokens
owner-only source hit                     16 tree nodes / 569 leaf tokens
fixed witness uses                         2 fragments
sequential 256-atom viewport               2 leaves / 681 tokens

one prefix insertion                      31 nodes visited
                                            23 nodes allocated
                                         6,082 encoded payload bytes allocated
distant suffix leaf identity                exact reuse

maximum typed page scratch             32,768 bytes
maximum encoded page scratch            4,096 bytes
maximum streaming roots                     9
streaming-bin storage                      128 bytes
```

The fixed witness costs 32 additional branch bytes relative to the earlier
64-byte summary: about 29 KiB, or 0.15 byte per semantic block in this
workload. It stores only the first four unmatched Enter labels. It is a query
accelerator, not structural authority. Paths deeper than four fall back to
the exact reverse monoid descent.

The full hit scans more than the owner-only hit because it also reconstructs
root-scoped Enter routes for every enclosing block so range and subtree
operations can begin without a directory. The remaining 569-token owner cost
is the selected variable-width 4 KiB leaf scan. It is bounded by page size,
not document size. Viewport traversal then stays sequential instead of
repeating source descent for every atom.

The 10 MiB ordinary paragraph is one coverage atom:

```text
semantic blocks              2
coverage atoms               1
leaf pages                   1
encoded token bytes         28
payload + slot             124 bytes
source-query tokens          5
```

This is finalized lossless state, not chronological per-line parser history.

## Semantic gates covered

The focused tests prove:

- byte and UTF-16 lookup with unequal metrics;
- upstream/downstream ownership at coverage boundaries and document end;
- quote-owned and document-owned gaps;
- an ancestor-owned continuation marker while Paragraph remains structurally
  open, with distinct semantic owner ancestry and structural open path;
- fail-closed owner-depth validation;
- matching Exit, source hull, and subtree traversal starting from a
  source-derived route cursor;
- stale route rejection after root replacement and fresh source recovery;
- source-addressed adjacent properties for heading, list/item metadata,
  fence metadata, HTML block type, and table alignments;
- a nested list tightness-changing edit propagated through generic route-based
  Enter replacement and associative child summaries without a sibling scan;
- fixed-size witness acceleration and sequential viewport traversal; and
- distant suffix page sharing across one and three retained revisions.

The generic property chunks are important. Earlier wording placed rare facts
in a `BlockId`-keyed sparse object, which would have smuggled the directory
cost back into the design. Structural facts now live immediately after Enter
and are reached from the same source-derived path. A property larger than 16
bytes uses typed continuation chunks; source payload such as an info string
remains a Crop-backed range rather than copied bytes.

## What 19.73 bytes/block includes

It includes:

- actual encoded Enter, Exit, property, and coverage bytes;
- stable 64-bit block IDs, generic kinds, and closed-child fold bits;
- list metadata on the spanning List and item metadata on every Item;
- byte/UTF-16 coverage metrics, part, and owner-relative depth;
- 16-byte leaf headers;
- actual 96-byte branch-summary encodings, including the four-label witness;
- one modeled 80-byte shared-arena slot per retained leaf and branch; and
- exact unique-node accounting for three structurally shared roots.

The current Arc challenger additionally retains a typed mirror so tests can
inspect tokens. That prototype-only lower bound is 29,822,933 bytes on the
100,000-item case, including 25,600,160 typed-token bytes. Production must
decode one bounded packed leaf on demand; the 19.73 figure is not achieved by
shipping the typed mirror.

## What it excludes

The 19.73 figure is **not total editor or even total worker memory**. It does
not include:

- Crop source text and its piece/coverage root (shared source authority);
- a manifest node carrying source revision, parser generation, profile,
  Unknown range, and adoption state;
- the parser continuation, open overlay, or sampled restart checkpoints;
- reference occurrence/winner indexes or requested inline/presentation facts;
- one top-level arena ownership journal and cancellation/reclamation state;
- allocator metadata beyond the modeled arena slot storage; or
- Flutter layout, host, selection, and input state.

The builder receipt includes typed/encoded page scratch and streaming-bin
roots, but not an arena transaction journal. Until the same code runs through
the shared `ArenaBuildTransaction`, its cancellation and total peak-memory
story remains unproved.

## Comparison and current judgment

The earlier 11.56-byte Euler result was structure only and must not be used as
the selection number. The arithmetic 21.25-byte “sparse flat” comparator was
also structure only and was not an executable full flat representation. The
current fixed-AoS flat witness (roughly 149 accounted bytes/block on its small
block lane) and the 26.06-byte hierarchical microtree witness are not
like-for-like either: the former deliberately overpacks records, while the
latter specializes Item-to-Paragraph entries and does not yet carry this
heterogeneous property set.

The useful conclusion is architectural, not a benchmark victory:

- the serialized-green candidate can unify structure, source ownership,
  source order, common list folds, and compact heterogeneous structural facts
  under one immutable authority;
- normal product discovery can be source-first and indexless;
- a small derived witness is sufficient to make viewport-start ancestry
  bounded without introducing a depth vector or global directory; and
- large open leaves coalesce instead of retaining historical line events.

This makes serialized green the **selected physical direction**, not a hack.
The final generic hierarchical candidate is rejected because its composed
codec cannot honestly represent the required nested semantic/source shape.
This decision does not select the current standalone `Arc` implementation for
production: its typed mirror, incomplete schemas, and non-transactional
mutations remain explicit blockers.

## Remaining production-selection gates

Production selection remains HOLD until the serialized lane proves all of:

1. generic route-based insert/delete, promotion, reference-only detach, and
   contiguous subtree reparent without falling back to an absolute rank;
2. one shared-arena build/adopt/cancel transaction, fuelled reclamation, a
   manifest root, and an honest total peak receipt;
3. parser-direct construction plus every-revision differential equivalence to
   fresh exact parses and the canonical range/coverage oracle;
4. restart, convergence, and suffix adoption from source boundaries while
   preserving stable bindings and property adjacency;
5. reference occurrence cursors and sampled restart anchors after prefix
   edits without retaining an old root;
6. randomized packed-codec, route-splice, AVL-balance, deep-path, and corrupted
   page tests; and
7. an actually executable optimized comparison only if the integrated packed
   receipt loses its expected density or requires a hidden repair/index
   mechanism; the rejected hierarchical receipt is not such a baseline.

If generic mutations require cursor repair, a directory, or a second source
of parent/child truth, this candidate loses despite its compact receipt. If
those gates remain local and source-derived, the evidence currently favors
serialized green as the physical semantic representation.

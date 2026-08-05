# Active leaf normalization transaction gate

Status: **Setext/reference storage mechanism GO; production transaction shape
HOLD**, 2026-07-16.

This gate asks one narrow question: can selected packed-green storage normalize
one provisional Paragraph through exact physical authority, regardless of
whether that Paragraph came from a retained immutable root or an unpublished
current candidate, without a `BlockId` lookup, parser-visible generic mutation,
aggregate source text, or a retained mutation event?

The answer is **yes for Setext and reference-only Paragraph finalization**. The
executable path is
[`v3_runtime_slice/src/serialized_green/active_leaf_transaction.rs`](v3_runtime_slice/src/serialized_green/active_leaf_transaction.rs).
The same experiment also shows why the production authority must be wider than
one final leaf: a GFM table normalization group can have zero, one, or multiple
direct replacement wrappers and can own a later Table body.

## What is executable

`ActiveLeafCapability` is opaque, non-`Clone`, and rooted in one exact packed
manifest. It contains the physical Paragraph Enter and its storage-resolved
matching Exit, not a semantic ID lookup. `ActiveLeafTransaction` consumes that
capability and exposes only two typed operations:

- `promote_setext(level)` changes the Paragraph Enter to canonical typed Setext
  Heading facts while retaining the same `BlockId`;
- `remove_reference_only()` removes the balanced Paragraph wrapper while
  preserving every physical source byte.

`CandidateActiveLeafStorage` contributes only candidate-local ownership. Once
bound, both candidate and retained origins call the same validation and packed
range-replacement kernel. Origin is recorded in the receipt but does not select
a different mutation algorithm.

The successful receipts prove:

- the original root remains queryable after success;
- a Setext Heading retains the Paragraph `BlockId` and exact source metric;
- an untouched distant suffix leaf retains exact `ArenaId` identity;
- reference-only finalization reduces the block/token summaries by exactly one
  balanced wrapper and preserves total byte/UTF-16 coverage;
- Paragraph-owned definition and ending runs become nearest-parent `Gap` with
  logical contribution `None`;
- ancestor-owned Quote/List/Item marker runs remain owned by the same semantic
  ancestors while their encoded relative depths are rebased after removal of
  the Paragraph (`3 -> 2`, `2 -> 1`, and `1 -> 0` in the nested witness);
- stale manifest capabilities fail before publication;
- a deliberately wrong far Exit fails after a replacement page has already
  been allocated, and transaction rollback returns live arena nodes to the
  exact pre-attempt baseline while the old root stays queryable; and
- a candidate containing thousands of small Paragraphs remains normally page
  packed; the gate asserts fewer than one tenth as many leaves as blocks, so
  candidate-local normalization does not imply one arena page per Paragraph.

There is no stored `Promote`, `Detach`, or replacement event in the resulting
green. The transaction re-encodes only affected packed leaves and publishes the
canonical final structure.

## Validation receipt

From `tool/parser_research/v3_runtime_slice`:

```text
cargo test --lib active_leaf_transaction
  6 passed

cargo test --release --lib active_leaf_transaction
  6 passed

cargo clippy --lib --tests -- -D warnings
  green

cargo test --all-targets
  green (99 unit tests plus every default integration target)
```

The six focused tests cover retained Setext, candidate-origin Setext, flat and
nested reference-only removal, stale authority, suffix identity, exact source
and balance, old-root coexistence, and late rollback after allocation.

## What this does not prove

### The mutation job is not fuelled yet

The mechanism currently uses the legacy synchronous `ArenaBuildTransaction`.
It decodes one page at a time and retains no document-sized event tape or source
string, but it still:

- scans from the Paragraph Enter to its matching Exit when minting authority;
- retains one replacement descriptor per changed packed page until commit; and
- cannot suspend between page transformations or sequence joins.

This is acceptable as a semantic/storage falsifier, not as a production worker
job. The production implementation must port this exact operation onto
`ArenaBuildSession`/`ArenaBuildOwner` and the resumable sequence mutation path.
Cancellation must transition to the existing constant-time abort plus fuelled
owner reclamation. Adding per-line shadow objects, a second event tape, or a
global tree rewrite would fail the gate.

### Candidate origin is storage-local, not yet CandidateWriter integration

The candidate witness wraps an unpublished, already balanced packed root. It
proves that origin does not require a second normalization kernel and that
ordinary pages remain dense. It does not yet prove that the current
`CandidateWriter` can suspend an unresolved Paragraph group and later splice
its normalized root. Today that writer seals events irreversibly into a
streaming sequence. Production integration therefore requires the writer to
hold one candidate-owned normalization-group lease and append its final packed
root, rather than first publishing provisional Paragraph events to the main
stream and trying to backpatch them.

### Table promotion falsifies a leaf-identity abstraction

A Paragraph normalization can end as:

```text
Paragraph             one wrapper, primary ID survives
Setext Heading        one wrapper, primary ID survives
Reference-only        zero wrappers, primary ID retires
Whole Table           one Table wrapper, new Table/Header/Cell IDs
Split-preface Table   closed Paragraph plus open Table, two wrappers
```

The Table footprint can later include an arbitrarily large body. A checkpoint
before its delimiter must be able to invalidate or adopt that whole typed
footprint without searching from a Paragraph ID. Implementing only a
Paragraph Enter/Exit capability would force heuristic inversion of canonical
green or a generic range rewrite.

The production name and authority should therefore be
`LeafNormalizationGroup`, with `ActiveLeafTransaction` as its Paragraph-phase
state machine. A sealed group manifest needs storage-private capabilities for:

- exact source extent and source/projection partitions;
- final structural footprint, which may be empty or contain multiple wrappers;
- exact checkpoint and reference-effect footprints;
- outcome-specific identity disposition; and
- nested capability generation so replacing a Table group invalidates Row/Cell
  capabilities without enumerating them during admission or cancellation.

Every checkpoint inside the unresolved Paragraph refers to the group identity,
not its eventual final kind. Finalization binds the group once; it does not
backpatch every checkpoint or preserve a mutation history.

## Required next executable gate

Promote this mechanism toward production only after one resumable
`LeafNormalizationGroupJob` proves all of the following:

1. fresh candidate and retained-base groups enter through one private physical
   capability path;
2. Setext and nested reference-only witnesses remain exact under one-page fuel;
3. whole-table and split-preface-table outcomes use exhaustive typed source
   partitions and storage-minted outcome identities;
4. a checkpoint before a delimiter can turn a large old Table back into a
   Paragraph without orphan descendants or a subtree scan;
5. a giant open Paragraph can converge through retained group provenance
   without scanning to its opener or eventual closer;
6. cancellation at every page/join/manifest boundary publishes nothing and
   reclaims candidate owners under explicit fuel; and
7. small closed blocks remain packed together in ordinary arena pages.

Until those receipts are green, this gate strengthens the architecture but
does not authorize production implementation of the complete normalization
subsystem.

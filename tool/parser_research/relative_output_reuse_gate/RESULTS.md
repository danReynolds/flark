# Relative output reuse results

Date: 2026-07-15. Host: Apple M1 Pro, arm64 macOS 26.2, Rust 1.93.1.

## Result

The state partition in `../ARCHITECTURE_STATE_PARTITION.md` survives this
gate. The promising architecture is:

1. Crop owns immutable source revisions and exact edit provenance.
2. A parser job may use revision-local absolute positions while scanning.
3. Before publication, parsed facts are sealed into Flark-owned immutable
   coverage/output pages with only page-local byte/UTF-16 coordinates.
4. A persistent balanced sequence composes those pages and derives current
   absolute coordinates through subtree prefix sums.
5. Reference values, dependent-inline generations, and container/list
   properties live in separate persistent indexes keyed by stable IDs.

Neither the parser nor the source rope is allowed to become the long-lived
coordinate authority for output. That makes the storage seam compatible with
either a Flark-owned parser or a bounded donor facade.

## Deterministic receipt

`cargo test --all-targets` passes six focused tests:

- same-cardinality Unicode prefix edit with exact suffix page and subtree
  identity;
- a page-count-changing insertion before 65,536 pages;
- real Crop-backed block parsing, clean-parse comparison, and source lease
  retirement across CRLF and Unicode;
- reference-symbol and list-property indirection;
- 128 repeated prefix edits retaining one suffix root; and
- 1,000 deterministic mixed splices against a flat oracle.

The release receipt is:

```text
relative-output-reuse receipt
initial pages=65536 height=17 page_allocations=65536 fact_records=65536 tree_nodes=131071
prefix insertion pages_created=1 facts_created=1 leaf_nodes=1 branch_nodes=16 nodes_visited=18
suffix subtree_pages=32768 root_shared=true probe_page_shared=true
probe absolute_byte=60005 absolute_utf16=60003 query_nodes=17
crop detach leaves=3 materialized_bytes=26 retained_strong=0 retained_weak=0
crop lease_dropped=true output_still_queryable=true
```

The prefix insertion changes page cardinality and adds an astral scalar. It
allocates exactly one payload page, one fact, one leaf node, and 16 branch
nodes. It visits 18 existing/index nodes. The unchanged 32,768-page right
subtree remains the same `Arc`, as does a suffix probe page. Its absolute byte
and UTF-16 positions differ and are reconstructed by a 17-node prefix-sum
walk.

No suffix output page, fact array, fact record, or reference record is
allocated during the mutation. This is direct evidence against a hidden
per-fact suffix rebase.

The Crop receipt is measured after the integrated `BlockJob`, its
`BlockOutput`, and all source-bound temporary leaf handles are dropped. The
detached output adds zero strong and zero weak leases. Dropping the last
`Arc<CropSnapshotLease>` makes an observer `Weak` fail to upgrade while an
absolute query over the detached output still succeeds.

Validation run:

```text
cargo test --all-targets
  6 integration tests pass

cargo clippy --all-targets -- -D warnings
  PASS

cargo run --release --bin reuse_receipt
  PASS; receipt above

RUSTC=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo build --release --target wasm32-unknown-unknown --lib
  PASS
```

## Assumptions challenged

### Crop identity is neither available nor necessary

The output model never asks Crop for a stable subtree ID. Exact source lineage
authorizes convergence, but Flark's immutable coverage page is the persistent
anchor after publication. This is cleaner than embedding
`{crop_root, absolute_start, absolute_end}` and later trying to repair it.

The lifetime test proves retirement of the old `CropSnapshotLease` wrapper.
An edited current Crop revision may legitimately share Crop-private source
nodes with its predecessor; this gate neither observes nor forbids that. The
important result is that output adds no ownership edge into either revision.

### Stable page identity alone is insufficient

Keeping a page pointer while storing absolute offsets inside its facts would
still require an O(suffix facts) rewrite. The executable type stores both byte
and UTF-16 endpoints locally and carries both metrics through every sequence
node. The tests verify that an astral prefix changes the two absolute metrics
by different amounts while the suffix page payload remains identical.

### A general splice need not preserve one monolithic suffix root

Balanced split/join can decompose the suffix's top path when page cardinality
changes. The requirement is structural sharing of unchanged payload pages and
large unaffected subtrees, not preservation of one particular tree
association. In the 65,536-page insertion, the entire 32,768-page right root
is retained exactly; only one logarithmic path is rebuilt. Clean semantic
identity must never depend on tree association.

### “Put the reference map in every aggregate” would hide linear work

Copying an exact symbol map into every output-tree summary would make a path
rewrite potentially O(all symbols). This proof uses an associative,
constant-size occurrence count/digest only as a change summary. It is not a
correctness authority because hashes can collide. Exact ordered occurrences,
winner selection, and consumer dependency sets must live in their own
persistent indexes.

The prototype `SymbolTable` and `PropertyTable` intentionally use whole-map
copy-on-write `BTreeMap`s. They prove page indirection and identity, not update
complexity. Production needs a persistent map/tree and separate defined ↔
undefined dependency scheduling.

### Output detachment cannot be deferred indefinitely

The integrated Crop block prototype currently emits absolute physical ranges
and weak Crop bindings. Those are safe only as parser-job temporaries. The
adapter drops them before returning output. A production implementation should
emit local facts directly when sealing each coverage page rather than publish
the current `BlockOutput` shape and detach it later.

## What this does not prove

- The adapter materializes the complete Crop source solely to derive UTF-16
  metrics and scalar-safe slices. That is unacceptable in the live path.
  Production must accumulate byte/UTF-16 coverage incrementally in the source
  cursor/page builder.
- Both old and edited documents are fully block-parsed in the clean comparison;
  the test manually performs the authorized output splice. Exact checkpoint
  convergence still has to invoke this seam in the real incremental job.
- The detachment adapter emits one coverage page per prototype block leaf to
  make ownership and local-coordinate checks transparent. Production pages
  should pack a bounded number of blocks/facts while preserving the same local
  anchoring and splice semantics.
- The integrated block grammar is still its declared narrow profile. This says
  nothing new about full CommonMark/GFM exactness.
- One `Arc` per page and one `Arc` per binary index node are clarity-first
  research representations, not the packed output memory design. The existing
  packed-page/arena work still has to be integrated with this persistent index.
- `PageId` is borrowed from the prototype block leaf allocator. Production
  coverage IDs need explicit lifecycle, convergence reuse, collision, and
  serialization rules.
- The reference digest is non-authoritative, and the exact occurrence index,
  first-definition winner aggregate, consumer dependency index, and cancellable
  defined/undefined invalidation path are not implemented here.
- List property indirection is demonstrated, but no parser-produced list
  instance identity or ancestor aggregate update is wired in yet.
- Dropping an entire output revision still performs unmetered `Arc` release
  work. Revision retirement belongs off the UI thread and needs its own bounded
  reclamation receipt.
- No Dart/FFI/Wasm transport, visible-range materialization, layout, paint, or
  floor-device latency is measured.

## Direction

Proceed with this relative-output contract as a hard acceptance condition for
the next parser candidate. Do not add stable Crop-node identity, absolute
persistent facts, or a Crop-root sidecar.

The next decisive gate should integrate this page builder into the exact
checkpoint/convergence job so one real prefix edit:

1. parses only through exact structural convergence;
2. seals changed pages directly with local byte/UTF-16 facts without document
   materialization;
3. splices the old suffix root through the persistent output index;
4. compares all canonical facts/ranges to a clean parse; and
5. retires source and output revisions on the worker under measured bounds.

In parallel, replace the proof-only reference/property maps with persistent
occurrence, dependency, and aggregate indexes. That work is orthogonal to the
Comrak-fork-versus-owned-parser decision; either parser architecture must pass
the same output and lifetime contract.

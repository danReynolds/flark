# Relative output reuse gate

Date: 2026-07-15.

## Verdict

**Narrow GO for revision-independent persistent output.** Flark does not need
stable node identity from Crop, and it must not retain a Crop descriptor as a
persistent fact anchor. A balanced output sequence whose leaves own only local
facts and source-length metrics can reuse an unchanged suffix after a prefix
edit without rebasing any suffix fact or retaining the old source revision.

This is an output-storage and coordinate-model result. It is not a complete
incremental parser, a convergence proof, a packed-memory result, or a choice
between the owned block core and the bounded Comrak challenger.

## Proven contract

`OutputPage` is an immutable Flark object. It contains:

- a Flark-minted `PageId`;
- the page's byte and UTF-16 coverage lengths;
- block facts whose endpoints are relative to that page;
- stable list-property IDs rather than copied tight/loose values; and
- reference occurrences keyed by stable symbol IDs.

It deliberately contains no Crop lease, weak lease, Crop/root identity,
absolute document offset, source slice, or source string. `OutputTree` indexes
pages with immutable balanced nodes. Every node aggregates page count, byte
length, UTF-16 length, fact count, and a constant-size reference-occurrence
summary.

Current absolute coordinates are query results, not stored facts. A query
walks one root-to-leaf path and adds left-subtree byte and UTF-16 lengths to the
page-local endpoint.

## Implemented proof

- A same-cardinality prefix replacement rebuilds the changed leaf and one
  minimal index path. Unchanged right subtrees and pages are retained by `Arc`
  identity.
- A general split/join splice handles insertions and deletions while preserving
  balanced height and sharing unchanged pages/subtrees.
- Allocation receipts separately count output pages, fact/reference records,
  leaf nodes, branch nodes, and visited nodes. There is no inferred
  “unchanged” work hidden in a wall-time number.
- The real Crop-backed `BlockJob` from `integrated_parser_slice` is exercised.
  Its revision-local absolute block ranges and weak source bindings are copied
  into temporary scalar values, converted to page-local facts, and dropped
  before detached output is returned.
- A clean reparse of an edited CRLF/Unicode document is semantically identical
  to a candidate built by replacing only the changed prefix page and retaining
  the old suffix pages.
- Separate symbol and property tables demonstrate that a reference winner
  value or list-tightness change need not rewrite a structural output page.
- A deterministic 1,000-splice oracle test covers mixed insert/delete/replace
  operations and validates ordering, identity, prefix sums, allocation bounds,
  and tree height.

## Reproduction

```sh
cd tool/parser_research/relative_output_reuse_gate
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run --release --bin reuse_receipt
RUSTC=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  rustup run stable cargo build --release --target wasm32-unknown-unknown --lib
```

See [RESULTS.md](RESULTS.md) for the captured receipt, assumptions challenged,
and remaining gates.

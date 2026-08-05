# Lazy inline fact-cache gate

Date: 2026-07-15.

## Verdict

**GO for lazy exact inline facts over a complete source/block spine.** A large
document does not need eager inline facts or a document-wide inline consumer
index. The editor can keep every leaf source-visible, parse only the visible +
overscan + actively edited leaves with the real bounded Comrak inline service,
and retain exact results in a strict byte-bounded cache.

This result removes the 55 MB protocol / 104 MB live-heap eager-retention
problem previously measured for a 10 MiB ordinary document. It does not prove
the complete block spine, worker transport, Flutter adoption, or floor-device
paint latency.

## Model

The gate composes two existing prototypes rather than inventing another
Markdown grammar:

- the relative-output model supplies Flark-owned stable leaf IDs and
  page-local byte/UTF-16 coordinates;
- a compact paged SoA directory retains source coverage, leaf generations,
  block-owned inline context, and relative lengths, but no inline facts;
- the cache key is the complete `LeafVersion`: stable ID, content generation,
  and `LeafInlineContext`, not source bytes/content generation alone;
- `LeafInlineContext` distinguishes ordinary paragraphs, headings, table
  cells, and first versus later list-item paragraphs. Only an exact block-spine
  certificate for the first paragraph of a list item maps to Comrak's
  task-list-aware inline entry point;
- viewport scheduling requests active, visible, then overscan leaves;
- each job calls the real patched Comrak `parse_inline_fragment` service;
- completion adoption validates document revision, the complete leaf version,
  window epoch, and current per-symbol presence generations; and
- a fixed-capacity, byte-accounted cache moves accepted fragments into place
  and evicts cold/overscan entries first.

An uncached, pending, evicted, over-cap, or stale leaf remains source-visible.
No predictive inline style is shown while exact work is absent.

Reference value changes are renderer/symbol-table changes and retain cached
leaf structure. Undefined/defined transitions are checked lazily against the
dependencies carried by each cached leaf. A never-parsed leaf retains no
dependency list; when it becomes visible, it parses against the current symbol
snapshot. Hidden document consumers are never enumerated.

## Executable coverage

- 10 MiB ordinary document with 106,997 leaves and zero eager inline facts.
- Synthetic 100 MiB descriptor/index scale with 1,069,975 leaves and zero
  source payload or inline facts.
- Visible + overscan + active-outside-viewport scheduling through the real
  Comrak service.
- Scroll, cache eviction, and source-visible fallback.
- Same-ID content-generation edit, stale preflight/completion rejection, and
  100 successive latest-window queue collapses.
- Same-ID, byte-identical context history across ordinary paragraph, certified
  first list-item paragraph, later list-item paragraph, heading, and table cell.
  Old facts are withdrawn immediately; parsing remains lazy; task facts exist
  only in the certified context.
- In-flight completion rejection when only block-owned structural context
  changes.
- Reference winner value reuse, cached presence invalidation, missed-leaf
  current-snapshot parsing, and adoption-time dependency validation.
- Native latency/retention/fact-density receipt.
- Raw `wasm32-unknown-unknown` probe exercising exact adoption, value reuse,
  presence invalidation, stale rejection, structural-context task withdrawal,
  and a 64-leaf window.

The full scorecard used during composition also exposed GFM example 631
(`a.b-c_d@a.b`) as a real bounded-inline divergence. The shared inline facade
now mirrors donor adjacent-Text coalescing and bracket context before email
autolinking; its 20 focused tests and 1,322 pristine full-parser differentials
remain green.

## Reproduction

```sh
cd tool/parser_research/lazy_inline_fact_cache_gate
cargo fmt -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run --release --bin cache_receipt

cargo fmt --manifest-path wasm_probe/Cargo.toml -- --check
RUSTC=$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  $HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo \
  build --manifest-path wasm_probe/Cargo.toml \
  --target wasm32-unknown-unknown --release
node scripts/bench_wasm.mjs
```

See [RESULTS.md](RESULTS.md) for receipts, challenged assumptions, and the next
integration gate.

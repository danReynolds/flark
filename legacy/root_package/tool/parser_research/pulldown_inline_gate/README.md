# Pulldown inline donor gate

## Verdict

**The algorithm seam passes; this retained representation fails the production
memory/hard-bound gate.**

The experiment falsifies the narrow claim that Pulldown's selected inline
algorithms intrinsically require `Tree<Item>`, `TreeIndex`, or general tree
surgery. Emphasis/strong, code spans, inline links, and reference links run over
segmented source with value-only continuation state and emit direct exact-range
facts. Every source and resolution phase can yield, including a giant code span,
a link destination/title, delimiter-stack search, nested-link disabling, and
style-fact emission.

It does **not** prove that the current syntax tape and fact vectors are a viable
Flark production representation. A 10 MiB unmatched delimiter-dense leaf retains
100,663,296 bytes of token capacity, above the 64 MiB auxiliary target. `Vec`
growth and `HashMap` rehash/allocation are counted as transitions but their copy
work cannot suspend, so the strict hard-real-time interpretation of the fuel
contract is not yet met. This is an algorithm proof, not a Gate B representation
pass.

## What was built

- `LogicalLeaf`: ordered source-backed segments plus virtual newline/space
  metadata. Virtual bytes participate in grammar but map to no invented source
  range.
- A syntax-only tape. Plain bytes are coalesced as runs and are not retained as
  tokens. Marker runs are one packed record, rather than one node per marker.
- A code-span index and resumable content inspection.
- Resumable bracket/link state with byte-stepped destination, title, and
  reference-label scans.
- Pulldown-derived delimiter flanking, modulo-three matching, and lower-bound
  logic, with stack search and match emission charged to fuel.
- Direct facts with full, marker, label, destination, title, and dependency
  ranges. Reference-definition and reference-use code share the exported
  normalization function and `UniCase` comparison.
- Cancellation checks between every charged transition.

There is no general mutable tree, parent pointer, sibling pointer, arena, or
`TreeIndex` in this crate. The library is 1,772 lines; tests and the receipt
driver bring the isolated experiment to 2,187 lines. That is material code for
a narrow subset and must be included in maintenance estimates.

## Evidence

Run:

```sh
cargo test --release --manifest-path tool/parser_research/pulldown_inline_gate/Cargo.toml
cargo clippy --manifest-path tool/parser_research/pulldown_inline_gate/Cargo.toml --all-targets -- -D warnings
cargo run --release --manifest-path tool/parser_research/pulldown_inline_gate/Cargo.toml --bin receipt
/usr/bin/time -l tool/parser_research/pulldown_inline_gate/target/release/receipt
```

Results on 2026-07-14:

```text
11 integration tests + 1 unit test passed
clippy passed with -D warnings

case=plain_10mib source_bytes=10485760 elapsed_ms=26 polls=2561 max_poll_work=4096 tokens=0 facts=0 plain_runs=1 token_capacity_bytes=0 fact_capacity_bytes=0 total_aux_bytes=0
case=giant_code_10mib source_bytes=10485762 elapsed_ms=53 polls=5121 max_poll_work=4096 tokens=2 facts=1 plain_runs=1 token_capacity_bytes=48 fact_capacity_bytes=320 total_aux_bytes=480
case=delimiter_dense_unmatched_10mib source_bytes=10485760 elapsed_ms=506 polls=8961 max_poll_work=4096 tokens=5242880 facts=0 plain_runs=5242880 token_capacity_bytes=100663296 fact_capacity_bytes=0 total_aux_bytes=100663296
case=styled_dense_1mib source_bytes=1048576 elapsed_ms=59 polls=1025 max_poll_work=4096 tokens=524288 facts=262144 plain_runs=524288 token_capacity_bytes=6291456 fact_capacity_bytes=20971520 total_aux_bytes=27263104

129040384 maximum resident set size (all four cases run sequentially)
```

The tests include:

- exact marker/content/link/reference/dependency ranges;
- clean-versus-resumed equality at fuel 1, 2, 7, and 31 after every revision in
  an edit history;
- 5,000 deterministic emphasis cases and 5,000 code-span cases differentially
  checked against `pulldown-cmark` 0.13.4;
- a curated inline-link validity/range matrix checked against Pulldown;
- nested-link disabling at fuel 1;
- Unicode case-insensitive reference matching and Flark's source-visible
  `[^1]` deviation;
- segmented virtual-byte mapping and mid-leaf cancellation.

The timings are receipts, not UI-thread budgets. The intended runtime remains a
worker isolate/Web Worker with revision cancellation. `max_poll_work=4096`
proves scheduler-visible transition accounting; it does not account for an
allocator's internal copy or OS scheduling.

## Why the dense case matters

Removing plain-text nodes fixes stock Pulldown's most obvious density problem:
a 10 MiB plain leaf retains zero syntax tokens, and a 10 MiB code span retains
two. It does not solve adversarial syntax density. The current 12-byte token plus
geometrically grown `Vec` reaches about 96 MiB for `_ ` repeated over 10 MiB.
A semantically dense 1 MiB `*a* ` leaf already retains about 26 MiB because its
262,144 exact style facts are deliberately rich Rust enums. Extrapolating that
layout to 10 MiB is disqualifying.

This is partly an output lower-bound problem, not only a parser problem. An
exact editor cannot retain millions of 80-byte heap-oriented facts under a
64 MiB cap. The production experiment therefore needs all of:

1. page-local/delta-packed marker records (target 4-6 bytes, no geometric
   capacity slack);
2. compact sealed fact chunks with chunk-local ranges and no `String` per use;
3. bounded old + pending + candidate memory during chunk replacement;
4. lazy/visible-leaf inline materialization so an adversarial off-screen leaf
   does not require a fully expanded projection;
5. external allocator/RSS instrumentation in addition to self-reported counts.

If that design cannot preserve exact clean-parse equivalence and source mapping
inside the cap, this donor direction should be killed rather than weakening the
gate.

## Explicit gaps

This crate is intentionally not a CommonMark/GFM parser:

- no block grammar, block incremental reuse, persistent suffix reuse, edit
  delta, or stable chunk identity;
- no entities/text-value materialization, images, inline HTML, hard/soft breaks,
  raw `<...>` autolinks, strikethrough, or math;
- no GFM extended bare autolink scanner. That algorithm remains a separate
  selected-profile port into the same inline machine, not a reason to introduce
  a second parser;
- reference-label scanning does not yet cover every table-specific escape and
  container-prefix rule from Pulldown;
- table rows are not implemented. Pipe ownership must use this same
  code-span/escape-aware lexer path; a second table-only inline scan would
  recreate the dual-parser risk;
- task markers remain block/list-item facts. The block machine must consume and
  own the marker range before handing the remaining leaf segments here;
- facts are grouped by resolution phase. `canonical_facts()` performs an
  intentionally test-only whole-result sort; production ordering must come from
  persistent ordered chunks;
- current `Vec`/`HashMap` growth, final retained-capacity accounting, and rich
  fact layout are not strict bounded-memory implementations;
- the packed spike offset is limited to roughly 512 MiB per logical leaf.

## Directional consequence

Do not fork stock Pulldown and expose its `FirstPass` tree as the live model. Do
use Pulldown 0.13.4 as the leading inline-algorithm donor for the next
representation experiment. Its index/value algorithms survived extraction more
cleanly than a pointer/arena API would, and selected semantics matched without a
general tree.

The next gate should combine this inline state with the compact Pulldown-derived
block/leaf-segment slice, replace both vectors with sealed packed pages, and run
the donor-neutral corpus plus external peak-memory instrumentation. Until that
passes, the correct decision is **architecture direction promising, donor seam
credible, production representation unresolved**.

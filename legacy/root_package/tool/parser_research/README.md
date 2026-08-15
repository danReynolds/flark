# Flark parser research probes

Temporary, decision-oriented experiments for the live Markdown architecture.
They are not package implementation code.

The current selected runtime architecture and its remaining stop conditions are
summarized in
[`ARCHITECTURAL_COHERENCE_AUDIT.md`](ARCHITECTURAL_COHERENCE_AUDIT.md),
[`PARSER_DONOR_BAKEOFF.md`](PARSER_DONOR_BAKEOFF.md),
[`PARSER_CONTROL_RENDEZVOUS.md`](PARSER_CONTROL_RENDEZVOUS.md),
[`REFERENCE_PREFIX_FINALIZER_GATE.md`](REFERENCE_PREFIX_FINALIZER_GATE.md),
[`REFERENCE_LABEL_NORMATIVE_GATE.md`](REFERENCE_LABEL_NORMATIVE_GATE.md),
[`SELECTED_RUNTIME_EXTRACTION_AUDIT.md`](SELECTED_RUNTIME_EXTRACTION_AUDIT.md),
[`PACKED_SERIALIZED_GREEN_GATE.md`](PACKED_SERIALIZED_GREEN_GATE.md),
[`DIRECT_PARSER_GREEN_COMPOSITION_GATE.md`](DIRECT_PARSER_GREEN_COMPOSITION_GATE.md),
and
[`SERIALIZED_GREEN_PROJECTION_SUFFICIENCY_AUDIT.md`](SERIALIZED_GREEN_PROJECTION_SUFFICIENCY_AUDIT.md).
The first restart-composer-to-packed-storage mechanical join is executable in
[`composed_adoption_storage_gate/`](composed_adoption_storage_gate/), but its
authority/adoption claim is rejected by
[`COMPOSED_STORAGE_AUTHORITY_AUDIT.md`](COMPOSED_STORAGE_AUTHORITY_AUDIT.md): it
proves only an immutable Enter rewrite until storage derives the source/root
proof and commits the complete recipe plus suffix atomically.

The broader completion criteria and stop conditions are tracked in
[`ARCHITECTURE_PROOF_LEDGER.md`](ARCHITECTURE_PROOF_LEDGER.md). It distinguishes
isolated mechanism evidence from a composed architecture acceptance result.
The current executable composition starts in
[`v3_runtime_slice/`](v3_runtime_slice/): its step-1 result composes exact Crop
lineage, a generation-safe page arena, and the reduced latest-one coordinator
without a grammar. The exact block continuation and repair aggregate are in
[`comrak_value_block_core/`](comrak_value_block_core/). The oversized lexical
path is split between the all-family handwritten semantic witness in
[`oversized_block_line_gate/`](oversized_block_line_gate/) and the one-family
single-source generation/maintenance proof in
[`generated_scanner_gate/`](generated_scanner_gate/). The selected source,
generated-scanner, donor-transition, and authoritative-replay seam plus its
remaining falsification gates are recorded in
[`GIANT_LINE_SOURCE_SCANNER_JOIN.md`](GIANT_LINE_SOURCE_SCANNER_JOIN.md).

`FINDINGS.md` and the current RFC 023 are chronological evidence and an
outdated draft, respectively; neither is the current implementation plan. The
active ownership contracts are
[`ARCHITECTURE_STATE_PARTITION.md`](ARCHITECTURE_STATE_PARTITION.md),
[`DART_SOURCE_STATE_PARTITION.md`](DART_SOURCE_STATE_PARTITION.md), and
[`V3_PACKAGE_MIGRATION_BOUNDARY.md`](V3_PACKAGE_MIGRATION_BOUNDARY.md). The
Flutter authority/frame-adoption mechanism and its still-open production input,
layout, adapter, and device gates are recorded in
[`FLUTTER_PARSER_TO_PAINT_GATE.md`](FLUTTER_PARSER_TO_PAINT_GATE.md). The
one-current-root versus root-history source decision and its AOT/GC receipts are
recorded in
[`dart/CURRENT_ROOT_INVERSE_SOURCE_RESULTS.md`](dart/CURRENT_ROOT_INVERSE_SOURCE_RESULTS.md).
The
device/UI/layout pressure test is recorded separately in
[`PHASE0_FEASIBILITY.md`](PHASE0_FEASIBILITY.md). The first
representative parser-fork upgrade is recorded in
[`COMRAK_MAINTENANCE_REHEARSAL.md`](COMRAK_MAINTENANCE_REHEARSAL.md).
The later checkpoint/state/output audit that rejects the narrow fork as the
lifetime representation is
[`COMRAK_STATE_OUTPUT_FALSIFICATION.md`](COMRAK_STATE_OUTPUT_FALSIFICATION.md).
The symmetric resource/semantic audit that rejects the current clean-room crate
as a production seed is
[`OWNED_PROTOTYPE_FALSIFICATION.md`](OWNED_PROTOTYPE_FALSIFICATION.md).
The maintained-fork versus Flark-owned-parser decision is pressure-tested in
[`PURPOSE_BUILT_PARSER_FEASIBILITY.md`](PURPOSE_BUILT_PARSER_FEASIBILITY.md).
The inverted, spec-first owned-parser execution plan and its pinned profile are
recorded in [`OWNED_PARSER_SPEC_TRIAL.md`](OWNED_PARSER_SPEC_TRIAL.md) and
[`OWNED_PARSER_SPEC_PROFILE.json`](OWNED_PARSER_SPEC_PROFILE.json).
The evolving receipt from that phase is
[`OWNED_PARSER_TRIAL_RESULTS.md`](OWNED_PARSER_TRIAL_RESULTS.md). The integrated
decision update is in [`PARSER_DONOR_BAKEOFF.md`](PARSER_DONOR_BAKEOFF.md):
Flark owns the runtime and control protocol, while individual donor algorithms
must earn their narrow seams. The working actor/parser boundary is specified in
[`PARSER_CONTROL_RENDEZVOUS.md`](PARSER_CONTROL_RENDEZVOUS.md). The still-earlier
Comrak-exclusive conclusion is preserved as a superseded evidence step in
[`PARSER_AUTHORITY_REASSESSMENT.md`](PARSER_AUTHORITY_REASSESSMENT.md).

The symmetric value-state seams are executable under
[`comrak_derived_core/`](comrak_derived_core/) and
[`pulldown_derived_core/`](pulldown_derived_core/). The donor-neutral acceptance
contracts are [`gate_a_harness/`](gate_a_harness/) for block structure and
[`gate_b_harness/`](gate_b_harness/) for inline/reference semantics. Their
passing self-tests prove that the gates reject known shortcuts, not that a
candidate parser passes either gate.

The Pulldown-derived inline extraction is under
[`pulldown_inline_gate/`](pulldown_inline_gate/). It proves that selected inline
algorithms do not intrinsically require Pulldown's mutable tree, while its
10 MiB dense-delimiter receipt deliberately fails the production memory gate.
[`packed_inline_state/`](packed_inline_state/) then demonstrates the compact
memory/scheduling representation independently, and
[`checkpoint_restart_state/`](checkpoint_restart_state/) demonstrates genuine
restart, exact open-state convergence, spanning-fact safety, and suffix reuse.
These were complementary mechanism proofs, not a composed CommonMark parser or
a Gate B pass. At that stage the next useful experiment was the integrated
commitment slice; the later exact Comrak-correspondent and bounded-inline gates
are reflected in the proof ledger at the top of this file.

## Owned-parser stop/go prototype

The independent crate under [`owned_parser/`](owned_parser/) certifies a
61-example foundation, prints full CommonMark and pinned stress scorecards,
tests persistent resumable checkpoints, and contains the full/pathological
benchmarks used in the parser decision.

```sh
cd owned_parser
cargo test --release -- --nocapture
cargo run --release --bin owned_parser_bench
cargo run --release --bin owned_parser_pathological
cargo run --release --bin owned_parser_nested -- 5000
cargo run --release --bin owned_backtick_pathological
cargo run --release --bin owned_blockquote_pathological
cargo test --release --test unified_machine_slice -- --nocapture
```

## GFM compatibility probe

Build the cmark-compatible Pulldown and `markdown-rs` renderers:

```sh
cargo build --release --bin cmark_compat
cargo build --release --bin markdown_rs_compat
```

Then run either cmark-gfm's `spec.txt` or `extensions.txt` through its own
normalizing test runner, supplying the extensions Flark enables.

`canonical_spec_diff.py` additionally removes harmless renderer differences
such as entity escaping, table alignment serialization, and empty table bodies
before reporting likely parser-semantic mismatches.

`full_parser_bakeoff` is a coarse cold/full-parse cost check for parser-fork
triage. It is not an incremental-parser benchmark and does not by itself select
an engine.

```sh
cargo run --release --bin full_parser_bakeoff
```

`pulldown_stock_memory` runs each pathological stock representation in a fresh
process so `/usr/bin/time -l` supplies an external RSS receipt instead of a
candidate-owned counter.

```sh
cargo build --release --bin pulldown_stock_memory
/usr/bin/time -l target/release/pulldown_stock_memory dense-lines
/usr/bin/time -l target/release/pulldown_stock_memory dense-inline
/usr/bin/time -l target/release/pulldown_stock_memory giant-line
```

## Edit-locality probe

```sh
cargo run --release --bin edit_locality
```

The probe fully parses the before and after documents with Comrak, then reports
separate structural and semantic invalidation envelopes. Full parsing is the
oracle; the probe does not claim to be an incremental parser.

## Pathological block probe

```sh
cargo run --release --bin pathological_blocks
```

This measures raw Comrak parsing for giant single paragraphs, fenced blocks,
and list containers. It tests whether a design that only checkpoints between
top-level blocks has hidden size limits.

## Exact Comrak checkpoint fork probe

`0001-Prototype-incremental-checkpoints-and-persistent-suf.patch` applies to
Comrak 0.50.0. It is deliberately test-heavy research code. It covers fresh
safe-checkpoint restoration, persistent source and syntax trees, exact suffix
splicing, checkpoints inside large lists/tables/raw blocks, local list
tightness summaries, inline leaf caches, reference dependencies and duplicate
definition precedence, revision/hash validation, adversarial transitions, and
the ignored 10,000-edit 1 MB oracle soak.

Example against a disposable Comrak checkout:

```sh
git apply /absolute/path/to/tool/parser_research/0001-Prototype-incremental-checkpoints-and-persistent-suf.patch
cargo test --release incremental_checkpoint_probe -- --nocapture
cargo test --release \
  checkpoint_chunks_remain_exact_across_ten_thousand_large_document_edits \
  -- --ignored --nocapture
```

The second command intentionally takes several minutes because it performs a
fresh 1 MB full parse after every incremental edit.

`0002-Extract-stateful-incremental-parser-API.patch` is the net follow-up patch
against Comrak 0.54.0. It moves the engine into a dedicated module, exposes a
narrow external API with real changed-chunk data, and leaves only 53 additions
and five deletions in existing upstream files. Its machine-readable base,
checksum, diff ownership, receipts, and open gaps are recorded in
`COMRAK_PATCH_INVENTORY.json`.

## Purpose-built parser kernel

The Flark-owned kernel spike isolates explicit block state, checkpoint
convergence, and suffix reuse from full CommonMark implementation work. It is
intentionally not a conforming parser; its value is measuring the machinery and
making adversarial state propagation executable.

```sh
cargo test --release --bin purpose_built_parser_spike -- --nocapture
cargo run --release --bin purpose_built_parser_spike
```

## Revisioned native/WASM source ABI

`src/lib.rs` is a disposable C/WASM ABI around a persistent UTF-8 source rope.
It validates revisions, a shared Dart/Rust UTF-8 fingerprint, byte ranges, and
UTF-8 scalar boundaries. It also exports a bounded stock-Comrak fragment parse
for a real WASM cost receipt; the resumable Comrak engine itself remains in the
fork patch.

```sh
cargo test --lib

RUSTC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc" \
  "$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo" \
  build --release --target wasm32-unknown-unknown --lib
node run_wasm_revisioned_handle.mjs
```

The explicit toolchain is needed on this machine because Homebrew's `rustc`
does not see rustup-installed targets.

## Dart hot-path and offload probes

The source-tree probe measures one local edit plus one 128-character range read
while retaining a bounded undo window. The isolate probe measures only the
message/scheduling floor of a persistent worker. Neither is a Flutter frame or
physical-device benchmark.

```sh
dart run dart/jank_model_probe.dart
dart run dart/isolate_roundtrip_probe.dart
```

## Binary delta codec and Flutter slice

`dart/incremental_delta_codec.dart` and `src/bin/delta_codec.rs` share a binary
golden for the revisioned syntax/projection delta. The Flutter vertical slice
connects that codec to the persistent Dart source and block aggregate, a
50,000-block lazy viewport, one composing input shard, and local rebuilds.

```sh
cargo test --bin delta_codec

flutter test --reporter expanded \
  ../../test/prototype/flark_revisioned_document_prototype_test.dart \
  ../../test/prototype/flark_incremental_delta_codec_prototype_test.dart \
  ../../test/prototype/flark_incremental_vertical_slice_prototype_test.dart \
  ../../test/prototype/flark_document_selection_coordinator_probe_test.dart
```

## Product-feel, web, and oversized-layout probes

The product-feel slice feeds real Comrak output through exact projection into
one persistent active `EditableText` in a 50,000-block variable-height
viewport. It compares delimiter-hidden and active-syntax-reveal modes and
covers a strong-delimiter completion, IME composition, a fence transition, and
paint.

The wrapping probes distinguish monolithic Flutter paragraph layout from a
resumable fixed-window break-index continuation. The latter deliberately
includes insertions/deletions that propagate wrapping through most of a 1 MB
paragraph.

```sh
flutter test --reporter expanded \
  ../../test/prototype/flark_product_feel_vertical_slice_prototype_test.dart \
  ../../test/prototype/flark_wrapped_paragraph_layout_probe_test.dart \
  ../../test/prototype/flark_incremental_wrap_convergence_probe_test.dart
```

The web probe requires Flutter's web test platform, not the `-d chrome` device
alias. It profiles the warmed whole-result bridge and verifies that an async
WASM completion can publish authoritative styling before the next paint.

```sh
flutter test --platform chrome --reporter expanded \
  ../../test/prototype/flark_web_wasm_urgent_path_probe_test.dart
```

Two Node probes split pure Comrak WASM work from the packaged bridge's
parse/payload and JSON-decode phases. Rebuild the packaged WASM after changing
the bridge.

```sh
node probe_comrak_fragment_wasm.mjs
node probe_packaged_comrak_wasm.mjs
```

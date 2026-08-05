# Comrak inline-fragment feasibility gate

This prototype answers one narrow architecture question: can Flark call the real
Comrak 0.54 inline parser on a block-spine-certified leaf and receive exact,
compact editor facts without maintaining a second predictive Markdown grammar?

The answer is conditionally yes. See [RESULTS.md](RESULTS.md) for the evidence,
limits, and scheduling implications. This directory is research-only; it does
not change the production bridge.

## Layout

- `vendor/comrak/`: shared research checkout. It also contains unrelated block
  probe edits, so do not use its whole-directory diff as the inline patch.
- `patches/comrak-inline-fragment-0.54.0.patch`: replayable inline-only patch
  against pristine Comrak 0.54.0.
- `src/origin_map.rs`: architecture-owned segmented logical-to-physical map.
- `tests/inline_fragment.rs`: inline semantics, projection, references, caps,
  and upstream single-leaf differentials.
- `tests/task_list_phase.rs`: strict post-inline task-list recognition,
  cross-phase precedence, source origins, and incomplete-typing transitions.
- `tests/full_parser_regression.rs`: patched-versus-pristine full-parser
  differential with annotations disabled.
- `src/bin/inline_fragment_bench.rs`: hot-leaf native benchmark.
- `src/bin/inline_document_bench.rs`: cold streamed-document benchmark with
  allocation and retention accounting.
- `wasm_probe/` and `scripts/bench_wasm.mjs`: raw WebAssembly benchmark.
- `scripts/check_patch_inventory.py` and `provenance/`: deterministic isolated
  patch replay, hashes, line inventory, and sensitive-function pins.
- `UPGRADE_REHEARSAL.md`: current-main replay and source-changing release
  boundary maintenance evidence.

## Reproduce

```sh
cargo test --test inline_fragment --no-default-features -- --nocapture
cargo test --test task_list_phase --no-default-features
cargo test --test full_parser_regression --no-default-features -- --nocapture
cargo test -p comrak --lib
cargo test -p comrak --doc
cargo clippy -p comrak --lib --no-default-features -- -D warnings
cargo clippy -p flark-comrak-inline-fragment-gate \
  --lib --bins --tests --no-default-features -- -D warnings
python3 scripts/check_patch_inventory.py
sh scripts/run_benchmarks.sh
sh scripts/compare_text_storage.sh
```

Task recognition is deliberately split at a narrow phase boundary. The block
spine certifies only that a leaf is the first paragraph child of an item under
a list (independent of whether the list is tight). The inline service then runs
Comrak's generated strict task scanner after the definitive inline parse and
only when the first inline child remains Text. This preserves entity decoding,
escape handling, and resolved-reference precedence without a second Markdown
grammar or a synthetic List/Item AST.

Apply the isolated patch from a pristine Comrak 0.54.0 checkout:

```sh
git apply --check /path/to/comrak-inline-fragment-0.54.0.patch
git apply /path/to/comrak-inline-fragment-0.54.0.patch
cargo check --lib --no-default-features
```

After an intentional change to the isolated wrapper, regenerate only its
new-file patch section and the manifest with
`python3 scripts/check_patch_inventory.py --refresh`, then replay the result
from pristine source. The shared vendor checkout also contains block research;
never regenerate the patch from its whole-directory diff.

The `research-large-inline` feature raises only the research ceilings to 64
KiB. The default remains an 8 KiB candidate urgent-path ceiling. The
`research-owned-text` feature restores the discarded owned-Text wire format so
the retention comparison is reproducible; it is not the recommended format.

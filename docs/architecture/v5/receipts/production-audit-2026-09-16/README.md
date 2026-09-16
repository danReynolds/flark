# Local production-audit receipts

See [the audit](../../production_audit_2026_09_16.md) for interpretation, fixes and
open release findings. Base is `50e13d43d449bf1aef65780f39639d82606237e5`;
measurements used the dirty candidate on `codex/production-audit`.
`candidate.json` records production source and profiling artifact SHA-256 values
at measurement time. `final-checks.json` records final tool/source hashes and
the browser candidate. No CI or independent peer review is claimed.

## Receipt map

- `parse-before.jsonl` / `parse-after.jsonl`: release Rust scaling samples from
  `native/flark_parse/examples/perf_audit.rs`, five warmups and 25 samples. Run
  `cargo run --release --example perf_audit` in `native/flark_parse`. All 15
  before/after model fingerprints match. Before used baseline extraction with
  the same diagnostic tool, after used this candidate.
- `kernel-before.log.gz` / `kernel-after.log.gz`: pinned 16 KiB benchmark from
  `dart run tool/bench_editor.dart 16` in `packages/flark`. Both failed the
  unchanged 4 ms gate. The after run experienced major competing machine load;
  neither a clean regression comparison nor a qualification pass is claimed.
- `fleury-cpu.jsonl`: initial diagnostic with dense fixtures. Some code/table
  cases fell back to source; code coloring revision was zero. Retained for
  transparency, not evidence of live-code performance.
- `fleury-cpu-final.jsonl`: final tool and fixtures, run with
  `dart run tool/perf_audit.dart` in `packages/flark_fleury`. 50 warmup and 100
  measured input pairs per case; actual input dispatch, cell layout and paint,
  source/caret/first-character assertions, worker colors, 40/80-column resize.
  Edits occur in a tail paragraph. No OS input, DOM/terminal presentation, scroll
  latency or real image decoding is included. Source fallback is preserved.
- `images.jsonl`: throwaway native default-preview diagnostic, mock HTTP PNG
  responses and real synchronous decode/resize. Shows the UI-isolate cost,
  including an allowed 2048-square image. Not a universal latency bound.
- `editing.jsonl`: throwaway source/caret probes for deferred structural deletion
  and synthesized missing table cells; inspect the exact source and command.
- `resource-red.log.gz`, `flutter-resource-red.log.gz`: reproduced lifetime
  failures before correction. Corresponding `*-resource-green.log.gz` files
  contain passing targeted tests after correction.
- `definition-regression.log.gz`, `rust-final.log.gz`, `transports.log.gz`:
  exact coordinate regression; full native tests; native/bundled/fresh Wasm
  parity across 1,322 cases. Commands are in the root README.
- `*-analysis*.log.gz`, `*-tests.log.gz`: package analysis and full kernel,
  Flutter-host and Fleury-host test suites. The last Fleury analysis also
  includes the final diagnostic tool. Framework and example suites were not
  rerun wholesale in this pass; see prior closeout for earlier results.
- `macos-result.json`, `macos-summary.json.gz`, `macos-samples.json.gz`,
  `macos-workbench.log.gz`: completed current-candidate 100-cycle / 3,000-edit
  profile, **failed** table reflow budget. Raw samples came from the failing
  integration run's teardown/stdout, not an older successful driver JSON.
  Local VM service URLs have been redacted from the log. Accessibility errors
  and competing workload are retained; no foreground loss occurred.
- `macos-normal-build.log.gz`, `web-build.log.gz`: ordinary macOS entry point
  restored after profiling, and final Fleury browser candidate build.

Run the native workbench from `packages/flark_flutter/example`:

```sh
flutter drive --profile -d macos \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/workbench_profile_test.dart
flutter build macos --profile --target lib/main.dart
```

The workbench requires foreground and uses a separate synthetic preference
namespace. Machine contention was observed in this session. Do not lower budgets
or discard failures when scheduling a controlled qualification repeat.

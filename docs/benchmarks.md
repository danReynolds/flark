# Benchmarks

Flark ships an enforced benchmark lane. Budgets are failing assertions, not
warnings.

```bash
./scripts/verify_benchmark_lane.sh
```

The lane runs `flutter test --tags benchmark test/v2/performance` with
`FLARK_BENCHMARK_ENFORCE_BUDGETS=true`. Each case prints a `flark_benchmark`
line with min/median/p95/max so regressions are visible in CI logs.

## Document-size sweep

Two headline numbers at 1 KB / 100 KB / 1 MB of realistic Markdown (headings,
emphasis, links, task lists, code fences). Captured on an Apple Silicon dev
machine under the Dart test VM — indicative of relative scaling, not absolute
release-AOT latency.

| Document | Keystroke apply (median / p95) | Native parse + decode (median) |
| --- | --- | --- |
| 1 KB | 4 µs / 19 µs | 1.5 ms |
| 100 KB | 172 µs / 1.06 ms | 306 ms |
| 1 MB | 5.5 ms / 18.9 ms | ~41 s ⚠️ |

- **Keystroke apply** is the synchronous core hot path
  (`FlarkEditorState.applyTransaction`: text-buffer rebuild + line reindex +
  selection map + history inverse). It scales cleanly with document length —
  sub-millisecond through 100 KB. The ~5.5 ms at 1 MB reflects the flat-string
  buffer's O(n) rebuild per edit (lifting that ceiling is tracked as a piece
  table / rope behind `FlarkTextBuffer`).

- **Native parse + decode** is the full `FlarkNativeComrakParseBackend.parse`
  (Comrak parse → JSON payload → Dart decode → result synthesis).

## ⚠️ Known issue: super-linear parse at very large documents

Native parse + decode is **super-linear** — roughly O(n²): 100 KB → 1 MB is a
10× input increase but a ~135× time increase (306 ms → ~41 s). An isolate alone
does not fix this (it moves the 41 s off the UI thread but the render plan still
will not update for 41 s); the algorithmic bottleneck in the parse/marker-
mapping/decode pipeline must be found and fixed. Tracked for Phase 3.

Because the 1 MB parse is currently ~41 s, it is **not** part of the routine
lane and is report-only. Reproduce it explicitly:

```bash
flutter test --tags benchmark \
  test/v2/performance/flark_v2_large_document_benchmark_test.dart \
  --dart-define=FLARK_BENCHMARK_HEAVY=true
```

## Budgets

Budgets are generous regression trackers (≈10× headroom over observed medians),
not tight SLAs — native parse timing varies widely with machine speed. They
exist to catch order-of-magnitude regressions, not to pin absolute numbers.

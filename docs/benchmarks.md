# Benchmarks

Flark ships an enforced benchmark lane. Budgets are failing assertions, not
warnings.

```bash
./scripts/verify_benchmark_lane.sh
```

The lane runs `flutter test --tags benchmark test/v2/performance` with
`FLARK_BENCHMARK_ENFORCE_BUDGETS=true`. Each case prints a `flark_benchmark`
line with min/median/p95/max so regressions are visible in CI logs.

These lane tests run in the **debug** test VM — good for relative scaling and
regression tracking, but not real-device frame time. For true profile-mode
frame timing (AOT, real raster) of live-rendered editing, run the harness in
`example/lib/perf_harness.dart` — see
`docs/architecture/live_rendered_rebuild_isolation.md` (profile numbers show the
per-keystroke build phase breaching the 60 fps budget at ~20–25 blocks).

## Document-size sweep

Two headline numbers at 1 KB / 100 KB / 1 MB of realistic Markdown (headings,
emphasis, links, task lists, code fences). Captured on an Apple Silicon dev
machine under the Dart test VM — indicative of relative scaling, not absolute
release-AOT latency.

| Document | Keystroke apply (median / p95) | Native parse + decode (median) |
| --- | --- | --- |
| 1 KB | 4 µs / 19 µs | 1 ms |
| 100 KB | 172 µs / 1.06 ms | 55 ms |
| 1 MB | 5.5 ms / 18.9 ms | ~0.5 s |

- **Keystroke apply** is the synchronous core hot path
  (`FlarkEditorState.applyTransaction`: text-buffer rebuild + line reindex +
  selection map + history inverse). It scales cleanly with document length —
  sub-millisecond through 100 KB. The ~5.5 ms at 1 MB reflects the flat-string
  buffer's O(n) rebuild per edit (lifting that ceiling is tracked as a piece
  table / rope behind `FlarkTextBuffer`).

- **Native parse + decode** is the full `FlarkNativeComrakParseBackend.parse`
  (Comrak parse → JSON payload → Dart decode → result synthesis). It is linear
  in document size: the 1 MB case sits at ~0.5 s, ~10× the 100 KB case.

### History: the O(n²) parse fix

The Dart result-synthesis layer (`_mapNativeResult`) was previously
**super-linear** — ~O(n²), so 1 MB took ~41 s. Three per-block scans over all
blocks/markers were the cause (renderable-block filtering, marker-only block
detection, and code-fence marker matching). Each became O(log n) with a
start-sorted index plus a prefix-max-end array, dropping 1 MB parse+decode from
~41 s to ~0.5 s (~88×) with no behavior change. The bridge (Comrak parse + JSON
+ decode) was already linear and untouched. The 1 MB parse budget now guards
against a quadratic regression.

## Budgets

Budgets are generous regression trackers (≈10× headroom over observed medians),
not tight SLAs — native parse timing varies widely with machine speed. They
exist to catch order-of-magnitude regressions, not to pin absolute numbers.

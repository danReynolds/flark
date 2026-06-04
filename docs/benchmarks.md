# Benchmarks

This is the benchmark scoreboard for Flark. Keep it current whenever perf work
lands: every optimization should leave behind a number, a comparison point, and
a conclusion about where to focus next.

## Current Conclusions

| Area | Current signal | Conclusion | Next focus |
| --- | --- | --- | --- |
| Live-rendered rebuild fanout | `builds_per_edit=1.0` through 10/20/40/80 blocks in the focused debug rebuild benchmark | Fixed: unchanged blocks no longer rebuild just because source/display offsets shift | Keep the benchmark as a regression gate |
| Live-rendered profile frame time | 40/80 blocks x end/start edits: `build_median=1.16-1.53ms`, `build_p95<=2.12ms` | Safely under the 16.7ms frame budget with large headroom | Re-run the profile gate after major widget changes |
| Peer comparison | Flark debug medians are flat at `8.87-10.17ms`; current super_editor is `5.14-6.19ms`; current flutter_quill is `4.67-5.00ms` | Peer-competitive on scaling and profile latency, not peer-leading on debug constant factor | Only chase live-block constant cost if peer-leading debug numbers become a product goal |
| Large-document source transaction | 1MB apply: `1.99ms` median / `6.88ms` p95 | Below a 60fps frame budget in the current lane | Piece table / rope only if 1MB live source editing becomes a product target |
| Native parse + decode | 1MB parse+decode: `712ms` median in the latest full run after mapper optimization | Improved by ~1.6-2.4x from the previous `1.17s`, but still the largest raw perf number | Continue with payload decode and result mapping before parser architecture |

**Current decision:** the broad live-rendered perf architecture work has hit
diminishing returns for the measured 40-80 block editing path. The remaining
live-rendered opportunity is a narrow constant-factor pass, not another rebuild
architecture rewrite. If Flark needs more perf work next, the clearer target is
huge-document parse/decode or explicit 1MB editing pressure.

**Narrow constant-factor pass:** a task-list hot-path refactor now skips
block-style signature construction for body blocks and skips text-span
segmentation when a rebuilt block has no inline runs. This removes real
per-frame work from the focused benchmark path, but the measured outcome is not
a decisive win: debug medians stayed in the same ~9-10ms band and profile-mode
frame timing stayed in the same ~1-2ms band. Keep the cleanup; do not use it as
evidence for more live-block constant-factor work.

## Live-Rendered Editing

### Debug Rebuild Benchmark

Harness:

```bash
flutter test test/v2/performance/flark_live_rendered_rebuild_benchmark_test.dart \
  --tags benchmark \
  --reporter compact
```

Latest local snapshot:

| Case | Current Flark pump median / p95 | Current rebuild fanout | Current peer median / p95 | Previous Flark shape |
| --- | --- | --- | --- | --- |
| 10 blocks | `8.87ms` / `11.80ms` | `1.0` block/edit | super_editor `5.85ms` / `12.50ms`; flutter_quill `4.67ms` / `8.67ms` | ~21ms, all-block fanout |
| 20 blocks | `8.95ms` / `11.43ms` | `1.0` block/edit | super_editor `5.49ms` / `14.21ms`; flutter_quill `4.90ms` / `10.58ms` | ~27ms, all-block fanout |
| 40 blocks | `9.69ms` / `12.05ms` | `1.0` block/edit | super_editor `6.19ms` / `14.21ms`; flutter_quill `4.90ms` / `13.31ms` | ~44ms, all-block fanout |
| 80 blocks | `10.17ms` / `11.83ms` | `1.0` block/edit | super_editor `5.14ms` / `8.98ms`; flutter_quill `5.00ms` / `6.02ms` | ~72ms, all-block fanout |
| 80 blocks, 600px viewport | `9.52ms` | `1.0` block/edit | not measured | Use this to decide whether virtualization is worth it |
| Source mode, 40 blocks | `1.80ms` / `2.45ms` | one editable | lower bound for source-string editing | already flat |

Interpretation: this is a debug test-VM benchmark. Use it for scaling shape and
regression detection, not real device latency. The important current signal is
that live-rendered editing is now flat in block count for unchanged blocks.

### Profile-Mode Frame Timing

Harness:

```bash
./scripts/verify_live_rendered_profile.sh
```

The profile gate runs `FLARK_PROFILE_EDIT=end|start` over 40 and 80 blocks on
`macos` by default and enforces generous build-median/build-p95 budgets. Use
these variables to narrow or tune the sweep:

- `FLARK_PROFILE_DEVICE`
- `FLARK_PROFILE_BLOCKS_LIST`
- `FLARK_PROFILE_EDITS_LIST`
- `FLARK_PROFILE_END_MEDIAN_BUDGET_MS`
- `FLARK_PROFILE_END_P95_BUDGET_MS`
- `FLARK_PROFILE_START_MEDIAN_BUDGET_MS`
- `FLARK_PROFILE_START_P95_BUDGET_MS`

Latest refreshed profile sweep:

| Case | Build median | Build p95 | Raster median | Raster p95 | Conclusion |
| --- | --- | --- | --- | --- | --- |
| 40 blocks, end edit | `1.16ms` | `2.12ms` | `558us` | `878us` | under 60fps budget with headroom |
| 40 blocks, start edit | `1.25ms` | `1.71ms` | `649us` | `1.11ms` | offset-shift case stays flat |
| 80 blocks, end edit | `1.53ms` | `1.80ms` | `666us` | `820us` | under 60fps budget with headroom |
| 80 blocks, start edit | `1.53ms` | `1.76ms` | `604us` | `715us` | offset-shift case stays flat |

After the narrow text-span fast path:

| Case | Build median | Build p95 | Raster median | Raster p95 | Read |
| --- | --- | --- | --- | --- | --- |
| 40 blocks, end edit | `1.18ms` | `1.40ms` | `600us` | `699us` | same fast band |
| 40 blocks, start edit | `1.18ms` | `2.04ms` | `635us` | `1.26ms` | same fast band |
| 80 blocks, end edit | `1.49ms` | `1.69ms` | `610us` | `728us` | same fast band |
| 80 blocks, start edit | `1.69ms` | `2.12ms` | `673us` | `782us` | same fast band |

Interpretation: the refactor is code-health positive and removes unnecessary
work, but profile timing was already so low that the practical perf result is
noise-level. This supports moving on unless a product goal specifically demands
peer-leading debug pump medians.

Older profile-mode progress:

| Blocks | Baseline | Stable ids | Instance reuse, end edit | Worst case, start edit |
| --- | --- | --- | --- | --- |
| 20 | `10.8ms` | `4.9ms` | `1.6ms` | not recorded |
| 40 | `25.0ms` | `9.2ms` | `1.9ms` | `7.6ms` |
| 80 | ~`50ms` | ~`18ms` | `2.2ms` | `13.1ms` |

Interpretation: debug rebuild fanout tells us whether the architecture scales;
profile-mode timing tells us whether users get missed frames.

## Document-Size Sweep

Harness:

```bash
./scripts/verify_benchmark_lane.sh
```

The enforced lane runs:

```bash
flutter test --tags benchmark test/v2/performance \
  --dart-define=FLARK_BENCHMARK_ENFORCE_BUDGETS=true \
  --reporter compact
```

Each benchmark prints a `flark_benchmark` line with min/median/p95/max. Budgets
are failing assertions, not warnings.

Two headline numbers at 1KB / 100KB / 1MB of realistic Markdown:

| Document | Keystroke apply median / p95 | Native parse + decode median / p95 | Current read |
| --- | --- | --- | --- |
| 1KB | `3us` / `3us` | `2.00ms` / `15.07ms` | trivial; parse p95 includes cold/noisy samples |
| 100KB | `161us` / `624us` | `94.83ms` / `179.50ms` | synchronous apply is healthy |
| 1MB | `1.99ms` / `6.88ms` | `712.46ms` / `712.46ms` | async parse/decode is the only large headline number |

- **Keystroke apply** is the synchronous core hot path
  (`FlarkEditorState.applyTransaction`: text-buffer rebuild, line reindex,
  selection map, history inverse). It scales cleanly through 1MB in the current
  lane, with p95 still below a 60fps frame budget.
- **Native parse + decode** is `FlarkNativeComrakParseBackend.parse` (Comrak
  parse, JSON payload, Dart decode, result synthesis). It is no longer
  pathological, but the 1MB async cost is the clearest remaining performance
  area if huge documents become important.

## Peer Calibration

Peer harnesses live outside the package:

- `benchmark/peer/` for flutter_quill
- `benchmark/peer_supereditor/` for super_editor

Use [benchmark/peer/README.md](../benchmark/peer/README.md) for peer setup and
toolchain caveats.

The useful conclusion is architectural: super_editor proves a block-based live
editor can be flat, and current Flark now matches that scaling shape. Current
peer reruns show Flark is still higher on debug pump constant factor, but the
profile-mode frame timing is already comfortably below budget. That makes broad
live-rendered architecture work a poor next bet unless we specifically want
peer-leading debug numbers.

## What To Focus On Next

1. Keep live-rendered rebuild/profile gates in CI-adjacent validation and move
   regular product work forward.
2. If peer-leading live-rendered numbers matter, run a narrow constant-factor
   investigation around live block chrome/layout. Do not restart the rebuild
   architecture unless `builds_per_edit` regresses.
3. If large files are a priority, investigate native parse/decode payload cost
   before text-buffer storage: current 1MB source apply is below frame budget.
4. Treat viewport virtualization as deferred until first-paint or huge visible
   documents become the measured bottleneck. It is no longer the per-keystroke
   rebuild-fanout fix.

## Large-Document Pitch

The current large-document bottleneck is not synchronous typing. At 1MB,
`keystroke_apply` is `1.99ms` median / `6.88ms` p95, while native parse+decode
is `712.46ms` median in the latest full run after optimizing
`FlarkUtf8Utf16Mapper` to avoid per-scalar `utf8.encode` calls. A prior
post-mapper run measured `498.42ms`; treat the durable signal as a clear
large-doc parse/decode win, not a tight SLA.

The phase profile now prints:

```text
flark_benchmark native_parse_profile_1MB_1000080chars iterations=3
  total_median=550.26ms utf8_encode_median=2.90ms
  bridge_total_median=191.67ms native_parse_median=72.86ms
  payload_decode_median=116.53ms result_mapping_median=355.52ms
  input_bytes=1000080 payload_bytes=8138528
```

The profiled total is noisier than the non-profiled headline, but the ordering
is clear: Flark-side result mapping and JSON payload decode dominate native
Comrak parse time. The next useful work is:

1. Split `_mapNativeResult` into subphases so the remaining `result_mapping`
   cost is attributable.
2. Reduce JSON payload decode cost or replace the wire format if decode remains
   above native parse time.
3. Optimize marker/list/code-fence mapping before considering incremental parse:
   native parse itself is currently smaller than payload decode and result
   mapping.
4. Leave `FlarkTextBuffer`/piece-table work behind this unless a new benchmark
   shows 1MB synchronous apply crossing frame budget again.

## Budget Policy

Budgets are generous regression trackers, not tight SLAs. They exist to catch
order-of-magnitude regressions and shape regressions. If a budget changes, the
commit should say which machine/harness produced the new baseline and why the
new threshold still catches the failure mode we care about.

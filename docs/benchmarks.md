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
| Large-document source transaction | 1MB apply: `2.61ms` median / `12.83ms` p95 | Below a 60fps frame budget in the current lane | Piece table / rope only if 1MB source apply crosses frame budget again |
| Large-document editor peer calibration | 1MB source editor: build `1.14ms`, apply `1.95ms`, pump `259.68ms` median; super_editor `3.20ms` / `755us` / `98.36ms`; flutter_quill `1.83s` / `962.66ms` / `153.54ms` | Build/apply are competitive or leading; 1MB viewport pump trails peers | Source viewport architecture before more text-buffer work |
| Large-document viewport probes | 1MB pump: Flark source `259.68ms`, raw multiline `EditableText` `278.55ms`, Flark live-rendered plain corpus `914.16ms` | The source gap is not a small Flark wrapper overhead; current live-rendered plain path is worse | Prototype virtualized source/block viewport instead of tuning raw `EditableText` flags |
| Native parse + decode | 1MB parse+decode: `517ms` median in the latest full run after mapper optimization | Improved by ~2.3x from the previous `1.17s`, but still the largest raw Flark-specific perf number | Continue with payload decode and result mapping before parser architecture |

**Current decision:** the broad live-rendered perf architecture work has hit
diminishing returns for the measured 40-80 block editing path. Large-document
build/apply are in good shape, but 1MB viewport pump is still the peer-facing
editor gap. The newest probes ruled out a cheap source-widget flag fix: Flark
source is already slightly faster than raw multiline `EditableText`, while the
current live-rendered plain-paragraph path is much slower. The largest raw
Flark-specific number is still native Markdown parse/decode and result mapping.

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
| 1KB | `3us` / `3us` | `796us` / `3.48ms` | trivial; parse p95 includes cold/noisy samples |
| 100KB | `160us` / `2.56ms` | `95.81ms` / `140.52ms` | synchronous apply is healthy |
| 1MB | `2.61ms` / `12.83ms` | `517.17ms` / `517.17ms` | async parse/decode is the largest raw number |

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

### Large-Document Editor Sweep

Harnesses:

```bash
flutter test test/v2/performance/flark_large_document_editor_benchmark_test.dart \
  --tags benchmark \
  --reporter compact
cd benchmark/peer && flutter test test/quill_large_document_benchmark_test.dart
cd ../peer_supereditor && flutter test test/super_editor_large_document_benchmark_test.dart
```

Latest local snapshot, debug test-VM in a 600px viewport:

| Case | Flark source editor median / p95 | super_editor median / p95 | flutter_quill median / p95 | Read |
| --- | --- | --- | --- | --- |
| 100KB model/controller build | `539us` / `2.16ms` | `1.24ms` / `2.01ms` | `35.08ms` / `46.99ms` | Flark leads this surface |
| 100KB edit apply | `175us` / `487us` | `156us` / `634us` | `3.52ms` / `4.10ms` | Flark and super_editor are both sub-ms |
| 100KB edit pump | `32.78ms` / `48.76ms` | `17.65ms` / `28.88ms` | `23.68ms` / `37.02ms` | Flark trails peers but stays in the same rough band |
| 1MB model/controller build | `1.14ms` / `1.76ms` | `3.20ms` / `4.72ms` | `1.83s` / `3.23s` | Flark leads this surface |
| 1MB edit apply | `1.95ms` / `7.02ms` | `755us` / `3.20ms` | `962.66ms` / `1.19s` | Competitive with the direct block peer, far ahead of Quill |
| 1MB edit pump | `259.68ms` / `384.84ms` | `98.36ms` / `136.79ms` | `153.54ms` / `205.48ms` | Flark trails both peers; this is the editor-side gap |

Interpretation: for peer-comparable editor interaction at 1MB, Flark is strong
on model build and edit apply, but not yet peer-leading on viewport pump. The
larger Flark-specific number remains Markdown parse/decode, which these peers
do not directly measure.

Additional local probes in the same harness:

| Probe | 100KB median / p95 | 1MB median / p95 | Read |
| --- | --- | --- | --- |
| Flark source editor pump | `32.78ms` / `48.76ms` | `259.68ms` / `384.84ms` | current peer-facing source result |
| Raw multiline `EditableText` pump | `28.92ms` / `51.33ms` | `278.55ms` / `366.39ms` | Flark source is not slower than raw Flutter text layout |
| Flark live-rendered plain-corpus pump | `59.53ms` / `119.60ms` | `914.16ms` / `1.17s` | current rendered plain path is not the large-doc answer |

Two discarded configuration probes are worth remembering: forcing `maxLines: 30`
or `expands: true` on the source editor did not improve the 1MB source pump.
That points away from another `EditableText` flag tweak and toward a virtualized
source/block viewport if peer-leading 1MB pump remains a goal.

## What To Focus On Next

1. Keep live-rendered rebuild/profile gates in CI-adjacent validation and move
   regular product work forward.
2. Use the large-document peer sweep as the editor-side calibration gate:
   build/apply are solid, while 1MB viewport pump is the peer-facing gap.
3. If peer-competitive 1MB editing remains the priority, prototype a virtualized
   source or block viewport. Current evidence says raw full-document
   `EditableText` layout is the cost class to avoid.
4. If large-file open/parse latency is the priority, investigate native
   parse/decode payload cost before parser architecture: result mapping and JSON
   decode are larger than native Comrak parse.

## Large-Document Pitch

The current large-document bottleneck is not synchronous typing. At 1MB,
`keystroke_apply` is `2.61ms` median / `12.83ms` p95, while native parse+decode
is `517.17ms` median in the latest full run after optimizing
`FlarkUtf8Utf16Mapper` to avoid per-scalar `utf8.encode` calls. Recent
post-mapper runs have ranged from roughly `498-712ms`; treat the durable signal
as a clear large-doc parse/decode win, not a tight SLA.

The peer-calibrated editor surface agrees with that read. At 1MB, current Flark
builds a source controller in `1.14ms`, applies a source edit in `1.95ms`, and
pumps the edited 600px viewport in `259.68ms`. super_editor is faster on
viewport pump at `98.36ms`, and flutter_quill is faster on viewport pump at
`153.54ms` but much slower on model build/apply. That means build/apply are
competitive, while viewport pump still has a clear peer-facing opportunity.

The phase profile now prints:

```text
flark_benchmark native_parse_profile_1MB_1000080chars iterations=3
  total_median=522.80ms utf8_encode_median=2.71ms
  bridge_total_median=182.79ms native_parse_median=65.42ms
  payload_decode_median=116.35ms result_mapping_median=332.42ms
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

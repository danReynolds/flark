# Benchmarks

This is the benchmark scoreboard for Flark. Keep it current whenever perf work
lands: every optimization should leave behind a number, a comparison point, and
a conclusion about where to focus next.

## Current Conclusions

| Area | Current signal | Conclusion | Next focus |
| --- | --- | --- | --- |
| Live-rendered rebuild fanout | `builds_per_edit=1.0` through 10/20/40/80 blocks in the focused debug rebuild benchmark | Fixed: unchanged blocks no longer rebuild just because source/display offsets shift | Keep the benchmark as a regression gate |
| Live-rendered profile frame time | 40/80 blocks x end/start edits: `build_median=1.16-1.53ms`, `build_p95<=2.12ms` | Safely under the 16.7ms frame budget with large headroom | Re-run the profile gate after major widget changes |
| Peer comparison | Flark debug medians are flat at `9.15-10.13ms`; current super_editor is `7.20-8.46ms`; current flutter_quill is `6.82-9.78ms` | Peer-competitive on scaling and profile latency, not peer-leading on debug constant factor | Only chase live-block constant cost if peer-leading debug numbers become a product goal |
| Large-document source transaction | 1MB apply: `4.01ms` median / `18.89ms` p95 | Median is healthy; p95 is noisy and still much smaller than viewport/parse costs | Piece table / rope only if 1MB source apply consistently crosses frame budget |
| Large-document editor peer calibration | 1MB source editor: build `1.30ms`, apply `1.59ms`, pump `54.13ms` median; super_editor `4.41ms` / `1.16ms` / `128.63ms`; flutter_quill `3.77s` / `1.11s` / `211.38ms` | Peer-leading build and viewport pump; apply remains peer-competitive | Keep the virtualized source viewport as the large-doc regression gate |
| Large-document viewport probes | 1MB pump: virtualized Flark source `54.13ms`, raw multiline `EditableText` `317.32ms`, Flark live-rendered plain corpus `428.22ms` | Virtualized source closes the full-document text-layout gap | Do not tune raw `EditableText` flags unless this gate regresses |
| Native parse + decode | 1MB full-run parse+decode: `848ms` min / `952ms` median; retained payload spike: payload `8.14MB` -> `7.69MB`; latest profile decode `243ms`, mapping `406ms` | Payload omission is a real cleanup, but decode timing and total parse/decode remain noisy; result mapping is still the clearest Flark-side phase | Split result mapping before parser architecture or a larger wire-format rewrite |

**Current decision:** the broad live-rendered perf architecture work has hit
diminishing returns for the measured 40-80 block editing path. Large-document
source editing is now peer-competitive on the measured editor surfaces:
virtualized source lines avoid the full-document `EditableText` layout cost and
bring 1MB viewport pump below both peer medians. The largest raw Flark-specific
number is still native Markdown parse/decode and result mapping.

**Payload/decode spike:** native now omits empty per-node payload objects and
the Dart codec maps decoded lists without intermediate list copies. This
deterministically reduced the 1MB native payload from `8,138,528` bytes to
`7,689,403` bytes. Decode timing remains noisy across full-suite and focused
runs, so treat byte reduction as the durable win. A compact marker-range wire
shape was tested and rejected: although it shrank the payload to `5,952,283`
bytes, repeated profile runs were slower, so the retained protocol keeps
object-form ranges.

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
| 10 blocks | `9.64ms` / `21.29ms` | `1.0` block/edit | super_editor `7.20ms` / `12.31ms`; flutter_quill `6.82ms` / `9.27ms` | ~21ms, all-block fanout |
| 20 blocks | `9.15ms` / `22.44ms` | `1.0` block/edit | super_editor `8.46ms` / `15.08ms`; flutter_quill `8.59ms` / `30.48ms` | ~27ms, all-block fanout |
| 40 blocks | `9.76ms` / `26.38ms` | `1.0` block/edit | super_editor `7.35ms` / `16.73ms`; flutter_quill `9.78ms` / `28.78ms` | ~44ms, all-block fanout |
| 80 blocks | `10.13ms` / `13.84ms` | `1.0` block/edit | super_editor `7.83ms` / `12.06ms`; flutter_quill `8.53ms` / `16.56ms` | ~72ms, all-block fanout |
| 80 blocks, 600px viewport | `8.99ms` | `1.0` block/edit | not measured | Use this to decide whether virtualization is worth it |
| Source mode, 40 blocks | `1.50ms` / `2.00ms` | one editable | lower bound for source-string editing | already flat |

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
| 1KB | `3us` / `4us` | `1.96ms` / `3.38ms` | trivial; parse p95 includes cold/noisy samples |
| 100KB | `164us` / `2.75ms` | `118.15ms` / `183.66ms` | synchronous apply is healthy |
| 1MB | `4.01ms` / `18.89ms` | `951.51ms` / `951.51ms` in the latest full validation run | async parse/decode is noisy and remains the largest raw number |

- **Keystroke apply** is the synchronous core hot path
  (`FlarkEditorState.applyTransaction`: text-buffer rebuild, line reindex,
  selection map, history inverse). It scales cleanly through 1MB in the current
  lane, with p95 still below a 60fps frame budget.
- **Native parse + decode** is `FlarkNativeComrakParseBackend.parse` (Comrak
  parse, JSON payload, Dart decode, result synthesis). It is no longer
  pathological, but the 1MB async cost is the clearest remaining performance
  area if huge documents become important. The latest payload spike reduced
  the native JSON payload size, but did not change this into an editor-side
  bottleneck.

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
| 100KB model/controller build | `1.40ms` / `3.56ms` | `1.58ms` / `7.05ms` | `41.84ms` / `58.90ms` | Flark and super_editor are both fast; Flark leads Quill |
| 100KB edit apply | `173us` / `1.95ms` | `143us` / `356us` | `5.72ms` / `15.23ms` | Flark and super_editor are both sub-ms medians |
| 100KB edit pump | `31.39ms` / `48.56ms` | `26.69ms` / `44.82ms` | `30.23ms` / `81.44ms` | 100KB stays in the peer band below the virtualization threshold |
| 1MB model/controller build | `1.30ms` / `1.74ms` | `4.41ms` / `11.51ms` | `3.77s` / `5.07s` | Flark leads this surface |
| 1MB edit apply | `1.59ms` / `5.47ms` | `1.16ms` / `4.07ms` | `1.11s` / `2.28s` | Competitive with the direct block peer, far ahead of Quill |
| 1MB edit pump | `54.13ms` / `98.76ms` | `128.63ms` / `134.13ms` | `211.38ms` / `450.18ms` | Flark now leads this peer-facing viewport surface |

Interpretation: for peer-comparable editor interaction at 1MB, Flark is now
solid. The virtualized source path moves viewport pump from worse-than-peer to
peer-leading in this debug test-VM lane. The larger Flark-specific number
remains Markdown parse/decode, which these peers do not directly measure.

Additional local probes in the same harness:

| Probe | 100KB median / p95 | 1MB median / p95 | Read |
| --- | --- | --- | --- |
| Flark source editor pump | `31.39ms` / `48.56ms` | `54.13ms` / `98.76ms` | large docs use the virtualized source viewport |
| Raw multiline `EditableText` pump | `26.96ms` / `55.95ms` | `317.32ms` / `374.04ms` | full-document text layout is the avoided cost class |
| Flark live-rendered plain-corpus pump | `51.79ms` / `78.48ms` | `428.22ms` / `504.03ms` | current rendered plain path is not the large-doc answer |

Two discarded configuration probes are worth remembering: forcing `maxLines: 30`
or `expands: true` on the source editor did not improve the 1MB source pump.
The win came from changing the architecture to visible line editables, not from
tuning raw `EditableText`.

## What To Focus On Next

1. Keep live-rendered rebuild/profile gates in CI-adjacent validation and move
   regular product work forward.
2. Use the large-document peer sweep as the editor-side calibration gate:
   build/apply are solid, and the virtualized source viewport has closed the
   1MB pump gap.
3. If large-doc editor work continues, harden virtualized source ergonomics
   around keyboard traversal, multi-line selections, and IME grouping rather
   than revisiting full-document text layout.
4. If large-file open/parse latency is the priority, split native result
   mapping into subphase timings before parser architecture: mapping remains
   larger than JSON decode and native Comrak parse.

## Large-Document Pitch

The current large-document bottleneck is not synchronous typing. At 1MB,
`keystroke_apply` is `4.01ms` median / `18.89ms` p95, while native parse+decode
is `951.51ms` median in the latest full validation run after the payload/decode
spike. Recent post-mapper runs remain noisy; treat the durable signal as
large-doc parse/decode being in the subsecond-to-roughly-one-second lane, not as
a tight SLA.

The peer-calibrated editor surface agrees with that read. At 1MB, current Flark
builds a source controller in `1.30ms`, applies a source edit in `1.59ms`, and
pumps the edited 600px viewport in `54.13ms`. That is below super_editor's
`128.63ms` viewport pump and flutter_quill's `211.38ms`, while also staying far
ahead of Quill on model build/apply. That makes editor-side large-doc
performance peer-competitive in the measured lane.

The phase profile now prints:

```text
flark_benchmark native_parse_profile_1MB_1000080chars iterations=3
  total_median=811.40ms utf8_encode_median=4.64ms
  bridge_total_median=400.89ms native_parse_median=133.66ms
  payload_decode_median=242.68ms result_mapping_median=405.65ms
  input_bytes=1000080 payload_bytes=7689403
```

The retained payload/decode spike omits empty native node payloads and avoids
intermediate list copies in the Dart payload codec. Against the same focused
1MB profile lane, payload size moved from `8,138,528` bytes to `7,689,403`
bytes. Payload decode timing is noisy: focused samples improved, while the
latest full lane printed `242.68ms`. A compact range-array payload was also
tested; it reduced bytes further to `5,952,283`, but repeated profile runs were
slower, so that wire shape was rejected.

The profiled total is noisier than the payload byte count, but the ordering is
clear enough: Flark-side result mapping is now larger than JSON payload decode,
and native Comrak parse itself is smaller than both. The next useful work is:

1. Split `_mapNativeResult` into marker/list/code-fence/inline/replacement
   subphases so the remaining `result_mapping` cost is attributable.
2. Optimize the largest mapped subphase before considering another wire-format
   change.
3. Only revisit compact or binary payloads if subphase instrumentation proves
   JSON decode is again larger than result mapping.
4. Leave `FlarkTextBuffer`/piece-table work behind this unless a new benchmark
   shows 1MB synchronous apply crossing frame budget again.

## Budget Policy

Budgets are generous regression trackers, not tight SLAs. They exist to catch
order-of-magnitude regressions and shape regressions. If a budget changes, the
commit should say which machine/harness produced the new baseline and why the
new threshold still catches the failure mode we care about.

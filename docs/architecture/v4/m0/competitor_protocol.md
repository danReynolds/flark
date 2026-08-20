# M0 competitor evidence and Mac protocol

**Status:** historical seed inventoried; profile protocol and fail-closed
coordinator implemented but the full 234-process run is not yet executed.
**Machine-readable authority:**
[`benchmark/v4/competitor_baseline_v1.json`](../../../../benchmark/v4/competitor_baseline_v1.json).
**Inventory commit:** `47692297661489bcbc2a2af4574a6a422cf68ef7`.

This document separates two kinds of evidence:

1. the repository's June 2026 Quill and SuperEditor debug-test results, retained
   as useful historical seed evidence; and
2. the Mac-first profile-mode protocol that must be executed before resolving
   a competitor-derived envelope.

The historical numbers are not profile product latency, do not resolve
`competitor-boundary` in `benchmark/v4/workloads_v1.json`, and cannot support a
public peer-performance claim.

The selected comparison boundary is deliberately narrower than the global
Markdown-app market: Flutter Quill and SuperEditor are the leading relevant
**embeddable Flutter editor-SDK cohort** available to this product study. They
exercise two materially different established Flutter editing architectures
(delta-backed rich text and document/node-backed editing), use Flutter's real
text-input and rendering stack on the same Mac, and can be embedded without an
unrelated app shell, database, or sync engine contaminating the measurement.
This scope does not claim that two packages exhaust or rank the worldwide
editor market, and it does not compare Markdown semantic correctness. The
historical README also records an AppFlowy Editor attempt that did not compile
on Flutter 3.41.9; it has no local timing and is not silently counted as a
measured peer.

“Leading relevant” is not subjective shorthand here. At
`2026-08-09T02:06:28Z`, M0 froze the eligible native-Flutter editor-SDK
candidate denominator and selected the top two by cumulative pub.dev likes, a
predeclared public adoption signal. Thirty-day downloads and pub points are
recorded context, not alternate post-hoc selectors:

| Candidate | Likes | 30-day downloads | Pub points | Selected |
| --- | ---: | ---: | ---: | --- |
| Flutter Quill | 2,142 | 230,102 | 140 | yes |
| SuperEditor | 782 | 7,776 | 60 | yes |
| AppFlowy Editor | 518 | 10,273 | 60 | no |
| Fleather | 191 | 9,080 | 160 | no |

The machine-readable snapshot records each official pub.dev package page and
score API endpoint, the inclusion/exclusion criteria, versions, and the
AppFlowy compile caveat. This proves leadership only within that frozen
criterion and SDK denominator; it is not a global product or performance
leadership claim.

## 1. Existing local peer artifacts

The current cohort has two isolated Flutter packages:

- Flutter Quill: [`benchmark/peer/`](../../../../benchmark/peer/), with the
  block-count harness at
  [`test/quill_benchmark_test.dart`](../../../../benchmark/peer/test/quill_benchmark_test.dart)
  and the large-document harness at
  [`test/quill_large_document_benchmark_test.dart`](../../../../benchmark/peer/test/quill_large_document_benchmark_test.dart).
  Its pubspec declares `flutter_quill: ^11.5.0`; no lockfile is tracked.
- SuperEditor: [`benchmark/peer_supereditor/`](../../../../benchmark/peer_supereditor/),
  with corresponding
  [`super_editor_benchmark_test.dart`](../../../../benchmark/peer_supereditor/test/super_editor_benchmark_test.dart)
  and
  [`super_editor_large_document_benchmark_test.dart`](../../../../benchmark/peer_supereditor/test/super_editor_large_document_benchmark_test.dart).
  Its pubspec pins git revision
  `22853bcc89def2b234017202a3f3cac36d3c088f`; no lockfile is tracked.

The methodology and toolchain caveats are recorded in
[`benchmark/peer/README.md`](../../../../benchmark/peer/README.md): a 600 by 600
logical-pixel viewport, direct one-character controller edits near the start,
five warmups and 40 timed `tester.pump()` samples for block-count cases, running
in the debug test VM. The documented 2026-06-05 run used Flutter 3.41.9 and Dart
3.11.5. SuperEditor was reported as `0.3.0-dev.51` and required deleting one
obsolete `updateStyle` override in the pub cache, but no patch artifact or
patched-tree hash survives.

The local result table is
[`legacy/docs/v2_v3/public/benchmarks.md`, lines 197-206](../../../../legacy/docs/v2_v3/public/benchmarks.md). The JSON inventory
pins SHA-256 values for that table, both pubspecs, both `.gitignore` files, and
all four harnesses; the contract test rejects unnoticed drift.

## 2. Historical seed values

All durations below are the preserved debug-test medians and p95 values. They
are transcribed in microseconds in the JSON.

### Small block-count edit pump

These values come from
[`legacy/docs/v2_v3/public/benchmarks.md`, lines 58-63](../../../../legacy/docs/v2_v3/public/benchmarks.md).

| Peer | 10 blocks | 20 blocks | 40 blocks | 80 blocks |
| --- | --- | --- | --- | --- |
| Flutter Quill | 6.82 / 9.27 ms | 8.59 / 30.48 ms | 9.78 / 28.78 ms | 8.53 / 16.56 ms |
| SuperEditor | 7.20 / 12.31 ms | 8.46 / 15.08 ms | 7.35 / 16.73 ms | 7.83 / 12.06 ms |

Each cell is median / p95 from 40 samples after five warmups. This is evidence
that both historical harnesses had roughly flat block-count scaling through 80
blocks. It is not a frame-time or input-to-paint claim.

### Large-document debug harness

| Peer | Historical label | Operation | Median | p95 |
| --- | --- | --- | ---: | ---: |
| Flutter Quill | 100KB | model build | 41.84 ms | 58.90 ms |
| Flutter Quill | 100KB | edit apply | 5.72 ms | 15.23 ms |
| Flutter Quill | 100KB | post-edit pump | 30.23 ms | 81.44 ms |
| Flutter Quill | 1MB | model build | 3.77 s | 5.07 s |
| Flutter Quill | 1MB | edit apply | 1.11 s | 2.28 s |
| Flutter Quill | 1MB | post-edit pump | 211.38 ms | 450.18 ms |
| SuperEditor | 100KB | model build | 1.58 ms | 7.05 ms |
| SuperEditor | 100KB | edit apply | 143 us | 356 us |
| SuperEditor | 100KB | post-edit pump | 26.69 ms | 44.82 ms |
| SuperEditor | 1MB | model build | 4.41 ms | 11.51 ms |
| SuperEditor | 1MB | edit apply | 1.16 ms | 4.07 ms |
| SuperEditor | 1MB | post-edit pump | 128.63 ms | 134.13 ms |

The labels are not byte-exact sizes. Both harnesses generate ASCII paragraphs
until a Dart `String.length` reaches at least 100,000 or 1,000,000 characters.
The result names printed the actual character count, but the preserved table
does not contain those names or raw logs. Therefore the JSON records
`actualCharacters: null` and `exactByteTier: false`. These measurements cannot
be silently relabeled 100 KiB or 1 MiB.

Further limitations are material:

- the historical Mac model, OS, refresh rate, commit, dependency lockfiles,
  raw samples, min/max, memory, and timeline traces were not preserved;
- only median and p95 survive, not the M0-required p50/p90/p99/max family;
- direct controller mutation and `tester.pump()` omit platform text input and
  a real profile application's raster completion;
- plain-text construction does not establish Markdown-source fidelity or
  semantic equivalence;
- Quill's caret dependency and SuperEditor's unrecorded cache patch prevent an
  exact dependency reconstruction.

The honest use of these values is hypothesis formation and regression
orientation. A current comparison requires the protocol below.

## 3. Profile-mode protocol

### Cohort and runners

Keep Flutter Quill and SuperEditor as the selected leading relevant embeddable
Flutter editor-SDK cohort. This cohort may resolve only the correspondingly
scoped competitor-derived Flutter SDK envelope; it is not evidence about every
desktop Markdown product. Do not modify their editor implementations or add
peer-specific virtualization. Harness plumbing may adapt the shared fixture
into each editor's documented default model, but must preserve and export the
resulting source for fidelity comparison.

The required profile runner entrypoints are:

```text
benchmark/peer/lib/competitor_profile_harness.dart
benchmark/peer_supereditor/lib/competitor_profile_harness.dart
```

Both runners now exist. Directly launching one runner is useful for peer-local
diagnosis, but it cannot produce a cohort receipt. The cross-peer authority is
`benchmark/peer_suite/tool/run_peer_suite.dart`, which freezes 234 fresh
profile processes in three interleaved groups, requires a five-minute idle
interval before each group, and refuses an executed run without explicit
exclusive-machine attestation. Its full command is:

```sh
dart run benchmark/peer_suite/tool/run_peer_suite.dart \
  --execute \
  --exclusive-machine-attested \
  --flutter=/absolute/path/to/flutter
```

The peer-local diagnostic commands are:

```sh
cd benchmark/peer
flutter pub get
flutter run --profile -d macos \
  -t lib/competitor_profile_harness.dart \
  --dart-define=COMPETITOR_PROTOCOL_ID=m0-mac-competitor-profile-v1

cd benchmark/peer_supereditor
flutter pub get
flutter run --profile -d macos \
  -t lib/competitor_profile_harness.dart \
  --dart-define=COMPETITOR_PROTOCOL_ID=m0-mac-competitor-profile-v1
```

Preserve each resolved `pubspec.lock`, `flutter pub deps --style=compact`
output, package source revision, and compatibility patch as hashed receipt
artifacts. Do not patch an anonymous pub cache without retaining the exact diff
and patched-tree hash.

### Fixture and size tiers

Use the `ordinary-prose` recipe from
[`benchmark/v4/workloads_v1.json`](../../../../benchmark/v4/workloads_v1.json)
and its `flark-v4-deterministic-markdown-v1` generator. Generate exact ASCII
UTF-8 byte lengths with no normalization:

| Tier | Exact bytes |
| --- | ---: |
| 1 MiB | 1,048,576 |
| 5 MiB | 5,242,880 |
| 10 MiB | 10,485,760 |
| 20 MiB | 20,971,520 |

Every result records target bytes, actual bytes, fixture SHA-256, line-ending
policy, and the exported final source SHA-256. If an editor necessarily adds,
removes, or normalizes source, record a fidelity failure and the exact diff;
do not change the fixture to hide the mismatch. The comparison covers
plain-source fidelity and editor interaction, not peer Markdown semantics.

### Workloads

Run every workload for every peer and size:

1. **Cold open:** 30 fresh profile-process samples, no in-process warmup. Start
   at process launch/fixture selection and stop at the first rasterized,
   interactive viewport containing the expected text. Record process-start to
   paint and document-load-start to paint separately. Timeout: 60 seconds.
2. **Sustained typing:** three fresh-process runs. After 20 unmeasured edits,
   deliver 200 characters at 10 Hz through the focused Flutter platform
   text-input channel, cycling
   `abcdefghijklmnopqrstuvwxyz0123456789`. Direct controller mutation is
   forbidden. Measure accepted-input to containing rasterized frame and input
   backlog.
3. **Local insert/delete:** three fresh-process runs at start, middle, and end.
   After ten warmup pairs per location, measure 100 `insert-x` / delete-that-x
   pairs per location through the same user-facing input path.
4. **32 KiB paste:** three fresh-process runs at start, middle, and end. After
   two warmups, collect 20 samples per location with an exact 32,768-byte ASCII
   payload. Measure accepted paste to complete exact paint, then select that
   exact pasted range and reset it through platform Backspace outside the
   measured interval. The reset must finish rasterizing and restore the exact
   base source before the next paste begins. Final export must equal the base
   fixture. Timeout: 60 seconds.

For distributions, retain every raw sample and report p50, p90, p99, and max.
Also record build and raster spans, longest synchronous span, missed frames,
input backlog, peak and retained RSS, crashes, hangs, timeouts, input loss, and
fidelity outcomes.

### Run control and provenance

Use one named physical Mac, plugged into power with Low Power Mode off. Record
machine model, CPU, cores, memory, macOS, power and thermal state, Flutter and
Dart versions/revisions, Xcode, profile artifact hash, display refresh rate,
device pixel ratio, and the exact 600 by 600 logical-pixel editor viewport.

No other build, test, benchmark, screen recording, indexing, or agent sweep may
run concurrently. Idle five minutes before each run group. Rotate competitor
and size order across three runs with a recorded Latin square; do not run all
of one peer first. Record exact argv, cwd, environment, UTC timestamps,
warmups, counts, cadence, timeouts, failures, and hashes for the runner,
fixture, lockfile, stdout, timeline, final export, and profile application.

## 4. Completion and target policy

A tier is a comparable competitor completion only when the editor:

- accepts the fixture or explicitly reports any normalization;
- paints an exact interactive viewport before the 60-second liveness timeout;
- makes every accepted typed, local-edit, and paste input visible and
  exportable without loss;
- drains its input backlog before timeout; and
- does not crash, hang, or exceed available memory.

No peer latency threshold determines completion. Latency is recorded as an
observation. The largest completed tier becomes the separately named
**embeddable-Flutter editor-SDK competitor boundary**, with fidelity
differences attached. No result from this cohort may be relabeled as a global
market-leader boundary.

Most importantly, **competitor behavior does not define Flark's 10 MiB
target**. Ten MiB is already a fixed Flark editor-scale tier in the M0 workload
matrix:

- a peer failure below 10 MiB cannot lower Flark's tier;
- a peer's latency cannot become Flark's performance threshold;
- a peer success above 10 MiB may create a separately named derived stretch
  tier, but it does not relabel or replace the fixed 10 MiB target.

Until the profile receipts exist, the machine-readable baseline remains
`claimEligible: false` and `mayResolveCompetitorDerivedSizeTiers: false`.

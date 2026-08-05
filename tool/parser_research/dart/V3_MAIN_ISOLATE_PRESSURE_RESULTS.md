# V3 Dart/main-isolate pressure results

Status: disposable architecture evidence, not a launch benchmark  
Date: 2026-07-15  
Host: MacBookPro18,1, Apple M1 Pro, 16 GiB, macOS, Dart 3.12.2,
Flutter 3.44.4

The executable probe is
[`v3_main_isolate_pressure_probe.dart`](v3_main_isolate_pressure_probe.dart).
The opt-in Flutter receipt is
[`flark_v3_text_editing_value_cost_probe_test.dart`](../../../test/prototype/flark_v3_text_editing_value_cost_probe_test.dart).
Both print JSONL receipts. Ordinary test runs skip the expensive Flutter
receipt.

These are host-VM falsification results. They are not physical-device frame,
paint, IME, web-worker, or RSS proofs. Short bulk-paste samples are identified
below and must not be read as statistically strong p99 measurements.

## Reproduction

```sh
DART=/Users/dan/Coding/flutter_arm64/bin/dart
FLUTTER=/Users/dan/Coding/flutter_arm64/bin/flutter

$DART compile exe \
  tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
  -o /tmp/flark_v3_pressure_probe

/tmp/flark_v3_pressure_probe source --size-mib=1
/tmp/flark_v3_pressure_probe source --size-mib=10
/tmp/flark_v3_pressure_probe source --size-mib=100

/tmp/flark_v3_pressure_probe string --size-mib=1
/tmp/flark_v3_pressure_probe string --size-mib=10
/tmp/flark_v3_pressure_probe string --size-mib=100

/tmp/flark_v3_pressure_probe paste --paste-kib=1 --base-mib=16
/tmp/flark_v3_pressure_probe paste --paste-kib=1024 --base-mib=16
/tmp/flark_v3_pressure_probe paste --paste-kib=10240 --base-mib=16 --iterations=1

/tmp/flark_v3_pressure_probe transfer --payload-kib=1
/tmp/flark_v3_pressure_probe transfer --payload-kib=1024
/tmp/flark_v3_pressure_probe transfer --payload-kib=10240 --iterations=5

/tmp/flark_v3_pressure_probe worker-source --size-mib=1
/tmp/flark_v3_pressure_probe worker-source --size-mib=10
/tmp/flark_v3_pressure_probe worker-source --size-mib=100

$FLUTTER test \
  --dart-define=FLARK_RUN_TEXT_COST_PROBE=true \
  --dart-define=FLARK_TEXT_COST_SIZE_MIB=10 \
  test/prototype/flark_v3_text_editing_value_cost_probe_test.dart \
  --reporter expanded
```

The `source` lane was also run under `dart run` to retain JIT evidence. A
representative 10 MiB JIT run used:

```sh
$DART run tool/parser_research/dart/v3_main_isolate_pressure_probe.dart \
  source --size-mib=10 --iterations=300 --typing-iterations=2048
```

## Receipts

### Fully indexed persistent Dart source

The production v3 source implementation was measured, not the older disposable
piece-table model.

| ASCII source | Eager construction | Advancing typing p50 | p99 | Observed max | Cold random p50 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 MiB | 59-62 ms | 25-26 us | 76-84 us | 0.5-0.7 ms | 107 us |
| 10 MiB | 0.49 s | 25 us | 61 us | 0.32 ms | 89 us |
| 100 MiB | 5.4-8.3 s | 26-32 us | 60-883 us | 5.8-57.6 ms | 90-100 us |

The 100 MiB long tails varied with allocation pressure. One 8,192-edit run
observed roughly 55 ms maxima during typing and Backspace; a later 4,096-edit
run observed a 5.8 ms typing maximum and 0.26 ms Backspace maximum. This does
not implicate document-length work in the edit algorithm, but it does falsify a
host-only claim that a main-isolate Dart heap already has a proven hard 4 ms
tail.

Repeated replacement of one already-isolated character measured as little as
1-2 us and is not representative. Advancing-caret and cold-random lanes were
added specifically to avoid that benchmark trap.

For a stable 100 MiB AOT run, 4, 16, and 64 scattered operations in one atomic
transaction measured approximately:

| Operations | p50 | p99 | max |
| ---: | ---: | ---: | ---: |
| 4 | 0.20 ms | 0.25 ms | 0.33 ms |
| 16 | 0.79 ms | 1.06 ms | 1.12 ms |
| 64 | 2.66 ms | 3.67 ms | 4.44 ms |

Allocation-heavy 1 MiB repetitions produced worse tails, including about
5.4 ms p99 for a 16-operation lane and a 9.8 ms maximum for a 64-operation
lane. The loop intentionally applies edits faster than a human can type, so it
is a GC pressure test rather than a frame distribution.

ASCII UTF-16 to UTF-8 and reverse mapping was about 7-9 us p50 per round trip
in AOT. A deliberately multibyte 4,096-code-unit leaf was about 9-10 us p50 and
17-33 us p99 per round trip. Tree depth from 1 to 100 MiB had little effect.
Batch samples occasionally inherited GC pauses from earlier allocation-heavy
lanes. The 10 MiB JIT receipt was materially slower (about 37 us p50 per ASCII
round trip), while advancing typing was still about 16 us p50 and 59 us p99.

### Flutter whole-string cost is separate from wrapper cost

The local Flutter SDK's `TextEditingValue.replaced` and delta insertion paths
call `String.replaceRange`. The opt-in Flutter test measured a plain
`TextEditingValue.copyWith` retaining the same `String` separately from text
mutation:

| Full value | Wrapper-only copy | Whole-string replace p50 | Flutter replace/delta p50 | Observed tail |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB | 55 ns | about 1 ms | about 1 ms | 5-12 ms |
| 10 MiB | 82 ns | 11.4 ms | 10.5-11.3 ms | 20-34 ms |
| 100 MiB | 264 ns | 192 ms | 166-370 ms | 0.42-0.73 s |

The standalone AOT lane agreed on the scaling: approximately 1.4-2.9 ms p50
at 1 MiB, 11.8-13.6 ms at 10 MiB, and 117-123 ms at 100 MiB. A full-document
`TextEditingValue` is therefore incompatible with the large-document target.
The bounded active input island is mandatory, not an optional optimization.

### Payload-size knee in the current synchronous source path

The current implementation validates, owns bounded chunks, UTF-8 encodes,
hashes, builds a tree, and retains parser bytes synchronously. AOT receipts on
the host were:

| ASCII payload | Insert p99 | Replace p99 | Notes |
| ---: | ---: | ---: | --- |
| 1 KiB | below 0.3 ms | below 0.3 ms | clean single-shot 58/168 us |
| 4 KiB | 0.31 ms | 0.46 ms | 50 samples |
| 8 KiB | 0.48 ms | 0.48 ms | 50 samples |
| 16 KiB | 1.20 ms | 1.64 ms | 30 samples |
| 32 KiB | 1.78 ms | 1.62 ms | 30 samples |
| 64 KiB | 3.23 ms | 3.34 ms | 30 samples |
| 128 KiB | 6.85 ms | 6.14 ms | 20 samples |
| 1 MiB | 46-96 ms typical | 46-96 ms typical | maxima up to 0.23-0.30 s under repetition |
| 10 MiB | 0.61-1.13 s single-shot | 0.53-1.27 s single-shot | repeated-GC maxima reached several seconds |

No-op detection is also payload-linear. An identical 1 MiB replacement took
about 4.5-5.1 ms while comparing 1,048,576 UTF-16 units; an identical 10 MiB
replacement took about 50 ms even though it avoided encoding and tree changes.

The result holds at least two payload-scale representations after a changed
edit: owned source chunks and the UTF-8 parser batch, in addition to the
caller's input `String`. Temporary chunk/code-unit lists and `BytesBuilder`
state add more pressure. The current source-work receipt explicitly does not
count branch-node allocation, and RSS was too GC-sensitive to treat as exact
allocation accounting.

### Isolate and transport boundary

After port warm-up, immutable `String` messages were effectively constant-time
on this native VM. Sending a 10 MiB string took about 0.5 us on the main
isolate; encoding it in the worker took about 31 ms end to end. In contrast:

| Payload | Direct `Uint8List` send call p50 | `TransferableTypedData.fromList` p50 | TTD send p50 |
| ---: | ---: | ---: | ---: |
| 1 MiB | 0.22 ms | 0.17 ms | 0.004 ms |
| 10 MiB | 1.81 ms | 1.86 ms | 0.13 ms |

`TransferableTypedData` avoids the send copy only after a transferable buffer
exists; constructing it from ordinary Dart bytes is still payload-linear. A
10 MiB TTD heartbeat run observed a 2.54 ms maximum one-millisecond tick gap.
Cold initialization and GC can make the copy higher, so this is not a hard
bound. Native immutable-string sharing must not be assumed for web structured
clone; the explicit Web Worker path needs its own receipt.

A worker-owned v3 source tree produced the following host receipts after a
port warm-up:

| Source | Worker eager build | Main send call | Worker local edit p50 | p99 | Main heartbeat during edit burst |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 MiB | 85 ms | 18 us | 6.8 us | 49 us | 1.38 ms max gap |
| 10 MiB | 0.50 s | 45 us | 5.6 us | 26 us | 1.20 ms max gap |
| 100 MiB | 5.1-6.6 s | 27-31 us | 107 us | 183-192 us | 1.5-1.7 ms max gap |

During the 100 MiB worker build, the main heartbeat maximum was 6.4 ms on one
repeat and 47.7 ms on another. Isolates remove synchronous parser work from the
UI event handler, but CPU, allocator, process memory, OS scheduling, and
measurement tails mean that “in an isolate” is not itself a jank proof.

## Architecture consequences

### A. Persistent main source with lazy bulk leaves

This preserves the strongest property of the current RFC: an exact active
source and source-coordinate selection are synchronously available to the UI.
IME composition, caret echo, local commands, undo grouping, bounded range
reads, and active-island handoff do not wait for a worker acknowledgment.
Ordinary fully indexed leaves already meet the locality hypothesis on this
host.

It requires a source model that the current implementation does not have:

- large ingest/paste is adopted as an immutable, source-exact bulk backing
  piece without eagerly chunking, hashing, and UTF-8 encoding it;
- the piece has UTF-16 extent and stable piece-relative anchors immediately,
  while UTF-8/newline/hash summaries may be explicitly pending;
- validation and summary construction run in bounded cooperative slices,
  prioritizing the active suffix and visible ranges;
- the parser worker receives the immutable string directly and performs UTF-8
  preparation there instead of receiving a main-isolate `Uint8List`;
- shared giant string backing is compacted after indexing or high-ratio
  deletion so a tiny survivor does not retain a giant paste forever;
- initial load is streamed or indexed viewport-first. Calling the current
  eager `fromString` on the UI isolate is prohibited at every measured size.

The hard issue is transaction truth. An arbitrary Dart string can contain an
unpaired surrogate, so a large bulk value cannot be declared a valid committed
Markdown source before validation unless the source contract changes. The
honest model needs a `bulkPending` source intent that becomes committed after
worker/cooperative validation, or a trusted platform/file decoder boundary.
Hash and UTF-8 mapping beyond a pending piece are unavailable until indexed.
Selection can remain exact in global UTF-16 plus piece-relative anchors, but
commands requiring global line/UTF-8 facts must await enrichment. This is
contained complexity, but it is real and must be prototyped before RFC 023 can
claim that all source commits are synchronously fully indexed.

### B. Worker canonical source with a bounded main cache

This removes the fully indexed Dart mirror and lets the Rust/source/parser core
own one materialized source. The main isolate retains only:

- the exact active input island and visible source/projection pages;
- global UTF-16 positions and stable anchors;
- a bounded authoritative write-ahead intent journal;
- revision-tagged caches and pending transaction IDs.

The active island can echo an IME edit immediately into the journal and local
slice while the worker validates and materializes it. The logical canonical
source is then the last worker snapshot plus the ordered journal, rather than
an unacknowledged optimistic UI string. Worker restart replays the journal.
Grammar must be preemptible so source acknowledgments are never queued behind a
long parse.

This model handles bulk ingest and memory more cleanly and avoids keeping both
a Dart tree and native rope. It also has a larger product/API cost:

- arbitrary range reads, large copy/export, and cold source access become
  asynchronous unless cached;
- cross-shard selection can keep source offsets synchronously, but copying a
  non-cached selection needs a worker response;
- commands needing non-visible neighboring source must prefetch or suspend;
- the compatibility synchronous whole-document `markdown` getter cannot stay
  cheap and current without recreating the duplicate full source;
- IME composition must never be rewritten by a late worker acknowledgment,
  and rejection/restart behavior needs explicit journal reconciliation;
- native-isolate and web-worker round-trip/device behavior becomes a launch
  dependency rather than an optimization.

### Recommendation

Do not abandon the main-source direction based on parser concerns: its normal
edit locality survived 100 MiB. Do reject the stronger assumption that the
main source can always be eagerly and synchronously fully indexed.

The next commitment spike should implement option A's lazy bulk piece and
exercise: 10 MiB open, 1/10 MiB paste, paste during composition, immediate
Backspace after paste, undo before enrichment completes, cross-piece selection,
large copy, invalid-surrogate rejection, worker restart, and deletion of all
but a few bytes. In parallel, a thin option-B slice should run those same
selection/IME/restart cases with a write-ahead journal and bounded cache. The
choice should be made on state-machine and failure complexity, not another
parser microbenchmark.

The current best prior is option A because it preserves synchronous IME and
selection semantics while changing only the bulk/bootstrap representation.
Pivot to option B if the pending-summary/validation state infects ordinary
source APIs or cannot meet memory retention limits. Neither the current eager
main tree nor a bare worker handoff is complete enough for launch.

Tentative host-informed launch defaults for the next spike are:

- at most **8 source operations** and **8 KiB total replacement payload** on
  the synchronous ordinary path;
- 9-16 operations may use that path only behind floor-device evidence and the
  same total byte cap; more than 16 is always bulk/cooperative;
- more than 8 KiB is routed before encoding/comparison to the bulk path. The
  main path may first check UTF-16 length, then compute exact UTF-8 bytes only
  for values already below the small bound;
- these are routing caps, not public document limits. They must be recalibrated
  downward or upward from physical 60/120 Hz floor-device p99/p999 results.

Eight KiB leaves headroom over the 0.48 ms host p99. Sixteen KiB already
reached 1.64 ms and is too close to the RFC's 2 ms UI budget before intent,
history, projection, layout, paint, a slower device, and GC are included.

## Preflight conclusion

Exact multidimensional preflight before every parser micro-operation is not
required for UI jank freedom once parsing is genuinely worker-owned and Dart
adoption is bounded. It also cannot predict GC, allocator, OS scheduling, or
device wall time exactly.

The simpler sufficient model is:

1. constant-time main-thread routing caps on operation count, replacement
   extent, message bytes, and adoption pages;
2. statically bounded ordinary loops (tree depth plus bounded leaf scans);
3. cooperative worker yield/cancellation points with charged bytes, nodes,
   transitions, and allocations;
4. hard worker memory, root, queue, document, and output caps;
5. periodic wall-clock deadline sampling and production p99/p999 telemetry;
6. fail-closed protocol validation before atomic adoption.

Multidimensional accounting remains valuable inside the worker for
supersession latency, denial-of-service resistance, memory safety, and
reproducible tests. It need not be a per-transition predictive admission proof
for the frame budget. If any urgent parser fallback runs on the main isolate,
it must obey the same small routing caps and resumable deadline; isolation may
not be silently bypassed.

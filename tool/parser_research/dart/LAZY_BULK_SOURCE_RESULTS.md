# Lazy bulk source spike

Status: disposable architecture evidence, not production code or a launch gate  
Date: 2026-07-15  
Host: MacBookPro18,1, Apple M1 Pro, 16 GiB, macOS, Dart 3.12.2

The spike is in
[`lazy_bulk_source_probe.dart`](lazy_bulk_source_probe.dart). The compiled-web
primitive probes are
[`lazy_bulk_web_string_probe.dart`](lazy_bulk_web_string_probe.dart),
[`lazy_bulk_wasm_runner.mjs`](lazy_bulk_wasm_runner.mjs), and the V8 worker
structured-clone proxy is
[`lazy_bulk_node_worker_probe.mjs`](lazy_bulk_node_worker_probe.mjs).

## Verdict

Proceed with an **Option-A-led hybrid**, not pure A or pure B:

- Keep an exact main-isolate logical source, stable UTF-16 positions, anchors,
  the active input island, and immediate edit/selection/undo behavior.
- Keep the existing small fully indexed path for ordinary transactions. The
  current host-informed route remains at most 8 operations and 8 KiB total
  replacement payload until device data recalibrates it.
- Represent a larger input as an immutable lazy backing piece without walking
  it. Such a transaction is a **provisional candidate**, not a committed source
  revision, until the worker validates it and returns exact byte/hash/line
  summaries. The last certified snapshot remains canonical for parser-result
  adoption.
- Send a separate bulk worker intent in UTF-16 coordinates. The worker derives
  UTF-8 coordinates, bytes, the fingerprint, and parser input. The current v3
  parser batch, which eagerly requires replacement bytes plus before/after
  hashes and UTF-8 lengths, cannot represent this state honestly.
- On web, stream bounded chunks or let the worker read an existing `Blob`/file
  backing directly. Do not send a 100 MiB JavaScript String in one `postMessage`.

This is a narrower change than making the worker the only canonical source.
The thin Option-B comparison kept a 4 KiB active cache and an intent journal;
local echo and pending undo worked, but cold range reads/copy became async and
a worker restart was impossible without a replayable base provider. It also
does not avoid the web transfer when a bulk paste or programmatic String starts
on the main thread.

The spike's immutable `List` piece table is only a semantic/timing harness.
Repeated edits are O(piece count). Production must put the same lazy backing
and readiness states in the existing persistent sum tree so edits, range
location, and anchor resolution remain logarithmic.

## What was proven on the native host

The bulk constructor reads `String.length`, creates one backing and one piece,
and does not normalize, validate, encode, hash, or index the payload.

Representative AOT receipts were:

| Operation | 10 MiB | 100 MiB | Interpretation |
| --- | ---: | ---: | --- |
| lazy handle adoption p50 | 0.33 us | 0.33 us | size-independent on this VM |
| adoption observed p99 | 2-8 us | 1.8 us | one 10 MiB max was 0.73 ms under allocation pressure |
| immediate Backspace + undo p50 | 0.17 us | 0.17 us | source payload was not scanned |
| immediate Backspace + undo observed p99 | 0.25-1.54 us | 0.38 us | one 10 MiB max was 2.5 us |
| forced-owned 4 KiB range read p50 | 131-132 us | 131 us | bounded by output, not document size |
| forced-owned 4 KiB observed p99 | 0.21-0.82 ms | 0.21 ms | one 10 MiB run maxed at 2.47 ms |
| delete all but 8 KiB and compact | 0.30 ms | 0.30 ms | current root retains 8 KiB of backing |
| 8 KiB enrichment poll p50 | 32-33 us | 32 us | this work belongs on the worker |
| enrichment poll observed p99 | 0.14-0.20 ms | 0.07 ms | maxima were 0.62-1.83 ms and 0.39 ms |
| full summary scan | 48-57 ms | 424 ms | linear total work, cooperatively sliced |

`String.length` stayed effectively constant from 1 to 100 MiB. Random
`codeUnitAt`, lazy adoption, and immediate edit timings also did not scale with
document size. This is VM evidence, not a Dart language complexity guarantee.

The source state before enrichment is deliberately explicit:

- UTF-16 extent, bounded range reads, piece anchors, selection offsets, local
  edits, and undo are exact.
- UTF-8 length/coordinates, line count, content hash, validation, and parser
  certification are pending.
- A worker request can carry `(base certified revision, logical revision,
  start/end UTF-16, immutable String/blob handle)`. A certified reply carries
  the fingerprint and exact derived summaries.

The main source therefore remains useful during worker latency; it is not a
second fallible Markdown parser. The ordered provisional intent journal is
small coordination state, not grammar prediction.

## Findings that challenged the model

### Pending really means provisional

An arbitrary Dart String can contain a lone surrogate. Accepting a 100 MiB
String as a committed valid source without reading it is impossible under the
current scalar-source contract. The spike accepts it only as a logical
candidate. Enrichment reports the exact invalid UTF-16 offset; it does not
silently encode U+FFFD.

The honest product policy is:

- large programmatic ingest/open has an async certified API;
- a large user paste may echo provisionally and remain immediately undoable,
  while the last certified revision stays canonical;
- a decoder boundary may use a trusted fast path only when it formally
  guarantees scalar-valid output;
- arbitrary UTF-16 must not be redefined as committed Markdown merely to make
  adoption look O(1).

The production candidate/committed state machine must test validation failure,
supersession, undo before acknowledgment, worker restart, and a later edit that
deletes malformed content. The spike's sequential per-backing validator does
not solve the last case for a large surviving suffix; a real worker job must
validate the current live piece sequence or support independently indexed
ranges.

Large same-length no-op detection is also deferred. The main isolate records an
intent rather than comparing 10 MiB. The worker may certify it as a no-op and
coalesce the logical transaction without ever making a provisional hash
authoritative.

### Initial open and hot paste have different newline contracts

Correction after auditing the current package source: v2
`FlarkDocument.fromMarkdown` eagerly normalizes CRLF and lone CR, but the
current `FlarkV3SourceDocument.fromString` already preserves their exact source
spelling and its tests enforce logical CRLF/lone-CR line aggregates. Ordinary
`apply` replacements preserve spelling as well. The remaining choice is a
pre-launch public-contract decision, not an implementation limitation in the
current v3 source.

A lazy initial open cannot provide both a compatibility-normalized UTF-16
extent and immediate exact positions before scanning. The architecture must
choose one:

1. preserve newline spelling in canonical v3 source and let the grammar stream
   interpret CRLF/CR as line endings;
2. stage normalization and do not expose the candidate as interactive/canonical
   until the worker returns its normalized mapping; or
3. give up viewport-first large open.

Preserving source spelling is the cleaner large-document model and matches the
current v3 source. It intentionally differs from v2; compatibility
normalization must be an explicit staged import transform rather than a hidden
constructor side effect.

### Bounded output does not imply a bounded Dart primitive

The first forced-copy implementation used
`String.fromCharCodes(source.codeUnits, start, end)`. On this VM it walked the
iterable prefix: a 4 KiB suffix read from a 10 MiB backing took about 99-110 ms
AOT, and an 8 KiB compaction took about 104 ms.

Using `source.codeUnits.sublist(start, end)` before `String.fromCharCodes`
restored output-bounded behavior: about 131 us p50 for a 4 KiB owned read and
about 0.30 ms to own two 4 KiB deletion survivors. This is the same ownership
idiom used by the current v3 source.

Plain `substring` was about 1.9 us for 4 KiB on the native VM, but the Dart API
does not promise whether a platform/runtime internally retains the parent
String. The conservative spike forces ownership. Production may use a
verified VM-specific copy fast path, but web retention must be measured.

### Deletion reclamation includes history policy

After deleting all but 8 KiB, the new current root structurally references only
8 KiB of owned backing. That is not a claim that process RSS or total editor
retention falls to 8 KiB:

- the previous persistent snapshot correctly retains the full backing for undo;
- the caller or platform may still retain the original paste String;
- GC timing is nondeterministic;
- larger survivors need background compaction rather than a large synchronous
  copy.

Production needs a byte-budgeted undo/history policy, explicit bulk-snapshot
eviction, and a compaction job whose results are adopted only for the matching
piece/revision. An item-count-only history limit is insufficient for one
100 MiB paste.

## Web evidence

The dart2js and dart2wasm probes were run under Node/V8, not a browser. They
showed size-independent String length, lazy-handle creation, and random access
from 1 to 100 MiB. A forced-owned 4 KiB slice was approximately:

| Runtime proxy | p50 | observed p99 |
| --- | ---: | ---: |
| dart2js on Node/V8 | 125 us | 125-250 us |
| dart2wasm on Node/V8 | 51-52 us | 103-123 us |

The same-realm sub-microsecond results are timer-resolution/optimizer limited;
they only reject obvious document-length scaling.

The Node `worker_threads` structured-clone proxy was materially different:

| String | `postMessage` call p50 | roundtrip p50 | observed send max |
| ---: | ---: | ---: | ---: |
| 1 MiB | 0.042 ms | 0.24 ms | 0.25 ms |
| 10 MiB | 0.39 ms | 2.33 ms | 4.58 ms |
| 100 MiB | 4.61 ms | 22.5 ms | 72.4 ms |

This is not a browser launch receipt, but it is enough to reject inheriting the
native-isolate immutable-String sharing assumption on web. For a main-owned
100 MiB String, total transfer work is unavoidable; the architecture must make
each main-thread chunk bounded and prioritize the active/visible region. For a
file or Blob already backed outside Dart, the worker should read that backing
directly rather than first materializing a main-thread whole String.

## Why pure worker-canonical is not the default

The Option-B harness kept only global UTF-16 length, a 4 KiB active island, and
an ordered journal. It confirmed the expected trade:

- IME/local edits and pending undo can be immediate inside the island;
- arbitrary range read, large copy, and cold commands require prefetch/async;
- worker restart requires a replayable base provider in addition to the
  journal;
- acknowledgment reconciliation becomes a product-visible state machine;
- a main-origin bulk paste still crosses the same web worker boundary.

Those costs are not justified merely to avoid a lazy main tree whose ordinary
local edit cost is already document-size independent. Worker-origin backings
remain valuable as an ingress-specific optimization for very large file/web
opens; that is the hybrid part of the recommendation.

## Required next gate

Before folding this into RFC 023, build one real candidate/certified source job
using the persistent sum tree and parser worker:

1. small certified edit, then 10/100 MiB provisional paste;
2. immediate Backspace, cross-piece range/selection, and undo before worker
   acknowledgment;
3. UTF-16 bulk intent transport, worker validation/UTF-8/hash, exact certified
   reply, cancellation, supersession, and stale-reply rejection;
4. malformed surrogate rejection and deletion of malformed content before
   validation reaches it;
5. CRLF hot paste plus a separately specified large-open policy;
6. large no-op certification and revision/history coalescing;
7. high-ratio deletion, byte-budgeted undo eviction, and background compaction;
8. Chrome/Safari/Firefox worker/Blob receipts, Flutter dart2js/dart2wasm, and
   floor-device AOT p99/p999 parser-to-paint measurements.

The direction should be rejected if provisional state leaks into every ordinary
edit after certification, if active commands routinely block on summary pages,
or if bounded web handoff cannot keep event-loop tails inside the launch budget.
The current receipts do not show those failures; they do rule out an eager
always-indexed main source and a one-message web bulk handoff.

The requested next gate is now implemented and reported in
[`PERSISTENT_CANDIDATE_SOURCE_RESULTS.md`](PERSISTENT_CANDIDATE_SOURCE_RESULTS.md).
It replaces this file's List harness with a persistent AVL sum tree, a real
native isolate, transferable prefix indexes, candidate/certified promotion,
supersession, malformed-delete handling, and byte-budgeted history.

## Reproduction

```sh
DART=/Users/dan/Coding/flutter_arm64/bin/dart

$DART compile exe \
  tool/parser_research/dart/lazy_bulk_source_probe.dart \
  -o /tmp/flark_lazy_bulk_probe
/tmp/flark_lazy_bulk_probe --size-mib=10 --iterations=100
/tmp/flark_lazy_bulk_probe --size-mib=100 --iterations=100

$DART compile js -O4 -DFLARK_WEB_RUNTIME=dart2js_node_v8 \
  tool/parser_research/dart/lazy_bulk_web_string_probe.dart \
  -o /tmp/flark_lazy_bulk_web_probe.js
node /tmp/flark_lazy_bulk_web_probe.js

$DART compile wasm -DFLARK_WEB_RUNTIME=dart2wasm_node_v8 \
  tool/parser_research/dart/lazy_bulk_web_string_probe.dart \
  -o /tmp/flark_lazy_bulk_web_probe.wasm
node tool/parser_research/dart/lazy_bulk_wasm_runner.mjs \
  /tmp/flark_lazy_bulk_web_probe

node tool/parser_research/dart/lazy_bulk_node_worker_probe.mjs
```

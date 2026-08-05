# Persistent candidate/certified source gate

Status: disposable architecture evidence, not production code or a launch gate  
Date: 2026-07-15  
Host: MacBookPro18,1, Apple M1 Pro, 16 GiB, macOS, Dart 3.12.2

The executable spike is
[`persistent_candidate_source_probe.dart`](persistent_candidate_source_probe.dart).
It replaces the earlier List harness with a persistent AVL sum tree and a real
native Dart isolate source worker.

## Verdict

The Option-A-led hybrid remains the best direction. This gate did **not** find
a reason to pivot to a worker-only canonical source.

The provisional layer stayed contained to one explicit state:

- the last certified snapshot owns authoritative hash/UTF-8/line metadata;
- one latest candidate owns exact UTF-16 source, active anchors, selection,
  bounded reads, undo, and an ordered intent journal;
- a large operation is provisional until worker certification;
- small edits during that interval append to the same candidate journal;
- atomic promotion removes the candidate state;
- subsequent ordinary edits certify synchronously again and are mirrored to
  the worker in order.

The worker was able to consume 1,000 ordinary certified edits after a bulk
promotion and then certify a later bulk edit from that mirrored revision. The
candidate state therefore did not permanently infect the ordinary source API.

The new caveat is allocator/jank, not algorithmic scaling. Main-isolate active
edits were extremely fast, but adversarial cold edits and long persistent-root
churn still produced rare multi-millisecond Dart/VM tails. A Dart isolate also
did not give a hard heartbeat bound while doing 100 MiB background indexing.
The architecture should proceed, but a packed/page-backed main tree and the
Rust worker path remain launch gates.

## Architecture exercised

### Main source

Each immutable tree node stores exact UTF-16 sum, height, piece count, and a
summary that is either ready or pending. Leaves reference immutable String
backings plus piece-relative origin ranges and an optional certified range
index. AVL split/concat copies only root-to-leaf paths.

Active anchors carry stable piece identity/origin plus their current global
UTF-16 coordinate. The small set of UI anchors is transformed directly from
each edit descriptor in O(1); arbitrary document-wide anchor lookup was not
added to this Dart spike.

An ordinary replacement of at most 8 KiB is scalar-validated and indexed
synchronously. If no candidate exists, the new root summary is exact and the
revision certifies synchronously. A larger replacement creates one pending
leaf and a UTF-16 worker intent without reading the payload.

### Worker certification and non-linear promotion

The isolate keeps its own persistent certified root. A job starts from the
declared certified revision, applies the ordered UTF-16 journal, then validates
only live pending leaf ranges in 8 KiB cooperative polls. It computes UTF-8
length, logical line breaks, and a polynomial content hash.

For each new live bulk leaf the worker returns transferable cumulative prefix
checkpoints. At the default 4 KiB spacing each checkpoint is 16 bytes:

- relative UTF-16 offset;
- cumulative UTF-8 bytes;
- cumulative logical line breaks;
- cumulative 32-bit hash.

The production design uses all hash lanes, but the range derivation is the
same. A range summary uses two binary-searched checkpoints, a polynomial
prefix subtraction, and at most one checkpoint interval of boundary scanning
per endpoint. The transferred typed buffer is adopted without copying its
contents.

Promotion replaces each pending leaf down one AVL path and rebuilds only that
path. A synthetic height-15 tree with 8,193 existing pieces touched 15 nodes
and promoted in 4-10 us AOT. Promotion did not scan or rebind the other pieces.

At 100 MiB, 4 KiB checkpoints were about 400 KiB and atomic main adoption was
7-12 us in the measured shallow candidate. This removes the earlier concern
that certification necessarily requires a linear main-tree rebind.

## Correctness receipts

The executable fails closed if any check below fails.

### Provisional editing

Before a 10/100 MiB acknowledgment, the main candidate successfully exercised:

- exact prefix-to-bulk cross-piece read;
- active anchor/caret transformation;
- Backspace against the bulk suffix;
- immediate undo before acknowledgment;
- a second Backspace and superseding worker request;
- stale/cancelled first-reply rejection;
- exact latest-reply promotion.

Representative AOT main costs were:

| Operation | Observed |
| --- | ---: |
| provisional 10/100 MiB adoption | 7-17 us |
| four-code-unit cross-piece read | 2-5 us |
| pre-ack Backspace | 1-3 us |
| pre-ack undo | below 1 us |
| worker dispatch call | 1-6 us |
| atomic promotion | 4-12 us |

These timings exclude construction of the incoming String, Flutter text-input
delivery, paint, and web structured clone.

### Malformed source edited before validation

A provisional value contained one lone high surrogate between two 1 MiB live
ranges. The first worker job began, then a main edit deleted that exact UTF-16
unit and superseded it before validation reached the bad range. The replacement
job built the final live piece sequence first, validated the two survivors,
skipped the deleted hole, and promoted exact `...aabb...` source.

The same malformed value without the deletion rejected at exact global UTF-16
offset 1,048,581. The last certified revision remained revision zero; invalid
source never became canonical.

### CRLF policy

A 2 MiB hot paste preserved literal CRLF and lone CR source spelling. Worker
line summaries treated CRLF as one logical line break and lone CR as one, and
the certified source still returned the original code units.

Initial open remains a separate policy:

- preserve-source-spelling can expose an interactive raw candidate
  immediately;
- compatibility normalization of a large initial open is explicitly staged
  and async;
- the spike does not pretend normalized extent is known before scanning.

### History and backing release

A large delete kept only two 4 KiB survivors. The current root copied exactly
8 KiB into bounded owned backings. The newest history entry intentionally kept
the prior 2 MiB backing, so immediate undo remained exact even though the
64 KiB history budget was temporarily exceeded.

After a subsequent edit, byte-budget eviction removed that oversized old root:

| State | Source bytes reachable from session roots |
| --- | ---: |
| current root after deletion | 8,192 |
| current + immediate undo root | 2,105,344 |
| after next edit evicts oversized history | 8,193 |

History also has a 2,048-entry ceiling. This is still a policy harness; real
undo grouping should charge deleted/inserted backing leases and avoid one root
per keystroke where product history groups continuous typing.

## Performance findings

### Active versus cold edits matters

With default 4 KiB checkpoints, 1,000 repeated edits at one active location in
an already-certified 10/100 MiB bulk source measured approximately:

| Bulk source | p50 | p99 | observed p999 | max |
| ---: | ---: | ---: | ---: | ---: |
| 10 MiB | 1.1 us | 22-57 us | 0.08-1.88 ms | 0.15-2.88 ms |
| 100 MiB | 1.2 us | 25 us | 0.16 ms | 0.40 ms |

Those samples include sending the compact certified edit to the worker mirror.
Once the active location is split into small pieces, the giant bulk backing is
not rescanned.

The deliberate cold-random lane touched new bulk checkpoints on every edit:

| Bulk source | p50 | observed p99 | observed p999/max |
| ---: | ---: | ---: | ---: |
| 10 MiB | 99-118 us | 0.58-1.00 ms | 3-6 ms / 14-18 ms |
| 100 MiB | 87 us | 1.73 ms | 6.82 ms / 7.73 ms |

Typical cost stayed document-size independent, but the allocation/GC tail did
not meet a hard four-millisecond claim.

This supports a two-level index rather than a uniformly fine index:

- keep approximately 4 KiB global checkpoints for modest memory/transfer;
- refine/cache the active island and visible ranges more finely;
- route large scattered programmatic batches to the worker;
- do not optimize the entire 100 MiB document for random synchronous edits a
  human editor does not generate.

A 1 KiB compile-time variant reduced cold-random 10/100 MiB p50 to 24-31 us
and p99 to 0.55-0.73 ms, but increased checkpoint transfer fourfold (about
1.6 MiB at 100 MiB) and increased worker allocation/contention. It is useful as
an active-region granularity, not clearly as the global default.

### O(log pieces) edit behavior

The 10,000-edit AOT lane grew from one source piece to 10,002 pieces:

- tree height: 15;
- first 100 average charged tree visits: 21.7;
- last 100: 54.5;
- maximum: 56;
- p50: about 4.3-4.5 us;
- observed p99: 9-69 us across runs.

The node count rose with tree height, not document bytes or piece count
linearly. Atomic deep-tree promotion likewise touched exactly one height-sized
path.

Rare tails varied substantially: p999 ranged from tens of microseconds to
about 2.2 ms, and observed maxima ranged from roughly 6 to 20 ms. This loop is
an intentionally allocation-heavy worst case: every character creates a
persistent revision, retains up to the history limit, then starts evicting old
roots. It proves the algorithm but falsifies a hard jank guarantee for ordinary
Dart object allocation.

Before launch, the main source needs one of:

1. packed/page-backed node storage with preallocated capacity and bounded
   iterative reclamation;
2. a source tree in the native/WASM arena with a bounded synchronous edit call
   plus the Dart active island;
3. evidence on floor devices that object-tree tails stay below the actual
   parser-to-paint budget after realistic undo grouping.

The first option preserves the strongest synchronous main-source semantics and
is the next local falsification target. Worker-canonical should become the
fallback only if that bounded storage path fails.

### Worker isolation is not a jank proof

Default 4 KiB-checkpoint runs observed roughly:

| Candidate | worker completion | main 1 ms heartbeat maximum |
| ---: | ---: | ---: |
| 10 MiB | 75-247 ms | 1.7-15 ms |
| 100 MiB | 0.72-1.48 s | 1.8-22 ms |

The heartbeat is a host Timer proxy, not a frame measurement, but repeated
multi-millisecond gaps falsify a hard claim based only on “it runs in an
isolate.” CPU, allocator, VM, GC, and OS contention remain shared.

Sleeping one millisecond every 16 worker polls did not solve it: a 100 MiB run
took about 3.5 s and still observed a 46 ms heartbeat gap. Coarse throttling is
not the answer.

The disposable worker builds indexes in Dart and accumulates checkpoint lists.
The promising production shape is the Rust OwnedParseJob building source index
pages in its bounded arena, prioritizing active/visible ranges, and streaming
bounded transferable pages. Native/WASM worker and physical-device receipts are
still required.

## What this changes in the architecture

The source protocol should distinguish:

```text
CertifiedSnapshot
  root + revision + exact fingerprint/UTF-8/line metadata

LatestCandidate
  exact UTF-16 root + base certified revision + logical revision
  + ordered UTF-16 intents + pending leaf identities

WorkerReply
  request/revision identity + validation result + exact root summary
  + transferable prefix-index pages for newly certified live leaves
```

Ordinary main certification and worker mirroring are separate facts. Main can
certify a small scalar-valid edit immediately, then enqueue that compact edit
to the worker. A barrier/revision protocol prevents a later bulk job from
starting against a lagging worker mirror.

No Markdown prediction exists in this layer. The worker/parser remains the one
grammar authority; the main source only owns exact text and coordinate facts.

## Remaining gaps

- The spike supports one edit per transaction, one 32-bit hash lane, and one
  latest candidate. Production needs atomic multi-edit sorting and all hash
  lanes.
- It has active-anchor transformation, not a persistent arbitrary-anchor
  registry.
- Worker error/restart recovery is minimal; restart must replay the certified
  root/provider plus ordered journal.
- Prefix-index messages are one transferable buffer per pending leaf. The
  production page protocol should stream bounded pages and validate page caps.
- No Flutter `TextEditingValue`, IME composition, active-island handoff,
  parser/layout/paint, physical-device, browser Worker, or WASM memory receipt
  is included.
- Both sides reuse the same Dart summary algorithm. Small differential String
  oracles pass, but the real gate must compare Rust/native/WASM output and clean
  full parse convergence.

## Recommendation and next gate

Keep the A-led hybrid and feed this source model into the real OwnedParseJob.
Do not pivot to worker-canonical yet.

The next decisive work is:

1. replace Dart object nodes with a small packed/page-arena source-tree spike
   and repeat the 10,000-edit/undo-tail lane;
2. have the Rust worker build and return the transferable range-index pages as
   part of source-to-block-to-inline work, rather than duplicating a Dart
   background indexer;
3. prioritize active and visible index pages so live rendering does not wait
   for a 100 MiB whole-source scan;
4. run the same protocol under Flutter native and actual browser Workers, then
   measure parser-to-paint p99/p999 on floor devices.

Pivot to worker-canonical only if the packed main source cannot bound edit and
reclamation work, or if candidate/certified reconciliation becomes visible in
ordinary post-promotion APIs. Neither failure occurred in this gate.

## Reproduction

```sh
DART=/Users/dan/Coding/flutter_arm64/bin/dart

$DART analyze \
  tool/parser_research/dart/persistent_candidate_source_probe.dart

$DART compile exe \
  tool/parser_research/dart/persistent_candidate_source_probe.dart \
  -o /tmp/flark_candidate_source_probe
/tmp/flark_candidate_source_probe --size-mib=10 --edits=10000
/tmp/flark_candidate_source_probe --size-mib=100 --edits=1000

# Finer global-checkpoint comparison.
$DART compile exe -DFLARK_CHECKPOINT_UTF16=1024 \
  tool/parser_research/dart/persistent_candidate_source_probe.dart \
  -o /tmp/flark_candidate_source_probe_1k
/tmp/flark_candidate_source_probe_1k --size-mib=10 --edits=10000
/tmp/flark_candidate_source_probe_1k --size-mib=100 --edits=1000

# Falsified coarse background-throttling experiment.
$DART compile exe -DFLARK_WORKER_THROTTLE_EVERY_POLLS=16 \
  tool/parser_research/dart/persistent_candidate_source_probe.dart \
  -o /tmp/flark_candidate_source_probe_throttled
/tmp/flark_candidate_source_probe_throttled --size-mib=100 --edits=1000
```

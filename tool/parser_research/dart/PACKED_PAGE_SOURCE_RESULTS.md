# Packed/page-backed main-source gate

Status: disposable architecture evidence, not production code or a launch gate  
Date: 2026-07-15  
Host: MacBookPro18,1, Apple M1 Pro, 16 GiB, macOS, Dart 3.12.2

The executable spike is
[`packed_page_source_probe.dart`](packed_page_source_probe.dart). It compares a
fixed-page persistent UTF-16 sum tree with a no-old-root/inverse-transaction
challenger and with the existing object-AVL receipts from
[`persistent_candidate_source_probe.dart`](persistent_candidate_source_probe.dart).

## Verdict

Packing works, but persistent packed roots should **not** become the default
architecture yet.

In clean serialized AOT runs, fixed typed pages removed the current object
probe's rare multi-millisecond edit tails. The result was document-size
independent from 10 to 100 MiB, allocated no pages during the measured edits,
and reclaimed roots iteratively within a fixed node budget. This is strong
evidence that a bounded Dart main-isolate source is feasible; moving all
canonical source ownership to the worker is not necessary for performance.

The more important result is that main-side persistence is probably solving
the wrong ownership problem. The inverse-history challenger retained only the
current root and kept bounded inverse source transactions for undo. It matched
the packed persistent edit latency while retaining about one sixth as many
nodes after 2,000 edits. A truly mutable current tree would also avoid the
functional path allocations that this conservative challenger still performs.

The recommended order is therefore:

1. prototype a mutable/logarithmic current Dart piece tree with a fixed,
   byte-bounded inverse transaction ring;
2. let the worker's Crop/source snapshots own immutable parser-job history;
3. retain only one explicit main-side base lease while a provisional bulk edit
   is being certified;
4. keep this packed arena as the fallback behind the same source API if the
   simpler mutable model fails physical-device p99/p999 gates.

This is a simplification of the earlier provisional architecture, not a move
to worker-canonical source. The main isolate still owns exact interactive
UTF-16 text, caret/selection, IME, undo, and the latest revision. Undo emits a
new forward source revision to the worker; it does not require restoring an
old main tree root.

## What the spike implements

The core is production-shaped enough to test the lifetime claim:

- a manually managed arena of 2,048-node `Uint32List` pages;
- fixed integer node handles, packed sums, heights, summary metadata, and
  backing ranges;
- persistent AVL split/concat with O(log pieces) edits;
- explicit reference counts for root and parent leases;
- an intrusive retirement queue with fixed 256-node edit slices;
- node/page reuse without recursive Dart object destruction;
- a fixed ring of persistent undo roots;
- a fixed inverse transaction store that leases no old roots;
- a current active anchor transformed directly by each source edit;
- exact UTF-16, UTF-8, CRLF/lone-CR, and content-hash summaries;
- sparse 4 KiB prefix indexes for certified large backings;
- an explicit pending-summary state for lazy large backings;
- bounded range reads with no whole-document edit copy.

The executable fails closed on differential String-oracle edits, surrogate
boundaries, exact CRLF/UTF-8 summaries, old-root undo, inverse-transaction undo,
pending bulk edits, active-anchor transformation, balanced sums, byte-exact
reads, bounded retirement, page reuse, and complete backing release.

No measured edit copied the document. A certified cold edit scanned at most
one 4,095-code-unit checkpoint interval in any atomic scan. Across the several
new fragment summaries created by one edit, the worst total was about 31 KiB;
that is why cold random edits remain around 80-100 us while active edits are
sub-microsecond.

## AOT receipts

The tables below use clean serialized runs. The same binaries were also run
while the host was materially CPU/GPU contended; those wall-clock tails are
discussed separately.

### Existing object AVL versus packed persistent

Two 10 MiB runs produced these ranges:

| Lane | Object AVL | Packed persistent |
| --- | ---: | ---: |
| active p50 | 1.17-1.21 us | 0.67-0.71 us |
| active p99 | 6.6-8.3 us | 0.83-3.0 us |
| active max | 15-48 us | 8.5-47.5 us |
| cold-random p50 | 93.5-95.1 us | 81.8-83.4 us |
| cold-random p99 | 413-502 us | 149-169 us |
| cold-random max | 2.16-2.44 ms | 0.206-0.469 ms |
| 10k churn p50 | 4.08-4.29 us | 7.33-7.67 us |
| 10k churn p99 | 6.2-12.4 us | 10.1-25.3 us |
| 10k churn max | 5.76-6.85 ms | 0.144-0.252 ms |

Packing moved the tail substantially but did not make every operation faster.
The 10,000-insertion median became about 1.8x slower because each logical node
access performs page/slot indirection and explicit reference-count work.

The object comparison is useful but not a clean causal proof that Dart node
objects alone created every old tail. The existing harness also creates edit,
work, receipt, and timing objects, retains immutable paths, and evicts history
with `List.removeAt(0)`. The packed harness uses a fixed ring and an
allocation-neutral long-lived sample clock. Those are exactly the unnecessary
costs the mutable-current challenger should remove before a manual arena is
adopted.

### 100 MiB scale check

The packed persistent result did not get slower with document extent:

| Lane | 10 MiB representative | 100 MiB |
| --- | ---: | ---: |
| active p50 / max | 0.67 us / 8.5 us | 0.708 us / 8.0 us |
| cold p50 / p99 / max | 81.8 / 148.7 / 205.9 us | 83.9 / 154.4 / 214.0 us |
| 10k churn p50 / p99 / max | 7.33 / 25.3 / 144.0 us | 7.33 / 14.0 / 58.0 us |
| edit-time page growth | 0 | 0 |
| largest measured heartbeat gap | 3.11 ms | 3.22 ms |

The 100 MiB sparse prefix index was 409,616 bytes and took 491 ms to build in
this AOT Dart probe. That build is not proposed as synchronous main-isolate
work. In the candidate/certified architecture, the worker builds and streams
those pages while the main source remains an exact pending candidate.

### Persistent roots versus inverse transactions

After 1,000 active and 1,000 cold edits to the same 10 MiB source:

| Retained state | Packed persistent | Inverse/no-old-root |
| --- | ---: | ---: |
| live arena nodes | 23,181 | 4,005 |
| high-water arena nodes | 23,204 | 4,047 |
| live backing slots | 2,001 | 1,002 |
| tree pieces | 2,003 | 2,003 |
| active p50 | 0.67-0.71 us | 0.88-0.92 us |
| cold p50 | 81.8-83.4 us | 84.6-85.6 us |
| cold max | 0.206-0.469 ms | 0.188-0.199 ms |

Both modes still allocated about 42,900 logical node versions because the
challenger deliberately reuses the functional update algorithm. The inverse
mode reclaimed those versions immediately instead of leasing them through
2,048 old roots. A mutable current tree should remove most path-version
allocation as well, so this is a conservative result in its favor.

The 10,000-revision persistent lane ended with 10,002 pieces, height 15,
47,412 live nodes, 10,001 live backing slots, and 2,048 history roots. It
performed 365,883 logical node allocations but reused 318,434 retired slots.
No new page was allocated during the edits.

## Reclamation and source safety

Retirement was physical and bounded:

- the edit-time retirement queue returned to zero after each measured edit;
- normal edit slices processed at most 28 nodes in the 100 MiB lane and 40 in
  the 10,000-revision lane, below the 256-node budget;
- closing the 10,000-revision session took 186 yielded slices of at most 256
  nodes, rather than one recursive last-owner drop;
- clean-run 256-node drain slices took at most 57 us;
- every verification ended with zero live nodes and zero live backing leases.

Large lazy source remains compatible with this storage. A backing without a
worker prefix index carries an explicit pending summary, but exact UTF-16
reads, small edits, undo, and anchor transforms still work. A small edit does
not pretend the pending bulk value has become certified.

The arena does not make Dart memory generally non-GC. Backing Strings, inverse
transaction payloads, timers, Flutter objects, and the rest of the application
remain managed. Page allocation itself can also pause if capacity is exhausted.
Production would need low-water refill outside the input phase, hard page and
backing caps, and fail-closed/backpressure behavior. The measured runs reserved
64 pages (131,072 nodes) in about 1-3 ms before the hot lane and observed zero
hot page growth.

## Contention falsification

Identical packed binaries were run while unrelated host GPU/CPU work was
materially contending. Despite zero page growth and an empty retirement queue,
individual wall-clock maxima rose into 9-53 ms and heartbeat gaps into 16-55
ms. A later serialized run returned to the sub-millisecond edit tails above.

Therefore the correct claim is limited:

- packing can remove this source's allocator/path-retirement contribution;
- it cannot provide a hard wall-clock no-jank guarantee;
- process scheduling, shared VM/GC activity, and renderer contention still
  require cooperative batching, frame-aware admission, and device p99/p999
  gates.

An isolate, a typed arena, or Rust/WASM can bound owned work. None can promise a
deadline while the OS does not schedule the UI thread.

## Complexity judgment

The packed source/index/arena/history core occupies roughly 980 lines in this
spike before the correctness and benchmark harness. More important than the
line count, it introduces manual ownership invariants across:

- parent references and root leases;
- split/concat ownership transfer;
- AVL single/double rotations;
- leaf-to-backing reference counts;
- intrusive retire/free queues;
- page capacity and reuse;
- summary propagation and pending state;
- root-history and undo ownership.

The first generic join implementation in the spike oscillated between tall
subtrees and overflowed the stack; it had to be replaced with explicit
single/double balancing. The final executable is checked, but that failure is
a fair maintenance warning: typed storage exchanges GC uncertainty for custom
memory-management correctness.

Rust/Crop would make persistent snapshot ownership more conventional, but
putting the only interactive source behind FFI/WASM introduces synchronous
bridge and web-memory questions. The cleaner division is likely:

```text
Main Dart isolate
  mutable exact current UTF-16 piece tree
  active anchor / selection / IME
  byte-bounded inverse transaction ring
  at most one provisional base lease + ordered edit journal

Worker native/WASM
  Crop persistent source snapshots
  immutable parser-job revision leases
  sparse UTF-16/UTF-8/line/hash index pages
  block / inline / projection work
```

That model uses persistence where concurrent parser jobs need it and mutation
where the single main-isolate editor owns one current state.

## Next decision gate

Before adopting this arena, build the smallest mutable-object challenger:

1. in-place AVL/B-tree rotations and sum updates for one current piece tree;
2. a fixed byte- and entry-bounded inverse transaction ring with grouped IME
   and typing transactions;
3. one provisional base-source lease for bulk certification, released on
   promotion/cancellation;
4. revision-token replay to the worker Crop mirror, including undo as a new
   forward edit;
5. the same allocation-neutral 10/100 MiB active, cold, and 10k churn lanes;
6. Flutter floor-device parser-to-paint p99/p999 with GC and frame telemetry.

Choose the packed arena only if that simpler current-state model still shows
source-attributable tails over the input budget. Choose native/WASM current
source only if both Dart shapes fail or cross-runtime ownership becomes simpler
than maintaining exact Dart coordinates. The evidence here establishes a
credible bounded fallback; it does not justify paying for it preemptively.

## Reproduction

```sh
DART=/Users/dan/Coding/flutter_arm64/bin/dart

$DART analyze tool/parser_research/dart/packed_page_source_probe.dart
$DART compile exe \
  tool/parser_research/dart/packed_page_source_probe.dart \
  -o /tmp/flark_packed_page_source_probe

/tmp/flark_packed_page_source_probe \
  --size-mib=10 \
  --active-edits=1000 \
  --cold-edits=1000 \
  --churn-edits=10000 \
  --reserve-nodes=131072

/tmp/flark_packed_page_source_probe \
  --size-mib=100 \
  --active-edits=1000 \
  --cold-edits=1000 \
  --churn-edits=10000 \
  --reserve-nodes=131072

# Existing object-AVL comparison.
$DART compile exe \
  tool/parser_research/dart/persistent_candidate_source_probe.dart \
  -o /tmp/flark_object_candidate_source_probe
/tmp/flark_object_candidate_source_probe --size-mib=10 --edits=10000
/tmp/flark_object_candidate_source_probe --size-mib=100 --edits=10000
```

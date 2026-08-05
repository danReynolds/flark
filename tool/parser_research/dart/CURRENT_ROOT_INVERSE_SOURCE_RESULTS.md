# One-current-root object source gate

Status: disposable architecture evidence, not production code or a launch gate  
Date: 2026-07-15  
Host: MacBookPro18,1, Apple M1 Pro, 16 GiB, macOS, Dart 3.12.2

The executable gate is the `--gate=current-root` mode in
[`persistent_candidate_source_probe.dart`](persistent_candidate_source_probe.dart).
It reuses the existing exact functional object-AVL edit kernel and 4 KiB
certified backing indexes. The control changes only ownership: one mode keeps
one current root plus inverse transactions, and the other keeps old roots.

## Verdict

The UI source does **not** need to be mutable as an architectural premise.

The simplest justified first implementation is:

```text
one current functional object-tree root
fixed byte- and entry-bounded inverse transaction ring
no old Dart root in undo history
undo published as a new forward worker revision
```

Retained roots, rather than functional path allocation by itself, caused the
large source-owned GC tails in this host gate. For the same 100 MiB trace, both
modes allocated exactly 354,623 node versions. The inverse mode retained 8,293
nodes; the root-history control retained 165,501. In the 10,000-edit trace both
allocated exactly 370,870 versions, while old-root history repeatedly produced
11-16 ms scavenges and the current-root mode's observed scavenge was 1.6 ms.

Functional updates still allocate. They are not proven harmless on floor
devices. But their allocation is short-lived, their p99/p999 host cost is below
one millisecond in every lane, and the object representation has materially
lower typical overhead than the packed arena. That evidence does not justify
adding in-place rotations, mutable aliasing rules, or manual storage now.

Keep the source API representation-neutral. Select in-place mutation only if a
profile-mode floor-device trace attributes a missed source-slice p999 gate to
the remaining functional allocations. Keep the already-green packed arena as
the bounded-allocation fallback if mutation is insufficient or too difficult
to verify.

## What is held constant

Both object modes use the same:

- `_split`, `_concat`, `_balance`, summary, compaction, and replacement code;
- immutable backing and piece objects;
- exact UTF-16, UTF-8, logical CRLF/lone-CR, and hash aggregates;
- scalar-boundary validation;
- 4 KiB sparse source checkpoints;
- edit sequence and transaction grouping;
- preallocated `Uint64List` timing samples and one long-lived sample clock;
- 2,048-entry history capacity; and
- AOT executable in a fresh process.

The inverse ring preallocates its operation arrays. It stores positions,
inserted lengths, and copied ordinary-lane deleted payloads, never `_Node` or a
root. It charges fixed metadata plus retained UTF-16 bytes, groups up to 64
operations, permits the newest oversized entry temporarily, and evicts it when
the next transaction would otherwise leave history over budget.

The root control stores one old root per equivalent grouped transaction. It is
intentionally a stronger causal control than comparing against the older
candidate harness, which also allocated timing receipts and used a shifting
`List.removeAt(0)` history.

## Exactness gate

The AOT/JIT-independent semantic run fails closed on:

- 1,024 deterministic randomized edits against a Dart `String` oracle;
- source spelling including CRLF and lone CR;
- exact UTF-16 and UTF-8 lengths and bidirectional scalar-boundary mappings;
- non-BMP scalars and rejection of offsets inside surrogate pairs;
- exact content hash and logical line aggregates after edits;
- AVL height/sum/summary invariants;
- upstream and downstream active-anchor transforms;
- typing-group undo, including 128 deterministic randomized grouped
  transactions and complete LIFO unwind;
- undo advancing the source from revision N to N+1 as one forward batch; and
- entry and retained-byte history eviction.

The verification receipt is:

```text
current_root_exact_gate:
  string_differential_edits=1024
  utf16_utf8_roundtrips=true
  crlf_lone_cr=true
  scalar_boundaries=true
  anchors=true
  grouped_undo=true
  randomized_grouped_undo_transactions=128
  undo_is_new_forward_revision=true
  old_roots_in_inverse_history=0
```

## Serialized AOT receipts

These are representative clean, serialized runs. Maxima are receipts, not
deadlines. Repeats and verbose-GC attribution are discussed below.

### 10 MiB

| Lane | Current root + inverse | Equivalent old-root control |
| --- | ---: | ---: |
| active p50 / p99 / max | 0.29 / 2.08 / 122 us | 0.25 / 1.71 / 143 us |
| cold p50 / p99 / max | 104 / 395 / 2,167 us | 103 / 400 / 2,182 us |
| 16-op scattered batch p50 / p99 / max | 21.9 / 263 / 1,962 us | 22.5 / 282 / 11,327 us |
| 10k churn p50 / p99 / p999 / max | 1.29 / 3.33 / 10.0 / 657 us | 1.29 / 3.54 / 61.0 / 10,667 us |
| retained nodes after large lanes | 8,293 | 166,134 |
| retained backing objects after large lanes | 3,260 | 11,573 |

One earlier clean-process current-root run was host-noisy: active p999 was
833 us and one cold edit reached 20.5 ms. Subsequent serialized repeats returned
to the table's range. That outlier is retained as evidence that a host maximum
cannot establish a deadline and that device telemetry remains mandatory.

### 100 MiB

| Lane | Current root + inverse | Equivalent old-root control |
| --- | ---: | ---: |
| active p50 / p99 / max | 0.29 / 2.83 / 61 us | 0.29 / 2.58 / 115 us |
| cold p50 / p99 / p999 / max | 77.5 / 353 / 426 / 1,379 us | 79.9 / 378 / 620 / 1,811 us |
| 16-op scattered batch p50 / p99 / max | 26.5 / 544 / 1,611 us | 26.8 / 561 / 6,140 us |
| 10k churn p50 / p99 / p999 / max | 1.33 / 2.42 / 6.63 / 2,108 us | 1.33 / 3.25 / 63.2 / 16,147 us |
| retained nodes after large lanes | 8,293 | 165,501 |
| retained backing objects after large lanes | 2,430 | 10,743 |
| large-lane RSS at receipt | 154 MiB | 182-184 MiB |

Document extent did not affect active or churn medians. Cold cost is governed
by the bounded backing checkpoint scan and local tree shape, not by 10 versus
100 MiB extent. The 100 MiB index was 409,616 bytes and took about 621-648 ms
to build; production builds it off the UI isolate and attaches returned pages.

### GC attribution

`DART_VM_OPTIONS=--verbose-gc` made the ownership difference explicit.

- Current root: the 100 MiB churn maximum was 1.62 ms while the VM reported a
  1.6 ms new-space scavenge.
- Root history: churn reached 16.15 ms while the VM reported 15.6 and 16.1 ms
  scavenges. Retained paths promoted 10.9-16.2 MiB into old space.

Therefore node retention is causally implicated. Functional allocation is also
observable—the current-root scavenge proves it—but it did not miss a reasonable
host source-slice p999 gate. Absolute maxima remain scheduler- and GC-sensitive.

## Packed fallback comparison

The existing packed/inverse AOT gate was rerun with 2,000 active and 2,000 cold
edits at each size. At 100 MiB it reported:

| Lane | Object current root | Packed inverse challenger |
| --- | ---: | ---: |
| active p50 / p99 / max | 0.29 / 2.83 / 61 us | 0.92 / 1.04 / 16 us |
| cold p50 / p99 / max | 77.5 / 353 / 1,379 us | 87.2 / 153 / 208 us |
| retained logical nodes | 8,293 after 4,512 revisions | 8,005 after 4,000 revisions |

The packed arena provides a materially tighter 100 MiB cold maximum but costs
about 3x on the active median and introduces explicit reference counts, free
lists, page capacity, and bounded retirement. Its existing probe has no
equivalent scattered-batch inverse lane and its 10k lane retains roots, so no
claim is made for those comparisons. The packed result brackets the benefit of
removing managed path allocation; it remains a useful fallback, not the default.

## Reclamation and backing receipts

Dart does not expose deterministic object reclamation to this AOT program. The
gate therefore reports two different facts rather than calling both “freed”:

1. exact graph reachability from the current root and any history roots; and
2. node versions allocated minus graph-reachable versions, which are logically
   unreachable and eligible for Dart GC.

Closing a session clears the current root and every history slot; graph-owned
nodes and backing objects then report zero. This proves ownership release, not
the time at which Dart's GC returns memory.

The inverse ring stores copied deleted source for the ordinary lane. This gate
does **not** prove a synchronous document-scale delete: a deletion above the
ordinary routing cap must retain bounded piece/backing descriptors or use the
explicit provisional-base lease rather than materialize the deleted range.
That bulk/history integration remains a separate acceptance gate.

## Architectural consequence

Use “single current root” as the invariant, not “mutable tree”:

```text
Flutter UI isolate
  exact current UTF-16 functional object tree
  bounded inverse transaction ring
  active selection, caret, composition, and source island
  ordered edits to the worker

worker
  immutable Crop revisions and parser-job leases
```

An edit may allocate and publish a new functional root, but the previous root
must become unreachable immediately unless an explicit short-lived handoff owns
it. Undo reconstructs source through an inverse forward transaction; it never
restores a Dart snapshot. Public source APIs must not expose tree identity, so
an in-place or packed implementation remains substitutable behind the same
contract.

Do not build an in-place tree merely because it sounds more optimal. Build it
only if floor-device profile telemetry shows the remaining current-root
functional allocation causes the source mutation slice to miss its p99/p999
budget. At that point the challenger must cover all four lanes and exactness,
not only an append-only microbenchmark.

## Evidence boundary

This proves a host-side ownership and source-algorithm decision. It does not
prove:

- Flutter frame, paint, layout, or IME deadlines;
- floor iOS/Android source p99/p999 or GC behavior;
- browser JavaScript/Wasm allocation and Worker transfer;
- provisional 10/100 MiB paste certification in the new inverse model;
- a document-scale delete/undo payload lease; or
- memory pressure while the rest of the editor and renderer are active.

Those are launch gates. A host AOT maximum is neither a no-jank guarantee nor a
reason to pre-emptively adopt manual memory management.

## Reproduction

```sh
DART=/Users/dan/Coding/flutter_arm64/bin/dart
SOURCE=tool/parser_research/dart/persistent_candidate_source_probe.dart

$DART analyze $SOURCE
$DART compile exe $SOURCE -o /tmp/flark_current_root_gate

/tmp/flark_current_root_gate \
  --gate=current-root --phase=verify --model=current

for SIZE in 10 100; do
  /tmp/flark_current_root_gate \
    --gate=current-root --phase=benchmark --model=current \
    --size-mib=$SIZE --active-edits=2000 --cold-edits=2000 \
    --batch-rounds=512 --batch-size=16 --churn-edits=10000 \
    --history-entries=2048 --history-bytes=8388608 \
    --history-operations=64

  /tmp/flark_current_root_gate \
    --gate=current-root --phase=benchmark --model=roots \
    --size-mib=$SIZE --active-edits=2000 --cold-edits=2000 \
    --batch-rounds=512 --batch-size=16 --churn-edits=10000 \
    --history-entries=2048 --history-bytes=8388608 \
    --history-operations=64
done

DART_VM_OPTIONS=--verbose-gc /tmp/flark_current_root_gate \
  --gate=current-root --phase=benchmark --model=current \
  --size-mib=100 --active-edits=2000 --cold-edits=2000 \
  --batch-rounds=512 --batch-size=16 --churn-edits=10000
```

Run models serially in fresh processes. Concurrent runs are useful contention
falsification but are not valid allocator comparisons.

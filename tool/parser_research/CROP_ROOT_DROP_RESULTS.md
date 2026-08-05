# Source-root and lineage-snapshot retirement probe

Status: **direct final-root destruction is rejected on edit admission; the v3
slice now transfers explicit bounded retirement ownership. Native/Wasm host
scheduling and device validation remain open. Scalar lineage remains a strict
small-history mechanism rather than gaining a second arena**, 2026-07-16.

## Question

The source and candidate transition can be fully prepared before the worker
actor publishes it, but assigning the prepared Crop root may destroy the last
owner of the previous immutable tree. That destructor is real work. This probe
separates:

- old-root construction;
- new-root construction;
- the source-root assignment/admission boundary;
- destruction of an explicitly retained old root; and
- destruction of the replacement root.

It also measures the analogous worst scalar-lineage case: a mapping job holds
one complete persistent lineage snapshot, the live ring overwrites every slot,
and dropping the job releases the last historical tree owner.

The executable harness is
`v3_runtime_slice/src/bin/crop_root_drop_probe.rs`. These are optimized
workstation receipts after one warm-up sample, not floor-device launch claims.

## Whole-document replacement receipt

Each lane replaces an all-ASCII document with a disjoint same-sized document.
`direct` lets source-root assignment destroy the old root. `retained` holds one
additional opaque owner through assignment and destroys it explicitly
afterward. Times are milliseconds over 100 samples.

| Size | Mode | Admission p50 / p95 / max | Deferred old drop p50 / p95 / max | New-root build p50 / p95 / max |
| --- | --- | ---: | ---: | ---: |
| 10 MiB | direct | 0.468 / 0.749 / 1.043 | 0 / 0 / 0 | 1.581 / 2.203 / 4.131 |
| 10 MiB | retained | 0.000041 / 0.000125 / 0.000250 | 0.503 / 0.778 / 0.803 | 1.594 / 2.070 / 2.312 |
| 100 MiB | direct | 5.301 / 6.943 / 11.789 | 0 / 0 / 0 | 14.415 / 19.848 / 21.966 |
| 100 MiB | retained | 0.000041 / 0.000125 / 0.000250 | 5.266 / 5.971 / 9.192 | 14.503 / 18.752 / 26.895 |

The retained lane does not make destruction cheaper. It makes ownership and
scheduling explicit: at 100 MiB it moves roughly 5--9 ms out of the atomic
source-publication boundary while reducing that assignment to tens of
nanoseconds. The roughly 14--27 ms construction tail is also real, but it is
already worker-side preflight and a whole-document replacement is routed as
bulk work rather than an ordinary typing kernel.

## Scalar-lineage snapshot receipt

For capacity `H`, the harness fills all `H` ring slots, starts a revision-zero
mapping job, performs another `H` edits so the live ring shares no nodes with
the historical snapshot, verifies revision zero has expired from the live
ring, and drops the job. A complete tree contains `2H - 1` nodes. Setup timings
cover `H` individual edits and are not one atomic operation. Times are
milliseconds over 100 samples.

| H | Historical nodes | Fill p50 / p95 / max | Full overwrite p50 / p95 / max | Last historical drop p50 / p95 / max |
| ---: | ---: | ---: | ---: | ---: |
| 1,000 | 1,999 | 0.502 / 0.592 / 4.008 | 0.510 / 0.748 / 3.919 | 0.039 / 0.043 / 0.052 |
| 10,000 | 19,999 | 6.289 / 8.076 / 13.374 | 6.323 / 9.685 / 13.037 | 0.463 / 0.762 / 4.169 |

This confirms the earlier four-sample observation while exposing a rare
10,000-record host tail above 4 ms. Lineage capacity is independent of document
size and expiry is correctness-safe: an expired proof triggers an exact clean
restart. The simple choice is therefore a strict, floor-device-calibrated small
history, with approximately 1,000 records as the current host-supported
starting point. Do not add another arena/reclaimer for lineage unless real
floor-device or workload evidence rejects that cap. Production constructors
must enforce the selected maximum rather than accept arbitrary capacity.

## Architecture consequence

`SourceStore` publication must not incidentally drop the previous Crop root.
The v3 slice now uses `mem::replace` and returns an opaque, non-cloneable
`RetiredSourceRoot` to `LiveDocumentStore`. Admission places that owner into an
inline FIFO without allocation or destruction.

- Native runtimes may drain the owner on a dedicated disposer.
- WebAssembly must drain only after the edit response/urgent candidate slice;
  because the Rust heap cannot simply be transferred to another Web Worker,
  the 100 MiB whole-replacement lane still needs an explicit worker scheduling,
  memory-backpressure, and sustained-replacement device gate.
- Incidental parser or query leases are not the mechanism: the last owner would
  otherwise be destroyed by whichever caller happens to release it last.
- The retirement queue must be bounded by owners and retained bytes. Saturation
  routes bulk work through backpressure; it may not move destruction back into
  ordinary edit admission or permit an unbounded stale-root queue.

This is a lifecycle correction around the selected immutable source model, not
a reason to replace Crop or add another source representation. Ordinary local
edits share almost the whole tree and should retire only their divergent path;
the deliberately disjoint replacement proves the required worst-case handoff.

## Implemented retirement-lane receipt

The production `LiveDocumentStore::accept_edit` seam now has both owner and
logical-source-byte bounds:

- four inline FIFO slots, with saturation checked before source preparation or
  candidate cancellation;
- a 256 MiB pessimistic logical-byte budget, charging each queued root at its
  full source length even when Crop shares most physical nodes;
- preflight for `max(old_root_bytes, prepared_next_root_bytes)`, so the old root
  fits on success and the unpublished next root fits if coordinator preparation
  or candidate cancellation subsequently rejects the edit;
- an allocation-free FIFO `take`/`drain` transfer to the host-selected disposal
  lane; and
- a borrowed `SourceQueryView` on the public live-document seam. It owns no
  `Arc`, cannot mint an owning cursor, and cannot overlap a mutable edit. The
  second recognition cursor is issued directly into the worker-owned candidate
  rather than through an escaping cloneable query snapshot.

Seven focused unit tests pass in debug and release. They prove success
retention and off-lane final destruction with a weak observer, FIFO order,
owner-count and logical-byte backpressure without clock/candidate mutation,
and retirement of unpublished roots after both coordinator and cancellation
rejection. `cargo clippy --lib -- -D warnings` is also green. The 256 MiB policy
admits the measured 100 MiB worst-case root, but it is deliberately a
source-byte accounting bound rather than a claim about exact allocator/RSS
retention.

The remaining platform contract is real. Taking the owner does not itself
choose a thread: native must call `dispose` on a disposer lane, while Wasm must
call it from a post-response/idle slice with backpressure. Closing a whole
document must likewise retire the actor/current root on a non-urgent lane, and
dropping `LiveDocumentStore` with an undrained FIFO is not yet a launch-safe
host path. These are host-integration and physical-device gates, not hidden
fallible paths in ordinary edit admission.

## Reproduction

```text
cargo build --release --bin crop_root_drop_probe
./target/release/crop_root_drop_probe root 10 100
./target/release/crop_root_drop_probe root 100 100
./target/release/crop_root_drop_probe lineage 1000 100
./target/release/crop_root_drop_probe lineage 10000 100
```

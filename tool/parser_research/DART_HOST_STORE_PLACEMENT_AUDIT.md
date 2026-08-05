# Dart host-store placement audit

Status: **production boundary selected; adapters and device traces pending**, 2026-07-18.

## Decision

Do not implement a second persistent measured-green tree in Dart.

The selected host representation is the same Rust persistent page store on
both platforms:

- native: one UI-isolate-owned FFI handle into the Rust host store;
- web: one separately instantiated, main-context WebAssembly host store; and
- parser execution: a long-lived isolate/Web Worker with its own unrelated
  parser memory and roots.

The worker transfers only bounded, credited closed-leaf chunks. The host store
validates and adopts them into an unqueryable staging root, performs the typed
splice, and atomically swaps an exact-current target. Dart owns canonical
source/input/selection/IME, the small authority controller, and bounded copied
viewport DTOs. It does not own green pages, a document manifest, a splice
engine, or a recursive document-sized object graph.

This preserves one implementation of the hard invariants: packed-green decode,
Program closure validation, persistent sequence summaries, typed splice proof,
viewport context recovery, staged abort, and fuelled retirement.

## Dart call surface

`FlarkV3HostStore` is intentionally an ABI-neutral logical interface. Native
FFI and web JavaScript/WebAssembly adapters implement the same calls:

1. `observeSourceVersion(exact)` immediately advances source authority and
   supersedes an in-flight offer.
2. `beginOffer(declaration)` admits one exact-source transaction.
3. `admitChunk(ownedBuffer)` consumes one transferred bounded buffer and its
   sole credit.
4. `poll(grant)` advances byte inspection, copying, allocations, commit
   preparation, or retirement under independent grants.
5. `requestCommit(offer, actual chunks/bytes, rolling transport digest,
   inserted-stream digest)` closes the one-pass stream and requests final
   validation plus atomic publication. Begin carries ceilings, not unavailable
   precomputed totals or target digests; the host computes ACK digests.
6. `acknowledgeDelivery(ack)` clears same-session publication backpressure.
7. `queryStructural(point, budget)` carries explicit encoded-byte, open-depth,
   and leaf-count ceilings and returns a typed structural-or-`SourceGap`
   outcome plus its work/size receipt. The current Rust depth-cap gap is
   BOF-to-EOF; preserving its range/reason leaves room for a later proven
   narrower fallback.
8. `abortOffer` and `close` begin fuelled cleanup rather than recursively
   destroying a document-sized Dart/Rust/Wasm object graph.

The Dart controller independently refuses stale offers and stale query output,
but these checks are not a replacement for the store's final exact-source/base
recheck. While structure is absent or behind, it exposes one BOF-to-EOF
`SourceGap`. An old presentation ACK may identify paint-cache shards only;
semantic actions, accessibility semantics, Markdown hit targets, and semantic
selection maps are disabled.

## Platform feasibility

No concrete cross-platform blocker was found.

The existing Flark web bridge already demonstrates loading a Wasm asset,
calling synchronous exports, copying a bounded typed-array input into Wasm
memory, and reading a bounded result on the browser main context. The host
store needs a separate module instance/handle from the parser worker, not
shared Wasm memory. This avoids `SharedArrayBuffer`, cross-origin isolation,
and synchronization as launch requirements. A bounded viewport query can stay
synchronous after asynchronous module initialization.

Native FFI already has the corresponding dynamic-library and pointer-response
pattern. Publication chunk construction remains in the parser isolate;
`TransferableTypedData.materialize()` produces the one bounded view consumed
by the UI-owned FFI handle. Web must use an explicitly transferred
`ArrayBuffer`; Flutter `compute()` is not an offload there.

There is an unavoidable bounded copy into the native/Wasm host arena and a
bounded copy out for viewport DTOs. This is preferable to a second tree and is
covered by the existing per-chunk/per-viewport budget. The adapters still need
floor-device measurements including Wasm memory growth and GC tails.

## Concrete mismatch found and resolved

`FlarkV3SourceDocument` and the Rust Crop source both legitimately begin at
revision zero, while `host_mirror::prepare_full_snapshot_bundle` had treated
zero as invalid even though it had no sentinel role there. That would have made
an untouched initial document unpublishable or forced a fragile bridge-only
`revision + 1` translation.

The host-only rejection is now removed and the Rust test
`full_snapshot_accepts_valid_source_revision_zero` pins the cross-boundary
value. Dart source, parser mirror, green manifest, offers, and ACK/base checks
can therefore keep numerically identical revisions.

## What the Dart prototype proves and does not prove

The focused model tests prove the controller contract: initial BOF gap,
Unicode source advance, stale offer rejection, nonzero middle-delta metadata,
atomic authority adoption, one-unacknowledged-offer backpressure, fresh-session
lost-ACK recovery, buffer ownership at the adapter seam, explicit viewport
budgets/typed depth fallback, startup degradation, and fail-closed store
desynchronization.

The fake is deliberately not a green/tree implementation. It does not prove
Rust chunk decoding, persistent splice complexity, viewport reconstruction,
retirement bounds, FFI/Wasm ABI correctness, or parser-to-paint latency. Those
remain Rust mechanism and native/web integration gates; reproducing them in
Dart would provide false confidence and create the maintenance split this
placement decision removes.

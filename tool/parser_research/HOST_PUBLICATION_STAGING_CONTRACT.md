# Large host-publication staging contract

Status: **canonical snapshot + authenticated delta staging mechanism GO; production transport/platform wiring HOLD**, 2026-07-18.

This contract replaces the v3 host mirror's one-shot `StructuralBundle` path
for initial snapshots, large paste, worker recovery, and any changed range that
does not fit the calibrated urgent bundle lane. It does not weaken the
exact-current host-authority rule and it does not make a stale parse
renderable.

## Decision

Use one protocol and one host-owned persistent representation for both small
and large publications:

```text
OfferBegin
  -> credited closed-leaf Chunk(s)
  -> resumable validation + prebuilt inserted sequence
  -> Commit request
  -> resumable commit preparation
  -> exact-current/base recheck
  -> atomic root + ACK swap

OfferBegin / Chunk / preparation
  -> Supersede or Abort
  -> constant-time authority withdrawal
  -> fuelled journal release and page reclamation
```

The existing small-bundle call may remain temporarily as a compatibility
adapter which internally emits one begin, one chunk, and one commit. It must
not remain a second mutation implementation.

The worker never constructs one document-sized bundle. The host never
constructs one document-sized object map or inserted-leaf vector. A complete
snapshot is assembled behind the installed root, then published by one pointer
swap. A large delta is assembled as its own sequence, then attached to the
exact installed base through the existing resumable persistent-sequence
splice. The final swap does not scale with document size or changed-range
size.

This is not parser partial publication. Until commit, the staging tree has no
query, paint, semantic-action, accessibility, or restart authority.

## Executable mechanism result

`v3_runtime_slice/src/host_publication_staging.rs` implements a separate
feature-gated mechanism probe using the production arena journal and
`ResumableStreamingSequenceBuilder`, `ResumableSequenceSplice`, and the
canonical copied-green continuation. The focused command is:

```text
cargo test --offline --features host-publication-staging-probe \
  --lib host_publication_staging::tests -- --nocapture
```

All ten tests pass. The dense snapshot receipt is:

```text
logical bytes                104,859,000
wire bytes                   111,035,250
leaves                       27,450
credited chunks              429
maximum live chunk bytes     258,880
maximum poll inspect bytes   4,096
maximum poll copy bytes      4,022
maximum poll transitions     35
publication poll transitions 2
publication poll copied bytes 81
arena high-water bytes       115,733,748
debug-host elapsed           10.315-12.880 seconds in observed runs
```

Elapsed time is a debug mechanism observation, not a performance claim. The
fixture exceeds the former 8 MiB and 8,192-object envelopes on both logical
and actual wire/storage work, while retaining one 256 KiB input buffer. The
last poll includes final manifest preparation plus the atomic swap; its two
transitions and 81 copied bytes are target-size independent. Separate tests
prove:

- canonical exact-page Program and zero-Program leaves under one-transition
  validation fuel;
- canonical corruption rejection even when the transport digest is valid;
- complete-document structural rejection across individually canonical leaf
  boundaries;
- authenticated middle-range delta publication under one-transition splice
  fuel, with exact untouched prefix/suffix wrapper `ArenaId` reuse;
- wrong but equal-shaped deleted-range proof rejection with the installed root
  unchanged;
- deletion to empty and insertion into an empty base;
- cancellation and source supersession at every observable staging/splice
  phase with fuel-one abort/reclamation; and
- one-credit backpressure and corrupt-chunk fail-closed behavior.

The probe also caught and corrected architecture-level hazards:

- `OfferBegin` exact transport totals caused a hidden output prepass, so actual
  totals/digests now close the one-pass stream at `Commit`;
- a sequential digest fold cannot reuse tree `height` without overflowing on
  large streams, so wire accumulation is explicitly height-free; and
- packed-page capacity includes child-edge storage, so payload and Program
  edges share one admitted page envelope;
- a leaf-valid closure is not necessarily document-valid, so sequence summaries
  carry structural balance/minimum-prefix and the manifest validates the whole
  document; and
- exact base ACK plus range coordinates do not authenticate the intended
  deletion, so the splice now checks a typed, height-free deleted-subtree
  summary immediately before releasing that subtree.

What this closes is the large canonical snapshot transaction, authenticated
persistent delta attachment, atomic visibility, and bounded lifetime shape.
It does **not** yet close production transport or platform integration.
Specifically:

- current full/delta exporters still need to stream record-local closures
  without first creating `StructuralBundle` vectors/maps;
- native FFI, main-context Wasm, `TransferableTypedData`, transferred
  `ArrayBuffer`, Wasm-memory copy/growth, GC/RSS, and floor-device frame traces
  remain physical product gates; and
- continuous-edit starvation during a 100 MiB initial snapshot remains a UX
  measurement and possible typed staging-reuse follow-up.

## Why the current bundle is not a large-document protocol

The feasibility host mirror currently limits one bundle to 8,192 copied
objects and 8 MiB, stages all objects in a `BTreeMap`, collects every inserted
leaf in a `Vec`, builds all measured pages synchronously, and assigns the new
`Arc` root in the same call. Raising those caps would turn a deliberate safety
limit into an unbounded UI-isolate kernel. Splitting the same bundle into
several ordinary splices would expose intermediate target states and make
atomic whole-target validation impossible.

The required correction is a transaction with resumable construction, not a
larger message.

## Authority and identities

`OfferBegin` binds all of the following before the host accepts a byte:

- protocol schema and a non-reusable `OfferId`;
- publication session and target host revision;
- exact target `SourceVersion` (document session, source revision, byte/UTF-16
  metric, and source content hash);
- parse generation and grammar revision;
- mode: fresh full snapshot or exact-base delta;
- for a delta, the complete installed base ACK fingerprint: publication
  session, host revision, manifest digest, sequence digest, leaf count, metric,
  and source revision;
- the typed leaf splice coordinates and deleted-range identity/content
  digests;
- expected inserted leaf count and target leaf count/metric, already available
  from persistent green/manifest summaries;
- hard maximum chunk count and total encoded bytes admitted for this offer;
  and
- declared per-chunk and per-closed-leaf bounds, which may only narrow the
  product profile.

Begin deliberately does **not** require the exact chunk count, exact wire byte
count, target content digest, or final rolling transport digest. The current
packed-green summary has no composable content digest, and obtaining one would
require the worker to pre-walk output or enlarge every persistent green
summary. Exact wire totals likewise require pre-encoding before the first
chunk. Chunks are instead produced in one pass. `Commit` supplies the actual
chunk/byte totals, final rolling transport digest, and inserted-stream content
digest. The host compares those with its streaming counters and inserted
builder summary, then computes the session-specific target sequence and
manifest digests for the ACK. Source, parse generation, grammar revision,
target leaf count/metric, canonical per-record validation, and any typed base
splice remain the semantic authority. Adding a digest to every green summary
can be reconsidered for a broader semantic need; this transport does not force
it.

The host admits at most one staging offer. An unacknowledged installed result
keeps the current same-session backpressure rule; lost-ACK recovery is a fresh
session plus a full snapshot.

Begin performs cheap validation and takes one arena build lease. It does not
retain the complete base tree, allocate target-sized storage, inspect source,
or withdraw the installed root.

## Chunk contract

One chunk is one transferred byte buffer with a fixed header and an ordered
sequence of complete closed-leaf records. Its header carries:

```text
OfferId
chunk ordinal
first inserted-leaf ordinal
leaf count
encoded byte count
chunk digest
```

Each record contains one stable green leaf ID plus the complete ordered
Program closure needed to decode that leaf after the worker root retires.
Program entries are record-local values, not globally addressable host
objects. Their worker child IDs are checked against that record's leaf edges
and then discarded. A shared Program used by another record is repeated and
validated independently, even across the same chunk. Equal IDs across records
therefore do not require conflict detection or a document-scale registry; only
the stable leaf ID participates in host sequence identity. This small wire
duplication makes every accepted record independently reclaimable.

The transport has one credit. The worker may send the next chunk only after
the host has completely validated and adopted the previous buffer into the
staging build. Thus live unconsumed transfer storage is at most one bounded
chunk, not one document or an event-loop queue of chunks.

The initial candidate limits are 256 KiB encoded per chunk, 4 KiB per arena
page, 128 Program children per leaf, and a separate 256 KiB retained
closed-leaf closure limit. These are memory/rejection ceilings, not launch
constants or atomic-kernel grants. A closure which can reach the child-count
times page-size product must use resumable header/page/event validation; it
cannot be smuggled through as one atomic leaf operation.

Validation rejects, without changing the installed root:

- wrong offer, ordinal, or first-leaf position;
- duplicate, skipped, replayed, truncated, or trailing records;
- a chunk/record/page/child count beyond the admitted profile;
- cross-session IDs or cross-chunk dependencies;
- bad object, closure, chunk, or rolling-offer digest;
- a Program in a leaf position or a leaf in a Program position;
- invalid packed-green encoding, metric, fact, structural, or child closure;
  and
- totals that overflow or exceed the begin declaration.

## Resumable host builder

The host consumes a chunk under three independent grants:

- encoded bytes inspected or hashed;
- bytes copied into host-owned pages; and
- state transitions/page allocations.

A poll may enter a page-copy kernel only when enough byte grant remains for
the whole bounded page. Closed-leaf validation is an explicit continuation
over its header, leaf bytes, and Program pages. No poll allocates a vector
proportional to the remaining chunk, closure, changed range, or document.

Copied-green admission uses one canonical continuation over header, structural
events, Program headers, and Program pieces. It calls the same canonical
decoders as installed green storage, checks exact child ordinals, refolds the
leaf summary, and enforces packed payload-plus-edge capacity before allocating
any staged page. The summary-only staging mode does not materialize a second
event vector or construct a scratch arena/second parser.

After resumably validating one closed leaf, the host allocates its bounded
record-local Program pages and unchanged canonical packed leaf in the staging
arena. A small host sequence wrapper owns that canonical leaf and becomes one
input to the existing `ResumableStreamingSequenceBuilder`. The builder keeps
at most one completed subtree per power-of-two leaf count and allocates at most
one branch per poll. Its scratch is preflighted before input.

Chunk acceptance advances only after every record is in that builder and the
chunk totals/digest match. The host also rejects a chunk which would cross the
begin-declared maximum chunk or total-byte envelope. A `ChunkAck` returns the
single transport credit.

The staging state stores counters and rolling summaries, not a list of prior
chunks or leaves. The sequence root owns all accepted leaf closures.

## Commit preparation and atomic publication

`Commit(OfferId, actual_chunk_count, actual_encoded_bytes,
rolling_transport_digest, inserted_stream_digest)` closes the input stream.
It first checks those actuals against the host's streaming counters, verifies
they stayed inside the begin maxima, and checks the inserted leaf count/metric
and content digest against the staged inserted sequence. It then starts a
resumable preparation job:

- **full snapshot:** finish the streaming sequence and wrap it in a target
  manifest;
- **delta:** retain the still-installed base root into the same build journal,
  run the existing allocation-granular owned-sequence splice using the
  already-built inserted root, and validate the actual isolated deleted
  subtree against the actor's typed height-free identity/content summary
  immediately before release; then wrap the result in a target manifest.

All splits, joins, rotations, range checks, and fallible allocations happen
before publication. One poll allocates at most one branch. A manifest is the
sole build owner before `ArenaBuildSession::commit` transfers it to the host.

Immediately before the swap, the host rechecks:

1. the exact current `SourceVersion` equals the begin target;
2. for a delta, the exact base ACK/root fingerprint is still installed;
3. target leaf count/metric equal the begin declaration, and the host-computed
   target sequence/manifest digests equal the prepared ACK; and
4. an ACK and one bounded retirement slot are available.

After that preflight the commit kernel is only:

```text
installed root <- prepared root
pending ACK    <- prepared ACK
dirty overlay  <- none
old root       -> preflighted retirement slot
```

There is no decoding, hashing, tree walk, changed-leaf loop, heap reservation,
or recursive destruction after the final recheck. Queries observe either the
complete prior root or the complete exact-current target root.

## Supersession, abort, and disposal

Any source advance makes an offer ineligible to commit. The host marks it
superseded in constant time, withdraws its input credit, and begins arena-build
abort without walking the candidate. Fuelled abort polling transfers at most
the granted number of journal owners to the arena release queue. Separate
fuelled reclamation drops at most the granted bounded pages/edges.

Committed old roots use the same bounded retirement lane. Dropping an `Arc`,
arena root, Dart list, or Wasm wrapper and hoping the last-owner cascade lands
off the frame is not an accepted disposal mechanism. Document close and
worker-recovery teardown use this lane too.

Only one offer may stage at once. A newer offer waits until the superseded
build journal has relinquished its small live-owner set; page reclamation may
continue behind it subject to arena byte backpressure. This bounds concurrent
candidate roots without making the UI wait for complete reclamation.

### Continuous-edit starvation

Exact-current-only publication has an honest consequence: a 100 MiB initial
snapshot can be repeatedly superseded if the user starts editing before it
finishes. The launch-safe behavior is exact source/caret/IME plus the stable
pending presentation until a current snapshot converges. The host must not
commit a known-stale structural tree merely to show progress.

The first optimization, if device traces show unacceptable starvation, is
typed reuse of already-validated staging subtrees across a replacement offer.
That requires worker-authored prefix/suffix survival proof and a new offer
generation; content-address coincidence alone is insufficient. Retargeting an
offer in place or silently applying edits to it is not part of this contract.

Urgent viewport facts remain a separate exact, request-scoped presentation
path. They may make the active region rich before a whole snapshot commits,
but cannot grant the staging tree global structural authority.

## Native and web placement

The protocol is platform-neutral, but the selected authoritative host store is
one Rust implementation on both platforms: in-process native code behind FFI
on Dart VM targets, and the same crate compiled to main-context Wasm on web.
Dart owns the controller, frame scheduler, source/input island, and bounded
viewport cache. A Dart persistent-tree implementation may remain only as a
differential oracle; it is not a second production authority.

### Native Dart/Flutter

Use a long-lived parser isolate. Build a bounded chunk there and send it as
`TransferableTypedData`. Dart documents that constructing the transferable is
linear in its bytes, while sending it through a port is constant time. Chunk
creation therefore belongs to the worker budget and chunk size remains
bounded; “transferable” does not make construction free.

The Rust arena lives behind a handle. The UI isolate submits one materialized
bounded buffer and polls bounded admission/query calls. A second Dart isolate
cannot return a shared arbitrary Dart object graph, so offloading a Dart tree
construction alone would not create a usable host root.

### Flutter web

Flutter `compute()` executes on the main thread on web, so it is not an
offload. Compile a separate Web Worker entrypoint and explicitly transfer the
underlying `ArrayBuffer` in `postMessage`'s transfer list. Posting a typed array
without transferring its buffer takes the structured-clone path and is
disallowed for publication chunks.

The Rust host arena runs as main-context Wasm so it can serve synchronous
bounded viewport queries. The transferred `ArrayBuffer` may still require one
bounded copy into Wasm linear memory; zero-copy across the JS/Wasm boundary is
not assumed. Wasm memory is admitted/grown before an urgent poll, chunks remain
bounded, and allocation/reclaim executes only under fuel. Rust heap ownership
never moves between the Web Worker and main context: only the encoded buffer
does. `SharedArrayBuffer`/threaded Wasm requires cross-origin isolation and
introduces synchronization and deployment constraints; it is an optional
measured specialization, not the correctness baseline.

Primary transfer references:

- [Dart `TransferableTypedData`](https://api.dart.dev/dart-isolate/TransferableTypedData-class.html)
- [Flutter concurrency and the web `compute` limitation](https://docs.flutter.dev/perf/isolates)
- [Dart concurrency on the web](https://dart.dev/language/concurrency#concurrency-on-the-web)
- [MDN transferable objects](https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Transferable_objects)
- [MDN `Worker.postMessage`](https://developer.mozilla.org/en-US/docs/Web/API/Worker/postMessage)

## Dart scheduler contract

Incoming messages only enqueue one small descriptor and schedule work; a
message callback does not decode a chunk. Admission polls run after urgent
input/source work and stop before the frame deadline. The candidate starting
budgets remain:

- ordinary UI-isolate publication poll p99 no more than 2 ms;
- hard single poll below 4 ms on floor devices;
- at most one bounded adoption/commit kernel per frame; and
- missed work remains pending without styled-to-source-to-styled flashing.

Byte/transition fuel is an accounting ceiling, not a time claim. Native and
web traces must calibrate grants independently and may reduce chunk size. The
UI trace includes message callback, materialization/view creation, decode,
allocation, commit swap, framework scheduling, GC, and retirement tails.

## Product scenarios

### 10/100 MiB initial open

Source is installed and editable through the source path. The worker emits a
fresh snapshot as credited chunks. The host incrementally constructs an
unqueryable candidate while source/old presentation stays live. Commit swaps
one complete exact-current root. No stage retains a whole wire image in
addition to the built tree.

### Large paste or whole-document replace

Source adopts the provisional/exact backing under the separate bulk-source
contract. Structural output remains pending. The worker emits either a typed
large delta or a fresh snapshot. Inserted leaves are prebuilt once; final
splice cost is logarithmic in base tree height, not paste size.

### Worker crash or lost ACK

Abort any incomplete offer and fuel its cleanup. A restarted worker uses a new
publication session and full snapshot. A lost committed ACK is recovered by
the already-defined fresh-session full snapshot rule; the old root remains
installed until replacement validation completes.

### Rapid supersession

Source never waits. At most one offer and one chunk buffer exist. Superseded
offers cannot commit, and cleanup cannot monopolize a frame. Latest-wins may
wait for staging-journal release or arena memory credit, which is observable
backpressure rather than an unbounded queue.

## Required falsifiers before production wiring

The Rust staging mechanism is now GO. Production wiring remains HOLD until the
combined executable/device evidence proves all of the following at production
scale and through the real transport adapters:

1. a synthetic 100 MiB ordinary snapshot exceeds the old 8 MiB/8,192-object
   limits, uses one bounded buffer, and commits with target-size-independent
   final work;
2. a large middle paste builds once and its final splice visits/allocates only
   a height-proportional boundary path;
3. abort after every decode, allocation, builder carry, reduction, split,
   rotation, manifest, and pre-swap phase leaves the installed base queryable;
4. source advance and base-ACK replacement immediately make a prepared target
   uncommittable;
5. duplicate, reordered, missing, corrupt, oversized, truncated, and trailing
   chunks fail closed;
6. one-credit backpressure bounds queued transfer bytes under a producer flood;
7. old-root, aborted-candidate, close, and recovery disposal are strictly
   fuelled and arena/RSS retention stays within declared owner and byte caps;
8. native `TransferableTypedData` and web transferred-`ArrayBuffer` traces show
   no document-scale clone;
9. floor-device 1/10/100 MiB parser-to-paint traces meet UI poll, input, IME,
   GC, recovery, and eventual-convergence gates; and
10. continuous typing during a large initial snapshot has an explicit measured
    pending duration and cannot publish stale semantics.

## Implementation map

1. **Done:** the separate Rust protocol probe uses `ArenaBuildSession`,
   `ResumableStreamingSequenceBuilder`, `ResumableSequenceSplice`, canonical
   copied-green validation, and `PageArena::poll_reclaim`.
2. Replace the `MeasuredLeafSequence` `Arc` tree with, or teach it to delegate
   to, the same iteratively retired page arena. Adding staged messages while
   retaining recursive `Arc` root destruction is incomplete.
3. **Done in the selected arena sequence:** attach the prebuilt inserted root
   directly; the staging root is never turned back into `Vec<MeasuredLeaf>` at
   commit.
4. Make current full/delta exporters stream closed leaves directly rather than
   first constructing `StructuralBundle.objects` and `splice.inserted`.
5. Add native isolate and web worker transport adapters with one credit and a
   single owned buffer.
6. Route the legacy small-bundle API through this state machine, then remove
   the one-shot implementation after equivalence tests.

The architectural claim is therefore narrow but strong: large publication is
a staged persistent-tree transaction with exact-current atomic visibility and
fuelled lifetime management. It is not a bigger bundle, a series of visible
partial splices, or a stale-result concession.

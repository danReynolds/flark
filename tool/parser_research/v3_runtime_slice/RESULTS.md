# v3 runtime composition slice: infrastructure and event-persistence result

Status: **infrastructure passed; persistent-sequence mechanics passed; full
structural event history is HOLD as the primary output**.

Update: the compact record-forest challenger now executes in this crate and
confirms bounded long-open-leaf retention, total coverage, structural sharing,
open-overlay/presentation composition, and a provisional direct-child fold
index. The event-tape decision below remains the discriminator that rejected
full history; it is no longer the newest architecture checkpoint. The
direct-child shape is deliberately unselected while it is compared with a
unified balanced-parentheses structural sequence; see
`DIRECT_CHILD_INDEX_CHALLENGER.md`.

This crate composes the identity-safe Crop source, scalar-only edit lineage,
generation-safe page lifetime, and reduced three-clock/latest-one coordinator
without importing the old parser, scheduler, prediction model, block facts, or
Comrak. It now also executes a grammar-free packed event sequence, projection
checkpoints, immutable suffix-preserving splices, and a composite output root.
It is deliberately not a Markdown parser (`MARKDOWN_GRAMMAR_ATTACHED` is
`false`).

## What is now executable

- `SourceStore` owns exactly the current `Arc<CropSnapshotLease>` and a bounded
  persistent segment-tree ring of scalar edit records. A lineage job snapshots
  one scalar tree root in O(1), performs one strictly depth-bounded preflight
  lookup, then validates and maps no more records than its poll fuel. It never
  clones the history or retains a Crop root. Empty ranges are rejected by range
  mapping; boundary movement is explicit through before/after affinity.
- `SourceStore::current_root` returns a non-`Clone` `SourceSnapshotLease` rather
  than its raw `Arc`; turning that lease into a cursor consumes it. Cursor
  refills copy at most 4 KiB even if a future Crop leaf becomes larger.
- `PageArena` stores bounded 4 KiB payloads behind generation-safe integer IDs.
  Child ownership is represented by integer-edge reference counts, and release
  is iterative. One reclaim poll performs no more than its supplied reference-
  transition fuel.
- Query IDs and ownership are now distinct. `ArenaId` remains a small copyable
  generation-safe query handle; allocation/retain returns a non-`Copy`
  `OwnedArenaRef`. Coordinator candidate adoption, sequence construction, page
  checkpoints, and manifest construction consume that token. A caller cannot
  transfer and later release the same reference.
- `Coordinator` keeps separate source, grammar, and parse-generation clocks.
  It owns one active parse plus one replaceable latest queued plan. Only the
  exact latest generation can attach or publish a candidate.
- Output handoff distinguishes worker-current, offered, and acknowledged roles.
  Superseded candidates/offers are queued through `PageArena::release_later`;
  remote release does not accidentally release a still-current worker root.
- Candidate transfer requires a distinct owned token. An arena node query ID
  that is merely reachable through a parent edge is not transferable. Future
  unchanged-output commits can use a new small manifest root while sharing all
  unchanged component children.
- `EventPageBuilder` packs scalar-only structural events and coverage-relative
  anchors into at most 4 KiB. Event stamps are self-relative
  `{generation-safe page ID, local event}`; a prefix edit never rebases a reused
  suffix stamp.
- A non-copyable physical-line continuation token permits event pages to split
  mid-line. Ordinary completed lines may share a page. Once a line spills,
  continuation pages are dedicated to it and must seal before the next
  `CoverageRecord`; this makes mixed continuation/next-line pages
  unrepresentable through the sink API.
- Leading projection checkpoints are separately persistent arena stacks. A
  page builder owns a real checkpoint lease while unsealed, so background
  reclamation cannot stale it; sealing transfers reachability into the page
  child edge and cancellation releases it.
- `OutputSequence` is an arena-backed balanced persistent sequence with byte
  and UTF-16 prefix sums, zero-coverage continuation summaries, immutable
  split/join/splice, and exact leaf/subtree identity reuse. `OutputRootManifest`
  owns the sequence through a real arena child edge and contains no Crop ID,
  descriptor, strong lease, or weak lease.

## Focused receipts

The crate currently has 54 tests (44 integration plus 10 private invariants);
all pass in debug and release builds. Raw WASM compilation and strict whole-
crate Clippy are green.

### Exact source and lineage: 11 integration tests plus 2 private invariant tests

- Unicode, astral scalars, LF/CRLF/lone-CR edits and Crop cursor output match a
  `String` oracle after every edit.
- Operation provenance maps only the exact unchanged prefix/suffix and rejects
  wrong-root, overlapping, and empty ranges.
- A deterministic 32-edit Unicode history checks every nonempty scalar-aligned
  range from every retained revision against unique byte-lineage tags. No
  changed sequence is certified unchanged.
- Multi-edit mapping yields after one edit record per unit of fuel. Constructor
  and poll metrics count every record examined, validated, mapping-attempted,
  and mapped, plus every persistent-tree node read; scalar record copies are
  zero.
- Stale expected revisions do not mutate the source.
- With a scalar mapping job still alive, the old Crop `Weak` lease can no
  longer upgrade; reported lineage retention is zero source roots.
- Bounded history returns `HistoryExpired`, never a byte/hash fallback.
- A deliberately corrupted later transition reports `BrokenChain` from a
  fuelled poll instead of hiding an eager whole-history validation pass.
- A 1,000-record history proves constructor work is one record and at most one
  tree path; zero fuel performs zero hidden work and each poll examines at most
  its 13-record grant.
- A 10,000-record job remains exact after the live ring completely overwrites
  all 10,000 slots. Each edit path-copies at most 15 nodes; construction reads
  14 nodes; mapping 10,000 records reads 143,616 nodes total and at most 15 per
  record. The job and live ring each retain at most 10,000 scalar records and
  19,999 tree nodes, and neither retains a source root.
- A 30,000-byte Unicode cursor fixture refills more than once, reproduces every
  byte, and never copies more than the explicit 4 KiB refill cap.

The 10,000-record full-divergence receipt also found one remaining production
gap: dropping the last old scalar-tree root synchronously releases O(capacity)
nodes. Four optimized host samples released 19,999 nodes in 0.408--0.635 ms;
the debug sample was 1.05 ms. This is small on the current host but is not
fuelled cancellation proof. The shipping default should therefore use a
strict, device-calibrated recent-lineage cap; expiry is always an exact clean
restart. Reuse the existing fuelled arena sequence for lineage only if worker
or device tail receipts reject that simpler cap. Do not add a second custom
reclaimer.

### Arena lifetime and explicit ownership: 5 tests

- Reused slots increment generation; an old `ArenaId` cannot alias the new
  occupant.
- A 20,000-node chain retires without recursive ownership. With fuel 7, every
  receipt reports at most 7 reference transitions, at most 7 reclaimed nodes,
  and at most `7 * 4 KiB` reclaimed payload bytes.
- Shared and duplicate child edges release exactly once per edge.
- Oversized payloads and unbounded slot growth are rejected; 1,000
  allocate/release cycles reuse one slot.
- Two retained `OwnedArenaRef`s are distinct transfers. Releasing one keeps the
  copied query ID live through the other; releasing the second stales it.

### Three-clock coordinator: 6 tests

- Transition identities/revisions must form one exact contiguous source chain.
- Repeated submissions retain the active parse and replace only its one queued
  plan; promotion selects the newest generation.
- A stale generation cannot attach or commit. Its already-attached candidate
  is released through the arena under caller fuel.
- A forged lease with the same remote ID but a different generation/revision
  is rejected.
- Remote release invalidates queries while preserving a worker-current root
  until its replacement commits.
- Across 1,000 commits with the UI deliberately holding the initial
  acknowledged root, published roots remain `<= 3` (observed maximum exactly
  3), background reclaim uses fuel 1, arena live roots remain `<= 3`, and the
  final acknowledged current root remains queryable.

### Packed event persistence: 6 tests

- Every required scalar event kind round-trips through the packed encoding.
  The decoded stamp uses the physical page's generation-safe `ArenaId` plus its
  local offset; page, branch, projection, and manifest headers are exercised by
  actual sequence queries rather than a detached codec-only fixture.
- One physical line emits 1,202 events across 6 pages. Temporary event storage
  peaks at 3,638 bytes, all continuation pages share one persistent projection
  node, and querying the last page visits one checkpoint node and replays zero
  earlier pages.
- The same spill history attempts to append line B to line A's final
  continuation. The builder forces a seal. Byte and UTF-16 lookup return line
  A's complete 6-page window at its start, line B alone at the shared source
  boundary, and the exact final coordinate at document end.
- Inserting one Unicode prefix page before 65,536 pages allocates one event page
  and 16 branch nodes, visits 34 sequence nodes, reuses all 65,536 old pages,
  and preserves both a 32,768-page subtree root and the exact suffix event
  stamp. Tree height changes from 17 to 18. Byte and UTF-16 prefixes shift by
  their independently correct amounts.
- An output manifest remains queryable after the only `SourceStore` and source
  lease are dropped; the observed `Weak<CropSnapshotLease>` cannot upgrade.
  Releasing the manifest reclaims the entire graph under caller fuel and makes
  its generation-safe ID stale.
- Invalid splice bounds and a mismatched projection parent are rejected before
  ownership mutation. Arena live/pending metrics are unchanged and the prior
  sequence remains queryable.

### Decisive retained-output discriminator

A 10 MiB, 100,000-line open paragraph/fence shape was emitted as one
`AppendRuns` and one `WriteEnd` per physical line. This is the straightforward
exact event history, not a deliberately delimiter-dense adversary:

```text
source bytes                 10,000,000
physical lines                  100,000
persistent structural events    200,001
event pages                       1,726
event-page payload bytes      7,044,906
all live arena nodes               5,176
all live arena payload bytes   7,245,006
retained payload per line          72.45 bytes
```

The event tape therefore passes identity, persistence, query, and lifetime
mechanics but fails the architectural economy test as currently shaped. A long
open leaf should not require retained output proportional to every source line
in addition to the compact coverage/run directories. Varints would improve the
constant; they do not remove the `AppendRuns + WriteEnd` record count. Treat
this as a **HOLD on full event history as primary output**, not as a performance
micro-optimization backlog.

## Bounded arena metadata follow-up (2026-07-16)

The original flat `PageArena::slots`, active-build table, and owner-journal
vectors contained a hidden tail: growing metadata for one new page could copy
all prior metadata in the same worker call. The executable arena now removes
that tail with three arena-specific bounds:

- the complete slot-segment descriptor directory is fallibly reserved when the
  arena is admitted; page slots grow in fixed 64-entry segments, retain the
  same `u32` index/generation handles, and move zero old slots/descriptors;
- the active-build directory is fallibly reserved once and guarded by a
  configurable logical cap (16 by default), so adding one `BuildSlot` never
  reallocates the directory; and
- owner journals use 16-entry owner segments. A fixed top directory points to
  64-descriptor blocks, each block preflights its full descriptor capacity
  before the first of its 1,024 owner slots. Neither journal boundary moves old
  owner entries or old segment descriptors.

The production resumable journal default remains 2,048 owners. The distinct
131,072-owner compatibility-transaction envelope exists only because the old
non-yielding 65,536-page oracle intentionally collects that many leaf owners;
it is configurable and is not a production recommendation. The default arena
also has explicit 1,048,576-slot and 512 MiB live encoded-storage limits.
Encoded node storage is now a fallibly reserved `Vec<u8>` whose actual capacity
is charged to the storage budget, avoiding a second potentially infallible
`Vec`-to-`Box<[u8]>` shrink allocation.

`AllocationReceipt`, `ArenaBuildAdmissionReceipt`, `ArenaBuildJournalMetrics`,
and `ArenaMetrics` expose requested initialization, actual allocator capacity,
segment/directory-block addition, hard limits, and prior entries/descriptors
moved. `tests/bounded_arena_metadata.rs` exercises:

- slot boundaries 0/64/128, reuse, generation-safe replacement, and explicit
  130-slot saturation;
- journal boundaries through 1,025 owners: 65 owner segments and two directory
  blocks, including the 1,024-owner directory-block transition;
- a strict two-build envelope and build-slot reuse;
- storage rejection before a page slot or child reference changes;
- journal rejection before a resumable allocation creates a new page owner;
- cancellation of every saturated-candidate owner exactly once while an older
  committed root remains queryable; and
- generation-exhausted slot retirement without aliasing the next slot.

There are two deliberately different failure receipts. A resumable session
preflights journal storage before it creates/retains a new owner; if a later
page allocation fails, at most one fixed, empty journal segment remains
allocated for reuse while logical journal, reference, slot, and candidate state
remain unchanged. The legacy transaction may receive an already-live owner
before it discovers its configured journal is saturated; it therefore performs
exactly one bounded transfer of that owner into iterative reclaim rather than
leak it. A focused unit test proves that transfer plus transaction rollback
reclaims every owner exactly once.

Validation receipts:

- `cargo test --all-targets`: green, including 5/5 new boundary tests and the
  restored 65,536-page prefix-insertion oracle;
- `cargo test --release --all-targets --quiet`: green;
- explicit `wasm32-unknown-unknown` check: green; and
- arena-only strict Clippy (`-D warnings`, `clippy::all`, and
  `clippy::pedantic`): green.

The shared strict-Clippy invocation is temporarily HOLD on concurrent
`candidate_writer`/`live_document`/checkpoint integration warnings; it reports
no arena or bounded-arena-test finding. The 512 MiB default is likewise a
safety envelope, not a 100 MiB product proof. Prior ordinary-shape receipts
suggest the 1,048,576-slot cap has substantial headroom (6,945 nodes per
100,000-block candidate and 5,176 event-tape nodes per 100,000 lines), but the
real worker-current/offered/acknowledged roots, active candidate, source,
presentation state, allocator overhead, and floor-device RSS must be measured
together before selecting a shipping limit or fallback policy.

## Surface audit

Runtime code is 4,323 physical lines; integration tests are 1,549 lines. The
event witness is intentionally kept in one file for this gate, but 1,935 lines
is too large for a production module and should be split by ownership, packing,
projection, and sequence concerns if its reusable mechanics graduate.

| Surface | Current | Prior research surface | Result |
|---|---:|---:|---|
| Crop source plus new lineage | 1,233 runtime lines | 552 (`crop_source.rs`) | The added surface is the measured persistent scalar-ring witness, exact chain validation, work/retention receipts, non-cloneable source lease, capped cursor, and lifetime observer used only by proof tests. Before shipping, reassess whether the generic arena sequence can supply the same persistence primitive without coupling source correctness to output semantics. |
| Page arena + ownership token | 506 | 1,304 (`arena.rs`) | Still 61% smaller; preserves bounded payload, integer edges, generation checks, and fuelled release, and now makes ownership transfer unforgeable from a query ID. |
| Reduced coordinator | 599 | 2,415 (`scheduler.rs`) | 75% smaller; keeps only clocks, latest-one admission, owned candidate adoption, publication roles, exact leases, ack/release, and retirement. |
| Event/coverage codec and page builder | about 780 of 1,935 | old event/fact shapes rejected | Scalar-only bounded pages, continuation capability, self-relative stamps, and page checkpoint ownership. |
| Persistent projection | about 280 of 1,935 | new | Structurally shared stack frames and path-copy updates; no literal/content payload. |
| AVL sequence, prefix query, viewport fold, manifest | about 875 of 1,935 | `relative_output_reuse_gate` algorithms | Arena-backed identity-preserving mechanics; no `Arc` output tree or Crop ownership. |
| Identities and crate boundary | 50 | mixed through old crate | New, dependency-free lifetime-domain boundary. |

A mechanical no-index diff finds 266 textually retained lines in the source
adapter, 233 in the arena, and 124 in the coordinator. Treat those as an
upper-bound audit signal, not a semantic authorship claim: the arena is a
reduced adaptation and the coordinator is a new kernel shaped by the old
state-machine invariants.

The normal dependency tree is only:

```text
flark-v3-runtime-slice
└── crop (pinned git revision d0234ce7)
    └── str_indices
```

There is no dependency on `integrated_parser_slice`, Comrak, or another
Markdown grammar. The crate locally allows only Clippy's missing public
`# Errors`/`# Panics` documentation lints while the prototype API is unstable.
That infrastructure-only snapshot had a green whole-crate
`cargo clippy --all-targets -- -D warnings` run. The current shared-tree strict
Clippy status, including concurrent candidate-writer holds and the clean
arena-only lane, is recorded in the bounded-metadata follow-up above.

The non-`Copy` ownership token closes the demonstrated double-release hole but
has no implicit `Drop`: retirement needs mutable access to its external arena.
The shared event/forest persistent sequence and leaf batches now use one
generic arena build transaction/cleanup journal. Forced mid-build, mid-splice,
invalid-range-with-replacement, corrupt-old-root, and page-N allocation failures
all reclaim their working owners while preserving the old root. The
representation-neutral `RecordForestCandidateTransaction` now spans the first
streamed component leaf through manifest commit, and its binomial-carry
sequence builder keeps temporary owners logarithmic. Real parser sinks and
bounded presentation construction still need to route through this path; see
`CANDIDATE_TRANSACTION_GATE.md` for failure and memory receipts.

## Independent reproduction commands

Run from `tool/parser_research/v3_runtime_slice`:

```sh
cargo test --test source_lineage
cargo test --release --test source_lineage \
  ten_thousand_record_snapshot_survives_ring_overwrite_with_bounded_scalars \
  -- --nocapture
cargo test --test arena_retirement
cargo test --test coordinator
cargo test --release --test event_tape -- --nocapture
cargo test --doc
cargo test --release --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo tree --edges normal
RUSTC=/Users/dan/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
  cargo check --target wasm32-unknown-unknown --all-targets
```

The explicit `RUSTC` selects the rustup standard library containing the WASM
target; this workstation's default Homebrew `rustc` has no WASM standard
library. The raw WASM compile is green.

## Decision and remaining gate

Preserve and reuse these green generic mechanisms:

- explicit non-copyable arena ownership/adoption;
- generation-safe query IDs and iterative retirement;
- bounded packed page builder plus non-copyable mid-line continuation;
- persistent projection checkpoints with real page/build leases;
- balanced arena sequence, byte/UTF-16 prefix sums, immutable splice, exact
  suffix leaf/subtree reuse, and composite manifest ownership; and
- transient self-relative stamps where ordered writes/repairs actually need
  them.

Do **not** promote the full structural event history to selected primary output.
The next challenger should stream exact parser events transiently into a
compact finalized representation:

- immutable finalized block stubs/terminal pages written once per completed
  block;
- an O(open depth), content-free overlay for currently open blocks and viewport
  projection;
- source-backed logical-run/coverage cursors kept in their compact dedicated
  directories rather than repeated per-line output events; and
- focused terminal, reference, page-order, and lazy-repair indexes only where
  a query or invalidation proves they are needed.

That shape retains the successful suffix identity and worker-lifetime model
while making a 10 MiB open paragraph/fence cost proportional to finalized
structure plus open depth, not 200,001 historical mutations. It must now prove
the same 65,536-page prefix insertion, distant viewport, list-repair ordering,
and zero-Crop lifetime receipts before the block grammar attaches.

This result still does **not** prove block grammar correctness, legal restart/
convergence, compact finalized-output sufficiency, reference ordering, inline
composition, or live-editor latency. It does narrow the primary-output choice
with executable evidence rather than taste.

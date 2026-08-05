# Candidate transaction and streaming-build gate

Status: **mechanism GREEN; parser composition still open** (2026-07-15).

This gate is representation-neutral. It does not choose plain preorder,
balanced-parentheses order, or the per-container direct-child challenger. It
proves the lifetime and temporary-memory substrate all of those candidates
need:

- one `RecordForestCandidateTransaction` owns every allocation from the first
  encoded component leaf through the final composite manifest;
- one shared `ArenaBuildTransaction` journals ownership, reuses released
  journal slots with generation-checked handles, and rolls back on drop;
- `StreamingSequenceBuilder` uses binomial carry, retaining at most one subtree
  per power-of-two leaf count instead of collecting all leaf owners; and
- commit builds component wrappers and the component tree inside that same
  rollback boundary.

## Forced cancellation and failure coverage

The private unit gate cancels after every component boundary, fails the second
leaf of a later component, and injects failure after each of the eight commit
allocations in a four-component candidate (four wrappers, three pair nodes,
and the final manifest). Every case retires to zero live arena nodes.

Additional shared-sequence tests force mid-build and mid-splice allocation
failure, invalid range with nonempty replacement, and a corrupt old root with
incoming replacements. Replacement/working owners are reclaimed and the old
valid root remains queryable.

Run:

```sh
cargo test --lib candidate_ -- --nocapture
cargo test --lib persistent_sequence::tests -- --nocapture
```

## 100,000-block streaming receipt

The candidate lazily emits packed record, order, and total-coverage leaves. It
does not collect payloads or owners in the candidate API.

```text
candidate_stream blocks=100000 arena_nodes=6945
arena_payload_bytes=14246320 slot_capacity=8192 slot_storage_bytes=655360
accounted_retained_bytes=14901680 accounted_bytes_per_block=149.01
caller_page_scratch=5376 max_leaf_buffer=4092
max_stream_roots=11 stream_bin_slots=16 stream_bin_bytes=384
max_live_owners=14 owner_journal_slots=14 owner_journal_capacity=16
owner_journal_bytes=512
```

The retained 14,901,680-byte figure accounts for all live arena payload bytes
plus the arena's allocated slot-vector capacity. It excludes allocator headers
and size-class rounding for 6,945 individually boxed payloads, the `PageArena`
value itself, source/Crop and run directories, presentation facts, the
coverage-order oracle, and host/UI state. Those exclusions mean it is an
accounted retained-heap receipt, not a claim about process RSS.

Temporary figures report allocated capacities, not only logical lengths: the
largest fixture-side typed page vector is 5,376 bytes; the largest encoded leaf
buffer is 4,092 bytes; streaming bins allocate 384 bytes; and the complete
transaction journal allocates 512 bytes. Iterator/framework and allocator
headers are not measurable through stable Rust here, but there is no hidden
document-sized owner or payload vector in the candidate implementation.

## Remaining integration boundary

The older `RecordForestManifest::build` remains as a compatibility path for
already-built components. Production extraction must route real parser sinks,
bounded presentation construction, and whichever structural representation is
selected through `RecordForestCandidateTransaction`; otherwise integration
could rebuild the pre-transaction gap outside this proven path. Typed parser
convergence and fresh-parse equality remain independent RED gates.


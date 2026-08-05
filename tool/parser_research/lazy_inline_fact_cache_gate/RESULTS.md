# Lazy inline fact-cache results

Date: 2026-07-15. Host: Apple M1 Pro, arm64 macOS 26.2, Rust 1.93.1.

## Decision

**Proceed with a complete incremental block/source spine and a lazy exact
inline layer.** Correctness did not require retaining full-document inline
facts or full-document inline dependency lists. This is now the leading
large-document shape regardless of whether the final block core is Flark-owned
or donor-derived.

The bounded Comrak inline facade remains compelling: it gives definitive
CommonMark/GFM inline semantics at demand time, while the cache makes its high
ordinary-Markdown fact density local to the viewport. There is no evidence here
that writing a custom inline parser would improve the architecture.

The red-team structural-context gap is closed: bytes and content generation are
not a complete inline cache identity. Each version now also carries exact
block-owned inline context. The same bytes cannot reuse a task marker after
moving out of the first paragraph of a list item, nor can an ordinary paragraph
hide a newly valid task marker after moving into that certified role.

## Native receipt

The ordinary leaf is 96 source bytes and includes emphasis, strong emphasis,
code, and a defined reference consumer. A 40-leaf viewport uses 20 leaves of
overscan on each side and one actively edited leaf outside the viewport.

Representative warmed runs:

```text
lazy-inline-fact-cache native receipt
document source_bytes=10485704 leaves=106997 descriptor_pages=418 descriptor_bytes=1564942 build_us=1625
initial eager_cache_entries=0 eager_facts=0 eager_projection_facts=0
window desired=81 queued=81 active_outside_visible=true schedule_ns=16542 parsed=81 adopted=81 parsed_bytes=7776
latency parse_p50_ns=3708 parse_p99_ns=11792 adoption_p50_ns=125 adoption_p99_ns=167
density facts=1620 facts_per_leaf=20.00 facts_per_kib=213.33 protocol_bytes=35802 protocol_per_source_byte=4.60
retained cache_entries=81 cache_bytes=77637 byte_cap=98304 facts=972 projection_facts=648 payload_bytes=1296 dependencies=81
scroll queued=80 parsed=80 adopted=80 evicted=51 cache_entries=110 old_source_visible=true
synthetic_100mib coverage_bytes=104857548 leaves=1069975 descriptor_pages=4180 descriptor_bytes=15648554 build_us=4392 eager_inline_facts=0
latest_wins collapsed=25 queue=26 stale_adoption=StaleRevision
references value_only_cache_hit=true presence_change_cache_miss=true invalidated_cached_leaves=1 document_consumers_enumerated=0
```

Across two immediately repeated warmed runs:

- window scheduling was 16.5-23.8 microseconds;
- Comrak parse p50 was 3.71-3.83 microseconds per leaf;
- parse p99 was 10.0-11.8 microseconds per leaf;
- cache adoption p50 was 0.125 microseconds;
- adoption p99 was 0.167-0.208 microseconds; and
- building the descriptor-only 100 MiB scale took 4.4-4.7 milliseconds.

These are workstation feasibility numbers, not floor-device SLAs. Adoption is
a move of the already-compact fragment vectors, not a fact-by-fact object
construction pass.

### Retained shape

The 10 MiB document keeps:

- 10,485,704 source bytes;
- 1,564,942 accounted descriptor/index bytes; and
- zero eager inline facts before viewport demand.

After parsing 81 requested leaves, the cache retains 77,637 accounted bytes:
972 semantic facts, 648 projection facts, 1,296 payload bytes, and 81 reference
dependencies. The cache's hard byte cap is 98,304 bytes. A distant scroll
evicts 51 cold entries on byte pressure and settles at 110 of the possible 128
entries; an evicted old leaf remains readable from source.

The 100 MiB synthetic scale has 1,069,975 stable leaf descriptors in 4,180
bounded pages, retains 15,648,554 accounted index bytes, and retains no source
payload or inline facts. This is descriptor/index scaling evidence, not a 100
MiB full-parser benchmark.

The exact structural-context array costs two bytes per leaf plus one boxed-slice
header per descriptor page in this deliberately simple representation. It adds
220,682 accounted bytes at the 10 MiB shape and 2,206,830 bytes at the 100 MiB
shape. This is prototype-only duplication: production block-output pages
already have to own leaf kind and the task-list structural certificate, so they
should feed that existing compact field/token into `LeafVersion` rather than
allocate a parallel context array. If needed, the finite valid contexts also
fit in one packed byte. The cache-entry body itself does not grow on this target
because the context occupies existing `LeafVersion` padding.

Accounting includes vector capacities, fixed cache entry slots, fact arrays,
payload capacity, dependency records/labels, descriptor bodies, `Arc` counters,
and Fenwick prefix arrays. It excludes allocator headers, size-class slack, and
RSS. It is a deterministic retained-body estimate.

### Fact density

The visible ordinary window produces 20 facts per leaf, 213.3 facts/KiB, and
35,802 protocol bytes for 7,776 source bytes: 4.60 protocol bytes per source
byte. This confirms the earlier concern: eager retention scales badly even
though parsing is fast. It also shows why lazy retention is sufficient—the
same dense representation is only 77.6 KiB when scoped to the live window.

## Raw WASM receipt

The raw Wasm export builds and adopts a complete 64-leaf window with the same
real Comrak service. A separate behavior probe returns mask `31`, proving in
Wasm that exact adoption, value-only reference reuse, presence invalidation,
stale-revision rejection, and certified-task-to-later-paragraph withdrawal and
reparse all succeeded.

Representative warmed run:

```text
backend=raw-wasm visible_leaves=64 p50_ns=245166 p99_ns=421542 memory_before_bytes=1245184 memory_after_behavior_bytes=1376256 memory_after_warmup_bytes=1376256 memory_after_samples_bytes=1376256 behavior_mask=31 checksum=13760682291200
```

Repeated runs placed p50 at 245-257 microseconds and p99 at 422-468
microseconds for the complete 64-leaf build/parse/adopt probe. Linear memory
grew once from 1.19 MiB to 1.31 MiB for the behavior probe and did not grow
through warmup or 100 measured window probes. Node runtime/worker transport is
not included.

## Correctness and scheduling receipts

Eight Rust tests (one directory unit plus seven integration tests) prove:

1. page-relative descriptor metrics reconstruct absolute source positions;
2. the 10 MiB/100 MiB shapes start with zero eager inline facts;
3. pending and evicted leaves remain source-visible;
4. active work outside the viewport is first in the queue;
5. local edits preserve stable leaf ID, advance content generation, reject a
   stale service preflight, and reject an already-completed old revision;
6. replacing the requested window collapses the previous bounded queue;
7. cache retention never exceeds its configured byte/entry bounds;
8. reference value-only changes reuse cached structure;
9. presence changes invalidate only a cached leaf when that leaf is queried;
10. a never-parsed consumer retains no dependency and parses against the
    current snapshot only when requested; and
11. dependency generation is checked again between parsing and adoption;
12. byte-identical source moving through ordinary paragraph, certified first
    list-item paragraph, later list-item paragraph, heading, and table cell
    invalidates the old context before parse and reparses only when scheduled;
13. exact task facts exist only for the certified first list-item paragraph;
    and
14. an in-flight completion is rejected when only structural context changes.

The Comrak kernel remains atomic. Latest-wins means queued work is collapsed,
in-flight completion is tagged/rejected, and the next current job wins; it does
not claim to interrupt Comrak midway through one bounded leaf.

## Assumptions challenged

### Content generation is not a complete leaf version

Task-list recognition is intentionally phase-coupled: the exact block core
certifies whether a paragraph is the first paragraph directly under a list
item, then the real Comrak inline scanner decides whether its leading decoded
text is a task marker. Identical bytes in a later paragraph, heading, table
cell, or ordinary paragraph have different semantics.

The cache therefore keys and adopts on stable ID + content generation +
`LeafInlineContext`. A lookup with a changed context removes the old fragment
immediately and exposes exact source while pending. Context changes enqueue no
eager work; the visible/active scheduler reparses them on demand. This avoids a
second grammar while closing the stale-task-fact failure mode.

### A document revision is not a cache-validity key

Invalidating every cached leaf on every document revision would make a prefix
edit or reference-value edit unnecessarily cold. Revision is instead an async
adoption fence. A retained entry is valid when its stable leaf ID/content
generation and block-owned inline context still match, and when every recorded
reference presence generation still matches. This lets unchanged leaves
survive unrelated revisions without accepting stale completions.

### A global inline consumer index is not required

Cached fragments already carry their small, deduplicated hit/miss dependency
sets. Validation examines only a requested cached leaf. Missed leaves carry no
inline state at all and remain source-visible; their eventual parse consults
the current reference snapshot. A presence change needs the UI/worker to
reschedule the known visible window, not enumerate hidden document consumers.

This does not eliminate the block spine's global definition occurrence/winner
index. It eliminates a second eager index of every inline consumer.

### Revision-safe source visibility is a real fallback, not an exact style

Raw source is always correct editor content, so pending/evicted leaves need no
predictive inline parser. It is visually degraded until the exact result is
adopted. On this workstation the whole 81-leaf demand set is sub-millisecond in
parser work, but transport, layout, paint, a floor device, giant leaves, and
backlog pressure determine whether that degradation is ever perceptible.

### Denser wire is not the next bottleneck for visible-window adoption

No SoA/delta/varint rewrite was added. The current window protocol is only
35.8 KiB, retained cache is 77.6 KiB, and native adoption is 0.08-0.13
microseconds because vectors move without per-fact decoding. A denser format
could reduce the 20-byte fact arrays, but it would not materially improve this
in-process visible-window adoption result.

Revisit packing only after measuring actual isolate/Web Worker transfer and
Flutter-side decoding. It remains important if facts are copied across that
boundary rather than transferred/shared.

### Cache work can stay simple because its cardinality is hard-bounded

The prototype deliberately uses compact vectors and linear lookup/sorting
inside a maximum 128-entry cache/roughly 81-leaf demand set. It does not build a
document-sized graph of cache-entry objects. Window scheduling remains tens of
microseconds. A production hash/index is optional evidence-driven tuning, not
an architecture prerequisite.

## Limits

- The directory is a uniform synthetic paragraph corpus. It derives initial
  stable IDs from ordinal and stores one `u32` content generation per leaf.
  Real block-spine pages must supply mixed kinds, origins, inserted/deleted
  IDs, and variable lengths. The executable context-history tests mutate one
  exact leaf through mixed roles; they do not prove that the block parser emits
  the right roles.
- The cache trusts the exact block spine's structural certificate. If the spine
  misclassifies a later paragraph as the first direct paragraph under a list
  item, Comrak will exactly parse the wrong request. The block-core structural
  differential remains the source of truth for that certificate.
- The executable local edit is same byte/UTF-16 length. The separate relative
  output reuse gate proves general length-changing persistent splices; the two
  representations are not integrated yet.
- The paged directory uses a mutable Fenwick prefix index for the current
  revision, not the final persistent block-output index.
- Reference tests use a tiny fixed set of pre-interned labels. Unknown labels
  share a research fallback ID; production requires the collision-free
  document symbol interner already specified by the inline gate.
- Value-only reuse proves structural cache validity, not the renderer's final
  symbol-value lookup or repaint.
- Presence validation is O(dependencies in the requested leaf), bounded by the
  inline service's dependency ceiling. It is not constant-time.
- The cache stores Comrak's current `InlineFragment` vectors. It has not yet
  composed the real segmented logical-to-physical origin map or a Flutter wire.
- Ordinary 96-byte leaves are favorable atomic units. Dense 8-64 KiB leaves,
  over-cap deferral, IME bursts, paste storms, and pathological fact ceilings
  still need queue/backpressure gates.
- The Wasm measurement runs synchronously in Node. It proves compiled behavior
  and parser/cache cost, not Web Worker messaging or browser frame isolation.
- Cache/document destruction and old-revision source retirement are not
  cooperatively metered here. They must remain off the UI thread.
- No Dart isolate, FFI, Web Worker, layout, paint, scroll velocity, or physical
  device is measured.

## Direction

Adopt lazy inline facts as a hard architecture contract:

- the block/source spine and structural renderer facts are complete and
  persistent;
- exact inline facts are demand-driven and disposable;
- the visible set, overscan, and active edited leaves are the only urgent work;
- source visibility is the fail-closed pending/eviction state;
- async completion adoption uses revision + complete leaf version + window
  epoch, and cache lookup uses the same version including block-owned inline
  context;
- cached reference structure validates per-symbol presence generations; and
- cache limits are expressed in bytes as well as entries.

The next gate should wire this controller to the real incremental block-output
pages, segmented origin maps, and native/Web Worker transport. Measure
edit/scroll request through Flutter/Web layout and paint, including stale
bursts and floor-device tails. Do that before spending time on a denser fact
wire or replacing the bounded Comrak inline service.

# Dart source and worker revision partition

Status: working architecture contract, 2026-07-15. This records the smallest
source model consistent with the executable Dart, Crop, transport, and Flutter
cost probes. The one-current-root host gate is recorded in
[`dart/CURRENT_ROOT_INVERSE_SOURCE_RESULTS.md`](dart/CURRENT_ROOT_INVERSE_SOURCE_RESULTS.md).
It is not a launch benchmark and does not supersede RFC 023.

## Decision

Keep exact interactive source on the Flutter UI isolate, but do not keep the
whole document in one `String` or `TextEditingValue`, and do not retain an old
Dart tree root as the undo/history representation for every edit.

The leading ownership split is:

```text
Flutter UI isolate
  one current UTF-16 piece/sum-tree root
  active input island + caret/selection/composing range
  fixed byte/entry-bounded inverse transaction ring
  at most one provisional bulk-base lease + ordered intent journal

native isolate / Web Worker
  Crop immutable UTF-8 source revisions
  certified UTF-16/UTF-8/line/hash index pages
  parser-job revision leases and retirement
```

The first challenger is now green on the host: keep the already-exact functional
object-tree update algorithm, but retain only its one current root and inverse
transactions. With exactly the same node allocations, removing historical roots
reduced retained nodes by about 20x in the composed large-source trace and
removed the control's repeated 11-16 ms GC tails. The remaining functional
allocation stayed below one millisecond at p999 in every host lane. Therefore
functional object updates, not in-place mutation, are the leading source model.
If source-attributable floor-device p99/p999 misses remain, challenge it with a
complete in-place implementation behind the same API. Persistence remains
mandatory only where concurrent worker jobs actually need snapshots. Undo is a
new forward edit to both sides; it does not restore an old Dart root.

The packed typed-page Dart tree remains a credible bounded-allocation fallback
if the one-current-root object tree misses floor-device source-attributable tail
limits and an in-place version either fails or is not maintainable. A
worker-canonical source remains a later fallback if all Dart representations
fail or if cross-runtime ownership becomes simpler than maintaining exact Dart
coordinates. Neither fallback should be paid for pre-emptively.

## Three revision states

Do not expose one ambiguous `revision` that implies all work is complete.

### `UiRevision`

The latest exact UTF-16 text accepted by the editor. It owns:

- exact code units and source spelling;
- active anchors, caret, selection, and IME composition;
- bounded synchronous range reads and local edits; and
- the ordered transaction journal needed to replay unacknowledged edits.

An ordinary scalar-valid edit can advance this revision synchronously. A bulk
input can also advance the logical UI revision as an explicitly provisional
piece: its UTF-16 extent and contents are exact, while validation and global
summaries may still be pending.

### `CertifiedSourceRevision`

The latest source state the worker has validated and indexed. It owns:

- exact scalar validity;
- UTF-8 lengths and coordinate mapping;
- logical CRLF/lone-CR line summaries and content fingerprints;
- the corresponding Crop root; and
- a replay barrier identifying every UI intent incorporated into it.

Parser jobs may start only from this state plus an ordered, validated edit
batch. A stale certification reply cannot roll back or rewrite the UI source.

### `FactRevision`

The source certification and parser profile from which a block/inline/layout
delta was derived. Fact adoption requires an exact match to the current
accepted source lineage and the relevant structural-context generations.
Source, caret, selection, and composing paint never wait for this revision.

This separation permits semantic latency without optimistic grammar. Pending
facts mean exact source-visible presentation, not guessed Markdown.

## Ordinary edit contract

The host-informed provisional routing cap remains at most eight operations and
8 KiB total replacement UTF-16 payload. The cap is internal and must be
recalibrated on floor devices; it is not a document-size limit.

Within the ordinary lane:

1. validate operation ordering, ranges, and scalar boundaries;
2. capture the bounded inverse transaction before mutation;
3. replace the one current sum-tree root using `O(log pieces)` path work; the
   first challenger may allocate immutable path nodes, while a reproduced
   allocation-tail failure selects the in-place variant;
4. transform the small active-anchor set directly from the edit descriptors;
5. append one revision-tokened UTF-16 intent to the worker journal; and
6. paint the active source island on the next frame.

The hot path must not:

- materialize the whole document or a document-scale `TextEditingValue`;
- retain the previous Dart tree root;
- compute or compare a payload above the routing cap;
- encode a large replacement to UTF-8;
- wait for Markdown parsing, worker acknowledgment, or global line indexing;
- synchronously reclaim an unbounded detached tree/backing; or
- create a stale-work queue that grows with typing rate.

The existing synchronous whole-document `markdown` getter may remain as an
explicit cold compatibility materialization before launch, but no editor,
command, renderer, selection, or save hot path may call it. Large-document
applications need bounded range reads plus asynchronous/streaming open, copy,
and export APIs. The package must either document the getter's `O(document)`
cost unambiguously or replace it with an API that cannot be mistaken for a
jank-safe operation; caching one whole `String` would merely reintroduce
document-scale edits.

Typing and IME edits are grouped into inverse transactions. History has both a
byte cap and an entry cap. A single oversized transaction may remain as the
immediate undo item, but the next committed transaction evicts it if it still
exceeds the byte budget.

## Bulk input contract

Inputs above the ordinary cap route before validation, equality comparison,
UTF-8 encoding, hashing, or chunking.

An arbitrary Dart `String` can contain unpaired surrogates, so large input
cannot honestly become a fully certified Markdown revision without reading
it. The UI may adopt it as an exact provisional UTF-16 backing piece so it can
echo, select, delete, and undo immediately. The last certified worker revision
remains the only base for authoritative parser facts.

The worker validates the latest live piece sequence in bounded pages, not the
original backing in isolation. Therefore a user may paste malformed UTF-16,
delete the malformed unit before validation reaches it, and certify the final
valid sequence. Superseded and cancelled replies are rejected by request,
base, logical-revision, and live-piece identity.

Certification returns bounded transferable index pages. Promotion attaches
those pages to the corresponding live backing through one tree path; it does
not scan or rebind the rest of the document. Promotion failure leaves the UI
source exact and provisional and leaves the prior certified revision intact.

Initial file open and hot paste are distinct:

- Hot paste preserves exact incoming CRLF/lone-CR spelling immediately.
- A large initial open should also preserve source spelling. Compatibility
  normalization, if retained at all, is an explicit staged import transform;
  normalized offsets cannot be claimed before the transform is complete.
- On web, a file or `Blob` already available to the worker should be read
  there. A main-owned large JavaScript string is streamed in bounded chunks;
  one giant structured-clone call is prohibited.

### Pre-launch newline contract decision

This intentionally challenges the current v2 public behavior.
`FlarkDocument.fromMarkdown` normalizes CRLF and lone CR to LF at ingest, and
the v2 tests document the normalized document as the source of truth. The
current v3 source implementation and its differential tests instead preserve
the original code units while treating CRLF and lone CR as logical line
breaks.

For the large-document, source-first product, make preservation the v3 default
before launch. It avoids a document-scale synchronous import transform,
preserves file/export fidelity, and keeps UI offsets truthful immediately.
Line-oriented commands must consume logical-line APIs rather than depend on LF
normalization. Offer an explicit normalizing import/export transform for
applications that need the v2 behavior; do not hide normalization inside the
source constructor. If compatibility is ultimately chosen instead, large
normalized input remains provisional and noncanonical until the worker returns
the exact transformed text and offset map. There is no honest constant-time
normalized adoption path.

## Active Flutter input island

Flutter's stock whole-string mutation cost scales with total text and is
already disqualifying at large sizes. The editable control therefore receives
only a bounded island around the active selection/composing range.

The island contract requires:

- stable mapping between island-local and global UTF-16 offsets;
- enough exact neighboring source for grapheme, word, line, and IME behavior;
- expansion or handoff before a command crosses an island boundary;
- no island replacement while an IME composing range is active unless the
  same logical text and composing offsets are preserved exactly;
- cross-island selection coordinates remain synchronous, while copying a
  large nonresident range may be asynchronous; and
- source paint remains available when Markdown facts or cold source pages are
  pending.

The island is a text-input and layout shard, not a semantic Markdown crop.

### Executable island-handoff evidence

The v3 Flutter controller now owns global selection/composition separately
from its bounded `TextEditingValue`. Its bulk lane plans the post-edit island
against a virtual old-source/replacement view before mutation, adopts the large
replacement as provisional UTF-16, and reads back only the selected bounded
range. Boundary planning does fixed candidate work and will not split a valid
surrogate pair or CRLF. A selection base may remain outside the island; the
global extent drives parser queries while Flutter receives a local collapsed
proxy. An active composition may move only when its text and global range are
unchanged.

Focused evidence covers a 100,000-code-unit paste with zero foreground UTF-8
encoding, a 64-code-unit resulting editable value, direct routing of Flutter's
typed insertion/replacement/deletion/non-text deltas, global cross-island
selection, exact IME handoff, and side-effect-free rejection when composition
cannot fit. This closes the mechanism gap, but not the platform gate: a real
`DeltaTextInputClient` connection, selection overlay/commands, and native/web
floor-device frame and GC traces remain required.

## Worker replay, restart, and undo

Every source intent carries:

```text
base certified revision
logical UI revision
ordered UTF-16 operations
transaction/request identity
optional provisional backing identity
```

The native worker can receive immutable strings cheaply on measured Dart VMs,
but the protocol must not rely on that behavior on web. Both transports expose
the same bounded page/edit semantics.

The worker acknowledges the longest incorporated intent prefix. The UI drops
only that prefix from its journal. On worker restart it provides a replayable
certified base (or source provider) and the remaining ordered journal. Undo
creates and journals the exact inverse as a new UI revision; an acknowledgment
of the original edit cannot erase it.

No parser result is adopted merely because its integer revision is numerically
recent. Adoption checks source lineage, content/fingerprint commitments,
profile, and structural context.

## Memory and reclamation invariants

- The UI owns one current root, not a retained root per history entry. An
  immutable update may briefly hand off old/new roots, but old path nodes must
  become reclaimable immediately after the transaction publishes.
- Inverse history is charged by retained backing bytes and entries, not just
  transaction count.
- A provisional bulk operation may retain one explicit base lease until
  promotion, cancellation, or replacement; the lease is never implicit.
- A high-ratio deletion schedules bounded survivor compaction so a tiny live
  piece does not indefinitely retain a giant backing.
- If packed storage is selected, node/free-list replenishment runs outside the
  input handler with low-water backpressure. Large backing compaction is always
  scheduled outside the input handler.
- Worker Crop roots are leased by revisioned jobs and retired independently of
  Dart undo history.
- Persistent parser facts and output pages retain no Dart backing or retired
  Crop root; current absolute ranges are reconstructed from relative facts.

## Decisive executable gates

The host ownership gate has run using the smallest one-current-root/no-old-root
challenger and the existing exact object tree. It compared the same functional
allocation trace against equivalent old-root history, the older immutable-root
candidate, and the packed fallback. It exercised:

1. functional-path balanced-tree edit/sum repair for one current root, with
   separate allocation, graph reachability, and backing-retention receipts;
2. fixed byte/entry-bounded inverse transactions with typing and IME grouping;
3. exact random differential edits, Unicode scalar boundaries, CRLF/lone-CR,
   UTF-16/UTF-8 mapping, anchors, undo, and undo-as-forward-edit; and
4. 10 and 100 MiB active, cold-random, scattered-batch, and 10,000-edit churn;

All four are green as host architecture evidence. Verbose-GC receipts attribute
the old-root control's long tails to retained paths. The host result selects the
functional current-root object model; it does not select or require an in-place
tree.

The remaining source/product gates are:

5. 10/100 MiB provisional adoption, edit/undo before certification, malformed
   surrogate deletion/retry, supersession, worker restart, and base release in
   the new inverse-history composition;
6. bounded bulk-delete inverse payload leases, backing reclamation, and
   high-ratio deletion compaction; and
7. a real Flutter active-island parser-to-paint trace with frame, GC, IME, and
   allocation telemetry on floor native and web devices.

Adopt the immutable-current object tree if source-attributable p99/p999 stays
inside the input budget once old-root history is removed. Adopt in-place
mutation only if it materially fixes a reproduced functional-allocation tail;
adopt the packed arena only if it materially fixes a reproduced mutable-source
tail. Reopen worker-canonical ownership only if all three fail or if ordinary
APIs become infected by provisional reconciliation.

## Evidence boundary

Current host probes demonstrate document-size-independent ordinary source
edits, a one-current-root object tree with inverse-only history, constant-time
lazy bulk adoption, exact candidate/certified histories, bounded packed storage,
and the need for an active input island. They do not yet prove the combined
inverse-history/provisional-base lifecycle, a document-scale delete lease,
physical-device frame deadlines, browser Worker transfer, or Flutter IME
handoff. In-place mutation is now a conditional fallback, not missing required
work. The remaining items stay explicit acceptance gates rather than assumptions
hidden in RFC prose.

# RFC 029: Large-document source, index, fragment, and viewport architecture

**Status:** PROPOSED — architecture selected for risk-first prototyping;
production cutover is forbidden until the source-admission and incremental-index
experiments in section 11 pass. 2026-08-16.

**Reading rule:** this RFC records the large-document architecture and its
decision history. The active product bar lives in
[NORTH_STAR.md](../../../NORTH_STAR.md), the measured handoff bar in
[DOGFOOD_MILESTONE.md](../../../DOGFOOD_MILESTONE.md), and test policy in
[live_editor_test_strategy.md](../../testing/live_editor_test_strategy.md).

**Amends:**
[RFC 026](rfc_026_flark_v4_product_architecture.md),
[RFC 027](rfc_027_continuously_rendered_markdown.md), and
[RFC 028](rfc_028_source_authoritative_edit_transactions.md).

**Scope:** large-document loading, source ownership, parse/index storage,
post-edit maintenance, viewport materialization, Flutter virtualization, and
the performance gates that join those layers. The package boundary, Rust-only
Markdown authority, exact-source model, and live-edit profiles remain unchanged.

## 1. Decision

Flark will not require any document-sized semantic representation before a
useful viewport can be displayed or edited. The selected architecture is:

```text
bounded UTF-8 admission / exact persistent source
        |
        +--> one primary GFM parser
                 |
                 +--> sparse persistent checkpoint/dependency index
                 |       (no document event Vec; no document Green root)
                 |
                 +--> bounded certified semantic fragments on demand
                            |
                            +--> one runtime product mapper and ABI envelope
                                      |
                                      +--> source-anchored virtual viewport
                                                |
                                                +--> bounded Flutter layout tiles
```

Recursive Green remains the exact generic syntax representation inside bounded
hot fragments. It is not a whole-document readiness gate, a document-wide row
model, or the revision's durable storage format.

The compact index is persistent across source revisions. An ordinary local
edit reparses from the nearest valid predecessor checkpoint until complete
parser, writer, coordinate, and dependency convergence, then structurally
shares the unchanged suffix. It does not restart a document scan from BOF and
call that incremental because each pump is small.

The Flutter surface is one continuous source-anchored virtual document. A
32-row result page may remain an ABI/cache unit, but a page boundary is never a
scroll, selection, accessibility, or layout boundary.

## 2. Product objective

The controlling objective is:

> High-performance initial load, jankless continuously rendered editing, and
> excellent scaling through the declared large-document envelope.

This decomposes into four independent obligations:

1. **First visibility:** useful BOF content does not wait for complete source
   copying, complete indexing, or a document-sized semantic tree.
2. **Foreground locality:** typing, selection, navigation, layout, and paint
   are proportional to the input event and visible window, not document size.
3. **Revision locality:** ordinary edits update source, index, dependencies,
   and cached fragments in work proportional to the affected/convergence
   region, not the suffix or complete document.
4. **Honest pathological behavior:** valid inputs with inherently unbounded
   semantic dependencies may fall back or wait under a typed contract, but
   never cause an unbounded foreground operation, stale semantic claim, crash,
   or anonymous capacity failure.

Passing one obligation does not imply another. In particular, a fast parser
fragment does not prove a fast public open, and a bounded pump does not prove
bounded total work.

The flagship performance claim is flatness, not a threshold: claim-eligible
receipts show the same open, typing, and edit-locality numbers across the
1 MiB, product, and 4x detector tiers. Interaction targets derive from
physics — next-frame presentation at native refresh, perceived-instant open —
never from peer parity. A peer measurement is taken once at claim time to
document leadership; it does not define the targets.

## 3. Non-negotiable invariants

- Exact valid UTF-8 source in Rust remains the sole mutation authority after
  admission.
- There is one GFM parser and one product semantic mapper. A prefix parser,
  cold parser, Dart scanner, flat-row grammar, or renderer-specific grammar is
  forbidden.
- Every source, index, fragment, semantic, anchor, and layout receipt names its
  revision or load authority. Old semantics are never mapped forward by
  coordinates alone.
- Every synchronous foreground operation has both a work/fact/output cap and a
  measured wall-time cap.
- No cap may convert unknown semantics into an authoritative empty result.
- Background work is preemptible and cannot retain a second complete revision
  while a third is admitted.
- Document size, block density, dependency density, and maximum indivisible
  text unit are separate capacity dimensions.
- A convenience API may be slower than the scalable API, but benchmark and
  product claims name which API was measured.

## 4. Source plane

### 4.1 Public admission modes

`flark_core` will expose three source shapes without depending on Flutter:

1. `open(String)` remains a convenience API. It lazily encodes bounded UTF-8
   chunks on the document worker, cuts only at Unicode-scalar boundaries, and
   must not allocate one complete derived UTF-8 list before native admission.
2. `openUtf8(Uint8List)` admits already encoded bytes without a String-to-UTF-8
   conversion. Ownership/copy behavior is explicit in its receipt.
3. `openUtf8Stream(Stream<Uint8List>, {expectedBytes})` is the scalable
   sequential-open API. At most a bounded ingress window is retained outside
   the Rust source store.

The ABI creation transaction gains an opening-query state. Exact admitted
source and suffix-independent certified rows may be queried before EOF. An
opening receipt carries a load identity, admitted byte/UTF-16 frontier,
declared total when known, and exact coverage. It cannot be confused with a
complete committed revision.

Certification during admission is allowed only when the primary parser proves
that an unreceived suffix cannot change the published public facts. Reference
absence, open containers, Setext/table lookahead, list finality, and comparable
dependencies remain unknown until their required frontier. A candidate that
cannot make that proof exposes no semantic authority.

The experiment determines the exact source-store mechanism. Production may use
append-published persistent Rope roots or a distinct opening builder, but it
must prove that promotion to the committed revision neither copies the complete
source nor reparses an already certified prefix.

Cursor exhaustion at the admitted frontier is not EOF. EOF is a separate,
explicit seal operation. A last unterminated physical line remains provisional
until a terminator arrives or the load seals; the parser cannot close its open
containers merely because the transport is temporarily out of bytes.

The authority model separates three identities:

- a stable load identity for one sequential input transaction;
- a load generation for each newly published immutable prefix root; and
- an edit revision, advanced only by user mutations.

Parser facts from an older generation may survive a later append only when
their complete source coverage is proven unchanged and did not include the
provisional tail. An admitted-prefix edit changes the edit revision and uses
normal source lineage. Root equality, coordinate equality, or shared load
identity alone is never reuse evidence.

### 4.2 Loading and editing are separate axes

A sequential load generation and a user-edit revision are distinct identities.
If the first viewport is advertised as editable before EOF, the source store
must retain an append frontier anchored in the original loading stream while
allowing edits only inside admitted exact ranges. Later chunks attach at that
transformed frontier; they are not positioned by a stale byte offset after a
prefix edit. Every receipt names both the load generation and edit revision.

An implementation that can paint before EOF but must disable editing until the
complete source commits has proved progressive display, not the existing first
*editable* viewport gate. The Experiment A result must state that distinction
and either implement safe edit-during-load behavior or fail the editable gate.

Incremental String-to-UTF-8 validation carries a pending high surrogate across
host chunk boundaries and rejects an unpaired surrogate before committing the
affected source. A transport chunk boundary is never treated as a Unicode
scalar boundary by assumption.

### 4.3 Random access and persistence

Fresh sequential input cannot provide a deep viewport before bytes or an index
near that viewport exist. The API therefore distinguishes:

- fresh sequential open;
- fresh fully resident UTF-8 open;
- reopen with an optional validated persistent index; and
- a future range-readable source adapter.

Persistent index support is not required for the first in-memory cutover, but
the index codec, source page hashes, and identities must permit it without an
architecture change. Cached state is never trusted from path, timestamp, or
length alone.

## 5. Persistent compact index

The index is an immutable page tree with structural sharing. Fixed-size pages
contain selected restart records and compact dependency facts; aggregate tree
nodes carry source-byte, UTF-16, physical-line, and renderable-row measures.
Coordinates after an edit are resolved through tree measures or source-anchor
indirection. They are not rewritten record by record.

Reusable page payloads are coordinate-relocatable. Absolute revision identity,
document row ordinals, source offsets, and globally sequential parser frame IDs
belong in a revision manifest, aggregate measure, or explicit remapping layer;
they may not be baked into every otherwise reusable suffix record. Checksums
authenticate immutable payload plus its revision envelope separately, so
promoting a proven unchanged page does not require re-encoding its contents.

Each restart authority includes everything required to resume the same primary
parser and writer without a Green root:

- source and UTF-16 cut;
- parser, writer, and open-container state;
- prefix row-count summary;
- line-ending and logical-projection state;
- dependency/reference prefix authority;
- parser/profile/codec versions; and
- integrity checksum and source lineage.

### 5.1 Incremental revision update

For one committed source transaction:

1. Transform the affected source range through the authoritative source edit.
2. Select the nearest predecessor checkpoint whose dependency authority is
   valid for the new revision.
3. Re-run the primary parser from that checkpoint.
4. Emit replacement compact pages and candidate fragments under fixed budgets.
5. Compare complete convergence state against later old-revision checkpoints.
6. At the first exact convergence point, splice the unchanged suffix page tree
   into the new revision through structural sharing and an authenticated
   coordinate/frame-identity mapping.
7. If convergence exceeds the foreground cap, publish exact current source and
   continue in background. Do not restart from BOF unless the grammar facts
   genuinely invalidate every prior checkpoint.

Convergence equality includes parser and writer continuation state, open
container identities/properties, line/logical state, row-count delta,
reference-prefix state, profile, and source lineage. Equal local bytes are
insufficient.

### 5.2 Non-local dependencies

Reference definitions use a separate persistent label-to-first-winner index.
An edit updates only labels whose definitions intersect the reparsed region.
Cached fragments retain the small set of winner identities they consumed and
revalidate those identities lazily. Changing one label does not eagerly walk
every reference use or invalidate fragments using unrelated labels.

Other backward edges—list tightness, Setext/table promotion, fence/HTML closure,
and container final facts—belong to parser convergence state. They may extend
the reparse region, but they may not trigger an unmeasured synchronous suffix
rewrite.

## 6. Bounded semantic fragments

A semantic fragment is an immutable current-authority Green slice plus the
complete product facts for a bounded source window. It carries absolute
byte/UTF-16/logical/row bases, authenticated ancestor and final facts,
dependency identities, and explicit covered/pending ranges.

Fragments are cached under a hard allocated-byte limit and keyed by revision,
source identity, parser profile, dependency identities, and coverage. Semantic
fragments do not depend on Flutter width or typography. Layout caches do.

One product mapper converts both complete-session and fragment Green rows.
Tables, inline facts, targets, edit capabilities, source/display coordinates,
and projection policy have no cold-only implementation.

### 6.1 Oversized semantic units

The current 8 KiB all-or-nothing inline-leaf cap and fixed table-fact cap are
not sufficient as the final product architecture. The implementation must
choose, measure, and version one of these outcomes per construct family:

- resumable/pageable semantic facts for a large but supported block;
- a compact per-block inline dependency index followed by bounded fact windows;
- a declared typed pathological fallback.

The ordinary product lane may not satisfy “continuously rendered” by selecting
fixtures whose blocks merely stay below an internal cap. Giant paragraphs,
tables, Unicode clusters, and document-spanning constructs have separate
declared envelopes and receipts.

No implementation can promise final GFM presentation for every arbitrary
prefix before seeing the suffix: a later definition, delimiter, or close can
change earlier semantics. Flark promises no raw-Markdown cold frame for the
declared product lane. Outside that lane it shows an explicit loading/fallback
state rather than a confidently wrong render, and remains editable only to the
degree authorized by exact current source.

## 7. Scheduling and cancellation

One document actor serializes source revisions and semantic publication.
Foreground work classes—input admission, source/caret publication, visible
fragment query, layout payload, and hit testing—outrank background index work.

Every background grant carries a transition cap and a wall deadline. The actor
yields at the first limit. Cancellation and supersession are work classes with
their own bounded receipts; reclamation cannot monopolize a later foreground
call.

Index progress is useful but never a prerequisite for literal source edits in
an already admitted range. Sustained typing may delay EOF publication, but it
must not repeatedly discard reusable prefix/suffix index pages or grow retired
generations.

## 8. Continuous virtual viewport

The framework-neutral runtime/Core contract is source-coordinate based:

- requested source anchor/range and directional overscan;
- exact current source coverage;
- certified semantic rows/fact pages where available;
- pending/fallback coverage elsewhere;
- continuation by source position, never UI page number;
- known prefix rows and optional exact total rows;
- query and dependency receipts.

Flutter owns a globally anchored layout index. It stores measured heights for
hot layout tiles and conservative estimates for unloaded ranges. Height updates
preserve the first visible source anchor and local glyph offset, so
recertification, width changes, and cache replacement do not jump the viewport.

The Flutter implementation uses native `Scrollable`/`ScrollPosition` behavior
and multiple cancellable prefetch windows. It supports direct source-anchor
jumps and saved-position reopen. ABI result pages and cache fragments remain
invisible implementation units; crossing one does not reset scroll offset.

Text shaping is tiled and offscreen layout is absent. A grapheme cluster,
bidirectional run, table row, or word-navigation request that defeats the
normal tile budget has an explicit cap/pathological receipt rather than
silently expanding one frame's work.

Selection anchors remain source-global. Dragging, keyboard extension, Select
All, and accessibility traversal load only geometry near active endpoints.
Copy/export of a large range is asynchronous and chunked through Core even
when the final platform clipboard API ultimately requires a String.

Whole-document queries are a declared feature class, not ad-hoc additions:
find-in-document, word count, outline, spell-check scoping, and export are
chunked background work through Core with bounded foreground slices. No
feature materializes a complete Dart String of the document, and a feature
in this class enters the RFC with its foreground work class and background
contract stated before any UI work.

## 9. Capacity model

Capacity is a vector, not one byte number:

- source bytes and UTF-16 units;
- physical lines and Markdown blocks;
- maximum physical line and grapheme cluster;
- maximum natural block/inline leaf/table;
- reference definitions, distinct labels, and uses;
- nesting depth;
- active fragments/layout tiles;
- anchors/history; and
- concurrent sessions.

The product floor remains 10 MiB ordinary Markdown on macOS, Android, and iOS,
plus the existing 5 MiB hostile density shapes. The leadership envelope is the
largest same-hardware comparable peer result, and the next meaningful tier is
the stretch target. A 4x engine/source-admission tier detects hidden linear
foreground work even when it is outside the public product envelope.

Every dimension has a typed cap or a passing receipt. Admission of source does
not imply that every possible density fits a semantic index budget, but a
density rejection is deterministic, early enough to remain recoverable, and
never an internal fault.

## 10. Frozen performance gates

All timings start at the public host event and include source conversion/copy,
isolate scheduling, FFI, Rust, Core adoption, Flutter layout, paint, and raster
unless explicitly labelled engine-only.

### 10.1 First visibility and indexing

- On the ordinary source-local fixture, first exact editable rendered viewport
  is below 200 ms on every qualified platform; below 100 ms on the development
  Mac is the stretch target.
- From 1 MiB through the product and peer-next tiers, first viewport publication
  consumes at most 512 KiB of admitted source before it becomes ready and does
  not wait for EOF index publication.
- The scalable admission APIs retain at most 2 MiB of derived ingress buffers
  outside the authoritative source and allocate no complete derived UTF-8 copy.
- Ordinary 10 MiB EOF index publication remains below 1 second on Mac and
  3 seconds on the qualified Pixel. Larger tiers report throughput; throughput
  may not regress by more than 20% solely because total size increased.
- Visible current projection is below 500 ms for every product-lane cold-open
  fixture. Pathological lanes use their predeclared fallback outcome and still
  meet the first-visibility/frame gates.

### 10.2 Foreground interaction

- Accepted source, caret, and selection are visible by the next frame with no
  backlog older than one frame.
- Engine foreground p99 is at most 4 ms; Flutter build+raster workload p99 is at
  most 8 ms; no editor-attributed frame or synchronous span reaches 16 ms; and
  editor-attributed dropped frames are zero.
- Those predicates hold while initial indexing, post-edit convergence,
  fragment cache misses, and retirement are active.

### 10.3 Incremental revision locality

- Ordinary convergent edits at BOF, middle, and EOF never start a clean parse
  from BOF unless BOF is the selected predecessor checkpoint.
- Across 1 MiB, 10 MiB, the peer-next tier, and the 4x engine detector, p99
  replay for an ordinary single-character edit is at most 64 KiB of source and
  index-page replacement is bounded by the replayed region plus twice the
  index-tree height and eight edge pages. Neither metric trends with total
  document size.
- The visible affected region recertifies below 100 ms p99 and 200 ms maximum
  on Mac; the 50/100 ms values are stretch targets. Mobile thresholds are
  frozen after the same receipt exists on the qualified device.
- Reference-winner edits record labels examined, definitions replaced, and
  fragment dependencies invalidated. Foreground work may not scale with total
  reference-use count.
- Unbounded-convergence fixtures must return their typed foreground fallback
  under the same frame caps and finish through measured background work.

### 10.4 Viewport mobility

- A saved source-anchor open, direct BOF/middle/EOF jumps, and ten seconds of
  maximum practical physical fling produce zero blank, stale-semantic,
  duplicate, reordered, or page-reset frames.
- End-to-end uncached viewport requests are below 100 ms p99 and 200 ms maximum;
  the Mac stretch target is 50 ms p99.
- Measured height replacement preserves the first visible source identity and
  local glyph position. Page/cache boundaries produce no observable scroll
  discontinuity.
- Resize, text-scale, and width changes remain bounded and anchor-stable.
- Accessibility traversal, summaries, and text-service requests at the
  envelope perform bounded work per step: no platform accessibility or text
  API response materializes complete document text, and per-step work is
  bounded by the visible window plus declared caps.

### 10.5 Memory and lifecycle

- The existing 12 MiB compact-index/reference and 8 MiB hot-fragment caps apply
  at the declared ordinary/hostile envelopes until measurement justifies a
  stricter formula.
- Global peak and retained-RSS gates include caller-visible source, Dart
  conversion buffers, isolate messages, native source, indexes, fragments,
  layout objects, retired revisions, history, and clipboard/export buffers.
- Receipts cover open, sustained edits, index convergence, cache thrash,
  large-range copy/export, close, reopen, and multiple sessions.
- All native/session state reaches zero after close. Retired capacity cannot
  accumulate across revisions or open/close cycles.

## 11. Risk-first proof sequence

No production path is switched merely because individual components compile.

### Experiment A — progressive source admission

Prove this in three ordered cuts; a later cut cannot be claimed from an earlier
one:

1. **A1 source lifecycle:** append-publish immutable exact prefixes, edit an
   admitted range, retain old snapshots, and seal the current root without a
   complete-source copy. Measure first-prefix time, total admission throughput,
   structural sharing, derived buffers, and peak RSS at 1/10/peer-next/4x.
2. **A2 parser lifecycle:** keep the one primary parser/controller open across
   transport starvation without sending EOF. Publish only closed facts with a
   proof that later bytes cannot change their product output. Include a split
   physical line, open fence/container, late reference definition, Setext/table
   lookahead, and a plain closed-block control.
3. **A3 product lifecycle:** route the opening authority through runtime,
   ABI, Core, and the real viewport. Capture the first exact editable rendered
   viewport before EOF, bytes admitted at publication, input-to-paint time, and
   equality with the sealed complete-source oracle after an edit during load.

The complete probe lazily admits the ordinary 1/10/peer-next/4x sources. It
captures derived buffers, copies, peak RSS, full index time, and exact equality
with the complete-source oracle. A late reference and spanning construct must
refuse early certification rather than publish provisional semantics.

Reject the design if ordinary first visibility remains proportional to total
source size, if promotion copies the complete source, or if early facts differ
from the final product oracle. A1 success proves only the source data path; it
does not satisfy the first-viewport gate without A2 and A3.

### Experiment B — persistent index revision locality

Build the compact index for ordinary, nested, table, reference-heavy, and
fallback fixtures. Apply edits at BOF/middle/EOF and record predecessor cut,
source replayed, pages allocated/reused/replaced, convergence state, dependency
changes, foreground grants, and final equality with a clean full parse.

Reject the design if ordinary local edits rewrite/rebase the suffix, restart
from BOF without semantic necessity, or require eager traversal of every
reference use.

### Experiment C — virtual viewport

Only after A and B pass, route fragments through the real ABI/Core contract and
a globally anchored Flutter prototype. Exercise saved-position open, direct
jumps, fling, cross-window selection, resize, Unicode giant units, cache
eviction, and edits above/below the viewport. Compare every painted source
identity and semantic digest with the headless oracle.

### Production cutover

Cut over only when the ordinary first-visibility, revision-locality, mobility,
memory, and current-revision correctness gates pass together. Keep the old full
Green build solely as an offline differential oracle until the compact path's
frozen semantic matrix passes; it is never a product fallback.

## 12. Consequences

This architecture is more work than optimizing the current full tree or
patching page transitions, but it removes their hidden document-sized
prerequisites instead of moving them. It preserves the already proven parser,
source, edit, and semantic machinery and confines new complexity to four
explicit abstractions: bounded admission, persistent compact index, certified
fragment, and source-anchored virtual viewport.

The architecture deliberately leaves optional persistent on-disk indexes and
range-readable sources after the in-memory proofs. Their identities and codecs
are accommodated now, but they do not distract from proving first-open and
post-edit asymptotics.

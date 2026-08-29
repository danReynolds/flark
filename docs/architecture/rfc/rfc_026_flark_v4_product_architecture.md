# RFC 026: Flark v4 product architecture

**Status:** ACCEPTED for implementation. 2026-08-08.

**Decision owner:** product direction agreed in design review.

**Amends:** [RFC 024](rfc_024_bounded_inframe_markdown_engine.md), whose
measurements and design rationale remain evidence. This RFC controls the v4
package boundary, platform scope, and selected implementation direction. The
[v4 build plan](../v4/build_plan.md) controls execution.

**Amended by:**
[RFC 027](rfc_027_continuously_rendered_markdown.md), which makes continuously
rendered editing and a shared read-only render contract the v4 product target;
and [RFC 029](rfc_029_large_document_architecture.md), which replaces a
document-sized semantic readiness model with bounded source admission, a
persistent compact index, certified Green fragments, and a source-anchored
virtual viewport.

**Package-boundary update (2026-08-29):**
[Portable editor architecture](../v4/portable_editor_architecture.md)
supersedes this RFC's public package naming and Flutter-product-only scope.
The destination is a pure-Dart `flark` editor kernel with a separate
`flark_flutter` adapter. Rust remains the source and Markdown authority.

## 1. Product directive

Build a live Markdown editor with breakthrough performance on large documents:

- exact source remains the document truth;
- certified Markdown remains rendered while it is focused and edited; syntax
  appears only as local current-source authoring/fallback, never from focus
  alone;
- typing, selection, scrolling, and composition stay visually fluid;
- expensive Markdown work is resumable and cannot monopolize a frame;
- the screen never presents uncertified Markdown semantics as authoritative;
- performance claims are backed by end-to-end editor measurements, not parser
  throughput alone.

**GFM compatibility** and **live Markdown projection** are both required, but
they are not the same claim:

- the GFM semantic matrix pins the CommonMark/GFM profile and tests the standard
  parsing behavior for blocks, inlines, references, tables, task lists,
  strikethrough, autolinks, HTML policy, and source preservation;
- the live-projection matrix tests how those semantics behave while the user
  types: incomplete syntax, marker reveal/hide, caret and selection movement,
  split/merge/paste/undo histories, pending certification, and transitions back
  to certified rendering.

Both matrices must pass through the incremental product path. Projection tests
cannot substitute for GFM conformance, and static GFM fixtures cannot substitute
for live editing behavior.

Flark is a Flutter product. Its initial platforms are macOS, Android, and iOS;
Windows follows after those are qualified. Work starts on the available Mac.
Mac evidence proves architecture and performance shape, not mobile constants.

“Large” means multiple mebibytes of editable Markdown, at least the largest
comparable envelope demonstrated by the leading relevant editors and ideally
the next meaningful tier beyond it. M0 measures a named competitor cohort on
the same machine, content shapes, fidelity rules, and edit/open workloads. One
MiB is a waypoint, not the ceiling. The public envelope remains provisional
until Flark passes its own end-to-end gates on named hardware.

## 2. Selected architecture

v4 has one dependency direction:

```text
flark                         Flutter editor and rendering surface
  -> flark_core               headless, general-purpose Dart API
       -> flark-abi           thin host-neutral native boundary
            -> flark-runtime host-neutral Rust document/session runtime
                 -> flark-parser
                 -> flark-engine
```

The public package identities at the destination are:

- `flark_core`: a headless Dart package with no Flutter dependency;
- `flark`: the Flutter editor product, depending on `flark_core`.

The existing Rust incremental engine is the selected engine. A whole-document
reparse may remain a benchmark control or falsification tool, but it is not a
second product path, fallback, size threshold, or runtime option.

No reverse dependency and no parallel Markdown implementation are permitted.

Within Rust, `flark-engine` owns source/storage primitives, `flark-parser` owns
grammar and certification proofs, and `flark-runtime` is the sole session
orchestrator exposed to `flark-abi`. The ABI never calls parser or engine
internals directly.

## 3. Rust ownership

The Rust runtime MUST own:

- canonical valid-UTF-8, byte-exact source and monotonically identified
  revisions;
- revision-checked, atomic source transactions;
- bounded reversible-edit payloads identified by opaque transaction tokens;
- CommonMark plus the selected GFM profile;
- block and inline parsing, references, and all Markdown interpretation;
- stable source anchors and their transformation through edits;
- source-byte/UTF-16 conversion against an explicitly named revision;
- current-revision certification, including the dependency knowledge required
  to certify a requested range;
- resumable work, explicit progress, convergence, cancellation, and faults;
- capped source, semantic, anchor, and coordinate-conversion queries.

The Rust runtime MUST NOT contain Dart, Flutter, widget, IME, display-frame,
isolate, typography, or platform-thread concepts. Parser trees, checkpoints,
arenas, candidates, dependency indexes, and recovery machinery remain private.

Source is valid UTF-8. Opening or editing with invalid UTF-8, including a Dart
string containing an unpaired surrogate, fails with a typed error before a
revision is committed. Flark performs no Unicode normalization and preserves
the exact valid UTF-8 bytes and line endings supplied. M0 pins the Unicode
grapheme version/library used by host editing policy and a cross-layer Unicode
fixture set.

Per-range certification is engine architecture, not transport decoration.
Markdown outside an edited range can change the meaning inside it. Therefore a
semantic result is usable only when the parser proves it for the requested
range at the current source revision. Without that proof the host renders exact
source neutrally; it never maps old semantics forward and calls them current.

## 4. Thin host-neutral ABI

`flark-abi` is a bounded access seam, not a second runtime. It exposes explicit
ABI-version and capability negotiation plus coarse, batched operations
equivalent to:

- create a document from supplied UTF-8 bytes/chunks, stream exact source
  chunks, and begin/pump/finalize bounded close;
- apply a small revision-checked source transaction;
- stage, commit, or abort a bounded-chunk bulk transaction;
- pump a bounded amount of work;
- query a capped source/semantic viewport;
- create, transform, and resolve stable anchors;
- convert explicitly typed source-byte and UTF-16 positions;
- inspect discriminated progress, pending, complete, cancelled, and fault
  states.

The runtime does not open filesystem paths or perform platform I/O. Save and
export stream bounded source chunks to the host.

Small edits have a declared maximum encoded size. A larger paste/replacement is
copied through a bulk transaction into Rust-owned staging chunks under bounded
calls, then committed by one bounded rope splice and revision change. An edit
is **accepted** only when commit returns the new revision; before that, the old
revision remains authoritative. Rust never retains an arbitrary host pointer.
The host owns input bytes until a call copies or explicitly transfers them;
Rust owns staged bytes after that call and releases them on commit, abort, or
session close.

A committed edit may return an opaque reversible transaction token. Rust holds
the exact inverse payload under an explicit history-byte budget; it does not
choose grouping or user-facing undo policy. `flark_core` orders/groups tokens
and stores selection snapshots, then asks Rust to apply the token. Large undo
therefore does not require a document-sized deleted-source copy in Dart.

Close is a resumable state machine. Marking a session closing and each release
pump are bounded; the final close receipt requires exactly zero live document,
transaction, continuation, and handle state. A document-sized destructor is
not permitted on the interactive frame path.

The ABI uses opaque generation-checked handles, fixed-width versioned records,
explicit ownership, panic containment, caller-owned buffers where practical,
and hard caps on every returned batch. It has no host callbacks, recursive
trees, serialized object graph, Dart object, Flutter position, or exposed Rust
layout.

Every query result and continuation is bound to a source revision and snapshot.
An edit invalidates or versions older continuations; source and semantic pages
from different revisions cannot be combined.

One session handle has one owner and is non-reentrant. It may migrate to another
thread/execution context only while idle; concurrent calls fail with a typed
status. Cancellation and supersession occur at pump boundaries unless a later
RFC deliberately adds concurrent cancellation.

The ABI is otherwise callable from a host-selected execution context. v4
initially calls it synchronously with a strict foreground budget because that
is the implementation being measured. The boundary does not require an
isolate, and does not prevent a later host from calling the same coarse API from
one. The legacy 62-round-trip measurement rejects that endpoint/publication
protocol; it is not evidence against isolate or worker placement itself. No
executor abstraction is added until measurements require it.

The boundary is designed so another language can bind to document, source,
parser, and certification behavior without refactoring the engine. It does not
promise identical Dart command/history/UI policy. v4 does not promise a
permanently stable third-party C ABI and does not deliver Swift, Kotlin,
JavaScript, or other SDKs. A small non-Dart ABI conformance harness is enough to
enforce host neutrality now; SDK abstractions wait for a real second consumer.

## 5. `flark_core`: headless Dart

`flark_core` owns:

- private native loading and raw bindings;
- safe document/session lifecycle and deterministic cleanup;
- idiomatic Dart revisions, transactions, anchors, budgets, progress, source
  reads, and bounded query results;
- typed, revision-validating wrappers over Rust-authored source-byte/UTF-16
  conversion;
- schedule-neutral pump/query APIs;
- canonical selection state, grapheme navigation/edit policy, source
  transactions, opaque history-token ordering, selection snapshots, and undo
  grouping that do not interpret Markdown or retain inverse document text.

It has no Flutter import. It does not keep an independent canonical source,
parse Markdown, infer certification, or export wire packets, native handles,
parser nodes, checkpoints, or green-tree records.

Commands promising a Markdown semantic outcome MUST use a Rust-authored edit
recipe or Rust runtime operation. Dart may issue literal source edits and route
UI intents, but it may not derive Markdown syntax from parser facts or grow
Dart-side Markdown scanners.

## 6. `flark`: Flutter product

`flark` owns the application-facing editor and rendering experience:

- the custom own-painted `FlarkEditor` and `FlarkMarkdownView`, consuming one
  shared internal projection/layout/paint contract with separate interaction
  machinery;
- frame scheduling and allocation of the core work budget;
- viewport and fragment virtualization;
- text shaping, layout, paint, hit testing, and visual invalidation;
- the bounded platform-input window and composition state machine;
- mapping between bounded input-window UTF-16 offsets and core positions, then
  from core grapheme positions to glyph-cluster and bidi visual geometry;
- caret/selection visuals, gestures, shortcuts/intents, platform undo adapter,
  clipboard UI, toolbar, magnifier, theming, semantics, and accessibility.

Flutter invokes `flark_core` selection, history, and command operations; it
does not maintain a second canonical selection or undo model. Packaging-only
Flutter plugin metadata may bundle native artifacts where a target platform
requires it, but raw bindings, lifecycle, and semantics remain in
`flark_core`.

Focus, selection, and caret movement do not reveal certified syntax markers or
switch an active row to raw source. The surface may expose current exact syntax
only for an incomplete, composing, pending, source-gap, faulted, explicitly
source-only, or future user-selected source range. It may not decide whether
Markdown structure is valid. Pending fallback is limited to the smallest
runtime-authenticated affected range available while unrelated certified
presentation remains current. RFC 027 and `flark-live-v2` define the exact
behavior.

The custom surface is a real architectural layer, but not a third public
package. It stays internally modular until an independent consumer justifies
another package boundary.

## 7. Correctness and liveness invariants

Every implementation milestone preserves these invariants:

1. **One source authority.** Rust owns the source; save/export reads it back.
2. **One Markdown authority.** No Dart or Flutter fallback parser or scanner.
3. **Revision-matched semantics.** Semantic results name a revision and a
   certified source range. Stale or uncertified facts are not painted as truth.
4. **Bounded foreground work.** Every synchronous edit, pump, query,
   conversion, destruction, layout, and paint unit has an explicit cap.
5. **No silent stops.** A progress token changes, work completes, or a typed
   terminal reason is returned. Quiescence is never ambiguous.
6. **Atomic large edits.** Paste and replacement preserve exact source even
   when semantic convergence spans frames. Oversized edits use the staged bulk
   transaction; next-frame visibility is measured from successful commit.
7. **Typed coordinates.** Source bytes, UTF-16 units, graphemes, glyph
   clusters, and visual bidi positions are never interchangeable integers.
8. **Source-stable selection.** Canonical selections use source anchors plus
   affinity, not parser-node identity.
9. **Bounded results.** A viewport query cannot accidentally materialize a
   document-sized object graph in Dart.

## 8. Performance evidence contract

The Mac-first harness measures the whole chain from platform edit through Rust,
Dart, Flutter layout, and raster completion. It records distributions and
outliers, not only averages:

- input-to-visible-source latency;
- foreground work by layer and longest synchronous span;
- Flutter build/raster timings and missed frames;
- convergence latency and uncertified character-frames;
- FFI call count and bytes returned per frame;
- allocations, retained memory, exactly-zero live document/handle state after
  close, and separately budgeted allocator/RSS baseline variance.

The minimum document matrix is 1 KiB, 25 KiB, 100 KiB, 1 MiB, 2 MiB, 5 MiB,
and 10 MiB, extended to the measured competitor boundary and the next larger
tier when necessary. It includes ordinary prose, delimiter-dense text, one
giant paragraph/line, many tiny blocks, nested containers, tables, references,
an open fence, sustained typing, streaming append, and a 32 KiB paste. An
engine-only tier at least four times the selected editor envelope detects
hidden document-sized work.

Provisional Mac design gates are:

- exact source, caret, and selection from an accepted edit are visible by the
  next rendered frame, with no input backlog older than one frame;
- the synchronous engine share stays within 4 ms at p99 for the declared local
  edit matrix;
- profiled Flutter frame workload while typing stays at or below 8 ms at p99
  as a stretch/headroom target;
- no editor-attributed Flutter frame or synchronous foreground span reaches the
  hard 16 ms budget, and no editor-attributed dropped frame is hidden by a
  percentile;
- at the selected multi-MiB editor envelope, the first exact, editable viewport
  paints below 200 ms and its current-revision projection certifies below
  500 ms on the named Mac configuration;
- local-edit foreground work remains bounded as document size grows; and
- every fixture converges or returns a specific, test-failing terminal fault.

The hard product contract is 16 ms; 8 ms preserves headroom and supports faster
displays. Actual frame misses are also evaluated at the recorded refresh mode.
A non-local GFM reclassification may converge over later bounded pumps, but the
typed source/caret/selection is never delayed and stale semantics are never
shown as current; the affected range renders exact source neutrally until
certified. These are Mac development gates, not mobile promises. Simulators may
validate functionality only. Android/iOS latency, thermal, memory-pressure,
IME, and touch claims require physical devices and the same versioned harness.

## 9. Current evidence and honest starting point

The current repository is a strong starting engine, not a finished product:

- all 652 CommonMark 0.31.2 fixtures are structurally admitted;
- the diagnostic CommonMark semantic ledger records 563 exact results, 77
  typed missing capabilities, and 12 explicit divergences;
- the selected GFM profile owns a complete 672-case executable semantic lane,
  currently at 572 exact, 81 typed missing, and 19 divergent; this is a
  complete gap map, not a conformance pass;
- certification currently falls back to whole-document pending rather than
  proving arbitrary current-revision ranges;
- a 32 KiB paste can silently fail to converge;
- the custom-surface prototype has not yet proven the full real-engine path.

Structural admission is not conformance. These numbers remain separate until
the semantic and incremental ledgers close them honestly.

The old endpoint/wire/host-publication integration is not the v4 boundary. Good
low-level patterns such as generation handles, panic barriers, fixed-width
receipts, and bounded buffers should be retained while that architecture is
replaced.

## 10. Migration constraints

The repository currently uses `flark` for the headless Dart package and
`flark_flutter` for Flutter. The destination reverses the ownership of the
`flark` name. As of 2026-08-08 the pub.dev package API returns not-found for
`flark`, `flark_core`, and `flark_flutter`; no hosted compatibility promise
blocks a pre-launch rename. Recheck immediately before first publication.

Migration therefore follows these constraints:

1. Freeze a green baseline before any rename.
2. Rename the Dart package `flark` to `flark_core` in a mechanical-only green
   commit, retaining behavior.
3. Rename the Flutter package `flark_flutter` to `flark` in a second
   mechanical-only green commit, retaining behavior.
4. Add and prove `flark-runtime` and `flark-abi` beside the legacy bridge.
5. Build the direct Dart and Flutter product paths under their final package
   identities.
6. Remove the legacy endpoint/wire/host-store path only after direct-path
   parity and archive-consumer receipts.
7. Perform any filesystem moves separately.

The identity cutover is not allowed to hide runtime, ABI, or behavior changes.

## 11. Non-goals for v4

- Web or Linux product support.
- A pure-Dart parser or runtime-selectable parser backend.
- A public parser AST or general editor-framework abstraction.
- Another language SDK before a real consumer exists.
- Recreating the endpoint/publication protocol behind new names.
- Flutter input or rendering concepts in `flark_core` or Rust.
- Public floor-device performance claims based on this Mac or a simulator.
- Collaborative editing, multi-source provenance, or rich-text document truth.

## 12. Acceptance condition

This architecture is realized when an edit can travel from the `flark` Flutter
surface through `flark_core` and the thin ABI into Rust-owned source, then
return a capped, revision-matched viewport containing only current-revision
certified semantics, with exact source available throughout, every layer
independently testable, and no Markdown decision outside Rust.

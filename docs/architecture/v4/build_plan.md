# v4 build plan

**Companion to [RFC 024](../rfc/rfc_024_bounded_inframe_markdown_engine.md).**
RFC 024 fixes *what* and *why*; this fixes *in what order* and *when each piece
is done*. 2026-08-06.

## The inventory, concretely

**Keep** — `native/comrak_bridge/crates/flark_parser` and `flark_engine`: the
block/inline grammar, source rope, packed green representation, reference
occurrence index, fuel/abort machinery. Independently cleared by the G2 bisect
(22/22 constructs, full mixture at 25 KB in 32 ms).

**Delete** — the endpoint protocol, wire codecs, publication path, independent
host store (`native/comrak_bridge/src/v3_*`), the `EditableText` island adapter
and every `SelectionArea` use (`packages/flark_flutter/lib/src/v3/flutter/`),
and v2's projection prediction plus its 35 Dart-side scanners. ~30k of ~137k
production Rust lines are boundary infrastructure that goes with the isolate.

**Build** — a lean direct FFI, the in-frame pump, and the surface (grown from
G4's Variant B, which already passes the §7 suite over plain strings).

## Sequence

### M0 · Walking skeleton
*The smallest thing that proves the whole stack end to end.* One document, one
own-painted surface, real engine, typing works.

- Lean synchronous FFI: apply-edit and bounded queries with **no wire protocol**.
  G3 measured the current one at 62 poll round-trips and 2.9 MB encoded per
  1 KB document — that is what this replaces.
- In-frame pump driven from a frame callback with a work budget (G3's shape).
- G4 Variant B wired to the engine instead of `List<String>`.

**Done when:** typing into a real parsed document is correct, and the jank
harness runs green on desktop. **This milestone exists because v3's integration
failure would have been caught here.** No further work starts until it holds.

### M1 · The surface
Grow the skeleton to a real editor.

- The full §7 acceptance suite passing against the *engine*, not plain strings.
- Virtualized rendering with unbuilt blocks asserted.
- Clipboard with exact source, undo/redo over source transactions, block
  split/merge, cross-block replacement.
- Marker-free live rendering (hide syntax around the caret).

**Done when:** the §7 suite is green against the engine, and G2 produces real
frame timings at 5 KB / 25 KB / 100 KB on desktop.

### M2 · Grammar to product level
Extend *incremental* coverage: block quotes, nested and loose lists, tables —
all currently fail closed.

**Open question to settle first, because it may shrink this milestone a lot:**
exact whole-file CommonMark parses at ~275 MiB/s, which is ~0.25 ms for a 71 KB
document. If v4's fail-closed path is a full reparse and bounded queries have
removed the Dart marshal, **failing closed may simply be fast enough at real
document sizes** — making incremental coverage a large-document optimisation
rather than a correctness requirement. Measure this before building it. It could
turn M2 from months into weeks.

**Done when:** the conformance ledger reflects reality (no structural-admission
vs incremental-coverage conflation), and real documents never visibly degrade.

### M3 · Platform completeness
The long tail, and where timelines historically go.

- Touch selection handles and the magnifier — **built by neither G4 variant and
  required by §7**. Half the target platforms are currently untested on the most
  user-visible interaction there is.
- The IME device matrix (G1) on real hardware — the runbook and recording sheet
  are already written.
- Accessibility semantics — untouched by anything so far.
- macOS / iOS / Android / Linux / Web bring-up and CI.

### M4 · Scale
- Viewport-first cold open (currently parses to completion).
- Indexed reference winner lookup — 71 s on Chrome for the 100k-reference
  fixture, cause already named in-tree.
- The 1 MB contract verified on a mid-range phone.

## Cross-cutting, enforced from M0

1. **No silent stops** (RFC 024 §6.2). Every terminal or quiescent state carries
   a discriminated reason that reaches the embedder. Four separate instances of
   this class have already been found; it is a design requirement, not a bug to
   fix a fifth time.
2. **Discriminated status codes.** `0x0111` has stood for at least four
   unrelated faults, which is why each needed its own investigation.
3. **Cosmetic degradation.** Above budget: last-certified structure mapped
   forward. Never a guess, never a block type the last authoritative parse did
   not produce.
4. **The jank harness is a permanent gate**, not a one-off.

## Known open bugs

- **Paste non-convergence** — 32 KB paste, 100,000 pumps, quiescent, source
  intact, no error. Diagnosed as a silent stall; root cause not yet found.
- **Over-window lines starting with a marker char** (`> # \` ~ < - _ * + 0-9`)
  still fault; lazy continuation into an open list item faults separately.
- **Viewport out-of-authority query** — moot if that layer is deleted as
  planned, but confirm rather than assume.

## Deferred, deliberately

Multi-source edit provenance (dropped — the strategic case for it did not
survive validation); the engine as a separately-shipped artifact; streaming
ingest; structural diff; collaborative editing. All remain *affordable* under
this architecture. None is a reason to build anything now.

## Is the path clear?

**Yes for M0 and M1** — decided, evidenced, and mostly a matter of execution.
**Yes but unsized for M2**, pending the reparse-cost measurement above.
**Clear but unstarted for M3 and M4**, which is where the real remaining time
lives — the editor around the engine, not the engine.

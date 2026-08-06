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

### M0 · Architectural proof
*Revised 2026-08-06 after external review. This was "walking skeleton"; the
reviewer was right that several things deferred to M3/M4 can **change the
design**, and so must come first. M0 is correspondingly bigger — plan
weeks, not days.*

Added to M0 by that review:

- **The whole-reparse challenge** (see M2 — it must happen *before* grammar
  work, not after).
- **Current-revision certification** (RFC 024 §4.4) — the correctness model,
  not a rendering detail.
- **Progress-token liveness** (RFC 024 §6.2) and **32 KB paste convergence**,
  which is a blocker rather than backlog because it violates the invariant
  outright.
- **End-to-end frame timing**, not parser timing alone.
- **A physical Android and iOS input vertical slice** — composition, autocorrect,
  soft-keyboard backspace across a block boundary, selection replacement,
  composition during scroll, input-window movement, hardware keyboard.
- **First floor-phone typing measurement.** Deferring all device evidence to M4
  repeats this program's original mistake: M4 should *certify* the envelope, not
  provide the first evidence the design works on its target hardware.

Then the original skeleton: one document, one own-painted surface, real engine,
typing works.

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

**The whole-reparse challenge — reframed 2026-08-06, and moved into M0.**

My original framing ("does the incremental parser earn its place at all?") had a
real flaw the reviewer caught: it invites an open-ended re-litigation that could
end in a size threshold, a runtime strategy switch, or a fallback — i.e. **two
implementations of Markdown semantics, which is precisely the disease v2 died
of.** The corrected framing:

> **v4 ships exactly one parsing strategy.** The incumbent is the incremental
> engine, because it already demonstrates bounded edit work, resumability,
> incremental reference resolution and acceptable pump latency. Challenge it
> **once**, with a **disposable** measurement on a throwaway branch, before
> expanding the grammar. **Delete the loser.** No parser abstraction, no runtime
> selection, no size thresholds, no fallback, no second conformance suite.

The comparison must measure the **whole chain**, not parse throughput: apply the
edit → parse the complete source → build native tree and reference state →
determine what changed for the viewport → answer the bounded render queries →
allocations and retained memory → Flutter layout and paint. Across realistic
prose, Markdown-dense text, giant paragraphs, reference-definition changes,
sustained typing, streaming append, large paste, and 1 MB — **on the floor
device**.

Whole-reparse replaces the incremental engine only if it wins **decisively on
all three axes**: performance margin (comfortably within its share of the frame,
not merely under 8 ms in isolation), conformance velocity (substantially easier
route to full CommonMark/GFM), and complexity deletion (enough incremental
machinery disappears to materially reduce maintenance). If it merely *matches*
at 24–71 KB while preserving the same tree, invalidation, reference, scheduling
and query machinery, it has **not** won — close the question and commit.

*Note on my earlier "may be weeks rather than months": withdrawn. It was not
supported by evidence, and long-tail Markdown grammar and recovery behaviour is
exactly where incremental parsers accumulate complexity.*

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

## Two design gaps that must close before M1 (external review, 2026-08-06)

**1. The bounded IME input window is unspecified and load-bearing.** RFC 024
claims both "Dart never materialises the document" and "one document-level
`DeltaTextInputClient`". Those are only compatible if the platform sees a
*bounded window*, not the document — and that design is currently implicit.
G4's Variant B did build one (the blocks the selection touches, `\n\n`
separator, a two-character invisible prefix so backspace at offset 0 is
reportable, capped at 2048 UTF-16 units) but it is prototype-grade and
undocumented. It must specify: what source range the platform value represents;
how source offsets map to platform offsets; how the window moves; whether it may
move during active composition; how backspace and delete cross window and block
boundaries; how autocorrect replacements outside the immediate word apply; what
happens when a document selection exceeds the window; and how the connection
resynchronises without corrupting composition.

**2. `Position(block, offset)` is underspecified in both halves.**

*`block`* must not be a parser node. Editing a fence, list boundary or block
quote can replace block structure while the user's position should remain
meaningful. **Canonical selection should be stable source anchors plus
affinity**, with parser blocks and layout blocks as derived views.

*`offset`* must not be an untyped integer. The implementation crosses source
byte offsets in Rust, UTF-16 code units in Flutter input, grapheme boundaries
for deletion and caret movement, shaped glyph clusters for hit testing, and
visual positions with bidi affinity. **Use distinct types with explicit
conversions** — the classic catastrophic editor bug in this area is a valid
integer used in the wrong coordinate space.

## The workload envelope, not a scalar

"1 MB" does not define a workload. A megabyte can be ordinary prose, one giant
paragraph, tens of thousands of tiny blocks, a huge table, delimiter-dense
Markdown, many global references, or a very large open fence. The contract
should eventually name: source size, block count, maximum block and line length,
syntax density, maximum interactive edit size, viewport size and text scale, and
a memory ceiling.

Accordingly: **1 MB is an internal architecture target now, and becomes a public
guarantee only after named-device verification**, with a stated degradation
contract beyond the verified envelope.

## Is the path clear?

**Yes for M0 and M1** — decided, evidenced, and mostly a matter of execution.
**Yes but unsized for M2**, pending the reparse-cost measurement above.
**Clear but unstarted for M3 and M4**, which is where the real remaining time
lives — the editor around the engine, not the engine.

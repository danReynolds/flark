# RFC 021: Surface edit/selection convergence

> **Historical v2/v3 design record.** [RFC 026](rfc_026_flark_v4_product_architecture.md),
> [RFC 027](rfc_027_continuously_rendered_markdown.md), and the
> [v4 build plan](../v4/build_plan.md) control the active product. Its status and
> implementation references below are retained only as prior evidence.

**Historical status (2026-07-11):** Motivating bug FIXED surgically (§3a); broader convergence
**deferred** as an optional simplification, not a bug-fix necessity. Empirical
tracing refuted this RFC's original premise — see §3.
**Author:** (this RFC)
**Related:** RFC 017 (controller module boundaries),
`legacy/docs/v2_v3/doc/architecture/live_edit_intent_pipeline.md`,
`legacy/docs/v2_v3/doc/architecture/live_rendered_rebuild_isolation.md`,
`docs/architecture/v2/inline_delimiter_validity_2026-07-10.md`.

## 1. Summary

> **Revision note (2026-07-11).** This RFC originally argued that *both* editing
> surfaces resolve editing semantics independently and that correctness was
> "split across them, neither complete." Empirical tracing (§3) **refuted** the
> load-bearing half of that claim: the whole-document **host surface is
> correct** for cross-block edits (the apparent bug was a test-harness
> artifact), and the only real defect was a **localized clip bug** in the
> per-block surface — now fixed in ~15 lines (§3a). The sweeping two-surface
> convergence below is therefore **not required** to fix the observed bugs; it
> remains a *possible* future simplification whose urgency is much lower than
> first framed. The rest of the document is retained, corrected, as the record
> of that investigation and as the design if the simplification is ever taken.

The original proposal: converge both surfaces onto a single source-space
edit/selection resolver, leaving the surfaces as thin input transports. The
analogy to the inline-validity work (four delimiter-placement sites → one
`FlarkInlineDelimiterPlacement` module) is real, but the evidence showed the
duplication is far less load-bearing than assumed — one surface owns the whole
document natively and is correct; only the other reconstructs, and its one real
bug was a narrow projection error, not the reconstruction model itself.

## 2. Background: two surfaces, chosen per document

The live editor mounts one of two editing surfaces, selected by document *content*, not a user mode (`live_block_editor.dart:88-103`, `_requiresBlockWidgetEditing:186-198`):

- **Host surface** — one whole-document projected `EditableText` (`flark_projected_editable_text.dart`). Used for documents that are only paragraphs + inline styling, and as the fallback when no render plan exists.
- **Per-block surface** — one editable per rendered block plus a synthetic blank editable per separator line (`projected_editable/live_block_text.dart`). Used the moment a document contains any list, quote, code block, table, or standalone image.

There is also a third, simpler surface — **raw source mode** (`flark_editable_text.dart`), where display == source — which shares the controller and input policy but has no projection/echo layer. It is largely outside the drift and is addressed only in §8.

Because the surface is chosen by content, **the same keystroke runs different code as a document gains or loses structure.** A user who types `- ` at the top of a plain document silently crosses from the host surface to the per-block surface mid-session. The two surfaces must therefore agree on editing semantics — and they do not.

## 3. The problem, as originally framed — and what the evidence actually showed

The RFC originally presented this table as proof of an architectural fault:

| Operation | Host surface | Per-block surface |
|---|---|---|
| Backspace over a **full** cross-block range | ✅ merges | ✅ honors it (`_deleteControllerSelection`, `live_block_text.dart:677`) |
| Backspace over a **partial** cross-block range | — | ❌ collapses to a one-char delete |
| Type over a cross-block range | ❌ *(claimed)* inserts at anchor | ✅ replaces (`_applyLocalEchoToDocumentSpanningSelection`) |

**Empirical tracing refuted the two ❌/— host cells.** The "host inserts at
anchor" cell came from the coverage suite driving the harness `type`, which
modelled a keystroke as `replaceRange(at, at, text)` — a pure *insertion*, not
a selection replacement. A faithful keystroke (real platform value, verified by
driving `tester.testTextInput.updateEditingValue`) replaces the selection:
`alpha\n\nbeta` with `4..8` selected + `X` → `alphXeta`, and a real cross-block
Backspace → `alpheta`. **The host is correct on both.** So there is no
"correctness split," and no bug that is "fixed on one surface but not the
other." The harness `type` was corrected to replace a non-collapsed selection
(making the suite faithful); the `flark_cross_block_selection_test.dart` header
records the correction.

That leaves exactly **one** real defect — the per-block partial-backspace — and
tracing showed even *it* was not the "two disagreeing rails" the inventory
hypothesized. See §3a.

### 3a. The real bug and its surgical fix

Instrumented tracing of the partial-backspace case (`> quoted\n\ntail`, select
`5..13`, Backspace) established the true mechanism:

- The document selection stays `5..13` through focus; it is **not** pre-collapsed.
- `_localSelection` (`live_block_text.dart`) projected the document selection
  into the extent block as a **collapsed caret at the block's end** (source 14)
  instead of the faithful clipped sub-range (`10..13`).
- The document-selection preservation guard in
  `_applyLocalDisplaySelectionToController` then evaluated
  `current.end (13) >= requestedEnd (14)` → **false** (the caret sat one past
  the selection extent), so it declined to preserve and let the selection
  collapse — after which Backspace did a one-char local delete.

Select-all was immune only because it covers each block fully, so
`_localSelection` returned a non-collapsed `0..textLength` and the guard held.

**Fix:** project a partially-overlapping document selection as its faithful
clipped, non-collapsed sub-range in every block it touches
(`_clippedLocalSelection`), exactly as a fully-covered block already returned
`0..textLength`. A partial selection now behaves like the whole-document case;
the guard preserves it; Backspace/replace reach the whole selection. ~15 lines,
no new abstraction, rebuild-isolation benchmark unchanged (`builds_per_edit=1.0`
at 10/20/40/80 blocks). The flipped regression test asserts `> quol`.

The lesson for §4 onward: the per-block surface's document-spanning
reconstruction is **not** inherently broken — it had one projection bug. The
case for wholesale convergence is correspondingly weaker than the original
framing claimed.

## 4. Root cause: the document-spanning seam

The host owns the whole document in one `EditableText`, so cross-block operations are native and need no code. The per-block surface must **reconstruct** a document-level selection/edit from a single block's clipped slice, and does so in three places guarded by five heuristics (`live_block_text.dart`):

1. **Projecting the doc selection *into* a block** — `_localSelection:532-579`. When the display selection partially overflows the block, it **keeps the current local selection or collapses to `TextSelection.collapsed(textLength)`** (`:565-573`).
2. **Applying a block-local selection *back* to the controller** — `_applyLocalDisplaySelectionToController:356-411`, including the `spansBeyondBlock` preservation guard (`:386-409`) that refuses to shrink a document selection down to a block clip.
3. **Applying a block-local edit to a document selection** — `_applyLocalEchoToDocumentSpanningSelection:609-675`, a prefix/suffix diff of the block-local echo applied over the whole document selection.

The seam: **the preservation logic is not uniform across these paths.** The `spansBeyondBlock` guard lives *only* in path 2. The classifier's plain selection-sync intent (`FlarkLiveBlockProjectedSelectionIntent`, executed `live_block_text.dart:336-338`) calls `applyProjectedSelection` **unconditionally**, with no guard. Combined with the overflow-collapse in path 1, a partially-overlapping block whose local caret has collapsed to its end overwrites the document selection on the next selection notification — *before* the Backspace key arrives. Meanwhile edits travel on two rails that disagree: IME `type` → path 3 (honors the full range); physical `Backspace` → `DeleteCharacterIntent` → `_deleteControllerSelection` (`:677`), which only fires while `controller.selection` is *still* non-collapsed. Those two rails disagreeing **is** the cross-block bug family.

## 5. What already exists to build on

Convergence is mostly *routing*, not *invention*. These are already surface-agnostic, source-space, and correct — both surfaces call them today:

- **Edit resolver:** `controller.applyProjectedTextEdit` → `FlarkProjectedTextEditAdapter.resolveDisplayEdit` (`flark_projected_text_edit_adapter.dart:60`) — the display-diff → source-transaction brain, including the caret-anchor affinity heuristic, marker-exit, armed-wrap, and the inline placement repairs.
- **Deletion/selection resolvers (pure):** `FlarkProjection.resolveBackspaceSelection` (`:658`), `resolveForwardDeleteSelection` (`:723`), `expandDeletionOverInlineRunMarkers` (`:597`), `inlineRunBoundaryStep` (`:555`).
- **Selection sink:** `controller.applyProjectedSelection` (`flark_flutter_controller.dart:1204`) and `applySelection` (`:1243`).
- **Structural key routing:** the input policy (`flark_markdown_input_policy.dart`) `dispatchEnter`/`dispatchBackspace`/`dispatchForwardDelete`, parameterized only by a `currentSelection` reader and an `applySelection` applier — which is *precisely* the seam that differs between surfaces.
- **Classifiers** are pure and table-tested; their shared *steps* are already functions. Only the *chains* are duplicated.

What is **not** yet shared: (a) the display→source **selection applier** (two implementations, only one carrying the doc-spanning guard), and (b) the **document-spanning edit** primitive (exists only as the block-local echo heuristic; the host has no counterpart). These two are the whole of the convergence.

## 6. Proposal

### 6.1 One surface input event

Both surfaces stop *deciding* and start *reporting*. Define one value type (headless, in the controller/projection layer):

```
FlarkSurfaceEdit {
  FlarkSourceRange ownerRange;   // the reporting editable's source span; whole document for the host
  String oldLocalValue, newLocalValue;
  FlarkSelection oldLocalSelection, newLocalSelection;
  TextRange? composing;
}
```

The host reports `ownerRange = [0, document.length)`; a block reports its slice. The shape is identical, so there is one downstream path.

### 6.2 One resolver, entirely in source space

Add a controller-level resolver that consumes a `FlarkSurfaceEdit` and produces exactly one transaction (or selection change), reusing §5's primitives:

- **Local → source** mapping happens **once**, using `ownerRange` — never re-derived per guard.
- **Selection changes** are reconciled against the controller's authoritative source selection by **one rule**: a reported local selection that is a *clipped subset* of an existing document-spanning selection does not shrink it; anything else replaces it. This single rule replaces path 1's overflow-collapse, path 2's `spansBeyondBlock` guard, and the unguarded classifier sync — the three inconsistent places become one.
- **Edits over a document-spanning selection** become a first-class controller primitive (promote `_applyLocalEchoToDocumentSpanningSelection`'s logic to the controller, source-space, surface-independent), so the host gets cross-block replace *for free* and the block stops owning a bespoke rail.
- **Backspace/type over a selection resolve identically** because both now read the controller's authoritative selection at resolution time, not a surface's reconstructed view.

### 6.3 Surfaces become dumb transports

After convergence, each surface's remaining responsibilities are exactly: **render** its editable(s), **own focus**, paint composing/cursor, and **emit `FlarkSurfaceEdit`s**. No surface reconstructs document selections, diffs echoes into document edits, or carries a preservation guard. The per-block `_applyLocalDisplaySelectionToController`, `_applyLocalEchoToDocumentSpanningSelection`, `_localSelection` overflow branch, and `_deleteControllerSelection` collapse to calls into the resolver, and most of their bodies are deleted.

## 7. Performance: what convergence must not touch

The per-block surface exists for **rebuild isolation**
(`legacy/docs/v2_v3/doc/architecture/live_rendered_rebuild_isolation.md`):
whole-document rebuild is ~0.6 ms/block linear (breaches 60 fps at ~20-25
blocks); stable-id widget reuse (`flark_live_block_reconciler.dart`) collapses
a keystroke to ~1 block rebuilt (40 blocks: 1.16 ms median; 80: 1.53 ms).
**Convergence keeps all of this** — it changes *where an edit is resolved*,
not *how blocks are rendered or reused*. The invariant to preserve:

> The shared resolver must be reachable **without** rebuilding unchanged blocks — i.e. it reads the controller's document selection at edit time and must **not** fold document-selection state into a block's content signature (`live_block_editor.dart:234` `liveBlockContentSignature`) or its reuse key (`:250-257`).

This is already how the block surface reaches the shared edit center today (edits go through `applyProjectedTextEdit` while rebuild cost stays ~1 block), so routing *selection* and *document-spanning edits* through the same center is compatible by construction. Gates that must stay green at every phase:

- `test/v2/flutter/flark_live_rendered_rebuild_test.dart` (`flarkDebugLiveBlockBuildCount <= 2` per edit).
- `test/v2/performance/flark_live_rendered_rebuild_benchmark_test.dart` (`builds_per_edit`, 10/20/40/80 blocks).
- `flark_live_block_reconciler_test.dart`, `flark_live_block_signature_test.dart`.

## 8. Migration (strangler, phased — each phase ships independently)

> **Deferred (§10).** This plan is retained as the design *if* the convergence is
> ever taken. The motivating bug is already fixed (§3a), so nothing here is
> scheduled. Phase 1 below — "unify the selection applier" — is the piece the
> §3a fix made unnecessary: the localized `_clippedLocalSelection` fix achieves
> the same correctness without moving the applier.

Every phase keeps the full suite green, with the cross-block, IME (`flark_ime_input_test.dart`), block-exit (`flark_live_block_exit_sequence_test.dart`), and rebuild-benchmark suites as the standing gates.

- **Phase 0 — introduce `FlarkSurfaceEdit` + the resolver skeleton** behind the existing entry points; no behavior change. The resolver initially just calls today's shared center.
- **Phase 1 — unify the selection applier.** Move the one reconciliation rule (§6.2) into the resolver; route *both* surfaces' selection appliers through it; delete the `spansBeyondBlock` guard and the overflow-collapse. **Acceptance:** the partial-cross-block-backspace test flips from skipped/pinned-buggy to green; host behavior unchanged.
- **Phase 2 — promote document-spanning edits to a controller primitive.** Route the block's echo path and add the host's missing counterpart. **Acceptance:** cross-block *type* replaces on both surfaces (host's insert-at-anchor bug fixed); the block's bespoke echo body is deleted.
- **Phase 3 — converge structural key routing.** Both surfaces pass the same `currentSelection`/`applySelection` into the input policy; resolve the row-15 immediate-parse asymmetry (pipeline doc) deliberately.
- **Phase 4 — (optional) fold raw source mode** into the same resolver with an identity projection; low priority since it is outside the drift.

The classifier *chains* (CLS host vs block) stay separate for now — their asymmetries are intentional and pinned (`flark_live_edit_classifier_test.dart` "asymmetry pins"); this RFC converges the *resolution*, not the *classification*. Merging the chains is a possible follow-up, not in scope.

## 9. Alternatives considered

- **Single virtualized whole-document editable (drop the per-block surface).** Conceptually simplest — one surface, no seam. Rejected as the primary path: it is a real performance bet against the rebuild-isolation results, and it would also lose per-block IME connection isolation (`live_rendered_rebuild_isolation.md:168-171`). Could be revisited if Flutter's editable virtualization matures, but not on the strength of these bugs.
- **Keep two surfaces, just add the missing guard to the classifier sync path.** This is the local patch for the partial-backspace bug (add the `spansBeyondBlock` guard where it's missing). Rejected as the *architecture* answer because it deepens the duplication (a fourth place that must stay in sync) and does nothing for the host's cross-block-type bug. Acceptable only as a stopgap if Phase 1 must be deferred.

## 10. Decisions (resolved 2026-07-11)

The three questions were left open in the draft; given the §3 findings they are
now decided, all toward *not over-reaching*:

1. **IME composing region — keep per-surface.** A composing region cannot span
   blocks, and the IME suite is green on both surfaces. If convergence ever
   happens, centralize echo *resolution* only and keep composing-span *painting*
   per surface. **No change now.**
2. **Raw source mode — leave separate.** It is correct, outside the drift, and
   folding it in buys nothing against a real defect. **Not scheduled.**
3. **Classifier chains — leave separate.** Their asymmetries are intentional and
   pinned (`flark_live_edit_classifier_test.dart`). Converging classification is
   explicitly out of scope. **No change now.**

**Overall decision:** the motivating bug is fixed surgically (§3a); the
full two-surface convergence (§6–§9) is **deferred**, to be reconsidered only if
a *future* defect is traced to the duplication itself rather than to a localized
projection error. The value of §6–§9 is now "reduce duplication for its own
sake," which does not clear the bar for a multi-phase refactor of a
performance-critical, IME-sensitive surface on its own.

## 11. Success criteria

- The cross-block coverage suite passes with **no surface-conditional expectations** — the same operation yields the same result regardless of which surface the document mounts.
- `_applyLocalDisplaySelectionToController`, `_applyLocalEchoToDocumentSpanningSelection`, and the `_localSelection` overflow branch are deleted or reduced to thin resolver calls.
- Rebuild-cost benchmarks unchanged within noise.
- No new surface-conditional branches introduced; net deletion of duplicated selection/edit logic.

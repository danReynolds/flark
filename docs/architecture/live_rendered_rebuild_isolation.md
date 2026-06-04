# Live-rendered rebuild isolation — findings & path forward

**Status:** research complete, not yet implemented.
**Goal:** make a keystroke in `liveRendered` mode rebuild only the edited block,
so per-keystroke cost is flat regardless of document size.

This consolidates two investigations: a source dive of **super_editor** (a
block-based WYSIWYG editor measured flat at ~3 ms across 10–80 blocks vs Flark's
21→72 ms) and a cited research pass on the generalizable patterns. See
`benchmark/peer/README.md` for the measurements.

## Why Flark is linear and super_editor is flat

- **Flark** rebuilds *every* `_FlarkLiveRenderedBlock` per keystroke. Verified
  cause (Flutter docs): the build phase skips an element **and its whole subtree
  only when it is re-rendered with a reference-identical widget instance**
  (`identical(oldWidget, newWidget)`). Flark constructs fresh block widgets each
  build, so none are skipped. **Stable keys do _not_ fix this** — `canUpdate`
  (runtimeType+key) decides update-in-place vs recreate, but an updated element
  still *rebuilds*. The lever is widget-instance reuse, not keys.
- **super_editor** is flat because of four separable techniques (below), resting
  on a node-tree model where blocks are addressed by **stable node IDs**, not
  absolute offsets — so editing block M never changes block N's identity or
  addressing, and only the edited node's component is dirtied.

## The four techniques (and what's portable)

1. **Reference-identical widget reuse** *(Flutter framework lever — directly
   portable).* Cache each block's built widget; on the next build, hand back the
   *same instance* for unchanged blocks. Flutter then skips that subtree.

2. **Centralized diff → per-block watch-and-skip** *(super_editor `_presenter`
   — portable).* A presenter computes which blocks changed (by `==` on view
   models) and emits `changedComponents`; each block (`_PresenterComponentBuilder`
   with `watchNode: nodeId`) calls `setState` only if its id is in that set.

3. **Most-volatile-phase-last, cached pipeline** *(super_editor — portable).*
   Style phases (baseline → doc → component → **selection**) are cached; only
   phases from the earliest dirty one re-run, so a **caret move re-runs only the
   selection phase** and rebuilds no content.

4. **Dedicated position-mapping layer** *(ProseMirror `StepMap`/`Mapping` —
   Flark already has the equivalent).* Block anchors are remapped *forward*
   through each change (content-free numeric ranges) rather than recomputed from
   absolute offsets. Flark's `projection.predictAfter()` already maps offsets
   through a transaction — this is the same idea.

**Key insight:** super_editor's stable node IDs are the *foundation* of its
flatness, but the techniques that deliver it are mostly Flutter-level and do
**not** require Flark to adopt a node-ID document model. Flark can stay
source-string-based and reach flat cost by making blocks *behave*
position-independently.

## The Flark bridge: source-offset model → flat cost

Flark's blocks are addressed by absolute source offsets, so block N's descriptor
changes whenever earlier content changes length → it rebuilds. To break that:

1. **Stable block identity via reconciliation.** Flark re-parses the whole
   string each edit, producing a fresh block list with no identity continuity.
   Assign stable ids by matching new blocks to the previous set (content + type
   diff), so an unchanged block keeps its id across offset shifts. (`_liveRendered
   BlockId` today falls back to `type:sourceRange.start`, which *is* the problem —
   it shifts.) Incremental reparse with fragment reuse (Lezer/tree-sitter style)
   is the gold standard but heavier; a content-based reconciliation diff is the
   lower-risk first cut.

2. **Offset-independent content signature.** Per block, compute a signature from
   its *display-slice text + structural descriptor + style* — **not** its
   absolute offsets. Equal signature ⇒ unchanged.

3. **Cache & reuse unchanged block widgets.** Parent keeps `Map<blockId,
   (signature, Widget)>`. If a block's signature matches, reuse the **same widget
   instance** → Flutter skips its subtree. Callbacks (e.g. move-to-neighbor) must
   be id-bound/stable so they don't break reference-identity.

4. **Resolve source offsets for editing separately.** A block that didn't
   rebuild still edits correctly because edit→source mapping is resolved at edit
   time via the existing projection/transaction mapping — not from cached
   offsets.

5. **Decouple selection/caret.** A caret move must not change any block's
   signature, so no block rebuilds (super_editor technique #3). Verify
   selection-only controller notifications don't perturb block signatures.

## Why this is lower-risk than the original node-view plan

The original sketch had each block **self-subscribe** to the controller and
**lazily resolve its own absolute offsets** — scattered, and it re-syncs focused
fields (IME-risky). This plan instead:

- Centralizes the diff in the parent (one place), block widget *types* are
  unchanged — no self-subscription, no per-type offset rebasing.
- Uses one well-defined Flutter mechanism (identical-instance reuse).
- **Is IME-safer:** unchanged blocks are reference-identical ⇒ their Elements/
  States (including any live input connection) are *not touched at all*. During
  composition in block N, blocks ≠ N are skipped entirely; block N rebuilds as it
  does today.

## Residual risks

- **Reconciliation correctness** — wrong id assignment ⇒ wrong reuse. Contained,
  testable.
- **Signature completeness** — a signature must change iff rendered content
  changes, or a stale block is reused. The sharpest correctness risk; mitigate by
  deriving the signature from the same inputs the block's build consumes.

## Staged plan (each independently landable + testable)

| Stage | Change | Guard |
| --- | --- | --- |
| 1 | Stable block identity (reconciliation diff over render-plan blocks) | identity-persistence test across offset-shifting edits |
| 2 | Offset-independent per-block content signature | unit tests: signature stable under offset shift, changes on content edit |
| 3 | Parent caches + reuses unchanged block widget instances | `flarkDebugLiveBlockBuildCount` drops from N to ~1 (the skipped guard test, un-skipped) |
| 4 | Selection/caret decoupled from content signature | caret-move test: 0 block rebuilds |
| 5 | *(optional)* viewport virtualization for huge docs' initial build | only if Stage 1–4 leave a first-paint cost worth bounding |

Stages 1–4 deliver the flat per-keystroke cost; virtualization (Stage 5) is for
initial build of very large documents and is likely unnecessary once 1–4 land.

## Sources

super_editor source (`lib/src/default_editor/layout_single_column/_presenter.dart`,
`_layout.dart`, `core/document.dart`); Flutter `inside-flutter`,
`Element.rebuild`, `Widget.canUpdate`, `ListenableBuilder`; ProseMirror
`prosemirror-transform` README; Lezer/tree-sitter incremental-parse docs.

# Live-rendered rebuild isolation — findings & path forward

**Status:** research complete, not yet implemented.
**Goal:** make a keystroke in `liveRendered` mode rebuild only the edited block,
so per-keystroke cost is flat regardless of document size.

This consolidates two investigations: a source dive of **super_editor** (a
block-based WYSIWYG editor measured flat at ~3 ms across 10–80 blocks vs Flark's
21→72 ms) and a cited research pass on the generalizable patterns. See
`benchmark/peer/README.md` for the measurements.

## Why this is warranted (profile-mode validation)

The debug-VM benchmarks understated the problem by ~10×. Measured in **profile
mode** on a real engine (macOS, AOT) via `example/lib/perf_harness.dart`, the
per-keystroke **build phase** of live-rendered editing is:

| Blocks | Build median | Build p95 | Raster |
| --- | --- | --- | --- |
| 10 | 6.0 ms | 8.2 ms | ~1 ms |
| 20 | 10.8 ms | 19.3 ms | ~1.3 ms |
| 40 | 25.0 ms | 42.0 ms | ~1.6 ms |
| 80 | 38.7 ms | 45.1 ms | ~1.4 ms |

Linear (~0.6 ms/block), and the cost is **entirely the Dart build phase** —
raster is negligible. The 60 fps budget (16.7 ms) is breached at **~20–25
blocks**, so a medium document drops frames on every keystroke in release. This
is real, not a debug artifact, and the build-phase isolation below targets it
directly: rebuilding ~1 block instead of N collapses the curve toward the
single-block cost (well under 1 ms) plus an O(N) cheap diff.

### Measured progress (profile mode, build median)

| Blocks | Baseline | Stage 1 (stable ids) | Stage 3 (typing / end) | Stage 3 (worst / start) |
| --- | --- | --- | --- | --- |
| 20 | 10.8 ms | 4.9 ms | **1.6 ms** | — |
| 40 | 25.0 ms | 9.2 ms | **1.9 ms** | 7.6 ms |
| 80 | ~50 ms | ~18 ms | **2.2 ms** | 13.1 ms |

- **Stage 1 (stable ids): ~2.5×.** The old offset-based id shifted for trailing
  blocks every keystroke, so Flutter *recreated* their Elements/States — costlier
  than rebuilding. Stable ids switch them to update-in-place.
- **Stage 3 (instance reuse): flat for realistic typing.** Reusing unchanged
  block instances collapses the slope to ~0.015 ms/block — flat ~2 ms across
  20–80 blocks for editing near the cursor (where blocks before it are reused),
  matching a node-tree editor like super_editor.
- **Worst case still bounded.** A start-of-document insertion shifts every later
  block's offsets, so those rebuild — but even 80 blocks (13 ms) stays within the
  60 fps budget.

Implemented as Stages 1–3 (Stage 4's caret/content decoupling is folded into
Stage 3's per-block selection key). Viewport virtualization (Stage 5) remains
optional and unneeded at these sizes.

Run the harness:

```bash
cd example
flutter run --profile -d macos -t lib/perf_harness.dart \
  --dart-define=FLARK_PROFILE_BLOCKS=40
```

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

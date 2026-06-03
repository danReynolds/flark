# Peer editor performance comparison

Isolated harnesses (separate packages, NOT part of `flark`) that run the **same
per-edit measurement** as Flark's own
`test/v2/performance/flark_live_rendered_rebuild_benchmark_test.dart` against
peer Flutter editors, to calibrate Flark's numbers against the ecosystem.

- `benchmark/peer/` — flutter_quill
- `benchmark/peer_supereditor/` — super_editor (git; needs a one-line patch, below)

Methodology (identical to Flark's): N line-paragraph blocks in a 600px viewport,
one-character insert near the document start, 40 timed `pump()`s, median/p95.
Debug test-VM timings — pessimistic vs profile/release, but the **ratios** and
**scaling shape** are the signal.

## Do the peers do live editing?

Yes — all three are WYSIWYG live editors (you type into rendered content, not
source). They split into two architectures:

- **Block-based** (one editable widget per block — same class as Flark's
  `liveRendered` mode, and what enables editable code/tables/checkboxes inline):
  **super_editor**, **appflowy_editor**.
- **Single rich-text layout** (no per-block widgets): **flutter_quill**.

So **super_editor is the direct architectural peer** to Flark's live-rendered
mode, which makes its result the most important one here.

## Results (debug test-VM, 600px viewport, per-edit pump median)

| Editor | 10 blk | 20 blk | 40 blk | 80 blk | Shape |
| --- | --- | --- | --- | --- | --- |
| **Flark — live-rendered** (block-based) | ~21 ms | ~27 ms | ~44 ms | ~72 ms | **linear in block count** |
| **super_editor** (block-based WYSIWYG) | ~3.5 ms | ~3.1 ms | ~3.1 ms | ~3.2 ms | **flat** |
| **flutter_quill** (single layout) | ~3.5 ms | ~3.8 ms | ~4.0 ms | ~4.1 ms | **flat** |
| **Flark — source mode** (one editable) | — | — | ~0.7 ms | — | flat |

## Takeaways

1. **A block-based live editor can be flat.** super_editor edits rendered blocks
   (paragraphs, headers, lists, images) just like Flark's live-rendered mode,
   yet stays at ~3 ms regardless of block count. It achieves this with a
   Presenter→ViewModel pipeline that **selectively rebuilds only the changed
   component** and caches the rest (plus viewport rendering) — not by making each
   block self-subscribe.

2. **Flark's live-rendered mode is the outlier: ~10–20× super_editor at 40–80
   blocks, and the only one that scales with block count.** The entire gap is
   Flark rebuilding *every* per-block editable on each keystroke.

3. **Flark's core is competitive.** Source mode (one editable, whole doc) is
   ~0.7 ms — faster than both peers. The document model, projection, and
   rendering pipeline are fine; only the live-rendered widget layer over-rebuilds.

4. **The fix is the SuperEditor path** — selective/memoized component rebuild
   (and viewport bounding) — not the higher-risk self-subscribing node-view
   rewrite Flark originally considered. super_editor is the existence proof.

## Reproduce

flutter_quill (turnkey):

```bash
cd benchmark/peer && flutter pub get
flutter test test/quill_benchmark_test.dart
```

super_editor (needs a one-line compat patch — see below):

```bash
cd benchmark/peer_supereditor && flutter pub get
# apply the patch below to the git-cached super_editor, then:
flutter test test/super_editor_benchmark_test.dart
```

## Toolchain caveat — why this took a patch

This is a bleeding-edge toolchain (**Flutter 3.41 / Dart 3.10**, April 2026),
newer than the editors' published compatibility:

- **flutter_quill 11.5** (current) — resolves and compiles cleanly. ✅
- **super_editor** — pub.dev is stale at **0.2.7** (24 months old); the current
  version ships via **git**. The git HEAD (`0.3.0-dev.*`) compiles **except** one
  line that calls a Flutter API 3.41 removed. Patch it:

  In the git-cached
  `super_editor/lib/src/default_editor/document_ime/ime_decoration.dart`, delete
  the override that Flutter 3.41 no longer defines:

  ```dart
  @override
  void updateStyle(TextInputStyle style) => client?.updateStyle(style);
  ```

  (`TextInputStyle` / `TextInputConnection.updateStyle` were removed from the
  framework. It's IME styling — irrelevant to the rebuild measurement.)

- **appflowy_editor** — the other block-based peer — caps at **0.1.12** on this
  SDK and **fails to compile** on Flutter 3.41 (multiple removed-API errors:
  `NodeVisitor` mixin, `TextInputClient` members, null-safety). Not benchmarked.
  Per research it uses per-node `ChangeNotifier` isolation, so it would also
  avoid Flark's all-blocks-rebuild.

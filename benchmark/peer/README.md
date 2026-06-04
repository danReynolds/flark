# Peer editor performance comparison

Isolated harnesses (separate packages, NOT part of `flark`) that run the **same
per-edit measurement** as Flark's own
`test/v2/performance/flark_live_rendered_rebuild_benchmark_test.dart` against
peer Flutter editors, to calibrate Flark's numbers against the ecosystem.
The large-document harnesses also compare 100KB/1MB model build, edit apply, and
post-edit viewport pump costs.

- `benchmark/peer/` — flutter_quill
- `benchmark/peer_supereditor/` — super_editor (git; fresh caches may need a
  one-line Flutter compatibility patch, below)

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

Fresh run on 2026-06-04 with Flutter 3.41.9. These are debug test-VM medians;
use them for scaling shape and relative constant-factor checks, not device
latency. See `docs/benchmarks.md` for p95 and profile-mode frame timings.

| Editor | 10 blk | 20 blk | 40 blk | 80 blk | Shape |
| --- | --- | --- | --- | --- | --- |
| **Flark — live-rendered, current** (block-based) | `8.87ms` | `8.95ms` | `9.69ms` | `10.17ms` | **flat; `builds_per_edit=1.0`** |
| **Flark — live-rendered, old baseline** (block-based) | ~21 ms | ~27 ms | ~44 ms | ~72 ms | **linear in block count** |
| **super_editor** (block-based WYSIWYG) | `5.85ms` | `5.49ms` | `6.19ms` | `5.14ms` | **flat** |
| **flutter_quill** (single layout) | `4.67ms` | `4.90ms` | `4.90ms` | `5.00ms` | **flat** |
| **Flark — source mode, current** (one editable) | — | — | `1.80ms` | — | flat |

## Takeaways

1. **A block-based live editor can be flat.** super_editor edits rendered blocks
   (paragraphs, headers, lists, images) just like Flark's live-rendered mode, and
   the current peer run stays near 5-6 ms regardless of block count. That remains
   the existence proof for selective/memoized component rebuilds.

2. **The old Flark outlier was rebuild fanout, and that gap is closed at the
   scaling layer.** Current Flark rebuilds one block per edit through 80 blocks;
   offset shifts no longer make unchanged later blocks rebuild.

3. **There is still a constant-factor question, but it is narrow.** Current
   Flark's debug pump medians are flat but higher than both peers. Profile-mode
   Flark frame timing is already under 2.2 ms p95 in the 40/80 block x
   end/start gate, so this is not evidence for another broad rebuild-architecture
   pass.

4. **Source mode remains the lower bound for one-editable work.** Current source
   mode at 40 blocks is `1.80ms` in the same debug rebuild benchmark, so the live
   block layer still carries extra constant cost even after fanout is fixed.

5. **The next peer-related work is maintenance, not invention.** Keep these
   harnesses runnable and rerun them after major widget changes. Only chase the
   remaining live block constant factor if peer-leading debug numbers become a
   concrete goal.

## Large-Document Results

Fresh run on 2026-06-04 with the same debug test-VM caveat:

| 1MB metric | Flark source editor | super_editor | flutter_quill | Read |
| --- | --- | --- | --- | --- |
| Model/controller build median | `1.28ms` | `3.78ms` | `2.91s` | Flark leads this surface |
| Edit apply median | `3.16ms` | `884us` | `1.38s` | Flark is close to the direct block peer |
| Viewport pump after edit median | `30.93ms` | `142.29ms` | `253.16ms` | Flark now leads this surface |

The conclusion is now positive for peer-comparable large-document editor
interaction. Flark's virtualized source viewport keeps 1MB build/apply strong
and moves viewport pump below both peer medians. The diagnostic Flark harness
also measures raw multiline `EditableText` and Flark live-rendered plain-corpus
pump: raw `EditableText` is `327.97ms` at 1MB, and the current live-rendered
plain path is `541.39ms`. The remaining high-value Flark-specific work is native
Markdown parse/decode, JSON payload decode, and result mapping.

## Reproduce

flutter_quill (turnkey):

```bash
cd benchmark/peer && flutter pub get
flutter test test/quill_benchmark_test.dart
flutter test test/quill_large_document_benchmark_test.dart
```

super_editor:

```bash
cd benchmark/peer_supereditor && flutter pub get
flutter test test/super_editor_benchmark_test.dart
flutter test test/super_editor_large_document_benchmark_test.dart
```

Flark large-document editor sweep:

```bash
flutter test test/v2/performance/flark_large_document_editor_benchmark_test.dart \
  --tags benchmark \
  --reporter compact
```

If a fresh git cache fails on `TextInputStyle` / `updateStyle`, apply the
compatibility patch below and rerun.

## Toolchain caveat — why this took a patch

This is a bleeding-edge toolchain (**Flutter 3.41.9 / Dart 3.11.5**, April
2026), newer than the editors' published compatibility:

- **flutter_quill 11.5** (current) — resolves and compiles cleanly. ✅
- **super_editor** — pub.dev is stale at **0.2.7** (24 months old); the current
  version ships via **git**. This run used `0.3.0-dev.51`. Some fresh git-cache
  checkouts compile **except** one line that calls a Flutter API 3.41 removed.
  Patch it if needed:

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

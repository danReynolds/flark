# Peer editor performance comparison

Isolated harness (separate package, NOT part of `flark`) that runs the **same
per-edit measurement** used by Flark's own
`test/v2/performance/flark_live_rendered_rebuild_benchmark_test.dart` against
peer Flutter editors, to calibrate Flark's numbers against the ecosystem.

Methodology (identical to Flark's): N line-paragraph blocks in a 600px viewport,
one-character insert near the document start, 40 timed `pump()`s, median/p95.
Debug test-VM timings — pessimistic vs profile/release, but the **ratios** are
the signal.

```bash
cd benchmark/peer
flutter pub get
flutter test --tags benchmark test/quill_benchmark_test.dart   # (no tag needed)
flutter test test/quill_benchmark_test.dart
```

## Results (debug test-VM, 600px viewport, per-edit pump median)

| Editor | 10 blocks | 20 blocks | 40 blocks | 80 blocks |
| --- | --- | --- | --- | --- |
| **Flark — live-rendered** (block widgets) | ~21 ms | ~27 ms | ~44 ms | ~72 ms |
| **Flark — source mode** (one editable) | — | — | **~0.7 ms** | — |
| **flutter_quill 11.5** (single rich-text layout) | ~3 ms | ~3 ms | ~3 ms | ~3 ms |

## Takeaways

1. **Flark's core is competitive.** Flark *source mode* (one `EditableText`,
   whole document) re-renders in ~0.7 ms — actually faster than Quill. The
   document model, projection, and single-editable rendering are not the
   problem.

2. **Flark's live-rendered mode is the outlier: ~7–15× Quill, and it scales
   with block count** while Quill stays flat (~3 ms). The entire gap is Flark
   rebuilding *every* per-block editable widget on each keystroke; Quill renders
   the document as a single rich-text layout, so it has no per-block widgets to
   rebuild.

3. **The trade is real, not a bug.** Flark's per-block widgets are what enable
   editable code fences, tables, and task checkboxes inline — Quill's
   single-layout model can't do that. The cost is the N-widget rebuild.

4. **The proven fix is to bound the rebuild to the viewport** (virtualization),
   which Quill (and, per research, super_editor) already do — not a
   per-block self-subscribing rewrite.

## Toolchain caveat (important)

Only `flutter_quill` (current, 11.5) both **resolves and compiles** on this
bleeding-edge toolchain (Flutter 3.41 / Dart 3.10, 2026). The block-based peers
most comparable to Flark could **not** be benchmarked here:

- `super_editor` caps at **0.2.7** on this SDK (current versions don't yet
  support Dart 3.10); 0.2.7 has a transitional, hard-to-drive API.
- `appflowy_editor` caps at **0.1.12**, which **fails to compile** on Flutter
  3.41 (pre-dates breaking `TextInputClient`/mixin/null-safety SDK changes).

Per the deep-research pass, both avoid Flark's all-blocks-rebuild architecturally
(super_editor via a Presenter/ViewModel memoization pipeline + viewport
virtualization; appflowy via per-node `ChangeNotifier` isolation), so they would
not exhibit Flark's linear-in-block-count cost. A current-version comparison
would require pinning Flark's toolchain to an older Flutter — not worth doing
just for this measurement.

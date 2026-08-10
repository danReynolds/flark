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

## Quill macOS profile runner

`lib/competitor_profile_harness.dart` is the real macOS profile-mode Quill
runner for `m0-mac-competitor-profile-v1`. It generates the frozen
`ordinary-prose` recipe at exact UTF-8 byte lengths, loads it into an unmodified
Quill editor in a 600 by 600 logical-pixel viewport, and writes raw
machine-readable process results plus the final plain-text export.

Measured character and delete edits are native macOS `NSEvent` key events sent
to Flutter's focused text-input responder. For paste, the harness loads the
exact 32,768-byte payload into `NSPasteboard.general`. Flutter's
`FlutterTextInputPlugin` does not advertise AppKit's `paste:` action, so the
harness delivers that pasteboard string through `insertText` on the active
platform `NSTextInputClient`, then restores the prior clipboard after Quill
accepts the edit. No measured edit mutates `QuillController` directly.

Each paste warmup and measured sample starts from the same frozen fixture. The
harness proves the canonical pre-paste and one-paste byte/hash states, selects
the exact pasted source range, sends a native platform backspace outside the
measured interval, waits for its accepting/raster frame, and proves the fixture
was restored before the next sample. Quill's single owned terminal newline is
classified explicitly; it is never generally trimmed. All 22 warmup/measured
transitions retain unique paste/reset sequences and comparable
request/ingress/accept/build/raster/callback timestamps, so a receipt cannot
hide an all-pastes-then-resets execution order.

The full Quill-only process matrix is:

```bash
cd benchmark/peer
dart run tool/run_quill_profile.dart
```

It builds one profile artifact, records its file-tree hash and the resolved
`pubspec.lock`, and launches 117 fresh processes: 30 cold opens per 1/5/10 MiB
tier, three 10 Hz typing runs per tier, and start/middle/end local-edit and
32 KiB-paste runs per tier. Each process preserves raw frame samples, memory,
machine/display state, source hashes, a bounded fidelity diff, stdout/stderr,
and a full final export under ignored `artifacts/`. Every export name contains
an invocation-unique process ID, and export creation refuses to overwrite an
existing artifact.

For wiring validation only, use the deliberately non-claim smoke matrix:

```bash
dart run tool/run_quill_profile.dart --smoke
```

Process and Quill-aggregate receipts report completion-envelope eligibility
separately from performance-claim eligibility. A completed Quill envelope may
be useful cohort input, but neither local output claims cohort eligibility. The
Quill runner does not capture a VM timeline/longest synchronous span, and the
protocol's competitor boundary still requires the separately pinned two-peer
aggregate receipt. Input-to-raster samples fail closed unless the selected
`FrameTiming.buildStart` is strictly later than model acceptance; a raster that
only finishes after acceptance is not attributed to that edit. Quill also
appends a required terminal newline to plain text; results record this as
`peer-appended-terminal-newline` instead of trimming it. The operator must also
enforce the protocol's five-minute idle periods and exclusive-host rule; the
runner records host state but cannot prove or enforce the absence of unrelated
work.

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

Fresh run on 2026-06-05 with Flutter 3.41.9. These are debug test-VM medians;
use them for scaling shape and relative constant-factor checks, not device
latency. See `docs/benchmarks.md` for p95 and profile-mode frame timings.

| Editor | 10 blk | 20 blk | 40 blk | 80 blk | Shape |
| --- | --- | --- | --- | --- | --- |
| **Flark — live-rendered, current** (block-based) | `9.64ms` | `9.15ms` | `9.76ms` | `10.13ms` | **flat; `builds_per_edit=1.0`** |
| **Flark — live-rendered, old baseline** (block-based) | ~21 ms | ~27 ms | ~44 ms | ~72 ms | **linear in block count** |
| **super_editor** (block-based WYSIWYG) | `7.20ms` | `8.46ms` | `7.35ms` | `7.83ms` | **flat** |
| **flutter_quill** (single layout) | `6.82ms` | `8.59ms` | `9.78ms` | `8.53ms` | **flat** |
| **Flark — source mode, current** (one editable) | — | — | `1.50ms` | — | flat |

## Takeaways

1. **A block-based live editor can be flat.** super_editor edits rendered blocks
   (paragraphs, headers, lists, images) just like Flark's live-rendered mode, and
   the current peer run stays near 7-8 ms regardless of block count. That remains
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
   mode at 40 blocks is `1.50ms` in the same debug rebuild benchmark, so the live
   block layer still carries extra constant cost even after fanout is fixed.

5. **The next peer-related work is maintenance, not invention.** Keep these
   harnesses runnable and rerun them after major widget changes. Only chase the
   remaining live block constant factor if peer-leading debug numbers become a
   concrete goal.

## Large-Document Results

Fresh run on 2026-06-05 with the same debug test-VM caveat:

| 1MB metric | Flark source editor | super_editor | flutter_quill | Read |
| --- | --- | --- | --- | --- |
| Model/controller build median | `1.30ms` | `4.41ms` | `3.77s` | Flark leads this surface |
| Edit apply median | `1.59ms` | `1.16ms` | `1.11s` | Flark is close to the direct block peer |
| Viewport pump after edit median | `54.13ms` | `128.63ms` | `211.38ms` | Flark now leads this surface |

The conclusion is now positive for peer-comparable large-document editor
interaction. Flark's virtualized source viewport keeps 1MB build/apply strong
and moves viewport pump below both peer medians. The diagnostic Flark harness
also measures raw multiline `EditableText` and Flark live-rendered plain-corpus
pump: raw `EditableText` is `317.32ms` at 1MB, and the current live-rendered
plain path is `428.22ms`. The remaining high-value Flark-specific work is native
Markdown parse/decode, especially attributing and reducing result mapping after
the retained payload/decode cleanup.

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

# V5 implementation review — 2026-09-04

**D0 is not passed.** The [native continuation](native_session_2026_09_04.md)
records the subsequent lifecycle diagnosis, browser-found editing corrections,
refreshed local evidence, and the older Dune composer in the sibling worktree.

**Original local checkpoint:** The new Flutter workbench is implemented and the local
functional checks are green. Attended native input, a valid production-path
performance/lifecycle run, and final binary verification remain open. The Mac
locked during the native work; the UI tool requires manual unlock. An unlock
request is pending. No timing from that condition qualifies the candidate.

This is uncommitted work on `v5/m2-kernel`, based on `9fcb092`, including the M2
corrections already in the checkout when this pass began. No commit, push, CI,
merge, release, or physical-phone result is claimed. The machine-readable local
record is [candidate.json](receipts/2026-09-04-local/candidate.json).

## Semantic checkpoint

Return now splits authenticated inline owners, preserves the container, and
keeps whitespace outside delimiters where Markdown requires it. Delete-to-empty
preserves the starting typing context; outside input stays outside. Partial-owner
and cross-row replacements reject atomically. Joining matching styled owners
removes adjacent delimiters as one closure. Rust supplies the innermost prefix
for empty nested containers instead of making Dart rediscover Markdown.

ATX and setext heading lifts preserve inline formatting. Table Return moves to
the same column in the next row, then creates a real paragraph separator on
exit, including when trailing gaps already exist. Cell-boundary deletion rejects
without damaging table syntax. Pending style combinations compose correctly;
composition cancellation restores the original pending intent and history.

The first semantic family produced 58 failures before correction. Direct cases
now check each repeated command and its immediate next input. Generated histories
run uninterrupted with occasional natural Undo/Redo. Redundant projection-against-
itself checks were removed. Positional caches are shared by selection-only
snapshots, and already legal anchors no longer enumerate every grapheme.

The final 16,694-byte dense facade diagnostic measured insert **2.20 ms p99**
and Backspace **1.96 ms p99**, below the 4 ms local kernel gate. Its 769 blocks
and 2,144 runs are explicitly admitted by the diagnostic, above the workbench's
provisional shape limits; this does not qualify that fixture for live workbench
editing. The run is recorded separately in
[kernel_benchmark.txt](receipts/2026-09-04-local/kernel_benchmark.txt).
It is a headless facade measurement, not input-to-raster or device qualification.

## Host and application checkpoint

`packages/flark_flutter` owns input connections, focus, clipboard routing,
TextPainter geometry, row painting, selection, and source inspection. The
Dart kernel owns canonical source, immutable publication, revision rejection,
composition history, and byte/shape admission. Flutter has no second Markdown
recognizer or parser reconciliation protocol.

Useful failures found and corrected during this pass:

- A composition update published new source with an old composing range.
  Controller batching now publishes one coherent value; native cancellation
  back to the starting source restores its original formatting intent.
- A rejected platform value was not always resynchronized, and duplicate
  acknowledgments published redundant state. The bridge sends the canonical
  value once when correction is required.
- Deferred clipboard work could outlive its document. Controller identity and
  revision checks now reject stale cut/paste, including equal-revision replacements.
- Burst vertical input and line-edge commands read the previous layout's
  selection/source. Geometry now prepares the current generation before navigation.
- Caret reveal happened after the proving frame, including on width-only reflow.
  The owning viewport is corrected during layout. Paint observations report
  only a caret that was actually drawn; scrolling away can intentionally hide it.
- Accessible range endpoints could cross a formatting closure and reject a valid
  replacement. Forward and backward visible selections now map to the appropriate
  inner source anchors.
- Excessive nesting could put text beyond the viewport. Quote/item depth is now
  part of admission, and admitted indentation fits the available text width.
- Normal exit could precede the latest queued draft save. Writes are serialized;
  exit awaits completion and cancels if saving fails. Selection alone does not
  write. This remains prototype preferences storage, not crash-proof document storage.
- The workbench overflowed at narrow widths, and its raw inspector bypassed the
  source paging bound. The header now adapts, and inspection shares bounded pages.

Source editing and inspection lay out at most 4,097 UTF-16 units or 128 physical
line breaks per page. Pages preserve source exactly, including CRLF and surrogate
pairs; normal grapheme boundaries are preferred. The controller still owns the
whole document and global offsets. A pathological grapheme larger than a page
cannot let an accessibility replacement spill into an adjacent page.

Two deliberate raster faults were calibrated: omitting glyph paint produced zero
non-background pixels, and removing italics made the compared rasters identical.
Both tests failed as intended; the production implementation was restored before
the full verification run. Navigation, reflow and selection regressions also
failed against the preceding implementation before their owning-layer fixes.

## Current local evidence

| Check | Result |
| --- | --- |
| Dart kernel analysis and full suite | No issues; 217 tests passed |
| Generated histories | 1,000 sequences of 40 commands, seed 2026, passed |
| Flutter host analysis and full suite | No issues; 72 tests passed |
| Workbench analysis and full suite | No issues; 11 tests passed |
| Rust release tests | 38 passed |
| Native / bundled Wasm / rebuilt Wasm | Identical render models across all 1,322 cases |
| Generated schema | Fresh; regeneration changes no output |
| Rust-free fresh consumer | Passed using the current rebuilt native artifact |
| Production source size, physical lines | Rust 1,734 / 3,000; Dart 4,056 / 8,000; Flutter 1,767 / 10,000 |
| Clean normal app builds | macOS profile and Flutter web/Wasm; hashes in candidate record |

The expanded public kernel cap is 29 concepts: the original 27 plus shared
admission limits and rejection reasons, both required by this host. Revision
and composition methods remain on the existing facade. Cached TextPainters are
host layout objects keyed by current presentation; they are not semantic state.

An earlier browser smoke loaded both Flutter's `main.dart.wasm` and the packaged
parser Wasm and exercised real browser typing, delete-to-empty, list exit and
Undo. It predates the final host corrections and is not final-browser qualification.
The final build must be rechecked through actual browser input after unlock.

## Performance and remaining handoff

The provisional workbench envelope is 32 KiB UTF-8, 1,024 physical lines,
512 blocks, 2,048 runs, 4,096 UTF-16 units per line/leaf block, and eight nested
quote/item containers. Writable source is capped at 256 KiB. These are candidate
limits, not a sealed promise. The 32 KiB D0 floor and 16.667 ms input-to-raster
budget have not been waived. The declared source-mode opening, latency and
lifecycle budgets still need a concrete measured profile before D0 can close.

The profile uses the production workbench, including persistence and application
UI, and matches each required post-input paint to its engine raster timestamp.
Its bounded fixtures cover prose, dense blocks, lists, tables, eight-level nesting,
unique references, and Unicode, each near the start, in the largest block and at
the end. Headless fixture tests prove that the insert/delete pairs really reach
32 KiB and remain admitted. Original unconstrained stress fixtures remain separate
and are not silently substituted. This harness prepares the run; it does not
supply a passing native receipt by itself.

The earlier native diagnostics included apparent dense/list budget misses, then
much slower frames while locked. Foreground/display provenance was insufficient;
those numbers cannot establish either a passing envelope or the intrinsic cost
of the final candidate. Rerun with the Mac unlocked, the exact app foregrounded,
and no lock transition. The normal rebuilt app replaces the profiling binary.
The native asset naming warning was investigated: both the app executable and
packaged parser library contain arm64 and x86_64 slices.

The next work is the attended macOS session: real AppKit character/key/menu/
clipboard routes, focus recovery, selection, scroll/resize, sustained editing,
open/close and document switching, followed by the complete performance/lifecycle
profile. Rebuild and open the exact recorded normal binary after any fixes.
Do not ask the owner to discover known B0/B1 failures as a substitute for this work.

Dune integration is separately unresolved: `/Users/dan/Coding/dune/lib` is empty
in the inspected checkout, and the intended composer path was requested. No
available floor Android has been qualified. Phone/CJK proof, images beyond alt
text, link/image popovers, highlighting, the deferred collapsed-formatting UX
experiment, Fleury, full web qualification and two weeks of owner dogfooding
remain explicit later work. Current collapsed formatting behavior remains the
existing documented whole-owner toggle.

The implementation has moved forward substantially. M2's named-commit CI gate,
M3's native/device gates, and D0's zero-B0/B1 handoff rule remain intact.

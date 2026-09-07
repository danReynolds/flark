# Flutter code-region integration — 2026-09-06

The Flutter workbench now opts into Tree-sitter editing and background coloring
for Dart, JavaScript, Python and YAML. The source/selection/history boundary and
automated host checks pass. Milestone 3 stays open for hands-on visual acceptance,
full-frame profiling and physical devices. Browser control reported a locked Mac
during this pass, so this document does not claim a visual dogfood session.

Follow-up: the [2026-09-07 browser review](BROWSER_DOGFOOD_REVIEW.md) records the
completed hands-on session, three host fixes and updated test/build receipts.
The full-frame and physical-device gates remain open.

## Ownership and scope

`flark` accepts an optional pure Dart `CodeEditingDelegate`. It does not acquire
a dependency on Tree-sitter, a Flutter API, or a second document model. The
`FlarkTreeSitter` adapter in `flark_flutter/code.dart` translates that small
request/proposal contract to the standalone `flark_tree_sitter` package. Flutter
loads and warms the synchronous editing queries before accepting input.

The kernel supplies only the projected code body, language and UTF-16 selection.
It maps returned edits through parser-owned content ranges, keeps each existing
line's Markdown prefix during Tab/Shift-Tab, adds the current line's prefix to
new lines, and uses the document's existing newline convention. One kernel
transaction owns source, selection and undo. Composition and paste do not invoke
automatic outdent. Multiline snippet paste retains literal indentation and adds
only the Markdown prefixes needed to remain inside the current fence.

Each opted-in controller owns one coloring worker. The caller separately owns
the synchronous analyzer. Workers start outside input callbacks and terminate on
controller disposal, including disposal during startup or pending work. The
active fence and visible fences take priority; an LRU holds at most 32 snippets
and 65,536 UTF-16 units. The worker queue still holds at most one running and one
pending request. Visible rows are updated after layout and scroll.

Results are immutable and valid only for exact code text and language currently
requested by the projection. This permits reuse across undo and identical
snippets without retaining absolute Markdown positions. Source-mode transitions,
language changes, deletion, superseding input and disposal invalidate pending
adoption. Color completion notifies the render surface directly; it cannot create
an editor revision, history entry, new input value, scroll reveal or save.
Colors only change foreground color, preserving all font metrics. Current code
paints plain while waiting; worker failure retains that behavior and is exposed
through `FlarkCodeColors.failure`.

Existing automatic language detection and the other language choices remain
available through the previous lightweight service. Explicit info-string choices
are authoritative. The new package still has four qualified grammars, not twenty.
Indented code blocks and virtual-space projections retain the previous editing
path. The workbench's document/shape admission limits are unchanged.

## What testing found

1. The browser render test caught missing theme support for Tree-sitter's YAML
   `property` capture. The parser returned valid scopes, but the entire example
   looked plain. The theme now handles property/tag/constant/boolean/constructor
   scopes alongside the existing token categories.
2. A multiline paste into a quote/list-contained fence could lose the prefixes
   after the first line. A failing authoring test reproduced it; the shared code
   translation now preserves the container and literal pasted indentation.
3. The legacy unknown/plain-language Enter path could indent after a brace
   without any recognized language. It now preserves indentation only.
4. Reviewing the bounded worker scheduler exposed potential starvation for a
   fence beyond the first 32. The surface now supplies its current viewport; a
   test scrolls priority to the 40th fence while the initial request is pending.

The first two are integration/test-methodology gaps, not obscure parser cases.
Correct Tree-sitter spans did not prove that the theme used them, and correct
snippet edits did not prove that Markdown containers survived clipboard input.
The response is to keep the same authoring scenarios at the source, input and
paint boundaries, rather than add only more parser examples.

## Evidence

The final local gate comprises:

- 434 Flark kernel tests.
- 376 Flutter host tests, including the authoring corpus wrapped in ordinary,
  quote/list and CRLF fences; exact source/caret, following character, reverse
  selection, undo/redo, paste and composition assertions. One component case
  uses a caller-specified unit that differs from the host's indentation policy
  and remains a component-only case.
- 25 workbench tests covering input, layout, saving and admission boundaries.
- 9 actual Chrome browser tests: four with the real Tree-sitter Wasm backend,
  Web Worker and Flutter input-controller/paint loop, plus five existing browser
  clipboard/input-context cases. This is Flutter's debug/DDC browser runner.
- Controlled delayed-worker tests assert the very first edited frame, later
  colored frame, unchanged caret/selection geometry, exact snapshot identity,
  no extra revision/history entry, stale language/source rejection, source-mode
  transitions, deletion, failure and disposal. A real native isolate colors
  multiple fences and is disposed with an edit pending.
- Strict Dart analysis, Flutter analysis and the workbench `flutter build web
  --wasm` build. Flutter still emits its existing CupertinoIcons font warning;
  no claim is made that the build is warning-free.

Logs and the source/build manifest are in
[`receipts/host-2026-09-06/`](receipts/host-2026-09-06/). Browser timing lines record
load/warm time and individual small-snippet input/frame waits. They are smoke
measurements from a debug runner, not a p99 release budget or raster timing.
In the four samples, colors were available by the first observed edited paint;
that does not establish absence of flicker during sustained typing.

Reproduce with the root pinned Rust toolchain on PATH (including matching
`RUSTC` and `RUSTDOC`), Dart 3.12.2 and Flutter 3.44.4:

```sh
# In packages/flark
dart analyze --fatal-infos
dart test
# In packages/flark_flutter
flutter pub get
flutter analyze
flutter test
# In packages/flark_flutter/example
flutter test
flutter test --platform chrome test/tree_sitter_web_test.dart test/web_input_test.dart
flutter build web --wasm
```

## Review decision and next gate

Keep the four-language host integration as the next candidate. The separation
remains small: one synchronous edit delegate, one controller-owned decoration
service, and the existing source/history authority. No grammar fork, language
server or JavaScript editor is introduced. The legacy service is an explicit
migration fallback, not evidence of twenty-language Tree-sitter support.

Before closing milestone 3, use the normal workbench to inspect cold startup,
repeated typing and deletion near its admitted code-block limit, auto/manual
language changes, mouse selection, Tab/Shift-Tab, quoted snippet paste, undo,
composition and document switching. Inspect actual color transitions for
whole-fence flicker and verify scrolling to passive fences. Then measure full
input-to-raster behavior in release/profile builds and retain separate physical
device, CSP/custom asset-routing and Fleury gates. Expand languages only after
that review; green automated tests alone do not close it.

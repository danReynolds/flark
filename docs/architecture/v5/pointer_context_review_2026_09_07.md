# Pointer formatting context — 2026-09-07

Owner report: exit a bold word, click back at its end, then type. The new text
was plain instead of continuing the bold word.

## Finding and decision

A mounted regression reproduced the failure by clicking 0.75 logical pixels
to the right of the caret at `say **what** next`: the selected source offset
was 12 (outside bold) instead of 10 (inside bold). Both offsets paint at the
same visible position. The surface chose the source anchor solely from which
side of the caret the pointer hit.

This was an unsuitable interaction rule, not just missing regression coverage.
The old `surface_contract_test.dart` explicitly required distinct contexts for
hits one pixel apart at a word edge. That test faithfully enforced a rule the
user could not perceive or predict. The preceding whitespace fix did not
introduce this behavior.

The active editing profile now chooses the word's context when the nearest
caret is at a word edge beside whitespace or a row boundary. Clicking after a
following plain space chooses plain text. Between adjacent non-whitespace
glyphs, the hit half can still distinguish their formatting. Keyboard movement
and explicit formatting toggles retain their existing rules.

The document owns this decision in `pointerAnchorAt`; both the kernel pointer
command and Flutter's geometry adapter call it. It uses projected characters
and parser-owned source anchors, without recognizing Markdown in the host.
The old surface-contract test now covers the remaining ambiguous case between
adjacent non-whitespace styles. New word-edge tests express the revised user
expectation instead of retaining the old hidden-affinity behavior.

## Local validation

- 660 kernel tests passed, including a seven-style pointer-context matrix over
  prose/quote and terminal/followed-by-space cases, with next input and history.
- 28 mounted pointer tests passed, including actual mouse hits to either side
  of styled word starts/ends, narrow layout, plain continuation after spaces,
  and double-click selection. Assertions cover source, typing style, caret and
  every edited paint. The complete host suite passed 824 tests.
- Core and host analysis passed; all eight Wasm browser input regressions
  passed; the normal `flutter build web --wasm` build and diff checks passed.
- In the rebuilt release web app at 1280 × 720, actual keyboard shortcuts
  authored `This is **what** next`. Clicking just beyond the bold glyph's end
  and typing `X` produced `This is **whatX** next`, visibly bold. Clicking after
  the following space and typing `Y` produced `This is **whatX** Ynext`, with
  plain `Y`. Undo/Redo retained that result; no console errors/warnings appeared.

Changes are local on `codex/v5-editor-qualification`. CI remains skipped at the
owner's request. This is a pointer behavior correction, not completion of the
outstanding native timing/lifecycle or physical-device qualification gates.

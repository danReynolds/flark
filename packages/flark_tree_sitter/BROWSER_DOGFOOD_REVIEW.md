# Browser code-region dogfood — 2026-09-07

The four-language candidate is ready for another exploratory browser session.
Hands-on work in the normal release/Wasm workbench found three host defects;
all three are fixed and have regressions demonstrated failing before the fix.
This completes the previously blocked browser exploration, not milestone 3's
full-frame, sustained-color-transition or physical-device qualification.

The subsequent [owner input follow-up](../../docs/architecture/v5/owner_code_input_review_2026_09_07.md)
records Ruby highlighting, scoped Select All and the browser keyboard clipboard
route uncovered in the next owner session. Its receipts supersede the test counts
below for the current candidate.

## Findings and corrections

1. **Code used a proportional fallback on Flutter web.** Requesting the family
   `monospace` did not load a monospace face. Eight `i`s and eight `W`s visibly
   occupied different widths; an actual browser paint/caret regression measured
   a 78.8745 px difference. The Flutter package now bundles unmodified Roboto
   Mono under a package-qualified family for fences, inline code, source editing
   and source inspection. Its upstream revision, hashes and SIL Open Font License
   are retained in `../flark_flutter/lib/assets/fonts/`. The same browser
   regression now passes in rendered and source modes. Its negative control
   verifies that the test is using real fonts rather than Ahem. A release build
   and visible equal-column check separately verify automatic asset loading.
2. **Reselecting the current language displayed an edit rejection.** The picker
   submitted an unchanged language command, whose no-op result became “This edit
   needs source mode.” The picker now dismisses that choice without issuing a
   command. The regression preserves exact snapshot identity and revision with
   no notice; hands-on reselecting Python preserves the caret and next character.
   Actual changes retain the existing controller/revision protection.
3. **An empty or undetected snippet displayed `Auto · null`.** The detector uses
   an empty string for no match; the toolbar treated every non-null result as a
   known display label. It now appends only a catalog label that exists. Empty
   and one-character cases show `Auto`, while the existing detected-JSON case
   still checks the positive path. The final release build was checked visually.

These are test-methodology gaps, not obscure grammar edge cases. The font test
previously checked the requested style, and synthetic equal-width fonts hid the
actual glyph problem. The language picker covered successful changes but missed
ordinary no-op and undetected states. The corrective scope stays in the Flutter
host; no language engine exception or grammar fork was needed. Keep real-font
geometry in the browser lane and include empty, unchanged and unknown states in
authoring journeys before expanding the language catalog.

## Actual browser journeys

The Codex embedded browser ran the normal `flutter build web --wasm` workbench
at an isolated loopback origin, using synthetic documents so the user's drafts
at port 8813 were not edited. Screenshots and accessibility observations were
inspected during the session; these are recorded observations, not an archived
video or frame-timing trace.

- Dart typed `}` removes body indentation; the following character, undo and
  redo keep the expected source. Backward mouse selection inside the fence is
  visibly painted; copying gives the selected body. Tab/Shift-Tab retain the
  selection, and replacing it followed by Enter continues inside the fence.
- Automatic/manual language changes include plain text, Dart/Python and
  returning to Automatic. Reselecting the current language is harmless.
- A quote/list-contained Python `else:` aligns with its `if`; Enter then
  `recover()` keeps both indentation and Markdown prefixes. Multiline paste
  adds `retry()` and `finish()` under the same quote/list fence, preserves the
  following paragraph, accepts the next character, and survives undo/redo.
- YAML `settings: |` followed by Enter indents scalar content two spaces;
  subsequent Enter preserves it. JavaScript Enter indents a body, and typing
  `}` on its indented blank line aligns it with the opener.
- Creating a fence on a blank line inserts its own closed region; the next
  character stays inside, and following prose remains outside. Two Up keys
  move from the following paragraph through consecutive empty lines; the next
  character lands on the expected earlier empty line.
- A 3,895 UTF-16-unit Dart body (near the workbench's 4,096-unit block limit)
  accepts 36 successive individual keypresses, 36 Backspaces and a following
  character. Full-document copy equals the expected exact source. The observed
  final views retain caret placement and code colors.
- A 45-fence document colors visible fences 41–44 while the caret is in the
  following prose, then colors passive fences 7–11 after scrolling upward.
- Switching Tour to Draft, editing Draft, returning to Tour, and cold reload
  retain the exact saved Tour source, verified through full-document copy.

## Automated evidence and boundaries

Final current-tree checks: **379 Flutter host tests, 25 workbench tests and
10 Chrome browser tests (414 total)**, clean Flutter analysis, and a successful
release/Wasm build. The browser lane includes the new real-font test, the real
Tree-sitter Wasm/Web Worker input-and-paint tests, and clipboard/input-context
tests. The build still reports the pre-existing CupertinoIcons font warning.
Logs, expected pre-fix failures and source/build hashes are in
[`receipts/browser-2026-09-07/`](receipts/browser-2026-09-07/).

No kernel or Tree-sitter implementation changed in this pass. The receipt checks
their previously recorded source hashes against the current tree; the prior
434-test kernel receipt remains historical evidence, not a claimed rerun today.
Chrome tests use Flutter's debug/DDC runner; hands-on checks use the release
workbench. The font browser test loads the actual face explicitly because the
test server does not perform a normal application's font-bundle startup.

Keep the four-language architecture and proceed with exploratory owner feedback.
Do not expand to twenty languages from these results alone. Repeated-input
screenshots cannot rule out intermediate whole-fence color flicker and do not
measure input-to-raster p99. Full-frame profiling and sustained transition
capture remain the next engineering gate, alongside separate real IME,
macOS/phone, CSP/custom routing and Fleury qualification. No D0/native or
production-performance milestone is closed by this review.

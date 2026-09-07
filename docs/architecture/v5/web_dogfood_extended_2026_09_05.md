# Extended browser dogfood — 2026-09-05

The deeper pass found eight gaps after the earlier green browser handoff. Six
came from ordinary interaction in the normal workbench; two parser extraction
errors came from another generated-history seed and were minimized into direct
editing regressions. All eight are corrected in the local candidate recorded
in the [receipt](receipts/2026-09-05-web-extended/candidate.json).

This supersedes the earlier September 5 candidate for exploratory D0-web use.
It does not close D0-macOS, phone-floor, IME, accessibility, other-browser or
numeric web-performance qualification.

## Findings and ownership

| Failure | Correction and evidence |
| --- | --- |
| Selecting `bold` from outside its closing marker in `before **bold** after` rejected replacement. | One kernel range-normalization helper maps hidden boundary endpoints to the selected visible content. Insert, paste, explicit replacement, deletion, Return and style toggle share it. Tests retain original selection/history and check the next character. |
| Double-click left a collapsed caret. | The Flutter host selects the word using its actual laid-out text, then maps that range through the projection. Mounted mouse tests check selected pixels, replacement paints and Undo for strong, code and link text. |
| Option-Backspace removed one grapheme instead of a word. | The host sends a word-delete flag through the existing deletion commands. The kernel uses the same word boundaries as navigation, in rendered and source modes. |
| Deleting `x` from `**two x**` exposed literal formatting because `**two **` is no longer strong Markdown. | Shared deletion moves newly exposed edge whitespace outside parser-owned emphasis delimiters. Surviving words stay styled, and the following character retains the original typing intent. Both directions and nested styles are covered. |
| Punctuation after a multiline link could reuse the preceding link's closing byte as its own text range. | Rust extraction authenticates relocated text after the previous sibling's end, even when repeated punctuation matches the wrong slice. Link/image, quote/root and LF/CRLF regressions cover the following break as well. |
| A GFM bare URL ending in `>` was assigned a hidden closing bracket without an opening `<`. | Rust strips autolink brackets only as a pair. Exact source/content ranges, projection, the next key and Undo are checked. |
| Browser copy of visible `two` from `one **two** three` pasted `two**`; browser defaults exported the source input mirror. | A focused-editor browser copy/cut binding writes rendered selection text to the clipboard event and prevents the raw DOM default. Source mode keeps exact syntax. Cut executes one existing kernel deletion. Real browser event tests check payload, paints, next input, history and listener disposal; normal-app keyboard copy/cut/paste also passes. |
| Command-Down did not reach document end after reload. | The host maps Command-Up/Down and Control-Home/End to existing source selection commands. Shift preserves the selection base. Mounted source/rendered tests check the next typed character, actual paint, full-range replacement and Undo. |

The larger change is the shared deletion rule, rather than separate fixes for
Backspace, Delete, cut and empty replacement. It only moves whitespace around
authenticated emphasis/strong/strike owners; it does not recognize Markdown in
Dart or introduce a second browser editing model. Rust remains the authority
for whether the resulting Markdown has the claimed structure.

Leading whitespace needs an explicit caret rule: deleting the initial `x` in
`**x two**` yields ` **two**`; the nearest legal caret is inside the surviving
owner, so the next `y` yields ` **ytwo**`. The trailing case yields `**two** `
and the next `y` yields `**two** **y**`. These are direct contract cases, not
post-paint repairs. Arbitrary partial cross-owner transformations remain outside
the declared edit profile.

## Verification

- Core: **365 tests**, including **1,000 generated command histories with seed
  2027**. Both newly discovered parser cases have ordinary named regressions.
- Flutter host: **106 tests**; workbench: **25**; real Chrome transport: **3**.
  Total Dart/Flutter/browser tests: **499**.
- Rust: **41 tests**; native/bundled Wasm identity across **1,322 cases**, plus
  a fresh Wasm rebuild. The new parser asset is included in the normal app.
- All three analyzers and the normal `flutter build web --wasm` pass.
- Meaningful pre-fix failures are retained for boundary selection, pointer
  selection, word deletion, both parser defects, browser clipboard and document
  navigation. Clipboard's negative control disables only its browser binding.

Normal-app checks use the Codex embedded browser at 1280 × 720 on the recorded
M1 Pro/macOS configuration. They combine visible pixels with exact expectations
for the complete source in the bounded DOM input context; every new fixture is
shorter than that context. They cover replacement, the following character,
Undo/Redo, actual keyboard copy/cut/paste, word deletion, selected word pixels,
multiline-link punctuation, the bare URL, ordered-list continuation and exit,
retaining a quote when exiting its nested list, Unicode grapheme deletion,
source-mode clipboard, reload and continued editing. The receipt separates
checks before and after the final normal rebuild.

The automation's instantaneous `clickCount: 2` is below Flutter's 40 ms minimum
double-tap interval. Two actual clicks separated by 70 ms selected the word
(111 ms total measured call time), then replacement, the next key and Undo
passed. This is a test-actuator constraint; it is not an editor delay or a
reason to weaken the gesture regression. A stale preview tab was replaced after
the server restart; the final candidate was freshly loaded.

## Reflection

The earlier readiness statement was too broad for the interaction coverage it
had. This pass is useful evidence of the exact V1–V4 risk: green tests and a
reasonable tour can still miss common transitions. Confidence should come from
deliberately varying how a selection was reached, deleting toward whitespace,
typing the next character, using the real platform clipboard and changing the
generated discovery seed. It should not come from the increased test count.

Keep this exploration-and-minimization loop as a recurring milestone activity.
Fix shared semantic rules in the kernel, extraction errors in Rust and delivery
or geometry in Flutter. Add the smallest regression at the layer that missed
the failure, then repeat the normal user interaction. Do not turn each finding
into another repair stage or a separate fixture/replay framework.

The candidate remains an exploratory browser handoff. The existing native B1
performance miss still needs its complete foreground rerun and normal AppKit
canaries. Narrow-browser resizing was not qualified; mounted responsive tests
do not replace that evidence. No native application was driven in this pass.

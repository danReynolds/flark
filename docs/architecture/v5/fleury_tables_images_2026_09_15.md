# Fleury tables and image previews — September 15, 2026

## Scope and ownership

Flark's existing parser and kernel remain the source/selection/history authority.
The Fleury host now lays multiple projected table cells out on one visual row,
with shared column widths, alignment, wrapping and rules. Every cell fragment
retains its projected row and UTF-16 offsets. Painting, pointer placement,
vertical movement, caret reporting and resource targets consume that geometry.
At very narrow widths, cells stack in source order with column numbers.

Images retain editable alt text and reserve fixed cell rectangles. Standalone
top-level images place their preview first, with a centered alt-text editing
line directly below it, shown only while the resource is active. Its reserved
geometry keeps entering/leaving the image from moving the following paragraph.
Inline images and images inside containers retain their projected text row.
The host mounts only visible previews (up to eight), using Fleury's existing
Image widget for pixel placements or terminal glyph rendering. Default HTTP
loading is byte/pixel/time bounded, first-frame only, and disposed on unmount.
A stable destination/occurrence key prevents unrelated source edits from
restarting the request. Applications can supply `imagePreviewBuilder`; theme
fields control table headers/borders and preview height. The playground's
bundled Flutter-logo PNG works without third-party network availability.

Tab, Shift-Tab and Enter match Flutter's table behavior. Image actions use the
shared guarded resource sessions and commands. Removing an image removes the
whole resource; undo restores it. Markdown is never recognized with host regexes.

## Dogfooding findings

The browser pass found two Fleury image-composition gaps that plain text/layout
checks could not expose:

1. A later popover did not occlude an already placed pixel image. The image
   compositor now exposes visible placement slices while preserving the existing
   recorded-placement API. Slices retain the original fit box and offsets.
   Cropped scratch copies, web and terminal presenters consume the visible list.
   Holes are preserved before a subsequent image paint so older image pixels
   cannot reappear beneath a later image; ordinary overlapping image paint order
   is preserved.
2. Image overlay cells discarded the underlying background. Letterbox areas
   therefore used the browser's initial dark background even in the light theme.
   Overlay cells, cached/scratch composition and DOM span conversion now retain
   the background, and ANSI diffing emits background changes even when both
   frames contain an image. The buffer fix alone passed its tests while the
   browser still showed dark bands; the final DOM boundary needed its own
   assertion and another visual check.

These are framework changes in an isolated checkout, not Markdown rules or
Flark-specific browser CSS patches.

### Follow-up on the four browser comments

- The original image label was link-styled at the left edge while its preview
  was centered. Standalone images now use the image itself as the primary
  action target. The editable alt label is centered directly below the image
  when active, with ordinary text styling. Click opens the existing resource
  controls; source mode retains exact Markdown. This follows the image-first
  direction documented by [Typora](https://support.typora.io/Images/) and
  [Obsidian](https://obsidian.md/changelog/2026-07-14-desktop-v1.13.2/), while
  retaining Flark's own guarded edit controls.
- Trailing table delimiter padding appeared as projected gap segments after
  Comrak's inline leaves. The host no longer gives those gaps painted width.
  Padding clicks and End resolve to visible content's edge; original source
  whitespace remains intact. Parsed whitespace inside a real inline run is
  retained. Left, center and right alignment and bold boundary behavior have
  direct edit/undo tests.
- Tasks retain the original `[ ]` / `[x]` appearance. Each list shares a
  marker gutter sized from its parsed items. Bullets are centered alongside
  checkboxes; labels and continuation lines share their start column. A
  separate list keeps its own width. All three checkbox cells are actionable.
  The intermediate Unicode replacement was an unnecessary visual regression: a
  one-cell glyph aligned the columns but rendered much too small in this font.
- The intermediate quarter-cell inset balanced code against the font bearings
  by shifting its background. The subsequent block-alignment review identified
  that this moved the outer edge relative to prose; the final model below
  replaces that approach.
- Image previews currently use centered `ImageFit.contain`; Fleury Image has
  no alignment option. A future left/center/right option belongs in presentation
  settings, including the fitting/placement contract across pixel surfaces.
  Per-image alignment is absent from standard CommonMark image syntax and would
  require an explicitly chosen extension or application metadata.

The initial test pass emphasized bounded geometry and successful operations.
It missed the product expectation that padding clicks must append without a
space, and that alt text should not read as a disconnected second resource.
The follow-up adds those visible editing expectations to the regressions.

## Shared block alignment follow-up

The latest change uses one containing edge for prose, quote rails, code surfaces,
and table frames. Internal content insets remain deliberate and themeable.

- Fleury `listIndent` supplies a stable minimum gutter across separate lists,
  including lists with and without tasks. Numbered labels can expand it.
  `quoteIndent` controls the quote rail and content gap independently.
- Code backgrounds remain at the containing edge and reserve horizontal padding
  on both sides. Optional `codePaddingRows` adds decorative top/bottom rows with
  fractional fill; those rows never become new caret positions. The example uses
  one column and a quarter row, which balances the observed font's bearings.
- Tables use outer-edge frame glyphs, with their internal rules inset from the
  frame. Fleury's existing block-element renderer gains the four standard
  Unicode edge corners U+1FB7C–U+1FB7F; no Flark-only CSS is introduced. Cell span
  tests cover their surrogate-pair encoding and exact two-rectangle painting.
- Nested code preserves the quote rail outside the code fill. Table continuation
  prefixes now preserve enclosing quote rails too.
- Flutter shares the same relationships in logical pixels. Quote rails start at
  the containing edge, bullets/checks center within a common gutter, code row
  spacing stays outside its symmetric padded background, and table strokes stay
  inside their rectangles. Nested tables respect the enclosing content indent.
  New `quoteIndent` and `listMarkerGap` theme metrics expose these decisions.

`test/fixtures/host_block_alignment.md` is consumed by both hosts. Fleury checks
painted cells, marker centers and pointer edits. Flutter captures actual pixels
to check the code, quote and table edges plus checkbox/bullet centers, and checks
first-frame caret/pointer editing and undo. A separate nested Fleury case covers
quote rails through code padding and table rows. These are local host receipts,
not native input/IME qualification.

## Dependency closeout — September 16

The framework changes were reapplied to current Fleury main `d6701677` in an
isolated worktree, preserving upstream fixes and rebuilding the embedded browser
client from the combined sources. Review also caught and fixed an image-only
invalidation case beneath a later image. The tracked host and example pins now
use `41967d499a6bd0fe53e5f5f07b31649df0adcc83`. Ignored local dependency overrides
were removed. The original Fleury checkout and unrelated work remain untouched.

The owner authorized review and merge with local validation and CI skipped.
Clean-checkout verification and landing details are recorded in
`fleury_support_closeout_2026_09_16.md`. Earlier receipts below describe the
implementation stages; they are not the final dependency-installation receipt.

## Verification

Receipts below are local; they do not establish physical IME, mobile-device or
terminal graphics protocol qualification. The new host scenarios assert the
first edited frame. Only image loading checks await asynchronous work.

- Host: shared columns/alignment, wrapping, wide characters, explicit empty
  cells, pointer edits, undo, Tab/Shift-Tab/Enter, vertical movement, cross-cell
  selections, links in cells, images in cells, image actions and source mode.
- Image loading: success, failure, byte/pixel limits, stale completion, disposal,
  stable caret geometry and no reload after inserting text before an image.
- The existing generated host geometry matrix now checks fragments in each
  visual row and still requires complete contiguous coverage of each projected
  row's text, bounded glyphs/hits and an addressable caret.
- Framework: image occlusion/cropping/background regressions, existing buffer
  and diff tests, browser-host tests and required local performance/wire gates.
- Browser: table click/Tab/typing/undo, image popover and edit form, missing-image
  error without reflow, light/dark themes, responsive layout and preview clipping.

The first broad framework run exposed two API-contract regressions in the
initial compositor draft; keeping recorded and visible placements separate fixed
both. PTY/CLI timeouts in the concurrent broad run passed when rerun serially.
The embedded remote client is rebuilt when its source fingerprint changes.

### Local receipts

- Initial Flark Fleury: analysis with fatal infos clean; 77 tests passed. The seven
  table/image scenarios were rerun after adding an assertion that Open dispatches
  a relative image URL against the application's base URI.
- Example: analysis with fatal infos clean; all 5 tests passed; Dart-to-JS build
  completed with the parser/highlighting Wasm assets and bundled PNG.
- Fleury: 3,535 core tests passed, one existing skip. All five image-composition
  regressions passed again after the final DOM-span fix.
- Fleury web: 222 VM tests passed earlier in this change; the final focused
  headless Chrome run passed 28 span, DOM-row and serve-DOM parity tests.
- All eight fast performance gates, wire gate and live serve-wire gate passed.
  The 11 PTY/CLI cases that timed out under the earlier concurrent workload
  passed when rerun serially.
- Changed-file formatting and both repository whitespace checks passed. Core
  analysis still reports four pre-existing brace-style infos in the buffer's
  bounding-box loops; there were no new errors or warnings.
- Browser dogfooding covered typing/Tab/undo in cells, image editing and a failed
  URL followed by undo, popover occlusion, dark/light backgrounds, preview
  clipping and a 420 by 760 viewport. No console warnings/errors were reported
  on the final rendering candidate. Image Open dispatch is covered by the host
  callback test; opening a new external window was not independently confirmed
  by the embedded browser.

The four-comment follow-up passed 79 host tests and 5 example tests, with
fatal-info analysis clean in both packages. Browser checks covered padding-click
typing (`YesX`, with no added gap), undo, checkbox toggling, direct alt-text
editing, image actions, light/dark themes and the 420 by 760 viewport. No new
framework change was needed in this follow-up; the framework receipts above
belong to the prior renderer fixes.

The next padding/checkbox follow-up passed 81 host tests and 5 example tests;
fatal-info analysis is clean in both packages. Added first-frame checks cover
mixed-list gutters, nested and separate lists, all three checkbox hit cells,
code-edge painting through wrapping, pointer insertion and undo. Browser
inspection at 1073 by 899 and 420 by 760 checked dark/light appearance,
checkbox toggling and code-edge insertion/undo. This follow-up uses existing
Fleury block-element painting and requires no additional framework changes.

The shared alignment follow-up passed 812 Flutter host tests and 31 Flutter
example tests. Fleury's host suite plus the additional nested alignment case
cover 83 tests, and its example passed all 5 tests. The 27 cell-span/CSS checks
passed on both Dart VM and headless Chrome. All eight fast framework gates, wire,
live serve-wire and rebuilt bundle-size gates passed; the embedded client
freshness check passed. No new native input or terminal-device proof is claimed.
Browser checks covered dark/light themes at 1073 by 899 and 420 by 760, code-edge
insertion/undo, moving across decorative code padding, checkbox toggling,
table padding-click append/undo and image preview clipping. Flutter's rendered
fixture was also captured and visually inspected.

Final sample candidate: `http://localhost:8820/revisions/80cc1444ecd8/`.
It is running locally on port 8820 and includes both a table and image preview.
No CI was run.

## Remaining limits

This adds table presentation and cell editing, not a spreadsheet UI for inserting
columns/rows or changing alignment. The existing kernel limitation for omitted
trailing table cells remains; explicitly authored empty cells are covered.
Browser CORS and Fleury's raster-format support still apply. Native image output
uses Fleury's capability-dependent renderer and remains a separate device gate.

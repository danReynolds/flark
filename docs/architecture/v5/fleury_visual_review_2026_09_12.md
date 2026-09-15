# Fleury visual dogfood follow-up — 2026-09-12

Base: Flark `3a3af9bf7af7b57c7bc24406fdb6accffe4df6fd` (main, including PRs
45 and 46). Follow-up branch: `codex/fleury-dogfood-polish`. Local-only tests;
no CI run, commit or push in this follow-up. The sibling Fleury checkout remains
at `a9eb740aacc60c8fd8483abaa1aa8ce49f78c3c0` with local changes and ignored path
overrides. This is not a clean dependency installation receipt.

## User observations

| Observation | Resolution / boundary |
| --- | --- |
| H1, H2 and H3 look the same size | Confirmed framework constraint: Fleury has one font size per cell grid, including on the web. The parser retains the heading level. Choosing per-level cell styles versus a Fleury variable-size-text feature remains open; no font-size support is claimed. |
| Clicking a link shifts its text and offers no controls | Link controls and shared guarded resource sessions are now in Flark main. The sibling Fleury DOM renderer fix aligns wide-character spans and inverse caret spans to the same cell box. |
| Code has an unwanted left border | Code bodies use two cells of padding with a filled background. Already in main, visually checked again. |
| Quote border is too thin | Quotes use a quarter-cell block bar, with a one-cell ASCII fallback under width policies that need it. Already in main, visually checked again. |

## Additional defects found by dogfooding

**Standard field keyboard replacement.** Cmd+A in Fleury's TextInput was not
bound, so replacing the destination appended instead. The field-setting action
used by the first test bypassed that input path. Fleury's shared keymap now
defines select-all for Ctrl/Cmd+A, and Command copy/cut/undo/redo aliases.
TextInput and TextArea apply the same action; explicit Emacs Ctrl+A remains
line-start. The resource journey now sends Cmd+A and actual text input.

**Popover resize and custom content.** Placement used the previous painted
width and a guessed six/eight-row height. Narrowing the viewport could move the
popover outside the visible area; a taller custom popover could clip its actions.
A small render adapter now reads the caret from the editor's current layout,
after the editor has painted, and passes the measured child size to Fleury's
`resolveAnchoredOffset`. The adapter stays within the editor viewport and
preserves its local versus screen coordinate distinction. It does not allocate
a second document layout or add document/selection state.

**Light theme contrast.** The example changed code backgrounds but left body
text and much of the page at browser/terminal defaults. The example now supplies
body foreground/background, Fleury's base text style, and a painted page
background. Default accents adapt to light/dark; explicitly chosen colors remain
the user's choice. RGB defaults/custom colors are included in the swatch palette
so the displayed selection is truthful. Copyable theme configuration includes
the body colors. Default resource surfaces derive their text, fill and border
colors from the ambient Fleury scheme.
The field base foreground is supplied to Fleury's interactive-style cascade as
well as its text-style cascade, beneath explicit application styles. Checking
only the dialog title missed the nearly invisible unfocused label field; the
regression now checks that value's actual painted colors too.

## Local verification

- Flark Fleury host: **56 tests**, analysis clean. Includes source/selection
  guards, pointer interaction, code/quote wrapping, actual keyboard destination
  replacement, first-frame popover resize, tall custom content and geometry reuse.
- Fleury example: **4 tests**, analysis clean. Added assertions on painted body,
  code, link, popover, dialog and background colors in light mode, alongside
  selection/history and focus checks. Dart2js build succeeds.
- The Fleury shared input fix passed **130 targeted tests**, including editable
  and read-only TextInput/TextArea, Ctrl/Command select-all, copy, replacement,
  undo/redo, explicit Emacs bindings and ignored key-up. Changed-file analysis
  is clean. The earlier whole-package analysis also reported unrelated existing
  lint findings; this is not a claim that the whole sibling checkout is clean.
- The Fleury DOM row/surface fix passed **24 Chrome tests**, including actual
  DOM Range baseline measurements next to a wide glyph. The browser package's
  analysis was clean.
- Local Fleury gates on September 9 passed: input allocation 208.4 B/key against
  207.9 baseline (+0.2%, allowed +10%); serve client bundle 382.0 KiB raw /
  100.1 KiB gzip against 512 / 160 KiB limits. Machine: local macOS ARM64;
  dependency revision and uncommitted-change boundary as above.

The final build is served at
`http://localhost:8820/revisions/162706969d3c/`. Browser checks exercised the sample,
link click and dialog, Cmd+A replacement, Enter/save, focus restoration and
Cmd+Z, dark/light rendering, and 1280-, 720- and 400-pixel-wide layouts. The
720-pixel resize kept an already-open popover visible and wrapped its controls.
The 400-pixel layout switched to a separate theme panel and kept the link form
usable. Source was observed through the host's rendered semantic value after
save, and Undo restored the original source.

Open dispatch is covered by pointer/modifier-click tests with the resolved URI.
The embedded browser dismissed the popover when Open was used, but no new
in-app tab or default-browser destination was observed. External browser launch
is therefore **not end-to-end confirmed** here; it remains a host/browser
integration check, not a reason to claim all link interactions qualified.

## What the misses say about testing

These were ordinary host interactions, not obscure Markdown edge cases. Kernel
and cell-buffer correctness did not establish DOM baseline alignment, readable
light-theme colors, keyboard handling inside a modal, or placement after resize.
The corrected tests assert those observable outcomes through their actual input
and paint paths. Edited-frame assertions run before settling; waiting is used
only for asynchronous route/focus, clipboard and worker completion.

Before landing this follow-up, review the Flark changes and the scoped sibling
Fleury renderer/keymap/widget changes together, preserving the unrelated Fleury
work. Select a reviewed Fleury dependency revision before claiming a reproducible
consumer setup. Heading presentation, tables, images, terminal/IME/lifecycle and
sustained input-to-present qualification remain separate work.

## Annotated browser comments follow-up

The five comments on candidate `162706969d3c` exposed ordinary visual and
interaction gaps. The `def` inset was two cells of host padding, not Ruby source
indentation. `FlarkCellTheme.codePadding` now makes that distinction configurable
(default 2; playground 1). Theme equality includes it so geometry is rebuilt
when padding changes; wrapped code, pointer placement and caret positions use
the same layout. Source whitespace is preserved.

The default link popover now uses content width bounded by its existing viewport
cap. It no longer stretches short URLs and action rows to 52 cells. Existing
measured placement, narrow wrapping and custom-builder behavior remain intact.

The playground uses a bordered theme panel, one blank row between swatch rows,
and consistent section gaps. Fixed five-row picker slots are gone. Custom RGB values replace their nearest
ANSI preset so light mode and custom choices retain the same two-row palette. The local
Fleury ColorPicker adds `rowSpacing` (default 0) and `showHelp` (default true);
the example supplies persistent help, so focus does not change its geometry.
Explicit secondary and border colors avoid the gray backing caused by applying
CSS opacity to filled cells in light mode.

Checkbox markers and link glyphs now publish semantic bounds from the painted
viewport. Browser cursor routing uses these bounds, without an input-intercepting
DOM overlay. Checkbox labels and wrapped continuation padding remain editable
text. Read-only checkboxes expose no activation. Targets are revalidated against
the current revision and visible geometry before semantic activation. The target
nodes sit beside the textarea node because HTML textarea elements cannot expose
interactive descendants in the accessibility mirror.

This pass: **63 host tests, 4 playground tests and 16 Fleury ColorPicker tests
passed**. Host/example and changed ColorPicker-file analysis is clean. New checks
cover padding 0/1/3, source indentation, caret/hit geometry, layout invalidation,
checkbox bounds after wrapping/scrolling, read-only and vanished targets,
content-fit popover width, swatch spacing, keyboard preview/commit and physical
pointer selection/export. These are local receipts; CI was intentionally skipped.

Final candidate: `http://localhost:8820/revisions/e9e536450c1b/`.
Browser dogfooding checked 1280- and 400-pixel layouts, dark/light styling,
scrolling the narrow customization panel, keyboard/pointer color selection,
checkbox toggle with `cursor: pointer`, adjacent text with `cursor: auto`, and
content-fit link controls with wrapping at narrow width. The final page exposes
checkbox/link accessibility nodes and produced no browser warnings/errors.
The older draft tab was left intact. Changes remain uncommitted and unmerged,
including the scoped ColorPicker API/test additions in the sibling checkout.

### September 13: code inset and blue contrast

The demo now uses `codePadding: 0`, including its exported Dart configuration.
Code begins flush with the painted background; the sample's two-space Ruby
indentation remains in the source and visible on the `puts` line.

The unreadable selected blue was from the raw ANSI palette, not the default
heading color. The playground now supplies 16 named RGB presets with light/dark
counterparts. Blue is the heading default. Selected preset hues adapt when the
user changes brightness; custom hex values outside the presets remain exact.
The shared Fleury ColorPicker API is unchanged by this follow-up.

All five example tests pass and example analysis is clean. The added regression
measures every preset's painted color against both actual page/code backgrounds
in both themes, requiring at least 4.5:1 contrast. It also selects Red then Blue
to verify an explicit Blue override adapts across brightness changes, and checks
flush code placement while preserving source indentation. No core/host behavior
changed, so their earlier test receipts were not repeated. Browser checks
confirmed flush code alignment and both palettes; explicit Blue selection became
`rgb(147, 197, 253)` in dark mode. No browser warnings/errors were observed.

Candidate: `http://localhost:8820/revisions/9458457debfb/`. The prior draft tab
was preserved. This work remains local and unmerged; CI was not run.

### September 15: review and reproducible dependency

The local-only status above is historical. Fleury's scoped changes are merged
in PR #253; Flark now uses exact Git pins and its host/example pass without
local path overrides. Current Fleury APIs required a small host migration. The
shared coloring and link parity follow-up, fresh browser checks, and remaining
qualification boundaries are recorded in
[the dual-host closure review](dual_host_review_2026_09_15.md).

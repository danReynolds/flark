# Fleury heading hierarchy — 2026-09-16

The Fleury host previously applied one `heading` style to every heading, despite
the shared projection already carrying the correct `headingLevel`. This change
adds per-level presentation in the Fleury host. The parser, Markdown source,
shared commands and Flutter rendering are unchanged.

## Presentation

- H1: bold accent title, without an underline or background band.
- H2: bold heading; the example uses slate with light/dark counterparts.
- H3: bold italic in body color.
- H4: italic; H5: regular; H6: muted. The bare cell theme uses terminal dim for
  H6; the example uses an explicit readable color without additional dimming.
- No default band, divider, underline or inline level labels. Those remain
  opt-in per-level controls.
- Optional three-cell level gutter for all headings. It reserves the same space
  for the whole rendered document, keeping prose and container edges aligned.
  Small widths fall back to inline labels where space permits.

`FlarkHeadingStyle` overrides keyed by level customize text style, band, divider
and label. The common heading style sits beneath per-level overrides. Band,
divider and indicator paint are separate theme fields. The example provides
per-level color/weight/italic/underline controls behind a disclosure, exports the
resolved styles and palette to Dart, and has a document sample (`?sample=headings`
on web, `--headings` in the terminal). Its paragraph breaks provide the sample's
breathing room; heading styles do not synthesize new blank source lines.

This is an application choice, not a claimed universal TUI convention. Without
color, H1/H2 can coincide. Without italic support some deeper levels can too.
The optional gutter gives exact hierarchy in those environments.

## Editing contract

Decorations depend on the parsed heading level and theme, never caret activity.
They do not enter source, selection or the clipboard. Wrapped lines share their
content origin; label space is reserved so labels cannot collide with the caret.
Arrow navigation skips divider rows, while pointer placement on a divider maps
back into its heading. In a gutter, label clicks map to the heading's content.
Quoted headings preserve their enclosing rail. Theme changes recheck caret
visibility after layout. Reverse-video selection toggles the underlying paint,
keeping the caret visible even on a monochrome title band.

## Local evidence

- Fleury host: 91 tests pass. Coverage includes undecorated heading defaults
  in monochrome/light/dark, opt-in band/divider/label behavior, wrapping and
  wide characters, pointer placement, clipboard, navigation, undo, gutter layout
  alongside lists/code/tables/quotes, source mode, and theme overrides.
- Existing authoring tests still type `#` through `######` character by character
  and verify the first edited frame. Selection coverage now measures reversal
  relative to the original glyph paint, including reverse-video title bands.
- Fleury example: six tests pass, including the painted H1/H2/H3 hierarchy,
  per-level color and italic/underline changes, preset adaptation to light mode,
  preserved draft/history, complete theme export/reset and narrow layouts.
  Both Dart analyses are clean.
- Browser checks: light/dark palette, H2 start click/type/undo, and 420×760
  layout with all six levels. A typed H3 was wrapped and inspected while editing.
  Toggling H3 italic in the theme panel updated all H3s without changing source.
  Final candidate: `/revisions/9184b2b57602/?sample=headings`.

Logs: `/tmp/flark-quiet-headings-tests.log` and
`/tmp/flark-quiet-headings-example-tests.log`. No CI or physical terminal/device claim.
The heading change adds no framework edits or new dependency. It lands with
the tables/images closeout against the reviewed framework Git pin, with local
overrides removed. See `fleury_support_closeout_2026_09_16.md` for the final
clean-checkout and landing receipt.

# Changelog

## Unreleased

- A table cell's text ends before the space or tab that pads its closing
  pipe, so the caret at the end of a cell sits after its last character
  rather than after the padding. Whitespace typed against a closing pipe stays
  the cell's text: typing gives the pipe a separator of its own.
- Typing the character that completes a table's delimiter row keeps the caret
  on that row when the lines below read as a table.
- Tab and Shift-Tab in indented code do nothing where the shift would carry
  the code into a list item before it.
- Text with line breaks put in a closed or setext heading leaves the
  heading's closing sequence or underline on its first line.
- A space typed before a `#` that ends a heading's text escapes that `#`, so it
  stays text and the next word follows the space.
- Text typed on an empty line of fenced code after a tab list marker stays in
  the code block.
- Shift with an arrow, End or Up/Down no longer selects hidden syntax alone
  (past a document's closing `**`): such an extension does nothing.
- Return keeps the paragraph's next line a paragraph where its indentation
  would have made it indented code, and breaks a line after the spaces that
  end it, so they no longer follow a new item's marker onto a blank line.
- Return at the end of a footnote definition inside a list item or another
  footnote continues the footnote instead of being refused.

## 0.5.0

First public preview. The parser libraries are those of release
`flark_parse-v0.1.0`.

- Parses Markdown with unmodified [comrak](https://github.com/kivikakk/comrak)
  0.54 (CommonMark 0.31.2 and GitHub Flavored Markdown) into a flat render
  model, through FFI on native platforms and a bundled Wasm module on the web,
  one instance of which every reader and session on a page shares.
- `FlarkSession`, `FlarkReader` and the `FlarkEditor` kernel: every edit
  re-parses the whole document, the projection hides syntax outside the caret's
  context, carets land only on legal source offsets, and a closed command set
  (typing, deletion, Return, styles, links and images, lists, quotes, tables,
  code blocks, and Undo/Redo with grouped history) keeps the source exactly as
  written.
- `syncLimit` and `FlarkLiveLimits` bound the synchronous path: 32 KiB on
  desktop and 16 KiB on phones and tablets by default; larger documents open in
  source mode. Through `FlarkSession`, text the editor cannot hold (a bare
  CR, an unpaired surrogate, or more than the 1 MiB writable limit) is a
  rejected load or a failed session rather than an exception, and
  `FlarkReader` shows it as source.
- `FlarkParseBackend.dispose()` frees a parser's native or Wasm memory. A
  native parser that is dropped instead is freed by a finalizer, and the VM
  refuses to copy one into another isolate.
- The build hook downloads the native parser for the build's target, checks it
  against the SHA-256 pinned in this package, and caches it. `prebuilt_dir`
  builds without network access. Android, iOS, macOS, Linux and Windows.
- `package:flark/recorder.dart`: a `FlarkEditRecorder` attached to an editor
  (`FlarkEditRecorder(editor)`, through `FlarkEditor.onCall`) records its
  calls and writes them as a Dart repro that replays the session, for turning
  a dogfooding surprise into a test.

# Changelog

## Unreleased

First public preview, to be released as 0.5.0. This entry is named for the
version in the commit that pins the parser libraries (see RELEASING.md).

- Parses Markdown with unmodified [comrak](https://github.com/kivikakk/comrak)
  0.54 (CommonMark 0.31.2 and GitHub Flavored Markdown) into a flat render
  model, through FFI on native platforms and a bundled Wasm module on the web.
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

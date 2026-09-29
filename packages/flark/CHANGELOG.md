# Changelog

## 0.5.0

First public preview.

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
  source mode.
- The build hook downloads the native parser for the build's target, checks it
  against the SHA-256 pinned in this package, and caches it. `prebuilt_dir`
  builds without network access. Android, iOS, macOS, Linux and Windows.

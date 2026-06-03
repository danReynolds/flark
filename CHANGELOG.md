# Changelog

## 0.1.0 - Unreleased

Initial standalone `flark` package hardening release.

API-shape changes (pre-publish, breaking):

- The `FlarkFlutterController` now owns parsing. Configure `parseBackend`,
  `parseProfile`, `parseDebounce`, and `onParseError` on the controller (or on
  the `initialMarkdown`/`markdown` convenience constructors of `MarkdownEditor`
  and `Markdown`). Passing these alongside a supplied `controller` now asserts —
  one document is parsed by one parser, shared across all attached surfaces.
  Added `ensureParsing()`, `configureParsing(...)`, `parseNow()`, and the
  surface-attachment lifecycle (`attachParsingSurface`/`detachParsingSurface`).
- `MarkdownEditor` and `Markdown` `profile`/`parseDebounce` are now nullable and
  apply only to widget-owned controllers.
- Unified the command surface: added `FlarkMarkdownShortcuts` with intent
  builders (`toggleStrong()`, etc.) and a default accelerator map
  (`FlarkMarkdownShortcuts.defaults()`), installed automatically via the new
  `MarkdownEditor.useDefaultShortcuts` (Cmd/Ctrl + B/I/E and Cmd/Ctrl+Shift+X).
  Keyboard shortcuts and toolbar helpers now drive the same commands.
- Made `events` the single semantic observation model and added the typed
  projections `FlarkFlutterController.markdownChanges` and `selectionChanges`.

- Promoted the v2 source-first editor architecture:
  - canonical Markdown source document;
  - pure Dart runtime, transactions, commands, projection, and render plans;
  - Flutter widgets as adapters over `FlarkFlutterController`.
- Split the public API by integration intent:
  - `flark.dart` for promoted v2 app APIs;
  - `flark_core.dart` for headless Dart/runtime APIs;
  - `flark_advanced.dart` for the full v2 integration surface.
- Removed the v1 compatibility layer, including the old
  `FlarkController`/`FlarkEditor` public API, legacy implementation
  tree, v1 oracle tests, and compatibility barrels.
- Added typed `FlarkFlutterController.events` for parse adoption,
  projection prediction, selection changes, undo, redo, and runtime changes.
- Added `MarkdownEditor.onParseError` and `Markdown.onParseError` so apps can
  observe scheduled
  background parser failures.
- Extracted the editor and read-only previewer into a standalone Flutter
  package workspace.
- Added native `comrak` parsing through the package FFI/native-assets flow, with
  iOS XCFramework and Android JNI packaging verification.
- Added the example app with editable and read-only Flark surfaces.
- Hardened controller, rendering, syntax, native bridge, markdown structure, and
  command module boundaries.
- Completed the Phase 4 markdown support policy for thematic breaks, indented
  code, escapes/entities, raw HTML text-only behavior, reference links,
  images/media previews, and source-first GFM tables.
- Added v2 render-plan-backed preview rendering for code fences, blockquotes,
  task list items, table grids, and overlay target controls.
- Added preview image-card actions for opening, copying, and selecting source
  image markdown, while keeping canonical Markdown source as the edit target.
- Added v2 live rendered editing through `FlarkMarkdownEditingMode.liveRendered`,
  preserving canonical source edits while styling projected Markdown in place
  and painting editable code-fence/blockquote chrome from the render plan.
- Promoted live rendered editing to true block-widget editing for parsed
  documents: task checkboxes toggle source markers, fenced code blocks edit the
  projected code body while preserving fences, and GFM table cells edit
  canonical Markdown table ranges.
- Added syntax highlighting, language selection, and copy actions for
  preview/live code fences without introducing an embedded IDE model.
- Added predictive render-plan mapping so live rendered styles and block chrome
  stay stable during the parse debounce window.
- Added a curated seven-PNG v2 visual golden suite for overview surfaces, live
  rendered editing, inline wrapping, code fences, blockquotes,
  tasks/tables/overlays, and compact mixed Markdown.
- Removed the default `google_fonts` dependency; package defaults now use a
  generic monospace family that apps can override through themes/styles.
- Added release, confidence, benchmark, native packaging, and example packaging
  verification scripts, including web source-only smoke coverage and upstream
  CommonMark/GFM v2 native parser contracts.

This package remains unpublished while `publish_to: none` is set and release
owner decisions are pending.

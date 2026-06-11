# Changelog

## Unreleased

Architecture hardening and correctness fixes from a full-package audit,
followed by a developer-experience pass.

Live-editing ergonomics:

- Typing `### ` (or any heading marker plus space) now renders an empty
  styled heading immediately; previously the raw marker stayed visible
  until the first content character arrived and then jumped into heading
  styling.
- Closing an inline styled run keeps the caret inside it: after typing the
  second backtick of `` `code` `` (or the closing `**`/`*`/`~~`), typing —
  including spaces — continues the run's style until an explicit exit
  (right-arrow at the end, or typing the marker character once more).
  Marker keystrokes trigger an immediate parse so the conversion lands
  before the next character.
- The caret is visible immediately in a code fence body after typing the
  ``` opener (the auto-close flow hands the body an already-focused node,
  which never fires the focus-change event that starts cursor blinking).
- Select-all followed by backspace (or typing) now edits the whole
  document in block-widget editing mode. The platform echoes such edits
  through the focused block only; re-anchoring no longer shrinks a
  document-spanning selection to that block, and IME-delivered edits over
  a spanning selection apply to the full selection.
- Deleting a styled run's full content (select-all + backspace over
  `` `test` ``) removes its hidden markers too instead of leaving orphaned
  literal backticks; typing over the selection replaces just the content,
  keeping the style. Display range selections now map symmetrically to the
  visible content.
- Inline-code highlights stay contiguous while typing: trailing spaces
  inside a run are now painted (Flutter skips span backgrounds on
  line-trailing whitespace, so the surfaces paint run backgrounds from
  layout boxes), and Enter at a run's end steps past the closing marker
  instead of splitting the span source into orphaned literal markers.
- Inline styled runs (code/strong/emphasis/strikethrough) can be re-entered
  at their trailing edge, Slack-style: placing the caret right after the
  run's last character keeps typing inside the run; plain left/right
  arrows step between the inside and outside caret states at the boundary
  (one visually stationary keypress); and typing the run's own marker
  character at the inside-end exits the run instead of inserting a literal
  marker. Ambiguous platform edits (a space typed before an existing
  space) anchor at the caret instead of diff-sliding out of the run, so
  multi-word styled text types naturally. The affinity model is documented
  in `doc/architecture/live_edit_intent_pipeline.md`.

Developer experience:

- Typography theming: `FlarkMarkdownThemeData` gains optional per-token
  `TextStyle` overrides merged over the computed defaults —
  `codeTextStyle` (custom code fonts), `inlineCodeTextStyle`,
  `headingTextStyle` plus per-level `heading1TextStyle`…`heading6TextStyle`
  (heading colors/fonts/sizes), `quoteTextStyle`, `linkTextStyle`
  (e.g. remove the underline), `strongTextStyle`, `emphasisTextStyle`, and
  `strikethroughTextStyle` — plus a `selectionColor` override (previously
  always derived from the cursor color). Bullet list markers now paint with
  `listMarkerColor` like ordered markers (previously `chromeLabelColor`),
  a subtle light-palette change pinned by regenerated goldens.

- Theming: `FlarkMarkdownThemeData` (with `light`/`dark` palettes and
  `copyWith`) now drives every chrome color — code fences, quotes, links,
  tables, checkboxes, menus, and the syntax-highlighting palette
  (`FlarkCodeSyntaxThemeData`). Provide it per widget via `theme:` or
  ambiently via `FlarkMarkdownTheme`; with no configuration the palette
  follows platform brightness. The light palette is identical to the
  previous hard-coded colors.
- Public widgets renamed for consistency and to avoid colliding with
  `flutter_markdown`: `Markdown` → `FlarkMarkdown`, `MarkdownEditor` →
  `FlarkMarkdownEditor`, `MarkdownEditorFormField` →
  `FlarkMarkdownEditorFormField`. The old names remain as deprecated
  typedefs until 1.0. `profile` is a deprecated alias of the new
  `parseProfile` parameter.
- `FlarkMarkdownEditor.readOnly` renders the live document without
  accepting edits, shortcuts, or block mutations;
  `FlarkMarkdownEditorFormField(enabled: false)` does the same; and
  `FlarkMarkdown(selectable: true)` enables text selection in previews.
- Links open on tap when `onOpenLink` is configured (the copy/edit menu
  moves to long-press), matching standard markdown-renderer behavior.
- Rebuilding a widget-owned editor/preview with a changed `onParseError`
  closure (e.g. an inline lambda recreated every build) no longer restarts
  the parse scheduler; only backend/profile/debounce changes do.
- Documented the native build prerequisites (Rust toolchain, Android NDK
  discovery, Windows unsupported, prebundled web WASM) in the README and
  parser/platforms guide; the build hook's errors now point there.

Behavior fixes:

- Undo/redo now map the projection and render plan through the applied
  inverse transactions instead of resetting them, so live-rendered surfaces
  no longer flash raw Markdown source on every undo. `FlarkHistoryResult`
  and `FlarkEditorRuntimeResult` expose `appliedTransactions`.
- Enter at the start of a non-empty blockquote line continues the quote
  instead of deleting the line's text.
- Toggling emphasis inside `**bold**` nests (`***bold***`) instead of
  stripping the strong markers; delimiter runs now decide marker validity.
- Heading commands compose after quote and list prefixes (`> # q`,
  `- ## item`) instead of producing `## > q` / `## - item`.
- Tab-indented fence markers are no longer treated as code fences, matching
  CommonMark/Comrak (a leading tab means indented code).
- A native paragraph is replaced by synthetic list items only when every
  non-blank line is an in-progress list marker; soft-wrapped paragraphs no
  longer lose their other lines.
- Parser bridges keep `replacementRanges` when attaching diagnostics
  (decoded HTML entities no longer revert to raw source on error paths).
- A failed WASM module load is retried on the next parse instead of being
  cached for the session.
- CRLF/CR line endings normalize to LF at document ingest.
- `blockAtDisplayOffset` attributes a boundary caret to the block it
  starts, not the previous block.
- A block whose content is replaced wholesale survives render-plan
  prediction (typing over a fully selected paragraph no longer flickers).

Production hardening (post-audit verification pass):

- `parseNow()` now resolves only once the current revision has an
  authoritative render plan, chaining onto in-flight parses instead of
  silently returning while the plan is stale.
- Undo history is bounded (`FlarkHistoryStack.maxEntries`, default 1000
  logical edit groups; oldest entries drop first).
- Undo/redo on an exhausted stack are true no-ops: the runtime returns the
  identical instance, so live surfaces keep their render plan.
- `MarkdownEditorFormField.reset()` normalizes CRLF input and no longer
  rewrites the document when the value is unchanged after normalization.
- `flarkNativeParseIsolateThresholdBytes` configures (or disables, for
  fake-async widget tests) the FFI worker-isolate parse offload; documented
  in the parser/platforms guide.
- Command-registry payload-type mismatches report notHandled instead of
  terminally rejecting, so typed handlers under a shared id are reachable.
- A grouped undo merges per-step projection-prediction metadata (touched
  flag, invalidated range) instead of reporting only the final step.
- Fence-nesting semantics (fence-looking lines inside an open fence are
  body text, per CommonMark) are pinned by test.

Architecture:

- Platform-echo handling is classified, not open-coded: each editing surface
  resolves an incoming text change through a pure, table-tested classifier
  (`classifyFlarkLiveBlockEdit` / `classifyFlarkHostEdit`) into one typed
  intent, and the widgets only execute intents. The classifiers live in a
  standalone library that cannot import the editor widgets, so their purity
  is compiler-enforced, and the recognizer ordering lives in exactly one
  inspectable place per surface. Every host/block recognizer asymmetry is
  named — intentional or convergence candidate — in
  `doc/architecture/live_edit_intent_pipeline.md` and pinned by tests; the
  remaining behavioral convergence is a documented device-test checklist.

- `FlarkRenderPlan.fidelity` (`authoritative`/`predicted`/`stale`) replaces
  the unread `'stale'`/`'predictive'` metadata flags;
  `FlarkFlutterController.hasUsableRenderPlan` centralizes the surface
  fallback decision.
- `FlarkMarkdownFenceLayout` is the single fence model of record, computed
  in one pass and shared by the source policies, the controller's
  structural prediction, and the parse backend's synthetic code blocks —
  removing the per-keystroke quadratic fence rescans.
- Large documents parse on a worker isolate (FFI platforms), keeping the
  UI isolate responsive; small documents still parse inline.
- The live block reconciler's identity key includes task/checkbox state,
  code language, and list kind, so same-text blocks cannot swap identities.
- The editor runtime applies each transaction once (history reuses the
  computed document) and the projected-editable implementation is split
  into focused part files.

## 0.1.0 - 2026-06-08

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

Prepared the package for its first public release with release metadata,
license, documentation links, screenshots, and CI coverage for the full release
gate.

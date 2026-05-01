# Sovereign Public API Inventory

Status date: 2026-05-01
Source: `lib/sovereign_editor.dart`

## Current Barrel Exports

The package currently exposes three different categories from one top-level
library:

### Intended Consumer Surface

- `SovereignController`
- `SovereignEditor`
- `SovereignMarkdownView`
- `SovereignEditorThemeData` and related theme types
- `SovereignMarkdownCommands`
- command result/capability/link-edit models

These are the likely stable package API after cleanup.

### Advanced Integration Surface

- syntax engine interfaces and snapshots
- native bridge preflight/load types
- UTF-8/UTF-16 offset mapper

These may remain public if we want consumers to plug in custom parser engines,
but they need explicit API docs and stability guarantees before release.

### Implementation/Test Surface Currently Exposed

- parser implementation adapters
- parse backend and scheduler internals
- block parser and fenced-code scanner
- markdown marker grammar and marker helpers
- projector, code highlighter, and geometry scanner
- block tree/node models
- decoration/edit/geometry/line-index/state/style models
- `UndoStack`
- `EditDiffer`

These should probably move behind `lib/src` unless a concrete consumer use case
is documented. They are useful for internal tests and package tooling, but they
should not become accidental public contracts.

First cleanup pass:

- removed parser implementation adapters, parse backend/scheduler internals,
  scanners, marker helpers, `UndoStack`, and `EditDiffer` from
  `lib/sovereign_editor.dart`;
- kept model types in the barrel for now because `SovereignController` and the
  syntax engine contracts still expose them in public signatures.

## Deep Import Risk

Most implementation files live directly under `lib/widgets/sovereign/...`, so
consumers can import internals even if the barrel stops exporting them. A
production package should move implementation code into `lib/src` in staged
waves.

## Proposed Stable API Target

Keep:

- `package:sovereign_editor/sovereign_editor.dart`
- editor and preview widgets
- controller lifecycle and text/selection APIs
- typed command API
- theming API
- markdown profile and syntax snapshot models only if customization requires
  them
- native preflight diagnostics

Hide:

- scanners and marker grammar
- geometry/projector internals
- renderer internals and painters
- undo stack implementation
- edit differ implementation
- internal state objects
- parser backend implementation classes

Secondary public library decision:

- no secondary public libraries are warranted for this cleanup pass;
- the only supported app-facing import is
  `package:sovereign_editor/sovereign_editor.dart`;
- deep imports remain test/white-box implementation details unless a future
  release explicitly documents a secondary library.

## Cleanup Waves

### Wave 1: API Decision

- Mark each current export as `stable`, `advanced`, or `internal`.
- Write short docs for every stable public type.
- Decide compatibility policy for removing deep imports.

### Wave 2: File Layout

- Create `lib/src/`.
- Move implementation-only files under `lib/src`.
- Update package tests to import through `src` only where white-box coverage is
  intentional.
- Keep consumer examples importing only `package:sovereign_editor/sovereign_editor.dart`.

Progress:

- Started with implementation helpers that are not part of the supported
  barrel API: moved `Logger` to `lib/src/helpers/logger.dart` and removed the
  public `AppColors` helper in favor of a private markdown-theme palette.
- Narrowed `lib/sovereign_editor.dart` to stop exporting parser
  implementations, scanners, marker helpers, `UndoStack`, and `EditDiffer`.
- Added `test/public_api/sovereign_editor_barrel_test.dart` as a top-level
  import smoke test for the supported editor, command, theme, syntax, native
  diagnostic, and UTF offset APIs.
- Moved `UndoStack` and `EditDiffer` behind `lib/src` as the first focused
  implementation-file migration wave.
- Moved presentation/render helpers behind `lib/src`, including
  `Tier1Painter`, inline-actions overlay/targeting helpers,
  read-only link tap tracking, and read-only task-checkbox overlay helpers.
  Package tests that intentionally inspect painter behavior now import the
  painter through `package:sovereign_editor/src/...`.
- Moved command implementation helpers behind `lib/src`, including block,
  inline, fence, link, transaction, range, selection, and command-context
  helpers. `SovereignMarkdownCommands` remains the supported public command
  facade.
- Moved `SyntaxParseScheduler` behind `lib/src`; tests that intentionally
  exercise the scheduler now import it as a white-box implementation detail.

### Wave 3: Naming Cleanup

- Replace public Dune vocabulary:
  - [x] `DuneMarkdownTheme` -> `SovereignMarkdownTheme`
- Keep package default palette names private/package-neutral.
- Keep deprecated aliases only if we need compatibility with Dune during a
  transition.

Progress:

- Renamed `lib/theme/dune_markdown_theme.dart` to
  `lib/theme/sovereign_markdown_theme.dart`.
- Replaced the public `DuneMarkdownTheme.dune()` constructor with
  `SovereignMarkdownTheme.standard()` without adding a compatibility alias.

### Wave 4: Compatibility and Docs

- Add migration notes for removed exports and renamed types.
- Add API docs and example snippets.
- Run `dart pub publish --dry-run` after release metadata exists.

Progress:

- Added `docs/production_readiness/api_migration_2026-05-01.md` covering the
  theme rename, removed top-level internals, edit-history internals moved to
  `lib/src`, the removed app palette helper, and the current decision not to
  add secondary public libraries.
- Established a clean docs-generation baseline by fixing unresolved Dart doc
  bracket references; `dart doc --dry-run` now reports 0 warnings and 0 errors.
- Added the first primary API prose wave for the top-level library,
  `SovereignController`, `SovereignEditor`, `SovereignMarkdownView`, command
  facade/result/capability/link-edit models, and editor/markdown theme classes.

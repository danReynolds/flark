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

### Wave 3: Naming Cleanup

- Replace public Dune vocabulary:
  - `DuneMarkdownTheme` -> `SovereignMarkdownTheme`
- Keep package default palette names private/package-neutral.
- Keep deprecated aliases only if we need compatibility with Dune during a
  transition.

### Wave 4: Compatibility and Docs

- Add migration notes for removed exports and renamed types.
- Add API docs and example snippets.
- Run `dart pub publish --dry-run` after release metadata exists.

# Sovereign Phase 1 API Migration Notes

Status date: 2026-05-01

These notes cover the first public API cleanup pass after extraction into the
standalone package.

## Supported Import Contract

Use the top-level library for app code:

```dart
import 'package:sovereign_editor/sovereign_editor.dart';
```

As of this cleanup pass, there are no supported secondary public libraries.
Deep imports are reserved for package tests and white-box package development
unless a future release explicitly documents a secondary library.

## Theme Rename

The public markdown theme now uses Sovereign vocabulary.

Before:

```dart
import 'package:sovereign_editor/theme/dune_markdown_theme.dart';

final theme = DuneMarkdownTheme.dune();
```

After:

```dart
import 'package:sovereign_editor/sovereign_editor.dart';

final theme = SovereignMarkdownTheme.standard();
```

For custom colors or typography, continue to use `copyWith` and pass the result
through `SovereignEditorThemeData(markdownTheme: ...)`.

## Removed Top-Level Barrel Exports

The top-level `sovereign_editor.dart` barrel no longer exports implementation
internals:

- parser implementation adapters;
- parse backend and scheduler internals;
- scanners and marker helpers;
- projector, code highlighter, and geometry scanner internals;
- `UndoStack`;
- `EditDiffer`.

App code should not depend on these types. For custom parsing integrations, use
the supported syntax contracts exported from the top-level library:

- `SyntaxEngine`;
- `SyntaxSnapshot`;
- `SyntaxParseRequest`;
- `SyntaxPredictRequest`;
- `SyntaxPrediction`;
- `MarkdownSyntaxProfile`.

There is no stable app-level replacement for parser adapters, scanners,
projectors, marker helpers, undo stack internals, or edit-diff internals. Those
remain implementation details.

## Undo and Edit History Internals

`UndoStack` and `EditDiffer` moved behind `lib/src`.

App code should use `SovereignController`, `SovereignMarkdownCommands`, and
controller undo/redo behavior rather than constructing edit-history internals.
White-box package tests may import `src` while the implementation is still
being refactored, but that is not an app compatibility contract.

## Removed App Palette Helper

The extracted package no longer exposes the old app-level `AppColors` helper.
Use `SovereignMarkdownTheme.standard().copyWith(...)`,
`SovereignEditorThemeData`, and the typed editor theme models instead.

## Temporarily Retained Model Types

Some model types remain exported because current public controller and syntax
contracts still expose them in signatures. Treat those exports as contract
carriers, not as permission to couple app code to renderer, scanner, or
projection internals.

Future cleanup waves may move more implementation-only files into `lib/src`
after their public signatures are narrowed.

# Flark Phase 1 API Migration Notes

Status date: 2026-05-01

These notes cover the first public API cleanup pass after extraction into the
standalone package.

## Supported Import Contract

Use the top-level library for app code:

```dart
import 'package:flark/flark.dart';
```

As of this cleanup pass, there are no supported secondary public libraries.
Deep imports are reserved for package tests and white-box package development
unless a future release explicitly documents a secondary library.

## Theme Rename

The public markdown theme now uses Flark vocabulary.

Before:

```dart
import 'package:flark/theme/dune_markdown_theme.dart';

final theme = DuneMarkdownTheme.dune();
```

After:

```dart
import 'package:flark/flark.dart';

final theme = FlarkMarkdownTheme.standard();
```

For custom colors or typography, continue to use `copyWith` and pass the result
through `FlarkEditorThemeData(markdownTheme: ...)`.

## Removed Top-Level Barrel Exports

The top-level `flark.dart` barrel no longer exports implementation
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

App code should use `FlarkController`, `FlarkMarkdownCommands`, and
controller undo/redo behavior rather than constructing edit-history internals.
White-box package tests may import `src` while the implementation is still
being refactored, but that is not an app compatibility contract.

## Presentation and Render Helpers

Several presentation helpers also moved behind `lib/src`:

- `Tier1Painter`;
- inline-actions overlay and targeting helpers;
- read-only link tap tracking helpers;
- read-only task-checkbox overlay helpers.

These were never part of the supported app-facing API. App code should continue
to compose `FlarkEditor` and `FlarkMarkdownView` instead of importing
their render helpers directly.

## Command Implementation Helpers

Command implementation helpers moved behind `lib/src`, including block, inline,
fence, link, transaction, range, selection, and command-context helpers.

App code should use `FlarkMarkdownCommands`,
`FlarkController.commands`, and the public command model types instead of
deep-importing command implementation files.

## Syntax Parse Scheduler

`SyntaxParseScheduler` moved behind `lib/src`. App code should use
`FlarkController`, `SyntaxEngine`, and the syntax request/snapshot
contracts rather than coordinating parse scheduling directly.

## Syntax Engine Factory

`SyntaxEngineFactory` moved behind `lib/src`. App code should rely on the
default `FlarkController` engine wiring or pass a custom public
`SyntaxEngine` into `FlarkController`/`FlarkMarkdownView`.

## Parser Backend and Adapter Implementations

Parser backend and adapter implementation classes moved behind `lib/src`:

- `CommonMarkParseBackend`;
- `CommonMarkSyntaxEngineAdapter`;
- `ComrakCommonMarkParseBackend`;
- `V1SyntaxEngineAdapter`.

App code should use the public `SyntaxEngine` injection points and syntax
request/snapshot contracts rather than constructing package backend/adapters
directly. Native bridge diagnostics and UTF offset mapping remain public where
they are needed for consumer-safe preflight and integration behavior.

## Markdown Logic and Scanner Internals

Markdown logic and scanner implementation classes moved behind `lib/src`,
including:

- `BlockParser`;
- `FencedCodeScanner`;
- `MarkdownMarkerGrammar`;
- `FlarkMarkdownMarkers`;
- `FlarkStyleScanner`;
- `FlarkGeometryScanner`;
- `FlarkCodeHighlighter`;
- `Projector`.

App code should use `FlarkController`, `FlarkEditor`,
`FlarkMarkdownView`, public syntax snapshots, and public model types rather
than deep-importing scanner/parser helpers. Package tests may still import
these through `package:flark/src/...` when they intentionally verify
white-box behavior.

## Core Service, Rendering, and Pipeline Internals

Core implementation modules moved behind `lib/src`, including:

- input-intent handlers and intent result models;
- edit operation pipeline, undo grouping policy, and value mutation
  coordinator;
- text renderer helpers and heading style policy;
- editor-session state carriers;
- syntax projection, prediction, and selection-mask helpers;
- markdown line, fence, indented-code, table, and navigation services.

App code should continue to use `FlarkController`, `FlarkEditor`,
`FlarkMarkdownView`, `FlarkMarkdownCommands`, public theme types, and
the public syntax/model contracts rather than deep-importing core services.
White-box package tests may import these through
`package:flark/src/...`.

## Controller and Editor Private Helpers

Controller/editor private helper files moved behind `lib/src`, including:

- controller policy part files;
- controller host adapter part files;
- controller diagnostics and table-tab host helpers;
- the controller navigation helper;
- editor inline-actions and task-checkbox overlay part files.

These files are not supported app-facing libraries. App code should use
`FlarkController`, `FlarkEditor`, and `FlarkMarkdownView` through
the top-level `flark.dart` library.

## Removed App Palette Helper

The extracted package no longer exposes the old app-level `AppColors` helper.
Use `FlarkMarkdownTheme.standard().copyWith(...)`,
`FlarkEditorThemeData`, and the typed editor theme models instead.

## Temporarily Retained Model Types

Some model types remain exported because current public controller and syntax
contracts still expose them in signatures. Treat those exports as contract
carriers, not as permission to couple app code to renderer, scanner, or
projection internals.

Future cleanup waves may move more implementation-only files into `lib/src`
after their public signatures are narrowed.

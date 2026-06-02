# Flark v2 Public API Sketch

Status date: 2026-05-02
Status: draft RFC

## Purpose

This sketch names the intended public v2 surfaces before they are exported. It
is not a commitment to publish these exact names immediately. The first v2 code
stays under `lib/src/v2` until the API is stable enough to expose.

## Proposed Libraries

```text
lib/flark.dart
  Existing primary Flutter package surface. It should eventually use v2 behind
  the same high-level widgets and controller concepts where possible.

lib/flark_core.dart
  Proposed headless core surface for non-Flutter tests, server-side tooling,
  command processing, markdown transforms, and advanced integrations.

lib/sovereign_editor_testing.dart
  Optional future testing helpers for fixture parsing, oracle comparison,
  render-plan assertions, and conformance runners.
```

Do not add these exports until the core API has passed the initial transaction,
history, command, projection, and render-plan slices.

## Core Public Concepts

The headless library should expose:

- `FlarkDocument`
- `FlarkTextBuffer`
- `FlarkSelection`
- `FlarkSourceRange`
- `FlarkSourceOperation`
- `FlarkTransaction`
- `FlarkTransactionMetadata`
- `FlarkTransactionIntent`
- `FlarkHistoryStack`
- `FlarkEditorState`
- `FlarkCommand`
- `FlarkCommandResult`
- `FlarkCommandRegistry`
- `FlarkExtension`
- `FlarkMarkdownRuntime`
- `FlarkMarkdownProfile`
- `FlarkProjection`
- `FlarkRenderPlan`

## Flutter Public Concepts

The Flutter library should expose:

- `FlarkFlutterController`
- `FlarkEditor`
- `FlarkMarkdownView`
- `FlarkEditorThemeData`
- `FlarkMarkdownCommands`
- `FlarkLinkAction`
- `FlarkMediaResolver`
- `FlarkEditorDiagnostics`

The existing v1 names should be preserved where they are already reasonable.
New v2 names should only be added when they describe a durable concept, not a
temporary adapter detail.

## Explicit Non-API

These should remain internal:

- native bridge payload structs;
- parser JSON/ABI DTOs;
- predictive reconciliation internals;
- source/display mapping implementation details;
- Flutter adapter host classes;
- scanner helpers;
- fixture importers;
- benchmark harness helpers.

## Public API Rules

- Prefer one canonical way to do each operation.
- Do not export compatibility aliases unless a migration guide requires them.
- Use typed models instead of loose maps.
- Unknown extension results and unknown parse payload fields must degrade
  gracefully.
- Avoid public constructors that make invalid states easy to create.
- Keep Flutter types out of `flark_core.dart`.
- Keep native parser details out of both public libraries except through
  diagnostics and capabilities.

## Staging Plan

1. Keep all v2 implementation under `lib/src/v2`.
2. Stabilize transaction/history tests.
3. Add command registry and command result tests.
4. Add parser/projection/render-plan types.
5. Run an API review against this sketch.
6. Add `lib/flark_core.dart` only when the exported names have
   enough behavior to be useful.
7. Migrate `lib/flark.dart` internals to v2 after adapter parity.

## Open Questions

- Should `FlarkHistoryStack` be public, or should history be mediated only
  through a future `FlarkEditorEngine`? Current direction: keep it as a
  companion/runtime-owned object, not inside `FlarkEditorState`.
- Should `FlarkTextBuffer` be public if it remains a simple immutable
  string wrapper, or should it stay internal until benchmarks require a richer
  implementation?
- Should command extension APIs be stable in the first v2 release, or staged as
  experimental?
- Should `FlarkMarkdownProfile` replace the current public
  `MarkdownSyntaxProfile`, or should v2 preserve the existing name?

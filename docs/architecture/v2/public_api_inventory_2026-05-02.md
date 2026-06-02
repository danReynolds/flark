# Sovereign v2 Public API Inventory

Status date: 2026-05-03
Source: `lib/sovereign_editor.dart`, `lib/sovereign_editor_core.dart`,
and `lib/sovereign_editor_v2.dart`
Status: v2-only promoted app barrel, headless core barrel, and full v2 barrel

The supported v2 import for application code is now:

- `package:sovereign_editor/sovereign_editor.dart`

The headless Dart import for non-Flutter integrations is:

- `package:sovereign_editor/sovereign_editor_core.dart`

The full v2 import remains available for custom parser, native bridge, and
extension integration work:

- `package:sovereign_editor/sovereign_editor_v2.dart`

Deep imports under `lib/src/v2/...` are implementation and white-box test
details unless a type is explicitly promoted through one of the public barrels.

## Barrel Shape

`lib/sovereign_editor_v2.dart` intentionally exports from six v2 barrels with
explicit `show` lists:

- `src/v2/core/core.dart`
- `src/v2/flutter/flutter.dart`
- `src/v2/markdown/markdown.dart`
- `src/v2/native/native.dart`
- `src/v2/projection/projection.dart`
- `src/v2/render_plan/render_plan.dart`

This keeps the experimental lane broad enough for downstream examples, custom
extensions, parser integration, projection-backed editing, and render-plan
overlays while preventing accidental export of every helper kept under
`lib/src/v2`.

The stable barrel exports a selected v2 subset. It includes the v2 controller,
the two high-level widgets, native parser adapter, markdown command families,
core transaction/runtime types, render-plan descriptors, and native backend
preflight diagnostics. It does not export low-level editing widgets, parser
schedulers, or implementation renderers. There is no v1 compatibility barrel.
New users and internal examples start on the source-first architecture.

## Core Runtime

Exported from `src/v2/core/core.dart`:

- command model: `SovereignCommand`, command context, command registry, command
  priorities, command results, and core editing commands;
- document model: `SovereignDocument`, `SovereignTextBuffer`,
  `SovereignUtf8Utf16Mapper`;
- extension model: `SovereignExtension`, extension set;
- runtime/state: `SovereignEditorRuntime`, `SovereignEditorRuntimeResult`, and
  `SovereignEditorState`;
- selection/range/transaction model: `SovereignSelection`,
  `SovereignMapAffinity`, `SovereignSourceRange`,
  `SovereignSourceOperation`, `SovereignTransaction`,
  `SovereignTransactionMetadata`, and `SovereignTransactionIntent`.

Boundary: this layer must remain pure Dart and must not import Flutter or
`dart:ui`.

## Markdown

Exported from `src/v2/markdown/markdown.dart`:

- block, inline, link, table, and task/list command extensions;
- `SovereignMarkdownEditingExtensions.standard()` for the default v2 markdown
  editing extension set;
- command capability query APIs for toolbar and active-mark state;
- markdown inline style identifiers;
- parser backend protocol, parse requests/results, parser capabilities,
  profiles, schema version constants, typed block/inline/hidden-range models,
  diagnostics, and ambiguity-zone models;
- `SovereignNativeComrakParseBackend`, the required default v2 adapter over
  the Comrak bridge ABI.

Boundary: parser payloads must preserve unknown fields and unknown variants
without crashing.

## Native Bridge

Exported from `src/v2/native/native.dart` in the full v2 barrel:

- `NativeComrakBridge`, parse input/result DTOs, ranges, block/inline models,
  diagnostics, and payload codec;
- native bridge load exception, failure kind, and preflight result;
- `createNativeComrakBridge()` and `preflightNativeComrakBridge()` for advanced
  native integration tests and custom parser wiring.

Boundary: app code should normally use `SovereignNativeComrakParseBackend`.
Native bridge types are advanced API because custom bridge injection and
white-box native tests need stable typed contracts.

## Projection

Exported from `src/v2/projection/projection.dart`:

- `SovereignProjection`, hidden ranges, cursor masks, ambiguity zones,
  predictive projection, and reconciliation;
- source/display selection mapping helpers;
- `SovereignProjectedTextEditAdapter`, the headless adapter that converts edits
  to projected display text back into source transactions.

Boundary: projection owns marker hiding and source/display mapping. Flutter
widgets may consume projection state, but must not duplicate marker mapping.

## Render Plan

Exported from `src/v2/render_plan/render_plan.dart`:

- `SovereignRenderPlan`, render blocks, inline runs, table/task/code/link/image
  descriptors, text style tokens, overlay target queries, and overlay plans;
- `SovereignRenderPlanExtension` and `SovereignRenderPlanContext` for semantic
  render-plan customization from registered extensions.

Boundary: edit and read-only surfaces consume the same render plan; widgets
should not reparse markdown to rediscover structure already present in the
plan.

## Flutter Adapter

Promoted through the public barrels from `src/v2/flutter/flutter.dart`:

- `SovereignFlutterController`;
- `SovereignControllerEvent` and `SovereignControllerEventKind` for typed
  update streams;
- `MarkdownEditor`, the promoted source-first editing widget. It can
  own a controller from `initialMarkdown` or use a caller-provided controller;
- `Markdown`, the promoted read-only widget. It can own a
  controller from `markdown` or consume a caller-provided controller's render
  plan;
- `onParseError` promoted-widget callbacks for scheduled background parser
  failures;
- `SovereignPreviewBlockWidgetBuilder` for custom read-only block rendering;
- `SovereignOverlayTargetWidgetBuilder` for custom overlay controls exposed
  through the promoted widgets;
- Flutter command `Intent`/`Action` integration.

Boundary: Flutter owns rendering, focus, platform text input, and actions. It is
not the source of truth for document state, history, commands, parsing,
projection, or render-plan semantics.

## Implementation-Only Flutter Types

The raw-source, projected, and live-rendered concrete editing widgets,
`SovereignReadOnlyPreview`, `SovereignParseScheduler`,
`SovereignRenderPlanOverlayControls`, and `SovereignTextDeltaAdapter` remain
under `lib/src/v2/flutter` for package implementation and white-box tests. They
are deliberately not exported by `sovereign_editor.dart` or
`sovereign_editor_v2.dart`; app code should reach them through
`MarkdownEditor` and `Markdown`.

## Verification

Current API guardrails:

- `test/v2/public_api/sovereign_editor_v2_public_api_test.dart` imports the
  experimental v2 barrel, smoke-tests the exported controller, commands, native
  parser adapter, projection, projected edit adapter, render-plan extension
  types, preview block builder, and the two promoted widget surfaces, and checks
  that the barrel uses explicit `show` exports and excludes implementation-only
  Flutter widgets.
- `test/public_api/sovereign_editor_barrel_test.dart` imports the promoted v2
  barrel and headless core barrel, then smoke-tests each surface.
- `test/v2/core/v2_core_import_boundary_test.dart` prevents Flutter imports in
  v2 headless layers.
- `test/v2/packaging/sovereign_v2_native_packaging_contract_test.dart` checks
  that the v2 native backend shares the hook-owned native bridge asset and ABI
  symbols.
- `test/v2/flutter/sovereign_v2_visual_golden_test.dart` pins the promoted
  source, projected, preview, and overlay-control visual contract.
- `test/v2/markdown/sovereign_v2_native_upstream_contract_test.dart` runs the
  native Comrak output through v2 projection and render-plan contracts across
  upstream CommonMark/GFM fixtures.
- `test/v2/flutter/sovereign_markdown_web_smoke_test.dart` is the release web
  smoke for promoted widgets and Comrak WASM parser loading.

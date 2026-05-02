# RFC 017: Sovereign Controller Module Boundaries

## 1. Summary

Define the final module lines for the Sovereign editor controller breakout so
`SovereignController` becomes a thin orchestrator and all behavior lives in
explicit, testable modules (not `part`-coupled internals).

This RFC is based on the current code shape in:

- `packages/sovereign_editor/lib/widgets/sovereign/controllers/`
- `packages/sovereign_editor/lib/widgets/sovereign/commands/`
- `packages/sovereign_editor/lib/widgets/sovereign/logic/`
- `packages/sovereign_editor/lib/widgets/sovereign/engine/`

---

## 2. Current State (What still leaks into controller)

`SovereignController` is reduced compared to earlier versions but still owns:

1. Selection guard and cursor-mask snapping.
2. Edit-op creation, merge policy, undo/redo grouping.
3. Parse scheduling and predictive/authoritative projection coordination.
4. Input-intent orchestration and policy dispatch.
5. Structural markdown helpers (line/fence/table/list/quote parsing bits).
6. Rendering dispatch and renderer telemetry coordination.

This is too much responsibility for one class, even with `part` extraction.

---

## 3. Final Module Boundaries

These are the correct submodules to keep long-term, with one adjustment:
`EditorSessionState` should be a composed state object, not a single untyped bag.

### 3.1 Controller facade

- `SovereignController` (thin orchestrator, public API surface)
- No business logic beyond delegation and guardrails.
- Owns framework-facing objects:
  - `StreamController<DecorationModel>` for decoration emissions,
  - `Projector` instance used by selection/projection guard.

### 3.2 Session state (composed)

- `EditorSessionState`
  - `DocumentState` (value/revision/line index/geometry)
  - `ProjectionState` (hidden/exclusion ranges + cursor masks + snapshot refs)
  - `HistoryState` (undo/redo + merge metadata + transaction state)
  - `TelemetryState` (predictive/render/parse counters)

State mutability contract:

- `EditorSessionState`, `DocumentState`, `ProjectionState`, and `HistoryState`
  are immutable value objects (`copyWith` replacement model).
- `TelemetryState` may use mutable counters for performance, but must not
  participate in correctness decisions.
- `DocumentState` updates are atomic: any new `TextEditingValue` must be paired
  with recomputed `LineIndex` and `GeometryModel` in the same returned state.

### 3.3 Editing pipeline

- `EditingPipeline`
  - op generation/diff/merge policy
  - transaction boundaries
  - undo/redo restoration flow
  - clear-redo semantics on fresh edits

### 3.4 Syntax projection

- `SyntaxProjectionCoordinator`
  - single-flight parse wiring
  - predictive + authoritative reconciliation
  - ambiguity-zone stabilization
  - projection updates
- `SelectionProjectionGuard`
  - selection projection + cursor-mask snapping
  - selection normalization invariants

### 3.5 Input intents

- `InputIntentRouter`
  - `EnterIntentHandler`
  - `TabIntentHandler`
  - `NavigationIntentHandler`
  - `BackspaceIntentHandler`

### 3.6 Markdown structure service (split into query + transform)

- `MarkdownStructureQueryService`
  - line starts/ends, list markers, quote context, fence context, table cell
    mapping, visibility helpers
- `MarkdownStructureTransformService`
  - structural edits: list/quote/fence/table transformations
  - no direct controller writes; returns edit plans

### 3.7 Rendering

- `TextRenderer` (pure render composition)
  - marker ranges
  - inline runs
  - block runs
  - span construction
  - code highlight builder

### 3.8 Commands (already right direction)

- Keep command facade in `commands/`:
  - `SovereignMarkdownCommands`
  - block/inline/link/fence command handlers
- Commands use controller API, not private internals.

---

## 4. Proposed File Layout

This is a target layout, not a literal snapshot. Placeholder interfaces are
intentionally omitted from code until they have concrete, wired implementations
(for example `editing_pipeline.dart`, `input_intent_router.dart`,
`markdown_structure_query_service.dart`, `markdown_structure_transform_service.dart`).

```text
packages/sovereign_editor/lib/widgets/sovereign/
  controller/
    sovereign_controller.dart
    controller_dependencies.dart
  core/
    state/
      editor_session_state.dart
      document_state.dart
      projection_state.dart
      history_state.dart
      telemetry_state.dart
    pipeline/
      editing_pipeline.dart
      edit_operation_factory.dart
      history_coordinator.dart
    syntax/
      syntax_projection_coordinator.dart
      selection_projection_guard.dart
    intents/
      input_intent_router.dart
      enter_intent_handler.dart
      tab_intent_handler.dart
      navigation_intent_handler.dart
      backspace_intent_handler.dart
    structure/
      markdown_structure_query_service.dart
      markdown_structure_transform_service.dart
      models/
        fence_context.dart
        quote_context.dart
        table_line.dart
    rendering/
      text_renderer.dart
      text_renderer_block_runs_builder.dart
      text_renderer_inline_runs.dart
      text_renderer_markers.dart
      text_renderer_span_builder.dart
      text_renderer_code_highlight_builder.dart
```

Notes:

- Existing `logic/` scanners remain source-of-truth low-level parsers.
- Existing `engine/` remains backend boundary.
- `part` files should be retired progressively as classes move to this layout.

---

## 5. Interface Contracts (Dart-level)

### 5.0 EditorContext contract

`EditorContext` is the only object passed to intents/structure transforms.
It must expose:

- read-only session snapshots (`EditorSessionState`, `TextEditingValue`),
- controlled edit primitives (`replaceRange`, `setSelection`, `commit`),
- no direct access to syntax scheduler internals, projection internals, or
  controller-private fields.

This prevents out-of-order side effects and keeps intent code deterministic.

### 5.1 Editing pipeline

```dart
abstract class EditingPipeline {
  EditApplyResult applyIncomingValue({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required EditorSessionState state,
  });

  UndoRedoResult undo({
    required TextEditingValue currentValue,
    required EditorSessionState state,
  });

  UndoRedoResult redo({
    required TextEditingValue currentValue,
    required EditorSessionState state,
  });
}
```

### 5.2 Syntax projection

```dart
abstract class SyntaxProjectionCoordinator {
  void scheduleParse({
    required String text,
    required int revision,
    required TextEditingValue currentValue,
    required EditorSessionState state,
  });

  ProjectionUpdate onSnapshot({
    required SyntaxSnapshot snapshot,
    required EditorSessionState state,
    required TextEditingValue currentValue,
  });

  ProjectionUpdate emitDecoration({
    required BlockTree tree,
    required TextEditingValue value,
    required EditOp? op,
    required EditorSessionState state,
  });
}
```

### 5.3 Selection guard

```dart
abstract class SelectionProjectionGuard {
  TextSelection projectAndSnap({
    required TextSelection requested,
    required TextSelection previous,
    required int textLength,
    required Projector projector,
    required CursorValidationMask mask,
  });
}
```

### 5.4 Input intent router

```dart
abstract class InputIntentRouter {
  IntentResult handleEnter(EditorContext context);
  IntentResult handleTab(EditorContext context, {required bool reverse});
  IntentResult handleArrowUp(EditorContext context);
  IntentResult handleArrowDown(EditorContext context);
  IntentResult handleBackspace(EditorContext context);
}
```

### 5.5 Markdown structure services

```dart
abstract class MarkdownStructureQueryService {
  FenceContext? fenceContextForCaret(String text, int caret);
  QuoteContext? quoteContextForLine(String text, int line);
  ParsedTableLine? parseTableLine(String text, LineIndex index, int line);
  ListMarkerContext? listMarkerForLine(String text, int lineStart, int lineEnd);
}

abstract class MarkdownStructureTransformService {
  StructuralEditResult continueList(EditorContext context);
  StructuralEditResult exitFence(EditorContext context);
  StructuralEditResult normalizeTableTab(EditorContext context, {required bool reverse});
  StructuralEditResult applyFenceBackspacePolicy(EditorContext context);
}
```

### 5.6 Text renderer

```dart
abstract class TextRenderer {
  TextSpan render(RenderContext context);
}
```

---

## 6. Dependency Direction (must hold)

1. `controller` -> `core/*` + `commands/*` + `engine/*` + `logic/*`
2. `core/intents` -> `core/structure` + `core/pipeline` (via interfaces)
3. `core/syntax` -> `engine/*` + `logic/*` + `models/*`
4. `core/rendering` -> `theme/*` + `models/*` + `logic/*`
5. No module imports `controller` internals directly.
6. Core data flow must be return-value driven. No internal pub/sub event-bus
   pattern for edit/projection/state transitions.

---

## 7. Mapping from current files

### 7.1 Move into `core/pipeline`

- `controllers/sovereign_value_mutation_coordinator.dart`
- `controllers/undo_stack.dart`
- `controllers/sovereign_differ.dart`

### 7.2 Move into `core/syntax`

- `controllers/sovereign_syntax_sync_coordinator.dart`
- `controllers/sovereign_predictive_reconciler.dart`

### 7.3 Move into `core/intents`

- `core/intents/input_intent_handler.dart`
- `core/intents/input_intent_enter_handler.dart`
- `core/intents/input_intent_tab_handler.dart`
- `core/intents/input_intent_navigation_handler.dart`
- `core/intents/input_intent_backspace_handler.dart`

### 7.4 Move into `core/structure`

- `core/structure/navigation/sovereign_navigation_helpers.dart`
- `core/structure/markdown_structure_query_service.dart`
- `controllers/sovereign_markdown_line_helpers.dart`
- table/fence/list/quote structural helper sections from policy files

### 7.5 Move into `core/rendering`

- all `controllers/sovereign_text_renderer*.dart`

### 7.6 Keep as command internals

- existing command implementation helpers under
  `lib/src/widgets/sovereign/commands/internal/*.dart`

---

## 8. Migration Strategy (low-risk sequence)

1. Introduce `EditorSessionState` + `SelectionProjectionGuard` first.
1.5. Define target abstract interfaces and adapt existing `part` classes to
     implement them in-place before moving files physically.
2. Move pipeline classes (`differ/undo/value mutation`) under `core/pipeline`.
3. Move syntax coordinator/reconciler under `core/syntax`.
4. Move intent handlers under `core/intents`, then remove policy leakage from controller.
5. Move structure helpers under `core/structure`.
6. Move renderer classes under `core/rendering`.
7. Collapse controller to orchestration-only and remove `part` usage for moved modules.

---

## 9. Acceptance Criteria

1. `SovereignController` <= 700 LOC and no structural helper implementations.
2. No `part` references to moved modules.
3. Existing behavior/test suite remains green.
4. Focused tests exist for each module boundary:
   - pipeline invariants,
   - syntax reconciliation + cursor safety,
   - intent routing decisions,
   - structure query/transform correctness,
   - renderer output determinism.

# Flark v2 Migration Guide

Status date: 2026-05-03

Flark is now v2-only from the stable package import:

```dart
import 'package:flark/flark.dart';
```

The legacy v1 API has been removed. Use the source-first API:

- `FlarkFlutterController`
- `FlarkMarkdownEditingExtensions.standard()`
- `MarkdownEditor`
- `Markdown`
- `FlarkNativeComrakParseBackend`
- render-plan descriptors and preview block builders

## Why v2 Exists

v1 grew from a Flutter text controller and widget pipeline. That made early
integration practical, but it left too much behavior coupled to UI state:
editing policies, projection, syntax parsing, render metadata, and preview
rendering could drift.

v2 makes source markdown the durable document, moves behavior into pure Dart
runtime modules, and treats Flutter widgets as adapters over that runtime.
Every edit flows through typed source transactions, parser output feeds a
headless projection model, and editable/read-only surfaces consume the same
render plan.

## Minimal v2 Editor

Create a controller with the standard markdown editing extension set:

```dart
final controller = FlarkFlutterController.fromMarkdown(
  initialMarkdown,
  extensions: FlarkMarkdownEditingExtensions.standard(),
);
```

Render the editor:

```dart
MarkdownEditor(
  controller: controller,
  editingMode: FlarkMarkdownEditingMode.liveRendered,
)
```

When `parseBackend` is omitted, the promoted v2 widgets require the packaged
Comrak backend. Backend load failures surface directly instead of silently
degrading to a second parser or source-only rendering. Pass a custom
`parseBackend` for non-Comrak parser strategies.

## Minimal v2 Preview

For a standalone read-only preview:

```dart
Markdown(
  markdown: markdown,
)
```

For split-pane editor/preview experiences, share one
`FlarkFlutterController` and use `Markdown(controller: ...)`
so the preview tracks the editor's parse/render state. The mounted editor owns
parser scheduling when `parseBackend` is omitted:

```dart
Column(
  children: [
    MarkdownEditor(
      controller: controller,
    ),
    Markdown(controller: controller),
  ],
)
```

## Preview and Render Customization

Use `Markdown`'s `blockBuilder` for custom block rendering while
still consuming the shared source/projection/render-plan state:

```dart
Markdown(
  controller: controller,
  blockBuilder: (context, block, displayText, style) {
    if (block.codeBlock == null) return null;
    return Text(displayText.substring(
      block.displayRange.start,
      block.displayRange.end,
    ));
  },
)
```

For semantic render-plan changes that should travel with the editor runtime,
register a `FlarkRenderPlanExtension` in the controller's extension set.

## Command Migration

v1:

```dart
final result = controller.commands.toggleBold();
```

v2:

```dart
final result = controller.dispatch(
  command: FlarkMarkdownInlineCommands.toggleInlineStyle,
  payload: const FlarkToggleInlineStylePayload(
    FlarkMarkdownInlineStyle.strong,
  ),
);
```

This is more explicit by design. Commands are typed, can be registered by
extensions, and return a runtime result that includes the updated immutable
runtime plus command status.

Use capability queries for toolbar state:

```dart
final capabilities = FlarkMarkdownCommandQueries.capabilitiesAtSelection(
  controller.state,
);
```

## Typed Controller Events

`FlarkFlutterController` is still a `ChangeNotifier` for Flutter widget
rebuilds, but integrations that need surgical reactions should listen to the
typed event stream:

```dart
final subscription = controller.events.listen((event) {
  if (event.kind == FlarkControllerEventKind.parseAdopted) {
    // Recompute parser-dependent extension state.
  }
});
```

Events distinguish projection prediction, parser adoption, selection-only
changes, undo, redo, and generic runtime changes.

## Projection and Source Text

`FlarkFlutterController.markdown` is always the canonical source document.
Projected editing hides parser-provided marker ranges from the editing surface
but maps selections and edits back to source offsets.

Use `FlarkMarkdownEditingMode.source` when users must see literal markdown
syntax, `FlarkMarkdownEditingMode.projected` when they should edit clean
marker-hidden text, and `FlarkMarkdownEditingMode.liveRendered` when they
should get rendered-in-place inline styling and editable task, code-fence, and
table block widgets while still editing canonical Markdown source.

## Overlay and Render-Plan Migration

v1 inline overlays discover link/image/task/table structure from widget-local
state. v2 exposes typed render-plan overlay targets:

```dart
MarkdownEditor(
  controller: controller,
  showOverlayControls: true,
  onOverlayTargetPressed: (target) {
    // Link, image, task, table, and code targets are typed descriptors.
  },
)
```

Applications with their own design system should prefer custom
`FlarkOverlayTargetWidgetBuilder` controls on `MarkdownEditor` or
`Markdown` over deep imports or reparsing.

## Compatibility Boundary

There is no compatibility layer in this package. Keep migrations explicit:

- replace `FlarkController` with `FlarkFlutterController`;
- replace `FlarkEditor` with `MarkdownEditor`;
- replace `FlarkMarkdownView` with `Markdown`;
- replace toolbar helpers with typed command dispatch;
- replace widget-local styling hooks with Flutter text styles, preview
  `blockBuilder`s, or render-plan extensions.

The v2 runtime, parser protocol, projection model, render plan, and Flutter
adapters are the only architecture to extend.

# Cookbook

All examples use the supported Flutter barrel:

```dart
import 'package:flark/flark.dart';
```

## Open and close a document

```dart
final controller = await FlarkEditorController.open('# Notes\n');
try {
  // Mount FlarkEditor or FlarkMarkdownView with this controller.
} finally {
  await controller.close();
}
```

In a widget, keep the controller in `State` and close it from `dispose`; see
[Getting started](getting_started.md) for the complete ownership pattern.

## Edit and save exact source

```dart
controller.replaceSelection('replacement');
await controller.debugWaitForMutationSettled();
final markdown = await controller.readSource();
```

`replaceSelection`, `deleteBackward`, `deleteForward`, and `insertNewline`
enter the same serialized edit pipeline as platform text input. UI code should
usually observe the controller rather than use the debug settling methods;
those methods primarily support deterministic integration tests.

## Undo and redo

```dart
if (controller.canUndo) await controller.undo();
if (controller.canRedo) await controller.redo();
```

Rebuild toolbar state from controller notifications. Do not maintain a second
application-side history stack.

## Share editor and preview state

```dart
Column(
  children: [
    Expanded(child: FlarkEditor(controller: controller)),
    Expanded(child: FlarkMarkdownView(controller: controller)),
  ],
)
```

Both surfaces consume the same source, parser, viewport, and certification
state. The application remains responsible for layout.

## Stream a large UTF-8 document

```dart
final controller = await FlarkEditorController.openUtf8Stream(byteChunks);
await controller.firstCertifiedPublication;
```

`byteChunks` is a `Stream<Uint8List>`. The `opening-session` native feature is
still feature-gated in repository builds; probe
`FlarkEditorController.streamedOpenSupported()` before exposing the path in a
general-purpose host. `FlarkEditorStatus.streaming` distinguishes an editable
opening controller from one whose admission has completed. Headless Core hosts
can additionally await `FlarkCoreDocument.openingSealed`.

## Activate semantic targets

Pass a callback to the read-only surface:

```dart
FlarkMarkdownView(
  controller: controller,
  onSemanticTarget: (target) {
    // Apply application-owned URL and navigation policy here.
  },
)
```

Flark reports the parsed target; the host decides whether and how to open it.

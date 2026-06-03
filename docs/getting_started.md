# Getting Started

## Editor

```dart
import 'package:flark/flark.dart';

MarkdownEditor(
  initialMarkdown: '# Notes\n\n- Write Markdown\n- Keep source truth',
  editingMode: FlarkMarkdownEditingMode.liveRendered,
  onChanged: (markdown) {
    // Persist markdown.
  },
)
```

`initialMarkdown` creates a widget-owned controller. Use it for forms, scratch
pads, and simple single-document editors.

## Preview

```dart
Markdown(markdown: '# Preview\n\nRendered from Markdown.')
```

Use this when the preview has its own source string.

## Shared State

Use `FlarkFlutterController` when an editor, preview, toolbar, or save button
needs the same document state. The controller owns parsing — configure the
parser on it, not on the widgets — and a single parser is shared across every
attached surface.

```dart
final controller = FlarkFlutterController.fromMarkdown(
  markdown,
  parseDebounce: const Duration(milliseconds: 40),
);

Row(
  children: [
    Expanded(child: MarkdownEditor(controller: controller)),
    Expanded(child: Markdown(controller: controller)),
  ],
)
```

Dispose app-owned controllers when the owning widget is disposed. Passing
`parseBackend`, `parseProfile`, `parseDebounce`, or `onParseError` to a widget
that already has a `controller` asserts; set them on the controller instead.

## Observing Changes

Observe `controller.events` for typed change events, or the convenience
projections for the common cases:

```dart
controller.markdownChanges.listen(save);
controller.selectionChanges.listen(updateToolbarState);
```

## Toolbar Commands and Shortcuts

Toolbar buttons call command helpers on the controller; the same commands are
bound to keyboard accelerators. `MarkdownEditor` installs a default map
(Cmd/Ctrl + B/I/E, Cmd/Ctrl+Shift+X) via `useDefaultShortcuts`. Add or override
bindings with `FlarkMarkdownShortcuts`:

```dart
MarkdownEditor(
  controller: controller,
  shortcuts: {
    const SingleActivator(LogicalKeyboardKey.keyK, meta: true):
        FlarkMarkdownShortcuts.toggleInlineCode(),
  },
)
```

## Editing Modes

- `FlarkMarkdownEditingMode.source`: raw Markdown text.
- `FlarkMarkdownEditingMode.projected`: Markdown markers are hidden while edits
  still map back to source.
- `FlarkMarkdownEditingMode.liveRendered`: projected text plus rendered inline
  styling and editable task, table, code-fence, and quote blocks.

## Parse Errors

Use `onParseError` to log or surface scheduled parser failures.

```dart
MarkdownEditor(
  initialMarkdown: markdown,
  onParseError: (error, stackTrace) {
    debugPrint('Markdown parser failed: $error');
  },
)
```

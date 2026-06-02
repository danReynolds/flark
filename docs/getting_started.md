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
needs the same document state.

```dart
final controller = FlarkFlutterController.fromMarkdown(markdown);

Row(
  children: [
    Expanded(child: MarkdownEditor(controller: controller)),
    Expanded(child: Markdown(controller: controller)),
  ],
)
```

Dispose app-owned controllers when the owning widget is disposed.

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

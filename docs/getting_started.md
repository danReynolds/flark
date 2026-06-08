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

## Forms

Use `MarkdownEditorFormField` when the editor participates in a Flutter `Form`.
It is a thin wrapper around `MarkdownEditor`: validation, saving,
autovalidation, reset, and restoration use Flutter's standard `FormField`
machinery, while Markdown editing still goes through the same controller and
parser paths.

```dart
final formKey = GlobalKey<FormState>();

Form(
  key: formKey,
  child: MarkdownEditorFormField(
    initialMarkdown: draftBody,
    editingMode: FlarkMarkdownEditingMode.liveRendered,
    validator: (markdown) {
      return markdown == null || markdown.trim().isEmpty
          ? 'Body is required'
          : null;
    },
    onSaved: saveDraftBody,
  ),
)
```

Pass `controller` instead of `initialMarkdown` when the form field shares state
with preview, toolbar, or save-button UI. As with `MarkdownEditor`, a shared
controller owns parser configuration.

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

Toolbar buttons call `controller.commands`; the same commands are bound to
keyboard accelerators. Read getters expose active state for selected buttons,
and mutation methods dispatch edits:

```dart
AnimatedBuilder(
  animation: controller,
  builder: (context, _) {
    final commands = controller.commands;

    return IconButton(
      icon: const Icon(Icons.format_bold),
      isSelected: commands.strongActive,
      onPressed: commands.canMutate ? commands.toggleStrong : null,
    );
  },
)
```

`FlarkFlutterController` is a `ChangeNotifier`, so toolbar UI can rebuild from
the controller, `controller.selectionChanges`, or `controller.events`.

`MarkdownEditor` installs a default shortcut map (Cmd/Ctrl + B/I/E,
Cmd/Ctrl+Shift+X) via `useDefaultShortcuts`. Add or override bindings with
`FlarkMarkdownShortcuts`:

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

- `FlarkMarkdownEditingMode.source`: exact Markdown source, including markers
  like `**strong**`, `_emphasis_`, links, fences, and table pipes.
- `FlarkMarkdownEditingMode.liveRendered`: recommended for most app editors.
  It keeps Markdown as the source of truth while hiding common markers, showing
  rendered inline styling, and providing editable task, table, code-fence, and
  quote blocks.

## Accessibility

The live-rendered surface composes editable block fields, so each block exposes
a standard text-field semantics node. Interactive chrome is labeled for
assistive technology: task checkboxes report a checkbox role with checked state
(`"Task, completed"` / `"Task, not completed"`), and the code-fence copy control
is a labeled button. IME composition into block fields groups into single undo
steps. Coverage lives in `test/v2/flutter/flark_live_rendered_a11y_test.dart`.

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

## More Recipes

See the [Cookbook](cookbook.md) for copy-pasteable toolbar, form, dirty-save,
link-dialog, document-switching, and custom-preview examples.

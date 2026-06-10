# Getting Started

## Editor

```dart
import 'package:flark/flark.dart';

FlarkMarkdownEditor(
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

Use `FlarkMarkdownEditorFormField` when the editor participates in a Flutter `Form`.
It is a thin wrapper around `FlarkMarkdownEditor`: validation, saving,
autovalidation, and reset use Flutter's standard `FormField` machinery
(`restorationId` is forwarded to `FormField` and follows its standard
behavior), while Markdown editing still goes through the same controller and
parser paths. `enabled: false` renders the field read-only.

```dart
final formKey = GlobalKey<FormState>();

Form(
  key: formKey,
  child: FlarkMarkdownEditorFormField(
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
with preview, toolbar, or save-button UI. As with `FlarkMarkdownEditor`, a shared
controller owns parser configuration.

## Preview

```dart
FlarkMarkdown(markdown: '# Preview\n\nRendered from FlarkMarkdown.')
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
    Expanded(child: FlarkMarkdownEditor(controller: controller)),
    Expanded(child: FlarkMarkdown(controller: controller)),
  ],
)
```

Dispose app-owned controllers when the owning widget is disposed. Passing
`parseBackend`, `parseProfile`, `parseDebounce`, or `onParseError` to a widget
that already has a `controller` asserts; set them on the controller instead.
(`profile` is a deprecated alias of `parseProfile` and will be removed before
1.0.)

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

`FlarkMarkdownEditor` installs a default shortcut map (Cmd/Ctrl + B/I/E,
Cmd/Ctrl+Shift+X) via `useDefaultShortcuts`. Add or override bindings with
`FlarkMarkdownShortcuts`:

```dart
FlarkMarkdownEditor(
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

## Theming

Chrome colors (code fences, quotes, links, tables, checkboxes, menus, syntax
highlighting) come from `FlarkMarkdownThemeData`. With no configuration the
palette follows platform brightness. Pass `theme:` to a widget, or wrap a
subtree in `FlarkMarkdownTheme` to theme every Flark surface below it:

```dart
FlarkMarkdownTheme(
  data: FlarkMarkdownThemeData.dark,
  child: ...,
)
```

Start from `FlarkMarkdownThemeData.light`/`.dark` and adjust with `copyWith`.
Fonts and text sizes stay on the widget `style`/`textStyle` parameters.

## Read-Only Surfaces

- `FlarkMarkdownEditor(readOnly: true)` renders the live document without
  accepting edits, shortcuts, or block mutations.
- `FlarkMarkdown(selectable: true)` lets users select and copy preview text
  (requires an `Overlay` ancestor, which `MaterialApp`/`CupertinoApp`
  provide).

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
FlarkMarkdownEditor(
  initialMarkdown: markdown,
  onParseError: (error, stackTrace) {
    debugPrint('Markdown parser failed: $error');
  },
)
```

## More Recipes

See the [Cookbook](cookbook.md) for copy-pasteable toolbar, form, dirty-save,
link-dialog, document-switching, and custom-preview examples.

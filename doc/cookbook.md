# Cookbook

These recipes show the app-facing patterns Flark expects developers to copy.
Use the app barrel unless a recipe explicitly says otherwise:

```dart
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flark/flark.dart';
```

## Simple Editor

Use `initialMarkdown` when the widget can own its controller.

```dart
FlarkMarkdownEditor(
  initialMarkdown: '# Notes\n\nWrite FlarkMarkdown.',
  onChanged: saveMarkdown,
)
```

`FlarkMarkdownEditor` defaults to `FlarkMarkdownEditingMode.liveRendered`, which is
the recommended mode for most app editors.

## Source Editor

Use source mode when users need to see exact Markdown markers.

```dart
FlarkMarkdownEditor(
  initialMarkdown: markdown,
  editingMode: FlarkMarkdownEditingMode.source,
  onChanged: saveMarkdown,
)
```

## Shared Editor And Preview

Use a controller when editor, preview, toolbar, save UI, or analytics need the
same document state.

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

Dispose app-owned controllers from the owning widget's `dispose` method.

## Toolbar

Toolbar UI belongs to the app. Flark provides the command reads and writes
through `controller.commands`.

```dart
class MarkdownToolbar extends StatelessWidget {
  const MarkdownToolbar({super.key, required this.controller});

  final FlarkFlutterController controller;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        final commands = controller.commands;

        return Row(
          children: [
            IconButton(
              tooltip: 'Bold',
              icon: const Icon(Icons.format_bold),
              isSelected: commands.strongActive,
              onPressed: commands.canMutate ? commands.toggleStrong : null,
            ),
            IconButton(
              tooltip: 'Heading 2',
              icon: const Icon(Icons.looks_two_outlined),
              isSelected: commands.headingLevel == 2,
              onPressed: commands.canMutate
                  ? () => commands.setHeadingLevel(2)
                  : null,
            ),
            IconButton(
              tooltip: 'Bulleted list',
              icon: const Icon(Icons.format_list_bulleted),
              isSelected: commands.bulletListActive,
              onPressed: commands.canMutate
                  ? commands.toggleBulletList
                  : null,
            ),
            IconButton(
              tooltip: 'Code fence',
              icon: const Icon(Icons.code),
              onPressed: commands.canMutate
                  ? () => commands.insertCodeFence(language: 'dart')
                  : null,
            ),
            IconButton(
              tooltip: 'Undo',
              icon: const Icon(Icons.undo),
              onPressed: commands.canUndo ? commands.undo : null,
            ),
            IconButton(
              tooltip: 'Redo',
              icon: const Icon(Icons.redo),
              onPressed: commands.canRedo ? commands.redo : null,
            ),
          ],
        );
      },
    );
  }
}
```

Use `AnimatedBuilder`, `controller.selectionChanges`, or `controller.events` to
refresh active toolbar state when selection or document state changes.

## Form Field

Use `FlarkMarkdownEditorFormField` when the editor participates in Flutter form
validation, save, reset, autovalidation, or restoration.

```dart
final formKey = GlobalKey<FormState>();

Form(
  key: formKey,
  child: FlarkMarkdownEditorFormField(
    initialMarkdown: draftBody,
    validator: (markdown) {
      return markdown == null || markdown.trim().isEmpty
          ? 'Body is required'
          : null;
    },
    onSaved: saveDraftBody,
  ),
)
```

Pass `controller` instead of `initialMarkdown` when the field shares state with
a preview, toolbar, or save button.

## Dirty Save Button

Track dirty state from `markdownChanges` when save UI lives outside a `Form`.

```dart
class DraftEditor extends StatefulWidget {
  const DraftEditor({
    super.key,
    required this.initialMarkdown,
    required this.onSave,
  });

  final String initialMarkdown;
  final Future<void> Function(String markdown) onSave;

  @override
  State<DraftEditor> createState() => _DraftEditorState();
}

class _DraftEditorState extends State<DraftEditor> {
  late final FlarkFlutterController _controller;
  late String _savedMarkdown;
  StreamSubscription<String>? _markdownSub;
  bool _dirty = false;

  @override
  void initState() {
    super.initState();
    _savedMarkdown = widget.initialMarkdown;
    _controller = FlarkFlutterController.fromMarkdown(widget.initialMarkdown);
    _markdownSub = _controller.markdownChanges.listen((markdown) {
      if (!mounted) return;
      setState(() => _dirty = markdown != _savedMarkdown);
    });
  }

  @override
  void dispose() {
    _markdownSub?.cancel();
    _controller.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final markdown = _controller.markdown;
    await widget.onSave(markdown);
    setState(() {
      _savedMarkdown = markdown;
      _dirty = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Expanded(child: FlarkMarkdownEditor(controller: _controller)),
        FilledButton(
          onPressed: _dirty ? _save : null,
          child: const Text('Save'),
        ),
      ],
    );
  }
}
```

## Link Edit Dialog

Let the app own dialog UI and pass the result back through the command facade.
`LinkEditDialog` below is your app's dialog widget.

```dart
final class LinkEditResult {
  const LinkEditResult({required this.label, required this.url});

  final String label;
  final String url;
}

Future<void> editLink(
  BuildContext context,
  FlarkFlutterController controller,
) async {
  final commands = controller.commands;
  final linkContext = commands.resolveLinkEditContext();
  final result = await showDialog<LinkEditResult>(
    context: context,
    builder: (context) {
      return LinkEditDialog(
        initialLabel: linkContext.label,
        initialUrl: linkContext.url,
      );
    },
  );

  if (result == null) return;
  commands.applyLinkEdit(
    context: linkContext,
    label: result.label,
    url: result.url,
  );
}
```

If `linkContext.isExisting` is true, the same flow edits the existing Markdown
link under the cursor or selection.

## Document Switching

For a widget-owned controller, give each document a stable key:

```dart
FlarkMarkdownEditor(
  key: ValueKey(document.id),
  initialMarkdown: document.markdown,
  onChanged: saveDraft,
)
```

For app-owned controller state, replace the controller when document identity
changes:

```dart
void loadDocument(String markdown) {
  setState(() {
    _controller.dispose();
    _controller = FlarkFlutterController.fromMarkdown(markdown);
  });
}
```

## Custom Preview Blocks

Use `blockBuilder` to replace only the blocks your app wants to customize.
Return `null` to keep Flark's default rendering for a block.

```dart
FlarkMarkdown(
  controller: controller,
  blockBuilder: (context, block, displayText, baseStyle) {
    final codeBlock = block.codeBlock;
    if (codeBlock == null) return null;

    final code = displayText.substring(
      block.displayRange.start,
      block.displayRange.end,
    );

    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFF4F6F8),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Text(
          code,
          style: baseStyle.copyWith(fontFamily: 'monospace'),
        ),
      ),
    );
  },
)
```

## Opening Links

Tapping a link calls `onOpenLink` with its destination. Wire it to
`url_launcher` (or your own router):

```dart
FlarkMarkdown(
  markdown: body,
  interactionConfig: FlarkMarkdownInteractionConfig(
    onOpenLink: (destination) {
      launchUrl(Uri.parse(destination));
    },
  ),
)
```

The same configuration works on `FlarkMarkdownEditor`. With an `onOpenLink`
handler, tap opens the link and long-press shows the copy/edit menu; without
one, tap shows the menu.

## Dark Mode and Custom Colors

Pass a `FlarkMarkdownThemeData`, or wrap a subtree in `FlarkMarkdownTheme` to
theme every Flark surface below it. To follow your app's Material theme:

```dart
Builder(
  builder: (context) {
    final brightness = Theme.of(context).brightness;
    return FlarkMarkdownEditor(
      controller: controller,
      theme: FlarkMarkdownThemeData.fromBrightness(brightness)
          .copyWith(linkColor: Theme.of(context).colorScheme.primary),
    );
  },
)
```

Without a theme, Flark follows platform brightness.

## Pre-Warming a Preview

A standalone preview parses asynchronously, so the first frame may show plain
text. To render fully on the first frame, own the controller and await an
immediate parse before showing the widget:

```dart
final controller = FlarkFlutterController.fromMarkdown(markdown);
await controller.parseNow();
// controller.hasAuthoritativeRenderPlan is now true.
runApp(... FlarkMarkdown(controller: controller) ...);
```

`parseNow()` resolves once the current document revision has an authoritative
render plan, including when a scheduled parse is already in flight.

## Parse Errors

Use `onParseError` for logging or app-level fallback UI.

```dart
FlarkMarkdownEditor(
  initialMarkdown: markdown,
  onParseError: (error, stackTrace) {
    debugPrint('Markdown parser failed: $error');
  },
)
```

When passing a shared `controller`, configure parser options on the controller
instead of on `FlarkMarkdownEditor` or `FlarkMarkdown`.

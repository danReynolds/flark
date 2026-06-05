import 'package:flutter/material.dart';
import 'package:flark/flark.dart';

const _sampleMarkdown = '''# Flark Markdown

Native Comrak parsing with live source-first editing.

- [x] Editable markdown surface
- [ ] Read-only preview surface
- [ ] Link and code actions

> A package consumer should import only the public barrel.

```dart
final controller = FlarkFlutterController.fromMarkdown(markdown);
```
''';

const _tableMarkdown = '''# Tables

| Feature | Status |
| --- | --- |
| Live editor | Ready |
| Preview | Ready |
''';

const _articleMarkdown = '''# Release Notes

Flark keeps Markdown as the canonical source while exposing rendered block
widgets for common editing flows.

## Highlights

- Live-rendered list, quote, task, table, and code-fence editing
- Shared parser-backed preview
- Comrak on native and web

```dart
final editor = MarkdownEditor(
  controller: controller,
  editingMode: FlarkMarkdownEditingMode.liveRendered,
);
```
''';

void main() {
  runApp(const FlarkExampleApp());
}

class FlarkExampleApp extends StatelessWidget {
  const FlarkExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    final scheme = ColorScheme.fromSeed(
      seedColor: const Color(0xFF25636A),
      brightness: Brightness.light,
    );
    return MaterialApp(
      title: 'Flark',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: scheme,
        scaffoldBackgroundColor: const Color(0xFFF5F7FA),
        dividerColor: const Color(0xFFD9E1EA),
        filledButtonTheme: FilledButtonThemeData(
          style: FilledButton.styleFrom(
            minimumSize: const Size(40, 38),
            shape: const RoundedRectangleBorder(
              borderRadius: BorderRadius.all(Radius.circular(8)),
            ),
          ),
        ),
        outlinedButtonTheme: OutlinedButtonThemeData(
          style: OutlinedButton.styleFrom(
            minimumSize: const Size(40, 38),
            shape: const RoundedRectangleBorder(
              borderRadius: BorderRadius.all(Radius.circular(8)),
            ),
          ),
        ),
        segmentedButtonTheme: SegmentedButtonThemeData(
          style: ButtonStyle(
            visualDensity: VisualDensity.compact,
            shape: WidgetStateProperty.all(
              const RoundedRectangleBorder(
                borderRadius: BorderRadius.all(Radius.circular(8)),
              ),
            ),
          ),
        ),
      ),
      home: const FlarkExampleScreen(),
    );
  }
}

enum _WorkspaceMode { source, liveRendered, rendered }

class FlarkExampleScreen extends StatefulWidget {
  const FlarkExampleScreen({super.key});

  @override
  State<FlarkExampleScreen> createState() => _FlarkExampleScreenState();
}

class _FlarkExampleScreenState extends State<FlarkExampleScreen> {
  late FlarkFlutterController _controller;
  final FocusNode _focusNode = FocusNode();
  _WorkspaceMode _mode = _WorkspaceMode.liveRendered;

  @override
  void initState() {
    super.initState();
    _controller = _createController(_sampleMarkdown);
  }

  FlarkFlutterController _createController(String markdown) {
    return FlarkFlutterController.fromMarkdown(
      markdown,
      parseDebounce: const Duration(milliseconds: 40),
    );
  }

  @override
  void dispose() {
    _focusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _PlaygroundToolbar(
              controller: _controller,
              mode: _mode,
              onModeChanged: (mode) => setState(() => _mode = mode),
              onSample: () => _loadDocument(_sampleMarkdown),
              onArticle: () => _loadDocument(_articleMarkdown),
              onTables: () => _loadDocument(_tableMarkdown),
              onScratch: () => _loadDocument(''),
              onCommand: _runCommand,
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
                child: _Workspace(
                  controller: _controller,
                  focusNode: _focusNode,
                  mode: _mode,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _loadDocument(String markdown) {
    setState(() {
      _focusNode.unfocus();
      _controller.dispose();
      _controller = _createController(markdown);
      _mode = _WorkspaceMode.liveRendered;
    });
  }

  void _runCommand(_ToolbarCommand command) {
    final before = _controller.state.revision;
    final commands = _controller.commands;
    switch (command) {
      case _ToolbarCommand.heading1:
        commands.setHeadingLevel(1, userEvent: 'example.toolbar.heading1');
      case _ToolbarCommand.heading2:
        commands.setHeadingLevel(2, userEvent: 'example.toolbar.heading2');
      case _ToolbarCommand.bold:
        commands.toggleStrong(userEvent: 'example.toolbar.bold');
      case _ToolbarCommand.italic:
        commands.toggleEmphasis(userEvent: 'example.toolbar.italic');
      case _ToolbarCommand.quote:
        commands.toggleQuote(userEvent: 'example.toolbar.quote');
      case _ToolbarCommand.bulletedList:
        commands.toggleBulletList(userEvent: 'example.toolbar.bulletList');
      case _ToolbarCommand.orderedList:
        commands.toggleOrderedList(userEvent: 'example.toolbar.orderedList');
      case _ToolbarCommand.taskList:
        commands.toggleTaskList(userEvent: 'example.toolbar.taskList');
      case _ToolbarCommand.codeFence:
        commands.insertCodeFence(
          language: 'dart',
          userEvent: 'example.toolbar.codeFence',
        );
      case _ToolbarCommand.table:
        commands.insertTable(
          columns: 3,
          bodyRows: 2,
          userEvent: 'example.toolbar.table',
        );
    }

    if (_controller.state.revision != before) {
      setState(() {
        if (_mode == _WorkspaceMode.rendered) {
          _mode = _WorkspaceMode.liveRendered;
        }
      });
      _focusNode.requestFocus();
    }
  }
}

class _PlaygroundToolbar extends StatelessWidget {
  const _PlaygroundToolbar({
    required this.controller,
    required this.mode,
    required this.onModeChanged,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final FlarkFlutterController controller;
  final _WorkspaceMode mode;
  final ValueChanged<_WorkspaceMode> onModeChanged;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: Color(0xFFFFFFFF),
        border: Border(bottom: BorderSide(color: Color(0xFFD9E1EA))),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
        child: LayoutBuilder(
          builder: (context, constraints) {
            final compact = constraints.maxWidth < 820;
            final header = _ToolbarHeader(
              controller: controller,
              mode: mode,
              onModeChanged: onModeChanged,
            );
            final actions = _ToolbarActions(
              compact: compact,
              onSample: onSample,
              onArticle: onArticle,
              onTables: onTables,
              onScratch: onScratch,
              onCommand: onCommand,
            );
            if (compact) {
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [header, const SizedBox(height: 10), actions],
              );
            }
            return Row(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Expanded(child: header),
                const SizedBox(width: 18),
                Flexible(flex: 2, child: actions),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _ToolbarHeader extends StatelessWidget {
  const _ToolbarHeader({
    required this.controller,
    required this.mode,
    required this.onModeChanged,
  });

  final FlarkFlutterController controller;
  final _WorkspaceMode mode;
  final ValueChanged<_WorkspaceMode> onModeChanged;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: 12,
      runSpacing: 10,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 290),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              const _ProductMark(),
              const SizedBox(width: 10),
              Flexible(
                child: Text(
                  'Flark Markdown',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.titleLarge?.copyWith(
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0,
                    color: const Color(0xFF16202A),
                  ),
                ),
              ),
            ],
          ),
        ),
        const _StatusChip(label: 'Comrak'),
        _ControllerStats(controller: controller),
        _ModeBar(mode: mode, onModeChanged: onModeChanged),
      ],
    );
  }
}

class _ProductMark extends StatelessWidget {
  const _ProductMark();

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.primary,
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: const SizedBox(
        width: 34,
        height: 34,
        child: Icon(Icons.edit_note, size: 22, color: Color(0xFFFFFFFF)),
      ),
    );
  }
}

class _ToolbarActions extends StatelessWidget {
  const _ToolbarActions({
    required this.compact,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final bool compact;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    final children = [
      _ScenarioButton(
        key: const ValueKey('flark-example-scenario-sample'),
        icon: Icons.description_outlined,
        label: 'Sample',
        onPressed: onSample,
      ),
      _ScenarioButton(
        key: const ValueKey('flark-example-scenario-article'),
        icon: Icons.article_outlined,
        label: 'Article',
        onPressed: onArticle,
      ),
      _ScenarioButton(
        key: const ValueKey('flark-example-scenario-tables'),
        icon: Icons.table_chart_outlined,
        label: 'Tables',
        onPressed: onTables,
      ),
      _ScenarioButton(
        key: const ValueKey('flark-example-scenario-scratch'),
        icon: Icons.add,
        label: 'Scratch',
        onPressed: onScratch,
        emphasized: true,
      ),
      const _ToolbarDivider(),
      _CommandButton(
        tooltip: 'Heading 1',
        icon: Icons.looks_one_outlined,
        onPressed: () => onCommand(_ToolbarCommand.heading1),
      ),
      _CommandButton(
        tooltip: 'Heading 2',
        icon: Icons.looks_two_outlined,
        onPressed: () => onCommand(_ToolbarCommand.heading2),
      ),
      _CommandButton(
        tooltip: 'Bold',
        icon: Icons.format_bold,
        onPressed: () => onCommand(_ToolbarCommand.bold),
      ),
      _CommandButton(
        tooltip: 'Italic',
        icon: Icons.format_italic,
        onPressed: () => onCommand(_ToolbarCommand.italic),
      ),
      _CommandButton(
        buttonKey: const ValueKey('flark-example-command-quote'),
        tooltip: 'Quote',
        icon: Icons.format_quote,
        onPressed: () => onCommand(_ToolbarCommand.quote),
      ),
      _CommandButton(
        tooltip: 'Bulleted list',
        icon: Icons.format_list_bulleted,
        onPressed: () => onCommand(_ToolbarCommand.bulletedList),
      ),
      _CommandButton(
        tooltip: 'Numbered list',
        icon: Icons.format_list_numbered,
        onPressed: () => onCommand(_ToolbarCommand.orderedList),
      ),
      _CommandButton(
        tooltip: 'Task list',
        icon: Icons.checklist,
        onPressed: () => onCommand(_ToolbarCommand.taskList),
      ),
      _CommandButton(
        buttonKey: const ValueKey('flark-example-command-code-fence'),
        tooltip: 'Code fence',
        icon: Icons.code,
        onPressed: () => onCommand(_ToolbarCommand.codeFence),
      ),
      _CommandButton(
        buttonKey: const ValueKey('flark-example-command-table'),
        tooltip: 'Table',
        icon: Icons.table_rows_outlined,
        onPressed: () => onCommand(_ToolbarCommand.table),
      ),
    ];

    return Wrap(
      spacing: 8,
      runSpacing: 8,
      alignment: compact ? WrapAlignment.start : WrapAlignment.end,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: children,
    );
  }
}

class _ScenarioButton extends StatelessWidget {
  const _ScenarioButton({
    super.key,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.emphasized = false,
  });

  final IconData icon;
  final String label;
  final VoidCallback onPressed;
  final bool emphasized;

  @override
  Widget build(BuildContext context) {
    final child = Row(
      mainAxisSize: MainAxisSize.min,
      children: [Icon(icon, size: 18), const SizedBox(width: 7), Text(label)],
    );
    if (emphasized) {
      return FilledButton.tonal(onPressed: onPressed, child: child);
    }
    return OutlinedButton(onPressed: onPressed, child: child);
  }
}

class _ToolbarDivider extends StatelessWidget {
  const _ToolbarDivider();

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 32,
      child: VerticalDivider(
        width: 8,
        thickness: 1,
        color: Theme.of(context).dividerColor,
      ),
    );
  }
}

class _StatusChip extends StatelessWidget {
  const _StatusChip({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFEAF2F3),
        border: Border.all(color: const Color(0xFFC7DDE0)),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        child: Text(
          label,
          style: Theme.of(context).textTheme.labelMedium?.copyWith(
            color: const Color(0xFF244C51),
            fontWeight: FontWeight.w700,
            letterSpacing: 0,
          ),
        ),
      ),
    );
  }
}

class _ControllerStats extends StatelessWidget {
  const _ControllerStats({required this.controller});

  final FlarkFlutterController controller;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, child) {
        final lineCount = controller.markdown.isEmpty
            ? 1
            : '\n'.allMatches(controller.markdown).length + 1;
        final status = controller.hasAuthoritativeRenderPlan
            ? 'Parsed'
            : 'Parsing';
        return Text(
          '$lineCount lines · ${controller.markdown.length} chars · $status',
          style: Theme.of(context).textTheme.labelLarge?.copyWith(
            color: const Color(0xFF52616F),
            letterSpacing: 0,
          ),
        );
      },
    );
  }
}

class _ModeBar extends StatelessWidget {
  const _ModeBar({required this.mode, required this.onModeChanged});

  final _WorkspaceMode mode;
  final ValueChanged<_WorkspaceMode> onModeChanged;

  @override
  Widget build(BuildContext context) {
    return SegmentedButton<_WorkspaceMode>(
      segments: const [
        ButtonSegment(
          value: _WorkspaceMode.source,
          icon: Icon(Icons.subject, size: 18),
          label: Text('Source', key: ValueKey('flark-example-mode-source')),
        ),
        ButtonSegment(
          value: _WorkspaceMode.liveRendered,
          icon: Icon(Icons.edit_note, size: 18),
          label: Text('Live Edit', key: ValueKey('flark-example-mode-live')),
        ),
        ButtonSegment(
          value: _WorkspaceMode.rendered,
          icon: Icon(Icons.visibility_outlined, size: 18),
          label: Text('Rendered', key: ValueKey('flark-example-mode-rendered')),
        ),
      ],
      selected: {mode},
      onSelectionChanged: (selection) => onModeChanged(selection.single),
      showSelectedIcon: false,
    );
  }
}

class _CommandButton extends StatelessWidget {
  const _CommandButton({
    this.buttonKey,
    required this.tooltip,
    required this.icon,
    required this.onPressed,
  });

  final Key? buttonKey;
  final String tooltip;
  final IconData icon;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: tooltip,
      waitDuration: const Duration(milliseconds: 450),
      child: IconButton.outlined(
        key: buttonKey,
        onPressed: onPressed,
        icon: Icon(icon, size: 20),
        style: IconButton.styleFrom(
          foregroundColor: const Color(0xFF25313C),
          side: const BorderSide(color: Color(0xFFD5DEE7)),
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(8)),
          ),
          minimumSize: const Size(40, 38),
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          visualDensity: VisualDensity.compact,
        ),
      ),
    );
  }
}

enum _ToolbarCommand {
  heading1,
  heading2,
  bold,
  italic,
  quote,
  bulletedList,
  orderedList,
  taskList,
  codeFence,
  table,
}

class _Workspace extends StatelessWidget {
  const _Workspace({
    required this.controller,
    required this.focusNode,
    required this.mode,
  });

  final FlarkFlutterController controller;
  final FocusNode focusNode;
  final _WorkspaceMode mode;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        if (mode == _WorkspaceMode.rendered) {
          return _PreviewPane(controller: controller, title: 'Rendered');
        }

        final editor = _EditorPane(
          controller: controller,
          focusNode: focusNode,
          editingMode: mode == _WorkspaceMode.source
              ? FlarkMarkdownEditingMode.source
              : FlarkMarkdownEditingMode.liveRendered,
        );
        final preview = _PreviewPane(controller: controller, title: 'Preview');

        if (constraints.maxWidth < 900) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(flex: 3, child: editor),
              const SizedBox(height: 12),
              Expanded(flex: 2, child: preview),
            ],
          );
        }

        return Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(flex: 6, child: editor),
            const SizedBox(width: 12),
            Expanded(flex: 5, child: preview),
          ],
        );
      },
    );
  }
}

class _EditorPane extends StatelessWidget {
  const _EditorPane({
    required this.controller,
    required this.focusNode,
    required this.editingMode,
  });

  final FlarkFlutterController controller;
  final FocusNode focusNode;
  final FlarkMarkdownEditingMode editingMode;

  @override
  Widget build(BuildContext context) {
    return _WorkbenchPane(
      title: 'Editor',
      icon: Icons.edit_note,
      trailing: _PaneModeLabel(editingMode: editingMode),
      footer: _EditorFooter(controller: controller),
      child: MarkdownEditor(
        controller: controller,
        editingMode: editingMode,
        focusNode: focusNode,
        expands: true,
        maxLines: null,
        cursorColor: Theme.of(context).colorScheme.primary,
        style: const TextStyle(
          fontSize: 16,
          height: 1.42,
          color: Color(0xFF18212B),
        ),
      ),
    );
  }
}

class _PreviewPane extends StatelessWidget {
  const _PreviewPane({required this.controller, required this.title});

  final FlarkFlutterController controller;
  final String title;

  @override
  Widget build(BuildContext context) {
    return _PreviewPaneBody(controller: controller, title: title);
  }
}

class _PreviewPaneBody extends StatefulWidget {
  const _PreviewPaneBody({required this.controller, required this.title});

  final FlarkFlutterController controller;
  final String title;

  @override
  State<_PreviewPaneBody> createState() => _PreviewPaneBodyState();
}

class _PreviewPaneBodyState extends State<_PreviewPaneBody> {
  final ScrollController _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return _WorkbenchPane(
      title: widget.title,
      icon: Icons.visibility_outlined,
      child: Scrollbar(
        controller: _scrollController,
        thumbVisibility: true,
        child: SingleChildScrollView(
          controller: _scrollController,
          padding: const EdgeInsets.only(right: 12),
          child: Markdown(controller: widget.controller),
        ),
      ),
    );
  }
}

class _WorkbenchPane extends StatelessWidget {
  const _WorkbenchPane({
    required this.title,
    required this.icon,
    required this.child,
    this.trailing,
    this.footer,
  });

  final String title;
  final IconData icon;
  final Widget child;
  final Widget? trailing;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final trailing = this.trailing;
    final headerChildren = <Widget>[
      Icon(icon, size: 18, color: const Color(0xFF52616F)),
      const SizedBox(width: 8),
      Text(
        title,
        style: Theme.of(context).textTheme.titleSmall?.copyWith(
          fontWeight: FontWeight.w700,
          color: const Color(0xFF1B2733),
          letterSpacing: 0,
        ),
      ),
      const Spacer(),
    ];
    if (trailing != null) {
      headerChildren.add(trailing);
    }
    return Material(
      color: const Color(0xFFFFFFFF),
      shape: RoundedRectangleBorder(
        side: const BorderSide(color: Color(0xFFD8E0E8)),
        borderRadius: BorderRadius.circular(8),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          DecoratedBox(
            decoration: const BoxDecoration(
              color: Color(0xFFF9FAFC),
              border: Border(bottom: BorderSide(color: Color(0xFFE0E6ED))),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Row(children: headerChildren),
            ),
          ),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(14, 14, 14, 10),
              child: child,
            ),
          ),
          if (footer != null)
            DecoratedBox(
              decoration: const BoxDecoration(
                color: Color(0xFFFBFCFD),
                border: Border(top: BorderSide(color: Color(0xFFE4E9EF))),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 8,
                ),
                child: footer!,
              ),
            ),
        ],
      ),
    );
  }
}

class _PaneModeLabel extends StatelessWidget {
  const _PaneModeLabel({required this.editingMode});

  final FlarkMarkdownEditingMode editingMode;

  @override
  Widget build(BuildContext context) {
    final label = switch (editingMode) {
      FlarkMarkdownEditingMode.source => 'Source',
      FlarkMarkdownEditingMode.liveRendered => 'Live',
    };
    return Text(
      label,
      style: Theme.of(context).textTheme.labelMedium?.copyWith(
        color: const Color(0xFF52616F),
        fontWeight: FontWeight.w700,
        letterSpacing: 0,
      ),
    );
  }
}

class _EditorFooter extends StatelessWidget {
  const _EditorFooter({required this.controller});

  final FlarkFlutterController controller;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, child) {
        final selection = controller.selection;
        final selected = selection.end - selection.start;
        final caret = selection.extentOffset.clamp(
          0,
          controller.markdown.length,
        );
        return Row(
          children: [
            Text(
              'Caret $caret',
              style: Theme.of(context).textTheme.labelMedium?.copyWith(
                color: const Color(0xFF667380),
                letterSpacing: 0,
              ),
            ),
            if (selected > 0) ...[
              const SizedBox(width: 10),
              Text(
                '$selected selected',
                style: Theme.of(context).textTheme.labelMedium?.copyWith(
                  color: const Color(0xFF667380),
                  letterSpacing: 0,
                ),
              ),
            ],
            const Spacer(),
            TextButton.icon(
              onPressed: controller.runtime.canUndo ? controller.undo : null,
              icon: const Icon(Icons.undo, size: 18),
              label: const Text('Undo'),
              style: TextButton.styleFrom(
                visualDensity: VisualDensity.compact,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              ),
            ),
            const SizedBox(width: 4),
            TextButton.icon(
              onPressed: controller.runtime.canRedo ? controller.redo : null,
              icon: const Icon(Icons.redo, size: 18),
              label: const Text('Redo'),
              style: TextButton.styleFrom(
                visualDensity: VisualDensity.compact,
                tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              ),
            ),
          ],
        );
      },
    );
  }
}

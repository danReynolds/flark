import 'package:flutter/material.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

const _sampleMarkdown = '''# Sovereign mobile note

Native packaging checkpoint for Android and iOS.

- [x] Editable markdown surface
- [ ] Read-only preview surface
- [ ] Link and code actions

> A package consumer should import only the public barrel.

| Platform | Parser asset |
| --- | --- |
| Android | Bundled `.so` |
| iOS | Process-linked XCFramework |

```dart
final controller = SovereignController(
  markdownProfile: MarkdownSyntaxProfile.commonMarkGfm,
);
```

Read the [package README](https://example.com) before release.
''';

void main() {
  runApp(const SovereignExampleApp());
}

class SovereignExampleApp extends StatelessWidget {
  const SovereignExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Sovereign',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF2F6F73),
          brightness: Brightness.light,
        ),
        scaffoldBackgroundColor: const Color(0xFFF6F4EF),
      ),
      home: const SovereignExampleScreen(),
    );
  }
}

enum _WorkspaceMode { edit, split, preview }

class SovereignExampleScreen extends StatefulWidget {
  const SovereignExampleScreen({super.key});

  @override
  State<SovereignExampleScreen> createState() => _SovereignExampleScreenState();
}

class _SovereignExampleScreenState extends State<SovereignExampleScreen> {
  late final SovereignController _controller;
  late final NativeComrakBridgePreflightResult _nativePreflight;
  late final SyntaxEngine? _syntaxEngine;
  final FocusNode _focusNode = FocusNode();
  _WorkspaceMode _mode = _WorkspaceMode.split;

  @override
  void initState() {
    super.initState();
    _nativePreflight = preflightNativeComrakBridge();
    _syntaxEngine =
        _nativePreflight.isAvailable ? null : const _PlainTextSyntaxEngine();
    _controller = SovereignController(
      text: _sampleMarkdown,
      syntaxEngine: _syntaxEngine,
      markdownProfile: MarkdownSyntaxProfile.commonMarkGfm,
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
    final colors = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Sovereign'),
        centerTitle: false,
        actions: [
          Padding(
            padding: const EdgeInsetsDirectional.only(end: 12),
            child: _NativeStatusChip(result: _nativePreflight),
          ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            _CommandBar(
              controller: _controller,
              mode: _mode,
              onModeChanged: (mode) => setState(() => _mode = mode),
              onCommand: _runCommand,
            ),
            Divider(height: 1, color: colors.outlineVariant),
            Expanded(
              child: ValueListenableBuilder<TextEditingValue>(
                valueListenable: _controller,
                builder: (context, value, _) {
                  return LayoutBuilder(
                    builder: (context, constraints) {
                      return _Workspace(
                        controller: _controller,
                        focusNode: _focusNode,
                        markdown: value.text,
                        syntaxEngine: _syntaxEngine,
                        mode: _mode,
                        wide: constraints.maxWidth >= 860,
                        onOpenLink: _showLink,
                      );
                    },
                  );
                },
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _runCommand(
    SovereignCommandResult Function(SovereignMarkdownCommands commands) command,
  ) {
    final result = command(_controller.commands);
    _focusNode.requestFocus();
    if (!mounted) return;
    if (result is SovereignCommandRejected) {
      _showSnack(result.reason);
    }
  }

  Future<void> _showLink(String url) async {
    if (!mounted) return;
    _showSnack(url);
  }

  void _showSnack(String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }
}

class _CommandBar extends StatelessWidget {
  const _CommandBar({
    required this.controller,
    required this.mode,
    required this.onModeChanged,
    required this.onCommand,
  });

  final SovereignController controller;
  final _WorkspaceMode mode;
  final ValueChanged<_WorkspaceMode> onModeChanged;
  final void Function(
    SovereignCommandResult Function(SovereignMarkdownCommands commands) command,
  ) onCommand;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Theme.of(context).colorScheme.surface,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
        child: Row(
          children: [
            SegmentedButton<_WorkspaceMode>(
              showSelectedIcon: false,
              segments: const [
                ButtonSegment(
                  value: _WorkspaceMode.edit,
                  icon: Icon(Icons.edit_outlined),
                  label: Text('Edit'),
                ),
                ButtonSegment(
                  value: _WorkspaceMode.split,
                  icon: Icon(Icons.splitscreen_outlined),
                  label: Text('Split'),
                ),
                ButtonSegment(
                  value: _WorkspaceMode.preview,
                  icon: Icon(Icons.visibility_outlined),
                  label: Text('Preview'),
                ),
              ],
              selected: {mode},
              onSelectionChanged: (selection) =>
                  onModeChanged(selection.single),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: ValueListenableBuilder<TextEditingValue>(
                  valueListenable: controller,
                  builder: (context, value, child) {
                    final active = controller.commands
                        .capabilitiesAtSelection()
                        .activeInlineStyle;
                    return Row(
                      children: [
                        _ToolButton(
                          tooltip: 'Heading 1',
                          icon: const Text(
                            'H1',
                            style: TextStyle(fontWeight: FontWeight.w800),
                          ),
                          onPressed: () => onCommand(
                            (commands) => commands.setHeadingLevel(1),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Bold',
                          icon: const Icon(Icons.format_bold),
                          selected: active == SovereignInlineStyle.bold,
                          onPressed: () => onCommand(
                            (commands) => commands.toggleInlineStyle(
                              SovereignInlineStyle.bold,
                            ),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Italic',
                          icon: const Icon(Icons.format_italic),
                          selected: active == SovereignInlineStyle.italic,
                          onPressed: () => onCommand(
                            (commands) => commands.toggleInlineStyle(
                              SovereignInlineStyle.italic,
                            ),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Inline code',
                          icon: const Icon(Icons.code),
                          selected: active == SovereignInlineStyle.inlineCode,
                          onPressed: () => onCommand(
                            (commands) => commands.toggleInlineStyle(
                              SovereignInlineStyle.inlineCode,
                            ),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Quote',
                          icon: const Icon(Icons.format_quote),
                          onPressed: () => onCommand(
                            (commands) => commands.toggleQuote(),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Bullet list',
                          icon: const Icon(Icons.format_list_bulleted),
                          onPressed: () => onCommand(
                            (commands) => commands.toggleBulletList(),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Task list',
                          icon: const Icon(Icons.check_box_outlined),
                          onPressed: () => onCommand(
                            (commands) => commands.toggleTaskList(),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Code block',
                          icon: const Icon(Icons.data_object),
                          onPressed: () => onCommand(
                            (commands) => commands.insertFence(
                              language: 'dart',
                            ),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Link',
                          icon: const Icon(Icons.link),
                          onPressed: () => onCommand(
                            (commands) => commands.insertLink(),
                          ),
                        ),
                        _ToolButton(
                          tooltip: 'Horizontal rule',
                          icon: const Icon(Icons.horizontal_rule),
                          onPressed: () => onCommand(
                            (commands) => commands.insertHorizontalRule(),
                          ),
                        ),
                      ],
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ToolButton extends StatelessWidget {
  const _ToolButton({
    required this.tooltip,
    required this.icon,
    required this.onPressed,
    this.selected = false,
  });

  final String tooltip;
  final Widget icon;
  final VoidCallback onPressed;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsetsDirectional.only(end: 4),
      child: IconButton(
        tooltip: tooltip,
        isSelected: selected,
        selectedIcon: icon,
        icon: icon,
        onPressed: onPressed,
      ),
    );
  }
}

class _Workspace extends StatelessWidget {
  const _Workspace({
    required this.controller,
    required this.focusNode,
    required this.markdown,
    required this.syntaxEngine,
    required this.mode,
    required this.wide,
    required this.onOpenLink,
  });

  final SovereignController controller;
  final FocusNode focusNode;
  final String markdown;
  final SyntaxEngine? syntaxEngine;
  final _WorkspaceMode mode;
  final bool wide;
  final Future<void> Function(String url) onOpenLink;

  @override
  Widget build(BuildContext context) {
    final editor = _EditorPane(
      controller: controller,
      focusNode: focusNode,
      onOpenLink: onOpenLink,
    );
    final preview = _PreviewPane(
      markdown: markdown,
      syntaxEngine: syntaxEngine,
      onOpenLink: onOpenLink,
    );

    return switch (mode) {
      _WorkspaceMode.edit => editor,
      _WorkspaceMode.preview => preview,
      _WorkspaceMode.split when wide => Row(
          children: [
            Expanded(child: editor),
            const VerticalDivider(width: 1),
            Expanded(child: preview),
          ],
        ),
      _WorkspaceMode.split => Column(
          children: [
            Expanded(child: editor),
            const Divider(height: 1),
            Expanded(child: preview),
          ],
        ),
    };
  }
}

class _EditorPane extends StatelessWidget {
  const _EditorPane({
    required this.controller,
    required this.focusNode,
    required this.onOpenLink,
  });

  final SovereignController controller;
  final FocusNode focusNode;
  final Future<void> Function(String url) onOpenLink;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return DecoratedBox(
      decoration: BoxDecoration(color: colors.surface),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: SovereignEditor(
          controller: controller,
          focusNode: focusNode,
          autofocus: true,
          wrapText: true,
          cursorColor: colors.primary,
          textStyle: const TextStyle(fontSize: 16, height: 1.55),
          onOpenLink: onOpenLink,
        ),
      ),
    );
  }
}

class _PreviewPane extends StatelessWidget {
  const _PreviewPane({
    required this.markdown,
    required this.syntaxEngine,
    required this.onOpenLink,
  });

  final String markdown;
  final SyntaxEngine? syntaxEngine;
  final Future<void> Function(String url) onOpenLink;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return DecoratedBox(
      decoration: BoxDecoration(color: colors.surfaceContainerLowest),
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: SovereignMarkdownView(
          markdown: markdown,
          profile: MarkdownSyntaxProfile.commonMarkGfm,
          selectable: true,
          showLinkActionsOverlay: true,
          syntaxEngine: syntaxEngine,
          onOpenLink: onOpenLink,
        ),
      ),
    );
  }
}

class _NativeStatusChip extends StatelessWidget {
  const _NativeStatusChip({required this.result});

  final NativeComrakBridgePreflightResult result;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final available = result.isAvailable;
    final label = available ? 'Native parser' : 'Plain text';
    final tooltip = available
        ? 'Native bridge loaded'
        : result.error?.summary ?? 'Native bridge unavailable';
    return Tooltip(
      message: tooltip,
      child: Chip(
        avatar: Icon(
          available ? Icons.check_circle_outline : Icons.info_outline,
          size: 18,
          color: available ? colors.primary : colors.tertiary,
        ),
        label: Text(label),
        visualDensity: VisualDensity.compact,
        side: BorderSide(color: colors.outlineVariant),
      ),
    );
  }
}

class _PlainTextSyntaxEngine implements SyntaxEngine {
  const _PlainTextSyntaxEngine();

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    return SyntaxSnapshot(
      revision: request.revision,
      blocks: const [],
      inlineTokens: const [],
      markerRanges: const [],
      exclusionRanges: const [],
      ambiguityZones: const [],
      cursorMask: PassthroughCursorValidationMask(
        textLength: request.text.length,
      ),
      diagnostics: const [],
    );
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return SyntaxPrediction.empty(
      revision: request.revision,
      textLength: request.text.length,
    );
  }
}

import 'package:flutter/material.dart';
import 'package:flark/flark.dart';

const _sampleMarkdown = '''# Flark Markdown

Native Comrak parsing with live source-first editing.

- [x] Live-rendered Markdown editing
- [ ] Canonical source output
- [ ] App-owned command controls

> A package consumer should import only the public barrel.

```dart
final controller = FlarkFlutterController.fromMarkdown(markdown);
```
''';

const _tableMarkdown = '''# Tables

| Feature | Status |
| --- | --- |
| Live editor | Ready |
| Read-only render | Ready |
''';

const _articleMarkdown = '''# Release Notes

Flark keeps Markdown as the canonical source while exposing rendered block
widgets for common editing flows.

## Highlights

- Live-rendered list, quote, task, table, and code-fence editing
- Shared parser-backed rendering
- Comrak on native and web

```dart
final editor = MarkdownEditor(
  controller: controller,
  editingMode: FlarkMarkdownEditingMode.liveRendered,
);
```
''';

const _editorSnippet = '''MarkdownEditor(
  initialMarkdown: '# Notes\\n\\nEdit **Markdown**.',
  onChanged: saveMarkdown,
)''';

const _controllerSnippet =
    '''final controller = FlarkFlutterController.fromMarkdown(markdown);

Row(
  children: [
    Expanded(child: MarkdownEditor(controller: controller)),
    Expanded(child: Markdown(controller: controller)),
  ],
)''';

const _toolbarSnippet = '''final commands = controller.commands;

IconButton(
  icon: const Icon(Icons.format_bold),
  isSelected: commands.strongActive,
  onPressed: commands.canMutate ? commands.toggleStrong : null,
)''';

const _formSnippet = '''MarkdownEditorFormField(
  initialMarkdown: draftBody,
  validator: validateMarkdown,
  onSaved: saveDraftBody,
)''';

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

class FlarkExampleScreen extends StatefulWidget {
  const FlarkExampleScreen({super.key});

  @override
  State<FlarkExampleScreen> createState() => _FlarkExampleScreenState();
}

class _FlarkExampleScreenState extends State<FlarkExampleScreen> {
  late FlarkFlutterController _controller;
  final FocusNode _focusNode = FocusNode();

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
        child: LayoutBuilder(
          builder: (context, constraints) {
            final compact = constraints.maxWidth < 820;
            final horizontalPadding = compact ? 18.0 : 36.0;

            return SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _SiteHeader(horizontalPadding: horizontalPadding),
                  _HeroBand(
                    horizontalPadding: horizontalPadding,
                    controller: _controller,
                    focusNode: _focusNode,
                    onSample: () => _loadDocument(_sampleMarkdown),
                    onArticle: () => _loadDocument(_articleMarkdown),
                    onTables: () => _loadDocument(_tableMarkdown),
                    onScratch: () => _loadDocument(''),
                    onCommand: _runCommand,
                  ),
                  _FeatureBreakdown(horizontalPadding: horizontalPadding),
                  _ExamplesSection(horizontalPadding: horizontalPadding),
                  _DocsSection(horizontalPadding: horizontalPadding),
                  const _SiteFooter(),
                ],
              ),
            );
          },
        ),
      ),
    );
  }

  void _loadDocument(String markdown) {
    setState(() {
      _focusNode.unfocus();
      _controller.dispose();
      _controller = _createController(markdown);
    });
  }

  void _runCommand(_ToolbarCommand command) {
    final commands = _controller.commands;
    final result = switch (command) {
      _ToolbarCommand.heading1 => commands.setHeadingLevel(
        1,
        userEvent: 'example.toolbar.heading1',
      ),
      _ToolbarCommand.heading2 => commands.setHeadingLevel(
        2,
        userEvent: 'example.toolbar.heading2',
      ),
      _ToolbarCommand.bold => commands.toggleStrong(
        userEvent: 'example.toolbar.bold',
      ),
      _ToolbarCommand.italic => commands.toggleEmphasis(
        userEvent: 'example.toolbar.italic',
      ),
      _ToolbarCommand.quote => commands.toggleQuote(
        userEvent: 'example.toolbar.quote',
      ),
      _ToolbarCommand.bulletedList => commands.toggleBulletList(
        userEvent: 'example.toolbar.bulletList',
      ),
      _ToolbarCommand.orderedList => commands.toggleOrderedList(
        userEvent: 'example.toolbar.orderedList',
      ),
      _ToolbarCommand.taskList => commands.toggleTaskList(
        userEvent: 'example.toolbar.taskList',
      ),
      _ToolbarCommand.codeFence => commands.insertCodeFence(
        language: 'dart',
        userEvent: 'example.toolbar.codeFence',
      ),
      _ToolbarCommand.table => commands.insertTable(
        columns: 3,
        bodyRows: 2,
        userEvent: 'example.toolbar.table',
      ),
    };

    if (result.commandResult.isHandled) {
      _focusNode.requestFocus();
    }
  }
}

class _SiteHeader extends StatelessWidget {
  const _SiteHeader({required this.horizontalPadding});

  final double horizontalPadding;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: Color(0xFFFFFFFF),
        border: Border(bottom: BorderSide(color: Color(0xFFDDE5ED))),
      ),
      child: Padding(
        padding: EdgeInsets.symmetric(
          horizontal: horizontalPadding,
          vertical: 14,
        ),
        child: Wrap(
          spacing: 14,
          runSpacing: 10,
          crossAxisAlignment: WrapCrossAlignment.center,
          alignment: WrapAlignment.spaceBetween,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const _ProductMark(),
                const SizedBox(width: 10),
                Text(
                  'Flark',
                  style: Theme.of(context).textTheme.titleLarge?.copyWith(
                    color: const Color(0xFF14212B),
                    fontWeight: FontWeight.w800,
                    letterSpacing: 0,
                  ),
                ),
              ],
            ),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: const [
                _HeaderPill(icon: Icons.play_circle_outline, label: 'Demo'),
                _HeaderPill(icon: Icons.integration_instructions, label: 'API'),
                _HeaderPill(icon: Icons.menu_book_outlined, label: 'Docs'),
                _HeaderPill(icon: Icons.code, label: 'Examples'),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _HeaderPill extends StatelessWidget {
  const _HeaderPill({required this.icon, required this.label});

  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFF6F8FA),
        border: Border.all(color: const Color(0xFFDCE4EC)),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 16, color: const Color(0xFF37545B)),
            const SizedBox(width: 6),
            Text(
              label,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: const Color(0xFF23313D),
                fontWeight: FontWeight.w700,
                letterSpacing: 0,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _HeroBand extends StatelessWidget {
  const _HeroBand({
    required this.horizontalPadding,
    required this.controller,
    required this.focusNode,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final double horizontalPadding;
  final FlarkFlutterController controller;
  final FocusNode focusNode;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFFF7FAFB),
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          horizontalPadding,
          28,
          horizontalPadding,
          38,
        ),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1180),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const _HeroCopy(),
                const SizedBox(height: 20),
                _LiveDemoPanel(
                  controller: controller,
                  focusNode: focusNode,
                  onSample: onSample,
                  onArticle: onArticle,
                  onTables: onTables,
                  onScratch: onScratch,
                  onCommand: onCommand,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _HeroCopy extends StatelessWidget {
  const _HeroCopy();

  @override
  Widget build(BuildContext context) {
    final copy = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Flark Markdown Editor',
          style: Theme.of(context).textTheme.displaySmall?.copyWith(
            color: const Color(0xFF13212B),
            fontWeight: FontWeight.w900,
            letterSpacing: 0,
            height: 1.03,
          ),
        ),
        const SizedBox(height: 12),
        Text(
          'A source-first Markdown editing package for Flutter apps. Keep '
          'Markdown as the durable document, edit it through a live-rendered '
          'surface, and wire commands into your own UI.',
          style: Theme.of(context).textTheme.titleMedium?.copyWith(
            color: const Color(0xFF435564),
            letterSpacing: 0,
            height: 1.36,
            fontWeight: FontWeight.w500,
          ),
        ),
      ],
    );
    const proof = Wrap(
      spacing: 10,
      runSpacing: 10,
      children: [
        _ProofChip(icon: Icons.edit_note, label: 'Live-rendered editing'),
        _ProofChip(icon: Icons.storage_outlined, label: 'Markdown source'),
        _ProofChip(icon: Icons.construction_outlined, label: 'App-owned UI'),
        _ProofChip(icon: Icons.speed, label: 'Native Comrak'),
      ],
    );

    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth < 850) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [copy, const SizedBox(height: 16), proof],
          );
        }

        return Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Expanded(flex: 7, child: copy),
            const SizedBox(width: 34),
            const Flexible(flex: 4, child: proof),
          ],
        );
      },
    );
  }
}

class _ProofChip extends StatelessWidget {
  const _ProofChip({required this.icon, required this.label});

  final IconData icon;
  final String label;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFFFFFFF),
        border: Border.all(color: const Color(0xFFD9E3EC)),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 18, color: const Color(0xFF25636A)),
            const SizedBox(width: 8),
            Text(
              label,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: const Color(0xFF1E2C36),
                fontWeight: FontWeight.w700,
                letterSpacing: 0,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _LiveDemoPanel extends StatelessWidget {
  const _LiveDemoPanel({
    required this.controller,
    required this.focusNode,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final FlarkFlutterController controller;
  final FocusNode focusNode;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: const Color(0xFFFFFFFF),
      elevation: 0,
      shape: RoundedRectangleBorder(
        side: const BorderSide(color: Color(0xFFD1DEE8)),
        borderRadius: BorderRadius.circular(8),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _DemoToolbar(
            controller: controller,
            onSample: onSample,
            onArticle: onArticle,
            onTables: onTables,
            onScratch: onScratch,
            onCommand: onCommand,
          ),
          SizedBox(
            height: 470,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
              child: _EditorPane(
                controller: controller,
                focusNode: focusNode,
                editingMode: FlarkMarkdownEditingMode.liveRendered,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _FeatureBreakdown extends StatelessWidget {
  const _FeatureBreakdown({required this.horizontalPadding});

  final double horizontalPadding;

  @override
  Widget build(BuildContext context) {
    return _PageBand(
      horizontalPadding: horizontalPadding,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionHeading(
            icon: Icons.layers_outlined,
            title: 'Package shape',
            body:
                'The public API stays small: one app import, two core widgets, '
                'a form wrapper, and a controller when surfaces need shared state.',
          ),
          const SizedBox(height: 18),
          LayoutBuilder(
            builder: (context, constraints) {
              final columns = constraints.maxWidth < 760 ? 1 : 3;
              return _ResponsiveGrid(
                columns: columns,
                children: const [
                  _FeatureCard(
                    icon: Icons.edit_note,
                    title: 'Live-rendered editing',
                    body:
                        'Write in a rendered Markdown surface while Flark keeps '
                        'the original source string as the durable document.',
                    accent: Color(0xFF25636A),
                  ),
                  _FeatureCard(
                    icon: Icons.visibility_outlined,
                    title: 'Preview from the same state',
                    body:
                        'Share one controller between editor, read-only preview, '
                        'toolbar, save button, and parser lifecycle.',
                    accent: Color(0xFF3E6E9E),
                  ),
                  _FeatureCard(
                    icon: Icons.construction_outlined,
                    title: 'UI-agnostic commands',
                    body:
                        'Build your own toolbar with concise active-state reads '
                        'and mutation verbs under controller.commands.',
                    accent: Color(0xFF7A5D2E),
                  ),
                  _FeatureCard(
                    icon: Icons.fact_check_outlined,
                    title: 'Flutter forms',
                    body:
                        'Use MarkdownEditorFormField for validation, save, reset, '
                        'autovalidation, and restoration flows.',
                    accent: Color(0xFF526D3B),
                  ),
                  _FeatureCard(
                    icon: Icons.data_object,
                    title: 'Native CommonMark/GFM',
                    body:
                        'Comrak-backed parsing powers block plans, inline style '
                        'ranges, links, tables, tasks, quotes, and code fences.',
                    accent: Color(0xFF84575B),
                  ),
                  _FeatureCard(
                    icon: Icons.memory,
                    title: 'Headless core',
                    body:
                        'Transactions, commands, projection, history, and render '
                        'plans live outside Flutter widgets for deeper integrations.',
                    accent: Color(0xFF5B6475),
                  ),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

class _ExamplesSection extends StatelessWidget {
  const _ExamplesSection({required this.horizontalPadding});

  final double horizontalPadding;

  @override
  Widget build(BuildContext context) {
    return _PageBand(
      horizontalPadding: horizontalPadding,
      color: const Color(0xFFFFFFFF),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionHeading(
            icon: Icons.code,
            title: 'Copy-paste API examples',
            body:
                'The same public barrel powers the live demo and the snippets '
                'below. Most apps should only import package:flark/flark.dart.',
          ),
          const SizedBox(height: 18),
          LayoutBuilder(
            builder: (context, constraints) {
              final columns = constraints.maxWidth < 900 ? 1 : 2;
              return _ResponsiveGrid(
                columns: columns,
                children: const [
                  _CodeCard(title: 'Basic field', code: _editorSnippet),
                  _CodeCard(title: 'Shared preview', code: _controllerSnippet),
                  _CodeCard(title: 'Toolbar command', code: _toolbarSnippet),
                  _CodeCard(title: 'Form field', code: _formSnippet),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

class _DocsSection extends StatelessWidget {
  const _DocsSection({required this.horizontalPadding});

  final double horizontalPadding;

  @override
  Widget build(BuildContext context) {
    return _PageBand(
      horizontalPadding: horizontalPadding,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionHeading(
            icon: Icons.menu_book_outlined,
            title: 'Docs map',
            body:
                'Start with the cookbook when building an app. Use the API '
                'surface guide to choose the right import tier.',
          ),
          const SizedBox(height: 18),
          LayoutBuilder(
            builder: (context, constraints) {
              final columns = constraints.maxWidth < 760 ? 1 : 3;
              return _ResponsiveGrid(
                columns: columns,
                children: const [
                  _DocCard(
                    icon: Icons.rocket_launch_outlined,
                    title: 'Getting started',
                    path: 'doc/getting_started.md',
                    body: 'Build an editor, preview Markdown, and share state.',
                  ),
                  _DocCard(
                    icon: Icons.receipt_long,
                    title: 'Cookbook',
                    path: 'doc/cookbook.md',
                    body:
                        'Toolbar, form, dirty-save, link, switching, and preview recipes.',
                  ),
                  _DocCard(
                    icon: Icons.account_tree_outlined,
                    title: 'API surface',
                    path: 'doc/api_surface.md',
                    body:
                        'Pick the app, core, or advanced import deliberately.',
                  ),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

class _PageBand extends StatelessWidget {
  const _PageBand({
    required this.horizontalPadding,
    required this.child,
    this.color = const Color(0xFFF5F7FA),
  });

  final double horizontalPadding;
  final Widget child;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: color,
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          horizontalPadding,
          38,
          horizontalPadding,
          42,
        ),
        child: child,
      ),
    );
  }
}

class _SectionHeading extends StatelessWidget {
  const _SectionHeading({
    required this.icon,
    required this.title,
    required this.body,
  });

  final IconData icon;
  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 920),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          DecoratedBox(
            decoration: BoxDecoration(
              color: const Color(0xFFEAF2F3),
              border: Border.all(color: const Color(0xFFCDE0E2)),
              borderRadius: const BorderRadius.all(Radius.circular(8)),
            ),
            child: Padding(
              padding: const EdgeInsets.all(10),
              child: Icon(icon, size: 22, color: const Color(0xFF25636A)),
            ),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                    color: const Color(0xFF14212B),
                    fontWeight: FontWeight.w800,
                    letterSpacing: 0,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  body,
                  style: Theme.of(context).textTheme.bodyLarge?.copyWith(
                    color: const Color(0xFF52616F),
                    height: 1.42,
                    letterSpacing: 0,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _ResponsiveGrid extends StatelessWidget {
  const _ResponsiveGrid({required this.columns, required this.children});

  final int columns;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        const gap = 14.0;
        final itemWidth =
            (constraints.maxWidth - gap * (columns - 1)) / columns;
        return Wrap(
          spacing: gap,
          runSpacing: gap,
          children: [
            for (final child in children)
              SizedBox(width: itemWidth, child: child),
          ],
        );
      },
    );
  }
}

class _FeatureCard extends StatelessWidget {
  const _FeatureCard({
    required this.icon,
    required this.title,
    required this.body,
    required this.accent,
  });

  final IconData icon;
  final String title;
  final String body;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: const Color(0xFFFFFFFF),
      shape: RoundedRectangleBorder(
        side: const BorderSide(color: Color(0xFFDDE5ED)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, color: accent, size: 26),
            const SizedBox(height: 14),
            Text(
              title,
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                color: const Color(0xFF182530),
                fontWeight: FontWeight.w800,
                letterSpacing: 0,
              ),
            ),
            const SizedBox(height: 8),
            Text(
              body,
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                color: const Color(0xFF566675),
                height: 1.42,
                letterSpacing: 0,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _CodeCard extends StatelessWidget {
  const _CodeCard({required this.title, required this.code});

  final String title;
  final String code;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: const Color(0xFF17212B),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          DecoratedBox(
            decoration: const BoxDecoration(
              color: Color(0xFF202D38),
              border: Border(bottom: BorderSide(color: Color(0xFF324353))),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              child: Text(
                title,
                style: Theme.of(context).textTheme.labelLarge?.copyWith(
                  color: const Color(0xFFEAF1F5),
                  fontWeight: FontWeight.w800,
                  letterSpacing: 0,
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(14),
            child: Text(
              code,
              style: const TextStyle(
                color: Color(0xFFF4F7F9),
                fontFamily: 'monospace',
                fontSize: 13,
                height: 1.45,
                letterSpacing: 0,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _DocCard extends StatelessWidget {
  const _DocCard({
    required this.icon,
    required this.title,
    required this.path,
    required this.body,
  });

  final IconData icon;
  final String title;
  final String path;
  final String body;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: const Color(0xFFFFFFFF),
      shape: RoundedRectangleBorder(
        side: const BorderSide(color: Color(0xFFDDE5ED)),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, color: const Color(0xFF25636A), size: 24),
            const SizedBox(height: 12),
            Text(
              title,
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                color: const Color(0xFF182530),
                fontWeight: FontWeight.w800,
                letterSpacing: 0,
              ),
            ),
            const SizedBox(height: 6),
            Text(
              path,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: const Color(0xFF25636A),
                fontWeight: FontWeight.w700,
                letterSpacing: 0,
              ),
            ),
            const SizedBox(height: 8),
            Text(
              body,
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                color: const Color(0xFF566675),
                height: 1.42,
                letterSpacing: 0,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SiteFooter extends StatelessWidget {
  const _SiteFooter();

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFF14212B),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 36, vertical: 24),
        child: Wrap(
          spacing: 14,
          runSpacing: 10,
          crossAxisAlignment: WrapCrossAlignment.center,
          alignment: WrapAlignment.spaceBetween,
          children: [
            Text(
              'Flark Markdown editor for Flutter',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                color: const Color(0xFFF3F7F9),
                fontWeight: FontWeight.w800,
                letterSpacing: 0,
              ),
            ),
            Text(
              'Import package:flark/flark.dart',
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                color: const Color(0xFFB8C5CE),
                letterSpacing: 0,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _DemoToolbar extends StatelessWidget {
  const _DemoToolbar({
    required this.controller,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final FlarkFlutterController controller;
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
            final header = _DemoToolbarHeader(controller: controller);
            final actions = _ToolbarActions(
              controller: controller,
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

class _DemoToolbarHeader extends StatelessWidget {
  const _DemoToolbarHeader({required this.controller});

  final FlarkFlutterController controller;

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
        const _StatusChip(label: 'Live playground'),
        _ControllerStats(controller: controller),
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
    required this.controller,
    required this.compact,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final FlarkFlutterController controller;
  final bool compact;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, child) {
        final commands = controller.commands;
        final canMutate = commands.canMutate;
        final hasRange = !controller.selection.isCollapsed;
        final canStyleSelection = canMutate && hasRange;
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
            selected: commands.headingLevel == 1,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.heading1),
          ),
          _CommandButton(
            tooltip: 'Heading 2',
            icon: Icons.looks_two_outlined,
            selected: commands.headingLevel == 2,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.heading2),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-bold'),
            tooltip: hasRange ? 'Bold' : 'Select text to bold',
            icon: Icons.format_bold,
            selected: commands.strongActive,
            enabled: canStyleSelection,
            onPressed: () => onCommand(_ToolbarCommand.bold),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-italic'),
            tooltip: hasRange ? 'Italic' : 'Select text to italicize',
            icon: Icons.format_italic,
            selected: commands.emphasisActive,
            enabled: canStyleSelection,
            onPressed: () => onCommand(_ToolbarCommand.italic),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-quote'),
            tooltip: 'Quote',
            icon: Icons.format_quote,
            selected: commands.quoteActive,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.quote),
          ),
          _CommandButton(
            tooltip: 'Bulleted list',
            icon: Icons.format_list_bulleted,
            selected: commands.bulletListActive,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.bulletedList),
          ),
          _CommandButton(
            tooltip: 'Numbered list',
            icon: Icons.format_list_numbered,
            selected: commands.orderedListActive,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.orderedList),
          ),
          _CommandButton(
            tooltip: 'Task list',
            icon: Icons.checklist,
            selected: commands.taskListActive,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.taskList),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-code-fence'),
            tooltip: 'Code fence',
            icon: Icons.code,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.codeFence),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-table'),
            tooltip: 'Table',
            icon: Icons.table_rows_outlined,
            selected: commands.tableActive,
            enabled: canMutate,
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
      },
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

class _CommandButton extends StatefulWidget {
  const _CommandButton({
    this.buttonKey,
    required this.tooltip,
    required this.icon,
    required this.onPressed,
    this.enabled = true,
    this.selected = false,
  });

  final Key? buttonKey;
  final String tooltip;
  final IconData icon;
  final VoidCallback onPressed;
  final bool enabled;
  final bool selected;

  @override
  State<_CommandButton> createState() => _CommandButtonState();
}

class _CommandButtonState extends State<_CommandButton> {
  late final FocusNode _focusNode = FocusNode(
    debugLabel: widget.tooltip,
    canRequestFocus: false,
    skipTraversal: true,
  );

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: widget.tooltip,
      waitDuration: const Duration(milliseconds: 450),
      child: IconButton.outlined(
        key: widget.buttonKey,
        focusNode: _focusNode,
        isSelected: widget.selected,
        selectedIcon: Icon(widget.icon, size: 20),
        onPressed: widget.enabled ? widget.onPressed : null,
        icon: Icon(widget.icon, size: 20),
        style: IconButton.styleFrom(
          foregroundColor: const Color(0xFF25313C),
          disabledForegroundColor: const Color(0xFF8D99A5),
          backgroundColor: widget.selected ? const Color(0xFFE3F0F2) : null,
          disabledBackgroundColor: const Color(0xFFF4F6F8),
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
      title: 'Live Markdown field',
      icon: Icons.edit_note,
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

class _WorkbenchPane extends StatelessWidget {
  const _WorkbenchPane({
    required this.title,
    required this.icon,
    required this.child,
    this.footer,
  });

  final String title;
  final IconData icon;
  final Widget child;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
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

import 'package:flutter/material.dart';
import 'package:flark/flark.dart';

const _heroArtworkAsset = 'assets/flark_atelier.jpg';

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
      seedColor: const Color(0xFF0D7772),
      brightness: Brightness.light,
    );
    return MaterialApp(
      title: 'Flark',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: scheme,
        scaffoldBackgroundColor: const Color(0xFFF6F7F4),
        dividerColor: const Color(0xFFD8DDD6),
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
  FocusNode _focusNode = FocusNode();
  final List<FocusNode> _retiredFocusNodes = [];

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
    for (final focusNode in _retiredFocusNodes) {
      focusNode.dispose();
    }
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
    final oldController = _controller;
    final oldFocusNode = _focusNode;
    oldFocusNode.unfocus();
    setState(() {
      _controller = _createController(markdown);
      _focusNode = FocusNode();
      _retiredFocusNodes.add(oldFocusNode);
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      oldController.dispose();
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
        color: Color(0xFFFBFCF8),
        border: Border(bottom: BorderSide(color: Color(0xFFE0E5DD))),
      ),
      child: Padding(
        padding: EdgeInsets.symmetric(
          horizontal: horizontalPadding,
          vertical: 13,
        ),
        child: LayoutBuilder(
          builder: (context, constraints) {
            final compact = constraints.maxWidth < 560;
            final brand = Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const _ProductMark(),
                const SizedBox(width: 10),
                Text(
                  'Flark',
                  style: Theme.of(context).textTheme.titleLarge?.copyWith(
                    color: const Color(0xFF171B18),
                    fontWeight: FontWeight.w900,
                    letterSpacing: 0,
                  ),
                ),
              ],
            );

            if (compact) {
              return brand;
            }

            return Row(
              children: [
                brand,
                const Spacer(),
                Text(
                  'Live Markdown playground',
                  style: Theme.of(context).textTheme.labelLarge?.copyWith(
                    color: const Color(0xFF58635C),
                    fontWeight: FontWeight.w800,
                    letterSpacing: 0,
                  ),
                ),
              ],
            );
          },
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
    return DecoratedBox(
      decoration: const BoxDecoration(color: Color(0xFFF6F7F4)),
      child: Stack(
        children: [
          Positioned.fill(child: _PlaygroundBackdrop()),
          Padding(
            padding: EdgeInsets.fromLTRB(
              horizontalPadding,
              28,
              horizontalPadding,
              46,
            ),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 1240),
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final compact = constraints.maxWidth < 900;
                    final demo = _LiveDemoPanel(
                      controller: controller,
                      focusNode: focusNode,
                      editorHeight: compact ? 520 : 600,
                      onSample: onSample,
                      onArticle: onArticle,
                      onTables: onTables,
                      onScratch: onScratch,
                      onCommand: onCommand,
                    );

                    if (compact) {
                      return Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const _HeroCopy(),
                          const SizedBox(height: 18),
                          demo,
                        ],
                      );
                    }

                    return Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        const _HeroCopy(),
                        const SizedBox(height: 22),
                        demo,
                      ],
                    );
                  },
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _PlaygroundBackdrop extends StatelessWidget {
  const _PlaygroundBackdrop();

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        const DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [Color(0xFFFBFCF8), Color(0xFFF1F4F0)],
            ),
          ),
        ),
        Positioned(
          right: -120,
          top: -90,
          width: 520,
          height: 320,
          child: Opacity(
            opacity: 0.14,
            child: Image.asset(
              _heroArtworkAsset,
              fit: BoxFit.cover,
              errorBuilder: (context, error, stackTrace) {
                return const SizedBox.shrink();
              },
            ),
          ),
        ),
        const Positioned(
          left: -80,
          bottom: -80,
          child: _SoftAccentDisc(color: Color(0x1A0D7772), size: 260),
        ),
        const Positioned(
          right: 160,
          bottom: 80,
          child: _SoftAccentDisc(color: Color(0x16E26F4A), size: 160),
        ),
      ],
    );
  }
}

class _SoftAccentDisc extends StatelessWidget {
  const _SoftAccentDisc({required this.color, required this.size});

  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(shape: BoxShape.circle, color: color),
      child: SizedBox.square(dimension: size),
    );
  }
}

class _HeroCopy extends StatelessWidget {
  const _HeroCopy();

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 820;
        final copy = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const _HeroEyebrow(),
            const SizedBox(height: 12),
            Text(
              'Flark Markdown Editor',
              style: Theme.of(context).textTheme.displaySmall?.copyWith(
                color: const Color(0xFF151A17),
                fontWeight: FontWeight.w900,
                letterSpacing: 0,
                height: 1.02,
              ),
            ),
            const SizedBox(height: 10),
            Text(
              'Try source-first Markdown editing live in Flutter.',
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                color: const Color(0xFF59655E),
                letterSpacing: 0,
                height: 1.35,
                fontWeight: FontWeight.w600,
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
            _ProofChip(icon: Icons.speed, label: 'Native Comrak'),
          ],
        );

        if (compact) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [copy, const SizedBox(height: 14), proof],
          );
        }

        return Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Expanded(child: copy),
            const SizedBox(width: 28),
            const Flexible(child: proof),
          ],
        );
      },
    );
  }
}

class _HeroEyebrow extends StatelessWidget {
  const _HeroEyebrow();

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFFFFFFF),
        border: Border.all(color: const Color(0xFFDDE5DF)),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Text(
          'Flutter package / live document field',
          style: Theme.of(context).textTheme.labelLarge?.copyWith(
            color: const Color(0xFF0B6F69),
            fontWeight: FontWeight.w900,
            letterSpacing: 0,
          ),
        ),
      ),
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
        border: Border.all(color: const Color(0xFFDDE5DF)),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 18, color: const Color(0xFF0B6F69)),
            const SizedBox(width: 8),
            Text(
              label,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: const Color(0xFF202823),
                fontWeight: FontWeight.w800,
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
    required this.editorHeight,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
  });

  final FlarkFlutterController controller;
  final FocusNode focusNode;
  final double editorHeight;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(8),
        boxShadow: const [
          BoxShadow(
            color: Color(0x1F1E2A24),
            blurRadius: 42,
            offset: Offset(0, 18),
          ),
          BoxShadow(
            color: Color(0x120B6F69),
            blurRadius: 18,
            offset: Offset(0, 4),
          ),
        ],
      ),
      child: Material(
        color: const Color(0xFFFFFFFF),
        elevation: 0,
        shape: RoundedRectangleBorder(
          side: const BorderSide(color: Color(0xFFC8D5CF), width: 1.2),
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
              height: editorHeight,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(12, 12, 12, 12),
                child: _EditorPane(
                  controller: controller,
                  focusNode: focusNode,
                  editingMode: FlarkMarkdownEditingMode.liveRendered,
                ),
              ),
            ),
          ],
        ),
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
      color: const Color(0xFFFFFFFF),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionHeading(
            eyebrow: 'The point',
            title: 'A playground first. A package second.',
            body:
                'Start by testing the live editor. Then use the same small API '
                'surface to bring that behavior into your app.',
          ),
          const SizedBox(height: 28),
          const _PrincipleStrip(),
        ],
      ),
    );
  }
}

class _PrincipleStrip extends StatelessWidget {
  const _PrincipleStrip();

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 860;
        final children = const [
          _PrincipleTile(
            number: '01',
            title: 'Source first',
            body:
                'Markdown is the document. The rendered field is an editing surface, not a separate truth.',
            accent: Color(0xFF0D7772),
          ),
          _PrincipleTile(
            number: '02',
            title: 'UI stays app-owned',
            body:
                'Commands expose state and mutations; your app decides what the toolbar, menu, or shortcut layer looks like.',
            accent: Color(0xFFE26F4A),
          ),
          _PrincipleTile(
            number: '03',
            title: 'Parser-backed detail',
            body:
                'Comrak-backed plans keep lists, quotes, tables, tasks, and code fences grounded in real Markdown.',
            accent: Color(0xFFDCA944),
          ),
        ];

        if (compact) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final child in children) ...[
                child,
                if (child != children.last) const SizedBox(height: 12),
              ],
            ],
          );
        }

        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final child in children) ...[
              Expanded(child: child),
              if (child != children.last) const SizedBox(width: 16),
            ],
          ],
        );
      },
    );
  }
}

class _PrincipleTile extends StatelessWidget {
  const _PrincipleTile({
    required this.number,
    required this.title,
    required this.body,
    required this.accent,
  });

  final String number;
  final String title;
  final String body;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        border: Border(top: BorderSide(color: Color(0xFF2D3029), width: 2)),
      ),
      child: Padding(
        padding: const EdgeInsets.only(top: 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              number,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: accent,
                fontWeight: FontWeight.w900,
                letterSpacing: 0,
              ),
            ),
            const SizedBox(height: 10),
            Text(
              title,
              style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                color: const Color(0xFF1E201A),
                fontWeight: FontWeight.w900,
                letterSpacing: 0,
                height: 1.05,
              ),
            ),
            const SizedBox(height: 10),
            Text(
              body,
              style: Theme.of(context).textTheme.bodyLarge?.copyWith(
                color: const Color(0xFF5E5648),
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

class _ExamplesSection extends StatelessWidget {
  const _ExamplesSection({required this.horizontalPadding});

  final double horizontalPadding;

  @override
  Widget build(BuildContext context) {
    return _PageBand(
      horizontalPadding: horizontalPadding,
      color: const Color(0xFFF7FAF8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionHeading(
            eyebrow: 'API shape',
            title: 'Small surface, expressive integrations.',
            body:
                'The same public barrel powers the playground. Start with a field, share a controller when needed, and wire commands into your own UI.',
          ),
          const SizedBox(height: 26),
          const _CodeGallery(),
        ],
      ),
    );
  }
}

class _CodeGallery extends StatelessWidget {
  const _CodeGallery();

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 900;
        final code = const _CodePanel(
          title: 'Basic field',
          code: _editorSnippet,
        );
        final recipes = const _RecipeRail();

        if (compact) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [code, const SizedBox(height: 14), recipes],
          );
        }

        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(flex: 7, child: code),
            const SizedBox(width: 18),
            Expanded(flex: 5, child: recipes),
          ],
        );
      },
    );
  }
}

class _RecipeRail extends StatelessWidget {
  const _RecipeRail();

  @override
  Widget build(BuildContext context) {
    return const Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _RecipeLine(title: 'Shared preview', code: _controllerSnippet),
        _RecipeLine(title: 'Toolbar command', code: _toolbarSnippet),
        _RecipeLine(title: 'Form field', code: _formSnippet),
      ],
    );
  }
}

class _RecipeLine extends StatelessWidget {
  const _RecipeLine({required this.title, required this.code});

  final String title;
  final String code;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: const Color(0xFFFFFFFF),
          border: Border.all(color: const Color(0xFFDDE5DF)),
          borderRadius: const BorderRadius.all(Radius.circular(8)),
        ),
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: Theme.of(context).textTheme.labelLarge?.copyWith(
                  color: const Color(0xFF0B6F69),
                  fontWeight: FontWeight.w900,
                  letterSpacing: 0,
                ),
              ),
              const SizedBox(height: 10),
              Text(
                code,
                style: const TextStyle(
                  color: Color(0xFF202823),
                  fontFamily: 'monospace',
                  fontSize: 12.5,
                  height: 1.42,
                  letterSpacing: 0,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _CodePanel extends StatelessWidget {
  const _CodePanel({required this.title, required this.code});

  final String title;
  final String code;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFFFFFFF),
        border: Border.all(color: const Color(0xFFC8D5CF), width: 1.2),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Icon(Icons.code, color: Color(0xFF0D7772), size: 20),
                const SizedBox(width: 8),
                Text(
                  title,
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    color: const Color(0xFF1D1F19),
                    fontWeight: FontWeight.w900,
                    letterSpacing: 0,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16),
            Text(
              code,
              style: const TextStyle(
                color: Color(0xFF1B1D19),
                fontFamily: 'monospace',
                fontSize: 14,
                height: 1.48,
                letterSpacing: 0,
              ),
            ),
          ],
        ),
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
      color: const Color(0xFFFFFFFF),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionHeading(
            eyebrow: 'Build path',
            title: 'A package page that keeps moving toward the code.',
            body:
                'The docs route developers from a first field to cookbook integrations and then into the API surface.',
          ),
          const SizedBox(height: 24),
          const _DocPath(),
        ],
      ),
    );
  }
}

class _DocPath extends StatelessWidget {
  const _DocPath();

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 760;
        final children = const [
          _DocStep(
            icon: Icons.rocket_launch_outlined,
            title: 'Getting started',
            path: 'doc/getting_started.md',
            body: 'Build an editor, preview Markdown, and share state.',
          ),
          _DocStep(
            icon: Icons.receipt_long,
            title: 'Cookbook',
            path: 'doc/cookbook.md',
            body:
                'Toolbar, form, dirty-save, link, switching, and preview recipes.',
          ),
          _DocStep(
            icon: Icons.account_tree_outlined,
            title: 'API surface',
            path: 'doc/api_surface.md',
            body: 'Pick the app, core, or advanced import deliberately.',
          ),
        ];

        if (compact) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final child in children) ...[
                child,
                if (child != children.last) const SizedBox(height: 12),
              ],
            ],
          );
        }

        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final child in children) ...[
              Expanded(child: child),
              if (child != children.last) const SizedBox(width: 16),
            ],
          ],
        );
      },
    );
  }
}

class _DocStep extends StatelessWidget {
  const _DocStep({
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
    return DecoratedBox(
      decoration: const BoxDecoration(
        border: Border(left: BorderSide(color: Color(0xFF0D7772), width: 3)),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(14, 0, 0, 0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, color: const Color(0xFF0D7772), size: 24),
            const SizedBox(height: 10),
            Text(
              title,
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                color: const Color(0xFF1D1F19),
                fontWeight: FontWeight.w900,
                letterSpacing: 0,
              ),
            ),
            const SizedBox(height: 6),
            Text(
              path,
              style: Theme.of(context).textTheme.labelLarge?.copyWith(
                color: const Color(0xFF9D4C32),
                fontWeight: FontWeight.w800,
                letterSpacing: 0,
              ),
            ),
            const SizedBox(height: 8),
            Text(
              body,
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                color: const Color(0xFF5E5648),
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

class _PageBand extends StatelessWidget {
  const _PageBand({
    required this.horizontalPadding,
    required this.child,
    required this.color,
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
          56,
          horizontalPadding,
          62,
        ),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1180),
            child: child,
          ),
        ),
      ),
    );
  }
}

class _SectionHeading extends StatelessWidget {
  const _SectionHeading({
    required this.eyebrow,
    required this.title,
    required this.body,
  });

  final String eyebrow;
  final String title;
  final String body;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 820),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            eyebrow,
            style: Theme.of(context).textTheme.labelLarge?.copyWith(
              color: const Color(0xFF0B6F69),
              fontWeight: FontWeight.w900,
              letterSpacing: 0,
            ),
          ),
          const SizedBox(height: 10),
          Text(
            title,
            style: Theme.of(context).textTheme.headlineMedium?.copyWith(
              color: const Color(0xFF151A17),
              fontWeight: FontWeight.w900,
              letterSpacing: 0,
              height: 1.08,
            ),
          ),
          const SizedBox(height: 12),
          Text(
            body,
            style: Theme.of(context).textTheme.bodyLarge?.copyWith(
              color: const Color(0xFF59655E),
              height: 1.45,
              letterSpacing: 0,
            ),
          ),
        ],
      ),
    );
  }
}

class _SiteFooter extends StatelessWidget {
  const _SiteFooter();

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFFF7FAF8),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 36, vertical: 26),
        child: Wrap(
          spacing: 14,
          runSpacing: 10,
          crossAxisAlignment: WrapCrossAlignment.center,
          alignment: WrapAlignment.spaceBetween,
          children: [
            Text(
              'Flark Markdown editor for Flutter',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                color: const Color(0xFF151A17),
                fontWeight: FontWeight.w900,
                letterSpacing: 0,
              ),
            ),
            Text(
              'Import package:flark/flark.dart',
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                color: const Color(0xFF59655E),
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
        color: Color(0xFFFBFCF8),
        border: Border(bottom: BorderSide(color: Color(0xFFDDE5DF))),
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
                    color: const Color(0xFF151A17),
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
        color: const Color(0xFF0B6F69),
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
            buttonKey: const ValueKey('flark-example-command-heading1'),
            tooltip: 'Heading 1',
            icon: Icons.looks_one_outlined,
            selected: commands.headingLevel == 1,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.heading1),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-heading2'),
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
            buttonKey: const ValueKey('flark-example-command-bulleted-list'),
            tooltip: 'Bulleted list',
            icon: Icons.format_list_bulleted,
            selected: commands.bulletListActive,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.bulletedList),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-ordered-list'),
            tooltip: 'Numbered list',
            icon: Icons.format_list_numbered,
            selected: commands.orderedListActive,
            enabled: canMutate,
            onPressed: () => onCommand(_ToolbarCommand.orderedList),
          ),
          _CommandButton(
            buttonKey: const ValueKey('flark-example-command-task-list'),
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
        color: const Color(0xFFEAF6F4),
        border: Border.all(color: const Color(0xFFCDE7E3)),
        borderRadius: const BorderRadius.all(Radius.circular(8)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        child: Text(
          label,
          style: Theme.of(context).textTheme.labelMedium?.copyWith(
            color: const Color(0xFF0B6F69),
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
            color: const Color(0xFF68736C),
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
          foregroundColor: const Color(0xFF202823),
          disabledForegroundColor: const Color(0xFF9AA39E),
          backgroundColor: widget.selected ? const Color(0xFFEAF6F4) : null,
          disabledBackgroundColor: const Color(0xFFF2F5F1),
          side: const BorderSide(color: Color(0xFFDDE5DF)),
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
          color: Color(0xFF151A17),
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
      Icon(icon, size: 18, color: const Color(0xFF68736C)),
      const SizedBox(width: 8),
      Text(
        title,
        style: Theme.of(context).textTheme.titleSmall?.copyWith(
          fontWeight: FontWeight.w700,
          color: const Color(0xFF151A17),
          letterSpacing: 0,
        ),
      ),
      const Spacer(),
    ];
    return Material(
      color: const Color(0xFFFFFFFF),
      shape: RoundedRectangleBorder(
        side: const BorderSide(color: Color(0xFFDDE5DF)),
        borderRadius: BorderRadius.circular(8),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          DecoratedBox(
            decoration: const BoxDecoration(
              color: Color(0xFFFBFCF8),
              border: Border(bottom: BorderSide(color: Color(0xFFE4EAE5))),
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
                color: Color(0xFFFBFCF8),
                border: Border(top: BorderSide(color: Color(0xFFE4EAE5))),
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
                color: const Color(0xFF68736C),
                letterSpacing: 0,
              ),
            ),
            if (selected > 0) ...[
              const SizedBox(width: 10),
              Text(
                '$selected selected',
                style: Theme.of(context).textTheme.labelMedium?.copyWith(
                  color: const Color(0xFF68736C),
                  letterSpacing: 0,
                ),
              ),
            ],
            const Spacer(),
            TextButton.icon(
              key: const ValueKey('flark-example-undo'),
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
              key: const ValueKey('flark-example-redo'),
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

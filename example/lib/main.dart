import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flark/flark.dart';
import 'package:url_launcher/url_launcher.dart';

const _heroArtworkAsset = 'assets/flark_atelier.jpg';

const _repoUrl = 'https://github.com/danReynolds/flark';
const _pubUrl = 'https://pub.dev/packages/flark';
const _docBase = 'https://github.com/danReynolds/flark/blob/main/doc/';

// ---------------------------------------------------------------------------
// Design language
// ---------------------------------------------------------------------------
// Flark's site reads like a writing studio: warm paper, ink, and the painterly
// teal / terracotta / ochre pulled from the hero artwork. Fraunces carries the
// editorial voice, Inter the interface, JetBrains Mono the code.

const _fontSerif = 'Fraunces';
const _fontSans = 'Inter';
const _fontMono = 'JetBrains Mono';

class _C {
  static const paper = Color(0xFFFBF6EC); // warm cream base
  static const paperDeep = Color(0xFFF3E9D8); // deeper paper band
  static const card = Color(0xFFFFFDF8); // raised surface
  static const ink = Color(0xFF221E16); // primary text
  static const inkSoft = Color(0xFF5B5347); // secondary text
  static const inkFaint = Color(0xFF8C8473); // tertiary / captions
  static const line = Color(0xFFE7DCC6); // hairline border
  static const lineSoft = Color(0xFFF0E7D6);
  static const teal = Color(0xFF0E6E66); // primary accent
  static const tealDeep = Color(0xFF0A4C47);
  static const tealTint = Color(0xFFE5F0ED); // teal wash
  static const terracotta = Color(0xFFC65F3F); // warm accent
  static const ochre = Color(0xFFCF9636); // golden accent
  static const sage = Color(0xFF6F8E78);
  static const navy = Color(0xFF2B4A63); // code identifiers
}

TextStyle _serif(
  double size,
  FontWeight weight,
  Color color, {
  double height = 1.04,
  double spacing = -0.2,
  FontStyle? style,
}) => TextStyle(
  fontFamily: _fontSerif,
  fontSize: size,
  fontWeight: weight,
  color: color,
  height: height,
  letterSpacing: spacing,
  fontStyle: style,
);

TextStyle _sans(
  double size,
  FontWeight weight,
  Color color, {
  double height = 1.45,
  double spacing = 0,
}) => TextStyle(
  fontFamily: _fontSans,
  fontSize: size,
  fontWeight: weight,
  color: color,
  height: height,
  letterSpacing: spacing,
);

TextStyle _mono(
  double size,
  FontWeight weight,
  Color color, {
  double height = 1.5,
}) => TextStyle(
  fontFamily: _fontMono,
  fontSize: size,
  fontWeight: weight,
  color: color,
  height: height,
);

// ---------------------------------------------------------------------------
// Sample documents and code snippets (drive the live playground + API gallery)
// ---------------------------------------------------------------------------

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
final editor = FlarkMarkdownEditor(
  controller: controller,
  interactionConfig: FlarkMarkdownInteractionConfig(
    onOpenLink: _open,
  ),
  editingMode: FlarkMarkdownEditingMode.liveRendered,
);
```
''';

const _editorSnippet = '''FlarkMarkdownEditor(
  initialMarkdown: '# Notes\\n\\nEdit **Markdown**.',
  onChanged: saveMarkdown,
)''';

const _controllerSnippet =
    '''final controller = FlarkFlutterController.fromMarkdown(markdown);

Row(
  children: [
    Expanded(child: FlarkMarkdownEditor(controller: controller)),
    Expanded(child: FlarkMarkdown(controller: controller)),
  ],
)''';

const _toolbarSnippet = '''final commands = controller.commands;

IconButton(
  icon: const Icon(Icons.format_bold),
  isSelected: commands.strongActive,
  onPressed: commands.canMutate ? commands.toggleStrong : null,
)''';

const _formSnippet = '''FlarkMarkdownEditorFormField(
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
      seedColor: _C.teal,
      brightness: Brightness.light,
    ).copyWith(primary: _C.teal, surface: _C.card);

    final base = ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      fontFamily: _fontSans,
    );

    return MaterialApp(
      title: 'Flark',
      debugShowCheckedModeBanner: false,
      // The site design is light-only; pin the markdown palette to match
      // instead of following the visitor's platform brightness.
      builder: (context, child) =>
          FlarkMarkdownTheme(data: FlarkMarkdownThemeData.light, child: child!),
      theme: base.copyWith(
        scaffoldBackgroundColor: _C.paper,
        dividerColor: _C.line,
        textSelectionTheme: const TextSelectionThemeData(
          cursorColor: _C.teal,
          selectionColor: Color(0x330E6E66),
        ),
        tooltipTheme: TooltipThemeData(
          waitDuration: const Duration(milliseconds: 450),
          decoration: BoxDecoration(
            color: _C.ink,
            borderRadius: BorderRadius.circular(8),
          ),
          textStyle: _sans(12.5, FontWeight.w600, Colors.white),
        ),
      ),
      home: const FlarkLandingScreen(),
    );
  }
}

class FlarkLandingScreen extends StatefulWidget {
  const FlarkLandingScreen({super.key});

  @override
  State<FlarkLandingScreen> createState() => _FlarkLandingScreenState();
}

class _FlarkLandingScreenState extends State<FlarkLandingScreen> {
  late FlarkFlutterController _controller;
  FocusNode _focusNode = FocusNode();
  final List<FocusNode> _retiredFocusNodes = [];

  final _scrollController = ScrollController();
  final _playgroundKey = GlobalKey();
  final _whyKey = GlobalKey();
  final _apiKey = GlobalKey();
  final _docsKey = GlobalKey();

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
    _scrollController.dispose();
    super.dispose();
  }

  void _scrollTo(GlobalKey key) {
    final ctx = key.currentContext;
    if (ctx == null) return;
    Scrollable.ensureVisible(
      ctx,
      duration: const Duration(milliseconds: 560),
      curve: Curves.easeOutCubic,
      alignment: 0.02,
    );
  }

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    final pad = width < 640
        ? 20.0
        : width < 1040
        ? 40.0
        : 64.0;

    return Scaffold(
      body: SafeArea(
        bottom: false,
        child: Scrollbar(
          controller: _scrollController,
          child: SingleChildScrollView(
            controller: _scrollController,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _TopBar(
                  pad: pad,
                  onPlayground: () => _scrollTo(_playgroundKey),
                  onWhy: () => _scrollTo(_whyKey),
                  onApi: () => _scrollTo(_apiKey),
                  onDocs: () => _scrollTo(_docsKey),
                ),
                _HeroBand(
                  key: _playgroundKey,
                  pad: pad,
                  controller: _controller,
                  focusNode: _focusNode,
                  onSample: () => _loadDocument(_sampleMarkdown),
                  onArticle: () => _loadDocument(_articleMarkdown),
                  onTables: () => _loadDocument(_tableMarkdown),
                  onScratch: () => _loadDocument(''),
                  onCommand: _runCommand,
                  onDocs: () => _scrollTo(_docsKey),
                ),
                _AtelierBand(pad: pad),
                _WhySection(key: _whyKey, pad: pad),
                _ApiSection(key: _apiKey, pad: pad),
                _DocsSection(key: _docsKey, pad: pad),
                const _Footer(),
              ],
            ),
          ),
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

Future<void> _open(String url) async {
  final uri = Uri.tryParse(url);
  if (uri == null) return;
  try {
    await launchUrl(uri, webOnlyWindowName: '_blank');
  } catch (_) {
    // Swallow launch failures (e.g. no handler) rather than throwing into
    // the gesture callback.
  }
}

// ---------------------------------------------------------------------------
// Top bar
// ---------------------------------------------------------------------------

class _TopBar extends StatelessWidget {
  const _TopBar({
    required this.pad,
    required this.onPlayground,
    required this.onWhy,
    required this.onApi,
    required this.onDocs,
  });

  final double pad;
  final VoidCallback onPlayground;
  final VoidCallback onWhy;
  final VoidCallback onApi;
  final VoidCallback onDocs;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: Color(0xF2FBF6EC),
        border: Border(bottom: BorderSide(color: _C.line)),
      ),
      child: Padding(
        padding: EdgeInsets.symmetric(horizontal: pad, vertical: 14),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1180),
            child: LayoutBuilder(
              builder: (context, constraints) {
                final compact = constraints.maxWidth < 860;
                return Row(
                  children: [
                    const _Brand(),
                    const Spacer(),
                    if (!compact) ...[
                      _NavLink(label: 'Playground', onTap: onPlayground),
                      _NavLink(label: 'Why Flark', onTap: onWhy),
                      _NavLink(label: 'API', onTap: onApi),
                      _NavLink(label: 'Docs', onTap: onDocs),
                      const SizedBox(width: 14),
                    ],
                    _GhostButton(
                      icon: Icons.code_rounded,
                      label: compact ? 'GitHub' : 'View source',
                      onPressed: () => _open(_repoUrl),
                    ),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }
}

class _Brand extends StatelessWidget {
  const _Brand();

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        const _Glyph(size: 34),
        const SizedBox(width: 11),
        Text('Flark', style: _serif(23, FontWeight.w900, _C.ink, spacing: 0)),
      ],
    );
  }
}

/// The Flark mark: an inked nib over a teal tile.
class _Glyph extends StatelessWidget {
  const _Glyph({required this.size});

  final double size;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        gradient: const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [_C.teal, _C.tealDeep],
        ),
        borderRadius: BorderRadius.circular(size * 0.28),
        boxShadow: [
          BoxShadow(
            color: _C.teal.withValues(alpha: 0.28),
            blurRadius: 12,
            offset: const Offset(0, 5),
          ),
        ],
      ),
      child: Icon(
        Icons.edit_note_rounded,
        size: size * 0.62,
        color: Colors.white,
      ),
    );
  }
}

class _NavLink extends StatelessWidget {
  const _NavLink({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return _Pressable(
      onTap: onTap,
      semanticLabel: label,
      builder: (hover) => Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        child: Text(
          label,
          style: _sans(14.5, FontWeight.w600, hover ? _C.teal : _C.inkSoft),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Hero
// ---------------------------------------------------------------------------

class _HeroBand extends StatelessWidget {
  const _HeroBand({
    super.key,
    required this.pad,
    required this.controller,
    required this.focusNode,
    required this.onSample,
    required this.onArticle,
    required this.onTables,
    required this.onScratch,
    required this.onCommand,
    required this.onDocs,
  });

  final double pad;
  final FlarkFlutterController controller;
  final FocusNode focusNode;
  final VoidCallback onSample;
  final VoidCallback onArticle;
  final VoidCallback onTables;
  final VoidCallback onScratch;
  final ValueChanged<_ToolbarCommand> onCommand;
  final VoidCallback onDocs;

  @override
  Widget build(BuildContext context) {
    return ClipRect(
      child: Stack(
        children: [
          const Positioned.fill(child: _HeroBackdrop()),
          Padding(
            padding: EdgeInsets.fromLTRB(pad, 48, pad, 72),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 1180),
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final stacked = constraints.maxWidth < 980;
                    final copy = _HeroCopy(onDocs: onDocs);
                    final workbench = _Workbench(
                      controller: controller,
                      focusNode: focusNode,
                      editorHeight: stacked ? 480 : 560,
                      onSample: onSample,
                      onArticle: onArticle,
                      onTables: onTables,
                      onScratch: onScratch,
                      onCommand: onCommand,
                    );
                    if (stacked) {
                      return Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [copy, const SizedBox(height: 34), workbench],
                      );
                    }
                    return Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(
                          flex: 5,
                          child: Padding(
                            padding: const EdgeInsets.only(top: 8),
                            child: copy,
                          ),
                        ),
                        const SizedBox(width: 44),
                        Expanded(flex: 6, child: workbench),
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

class _HeroBackdrop extends StatelessWidget {
  const _HeroBackdrop();

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
              colors: [Color(0xFFFCF9F1), _C.paper],
            ),
          ),
        ),
        const Positioned(
          left: -150,
          top: -120,
          child: _Blob(color: Color(0x1A0E6E66), size: 360),
        ),
        const Positioned(
          right: -90,
          bottom: -110,
          child: _Blob(color: Color(0x14CF9636), size: 320),
        ),
        const Positioned(
          right: 220,
          top: 40,
          child: _Blob(color: Color(0x12C65F3F), size: 150),
        ),
      ],
    );
  }
}

class _Blob extends StatelessWidget {
  const _Blob({required this.color, required this.size});

  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) {
    return IgnorePointer(
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          gradient: RadialGradient(colors: [color, color.withValues(alpha: 0)]),
        ),
      ),
    );
  }
}

class _HeroCopy extends StatelessWidget {
  const _HeroCopy({required this.onDocs});

  final VoidCallback onDocs;

  @override
  Widget build(BuildContext context) {
    return _FadeSlideIn(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _Eyebrow('Markdown-first editing'),
          const SizedBox(height: 20),
          // Required string: 'Flark Markdown Editor' — must be a plain Text so
          // find.text() in the example tests resolves it.
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 440),
            child: Text(
              'Flark Markdown Editor',
              style: _serif(54, FontWeight.w900, _C.ink, height: 1.0),
            ),
          ),
          const SizedBox(height: 12),
          Container(width: 64, height: 3, color: _C.terracotta),
          const SizedBox(height: 20),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 460),
            child: Text(
              'Edit Markdown live, in place — bold, lists, quotes, tables, and '
              'code fences render as you type, while the document stays plain, '
              'canonical Markdown the whole time.',
              style: _sans(17.5, FontWeight.w400, _C.inkSoft, height: 1.5),
            ),
          ),
          const SizedBox(height: 26),
          const _InstallPill(),
          const SizedBox(height: 24),
          Wrap(
            spacing: 14,
            runSpacing: 14,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              _PrimaryButton(
                icon: Icons.menu_book_rounded,
                label: 'Read the docs',
                onPressed: onDocs,
              ),
              _GhostButton(
                icon: Icons.north_east_rounded,
                label: 'pub.dev',
                onPressed: () => _open(_pubUrl),
              ),
            ],
          ),
          const SizedBox(height: 28),
          const Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              _ProofChip(
                icon: Icons.edit_note_rounded,
                label: 'Live-rendered editing',
              ),
              _ProofChip(icon: Icons.bolt_rounded, label: 'Native Comrak'),
              _ProofChip(icon: Icons.devices_rounded, label: 'Native + web'),
            ],
          ),
        ],
      ),
    );
  }
}

class _Eyebrow extends StatelessWidget {
  const _Eyebrow(this.text, {this.color = _C.teal});

  final String text;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(width: 22, height: 2, color: color),
        const SizedBox(width: 10),
        Flexible(
          child: Text(
            text.toUpperCase(),
            style: _sans(12.5, FontWeight.w700, color, spacing: 1.6),
          ),
        ),
      ],
    );
  }
}

class _InstallPill extends StatefulWidget {
  const _InstallPill();

  @override
  State<_InstallPill> createState() => _InstallPillState();
}

class _InstallPillState extends State<_InstallPill> {
  bool _copied = false;

  Future<void> _copy() async {
    await Clipboard.setData(const ClipboardData(text: 'flutter pub add flark'));
    if (!mounted) return;
    setState(() => _copied = true);
    await Future<void>.delayed(const Duration(milliseconds: 1400));
    if (mounted) setState(() => _copied = false);
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: _C.ink,
        borderRadius: BorderRadius.circular(12),
        boxShadow: [
          BoxShadow(
            color: _C.ink.withValues(alpha: 0.18),
            blurRadius: 22,
            offset: const Offset(0, 10),
          ),
        ],
      ),
      padding: const EdgeInsets.fromLTRB(18, 13, 10, 13),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('\$', style: _mono(14.5, FontWeight.w500, _C.sage)),
          const SizedBox(width: 10),
          Text(
            'flutter pub add flark',
            style: _mono(14.5, FontWeight.w500, const Color(0xFFF3EEDF)),
          ),
          const SizedBox(width: 14),
          Tooltip(
            message: _copied ? 'Copied' : 'Copy',
            child: IconButton(
              onPressed: _copy,
              visualDensity: VisualDensity.compact,
              icon: Icon(
                _copied ? Icons.check_rounded : Icons.copy_rounded,
                size: 18,
                color: _copied ? _C.sage : const Color(0xFFB8B09C),
              ),
            ),
          ),
        ],
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
    return Container(
      decoration: BoxDecoration(
        color: _C.card,
        border: Border.all(color: _C.line),
        borderRadius: BorderRadius.circular(30),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 15, vertical: 9),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 17, color: _C.teal),
          const SizedBox(width: 8),
          Text(label, style: _sans(13.5, FontWeight.w600, _C.ink)),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Atelier showcase
// ---------------------------------------------------------------------------

class _AtelierBand extends StatelessWidget {
  const _AtelierBand({required this.pad});

  final double pad;

  @override
  Widget build(BuildContext context) {
    return _Band(
      pad: pad,
      background: const BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [_C.paperDeep, _C.paper],
        ),
      ),
      child: const _AtelierBanner(),
    );
  }
}

class _AtelierBanner extends StatelessWidget {
  const _AtelierBanner();

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final w = constraints.maxWidth;
        final height = w < 640
            ? 250.0
            : w < 980
            ? 300.0
            : 360.0;
        return Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(22),
            boxShadow: [
              BoxShadow(
                color: _C.ink.withValues(alpha: 0.16),
                blurRadius: 48,
                offset: const Offset(0, 24),
              ),
            ],
          ),
          clipBehavior: Clip.antiAlias,
          child: Stack(
            children: [
              Positioned.fill(
                child: Image.asset(
                  _heroArtworkAsset,
                  fit: BoxFit.cover,
                  alignment: Alignment.centerLeft,
                  semanticLabel:
                      'A painterly writing studio with floating '
                      'Markdown and code cards',
                  errorBuilder: (context, error, stackTrace) =>
                      const ColoredBox(color: _C.tealTint),
                ),
              ),
              const Positioned.fill(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    gradient: LinearGradient(
                      begin: Alignment.centerLeft,
                      end: Alignment.centerRight,
                      colors: [Color(0xCC1B1810), Color(0x111B1810)],
                    ),
                  ),
                ),
              ),
              // Non-positioned child drives the banner height; image fills it.
              SizedBox(
                width: double.infinity,
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: height),
                  child: Padding(
                    padding: EdgeInsets.all(w < 640 ? 26 : 44),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const _Eyebrow(
                          'The Flark atelier',
                          color: Color(0xFFE9D9B8),
                        ),
                        const SizedBox(height: 16),
                        ConstrainedBox(
                          constraints: const BoxConstraints(maxWidth: 540),
                          child: Text(
                            'Markdown is the document — everything else is just '
                            'a nicer way to write it.',
                            style: _serif(
                              w < 640 ? 25 : 34,
                              FontWeight.w400,
                              Colors.white,
                              height: 1.12,
                              style: FontStyle.italic,
                            ),
                          ),
                        ),
                        const SizedBox(height: 20),
                        const Wrap(
                          spacing: 10,
                          runSpacing: 10,
                          children: [
                            _GlassTag('CommonMark'),
                            _GlassTag('GitHub-flavored'),
                            _GlassTag('Native + WASM Comrak'),
                          ],
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _GlassTag extends StatelessWidget {
  const _GlassTag(this.label);

  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: Colors.white.withValues(alpha: 0.16),
        borderRadius: BorderRadius.circular(30),
        border: Border.all(color: Colors.white.withValues(alpha: 0.32)),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
      child: Text(label, style: _sans(13, FontWeight.w600, Colors.white)),
    );
  }
}

// ---------------------------------------------------------------------------
// Live playground (workbench)
// ---------------------------------------------------------------------------

class _Workbench extends StatelessWidget {
  const _Workbench({
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
    return Container(
      decoration: BoxDecoration(
        color: _C.card,
        borderRadius: BorderRadius.circular(18),
        border: Border.all(color: _C.line),
        boxShadow: [
          BoxShadow(
            color: _C.ink.withValues(alpha: 0.10),
            blurRadius: 50,
            offset: const Offset(0, 24),
          ),
        ],
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _WindowBar(controller: controller),
          _WorkbenchToolbar(
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
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 14),
              child: _EditorPane(controller: controller, focusNode: focusNode),
            ),
          ),
        ],
      ),
    );
  }
}

class _WindowBar extends StatelessWidget {
  const _WindowBar({required this.controller});

  final FlarkFlutterController controller;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: const BoxDecoration(
        color: Color(0xFFF6EFDF),
        border: Border(bottom: BorderSide(color: _C.lineSoft)),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
      child: Row(
        children: [
          const _Dot(_C.terracotta),
          const SizedBox(width: 7),
          const _Dot(_C.ochre),
          const SizedBox(width: 7),
          const _Dot(_C.sage),
          const SizedBox(width: 16),
          const _Glyph(size: 22),
          const SizedBox(width: 9),
          Expanded(
            child: Text(
              'Flark Markdown',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: _sans(14, FontWeight.w700, _C.ink),
            ),
          ),
          const SizedBox(width: 12),
          const _LivePill(),
          const SizedBox(width: 8),
          _ParseBadge(controller: controller),
        ],
      ),
    );
  }
}

class _LivePill extends StatelessWidget {
  const _LivePill();

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: _C.card,
        border: Border.all(color: _C.line),
        borderRadius: BorderRadius.circular(20),
      ),
      padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 5),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 7,
            height: 7,
            decoration: const BoxDecoration(
              color: _C.terracotta,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 7),
          // Required string: 'Live playground'
          Text(
            'Live playground',
            style: _sans(12, FontWeight.w700, _C.inkSoft),
          ),
        ],
      ),
    );
  }
}

class _Dot extends StatelessWidget {
  const _Dot(this.color);
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 11,
      height: 11,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.85),
        shape: BoxShape.circle,
      ),
    );
  }
}

class _ParseBadge extends StatelessWidget {
  const _ParseBadge({required this.controller});

  final FlarkFlutterController controller;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final parsed = controller.hasAuthoritativeRenderPlan;
        return Container(
          decoration: BoxDecoration(
            color: parsed ? _C.tealTint : const Color(0xFFF6E9D6),
            borderRadius: BorderRadius.circular(20),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 5),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 7,
                height: 7,
                decoration: BoxDecoration(
                  color: parsed ? _C.teal : _C.ochre,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 7),
              Text(
                parsed ? 'Parsed' : 'Parsing',
                style: _sans(
                  12,
                  FontWeight.w700,
                  parsed ? _C.tealDeep : const Color(0xFF8A6A1E),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _WorkbenchToolbar extends StatelessWidget {
  const _WorkbenchToolbar({
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
    return Container(
      decoration: const BoxDecoration(
        color: _C.card,
        border: Border(bottom: BorderSide(color: _C.lineSoft)),
      ),
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final compact = constraints.maxWidth < 720;
          final scenarios = Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              _ScenarioChip(
                key: const ValueKey('flark-example-scenario-sample'),
                icon: Icons.description_outlined,
                label: 'Sample',
                onPressed: onSample,
              ),
              _ScenarioChip(
                key: const ValueKey('flark-example-scenario-article'),
                icon: Icons.article_outlined,
                label: 'Article',
                onPressed: onArticle,
              ),
              _ScenarioChip(
                key: const ValueKey('flark-example-scenario-tables'),
                icon: Icons.table_chart_outlined,
                label: 'Tables',
                onPressed: onTables,
              ),
              _ScenarioChip(
                key: const ValueKey('flark-example-scenario-scratch'),
                icon: Icons.add_rounded,
                label: 'Scratch',
                onPressed: onScratch,
                emphasized: true,
              ),
            ],
          );

          final commands = _CommandCluster(
            controller: controller,
            onCommand: onCommand,
            alignEnd: !compact,
          );

          if (compact) {
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [scenarios, const SizedBox(height: 12), commands],
            );
          }
          return Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              scenarios,
              const Spacer(),
              Flexible(child: commands),
            ],
          );
        },
      ),
    );
  }
}

class _CommandCluster extends StatelessWidget {
  const _CommandCluster({
    required this.controller,
    required this.onCommand,
    required this.alignEnd,
  });

  final FlarkFlutterController controller;
  final ValueChanged<_ToolbarCommand> onCommand;
  final bool alignEnd;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final commands = controller.commands;
        final canMutate = commands.canMutate;
        final hasRange = !controller.selection.isCollapsed;
        final canStyle = canMutate && hasRange;
        return Wrap(
          spacing: 6,
          runSpacing: 6,
          alignment: alignEnd ? WrapAlignment.end : WrapAlignment.start,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
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
            const _ClusterDivider(),
            _CommandButton(
              buttonKey: const ValueKey('flark-example-command-bold'),
              tooltip: hasRange ? 'Bold' : 'Select text to bold',
              icon: Icons.format_bold,
              selected: commands.strongActive,
              enabled: canStyle,
              onPressed: () => onCommand(_ToolbarCommand.bold),
            ),
            _CommandButton(
              buttonKey: const ValueKey('flark-example-command-italic'),
              tooltip: hasRange ? 'Italic' : 'Select text to italicize',
              icon: Icons.format_italic,
              selected: commands.emphasisActive,
              enabled: canStyle,
              onPressed: () => onCommand(_ToolbarCommand.italic),
            ),
            const _ClusterDivider(),
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
            const _ClusterDivider(),
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
          ],
        );
      },
    );
  }
}

class _ClusterDivider extends StatelessWidget {
  const _ClusterDivider();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 1,
      height: 26,
      margin: const EdgeInsets.symmetric(horizontal: 4),
      color: _C.line,
    );
  }
}

class _ScenarioChip extends StatelessWidget {
  const _ScenarioChip({
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
    return _Pressable(
      onTap: onPressed,
      semanticLabel: 'Load $label document',
      builder: (hover) {
        final Color bg;
        final Color fg;
        final Color border;
        if (emphasized) {
          bg = hover ? _C.tealDeep : _C.teal;
          fg = Colors.white;
          border = bg;
        } else {
          bg = hover ? _C.tealTint : _C.card;
          fg = hover ? _C.tealDeep : _C.ink;
          border = hover ? _C.teal.withValues(alpha: 0.4) : _C.line;
        }
        return AnimatedContainer(
          duration: const Duration(milliseconds: 140),
          decoration: BoxDecoration(
            color: bg,
            border: Border.all(color: border),
            borderRadius: BorderRadius.circular(9),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 9),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 17, color: fg),
              const SizedBox(width: 7),
              Text(label, style: _sans(13.5, FontWeight.w600, fg)),
            ],
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
      child: IconButton(
        key: widget.buttonKey,
        focusNode: _focusNode,
        isSelected: widget.selected,
        onPressed: widget.enabled ? widget.onPressed : null,
        icon: Icon(widget.icon, size: 19),
        style: IconButton.styleFrom(
          foregroundColor: _C.inkSoft,
          disabledForegroundColor: const Color(0xFFBDB6A6),
          backgroundColor: widget.selected ? _C.tealTint : Colors.transparent,
          highlightColor: _C.tealTint,
          hoverColor: _C.lineSoft,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(8),
            side: BorderSide(
              color: widget.selected ? _C.teal.withValues(alpha: 0.4) : _C.line,
            ),
          ),
          minimumSize: const Size(38, 38),
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          visualDensity: VisualDensity.compact,
        ),
      ),
    );
  }
}

class _EditorPane extends StatelessWidget {
  const _EditorPane({required this.controller, required this.focusNode});

  final FlarkFlutterController controller;
  final FocusNode focusNode;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: 4, bottom: 10),
          child: Row(
            children: [
              const Icon(Icons.edit_note_rounded, size: 17, color: _C.inkFaint),
              const SizedBox(width: 7),
              // Required string: 'Live Markdown field'
              Text(
                'Live Markdown field',
                style: _sans(13, FontWeight.w700, _C.inkSoft, spacing: 0.2),
              ),
            ],
          ),
        ),
        Expanded(
          child: Container(
            decoration: BoxDecoration(
              color: const Color(0xFFFFFEFB),
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: _C.line),
            ),
            clipBehavior: Clip.antiAlias,
            padding: const EdgeInsets.fromLTRB(18, 16, 14, 12),
            child: FlarkMarkdownEditor(
              controller: controller,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
              focusNode: focusNode,
              expands: true,
              maxLines: null,
              cursorColor: _C.teal,
              style: _sans(16, FontWeight.w400, _C.ink, height: 1.5),
            ),
          ),
        ),
        const SizedBox(height: 10),
        _EditorFooter(controller: controller),
      ],
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
      builder: (context, _) {
        final selection = controller.selection;
        final selected = selection.end - selection.start;
        final caret = selection.extentOffset.clamp(
          0,
          controller.markdown.length,
        );
        final status = selected > 0
            ? 'Caret $caret  ·  $selected selected'
            : 'Caret $caret  ·  ${controller.markdown.length} chars';
        return Row(
          children: [
            // Required: a single Text containing 'Caret'.
            Flexible(
              child: Text(
                status,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: _mono(12, FontWeight.w500, _C.inkFaint),
              ),
            ),
            const SizedBox(width: 12),
            _FooterAction(
              buttonKey: const ValueKey('flark-example-undo'),
              icon: Icons.undo_rounded,
              label: 'Undo',
              onPressed: controller.runtime.canUndo ? controller.undo : null,
            ),
            const SizedBox(width: 2),
            _FooterAction(
              buttonKey: const ValueKey('flark-example-redo'),
              icon: Icons.redo_rounded,
              label: 'Redo',
              onPressed: controller.runtime.canRedo ? controller.redo : null,
            ),
          ],
        );
      },
    );
  }
}

class _FooterAction extends StatelessWidget {
  const _FooterAction({
    required this.buttonKey,
    required this.icon,
    required this.label,
    required this.onPressed,
  });

  final Key buttonKey;
  final IconData icon;
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return TextButton.icon(
      key: buttonKey,
      onPressed: onPressed,
      icon: Icon(icon, size: 17),
      label: Text(label, style: _sans(13, FontWeight.w600, _C.inkSoft)),
      style: TextButton.styleFrom(
        foregroundColor: _C.inkSoft,
        disabledForegroundColor: const Color(0xFFC2BBAB),
        visualDensity: VisualDensity.compact,
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
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

// ---------------------------------------------------------------------------
// Why Flark
// ---------------------------------------------------------------------------

class _WhySection extends StatelessWidget {
  const _WhySection({super.key, required this.pad});

  final double pad;

  @override
  Widget build(BuildContext context) {
    return _Band(
      pad: pad,
      background: const BoxDecoration(color: _C.card),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _Eyebrow('Why Flark', color: _C.terracotta),
          const SizedBox(height: 18),
          _SectionTitle('The document is always FlarkMarkdown.', maxWidth: 720),
          const SizedBox(height: 14),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 720),
            child: Text(
              'No private rich-text model, no lossy round-trips. The editor, '
              'preview, toolbar commands, and rendered blocks all read from one '
              'canonical source document.',
              style: _sans(17, FontWeight.w400, _C.inkSoft),
            ),
          ),
          const SizedBox(height: 40),
          LayoutBuilder(
            builder: (context, constraints) {
              const tiles = [
                _WhyTile(
                  number: '01',
                  accent: _C.teal,
                  icon: Icons.notes_rounded,
                  title: 'Source first',
                  body:
                      'Markdown is the truth. The rendered field is an editing '
                      'surface over it, not a separate state to keep in sync.',
                ),
                _WhyTile(
                  number: '02',
                  accent: _C.terracotta,
                  icon: Icons.tune_rounded,
                  title: 'UI stays yours',
                  body:
                      'Commands expose state and mutations. Your app owns what '
                      'the toolbar, menus, and shortcuts actually look like.',
                ),
                _WhyTile(
                  number: '03',
                  accent: _C.ochre,
                  icon: Icons.bolt_rounded,
                  title: 'Parser-backed',
                  body:
                      'Comrak plans keep lists, quotes, tables, tasks, and code '
                      'fences grounded in real Markdown — native and on web.',
                ),
              ];
              if (constraints.maxWidth < 860) {
                return Column(
                  children: [
                    for (final t in tiles) ...[
                      t,
                      if (t != tiles.last) const SizedBox(height: 16),
                    ],
                  ],
                );
              }
              return IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final t in tiles) ...[
                      Expanded(child: t),
                      if (t != tiles.last) const SizedBox(width: 20),
                    ],
                  ],
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}

class _WhyTile extends StatefulWidget {
  const _WhyTile({
    required this.number,
    required this.accent,
    required this.icon,
    required this.title,
    required this.body,
  });

  final String number;
  final Color accent;
  final IconData icon;
  final String title;
  final String body;

  @override
  State<_WhyTile> createState() => _WhyTileState();
}

class _WhyTileState extends State<_WhyTile> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      onEnter: (_) => setState(() => _hover = true),
      onExit: (_) => setState(() => _hover = false),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 180),
        curve: Curves.easeOut,
        transform: Matrix4.translationValues(0, _hover ? -4 : 0, 0),
        padding: const EdgeInsets.fromLTRB(24, 24, 24, 26),
        decoration: BoxDecoration(
          color: _C.paper,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(
            color: _hover ? widget.accent.withValues(alpha: 0.4) : _C.line,
          ),
          boxShadow: [
            if (_hover)
              BoxShadow(
                color: widget.accent.withValues(alpha: 0.14),
                blurRadius: 30,
                offset: const Offset(0, 14),
              ),
          ],
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Container(
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    color: widget.accent.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Icon(widget.icon, size: 22, color: widget.accent),
                ),
                const Spacer(),
                Text(
                  widget.number,
                  style: _serif(22, FontWeight.w700, widget.accent, spacing: 0),
                ),
              ],
            ),
            const SizedBox(height: 20),
            Text(
              widget.title,
              style: _serif(23, FontWeight.w700, _C.ink, spacing: 0),
            ),
            const SizedBox(height: 10),
            Text(widget.body, style: _sans(15, FontWeight.w400, _C.inkSoft)),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// API gallery
// ---------------------------------------------------------------------------

class _ApiSection extends StatelessWidget {
  const _ApiSection({super.key, required this.pad});

  final double pad;

  @override
  Widget build(BuildContext context) {
    return _Band(
      pad: pad,
      background: const BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [_C.paperDeep, _C.paper],
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _Eyebrow('The API', color: _C.navy),
          const SizedBox(height: 18),
          _SectionTitle('One barrel. A handful of widgets.', maxWidth: 720),
          const SizedBox(height: 14),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 720),
            child: Text(
              "Import package:flark/flark.dart and you have everything the "
              'playground above uses. Drop in a field, share a controller, or '
              'wire commands into your own UI.',
              style: _sans(17, FontWeight.w400, _C.inkSoft),
            ),
          ),
          const SizedBox(height: 32),
          LayoutBuilder(
            builder: (context, constraints) {
              final compact = constraints.maxWidth < 900;
              const primary = _CodeCard(
                title: 'A field',
                subtitle: 'flark.dart',
                code: _editorSnippet,
              );
              const rail = Column(
                children: [
                  _RecipeCard(
                    title: 'Shared preview',
                    code: _controllerSnippet,
                  ),
                  SizedBox(height: 16),
                  _RecipeCard(title: 'Toolbar command', code: _toolbarSnippet),
                  SizedBox(height: 16),
                  _RecipeCard(title: 'Form field', code: _formSnippet),
                ],
              );
              if (compact) {
                return const Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [primary, SizedBox(height: 16), rail],
                );
              }
              return const Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(flex: 6, child: primary),
                  SizedBox(width: 20),
                  Expanded(flex: 5, child: rail),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

class _CodeCard extends StatelessWidget {
  const _CodeCard({
    required this.title,
    required this.subtitle,
    required this.code,
  });

  final String title;
  final String subtitle;
  final String code;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: const Color(0xFF221E16),
        borderRadius: BorderRadius.circular(16),
        boxShadow: [
          BoxShadow(
            color: _C.ink.withValues(alpha: 0.22),
            blurRadius: 40,
            offset: const Offset(0, 20),
          ),
        ],
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            decoration: const BoxDecoration(
              color: Color(0xFF2C2719),
              border: Border(bottom: BorderSide(color: Color(0xFF3C3624))),
            ),
            padding: const EdgeInsets.fromLTRB(20, 14, 20, 14),
            child: Row(
              children: [
                const Icon(Icons.terminal_rounded, size: 18, color: _C.sage),
                const SizedBox(width: 10),
                Text(
                  title,
                  style: _sans(14.5, FontWeight.w700, const Color(0xFFF3EEDF)),
                ),
                const Spacer(),
                Text(
                  subtitle,
                  style: _mono(12.5, FontWeight.w500, const Color(0xFF8F876F)),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(22, 22, 22, 24),
            child: _CodeText(code, fontSize: 14.5, onDark: true),
          ),
        ],
      ),
    );
  }
}

class _RecipeCard extends StatelessWidget {
  const _RecipeCard({required this.title, required this.code});

  final String title;
  final String code;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: _C.card,
        border: Border.all(color: _C.line),
        borderRadius: BorderRadius.circular(14),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            padding: const EdgeInsets.fromLTRB(18, 12, 18, 12),
            decoration: const BoxDecoration(
              border: Border(bottom: BorderSide(color: _C.lineSoft)),
            ),
            child: Row(
              children: [
                Container(
                  width: 8,
                  height: 8,
                  decoration: const BoxDecoration(
                    color: _C.teal,
                    shape: BoxShape.circle,
                  ),
                ),
                const SizedBox(width: 10),
                Text(title, style: _sans(13.5, FontWeight.w700, _C.ink)),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
            child: _CodeText(code, fontSize: 12.5, onDark: false),
          ),
        ],
      ),
    );
  }
}

/// Lightweight Dart-ish syntax tinting for the code gallery.
class _CodeText extends StatelessWidget {
  const _CodeText(this.code, {required this.fontSize, required this.onDark});

  final String code;
  final double fontSize;
  final bool onDark;

  static const _keywords = {
    'final',
    'const',
    'var',
    'void',
    'return',
    'import',
    'class',
    'extends',
    'new',
    'true',
    'false',
    'null',
    'if',
    'else',
    'for',
    'this',
    'super',
    'late',
    'required',
    'async',
    'await',
    'with',
    'as',
    'show',
    'enum',
  };

  @override
  Widget build(BuildContext context) {
    final base = onDark ? const Color(0xFFE9E3D2) : _C.ink;
    final keyword = onDark ? const Color(0xFF6FC3B3) : _C.teal;
    final string = onDark ? const Color(0xFFE6A06E) : _C.terracotta;
    final type = onDark ? const Color(0xFF8FB8DA) : _C.navy;
    final comment = onDark ? const Color(0xFF7C7560) : _C.inkFaint;
    final number = onDark ? const Color(0xFFE0BE6A) : _C.ochre;

    final spans = <TextSpan>[];
    var i = 0;
    final n = code.length;

    bool isWord(int c) =>
        (c >= 48 && c <= 57) ||
        (c >= 65 && c <= 90) ||
        (c >= 97 && c <= 122) ||
        c == 95;

    while (i < n) {
      final ch = code[i];
      // line comment
      if (ch == '/' && i + 1 < n && code[i + 1] == '/') {
        var j = i;
        while (j < n && code[j] != '\n') {
          j++;
        }
        spans.add(
          TextSpan(
            text: code.substring(i, j),
            style: TextStyle(color: comment, fontStyle: FontStyle.italic),
          ),
        );
        i = j;
        continue;
      }
      // string
      if (ch == "'" || ch == '"') {
        final quote = ch;
        var j = i + 1;
        while (j < n) {
          if (code[j] == '\\' && j + 1 < n) {
            j += 2;
            continue;
          }
          if (code[j] == quote) {
            j++;
            break;
          }
          j++;
        }
        spans.add(
          TextSpan(
            text: code.substring(i, j),
            style: TextStyle(color: string),
          ),
        );
        i = j;
        continue;
      }
      // identifier / keyword
      final code0 = code.codeUnitAt(i);
      if (isWord(code0) && !(code0 >= 48 && code0 <= 57)) {
        var j = i;
        while (j < n && isWord(code.codeUnitAt(j))) {
          j++;
        }
        final word = code.substring(i, j);
        final Color c;
        if (_keywords.contains(word)) {
          c = keyword;
        } else if (word.isNotEmpty &&
            word[0] == word[0].toUpperCase() &&
            word[0] != word[0].toLowerCase()) {
          c = type;
        } else {
          c = base;
        }
        spans.add(
          TextSpan(
            text: word,
            style: TextStyle(color: c),
          ),
        );
        i = j;
        continue;
      }
      // number
      if (code0 >= 48 && code0 <= 57) {
        var j = i;
        while (j < n &&
            (code.codeUnitAt(j) >= 48 && code.codeUnitAt(j) <= 57 ||
                code[j] == '.')) {
          j++;
        }
        spans.add(
          TextSpan(
            text: code.substring(i, j),
            style: TextStyle(color: number),
          ),
        );
        i = j;
        continue;
      }
      // default single char
      spans.add(
        TextSpan(
          text: ch,
          style: TextStyle(color: base),
        ),
      );
      i++;
    }

    return RichText(
      text: TextSpan(
        style: _mono(fontSize, FontWeight.w500, base, height: 1.6),
        children: spans,
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Docs
// ---------------------------------------------------------------------------

class _DocsSection extends StatelessWidget {
  const _DocsSection({super.key, required this.pad});

  final double pad;

  @override
  Widget build(BuildContext context) {
    return _Band(
      pad: pad,
      background: const BoxDecoration(color: _C.card),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _Eyebrow('Keep reading', color: _C.sage),
          const SizedBox(height: 18),
          _SectionTitle(
            'From a first field to the full surface.',
            maxWidth: 720,
          ),
          const SizedBox(height: 32),
          LayoutBuilder(
            builder: (context, constraints) {
              const docs = [
                _DocCard(
                  icon: Icons.rocket_launch_outlined,
                  title: 'Getting started',
                  body: 'Build an editor, preview Markdown, and share state.',
                  href: '${_docBase}getting_started.md',
                ),
                _DocCard(
                  icon: Icons.receipt_long_outlined,
                  title: 'Cookbook',
                  body: 'Toolbar, form, dirty-save, link, and preview recipes.',
                  href: '${_docBase}cookbook.md',
                ),
                _DocCard(
                  icon: Icons.account_tree_outlined,
                  title: 'API surface',
                  body: 'Pick the app, core, or advanced import deliberately.',
                  href: '${_docBase}api_surface.md',
                ),
                _DocCard(
                  icon: Icons.speed_outlined,
                  title: 'Benchmarks',
                  body: 'The enforced performance lane and methodology.',
                  href: '${_docBase}benchmarks.md',
                ),
              ];
              final columns = constraints.maxWidth < 620
                  ? 1
                  : constraints.maxWidth < 940
                  ? 2
                  : 4;
              return _DocGrid(columns: columns, cards: docs);
            },
          ),
        ],
      ),
    );
  }
}

class _DocGrid extends StatelessWidget {
  const _DocGrid({required this.columns, required this.cards});

  final int columns;
  final List<_DocCard> cards;

  @override
  Widget build(BuildContext context) {
    const gap = 16.0;
    final rows = <Widget>[];
    for (var i = 0; i < cards.length; i += columns) {
      final slice = cards.sublist(i, (i + columns).clamp(0, cards.length));
      rows.add(
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (var j = 0; j < columns; j++) ...[
                Expanded(
                  child: j < slice.length ? slice[j] : const SizedBox.shrink(),
                ),
                if (j != columns - 1) const SizedBox(width: gap),
              ],
            ],
          ),
        ),
      );
      if (i + columns < cards.length) {
        rows.add(const SizedBox(height: gap));
      }
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: rows,
    );
  }
}

class _DocCard extends StatelessWidget {
  const _DocCard({
    required this.icon,
    required this.title,
    required this.body,
    required this.href,
  });

  final IconData icon;
  final String title;
  final String body;
  final String href;

  @override
  Widget build(BuildContext context) {
    return _Pressable(
      onTap: () => _open(href),
      semanticLabel: '$title — open documentation',
      builder: (hover) => AnimatedContainer(
        duration: const Duration(milliseconds: 160),
        transform: Matrix4.translationValues(0, hover ? -3 : 0, 0),
        padding: const EdgeInsets.fromLTRB(20, 22, 20, 22),
        decoration: BoxDecoration(
          color: _C.paper,
          borderRadius: BorderRadius.circular(14),
          border: Border.all(
            color: hover ? _C.teal.withValues(alpha: 0.4) : _C.line,
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon, size: 24, color: _C.teal),
            const SizedBox(height: 16),
            Text(title, style: _serif(19, FontWeight.w700, _C.ink, spacing: 0)),
            const SizedBox(height: 8),
            Text(body, style: _sans(13.5, FontWeight.w400, _C.inkSoft)),
            const SizedBox(height: 16),
            Row(
              children: [
                Text('Read', style: _sans(13, FontWeight.w700, _C.teal)),
                const SizedBox(width: 5),
                AnimatedSlide(
                  duration: const Duration(milliseconds: 160),
                  offset: Offset(hover ? 0.25 : 0, 0),
                  child: const Icon(
                    Icons.arrow_forward_rounded,
                    size: 15,
                    color: _C.teal,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

class _Footer extends StatelessWidget {
  const _Footer();

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(color: _C.ink),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 40, vertical: 48),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1180),
            child: LayoutBuilder(
              builder: (context, constraints) {
                final stacked = constraints.maxWidth < 860;
                final brand = Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const _Glyph(size: 32),
                        const SizedBox(width: 11),
                        Flexible(
                          child: Text(
                            'Flark',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: _serif(
                              22,
                              FontWeight.w900,
                              Colors.white,
                              spacing: 0,
                            ),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    Text(
                      'Markdown-first editing and rendering for Flutter.',
                      style: _sans(
                        14.5,
                        FontWeight.w400,
                        const Color(0xFFBDB6A6),
                      ),
                    ),
                    const SizedBox(height: 10),
                    Container(
                      decoration: BoxDecoration(
                        color: const Color(0xFF2C2719),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      padding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 8,
                      ),
                      child: Text(
                        "import 'package:flark/flark.dart';",
                        style: _mono(
                          13,
                          FontWeight.w500,
                          const Color(0xFF8FB8DA),
                        ),
                      ),
                    ),
                  ],
                );
                final links = Wrap(
                  spacing: 28,
                  runSpacing: 12,
                  children: [
                    _FooterLink(label: 'pub.dev', onTap: () => _open(_pubUrl)),
                    _FooterLink(label: 'GitHub', onTap: () => _open(_repoUrl)),
                    _FooterLink(
                      label: 'Getting started',
                      onTap: () => _open('${_docBase}getting_started.md'),
                    ),
                    _FooterLink(
                      label: 'Cookbook',
                      onTap: () => _open('${_docBase}cookbook.md'),
                    ),
                  ],
                );
                if (stacked) {
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [brand, const SizedBox(height: 28), links],
                  );
                }
                return Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(child: brand),
                    const SizedBox(width: 40),
                    links,
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }
}

class _FooterLink extends StatelessWidget {
  const _FooterLink({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return _Pressable(
      onTap: onTap,
      semanticLabel: label,
      builder: (hover) => Text(
        label,
        style: _sans(
          14.5,
          FontWeight.w600,
          hover ? Colors.white : const Color(0xFFBDB6A6),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

class _Band extends StatelessWidget {
  const _Band({
    required this.pad,
    required this.child,
    required this.background,
  });

  final double pad;
  final Widget child;
  final BoxDecoration background;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: background,
      child: Padding(
        padding: EdgeInsets.fromLTRB(pad, 76, pad, 84),
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

class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.text, {required this.maxWidth});

  final String text;
  final double maxWidth;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: BoxConstraints(maxWidth: maxWidth),
      child: Text(
        text,
        style: _serif(38, FontWeight.w700, _C.ink, height: 1.06),
      ),
    );
  }
}

class _PrimaryButton extends StatelessWidget {
  const _PrimaryButton({
    required this.icon,
    required this.label,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return _Pressable(
      onTap: onPressed,
      semanticLabel: label,
      builder: (hover) => AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 15),
        decoration: BoxDecoration(
          color: hover ? _C.tealDeep : _C.teal,
          borderRadius: BorderRadius.circular(11),
          boxShadow: [
            BoxShadow(
              color: _C.teal.withValues(alpha: hover ? 0.36 : 0.24),
              blurRadius: hover ? 26 : 18,
              offset: const Offset(0, 9),
            ),
          ],
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 18, color: Colors.white),
            const SizedBox(width: 9),
            Text(label, style: _sans(15, FontWeight.w700, Colors.white)),
          ],
        ),
      ),
    );
  }
}

class _GhostButton extends StatelessWidget {
  const _GhostButton({
    required this.icon,
    required this.label,
    required this.onPressed,
  });

  final IconData icon;
  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return _Pressable(
      onTap: onPressed,
      semanticLabel: label,
      builder: (hover) => AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 13),
        decoration: BoxDecoration(
          color: hover ? _C.tealTint : _C.card,
          borderRadius: BorderRadius.circular(11),
          border: Border.all(
            color: hover ? _C.teal.withValues(alpha: 0.45) : _C.line,
          ),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 17, color: _C.ink),
            const SizedBox(width: 8),
            Text(label, style: _sans(14.5, FontWeight.w700, _C.ink)),
          ],
        ),
      ),
    );
  }
}

/// Hover + tap + click-cursor + button semantics for the site's custom
/// (non-Material) interactive surfaces. The visual is supplied by [builder],
/// which receives the current hover state.
class _Pressable extends StatefulWidget {
  const _Pressable({
    required this.onTap,
    required this.semanticLabel,
    required this.builder,
  });

  final VoidCallback onTap;
  final String semanticLabel;
  final Widget Function(bool hovering) builder;

  @override
  State<_Pressable> createState() => _PressableState();
}

class _PressableState extends State<_Pressable> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: widget.semanticLabel,
      excludeSemantics: true,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onTap,
          behavior: HitTestBehavior.opaque,
          child: widget.builder(_hover),
        ),
      ),
    );
  }
}

/// One-shot fade + rise used by hero content.
class _FadeSlideIn extends StatefulWidget {
  const _FadeSlideIn({required this.child});

  final Widget child;
  static const double _offset = 16;

  @override
  State<_FadeSlideIn> createState() => _FadeSlideInState();
}

class _FadeSlideInState extends State<_FadeSlideIn>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 640),
  )..forward();

  late final Animation<double> _curve = CurvedAnimation(
    parent: _controller,
    curve: Curves.easeOutCubic,
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _curve,
      builder: (context, child) {
        return Opacity(
          opacity: _curve.value,
          child: Transform.translate(
            offset: Offset(0, (1 - _curve.value) * _FadeSlideIn._offset),
            child: child,
          ),
        );
      },
      child: widget.child,
    );
  }
}

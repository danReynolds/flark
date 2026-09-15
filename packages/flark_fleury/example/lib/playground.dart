import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart';

import 'theme_palette.dart';

const sample = '''# Flark / Fleury

One Markdown kernel, a second host. Edit **bold**, *italic*, or `inline code`.
Try [a link](https://dart.dev) and a wide character: 界. Then resize the window.

> The document, selection and undo history stay in Flark.

- [ ] Click this task checkbox
- Try Enter to continue this list

```ruby
def hello
  puts 'Hello from Fleury'
end
```

Type here. Cmd/Ctrl+B toggles bold. Cmd/Ctrl+Z undoes.
''';

class Playground extends StatefulWidget {
  const Playground({super.key, required this.controller, this.onOpenLink});
  final FlarkFleuryController controller;
  final void Function(Uri)? onOpenLink;
  @override
  State<Playground> createState() => _PlaygroundState();
}

class _PlaygroundState extends State<Playground> {
  Color? _linkOverride, _headingOverride, _keywordOverride;
  bool _light = false, _showTheme = false;
  String _message = 'Changes stay in this session.';
  final _editorFocus = FocusNode(debugLabel: 'Playground editor');

  Color get _foreground =>
      _light ? const RgbColor(25, 35, 45) : const RgbColor(220, 230, 240);
  Color get _background =>
      _light ? const RgbColor(250, 250, 252) : const RgbColor(18, 24, 32);
  Color get _secondary =>
      _light ? const RgbColor(93, 104, 115) : const RgbColor(155, 166, 178);
  Color get _border =>
      _light ? const RgbColor(190, 197, 204) : const RgbColor(70, 80, 92);
  List<Color> get _presets => [
    for (final preset in accentPresets) _light ? preset.light : preset.dark,
  ];
  Color get _link => _linkOverride ?? _presets[6];
  Color get _heading => _headingOverride ?? _presets[4];
  Color get _keyword => _keywordOverride ?? _presets[5];
  Color get _string => _presets[2];

  void _toggleTheme() => setState(() {
    final previous = _presets;
    _light = !_light;
    final next = _presets;
    Color? adapt(Color? color) {
      if (color == null) return null;
      final index = previous.indexOf(color);
      return index < 0 ? color : next[index];
    }

    _headingOverride = adapt(_headingOverride);
    _linkOverride = adapt(_linkOverride);
    _keywordOverride = adapt(_keywordOverride);
  });

  String _colorLabel(Color color, int index) {
    final preset = _presets.indexOf(color);
    return preset < 0 ? 'Custom color' : accentPresets[preset].name;
  }

  // Keep custom RGB choices visible without adding a third row. Substitute the
  // nearest preset; the remaining palette and # hex entry stay available.
  List<Color> _palette(Color current) {
    final colors = _presets;
    if (!colors.contains(current)) {
      final rgb = current.toRgb();
      int distance(Color color) {
        final other = color.toRgb();
        final r = rgb.r - other.r, g = rgb.g - other.g, b = rgb.b - other.b;
        return r * r + g * g + b * b;
      }

      var nearest = 0;
      for (var i = 1; i < colors.length; i++) {
        if (distance(colors[i]) < distance(colors[nearest])) nearest = i;
      }
      colors[nearest] = current;
    }
    return colors;
  }

  FlarkEditor get editor => widget.controller.editor;
  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_changed);
  }

  void _changed() => setState(() {});

  String get _configuration =>
      '''FlarkCellTheme(
  body: CellStyle(foreground: $_foreground, background: $_background),
  heading: CellStyle(foreground: $_heading, bold: true),
  link: CellStyle(foreground: $_link, underline: true),
  codePadding: 0,
  code: CellStyle(background: ${_light ? 'RgbColor(235, 239, 244)' : 'RgbColor(30, 38, 48)'}),
  syntax: {
    CodeSyntaxRole.keyword: CellStyle(foreground: $_keyword),
    CodeSyntaxRole.string: CellStyle(foreground: $_string),
    CodeSyntaxRole.comment: CellStyle(foreground: $_secondary),
  },
)''';

  @override
  Widget build(BuildContext context) {
    final theme = FlarkCellTheme(
      body: CellStyle(foreground: _foreground, background: _background),
      heading: CellStyle(foreground: _heading, bold: true),
      link: CellStyle(foreground: _link, underline: true),
      codePadding: 0,
      code: CellStyle(
        background: _light
            ? const RgbColor(235, 239, 244)
            : const RgbColor(30, 38, 48),
      ),
      syntax: {
        CodeSyntaxRole.keyword: CellStyle(foreground: _keyword),
        CodeSyntaxRole.string: CellStyle(foreground: _string),
        CodeSyntaxRole.comment: CellStyle(foreground: _secondary),
      },
    );
    final data = ThemeData(
      brightness: _light ? Brightness.light : Brightness.dark,
      textStyle: CellStyle(foreground: _foreground, background: _background),
      mutedStyle: CellStyle(foreground: _secondary),
      colorScheme: ColorScheme(
        foreground: _foreground,
        background: _background,
      ),
      extensions: [theme],
    );
    Widget colorSection(
      String title,
      String label,
      Color value,
      void Function(Color) onChanged,
    ) => Padding(
      padding: const EdgeInsets.only(bottom: 1),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title),
          ColorPicker(
            columns: 8,
            swatchWidth: 1,
            rowSpacing: 1,
            showHelp: false,
            value: value,
            colors: _palette(value),
            semanticLabel: label,
            semanticColorLabelBuilder: _colorLabel,
            onChanged: onChanged,
          ),
        ],
      ),
    );
    final controls = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          border: BoxBorder(cellStyle: CellStyle(foreground: _border)),
          padding: const EdgeInsets.symmetric(horizontal: 1),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('THEME', style: CellStyle(bold: true)),
              Button(
                label: _light ? 'Light / dark' : 'Dark / light',
                onPressed: _toggleTheme,
              ),
              const SizedBox(height: 1),
              colorSection(
                'Headings',
                'Heading color',
                _heading,
                (v) => setState(() => _headingOverride = v),
              ),
              colorSection(
                'Links',
                'Link color',
                _link,
                (v) => setState(() => _linkOverride = v),
              ),
              colorSection(
                'Code keywords',
                'Keyword color',
                _keyword,
                (v) => setState(() => _keywordOverride = v),
              ),
              Text(
                'Arrows: preview\nEnter: apply · # hex',
                style: CellStyle(foreground: _secondary),
              ),
              const SizedBox(height: 1),
              Button(
                label: 'Reset theme',
                onPressed: () => setState(() {
                  _linkOverride = _headingOverride = _keywordOverride = null;
                  _light = false;
                }),
              ),
              LayoutBuilder(
                builder: (context, _) => Button(
                  label: 'Copy theme Dart',
                  onPressed: () async {
                    await ClipboardScope.of(context).write(_configuration);
                    if (mounted) setState(() => _message = 'Theme copied.');
                  },
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 1),
        Button(
          label: editor.sourceMode ? 'Rendered mode' : 'Source mode',
          onPressed: () {
            editor.setSourceMode(!editor.sourceMode);
            _editorFocus.requestFocus();
          },
        ),
        Button(
          label: 'Reset sample',
          onPressed: () {
            editor.apply(ReplaceRange(0, editor.source.length, sample));
            editor.apply(const SetSelection.caret(0));
            _editorFocus.requestFocus();
          },
        ),
        const Text(''),
        Text(
          'Escape: leave editor\nTab: move through controls\nCode: Tab / Shift+Tab\nCmd/Ctrl+A: fence, then all',
          style: CellStyle(foreground: _secondary),
        ),
      ],
    );
    return FleuryApp(
      title: 'Flark · Fleury',
      theme: data,
      home: Container(
        color: _background,
        child: Column(
          children: [
            const Text(
              ' FLARK / FLEURY   ·   second-host playground',
              style: CellStyle(bold: true),
            ),
            Expanded(
              child: LayoutBuilder(
                builder: (context, constraints) =>
                    (constraints.maxCols ?? 80) < 64
                    ? Column(
                        children: [
                          Button(
                            label: _showTheme
                                ? 'Back to editor'
                                : 'Customize theme',
                            onPressed: () =>
                                setState(() => _showTheme = !_showTheme),
                          ),
                          Expanded(
                            child: _showTheme
                                ? ScrollView(
                                    child: Padding(
                                      padding: const EdgeInsets.all(1),
                                      child: controls,
                                    ),
                                  )
                                : Padding(
                                    padding: const EdgeInsets.all(1),
                                    child: FlarkEditorView(
                                      controller: widget.controller,
                                      onOpenLink: widget.onOpenLink,
                                      focusNode: _editorFocus,
                                      autofocus: true,
                                    ),
                                  ),
                          ),
                        ],
                      )
                    : Row(
                        children: [
                          if ((constraints.maxCols ?? 80) >= 64)
                            SizedBox(
                              width: 30,
                              child: ScrollView(
                                child: Padding(
                                  padding: const EdgeInsets.all(1),
                                  child: controls,
                                ),
                              ),
                            ),
                          Expanded(
                            child: Padding(
                              padding: const EdgeInsets.all(1),
                              child: FlarkEditorView(
                                controller: widget.controller,
                                onOpenLink: widget.onOpenLink,
                                focusNode: _editorFocus,
                                autofocus: true,
                              ),
                            ),
                          ),
                        ],
                      ),
              ),
            ),
            Text(
              ' ${editor.sourceMode ? 'SOURCE (bounded window)' : 'RENDERED'} · ${editor.source.length} characters · ${editor.selection} · ${widget.controller.highlightError == null ? _message : 'Code colors unavailable'}',
              style: CellStyle(foreground: _secondary),
            ),
          ],
        ),
      ),
    );
  }

  @override
  void dispose() {
    widget.controller.removeListener(_changed);
    _editorFocus.dispose();
    super.dispose();
  }
}

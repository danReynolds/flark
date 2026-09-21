import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
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

| Feature | Flutter | Fleury |
| :--- | :---: | ---: |
| **Tables** | Yes | Yes |
| Image previews | Yes | Yes |

![Flutter logo](demo.png "Click to open or edit this image")

---

Type here. Use the formatting bar or Markdown as you write.
''';

const headingSample = '''# A weekend by the lake

Two days away, a small cabin, and very little on the agenda. A few notes for the trip.


## Before you go

Leave Friday afternoon. Pick up groceries on the way and arrive before sunset.

### Packing light
- A warm layer for the evening
- A book you've been meaning to finish
- Coffee, breakfast, and something to share

### Finding the cabin
Take the second gravel road after the bridge. The blue door faces the lake.


## Once you arrive

Unpack, put the kettle on, and leave the rest for tomorrow.

### A slow Saturday
Walk along the shore in the morning. Lunch on the deck. **No reservations needed.**

#### If it rains
There are board games in the cupboard.

##### A note about the fireplace
Dry wood is stacked beside the back door.

###### Before heading home
Close the windows and leave the key on the hook.
''';

class Playground extends StatefulWidget {
  const Playground({
    super.key,
    required this.controller,
    this.onOpenLink,
    this.imagePreviewBuilder,
    this.baseUri,
  });
  final FlarkFleuryController controller;
  final void Function(Uri)? onOpenLink;
  final Uri? baseUri;
  final FlarkFleuryImagePreviewBuilder? imagePreviewBuilder;
  @override
  State<Playground> createState() => _PlaygroundState();
}

class _PlaygroundState extends State<Playground> {
  Color? _linkOverride, _headingOverride, _keywordOverride;
  bool _light = false, _showTheme = false;
  int _headingLevel = 1;
  bool _showHeadingStyles = false;
  bool _headingGutter = false;
  final _headingStyles = <int, FlarkHeadingStyle>{};
  final _headingColors = <int, Color>{};
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
    _headingColors.updateAll((_, color) => adapt(color)!);
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

  FlarkHeadingStyle get _levelStyle =>
      _headingStyles[_headingLevel] ??
      FlarkCellTheme.defaultHeadingStyles[_headingLevel]!;

  Color _headingColor(int level) =>
      _headingColors[level] ??
      switch (level) {
        1 => _heading,
        2 =>
          _light ? const RgbColor(70, 94, 123) : const RgbColor(176, 195, 220),
        6 => _secondary,
        _ => _foreground,
      };

  // Export the resolved palette too: copied Dart reproduces all six levels.
  Map<int, FlarkHeadingStyle> get _configuredHeadings => {
    for (var level = 1; level <= 6; level++) level: _configuredHeading(level),
  };

  FlarkHeadingStyle _configuredHeading(int level) {
    final style =
        _headingStyles[level] ?? FlarkCellTheme.defaultHeadingStyles[level]!;
    return FlarkHeadingStyle(
      style: style.style.copyWith(
        foreground: _headingColor(level),
        // An explicit muted color replaces terminal dimming in the example.
        dim: level == 6 ? false : null,
      ),
      band: style.band,
      divider: style.divider,
      showLevel: style.showLevel,
    );
  }

  void _changeHeading({
    bool? bold,
    bool? italic,
    bool? underline,
    bool? band,
    bool? divider,
    bool? showLevel,
  }) => setState(() {
    final current = _levelStyle;
    _headingStyles[_headingLevel] = FlarkHeadingStyle(
      style: current.style.copyWith(
        bold: bold,
        italic: italic,
        underline: underline,
      ),
      band: band ?? current.band,
      divider: divider ?? current.divider,
      showLevel: showLevel ?? current.showLevel,
    );
  });

  String get _headingConfiguration => _configuredHeadings.entries
      .map((entry) {
        final s = entry.value;
        final fields = [
          'foreground: ${s.style.foreground}',
          if (s.style.boldOrNull case final bold?) 'bold: $bold',
          if (s.style.italicOrNull case final italic?) 'italic: $italic',
          if (s.style.underlineOrNull case final underline?)
            'underline: $underline',
          if (s.style.dimOrNull case final dim?) 'dim: $dim',
        ].join(', ');
        return '    ${entry.key}: FlarkHeadingStyle(style: CellStyle($fields), band: ${s.band}, divider: ${s.divider}, showLevel: ${s.showLevel}),';
      })
      .join('\n');
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
  headingGutter: $_headingGutter,
  headingDivider: CellStyle(foreground: $_border),
  thematicBreak: CellStyle(foreground: $_border),
  headingIndicator: CellStyle(foreground: $_secondary),
  headingStyles: {
$_headingConfiguration
  },
  link: CellStyle(foreground: $_link, underline: true),
  codePadding: 1,
  codePaddingRows: 0.25,
  tableBorder: CellStyle(foreground: $_border),
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
      headingGutter: _headingGutter,
      headingStyles: _configuredHeadings,
      headingDivider: CellStyle(foreground: _border),
      thematicBreak: CellStyle(foreground: _border),
      headingIndicator: CellStyle(foreground: _secondary),
      link: CellStyle(foreground: _link, underline: true),
      codePadding: 1,
      codePaddingRows: 0.25,
      tableBorder: CellStyle(foreground: _border),
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
                text: _light ? 'Light / dark' : 'Dark / light',
                onPressed: _toggleTheme,
              ),
              const SizedBox(height: 1),
              colorSection(
                'Title / H1',
                'Heading color',
                _heading,
                (v) => setState(() => _headingOverride = v),
              ),
              Button(
                text: _showHeadingStyles
                    ? 'Hide heading styles'
                    : 'Heading styles',
                onPressed: () =>
                    setState(() => _showHeadingStyles = !_showHeadingStyles),
              ),
              if (_showHeadingStyles) ...[
                Button(
                  text: 'Heading level: H$_headingLevel',
                  onPressed: () =>
                      setState(() => _headingLevel = _headingLevel % 6 + 1),
                ),
                colorSection(
                  'H$_headingLevel color',
                  'Level color',
                  _headingColor(_headingLevel),
                  (v) => setState(() {
                    if (_headingLevel == 1) {
                      _headingOverride = v;
                    } else {
                      _headingColors[_headingLevel] = v;
                    }
                  }),
                ),
                Button(
                  text:
                      'Bold: ${_levelStyle.style.boldOrNull ?? true ? "on" : "off"}',
                  onPressed: () => _changeHeading(
                    bold: !(_levelStyle.style.boldOrNull ?? true),
                  ),
                ),
                Button(
                  text: 'Italic: ${_levelStyle.style.italic ? "on" : "off"}',
                  onPressed: () =>
                      _changeHeading(italic: !_levelStyle.style.italic),
                ),
                Button(
                  text:
                      'Underline: ${_levelStyle.style.underline ? "on" : "off"}',
                  onPressed: () =>
                      _changeHeading(underline: !_levelStyle.style.underline),
                ),
                Button(
                  text: 'Band: ${_levelStyle.band ? "on" : "off"}',
                  onPressed: () => _changeHeading(band: !_levelStyle.band),
                ),
                Button(
                  text: 'Divider: ${_levelStyle.divider ? "on" : "off"}',
                  onPressed: () =>
                      _changeHeading(divider: !_levelStyle.divider),
                ),
                Button(
                  text: 'Level label: ${_levelStyle.showLevel ? "on" : "off"}',
                  onPressed: () =>
                      _changeHeading(showLevel: !_levelStyle.showLevel),
                ),
                Button(
                  text: 'Level gutter: ${_headingGutter ? "on" : "off"}',
                  onPressed: () =>
                      setState(() => _headingGutter = !_headingGutter),
                ),
              ],
              const SizedBox(height: 1),
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
                text: 'Reset theme',
                onPressed: () => setState(() {
                  _linkOverride = _headingOverride = _keywordOverride = null;
                  _light = false;
                  _headingStyles.clear();
                  _headingColors.clear();
                  _headingGutter = false;
                }),
              ),
              LayoutBuilder(
                builder: (context, _) => Button(
                  text: 'Copy theme Dart',
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
          text: editor.sourceMode ? 'Rendered mode' : 'Source mode',
          onPressed: () {
            editor.setSourceMode(!editor.sourceMode);
            _editorFocus.requestFocus();
          },
        ),
        Button(
          text: 'Heading sample',
          onPressed: () {
            editor.apply(ReplaceRange(0, editor.source.length, headingSample));
            editor.apply(const SetSelection.caret(0));
            _editorFocus.requestFocus();
          },
        ),
        Button(
          text: 'Reset sample',
          onPressed: () {
            editor.apply(ReplaceRange(0, editor.source.length, sample));
            editor.apply(const SetSelection.caret(0));
            _editorFocus.requestFocus();
          },
        ),
        const Text(''),
        Text(
          'Escape: leave editor\nTab: move through controls\nCode: Tab / Shift+Tab\nTables: Tab between cells\nCmd/Ctrl+A: fence, then all',
          style: CellStyle(foreground: _secondary),
        ),
      ],
    );
    final customization = ScrollView(
      child: Padding(padding: const EdgeInsets.all(1), child: controls),
    );
    return FleuryApp(
      title: 'Flark composer · Fleury',
      theme: data,
      home: Container(
        color: _background,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 1),
              child: Wrap(
                spacing: 2,
                children: [
                  const Text(
                    'FLARK  /  Untitled document',
                    style: CellStyle(bold: true),
                  ),
                  Button(
                    text: 'New document',
                    onPressed: () {
                      editor.apply(ReplaceRange(0, editor.source.length, ''));
                      _editorFocus.requestFocus();
                    },
                  ),
                  Button(
                    text: _showTheme ? 'Close theme' : 'Customize theme',
                    onPressed: () => setState(() => _showTheme = !_showTheme),
                  ),
                  LayoutBuilder(
                    builder: (context, _) => Button(
                      text: 'Copy Markdown',
                      onPressed: () async {
                        await ClipboardScope.of(context).write(editor.source);
                        if (mounted) {
                          setState(() => _message = 'Markdown copied.');
                        }
                      },
                    ),
                  ),
                ],
              ),
            ),
            Expanded(
              child: LayoutBuilder(
                builder: (context, constraints) {
                  final wide = (constraints.maxCols ?? 80) >= 90;
                  return Column(
                    children: [
                      // Keep the document mounted when opening settings so its
                      // selection, viewport and input connection survive.
                      SizedBox(
                        height: _showTheme && !wide
                            ? ((constraints.maxRows ?? 30) ~/ 3).clamp(1, 18)
                            : 0,
                        child: _showTheme && !wide
                            ? customization
                            : const SizedBox(),
                      ),
                      Expanded(
                        child: Row(
                          children: [
                            Expanded(
                              child: Padding(
                                padding: const EdgeInsets.all(1),
                                child: Container(
                                  border: BoxBorder(
                                    cellStyle: CellStyle(foreground: _border),
                                  ),
                                  padding: const EdgeInsets.symmetric(
                                    horizontal: 2,
                                    vertical: 1,
                                  ),
                                  child: FlarkEditorView(
                                    controller: widget.controller,
                                    showToolbar: !(_showTheme && !wide),
                                    onOpenLink: widget.onOpenLink,
                                    baseUri: widget.baseUri,
                                    imagePreviewBuilder:
                                        widget.imagePreviewBuilder,
                                    focusNode: _editorFocus,
                                    autofocus: true,
                                    onNotice: (notice) =>
                                        setState(() => _message = notice),
                                  ),
                                ),
                              ),
                            ),
                            if (_showTheme && wide)
                              SizedBox(width: 32, child: customization),
                          ],
                        ),
                      ),
                    ],
                  );
                },
              ),
            ),
            Text(
              ' ${editor.sourceMode ? 'SOURCE' : 'MARKDOWN'} · ${editor.source.length} characters · ${widget.controller.highlightError == null ? _message : 'Code colors unavailable'}',
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

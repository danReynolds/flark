import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart';

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
  Color _link = Colors.cyan, _heading = Colors.cyan, _keyword = Colors.magenta;
  bool _light = false, _showTheme = false;
  String _message = 'Changes stay in this session.';
  final _editorFocus = FocusNode(debugLabel: 'Playground editor');

  FlarkEditor get editor => widget.controller.editor;
  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_changed);
  }

  void _changed() => setState(() {});

  String get _configuration =>
      '''FlarkCellTheme(
  heading: CellStyle(foreground: $_heading, bold: true),
  link: CellStyle(foreground: $_link, underline: true),
  code: CellStyle(background: ${_light ? 'RgbColor(235, 239, 244)' : 'RgbColor(30, 38, 48)'}),
  syntax: {
    'keyword': CellStyle(foreground: $_keyword),
    'string': CellStyle(foreground: Colors.green),
    'comment': CellStyle(dim: true),
  },
)''';

  @override
  Widget build(BuildContext context) {
    final theme = FlarkCellTheme(
      heading: CellStyle(foreground: _heading, bold: true),
      link: CellStyle(foreground: _link, underline: true),
      code: CellStyle(
        background: _light
            ? const RgbColor(235, 239, 244)
            : const RgbColor(30, 38, 48),
      ),
      syntax: {
        'keyword': CellStyle(foreground: _keyword),
        'string': const CellStyle(foreground: Colors.green),
        'comment': const CellStyle(dim: true),
      },
    );
    final data = ThemeData(
      brightness: _light ? Brightness.light : Brightness.dark,
      colorScheme: ColorScheme(
        foreground: _light
            ? const RgbColor(25, 35, 45)
            : const RgbColor(220, 230, 240),
        background: _light
            ? const RgbColor(250, 250, 252)
            : const RgbColor(18, 24, 32),
      ),
      extensions: [theme],
    );
    final controls = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('THEME', style: CellStyle(bold: true)),
        Button(
          label: _light ? 'Light / dark' : 'Dark / light',
          onPressed: () => setState(() => _light = !_light),
        ),
        const Text('Headings'),
        SizedBox(
          height: 5,
          child: ColorPicker(
            columns: 8,
            swatchWidth: 1,
            value: _heading,
            semanticLabel: 'Heading color',
            onChanged: (v) => setState(() => _heading = v),
          ),
        ),
        const Text('Links'),
        SizedBox(
          height: 5,
          child: ColorPicker(
            columns: 8,
            swatchWidth: 1,
            value: _link,
            semanticLabel: 'Link color',
            onChanged: (v) => setState(() => _link = v),
          ),
        ),
        const Text('Code keywords'),
        SizedBox(
          height: 5,
          child: ColorPicker(
            columns: 8,
            swatchWidth: 1,
            value: _keyword,
            semanticLabel: 'Keyword color',
            onChanged: (v) => setState(() => _keyword = v),
          ),
        ),
        Button(
          label: 'Reset theme',
          onPressed: () => setState(() {
            _link = Colors.cyan;
            _heading = Colors.cyan;
            _keyword = Colors.magenta;
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
        const Text(''),
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
        const Text(
          'Escape: leave editor\nTab: move through controls\nCode: Tab / Shift+Tab\nCmd/Ctrl+A: fence, then all',
          style: CellStyle(dim: true),
        ),
      ],
    );
    return FleuryApp(
      title: 'Flark · Fleury',
      theme: data,
      home: Column(
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
                            width: 29,
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
            style: const CellStyle(dim: true),
          ),
        ],
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

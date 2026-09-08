import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/code.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:url_launcher/url_launcher.dart';
import 'custom_controls.dart';
import 'theme_color_control.dart';
import 'theme_settings.dart';

const themeSample = '''# Make it yours

Click [this link](https://docs.flutter.dev/) to try the controls. Keep writing in **bold**, *italic*, ~~struck~~ or `inline code`.

## A little structure

> A quote with a [**bold link**](https://dart.dev/).

- A thought to keep
- [ ] Something to try
- [x] Something finished

| Setting | Result |
| --- | --- |
| Theme | Yours |
| Markdown | Portable |

```dart
void greet(String name) {
  // A small snippet
  print('Hello, \$name');
}
```

```ruby
def greet(name)
  puts "Hello, #{name}"
end
```

![Demo image](/demo.png)

![Unavailable example](/missing.png)

---

### Keep experimenting

Theme changes preserve your text and undo history.
''';

ImageProvider<Object>? demoImageProvider(Uri uri) =>
    uri.path == '/demo.png' ? const AssetImage('assets/demo.png') : null;

class ThemePlayground extends StatefulWidget {
  const ThemePlayground({super.key, required this.backend, this.code});
  final FlarkParseBackend backend;
  final FlarkTreeSitter? code;
  @override
  State<ThemePlayground> createState() => _ThemePlaygroundState();
}

class _ThemePlaygroundState extends State<ThemePlayground> {
  final settings = ThemeSettings();
  late final FlarkController editor;
  var section = 'Appearance', resetEpoch = 0;
  var textRole = FlarkTextRole.link, colorRole = FlarkColorRole.canvas;
  var metricRole = FlarkMetric.documentPadding,
      syntaxRole = FlarkSyntaxRole.keyword;
  late final Future<String> controlsSource;

  FlarkController makeController() {
    final e = FlarkEditor(
      widget.backend,
      text: themeSample,
      codeEditing: widget.code,
    );
    return FlarkController(
      e,
      codeColors: widget.code == null ? null : FlarkCodeColors(e),
    );
  }

  @override
  void initState() {
    super.initState();
    editor = makeController();
    controlsSource = rootBundle.loadString('lib/custom_controls.dart');
  }

  @override
  void dispose() {
    editor.dispose();
    super.dispose();
  }

  void change(VoidCallback update) => setState(update);
  Future<void> openLink(Uri uri) async {
    final opened = await launchUrl(uri, mode: LaunchMode.externalApplication);
    if (!opened && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Could not open this link.')),
      );
    }
  }

  Future<void> configuration(BuildContext context, {bool copy = false}) async {
    final customSource = await controlsSource;
    if (!context.mounted) return;
    final code = settings.exportDart(customControlsSource: customSource);
    if (copy) {
      await Clipboard.setData(ClipboardData(text: code));
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Dart configuration copied.')),
        );
      }
      return;
    }
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Your Dart configuration'),
        content: SizedBox(
          width: 720,
          child: SingleChildScrollView(
            child: SelectableText(
              code,
              style: const TextStyle(
                fontFamily: flarkCodeFontFamily,
                fontSize: 12,
              ),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Close'),
          ),
          FilledButton(
            onPressed: () => Clipboard.setData(ClipboardData(text: code)),
            child: const Text('Copy Dart'),
          ),
        ],
      ),
    );
  }

  String label(Enum role) => role.name.replaceAllMapped(
    RegExp(r'[A-Z]'),
    (m) => ' ${m[0]!.toLowerCase()}',
  );

  Widget dropdown<T>(
    String title,
    T value,
    List<T> values,
    ValueChanged<T> changed,
    String Function(T) name,
  ) => DropdownButtonFormField<T>(
    key: ValueKey((title, value)),
    initialValue: value,
    isExpanded: true,
    decoration: InputDecoration(labelText: title),
    items: values
        .map((v) => DropdownMenuItem(value: v, child: Text(name(v))))
        .toList(),
    onChanged: (value) {
      if (value != null) changed(value);
    },
  );

  Widget colorInput(String title, Color color, ValueChanged<Color> changed) =>
      ThemeColorControl(
        key: ValueKey((title, resetEpoch)),
        title: title,
        color: color,
        onChanged: changed,
      );

  Widget slider(
    String title,
    double value,
    double min,
    double max,
    ValueChanged<double> changed,
  ) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text('$title · ${value.toStringAsFixed(1)}'),
      Slider(
        label: value.toStringAsFixed(1),
        value: value.clamp(min, max),
        min: min,
        max: max,
        onChanged: changed,
      ),
    ],
  );

  Widget controls(BuildContext context) {
    final resolved = FlarkThemeData.resolve(
      context,
      overrides: settings.perInstance ? settings.theme : null,
    );
    final style = settings.styles[textRole] ?? const TextStyle();
    final baseStyle =
        resolved.styles[textRole] ?? resolved.styles[FlarkTextRole.body]!;
    void setStyle(TextStyle next) =>
        change(() => settings.styles[textRole] = next);
    return ListView(
      padding: const EdgeInsets.all(20),
      children: [
        Text('Your theme', style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 6),
        const Text(
          'Change a setting, then try it in the document. Your writing stays put.',
        ),
        const SizedBox(height: 20),
        dropdown(
          'Customize',
          section,
          ['Appearance', 'Typography', 'Blocks', 'Syntax colors', 'Controls'],
          (v) => change(() {
            section = v;
            resetEpoch++;
          }),
          (v) => v,
        ),
        const SizedBox(height: 20),
        if (section == 'Appearance') ...[
          const Text('Start with a preset'),
          const SizedBox(height: 8),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: ['Light', 'Dark', 'Notebook']
                .map(
                  (name) => OutlinedButton(
                    onPressed: () => change(() {
                      settings.preset(name);
                      resetEpoch++;
                    }),
                    child: Text(name),
                  ),
                )
                .toList(),
          ),
          const SizedBox(height: 12),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Dark mode'),
            value: settings.brightness == Brightness.dark,
            onChanged: (v) => change(() {
              settings.brightness = v ? Brightness.dark : Brightness.light;
              resetEpoch++;
            }),
          ),
          colorInput(
            'App accent',
            settings.seed,
            (c) => change(() => settings.seed = c),
          ),
          colorInput(
            'Link color',
            resolved.styles[FlarkTextRole.link]!.color!,
            (c) => change(
              () => settings.styles[FlarkTextRole.link] =
                  (settings.styles[FlarkTextRole.link] ?? const TextStyle())
                      .copyWith(color: c),
            ),
          ),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Per-editor overrides'),
            subtitle: const Text(
              'Off: inherit the application theme. On: pass it directly to this editor.',
            ),
            value: settings.perInstance,
            onChanged: (v) => change(() => settings.perInstance = v),
          ),
        ],
        if (section == 'Typography') ...[
          dropdown(
            'Text style',
            textRole,
            FlarkTextRole.values,
            (v) => change(() {
              textRole = v;
              resetEpoch++;
            }),
            label,
          ),
          colorInput(
            '${label(textRole)} color',
            baseStyle.color ?? resolved.styles[FlarkTextRole.body]!.color!,
            (c) => setStyle(style.copyWith(color: c)),
          ),
          slider(
            'Font size',
            style.fontSize ?? baseStyle.fontSize ?? 16,
            10,
            40,
            (v) => setStyle(style.copyWith(fontSize: v)),
          ),
          slider(
            'Line height',
            style.height ?? baseStyle.height ?? 1.45,
            1,
            2.2,
            (v) => setStyle(style.copyWith(height: v)),
          ),
          dropdown(
            'Font family',
            style.fontFamily ?? 'Inherit',
            ['Inherit', 'Georgia', flarkCodeFontFamily],
            (v) => setStyle(
              TextStyle(
                color: style.color,
                fontSize: style.fontSize,
                height: style.height,
                fontFamily: v == 'Inherit' ? null : v,
                fontWeight: style.fontWeight,
                fontStyle: style.fontStyle,
                decoration: style.decoration,
              ),
            ),
            (v) => v == flarkCodeFontFamily ? 'Monospace' : v,
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            children: [
              FilterChip(
                label: const Text('Bold'),
                selected:
                    (style.fontWeight ?? baseStyle.fontWeight) ==
                    FontWeight.w700,
                onSelected: (v) => setStyle(
                  style.copyWith(
                    fontWeight: v ? FontWeight.w700 : FontWeight.w400,
                  ),
                ),
              ),
              FilterChip(
                label: const Text('Italic'),
                selected:
                    (style.fontStyle ?? baseStyle.fontStyle) ==
                    FontStyle.italic,
                onSelected: (v) => setStyle(
                  style.copyWith(
                    fontStyle: v ? FontStyle.italic : FontStyle.normal,
                  ),
                ),
              ),
              FilterChip(
                label: const Text('Underline'),
                selected:
                    (style.decoration ?? baseStyle.decoration)?.contains(
                      TextDecoration.underline,
                    ) ==
                    true,
                onSelected: (v) => setStyle(
                  style.copyWith(
                    decoration: v
                        ? TextDecoration.underline
                        : TextDecoration.none,
                  ),
                ),
              ),
            ],
          ),
          TextButton(
            onPressed: () => change(() {
              settings.styles.remove(textRole);
              resetEpoch++;
            }),
            child: const Text('Inherit this text style'),
          ),
        ],
        if (section == 'Blocks') ...[
          dropdown(
            'Decoration',
            colorRole,
            FlarkColorRole.values,
            (v) => change(() {
              colorRole = v;
              resetEpoch++;
            }),
            label,
          ),
          colorInput(
            '${label(colorRole)} color',
            resolved.colors[colorRole]!,
            (c) => change(() => settings.colors[colorRole] = c),
          ),
          const SizedBox(height: 12),
          dropdown(
            'Spacing or shape',
            metricRole,
            FlarkMetric.values,
            (v) => change(() => metricRole = v),
            label,
          ),
          const SizedBox(height: 12),
          slider(
            label(metricRole),
            resolved.metrics[metricRole]!,
            metricRole == FlarkMetric.imageHeight ||
                    metricRole == FlarkMetric.imageMaxWidth
                ? 40
                : 0,
            metricRole == FlarkMetric.imageMaxWidth
                ? 640
                : metricRole == FlarkMetric.imageHeight
                ? 320
                : 48,
            (v) => change(() => settings.metrics[metricRole] = v),
          ),
          TextButton(
            onPressed: () => change(() {
              settings.colors.remove(colorRole);
              settings.metrics.remove(metricRole);
              resetEpoch++;
            }),
            child: const Text('Inherit these block settings'),
          ),
        ],
        if (section == 'Syntax colors') ...[
          const Text(
            'Scroll to the Dart and Ruby snippets. Token colors can change without changing code geometry.',
          ),
          const SizedBox(height: 12),
          dropdown(
            'Token',
            syntaxRole,
            FlarkSyntaxRole.values,
            (v) => change(() {
              syntaxRole = v;
              resetEpoch++;
            }),
            label,
          ),
          colorInput(
            '${label(syntaxRole)} color',
            resolved.syntaxColors[syntaxRole]!,
            (c) => change(() => settings.syntaxColors[syntaxRole] = c),
          ),
          TextButton(
            onPressed: () => change(() {
              settings.syntaxColors.remove(syntaxRole);
              resetEpoch++;
            }),
            child: const Text('Inherit this token color'),
          ),
        ],
        if (section == 'Controls') ...[
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Custom link controls'),
            subtitle: const Text(
              'A branded popover and an editing sheet, built with public guarded actions.',
            ),
            value: settings.customControls,
            onChanged: (v) => change(() => settings.customControls = v),
          ),
          const SizedBox(height: 12),
          const Text(
            'Click a link in the editor. Try Open, Edit and Remove, then Undo and keep typing. Cmd/Ctrl-K edits the link at your caret.',
          ),
          const SizedBox(height: 12),
          const Text(
            'The configuration includes the complete custom-control source when this option is on.',
          ),
        ],
      ],
    );
  }

  Widget pane(String title, Widget child) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Text(title, style: const TextStyle(fontWeight: FontWeight.w600)),
      ),
      const Divider(height: 1),
      Expanded(child: child),
    ],
  );

  Widget liveEditor() => pane(
    'Live editor',
    FlarkEditorWidget(
      key: const ValueKey('playground-editor'),
      controller: editor,
      theme: settings.perInstance ? settings.theme : null,
      baseUri: Uri.parse('https://example.com/'),
      imageProvider: demoImageProvider,
      onOpenLink: openLink,
      linkPopoverBuilder: settings.customControls ? brandedLinkPopover : null,
      presentResourceEditor: settings.customControls ? showResourceSheet : null,
    ),
  );

  @override
  Widget build(BuildContext context) => Theme(
    data: settings.material,
    child: Builder(
      builder: (context) => Scaffold(
        appBar: AppBar(
          title: const Text('Theme playground'),
          actions: [
            IconButton(
              tooltip: 'Reset theme',
              onPressed: () => change(() {
                settings.preset('Light');
                resetEpoch++;
              }),
              icon: const Icon(Icons.restart_alt),
            ),
            IconButton(
              tooltip: 'Reset sample',
              onPressed: () => editor.command(
                ReplaceRange(0, editor.text.length, themeSample),
              ),
              icon: const Icon(Icons.restore_page),
            ),
            IconButton(
              tooltip: 'View Dart configuration',
              onPressed: () => configuration(context),
              icon: const Icon(Icons.code),
            ),
            IconButton(
              tooltip: 'Copy Dart configuration',
              onPressed: () => configuration(context, copy: true),
              icon: const Icon(Icons.copy),
            ),
          ],
        ),
        body: LayoutBuilder(
          builder: (context, constraints) {
            if (constraints.maxWidth >= 760) {
              return Row(
                children: [
                  SizedBox(width: 300, child: controls(context)),
                  const VerticalDivider(width: 1),
                  Expanded(child: liveEditor()),
                ],
              );
            }
            return Column(
              children: [
                SizedBox(height: 260, child: controls(context)),
                const Divider(height: 1),
                Expanded(child: liveEditor()),
              ],
            );
          },
        ),
      ),
    ),
  );
}

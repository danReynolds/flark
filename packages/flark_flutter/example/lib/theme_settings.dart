import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';

/// Example application state; all rendering still uses the public theme API.
class ThemeSettings {
  Brightness brightness = Brightness.light;
  Color seed = const Color(0xff3a628f);
  bool customControls = false, perInstance = false;
  final styles = <FlarkTextRole, TextStyle>{};
  final colors = <FlarkColorRole, Color>{};
  final metrics = <FlarkMetric, double>{};
  final syntaxColors = <FlarkSyntaxRole, Color>{};
  FlarkThemeData get theme => FlarkThemeData(
    styles: styles,
    colors: colors,
    metrics: metrics,
    syntaxColors: syntaxColors,
  );
  ThemeData get material => ThemeData(
    colorScheme: ColorScheme.fromSeed(seedColor: seed, brightness: brightness),
    extensions: perInstance ? [] : [theme],
  );
  void preset(String name) {
    styles.clear();
    colors.clear();
    metrics.clear();
    syntaxColors.clear();
    brightness = name == 'Dark' ? Brightness.dark : Brightness.light;
    seed = name == 'Notebook'
        ? const Color(0xff386347)
        : const Color(0xff3a628f);
    customControls = name == 'Notebook';
    perInstance = false;
    if (name == 'Notebook') {
      styles[FlarkTextRole.body] = const TextStyle(
        fontFamily: 'Georgia',
        fontSize: 19,
        height: 1.6,
      );
      metrics[FlarkMetric.documentPadding] = 24;
      metrics[FlarkMetric.headingSpacing] = 18;
    }
  }

  /// A drop-in widget with explicit controller ownership at the app boundary.
  /// Include the actual custom-controls source when that presentation is chosen.
  String exportDart({String customControlsSource = ''}) {
    String color(Color c) =>
        'const Color(0x${c.toARGB32().toRadixString(16).padLeft(8, '0')})';
    String quote(String s) =>
        "'${s.replaceAll('\\', '\\\\').replaceAll("'", "\\'").replaceAll('\n', '\\n').replaceAll(r'$', r'\$')}'";
    String style(TextStyle s) =>
        'const TextStyle(${[if (s.color != null) 'color: ${color(s.color!).replaceFirst('const ', '')}', if (s.backgroundColor != null) 'backgroundColor: ${color(s.backgroundColor!).replaceFirst('const ', '')}', if (s.fontSize != null) 'fontSize: ${s.fontSize}', if (s.height != null) 'height: ${s.height}', if (s.fontFamily != null) 'fontFamily: ${quote(s.fontFamily!)}', if (s.fontWeight != null) 'fontWeight: FontWeight.w${s.fontWeight!.value}', if (s.fontStyle != null) 'fontStyle: FontStyle.${s.fontStyle!.name}', if (s.letterSpacing != null) 'letterSpacing: ${s.letterSpacing}', if (s.decoration != null) 'decoration: ${s.decoration == TextDecoration.none
              ? 'TextDecoration.none'
              : s.decoration == TextDecoration.underline
              ? 'TextDecoration.underline'
              : 'TextDecoration.lineThrough'}'].join(', ')})';
    final themeCode =
        '''FlarkThemeData(
      styles: {${styles.entries.map((e) => '\n        FlarkTextRole.${e.key.name}: ${style(e.value)},').join()}\n      },
      colors: {${colors.entries.map((e) => '\n        FlarkColorRole.${e.key.name}: ${color(e.value)},').join()}\n      },
      metrics: {${metrics.entries.map((e) => '\n        FlarkMetric.${e.key.name}: ${e.value},').join()}\n      },
      syntaxColors: {${syntaxColors.entries.map((e) => '\n        FlarkSyntaxRole.${e.key.name}: ${color(e.value)},').join()}\n      },
    )''';
    return '''import 'package:flutter/material.dart';
import 'package:flark_flutter/flark_flutter.dart';

// Supply the editor controller from your application.
// See the package example's backend.dart for native/web parser initialization.
class ConfiguredMarkdown extends StatelessWidget {
  const ConfiguredMarkdown({super.key, required this.editor, this.onOpenLink});
  final FlarkController editor;
  final ValueChanged<Uri>? onOpenLink;
  @override
  Widget build(BuildContext context) {
    final markdown = $themeCode;
    final material = ThemeData(
      colorScheme: ColorScheme.fromSeed(seedColor: ${color(seed)}, brightness: Brightness.${brightness.name}),
      extensions: ${perInstance ? '[]' : '[markdown]'},
    );
    return Theme(data: material, child: FlarkEditorWidget(controller: editor,
        theme: ${perInstance ? 'markdown' : 'null'}, onOpenLink: onOpenLink,
        ${customControls ? 'linkPopoverBuilder: brandedLinkPopover,\n        presentResourceEditor: showResourceSheet,' : ''}
    ));
  }
}
${customControls ? customControlsSource.split('\n').where((line) => !line.startsWith('import ')).join('\n') : ''}
''';
  }
}

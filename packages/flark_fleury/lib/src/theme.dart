import 'package:fleury/fleury_core.dart';

/// Plain cell styles, installed in ThemeData.extensions or on one editor.
/// Null terminal foreground/background are preserved by the default theme.
final class FlarkCellTheme {
  const FlarkCellTheme({
    this.body = CellStyle.none,
    this.heading = const CellStyle(bold: true),
    this.marker = CellStyle.none,
    this.quote = const CellStyle(italic: true),
    this.link = const CellStyle(underline: true),
    this.code = CellStyle.none,
    this.selection = const CellStyle(inverse: true),
    this.caret = const CellStyle(inverse: true),
    this.syntax = const {},
  });

  final CellStyle body, heading, marker, quote, link, code, selection, caret;
  final Map<String, CellStyle> syntax;

  static FlarkCellTheme of(BuildContext context) {
    final theme = Theme.of(context);
    return theme.extension<FlarkCellTheme>() ??
        FlarkCellTheme(
          body: CellStyle(
            foreground: theme.colorScheme.foreground,
            background: theme.colorScheme.background,
          ).merge(theme.textStyle),
          heading: CellStyle(foreground: theme.colorScheme.primary, bold: true),
          link: CellStyle(foreground: theme.colorScheme.info, underline: true),
          syntax: {
            'keyword': CellStyle(foreground: theme.colorScheme.primary),
            'string': CellStyle(foreground: theme.colorScheme.success),
            'number': CellStyle(foreground: theme.colorScheme.warning),
            'function': CellStyle(foreground: theme.colorScheme.info),
            'comment': theme.mutedStyle,
          },
        );
  }
}

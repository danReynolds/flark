import 'package:flark/code.dart';
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
  /// Keyed by presentation role, not by the analyzer's scope names: keying
  /// on raw scopes left `built_in`, `regexp`, `tag` and a dozen others
  /// rendering as body text.
  final Map<CodeSyntaxRole, CellStyle> syntax;

  /// A value type: the host rebuilds one per frame when no theme extension is
  /// installed, and the cell layout reuses its geometry only while the theme it
  /// was built with still compares equal.
  @override
  bool operator ==(Object other) =>
      other is FlarkCellTheme &&
      other.body == body &&
      other.heading == heading &&
      other.marker == marker &&
      other.quote == quote &&
      other.link == link &&
      other.code == code &&
      other.selection == selection &&
      other.caret == caret &&
      _sameSyntax(other.syntax);

  bool _sameSyntax(Map<CodeSyntaxRole, CellStyle> other) {
    if (other.length != syntax.length) return false;
    for (final entry in syntax.entries) {
      if (other[entry.key] != entry.value) return false;
    }
    return true;
  }

  @override
  int get hashCode => Object.hash(
    body,
    heading,
    marker,
    quote,
    link,
    code,
    selection,
    caret,
    syntax.length,
  );

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
            CodeSyntaxRole.keyword: CellStyle(
              foreground: theme.colorScheme.primary,
            ),
            CodeSyntaxRole.string: CellStyle(
              foreground: theme.colorScheme.success,
            ),
            CodeSyntaxRole.number: CellStyle(
              foreground: theme.colorScheme.warning,
            ),
            CodeSyntaxRole.function: CellStyle(
              foreground: theme.colorScheme.info,
            ),
            CodeSyntaxRole.variable: CellStyle(
              foreground: theme.colorScheme.foreground,
            ),
            CodeSyntaxRole.comment: theme.mutedStyle,
          },
        );
  }
}

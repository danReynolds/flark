import 'package:flark/code.dart';
import 'package:fleury/fleury_core.dart';

/// Presentation of one heading level. Decorations never become source text.
final class FlarkHeadingStyle {
  const FlarkHeadingStyle({
    this.style = CellStyle.none,
    this.band = false,
    this.divider = false,
    this.showLevel = false,
  });

  final CellStyle style;
  final bool band, divider, showLevel;

  @override
  bool operator ==(Object other) =>
      other is FlarkHeadingStyle &&
      other.style == style &&
      other.band == band &&
      other.divider == divider &&
      other.showLevel == showLevel;

  @override
  int get hashCode => Object.hash(style, band, divider, showLevel);
}

/// Plain cell styles, installed in ThemeData.extensions or on one editor.
/// Null terminal foreground/background are preserved by the default theme.
final class FlarkCellTheme {
  const FlarkCellTheme({
    this.body = CellStyle.none,
    this.heading = const CellStyle(bold: true),
    this.headingStyles = const {},
    this.headingBand,
    this.headingDivider = CellStyle.none,
    this.thematicBreak = CellStyle.none,
    this.headingIndicator = CellStyle.none,
    this.headingGutter = false,
    this.marker = CellStyle.none,
    this.quote = const CellStyle(italic: true),
    this.link = const CellStyle(underline: true),
    this.code = CellStyle.none,
    this.codePadding = 2,
    this.codePaddingRows = 0,
    this.listIndent = 4,
    this.quoteIndent = 2,
    this.tableBorder = CellStyle.none,
    this.tableHeader = const CellStyle(bold: true),
    this.imagePreviewRows = 8,
    this.selection = const CellStyle(inverse: true),
    this.caret = const CellStyle(inverse: true),
    this.syntax = const {},
  }) : assert(codePadding >= 0),
       assert(codePaddingRows >= 0 && codePaddingRows <= 1),
       assert(listIndent >= 2),
       assert(quoteIndent >= 1),
       assert(imagePreviewRows >= 0);

  final CellStyle body, heading, marker, quote, link, code, selection, caret;

  /// Overrides keyed by level 1–6. Missing entries use [defaultHeadingStyles].
  /// [heading] is the common text style beneath these per-level overrides.
  final Map<int, FlarkHeadingStyle> headingStyles;
  static const defaultHeadingStyles = {
    1: FlarkHeadingStyle(),
    2: FlarkHeadingStyle(),
    3: FlarkHeadingStyle(style: CellStyle(italic: true)),
    4: FlarkHeadingStyle(style: CellStyle(bold: false, italic: true)),
    5: FlarkHeadingStyle(style: CellStyle(bold: false)),
    6: FlarkHeadingStyle(style: CellStyle(bold: false, dim: true)),
  };

  FlarkHeadingStyle headingFor(int level) =>
      headingStyles[level] ??
      defaultHeadingStyles[level] ??
      const FlarkHeadingStyle();

  /// A null band derives a subtle fill from body colors, or uses reverse video
  /// when terminal colors are unknown. Divider and indicator styles are separate
  /// from heading text so they can be subdued without reducing text contrast.
  final CellStyle? headingBand;
  final CellStyle headingDivider, headingIndicator;

  /// The horizontal rule painted for a Markdown thematic break.
  final CellStyle thematicBreak;

  /// Reserve three cells beside the whole document for H1–H6 indicators.
  /// Text and containing block edges remain aligned. Under eight columns this
  /// falls back to inline indicators to keep room for editing.
  final bool headingGutter;

  CellStyle headingSurface(int level) {
    if (!headingFor(level).band) return body;
    if (headingBand != null) return body.merge(headingBand!);
    final bg = body.background?.toRgb(), fg = body.foreground?.toRgb();
    if (bg == null || fg == null) {
      return body.merge(const CellStyle(inverse: true));
    }
    int mix(int a, int b) => (a * .92 + b * .08).round();
    return body.merge(
      CellStyle(
        background: RgbColor(mix(bg.r, fg.r), mix(bg.g, fg.g), mix(bg.b, fg.b)),
      ),
    );
  }

  /// Leading cells inside a fenced code surface, independent of source indent.
  /// Text stays on whole cells, so fractional values round up. The background
  /// starts at the containing block's edge, independently of this padding.
  final num codePadding;
  int get codePaddingColumns => codePadding.ceil();

  /// Painted padding above/below code, in eighth-row increments. Each nonzero
  /// edge reserves a decorative row; its unpainted part is outside the block.
  final num codePaddingRows;

  /// Minimum list content indent, including the marker and a one-cell gap.
  /// Numbered lists expand their gutter together when a label needs more room.
  final int listIndent;

  /// Quote rail plus the gap before quoted content, in cells.
  final int quoteIndent;
  final CellStyle tableBorder, tableHeader;

  /// Fixed preview height in cells. Zero keeps only editable alt text.
  final int imagePreviewRows;

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
      other.headingBand == headingBand &&
      other.headingDivider == headingDivider &&
      other.thematicBreak == thematicBreak &&
      other.headingIndicator == headingIndicator &&
      other.headingGutter == headingGutter &&
      other.headingStyles.length == headingStyles.length &&
      headingStyles.entries.every(
        (e) => other.headingStyles[e.key] == e.value,
      ) &&
      other.marker == marker &&
      other.quote == quote &&
      other.link == link &&
      other.code == code &&
      other.codePadding == codePadding &&
      other.codePaddingRows == codePaddingRows &&
      other.listIndent == listIndent &&
      other.quoteIndent == quoteIndent &&
      other.tableBorder == tableBorder &&
      other.tableHeader == tableHeader &&
      other.imagePreviewRows == imagePreviewRows &&
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
  int get hashCode => Object.hashAll([
    body,
    heading,
    headingBand,
    headingDivider,
    thematicBreak,
    headingIndicator,
    headingGutter,
    Object.hashAllUnordered(
      headingStyles.entries.map((e) => Object.hash(e.key, e.value)),
    ),
    marker,
    quote,
    link,
    code,
    codePadding,
    codePaddingRows,
    listIndent,
    quoteIndent,
    tableBorder,
    tableHeader,
    imagePreviewRows,
    selection,
    caret,
    syntax.length,
  ]);

  static FlarkCellTheme of(BuildContext context) {
    final theme = Theme.of(context);
    return theme.extension<FlarkCellTheme>() ??
        FlarkCellTheme(
          body: CellStyle(
            foreground: theme.colorScheme.foreground,
            background: theme.colorScheme.background,
          ).merge(theme.textStyle),
          heading: const CellStyle(bold: true),
          headingStyles: {
            for (final entry in defaultHeadingStyles.entries)
              entry.key: FlarkHeadingStyle(
                style: entry.value.style.merge(
                  CellStyle(
                    foreground: switch (entry.key) {
                      1 => theme.colorScheme.primary,
                      _ =>
                        theme.textStyle.foreground ??
                            theme.colorScheme.foreground,
                    },
                  ),
                ),
              ),
          },
          headingDivider: theme.mutedStyle,
          thematicBreak: theme.mutedStyle,
          headingIndicator: theme.mutedStyle,
          link: CellStyle(foreground: theme.colorScheme.info, underline: true),
          tableBorder: theme.mutedStyle,
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

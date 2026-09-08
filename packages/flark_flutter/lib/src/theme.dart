import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'code_font.dart';

/// Styles merge over the body, preserving combined Markdown attributes.
enum FlarkTextRole {
  body,
  heading1,
  heading2,
  heading3,
  heading4,
  heading5,
  heading6,
  strong,
  emphasis,
  strikethrough,
  link,
  inlineCode,
  codeBlock,
  quote,
  listMarker,
  tableHeader,
  imageLabel,
}

enum FlarkColorRole {
  canvas,
  codeBackground,
  quoteBackground,
  quoteRail,
  tableBorder,
  tableHeaderBackground,
  rule,
  taskBorder,
  taskFill,
  taskCheck,
  selection,
  caret,
  imageBackground,
  imageBorder,
}

/// Logical pixels. Values are finite and nonnegative; image dimensions positive.
enum FlarkMetric {
  documentPadding,
  rowSpacing,
  rowInset,
  headingSpacing,
  listIndent,
  tablePadding,
  codePadding,
  codeRadius,
  quoteRailWidth,
  ruleWidth,
  imageHeight,
  imageMaxWidth,
  imageSpacing,
  imageRadius,
}

/// Token overrides change color only: asynchronous analysis cannot move glyphs.
enum FlarkSyntaxRole { keyword, string, number, comment, function, variable }

/// Sparse, immutable Markdown overrides. Install in [ThemeData.extensions] or
/// pass to an editor/viewer's `theme`. Unspecified roles inherit host defaults.
class FlarkThemeData extends ThemeExtension<FlarkThemeData> {
  FlarkThemeData({
    Map<FlarkTextRole, TextStyle> styles = const {},
    Map<FlarkColorRole, Color> colors = const {},
    Map<FlarkMetric, double> metrics = const {},
    Map<FlarkSyntaxRole, Color> syntaxColors = const {},
  }) : styles = Map.unmodifiable(styles),
       colors = Map.unmodifiable(colors),
       metrics = Map.unmodifiable(metrics),
       syntaxColors = Map.unmodifiable(syntaxColors) {
    for (final e in metrics.entries) {
      if (!e.value.isFinite ||
          e.value < 0 ||
          e.value > 2048 ||
          ((e.key == FlarkMetric.imageHeight ||
                  e.key == FlarkMetric.imageMaxWidth) &&
              e.value == 0)) {
        throw ArgumentError.value(e.value, e.key.name, 'Invalid layout metric');
      }
    }
    for (final style in styles.values) {
      for (final value in [style.fontSize, style.height]) {
        if (value != null && (!value.isFinite || value <= 0)) {
          throw ArgumentError.value(
            value,
            'style',
            'Must be finite and positive',
          );
        }
      }
    }
  }

  final Map<FlarkTextRole, TextStyle> styles;
  final Map<FlarkColorRole, Color> colors;
  final Map<FlarkMetric, double> metrics;
  final Map<FlarkSyntaxRole, Color> syntaxColors;

  @override
  FlarkThemeData copyWith({
    Map<FlarkTextRole, TextStyle>? styles,
    Map<FlarkColorRole, Color>? colors,
    Map<FlarkMetric, double>? metrics,
    Map<FlarkSyntaxRole, Color>? syntaxColors,
  }) => FlarkThemeData(
    styles: styles ?? this.styles,
    colors: colors ?? this.colors,
    metrics: metrics ?? this.metrics,
    syntaxColors: syntaxColors ?? this.syntaxColors,
  );

  /// Merge overrides by role; text styles merge field by field.
  FlarkThemeData merge(FlarkThemeData? other) => other == null
      ? this
      : FlarkThemeData(
          styles: {
            ...styles,
            for (final e in other.styles.entries)
              e.key: styles[e.key]?.merge(e.value) ?? e.value,
          },
          colors: {...colors, ...other.colors},
          metrics: {...metrics, ...other.metrics},
          syntaxColors: {...syntaxColors, ...other.syntaxColors},
        );

  @override
  FlarkThemeData lerp(covariant FlarkThemeData? other, double t) {
    if (other == null) return this;
    t = t.clamp(0, 1);
    // Missing overrides continue to inherit; do not animate an absent metric
    // from zero or fade inherited text out while transitioning sparse themes.
    Map<K, V> mix<K, V>(Map<K, V> a, Map<K, V> b, V Function(V, V) f) => {
      for (final key in {...a.keys, ...b.keys})
        if (a.containsKey(key) && b.containsKey(key))
          key: f(a[key] as V, b[key] as V)
        else if (t < .5 && a.containsKey(key))
          key: a[key] as V
        else if (t >= .5 && b.containsKey(key))
          key: b[key] as V,
    };
    return FlarkThemeData(
      styles: mix(styles, other.styles, (a, b) => TextStyle.lerp(a, b, t)!),
      colors: mix(colors, other.colors, (a, b) => Color.lerp(a, b, t)!),
      metrics: mix(metrics, other.metrics, (a, b) => a + (b - a) * t),
      syntaxColors: mix(
        syntaxColors,
        other.syntaxColors,
        (a, b) => Color.lerp(a, b, t)!,
      ),
    );
  }

  /// Resolve toolkit defaults, ambient extension, then per-instance overrides.
  /// The compatibility body style applies last; role styles remain specific.
  static FlarkThemeData resolve(
    BuildContext context, {
    FlarkThemeData? overrides,
    TextStyle? bodyStyle,
  }) {
    final material = Theme.of(context), c = material.colorScheme;
    final dark = material.brightness == Brightness.dark;
    final base = FlarkThemeData(
      styles: {
        FlarkTextRole.body:
            (material.textTheme.bodyLarge ?? const TextStyle(fontSize: 17))
                .copyWith(
                  color: material.textTheme.bodyLarge?.color ?? c.onSurface,
                  fontSize: material.textTheme.bodyLarge?.fontSize ?? 17,
                  height: material.textTheme.bodyLarge?.height ?? 1.45,
                ),
        FlarkTextRole.strong: const TextStyle(fontWeight: FontWeight.w700),
        FlarkTextRole.emphasis: const TextStyle(fontStyle: FontStyle.italic),
        FlarkTextRole.strikethrough: const TextStyle(
          decoration: TextDecoration.lineThrough,
        ),
        FlarkTextRole.link: TextStyle(
          color: c.primary,
          decoration: TextDecoration.underline,
        ),
        FlarkTextRole.inlineCode: TextStyle(
          fontFamily: flarkCodeFontFamily,
          backgroundColor: c.surfaceContainerHighest,
        ),
        FlarkTextRole.codeBlock: const TextStyle(
          fontFamily: flarkCodeFontFamily,
        ),
        FlarkTextRole.tableHeader: const TextStyle(fontWeight: FontWeight.w700),
        FlarkTextRole.imageLabel: TextStyle(
          fontSize: 13,
          color: c.onSurfaceVariant,
        ),
      },
      colors: {
        FlarkColorRole.canvas: c.surface,
        FlarkColorRole.codeBackground: c.surfaceContainerLow,
        FlarkColorRole.quoteBackground: Colors.transparent,
        FlarkColorRole.quoteRail: c.outline,
        FlarkColorRole.tableBorder: c.outlineVariant,
        FlarkColorRole.tableHeaderBackground: c.surfaceContainerLow,
        FlarkColorRole.rule: c.outline,
        FlarkColorRole.taskBorder: c.onSurface,
        FlarkColorRole.taskFill: Colors.transparent,
        FlarkColorRole.taskCheck: c.primary,
        FlarkColorRole.selection:
            material.textSelectionTheme.selectionColor ??
            c.primary.withValues(alpha: .25),
        FlarkColorRole.caret:
            material.textSelectionTheme.cursorColor ?? c.primary,
        FlarkColorRole.imageBackground: c.surfaceContainerLow,
        FlarkColorRole.imageBorder: c.outlineVariant,
      },
      metrics: const {
        FlarkMetric.documentPadding: 16,
        FlarkMetric.rowSpacing: 8,
        FlarkMetric.rowInset: 4,
        FlarkMetric.headingSpacing: 8,
        FlarkMetric.listIndent: 22,
        FlarkMetric.tablePadding: 8,
        FlarkMetric.codePadding: 4,
        FlarkMetric.codeRadius: 5,
        FlarkMetric.quoteRailWidth: 3,
        FlarkMetric.ruleWidth: 1,
        FlarkMetric.imageHeight: 180,
        FlarkMetric.imageMaxWidth: 480,
        FlarkMetric.imageSpacing: 12,
        FlarkMetric.imageRadius: 6,
      },
      syntaxColors: {
        FlarkSyntaxRole.keyword: Color(dark ? 0xffd2a8ff : 0xff6639ba),
        FlarkSyntaxRole.string: Color(dark ? 0xffa5d6ff : 0xff116329),
        FlarkSyntaxRole.number: Color(dark ? 0xff79c0ff : 0xff0550ae),
        FlarkSyntaxRole.comment: Color(dark ? 0xffa4aab3 : 0xff57606a),
        FlarkSyntaxRole.function: Color(dark ? 0xffffa657 : 0xff953800),
        FlarkSyntaxRole.variable: Color(dark ? 0xffffd580 : 0xff825800),
      },
    ).merge(material.extension<FlarkThemeData>()).merge(overrides);
    return bodyStyle == null
        ? base
        : base.merge(
            FlarkThemeData(
              styles: {FlarkTextRole.body: bodyStyle.copyWith(inherit: true)},
            ),
          );
  }

  @override
  bool operator ==(Object other) =>
      other is FlarkThemeData &&
      mapEquals(styles, other.styles) &&
      mapEquals(colors, other.colors) &&
      mapEquals(metrics, other.metrics) &&
      mapEquals(syntaxColors, other.syntaxColors);
  @override
  int get hashCode => Object.hash(
    Object.hashAllUnordered(
      styles.entries.map((e) => Object.hash(e.key, e.value)),
    ),
    Object.hashAllUnordered(
      colors.entries.map((e) => Object.hash(e.key, e.value)),
    ),
    Object.hashAllUnordered(
      metrics.entries.map((e) => Object.hash(e.key, e.value)),
    ),
    Object.hashAllUnordered(
      syntaxColors.entries.map((e) => Object.hash(e.key, e.value)),
    ),
  );
}

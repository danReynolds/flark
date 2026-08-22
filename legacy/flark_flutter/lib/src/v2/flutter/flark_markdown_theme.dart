import 'package:flutter/widgets.dart';

/// Colors for syntax-highlighted code, keyed by highlight class.
///
/// Used by code fences in both the live editor and the read-only preview.
@immutable
final class FlarkCodeSyntaxThemeData {
  const FlarkCodeSyntaxThemeData({
    this.commentColor = const Color(0xFF64748B),
    this.stringColor = const Color(0xFF0F766E),
    this.numberColor = const Color(0xFFB45309),
    this.keywordColor = const Color(0xFF7C3AED),
    this.functionColor = const Color(0xFF0369A1),
    this.typeColor = const Color(0xFF047857),
    this.attributeColor = const Color(0xFF1D4ED8),
    this.variableColor = const Color(0xFFC2410C),
    this.metaColor = const Color(0xFF475569),
    this.deletionColor = const Color(0xFFB91C1C),
    this.additionColor = const Color(0xFF047857),
  });

  static const FlarkCodeSyntaxThemeData light = FlarkCodeSyntaxThemeData();

  static const FlarkCodeSyntaxThemeData dark = FlarkCodeSyntaxThemeData(
    commentColor: Color(0xFF8B98A5),
    stringColor: Color(0xFF5FB8AE),
    numberColor: Color(0xFFE0AC53),
    keywordColor: Color(0xFFB69DF8),
    functionColor: Color(0xFF6CB6FF),
    typeColor: Color(0xFF57AB5A),
    attributeColor: Color(0xFF96D0FF),
    variableColor: Color(0xFFF69D50),
    metaColor: Color(0xFF8B98A5),
    deletionColor: Color(0xFFE5534B),
    additionColor: Color(0xFF57AB5A),
  );

  final Color commentColor;
  final Color stringColor;
  final Color numberColor;
  final Color keywordColor;
  final Color functionColor;
  final Color typeColor;
  final Color attributeColor;
  final Color variableColor;
  final Color metaColor;
  final Color deletionColor;
  final Color additionColor;

  FlarkCodeSyntaxThemeData copyWith({
    Color? commentColor,
    Color? stringColor,
    Color? numberColor,
    Color? keywordColor,
    Color? functionColor,
    Color? typeColor,
    Color? attributeColor,
    Color? variableColor,
    Color? metaColor,
    Color? deletionColor,
    Color? additionColor,
  }) {
    return FlarkCodeSyntaxThemeData(
      commentColor: commentColor ?? this.commentColor,
      stringColor: stringColor ?? this.stringColor,
      numberColor: numberColor ?? this.numberColor,
      keywordColor: keywordColor ?? this.keywordColor,
      functionColor: functionColor ?? this.functionColor,
      typeColor: typeColor ?? this.typeColor,
      attributeColor: attributeColor ?? this.attributeColor,
      variableColor: variableColor ?? this.variableColor,
      metaColor: metaColor ?? this.metaColor,
      deletionColor: deletionColor ?? this.deletionColor,
      additionColor: additionColor ?? this.additionColor,
    );
  }

  @override
  bool operator ==(Object other) {
    return other is FlarkCodeSyntaxThemeData &&
        other.commentColor == commentColor &&
        other.stringColor == stringColor &&
        other.numberColor == numberColor &&
        other.keywordColor == keywordColor &&
        other.functionColor == functionColor &&
        other.typeColor == typeColor &&
        other.attributeColor == attributeColor &&
        other.variableColor == variableColor &&
        other.metaColor == metaColor &&
        other.deletionColor == deletionColor &&
        other.additionColor == additionColor;
  }

  @override
  int get hashCode {
    return Object.hash(
      commentColor,
      stringColor,
      numberColor,
      keywordColor,
      functionColor,
      typeColor,
      attributeColor,
      variableColor,
      metaColor,
      deletionColor,
      additionColor,
    );
  }
}

/// Visual styling for Flark markdown surfaces.
///
/// The default constructor produces the light palette; [dark] is a matching
/// dark palette. Customize with constructor arguments or [copyWith]:
///
/// ```dart
/// FlarkMarkdownEditor(
///   theme: FlarkMarkdownThemeData.dark.copyWith(linkColor: myBlue),
///   ...
/// )
/// ```
///
/// The base text size and family come from the editor/preview `textStyle`;
/// the theme layers colors on top plus optional per-token typography
/// overrides ([codeTextStyle], [headingTextStyle], [linkTextStyle], …) that
/// merge over the computed defaults.
@immutable
final class FlarkMarkdownThemeData {
  const FlarkMarkdownThemeData({
    this.codeTextColor = const Color(0xFF17202A),
    this.quoteTextColor = const Color(0xFF42526E),
    this.linkColor = const Color(0xFF0057B8),
    this.listMarkerColor = const Color(0xFF5B677A),
    this.chromeLabelColor = const Color(0xFF42526E),
    this.chromeSelectedLabelColor = const Color(0xFF17202A),
    this.captionTextColor = const Color(0xFF5B6B7F),
    this.errorTextColor = const Color(0xFFB3261E),
    this.inlineCodeBackgroundColor = const Color(0xFFEFF3F7),
    this.codeBlockBackgroundColor = const Color(0xFFF1F4F8),
    this.quoteBackgroundColor = const Color(0xFFF8FAFC),
    this.quoteRailColor = const Color(0xFF7A8CA3),
    this.cardBackgroundColor = const Color(0xFFF8FAFC),
    this.chipBackgroundColor = const Color(0xFFE2E8F0),
    this.chipActiveBackgroundColor = const Color(0xFFD7DEE8),
    this.menuBackgroundColor = const Color(0xFFFFFFFF),
    this.menuShadowColor = const Color(0x1A000000),
    this.borderColor = const Color(0xFFD7DEE8),
    this.overlayControlBorderColor = const Color(0xFFB8C1CC),
    this.tableHeaderBackgroundColor = const Color(0xFFF1F4F8),
    this.tableRowBackgroundColor = const Color(0xFFFFFFFF),
    this.tableDividerColor = const Color(0xFFE2E8F0),
    this.checkboxCheckedColor = const Color(0xFF2E7D32),
    this.checkboxBorderColor = const Color(0xFF7A8CA3),
    this.checkboxFillColor = const Color(0xFFFFFFFF),
    this.checkboxCheckmarkColor = const Color(0xFFFFFFFF),
    this.cursorColor = const Color(0xFF006ADC),
    this.selectionColor,
    this.syntaxTheme = FlarkCodeSyntaxThemeData.light,
    this.codeTextStyle,
    this.inlineCodeTextStyle,
    this.headingTextStyle,
    this.heading1TextStyle,
    this.heading2TextStyle,
    this.heading3TextStyle,
    this.heading4TextStyle,
    this.heading5TextStyle,
    this.heading6TextStyle,
    this.quoteTextStyle,
    this.linkTextStyle,
    this.strongTextStyle,
    this.emphasisTextStyle,
    this.strikethroughTextStyle,
  });

  /// The default palette; identical to [FlarkMarkdownThemeData.new] defaults.
  static const FlarkMarkdownThemeData light = FlarkMarkdownThemeData();

  static const FlarkMarkdownThemeData dark = FlarkMarkdownThemeData(
    codeTextColor: Color(0xFFE6EDF3),
    quoteTextColor: Color(0xFFADBAC7),
    linkColor: Color(0xFF539BF5),
    listMarkerColor: Color(0xFF8B98A5),
    chromeLabelColor: Color(0xFFADBAC7),
    chromeSelectedLabelColor: Color(0xFFE6EDF3),
    captionTextColor: Color(0xFF8B98A5),
    errorTextColor: Color(0xFFE5534B),
    inlineCodeBackgroundColor: Color(0xFF2D333B),
    codeBlockBackgroundColor: Color(0xFF22272E),
    quoteBackgroundColor: Color(0xFF262C33),
    quoteRailColor: Color(0xFF768390),
    cardBackgroundColor: Color(0xFF2D333B),
    chipBackgroundColor: Color(0xFF373E47),
    chipActiveBackgroundColor: Color(0xFF444C56),
    menuBackgroundColor: Color(0xFF2D333B),
    menuShadowColor: Color(0x66000000),
    borderColor: Color(0xFF444C56),
    overlayControlBorderColor: Color(0xFF545D68),
    tableHeaderBackgroundColor: Color(0xFF2D333B),
    tableRowBackgroundColor: Color(0xFF22272E),
    tableDividerColor: Color(0xFF373E47),
    checkboxCheckedColor: Color(0xFF57AB5A),
    checkboxBorderColor: Color(0xFF768390),
    checkboxFillColor: Color(0xFF22272E),
    checkboxCheckmarkColor: Color(0xFFFFFFFF),
    cursorColor: Color(0xFF539BF5),
    syntaxTheme: FlarkCodeSyntaxThemeData.dark,
  );

  /// [light] or [dark] by [brightness].
  factory FlarkMarkdownThemeData.fromBrightness(Brightness brightness) {
    return brightness == Brightness.dark ? dark : light;
  }

  /// Text color inside code fences (the body of a code block).
  final Color codeTextColor;

  /// Text color inside blockquotes.
  final Color quoteTextColor;

  /// Link text color (links are additionally underlined).
  final Color linkColor;

  /// Ordered-list marker text color (`1.`, `2.` …).
  final Color listMarkerColor;

  /// Label color for editor chrome: copy buttons, language badges, menu
  /// actions, and bullet markers.
  final Color chromeLabelColor;

  /// Label color for the selected entry in chrome menus.
  final Color chromeSelectedLabelColor;

  /// Secondary caption color (e.g. the destination line on image cards).
  final Color captionTextColor;

  /// Validation error text color on [FlarkMarkdownEditorFormField] when no
  /// `errorStyle` is supplied.
  final Color errorTextColor;

  /// Background behind inline `code` spans in the live editor.
  final Color inlineCodeBackgroundColor;

  /// Background of fenced code blocks.
  final Color codeBlockBackgroundColor;

  /// Background of blockquotes.
  final Color quoteBackgroundColor;

  /// The vertical rail on the leading edge of blockquotes.
  final Color quoteRailColor;

  /// Background of cards and small action chips (image cards, menu actions).
  final Color cardBackgroundColor;

  /// Background of chrome chips (copy button, language badge).
  final Color chipBackgroundColor;

  /// Background of an activated chrome chip (e.g. the open language picker).
  final Color chipActiveBackgroundColor;

  /// Background of floating menus (link menus, language picker).
  final Color menuBackgroundColor;

  /// Drop shadow of floating menus (the blur radius and offset are fixed).
  final Color menuShadowColor;

  /// Border for code fences, tables, cards, and menus.
  final Color borderColor;

  /// Border for the default render-plan overlay controls.
  final Color overlayControlBorderColor;

  /// Background of the table header row.
  final Color tableHeaderBackgroundColor;

  /// Background of non-header table rows.
  final Color tableRowBackgroundColor;

  /// Divider lines between table cells.
  final Color tableDividerColor;

  /// Fill and border of a checked task-list checkbox.
  final Color checkboxCheckedColor;

  /// Border of an unchecked task-list checkbox.
  final Color checkboxBorderColor;

  /// Fill of an unchecked task-list checkbox.
  final Color checkboxFillColor;

  /// The checkmark glyph inside a checked task-list checkbox.
  final Color checkboxCheckmarkColor;

  /// Default text cursor color; an explicit `cursorColor` widget parameter
  /// or an ambient [DefaultSelectionStyle] takes precedence.
  final Color cursorColor;

  /// Text selection highlight.
  ///
  /// When null, a translucent tint of the effective cursor color is used.
  final Color? selectionColor;

  /// Colors for syntax-highlighted code.
  final FlarkCodeSyntaxThemeData syntaxTheme;

  /// Merged over the computed code-block text style (code fences in both
  /// the live editor and the preview). The default is the base editor
  /// style with [codeTextColor], the `monospace` family, and 1.35 line
  /// height; use this to supply a custom code font, size, or height.
  final TextStyle? codeTextStyle;

  /// Merged over inline `code` span styling (monospace family by default).
  final TextStyle? inlineCodeTextStyle;

  /// Merged over the computed heading style for every level.
  ///
  /// The default heading style scales the base font size by
  /// `(7 - level) * 2` logical pixels and applies bold; merge a color,
  /// family, or size here to override it. Applied before the per-level
  /// [heading1TextStyle]…[heading6TextStyle] overrides.
  final TextStyle? headingTextStyle;

  /// Per-level heading override, merged after [headingTextStyle].
  final TextStyle? heading1TextStyle;

  /// Per-level heading override, merged after [headingTextStyle].
  final TextStyle? heading2TextStyle;

  /// Per-level heading override, merged after [headingTextStyle].
  final TextStyle? heading3TextStyle;

  /// Per-level heading override, merged after [headingTextStyle].
  final TextStyle? heading4TextStyle;

  /// Per-level heading override, merged after [headingTextStyle].
  final TextStyle? heading5TextStyle;

  /// Per-level heading override, merged after [headingTextStyle].
  final TextStyle? heading6TextStyle;

  /// Merged over blockquote text (colored [quoteTextColor] by default).
  final TextStyle? quoteTextStyle;

  /// Merged over link text ([linkColor] plus underline by default — set
  /// `decoration: TextDecoration.none` here to remove the underline).
  final TextStyle? linkTextStyle;

  /// Merged over strong (`**bold**`) text, bold by default.
  final TextStyle? strongTextStyle;

  /// Merged over emphasized (`*italic*`) text, italic by default.
  final TextStyle? emphasisTextStyle;

  /// Merged over strikethrough text, line-through by default.
  final TextStyle? strikethroughTextStyle;

  /// The heading override for [level] (1–6), merged after
  /// [headingTextStyle].
  TextStyle? headingLevelTextStyle(int level) {
    return switch (level) {
      1 => heading1TextStyle,
      2 => heading2TextStyle,
      3 => heading3TextStyle,
      4 => heading4TextStyle,
      5 => heading5TextStyle,
      6 => heading6TextStyle,
      _ => null,
    };
  }

  FlarkMarkdownThemeData copyWith({
    Color? codeTextColor,
    Color? quoteTextColor,
    Color? linkColor,
    Color? listMarkerColor,
    Color? chromeLabelColor,
    Color? chromeSelectedLabelColor,
    Color? captionTextColor,
    Color? errorTextColor,
    Color? inlineCodeBackgroundColor,
    Color? codeBlockBackgroundColor,
    Color? quoteBackgroundColor,
    Color? quoteRailColor,
    Color? cardBackgroundColor,
    Color? chipBackgroundColor,
    Color? chipActiveBackgroundColor,
    Color? menuBackgroundColor,
    Color? menuShadowColor,
    Color? borderColor,
    Color? overlayControlBorderColor,
    Color? tableHeaderBackgroundColor,
    Color? tableRowBackgroundColor,
    Color? tableDividerColor,
    Color? checkboxCheckedColor,
    Color? checkboxBorderColor,
    Color? checkboxFillColor,
    Color? checkboxCheckmarkColor,
    Color? cursorColor,
    Color? selectionColor,
    FlarkCodeSyntaxThemeData? syntaxTheme,
    TextStyle? codeTextStyle,
    TextStyle? inlineCodeTextStyle,
    TextStyle? headingTextStyle,
    TextStyle? heading1TextStyle,
    TextStyle? heading2TextStyle,
    TextStyle? heading3TextStyle,
    TextStyle? heading4TextStyle,
    TextStyle? heading5TextStyle,
    TextStyle? heading6TextStyle,
    TextStyle? quoteTextStyle,
    TextStyle? linkTextStyle,
    TextStyle? strongTextStyle,
    TextStyle? emphasisTextStyle,
    TextStyle? strikethroughTextStyle,
  }) {
    return FlarkMarkdownThemeData(
      codeTextColor: codeTextColor ?? this.codeTextColor,
      quoteTextColor: quoteTextColor ?? this.quoteTextColor,
      linkColor: linkColor ?? this.linkColor,
      listMarkerColor: listMarkerColor ?? this.listMarkerColor,
      chromeLabelColor: chromeLabelColor ?? this.chromeLabelColor,
      chromeSelectedLabelColor:
          chromeSelectedLabelColor ?? this.chromeSelectedLabelColor,
      captionTextColor: captionTextColor ?? this.captionTextColor,
      errorTextColor: errorTextColor ?? this.errorTextColor,
      inlineCodeBackgroundColor:
          inlineCodeBackgroundColor ?? this.inlineCodeBackgroundColor,
      codeBlockBackgroundColor:
          codeBlockBackgroundColor ?? this.codeBlockBackgroundColor,
      quoteBackgroundColor: quoteBackgroundColor ?? this.quoteBackgroundColor,
      quoteRailColor: quoteRailColor ?? this.quoteRailColor,
      cardBackgroundColor: cardBackgroundColor ?? this.cardBackgroundColor,
      chipBackgroundColor: chipBackgroundColor ?? this.chipBackgroundColor,
      chipActiveBackgroundColor:
          chipActiveBackgroundColor ?? this.chipActiveBackgroundColor,
      menuBackgroundColor: menuBackgroundColor ?? this.menuBackgroundColor,
      menuShadowColor: menuShadowColor ?? this.menuShadowColor,
      borderColor: borderColor ?? this.borderColor,
      overlayControlBorderColor:
          overlayControlBorderColor ?? this.overlayControlBorderColor,
      tableHeaderBackgroundColor:
          tableHeaderBackgroundColor ?? this.tableHeaderBackgroundColor,
      tableRowBackgroundColor:
          tableRowBackgroundColor ?? this.tableRowBackgroundColor,
      tableDividerColor: tableDividerColor ?? this.tableDividerColor,
      checkboxCheckedColor: checkboxCheckedColor ?? this.checkboxCheckedColor,
      checkboxBorderColor: checkboxBorderColor ?? this.checkboxBorderColor,
      checkboxFillColor: checkboxFillColor ?? this.checkboxFillColor,
      checkboxCheckmarkColor:
          checkboxCheckmarkColor ?? this.checkboxCheckmarkColor,
      cursorColor: cursorColor ?? this.cursorColor,
      selectionColor: selectionColor ?? this.selectionColor,
      syntaxTheme: syntaxTheme ?? this.syntaxTheme,
      codeTextStyle: codeTextStyle ?? this.codeTextStyle,
      inlineCodeTextStyle: inlineCodeTextStyle ?? this.inlineCodeTextStyle,
      headingTextStyle: headingTextStyle ?? this.headingTextStyle,
      heading1TextStyle: heading1TextStyle ?? this.heading1TextStyle,
      heading2TextStyle: heading2TextStyle ?? this.heading2TextStyle,
      heading3TextStyle: heading3TextStyle ?? this.heading3TextStyle,
      heading4TextStyle: heading4TextStyle ?? this.heading4TextStyle,
      heading5TextStyle: heading5TextStyle ?? this.heading5TextStyle,
      heading6TextStyle: heading6TextStyle ?? this.heading6TextStyle,
      quoteTextStyle: quoteTextStyle ?? this.quoteTextStyle,
      linkTextStyle: linkTextStyle ?? this.linkTextStyle,
      strongTextStyle: strongTextStyle ?? this.strongTextStyle,
      emphasisTextStyle: emphasisTextStyle ?? this.emphasisTextStyle,
      strikethroughTextStyle:
          strikethroughTextStyle ?? this.strikethroughTextStyle,
    );
  }

  @override
  bool operator ==(Object other) {
    return other is FlarkMarkdownThemeData &&
        other.codeTextColor == codeTextColor &&
        other.quoteTextColor == quoteTextColor &&
        other.linkColor == linkColor &&
        other.listMarkerColor == listMarkerColor &&
        other.chromeLabelColor == chromeLabelColor &&
        other.chromeSelectedLabelColor == chromeSelectedLabelColor &&
        other.captionTextColor == captionTextColor &&
        other.errorTextColor == errorTextColor &&
        other.inlineCodeBackgroundColor == inlineCodeBackgroundColor &&
        other.codeBlockBackgroundColor == codeBlockBackgroundColor &&
        other.quoteBackgroundColor == quoteBackgroundColor &&
        other.quoteRailColor == quoteRailColor &&
        other.cardBackgroundColor == cardBackgroundColor &&
        other.chipBackgroundColor == chipBackgroundColor &&
        other.chipActiveBackgroundColor == chipActiveBackgroundColor &&
        other.menuBackgroundColor == menuBackgroundColor &&
        other.menuShadowColor == menuShadowColor &&
        other.borderColor == borderColor &&
        other.overlayControlBorderColor == overlayControlBorderColor &&
        other.tableHeaderBackgroundColor == tableHeaderBackgroundColor &&
        other.tableRowBackgroundColor == tableRowBackgroundColor &&
        other.tableDividerColor == tableDividerColor &&
        other.checkboxCheckedColor == checkboxCheckedColor &&
        other.checkboxBorderColor == checkboxBorderColor &&
        other.checkboxFillColor == checkboxFillColor &&
        other.checkboxCheckmarkColor == checkboxCheckmarkColor &&
        other.cursorColor == cursorColor &&
        other.selectionColor == selectionColor &&
        other.syntaxTheme == syntaxTheme &&
        other.codeTextStyle == codeTextStyle &&
        other.inlineCodeTextStyle == inlineCodeTextStyle &&
        other.headingTextStyle == headingTextStyle &&
        other.heading1TextStyle == heading1TextStyle &&
        other.heading2TextStyle == heading2TextStyle &&
        other.heading3TextStyle == heading3TextStyle &&
        other.heading4TextStyle == heading4TextStyle &&
        other.heading5TextStyle == heading5TextStyle &&
        other.heading6TextStyle == heading6TextStyle &&
        other.quoteTextStyle == quoteTextStyle &&
        other.linkTextStyle == linkTextStyle &&
        other.strongTextStyle == strongTextStyle &&
        other.emphasisTextStyle == emphasisTextStyle &&
        other.strikethroughTextStyle == strikethroughTextStyle;
  }

  @override
  int get hashCode {
    return Object.hashAll([
      codeTextColor,
      quoteTextColor,
      linkColor,
      listMarkerColor,
      chromeLabelColor,
      chromeSelectedLabelColor,
      captionTextColor,
      errorTextColor,
      inlineCodeBackgroundColor,
      codeBlockBackgroundColor,
      quoteBackgroundColor,
      quoteRailColor,
      cardBackgroundColor,
      chipBackgroundColor,
      chipActiveBackgroundColor,
      menuBackgroundColor,
      menuShadowColor,
      borderColor,
      overlayControlBorderColor,
      tableHeaderBackgroundColor,
      tableRowBackgroundColor,
      tableDividerColor,
      checkboxCheckedColor,
      checkboxBorderColor,
      checkboxFillColor,
      checkboxCheckmarkColor,
      cursorColor,
      selectionColor,
      syntaxTheme,
      codeTextStyle,
      inlineCodeTextStyle,
      headingTextStyle,
      heading1TextStyle,
      heading2TextStyle,
      heading3TextStyle,
      heading4TextStyle,
      heading5TextStyle,
      heading6TextStyle,
      quoteTextStyle,
      linkTextStyle,
      strongTextStyle,
      emphasisTextStyle,
      strikethroughTextStyle,
    ]);
  }
}

/// Ambient [FlarkMarkdownThemeData] for a widget subtree.
///
/// Flark surfaces resolve their colors with [of]. When no
/// [FlarkMarkdownTheme] ancestor exists, [of] falls back to
/// [FlarkMarkdownThemeData.light] or [FlarkMarkdownThemeData.dark] based on
/// the platform brightness, so apps that follow the system theme get a
/// matching markdown palette without any configuration. Apps whose theme is
/// independent of platform brightness should provide a theme explicitly —
/// either with this widget or with the `theme` parameter on the markdown
/// surfaces.
final class FlarkMarkdownTheme extends InheritedWidget {
  const FlarkMarkdownTheme({
    super.key,
    required this.data,
    required super.child,
  });

  final FlarkMarkdownThemeData data;

  /// The nearest theme, or a brightness-matched default when none is given.
  static FlarkMarkdownThemeData of(BuildContext context) {
    final inherited = context
        .dependOnInheritedWidgetOfExactType<FlarkMarkdownTheme>();
    if (inherited != null) return inherited.data;
    return switch (MediaQuery.maybePlatformBrightnessOf(context)) {
      Brightness.dark => FlarkMarkdownThemeData.dark,
      Brightness.light || null => FlarkMarkdownThemeData.light,
    };
  }

  @override
  bool updateShouldNotify(FlarkMarkdownTheme oldWidget) {
    return oldWidget.data != data;
  }
}

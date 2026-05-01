import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

class _DefaultMarkdownPalette {
  static const Color white = Colors.white;
  static const Color sand = Color(0xFFD4C5A9);
  static const Color gold = Color(0xFFC5A065);
  static const Color brown = Color(0xFF2C241E);
}

@immutable
class DuneMarkdownTheme extends ThemeExtension<DuneMarkdownTheme> {
  final double h1Scale;
  final double h2Scale;
  final double h3Scale;
  final double h4Scale;
  final double h5Scale;
  final double h6Scale;
  final FontWeight headingWeight;

  final Color blockquoteTextColor;
  final Color blockquoteBorderColor;
  final Color taskCheckedColor;

  final Color inlineCodeTextColor;
  final Color inlineCodeBackgroundColor;

  final Color codeBlockTextColor;
  final Color codeBlockBackgroundColor;
  final Color codeBlockBorderColor;

  final Color linkColor;

  final Color syntaxKeywordColor;
  final Color syntaxStringColor;
  final Color syntaxCommentColor;
  final Color syntaxNumberColor;
  final Color syntaxTitleColor;
  final Color syntaxBuiltInColor;
  final Color syntaxLiteralColor;
  final Color syntaxMetaColor;

  final EdgeInsets blockquotePadding;
  final BorderRadius blockquoteBorderRadius;
  final EdgeInsets codeBlockPadding;
  final BorderRadius codeBlockBorderRadius;

  final double blockSpacing;
  final double listIndent;
  final String? monospaceFontFamily;

  const DuneMarkdownTheme({
    required this.h1Scale,
    required this.h2Scale,
    required this.h3Scale,
    required this.h4Scale,
    required this.h5Scale,
    required this.h6Scale,
    required this.headingWeight,
    required this.blockquoteTextColor,
    required this.blockquoteBorderColor,
    required this.taskCheckedColor,
    required this.inlineCodeTextColor,
    required this.inlineCodeBackgroundColor,
    required this.codeBlockTextColor,
    required this.codeBlockBackgroundColor,
    required this.codeBlockBorderColor,
    required this.linkColor,
    required this.syntaxKeywordColor,
    required this.syntaxStringColor,
    required this.syntaxCommentColor,
    required this.syntaxNumberColor,
    required this.syntaxTitleColor,
    required this.syntaxBuiltInColor,
    required this.syntaxLiteralColor,
    required this.syntaxMetaColor,
    required this.blockquotePadding,
    required this.blockquoteBorderRadius,
    required this.codeBlockPadding,
    required this.codeBlockBorderRadius,
    required this.blockSpacing,
    required this.listIndent,
    required this.monospaceFontFamily,
  });

  factory DuneMarkdownTheme.dune() {
    return DuneMarkdownTheme(
      h1Scale: 2.00,
      h2Scale: 1.70,
      h3Scale: 1.45,
      h4Scale: 1.28,
      h5Scale: 1.16,
      h6Scale: 1.08,
      headingWeight: FontWeight.w700,
      blockquoteTextColor: _DefaultMarkdownPalette.sand.withValues(
        alpha: 0.88,
      ),
      blockquoteBorderColor: _DefaultMarkdownPalette.gold.withValues(
        alpha: 0.72,
      ),
      taskCheckedColor: Colors.white54,
      inlineCodeTextColor: _DefaultMarkdownPalette.sand.withValues(
        alpha: 0.94,
      ),
      inlineCodeBackgroundColor: _DefaultMarkdownPalette.brown.withValues(
        alpha: 0.45,
      ),
      codeBlockTextColor: _DefaultMarkdownPalette.white.withValues(
        alpha: 0.94,
      ),
      codeBlockBackgroundColor: _DefaultMarkdownPalette.brown.withValues(
        alpha: 0.58,
      ),
      codeBlockBorderColor: _DefaultMarkdownPalette.sand.withValues(
        alpha: 0.30,
      ),
      linkColor: _DefaultMarkdownPalette.gold,
      syntaxKeywordColor: const Color(0xFFE7C27A),
      syntaxStringColor: const Color(0xFFD9A66B),
      syntaxCommentColor: const Color(0xFF9A8E78),
      syntaxNumberColor: const Color(0xFFBFD0A1),
      syntaxTitleColor: const Color(0xFFF0D8A8),
      syntaxBuiltInColor: const Color(0xFFA9D0C2),
      syntaxLiteralColor: const Color(0xFFD2A9C6),
      syntaxMetaColor: const Color(0xFFC7CDD8),
      blockquotePadding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
      blockquoteBorderRadius: BorderRadius.circular(12),
      codeBlockPadding: const EdgeInsets.all(12),
      codeBlockBorderRadius: BorderRadius.circular(12),
      blockSpacing: 8.0,
      listIndent: 24.0,
      monospaceFontFamily: GoogleFonts.sourceCodePro().fontFamily,
    );
  }

  static final DuneMarkdownTheme _defaultDuneTheme = DuneMarkdownTheme.dune();

  static DuneMarkdownTheme of(BuildContext context) {
    return Theme.of(context).extension<DuneMarkdownTheme>() ??
        _defaultDuneTheme;
  }

  double headingScale(int level) {
    switch (level.clamp(1, 6)) {
      case 1:
        return h1Scale;
      case 2:
        return h2Scale;
      case 3:
        return h3Scale;
      case 4:
        return h4Scale;
      case 5:
        return h5Scale;
      case 6:
      default:
        return h6Scale;
    }
  }

  TextStyle headingStyleFor(TextStyle base, int level) {
    final baseSize = base.fontSize ?? 14.0;
    return base.copyWith(
      fontWeight: headingWeight,
      fontSize: baseSize * headingScale(level),
    );
  }

  TextStyle blockquoteStyleFor(TextStyle base) {
    return base.copyWith(
      fontStyle: FontStyle.italic,
      color: blockquoteTextColor,
    );
  }

  TextStyle taskCheckedStyleFor(TextStyle base) {
    return base.copyWith(
      decoration: TextDecoration.lineThrough,
      color: taskCheckedColor,
    );
  }

  TextStyle inlineCodeStyleFor(TextStyle base) {
    return base.copyWith(
      fontFamily: monospaceFontFamily,
      color: inlineCodeTextColor,
      backgroundColor: inlineCodeBackgroundColor,
    );
  }

  TextStyle linkStyleFor(TextStyle base) {
    return base.copyWith(
      color: linkColor,
      decoration: TextDecoration.underline,
      decorationColor: linkColor,
    );
  }

  TextStyle codeBlockStyleFor(TextStyle base) {
    return base.copyWith(
      fontFamily: monospaceFontFamily,
      color: codeBlockTextColor,
      height: 1.45,
    );
  }

  TextStyle? syntaxStyleForClass(String className) {
    switch (className) {
      case 'keyword':
        return TextStyle(color: syntaxKeywordColor);
      case 'string':
      case 'regexp':
        return TextStyle(color: syntaxStringColor);
      case 'comment':
        return TextStyle(
          color: syntaxCommentColor,
          fontStyle: FontStyle.italic,
        );
      case 'number':
        return TextStyle(color: syntaxNumberColor);
      case 'title':
      case 'function':
        return TextStyle(color: syntaxTitleColor);
      case 'built_in':
      case 'type':
        return TextStyle(color: syntaxBuiltInColor);
      case 'literal':
        return TextStyle(color: syntaxLiteralColor);
      case 'meta':
      case 'tag':
      case 'name':
      case 'attr':
      case 'attribute':
        return TextStyle(color: syntaxMetaColor);
      default:
        return null;
    }
  }

  @override
  DuneMarkdownTheme copyWith({
    double? h1Scale,
    double? h2Scale,
    double? h3Scale,
    double? h4Scale,
    double? h5Scale,
    double? h6Scale,
    FontWeight? headingWeight,
    Color? blockquoteTextColor,
    Color? blockquoteBorderColor,
    Color? taskCheckedColor,
    Color? inlineCodeTextColor,
    Color? inlineCodeBackgroundColor,
    Color? codeBlockTextColor,
    Color? codeBlockBackgroundColor,
    Color? codeBlockBorderColor,
    Color? linkColor,
    Color? syntaxKeywordColor,
    Color? syntaxStringColor,
    Color? syntaxCommentColor,
    Color? syntaxNumberColor,
    Color? syntaxTitleColor,
    Color? syntaxBuiltInColor,
    Color? syntaxLiteralColor,
    Color? syntaxMetaColor,
    EdgeInsets? blockquotePadding,
    BorderRadius? blockquoteBorderRadius,
    EdgeInsets? codeBlockPadding,
    BorderRadius? codeBlockBorderRadius,
    double? blockSpacing,
    double? listIndent,
    String? monospaceFontFamily,
  }) {
    return DuneMarkdownTheme(
      h1Scale: h1Scale ?? this.h1Scale,
      h2Scale: h2Scale ?? this.h2Scale,
      h3Scale: h3Scale ?? this.h3Scale,
      h4Scale: h4Scale ?? this.h4Scale,
      h5Scale: h5Scale ?? this.h5Scale,
      h6Scale: h6Scale ?? this.h6Scale,
      headingWeight: headingWeight ?? this.headingWeight,
      blockquoteTextColor: blockquoteTextColor ?? this.blockquoteTextColor,
      blockquoteBorderColor:
          blockquoteBorderColor ?? this.blockquoteBorderColor,
      taskCheckedColor: taskCheckedColor ?? this.taskCheckedColor,
      inlineCodeTextColor: inlineCodeTextColor ?? this.inlineCodeTextColor,
      inlineCodeBackgroundColor:
          inlineCodeBackgroundColor ?? this.inlineCodeBackgroundColor,
      codeBlockTextColor: codeBlockTextColor ?? this.codeBlockTextColor,
      codeBlockBackgroundColor:
          codeBlockBackgroundColor ?? this.codeBlockBackgroundColor,
      codeBlockBorderColor: codeBlockBorderColor ?? this.codeBlockBorderColor,
      linkColor: linkColor ?? this.linkColor,
      syntaxKeywordColor: syntaxKeywordColor ?? this.syntaxKeywordColor,
      syntaxStringColor: syntaxStringColor ?? this.syntaxStringColor,
      syntaxCommentColor: syntaxCommentColor ?? this.syntaxCommentColor,
      syntaxNumberColor: syntaxNumberColor ?? this.syntaxNumberColor,
      syntaxTitleColor: syntaxTitleColor ?? this.syntaxTitleColor,
      syntaxBuiltInColor: syntaxBuiltInColor ?? this.syntaxBuiltInColor,
      syntaxLiteralColor: syntaxLiteralColor ?? this.syntaxLiteralColor,
      syntaxMetaColor: syntaxMetaColor ?? this.syntaxMetaColor,
      blockquotePadding: blockquotePadding ?? this.blockquotePadding,
      blockquoteBorderRadius:
          blockquoteBorderRadius ?? this.blockquoteBorderRadius,
      codeBlockPadding: codeBlockPadding ?? this.codeBlockPadding,
      codeBlockBorderRadius:
          codeBlockBorderRadius ?? this.codeBlockBorderRadius,
      blockSpacing: blockSpacing ?? this.blockSpacing,
      listIndent: listIndent ?? this.listIndent,
      monospaceFontFamily: monospaceFontFamily ?? this.monospaceFontFamily,
    );
  }

  @override
  DuneMarkdownTheme lerp(
    covariant ThemeExtension<DuneMarkdownTheme>? other,
    double t,
  ) {
    if (other is! DuneMarkdownTheme) {
      return this;
    }

    return DuneMarkdownTheme(
      h1Scale: lerpDouble(h1Scale, other.h1Scale, t),
      h2Scale: lerpDouble(h2Scale, other.h2Scale, t),
      h3Scale: lerpDouble(h3Scale, other.h3Scale, t),
      h4Scale: lerpDouble(h4Scale, other.h4Scale, t),
      h5Scale: lerpDouble(h5Scale, other.h5Scale, t),
      h6Scale: lerpDouble(h6Scale, other.h6Scale, t),
      headingWeight: t < 0.5 ? headingWeight : other.headingWeight,
      blockquoteTextColor: Color.lerp(
        blockquoteTextColor,
        other.blockquoteTextColor,
        t,
      )!,
      blockquoteBorderColor: Color.lerp(
        blockquoteBorderColor,
        other.blockquoteBorderColor,
        t,
      )!,
      taskCheckedColor: Color.lerp(
        taskCheckedColor,
        other.taskCheckedColor,
        t,
      )!,
      inlineCodeTextColor: Color.lerp(
        inlineCodeTextColor,
        other.inlineCodeTextColor,
        t,
      )!,
      inlineCodeBackgroundColor: Color.lerp(
        inlineCodeBackgroundColor,
        other.inlineCodeBackgroundColor,
        t,
      )!,
      codeBlockTextColor: Color.lerp(
        codeBlockTextColor,
        other.codeBlockTextColor,
        t,
      )!,
      codeBlockBackgroundColor: Color.lerp(
        codeBlockBackgroundColor,
        other.codeBlockBackgroundColor,
        t,
      )!,
      codeBlockBorderColor: Color.lerp(
        codeBlockBorderColor,
        other.codeBlockBorderColor,
        t,
      )!,
      linkColor: Color.lerp(linkColor, other.linkColor, t)!,
      syntaxKeywordColor: Color.lerp(
        syntaxKeywordColor,
        other.syntaxKeywordColor,
        t,
      )!,
      syntaxStringColor: Color.lerp(
        syntaxStringColor,
        other.syntaxStringColor,
        t,
      )!,
      syntaxCommentColor: Color.lerp(
        syntaxCommentColor,
        other.syntaxCommentColor,
        t,
      )!,
      syntaxNumberColor: Color.lerp(
        syntaxNumberColor,
        other.syntaxNumberColor,
        t,
      )!,
      syntaxTitleColor: Color.lerp(
        syntaxTitleColor,
        other.syntaxTitleColor,
        t,
      )!,
      syntaxBuiltInColor: Color.lerp(
        syntaxBuiltInColor,
        other.syntaxBuiltInColor,
        t,
      )!,
      syntaxLiteralColor: Color.lerp(
        syntaxLiteralColor,
        other.syntaxLiteralColor,
        t,
      )!,
      syntaxMetaColor: Color.lerp(syntaxMetaColor, other.syntaxMetaColor, t)!,
      blockquotePadding: EdgeInsets.lerp(
        blockquotePadding,
        other.blockquotePadding,
        t,
      )!,
      blockquoteBorderRadius: BorderRadius.lerp(
        blockquoteBorderRadius,
        other.blockquoteBorderRadius,
        t,
      )!,
      codeBlockPadding: EdgeInsets.lerp(
        codeBlockPadding,
        other.codeBlockPadding,
        t,
      )!,
      codeBlockBorderRadius: BorderRadius.lerp(
        codeBlockBorderRadius,
        other.codeBlockBorderRadius,
        t,
      )!,
      blockSpacing: lerpDouble(blockSpacing, other.blockSpacing, t),
      listIndent: lerpDouble(listIndent, other.listIndent, t),
      monospaceFontFamily:
          t < 0.5 ? monospaceFontFamily : other.monospaceFontFamily,
    );
  }
}

double lerpDouble(double a, double b, double t) {
  return a + ((b - a) * t);
}

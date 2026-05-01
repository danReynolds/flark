import 'package:flutter/widgets.dart';
import 'package:sovereign_editor/theme/sovereign_markdown_theme.dart';

import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

/// Heading typography policy for the live editor.
///
/// The editor paints block geometry with a fixed line-height model, so very
/// large heading font sizes can overflow into adjacent lines and visually
/// desync rails/backgrounds from text. We cap heading scale in-editor while
/// preserving a clear size distinction from body text.
abstract final class EditorHeadingStylePolicy {
  static const double _kMaxHeadingScaleForFixedLineHeight = 1.35;

  static TextStyle resolve({
    required TextStyle base,
    required int level,
    required SovereignMarkdownTheme markdownTheme,
    SovereignHeadingsTheme? headingTheme,
  }) {
    final themed = markdownTheme
        .headingStyleFor(base, level.clamp(1, 6))
        .merge(headingTheme?.styleForLevel(level));
    return _capForFixedLineHeight(base: base, style: themed);
  }

  static TextStyle _capForFixedLineHeight({
    required TextStyle base,
    required TextStyle style,
  }) {
    final baseFontSize = base.fontSize ?? 14.0;
    final maxFontSize = baseFontSize * _kMaxHeadingScaleForFixedLineHeight;
    final targetFontSize = style.fontSize ?? baseFontSize;
    if (targetFontSize <= maxFontSize) return style;
    return style.copyWith(fontSize: maxFontSize);
  }
}

import 'sovereign_inline_style.dart';

class SovereignCommandCapabilities {
  final bool isComposing;
  final bool canMutate;
  final SovereignInlineStyle? activeInlineStyle;
  final int? activeHeadingLevel;
  final bool quoteActive;

  const SovereignCommandCapabilities({
    required this.isComposing,
    required this.canMutate,
    required this.activeInlineStyle,
    required this.activeHeadingLevel,
    required this.quoteActive,
  });

  bool get hasActiveInlineStyle => activeInlineStyle != null;
}

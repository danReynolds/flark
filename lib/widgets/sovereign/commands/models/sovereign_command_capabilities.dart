import 'sovereign_inline_style.dart';

/// Snapshot of command enablement and active formatting at a selection.
class SovereignCommandCapabilities {
  /// Whether the platform IME is currently composing text.
  final bool isComposing;

  /// Whether commands that mutate text should currently be enabled.
  final bool canMutate;

  /// Active inline style at the selection, if any.
  final SovereignInlineStyle? activeInlineStyle;

  /// Active heading level at the selection, if any.
  final int? activeHeadingLevel;

  /// Whether the selection is inside a blockquote.
  final bool quoteActive;

  /// Creates a capabilities snapshot.
  const SovereignCommandCapabilities({
    required this.isComposing,
    required this.canMutate,
    required this.activeInlineStyle,
    required this.activeHeadingLevel,
    required this.quoteActive,
  });

  /// Whether any inline style is active at the selection.
  bool get hasActiveInlineStyle => activeInlineStyle != null;
}

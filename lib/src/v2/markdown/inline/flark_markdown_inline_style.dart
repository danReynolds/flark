enum FlarkMarkdownInlineStyle { emphasis, strong, inlineCode, strikethrough }

extension FlarkMarkdownInlineStyleMarker on FlarkMarkdownInlineStyle {
  /// The canonical delimiter the editor writes when applying this style.
  String get marker {
    return switch (this) {
      FlarkMarkdownInlineStyle.emphasis => '*',
      FlarkMarkdownInlineStyle.strong => '**',
      FlarkMarkdownInlineStyle.inlineCode => '`',
      FlarkMarkdownInlineStyle.strikethrough => '~~',
    };
  }

  /// Every delimiter form CommonMark/GFM accepts for this style, canonical
  /// first (so `equivalentMarkers.first == marker`).
  ///
  /// Emphasis, strong, and strikethrough each have an alternate spelling
  /// (`*`/`_`, `**`/`__`, `~~`/`~`); inline code has only backticks. Applying a
  /// style always writes the canonical [marker]; recognizing an existing one —
  /// to toggle it off or to light a toolbar — must accept any form. Both the
  /// toggle command and the capability layer read this single list so the two
  /// never drift out of sync.
  List<String> get equivalentMarkers {
    return switch (this) {
      FlarkMarkdownInlineStyle.emphasis => const ['*', '_'],
      FlarkMarkdownInlineStyle.strong => const ['**', '__'],
      FlarkMarkdownInlineStyle.inlineCode => const ['`'],
      FlarkMarkdownInlineStyle.strikethrough => const ['~~', '~'],
    };
  }
}

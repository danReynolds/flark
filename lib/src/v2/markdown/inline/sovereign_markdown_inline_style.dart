enum FlarkMarkdownInlineStyle { emphasis, strong, inlineCode, strikethrough }

extension FlarkMarkdownInlineStyleMarker on FlarkMarkdownInlineStyle {
  String get marker {
    return switch (this) {
      FlarkMarkdownInlineStyle.emphasis => '*',
      FlarkMarkdownInlineStyle.strong => '**',
      FlarkMarkdownInlineStyle.inlineCode => '`',
      FlarkMarkdownInlineStyle.strikethrough => '~~',
    };
  }
}

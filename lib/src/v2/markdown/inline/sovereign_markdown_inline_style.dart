enum SovereignMarkdownInlineStyle {
  emphasis,
  strong,
  inlineCode,
  strikethrough,
}

extension SovereignMarkdownInlineStyleMarker on SovereignMarkdownInlineStyle {
  String get marker {
    return switch (this) {
      SovereignMarkdownInlineStyle.emphasis => '*',
      SovereignMarkdownInlineStyle.strong => '**',
      SovereignMarkdownInlineStyle.inlineCode => '`',
      SovereignMarkdownInlineStyle.strikethrough => '~~',
    };
  }
}

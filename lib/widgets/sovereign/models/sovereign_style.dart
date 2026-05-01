/// Inline style kinds emitted by Sovereign syntax tokens.
enum SovereignStyleType {
  /// Bold emphasis.
  bold,

  /// Italic emphasis.
  italic,

  /// Inline code span.
  code,

  /// Link text.
  link,

  /// Image alt text.
  image,
}

/// Value-type defining a specific visual style.
class SovereignStyle {
  /// Style kinds included in this value.
  final Set<SovereignStyleType> types;

  /// Creates a style value from [types].
  const SovereignStyle(this.types);

  /// Bold inline text style.
  static const bold = SovereignStyle({SovereignStyleType.bold});

  /// Italic inline text style.
  static const italic = SovereignStyle({SovereignStyleType.italic});

  /// Inline code text style.
  static const code = SovereignStyle({SovereignStyleType.code});

  /// Link text style.
  static const link = SovereignStyle({SovereignStyleType.link});

  /// Image alt-text style.
  static const image = SovereignStyle({SovereignStyleType.image});

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is SovereignStyle && _setEquals(types, other.types);

  @override
  int get hashCode => Object.hashAllUnordered(types);

  bool _setEquals(Set a, Set b) {
    if (a.length != b.length) return false;
    return a.containsAll(b);
  }
}

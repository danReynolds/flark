enum SovereignStyleType { bold, italic, code, link, image }

/// Value-type defining a specific visual style.
class SovereignStyle {
  final Set<SovereignStyleType> types;

  const SovereignStyle(this.types);

  static const bold = SovereignStyle({SovereignStyleType.bold});
  static const italic = SovereignStyle({SovereignStyleType.italic});
  static const code = SovereignStyle({SovereignStyleType.code});
  static const link = SovereignStyle({SovereignStyleType.link});
  static const image = SovereignStyle({SovereignStyleType.image});

  // Helpers for merging?

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

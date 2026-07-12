/// CommonMark flanking checks for the emphasis-family delimiters (`*`, `_`,
/// `~~`), shared by every editing decision that reads or writes them.
///
/// CommonMark only recognizes an emphasis/strong/strikethrough delimiter when
/// its surroundings allow it: a closing delimiter must not be preceded by
/// whitespace, an opening delimiter must not be followed by whitespace, and
/// `_` additionally refuses to open or close inside a word. Editing code that
/// places markers without honoring these rules writes source that *looks*
/// styled to the editor but parses as literal text — the bug class these
/// predicates exist to make unrepresentable.
///
/// The character classes are pragmatic approximations of the spec's Unicode
/// definitions (whitespace beyond ASCII covers the common `Zs` code points;
/// punctuation covers ASCII). The authoritative validity oracle remains the
/// Comrak parse — these predicates only steer write-side placement, and the
/// round-trip test gates assert agreement with the real parser.
abstract final class FlarkInlineFlanking {
  /// Whether [codeUnit] is CommonMark "Unicode whitespace": space, tab, line
  /// feed, form feed, carriage return, or a common `Zs` space separator.
  static bool isUnicodeWhitespace(int codeUnit) {
    return switch (codeUnit) {
      0x20 || 0x09 || 0x0A || 0x0C || 0x0D => true,
      0xA0 || 0x1680 || 0x202F || 0x205F || 0x3000 => true,
      _ => codeUnit >= 0x2000 && codeUnit <= 0x200A,
    };
  }

  /// Whether [codeUnit] is ASCII punctuation (the spec's punctuation class,
  /// restricted to ASCII).
  static bool isPunctuation(int codeUnit) {
    return (codeUnit >= 0x21 && codeUnit <= 0x2F) ||
        (codeUnit >= 0x3A && codeUnit <= 0x40) ||
        (codeUnit >= 0x5B && codeUnit <= 0x60) ||
        (codeUnit >= 0x7B && codeUnit <= 0x7E);
  }

  /// Whether the delimiter occupying `[start, end)` of [source] is
  /// left-flanking: not followed by whitespace, and not followed by
  /// punctuation unless preceded by whitespace or punctuation.
  static bool isLeftFlanking(String source, int start, int end) {
    final after = end < source.length ? source.codeUnitAt(end) : null;
    if (after == null || isUnicodeWhitespace(after)) return false;
    if (!isPunctuation(after)) return true;
    final before = start > 0 ? source.codeUnitAt(start - 1) : null;
    return before == null ||
        isUnicodeWhitespace(before) ||
        isPunctuation(before);
  }

  /// Whether the delimiter occupying `[start, end)` of [source] is
  /// right-flanking: not preceded by whitespace, and not preceded by
  /// punctuation unless followed by whitespace or punctuation.
  static bool isRightFlanking(String source, int start, int end) {
    final before = start > 0 ? source.codeUnitAt(start - 1) : null;
    if (before == null || isUnicodeWhitespace(before)) return false;
    if (!isPunctuation(before)) return true;
    final after = end < source.length ? source.codeUnitAt(end) : null;
    return after == null || isUnicodeWhitespace(after) || isPunctuation(after);
  }

  /// Whether the delimiter at `[start, end)` can open a run.
  ///
  /// `*` and `~` open when left-flanking. `_` additionally must not sit
  /// intraword: it can open only when it is not right-flanking, or is preceded
  /// by punctuation.
  static bool canOpen(String source, int start, int end) {
    if (!isLeftFlanking(source, start, end)) return false;
    if (source.codeUnitAt(start) != 0x5F) return true;
    if (!isRightFlanking(source, start, end)) return true;
    final before = start > 0 ? source.codeUnitAt(start - 1) : null;
    return before != null && isPunctuation(before);
  }

  /// Whether the delimiter at `[start, end)` can close a run.
  ///
  /// `*` and `~` close when right-flanking. `_` additionally must not sit
  /// intraword: it can close only when it is not left-flanking, or is followed
  /// by punctuation.
  static bool canClose(String source, int start, int end) {
    if (!isRightFlanking(source, start, end)) return false;
    if (source.codeUnitAt(start) != 0x5F) return true;
    if (!isLeftFlanking(source, start, end)) return true;
    final after = end < source.length ? source.codeUnitAt(end) : null;
    return after != null && isPunctuation(after);
  }

  /// Whether the character at [offset] is preceded by an odd number of
  /// backslashes (i.e. is escaped).
  static bool isEscaped(String source, int offset) {
    var backslashes = 0;
    for (var cursor = offset - 1; cursor >= 0; cursor -= 1) {
      if (source.codeUnitAt(cursor) != 0x5C) break;
      backslashes += 1;
    }
    return backslashes.isOdd;
  }
}

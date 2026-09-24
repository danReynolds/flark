// Ported from CodeMirror 5.65.21 src/util/StringStream.js and
// src/util/misc.js (countColumn).
// CodeMirror, copyright (c) by Marijn Haverbeke and others.
// Distributed under an MIT license: https://codemirror.net/5/LICENSE

/// Lines around the one being tokenized, for modes that look ahead.
abstract interface class LineOracle {
  String? lookAhead(int n);
}

/// A line fed to a mode. Positions are UTF-16 code units, as in JavaScript,
/// and each character a method returns is one code unit.
///
/// A regular expression given to [match] must not start with `^`: it is
/// matched at [pos] as a prefix, which is what CodeMirror's anchored patterns
/// on the rest of the line mean.
final class StringStream {
  StringStream(this.string, [int? tabSize, this.lineOracle])
    : tabSize = tabSize == null || tabSize == 0 ? 8 : tabSize;

  final String string;
  final int tabSize;
  final LineOracle? lineOracle;
  int pos = 0, start = 0;
  int lastColumnPos = 0, lastColumnValue = 0;
  int lineStart = 0;

  bool eol() => pos >= string.length;
  bool sol() => pos == lineStart;
  String? peek() => pos < string.length ? string[pos] : null;
  String? next() => pos < string.length ? string[pos++] : null;

  /// [match] is a character, a [RegExp] tested against one character, or a
  /// predicate.
  String? eat(Object match) {
    if (pos >= string.length) return null;
    final ch = string[pos];
    final ok = switch (match) {
      String() => ch == match,
      RegExp() => match.hasMatch(ch),
      bool Function(String) test => test(ch),
      _ => throw ArgumentError.value(match, 'match'),
    };
    if (!ok) return null;
    pos++;
    return ch;
  }

  bool eatWhile(Object match) {
    final start = pos;
    while (eat(match) != null) {}
    return pos > start;
  }

  bool eatSpace() => eatWhileCode(isJsSpace);

  /// [eatWhile] with a test on code units, which allocates no characters.
  bool eatWhileCode(bool Function(int unit) test) {
    final start = pos;
    while (pos < string.length && test(string.codeUnitAt(pos))) {
      pos++;
    }
    return pos > start;
  }

  void skipToEnd() => pos = string.length;

  bool skipTo(String ch) {
    final found = string.indexOf(ch, pos);
    if (found < 0) return false;
    pos = found;
    return true;
  }

  void backUp(int n) => pos -= n;

  int column() {
    if (lastColumnPos < start) {
      lastColumnValue = countColumn(
        string,
        start,
        tabSize,
        lastColumnPos,
        lastColumnValue,
      );
      lastColumnPos = start;
    }
    return lastColumnValue -
        (lineStart != 0 ? countColumn(string, lineStart, tabSize) : 0);
  }

  int indentation() =>
      countColumn(string, null, tabSize) -
      (lineStart != 0 ? countColumn(string, lineStart, tabSize) : 0);

  /// CodeMirror's `match` for a string pattern: consumes it when the line
  /// continues with it.
  bool matchString(
    String pattern, {
    bool consume = true,
    bool caseInsensitive = false,
  }) {
    final end = pos + pattern.length;
    var substr = string.substring(
      pos,
      end > string.length ? string.length : end,
    );
    var wanted = pattern;
    if (caseInsensitive) {
      substr = substr.toLowerCase();
      wanted = wanted.toLowerCase();
    }
    if (substr != wanted) return false;
    if (consume) pos += pattern.length;
    return true;
  }

  /// CodeMirror's `match` for a regular expression anchored at [pos].
  Match? match(RegExp pattern, {bool consume = true}) {
    assert(!pattern.pattern.startsWith('^'), 'anchor is implied');
    final found = pattern.matchAsPrefix(string, pos);
    if (found != null && consume) pos = found.end;
    return found;
  }

  String current() => string.substring(start, pos);

  T hideFirstChars<T>(int n, T Function() inner) {
    lineStart += n;
    try {
      return inner();
    } finally {
      lineStart -= n;
    }
  }

  String? lookAhead(int n) => lineOracle?.lookAhead(n);
}

/// JavaScript's `\s` for one code unit: its white space and line
/// terminators, which include U+00A0.
bool isJsSpace(int u) =>
    u == 0x20 ||
    (u >= 0x09 && u <= 0x0d) ||
    u == 0xa0 ||
    u == 0x1680 ||
    (u >= 0x2000 && u <= 0x200a) ||
    u == 0x2028 ||
    u == 0x2029 ||
    u == 0x202f ||
    u == 0x205f ||
    u == 0x3000 ||
    u == 0xfeff;
final _nonSpace = RegExp(r'[^\s\u00a0]');

/// The column of [end] in [string], expanding tabs to [tabSize]. With a null
/// [end], the column of the first non-whitespace character.
int countColumn(
  String string,
  int? end,
  int tabSize, [
  int startIndex = 0,
  int startValue = 0,
]) {
  if (end == null) {
    end = string.indexOf(_nonSpace);
    if (end == -1) end = string.length;
  }
  for (var i = startIndex, n = startValue; ;) {
    final nextTab = string.indexOf('\t', i);
    if (nextTab < 0 || nextTab >= end) return n + (end - i);
    n += nextTab - i;
    n += tabSize - (n % tabSize);
    i = nextTab + 1;
  }
}

import 'stream.dart';

/// Editor options a mode reads when it is created: CodeMirror's `indentUnit`
/// and `tabSize`, in columns.
final class ModeConfig {
  const ModeConfig({this.indentUnit = 2, this.tabSize = 4});
  final int indentUnit, tabSize;
}

/// A CodeMirror mode: a line-based tokenizer with copyable state, as
/// CodeMirror 5 defines modes and CodeMirror 6 its stream parsers.
///
/// [token] returns the style of what it consumed: class names such as
/// `keyword` or CodeMirror 6 tag names such as `variableName.special`, or
/// null for none. It may consume nothing to change state; [readToken] calls
/// it again, as the editors do. [indent] returns the column for a line that
/// starts with `textAfter`, or null where CodeMirror has no answer.
abstract class Mode<S> {
  Mode([this.config = const ModeConfig()]);
  final ModeConfig config;

  S startState([int baseColumn = 0]);
  String? token(StringStream stream, S state);
  S copyState(S state);

  /// Called for an empty line, which [token] never sees.
  void blankLine(S state) {}

  /// Whether the mode computes indentation; CodeMirror copies the previous
  /// line's when it does not.
  bool get hasIndent => false;
  int? indent(S state, String textAfter, String line) => null;

  /// Typing a line prefix that matches this re-indents the line.
  RegExp? get electricInput => null;

  /// Typing any of these characters re-indents the line.
  String? get electricChars => null;
}

/// The next token's style, read as CodeMirror's editors do: a mode may
/// return without consuming anything, to change state, up to ten times.
///
/// A mode that still has not advanced throws upstream, which ends
/// highlighting for the whole document. Here the stream steps over one
/// character, a surrogate pair whole, with the style the mode last gave:
/// Rust's comment state reads only `.*`, which cannot match U+2028, and that
/// one character must not cost the snippet its colors or the host its
/// frame. Wherever upstream advances, the tokens are the same.
String? readToken<S>(Mode<S> mode, StringStream stream, S state) {
  stream.start = stream.pos;
  String? style;
  for (var i = 0; i < 10; i++) {
    style = mode.token(stream, state);
    if (stream.pos > stream.start) return style;
  }
  final string = stream.string, at = stream.start;
  final pair =
      at + 1 < string.length &&
      (string.codeUnitAt(at) & 0xfc00) == 0xd800 &&
      (string.codeUnitAt(at + 1) & 0xfc00) == 0xdc00;
  stream.pos = at + (pair ? 2 : 1);
  return style;
}

/// A stream over [line] with [mode]'s tab size and indent unit.
StringStream streamFor(Mode<Object?> mode, String line, [LineOracle? oracle]) =>
    StringStream.withUnit(
      line,
      mode.config.tabSize,
      oracle,
      indentUnit: mode.config.indentUnit,
    );

/// CodeMirror's line splitting: CRLF, CR and LF.
final lineBreak = RegExp('\r\n?|\n');

/// [text]'s lines and the offset where each starts.
({List<String> lines, List<int> starts}) splitLines(String text) {
  final lines = <String>[], starts = <int>[];
  var start = 0;
  for (final match in lineBreak.allMatches(text)) {
    lines.add(text.substring(start, match.start));
    starts.add(start);
    start = match.end;
  }
  lines.add(text.substring(start));
  starts.add(start);
  return (lines: lines, starts: starts);
}

/// Runs [mode] over [text] as CodeMirror's `runMode` does, calling [onToken]
/// with each token's line, start, end and style. [onLine] sees each line with
/// the state before it, which callers may copy but must not advance.
S runMode<S>(
  Mode<S> mode,
  String text, {
  S? state,
  void Function(int line, int start, int end, String? style)? onToken,
  void Function(int line, String text, S state)? onLine,
}) {
  final lines = splitLines(text).lines;
  final current = state ?? mode.startState();
  final oracle = LineList(lines);
  for (var i = 0; i < lines.length; i++) {
    onLine?.call(i, lines[i], current);
    oracle.line = i;
    final stream = streamFor(mode, lines[i], oracle);
    if (stream.string.isEmpty) mode.blankLine(current);
    while (!stream.eol()) {
      final style = readToken(mode, stream, current);
      onToken?.call(i, stream.start, stream.pos, style);
    }
  }
  return current;
}

/// Look-ahead over split lines; [line] is the one being tokenized.
final class LineList implements LineOracle {
  LineList(this.lines);
  final List<String> lines;
  int line = 0;
  @override
  String? lookAhead(int n) {
    final i = line + n;
    return i >= 0 && i < lines.length ? lines[i] : null;
  }
}

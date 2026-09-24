import 'stream.dart';

/// Editor options a mode reads when it is created: CodeMirror's `indentUnit`
/// and `tabSize`, in columns.
final class ModeConfig {
  const ModeConfig({this.indentUnit = 2, this.tabSize = 4});
  final int indentUnit, tabSize;
}

/// A CodeMirror 5 mode: a line-based tokenizer with copyable state.
///
/// [token] consumes at least one character from the stream and returns its
/// style: CodeMirror's space-separated class names, or null for none.
/// [indent] returns the column for a line that starts with `textAfter`, or
/// null where CodeMirror returns `CodeMirror.Pass`.
abstract class Mode<S> {
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
  int tabSize = 4,
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
    final stream = StringStream(lines[i], tabSize, oracle);
    if (stream.string.isEmpty) mode.blankLine(current);
    while (!stream.eol()) {
      final style = mode.token(stream, current);
      onToken?.call(i, stream.start, stream.pos, style);
      stream.start = stream.pos;
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

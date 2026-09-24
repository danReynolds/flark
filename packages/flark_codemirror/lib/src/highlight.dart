import 'package:flark/code.dart';

import 'mode.dart';
import 'stream.dart';

/// Tokens of [source] under [mode] as Flark code tokens: code-body UTF-16
/// ranges that tile the source, with a kind from the vocabulary hosts theme
/// ([codeSyntaxRole]) or null. Adjacent tokens of one kind are merged, as
/// CodeMirror's `flattenSpans` does.
List<CodeToken> codeMirrorTokens(
  Mode<Object?> mode,
  String source, {
  int tabSize = 4,
}) {
  final tokens = <CodeToken>[];
  // The pending run: where it starts and its kind.
  var start = 0;
  String? kind;
  void add(int at, String? next) {
    if (next == kind) return;
    if (at > start) {
      tokens.add(CodeToken(start, at, kind));
      start = at;
    }
    kind = next;
  }

  final state = mode.startState();
  final (:lines, :starts) = splitLines(source);
  final oracle = LineList(lines);
  for (var i = 0; i < lines.length; i++) {
    final line = lines[i], offset = starts[i];
    // The line break before this line is unstyled.
    if (i > 0) add(starts[i - 1] + lines[i - 1].length, null);
    oracle.line = i;
    final stream = StringStream(line, tabSize, oracle);
    if (line.isEmpty) mode.blankLine(state);
    while (!stream.eol()) {
      final style = mode.token(stream, state);
      add(
        offset + stream.start,
        style == null
            ? null
            : codeMirrorKind(style, line, stream.start, stream.pos),
      );
      stream.start = stream.pos;
    }
  }
  if (source.length > start || tokens.isEmpty) {
    tokens.add(CodeToken(start, source.length, kind));
  }
  return tokens;
}

final _constantName = RegExp(r'^_*[A-Z][A-Z\d_]+$');
final _capitalized = RegExp(r'^[A-Z]');

/// The Flark kind for a CodeMirror style. CodeMirror styles definitions,
/// variables and properties alike; like Flark's previous highlighter, a name
/// followed by `(` is a function, a capitalized one a constructor and an
/// all-caps one a constant.
String? codeMirrorKind(String style, String line, int start, int end) {
  switch (style) {
    case 'keyword':
      return 'keyword';
    case 'atom':
      return 'constant';
    case 'number' || 'number property':
      return 'number';
    case 'string' || 'string-2' || 'string property':
      return 'string';
    case 'comment':
      return 'comment';
    case 'meta':
      return 'meta';
    case 'type' || 'variable-3':
      return 'type';
    case 'operator':
      return 'operator';
    case 'def' || 'variable' || 'variable-2' || 'property':
      final name = line.substring(start, end);
      if (_constantName.hasMatch(name) && style != 'property') {
        return 'constant';
      }
      if (_calls(line, end)) return 'function';
      if (style == 'property') return 'property';
      if (_capitalized.hasMatch(name)) return 'constructor';
      return 'variable';
  }
  return null;
}

/// Whether the rest of [line] after [end] starts with a call's `(`, past
/// spaces and type arguments.
bool _calls(String line, int end) {
  var i = end;
  while (i < line.length &&
      (line.codeUnitAt(i) == 32 || line.codeUnitAt(i) == 9)) {
    i++;
  }
  if (i < line.length && line.codeUnitAt(i) == 0x3c) {
    var depth = 0;
    for (; i < line.length; i++) {
      final unit = line.codeUnitAt(i);
      if (unit == 0x3c) depth++;
      if (unit == 0x3e && --depth == 0) {
        i++;
        break;
      }
      if (unit == 0x28 || unit == 0x3b || unit == 0x7b) return false;
    }
  }
  return i < line.length && line.codeUnitAt(i) == 0x28;
}

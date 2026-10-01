import 'package:flark/code.dart';

import 'mode.dart';

/// Tokens of [source] under [mode] as Flark code tokens: code-body UTF-16
/// ranges that tile the source, with a kind from the vocabulary hosts theme
/// ([codeSyntaxRole]) or null. Adjacent tokens of one kind are merged, as
/// CodeMirror's `flattenSpans` does.
///
/// Hosts call this while laying out a frame, so it never throws: should a
/// mode fail, the tokens it read stand and the rest of the snippet is one
/// plain token.
List<CodeToken> codeMirrorTokens(Mode<Object?> mode, String source) {
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

  // The end of the last token read, where a failing mode's plain rest starts.
  var read = 0;
  try {
    final state = mode.startState();
    final (:lines, :starts) = splitLines(source);
    final oracle = LineList(lines);
    for (var i = 0; i < lines.length; i++) {
      final line = lines[i], offset = starts[i];
      // The line break before this line is unstyled.
      if (i > 0) add(starts[i - 1] + lines[i - 1].length, null);
      read = offset;
      oracle.line = i;
      final stream = streamFor(mode, line, oracle);
      if (line.isEmpty) mode.blankLine(state);
      while (!stream.eol()) {
        final style = readToken(mode, stream, state);
        add(
          offset + stream.start,
          style == null
              ? null
              : codeMirrorKind(style, line, stream.start, stream.pos),
        );
        read = offset + stream.pos;
      }
    }
  } on Object {
    add(read, null);
  }
  if (source.length > start || tokens.isEmpty) {
    tokens.add(CodeToken(start, source.length, kind));
  }
  return tokens;
}

final _constantName = RegExp(r'^_*[A-Z][A-Z\d_]+$');
final _capitalized = RegExp(r'^[A-Z]');

/// Kinds for CodeMirror's style names: CodeMirror 5's classes and the tag
/// names CodeMirror 6's stream parsers return. Names are resolved by
/// [codeMirrorKind]; those absent here have no kind.
const _kinds = {
  'keyword': 'keyword',
  'operator': 'operator',
  'atom': 'constant',
  'bool': 'constant',
  'null': 'constant',
  'number': 'number',
  'string': 'string',
  'string-2': 'string',
  'string.special': 'string',
  'character': 'string',
  'link': 'string',
  'comment': 'comment',
  'quote': 'comment',
  'meta': 'meta',
  'macroName': 'meta',
  'header': 'keyword',
  'heading': 'keyword',
  'type': 'type',
  'typeName': 'type',
  'variable-3': 'type',
  'className': 'type',
  'namespace': 'type',
  'variableName.function': 'function',
  'tag': 'tag',
  'tagName': 'tag',
  'attribute': 'attribute',
  'attributeName': 'attribute',
  'qualifier': 'selector-class',
  'self': 'variable',
  'labelName': 'variable',
};

/// Styles whose kind depends on the name: definitions, variables,
/// properties and builtins.
const _names = {
  'def',
  'variable',
  'variable-2',
  'variableName',
  'variableName.special',
  'variableName.definition',
  'property',
  'propertyName',
  'builtin',
};

/// The Flark kind for a CodeMirror style. CodeMirror styles definitions,
/// variables and properties alike; like Flark's previous highlighter, a name
/// followed by `(` is a function, a capitalized one a constructor and an
/// all-caps one a constant. A builtin is a function when called and a type
/// otherwise. A style of several names (`string property`) takes the first
/// with a kind, ignoring `error`.
String? codeMirrorKind(String style, String line, int start, int end) {
  if (style.contains(' ')) {
    for (final part in style.split(' ')) {
      final kind = part == 'error' || part == 'invalid'
          ? null
          : codeMirrorKind(part, line, start, end);
      if (kind != null) return kind;
    }
    return null;
  }
  final kind = _kinds[style];
  if (kind != null) return kind;
  if (!_names.contains(style)) return null;
  final name = line.substring(start, end);
  final property = style == 'property' || style == 'propertyName';
  if (style == 'builtin') return _calls(line, end) ? 'function' : 'type';
  if (_constantName.hasMatch(name) && !property) return 'constant';
  if (_calls(line, end)) return 'function';
  if (property) return 'property';
  if (_capitalized.hasMatch(name)) return 'constructor';
  return 'variable';
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
    i = _TypeArguments.of(line).after(i);
    if (i < 0) return false;
  }
  return i < line.length && line.codeUnitAt(i) == 0x28;
}

/// Where the type arguments opened by each `<` of a line end: just past the
/// `>` that closes it, or -1 when a `(`, `;` or `{` comes first or none does.
/// Matched in one pass and kept for the line being highlighted, since every
/// name before a `<` asks: scanning ahead per name made a run of unclosed
/// `<` (`a<b<c…`) quadratic in the line.
final class _TypeArguments {
  _TypeArguments._(this._line, this._ends);

  final String _line;
  final Map<int, int> _ends;

  static _TypeArguments? _last;

  static _TypeArguments of(String line) {
    final last = _last;
    if (last != null && identical(last._line, line)) return last;
    final ends = <int, int>{};
    final open = <int>[];
    for (var i = 0; i < line.length; i++) {
      final unit = line.codeUnitAt(i);
      if (unit == 0x3c) {
        open.add(i);
      } else if (unit == 0x3e) {
        if (open.isNotEmpty) ends[open.removeLast()] = i + 1;
      } else if (unit == 0x28 || unit == 0x3b || unit == 0x7b) {
        // Every `<` still open meets this stop before its `>`.
        for (final at in open) {
          ends[at] = -1;
        }
        open.clear();
      }
    }
    for (final at in open) {
      ends[at] = -1;
    }
    return _last = _TypeArguments._(line, ends);
  }

  /// The end of the type arguments the `<` at [open] starts.
  int after(int open) => _ends[open]!;
}

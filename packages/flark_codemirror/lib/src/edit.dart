import 'package:flark/code.dart';

import 'mode.dart';
import 'stream.dart';

/// Columns a tab spans when indentation is measured, as CodeMirror's default.
const codeTabSize = 4;

/// CodeMirror gives up on indentation past this column and keeps the
/// previous line's.
const _maxIndent = 150;

final _leadingSpace = RegExp(r'^\s*');
final _closerLine = RegExp(r'^\s*[\)\]]$');

/// An edit for [action] at [base]/[extent] in [source], in code-body UTF-16
/// coordinates, or null when the request is malformed.
///
/// Enter indents the new line as CodeMirror's `newlineAndIndent` does, from
/// the mode's indentation for the state before it; between `[]` or `{}` it
/// opens an empty line and indents both, as CodeMirror's `closebrackets`
/// addon does, unless the opener is inside a string or comment. Enter on an
/// indented empty line keeps its exact whitespace. Typing re-indents the line
/// when the mode's electric input matches it, or when a closing `)` or `]`
/// starts it, which the modes indent like braces. Tab and Shift-Tab shift
/// whole lines by [indentUnit], or insert it at a caret.
CodeEditProposal? proposeCodeEdit(
  Mode<Object?> Function(int indentColumns) modeFor,
  String source, {
  required int base,
  required int extent,
  required CodeEditingAction action,
  required String text,
  required String indentUnit,
}) {
  if (!_validPosition(source, base) || !_validPosition(source, extent)) {
    return null;
  }
  if (!(indentUnit == '\t' ||
      (indentUnit.isNotEmpty &&
          indentUnit.length <= 8 &&
          indentUnit.codeUnits.every((u) => u == 32)))) {
    return null;
  }
  if (action != CodeEditingAction.insert && text.isNotEmpty) return null;
  final columns = indentUnit == '\t' ? codeTabSize : indentUnit.length;
  final start = base < extent ? base : extent;
  final end = base < extent ? extent : base;
  switch (action) {
    case CodeEditingAction.indent || CodeEditingAction.outdent:
      return _shift(
        source,
        base,
        extent,
        action == CodeEditingAction.indent,
        indentUnit,
      );
    case CodeEditingAction.newline:
      return _newline(modeFor(columns), source, start, end, indentUnit);
    case CodeEditingAction.insert:
      return _insert(modeFor(columns), source, start, end, text, indentUnit);
  }
}

bool _validPosition(String source, int position) =>
    position >= 0 &&
    position <= source.length &&
    (position == 0 ||
        position == source.length ||
        !(_isLead(source.codeUnitAt(position - 1)) &&
                _isTrail(source.codeUnitAt(position))) &&
            !(source.codeUnitAt(position - 1) == 13 &&
                source.codeUnitAt(position) == 10));

bool _isLead(int unit) => unit >= 0xd800 && unit <= 0xdbff;
bool _isTrail(int unit) => unit >= 0xdc00 && unit <= 0xdfff;

int _lineStart(String s, int p) {
  final i = p == 0 ? -1 : s.lastIndexOf('\n', p - 1);
  return i + 1;
}

int _lineEnd(String s, int p) {
  for (var i = p; i < s.length; i++) {
    final unit = s.codeUnitAt(i);
    if (unit == 10 || unit == 13) return i;
  }
  return s.length;
}

/// The state before the line that starts at [lineStart] in [text].
Object? _stateBefore(Mode<Object?> mode, String text, int lineStart) =>
    lineStart == 0
    ? mode.startState()
    : runMode(mode, text.substring(0, lineStart - 1), tabSize: codeTabSize);

/// Indentation for [column]: spaces, or tabs then spaces when the snippet
/// indents with tabs, as CodeMirror's `indentWithTabs`.
String _indentation(int column, String unit) {
  if (unit != '\t') return ' ' * column;
  return '\t' * (column ~/ codeTabSize) + ' ' * (column % codeTabSize);
}

/// CodeMirror's smart indentation for a line that starts with [textAfter]
/// after [state], or null where CodeMirror falls back.
int? _smart(Mode<Object?> mode, Object? state, String textAfter, String line) {
  if (!mode.hasIndent) return null;
  final column = mode.indent(mode.copyState(state), textAfter, line);
  return column == null || column > _maxIndent ? null : column;
}

CodeEditProposal _newline(
  Mode<Object?> mode,
  String source,
  int start,
  int end,
  String unit,
) {
  final line = _lineStart(source, start);
  final before = source.substring(line, start);
  final lineEnd = _lineEnd(source, end);
  final rest = source.substring(end, lineEnd < end ? end : lineEnd);
  final restSpace = _leadingSpace.firstMatch(rest)![0]!;
  final textAfter = rest.substring(restSpace.length);
  // The caret line as Enter leaves it, then the state after it.
  final head = source.substring(0, start);
  final state = runMode(mode, head, tabSize: codeTabSize);
  final previous = codeLeadingWhitespace(before);
  String indentFor(Object? state, String after) {
    final column = _smart(mode, state, after, after);
    return column == null ? previous : _indentation(column, unit);
  }

  if (start == end &&
      start > 0 &&
      end < source.length &&
      _explodes(source[start - 1], source[end]) &&
      !_literalAt(mode, source, start - 1)) {
    final middle = indentFor(state, '');
    // The closer's line follows an empty one.
    final blank = runMode(mode, '$head\n', tabSize: codeTabSize);
    final closer = indentFor(blank, textAfter);
    final inserted = '\n$middle\n$closer';
    final caret = start + 1 + middle.length;
    return CodeEditProposal(start, end, inserted, caret, caret);
  }
  final indent = before.isNotEmpty && previous == before && textAfter.isEmpty
      ? before
      : indentFor(state, textAfter);
  final caret = start + 1 + indent.length;
  return CodeEditProposal(
    start,
    end + restSpace.length,
    '\n$indent',
    caret,
    caret,
  );
}

/// CodeMirror's `closebrackets` explodes `[]` and `{}` by default.
bool _explodes(String open, String close) =>
    open == '[' && close == ']' || open == '{' && close == '}';

/// Whether the character at [offset] is inside a string, comment or other
/// styled literal token rather than code punctuation.
bool _literalAt(Mode<Object?> mode, String source, int offset) {
  final line = _lineStart(source, offset);
  final state = _stateBefore(mode, source, line);
  final text = source.substring(line, _lineEnd(source, offset));
  final stream = StringStream(text, codeTabSize);
  final column = offset - line;
  while (!stream.eol()) {
    final style = mode.token(stream, state);
    if (stream.pos > column) {
      return style != null &&
          (style.contains('string') ||
              style.contains('comment') ||
              style.contains('regexp'));
    }
    stream.start = stream.pos;
  }
  return false;
}

CodeEditProposal? _insert(
  Mode<Object?> mode,
  String source,
  int start,
  int end,
  String text,
  String unit,
) {
  final candidate = source.replaceRange(start, end, text);
  final caret = start + text.length;
  final plain = CodeEditProposal(start, end, text, caret, caret);
  final electric = mode.electricInput, chars = mode.electricChars;
  if (text.contains('\n') || text.contains('\r')) return plain;
  final line = _lineStart(candidate, caret);
  // CodeMirror skips electric input past column 100.
  if (caret - line > 100) return plain;
  final upToCaret = candidate.substring(line, caret);
  final triggered =
      (chars != null
          ? text.split('').any(chars.contains)
          : electric != null && electric.hasMatch(upToCaret)) ||
      mode.hasIndent && _closerLine.hasMatch(upToCaret);
  if (!triggered) return plain;
  final lineText = candidate.substring(line, _lineEnd(candidate, caret));
  final space = _leadingSpace.firstMatch(lineText)![0]!;
  if (space.length > upToCaret.length) return plain;
  final column = _smart(
    mode,
    _stateBefore(mode, candidate, line),
    lineText.substring(space.length),
    lineText,
  );
  if (column == null) return plain;
  final indent = _indentation(column, unit);
  if (indent == space) return plain;
  final replaced = indent + candidate.substring(line + space.length, caret);
  return CodeEditProposal(
    line,
    end,
    replaced,
    line + replaced.length,
    line + replaced.length,
  );
}

/// Tab and Shift-Tab over whole lines: one unit added, or one tab or up to a
/// unit of spaces removed. A caret with Tab inserts the unit.
CodeEditProposal _shift(
  String source,
  int base,
  int extent,
  bool indent,
  String unit,
) {
  final start = base < extent ? base : extent;
  final end = base < extent ? extent : base;
  if (base == extent && indent) {
    return CodeEditProposal(
      base,
      base,
      unit,
      base + unit.length,
      base + unit.length,
    );
  }
  final first = _lineStart(source, start);
  final last = end > start && _lineStart(source, end) == end
      ? _lineStart(source, end - 1)
      : _lineStart(source, end);
  final stop = _lineEnd(source, last);
  final changes = <(int, int, String)>[];
  var line = first;
  while (line <= last) {
    final space = codeLeadingWhitespace(
      source.substring(line, _lineEnd(source, line)),
    );
    final removed = indent
        ? 0
        : space.startsWith('\t')
        ? 1
        : space.length < (unit == '\t' ? 4 : unit.length)
        ? space.length
        : (unit == '\t' ? 4 : unit.length);
    changes.add((line, removed, indent ? unit : ''));
    final next = source.indexOf('\n', line);
    line = next < 0 ? last + 1 : next + 1;
  }
  final buffer = StringBuffer();
  var at = first;
  for (final (position, removed, inserted) in changes) {
    buffer
      ..write(source.substring(at, position))
      ..write(inserted);
    at = position + removed;
  }
  buffer.write(source.substring(at, stop));
  int mapped(int position) {
    var delta = 0;
    for (final (at, removed, inserted) in changes) {
      if (position < at) break;
      if (position <= at + removed) return at + delta + inserted.length;
      delta += inserted.length - removed;
    }
    return position + delta;
  }

  return CodeEditProposal(
    first,
    stop,
    buffer.toString(),
    mapped(base),
    mapped(extent),
  );
}

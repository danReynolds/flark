import '../../core/core.dart';
import 'flark_markdown_editing_result.dart';
import 'flark_markdown_fenced_code_policy.dart';

final class FlarkMarkdownInputEngine {
  const FlarkMarkdownInputEngine._();

  static FlarkMarkdownSourceEdit enter({
    required String markdown,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed) {
      return _sourceEdit(
        range: FlarkSourceRange(selection.start, selection.end),
        replacementText: '\n',
      );
    }

    final line = _lineAtSelection(markdown, selection);
    final beforeCaret = markdown.substring(line.start, selection.start);
    final quotePrefix = _quotePrefix(beforeCaret);
    final content = beforeCaret.substring(quotePrefix.length);

    final fencedCodeEnter = FlarkMarkdownFencedCodePolicy.enter(
      markdown: markdown,
      caret: selection.start,
    );
    if (fencedCodeEnter != null) return fencedCodeEnter;

    final headingExit = _emptyHeadingExit(
      markdown: markdown,
      selection: selection,
      line: line,
      beforeCaret: beforeCaret,
      quotePrefix: quotePrefix,
    );
    if (headingExit != null) return headingExit;

    final list = _ListContinuation.tryParse(content);
    if (list != null) {
      return _handleListEnter(
        selection: selection,
        line: line,
        quotePrefix: quotePrefix,
        list: list,
      );
    }

    final indentedCode = _indentedCodeContinuation(
      markdown: markdown,
      selection: selection,
      line: line,
      beforeCaret: beforeCaret,
      quotePrefix: quotePrefix,
    );
    if (indentedCode != null) return indentedCode;

    if (quotePrefix.isNotEmpty) {
      final replacement = content.trim().isEmpty ? '\n' : '\n$quotePrefix';
      final range = content.trim().isEmpty
          ? FlarkSourceRange(line.start, line.end)
          : FlarkSourceRange(selection.start, selection.start);
      return _sourceEdit(range: range, replacementText: replacement);
    }

    return _sourceEdit(
      range: FlarkSourceRange(selection.start, selection.start),
      replacementText: '\n',
    );
  }

  static FlarkMarkdownInputResult? backspace({
    required String markdown,
    required FlarkSelection selection,
  }) {
    if (!selection.isCollapsed) {
      return _sourceEdit(
        range: FlarkSourceRange(selection.start, selection.end),
        replacementText: '',
      );
    }
    if (selection.start == 0) return null;

    final fencedCodeBackspace = FlarkMarkdownFencedCodePolicy.backspace(
      markdown: markdown,
      caret: selection.start,
    );
    if (fencedCodeBackspace != null) return fencedCodeBackspace;

    final line = _lineAtSelection(markdown, selection);
    final beforeCaret = markdown.substring(line.start, selection.start);
    final quotePrefix = _quotePrefix(beforeCaret);
    final content = beforeCaret.substring(quotePrefix.length);

    final listBackspace = _listBoundaryBackspace(
      line: line,
      quotePrefix: quotePrefix,
      list: _ListContinuation.tryParse(content),
    );
    if (listBackspace != null) return listBackspace;

    final headingBackspace = _headingBoundaryBackspace(
      line: line,
      beforeCaret: beforeCaret,
      quotePrefix: quotePrefix,
      selection: selection,
    );
    if (headingBackspace != null) return headingBackspace;

    final indentedCodeBackspace = _indentedCodeBackspace(
      beforeCaret: beforeCaret,
      quotePrefix: quotePrefix,
      selection: selection,
    );
    if (indentedCodeBackspace != null) return indentedCodeBackspace;

    final quoteBoundaryBackspace = _quoteBoundaryBackspace(
      markdown: markdown,
      selection: selection,
      line: line,
      beforeCaret: beforeCaret,
      quotePrefix: quotePrefix,
    );
    if (quoteBoundaryBackspace != null) return quoteBoundaryBackspace;

    return _sourceEdit(
      range: FlarkSourceRange(selection.start - 1, selection.start),
      replacementText: '',
    );
  }
}

FlarkMarkdownSourceEdit _sourceEdit({
  required FlarkSourceRange range,
  required String replacementText,
  FlarkSelection? selectionAfter,
}) {
  return FlarkMarkdownSourceEdit(
    range: range,
    replacementText: replacementText,
    selectionAfter:
        selectionAfter ??
        FlarkSelection.collapsed(range.start + replacementText.length),
  );
}

FlarkMarkdownSourceEdit _handleListEnter({
  required FlarkSelection selection,
  required _Line line,
  required String quotePrefix,
  required _ListContinuation list,
}) {
  if (list.isEmptyItem) {
    if (quotePrefix.isEmpty) {
      return _sourceEdit(
        range: FlarkSourceRange(line.start, line.end),
        replacementText: '${list.indent}\n',
      );
    }
    return _sourceEdit(
      range: FlarkSourceRange(line.start + quotePrefix.length, line.end),
      replacementText: '${list.indent}\n$quotePrefix${list.indent}',
    );
  }

  return _sourceEdit(
    range: FlarkSourceRange(selection.start, selection.start),
    replacementText: '\n$quotePrefix${list.indent}${list.nextMarker}',
  );
}

FlarkMarkdownSourceEdit? _emptyHeadingExit({
  required String markdown,
  required FlarkSelection selection,
  required _Line line,
  required String beforeCaret,
  required String quotePrefix,
}) {
  if (quotePrefix.isNotEmpty) return null;
  final afterCaret = markdown.substring(selection.start, line.end);
  if (afterCaret.trim().isNotEmpty) return null;

  final headingIndent = _emptyHeadingIndent(beforeCaret);
  if (headingIndent == null) return null;

  return _sourceEdit(
    range: FlarkSourceRange(line.start, line.end),
    replacementText: '$headingIndent\n',
  );
}

FlarkMarkdownSourceEdit? _indentedCodeContinuation({
  required String markdown,
  required FlarkSelection selection,
  required _Line line,
  required String beforeCaret,
  required String quotePrefix,
}) {
  if (quotePrefix.isNotEmpty) return null;
  final codeIndent = _codeIndent(beforeCaret);
  if (codeIndent == null) return null;

  final afterCaret = markdown.substring(selection.start, line.end);
  final body = beforeCaret.substring(codeIndent.length) + afterCaret;
  if (body.trim().isEmpty) {
    return _sourceEdit(
      range: FlarkSourceRange(line.start, line.end),
      replacementText: '\n',
    );
  }

  return _sourceEdit(
    range: FlarkSourceRange(selection.start, selection.start),
    replacementText: '\n$codeIndent',
  );
}

FlarkMarkdownSourceEdit? _listBoundaryBackspace({
  required _Line line,
  required String quotePrefix,
  required _ListContinuation? list,
}) {
  if (list == null || list.body.isNotEmpty) return null;

  final markerStart = line.start + quotePrefix.length + list.indent.length;
  if (list.taskMarker != null) {
    return _sourceEdit(
      range: FlarkSourceRange(
        markerStart + list.marker.length,
        markerStart + list.marker.length + list.taskMarker!.length,
      ),
      replacementText: '',
    );
  }

  return _sourceEdit(
    range: FlarkSourceRange(markerStart, markerStart + list.marker.length),
    replacementText: '',
  );
}

FlarkMarkdownSourceEdit? _headingBoundaryBackspace({
  required _Line line,
  required String beforeCaret,
  required String quotePrefix,
  required FlarkSelection selection,
}) {
  if (quotePrefix.isNotEmpty) return null;
  final headingIndent = _emptyHeadingIndent(beforeCaret);
  if (headingIndent == null) return null;

  final markerStart = line.start + headingIndent.length;
  return _sourceEdit(
    range: FlarkSourceRange(markerStart, selection.start),
    replacementText: '',
  );
}

FlarkMarkdownSourceEdit? _indentedCodeBackspace({
  required String beforeCaret,
  required String quotePrefix,
  required FlarkSelection selection,
}) {
  if (quotePrefix.isNotEmpty) return null;
  if (beforeCaret.isEmpty || beforeCaret.trim().isNotEmpty) return null;

  final caret = selection.start;
  if (beforeCaret.endsWith('\t')) {
    return _sourceEdit(
      range: FlarkSourceRange(caret - 1, caret),
      replacementText: '',
    );
  }

  final removableSpaces = beforeCaret.length % 4 == 0
      ? 4
      : beforeCaret.length % 4;
  if (removableSpaces <= 1 || beforeCaret.length < 4) return null;

  return _sourceEdit(
    range: FlarkSourceRange(caret - removableSpaces, caret),
    replacementText: '',
  );
}

FlarkMarkdownSourceEdit? _quoteBoundaryBackspace({
  required String markdown,
  required FlarkSelection selection,
  required _Line line,
  required String beforeCaret,
  required String quotePrefix,
}) {
  if (quotePrefix.isEmpty) return null;
  final contentBeforeCaret = beforeCaret.substring(quotePrefix.length);
  if (contentBeforeCaret.trim().isNotEmpty) return null;

  final afterCaret = markdown.substring(selection.start, line.end);
  final markerRange = _lastQuoteMarkerRange(line.start, quotePrefix);
  if (markerRange == null) return null;
  if (afterCaret.trim().isNotEmpty || _quoteDepth(quotePrefix) > 1) {
    return _sourceEdit(range: markerRange, replacementText: '');
  }

  return _sourceEdit(
    range: FlarkSourceRange(line.start, line.end),
    replacementText: '',
  );
}

FlarkSourceRange? _lastQuoteMarkerRange(int lineStart, String quotePrefix) {
  final markerStart = quotePrefix.lastIndexOf('>');
  if (markerStart < 0) return null;
  var markerEnd = markerStart + 1;
  if (markerEnd < quotePrefix.length &&
      quotePrefix.codeUnitAt(markerEnd) == 32 &&
      (markerStart == 0 || quotePrefix.codeUnitAt(markerStart - 1) == 32)) {
    markerEnd++;
  }
  return FlarkSourceRange(lineStart + markerStart, lineStart + markerEnd);
}

int _quoteDepth(String quotePrefix) {
  var depth = 0;
  for (var index = 0; index < quotePrefix.length; index++) {
    if (quotePrefix.codeUnitAt(index) == 62) depth++;
  }
  return depth;
}

final class _Line {
  const _Line({required this.start, required this.end});

  final int start;
  final int end;
}

final class _ListContinuation {
  const _ListContinuation({
    required this.indent,
    required this.marker,
    required this.taskMarker,
    required this.body,
    required this.orderedNumber,
    required this.orderedDelimiter,
    required this.orderedPadding,
  });

  final String indent;
  final String marker;
  final String? taskMarker;
  final String body;
  final int? orderedNumber;
  final String? orderedDelimiter;
  final String? orderedPadding;

  bool get isEmptyItem => body.trim().isEmpty;

  bool get isTask => taskMarker != null;

  String get nextMarker {
    final task = isTask ? '[ ] ' : '';
    final number = orderedNumber;
    if (number == null) return '$marker$task';
    return '${number + 1}$orderedDelimiter$orderedPadding$task';
  }

  static _ListContinuation? tryParse(String content) {
    final indentEnd = _horizontalWhitespaceEnd(content, 0);
    if (indentEnd >= content.length) return null;

    final markerCodeUnit = content.codeUnitAt(indentEnd);
    if (markerCodeUnit == 45 || markerCodeUnit == 43 || markerCodeUnit == 42) {
      var index = indentEnd + 1;
      if (index >= content.length ||
          !_isHorizontalWhitespaceCodeUnit(content.codeUnitAt(index))) {
        return null;
      }
      index = _horizontalWhitespaceEnd(content, index);
      final taskMarker = _parseTaskMarker(content, index);
      if (taskMarker != null) index = taskMarker.end;
      return _ListContinuation(
        indent: content.substring(0, indentEnd),
        marker: content.substring(indentEnd, taskMarker?.start ?? index),
        taskMarker: taskMarker?.text,
        body: content.substring(index),
        orderedNumber: null,
        orderedDelimiter: null,
        orderedPadding: null,
      );
    }

    if (!_isDigitCodeUnit(markerCodeUnit)) return null;
    var digitEnd = indentEnd + 1;
    while (digitEnd < content.length &&
        digitEnd - indentEnd < 9 &&
        _isDigitCodeUnit(content.codeUnitAt(digitEnd))) {
      digitEnd++;
    }
    if (digitEnd < content.length &&
        _isDigitCodeUnit(content.codeUnitAt(digitEnd))) {
      return null;
    }
    if (digitEnd >= content.length) return null;
    final delimiter = content[digitEnd];
    if (delimiter != '.' && delimiter != ')') return null;
    var index = digitEnd + 1;
    if (index >= content.length ||
        !_isHorizontalWhitespaceCodeUnit(content.codeUnitAt(index))) {
      return null;
    }
    index = _horizontalWhitespaceEnd(content, index);
    final taskMarker = _parseTaskMarker(content, index);
    if (taskMarker != null) index = taskMarker.end;
    return _ListContinuation(
      indent: content.substring(0, indentEnd),
      marker: content.substring(indentEnd, taskMarker?.start ?? index),
      taskMarker: taskMarker?.text,
      body: content.substring(index),
      orderedNumber: int.parse(content.substring(indentEnd, digitEnd)),
      orderedDelimiter: delimiter,
      orderedPadding: content.substring(
        digitEnd + 1,
        taskMarker?.start ?? index,
      ),
    );
  }
}

_Line _lineAtSelection(String markdown, FlarkSelection selection) {
  final previousBreak = selection.start == 0
      ? -1
      : markdown.lastIndexOf('\n', selection.start - 1);
  final lineStart = previousBreak + 1;
  final lineBreak = markdown.indexOf('\n', selection.start);
  return _Line(
    start: lineStart,
    end: lineBreak < 0 ? markdown.length : lineBreak,
  );
}

String _quotePrefix(String text) {
  var index = 0;
  var depth = 0;
  while (index < text.length && text.codeUnitAt(index) == 62) {
    depth++;
    index++;
    if (index < text.length &&
        _isHorizontalWhitespaceCodeUnit(text.codeUnitAt(index))) {
      index++;
    }
  }
  return depth == 0 ? '' : text.substring(0, index);
}

String? _codeIndent(String text) {
  if (text.isEmpty) return null;
  if (text.codeUnitAt(0) == 9) return '\t';

  var spaces = 0;
  while (spaces < text.length && text.codeUnitAt(spaces) == 32) {
    spaces++;
  }
  return spaces >= 4 ? text.substring(0, spaces) : null;
}

String? _emptyHeadingIndent(String text) {
  var index = 0;
  while (index < text.length && index < 3) {
    final codeUnit = text.codeUnitAt(index);
    if (!_isHorizontalWhitespaceCodeUnit(codeUnit)) break;
    index++;
  }
  final indentEnd = index;

  var hashCount = 0;
  while (index < text.length && text.codeUnitAt(index) == 35) {
    hashCount++;
    index++;
  }
  if (hashCount < 1 || hashCount > 6) return null;

  while (index < text.length) {
    if (!_isHorizontalWhitespaceCodeUnit(text.codeUnitAt(index))) return null;
    index++;
  }
  return text.substring(0, indentEnd);
}

_TaskMarker? _parseTaskMarker(String text, int start) {
  if (start + 3 >= text.length) return null;
  if (text.codeUnitAt(start) != 91 || text.codeUnitAt(start + 2) != 93) {
    return null;
  }
  final markerCodeUnit = text.codeUnitAt(start + 1);
  if (markerCodeUnit != 32 && markerCodeUnit != 120 && markerCodeUnit != 88) {
    return null;
  }
  var end = start + 3;
  if (end >= text.length ||
      !_isHorizontalWhitespaceCodeUnit(text.codeUnitAt(end))) {
    return null;
  }
  end = _horizontalWhitespaceEnd(text, end);
  return _TaskMarker(text.substring(start, end), start, end);
}

int _horizontalWhitespaceEnd(String text, int start) {
  var index = start;
  while (index < text.length &&
      _isHorizontalWhitespaceCodeUnit(text.codeUnitAt(index))) {
    index++;
  }
  return index;
}

bool _isHorizontalWhitespaceCodeUnit(int codeUnit) {
  return codeUnit == 32 || codeUnit == 9;
}

bool _isDigitCodeUnit(int codeUnit) {
  return codeUnit >= 48 && codeUnit <= 57;
}

final class _TaskMarker {
  const _TaskMarker(this.text, this.start, this.end);

  final String text;
  final int start;
  final int end;
}

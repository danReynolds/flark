import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import '../navigation/navigation_line_utils.dart';
import 'table_line_parser.dart';

class TableEditingFormatResult {
  const TableEditingFormatResult({required this.text, required this.caret});

  final String text;
  final int caret;
}

class TableEditingService {
  const TableEditingService();

  TextEditingValue maybeContinueEstablishedTableOnEnter({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required int? enterCaret,
    required LineIndex lineIndex,
    required bool Function(String text, int caret) isCaretInFencedCode,
  }) {
    final caret = enterCaret;
    if (caret == null) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    if (isCaretInFencedCode(oldText, caret)) return newValue;

    final oldLine = lineIndex.lineAtOffset(caret);
    final lineStart = lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = NavigationLineUtils.lineEndWithBreak(
      lineIndex,
      oldText,
      oldLine,
    );
    final lineEnd = NavigationLineUtils.lineContentEnd(
      oldText,
      lineStart,
      lineEndWithBreak,
    );

    final row = TableLineParser.matchRowShape(oldText, lineStart, lineEnd);
    if (row == null) return newValue;

    if (caret <= lineStart + row.indent.length) return newValue;

    if (!_isInEstablishedTable(oldText, lineIndex, oldLine, row)) {
      return newValue;
    }

    var insertAt = (caret + 1).clamp(0, newText.length);
    if (newValue.selection.isValid && newValue.selection.isCollapsed) {
      final selectionOffset = newValue.selection.baseOffset.clamp(
        0,
        newText.length,
      );
      if (selectionOffset >= insertAt) {
        insertAt = selectionOffset;
      }
    }

    final template = emptyRowTemplate(row.columnCount, indent: row.indent);
    final continued = newText.replaceRange(insertAt, insertAt, template);
    final initialCaretOffset = (insertAt + row.indent.length + 2).clamp(
      0,
      continued.length,
    );
    final formatted = formatEstablishedTableAroundCaret(
      continued,
      initialCaretOffset,
    );
    final outputText = formatted?.text ?? continued;
    final caretOffset = (formatted?.caret ?? initialCaretOffset).clamp(
      0,
      outputText.length,
    );
    return newValue.copyWith(
      text: outputText,
      selection: TextSelection.collapsed(offset: caretOffset),
      composing: TextRange.empty,
    );
  }

  String emptyRowTemplate(int columns, {required String indent}) {
    final safeColumns = columns < 1 ? 1 : columns;
    final cells = List.filled(safeColumns, '').join(' | ');
    return '$indent| $cells |';
  }

  TableEditingFormatResult? formatEstablishedTableAroundCaret(
    String text,
    int caret,
  ) {
    if (text.isEmpty) return null;
    final targetLine = _lineAtOffset(text, caret.clamp(0, text.length));
    final targetBounds = _lineBounds(text, targetLine);
    if (targetBounds == null) return null;
    final targetShape = TableLineParser.matchRowShape(
      text,
      targetBounds.$1,
      targetBounds.$2,
    );
    if (targetShape == null) return null;

    var startLine = targetLine;
    var endLineExclusive = targetLine + 1;

    while (startLine > 0) {
      final prevBounds = _lineBounds(text, startLine - 1);
      if (prevBounds == null) break;
      final prevShape = TableLineParser.matchRowShape(
        text,
        prevBounds.$1,
        prevBounds.$2,
      );
      if (prevShape == null ||
          prevShape.columnCount != targetShape.columnCount) {
        break;
      }
      startLine--;
    }
    while (true) {
      final nextBounds = _lineBounds(text, endLineExclusive);
      if (nextBounds == null) break;
      final nextShape = TableLineParser.matchRowShape(
        text,
        nextBounds.$1,
        nextBounds.$2,
      );
      if (nextShape == null ||
          nextShape.columnCount != targetShape.columnCount) {
        break;
      }
      endLineExclusive++;
    }

    final rowLines = <(
      int line,
      int start,
      int end,
      int endWithBreak,
      TableLineShape shape,
      List<String> cells,
    )>[];
    var hasSeparator = false;
    final widths = List<int>.filled(targetShape.columnCount, 3);

    for (var line = startLine; line < endLineExclusive; line++) {
      final bounds = _lineBounds(text, line);
      if (bounds == null) return null;
      final lineStart = bounds.$1;
      final lineEnd = bounds.$2;
      final lineEndWithBreak = bounds.$3;
      final shape = TableLineParser.matchRowShape(text, lineStart, lineEnd);
      if (shape == null || shape.columnCount != targetShape.columnCount) {
        return null;
      }
      final body = text.substring(lineStart + shape.indent.length, lineEnd);
      final cells = TableLineParser.splitCellTexts(body);
      if (cells == null || cells.length != targetShape.columnCount) {
        return null;
      }

      if (shape.isSeparator) {
        hasSeparator = true;
      } else {
        for (var i = 0; i < cells.length; i++) {
          final width = cells[i].trim().length;
          if (width > widths[i]) widths[i] = width;
        }
      }
      rowLines.add((
        line,
        lineStart,
        lineEnd,
        lineEndWithBreak,
        shape,
        cells,
      ));
    }
    if (!hasSeparator) return null;

    final formattedLines = <String>[];
    for (final entry in rowLines) {
      final shape = entry.$5;
      final cells = entry.$6;
      if (shape.isSeparator) {
        final parts = <String>[];
        for (var i = 0; i < cells.length; i++) {
          final alignment = TableLineParser.separatorAlignment(cells[i]);
          parts.add(
            _formatSeparatorCell(widths[i], alignment.$1, alignment.$2),
          );
        }
        formattedLines.add('${shape.indent}| ${parts.join(' | ')} |');
      } else {
        final parts = <String>[];
        for (var i = 0; i < cells.length; i++) {
          final trimmed = cells[i].trim();
          final pad = widths[i] - trimmed.length;
          parts.add(pad > 0 ? '$trimmed${' ' * pad}' : trimmed);
        }
        formattedLines.add('${shape.indent}| ${parts.join(' | ')} |');
      }
    }

    final regionStart = rowLines.first.$2;
    final regionEnd = rowLines.last.$4;
    final preserveTrailingNewline = regionEnd > rowLines.last.$3 &&
        regionEnd <= text.length &&
        text.codeUnitAt(regionEnd - 1) == 10;
    var regionText = formattedLines.join('\n');
    if (preserveTrailingNewline && !regionText.endsWith('\n')) {
      regionText = '$regionText\n';
    }
    final updated = text.replaceRange(regionStart, regionEnd, regionText);

    final targetFormattedBounds = _lineBounds(updated, targetLine);
    if (targetFormattedBounds == null) {
      return TableEditingFormatResult(
        text: updated,
        caret: caret.clamp(0, updated.length),
      );
    }
    final targetLineStart = targetFormattedBounds.$1;
    final targetLineEnd = targetFormattedBounds.$2;
    final targetUpdatedShape = TableLineParser.matchRowShape(
      updated,
      targetLineStart,
      targetLineEnd,
    );
    final newCaret = targetUpdatedShape == null
        ? caret.clamp(0, updated.length).toInt()
        : (targetLineStart + targetUpdatedShape.indent.length + 2).clamp(
            0,
            updated.length,
          );
    return TableEditingFormatResult(text: updated, caret: newCaret);
  }

  bool _isInEstablishedTable(
    String text,
    LineIndex lineIndex,
    int line,
    TableLineShape current,
  ) {
    if (current.isSeparator) return true;

    for (var scan = line - 1; scan >= 0; scan--) {
      final start = lineIndex.offsetAtLine(scan);
      final endWithBreak = NavigationLineUtils.lineEndWithBreak(
        lineIndex,
        text,
        scan,
      );
      final end = NavigationLineUtils.lineContentEnd(text, start, endWithBreak);
      final shape = TableLineParser.matchRowShape(text, start, end);
      if (shape == null) break;
      if (shape.columnCount != current.columnCount) break;
      if (shape.isSeparator) return true;
    }
    return false;
  }

  static String _formatSeparatorCell(int width, bool left, bool right) {
    final dashCount = width < 3 ? 3 : width;
    return '${left ? ':' : ''}${'-' * dashCount}${right ? ':' : ''}';
  }

  static (int, int, int)? _lineBounds(String text, int line) {
    if (line < 0) return null;
    var lineIndex = 0;
    var start = 0;
    while (lineIndex < line && start <= text.length) {
      final next = text.indexOf('\n', start);
      if (next == -1) return null;
      start = next + 1;
      lineIndex++;
    }
    if (start > text.length) return null;
    final lineEndWithBreak = (() {
      final next = text.indexOf('\n', start);
      return next == -1 ? text.length : next + 1;
    })();
    final lineEnd = (lineEndWithBreak > start &&
            text.codeUnitAt(lineEndWithBreak - 1) == 10)
        ? lineEndWithBreak - 1
        : lineEndWithBreak;
    return (start, lineEnd, lineEndWithBreak);
  }

  static int _lineAtOffset(String text, int offset) {
    final safe = offset.clamp(0, text.length).toInt();
    var line = 0;
    for (var i = 0; i < safe; i++) {
      if (text.codeUnitAt(i) == 10) line++;
    }
    return line;
  }
}

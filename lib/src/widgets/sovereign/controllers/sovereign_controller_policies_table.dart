part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

class _TableFormatResult {
  final String text;
  final int caret;

  const _TableFormatResult({required this.text, required this.caret});
}

abstract final class _TablePolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(name: 'enter-table', priority: 36, apply: _onEnter),
  ];

  static TextEditingValue _onEnter(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
    final caret = context.intent.enterCaret;
    if (caret == null) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    // Never run inside fenced code.
    if (helpers.fenceContextForCaret(
          oldText,
          caret,
          includeUnclosedEof: true,
        ) !=
        null) {
      return newValue;
    }

    final oldLine = helpers.lineIndex.lineAtOffset(caret);
    final lineStart = helpers.lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = helpers.lineEndWithBreak(oldText, oldLine);
    final lineEnd = helpers.lineContentEnd(
      oldText,
      lineStart,
      lineEndWithBreak,
    );

    final row = _matchTableRowShape(oldText, lineStart, lineEnd);
    if (row == null) return newValue;

    // Pressing Enter before table row content should remain a normal split.
    if (caret <= lineStart + row.indent.length) return newValue;

    if (!_isInEstablishedTable(oldText, helpers, oldLine, row)) {
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

    final template = _emptyRowTemplate(row.columnCount, indent: row.indent);
    final continued = newText.replaceRange(insertAt, insertAt, template);
    final initialCaretOffset = (insertAt + row.indent.length + 2).clamp(
      0,
      continued.length,
    );
    final formatted = _formatEstablishedTableAroundCaret(
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

  static bool _isInEstablishedTable(
    String text,
    _PolicyHelpers helpers,
    int line,
    TableLineShape current,
  ) {
    if (current.isSeparator) return true;

    for (var scan = line - 1; scan >= 0; scan--) {
      final start = helpers.lineIndex.offsetAtLine(scan);
      final endWithBreak = helpers.lineEndWithBreak(text, scan);
      final end = helpers.lineContentEnd(text, start, endWithBreak);
      final shape = _matchTableRowShape(text, start, end);
      if (shape == null) break;
      if (shape.columnCount != current.columnCount) break;
      if (shape.isSeparator) return true;
    }
    return false;
  }

  static TableLineShape? _matchTableRowShape(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    if (lineEnd <= lineStart || lineStart < 0 || lineEnd > text.length) {
      return null;
    }
    final line = text.substring(lineStart, lineEnd);
    if (line.trim().isEmpty) return null;

    final indent = NavigationLineUtils.leadingWhitespacePrefix(line);
    final body = line.substring(indent.length);
    if (body.isEmpty || body.startsWith('>')) {
      return null; // phase-1: no quoted tables
    }

    final split = _splitTableCells(body);
    if (split == null) return null;
    if (split.cells.length < 2) return null;

    var separatorCells = 0;
    for (final cell in split.cells) {
      if (_isTableSeparatorCell(cell)) separatorCells++;
    }
    final isSeparator = separatorCells == split.cells.length;

    return TableLineShape(
      columnCount: split.cells.length,
      isSeparator: isSeparator,
      indent: indent,
    );
  }

  static _SplitTableCellsResult? _splitTableCells(String body) {
    final cells = <String>[];
    var sawPipe = false;
    var start = 0;
    var i = 0;
    while (i < body.length) {
      final cu = body.codeUnitAt(i);
      if (cu == 92) {
        // Skip escaped next code unit.
        i += 2;
        continue;
      }
      if (cu == 124) {
        sawPipe = true;
        cells.add(body.substring(start, i));
        start = i + 1;
      }
      i++;
    }
    if (!sawPipe) return null;
    cells.add(body.substring(start));

    final leftTrimmed = body.trimLeft();
    final rightTrimmed = body.trimRight();
    final hasLeadingPipe = leftTrimmed.startsWith('|');
    final hasTrailingPipe = rightTrimmed.endsWith('|');

    var normalized = List<String>.from(cells);
    if (hasLeadingPipe &&
        normalized.isNotEmpty &&
        normalized.first.trim().isEmpty) {
      normalized = normalized.sublist(1);
    }
    if (hasTrailingPipe &&
        normalized.isNotEmpty &&
        normalized.last.trim().isEmpty) {
      normalized = normalized.sublist(0, normalized.length - 1);
    }
    if (normalized.isEmpty) return null;

    return _SplitTableCellsResult(cells: normalized);
  }

  static bool _isTableSeparatorCell(String rawCell) {
    var cell = rawCell.trim();
    if (cell.isEmpty) return false;
    if (cell.startsWith(':')) cell = cell.substring(1);
    if (cell.endsWith(':')) cell = cell.substring(0, cell.length - 1);
    if (cell.isEmpty) return false;
    for (var i = 0; i < cell.length; i++) {
      if (cell.codeUnitAt(i) != 45) return false; // '-'
    }
    return true;
  }

  static String _emptyRowTemplate(int columns, {required String indent}) {
    final safeColumns = columns < 1 ? 1 : columns;
    final cells = List.filled(safeColumns, '').join(' | ');
    return '$indent| $cells |';
  }

  static _TableFormatResult? _formatEstablishedTableAroundCaret(
    String text,
    int caret,
  ) {
    if (text.isEmpty) return null;
    final targetLine = _lineAtOffset(text, caret.clamp(0, text.length));
    final targetBounds = _lineBounds(text, targetLine);
    if (targetBounds == null) return null;
    final targetShape = _matchTableRowShape(
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
      final prevShape = _matchTableRowShape(text, prevBounds.$1, prevBounds.$2);
      if (prevShape == null ||
          prevShape.columnCount != targetShape.columnCount) {
        break;
      }
      startLine--;
    }
    while (true) {
      final nextBounds = _lineBounds(text, endLineExclusive);
      if (nextBounds == null) break;
      final nextShape = _matchTableRowShape(text, nextBounds.$1, nextBounds.$2);
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
      final shape = _matchTableRowShape(text, lineStart, lineEnd);
      if (shape == null || shape.columnCount != targetShape.columnCount) {
        return null;
      }
      final body = text.substring(lineStart + shape.indent.length, lineEnd);
      final split = _splitTableCells(body);
      if (split == null || split.cells.length != targetShape.columnCount) {
        return null;
      }

      if (shape.isSeparator) {
        hasSeparator = true;
      } else {
        for (var i = 0; i < split.cells.length; i++) {
          final width = split.cells[i].trim().length;
          if (width > widths[i]) widths[i] = width;
        }
      }
      rowLines.add((
        line,
        lineStart,
        lineEnd,
        lineEndWithBreak,
        shape,
        split.cells,
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
          final alignment = _separatorAlignment(cells[i]);
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
      return _TableFormatResult(
        text: updated,
        caret: caret.clamp(0, updated.length),
      );
    }
    final targetLineStart = targetFormattedBounds.$1;
    final targetLineEnd = targetFormattedBounds.$2;
    final targetUpdatedShape = _matchTableRowShape(
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
    return _TableFormatResult(text: updated, caret: newCaret);
  }

  static (bool left, bool right) _separatorAlignment(String rawCell) {
    final cell = rawCell.trim();
    if (cell.isEmpty) return (false, false);
    return (cell.startsWith(':'), cell.endsWith(':'));
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

class _SplitTableCellsResult {
  final List<String> cells;

  const _SplitTableCellsResult({required this.cells});
}

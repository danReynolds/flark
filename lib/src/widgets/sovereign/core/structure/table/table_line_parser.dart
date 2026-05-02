import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

class TableLineShape {
  const TableLineShape({
    required this.columnCount,
    required this.isSeparator,
    required this.indent,
  });

  final int columnCount;
  final bool isSeparator;
  final String indent;
}

class ParsedTableLine {
  const ParsedTableLine({
    required this.line,
    required this.lineStart,
    required this.lineEnd,
    required this.lineEndWithBreak,
    required this.indent,
    required this.columnCount,
    required this.isSeparator,
    required this.cells,
  });

  final int line;
  final int lineStart;
  final int lineEnd;
  final int lineEndWithBreak;
  final String indent;
  final int columnCount;
  final bool isSeparator;
  final List<TableCellCaretBounds> cells;
}

class TableCellCaretBounds {
  const TableCellCaretBounds({
    required this.rawStart,
    required this.rawEnd,
    required this.contentStart,
    required this.contentEnd,
    required this.preferredCaret,
  });

  final int rawStart;
  final int rawEnd;
  final int contentStart;
  final int contentEnd;
  final int preferredCaret;
}

typedef TableRowShapeResolver = TableLineShape? Function(
    String text, int lineStart, int lineEnd);

class TableLineParser {
  const TableLineParser._();

  static TableLineShape? matchRowShape(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    if (lineEnd <= lineStart || lineStart < 0 || lineEnd > text.length) {
      return null;
    }
    final line = text.substring(lineStart, lineEnd);
    if (line.trim().isEmpty) return null;

    final indent = _leadingWhitespacePrefix(line);
    final body = line.substring(indent.length);
    if (body.isEmpty || body.startsWith('>')) {
      return null;
    }

    final cells = splitCellTexts(body);
    if (cells == null || cells.length < 2) return null;

    var separatorCells = 0;
    for (final cell in cells) {
      if (isSeparatorCell(cell)) separatorCells++;
    }
    final isSeparator = separatorCells == cells.length;

    return TableLineShape(
      columnCount: cells.length,
      isSeparator: isSeparator,
      indent: indent,
    );
  }

  static List<String>? splitCellTexts(String body) {
    final cells = <String>[];
    var sawPipe = false;
    var start = 0;
    var i = 0;
    while (i < body.length) {
      final cu = body.codeUnitAt(i);
      if (cu == 92) {
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

    return normalized;
  }

  static bool isSeparatorCell(String rawCell) {
    var cell = rawCell.trim();
    if (cell.isEmpty) return false;
    if (cell.startsWith(':')) cell = cell.substring(1);
    if (cell.endsWith(':')) cell = cell.substring(0, cell.length - 1);
    if (cell.isEmpty) return false;
    for (var i = 0; i < cell.length; i++) {
      if (cell.codeUnitAt(i) != 45) return false;
    }
    return true;
  }

  static (bool left, bool right) separatorAlignment(String rawCell) {
    final cell = rawCell.trim();
    if (cell.isEmpty) return (false, false);
    return (cell.startsWith(':'), cell.endsWith(':'));
  }

  static ParsedTableLine? parseLineAt({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
    required TableRowShapeResolver rowShapeResolver,
  }) {
    if (line < 0 || line >= lineIndex.lineCount) return null;
    final lineStart = lineIndex.offsetAtLine(line);
    if (isLineInsideFencedGeometry(lineStart)) return null;
    final lineEndWithBreak = _lineEndWithBreak(lineIndex, text, line);
    final lineEnd = _lineContentEnd(text, lineStart, lineEndWithBreak);
    final shape = rowShapeResolver(text, lineStart, lineEnd);
    if (shape == null) return null;

    final lineText = text.substring(lineStart, lineEnd);
    final body = lineText.substring(shape.indent.length);
    final cells = _parseTableCellsFromBody(
      body,
      baseOffset: lineStart + shape.indent.length,
    );
    if (cells == null || cells.length != shape.columnCount) return null;

    return ParsedTableLine(
      line: line,
      lineStart: lineStart,
      lineEnd: lineEnd,
      lineEndWithBreak: lineEndWithBreak,
      indent: shape.indent,
      columnCount: shape.columnCount,
      isSeparator: shape.isSeparator,
      cells: cells,
    );
  }

  static int? tableCellIndexForCaret(ParsedTableLine row, int caret) {
    for (var i = 0; i < row.cells.length; i++) {
      final cell = row.cells[i];
      if (caret >= cell.rawStart && caret <= cell.rawEnd) return i;
    }
    if (caret < row.cells.first.rawStart) return 0;
    if (caret > row.cells.last.rawEnd) return row.cells.length - 1;
    return null;
  }

  static List<TableCellCaretBounds>? _parseTableCellsFromBody(
    String body, {
    required int baseOffset,
  }) {
    final rawSegments = <_RawCellSegment>[];
    var sawPipe = false;
    var start = 0;
    var i = 0;
    while (i < body.length) {
      final cu = body.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 124) {
        sawPipe = true;
        rawSegments.add(_RawCellSegment(start: start, end: i));
        start = i + 1;
      }
      i++;
    }
    if (!sawPipe) return null;
    rawSegments.add(_RawCellSegment(start: start, end: body.length));

    final hasLeadingPipe = body.trimLeft().startsWith('|');
    final hasTrailingPipe = body.trimRight().endsWith('|');
    var segments = List<_RawCellSegment>.from(rawSegments);
    if (hasLeadingPipe &&
        segments.isNotEmpty &&
        body
            .substring(segments.first.start, segments.first.end)
            .trim()
            .isEmpty) {
      segments = segments.sublist(1);
    }
    if (hasTrailingPipe &&
        segments.isNotEmpty &&
        body.substring(segments.last.start, segments.last.end).trim().isEmpty) {
      segments = segments.sublist(0, segments.length - 1);
    }
    if (segments.isEmpty) return null;

    final cells = <TableCellCaretBounds>[];
    for (final seg in segments) {
      var contentStart = seg.start;
      while (contentStart < seg.end) {
        final cu = body.codeUnitAt(contentStart);
        if (cu != 32 && cu != 9) break;
        contentStart++;
      }
      var contentEnd = seg.end;
      while (contentEnd > seg.start) {
        final cu = body.codeUnitAt(contentEnd - 1);
        if (cu != 32 && cu != 9) break;
        contentEnd--;
      }

      var preferred = contentStart;
      if (preferred >= seg.end && seg.start < seg.end) {
        preferred = seg.start;
      }
      if (preferred < seg.end && body.codeUnitAt(preferred) == 32) {
        preferred++;
      }
      if (preferred > seg.end) preferred = seg.end;

      cells.add(
        TableCellCaretBounds(
          rawStart: baseOffset + seg.start,
          rawEnd: baseOffset + seg.end,
          contentStart: baseOffset + contentStart,
          contentEnd: baseOffset + contentEnd,
          preferredCaret: (baseOffset + preferred)
              .clamp(0, baseOffset + body.length)
              .toInt(),
        ),
      );
    }
    return cells;
  }

  static int _lineEndWithBreak(LineIndex lineIndex, String text, int line) {
    if (line + 1 < lineIndex.lineCount) {
      return lineIndex.offsetAtLine(line + 1);
    }
    return text.length;
  }

  static int _lineContentEnd(String text, int lineStart, int lineEndWithBreak) {
    if (lineEndWithBreak > lineStart &&
        text.codeUnitAt(lineEndWithBreak - 1) == 10) {
      return lineEndWithBreak - 1;
    }
    return lineEndWithBreak;
  }

  static String _leadingWhitespacePrefix(String text) {
    var cursor = 0;
    while (cursor < text.length) {
      final cu = text.codeUnitAt(cursor);
      if (cu != 32 && cu != 9) break;
      cursor++;
    }
    return text.substring(0, cursor);
  }
}

class _RawCellSegment {
  const _RawCellSegment({required this.start, required this.end});

  final int start;
  final int end;
}

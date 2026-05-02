import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

import 'table_editing_service.dart';
import 'table_line_parser.dart';

class TableCommandEditingResult {
  const TableCommandEditingResult({
    required this.text,
    required this.selection,
  });

  final String text;
  final TextSelection selection;
}

class _EstablishedTableContext {
  const _EstablishedTableContext({
    required this.rows,
    required this.currentRowIndex,
    required this.currentColumnIndex,
    required this.separatorRowIndex,
  });

  final List<ParsedTableLine> rows;
  final int currentRowIndex;
  final int currentColumnIndex;
  final int separatorRowIndex;

  ParsedTableLine get currentRow => rows[currentRowIndex];
  int get columnCount => currentRow.columnCount;
}

class TableCommandEditingService {
  const TableCommandEditingService([
    this._tableEditing = const TableEditingService(),
  ]);

  final TableEditingService _tableEditing;

  TableCommandEditingResult insertTable({
    required String text,
    required TextSelection selection,
    int columns = 2,
    int bodyRows = 1,
  }) {
    final safeSelection = _safeSelection(selection, text.length);
    final safeColumns = columns < 2 ? 2 : columns;
    final safeBodyRows = bodyRows < 1 ? 1 : bodyRows;
    final before = text.substring(0, safeSelection.start);
    final after = text.substring(safeSelection.end);
    final prefix = _blockPrefix(before);
    final suffix = _blockSuffix(after);
    final template = _tableTemplate(
      columns: safeColumns,
      bodyRows: safeBodyRows,
    );
    final insertion = '$prefix$template\n$suffix';
    final updated = text.replaceRange(
      safeSelection.start,
      safeSelection.end,
      insertion,
    );

    final firstBodyOffsetInTemplate = _firstBodyCellOffsetInTemplate(template);
    final unformattedCaret =
        (safeSelection.start + prefix.length + firstBodyOffsetInTemplate)
            .clamp(0, updated.length)
            .toInt();
    final formatted = _tableEditing.formatEstablishedTableAroundCaret(
      updated,
      unformattedCaret,
    );
    final outputText = formatted?.text ?? updated;
    final outputCaret = (formatted?.caret ?? unformattedCaret)
        .clamp(0, outputText.length)
        .toInt();
    return TableCommandEditingResult(
      text: outputText,
      selection: TextSelection.collapsed(offset: outputCaret),
    );
  }

  TableCommandEditingResult? insertRowBelow({
    required String text,
    required TextSelection selection,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
  }) {
    final table = _establishedTableAtSelection(
      text: text,
      selection: selection,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
    );
    if (table == null) return null;

    final insertAfterIndex = table.currentRowIndex <= table.separatorRowIndex
        ? table.separatorRowIndex
        : table.currentRowIndex;
    final anchor = table.rows[insertAfterIndex];
    final anchorHasLineBreak = anchor.lineEndWithBreak > anchor.lineEnd;
    final prefix = anchorHasLineBreak ? '' : '\n';
    final suffix = anchorHasLineBreak ? '\n' : '';
    final template = _tableEditing.emptyRowTemplate(
      table.columnCount,
      indent: anchor.indent,
    );
    final insertion = '$prefix$template$suffix';
    final updated = text.replaceRange(
      anchor.lineEndWithBreak,
      anchor.lineEndWithBreak,
      insertion,
    );
    final targetLine = anchor.line + 1;
    final caret =
        anchor.lineEndWithBreak + prefix.length + anchor.indent.length + 2;
    return _formatAndSelectCell(
      updated,
      caret,
      targetLine: targetLine,
      targetCell: 0,
    );
  }

  TableCommandEditingResult? deleteRow({
    required String text,
    required TextSelection selection,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
  }) {
    final table = _establishedTableAtSelection(
      text: text,
      selection: selection,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
    );
    if (table == null) return null;
    if (table.currentRowIndex <= table.separatorRowIndex) return null;

    final row = table.currentRow;
    final updated = text.replaceRange(row.lineStart, row.lineEndWithBreak, '');
    final targetRow = _targetRowAfterDeletingCurrentRow(table);
    final targetLine =
        targetRow.line > row.line ? targetRow.line - 1 : targetRow.line;
    final targetCell = table.currentColumnIndex.clamp(0, table.columnCount - 1);
    final caret = _preferredCaretForCell(
          updated,
          targetLine: targetLine,
          targetCell: targetCell,
        ) ??
        row.lineStart.clamp(0, updated.length).toInt();
    return _formatAndSelectCell(
      updated,
      caret,
      targetLine: targetLine,
      targetCell: targetCell,
    );
  }

  TableCommandEditingResult? insertColumnRight({
    required String text,
    required TextSelection selection,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
  }) {
    final table = _establishedTableAtSelection(
      text: text,
      selection: selection,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
    );
    if (table == null) return null;

    final insertIndex = table.currentColumnIndex + 1;
    final rowTexts = <String>[];
    for (final row in table.rows) {
      final cells = _cellTexts(text, row);
      if (cells == null) return null;
      cells.insert(insertIndex, row.isSeparator ? '---' : '');
      rowTexts.add(_formatSourceRow(cells, indent: row.indent));
    }

    final updated = _replaceTableRows(text, table.rows, rowTexts);
    final caret = _preferredCaretForCell(
          updated,
          targetLine: table.currentRow.line,
          targetCell: insertIndex,
        ) ??
        table.currentRow.lineStart.clamp(0, updated.length).toInt();
    return _formatAndSelectCell(
      updated,
      caret,
      targetLine: table.currentRow.line,
      targetCell: insertIndex,
    );
  }

  TableCommandEditingResult? deleteColumn({
    required String text,
    required TextSelection selection,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
  }) {
    final table = _establishedTableAtSelection(
      text: text,
      selection: selection,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
    );
    if (table == null) return null;
    if (table.columnCount <= 2) return null;

    final deleteIndex = table.currentColumnIndex;
    final rowTexts = <String>[];
    for (final row in table.rows) {
      final cells = _cellTexts(text, row);
      if (cells == null || deleteIndex >= cells.length) return null;
      cells.removeAt(deleteIndex);
      rowTexts.add(_formatSourceRow(cells, indent: row.indent));
    }

    final targetCell = deleteIndex.clamp(0, table.columnCount - 2);
    final updated = _replaceTableRows(text, table.rows, rowTexts);
    final caret = _preferredCaretForCell(
          updated,
          targetLine: table.currentRow.line,
          targetCell: targetCell,
        ) ??
        table.currentRow.lineStart.clamp(0, updated.length).toInt();
    return _formatAndSelectCell(
      updated,
      caret,
      targetLine: table.currentRow.line,
      targetCell: targetCell,
    );
  }

  static TextSelection _safeSelection(TextSelection selection, int length) {
    if (!selection.isValid || selection.start < 0 || selection.end < 0) {
      return TextSelection.collapsed(offset: length);
    }
    final start = selection.start.clamp(0, length).toInt();
    final end = selection.end.clamp(0, length).toInt();
    return TextSelection(
      baseOffset: start,
      extentOffset: end,
      affinity: selection.affinity,
      isDirectional: selection.isDirectional,
    );
  }

  static String _blockPrefix(String before) {
    if (before.isEmpty) return '';
    if (before.endsWith('\n\n')) return '';
    if (before.endsWith('\n')) return '\n';
    return '\n\n';
  }

  static String _blockSuffix(String after) {
    if (after.isEmpty) return '';
    if (after.startsWith('\n')) return '';
    return '\n';
  }

  static String _tableTemplate({required int columns, required int bodyRows}) {
    final headers = List<String>.generate(columns, (i) => 'Header ${i + 1}');
    final separator = List<String>.filled(columns, '---');
    final rows = <String>[
      _formatSourceRow(headers, indent: ''),
      _formatSourceRow(separator, indent: ''),
      for (var i = 0; i < bodyRows; i++)
        _formatSourceRow(List<String>.filled(columns, ''), indent: ''),
    ];
    return rows.join('\n');
  }

  static int _firstBodyCellOffsetInTemplate(String template) {
    final firstBreak = template.indexOf('\n');
    if (firstBreak == -1) return 0;
    final secondBreak = template.indexOf('\n', firstBreak + 1);
    if (secondBreak == -1) return template.length;
    return (secondBreak + 3).clamp(0, template.length).toInt();
  }

  static _EstablishedTableContext? _establishedTableAtSelection({
    required String text,
    required TextSelection selection,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
  }) {
    if (!selection.isValid || !selection.isCollapsed) return null;
    if (text.isEmpty) return null;

    final caret = selection.extentOffset.clamp(0, text.length).toInt();
    final line = lineIndex.lineAtOffset(caret);
    final current = _parseTableLineAt(
      text: text,
      line: line,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
    );
    if (current == null) return null;

    final currentColumn = TableLineParser.tableCellIndexForCaret(
      current,
      caret,
    );
    if (currentColumn == null) return null;

    var firstLine = line;
    while (firstLine > 0) {
      final prev = _parseTableLineAt(
        text: text,
        line: firstLine - 1,
        lineIndex: lineIndex,
        isLineInsideFencedGeometry: isLineInsideFencedGeometry,
      );
      if (prev == null || prev.columnCount != current.columnCount) break;
      firstLine--;
    }

    var endLineExclusive = line + 1;
    while (endLineExclusive < lineIndex.lineCount) {
      final next = _parseTableLineAt(
        text: text,
        line: endLineExclusive,
        lineIndex: lineIndex,
        isLineInsideFencedGeometry: isLineInsideFencedGeometry,
      );
      if (next == null || next.columnCount != current.columnCount) break;
      endLineExclusive++;
    }

    final rows = <ParsedTableLine>[];
    var separatorIndex = -1;
    var currentRowIndex = -1;
    for (var scan = firstLine; scan < endLineExclusive; scan++) {
      final row = _parseTableLineAt(
        text: text,
        line: scan,
        lineIndex: lineIndex,
        isLineInsideFencedGeometry: isLineInsideFencedGeometry,
      );
      if (row == null || row.columnCount != current.columnCount) return null;
      if (row.isSeparator && separatorIndex == -1) {
        separatorIndex = rows.length;
      }
      if (scan == line) currentRowIndex = rows.length;
      rows.add(row);
    }

    if (separatorIndex == -1 || currentRowIndex == -1) return null;
    return _EstablishedTableContext(
      rows: rows,
      currentRowIndex: currentRowIndex,
      currentColumnIndex: currentColumn,
      separatorRowIndex: separatorIndex,
    );
  }

  static ParsedTableLine? _parseTableLineAt({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
  }) {
    return TableLineParser.parseLineAt(
      text: text,
      line: line,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
      rowShapeResolver: TableLineParser.matchRowShape,
    );
  }

  static List<String>? _cellTexts(String text, ParsedTableLine row) {
    final cells = <String>[];
    for (final cell in row.cells) {
      if (cell.rawStart < 0 || cell.rawEnd > text.length) return null;
      cells.add(text.substring(cell.rawStart, cell.rawEnd).trim());
    }
    return cells;
  }

  static String _replaceTableRows(
    String text,
    List<ParsedTableLine> rows,
    List<String> rowTexts,
  ) {
    final regionStart = rows.first.lineStart;
    final regionEnd = rows.last.lineEndWithBreak;
    final preserveTrailingNewline =
        rows.last.lineEndWithBreak > rows.last.lineEnd;
    var replacement = rowTexts.join('\n');
    if (preserveTrailingNewline) replacement = '$replacement\n';
    return text.replaceRange(regionStart, regionEnd, replacement);
  }

  static String _formatSourceRow(List<String> cells, {required String indent}) {
    return '$indent| ${cells.join(' | ')} |';
  }

  static ParsedTableLine _targetRowAfterDeletingCurrentRow(
    _EstablishedTableContext table,
  ) {
    for (var scan = table.currentRowIndex + 1;
        scan < table.rows.length;
        scan++) {
      final row = table.rows[scan];
      if (!row.isSeparator) return row;
    }
    for (var scan = table.currentRowIndex - 1; scan >= 0; scan--) {
      final row = table.rows[scan];
      if (!row.isSeparator) return row;
    }
    return table.rows.first;
  }

  TableCommandEditingResult _formatAndSelectCell(
    String text,
    int caret, {
    required int targetLine,
    required int targetCell,
  }) {
    final formatted = _tableEditing.formatEstablishedTableAroundCaret(
      text,
      caret,
    );
    final outputText = formatted?.text ?? text;
    final outputCaret = _preferredCaretForCell(
          outputText,
          targetLine: targetLine,
          targetCell: targetCell,
        ) ??
        (formatted?.caret ?? caret).clamp(0, outputText.length).toInt();
    return TableCommandEditingResult(
      text: outputText,
      selection: TextSelection.collapsed(offset: outputCaret),
    );
  }

  static int? _preferredCaretForCell(
    String text, {
    required int targetLine,
    required int targetCell,
  }) {
    if (text.isEmpty) return null;
    final lineIndex = LineIndex.fromText(text);
    final line = targetLine.clamp(0, lineIndex.lineCount - 1).toInt();
    final row = _parseTableLineAt(
      text: text,
      line: line,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: (_) => false,
    );
    if (row == null || row.cells.isEmpty) return null;
    final cell = targetCell.clamp(0, row.cells.length - 1).toInt();
    return row.cells[cell].preferredCaret.clamp(0, text.length).toInt();
  }
}

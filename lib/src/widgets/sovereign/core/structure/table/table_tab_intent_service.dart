import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import '../navigation/navigation_line_utils.dart';
import 'table_line_parser.dart';

class TableTabFormattingResult {
  const TableTabFormattingResult({required this.text, required this.caret});

  final String text;
  final int caret;
}

abstract class TableTabIntentHost {
  TextEditingValue get value;
  TextSelection get selection;
  set selection(TextSelection value);
  LineIndex get lineIndex;

  void commitProgrammaticTextEdit(TextEditingValue newValue);

  bool isCaretInsideFence(String text, int caret);
  ParsedTableLine? parseTableLineAt(String text, int line);
  bool tableRegionHasSeparator(String text, int line, int columnCount);
  int? tableCellIndexForCaret(ParsedTableLine row, int caret);
  ParsedTableLine? findAdjacentTableLine({
    required String text,
    required int line,
    required int columnCount,
    required bool forward,
    bool skipSeparator,
  });
  String emptyRowTemplate(int columns, {required String indent});
  TableTabFormattingResult? formatEstablishedTableAroundCaret(
    String text,
    int caret,
  );
}

class TableTabIntentService {
  const TableTabIntentService(this._host);

  final TableTabIntentHost _host;

  bool tryHandleTabKey({required bool reverse}) {
    final selection = _host.selection;
    if (!selection.isValid || !selection.isCollapsed) return false;

    final text = _host.value.text;
    final caret = selection.baseOffset;
    if (caret < 0 || caret > text.length) return false;

    if (_host.isCaretInsideFence(text, caret)) return false;

    final line = _host.lineIndex.lineAtOffset(caret);
    final parsed = _host.parseTableLineAt(text, line);
    if (parsed == null) return false;
    if (!_host.tableRegionHasSeparator(text, line, parsed.columnCount)) {
      return false;
    }

    final currentCell = _host.tableCellIndexForCaret(parsed, caret);
    if (currentCell == null) return false;

    // Treat separator rows as non-editable navigation rows: tab jumps across
    // them instead of traversing their dash cells.
    if (parsed.isSeparator) {
      final adjacent = _host.findAdjacentTableLine(
        text: text,
        line: line,
        columnCount: parsed.columnCount,
        forward: !reverse,
        skipSeparator: true,
      );
      if (adjacent == null) return false;
      _host.selection = TextSelection.collapsed(
        offset: reverse
            ? adjacent.cells.last.preferredCaret
            : adjacent.cells.first.preferredCaret,
      );
      return true;
    }

    if (reverse) {
      if (currentCell > 0) {
        _host.selection = TextSelection.collapsed(
          offset: parsed.cells[currentCell - 1].preferredCaret,
        );
        return true;
      }
      final prev = _host.findAdjacentTableLine(
        text: text,
        line: line,
        columnCount: parsed.columnCount,
        forward: false,
        skipSeparator: true,
      );
      if (prev == null) return false;
      _host.selection = TextSelection.collapsed(
        offset: prev.cells.last.preferredCaret,
      );
      return true;
    }

    if (currentCell + 1 < parsed.cells.length) {
      _host.selection = TextSelection.collapsed(
        offset: parsed.cells[currentCell + 1].preferredCaret,
      );
      return true;
    }

    final next = _host.findAdjacentTableLine(
      text: text,
      line: line,
      columnCount: parsed.columnCount,
      forward: true,
      skipSeparator: true,
    );
    if (next != null) {
      _host.selection = TextSelection.collapsed(
        offset: next.cells.first.preferredCaret,
      );
      return true;
    }

    // At the last cell of the last row: create a new empty row.
    final lineEndWithBreak = NavigationLineUtils.lineEndWithBreak(
      _host.lineIndex,
      text,
      line,
    );
    final template = _host.emptyRowTemplate(
      parsed.columnCount,
      indent: parsed.indent,
    );
    final insertPrefix = (lineEndWithBreak > 0 &&
            lineEndWithBreak <= text.length &&
            text.codeUnitAt(lineEndWithBreak - 1) == 10)
        ? ''
        : '\n';
    final insert = '$insertPrefix$template';
    final insertedText = text.replaceRange(
      lineEndWithBreak,
      lineEndWithBreak,
      insert,
    );
    final newCaret =
        lineEndWithBreak + insertPrefix.length + parsed.indent.length + 2;
    final formatted = _host.formatEstablishedTableAroundCaret(
      insertedText,
      newCaret,
    );
    final newText = formatted?.text ?? insertedText;
    final finalCaret =
        (formatted?.caret ?? newCaret).clamp(0, newText.length).toInt();

    _host.commitProgrammaticTextEdit(
      _host.value.copyWith(
        text: newText,
        selection: TextSelection.collapsed(offset: finalCaret),
        composing: TextRange.empty,
      ),
    );
    return true;
  }
}

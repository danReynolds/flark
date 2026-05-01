import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import 'table_line_parser.dart';

class TableNavigationService {
  const TableNavigationService._();

  static ParsedTableLine? parseLineAt({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required bool Function(int lineStart) isLineInsideFencedGeometry,
    required TableRowShapeResolver rowShapeResolver,
  }) {
    return TableLineParser.parseLineAt(
      text: text,
      line: line,
      lineIndex: lineIndex,
      isLineInsideFencedGeometry: isLineInsideFencedGeometry,
      rowShapeResolver: rowShapeResolver,
    );
  }

  static int? tableCellIndexForCaret(ParsedTableLine row, int caret) {
    return TableLineParser.tableCellIndexForCaret(row, caret);
  }

  static bool tableRegionHasSeparator({
    required String text,
    required int line,
    required int columnCount,
    required LineIndex lineIndex,
    required ParsedTableLine? Function(String text, int line) parseLineAt,
  }) {
    for (var scan = line; scan >= 0; scan--) {
      final row = parseLineAt(text, scan);
      if (row == null || row.columnCount != columnCount) break;
      if (row.isSeparator) return true;
    }
    for (var scan = line + 1; scan < lineIndex.lineCount; scan++) {
      final row = parseLineAt(text, scan);
      if (row == null || row.columnCount != columnCount) break;
      if (row.isSeparator) return true;
    }
    return false;
  }

  static ParsedTableLine? findAdjacentTableLine({
    required String text,
    required int line,
    required int columnCount,
    required bool forward,
    required bool skipSeparator,
    required LineIndex lineIndex,
    required ParsedTableLine? Function(String text, int line) parseLineAt,
  }) {
    var scan = forward ? line + 1 : line - 1;
    while (scan >= 0 && scan < lineIndex.lineCount) {
      final row = parseLineAt(text, scan);
      if (row == null || row.columnCount != columnCount) return null;
      if (!skipSeparator || !row.isSeparator) return row;
      scan += forward ? 1 : -1;
    }
    return null;
  }
}

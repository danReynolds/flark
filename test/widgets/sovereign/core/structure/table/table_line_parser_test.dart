import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/core/structure/table/table_line_parser.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

void main() {
  group('TableLineParser', () {
    test('parses caret bounds for a standard table row', () {
      const text = '| a | bb | ccc |\n';
      final lineIndex = LineIndex.fromText(text);

      final row = TableLineParser.parseLineAt(
        text: text,
        line: 0,
        lineIndex: lineIndex,
        isLineInsideFencedGeometry: (_) => false,
        rowShapeResolver: (_, _, _) => const TableLineShape(
          columnCount: 3,
          isSeparator: false,
          indent: '',
        ),
      );

      expect(row, isNotNull);
      expect(row!.cells.length, 3);
      expect(row.cells.first.preferredCaret, 2);
      expect(row.cells[1].preferredCaret, 6);
      expect(row.cells[2].preferredCaret, 11);

      expect(TableLineParser.tableCellIndexForCaret(row, 1), 0);
      expect(TableLineParser.tableCellIndexForCaret(row, 6), 1);
      expect(TableLineParser.tableCellIndexForCaret(row, 14), 2);
    });

    test('returns null when line is inside fenced geometry', () {
      const text = '| a | b |\n';
      final lineIndex = LineIndex.fromText(text);

      final row = TableLineParser.parseLineAt(
        text: text,
        line: 0,
        lineIndex: lineIndex,
        isLineInsideFencedGeometry: (_) => true,
        rowShapeResolver: (_, _, _) => const TableLineShape(
          columnCount: 2,
          isSeparator: false,
          indent: '',
        ),
      );

      expect(row, isNull);
    });
  });
}

import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/core/structure/navigation/navigation_line_utils.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

void main() {
  group('NavigationLineUtils', () {
    test('line end and content end respect trailing newline', () {
      const text = 'alpha\nbeta\n';
      final index = LineIndex.fromText(text);

      final firstEndWithBreak = NavigationLineUtils.lineEndWithBreak(
        index,
        text,
        0,
      );
      final firstContentEnd = NavigationLineUtils.lineContentEnd(
        text,
        0,
        firstEndWithBreak,
      );
      expect(firstEndWithBreak, 6);
      expect(firstContentEnd, 5);

      final secondStart = index.offsetAtLine(1);
      final secondEndWithBreak = NavigationLineUtils.lineEndWithBreak(
        index,
        text,
        1,
      );
      final secondContentEnd = NavigationLineUtils.lineContentEnd(
        text,
        secondStart,
        secondEndWithBreak,
      );
      expect(secondEndWithBreak, text.length);
      expect(secondContentEnd, text.length - 1);
    });

    test('column aligned offset clamps to line width', () {
      const text = 'ab\ncdef\n';
      final index = LineIndex.fromText(text);

      final offset = NavigationLineUtils.columnAlignedOffsetForLineOrBoundary(
        text: text,
        lineIndex: index,
        line: 0,
        column: 10,
        afterDocument: false,
      );
      expect(offset, 2);
    });
  });
}

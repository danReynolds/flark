import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/vertical_caret_navigation.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

void main() {
  group('VerticalCaretNavigation', () {
    test('moves caret down preserving preferred column', () {
      const text = 'ab\ncdef\nxy\n';
      final lineIndex = LineIndex.fromText(text);
      final selection = const TextSelection.collapsed(offset: 2);

      final move1 = VerticalCaretNavigation.compute(
        selection: selection,
        text: text,
        lineIndex: lineIndex,
        forward: true,
      );
      expect(move1, isNotNull);
      expect(move1!.targetOffset, 5);
      expect(move1.preferredColumn, 2);

      final move2 = VerticalCaretNavigation.compute(
        selection: TextSelection.collapsed(offset: move1.targetOffset),
        text: text,
        lineIndex: lineIndex,
        forward: true,
        preferredColumn: move1.preferredColumn,
      );
      expect(move2, isNotNull);
      expect(move2!.targetOffset, 10);
    });

    test('returns null when movement would stay on same line', () {
      const text = 'ab';
      final lineIndex = LineIndex.fromText(text);
      final selection = const TextSelection.collapsed(offset: 1);

      final move = VerticalCaretNavigation.compute(
        selection: selection,
        text: text,
        lineIndex: lineIndex,
        forward: true,
      );
      expect(move, isNull);
    });
  });
}

import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/syntax_types.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';
import 'package:sovereign_editor/widgets/sovereign/models/sovereign_style.dart';

void main() {
  group('BlockSpan', () {
    test('length reflects range width and value equality is stable', () {
      const a = BlockSpan(
        type: BlockType.header,
        start: 3,
        end: 8,
        payload: {'level': 2},
      );
      const b = BlockSpan(
        type: BlockType.header,
        start: 3,
        end: 8,
        payload: {'level': 2},
      );
      const c = BlockSpan(type: BlockType.header, start: 3, end: 9);

      expect(a.length, 5);
      expect(a, b);
      expect(a.hashCode, b.hashCode);
      expect(a, isNot(c));
    });
  });

  group('InlineSpanToken', () {
    test(
      'length reflects range width and equality compares style and range',
      () {
        const a = InlineSpanToken(
          style: SovereignStyle.bold,
          start: 10,
          end: 14,
        );
        const b = InlineSpanToken(
          style: SovereignStyle.bold,
          start: 10,
          end: 14,
        );
        const c = InlineSpanToken(
          style: SovereignStyle.italic,
          start: 10,
          end: 14,
        );

        expect(a.length, 4);
        expect(a, b);
        expect(a.hashCode, b.hashCode);
        expect(a, isNot(c));
      },
    );
  });
}

import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';

void main() {
  group('MeasuredBlock', () {
    test('uses value semantics and includes range info in toString', () {
      const a = MeasuredBlock(
        startOffset: 0,
        endOffset: 12,
        startLine: 0,
        endLine: 2,
      );
      const b = MeasuredBlock(
        startOffset: 0,
        endOffset: 12,
        startLine: 0,
        endLine: 2,
      );
      const c = MeasuredBlock(
        startOffset: 1,
        endOffset: 12,
        startLine: 0,
        endLine: 2,
      );

      expect(a, b);
      expect(a.hashCode, b.hashCode);
      expect(a, isNot(c));
      expect(a.toString(), contains('range: 0-12'));
      expect(a.toString(), contains('lines: 0-2'));
    });
  });

  group('GeometryModel', () {
    test('empty singleton has no measured blocks', () {
      expect(GeometryModel.empty.codeBlocks, isEmpty);
      expect(GeometryModel.empty.quoteBlocks, isEmpty);
    });

    test('uses value semantics for block collections', () {
      const code = MeasuredBlock(
        startOffset: 0,
        endOffset: 20,
        startLine: 0,
        endLine: 3,
      );
      const quote = MeasuredBlock(
        startOffset: 21,
        endOffset: 40,
        startLine: 3,
        endLine: 6,
      );
      const a = GeometryModel(codeBlocks: [code], quoteBlocks: [quote]);
      const b = GeometryModel(codeBlocks: [code], quoteBlocks: [quote]);
      const c = GeometryModel(codeBlocks: [quote], quoteBlocks: [code]);

      expect(a, b);
      expect(a.hashCode, b.hashCode);
      expect(a, isNot(c));
      expect(a.toString(), contains('codeBlocks: 1'));
      expect(a.toString(), contains('quoteBlocks: 1'));
    });
  });
}

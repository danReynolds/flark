import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';

void main() {
  group('SovereignTextBuffer', () {
    test('indexes UTF-16 offsets by line', () {
      final buffer = SovereignTextBuffer('a\nbc\n');

      expect(buffer.length, 5);
      expect(buffer.lineCount, 3);
      expect(buffer.lineStart(0), 0);
      expect(buffer.lineStart(1), 2);
      expect(buffer.lineStart(2), 5);
      expect(buffer.lineEnd(0), 1);
      expect(buffer.lineEnd(1), 4);
      expect(buffer.lineEnd(2), 5);
      expect(buffer.lineAtOffset(0), 0);
      expect(buffer.lineAtOffset(1), 0);
      expect(buffer.lineAtOffset(2), 1);
      expect(buffer.lineAtOffset(4), 1);
      expect(buffer.lineAtOffset(5), 2);
    });

    test('replaces a source range immutably', () {
      final buffer = SovereignTextBuffer('hello world');
      final next = buffer.replaceRange(6, 11, 'Sovereign');

      expect(buffer.text, 'hello world');
      expect(next.text, 'hello Sovereign');
      expect(next.lineCount, 1);
    });
  });
}

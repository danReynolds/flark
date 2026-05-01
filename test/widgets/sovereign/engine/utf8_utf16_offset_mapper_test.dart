import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/utf8_utf16_offset_mapper.dart';

void main() {
  group('Utf8Utf16OffsetMapper', () {
    test('ASCII offsets map 1:1', () {
      const text = 'hello';
      final mapper = Utf8Utf16OffsetMapper.fromText(text);

      expect(mapper.utf16Length, text.length);
      expect(mapper.utf8Length, text.length);

      for (var i = 0; i <= text.length; i++) {
        expect(mapper.isUtf16ScalarBoundary(i), isTrue);
        expect(mapper.isUtf8ScalarBoundary(i), isTrue);
        expect(mapper.utf16ToUtf8(i), i);
        expect(mapper.utf8ToUtf16(i), i);
        expect(mapper.utf16ToUtf8Exact(i), i);
        expect(mapper.utf8ToUtf16Exact(i), i);
      }

      expect(mapper.utf16ToUtf8(-5), 0);
      expect(mapper.utf8ToUtf16(-5), 0);
      expect(mapper.utf16ToUtf8(99), text.length);
      expect(mapper.utf8ToUtf16(99), text.length);
    });

    test(
      'emoji surrogate pair maps with floor semantics for non-boundary offsets',
      () {
        const text = 'A🎨B';
        final mapper = Utf8Utf16OffsetMapper.fromText(text);

        expect(mapper.utf16Length, 4); // A (1) + 🎨 (2) + B (1)
        expect(mapper.utf8Length, 6); // A (1) + 🎨 (4) + B (1)

        expect(mapper.utf16ToUtf8Exact(0), 0);
        expect(mapper.utf16ToUtf8Exact(1), 1);
        expect(mapper.utf16ToUtf8Exact(3), 5);
        expect(mapper.utf16ToUtf8Exact(4), 6);
        expect(() => mapper.utf16ToUtf8Exact(2), throwsStateError);

        expect(mapper.utf8ToUtf16Exact(0), 0);
        expect(mapper.utf8ToUtf16Exact(1), 1);
        expect(mapper.utf8ToUtf16Exact(5), 3);
        expect(mapper.utf8ToUtf16Exact(6), 4);
        expect(() => mapper.utf8ToUtf16Exact(2), throwsStateError);
        expect(() => mapper.utf8ToUtf16Exact(4), throwsStateError);

        expect(mapper.utf16ToUtf8(2), 1); // inside surrogate pair -> floor
        expect(mapper.utf8ToUtf16(2), 1); // inside emoji bytes -> floor
        expect(mapper.utf8ToUtf16(4), 1); // inside emoji bytes -> floor
      },
    );

    test('round-trips scalar boundaries for mixed unicode text', () {
      const text = 'e\u0301 אב🎯z';
      final mapper = Utf8Utf16OffsetMapper.fromText(text);

      expect(mapper.utf8Length, utf8.encode(text).length);

      for (var utf16 = 0; utf16 <= mapper.utf16Length; utf16++) {
        if (!mapper.isUtf16ScalarBoundary(utf16)) continue;
        final utf8Offset = mapper.utf16ToUtf8Exact(utf16);
        expect(mapper.isUtf8ScalarBoundary(utf8Offset), isTrue);
        expect(mapper.utf8ToUtf16Exact(utf8Offset), utf16);
      }

      for (var utf8Offset = 0; utf8Offset <= mapper.utf8Length; utf8Offset++) {
        if (!mapper.isUtf8ScalarBoundary(utf8Offset)) continue;
        final utf16 = mapper.utf8ToUtf16Exact(utf8Offset);
        expect(mapper.isUtf16ScalarBoundary(utf16), isTrue);
        expect(mapper.utf16ToUtf8Exact(utf16), utf8Offset);
      }
    });

    test('unpaired surrogate handling matches dart utf8 encoder behavior', () {
      final text = String.fromCharCodes(<int>[0xD83C, 0x0041, 0xDC00]);
      final mapper = Utf8Utf16OffsetMapper.fromText(text);

      final expectedBytes = utf8.encode(text).length;
      expect(mapper.utf8Length, expectedBytes);
      expect(mapper.utf16Length, text.length);

      // U+FFFD (3 bytes), 'A' (1 byte), U+FFFD (3 bytes)
      expect(mapper.utf16ToUtf8Exact(0), 0);
      expect(mapper.utf16ToUtf8Exact(1), 3);
      expect(mapper.utf16ToUtf8Exact(2), 4);
      expect(mapper.utf16ToUtf8Exact(3), 7);
    });
  });
}

import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';

void main() {
  group('SovereignUtf8Utf16Mapper', () {
    test('maps ASCII offsets directly', () {
      final mapper = SovereignUtf8Utf16Mapper('abc');

      expect(mapper.utf16Length, 3);
      expect(mapper.utf8Length, 3);
      expect(mapper.utf8OffsetForUtf16Offset(0), 0);
      expect(mapper.utf8OffsetForUtf16Offset(3), 3);
      expect(mapper.utf16OffsetForUtf8Offset(2), 2);
    });

    test('maps BMP multibyte characters', () {
      final text = 'aéb';
      final mapper = SovereignUtf8Utf16Mapper(text);

      expect(utf8.encode(text), hasLength(4));
      expect(mapper.utf16Length, 3);
      expect(mapper.utf8Length, 4);
      expect(mapper.utf8OffsetForUtf16Offset(1), 1);
      expect(mapper.utf8OffsetForUtf16Offset(2), 3);
      expect(mapper.utf16OffsetForUtf8Offset(1), 1);
      expect(mapper.utf16OffsetForUtf8Offset(2), 1);
      expect(mapper.utf16OffsetForUtf8Offset(3), 2);
    });

    test('maps non-BMP characters across surrogate pairs', () {
      final text = 'a😀b';
      final mapper = SovereignUtf8Utf16Mapper(text);

      expect(text.length, 4);
      expect(utf8.encode(text), hasLength(6));
      expect(mapper.utf8OffsetForUtf16Offset(1), 1);
      expect(mapper.utf8OffsetForUtf16Offset(2), 1);
      expect(mapper.utf8OffsetForUtf16Offset(3), 5);
      expect(mapper.utf16OffsetForUtf8Offset(1), 1);
      expect(mapper.utf16OffsetForUtf8Offset(2), 1);
      expect(mapper.utf16OffsetForUtf8Offset(5), 3);
    });
  });
}

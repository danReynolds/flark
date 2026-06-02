import 'dart:convert';

final class FlarkUtf8Utf16Mapper {
  FlarkUtf8Utf16Mapper(String text)
    : _utf16ToUtf8 = _buildUtf16ToUtf8(text),
      _utf8ToUtf16 = _buildUtf8ToUtf16(text);

  final List<int> _utf16ToUtf8;
  final List<int> _utf8ToUtf16;

  int get utf16Length => _utf16ToUtf8.length - 1;

  int get utf8Length => _utf8ToUtf16.length - 1;

  int utf8OffsetForUtf16Offset(int utf16Offset) {
    if (utf16Offset < 0 || utf16Offset > utf16Length) {
      throw RangeError.range(utf16Offset, 0, utf16Length, 'utf16Offset');
    }
    return _utf16ToUtf8[utf16Offset];
  }

  int utf16OffsetForUtf8Offset(int utf8Offset) {
    if (utf8Offset < 0 || utf8Offset > utf8Length) {
      throw RangeError.range(utf8Offset, 0, utf8Length, 'utf8Offset');
    }
    return _utf8ToUtf16[utf8Offset];
  }

  static List<int> _buildUtf16ToUtf8(String text) {
    final mapping = List<int>.filled(text.length + 1, 0);
    var utf16Offset = 0;
    var utf8Offset = 0;

    for (final rune in text.runes) {
      final scalar = String.fromCharCode(rune);
      final utf16Length = scalar.length;
      final utf8Length = utf8.encode(scalar).length;
      mapping[utf16Offset] = utf8Offset;
      for (var i = 1; i < utf16Length; i += 1) {
        mapping[utf16Offset + i] = utf8Offset;
      }
      utf16Offset += utf16Length;
      utf8Offset += utf8Length;
      mapping[utf16Offset] = utf8Offset;
    }

    return List<int>.unmodifiable(mapping);
  }

  static List<int> _buildUtf8ToUtf16(String text) {
    final totalUtf8Length = utf8.encode(text).length;
    final mapping = List<int>.filled(totalUtf8Length + 1, 0);
    var utf16Offset = 0;
    var utf8Offset = 0;

    for (final rune in text.runes) {
      final scalar = String.fromCharCode(rune);
      final utf16Length = scalar.length;
      final utf8Length = utf8.encode(scalar).length;
      mapping[utf8Offset] = utf16Offset;
      for (var i = 1; i < utf8Length; i += 1) {
        mapping[utf8Offset + i] = utf16Offset;
      }
      utf16Offset += utf16Length;
      utf8Offset += utf8Length;
      mapping[utf8Offset] = utf16Offset;
    }

    return List<int>.unmodifiable(mapping);
  }
}

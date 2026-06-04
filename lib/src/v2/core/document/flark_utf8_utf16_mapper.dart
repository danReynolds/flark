final class FlarkUtf8Utf16Mapper {
  FlarkUtf8Utf16Mapper(String text) : this._(_buildMappings(text));

  FlarkUtf8Utf16Mapper._(_Utf8Utf16Mappings mappings)
    : _utf16ToUtf8 = mappings.utf16ToUtf8,
      _utf8ToUtf16 = mappings.utf8ToUtf16;

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

  static _Utf8Utf16Mappings _buildMappings(String text) {
    final utf16ToUtf8 = List<int>.filled(text.length + 1, 0);
    final utf8ToUtf16 = <int>[];
    var utf16Offset = 0;
    var utf8Offset = 0;

    while (utf16Offset < text.length) {
      final unit = text.codeUnitAt(utf16Offset);
      final utf16Length = _utf16ScalarLength(text, utf16Offset, unit);
      final utf8Length = _utf8ScalarLength(unit, utf16Length);
      utf16ToUtf8[utf16Offset] = utf8Offset;
      for (var i = 1; i < utf16Length; i += 1) {
        utf16ToUtf8[utf16Offset + i] = utf8Offset;
      }
      for (var i = 0; i < utf8Length; i += 1) {
        utf8ToUtf16.add(utf16Offset);
      }
      utf16Offset += utf16Length;
      utf8Offset += utf8Length;
      utf16ToUtf8[utf16Offset] = utf8Offset;
    }

    utf8ToUtf16.add(text.length);
    return _Utf8Utf16Mappings(
      utf16ToUtf8: List<int>.unmodifiable(utf16ToUtf8),
      utf8ToUtf16: List<int>.unmodifiable(utf8ToUtf16),
    );
  }

  static int _utf16ScalarLength(String text, int offset, int unit) {
    if (!_isHighSurrogate(unit) || offset + 1 >= text.length) return 1;
    return _isLowSurrogate(text.codeUnitAt(offset + 1)) ? 2 : 1;
  }

  static int _utf8ScalarLength(int unit, int utf16Length) {
    if (utf16Length == 2) return 4;
    if (unit <= 0x7F) return 1;
    if (unit <= 0x7FF) return 2;
    return 3;
  }

  static bool _isHighSurrogate(int unit) => unit >= 0xD800 && unit <= 0xDBFF;

  static bool _isLowSurrogate(int unit) => unit >= 0xDC00 && unit <= 0xDFFF;
}

final class _Utf8Utf16Mappings {
  const _Utf8Utf16Mappings({
    required this.utf16ToUtf8,
    required this.utf8ToUtf16,
  });

  final List<int> utf16ToUtf8;
  final List<int> utf8ToUtf16;
}

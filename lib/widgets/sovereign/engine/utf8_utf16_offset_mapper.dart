import 'dart:convert';
import 'dart:typed_data';

/// Maps offsets between UTF-16 code units (Dart-native string indexing) and
/// UTF-8 byte offsets (common parser indexing).
///
/// Behavior notes:
/// - Offsets are clamped to valid ranges.
/// - Non-boundary offsets map using floor semantics.
/// - `...Exact` methods require scalar boundaries and throw otherwise.
class Utf8Utf16OffsetMapper {
  final String text;
  final Uint32List _utf16ToUtf8Floor;
  final Uint32List _utf16Boundaries;
  final Uint32List _utf8Boundaries;

  Utf8Utf16OffsetMapper._(
    this.text,
    this._utf16ToUtf8Floor,
    this._utf16Boundaries,
    this._utf8Boundaries,
  );

  factory Utf8Utf16OffsetMapper.fromText(String text) {
    final utf16ToUtf8Floor = Uint32List(text.length + 1);
    final utf16Boundaries = <int>[0];
    final utf8Boundaries = <int>[0];

    var utf16 = 0;
    var utf8ByteOffset = 0;
    while (utf16 < text.length) {
      final (scalar, utf16Len) = _decodeScalar(text, utf16);
      final utf8Len = _utf8LengthForScalar(scalar);

      for (var i = 0; i < utf16Len; i++) {
        utf16ToUtf8Floor[utf16 + i] = utf8ByteOffset;
      }

      utf16 += utf16Len;
      utf8ByteOffset += utf8Len;
      utf16ToUtf8Floor[utf16] = utf8ByteOffset;
      utf16Boundaries.add(utf16);
      utf8Boundaries.add(utf8ByteOffset);
    }

    assert(() {
      final encodedLength = utf8.encode(text).length;
      return encodedLength == utf8ByteOffset;
    }(), 'UTF-8 mapper length mismatch');

    return Utf8Utf16OffsetMapper._(
      text,
      utf16ToUtf8Floor,
      Uint32List.fromList(utf16Boundaries),
      Uint32List.fromList(utf8Boundaries),
    );
  }

  int get utf16Length => text.length;
  int get utf8Length => _utf8Boundaries.last;

  int utf16ToUtf8(int utf16Offset) {
    final safe = utf16Offset.clamp(0, utf16Length);
    return _utf16ToUtf8Floor[safe];
  }

  int utf8ToUtf16(int utf8Offset) {
    final safe = utf8Offset.clamp(0, utf8Length);
    final floorIndex = _floorBoundaryIndex(_utf8Boundaries, safe);
    return _utf16Boundaries[floorIndex];
  }

  int utf16ToUtf8Exact(int utf16Offset) {
    final safe = utf16Offset.clamp(0, utf16Length);
    if (!isUtf16ScalarBoundary(safe)) {
      throw StateError('UTF-16 offset $safe is not a scalar boundary.');
    }
    return _utf16ToUtf8Floor[safe];
  }

  int utf8ToUtf16Exact(int utf8Offset) {
    final safe = utf8Offset.clamp(0, utf8Length);
    if (!isUtf8ScalarBoundary(safe)) {
      throw StateError('UTF-8 offset $safe is not a scalar boundary.');
    }
    final boundaryIndex = _exactBoundaryIndex(_utf8Boundaries, safe);
    return _utf16Boundaries[boundaryIndex];
  }

  bool isUtf16ScalarBoundary(int utf16Offset) {
    final safe = utf16Offset.clamp(0, utf16Length);
    return _exactBoundaryIndex(_utf16Boundaries, safe) != -1;
  }

  bool isUtf8ScalarBoundary(int utf8Offset) {
    final safe = utf8Offset.clamp(0, utf8Length);
    return _exactBoundaryIndex(_utf8Boundaries, safe) != -1;
  }

  static int _floorBoundaryIndex(Uint32List boundaries, int value) {
    var lo = 0;
    var hi = boundaries.length - 1;
    while (lo <= hi) {
      final mid = (lo + hi) >> 1;
      final current = boundaries[mid];
      if (current == value) return mid;
      if (current < value) {
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return hi < 0 ? 0 : hi;
  }

  static int _exactBoundaryIndex(Uint32List boundaries, int value) {
    var lo = 0;
    var hi = boundaries.length - 1;
    while (lo <= hi) {
      final mid = (lo + hi) >> 1;
      final current = boundaries[mid];
      if (current == value) return mid;
      if (current < value) {
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return -1;
  }

  static (int scalar, int utf16Len) _decodeScalar(String text, int offset) {
    final first = text.codeUnitAt(offset);
    final hasNext = offset + 1 < text.length;
    final isHigh = first >= 0xD800 && first <= 0xDBFF;
    if (isHigh && hasNext) {
      final second = text.codeUnitAt(offset + 1);
      final isLow = second >= 0xDC00 && second <= 0xDFFF;
      if (isLow) {
        final hi = first - 0xD800;
        final lo = second - 0xDC00;
        final scalar = 0x10000 + ((hi << 10) | lo);
        return (scalar, 2);
      }
    }

    if (first >= 0xD800 && first <= 0xDFFF) {
      // Match UTF-8 encoder behavior for invalid scalar values.
      return (0xFFFD, 1);
    }

    return (first, 1);
  }

  static int _utf8LengthForScalar(int scalar) {
    if (scalar <= 0x7F) return 1;
    if (scalar <= 0x7FF) return 2;
    if (scalar <= 0xFFFF) return 3;
    return 4;
  }
}

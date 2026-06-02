final class FlarkTextBuffer {
  FlarkTextBuffer(this.text) : _lineStarts = _computeLineStarts(text);

  final String text;
  final List<int> _lineStarts;

  int get length => text.length;

  int get lineCount => _lineStarts.length;

  List<int> get lineStarts => List<int>.unmodifiable(_lineStarts);

  int lineStart(int lineIndex) {
    if (lineIndex < 0 || lineIndex >= _lineStarts.length) {
      throw RangeError.range(lineIndex, 0, _lineStarts.length - 1, 'lineIndex');
    }
    return _lineStarts[lineIndex];
  }

  int lineEnd(int lineIndex) {
    final start = lineStart(lineIndex);
    final nextStart = lineIndex + 1 < _lineStarts.length
        ? _lineStarts[lineIndex + 1]
        : text.length;
    if (nextStart > start && text.codeUnitAt(nextStart - 1) == 0x0A) {
      return nextStart - 1;
    }
    return nextStart;
  }

  int lineAtOffset(int offset) {
    _checkOffset(offset);

    var low = 0;
    var high = _lineStarts.length - 1;
    while (low <= high) {
      final mid = low + ((high - low) >> 1);
      final start = _lineStarts[mid];
      if (start == offset) return mid;
      if (start < offset) {
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return high;
  }

  FlarkTextBuffer replaceRange(int start, int end, String replacement) {
    _checkRange(start, end);
    return FlarkTextBuffer(text.replaceRange(start, end, replacement));
  }

  void _checkOffset(int offset) {
    if (offset < 0 || offset > text.length) {
      throw RangeError.range(offset, 0, text.length, 'offset');
    }
  }

  void _checkRange(int start, int end) {
    _checkOffset(start);
    _checkOffset(end);
    if (start > end) {
      throw RangeError.range(start, 0, end, 'start');
    }
  }

  static List<int> _computeLineStarts(String text) {
    final starts = <int>[0];
    for (var i = 0; i < text.length; i += 1) {
      if (text.codeUnitAt(i) == 0x0A) {
        starts.add(i + 1);
      }
    }
    return List<int>.unmodifiable(starts);
  }
}

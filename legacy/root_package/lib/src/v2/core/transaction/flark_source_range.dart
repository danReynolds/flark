final class FlarkSourceRange {
  const FlarkSourceRange(this.start, this.end);

  final int start;
  final int end;

  int get length => end - start;

  bool get isCollapsed => start == end;

  bool containsOffset(int offset) => offset >= start && offset <= end;

  bool containsRange(FlarkSourceRange other) {
    return other.start >= start && other.end <= end;
  }

  bool intersects(FlarkSourceRange other) {
    return start < other.end && other.start < end;
  }

  FlarkSourceRange union(FlarkSourceRange other) {
    return FlarkSourceRange(
      start < other.start ? start : other.start,
      end > other.end ? end : other.end,
    );
  }

  FlarkSourceRange validate(int textLength) {
    _checkOffset(start, textLength, 'start');
    _checkOffset(end, textLength, 'end');
    if (start > end) {
      throw RangeError.range(start, 0, end, 'start');
    }
    return this;
  }

  static void _checkOffset(int offset, int textLength, String name) {
    if (offset < 0 || offset > textLength) {
      throw RangeError.range(offset, 0, textLength, name);
    }
  }

  @override
  bool operator ==(Object other) {
    return other is FlarkSourceRange &&
        other.start == start &&
        other.end == end;
  }

  @override
  int get hashCode => Object.hash(start, end);

  @override
  String toString() => 'FlarkSourceRange($start, $end)';
}

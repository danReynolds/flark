final class SovereignSourceRange {
  const SovereignSourceRange(this.start, this.end);

  final int start;
  final int end;

  int get length => end - start;

  bool get isCollapsed => start == end;

  bool containsOffset(int offset) => offset >= start && offset <= end;

  bool containsRange(SovereignSourceRange other) {
    return other.start >= start && other.end <= end;
  }

  bool intersects(SovereignSourceRange other) {
    return start < other.end && other.start < end;
  }

  SovereignSourceRange union(SovereignSourceRange other) {
    return SovereignSourceRange(
      start < other.start ? start : other.start,
      end > other.end ? end : other.end,
    );
  }

  SovereignSourceRange validate(int textLength) {
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
    return other is SovereignSourceRange &&
        other.start == start &&
        other.end == end;
  }

  @override
  int get hashCode => Object.hash(start, end);

  @override
  String toString() => 'SovereignSourceRange($start, $end)';
}

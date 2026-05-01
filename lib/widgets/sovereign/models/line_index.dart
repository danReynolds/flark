/// A compact index of line start offsets.
///
/// Allows O(log N) line lookups and O(1) offset lookups.
///
/// Invariants:
/// 1. [lineStarts] always contains at least `0`.
/// 2. [lineStarts] is strictly increasing.
class LineIndex {
  final List<int> lineStarts;
  final int maxColumn;

  LineIndex(this.lineStarts, this.maxColumn);

  factory LineIndex.empty() => LineIndex([0], 0);

  /// Rebuilds the index from the full text.
  ///
  /// Optimization for Phase 1: Fast enough for < 1MB on UI thread.
  factory LineIndex.fromText(String text) {
    if (text.isEmpty) return LineIndex.empty();

    final starts = <int>[0];
    int offset = 0;
    int maxCol = 0;

    while (true) {
      final index = text.indexOf('\n', offset);
      if (index == -1) {
        // Check last line length
        final len = text.length - offset;
        if (len > maxCol) maxCol = len;
        break;
      }

      final len = index - offset;
      if (len > maxCol) maxCol = len;

      offset = index + 1;
      starts.add(offset);
    }
    return LineIndex(starts, maxCol);
  }

  int get lineCount => lineStarts.length;

  /// Returns the line number containing [offset].
  int lineAtOffset(int offset) {
    if (offset < 0) return 0;
    if (lineStarts.isEmpty) return 0;
    if (offset >= lineStarts.last && lineStarts.length > 1) {
      // Optimization: Checking last line first is common for typing at end.
      return lineStarts.length - 1;
    }

    // Binary search
    int low = 0;
    int high = lineStarts.length - 1;

    while (low <= high) {
      final mid = (low + high) ~/ 2;
      final start = lineStarts[mid];

      if (start == offset) {
        return mid;
      } else if (start < offset) {
        low = mid + 1;
      } else {
        high = mid - 1;
      }
    }
    return high < 0 ? 0 : high;
  }

  /// Returns the start offset of [line].
  int offsetAtLine(int line) {
    if (line < 0) return 0;
    if (line >= lineStarts.length) return lineStarts.last;
    return lineStarts[line];
  }
}

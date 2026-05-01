class FencedCodeBlock {
  final int start;
  final int end;

  const FencedCodeBlock(this.start, this.end);

  @override
  String toString() => 'FencedCodeBlock($start-$end)';
}

/// Shared scanner for V1 fenced code blocks.
///
/// Dialect (intentionally simplified):
/// - Fence opener: ``` at column 0
/// - Fence closer: ``` at column 0
/// - Info string allowed on opening line (ignored)
/// - If no closing fence exists, the block runs to EOF
class FencedCodeScanner {
  const FencedCodeScanner();

  static bool startsFence(String text, int offset) {
    // Only supports ``` fences in V1.
    return text.startsWith('```', offset);
  }

  static int endOfLine(String text, int start) {
    final idx = text.indexOf('\n', start);
    return idx == -1 ? text.length : idx + 1;
  }

  /// Returns the exclusive end offset for a fenced code block that starts at
  /// [startOffset]. Callers must ensure [startsFence(text, startOffset)].
  static int blockEnd(String text, int startOffset) {
    final contentStart = endOfLine(text, startOffset);
    int lineStart = contentStart;

    // Walk line starts so we only accept column-0 closing fences.
    while (lineStart < text.length) {
      if (startsFence(text, lineStart)) {
        return endOfLine(text, lineStart);
      }
      lineStart = endOfLine(text, lineStart);
    }

    return text.length;
  }

  /// Scans the whole document for fenced code blocks.
  ///
  /// NOTE: This only finds fences that begin at column 0. The scan walks line
  /// starts (offsets returned by [endOfLine]) to maintain that invariant.
  static List<FencedCodeBlock> scan(String text) {
    if (text.isEmpty) return const [];

    final blocks = <FencedCodeBlock>[];
    int offset = 0;

    while (offset < text.length) {
      if (startsFence(text, offset)) {
        final end = blockEnd(text, offset);
        blocks.add(FencedCodeBlock(offset, end));
        offset = end;
        continue;
      }

      offset = endOfLine(text, offset);
    }

    return blocks;
  }
}

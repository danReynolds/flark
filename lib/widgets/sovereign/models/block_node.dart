/// Structural markdown block kinds recognized by Sovereign.
enum BlockType {
  /// Plain paragraph text.
  paragraph,

  /// ATX or Setext heading block.
  header,

  /// Thematic break block.
  thematicBreak,

  /// Fenced code block.
  fencedCode,

  /// Blockquote block.
  blockquote,

  /// Unordered list block.
  unorderedList,

  /// Ordered list block.
  orderedList,

  /// Markdown table block.
  table,
}

/// A structural block in the document.
///
/// Invariant: [end] > [start].
class BlockNode {
  /// Structural kind for this block.
  final BlockType type;

  /// Start offset (inclusive).
  final int start;

  /// End offset (exclusive).
  final int end;

  /// Optional payload (e.g. language, header level).
  final Map<String, dynamic>? payload;

  /// Creates a structural block spanning `[start, end)`.
  const BlockNode({
    required this.type,
    required this.start,
    required this.end,
    this.payload,
  });

  /// Number of UTF-16 code units covered by this block.
  int get length => end - start;

  /// Returns a copy with selected fields replaced.
  BlockNode copyWith({
    BlockType? type,
    int? start,
    int? end,
    Map<String, dynamic>? payload,
  }) {
    return BlockNode(
      type: type ?? this.type,
      start: start ?? this.start,
      end: end ?? this.end,
      payload: payload ?? this.payload,
    );
  }

  @override
  String toString() => '$type[$start-$end]';
}

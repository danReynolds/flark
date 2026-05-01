enum BlockType {
  paragraph,
  header,
  thematicBreak,
  fencedCode,
  blockquote,
  unorderedList,
  orderedList,
  table,
}

/// A structural block in the document.
///
/// Invariant: [end] > [start].
class BlockNode {
  final BlockType type;

  /// Start offset (inclusive).
  final int start;

  /// End offset (exclusive).
  final int end;

  /// Optional payload (e.g. language, header level).
  final Map<String, dynamic>? payload;

  const BlockNode({
    required this.type,
    required this.start,
    required this.end,
    this.payload,
  });

  int get length => end - start;

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

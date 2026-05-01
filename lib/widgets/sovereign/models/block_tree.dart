import 'block_node.dart';

/// The read-only metadata overlay (Syntax Tree).
///
/// Invariants:
/// 1. Blocks are sorted by `start` offset.
/// 2. Blocks are non-overlapping.
class BlockTree {
  final List<BlockNode> blocks;

  const BlockTree(this.blocks);

  factory BlockTree.empty() => const BlockTree([]);

  /// Returns the block containing [offset].
  ///
  /// If [offset] is in a gap, returns a synthetic [BlockType.paragraph].
  BlockNode nodeAt(int offset) {
    if (blocks.isEmpty) {
      // Fallback for empty tree.
      return BlockNode(type: BlockType.paragraph, start: 0, end: offset + 1);
    }

    int low = 0;
    int high = blocks.length - 1;

    while (low <= high) {
      final mid = (low + high) ~/ 2;
      final block = blocks[mid];

      if (offset >= block.start && offset < block.end) {
        return block;
      } else if (offset < block.start) {
        high = mid - 1;
      } else {
        low = mid + 1;
      }
    }

    // Not found in a block -> implicit Paragraph (The Gap).
    // Identify boundaries of the gap.
    int gapStart = 0;
    int gapEnd = 999999999; // Effectively infinite for queries

    // 'high' is the index of the block strictly before 'offset'.
    // 'low' is the index of the block strictly after 'offset'.

    if (high >= 0) {
      gapStart = blocks[high].end;
    }
    if (low < blocks.length) {
      gapEnd = blocks[low].start;
    }

    return BlockNode(type: BlockType.paragraph, start: gapStart, end: gapEnd);
  }
}

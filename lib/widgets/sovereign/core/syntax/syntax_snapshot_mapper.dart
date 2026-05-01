import '../../engine/syntax_snapshot.dart';
import '../../models/block_node.dart';
import '../../models/block_tree.dart';

class SyntaxSnapshotMapper {
  const SyntaxSnapshotMapper._();

  static BlockTree blockTreeFromSnapshot(SyntaxSnapshot snapshot) {
    if (snapshot.blocks.isEmpty) return BlockTree.empty();

    final blocks = <BlockNode>[
      for (final span in snapshot.blocks)
        if (span.end > span.start)
          BlockNode(
            type: span.type,
            start: span.start,
            end: span.end,
            payload: span.payload.isEmpty
                ? null
                : Map<String, dynamic>.from(span.payload),
          ),
    ]..sort((a, b) {
        final byStart = a.start.compareTo(b.start);
        if (byStart != 0) return byStart;
        return a.end.compareTo(b.end);
      });
    return BlockTree(blocks);
  }
}

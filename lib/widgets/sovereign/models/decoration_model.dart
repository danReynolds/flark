import 'package:flutter/services.dart';
import 'block_tree.dart';
import 'line_index.dart';

/// Render-facing decoration metadata for a controller revision.
class DecorationModel {
  /// Structural block tree matched to [originRevision].
  final BlockTree tree;

  /// Line index used to map decoration ranges to visual lines.
  final LineIndex lineIndex;

  /// Controller text revision that produced [tree].
  final int originRevision;

  /// Phase 5 active formatting projection.
  /// The hidden ranges (collapsed markers) in Storage Space.
  final List<TextRange> hiddenRanges;

  /// Phase 5 active formatting projection.
  /// Epoch increments whenever [hiddenRanges] changes (pop/collapse),
  /// forcing a rebuild even if the text revision has not changed.
  final int projectionEpoch;

  /// Creates decoration metadata for render layers.
  const DecorationModel({
    required this.tree,
    required this.lineIndex,
    required this.originRevision,
    this.hiddenRanges = const [],
    this.projectionEpoch = 0,
  });

  /// Creates an empty decoration model.
  factory DecorationModel.empty() {
    return DecorationModel(
      tree: BlockTree.empty(),
      lineIndex: LineIndex.empty(),
      originRevision: 0,
      hiddenRanges: const [],
      projectionEpoch: 0,
    );
  }

  /// Returns a copy with selected decoration fields replaced.
  DecorationModel copyWith({
    BlockTree? tree,
    LineIndex? lineIndex,
    int? originRevision,
    List<TextRange>? hiddenRanges,
    int? projectionEpoch,
  }) {
    return DecorationModel(
      tree: tree ?? this.tree,
      lineIndex: lineIndex ?? this.lineIndex,
      originRevision: originRevision ?? this.originRevision,
      hiddenRanges: hiddenRanges ?? this.hiddenRanges,
      projectionEpoch: projectionEpoch ?? this.projectionEpoch,
    );
  }
}

import 'package:flutter/services.dart';
import 'block_tree.dart';
import 'line_index.dart';

class DecorationModel {
  final BlockTree tree;

  /// Semantic LineIndex matched to the request revision.
  /// Note: The View might use a newer LineIndex from the Controller for Tier 1 mapping,
  /// but this model carries the index used during parsing if needed (e.g. for block Y calcs).
  ///
  /// Actually, Spec 3.7 says: "DecorationModel may carry a stale BlockTree but must reference the current LineIndex".
  /// So the Painter should pull the current lineIndex from the Controller?
  /// Or the Controller pushes the *current* LineIndex into this model when emitting?
  ///
  /// Let's stick to the model carrying the index it was built with,
  /// BUT the spec says "must reference the current LineIndex".
  ///
  /// Simpler: The Stream emits (Tree, Index).
  /// If Index is updated synchronously, the Controller pushes a new DecorationModel
  /// with (OldTree, NewIndex) immediately on every edit?
  ///
  /// Yes, that keeps the Painter reactive.
  final LineIndex lineIndex;

  final int originRevision;

  /// [Phase 5] Active Formatting Projection
  /// The hidden ranges (collapsed markers) in Storage Space.
  final List<TextRange> hiddenRanges;

  /// [Phase 5] Active Formatting Projection
  /// Epoch increments whenever [hiddenRanges] changes (pop/collapse),
  /// forcing a rebuild even if [textRevision] hasn't changed.
  final int projectionEpoch;

  const DecorationModel({
    required this.tree,
    required this.lineIndex,
    required this.originRevision,
    this.hiddenRanges = const [],
    this.projectionEpoch = 0,
  });

  factory DecorationModel.empty() {
    return DecorationModel(
      tree: BlockTree.empty(),
      lineIndex: LineIndex.empty(),
      originRevision: 0,
      hiddenRanges: const [],
      projectionEpoch: 0,
    );
  }

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

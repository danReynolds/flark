part of 'editor.dart';

/// Shape eligibility for synchronous projection. Hosts qualify these limits
/// together with [FlarkEditor.syncLimit] on their actual input/paint path.
final class FlarkLiveLimits {
  const FlarkLiveLimits({
    this.lines = 2048,
    this.lineCodeUnits = 4096,
    this.blocks = 2048,
    this.runs = 8192,
    this.blockCodeUnits = 16384,
    this.containerDepth = 8,
  });
  final int lines, lineCodeUnits, blocks, runs, blockCodeUnits;

  /// Maximum combined quote and list-item nesting, matching projected shells.
  final int containerDepth;

  bool _admitsModel(RenderModel model) {
    if (model.blockCount > blocks || model.runCount > runs) return false;
    final depths = List<int>.filled(model.blockCount, 0);
    for (final block in model.blocks) {
      final parentDepth = block.parent == noParent ? 0 : depths[block.parent];
      final depth =
          parentDepth +
          (block.kind == BlockKind.blockQuote || block.kind == BlockKind.item
              ? 1
              : 0);
      if (depth > containerDepth) return false;
      depths[block.index] = depth;
      if (block.isLeaf && block.endUtf16 - block.startUtf16 > blockCodeUnits) {
        return false;
      }
    }
    return true;
  }

  bool _admitsSource(String source) {
    var lineCount = 1, width = 0;
    for (var i = 0; i < source.length; i++) {
      if (source.codeUnitAt(i) == 10) {
        if (++lineCount > lines) return false;
        width = 0;
      } else if (++width > lineCodeUnits) {
        return false;
      }
    }
    return lineCount <= lines;
  }
}

enum FlarkRejection {
  staleRevision,
  unsupportedEdit,
  invalidSource,
  extractionDeviation,
  sourceLimit,
}

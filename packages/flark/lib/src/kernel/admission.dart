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

  bool _admitsStats(_SourceStats stats) =>
      stats.lines <= lines && stats.widestLine <= lineCodeUnits;

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

/// Everything a commit checks before parsing, gathered in one pass over the
/// candidate: the source contract (no bare CR, well-formed UTF-16), UTF-8
/// size, line count and widest line in code units. Separate checks walked the
/// whole document four times per keystroke.
final class _SourceStats {
  const _SourceStats._(this.valid, this.utf8Bytes, this.lines, this.widestLine);

  static const _invalid = _SourceStats._(false, 0, 0, 0);

  factory _SourceStats.of(String source) {
    var bytes = 0, lines = 1, width = 0, widest = 0;
    for (var i = 0; i < source.length; i++) {
      final unit = source.codeUnitAt(i);
      if (unit == 0x0A) {
        bytes++;
        lines++;
        if (width > widest) widest = width;
        width = 0;
        continue;
      }
      width++;
      if (unit <= 0x7F) {
        if (unit == 0x0D &&
            (i + 1 == source.length || source.codeUnitAt(i + 1) != 0x0A)) {
          return _invalid;
        }
        bytes++;
      } else if (unit <= 0x7FF) {
        bytes += 2;
      } else if (unit >= 0xD800 && unit <= 0xDBFF) {
        if (i + 1 == source.length) return _invalid;
        final low = source.codeUnitAt(i + 1);
        if (low < 0xDC00 || low > 0xDFFF) return _invalid;
        bytes += 4;
        width++;
        i++;
      } else if (unit >= 0xDC00 && unit <= 0xDFFF) {
        return _invalid;
      } else {
        bytes += 3;
      }
    }
    return _SourceStats._(true, bytes, lines, width > widest ? width : widest);
  }

  final bool valid;
  final int utf8Bytes, lines, widestLine;
}

enum FlarkRejection {
  staleRevision,
  unsupportedEdit,
  invalidSource,
  extractionDeviation,
  sourceLimit,
}

import 'package:flutter/foundation.dart';

/// Represents the geometry of a single block for painting purposes.
///
/// This is a "Tier 1" Synchronous data structure. It is derived directly
/// from the current [TextEditingValue] and [LineIndex] in the same
/// microtask as the text update.
///
/// It strictly represents *Visual Extent* (lines), not just logical content.
@immutable
class MeasuredBlock {
  /// The inclusive start offset of the block in the text.
  final int startOffset;

  /// The exclusive end offset of the block in the text.
  final int endOffset;

  /// The visual start line index (0-based).
  final int startLine;

  /// The visual end line index (exclusive).
  ///
  /// Painting typically occurs from [startLine] * lineHeight to [endLine] * lineHeight.
  ///
  /// **Invariant**: If the block ends with a newline character, this [endLine]
  /// MUST include the subsequent empty line to strictly prevent background lag
  /// on "Enter".
  final int endLine;

  /// Optional paint start line (exclusive semantics are handled by [paintEndLine]).
  ///
  /// Defaults to [startLine]. For fenced code blocks we can override this to
  /// skip painting hidden marker lines while keeping source geometry unchanged.
  final int paintStartLine;

  /// Optional paint end line (exclusive).
  ///
  /// Defaults to [endLine]. For fenced code blocks we can override this to
  /// skip painting hidden marker lines while keeping source geometry unchanged.
  final int paintEndLine;

  const MeasuredBlock({
    required this.startOffset,
    required this.endOffset,
    required this.startLine,
    required this.endLine,
    int? paintStartLine,
    int? paintEndLine,
  })  : paintStartLine = paintStartLine ?? startLine,
        paintEndLine = paintEndLine ?? endLine;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is MeasuredBlock &&
          runtimeType == other.runtimeType &&
          startOffset == other.startOffset &&
          endOffset == other.endOffset &&
          startLine == other.startLine &&
          endLine == other.endLine &&
          paintStartLine == other.paintStartLine &&
          paintEndLine == other.paintEndLine;

  @override
  int get hashCode =>
      startOffset.hashCode ^
      endOffset.hashCode ^
      startLine.hashCode ^
      endLine.hashCode ^
      paintStartLine.hashCode ^
      paintEndLine.hashCode;

  @override
  String toString() {
    return 'MeasuredBlock(range: $startOffset-$endOffset, lines: $startLine-$endLine, paint: $paintStartLine-$paintEndLine)';
  }
}

/// The authoritative synchronous geometry model for the Sovereign Editor.
///
/// This model separates "Tier 1" geometry (critical for background painting)
/// from "Tier 2" decoration (syntax highlighting).
///
/// It MUST be updated in the same transaction as [SovereignController.value].
@immutable
class GeometryModel {
  /// The list of fenced code blocks identified by the synchronous scanner.
  final List<MeasuredBlock> codeBlocks;

  /// The list of blockquote blocks identified by the synchronous scanner.
  final List<MeasuredBlock> quoteBlocks;

  const GeometryModel({
    this.codeBlocks = const [],
    this.quoteBlocks = const [],
  });

  static const empty = GeometryModel();

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is GeometryModel &&
          runtimeType == other.runtimeType &&
          listEquals(codeBlocks, other.codeBlocks) &&
          listEquals(quoteBlocks, other.quoteBlocks);

  @override
  int get hashCode => Object.hash(codeBlocks, quoteBlocks);

  @override
  String toString() =>
      'GeometryModel(codeBlocks: ${codeBlocks.length}, quoteBlocks: ${quoteBlocks.length})';
}

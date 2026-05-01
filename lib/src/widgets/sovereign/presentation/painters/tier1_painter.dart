import 'package:flutter/widgets.dart';
import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';

// --- TIER 1 PAINTER (Synchronous Backgrounds) ---
// Renders active fenced code blocks purely from synchronous geometry.
//
// Hardened Contract (RFC 007):
// 1. Pure Renderer: No text inspection allowed. Must use GeometryModel.
// 2. Sync Invalidation: Must repaint on every SovereignController value change.
class Tier1Painter extends CustomPainter {
  final GeometryModel geometry;
  final double lineHeight;
  final Offset viewport;
  final EdgeInsets contentPadding;
  final Color codeBlockBackgroundColor;
  final BorderRadius codeBlockBorderRadius;
  final double codeBlockHorizontalInset;
  final double codeBlockVerticalInset;
  final Color quoteRailColor;
  final double quoteRailWidth;
  final double quoteRailInset;
  final Radius quoteRailRadius;

  Tier1Painter({
    required this.geometry,
    required this.lineHeight,
    required this.viewport,
    this.contentPadding = EdgeInsets.zero,
    required this.codeBlockBackgroundColor,
    required this.codeBlockBorderRadius,
    this.codeBlockHorizontalInset = 2.0,
    this.codeBlockVerticalInset = 1.0,
    required this.quoteRailColor,
    this.quoteRailWidth = 4.0,
    this.quoteRailInset = 8.0,
    this.quoteRailRadius = const Radius.circular(2.0),
  });

  @override
  void paint(Canvas canvas, Size size) {
    // 1. Draw Code Blocks
    final fillPaint = Paint()
      ..color = codeBlockBackgroundColor
      ..isAntiAlias = true;

    for (final block in geometry.codeBlocks) {
      // GeometryModel gives us authoritative PAINT EXTENT (lines).
      // RFC 007: MeasuredBlock already accounts for trailing empty lines.
      // We just iterate startLine -> endLine.

      final top = contentPadding.top + (block.paintStartLine * lineHeight);
      final bottom = contentPadding.top + (block.paintEndLine * lineHeight);

      // Culling optimization
      if (bottom - viewport.dy < 0 || top - viewport.dy > size.height) {
        continue;
      }

      // Slight inset + rounded corners to match markdown-body code block shape.
      final rawTop = top - codeBlockVerticalInset;
      final clampedTop = rawTop.clamp(contentPadding.top, double.infinity);
      final clippedTopInset = clampedTop - rawTop;
      final adjustedBottom = (bottom + codeBlockVerticalInset - clippedTopInset)
          .clamp(clampedTop + 1.0, double.infinity);
      final blockRect = Rect.fromLTRB(
        contentPadding.left + codeBlockHorizontalInset,
        clampedTop.toDouble(),
        size.width - contentPadding.right - codeBlockHorizontalInset,
        adjustedBottom.toDouble(),
      );
      if (blockRect.width <= 0 || blockRect.height <= 0) {
        continue;
      }
      final radiusCap = 8.0;
      final effectiveRadius = BorderRadius.only(
        topLeft: Radius.circular(
          codeBlockBorderRadius.topLeft.x.clamp(3.0, radiusCap),
        ),
        topRight: Radius.circular(
          codeBlockBorderRadius.topRight.x.clamp(3.0, radiusCap),
        ),
        bottomLeft: Radius.circular(
          codeBlockBorderRadius.bottomLeft.x.clamp(3.0, radiusCap),
        ),
        bottomRight: Radius.circular(
          codeBlockBorderRadius.bottomRight.x.clamp(3.0, radiusCap),
        ),
      );
      final rrect = effectiveRadius.toRRect(blockRect);
      canvas.drawRRect(rrect, fillPaint);
    }

    // 2. Draw Blockquote rails
    final railPaint = Paint()
      ..color = quoteRailColor
      ..isAntiAlias = true;

    for (final block in geometry.quoteBlocks) {
      final top = contentPadding.top + (block.startLine * lineHeight);
      final bottom = contentPadding.top + (block.endLine * lineHeight);

      if (bottom - viewport.dy < 0 || top - viewport.dy > size.height) {
        continue;
      }

      final blockHeight = bottom - top;
      if (blockHeight <= 0) continue;
      final verticalPad = (lineHeight * 0.12).clamp(
        0.0,
        ((blockHeight - 1.0) / 2).clamp(0.0, double.infinity),
      );
      final railTop = top + verticalPad;
      final railHeight = (blockHeight - (verticalPad * 2)).clamp(
        1.0,
        blockHeight,
      );
      final railRect = Rect.fromLTWH(
        contentPadding.left + quoteRailInset,
        railTop,
        quoteRailWidth,
        railHeight,
      );
      canvas.drawRRect(
        RRect.fromRectAndRadius(railRect, quoteRailRadius),
        railPaint,
      );
    }
  }

  @override
  bool shouldRepaint(covariant Tier1Painter oldDelegate) {
    return oldDelegate.geometry != geometry ||
        oldDelegate.lineHeight != lineHeight ||
        oldDelegate.contentPadding != contentPadding ||
        oldDelegate.codeBlockBackgroundColor != codeBlockBackgroundColor ||
        oldDelegate.codeBlockBorderRadius != codeBlockBorderRadius ||
        oldDelegate.codeBlockHorizontalInset != codeBlockHorizontalInset ||
        oldDelegate.codeBlockVerticalInset != codeBlockVerticalInset ||
        oldDelegate.quoteRailColor != quoteRailColor ||
        oldDelegate.quoteRailWidth != quoteRailWidth ||
        oldDelegate.quoteRailInset != quoteRailInset ||
        oldDelegate.quoteRailRadius != quoteRailRadius ||
        oldDelegate.viewport != viewport;
  }
}

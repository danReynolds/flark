import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/decoration_model.dart';

/// RFC 004 projection logic for active formatting.
/// Handles coordinate mapping between Storage Space (S) and Presentation Space (P).
class Projector {
  final DecorationModel model;

  const Projector(this.model);

  bool get hasHiddenRanges => model.hiddenRanges.isNotEmpty;

  /// The Choke Point: Projects a raw selection (from engine/keyboard)
  /// into a valid Storage Selection (avoiding hidden ranges).
  ///
  /// Movement direction helps disambiguate landing spots (Strategy 2.4).
  /// - forward: Snap to end
  /// - backward: Snap to start
  TextSelection projectSelection(
    TextSelection raw, {
    required TextSelection previousSelection,
    bool isDirectional = false,
  }) {
    if (!hasHiddenRanges) return raw;

    // 1. If selection is collapsed (Caret)
    if (raw.isCollapsed) {
      final offset = raw.baseOffset;
      // Check for intersection
      for (final range in model.hiddenRanges) {
        if (_isInside(range, offset)) {
          // Degenerate!
          // Determine direction or use nearest boundary on taps/static.
          final prev = previousSelection.extentOffset;
          final bool staticTap = prev == offset;
          if (staticTap) {
            // Static tap at a hidden marker. For code fences (len == 3) we
            // snap forward to the end so Enter inserts after the fence. For
            // other markers, keep nearest-boundary behavior to avoid
            // unexpected jumps in inline styles.
            final len = range.end - range.start;
            if (len == 3) {
              return TextSelection.fromPosition(
                TextPosition(offset: range.end, affinity: raw.affinity),
              );
            }
            final distStart = (offset - range.start).abs();
            final distEnd = (offset - range.end).abs();
            final newOffset = distStart <= distEnd ? range.start : range.end;
            return TextSelection.fromPosition(
              TextPosition(offset: newOffset, affinity: raw.affinity),
            );
          }

          final movingBackward = prev > offset;

          // heuristic: Bias Forward for Forward moves, Backward for Backward.
          final newOffset = movingBackward ? range.start : range.end;

          return TextSelection.fromPosition(
            TextPosition(offset: newOffset, affinity: raw.affinity),
          );
        }
      }
      return raw;
    }

    // 2. If selection is Range (Drag)
    // Strategy: Preserve anchor, project extent.
    // Spec 2.4.4: "Preserve the anchor; project only the moving endpoint (extent)."
    // Wait, TextSelection doesn't track "moving endpoint" explicitly, but base/extent imply it.
    // If base == previous.base, then extent is moving.

    // For now, let's project both ends to be safe boundaries.
    final base = _snapToBoundary(raw.baseOffset);
    final extent = _snapToBoundary(raw.extentOffset);

    return raw.copyWith(baseOffset: base, extentOffset: extent);
  }

  int _snapToBoundary(int offset) {
    for (final range in model.hiddenRanges) {
      if (_isInside(range, offset)) {
        // Tie-break: Closest boundary?
        final distStart = (offset - range.start).abs();
        final distEnd = (offset - range.end).abs();
        return distStart < distEnd ? range.start : range.end;
      }
    }
    return offset;
  }

  bool _isInside(TextRange range, int offset) {
    // Caret offsets are *between* characters.
    // Treat the boundaries as valid landing positions; only interior offsets
    // (between hidden characters) are considered degenerate.
    return offset > range.start && offset < range.end;
  }
}

import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';

class SelectionMaskUtils {
  const SelectionMaskUtils._();

  static TextSelection clampSelectionToText(
    TextSelection selection,
    int textLength,
  ) {
    if (!selection.isValid || selection.start < 0 || selection.end < 0) {
      return TextSelection.collapsed(offset: textLength);
    }
    final start = selection.start.clamp(0, textLength).toInt();
    final end = selection.end.clamp(0, textLength).toInt();
    return TextSelection(
      baseOffset: start,
      extentOffset: end,
      affinity: selection.affinity,
      isDirectional: selection.isDirectional,
    );
  }

  static TextSelection snapSelectionWithCursorMask(
    TextSelection selection, {
    required int textLength,
    required CursorValidationMask mask,
  }) {
    final clamped = clampSelectionToText(selection, textLength);
    if (!clamped.isValid) return clamped;

    final snappedBase = mask.snapToSafeOffset(clamped.baseOffset);
    final snappedExtent = mask.snapToSafeOffset(clamped.extentOffset);
    if (snappedBase == clamped.baseOffset &&
        snappedExtent == clamped.extentOffset) {
      return clamped;
    }

    return TextSelection(
      baseOffset: snappedBase.clamp(0, textLength).toInt(),
      extentOffset: snappedExtent.clamp(0, textLength).toInt(),
      affinity: clamped.affinity,
      isDirectional: clamped.isDirectional,
    );
  }

  static CursorValidationMask normalizeCursorMaskToText(
    CursorValidationMask mask, {
    required int textLength,
    List<TextRange> fallbackHiddenRanges = const [],
  }) {
    if (mask is HiddenRangeCursorValidationMask) {
      if (mask.textLength == textLength) return mask;
      return HiddenRangeCursorValidationMask(
        textLength: textLength,
        hiddenRanges: _normalizeRanges(mask.hiddenRanges, textLength),
      );
    }

    if (mask is PassthroughCursorValidationMask) {
      if (mask.textLength == textLength) return mask;
      return PassthroughCursorValidationMask(textLength: textLength);
    }

    if (fallbackHiddenRanges.isNotEmpty) {
      return HiddenRangeCursorValidationMask(
        textLength: textLength,
        hiddenRanges: _normalizeRanges(fallbackHiddenRanges, textLength),
      );
    }

    return PassthroughCursorValidationMask(textLength: textLength);
  }

  static List<TextRange> _normalizeRanges(
    Iterable<TextRange> ranges,
    int textLength,
  ) {
    final sanitized = <TextRange>[];
    for (final range in ranges) {
      final start = range.start.clamp(0, textLength).toInt();
      final end = range.end.clamp(0, textLength).toInt();
      if (end <= start) continue;
      sanitized.add(TextRange(start: start, end: end));
    }
    sanitized.sort((a, b) {
      final byStart = a.start.compareTo(b.start);
      if (byStart != 0) return byStart;
      return a.end.compareTo(b.end);
    });

    final normalized = <TextRange>[];
    for (final range in sanitized) {
      if (normalized.isEmpty) {
        normalized.add(range);
        continue;
      }
      final last = normalized.last;
      if (range.start < last.end) continue;
      if (range.start == last.start && range.end == last.end) continue;
      normalized.add(range);
    }
    return normalized;
  }
}

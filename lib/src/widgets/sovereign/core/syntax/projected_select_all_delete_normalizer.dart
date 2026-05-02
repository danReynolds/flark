import 'package:flutter/services.dart';

import 'projection_range_utils.dart';

abstract final class ProjectedSelectAllDeleteNormalizer {
  static TextEditingValue normalize({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required List<TextRange> projectedHiddenRanges,
  }) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid || oldSel.isCollapsed) {
      return newValue;
    }
    if (!newSel.isCollapsed || newSel.baseOffset != 0) return newValue;
    if (oldSel.start != 0) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (oldText.isEmpty || oldSel.end <= 0 || oldSel.end >= oldText.length) {
      return newValue;
    }

    final expectedDelete = oldText.replaceRange(oldSel.start, oldSel.end, '');
    if (newText != expectedDelete) return newValue;

    final hidden = ProjectionRangeUtils.normalizeHiddenRanges(
      projectedHiddenRanges,
      oldText.length,
    );
    if (hidden.isEmpty) return newValue;

    final hiddenLength = hidden.fold<int>(
      0,
      (sum, range) => sum + range.end - range.start,
    );
    final projectedVisibleLength = oldText.length - hiddenLength;
    if (projectedVisibleLength <= 0 || oldSel.end != projectedVisibleLength) {
      return newValue;
    }

    return const TextEditingValue(
      text: '',
      selection: TextSelection.collapsed(offset: 0),
      composing: TextRange.empty,
    );
  }
}

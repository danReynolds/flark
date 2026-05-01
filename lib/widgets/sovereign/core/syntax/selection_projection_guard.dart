import 'package:flutter/services.dart';

import '../../engine/syntax_snapshot.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/projector.dart';

abstract class SelectionProjectionGuard {
  TextSelection projectAndSnap({
    required TextSelection requested,
    required TextSelection previous,
    required int textLength,
    required Projector projector,
    required CursorValidationMask mask,
  });
}

class DefaultSelectionProjectionGuard implements SelectionProjectionGuard {
  const DefaultSelectionProjectionGuard();

  @override
  TextSelection projectAndSnap({
    required TextSelection requested,
    required TextSelection previous,
    required int textLength,
    required Projector projector,
    required CursorValidationMask mask,
  }) {
    final projected = projector.projectSelection(
      requested,
      previousSelection: previous,
    );
    final clamped = _clampSelectionToText(projected, textLength);
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

  static TextSelection _clampSelectionToText(
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
}

import 'package:flutter/services.dart';

TextSelection safeSelectionForText(TextSelection selection, int textLength) {
  if (!selection.isValid || selection.start < 0 || selection.end < 0) {
    return TextSelection.collapsed(offset: textLength);
  }
  final start = selection.start.clamp(0, textLength);
  final end = selection.end.clamp(0, textLength);
  return TextSelection(
    baseOffset: start,
    extentOffset: end,
    affinity: selection.affinity,
    isDirectional: selection.isDirectional,
  );
}

TextSelection clampSelectionToText(TextSelection selection, int textLength) =>
    safeSelectionForText(selection, textLength);

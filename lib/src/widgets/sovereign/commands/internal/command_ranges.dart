import 'package:flutter/services.dart';

({int start, int end}) selectedLineRange(String text, TextSelection selection) {
  if (text.isEmpty) {
    return (start: 0, end: 0);
  }
  final safe = _safeSelection(selection, text.length);
  final start = lineStart(text, safe.start);
  final endAnchor =
      safe.isCollapsed ? safe.end : (safe.end - 1).clamp(0, text.length);
  final end = lineEnd(text, endAnchor);
  return (start: start, end: end);
}

int lineStart(String text, int offset) {
  if (text.isEmpty) return 0;
  final safe = offset.clamp(0, text.length);
  if (safe == 0) return 0;
  final prevBreak = text.lastIndexOf('\n', safe - 1);
  return prevBreak == -1 ? 0 : prevBreak + 1;
}

int lineEnd(String text, int offset) {
  if (text.isEmpty) return 0;
  final safe = offset.clamp(0, text.length);
  final nextBreak = text.indexOf('\n', safe);
  return nextBreak == -1 ? text.length : nextBreak;
}

TextSelection _safeSelection(TextSelection selection, int length) {
  if (!selection.isValid || selection.start < 0 || selection.end < 0) {
    return TextSelection.collapsed(offset: length);
  }
  final start = selection.start.clamp(0, length);
  final end = selection.end.clamp(0, length);
  return TextSelection(
    baseOffset: start,
    extentOffset: end,
    affinity: selection.affinity,
    isDirectional: selection.isDirectional,
  );
}

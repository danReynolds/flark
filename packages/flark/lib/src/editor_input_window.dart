import 'dart:math' as math;

import 'editor_text.dart';

/// One immutable bounded input window planned without a frontend dependency.
final class FlarkEditorInputWindow {
  const FlarkEditorInputWindow({
    required this.text,
    required this.globalUtf16Start,
    required this.selection,
    required this.activeOrdinal,
    required this.canonicalSelectionBaseUtf16,
    required this.canonicalSelectionExtentUtf16,
    required this.crossRowSelection,
    required this.selectionRepresented,
  });

  final String text;
  final int globalUtf16Start;
  final FlarkTextSelection selection;
  final int activeOrdinal;
  final int canonicalSelectionBaseUtf16;
  final int canonicalSelectionExtentUtf16;
  final bool crossRowSelection;
  final bool selectionRepresented;
}

/// Plans bounded UTF-16 windows shared by every host adapter.
///
/// The planner knows no platform text object and no Markdown meaning. It owns
/// only capacity, scalar-aligned cuts, and the exact relationship between a
/// local surrogate selection and its canonical global endpoints.
abstract final class FlarkEditorInputWindowPlanner {
  static FlarkEditorInputWindow activate({
    required String text,
    required int sourceStart,
    required int caret,
    int? selectionExtent,
    required int ordinal,
    required FlarkTextAffinity affinity,
    required int maximumCodeUnits,
  }) {
    _checkCapacity(maximumCodeUnits);
    final requestedLocalCaret = caret - sourceStart;
    final requestedLocalExtent = selectionExtent == null
        ? requestedLocalCaret
        : selectionExtent - sourceStart;
    final localCaret = requestedLocalCaret.clamp(0, text.length);
    final localExtent = selectionExtent == null
        ? localCaret
        : requestedLocalExtent.clamp(0, text.length);
    var windowStart = 0;
    var windowEnd = text.length;
    if (text.length > maximumCodeUnits) {
      windowStart = (localCaret - maximumCodeUnits ~/ 2).clamp(
        0,
        text.length - maximumCodeUnits,
      );
      final selectionStart = math.min(localCaret, localExtent);
      final selectionEnd = math.max(localCaret, localExtent);
      if (selectionEnd - selectionStart <= maximumCodeUnits) {
        if (selectionStart < windowStart) windowStart = selectionStart;
        if (selectionEnd > windowStart + maximumCodeUnits) {
          windowStart = selectionEnd - maximumCodeUnits;
        }
      }
      windowEnd = windowStart + maximumCodeUnits;
    }
    final alignedWindow = scalarAlignedUtf16Window(
      text,
      windowStart,
      windowEnd,
    );
    windowStart = alignedWindow.start;
    windowEnd = alignedWindow.end;
    final window = text.substring(windowStart, windowEnd);
    final windowCaret = localCaret - windowStart;
    final windowExtent = localExtent - windowStart;
    final selectionRepresented =
        requestedLocalCaret == localCaret &&
        requestedLocalExtent == localExtent &&
        windowCaret >= 0 &&
        windowCaret <= window.length &&
        windowExtent >= 0 &&
        windowExtent <= window.length;
    final globalStart = sourceStart + windowStart;
    final selection = selectionExtent != null && selectionRepresented
        ? FlarkTextSelection(
            baseOffset: windowCaret,
            extentOffset: windowExtent,
            affinity: affinity,
            isDirectional: true,
          )
        : FlarkTextSelection.collapsed(
            offset: windowCaret.clamp(0, window.length),
            affinity: affinity,
          );
    return FlarkEditorInputWindow(
      text: window,
      globalUtf16Start: globalStart,
      selection: selection,
      activeOrdinal: ordinal,
      canonicalSelectionBaseUtf16:
          selectionExtent != null && !selectionRepresented
          ? caret
          : globalStart + selection.baseOffset,
      canonicalSelectionExtentUtf16:
          selectionExtent != null && !selectionRepresented
          ? selectionExtent
          : globalStart + selection.extentOffset,
      crossRowSelection:
          selectionExtent != null &&
          selectionRepresented &&
          caret != selectionExtent,
      selectionRepresented: selectionRepresented,
    );
  }

  static FlarkEditorInputWindow collapsed({
    required String text,
    required int sourceStart,
    required int caret,
    required int ordinal,
    required int maximumCodeUnits,
  }) {
    _checkCapacity(maximumCodeUnits);
    final localCaret = (caret - sourceStart).clamp(0, text.length);
    final candidateStart = text.length <= maximumCodeUnits
        ? 0
        : (localCaret - maximumCodeUnits ~/ 2).clamp(
            0,
            text.length - maximumCodeUnits,
          );
    final candidateEnd = math.min(
      text.length,
      candidateStart + maximumCodeUnits,
    );
    final window = scalarAlignedUtf16Window(text, candidateStart, candidateEnd);
    final globalStart = sourceStart + window.start;
    final selection = FlarkTextSelection.collapsed(
      offset: localCaret - window.start,
    );
    return FlarkEditorInputWindow(
      text: text.substring(window.start, window.end),
      globalUtf16Start: globalStart,
      selection: selection,
      activeOrdinal: ordinal,
      canonicalSelectionBaseUtf16: globalStart + selection.baseOffset,
      canonicalSelectionExtentUtf16: globalStart + selection.extentOffset,
      crossRowSelection: false,
      selectionRepresented: true,
    );
  }

  static void _checkCapacity(int maximumCodeUnits) {
    if (maximumCodeUnits <= 0) {
      throw ArgumentError.value(
        maximumCodeUnits,
        'maximumCodeUnits',
        'must be positive',
      );
    }
  }
}

import 'dart:math' as math;

import 'editor_text.dart';
import 'editor_viewport_state.dart';
import 'models.dart';
import 'pending_presentation.dart';
import 'presentation.dart';
import 'surface_projector.dart';

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
  final int? activeOrdinal;
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
  /// Applies one authoritative source splice to an existing bounded input
  /// window while preserving its exact global origin and collapsed result
  /// caret. A null result means the host must rebuild from source geometry.
  static FlarkEditorInputWindow? afterCommittedSplice({
    required FlarkEditorInputValue base,
    required int inputGlobalUtf16Start,
    required int? activeOrdinal,
    required int startUtf16,
    required int endUtf16,
    required String replacement,
    required int resultCaretUtf16,
    required int maximumCodeUnits,
  }) {
    _checkCapacity(maximumCodeUnits);
    if (inputGlobalUtf16Start < 0 ||
        startUtf16 < 0 ||
        endUtf16 < startUtf16 ||
        resultCaretUtf16 < 0) {
      throw ArgumentError('Committed input-window splice must be valid');
    }
    if (base.text.length > maximumCodeUnits) return null;
    final windowEnd = inputGlobalUtf16Start + base.text.length;
    final delta = replacement.length - (endUtf16 - startUtf16);
    if (endUtf16 <= inputGlobalUtf16Start || startUtf16 >= windowEnd) {
      final resultWindowStart = endUtf16 <= inputGlobalUtf16Start
          ? inputGlobalUtf16Start + delta
          : inputGlobalUtf16Start;
      final localCaret = resultCaretUtf16 - resultWindowStart;
      if (localCaret < 0 || localCaret > base.text.length) return null;
      return FlarkEditorInputWindow(
        text: base.text,
        globalUtf16Start: resultWindowStart,
        selection: FlarkTextSelection.collapsed(offset: localCaret),
        activeOrdinal: activeOrdinal,
        canonicalSelectionBaseUtf16: resultCaretUtf16,
        canonicalSelectionExtentUtf16: resultCaretUtf16,
        crossRowSelection: false,
        selectionRepresented: true,
      );
    }
    final localStart = startUtf16 - inputGlobalUtf16Start;
    final localEnd = endUtf16 - inputGlobalUtf16Start;
    if (localStart < 0 ||
        localEnd < localStart ||
        localEnd > base.text.length) {
      return null;
    }
    final text = base.text.replaceRange(localStart, localEnd, replacement);
    final localCaret = resultCaretUtf16 - inputGlobalUtf16Start;
    if (text.length > maximumCodeUnits ||
        localCaret < 0 ||
        localCaret > text.length) {
      return null;
    }
    return FlarkEditorInputWindow(
      text: text,
      globalUtf16Start: inputGlobalUtf16Start,
      selection: FlarkTextSelection.collapsed(offset: localCaret),
      activeOrdinal: activeOrdinal,
      canonicalSelectionBaseUtf16: resultCaretUtf16,
      canonicalSelectionExtentUtf16: resultCaretUtf16,
      crossRowSelection: false,
      selectionRepresented: true,
    );
  }

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

  /// Restores a collapsed caret from parser/pending surface geometry.
  static FlarkEditorInputWindow restoreCollapsed({
    required FlarkEditorViewportState viewportState,
    required FlarkSurfaceProjector projector,
    required FlarkPendingPresentationSnapshot pendingPresentation,
    required int caret,
    required int sourceUtf16Length,
    required int maximumCodeUnits,
    int? preferredOrdinal,
  }) {
    final gap = paragraphGap(
      viewportState: viewportState,
      projector: projector,
      gap: pendingPresentation.paragraphGap,
      caret: caret,
      maximumCodeUnits: maximumCodeUnits,
    );
    if (gap != null) return gap;
    final boundary = caretBoundary(
      viewportState: viewportState,
      boundary: pendingPresentation.caretBoundary,
      caret: caret,
      maximumCodeUnits: maximumCodeUnits,
    );
    if (boundary != null) return boundary;

    FlarkViewportRow? row;
    if (preferredOrdinal != null) {
      for (final candidate in viewportState.rows) {
        final range = viewportState.mapRange(
          projector.activationRange(candidate),
        );
        if (candidate.ordinal == preferredOrdinal &&
            range.start <= caret &&
            caret <= range.end) {
          row = candidate;
          break;
        }
      }
    }
    final ordinalAtCaret = projector.surfaceOrdinalAt(
      rows: viewportState.rows,
      globalUtf16Offset: caret,
      sourceUtf16Length: sourceUtf16Length,
    );
    if (row == null && ordinalAtCaret != null) {
      for (final candidate in viewportState.rows) {
        if (candidate.ordinal == ordinalAtCaret) {
          row = candidate;
          break;
        }
      }
    }
    if (row != null) {
      final range = viewportState.mapRange(projector.activationRange(row));
      if (range.start >= viewportState.visibleUtf16Start &&
          range.end <= viewportState.visibleUtf16End &&
          range.start <= caret &&
          caret <= range.end) {
        return collapsed(
          text: viewportState.sliceVisibleUtf16(range.start, range.end),
          sourceStart: range.start,
          caret: caret,
          ordinal: row.ordinal,
          maximumCodeUnits: maximumCodeUnits,
        );
      }
    }
    return _physicalLine(
      viewportState: viewportState,
      caret: caret,
      ordinal: ordinalAtCaret ?? -1,
      maximumCodeUnits: maximumCodeUnits,
    );
  }

  static FlarkEditorInputWindow? paragraphGap({
    required FlarkEditorViewportState viewportState,
    required FlarkSurfaceProjector projector,
    required FlarkCoreCommittedPresentationGapV1? gap,
    required int caret,
    required int maximumCodeUnits,
  }) {
    if (gap == null) return null;
    final end = _committedGapEnd(viewportState, projector, gap);
    if (caret < gap.rowEndUtf16 || caret > end) return null;
    if (gap.rowEndUtf16 < viewportState.visibleUtf16Start ||
        end > viewportState.visibleUtf16End) {
      return null;
    }
    return collapsed(
      text: viewportState.sliceVisibleUtf16(gap.rowEndUtf16, end),
      sourceStart: gap.rowEndUtf16,
      caret: caret,
      ordinal: -gap.rowOrdinal - 1,
      maximumCodeUnits: maximumCodeUnits,
    );
  }

  static FlarkEditorInputWindow? caretBoundary({
    required FlarkEditorViewportState viewportState,
    required FlarkPendingCaretBoundary? boundary,
    required int caret,
    required int maximumCodeUnits,
  }) {
    if (boundary == null) return null;
    final end = _caretBoundaryInputEnd(viewportState, boundary);
    if (end == null || caret < boundary.rowEndUtf16 || caret > end) {
      return null;
    }
    var inputStart = boundary.rowEndUtf16;
    var inputEnd = end;
    if (caret == end && end < viewportState.visibleUtf16End) {
      // The shared physical-line edge belongs to the downstream line.
      inputStart = end;
      final localStart = inputStart - viewportState.visibleUtf16Start;
      final newline = viewportState.visibleSource.indexOf('\n', localStart);
      inputEnd = newline == -1
          ? viewportState.visibleUtf16End
          : viewportState.visibleUtf16Start + newline + 1;
    }
    return collapsed(
      text: viewportState.sliceVisibleUtf16(inputStart, inputEnd),
      sourceStart: inputStart,
      caret: caret,
      ordinal: -boundary.rowOrdinal - 1,
      maximumCodeUnits: maximumCodeUnits,
    );
  }

  static FlarkEditorInputWindow neutralLine({
    required FlarkEditorViewportState viewportState,
    required int caret,
    required int maximumCodeUnits,
  }) {
    if (viewportState.visibleSource.isEmpty) {
      return collapsed(
        text: '',
        sourceStart: viewportState.visibleUtf16Start,
        caret: viewportState.visibleUtf16Start,
        ordinal: -1,
        maximumCodeUnits: maximumCodeUnits,
      );
    }
    final localCaret = (caret - viewportState.visibleUtf16Start).clamp(
      0,
      viewportState.visibleSource.length,
    );
    final lineStart = localCaret == 0
        ? 0
        : viewportState.visibleSource.lastIndexOf('\n', localCaret - 1) + 1;
    var lineOrdinal = 0;
    for (var index = 0; index < lineStart; index += 1) {
      if (viewportState.visibleSource.codeUnitAt(index) == 0x0a) {
        lineOrdinal += 1;
      }
    }
    return _physicalLine(
      viewportState: viewportState,
      caret: caret,
      ordinal: -lineOrdinal - 1,
      maximumCodeUnits: maximumCodeUnits,
    );
  }

  static FlarkEditorInputWindow _physicalLine({
    required FlarkEditorViewportState viewportState,
    required int caret,
    required int ordinal,
    required int maximumCodeUnits,
  }) {
    final localCaret = (caret - viewportState.visibleUtf16Start).clamp(
      0,
      viewportState.visibleSource.length,
    );
    final lineStart = localCaret == 0
        ? 0
        : viewportState.visibleSource.lastIndexOf('\n', localCaret - 1) + 1;
    final newline = viewportState.visibleSource.indexOf('\n', localCaret);
    final lineEnd = newline == -1
        ? viewportState.visibleSource.length
        : newline + 1;
    return collapsed(
      text: viewportState.visibleSource.substring(lineStart, lineEnd),
      sourceStart: viewportState.visibleUtf16Start + lineStart,
      caret: caret,
      ordinal: ordinal,
      maximumCodeUnits: maximumCodeUnits,
    );
  }

  static int _committedGapEnd(
    FlarkEditorViewportState viewportState,
    FlarkSurfaceProjector projector,
    FlarkCoreCommittedPresentationGapV1 gap,
  ) {
    var end = viewportState.visibleUtf16End;
    final localStart = gap.rowEndUtf16 - viewportState.visibleUtf16Start;
    if (0 <= localStart && localStart < viewportState.visibleSource.length) {
      final newline = viewportState.visibleSource.indexOf('\n', localStart);
      if (newline >= 0) end = viewportState.visibleUtf16Start + newline + 1;
    }
    for (final row in viewportState.rows) {
      if (row.ordinal == gap.rowOrdinal) continue;
      final start = projector.surfaceSourceRange(row).start;
      if (start > gap.rowEndUtf16) end = math.min(end, start);
    }
    return end;
  }

  static int? _caretBoundaryInputEnd(
    FlarkEditorViewportState viewportState,
    FlarkPendingCaretBoundary boundary,
  ) {
    final localStart = boundary.rowEndUtf16 - viewportState.visibleUtf16Start;
    if (localStart < 0 || localStart > viewportState.visibleSource.length) {
      return null;
    }
    final newline = viewportState.visibleSource.indexOf('\n', localStart);
    return newline == -1
        ? viewportState.visibleUtf16End
        : viewportState.visibleUtf16Start + newline + 1;
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

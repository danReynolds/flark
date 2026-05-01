part of 'sovereign_text_renderer.dart';

class _SovereignTextRendererMarkers {
  const _SovereignTextRendererMarkers._();

  static bool isBlockquoteMarkerRange(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    if (end - start != 2) return false;
    if (start < 0 || end > text.length) return false;
    if (!text.startsWith('> ', start)) return false;
    if (start == 0) return true;
    return text.codeUnitAt(start - 1) == 10;
  }

  static bool isUnorderedListMarkerRange(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    if (start < 0 || end > text.length || start >= end) return false;

    final lineStart = _SovereignRendererUtils.lineStartForOffset(text, start);
    final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
    final lineEnd =
        (lineEndWithBreak > 0 && text.codeUnitAt(lineEndWithBreak - 1) == 10)
            ? lineEndWithBreak - 1
            : lineEndWithBreak;
    if (lineEnd <= lineStart) return false;

    final marker = _SovereignRendererUtils.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    return marker != null &&
        !marker.isOrdered &&
        start == marker.markerStart &&
        end == marker.markerEnd;
  }

  static bool isMarkdownImageOpenerMarkerRange(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    if (end - start != 2) return false;
    if (start < 0 || end > text.length) return false;
    return text.startsWith('![', start);
  }

  static bool isOrderedListMarkerRange(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    if (start < 0 || end > text.length || start >= end) return false;

    final lineStart = _SovereignRendererUtils.lineStartForOffset(text, start);
    final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
    final lineEnd =
        (lineEndWithBreak > 0 && text.codeUnitAt(lineEndWithBreak - 1) == 10)
            ? lineEndWithBreak - 1
            : lineEndWithBreak;
    if (lineEnd <= lineStart) return false;

    final marker = _SovereignRendererUtils.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    return marker != null &&
        marker.isOrdered &&
        start == marker.markerStart &&
        end == marker.markerEnd;
  }

  static bool isMarkdownLinkOrImageTailMarkerRange(
    String text,
    TextRange range,
  ) {
    final start = range.start;
    final end = range.end;
    if (start < 0 || end > text.length || start >= end) return false;
    final hiddenText = text.substring(start, end);
    if (!hiddenText.startsWith('](') || !hiddenText.endsWith(')')) return false;
    // Hidden link/image tails are single-line marker segments.
    return !hiddenText.contains('\n') && !hiddenText.contains('\r');
  }

  static String? taskListMarkerVisualForRange(
    String text,
    TextRange range, {
    SovereignTaskCheckboxTheme? taskCheckboxTheme,
  }) {
    if (range.start < 0 ||
        range.end > text.length ||
        range.start >= range.end) {
      return null;
    }

    final lineStart = _SovereignRendererUtils.lineStartForOffset(
      text,
      range.start,
    );
    final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
    final lineEnd =
        (lineEndWithBreak > 0 && text.codeUnitAt(lineEndWithBreak - 1) == 10)
            ? lineEndWithBreak - 1
            : lineEndWithBreak;
    if (lineEnd <= lineStart) return null;

    final marker = _SovereignRendererUtils.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (marker == null) return null;
    final task = _SovereignRendererUtils.taskMarkerInfo(
      text,
      marker.markerEnd,
      lineEnd,
    );
    if (task == null) return null;

    final taskRange = TextRange(
      start: marker.markerEnd,
      end: task.contentStart,
    );
    final combinedRange = TextRange(
      start: marker.markerStart,
      end: task.contentStart,
    );
    final isTaskOnly =
        range.start == taskRange.start && range.end == taskRange.end;
    final isCombined =
        range.start == combinedRange.start && range.end == combinedRange.end;
    if (!isTaskOnly && !isCombined) return null;

    final stateCodeUnit = text.codeUnitAt(marker.markerEnd + 1);
    final checked = stateCodeUnit == 120 || stateCodeUnit == 88;
    final gapSpaces =
        (taskCheckboxTheme?.labelGapSpaces ?? 2).clamp(1, 4).toInt();
    // Keep marker width stable when using the custom overlay so the task text
    // does not shift horizontally when toggling checked/unchecked state.
    final useCustomOverlay = taskCheckboxTheme?.useCustomOverlay == true;
    final checkboxGlyph =
        useCustomOverlay ? '\u2610' : (checked ? '\u2611' : '\u2610');
    final checkboxVisible = '$checkboxGlyph${' ' * gapSpaces}';
    final prefix = (isCombined && marker.isOrdered)
        ? text.substring(marker.markerStart, marker.markerEnd)
        : '';
    return padVisualMarker(prefix + checkboxVisible, range.end - range.start);
  }

  static bool isUnorderedTaskListBulletMarkerRange(
    String text,
    TextRange range,
  ) {
    final lineStart = _SovereignRendererUtils.lineStartForOffset(
      text,
      range.start,
    );
    final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
    final lineEnd =
        (lineEndWithBreak > 0 && text.codeUnitAt(lineEndWithBreak - 1) == 10)
            ? lineEndWithBreak - 1
            : lineEndWithBreak;
    if (lineEnd <= lineStart) return false;

    final marker = _SovereignRendererUtils.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (marker == null || marker.isOrdered) return false;

    final markerRange = TextRange(
      start: marker.markerStart,
      end: marker.markerEnd,
    );
    if (range.start != markerRange.start || range.end != markerRange.end) {
      return false;
    }

    final task = _SovereignRendererUtils.taskMarkerInfo(
      text,
      marker.markerEnd,
      lineEnd,
    );
    return task != null;
  }

  static String padVisualMarker(String value, int targetLength) {
    if (value.length >= targetLength) {
      return value.substring(0, targetLength);
    }
    final out = StringBuffer(value);
    for (var i = value.length; i < targetLength; i++) {
      out.write('\u200B');
    }
    return out.toString();
  }

  static bool isThematicBreakMarkerRange(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    if (start < 0 || end > text.length || start >= end) return false;

    final lineStart = _SovereignRendererUtils.lineStartForOffset(text, start);
    final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
    final lineEnd =
        (lineEndWithBreak > 0 && text.codeUnitAt(lineEndWithBreak - 1) == 10)
            ? lineEndWithBreak - 1
            : lineEndWithBreak;
    if (lineEnd <= lineStart) return false;

    final inLine = MarkdownMarkerGrammar.matchThematicBreakMarkerRange(
      text.substring(lineStart, lineEnd),
      dialect: MarkdownMarkerDialect.commonMark,
    );
    if (inLine == null) return false;
    return start == lineStart + inLine.start && end == lineStart + inLine.end;
  }

  static String thematicBreakVisualMarker(String hiddenText) {
    if (hiddenText.isEmpty) return hiddenText;
    final out = StringBuffer();
    for (var i = 0; i < hiddenText.length; i++) {
      final cu = hiddenText.codeUnitAt(i);
      if (cu == 32 || cu == 9) {
        out.writeCharCode(cu);
      } else {
        out.write('\u2500');
      }
    }
    return out.toString();
  }

  static bool hiddenRangeHasBlockKind({
    required List<_BlockStyleRun> blockRuns,
    required int fromIndex,
    required TextRange range,
    required _BlockStyleKind kind,
  }) {
    var i = fromIndex;
    while (i < blockRuns.length && blockRuns[i].end <= range.start) {
      i++;
    }
    while (i < blockRuns.length && blockRuns[i].start < range.end) {
      final run = blockRuns[i];
      if (run.kind == kind &&
          run.start <= range.start &&
          run.end >= range.end) {
        return true;
      }
      i++;
    }
    return false;
  }

  static bool isStandaloneThematicBreakFallback(String text, TextRange range) {
    if (!isThematicBreakMarkerRange(text, range)) return false;

    final lineStart = _SovereignRendererUtils.lineStartForOffset(
      text,
      range.start,
    );
    if (lineStart == 0) return true;

    final prevLineEnd = lineStart - 1;
    final prevLineStart = prevLineEnd > 0
        ? _SovereignRendererUtils.lineStartForOffset(text, prevLineEnd)
        : 0;
    for (var i = prevLineStart; i < prevLineEnd; i++) {
      final cu = text.codeUnitAt(i);
      if (cu == 32 || cu == 9) continue;
      return false;
    }
    return true;
  }

  static String unorderedListVisualMarker(String hiddenText) {
    if (hiddenText.isEmpty) return hiddenText;
    if (hiddenText.length == 1) return '\u2022';
    return '\u2022${hiddenText.substring(1)}';
  }
}

import 'package:flutter/services.dart';

import '../../logic/sovereign_markdown_markers.dart';
import 'models/list_marker_context.dart';
import 'models/task_marker_info.dart';

class MarkdownLineHelpers {
  const MarkdownLineHelpers._();

  static int lineStartForOffset(String text, int offset) {
    return SovereignMarkdownMarkers.lineStartForOffset(text, offset);
  }

  static int headerMarkerLength(String text, int lineStart, int lineEnd) {
    var level = 0;
    while (lineStart + level < lineEnd &&
        text.codeUnitAt(lineStart + level) == 35 &&
        level < 6) {
      level++;
    }
    if (level == 0) return 0;
    if (lineStart + level >= lineEnd) return 0;
    if (text.codeUnitAt(lineStart + level) != 32) return 0;
    return level + 1;
  }

  static int blockquoteMarkerLength(String text, int lineStart, int lineEnd) {
    if (lineStart + 1 >= lineEnd) return 0;
    if (text.codeUnitAt(lineStart) == 62 &&
        text.codeUnitAt(lineStart + 1) == 32) {
      return 2;
    }
    return 0;
  }

  static int unorderedListMarkerLength(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    if (lineStart + 1 >= lineEnd) return 0;
    final cu = text.codeUnitAt(lineStart);
    if ((cu == 45 || cu == 42) && text.codeUnitAt(lineStart + 1) == 32) {
      return 2;
    }
    return 0;
  }

  static int orderedListMarkerLength(String text, int lineStart, int lineEnd) {
    var i = lineStart;
    while (i < lineEnd) {
      final cu = text.codeUnitAt(i);
      if (cu < 48 || cu > 57) break;
      i++;
    }
    if (i == lineStart) return 0;
    if (i + 1 >= lineEnd) return 0;
    if (text.codeUnitAt(i) != 46 || text.codeUnitAt(i + 1) != 32) return 0;
    return (i - lineStart) + 2;
  }

  static ListMarkerContext? listMarkerForLine(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    if (lineEnd <= lineStart) return null;

    final unorderedLen = unorderedListMarkerLength(text, lineStart, lineEnd);
    if (unorderedLen > 0) {
      var contentStart = lineStart + unorderedLen;
      var continueMarker = text.substring(lineStart, contentStart);
      final taskInfo = taskMarkerInfo(text, contentStart, lineEnd);
      if (taskInfo != null) {
        contentStart = taskInfo.contentStart;
        final bullet = continueMarker.trimRight();
        continueMarker = '$bullet [ ] ';
      }
      return ListMarkerContext(
        markerStart: lineStart,
        markerEnd: lineStart + unorderedLen,
        contentStart: contentStart,
        continueMarker: continueMarker,
        isOrdered: false,
      );
    }

    final orderedLen = orderedListMarkerLength(text, lineStart, lineEnd);
    if (orderedLen > 0) {
      var indexEnd = lineStart;
      while (indexEnd < lineEnd) {
        final cu = text.codeUnitAt(indexEnd);
        if (cu < 48 || cu > 57) break;
        indexEnd++;
      }
      final current = int.tryParse(text.substring(lineStart, indexEnd));
      final next = current == null ? 1 : current + 1;
      var contentStart = lineStart + orderedLen;
      var continueMarker = '$next. ';
      final taskInfo = taskMarkerInfo(text, contentStart, lineEnd);
      if (taskInfo != null) {
        contentStart = taskInfo.contentStart;
        continueMarker = '$next. [ ] ';
      }
      return ListMarkerContext(
        markerStart: lineStart,
        markerEnd: lineStart + orderedLen,
        contentStart: contentStart,
        continueMarker: continueMarker,
        isOrdered: true,
      );
    }

    return null;
  }

  static ListMarkerContext? listMarkerForLineAllowingQuotePrefix(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    final direct = listMarkerForLine(text, lineStart, lineEnd);
    if (direct != null) return direct;

    var cursor = lineStart;
    while (cursor < lineEnd) {
      if (text.codeUnitAt(cursor) != 62) break;
      cursor++;
      if (cursor < lineEnd && text.codeUnitAt(cursor) == 32) {
        cursor++;
      }
      while (cursor < lineEnd && text.codeUnitAt(cursor) == 32) {
        cursor++;
      }

      final nested = listMarkerForLine(text, cursor, lineEnd);
      if (nested != null) return nested;
    }

    return null;
  }

  static bool isLineBodyBlankFrom(String text, int start, int lineEnd) {
    final safeStart = start.clamp(0, text.length).toInt();
    final safeEnd = lineEnd.clamp(safeStart, text.length).toInt();
    for (var i = safeStart; i < safeEnd; i++) {
      final cu = text.codeUnitAt(i);
      if (cu == 32 || cu == 9) continue;
      return false;
    }
    return true;
  }

  static String? inlineMarkerToken(String text, TextRange range) {
    final start = range.start;
    final end = range.end;
    final len = end - start;
    if (start < 0 || end > text.length || len <= 0) return null;
    if (len == 2 && text.startsWith('**', start)) return '**';
    if (len != 1) return null;
    final cu = text.codeUnitAt(start);
    if (cu == 42 || cu == 95 || cu == 96) {
      return String.fromCharCode(cu);
    }
    return null;
  }

  static TextRange? markdownLinkOrImageTailRangeAt(String text, int start) {
    if (start < 0 || start + 3 > text.length) return null;
    if (!text.startsWith('](', start)) return null;

    var parenDepth = 1;
    var i = start + 2;
    while (i < text.length) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return null;
      if (cu == 40) {
        parenDepth++;
        i++;
        continue;
      }
      if (cu == 41) {
        parenDepth--;
        if (parenDepth > 0) {
          i++;
          continue;
        }
        return TextRange(start: start, end: i + 1);
      }
      i++;
    }
    return null;
  }

  static List<TextRange> selectionCenteredEmptyInlineRanges(
    TextEditingValue value,
  ) {
    final selection = value.selection;
    if (!selection.isValid || !selection.isCollapsed) return const [];

    final text = value.text;
    final caret = selection.baseOffset;
    if (caret < 0 || caret > text.length) return const [];

    bool hasTokenAt(int start, String token) {
      if (start < 0) return false;
      final end = start + token.length;
      if (end > text.length) return false;
      return text.startsWith(token, start);
    }

    final ranges = <TextRange>[];
    if (hasTokenAt(caret - 2, '**') && hasTokenAt(caret, '**')) {
      ranges.add(TextRange(start: caret - 2, end: caret));
      ranges.add(TextRange(start: caret, end: caret + 2));
    }
    if (hasTokenAt(caret - 1, '_') && hasTokenAt(caret, '_')) {
      ranges.add(TextRange(start: caret - 1, end: caret));
      ranges.add(TextRange(start: caret, end: caret + 1));
    }
    if (hasTokenAt(caret - 1, '`') && hasTokenAt(caret, '`')) {
      ranges.add(TextRange(start: caret - 1, end: caret));
      ranges.add(TextRange(start: caret, end: caret + 1));
    }

    return ranges;
  }

  static TaskMarkerInfo? taskMarkerInfo(String text, int start, int lineEnd) {
    if (start + 3 >= lineEnd) return null;
    if (text.codeUnitAt(start) != 91) return null;
    final state = text.codeUnitAt(start + 1);
    if (text.codeUnitAt(start + 2) != 93) return null;
    if (text.codeUnitAt(start + 3) != 32) return null;

    final isChecked = state == 120 || state == 88;
    final isUnchecked = state == 32;
    if (!isChecked && !isUnchecked) return null;

    return TaskMarkerInfo(isChecked: isChecked, contentStart: start + 4);
  }
}

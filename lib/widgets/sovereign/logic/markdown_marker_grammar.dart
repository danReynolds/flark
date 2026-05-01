import 'package:flutter/services.dart';

enum MarkdownMarkerDialect { sovereignV1, commonMark }

class MarkdownAtxHeadingMatch {
  final int level;
  final int markerStartIndex;

  const MarkdownAtxHeadingMatch({
    required this.level,
    required this.markerStartIndex,
  });
}

class MarkdownListMarkerMatch {
  final bool ordered;
  final int indentColumns;
  final int markerStartIndex;
  final int markerEndIndex;

  const MarkdownListMarkerMatch({
    required this.ordered,
    required this.indentColumns,
    required this.markerStartIndex,
    required this.markerEndIndex,
  });
}

abstract final class MarkdownMarkerGrammar {
  static (int columns, int index) leadingIndent(String line) {
    var columns = 0;
    var index = 0;
    while (index < line.length) {
      final cu = line.codeUnitAt(index);
      if (cu == 32) {
        columns++;
        index++;
        continue;
      }
      if (cu == 9) {
        columns += 4 - (columns % 4);
        index++;
        continue;
      }
      break;
    }
    return (columns, index);
  }

  static MarkdownAtxHeadingMatch? matchAtxHeading(
    String line, {
    required MarkdownMarkerDialect dialect,
  }) {
    final (columns, index) = leadingIndent(line);
    if (dialect == MarkdownMarkerDialect.commonMark) {
      if (columns > 3 || index >= line.length) return null;
      var level = 0;
      var i = index;
      while (i < line.length && line.codeUnitAt(i) == 35 && level < 6) {
        level++;
        i++;
      }
      if (level == 0) return null;
      if (i < line.length &&
          line.codeUnitAt(i) != 32 &&
          line.codeUnitAt(i) != 9) {
        return null;
      }
      return MarkdownAtxHeadingMatch(level: level, markerStartIndex: index);
    }

    if (index != 0 || index >= line.length) return null;
    var level = 0;
    while (index + level < line.length &&
        line.codeUnitAt(index + level) == 35 &&
        level < 6) {
      level++;
    }
    if (level == 0) return null;
    if (index + level >= line.length) return null;
    if (line.codeUnitAt(index + level) != 32) return null;
    return MarkdownAtxHeadingMatch(level: level, markerStartIndex: 0);
  }

  static bool isSetextUnderline(
    String line, {
    required MarkdownMarkerDialect dialect,
  }) {
    if (dialect != MarkdownMarkerDialect.commonMark) return false;
    final (columns, index) = leadingIndent(line);
    if (columns > 3 || index >= line.length) return false;
    final marker = line.codeUnitAt(index);
    if (marker != 45 && marker != 61) return false;
    for (var i = index; i < line.length; i++) {
      final cu = line.codeUnitAt(i);
      if (cu == 32 || cu == 9) continue;
      if (cu != marker) return false;
    }
    return true;
  }

  static int? matchBlockquoteMarkerEnd(
    String line, {
    required MarkdownMarkerDialect dialect,
  }) {
    final (columns, index) = leadingIndent(line);
    if (dialect == MarkdownMarkerDialect.commonMark) {
      if (columns > 3 || index >= line.length || line.codeUnitAt(index) != 62) {
        return null;
      }
      var end = index + 1;
      if (end < line.length) {
        final next = line.codeUnitAt(end);
        if (next == 32 || next == 9) end++;
      }
      return end;
    }

    if (index != 0 || index + 1 >= line.length) return null;
    if (line.codeUnitAt(index) == 62 && line.codeUnitAt(index + 1) == 32) {
      return index + 2;
    }
    return null;
  }

  static MarkdownListMarkerMatch? matchListMarker(
    String line, {
    required MarkdownMarkerDialect dialect,
  }) {
    final (indentColumns, markerStart) = leadingIndent(line);
    final maxIndent = dialect == MarkdownMarkerDialect.commonMark ? 3 : 0;
    if (indentColumns > maxIndent || markerStart >= line.length) return null;

    bool isWhitespace(int cu) {
      if (dialect == MarkdownMarkerDialect.commonMark) {
        return cu == 32 || cu == 9;
      }
      return cu == 32;
    }

    final first = line.codeUnitAt(markerStart);
    final allowPlus = dialect == MarkdownMarkerDialect.commonMark;
    if (first == 45 || first == 42 || (allowPlus && first == 43)) {
      final markerEnd = markerStart + 1;
      if (markerEnd >= line.length) return null;
      final next = line.codeUnitAt(markerEnd);
      if (!isWhitespace(next)) return null;
      return MarkdownListMarkerMatch(
        ordered: false,
        indentColumns: indentColumns,
        markerStartIndex: markerStart,
        markerEndIndex: markerEnd + 1,
      );
    }

    var i = markerStart;
    var digits = 0;
    final maxDigits =
        dialect == MarkdownMarkerDialect.commonMark ? 9 : line.length;
    while (i < line.length &&
        line.codeUnitAt(i) >= 48 &&
        line.codeUnitAt(i) <= 57 &&
        digits < maxDigits) {
      i++;
      digits++;
    }
    if (digits == 0 || i >= line.length) return null;

    final punct = line.codeUnitAt(i);
    final validPunct = dialect == MarkdownMarkerDialect.commonMark
        ? (punct == 46 || punct == 41)
        : punct == 46;
    if (!validPunct) return null;
    i++;

    if (i >= line.length) return null;
    final spacer = line.codeUnitAt(i);
    if (!isWhitespace(spacer)) return null;

    return MarkdownListMarkerMatch(
      ordered: true,
      indentColumns: indentColumns,
      markerStartIndex: markerStart,
      markerEndIndex: i + 1,
    );
  }

  static TextRange? matchTaskCheckboxRange(String line, int markerEnd) {
    var i = markerEnd;
    while (i < line.length) {
      final cu = line.codeUnitAt(i);
      if (cu != 32 && cu != 9) break;
      i++;
    }
    if (i + 2 >= line.length || line.codeUnitAt(i) != 91) return null;
    final state = line.codeUnitAt(i + 1);
    if (!(state == 32 || state == 120 || state == 88)) return null;
    if (line.codeUnitAt(i + 2) != 93) return null;
    var end = i + 3;
    if (end < line.length) {
      final spacer = line.codeUnitAt(end);
      if (spacer == 32 || spacer == 9) end++;
    }
    return TextRange(start: i, end: end);
  }

  static TextRange? listMarkerRangeInLineAllowingQuotePrefix(
    String line, {
    required int from,
    required MarkdownMarkerDialect dialect,
  }) {
    if (from < 0 || from >= line.length) return null;

    TextRange? directAt(int start) {
      if (start < 0 || start >= line.length) return null;
      final marker = matchListMarker(line.substring(start), dialect: dialect);
      if (marker == null) return null;
      return TextRange(
        start: start + marker.markerStartIndex,
        end: start + marker.markerEndIndex,
      );
    }

    final direct = directAt(from);
    if (direct != null) return direct;

    var cursor = from;
    while (cursor < line.length) {
      if (line.codeUnitAt(cursor) != 62) break;
      cursor++;
      if (cursor < line.length && line.codeUnitAt(cursor) == 32) {
        cursor++;
      }
      while (cursor < line.length && line.codeUnitAt(cursor) == 32) {
        cursor++;
      }
      final nested = directAt(cursor);
      if (nested != null) return nested;
    }

    return null;
  }

  static TextRange? matchThematicBreakMarkerRange(
    String line, {
    required MarkdownMarkerDialect dialect,
  }) {
    final (indentColumns, markerStart) = leadingIndent(line);
    if (dialect == MarkdownMarkerDialect.commonMark && indentColumns > 3) {
      return null;
    }
    if (markerStart >= line.length) return null;

    final marker = line.codeUnitAt(markerStart);
    if (marker != 45 && marker != 42 && marker != 95) {
      return null; // -, *, _
    }

    var markerCount = 0;
    for (var i = markerStart; i < line.length; i++) {
      final cu = line.codeUnitAt(i);
      if (cu == 32 || cu == 9) continue;
      if (cu != marker) return null;
      markerCount++;
    }
    if (markerCount < 3) return null;

    return TextRange(start: markerStart, end: line.length);
  }

  static TextRange? matchReferenceDefinitionMarkerRange(
    String line, {
    required MarkdownMarkerDialect dialect,
  }) {
    final (indentColumns, markerStart) = leadingIndent(line);
    if (dialect == MarkdownMarkerDialect.commonMark && indentColumns > 3) {
      return null;
    }
    if (markerStart >= line.length || line.codeUnitAt(markerStart) != 91) {
      return null; // [
    }

    var i = markerStart + 1;
    var labelHasContent = false;
    while (i < line.length) {
      final cu = line.codeUnitAt(i);
      if (cu == 92) {
        i += 2; // escaped char
        labelHasContent = true;
        continue;
      }
      if (cu == 93) break; // ]
      if (cu == 10 || cu == 13) return null;
      labelHasContent = true;
      i++;
    }
    if (!labelHasContent || i >= line.length || line.codeUnitAt(i) != 93) {
      return null;
    }
    if (i + 1 >= line.length || line.codeUnitAt(i + 1) != 58) {
      return null; // :
    }

    var markerEnd = i + 2; // include ]:
    while (markerEnd < line.length) {
      final cu = line.codeUnitAt(markerEnd);
      if (cu != 32 && cu != 9) break;
      markerEnd++;
    }
    if (markerEnd >= line.length) return null; // require destination content

    return TextRange(start: markerStart, end: markerEnd);
  }
}

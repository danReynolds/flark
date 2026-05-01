import '../../../models/line_index.dart';

class NavigationLineUtils {
  const NavigationLineUtils._();

  static int lineEndWithBreak(LineIndex lineIndex, String text, int line) {
    if (line + 1 < lineIndex.lineCount) {
      return lineIndex.offsetAtLine(line + 1);
    }
    return text.length;
  }

  static int lineContentEnd(String text, int lineStart, int lineEndWithBreak) {
    if (lineEndWithBreak > lineStart &&
        text.codeUnitAt(lineEndWithBreak - 1) == 10) {
      return lineEndWithBreak - 1;
    }
    return lineEndWithBreak;
  }

  static int columnAlignedOffsetForLineOrBoundary({
    required String text,
    required LineIndex lineIndex,
    required int line,
    required int column,
    required bool afterDocument,
  }) {
    if (lineIndex.lineCount <= 0) return 0;
    if (line < 0) return 0;
    if (line >= lineIndex.lineCount) {
      return afterDocument ? text.length : 0;
    }

    final lineStart = lineIndex.offsetAtLine(line);
    final endWithBreak = NavigationLineUtils.lineEndWithBreak(
      lineIndex,
      text,
      line,
    );
    final lineEnd = lineContentEnd(text, lineStart, endWithBreak);
    final lineColumnMax = (lineEnd - lineStart).clamp(0, text.length);
    return lineStart + column.clamp(0, lineColumnMax);
  }

  static bool isWhitespaceLine(String text, int start, int end) {
    for (var i = start; i < end; i++) {
      final ch = text.codeUnitAt(i);
      if (ch == 10) continue;
      if (ch == 32 || ch == 9) continue;
      return false;
    }
    return true;
  }

  static String leadingWhitespacePrefix(String input) {
    var i = 0;
    while (i < input.length) {
      final ch = input.codeUnitAt(i);
      if (ch != 32 && ch != 9) break;
      i++;
    }
    return i == 0 ? '' : input.substring(0, i);
  }

  static bool isHorizontalWhitespaceOnly(String input) {
    for (var i = 0; i < input.length; i++) {
      final ch = input.codeUnitAt(i);
      if (ch != 32 && ch != 9) return false;
    }
    return true;
  }

  static String trimRightHorizontalWhitespace(String input) {
    var end = input.length;
    while (end > 0) {
      final ch = input.codeUnitAt(end - 1);
      if (ch != 32 && ch != 9) break;
      end--;
    }
    return end == input.length ? input : input.substring(0, end);
  }
}

import '../../core/core.dart';

final class FlarkMarkdownFenceLine {
  const FlarkMarkdownFenceLine({
    required this.indent,
    required this.marker,
    required this.markerLength,
    required this.infoString,
    required this.language,
    required this.canClose,
  });

  final String indent;
  final String marker;
  final int markerLength;
  final String? infoString;
  final String? language;
  final bool canClose;

  bool closes(FlarkMarkdownFencedCodeContext open) {
    return canClose &&
        marker == open.marker &&
        markerLength >= open.markerLength;
  }
}

final class FlarkMarkdownFencedCodeContext {
  const FlarkMarkdownFencedCodeContext({
    required this.openingLineStart,
    required this.openingLineEndWithBreak,
    required this.bodyStart,
    required this.openingIndent,
    required this.marker,
    required this.markerLength,
    required this.infoString,
    required this.language,
    required this.closingLineStart,
    required this.closingLineEnd,
    required this.closingLineEndWithBreak,
  });

  final int openingLineStart;
  final int openingLineEndWithBreak;
  final int bodyStart;
  final String openingIndent;
  final String marker;
  final int markerLength;
  final String? infoString;
  final String? language;
  final int? closingLineStart;
  final int? closingLineEnd;
  final int? closingLineEndWithBreak;

  bool get isClosed => closingLineStart != null;

  int bodyEnd(String markdown) => closingLineStart ?? markdown.length;

  FlarkSourceRange bodyContentRange(String markdown) {
    var end = bodyEnd(markdown);
    if (isClosed &&
        _bodyHasContentBeforeFinalLineBreak(markdown, bodyStart, end)) {
      if (end > bodyStart && markdown.codeUnitAt(end - 1) == 0x0A) {
        end--;
      }
      if (end > bodyStart && markdown.codeUnitAt(end - 1) == 0x0D) {
        end--;
      }
    }
    return FlarkSourceRange(bodyStart, end).validate(markdown.length);
  }
}

abstract final class FlarkMarkdownFencedCodeScanner {
  static FlarkMarkdownFencedCodeContext? contextAt(
    String markdown,
    int rawCaret,
  ) {
    if (markdown.isEmpty) return null;
    final caret = rawCaret.clamp(0, markdown.length);
    final caretLineStart = lineStartForOffset(markdown, caret);
    FlarkMarkdownFencedCodeContext? active;
    var lineStart = 0;

    while (lineStart <= caretLineStart && lineStart <= markdown.length) {
      final lineEnd = lineContentEnd(markdown, lineStart);
      final lineText = markdown.substring(lineStart, lineEnd);
      final fence = fenceLine(lineText);
      if (fence != null) {
        if (active == null) {
          active = _openContext(
            markdown: markdown,
            lineStart: lineStart,
            fence: fence,
          );
          if (lineStart == caretLineStart) return null;
        } else if (fence.closes(active)) {
          if (lineStart == caretLineStart) return null;
          active = null;
        }
      }

      if (lineStart == caretLineStart) break;
      final next = lineEndWithBreak(markdown, lineStart);
      if (next <= lineStart || next >= markdown.length) break;
      lineStart = next;
    }

    final open = active;
    if (open == null) return null;
    return _withClosingFence(markdown, open);
  }

  static FlarkMarkdownFencedCodeContext? contextForOpeningLine(
    String markdown,
    int openingLineStart,
  ) {
    if (openingLineStart < 0 || openingLineStart >= markdown.length) {
      return null;
    }
    final lineStart = lineStartForOffset(markdown, openingLineStart);
    if (lineStart != openingLineStart) return null;
    final lineEnd = lineContentEnd(markdown, lineStart);
    final fence = fenceLine(markdown.substring(lineStart, lineEnd));
    if (fence == null) return null;
    final open = _openContext(
      markdown: markdown,
      lineStart: lineStart,
      fence: fence,
    );
    return _withClosingFence(markdown, open);
  }

  static FlarkMarkdownFenceLine? fenceLine(String lineText) {
    var index = 0;
    while (index < lineText.length && index < 3) {
      final codeUnit = lineText.codeUnitAt(index);
      if (codeUnit != 32 && codeUnit != 9) break;
      index++;
    }
    final markerStart = index;
    if (markerStart >= lineText.length) return null;

    final markerCodeUnit = lineText.codeUnitAt(markerStart);
    if (markerCodeUnit != 96 && markerCodeUnit != 126) return null;
    while (index < lineText.length &&
        lineText.codeUnitAt(index) == markerCodeUnit) {
      index++;
    }

    final markerLength = index - markerStart;
    if (markerLength < 3) return null;

    final tail = lineText.substring(index);
    if (markerCodeUnit == 96 && tail.contains('`')) return null;
    final trimmedTail = tail.trim();
    final language = trimmedTail.isEmpty ? null : firstWord(trimmedTail);
    return FlarkMarkdownFenceLine(
      indent: lineText.substring(0, markerStart),
      marker: String.fromCharCode(markerCodeUnit),
      markerLength: markerLength,
      infoString: trimmedTail.isEmpty ? null : trimmedTail,
      language: language,
      canClose: trimmedTail.isEmpty,
    );
  }

  static String firstWord(String text) {
    var end = 0;
    while (end < text.length) {
      final codeUnit = text.codeUnitAt(end);
      if (codeUnit == 32 || codeUnit == 9) break;
      end++;
    }
    return text.substring(0, end);
  }

  static int lineStartForOffset(String text, int rawOffset) {
    if (text.isEmpty) return 0;
    final offset = rawOffset.clamp(0, text.length);
    if (offset == 0) return 0;
    final newline = text.lastIndexOf('\n', offset - 1);
    return newline < 0 ? 0 : newline + 1;
  }

  static int lineEndWithBreak(String text, int lineStart) {
    final newline = text.indexOf('\n', lineStart);
    return newline < 0 ? text.length : newline + 1;
  }

  static int lineContentEnd(String text, int lineStart) {
    final lineEnd = lineEndWithBreak(text, lineStart);
    if (lineEnd > lineStart && text.codeUnitAt(lineEnd - 1) == 10) {
      return lineEnd - 1;
    }
    return lineEnd;
  }

  static String leadingHorizontalWhitespace(String text) {
    var index = 0;
    while (index < text.length) {
      final codeUnit = text.codeUnitAt(index);
      if (codeUnit != 32 && codeUnit != 9) break;
      index++;
    }
    return text.substring(0, index);
  }

  static bool isHorizontalWhitespace(String text) {
    for (var index = 0; index < text.length; index++) {
      final codeUnit = text.codeUnitAt(index);
      if (codeUnit != 32 && codeUnit != 9) return false;
    }
    return true;
  }

  static bool isWhitespace(String text) {
    for (var index = 0; index < text.length; index++) {
      final codeUnit = text.codeUnitAt(index);
      if (codeUnit != 32 && codeUnit != 9 && codeUnit != 10 && codeUnit != 13) {
        return false;
      }
    }
    return true;
  }

  static FlarkMarkdownFencedCodeContext _openContext({
    required String markdown,
    required int lineStart,
    required FlarkMarkdownFenceLine fence,
  }) {
    return FlarkMarkdownFencedCodeContext(
      openingLineStart: lineStart,
      openingLineEndWithBreak: lineEndWithBreak(markdown, lineStart),
      bodyStart: lineEndWithBreak(markdown, lineStart),
      openingIndent: fence.indent,
      marker: fence.marker,
      markerLength: fence.markerLength,
      infoString: fence.infoString,
      language: fence.language,
      closingLineStart: null,
      closingLineEnd: null,
      closingLineEndWithBreak: null,
    );
  }

  static FlarkMarkdownFencedCodeContext _withClosingFence(
    String markdown,
    FlarkMarkdownFencedCodeContext open,
  ) {
    int? closingLineStart;
    int? closingLineEnd;
    int? closingLineEndWithBreak;
    var lineStart = open.openingLineEndWithBreak;
    while (lineStart < markdown.length) {
      final lineEnd = lineContentEnd(markdown, lineStart);
      final fence = fenceLine(markdown.substring(lineStart, lineEnd));
      if (fence != null && fence.closes(open)) {
        closingLineStart = lineStart;
        closingLineEnd = lineEnd;
        closingLineEndWithBreak =
            FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
              markdown,
              lineStart,
            );
        break;
      }
      final next = lineEndWithBreak(markdown, lineStart);
      if (next <= lineStart || next >= markdown.length) break;
      lineStart = next;
    }

    return FlarkMarkdownFencedCodeContext(
      openingLineStart: open.openingLineStart,
      openingLineEndWithBreak: open.openingLineEndWithBreak,
      bodyStart: open.bodyStart,
      openingIndent: open.openingIndent,
      marker: open.marker,
      markerLength: open.markerLength,
      infoString: open.infoString,
      language: open.language,
      closingLineStart: closingLineStart,
      closingLineEnd: closingLineEnd,
      closingLineEndWithBreak: closingLineEndWithBreak,
    );
  }
}

bool _bodyHasContentBeforeFinalLineBreak(
  String markdown,
  int bodyStart,
  int bodyEnd,
) {
  var scanEnd = bodyEnd;
  if (scanEnd > bodyStart && markdown.codeUnitAt(scanEnd - 1) == 0x0A) {
    scanEnd--;
  }
  if (scanEnd > bodyStart && markdown.codeUnitAt(scanEnd - 1) == 0x0D) {
    scanEnd--;
  }
  if (scanEnd == bodyEnd) return false;
  for (var index = bodyStart; index < scanEnd; index++) {
    final codeUnit = markdown.codeUnitAt(index);
    if (codeUnit != 0x0A && codeUnit != 0x0D) return true;
  }
  return false;
}

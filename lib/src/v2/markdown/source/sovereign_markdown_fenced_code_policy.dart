import '../../core/core.dart';
import 'sovereign_markdown_editing_result.dart';
import 'sovereign_markdown_fenced_code_scanner.dart';

final class SovereignMarkdownFencedCodePolicy {
  const SovereignMarkdownFencedCodePolicy._();

  static SovereignMarkdownSourceEdit? enter({
    required String markdown,
    required int caret,
  }) {
    final context = SovereignMarkdownFencedCodeScanner.contextAt(
      markdown,
      caret,
    );
    if (context == null) return null;

    final lineStart =
        SovereignMarkdownFencedCodeScanner.lineStartForOffset(markdown, caret);
    final lineEnd =
        SovereignMarkdownFencedCodeScanner.lineContentEnd(markdown, lineStart);
    if (_isTrailingBlankFenceLine(
      markdown: markdown,
      context: context,
      lineStart: lineStart,
      lineEnd: lineEnd,
    )) {
      return _exitFenceOnBlankLine(
        markdown: markdown,
        context: context,
        lineStart: lineStart,
      );
    }

    final beforeCaret = markdown.substring(lineStart, caret);
    final indent =
        SovereignMarkdownFencedCodeScanner.leadingHorizontalWhitespace(
      beforeCaret,
    );
    final trimmedBeforeCaret = beforeCaret.trimRight();
    final extraIndent = _shouldIncreaseIndent(
      trimmedBeforeCaret,
      context.language,
    )
        ? _preferredIndentUnit(indent)
        : '';
    final replacement = '\n$indent$extraIndent';
    return SovereignMarkdownSourceEdit(
      range: SovereignSourceRange(caret, caret),
      replacementText: replacement,
      selectionAfter: SovereignSelection.collapsed(
        caret + replacement.length,
      ),
    );
  }

  static SovereignMarkdownSourceEdit? autoOutdentCloserInsertion({
    required String markdown,
    required int insertionOffset,
    required String insertedText,
  }) {
    if (!_isAutoOutdentCloser(insertedText)) return null;
    final context = SovereignMarkdownFencedCodeScanner.contextAt(
      markdown,
      insertionOffset,
    );
    if (context == null) return null;

    final lineStart = SovereignMarkdownFencedCodeScanner.lineStartForOffset(
      markdown,
      insertionOffset,
    );
    final lineEnd =
        SovereignMarkdownFencedCodeScanner.lineContentEnd(markdown, lineStart);
    final beforeCaret = markdown.substring(lineStart, insertionOffset);
    final afterCaret = markdown.substring(insertionOffset, lineEnd);
    if (!SovereignMarkdownFencedCodeScanner.isHorizontalWhitespace(
          beforeCaret,
        ) ||
        !SovereignMarkdownFencedCodeScanner.isHorizontalWhitespace(
          afterCaret,
        ) ||
        beforeCaret.isEmpty) {
      return null;
    }

    final unit = _preferredOutdentUnitForLine(
      markdown: markdown,
      context: context,
      lineStart: lineStart,
      currentIndent: beforeCaret,
    );
    final outdented = _removeOneIndentUnit(beforeCaret, unit);
    if (outdented == beforeCaret) return null;

    final replacement = '$outdented$insertedText';
    return SovereignMarkdownSourceEdit(
      range: SovereignSourceRange(lineStart, insertionOffset),
      replacementText: replacement,
      selectionAfter: SovereignSelection.collapsed(
        lineStart + replacement.length,
      ),
    );
  }

  static SovereignMarkdownSourceEdit? multilinePasteIndentation({
    required String markdown,
    required int insertionOffset,
    required String insertedText,
  }) {
    if (!insertedText.contains('\n')) return null;
    final context = SovereignMarkdownFencedCodeScanner.contextAt(
      markdown,
      insertionOffset,
    );
    if (context == null) return null;

    final lineStart = SovereignMarkdownFencedCodeScanner.lineStartForOffset(
      markdown,
      insertionOffset,
    );
    final beforeCaret = markdown.substring(lineStart, insertionOffset);
    final baseIndent =
        SovereignMarkdownFencedCodeScanner.leadingHorizontalWhitespace(
      beforeCaret,
    );
    if (baseIndent.isEmpty) return null;

    final normalized = _prefixLinesAfterNewline(insertedText, baseIndent);
    if (normalized == insertedText) return null;
    return SovereignMarkdownSourceEdit(
      range: SovereignSourceRange(insertionOffset, insertionOffset),
      replacementText: normalized,
      selectionAfter: SovereignSelection.collapsed(
        insertionOffset + normalized.length,
      ),
    );
  }

  static SovereignMarkdownInputResult? backspace({
    required String markdown,
    required int caret,
  }) {
    final previousFence = _closedFenceBeforeCaret(markdown, caret);
    if (previousFence != null) {
      final bodyRange = previousFence.bodyContentRange(markdown);
      return SovereignMarkdownSelectionMove(
        selectionAfter: SovereignSelection.collapsed(bodyRange.end),
      );
    }

    final context = SovereignMarkdownFencedCodeScanner.contextAt(
          markdown,
          caret,
        ) ??
        _emptyClosedFenceContextAtBodyStart(markdown, caret);
    if (context == null) return null;
    if (caret != context.bodyStart) return null;

    final bodyEnd = context.bodyEnd(markdown);
    final bodyText = markdown.substring(context.bodyStart, bodyEnd);
    if (!SovereignMarkdownFencedCodeScanner.isWhitespace(bodyText)) {
      return null;
    }

    final removeEnd = context.closingLineEndWithBreak ?? markdown.length;
    return SovereignMarkdownSourceEdit(
      range: SovereignSourceRange(context.openingLineStart, removeEnd),
      replacementText: '',
      selectionAfter: SovereignSelection.collapsed(context.openingLineStart),
    );
  }

  static List<SovereignSourceOperation> indentOperations({
    required String markdown,
    required SovereignSourceRange bodyRange,
    required SovereignSelection selection,
  }) {
    if (selection.isCollapsed) {
      return [SovereignSourceOperation.insert(selection.start, '  ')];
    }

    return [
      for (final lineStart in _selectedCodeLineStarts(
        markdown: markdown,
        bodyRange: bodyRange,
        selection: selection,
      ))
        SovereignSourceOperation.insert(lineStart, '  '),
    ];
  }

  static List<SovereignSourceOperation> outdentOperations({
    required String markdown,
    required SovereignSourceRange bodyRange,
    required SovereignSelection selection,
  }) {
    final operations = <SovereignSourceOperation>[];
    for (final lineStart in _selectedCodeLineStarts(
      markdown: markdown,
      bodyRange: bodyRange,
      selection: selection,
    )) {
      if (lineStart >= bodyRange.end) continue;
      final first = markdown.codeUnitAt(lineStart);
      if (first == 9) {
        operations.add(
          SovereignSourceOperation.delete(lineStart, lineStart + 1),
        );
        continue;
      }
      if (first != 32) continue;
      var deleteEnd = lineStart + 1;
      if (deleteEnd < bodyRange.end && markdown.codeUnitAt(deleteEnd) == 32) {
        deleteEnd++;
      }
      operations.add(SovereignSourceOperation.delete(lineStart, deleteEnd));
    }
    return operations;
  }
}

SovereignMarkdownFencedCodeContext? _closedFenceBeforeCaret(
  String markdown,
  int caret,
) {
  if (caret <= 0 || caret > markdown.length) return null;

  var scanLineStart = 0;
  while (scanLineStart < caret) {
    final context = SovereignMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      scanLineStart,
    );
    final closingLineEnd = context?.closingLineEnd;
    final closingLineEndWithBreak = context?.closingLineEndWithBreak;
    if (closingLineEnd != null &&
        (closingLineEnd == caret || closingLineEndWithBreak == caret)) {
      return context;
    }

    final next = SovereignMarkdownFencedCodeScanner.lineEndWithBreak(
      markdown,
      scanLineStart,
    );
    if (next <= scanLineStart || next > markdown.length) break;
    scanLineStart = next;
  }
  return null;
}

SovereignMarkdownFencedCodeContext? _emptyClosedFenceContextAtBodyStart(
  String markdown,
  int caret,
) {
  if (markdown.isEmpty) return null;
  final lineStart = SovereignMarkdownFencedCodeScanner.lineStartForOffset(
    markdown,
    caret,
  );
  if (lineStart != caret) return null;

  var scanLineStart = 0;
  while (scanLineStart < lineStart) {
    final context = SovereignMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      scanLineStart,
    );
    if (context != null &&
        context.bodyStart == lineStart &&
        context.closingLineStart == lineStart) {
      return context;
    }

    final next = SovereignMarkdownFencedCodeScanner.lineEndWithBreak(
      markdown,
      scanLineStart,
    );
    if (next <= scanLineStart || next > markdown.length) break;
    scanLineStart = next;
  }
  return null;
}

SovereignMarkdownSourceEdit? _exitFenceOnBlankLine({
  required String markdown,
  required SovereignMarkdownFencedCodeContext context,
  required int lineStart,
}) {
  if (context.closingLineStart != null) {
    final closeLineStart = context.closingLineStart!;
    final closeLineEnd = context.closingLineEnd!;
    final closeLineEndWithBreak = context.closingLineEndWithBreak!;
    final hasBreakAfterClose = closeLineEndWithBreak > closeLineEnd;
    final closingLine = markdown.substring(closeLineStart, closeLineEnd);
    final replacement = '$closingLine${hasBreakAfterClose ? '' : '\n'}';
    return SovereignMarkdownSourceEdit(
      range: SovereignSourceRange(lineStart, closeLineEnd),
      replacementText: replacement,
      selectionAfter: SovereignSelection.collapsed(
        lineStart + replacement.length,
      ),
    );
  }

  final closingLine =
      '${context.openingIndent}${_repeat(context.marker, context.markerLength)}';
  final replacement = '$closingLine\n';
  return SovereignMarkdownSourceEdit(
    range: SovereignSourceRange(lineStart, markdown.length),
    replacementText: replacement,
    selectionAfter: SovereignSelection.collapsed(
      lineStart + replacement.length,
    ),
  );
}

bool _isTrailingBlankFenceLine({
  required String markdown,
  required SovereignMarkdownFencedCodeContext context,
  required int lineStart,
  required int lineEnd,
}) {
  if (!SovereignMarkdownFencedCodeScanner.isWhitespace(
    markdown.substring(lineStart, lineEnd),
  )) {
    return false;
  }
  final bodyEnd = context.bodyEnd(markdown);
  if (lineStart < context.bodyStart || lineStart > bodyEnd) return false;
  return SovereignMarkdownFencedCodeScanner.isWhitespace(
    markdown.substring(lineStart, bodyEnd),
  );
}

bool _shouldIncreaseIndent(String trimmedBeforeCaret, String? language) {
  if (trimmedBeforeCaret.isEmpty) return false;
  final last = trimmedBeforeCaret.codeUnitAt(trimmedBeforeCaret.length - 1);
  if (last == 123 || last == 91 || last == 40) return true;
  if (last != 58) return false;
  return switch (language?.trim().toLowerCase()) {
    'python' || 'py' || 'yaml' || 'yml' || 'bash' || 'sh' || 'shell' => true,
    _ => false,
  };
}

String _preferredIndentUnit(String currentIndent) {
  if (currentIndent.contains('\t')) return '\t';
  if (currentIndent.length >= 4 && currentIndent.length % 4 == 0) {
    return '    ';
  }
  return '  ';
}

String _preferredOutdentUnitForLine({
  required String markdown,
  required SovereignMarkdownFencedCodeContext context,
  required int lineStart,
  required String currentIndent,
}) {
  if (currentIndent.startsWith('\t')) return '\t';

  final currentWidth = currentIndent.length;
  var scanEnd = lineStart;
  while (scanEnd > context.bodyStart) {
    final previousLineStart =
        SovereignMarkdownFencedCodeScanner.lineStartForOffset(
      markdown,
      scanEnd - 1,
    );
    if (previousLineStart < context.bodyStart) break;
    final previousLineEnd = SovereignMarkdownFencedCodeScanner.lineContentEnd(
      markdown,
      previousLineStart,
    );
    final previousLine = markdown.substring(previousLineStart, previousLineEnd);
    scanEnd = previousLineStart;
    if (SovereignMarkdownFencedCodeScanner.isWhitespace(previousLine)) {
      continue;
    }

    final previousIndent =
        SovereignMarkdownFencedCodeScanner.leadingHorizontalWhitespace(
      previousLine,
    );
    if (!SovereignMarkdownFencedCodeScanner.isHorizontalWhitespace(
      previousIndent,
    )) {
      continue;
    }
    final previousWidth = previousIndent.length;
    if (previousWidth >= currentWidth) continue;
    final delta = currentWidth - previousWidth;
    if (delta <= 0 || delta > currentWidth) continue;
    return _repeat(' ', delta);
  }

  return _preferredIndentUnit(currentIndent);
}

String _removeOneIndentUnit(String indent, String unit) {
  if (indent.isEmpty) return indent;
  if (indent.startsWith('\t')) return indent.substring(1);

  var leadingSpaces = 0;
  while (
      leadingSpaces < indent.length && indent.codeUnitAt(leadingSpaces) == 32) {
    leadingSpaces++;
  }
  if (leadingSpaces == 0) return indent;

  final requestedSpaces = unit == '\t' ? 4 : unit.length.clamp(1, 4);
  final removeSpaces = requestedSpaces.clamp(1, leadingSpaces);
  return indent.substring(removeSpaces);
}

bool _isAutoOutdentCloser(String text) {
  if (text.length != 1) return false;
  final codeUnit = text.codeUnitAt(0);
  return codeUnit == 125 || codeUnit == 93 || codeUnit == 41;
}

String _prefixLinesAfterNewline(String text, String prefix) {
  final buffer = StringBuffer();
  for (var index = 0; index < text.length; index++) {
    final codeUnit = text.codeUnitAt(index);
    buffer.writeCharCode(codeUnit);
    if (codeUnit != 10) continue;
    buffer.write(prefix);
  }
  return buffer.toString();
}

List<int> _selectedCodeLineStarts({
  required String markdown,
  required SovereignSourceRange bodyRange,
  required SovereignSelection selection,
}) {
  final starts = <int>[];
  final selectionStart = selection.start.clamp(bodyRange.start, bodyRange.end);
  var selectionEnd = selection.end.clamp(bodyRange.start, bodyRange.end);
  if (!selection.isCollapsed &&
      selectionEnd > selectionStart &&
      markdown.codeUnitAt(selectionEnd - 1) == 10) {
    selectionEnd--;
  }

  var lineStart = _lineStartBefore(markdown, selectionStart, bodyRange.start);
  while (lineStart <= selectionEnd) {
    starts.add(lineStart);
    final nextBreak = markdown.indexOf('\n', lineStart);
    if (nextBreak < 0 || nextBreak + 1 > bodyRange.end) break;
    lineStart = nextBreak + 1;
    if (lineStart > selectionEnd) break;
  }
  return starts;
}

int _lineStartBefore(String markdown, int offset, int lowerBound) {
  if (offset <= lowerBound) return lowerBound;
  final newline = markdown.lastIndexOf('\n', offset - 1);
  if (newline < lowerBound) return lowerBound;
  return newline + 1;
}

String _repeat(String text, int count) {
  final buffer = StringBuffer();
  for (var i = 0; i < count; i++) {
    buffer.write(text);
  }
  return buffer.toString();
}

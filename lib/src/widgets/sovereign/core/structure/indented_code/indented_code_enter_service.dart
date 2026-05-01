import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/geometry_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import '../markdown_line_helpers.dart';
import '../navigation/navigation_line_utils.dart';

class IndentedCodeEnterService {
  const IndentedCodeEnterService();

  bool tryHandleEnter({
    required TextEditingValue value,
    required int caret,
    required LineIndex lineIndex,
    required GeometryModel geometry,
    required void Function(TextEditingValue newValue)
        commitProgrammaticTextEdit,
  }) {
    final text = value.text;
    if (caret < 0 || caret > text.length) return false;
    final line = lineIndex.lineAtOffset(caret);
    if (line < 0 || line >= lineIndex.lineCount) return false;

    final lineStart = lineIndex.offsetAtLine(line);
    if (_isLineInsideFencedGeometry(
      lineStartOffset: lineStart,
      geometry: geometry,
    )) {
      return false;
    }

    final lineEndWithBreak = NavigationLineUtils.lineEndWithBreak(
      lineIndex,
      text,
      line,
    );
    final lineEnd = NavigationLineUtils.lineContentEnd(
      text,
      lineStart,
      lineEndWithBreak,
    );
    if (lineStart > lineEnd || lineEnd > text.length) return false;

    final lineText = text.substring(lineStart, lineEnd);
    final indent = NavigationLineUtils.leadingWhitespacePrefix(lineText);
    if (indent.isEmpty) return false;

    final isTabIndented = indent.codeUnitAt(0) == 9;
    final isFourSpaceIndented = !isTabIndented && indent.length >= 4;
    if (!isTabIndented && !isFourSpaceIndented) return false;

    if (_lineStartsWithListMarkerAfterIndent(text, lineStart, lineEnd)) {
      return false;
    }

    final hasCodeContext = _lineLooksLikeIndentedCode(lineText) ||
        (line > 0 &&
            _lineLooksLikeIndentedCodeAt(
              text: text,
              line: line - 1,
              lineIndex: lineIndex,
              geometry: geometry,
            ));
    if (!hasCodeContext) return false;

    final newText = text.replaceRange(caret, caret, '\n$indent');
    commitProgrammaticTextEdit(
      value.copyWith(
        text: newText,
        selection: TextSelection.collapsed(offset: caret + 1 + indent.length),
        composing: TextRange.empty,
      ),
    );
    return true;
  }

  bool _lineLooksLikeIndentedCodeAt({
    required String text,
    required int line,
    required LineIndex lineIndex,
    required GeometryModel geometry,
  }) {
    if (line < 0 || line >= lineIndex.lineCount) return false;
    final lineStart = lineIndex.offsetAtLine(line);
    if (_isLineInsideFencedGeometry(
      lineStartOffset: lineStart,
      geometry: geometry,
    )) {
      return false;
    }
    final lineEndWithBreak = NavigationLineUtils.lineEndWithBreak(
      lineIndex,
      text,
      line,
    );
    final lineEnd = NavigationLineUtils.lineContentEnd(
      text,
      lineStart,
      lineEndWithBreak,
    );
    if (lineEnd <= lineStart) return false;
    if (_lineStartsWithListMarkerAfterIndent(text, lineStart, lineEnd)) {
      return false;
    }
    return _lineLooksLikeIndentedCode(text.substring(lineStart, lineEnd));
  }

  bool _lineStartsWithListMarkerAfterIndent(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    if (lineStart < 0 || lineEnd > text.length || lineStart >= lineEnd) {
      return false;
    }
    var cursor = lineStart;
    while (cursor < lineEnd) {
      final cu = text.codeUnitAt(cursor);
      if (cu == 32 || cu == 9) {
        cursor++;
        continue;
      }
      break;
    }
    if (cursor >= lineEnd) return false;
    return MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
          text,
          cursor,
          lineEnd,
        ) !=
        null;
  }

  bool _lineLooksLikeIndentedCode(String lineText) {
    final indent = NavigationLineUtils.leadingWhitespacePrefix(lineText);
    if (indent.isEmpty) return false;
    if (indent.codeUnitAt(0) == 9) return true;
    return indent.length >= 4;
  }

  bool _isLineInsideFencedGeometry({
    required int lineStartOffset,
    required GeometryModel geometry,
  }) {
    for (final block in geometry.codeBlocks) {
      if (lineStartOffset >= block.startOffset &&
          lineStartOffset < block.endOffset) {
        return true;
      }
    }
    return false;
  }
}

import 'package:flutter/services.dart';

import 'package:sovereign_editor/src/widgets/sovereign/logic/markdown_marker_grammar.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

class MarkdownStructureTransformService {
  const MarkdownStructureTransformService();

  TextEditingValue maybeExitEmptyAtxHeadingOnEnter({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required int? enterCaret,
    required LineIndex lineIndex,
    required bool Function(String text, int caret) isCaretInFencedCode,
  }) {
    final caret = enterCaret;
    if (caret == null) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    if (isCaretInFencedCode(oldText, caret)) return newValue;

    final oldLine = lineIndex.lineAtOffset(caret);
    final lineStart = lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = (oldLine + 1 < lineIndex.lineCount)
        ? lineIndex.offsetAtLine(oldLine + 1)
        : oldText.length;
    final lineEnd = (lineEndWithBreak > lineStart &&
            oldText.codeUnitAt(lineEndWithBreak - 1) == 10)
        ? lineEndWithBreak - 1
        : lineEndWithBreak;
    if (lineEnd <= lineStart) return newValue;

    final lineText = oldText.substring(lineStart, lineEnd);
    final heading = MarkdownMarkerGrammar.matchAtxHeading(
      lineText,
      dialect: MarkdownMarkerDialect.commonMark,
    );
    if (heading == null) return newValue;

    final markerStart = lineStart + heading.markerStartIndex;
    final markerEnd = markerStart + heading.level;
    if (caret < markerEnd) return newValue;

    var contentStart = markerEnd;
    while (contentStart < lineEnd) {
      final cu = oldText.codeUnitAt(contentStart);
      if (cu == 32 || cu == 9) {
        contentStart++;
        continue;
      }
      break;
    }
    if (contentStart < lineEnd) {
      return newValue;
    }

    final newlineIndex = caret.clamp(0, newText.length);
    if (markerStart >= newlineIndex) return newValue;
    final exited = newText.replaceRange(markerStart, newlineIndex, '');
    final caretShift = newlineIndex - markerStart;
    final targetCaret = (newValue.selection.baseOffset - caretShift).clamp(
      0,
      exited.length,
    );
    return newValue.copyWith(
      text: exited,
      selection: TextSelection.collapsed(offset: targetCaret),
      composing: TextRange.empty,
    );
  }
}

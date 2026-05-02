import 'package:flutter/services.dart';

import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_line_helpers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/list_marker_context.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/quote_context.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/navigation_line_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/projection_range_utils.dart';
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

  TextEditingValue maybeContinueOrExitBlockquoteOnEnter({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required int? enterCaret,
    required LineIndex lineIndex,
    required bool Function(String text, int caret) isCaretInFencedCode,
    required QuoteContext? Function(String text, int line) quoteContextForLine,
    required bool Function(String text, int line) isQuoteLineBodyBlank,
  }) {
    final caret = enterCaret;
    if (caret == null) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    if (isCaretInFencedCode(oldText, caret)) return newValue;

    final oldLine = lineIndex.lineAtOffset(caret);
    final quoteContext = quoteContextForLine(oldText, oldLine);
    if (quoteContext == null) return newValue;

    final lineStart = lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = (oldLine + 1 < lineIndex.lineCount)
        ? lineIndex.offsetAtLine(oldLine + 1)
        : oldText.length;
    final lineEnd = (lineEndWithBreak > lineStart &&
            oldText.codeUnitAt(lineEndWithBreak - 1) == 10)
        ? lineEndWithBreak - 1
        : lineEndWithBreak;
    final markerLen = ProjectionRangeUtils.blockquoteMarkerLength(
      oldText,
      lineStart,
      lineEnd,
    );
    if (markerLen <= 0) return newValue;

    if (caret < lineStart + markerLen) return newValue;

    if (isQuoteLineBodyBlank(oldText, oldLine)) {
      if (!newText.startsWith('> ', lineStart)) return newValue;
      var removeEnd = (lineStart + markerLen).clamp(0, newText.length);
      while (removeEnd < newText.length) {
        final cu = newText.codeUnitAt(removeEnd);
        if (cu == 32 || cu == 9) {
          removeEnd++;
          continue;
        }
        break;
      }
      final exited = newText.replaceRange(lineStart, removeEnd, '');
      final caretShift = removeEnd - lineStart;
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

    final insertAt = (caret + 1).clamp(0, newText.length);
    final continued = newText.replaceRange(insertAt, insertAt, '> ');
    return newValue.copyWith(
      text: continued,
      selection: TextSelection.collapsed(offset: insertAt + 2),
      composing: TextRange.empty,
    );
  }

  TextEditingValue maybeContinueOrExitListOnEnter({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required int? enterCaret,
    required LineIndex lineIndex,
    required bool Function(String text, int caret) isCaretInFencedCode,
    required ListMarkerContext? Function(
      String text,
      int lineStart,
      int lineEnd,
    ) editableListMarkerForLine,
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
    final marker = editableListMarkerForLine(oldText, lineStart, lineEnd);
    if (marker == null) return newValue;

    if (caret < marker.contentStart) return newValue;

    if (MarkdownLineHelpers.isLineBodyBlankFrom(
      oldText,
      marker.contentStart,
      lineEnd,
    )) {
      final expectedCaret = (caret + 1).clamp(0, newText.length);
      final currentCaret =
          (newValue.selection.isValid && newValue.selection.isCollapsed)
              ? newValue.selection.baseOffset.clamp(0, newText.length)
              : expectedCaret;
      if (currentCaret != expectedCaret) return newValue;

      var removeEnd = marker.contentStart;
      while (removeEnd < newText.length) {
        final cu = newText.codeUnitAt(removeEnd);
        if (cu == 32 || cu == 9) {
          removeEnd++;
          continue;
        }
        break;
      }
      final markerStart = marker.markerStart.clamp(0, removeEnd).toInt();
      final exited = newText.replaceRange(markerStart, removeEnd, '');
      final caretShift = removeEnd - markerStart;
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

    var insertAt = (caret + 1).clamp(0, newText.length);
    if (newValue.selection.isValid && newValue.selection.isCollapsed) {
      final selectionOffset = newValue.selection.baseOffset.clamp(
        0,
        newText.length,
      );
      if (selectionOffset >= insertAt) {
        insertAt = selectionOffset;
      }
    }
    var continueMarker = marker.continueMarker;
    if (marker.markerStart > lineStart) {
      final prefix = oldText.substring(lineStart, marker.markerStart);
      if (NavigationLineUtils.isHorizontalWhitespaceOnly(prefix)) {
        continueMarker = '$prefix$continueMarker';
      }
    }

    final continued = newText.replaceRange(insertAt, insertAt, continueMarker);
    return newValue.copyWith(
      text: continued,
      selection: TextSelection.collapsed(
        offset: insertAt + continueMarker.length,
      ),
      composing: TextRange.empty,
    );
  }
}

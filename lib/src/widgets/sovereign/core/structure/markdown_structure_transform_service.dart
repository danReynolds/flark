import 'package:flutter/services.dart';

import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_line_helpers.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/fence_context.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/list_marker_context.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/quote_context.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/models/vertical_arrow_edit_context.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/navigation/navigation_line_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/table/table_editing_service.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/syntax/projection_range_utils.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/markdown_marker_grammar.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

typedef BlockquoteArrowExitPredicate = bool Function({
  required String text,
  required QuoteContext context,
  required int fromLine,
  required int toLine,
});

typedef FenceArrowExitPredicate = bool Function({
  required String text,
  required FenceContext context,
  required int fromLine,
  required int toLine,
});

class MarkdownStructureTransformService {
  const MarkdownStructureTransformService();

  static const TableEditingService _tableEditing = TableEditingService();

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

  TextEditingValue maybeExitBlockquoteOnArrowDown({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required QuoteContext? Function(String text, int line) quoteContextForLine,
    required BlockquoteArrowExitPredicate shouldExitBlockquoteOnArrowDown,
  }) =>
      _maybeExitBlockquoteOnVerticalArrow(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        movingDown: true,
        quoteContextForLine: quoteContextForLine,
        shouldExitBlockquote: shouldExitBlockquoteOnArrowDown,
      );

  TextEditingValue maybeExitBlockquoteOnArrowUp({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required QuoteContext? Function(String text, int line) quoteContextForLine,
    required BlockquoteArrowExitPredicate shouldExitBlockquoteOnArrowUp,
  }) =>
      _maybeExitBlockquoteOnVerticalArrow(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        movingDown: false,
        quoteContextForLine: quoteContextForLine,
        shouldExitBlockquote: shouldExitBlockquoteOnArrowUp,
      );

  TextEditingValue _maybeExitBlockquoteOnVerticalArrow({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required bool movingDown,
    required QuoteContext? Function(String text, int line) quoteContextForLine,
    required BlockquoteArrowExitPredicate shouldExitBlockquote,
  }) {
    final arrow = VerticalArrowEditContext.detect(
      oldValue: oldValue,
      newValue: newValue,
      lineIndex: lineIndex,
      movingDown: movingDown,
    );
    if (arrow == null) return newValue;

    final quoteContext = quoteContextForLine(arrow.text, arrow.oldLine);
    if (quoteContext == null) return newValue;
    if (!shouldExitBlockquote(
      text: arrow.text,
      context: quoteContext,
      fromLine: arrow.oldLine,
      toLine: arrow.newLine,
    )) {
      return newValue;
    }

    final column = arrow.oldCaret - lineIndex.offsetAtLine(arrow.oldLine);
    final exitOffset = NavigationLineUtils.columnAlignedOffsetForLineOrBoundary(
      text: arrow.text,
      lineIndex: lineIndex,
      line: movingDown
          ? quoteContext.endLineExclusive
          : quoteContext.startLine - 1,
      column: column,
      afterDocument: movingDown,
    );
    return newValue.copyWith(
      selection: TextSelection.collapsed(offset: exitOffset),
      composing: TextRange.empty,
    );
  }

  TextEditingValue maybeExitFencedCodeOnArrowDown({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required FenceContext? Function(
      String text,
      int caret, {
      required bool includeUnclosedEof,
    }) fenceContextForCaret,
    required FenceArrowExitPredicate shouldExitFenceOnArrowDown,
    required int Function(String text, int openLine, int closeLineExclusive)
        trailingBlankTrimStart,
  }) {
    final arrow = VerticalArrowEditContext.detect(
      oldValue: oldValue,
      newValue: newValue,
      lineIndex: lineIndex,
      movingDown: true,
    );
    if (arrow == null) return newValue;

    final context = fenceContextForCaret(
      arrow.text,
      arrow.oldCaret,
      includeUnclosedEof: false,
    );
    if (context == null || !context.hasClosingFence) {
      return newValue;
    }

    if (!shouldExitFenceOnArrowDown(
      text: arrow.text,
      context: context,
      fromLine: arrow.oldLine,
      toLine: arrow.newLine,
    )) {
      return newValue;
    }

    final closeLine = context.closeLine;
    if (closeLine != null && closeLine > context.openLine) {
      final closeLineStart = lineIndex.offsetAtLine(closeLine);
      final trimStart = trailingBlankTrimStart(
        arrow.text,
        context.openLine,
        closeLine,
      );
      if (trimStart < closeLineStart) {
        final deletedLen = closeLineStart - trimStart;
        var exitText = arrow.text.replaceRange(trimStart, closeLineStart, '');
        var exitOffset = (context.endOffset - deletedLen).clamp(
          0,
          exitText.length,
        );
        if (exitOffset == exitText.length &&
            exitText.isNotEmpty &&
            exitText.codeUnitAt(exitText.length - 1) != 10) {
          exitText = '$exitText\n';
          exitOffset = exitText.length;
        }
        return newValue.copyWith(
          text: exitText,
          selection: TextSelection.collapsed(offset: exitOffset),
          composing: TextRange.empty,
        );
      }
    }

    final exitOffset = context.endOffset.clamp(0, arrow.text.length);
    return newValue.copyWith(
      selection: TextSelection.collapsed(offset: exitOffset),
      composing: TextRange.empty,
    );
  }

  TextEditingValue maybeExitFencedCodeOnArrowUp({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required FenceContext? Function(
      String text,
      int caret, {
      required bool includeUnclosedEof,
    }) fenceContextForCaret,
    required FenceArrowExitPredicate shouldExitFenceOnArrowUp,
  }) {
    final arrow = VerticalArrowEditContext.detect(
      oldValue: oldValue,
      newValue: newValue,
      lineIndex: lineIndex,
      movingDown: false,
    );
    if (arrow == null) return newValue;

    final context = fenceContextForCaret(
      arrow.text,
      arrow.oldCaret,
      includeUnclosedEof: false,
    );
    if (context == null) return newValue;

    if (!shouldExitFenceOnArrowUp(
      text: arrow.text,
      context: context,
      fromLine: arrow.oldLine,
      toLine: arrow.newLine,
    )) {
      return newValue;
    }

    final exitOffset = context.startOffset.clamp(0, arrow.text.length);
    return newValue.copyWith(
      selection: TextSelection.collapsed(offset: exitOffset),
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

  TextEditingValue maybeHandleListBackspaceBoundary({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required bool Function(String text, int caret) isCaretInFencedCode,
    required ListMarkerContext? Function(
      String text,
      int lineStart,
      int lineEnd,
    ) editableListMarkerForLine,
  }) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }
    if (oldValue.text.length != newValue.text.length + 1) return newValue;

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid) return newValue;
    if (!oldSel.isCollapsed || !newSel.isCollapsed) return newValue;

    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret <= 0 || oldCaret > oldValue.text.length) return newValue;
    if (newCaret != oldCaret - 1) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    final deletedOffset = newCaret;
    if (!newText.startsWith(oldText.substring(0, deletedOffset))) {
      return newValue;
    }
    if (newText.substring(deletedOffset) !=
        oldText.substring(deletedOffset + 1)) {
      return newValue;
    }

    if (isCaretInFencedCode(oldText, oldCaret)) {
      return newValue;
    }

    final line = lineIndex.lineAtOffset(oldCaret);
    final lineStart = lineIndex.offsetAtLine(line);
    final lineEndWithBreak = (line + 1 < lineIndex.lineCount)
        ? lineIndex.offsetAtLine(line + 1)
        : oldText.length;
    final lineEnd = (lineEndWithBreak > lineStart &&
            oldText.codeUnitAt(lineEndWithBreak - 1) == 10)
        ? lineEndWithBreak - 1
        : lineEndWithBreak;
    final marker = editableListMarkerForLine(oldText, lineStart, lineEnd);
    if (marker == null) return newValue;
    if (oldCaret != marker.contentStart) return newValue;
    if (deletedOffset != marker.contentStart - 1) return newValue;

    final emptyListItemAtEof = lineStart > 0 &&
        lineEndWithBreak == oldText.length &&
        oldText.codeUnitAt(lineStart - 1) == 10 &&
        MarkdownLineHelpers.isLineBodyBlankFrom(
          oldText,
          marker.contentStart,
          lineEnd,
        );
    if (emptyListItemAtEof) {
      final collapsedText = oldText.replaceRange(lineStart - 1, lineEnd, '');
      final collapsedCaret = (lineStart - 1).clamp(0, collapsedText.length);
      return newValue.copyWith(
        text: collapsedText,
        selection: TextSelection.collapsed(offset: collapsedCaret),
        composing: TextRange.empty,
      );
    }

    final taskAtMarker = MarkdownLineHelpers.taskMarkerInfo(
      oldText,
      marker.markerEnd,
      lineEnd,
    );
    if (taskAtMarker != null &&
        marker.contentStart == taskAtMarker.contentStart) {
      final adjustedText = oldText.replaceRange(
        marker.markerEnd,
        marker.contentStart,
        '',
      );
      final removedLen = marker.contentStart - marker.markerEnd;
      final adjustedCaret = (oldCaret - removedLen).clamp(
        0,
        adjustedText.length,
      );
      return newValue.copyWith(
        text: adjustedText,
        selection: TextSelection.collapsed(offset: adjustedCaret),
        composing: TextRange.empty,
      );
    }

    final adjustedText = oldText.replaceRange(
      marker.markerStart,
      marker.contentStart,
      '',
    );
    final adjustedCaret =
        (oldCaret - (marker.contentStart - marker.markerStart)).clamp(
      0,
      adjustedText.length,
    );
    return newValue.copyWith(
      text: adjustedText,
      selection: TextSelection.collapsed(offset: adjustedCaret),
      composing: TextRange.empty,
    );
  }

  TextEditingValue maybeContinueEstablishedTableOnEnter({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required int? enterCaret,
    required LineIndex lineIndex,
    required bool Function(String text, int caret) isCaretInFencedCode,
  }) =>
      _tableEditing.maybeContinueEstablishedTableOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        isCaretInFencedCode: isCaretInFencedCode,
      );

  String emptyTableRowTemplate(int columns, {required String indent}) =>
      _tableEditing.emptyRowTemplate(columns, indent: indent);

  TableEditingFormatResult? formatEstablishedTableAroundCaret(
    String text,
    int caret,
  ) =>
      _tableEditing.formatEstablishedTableAroundCaret(text, caret);
}

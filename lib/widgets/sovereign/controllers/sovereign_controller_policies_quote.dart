part of 'sovereign_controller.dart';

abstract final class _QuotePolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(
      name: 'exit-arrow-up-blockquote',
      priority: 15,
      apply: _onArrowUp,
    ),
    _EditTransformRule(
      name: 'exit-arrow-down-blockquote',
      priority: 25,
      apply: _onArrowDown,
    ),
    _EditTransformRule(name: 'enter-blockquote', priority: 35, apply: _onEnter),
  ];

  static TextEditingValue _onEnter(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
    final caret = context.intent.enterCaret;
    if (caret == null) return newValue;
    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    // Blockquote enter behavior should never run inside fenced code.
    if (helpers.fenceContextForCaret(
          oldText,
          caret,
          includeUnclosedEof: true,
        ) !=
        null) {
      return newValue;
    }

    final oldLine = helpers.lineIndex.lineAtOffset(caret);
    final quoteContext = helpers.quoteContextForLine(oldText, oldLine);
    if (quoteContext == null) return newValue;

    final lineStart = helpers.lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = helpers.lineEndWithBreak(oldText, oldLine);
    final lineEnd = helpers.lineContentEnd(
      oldText,
      lineStart,
      lineEndWithBreak,
    );
    final markerLen = ProjectionRangeUtils.blockquoteMarkerLength(
      oldText,
      lineStart,
      lineEnd,
    );
    if (markerLen <= 0) return newValue;

    // Pressing Enter before the marker is a normal line split.
    if (caret < lineStart + markerLen) return newValue;

    if (helpers.isQuoteLineBodyBlank(oldText, oldLine)) {
      // Enter on an empty quote line exits quote mode.
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

    // Continue quote on next line.
    final insertAt = (caret + 1).clamp(0, newText.length);
    final continued = newText.replaceRange(insertAt, insertAt, '> ');
    return newValue.copyWith(
      text: continued,
      selection: TextSelection.collapsed(offset: insertAt + 2),
      composing: TextRange.empty,
    );
  }

  static TextEditingValue _onArrowDown(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
    final arrow = _VerticalArrowEditContext.detect(
      oldValue: oldValue,
      newValue: newValue,
      lineIndex: helpers.lineIndex,
      movingDown: true,
    );
    if (arrow == null) return newValue;

    final quoteContext = helpers.quoteContextForLine(arrow.text, arrow.oldLine);
    if (quoteContext == null) return newValue;
    if (!helpers.shouldExitBlockquoteOnArrowDown(
      text: arrow.text,
      context: quoteContext,
      fromLine: arrow.oldLine,
      toLine: arrow.newLine,
    )) {
      return newValue;
    }

    final column =
        arrow.oldCaret - helpers.lineIndex.offsetAtLine(arrow.oldLine);
    final exitOffset = helpers.columnAlignedOffsetForLineOrBoundary(
      text: arrow.text,
      line: quoteContext.endLineExclusive,
      column: column,
      afterDocument: true,
    );
    return newValue.copyWith(
      selection: TextSelection.collapsed(offset: exitOffset),
      composing: TextRange.empty,
    );
  }

  static TextEditingValue _onArrowUp(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
    final arrow = _VerticalArrowEditContext.detect(
      oldValue: oldValue,
      newValue: newValue,
      lineIndex: helpers.lineIndex,
      movingDown: false,
    );
    if (arrow == null) return newValue;

    final quoteContext = helpers.quoteContextForLine(arrow.text, arrow.oldLine);
    if (quoteContext == null) return newValue;
    if (!helpers.shouldExitBlockquoteOnArrowUp(
      text: arrow.text,
      context: quoteContext,
      fromLine: arrow.oldLine,
      toLine: arrow.newLine,
    )) {
      return newValue;
    }

    final column =
        arrow.oldCaret - helpers.lineIndex.offsetAtLine(arrow.oldLine);
    final exitOffset = helpers.columnAlignedOffsetForLineOrBoundary(
      text: arrow.text,
      line: quoteContext.startLine - 1,
      column: column,
      afterDocument: false,
    );
    return newValue.copyWith(
      selection: TextSelection.collapsed(offset: exitOffset),
      composing: TextRange.empty,
    );
  }
}

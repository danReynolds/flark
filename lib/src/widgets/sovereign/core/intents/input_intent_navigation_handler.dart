import 'package:flutter/services.dart';

import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';
import 'input_intent_models.dart';
import '../structure/models/fence_context.dart';
import '../structure/models/quote_context.dart';

abstract class SovereignNavigationIntentHost {
  TextEditingValue get value;
  TextSelection get selection;
  set selection(TextSelection value);
  LineIndex get lineIndex;
  void commitProgrammaticTextEdit(TextEditingValue newValue);

  FenceContext? fenceContextForCaret(
    String text,
    int caret, {
    required bool includeUnclosedEof,
  });
  QuoteContext? quoteContextForLine(String text, int line);

  bool shouldExitFenceOnArrowDown({
    required String text,
    required FenceContext context,
    required int fromLine,
    required int toLine,
  });
  bool shouldExitBlockquoteOnArrowDown({
    required String text,
    required QuoteContext context,
    required int fromLine,
    required int toLine,
  });
  bool shouldExitFenceOnArrowUp({
    required String text,
    required FenceContext context,
    required int fromLine,
    required int toLine,
  });
  bool shouldExitBlockquoteOnArrowUp({
    required String text,
    required QuoteContext context,
    required int fromLine,
    required int toLine,
  });
  int columnAlignedOffsetForLineOrBoundary({
    required String text,
    required int line,
    required int column,
    required bool afterDocument,
  });
  bool moveCaretVertically({required bool forward});
  FenceEnterExitResult? computeFenceExitOnEnter({
    required String text,
    required int caret,
    required FenceContext context,
  });
  int trailingBlankTrimStart(String text, int openLine, int closeLineExclusive);
  bool isWhitespaceLine(String text, int start, int end);
}

class SovereignArrowExitIntentHandler {
  SovereignArrowExitIntentHandler(this._host);

  final SovereignNavigationIntentHost _host;

  bool handleArrowDownKey() {
    if (tryExitFencedCodeOnArrowDown()) return true;
    if (tryExitBlockquoteOnArrowDown()) return true;
    return _host.moveCaretVertically(forward: true);
  }

  bool handleArrowUpKey() {
    if (tryExitFencedCodeOnArrowUp()) return true;
    if (tryExitBlockquoteOnArrowUp()) return true;
    return _host.moveCaretVertically(forward: false);
  }

  bool tryExitFencedCodeOnArrowDown() {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid || !sel.isCollapsed) return false;

    final caret = sel.baseOffset;
    final text = _host.value.text;
    if (caret < 0 || caret > text.length) return false;

    final context = _host.fenceContextForCaret(
      text,
      caret,
      includeUnclosedEof: true,
    );
    if (context == null) return false;

    bool trimTrailingBlankFenceLinesAndExitIfAny() {
      final closeLine = context.closeLine;
      if (closeLine == null || closeLine <= context.openLine) {
        return false;
      }

      final closeLineStart = _host.lineIndex.offsetAtLine(closeLine);
      final trimStart = _host.trailingBlankTrimStart(
        text,
        context.openLine,
        closeLine,
      );
      if (trimStart >= closeLineStart) return false;

      final deletedLen = closeLineStart - trimStart;
      var exitText = text.replaceRange(trimStart, closeLineStart, '');
      var exitCaret = (context.endOffset - deletedLen).clamp(
        0,
        exitText.length,
      );
      if (exitCaret == exitText.length &&
          exitText.isNotEmpty &&
          exitText.codeUnitAt(exitText.length - 1) != 10) {
        exitText = '$exitText\n';
        exitCaret = exitText.length;
      }
      _host.commitProgrammaticTextEdit(
        _host.value.copyWith(
          text: exitText,
          selection: TextSelection.collapsed(offset: exitCaret),
          composing: TextRange.empty,
        ),
      );
      return true;
    }

    final caretLine = _host.lineIndex.lineAtOffset(caret);

    if (context.hasClosingFence && context.closeLine != null) {
      if (caretLine == context.closeLine) {
        if (trimTrailingBlankFenceLinesAndExitIfAny()) return true;
        _host.selection = TextSelection.collapsed(
          offset: context.endOffset.clamp(0, text.length),
        );
        return true;
      }
    }

    if (!context.hasClosingFence) {
      if (context.endOffset != text.length) return false;
      final lastLine = _host.lineIndex.lineCount - 1;
      if (caretLine != lastLine) return false;

      final blankStart = _host.lineIndex.offsetAtLine(lastLine);
      final blankEnd = (lastLine + 1 < _host.lineIndex.lineCount)
          ? _host.lineIndex.offsetAtLine(lastLine + 1)
          : text.length;
      if (!_host.isWhitespaceLine(text, blankStart, blankEnd)) {
        return false;
      }
      final exit = _host.computeFenceExitOnEnter(
        text: text,
        caret: caret,
        context: context,
      );
      if (exit == null) return false;
      _host.commitProgrammaticTextEdit(
        _host.value.copyWith(
          text: exit.text,
          selection: TextSelection.collapsed(offset: exit.caret),
          composing: TextRange.empty,
        ),
      );
      return true;
    }

    final targetLine = (caretLine + 1).clamp(0, _host.lineIndex.lineCount - 1);

    if (_host.shouldExitFenceOnArrowDown(
      text: text,
      context: context,
      fromLine: caretLine,
      toLine: targetLine,
    )) {
      if (trimTrailingBlankFenceLinesAndExitIfAny()) return true;
      _host.selection = TextSelection.collapsed(
        offset: context.endOffset.clamp(0, text.length),
      );
      return true;
    }

    return false;
  }

  bool tryExitBlockquoteOnArrowDown() {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid || !sel.isCollapsed) return false;

    final text = _host.value.text;
    final caret = sel.baseOffset;
    if (caret < 0 || caret > text.length) return false;

    final caretLine = _host.lineIndex.lineAtOffset(caret);
    final context = _host.quoteContextForLine(text, caretLine);
    if (context == null) return false;

    final targetLine = (caretLine + 1).clamp(0, _host.lineIndex.lineCount - 1);
    if (!_host.shouldExitBlockquoteOnArrowDown(
      text: text,
      context: context,
      fromLine: caretLine,
      toLine: targetLine,
    )) {
      return false;
    }

    final column = caret - _host.lineIndex.offsetAtLine(caretLine);
    final exitOffset = _host.columnAlignedOffsetForLineOrBoundary(
      text: text,
      line: context.endLineExclusive,
      column: column,
      afterDocument: true,
    );
    _host.selection = TextSelection.collapsed(offset: exitOffset);
    return true;
  }

  bool tryExitFencedCodeOnArrowUp() {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid || !sel.isCollapsed) return false;

    final caret = sel.baseOffset;
    final text = _host.value.text;
    if (caret < 0 || caret > text.length) return false;

    final context = _host.fenceContextForCaret(
      text,
      caret,
      includeUnclosedEof: true,
    );
    if (context == null) return false;

    final caretLine = _host.lineIndex.lineAtOffset(caret);

    if (caretLine == context.openLine) {
      _host.selection = TextSelection.collapsed(
        offset: context.startOffset.clamp(0, text.length),
      );
      return true;
    }

    final targetLine = (caretLine - 1).clamp(0, _host.lineIndex.lineCount - 1);

    if (!_host.shouldExitFenceOnArrowUp(
      text: text,
      context: context,
      fromLine: caretLine,
      toLine: targetLine,
    )) {
      return false;
    }

    _host.selection = TextSelection.collapsed(
      offset: context.startOffset.clamp(0, text.length),
    );
    return true;
  }

  bool tryExitBlockquoteOnArrowUp() {
    if (_host.value.composing.isValid) return false;
    final sel = _host.selection;
    if (!sel.isValid || !sel.isCollapsed) return false;

    final text = _host.value.text;
    final caret = sel.baseOffset;
    if (caret < 0 || caret > text.length) return false;

    final caretLine = _host.lineIndex.lineAtOffset(caret);
    final context = _host.quoteContextForLine(text, caretLine);
    if (context == null) return false;

    final targetLine = (caretLine - 1).clamp(0, _host.lineIndex.lineCount - 1);
    if (!_host.shouldExitBlockquoteOnArrowUp(
      text: text,
      context: context,
      fromLine: caretLine,
      toLine: targetLine,
    )) {
      return false;
    }

    final column = caret - _host.lineIndex.offsetAtLine(caretLine);
    final exitOffset = _host.columnAlignedOffsetForLineOrBoundary(
      text: text,
      line: context.startLine - 1,
      column: column,
      afterDocument: false,
    );
    _host.selection = TextSelection.collapsed(offset: exitOffset);
    return true;
  }
}

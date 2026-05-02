part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

class _PolicyContext {
  final _PolicyHelpers helpers;
  final TextEditingValue oldValue;
  final _PolicyEditIntent intent;

  const _PolicyContext({
    required this.helpers,
    required this.oldValue,
    required this.intent,
  });
}

class _PolicyEditIntent {
  final int? enterCaret;

  const _PolicyEditIntent._({this.enterCaret});

  static _PolicyEditIntent detect(
    TextEditingValue oldValue,
    TextEditingValue incomingValue,
  ) {
    if (oldValue.composing.isValid || incomingValue.composing.isValid) {
      return const _PolicyEditIntent._();
    }
    final oldSel = oldValue.selection;
    final newSel = incomingValue.selection;
    if (!oldSel.isValid || !oldSel.isCollapsed) {
      return const _PolicyEditIntent._();
    }
    if (!newSel.isValid || !newSel.isCollapsed) {
      return const _PolicyEditIntent._();
    }

    final caret = oldSel.baseOffset;
    final oldText = oldValue.text;
    final newText = incomingValue.text;
    if (caret < 0 || caret > oldText.length) {
      return const _PolicyEditIntent._();
    }
    if (newText.length != oldText.length + 1) {
      return const _PolicyEditIntent._();
    }
    if (caret >= newText.length || newText.codeUnitAt(caret) != 10) {
      return const _PolicyEditIntent._();
    }
    if (newSel.baseOffset != caret + 1) {
      return const _PolicyEditIntent._();
    }
    if (!newText.startsWith(oldText.substring(0, caret))) {
      return const _PolicyEditIntent._();
    }
    if (newText.substring(caret + 1) != oldText.substring(caret)) {
      return const _PolicyEditIntent._();
    }
    return _PolicyEditIntent._(enterCaret: caret);
  }
}

class _EditTransformRule {
  final String name;
  final int priority;
  final TextEditingValue Function(
    _PolicyContext context,
    TextEditingValue value,
  ) apply;

  const _EditTransformRule({
    required this.name,
    required this.priority,
    required this.apply,
  });
}

class _PolicyHelpers {
  final SovereignController _controller;

  const _PolicyHelpers(this._controller);

  LineIndex get lineIndex => _controller._lineIndex;

  TextEditingValue maybeExitFencedCodeOnArrowUp(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeExitFencedCodeOnArrowUp(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        fenceContextForCaret: fenceContextForCaret,
        shouldExitFenceOnArrowUp: _controller._shouldExitFenceOnArrowUp,
      );

  TextEditingValue maybeExitFencedCodeOnArrowDown(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeExitFencedCodeOnArrowDown(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        fenceContextForCaret: fenceContextForCaret,
        shouldExitFenceOnArrowDown: _controller._shouldExitFenceOnArrowDown,
        trailingBlankTrimStart: _controller._trailingBlankTrimStart,
      );

  TextEditingValue maybeExitFencedCodeOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeExitFencedCodeOnEnter(oldValue, newValue);

  TextEditingValue maybeContinueOutsideClosingFenceEof(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeContinueOutsideClosingFenceEof(oldValue, newValue);

  TextEditingValue maybeNormalizeFencedMultilinePaste(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeNormalizeFencedMultilinePaste(oldValue, newValue);

  TextEditingValue maybeKeepClosingFenceOnOwnLine(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeKeepClosingFenceOnOwnLine(oldValue, newValue);

  TextEditingValue maybeExpandFencedPairOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeExpandFencedPairOnEnter(oldValue, newValue);

  TextEditingValue maybeAutoIndentFencedCodeOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeAutoIndentFencedCodeOnEnter(oldValue, newValue);

  TextEditingValue maybeWrapFencedSelectionOnOpenerInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeWrapFencedSelectionOnOpenerInsert(oldValue, newValue);

  TextEditingValue maybeAutoPairFencedOpenerInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeAutoPairFencedOpenerInsert(oldValue, newValue);

  TextEditingValue maybeSkipFencedCloserInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeSkipFencedCloserInsert(oldValue, newValue);

  TextEditingValue maybeOutdentFencedCodeOnCloserInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeOutdentFencedCodeOnCloserInsert(oldValue, newValue);

  TextEditingValue maybeDeleteFencedPairOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._inputIntents.maybeDeleteFencedPairOnBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeOutdentFencedCodeOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._inputIntents.maybeOutdentFencedCodeOnBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeCollapseEmptyFenceOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._inputIntents.maybeCollapseEmptyFenceOnBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeProtectEmptyFenceEntryBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._inputIntents.maybeProtectEmptyFenceEntryBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeProtectHiddenFenceBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._inputIntents.maybeProtectHiddenFenceBackspace(
        oldValue,
        newValue,
      );

  TextEditingValue maybeReenterInlineWrapperOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._inputIntents.maybeReenterInlineWrapperOnBackspace(
        oldValue,
        newValue,
      );

  structure.FenceContext? fenceContextForCaret(
    String text,
    int caret, {
    required bool includeUnclosedEof,
  }) =>
      _controller._fenceContextForCaret(
        text,
        caret,
        includeUnclosedEof: includeUnclosedEof,
      );

  structure.QuoteContext? quoteContextForLine(String text, int line) =>
      _controller._quoteContextForLine(text, line);

  TextEditingValue maybeExitEmptyAtxHeadingOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeExitEmptyAtxHeadingOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        isCaretInFencedCode: (text, caret) =>
            fenceContextForCaret(text, caret, includeUnclosedEof: true) != null,
      );

  TextEditingValue maybeContinueOrExitBlockquoteOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeContinueOrExitBlockquoteOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        isCaretInFencedCode: (text, caret) =>
            fenceContextForCaret(text, caret, includeUnclosedEof: true) != null,
        quoteContextForLine: quoteContextForLine,
        isQuoteLineBodyBlank: isQuoteLineBodyBlank,
      );

  TextEditingValue maybeExitBlockquoteOnArrowDown(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeExitBlockquoteOnArrowDown(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        quoteContextForLine: quoteContextForLine,
        shouldExitBlockquoteOnArrowDown: shouldExitBlockquoteOnArrowDown,
      );

  TextEditingValue maybeExitBlockquoteOnArrowUp(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeExitBlockquoteOnArrowUp(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        quoteContextForLine: quoteContextForLine,
        shouldExitBlockquoteOnArrowUp: shouldExitBlockquoteOnArrowUp,
      );

  TextEditingValue maybeContinueOrExitListOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeContinueOrExitListOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        isCaretInFencedCode: (text, caret) =>
            fenceContextForCaret(text, caret, includeUnclosedEof: true) != null,
        editableListMarkerForLine: editableListMarkerForLine,
      );

  TextEditingValue maybeHandleListBackspaceBoundary(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeHandleListBackspaceBoundary(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        isCaretInFencedCode: (text, caret) =>
            fenceContextForCaret(text, caret, includeUnclosedEof: true) != null,
        editableListMarkerForLine: editableListMarkerForLine,
      );

  TextEditingValue maybeContinueEstablishedTableOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeContinueEstablishedTableOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        isCaretInFencedCode: (text, caret) =>
            fenceContextForCaret(text, caret, includeUnclosedEof: true) != null,
      );

  structure.ListMarkerContext? editableListMarkerForLine(
    String text,
    int lineStart,
    int lineEnd,
  ) =>
      _controller._structureQueries.editableListMarkerForLine(
        text,
        lineStart,
        lineEnd,
      );

  int lineEndWithBreak(String text, int line) =>
      NavigationLineUtils.lineEndWithBreak(lineIndex, text, line);

  int lineContentEnd(String text, int lineStart, int lineEndWithBreak) =>
      NavigationLineUtils.lineContentEnd(text, lineStart, lineEndWithBreak);

  bool isQuoteLineBodyBlank(String text, int line) =>
      _controller._isQuoteLineBodyBlank(text, line);

  bool shouldExitBlockquoteOnArrowDown({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _controller._shouldExitBlockquoteOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
      );

  bool shouldExitBlockquoteOnArrowUp({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _controller._shouldExitBlockquoteOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
      );
}

final List<_EditTransformRule> _kEditTransformRules = <_EditTransformRule>[
  ..._FencePolicy.rules,
  ..._QuotePolicy.rules,
  ..._LinkPolicy.rules,
  ..._HeadingPolicy.rules,
  ..._TablePolicy.rules,
  ..._InlinePolicy.rules,
  ..._ListPolicy.rules,
]..sort((a, b) => a.priority.compareTo(b.priority));

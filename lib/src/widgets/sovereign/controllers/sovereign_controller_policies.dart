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
  GeometryModel get geometry => _controller._geometry;

  TextEditingValue maybeExitFencedCodeOnArrowUp(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeExitFencedCodeOnArrowUp(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        fenceContextForCaret: fenceContextForCaret,
        shouldExitFenceOnArrowUp: shouldExitFenceOnArrowUp,
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
        shouldExitFenceOnArrowDown: shouldExitFenceOnArrowDown,
        trailingBlankTrimStart: trailingBlankTrimStart,
      );

  TextEditingValue maybeExitFencedCodeOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeExitFencedCodeOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        suppressFenceExitOnEnter:
            _controller._suppressFenceExitOnEnterDepth > 0,
        fenceContextForCaret: fenceContextForCaret,
        computeFenceExitOnEnter: computeFenceExitOnEnter,
      );

  TextEditingValue maybeContinueOutsideClosingFenceEof(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeContinueOutsideClosingFenceEof(
        oldValue: oldValue,
        newValue: newValue,
      );

  TextEditingValue maybeNormalizeFencedMultilinePaste(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeNormalizeFencedMultilinePaste(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        skipNormalization: _controller._undoBoundaryDepth > 0,
        isRangeInFenceBody: isRangeInFenceBody,
      );

  TextEditingValue maybeKeepClosingFenceOnOwnLine(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeKeepClosingFenceOnOwnLine(
        oldValue: oldValue,
        newValue: newValue,
      );

  TextEditingValue maybeExpandFencedPairOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeExpandFencedPairOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        isCaretInFenceBody: isCaretInFenceBody,
      );

  TextEditingValue maybeAutoIndentFencedCodeOnEnter(
    TextEditingValue oldValue,
    TextEditingValue newValue, {
    required int? enterCaret,
  }) =>
      _controller._structureTransforms.maybeAutoIndentFencedCodeOnEnter(
        oldValue: oldValue,
        newValue: newValue,
        enterCaret: enterCaret,
        lineIndex: lineIndex,
        fenceContextForCaret: fenceContextForCaret,
        fenceLanguageForBlock: fenceLanguageForBlock,
      );

  TextEditingValue maybeWrapFencedSelectionOnOpenerInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeWrapFencedSelectionOnOpenerInsert(
        oldValue: oldValue,
        newValue: newValue,
        isRangeInFenceBody: isRangeInFenceBody,
      );

  TextEditingValue maybeAutoPairFencedOpenerInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeAutoPairFencedOpenerInsert(
        oldValue: oldValue,
        newValue: newValue,
        isCaretInFenceBody: isCaretInFenceBody,
      );

  TextEditingValue maybeSkipFencedCloserInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeSkipFencedCloserInsert(
        oldValue: oldValue,
        newValue: newValue,
        isCaretInFenceBody: isCaretInFenceBody,
      );

  TextEditingValue maybeOutdentFencedCodeOnCloserInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._structureTransforms.maybeOutdentFencedCodeOnCloserInsert(
        oldValue: oldValue,
        newValue: newValue,
        lineIndex: lineIndex,
        fenceContextForCaret: fenceContextForCaret,
        preferredOutdentUnitForLine: preferredOutdentUnitForLine,
      );

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
      _controller._structureQueries.fenceContextForCaret(
        text: text,
        caret: caret,
        lineIndex: lineIndex,
        geometry: geometry,
        includeUnclosedEof: includeUnclosedEof,
      );

  structure.QuoteContext? quoteContextForLine(String text, int line) =>
      _controller._structureQueries.quoteContextForLine(
        text: text,
        line: line,
        lineIndex: lineIndex,
        geometry: geometry,
      );

  bool isCaretInFenceBody(String text, int caret) =>
      _controller._structureQueries.isCaretInFenceBody(
        text: text,
        caret: caret,
        lineIndex: lineIndex,
        geometry: geometry,
      );

  bool isRangeInFenceBody(String text, int start, int end) =>
      _controller._structureQueries.isRangeInFenceBody(
        text: text,
        start: start,
        end: end,
        lineIndex: lineIndex,
        geometry: geometry,
      );

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
      _controller._structureQueries.isQuoteLineBodyBlank(
        text: text,
        line: line,
        lineIndex: lineIndex,
      );

  bool shouldExitFenceOnArrowDown({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _controller._navigationHelpers.shouldExitFenceOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: lineIndex,
      );

  bool shouldExitFenceOnArrowUp({
    required String text,
    required structure.FenceContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _controller._navigationHelpers.shouldExitFenceOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: lineIndex,
      );

  bool shouldExitBlockquoteOnArrowDown({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _controller._navigationHelpers.shouldExitBlockquoteOnArrowDown(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: lineIndex,
        geometry: geometry,
      );

  bool shouldExitBlockquoteOnArrowUp({
    required String text,
    required structure.QuoteContext context,
    required int fromLine,
    required int toLine,
  }) =>
      _controller._navigationHelpers.shouldExitBlockquoteOnArrowUp(
        text: text,
        context: context,
        fromLine: fromLine,
        toLine: toLine,
        lineIndex: lineIndex,
        geometry: geometry,
      );

  FenceEnterExitResult? computeFenceExitOnEnter({
    required String text,
    required int caret,
    required structure.FenceContext context,
  }) =>
      _controller._navigationHelpers.computeFenceExitOnEnter(
        text: text,
        caret: caret,
        context: context,
        lineIndex: lineIndex,
      );

  String? fenceLanguageForBlock(String text, int blockStartOffset) =>
      _controller._structureQueries.fenceLanguageForBlock(
        text: text,
        blockStartOffset: blockStartOffset,
      );

  String preferredOutdentUnitForLine({
    required String text,
    required MeasuredBlock block,
    required int line,
    required String currentIndent,
  }) =>
      _controller._navigationHelpers.preferredOutdentUnitForLine(
        text: text,
        block: block,
        line: line,
        currentIndent: currentIndent,
        lineIndex: lineIndex,
      );

  int trailingBlankTrimStart(
    String text,
    int openLine,
    int closeLineExclusive,
  ) =>
      _controller._navigationHelpers.trailingBlankTrimStart(
        text: text,
        openLine: openLine,
        closeLineExclusive: closeLineExclusive,
        lineIndex: lineIndex,
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

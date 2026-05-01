part of 'sovereign_controller.dart';

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

class _VerticalArrowEditContext {
  const _VerticalArrowEditContext({
    required this.text,
    required this.oldCaret,
    required this.newCaret,
    required this.oldLine,
    required this.newLine,
  });

  final String text;
  final int oldCaret;
  final int newCaret;
  final int oldLine;
  final int newLine;

  static _VerticalArrowEditContext? detect({
    required TextEditingValue oldValue,
    required TextEditingValue newValue,
    required LineIndex lineIndex,
    required bool movingDown,
  }) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return null;
    }
    if (oldValue.text != newValue.text) return null;

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid) return null;
    if (!oldSel.isCollapsed || !newSel.isCollapsed) return null;

    final text = oldValue.text;
    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret < 0 || oldCaret > text.length) return null;
    if (newCaret < 0 || newCaret > text.length) return null;
    if (oldCaret == newCaret) return null;

    final oldLine = lineIndex.lineAtOffset(oldCaret);
    final newLine = lineIndex.lineAtOffset(newCaret);
    final expectedLine = movingDown ? oldLine + 1 : oldLine - 1;
    if (newLine != expectedLine) return null;

    return _VerticalArrowEditContext(
      text: text,
      oldCaret: oldCaret,
      newCaret: newCaret,
      oldLine: oldLine,
      newLine: newLine,
    );
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
      _controller._maybeExitFencedCodeOnArrowUp(oldValue, newValue);

  TextEditingValue maybeExitFencedCodeOnArrowDown(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeExitFencedCodeOnArrowDown(oldValue, newValue);

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
      _controller._maybeDeleteFencedPairOnBackspace(oldValue, newValue);

  TextEditingValue maybeOutdentFencedCodeOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeOutdentFencedCodeOnBackspace(oldValue, newValue);

  TextEditingValue maybeCollapseEmptyFenceOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeCollapseEmptyFenceOnBackspace(oldValue, newValue);

  TextEditingValue maybeProtectEmptyFenceEntryBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeProtectEmptyFenceEntryBackspace(oldValue, newValue);

  TextEditingValue maybeProtectHiddenFenceBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeProtectHiddenFenceBackspace(oldValue, newValue);

  TextEditingValue maybeReenterInlineWrapperOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) =>
      _controller._maybeReenterInlineWrapperOnBackspace(oldValue, newValue);

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

  int columnAlignedOffsetForLineOrBoundary({
    required String text,
    required int line,
    required int column,
    required bool afterDocument,
  }) =>
      NavigationLineUtils.columnAlignedOffsetForLineOrBoundary(
        text: text,
        lineIndex: lineIndex,
        line: line,
        column: column,
        afterDocument: afterDocument,
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

part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

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
  ) =>
      context.helpers.maybeContinueOrExitBlockquoteOnEnter(
        context.oldValue,
        newValue,
        enterCaret: context.intent.enterCaret,
      );

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

part of 'sovereign_controller.dart';

abstract final class _LinkPolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(
      name: 'whitespace-exit-markdown-link-tail',
      priority: 33,
      apply: _onWhitespaceInsertAtLinkTailBoundary,
    ),
  ];

  static TextEditingValue _onWhitespaceInsertAtLinkTailBoundary(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final oldValue = context.oldValue;
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid ||
        !newSel.isValid ||
        !oldSel.isCollapsed ||
        !newSel.isCollapsed) {
      return newValue;
    }

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (newText.length != oldText.length + 1) {
      return newValue;
    }

    final caret = oldSel.baseOffset;
    if (caret < 0 || caret > oldText.length) {
      return newValue;
    }
    if (newSel.baseOffset != caret + 1) {
      return newValue;
    }
    if (!newText.startsWith(oldText.substring(0, caret)) ||
        newText.substring(caret + 1) != oldText.substring(caret)) {
      return newValue;
    }

    final insertedCodeUnit = newText.codeUnitAt(caret);
    if (insertedCodeUnit != 32 && insertedCodeUnit != 10) {
      return newValue;
    }

    final tailRange = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
      oldText,
      caret,
    );
    if (tailRange == null) {
      return newValue;
    }

    final insertedText = String.fromCharCode(insertedCodeUnit);
    final shifted = oldText.replaceRange(
      tailRange.end,
      tailRange.end,
      insertedText,
    );
    final targetCaret = (tailRange.end + insertedText.length).clamp(
      0,
      shifted.length,
    );

    return newValue.copyWith(
      text: shifted,
      selection: TextSelection.collapsed(offset: targetCaret),
      composing: TextRange.empty,
    );
  }
}

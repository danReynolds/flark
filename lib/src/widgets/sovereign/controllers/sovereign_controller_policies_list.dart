part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

abstract final class _InlinePolicy {
  static const String _inlinePlaceholder = '\u2060';

  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(
      name: 'promote-inline-placeholder-insert',
      priority: 35,
      apply: _promotePlaceholderInsertion,
    ),
    _EditTransformRule(
      name: 'insert-inside-empty-inline-wrapper-tail',
      priority: 36,
      apply: _onInsertAtEmptyWrapperTail,
    ),
    _EditTransformRule(
      name: 'collapse-inline-placeholder-backspace',
      priority: 104,
      apply: _collapsePlaceholderBackspace,
    ),
    _EditTransformRule(
      name: 'preserve-empty-inline-wrapper-on-backspace',
      priority: 104,
      apply: _preserveEmptyWrapperOnBackspace,
    ),
    _EditTransformRule(
      name: 'reenter-inline-wrapper-on-backspace',
      priority: 105,
      apply: (context, value) => context.helpers
          .maybeReenterInlineWrapperOnBackspace(context.oldValue, value),
    ),
  ];

  static TextEditingValue _onInsertAtEmptyWrapperTail(
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
    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret < 0 ||
        oldCaret > oldText.length ||
        newCaret <= oldCaret ||
        newCaret > newText.length) {
      return newValue;
    }

    // Restrict to pure insertion at oldCaret.
    if (!newText.startsWith(oldText.substring(0, oldCaret))) {
      return newValue;
    }
    if (newText.substring(newCaret) != oldText.substring(oldCaret)) {
      return newValue;
    }
    // Never run inline wrapper-tail insertion around fenced code boundaries.
    // Typing right after ``` must remain fence info/body editing, not `x` reflow.
    if (context.helpers.fenceContextForCaret(
          oldText,
          oldCaret,
          includeUnclosedEof: true,
        ) !=
        null) {
      return newValue;
    }
    final inserted = newText.substring(oldCaret, newCaret);
    if (inserted.isEmpty || inserted.contains('\n')) {
      return newValue;
    }

    // Never reinterpret edits on a fence marker line as inline wrapper tail
    // insertion. This prevents EOF edits after ``` from becoming ``x`.
    if (oldText.isNotEmpty && oldCaret > 0) {
      final probeOffset =
          (oldCaret == oldText.length) ? oldCaret - 1 : oldCaret;
      final lineStart = ProjectionRangeUtils.lineStartForOffset(
        oldText,
        probeOffset,
      );
      if (oldText.startsWith('```', lineStart)) {
        return newValue;
      }
    }

    // Prefer longer tokens first to disambiguate `**` vs `*`.
    // Keep this constrained to wrappers that the composer inserts as
    // persistent typing modes. Matching single `*` here misclassifies the
    // trailing `**` in `**x**` as an empty wrapper and pulls new text inside.
    const tokens = <String>['**', '`'];
    for (final token in tokens) {
      final tokenLen = token.length;
      final wrapperStart = oldCaret - (tokenLen * 2);
      if (wrapperStart < 0) continue;
      final contentStart = wrapperStart + tokenLen;
      if (!oldText.startsWith(token, wrapperStart)) continue;
      if (!oldText.startsWith(token, contentStart)) continue;

      final adjustedText = oldText.replaceRange(
        contentStart,
        contentStart,
        inserted,
      );
      final adjustedCaret = (contentStart + inserted.length).clamp(
        0,
        adjustedText.length,
      );
      return newValue.copyWith(
        text: adjustedText,
        selection: TextSelection.collapsed(offset: adjustedCaret),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }

  static TextEditingValue _promotePlaceholderInsertion(
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
    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret <= 0 ||
        oldCaret > oldText.length ||
        newCaret <= oldCaret ||
        newCaret > newText.length) {
      return newValue;
    }

    if (!newText.startsWith(oldText.substring(0, oldCaret))) {
      return newValue;
    }
    if (newText.substring(newCaret) != oldText.substring(oldCaret)) {
      return newValue;
    }
    if (oldText.codeUnitAt(oldCaret - 1) != _inlinePlaceholder.codeUnitAt(0)) {
      return newValue;
    }

    const markerPairs = <(String, String)>[
      ('**', '**'),
      ('*', '*'),
      ('`', '`'),
    ];
    for (final pair in markerPairs) {
      final prefix = pair.$1;
      final suffix = pair.$2;
      final prefixStart = oldCaret - 1 - prefix.length;
      if (prefixStart < 0) continue;
      if (!oldText.startsWith(prefix, prefixStart)) continue;
      if (!oldText.startsWith(suffix, oldCaret)) continue;

      final promoted = newText.replaceRange(oldCaret - 1, oldCaret, '');
      return newValue.copyWith(
        text: promoted,
        selection: TextSelection.collapsed(offset: newCaret - 1),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }

  static TextEditingValue _collapsePlaceholderBackspace(
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
    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret <= 0 ||
        oldCaret > oldText.length ||
        newCaret != oldCaret - 1 ||
        oldText.length != newText.length + 1) {
      return newValue;
    }

    if (oldText.codeUnitAt(oldCaret - 1) != _inlinePlaceholder.codeUnitAt(0)) {
      return newValue;
    }

    if (!newText.startsWith(oldText.substring(0, oldCaret - 1))) {
      return newValue;
    }
    if (newText.substring(oldCaret - 1) != oldText.substring(oldCaret)) {
      return newValue;
    }

    const markerPairs = <(String, String)>[
      ('**', '**'),
      ('*', '*'),
      ('`', '`'),
    ];
    for (final pair in markerPairs) {
      final prefix = pair.$1;
      final suffix = pair.$2;
      final prefixStart = oldCaret - 1 - prefix.length;
      final suffixStart = oldCaret;
      if (prefixStart < 0) continue;
      if (!oldText.startsWith(prefix, prefixStart)) continue;
      if (!oldText.startsWith(suffix, suffixStart)) continue;
      final suffixEnd = suffixStart + suffix.length;
      if (suffixEnd > oldText.length) continue;

      final collapsed = oldText.replaceRange(prefixStart, suffixEnd, '');
      return newValue.copyWith(
        text: collapsed,
        selection: TextSelection.collapsed(offset: prefixStart),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }

  static TextEditingValue _preserveEmptyWrapperOnBackspace(
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
    final oldCaret = oldSel.baseOffset;
    final newCaret = newSel.baseOffset;
    if (oldCaret <= 0 ||
        oldCaret > oldText.length ||
        newCaret != oldCaret - 1 ||
        oldText.length != newText.length + 1) {
      return newValue;
    }

    if (!newText.startsWith(oldText.substring(0, oldCaret - 1))) {
      return newValue;
    }
    if (newText.substring(oldCaret - 1) != oldText.substring(oldCaret)) {
      return newValue;
    }

    final deletedCodeUnit = oldText.codeUnitAt(oldCaret - 1);
    if (deletedCodeUnit == _inlinePlaceholder.codeUnitAt(0)) {
      return newValue;
    }

    const markerPairs = <(String, String)>[
      ('**', '**'),
      ('*', '*'),
      ('_', '_'),
      ('`', '`'),
    ];
    for (final pair in markerPairs) {
      final prefix = pair.$1;
      final suffix = pair.$2;
      final prefixStart = oldCaret - 1 - prefix.length;
      final suffixStart = oldCaret;
      if (prefixStart < 0) continue;
      if (!oldText.startsWith(prefix, prefixStart)) continue;
      if (!oldText.startsWith(suffix, suffixStart)) continue;

      final placeholderInserted = newText.replaceRange(
        newCaret,
        newCaret,
        _inlinePlaceholder,
      );
      return newValue.copyWith(
        text: placeholderInserted,
        selection: TextSelection.collapsed(offset: newCaret + 1),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }
}

abstract final class _ListPolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(name: 'enter-list', priority: 37, apply: _onEnter),
    _EditTransformRule(
      name: 'backspace-list-boundary',
      priority: 107,
      apply: _onBackspaceBoundary,
    ),
  ];

  static TextEditingValue _onEnter(
    _PolicyContext context,
    TextEditingValue newValue,
  ) =>
      context.helpers.maybeContinueOrExitListOnEnter(
        context.oldValue,
        newValue,
        enterCaret: context.intent.enterCaret,
      );

  static TextEditingValue _onBackspaceBoundary(
    _PolicyContext context,
    TextEditingValue newValue,
  ) =>
      context.helpers.maybeHandleListBackspaceBoundary(
        context.oldValue,
        newValue,
      );
}

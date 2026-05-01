part of 'sovereign_controller.dart';

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
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
    final caret = context.intent.enterCaret;
    if (caret == null) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    // List-enter policy should not run while typing inside fenced code.
    if (helpers.fenceContextForCaret(
          oldText,
          caret,
          includeUnclosedEof: true,
        ) !=
        null) {
      return newValue;
    }

    final oldLine = helpers.lineIndex.lineAtOffset(caret);
    final lineStart = helpers.lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = helpers.lineEndWithBreak(oldText, oldLine);
    final lineEnd = helpers.lineContentEnd(
      oldText,
      lineStart,
      lineEndWithBreak,
    );
    final marker = _listMarkerForEditableLine(oldText, lineStart, lineEnd);
    if (marker == null) return newValue;

    // Pressing Enter before list content is a normal split.
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
      // If an earlier policy already changed the inserted line (for example
      // quote continuation), keep that structural transform and avoid
      // unlisting the originating line.
      if (currentCaret != expectedCaret) return newValue;

      // Enter on an empty list item exits list mode by removing marker.
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

  static TextEditingValue _onBackspaceBoundary(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
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

    if (helpers.fenceContextForCaret(
          oldText,
          oldCaret,
          includeUnclosedEof: true,
        ) !=
        null) {
      return newValue;
    }

    final line = helpers.lineIndex.lineAtOffset(oldCaret);
    final lineStart = helpers.lineIndex.offsetAtLine(line);
    final lineEndWithBreak = helpers.lineEndWithBreak(oldText, line);
    final lineEnd = helpers.lineContentEnd(
      oldText,
      lineStart,
      lineEndWithBreak,
    );
    final marker = _listMarkerForEditableLine(oldText, lineStart, lineEnd);
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

  static structure.ListMarkerContext? _listMarkerForEditableLine(
    String text,
    int lineStart,
    int lineEnd,
  ) {
    final direct = MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
      text,
      lineStart,
      lineEnd,
    );
    if (direct != null) return direct;

    var cursor = lineStart;
    while (cursor < lineEnd) {
      final cu = text.codeUnitAt(cursor);
      if (cu == 32 || cu == 9) {
        cursor++;
        continue;
      }
      break;
    }
    if (cursor == lineStart || cursor >= lineEnd) return null;

    return MarkdownLineHelpers.listMarkerForLineAllowingQuotePrefix(
      text,
      cursor,
      lineEnd,
    );
  }
}

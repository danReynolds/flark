part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

extension _FenceBackspacePolicyOps on SovereignController {
  TextEditingValue _maybeReenterInlineWrapperOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }
    if (!oldValue.selection.isValid ||
        !newValue.selection.isValid ||
        !oldValue.selection.isCollapsed ||
        !newValue.selection.isCollapsed) {
      return newValue;
    }

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (oldText.length != newText.length + 1) return newValue;

    final oldCaret = oldValue.selection.baseOffset;
    final newCaret = newValue.selection.baseOffset;
    if (oldCaret != newCaret + 1) return newValue;
    if (newCaret < 0 || newCaret >= oldText.length) return newValue;

    bool isInsideFencedCodeBlock(int caret) {
      for (final block in _geometry.codeBlocks) {
        if (caret >= block.startOffset && caret <= block.endOffset) {
          return true;
        }
      }
      return false;
    }

    if (isInsideFencedCodeBlock(oldCaret)) return newValue;

    // If backspace deletes the opener of an empty inline-code wrapper (`|`),
    // collapse the remaining closer so the user can fully clear the token.
    // Without this, the remaining lone backtick sits to the right of the
    // caret and feels undeletable from backspace.
    final deletedOffset = oldCaret - 1;
    if (deletedOffset >= 0 &&
        deletedOffset + 1 < oldText.length &&
        oldText.codeUnitAt(deletedOffset) == 96 &&
        oldText.codeUnitAt(deletedOffset + 1) == 96 &&
        newCaret == deletedOffset &&
        newCaret < newText.length &&
        newText.codeUnitAt(newCaret) == 96) {
      final collapsed = newText.replaceRange(newCaret, newCaret + 1, '');
      return oldValue.copyWith(
        text: collapsed,
        selection: TextSelection.collapsed(offset: newCaret),
        composing: TextRange.empty,
      );
    }

    ({int prefixStart, int contentStart, int suffixStart})?
        findWrapperEndingAtCaret({
      required String text,
      required int caret,
      required String prefix,
      required String suffix,
    }) {
      final suffixStart = caret - suffix.length;
      if (suffixStart < 0 ||
          suffixStart + suffix.length > text.length ||
          !text.startsWith(suffix, suffixStart)) {
        return null;
      }

      // No room for an opener before the suffix.
      final openerSearchStart = suffixStart - 1;
      if (openerSearchStart < 0) return null;

      var prefixIndex = text.lastIndexOf(prefix, openerSearchStart);
      while (prefixIndex >= 0) {
        final contentStart = prefixIndex + prefix.length;
        if (contentStart > suffixStart) {
          if (prefixIndex == 0) break;
          prefixIndex = text.lastIndexOf(prefix, prefixIndex - 1);
          continue;
        }

        final firstSuffix = text.indexOf(suffix, contentStart);
        if (firstSuffix == suffixStart) {
          return (
            prefixStart: prefixIndex,
            contentStart: contentStart,
            suffixStart: suffixStart,
          );
        }

        if (prefixIndex == 0) break;
        prefixIndex = text.lastIndexOf(prefix, prefixIndex - 1);
      }

      return null;
    }

    const wrappers = <({String prefix, String suffix})>[
      (prefix: '`', suffix: '`'),
      (prefix: '**', suffix: '**'),
      // Keep single-asterisk italic in sync with command-generated markdown.
      (prefix: '*', suffix: '*'),
      (prefix: '_', suffix: '_'),
    ];
    for (final wrapper in wrappers) {
      final found = findWrapperEndingAtCaret(
        text: oldText,
        caret: oldCaret,
        prefix: wrapper.prefix,
        suffix: wrapper.suffix,
      );
      if (found == null) continue;
      if (found.contentStart >= found.suffixStart) continue;

      final deletedInsideSuffix =
          newCaret >= found.suffixStart && newCaret < oldCaret;
      if (!deletedInsideSuffix) continue;

      final deleteOffset = found.suffixStart - 1;
      if (deleteOffset >= found.contentStart) {
        final rewrittenText = oldText.replaceRange(
          deleteOffset,
          deleteOffset + 1,
          '',
        );
        return oldValue.copyWith(
          text: rewrittenText,
          selection: TextSelection.collapsed(offset: found.suffixStart - 1),
          composing: TextRange.empty,
        );
      }

      return oldValue.copyWith(
        selection: TextSelection.collapsed(offset: found.suffixStart),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }

  TextEditingValue _maybeDeleteFencedPairOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    if (!oldSel.isValid || !oldSel.isCollapsed) return newValue;
    final oldText = oldValue.text;
    final newText = newValue.text;
    final caret = oldSel.baseOffset;
    if (caret <= 0 || caret > oldText.length) return newValue;
    if (newText.length != oldText.length - 1) return newValue;

    final deletedOffset = caret - 1;
    if (deletedOffset < 0 || deletedOffset > newText.length) return newValue;
    if (!newText.startsWith(oldText.substring(0, deletedOffset))) {
      return newValue;
    }
    if (newText.substring(deletedOffset) != oldText.substring(caret)) {
      return newValue;
    }
    if (!_isCaretInFenceBody(oldText, caret)) return newValue;

    if (deletedOffset >= oldText.length - 1) return newValue;
    final opener = oldText.codeUnitAt(deletedOffset);
    final closer = FenceEditingUtils.smartPairMap[opener];
    if (closer == null) return newValue;
    if (oldText.codeUnitAt(deletedOffset + 1) != closer) return newValue;
    if (newText.codeUnitAt(deletedOffset) != closer) return newValue;

    final collapsedText = newText.replaceRange(
      deletedOffset,
      deletedOffset + 1,
      '',
    );
    return newValue.copyWith(
      text: collapsedText,
      selection: TextSelection.collapsed(offset: deletedOffset),
      composing: TextRange.empty,
    );
  }

  TextEditingValue _maybeOutdentFencedCodeOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    if (!oldSel.isValid || !oldSel.isCollapsed) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    final caret = oldSel.baseOffset;
    if (caret <= 0 || caret > oldText.length) return newValue;
    if (newText.length != oldText.length - 1) return newValue;

    final deletedOffset = caret - 1;
    if (deletedOffset < 0 || deletedOffset > newText.length) return newValue;
    if (!newText.startsWith(oldText.substring(0, deletedOffset))) {
      return newValue;
    }
    if (newText.substring(deletedOffset) != oldText.substring(caret)) {
      return newValue;
    }

    final line = _lineIndex.lineAtOffset(caret);
    final lineStart = _lineIndex.offsetAtLine(line);
    if (lineStart < 0 || lineStart > caret) return newValue;
    final lineEnd = FencedCodeScanner.endOfLine(oldText, lineStart);
    final lineText = oldText.substring(lineStart, lineEnd);
    final leading = NavigationLineUtils.leadingWhitespacePrefix(lineText);
    if (leading.isEmpty) return newValue;

    final leadingEnd = lineStart + leading.length;
    if (caret != leadingEnd) return newValue;
    if (deletedOffset < lineStart || deletedOffset >= leadingEnd) {
      return newValue;
    }

    final context = _fenceContextForCaret(
      oldText,
      caret,
      includeUnclosedEof: true,
    );
    if (context == null) return newValue;
    if (line <= context.openLine) return newValue;
    if (context.closeLine != null && line >= context.closeLine!) {
      return newValue;
    }

    final unit = _preferredOutdentUnitForLine(
      text: oldText,
      block: context.block,
      line: line,
      currentIndent: leading,
    );
    final reduced = FenceEditingUtils.removeOneIndentUnit(leading, unit);
    if (reduced == leading) return newValue;

    final adjustedText = oldText.replaceRange(lineStart, leadingEnd, reduced);
    final removed = leading.length - reduced.length;
    final adjustedCaret =
        (caret - removed).clamp(0, adjustedText.length).toInt();
    return newValue.copyWith(
      text: adjustedText,
      selection: TextSelection.collapsed(offset: adjustedCaret),
      composing: TextRange.empty,
    );
  }

  TextEditingValue _maybeProtectHiddenFenceBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid) return newValue;
    if (!oldSel.isCollapsed || !newSel.isCollapsed) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (oldText.length != newText.length + 1) return newValue;
    if (oldSel.baseOffset <= 0 || oldSel.baseOffset > oldText.length) {
      return newValue;
    }

    var deletedOffset = 0;
    while (deletedOffset < newText.length &&
        oldText.codeUnitAt(deletedOffset) ==
            newText.codeUnitAt(deletedOffset)) {
      deletedOffset++;
    }
    if (deletedOffset >= oldText.length) return newValue;
    if (oldText.substring(deletedOffset + 1) !=
        newText.substring(deletedOffset)) {
      return newValue;
    }

    if (oldSel.baseOffset != deletedOffset + 1) return newValue;

    TextRange? deletedFenceMarker;
    for (final range in _projectedHiddenRanges) {
      if (deletedOffset >= range.start &&
          deletedOffset < range.end &&
          ProjectionRangeUtils.isFenceMarkerRange(oldText, range)) {
        deletedFenceMarker = range;
        break;
      }
    }
    if (deletedFenceMarker == null) {
      final fencedBlocks = FencedCodeScanner.scan(oldText);
      final fenceMarkers = ProjectionRangeUtils.fencedCodeFenceMarkers(
        oldText,
        fencedBlocks,
      );
      for (final range in fenceMarkers) {
        if (deletedOffset >= range.start &&
            deletedOffset < range.end &&
            ProjectionRangeUtils.isFenceMarkerRange(oldText, range)) {
          deletedFenceMarker = range;
          break;
        }
      }
    }
    if (deletedFenceMarker == null) return newValue;

    final marker = deletedFenceMarker;
    final target = marker.start <= 0 ? 0 : marker.start - 1;
    return oldValue.copyWith(
      selection: TextSelection.collapsed(offset: target),
      composing: TextRange.empty,
    );
  }

  TextEditingValue _maybeCollapseEmptyFenceOnBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid) return newValue;
    if (!oldSel.isCollapsed || !newSel.isCollapsed) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (oldText.length != newText.length + 1) return newValue;
    if (oldSel.baseOffset <= 0 || oldSel.baseOffset > oldText.length) {
      return newValue;
    }

    var deletedOffset = 0;
    while (deletedOffset < newText.length &&
        oldText.codeUnitAt(deletedOffset) ==
            newText.codeUnitAt(deletedOffset)) {
      deletedOffset++;
    }
    if (deletedOffset >= oldText.length) return newValue;
    if (oldText.substring(deletedOffset + 1) !=
        newText.substring(deletedOffset)) {
      return newValue;
    }

    if (oldSel.baseOffset != deletedOffset + 1) return newValue;

    final fencedBlocks = FencedCodeScanner.scan(oldText);
    for (final block in fencedBlocks) {
      if (block.start < 0 ||
          block.end > oldText.length ||
          block.start >= block.end) {
        continue;
      }
      if (deletedOffset < block.start || deletedOffset >= block.end) continue;

      final openLineEnd = FencedCodeScanner.endOfLine(oldText, block.start);
      final closeLineStart = ProjectionRangeUtils.lineStartForOffset(
        oldText,
        block.end - 1,
      );
      final hasClosingFence = closeLineStart != block.start &&
          closeLineStart + 3 <= oldText.length &&
          oldText.startsWith('```', closeLineStart);
      final hasVisibleContent = FenceEditingUtils.fenceHasVisibleContent(
        text: oldText,
        fenceStart: block.start,
        openLineEnd: openLineEnd,
        closeLineStart: hasClosingFence ? closeLineStart : null,
      );
      final isImmediateUnclosedCancel = !hasClosingFence &&
          !hasVisibleContent &&
          oldSel.baseOffset == openLineEnd &&
          deletedOffset == openLineEnd - 1 &&
          openLineEnd == oldText.length;

      final isImmediateClosedCancel = hasClosingFence &&
          !hasVisibleContent &&
          oldText.substring(openLineEnd, closeLineStart) == '\n' &&
          closeLineStart == openLineEnd + 1 &&
          deletedOffset == openLineEnd &&
          (oldSel.baseOffset == openLineEnd ||
              oldSel.baseOffset == closeLineStart);

      if (!isImmediateUnclosedCancel && !isImmediateClosedCancel) continue;

      final removeEnd = hasClosingFence ? block.end : openLineEnd;
      final collapsedText = oldText.replaceRange(block.start, removeEnd, '');
      final collapsedOffset = block.start.clamp(0, collapsedText.length);
      return oldValue.copyWith(
        text: collapsedText,
        selection: TextSelection.collapsed(offset: collapsedOffset),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }

  TextEditingValue _maybeProtectEmptyFenceEntryBackspace(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    final newSel = newValue.selection;
    if (!oldSel.isValid || !newSel.isValid) return newValue;
    if (!oldSel.isCollapsed || !newSel.isCollapsed) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (oldText.length != newText.length + 1) return newValue;
    if (oldSel.baseOffset <= 0 || oldSel.baseOffset > oldText.length) {
      return newValue;
    }

    var deletedOffset = 0;
    while (deletedOffset < newText.length &&
        oldText.codeUnitAt(deletedOffset) ==
            newText.codeUnitAt(deletedOffset)) {
      deletedOffset++;
    }
    if (deletedOffset >= oldText.length) return newValue;
    if (oldText.substring(deletedOffset + 1) !=
        newText.substring(deletedOffset)) {
      return newValue;
    }

    if (oldSel.baseOffset != deletedOffset + 1) return newValue;
    if (oldText.codeUnitAt(deletedOffset) != 10) return newValue;

    bool isUnclosedFenceAtEof(MeasuredBlock b) {
      if (b.endOffset != oldText.length) return false;
      if (b.endOffset <= 0) return true;
      final closeLineStart = ProjectionRangeUtils.lineStartForOffset(
        oldText,
        b.endOffset - 1,
      );
      final hasClosingFence = closeLineStart != b.startOffset &&
          closeLineStart + 3 <= oldText.length &&
          oldText.startsWith('```', closeLineStart);
      return !hasClosingFence;
    }

    for (final block in _geometry.codeBlocks) {
      if (!isUnclosedFenceAtEof(block)) continue;
      if (block.startOffset < 0 || block.startOffset + 3 > oldText.length) {
        continue;
      }
      if (!oldText.startsWith('```', block.startOffset)) continue;

      final openLineEnd = FencedCodeScanner.endOfLine(
        oldText,
        block.startOffset,
      );
      final isOpenLineNewline = deletedOffset == openLineEnd - 1;
      final isAtBodyStart = oldSel.baseOffset == openLineEnd;
      final isBodyEmpty = openLineEnd == oldText.length;
      final hasVisibleContent = FenceEditingUtils.fenceHasVisibleContent(
        text: oldText,
        fenceStart: block.startOffset,
        openLineEnd: openLineEnd,
      );
      if (!isOpenLineNewline ||
          !isAtBodyStart ||
          !isBodyEmpty ||
          hasVisibleContent) {
        continue;
      }

      final collapsedText = oldText.replaceRange(
        block.startOffset,
        openLineEnd,
        '',
      );
      return oldValue.copyWith(
        text: collapsedText,
        selection: TextSelection.collapsed(
          offset: block.startOffset.clamp(0, collapsedText.length),
        ),
        composing: TextRange.empty,
      );
    }

    return newValue;
  }
}

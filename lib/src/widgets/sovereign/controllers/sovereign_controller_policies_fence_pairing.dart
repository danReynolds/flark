part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

extension _FencePairingPolicyOps on SovereignController {
  TextEditingValue _maybeOutdentFencedCodeOnCloserInsert(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }

    final oldSel = oldValue.selection;
    if (!oldSel.isValid || !oldSel.isCollapsed) return newValue;
    final caret = oldSel.baseOffset;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    if (newText.length != oldText.length + 1) return newValue;
    if (caret >= newText.length) return newValue;
    if (newValue.selection.isValid &&
        newValue.selection.isCollapsed &&
        newValue.selection.baseOffset != caret + 1) {
      return newValue;
    }
    if (!newText.startsWith(oldText.substring(0, caret))) return newValue;
    if (newText.substring(caret + 1) != oldText.substring(caret)) {
      return newValue;
    }

    final insertedCu = newText.codeUnitAt(caret);
    if (!FenceEditingUtils.isAutoOutdentCloser(insertedCu)) return newValue;

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

    MeasuredBlock? containing;
    for (final block in _geometry.codeBlocks) {
      final inside = caret >= block.startOffset && caret < block.endOffset;
      final atUnclosedEofEnd =
          caret == block.endOffset && isUnclosedFenceAtEof(block);
      if (inside || atUnclosedEofEnd) {
        containing = block;
        break;
      }
    }
    if (containing == null) return newValue;

    final caretLine = _lineIndex.lineAtOffset(caret);
    final lineStart = _lineIndex.offsetAtLine(caretLine);
    if (lineStart < 0 || lineStart > caret) return newValue;
    final leading = oldText.substring(lineStart, caret);
    if (!NavigationLineUtils.isHorizontalWhitespaceOnly(leading) ||
        leading.isEmpty) {
      return newValue;
    }

    final unit = _preferredOutdentUnitForLine(
      text: oldText,
      block: containing,
      line: caretLine,
      currentIndent: leading,
    );
    final reduced = FenceEditingUtils.removeOneIndentUnit(leading, unit);
    if (reduced == leading) return newValue;

    final adjustedText = newText.replaceRange(lineStart, caret, reduced);
    final removed = leading.length - reduced.length;
    final adjustedCaret =
        (caret + 1 - removed).clamp(0, adjustedText.length).toInt();
    return newValue.copyWith(
      text: adjustedText,
      selection: TextSelection.collapsed(offset: adjustedCaret),
      composing: TextRange.empty,
    );
  }
}

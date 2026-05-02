part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

extension _FenceNavigationPolicyOps on SovereignController {
  TextEditingValue _maybeNormalizeFencedMultilinePaste(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.composing.isValid || newValue.composing.isValid) {
      return newValue;
    }
    if (_undoBoundaryDepth > 0) return newValue;
    if (oldValue.text == newValue.text) return newValue;

    final diff = EditDiffer.diff(
      oldVal: oldValue,
      newVal: newValue,
      nextOpId: 0,
      isSmart: true,
      undoGroupId: 0,
    );
    if (diff.kind != EditOpKind.text) return newValue;
    if (!diff.insertedText.contains('\n')) return newValue;

    final replaced = diff.replacedRange;
    final oldText = oldValue.text;
    final start = replaced.start;
    final end = replaced.end;
    if (!_isRangeInFenceBody(oldText, start, end)) return newValue;

    final line = _lineIndex.lineAtOffset(start);
    final lineStart = _lineIndex.offsetAtLine(line);
    if (lineStart < 0 || lineStart > start) return newValue;
    final beforeInsertion = oldText.substring(lineStart, start);
    if (!NavigationLineUtils.isHorizontalWhitespaceOnly(beforeInsertion)) {
      return newValue;
    }

    final normalizedInserted = FenceEditingUtils.normalizeFencedMultilineInsert(
      insertedText: diff.insertedText,
      baseIndent: beforeInsertion,
    );
    if (normalizedInserted == diff.insertedText) return newValue;

    final adjustedText = oldText.replaceRange(start, end, normalizedInserted);
    final adjustedCaret = (start + normalizedInserted.length).clamp(
      0,
      adjustedText.length,
    );
    return newValue.copyWith(
      text: adjustedText,
      selection: TextSelection.collapsed(offset: adjustedCaret),
      composing: TextRange.empty,
    );
  }

  TextEditingValue _maybeKeepClosingFenceOnOwnLine(
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
    if (caret < 0 || caret > oldText.length) return newValue;
    if (newText.length != oldText.length + 1) return newValue;
    if (caret >= newText.length) return newValue;
    if (!newText.startsWith(oldText.substring(0, caret))) return newValue;
    if (newText.substring(caret + 1) != oldText.substring(caret)) {
      return newValue;
    }
    if (newValue.selection.isValid &&
        newValue.selection.isCollapsed &&
        newValue.selection.baseOffset != caret + 1) {
      return newValue;
    }

    final inserted = newText.codeUnitAt(caret);
    if (inserted == 10) return newValue;

    if (!oldText.startsWith('```', caret)) return newValue;
    if (caret > 0 && oldText.codeUnitAt(caret - 1) != 10) return newValue;

    var isClosingFenceLine = false;
    final fencedBlocks = FencedCodeScanner.scan(oldText);
    for (final block in fencedBlocks) {
      if (block.end <= 0 || block.end > oldText.length) continue;
      final closeLineStart = ProjectionRangeUtils.lineStartForOffset(
        oldText,
        block.end - 1,
      );
      final hasClosingFence = closeLineStart != block.start &&
          closeLineStart + 3 <= oldText.length &&
          oldText.startsWith('```', closeLineStart);
      if (hasClosingFence && closeLineStart == caret) {
        isClosingFenceLine = true;
        break;
      }
    }
    if (!isClosingFenceLine) return newValue;

    final adjustedText = newText.replaceRange(caret + 1, caret + 1, '\n');
    return newValue.copyWith(
      text: adjustedText,
      selection: TextSelection.collapsed(offset: caret + 1),
      composing: TextRange.empty,
    );
  }
}

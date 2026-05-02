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
}

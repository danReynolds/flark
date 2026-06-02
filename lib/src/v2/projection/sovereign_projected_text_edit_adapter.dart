import '../core/core.dart';
import 'sovereign_projection.dart';

final class SovereignProjectedTextEditAdapter {
  const SovereignProjectedTextEditAdapter();

  SovereignTransaction? transactionFromDisplayEdit({
    required String currentMarkdown,
    required SovereignProjection projection,
    required String oldDisplayText,
    required String newDisplayText,
    SovereignSelection? sourceSelectionBefore,
    int? undoGroupId,
    SovereignMapAffinity fallbackInsertionAffinity =
        SovereignMapAffinity.downstream,
  }) {
    if (currentMarkdown.length != projection.textLength) return null;
    if (projection.projectText(currentMarkdown) != oldDisplayText) return null;

    final diff = _DisplayTextDiff.between(oldDisplayText, newDisplayText);
    if (diff == null) return null;

    final sourceRange = _sourceRangeForDiff(
      diff,
      projection: projection,
      sourceSelectionBefore: sourceSelectionBefore,
      fallbackInsertionAffinity: fallbackInsertionAffinity,
    );
    if (sourceRange == null) return null;
    if (sourceRange.start > sourceRange.end ||
        sourceRange.end > currentMarkdown.length) {
      return null;
    }

    return SovereignTransaction.single(
      SovereignSourceOperation.replace(
        replacedRange: sourceRange,
        replacementText: diff.replacementText,
      ),
      selectionBefore: sourceSelectionBefore,
      selectionAfter: SovereignSelection.collapsed(
        sourceRange.start + diff.replacementText.length,
      ),
      metadata: SovereignTransactionMetadata(
        intent: SovereignTransactionIntent.input,
        userEvent: 'input.projected',
        undoGroupId: undoGroupId,
        parseInvalidationRange: sourceRange,
        projectionInvalidationRange: sourceRange,
      ),
    );
  }

  SovereignSourceRange? _sourceRangeForDiff(
    _DisplayTextDiff diff, {
    required SovereignProjection projection,
    required SovereignMapAffinity fallbackInsertionAffinity,
    SovereignSelection? sourceSelectionBefore,
  }) {
    final selectionRange = _matchingSourceSelectionRange(
      sourceSelectionBefore,
      displayStart: diff.oldStart,
      displayEnd: diff.oldEnd,
      projection: projection,
    );
    if (selectionRange != null) return selectionRange;

    if (diff.isInsertion) {
      final sourceOffset = projection.displayToSourceOffset(
        diff.oldStart,
        affinity: fallbackInsertionAffinity,
      );
      return SovereignSourceRange(sourceOffset, sourceOffset);
    }

    final sourceStart = projection.displayToSourceOffset(
      diff.oldStart,
      affinity: SovereignMapAffinity.downstream,
    );
    final sourceEnd = projection.displayToSourceOffset(
      diff.oldEnd,
      affinity: SovereignMapAffinity.upstream,
    );
    if (sourceStart > sourceEnd) return null;
    return SovereignSourceRange(sourceStart, sourceEnd);
  }

  SovereignSourceRange? _matchingSourceSelectionRange(
    SovereignSelection? sourceSelectionBefore, {
    required int displayStart,
    required int displayEnd,
    required SovereignProjection projection,
  }) {
    if (sourceSelectionBefore == null) return null;
    final normalized = SovereignSelection(
      baseOffset: projection.cursorMask.normalize(
        sourceSelectionBefore.start,
        affinity: SovereignMapAffinity.downstream,
      ),
      extentOffset: projection.cursorMask.normalize(
        sourceSelectionBefore.end,
        affinity: SovereignMapAffinity.upstream,
      ),
    );
    if (projection.sourceToDisplayOffset(normalized.start) != displayStart ||
        projection.sourceToDisplayOffset(normalized.end) != displayEnd) {
      return null;
    }
    return SovereignSourceRange(normalized.start, normalized.end);
  }
}

final class _DisplayTextDiff {
  const _DisplayTextDiff({
    required this.oldStart,
    required this.oldEnd,
    required this.replacementText,
  });

  final int oldStart;
  final int oldEnd;
  final String replacementText;

  bool get isInsertion => oldStart == oldEnd && replacementText.isNotEmpty;

  static _DisplayTextDiff? between(String oldText, String newText) {
    if (oldText == newText) return null;

    var prefixLength = 0;
    final sharedPrefixLimit = oldText.length < newText.length
        ? oldText.length
        : newText.length;
    while (prefixLength < sharedPrefixLimit &&
        oldText.codeUnitAt(prefixLength) == newText.codeUnitAt(prefixLength)) {
      prefixLength++;
    }

    var oldSuffix = oldText.length;
    var newSuffix = newText.length;
    while (oldSuffix > prefixLength &&
        newSuffix > prefixLength &&
        oldText.codeUnitAt(oldSuffix - 1) ==
            newText.codeUnitAt(newSuffix - 1)) {
      oldSuffix--;
      newSuffix--;
    }

    return _DisplayTextDiff(
      oldStart: prefixLength,
      oldEnd: oldSuffix,
      replacementText: newText.substring(prefixLength, newSuffix),
    );
  }
}

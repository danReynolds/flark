import '../core/core.dart';
import 'sovereign_projection.dart';

final class FlarkProjectedTextEditAdapter {
  const FlarkProjectedTextEditAdapter();

  FlarkTransaction? transactionFromDisplayEdit({
    required String currentMarkdown,
    required FlarkProjection projection,
    required String oldDisplayText,
    required String newDisplayText,
    FlarkSelection? sourceSelectionBefore,
    int? undoGroupId,
    FlarkMapAffinity fallbackInsertionAffinity = FlarkMapAffinity.downstream,
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

    return FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: sourceRange,
        replacementText: diff.replacementText,
      ),
      selectionBefore: sourceSelectionBefore,
      selectionAfter: FlarkSelection.collapsed(
        sourceRange.start + diff.replacementText.length,
      ),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: 'input.projected',
        undoGroupId: undoGroupId,
        parseInvalidationRange: sourceRange,
        projectionInvalidationRange: sourceRange,
      ),
    );
  }

  FlarkSourceRange? _sourceRangeForDiff(
    _DisplayTextDiff diff, {
    required FlarkProjection projection,
    required FlarkMapAffinity fallbackInsertionAffinity,
    FlarkSelection? sourceSelectionBefore,
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
      return FlarkSourceRange(sourceOffset, sourceOffset);
    }

    final sourceStart = projection.displayToSourceOffset(
      diff.oldStart,
      affinity: FlarkMapAffinity.downstream,
    );
    final sourceEnd = projection.displayToSourceOffset(
      diff.oldEnd,
      affinity: FlarkMapAffinity.upstream,
    );
    if (sourceStart > sourceEnd) return null;
    return FlarkSourceRange(sourceStart, sourceEnd);
  }

  FlarkSourceRange? _matchingSourceSelectionRange(
    FlarkSelection? sourceSelectionBefore, {
    required int displayStart,
    required int displayEnd,
    required FlarkProjection projection,
  }) {
    if (sourceSelectionBefore == null) return null;
    final normalized = FlarkSelection(
      baseOffset: projection.cursorMask.normalize(
        sourceSelectionBefore.start,
        affinity: FlarkMapAffinity.downstream,
      ),
      extentOffset: projection.cursorMask.normalize(
        sourceSelectionBefore.end,
        affinity: FlarkMapAffinity.upstream,
      ),
    );
    if (projection.sourceToDisplayOffset(normalized.start) != displayStart ||
        projection.sourceToDisplayOffset(normalized.end) != displayEnd) {
      return null;
    }
    return FlarkSourceRange(normalized.start, normalized.end);
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

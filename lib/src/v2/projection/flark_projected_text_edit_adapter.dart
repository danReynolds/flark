import '../core/core.dart';
import 'flark_projection.dart';

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
    ({String open, String close})? insertionWrap,
  }) {
    if (currentMarkdown.length != projection.textLength) return null;
    if (projection.projectText(currentMarkdown) != oldDisplayText) return null;

    final diff = _DisplayTextDiff.between(
      oldDisplayText,
      newDisplayText,
      anchor: _displayCaretAnchor(
        projection,
        sourceSelectionBefore,
        oldDisplayLength: oldDisplayText.length,
      ),
    );
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

    final markerExit = _inlineRunMarkerExit(
      diff: diff,
      sourceRange: sourceRange,
      currentMarkdown: currentMarkdown,
      projection: projection,
      sourceSelectionBefore: sourceSelectionBefore,
    );
    if (markerExit != null) return markerExit;

    // A pending ("armed") inline style wraps the typed run: a collapsed
    // insertion becomes `open + text + close` with the caret left inside the
    // run, so continued typing extends it through the normal caret-affinity
    // model. Marker-exit above takes precedence (it returns early).
    //
    // The wrap is skipped when its outer markers would sit flush against an
    // identical marker character already in the source — e.g. arming italic
    // inside `***x***`, where inserting `*y*` next to the existing `***` would
    // merge into `****` and corrupt the run. Falling through to a plain
    // insertion lets caret affinity extend the existing run instead.
    if (insertionWrap != null &&
        sourceRange.isCollapsed &&
        diff.replacementText.isNotEmpty &&
        !_wrapMarkersWouldMerge(
          currentMarkdown,
          sourceRange.start,
          insertionWrap,
        )) {
      final wrappedText =
          '${insertionWrap.open}${diff.replacementText}${insertionWrap.close}';
      return FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: sourceRange,
          replacementText: wrappedText,
        ),
        selectionBefore: sourceSelectionBefore,
        selectionAfter: FlarkSelection.collapsed(
          sourceRange.start +
              insertionWrap.open.length +
              diff.replacementText.length,
        ),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'input.projected.pendingInlineStyle',
          undoGroupId: undoGroupId,
          parseInvalidationRange: sourceRange,
          projectionInvalidationRange: sourceRange,
        ),
      );
    }

    final effectiveRange = diff.replacementText.isEmpty
        ? projection.expandDeletionOverInlineRunMarkers(sourceRange)
        : sourceRange;

    return FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: effectiveRange,
        replacementText: diff.replacementText,
      ),
      selectionBefore: sourceSelectionBefore,
      selectionAfter: FlarkSelection.collapsed(
        effectiveRange.start + diff.replacementText.length,
      ),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: 'input.projected',
        undoGroupId: undoGroupId,
        parseInvalidationRange: effectiveRange,
        projectionInvalidationRange: effectiveRange,
      ),
    );
  }

  /// Whether wrapping a collapsed insertion at [caret] in [currentMarkdown]
  /// would place one of the wrap's outer markers flush against an identical
  /// marker character, merging them into a longer (corrupting) run.
  bool _wrapMarkersWouldMerge(
    String currentMarkdown,
    int caret,
    ({String open, String close}) wrap,
  ) {
    return wrapMarkersWouldMerge(
      currentMarkdown,
      caret,
      open: wrap.open,
      close: wrap.close,
    );
  }

  /// Whether wrapping a collapsed insertion at [caret] in [source] with the
  /// outer markers [open]/[close] would sit flush against an identical marker
  /// character, merging into a longer (corrupting) run.
  ///
  /// Shared with the controller so the toolbar can refuse to arm a style whose
  /// wrap would be dropped at the caret — e.g. arming italic at a bold run's
  /// trailing edge, where `**a*b***` is not representable. Keeping one
  /// predicate keeps the armed-state display honest about what typing will do.
  static bool wrapMarkersWouldMerge(
    String source,
    int caret, {
    required String open,
    required String close,
  }) {
    if (open.isEmpty || close.isEmpty || caret < 0 || caret > source.length) {
      return false;
    }
    final openChar = open.codeUnitAt(0);
    final closeChar = close.codeUnitAt(close.length - 1);
    final before = caret > 0 ? source.codeUnitAt(caret - 1) : null;
    final after = caret < source.length ? source.codeUnitAt(caret) : null;
    return before == openChar || after == closeChar;
  }

  /// Typing a run's own marker character at its inside-end exits the run:
  /// the caret steps past the hidden closing marker instead of a literal
  /// marker character landing inside the styled text.
  FlarkTransaction? _inlineRunMarkerExit({
    required _DisplayTextDiff diff,
    required FlarkSourceRange sourceRange,
    required String currentMarkdown,
    required FlarkProjection projection,
    required FlarkSelection? sourceSelectionBefore,
  }) {
    if (!diff.isInsertion || !sourceRange.isCollapsed) return null;
    final marker = projection.inlineRunClosingMarkerAt(sourceRange.start);
    if (marker == null) return null;
    final markerText = currentMarkdown.substring(marker.start, marker.end);
    if (markerText.isEmpty || !markerText.startsWith(diff.replacementText)) {
      return null;
    }
    return FlarkTransaction(
      operations: const [],
      selectionBefore: sourceSelectionBefore,
      selectionAfter: FlarkSelection.collapsed(marker.end),
      metadata: const FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.selection,
        userEvent: 'input.projected.inlineRunMarkerExit',
        addToHistory: false,
      ),
    );
  }

  /// The old display caret position used to anchor ambiguous diffs, or
  /// null when the prior selection is unknown or not a caret.
  int? _displayCaretAnchor(
    FlarkProjection projection,
    FlarkSelection? sourceSelectionBefore, {
    required int oldDisplayLength,
  }) {
    if (sourceSelectionBefore == null || !sourceSelectionBefore.isCollapsed) {
      return null;
    }
    final offset = sourceSelectionBefore.extentOffset;
    if (offset < 0 || offset > projection.textLength) return null;
    final display = projection.sourceToDisplayOffset(offset);
    if (display < 0 || display > oldDisplayLength) return null;
    return display;
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

  static _DisplayTextDiff? between(
    String oldText,
    String newText, {
    int? anchor,
  }) {
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

    final diff = _DisplayTextDiff(
      oldStart: prefixLength,
      oldEnd: oldSuffix,
      replacementText: newText.substring(prefixLength, newSuffix),
    );
    return _anchoredAtCaret(diff, oldText, newText, anchor) ?? diff;
  }

  /// Re-derives an ambiguous pure insertion or deletion at the old caret.
  ///
  /// Typing a character identical to the character after the caret (for
  /// example a space before an existing space) makes the prefix-greedy
  /// diff slide the edit window past the caret. Across a styled run's
  /// hidden trailing marker that changes meaning: the edit escapes the
  /// run. When the same old → new change is expressible exactly at the
  /// caret, prefer that interpretation.
  static _DisplayTextDiff? _anchoredAtCaret(
    _DisplayTextDiff diff,
    String oldText,
    String newText,
    int? anchor,
  ) {
    if (anchor == null) return null;
    final delta = newText.length - oldText.length;
    if (delta > 0 && diff.isInsertion && diff.oldStart != anchor) {
      // Insertion of `delta` chars at the caret.
      if (anchor < 0 || anchor > oldText.length) return null;
      if (oldText.substring(0, anchor) == newText.substring(0, anchor) &&
          oldText.substring(anchor) == newText.substring(anchor + delta)) {
        return _DisplayTextDiff(
          oldStart: anchor,
          oldEnd: anchor,
          replacementText: newText.substring(anchor, anchor + delta),
        );
      }
      return null;
    }
    if (delta < 0 && diff.replacementText.isEmpty && diff.oldEnd != anchor) {
      // Deletion of `-delta` chars ending at the caret (backspace).
      final start = anchor + delta;
      if (start < 0 || anchor > oldText.length) return null;
      if (oldText.substring(0, start) == newText.substring(0, start) &&
          oldText.substring(anchor) == newText.substring(start)) {
        return _DisplayTextDiff(
          oldStart: start,
          oldEnd: anchor,
          replacementText: '',
        );
      }
    }
    return null;
  }
}

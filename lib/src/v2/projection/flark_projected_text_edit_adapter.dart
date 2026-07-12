import '../core/core.dart';
import '../markdown/inline/flark_inline_delimiter_placement.dart';
import 'flark_projection.dart';

/// The resolved source edit for one display-text change.
///
/// [continuationMarker] is non-null when the edit committed whitespace
/// outside that delimiter cluster to keep the source valid CommonMark; the
/// controller re-arms the cluster's styles so the next styled keystroke
/// re-enters the run instead of starting a sibling.
final class FlarkProjectedEditResolution {
  const FlarkProjectedEditResolution(
    this.transaction, {
    this.continuationMarker,
    this.requestsImmediateParse = false,
    this.authoredMarkers = const [],
  });

  final FlarkTransaction transaction;
  final String? continuationMarker;

  /// Whether the edit created or moved Markdown structure that should render
  /// without waiting for the debounced parse.
  final bool requestsImmediateParse;

  /// Delimiter clusters the edit itself wrote (post-edit source coordinates),
  /// for the controller to hide in the predicted projection immediately —
  /// otherwise the editable resyncs transient raw markers to the platform,
  /// flashing them and cancelling any active IME composition.
  final List<FlarkAuthoredMarker> authoredMarkers;
}

final class FlarkProjectedTextEditAdapter {
  const FlarkProjectedTextEditAdapter();

  /// Backwards-compatible projection of [resolveDisplayEdit] to its
  /// transaction.
  FlarkTransaction? transactionFromDisplayEdit({
    required String currentMarkdown,
    required FlarkProjection projection,
    required String oldDisplayText,
    required String newDisplayText,
    FlarkSelection? sourceSelectionBefore,
    int? newDisplayCaret,
    int? undoGroupId,
    FlarkMapAffinity fallbackInsertionAffinity = FlarkMapAffinity.downstream,
    ({String open, String close})? insertionWrap,
  }) {
    return resolveDisplayEdit(
      currentMarkdown: currentMarkdown,
      projection: projection,
      oldDisplayText: oldDisplayText,
      newDisplayText: newDisplayText,
      sourceSelectionBefore: sourceSelectionBefore,
      newDisplayCaret: newDisplayCaret,
      undoGroupId: undoGroupId,
      fallbackInsertionAffinity: fallbackInsertionAffinity,
      insertionWrap: insertionWrap,
    )?.transaction;
  }

  /// Resolves one display-text change into a source transaction.
  ///
  /// [newDisplayCaret] is the platform's own post-edit collapsed caret, in
  /// the coordinates of [newDisplayText], or null when the platform reported a
  /// range/invalid selection or the caller cannot vouch for it. It only ever
  /// corrects the recomputed *caret* on the plain replacement path (see
  /// [_plainPathSelectionAfter]) for the iOS-autocorrect shape the greedy diff
  /// mis-places; it never changes the resulting document text, and the
  /// inline-marker exit/repair paths keep the carets their placement logic
  /// deliberately computes.
  FlarkProjectedEditResolution? resolveDisplayEdit({
    required String currentMarkdown,
    required FlarkProjection projection,
    required String oldDisplayText,
    required String newDisplayText,
    FlarkSelection? sourceSelectionBefore,
    int? newDisplayCaret,
    int? undoGroupId,
    FlarkMapAffinity fallbackInsertionAffinity = FlarkMapAffinity.downstream,
    ({String open, String close})? insertionWrap,
  }) {
    if (currentMarkdown.length != projection.textLength) return null;
    if (projection.projectText(currentMarkdown) != oldDisplayText) return null;

    final oldDisplayCaret = _displayCaretAnchor(
      projection,
      sourceSelectionBefore,
      oldDisplayLength: oldDisplayText.length,
    );
    final diff = _DisplayTextDiff.between(
      oldDisplayText,
      newDisplayText,
      anchor: oldDisplayCaret,
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
    if (markerExit != null) {
      return FlarkProjectedEditResolution(markerExit);
    }

    // The parser's own runs, not the textual scanner's approximation of them:
    // placement decisions relocate delimiters, and only runs the parser
    // actually recognizes may ever be touched.
    final runs = projection.inlineRunScans(currentMarkdown);

    // A pending ("armed") inline style wraps the typed run through the
    // canonical placement rules: the wrap hugs the text's core, edge
    // whitespace stays outside the delimiters, and typing in a re-entry gap
    // extends the run. Marker-exit above takes precedence (it returns early).
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
      final placement = FlarkInlineDelimiterPlacement.armedWrap(
        source: currentMarkdown,
        caret: sourceRange.start,
        text: diff.replacementText,
        open: insertionWrap.open,
        close: insertionWrap.close,
        // Inline code spans may legally hug whitespace; every other armed
        // style must keep whitespace outside its delimiters.
        edgeSensitive: !insertionWrap.open.contains('`'),
        runs: runs,
      );
      return _resolutionFromPlacement(
        placement,
        sourceSelectionBefore: sourceSelectionBefore,
        undoGroupId: undoGroupId,
        userEvent: 'input.projected.pendingInlineStyle',
      );
    }

    // An edit inside a flanking-valid run's content whose plain application
    // would strand the run's delimiters against whitespace (typing a space at
    // the trailing edge, deleting the last word before the close, replacing a
    // selection with whitespace) relocates the delimiters instead, so the
    // source never carries markers CommonMark would refuse.
    final repair = FlarkInlineDelimiterPlacement.contentEditRepair(
      source: currentMarkdown,
      start: sourceRange.start,
      end: sourceRange.end,
      text: diff.replacementText,
      runs: runs,
    );
    if (repair != null) {
      return _resolutionFromPlacement(
        repair,
        sourceSelectionBefore: sourceSelectionBefore,
        undoGroupId: undoGroupId,
        userEvent: 'input.projected.inlineEdgeRepair',
      );
    }

    // An edit whose range covers one half of a hidden marker pair (a
    // selection reaching across a run's edge) would orphan the surviving
    // half as literal text; a deletion consuming the gap between two runs
    // would fuse their delimiters. Both rebalance here. Code spans are
    // included for the crossing repair only — an orphaned backtick turns
    // the rest of the document into a code span — while the joining merge
    // stays emphasis-family. Edits fully inside one run's content never
    // reach this (contentEditRepair's territory), and edits covering a
    // whole pair fall through to the expansion/plain handling below.
    // Marker-crossing needs a range spanning a run edge and joining needs a
    // gap-consuming deletion, so a plain collapsed insertion (the hot typing
    // path) can match neither. Skip both — and the extra code-span scan
    // `markerCrossingRepair` requires — on that path.
    final balanceApplies =
        !sourceRange.isCollapsed || diff.replacementText.isEmpty;
    final balance = !balanceApplies
        ? null
        : FlarkInlineDelimiterPlacement.markerCrossingRepair(
                source: currentMarkdown,
                start: sourceRange.start,
                end: sourceRange.end,
                text: diff.replacementText,
                runs: projection.inlineRunScans(
                  currentMarkdown,
                  includeCodeSpans: true,
                ),
              ) ??
              FlarkInlineDelimiterPlacement.joiningDeletionRepair(
                source: currentMarkdown,
                start: sourceRange.start,
                end: sourceRange.end,
                text: diff.replacementText,
                runs: runs,
              );
    if (balance != null) {
      return _resolutionFromPlacement(
        balance,
        sourceSelectionBefore: sourceSelectionBefore,
        undoGroupId: undoGroupId,
        userEvent: 'input.projected.inlineMarkerBalanceRepair',
      );
    }

    final effectiveRange = diff.replacementText.isEmpty
        ? projection.expandDeletionOverInlineRunMarkers(sourceRange)
        : sourceRange;

    return FlarkProjectedEditResolution(
      FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: effectiveRange,
          replacementText: diff.replacementText,
        ),
        selectionBefore: sourceSelectionBefore,
        selectionAfter: _plainPathSelectionAfter(
          projection: projection,
          diff: diff,
          effectiveRange: effectiveRange,
          oldDisplayLength: oldDisplayText.length,
          oldDisplayCaret: oldDisplayCaret,
          newDisplayLength: newDisplayText.length,
          newDisplayCaret: newDisplayCaret,
        ),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'input.projected',
          undoGroupId: undoGroupId,
          parseInvalidationRange: effectiveRange,
          projectionInvalidationRange: effectiveRange,
        ),
      ),
    );
  }

  /// The post-edit caret for the plain replacement path.
  ///
  /// Defaults to the end of the replacement — where a caret that simply
  /// follows the inserted text lands. When the platform reported a post-edit
  /// caret [newDisplayCaret] that sits *beyond* the greedy diff's new end, in
  /// the shared suffix the diff trimmed, honor it instead: that is the iOS
  /// autocorrect shape (`dont` → `don't` keeps the trailing `t`; a retroactive
  /// fix keeps the whole tail) whose greedy caret would otherwise land
  /// mid-word. Only the caret moves — the source edit is unchanged, so every
  /// inline-marker path upstream still sees the greedy diff — and the shared
  /// suffix is untouched by the edit, so its old display offset maps through
  /// the current projection and shifts by the edit's source-length delta.
  ///
  /// The correction is gated on the caret tracking the edit: the platform
  /// caret must have moved from the old caret by exactly the text-length
  /// change (`newLen - oldLen`), which every "type and the caret follows" edit
  /// — autocorrect included — satisfies. A whole-text replacement that parks
  /// the caret at the end regardless (what `enterText` and select-all edits
  /// deliver) fails this and keeps the default, so a single character typed
  /// mid-text is never mistaken for a suffix-spanning correction and the
  /// marker exit/re-entry carets are never fought.
  static FlarkSelection _plainPathSelectionAfter({
    required FlarkProjection projection,
    required _DisplayTextDiff diff,
    required FlarkSourceRange effectiveRange,
    required int oldDisplayLength,
    required int? oldDisplayCaret,
    required int newDisplayLength,
    required int? newDisplayCaret,
  }) {
    final defaultCaret = effectiveRange.start + diff.replacementText.length;
    final caret = newDisplayCaret;
    if (caret == null ||
        oldDisplayCaret == null ||
        caret <= diff.newReplacementEnd ||
        caret > newDisplayLength) {
      return FlarkSelection.collapsed(defaultCaret);
    }
    // The caret must track the edit: a "type and the caret follows" change
    // moves it by exactly the text-length delta. A whole-text replace that
    // pins the caret to the end (enterText, select-all) does not, and must
    // not be read as a suffix-spanning correction.
    if (caret - oldDisplayCaret != newDisplayLength - oldDisplayLength) {
      return FlarkSelection.collapsed(defaultCaret);
    }
    // The caret's offset into the trimmed shared suffix, lifted back to the
    // old display (where the suffix began at the diff's old end).
    final oldSuffixDisplayOffset = diff.oldEnd + (caret - diff.newReplacementEnd);
    if (oldSuffixDisplayOffset > projection.displayLength) {
      return FlarkSelection.collapsed(defaultCaret);
    }
    final sourceLengthDelta =
        diff.replacementText.length - (effectiveRange.end - effectiveRange.start);
    return FlarkSelection.collapsed(
      projection.displayToSourceOffset(oldSuffixDisplayOffset) +
          sourceLengthDelta,
    );
  }

  /// Wraps a placement [edit] into its resolution: the same transaction,
  /// continuation marker, immediate-parse request, and authored markers used by
  /// every inline-placement branch (armed wrap, content-edit repair, and
  /// marker-balance repair), which differ only by [edit] and [userEvent].
  static FlarkProjectedEditResolution _resolutionFromPlacement(
    FlarkInlinePlacementEdit edit, {
    required FlarkSelection? sourceSelectionBefore,
    required int? undoGroupId,
    required String userEvent,
  }) {
    return FlarkProjectedEditResolution(
      _placementTransaction(
        edit,
        sourceSelectionBefore: sourceSelectionBefore,
        undoGroupId: undoGroupId,
        userEvent: userEvent,
      ),
      continuationMarker: edit.continuationMarker,
      requestsImmediateParse: true,
      authoredMarkers: edit.authoredMarkers,
    );
  }

  static FlarkTransaction _placementTransaction(
    FlarkInlinePlacementEdit placement, {
    required FlarkSelection? sourceSelectionBefore,
    required int? undoGroupId,
    required String userEvent,
  }) {
    return FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: placement.range,
        replacementText: placement.replacement,
      ),
      selectionBefore: sourceSelectionBefore,
      selectionAfter: FlarkSelection.collapsed(placement.caretAfter),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: userEvent,
        undoGroupId: undoGroupId,
        parseInvalidationRange: placement.range,
        projectionInvalidationRange: placement.range,
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

  /// The greedy diff's new-text offset just past its replacement — where a
  /// caret that merely follows the inserted text sits. The trimmed shared
  /// suffix, if any, begins here.
  int get newReplacementEnd => oldStart + replacementText.length;

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

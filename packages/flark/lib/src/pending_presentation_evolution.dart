import 'document.dart';
import 'models.dart';
import 'pending_presentation.dart';
import 'presentation.dart';
import 'projection_continuity.dart';

/// Resolves the parser-authored structural consequence against one captured
/// bounded presentation.
///
/// A negative frontend ordinal may stand for a physical line inside a larger
/// parser row. In that case the exact receipt splice can recover ownership
/// only when exactly one captured Core row strictly contains it.
FlarkCoreCommittedPresentationTransitionV1?
resolvePendingPresentationTransition({
  required FlarkCoreEditIntentReceiptV1 receipt,
  required FlarkPendingPresentationSnapshot pendingPresentation,
  required int? activeOrdinal,
  required List<FlarkCorePresentationRow> priorRows,
}) {
  final structuralIndex = pendingPresentation.structuralIndexForRange(
    receipt.baseUtf16Start,
    receipt.baseUtf16End,
  );
  final structural = structuralIndex < 0
      ? null
      : pendingPresentation.structuralSurfaces[structuralIndex].surface;
  var resolvedOrdinal = structural?.rowOrdinal ?? activeOrdinal;
  var activeIndex = resolvedOrdinal == null
      ? -1
      : priorRows.indexWhere((row) => row.ordinal == resolvedOrdinal);
  if (structural == null && activeIndex < 0) {
    final containing = <int>[];
    for (var index = 0; index < priorRows.length; index += 1) {
      final range = priorRows[index].sourceUtf16;
      if (range.start < receipt.baseUtf16Start &&
          receipt.baseUtf16End < range.end) {
        containing.add(index);
      }
    }
    if (containing.length == 1) {
      activeIndex = containing.single;
      resolvedOrdinal = priorRows[activeIndex].ordinal;
    }
  }

  FlarkCorePresentationRow? rowAt(int index) =>
      index < 0 || index >= priorRows.length ? null : priorRows[index];

  return resolveCommittedPresentationTransitionV1(
    receipt: receipt,
    priorActiveOrdinal: resolvedOrdinal,
    activeRow: structural?.presentation ?? rowAt(activeIndex),
    precedingRow: rowAt(activeIndex - 1),
    priorGapPending: pendingPresentation.paragraphGap != null,
    activeRowTransitional: structural != null,
  );
}

/// Retains the nonvisual command boundary owned by a certified structural
/// successor after its temporary visual surfaces are superseded.
FlarkPendingCaretBoundary? caretBoundaryForStructuralSurfaces(
  Iterable<FlarkPendingStructuralSurface> states,
) {
  FlarkCoreCommittedPresentationSurfaceV1? previous;
  for (final state in states) {
    final surface = state.surface;
    final previousIsSeparator =
        previous?.role ==
            FlarkCoreCommittedPresentationSurfaceRole.blockSeparator ||
        previous?.role ==
            FlarkCoreCommittedPresentationSurfaceRole.visibleBlankSeparator;
    if (surface.role ==
            FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor &&
        previous != null &&
        (surface.presentation.text.isEmpty || previousIsSeparator)) {
      return FlarkPendingCaretBoundary(
        rowOrdinal: surface.rowOrdinal,
        rowEndUtf16: previousIsSeparator
            ? previous.sourceUtf16.start
            : previous.sourceUtf16.end,
        authorizedContentUtf16: surface.sourceUtf16,
        projectionEditCells: surface.projectionEditCells,
      );
    }
    previous = surface;
  }
  return null;
}

/// Whether one certified viewport has replaced the parser authority carried
/// by the current pending dependency presentation.
///
/// This fails closed for incomplete bounded plans, uncovered result ranges,
/// unavailable inline facts, and older revisions. A frontend may retire the
/// pending dependency only when this returns true.
bool certifiedViewportSupersedesPendingDependency({
  required FlarkViewport viewport,
  required FlarkPendingPresentationSnapshot pendingPresentation,
}) {
  final continuity = pendingPresentation.dependency;
  if (continuity == null || !viewport.isCertified) return false;
  if (viewport.revision < continuity.resultRevision) return false;
  final authorized = continuity.affectedUtf16;
  if (continuity.removesOwnerRow) {
    return viewport.coveredUtf16.start <= authorized.start &&
        authorized.end <= viewport.coveredUtf16.end;
  }
  if (viewport.rows.isEmpty) return false;
  if (continuity.authority
      case final FlarkBoundedPendingPresentationPlanReceipt plan) {
    if (plan.prefixLength < plan.plan.sequence.length) return false;
    if (viewport.coveredUtf16.start > authorized.start ||
        authorized.end > viewport.coveredUtf16.end) {
      return false;
    }
    final coveringRows = viewport.rows.where(
      (row) =>
          row.sourceUtf16.start < authorized.end &&
          authorized.start < row.sourceUtf16.end,
    );
    return coveringRows.isNotEmpty &&
        coveringRows.every((row) => row.inlineFacts != null);
  }
  for (final row in viewport.rows) {
    final source = _exactViewportRowRange(row);
    if (source.start > authorized.start || authorized.end > source.end) {
      continue;
    }
    if (row.kind != continuity.presentation.kind ||
        row.projectionSegments != null) {
      return true;
    }
    final inlineFacts = row.inlineFacts;
    if (inlineFacts == null) return false;
    if (continuity.presentsExactIsland || inlineFacts.isNotEmpty) return true;
    return continuity.presentation.runs.any(
      (run) =>
          run.styles.isNotEmpty &&
          run.sourceUtf16Start < authorized.end &&
          authorized.start < run.sourceUtf16End,
    );
  }
  return false;
}

FlarkSourceRange _exactViewportRowRange(FlarkViewportRow row) {
  final prefix = row.listItem?.prefixUtf16 ?? row.blockQuote?.prefixUtf16;
  return prefix == null
      ? row.sourceUtf16
      : FlarkSourceRange(prefix.start, row.sourceUtf16.end);
}

/// Binds one parser-authorized edit to its first immutable pending
/// presentation.
///
/// The frontend supplies only the bounded source and the already projected
/// base row. Markdown classification and edit permission remain in
/// [authority]. A null result means the frontend must wait for fresh parser
/// certification.
FlarkPendingDependencyPresentation? bindPendingDependencyPresentation({
  required int rowOrdinal,
  required FlarkCorePresentationRow base,
  required FlarkPendingDependencyAuthority authority,
  required String visibleSource,
  required int visibleUtf16Start,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (authority is FlarkBoundedPendingPresentationPlanReceipt) {
    final source = _visibleSourceAfterSplice(
      visibleSource: visibleSource,
      visibleUtf16Start: visibleUtf16Start,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
      replacement: replacement,
    );
    if (source == null) return null;
    return materializeBoundedPendingPresentationPlan(
      authority: authority,
      rowOrdinal: rowOrdinal,
      visibleSource: source,
      visibleUtf16Start: visibleUtf16Start,
    );
  }
  final presentation = advancePendingPresentationRow(
    presentation: base,
    authority: authority,
    visibleSource: visibleSource,
    visibleUtf16Start: visibleUtf16Start,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    replacement: replacement,
  );
  if (presentation == null) return null;
  return FlarkPendingDependencyPresentation(
    rowOrdinal: rowOrdinal,
    authority: authority,
    removesOwnerRow:
        authority is FlarkProjectionEditCellReceipt &&
        authority.resultBlockShell?.kind ==
            FlarkProjectionResultBlockKind.removed,
    presentation: presentation,
  );
}

/// Advances an existing pending dependency through one parser-authorized edit.
FlarkPendingDependencyPresentation? advancePendingDependencyPresentation({
  required FlarkPendingDependencyPresentation current,
  required FlarkPendingDependencyAuthority authority,
  required String visibleSource,
  required int visibleUtf16Start,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (authority is FlarkBoundedPendingPresentationPlanReceipt) {
    final source = _visibleSourceAfterSplice(
      visibleSource: visibleSource,
      visibleUtf16Start: visibleUtf16Start,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
      replacement: replacement,
    );
    if (source == null) return null;
    return materializeBoundedPendingPresentationPlan(
      authority: authority,
      rowOrdinal: current.rowOrdinal,
      visibleSource: source,
      visibleUtf16Start: visibleUtf16Start,
    );
  }
  final presentation = advancePendingPresentationRow(
    presentation: current.presentation,
    authority: authority,
    visibleSource: visibleSource,
    visibleUtf16Start: visibleUtf16Start,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    replacement: replacement,
  );
  if (presentation == null) return null;
  return FlarkPendingDependencyPresentation(
    rowOrdinal: current.rowOrdinal,
    authority: authority,
    presentation: presentation,
  );
}

/// Evolves one framework-neutral presentation row through an exact edit.
///
/// This function performs no Markdown recognition. It can only transform the
/// closure and block shell already carried by [authority].
FlarkCorePresentationRow? advancePendingPresentationRow({
  required FlarkCorePresentationRow presentation,
  required FlarkPendingDependencyAuthority authority,
  required String visibleSource,
  required int visibleUtf16Start,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) => switch (authority) {
  FlarkProjectionContinuityReceipt() => _advanceContinuityRow(
    presentation: presentation,
    authorizedContent: authority.authorizedContentUtf16,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    replacement: replacement,
  ),
  FlarkProjectionEditCellReceipt() => _advanceEditCellRow(
    presentation: presentation,
    receipt: authority,
    visibleSource: visibleSource,
    visibleUtf16Start: visibleUtf16Start,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    replacement: replacement,
  ),
  FlarkBoundedPendingPresentationPlanReceipt() => null,
};

String? _visibleSourceAfterSplice({
  required String visibleSource,
  required int visibleUtf16Start,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  final localStart = startUtf16 - visibleUtf16Start;
  final localEnd = endUtf16 - visibleUtf16Start;
  if (localStart < 0 ||
      localStart > localEnd ||
      localEnd > visibleSource.length) {
    return null;
  }
  return visibleSource.replaceRange(localStart, localEnd, replacement);
}

FlarkCorePresentationRow? _advanceContinuityRow({
  required FlarkCorePresentationRow presentation,
  required FlarkSourceRange authorizedContent,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  final delta = replacement.length - (endUtf16 - startUtf16);
  final baseAuthorizedContent = FlarkSourceRange(
    authorizedContent.start,
    authorizedContent.end - delta,
  );
  var target = -1;
  for (var index = 0; index < presentation.runs.length; index += 1) {
    final run = presentation.runs[index];
    final insertionInside =
        startUtf16 == endUtf16 &&
        startUtf16 >= run.sourceUtf16Start &&
        startUtf16 <= run.sourceUtf16End;
    final replacementInside =
        startUtf16 < endUtf16 &&
        startUtf16 >= run.sourceUtf16Start &&
        endUtf16 <= run.sourceUtf16End;
    final runInsideAuthority =
        baseAuthorizedContent.start <= run.sourceUtf16Start &&
        run.sourceUtf16End <= baseAuthorizedContent.end;
    if (run.sourceExact &&
        runInsideAuthority &&
        (insertionInside || replacementInside)) {
      target = index;
      break;
    }
  }

  if (target < 0) {
    if (startUtf16 != endUtf16 ||
        startUtf16 < baseAuthorizedContent.start ||
        startUtf16 > baseAuthorizedContent.end) {
      return null;
    }
    var insertionIndex = presentation.runs.length;
    for (var index = 0; index < presentation.runs.length; index += 1) {
      if (presentation.runs[index].sourceUtf16Start >= startUtf16) {
        insertionIndex = index;
        break;
      }
    }
    final runs = List<FlarkCorePresentationRun>.unmodifiable([
      ...presentation.runs.take(insertionIndex),
      FlarkCorePresentationRun(
        text: replacement,
        sourceUtf16Start: startUtf16,
        sourceUtf16End: startUtf16 + replacement.length,
        sourceExact: true,
        styles: const {},
      ),
      ...presentation.runs
          .skip(insertionIndex)
          .map((run) => _shiftRun(run, delta)),
    ]);
    return _copyRowWithRuns(
      presentation,
      sourceUtf16: _shiftRowEnd(presentation.sourceUtf16, delta),
      text: runs.map((run) => run.text).join(),
      runs: runs,
    );
  }

  final runs = <FlarkCorePresentationRun>[];
  for (var index = 0; index < presentation.runs.length; index += 1) {
    final run = presentation.runs[index];
    if (index < target) {
      runs.add(run);
    } else if (index == target) {
      final localStart = startUtf16 - run.sourceUtf16Start;
      final localEnd = endUtf16 - run.sourceUtf16Start;
      runs.add(
        FlarkCorePresentationRun(
          text: run.text.replaceRange(localStart, localEnd, replacement),
          sourceUtf16Start: run.sourceUtf16Start,
          sourceUtf16End: run.sourceUtf16End + delta,
          sourceExact: true,
          styles: run.styles,
        ),
      );
    } else {
      runs.add(_shiftRun(run, delta));
    }
  }
  final immutableRuns = List<FlarkCorePresentationRun>.unmodifiable(runs);
  return _copyRowWithRuns(
    presentation,
    sourceUtf16: _shiftRowEnd(presentation.sourceUtf16, delta),
    text: immutableRuns.map((run) => run.text).join(),
    runs: immutableRuns,
  );
}

FlarkCorePresentationRow? _advanceEditCellRow({
  required FlarkCorePresentationRow presentation,
  required FlarkProjectionEditCellReceipt receipt,
  required String visibleSource,
  required int visibleUtf16Start,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  final resultShell = receipt.resultBlockShell;
  if ((!receipt.retainBlockShell && resultShell == null) ||
      !receipt.presentClosureExact) {
    return null;
  }
  final base = receipt.baseAffectedUtf16;
  final result = receipt.affectedUtf16;
  final baseText = _trySliceVisible(
    visibleSource: visibleSource,
    visibleUtf16Start: visibleUtf16Start,
    startUtf16: base.start,
    endUtf16: base.end,
  );
  if (baseText == null) return null;
  final localStart = startUtf16 - base.start;
  final localEnd = endUtf16 - base.start;
  if (localStart < 0 || localStart > localEnd || localEnd > baseText.length) {
    return null;
  }
  final resultText = baseText.replaceRange(localStart, localEnd, replacement);
  if (resultText.length != result.length) return null;
  final delta = result.length - base.length;
  final outputSource = _shiftRowEnd(presentation.sourceUtf16, delta);
  final before = <FlarkCorePresentationRun>[];
  final after = <FlarkCorePresentationRun>[];
  for (final run in presentation.runs) {
    if (base.length == 0 &&
        run.sourceUtf16Start == base.start &&
        run.sourceUtf16End == base.end) {
      continue;
    }
    if (run.sourceUtf16End <= base.start) {
      before.add(run);
      continue;
    }
    if (run.sourceUtf16Start >= base.end) {
      after.add(_shiftRun(run, delta));
      continue;
    }
    if (run.sourceUtf16Start < base.start || run.sourceUtf16End > base.end) {
      if (!run.sourceExact || run.styles.isNotEmpty) return null;
      if (run.sourceUtf16Start < base.start) {
        final prefix = _trySliceVisible(
          visibleSource: visibleSource,
          visibleUtf16Start: visibleUtf16Start,
          startUtf16: run.sourceUtf16Start,
          endUtf16: base.start,
        );
        if (prefix == null ||
            prefix.length != base.start - run.sourceUtf16Start) {
          return null;
        }
        before.add(
          FlarkCorePresentationRun(
            text: prefix,
            sourceUtf16Start: run.sourceUtf16Start,
            sourceUtf16End: base.start,
            sourceExact: true,
            styles: const {},
          ),
        );
      }
      if (run.sourceUtf16End > base.end) {
        final suffix = _trySliceVisible(
          visibleSource: visibleSource,
          visibleUtf16Start: visibleUtf16Start,
          startUtf16: base.end,
          endUtf16: run.sourceUtf16End,
        );
        if (suffix == null || suffix.length != run.sourceUtf16End - base.end) {
          return null;
        }
        after.add(
          FlarkCorePresentationRun(
            text: suffix,
            sourceUtf16Start: result.end,
            sourceUtf16End: run.sourceUtf16End + delta,
            sourceExact: true,
            styles: const {},
          ),
        );
      }
    }
  }
  if (!receipt.retainOutsideClosure &&
      (before.isNotEmpty || after.isNotEmpty)) {
    return null;
  }

  if (resultShell != null) {
    if (before.isNotEmpty ||
        after.isNotEmpty ||
        resultShell.prefixUtf16Length > resultText.length) {
      return null;
    }
    if (resultShell.kind == FlarkProjectionResultBlockKind.removed) {
      if (resultText.isNotEmpty || result.length != 0) return null;
      return FlarkCorePresentationRow(
        sourceUtf16: outputSource,
        leadingText: '',
        text: '',
        globalUtf16Start: result.start,
        kind: 0,
        headingLevel: null,
        blockQuoteDepth: null,
        codeBlock: null,
        thematicBreak: false,
        listItem: false,
        ordinal: presentation.ordinal,
        runs: const [],
      );
    }
    final contentStart = result.start + resultShell.prefixUtf16Length;
    final content = resultText.substring(resultShell.prefixUtf16Length);
    final block = switch (resultShell.kind) {
      FlarkProjectionResultBlockKind.plain => (
        leading: '',
        kind: 5,
        heading: null,
        quote: null,
        list: false,
      ),
      FlarkProjectionResultBlockKind.atxHeading => (
        leading: '',
        kind: 12,
        heading: resultShell.parameter,
        quote: null,
        list: false,
      ),
      FlarkProjectionResultBlockKind.blockQuote => (
        leading: List<String>.filled(resultShell.parameter, '│ ').join(),
        kind: 5,
        heading: null,
        quote: resultShell.parameter,
        list: false,
      ),
      FlarkProjectionResultBlockKind.listItem => (
        leading: resultText.substring(0, resultShell.prefixUtf16Length),
        kind: 5,
        heading: null,
        quote: null,
        list: true,
      ),
      FlarkProjectionResultBlockKind.removed => throw StateError(
        'removed result shell was handled before block construction',
      ),
    };
    final runs = List<FlarkCorePresentationRun>.unmodifiable([
      FlarkCorePresentationRun(
        text: content,
        sourceUtf16Start: contentStart,
        sourceUtf16End: result.end,
        sourceExact: true,
        styles: const {},
      ),
    ]);
    return FlarkCorePresentationRow(
      sourceUtf16: outputSource,
      leadingText: block.leading,
      text: content,
      globalUtf16Start: contentStart,
      kind: block.kind,
      headingLevel: block.heading,
      blockQuoteDepth: block.quote,
      codeBlock: null,
      thematicBreak: false,
      listItem: block.list,
      ordinal: presentation.ordinal,
      runs: runs,
    );
  }

  final runs = List<FlarkCorePresentationRun>.unmodifiable([
    ...before,
    FlarkCorePresentationRun(
      text: resultText,
      sourceUtf16Start: result.start,
      sourceUtf16End: result.end,
      sourceExact: true,
      styles: const {},
    ),
    ...after,
  ]);
  return _copyRowWithRuns(
    presentation,
    sourceUtf16: outputSource,
    text: runs.map((run) => run.text).join(),
    runs: runs,
  );
}

FlarkCorePresentationRun _shiftRun(FlarkCorePresentationRun run, int delta) =>
    FlarkCorePresentationRun(
      text: run.text,
      sourceUtf16Start: run.sourceUtf16Start + delta,
      sourceUtf16End: run.sourceUtf16End + delta,
      sourceExact: run.sourceExact,
      styles: run.styles,
    );

FlarkSourceRange _shiftRowEnd(FlarkSourceRange source, int delta) =>
    FlarkSourceRange(source.start, source.end + delta);

String? _trySliceVisible({
  required String visibleSource,
  required int visibleUtf16Start,
  required int startUtf16,
  required int endUtf16,
}) {
  final start = startUtf16 - visibleUtf16Start;
  final end = endUtf16 - visibleUtf16Start;
  if (start < 0 || start > end || end > visibleSource.length) return null;
  return visibleSource.substring(start, end);
}

FlarkCorePresentationRow _copyRowWithRuns(
  FlarkCorePresentationRow source, {
  required FlarkSourceRange sourceUtf16,
  required String text,
  required List<FlarkCorePresentationRun> runs,
}) => FlarkCorePresentationRow(
  sourceUtf16: sourceUtf16,
  leadingText: source.leadingText,
  text: text,
  globalUtf16Start: source.globalUtf16Start,
  kind: source.kind,
  headingLevel: source.headingLevel,
  blockQuoteDepth: source.blockQuoteDepth,
  codeBlock: source.codeBlock,
  thematicBreak: source.thematicBreak,
  listItem: source.listItem,
  ordinal: source.ordinal,
  runs: runs,
);

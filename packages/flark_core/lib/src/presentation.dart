import 'document.dart';
import 'models.dart';

/// Framework-neutral style roles published to a Dart presentation adapter.
///
/// A frontend chooses fonts, colors, and widgets for these roles. It must not
/// reinterpret Markdown source to decide which roles apply.
enum FlarkCorePresentationInlineStyle {
  emphasis,
  strong,
  code,
  strikethrough,
  link,
}

/// One source-mapped run in a framework-neutral rendered row.
final class FlarkCorePresentationRun {
  const FlarkCorePresentationRun({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.sourceExact,
    required this.styles,
  }) : assert(sourceUtf16Start >= 0),
       assert(sourceUtf16End >= sourceUtf16Start),
       assert(!sourceExact || sourceUtf16End - sourceUtf16Start == text.length);

  final String text;
  final int sourceUtf16Start;
  final int sourceUtf16End;
  final bool sourceExact;
  final Set<FlarkCorePresentationInlineStyle> styles;
}

/// Presentation data shared by every Dart frontend.
///
/// Selection, hit testing, layout, and paint objects deliberately stay in the
/// frontend adapter. Source mapping and semantic style roles do not.
final class FlarkCorePresentationRow {
  const FlarkCorePresentationRow({
    required this.sourceUtf16,
    required this.leadingText,
    required this.text,
    required this.globalUtf16Start,
    required this.kind,
    required this.headingLevel,
    required this.blockQuoteDepth,
    required this.codeBlock,
    required this.thematicBreak,
    required this.ordinal,
    required this.runs,
  });

  final FlarkSourceRange sourceUtf16;
  final String leadingText;
  final String text;
  final int globalUtf16Start;
  final int kind;
  final int? headingLevel;
  final int? blockQuoteDepth;
  final FlarkCodeBlockPresentation? codeBlock;
  final bool thematicBreak;
  final int ordinal;
  final List<FlarkCorePresentationRun> runs;
}

/// Receipt-backed neutral gap introduced while a paragraph/list split awaits
/// parser certification.
final class FlarkCoreCommittedPresentationGapV1 {
  const FlarkCoreCommittedPresentationGapV1({
    required this.rowOrdinal,
    required this.rowEndUtf16,
  });

  final int rowOrdinal;
  final int rowEndUtf16;
}

/// Receipt-backed surface assembled only from source-mapped runs that remain
/// valid across the committed splice.
final class FlarkCoreCommittedPresentationSurfaceV1 {
  const FlarkCoreCommittedPresentationSurfaceV1({
    required this.rowOrdinal,
    required this.sourceUtf16,
    required this.presentation,
    this.removedRowOrdinal,
  });

  final int rowOrdinal;
  final FlarkSourceRange sourceUtf16;
  final FlarkCorePresentationRow presentation;
  final int? removedRowOrdinal;
}

/// Complete framework-neutral transitional presentation for one authoritative
/// semantic edit receipt.
final class FlarkCoreCommittedPresentationTransitionV1 {
  FlarkCoreCommittedPresentationTransitionV1({
    this.gap,
    List<FlarkCoreCommittedPresentationSurfaceV1> surfaces = const [],
    List<int> removedRowOrdinals = const [],
    this.clearPriorGap = false,
    this.retainPriorGap = false,
  }) : surfaces = List.unmodifiable(surfaces),
       removedRowOrdinals = List.unmodifiable(removedRowOrdinals),
       assert(
         gap != null ||
             surfaces.isNotEmpty ||
             removedRowOrdinals.isNotEmpty ||
             clearPriorGap ||
             retainPriorGap,
       );

  final FlarkCoreCommittedPresentationGapV1? gap;
  final List<FlarkCoreCommittedPresentationSurfaceV1> surfaces;
  final List<int> removedRowOrdinals;
  final bool clearPriorGap;
  final bool retainPriorGap;

  /// Compatibility view for transitions that still publish exactly one row.
  FlarkCoreCommittedPresentationSurfaceV1? get surface =>
      surfaces.length == 1 ? surfaces.single : null;
}

/// Applies the presentation consequence already classified by Rust.
///
/// This function does not recognize Markdown. It validates the typed receipt,
/// maps only source-contiguous, source-exact, unstyled runs through its exact
/// splice, and returns the bounded transition any Dart frontend may display
/// until the parser certifies the result revision. A predecessor row with any
/// projection gap fails closed because the row model cannot distinguish a
/// parser-owned block prefix from hidden inline syntax inside that gap, and the
/// structural receipt carries no result-revision inline-fact proof. Returning
/// null means the frontend must fall back to exact current source for the
/// affected region.
FlarkCoreCommittedPresentationTransitionV1?
resolveCommittedPresentationTransitionV1({
  required FlarkCoreEditIntentReceiptV1 receipt,
  required int? priorActiveOrdinal,
  required FlarkCorePresentationRow? activeRow,
  required FlarkCorePresentationRow? precedingRow,
  required bool priorGapPending,
}) {
  if (!receipt.hasCommit) return null;
  switch (receipt.presentationTransition) {
    case FlarkCoreEditPresentationTransitionV1.splitParagraph:
    case FlarkCoreEditPresentationTransitionV1.continueList:
    case FlarkCoreEditPresentationTransitionV1.continueBlockQuote:
    case FlarkCoreEditPresentationTransitionV1.continueIndentedCode:
      if ((receipt.presentationTransition ==
                  FlarkCoreEditPresentationTransitionV1.continueBlockQuote ||
              receipt.presentationTransition ==
                  FlarkCoreEditPresentationTransitionV1.continueIndentedCode) &&
          activeRow != null &&
          !_hasUncertifiedInlineProjection(activeRow)) {
        final projected = _continueProjectedPrefixedRow(activeRow, receipt);
        if (projected != null) return projected;
      }
      final ordinal = priorActiveOrdinal;
      final lineEndingLength = switch (receipt.presentationTransition) {
        FlarkCoreEditPresentationTransitionV1.splitParagraph =>
          _paragraphSplitRowExtension(receipt.replacement),
        FlarkCoreEditPresentationTransitionV1.continueList =>
          _leadingLineEndingLength(receipt.replacement),
        FlarkCoreEditPresentationTransitionV1.continueBlockQuote =>
          _leadingLineEndingLength(receipt.replacement),
        FlarkCoreEditPresentationTransitionV1.continueIndentedCode =>
          _leadingLineEndingLength(receipt.replacement),
        _ => null,
      };
      if (ordinal == null ||
          ordinal < 0 ||
          receipt.baseUtf16Start != receipt.baseUtf16End ||
          lineEndingLength == null) {
        return null;
      }
      return FlarkCoreCommittedPresentationTransitionV1(
        gap: FlarkCoreCommittedPresentationGapV1(
          rowOrdinal: ordinal,
          rowEndUtf16: receipt.baseUtf16Start + lineEndingLength,
        ),
      );
    case FlarkCoreEditPresentationTransitionV1.retainParagraphGap:
      return priorGapPending
          ? FlarkCoreCommittedPresentationTransitionV1(retainPriorGap: true)
          : null;
    case FlarkCoreEditPresentationTransitionV1.mergeParagraph:
      if (priorGapPending || activeRow == null || precedingRow == null) {
        return FlarkCoreCommittedPresentationTransitionV1(clearPriorGap: true);
      }
      if (_hasUncertifiedInlineProjection(activeRow) ||
          _hasUncertifiedInlineProjection(precedingRow)) {
        return FlarkCoreCommittedPresentationTransitionV1(clearPriorGap: true);
      }
      final runs = _mapRunsThroughCommittedSplice([
        ...precedingRow.runs,
        ...activeRow.runs,
      ], receipt);
      if (runs == null) {
        return FlarkCoreCommittedPresentationTransitionV1(clearPriorGap: true);
      }
      final resultEnd = activeRow.sourceUtf16.end + _utf16Delta(receipt);
      final source = FlarkSourceRange(
        precedingRow.sourceUtf16.start,
        resultEnd,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        clearPriorGap: true,
        surfaces: [
          FlarkCoreCommittedPresentationSurfaceV1(
            rowOrdinal: precedingRow.ordinal,
            removedRowOrdinal: activeRow.ordinal,
            sourceUtf16: source,
            presentation: FlarkCorePresentationRow(
              sourceUtf16: source,
              leadingText: precedingRow.leadingText,
              text: '${precedingRow.text}${activeRow.text}',
              globalUtf16Start: precedingRow.sourceUtf16.start,
              kind: precedingRow.kind,
              headingLevel: precedingRow.headingLevel,
              blockQuoteDepth: precedingRow.blockQuoteDepth,
              codeBlock: precedingRow.codeBlock,
              thematicBreak: precedingRow.thematicBreak,
              ordinal: precedingRow.ordinal,
              runs: runs,
            ),
          ),
        ],
      );
    case FlarkCoreEditPresentationTransitionV1.indentList:
      if (activeRow == null ||
          _hasUncertifiedInlineProjection(activeRow) ||
          receipt.replacement.isEmpty) {
        return null;
      }
      final inserted = receipt.replacement.length;
      if (receipt.baseUtf16Start != receipt.baseUtf16End ||
          inserted <= 0 ||
          receipt.replacement.codeUnits.any((unit) => unit != 0x20)) {
        return null;
      }
      final runs = _mapRunsThroughCommittedSplice(activeRow.runs, receipt);
      if (runs == null) return null;
      final delta = _utf16Delta(receipt);
      final source = FlarkSourceRange(
        receipt.resultUtf16Start,
        activeRow.sourceUtf16.end + delta,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        surfaces: [
          FlarkCoreCommittedPresentationSurfaceV1(
            rowOrdinal: activeRow.ordinal,
            sourceUtf16: source,
            presentation: FlarkCorePresentationRow(
              sourceUtf16: source,
              leadingText: '${receipt.replacement}${activeRow.leadingText}',
              text: activeRow.text,
              globalUtf16Start: activeRow.globalUtf16Start + delta,
              kind: activeRow.kind,
              headingLevel: activeRow.headingLevel,
              blockQuoteDepth: activeRow.blockQuoteDepth,
              codeBlock: activeRow.codeBlock,
              thematicBreak: activeRow.thematicBreak,
              ordinal: activeRow.ordinal,
              runs: runs,
            ),
          ),
        ],
      );
    case FlarkCoreEditPresentationTransitionV1.outdentList:
      if (activeRow == null ||
          _hasUncertifiedInlineProjection(activeRow) ||
          receipt.replacement.isNotEmpty) {
        return null;
      }
      final removed = receipt.baseUtf16End - receipt.baseUtf16Start;
      if (removed <= 0 || activeRow.leadingText.length < removed) return null;
      final runs = _mapRunsThroughCommittedSplice(activeRow.runs, receipt);
      if (runs == null) return null;
      final delta = _utf16Delta(receipt);
      final source = FlarkSourceRange(
        receipt.resultUtf16Start,
        activeRow.sourceUtf16.end + delta,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        surfaces: [
          FlarkCoreCommittedPresentationSurfaceV1(
            rowOrdinal: activeRow.ordinal,
            sourceUtf16: source,
            presentation: FlarkCorePresentationRow(
              sourceUtf16: source,
              leadingText: activeRow.leadingText.substring(removed),
              text: activeRow.text,
              globalUtf16Start: activeRow.globalUtf16Start + delta,
              kind: activeRow.kind,
              headingLevel: activeRow.headingLevel,
              blockQuoteDepth: activeRow.blockQuoteDepth,
              codeBlock: activeRow.codeBlock,
              thematicBreak: activeRow.thematicBreak,
              ordinal: activeRow.ordinal,
              runs: runs,
            ),
          ),
        ],
      );
    case FlarkCoreEditPresentationTransitionV1.outdentBlockQuote:
      if (activeRow == null ||
          _hasUncertifiedInlineProjection(activeRow) ||
          activeRow.blockQuoteDepth == null ||
          activeRow.blockQuoteDepth! <= 1) {
        return null;
      }
      return _outdentProjectedBlockQuote(activeRow, receipt);
    case FlarkCoreEditPresentationTransitionV1.joinIndentedCode:
      if (activeRow == null || _hasUncertifiedInlineProjection(activeRow)) {
        return null;
      }
      return _joinProjectedIndentedCode(activeRow, receipt);
    case FlarkCoreEditPresentationTransitionV1.deleteThematicBreak:
      if (activeRow == null ||
          !activeRow.thematicBreak ||
          receipt.replacement.isNotEmpty ||
          receipt.baseUtf16End != activeRow.sourceUtf16.end ||
          receipt.baseUtf16Start < activeRow.sourceUtf16.start ||
          receipt.baseUtf16Start >= receipt.baseUtf16End) {
        return null;
      }
      return FlarkCoreCommittedPresentationTransitionV1(
        removedRowOrdinals: [activeRow.ordinal],
        clearPriorGap: true,
      );
    case FlarkCoreEditPresentationTransitionV1.liftList:
    case FlarkCoreEditPresentationTransitionV1.exitList:
    case FlarkCoreEditPresentationTransitionV1.exitBlockQuote:
    case FlarkCoreEditPresentationTransitionV1.liftBlockQuote:
    case FlarkCoreEditPresentationTransitionV1.exitHeading:
    case FlarkCoreEditPresentationTransitionV1.liftHeading:
    case FlarkCoreEditPresentationTransitionV1.liftIndentedCode:
      if (activeRow == null || _hasUncertifiedInlineProjection(activeRow)) {
        return null;
      }
      if (receipt.presentationTransition ==
              FlarkCoreEditPresentationTransitionV1.liftBlockQuote ||
          receipt.presentationTransition ==
              FlarkCoreEditPresentationTransitionV1.exitBlockQuote) {
        final projected = _splitProjectedBlockQuote(activeRow, receipt);
        if (projected != null) return projected;
      }
      final runs = _mapRunsThroughCommittedSplice(activeRow.runs, receipt);
      if (runs == null) return null;
      final resultEnd = activeRow.sourceUtf16.end + _utf16Delta(receipt);
      final source = FlarkSourceRange(
        receipt.resultUtf16End,
        resultEnd < receipt.resultUtf16End ? receipt.resultUtf16End : resultEnd,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        surfaces: [
          FlarkCoreCommittedPresentationSurfaceV1(
            rowOrdinal: activeRow.ordinal,
            sourceUtf16: source,
            presentation: FlarkCorePresentationRow(
              sourceUtf16: source,
              leadingText: '',
              text: activeRow.text,
              globalUtf16Start: receipt.resultUtf16End,
              kind: 5,
              headingLevel: null,
              blockQuoteDepth: null,
              codeBlock: null,
              thematicBreak: false,
              ordinal: activeRow.ordinal,
              runs: runs,
            ),
          ),
        ],
      );
    case FlarkCoreEditPresentationTransitionV1.toggleTaskChecked:
    case FlarkCoreEditPresentationTransitionV1.none:
      return null;
  }
}

bool _hasUncertifiedInlineProjection(FlarkCorePresentationRow row) {
  if (row.runs.isEmpty) {
    return row.globalUtf16Start < row.sourceUtf16.end;
  }
  var expectedSourceStart = row.globalUtf16Start;
  for (final run in row.runs) {
    if (!run.sourceExact ||
        run.styles.isNotEmpty ||
        run.sourceUtf16Start != expectedSourceStart) {
      return true;
    }
    expectedSourceStart = run.sourceUtf16End;
  }
  return false;
}

FlarkCoreCommittedPresentationTransitionV1? _continueProjectedPrefixedRow(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final projectedQuote =
      receipt.presentationTransition ==
          FlarkCoreEditPresentationTransitionV1.continueBlockQuote &&
      row.blockQuoteDepth != null &&
      row.blockQuoteDepth! >= 1 &&
      row.runs.length >= 2;
  final projectedIndentedCode =
      receipt.presentationTransition ==
          FlarkCoreEditPresentationTransitionV1.continueIndentedCode &&
      row.codeBlock?.style == FlarkCodeBlockStyle.indented;
  if ((!projectedQuote && !projectedIndentedCode) ||
      receipt.baseUtf16Start != receipt.baseUtf16End) {
    return null;
  }
  final endingLength = _leadingLineEndingLength(receipt.replacement);
  if (endingLength == null || receipt.replacement.length <= endingLength) {
    return null;
  }
  var hasHiddenGap = false;
  for (var index = 1; index < row.runs.length; index += 1) {
    if (row.runs[index - 1].sourceUtf16End < row.runs[index].sourceUtf16Start) {
      hasHiddenGap = true;
      break;
    }
  }
  if (projectedQuote && !hasHiddenGap) return null;

  final insertion = receipt.baseUtf16Start;
  final delta = _utf16Delta(receipt);
  final mapped = <FlarkCorePresentationRun>[];
  var inserted = false;
  for (final run in row.runs) {
    if (run.sourceUtf16End < insertion ||
        (run.sourceUtf16End == insertion && inserted)) {
      mapped.add(run);
      continue;
    }
    if (!inserted &&
        run.sourceExact &&
        run.sourceUtf16Start <= insertion &&
        insertion <= run.sourceUtf16End) {
      final split = insertion - run.sourceUtf16Start;
      if (split < 0 || split > run.text.length) return null;
      if (split > 0) {
        mapped.add(
          FlarkCorePresentationRun(
            text: run.text.substring(0, split),
            sourceUtf16Start: run.sourceUtf16Start,
            sourceUtf16End: insertion,
            sourceExact: true,
            styles: run.styles,
          ),
        );
      }
      final ending = receipt.replacement.substring(0, endingLength);
      mapped.add(
        FlarkCorePresentationRun(
          text: ending,
          sourceUtf16Start: insertion,
          sourceUtf16End: insertion + endingLength,
          sourceExact: true,
          styles: const {},
        ),
      );
      if (split < run.text.length) {
        mapped.add(
          FlarkCorePresentationRun(
            text: run.text.substring(split),
            sourceUtf16Start: insertion + receipt.replacement.length,
            sourceUtf16End: run.sourceUtf16End + delta,
            sourceExact: true,
            styles: run.styles,
          ),
        );
      }
      inserted = true;
      continue;
    }
    if (run.sourceUtf16Start < insertion) return null;
    mapped.add(
      FlarkCorePresentationRun(
        text: run.text,
        sourceUtf16Start: run.sourceUtf16Start + delta,
        sourceUtf16End: run.sourceUtf16End + delta,
        sourceExact: run.sourceExact,
        styles: run.styles,
      ),
    );
  }
  if (!inserted) return null;

  final source = FlarkSourceRange(
    row.sourceUtf16.start,
    row.sourceUtf16.end + delta,
  );
  final runs = List<FlarkCorePresentationRun>.unmodifiable(mapped);
  return FlarkCoreCommittedPresentationTransitionV1(
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: source,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: source,
          leadingText: row.leadingText,
          text: runs.map((run) => run.text).join(),
          globalUtf16Start: row.globalUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: row.codeBlock,
          thematicBreak: row.thematicBreak,
          ordinal: row.ordinal,
          runs: runs,
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _joinProjectedIndentedCode(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  if (row.codeBlock?.style != FlarkCodeBlockStyle.indented ||
      receipt.replacement.isNotEmpty ||
      receipt.baseUtf16Start >= receipt.baseUtf16End) {
    return null;
  }
  final delta = _utf16Delta(receipt);
  final mapped = <FlarkCorePresentationRun>[];
  var cutStartFound = false;
  for (final run in row.runs) {
    if (run.sourceUtf16End <= receipt.baseUtf16Start) {
      mapped.add(run);
      continue;
    }
    if (!cutStartFound &&
        run.sourceExact &&
        run.sourceUtf16Start <= receipt.baseUtf16Start &&
        receipt.baseUtf16Start <= run.sourceUtf16End) {
      final retained = receipt.baseUtf16Start - run.sourceUtf16Start;
      if (retained < 0 || retained > run.text.length) return null;
      if (retained > 0) {
        mapped.add(
          FlarkCorePresentationRun(
            text: run.text.substring(0, retained),
            sourceUtf16Start: run.sourceUtf16Start,
            sourceUtf16End: receipt.baseUtf16Start,
            sourceExact: true,
            styles: run.styles,
          ),
        );
      }
      cutStartFound = true;
      continue;
    }
    if (run.sourceUtf16Start < receipt.baseUtf16End) return null;
    mapped.add(
      FlarkCorePresentationRun(
        text: run.text,
        sourceUtf16Start: run.sourceUtf16Start + delta,
        sourceUtf16End: run.sourceUtf16End + delta,
        sourceExact: run.sourceExact,
        styles: run.styles,
      ),
    );
  }
  if (!cutStartFound) return null;
  final source = FlarkSourceRange(
    row.sourceUtf16.start,
    row.sourceUtf16.end + delta,
  );
  final runs = List<FlarkCorePresentationRun>.unmodifiable(mapped);
  return FlarkCoreCommittedPresentationTransitionV1(
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: source,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: source,
          leadingText: row.leadingText,
          text: runs.map((run) => run.text).join(),
          globalUtf16Start: row.globalUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: row.codeBlock,
          thematicBreak: row.thematicBreak,
          ordinal: row.ordinal,
          runs: runs,
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _splitProjectedBlockQuote(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  if (row.blockQuoteDepth != 1 || row.runs.length < 2) return null;
  final before = <FlarkCorePresentationRun>[];
  final after = <FlarkCorePresentationRun>[];
  final delta = _utf16Delta(receipt);
  for (final run in row.runs) {
    if (run.sourceUtf16End <= receipt.baseUtf16Start) {
      before.add(run);
      continue;
    }
    if (run.sourceUtf16Start >= receipt.baseUtf16End) {
      after.add(
        FlarkCorePresentationRun(
          text: run.text,
          sourceUtf16Start: run.sourceUtf16Start + delta,
          sourceUtf16End: run.sourceUtf16End + delta,
          sourceExact: run.sourceExact,
          styles: run.styles,
        ),
      );
      continue;
    }
    return null;
  }
  // The splice must target a hidden gap after at least one retained quoted
  // run. A first-line lift is represented by the existing single plain
  // surface, and a range that overlaps a run is not a projected-prefix edit.
  if (before.isEmpty || receipt.replacement.isEmpty) return null;

  final quoteEnd = receipt.resultUtf16Start;
  final resultEnd = row.sourceUtf16.end + delta;
  if (quoteEnd < row.sourceUtf16.start ||
      resultEnd < receipt.resultUtf16Start) {
    return null;
  }
  final replacementRun = FlarkCorePresentationRun(
    text: receipt.replacement,
    sourceUtf16Start: receipt.resultUtf16Start,
    sourceUtf16End: receipt.resultUtf16End,
    sourceExact: true,
    styles: const {},
  );
  final quoteRuns = List<FlarkCorePresentationRun>.unmodifiable(before);
  final plainRuns = List<FlarkCorePresentationRun>.unmodifiable([
    replacementRun,
    ...after,
  ]);
  final quoteSource = FlarkSourceRange(row.sourceUtf16.start, quoteEnd);
  final plainSource = FlarkSourceRange(receipt.resultUtf16Start, resultEnd);

  return FlarkCoreCommittedPresentationTransitionV1(
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: quoteSource,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: quoteSource,
          leadingText: row.leadingText,
          text: quoteRuns.map((run) => run.text).join(),
          globalUtf16Start: row.globalUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: row.codeBlock,
          thematicBreak: row.thematicBreak,
          ordinal: row.ordinal,
          runs: quoteRuns,
        ),
      ),
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: plainSource,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: plainSource,
          leadingText: '',
          text: plainRuns.map((run) => run.text).join(),
          globalUtf16Start: receipt.resultUtf16Start,
          kind: 5,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: plainRuns,
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _outdentProjectedBlockQuote(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final depth = row.blockQuoteDepth;
  if (depth == null ||
      depth <= 1 ||
      receipt.baseUtf16Start >= receipt.baseUtf16End) {
    return null;
  }
  final delta = _utf16Delta(receipt);
  final replacementEnding = receipt.replacement.isEmpty
      ? null
      : _leadingLineEndingLength(receipt.replacement);
  if (receipt.replacement.isNotEmpty &&
      (replacementEnding == null ||
          receipt.replacement.length <= replacementEnding)) {
    return null;
  }
  final targetIndex = row.runs.indexWhere(
    (run) => run.sourceUtf16Start == receipt.baseUtf16End,
  );
  if (targetIndex < 0) return null;
  var targetEndExclusive = targetIndex + 1;
  while (targetEndExclusive < row.runs.length &&
      row.runs[targetEndExclusive - 1].sourceUtf16End ==
          row.runs[targetEndExclusive].sourceUtf16Start) {
    targetEndExclusive += 1;
  }

  FlarkCorePresentationRun map(FlarkCorePresentationRun run) =>
      FlarkCorePresentationRun(
        text: run.text,
        sourceUtf16Start: run.sourceUtf16Start + delta,
        sourceUtf16End: run.sourceUtf16End + delta,
        sourceExact: run.sourceExact,
        styles: run.styles,
      );

  final before = row.runs.take(targetIndex).toList(growable: false);
  final mappedTarget = row.runs
      .skip(targetIndex)
      .take(targetEndExclusive - targetIndex)
      .map(map)
      .toList(growable: false);
  final target = <FlarkCorePresentationRun>[
    if (replacementEnding != null)
      FlarkCorePresentationRun(
        text: receipt.replacement.substring(0, replacementEnding),
        sourceUtf16Start: receipt.resultUtf16Start,
        sourceUtf16End: receipt.resultUtf16Start + replacementEnding,
        sourceExact: true,
        styles: const {},
      ),
    ...mappedTarget,
  ];
  final after = row.runs
      .skip(targetEndExclusive)
      .map(map)
      .toList(growable: false);
  final surfaces = <FlarkCoreCommittedPresentationSurfaceV1>[];
  var ordinal = row.ordinal;

  if (before.isNotEmpty) {
    final source = FlarkSourceRange(
      row.sourceUtf16.start,
      receipt.resultUtf16Start,
    );
    surfaces.add(
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: ordinal,
        sourceUtf16: source,
        presentation: _quotePresentationFromRuns(
          row,
          source: source,
          depth: depth,
          runs: before,
        ),
      ),
    );
    ordinal += 1;
  }

  final targetEnd = after.isEmpty
      ? row.sourceUtf16.end + delta
      : target.last.sourceUtf16End;
  final targetSource = FlarkSourceRange(receipt.resultUtf16Start, targetEnd);
  surfaces.add(
    FlarkCoreCommittedPresentationSurfaceV1(
      rowOrdinal: ordinal,
      sourceUtf16: targetSource,
      presentation: _quotePresentationFromRuns(
        row,
        source: targetSource,
        depth: depth - 1,
        runs: target,
      ),
    ),
  );
  ordinal += 1;

  if (after.isNotEmpty) {
    final source = FlarkSourceRange(
      target.last.sourceUtf16End,
      row.sourceUtf16.end + delta,
    );
    surfaces.add(
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: ordinal,
        sourceUtf16: source,
        presentation: _quotePresentationFromRuns(
          row,
          source: source,
          depth: depth,
          runs: after,
        ),
      ),
    );
  }
  return FlarkCoreCommittedPresentationTransitionV1(surfaces: surfaces);
}

FlarkCorePresentationRow _quotePresentationFromRuns(
  FlarkCorePresentationRow row, {
  required FlarkSourceRange source,
  required int depth,
  required List<FlarkCorePresentationRun> runs,
}) => FlarkCorePresentationRow(
  sourceUtf16: source,
  leadingText: row.leadingText,
  text: runs.map((run) => run.text).join(),
  globalUtf16Start: runs.first.sourceUtf16Start,
  kind: row.kind,
  headingLevel: row.headingLevel,
  blockQuoteDepth: depth,
  codeBlock: row.codeBlock,
  thematicBreak: row.thematicBreak,
  ordinal: row.ordinal,
  runs: List.unmodifiable(runs),
);

List<FlarkCorePresentationRun>? _mapRunsThroughCommittedSplice(
  List<FlarkCorePresentationRun> runs,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final mapped = <FlarkCorePresentationRun>[];
  final delta = _utf16Delta(receipt);
  for (final run in runs) {
    if (run.sourceUtf16End <= receipt.baseUtf16Start) {
      mapped.add(run);
      continue;
    }
    if (run.sourceUtf16Start < receipt.baseUtf16End) return null;
    mapped.add(
      FlarkCorePresentationRun(
        text: run.text,
        sourceUtf16Start: run.sourceUtf16Start + delta,
        sourceUtf16End: run.sourceUtf16End + delta,
        sourceExact: run.sourceExact,
        styles: run.styles,
      ),
    );
  }
  return List.unmodifiable(mapped);
}

int _utf16Delta(FlarkCoreEditIntentReceiptV1 receipt) =>
    receipt.replacement.length -
    (receipt.baseUtf16End - receipt.baseUtf16Start);

int? _paragraphSplitRowExtension(String replacement) => switch (replacement) {
  '\n' || '\r' || '\r\n' => 0,
  '\n\n' || '\r\r' => 1,
  '\r\n\r\n' => 2,
  _ => null,
};

int? _leadingLineEndingLength(String replacement) {
  if (replacement.startsWith('\r\n')) return 2;
  if (replacement.startsWith('\n') || replacement.startsWith('\r')) return 1;
  return null;
}

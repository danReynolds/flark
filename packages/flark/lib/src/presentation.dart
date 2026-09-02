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
  FlarkCorePresentationRun({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.sourceExact,
    required Set<FlarkCorePresentationInlineStyle> styles,
  }) : styles = Set.unmodifiable(styles),
       assert(sourceUtf16Start >= 0),
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
  FlarkCorePresentationRow({
    required this.sourceUtf16,
    required this.leadingText,
    required this.text,
    required this.globalUtf16Start,
    required this.kind,
    required this.headingLevel,
    required this.blockQuoteDepth,
    required this.codeBlock,
    required this.thematicBreak,
    this.listItem = false,
    required this.ordinal,
    required List<FlarkCorePresentationRun> runs,
  }) : runs = List.unmodifiable(runs);

  final FlarkSourceRange sourceUtf16;
  final String leadingText;
  final String text;
  final int globalUtf16Start;
  final int kind;
  final int? headingLevel;
  final int? blockQuoteDepth;
  final FlarkCodeBlockPresentation? codeBlock;
  final bool thematicBreak;
  final bool listItem;
  final int ordinal;
  final List<FlarkCorePresentationRun> runs;
}

/// Receipt-backed neutral gap introduced by a paragraph/list split.
///
/// Fresh parser rows supersede its temporary visual partition, but the gap
/// continues to identify an editor-owned caret boundary that the Markdown AST
/// cannot represent. That interaction authority lives until a later edit or
/// explicit row activation leaves the gap.
final class FlarkCoreCommittedPresentationGapV1 {
  const FlarkCoreCommittedPresentationGapV1({
    required this.rowOrdinal,
    required this.rowEndUtf16,
  });

  final int rowOrdinal;
  final int rowEndUtf16;
}

/// The parser-owned function of one row in a temporary structural lineage.
///
/// A transient block separator remains source-owned while its following
/// successor is empty, but stops painting once that successor contains text.
/// A visible blank separator represents an additional physical line ending
/// and remains painted. Keeping that distinction typed avoids asking a
/// frontend to reinterpret source geometry or Markdown ownership.
enum FlarkCoreCommittedPresentationSurfaceRole {
  content,
  blockSeparator,
  visibleBlankSeparator,
  editableSuccessor,
}

/// Receipt-backed surface assembled only from source-mapped runs that remain
/// valid across the committed splice.
final class FlarkCoreCommittedPresentationSurfaceV1 {
  FlarkCoreCommittedPresentationSurfaceV1({
    required this.rowOrdinal,
    required this.sourceUtf16,
    required this.presentation,
    this.removedRowOrdinal,
    this.projectionCurrent = false,
    List<FlarkProjectionEditCell> projectionEditCells = const [],
    this.role = FlarkCoreCommittedPresentationSurfaceRole.content,
  }) : projectionEditCells = List.unmodifiable(projectionEditCells);

  final int rowOrdinal;
  final FlarkSourceRange sourceUtf16;
  final FlarkCorePresentationRow presentation;
  final int? removedRowOrdinal;
  final bool projectionCurrent;
  final List<FlarkProjectionEditCell> projectionEditCells;
  final FlarkCoreCommittedPresentationSurfaceRole role;
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
  bool activeRowTransitional = false,
}) {
  if (!receipt.hasCommit) return null;
  switch (receipt.presentationTransition) {
    case FlarkCoreEditPresentationTransitionV1.splitParagraph:
    case FlarkCoreEditPresentationTransitionV1.continueList:
    case FlarkCoreEditPresentationTransitionV1.continueBlockQuote:
    case FlarkCoreEditPresentationTransitionV1.continueIndentedCode:
      if (receipt.presentationTransition ==
              FlarkCoreEditPresentationTransitionV1.splitParagraph &&
          activeRow != null) {
        if (receipt.presentationProven) {
          final fenced = _splitProvenFencedCode(activeRow, receipt);
          if (fenced != null) return fenced;
          final projected = _splitProvenParagraph(activeRow, receipt);
          if (projected != null) return projected;
        }
        // An unproved carried surface is result presentation, not fresh parser
        // authority. It may remain painted, but cannot recursively authorize
        // another structural partition before certification catches up.
        if (!activeRowTransitional) {
          final exact = _splitExactParagraphFallback(activeRow, receipt);
          if (exact != null) return exact;
        }
      }
      if (receipt.presentationTransition ==
              FlarkCoreEditPresentationTransitionV1.continueList &&
          receipt.presentationProven &&
          activeRow != null) {
        final projected = _continueProvenTerminalList(activeRow, receipt);
        if (projected != null) return projected;
      }
      if (receipt.presentationTransition ==
              FlarkCoreEditPresentationTransitionV1.continueBlockQuote &&
          receipt.presentationProven &&
          activeRow != null) {
        final projected = _continueProvenTerminalBlockQuote(activeRow, receipt);
        if (projected != null) return projected;
      }
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
      if (!receipt.presentationProven &&
          (_hasUncertifiedInlineProjection(activeRow) ||
              _hasUncertifiedInlineProjection(precedingRow))) {
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
            projectionCurrent: receipt.presentationProven,
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
            projectionCurrent: receipt.presentationProven,
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
            projectionCurrent: receipt.presentationProven,
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
    case FlarkCoreEditPresentationTransitionV1.joinFencedCode:
      if (activeRow == null || !receipt.presentationProven) return null;
      return _joinProvenFencedCode(activeRow, receipt);
    case FlarkCoreEditPresentationTransitionV1.deleteInlineOwner:
      if (activeRow == null) return null;
      return _deleteInlineOwner(activeRow, receipt);
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
      if ((receipt.presentationTransition ==
                  FlarkCoreEditPresentationTransitionV1.exitList ||
              receipt.presentationTransition ==
                  FlarkCoreEditPresentationTransitionV1.liftList ||
              receipt.presentationTransition ==
                  FlarkCoreEditPresentationTransitionV1.exitBlockQuote ||
              receipt.presentationTransition ==
                  FlarkCoreEditPresentationTransitionV1.liftBlockQuote) &&
          receipt.presentationProven &&
          activeRow != null) {
        final projected = _exitProvenEmptyPrefixedRow(activeRow, receipt);
        if (projected != null) return projected;
      }
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

FlarkCoreCommittedPresentationTransitionV1? _deleteInlineOwner(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  if (receipt.replacement.isNotEmpty ||
      receipt.baseUtf16Start >= receipt.baseUtf16End ||
      receipt.baseUtf16Start < row.sourceUtf16.start ||
      receipt.baseUtf16End > row.sourceUtf16.end) {
    return null;
  }
  final runs = <FlarkCorePresentationRun>[];
  final delta = _utf16Delta(receipt);
  for (final run in row.runs) {
    if (run.sourceUtf16End <= receipt.baseUtf16Start) {
      runs.add(run);
      continue;
    }
    if (run.sourceUtf16Start >= receipt.baseUtf16End) {
      runs.add(
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
    if (run.sourceUtf16Start < receipt.baseUtf16Start ||
        run.sourceUtf16End > receipt.baseUtf16End) {
      return null;
    }
  }
  final resultSource = FlarkSourceRange(
    row.sourceUtf16.start,
    row.sourceUtf16.end + delta,
  );
  if (resultSource.end < resultSource.start) return null;
  final mappedRuns = List<FlarkCorePresentationRun>.unmodifiable(runs);
  return FlarkCoreCommittedPresentationTransitionV1(
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: resultSource,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: resultSource,
          leadingText: row.leadingText,
          text: mappedRuns.map((run) => run.text).join(),
          globalUtf16Start: mappedRuns.isEmpty
              ? resultSource.start
              : mappedRuns.first.sourceUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: row.codeBlock,
          thematicBreak: row.thematicBreak,
          listItem: row.listItem,
          ordinal: row.ordinal,
          runs: mappedRuns,
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _splitExactParagraphFallback(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final endingLength = _paragraphSplitRowExtension(receipt.replacement);
  if (endingLength == null ||
      receipt.baseUtf16Start != receipt.baseUtf16End ||
      receipt.baseUtf16Start < row.sourceUtf16.start ||
      receipt.baseUtf16Start > row.sourceUtf16.end) {
    return null;
  }
  final predecessorSource = FlarkSourceRange(
    row.sourceUtf16.start,
    receipt.baseUtf16Start + endingLength,
  );
  final resultEnd = row.sourceUtf16.end + _utf16Delta(receipt);
  if (resultEnd < receipt.resultSelectionUtf16) return null;
  final successorSource = FlarkSourceRange(
    receipt.resultSelectionUtf16,
    resultEnd,
  );

  FlarkCoreCommittedPresentationSurfaceV1 exactSurface(
    FlarkSourceRange source, {
    FlarkCoreCommittedPresentationSurfaceRole role =
        FlarkCoreCommittedPresentationSurfaceRole.content,
  }) => FlarkCoreCommittedPresentationSurfaceV1(
    rowOrdinal: row.ordinal,
    sourceUtf16: source,
    role: role,
    presentation: FlarkCorePresentationRow(
      sourceUtf16: source,
      leadingText: '',
      text: '',
      globalUtf16Start: source.start,
      kind: 0,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: row.ordinal,
      runs: const [],
    ),
  );

  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      exactSurface(predecessorSource),
      exactSurface(
        successorSource,
        role: FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _exitProvenEmptyPrefixedRow(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final typedBlockMatches = switch (receipt.presentationTransition) {
    FlarkCoreEditPresentationTransitionV1.exitList ||
    FlarkCoreEditPresentationTransitionV1.liftList => row.listItem,
    FlarkCoreEditPresentationTransitionV1.exitBlockQuote ||
    FlarkCoreEditPresentationTransitionV1.liftBlockQuote =>
      row.blockQuoteDepth != null && row.blockQuoteDepth! > 0,
    _ => false,
  };
  final replacementIsLineEnding =
      receipt.replacement.isEmpty ||
      receipt.replacement == '\n' ||
      receipt.replacement == '\r' ||
      receipt.replacement == '\r\n';
  if (!typedBlockMatches ||
      row.text.isNotEmpty ||
      row.leadingText.isEmpty ||
      !replacementIsLineEnding ||
      receipt.baseUtf16Start != row.sourceUtf16.start ||
      receipt.baseUtf16End != row.globalUtf16Start ||
      receipt.resultSelectionUtf16 != receipt.resultUtf16End) {
    return null;
  }
  final resultEnd = row.sourceUtf16.end + _utf16Delta(receipt);
  if (resultEnd < receipt.resultUtf16End) return null;
  final source = FlarkSourceRange(receipt.resultUtf16End, resultEnd);
  final separatorSource = FlarkSourceRange(
    receipt.resultUtf16Start,
    receipt.resultUtf16End,
  );
  if (separatorSource.length == 0) return null;
  final insertionPoint = FlarkSourceRange(
    receipt.resultUtf16End,
    receipt.resultUtf16End,
  );
  final byteInsertionPoint = FlarkSourceRange(
    receipt.resultByteEnd,
    receipt.resultByteEnd,
  );
  final cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
    affectedBytes: byteInsertionPoint,
    affectedUtf16: insertionPoint,
    triggerBytes: byteInsertionPoint,
    triggerUtf16: insertionPoint,
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: separatorSource,
        projectionCurrent: true,
        role: FlarkCoreCommittedPresentationSurfaceRole.blockSeparator,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: separatorSource,
          leadingText: '',
          text: '',
          globalUtf16Start: separatorSource.start,
          kind: 0,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: source,
        projectionCurrent: true,
        projectionEditCells: [cell],
        role: FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: source,
          leadingText: '',
          text: '',
          globalUtf16Start: receipt.resultUtf16End,
          kind: 5,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _continueProvenTerminalBlockQuote(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final endingLength = _leadingLineEndingLength(receipt.replacement);
  if (endingLength == null ||
      row.blockQuoteDepth == null ||
      row.blockQuoteDepth! <= 0 ||
      row.leadingText.isEmpty ||
      _hasUncertifiedInlineProjection(row) ||
      row.runs.isEmpty ||
      receipt.baseUtf16Start != receipt.baseUtf16End ||
      receipt.baseUtf16Start != row.runs.last.sourceUtf16End ||
      receipt.resultSelectionUtf16 !=
          receipt.baseUtf16Start + receipt.replacement.length) {
    return null;
  }
  final prefix = receipt.replacement.substring(endingLength);
  if (prefix.isEmpty || prefix.contains('\n') || prefix.contains('\r')) {
    return null;
  }
  final runs = _mapRunsThroughCommittedSplice(row.runs, receipt);
  if (runs == null) return null;
  final prefixStart = receipt.baseUtf16Start + endingLength;
  final successorStart = receipt.resultSelectionUtf16;
  final predecessorSource = FlarkSourceRange(
    row.sourceUtf16.start,
    prefixStart,
  );
  final successorSource = FlarkSourceRange(prefixStart, successorStart);
  final insertionPoint = FlarkSourceRange(successorStart, successorStart);
  final byteInsertionPoint = FlarkSourceRange(
    receipt.resultByteEnd,
    receipt.resultByteEnd,
  );
  final cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
    affectedBytes: byteInsertionPoint,
    affectedUtf16: insertionPoint,
    triggerBytes: byteInsertionPoint,
    triggerUtf16: insertionPoint,
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: predecessorSource,
        projectionCurrent: true,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: predecessorSource,
          leadingText: row.leadingText,
          text: row.text,
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
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: successorSource,
        projectionCurrent: true,
        projectionEditCells: [cell],
        presentation: FlarkCorePresentationRow(
          sourceUtf16: successorSource,
          leadingText: row.leadingText,
          text: '',
          globalUtf16Start: successorStart,
          kind: 15,
          headingLevel: null,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _splitProvenParagraph(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final endingLength = _paragraphSplitRowExtension(receipt.replacement);
  if (endingLength == null ||
      receipt.baseUtf16Start != receipt.baseUtf16End ||
      receipt.baseUtf16Start < row.globalUtf16Start ||
      receipt.baseUtf16Start > row.sourceUtf16.end ||
      row.runs.isEmpty) {
    return null;
  }
  if (receipt.baseUtf16Start != row.runs.last.sourceUtf16End) {
    return _splitProvenEmbeddedPlainLine(row, receipt, endingLength);
  }
  final runs = _mapRunsThroughCommittedSplice(row.runs, receipt);
  if (runs == null) return null;
  final predecessorSource = FlarkSourceRange(
    row.sourceUtf16.start,
    receipt.baseUtf16Start + endingLength,
  );
  final successorStart = receipt.resultSelectionUtf16;
  final successorSource = FlarkSourceRange(successorStart, successorStart);
  final neutralSource = FlarkSourceRange(predecessorSource.end, successorStart);
  if (neutralSource.length == 0) return null;
  // A two-ending split assigns its first ending to the predecessor. A split
  // before source that already follows the visible run does the same. The
  // neutral ending is therefore an additional, visibly blank row.
  final predecessorOwnsLineEndingAfterSplice =
      endingLength > 0 || receipt.baseUtf16Start < row.sourceUtf16.end;
  final cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
    affectedBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd,
    ),
    affectedUtf16: successorSource,
    triggerBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd,
    ),
    triggerUtf16: successorSource,
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: predecessorSource,
        projectionCurrent: true,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: predecessorSource,
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
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: neutralSource,
        projectionCurrent: true,
        role: predecessorOwnsLineEndingAfterSplice
            ? FlarkCoreCommittedPresentationSurfaceRole.visibleBlankSeparator
            : FlarkCoreCommittedPresentationSurfaceRole.blockSeparator,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: neutralSource,
          leadingText: '',
          text: '',
          globalUtf16Start: neutralSource.start,
          kind: 0,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: successorSource,
        projectionCurrent: true,
        projectionEditCells: [cell],
        role: FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: successorSource,
          leadingText: '',
          text: '',
          globalUtf16Start: successorStart,
          kind: 5,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _splitProvenFencedCode(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final codeBlock = row.codeBlock;
  if (codeBlock == null ||
      !codeBlock.isFenced ||
      !codeBlock.closed ||
      _leadingLineEndingLength(receipt.replacement) !=
          receipt.replacement.length ||
      receipt.baseUtf16Start != receipt.baseUtf16End ||
      row.runs.length != 1) {
    return null;
  }
  final run = row.runs.single;
  if (!run.sourceExact ||
      run.styles.isNotEmpty ||
      receipt.baseUtf16Start < run.sourceUtf16Start ||
      receipt.baseUtf16Start >= run.sourceUtf16End) {
    return null;
  }
  final split = receipt.baseUtf16Start - run.sourceUtf16Start;
  final text = run.text.replaceRange(split, split, receipt.replacement);
  final delta = _utf16Delta(receipt);
  final source = FlarkSourceRange(
    row.sourceUtf16.start,
    row.sourceUtf16.end + delta,
  );
  final mappedRun = FlarkCorePresentationRun(
    text: text,
    sourceUtf16Start: run.sourceUtf16Start,
    sourceUtf16End: run.sourceUtf16End + delta,
    sourceExact: true,
    styles: const {},
  );
  final insertionPoint = FlarkSourceRange(
    receipt.resultSelectionUtf16,
    receipt.resultSelectionUtf16,
  );
  final cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd,
    affectedBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd,
    ),
    affectedUtf16: insertionPoint,
    triggerBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd,
    ),
    triggerUtf16: insertionPoint,
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    // The semantic receipt proves a new physical code-line boundary. The
    // append vocabulary excludes both fence characters, so a first literal
    // unit and its same-line successors cannot expose a closing fence.
    chainResultCell: true,
    terminalSpaceAvailable: true,
  );
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: source,
        projectionCurrent: true,
        projectionEditCells: [cell],
        presentation: FlarkCorePresentationRow(
          sourceUtf16: source,
          leadingText: row.leadingText,
          text: text,
          globalUtf16Start: row.globalUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: codeBlock,
          thematicBreak: row.thematicBreak,
          ordinal: row.ordinal,
          runs: [mappedRun],
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _joinProvenFencedCode(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final codeBlock = row.codeBlock;
  if (codeBlock == null ||
      !codeBlock.isFenced ||
      !codeBlock.closed ||
      receipt.replacement.isNotEmpty ||
      receipt.baseUtf16Start >= receipt.baseUtf16End ||
      row.runs.length != 1) {
    return null;
  }
  final run = row.runs.single;
  if (!run.sourceExact ||
      run.styles.isNotEmpty ||
      receipt.baseUtf16Start < run.sourceUtf16Start ||
      receipt.baseUtf16End > run.sourceUtf16End) {
    return null;
  }
  final start = receipt.baseUtf16Start - run.sourceUtf16Start;
  final end = receipt.baseUtf16End - run.sourceUtf16Start;
  final removed = run.text.substring(start, end);
  if (removed != '\n' && removed != '\r' && removed != '\r\n') return null;
  final text = run.text.replaceRange(start, end, '');
  final delta = _utf16Delta(receipt);
  final source = FlarkSourceRange(
    row.sourceUtf16.start,
    row.sourceUtf16.end + delta,
  );
  final mappedRun = FlarkCorePresentationRun(
    text: text,
    sourceUtf16Start: run.sourceUtf16Start,
    sourceUtf16End: run.sourceUtf16End + delta,
    sourceExact: true,
    styles: const {},
  );
  final joinedAtEmptyLine =
      _leadingLineEndingLength(run.text.substring(end)) != null;
  final insertionPoint = FlarkSourceRange(
    receipt.resultSelectionUtf16,
    receipt.resultSelectionUtf16,
  );
  final cells = joinedAtEmptyLine
      ? [
          FlarkProjectionEditCell(
            matcher: FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd,
            affectedBytes: FlarkSourceRange(
              receipt.resultByteEnd,
              receipt.resultByteEnd,
            ),
            affectedUtf16: insertionPoint,
            triggerBytes: FlarkSourceRange(
              receipt.resultByteEnd,
              receipt.resultByteEnd,
            ),
            triggerUtf16: insertionPoint,
            retainBlockShell: true,
            retainOutsideClosure: true,
            presentClosureExact: true,
            chainResultCell: true,
            terminalSpaceAvailable: true,
          ),
        ]
      : const <FlarkProjectionEditCell>[];
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: source,
        projectionCurrent: true,
        projectionEditCells: cells,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: source,
          leadingText: row.leadingText,
          text: text,
          globalUtf16Start: row.globalUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: codeBlock,
          thematicBreak: row.thematicBreak,
          ordinal: row.ordinal,
          runs: [mappedRun],
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _splitProvenEmbeddedPlainLine(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
  int endingLength,
) {
  if (endingLength != 0 ||
      receipt.replacement.length != 1 ||
      row.leadingText.isNotEmpty ||
      row.kind != 5 ||
      row.runs.length != 1) {
    return null;
  }
  final run = row.runs.single;
  if (!run.sourceExact ||
      run.styles.isNotEmpty ||
      receipt.baseUtf16Start < run.sourceUtf16Start ||
      receipt.baseUtf16Start >= run.sourceUtf16End) {
    return null;
  }
  final split = receipt.baseUtf16Start - run.sourceUtf16Start;
  if (split < 0 ||
      split + 1 >= run.text.length ||
      run.text.codeUnitAt(split) != 0x20 ||
      run.text.codeUnitAt(split + 1) == 0x20 ||
      run.text.codeUnitAt(split + 1) == 0x09) {
    return null;
  }
  final predecessorVisible = _withoutTrailingLineEnding(
    run.text.substring(0, split),
  );
  final successorVisible = _withoutTrailingLineEnding(
    run.text.substring(split + 1),
  );
  final predecessorSource = FlarkSourceRange(
    row.sourceUtf16.start,
    receipt.baseUtf16Start,
  );
  final resultEnd = row.sourceUtf16.end + _utf16Delta(receipt);
  final successorSource = FlarkSourceRange(
    receipt.resultSelectionUtf16,
    resultEnd,
  );
  final neutralSource = FlarkSourceRange(
    predecessorSource.end,
    successorSource.start,
  );
  if (neutralSource.length == 0) return null;
  final successorVisibleStart = receipt.resultSelectionUtf16 + 1;
  final cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.anyNoCrLfSplice,
    affectedBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd + 1,
    ),
    affectedUtf16: FlarkSourceRange(
      receipt.resultUtf16End,
      receipt.resultUtf16End + 1,
    ),
    triggerBytes: FlarkSourceRange(
      receipt.resultByteEnd + 1,
      receipt.resultByteEnd + 1,
    ),
    triggerUtf16: FlarkSourceRange(
      receipt.resultUtf16End + 1,
      receipt.resultUtf16End + 1,
    ),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    // The split itself proves this projection, but an edit at the visible
    // successor start can change the remaining lines into a different block
    // (for example a table). Admit one exact non-newline splice through the
    // cell, then require fresh parser certification before publication.
    chainResultCell: false,
  );
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: predecessorSource,
        projectionCurrent: true,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: predecessorSource,
          leadingText: '',
          text: predecessorVisible,
          globalUtf16Start: run.sourceUtf16Start,
          kind: 5,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: [
            FlarkCorePresentationRun(
              text: predecessorVisible,
              sourceUtf16Start: run.sourceUtf16Start,
              sourceUtf16End: run.sourceUtf16Start + predecessorVisible.length,
              sourceExact: true,
              styles: const {},
            ),
          ],
        ),
      ),
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: neutralSource,
        projectionCurrent: true,
        // This split is inside an existing physical line and its successor
        // already contains visible text. The inserted line ending therefore
        // represents a durable blank line, not the transient separator owned
        // by an empty editable successor.
        role: FlarkCoreCommittedPresentationSurfaceRole.visibleBlankSeparator,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: neutralSource,
          leadingText: '',
          text: '',
          globalUtf16Start: neutralSource.start,
          kind: 0,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: successorSource,
        projectionCurrent: true,
        projectionEditCells: [cell],
        role: FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: successorSource,
          leadingText: '',
          text: successorVisible,
          globalUtf16Start: successorVisibleStart,
          kind: 5,
          headingLevel: null,
          blockQuoteDepth: null,
          codeBlock: null,
          thematicBreak: false,
          ordinal: row.ordinal,
          runs: [
            FlarkCorePresentationRun(
              text: successorVisible,
              sourceUtf16Start: successorVisibleStart,
              sourceUtf16End: successorVisibleStart + successorVisible.length,
              sourceExact: true,
              styles: const {},
            ),
          ],
        ),
      ),
    ],
  );
}

FlarkCoreCommittedPresentationTransitionV1? _continueProvenTerminalList(
  FlarkCorePresentationRow row,
  FlarkCoreEditIntentReceiptV1 receipt,
) {
  final endingLength = _leadingLineEndingLength(receipt.replacement);
  if (endingLength == null ||
      !row.listItem ||
      _hasUncertifiedInlineProjection(row) ||
      row.runs.isEmpty ||
      receipt.baseUtf16Start != receipt.baseUtf16End ||
      receipt.baseUtf16Start != row.runs.last.sourceUtf16End ||
      receipt.resultSelectionUtf16 !=
          receipt.baseUtf16Start + receipt.replacement.length) {
    return null;
  }
  final prefix = receipt.replacement.substring(endingLength);
  if (prefix.isEmpty || prefix.contains('\n') || prefix.contains('\r')) {
    return null;
  }
  final runs = _mapRunsThroughCommittedSplice(row.runs, receipt);
  if (runs == null) return null;
  final prefixStart = receipt.baseUtf16Start + endingLength;
  final successorStart = receipt.resultSelectionUtf16;
  final predecessorSource = FlarkSourceRange(
    row.sourceUtf16.start,
    prefixStart,
  );
  final successorSource = FlarkSourceRange(prefixStart, successorStart);
  final cell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
    affectedBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd,
    ),
    affectedUtf16: FlarkSourceRange(successorStart, successorStart),
    triggerBytes: FlarkSourceRange(
      receipt.resultByteEnd,
      receipt.resultByteEnd,
    ),
    triggerUtf16: FlarkSourceRange(successorStart, successorStart),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  return FlarkCoreCommittedPresentationTransitionV1(
    clearPriorGap: true,
    surfaces: [
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: predecessorSource,
        projectionCurrent: true,
        presentation: FlarkCorePresentationRow(
          sourceUtf16: predecessorSource,
          leadingText: row.leadingText,
          text: row.text,
          globalUtf16Start: row.globalUtf16Start,
          kind: row.kind,
          headingLevel: row.headingLevel,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: row.codeBlock,
          thematicBreak: row.thematicBreak,
          listItem: true,
          ordinal: row.ordinal,
          runs: runs,
        ),
      ),
      FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: row.ordinal,
        sourceUtf16: successorSource,
        projectionCurrent: true,
        projectionEditCells: [cell],
        presentation: FlarkCorePresentationRow(
          sourceUtf16: successorSource,
          leadingText: prefix,
          text: '',
          globalUtf16Start: successorStart,
          kind: row.kind,
          headingLevel: null,
          blockQuoteDepth: row.blockQuoteDepth,
          codeBlock: null,
          thematicBreak: false,
          listItem: true,
          ordinal: row.ordinal,
          runs: const [],
        ),
      ),
    ],
  );
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

String _withoutTrailingLineEnding(String text) {
  if (text.endsWith('\r\n')) return text.substring(0, text.length - 2);
  if (text.endsWith('\n') || text.endsWith('\r')) {
    return text.substring(0, text.length - 1);
  }
  return text;
}

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

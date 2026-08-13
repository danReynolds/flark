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
  const FlarkCoreCommittedPresentationTransitionV1({
    this.gap,
    this.surface,
    this.clearPriorGap = false,
  }) : assert(gap != null || surface != null || clearPriorGap);

  final FlarkCoreCommittedPresentationGapV1? gap;
  final FlarkCoreCommittedPresentationSurfaceV1? surface;
  final bool clearPriorGap;
}

/// Applies the presentation consequence already classified by Rust.
///
/// This function does not recognize Markdown. It validates the typed receipt,
/// maps existing source runs through its exact splice, and returns the bounded
/// transition any Dart frontend may display until the parser certifies the
/// result revision. Returning null means the frontend must fall back to exact
/// current source for the affected region.
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
      final ordinal = priorActiveOrdinal;
      final lineEndingLength = switch (receipt.presentationTransition) {
        FlarkCoreEditPresentationTransitionV1.splitParagraph =>
          _doubledLineEndingLength(receipt.replacement),
        FlarkCoreEditPresentationTransitionV1.continueList =>
          _leadingLineEndingLength(receipt.replacement),
        FlarkCoreEditPresentationTransitionV1.continueBlockQuote =>
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
    case FlarkCoreEditPresentationTransitionV1.mergeParagraph:
      if (priorGapPending || activeRow == null || precedingRow == null) {
        return const FlarkCoreCommittedPresentationTransitionV1(
          clearPriorGap: true,
        );
      }
      final runs = _mapRunsThroughCommittedSplice([
        ...precedingRow.runs,
        ...activeRow.runs,
      ], receipt);
      if (runs == null) {
        return const FlarkCoreCommittedPresentationTransitionV1(
          clearPriorGap: true,
        );
      }
      final resultEnd = activeRow.sourceUtf16.end + _utf16Delta(receipt);
      final source = FlarkSourceRange(
        precedingRow.sourceUtf16.start,
        resultEnd,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        clearPriorGap: true,
        surface: FlarkCoreCommittedPresentationSurfaceV1(
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
      );
    case FlarkCoreEditPresentationTransitionV1.outdentList:
      if (activeRow == null || receipt.replacement.isNotEmpty) return null;
      final removed = receipt.baseUtf16End - receipt.baseUtf16Start;
      if (removed <= 0 || activeRow.leadingText.length < removed) return null;
      final runs = _mapRunsThroughCommittedSplice(activeRow.runs, receipt);
      if (runs == null) return null;
      final delta = _utf16Delta(receipt);
      final source = FlarkSourceRange(
        activeRow.sourceUtf16.start + delta,
        activeRow.sourceUtf16.end + delta,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        surface: FlarkCoreCommittedPresentationSurfaceV1(
          rowOrdinal: activeRow.ordinal,
          sourceUtf16: source,
          presentation: FlarkCorePresentationRow(
            sourceUtf16: source,
            leadingText: activeRow.leadingText.substring(removed),
            text: activeRow.text,
            globalUtf16Start: source.start,
            kind: activeRow.kind,
            headingLevel: activeRow.headingLevel,
            blockQuoteDepth: activeRow.blockQuoteDepth,
            codeBlock: activeRow.codeBlock,
            thematicBreak: activeRow.thematicBreak,
            ordinal: activeRow.ordinal,
            runs: runs,
          ),
        ),
      );
    case FlarkCoreEditPresentationTransitionV1.liftList:
    case FlarkCoreEditPresentationTransitionV1.exitList:
    case FlarkCoreEditPresentationTransitionV1.exitBlockQuote:
    case FlarkCoreEditPresentationTransitionV1.liftBlockQuote:
    case FlarkCoreEditPresentationTransitionV1.exitHeading:
    case FlarkCoreEditPresentationTransitionV1.liftHeading:
      if (activeRow == null) return null;
      final runs = _mapRunsThroughCommittedSplice(activeRow.runs, receipt);
      if (runs == null) return null;
      final resultEnd = activeRow.sourceUtf16.end + _utf16Delta(receipt);
      final source = FlarkSourceRange(
        receipt.resultUtf16End,
        resultEnd < receipt.resultUtf16End ? receipt.resultUtf16End : resultEnd,
      );
      return FlarkCoreCommittedPresentationTransitionV1(
        surface: FlarkCoreCommittedPresentationSurfaceV1(
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
      );
    case FlarkCoreEditPresentationTransitionV1.none:
      return null;
  }
}

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

int? _doubledLineEndingLength(String replacement) => switch (replacement) {
  '\n\n' || '\r\r' => 1,
  '\r\n\r\n' => 2,
  _ => null,
};

int? _leadingLineEndingLength(String replacement) {
  if (replacement.startsWith('\r\n')) return 2;
  if (replacement.startsWith('\n') || replacement.startsWith('\r')) return 1;
  return null;
}

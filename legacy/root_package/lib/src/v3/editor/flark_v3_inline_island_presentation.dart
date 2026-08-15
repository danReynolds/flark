import '../host/flark_v3_host_protocol.dart';
import '../runtime/public/flark_v3_document_query.dart';
import '../runtime/public/flark_v3_inline_facts.dart';
import '../source/flark_v3_source_document.dart';
import 'flark_v3_inline_projection.dart';
import 'flark_v3_source_projection.dart';

/// Why one bounded active island must remain exact source paint.
///
/// These are authority/completeness states, not Markdown classifications.
enum FlarkV3InlineIslandSourcePaintReason {
  structureNotCurrent,
  structureNotInlineBearing,
  islandNotCertified,
  inlineFactsAbsent,
  inlineFactsUnsupported,
  inlineFactsNotCurrent,
}

/// One fail-closed Dart presentation decision for the active editing island.
///
/// The resolver performs no source scanning and recognizes no Markdown. It
/// joins only an exact structural viewport and its optional parser-certified
/// whole-leaf inline facts.
sealed class FlarkV3InlineIslandPresentation {
  const FlarkV3InlineIslandPresentation._({required this.source});

  factory FlarkV3InlineIslandPresentation.resolve({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3DocumentStructuralQuery structuralQuery,
    required FlarkV3SourceSpan activeIsland,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) {
    FlarkV3SourcePaintInlineIslandPresentation fallback(
      FlarkV3InlineIslandSourcePaintReason reason,
    ) => FlarkV3SourcePaintInlineIslandPresentation._(
      source: activeIsland,
      reason: reason,
    );

    if (structuralQuery.sourceRevision != expectedSource.revision ||
        structuralQuery.structureRevision != expectedSource.revision) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.structureNotCurrent);
    }
    final inlineContentSource = structuralQuery.structure.inlineContentSource;
    if (!structuralQuery.structure.canCarryInlineFacts ||
        inlineContentSource == null ||
        structuralQuery.projection.kind != structuralQuery.structure.kind ||
        !_sameSpan(
          structuralQuery.projection.projectedSource,
          inlineContentSource,
        )) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.structureNotInlineBearing,
      );
    }
    if (!_sameSpan(structuralQuery.projection.projectedSource, activeIsland)) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.islandNotCertified);
    }

    final facts = structuralQuery.inlineFacts;
    if (facts == null) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.inlineFactsAbsent);
    }
    if (facts.disposition == FlarkV3InlineFactsDisposition.unsupported) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsUnsupported,
      );
    }
    if (facts.sourceVersion != expectedSource ||
        !_containsSpan(activeIsland, facts.source) ||
        facts.source.startUtf8 >= facts.source.endUtf8) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsNotCurrent,
      );
    }

    try {
      final inlineProjection = FlarkV3InlineProjection.fromValidatedFacts(
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
        facts: facts,
        markerPolicy: markerPolicy,
      );
      return FlarkV3AuthoritativeInlineIslandPresentation._(
        source: activeIsland,
        facts: facts,
        projection: inlineProjection,
        sourceProjection: _enclosingSourceProjection(
          sourceDocument: sourceDocument,
          expectedSource: expectedSource,
          enclosingSource: activeIsland,
          inlineProjection: inlineProjection,
        ),
      );
    } on FlarkV3InlineProjectionException {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsNotCurrent,
      );
    }
  }

  /// Resolves the exact inline island selected through recursive Green.
  ///
  /// Both Paragraph and inline cuts come from the installed parser sidecar.
  /// This resolver only joins those exact authorities and never scans source
  /// for container prefixes or Markdown delimiters.
  factory FlarkV3InlineIslandPresentation.resolveRecursiveGreenInlineLeaf({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3RecursiveGreenPointQuery recursiveQuery,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) {
    final paragraphSource = recursiveQuery.paragraphSource;
    final inlineSource = recursiveQuery.inlineSource;
    final activeIsland = inlineSource ?? recursiveQuery.source;
    FlarkV3SourcePaintInlineIslandPresentation fallback(
      FlarkV3InlineIslandSourcePaintReason reason,
    ) => FlarkV3SourcePaintInlineIslandPresentation._(
      source: activeIsland,
      reason: reason,
    );

    if (recursiveQuery.sourceRevision != expectedSource.revision ||
        recursiveQuery.structureRevision != expectedSource.revision) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.structureNotCurrent);
    }
    if (!(recursiveQuery.owner.kind?.isInlineBearing ?? false)) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.structureNotInlineBearing,
      );
    }
    if (paragraphSource == null ||
        inlineSource == null ||
        !_containsSpan(paragraphSource, inlineSource) ||
        !_containsSpan(paragraphSource, recursiveQuery.source)) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.islandNotCertified);
    }
    final facts = recursiveQuery.inlineFacts;
    if (facts == null) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.inlineFactsAbsent);
    }
    if (facts.disposition == FlarkV3InlineFactsDisposition.unsupported) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsUnsupported,
      );
    }
    if (facts.sourceVersion != expectedSource ||
        !_sameSpan(facts.source, inlineSource)) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsNotCurrent,
      );
    }

    try {
      final inlineProjection = FlarkV3InlineProjection.fromValidatedFacts(
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
        facts: facts,
        markerPolicy: markerPolicy,
      );
      return FlarkV3AuthoritativeInlineIslandPresentation._(
        source: inlineSource,
        facts: facts,
        projection: inlineProjection,
        sourceProjection: inlineProjection.sourceProjection,
      );
    } on FlarkV3InlineProjectionException {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsNotCurrent,
      );
    }
  }

  /// Compatibility wrapper for the original Paragraph-only entry point.
  factory FlarkV3InlineIslandPresentation.resolveRecursiveGreenParagraph({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3RecursiveGreenPointQuery recursiveQuery,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) => FlarkV3InlineIslandPresentation.resolveRecursiveGreenInlineLeaf(
    sourceDocument: sourceDocument,
    expectedSource: expectedSource,
    recursiveQuery: recursiveQuery,
    markerPolicy: markerPolicy,
  );

  /// Resolves parser-certified inline facts for the selected item of an exact
  /// tight bullet-list projection.
  ///
  /// Retained as a source-compatible entry point for the original bullet-only
  /// API. New list kinds share [resolveTightListItem].
  factory FlarkV3InlineIslandPresentation.resolveBulletListItem({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3DocumentStructuralQuery structuralQuery,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) {
    if (structuralQuery.structure.kind !=
        FlarkV3DocumentStructureKind.bulletList) {
      return FlarkV3SourcePaintInlineIslandPresentation._(
        source: structuralQuery.structure.source,
        reason: FlarkV3InlineIslandSourcePaintReason.structureNotInlineBearing,
      );
    }
    return FlarkV3InlineIslandPresentation.resolveTightListItem(
      sourceDocument: sourceDocument,
      expectedSource: expectedSource,
      structuralQuery: structuralQuery,
      markerPolicy: markerPolicy,
    );
  }

  /// Resolves parser-certified inline facts for the selected item of an exact
  /// tight ordered-list projection.
  factory FlarkV3InlineIslandPresentation.resolveOrderedListItem({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3DocumentStructuralQuery structuralQuery,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) {
    if (structuralQuery.structure.kind !=
        FlarkV3DocumentStructureKind.orderedList) {
      return FlarkV3SourcePaintInlineIslandPresentation._(
        source: structuralQuery.structure.source,
        reason: FlarkV3InlineIslandSourcePaintReason.structureNotInlineBearing,
      );
    }
    return FlarkV3InlineIslandPresentation.resolveTightListItem(
      sourceDocument: sourceDocument,
      expectedSource: expectedSource,
      structuralQuery: structuralQuery,
      markerPolicy: markerPolicy,
    );
  }

  /// Resolves parser-certified inline facts for the selected item of an exact
  /// tight-list projection.
  ///
  /// This is a join of two parser certificates, not a Dart-side recognition
  /// path: the cached list projection identifies the selected content range,
  /// and the inline sidecar must bind that exact range and source revision.
  factory FlarkV3InlineIslandPresentation.resolveTightListItem({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3DocumentStructuralQuery structuralQuery,
    FlarkV3InlineMarkerPolicy markerPolicy =
        FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
  }) {
    final projection = switch (structuralQuery.structure.kind) {
      FlarkV3DocumentStructureKind.bulletList =>
        structuralQuery.bulletListProjection,
      FlarkV3DocumentStructureKind.orderedList =>
        structuralQuery.orderedListProjection,
      _ => null,
    };
    final activeIsland = projection?.selectedItem.content;
    final fallbackSource = activeIsland ?? structuralQuery.structure.source;
    FlarkV3SourcePaintInlineIslandPresentation fallback(
      FlarkV3InlineIslandSourcePaintReason reason,
    ) => FlarkV3SourcePaintInlineIslandPresentation._(
      source: fallbackSource,
      reason: reason,
    );

    if (structuralQuery.sourceRevision != expectedSource.revision ||
        structuralQuery.structureRevision != expectedSource.revision) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.structureNotCurrent);
    }
    if (projection == null ||
        activeIsland == null ||
        projection.sourceVersion != expectedSource ||
        !_sameSpan(projection.source, structuralQuery.structure.source)) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.structureNotInlineBearing,
      );
    }

    final facts = structuralQuery.inlineFacts;
    if (facts == null) {
      return fallback(FlarkV3InlineIslandSourcePaintReason.inlineFactsAbsent);
    }
    if (facts.disposition == FlarkV3InlineFactsDisposition.unsupported) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsUnsupported,
      );
    }
    if (facts.sourceVersion != expectedSource ||
        !_sameSpan(facts.source, activeIsland)) {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsNotCurrent,
      );
    }

    try {
      final inlineProjection = FlarkV3InlineProjection.fromValidatedFacts(
        sourceDocument: sourceDocument,
        expectedSource: expectedSource,
        facts: facts,
        markerPolicy: markerPolicy,
      );
      return FlarkV3AuthoritativeInlineIslandPresentation._(
        source: activeIsland,
        facts: facts,
        projection: inlineProjection,
        sourceProjection: inlineProjection.sourceProjection,
      );
    } on FlarkV3InlineProjectionException {
      return fallback(
        FlarkV3InlineIslandSourcePaintReason.inlineFactsNotCurrent,
      );
    }
  }

  /// Exact active-island source range. Fallback never changes this range.
  final FlarkV3SourceSpan source;
}

/// A complete parser-authorized projection for one exact active island.
final class FlarkV3AuthoritativeInlineIslandPresentation
    extends FlarkV3InlineIslandPresentation {
  const FlarkV3AuthoritativeInlineIslandPresentation._({
    required super.source,
    required this.facts,
    required this.projection,
    required this.sourceProjection,
  }) : super._();

  final FlarkV3InlineFacts facts;
  final FlarkV3InlineProjection projection;

  /// Parser-bound enclosing structural source projection.
  ///
  /// [projection] may cover a strict inline subrange, while this projection
  /// retains structural source such as a Paragraph's terminal line ending.
  final FlarkV3SourceProjection sourceProjection;
}

/// Typed instruction to retain literal source for the complete active island.
final class FlarkV3SourcePaintInlineIslandPresentation
    extends FlarkV3InlineIslandPresentation {
  const FlarkV3SourcePaintInlineIslandPresentation._({
    required super.source,
    required this.reason,
  }) : super._();

  final FlarkV3InlineIslandSourcePaintReason reason;
}

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

bool _containsSpan(FlarkV3SourceSpan parent, FlarkV3SourceSpan child) =>
    parent.startUtf8 <= child.startUtf8 &&
    child.endUtf8 <= parent.endUtf8 &&
    parent.startUtf16 <= child.startUtf16 &&
    child.endUtf16 <= parent.endUtf16;

FlarkV3SourceProjection _enclosingSourceProjection({
  required FlarkV3SourceDocument sourceDocument,
  required FlarkV3SourceVersion expectedSource,
  required FlarkV3SourceSpan enclosingSource,
  required FlarkV3InlineProjection inlineProjection,
}) {
  if (enclosingSource.startUtf16 == inlineProjection.sourceStartUtf16 &&
      enclosingSource.endUtf16 == inlineProjection.sourceEndUtf16) {
    return inlineProjection.sourceProjection;
  }
  final sourceText = sourceDocument.readRange(
    enclosingSource.startUtf16,
    enclosingSource.endUtf16,
  );
  final maximumUtf16 =
      sourceText.length > FlarkV3SourceProjection.defaultMaximumSourceUtf16
      ? sourceText.length
      : FlarkV3SourceProjection.defaultMaximumSourceUtf16;
  return FlarkV3SourceProjection.fromSource(
    sourceStartUtf16: enclosingSource.startUtf16,
    sourceText: sourceText,
    pieces: [
      FlarkV3SourceProjectionPiece.copy(
        sourceStartUtf16: enclosingSource.startUtf16,
        sourceEndUtf16: enclosingSource.endUtf16,
      ),
    ],
    certifiedSourceVersion: expectedSource,
    maximumSourceUtf16: maximumUtf16,
    maximumDisplayUtf16: maximumUtf16,
  );
}

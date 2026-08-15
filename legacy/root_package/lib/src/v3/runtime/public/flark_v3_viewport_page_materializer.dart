import 'dart:typed_data';

import '../../editor/flark_v3_inline_projection.dart';
import '../../editor/flark_v3_source_projection.dart';
import '../../host/host.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_document_query.dart';
import 'flark_v3_indented_code_projection.dart';
import 'flark_v3_inline_facts.dart';
import 'flark_v3_visible_block_set.dart';

/// Exact authority shared by every block in one materialized viewport page.
///
/// [structureGeneration] is caller-local invalidation authority. The remaining
/// fields come from the exact structural ACK authenticated by schema 8.
final class FlarkV3MaterializedViewportIdentity {
  const FlarkV3MaterializedViewportIdentity({
    required this.sourceVersion,
    required this.sourceRoot,
    required this.parseGeneration,
    required this.structureGeneration,
    required this.viewportGeneration,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3SourceRootId sourceRoot;
  final int parseGeneration;
  final int structureGeneration;
  final int viewportGeneration;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3MaterializedViewportIdentity &&
      other.sourceVersion == sourceVersion &&
      other.sourceRoot == sourceRoot &&
      other.parseGeneration == parseGeneration &&
      other.structureGeneration == structureGeneration &&
      other.viewportGeneration == viewportGeneration;

  @override
  int get hashCode => Object.hash(
    sourceVersion,
    sourceRoot,
    parseGeneration,
    structureGeneration,
    viewportGeneration,
  );
}

/// Why an aggregate page could not be joined to exact-current structure.
///
/// A page-level fallback exposes no partially joined blocks. Callers retain
/// source truth and retry after the named authority becomes current.
enum FlarkV3ViewportPageFallbackReason {
  structuralBaseChanged,
  structuralGenerationChanged,
  sourceAuthorityUnavailable,
  sourceAuthorityChanged,
  requestedRangeChanged,
  structuralCoverageUnavailable,
  structuralCoverageInvalid,
  entryStructureMismatch,
}

/// Why one exact structural block must paint canonical source.
///
/// These are completeness/authority states, not Markdown classifications.
enum FlarkV3ViewportBlockFallbackReason {
  payloadAbsent,
  parserUnsupported,
  pointPathAuthorityAbsent,
  payloadRejected,
  structuralProjectionExceedsBound,
}

sealed class FlarkV3ViewportPageMaterialization {
  const FlarkV3ViewportPageMaterialization();
}

/// One exact schema-8 cut joined to consecutive structural blocks.
///
/// [blocks] contains every structural ordinal in the page cut, including
/// blocks for which schema 8 carried no presentation payload. Those blocks
/// are represented by [FlarkV3SourceFallbackViewportBlock].
final class FlarkV3ExactViewportPageMaterialization
    extends FlarkV3ViewportPageMaterialization {
  FlarkV3ExactViewportPageMaterialization._({
    required this.identity,
    required this.ack,
    required this.coveredSource,
    required List<FlarkV3MaterializedViewportBlock> blocks,
  }) : blocks = List<FlarkV3MaterializedViewportBlock>.unmodifiable(blocks);

  final FlarkV3MaterializedViewportIdentity identity;
  final FlarkV3ViewportPresentationAck ack;
  final FlarkV3SourceSpan coveredSource;
  final List<FlarkV3MaterializedViewportBlock> blocks;

  bool get complete => ack.binding.complete;
  int get firstOrdinal => blocks.first.ordinal;
  int get lastOrdinal => blocks.last.ordinal;
  bool get containsSourceFallback =>
      blocks.any((block) => block is FlarkV3SourceFallbackViewportBlock);
}

/// Fail-closed result for a page whose authority cannot be joined exactly.
final class FlarkV3SourceFallbackViewportPage
    extends FlarkV3ViewportPageMaterialization {
  const FlarkV3SourceFallbackViewportPage._({
    required this.reason,
    required this.requestedSource,
  });

  final FlarkV3ViewportPageFallbackReason reason;
  final FlarkV3SourceSpan requestedSource;
}

/// One structural block in an exact viewport cut.
sealed class FlarkV3MaterializedViewportBlock {
  const FlarkV3MaterializedViewportBlock._({
    required this.identity,
    required this.structuralBlock,
    required this.entry,
  });

  final FlarkV3MaterializedViewportIdentity identity;
  final FlarkV3DocumentStructuralBlock structuralBlock;

  /// Null only when the schema-8 page did not carry a child for this ordinal.
  final FlarkV3ViewportPresentationAggregateEntry? entry;

  int get ordinal => structuralBlock.ordinal;
  FlarkV3DocumentStructure get structure => structuralBlock.structure;
  FlarkV3DocumentStructureKind get kind => structure.kind;
  FlarkV3SourceSpan get physicalSource => structure.source;

  /// Exact parser-selected presentation source.
  ///
  /// For a missing child this remains the structural visible source. For an
  /// installed child it is the binding authenticated by schema 8.
  FlarkV3SourceSpan get visibleSource {
    final binding = entry?.binding;
    return binding == null
        ? structure.visibleSource
        : FlarkV3SourceSpan(
            startUtf8: binding.visibleStartUtf8,
            endUtf8: binding.visibleEndUtf8,
            startUtf16: binding.visibleStartUtf16,
            endUtf16: binding.visibleEndUtf16,
          );
  }

  int? get headingLevel => structure.heading?.level;
  bool get isAuthoritative => this is FlarkV3AuthoritativeViewportBlock;
}

/// Common display contract for fully decoded, parser-authored blocks.
abstract interface class FlarkV3AuthoritativeViewportBlock {
  String get displayText;
}

/// Marker-free whole-leaf inline presentation.
final class FlarkV3InlineViewportBlock extends FlarkV3MaterializedViewportBlock
    implements FlarkV3AuthoritativeViewportBlock {
  const FlarkV3InlineViewportBlock._({
    required super.identity,
    required super.structuralBlock,
    required super.entry,
    required this.facts,
    required this.projection,
  }) : super._();

  final FlarkV3InlineFacts facts;
  final FlarkV3InlineProjection projection;

  @override
  String get displayText => projection.displayText;

  List<FlarkV3InlineDisplayRun> get displayRuns => projection.runs;
}

/// Marker-free indented-code presentation built from physical-line records.
final class FlarkV3IndentedCodeViewportBlock
    extends FlarkV3MaterializedViewportBlock
    implements FlarkV3AuthoritativeViewportBlock {
  const FlarkV3IndentedCodeViewportBlock._({
    required super.identity,
    required super.structuralBlock,
    required super.entry,
    required this.payload,
    required this.projection,
  }) : super._();

  final FlarkV3IndentedCodeProjectionPayload payload;
  final FlarkV3SourceProjection projection;

  @override
  String get displayText => projection.displayText;
}

/// Marker-free fenced-code body named directly by structural parser facts.
///
/// No schema-8 child is needed: [bodySource] is already exact structural
/// authority and [displayText] is one bounded source read of that range.
final class FlarkV3FencedCodeViewportBlock
    extends FlarkV3MaterializedViewportBlock
    implements FlarkV3AuthoritativeViewportBlock {
  const FlarkV3FencedCodeViewportBlock._({
    required super.identity,
    required super.structuralBlock,
    required super.entry,
    required this.bodySource,
    required this.displayText,
  }) : super._();

  final FlarkV3SourceSpan bodySource;

  @override
  final String displayText;
}

/// Exact structural atom with no source-backed display text.
///
/// Empty boundaries and thematic breaks are rendered from [kind] by adapters;
/// there is no delimiter-removal or source classification step in Dart.
final class FlarkV3AtomicViewportBlock extends FlarkV3MaterializedViewportBlock
    implements FlarkV3AuthoritativeViewportBlock {
  const FlarkV3AtomicViewportBlock._({
    required super.identity,
    required super.structuralBlock,
    required super.entry,
  }) : super._();

  @override
  String get displayText => '';
}

/// Exact structural block whose complete parser-authored display is absent.
///
/// Consumers may paint [physicalSource] from current source authority. They
/// must not derive a marker-free projection from that source.
final class FlarkV3SourceFallbackViewportBlock
    extends FlarkV3MaterializedViewportBlock {
  const FlarkV3SourceFallbackViewportBlock._({
    required super.identity,
    required super.structuralBlock,
    required super.entry,
    required this.reason,
    required this.unsupportedReason,
  }) : super._();

  final FlarkV3ViewportBlockFallbackReason reason;

  /// Exact parser reason for an unsupported child; null for other fallbacks.
  final int? unsupportedReason;
}

enum FlarkV3RecursiveGreenRowPresentationDisposition {
  authoritative,
  unsupported,
}

/// One schema-11 row joined to its exact parser-authored display payload.
///
/// Container ancestry and editable geometry always come from [row]. Inline
/// display facts come only from the ACK/frame/row-bound aggregate [entry].
/// Unsupported rows expose no partial marker-free display.
final class FlarkV3MaterializedRecursiveGreenRow {
  FlarkV3MaterializedRecursiveGreenRow.authoritative({
    required this.identity,
    required this.structuralAck,
    required this.row,
    required this.entry,
    required this.displayText,
    this.inlineFacts,
    this.inlineProjection,
  }) : disposition =
           FlarkV3RecursiveGreenRowPresentationDisposition.authoritative,
       fallbackReason = null;

  FlarkV3MaterializedRecursiveGreenRow.unsupported({
    required this.identity,
    required this.structuralAck,
    required this.row,
    required this.entry,
    required this.fallbackReason,
  }) : disposition =
           FlarkV3RecursiveGreenRowPresentationDisposition.unsupported,
       displayText = '',
       inlineFacts = null,
       inlineProjection = null;

  final FlarkV3MaterializedViewportIdentity identity;
  final FlarkV3StructuralAck structuralAck;
  final FlarkV3RecursiveGreenRenderableRow row;
  final FlarkV3ViewportPresentationAggregateEntry? entry;
  final FlarkV3RecursiveGreenRowPresentationDisposition disposition;
  final String displayText;
  final FlarkV3InlineFacts? inlineFacts;
  final FlarkV3InlineProjection? inlineProjection;
  final FlarkV3ViewportBlockFallbackReason? fallbackReason;

  bool get isAuthoritative =>
      disposition ==
      FlarkV3RecursiveGreenRowPresentationDisposition.authoritative;
}

/// One exact recursive-Green row cut and all row-bound display payloads.
final class FlarkV3ExactRecursiveGreenViewportPageMaterialization
    extends FlarkV3ViewportPageMaterialization {
  FlarkV3ExactRecursiveGreenViewportPageMaterialization._({
    required this.identity,
    required this.ack,
    required this.coveredSource,
    required this.startGlobalRowOrdinal,
    required this.totalGlobalRowCount,
    required this.selectedRowIndex,
    required List<FlarkV3MaterializedRecursiveGreenRow> rows,
  }) : rows = List<FlarkV3MaterializedRecursiveGreenRow>.unmodifiable(rows);

  final FlarkV3MaterializedViewportIdentity identity;
  final FlarkV3ViewportPresentationAck ack;
  final FlarkV3SourceSpan coveredSource;
  final BigInt startGlobalRowOrdinal;
  final BigInt totalGlobalRowCount;
  final int? selectedRowIndex;
  final List<FlarkV3MaterializedRecursiveGreenRow> rows;

  bool get complete => ack.binding.complete;
  bool get containsSourceFallback => rows.any((row) => !row.isAuthoritative);
}

/// Joins one strict schema-8 page to exact-current structural/source authority.
///
/// This class performs no Markdown recognition. It validates the authenticated
/// source/base/range/ordinal join and delegates payload interpretation only to
/// existing parser-payload decoders.
final class FlarkV3ViewportPageMaterializer {
  const FlarkV3ViewportPageMaterializer();

  /// Joins one schema-11 recursive-Green directory to its row-keyed schema-10
  /// viewport aggregate.
  ///
  /// This validates exact ACK, global ordinal, frame, physical/editable span,
  /// and payload authority before invoking the existing inline decoder. It
  /// performs no Markdown recognition and source-paints unsupported rows.
  FlarkV3ViewportPageMaterialization materializeRecursiveGreenRows({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3StructuralAck currentStructuralAck,
    required int currentStructureGeneration,
    required FlarkV3RecursiveGreenRowRange rowRange,
    required FlarkV3ViewportPresentationAggregatePage page,
  }) {
    FlarkV3SourceFallbackViewportPage fallback(
      FlarkV3ViewportPageFallbackReason reason,
    ) => FlarkV3SourceFallbackViewportPage._(
      reason: reason,
      requestedSource: rowRange.requestedSource,
    );

    if (rowRange.structuralAck != currentStructuralAck ||
        page.ack.baseAck != currentStructuralAck ||
        page.wireSchema !=
            FlarkV3ViewportPresentationAggregatePage.recursiveGreenSchema) {
      return fallback(FlarkV3ViewportPageFallbackReason.structuralBaseChanged);
    }
    if (currentStructureGeneration < 0 ||
        rowRange.structureGeneration != currentStructureGeneration) {
      return fallback(
        FlarkV3ViewportPageFallbackReason.structuralGenerationChanged,
      );
    }
    final source = currentStructuralAck.sourceVersion;
    if (!sourceDocument.hasCertifiedFacts ||
        rowRange.sourceRevision != source.revision ||
        rowRange.structureRevision != source.revision ||
        sourceDocument.revision != source.revision ||
        sourceDocument.utf8Length != source.metric.bytes ||
        sourceDocument.utf16Length != source.metric.utf16 ||
        sourceDocument.contentHash128 != source.contentHash) {
      return fallback(FlarkV3ViewportPageFallbackReason.sourceAuthorityChanged);
    }
    final binding = page.ack.binding;
    // The structural query may begin inside the selected row. The passive
    // aggregate is instead demanded over the exact row cut, whose first byte
    // must be the first row's physical start. Both aggregate ranges therefore
    // bind to [coveredSource], while [requestedSource] remains only the
    // caller's locator demand.
    if (!_sameSpan(_span(binding.requestedRange), rowRange.coveredSource) ||
        !_sameSpan(_span(binding.coveredRange), rowRange.coveredSource) ||
        rowRange.rows.isEmpty ||
        _u64Value(binding.start.blockOrdinal) !=
            rowRange.rows.first.globalOrdinal ||
        _u64Value(binding.next.blockOrdinal) !=
            rowRange.rows.last.globalOrdinal + BigInt.one) {
      return fallback(FlarkV3ViewportPageFallbackReason.requestedRangeChanged);
    }

    final rowsByOrdinal = <BigInt, FlarkV3RecursiveGreenRenderableRow>{
      for (final row in rowRange.rows) row.globalOrdinal: row,
    };
    final entriesByOrdinal =
        <BigInt, FlarkV3ViewportPresentationAggregateEntry>{};
    for (final entry in page.entries) {
      final ordinal = entry.globalRowOrdinal;
      final frame = entry.recursiveGreenFrameId;
      if (ordinal == null || frame == null) {
        return fallback(
          FlarkV3ViewportPageFallbackReason.entryStructureMismatch,
        );
      }
      final ordinalValue = _u64Value(ordinal);
      final row = rowsByOrdinal[ordinalValue];
      if (row == null ||
          entriesByOrdinal.containsKey(ordinalValue) ||
          _u64Value(frame) != row.frameId ||
          !_entryMatchesRecursiveGreenRow(sourceDocument, entry, row)) {
        return fallback(
          FlarkV3ViewportPageFallbackReason.entryStructureMismatch,
        );
      }
      entriesByOrdinal[ordinalValue] = entry;
    }

    final identity = FlarkV3MaterializedViewportIdentity(
      sourceVersion: source,
      sourceRoot: currentStructuralAck.sourceRoot,
      parseGeneration: currentStructuralAck.parseGeneration,
      structureGeneration: currentStructureGeneration,
      viewportGeneration: binding.viewportGeneration,
    );
    final rows = <FlarkV3MaterializedRecursiveGreenRow>[
      for (final row in rowRange.rows)
        _materializeRecursiveGreenRow(
          sourceDocument: sourceDocument,
          source: source,
          structuralAck: currentStructuralAck,
          identity: identity,
          row: row,
          entry: entriesByOrdinal[row.globalOrdinal],
        ),
    ];
    return FlarkV3ExactRecursiveGreenViewportPageMaterialization._(
      identity: identity,
      ack: page.ack,
      coveredSource: rowRange.coveredSource,
      startGlobalRowOrdinal: rowRange.startGlobalRowOrdinal,
      totalGlobalRowCount: rowRange.totalGlobalRowCount,
      selectedRowIndex: rowRange.selectedRowIndex,
      rows: rows,
    );
  }

  FlarkV3ViewportPageMaterialization materialize({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3StructuralAck currentStructuralAck,
    required int currentStructureGeneration,
    required FlarkV3ExactVisibleBlockSet visibleBlocks,
    required FlarkV3ViewportPresentationAggregatePage page,
  }) {
    final requestedSource = _span(page.ack.binding.requestedRange);
    FlarkV3SourceFallbackViewportPage fallback(
      FlarkV3ViewportPageFallbackReason reason,
    ) => FlarkV3SourceFallbackViewportPage._(
      reason: reason,
      requestedSource: requestedSource,
    );

    if (page.ack.baseAck != currentStructuralAck) {
      return fallback(FlarkV3ViewportPageFallbackReason.structuralBaseChanged);
    }
    if (currentStructureGeneration < 0 ||
        visibleBlocks.demand.structureGeneration !=
            currentStructureGeneration) {
      return fallback(
        FlarkV3ViewportPageFallbackReason.structuralGenerationChanged,
      );
    }
    if (!sourceDocument.hasCertifiedFacts) {
      return fallback(
        FlarkV3ViewportPageFallbackReason.sourceAuthorityUnavailable,
      );
    }
    final source = currentStructuralAck.sourceVersion;
    if (sourceDocument.revision != source.revision ||
        sourceDocument.utf8Length != source.metric.bytes ||
        sourceDocument.utf16Length != source.metric.utf16 ||
        sourceDocument.contentHash128 != source.contentHash ||
        visibleBlocks.demand.sourceRevision != source.revision) {
      return fallback(FlarkV3ViewportPageFallbackReason.sourceAuthorityChanged);
    }

    final demand = visibleBlocks.demand;
    final requested = page.ack.binding.requestedRange;
    if (demand.startUtf16 != requested.startUtf16 ||
        demand.endUtf16 != requested.endUtf16 ||
        sourceDocument.utf16ToUtf8(demand.startUtf16) != requested.startUtf8 ||
        sourceDocument.utf16ToUtf8(demand.endUtf16) != requested.endUtf8) {
      return fallback(FlarkV3ViewportPageFallbackReason.requestedRangeChanged);
    }

    final cut = _selectStructuralCut(visibleBlocks, page.ack.binding);
    if (cut == null) {
      return fallback(
        FlarkV3ViewportPageFallbackReason.structuralCoverageUnavailable,
      );
    }
    if (!_validStructuralCut(sourceDocument, cut, page.ack.binding)) {
      return fallback(
        FlarkV3ViewportPageFallbackReason.structuralCoverageInvalid,
      );
    }

    final entriesByOrdinal = <int, FlarkV3ViewportPresentationAggregateEntry>{};
    for (final entry in page.entries) {
      final ordinal = entry.binding.blockOrdinal;
      if (!ordinal.fitsU32) {
        return fallback(
          FlarkV3ViewportPageFallbackReason.entryStructureMismatch,
        );
      }
      final block = cut.byOrdinal[ordinal.lowWord];
      if (block == null ||
          entriesByOrdinal.containsKey(ordinal.lowWord) ||
          !_entryMatchesStructure(sourceDocument, entry, block)) {
        return fallback(
          FlarkV3ViewportPageFallbackReason.entryStructureMismatch,
        );
      }
      entriesByOrdinal[ordinal.lowWord] = entry;
    }

    final identity = FlarkV3MaterializedViewportIdentity(
      sourceVersion: source,
      sourceRoot: currentStructuralAck.sourceRoot,
      parseGeneration: currentStructuralAck.parseGeneration,
      structureGeneration: currentStructureGeneration,
      viewportGeneration: page.ack.binding.viewportGeneration,
    );
    final blocks = <FlarkV3MaterializedViewportBlock>[
      for (final block in cut.blocks)
        _materializeBlock(
          sourceDocument: sourceDocument,
          source: source,
          identity: identity,
          block: block,
          entry: entriesByOrdinal[block.ordinal],
        ),
    ];
    return FlarkV3ExactViewportPageMaterialization._(
      identity: identity,
      ack: page.ack,
      coveredSource: _span(page.ack.binding.coveredRange),
      blocks: blocks,
    );
  }

  FlarkV3MaterializedRecursiveGreenRow _materializeRecursiveGreenRow({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion source,
    required FlarkV3StructuralAck structuralAck,
    required FlarkV3MaterializedViewportIdentity identity,
    required FlarkV3RecursiveGreenRenderableRow row,
    required FlarkV3ViewportPresentationAggregateEntry? entry,
  }) {
    FlarkV3MaterializedRecursiveGreenRow fallback(
      FlarkV3ViewportBlockFallbackReason reason,
    ) => FlarkV3MaterializedRecursiveGreenRow.unsupported(
      identity: identity,
      structuralAck: structuralAck,
      row: row,
      entry: entry,
      fallbackReason: reason,
    );

    final editableSource = row.editableSource;
    switch (row.editCapability) {
      case FlarkV3RecursiveGreenRowEditCapability.projectedReserved:
        return fallback(
          entry == null
              ? FlarkV3ViewportBlockFallbackReason.payloadAbsent
              : FlarkV3ViewportBlockFallbackReason.payloadRejected,
        );
      case FlarkV3RecursiveGreenRowEditCapability.unavailable:
        return fallback(
          entry == null
              ? FlarkV3ViewportBlockFallbackReason.parserUnsupported
              : FlarkV3ViewportBlockFallbackReason.payloadRejected,
        );
      case FlarkV3RecursiveGreenRowEditCapability.contiguous:
        if (editableSource == null) {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        }
    }

    if (row.kind.isTerminalEmptyItem) {
      if (entry != null ||
          row.presentationKind !=
              FlarkV3RecursiveGreenRowPresentationKind.inline ||
          editableSource.startUtf16 != editableSource.endUtf16 ||
          editableSource.startUtf8 != editableSource.endUtf8) {
        return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
      }
      return FlarkV3MaterializedRecursiveGreenRow.authoritative(
        identity: identity,
        structuralAck: structuralAck,
        row: row,
        entry: null,
        displayText: '',
      );
    }

    switch (row.presentationKind) {
      case FlarkV3RecursiveGreenRowPresentationKind.fencedCode:
        if (entry != null) {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        }
        return FlarkV3MaterializedRecursiveGreenRow.authoritative(
          identity: identity,
          structuralAck: structuralAck,
          row: row,
          entry: null,
          displayText: sourceDocument.readRange(
            editableSource.startUtf16,
            editableSource.endUtf16,
          ),
        );
      case FlarkV3RecursiveGreenRowPresentationKind.thematicBreak:
        if (entry != null ||
            editableSource.startUtf16 != editableSource.endUtf16) {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        }
        return FlarkV3MaterializedRecursiveGreenRow.authoritative(
          identity: identity,
          structuralAck: structuralAck,
          row: row,
          entry: null,
          displayText: '',
        );
      case FlarkV3RecursiveGreenRowPresentationKind.inline:
        if (entry == null) {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadAbsent);
        }
        if (!entry.isAuthoritative) {
          return fallback(FlarkV3ViewportBlockFallbackReason.parserUnsupported);
        }
        if (entry.payloadKind !=
            FlarkV3ViewportPresentationPayloadKind.inline) {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        }
        try {
          final factBytes =
              entry.recordCount * FlarkV3InlineFactsDecoder.recordBytes;
          if (entry.payload.lengthInBytes < factBytes) {
            return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
          }
          final encodedFacts = Uint8List.sublistView(
            entry.payload,
            0,
            factBytes,
          );
          final encodedValues = Uint8List.sublistView(entry.payload, factBytes);
          final inlineValues = encodedValues.isEmpty
              ? null
              : FlarkV3InlineValuesPayload(
                  sourceVersion: entry.sourceVersion,
                  profilePartition: entry.binding.parserProfile.value,
                  source: editableSource,
                  encodedBytes: encodedValues,
                );
          final facts = FlarkV3InlineFactsDecoder.decode(
            sourceDocument: sourceDocument,
            expectedSource: source,
            factSource: entry.sourceVersion,
            expectedProfilePartition: entry.binding.parserProfile.value,
            profilePartition: entry.binding.parserProfile.value,
            expectedLeaf: editableSource,
            factLeaf: editableSource,
            disposition: FlarkV3InlineFactsDisposition.authoritative,
            factCount: entry.recordCount,
            encodedFacts: encodedFacts,
            inlineValues: inlineValues,
          );
          final projection = FlarkV3InlineProjection.fromValidatedFacts(
            sourceDocument: sourceDocument,
            expectedSource: source,
            facts: facts,
            markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          );
          return FlarkV3MaterializedRecursiveGreenRow.authoritative(
            identity: identity,
            structuralAck: structuralAck,
            row: row,
            entry: entry,
            displayText: projection.displayText,
            inlineFacts: facts,
            inlineProjection: projection,
          );
        } on FlarkV3InlineFactsDecodeException {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        } on FlarkV3InlineProjectionException {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        } on RangeError {
          return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
        }
      case FlarkV3RecursiveGreenRowPresentationKind.indentedCode:
      case FlarkV3RecursiveGreenRowPresentationKind.html:
        return fallback(FlarkV3ViewportBlockFallbackReason.payloadAbsent);
    }
  }

  FlarkV3MaterializedViewportBlock _materializeBlock({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion source,
    required FlarkV3MaterializedViewportIdentity identity,
    required FlarkV3DocumentStructuralBlock block,
    required FlarkV3ViewportPresentationAggregateEntry? entry,
  }) {
    FlarkV3SourceFallbackViewportBlock fallback(
      FlarkV3ViewportBlockFallbackReason reason, {
      int? unsupportedReason,
    }) => FlarkV3SourceFallbackViewportBlock._(
      identity: identity,
      structuralBlock: block,
      entry: entry,
      reason: reason,
      unsupportedReason: unsupportedReason,
    );

    final structure = block.structure;
    if (structure.kind == FlarkV3DocumentStructureKind.fencedCode) {
      final body = structure.fencedCode!.bodySource;
      final bodyBytes = body.endUtf8 - body.startUtf8;
      final bodyUtf16 = body.endUtf16 - body.startUtf16;
      if (bodyBytes > FlarkV3InlineFacts.maximumWholeLeafSourceBytes ||
          bodyUtf16 > FlarkV3InlineFacts.maximumWholeLeafSourceBytes) {
        return fallback(
          FlarkV3ViewportBlockFallbackReason.structuralProjectionExceedsBound,
        );
      }
      return FlarkV3FencedCodeViewportBlock._(
        identity: identity,
        structuralBlock: block,
        entry: entry,
        bodySource: body,
        displayText: sourceDocument.readRange(body.startUtf16, body.endUtf16),
      );
    }
    if (structure.kind == FlarkV3DocumentStructureKind.empty ||
        structure.kind == FlarkV3DocumentStructureKind.thematicBreak ||
        structure.unknownReason == FlarkV3DocumentUnknownReason.blankBoundary) {
      return FlarkV3AtomicViewportBlock._(
        identity: identity,
        structuralBlock: block,
        entry: entry,
      );
    }
    if (entry == null) {
      return fallback(FlarkV3ViewportBlockFallbackReason.payloadAbsent);
    }
    if (!entry.isAuthoritative) {
      return fallback(
        FlarkV3ViewportBlockFallbackReason.parserUnsupported,
        unsupportedReason: entry.unsupportedReason,
      );
    }

    try {
      switch (entry.payloadKind) {
        case FlarkV3ViewportPresentationPayloadKind.inline:
          final leaf = block.structure.inlineContentSource!;
          final factBytes =
              entry.recordCount * FlarkV3InlineFactsDecoder.recordBytes;
          if (entry.payload.lengthInBytes < factBytes) {
            return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
          }
          final encodedFacts = Uint8List.sublistView(
            entry.payload,
            0,
            factBytes,
          );
          final encodedValues = Uint8List.sublistView(entry.payload, factBytes);
          final inlineValues = encodedValues.isEmpty
              ? null
              : FlarkV3InlineValuesPayload(
                  sourceVersion: entry.sourceVersion,
                  profilePartition: entry.binding.parserProfile.value,
                  source: leaf,
                  encodedBytes: encodedValues,
                );
          final facts = FlarkV3InlineFactsDecoder.decode(
            sourceDocument: sourceDocument,
            expectedSource: source,
            factSource: entry.sourceVersion,
            expectedProfilePartition: entry.binding.parserProfile.value,
            profilePartition: entry.binding.parserProfile.value,
            expectedLeaf: leaf,
            factLeaf: leaf,
            disposition: FlarkV3InlineFactsDisposition.authoritative,
            factCount: entry.recordCount,
            encodedFacts: encodedFacts,
            inlineValues: inlineValues,
          );
          final projection = FlarkV3InlineProjection.fromValidatedFacts(
            sourceDocument: sourceDocument,
            expectedSource: source,
            facts: facts,
            markerPolicy: FlarkV3InlineMarkerPolicy.hideCertifiedMarkers,
          );
          return FlarkV3InlineViewportBlock._(
            identity: identity,
            structuralBlock: block,
            entry: entry,
            facts: facts,
            projection: projection,
          );
        case FlarkV3ViewportPresentationPayloadKind.indentedCode:
          final payload = FlarkV3IndentedCodeProjectionDecoder.decode(
            sourceDocument: sourceDocument,
            expectedSource: source,
            source: block.structure.source,
            facts: block.structure.indentedCode!,
            encodedRecords: entry.payload,
          );
          return FlarkV3IndentedCodeViewportBlock._(
            identity: identity,
            structuralBlock: block,
            entry: entry,
            payload: payload,
            projection: payload.toSourceProjection(),
          );
        case FlarkV3ViewportPresentationPayloadKind.blockQuote:
        case FlarkV3ViewportPresentationPayloadKind.bulletList:
        case FlarkV3ViewportPresentationPayloadKind.orderedListItem:
          // The existing decoders require an authenticated point path. Schema
          // 8 carries no point-path table, and a structure-only range cannot
          // truthfully synthesize one.
          return fallback(
            FlarkV3ViewportBlockFallbackReason.pointPathAuthorityAbsent,
          );
        case FlarkV3ViewportPresentationPayloadKind.unsupported:
          return fallback(
            FlarkV3ViewportBlockFallbackReason.parserUnsupported,
            unsupportedReason: entry.unsupportedReason,
          );
      }
    } on FlarkV3InlineFactsDecodeException {
      return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
    } on FlarkV3InlineProjectionException {
      return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
    } on FlarkV3IndentedCodeProjectionDecodeException {
      return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
    } on RangeError {
      return fallback(FlarkV3ViewportBlockFallbackReason.payloadRejected);
    }
  }
}

final class _StructuralCut {
  const _StructuralCut({required this.blocks, required this.byOrdinal});

  final List<FlarkV3DocumentStructuralBlock> blocks;
  final Map<int, FlarkV3DocumentStructuralBlock> byOrdinal;
}

_StructuralCut? _selectStructuralCut(
  FlarkV3ExactVisibleBlockSet visibleBlocks,
  FlarkV3ViewportPresentationBinding binding,
) {
  if (!binding.start.blockOrdinal.fitsU32 || visibleBlocks.blocks.isEmpty) {
    return null;
  }
  final firstOrdinal = binding.start.blockOrdinal.lowWord;
  final selected = <FlarkV3DocumentStructuralBlock>[];
  var started = false;
  for (final block in visibleBlocks.blocks) {
    if (!started) {
      if (block.ordinal != firstOrdinal) continue;
      started = true;
    }
    if (_ordinalAtOrAfterCut(block.ordinal, binding.next.blockOrdinal)) break;
    selected.add(block);
  }
  if (selected.isEmpty ||
      _incrementOrdinal(selected.last.ordinal) != binding.next.blockOrdinal) {
    return null;
  }
  return _StructuralCut(
    blocks: List.unmodifiable(selected),
    byOrdinal: Map.unmodifiable({
      for (final block in selected) block.ordinal: block,
    }),
  );
}

bool _validStructuralCut(
  FlarkV3SourceDocument sourceDocument,
  _StructuralCut cut,
  FlarkV3ViewportPresentationBinding binding,
) {
  final covered = _span(binding.coveredRange);
  final first = cut.blocks.first.structure.source;
  final last = cut.blocks.last.structure.source;
  if (first.startUtf8 != covered.startUtf8 ||
      first.startUtf16 != covered.startUtf16 ||
      last.endUtf8 != covered.endUtf8 ||
      last.endUtf16 != covered.endUtf16) {
    return false;
  }

  FlarkV3DocumentStructuralBlock? previous;
  for (final block in cut.blocks) {
    final structure = block.structure;
    final projection = block.projection;
    final exactBlankBoundary =
        structure.unknownReason == FlarkV3DocumentUnknownReason.blankBoundary &&
        structure.visibleSource.startUtf8 == structure.source.startUtf8 &&
        structure.visibleSource.endUtf8 == structure.source.startUtf8 &&
        structure.visibleSource.startUtf16 == structure.source.startUtf16 &&
        structure.visibleSource.endUtf16 == structure.source.startUtf16 &&
        _sameSpan(projection.projectedSource, structure.source);
    if (projection.kind != structure.kind ||
        !_sameSpan(projection.source, structure.source) ||
        !exactBlankBoundary &&
            !_sameSpan(projection.projectedSource, structure.visibleSource) ||
        !_validSourceSpan(sourceDocument, structure.source) ||
        !_validSourceSpan(sourceDocument, structure.visibleSource) ||
        !_contains(structure.source, structure.visibleSource)) {
      return false;
    }
    if (previous != null &&
        (block.ordinal != previous.ordinal + 1 ||
            structure.source.startUtf8 != previous.structure.source.endUtf8 ||
            structure.source.startUtf16 !=
                previous.structure.source.endUtf16)) {
      return false;
    }
    previous = block;
  }
  return true;
}

bool _entryMatchesStructure(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3ViewportPresentationAggregateEntry entry,
  FlarkV3DocumentStructuralBlock block,
) {
  final binding = entry.binding;
  final physical = FlarkV3SourceSpan(
    startUtf8: binding.physicalStartUtf8,
    endUtf8: binding.physicalEndUtf8,
    startUtf16: binding.physicalStartUtf16,
    endUtf16: binding.physicalEndUtf16,
  );
  final visible = FlarkV3SourceSpan(
    startUtf8: binding.visibleStartUtf8,
    endUtf8: binding.visibleEndUtf8,
    startUtf16: binding.visibleStartUtf16,
    endUtf16: binding.visibleEndUtf16,
  );
  if (!_sameSpan(physical, block.structure.source) ||
      !_validSourceSpan(sourceDocument, visible)) {
    return false;
  }

  final expectedVisible = switch (entry.payloadKind) {
    FlarkV3ViewportPresentationPayloadKind.inline =>
      block.structure.inlineContentSource,
    FlarkV3ViewportPresentationPayloadKind.indentedCode ||
    FlarkV3ViewportPresentationPayloadKind.blockQuote ||
    FlarkV3ViewportPresentationPayloadKind.bulletList ||
    FlarkV3ViewportPresentationPayloadKind.orderedListItem =>
      block.structure.source,
    FlarkV3ViewportPresentationPayloadKind.unsupported => null,
  };
  final kindMatches = switch (entry.payloadKind) {
    FlarkV3ViewportPresentationPayloadKind.inline =>
      block.structure.kind == FlarkV3DocumentStructureKind.paragraph ||
          block.structure.kind == FlarkV3DocumentStructureKind.heading,
    FlarkV3ViewportPresentationPayloadKind.indentedCode =>
      block.structure.kind == FlarkV3DocumentStructureKind.indentedCode &&
          block.structure.indentedCode != null,
    FlarkV3ViewportPresentationPayloadKind.blockQuote =>
      block.structure.kind == FlarkV3DocumentStructureKind.blockQuote &&
          block.structure.blockQuote != null,
    FlarkV3ViewportPresentationPayloadKind.bulletList =>
      block.structure.kind == FlarkV3DocumentStructureKind.bulletList &&
          block.structure.bulletList != null,
    FlarkV3ViewportPresentationPayloadKind.orderedListItem =>
      block.structure.kind == FlarkV3DocumentStructureKind.orderedList &&
          block.structure.orderedList != null,
    FlarkV3ViewportPresentationPayloadKind.unsupported => true,
  };
  if (!kindMatches) return false;
  return expectedVisible == null
      ? _contains(block.structure.source, visible)
      : _sameSpan(expectedVisible, visible);
}

bool _entryMatchesRecursiveGreenRow(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3ViewportPresentationAggregateEntry entry,
  FlarkV3RecursiveGreenRenderableRow row,
) {
  final rowEditable = row.editableSource;
  if (row.editCapability != FlarkV3RecursiveGreenRowEditCapability.contiguous ||
      rowEditable == null) {
    return false;
  }
  final binding = entry.binding;
  final physical = FlarkV3SourceSpan(
    startUtf8: binding.physicalStartUtf8,
    endUtf8: binding.physicalEndUtf8,
    startUtf16: binding.physicalStartUtf16,
    endUtf16: binding.physicalEndUtf16,
  );
  final editable = FlarkV3SourceSpan(
    startUtf8: binding.visibleStartUtf8,
    endUtf8: binding.visibleEndUtf8,
    startUtf16: binding.visibleStartUtf16,
    endUtf16: binding.visibleEndUtf16,
  );
  return _sameSpan(physical, row.physicalSource) &&
      _sameSpan(editable, rowEditable) &&
      _validSourceSpan(sourceDocument, physical) &&
      _validSourceSpan(sourceDocument, editable) &&
      _contains(physical, editable) &&
      row.presentationKind == FlarkV3RecursiveGreenRowPresentationKind.inline &&
      entry.payloadKind == FlarkV3ViewportPresentationPayloadKind.inline;
}

BigInt _u64Value(FlarkV3ProtocolU64 value) =>
    BigInt.from(value.lowWord) | (BigInt.from(value.highWord) << 32);

bool _validSourceSpan(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3SourceSpan span,
) =>
    span.endUtf8 <= sourceDocument.utf8Length &&
    span.endUtf16 <= sourceDocument.utf16Length &&
    sourceDocument.utf8ToUtf16(span.startUtf8) == span.startUtf16 &&
    sourceDocument.utf8ToUtf16(span.endUtf8) == span.endUtf16;

bool _contains(FlarkV3SourceSpan outer, FlarkV3SourceSpan inner) =>
    inner.startUtf8 >= outer.startUtf8 &&
    inner.endUtf8 <= outer.endUtf8 &&
    inner.startUtf16 >= outer.startUtf16 &&
    inner.endUtf16 <= outer.endUtf16;

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

FlarkV3SourceSpan _span(FlarkV3ViewportPresentationMetricRange range) =>
    FlarkV3SourceSpan(
      startUtf8: range.startUtf8,
      endUtf8: range.endUtf8,
      startUtf16: range.startUtf16,
      endUtf16: range.endUtf16,
    );

bool _ordinalAtOrAfterCut(int ordinal, FlarkV3ProtocolU64 cut) =>
    cut.highWord == 0 && ordinal >= cut.lowWord;

FlarkV3ProtocolU64 _incrementOrdinal(int ordinal) => ordinal == 0xffffffff
    ? FlarkV3ProtocolU64(lowWord: 0, highWord: 1)
    : FlarkV3ProtocolU64.fromU32(ordinal + 1);

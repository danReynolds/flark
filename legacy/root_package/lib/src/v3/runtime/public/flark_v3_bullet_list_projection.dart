import 'dart:typed_data';

import '../../editor/flark_v3_source_projection.dart';
import '../../host/flark_v3_host_protocol.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_document_query.dart';

/// Parser-authored source geometry shared by compact tight-list item kinds.
///
/// Concrete payloads may carry kind-specific facts, such as an ordered
/// marker's exact source span. Consumers that only need marker-free editing
/// use this interface and do not branch on Markdown syntax.
abstract interface class FlarkV3TightListItemProjectionRecord {
  int get ordinal;
  int get relativeItemStartUtf8;
  FlarkV3SourceSpan get physicalSource;
  FlarkV3SourceSpan get hiddenPrefix;
  FlarkV3SourceSpan get continuationPrefix;
  FlarkV3SourceSpan get content;
  FlarkV3SourceSpan get lineEnding;
  bool get isEmpty;
  int get projectedUtf8Length;
  int get projectedUtf16Length;
  int get displayUtf16Length;
}

/// One parser-authored physical item in a tight bullet-list projection.
final class FlarkV3BulletListItemProjectionRecord
    implements FlarkV3TightListItemProjectionRecord {
  const FlarkV3BulletListItemProjectionRecord._({
    required this.ordinal,
    required this.relativeItemStartUtf8,
    required this.physicalSource,
    required this.hiddenPrefix,
    required this.continuationPrefix,
    required this.content,
    required this.lineEnding,
  });

  @override
  final int ordinal;
  @override
  final int relativeItemStartUtf8;
  @override
  final FlarkV3SourceSpan physicalSource;
  @override
  final FlarkV3SourceSpan hiddenPrefix;

  /// Exact parser-authored continuation and prefix-removal authority.
  ///
  /// This may be an interior subspan of [hiddenPrefix], for example `- ` in
  /// the terminal-empty source prefix `-   `.
  @override
  final FlarkV3SourceSpan continuationPrefix;

  @override
  final FlarkV3SourceSpan content;
  @override
  final FlarkV3SourceSpan lineEnding;

  @override
  bool get isEmpty => content.startUtf8 == content.endUtf8;
  @override
  int get projectedUtf8Length => physicalSource.endUtf8 - hiddenPrefix.endUtf8;
  @override
  int get projectedUtf16Length =>
      physicalSource.endUtf16 - hiddenPrefix.endUtf16;

  /// UTF-16 display length after the physical CR, LF, or CRLF is projected to
  /// one logical LF. Parser-authored projected metrics above remain physical
  /// source metrics and therefore count CRLF as two UTF-16 code units.
  @override
  int get displayUtf16Length =>
      content.endUtf16 -
      content.startUtf16 +
      (lineEnding.startUtf16 == lineEnding.endUtf16 ? 0 : 1);
}

/// Exact, parser-authorized inputs for local editing of the selected item.
///
/// These are source values and capabilities, not a request for an adapter to
/// recognize Markdown. The canonical line-ending fallback is deterministic
/// editing policy when the selected EOF item and every predecessor omit one.
final class FlarkV3TightListItemEditingInputs {
  /// Creates exact edit inputs already certified by a projection decoder.
  ///
  /// Constructing this value alone grants no source authority. The enclosing
  /// projection payload and source version remain the authority boundary.
  const FlarkV3TightListItemEditingInputs.parserAuthored({
    required this.activeHiddenSourcePrefix,
    required this.activeRemovableSourcePrefix,
    required this.activeRemovableSourcePrefixOffsetUtf16,
    required this.continuationSourcePrefix,
    required this.canonicalLineEnding,
    required this.emptyEnterExits,
    required this.backspaceAtStartRemovesPrefix,
  });

  final String activeHiddenSourcePrefix;
  final String activeRemovableSourcePrefix;

  /// Offset of [activeRemovableSourcePrefix] inside
  /// [activeHiddenSourcePrefix].
  final int activeRemovableSourcePrefixOffsetUtf16;

  final String continuationSourcePrefix;
  final String canonicalLineEnding;
  final bool emptyEnterExits;
  final bool backspaceAtStartRemovesPrefix;
}

/// Source-compatible name for the original bullet-only editing input type.
typedef FlarkV3TightBulletListItemEditingInputs =
    FlarkV3TightListItemEditingInputs;

/// Marker-free editing seam shared by parser-certified tight list kinds.
///
/// Syntax-specific facts and marker paint data remain on concrete payloads.
/// This interface owns only exact selected-item geometry and edit mechanics.
abstract interface class FlarkV3TightListItemProjectionPayload {
  FlarkV3SourceVersion get sourceVersion;
  int get sourceRevision;
  FlarkV3SourceSpan get source;
  FlarkV3SourceSpan get projectionSource;
  FlarkV3DocumentPointPath get pointPath;
  List<FlarkV3TightListItemProjectionRecord> get records;
  int get selectedItemOrdinal;
  FlarkV3TightListItemEditingInputs get editingInputs;
  List<FlarkV3SourceProjectionPiece> get projectionPieces;
  bool get coversWholeList;
  FlarkV3TightListItemProjectionRecord get selectedItem;
  int get selectedItemDisplayUtf16Length;

  FlarkV3SourceProjection toSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  });

  FlarkV3SourceProjection toSelectedItemSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  });
}

/// Exact marker-free list projection and selected-item editing authority.
final class FlarkV3BulletListProjectionPayload
    implements FlarkV3TightListItemProjectionPayload {
  const FlarkV3BulletListProjectionPayload._({
    required this.sourceVersion,
    required this.source,
    required this.projectionSource,
    required this.facts,
    required this.pointPath,
    required this.records,
    required this.selectedItemOrdinal,
    required this.editingInputs,
    required this.projectionPieces,
    required this.coversWholeList,
    required String sourceText,
  }) : _sourceText = sourceText;

  @override
  final FlarkV3SourceVersion sourceVersion;
  @override
  int get sourceRevision => sourceVersion.revision;

  @override
  final FlarkV3SourceSpan source;
  @override
  final FlarkV3SourceSpan projectionSource;
  final FlarkV3BulletListFacts facts;
  @override
  final FlarkV3DocumentPointPath pointPath;
  @override
  final List<FlarkV3BulletListItemProjectionRecord> records;
  @override
  final int selectedItemOrdinal;
  @override
  final FlarkV3TightListItemEditingInputs editingInputs;
  @override
  final List<FlarkV3SourceProjectionPiece> projectionPieces;
  @override
  final bool coversWholeList;

  @override
  FlarkV3BulletListItemProjectionRecord get selectedItem =>
      records.firstWhere((record) => record.ordinal == selectedItemOrdinal);
  @override
  int get selectedItemDisplayUtf16Length => selectedItem.displayUtf16Length;

  final String _sourceText;

  /// Builds the marker-free, LF-normalized display covered by this payload.
  ///
  /// Whole-list schema 5 covers [source]. Compact schema 6 covers only
  /// [projectionSource], which is the selected item.
  @override
  FlarkV3SourceProjection toSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  }) {
    return FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: projectionSource.startUtf16,
      sourceText: _sourceText,
      pieces: projectionPieces,
      certifiedSourceVersion: sourceVersion,
      maximumSourceUtf16: maximumSourceUtf16,
      maximumDisplayUtf16: maximumDisplayUtf16,
    );
  }

  /// Builds the independently editable projection for the selected item.
  ///
  /// The whole-list projection is useful for read/display composition; the
  /// selected item remains the bounded editor island named by [pointPath].
  @override
  FlarkV3SourceProjection toSelectedItemSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  }) {
    final item = selectedItem;
    final itemSourceStart =
        item.physicalSource.startUtf16 - projectionSource.startUtf16;
    final itemSourceEnd =
        item.physicalSource.endUtf16 - projectionSource.startUtf16;
    final lineEndingStart =
        item.lineEnding.startUtf16 - projectionSource.startUtf16;
    final lineEndingEnd =
        item.lineEnding.endUtf16 - projectionSource.startUtf16;
    final itemPieces = <FlarkV3SourceProjectionPiece>[];
    _appendVisibleItemPieces(
      itemPieces,
      item,
      lineEndingText: _sourceText.substring(lineEndingStart, lineEndingEnd),
    );
    return FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: item.physicalSource.startUtf16,
      sourceText: _sourceText.substring(itemSourceStart, itemSourceEnd),
      pieces: itemPieces,
      certifiedSourceVersion: sourceVersion,
      maximumSourceUtf16: maximumSourceUtf16,
      maximumDisplayUtf16: maximumDisplayUtf16,
    );
  }
}

/// A corrupt or source-incompatible bullet-list projection payload.
final class FlarkV3BulletListProjectionDecodeException implements Exception {
  const FlarkV3BulletListProjectionDecodeException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3BulletListProjectionDecodeException($message)';
}

/// Decoder for canonical 28-byte little-endian item records.
///
/// Each record carries relative item start, physical length, hidden-prefix
/// length, exact continuation-prefix start/end relative to the item, content
/// UTF-8 length, and content UTF-16 length. Dart checks the source-backed
/// recipe and path against parser facts, then mechanically materializes
/// projection and editing values.
final class FlarkV3BulletListProjectionDecoder {
  const FlarkV3BulletListProjectionDecoder._();

  static const int recordBytes = 28;

  static FlarkV3BulletListProjectionPayload decode({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceSpan source,
    required FlarkV3BulletListFacts facts,
    required FlarkV3DocumentPointPath pointPath,
    required Uint8List encodedRecords,
  }) {
    if (!sourceDocument.hasCertifiedFacts) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Projection decoding requires certified source coordinates.',
      );
    }
    _validateSourceAuthority(sourceDocument, expectedSource);
    final mapper = _Utf8SpanMapper(sourceDocument);
    _validateBlockSource(source, sourceDocument, mapper);
    if (encodedRecords.lengthInBytes % recordBytes != 0) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Bullet-list projection is not a whole number of canonical records.',
      );
    }
    final recordCount = encodedRecords.lengthInBytes ~/ recordBytes;
    if (recordCount == 0 || recordCount != facts.itemCount) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Item count does not match the structural list summary.',
      );
    }

    final blockLengthUtf8 = source.endUtf8 - source.startUtf8;
    final data = ByteData.sublistView(encodedRecords);
    final records = <FlarkV3BulletListItemProjectionRecord>[];
    final pieces = <FlarkV3SourceProjectionPiece>[];
    var relativeCursor = 0;
    var paragraphCount = 0;
    var projectedUtf8Length = 0;
    var projectedUtf16Length = 0;
    int? terminalEmptyRelativeStartUtf8;

    for (var index = 0; index < recordCount; index += 1) {
      final offset = index * recordBytes;
      final relativeItemStart = data.getUint32(offset, Endian.little);
      final physicalLength = data.getUint32(offset + 4, Endian.little);
      final hiddenPrefixLength = data.getUint32(offset + 8, Endian.little);
      final continuationPrefixStart = data.getUint32(
        offset + 12,
        Endian.little,
      );
      final continuationPrefixEnd = data.getUint32(offset + 16, Endian.little);
      final contentLengthUtf8 = data.getUint32(offset + 20, Endian.little);
      final authoredContentLengthUtf16 = data.getUint32(
        offset + 24,
        Endian.little,
      );
      if (relativeItemStart != relativeCursor ||
          physicalLength == 0 ||
          hiddenPrefixLength == 0) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'Item records do not form a prefixed contiguous physical tiling.',
        );
      }

      final relativeItemEnd = _checkedU32End(
        relativeItemStart,
        physicalLength,
        'physical item',
      );
      if (relativeItemEnd > blockLengthUtf8) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'A list item escapes its exact block.',
        );
      }
      final prefixAndContent = _checkedU32End(
        hiddenPrefixLength,
        contentLengthUtf8,
        'item prefix and content',
      );
      if (prefixAndContent > physicalLength) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'An item prefix and content escape their physical line.',
        );
      }
      if (continuationPrefixStart >= continuationPrefixEnd ||
          continuationPrefixEnd > hiddenPrefixLength) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'An item continuation authority escapes its hidden prefix.',
        );
      }
      final lineEndingLength = physicalLength - prefixAndContent;
      if (lineEndingLength > 2 ||
          lineEndingLength == 0 && index != recordCount - 1) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'A list item has invalid physical line-ending geometry.',
        );
      }

      final empty = contentLengthUtf8 == 0;
      if (empty != (authoredContentLengthUtf16 == 0) ||
          empty && index != recordCount - 1) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'Only the final item may carry zero-content terminal geometry.',
        );
      }

      final itemStartUtf8 = _checkedDocumentOffset(
        source.startUtf8,
        relativeItemStart,
        'item start',
      );
      final prefixEndUtf8 = _checkedDocumentOffset(
        itemStartUtf8,
        hiddenPrefixLength,
        'hidden prefix end',
      );
      final continuationStartUtf8 = _checkedDocumentOffset(
        itemStartUtf8,
        continuationPrefixStart,
        'continuation prefix start',
      );
      final continuationEndUtf8 = _checkedDocumentOffset(
        itemStartUtf8,
        continuationPrefixEnd,
        'continuation prefix end',
      );
      final contentEndUtf8 = _checkedDocumentOffset(
        prefixEndUtf8,
        contentLengthUtf8,
        'item content end',
      );
      final itemEndUtf8 = _checkedDocumentOffset(
        itemStartUtf8,
        physicalLength,
        'item end',
      );
      final physicalSource = mapper.span(
        itemStartUtf8,
        itemEndUtf8,
        'physical list item',
      );
      final hiddenPrefix = mapper.span(
        itemStartUtf8,
        prefixEndUtf8,
        'list hidden prefix',
      );
      final continuationPrefix = mapper.span(
        continuationStartUtf8,
        continuationEndUtf8,
        'list continuation prefix',
      );
      final content = mapper.span(
        prefixEndUtf8,
        contentEndUtf8,
        'list item content',
      );
      final lineEnding = mapper.span(
        contentEndUtf8,
        itemEndUtf8,
        'list item line ending',
      );
      if (content.endUtf16 - content.startUtf16 != authoredContentLengthUtf16) {
        throw const FlarkV3BulletListProjectionDecodeException(
          'Item content disagrees across UTF-8 and UTF-16 coordinates.',
        );
      }
      _validatePhysicalLine(
        sourceDocument,
        physicalSource: physicalSource,
        lineEnding: lineEnding,
      );
      records.add(
        FlarkV3BulletListItemProjectionRecord._(
          ordinal: index,
          relativeItemStartUtf8: relativeItemStart,
          physicalSource: physicalSource,
          hiddenPrefix: hiddenPrefix,
          continuationPrefix: continuationPrefix,
          content: content,
          lineEnding: lineEnding,
        ),
      );
      _appendVisibleItemPieces(
        pieces,
        records.last,
        lineEndingText: _read(sourceDocument, lineEnding),
      );

      if (empty) {
        terminalEmptyRelativeStartUtf8 = relativeItemStart;
      } else {
        paragraphCount += 1;
      }
      projectedUtf8Length = _checkedU32End(
        projectedUtf8Length,
        physicalLength - hiddenPrefixLength,
        'projected UTF-8 length',
      );
      projectedUtf16Length = _checkedU32End(
        projectedUtf16Length,
        physicalSource.endUtf16 - hiddenPrefix.endUtf16,
        'projected UTF-16 length',
      );
      relativeCursor = relativeItemEnd;
    }

    if (relativeCursor != blockLengthUtf8 ||
        paragraphCount != facts.paragraphCount ||
        terminalEmptyRelativeStartUtf8 !=
            facts.terminalEmptyRelativeStartUtf8 ||
        projectedUtf8Length != facts.projectedUtf8Length ||
        projectedUtf16Length != facts.projectedUtf16Length) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Item records disagree with structural list aggregates.',
      );
    }

    final selectedItemOrdinal = _validatePointPath(
      source,
      facts,
      pointPath,
      records,
    );
    final selectedItem = records[selectedItemOrdinal];
    final activeHiddenSourcePrefix = _read(
      sourceDocument,
      selectedItem.hiddenPrefix,
    );
    final activeRemovableSourcePrefix = _read(
      sourceDocument,
      selectedItem.continuationPrefix,
    );
    final activeRemovableSourcePrefixOffsetUtf16 =
        selectedItem.continuationPrefix.startUtf16 -
        selectedItem.hiddenPrefix.startUtf16;
    var canonicalLineEnding = _read(sourceDocument, selectedItem.lineEnding);
    if (canonicalLineEnding.isEmpty) {
      for (var index = selectedItemOrdinal - 1; index >= 0; index -= 1) {
        canonicalLineEnding = _read(sourceDocument, records[index].lineEnding);
        if (canonicalLineEnding.isNotEmpty) break;
      }
    }
    if (canonicalLineEnding.isEmpty) canonicalLineEnding = '\n';

    final emptyEnterExits =
        selectedItem.isEmpty &&
        selectedItemOrdinal == records.length - 1 &&
        facts.terminalEmptyRelativeStartUtf8 ==
            selectedItem.relativeItemStartUtf8;
    return FlarkV3BulletListProjectionPayload._(
      sourceVersion: expectedSource,
      source: source,
      projectionSource: source,
      facts: facts,
      pointPath: pointPath,
      records: List.unmodifiable(records),
      selectedItemOrdinal: selectedItemOrdinal,
      editingInputs: FlarkV3TightListItemEditingInputs.parserAuthored(
        activeHiddenSourcePrefix: activeHiddenSourcePrefix,
        activeRemovableSourcePrefix: activeRemovableSourcePrefix,
        activeRemovableSourcePrefixOffsetUtf16:
            activeRemovableSourcePrefixOffsetUtf16,
        continuationSourcePrefix: activeRemovableSourcePrefix,
        canonicalLineEnding: canonicalLineEnding,
        emptyEnterExits: emptyEnterExits,
        backspaceAtStartRemovesPrefix: true,
      ),
      projectionPieces: List.unmodifiable(pieces),
      coversWholeList: true,
      sourceText: sourceDocument.readRange(source.startUtf16, source.endUtf16),
    );
  }

  /// Decodes viewport schema 6: one parser-selected item plus compact editing
  /// metadata. The payload size is independent of the enclosing list length.
  static FlarkV3BulletListProjectionPayload decodeSelectedItem({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceSpan source,
    required FlarkV3BulletListFacts facts,
    required FlarkV3DocumentPointPath pointPath,
    required Uint8List encodedPayload,
  }) {
    const metadataBytes = 8;
    if (!sourceDocument.hasCertifiedFacts) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Projection decoding requires certified source coordinates.',
      );
    }
    _validateSourceAuthority(sourceDocument, expectedSource);
    final mapper = _Utf8SpanMapper(sourceDocument);
    _validateBlockSource(source, sourceDocument, mapper);
    if (encodedPayload.lengthInBytes != metadataBytes + recordBytes) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact bullet-list projection has an invalid fixed width.',
      );
    }
    final data = ByteData.sublistView(encodedPayload);
    final selectedOrdinal = data.getUint32(0, Endian.little);
    final canonicalLineEnding = switch (data.getUint8(4)) {
      1 => '\n',
      2 => '\r\n',
      3 => '\r',
      _ => throw const FlarkV3BulletListProjectionDecodeException(
        'Compact bullet-list projection has no canonical line ending.',
      ),
    };
    if (data.getUint8(5) != 0 ||
        data.getUint8(6) != 0 ||
        data.getUint8(7) != 0 ||
        selectedOrdinal >= facts.itemCount) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact bullet-list editing metadata is invalid.',
      );
    }

    const offset = metadataBytes;
    final relativeItemStart = data.getUint32(offset, Endian.little);
    final physicalLength = data.getUint32(offset + 4, Endian.little);
    final hiddenPrefixLength = data.getUint32(offset + 8, Endian.little);
    final continuationPrefixStart = data.getUint32(offset + 12, Endian.little);
    final continuationPrefixEnd = data.getUint32(offset + 16, Endian.little);
    final contentLengthUtf8 = data.getUint32(offset + 20, Endian.little);
    final authoredContentLengthUtf16 = data.getUint32(
      offset + 24,
      Endian.little,
    );
    if (physicalLength == 0 ||
        hiddenPrefixLength == 0 ||
        continuationPrefixStart >= continuationPrefixEnd ||
        continuationPrefixEnd > hiddenPrefixLength) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact item prefix geometry is invalid.',
      );
    }
    final relativeItemEnd = _checkedU32End(
      relativeItemStart,
      physicalLength,
      'compact physical item',
    );
    final blockLengthUtf8 = source.endUtf8 - source.startUtf8;
    final prefixAndContent = _checkedU32End(
      hiddenPrefixLength,
      contentLengthUtf8,
      'compact item prefix and content',
    );
    if (relativeItemEnd > blockLengthUtf8 ||
        prefixAndContent > physicalLength) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact item escapes its exact list block.',
      );
    }
    final lineEndingLength = physicalLength - prefixAndContent;
    final empty = contentLengthUtf8 == 0;
    final terminalRelativeStart = facts.terminalEmptyRelativeStartUtf8;
    final isCertifiedTerminalEmpty =
        empty &&
        selectedOrdinal + 1 == facts.itemCount &&
        terminalRelativeStart == relativeItemStart;
    if (lineEndingLength > 2 ||
        empty != (authoredContentLengthUtf16 == 0) ||
        empty != isCertifiedTerminalEmpty) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact item terminal geometry is invalid.',
      );
    }

    final itemStartUtf8 = _checkedDocumentOffset(
      source.startUtf8,
      relativeItemStart,
      'compact item start',
    );
    final prefixEndUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      hiddenPrefixLength,
      'compact hidden prefix end',
    );
    final continuationStartUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      continuationPrefixStart,
      'compact continuation prefix start',
    );
    final continuationEndUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      continuationPrefixEnd,
      'compact continuation prefix end',
    );
    final contentEndUtf8 = _checkedDocumentOffset(
      prefixEndUtf8,
      contentLengthUtf8,
      'compact item content end',
    );
    final itemEndUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      physicalLength,
      'compact item end',
    );
    final physicalSource = mapper.span(
      itemStartUtf8,
      itemEndUtf8,
      'compact physical list item',
    );
    final hiddenPrefix = mapper.span(
      itemStartUtf8,
      prefixEndUtf8,
      'compact list hidden prefix',
    );
    final continuationPrefix = mapper.span(
      continuationStartUtf8,
      continuationEndUtf8,
      'compact list continuation prefix',
    );
    final content = mapper.span(
      prefixEndUtf8,
      contentEndUtf8,
      'compact list item content',
    );
    final lineEnding = mapper.span(
      contentEndUtf8,
      itemEndUtf8,
      'compact list item line ending',
    );
    if (content.endUtf16 - content.startUtf16 != authoredContentLengthUtf16) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact item content disagrees across UTF-8 and UTF-16.',
      );
    }
    _validatePhysicalLine(
      sourceDocument,
      physicalSource: physicalSource,
      lineEnding: lineEnding,
    );
    final physicalLineEnding = _read(sourceDocument, lineEnding);
    if (physicalLineEnding.isNotEmpty &&
        physicalLineEnding != canonicalLineEnding) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact item canonical line ending disagrees with source.',
      );
    }

    final record = FlarkV3BulletListItemProjectionRecord._(
      ordinal: selectedOrdinal,
      relativeItemStartUtf8: relativeItemStart,
      physicalSource: physicalSource,
      hiddenPrefix: hiddenPrefix,
      continuationPrefix: continuationPrefix,
      content: content,
      lineEnding: lineEnding,
    );
    _validateSelectedItemPointPath(
      source,
      facts,
      pointPath,
      record,
      selectedOrdinal,
    );
    final pieces = <FlarkV3SourceProjectionPiece>[];
    _appendVisibleItemPieces(
      pieces,
      record,
      lineEndingText: physicalLineEnding,
    );
    final activeHiddenSourcePrefix = _read(sourceDocument, hiddenPrefix);
    final activeRemovableSourcePrefix = _read(
      sourceDocument,
      continuationPrefix,
    );
    return FlarkV3BulletListProjectionPayload._(
      sourceVersion: expectedSource,
      source: source,
      projectionSource: physicalSource,
      facts: facts,
      pointPath: pointPath,
      records: List.unmodifiable(<FlarkV3BulletListItemProjectionRecord>[
        record,
      ]),
      selectedItemOrdinal: selectedOrdinal,
      editingInputs: FlarkV3TightListItemEditingInputs.parserAuthored(
        activeHiddenSourcePrefix: activeHiddenSourcePrefix,
        activeRemovableSourcePrefix: activeRemovableSourcePrefix,
        activeRemovableSourcePrefixOffsetUtf16:
            continuationPrefix.startUtf16 - hiddenPrefix.startUtf16,
        continuationSourcePrefix: activeRemovableSourcePrefix,
        canonicalLineEnding: canonicalLineEnding,
        emptyEnterExits: isCertifiedTerminalEmpty,
        backspaceAtStartRemovesPrefix: true,
      ),
      projectionPieces: List.unmodifiable(pieces),
      coversWholeList: false,
      sourceText: sourceDocument.readRange(
        physicalSource.startUtf16,
        physicalSource.endUtf16,
      ),
    );
  }
}

void _appendVisibleItemPieces(
  List<FlarkV3SourceProjectionPiece> pieces,
  FlarkV3BulletListItemProjectionRecord item, {
  required String lineEndingText,
}) {
  pieces.add(
    FlarkV3SourceProjectionPiece.hide(
      sourceStartUtf16: item.hiddenPrefix.startUtf16,
      sourceEndUtf16: item.hiddenPrefix.endUtf16,
    ),
  );
  if (item.content.startUtf16 != item.content.endUtf16) {
    pieces.add(
      FlarkV3SourceProjectionPiece.copy(
        sourceStartUtf16: item.content.startUtf16,
        sourceEndUtf16: item.content.endUtf16,
      ),
    );
  }
  if (item.lineEnding.startUtf16 == item.lineEnding.endUtf16) return;
  pieces.add(
    lineEndingText == '\n'
        ? FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: item.lineEnding.startUtf16,
            sourceEndUtf16: item.lineEnding.endUtf16,
          )
        : FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: item.lineEnding.startUtf16,
            sourceEndUtf16: item.lineEnding.endUtf16,
            displayText: '\n',
          ),
  );
}

int _validatePointPath(
  FlarkV3SourceSpan source,
  FlarkV3BulletListFacts facts,
  FlarkV3DocumentPointPath pointPath,
  List<FlarkV3BulletListItemProjectionRecord> records,
) {
  if (pointPath.nodes.length != 2 && pointPath.nodes.length != 3) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Bullet-list point path has an unsupported depth.',
    );
  }
  final list = pointPath.nodes[0];
  final item = pointPath.nodes[1];
  final selectedOrdinal = item.firstRun;
  if (selectedOrdinal < 0 || selectedOrdinal >= records.length) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Bullet-list point path selects an unknown item.',
    );
  }
  final record = records[selectedOrdinal];
  if (list.kind != FlarkV3DocumentPointPathNodeKind.list ||
      list.depth != 0 ||
      list.parentIndex != null ||
      !list.isNoncontiguous ||
      list.isSelected ||
      !_sameSpan(list.source, source) ||
      list.firstRun != 0 ||
      list.runCount != facts.itemCount ||
      list.projectedUtf8Length != facts.projectedUtf8Length ||
      list.projectedUtf16Length != facts.projectedUtf16Length ||
      item.kind != FlarkV3DocumentPointPathNodeKind.listItem ||
      item.depth != 1 ||
      item.parentIndex != 0 ||
      !item.isNoncontiguous ||
      !_sameSpan(item.source, record.physicalSource) ||
      item.runCount != 1 ||
      item.projectedUtf8Length != record.projectedUtf8Length ||
      item.projectedUtf16Length != record.projectedUtf16Length) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Bullet-list point path disagrees with its item records.',
    );
  }

  if (record.isEmpty) {
    if (pointPath.nodes.length != 2 ||
        !item.isSelected ||
        selectedOrdinal != records.length - 1 ||
        facts.terminalEmptyRelativeStartUtf8 != record.relativeItemStartUtf8) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'A terminal-empty list path must select its final ListItem.',
      );
    }
    return selectedOrdinal;
  }

  if (pointPath.nodes.length != 3 || item.isSelected) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'A nonempty list path must select its Paragraph child.',
    );
  }
  final paragraph = pointPath.nodes[2];
  if (paragraph.kind != FlarkV3DocumentPointPathNodeKind.paragraph ||
      paragraph.depth != 2 ||
      paragraph.parentIndex != 1 ||
      paragraph.isNoncontiguous ||
      !paragraph.isSelected ||
      !_sameSpan(paragraph.source, record.content) ||
      paragraph.firstRun != selectedOrdinal ||
      paragraph.runCount != 1 ||
      paragraph.projectedUtf8Length !=
          record.content.endUtf8 - record.content.startUtf8 ||
      paragraph.projectedUtf16Length !=
          record.content.endUtf16 - record.content.startUtf16) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'A selected list Paragraph disagrees with its item content.',
    );
  }
  return selectedOrdinal;
}

void _validateSelectedItemPointPath(
  FlarkV3SourceSpan source,
  FlarkV3BulletListFacts facts,
  FlarkV3DocumentPointPath pointPath,
  FlarkV3BulletListItemProjectionRecord record,
  int selectedOrdinal,
) {
  if (pointPath.nodes.length != 2 && pointPath.nodes.length != 3) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Compact bullet-list point path has an unsupported depth.',
    );
  }
  final list = pointPath.nodes[0];
  final item = pointPath.nodes[1];
  if (list.kind != FlarkV3DocumentPointPathNodeKind.list ||
      list.depth != 0 ||
      list.parentIndex != null ||
      !list.isNoncontiguous ||
      list.isSelected ||
      !_sameSpan(list.source, source) ||
      list.firstRun != 0 ||
      list.runCount != facts.itemCount ||
      list.projectedUtf8Length != facts.projectedUtf8Length ||
      list.projectedUtf16Length != facts.projectedUtf16Length ||
      item.kind != FlarkV3DocumentPointPathNodeKind.listItem ||
      item.depth != 1 ||
      item.parentIndex != 0 ||
      !item.isNoncontiguous ||
      !_sameSpan(item.source, record.physicalSource) ||
      item.firstRun != selectedOrdinal ||
      item.runCount != 1 ||
      item.projectedUtf8Length != record.projectedUtf8Length ||
      item.projectedUtf16Length != record.projectedUtf16Length) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Compact bullet-list path disagrees with its selected item.',
    );
  }
  if (record.isEmpty) {
    if (pointPath.nodes.length != 2 ||
        !item.isSelected ||
        selectedOrdinal + 1 != facts.itemCount ||
        facts.terminalEmptyRelativeStartUtf8 != record.relativeItemStartUtf8) {
      throw const FlarkV3BulletListProjectionDecodeException(
        'Compact terminal-empty path is not the certified final item.',
      );
    }
    return;
  }
  if (pointPath.nodes.length != 3 || item.isSelected) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Compact nonempty item must select its Paragraph child.',
    );
  }
  final paragraph = pointPath.nodes[2];
  if (paragraph.kind != FlarkV3DocumentPointPathNodeKind.paragraph ||
      paragraph.depth != 2 ||
      paragraph.parentIndex != 1 ||
      paragraph.isNoncontiguous ||
      !paragraph.isSelected ||
      !_sameSpan(paragraph.source, record.content) ||
      paragraph.firstRun != selectedOrdinal ||
      paragraph.runCount != 1 ||
      paragraph.projectedUtf8Length !=
          record.content.endUtf8 - record.content.startUtf8 ||
      paragraph.projectedUtf16Length !=
          record.content.endUtf16 - record.content.startUtf16) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Compact selected Paragraph disagrees with item content.',
    );
  }
}

void _validateSourceAuthority(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3SourceVersion expectedSource,
) {
  if (expectedSource.revision != sourceDocument.revision ||
      expectedSource.metric.bytes != sourceDocument.utf8Length ||
      expectedSource.metric.utf16 != sourceDocument.utf16Length ||
      expectedSource.contentHash != sourceDocument.contentHash128) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Projection authority does not match the certified source document.',
    );
  }
}

void _validateBlockSource(
  FlarkV3SourceSpan source,
  FlarkV3SourceDocument sourceDocument,
  _Utf8SpanMapper mapper,
) {
  if (source.startUtf8 < 0 ||
      source.endUtf8 <= source.startUtf8 ||
      source.endUtf8 > sourceDocument.utf8Length ||
      source.startUtf16 < 0 ||
      source.endUtf16 <= source.startUtf16 ||
      source.endUtf16 > sourceDocument.utf16Length ||
      source.endUtf8 - source.startUtf8 > _u32Maximum) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Bullet-list projection source is outside the certified document.',
    );
  }
  mapper.expect(source.startUtf8, source.startUtf16, 'list start');
  mapper.expect(source.endUtf8, source.endUtf16, 'list end');
}

void _validatePhysicalLine(
  FlarkV3SourceDocument sourceDocument, {
  required FlarkV3SourceSpan physicalSource,
  required FlarkV3SourceSpan lineEnding,
}) {
  final body = sourceDocument.readRange(
    physicalSource.startUtf16,
    lineEnding.startUtf16,
  );
  if (body.contains('\r') || body.contains('\n')) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'A bullet-list item crosses a physical line boundary.',
    );
  }
  final ending = _read(sourceDocument, lineEnding);
  if (ending.isNotEmpty &&
      ending != '\n' &&
      ending != '\r' &&
      ending != '\r\n') {
    throw const FlarkV3BulletListProjectionDecodeException(
      'A bullet-list item does not end at a physical line ending.',
    );
  }
  if (ending.length != lineEnding.endUtf8 - lineEnding.startUtf8) {
    throw const FlarkV3BulletListProjectionDecodeException(
      'Bullet-list line-ending width disagrees with exact source.',
    );
  }
}

String _read(FlarkV3SourceDocument sourceDocument, FlarkV3SourceSpan span) =>
    sourceDocument.readRange(span.startUtf16, span.endUtf16);

bool _sameSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

int _checkedU32End(int start, int length, String name) {
  if (start < 0 ||
      start > _u32Maximum ||
      length < 0 ||
      length > _u32Maximum - start) {
    throw FlarkV3BulletListProjectionDecodeException(
      '$name overflows canonical u32 geometry.',
    );
  }
  return start + length;
}

int _checkedDocumentOffset(int start, int length, String name) {
  if (start < 0 || length < 0 || start > _maximumSafeInteger - length) {
    throw FlarkV3BulletListProjectionDecodeException(
      '$name overflows canonical document geometry.',
    );
  }
  return start + length;
}

final class _Utf8SpanMapper {
  _Utf8SpanMapper(this._sourceDocument);

  final FlarkV3SourceDocument _sourceDocument;
  final Map<int, int> _cache = <int, int>{};

  void expect(int utf8, int utf16, String name) {
    if (_map(utf8, name) != utf16) {
      throw FlarkV3BulletListProjectionDecodeException(
        '$name does not match exact source coordinates.',
      );
    }
  }

  FlarkV3SourceSpan span(int startUtf8, int endUtf8, String name) {
    final startUtf16 = _map(startUtf8, '$name start');
    final endUtf16 = _map(endUtf8, '$name end');
    if (endUtf16 < startUtf16) {
      throw FlarkV3BulletListProjectionDecodeException(
        '$name has non-monotonic source coordinates.',
      );
    }
    return FlarkV3SourceSpan(
      startUtf8: startUtf8,
      endUtf8: endUtf8,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
    );
  }

  int _map(int utf8, String name) {
    if (utf8 < 0 || utf8 > _sourceDocument.utf8Length) {
      throw FlarkV3BulletListProjectionDecodeException(
        '$name is outside exact source.',
      );
    }
    final cached = _cache[utf8];
    if (cached != null) return cached;
    late final int utf16;
    try {
      utf16 = _sourceDocument.utf8ToUtf16(utf8);
    } on Object {
      throw FlarkV3BulletListProjectionDecodeException(
        '$name is not an exact UTF-8 scalar boundary.',
      );
    }
    _cache[utf8] = utf16;
    return utf16;
  }
}

const int _u32Maximum = 0xFFFFFFFF;
const int _maximumSafeInteger = 0x1FFFFFFFFFFFFF;

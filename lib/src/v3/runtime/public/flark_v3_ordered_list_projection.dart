import 'dart:typed_data';

import '../../editor/flark_v3_source_projection.dart';
import '../../host/flark_v3_host_protocol.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_bullet_list_projection.dart';
import 'flark_v3_document_query.dart';

/// Product policy when incrementing the largest CommonMark ordered marker.
enum FlarkV3OrderedListContinuationOverflowPolicy {
  /// Repeat the exact current numeric value instead of emitting ten digits.
  ///
  /// CommonMark permits nonsequential item markers, so this remains valid and
  /// is less destructive than rejecting an otherwise editable list item.
  repeatCurrentMarker,
}

/// Parser-authored source geometry for one selected tight ordered-list item.
final class FlarkV3OrderedListItemProjectionRecord
    implements FlarkV3TightListItemProjectionRecord {
  const FlarkV3OrderedListItemProjectionRecord._({
    required this.ordinal,
    required this.relativeItemStartUtf8,
    required this.physicalSource,
    required this.hiddenPrefix,
    required this.continuationPrefix,
    required this.content,
    required this.lineEnding,
    required this.openingMarker,
    required this.markerValue,
  });

  @override
  final int ordinal;
  @override
  final int relativeItemStartUtf8;
  @override
  final FlarkV3SourceSpan physicalSource;
  @override
  final FlarkV3SourceSpan hiddenPrefix;
  @override
  final FlarkV3SourceSpan continuationPrefix;
  @override
  final FlarkV3SourceSpan content;
  @override
  final FlarkV3SourceSpan lineEnding;

  /// Exact parser-selected authored marker, including its delimiter.
  final FlarkV3SourceSpan openingMarker;

  /// Parser-certified numeric value of [openingMarker].
  ///
  /// Dart does not parse [openingMarker] to recover this value.
  final int markerValue;

  @override
  bool get isEmpty => content.startUtf8 == content.endUtf8;
  @override
  int get projectedUtf8Length => physicalSource.endUtf8 - hiddenPrefix.endUtf8;
  @override
  int get projectedUtf16Length =>
      physicalSource.endUtf16 - hiddenPrefix.endUtf16;
  @override
  int get displayUtf16Length =>
      content.endUtf16 -
      content.startUtf16 +
      (lineEnding.startUtf16 == lineEnding.endUtf16 ? 0 : 1);
}

/// Exact compact projection and editing authority for one ordered-list item.
final class FlarkV3OrderedListProjectionPayload
    implements FlarkV3TightListItemProjectionPayload {
  const FlarkV3OrderedListProjectionPayload._({
    required this.sourceVersion,
    required this.source,
    required this.projectionSource,
    required this.facts,
    required this.pointPath,
    required this.records,
    required this.selectedItemOrdinal,
    required this.editingInputs,
    required this.projectionPieces,
    required this.selectedMarkerText,
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

  /// Parser-certified list-level start, delimiter, and projection metrics.
  final FlarkV3OrderedListFacts facts;

  @override
  final FlarkV3DocumentPointPath pointPath;
  @override
  final List<FlarkV3OrderedListItemProjectionRecord> records;
  @override
  final int selectedItemOrdinal;
  @override
  final FlarkV3TightListItemEditingInputs editingInputs;
  @override
  final List<FlarkV3SourceProjectionPiece> projectionPieces;

  /// Exact source substring named by [selectedItem]'s opening-marker span.
  ///
  /// Consumers paint this value directly. They must not reconstruct it from
  /// [FlarkV3OrderedListItemProjectionRecord.markerValue].
  final String selectedMarkerText;

  @override
  bool get coversWholeList => false;

  @override
  FlarkV3OrderedListItemProjectionRecord get selectedItem => records.single;
  @override
  int get selectedItemDisplayUtf16Length => selectedItem.displayUtf16Length;

  final String _sourceText;

  @override
  FlarkV3SourceProjection toSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  }) => FlarkV3SourceProjection.fromSource(
    sourceStartUtf16: projectionSource.startUtf16,
    sourceText: _sourceText,
    pieces: projectionPieces,
    certifiedSourceVersion: sourceVersion,
    maximumSourceUtf16: maximumSourceUtf16,
    maximumDisplayUtf16: maximumDisplayUtf16,
  );

  @override
  FlarkV3SourceProjection toSelectedItemSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  }) => toSourceProjection(
    maximumSourceUtf16: maximumSourceUtf16,
    maximumDisplayUtf16: maximumDisplayUtf16,
  );
}

/// A corrupt or source-incompatible ordered-list projection payload.
final class FlarkV3OrderedListProjectionDecodeException implements Exception {
  const FlarkV3OrderedListProjectionDecodeException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3OrderedListProjectionDecodeException($message)';
}

/// Decoder for one compact ordered-list item.
///
/// The payload contains 20 bytes of selected-item metadata followed by the
/// existing 28-byte marked-line projection record. Keeping these constants
/// here isolates this provisional viewport transport layout from the public
/// payload model.
final class FlarkV3OrderedListProjectionDecoder {
  const FlarkV3OrderedListProjectionDecoder._();

  static const int metadataBytes = 20;
  static const int recordBytes = 28;
  static const int encodedPayloadBytes = metadataBytes + recordBytes;
  static const int maximumMarkerValue = 999999999;
  static const FlarkV3OrderedListContinuationOverflowPolicy
  continuationOverflowPolicy =
      FlarkV3OrderedListContinuationOverflowPolicy.repeatCurrentMarker;

  /// Decodes one parser-selected item and exact authored marker.
  ///
  /// Metadata layout:
  ///
  /// - `u32` selected ordinal at byte 0;
  /// - `u8` canonical line ending at byte 4, then three zero bytes;
  /// - `u32` opening-marker start/end relative to the item at bytes 8/12;
  /// - `u32` parser-certified marker value at byte 16;
  /// - one canonical 28-byte marked-line record at byte 20.
  static FlarkV3OrderedListProjectionPayload decodeSelectedItem({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceSpan source,
    required FlarkV3OrderedListFacts facts,
    required FlarkV3DocumentPointPath pointPath,
    required Uint8List encodedPayload,
  }) {
    if (!sourceDocument.hasCertifiedFacts) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Projection decoding requires certified source coordinates.',
      );
    }
    _validateSourceAuthority(sourceDocument, expectedSource);
    final mapper = _Utf8SpanMapper(sourceDocument);
    _validateBlockSource(source, sourceDocument, mapper);
    if (encodedPayload.lengthInBytes != encodedPayloadBytes) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list projection has an invalid fixed width.',
      );
    }

    final data = ByteData.sublistView(encodedPayload);
    final selectedOrdinal = data.getUint32(0, Endian.little);
    final canonicalLineEnding = switch (data.getUint8(4)) {
      1 => '\n',
      2 => '\r\n',
      3 => '\r',
      _ => throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list projection has no canonical line ending.',
      ),
    };
    final openingMarkerStart = data.getUint32(8, Endian.little);
    final openingMarkerEnd = data.getUint32(12, Endian.little);
    final markerValue = data.getUint32(16, Endian.little);
    final openingMarkerLength = openingMarkerEnd - openingMarkerStart;
    if (data.getUint8(5) != 0 ||
        data.getUint8(6) != 0 ||
        data.getUint8(7) != 0 ||
        selectedOrdinal >= facts.itemCount ||
        openingMarkerStart >= openingMarkerEnd ||
        openingMarkerLength < 2 ||
        openingMarkerLength > 10 ||
        markerValue > maximumMarkerValue) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list editing metadata is invalid.',
      );
    }

    const recordOffset = metadataBytes;
    final relativeItemStart = data.getUint32(recordOffset, Endian.little);
    final physicalLength = data.getUint32(recordOffset + 4, Endian.little);
    final hiddenPrefixLength = data.getUint32(recordOffset + 8, Endian.little);
    final continuationPrefixStart = data.getUint32(
      recordOffset + 12,
      Endian.little,
    );
    final continuationPrefixEnd = data.getUint32(
      recordOffset + 16,
      Endian.little,
    );
    final contentLengthUtf8 = data.getUint32(recordOffset + 20, Endian.little);
    final contentLengthUtf16 = data.getUint32(recordOffset + 24, Endian.little);
    if (physicalLength == 0 ||
        hiddenPrefixLength == 0 ||
        continuationPrefixStart >= continuationPrefixEnd ||
        continuationPrefixEnd > hiddenPrefixLength ||
        openingMarkerStart < continuationPrefixStart ||
        openingMarkerEnd > continuationPrefixEnd) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list item prefix geometry is invalid.',
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
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list item escapes its exact list block.',
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
        empty != (contentLengthUtf16 == 0) ||
        empty != isCertifiedTerminalEmpty) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list terminal geometry is invalid.',
      );
    }

    final itemStartUtf8 = _checkedDocumentOffset(
      source.startUtf8,
      relativeItemStart,
      'compact item start',
    );
    final itemEndUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      physicalLength,
      'compact item end',
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
    final openingMarkerStartUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      openingMarkerStart,
      'compact opening marker start',
    );
    final openingMarkerEndUtf8 = _checkedDocumentOffset(
      itemStartUtf8,
      openingMarkerEnd,
      'compact opening marker end',
    );
    final contentEndUtf8 = _checkedDocumentOffset(
      prefixEndUtf8,
      contentLengthUtf8,
      'compact item content end',
    );

    final physicalSource = mapper.span(
      itemStartUtf8,
      itemEndUtf8,
      'compact physical ordered-list item',
    );
    final hiddenPrefix = mapper.span(
      itemStartUtf8,
      prefixEndUtf8,
      'compact ordered-list hidden prefix',
    );
    final continuationPrefix = mapper.span(
      continuationStartUtf8,
      continuationEndUtf8,
      'compact ordered-list continuation prefix',
    );
    final openingMarker = mapper.span(
      openingMarkerStartUtf8,
      openingMarkerEndUtf8,
      'compact ordered-list opening marker',
    );
    final content = mapper.span(
      prefixEndUtf8,
      contentEndUtf8,
      'compact ordered-list item content',
    );
    final lineEnding = mapper.span(
      contentEndUtf8,
      itemEndUtf8,
      'compact ordered-list item line ending',
    );
    if (content.endUtf16 - content.startUtf16 != contentLengthUtf16 ||
        openingMarker.endUtf16 - openingMarker.startUtf16 !=
            openingMarkerLength) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list item disagrees across UTF-8 and UTF-16.',
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
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list canonical line ending disagrees with source.',
      );
    }

    final selectedMarkerText = _read(sourceDocument, openingMarker);
    if (!_markerPayloadIsExact(
      selectedMarkerText,
      markerValue: markerValue,
      delimiter: facts.delimiter,
    )) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered-list marker disagrees with its certified metadata.',
      );
    }
    final record = FlarkV3OrderedListItemProjectionRecord._(
      ordinal: selectedOrdinal,
      relativeItemStartUtf8: relativeItemStart,
      physicalSource: physicalSource,
      hiddenPrefix: hiddenPrefix,
      continuationPrefix: continuationPrefix,
      content: content,
      lineEnding: lineEnding,
      openingMarker: openingMarker,
      markerValue: markerValue,
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
    final continuationSourcePrefix = _continuationSourcePrefix(
      activeRemovableSourcePrefix: activeRemovableSourcePrefix,
      continuationPrefix: continuationPrefix,
      openingMarker: openingMarker,
      openingMarkerLength: openingMarkerLength,
      markerValue: markerValue,
      delimiter: facts.delimiter,
    );
    return FlarkV3OrderedListProjectionPayload._(
      sourceVersion: expectedSource,
      source: source,
      projectionSource: physicalSource,
      facts: facts,
      pointPath: pointPath,
      records: List.unmodifiable(<FlarkV3OrderedListItemProjectionRecord>[
        record,
      ]),
      selectedItemOrdinal: selectedOrdinal,
      editingInputs: FlarkV3TightListItemEditingInputs.parserAuthored(
        activeHiddenSourcePrefix: activeHiddenSourcePrefix,
        activeRemovableSourcePrefix: activeRemovableSourcePrefix,
        activeRemovableSourcePrefixOffsetUtf16:
            continuationPrefix.startUtf16 - hiddenPrefix.startUtf16,
        continuationSourcePrefix: continuationSourcePrefix,
        canonicalLineEnding: canonicalLineEnding,
        emptyEnterExits: isCertifiedTerminalEmpty,
        backspaceAtStartRemovesPrefix: true,
      ),
      projectionPieces: List.unmodifiable(pieces),
      selectedMarkerText: selectedMarkerText,
      sourceText: sourceDocument.readRange(
        physicalSource.startUtf16,
        physicalSource.endUtf16,
      ),
    );
  }
}

bool _markerPayloadIsExact(
  String markerText, {
  required int markerValue,
  required FlarkV3OrderedListDelimiter delimiter,
}) {
  if (markerText.length < 2 || markerText.length > 10) return false;
  if (markerText.codeUnitAt(markerText.length - 1) !=
      delimiter.sourceCharacter.codeUnitAt(0)) {
    return false;
  }
  var parsedValue = 0;
  for (var index = 0; index < markerText.length - 1; index += 1) {
    final codeUnit = markerText.codeUnitAt(index);
    if (codeUnit < 0x30 || codeUnit > 0x39) return false;
    parsedValue = parsedValue * 10 + codeUnit - 0x30;
  }
  return parsedValue == markerValue;
}

String _continuationSourcePrefix({
  required String activeRemovableSourcePrefix,
  required FlarkV3SourceSpan continuationPrefix,
  required FlarkV3SourceSpan openingMarker,
  required int openingMarkerLength,
  required int markerValue,
  required FlarkV3OrderedListDelimiter delimiter,
}) {
  final markerStart = openingMarker.startUtf16 - continuationPrefix.startUtf16;
  final markerEnd = openingMarker.endUtf16 - continuationPrefix.startUtf16;
  if (markerStart < 0 ||
      markerEnd <= markerStart ||
      markerEnd > activeRemovableSourcePrefix.length) {
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Ordered-list marker escapes its removable source prefix.',
    );
  }
  final continuationValue =
      switch (FlarkV3OrderedListProjectionDecoder.continuationOverflowPolicy) {
        FlarkV3OrderedListContinuationOverflowPolicy.repeatCurrentMarker
            when markerValue ==
                FlarkV3OrderedListProjectionDecoder.maximumMarkerValue =>
          markerValue,
        FlarkV3OrderedListContinuationOverflowPolicy.repeatCurrentMarker =>
          markerValue + 1,
      };
  final digitWidth = openingMarkerLength - 1;
  final continuationMarker =
      continuationValue.toString().padLeft(digitWidth, '0') +
      delimiter.sourceCharacter;
  return activeRemovableSourcePrefix.replaceRange(
    markerStart,
    markerEnd,
    continuationMarker,
  );
}

void _appendVisibleItemPieces(
  List<FlarkV3SourceProjectionPiece> pieces,
  FlarkV3OrderedListItemProjectionRecord item, {
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

void _validateSelectedItemPointPath(
  FlarkV3SourceSpan source,
  FlarkV3OrderedListFacts facts,
  FlarkV3DocumentPointPath pointPath,
  FlarkV3OrderedListItemProjectionRecord record,
  int selectedOrdinal,
) {
  if (pointPath.nodes.length != 2 && pointPath.nodes.length != 3) {
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Compact ordered-list point path has an unsupported depth.',
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
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Compact ordered-list path disagrees with its selected item.',
    );
  }
  if (record.isEmpty) {
    if (pointPath.nodes.length != 2 ||
        !item.isSelected ||
        selectedOrdinal + 1 != facts.itemCount ||
        facts.terminalEmptyRelativeStartUtf8 != record.relativeItemStartUtf8) {
      throw const FlarkV3OrderedListProjectionDecodeException(
        'Compact ordered terminal-empty path is not the certified final item.',
      );
    }
    return;
  }
  if (pointPath.nodes.length != 3 || item.isSelected) {
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Compact ordered nonempty item must select its Paragraph child.',
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
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Compact ordered selected Paragraph disagrees with item content.',
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
    throw const FlarkV3OrderedListProjectionDecodeException(
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
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Ordered-list projection source is outside the certified document.',
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
    throw const FlarkV3OrderedListProjectionDecodeException(
      'An ordered-list item crosses a physical line boundary.',
    );
  }
  final ending = _read(sourceDocument, lineEnding);
  if (ending.isNotEmpty &&
      ending != '\n' &&
      ending != '\r' &&
      ending != '\r\n') {
    throw const FlarkV3OrderedListProjectionDecodeException(
      'An ordered-list item does not end at a physical line ending.',
    );
  }
  if (ending.length != lineEnding.endUtf8 - lineEnding.startUtf8) {
    throw const FlarkV3OrderedListProjectionDecodeException(
      'Ordered-list line-ending width disagrees with exact source.',
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
    throw FlarkV3OrderedListProjectionDecodeException(
      '$name overflows canonical u32 geometry.',
    );
  }
  return start + length;
}

int _checkedDocumentOffset(int start, int length, String name) {
  if (start < 0 || length < 0 || start > _maximumSafeInteger - length) {
    throw FlarkV3OrderedListProjectionDecodeException(
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
      throw FlarkV3OrderedListProjectionDecodeException(
        '$name does not match exact source coordinates.',
      );
    }
  }

  FlarkV3SourceSpan span(int startUtf8, int endUtf8, String name) {
    final startUtf16 = _map(startUtf8, '$name start');
    final endUtf16 = _map(endUtf8, '$name end');
    if (endUtf16 < startUtf16) {
      throw FlarkV3OrderedListProjectionDecodeException(
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
      throw FlarkV3OrderedListProjectionDecodeException(
        '$name is outside exact source.',
      );
    }
    final cached = _cache[utf8];
    if (cached != null) return cached;
    late final int utf16;
    try {
      utf16 = _sourceDocument.utf8ToUtf16(utf8);
    } on Object {
      throw FlarkV3OrderedListProjectionDecodeException(
        '$name is not an exact UTF-8 scalar boundary.',
      );
    }
    _cache[utf8] = utf16;
    return utf16;
  }
}

const int _u32Maximum = 0xFFFFFFFF;
const int _maximumSafeInteger = 0x1FFFFFFFFFFFFF;

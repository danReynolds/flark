import 'dart:typed_data';

import '../../editor/flark_v3_source_projection.dart';
import '../../host/flark_v3_host_protocol.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_document_query.dart';

/// Parser-authored role of one physical line in a block-quote projection.
enum FlarkV3BlockQuoteLineProjectionKind { marked, lazyContinuation }

/// One exact physical-source line in a selected block-quote Paragraph.
final class FlarkV3BlockQuoteLineProjectionRecord {
  const FlarkV3BlockQuoteLineProjectionRecord._({
    required this.relativeLineStartUtf8,
    required this.physicalSource,
    required this.hiddenPrefix,
    required this.content,
    required this.lineEnding,
    required this.kind,
  });

  final int relativeLineStartUtf8;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3SourceSpan hiddenPrefix;
  final FlarkV3SourceSpan content;
  final FlarkV3SourceSpan lineEnding;
  final FlarkV3BlockQuoteLineProjectionKind kind;

  bool get isMarked => kind == FlarkV3BlockQuoteLineProjectionKind.marked;
  bool get isLazyContinuation =>
      kind == FlarkV3BlockQuoteLineProjectionKind.lazyContinuation;
}

/// Exact source-backed projection for one selected quote Paragraph path.
final class FlarkV3BlockQuoteProjectionPayload {
  const FlarkV3BlockQuoteProjectionPayload._({
    required this.sourceVersion,
    required this.source,
    required this.facts,
    required this.pointPath,
    required this.records,
    required this.projectionPieces,
    required String sourceText,
  }) : _sourceText = sourceText;

  final FlarkV3SourceVersion sourceVersion;
  int get sourceRevision => sourceVersion.revision;

  final FlarkV3SourceSpan source;
  final FlarkV3BlockQuoteFacts facts;
  final FlarkV3DocumentPointPath pointPath;
  final List<FlarkV3BlockQuoteLineProjectionRecord> records;
  final List<FlarkV3SourceProjectionPiece> projectionPieces;

  final String _sourceText;

  /// Builds the marker-free Paragraph projection without recognizing Markdown.
  FlarkV3SourceProjection toSourceProjection({
    int maximumSourceUtf16 = FlarkV3SourceProjection.defaultMaximumSourceUtf16,
    int maximumDisplayUtf16 =
        FlarkV3SourceProjection.defaultMaximumDisplayUtf16,
  }) {
    return FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: source.startUtf16,
      sourceText: _sourceText,
      pieces: projectionPieces,
      certifiedSourceVersion: sourceVersion,
      maximumSourceUtf16: maximumSourceUtf16,
      maximumDisplayUtf16: maximumDisplayUtf16,
    );
  }
}

/// A corrupt or source-incompatible block-quote projection payload.
final class FlarkV3BlockQuoteProjectionDecodeException implements Exception {
  const FlarkV3BlockQuoteProjectionDecodeException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3BlockQuoteProjectionDecodeException($message)';
}

/// Decoder for canonical 20-byte little-endian quote-line records.
///
/// Each record contains relative line start, physical length, hidden-prefix
/// length, content length, and flags. Exactly one of [markedFlag] and
/// [lazyContinuationFlag] must be set.
final class FlarkV3BlockQuoteProjectionDecoder {
  const FlarkV3BlockQuoteProjectionDecoder._();

  static const int recordBytes = 20;
  static const int markedFlag = 1;
  static const int lazyContinuationFlag = 1 << 1;
  static const int _flagMask = markedFlag | lazyContinuationFlag;

  static FlarkV3BlockQuoteProjectionPayload decode({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceSpan source,
    required FlarkV3BlockQuoteFacts facts,
    required FlarkV3DocumentPointPath pointPath,
    required Uint8List encodedRecords,
  }) {
    if (!sourceDocument.hasCertifiedFacts) {
      throw const FlarkV3BlockQuoteProjectionDecodeException(
        'Projection decoding requires certified source coordinates.',
      );
    }
    _validateSourceAuthority(sourceDocument, expectedSource);
    final mapper = _Utf8SpanMapper(sourceDocument);
    _validateBlockSource(source, sourceDocument, mapper);
    _validatePointPath(source, facts, pointPath);
    if (encodedRecords.lengthInBytes % recordBytes != 0) {
      throw const FlarkV3BlockQuoteProjectionDecodeException(
        'Quote projection is not a whole number of canonical records.',
      );
    }
    final recordCount = encodedRecords.lengthInBytes ~/ recordBytes;
    if (recordCount == 0 ||
        recordCount != facts.lineCount ||
        recordCount != pointPath.root.runCount) {
      throw const FlarkV3BlockQuoteProjectionDecodeException(
        'Quote-line count does not match the structural path summary.',
      );
    }

    final blockLengthUtf8 = source.endUtf8 - source.startUtf8;
    final data = ByteData.sublistView(encodedRecords);
    final records = <FlarkV3BlockQuoteLineProjectionRecord>[];
    final pieces = <FlarkV3SourceProjectionPiece>[];
    var relativeCursor = 0;
    var projectedUtf8Length = 0;
    var projectedUtf16Length = 0;

    for (var index = 0; index < recordCount; index += 1) {
      final offset = index * recordBytes;
      final relativeLineStart = data.getUint32(offset, Endian.little);
      final physicalLength = data.getUint32(offset + 4, Endian.little);
      final hiddenPrefixLength = data.getUint32(offset + 8, Endian.little);
      final contentLength = data.getUint32(offset + 12, Endian.little);
      final flags = data.getUint32(offset + 16, Endian.little);
      if (flags & ~_flagMask != 0 ||
          flags != markedFlag && flags != lazyContinuationFlag) {
        throw const FlarkV3BlockQuoteProjectionDecodeException(
          'A quote-line record must have exactly one recognized line kind.',
        );
      }
      if (relativeLineStart != relativeCursor ||
          physicalLength == 0 ||
          contentLength == 0) {
        throw const FlarkV3BlockQuoteProjectionDecodeException(
          'Quote-line records do not form a nonempty physical-line tiling.',
        );
      }
      final marked = flags == markedFlag;
      if (marked ? hiddenPrefixLength == 0 : hiddenPrefixLength != 0) {
        throw const FlarkV3BlockQuoteProjectionDecodeException(
          'Quote-line prefix geometry disagrees with its parser-authored kind.',
        );
      }

      final relativeLineEnd = _checkedU32End(
        relativeLineStart,
        physicalLength,
        'physical line',
      );
      if (relativeLineEnd > blockLengthUtf8) {
        throw const FlarkV3BlockQuoteProjectionDecodeException(
          'A quote-line record escapes its exact block.',
        );
      }
      final prefixAndContent = _checkedU32End(
        hiddenPrefixLength,
        contentLength,
        'quote prefix and content',
      );
      if (prefixAndContent > physicalLength) {
        throw const FlarkV3BlockQuoteProjectionDecodeException(
          'Quote prefix and content escape their physical line.',
        );
      }
      final lineEndingLength = physicalLength - prefixAndContent;
      if (lineEndingLength > 2 ||
          lineEndingLength == 0 && index != recordCount - 1) {
        throw const FlarkV3BlockQuoteProjectionDecodeException(
          'A quote-line record has invalid physical line-ending geometry.',
        );
      }

      final lineStartUtf8 = _checkedDocumentOffset(
        source.startUtf8,
        relativeLineStart,
        'physical line start',
      );
      final prefixEndUtf8 = _checkedDocumentOffset(
        lineStartUtf8,
        hiddenPrefixLength,
        'hidden prefix end',
      );
      final contentEndUtf8 = _checkedDocumentOffset(
        prefixEndUtf8,
        contentLength,
        'content end',
      );
      final lineEndUtf8 = _checkedDocumentOffset(
        lineStartUtf8,
        physicalLength,
        'physical line end',
      );
      final physicalSource = mapper.span(
        lineStartUtf8,
        lineEndUtf8,
        'physical quote line',
      );
      final hiddenPrefix = mapper.span(
        lineStartUtf8,
        prefixEndUtf8,
        'quote hidden prefix',
      );
      final content = mapper.span(
        prefixEndUtf8,
        contentEndUtf8,
        'quote content',
      );
      final lineEnding = mapper.span(
        contentEndUtf8,
        lineEndUtf8,
        'quote line ending',
      );
      _validatePhysicalLine(
        sourceDocument,
        physicalSource: physicalSource,
        lineEnding: lineEnding,
      );

      records.add(
        FlarkV3BlockQuoteLineProjectionRecord._(
          relativeLineStartUtf8: relativeLineStart,
          physicalSource: physicalSource,
          hiddenPrefix: hiddenPrefix,
          content: content,
          lineEnding: lineEnding,
          kind: marked
              ? FlarkV3BlockQuoteLineProjectionKind.marked
              : FlarkV3BlockQuoteLineProjectionKind.lazyContinuation,
        ),
      );
      if (hiddenPrefix.startUtf16 != hiddenPrefix.endUtf16) {
        pieces.add(
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: hiddenPrefix.startUtf16,
            sourceEndUtf16: hiddenPrefix.endUtf16,
          ),
        );
      }
      pieces.add(
        FlarkV3SourceProjectionPiece.copy(
          sourceStartUtf16: content.startUtf16,
          sourceEndUtf16: physicalSource.endUtf16,
        ),
      );

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
      relativeCursor = relativeLineEnd;
    }

    final selectedLeaf = pointPath.selectedLeaf;
    if (relativeCursor != blockLengthUtf8 ||
        projectedUtf8Length != facts.projectedUtf8Length ||
        projectedUtf16Length != facts.projectedUtf16Length ||
        projectedUtf8Length != selectedLeaf.projectedUtf8Length ||
        projectedUtf16Length != selectedLeaf.projectedUtf16Length) {
      throw const FlarkV3BlockQuoteProjectionDecodeException(
        'Quote-line records disagree with structural projection aggregates.',
      );
    }
    return FlarkV3BlockQuoteProjectionPayload._(
      sourceVersion: expectedSource,
      source: source,
      facts: facts,
      pointPath: pointPath,
      records: List.unmodifiable(records),
      projectionPieces: List.unmodifiable(pieces),
      sourceText: sourceDocument.readRange(source.startUtf16, source.endUtf16),
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
    throw const FlarkV3BlockQuoteProjectionDecodeException(
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
    throw const FlarkV3BlockQuoteProjectionDecodeException(
      'Block-quote projection source is outside the certified document.',
    );
  }
  mapper.expect(source.startUtf8, source.startUtf16, 'block start');
  mapper.expect(source.endUtf8, source.endUtf16, 'block end');
}

void _validatePointPath(
  FlarkV3SourceSpan source,
  FlarkV3BlockQuoteFacts facts,
  FlarkV3DocumentPointPath pointPath,
) {
  if (pointPath.nodes.length != 2) {
    throw const FlarkV3BlockQuoteProjectionDecodeException(
      'Block-quote projection path disagrees with its structural summary.',
    );
  }
  final ancestor = pointPath.root;
  final leaf = pointPath.selectedLeaf;
  if (ancestor.kind != FlarkV3DocumentPointPathNodeKind.blockQuote ||
      ancestor.depth != 0 ||
      ancestor.parentIndex != null ||
      ancestor.isNoncontiguous ||
      ancestor.isSelected ||
      !_sameSpan(ancestor.source, source) ||
      ancestor.firstRun != 0 ||
      ancestor.runCount != facts.lineCount ||
      ancestor.projectedUtf8Length != facts.projectedUtf8Length ||
      ancestor.projectedUtf16Length != facts.projectedUtf16Length ||
      leaf.kind != FlarkV3DocumentPointPathNodeKind.paragraph ||
      leaf.depth != 1 ||
      leaf.parentIndex != 0 ||
      !_sameSpan(leaf.source, source) ||
      !leaf.isNoncontiguous ||
      !leaf.isSelected ||
      leaf.firstRun != facts.childFirstLine ||
      leaf.runCount != facts.childLineCount ||
      leaf.projectedUtf8Length != facts.projectedUtf8Length ||
      leaf.projectedUtf16Length != facts.projectedUtf16Length) {
    throw const FlarkV3BlockQuoteProjectionDecodeException(
      'Block-quote projection path disagrees with its structural summary.',
    );
  }
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
    throw const FlarkV3BlockQuoteProjectionDecodeException(
      'A quote-line record crosses a physical line boundary.',
    );
  }
  final ending = sourceDocument.readRange(
    lineEnding.startUtf16,
    lineEnding.endUtf16,
  );
  if (ending.isNotEmpty &&
      ending != '\n' &&
      ending != '\r' &&
      ending != '\r\n') {
    throw const FlarkV3BlockQuoteProjectionDecodeException(
      'A quote-line record does not end at a physical line ending.',
    );
  }
  if (ending.length != lineEnding.endUtf8 - lineEnding.startUtf8) {
    throw const FlarkV3BlockQuoteProjectionDecodeException(
      'Quote line-ending width disagrees with exact source.',
    );
  }
}

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
    throw FlarkV3BlockQuoteProjectionDecodeException(
      '$name overflows canonical u32 geometry.',
    );
  }
  return start + length;
}

int _checkedDocumentOffset(int start, int length, String name) {
  if (start < 0 || length < 0 || start > _maximumSafeInteger - length) {
    throw FlarkV3BlockQuoteProjectionDecodeException(
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
      throw FlarkV3BlockQuoteProjectionDecodeException(
        '$name does not match exact source coordinates.',
      );
    }
  }

  FlarkV3SourceSpan span(int startUtf8, int endUtf8, String name) {
    final startUtf16 = _map(startUtf8, '$name start');
    final endUtf16 = _map(endUtf8, '$name end');
    if (endUtf16 < startUtf16) {
      throw FlarkV3BlockQuoteProjectionDecodeException(
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
      throw FlarkV3BlockQuoteProjectionDecodeException(
        '$name is outside exact source.',
      );
    }
    final cached = _cache[utf8];
    if (cached != null) return cached;
    late final int utf16;
    try {
      utf16 = _sourceDocument.utf8ToUtf16(utf8);
    } on Object {
      throw FlarkV3BlockQuoteProjectionDecodeException(
        '$name is not an exact UTF-8 scalar boundary.',
      );
    }
    _cache[utf8] = utf16;
    return utf16;
  }
}

const int _u32Maximum = 0xFFFFFFFF;
const int _maximumSafeInteger = 0x1FFFFFFFFFFFFF;

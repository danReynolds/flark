import 'dart:typed_data';

import '../../editor/flark_v3_source_projection.dart';
import '../../host/flark_v3_host_protocol.dart';
import '../../source/flark_v3_source_document.dart';
import 'flark_v3_document_query.dart';

/// One parser-authored physical line in an indented-code projection payload.
///
/// Every range is absolute in the certified document. The record reports
/// source geometry only; it does not ask Dart to recognize indentation or any
/// other Markdown construct.
final class FlarkV3IndentedCodeLineProjectionRecord {
  const FlarkV3IndentedCodeLineProjectionRecord._({
    required this.relativeLineStartUtf8,
    required this.physicalSource,
    required this.hiddenPrefix,
    required this.content,
    required this.lineEnding,
    required this.isInternalBlank,
  });

  /// UTF-8 line start relative to the enclosing indented-code block.
  final int relativeLineStartUtf8;

  /// Exact complete physical line, including its physical line ending.
  final FlarkV3SourceSpan physicalSource;

  /// Exact parser-authored prefix hidden from the live display.
  final FlarkV3SourceSpan hiddenPrefix;

  /// Exact source-backed code content after [hiddenPrefix].
  ///
  /// This is empty only for a parser-certified [isInternalBlank] line.
  final FlarkV3SourceSpan content;

  /// Exact physical CR, LF, or CRLF ending. It may be empty only on the final
  /// nonblank line.
  final FlarkV3SourceSpan lineEnding;

  final bool isInternalBlank;

  int get physicalSourceLengthUtf8 =>
      physicalSource.endUtf8 - physicalSource.startUtf8;
  int get hiddenPrefixLengthUtf8 =>
      hiddenPrefix.endUtf8 - hiddenPrefix.startUtf8;
  int get contentLengthUtf8 => content.endUtf8 - content.startUtf8;
  int get lineEndingLengthUtf8 => lineEnding.endUtf8 - lineEnding.startUtf8;
}

/// Exact, source-backed line recipe for one parser-certified code block.
///
/// [records] tile [source] completely. [projectionPieces] are mechanical
/// hide/copy instructions derived from those records, never from Markdown
/// recognition in Dart.
final class FlarkV3IndentedCodeProjectionPayload {
  const FlarkV3IndentedCodeProjectionPayload._({
    required this.sourceVersion,
    required this.source,
    required this.facts,
    required this.records,
    required this.projectionPieces,
    required String sourceText,
  }) : _sourceText = sourceText;

  /// Exact session-bearing source authority attached at decode time.
  final FlarkV3SourceVersion sourceVersion;
  int get sourceRevision => sourceVersion.revision;

  final FlarkV3SourceSpan source;
  final FlarkV3IndentedCodeFacts facts;
  final List<FlarkV3IndentedCodeLineProjectionRecord> records;
  final List<FlarkV3SourceProjectionPiece> projectionPieces;

  final String _sourceText;

  /// Builds the generic source/display projection consumed by Dart adapters.
  ///
  /// The exact authority supplied to the decoder is retained automatically;
  /// callers cannot rebind these parser-authored pieces to another session.
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

/// A corrupt or source-incompatible indented-code projection payload.
///
/// This reports a transport/authority failure, not a Markdown parse error.
final class FlarkV3IndentedCodeProjectionDecodeException implements Exception {
  const FlarkV3IndentedCodeProjectionDecodeException(this.message);

  final String message;

  @override
  String toString() => 'FlarkV3IndentedCodeProjectionDecodeException($message)';
}

/// Decoder for canonical 20-byte little-endian physical-line records.
///
/// The record layout is:
///
/// * relative line start at byte 0;
/// * physical source length at byte 4;
/// * hidden prefix length at byte 8;
/// * content length at byte 12;
/// * flags at byte 16.
///
/// All coordinates and lengths are UTF-8. Dart validates exact source
/// geometry and translates it to UTF-16, but never derives indentation runs
/// from source text.
final class FlarkV3IndentedCodeProjectionDecoder {
  const FlarkV3IndentedCodeProjectionDecoder._();

  static const int recordBytes = 20;
  static const int relativeLineStartOffset = 0;
  static const int physicalSourceLengthOffset = 4;
  static const int hiddenPrefixLengthOffset = 8;
  static const int contentLengthOffset = 12;
  static const int flagsOffset = 16;
  static const int internalBlankFlag = 1;

  static FlarkV3IndentedCodeProjectionPayload decode({
    required FlarkV3SourceDocument sourceDocument,
    required FlarkV3SourceVersion expectedSource,
    required FlarkV3SourceSpan source,
    required FlarkV3IndentedCodeFacts facts,
    required Uint8List encodedRecords,
  }) {
    if (!sourceDocument.hasCertifiedFacts) {
      throw const FlarkV3IndentedCodeProjectionDecodeException(
        'Projection decoding requires certified source coordinates.',
      );
    }
    _validateSourceAuthority(sourceDocument, expectedSource);
    final mapper = _Utf8SpanMapper(sourceDocument);
    _validateBlockSource(source, sourceDocument, mapper);
    if (facts.deindentColumns != 4) {
      throw const FlarkV3IndentedCodeProjectionDecodeException(
        'Indented-code projection uses an unsupported deindent recipe.',
      );
    }
    if (encodedRecords.lengthInBytes % recordBytes != 0) {
      throw const FlarkV3IndentedCodeProjectionDecodeException(
        'Projection payload is not a whole number of canonical records.',
      );
    }
    final recordCount = encodedRecords.lengthInBytes ~/ recordBytes;
    if (recordCount == 0 || recordCount != facts.lineCount) {
      throw const FlarkV3IndentedCodeProjectionDecodeException(
        'Projection record count does not match the structural line summary.',
      );
    }

    final blockLengthUtf8 = source.endUtf8 - source.startUtf8;
    final data = ByteData.sublistView(encodedRecords);
    final records = <FlarkV3IndentedCodeLineProjectionRecord>[];
    final pieces = <FlarkV3SourceProjectionPiece>[];
    var relativeCursor = 0;
    var projectedUtf8Length = 0;
    var projectedUtf16Length = 0;
    var terminalLineEndingBytes = 0;

    for (var index = 0; index < recordCount; index += 1) {
      final offset = index * recordBytes;
      final relativeLineStart = data.getUint32(
        offset + relativeLineStartOffset,
        Endian.little,
      );
      final physicalLength = data.getUint32(
        offset + physicalSourceLengthOffset,
        Endian.little,
      );
      final hiddenPrefixLength = data.getUint32(
        offset + hiddenPrefixLengthOffset,
        Endian.little,
      );
      final contentLength = data.getUint32(
        offset + contentLengthOffset,
        Endian.little,
      );
      final flags = data.getUint32(offset + flagsOffset, Endian.little);
      if ((flags & ~internalBlankFlag) != 0) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Projection record contains unknown flags.',
        );
      }
      if (relativeLineStart != relativeCursor || physicalLength == 0) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Projection records do not form a contiguous physical-line tiling.',
        );
      }

      final relativeLineEnd = _checkedU32End(
        relativeLineStart,
        physicalLength,
        'physical line',
      );
      if (relativeLineEnd > blockLengthUtf8) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Projection physical line escapes its exact block.',
        );
      }
      final prefixAndContent = _checkedU32End(
        hiddenPrefixLength,
        contentLength,
        'hidden prefix and content',
      );
      if (prefixAndContent > physicalLength) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Projection prefix and content escape their physical line.',
        );
      }
      final lineEndingLength = physicalLength - prefixAndContent;
      if (lineEndingLength > 2) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Projection record has a non-canonical physical line ending width.',
        );
      }

      final internalBlank = (flags & internalBlankFlag) != 0;
      if (internalBlank) {
        if (index == 0 ||
            index == recordCount - 1 ||
            contentLength != 0 ||
            lineEndingLength == 0) {
          throw const FlarkV3IndentedCodeProjectionDecodeException(
            'Internal blank records must be bounded, empty physical lines.',
          );
        }
      } else if (hiddenPrefixLength == 0 || contentLength == 0) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Nonblank code lines require a hidden prefix and source content.',
        );
      }
      if (lineEndingLength == 0 && index != recordCount - 1) {
        throw const FlarkV3IndentedCodeProjectionDecodeException(
          'Only the final physical line may omit its line ending.',
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
        'physical line',
      );
      final hiddenPrefix = mapper.span(
        lineStartUtf8,
        prefixEndUtf8,
        'hidden prefix',
      );
      final content = mapper.span(
        prefixEndUtf8,
        contentEndUtf8,
        'code content',
      );
      final lineEnding = mapper.span(
        contentEndUtf8,
        lineEndUtf8,
        'physical line ending',
      );
      _validatePhysicalLine(
        sourceDocument,
        physicalSource: physicalSource,
        lineEnding: lineEnding,
      );
      _validateHiddenPrefix(
        sourceDocument,
        hiddenPrefix,
        isFirst: index == 0,
        hasBofBom: facts.hasBofBom,
        internalBlank: internalBlank,
      );

      records.add(
        FlarkV3IndentedCodeLineProjectionRecord._(
          relativeLineStartUtf8: relativeLineStart,
          physicalSource: physicalSource,
          hiddenPrefix: hiddenPrefix,
          content: content,
          lineEnding: lineEnding,
          isInternalBlank: internalBlank,
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
      if (hiddenPrefix.endUtf16 != physicalSource.endUtf16) {
        pieces.add(
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: hiddenPrefix.endUtf16,
            sourceEndUtf16: physicalSource.endUtf16,
          ),
        );
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
      terminalLineEndingBytes = lineEndingLength;
      relativeCursor = relativeLineEnd;
    }

    if (relativeCursor != blockLengthUtf8) {
      throw const FlarkV3IndentedCodeProjectionDecodeException(
        'Projection records do not exhaust their exact block.',
      );
    }
    if (projectedUtf8Length != facts.projectedUtf8Length ||
        projectedUtf16Length != facts.projectedUtf16Length ||
        terminalLineEndingBytes != facts.terminalLineEndingBytes) {
      throw const FlarkV3IndentedCodeProjectionDecodeException(
        'Projection records do not match the structural projection summary.',
      );
    }

    return FlarkV3IndentedCodeProjectionPayload._(
      sourceVersion: expectedSource,
      source: source,
      facts: facts,
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
    throw const FlarkV3IndentedCodeProjectionDecodeException(
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
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Indented-code projection source is outside the certified document.',
    );
  }
  mapper.expect(source.startUtf8, source.startUtf16, 'block start');
  mapper.expect(source.endUtf8, source.endUtf16, 'block end');
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
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Projection record crosses a physical line boundary.',
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
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Projection record does not end at an exact physical line ending.',
    );
  }
  final utf8Length = lineEnding.endUtf8 - lineEnding.startUtf8;
  if (ending.length != utf8Length) {
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Projection physical line-ending width does not match exact source.',
    );
  }
}

void _validateHiddenPrefix(
  FlarkV3SourceDocument sourceDocument,
  FlarkV3SourceSpan hiddenPrefix, {
  required bool isFirst,
  required bool hasBofBom,
  required bool internalBlank,
}) {
  var prefix = sourceDocument.readRange(
    hiddenPrefix.startUtf16,
    hiddenPrefix.endUtf16,
  );
  final ownsBofBom = isFirst && prefix.startsWith('\uFEFF');
  if (isFirst &&
      (ownsBofBom != hasBofBom ||
          hasBofBom &&
              (hiddenPrefix.startUtf8 != 0 || hiddenPrefix.startUtf16 != 0))) {
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Projection BOF BOM ownership does not match its structural summary.',
    );
  }
  if (ownsBofBom) prefix = prefix.substring(1);
  if (prefix.codeUnits.any(
    (codeUnit) => codeUnit != 0x20 && codeUnit != 0x09,
  )) {
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Projection hidden prefix is not source horizontal whitespace.',
    );
  }
  if (!internalBlank && prefix.isEmpty) {
    throw const FlarkV3IndentedCodeProjectionDecodeException(
      'Nonblank projection prefix contains no source indentation.',
    );
  }
}

int _checkedU32End(int start, int length, String name) {
  if (start < 0 ||
      start > _u32Maximum ||
      length < 0 ||
      length > _u32Maximum - start) {
    throw FlarkV3IndentedCodeProjectionDecodeException(
      '$name overflows canonical u32 geometry.',
    );
  }
  return start + length;
}

int _checkedDocumentOffset(int start, int length, String name) {
  if (start < 0 || length < 0 || start > _maximumSafeInteger - length) {
    throw FlarkV3IndentedCodeProjectionDecodeException(
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
      throw FlarkV3IndentedCodeProjectionDecodeException(
        '$name does not match exact source coordinates.',
      );
    }
  }

  FlarkV3SourceSpan span(int startUtf8, int endUtf8, String name) {
    final startUtf16 = _map(startUtf8, '$name start');
    final endUtf16 = _map(endUtf8, '$name end');
    if (endUtf16 < startUtf16) {
      throw FlarkV3IndentedCodeProjectionDecodeException(
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
      throw FlarkV3IndentedCodeProjectionDecodeException(
        '$name is outside exact source.',
      );
    }
    final cached = _cache[utf8];
    if (cached != null) return cached;
    late final int utf16;
    try {
      utf16 = _sourceDocument.utf8ToUtf16(utf8);
    } on Object {
      throw FlarkV3IndentedCodeProjectionDecodeException(
        '$name is not an exact UTF-8 scalar boundary.',
      );
    }
    _cache[utf8] = utf16;
    return utf16;
  }
}

const int _u32Maximum = 0xFFFFFFFF;
const int _maximumSafeInteger = 0x1FFFFFFFFFFFFF;

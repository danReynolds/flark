import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('Setext H1 exposes exact content, underline, and CRLF geometry', () {
    const source = '**H1 β😀**\r\n  ===  \r\n';
    final authority = _authority(source);
    final sourceEnd = utf8.encode(source).length;
    final underlineStart = _byteOffsetOf(source, '===');
    final underlineEnd = underlineStart + 3;
    final lineEndingStart = underlineEnd + 2;
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: sourceEnd,
      contentStart: 0,
      contentEnd: underlineStart - 4,
      level: 1,
      openingIndent: 2,
      underlineStart: underlineStart,
      underlineEnd: underlineEnd,
      lineEndingStart: lineEndingStart,
      lineEndingEnd: sourceEnd,
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.heading);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.heading);
    expect(result.structure.canCarryInlineFacts, isTrue);
    expect(result.structure.referenceDefinitionCount, 0);
    final heading = result.structure.heading! as FlarkV3SetextHeadingFacts;
    expect(heading.level, 1);
    expect(heading.openingIndent, 2);
    _expectSpan(
      authority.document,
      heading.contentSource,
      0,
      underlineStart - 4,
    );
    _expectSpan(
      authority.document,
      heading.contentLineEnding,
      underlineStart - 4,
      underlineStart - 2,
    );
    _expectSpan(
      authority.document,
      heading.underlineMarker,
      underlineStart,
      underlineEnd,
    );
    _expectSpan(
      authority.document,
      heading.underlineLineEnding,
      lineEndingStart,
      sourceEnd,
    );
    _expectSpan(
      authority.document,
      result.projection.projectedSource,
      0,
      underlineStart - 4,
    );
  });

  test('Setext H2 accepts an empty underline line ending', () {
    const source = 'Title\n---';
    final authority = _authority(source);
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: source.length,
      contentStart: 0,
      contentEnd: 5,
      level: 2,
      openingIndent: 0,
      underlineStart: 6,
      underlineEnd: 9,
      lineEndingStart: 9,
      lineEndingEnd: 9,
    );

    final heading = result.structure.heading! as FlarkV3SetextHeadingFacts;
    expect(heading.level, 2);
    expect(heading.openingIndent, 0);
    _expectSpan(authority.document, heading.contentSource, 0, 5);
    _expectSpan(authority.document, heading.contentLineEnding, 5, 6);
    _expectSpan(authority.document, heading.underlineMarker, 6, 9);
    _expectSpan(authority.document, heading.underlineLineEnding, 9, 9);
  });

  test('Setext preserves a parser-certified leading definition count', () {
    const source = '[ref]: /target\n\nTitle\n---\n';
    final authority = _authority(source);
    final contentStart = _byteOffsetOf(source, 'Title');
    final underlineStart = _byteOffsetOf(source, '---');
    final sourceEnd = utf8.encode(source).length;
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: sourceEnd,
      contentStart: contentStart,
      contentEnd: underlineStart - 1,
      level: 2,
      openingIndent: 0,
      underlineStart: underlineStart,
      underlineEnd: underlineStart + 3,
      lineEndingStart: underlineStart + 3,
      lineEndingEnd: sourceEnd,
      referenceDefinitionCount: 1,
    );

    expect(result.structure.referenceDefinitionCount, 1);
    final heading = result.structure.heading! as FlarkV3SetextHeadingFacts;
    _expectSpan(
      authority.document,
      heading.contentSource,
      contentStart,
      underlineStart - 1,
    );
    _expectSpan(
      authority.document,
      heading.contentLineEnding,
      underlineStart - 1,
      underlineStart,
    );
  });

  test('heading decoder rejects a Green/projection syntax mismatch', () {
    const source = 'Title\n---\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        contentStart: 0,
        contentEnd: 5,
        level: 2,
        openingIndent: 0,
        underlineStart: 6,
        underlineEnd: 9,
        lineEndingStart: 9,
        lineEndingEnd: 10,
        projectionVariant: 4,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('Setext decoder rejects invalid levels and reserved metadata', () {
    const source = 'Title\n---\n';
    final authority = _authority(source);

    for (final metadata in [3, 2 | (1 << 10)]) {
      expect(
        () => _decode(
          authority,
          sourceStart: 0,
          sourceEnd: source.length,
          contentStart: 0,
          contentEnd: 5,
          level: 2,
          openingIndent: 0,
          underlineStart: 6,
          underlineEnd: 9,
          lineEndingStart: 9,
          lineEndingEnd: 10,
          metadataOverride: metadata,
        ),
        throwsA(isA<FlarkV3DocumentQueryException>()),
      );
    }
  });

  test('Setext decoder rejects contradictory indent and marker geometry', () {
    const source = 'Title\n---\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        contentStart: 0,
        contentEnd: 5,
        level: 2,
        openingIndent: 1,
        underlineStart: 6,
        underlineEnd: 9,
        lineEndingStart: 9,
        lineEndingEnd: 10,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        contentStart: 0,
        contentEnd: 5,
        level: 2,
        openingIndent: 0,
        underlineStart: 6,
        underlineEnd: 6,
        lineEndingStart: 9,
        lineEndingEnd: 10,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        contentStart: 0,
        contentEnd: 3,
        level: 2,
        openingIndent: 0,
        underlineStart: 6,
        underlineEnd: 9,
        lineEndingStart: 9,
        lineEndingEnd: 10,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('Setext decoder rejects an oversized underline line ending', () {
    const source = 'Title\n---abc';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        contentStart: 0,
        contentEnd: 5,
        level: 2,
        openingIndent: 0,
        underlineStart: 6,
        underlineEnd: 9,
        lineEndingStart: 9,
        lineEndingEnd: 12,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });
}

({FlarkV3SourceDocument document, FlarkV3SourceVersion version}) _authority(
  String source,
) {
  final document = FlarkV3SourceDocument.fromString(source);
  return (
    document: document,
    version: FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
      document: document,
    ),
  );
}

FlarkV3DocumentStructuralQuery _decode(
  ({FlarkV3SourceDocument document, FlarkV3SourceVersion version}) authority, {
  required int sourceStart,
  required int sourceEnd,
  required int contentStart,
  required int contentEnd,
  required int level,
  required int openingIndent,
  required int underlineStart,
  required int underlineEnd,
  required int lineEndingStart,
  required int lineEndingEnd,
  int referenceDefinitionCount = 0,
  int? metadataOverride,
  int projectionVariant = 5,
}) {
  final encoded = _viewport(
    sourceStart: sourceStart,
    sourceEnd: sourceEnd,
    contentStart: contentStart,
    contentEnd: contentEnd,
    level: level,
    openingIndent: openingIndent,
    underlineStart: underlineStart,
    underlineEnd: underlineEnd,
    lineEndingStart: lineEndingStart,
    lineEndingEnd: lineEndingEnd,
    referenceDefinitionCount: referenceDefinitionCount,
    metadataOverride: metadataOverride,
    projectionVariant: projectionVariant,
  );
  return FlarkV3DocumentQueryDecoder.decode(
    sourceDocument: authority.document,
    expectedSource: authority.version,
    expectedProfilePartition: 1,
    viewport: FlarkV3HostStructuralViewport.owned(
      sourceVersion: authority.version,
      range: FlarkV3MetricRange(
        start: FlarkV3SourceMetric(
          bytes: sourceStart,
          utf16: authority.document.utf8ToUtf16(sourceStart),
        ),
        end: FlarkV3SourceMetric(
          bytes: sourceEnd,
          utf16: authority.document.utf8ToUtf16(sourceEnd),
        ),
      ),
      encoded: encoded,
      receipt: FlarkV3HostViewportReceipt(
        encodedBytes: encoded.length,
        leafCount: 2,
        openDepth: 1,
        treeNodesVisited: 2,
        summaryNodesSkipped: 0,
      ),
    ),
  );
}

Uint8List _viewport({
  required int sourceStart,
  required int sourceEnd,
  required int contentStart,
  required int contentEnd,
  required int level,
  required int openingIndent,
  required int underlineStart,
  required int underlineEnd,
  required int lineEndingStart,
  required int lineEndingEnd,
  required int referenceDefinitionCount,
  required int? metadataOverride,
  required int projectionVariant,
}) {
  final bytes = Uint8List(20 + 80 + 56);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 1, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little);

  const green = 20;
  bytes.setRange(green, green + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(green + 8, 1, Endian.little)
    ..setUint8(green + 12, 5)
    ..setUint64(green + 16, sourceStart, Endian.little)
    ..setUint64(green + 24, sourceEnd, Endian.little)
    ..setUint64(green + 32, contentStart, Endian.little)
    ..setUint64(green + 40, contentEnd, Endian.little)
    ..setUint64(
      green + 48,
      metadataOverride ?? (level | (openingIndent << 8)),
      Endian.little,
    )
    ..setUint32(green + 56, underlineStart, Endian.little)
    ..setUint32(green + 60, underlineEnd, Endian.little)
    ..setUint32(green + 64, lineEndingStart, Endian.little)
    ..setUint32(green + 68, lineEndingEnd, Endian.little)
    ..setUint64(green + 72, referenceDefinitionCount, Endian.little);

  const projection = green + 80;
  bytes.setRange(projection, projection + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(projection + 8, 1, Endian.little)
    ..setUint8(projection + 12, projectionVariant)
    ..setUint64(projection + 16, sourceStart, Endian.little)
    ..setUint64(projection + 24, sourceEnd, Endian.little)
    ..setUint64(projection + 32, contentStart, Endian.little)
    ..setUint64(projection + 40, contentEnd, Endian.little)
    ..setUint64(projection + 48, 1, Endian.little);
  return bytes;
}

int _byteOffsetOf(String source, String needle, {int start = 0}) {
  final utf16Index = source.indexOf(needle, start);
  if (utf16Index < 0) {
    throw StateError('Missing test needle: $needle');
  }
  return utf8.encode(source.substring(0, utf16Index)).length;
}

void _expectSpan(
  FlarkV3SourceDocument document,
  FlarkV3SourceSpan span,
  int startUtf8,
  int endUtf8,
) {
  expect(span.startUtf8, startUtf8);
  expect(span.endUtf8, endUtf8);
  expect(span.startUtf16, document.utf8ToUtf16(startUtf8));
  expect(span.endUtf16, document.utf8ToUtf16(endUtf8));
}

import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/editor/flark_v3_inline_island_presentation.dart';
import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test(
    'ATX viewport exposes marker-free geometry and heading inline facts',
    () {
      const source = 'p\n\n  ###  **é** ###  \r\n';
      final authority = _authority(source);
      final result = _decode(
        authority,
        sourceStart: 3,
        sourceEnd: 24,
        contentStart: 10,
        contentEnd: 16,
        level: 3,
        openingIndent: 2,
        openingStart: 5,
        openingEnd: 8,
        closingStart: 17,
        closingEnd: 20,
        lineEndingStart: 22,
        lineEndingEnd: 24,
        inlineRecord: _inlineRecord(
          kind: 2,
          length: 6,
          contentStart: 2,
          contentLength: 2,
        ),
      );

      expect(result.structure.kind, FlarkV3DocumentStructureKind.heading);
      expect(result.projection.kind, FlarkV3DocumentStructureKind.heading);
      expect(result.structure.referenceDefinitionCount, 0);
      expect(result.structure.canCarryInlineFacts, isTrue);
      _expectSpan(result.structure.source, 3, 24, 3, 23);
      _expectSpan(result.structure.inlineContentSource!, 10, 16, 10, 15);
      _expectSpan(result.projection.projectedSource, 10, 16, 10, 15);

      final heading = result.structure.heading! as FlarkV3AtxHeadingFacts;
      expect(heading.level, 3);
      expect(heading.hasClosingMarker, isTrue);
      _expectSpan(heading.openingMarker, 5, 8, 5, 8);
      _expectSpan(heading.contentSource, 10, 16, 10, 15);
      _expectSpan(heading.closingMarker!, 17, 20, 16, 19);

      final inline = result.inlineFacts!;
      expect(inline.disposition, FlarkV3InlineFactsDisposition.authoritative);
      expect(inline.facts, hasLength(1));
      expect(inline.facts.single.kind, FlarkV3InlineFactKind.strong);
      _expectSpan(inline.facts.single.source, 10, 16, 10, 15);
      _expectSpan(inline.facts.single.content, 12, 14, 12, 13);

      expect(
        FlarkV3InlineIslandPresentation.resolve(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          structuralQuery: result,
          activeIsland: heading.contentSource,
        ),
        isA<FlarkV3AuthoritativeInlineIslandPresentation>(),
      );
    },
  );

  test('ATX viewport joins an authenticated direct-link value trailer', () {
    const source = '# [x](&bsol;*)\n';
    final authority = _authority(source);
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: 15,
      contentStart: 2,
      contentEnd: 14,
      level: 1,
      openingIndent: 0,
      openingStart: 0,
      openingEnd: 1,
      lineEndingStart: 14,
      lineEndingEnd: 15,
      inlineRecord: _inlineRecord(
        kind: 10,
        length: 12,
        contentStart: 1,
        contentLength: 1,
      ),
      inlineValues: _directLinkValues(
        destinationStart: 4,
        destinationLength: 7,
        cookedDestination: '*',
      ),
    );

    final fact = result.inlineFacts!.facts.single;
    expect(fact.kind, FlarkV3InlineFactKind.directLink);
    expect(fact.linkAnnotation!.kind, FlarkV3InlineLinkKind.direct);
    expect(fact.linkAnnotation!.destination, '*');
    expect(fact.linkAnnotation!.title, isNull);
    _expectSpan(fact.content, 3, 4, 3, 4);
    _expectSpan(fact.linkAnnotation!.destinationSource, 6, 13, 6, 13);
  });

  test('ATX viewport preserves an absent closing marker', () {
    const source = '######\tβ#   \r';
    final authority = _authority(source);
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: 14,
      contentStart: 7,
      contentEnd: 10,
      level: 6,
      openingIndent: 0,
      openingStart: 0,
      openingEnd: 6,
      lineEndingStart: 13,
      lineEndingEnd: 14,
    );

    final heading = result.structure.heading! as FlarkV3AtxHeadingFacts;
    expect(heading.level, 6);
    expect(heading.hasClosingMarker, isFalse);
    expect(heading.closingMarker, isNull);
    _expectSpan(heading.contentSource, 7, 10, 7, 9);
  });

  test('ATX decoder accepts parser-certified BOF BOM geometry', () {
    const source = '\uFEFF # x\n';
    final authority = _authority(source);
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: 8,
      contentStart: 6,
      contentEnd: 7,
      level: 1,
      openingIndent: 1,
      openingStart: 4,
      openingEnd: 5,
      lineEndingStart: 7,
      lineEndingEnd: 8,
      hasBofBom: true,
    );

    final heading = result.structure.heading! as FlarkV3AtxHeadingFacts;
    _expectSpan(heading.openingMarker, 4, 5, 2, 3);
    _expectSpan(heading.contentSource, 6, 7, 4, 5);
  });

  test('ATX decoder rejects a BOM claim away from byte zero', () {
    const source = 'p\n\uFEFF# x\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 2,
        sourceEnd: 9,
        contentStart: 7,
        contentEnd: 8,
        level: 1,
        openingIndent: 0,
        openingStart: 5,
        openingEnd: 6,
        lineEndingStart: 8,
        lineEndingEnd: 9,
        hasBofBom: true,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('ATX decoder fails closed on contradictory closing metadata', () {
    const source = '# heading\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: 10,
        contentStart: 2,
        contentEnd: 9,
        level: 1,
        openingIndent: 0,
        openingStart: 0,
        openingEnd: 1,
        closingStart: 0,
        closingEnd: 1,
        lineEndingStart: 9,
        lineEndingEnd: 10,
        closed: false,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('ATX decoder rejects structurally impossible opening indentation', () {
    const source = '    # x\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: 8,
        contentStart: 6,
        contentEnd: 7,
        level: 1,
        openingIndent: 0,
        openingStart: 4,
        openingEnd: 5,
        lineEndingStart: 7,
        lineEndingEnd: 8,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('empty ATX content cannot claim an inline sidecar', () {
    const source = '#\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: 2,
        contentStart: 1,
        contentEnd: 1,
        level: 1,
        openingIndent: 0,
        openingStart: 0,
        openingEnd: 1,
        lineEndingStart: 1,
        lineEndingEnd: 2,
        inlineRecord: Uint8List(0),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('inline viewport rejects retired structural and payload schemas', () {
    const source = '# *x*\n';
    final authority = _authority(source);
    final record = _inlineRecord(
      kind: 1,
      length: 3,
      contentStart: 1,
      contentLength: 1,
    );

    FlarkV3DocumentStructuralQuery decode({
      int? viewportSchema,
      String inlineMagic = 'FLKIN002',
      int inlineSchema = 2,
    }) => _decode(
      authority,
      sourceStart: 0,
      sourceEnd: 6,
      contentStart: 2,
      contentEnd: 5,
      level: 1,
      openingIndent: 0,
      openingStart: 0,
      openingEnd: 1,
      lineEndingStart: 5,
      lineEndingEnd: 6,
      inlineRecord: record,
      viewportSchema: viewportSchema,
      inlineMagic: inlineMagic,
      inlineSchema: inlineSchema,
    );

    expect(
      () => decode(viewportSchema: 2),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => decode(inlineSchema: 1),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => decode(inlineMagic: 'FLKIN001'),
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
  required int openingStart,
  required int openingEnd,
  int? closingStart,
  int? closingEnd,
  required int lineEndingStart,
  required int lineEndingEnd,
  bool? closed,
  bool hasBofBom = false,
  Uint8List? inlineRecord,
  Uint8List? inlineValues,
  int? viewportSchema,
  String inlineMagic = 'FLKIN002',
  int inlineSchema = 2,
}) {
  final encoded = _viewport(
    sourceStart: sourceStart,
    sourceEnd: sourceEnd,
    contentStart: contentStart,
    contentEnd: contentEnd,
    level: level,
    openingIndent: openingIndent,
    openingStart: openingStart,
    openingEnd: openingEnd,
    closingStart: closingStart,
    closingEnd: closingEnd,
    lineEndingStart: lineEndingStart,
    lineEndingEnd: lineEndingEnd,
    closed: closed,
    hasBofBom: hasBofBom,
    inlineRecord: inlineRecord,
    inlineValues: inlineValues,
    viewportSchema: viewportSchema,
    inlineMagic: inlineMagic,
    inlineSchema: inlineSchema,
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
        leafCount: inlineRecord == null ? 2 : 3,
        openDepth: 1,
        treeNodesVisited: inlineRecord == null ? 2 : 3,
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
  required int openingStart,
  required int openingEnd,
  int? closingStart,
  int? closingEnd,
  required int lineEndingStart,
  required int lineEndingEnd,
  bool? closed,
  bool hasBofBom = false,
  Uint8List? inlineRecord,
  Uint8List? inlineValues,
  int? viewportSchema,
  String inlineMagic = 'FLKIN002',
  int inlineSchema = 2,
}) {
  final includeInline = inlineRecord != null;
  final inlineLength = includeInline
      ? 48 + inlineRecord.length + (inlineValues?.length ?? 0)
      : 0;
  final headerBytes = includeInline ? 24 : 20;
  final bytes = Uint8List(headerBytes + 80 + 56 + inlineLength);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, viewportSchema ?? (includeInline ? 8 : 1), Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little);
  if (includeInline) {
    data.setUint32(20, inlineLength, Endian.little);
  }

  final hasClosing = closed ?? closingStart != null;
  final green = headerBytes;
  bytes.setRange(green, green + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(green + 8, 1, Endian.little)
    ..setUint8(green + 12, 4)
    ..setUint64(green + 16, sourceStart, Endian.little)
    ..setUint64(green + 24, sourceEnd, Endian.little)
    ..setUint64(green + 32, contentStart, Endian.little)
    ..setUint64(green + 40, contentEnd, Endian.little)
    ..setUint64(
      green + 48,
      level |
          (hasClosing ? 1 << 8 : 0) |
          (openingIndent << 9) |
          (hasBofBom ? 1 << 11 : 0),
      Endian.little,
    )
    ..setUint32(green + 56, openingStart, Endian.little)
    ..setUint32(green + 60, openingEnd, Endian.little)
    ..setUint32(green + 64, closingStart ?? 0xffffffff, Endian.little)
    ..setUint32(green + 68, closingEnd ?? 0xffffffff, Endian.little)
    ..setUint32(green + 72, lineEndingStart, Endian.little)
    ..setUint32(green + 76, lineEndingEnd, Endian.little);

  final projection = green + 80;
  bytes.setRange(projection, projection + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(projection + 8, 1, Endian.little)
    ..setUint8(projection + 12, 4)
    ..setUint64(projection + 16, sourceStart, Endian.little)
    ..setUint64(projection + 24, sourceEnd, Endian.little)
    ..setUint64(projection + 32, contentStart, Endian.little)
    ..setUint64(projection + 40, contentEnd, Endian.little)
    ..setUint64(projection + 48, 1, Endian.little);

  if (includeInline) {
    final inline = projection + 56;
    bytes.setRange(inline, inline + 8, ascii.encode(inlineMagic));
    data
      ..setUint32(inline + 8, inlineSchema, Endian.little)
      ..setUint8(inline + 12, 1)
      ..setUint32(inline + 16, 1, Endian.little)
      ..setUint32(inline + 20, inlineRecord.length ~/ 20, Endian.little)
      ..setUint64(inline + 24, contentStart, Endian.little)
      ..setUint64(inline + 32, contentEnd, Endian.little)
      ..setUint32(inline + 40, 20, Endian.little)
      ..setUint32(inline + 44, 0, Endian.little);
    final factsEnd = inline + 48 + inlineRecord.length;
    bytes.setRange(inline + 48, factsEnd, inlineRecord);
    if (inlineValues != null) {
      bytes.setRange(factsEnd, bytes.length, inlineValues);
    }
  }
  return bytes;
}

Uint8List _inlineRecord({
  required int kind,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(20);
  ByteData.sublistView(bytes)
    ..setUint8(0, kind)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}

Uint8List _directLinkValues({
  required int destinationStart,
  required int destinationLength,
  required String cookedDestination,
}) {
  final cooked = utf8.encode(cookedDestination);
  final bytes = Uint8List(16 + 32 + cooked.length);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKIV001'));
  data
    ..setUint32(8, 1, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, 0, Endian.little)
    ..setUint32(20, 0, Endian.little)
    ..setUint32(24, destinationStart, Endian.little)
    ..setUint32(28, destinationLength, Endian.little)
    ..setUint32(32, 0, Endian.little)
    ..setUint32(36, 0, Endian.little)
    ..setUint32(40, cooked.length, Endian.little)
    ..setUint32(44, 0, Endian.little);
  bytes.setRange(48, bytes.length, cooked);
  return bytes;
}

void _expectSpan(
  FlarkV3SourceSpan span,
  int startUtf8,
  int endUtf8,
  int startUtf16,
  int endUtf16,
) {
  expect(span.startUtf8, startUtf8);
  expect(span.endUtf8, endUtf8);
  expect(span.startUtf16, startUtf16);
  expect(span.endUtf16, endUtf16);
}

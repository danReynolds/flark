import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('thematic break exposes empty projection and exact CRLF geometry', () {
    const source = '  * * * \r\n';
    final authority = _authority(source);
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: utf8.encode(source).length,
      marker: _asterisk,
      openingIndent: 2,
      markerStart: 2,
      markerEnd: 7,
      lineEndingStart: 8,
      lineEndingEnd: 10,
      markerCount: 3,
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.thematicBreak);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.thematicBreak);
    expect(result.structure.referenceDefinitionCount, 0);
    expect(result.structure.inlineContentSource, isNull);
    expect(result.structure.canCarryInlineFacts, isFalse);
    expect(result.inlineFacts, isNull);
    expect(result.projection.runCount, 0);
    _expectSpan(authority.document, result.structure.source, 0, 10);
    _expectSpan(authority.document, result.structure.visibleSource, 0, 0);
    _expectSpan(authority.document, result.projection.projectedSource, 0, 0);

    final thematicBreak = result.structure.thematicBreak!;
    expect(thematicBreak.marker, FlarkV3ThematicBreakMarker.asterisk);
    expect(thematicBreak.markerCount, 3);
    expect(thematicBreak.openingIndent, 2);
    expect(thematicBreak.hasBofBom, isFalse);
    _expectSpan(authority.document, thematicBreak.markerEnvelope, 2, 7);
    _expectSpan(authority.document, thematicBreak.lineEnding, 8, 10);
  });

  test('hyphen EOF and underscore breaks preserve exact UTF-8/UTF-16', () {
    const hyphenSource = 'β😀\n\n---';
    final hyphenAuthority = _authority(hyphenSource);
    final hyphenStart = _byteOffsetOf(hyphenSource, '---');
    final hyphenEnd = utf8.encode(hyphenSource).length;
    final hyphen = _decode(
      hyphenAuthority,
      sourceStart: hyphenStart,
      sourceEnd: hyphenEnd,
      marker: _hyphen,
      openingIndent: 0,
      markerStart: hyphenStart,
      markerEnd: hyphenEnd,
      lineEndingStart: hyphenEnd,
      lineEndingEnd: hyphenEnd,
      markerCount: 3,
    );

    expect(
      hyphen.structure.thematicBreak!.marker,
      FlarkV3ThematicBreakMarker.hyphen,
    );
    _expectSpan(
      hyphenAuthority.document,
      hyphen.structure.source,
      hyphenStart,
      hyphenEnd,
    );
    _expectSpan(
      hyphenAuthority.document,
      hyphen.structure.visibleSource,
      hyphenStart,
      hyphenStart,
    );
    _expectSpan(
      hyphenAuthority.document,
      hyphen.structure.thematicBreak!.lineEnding,
      hyphenEnd,
      hyphenEnd,
    );

    const underscoreSource = '_ _ _\n';
    final underscoreAuthority = _authority(underscoreSource);
    final underscore = _decode(
      underscoreAuthority,
      sourceStart: 0,
      sourceEnd: underscoreSource.length,
      marker: _underscore,
      openingIndent: 0,
      markerStart: 0,
      markerEnd: 5,
      lineEndingStart: 5,
      lineEndingEnd: 6,
      markerCount: 3,
    );

    expect(
      underscore.structure.thematicBreak!.marker,
      FlarkV3ThematicBreakMarker.underscore,
    );
    _expectSpan(
      underscoreAuthority.document,
      underscore.structure.thematicBreak!.markerEnvelope,
      0,
      5,
    );
  });

  test('thematic break accepts parser-certified BOF BOM geometry', () {
    const source = '\uFEFF___\n';
    final authority = _authority(source);
    final sourceEnd = utf8.encode(source).length;
    final result = _decode(
      authority,
      sourceStart: 0,
      sourceEnd: sourceEnd,
      marker: _underscore,
      openingIndent: 0,
      hasBofBom: true,
      markerStart: 3,
      markerEnd: 6,
      lineEndingStart: 6,
      lineEndingEnd: sourceEnd,
      markerCount: 3,
    );

    final thematicBreak = result.structure.thematicBreak!;
    expect(thematicBreak.hasBofBom, isTrue);
    expect(thematicBreak.openingIndent, 0);
    _expectSpan(authority.document, result.structure.source, 0, sourceEnd);
    _expectSpan(authority.document, thematicBreak.markerEnvelope, 3, 6);
    _expectSpan(authority.document, thematicBreak.lineEnding, 6, sourceEnd);
  });

  test('thematic-break decoder rejects invalid marker and marker counts', () {
    const source = '***\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: 0x78,
        openingIndent: 0,
        markerStart: 0,
        markerEnd: 3,
        lineEndingStart: 3,
        lineEndingEnd: 4,
        markerCount: 3,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    for (final corruptCount in [2, 4]) {
      expect(
        () => _decode(
          authority,
          sourceStart: 0,
          sourceEnd: source.length,
          marker: _asterisk,
          openingIndent: 0,
          markerStart: 0,
          markerEnd: 3,
          lineEndingStart: 3,
          lineEndingEnd: 4,
          markerCount: corruptCount,
        ),
        throwsA(isA<FlarkV3DocumentQueryException>()),
      );
    }
  });

  test('thematic-break decoder rejects contradictory indent and BOM', () {
    const source = '***\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: _asterisk,
        openingIndent: 1,
        markerStart: 0,
        markerEnd: 3,
        lineEndingStart: 3,
        lineEndingEnd: 4,
        markerCount: 3,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );

    const prefixedSource = 'p\n***\n';
    final prefixedAuthority = _authority(prefixedSource);
    expect(
      () => _decode(
        prefixedAuthority,
        sourceStart: 2,
        sourceEnd: prefixedSource.length,
        marker: _asterisk,
        openingIndent: 0,
        hasBofBom: true,
        markerStart: 2,
        markerEnd: 5,
        lineEndingStart: 5,
        lineEndingEnd: 6,
        markerCount: 3,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('thematic-break decoder rejects geometry and reserved metadata', () {
    const source = '***abc';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: _asterisk,
        openingIndent: 0,
        markerStart: 0,
        markerEnd: 3,
        lineEndingStart: 3,
        lineEndingEnd: 6,
        markerCount: 3,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: _asterisk,
        openingIndent: 0,
        markerStart: 0,
        markerEnd: 0,
        lineEndingStart: 3,
        lineEndingEnd: 6,
        markerCount: 3,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: _asterisk,
        openingIndent: 0,
        markerStart: 0,
        markerEnd: 3,
        lineEndingStart: 3,
        lineEndingEnd: 6,
        markerCount: 3,
        metadataOverride: _asterisk | (1 << 11),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('thematic-break projection rejects a visible run or wrong variant', () {
    const source = '***\n';
    final authority = _authority(source);

    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: _asterisk,
        openingIndent: 0,
        markerStart: 0,
        markerEnd: 3,
        lineEndingStart: 3,
        lineEndingEnd: 4,
        markerCount: 3,
        projectionRuns: 1,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => _decode(
        authority,
        sourceStart: 0,
        sourceEnd: source.length,
        marker: _asterisk,
        openingIndent: 0,
        markerStart: 0,
        markerEnd: 3,
        lineEndingStart: 3,
        lineEndingEnd: 4,
        markerCount: 3,
        projectionVariant: 2,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('wire variant 6 is distinct from unsupported opener detail 6', () {
    const source = '***\n';
    final authority = _authority(source);
    final encoded = _unsupportedDetail6Viewport(sourceEnd: source.length);
    final result = _decodeEncoded(
      authority,
      sourceStart: 0,
      sourceEnd: source.length,
      encoded: encoded,
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.unknown);
    expect(
      result.structure.unknownReason,
      FlarkV3DocumentUnknownReason.unsupportedSource,
    );
    expect(result.structure.thematicBreak, isNull);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.unknown);
    expect(result.projection.runCount, 1);
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
  required int marker,
  required int openingIndent,
  bool hasBofBom = false,
  required int markerStart,
  required int markerEnd,
  required int lineEndingStart,
  required int lineEndingEnd,
  required int markerCount,
  int? metadataOverride,
  int projectionVariant = 6,
  int projectionRuns = 0,
}) {
  final encoded = _thematicBreakViewport(
    sourceStart: sourceStart,
    sourceEnd: sourceEnd,
    marker: marker,
    openingIndent: openingIndent,
    hasBofBom: hasBofBom,
    markerStart: markerStart,
    markerEnd: markerEnd,
    lineEndingStart: lineEndingStart,
    lineEndingEnd: lineEndingEnd,
    markerCount: markerCount,
    metadataOverride: metadataOverride,
    projectionVariant: projectionVariant,
    projectionRuns: projectionRuns,
  );
  return _decodeEncoded(
    authority,
    sourceStart: sourceStart,
    sourceEnd: sourceEnd,
    encoded: encoded,
  );
}

FlarkV3DocumentStructuralQuery _decodeEncoded(
  ({FlarkV3SourceDocument document, FlarkV3SourceVersion version}) authority, {
  required int sourceStart,
  required int sourceEnd,
  required Uint8List encoded,
}) {
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

Uint8List _thematicBreakViewport({
  required int sourceStart,
  required int sourceEnd,
  required int marker,
  required int openingIndent,
  required bool hasBofBom,
  required int markerStart,
  required int markerEnd,
  required int lineEndingStart,
  required int lineEndingEnd,
  required int markerCount,
  required int? metadataOverride,
  required int projectionVariant,
  required int projectionRuns,
}) {
  final bytes = _emptyViewport();
  final data = ByteData.sublistView(bytes);
  const green = 20;
  data
    ..setUint8(green + 12, 6)
    ..setUint64(green + 16, sourceStart, Endian.little)
    ..setUint64(green + 24, sourceEnd, Endian.little)
    ..setUint64(green + 32, sourceStart, Endian.little)
    ..setUint64(green + 40, sourceStart, Endian.little)
    ..setUint64(
      green + 48,
      metadataOverride ??
          marker | (openingIndent << 8) | (hasBofBom ? 1 << 10 : 0),
      Endian.little,
    )
    ..setUint32(green + 56, markerStart, Endian.little)
    ..setUint32(green + 60, markerEnd, Endian.little)
    ..setUint32(green + 64, lineEndingStart, Endian.little)
    ..setUint32(green + 68, lineEndingEnd, Endian.little)
    ..setUint64(green + 72, markerCount, Endian.little);

  const projection = green + 80;
  data
    ..setUint8(projection + 12, projectionVariant)
    ..setUint64(projection + 16, sourceStart, Endian.little)
    ..setUint64(projection + 24, sourceEnd, Endian.little)
    ..setUint64(projection + 32, sourceStart, Endian.little)
    ..setUint64(projection + 40, sourceStart, Endian.little)
    ..setUint64(projection + 48, projectionRuns, Endian.little);
  return bytes;
}

Uint8List _unsupportedDetail6Viewport({required int sourceEnd}) {
  final bytes = _emptyViewport();
  final data = ByteData.sublistView(bytes);
  const green = 20;
  data
    ..setUint8(green + 12, 2)
    ..setUint64(green + 16, 0, Endian.little)
    ..setUint64(green + 24, sourceEnd, Endian.little)
    ..setUint64(green + 32, 0, Endian.little)
    ..setUint64(green + 40, sourceEnd, Endian.little)
    ..setUint64(green + 48, 0, Endian.little)
    ..setUint32(green + 56, 2, Endian.little)
    ..setUint32(green + 60, 6, Endian.little)
    ..setUint64(green + 64, 0, Endian.little)
    ..setUint64(green + 72, 0, Endian.little);

  const projection = green + 80;
  data
    ..setUint8(projection + 12, 2)
    ..setUint64(projection + 16, 0, Endian.little)
    ..setUint64(projection + 24, sourceEnd, Endian.little)
    ..setUint64(projection + 32, 0, Endian.little)
    ..setUint64(projection + 40, sourceEnd, Endian.little)
    ..setUint64(projection + 48, 1, Endian.little);
  return bytes;
}

Uint8List _emptyViewport() {
  final bytes = Uint8List(20 + 80 + 56);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 1, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little);

  const green = 20;
  bytes.setRange(green, green + 8, ascii.encode('FLKGR001'));
  data.setUint32(green + 8, 1, Endian.little);

  const projection = green + 80;
  bytes.setRange(projection, projection + 8, ascii.encode('FLKPR001'));
  data.setUint32(projection + 8, 1, Endian.little);
  return bytes;
}

int _byteOffsetOf(String source, String needle) {
  final utf16Index = source.indexOf(needle);
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

const int _asterisk = 0x2a;
const int _hyphen = 0x2d;
const int _underscore = 0x5f;

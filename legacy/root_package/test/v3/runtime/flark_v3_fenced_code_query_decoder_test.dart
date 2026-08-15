import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('fenced-code viewport exposes parser-certified source geometry', () {
    const source = 'p\n\n```dart\né\n```\n';
    final document = FlarkV3SourceDocument.fromString(source);
    final version = FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
      document: document,
    );

    final result = FlarkV3DocumentQueryDecoder.decode(
      sourceDocument: document,
      expectedSource: version,
      expectedProfilePartition: 1,
      viewport: _viewport(version),
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.fencedCode);
    expect(result.structure.referenceDefinitionCount, 0);
    expect(result.inlineFacts, isNull);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.fencedCode);
    _expectSpan(result.structure.source, 3, 18, 3, 17);
    _expectSpan(result.projection.projectedSource, 11, 14, 11, 13);

    final fence = result.structure.fencedCode!;
    expect(fence.marker, FlarkV3CodeFenceMarker.backtick);
    expect(fence.openingIndent, 0);
    expect(fence.closed, isTrue);
    _expectSpan(fence.openingMarker, 3, 6, 3, 6);
    _expectSpan(fence.rawInfoSource, 6, 10, 6, 10);
    _expectSpan(fence.bodySource, 11, 14, 11, 13);
    _expectSpan(fence.closingMarker!, 14, 17, 13, 16);
  });

  test('fenced-code decoder fails closed on contradictory metadata', () {
    const source = 'p\n\n```dart\né\n```\n';
    final document = FlarkV3SourceDocument.fromString(source);
    final version = FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
      document: document,
    );
    final encoded = Uint8List.fromList(_viewport(version).encoded);
    ByteData.sublistView(encoded).setUint8(20 + 48, 0x78);

    expect(
      () => FlarkV3DocumentQueryDecoder.decode(
        sourceDocument: document,
        expectedSource: version,
        expectedProfilePartition: 1,
        viewport: FlarkV3HostStructuralViewport.owned(
          sourceVersion: version,
          range: _fenceRange,
          encoded: encoded,
          receipt: _receipt(encoded.length),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });
}

FlarkV3HostStructuralViewport _viewport(FlarkV3SourceVersion version) {
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
    ..setUint8(green + 12, 3)
    ..setUint64(green + 16, 3, Endian.little)
    ..setUint64(green + 24, 18, Endian.little)
    ..setUint64(green + 32, 11, Endian.little)
    ..setUint64(green + 40, 14, Endian.little)
    ..setUint64(green + 48, 0x10060, Endian.little)
    ..setUint32(green + 56, 3, Endian.little)
    ..setUint32(green + 60, 6, Endian.little)
    ..setUint32(green + 64, 6, Endian.little)
    ..setUint32(green + 68, 10, Endian.little)
    ..setUint32(green + 72, 14, Endian.little)
    ..setUint32(green + 76, 17, Endian.little);

  const projection = green + 80;
  bytes.setRange(projection, projection + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(projection + 8, 1, Endian.little)
    ..setUint8(projection + 12, 3)
    ..setUint64(projection + 16, 3, Endian.little)
    ..setUint64(projection + 24, 18, Endian.little)
    ..setUint64(projection + 32, 11, Endian.little)
    ..setUint64(projection + 40, 14, Endian.little)
    ..setUint64(projection + 48, 1, Endian.little);

  return FlarkV3HostStructuralViewport.owned(
    sourceVersion: version,
    range: _fenceRange,
    encoded: bytes,
    receipt: _receipt(bytes.length),
  );
}

final _fenceRange = FlarkV3MetricRange(
  start: FlarkV3SourceMetric(bytes: 3, utf16: 3),
  end: FlarkV3SourceMetric(bytes: 18, utf16: 17),
);

FlarkV3HostViewportReceipt _receipt(int encodedBytes) =>
    FlarkV3HostViewportReceipt(
      encodedBytes: encodedBytes,
      leafCount: 1,
      openDepth: 1,
      treeNodesVisited: 1,
      summaryNodesSkipped: 0,
    );

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

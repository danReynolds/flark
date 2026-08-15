import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('decodes one consecutive Unicode-aware structural block page', () {
    final fixture = _RangeFixture();

    final decoded = FlarkV3DocumentQueryDecoder.decodeBlockRange(
      sourceDocument: fixture.document,
      expectedSource: fixture.sourceVersion,
      range: fixture.range(),
    );

    expect(decoded.blocks, hasLength(2));
    expect(decoded.blocks.map((block) => block.ordinal), [7, 8]);
    expect(
      decoded.blocks.map((block) => block.structure.kind),
      everyElement(FlarkV3DocumentStructureKind.paragraph),
    );
    _expectSpan(decoded.blocks[0].structure.source, 0, 3, 0, 2);
    _expectSpan(decoded.blocks[1].structure.source, 3, 5, 2, 3);
    _expectSpan(decoded.coveredSource, 0, 5, 0, 3);
  });

  test('rejects nonconsecutive ordinals and source records', () {
    final fixture = _RangeFixture();

    expect(
      () => FlarkV3DocumentQueryDecoder.decodeBlockRange(
        sourceDocument: fixture.document,
        expectedSource: fixture.sourceVersion,
        range: fixture.range(
          mutate: (data) => data.setUint64(192, 9, Endian.little),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => FlarkV3DocumentQueryDecoder.decodeBlockRange(
        sourceDocument: fixture.document,
        expectedSource: fixture.sourceVersion,
        range: fixture.range(
          mutate: (data) => data.setUint32(204, 1, Endian.little),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => FlarkV3DocumentQueryDecoder.decodeBlockRange(
        sourceDocument: fixture.document,
        expectedSource: fixture.sourceVersion,
        range: fixture.range(
          mutate: (data) => data.setUint32(200, 6, Endian.little),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });

  test('rejects envelope and nested Green contradictions', () {
    final fixture = _RangeFixture();

    expect(
      () => FlarkV3DocumentQueryDecoder.decodeBlockRange(
        sourceDocument: fixture.document,
        expectedSource: fixture.sourceVersion,
        range: fixture.range(
          mutate: (data) => data.setUint32(16, 159, Endian.little),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
    expect(
      () => FlarkV3DocumentQueryDecoder.decodeBlockRange(
        sourceDocument: fixture.document,
        expectedSource: fixture.sourceVersion,
        range: fixture.range(
          mutate: (data) => data.setUint64(56, 2, Endian.little),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
    );
  });
}

final class _RangeFixture {
  _RangeFixture()
    : document = FlarkV3SourceDocument.fromString('é\nβ'),
      sourceVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
        document: FlarkV3SourceDocument.fromString('é\nβ'),
      );

  final FlarkV3SourceDocument document;
  final FlarkV3SourceVersion sourceVersion;

  FlarkV3HostStructuralBlockRange range({
    void Function(ByteData data)? mutate,
  }) {
    final encoded = Uint8List(32 + 2 * 160);
    final data = ByteData.sublistView(encoded);
    encoded.setRange(0, 8, ascii.encode('FLKVR001'));
    data
      ..setUint32(8, 1, Endian.little)
      ..setUint32(12, 32, Endian.little)
      ..setUint32(16, 160, Endian.little)
      ..setUint32(20, 2, Endian.little)
      ..setUint32(24, 1, Endian.little)
      ..setUint32(28, 0, Endian.little);
    _record(
      encoded,
      offset: 32,
      ordinal: 7,
      startUtf8: 0,
      endUtf8: 3,
      startUtf16: 0,
      endUtf16: 2,
    );
    _record(
      encoded,
      offset: 192,
      ordinal: 8,
      startUtf8: 3,
      endUtf8: 5,
      startUtf16: 2,
      endUtf16: 3,
    );
    mutate?.call(data);
    final requested = FlarkV3MetricRange(
      start: FlarkV3SourceMetric.zero,
      end: sourceVersion.metric,
    );
    return FlarkV3HostStructuralBlockRange.owned(
      sourceVersion: sourceVersion,
      requestedRange: requested,
      coveredRange: requested,
      encoded: encoded,
      receipt: FlarkV3HostBlockRangeReceipt(
        encodedBytes: encoded.length,
        blockCount: 2,
        storagePagesVisited: 1,
        openDepth: 1,
        treeNodesVisited: 1,
        packedEntriesInspected: 2,
        summaryNodesSkipped: 0,
        complete: true,
      ),
      continuation: null,
    );
  }
}

void _record(
  Uint8List bytes, {
  required int offset,
  required int ordinal,
  required int startUtf8,
  required int endUtf8,
  required int startUtf16,
  required int endUtf16,
}) {
  final data = ByteData.sublistView(bytes);
  data
    ..setUint64(offset, ordinal, Endian.little)
    ..setUint32(offset + 8, startUtf8, Endian.little)
    ..setUint32(offset + 12, startUtf16, Endian.little)
    ..setUint32(offset + 16, endUtf8, Endian.little)
    ..setUint32(offset + 20, endUtf16, Endian.little);
  _green(bytes, offset + 24, startUtf8: startUtf8, endUtf8: endUtf8);
  _projection(bytes, offset + 104, startUtf8: startUtf8, endUtf8: endUtf8);
}

void _green(
  Uint8List bytes,
  int offset, {
  required int startUtf8,
  required int endUtf8,
}) {
  final data = ByteData.sublistView(bytes);
  bytes.setRange(offset, offset + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(offset + 8, 1, Endian.little)
    ..setUint8(offset + 12, 1)
    ..setUint64(offset + 16, startUtf8, Endian.little)
    ..setUint64(offset + 24, endUtf8, Endian.little)
    ..setUint64(offset + 32, startUtf8, Endian.little)
    ..setUint64(offset + 40, endUtf8, Endian.little)
    ..setUint64(offset + 48, 0, Endian.little)
    ..setUint32(offset + 56, 0, Endian.little)
    ..setUint32(offset + 60, 0, Endian.little)
    ..setUint64(offset + 64, 0, Endian.little)
    ..setUint64(offset + 72, 0, Endian.little);
}

void _projection(
  Uint8List bytes,
  int offset, {
  required int startUtf8,
  required int endUtf8,
}) {
  final data = ByteData.sublistView(bytes);
  bytes.setRange(offset, offset + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(offset + 8, 1, Endian.little)
    ..setUint8(offset + 12, 1)
    ..setUint64(offset + 16, startUtf8, Endian.little)
    ..setUint64(offset + 24, endUtf8, Endian.little)
    ..setUint64(offset + 32, startUtf8, Endian.little)
    ..setUint64(offset + 40, endUtf8, Endian.little)
    ..setUint64(offset + 48, 1, Endian.little);
}

void _expectSpan(
  FlarkV3SourceSpan span,
  int startUtf8,
  int endUtf8,
  int startUtf16,
  int endUtf16,
) {
  expect(
    (span.startUtf8, span.endUtf8, span.startUtf16, span.endUtf16),
    (startUtf8, endUtf8, startUtf16, endUtf16),
  );
}

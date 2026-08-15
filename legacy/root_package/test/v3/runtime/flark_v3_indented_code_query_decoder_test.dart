import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('decodes a non-contiguous indented-code projection summary', () {
    const source = '    alpha\r\n      β\n\tgamma';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;
    final result = _decode(
      authority,
      sourceEnd: sourceBytes,
      lineCount: 3,
      projectedUtf8Length: 17,
      projectedUtf16Length: 16,
      terminalLineEndingBytes: 0,
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.indentedCode);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.indentedCode);
    expect(result.structure.inlineContentSource, isNull);
    expect(result.structure.canCarryInlineFacts, isFalse);
    expect(result.inlineFacts, isNull);
    expect(result.projection.runCount, 3);
    _expectSpan(authority.document, result.structure.source, 0, sourceBytes);
    _expectSpan(authority.document, result.structure.visibleSource, 0, 0);
    _expectSpan(authority.document, result.projection.projectedSource, 0, 0);

    final facts = result.structure.indentedCode!;
    expect(facts.deindentColumns, 4);
    expect(facts.hasBofBom, isFalse);
    expect(facts.lineCount, 3);
    expect(facts.projectedUtf8Length, 17);
    expect(facts.projectedUtf16Length, 16);
    expect(facts.terminalLineEndingBytes, 0);
  });

  test('accepts a BOF BOM and exact terminal CRLF width', () {
    const source = '\uFEFF    code\r\n';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;
    final result = _decode(
      authority,
      sourceEnd: sourceBytes,
      hasBofBom: true,
      lineCount: 1,
      projectedUtf8Length: 6,
      projectedUtf16Length: 6,
      terminalLineEndingBytes: 2,
    );

    final facts = result.structure.indentedCode!;
    expect(facts.hasBofBom, isTrue);
    expect(facts.terminalLineEndingBytes, 2);
  });

  test('schema 3 joins the exact per-line projection payload', () {
    const source = '    alpha\r\n      β\n\tgamma';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;
    final encoded = _viewportWithIndentedProjection(
      sourceEnd: sourceBytes,
      metadata: 4,
      lineCount: 3,
      projectedUtf8Length: 17,
      projectedUtf16Length: 16,
      terminalLineEndingBytes: 0,
      records: const <({int start, int physical, int hidden, int content})>[
        (start: 0, physical: 11, hidden: 4, content: 5),
        (start: 11, physical: 9, hidden: 4, content: 4),
        (start: 20, physical: 6, hidden: 1, content: 5),
      ],
    );

    final result = _decodeEncoded(authority, sourceBytes, encoded);
    final payload = result.indentedCodeProjection;

    expect(result.inlineFacts, isNull);
    expect(payload, isNotNull);
    expect(payload!.sourceVersion, authority.version);
    expect(payload.records, hasLength(3));
    expect(payload.records[0].hiddenPrefixLengthUtf8, 4);
    expect(payload.records[1].contentLengthUtf8, 4);
    expect(payload.records[2].hiddenPrefixLengthUtf8, 1);
    expect(payload.toSourceProjection().displayText, 'alpha\r\n  β\ngamma');
  });

  test('rejects corrupt recipe, aggregate, and projection facts', () {
    const source = '    code\n';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;

    for (final corrupt in <Uint8List Function()>[
      () => _viewport(
        sourceEnd: sourceBytes,
        metadata: 3,
        lineCount: 1,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 1,
        projectionRuns: 1,
      ),
      () => _viewport(
        sourceEnd: sourceBytes,
        metadata: 4,
        lineCount: 0,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 1,
        projectionRuns: 1,
      ),
      () => _viewport(
        sourceEnd: sourceBytes,
        metadata: 4,
        lineCount: 1,
        projectedUtf8Length: sourceBytes + 1,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 1,
        projectionRuns: 1,
      ),
      () => _viewport(
        sourceEnd: sourceBytes,
        metadata: 4,
        lineCount: 1,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 3,
        projectionRuns: 1,
      ),
      () => _viewport(
        sourceEnd: sourceBytes,
        metadata: 4,
        lineCount: 1,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 1,
        projectionRuns: 2,
      ),
      () => _viewport(
        sourceEnd: sourceBytes,
        metadata: 4,
        lineCount: 1,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 1,
        projectionRuns: 1,
        reserved: 1,
      ),
    ]) {
      expect(
        () => _decodeEncoded(authority, sourceBytes, corrupt()),
        throwsA(isA<FlarkV3DocumentQueryException>()),
      );
    }
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
  required int sourceEnd,
  bool hasBofBom = false,
  required int lineCount,
  required int projectedUtf8Length,
  required int projectedUtf16Length,
  required int terminalLineEndingBytes,
}) => _decodeEncoded(
  authority,
  sourceEnd,
  _viewport(
    sourceEnd: sourceEnd,
    metadata: 4 | (hasBofBom ? 1 << 8 : 0),
    lineCount: lineCount,
    projectedUtf8Length: projectedUtf8Length,
    projectedUtf16Length: projectedUtf16Length,
    terminalLineEndingBytes: terminalLineEndingBytes,
    projectionRuns: lineCount,
  ),
);

FlarkV3DocumentStructuralQuery _decodeEncoded(
  ({FlarkV3SourceDocument document, FlarkV3SourceVersion version}) authority,
  int sourceEnd,
  Uint8List encoded,
) => FlarkV3DocumentQueryDecoder.decode(
  sourceDocument: authority.document,
  expectedSource: authority.version,
  expectedProfilePartition: 1,
  viewport: FlarkV3HostStructuralViewport.owned(
    sourceVersion: authority.version,
    range: FlarkV3MetricRange(
      start: FlarkV3SourceMetric(bytes: 0, utf16: 0),
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

Uint8List _viewport({
  required int sourceEnd,
  required int metadata,
  required int lineCount,
  required int projectedUtf8Length,
  required int projectedUtf16Length,
  required int terminalLineEndingBytes,
  required int projectionRuns,
  int reserved = 0,
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
    ..setUint8(green + 12, 7)
    ..setUint64(green + 16, 0, Endian.little)
    ..setUint64(green + 24, sourceEnd, Endian.little)
    ..setUint64(green + 32, 0, Endian.little)
    ..setUint64(green + 40, 0, Endian.little)
    ..setUint64(green + 48, metadata, Endian.little)
    ..setUint32(green + 56, lineCount, Endian.little)
    ..setUint32(green + 60, projectedUtf8Length, Endian.little)
    ..setUint32(green + 64, projectedUtf16Length, Endian.little)
    ..setUint32(green + 68, terminalLineEndingBytes, Endian.little)
    ..setUint64(green + 72, reserved, Endian.little);

  const projection = green + 80;
  bytes.setRange(projection, projection + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(projection + 8, 1, Endian.little)
    ..setUint8(projection + 12, 7)
    ..setUint64(projection + 16, 0, Endian.little)
    ..setUint64(projection + 24, sourceEnd, Endian.little)
    ..setUint64(projection + 32, 0, Endian.little)
    ..setUint64(projection + 40, 0, Endian.little)
    ..setUint64(projection + 48, projectionRuns, Endian.little);
  return bytes;
}

Uint8List _viewportWithIndentedProjection({
  required int sourceEnd,
  required int metadata,
  required int lineCount,
  required int projectedUtf8Length,
  required int projectedUtf16Length,
  required int terminalLineEndingBytes,
  required List<({int start, int physical, int hidden, int content})> records,
}) {
  final structural = _viewport(
    sourceEnd: sourceEnd,
    metadata: metadata,
    lineCount: lineCount,
    projectedUtf8Length: projectedUtf8Length,
    projectedUtf16Length: projectedUtf16Length,
    terminalLineEndingBytes: terminalLineEndingBytes,
    projectionRuns: lineCount,
  );
  final payloadBytes = records.length * 20;
  final bytes = Uint8List(28 + 80 + 56 + payloadBytes);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 3, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little)
    ..setUint8(20, 2)
    ..setUint32(24, payloadBytes, Endian.little);
  bytes.setRange(28, 28 + 80 + 56, structural.sublist(20));
  for (var index = 0; index < records.length; index += 1) {
    final record = records[index];
    final offset = 28 + 80 + 56 + index * 20;
    data
      ..setUint32(offset, record.start, Endian.little)
      ..setUint32(offset + 4, record.physical, Endian.little)
      ..setUint32(offset + 8, record.hidden, Endian.little)
      ..setUint32(offset + 12, record.content, Endian.little)
      ..setUint32(offset + 16, 0, Endian.little);
  }
  return bytes;
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

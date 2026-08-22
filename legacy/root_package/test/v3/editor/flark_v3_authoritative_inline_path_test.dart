import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/editor/flark_v3_inline_island_presentation.dart';
import 'package:flark/src/v3/editor/flark_v3_inline_projection.dart';
import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('viewport v8 drives exact bold, emphasis, and code or source paint', () {
    const source = '*a* **b** `c`';
    final document = FlarkV3SourceDocument.fromString(source);
    final sourceVersion = FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
      document: document,
    );
    final exact = _decode(
      document,
      sourceVersion,
      records: [
        _fact(kind: 1, start: 0, length: 3, contentStart: 1),
        _fact(kind: 2, start: 4, length: 5, contentStart: 6),
        _fact(kind: 3, start: 10, length: 3, contentStart: 11),
      ],
    );
    final presentation = FlarkV3InlineIslandPresentation.resolve(
      sourceDocument: document,
      expectedSource: sourceVersion,
      structuralQuery: exact,
      activeIsland: exact.projection.projectedSource,
    );

    expect(presentation, isA<FlarkV3AuthoritativeInlineIslandPresentation>());
    final projection =
        (presentation as FlarkV3AuthoritativeInlineIslandPresentation)
            .projection;
    expect(projection.displayText, 'a b c');
    expect(
      projection.runs.singleWhere((run) => run.text == 'a').semanticStyles,
      [FlarkV3InlineFactKind.emphasis],
    );
    expect(
      projection.runs.singleWhere((run) => run.text == 'b').semanticStyles,
      [FlarkV3InlineFactKind.strong],
    );
    expect(
      projection.runs.singleWhere((run) => run.text == 'c').semanticStyles,
      [FlarkV3InlineFactKind.code],
    );
    expect(projection.sourceToDisplayOffset(0), 0);
    expect(projection.sourceToDisplayOffset(1), 0);
    expect(projection.sourceToDisplayOffset(2), 1);
    expect(projection.sourceToDisplayOffset(3), 1);
    expect(
      projection.displayToSourceOffset(
        1,
        affinity: FlarkV3InlineProjectionAffinity.upstream,
      ),
      2,
    );
    expect(
      projection.displayToSourceOffset(
        1,
        affinity: FlarkV3InlineProjectionAffinity.downstream,
      ),
      3,
    );
    expect(
      [
        for (var offset = 0; offset <= source.length; offset += 1)
          projection.sourceToDisplayOffset(offset),
      ],
      orderedEquals(
        [
          for (var offset = 0; offset <= source.length; offset += 1)
            projection.sourceToDisplayOffset(offset),
        ]..sort(),
      ),
    );
    for (final affinity in FlarkV3InlineProjectionAffinity.values) {
      final mapped = [
        for (
          var offset = 0;
          offset <= projection.displayLengthUtf16;
          offset += 1
        )
          projection.displayToSourceOffset(offset, affinity: affinity),
      ];
      expect(mapped, orderedEquals([...mapped]..sort()));
    }

    final absent = _decode(document, sourceVersion, includeInline: false);
    final absentPresentation = FlarkV3InlineIslandPresentation.resolve(
      sourceDocument: document,
      expectedSource: sourceVersion,
      structuralQuery: absent,
      activeIsland: absent.projection.projectedSource,
    );
    expect(
      absentPresentation,
      isA<FlarkV3SourcePaintInlineIslandPresentation>(),
    );
    expect(
      (absentPresentation as FlarkV3SourcePaintInlineIslandPresentation).reason,
      FlarkV3InlineIslandSourcePaintReason.inlineFactsAbsent,
    );
    expect(absentPresentation.source.startUtf16, 0);
    expect(absentPresentation.source.endUtf16, source.length);

    final unsupported = _decode(document, sourceVersion, disposition: 2);
    expect(
      FlarkV3InlineIslandPresentation.resolve(
        sourceDocument: document,
        expectedSource: sourceVersion,
        structuralQuery: unsupported,
        activeIsland: unsupported.projection.projectedSource,
      ),
      isA<FlarkV3SourcePaintInlineIslandPresentation>().having(
        (value) => value.reason,
        'reason',
        FlarkV3InlineIslandSourcePaintReason.inlineFactsUnsupported,
      ),
    );
  });
}

FlarkV3DocumentStructuralQuery _decode(
  FlarkV3SourceDocument document,
  FlarkV3SourceVersion sourceVersion, {
  bool includeInline = true,
  int disposition = 1,
  List<Uint8List> records = const [],
}) {
  final encoded = _viewport(
    document,
    includeInline: includeInline,
    disposition: disposition,
    records: records,
  );
  return FlarkV3DocumentQueryDecoder.decode(
    sourceDocument: document,
    expectedSource: sourceVersion,
    expectedProfilePartition: 1,
    viewport: FlarkV3HostStructuralViewport.owned(
      sourceVersion: sourceVersion,
      range: FlarkV3MetricRange(
        start: FlarkV3SourceMetric.zero,
        end: sourceVersion.metric,
      ),
      encoded: encoded,
      receipt: FlarkV3HostViewportReceipt(
        encodedBytes: encoded.length,
        leafCount: includeInline ? 3 : 2,
        openDepth: 1,
        treeNodesVisited: includeInline ? 3 : 2,
        summaryNodesSkipped: 0,
      ),
    ),
  );
}

Uint8List _viewport(
  FlarkV3SourceDocument document, {
  required bool includeInline,
  required int disposition,
  required List<Uint8List> records,
}) {
  final inlineLength = includeInline ? 48 + records.length * 20 : 0;
  final headerBytes = includeInline ? 24 : 20;
  final bytes = Uint8List(headerBytes + 80 + 56 + inlineLength);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, includeInline ? 8 : 1, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little);
  if (includeInline) {
    data.setUint32(20, inlineLength, Endian.little);
  }
  _green(bytes, headerBytes, document.utf8Length);
  _projection(bytes, headerBytes + 80, document.utf8Length);
  if (!includeInline) return bytes;

  final start = headerBytes + 80 + 56;
  bytes.setRange(start, start + 8, ascii.encode('FLKIN002'));
  data
    ..setUint32(start + 8, 2, Endian.little)
    ..setUint8(start + 12, disposition)
    ..setUint32(start + 16, 1, Endian.little)
    ..setUint32(start + 20, records.length, Endian.little)
    ..setUint64(start + 24, 0, Endian.little)
    ..setUint64(start + 32, document.utf8Length, Endian.little)
    ..setUint32(start + 40, 20, Endian.little)
    ..setUint32(start + 44, 0, Endian.little);
  var offset = start + 48;
  for (final record in records) {
    bytes.setRange(offset, offset + record.length, record);
    offset += record.length;
  }
  return bytes;
}

void _green(Uint8List bytes, int offset, int sourceBytes) {
  final data = ByteData.sublistView(bytes);
  bytes.setRange(offset, offset + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(offset + 8, 1, Endian.little)
    ..setUint8(offset + 12, 1)
    ..setUint64(offset + 16, 0, Endian.little)
    ..setUint64(offset + 24, sourceBytes, Endian.little)
    ..setUint64(offset + 32, 0, Endian.little)
    ..setUint64(offset + 40, sourceBytes, Endian.little)
    ..setUint64(offset + 48, 0, Endian.little)
    ..setUint32(offset + 56, 0, Endian.little)
    ..setUint32(offset + 60, 0, Endian.little)
    ..setUint64(offset + 64, 0, Endian.little)
    ..setUint64(offset + 72, 0, Endian.little);
}

void _projection(Uint8List bytes, int offset, int sourceBytes) {
  final data = ByteData.sublistView(bytes);
  bytes.setRange(offset, offset + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(offset + 8, 1, Endian.little)
    ..setUint8(offset + 12, 1)
    ..setUint64(offset + 16, 0, Endian.little)
    ..setUint64(offset + 24, sourceBytes, Endian.little)
    ..setUint64(offset + 32, 0, Endian.little)
    ..setUint64(offset + 40, sourceBytes, Endian.little)
    ..setUint64(offset + 48, 1, Endian.little);
}

Uint8List _fact({
  required int kind,
  required int start,
  required int length,
  required int contentStart,
}) {
  final bytes = Uint8List(20);
  ByteData.sublistView(bytes)
    ..setUint8(0, kind)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, 1, Endian.little);
  return bytes;
}

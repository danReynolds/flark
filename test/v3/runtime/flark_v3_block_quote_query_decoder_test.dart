import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_block_quote_projection.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('schema 4 decodes an exact noncontiguous quote Paragraph path', () {
    const source = '   > α\r\n> beta\nlazy';
    const projected = 'α\r\nbeta\nlazy';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;
    final encoded = _blockQuoteViewport(
      sourceBytes: sourceBytes,
      sourceUtf16: source.length,
      projectedUtf8: utf8.encode(projected).length,
      projectedUtf16: projected.length,
      records: const <_QuoteLine>[
        _QuoteLine(start: 0, physical: 9, hidden: 5, content: 2, flags: 1),
        _QuoteLine(start: 9, physical: 7, hidden: 2, content: 4, flags: 1),
        _QuoteLine(start: 16, physical: 4, hidden: 0, content: 4, flags: 2),
      ],
    );

    final result = _decode(authority, encoded);
    expect(result.structure.kind, FlarkV3DocumentStructureKind.blockQuote);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.blockQuote);
    expect(result.structure.visibleSource.startUtf8, 0);
    expect(result.structure.visibleSource.endUtf8, 0);
    expect(result.projection.projectedSource.startUtf8, 0);
    expect(result.projection.projectedSource.endUtf8, 0);
    expect(result.projection.runCount, 3);

    final facts = result.structure.blockQuote!;
    expect(facts.lineCount, 3);
    expect(facts.childFirstLine, 0);
    expect(facts.childLineCount, 3);
    expect(facts.projectedUtf8Length, 13);
    expect(facts.projectedUtf16Length, 12);

    final path = result.pointPath!;
    expect(path.nodes, hasLength(2));
    expect(path.root, same(path.nodes.first));
    expect(
      path.blockQuoteAncestor.kind,
      FlarkV3DocumentPointPathNodeKind.blockQuote,
    );
    expect(path.blockQuoteAncestor.depth, 0);
    expect(path.blockQuoteAncestor.parentIndex, isNull);
    expect(path.blockQuoteAncestor.isNoncontiguous, isFalse);
    expect(path.blockQuoteAncestor.isSelected, isFalse);
    expect(path.selectedLeaf.kind, FlarkV3DocumentPointPathNodeKind.paragraph);
    expect(path.selectedLeaf.depth, 1);
    expect(path.selectedLeaf.parentIndex, 0);
    expect(path.selectedLeaf.isNoncontiguous, isTrue);
    expect(path.selectedLeaf.isSelected, isTrue);
    expect(path.selectedLeaf.firstRun, 0);
    expect(path.selectedLeaf.runCount, 3);
    _expectSpan(path.selectedLeaf.source, 0, sourceBytes, 0, source.length);

    final payload = result.blockQuoteProjection!;
    expect(payload.sourceVersion, authority.version);
    expect(payload.pointPath, same(path));
    expect(payload.records, hasLength(3));
    expect(payload.records[0].isMarked, isTrue);
    expect(payload.records[1].isMarked, isTrue);
    expect(payload.records[2].isLazyContinuation, isTrue);
    expect(
      payload.records[2].kind,
      FlarkV3BlockQuoteLineProjectionKind.lazyContinuation,
    );
    expect(_read(authority.document, payload.records[0].hiddenPrefix), '   > ');
    expect(_read(authority.document, payload.records[0].content), 'α');
    expect(_read(authority.document, payload.records[0].lineEnding), '\r\n');
    expect(_read(authority.document, payload.records[2].hiddenPrefix), isEmpty);
    expect(_read(authority.document, payload.records[2].content), 'lazy');
    final projection = payload.toSourceProjection();
    expect(projection.isCertified, isTrue);
    expect(projection.sourceText, source);
    expect(projection.displayText, projected);
  });

  test('decodes a path-independent recursive quote certificate', () {
    const source = '> a\nlazy';
    final authority = _authority(source);
    final certificate = FlarkV3BlockQuoteProjectionDecoder.decodeCertificate(
      sourceDocument: authority.document,
      expectedSource: authority.version,
      source: FlarkV3SourceSpan(
        startUtf8: 0,
        endUtf8: authority.document.utf8Length,
        startUtf16: 0,
        endUtf16: authority.document.utf16Length,
      ),
      encodedRecords: _quoteRecords(const <_QuoteLine>[
        _QuoteLine(start: 0, physical: 4, hidden: 2, content: 1, flags: 1),
        _QuoteLine(start: 4, physical: 4, hidden: 0, content: 4, flags: 2),
      ]),
    );

    expect(certificate.sourceVersion, authority.version);
    expect(certificate.records, hasLength(2));
    expect(certificate.projectedUtf8Length, 6);
    expect(certificate.projectedUtf16Length, 6);
    expect(certificate.toSourceProjection().displayText, 'a\nlazy');
  });

  test('schema 4 rejects corrupt path, line, and aggregate geometry', () {
    const source = '   > α\r\n> beta\nlazy';
    const projected = 'α\r\nbeta\nlazy';
    final authority = _authority(source);
    final valid = _blockQuoteViewport(
      sourceBytes: utf8.encode(source).length,
      sourceUtf16: source.length,
      projectedUtf8: utf8.encode(projected).length,
      projectedUtf16: projected.length,
      records: const <_QuoteLine>[
        _QuoteLine(start: 0, physical: 9, hidden: 5, content: 2, flags: 1),
        _QuoteLine(start: 9, physical: 7, hidden: 2, content: 4, flags: 1),
        _QuoteLine(start: 16, physical: 4, hidden: 0, content: 4, flags: 2),
      ],
    );

    for (final mutate in <void Function(Uint8List, ByteData)>[
      (bytes, _) => bytes[23] = 1,
      (_, data) => data.setUint16(20, 3, Endian.little),
      (_, data) => data.setUint32(24, 40, Endian.little),
      (bytes, _) => bytes[_pathStart + 40 + 1] = 1,
      (_, data) =>
          data.setUint32(_pathStart + 20, source.length - 1, Endian.little),
      (_, data) => data.setUint32(_greenStart + 68, 12, Endian.little),
      (_, data) => data.setUint32(_payloadStart + 16, 3, Endian.little),
      (_, data) => data.setUint32(_payloadStart + 20, 10, Endian.little),
      (_, data) => data.setUint32(_payloadStart + 40 + 8, 1, Endian.little),
    ]) {
      final corrupt = Uint8List.fromList(valid);
      mutate(corrupt, ByteData.sublistView(corrupt));
      expect(
        () => _decode(authority, corrupt),
        throwsA(isA<FlarkV3DocumentQueryException>()),
      );
    }
  });

  test('schema 5 derives Unicode UTF-16 for a variable-depth list path', () {
    const source = '- α\n';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;
    final encoded = _schema5PointPathViewport(
      sourceBytes: sourceBytes,
      sourceUtf16: source.length,
      pathNodes: const <_PointPathNode>[
        _PointPathNode(
          kind: 3,
          flags: 0,
          depth: 0,
          parent: 0xffffffff,
          projectedUtf8: 3,
          projectedUtf16: 2,
        ),
        _PointPathNode(
          kind: 4,
          flags: 0,
          depth: 1,
          parent: 0,
          projectedUtf8: 3,
          projectedUtf16: 2,
        ),
        _PointPathNode(
          kind: 2,
          flags: 2,
          depth: 2,
          parent: 1,
          sourceStartUtf8: 2,
          sourceEndUtf8: 4,
          projectedUtf8: 2,
          projectedUtf16: 1,
        ),
      ],
    );

    expect(
      () => _decode(authority, encoded),
      throwsA(
        isA<FlarkV3DocumentQueryException>().having(
          (error) => error.message,
          'message',
          contains('list payload does not match its exact point path'),
        ),
      ),
      reason:
          'the schema-5 path must pass byte-boundary mapping and generic '
          'topology before rejecting the deliberately mismatched root facts',
    );
  });

  test('schema 5 admits a terminal empty selected list item', () {
    const source = '- \n';
    final authority = _authority(source);
    final encoded = _schema5PointPathViewport(
      sourceBytes: utf8.encode(source).length,
      sourceUtf16: source.length,
      pathNodes: const <_PointPathNode>[
        _PointPathNode(
          kind: 3,
          flags: 0,
          depth: 0,
          parent: 0xffffffff,
          projectedUtf8: 0,
          projectedUtf16: 0,
        ),
        _PointPathNode(
          kind: 4,
          flags: 2,
          depth: 1,
          parent: 0,
          projectedUtf8: 0,
          projectedUtf16: 0,
        ),
      ],
    );

    expect(
      () => _decode(authority, encoded),
      throwsA(
        isA<FlarkV3DocumentQueryException>().having(
          (error) => error.message,
          'message',
          contains('list payload does not match its exact point path'),
        ),
      ),
      reason:
          'an exact empty item ends at ListItem rather than inventing a '
          'Paragraph or nonempty display range',
    );
  });

  test('schema 5 rejects hostile generic point-path topology', () {
    const source = '- α\n';
    final authority = _authority(source);
    final sourceBytes = utf8.encode(source).length;

    Uint8List viewport(List<_PointPathNode> pathNodes) =>
        _schema5PointPathViewport(
          sourceBytes: sourceBytes,
          sourceUtf16: source.length,
          pathNodes: pathNodes,
        );

    final cases = <({Uint8List encoded, String message, String reason})>[
      (
        encoded: viewport(const <_PointPathNode>[]),
        message: 'viewport schema does not match its point-path payload',
        reason: 'an exact path cannot be empty',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(kind: 5, flags: 0, depth: 0, parent: 0xffffffff),
          _PointPathNode(kind: 2, flags: 2, depth: 1, parent: 0),
        ]),
        message: 'unknown structure kind',
        reason: 'an unpublished path kind must fail closed',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(kind: 3, flags: 0, depth: 0, parent: 0xffffffff),
          _PointPathNode(kind: 2, flags: 2, depth: 2, parent: 0),
        ]),
        message: 'one selected outer-to-inner ancestry',
        reason: 'depth cannot skip a level',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(kind: 3, flags: 0, depth: 0, parent: 0xffffffff),
          _PointPathNode(kind: 2, flags: 2, depth: 1, parent: 0xffffffff),
        ]),
        message: 'one selected outer-to-inner ancestry',
        reason: 'a child cannot detach from the previous path node',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(kind: 3, flags: 2, depth: 0, parent: 0xffffffff),
          _PointPathNode(kind: 2, flags: 2, depth: 1, parent: 0),
        ]),
        message: 'one selected outer-to-inner ancestry',
        reason: 'only the terminal node may be selected',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(
            kind: 3,
            flags: 0,
            depth: 0,
            parent: 0xffffffff,
            sourceStartUtf8: 1,
          ),
          _PointPathNode(kind: 2, flags: 2, depth: 1, parent: 0),
        ]),
        message: 'escapes its parser-authored parent envelope',
        reason: 'a child source envelope cannot escape its parent',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(
            kind: 3,
            flags: 0,
            depth: 0,
            parent: 0xffffffff,
            runCount: 1,
          ),
          _PointPathNode(
            kind: 2,
            flags: 2,
            depth: 1,
            parent: 0,
            firstRun: 1,
            runCount: 1,
          ),
        ]),
        message: 'escapes its parser-authored parent envelope',
        reason: 'a child projection-run slice cannot escape its parent',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(kind: 3, flags: 0, depth: 0, parent: 0xffffffff),
          _PointPathNode(
            kind: 2,
            flags: 2,
            depth: 1,
            parent: 0,
            projectedUtf8: 0,
            projectedUtf16: 1,
          ),
        ]),
        message: 'invalid source or projection geometry',
        reason: 'an empty projection must agree in UTF-8 and UTF-16',
      ),
      (
        encoded: viewport(const <_PointPathNode>[
          _PointPathNode(kind: 3, flags: 0, depth: 0, parent: 0xffffffff),
          _PointPathNode(
            kind: 2,
            flags: 2,
            depth: 1,
            parent: 0,
            sourceStartUtf8: 3,
            sourceEndUtf8: 4,
          ),
        ]),
        message: 'not aligned to exact source boundaries',
        reason: 'a byte offset inside a Unicode scalar must fail closed',
      ),
    ];

    for (final testCase in cases) {
      expect(
        () => _decode(authority, testCase.encoded),
        throwsA(
          isA<FlarkV3DocumentQueryException>().having(
            (error) => error.message,
            'message',
            contains(testCase.message),
          ),
        ),
        reason: testCase.reason,
      );
    }
  });

  test('the additive decoder still accepts viewport schema 1', () {
    const source = 'plain\n';
    final authority = _authority(source);
    final result = _decode(authority, _paragraphViewport(source.length));

    expect(result.structure.kind, FlarkV3DocumentStructureKind.paragraph);
    expect(result.pointPath, isNull);
    expect(result.blockQuoteProjection, isNull);
  });
}

typedef _Authority = ({
  FlarkV3SourceDocument document,
  FlarkV3SourceVersion version,
});

final class _QuoteLine {
  const _QuoteLine({
    required this.start,
    required this.physical,
    required this.hidden,
    required this.content,
    required this.flags,
  });

  final int start;
  final int physical;
  final int hidden;
  final int content;
  final int flags;
}

final class _PointPathNode {
  const _PointPathNode({
    required this.kind,
    required this.flags,
    required this.depth,
    required this.parent,
    this.sourceStartUtf8 = 0,
    this.sourceEndUtf8,
    this.firstRun = 0,
    this.runCount,
    this.projectedUtf8,
    this.projectedUtf16,
  });

  final int kind;
  final int flags;
  final int depth;
  final int parent;
  final int sourceStartUtf8;
  final int? sourceEndUtf8;
  final int firstRun;
  final int? runCount;
  final int? projectedUtf8;
  final int? projectedUtf16;
}

Uint8List _quoteRecords(List<_QuoteLine> records) {
  final bytes = Uint8List(
    records.length * FlarkV3BlockQuoteProjectionDecoder.recordBytes,
  );
  final data = ByteData.sublistView(bytes);
  for (var index = 0; index < records.length; index += 1) {
    final record = records[index];
    final offset = index * FlarkV3BlockQuoteProjectionDecoder.recordBytes;
    data
      ..setUint32(offset, record.start, Endian.little)
      ..setUint32(offset + 4, record.physical, Endian.little)
      ..setUint32(offset + 8, record.hidden, Endian.little)
      ..setUint32(offset + 12, record.content, Endian.little)
      ..setUint32(offset + 16, record.flags, Endian.little);
  }
  return bytes;
}

_Authority _authority(String source) {
  final document = FlarkV3SourceDocument.fromString(source);
  return (
    document: document,
    version: FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(11, 12, 13, 14),
      document: document,
    ),
  );
}

FlarkV3DocumentStructuralQuery _decode(
  _Authority authority,
  Uint8List encoded,
) => FlarkV3DocumentQueryDecoder.decode(
  sourceDocument: authority.document,
  expectedSource: authority.version,
  expectedProfilePartition: 1,
  viewport: FlarkV3HostStructuralViewport.owned(
    sourceVersion: authority.version,
    range: FlarkV3MetricRange(
      start: FlarkV3SourceMetric(bytes: 0, utf16: 0),
      end: authority.version.metric,
    ),
    encoded: encoded,
    receipt: FlarkV3HostViewportReceipt(
      encodedBytes: encoded.length,
      leafCount: 1,
      openDepth: 2,
      treeNodesVisited: 2,
      summaryNodesSkipped: 0,
    ),
  ),
);

Uint8List _blockQuoteViewport({
  required int sourceBytes,
  required int sourceUtf16,
  required int projectedUtf8,
  required int projectedUtf16,
  required List<_QuoteLine> records,
}) {
  const nodes = <_PointPathNode>[
    _PointPathNode(kind: 1, flags: 0, depth: 0, parent: 0xffffffff),
    _PointPathNode(kind: 2, flags: 3, depth: 1, parent: 0),
  ];
  final payloadBytes = records.length * 20;
  final bytes = Uint8List(_payloadStart + payloadBytes);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 4, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little)
    ..setUint16(20, 2, Endian.little)
    ..setUint8(22, 3)
    ..setUint8(23, 0)
    ..setUint32(24, 80, Endian.little)
    ..setUint32(28, payloadBytes, Endian.little);

  bytes.setRange(_greenStart, _greenStart + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(_greenStart + 8, 1, Endian.little)
    ..setUint8(_greenStart + 12, 8)
    ..setUint64(_greenStart + 16, 0, Endian.little)
    ..setUint64(_greenStart + 24, sourceBytes, Endian.little)
    ..setUint64(_greenStart + 32, 0, Endian.little)
    ..setUint64(_greenStart + 40, 0, Endian.little)
    ..setUint64(_greenStart + 48, 1, Endian.little)
    ..setUint32(_greenStart + 56, records.length, Endian.little)
    ..setUint32(_greenStart + 60, 0, Endian.little)
    ..setUint32(_greenStart + 64, records.length, Endian.little)
    ..setUint32(_greenStart + 68, projectedUtf8, Endian.little)
    ..setUint32(_greenStart + 72, projectedUtf16, Endian.little)
    ..setUint32(_greenStart + 76, 0, Endian.little);

  bytes.setRange(
    _projectionStart,
    _projectionStart + 8,
    ascii.encode('FLKPR001'),
  );
  data
    ..setUint32(_projectionStart + 8, 1, Endian.little)
    ..setUint8(_projectionStart + 12, 8)
    ..setUint64(_projectionStart + 16, 0, Endian.little)
    ..setUint64(_projectionStart + 24, sourceBytes, Endian.little)
    ..setUint64(_projectionStart + 32, 0, Endian.little)
    ..setUint64(_projectionStart + 40, 0, Endian.little)
    ..setUint64(_projectionStart + 48, records.length, Endian.little);

  for (var index = 0; index < nodes.length; index += 1) {
    _writePathNode(
      data,
      _pathStart + index * 40,
      node: nodes[index],
      sourceBytes: sourceBytes,
      sourceUtf16: sourceUtf16,
      defaultRunCount: records.length,
      defaultProjectedUtf8: projectedUtf8,
      defaultProjectedUtf16: projectedUtf16,
    );
  }

  for (var index = 0; index < records.length; index += 1) {
    final record = records[index];
    final offset = _payloadStart + index * 20;
    data
      ..setUint32(offset, record.start, Endian.little)
      ..setUint32(offset + 4, record.physical, Endian.little)
      ..setUint32(offset + 8, record.hidden, Endian.little)
      ..setUint32(offset + 12, record.content, Endian.little)
      ..setUint32(offset + 16, record.flags, Endian.little);
  }
  return bytes;
}

Uint8List _schema5PointPathViewport({
  required int sourceBytes,
  required int sourceUtf16,
  required List<_PointPathNode> pathNodes,
}) {
  final pathTableBytes = pathNodes.length * 32;
  final payloadStart = _pathStart + pathTableBytes;
  final bytes = Uint8List(payloadStart + 1);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 5, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little)
    ..setUint16(20, pathNodes.length, Endian.little)
    ..setUint8(22, 4)
    ..setUint8(23, 0)
    ..setUint32(24, pathTableBytes, Endian.little)
    ..setUint32(28, 1, Endian.little);

  bytes.setRange(_greenStart, _greenStart + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(_greenStart + 8, 1, Endian.little)
    ..setUint8(_greenStart + 12, 1)
    ..setUint64(_greenStart + 16, 0, Endian.little)
    ..setUint64(_greenStart + 24, sourceBytes, Endian.little)
    ..setUint64(_greenStart + 32, 0, Endian.little)
    ..setUint64(_greenStart + 40, sourceBytes, Endian.little);

  bytes.setRange(
    _projectionStart,
    _projectionStart + 8,
    ascii.encode('FLKPR001'),
  );
  data
    ..setUint32(_projectionStart + 8, 1, Endian.little)
    ..setUint8(_projectionStart + 12, 1)
    ..setUint64(_projectionStart + 16, 0, Endian.little)
    ..setUint64(_projectionStart + 24, sourceBytes, Endian.little)
    ..setUint64(_projectionStart + 32, 0, Endian.little)
    ..setUint64(_projectionStart + 40, sourceBytes, Endian.little)
    ..setUint64(_projectionStart + 48, 1, Endian.little);

  for (var index = 0; index < pathNodes.length; index += 1) {
    final node = pathNodes[index];
    final offset = _pathStart + index * 32;
    data
      ..setUint8(offset, node.kind)
      ..setUint8(offset + 1, node.flags)
      ..setUint16(offset + 2, node.depth, Endian.little)
      ..setUint32(offset + 4, node.parent, Endian.little)
      ..setUint32(offset + 8, node.sourceStartUtf8, Endian.little)
      ..setUint32(offset + 12, node.sourceEndUtf8 ?? sourceBytes, Endian.little)
      ..setUint32(offset + 16, node.firstRun, Endian.little)
      ..setUint32(offset + 20, node.runCount ?? 1, Endian.little)
      ..setUint32(offset + 24, node.projectedUtf8 ?? sourceBytes, Endian.little)
      ..setUint32(
        offset + 28,
        node.projectedUtf16 ?? sourceUtf16,
        Endian.little,
      );
  }
  bytes[payloadStart] = 0;
  return bytes;
}

void _writePathNode(
  ByteData data,
  int offset, {
  required _PointPathNode node,
  required int sourceBytes,
  required int sourceUtf16,
  required int defaultRunCount,
  required int defaultProjectedUtf8,
  required int defaultProjectedUtf16,
}) {
  data
    ..setUint8(offset, node.kind)
    ..setUint8(offset + 1, node.flags)
    ..setUint16(offset + 2, node.depth, Endian.little)
    ..setUint32(offset + 4, node.parent, Endian.little)
    ..setUint32(offset + 8, node.sourceStartUtf8, Endian.little)
    ..setUint32(offset + 12, sourceBytes, Endian.little)
    ..setUint32(offset + 16, 0, Endian.little)
    ..setUint32(offset + 20, sourceUtf16, Endian.little)
    ..setUint32(offset + 24, node.firstRun, Endian.little)
    ..setUint32(offset + 28, node.runCount ?? defaultRunCount, Endian.little)
    ..setUint32(
      offset + 32,
      node.projectedUtf8 ?? defaultProjectedUtf8,
      Endian.little,
    )
    ..setUint32(
      offset + 36,
      node.projectedUtf16 ?? defaultProjectedUtf16,
      Endian.little,
    );
}

Uint8List _paragraphViewport(int sourceLength) {
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
    ..setUint8(green + 12, 1)
    ..setUint64(green + 16, 0, Endian.little)
    ..setUint64(green + 24, sourceLength, Endian.little)
    ..setUint64(green + 32, 0, Endian.little)
    ..setUint64(green + 40, sourceLength, Endian.little);
  const projection = green + 80;
  bytes.setRange(projection, projection + 8, ascii.encode('FLKPR001'));
  data
    ..setUint32(projection + 8, 1, Endian.little)
    ..setUint8(projection + 12, 1)
    ..setUint64(projection + 16, 0, Endian.little)
    ..setUint64(projection + 24, sourceLength, Endian.little)
    ..setUint64(projection + 32, 0, Endian.little)
    ..setUint64(projection + 40, sourceLength, Endian.little)
    ..setUint64(projection + 48, 1, Endian.little);
  return bytes;
}

String _read(FlarkV3SourceDocument document, FlarkV3SourceSpan span) =>
    document.readRange(span.startUtf16, span.endUtf16);

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

const int _greenStart = 32;
const int _projectionStart = _greenStart + 80;
const int _pathStart = _projectionStart + 56;
const int _payloadStart = _pathStart + 80;

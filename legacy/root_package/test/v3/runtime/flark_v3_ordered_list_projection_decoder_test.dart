import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_bullet_list_projection.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_ordered_list_projection.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  const mixedSource = '007) α😀\r\n9) beta\r\n42) ';
  const mixedItems = <_Item>[
    _Item(
      start: 0,
      physical: 13,
      hidden: 5,
      continuationStart: 0,
      continuationEnd: 5,
      contentUtf8: 6,
      contentUtf16: 3,
      markerStart: 0,
      markerEnd: 4,
      markerValue: 7,
    ),
    _Item(
      start: 13,
      physical: 9,
      hidden: 3,
      continuationStart: 0,
      continuationEnd: 3,
      contentUtf8: 4,
      contentUtf16: 4,
      markerStart: 0,
      markerEnd: 2,
      markerValue: 9,
    ),
    _Item(
      start: 22,
      physical: 4,
      hidden: 4,
      continuationStart: 0,
      continuationEnd: 4,
      contentUtf8: 0,
      contentUtf16: 0,
      markerStart: 0,
      markerEnd: 3,
      markerValue: 42,
    ),
  ];

  test('decodes exact zero-padded marker and parser-driven continuation', () {
    final authority = _authority(mixedSource);
    final result = _decode(
      authority,
      _orderedViewport(
        source: mixedSource,
        items: mixedItems,
        selectedOrdinal: 0,
        listStart: 7,
        delimiter: 0x29,
        canonicalLineEnding: 2,
      ),
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.orderedList);
    final payload = result.orderedListProjection!;
    final generic = payload as FlarkV3TightListItemProjectionPayload;
    expect(payload.facts.start, 7);
    expect(payload.facts.delimiter, FlarkV3OrderedListDelimiter.parenthesis);
    expect(payload.selectedItemOrdinal, 0);
    expect(payload.selectedMarkerText, '007)');
    expect(
      _read(authority.document, payload.selectedItem.openingMarker),
      '007)',
    );
    expect(payload.selectedItem.markerValue, 7);
    expect(payload.toSourceProjection().sourceText, '007) α😀\r\n');
    expect(payload.toSourceProjection().displayText, 'α😀\n');
    expect(generic.toSelectedItemSourceProjection().displayText, 'α😀\n');
    expect(payload.editingInputs.activeHiddenSourcePrefix, '007) ');
    expect(payload.editingInputs.activeRemovableSourcePrefix, '007) ');
    expect(payload.editingInputs.continuationSourcePrefix, '008) ');
    expect(payload.editingInputs.canonicalLineEnding, '\r\n');
    expect(payload.editingInputs.emptyEnterExits, isFalse);
    expect(payload.editingInputs.backspaceAtStartRemovesPrefix, isTrue);
  });

  test('nonsequential marker increments its certified literal value', () {
    final authority = _authority(mixedSource);
    final payload = _decode(
      authority,
      _orderedViewport(
        source: mixedSource,
        items: mixedItems,
        selectedOrdinal: 1,
        listStart: 7,
        delimiter: 0x29,
        canonicalLineEnding: 2,
      ),
    ).orderedListProjection!;

    expect(payload.selectedMarkerText, '9)');
    expect(payload.selectedItem.markerValue, 9);
    expect(payload.editingInputs.continuationSourcePrefix, '10) ');
    expect(payload.toSourceProjection().displayText, 'beta\n');
  });

  test('terminal empty item exposes exact exit authority', () {
    final authority = _authority(mixedSource);
    final payload = _decode(
      authority,
      _orderedViewport(
        source: mixedSource,
        items: mixedItems,
        selectedOrdinal: 2,
        listStart: 7,
        delimiter: 0x29,
        canonicalLineEnding: 2,
      ),
    ).orderedListProjection!;

    expect(payload.selectedMarkerText, '42)');
    expect(payload.toSourceProjection().displayText, isEmpty);
    expect(payload.editingInputs.activeHiddenSourcePrefix, '42) ');
    expect(payload.editingInputs.continuationSourcePrefix, '43) ');
    expect(payload.editingInputs.canonicalLineEnding, '\r\n');
    expect(payload.editingInputs.emptyEnterExits, isTrue);
  });

  test('BOF BOM and opening indent remain protected source', () {
    const source = '\uFEFF  007) item\n';
    const items = <_Item>[
      _Item(
        start: 0,
        physical: 15,
        hidden: 10,
        continuationStart: 3,
        continuationEnd: 10,
        contentUtf8: 4,
        contentUtf16: 4,
        markerStart: 5,
        markerEnd: 9,
        markerValue: 7,
      ),
    ];
    final authority = _authority(source);
    final payload = _decode(
      authority,
      _orderedViewport(
        source: source,
        items: items,
        selectedOrdinal: 0,
        listStart: 7,
        delimiter: 0x29,
        canonicalLineEnding: 1,
      ),
    ).orderedListProjection!;

    expect(payload.selectedMarkerText, '007)');
    expect(payload.editingInputs.activeHiddenSourcePrefix, '\uFEFF  007) ');
    expect(payload.editingInputs.activeRemovableSourcePrefix, '  007) ');
    expect(payload.editingInputs.activeRemovableSourcePrefixOffsetUtf16, 1);
    expect(payload.editingInputs.continuationSourcePrefix, '  008) ');
    expect(payload.toSourceProjection().displayText, 'item\n');
  });

  test('maximum marker repeats by explicit valid-overflow policy', () {
    const source = '999999999. max\n';
    const items = <_Item>[
      _Item(
        start: 0,
        physical: 15,
        hidden: 11,
        continuationStart: 0,
        continuationEnd: 11,
        contentUtf8: 3,
        contentUtf16: 3,
        markerStart: 0,
        markerEnd: 10,
        markerValue: 999999999,
      ),
    ];
    final authority = _authority(source);
    final payload = _decode(
      authority,
      _orderedViewport(
        source: source,
        items: items,
        selectedOrdinal: 0,
        listStart: 999999999,
        delimiter: 0x2e,
        canonicalLineEnding: 1,
      ),
    ).orderedListProjection!;

    expect(
      FlarkV3OrderedListProjectionDecoder.continuationOverflowPolicy,
      FlarkV3OrderedListContinuationOverflowPolicy.repeatCurrentMarker,
    );
    expect(payload.selectedMarkerText, '999999999.');
    expect(payload.editingInputs.continuationSourcePrefix, '999999999. ');
  });

  test('rejects reserved, marker-value, delimiter, and span tampering', () {
    const source = '007) item\n';
    const item = _Item(
      start: 0,
      physical: 10,
      hidden: 5,
      continuationStart: 0,
      continuationEnd: 5,
      contentUtf8: 4,
      contentUtf16: 4,
      markerStart: 0,
      markerEnd: 4,
      markerValue: 7,
    );
    final authority = _authority(source);
    final result = _decode(
      authority,
      _orderedViewport(
        source: source,
        items: const <_Item>[item],
        selectedOrdinal: 0,
        listStart: 7,
        delimiter: 0x29,
        canonicalLineEnding: 1,
      ),
    );
    final facts = result.structure.orderedList!;
    final pointPath = result.pointPath!;
    final listSource = result.structure.source;

    Uint8List validPayload() =>
        _orderedPayload(item: item, selectedOrdinal: 0, canonicalLineEnding: 1);

    final reserved = validPayload()..[5] = 1;
    final wrongValue = validPayload();
    ByteData.sublistView(wrongValue).setUint32(16, 8, Endian.little);
    final wrongSpan = validPayload();
    ByteData.sublistView(wrongSpan)
      ..setUint32(8, 1, Endian.little)
      ..setUint32(12, 5, Endian.little);

    for (final payload in <Uint8List>[reserved, wrongValue, wrongSpan]) {
      expect(
        () => FlarkV3OrderedListProjectionDecoder.decodeSelectedItem(
          sourceDocument: authority.document,
          expectedSource: authority.version,
          source: listSource,
          facts: facts,
          pointPath: pointPath,
          encodedPayload: payload,
        ),
        throwsA(isA<FlarkV3OrderedListProjectionDecodeException>()),
      );
    }

    const hostileSource = '0x7) item\n';
    final hostileAuthority = _authority(hostileSource);
    expect(
      () => FlarkV3OrderedListProjectionDecoder.decodeSelectedItem(
        sourceDocument: hostileAuthority.document,
        expectedSource: hostileAuthority.version,
        source: listSource,
        facts: facts,
        pointPath: pointPath,
        encodedPayload: validPayload(),
      ),
      throwsA(isA<FlarkV3OrderedListProjectionDecodeException>()),
      reason: 'marker integrity checks digits without classifying Markdown',
    );

    final wrongDelimiterFacts = FlarkV3OrderedListFacts(
      start: facts.start,
      delimiter: FlarkV3OrderedListDelimiter.period,
      itemCount: facts.itemCount,
      terminalEmptyRelativeStartUtf8: facts.terminalEmptyRelativeStartUtf8,
      paragraphCount: facts.paragraphCount,
      projectedUtf8Length: facts.projectedUtf8Length,
      projectedUtf16Length: facts.projectedUtf16Length,
    );
    expect(
      () => FlarkV3OrderedListProjectionDecoder.decodeSelectedItem(
        sourceDocument: authority.document,
        expectedSource: authority.version,
        source: listSource,
        facts: wrongDelimiterFacts,
        pointPath: pointPath,
        encodedPayload: validPayload(),
      ),
      throwsA(isA<FlarkV3OrderedListProjectionDecodeException>()),
    );
  });

  test('transport width stays isolated and fixed', () {
    expect(FlarkV3OrderedListProjectionDecoder.metadataBytes, 20);
    expect(FlarkV3OrderedListProjectionDecoder.recordBytes, 28);
    expect(FlarkV3OrderedListProjectionDecoder.encodedPayloadBytes, 48);
  });
}

typedef _Authority = ({
  FlarkV3SourceDocument document,
  FlarkV3SourceVersion version,
});

final class _Item {
  const _Item({
    required this.start,
    required this.physical,
    required this.hidden,
    required this.continuationStart,
    required this.continuationEnd,
    required this.contentUtf8,
    required this.contentUtf16,
    required this.markerStart,
    required this.markerEnd,
    required this.markerValue,
  });

  final int start;
  final int physical;
  final int hidden;
  final int continuationStart;
  final int continuationEnd;
  final int contentUtf8;
  final int contentUtf16;
  final int markerStart;
  final int markerEnd;
  final int markerValue;

  int get lineEnding => physical - hidden - contentUtf8;
  int get projectedUtf8 => contentUtf8 + lineEnding;
  int get projectedUtf16 => contentUtf16 + lineEnding;
}

_Authority _authority(String source) {
  final document = FlarkV3SourceDocument.fromString(source);
  return (
    document: document,
    version: FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(31, 32, 33, 34),
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
      openDepth: 3,
      treeNodesVisited: 3,
      summaryNodesSkipped: 0,
    ),
  ),
);

Uint8List _orderedViewport({
  required String source,
  required List<_Item> items,
  required int selectedOrdinal,
  required int listStart,
  required int delimiter,
  required int canonicalLineEnding,
}) {
  final sourceUtf8 = utf8.encode(source).length;
  final terminalEmpty = items.last.contentUtf8 == 0;
  final paragraphCount = items.where((item) => item.contentUtf8 != 0).length;
  final projectedUtf8 = items.fold<int>(
    0,
    (sum, item) => sum + item.projectedUtf8,
  );
  final projectedUtf16 = items.fold<int>(
    0,
    (sum, item) => sum + item.projectedUtf16,
  );
  final selected = items[selectedOrdinal];
  final selectedEmpty = selected.contentUtf8 == 0;
  final nodeCount = selectedEmpty ? 2 : 3;
  final payloadStart = _pathStart + nodeCount * _pathRecordBytes;
  final payload = _orderedPayload(
    item: selected,
    selectedOrdinal: selectedOrdinal,
    canonicalLineEnding: canonicalLineEnding,
  );
  final bytes = Uint8List(payloadStart + payload.length);
  final data = ByteData.sublistView(bytes);

  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 7, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little)
    ..setUint16(20, nodeCount, Endian.little)
    ..setUint8(22, 6)
    ..setUint8(23, 0)
    ..setUint32(24, nodeCount * _pathRecordBytes, Endian.little)
    ..setUint32(28, payload.length, Endian.little);

  bytes.setRange(_greenStart, _greenStart + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(_greenStart + 8, 1, Endian.little)
    ..setUint8(_greenStart + 12, 10)
    ..setUint64(_greenStart + 16, 0, Endian.little)
    ..setUint64(_greenStart + 24, sourceUtf8, Endian.little)
    ..setUint64(_greenStart + 32, 0, Endian.little)
    ..setUint64(_greenStart + 40, 0, Endian.little)
    ..setUint64(
      _greenStart + 48,
      1 | (delimiter << 8) | (1 << 16),
      Endian.little,
    )
    ..setUint32(_greenStart + 56, items.length, Endian.little)
    ..setUint32(
      _greenStart + 60,
      terminalEmpty ? items.last.start : 0xffffffff,
      Endian.little,
    )
    ..setUint32(_greenStart + 64, paragraphCount, Endian.little)
    ..setUint32(_greenStart + 68, projectedUtf8, Endian.little)
    ..setUint32(_greenStart + 72, projectedUtf16, Endian.little)
    ..setUint32(_greenStart + 76, listStart, Endian.little);

  bytes.setRange(
    _projectionStart,
    _projectionStart + 8,
    ascii.encode('FLKPR001'),
  );
  data
    ..setUint32(_projectionStart + 8, 1, Endian.little)
    ..setUint8(_projectionStart + 12, 10)
    ..setUint64(_projectionStart + 16, 0, Endian.little)
    ..setUint64(_projectionStart + 24, sourceUtf8, Endian.little)
    ..setUint64(_projectionStart + 32, 0, Endian.little)
    ..setUint64(_projectionStart + 40, 0, Endian.little)
    ..setUint64(_projectionStart + 48, items.length, Endian.little);

  _writePathNode(
    data,
    _pathStart,
    kind: 3,
    flags: 1,
    depth: 0,
    parent: 0xffffffff,
    sourceStart: 0,
    sourceEnd: sourceUtf8,
    firstRun: 0,
    runCount: items.length,
    projectedUtf8: projectedUtf8,
    projectedUtf16: projectedUtf16,
  );
  _writePathNode(
    data,
    _pathStart + _pathRecordBytes,
    kind: 4,
    flags: 1 | (selectedEmpty ? 2 : 0),
    depth: 1,
    parent: 0,
    sourceStart: selected.start,
    sourceEnd: selected.start + selected.physical,
    firstRun: selectedOrdinal,
    runCount: 1,
    projectedUtf8: selected.projectedUtf8,
    projectedUtf16: selected.projectedUtf16,
  );
  if (!selectedEmpty) {
    _writePathNode(
      data,
      _pathStart + 2 * _pathRecordBytes,
      kind: 2,
      flags: 2,
      depth: 2,
      parent: 1,
      sourceStart: selected.start + selected.hidden,
      sourceEnd: selected.start + selected.hidden + selected.contentUtf8,
      firstRun: selectedOrdinal,
      runCount: 1,
      projectedUtf8: selected.contentUtf8,
      projectedUtf16: selected.contentUtf16,
    );
  }
  bytes.setRange(payloadStart, bytes.length, payload);
  return bytes;
}

Uint8List _orderedPayload({
  required _Item item,
  required int selectedOrdinal,
  required int canonicalLineEnding,
}) {
  final bytes = Uint8List(
    FlarkV3OrderedListProjectionDecoder.encodedPayloadBytes,
  );
  final data = ByteData.sublistView(bytes);
  data
    ..setUint32(0, selectedOrdinal, Endian.little)
    ..setUint8(4, canonicalLineEnding)
    ..setUint32(8, item.markerStart, Endian.little)
    ..setUint32(12, item.markerEnd, Endian.little)
    ..setUint32(16, item.markerValue, Endian.little)
    ..setUint32(20, item.start, Endian.little)
    ..setUint32(24, item.physical, Endian.little)
    ..setUint32(28, item.hidden, Endian.little)
    ..setUint32(32, item.continuationStart, Endian.little)
    ..setUint32(36, item.continuationEnd, Endian.little)
    ..setUint32(40, item.contentUtf8, Endian.little)
    ..setUint32(44, item.contentUtf16, Endian.little);
  return bytes;
}

void _writePathNode(
  ByteData data,
  int offset, {
  required int kind,
  required int flags,
  required int depth,
  required int parent,
  required int sourceStart,
  required int sourceEnd,
  required int firstRun,
  required int runCount,
  required int projectedUtf8,
  required int projectedUtf16,
}) {
  data
    ..setUint8(offset, kind)
    ..setUint8(offset + 1, flags)
    ..setUint16(offset + 2, depth, Endian.little)
    ..setUint32(offset + 4, parent, Endian.little)
    ..setUint32(offset + 8, sourceStart, Endian.little)
    ..setUint32(offset + 12, sourceEnd, Endian.little)
    ..setUint32(offset + 16, firstRun, Endian.little)
    ..setUint32(offset + 20, runCount, Endian.little)
    ..setUint32(offset + 24, projectedUtf8, Endian.little)
    ..setUint32(offset + 28, projectedUtf16, Endian.little);
}

String _read(FlarkV3SourceDocument document, FlarkV3SourceSpan span) =>
    document.readRange(span.startUtf16, span.endUtf16);

const int _greenStart = 32;
const int _projectionStart = _greenStart + 80;
const int _pathStart = _projectionStart + 56;
const int _pathRecordBytes = 32;

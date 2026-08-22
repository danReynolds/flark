import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_bullet_list_projection.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_current_revision_inline_cache.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('decodes Unicode, CRLF, BOM, and the selected item projection', () {
    const source = '\uFEFF- α\r\n- 🙂\n';
    const items = <_Item>[
      _Item(
        start: 0,
        physical: 9,
        hidden: 5,
        continuationStart: 3,
        continuationEnd: 5,
        contentUtf8: 2,
        contentUtf16: 1,
      ),
      _Item(
        start: 9,
        physical: 7,
        hidden: 2,
        continuationStart: 0,
        continuationEnd: 2,
        contentUtf8: 4,
        contentUtf16: 2,
      ),
    ];
    final authority = _authority(source);
    final result = _decode(
      authority,
      _listViewport(
        source: source,
        items: items,
        selectedOrdinal: 1,
        marker: 0x2d,
      ),
    );

    expect(result.structure.kind, FlarkV3DocumentStructureKind.bulletList);
    expect(result.projection.kind, FlarkV3DocumentStructureKind.bulletList);
    expect(result.projection.runCount, 2);
    final facts = result.structure.bulletList!;
    expect(facts.marker, FlarkV3BulletListMarker.hyphen);
    expect(facts.itemCount, 2);
    expect(facts.paragraphCount, 2);
    expect(facts.terminalEmptyRelativeStartUtf8, isNull);
    expect(facts.projectedUtf8Length, 9);
    expect(facts.projectedUtf16Length, 6);

    final payload = result.bulletListProjection!;
    final FlarkV3TightListItemProjectionPayload genericPayload = payload;
    expect(payload.sourceVersion, authority.version);
    expect(genericPayload.selectedItemOrdinal, 1);
    expect(genericPayload.editingInputs.continuationSourcePrefix, '- ');
    expect(payload.records, hasLength(2));
    expect(payload.selectedItemOrdinal, 1);
    expect(
      _read(authority.document, payload.records[0].hiddenPrefix),
      '\uFEFF- ',
    );
    expect(
      _read(authority.document, payload.records[0].continuationPrefix),
      '- ',
    );
    expect(payload.records[0].continuationPrefix.startUtf8, 3);
    expect(payload.records[0].continuationPrefix.endUtf8, 5);
    expect(payload.records[0].continuationPrefix.startUtf16, 1);
    expect(payload.records[0].continuationPrefix.endUtf16, 3);
    expect(_read(authority.document, payload.selectedItem.content), '🙂');
    final wholeProjection = payload.toSourceProjection();
    expect(wholeProjection.sourceText, source);
    expect(wholeProjection.displayText, 'α\n🙂\n');
    expect(wholeProjection.pieces[2].isReplaced, isTrue);
    expect(wholeProjection.pieces[2].displayText, '\n');
    final selectedProjection = payload.toSelectedItemSourceProjection();
    expect(selectedProjection.sourceStartUtf16, 6);
    expect(selectedProjection.sourceEndUtf16, 11);
    expect(selectedProjection.sourceText, '- 🙂\n');
    expect(selectedProjection.displayText, '🙂\n');
    expect(selectedProjection.pieces.first.sourceStartUtf16, 6);
    expect(selectedProjection.pieces.first.sourceEndUtf16, 8);
    expect(payload.selectedItem.projectedUtf16Length, 3);
    expect(payload.selectedItemDisplayUtf16Length, 3);
    expect(payload.editingInputs.activeHiddenSourcePrefix, '- ');
    expect(payload.editingInputs.activeRemovableSourcePrefix, '- ');
    expect(payload.editingInputs.activeRemovableSourcePrefixOffsetUtf16, 0);
    expect(payload.editingInputs.continuationSourcePrefix, '- ');
    expect(payload.editingInputs.canonicalLineEnding, '\n');
    expect(payload.editingInputs.emptyEnterExits, isFalse);
    expect(payload.editingInputs.backspaceAtStartRemovesPrefix, isTrue);

    final path = payload.pointPath;
    expect(path.nodes, hasLength(3));
    expect(path.root.kind, FlarkV3DocumentPointPathNodeKind.list);
    expect(path.selectedLeaf.kind, FlarkV3DocumentPointPathNodeKind.paragraph);
    expect(path.selectedLeaf.firstRun, 1);

    final firstSelected = _decode(
      authority,
      _listViewport(
        source: source,
        items: items,
        selectedOrdinal: 0,
        marker: 0x2d,
      ),
    ).bulletListProjection!;
    expect(firstSelected.selectedItem.projectedUtf16Length, 3);
    expect(firstSelected.selectedItemDisplayUtf16Length, 2);
    expect(
      firstSelected.toSelectedItemSourceProjection().sourceText,
      '\uFEFF- α\r\n',
    );
    expect(firstSelected.toSelectedItemSourceProjection().displayText, 'α\n');
  });

  test(
    'terminal empty item keeps parser-carried interior removal authority',
    () {
      const source = '- one\r\n-   ';
      const items = <_Item>[
        _Item(
          start: 0,
          physical: 7,
          hidden: 2,
          continuationStart: 0,
          continuationEnd: 2,
          contentUtf8: 3,
          contentUtf16: 3,
        ),
        _Item(
          start: 7,
          physical: 4,
          hidden: 4,
          continuationStart: 0,
          continuationEnd: 2,
          contentUtf8: 0,
          contentUtf16: 0,
        ),
      ];
      final authority = _authority(source);
      final result = _decode(
        authority,
        _listViewport(
          source: source,
          items: items,
          selectedOrdinal: 1,
          marker: 0x2d,
        ),
      );
      final payload = result.bulletListProjection!;

      expect(result.structure.bulletList!.terminalEmptyRelativeStartUtf8, 7);
      expect(payload.pointPath.nodes, hasLength(2));
      expect(
        payload.pointPath.selectedLeaf.kind,
        FlarkV3DocumentPointPathNodeKind.listItem,
      );
      expect(payload.selectedItem.isEmpty, isTrue);
      final wholeProjection = payload.toSourceProjection();
      expect(wholeProjection.sourceText, source);
      expect(wholeProjection.displayText, 'one\n');
      expect(wholeProjection.pieces[2].isReplaced, isTrue);
      final selectedProjection = payload.toSelectedItemSourceProjection();
      expect(selectedProjection.sourceStartUtf16, 7);
      expect(selectedProjection.sourceEndUtf16, 11);
      expect(selectedProjection.sourceText, '-   ');
      expect(selectedProjection.displayText, isEmpty);
      expect(payload.selectedItem.continuationPrefix.startUtf8, 7);
      expect(payload.selectedItem.continuationPrefix.endUtf8, 9);
      expect(payload.selectedItem.hiddenPrefix.endUtf8, 11);
      expect(
        _read(
          authority.document,
          FlarkV3SourceSpan(
            startUtf8: payload.selectedItem.continuationPrefix.endUtf8,
            endUtf8: payload.selectedItem.hiddenPrefix.endUtf8,
            startUtf16: payload.selectedItem.continuationPrefix.endUtf16,
            endUtf16: payload.selectedItem.hiddenPrefix.endUtf16,
          ),
        ),
        '  ',
        reason: 'the exact interior removal cut preserves trailing spaces',
      );
      expect(payload.editingInputs.activeHiddenSourcePrefix, '-   ');
      expect(payload.editingInputs.activeRemovableSourcePrefix, '- ');
      expect(payload.editingInputs.activeRemovableSourcePrefixOffsetUtf16, 0);
      expect(payload.editingInputs.continuationSourcePrefix, '- ');
      expect(payload.editingInputs.canonicalLineEnding, '\r\n');
      expect(payload.editingInputs.emptyEnterExits, isTrue);
    },
  );

  test('BOF BOM removal offset and EOF line-ending fallback remain exact', () {
    const source = '\uFEFF-   ';
    const items = <_Item>[
      _Item(
        start: 0,
        physical: 7,
        hidden: 7,
        continuationStart: 3,
        continuationEnd: 5,
        contentUtf8: 0,
        contentUtf16: 0,
      ),
    ];
    final authority = _authority(source);
    final result = _decode(
      authority,
      _listViewport(
        source: source,
        items: items,
        selectedOrdinal: 0,
        marker: 0x2d,
      ),
    );
    final inputs = result.bulletListProjection!.editingInputs;
    final record = result.bulletListProjection!.selectedItem;

    expect(inputs.activeHiddenSourcePrefix, '\uFEFF-   ');
    expect(inputs.activeRemovableSourcePrefix, '- ');
    expect(inputs.activeRemovableSourcePrefixOffsetUtf16, 1);
    expect(record.continuationPrefix.startUtf8, 3);
    expect(record.continuationPrefix.endUtf8, 5);
    expect(record.continuationPrefix.startUtf16, 1);
    expect(record.continuationPrefix.endUtf16, 3);
    expect(inputs.continuationSourcePrefix, '- ');
    expect(inputs.canonicalLineEnding, '\n');
    expect(inputs.emptyEnterExits, isTrue);
  });

  test('rejects hostile item cuts, metrics, tiling, and selected path', () {
    const source = '\uFEFF- α\r\n- 🙂\n';
    const items = <_Item>[
      _Item(
        start: 0,
        physical: 9,
        hidden: 5,
        continuationStart: 3,
        continuationEnd: 5,
        contentUtf8: 2,
        contentUtf16: 1,
      ),
      _Item(
        start: 9,
        physical: 7,
        hidden: 2,
        continuationStart: 0,
        continuationEnd: 2,
        contentUtf8: 4,
        contentUtf16: 2,
      ),
    ];
    final authority = _authority(source);
    final valid = _listViewport(
      source: source,
      items: items,
      selectedOrdinal: 1,
      marker: 0x2d,
    );
    final payloadStart = _pathStart + 3 * _pathRecordBytes;

    for (final mutate in <void Function(Uint8List, ByteData)>[
      (_, data) => data.setUint32(payloadStart + 16, 6, Endian.little),
      (_, data) => data.setUint32(payloadStart + 24, 2, Endian.little),
      (_, data) =>
          data.setUint32(payloadStart + _itemRecordBytes, 8, Endian.little),
      (_, data) => data.setUint32(_pathStart + 32 + 16, 2, Endian.little),
      (_, data) => data.setUint32(_pathStart + 64 + 8, 12, Endian.little),
      (_, data) => data.setUint32(_greenStart + 68, 8, Endian.little),
    ]) {
      final corrupt = Uint8List.fromList(valid);
      mutate(corrupt, ByteData.sublistView(corrupt));
      expect(
        () => _decode(authority, corrupt),
        throwsA(isA<FlarkV3DocumentQueryException>()),
      );
    }
  });

  test('current-revision inline cache preserves the typed list query', () {
    const source = '- item\n';
    const items = <_Item>[
      _Item(
        start: 0,
        physical: 7,
        hidden: 2,
        continuationStart: 0,
        continuationEnd: 2,
        contentUtf8: 4,
        contentUtf16: 4,
      ),
    ];
    final authority = _authority(source);
    final query = _decode(
      authority,
      _listViewport(
        source: source,
        items: items,
        selectedOrdinal: 0,
        marker: 0x2d,
      ),
    );
    final cache = FlarkV3CurrentRevisionInlineCache(
      maximumEntries: 2,
      maximumFactRecords: 2,
    );
    final resolved = cache.resolve(
      authority: _ack(authority.version),
      query: query,
    );

    expect(resolved, same(query));
    expect(resolved.bulletListProjection, same(query.bulletListProjection));
  });

  test(
    'sequential list and selected-item inline certificates join exactly',
    () {
      const source = '- **x**\n';
      const items = <_Item>[
        _Item(
          start: 0,
          physical: 8,
          hidden: 2,
          continuationStart: 0,
          continuationEnd: 2,
          contentUtf8: 5,
          contentUtf16: 5,
        ),
      ];
      final authority = _authority(source);
      final listQuery = _decode(
        authority,
        _listViewport(
          source: source,
          items: items,
          selectedOrdinal: 0,
          marker: 0x2d,
        ),
      );
      final inlineQuery = _decode(
        authority,
        _selectedItemInlineViewport(
          source: source,
          items: items,
          leafStart: 2,
          leafEnd: 7,
          disposition: 1,
          record: _strongRecord(),
        ),
      );
      expect(inlineQuery.inlineFacts!.source.startUtf8, 2);
      expect(inlineQuery.inlineFacts!.source.endUtf8, 7);
      expect(inlineQuery.bulletListProjection, isNull);

      final cache = FlarkV3CurrentRevisionInlineCache(
        maximumEntries: 2,
        maximumFactRecords: 2,
      );
      final ack = _ack(authority.version);
      cache.resolve(authority: ack, query: listQuery);
      final joined = cache.resolve(authority: ack, query: inlineQuery);

      expect(joined.bulletListProjection, same(listQuery.bulletListProjection));
      expect(joined.pointPath, same(listQuery.pointPath));
      expect(joined.inlineFacts, same(inlineQuery.inlineFacts));
      expect(
        joined.inlineFacts!.source.startUtf16,
        joined.bulletListProjection!.selectedItem.content.startUtf16,
      );
      expect(
        joined.inlineFacts!.facts.single.kind,
        FlarkV3InlineFactKind.strong,
      );

      final mismatchedRange = _decode(
        authority,
        _selectedItemInlineViewport(
          source: source,
          items: items,
          leafStart: 3,
          leafEnd: 7,
          disposition: 2,
        ),
      );
      final mismatchResult = cache.resolve(
        authority: ack,
        query: mismatchedRange,
      );
      expect(mismatchResult, same(mismatchedRange));
      expect(mismatchResult.bulletListProjection, isNull);
      expect(
        mismatchResult.inlineFacts!.source,
        isNot(listQuery.bulletListProjection!.selectedItem.content),
        reason: 'nearby parser authority must not decorate the selected item',
      );

      final unsupported = _decode(
        authority,
        _selectedItemInlineViewport(
          source: source,
          items: items,
          leafStart: 2,
          leafEnd: 7,
          disposition: 2,
        ),
      );
      final unsupportedJoined = cache.resolve(
        authority: ack,
        query: unsupported,
      );
      expect(
        unsupportedJoined.inlineFacts!.disposition,
        FlarkV3InlineFactsDisposition.unsupported,
      );
      expect(unsupportedJoined.inlineFacts!.facts, isEmpty);
      expect(unsupportedJoined.bulletListProjection, isNotNull);
    },
  );

  test(
    'schema 6 decodes one selected item independently of whole-list size',
    () {
      const itemCount = 2000;
      const selectedOrdinal = 1537;
      final source = StringBuffer();
      final items = <_Item>[];
      var offset = 0;
      for (var ordinal = 0; ordinal < itemCount; ordinal += 1) {
        final content = 'item-${ordinal.toString().padLeft(4, '0')}';
        final physical = 2 + content.length + 2;
        items.add(
          _Item(
            start: offset,
            physical: physical,
            hidden: 2,
            continuationStart: 0,
            continuationEnd: 2,
            contentUtf8: content.length,
            contentUtf16: content.length,
          ),
        );
        source.write('- $content\r\n');
        offset += physical;
      }
      final authority = _authority(source.toString());
      final encoded = _compactListItemViewport(
        source: source.toString(),
        items: items,
        selectedOrdinal: selectedOrdinal,
        marker: 0x2d,
        canonicalLineEnding: 2,
      );
      final payload = _decode(authority, encoded).bulletListProjection!;

      expect(encoded, hasLength(32 + 80 + 56 + 3 * 32 + 8 + 28));
      expect(payload.coversWholeList, isFalse);
      expect(payload.records, hasLength(1));
      expect(payload.selectedItemOrdinal, selectedOrdinal);
      expect(payload.selectedItem.ordinal, selectedOrdinal);
      expect(
        payload.selectedItem.relativeItemStartUtf8,
        items[selectedOrdinal].start,
      );
      expect(payload.toSourceProjection().sourceText, '- item-1537\r\n');
      expect(payload.toSourceProjection().displayText, 'item-1537\n');
      expect(
        payload.toSelectedItemSourceProjection().displayText,
        'item-1537\n',
      );
      expect(payload.editingInputs.continuationSourcePrefix, '- ');
      expect(payload.editingInputs.canonicalLineEnding, '\r\n');
    },
  );
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
  });

  final int start;
  final int physical;
  final int hidden;
  final int continuationStart;
  final int continuationEnd;
  final int contentUtf8;
  final int contentUtf16;

  int get lineEnding => physical - hidden - contentUtf8;
  int get projectedUtf8 => contentUtf8 + lineEnding;
  int get projectedUtf16 => contentUtf16 + lineEnding;
}

_Authority _authority(String source) {
  final document = FlarkV3SourceDocument.fromString(source);
  return (
    document: document,
    version: FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(21, 22, 23, 24),
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

Uint8List _listViewport({
  required String source,
  required List<_Item> items,
  required int selectedOrdinal,
  required int marker,
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
  final payloadLength = items.length * _itemRecordBytes;
  final payloadStart = _pathStart + nodeCount * _pathRecordBytes;
  final bytes = Uint8List(payloadStart + payloadLength);
  final data = ByteData.sublistView(bytes);

  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 5, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little)
    ..setUint16(20, nodeCount, Endian.little)
    ..setUint8(22, 4)
    ..setUint8(23, 0)
    ..setUint32(24, nodeCount * _pathRecordBytes, Endian.little)
    ..setUint32(28, payloadLength, Endian.little);

  bytes.setRange(_greenStart, _greenStart + 8, ascii.encode('FLKGR001'));
  data
    ..setUint32(_greenStart + 8, 1, Endian.little)
    ..setUint8(_greenStart + 12, 9)
    ..setUint64(_greenStart + 16, 0, Endian.little)
    ..setUint64(_greenStart + 24, sourceUtf8, Endian.little)
    ..setUint64(_greenStart + 32, 0, Endian.little)
    ..setUint64(_greenStart + 40, 0, Endian.little)
    ..setUint64(_greenStart + 48, 1 | (marker << 8) | (1 << 16), Endian.little)
    ..setUint32(_greenStart + 56, items.length, Endian.little)
    ..setUint32(
      _greenStart + 60,
      terminalEmpty ? items.last.start : 0xffffffff,
      Endian.little,
    )
    ..setUint32(_greenStart + 64, paragraphCount, Endian.little)
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
    ..setUint8(_projectionStart + 12, 9)
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

  for (var index = 0; index < items.length; index += 1) {
    final item = items[index];
    final offset = payloadStart + index * _itemRecordBytes;
    data
      ..setUint32(offset, item.start, Endian.little)
      ..setUint32(offset + 4, item.physical, Endian.little)
      ..setUint32(offset + 8, item.hidden, Endian.little)
      ..setUint32(offset + 12, item.continuationStart, Endian.little)
      ..setUint32(offset + 16, item.continuationEnd, Endian.little)
      ..setUint32(offset + 20, item.contentUtf8, Endian.little)
      ..setUint32(offset + 24, item.contentUtf16, Endian.little);
  }
  return bytes;
}

Uint8List _compactListItemViewport({
  required String source,
  required List<_Item> items,
  required int selectedOrdinal,
  required int marker,
  required int canonicalLineEnding,
}) {
  final whole = _listViewport(
    source: source,
    items: items,
    selectedOrdinal: selectedOrdinal,
    marker: marker,
  );
  final selected = items[selectedOrdinal];
  final nodeCount = selected.contentUtf8 == 0 ? 2 : 3;
  final payloadStart = _pathStart + nodeCount * _pathRecordBytes;
  final compact = Uint8List(payloadStart + 8 + _itemRecordBytes);
  compact.setRange(0, payloadStart, whole);
  final compactData = ByteData.sublistView(compact);
  compactData
    ..setUint32(8, 6, Endian.little)
    ..setUint8(22, 5)
    ..setUint32(28, 8 + _itemRecordBytes, Endian.little)
    ..setUint32(payloadStart, selectedOrdinal, Endian.little)
    ..setUint8(payloadStart + 4, canonicalLineEnding);
  final wholePayloadStart = _pathStart + nodeCount * _pathRecordBytes;
  final selectedRecordStart =
      wholePayloadStart + selectedOrdinal * _itemRecordBytes;
  compact.setRange(
    payloadStart + 8,
    payloadStart + 8 + _itemRecordBytes,
    whole,
    selectedRecordStart,
  );
  return compact;
}

Uint8List _selectedItemInlineViewport({
  required String source,
  required List<_Item> items,
  required int leafStart,
  required int leafEnd,
  required int disposition,
  Uint8List? record,
}) {
  final list = _listViewport(
    source: source,
    items: items,
    selectedOrdinal: 0,
    marker: 0x2d,
  );
  final factCount = record == null ? 0 : 1;
  final payloadLength = 48 + factCount * 20;
  const headerBytes = 24;
  const greenStart = headerBytes;
  const projectionStart = greenStart + 80;
  const payloadStart = projectionStart + 56;
  final bytes = Uint8List(payloadStart + payloadLength);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 8, Endian.little)
    ..setUint32(12, 80, Endian.little)
    ..setUint32(16, 56, Endian.little)
    ..setUint32(20, payloadLength, Endian.little);
  bytes.setRange(greenStart, projectionStart, list, _greenStart);
  bytes.setRange(projectionStart, payloadStart, list, _projectionStart);
  bytes.setRange(payloadStart, payloadStart + 8, ascii.encode('FLKIN002'));
  data
    ..setUint32(payloadStart + 8, 2, Endian.little)
    ..setUint8(payloadStart + 12, disposition)
    ..setUint32(payloadStart + 16, 1, Endian.little)
    ..setUint32(payloadStart + 20, factCount, Endian.little)
    ..setUint64(payloadStart + 24, leafStart, Endian.little)
    ..setUint64(payloadStart + 32, leafEnd, Endian.little)
    ..setUint32(payloadStart + 40, 20, Endian.little)
    ..setUint32(payloadStart + 44, 0, Endian.little);
  if (record != null) {
    bytes.setRange(payloadStart + 48, bytes.length, record);
  }
  return bytes;
}

Uint8List _strongRecord() {
  final record = Uint8List(20);
  ByteData.sublistView(record)
    ..setUint8(0, 2)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 5, Endian.little)
    ..setUint32(12, 2, Endian.little)
    ..setUint32(16, 1, Endian.little);
  return record;
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

FlarkV3StructuralAck _ack(FlarkV3SourceVersion sourceVersion) =>
    FlarkV3StructuralAck(
      publicationSession: FlarkV3PublicationSessionId(1, 2, 3, 4),
      hostRevision: FlarkV3HostRevisionId(1),
      sourceVersion: sourceVersion,
      sourceRoot: FlarkV3SourceRootId(1, 1),
      parseGeneration: 1,
      grammarRevision: 1,
      syntaxProfile: FlarkV3SyntaxProfileId(1),
      authorityMask: FlarkV3StructuralAuthorityMask.complete,
      recordCount: 1,
      sequenceDigest: FlarkV3ProtocolDigest128(1, 2, 3, 4),
      manifestDigest: FlarkV3ProtocolDigest128(5, 6, 7, 8),
    );

String _read(FlarkV3SourceDocument document, FlarkV3SourceSpan span) =>
    document.readRange(span.startUtf16, span.endUtf16);

const int _greenStart = 32;
const int _projectionStart = _greenStart + 80;
const int _pathStart = _projectionStart + 56;
const int _pathRecordBytes = 32;
const int _itemRecordBytes = 28;

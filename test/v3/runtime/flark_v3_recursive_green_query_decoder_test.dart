import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/editor/flark_v3_inline_island_presentation.dart';
import 'package:flark/src/v3/editor/flark_v3_inline_projection.dart';
import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_block_quote_projection.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_current_revision_inline_cache.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  test('schema 9 joins nested CM321 inline authority marker-free', () {
    const source = '- a\n  > **b** and _e_\n  ```\n  c\n  ```\n- d\n';
    final document = FlarkV3SourceDocument.fromString(source);
    final version = FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(0x321, 2, 3, 4),
      document: document,
    );
    final point = source.indexOf('b');
    final encoded = _schema9(point);
    final result = FlarkV3DocumentQueryDecoder.decodePointViewport(
      sourceDocument: document,
      expectedSource: version,
      expectedProfilePartition: 1,
      viewport: FlarkV3HostStructuralViewport.owned(
        sourceVersion: version,
        range: FlarkV3MetricRange(
          start: FlarkV3SourceMetric(bytes: point, utf16: point),
          end: FlarkV3SourceMetric(bytes: point + 1, utf16: point + 1),
        ),
        encoded: encoded,
        receipt: FlarkV3HostViewportReceipt(
          encodedBytes: encoded.length,
          leafCount: 1,
          openDepth: 5,
          treeNodesVisited: 2,
          summaryNodesSkipped: 1,
        ),
      ),
    );

    final green = result as FlarkV3RecursiveGreenPointQuery;
    expect(green.source.startUtf16, point);
    expect(green.source.endUtf16, point + 1);
    expect(green.isIdentityEditableContent, isTrue);
    expect(green.ancestry.map((ancestor) => ancestor.kind).toList(), const [
      FlarkV3RecursiveGreenKind.document,
      FlarkV3RecursiveGreenKind.list,
      FlarkV3RecursiveGreenKind.item,
      FlarkV3RecursiveGreenKind.blockQuote,
      FlarkV3RecursiveGreenKind.paragraph,
    ]);
    expect(green.owner, same(green.ancestry.last));
    expect(green.owner.frameId, BigInt.from(5));

    final paragraphStart = source.indexOf('  > ');
    final paragraphEnd = source.indexOf('\n', paragraphStart) + 1;
    final inlineStart = source.indexOf('**b**');
    final inlineEnd = paragraphEnd - 1;
    FlarkV3HotInlineSidecarBinding bindingForFrame(int frame) =>
        FlarkV3HotInlineSidecarBinding(
          parserProfile: FlarkV3SyntaxProfileId(1),
          refinementGeneration: FlarkV3ProtocolU64.fromU32(1),
          blockOrdinal: FlarkV3ProtocolU64(
            lowWord: frame,
            highWord: 0x80000000,
          ),
          physicalStartUtf8: paragraphStart,
          physicalEndUtf8: paragraphEnd,
          visibleStartUtf8: inlineStart,
          visibleEndUtf8: inlineEnd,
          physicalStartUtf16: paragraphStart,
          physicalEndUtf16: paragraphEnd,
          visibleStartUtf16: inlineStart,
          visibleEndUtf16: inlineEnd,
        );
    final binding = bindingForFrame(5);
    final outcome = FlarkV3InlineSidecarQueryAuthoritative(
      payloadKind: FlarkV3InlineSidecarPayloadKind.inline,
      factCount: 2,
      valueEntryCount: 0,
      treeNodesVisited: 3,
      encodedFacts: _inlineFacts(),
      encodedValues: Uint8List(0),
    );
    final joined = FlarkV3DocumentQueryDecoder.joinRecursiveGreenInline(
      sourceDocument: document,
      expectedSource: version,
      expectedProfilePartition: 1,
      query: green,
      binding: binding,
      outcome: outcome,
    );
    expect(joined.paragraphSource?.startUtf16, paragraphStart);
    expect(joined.paragraphSource?.endUtf16, paragraphEnd);
    expect(joined.inlineSource?.startUtf16, inlineStart);
    expect(joined.inlineSource?.endUtf16, inlineEnd);
    expect(joined.inlineFacts?.facts.map((fact) => fact.kind), const [
      FlarkV3InlineFactKind.strong,
      FlarkV3InlineFactKind.emphasis,
    ]);
    expect(
      () => FlarkV3DocumentQueryDecoder.joinRecursiveGreenInline(
        sourceDocument: document,
        expectedSource: version,
        expectedProfilePartition: 1,
        query: green,
        binding: bindingForFrame(6),
        outcome: outcome,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
      reason: 'a neighboring recursive-Green frame cannot borrow the sidecar',
    );

    final presentation =
        FlarkV3InlineIslandPresentation.resolveRecursiveGreenParagraph(
              sourceDocument: document,
              expectedSource: version,
              recursiveQuery: joined,
            )
            as FlarkV3AuthoritativeInlineIslandPresentation;
    expect(presentation.projection.sourceText, '**b** and _e_');
    expect(presentation.projection.displayText, 'b and e');
    expect(presentation.projection.displayText, isNot(contains('*')));
    expect(presentation.projection.displayText, isNot(contains('_')));

    final insertionSource = presentation.projection.displayToSourceOffset(
      1,
      affinity: FlarkV3InlineProjectionAffinity.upstream,
    );
    expect(insertionSource, inlineStart + 3);
    expect(
      presentation.projection.displayToSourceOffset(
        1,
        affinity: FlarkV3InlineProjectionAffinity.downstream,
      ),
      inlineStart + 5,
      reason: 'affinity exposes the exact hidden strong closer boundary',
    );
    expect(
      source.replaceRange(insertionSource, insertionSource, '!'),
      '- a\n  > **b!** and _e_\n  ```\n  c\n  ```\n- d\n',
    );
    expect(document.toString(), source);
  });

  test('recursive quote projection survives singleton sidecar replacement', () {
    const source = '- a\n  > **b** and _e_\n  ```\n  c\n  ```\n- d\n';
    final document = FlarkV3SourceDocument.fromString(source);
    final version = FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(0x322, 2, 3, 4),
      document: document,
    );
    final point = source.indexOf('b');
    FlarkV3RecursiveGreenPointQuery decodeGreen({int ownerFrame = 5}) =>
        FlarkV3DocumentQueryDecoder.decodePointViewport(
              sourceDocument: document,
              expectedSource: version,
              expectedProfilePartition: 1,
              viewport: FlarkV3HostStructuralViewport.owned(
                sourceVersion: version,
                range: FlarkV3MetricRange(
                  start: FlarkV3SourceMetric(bytes: point, utf16: point),
                  end: FlarkV3SourceMetric(bytes: point + 1, utf16: point + 1),
                ),
                encoded: _schema9(point, ownerFrame: ownerFrame),
                receipt: FlarkV3HostViewportReceipt(
                  encodedBytes: 192,
                  leafCount: 1,
                  openDepth: 5,
                  treeNodesVisited: 2,
                  summaryNodesSkipped: 1,
                ),
              ),
            )
            as FlarkV3RecursiveGreenPointQuery;

    final green = decodeGreen();
    final paragraphStart = source.indexOf('  > ');
    final paragraphEnd = source.indexOf('\n', paragraphStart) + 1;
    final inlineStart = source.indexOf('**b**');
    final inlineEnd = paragraphEnd - 1;
    final paragraphSource = FlarkV3SourceSpan(
      startUtf8: paragraphStart,
      endUtf8: paragraphEnd,
      startUtf16: paragraphStart,
      endUtf16: paragraphEnd,
    );
    final quoteRecords = Uint8List(
      FlarkV3BlockQuoteProjectionDecoder.recordBytes,
    );
    ByteData.sublistView(quoteRecords)
      ..setUint32(0, 0, Endian.little)
      ..setUint32(4, paragraphEnd - paragraphStart, Endian.little)
      ..setUint32(8, 4, Endian.little)
      ..setUint32(12, paragraphEnd - paragraphStart - 5, Endian.little)
      ..setUint32(
        16,
        FlarkV3BlockQuoteProjectionDecoder.markedFlag,
        Endian.little,
      );
    FlarkV3HotInlineSidecarBinding quoteBinding({int ownerFrame = 4}) =>
        FlarkV3HotInlineSidecarBinding(
          parserProfile: FlarkV3SyntaxProfileId(1),
          refinementGeneration: FlarkV3ProtocolU64.fromU32(1),
          blockOrdinal: FlarkV3ProtocolU64(
            lowWord: ownerFrame,
            highWord: 0x80000000,
          ),
          physicalStartUtf8: paragraphStart,
          physicalEndUtf8: paragraphEnd,
          visibleStartUtf8: paragraphStart,
          visibleEndUtf8: paragraphEnd,
          physicalStartUtf16: paragraphStart,
          physicalEndUtf16: paragraphEnd,
          visibleStartUtf16: paragraphStart,
          visibleEndUtf16: paragraphEnd,
        );
    final quoteOutcome = FlarkV3InlineSidecarQueryAuthoritative(
      payloadKind: FlarkV3InlineSidecarPayloadKind.blockQuote,
      factCount: 1,
      valueEntryCount: 0,
      treeNodesVisited: 2,
      encodedFacts: quoteRecords,
      encodedValues: Uint8List(0),
    );
    final joinedQuote =
        FlarkV3DocumentQueryDecoder.joinRecursiveGreenBlockQuoteProjection(
          sourceDocument: document,
          expectedSource: version,
          expectedProfilePartition: 1,
          query: green,
          binding: quoteBinding(),
          outcome: quoteOutcome,
        );
    final projection = joinedQuote.blockQuoteProjection!;
    expect(joinedQuote.paragraphSource?.startUtf8, paragraphSource.startUtf8);
    expect(joinedQuote.paragraphSource?.endUtf8, paragraphSource.endUtf8);
    expect(joinedQuote.paragraphSource?.startUtf16, paragraphSource.startUtf16);
    expect(joinedQuote.paragraphSource?.endUtf16, paragraphSource.endUtf16);
    expect(projection.toSourceProjection().displayText, '**b** and _e_\n');
    expect(
      () => FlarkV3DocumentQueryDecoder.joinRecursiveGreenBlockQuoteProjection(
        sourceDocument: document,
        expectedSource: version,
        expectedProfilePartition: 1,
        query: green,
        binding: quoteBinding(ownerFrame: 3),
        outcome: quoteOutcome,
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
      reason: 'the quote payload is owned by its exact ancestor frame',
    );
    expect(
      () => FlarkV3DocumentQueryDecoder.joinRecursiveGreenBlockQuoteProjection(
        sourceDocument: document,
        expectedSource: version,
        expectedProfilePartition: 1,
        query: green,
        binding: quoteBinding(),
        outcome: FlarkV3InlineSidecarQueryAuthoritative(
          payloadKind: FlarkV3InlineSidecarPayloadKind.inline,
          factCount: 2,
          valueEntryCount: 0,
          treeNodesVisited: 3,
          encodedFacts: _inlineFacts(),
          encodedValues: Uint8List(0),
        ),
      ),
      throwsA(isA<FlarkV3DocumentQueryException>()),
      reason: 'typed payload lanes cannot be confused at the Dart boundary',
    );
    final authority = _ack(version, generation: 1);
    final cache = FlarkV3CurrentRevisionInlineCache(
      maximumEntries: 4,
      maximumFactRecords: 8,
    );

    final projectionStage = cache.resolveRecursiveGreen(
      authority: authority,
      query: joinedQuote,
    );
    expect(projectionStage.blockQuoteProjection, same(projection));
    expect(cache.recursiveBlockQuoteEntryCount, 1);

    FlarkV3HotInlineSidecarBinding bindingFrom(int physicalStart) =>
        FlarkV3HotInlineSidecarBinding(
          parserProfile: FlarkV3SyntaxProfileId(1),
          refinementGeneration: FlarkV3ProtocolU64.fromU32(2),
          blockOrdinal: FlarkV3ProtocolU64(lowWord: 5, highWord: 0x80000000),
          physicalStartUtf8: physicalStart,
          physicalEndUtf8: paragraphEnd,
          visibleStartUtf8: inlineStart,
          visibleEndUtf8: inlineEnd,
          physicalStartUtf16: physicalStart,
          physicalEndUtf16: paragraphEnd,
          visibleStartUtf16: inlineStart,
          visibleEndUtf16: inlineEnd,
        );
    final inlineOutcome = FlarkV3InlineSidecarQueryAuthoritative(
      payloadKind: FlarkV3InlineSidecarPayloadKind.inline,
      factCount: 2,
      valueEntryCount: 0,
      treeNodesVisited: 3,
      encodedFacts: _inlineFacts(),
      encodedValues: Uint8List(0),
    );
    FlarkV3RecursiveGreenPointQuery joinInline(int physicalStart) =>
        FlarkV3DocumentQueryDecoder.joinRecursiveGreenInline(
          sourceDocument: document,
          expectedSource: version,
          expectedProfilePartition: 1,
          query: green,
          binding: bindingFrom(physicalStart),
          outcome: inlineOutcome,
        );

    final mismatchedRange = cache.resolveRecursiveGreen(
      authority: authority,
      query: joinInline(paragraphStart + 2),
    );
    expect(
      mismatchedRange.blockQuoteProjection,
      isNull,
      reason: 'the same owner frame cannot join a different physical range',
    );

    final inlineStage = joinInline(paragraphStart);
    expect(inlineStage.blockQuoteProjection, isNull);

    final combined = cache.resolveRecursiveGreen(
      authority: authority,
      query: inlineStage,
    );
    expect(combined.blockQuoteProjection, same(projection));
    expect(combined.inlineFacts, same(inlineStage.inlineFacts));
    expect(combined.paragraphSource, same(inlineStage.paragraphSource));

    final afterSidecarMoved = cache.resolveRecursiveGreen(
      authority: authority,
      query: green,
    );
    expect(afterSidecarMoved.blockQuoteProjection, same(projection));
    expect(afterSidecarMoved.inlineFacts, same(inlineStage.inlineFacts));

    final neighboringOwner = cache.resolveRecursiveGreen(
      authority: authority,
      query: decodeGreen(ownerFrame: 6),
    );
    expect(neighboringOwner.blockQuoteProjection, isNull);
    expect(neighboringOwner.inlineFacts, isNull);

    final afterAckReplacement = cache.resolveRecursiveGreen(
      authority: _ack(version, generation: 2),
      query: green,
    );
    expect(afterAckReplacement.blockQuoteProjection, isNull);
    expect(afterAckReplacement.inlineFacts, isNull);
    expect(cache.recursiveBlockQuoteEntryCount, 0);
    expect(cache.entryCount, 0);
  });
}

FlarkV3StructuralAck _ack(
  FlarkV3SourceVersion sourceVersion, {
  required int generation,
}) => FlarkV3StructuralAck(
  publicationSession: FlarkV3PublicationSessionId(generation, 20, 30, 40),
  hostRevision: FlarkV3HostRevisionId(generation),
  sourceVersion: sourceVersion,
  sourceRoot: FlarkV3SourceRootId(generation, 1),
  parseGeneration: generation,
  grammarRevision: flarkV3CurrentGrammarRevision,
  syntaxProfile: FlarkV3SyntaxProfileId(1),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 1,
  sequenceDigest: FlarkV3ProtocolDigest128(generation, 2, 3, 4),
  manifestDigest: FlarkV3ProtocolDigest128(generation, 5, 6, 7),
);

Uint8List _inlineFacts() {
  final bytes = Uint8List(40);
  final data = ByteData.sublistView(bytes);
  void record(
    int offset, {
    required int kind,
    required int start,
    required int length,
    required int contentStart,
    required int contentLength,
  }) {
    data
      ..setUint8(offset, kind)
      ..setUint8(offset + 1, 0)
      ..setUint16(offset + 2, 0, Endian.little)
      ..setUint32(offset + 4, start, Endian.little)
      ..setUint32(offset + 8, length, Endian.little)
      ..setUint32(offset + 12, contentStart, Endian.little)
      ..setUint32(offset + 16, contentLength, Endian.little);
  }

  record(0, kind: 2, start: 0, length: 5, contentStart: 2, contentLength: 1);
  record(20, kind: 1, start: 10, length: 3, contentStart: 11, contentLength: 1);
  return bytes;
}

Uint8List _schema9(int point, {int ownerFrame = 5}) {
  const header = 112;
  const record = 16;
  const kinds = [1, 3, 4, 2, 5];
  final bytes = Uint8List(header + kinds.length * record);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKVP001'));
  data
    ..setUint32(8, 9, Endian.little)
    ..setUint32(12, header, Endian.little)
    ..setUint32(16, record, Endian.little)
    ..setUint32(20, 1, Endian.little)
    ..setUint32(24, 1, Endian.little)
    ..setUint32(28, 1, Endian.little)
    ..setUint32(32, 0, Endian.little)
    ..setUint32(36, kinds.length, Endian.little)
    ..setUint32(40, kinds.length - 1, Endian.little)
    ..setUint16(44, 5, Endian.little)
    ..setUint8(46, 1)
    ..setUint8(47, 1)
    ..setUint32(48, point, Endian.little)
    ..setUint32(52, point + 1, Endian.little)
    ..setUint32(56, point, Endian.little)
    ..setUint32(60, point + 1, Endian.little)
    ..setUint32(64, 1, Endian.little)
    ..setUint32(68, 1, Endian.little)
    ..setUint32(72, 1, Endian.little)
    ..setUint32(76, 1, Endian.little)
    ..setUint32(80, point, Endian.little)
    ..setUint32(84, point, Endian.little)
    ..setUint32(88, 1, Endian.little)
    ..setUint32(92, 0, Endian.little)
    ..setUint32(96, 0, Endian.little)
    ..setUint32(100, 30, Endian.little)
    ..setUint32(104, 1, Endian.little)
    ..setUint32(108, 5, Endian.little);
  for (var index = 0; index < kinds.length; index += 1) {
    final offset = header + index * record;
    data
      ..setUint32(
        offset,
        index == kinds.length - 1 ? ownerFrame : index + 1,
        Endian.little,
      )
      ..setUint32(offset + 4, 0, Endian.little)
      ..setUint16(offset + 8, kinds[index], Endian.little)
      ..setUint16(offset + 10, index == kinds.length - 1 ? 1 : 0, Endian.little)
      ..setUint32(offset + 12, 0, Endian.little);
  }
  return bytes;
}

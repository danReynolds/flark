import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_query.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_viewport_page_materializer.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_visible_block_set.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3ViewportPageMaterializer', () {
    test('joins exact ordinals and marker-free inline facts without inventing '
        'missing payloads', () {
      final document = FlarkV3SourceDocument.fromString('**b**\nplain');
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final blocks = [
        _paragraph(ordinal: 0, source: _span(0, 6), visible: _span(0, 5)),
        _paragraph(ordinal: 1, source: _span(6, 11), visible: _span(6, 11)),
      ];
      final fixture = _page(
        baseAck: ack,
        blocks: blocks,
        children: [
          _Child(
            block: blocks.first,
            visible: _span(0, 5),
            kind: FlarkV3ViewportPresentationPayloadKind.inline,
            recordCount: 1,
            payload: _inlineStrongRecord(),
          ),
        ],
      );

      final result = const FlarkV3ViewportPageMaterializer().materialize(
        sourceDocument: document,
        currentStructuralAck: ack,
        currentStructureGeneration: 9,
        visibleBlocks: _visibleSet(
          source: source,
          blocks: blocks,
          structureGeneration: 9,
        ),
        page: fixture,
      );

      expect(result, isA<FlarkV3ExactViewportPageMaterialization>());
      final exact = result as FlarkV3ExactViewportPageMaterialization;
      expect(exact.identity.sourceVersion, source);
      expect(exact.identity.sourceRoot, ack.sourceRoot);
      expect(exact.identity.parseGeneration, ack.parseGeneration);
      expect(exact.identity.structureGeneration, 9);
      expect(exact.identity.viewportGeneration, 12);
      expect(exact.blocks, hasLength(2));

      final inline = exact.blocks.first as FlarkV3InlineViewportBlock;
      expect(inline.ordinal, 0);
      expect(inline.displayText, 'b');
      expect(inline.displayRuns, hasLength(1));
      expect(inline.displayRuns.single.semanticStyles, [
        FlarkV3InlineFactKind.strong,
      ]);

      final absent = exact.blocks.last as FlarkV3SourceFallbackViewportBlock;
      expect(absent.reason, FlarkV3ViewportBlockFallbackReason.payloadAbsent);
      expect(absent.ordinal, 1);
      expect(exact.containsSourceFallback, isTrue);
    });

    test(
      'materializes escaped punctuation marker-free and rejects malformed geometry',
      () {
        final document = FlarkV3SourceDocument.fromString(r'\*');
        final source = _sourceVersion(document);
        final ack = _structuralAck(source);
        final block = _paragraph(
          ordinal: 0,
          source: _span(0, 2),
          visible: _span(0, 2),
        );
        final visible = _visibleSet(
          source: source,
          blocks: [block],
          structureGeneration: 1,
        );
        const materializer = FlarkV3ViewportPageMaterializer();

        FlarkV3ViewportPageMaterialization materialize(Uint8List payload) =>
            materializer.materialize(
              sourceDocument: document,
              currentStructuralAck: ack,
              currentStructureGeneration: 1,
              visibleBlocks: visible,
              page: _page(
                baseAck: ack,
                blocks: [block],
                children: [
                  _Child(
                    block: block,
                    visible: block.structure.visibleSource,
                    kind: FlarkV3ViewportPresentationPayloadKind.inline,
                    recordCount: 1,
                    payload: payload,
                  ),
                ],
              ),
            );

        final exact =
            materialize(_inlineEscapedPunctuationRecord())
                as FlarkV3ExactViewportPageMaterialization;
        final inline = exact.blocks.single as FlarkV3InlineViewportBlock;
        expect(inline.displayText, '*');
        expect(
          inline.facts.facts.single.kind,
          FlarkV3InlineFactKind.escapedPunctuation,
        );
        expect(inline.displayRuns, hasLength(1));
        expect(inline.displayRuns.single.semanticStyles, isEmpty);
        expect(exact.containsSourceFallback, isFalse);

        final malformed =
            materialize(_inlineEscapedPunctuationRecord(contentLength: 0))
                as FlarkV3ExactViewportPageMaterialization;
        final fallback =
            malformed.blocks.single as FlarkV3SourceFallbackViewportBlock;
        expect(
          fallback.reason,
          FlarkV3ViewportBlockFallbackReason.payloadRejected,
        );
      },
    );

    test(
      'materializes a certified character reference as styled passive text',
      () {
        const sourceText = '*&amp;*';
        final document = FlarkV3SourceDocument.fromString(sourceText);
        final source = _sourceVersion(document);
        final ack = _structuralAck(source);
        final block = _paragraph(
          ordinal: 0,
          source: _span(0, sourceText.length),
          visible: _span(0, sourceText.length),
        );
        final payload = Uint8List.fromList([
          ..._inlineEmphasisAroundEntityRecord(),
          ..._inlineCharacterReferenceRecord(),
        ]);

        final result = const FlarkV3ViewportPageMaterializer().materialize(
          sourceDocument: document,
          currentStructuralAck: ack,
          currentStructureGeneration: 1,
          visibleBlocks: _visibleSet(
            source: source,
            blocks: [block],
            structureGeneration: 1,
          ),
          page: _page(
            baseAck: ack,
            blocks: [block],
            children: [
              _Child(
                block: block,
                visible: block.structure.visibleSource,
                kind: FlarkV3ViewportPresentationPayloadKind.inline,
                recordCount: 2,
                payload: payload,
              ),
            ],
          ),
        );

        final exact = result as FlarkV3ExactViewportPageMaterialization;
        final inline = exact.blocks.single as FlarkV3InlineViewportBlock;
        expect(inline.displayText, '&');
        expect(inline.displayRuns, hasLength(1));
        expect(inline.displayRuns.single.text, '&');
        expect(inline.displayRuns.single.semanticStyles, [
          FlarkV3InlineFactKind.emphasis,
        ]);
        expect(
          inline.displayRuns.single.semanticFacts.map((fact) => fact.kind),
          [
            FlarkV3InlineFactKind.emphasis,
            FlarkV3InlineFactKind.characterReference,
          ],
        );
      },
    );

    test(
      'materializes cooked URI entity labels with passive link authority',
      () {
        const sourceText = '<https://e.test/?q=&amp;>';
        const entity = '&amp;';
        const cookedTarget = 'https://e.test/?q=&';
        final entityStart = sourceText.indexOf(entity);
        final document = FlarkV3SourceDocument.fromString(sourceText);
        final source = _sourceVersion(document);
        final ack = _structuralAck(source);
        final block = _paragraph(
          ordinal: 0,
          source: _span(0, sourceText.length),
          visible: _span(0, sourceText.length),
        );
        final payload = Uint8List.fromList([
          ..._inlineUriRecord(
            sourceLength: sourceText.length,
            contentLength: sourceText.length - 2,
          ),
          ..._inlineCharacterReferenceRecord(start: entityStart),
        ]);

        final result = const FlarkV3ViewportPageMaterializer().materialize(
          sourceDocument: document,
          currentStructuralAck: ack,
          currentStructureGeneration: 1,
          visibleBlocks: _visibleSet(
            source: source,
            blocks: [block],
            structureGeneration: 1,
          ),
          page: _page(
            baseAck: ack,
            blocks: [block],
            children: [
              _Child(
                block: block,
                visible: block.structure.visibleSource,
                kind: FlarkV3ViewportPresentationPayloadKind.inline,
                recordCount: 2,
                payload: payload,
              ),
            ],
          ),
        );

        final exact = result as FlarkV3ExactViewportPageMaterialization;
        final inline = exact.blocks.single as FlarkV3InlineViewportBlock;
        expect(inline.displayText, cookedTarget);
        expect(inline.displayRuns.map((run) => run.text), [
          'https://e.test/?q=',
          '&',
        ]);
        expect(
          inline.displayRuns.map((run) => run.linkAnnotation?.destination),
          everyElement(cookedTarget),
        );
        expect(inline.displayRuns.last.semanticStyles, isEmpty);
      },
    );

    test(
      'materializes a marker-free direct link from the joined value lane',
      () {
        const sourceText = '[x](&bsol;*)';
        final document = FlarkV3SourceDocument.fromString(sourceText);
        final source = _sourceVersion(document);
        final ack = _structuralAck(source);
        final block = _paragraph(
          ordinal: 0,
          source: _span(0, sourceText.length),
          visible: _span(0, sourceText.length),
        );
        final payload = Uint8List.fromList([
          ..._inlineDirectLinkRecord(),
          ..._inlineDirectLinkValues(),
        ]);

        final result = const FlarkV3ViewportPageMaterializer().materialize(
          sourceDocument: document,
          currentStructuralAck: ack,
          currentStructureGeneration: 1,
          visibleBlocks: _visibleSet(
            source: source,
            blocks: [block],
            structureGeneration: 1,
          ),
          page: _page(
            baseAck: ack,
            blocks: [block],
            children: [
              _Child(
                block: block,
                visible: block.structure.visibleSource,
                kind: FlarkV3ViewportPresentationPayloadKind.inline,
                recordCount: 1,
                payload: payload,
              ),
            ],
          ),
        );

        final exact = result as FlarkV3ExactViewportPageMaterialization;
        final inline = exact.blocks.single as FlarkV3InlineViewportBlock;
        expect(inline.displayText, 'x');
        expect(inline.displayRuns, hasLength(1));
        expect(inline.displayRuns.single.linkAnnotation?.destination, '*');
        expect(
          inline.displayRuns.single.linkAnnotation?.targetRecipe,
          FlarkV3InlineLinkTargetRecipe.companionCookedValue,
        );
      },
    );

    test('decodes indented-code records through the existing decoder', () {
      final document = FlarkV3SourceDocument.fromString('    code\n');
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final block = _indentedCode(ordinal: 4, source: _span(0, 9));
      final fixture = _page(
        baseAck: ack,
        blocks: [block],
        children: [
          _Child(
            block: block,
            visible: block.structure.source,
            kind: FlarkV3ViewportPresentationPayloadKind.indentedCode,
            recordCount: 1,
            payload: _lineRecord(
              physicalLength: 9,
              hiddenPrefixLength: 4,
              contentLength: 4,
            ),
          ),
        ],
      );

      final result = const FlarkV3ViewportPageMaterializer().materialize(
        sourceDocument: document,
        currentStructuralAck: ack,
        currentStructureGeneration: 2,
        visibleBlocks: _visibleSet(
          source: source,
          blocks: [block],
          structureGeneration: 2,
        ),
        page: fixture,
      );

      final exact = result as FlarkV3ExactViewportPageMaterialization;
      final code = exact.blocks.single as FlarkV3IndentedCodeViewportBlock;
      expect(code.displayText, 'code\n');
      expect(code.payload.records, hasLength(1));
      expect(code.projection.isCertified, isTrue);
    });

    test('joins mixed paragraph heading and structural fenced-code body', () {
      final document = FlarkV3SourceDocument.fromString(
        '**b**\n# *h*\n```dart\ncode\n```\n',
      );
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final paragraph = _paragraph(
        ordinal: 0,
        source: _span(0, 6),
        visible: _span(0, 5),
      );
      final heading = _heading(
        ordinal: 1,
        source: _span(6, 12),
        content: _span(8, 11),
      );
      final fence = _fencedCode(
        ordinal: 2,
        source: _span(12, 29),
        openingMarker: _span(12, 15),
        rawInfo: _span(15, 19),
        body: _span(20, 25),
        closingMarker: _span(25, 28),
      );
      final blocks = [paragraph, heading, fence];
      final page = _page(
        baseAck: ack,
        blocks: blocks,
        children: [
          _Child(
            block: paragraph,
            visible: paragraph.structure.visibleSource,
            kind: FlarkV3ViewportPresentationPayloadKind.inline,
            recordCount: 1,
            payload: _inlineStrongRecord(),
          ),
          _Child(
            block: heading,
            visible: heading.structure.visibleSource,
            kind: FlarkV3ViewportPresentationPayloadKind.inline,
            recordCount: 1,
            payload: _inlineEmphasisRecord(),
          ),
        ],
      );

      final result = const FlarkV3ViewportPageMaterializer().materialize(
        sourceDocument: document,
        currentStructuralAck: ack,
        currentStructureGeneration: 10,
        visibleBlocks: _visibleSet(
          source: source,
          blocks: blocks,
          structureGeneration: 10,
        ),
        page: page,
      );

      final exact = result as FlarkV3ExactViewportPageMaterialization;
      expect(exact.blocks.map((block) => block.ordinal), [0, 1, 2]);
      expect((exact.blocks[0] as FlarkV3InlineViewportBlock).displayText, 'b');
      final materializedHeading = exact.blocks[1] as FlarkV3InlineViewportBlock;
      expect(materializedHeading.headingLevel, 1);
      expect(materializedHeading.displayText, 'h');
      final materializedFence =
          exact.blocks[2] as FlarkV3FencedCodeViewportBlock;
      expect(materializedFence.bodySource, same(fence.structure.visibleSource));
      expect(materializedFence.displayText, 'code\n');
      expect(exact.containsSourceFallback, isFalse);
    });

    test('accepts exact blank boundaries as atomic empty blocks', () {
      final document = FlarkV3SourceDocument.fromString('a\n\nb');
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final blocks = [
        _paragraph(ordinal: 0, source: _span(0, 2), visible: _span(0, 1)),
        _blankBoundary(ordinal: 1, source: _span(2, 3)),
        _paragraph(ordinal: 2, source: _span(3, 4), visible: _span(3, 4)),
      ];
      final page = _page(baseAck: ack, blocks: blocks, children: const []);

      final result = const FlarkV3ViewportPageMaterializer().materialize(
        sourceDocument: document,
        currentStructuralAck: ack,
        currentStructureGeneration: 3,
        visibleBlocks: _visibleSet(
          source: source,
          blocks: blocks,
          structureGeneration: 3,
        ),
        page: page,
      );

      final exact = result as FlarkV3ExactViewportPageMaterialization;
      expect(exact.blocks.map((block) => block.ordinal), [0, 1, 2]);
      expect(exact.blocks[1], isA<FlarkV3AtomicViewportBlock>());
      expect(
        (exact.blocks[1] as FlarkV3AtomicViewportBlock).displayText,
        isEmpty,
      );
    });

    test('rejects invalid inline records as whole-block source fallback', () {
      final document = FlarkV3SourceDocument.fromString('**b**');
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final block = _paragraph(
        ordinal: 0,
        source: _span(0, 5),
        visible: _span(0, 5),
      );
      final invalidRecord = _inlineStrongRecord()..[0] = 99;
      final page = _page(
        baseAck: ack,
        blocks: [block],
        children: [
          _Child(
            block: block,
            visible: block.structure.visibleSource,
            kind: FlarkV3ViewportPresentationPayloadKind.inline,
            recordCount: 1,
            payload: invalidRecord,
          ),
        ],
      );

      final result = const FlarkV3ViewportPageMaterializer().materialize(
        sourceDocument: document,
        currentStructuralAck: ack,
        currentStructureGeneration: 1,
        visibleBlocks: _visibleSet(
          source: source,
          blocks: [block],
          structureGeneration: 1,
        ),
        page: page,
      );

      final fallback =
          (result as FlarkV3ExactViewportPageMaterialization).blocks.single
              as FlarkV3SourceFallbackViewportBlock;
      expect(
        fallback.reason,
        FlarkV3ViewportBlockFallbackReason.payloadRejected,
      );
    });

    test('preserves the exact parser unsupported reason', () {
      final document = FlarkV3SourceDocument.fromString('plain');
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final block = _paragraph(
        ordinal: 0,
        source: _span(0, 5),
        visible: _span(0, 5),
      );
      final page = _page(
        baseAck: ack,
        blocks: [block],
        children: [
          _Child(
            block: block,
            visible: block.structure.visibleSource,
            kind: FlarkV3ViewportPresentationPayloadKind.unsupported,
            recordCount: 0,
            payload: Uint8List(12),
            unsupportedReason: 91,
          ),
        ],
      );

      final result = const FlarkV3ViewportPageMaterializer().materialize(
        sourceDocument: document,
        currentStructuralAck: ack,
        currentStructureGeneration: 1,
        visibleBlocks: _visibleSet(
          source: source,
          blocks: [block],
          structureGeneration: 1,
        ),
        page: page,
      );

      final fallback =
          (result as FlarkV3ExactViewportPageMaterialization).blocks.single
              as FlarkV3SourceFallbackViewportBlock;
      expect(
        fallback.reason,
        FlarkV3ViewportBlockFallbackReason.parserUnsupported,
      );
      expect(fallback.unsupportedReason, 91);
    });

    test(
      'does not synthesize the point path missing from schema 8 list payloads',
      () {
        final document = FlarkV3SourceDocument.fromString('- item\n');
        final source = _sourceVersion(document);
        final ack = _structuralAck(source);
        final block = _bulletList(ordinal: 3, source: _span(0, 7));
        final page = _page(
          baseAck: ack,
          blocks: [block],
          children: [
            _Child(
              block: block,
              visible: block.structure.source,
              kind: FlarkV3ViewportPresentationPayloadKind.bulletList,
              recordCount: 1,
              payload: Uint8List(28),
            ),
          ],
        );

        final result = const FlarkV3ViewportPageMaterializer().materialize(
          sourceDocument: document,
          currentStructuralAck: ack,
          currentStructureGeneration: 5,
          visibleBlocks: _visibleSet(
            source: source,
            blocks: [block],
            structureGeneration: 5,
          ),
          page: page,
        );

        final fallback =
            (result as FlarkV3ExactViewportPageMaterialization).blocks.single
                as FlarkV3SourceFallbackViewportBlock;
        expect(
          fallback.reason,
          FlarkV3ViewportBlockFallbackReason.pointPathAuthorityAbsent,
        );
      },
    );

    test('fails the complete page closed across current-authority seams', () {
      final document = FlarkV3SourceDocument.fromString('plain');
      final source = _sourceVersion(document);
      final ack = _structuralAck(source);
      final block = _paragraph(
        ordinal: 7,
        source: _span(0, 5),
        visible: _span(0, 5),
      );
      final page = _page(baseAck: ack, blocks: [block], children: const []);
      final exactVisible = _visibleSet(
        source: source,
        blocks: [block],
        structureGeneration: 8,
      );
      final materializer = const FlarkV3ViewportPageMaterializer();

      final differentAck = _structuralAck(source, publicationSeed: 100);
      expect(
        (materializer.materialize(
                  sourceDocument: document,
                  currentStructuralAck: differentAck,
                  currentStructureGeneration: 8,
                  visibleBlocks: exactVisible,
                  page: page,
                )
                as FlarkV3SourceFallbackViewportPage)
            .reason,
        FlarkV3ViewportPageFallbackReason.structuralBaseChanged,
      );
      expect(
        (materializer.materialize(
                  sourceDocument: document,
                  currentStructuralAck: ack,
                  currentStructureGeneration: 9,
                  visibleBlocks: exactVisible,
                  page: page,
                )
                as FlarkV3SourceFallbackViewportPage)
            .reason,
        FlarkV3ViewportPageFallbackReason.structuralGenerationChanged,
      );

      final otherDocument = FlarkV3SourceDocument.fromString('other');
      expect(
        (materializer.materialize(
                  sourceDocument: otherDocument,
                  currentStructuralAck: ack,
                  currentStructureGeneration: 8,
                  visibleBlocks: exactVisible,
                  page: page,
                )
                as FlarkV3SourceFallbackViewportPage)
            .reason,
        FlarkV3ViewportPageFallbackReason.sourceAuthorityChanged,
      );

      final wrongOrdinalBlock = _paragraph(
        ordinal: 8,
        source: _span(0, 5),
        visible: _span(0, 5),
      );
      expect(
        (materializer.materialize(
                  sourceDocument: document,
                  currentStructuralAck: ack,
                  currentStructureGeneration: 8,
                  visibleBlocks: _visibleSet(
                    source: source,
                    blocks: [wrongOrdinalBlock],
                    structureGeneration: 8,
                  ),
                  page: page,
                )
                as FlarkV3SourceFallbackViewportPage)
            .reason,
        FlarkV3ViewportPageFallbackReason.structuralCoverageUnavailable,
      );

      final mismatchedEntryBlock = _paragraph(
        ordinal: 7,
        source: _span(0, 4),
        visible: _span(0, 4),
      );
      final mismatchedEntryPage = _page(
        baseAck: ack,
        blocks: [block],
        children: [
          _Child(
            block: mismatchedEntryBlock,
            visible: mismatchedEntryBlock.structure.visibleSource,
            kind: FlarkV3ViewportPresentationPayloadKind.inline,
            recordCount: 0,
            payload: Uint8List(0),
          ),
        ],
      );
      expect(
        (materializer.materialize(
                  sourceDocument: document,
                  currentStructuralAck: ack,
                  currentStructureGeneration: 8,
                  visibleBlocks: exactVisible,
                  page: mismatchedEntryPage,
                )
                as FlarkV3SourceFallbackViewportPage)
            .reason,
        FlarkV3ViewportPageFallbackReason.entryStructureMismatch,
      );
    });
  });
}

final class _Child {
  const _Child({
    required this.block,
    required this.visible,
    required this.kind,
    required this.recordCount,
    required this.payload,
    this.unsupportedReason = 0,
  });

  final FlarkV3DocumentStructuralBlock block;
  final FlarkV3SourceSpan visible;
  final FlarkV3ViewportPresentationPayloadKind kind;
  final int recordCount;
  final Uint8List payload;
  final int unsupportedReason;
}

FlarkV3ViewportPresentationAggregatePage _page({
  required FlarkV3StructuralAck baseAck,
  required List<FlarkV3DocumentStructuralBlock> blocks,
  required List<_Child> children,
}) {
  final first = blocks.first;
  final last = blocks.last;
  final requested = FlarkV3ViewportPresentationMetricRange(
    startUtf8: first.structure.source.startUtf8,
    startUtf16: first.structure.source.startUtf16,
    endUtf8: last.structure.source.endUtf8,
    endUtf16: last.structure.source.endUtf16,
  );
  final binding = FlarkV3ViewportPresentationBinding(
    viewportGeneration: 12,
    requestedRange: requested,
    coveredRange: requested,
    start: FlarkV3ViewportPresentationVisitStart(
      blockOrdinal: FlarkV3ProtocolU64.fromU32(first.ordinal),
      utf8Offset: requested.startUtf8,
      utf16Offset: requested.startUtf16,
    ),
    next: FlarkV3ViewportPresentationVisitStart(
      blockOrdinal: FlarkV3ProtocolU64.fromU32(last.ordinal + 1),
      utf8Offset: requested.endUtf8,
      utf16Offset: requested.endUtf16,
    ),
    complete: true,
  );
  final envelope = FlarkV3ViewportPresentationEnvelopeMetrics(
    visitedStructuralEntries: blocks.length,
    visitedStoragePages: 1,
    orderedLeafCount: children.length,
    inlineSourceBytes: children.fold(
      0,
      (sum, child) => sum + child.visible.endUtf8 - child.visible.startUtf8,
    ),
    factCount: children.fold(0, (sum, child) => sum + child.recordCount),
    transferredNodeCount: children.length,
    parserTransitions: children.length + 1,
    aggregateEnvelopeDigest256: _digest256(50),
  );
  final ack = FlarkV3ViewportPresentationAck(
    publicationSession: FlarkV3PublicationSessionId(31, 32, 33, 34),
    baseAck: baseAck,
    binding: binding,
    envelope: envelope,
    actualFrameCount: children.length * 3 + 3,
    actualEncodedFrameBytes: 512,
    aggregateRootStreamDigest: _digest128(70),
  );

  const headerBytes = FlarkV3ViewportPresentationAggregatePage.headerBytes;
  const entryBytes =
      FlarkV3ViewportPresentationAggregatePage.directoryEntryBytes;
  final payloadStart = headerBytes + children.length * entryBytes;
  final payloadBytes = children.fold(
    0,
    (sum, child) => sum + child.payload.lengthInBytes,
  );
  final bytes = Uint8List(payloadStart + payloadBytes);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(
    0,
    FlarkV3ViewportPresentationAggregatePage.magicBytes.length,
    FlarkV3ViewportPresentationAggregatePage.magicBytes,
  );
  data
    ..setUint32(
      8,
      FlarkV3ViewportPresentationAggregatePage.schema,
      Endian.little,
    )
    ..setUint32(12, headerBytes, Endian.little)
    ..setUint32(16, entryBytes, Endian.little)
    ..setUint32(20, children.length, Endian.little)
    ..setUint32(24, payloadStart, Endian.little)
    ..setUint32(28, bytes.lengthInBytes, Endian.little);
  _writeId(data, 32, ack.publicationSession);
  _writeId(data, 48, baseAck.publicationSession);
  data
    ..setUint32(64, binding.viewportGeneration, Endian.little)
    ..setUint32(68, 1, Endian.little);
  _writeRange(data, 72, binding.requestedRange);
  _writeRange(data, 88, binding.coveredRange);
  data
    ..setUint32(104, ack.actualFrameCount, Endian.little)
    ..setUint32(108, ack.actualEncodedFrameBytes, Endian.little);
  _writeId(data, 112, ack.aggregateRootStreamDigest);
  for (var index = 0; index < 8; index += 1) {
    data.setUint32(128 + index * 4, 0xa0 + index, Endian.little);
  }

  var payloadOffset = payloadStart;
  for (var index = 0; index < children.length; index += 1) {
    final child = children[index];
    final offset = headerBytes + index * entryBytes;
    final unsupported =
        child.kind == FlarkV3ViewportPresentationPayloadKind.unsupported;
    data
      ..setUint32(offset, index, Endian.little)
      ..setUint32(offset + 4, baseAck.sourceVersion.revision, Endian.little);
    _writeId(data, offset + 8, baseAck.sourceVersion.documentSession);
    data
      ..setUint32(offset + 24, baseAck.sourceRoot.highWord, Endian.little)
      ..setUint32(offset + 28, baseAck.sourceRoot.lowWord, Endian.little)
      ..setUint32(
        offset + 32,
        baseAck.sourceVersion.contentHash.word0,
        Endian.little,
      )
      ..setUint32(
        offset + 36,
        baseAck.sourceVersion.contentHash.word1,
        Endian.little,
      )
      ..setUint32(
        offset + 40,
        baseAck.sourceVersion.contentHash.word2,
        Endian.little,
      )
      ..setUint32(
        offset + 44,
        baseAck.sourceVersion.contentHash.word3,
        Endian.little,
      )
      ..setUint32(
        offset + 48,
        baseAck.sourceVersion.metric.bytes,
        Endian.little,
      )
      ..setUint32(
        offset + 52,
        baseAck.sourceVersion.metric.utf16,
        Endian.little,
      )
      ..setUint32(offset + 56, baseAck.parseGeneration, Endian.little)
      ..setUint32(offset + 64, baseAck.syntaxProfile.value, Endian.little)
      ..setUint32(offset + 72, binding.viewportGeneration, Endian.little)
      ..setUint32(offset + 80, child.block.ordinal, Endian.little)
      ..setUint32(
        offset + 88,
        child.block.structure.source.startUtf8,
        Endian.little,
      )
      ..setUint32(
        offset + 92,
        child.block.structure.source.endUtf8,
        Endian.little,
      )
      ..setUint32(offset + 96, child.visible.startUtf8, Endian.little)
      ..setUint32(offset + 100, child.visible.endUtf8, Endian.little)
      ..setUint32(
        offset + 104,
        child.block.structure.source.startUtf16,
        Endian.little,
      )
      ..setUint32(
        offset + 108,
        child.block.structure.source.endUtf16,
        Endian.little,
      )
      ..setUint32(offset + 112, child.visible.startUtf16, Endian.little)
      ..setUint32(offset + 116, child.visible.endUtf16, Endian.little)
      ..setUint8(offset + 120, _payloadKindCode(child.kind))
      ..setUint8(offset + 121, unsupported ? 2 : 1)
      ..setUint32(offset + 124, child.recordCount, Endian.little)
      ..setUint32(offset + 128, payloadOffset, Endian.little)
      ..setUint32(offset + 132, child.payload.lengthInBytes, Endian.little)
      ..setUint32(offset + 136, child.unsupportedReason, Endian.little);
    bytes.setRange(
      payloadOffset,
      payloadOffset + child.payload.lengthInBytes,
      child.payload,
    );
    payloadOffset += child.payload.lengthInBytes;
  }
  return FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
    ack: ack,
    encodedPage: bytes,
  );
}

FlarkV3ExactVisibleBlockSet _visibleSet({
  required FlarkV3SourceVersion source,
  required List<FlarkV3DocumentStructuralBlock> blocks,
  required int structureGeneration,
}) {
  final first = blocks.first.structure.source;
  final last = blocks.last.structure.source;
  return FlarkV3ExactVisibleBlockSet(
    demand: FlarkV3VisibleBlockDemand(
      sourceRevision: source.revision,
      structureGeneration: structureGeneration,
      startUtf16: first.startUtf16,
      endUtf16: last.endUtf16,
    ),
    coveredSource: FlarkV3SourceSpan(
      startUtf8: first.startUtf8,
      endUtf8: last.endUtf8,
      startUtf16: first.startUtf16,
      endUtf16: last.endUtf16,
    ),
    blocks: blocks,
    demandCovered: true,
    truncated: false,
  );
}

FlarkV3DocumentStructuralBlock _paragraph({
  required int ordinal,
  required FlarkV3SourceSpan source,
  required FlarkV3SourceSpan visible,
}) => FlarkV3DocumentStructuralBlock(
  ordinal: ordinal,
  structure: FlarkV3DocumentStructure(
    kind: FlarkV3DocumentStructureKind.paragraph,
    source: source,
    visibleSource: visible,
    referenceDefinitionCount: 0,
  ),
  projection: FlarkV3DocumentProjection(
    kind: FlarkV3DocumentStructureKind.paragraph,
    source: source,
    projectedSource: visible,
    runCount: 1,
  ),
);

FlarkV3DocumentStructuralBlock _blankBoundary({
  required int ordinal,
  required FlarkV3SourceSpan source,
}) {
  final visible = FlarkV3SourceSpan(
    startUtf8: source.startUtf8,
    endUtf8: source.startUtf8,
    startUtf16: source.startUtf16,
    endUtf16: source.startUtf16,
  );
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.unknown,
      source: source,
      visibleSource: visible,
      referenceDefinitionCount: 0,
      unknownReason: FlarkV3DocumentUnknownReason.blankBoundary,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.unknown,
      source: source,
      projectedSource: source,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralBlock _indentedCode({
  required int ordinal,
  required FlarkV3SourceSpan source,
}) {
  const hidden = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: 0,
    startUtf16: 0,
    endUtf16: 0,
  );
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.indentedCode,
      source: source,
      visibleSource: hidden,
      referenceDefinitionCount: 0,
      indentedCode: const FlarkV3IndentedCodeFacts(
        deindentColumns: 4,
        hasBofBom: false,
        lineCount: 1,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
        terminalLineEndingBytes: 1,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.indentedCode,
      source: source,
      projectedSource: hidden,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralBlock _heading({
  required int ordinal,
  required FlarkV3SourceSpan source,
  required FlarkV3SourceSpan content,
}) => FlarkV3DocumentStructuralBlock(
  ordinal: ordinal,
  structure: FlarkV3DocumentStructure(
    kind: FlarkV3DocumentStructureKind.heading,
    source: source,
    visibleSource: content,
    referenceDefinitionCount: 0,
    heading: FlarkV3AtxHeadingFacts(
      level: 1,
      contentSource: content,
      openingMarker: FlarkV3SourceSpan(
        startUtf8: source.startUtf8,
        endUtf8: source.startUtf8 + 1,
        startUtf16: source.startUtf16,
        endUtf16: source.startUtf16 + 1,
      ),
      closingMarker: null,
    ),
  ),
  projection: FlarkV3DocumentProjection(
    kind: FlarkV3DocumentStructureKind.heading,
    source: source,
    projectedSource: content,
    runCount: 1,
  ),
);

FlarkV3DocumentStructuralBlock _fencedCode({
  required int ordinal,
  required FlarkV3SourceSpan source,
  required FlarkV3SourceSpan openingMarker,
  required FlarkV3SourceSpan rawInfo,
  required FlarkV3SourceSpan body,
  required FlarkV3SourceSpan closingMarker,
}) => FlarkV3DocumentStructuralBlock(
  ordinal: ordinal,
  structure: FlarkV3DocumentStructure(
    kind: FlarkV3DocumentStructureKind.fencedCode,
    source: source,
    visibleSource: body,
    referenceDefinitionCount: 0,
    fencedCode: FlarkV3FencedCodeFacts(
      marker: FlarkV3CodeFenceMarker.backtick,
      openingIndent: 0,
      openingMarker: openingMarker,
      rawInfoSource: rawInfo,
      bodySource: body,
      closingMarker: closingMarker,
    ),
  ),
  projection: FlarkV3DocumentProjection(
    kind: FlarkV3DocumentStructureKind.fencedCode,
    source: source,
    projectedSource: body,
    runCount: 1,
  ),
);

FlarkV3DocumentStructuralBlock _bulletList({
  required int ordinal,
  required FlarkV3SourceSpan source,
}) {
  const hidden = FlarkV3SourceSpan(
    startUtf8: 0,
    endUtf8: 0,
    startUtf16: 0,
    endUtf16: 0,
  );
  return FlarkV3DocumentStructuralBlock(
    ordinal: ordinal,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.bulletList,
      source: source,
      visibleSource: hidden,
      referenceDefinitionCount: 0,
      bulletList: const FlarkV3BulletListFacts(
        marker: FlarkV3BulletListMarker.hyphen,
        itemCount: 1,
        terminalEmptyRelativeStartUtf8: null,
        paragraphCount: 1,
        projectedUtf8Length: 5,
        projectedUtf16Length: 5,
      ),
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.bulletList,
      source: source,
      projectedSource: hidden,
      runCount: 1,
    ),
  );
}

FlarkV3SourceVersion _sourceVersion(FlarkV3SourceDocument document) =>
    FlarkV3SourceVersion.fromDocument(
      documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
      document: document,
    );

FlarkV3StructuralAck _structuralAck(
  FlarkV3SourceVersion source, {
  int publicationSeed = 10,
}) => FlarkV3StructuralAck(
  publicationSession: FlarkV3PublicationSessionId(
    publicationSeed,
    publicationSeed + 1,
    publicationSeed + 2,
    publicationSeed + 3,
  ),
  hostRevision: FlarkV3HostRevisionId(1),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(5, 6),
  parseGeneration: 3,
  grammarRevision: 4,
  syntaxProfile: FlarkV3SyntaxProfileId(7),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 8,
  sequenceDigest: _digest128(20),
  manifestDigest: _digest128(30),
);

FlarkV3SourceSpan _span(int start, int end) => FlarkV3SourceSpan(
  startUtf8: start,
  endUtf8: end,
  startUtf16: start,
  endUtf16: end,
);

Uint8List _inlineStrongRecord() {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 2)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 5, Endian.little)
    ..setUint32(12, 2, Endian.little)
    ..setUint32(16, 1, Endian.little);
  return bytes;
}

Uint8List _inlineEmphasisRecord() {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 1)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 3, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, 1, Endian.little);
  return bytes;
}

Uint8List _inlineEmphasisAroundEntityRecord() {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 1)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 7, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, 5, Endian.little);
  return bytes;
}

Uint8List _inlineUriRecord({
  required int sourceLength,
  required int contentLength,
}) {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 5)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, sourceLength, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}

Uint8List _inlineCharacterReferenceRecord({int start = 1}) {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 9)
    ..setUint8(1, 1)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, 5, Endian.little)
    ..setUint32(12, 0x26, Endian.little)
    ..setUint32(16, 0, Endian.little);
  return bytes;
}

Uint8List _inlineDirectLinkRecord() {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 10)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 12, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, 1, Endian.little);
  return bytes;
}

Uint8List _inlineDirectLinkValues() {
  final bytes = Uint8List(49);
  final data = ByteData.sublistView(bytes);
  bytes.setRange(0, 8, ascii.encode('FLKIV001'));
  data
    ..setUint32(8, 1, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, 0, Endian.little)
    ..setUint32(20, 0, Endian.little)
    ..setUint32(24, 4, Endian.little)
    ..setUint32(28, 7, Endian.little)
    ..setUint32(32, 0, Endian.little)
    ..setUint32(36, 0, Endian.little)
    ..setUint32(40, 1, Endian.little)
    ..setUint32(44, 0, Endian.little)
    ..setUint8(48, 0x2a);
  return bytes;
}

Uint8List _inlineEscapedPunctuationRecord({int contentLength = 1}) {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint8(0, 7)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, 2, Endian.little)
    ..setUint32(12, 1, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}

Uint8List _lineRecord({
  required int physicalLength,
  required int hiddenPrefixLength,
  required int contentLength,
}) {
  final bytes = Uint8List(20);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint32(0, 0, Endian.little)
    ..setUint32(4, physicalLength, Endian.little)
    ..setUint32(8, hiddenPrefixLength, Endian.little)
    ..setUint32(12, contentLength, Endian.little)
    ..setUint32(16, 0, Endian.little);
  return bytes;
}

int _payloadKindCode(FlarkV3ViewportPresentationPayloadKind kind) =>
    switch (kind) {
      FlarkV3ViewportPresentationPayloadKind.inline => 1,
      FlarkV3ViewportPresentationPayloadKind.indentedCode => 2,
      FlarkV3ViewportPresentationPayloadKind.blockQuote => 3,
      FlarkV3ViewportPresentationPayloadKind.bulletList => 4,
      FlarkV3ViewportPresentationPayloadKind.orderedListItem => 6,
      FlarkV3ViewportPresentationPayloadKind.unsupported => 0xff,
    };

void _writeRange(
  ByteData data,
  int offset,
  FlarkV3ViewportPresentationMetricRange range,
) {
  data
    ..setUint32(offset, range.startUtf8, Endian.little)
    ..setUint32(offset + 4, range.startUtf16, Endian.little)
    ..setUint32(offset + 8, range.endUtf8, Endian.little)
    ..setUint32(offset + 12, range.endUtf16, Endian.little);
}

void _writeId(ByteData data, int offset, FlarkV3ProtocolId128 id) {
  data
    ..setUint32(offset, id.word0, Endian.little)
    ..setUint32(offset + 4, id.word1, Endian.little)
    ..setUint32(offset + 8, id.word2, Endian.little)
    ..setUint32(offset + 12, id.word3, Endian.little);
}

FlarkV3ProtocolDigest128 _digest128(int first) =>
    FlarkV3ProtocolDigest128(first, first + 1, first + 2, first + 3);

FlarkV3ProtocolDigest256 _digest256(int first) => FlarkV3ProtocolDigest256(
  first,
  first + 1,
  first + 2,
  first + 3,
  first + 4,
  first + 5,
  first + 6,
  first + 7,
);

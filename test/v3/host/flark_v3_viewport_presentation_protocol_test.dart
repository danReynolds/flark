import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

import '../support/flark_v3_viewport_page_fixture.dart';

void main() {
  group('FlarkV3 viewport-presentation semantic contract', () {
    test('complete and partial bindings authenticate exact ordered cuts', () {
      final complete = _binding();
      expect(complete.complete, isTrue);

      final partial = _binding(
        coveredEndUtf8: 5,
        coveredEndUtf16: 4,
        nextOrdinal: 9,
        complete: false,
      );
      expect(partial.complete, isFalse);

      expect(
        () => _binding(
          coveredEndUtf8: 5,
          coveredEndUtf16: 4,
          nextOrdinal: 9,
          complete: true,
        ),
        throwsArgumentError,
      );
      expect(() => _binding(nextOrdinal: 7), throwsArgumentError);
      expect(
        () => FlarkV3ViewportPresentationMetricRange(
          startUtf8: 4,
          startUtf16: 4,
          endUtf8: 4,
          endUtf16: 5,
        ),
        throwsRangeError,
      );
    });

    test('semantic limits and aggregate totals fail closed', () {
      expect(
        () => _queryLimits(maximumStructuralEntries: 4, maximumInlineLeaves: 5),
        throwsRangeError,
      );
      expect(
        () => _queryLimits(
          maximumInlineLeafSourceBytes: 2048,
          maximumInlineSourceBytes: 1024,
        ),
        throwsRangeError,
      );
      expect(
        () => _queryLimits(
          maximumStructuralEntries:
              FlarkV3ViewportPresentationQueryLimits
                  .productMaximumStructuralEntries +
              1,
        ),
        throwsRangeError,
      );
      expect(12 + 24 * 168, lessThanOrEqualTo(5140));
      expect(FlarkV3HostOfferLimits.productMaximumFrameBytes, 5140);
      expect(
        () => FlarkV3HostOfferLimits(
          maximumFrameCount: 256,
          maximumEncodedFrameBytes: 4 * 1024 * 1024,
          maximumPacketBytes: FlarkV3HostPublicationPacket.maximumRawBytes,
          maximumFrameBytes: 64 * 1024,
          maximumProgramChildren: 128,
        ),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ViewportPresentationOfferLimits(
          maximumFrameCount: 256,
          maximumEncodedFrameBytes: 4 * 1024 * 1024,
          maximumPacketBytes: FlarkV3HostPublicationPacket.maximumRawBytes,
          maximumFrameBytes: 64 * 1024,
          maximumProgramChildren: 128,
        ),
        returnsNormally,
      );
      expect(
        () => FlarkV3ViewportPresentationOfferLimits(
          maximumFrameCount: 256,
          maximumEncodedFrameBytes: 4 * 1024 * 1024,
          maximumPacketBytes: FlarkV3HostPublicationPacket.maximumRawBytes,
          maximumFrameBytes: 64 * 1024 + 1,
          maximumProgramChildren: 128,
        ),
        throwsRangeError,
      );

      final partial = _binding(
        coveredEndUtf8: 5,
        coveredEndUtf16: 4,
        nextOrdinal: 9,
        complete: false,
      );
      expect(
        () => _envelope(
          visitedStructuralEntries: 3,
        ).requireBindingAndLimits(partial, _queryLimits()),
        throwsArgumentError,
      );
      expect(
        () => _envelope(
          visitedStructuralEntries: 2,
          factCount: 33,
        ).requireBindingAndLimits(partial, _queryLimits()),
        throwsArgumentError,
      );

      final crossLaneBinding = FlarkV3ViewportPresentationBinding(
        viewportGeneration: 10,
        requestedRange: _range(endUtf8: 10, endUtf16: 8),
        coveredRange: _range(endUtf8: 10, endUtf16: 8),
        start: FlarkV3ViewportPresentationVisitStart(
          blockOrdinal: _u64(0xfffffffe, high: 2),
          utf8Offset: 0,
          utf16Offset: 0,
        ),
        next: FlarkV3ViewportPresentationVisitStart(
          blockOrdinal: _u64(1, high: 3),
          utf8Offset: 10,
          utf16Offset: 8,
        ),
        complete: true,
      );
      expect(
        () => _envelope().requireBindingAndLimits(
          crossLaneBinding,
          _queryLimits(),
        ),
        returnsNormally,
      );
    });

    test('Begin and ACK retain the complete structural base', () {
      final begin = _begin();
      expect(begin.hasExactBase(_baseAck), isTrue);
      expect(begin.hasExactBase(_differentBaseAck), isFalse);

      expect(
        () => _begin(publicationSession: _structuralPublication),
        throwsArgumentError,
      );
      expect(() => _begin(schema: 2), throwsArgumentError);
      expect(
        () => _begin(limits: _offerLimits(maximumFrameCount: 2)),
        throwsArgumentError,
      );

      final ack = _ack();
      expect(ack.baseAck, same(_baseAck));
      expect(ack.binding, _binding());
      expect(ack.actualFrameCount, 11);
      expect(ack.actualEncodedFrameBytes, 900);
      expect(() => ack.requireLimits(_offerLimits()), returnsNormally);
      expect({
        _ack(),
        _ack(),
        _ack(actualEncodedFrameBytes: 901),
      }, hasLength(2));
      expect(
        () => FlarkV3ViewportPresentationAck(
          publicationSession: _structuralPublication,
          baseAck: _baseAck,
          binding: _binding(),
          envelope: _envelope(),
          actualFrameCount: 11,
          actualEncodedFrameBytes: 900,
          aggregateRootStreamDigest: _digest128(150),
        ),
        throwsArgumentError,
      );
      expect(() => _ack(actualFrameCount: 2), throwsRangeError);
      expect(() => _ack(actualFrameCount: 10), throwsArgumentError);
      expect(() => _ack(actualEncodedFrameBytes: 0), throwsRangeError);
      expect(
        () => _ack(
          actualEncodedFrameBytes:
              FlarkV3ViewportPresentationQueryLimits
                  .productMaximumEncodedFrameBytes +
              1,
        ),
        throwsArgumentError,
      );
      expect(
        () => _ack().requireLimits(_offerLimits(maximumFrameCount: 10)),
        throwsRangeError,
      );
      expect(
        () => _ack(actualEncodedFrameBytes: 4097).requireLimits(_offerLimits()),
        throwsRangeError,
      );
    });

    test('commit and poll outcomes stay VPB1-specific and bounded', () {
      final commit = FlarkV3ViewportPresentationCommitRequest(
        offerId: _offer,
        actualFrameCount: 4,
        actualEncodedFrameBytes: 900,
        rollingTransportDigest: _digest128(100),
        aggregateRootStreamDigest: _digest128(110),
      );
      expect(commit.actualFrameCount, 4);
      expect(() => commit.requireLimits(_offerLimits()), returnsNormally);
      expect(
        () => FlarkV3ViewportPresentationCommitRequest(
          offerId: _offer,
          actualFrameCount: 2,
          actualEncodedFrameBytes: 900,
          rollingTransportDigest: _digest128(100),
          aggregateRootStreamDigest: _digest128(110),
        ),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ViewportPresentationCommitRequest(
          offerId: _offer,
          actualFrameCount: 17,
          actualEncodedFrameBytes: 900,
          rollingTransportDigest: _digest128(100),
          aggregateRootStreamDigest: _digest128(110),
        ).requireLimits(_offerLimits()),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ViewportPresentationHostPacketCredit(
          offerId: _offer,
          nextFrameOrdinal: 0,
        ),
        throwsRangeError,
      );

      final outcomes = <FlarkV3ViewportPresentationHostPollOutcome>[
        const FlarkV3ViewportPresentationHostPollPending(),
        FlarkV3ViewportPresentationHostPacketCredit(
          offerId: _offer,
          nextFrameOrdinal: 4,
        ),
        FlarkV3ViewportPresentationHostCommitted(_ack()),
        FlarkV3ViewportPresentationHostAbortComplete(_offer),
        const FlarkV3ViewportPresentationHostClosed(),
      ];
      expect(outcomes, hasLength(5));
      expect(
        outcomes
            .whereType<FlarkV3ViewportPresentationHostCommitted>()
            .single
            .ack,
        _ack(),
      );
    });

    test('query returns one exact-ACK-bound owned FLKVP001 page', () {
      final fixture = buildFlarkV3ViewportPageFixture();
      final ack = fixture.ack;
      final bytes = fixture.bytes;
      final page = FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
        ack: ack,
        encodedPage: bytes,
      );
      final capability = _ViewportCapability(page);
      final result = capability.queryViewportPresentation(
        FlarkV3ViewportPresentationQuery(
          ack: ack,
          maximumEncodedBytes: bytes.length,
        ),
      );
      final available =
          (result
                      as FlarkV3HostAccepted<
                        FlarkV3ViewportPresentationQueryOutcome
                      >)
                  .value
              as FlarkV3ViewportPresentationQueryAvailable;
      expect(available.page.ack, ack);
      expect(identical(available.page.encodedPage, bytes), isTrue);
      expect(capability.queryCount, 1);

      expect(
        () =>
            FlarkV3ViewportPresentationQuery(ack: ack, maximumEncodedBytes: 0),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ViewportPresentationAggregatePage.fromOwnedBytes(
          ack: ack,
          encodedPage: Uint8List.fromList('FLKVP002'.codeUnits),
        ),
        throwsA(isA<FlarkV3ViewportPresentationPageDecodeException>()),
      );
    });
  });
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _offer = FlarkV3OfferId(19, 20, 21, 22);
final _structuralPublication = FlarkV3PublicationSessionId(5, 6, 7, 8);
final _viewportPublication = FlarkV3PublicationSessionId(23, 24, 25, 26);

FlarkV3ProtocolU64 _u64(int low, {int high = 0}) =>
    FlarkV3ProtocolU64(lowWord: low, highWord: high);

FlarkV3ProtocolDigest128 _digest128(int first) =>
    FlarkV3ProtocolDigest128(first, first + 1, first + 2, first + 3);

FlarkV3ProtocolDigest256 _digest256(int word) =>
    FlarkV3ProtocolDigest256(word, word, word, word, word, word, word, word);

final _source = FlarkV3SourceVersion(
  documentSession: _documentSession,
  revision: 1,
  metric: FlarkV3SourceMetric(bytes: 10, utf16: 8),
  contentHash: FlarkV3ContentHash128(1, 2, 3, 4),
);

final _baseAck = FlarkV3StructuralAck(
  publicationSession: _structuralPublication,
  hostRevision: FlarkV3HostRevisionId(1),
  sourceVersion: _source,
  sourceRoot: FlarkV3SourceRootId(0, 1),
  parseGeneration: 1,
  grammarRevision: 1,
  syntaxProfile: FlarkV3SyntaxProfileId(1),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 3,
  sequenceDigest: _digest128(60),
  manifestDigest: _digest128(70),
);

final _differentBaseAck = FlarkV3StructuralAck(
  publicationSession: _baseAck.publicationSession,
  hostRevision: _baseAck.hostRevision,
  sourceVersion: _baseAck.sourceVersion,
  sourceRoot: _baseAck.sourceRoot,
  parseGeneration: _baseAck.parseGeneration,
  grammarRevision: _baseAck.grammarRevision,
  syntaxProfile: _baseAck.syntaxProfile,
  authorityMask: _baseAck.authorityMask,
  recordCount: _baseAck.recordCount,
  sequenceDigest: _digest128(61),
  manifestDigest: _baseAck.manifestDigest,
);

FlarkV3ViewportPresentationMetricRange _range({
  int endUtf8 = 10,
  int endUtf16 = 8,
}) => FlarkV3ViewportPresentationMetricRange(
  startUtf8: 0,
  startUtf16: 0,
  endUtf8: endUtf8,
  endUtf16: endUtf16,
);

FlarkV3ViewportPresentationBinding _binding({
  int coveredEndUtf8 = 10,
  int coveredEndUtf16 = 8,
  int nextOrdinal = 10,
  bool complete = true,
}) => FlarkV3ViewportPresentationBinding(
  viewportGeneration: 9,
  requestedRange: _range(),
  coveredRange: _range(endUtf8: coveredEndUtf8, endUtf16: coveredEndUtf16),
  start: FlarkV3ViewportPresentationVisitStart(
    blockOrdinal: _u64(7),
    utf8Offset: 0,
    utf16Offset: 0,
  ),
  next: FlarkV3ViewportPresentationVisitStart(
    blockOrdinal: _u64(nextOrdinal),
    utf8Offset: coveredEndUtf8,
    utf16Offset: coveredEndUtf16,
  ),
  complete: complete,
);

FlarkV3ViewportPresentationQueryLimits _queryLimits({
  int maximumStructuralEntries = 8,
  int maximumStoragePages = 8,
  int maximumInlineLeaves = 8,
  int maximumInlineLeafSourceBytes = 1024,
  int maximumInlineSourceBytes = 4096,
  int maximumFactRecords = 32,
  int maximumEncodedFrameBytes = 4096,
  int maximumParserTransitions = 1000,
}) => FlarkV3ViewportPresentationQueryLimits(
  maximumStructuralEntries: maximumStructuralEntries,
  maximumStoragePages: maximumStoragePages,
  maximumInlineLeaves: maximumInlineLeaves,
  maximumInlineLeafSourceBytes: maximumInlineLeafSourceBytes,
  maximumInlineSourceBytes: maximumInlineSourceBytes,
  maximumFactRecords: maximumFactRecords,
  maximumEncodedFrameBytes: maximumEncodedFrameBytes,
  maximumParserTransitions: maximumParserTransitions,
);

FlarkV3ViewportPresentationEnvelopeMetrics _envelope({
  int visitedStructuralEntries = 3,
  int factCount = 4,
}) => FlarkV3ViewportPresentationEnvelopeMetrics(
  visitedStructuralEntries: visitedStructuralEntries,
  visitedStoragePages: 2,
  orderedLeafCount: 2,
  inlineSourceBytes: 8,
  factCount: factCount,
  transferredNodeCount: 4,
  parserTransitions: 12,
  aggregateEnvelopeDigest256: _digest256(0xa1a1a1a1),
);

FlarkV3ViewportPresentationOfferLimits _offerLimits({
  int maximumFrameCount = 16,
}) => FlarkV3ViewportPresentationOfferLimits(
  maximumFrameCount: maximumFrameCount,
  maximumEncodedFrameBytes: 4096,
  maximumPacketBytes: 2048,
  maximumFrameBytes: 1024,
  maximumProgramChildren: 32,
);

FlarkV3ViewportPresentationOfferBegin _begin({
  int schema = FlarkV3ViewportPresentationOfferBegin.supportedSchema,
  FlarkV3PublicationSessionId? publicationSession,
  FlarkV3ViewportPresentationOfferLimits? limits,
}) => FlarkV3ViewportPresentationOfferBegin(
  schema: schema,
  offerId: _offer,
  publicationSession: publicationSession ?? _viewportPublication,
  baseAck: _baseAck,
  binding: _binding(),
  envelope: _envelope(),
  queryLimits: _queryLimits(),
  limits: limits ?? _offerLimits(),
);

FlarkV3ViewportPresentationAck _ack({
  int actualFrameCount = 11,
  int actualEncodedFrameBytes = 900,
}) => FlarkV3ViewportPresentationAck(
  publicationSession: _viewportPublication,
  baseAck: _baseAck,
  binding: _binding(),
  envelope: _envelope(),
  actualFrameCount: actualFrameCount,
  actualEncodedFrameBytes: actualEncodedFrameBytes,
  aggregateRootStreamDigest: _digest128(150),
);

final class _ViewportCapability
    implements FlarkV3ViewportPresentationHostStore {
  _ViewportCapability(this.page);

  final FlarkV3ViewportPresentationAggregatePage page;
  int queryCount = 0;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  ) => const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack) =>
      const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  ) => const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) => const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant) =>
      const FlarkV3HostAccepted(FlarkV3ViewportPresentationHostPollPending());

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query) {
    queryCount += 1;
    if (query.ack != page.ack ||
        query.maximumEncodedBytes < page.encodedPage.length) {
      return const FlarkV3HostAccepted(
        FlarkV3ViewportPresentationQueryUnavailable(),
      );
    }
    return FlarkV3HostAccepted(FlarkV3ViewportPresentationQueryAvailable(page));
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) => const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);
}

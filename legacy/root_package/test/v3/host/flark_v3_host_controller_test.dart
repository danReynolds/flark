import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

import '../support/flark_v3_publication_packet_fixture.dart';

void main() {
  test('initial authority is an exact editable BOF source gap', () {
    final source = _sourceVersion(_documentSession, 'alpha', revision: 0);
    final store = _ModelHostStore();
    final controller = _attach(store, source);

    final pending = controller.presentationState;
    expect(pending, isA<FlarkV3StablePendingPresentation>());
    final stable = pending as FlarkV3StablePendingPresentation;
    expect(stable.reason, FlarkV3StablePendingReason.initialSnapshot);
    expect(stable.sourceGap!.range.start, FlarkV3SourceMetric.zero);
    expect(stable.sourceGap!.range.end, source.metric);
    expect(stable.sourceGap!.sourceEditable, isTrue);
    expect(stable.sourceGap!.semanticActionsValid, isFalse);
    expect(stable.sourceGap!.accessibilitySemanticsValid, isFalse);
    expect(stable.sourceGap!.markdownHitTargetsValid, isFalse);
    expect(stable.sourceGap!.semanticSelectionMapValid, isFalse);

    final query = controller.query(
      FlarkV3HostPointQuery(
        sourceVersion: source,
        position: FlarkV3SourceMetric.zero,
        budget: _queryBudget,
      ),
    );
    expect(
      (query as FlarkV3HostAccepted<FlarkV3HostPresentationQuery>).value,
      isA<FlarkV3SourceGapPresentationQuery>(),
    );
    expect(store.structuralQueries, 0);
  });

  test(
    'Unicode insert/delete stays exact-source pending and rejects stale output',
    () {
      var document = FlarkV3SourceDocument.fromString('a🌍b\n');
      final source0 = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: document,
      );
      final store = _ModelHostStore();
      final controller = _attach(store, source0);
      final base = _publishOnePacket(
        controller,
        _snapshot(source0, session: _publicationA, hostRevision: 1),
      );
      expect(
        controller.presentationState,
        isA<FlarkV3ExactStructuralPresentation>(),
      );
      expect(controller.acknowledgeDelivery(base), isA<FlarkV3HostAccepted>());

      document = document
          .apply(
            FlarkV3SourceTransaction.single(
              baseRevision: document.revision,
              operation: const FlarkV3SourceEdit(
                startUtf16: 1,
                endUtf16: 1,
                replacement: 'é',
              ),
            ),
          )
          .document;
      final source1 = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: document,
      );
      expect(controller.observeSourceEdit(source1).storeSynchronized, isTrue);
      var pending =
          controller.presentationState as FlarkV3StablePendingPresentation;
      expect(pending.sourceGap!.range.start, FlarkV3SourceMetric.zero);
      expect(pending.sourceGap!.range.end, source1.metric);
      expect(pending.stablePaintAck, base);
      expect(pending.stablePaintAck!.sourceVersion, source0);
      expect(
        controller.beginOffer(
          _snapshot(source0, session: _publicationB, hostRevision: 1),
        ),
        _rejected(FlarkV3HostRejectReason.staleSource),
      );

      document = document
          .apply(
            FlarkV3SourceTransaction.single(
              baseRevision: document.revision,
              operation: const FlarkV3SourceEdit(
                startUtf16: 2,
                endUtf16: 4,
                replacement: '',
              ),
            ),
          )
          .document;
      expect(document.toString(), 'aéb\n');
      final source2 = FlarkV3SourceVersion.fromDocument(
        documentSession: _documentSession,
        document: document,
      );
      expect(controller.observeSourceEdit(source2).storeSynchronized, isTrue);
      pending =
          controller.presentationState as FlarkV3StablePendingPresentation;
      expect(pending.sourceGap!.range.start, FlarkV3SourceMetric.zero);
      expect(pending.sourceGap!.range.end, source2.metric);
      expect(
        pending.sourceGap!.sourceVersion.contentHash,
        document.contentHash128,
      );
      expect(
        store.installedAck,
        base,
        reason: 'old root remains only a paint cache',
      );
    },
  );

  test('commit supplies one-pass actual totals under begin ceilings', () {
    final source = _sourceVersion(_documentSession, 'stream\n', revision: 0);
    final store = _ModelHostStore();
    final controller = _attach(store, source);
    final offer = _snapshot(source, session: _publicationA, hostRevision: 1);

    expect(controller.beginOffer(offer), isA<FlarkV3HostAccepted>());
    expect(
      controller.admitPacket(
        testPublicationPacket(
          offerId: offer.offerId,
          firstFrameOrdinal: 0,
          firstRecordOrdinal: 0,
          recordCount: 1,
          digest: _digest(41),
          frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
        ),
      ),
      isA<FlarkV3HostAccepted>(),
    );
    expect(_poll(controller), isA<FlarkV3HostPacketCredit>());
    expect(offer.limits.maximumFrameCount, greaterThan(1));
    expect(offer.limits.maximumEncodedFrameBytes, greaterThan(3));
    expect(
      controller.requestCommit(
        FlarkV3HostCommitRequest(
          offerId: offer.offerId,
          actualFrameCount: 2,
          actualEncodedFrameBytes: 3,
          rollingTransportDigest: _digest(61),
          canonicalStreamDigest: _digest(62),
        ),
      ),
      _rejected(FlarkV3HostRejectReason.invalid),
    );
    expect(
      controller.requestCommit(_commitRequest(offer)),
      isA<FlarkV3HostAccepted>(),
    );
    final ack = (_poll(controller) as FlarkV3HostCommitted).ack;
    expect(ack.sequenceDigest, _digest(100));
    expect(ack.manifestDigest, _digest(200));
  });

  test('host commit cannot change parser authority declared at begin', () {
    final source = _sourceVersion(_documentSession, 'exact\n', revision: 0);
    final store = _ModelHostStore()..ackParseGenerationDelta = 1;
    final controller = _attach(store, source);
    final offer = _snapshot(source, session: _publicationA, hostRevision: 1);

    expect(controller.beginOffer(offer), isA<FlarkV3HostAccepted>());
    expect(
      controller.admitPacket(
        testPublicationPacket(
          offerId: offer.offerId,
          firstFrameOrdinal: 0,
          firstRecordOrdinal: 0,
          recordCount: 1,
          digest: _digest(41),
          frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
        ),
      ),
      isA<FlarkV3HostAccepted>(),
    );
    expect(_poll(controller), isA<FlarkV3HostPacketCredit>());
    expect(
      controller.requestCommit(_commitRequest(offer)),
      isA<FlarkV3HostAccepted>(),
    );

    expect(
      controller.poll(
        FlarkV3HostWorkGrant(
          inspectBytes: 4096,
          copyBytes: 4096,
          transitions: 64,
        ),
      ),
      _rejected(FlarkV3HostRejectReason.invalid),
    );
    expect(controller.pendingDeliveryAck, isNull);
    expect(
      controller.presentationState,
      isA<FlarkV3StablePendingPresentation>(),
    );
  });

  test('exact-base References delta becomes exact only at atomic commit', () {
    final oldSource = _sourceVersion(
      _documentSession,
      'left\nold\nright\n',
      revision: 0,
    );
    final newSource = _sourceVersion(
      _documentSession,
      'left\nnew\nright\n',
      revision: 1,
    );
    final store = _ModelHostStore();
    final controller = _attach(store, oldSource);
    final base = _publishOnePacket(
      controller,
      _snapshot(oldSource, session: _publicationA, hostRevision: 1, records: 3),
    );
    controller.acknowledgeDelivery(base);
    controller.observeSourceEdit(newSource);

    final delta = _delta(
      newSource,
      base: base,
      hostRevision: 2,
      transferred: 2,
      targetRecords: 3,
    );
    expect(delta.mode, FlarkV3PublicationMode.exactBaseReferencesDelta);
    expect(controller.beginOffer(delta), isA<FlarkV3HostAccepted>());
    final packet = testPublicationPacket(
      offerId: delta.offerId,
      firstFrameOrdinal: 0,
      firstRecordOrdinal: 0,
      recordCount: delta.transferredRecordCount,
      digest: _digest(41),
      frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
    );
    expect(controller.admitPacket(packet), isA<FlarkV3HostAccepted>());
    packet.rawBytes.fillRange(0, packet.rawBytes.length, 0);
    expect(
      controller.presentationState,
      isA<FlarkV3StablePendingPresentation>(),
      reason: 'admission must never expose a partial target',
    );
    expect(_poll(controller), isA<FlarkV3HostPacketCredit>());
    expect(
      controller.requestCommit(_commitRequest(delta)),
      isA<FlarkV3HostAccepted>(),
    );
    final committed = _poll(controller) as FlarkV3HostCommitted;
    expect(committed.ack.sourceVersion, newSource);
    expect(
      store.lastCommittedMode,
      FlarkV3PublicationMode.exactBaseReferencesDelta,
    );
    expect(store.lastCommittedPacketPrefix, [0xC1, 0xAA, 0x55]);
    expect(
      controller.presentationState,
      isA<FlarkV3ExactStructuralPresentation>(),
    );

    final query = controller.query(
      FlarkV3HostPointQuery(
        sourceVersion: newSource,
        position: FlarkV3SourceMetric(bytes: 6, utf16: 6),
        budget: _queryBudget,
      ),
    );
    expect(
      (query as FlarkV3HostAccepted<FlarkV3HostPresentationQuery>).value,
      isA<FlarkV3StructuralPresentationQuery>(),
    );
    expect(store.structuralQueries, 1);

    store.rejectQueryForBounds = true;
    final boundedFallback = controller.query(
      FlarkV3HostPointQuery(
        sourceVersion: newSource,
        position: FlarkV3SourceMetric(bytes: 6, utf16: 6),
        budget: _queryBudget,
      ),
    );
    final gap =
        (boundedFallback as FlarkV3HostAccepted<FlarkV3HostPresentationQuery>)
                .value
            as FlarkV3SourceGapPresentationQuery;
    expect(gap.gap.sourceEditable, isTrue);
    expect(gap.gap.semanticActionsValid, isFalse);
    expect(gap.gap.range.start, FlarkV3SourceMetric.zero);
    expect(gap.gap.range.end, newSource.metric);
    expect(gap.gap.structuralReason, FlarkV3HostSourceGapReason.openDepthLimit);
    expect(gap.gap.structuralReceipt!.openDepth, _queryBudget.maxOpenDepth);

    store.rejectQueryForBounds = false;
    store.treeNodesVisitedOverride = _queryBudget.maxTreeNodesVisited + 1;
    final overVisited = controller.query(
      FlarkV3HostPointQuery(
        sourceVersion: newSource,
        position: FlarkV3SourceMetric(bytes: 6, utf16: 6),
        budget: _queryBudget,
      ),
    );
    final defensiveGap =
        (overVisited as FlarkV3HostAccepted<FlarkV3HostPresentationQuery>).value
            as FlarkV3SourceGapPresentationQuery;
    expect(defensiveGap.gap.structuralReceipt, isNull);
  });

  test(
    'one unacknowledged offer backpressures and fresh snapshot recovers',
    () {
      final source0 = _sourceVersion(_documentSession, 'one\n', revision: 0);
      final source1 = _sourceVersion(_documentSession, 'two\n', revision: 1);
      final source2 = _sourceVersion(_documentSession, 'three\n', revision: 2);
      final store = _ModelHostStore();
      final controller = _attach(store, source0);
      final base = _publishOnePacket(
        controller,
        _snapshot(source0, session: _publicationA, hostRevision: 1),
      );
      controller.acknowledgeDelivery(base);

      controller.observeSourceEdit(source1);
      final lostAck = _publishOnePacket(
        controller,
        _delta(source1, base: base, hostRevision: 2),
      );
      expect(controller.pendingDeliveryAck, lostAck);

      controller.observeSourceEdit(source2);
      final sameSession = _delta(source2, base: lostAck, hostRevision: 3);
      expect(
        controller.beginOffer(sameSession),
        _rejected(FlarkV3HostRejectReason.backpressure),
      );

      final recovery = _snapshot(
        source2,
        session: _publicationB,
        hostRevision: 1,
      );
      final recovered = _publishOnePacket(controller, recovery);
      expect(recovered.publicationSession, _publicationB);
      expect(controller.pendingDeliveryAck, recovered);
      expect(
        controller.acknowledgeDelivery(lostAck),
        _rejected(FlarkV3HostRejectReason.invalid),
      );
      expect(
        controller.acknowledgeDelivery(recovered),
        isA<FlarkV3HostAccepted>(),
      );
    },
  );

  test(
    'store failure cannot roll back exact source or authorize stale paint',
    () {
      final source0 = _sourceVersion(_documentSession, 'old', revision: 0);
      final source1 = _sourceVersion(_documentSession, 'new', revision: 1);
      final store = _ModelHostStore();
      final controller = _attach(store, source0);
      store.rejectNextObservation = true;

      final adoption = controller.observeSourceEdit(source1);
      expect(adoption.sourceAccepted, isTrue);
      expect(adoption.storeSynchronized, isFalse);
      expect(adoption.storeRejection!.reason, FlarkV3HostRejectReason.closed);
      final pending =
          controller.presentationState as FlarkV3StablePendingPresentation;
      expect(pending.sourceVersion, source1);
      expect(pending.reason, FlarkV3StablePendingReason.storeUnsynchronized);
      expect(pending.sourceGap!.semanticActionsValid, isFalse);
      expect(
        controller.beginOffer(
          _snapshot(source1, session: _publicationB, hostRevision: 1),
        ),
        _rejected(FlarkV3HostRejectReason.closed),
      );
      expect(controller.resynchronizeStore(), isA<FlarkV3HostAccepted>());
    },
  );

  test(
    'startup host failure still returns a usable exact-source controller',
    () {
      final source = _sourceVersion(_documentSession, 'offline', revision: 0);
      final store = _ModelHostStore()..rejectNextObservation = true;
      final attachment = FlarkV3HostController.attach(
        currentSource: source,
        store: store,
      );

      expect(attachment.storeSynchronized, isFalse);
      expect(attachment.storeRejection!.reason, FlarkV3HostRejectReason.closed);
      final pending =
          attachment.controller.presentationState
              as FlarkV3StablePendingPresentation;
      expect(pending.sourceVersion, source);
      expect(pending.reason, FlarkV3StablePendingReason.storeUnsynchronized);
      expect(pending.sourceGap!.sourceEditable, isTrue);
      expect(pending.sourceGap!.semanticActionsValid, isFalse);
      expect(
        attachment.controller.resynchronizeStore(),
        isA<FlarkV3HostAccepted>(),
      );
    },
  );

  test('ordinal locator accepts one exact fake-store witness', () {
    final source = _sourceVersion(
      _documentSession,
      'alpha\nbeta\n',
      revision: 0,
    );
    final store = _ModelHostStore();
    final controller = _attach(store, source);
    _publishOnePacket(
      controller,
      _snapshot(source, session: _publicationA, hostRevision: 1),
    );
    final query = FlarkV3HostStructuralOrdinalWindowQuery(
      sourceVersion: source,
      startBlockOrdinal: FlarkV3ProtocolU64.fromU32(1),
      budget: FlarkV3HostStructuralOrdinalWindowBudget(
        maximumEntries: 4,
        maximumStoragePagesVisited: 2,
        maximumTreeNodesVisited: 16,
        maximumPackedEntriesInspected: 64,
      ),
    );
    store.ordinalOutcome = (request) => FlarkV3HostStructuralOrdinalWindow(
      sourceVersion: request.sourceVersion,
      totalBlockCount: FlarkV3ProtocolU64.fromU32(2),
      startBlockOrdinal: request.startBlockOrdinal,
      nextBlockOrdinal: FlarkV3ProtocolU64.fromU32(2),
      startSource: FlarkV3SourceMetric(bytes: 6, utf16: 6),
      nextSource: source.metric,
      work: FlarkV3HostStructuralOrdinalWindowWorkReceipt(
        storagePagesVisited: 1,
        treeNodesVisited: 2,
        packedEntriesInspected: 2,
        summaryNodesSkipped: 0,
      ),
      complete: true,
    );

    final result = controller.queryStructuralOrdinalWindow(query);
    expect(
      (result as FlarkV3HostAccepted<FlarkV3HostStructuralOrdinalWindowOutcome>)
          .value,
      isA<FlarkV3HostStructuralOrdinalWindow>(),
    );
  });

  test('malformed fake ordinal witness is quarantined and fails closed', () {
    final source = _sourceVersion(
      _documentSession,
      'alpha\nbeta\n',
      revision: 0,
    );
    final store = _ModelHostStore();
    final controller = _attach(store, source);
    _publishOnePacket(
      controller,
      _snapshot(source, session: _publicationA, hostRevision: 1),
    );
    final query = FlarkV3HostStructuralOrdinalWindowQuery(
      sourceVersion: source,
      startBlockOrdinal: FlarkV3ProtocolU64.fromU32(1),
      budget: FlarkV3HostStructuralOrdinalWindowBudget(
        maximumEntries: 4,
        maximumStoragePagesVisited: 2,
        maximumTreeNodesVisited: 16,
        maximumPackedEntriesInspected: 64,
      ),
    );
    store.ordinalOutcome = (request) => FlarkV3HostStructuralOrdinalWindow(
      sourceVersion: request.sourceVersion,
      totalBlockCount: FlarkV3ProtocolU64.fromU32(2),
      startBlockOrdinal: request.startBlockOrdinal,
      nextBlockOrdinal: request.startBlockOrdinal,
      startSource: FlarkV3SourceMetric(bytes: 6, utf16: 6),
      nextSource: FlarkV3SourceMetric(bytes: 6, utf16: 6),
      work: FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero,
      complete: false,
    );

    final result = controller.queryStructuralOrdinalWindow(query);
    final failure =
        (result
                    as FlarkV3HostAccepted<
                      FlarkV3HostStructuralOrdinalWindowOutcome
                    >)
                .value
            as FlarkV3HostStructuralOrdinalWindowFailure;
    expect(
      failure.reason,
      FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable,
    );
    expect(
      (controller.presentationState as FlarkV3StablePendingPresentation).reason,
      FlarkV3StablePendingReason.storeUnsynchronized,
    );
  });
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _publicationA = FlarkV3PublicationSessionId(10, 11, 12, 13);
final _publicationB = FlarkV3PublicationSessionId(20, 21, 22, 23);
final _syntaxProfile = FlarkV3SyntaxProfileId(1);
final _queryBudget = FlarkV3HostQueryBudget(
  maxEncodedBytes: 64 * 1024,
  maxOpenDepth: 256,
  maxLeafCount: 512,
  maxTreeNodesVisited: 2048,
);
final _offerLimits = FlarkV3HostOfferLimits(
  maximumFrameCount: 4,
  maximumEncodedFrameBytes: 1024,
  maximumPacketBytes: 256,
  maximumFrameBytes: 128,
  maximumProgramChildren: 128,
);

FlarkV3HostController _attach(
  FlarkV3HostStore store,
  FlarkV3SourceVersion source,
) => FlarkV3HostController.attach(
  currentSource: source,
  store: store,
).controller;

FlarkV3SourceVersion _sourceVersion(
  FlarkV3DocumentSessionId session,
  String source, {
  required int revision,
}) {
  final exact = FlarkV3SourceDocument.fromString(source);
  return FlarkV3SourceVersion(
    documentSession: session,
    revision: revision,
    metric: FlarkV3SourceMetric(
      bytes: exact.utf8Length,
      utf16: exact.utf16Length,
    ),
    contentHash: exact.contentHash128,
  );
}

FlarkV3HostOfferBegin _snapshot(
  FlarkV3SourceVersion source, {
  required FlarkV3PublicationSessionId session,
  required int hostRevision,
  int records = 1,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(hostRevision, source.revision, 1, 1),
  publicationSession: session,
  targetHostRevision: FlarkV3HostRevisionId(hostRevision),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(0, source.revision + 1),
  parseGeneration: source.revision + 1,
  grammarRevision: 1,
  syntaxProfile: _syntaxProfile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  mode: FlarkV3PublicationMode.fullSnapshot,
  baseAck: null,
  transferredRecordCount: records,
  targetRecordCount: records,
  limits: _offerLimits,
);

FlarkV3HostOfferBegin _delta(
  FlarkV3SourceVersion source, {
  required FlarkV3StructuralAck base,
  required int hostRevision,
  int transferred = 1,
  int? targetRecords,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(hostRevision, source.revision, 2, 2),
  publicationSession: FlarkV3PublicationSessionId(
    100 + hostRevision,
    101 + hostRevision,
    102 + hostRevision,
    103 + hostRevision,
  ),
  targetHostRevision: FlarkV3HostRevisionId(hostRevision),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(0, source.revision + 1),
  parseGeneration: source.revision + 1,
  grammarRevision: 1,
  syntaxProfile: _syntaxProfile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  mode: FlarkV3PublicationMode.exactBaseReferencesDelta,
  baseAck: base,
  transferredRecordCount: transferred,
  targetRecordCount: targetRecords ?? base.recordCount + transferred,
  limits: _offerLimits,
);

FlarkV3StructuralAck _publishOnePacket(
  FlarkV3HostController controller,
  FlarkV3HostOfferBegin offer,
) {
  expect(controller.beginOffer(offer), isA<FlarkV3HostAccepted>());
  expect(
    controller.admitPacket(
      testPublicationPacket(
        offerId: offer.offerId,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: offer.transferredRecordCount,
        digest: _digest(41),
        frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
      ),
    ),
    isA<FlarkV3HostAccepted>(),
  );
  expect(_poll(controller), isA<FlarkV3HostPacketCredit>());
  expect(
    controller.requestCommit(_commitRequest(offer)),
    isA<FlarkV3HostAccepted>(),
  );
  return (_poll(controller) as FlarkV3HostCommitted).ack;
}

FlarkV3HostPollOutcome _poll(FlarkV3HostController controller) =>
    (controller.poll(
              FlarkV3HostWorkGrant(
                inspectBytes: 4096,
                copyBytes: 4096,
                transitions: 64,
              ),
            )
            as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
        .value;

FlarkV3HostCommitRequest _commitRequest(FlarkV3HostOfferBegin offer) =>
    FlarkV3HostCommitRequest(
      offerId: offer.offerId,
      actualFrameCount: 1,
      actualEncodedFrameBytes: 3,
      rollingTransportDigest: _digest(61),
      canonicalStreamDigest: _digest(62),
    );

Matcher _rejected(FlarkV3HostRejectReason reason) => isA<FlarkV3HostRejected>()
    .having((result) => result.rejection.reason, 'reason', reason);

FlarkV3ProtocolDigest128 _digest(int seed) =>
    FlarkV3ProtocolDigest128(seed, seed + 1, seed + 2, seed + 3);

final class _ModelHostStore
    implements FlarkV3HostStore, FlarkV3StructuralOrdinalWindowHostStore {
  FlarkV3SourceVersion? currentSource;
  FlarkV3StructuralAck? installedAck;
  FlarkV3StructuralAck? pendingAck;
  _ModelOffer? active;
  bool closed = false;
  bool rejectNextObservation = false;
  bool rejectQueryForBounds = false;
  int? treeNodesVisitedOverride;
  int ackParseGenerationDelta = 0;
  int structuralQueries = 0;
  FlarkV3PublicationMode? lastCommittedMode;
  List<int>? lastCommittedPacketPrefix;
  FlarkV3HostStructuralOrdinalWindowOutcome Function(
    FlarkV3HostStructuralOrdinalWindowQuery query,
  )?
  ordinalOutcome;

  @override
  FlarkV3HostCallResult<FlarkV3HostStructuralOrdinalWindowOutcome>
  queryStructuralOrdinalWindow(FlarkV3HostStructuralOrdinalWindowQuery query) =>
      FlarkV3HostAccepted(
        ordinalOutcome?.call(query) ??
            FlarkV3HostStructuralOrdinalWindowFailure(
              sourceVersion: query.sourceVersion,
              totalBlockCount: FlarkV3ProtocolU64.zero,
              startBlockOrdinal: query.startBlockOrdinal,
              reason:
                  FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable,
              work: FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero,
            ),
      );

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    if (closed || rejectNextObservation) {
      rejectNextObservation = false;
      return _reject(FlarkV3HostRejectReason.closed, 'store unavailable');
    }
    final current = currentSource;
    if (current != null &&
        (current.documentSession != sourceVersion.documentSession ||
            sourceVersion.revision < current.revision)) {
      return _reject(FlarkV3HostRejectReason.invalid, 'invalid source lineage');
    }
    currentSource = sourceVersion;
    if (active?.begin.sourceVersion != sourceVersion) {
      active = null;
    }
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    if (closed) return _reject(FlarkV3HostRejectReason.closed, 'closed');
    if (active != null) {
      return _reject(FlarkV3HostRejectReason.backpressure, 'offer active');
    }
    final source = currentSource;
    if (source == null || begin.sourceVersion != source) {
      return _reject(
        begin.sourceVersion.revision < (source?.revision ?? 0)
            ? FlarkV3HostRejectReason.staleSource
            : FlarkV3HostRejectReason.exactSourceMismatch,
        'source mismatch',
      );
    }
    final waiting = pendingAck;
    if (waiting != null &&
        (begin.mode != FlarkV3PublicationMode.fullSnapshot ||
            begin.publicationSession == waiting.publicationSession)) {
      return _reject(FlarkV3HostRejectReason.backpressure, 'ACK pending');
    }
    final installed = installedAck;
    if (begin.mode == FlarkV3PublicationMode.exactBaseReferencesDelta) {
      if (installed == null || begin.baseAck != installed) {
        return _reject(FlarkV3HostRejectReason.baseMismatch, 'base mismatch');
      }
    } else if (installed != null &&
        begin.publicationSession == installed.publicationSession &&
        begin.targetHostRevision.value <= installed.hostRevision.value) {
      return _reject(FlarkV3HostRejectReason.baseMismatch, 'snapshot rollback');
    }
    active = _ModelOffer(begin);
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final offer = active;
    if (offer == null || offer.begin.offerId != packet.offerId) {
      return _reject(FlarkV3HostRejectReason.wrongOffer, 'wrong offer');
    }
    if (offer.pendingPacket != null) {
      return _reject(FlarkV3HostRejectReason.backpressure, 'credit consumed');
    }
    if (packet.firstFrameOrdinal != offer.nextFrame ||
        packet.firstRecordOrdinal != offer.acceptedRecords ||
        packet.firstFrameOrdinal + packet.frameCount >
            offer.begin.limits.maximumFrameCount ||
        packet.rawBytes.length > offer.begin.limits.maximumPacketBytes ||
        packet.aggregateFrameBytes > offer.begin.limits.maximumFrameBytes ||
        offer.acceptedBytes + packet.aggregateFrameBytes >
            offer.begin.limits.maximumEncodedFrameBytes) {
      return _reject(
        FlarkV3HostRejectReason.corruptPublication,
        'packet order',
      );
    }
    offer.pendingPacket = _OwnedModelPacket(
      frameCount: packet.frameCount,
      recordCount: packet.aggregateRecordCount,
      frameBytes: Uint8List.fromList(testSingleFrameBody(packet)),
    );
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    final offer = active;
    if (offer == null || offer.begin.offerId != request.offerId) {
      return _reject(FlarkV3HostRejectReason.wrongOffer, 'wrong offer');
    }
    if (offer.pendingPacket != null ||
        offer.nextFrame != request.actualFrameCount ||
        offer.acceptedRecords != offer.begin.transferredRecordCount ||
        offer.acceptedBytes != request.actualEncodedFrameBytes ||
        request.actualFrameCount > offer.begin.limits.maximumFrameCount ||
        request.actualEncodedFrameBytes >
            offer.begin.limits.maximumEncodedFrameBytes) {
      return _reject(FlarkV3HostRejectReason.invalid, 'offer incomplete');
    }
    offer.commitRequest = request;
    offer.commitRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    if (active?.begin.offerId != offerId) {
      return _reject(FlarkV3HostRejectReason.wrongOffer, 'wrong offer');
    }
    active!.abortRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final offer = active;
    if (offer == null) {
      return const FlarkV3HostAccepted(FlarkV3HostPollPending());
    }
    if (offer.abortRequested) {
      active = null;
      return FlarkV3HostAccepted(FlarkV3HostAbortComplete(offer.begin.offerId));
    }
    final pendingPacket = offer.pendingPacket;
    if (pendingPacket != null) {
      if (grant.inspectBytes < pendingPacket.frameBytes.length ||
          grant.copyBytes < pendingPacket.frameBytes.length ||
          grant.transitions == 0) {
        return const FlarkV3HostAccepted(FlarkV3HostPollPending());
      }
      offer.pendingPacket = null;
      offer.nextFrame += pendingPacket.frameCount;
      offer.acceptedRecords += pendingPacket.recordCount;
      offer.acceptedBytes += pendingPacket.frameBytes.length;
      offer.lastPacketPrefix = pendingPacket.frameBytes
          .take(3)
          .toList(growable: false);
      return FlarkV3HostAccepted(
        FlarkV3HostPacketCredit(
          offerId: offer.begin.offerId,
          nextFrameOrdinal: offer.nextFrame,
        ),
      );
    }
    if (!offer.commitRequested) {
      return const FlarkV3HostAccepted(FlarkV3HostPollPending());
    }
    if (currentSource != offer.begin.sourceVersion) {
      active = null;
      return _reject(FlarkV3HostRejectReason.superseded, 'source advanced');
    }
    final ack = FlarkV3StructuralAck(
      publicationSession: offer.begin.publicationSession,
      hostRevision: offer.begin.targetHostRevision,
      sourceVersion: offer.begin.sourceVersion,
      sourceRoot: offer.begin.sourceRoot,
      parseGeneration: offer.begin.parseGeneration + ackParseGenerationDelta,
      grammarRevision: offer.begin.grammarRevision,
      syntaxProfile: offer.begin.syntaxProfile,
      authorityMask: offer.begin.authorityMask,
      recordCount: offer.begin.targetRecordCount,
      sequenceDigest: _digest(100 + offer.begin.sourceVersion.revision),
      manifestDigest: _digest(200 + offer.begin.sourceVersion.revision),
    );
    installedAck = ack;
    pendingAck = ack;
    lastCommittedMode = offer.begin.mode;
    lastCommittedPacketPrefix = offer.lastPacketPrefix;
    active = null;
    return FlarkV3HostAccepted(FlarkV3HostCommitted(ack));
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) {
    if (pendingAck != ack) {
      return _reject(FlarkV3HostRejectReason.invalid, 'ACK mismatch');
    }
    pendingAck = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) {
    structuralQueries += 1;
    if (rejectQueryForBounds) {
      return FlarkV3HostAccepted(
        FlarkV3HostStoreSourceGapQuery(
          FlarkV3HostLocalSourceGap(
            sourceVersion: currentSource!,
            range: FlarkV3MetricRange(
              start: FlarkV3SourceMetric.zero,
              end: currentSource!.metric,
            ),
            reason: FlarkV3HostSourceGapReason.openDepthLimit,
            receipt: FlarkV3HostViewportReceipt(
              encodedBytes: 0,
              leafCount: 1,
              openDepth: query.budget.maxOpenDepth,
              treeNodesVisited: 2,
              summaryNodesSkipped: 1,
            ),
          ),
        ),
      );
    }
    if (installedAck?.sourceVersion != currentSource ||
        query.sourceVersion != currentSource) {
      return _reject(
        FlarkV3HostRejectReason.exactSourceMismatch,
        'structure is not exact current',
      );
    }
    return FlarkV3HostAccepted(
      FlarkV3HostStoreStructuralQuery(
        FlarkV3HostStructuralViewport.copied(
          sourceVersion: currentSource!,
          range: FlarkV3MetricRange(
            start: FlarkV3SourceMetric.zero,
            end: currentSource!.metric,
          ),
          encoded: Uint8List.fromList([1, 2, 3]),
          receipt: FlarkV3HostViewportReceipt(
            encodedBytes: 3,
            leafCount: 1,
            openDepth: 1,
            treeNodesVisited: treeNodesVisitedOverride ?? 2,
            summaryNodesSkipped: 1,
          ),
        ),
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    closed = true;
    active = null;
    return _accepted();
  }
}

final class _ModelOffer {
  _ModelOffer(this.begin);

  final FlarkV3HostOfferBegin begin;
  int nextFrame = 0;
  int acceptedRecords = 0;
  int acceptedBytes = 0;
  _OwnedModelPacket? pendingPacket;
  bool commitRequested = false;
  FlarkV3HostCommitRequest? commitRequest;
  bool abortRequested = false;
  List<int>? lastPacketPrefix;
}

final class _OwnedModelPacket {
  const _OwnedModelPacket({
    required this.frameCount,
    required this.recordCount,
    required this.frameBytes,
  });

  final int frameCount;
  final int recordCount;
  final Uint8List frameBytes;
}

FlarkV3HostAccepted<FlarkV3HostUnit> _accepted() =>
    const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

FlarkV3HostRejected<T> _reject<T>(
  FlarkV3HostRejectReason reason,
  String message,
) => FlarkV3HostRejected(FlarkV3HostRejection(reason, message));

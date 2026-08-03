import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/session/session.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

import '../support/flark_v3_publication_packet_fixture.dart';

void main() {
  test(
    'sidecar lifecycle remains sibling to structural presentation and bounded',
    () {
      final store = _LifecycleStore();
      final session = _session(store);
      addTearDown(session.close);
      final base = _installStructure(session, generation: 1);
      final presentationBefore = session.presentationState;
      final sidecar = _sidecarOffer(base, generation: 1);

      expect(session.supportsInlineSidecars, isTrue);
      expect(
        session.beginInlineSidecarOffer(sidecar),
        isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
      );
      final packet = _sidecarPacket(sidecar);
      expect(
        session.admitInlineSidecarPacket(packet),
        isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
      );
      expect(
        (session.pollInlineSidecar(_grant)
                as FlarkV3HostAccepted<FlarkV3InlineSidecarHostPollOutcome>)
            .value,
        isA<FlarkV3InlineSidecarHostPacketCredit>(),
      );
      final commit = _sidecarCommit(sidecar);
      expect(
        session.requestInlineSidecarCommit(commit),
        isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
      );
      final committed =
          (session.pollInlineSidecar(_grant)
                      as FlarkV3HostAccepted<
                        FlarkV3InlineSidecarHostPollOutcome
                      >)
                  .value
              as FlarkV3InlineSidecarHostCommitted;

      expect(committed.ack.baseAck, base);
      expect(session.pendingInlineSidecarDeliveryAck, committed.ack);
      expect(session.installedInlineSidecarAck, committed.ack);
      expect(session.pendingDeliveryAck, isNull);
      expect(
        session.presentationState.runtimeType,
        presentationBefore.runtimeType,
      );
      expect(
        (session.presentationState as FlarkV3ExactStructuralPresentation).ack,
        base,
      );
      expect(
        session.queryInlineSidecar(
          FlarkV3InlineSidecarQuery(
            binding: sidecar.binding,
            maximumEncodedBytes: 20,
          ),
        ),
        isA<FlarkV3HostAccepted<FlarkV3InlineSidecarQueryOutcome>>(),
      );
      expect(
        session.acknowledgeInlineSidecarDelivery(committed.ack),
        isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
      );
      expect(session.pendingInlineSidecarDeliveryAck, isNull);

      final pollsBefore = store.sidecarPollCount;
      final rejected = session.pollInlineSidecar(
        FlarkV3HostWorkGrant(
          inspectBytes:
              FlarkDocumentWorkProfile.prototype.maximumHostInspectBytes + 1,
          copyBytes: 0,
          transitions: 0,
        ),
      );
      expect(
        (rejected as FlarkV3HostRejected<FlarkV3InlineSidecarHostPollOutcome>)
            .rejection
            .reason,
        FlarkV3HostRejectReason.foregroundBoundExceeded,
      );
      expect(store.sidecarPollCount, pollsBefore);
    },
  );

  test('structural and source advance reject stale sidecar work', () {
    final store = _LifecycleStore();
    final session = _session(store);
    addTearDown(session.close);
    final base = _installStructure(session, generation: 1);
    final staleOnStructure = _sidecarOffer(base, generation: 1);
    expect(
      session.beginInlineSidecarOffer(staleOnStructure),
      isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
    );

    final replacement = _installStructure(session, generation: 2);
    expect(replacement, isNot(base));
    expect(session.installedInlineSidecarAck, isNull);
    expect(
      (session.requestInlineSidecarCommit(_sidecarCommit(staleOnStructure))
              as FlarkV3HostRejected<FlarkV3HostUnit>)
          .rejection
          .reason,
      FlarkV3HostRejectReason.wrongOffer,
    );
    expect(
      (session.presentationState as FlarkV3ExactStructuralPresentation).ack,
      replacement,
    );

    final staleOnSource = _sidecarOffer(replacement, generation: 2);
    expect(
      session.beginInlineSidecarOffer(staleOnSource),
      isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
    );
    final edit = session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 5,
          endUtf16: 5,
          replacement: '!',
        ),
      ),
    );
    expect(edit.changed, isTrue);
    expect(store.sidecarAbortCount, 1);
    expect(
      (session.admitInlineSidecarPacket(_sidecarPacket(staleOnSource))
              as FlarkV3HostRejected<FlarkV3HostUnit>)
          .rejection
          .reason,
      FlarkV3HostRejectReason.wrongOffer,
    );
    expect(session.installedInlineSidecarAck, isNull);
    expect(session.presentationState, isA<FlarkV3StablePendingPresentation>());
  });

  test('structural-only custom stores fail closed for sidecars', () {
    final store = _StructuralOnlyStore();
    final session = _session(store);
    addTearDown(session.close);
    final base = _installStructure(session, generation: 1);

    expect(session.supportsInlineSidecars, isFalse);
    final result = session.beginInlineSidecarOffer(
      _sidecarOffer(base, generation: 1),
    );
    final rejection =
        (result as FlarkV3HostRejected<FlarkV3HostUnit>).rejection;
    expect(rejection.reason, FlarkV3HostRejectReason.closed);
    expect(rejection.message, contains('capability'));
    expect(
      (session.presentationState as FlarkV3ExactStructuralPresentation).ack,
      base,
    );
  });
}

final _grant = FlarkV3HostWorkGrant(
  inspectBytes: 4096,
  copyBytes: 4096,
  transitions: 32,
);

final _document = FlarkV3DocumentSessionId(801, 802, 803, 804);

FlarkDocumentSession _session(FlarkV3HostStore store) {
  final session = FlarkDocumentSession.attach(
    sourceSession: FlarkV3SourceSession.fromString('**x**'),
    documentSession: _document,
    hostStore: store,
  );
  _acknowledgeAllSourceWorkerSync(session);
  return session;
}

void _acknowledgeAllSourceWorkerSync(FlarkDocumentSession session) {
  while (session.hasPendingSourceWorkerSync) {
    final lease = session.beginSourceWorkerSync();
    final target = switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
      FlarkV3SourceIntentSyncLease() => lease.targetStamp,
    };
    final observed = FlarkV3ObservedSourceReplicaVersion(
      revision: target.revision,
      utf16Length: target.utf16Length,
      utf8Length: switch (target) {
        FlarkV3KnownSourceStamp() => target.utf8Length,
        FlarkV3ProvisionalSourceStamp() =>
          utf8.encode(session.source.toString()).length,
      },
      intentHighWater: switch (lease) {
        FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
        FlarkV3SourceIntentSyncLease() => lease.lastSequence,
      },
    );
    final acknowledgement = switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
        observedReplica: lease.isLast ? observed : null,
      ),
      FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
        observedReplica: observed,
      ),
    };
    expect(
      session.acknowledgeSourceWorkerSync(acknowledgement).disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
  }
}

FlarkV3StructuralAck _installStructure(
  FlarkDocumentSession session, {
  required int generation,
}) {
  final offer = _structuralOffer(session.sourceVersion, generation);
  expect(
    session.beginOffer(offer),
    isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
  );
  expect(
    session.requestCommit(
      FlarkV3HostCommitRequest(
        offerId: offer.offerId,
        actualFrameCount: 0,
        actualEncodedFrameBytes: 0,
        rollingTransportDigest: FlarkV3ProtocolDigest128.zero,
        canonicalStreamDigest: FlarkV3ProtocolDigest128.zero,
      ),
    ),
    isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
  );
  final committed =
      (session.pollHost(_grant) as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
              .value
          as FlarkV3HostCommitted;
  expect(
    session.acknowledgeDelivery(committed.ack),
    isA<FlarkV3HostAccepted<FlarkV3HostUnit>>(),
  );
  return committed.ack;
}

FlarkV3HostOfferBegin _structuralOffer(
  FlarkV3SourceVersion source,
  int generation,
) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(810 + generation, 811, 812, 813),
  publicationSession: FlarkV3PublicationSessionId(
    820 + generation,
    821,
    822,
    823,
  ),
  targetHostRevision: FlarkV3HostRevisionId(generation),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(0, generation),
  parseGeneration: generation,
  grammarRevision: 1,
  syntaxProfile: FlarkV3SyntaxProfileId(1),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  mode: FlarkV3PublicationMode.fullSnapshot,
  baseAck: null,
  transferredRecordCount: 1,
  targetRecordCount: 1,
  limits: FlarkV3HostOfferLimits(
    maximumFrameCount: 1,
    maximumEncodedFrameBytes: 1024,
    maximumPacketBytes: 1092,
    maximumFrameBytes: 1024,
    maximumProgramChildren: 1,
  ),
);

FlarkV3HotInlineSidecarOfferBegin _sidecarOffer(
  FlarkV3StructuralAck base, {
  required int generation,
}) => FlarkV3HotInlineSidecarOfferBegin(
  offerId: FlarkV3OfferId(830 + generation, 831, 832, 833),
  publicationSession: FlarkV3PublicationSessionId(
    840 + generation,
    841,
    842,
    843,
  ),
  baseAck: base,
  binding: FlarkV3HotInlineSidecarBinding(
    parserProfile: base.syntaxProfile,
    refinementGeneration: FlarkV3ProtocolU64.fromU32(generation),
    blockOrdinal: FlarkV3ProtocolU64.zero,
    physicalStartUtf8: 0,
    physicalEndUtf8: 5,
    visibleStartUtf8: 0,
    visibleEndUtf8: 5,
    physicalStartUtf16: 0,
    physicalEndUtf16: 5,
    visibleStartUtf16: 0,
    visibleEndUtf16: 5,
  ),
  envelope: FlarkV3HotInlineSidecarEnvelopeMetrics(
    ipr2DescriptorBytes:
        FlarkV3HotInlineSidecarEnvelopeMetrics.ipr2FixedDescriptorBytes,
    transferredNodeCount: 1,
    hio1EnvelopeDigest256: FlarkV3ProtocolDigest256(1, 2, 3, 4, 5, 6, 7, 8),
    disposition: FlarkV3HotInlineSidecarAuthoritative(
      logicalPageCount: FlarkV3ProtocolU64.fromU32(1),
      factCount: FlarkV3ProtocolU64.fromU32(1),
      storagePageCount: FlarkV3ProtocolU64.fromU32(1),
      linkValueEntryCount: 0,
      linkValueStoragePageCount: FlarkV3ProtocolU64.zero,
      linkValueEncodedBytes: 0,
      orderedCommitment256: FlarkV3ProtocolDigest256(
        11,
        12,
        13,
        14,
        15,
        16,
        17,
        18,
      ),
    ),
  ),
  limits: FlarkV3HostOfferLimits(
    maximumFrameCount: 3,
    maximumEncodedFrameBytes: 1024,
    maximumPacketBytes: 1092,
    maximumFrameBytes: 1024,
    maximumProgramChildren: 1,
  ),
);

FlarkV3HostPublicationPacket _sidecarPacket(
  FlarkV3HotInlineSidecarOfferBegin sidecar,
) => testPublicationPacket(
  offerId: sidecar.offerId,
  firstFrameOrdinal: 0,
  firstRecordOrdinal: 0,
  recordCount: 1,
  digest: FlarkV3ProtocolDigest128(1, 2, 3, 4),
  frameBytes: Uint8List.fromList(const [1]),
);

FlarkV3HotInlineSidecarCommitRequest _sidecarCommit(
  FlarkV3HotInlineSidecarOfferBegin sidecar,
) => FlarkV3HotInlineSidecarCommitRequest(
  offerId: sidecar.offerId,
  actualFrameCount: 3,
  actualEncodedFrameBytes: 3,
  rollingTransportDigest: FlarkV3ProtocolDigest128(21, 22, 23, 24),
  rootStreamDigest: FlarkV3ProtocolDigest128(31, 32, 33, 34),
);

final class _LifecycleStore
    implements FlarkV3HostStore, FlarkV3InlineSidecarHostStore {
  FlarkV3SourceVersion? currentSource;
  FlarkV3HostOfferBegin? structuralOffer;
  FlarkV3HostCommitRequest? structuralCommit;
  FlarkV3StructuralAck? structuralAck;
  FlarkV3HotInlineSidecarOfferBegin? sidecarOffer;
  FlarkV3HotInlineSidecarCommitRequest? sidecarCommit;
  FlarkV3InlineSidecarAck? sidecarAck;
  FlarkV3OfferId? sidecarPacketOffer;
  FlarkV3OfferId? sidecarAbortOffer;
  int sidecarPollCount = 0;
  int sidecarAbortCount = 0;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    currentSource = sourceVersion;
    structuralOffer = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    structuralOffer = begin;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => _accepted();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    structuralCommit = request;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    structuralOffer = null;
    structuralCommit = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final offer = structuralOffer;
    if (offer != null && structuralCommit != null) {
      final ack = FlarkV3StructuralAck(
        publicationSession: offer.publicationSession,
        hostRevision: offer.targetHostRevision,
        sourceVersion: offer.sourceVersion,
        sourceRoot: offer.sourceRoot,
        parseGeneration: offer.parseGeneration,
        grammarRevision: offer.grammarRevision,
        syntaxProfile: offer.syntaxProfile,
        authorityMask: offer.authorityMask,
        recordCount: offer.targetRecordCount,
        sequenceDigest: FlarkV3ProtocolDigest128(41, 42, 43, 44),
        manifestDigest: FlarkV3ProtocolDigest128(51, 52, 53, 54),
      );
      structuralAck = ack;
      structuralOffer = null;
      structuralCommit = null;
      return FlarkV3HostAccepted(FlarkV3HostCommitted(ack));
    }
    return const FlarkV3HostAccepted(FlarkV3HostPollPending());
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => _accepted();

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => FlarkV3HostAccepted(
    FlarkV3HostStoreSourceGapQuery(
      FlarkV3HostLocalSourceGap(
        sourceVersion: currentSource!,
        range: FlarkV3MetricRange(
          start: FlarkV3SourceMetric.zero,
          end: currentSource!.metric,
        ),
        reason: FlarkV3HostSourceGapReason.unavailableFacts,
        receipt: FlarkV3HostViewportReceipt(
          encodedBytes: 0,
          leafCount: 0,
          openDepth: 0,
          treeNodesVisited: 0,
          summaryNodesSkipped: 0,
        ),
      ),
    ),
  );

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginInlineSidecarOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  ) {
    sidecarOffer = begin;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitInlineSidecarPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    sidecarPacketOffer = packet.offerId;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestInlineSidecarCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  ) {
    sidecarCommit = request;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortInlineSidecarOffer(
    FlarkV3OfferId offerId,
  ) {
    sidecarAbortCount += 1;
    sidecarAbortOffer = offerId;
    sidecarOffer = null;
    sidecarCommit = null;
    sidecarPacketOffer = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarHostPollOutcome> pollInlineSidecar(
    FlarkV3HostWorkGrant grant,
  ) {
    sidecarPollCount += 1;
    final aborted = sidecarAbortOffer;
    if (aborted != null) {
      sidecarAbortOffer = null;
      return FlarkV3HostAccepted(
        FlarkV3InlineSidecarHostAbortComplete(aborted),
      );
    }
    final packetOffer = sidecarPacketOffer;
    if (packetOffer != null) {
      sidecarPacketOffer = null;
      return FlarkV3HostAccepted(
        FlarkV3InlineSidecarHostPacketCredit(
          offerId: packetOffer,
          nextFrameOrdinal: 1,
        ),
      );
    }
    final offer = sidecarOffer;
    final commit = sidecarCommit;
    if (offer != null && commit != null) {
      final disposition = switch (offer.envelope.disposition) {
        FlarkV3HotInlineSidecarAuthoritative() =>
          FlarkV3InlineSidecarAckDisposition.authoritative,
        FlarkV3HotInlineSidecarUnsupported() =>
          FlarkV3InlineSidecarAckDisposition.unsupported,
      };
      final ack = FlarkV3InlineSidecarAck(
        publicationSession: offer.publicationSession,
        baseAck: offer.baseAck,
        refinementGeneration: offer.binding.refinementGeneration,
        blockOrdinal: offer.binding.blockOrdinal,
        transferredNodeCount: offer.envelope.transferredNodeCount,
        disposition: disposition,
        hio1EnvelopeDigest256: offer.envelope.hio1EnvelopeDigest256,
        rootStreamDigest: commit.rootStreamDigest,
      );
      sidecarAck = ack;
      sidecarOffer = null;
      sidecarCommit = null;
      return FlarkV3HostAccepted(FlarkV3InlineSidecarHostCommitted(ack));
    }
    return const FlarkV3HostAccepted(FlarkV3InlineSidecarHostPollPending());
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeInlineSidecarDelivery(
    FlarkV3InlineSidecarAck ack,
  ) => _accepted();

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> queryInlineSidecar(
    FlarkV3InlineSidecarQuery query,
  ) => FlarkV3HostAccepted(
    FlarkV3InlineSidecarQueryAuthoritative(
      payloadKind: FlarkV3InlineSidecarPayloadKind.inline,
      factCount: 1,
      valueEntryCount: 0,
      treeNodesVisited: 1,
      encodedFacts: Uint8List(20),
      encodedValues: Uint8List(0),
    ),
  );

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() => _accepted();
}

final class _StructuralOnlyStore implements FlarkV3HostStore {
  final _LifecycleStore _delegate = _LifecycleStore();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => _delegate.observeSourceVersion(sourceVersion);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) => _delegate.beginOffer(begin);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => _delegate.admitPacket(packet);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => _delegate.requestCommit(request);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      _delegate.abortOffer(offerId);

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => _delegate.poll(grant);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => _delegate.acknowledgeDelivery(ack);

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => _delegate.queryStructural(query);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() => _delegate.close();
}

FlarkV3HostAccepted<FlarkV3HostUnit> _accepted() =>
    const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

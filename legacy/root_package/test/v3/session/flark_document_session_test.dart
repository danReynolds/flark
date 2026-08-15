import 'dart:convert';

import 'package:flark/flark_adapter.dart';
import 'package:test/test.dart';

void main() {
  test('indexed edit and undo advance one source-and-host authority', () {
    final sourceSession = FlarkV3SourceSession.fromString('live');
    final store = _SessionHostStore();
    final session = FlarkDocumentSession.attach(
      sourceSession: sourceSession,
      documentSession: _documentSession,
      hostStore: store,
    );
    addTearDown(session.close);

    final edit = session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 4,
          endUtf16: 4,
          replacement: '!',
        ),
      ),
    );

    expect(edit.changed, isTrue);
    expect(edit.provisional, isFalse);
    expect(edit.certifiedAdoption!.storeSynchronized, isTrue);
    expect(session.source.toString(), 'live!');
    expect(session.sourceVersion.revision, 1);
    expect(store.currentSource, session.sourceVersion);
    expect(session.uiSource.bindsCertified(session.sourceVersion), isTrue);

    final undo = session.undo()!;
    expect(undo.changed, isTrue);
    expect(undo.provisional, isFalse);
    expect(session.source.toString(), 'live');
    expect(session.sourceVersion.revision, 2);
    expect(store.currentSource, session.sourceVersion);
  });

  test(
    'provisional edit suppresses publication until exact-current certification',
    () {
      final sourceSession = FlarkV3SourceSession.fromString(
        'live',
        ordinaryReplacementUtf16Limit: 1,
      );
      final store = _SessionHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _documentSession,
        hostStore: store,
      );
      addTearDown(session.close);
      _acknowledgeAllSourceWorkerSync(session);

      final offer = _offer(session.sourceVersion);
      expect(session.beginOffer(offer), isA<FlarkV3HostAccepted>());

      final edit = session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 4,
            endUtf16: 4,
            replacement: '!!',
          ),
        ),
      );

      expect(edit.provisional, isTrue);
      expect(
        edit.uiAdvance!.activeOfferAbort,
        isNull,
        reason:
            'Certified source adoption owns physical host supersession; '
            'provisional UI authority only suppresses staging locally.',
      );
      expect(session.source.toString(), 'live!!');
      expect(session.sourceVersion.revision, 0);
      expect(session.uiSource.uiRevision, 1);
      expect(session.currentUiSourceCertified, isFalse);
      expect(
        session.query(
          FlarkV3HostPointQuery(
            sourceVersion: session.sourceVersion,
            position: FlarkV3SourceMetric.zero,
            budget: _queryBudget,
          ),
        ),
        isA<FlarkV3HostRejected>(),
      );

      _acknowledgeAllSourceWorkerSync(session);
      final request = session.beginSourceCertification();
      final certification = FlarkV3SourceCertificationReceipt.scan(
        request,
        sourceReplica: session.source,
      );
      final promoted = session.applySourceCertification(certification);

      expect(promoted.promoted, isTrue);
      expect(promoted.hostAdoption!.storeSynchronized, isTrue);
      expect(session.currentUiSourceCertified, isTrue);
      expect(session.sourceVersion.revision, 1);
      expect(store.currentSource, session.sourceVersion);
    },
  );

  test(
    'staged source certification advances host authority only at commit',
    () {
      final sourceSession = FlarkV3SourceSession.fromString(
        'live',
        ordinaryReplacementUtf16Limit: 1,
      );
      final store = _SessionHostStore();
      final session = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: _documentSession,
        hostStore: store,
      );
      addTearDown(session.close);
      final certifiedBefore = session.sourceVersion;
      final observationsBefore = store.observeSourceVersionCount;
      final replacement = '\n${'x' * 96}';

      final edit = session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: session.source.utf16Length,
            endUtf16: session.source.utf16Length,
            replacement: replacement,
          ),
        ),
      );
      expect(edit.provisional, isTrue);
      expect(session.currentUiSourceCertified, isFalse);
      expect(session.sourceVersion, certifiedBefore);
      expect(store.currentSource, certifiedBefore);
      expect(store.observeSourceVersionCount, observationsBefore);

      _acknowledgeAllSourceWorkerSync(session);
      expect(session.sourceWorkerSynchronized, isTrue);
      final request = session.beginSourceCertification();
      final scanner = FlarkV3SourceFactScanner(
        request,
        sourceReplica: sourceSession.acknowledgedSourceReplica(),
        checkpointSpacingUtf16: 8,
      );
      const credit = FlarkV3SourceFactScanCredit(
        maximumSourceUtf16: 4,
        maximumSourceNodes: 2,
        maximumOutputCheckpoints: 1,
        maximumWireBytes: 128,
      );
      FlarkV3SourceFactCompletion? completion;
      var polls = 0;
      var pages = 0;
      while (completion == null) {
        final poll = scanner.poll(credit);
        polls += 1;
        expect(polls, lessThan(1000), reason: 'bounded scanner must progress');
        expect(poll.work.sourceUtf16Examined, lessThanOrEqualTo(4));
        expect(poll.work.sourceNodesVisited, lessThanOrEqualTo(2));
        expect(poll.work.checkpointsEmitted, lessThanOrEqualTo(1));
        expect(poll.work.wireBytesEmitted, lessThanOrEqualTo(128));
        if (poll.page case final page?) {
          pages += 1;
          final staged = session.stageSourceCertificationCheckpointPage(page);
          expect(staged.disposition, FlarkV3SourceFactStageDisposition.staged);
          expect(staged.piecesAttached, lessThanOrEqualTo(1));
          expect(staged.pathNodesVisited, lessThanOrEqualTo(512));
          expect(page.isConsumed, isTrue);
          expect(session.source.isFullyIndexed, isFalse);
          expect(session.sourceVersion, certifiedBefore);
          expect(store.currentSource, certifiedBefore);
          expect(store.observeSourceVersionCount, observationsBefore);
        }
        completion = poll.completion;
      }

      expect(polls, greaterThan(1));
      expect(pages, greaterThan(1));
      expect(completion.pageCount, pages);
      expect(session.source.toString(), 'live$replacement');
      expect(session.source.isFullyIndexed, isFalse);

      final promoted = session.commitSourceFactCertification(completion);
      expect(promoted.promoted, isTrue);
      expect(promoted.hostAdoption!.storeSynchronized, isTrue);
      expect(session.source.isFullyIndexed, isTrue);
      expect(session.currentUiSourceCertified, isTrue);
      expect(session.sourceVersion.revision, certifiedBefore.revision + 1);
      expect(store.currentSource, session.sourceVersion);
      expect(store.observeSourceVersionCount, observationsBefore + 1);

      final replayed = session.commitSourceFactCertification(completion);
      expect(
        replayed.promotion.disposition,
        FlarkV3SourcePromotionDisposition.stale,
      );
      expect(replayed.hostAdoption, isNull);
      expect(store.observeSourceVersionCount, observationsBefore + 1);
    },
  );

  test('document work profile rejects oversized caller work', () {
    final store = _SessionHostStore();
    final session = FlarkDocumentSession.attach(
      sourceSession: FlarkV3SourceSession.fromString('x'),
      documentSession: _documentSession,
      hostStore: store,
    );
    addTearDown(session.close);

    final poll = session.pollHost(
      FlarkV3HostWorkGrant(
        inspectBytes:
            FlarkDocumentWorkProfile.prototype.maximumHostInspectBytes + 1,
        copyBytes: 0,
        transitions: 0,
      ),
    );
    expect(
      (poll as FlarkV3HostRejected<FlarkV3HostPollOutcome>).rejection.reason,
      FlarkV3HostRejectReason.foregroundBoundExceeded,
    );
    expect(store.pollCount, 0);

    expect(
      () => session.query(
        FlarkV3HostPointQuery(
          sourceVersion: session.sourceVersion,
          position: FlarkV3SourceMetric.zero,
          budget: FlarkV3HostQueryBudget(
            maxEncodedBytes:
                FlarkDocumentWorkProfile.prototype.maximumQueryEncodedBytes + 1,
            maxOpenDepth: 1,
            maxLeafCount: 1,
            maxTreeNodesVisited: 1,
          ),
        ),
      ),
      throwsArgumentError,
    );
    expect(
      () => session.query(
        FlarkV3HostPointQuery(
          sourceVersion: session.sourceVersion,
          position: FlarkV3SourceMetric.zero,
          budget: FlarkV3HostQueryBudget(
            maxEncodedBytes: 1,
            maxOpenDepth: 1,
            maxLeafCount: 1,
            maxTreeNodesVisited:
                FlarkDocumentWorkProfile
                    .prototype
                    .maximumQueryTreeNodesVisited +
                1,
          ),
        ),
      ),
      throwsArgumentError,
    );
  });
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);

final _queryBudget = FlarkV3HostQueryBudget(
  maxEncodedBytes: 4096,
  maxOpenDepth: 32,
  maxLeafCount: 64,
  maxTreeNodesVisited: 256,
);

void _acknowledgeAllSourceWorkerSync(FlarkDocumentSession session) {
  while (session.hasPendingSourceWorkerSync) {
    final lease = session.beginSourceWorkerSync();
    final acknowledgement = switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
        observedReplica: lease.isLast ? _observedFor(session, lease) : null,
      ),
      FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
        observedReplica: _observedFor(session, lease),
      ),
    };
    expect(
      session.acknowledgeSourceWorkerSync(acknowledgement).disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
  }
}

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkDocumentSession session,
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  final utf8Length = switch (target) {
    FlarkV3KnownSourceStamp() => target.utf8Length,
    FlarkV3ProvisionalSourceStamp() =>
      utf8.encode(session.source.toString()).length,
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: utf8Length,
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

FlarkV3HostOfferBegin _offer(FlarkV3SourceVersion source) =>
    FlarkV3HostOfferBegin(
      offerId: FlarkV3OfferId(10, 11, 12, 13),
      publicationSession: FlarkV3PublicationSessionId(20, 21, 22, 23),
      targetHostRevision: FlarkV3HostRevisionId(1),
      sourceVersion: source,
      sourceRoot: FlarkV3SourceRootId(0, 1),
      parseGeneration: 1,
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

final class _SessionHostStore implements FlarkV3HostStore {
  FlarkV3SourceVersion? currentSource;
  FlarkV3HostOfferBegin? active;
  FlarkV3OfferId? pendingAbort;
  int pollCount = 0;
  int observeSourceVersionCount = 0;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    observeSourceVersionCount += 1;
    currentSource = sourceVersion;
    active = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    active = begin;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => _accepted();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => _accepted();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    pendingAbort = offerId;
    active = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    pollCount += 1;
    final aborted = pendingAbort;
    if (aborted != null) {
      pendingAbort = null;
      return FlarkV3HostAccepted(FlarkV3HostAbortComplete(aborted));
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
  FlarkV3HostCallResult<FlarkV3HostUnit> close() => _accepted();
}

FlarkV3HostAccepted<FlarkV3HostUnit> _accepted() =>
    const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

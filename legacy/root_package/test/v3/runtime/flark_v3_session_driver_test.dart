import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/flark_adapter.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_session_driver.dart';
import 'package:flark/src/v3/runtime/flark_v3_viewport_presentation_transport.dart';
import 'package:test/test.dart';

import '../support/flark_v3_publication_packet_fixture.dart';

void main() {
  test('driver rejects host grants that can never advance one frame', () {
    const maximumFrameBytes = FlarkV3HostOfferLimits.productMaximumFrameBytes;
    const minimumInspectBytes =
        FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes +
        maximumFrameBytes;

    for (final grant in <FlarkV3HostWorkGrant>[
      FlarkV3HostWorkGrant(inspectBytes: 0, copyBytes: 0, transitions: 0),
      FlarkV3HostWorkGrant(
        inspectBytes: minimumInspectBytes - 1,
        copyBytes: maximumFrameBytes,
        transitions: 1,
      ),
      FlarkV3HostWorkGrant(
        inspectBytes: minimumInspectBytes,
        copyBytes: maximumFrameBytes - 1,
        transitions: 1,
      ),
      FlarkV3HostWorkGrant(
        inspectBytes: minimumInspectBytes,
        copyBytes: maximumFrameBytes,
        transitions: 0,
      ),
    ]) {
      expect(
        () => _harness(
          FlarkV3SourceSession.fromString('live'),
          acknowledgeOpen: false,
          hostPollGrant: grant,
        ),
        throwsA(
          isA<ArgumentError>().having(
            (error) => error.name,
            'name',
            'hostPollGrant',
          ),
        ),
      );
    }
  });

  test('driver accepts the exact one-frame host grant boundary', () {
    const maximumFrameBytes = FlarkV3HostOfferLimits.productMaximumFrameBytes;
    final harness = _harness(
      FlarkV3SourceSession.fromString('live'),
      acknowledgeOpen: false,
      hostPollGrant: FlarkV3HostWorkGrant(
        inspectBytes:
            FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes +
            maximumFrameBytes,
        copyBytes: maximumFrameBytes,
        transitions: 1,
      ),
    );
    addTearDown(harness.dispose);

    expect(harness.driver.state, FlarkV3SessionDriverState.opening);
  });

  test('empty documents require exact fresh-open and explicit source seed', () {
    final harness = _harness(
      FlarkV3SourceSession.fromString(''),
      acknowledgeOpen: false,
    );
    addTearDown(harness.dispose);

    final open = harness.transport.last<FlarkV3ParserOpen>();
    expect(open.mode, FlarkV3ParserOpenMode.fresh);
    expect(open.binding, harness.driver.parserBinding);
    expect(harness.driver.state, FlarkV3SessionDriverState.opening);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.idle);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      isEmpty,
    );

    final wrongBinding = FlarkV3ParserSessionBinding(
      documentSession: open.binding.documentSession,
      sourceSessionIdentity: open.binding.sourceSessionIdentity + 1,
      workerGeneration: open.binding.workerGeneration,
    );
    harness.transport.emit(
      FlarkV3ParserOpened(
        eventId: 1,
        binding: wrongBinding,
        mode: FlarkV3ParserOpenMode.fresh,
      ),
    );
    harness.driver.pump();
    expect(harness.driver.state, FlarkV3SessionDriverState.opening);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
    );

    harness.transport.emit(
      FlarkV3ParserOpened(
        eventId: 2,
        binding: open.binding,
        mode: FlarkV3ParserOpenMode.recovery,
      ),
    );
    harness.driver.pump();
    expect(harness.driver.state, FlarkV3SessionDriverState.opening);

    harness.transport.emit(
      FlarkV3ParserOpened(
        eventId: 3,
        binding: open.binding,
        mode: FlarkV3ParserOpenMode.fresh,
      ),
    );
    harness.driver.pump();
    expect(harness.driver.state, FlarkV3SessionDriverState.open);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final seed = harness.transport.last<FlarkV3ParserSynchronizeSource>().lease;
    expect(seed, isA<FlarkV3SourceSnapshotSyncLease>());
    final emptySeed = seed as FlarkV3SourceSnapshotSyncLease;
    expect(emptySeed.startUtf16, 0);
    expect(emptySeed.endUtf16, 0);
    expect(emptySeed.totalUtf16Length, 0);
    expect(emptySeed.throughIntentSequence, 0);
    expect(emptySeed.source, isEmpty);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      hasLength(1),
    );
  });

  test('one source lease and one callback event can be in flight', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final lease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    expect(harness.driver.hasSourceLeaseInFlight, isTrue);

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.idle);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      hasLength(1),
    );

    harness.transport.emit(_ackEvent(2, lease));
    expect(harness.driver.hasPendingParserEvent, isTrue);
    expect(() => harness.transport.emit(_ackEvent(3, lease)), throwsStateError);

    final receipt = harness.driver.pump();
    expect(receipt.action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.driver.hasPendingParserEvent, isFalse);
    expect(harness.driver.hasSourceLeaseInFlight, isFalse);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.accepted,
    );
    expect(harness.session.sourceWorkerSynchronized, isTrue);
  });

  test('only the exact typed source acknowledgement returns lease credit', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    harness.driver.pump();
    final lease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;

    harness.transport.emit(
      FlarkV3ParserSourceSynchronized(
        eventId: 2,
        workerGeneration: lease.workerGeneration,
        acknowledgement: _wrongLeaseAcknowledgement(lease),
      ),
    );
    harness.driver.pump();

    expect(harness.driver.hasSourceLeaseInFlight, isTrue);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.stale,
    );

    harness.transport.emit(_ackEvent(3, lease));
    harness.driver.pump();
    expect(harness.driver.hasSourceLeaseInFlight, isFalse);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.accepted,
    );

    harness.transport.emit(_ackEvent(4, lease));
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.stale,
    );
  });

  test('canonical SourceFacts pages stay provisional until terminal proof', () {
    const source = 'abcdefgh';
    final sourceSession = FlarkV3SourceSession.fromProvisionalString(source);
    final harness = _harness(
      sourceSession,
      certifiedSourceVersion: FlarkV3SourceVersion.empty(_documentSession),
    );
    addTearDown(harness.dispose);

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final lease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    final targetStamp = switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
      FlarkV3SourceIntentSyncLease() => lease.targetStamp,
    };
    final observedReplica = FlarkV3ObservedSourceReplicaVersion(
      revision: targetStamp.revision,
      utf16Length: source.length,
      utf8Length: utf8.encode(source).length,
      intentHighWater: switch (lease) {
        FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
        FlarkV3SourceIntentSyncLease() => lease.lastSequence,
      },
    );
    harness.transport.emit(
      FlarkV3ParserSourceSynchronized(
        eventId: 2,
        workerGeneration: lease.workerGeneration,
        acknowledgement: switch (lease) {
          FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
            observedReplica: lease.isLast ? observedReplica : null,
          ),
          FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
            observedReplica: observedReplica,
          ),
        },
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.session.sourceWorkerSynchronized, isTrue);
    expect(harness.session.source.hasCertifiedFacts, isFalse);
    expect(harness.session.currentUiSourceCertified, isFalse);

    final binding = harness.driver.parserBinding;
    final lineage = FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: binding.sourceSessionIdentity,
      requestId: 31,
      workerGeneration: binding.workerGeneration,
      workerReplicaRevision: observedReplica.revision,
      uiRevision: harness.session.uiRevision,
      utf16Length: source.length,
      intentHighWater: observedReplica.intentHighWater,
    );
    final facts = <FlarkV3SourcePrefixFacts>[
      _canonicalFact(source, 4),
      _canonicalFact(source, source.length),
    ];
    final page = FlarkV3CanonicalSourceFactCheckpointPage(
      lineage: lineage,
      pageOrdinal: 0,
      pageCount: 1,
      checkpointCount: facts.length,
      checkpointSpacingUtf16: 4,
      checkpoints: facts,
    );
    harness.transport.emit(
      FlarkV3ParserSourceFactsPage(eventId: 3, binding: binding, page: page),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    final pageReceipt = harness.transport.last<FlarkV3ParserEventReceipt>();
    expect(pageReceipt.disposition, FlarkV3ParserEventDisposition.accepted);
    expect(pageReceipt.sourceCertification, isNull);
    expect(page.isConsumed, isTrue);
    expect(harness.session.source.hasCertifiedFacts, isFalse);
    expect(harness.session.currentUiSourceCertified, isFalse);

    final oracle = FlarkV3SourceDocument.fromString(source);
    final completion = FlarkV3CanonicalSourceFactCompletion(
      lineage: lineage,
      fingerprintAlgorithm: 1,
      fingerprint: FlarkV3SourceFingerprint(
        revision: harness.session.uiRevision,
        utf16Length: source.length,
        utf8Length: utf8.encode(source).length,
        contentHash128: oracle.contentHash128,
      ),
      logicalLineBreaks: 0,
      checkpointSpacingUtf16: 4,
      checkpointCount: facts.length,
      pageCount: 1,
      checkpointHash128: _portableFactsHash(facts),
    );
    harness.transport.emit(
      FlarkV3ParserSourceFactsCompleted(
        eventId: 4,
        binding: binding,
        completion: completion,
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    final terminalReceipt = harness.transport.last<FlarkV3ParserEventReceipt>();
    expect(terminalReceipt.disposition, FlarkV3ParserEventDisposition.accepted);
    final proof = terminalReceipt.sourceCertification!;
    expect(proof.lineage, lineage);
    expect(proof.fingerprint, completion.fingerprint);
    expect(proof.checkpointHash128, completion.checkpointHash128);
    expect(harness.session.source.hasCertifiedFacts, isTrue);
    expect(harness.session.currentUiSourceCertified, isTrue);
    expect(harness.session.storeSourceSynchronized, isTrue);
    expect(harness.session.sourceVersion.revision, harness.session.uiRevision);
    expect(harness.session.sourceVersion.metric.utf16, source.length);
    expect(harness.session.sourceVersion.contentHash, oracle.contentHash128);
  });

  test('canonical SourceFacts delta promotes only at its terminal proof', () {
    const baseSource = 'abcdefgh';
    const targetSource = 'abcdZfgh';
    final sourceSession = FlarkV3SourceSession.fromProvisionalString(
      baseSource,
    );
    final harness = _harness(
      sourceSession,
      certifiedSourceVersion: FlarkV3SourceVersion.empty(_documentSession),
    );
    addTearDown(harness.dispose);

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final baseLease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    final baseObserved = FlarkV3ObservedSourceReplicaVersion(
      revision: switch (baseLease) {
        FlarkV3SourceSnapshotSyncLease() => baseLease.targetStamp.revision,
        FlarkV3SourceIntentSyncLease() => baseLease.targetStamp.revision,
      },
      utf16Length: baseSource.length,
      utf8Length: utf8.encode(baseSource).length,
      intentHighWater: switch (baseLease) {
        FlarkV3SourceSnapshotSyncLease() => baseLease.throughIntentSequence,
        FlarkV3SourceIntentSyncLease() => baseLease.lastSequence,
      },
    );
    harness.transport.emit(
      FlarkV3ParserSourceSynchronized(
        eventId: 2,
        workerGeneration: baseLease.workerGeneration,
        acknowledgement: switch (baseLease) {
          FlarkV3SourceSnapshotSyncLease() => baseLease.acknowledgement(
            observedReplica: baseLease.isLast ? baseObserved : null,
          ),
          FlarkV3SourceIntentSyncLease() => baseLease.acknowledgement(
            observedReplica: baseObserved,
          ),
        },
      ),
    );
    harness.driver.pump();
    final binding = harness.driver.parserBinding;
    final baseLineage = FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: binding.sourceSessionIdentity,
      requestId: 41,
      workerGeneration: binding.workerGeneration,
      workerReplicaRevision: harness.session.uiRevision,
      uiRevision: harness.session.uiRevision,
      utf16Length: baseSource.length,
      intentHighWater: baseObserved.intentHighWater,
    );
    final baseFacts = <FlarkV3SourcePrefixFacts>[
      _canonicalFact(baseSource, 4),
      _canonicalFact(baseSource, baseSource.length),
    ];
    harness.transport.emit(
      FlarkV3ParserSourceFactsPage(
        eventId: 3,
        binding: binding,
        page: FlarkV3CanonicalSourceFactCheckpointPage(
          lineage: baseLineage,
          pageOrdinal: 0,
          pageCount: 1,
          checkpointCount: baseFacts.length,
          checkpointSpacingUtf16: 4,
          checkpoints: baseFacts,
        ),
      ),
    );
    harness.driver.pump();
    final baseOracle = FlarkV3SourceDocument.fromString(baseSource);
    harness.transport.emit(
      FlarkV3ParserSourceFactsCompleted(
        eventId: 4,
        binding: binding,
        completion: FlarkV3CanonicalSourceFactCompletion(
          lineage: baseLineage,
          fingerprintAlgorithm: 1,
          fingerprint: FlarkV3SourceFingerprint(
            revision: harness.session.uiRevision,
            utf16Length: baseSource.length,
            utf8Length: utf8.encode(baseSource).length,
            contentHash128: baseOracle.contentHash128,
          ),
          logicalLineBreaks: 0,
          checkpointSpacingUtf16: 4,
          checkpointCount: baseFacts.length,
          pageCount: 1,
          checkpointHash128: _portableFactsHash(baseFacts),
        ),
      ),
    );
    harness.driver.pump();
    final baseAuthority = sourceSession.installedCanonicalSourceFactAuthority!;
    final baseOffer = _publicationOffer(harness.session.sourceVersion);
    _beginPublication(harness, baseOffer, eventId: 5);
    final baseAck = _commitPublication(
      harness,
      baseOffer,
      packetEventId: 6,
      commitEventId: 7,
    );
    expect(
      identical(
        harness.session.retainedCanonicalSourceFactDeltaBase,
        baseAuthority,
      ),
      isTrue,
    );
    harness.transport.emit(
      FlarkV3ParserPublicationDeliveryAcknowledged(
        eventId: 8,
        binding: binding,
        ack: baseAck,
      ),
    );
    harness.driver.pump();

    harness.session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: harness.session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 4,
          endUtf16: 5,
          replacement: 'Z',
        ),
      ),
    );
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final targetLease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    final observed = FlarkV3ObservedSourceReplicaVersion(
      revision: switch (targetLease) {
        FlarkV3SourceSnapshotSyncLease() => targetLease.targetStamp.revision,
        FlarkV3SourceIntentSyncLease() => targetLease.targetStamp.revision,
      },
      utf16Length: targetSource.length,
      utf8Length: utf8.encode(targetSource).length,
      intentHighWater: switch (targetLease) {
        FlarkV3SourceSnapshotSyncLease() => targetLease.throughIntentSequence,
        FlarkV3SourceIntentSyncLease() => targetLease.lastSequence,
      },
    );
    harness.transport.emit(
      FlarkV3ParserSourceSynchronized(
        eventId: 9,
        workerGeneration: targetLease.workerGeneration,
        acknowledgement: switch (targetLease) {
          FlarkV3SourceSnapshotSyncLease() => targetLease.acknowledgement(
            observedReplica: targetLease.isLast ? observed : null,
          ),
          FlarkV3SourceIntentSyncLease() => targetLease.acknowledgement(
            observedReplica: observed,
          ),
        },
      ),
    );
    harness.driver.pump();
    expect(
      identical(
        harness.session.retainedCanonicalSourceFactDeltaBase,
        baseAuthority,
      ),
      isTrue,
    );

    final targetLineage = FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: binding.sourceSessionIdentity,
      requestId: 42,
      workerGeneration: binding.workerGeneration,
      workerReplicaRevision: observed.revision,
      uiRevision: harness.session.uiRevision,
      utf16Length: targetSource.length,
      intentHighWater: observed.intentHighWater,
    );
    final targetFacts = <FlarkV3SourcePrefixFacts>[
      _canonicalFact(targetSource, 4),
      _canonicalFact(targetSource, targetSource.length),
    ];
    const targetRootGuard = FlarkV3ContentHash128(101, 102, 103, 104);
    harness.transport.emit(
      FlarkV3ParserSourceFactsDeltaBegin(
        eventId: 10,
        binding: binding,
        header: FlarkV3ParserSourceFactsDeltaHeader(
          lineage: targetLineage,
          baseFingerprint: baseAuthority.fingerprint,
          baseCheckpointRootGuard128: baseAuthority.checkpointHash128,
          baseCheckpointCount: baseAuthority.checkpointCount,
          basePageCount: baseAuthority.pageCount,
          baseCheckpointSpacingUtf16: baseAuthority.checkpointSpacingUtf16,
          basePageStart: 0,
          basePageEnd: 1,
          targetPageStart: 0,
          targetPageEnd: 1,
          targetCheckpointCount: targetFacts.length,
          targetPageCount: 1,
          targetCheckpointRootGuardAlgorithm:
              flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
          targetCheckpointRootGuard128: targetRootGuard,
          replacementCheckpointCount: targetFacts.length,
        ),
      ),
    );
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.accepted,
    );

    final deltaPage = FlarkV3CanonicalSourceFactDeltaCheckpointPage(
      lineage: targetLineage,
      pageOrdinal: 0,
      checkpoints: targetFacts,
    );
    harness.transport.emit(
      FlarkV3ParserSourceFactsDeltaPage(
        eventId: 11,
        binding: binding,
        page: deltaPage,
      ),
    );
    harness.driver.pump();
    expect(deltaPage.isConsumed, isTrue);
    expect(harness.session.currentUiSourceCertified, isFalse);

    final targetOracle = FlarkV3SourceDocument.fromString(targetSource);
    harness.transport.emit(
      FlarkV3ParserSourceFactsDeltaCompleted(
        eventId: 12,
        binding: binding,
        completion: FlarkV3CanonicalSourceFactDeltaCompletion(
          lineage: targetLineage,
          fingerprintAlgorithm: 1,
          fingerprint: FlarkV3SourceFingerprint(
            revision: harness.session.uiRevision,
            utf16Length: targetSource.length,
            utf8Length: utf8.encode(targetSource).length,
            contentHash128: targetOracle.contentHash128,
          ),
          logicalLineBreaks: 0,
          checkpointSpacingUtf16: 4,
          checkpointCount: targetFacts.length,
          pageCount: 1,
          checkpointRootGuardAlgorithm:
              flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
          checkpointRootGuard128: targetRootGuard,
          replacementCheckpointHash128: _portableFactsHash(targetFacts),
        ),
      ),
    );
    harness.driver.pump();
    final receipt = harness.transport.last<FlarkV3ParserEventReceipt>();
    expect(receipt.disposition, FlarkV3ParserEventDisposition.accepted);
    expect(receipt.sourceCertification?.checkpointHash128, targetRootGuard);
    expect(harness.session.currentUiSourceCertified, isTrue);
    expect(harness.session.storeSourceSynchronized, isTrue);
    expect(
      harness.session.sourceVersion.contentHash,
      targetOracle.contentHash128,
    );
    expect(
      identical(
        harness.session.retainedCanonicalSourceFactDeltaBase,
        baseAuthority,
      ),
      isTrue,
    );

    const latestSource = 'abcdZYgh';
    harness.session.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: harness.session.uiRevision,
        operation: const FlarkV3SourceEdit(
          startUtf16: 5,
          endUtf16: 6,
          replacement: 'Y',
        ),
      ),
    );
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final latestLease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    final latestObserved = FlarkV3ObservedSourceReplicaVersion(
      revision: switch (latestLease) {
        FlarkV3SourceSnapshotSyncLease() => latestLease.targetStamp.revision,
        FlarkV3SourceIntentSyncLease() => latestLease.targetStamp.revision,
      },
      utf16Length: latestSource.length,
      utf8Length: utf8.encode(latestSource).length,
      intentHighWater: switch (latestLease) {
        FlarkV3SourceSnapshotSyncLease() => latestLease.throughIntentSequence,
        FlarkV3SourceIntentSyncLease() => latestLease.lastSequence,
      },
    );
    harness.transport.emit(
      FlarkV3ParserSourceSynchronized(
        eventId: 13,
        workerGeneration: latestLease.workerGeneration,
        acknowledgement: switch (latestLease) {
          FlarkV3SourceSnapshotSyncLease() => latestLease.acknowledgement(
            observedReplica: latestLease.isLast ? latestObserved : null,
          ),
          FlarkV3SourceIntentSyncLease() => latestLease.acknowledgement(
            observedReplica: latestObserved,
          ),
        },
      ),
    );
    harness.driver.pump();

    final latestFacts = <FlarkV3SourcePrefixFacts>[
      _canonicalFact(latestSource, 4),
      _canonicalFact(latestSource, latestSource.length),
    ];
    harness.transport.emit(
      FlarkV3ParserSourceFactsDeltaBegin(
        eventId: 14,
        binding: binding,
        header: FlarkV3ParserSourceFactsDeltaHeader(
          lineage: FlarkV3SourceCertificationLineage(
            sourceSessionIdentity: binding.sourceSessionIdentity,
            requestId: 43,
            workerGeneration: binding.workerGeneration,
            workerReplicaRevision: latestObserved.revision,
            uiRevision: harness.session.uiRevision,
            utf16Length: latestSource.length,
            intentHighWater: latestObserved.intentHighWater,
          ),
          baseFingerprint: baseAuthority.fingerprint,
          baseCheckpointRootGuard128: baseAuthority.checkpointHash128,
          baseCheckpointCount: baseAuthority.checkpointCount,
          basePageCount: baseAuthority.pageCount,
          baseCheckpointSpacingUtf16: baseAuthority.checkpointSpacingUtf16,
          basePageStart: 0,
          basePageEnd: 1,
          targetPageStart: 0,
          targetPageEnd: 1,
          targetCheckpointCount: latestFacts.length,
          targetPageCount: 1,
          targetCheckpointRootGuardAlgorithm:
              flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
          targetCheckpointRootGuard128: _portableFactsHash(latestFacts),
          replacementCheckpointCount: latestFacts.length,
        ),
      ),
    );
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.accepted,
    );
  });

  test('malformed current completion releases phantom credit for reseed', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final lease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;

    harness.transport.emit(
      FlarkV3ParserSourceSynchronized(
        eventId: 2,
        workerGeneration: lease.workerGeneration,
        acknowledgement: _wrongObservationAcknowledgement(lease),
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.driver.hasSourceLeaseInFlight, isFalse);
    expect(harness.session.hasPendingSourceWorkerSync, isTrue);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.stale,
    );

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final reseed = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    expect(reseed.leaseId, isNot(lease.leaseId));
    expect(reseed, isA<FlarkV3SourceSnapshotSyncLease>());
  });

  test('rapid edits drain the old lease then send only the latest rebase', () {
    final harness = _harness(
      FlarkV3SourceSession.fromString('a', workerJournalEntryLimit: 1),
    );
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);

    _append(harness.session, 'b');
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final oldIntent = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    expect(oldIntent, isA<FlarkV3SourceIntentSyncLease>());

    _append(harness.session, 'c');
    harness.driver.markDirty();
    expect(harness.session.source.toString(), 'abc');
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      hasLength(2),
    );

    harness.transport.emit(_ackEvent(3, oldIntent));
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.stale,
    );
    expect(harness.driver.hasSourceLeaseInFlight, isFalse);

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final latest =
        harness.transport.last<FlarkV3ParserSynchronizeSource>().lease
            as FlarkV3SourceSnapshotSyncLease;
    expect(latest.baseUiRevision, harness.session.uiRevision);
    expect(latest.source, 'abc');
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      hasLength(3),
    );

    harness.transport.emit(_ackEvent(4, latest));
    harness.driver.pump();
    expect(harness.session.sourceWorkerSynchronized, isTrue);
  });

  test(
    'restart requires a quiet terminal worker fault and reseeds recovery',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      harness.driver.pump();
      final oldLease = harness.transport
          .last<FlarkV3ParserSynchronizeSource>()
          .lease;

      expect(harness.driver.restart, throwsStateError);
      harness.transport.emit(
        FlarkV3ParserFailed(
          eventId: 2,
          workerGeneration: oldLease.workerGeneration,
          failureCode: 17,
        ),
      );
      expect(
        harness.driver.restart,
        throwsStateError,
        reason: 'the terminal event must return its credit before recovery',
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.parserEvent,
      );
      expect(harness.driver.state, FlarkV3SessionDriverState.faulted);
      expect(harness.driver.hasSourceLeaseInFlight, isFalse);

      final restart = harness.driver.restart();
      expect(restart.workerGeneration, oldLease.workerGeneration + 1);
      expect(harness.driver.hasSourceLeaseInFlight, isFalse);
      final recoveryOpen = harness.transport.last<FlarkV3ParserOpen>();
      expect(recoveryOpen.mode, FlarkV3ParserOpenMode.recovery);
      expect(recoveryOpen.binding.workerGeneration, restart.workerGeneration);
      expect(harness.driver.state, FlarkV3SessionDriverState.opening);
      expect(harness.driver.pump().action, FlarkV3SessionPumpAction.idle);

      harness.transport.emit(
        FlarkV3ParserOpened(
          eventId: 1,
          binding: recoveryOpen.binding,
          mode: recoveryOpen.mode,
        ),
      );
      harness.driver.pump();

      expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
      final currentLease =
          harness.transport.last<FlarkV3ParserSynchronizeSource>().lease
              as FlarkV3SourceSnapshotSyncLease;
      expect(currentLease.workerGeneration, restart.workerGeneration);
      expect(currentLease.source, 'live');

      harness.transport.emit(_ackEvent(2, currentLease));
      harness.driver.pump();
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.accepted,
      );
      expect(harness.session.sourceWorkerSynchronized, isTrue);
    },
  );

  test('restart cannot replace a live opening endpoint', () {
    final harness = _harness(
      FlarkV3SourceSession.fromString('live'),
      acknowledgeOpen: false,
    );
    addTearDown(harness.dispose);
    expect(harness.driver.state, FlarkV3SessionDriverState.opening);
    expect(harness.driver.restart, throwsStateError);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserOpen>(),
      hasLength(1),
    );
  });

  test('publication is credited through exact worker delivery ACK', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);

    final begin = _publicationOffer(harness.session.sourceVersion);
    harness.transport.emit(
      FlarkV3ParserPublicationBegin(
        eventId: 3,
        binding: harness.driver.parserBinding,
        begin: begin,
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.store.beginCount, 1);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.accepted,
    );

    final packet = _publicationPacket(begin);
    harness.transport.emit(
      FlarkV3ParserPublicationPacket(
        eventId: 4,
        binding: harness.driver.parserBinding,
        packet: packet,
      ),
    );
    harness.driver.pump();
    final packetTicket = harness.driver.activeHostPollTicket!;
    expect(packetTicket.binding, harness.driver.parserBinding);
    expect(packetTicket.pollTicket, 4);
    expect(packetTicket.offerId, begin.offerId);
    expect(packetTicket.phase, FlarkV3ParserHostPollPhase.packetCredit);
    harness.store.pendingPolls = 1;
    final pollsBefore = harness.store.pollCount;
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.store.pollCount, pollsBefore + 1);
    expect(harness.driver.hasPendingHostPoll, isTrue);
    expect(harness.driver.activeHostPollTicket, packetTicket);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    final packetCompletion = harness.transport
        .last<FlarkV3ParserHostPollCompleted>();
    expect(packetCompletion.ticket, packetTicket);
    expect(packetCompletion.outcome, isA<FlarkV3HostPacketCredit>());
    expect(
      harness.driver.publicationState,
      FlarkV3PublicationDriverState.acceptingPackets,
    );

    harness.transport.emit(
      FlarkV3ParserPublicationCommitRequested(
        eventId: 5,
        binding: harness.driver.parserBinding,
        request: _publicationCommit(begin, packet),
      ),
    );
    harness.driver.pump();
    expect(harness.driver.activeHostPollTicket?.pollTicket, 5);
    expect(
      harness.driver.activeHostPollTicket?.phase,
      FlarkV3ParserHostPollPhase.commit,
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    final commitCompletion = harness.transport
        .last<FlarkV3ParserHostPollCompleted>();
    expect(commitCompletion.pollTicket, 5);
    final committed = commitCompletion.outcome as FlarkV3HostCommitted;
    final ack = committed.ack;
    expect(ack.sourceVersion, begin.sourceVersion);
    expect(ack.sourceRoot, begin.sourceRoot);
    expect(ack.parseGeneration, begin.parseGeneration);
    expect(ack.grammarRevision, begin.grammarRevision);
    expect(ack.syntaxProfile, begin.syntaxProfile);
    expect(ack.authorityMask, begin.authorityMask);
    expect(harness.session.pendingDeliveryAck, ack);
    expect(harness.store.deliveryAckCount, 0);

    final wrongAck = _copyAck(ack, parseGeneration: ack.parseGeneration + 1);
    harness.transport.emit(
      FlarkV3ParserPublicationDeliveryAcknowledged(
        eventId: 6,
        binding: harness.driver.parserBinding,
        ack: wrongAck,
      ),
    );
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
    );
    expect(harness.store.deliveryAckCount, 0);
    expect(harness.session.pendingDeliveryAck, ack);

    harness.transport.emit(
      FlarkV3ParserPublicationDeliveryAcknowledged(
        eventId: 7,
        binding: harness.driver.parserBinding,
        ack: ack,
      ),
    );
    harness.driver.pump();
    expect(harness.store.deliveryAckCount, 1);
    expect(harness.session.pendingDeliveryAck, isNull);
    expect(harness.driver.publicationState, FlarkV3PublicationDriverState.idle);
  });

  test('structural and sidecar callbacks share one event-credit cell', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);
    final base = _installAndDeliverStructuralBase(harness);

    expect(harness.driver.requestInlineRefinement(utf16Offset: 2), 1);
    expect(
      harness.driver.pump().action,
      FlarkV3SessionPumpAction.inlineRefinementRequest,
    );
    final begin = _inlineSidecarOffer(base, refinementGeneration: 1);
    harness.transport.emitInlineSidecar(
      FlarkV3ParserInlineSidecarBegin(
        eventId: 7,
        binding: harness.driver.parserBinding,
        begin: begin,
      ),
    );

    expect(
      () => harness.transport.emit(
        FlarkV3ParserPublicationBegin(
          eventId: 8,
          binding: harness.driver.parserBinding,
          begin: _publicationOffer(harness.session.sourceVersion),
        ),
      ),
      throwsStateError,
    );
    expect(harness.driver.hasPendingParserEvent, isTrue);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    final receipt = harness.transport.last<FlarkV3ParserEventReceipt>();
    expect(receipt.eventId, 7);
    expect(receipt.binding, harness.driver.parserBinding);
    expect(receipt.disposition, FlarkV3ParserEventDisposition.accepted);
  });

  test(
    'passive viewport demand coalesces and focused inline demand preempts it',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);
      _installAndDeliverStructuralBase(harness);

      final limits = FlarkV3ParserViewportPresentationLimits();
      expect(
        harness.driver.requestViewportPresentation(
          requestedStartUtf8: 0,
          requestedStartUtf16: 0,
          requestedEndUtf8: 4,
          requestedEndUtf16: 4,
          startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
          limits: limits,
        ),
        1,
      );
      expect(
        harness.driver.requestViewportPresentation(
          requestedStartUtf8: 0,
          requestedStartUtf16: 0,
          requestedEndUtf8: 4,
          requestedEndUtf16: 4,
          startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
          limits: limits,
        ),
        2,
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.viewportPresentationRequest,
      );
      expect(
        harness.transport.commands
            .whereType<FlarkV3ParserPresentViewport>()
            .single
            .viewportGeneration,
        2,
      );
      expect(harness.driver.viewportPresentationAttemptOutcomeGeneration, 0);
      harness.transport.emit(
        FlarkV3ParserViewportPresentationUnavailable(
          eventId: 7,
          binding: harness.driver.parserBinding,
          viewportGeneration: 2,
          reasonCode:
              FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason,
        ),
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.parserEvent,
      );
      expect(harness.driver.viewportPresentationAttemptOutcomeGeneration, 1);
      expect(harness.driver.lastViewportPresentationUnavailableGeneration, 2);
      expect(
        harness.driver.lastViewportPresentationUnavailableReason,
        FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason,
      );
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.accepted,
      );

      expect(
        harness.driver.requestViewportPresentation(
          requestedStartUtf8: 0,
          requestedStartUtf16: 0,
          requestedEndUtf8: 4,
          requestedEndUtf16: 4,
          startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
          limits: limits,
        ),
        3,
      );
      expect(
        harness.driver.lastViewportPresentationUnavailableGeneration,
        isNull,
      );
      expect(harness.driver.lastViewportPresentationUnavailableReason, isNull);
      expect(harness.driver.requestInlineRefinement(utf16Offset: 2), 1);
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.inlineRefinementRequest,
      );
      expect(
        harness.transport.commands.whereType<FlarkV3ParserPresentViewport>(),
        hasLength(1),
      );
    },
  );

  test('viewport unavailability remains bound to the issued generation when a '
      'newer demand is pending', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);
    _installAndDeliverStructuralBase(harness);

    final limits = FlarkV3ParserViewportPresentationLimits();
    expect(
      harness.driver.requestViewportPresentation(
        requestedStartUtf8: 0,
        requestedStartUtf16: 0,
        requestedEndUtf8: 4,
        requestedEndUtf16: 4,
        startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
        limits: limits,
      ),
      1,
    );
    expect(
      harness.driver.pump().action,
      FlarkV3SessionPumpAction.viewportPresentationRequest,
    );
    expect(
      harness.driver.requestViewportPresentation(
        requestedStartUtf8: 0,
        requestedStartUtf16: 0,
        requestedEndUtf8: 4,
        requestedEndUtf16: 4,
        startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
        limits: limits,
      ),
      2,
    );

    harness.transport.emit(
      FlarkV3ParserViewportPresentationUnavailable(
        eventId: 7,
        binding: harness.driver.parserBinding,
        viewportGeneration: 1,
        reasonCode:
            FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason,
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.driver.viewportPresentationAttemptOutcomeGeneration, 1);
    expect(harness.driver.lastViewportPresentationUnavailableGeneration, 1);
    expect(
      harness.driver.lastViewportPresentationUnavailableReason,
      FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason,
    );

    expect(
      harness.driver.pump().action,
      FlarkV3SessionPumpAction.viewportPresentationRequest,
    );
    expect(
      harness.transport.commands
          .whereType<FlarkV3ParserPresentViewport>()
          .last
          .viewportGeneration,
      2,
    );
  });

  test(
    'focused inline demand preempts a pending viewport host poll without starvation',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);
      final base = _installAndDeliverStructuralBase(harness);

      expect(
        harness.driver.requestViewportPresentation(
          requestedStartUtf8: 0,
          requestedStartUtf16: 0,
          requestedEndUtf8: 4,
          requestedEndUtf16: 4,
          startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
          limits: FlarkV3ParserViewportPresentationLimits(),
        ),
        1,
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.viewportPresentationRequest,
      );
      final begin = _viewportPresentationOffer(base, viewportGeneration: 1);
      harness.transport.emitViewportPresentation(
        FlarkV3ParserViewportPresentationBegin(
          eventId: 7,
          binding: harness.driver.parserBinding,
          begin: begin,
        ),
      );
      harness.driver.pump();
      expect(
        harness.driver.viewportPresentationPublicationState,
        FlarkV3ViewportPresentationPublicationDriverState.acceptingPackets,
      );

      expect(harness.driver.requestInlineRefinement(utf16Offset: 2), 1);
      final waitingForViewportEvent = harness.driver.pump();
      expect(waitingForViewportEvent.action, FlarkV3SessionPumpAction.idle);
      expect(
        waitingForViewportEvent.needsMoreWork,
        isFalse,
        reason:
            'focused work must not enter the deferred command cell after '
            'viewport Begin has escaped',
      );

      final packet = testPublicationPacket(
        offerId: begin.offerId,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: 1,
        digest: FlarkV3ProtocolDigest128(91, 92, 93, 94),
        frameBytes: Uint8List.fromList(const <int>[1, 2, 3, 4]),
      );
      harness.transport.emitViewportPresentation(
        FlarkV3ParserViewportPresentationPacket(
          eventId: 8,
          binding: harness.driver.parserBinding,
          packet: packet,
        ),
      );
      harness.driver.pump();
      expect(
        harness.driver.viewportPresentationPublicationState,
        FlarkV3ViewportPresentationPublicationDriverState.awaitingPacketCredit,
      );

      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.viewportPresentationHostPoll,
      );
      final rejected = harness.transport
          .lastViewport<FlarkV3ParserViewportPresentationHostPollRejected>();
      expect(rejected.ticket.pollTicket, 8);
      expect(rejected.reason, FlarkV3HostRejectReason.superseded);
      expect(
        harness.driver.viewportPresentationPublicationState,
        FlarkV3ViewportPresentationPublicationDriverState.idle,
      );

      harness.transport.emit(
        FlarkV3ParserViewportPresentationUnavailable(
          eventId: 9,
          binding: harness.driver.parserBinding,
          viewportGeneration: 1,
          reasonCode:
              FlarkV3ParserViewportPresentationUnavailable.hostRejectedReason,
        ),
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.parserEvent,
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.inlineRefinementRequest,
      );
      expect(
        harness.transport
            .last<FlarkV3ParserRefineInline>()
            .refinementGeneration,
        1,
      );
    },
  );

  test(
    'retryable inline unavailability terminalizes one attempt exactly once',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);
      final base = _installAndDeliverStructuralBase(harness);

      expect(harness.driver.requestInlineRefinement(utf16Offset: 2), 1);
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.inlineRefinementRequest,
      );
      expect(harness.driver.inlineAttemptOutcomeGeneration, 0);

      harness.transport.emit(
        FlarkV3ParserInlineRefinementUnavailable(
          eventId: 7,
          binding: harness.driver.parserBinding,
          refinementGeneration: 1,
          reasonCode:
              FlarkV3ParserInlineRefinementUnavailable.retryableBusyReason,
        ),
      );
      harness.driver.pump();

      expect(harness.driver.state, FlarkV3SessionDriverState.open);
      expect(harness.driver.inlineAttemptOutcomeGeneration, 1);
      expect(
        harness.driver.inlineSidecarPublicationState,
        FlarkV3InlineSidecarPublicationDriverState.idle,
      );
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.accepted,
      );

      harness.transport.emit(
        FlarkV3ParserInlineRefinementUnavailable(
          eventId: 8,
          binding: harness.driver.parserBinding,
          refinementGeneration: 1,
          reasonCode:
              FlarkV3ParserInlineRefinementUnavailable.retryableBusyReason,
        ),
      );
      harness.driver.pump();
      expect(harness.driver.inlineAttemptOutcomeGeneration, 1);
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.rejected,
      );

      expect(harness.driver.requestInlineRefinement(utf16Offset: 3), 2);
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.inlineRefinementRequest,
      );
      harness.transport.emitInlineSidecar(
        FlarkV3ParserInlineSidecarBegin(
          eventId: 9,
          binding: harness.driver.parserBinding,
          begin: _inlineSidecarOffer(base, refinementGeneration: 1),
        ),
      );
      harness.driver.pump();
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.rejected,
        reason: 'the terminalized generation cannot later open a sidecar',
      );
      expect(harness.driver.inlineAttemptOutcomeGeneration, 1);
    },
  );

  test(
    'pending inline waits for committed viewport delivery acknowledgement',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);
      final base = _installAndDeliverStructuralBase(harness);
      final limits = FlarkV3ParserViewportPresentationLimits();

      expect(
        harness.driver.requestViewportPresentation(
          requestedStartUtf8: 0,
          requestedStartUtf16: 0,
          requestedEndUtf8: 4,
          requestedEndUtf16: 4,
          startBlockOrdinal: FlarkV3ProtocolU64.fromU32(0),
          limits: limits,
        ),
        1,
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.viewportPresentationRequest,
      );
      final begin = _viewportPresentationOffer(base, viewportGeneration: 1);
      harness.transport.emitViewportPresentation(
        FlarkV3ParserViewportPresentationBegin(
          eventId: 7,
          binding: harness.driver.parserBinding,
          begin: begin,
        ),
      );
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.parserEvent,
      );
      expect(
        harness.driver.viewportPresentationPublicationState,
        FlarkV3ViewportPresentationPublicationDriverState.acceptingPackets,
      );

      final packet = testPublicationPacket(
        offerId: begin.offerId,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: 1,
        digest: FlarkV3ProtocolDigest128(91, 92, 93, 94),
        frameBytes: Uint8List.fromList(const <int>[1, 2, 3, 4]),
      );
      harness.transport.emitViewportPresentation(
        FlarkV3ParserViewportPresentationPacket(
          eventId: 8,
          binding: harness.driver.parserBinding,
          packet: packet,
        ),
      );
      harness.driver.pump();
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.viewportPresentationHostPoll,
      );
      expect(
        harness.transport
            .lastViewport<FlarkV3ParserViewportPresentationHostPollCompleted>()
            .outcome,
        isA<FlarkV3ViewportPresentationHostPacketCredit>(),
      );

      final commit = FlarkV3ViewportPresentationCommitRequest(
        offerId: begin.offerId,
        actualFrameCount: 6,
        actualEncodedFrameBytes: 400,
        rollingTransportDigest: FlarkV3ProtocolDigest128(101, 102, 103, 104),
        aggregateRootStreamDigest: FlarkV3ProtocolDigest128(111, 112, 113, 114),
      );
      harness.transport.emitViewportPresentation(
        FlarkV3ParserViewportPresentationCommitRequested(
          eventId: 9,
          binding: harness.driver.parserBinding,
          request: commit,
        ),
      );
      harness.driver.pump();
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.viewportPresentationHostPoll,
      );
      final committed =
          harness.transport
                  .lastViewport<
                    FlarkV3ParserViewportPresentationHostPollCompleted
                  >()
                  .outcome
              as FlarkV3ViewportPresentationHostCommitted;
      expect(harness.driver.viewportPresentationAttemptOutcomeGeneration, 1);
      expect(
        harness.driver.viewportPresentationPublicationState,
        FlarkV3ViewportPresentationPublicationDriverState.awaitingDeliveryAck,
      );

      expect(harness.driver.requestInlineRefinement(utf16Offset: 2), 1);
      final blocked = harness.driver.pump();
      expect(
        blocked.action,
        FlarkV3SessionPumpAction.idle,
        reason:
            'a point request cannot overtake the viewport delivery event '
            'created by its completed host commit',
      );
      expect(
        blocked.needsMoreWork,
        isFalse,
        reason: 'waiting on parser delivery must not busy-spin the host pump',
      );

      harness.transport.emitViewportPresentation(
        FlarkV3ParserViewportPresentationDeliveryAcknowledged(
          eventId: 10,
          binding: harness.driver.parserBinding,
          ack: committed.ack,
        ),
      );
      harness.driver.pump();
      expect(
        harness.driver.viewportPresentationPublicationState,
        FlarkV3ViewportPresentationPublicationDriverState.idle,
      );
      expect(harness.session.pendingViewportPresentationDeliveryAck, isNull);
      expect(
        harness.driver.pump().action,
        FlarkV3SessionPumpAction.inlineRefinementRequest,
      );
      expect(
        harness.transport.commands.whereType<FlarkV3ParserHostPollCompleted>(),
        hasLength(2),
        reason: 'VPB1 cannot enter structural host-poll commands.',
      );
    },
  );

  test(
    'sidecar commit advances presentation once and delivery clears only its lane',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);
      final structuralAck = _installAndDeliverStructuralBase(harness);
      expect(harness.driver.inlineAttemptOutcomeGeneration, 0);
      final sidecar = _commitInlineSidecar(
        harness,
        structuralAck,
        refinementGeneration: 1,
        beginEventId: 7,
        packetEventId: 8,
        commitEventId: 9,
      );

      expect(harness.driver.inlinePresentationGeneration, 1);
      expect(harness.driver.inlineAttemptOutcomeGeneration, 1);
      expect(
        harness.driver.inlineSidecarPublicationState,
        FlarkV3InlineSidecarPublicationDriverState.awaitingDeliveryAck,
      );
      expect(harness.session.pendingInlineSidecarDeliveryAck, sidecar);
      expect(harness.session.pendingDeliveryAck, isNull);
      expect(
        harness.driver.publicationState,
        FlarkV3PublicationDriverState.idle,
      );
      expect(
        (harness.session.presentationState
                as FlarkV3ExactStructuralPresentation)
            .ack,
        structuralAck,
        reason: 'a sidecar commit cannot replace structural authority',
      );
      expect(
        harness.transport.commands.whereType<FlarkV3ParserHostPollCompleted>(),
        hasLength(2),
        reason: 'only the earlier structural packet and commit use this lane',
      );
      expect(
        harness.transport.inlineSidecarCommands
            .whereType<FlarkV3ParserInlineSidecarHostPollCompleted>(),
        hasLength(2),
      );

      harness.transport.emitInlineSidecar(
        FlarkV3ParserInlineSidecarDeliveryAcknowledged(
          eventId: 10,
          binding: harness.driver.parserBinding,
          ack: sidecar,
        ),
      );
      harness.driver.pump();

      expect(harness.driver.inlinePresentationGeneration, 1);
      expect(
        harness.driver.inlineAttemptOutcomeGeneration,
        1,
        reason: 'delivery ACK is backpressure, not another attempt outcome',
      );
      expect(
        harness.driver.inlineSidecarPublicationState,
        FlarkV3InlineSidecarPublicationDriverState.idle,
      );
      expect(harness.session.pendingInlineSidecarDeliveryAck, isNull);
      expect(harness.store.inlineSidecarDeliveryAckCount, 1);
      expect(harness.session.pendingDeliveryAck, isNull);
      expect(
        (harness.session.presentationState
                as FlarkV3ExactStructuralPresentation)
            .ack,
        structuralAck,
      );
    },
  );

  test('newer sidecar requests replace through typed failure and abort', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);
    final structuralAck = _installAndDeliverStructuralBase(harness);
    final installed = _commitInlineSidecar(
      harness,
      structuralAck,
      refinementGeneration: 1,
      beginEventId: 7,
      packetEventId: 8,
      commitEventId: 9,
    );
    expect(harness.driver.inlineAttemptOutcomeGeneration, 1);
    harness.transport.emitInlineSidecar(
      FlarkV3ParserInlineSidecarDeliveryAcknowledged(
        eventId: 10,
        binding: harness.driver.parserBinding,
        ack: installed,
      ),
    );
    harness.driver.pump();

    expect(harness.driver.requestInlineRefinement(utf16Offset: 1), 2);
    harness.driver.pump();
    final replacement = _inlineSidecarOffer(
      structuralAck,
      refinementGeneration: 2,
      offerWord0: 62,
      publicationWord0: 72,
    );
    harness.transport.emitInlineSidecar(
      FlarkV3ParserInlineSidecarBegin(
        eventId: 11,
        binding: harness.driver.parserBinding,
        begin: replacement,
      ),
    );
    harness.driver.pump();
    final failure = FlarkV3ParserInlineSidecarFailed(
      eventId: 12,
      binding: harness.driver.parserBinding,
      offerId: replacement.offerId,
      failureCode: 9,
    );
    harness.transport.emitInlineSidecar(failure);
    harness.driver.pump();
    expect(harness.driver.lastInlineSidecarFailure, same(failure));
    expect(
      harness.driver.inlineAttemptOutcomeGeneration,
      1,
      reason: 'failure starts abort but is not a second terminal outcome',
    );
    expect(
      harness.driver.inlineSidecarPublicationState,
      FlarkV3InlineSidecarPublicationDriverState.aborting,
    );
    expect(
      harness.driver.pump().action,
      FlarkV3SessionPumpAction.inlineSidecarHostPoll,
    );
    expect(
      harness.transport
          .lastInlineSidecar<FlarkV3ParserInlineSidecarHostPollCompleted>()
          .outcome,
      isA<FlarkV3InlineSidecarHostAbortComplete>(),
    );
    expect(harness.driver.inlineAttemptOutcomeGeneration, 2);

    expect(harness.driver.requestInlineRefinement(utf16Offset: 3), 3);
    harness.driver.pump();
    final next = _inlineSidecarOffer(
      structuralAck,
      refinementGeneration: 3,
      offerWord0: 63,
      publicationWord0: 73,
    );
    harness.transport.emitInlineSidecar(
      FlarkV3ParserInlineSidecarBegin(
        eventId: 13,
        binding: harness.driver.parserBinding,
        begin: next,
      ),
    );
    harness.driver.pump();
    harness.transport.emitInlineSidecar(
      FlarkV3ParserInlineSidecarAbortRequested(
        eventId: 14,
        binding: harness.driver.parserBinding,
        offerId: next.offerId,
      ),
    );
    harness.driver.pump();
    expect(
      harness.driver.inlineAttemptOutcomeGeneration,
      2,
      reason: 'abort request is not complete until the host poll finishes',
    );
    harness.driver.pump();

    expect(
      harness.driver.inlineSidecarPublicationState,
      FlarkV3InlineSidecarPublicationDriverState.idle,
    );
    expect(harness.driver.inlinePresentationGeneration, 1);
    expect(harness.driver.inlineAttemptOutcomeGeneration, 3);
    expect(harness.store.inlineSidecarBeginCount, 3);
    expect(
      (harness.session.presentationState as FlarkV3ExactStructuralPresentation)
          .ack,
      structuralAck,
    );
  });

  test('default packet poll uses the bounded foreground quantum', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);

    final begin = _publicationOffer(harness.session.sourceVersion);
    _beginPublication(harness, begin, eventId: 3);
    harness.transport.emit(
      FlarkV3ParserPublicationPacket(
        eventId: 4,
        binding: harness.driver.parserBinding,
        packet: _publicationPacket(begin),
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);

    final grant = harness.store.lastPollGrant!;
    expect(grant.inspectBytes, 16 * 1024);
    expect(grant.copyBytes, 16 * 1024);
    expect(grant.transitions, 32);
  });

  test(
    'crossed same-generation publication bindings never reach host state',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);
      final begin = _publicationOffer(harness.session.sourceVersion);
      final current = harness.driver.parserBinding;
      final crossed = [
        FlarkV3ParserSessionBinding(
          documentSession: current.documentSession,
          sourceSessionIdentity: current.sourceSessionIdentity + 1,
          workerGeneration: current.workerGeneration,
        ),
        FlarkV3ParserSessionBinding(
          documentSession: FlarkV3DocumentSessionId(201, 202, 203, 204),
          sourceSessionIdentity: current.sourceSessionIdentity,
          workerGeneration: current.workerGeneration,
        ),
      ];

      for (var index = 0; index < crossed.length; index += 1) {
        harness.transport.emit(
          FlarkV3ParserPublicationBegin(
            eventId: 3 + index,
            binding: crossed[index],
            begin: begin,
          ),
        );
        harness.driver.pump();
        expect(harness.store.beginCount, 0);
        final receipt = harness.transport.last<FlarkV3ParserEventReceipt>();
        expect(receipt.binding, crossed[index]);
        expect(receipt.disposition, FlarkV3ParserEventDisposition.stale);
      }

      // Rejected crossed traffic never advances the current endpoint event lane.
      _beginPublication(harness, begin, eventId: 3);
      expect(harness.store.beginCount, 1);
    },
  );

  test('faults on packet credit whose next frame ordinal is not exact', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);

    final begin = _publicationOffer(harness.session.sourceVersion);
    _beginPublication(harness, begin, eventId: 3);
    harness.transport.emit(
      FlarkV3ParserPublicationPacket(
        eventId: 4,
        binding: harness.driver.parserBinding,
        packet: _publicationPacket(begin),
      ),
    );
    harness.driver.pump();
    harness.store.packetCreditOrdinalOverride = 2;
    harness.driver.pump();

    expect(harness.driver.state, FlarkV3SessionDriverState.faulted);
    expect(
      harness.driver.lastHostRejection?.reason,
      FlarkV3HostRejectReason.invalid,
    );
    expect(
      harness.transport.last<FlarkV3ParserHostPollRejected>().reason,
      FlarkV3HostRejectReason.invalid,
    );
    expect(
      harness.transport.last<FlarkV3ParserHostPollRejected>().pollTicket,
      4,
    );
  });

  test('failure after commit recovers through a new full-snapshot session', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);

    final original = _publicationOffer(harness.session.sourceVersion);
    _beginPublication(harness, original, eventId: 3);
    final originalAck = _commitPublication(
      harness,
      original,
      packetEventId: 4,
      commitEventId: 5,
    );
    expect(harness.session.pendingDeliveryAck, originalAck);
    expect(harness.store.deliveryAckCount, 0);

    final generation = harness.driver.workerGeneration!;
    final failure = FlarkV3ParserFailed(
      eventId: 6,
      workerGeneration: generation,
      failureCode: 17,
    );
    harness.transport.emit(failure);
    harness.driver.pump();

    expect(harness.driver.state, FlarkV3SessionDriverState.faulted);
    expect(harness.driver.publicationState, FlarkV3PublicationDriverState.idle);
    expect(harness.session.pendingDeliveryAck, originalAck);
    expect(harness.store.deliveryAckCount, 0);
    expect(
      (harness.session.presentationState as FlarkV3ExactStructuralPresentation)
          .ack,
      originalAck,
      reason: 'the exact installed root remains available during recovery',
    );

    final restart = harness.driver.restart();
    expect(restart.workerGeneration, generation + 1);
    _acknowledgeCurrentOpen(harness, eventId: 1);
    _synchronizeCurrentLease(harness, eventId: 2);

    _append(harness.session, '!');
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final editedLease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;
    harness.transport.emit(_ackEvent(3, editedLease));
    harness.driver.pump();

    final replacementSession = FlarkV3PublicationSessionId(31, 32, 33, 34);
    final falseBase = _copyAck(
      originalAck,
      publicationSession: replacementSession,
    );
    final recoveryDelta = _publicationDelta(
      harness.session.sourceVersion,
      base: falseBase,
      publicationSession: FlarkV3PublicationSessionId(41, 42, 43, 44),
    );
    harness.transport.emit(
      FlarkV3ParserPublicationBegin(
        eventId: 4,
        binding: harness.driver.parserBinding,
        begin: recoveryDelta,
      ),
    );
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
      reason: 'recovery cannot continue from any claimed delta base',
    );
    expect(harness.store.beginCount, 1);

    final sameSession = _publicationOffer(harness.session.sourceVersion);
    harness.transport.emit(
      FlarkV3ParserPublicationBegin(
        eventId: 5,
        binding: harness.driver.parserBinding,
        begin: sameSession,
      ),
    );
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
    );
    expect(harness.store.beginCount, 1);

    final replacement = _publicationOffer(
      harness.session.sourceVersion,
      publicationSession: replacementSession,
      offerWord0: 41,
      sourceRoot: FlarkV3SourceRootId(0, 202),
    );
    _beginPublication(harness, replacement, eventId: 6);
    expect(
      (harness.session.presentationState as FlarkV3StablePendingPresentation)
          .stablePaintAck,
      originalAck,
      reason: 'staging cannot withdraw the prior exact paint root',
    );
    final replacementAck = _commitPublication(
      harness,
      replacement,
      packetEventId: 7,
      commitEventId: 8,
    );
    expect(harness.session.pendingDeliveryAck, replacementAck);
    expect(harness.store.deliveryAckCount, 0);

    harness.transport.emit(
      FlarkV3ParserPublicationDeliveryAcknowledged(
        eventId: 9,
        binding: harness.driver.parserBinding,
        ack: originalAck,
      ),
    );
    harness.driver.pump();
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
    );
    expect(harness.store.deliveryAckCount, 0);

    harness.transport.emit(
      FlarkV3ParserPublicationDeliveryAcknowledged(
        eventId: 10,
        binding: harness.driver.parserBinding,
        ack: replacementAck,
      ),
    );
    harness.driver.pump();
    expect(harness.store.deliveryAckCount, 1);
    expect(harness.session.pendingDeliveryAck, isNull);
    expect(harness.driver.publicationState, FlarkV3PublicationDriverState.idle);
  });

  test(
    'unconfigured publication profile and unknown authority fail closed',
    () {
      expect(() => FlarkV3StructuralAuthorityMask(1 << 12), throwsRangeError);
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      _synchronizeCurrentLease(harness, eventId: 2);

      final begin = _publicationOffer(
        harness.session.sourceVersion,
        syntaxProfile: FlarkV3SyntaxProfileId(2),
      );
      harness.transport.emit(
        FlarkV3ParserPublicationBegin(
          eventId: 3,
          binding: harness.driver.parserBinding,
          begin: begin,
        ),
      );
      harness.driver.pump();

      expect(harness.store.beginCount, 0);
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.rejected,
      );
    },
  );

  test('publication failure aborts through a typed host completion', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);
    final begin = _publicationOffer(harness.session.sourceVersion);
    harness.transport.emit(
      FlarkV3ParserPublicationBegin(
        eventId: 3,
        binding: harness.driver.parserBinding,
        begin: begin,
      ),
    );
    harness.driver.pump();

    final failure = FlarkV3ParserPublicationFailed(
      eventId: 4,
      binding: harness.driver.parserBinding,
      offerId: begin.offerId,
      failureCode: 7,
    );
    harness.transport.emit(failure);
    harness.driver.pump();
    expect(harness.driver.lastPublicationFailure, same(failure));
    expect(
      harness.driver.publicationState,
      FlarkV3PublicationDriverState.aborting,
    );
    expect(harness.driver.activeHostPollTicket?.pollTicket, failure.eventId);
    expect(
      harness.driver.activeHostPollTicket?.phase,
      FlarkV3ParserHostPollPhase.abort,
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    final completion = harness.transport.last<FlarkV3ParserHostPollCompleted>();
    expect(completion.ticket.pollTicket, failure.eventId);
    expect(completion.outcome, isA<FlarkV3HostAbortComplete>());
    expect(harness.driver.publicationState, FlarkV3PublicationDriverState.idle);
  });

  test('new source coalesces staging supersession into latest sync', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);
    final begin = _publicationOffer(harness.session.sourceVersion);
    harness.transport.emit(
      FlarkV3ParserPublicationBegin(
        eventId: 3,
        binding: harness.driver.parserBinding,
        begin: begin,
      ),
    );
    harness.driver.pump();

    _append(harness.session, '!');
    harness.driver.markDirty();

    expect(harness.driver.publicationState, FlarkV3PublicationDriverState.idle);
    expect(harness.driver.hasPendingHostPoll, isFalse);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSupersede>(),
      isEmpty,
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSupersede>(),
      isEmpty,
    );
  });

  test('newer source waits behind one older live source command', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);

    _append(harness.session, '!');
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    final olderLease = harness.transport
        .last<FlarkV3ParserSynchronizeSource>()
        .lease;

    _append(harness.session, '?');
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.idle);

    _append(harness.session, '#');
    harness.driver.markDirty();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.idle);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSupersede>(),
      isEmpty,
      reason: 'The older source command already owns the speculative slot.',
    );
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      hasLength(2),
      reason: 'Initial synchronization plus the one older live edit command.',
    );

    harness.transport.emit(_ackEvent(3, olderLease));
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSupersede>(),
      isEmpty,
    );
    expect(
      harness.transport.commands.whereType<FlarkV3ParserSynchronizeSource>(),
      hasLength(3),
    );
  });

  test('close waits for both parser closed and host drained', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    harness.driver.pump();
    final generation = harness.driver.workerGeneration!;

    harness.driver.beginClose();
    harness.driver.beginClose();
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(harness.store.closeCount, 1);
    expect(harness.driver.hasSourceLeaseInFlight, isFalse);
    expect(harness.transport.last<FlarkV3ParserBeginClose>(), isNotNull);
    expect(
      harness.transport.commands.whereType<FlarkV3ParserBeginClose>(),
      hasLength(1),
    );

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserDrain);
    final drain = harness.transport.last<FlarkV3ParserDrainGrant>();
    expect(drain.binding, harness.driver.parserBinding);
    expect(drain.maximumTransitions, lessThanOrEqualTo(256));

    harness.transport.emit(
      FlarkV3ParserClosed(eventId: 2, workerGeneration: generation),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(harness.driver.parserClosed, isFalse);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
    );

    harness.transport.emit(
      FlarkV3ParserDrainProgress(
        eventId: 3,
        binding: drain.binding,
        drainId: drain.drainId,
        releasedSourceLeases: 1,
        releasedSourceBytes: 4,
        arenaTransitions: 1,
        arenaNodesReclaimed: 2,
        complete: true,
      ),
    );
    harness.driver.pump();
    expect(harness.driver.parserDrained, isTrue);
    harness.transport.emit(
      FlarkV3ParserClosed(eventId: 4, workerGeneration: generation),
    );
    harness.driver.pump();
    expect(harness.driver.parserClosed, isTrue);
    expect(harness.driver.hostDrained, isFalse);
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(harness.transport.closed, isFalse);

    harness.store.closePendingPolls = 1;
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(harness.transport.closed, isFalse);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.driver.hostDrained, isTrue);
    expect(harness.driver.state, FlarkV3SessionDriverState.closed);
    expect(harness.transport.closed, isTrue);
    expect(harness.store.closeCount, 1);
  });

  test(
    'close latches only after an already-credited source ACK is applied',
    () {
      final harness = _harness(FlarkV3SourceSession.fromString('live'));
      addTearDown(harness.dispose);
      expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
      final lease = harness.transport
          .last<FlarkV3ParserSynchronizeSource>()
          .lease;
      harness.transport.emit(_ackEvent(2, lease));
      expect(harness.driver.hasPendingParserEvent, isTrue);

      harness.driver.beginClose();
      harness.driver.beginClose();
      expect(harness.driver.state, FlarkV3SessionDriverState.open);
      expect(harness.driver.hasSourceLeaseInFlight, isTrue);
      expect(harness.store.closeCount, 0);
      expect(
        harness.transport.commands.whereType<FlarkV3ParserBeginClose>(),
        isEmpty,
      );

      final eventPump = harness.driver.pump();
      expect(eventPump.action, FlarkV3SessionPumpAction.parserEvent);
      expect(eventPump.needsMoreWork, isTrue);
      expect(harness.session.sourceWorkerSynchronized, isTrue);
      expect(harness.driver.hasSourceLeaseInFlight, isFalse);
      expect(
        harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
        FlarkV3ParserEventDisposition.accepted,
      );
      expect(harness.driver.state, FlarkV3SessionDriverState.open);
      expect(harness.store.closeCount, 0);

      expect(harness.driver.pump().action, FlarkV3SessionPumpAction.closeLatch);
      expect(harness.driver.state, FlarkV3SessionDriverState.closing);
      expect(harness.store.closeCount, 1);
      expect(
        harness.transport.commands.whereType<FlarkV3ParserBeginClose>(),
        hasLength(1),
      );
    },
  );

  test('close-poll rejection is not fabricated as a publication result', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    final priorPublicationRejections = harness.transport.commands
        .whereType<FlarkV3ParserHostPollRejected>()
        .length;
    harness.store.pollRejectReason = FlarkV3HostRejectReason.closed;

    harness.driver.beginClose();
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserDrain);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);

    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(
      harness.driver.lastHostRejection?.reason,
      FlarkV3HostRejectReason.closed,
    );
    expect(
      harness.transport.commands
          .whereType<FlarkV3ParserHostPollRejected>()
          .length,
      priorPublicationRejections,
    );
  });

  test('close drains an active publication and rejects its late event', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    _synchronizeCurrentLease(harness, eventId: 2);
    final begin = _publicationOffer(harness.session.sourceVersion);
    _beginPublication(harness, begin, eventId: 3);
    expect(
      harness.driver.publicationState,
      FlarkV3PublicationDriverState.acceptingPackets,
    );

    final generation = harness.driver.workerGeneration!;
    harness.store.closePendingPolls = 1;
    harness.driver.beginClose();
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);

    harness.transport.emit(
      FlarkV3ParserPublicationPacket(
        eventId: 4,
        binding: harness.driver.parserBinding,
        packet: _publicationPacket(begin),
      ),
    );
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
    expect(
      harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
      FlarkV3ParserEventDisposition.rejected,
    );

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserDrain);
    final drain = harness.transport.last<FlarkV3ParserDrainGrant>();
    harness.transport.emit(
      FlarkV3ParserDrainProgress(
        eventId: 5,
        binding: drain.binding,
        drainId: drain.drainId,
        releasedSourceLeases: 0,
        releasedSourceBytes: 0,
        arenaTransitions: 1,
        arenaNodesReclaimed: 1,
        complete: true,
      ),
    );
    harness.driver.pump();
    harness.transport.emit(
      FlarkV3ParserClosed(eventId: 6, workerGeneration: generation),
    );
    harness.driver.pump();
    expect(harness.driver.parserClosed, isTrue);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.driver.state, FlarkV3SessionDriverState.closed);
    expect(harness.transport.closed, isTrue);
  });

  test('terminal parser failure during close still drains the host', () {
    final harness = _harness(FlarkV3SourceSession.fromString('live'));
    addTearDown(harness.dispose);
    harness.driver.pump();
    final generation = harness.driver.workerGeneration!;

    harness.store.closePendingPolls = 1;
    harness.driver.beginClose();
    final failure = FlarkV3ParserFailed(
      eventId: 2,
      workerGeneration: generation,
      failureCode: 23,
    );
    harness.transport.emit(failure);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);

    expect(harness.driver.lastFailure, same(failure));
    expect(harness.driver.parserClosed, isTrue);
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(harness.driver.hostDrained, isFalse);
    expect(harness.transport.closed, isFalse);

    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.driver.state, FlarkV3SessionDriverState.closing);
    expect(harness.driver.pump().action, FlarkV3SessionPumpAction.hostPoll);
    expect(harness.driver.hostDrained, isTrue);
    expect(harness.driver.state, FlarkV3SessionDriverState.closed);
    expect(harness.transport.closed, isTrue);
  });
}

final _documentSession = FlarkV3DocumentSessionId(101, 102, 103, 104);
final _syntaxProfile = FlarkV3SyntaxProfileId(1);
final _publicationAuthority = FlarkV3ParserPublicationAuthority(
  grammarRevision: 7,
  syntaxProfile: _syntaxProfile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
);

_Harness _harness(
  FlarkV3SourceSession sourceSession, {
  bool acknowledgeOpen = true,
  FlarkV3SourceVersion? certifiedSourceVersion,
  FlarkV3HostWorkGrant? hostPollGrant,
}) {
  final store = _SourceOnlyHostStore();
  final session = FlarkDocumentSession.attach(
    sourceSession: sourceSession,
    documentSession: _documentSession,
    hostStore: store,
    certifiedSourceVersion: certifiedSourceVersion,
  );
  final transport = _FakeParserTransport();
  final driver = FlarkV3SessionDriver(
    session: session,
    transport: transport,
    parserBinding: FlarkV3ParserSessionBinding(
      documentSession: _documentSession,
      sourceSessionIdentity: sourceSession.sourceSessionIdentity,
      workerGeneration: sourceSession.workerGeneration,
    ),
    publicationAuthority: _publicationAuthority,
    hostPollGrant: hostPollGrant,
  );
  final harness = _Harness(session, store, transport, driver);
  if (acknowledgeOpen) {
    _acknowledgeCurrentOpen(harness, eventId: 1);
  }
  return harness;
}

void _acknowledgeCurrentOpen(_Harness harness, {required int eventId}) {
  final open = harness.transport.last<FlarkV3ParserOpen>();
  harness.transport.emit(
    FlarkV3ParserOpened(
      eventId: eventId,
      binding: open.binding,
      mode: open.mode,
    ),
  );
  expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
  expect(harness.driver.state, FlarkV3SessionDriverState.open);
  expect(
    harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
    FlarkV3ParserEventDisposition.accepted,
  );
}

void _synchronizeCurrentLease(_Harness harness, {required int eventId}) {
  expect(harness.driver.pump().action, FlarkV3SessionPumpAction.sourceSync);
  final lease = harness.transport.last<FlarkV3ParserSynchronizeSource>().lease;
  harness.transport.emit(_ackEvent(eventId, lease));
  expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
}

void _append(FlarkDocumentSession session, String text) {
  final end = session.source.utf16Length;
  session.apply(
    FlarkV3SourceTransaction.single(
      baseRevision: session.uiRevision,
      operation: FlarkV3SourceEdit(
        startUtf16: end,
        endUtf16: end,
        replacement: text,
      ),
    ),
  );
}

FlarkV3ParserSourceSynchronized _ackEvent(
  int eventId,
  FlarkV3SourceWorkerSyncLease lease,
) => FlarkV3ParserSourceSynchronized(
  eventId: eventId,
  workerGeneration: lease.workerGeneration,
  acknowledgement: switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
      observedReplica: lease.isLast ? _observedFor(lease) : null,
    ),
    FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
      observedReplica: _observedFor(lease),
    ),
  },
);

FlarkV3SourceWorkerSyncAcknowledgement _wrongLeaseAcknowledgement(
  FlarkV3SourceWorkerSyncLease lease,
) => switch (lease) {
  FlarkV3SourceSnapshotSyncLease() => FlarkV3SourceSnapshotSyncAcknowledgement(
    sourceSessionIdentity: lease.sourceSessionIdentity,
    leaseId: lease.leaseId + 1,
    workerGeneration: lease.workerGeneration,
    baseUiRevision: lease.baseUiRevision,
    startUtf16: lease.startUtf16,
    endUtf16: lease.endUtf16,
    throughIntentSequence: lease.throughIntentSequence,
    observedReplica: lease.isLast ? _observedFor(lease) : null,
  ),
  FlarkV3SourceIntentSyncLease() => FlarkV3SourceIntentSyncAcknowledgement(
    sourceSessionIdentity: lease.sourceSessionIdentity,
    leaseId: lease.leaseId + 1,
    workerGeneration: lease.workerGeneration,
    firstSequence: lease.firstSequence,
    lastSequence: lease.lastSequence,
    entryCount: lease.intents.length,
    payloadUtf16: lease.payloadUtf16,
    observedReplica: _observedFor(lease),
  ),
};

FlarkV3SourceWorkerSyncAcknowledgement _wrongObservationAcknowledgement(
  FlarkV3SourceWorkerSyncLease lease,
) => switch (lease) {
  FlarkV3SourceSnapshotSyncLease() => FlarkV3SourceSnapshotSyncAcknowledgement(
    sourceSessionIdentity: lease.sourceSessionIdentity,
    leaseId: lease.leaseId,
    workerGeneration: lease.workerGeneration,
    baseUiRevision: lease.baseUiRevision,
    startUtf16: lease.startUtf16,
    endUtf16: lease.endUtf16,
    throughIntentSequence: lease.throughIntentSequence,
    observedReplica: FlarkV3ObservedSourceReplicaVersion(
      revision: lease.targetStamp.revision,
      utf16Length: lease.targetStamp.utf16Length + 1,
      utf8Length: _observedFor(lease).utf8Length,
      intentHighWater: lease.throughIntentSequence,
    ),
  ),
  FlarkV3SourceIntentSyncLease() => FlarkV3SourceIntentSyncAcknowledgement(
    sourceSessionIdentity: lease.sourceSessionIdentity,
    leaseId: lease.leaseId,
    workerGeneration: lease.workerGeneration,
    firstSequence: lease.firstSequence,
    lastSequence: lease.lastSequence,
    entryCount: lease.intents.length,
    payloadUtf16: lease.payloadUtf16,
    observedReplica: FlarkV3ObservedSourceReplicaVersion(
      revision: lease.targetStamp.revision,
      utf16Length: lease.targetStamp.utf16Length + 1,
      utf8Length: _observedFor(lease).utf8Length,
      intentHighWater: lease.lastSequence,
    ),
  ),
};

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  final utf8Length = switch (target) {
    FlarkV3KnownSourceStamp() => target.utf8Length,
    FlarkV3ProvisionalSourceStamp() => throw StateError(
      'This driver fixture only installs known source targets.',
    ),
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

FlarkV3SourcePrefixFacts _canonicalFact(String source, int utf16Offset) {
  final prefix = source.substring(0, utf16Offset);
  return FlarkV3SourcePrefixFacts(
    utf16Offset: utf16Offset,
    utf8Offset: utf8.encode(prefix).length,
    newlines: 0,
    hash: FlarkV3SourceDocument.fromString(prefix).contentHash128,
  );
}

FlarkV3ContentHash128 _portableFactsHash(List<FlarkV3SourcePrefixFacts> facts) {
  const mask32 = 0xFFFFFFFF;
  const bases = [0x00100193, 0x9E3779B1, 0x85EBCA77, 0xC2B2AE3D];
  var words = [0, 0, 0, 0];
  for (final fact in facts) {
    for (final value in [
      fact.utf16Offset,
      fact.utf8Offset,
      fact.newlines,
      fact.hash.word0,
      fact.hash.word1,
      fact.hash.word2,
      fact.hash.word3,
    ]) {
      for (var shift = 0; shift < 64; shift += 8) {
        final term = ((value >>> shift) & 0xFF) + 1;
        words = [
          for (var lane = 0; lane < 4; lane += 1)
            (words[lane] * bases[lane] + term) & mask32,
        ];
      }
    }
  }
  return FlarkV3ContentHash128(words[0], words[1], words[2], words[3]);
}

FlarkV3HostOfferBegin _publicationOffer(
  FlarkV3SourceVersion sourceVersion, {
  FlarkV3SyntaxProfileId? syntaxProfile,
  FlarkV3PublicationSessionId? publicationSession,
  FlarkV3SourceRootId? sourceRoot,
  int offerWord0 = 11,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(offerWord0, sourceVersion.revision, 12, 13),
  publicationSession:
      publicationSession ?? FlarkV3PublicationSessionId(21, 22, 23, 24),
  targetHostRevision: FlarkV3HostRevisionId(1),
  sourceVersion: sourceVersion,
  sourceRoot: sourceRoot ?? FlarkV3SourceRootId(0, 101),
  parseGeneration: 1,
  grammarRevision: 7,
  syntaxProfile: syntaxProfile ?? _syntaxProfile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  mode: FlarkV3PublicationMode.fullSnapshot,
  baseAck: null,
  transferredRecordCount: 1,
  targetRecordCount: 1,
  limits: FlarkV3HostOfferLimits(
    maximumFrameCount: 1,
    maximumEncodedFrameBytes: 64,
    maximumPacketBytes: 132,
    maximumFrameBytes: 64,
    maximumProgramChildren: 8,
  ),
);

FlarkV3HostPublicationPacket _publicationPacket(FlarkV3HostOfferBegin begin) =>
    testPublicationPacket(
      offerId: begin.offerId,
      firstFrameOrdinal: 0,
      firstRecordOrdinal: 0,
      recordCount: 1,
      digest: FlarkV3ProtocolDigest128(1, 2, 3, 4),
      frameBytes: Uint8List.fromList(const [1, 2, 3, 4]),
    );

FlarkV3HostCommitRequest _publicationCommit(
  FlarkV3HostOfferBegin begin,
  FlarkV3HostPublicationPacket packet,
) => FlarkV3HostCommitRequest(
  offerId: begin.offerId,
  actualFrameCount: packet.frameCount,
  actualEncodedFrameBytes: packet.aggregateFrameBytes,
  rollingTransportDigest: FlarkV3ProtocolDigest128(5, 6, 7, 8),
  canonicalStreamDigest: FlarkV3ProtocolDigest128(9, 10, 11, 12),
);

FlarkV3HostOfferBegin _publicationDelta(
  FlarkV3SourceVersion sourceVersion, {
  required FlarkV3StructuralAck base,
  required FlarkV3PublicationSessionId publicationSession,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(51, sourceVersion.revision, 52, 53),
  publicationSession: publicationSession,
  targetHostRevision: FlarkV3HostRevisionId(2),
  sourceVersion: sourceVersion,
  sourceRoot: FlarkV3SourceRootId(0, 303),
  parseGeneration: base.parseGeneration + 1,
  grammarRevision: base.grammarRevision,
  syntaxProfile: base.syntaxProfile,
  authorityMask: base.authorityMask,
  mode: FlarkV3PublicationMode.exactBaseReferencesDelta,
  baseAck: base,
  transferredRecordCount: 1,
  targetRecordCount: 2,
  limits: FlarkV3HostOfferLimits(
    maximumFrameCount: 1,
    maximumEncodedFrameBytes: 64,
    maximumPacketBytes: 132,
    maximumFrameBytes: 64,
    maximumProgramChildren: 8,
  ),
);

void _beginPublication(
  _Harness harness,
  FlarkV3HostOfferBegin begin, {
  required int eventId,
}) {
  harness.transport.emit(
    FlarkV3ParserPublicationBegin(
      eventId: eventId,
      binding: harness.driver.parserBinding,
      begin: begin,
    ),
  );
  expect(harness.driver.pump().action, FlarkV3SessionPumpAction.parserEvent);
  expect(
    harness.transport.last<FlarkV3ParserEventReceipt>().disposition,
    FlarkV3ParserEventDisposition.accepted,
  );
}

FlarkV3StructuralAck _commitPublication(
  _Harness harness,
  FlarkV3HostOfferBegin begin, {
  required int packetEventId,
  required int commitEventId,
}) {
  final packet = _publicationPacket(begin);
  harness.transport.emit(
    FlarkV3ParserPublicationPacket(
      eventId: packetEventId,
      binding: harness.driver.parserBinding,
      packet: packet,
    ),
  );
  harness.driver.pump();
  harness.driver.pump();
  harness.transport.emit(
    FlarkV3ParserPublicationCommitRequested(
      eventId: commitEventId,
      binding: harness.driver.parserBinding,
      request: _publicationCommit(begin, packet),
    ),
  );
  harness.driver.pump();
  harness.driver.pump();
  return (harness.transport.last<FlarkV3ParserHostPollCompleted>().outcome
          as FlarkV3HostCommitted)
      .ack;
}

FlarkV3StructuralAck _installAndDeliverStructuralBase(_Harness harness) {
  final begin = _publicationOffer(harness.session.sourceVersion);
  _beginPublication(harness, begin, eventId: 3);
  final ack = _commitPublication(
    harness,
    begin,
    packetEventId: 4,
    commitEventId: 5,
  );
  harness.transport.emit(
    FlarkV3ParserPublicationDeliveryAcknowledged(
      eventId: 6,
      binding: harness.driver.parserBinding,
      ack: ack,
    ),
  );
  harness.driver.pump();
  expect(harness.session.pendingDeliveryAck, isNull);
  return ack;
}

FlarkV3HotInlineSidecarOfferBegin _inlineSidecarOffer(
  FlarkV3StructuralAck baseAck, {
  required int refinementGeneration,
  int offerWord0 = 61,
  int publicationWord0 = 71,
}) => FlarkV3HotInlineSidecarOfferBegin(
  offerId: FlarkV3OfferId(offerWord0, refinementGeneration, 1, 1),
  publicationSession: FlarkV3PublicationSessionId(
    publicationWord0,
    refinementGeneration,
    1,
    1,
  ),
  baseAck: baseAck,
  binding: FlarkV3HotInlineSidecarBinding(
    parserProfile: baseAck.syntaxProfile,
    refinementGeneration: FlarkV3ProtocolU64.fromU32(refinementGeneration),
    blockOrdinal: FlarkV3ProtocolU64.fromU32(0),
    physicalStartUtf8: 0,
    physicalEndUtf8: baseAck.sourceVersion.metric.bytes,
    visibleStartUtf8: 0,
    visibleEndUtf8: baseAck.sourceVersion.metric.bytes,
    physicalStartUtf16: 0,
    physicalEndUtf16: baseAck.sourceVersion.metric.utf16,
    visibleStartUtf16: 0,
    visibleEndUtf16: baseAck.sourceVersion.metric.utf16,
  ),
  envelope: FlarkV3HotInlineSidecarEnvelopeMetrics(
    hio1EncodedBytes: FlarkV3HotInlineSidecarEnvelopeMetrics.hio1EnvelopeBytes,
    ipr2DescriptorBytes: 0,
    transferredNodeCount: 1,
    hio1EnvelopeDigest256: FlarkV3ProtocolDigest256.zero,
    disposition: FlarkV3HotInlineSidecarUnsupported(
      reason: 7,
      metadataCommitment256: FlarkV3ProtocolDigest256.zero,
    ),
  ),
  limits: FlarkV3HostOfferLimits(
    maximumFrameCount: 4,
    maximumEncodedFrameBytes: 256,
    maximumPacketBytes: 132,
    maximumFrameBytes: 64,
    maximumProgramChildren: 8,
  ),
);

FlarkV3ViewportPresentationOfferBegin _viewportPresentationOffer(
  FlarkV3StructuralAck baseAck, {
  required int viewportGeneration,
}) => FlarkV3ViewportPresentationOfferBegin(
  offerId: FlarkV3OfferId(121, viewportGeneration, 1, 1),
  publicationSession: FlarkV3PublicationSessionId(
    131,
    viewportGeneration,
    1,
    1,
  ),
  baseAck: baseAck,
  binding: FlarkV3ViewportPresentationBinding(
    viewportGeneration: viewportGeneration,
    requestedRange: FlarkV3ViewportPresentationMetricRange(
      startUtf8: 0,
      startUtf16: 0,
      endUtf8: baseAck.sourceVersion.metric.bytes,
      endUtf16: baseAck.sourceVersion.metric.utf16,
    ),
    coveredRange: FlarkV3ViewportPresentationMetricRange(
      startUtf8: 0,
      startUtf16: 0,
      endUtf8: baseAck.sourceVersion.metric.bytes,
      endUtf16: baseAck.sourceVersion.metric.utf16,
    ),
    start: FlarkV3ViewportPresentationVisitStart(
      blockOrdinal: FlarkV3ProtocolU64.fromU32(0),
      utf8Offset: 0,
      utf16Offset: 0,
    ),
    next: FlarkV3ViewportPresentationVisitStart(
      blockOrdinal: FlarkV3ProtocolU64.fromU32(1),
      utf8Offset: baseAck.sourceVersion.metric.bytes,
      utf16Offset: baseAck.sourceVersion.metric.utf16,
    ),
    complete: true,
  ),
  envelope: FlarkV3ViewportPresentationEnvelopeMetrics(
    visitedStructuralEntries: 1,
    visitedStoragePages: 1,
    orderedLeafCount: 1,
    inlineSourceBytes: baseAck.sourceVersion.metric.bytes,
    factCount: 1,
    transferredNodeCount: 1,
    parserTransitions: 8,
    aggregateEnvelopeDigest256: FlarkV3ProtocolDigest256.zero,
  ),
  queryLimits: FlarkV3ViewportPresentationQueryLimits(
    maximumStructuralEntries: 64,
    maximumStoragePages: 25,
    maximumInlineLeaves: flarkV3DefaultViewportPresentationEntryCapacity,
    maximumInlineLeafSourceBytes: 8 * 1024,
    maximumInlineSourceBytes: 64 * 1024,
    maximumFactRecords: 2048,
    maximumEncodedFrameBytes: 512 * 1024,
    maximumParserTransitions: 250000,
  ),
  limits: FlarkV3ViewportPresentationOfferLimits(
    maximumFrameCount: 6,
    maximumEncodedFrameBytes: 512 * 1024,
    maximumPacketBytes: FlarkV3HostPublicationPacket.maximumRawBytes,
    maximumFrameBytes: FlarkV3HostPublicationPacket.maximumAggregateFrameBytes,
    maximumProgramChildren:
        FlarkV3ViewportPresentationOfferLimits.productMaximumProgramChildren,
  ),
);

FlarkV3HostPublicationPacket _inlineSidecarPacket(
  FlarkV3HotInlineSidecarOfferBegin begin,
) => testPublicationPacket(
  offerId: begin.offerId,
  firstFrameOrdinal: 0,
  firstRecordOrdinal: 0,
  recordCount: 1,
  digest: FlarkV3ProtocolDigest128(81, 82, 83, 84),
  frameBytes: Uint8List.fromList(const <int>[5, 6, 7, 8]),
);

FlarkV3HotInlineSidecarCommitRequest _inlineSidecarCommit(
  FlarkV3HotInlineSidecarOfferBegin begin,
  FlarkV3HostPublicationPacket packet,
) => FlarkV3HotInlineSidecarCommitRequest(
  offerId: begin.offerId,
  actualFrameCount: 2,
  actualEncodedFrameBytes: packet.aggregateFrameBytes,
  rollingTransportDigest: FlarkV3ProtocolDigest128(85, 86, 87, 88),
  rootStreamDigest: FlarkV3ProtocolDigest128(89, 90, 91, 92),
);

FlarkV3InlineSidecarAck _commitInlineSidecar(
  _Harness harness,
  FlarkV3StructuralAck baseAck, {
  required int refinementGeneration,
  required int beginEventId,
  required int packetEventId,
  required int commitEventId,
}) {
  final initialAttemptOutcomeGeneration =
      harness.driver.inlineAttemptOutcomeGeneration;
  expect(
    harness.driver.requestInlineRefinement(utf16Offset: 2),
    refinementGeneration,
  );
  expect(
    harness.driver.pump().action,
    FlarkV3SessionPumpAction.inlineRefinementRequest,
  );
  final begin = _inlineSidecarOffer(
    baseAck,
    refinementGeneration: refinementGeneration,
  );
  harness.transport.emitInlineSidecar(
    FlarkV3ParserInlineSidecarBegin(
      eventId: beginEventId,
      binding: harness.driver.parserBinding,
      begin: begin,
    ),
  );
  harness.driver.pump();
  final packet = _inlineSidecarPacket(begin);
  harness.transport.emitInlineSidecar(
    FlarkV3ParserInlineSidecarPacket(
      eventId: packetEventId,
      binding: harness.driver.parserBinding,
      packet: packet,
    ),
  );
  harness.driver.pump();
  expect(
    harness.driver.pump().action,
    FlarkV3SessionPumpAction.inlineSidecarHostPoll,
  );
  expect(
    harness.driver.inlineAttemptOutcomeGeneration,
    initialAttemptOutcomeGeneration,
    reason: 'packet credit is progress, not a terminal attempt outcome',
  );
  harness.transport.emitInlineSidecar(
    FlarkV3ParserInlineSidecarCommitRequested(
      eventId: commitEventId,
      binding: harness.driver.parserBinding,
      request: _inlineSidecarCommit(begin, packet),
    ),
  );
  harness.driver.pump();
  expect(
    harness.driver.pump().action,
    FlarkV3SessionPumpAction.inlineSidecarHostPoll,
  );
  expect(
    harness.driver.inlineAttemptOutcomeGeneration,
    initialAttemptOutcomeGeneration + 1,
  );
  return (harness.transport
              .lastInlineSidecar<FlarkV3ParserInlineSidecarHostPollCompleted>()
              .outcome
          as FlarkV3InlineSidecarHostCommitted)
      .ack;
}

FlarkV3StructuralAck _copyAck(
  FlarkV3StructuralAck ack, {
  int? parseGeneration,
  FlarkV3PublicationSessionId? publicationSession,
}) => FlarkV3StructuralAck(
  publicationSession: publicationSession ?? ack.publicationSession,
  hostRevision: ack.hostRevision,
  sourceVersion: ack.sourceVersion,
  sourceRoot: ack.sourceRoot,
  parseGeneration: parseGeneration ?? ack.parseGeneration,
  grammarRevision: ack.grammarRevision,
  syntaxProfile: ack.syntaxProfile,
  authorityMask: ack.authorityMask,
  recordCount: ack.recordCount,
  sequenceDigest: ack.sequenceDigest,
  manifestDigest: ack.manifestDigest,
);

final class _Harness {
  const _Harness(this.session, this.store, this.transport, this.driver);

  final FlarkDocumentSession session;
  final _SourceOnlyHostStore store;
  final _FakeParserTransport transport;
  final FlarkV3SessionDriver driver;

  void dispose() {
    if (driver.state != FlarkV3SessionDriverState.closed) {
      driver.forceClose();
    }
  }
}

final class _FakeParserTransport
    implements
        FlarkV3ParserTransport,
        FlarkV3ParserInlineSidecarTransport,
        FlarkV3ParserViewportPresentationTransport {
  final List<FlarkV3ParserCommand> commands = [];
  final List<FlarkV3ParserInlineSidecarHostPollCommand> inlineSidecarCommands =
      [];
  final List<FlarkV3ParserViewportPresentationHostPollCommand>
  viewportCommands = [];
  FlarkV3ParserEventCallback? _callback;
  FlarkV3ParserInlineSidecarEventCallback? _inlineSidecarCallback;
  FlarkV3ParserViewportPresentationEventCallback? _viewportCallback;
  bool closed = false;

  @override
  void bind(FlarkV3ParserEventCallback onEvent) {
    if (_callback != null) throw StateError('Transport already bound.');
    _callback = onEvent;
  }

  @override
  void bindInlineSidecar(FlarkV3ParserInlineSidecarEventCallback onEvent) {
    if (_inlineSidecarCallback != null) {
      throw StateError('Inline-sidecar transport already bound.');
    }
    _inlineSidecarCallback = onEvent;
  }

  @override
  void bindViewportPresentation(
    FlarkV3ParserViewportPresentationEventCallback onEvent,
  ) {
    if (_viewportCallback != null) {
      throw StateError('Viewport transport already bound.');
    }
    _viewportCallback = onEvent;
  }

  @override
  void send(FlarkV3ParserCommand command) => commands.add(command);

  @override
  void close() => closed = true;

  void emit(FlarkV3ParserEvent event) => _callback!(event);

  void emitInlineSidecar(FlarkV3ParserInlineSidecarEvent event) =>
      _inlineSidecarCallback!(event);

  void emitViewportPresentation(FlarkV3ParserViewportPresentationEvent event) =>
      _viewportCallback!(event);

  @override
  void sendInlineSidecarHostPoll(
    FlarkV3ParserInlineSidecarHostPollCommand command,
  ) => inlineSidecarCommands.add(command);

  @override
  void sendViewportPresentationHostPoll(
    FlarkV3ParserViewportPresentationHostPollCommand command,
  ) => viewportCommands.add(command);

  T last<T extends FlarkV3ParserCommand>() => commands.whereType<T>().last;

  T lastInlineSidecar<T extends FlarkV3ParserInlineSidecarHostPollCommand>() =>
      inlineSidecarCommands.whereType<T>().last;

  T
  lastViewport<T extends FlarkV3ParserViewportPresentationHostPollCommand>() =>
      viewportCommands.whereType<T>().last;
}

final class _SourceOnlyHostStore
    implements
        FlarkV3HostStore,
        FlarkV3InlineSidecarHostStore,
        FlarkV3ViewportPresentationHostStore {
  int closeCount = 0;
  int closePendingPolls = 0;
  int pendingPolls = 0;
  int? packetCreditOrdinalOverride;
  FlarkV3HostRejectReason? pollRejectReason;
  int pollCount = 0;
  int beginCount = 0;
  int deliveryAckCount = 0;
  bool closeRequested = false;
  bool commitRequested = false;
  bool abortRequested = false;
  FlarkV3HostOfferBegin? active;
  FlarkV3HostPublicationPacket? pendingPacket;
  FlarkV3HostWorkGrant? lastPollGrant;
  FlarkV3StructuralAck? pendingDelivery;
  int inlineSidecarBeginCount = 0;
  int inlineSidecarPollCount = 0;
  int inlineSidecarDeliveryAckCount = 0;
  int inlineSidecarPendingPolls = 0;
  bool inlineSidecarCommitRequested = false;
  bool inlineSidecarAbortRequested = false;
  FlarkV3HotInlineSidecarOfferBegin? activeInlineSidecar;
  FlarkV3HotInlineSidecarCommitRequest? activeInlineSidecarCommit;
  FlarkV3HostPublicationPacket? pendingInlineSidecarPacket;
  FlarkV3InlineSidecarAck? pendingInlineSidecarDelivery;
  FlarkV3InlineSidecarAck? installedInlineSidecar;
  FlarkV3ViewportPresentationOfferBegin? activeViewportPresentation;
  FlarkV3ViewportPresentationCommitRequest? activeViewportPresentationCommit;
  FlarkV3HostPublicationPacket? pendingViewportPresentationPacket;
  FlarkV3ViewportPresentationAck? pendingViewportPresentationDelivery;
  FlarkV3ViewportPresentationAck? installedViewportPresentation;
  bool viewportPresentationAbortRequested = false;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    active = null;
    pendingPacket = null;
    commitRequested = false;
    abortRequested = false;
    activeInlineSidecar = null;
    activeInlineSidecarCommit = null;
    pendingInlineSidecarPacket = null;
    pendingInlineSidecarDelivery = null;
    installedInlineSidecar = null;
    inlineSidecarCommitRequested = false;
    inlineSidecarAbortRequested = false;
    activeViewportPresentation = null;
    activeViewportPresentationCommit = null;
    pendingViewportPresentationPacket = null;
    pendingViewportPresentationDelivery = null;
    installedViewportPresentation = null;
    viewportPresentationAbortRequested = false;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    closeCount += 1;
    closeRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    if (active?.offerId != offerId) return _wrongOffer();
    abortRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) {
    if (pendingDelivery != ack) return _wrongOffer();
    pendingDelivery = null;
    deliveryAckCount += 1;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (active?.offerId != packet.offerId || pendingPacket != null) {
      return _wrongOffer();
    }
    pendingPacket = packet;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    beginCount += 1;
    if (active != null) return _wrongOffer();
    final waiting = pendingDelivery;
    if (waiting != null &&
        (begin.mode != FlarkV3PublicationMode.fullSnapshot ||
            begin.publicationSession == waiting.publicationSession)) {
      return _wrongOffer();
    }
    active = begin;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    pollCount += 1;
    lastPollGrant = grant;
    final rejectReason = pollRejectReason;
    if (rejectReason != null) {
      return FlarkV3HostRejected(
        FlarkV3HostRejection(rejectReason, 'Synthetic host-poll rejection.'),
      );
    }
    if (closeRequested) {
      if (closePendingPolls > 0) {
        closePendingPolls -= 1;
        return const FlarkV3HostAccepted(FlarkV3HostPollPending());
      }
      return const FlarkV3HostAccepted(FlarkV3HostClosed());
    }
    if (pendingPolls > 0) {
      pendingPolls -= 1;
      return const FlarkV3HostAccepted(FlarkV3HostPollPending());
    }
    final begin = active;
    if (abortRequested && begin != null) {
      abortRequested = false;
      active = null;
      pendingPacket = null;
      commitRequested = false;
      return FlarkV3HostAccepted(FlarkV3HostAbortComplete(begin.offerId));
    }
    final packet = pendingPacket;
    if (begin != null && packet != null) {
      pendingPacket = null;
      return FlarkV3HostAccepted(
        FlarkV3HostPacketCredit(
          offerId: begin.offerId,
          nextFrameOrdinal:
              packetCreditOrdinalOverride ??
              packet.firstFrameOrdinal + packet.frameCount,
        ),
      );
    }
    if (begin != null && commitRequested) {
      commitRequested = false;
      active = null;
      final ack = FlarkV3StructuralAck(
        publicationSession: begin.publicationSession,
        hostRevision: begin.targetHostRevision,
        sourceVersion: begin.sourceVersion,
        sourceRoot: begin.sourceRoot,
        parseGeneration: begin.parseGeneration,
        grammarRevision: begin.grammarRevision,
        syntaxProfile: begin.syntaxProfile,
        authorityMask: begin.authorityMask,
        recordCount: begin.targetRecordCount,
        sequenceDigest: FlarkV3ProtocolDigest128(31, 32, 33, 34),
        manifestDigest: FlarkV3ProtocolDigest128(41, 42, 43, 44),
      );
      pendingDelivery = ack;
      return FlarkV3HostAccepted(FlarkV3HostCommitted(ack));
    }
    return const FlarkV3HostAccepted(FlarkV3HostPollPending());
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => throw UnsupportedError('No queries in this source-only test.');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    if (active?.offerId != request.offerId || pendingPacket != null) {
      return _wrongOffer();
    }
    commitRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginInlineSidecarOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  ) {
    inlineSidecarBeginCount += 1;
    if (active != null ||
        activeInlineSidecar != null ||
        pendingInlineSidecarDelivery != null) {
      return _wrongOffer();
    }
    activeInlineSidecar = begin;
    activeInlineSidecarCommit = null;
    inlineSidecarCommitRequested = false;
    inlineSidecarAbortRequested = false;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitInlineSidecarPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (activeInlineSidecar?.offerId != packet.offerId ||
        pendingInlineSidecarPacket != null) {
      return _wrongOffer();
    }
    pendingInlineSidecarPacket = packet;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestInlineSidecarCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  ) {
    if (activeInlineSidecar?.offerId != request.offerId ||
        pendingInlineSidecarPacket != null) {
      return _wrongOffer();
    }
    activeInlineSidecarCommit = request;
    inlineSidecarCommitRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortInlineSidecarOffer(
    FlarkV3OfferId offerId,
  ) {
    if (activeInlineSidecar?.offerId != offerId) return _wrongOffer();
    inlineSidecarAbortRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarHostPollOutcome> pollInlineSidecar(
    FlarkV3HostWorkGrant grant,
  ) {
    inlineSidecarPollCount += 1;
    if (closeRequested) {
      return const FlarkV3HostAccepted(FlarkV3InlineSidecarHostClosed());
    }
    if (inlineSidecarPendingPolls > 0) {
      inlineSidecarPendingPolls -= 1;
      return const FlarkV3HostAccepted(FlarkV3InlineSidecarHostPollPending());
    }
    final begin = activeInlineSidecar;
    if (inlineSidecarAbortRequested && begin != null) {
      inlineSidecarAbortRequested = false;
      activeInlineSidecar = null;
      activeInlineSidecarCommit = null;
      pendingInlineSidecarPacket = null;
      inlineSidecarCommitRequested = false;
      return FlarkV3HostAccepted(
        FlarkV3InlineSidecarHostAbortComplete(begin.offerId),
      );
    }
    final packet = pendingInlineSidecarPacket;
    if (begin != null && packet != null) {
      pendingInlineSidecarPacket = null;
      return FlarkV3HostAccepted(
        FlarkV3InlineSidecarHostPacketCredit(
          offerId: begin.offerId,
          nextFrameOrdinal: packet.firstFrameOrdinal + packet.frameCount,
        ),
      );
    }
    final commit = activeInlineSidecarCommit;
    if (begin != null && commit != null && inlineSidecarCommitRequested) {
      inlineSidecarCommitRequested = false;
      activeInlineSidecar = null;
      activeInlineSidecarCommit = null;
      final ack = FlarkV3InlineSidecarAck(
        publicationSession: begin.publicationSession,
        baseAck: begin.baseAck,
        refinementGeneration: begin.binding.refinementGeneration,
        blockOrdinal: begin.binding.blockOrdinal,
        transferredNodeCount: begin.envelope.transferredNodeCount,
        disposition: switch (begin.envelope.disposition) {
          FlarkV3HotInlineSidecarAuthoritative() =>
            FlarkV3InlineSidecarAckDisposition.authoritative,
          FlarkV3HotInlineSidecarUnsupported() =>
            FlarkV3InlineSidecarAckDisposition.unsupported,
        },
        hio1EnvelopeDigest256: begin.envelope.hio1EnvelopeDigest256,
        rootStreamDigest: commit.rootStreamDigest,
      );
      pendingInlineSidecarDelivery = ack;
      installedInlineSidecar = ack;
      return FlarkV3HostAccepted(FlarkV3InlineSidecarHostCommitted(ack));
    }
    return const FlarkV3HostAccepted(FlarkV3InlineSidecarHostPollPending());
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeInlineSidecarDelivery(
    FlarkV3InlineSidecarAck ack,
  ) {
    if (pendingInlineSidecarDelivery != ack) return _wrongOffer();
    pendingInlineSidecarDelivery = null;
    inlineSidecarDeliveryAckCount += 1;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> queryInlineSidecar(
    FlarkV3InlineSidecarQuery query,
  ) {
    if (installedInlineSidecar == null) {
      return const FlarkV3HostAccepted(FlarkV3InlineSidecarQueryUnavailable());
    }
    return FlarkV3HostAccepted(
      FlarkV3InlineSidecarQueryUnsupported(
        reason: 7,
        metadata: Uint8List.fromList(const <int>[7]),
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) {
    if (activeViewportPresentation != null ||
        pendingViewportPresentationDelivery != null) {
      return _wrongOffer();
    }
    activeViewportPresentation = begin;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (activeViewportPresentation?.offerId != packet.offerId ||
        pendingViewportPresentationPacket != null) {
      return _wrongOffer();
    }
    pendingViewportPresentationPacket = packet;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) {
    if (activeViewportPresentation?.offerId != request.offerId ||
        pendingViewportPresentationPacket != null) {
      return _wrongOffer();
    }
    activeViewportPresentationCommit = request;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  ) {
    if (activeViewportPresentation?.offerId != offerId) return _wrongOffer();
    viewportPresentationAbortRequested = true;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant) {
    final begin = activeViewportPresentation;
    if (begin == null) {
      return const FlarkV3HostAccepted(
        FlarkV3ViewportPresentationHostPollPending(),
      );
    }
    if (viewportPresentationAbortRequested) {
      activeViewportPresentation = null;
      activeViewportPresentationCommit = null;
      pendingViewportPresentationPacket = null;
      viewportPresentationAbortRequested = false;
      return FlarkV3HostAccepted(
        FlarkV3ViewportPresentationHostAbortComplete(begin.offerId),
      );
    }
    final packet = pendingViewportPresentationPacket;
    if (packet != null) {
      pendingViewportPresentationPacket = null;
      return FlarkV3HostAccepted(
        FlarkV3ViewportPresentationHostPacketCredit(
          offerId: begin.offerId,
          nextFrameOrdinal: packet.firstFrameOrdinal + packet.frameCount,
        ),
      );
    }
    final commit = activeViewportPresentationCommit;
    if (commit != null) {
      activeViewportPresentation = null;
      activeViewportPresentationCommit = null;
      final ack = FlarkV3ViewportPresentationAck(
        publicationSession: begin.publicationSession,
        baseAck: begin.baseAck,
        binding: begin.binding,
        envelope: begin.envelope,
        actualFrameCount: commit.actualFrameCount,
        actualEncodedFrameBytes: commit.actualEncodedFrameBytes,
        aggregateRootStreamDigest: commit.aggregateRootStreamDigest,
      );
      pendingViewportPresentationDelivery = ack;
      installedViewportPresentation = ack;
      return FlarkV3HostAccepted(FlarkV3ViewportPresentationHostCommitted(ack));
    }
    return const FlarkV3HostAccepted(
      FlarkV3ViewportPresentationHostPollPending(),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack) {
    if (pendingViewportPresentationDelivery != ack) return _wrongOffer();
    pendingViewportPresentationDelivery = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query) =>
      const FlarkV3HostAccepted(FlarkV3ViewportPresentationQueryUnavailable());
}

FlarkV3HostAccepted<FlarkV3HostUnit> _accepted() =>
    const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

FlarkV3HostRejected<FlarkV3HostUnit> _wrongOffer() => const FlarkV3HostRejected(
  FlarkV3HostRejection(FlarkV3HostRejectReason.wrongOffer, 'Wrong test offer.'),
);

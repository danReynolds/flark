import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_publication_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_session_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3ParserSessionWireCodec source leases', () {
    test('round trips seed and continuation snapshots with exact ACKs', () {
      final session = FlarkV3SourceSession.fromString('a🌍bc');
      final seed =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 3)
              as FlarkV3SourceSnapshotSyncLease;
      final binding = _bindingFor(seed);

      final decodedSeed = FlarkV3ParserSessionWireCodec.decodeCommand(
        FlarkV3ParserSessionWireCodec.encodeParserCommand(
          FlarkV3ParserSynchronizeSource(seed),
          binding: binding,
          correlationId: seed.leaseId,
        ),
        establishedBinding: binding,
      );
      final seedCommand =
          decodedSeed.command as FlarkV3ParserSessionSnapshotCommand;
      expect(seedCommand.isSeed, isTrue);
      expect(seedCommand.source, 'a🌍');
      expect(seedCommand.endUtf16, 3);
      expect(seedCommand.totalUtf16Length, 5);
      expect(seedCommand.targetStamp, seed.targetStamp);
      expect(decodedSeed.correlationId, seed.leaseId);

      final decodedSeedAck =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeEvent(
                  FlarkV3ParserSessionSourceSynchronizedEvent(
                    binding: binding,
                    eventId: 11,
                    acknowledgement: seedCommand.acknowledgement(
                      observedReplica: null,
                    ),
                  ),
                  expectedBinding: binding,
                ),
                expectedBinding: binding,
              )
              as FlarkV3ParserSessionSourceSynchronizedEvent;
      _expectSourceAcknowledgement(
        decodedSeedAck.acknowledgement,
        seed.acknowledgement(observedReplica: null),
      );

      session.acknowledgeWorkerSync(
        seed.acknowledgement(observedReplica: null),
      );
      final continuation =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 3)
              as FlarkV3SourceSnapshotSyncLease;
      final decodedContinuation = FlarkV3ParserSessionWireCodec.decodeCommand(
        FlarkV3ParserSessionWireCodec.encodeParserCommand(
          FlarkV3ParserSynchronizeSource(continuation),
          binding: binding,
          correlationId: continuation.leaseId,
        ),
        establishedBinding: binding,
      );
      final continuationCommand =
          decodedContinuation.command as FlarkV3ParserSessionSnapshotCommand;
      expect(continuationCommand.isSeed, isFalse);
      expect(continuationCommand.startUtf16, 3);
      expect(continuationCommand.endUtf16, 5);
      expect(continuationCommand.source, 'bc');
    });

    test('round trips a bounded edit page and exact edit ACK', () {
      final session = FlarkV3SourceSession.fromString('hi');
      final seed = session.beginWorkerSync() as FlarkV3SourceSnapshotSyncLease;
      session.acknowledgeWorkerSync(
        seed.acknowledgement(observedReplica: _observedFor(session, seed)),
      );
      session.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: session.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 2,
            endUtf16: 2,
            replacement: '🌍',
          ),
        ),
      );
      final lease = session.beginWorkerSync() as FlarkV3SourceIntentSyncLease;
      final binding = _bindingFor(lease);

      final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
        FlarkV3ParserSessionWireCodec.encodeParserCommand(
          FlarkV3ParserSynchronizeSource(lease),
          binding: binding,
          correlationId: lease.leaseId,
        ),
        establishedBinding: binding,
      );
      final command = decoded.command as FlarkV3ParserSessionEditCommand;
      expect(command.firstSequence, lease.firstSequence);
      expect(command.lastSequence, lease.lastSequence);
      expect(command.payloadUtf16, 2);
      expect(command.intents, hasLength(1));
      expect(command.baseStamp, lease.baseStamp);
      expect(command.targetStamp, lease.targetStamp);
      expect(command.intents.single.baseStamp, lease.intents.single.baseStamp);
      expect(
        command.intents.single.targetStamp,
        lease.intents.single.targetStamp,
      );
      expect(command.intents.single.operations, hasLength(1));
      final operation = command.intents.single.operations.single;
      expect(operation.startUtf16, 2);
      expect(operation.endUtf16, 2);
      expect(operation.replacement.readRange(0, 2), '🌍');
      final observed = _observedFor(session, lease);

      final decodedEvent =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeParserEvent(
                  FlarkV3ParserSourceSynchronized(
                    eventId: 5,
                    workerGeneration: binding.workerGeneration,
                    acknowledgement: command.acknowledgement(
                      observedReplica: observed,
                    ),
                  ),
                  binding: binding,
                ),
                expectedBinding: binding,
              )
              as FlarkV3ParserSessionSourceSynchronizedEvent;
      _expectSourceAcknowledgement(
        decodedEvent.acknowledgement,
        lease.acknowledgement(observedReplica: observed),
      );
    });

    test('rejects unknown source-stamp and noncanonical observation tags', () {
      final session = FlarkV3SourceSession.fromString('abcd');
      final lease =
          session.beginWorkerSync(maximumSnapshotPageUtf16: 2)
              as FlarkV3SourceSnapshotSyncLease;
      final binding = _bindingFor(lease);
      final command = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserSynchronizeSource(lease),
        binding: binding,
        correlationId: lease.leaseId,
      );
      final unknownStamp = Uint8List.fromList(command);
      ByteData.sublistView(
        unknownStamp,
      ).setUint32(FlarkV3WireProtocol.headerBytes + 28 + 24, 7, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          unknownStamp,
          establishedBinding: binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );

      final event = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionSourceSynchronizedEvent(
          binding: binding,
          eventId: 19,
          acknowledgement: lease.acknowledgement(observedReplica: null),
        ),
        expectedBinding: binding,
      );
      final noncanonicalAbsentObservation = Uint8List.fromList(event);
      ByteData.sublistView(
        noncanonicalAbsentObservation,
      ).setUint32(FlarkV3WireProtocol.headerBytes + 28 + 24, 1, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeEvent(
          noncanonicalAbsentObservation,
          expectedBinding: binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );
    });

    test('rejects revision gaps and overlapping edit operations', () {
      FlarkV3ParserSessionEditCommand editCommand({required int uiRevision}) =>
          FlarkV3ParserSessionEditCommand(
            binding: _binding,
            leaseId: 6,
            intents: [
              FlarkV3SourceIntent(
                workerGeneration: _binding.workerGeneration,
                sequence: 10,
                baseUiRevision: 0,
                uiRevision: uiRevision,
                baseStamp: const FlarkV3ProvisionalSourceStamp(
                  revision: 0,
                  utf16Length: 2,
                ),
                targetStamp: FlarkV3ProvisionalSourceStamp(
                  revision: uiRevision,
                  utf16Length: 1,
                ),
                operations: const [
                  FlarkV3SourceIntentEdit(
                    startUtf16: 0,
                    endUtf16: 2,
                    replacement: FlarkV3StringSourcePayload(''),
                  ),
                  FlarkV3SourceIntentEdit(
                    startUtf16: 2,
                    endUtf16: 2,
                    replacement: FlarkV3StringSourcePayload('x'),
                  ),
                ],
              ),
            ],
            payloadUtf16: 1,
          );

      expect(
        () => FlarkV3ParserSessionWireCodec.encodeCommand(
          editCommand(uiRevision: 2),
          correlationId: 6,
          establishedBinding: _binding,
        ),
        throwsArgumentError,
      );

      final valid = FlarkV3ParserSessionWireCodec.encodeCommand(
        editCommand(uiRevision: 1),
        correlationId: 6,
        establishedBinding: _binding,
      );
      const intentOffset = FlarkV3WireProtocol.headerBytes + 28 + 28;

      final revisionGap = Uint8List.fromList(valid);
      ByteData.sublistView(
        revisionGap,
      ).setUint32(intentOffset + 8, 2, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          revisionGap,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );

      final overlap = Uint8List.fromList(valid);
      const secondOperationOffset = intentOffset + 16 + 64 + 16;
      ByteData.sublistView(
        overlap,
      ).setUint32(secondOperationOffset, 1, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          overlap,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );
    });

    test(
      'returns accepted and stale source receipts without a second lease',
      () {
        for (final source in [
          const FlarkV3SourceWorkerSyncAckReceipt.acknowledged(
            droppedIntentEntries: 2,
            droppedPayloadUtf16: 5,
            droppedDeletedUtf16: 3,
            droppedOperationCount: 2,
            workerRevision: 7,
          ),
          const FlarkV3SourceWorkerSyncAckReceipt.stale(workerRevision: 6),
        ]) {
          final disposition =
              source.disposition ==
                  FlarkV3SourceWorkerSyncAckDisposition.acknowledged
              ? FlarkV3ParserEventDisposition.accepted
              : FlarkV3ParserEventDisposition.stale;
          final receipt = FlarkV3ParserEventReceipt(
            eventId: 19,
            workerGeneration: _binding.workerGeneration,
            disposition: disposition,
            sourceSync: source,
          );
          final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
            FlarkV3ParserSessionWireCodec.encodeParserCommand(
              receipt,
              binding: _binding,
              correlationId: receipt.eventId,
            ),
            establishedBinding: _binding,
          );
          final command =
              decoded.command as FlarkV3ParserSessionEventReceiptCommand;
          expect(command.eventId, receipt.eventId);
          expect(command.disposition, disposition);
          expect(command.sourceSync?.disposition, source.disposition);
          expect(
            command.sourceSync?.droppedIntentEntries,
            source.droppedIntentEntries,
          );
          expect(command.sourceSync?.workerRevision, source.workerRevision);
          expect(command.toParserCommand().sourceSync, isNotNull);
        }
      },
    );
  });

  group('FlarkV3ParserSessionWireCodec canonical SourceFacts', () {
    const lineage = FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: 9,
      requestId: 31,
      workerGeneration: 3,
      workerReplicaRevision: 7,
      uiRevision: 7,
      utf16Length: 10,
      intentHighWater: 4,
    );
    final page = FlarkV3CanonicalSourceFactCheckpointPage(
      lineage: lineage,
      pageOrdinal: 0,
      pageCount: 1,
      checkpointCount: 2,
      checkpointSpacingUtf16: 8,
      checkpoints: const [
        FlarkV3SourcePrefixFacts(
          utf16Offset: 4,
          utf8Offset: 5,
          newlines: 1,
          hash: FlarkV3ContentHash128(11, 12, 13, 14),
        ),
        FlarkV3SourcePrefixFacts(
          utf16Offset: 10,
          utf8Offset: 12,
          newlines: 2,
          hash: FlarkV3ContentHash128(21, 22, 23, 24),
        ),
      ],
    );
    const completion = FlarkV3CanonicalSourceFactCompletion(
      lineage: lineage,
      fingerprintAlgorithm: 1,
      fingerprint: FlarkV3SourceFingerprint(
        revision: 7,
        utf16Length: 10,
        utf8Length: 12,
        contentHash128: FlarkV3ContentHash128(41, 42, 43, 44),
      ),
      logicalLineBreaks: 2,
      checkpointSpacingUtf16: 8,
      checkpointCount: 2,
      pageCount: 1,
      checkpointHash128: FlarkV3ContentHash128(51, 52, 53, 54),
    );
    const proof = FlarkV3CanonicalSourcePromotionProof(
      lineage: lineage,
      fingerprintAlgorithm: 1,
      fingerprint: FlarkV3SourceFingerprint(
        revision: 7,
        utf16Length: 10,
        utf8Length: 12,
        contentHash128: FlarkV3ContentHash128(41, 42, 43, 44),
      ),
      logicalLineBreaks: 2,
      checkpointSpacingUtf16: 8,
      checkpointCount: 2,
      pageCount: 1,
      checkpointHash128: FlarkV3ContentHash128(51, 52, 53, 54),
    );
    const completionWords = <int>[
      31,
      7,
      7,
      10,
      4,
      1,
      7,
      10,
      12,
      2,
      8,
      2,
      1,
      41,
      42,
      43,
      44,
      51,
      52,
      53,
      54,
    ];

    test('matches Rust page and completion schema-three goldens', () {
      final pageBytes = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionSourceFactsPageEvent(
          binding: _binding,
          eventId: 30,
          page: page,
        ),
        expectedBinding: _binding,
      );
      final pageFrame = FlarkV3WireProtocol.decode(
        pageBytes,
        kind: FlarkV3WireFrameKind.request,
      );
      expect(pageFrame.opcode, FlarkV3WireOpcode.parserPoll);
      expect(
        pageFrame.payload,
        orderedEquals(
          _sessionPayload(2, const [
            31,
            7,
            7,
            10,
            4,
            8,
            0,
            1,
            2,
            2,
            5,
            4,
            1,
            11,
            12,
            13,
            14,
            12,
            10,
            2,
            21,
            22,
            23,
            24,
          ]),
        ),
      );
      final decodedPage =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                pageBytes,
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionSourceFactsPageEvent;
      expect(decodedPage.page.lineage, lineage);
      expect(decodedPage.page.checkpoints, orderedEquals(page.checkpoints));
      expect(decodedPage.toParserEvent().binding, _binding);

      final completionBytes = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionSourceFactsCompletedEvent(
          binding: _binding,
          eventId: 31,
          completion: completion,
        ),
        expectedBinding: _binding,
      );
      final completionFrame = FlarkV3WireProtocol.decode(
        completionBytes,
        kind: FlarkV3WireFrameKind.request,
      );
      expect(completionFrame.opcode, FlarkV3WireOpcode.parserPoll);
      expect(
        completionFrame.payload,
        orderedEquals(_sessionPayload(3, completionWords)),
      );
      final decodedCompletion =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                completionBytes,
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionSourceFactsCompletedEvent;
      expect(decodedCompletion.completion.lineage, lineage);
      expect(decodedCompletion.completion.fingerprint, completion.fingerprint);
      expect(decodedCompletion.toParserEvent().binding, _binding);
    });

    test('matches Rust incremental SourceFacts delta schema-three goldens', () {
      const targetGuard = FlarkV3ContentHash128(81, 82, 83, 84);
      const replacementHash = FlarkV3ContentHash128(91, 92, 93, 94);
      const deltaHeader = FlarkV3ParserSourceFactsDeltaHeader(
        lineage: lineage,
        baseFingerprint: FlarkV3SourceFingerprint(
          revision: 6,
          utf16Length: 9,
          utf8Length: 11,
          contentHash128: FlarkV3ContentHash128(61, 62, 63, 64),
        ),
        baseCheckpointRootGuard128: FlarkV3ContentHash128(71, 72, 73, 74),
        baseCheckpointCount: 2,
        basePageCount: 1,
        baseCheckpointSpacingUtf16: 8,
        basePageStart: 0,
        basePageEnd: 1,
        targetPageStart: 0,
        targetPageEnd: 1,
        targetCheckpointCount: 2,
        targetPageCount: 1,
        targetCheckpointRootGuardAlgorithm:
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
        targetCheckpointRootGuard128: targetGuard,
        replacementCheckpointCount: 2,
      );
      final deltaPage = FlarkV3CanonicalSourceFactDeltaCheckpointPage(
        lineage: lineage,
        pageOrdinal: 0,
        checkpoints: page.checkpoints,
      );
      const deltaCompletion = FlarkV3CanonicalSourceFactDeltaCompletion(
        lineage: lineage,
        fingerprintAlgorithm: 1,
        fingerprint: FlarkV3SourceFingerprint(
          revision: 7,
          utf16Length: 10,
          utf8Length: 12,
          contentHash128: FlarkV3ContentHash128(41, 42, 43, 44),
        ),
        logicalLineBreaks: 2,
        checkpointSpacingUtf16: 8,
        checkpointCount: 2,
        pageCount: 1,
        checkpointRootGuardAlgorithm:
            flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
        checkpointRootGuard128: targetGuard,
        replacementCheckpointHash128: replacementHash,
      );

      final beginBytes = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionSourceFactsDeltaBeginEvent(
          binding: _binding,
          eventId: 32,
          header: deltaHeader,
        ),
        expectedBinding: _binding,
      );
      expect(
        FlarkV3WireProtocol.decode(
          beginBytes,
          kind: FlarkV3WireFrameKind.request,
        ).payload,
        orderedEquals(
          _sessionPayload(4, const [
            31,
            7,
            7,
            10,
            4,
            6,
            9,
            11,
            61,
            62,
            63,
            64,
            71,
            72,
            73,
            74,
            2,
            1,
            8,
            0,
            1,
            0,
            1,
            2,
            1,
            2,
            81,
            82,
            83,
            84,
            2,
          ]),
        ),
      );
      final decodedBegin =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                beginBytes,
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionSourceFactsDeltaBeginEvent;
      expect(decodedBegin.header.targetCheckpointRootGuard128, targetGuard);
      expect(decodedBegin.toParserEvent().binding, _binding);

      final pageBytes = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionSourceFactsDeltaPageEvent(
          binding: _binding,
          eventId: 33,
          page: deltaPage,
        ),
        expectedBinding: _binding,
      );
      expect(
        FlarkV3WireProtocol.decode(
          pageBytes,
          kind: FlarkV3WireFrameKind.request,
        ).payload,
        orderedEquals(
          _sessionPayload(5, const [
            31,
            7,
            7,
            10,
            4,
            0,
            2,
            5,
            4,
            1,
            11,
            12,
            13,
            14,
            12,
            10,
            2,
            21,
            22,
            23,
            24,
          ]),
        ),
      );
      final decodedPage =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                pageBytes,
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionSourceFactsDeltaPageEvent;
      expect(decodedPage.page.pageOrdinal, 0);
      expect(decodedPage.page.checkpoints, orderedEquals(page.checkpoints));

      final completionBytes = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionSourceFactsDeltaCompletedEvent(
          binding: _binding,
          eventId: 34,
          completion: deltaCompletion,
        ),
        expectedBinding: _binding,
      );
      expect(
        FlarkV3WireProtocol.decode(
          completionBytes,
          kind: FlarkV3WireFrameKind.request,
        ).payload,
        orderedEquals(
          _sessionPayload(6, const [
            31,
            7,
            7,
            10,
            4,
            1,
            7,
            10,
            12,
            2,
            8,
            2,
            1,
            41,
            42,
            43,
            44,
            81,
            82,
            83,
            84,
            2,
            91,
            92,
            93,
            94,
          ]),
        ),
      );
      final decodedCompletion =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                completionBytes,
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionSourceFactsDeltaCompletedEvent;
      expect(
        decodedCompletion.completion.checkpointRootGuardAlgorithm,
        flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
      );
      expect(
        decodedCompletion.completion.replacementCheckpointHash128,
        replacementHash,
      );
      expect(decodedCompletion.toParserEvent().binding, _binding);
    });

    test('round trips only an accepted exact promotion proof', () {
      final receipt = FlarkV3ParserEventReceipt(
        eventId: 31,
        binding: _binding,
        disposition: FlarkV3ParserEventDisposition.accepted,
        sourceCertification: proof,
      );
      final bytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        receipt,
        binding: _binding,
        correlationId: receipt.eventId,
      );
      final frame = FlarkV3WireProtocol.decode(
        bytes,
        kind: FlarkV3WireFrameKind.request,
      );
      expect(
        frame.payload,
        orderedEquals(_sessionPayload(0, const [0, 1, ...completionWords])),
      );
      final decoded =
          FlarkV3ParserSessionWireCodec.decodeCommand(
                bytes,
                establishedBinding: _binding,
              ).command
              as FlarkV3ParserSessionEventReceiptCommand;
      expect(decoded.sourceSync, isNull);
      expect(decoded.sourceCertification?.lineage, lineage);
      expect(
        decoded.sourceCertification?.checkpointHash128,
        proof.checkpointHash128,
      );
      expect(decoded.toParserCommand().sourceCertification, isNotNull);

      final crossedFingerprint = Uint8List.fromList(bytes);
      ByteData.sublistView(crossedFingerprint).setUint32(
        FlarkV3WireProtocol.headerBytes + 28 + 8 + 24,
        8,
        Endian.little,
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          crossedFingerprint,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );

      final staleProof = Uint8List.fromList(bytes);
      ByteData.sublistView(staleProof).setUint16(
        FlarkV3WireProtocol.headerBytes + 2,
        FlarkV3ParserEventDisposition.stale.index,
        Endian.little,
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          staleProof,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );

      final crossedReceiptKinds = Uint8List.fromList(bytes);
      ByteData.sublistView(
        crossedReceiptKinds,
      ).setUint32(FlarkV3WireProtocol.headerBytes + 28, 1, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          crossedReceiptKinds,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );
    });
  });

  group('FlarkV3ParserSessionWireCodec lifecycle', () {
    test('opens fresh and recovers only through the next exact generation', () {
      final freshBytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserOpen(binding: _binding, mode: FlarkV3ParserOpenMode.fresh),
        binding: _binding,
        correlationId: 7,
      );
      expect(freshBytes, orderedEquals(_freshOpenGolden));
      final fresh = FlarkV3ParserSessionWireCodec.decodeCommand(freshBytes);
      expect(
        (fresh.command as FlarkV3ParserSessionOpenCommand).mode,
        FlarkV3ParserOpenMode.fresh,
      );

      final recoveredBinding = _binding.copyWith(workerGeneration: 4);
      final recoveryBytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserOpen(
          binding: recoveredBinding,
          mode: FlarkV3ParserOpenMode.recovery,
        ),
        binding: recoveredBinding,
        correlationId: 8,
      );
      final recovered = FlarkV3ParserSessionWireCodec.decodeCommand(
        recoveryBytes,
        establishedBinding: _binding,
      );
      expect(
        (recovered.command as FlarkV3ParserSessionOpenCommand).mode,
        FlarkV3ParserOpenMode.recovery,
      );
      expect(recovered.command.binding, recoveredBinding);

      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          recoveryBytes,
          establishedBinding: _binding.copyWith(workerGeneration: 2),
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.identityMismatch),
        ),
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          recoveryBytes,
          establishedBinding: _binding.copyWith(sourceSessionIdentity: 10),
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.identityMismatch),
        ),
      );
    });

    test('round trips a fully bound typed supersede command', () {
      final bytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserSupersede(binding: _binding, targetUiRevision: 19),
        binding: _binding,
        correlationId: 41,
      );
      final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
        bytes,
        establishedBinding: _binding,
      );
      final command = decoded.command as FlarkV3ParserSessionSupersedeCommand;
      expect(command.binding, _binding);
      expect(command.targetUiRevision, 19);

      expect(
        () => FlarkV3ParserSessionWireCodec.encodeParserCommand(
          FlarkV3ParserSupersede(
            binding: _binding.copyWith(sourceSessionIdentity: 10),
            targetUiRevision: 19,
          ),
          binding: _binding,
          correlationId: 42,
        ),
        throwsArgumentError,
      );
    });

    test('round trips an exact-base late inline refinement command', () {
      final source = FlarkV3SourceVersion(
        documentSession: _document,
        revision: 7,
        metric: FlarkV3SourceMetric(bytes: 10, utf16: 9),
        contentHash: FlarkV3ContentHash128(11, 12, 13, 14),
      );
      final ack = FlarkV3StructuralAck(
        publicationSession: FlarkV3PublicationSessionId(21, 22, 23, 24),
        hostRevision: FlarkV3HostRevisionId(3),
        sourceVersion: source,
        sourceRoot: FlarkV3SourceRootId(31, 32),
        parseGeneration: 4,
        grammarRevision: 5,
        syntaxProfile: FlarkV3SyntaxProfileId(6),
        authorityMask: FlarkV3StructuralAuthorityMask.complete,
        recordCount: 17,
        sequenceDigest: FlarkV3ProtocolDigest128(41, 42, 43, 44),
        manifestDigest: FlarkV3ProtocolDigest128(51, 52, 53, 54),
      );
      final bytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserRefineInline(
          binding: _binding,
          refinementGeneration: 8,
          sourceVersion: source,
          baseAck: ack,
          byteOffset: 9,
          utf16Offset: 8,
          affinity: FlarkV3InlinePointAffinity.after,
          target: FlarkV3InlineRefinementTarget.bulletListItemProjection,
        ),
        binding: _binding,
        correlationId: 43,
      );
      final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
        bytes,
        establishedBinding: _binding,
      );
      final command =
          decoded.command as FlarkV3ParserSessionInlineRefinementCommand;
      expect(command.binding, _binding);
      expect(command.refinementGeneration, 8);
      expect(command.sourceVersion, source);
      expect(command.baseAck, ack);
      expect(command.byteOffset, 9);
      expect(command.utf16Offset, 8);
      expect(command.affinity, FlarkV3InlinePointAffinity.after);
      expect(
        command.target,
        FlarkV3InlineRefinementTarget.bulletListItemProjection,
      );

      final zeroGeneration = Uint8List.fromList(bytes);
      ByteData.sublistView(
        zeroGeneration,
      ).setUint32(FlarkV3WireProtocol.headerBytes + 28, 0, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          zeroGeneration,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );
    });

    test('round trips an exact-base bounded viewport presentation command', () {
      final source = FlarkV3SourceVersion(
        documentSession: _document,
        revision: 7,
        metric: FlarkV3SourceMetric(bytes: 10, utf16: 9),
        contentHash: FlarkV3ContentHash128(11, 12, 13, 14),
      );
      final ack = FlarkV3StructuralAck(
        publicationSession: FlarkV3PublicationSessionId(21, 22, 23, 24),
        hostRevision: FlarkV3HostRevisionId(3),
        sourceVersion: source,
        sourceRoot: FlarkV3SourceRootId(31, 32),
        parseGeneration: 4,
        grammarRevision: 5,
        syntaxProfile: FlarkV3SyntaxProfileId(6),
        authorityMask: FlarkV3StructuralAuthorityMask.complete,
        recordCount: 17,
        sequenceDigest: FlarkV3ProtocolDigest128(41, 42, 43, 44),
        manifestDigest: FlarkV3ProtocolDigest128(51, 52, 53, 54),
      );
      final limits = FlarkV3ParserViewportPresentationLimits(
        maximumStructuralEntries: 47,
        maximumStoragePages: 2,
        maximumInlineLeaves: 24,
      );
      final bytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserPresentViewport(
          binding: _binding,
          viewportGeneration: 9,
          sourceVersion: source,
          baseAck: ack,
          requestedStartUtf8: 0,
          requestedStartUtf16: 0,
          requestedEndUtf8: 10,
          requestedEndUtf16: 9,
          startBlockOrdinal: FlarkV3ProtocolU64(lowWord: 8, highWord: 1),
          startUtf8: 0,
          startUtf16: 0,
          limits: limits,
        ),
        binding: _binding,
        correlationId: 44,
      );
      final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
        bytes,
        establishedBinding: _binding,
      );
      final command =
          decoded.command as FlarkV3ParserSessionViewportPresentationCommand;
      expect(command.binding, _binding);
      expect(command.viewportGeneration, 9);
      expect(command.sourceVersion, source);
      expect(command.baseAck, ack);
      expect(command.startBlockOrdinal.lowWord, 8);
      expect(command.startBlockOrdinal.highWord, 1);
      expect(command.requestedEndUtf8, 10);
      expect(command.requestedEndUtf16, 9);
      expect(command.limits.maximumStructuralEntries, 47);
      expect(command.limits.maximumInlineLeaves, 24);
      expect(command.limits.maximumInlineLeafSourceBytes, 8 * 1024);

      expect(
        () => FlarkV3ParserViewportPresentationLimits(
          maximumInlineLeafSourceBytes: 4096,
          maximumInlineSourceBytes: 2048,
        ),
        throwsRangeError,
      );

      final skippedPrefix = Uint8List.fromList(bytes);
      const sourceVersionBytes = 44;
      const structuralAckBytes = 124;
      final startUtf8Offset =
          FlarkV3WireProtocol.headerBytes +
          28 +
          4 +
          sourceVersionBytes +
          structuralAckBytes +
          24;
      ByteData.sublistView(
        skippedPrefix,
      ).setUint32(startUtf8Offset, 1, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          skippedPrefix,
          establishedBinding: _binding,
        ),
        throwsA(_sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue)),
      );
    });

    test('round trips opened, failure, begin-close, and parser-closed', () {
      final opened =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeEvent(
                  FlarkV3ParserSessionOpenedEvent(
                    binding: _binding,
                    eventId: 1,
                    mode: FlarkV3ParserOpenMode.fresh,
                  ),
                  expectedBinding: _binding,
                ),
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionOpenedEvent;
      expect(opened.mode, FlarkV3ParserOpenMode.fresh);
      expect(opened.toParserEvent().binding, _binding);

      final failed =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeParserEvent(
                  FlarkV3ParserFailed(
                    eventId: 2,
                    workerGeneration: _binding.workerGeneration,
                    failureCode: 0x1020,
                  ),
                  binding: _binding,
                ),
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionFailedEvent;
      expect(failed.failureCode, 0x1020);
      expect(failed.toParserEvent().failureCode, 0x1020);
      expect(
        () => FlarkV3ParserSessionWireCodec.encodeParserEvent(
          FlarkV3ParserFailed(
            eventId: 9,
            workerGeneration: _binding.workerGeneration + 1,
            failureCode: 1,
          ),
          binding: _binding,
        ),
        throwsArgumentError,
      );

      final unavailable =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeParserEvent(
                  FlarkV3ParserInlineRefinementUnavailable(
                    eventId: 3,
                    binding: _binding,
                    refinementGeneration: 9,
                    reasonCode: FlarkV3ParserInlineRefinementUnavailable
                        .retryableBusyReason,
                  ),
                  binding: _binding,
                ),
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionInlineRefinementUnavailableEvent;
      expect(unavailable.refinementGeneration, 9);
      expect(
        unavailable.reasonCode,
        FlarkV3ParserInlineRefinementUnavailable.retryableBusyReason,
      );

      final viewportUnavailable =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeParserEvent(
                  FlarkV3ParserViewportPresentationUnavailable(
                    eventId: 4,
                    binding: _binding,
                    viewportGeneration: 11,
                    reasonCode: FlarkV3ParserViewportPresentationUnavailable
                        .budgetExceededReason,
                  ),
                  binding: _binding,
                ),
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionViewportPresentationUnavailableEvent;
      expect(viewportUnavailable.viewportGeneration, 11);
      expect(
        viewportUnavailable.reasonCode,
        FlarkV3ParserViewportPresentationUnavailable.budgetExceededReason,
      );

      for (final generation in <int?>[_binding.workerGeneration, null]) {
        final close = FlarkV3ParserSessionWireCodec.decodeCommand(
          FlarkV3ParserSessionWireCodec.encodeParserCommand(
            FlarkV3ParserBeginClose(generation),
            binding: _binding,
            correlationId: 3,
          ),
          establishedBinding: _binding,
        );
        expect(
          (close.command as FlarkV3ParserSessionBeginCloseCommand)
              .activeGeneration,
          generation ?? 0,
        );
      }

      final closed =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                FlarkV3ParserSessionWireCodec.encodeParserEvent(
                  FlarkV3ParserClosed(
                    eventId: 4,
                    workerGeneration: _binding.workerGeneration,
                  ),
                  binding: _binding,
                ),
                expectedBinding: _binding,
              )
              as FlarkV3ParserSessionClosedEvent;
      expect(closed.toParserEvent().workerGeneration, 3);
    });

    test('bounds drain progress by the exact grant', () {
      final grant = FlarkV3ParserSessionDrainGrant(
        binding: _binding,
        drainId: 21,
        maximumTransitions: 3,
      );
      final decodedGrant = FlarkV3ParserSessionWireCodec.decodeCommand(
        FlarkV3ParserSessionWireCodec.encodeCommand(
          grant,
          correlationId: grant.drainId,
          establishedBinding: _binding,
        ),
        establishedBinding: _binding,
      );
      expect(
        (decodedGrant.command as FlarkV3ParserSessionDrainGrant)
            .maximumTransitions,
        3,
      );

      final progress = FlarkV3ParserSessionDrainProgressEvent(
        binding: _binding,
        eventId: 22,
        drainId: grant.drainId,
        releasedSourceLeases: 1,
        releasedSourceBytes: 4096,
        arenaTransitions: 2,
        arenaNodesReclaimed: 17,
        complete: true,
      );
      final bytes = FlarkV3ParserSessionWireCodec.encodeEvent(
        progress,
        expectedBinding: _binding,
        expectedDrainGrant: grant,
      );
      final decoded =
          FlarkV3ParserSessionWireCodec.decodeEvent(
                bytes,
                expectedBinding: _binding,
                expectedDrainGrant: grant,
              )
              as FlarkV3ParserSessionDrainProgressEvent;
      expect(decoded.drainId, 21);
      expect(decoded.releasedSourceLeases, 1);
      expect(decoded.arenaTransitions, 2);
      expect(decoded.complete, isTrue);
      expect(
        decoded.toParserEvent().bindsGrant(
          FlarkV3ParserDrainGrant(
            binding: _binding,
            drainId: grant.drainId,
            maximumTransitions: grant.maximumTransitions,
          ),
        ),
        isTrue,
      );

      final smallerGrant = FlarkV3ParserSessionDrainGrant(
        binding: _binding,
        drainId: 21,
        maximumTransitions: 2,
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeEvent(
          bytes,
          expectedBinding: _binding,
          expectedDrainGrant: smallerGrant,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.identityMismatch),
        ),
      );
    });
  });

  group('FlarkV3ParserSessionWireCodec fail-closed decoding', () {
    test('rejects stale document, source, generation, and correlation', () {
      final session = FlarkV3SourceSession.fromString('live');
      final lease = session.beginWorkerSync() as FlarkV3SourceSnapshotSyncLease;
      final binding = _bindingFor(lease);
      final bytes = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserSynchronizeSource(lease),
        binding: binding,
        correlationId: lease.leaseId,
      );

      for (final expected in [
        binding.copyWith(documentSession: FlarkV3DocumentSessionId(8, 2, 3, 4)),
        binding.copyWith(
          sourceSessionIdentity: binding.sourceSessionIdentity + 1,
        ),
        binding.copyWith(workerGeneration: binding.workerGeneration + 1),
      ]) {
        expect(
          () => FlarkV3ParserSessionWireCodec.decodeCommand(
            bytes,
            establishedBinding: expected,
          ),
          throwsA(
            _sessionFailure(FlarkV3ParserSessionWireFailure.identityMismatch),
          ),
        );
      }

      final wrongCorrelation = Uint8List.fromList(bytes);
      ByteData.sublistView(
        wrongCorrelation,
      ).setUint32(16, lease.leaseId + 1, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          wrongCorrelation,
          establishedBinding: binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.identityMismatch),
        ),
      );
    });

    test('rejects malformed schema, variant, opcode, UTF-8, and bounds', () {
      final session = FlarkV3SourceSession.fromString('x');
      final lease = session.beginWorkerSync() as FlarkV3SourceSnapshotSyncLease;
      final binding = _bindingFor(lease);
      final valid = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        FlarkV3ParserSynchronizeSource(lease),
        binding: binding,
        correlationId: lease.leaseId,
      );

      _expectCommandMutation(
        valid,
        FlarkV3WireProtocol.headerBytes,
        1,
        binding,
        FlarkV3ParserSessionWireFailure.unsupportedSchema,
      );
      _expectCommandMutation(
        valid,
        FlarkV3WireProtocol.headerBytes + 2,
        7,
        binding,
        FlarkV3ParserSessionWireFailure.unknownVariant,
      );
      _expectCommandMutation(
        valid,
        8,
        FlarkV3WireOpcode.parserPoll.code & 0xff,
        binding,
        FlarkV3ParserSessionWireFailure.unexpectedOpcode,
      );
      _expectCommandMutation(
        valid,
        valid.length - 1,
        0xff,
        binding,
        FlarkV3ParserSessionWireFailure.invalidUtf8,
      );

      final oversized = Uint8List.fromList(valid);
      ByteData.sublistView(oversized).setUint32(
        FlarkV3WireProtocol.headerBytes + 84,
        FlarkV3ParserSessionWireCodec.maximumSnapshotUtf16 + 1,
        Endian.little,
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          oversized,
          establishedBinding: binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.oversizedValue),
        ),
      );

      final truncated = Uint8List.fromList(valid);
      ByteData.sublistView(
        truncated,
      ).setUint32(FlarkV3WireProtocol.headerBytes + 88, 2, Endian.little);
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          truncated,
          establishedBinding: binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.truncatedPayload),
        ),
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeCommand(
          _resizePayload(valid, 1),
          establishedBinding: binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.trailingPayload),
        ),
      );
    });

    test(
      'normalizes invalid common identities and rejects stale close epoch',
      () {
        final valid = FlarkV3ParserSessionWireCodec.encodeCommand(
          FlarkV3ParserSessionOpenCommand(
            binding: _binding,
            mode: FlarkV3ParserOpenMode.fresh,
          ),
          correlationId: 1,
        );
        for (final offset in [4, 24]) {
          final invalid = Uint8List.fromList(valid);
          ByteData.sublistView(
            invalid,
            FlarkV3WireProtocol.headerBytes,
          ).setUint32(offset, 0, Endian.little);
          expect(
            () => FlarkV3ParserSessionWireCodec.decodeCommand(invalid),
            throwsA(
              _sessionFailure(FlarkV3ParserSessionWireFailure.invalidValue),
            ),
          );
        }

        final close = FlarkV3ParserSessionWireCodec.encodeParserCommand(
          FlarkV3ParserBeginClose(_binding.workerGeneration),
          binding: _binding,
          correlationId: 2,
        );
        final staleClose = Uint8List.fromList(close);
        ByteData.sublistView(staleClose).setUint32(
          FlarkV3WireProtocol.headerBytes + 28,
          _binding.workerGeneration - 1,
          Endian.little,
        );
        expect(
          () => FlarkV3ParserSessionWireCodec.decodeCommand(
            staleClose,
            establishedBinding: _binding,
          ),
          throwsA(
            _sessionFailure(FlarkV3ParserSessionWireFailure.identityMismatch),
          ),
        );
      },
    );

    test('does not accept publication frames or duplicate payload bodies', () {
      final publication = FlarkV3ParserPublicationWireCodec.encodeEvent(
        FlarkV3ParserPublicationFailed(
          eventId: 1,
          binding: _binding,
          offerId: FlarkV3OfferId(1, 2, 3, 4),
          failureCode: 7,
        ),
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeEvent(
          publication,
          expectedBinding: _binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.unexpectedOpcode),
        ),
      );

      final shortWrongOpcode = FlarkV3WireProtocol.encode(
        FlarkV3WireFrame.owned(
          kind: FlarkV3WireFrameKind.request,
          opcode: FlarkV3WireOpcode.publishAbort,
          correlationId: 2,
          payload: Uint8List(0),
        ),
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeEvent(
          shortWrongOpcode,
          expectedBinding: _binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.unexpectedOpcode),
        ),
      );

      final close = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionClosedEvent(binding: _binding, eventId: 3),
        expectedBinding: _binding,
      );
      expect(
        () => FlarkV3ParserSessionWireCodec.decodeEvent(
          _resizePayload(close, 28),
          expectedBinding: _binding,
        ),
        throwsA(
          _sessionFailure(FlarkV3ParserSessionWireFailure.trailingPayload),
        ),
      );
    });
  });
}

final _document = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _binding = FlarkV3ParserSessionBinding(
  documentSession: _document,
  sourceSessionIdentity: 9,
  workerGeneration: 3,
);

final _freshOpenGolden = Uint8List.fromList([
  0x46,
  0x4c,
  0x4b,
  0x33,
  0x01,
  0x00,
  0x00,
  0x00,
  0x00,
  0x02,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x00,
  0x07,
  0x00,
  0x00,
  0x00,
  0x1c,
  0x00,
  0x00,
  0x00,
  0x03,
  0x00,
  0x00,
  0x00,
  0x03,
  0x00,
  0x00,
  0x00,
  0x01,
  0x00,
  0x00,
  0x00,
  0x02,
  0x00,
  0x00,
  0x00,
  0x03,
  0x00,
  0x00,
  0x00,
  0x04,
  0x00,
  0x00,
  0x00,
  0x09,
  0x00,
  0x00,
  0x00,
]);

Uint8List _sessionPayload(int variant, List<int> words) {
  const commonBytes = 28;
  final bytes = Uint8List(commonBytes + words.length * 4);
  final data = ByteData.sublistView(bytes);
  data
    ..setUint16(0, 3, Endian.little)
    ..setUint16(2, variant, Endian.little)
    ..setUint32(4, _binding.workerGeneration, Endian.little)
    ..setUint32(8, _binding.documentSession.word0, Endian.little)
    ..setUint32(12, _binding.documentSession.word1, Endian.little)
    ..setUint32(16, _binding.documentSession.word2, Endian.little)
    ..setUint32(20, _binding.documentSession.word3, Endian.little)
    ..setUint32(24, _binding.sourceSessionIdentity, Endian.little);
  for (var index = 0; index < words.length; index += 1) {
    data.setUint32(commonBytes + index * 4, words[index], Endian.little);
  }
  return bytes;
}

FlarkV3ParserSessionBinding _bindingFor(FlarkV3SourceWorkerSyncLease lease) =>
    FlarkV3ParserSessionBinding(
      documentSession: _document,
      sourceSessionIdentity: lease.sourceSessionIdentity,
      workerGeneration: lease.workerGeneration,
    );

extension on FlarkV3ParserSessionBinding {
  FlarkV3ParserSessionBinding copyWith({
    FlarkV3DocumentSessionId? documentSession,
    int? sourceSessionIdentity,
    int? workerGeneration,
  }) => FlarkV3ParserSessionBinding(
    documentSession: documentSession ?? this.documentSession,
    sourceSessionIdentity: sourceSessionIdentity ?? this.sourceSessionIdentity,
    workerGeneration: workerGeneration ?? this.workerGeneration,
  );
}

void _expectSourceAcknowledgement(
  FlarkV3SourceWorkerSyncAcknowledgement actual,
  FlarkV3SourceWorkerSyncAcknowledgement expected,
) {
  expect(actual.runtimeType, expected.runtimeType);
  expect(actual.sourceSessionIdentity, expected.sourceSessionIdentity);
  expect(actual.leaseId, expected.leaseId);
  expect(actual.workerGeneration, expected.workerGeneration);
  switch ((actual, expected)) {
    case (
      FlarkV3SourceSnapshotSyncAcknowledgement actual,
      FlarkV3SourceSnapshotSyncAcknowledgement expected,
    ):
      expect(actual.baseUiRevision, expected.baseUiRevision);
      expect(actual.startUtf16, expected.startUtf16);
      expect(actual.endUtf16, expected.endUtf16);
      expect(actual.throughIntentSequence, expected.throughIntentSequence);
      expect(actual.observedReplica, expected.observedReplica);
    case (
      FlarkV3SourceIntentSyncAcknowledgement actual,
      FlarkV3SourceIntentSyncAcknowledgement expected,
    ):
      expect(actual.firstSequence, expected.firstSequence);
      expect(actual.lastSequence, expected.lastSequence);
      expect(actual.entryCount, expected.entryCount);
      expect(actual.payloadUtf16, expected.payloadUtf16);
      expect(actual.observedReplica, expected.observedReplica);
    default:
      fail('Acknowledgement variants diverged.');
  }
}

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkV3SourceSession session,
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: switch (target) {
      FlarkV3KnownSourceStamp() => target.utf8Length,
      FlarkV3ProvisionalSourceStamp() =>
        utf8.encode(session.document.toString()).length,
    },
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

void _expectCommandMutation(
  Uint8List original,
  int offset,
  int value,
  FlarkV3ParserSessionBinding binding,
  FlarkV3ParserSessionWireFailure failure,
) {
  final bytes = Uint8List.fromList(original)..[offset] = value;
  expect(
    () => FlarkV3ParserSessionWireCodec.decodeCommand(
      bytes,
      establishedBinding: binding,
    ),
    throwsA(_sessionFailure(failure)),
  );
}

Uint8List _resizePayload(Uint8List original, int delta) {
  final resized = Uint8List(original.length + delta);
  resized.setRange(0, delta < 0 ? resized.length : original.length, original);
  final data = ByteData.sublistView(resized);
  data.setUint32(
    20,
    original.length - FlarkV3WireProtocol.headerBytes + delta,
    Endian.little,
  );
  return resized;
}

Matcher _sessionFailure(FlarkV3ParserSessionWireFailure failure) =>
    isA<FlarkV3ParserSessionWireFormatException>().having(
      (error) => error.failure,
      'failure',
      failure,
    );

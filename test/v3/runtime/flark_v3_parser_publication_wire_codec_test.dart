import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_publication_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3ParserPublicationWireCodec events', () {
    test('schema-v4 abort event has the exact binding-first golden layout', () {
      final encoded = FlarkV3ParserPublicationWireCodec.encodeEvent(
        FlarkV3ParserPublicationAbortRequested(
          eventId: 10,
          binding: _binding,
          offerId: _offer,
        ),
      );

      expect(
        _hex(encoded),
        '46 4c 4b 33 01 00 01 00 13 01 00 00 00 00 00 00 '
        '0a 00 00 00 2c 00 00 00 '
        '04 00 00 00 03 00 00 00 '
        '01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 '
        '02 00 00 00 '
        '09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00',
      );
    });

    test('round trips full and exact-base publication begins', () {
      for (final begin in [_fullBegin(), _deltaBegin()]) {
        final decoded =
            FlarkV3ParserPublicationWireCodec.decodeEvent(
                  FlarkV3ParserPublicationWireCodec.encodeEvent(
                    FlarkV3ParserPublicationBegin(
                      eventId: 7,
                      binding: _binding,
                      begin: begin,
                    ),
                  ),
                  expectedBinding: _binding,
                )
                as FlarkV3ParserPublicationBegin;

        expect(decoded.eventId, 7);
        expect(decoded.workerGeneration, 3);
        _expectBegin(decoded.begin, begin);
      }
    });

    test('round trips one opaque multi-frame packet without copying', () {
      final packetBytes = _twoFramePacketBytes();
      final event = FlarkV3ParserPublicationPacket(
        eventId: 8,
        binding: _binding,
        packet: FlarkV3HostPublicationPacket.fromCopiedBytes(packetBytes),
      );
      final encoded = FlarkV3ParserPublicationWireCodec.encodeEvent(event);
      final decoded =
          FlarkV3ParserPublicationWireCodec.decodeEvent(
                encoded,
                expectedBinding: _binding,
              )
              as FlarkV3ParserPublicationPacket;

      expect(decoded.packet.offerId, _offer);
      expect(decoded.packet.firstFrameOrdinal, 4);
      expect(decoded.packet.firstRecordOrdinal, 20);
      expect(decoded.packet.frameCount, 2);
      expect(decoded.packet.aggregateRecordCount, 5);
      expect(decoded.packet.aggregateFrameBytes, 5);
      expect(decoded.packet.rawBytes, orderedEquals(packetBytes));
      encoded[encoded.length - 1] = 0xdd;
      expect(
        decoded.packet.rawBytes.last,
        0xdd,
        reason: 'decode retains one view over the packet bytes',
      );
    });

    test('rejects packet magic, version, and flags before host transfer', () {
      final event = FlarkV3ParserPublicationPacket(
        eventId: 8,
        binding: _binding,
        packet: FlarkV3HostPublicationPacket.fromOwnedBytes(
          _twoFramePacketBytes(),
        ),
      );
      final encoded = FlarkV3ParserPublicationWireCodec.encodeEvent(event);
      const packetOffset = FlarkV3WireProtocol.headerBytes + 28;
      for (final (offset, value) in const [(0, 0), (4, 2), (6, 1)]) {
        final raw = _twoFramePacketBytes()..[offset] = value;
        expect(
          () => FlarkV3HostPublicationPacket.fromOwnedBytes(raw),
          throwsArgumentError,
        );
        final hostile = Uint8List.fromList(encoded)
          ..[packetOffset + offset] = value;
        expect(
          () => FlarkV3ParserPublicationWireCodec.decodeEvent(
            hostile,
            expectedBinding: _binding,
          ),
          throwsA(
            _payloadFailure(FlarkV3ParserPublicationWireFailure.invalidValue),
          ),
        );
      }
    });

    test(
      'keeps the frame directory opaque for incremental host validation',
      () {
        expect(FlarkV3HostPublicationPacket.wireHeaderBytes, 44);
        expect(FlarkV3HostPublicationPacket.maximumRawBytes, 71_724);
        final raw = _twoFramePacketBytes();
        ByteData.sublistView(raw).setUint32(
          FlarkV3HostPublicationPacket.wireHeaderBytes,
          0xffffffff,
          Endian.little,
        );

        final packet = FlarkV3HostPublicationPacket.fromOwnedBytes(raw);

        expect(packet.frameCount, 2);
        expect(packet.aggregateFrameBytes, 5);
        expect(identical(packet.rawBytes, raw), isTrue);
      },
    );

    test('round trips commit, abort, failure, and delivery proof', () {
      final commit = FlarkV3ParserPublicationCommitRequested(
        eventId: 9,
        binding: _binding,
        request: FlarkV3HostCommitRequest(
          offerId: _offer,
          actualFrameCount: 2,
          actualEncodedFrameBytes: 800,
          rollingTransportDigest: _digest(40),
          canonicalStreamDigest: _digest(50),
        ),
      );
      final decodedCommit =
          _roundTrip(commit) as FlarkV3ParserPublicationCommitRequested;
      expect(decodedCommit.request.offerId, _offer);
      expect(decodedCommit.request.actualFrameCount, 2);
      expect(decodedCommit.request.actualEncodedFrameBytes, 800);
      expect(decodedCommit.request.rollingTransportDigest, _digest(40));
      expect(decodedCommit.request.canonicalStreamDigest, _digest(50));

      final decodedAbort =
          _roundTrip(
                FlarkV3ParserPublicationAbortRequested(
                  eventId: 10,
                  binding: _binding,
                  offerId: _offer,
                ),
              )
              as FlarkV3ParserPublicationAbortRequested;
      expect(decodedAbort.offerId, _offer);

      final decodedFailure =
          _roundTrip(
                FlarkV3ParserPublicationFailed(
                  eventId: 11,
                  binding: _binding,
                  offerId: _offer,
                  failureCode: 0x1020,
                ),
              )
              as FlarkV3ParserPublicationFailed;
      expect(decodedFailure.offerId, _offer);
      expect(decodedFailure.failureCode, 0x1020);

      final decodedDelivery =
          _roundTrip(
                FlarkV3ParserPublicationDeliveryAcknowledged(
                  eventId: 12,
                  binding: _binding,
                  ack: _baseAck,
                ),
              )
              as FlarkV3ParserPublicationDeliveryAcknowledged;
      expect(decodedDelivery.ack, _baseAck);
    });

    test('enforces packet and unsigned event bounds', () {
      expect(
        () => FlarkV3ParserPublicationWireCodec.encodeEvent(
          FlarkV3ParserPublicationPacket(
            eventId: 1,
            binding: _binding,
            packet: FlarkV3HostPublicationPacket.fromOwnedBytes(
              _twoFramePacketBytes(
                trailingBytes: FlarkV3ParserPublicationWireCodec
                    .maximumPacketEncodedFrameBytes,
              ),
            ),
          ),
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3ParserPublicationWireCodec.encodeEvent(
          FlarkV3ParserPublicationAbortRequested(
            eventId: 0x100000000,
            binding: _binding,
            offerId: _offer,
          ),
        ),
        throwsRangeError,
      );
    });

    test('rejects schema, variant, opcode, truncation, and trailing data', () {
      final valid = FlarkV3ParserPublicationWireCodec.encodeEvent(
        FlarkV3ParserPublicationAbortRequested(
          eventId: 1,
          binding: _binding,
          offerId: _offer,
        ),
      );
      _expectPayloadMutation(
        valid,
        FlarkV3WireProtocol.headerBytes,
        3,
        FlarkV3ParserPublicationWireFailure.unsupportedSchema,
      );

      final validBegin = FlarkV3ParserPublicationWireCodec.encodeEvent(
        FlarkV3ParserPublicationBegin(
          eventId: 2,
          binding: _binding,
          begin: _fullBegin(),
        ),
      );
      _expectPayloadMutation(
        validBegin,
        FlarkV3WireProtocol.headerBytes + 28,
        2,
        FlarkV3ParserPublicationWireFailure.unsupportedSchema,
      );
      _expectPayloadMutation(
        valid,
        FlarkV3WireProtocol.headerBytes + 2,
        2,
        FlarkV3ParserPublicationWireFailure.unknownVariant,
      );
      _expectPayloadMutation(
        valid,
        8,
        FlarkV3WireOpcode.hostOpen.code & 0xff,
        FlarkV3ParserPublicationWireFailure.unexpectedOpcode,
      );

      expect(
        () => FlarkV3ParserPublicationWireCodec.decodeEvent(
          _resizePayload(valid, -1),
          expectedBinding: _binding,
        ),
        throwsA(
          _payloadFailure(FlarkV3ParserPublicationWireFailure.truncatedPayload),
        ),
      );
      expect(
        () => FlarkV3ParserPublicationWireCodec.decodeEvent(
          _resizePayload(valid, 1),
          expectedBinding: _binding,
        ),
        throwsA(
          _payloadFailure(FlarkV3ParserPublicationWireFailure.trailingPayload),
        ),
      );
    });

    test('rejects unsupported manifest schema and values beyond v1 lanes', () {
      expect(() => _fullBegin(schema: 2), throwsArgumentError);
      expect(
        () =>
            FlarkV3SourceMetric(bytes: flarkV3TransportV1Maximum + 1, utf16: 0),
        throwsRangeError,
      );
      expect(
        () => FlarkV3HostRevisionId(flarkV3TransportV1Maximum + 1),
        throwsRangeError,
      );
      expect(
        () => FlarkV3SourceVersion(
          documentSession: _documentSession,
          revision: flarkV3TransportV1Maximum + 1,
          metric: FlarkV3SourceMetric.zero,
          contentHash: FlarkV3ContentHash128.zero,
        ),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ParserSessionBinding(
          documentSession: _documentSession,
          sourceSessionIdentity: 1,
          workerGeneration: flarkV3TransportV1Maximum + 1,
        ),
        throwsRangeError,
      );
      final frameOverflow = _twoFramePacketBytes();
      ByteData.sublistView(
        frameOverflow,
      ).setUint32(24, flarkV3TransportV1Maximum, Endian.little);
      expect(
        () => FlarkV3HostPublicationPacket.fromOwnedBytes(frameOverflow),
        throwsArgumentError,
      );
      final recordOverflow = _twoFramePacketBytes();
      ByteData.sublistView(
        recordOverflow,
      ).setUint32(28, flarkV3TransportV1Maximum, Endian.little);
      expect(
        () => FlarkV3HostPublicationPacket.fromOwnedBytes(recordOverflow),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3HostQueryBudget(
          maxEncodedBytes: 1,
          maxOpenDepth: 1,
          maxLeafCount: 1,
          maxTreeNodesVisited: flarkV3TransportV1Maximum + 1,
        ),
        throwsRangeError,
      );
      expect(
        () => FlarkV3HostPacketCredit(
          offerId: _offer,
          nextFrameOrdinal: flarkV3TransportV1Maximum + 1,
        ),
        throwsRangeError,
      );
    });

    test(
      'rejects crossed document, source identity, and generation bindings',
      () {
        final encoded = FlarkV3ParserPublicationWireCodec.encodeEvent(
          FlarkV3ParserPublicationAbortRequested(
            eventId: 21,
            binding: _binding,
            offerId: _offer,
          ),
        );
        for (final crossed in [
          FlarkV3ParserSessionBinding(
            documentSession: FlarkV3DocumentSessionId(9, 2, 3, 4),
            sourceSessionIdentity: _binding.sourceSessionIdentity,
            workerGeneration: _binding.workerGeneration,
          ),
          FlarkV3ParserSessionBinding(
            documentSession: _binding.documentSession,
            sourceSessionIdentity: _binding.sourceSessionIdentity + 1,
            workerGeneration: _binding.workerGeneration,
          ),
          FlarkV3ParserSessionBinding(
            documentSession: _binding.documentSession,
            sourceSessionIdentity: _binding.sourceSessionIdentity,
            workerGeneration: _binding.workerGeneration + 1,
          ),
        ]) {
          expect(
            () => FlarkV3ParserPublicationWireCodec.decodeEvent(
              encoded,
              expectedBinding: crossed,
            ),
            throwsA(
              _payloadFailure(
                FlarkV3ParserPublicationWireFailure.identityMismatch,
              ),
            ),
          );
        }

        expect(
          () => FlarkV3ParserPublicationWireCodec.encodeEvent(
            FlarkV3ParserPublicationBegin(
              eventId: 22,
              binding: FlarkV3ParserSessionBinding(
                documentSession: FlarkV3DocumentSessionId(9, 9, 9, 9),
                sourceSessionIdentity: 2,
                workerGeneration: 3,
              ),
              begin: _fullBegin(),
            ),
          ),
          throwsArgumentError,
        );
      },
    );
  });

  group('FlarkV3ParserPublicationWireCodec commands', () {
    test('schema-v4 host-poll result has the exact causal-ticket layout', () {
      final encoded = FlarkV3ParserPublicationWireCodec.encodeCommand(
        FlarkV3ParserHostPollCompleted(
          ticket: _ticket(FlarkV3ParserHostPollPhase.abort, pollTicket: 102),
          outcome: FlarkV3HostAbortComplete(_offer),
        ),
      );

      expect(
        _hex(encoded),
        '46 4c 4b 33 01 00 01 00 20 01 00 00 00 00 00 00 '
        '66 00 00 00 44 00 00 00 '
        '04 00 03 00 03 00 00 00 '
        '01 00 00 00 02 00 00 00 03 00 00 00 04 00 00 00 '
        '02 00 00 00 '
        '66 00 00 00 '
        '09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00 '
        '02 00 00 00 '
        '09 00 00 00 0a 00 00 00 0b 00 00 00 0c 00 00 00',
      );
    });

    test('rejects event credit owned by the session schema', () {
      expect(
        () => FlarkV3ParserPublicationWireCodec.encodeCommand(
          FlarkV3ParserEventReceipt(
            eventId: 12,
            binding: _binding,
            disposition: FlarkV3ParserEventDisposition.stale,
          ),
        ),
        throwsArgumentError,
      );
    });

    test('round trips terminal causal host-poll results and rejections', () {
      final cases = <(FlarkV3ParserHostPollTicket, FlarkV3HostPollOutcome)>[
        (
          _ticket(FlarkV3ParserHostPollPhase.packetCredit, pollTicket: 100),
          FlarkV3HostPacketCredit(offerId: _offer, nextFrameOrdinal: 6),
        ),
        (
          _ticket(FlarkV3ParserHostPollPhase.commit, pollTicket: 101),
          FlarkV3HostCommitted(_baseAck),
        ),
        (
          _ticket(FlarkV3ParserHostPollPhase.abort, pollTicket: 102),
          FlarkV3HostAbortComplete(_offer),
        ),
      ];
      for (final (ticket, outcome) in cases) {
        final frame = _roundTripCommand(
          FlarkV3ParserHostPollCompleted(ticket: ticket, outcome: outcome),
        );
        final decoded = frame.command as FlarkV3ParserHostPollCompleted;
        expect(frame.correlationId, ticket.pollTicket);
        expect(frame.pollTicket, ticket);
        expect(decoded.ticket, ticket);
        switch ((decoded.outcome, outcome)) {
          case (
            FlarkV3HostPacketCredit(:final offerId, :final nextFrameOrdinal),
            FlarkV3HostPacketCredit(),
          ):
            expect(offerId, _offer);
            expect(nextFrameOrdinal, 6);
          case (FlarkV3HostCommitted(:final ack), FlarkV3HostCommitted()):
            expect(ack, _baseAck);
          case (
            FlarkV3HostAbortComplete(:final offerId),
            FlarkV3HostAbortComplete(),
          ):
            expect(offerId, _offer);
          default:
            expect(decoded.outcome.runtimeType, outcome.runtimeType);
        }
      }

      for (final reason in FlarkV3HostRejectReason.values) {
        final ticket = _ticket(
          FlarkV3ParserHostPollPhase.abort,
          pollTicket: 103,
        );
        final decoded =
            _roundTripCommand(
                  FlarkV3ParserHostPollRejected(ticket: ticket, reason: reason),
                ).command
                as FlarkV3ParserHostPollRejected;
        expect(decoded.ticket, ticket);
        expect(decoded.reason, reason);
      }
    });

    test('requires a positive u32 poll ticket', () {
      expect(
        () => FlarkV3ParserHostPollTicket(
          binding: _binding,
          pollTicket: 0,
          offerId: _offer,
          phase: FlarkV3ParserHostPollPhase.commit,
        ),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ParserHostPollTicket(
          binding: _binding,
          pollTicket: 0x100000000,
          offerId: _offer,
          phase: FlarkV3ParserHostPollPhase.commit,
        ),
        throwsRangeError,
      );
      expect(
        () => FlarkV3ParserHostPollCompleted(
          ticket: _ticket(FlarkV3ParserHostPollPhase.commit, pollTicket: 9),
          outcome: FlarkV3HostPacketCredit(
            offerId: _offer,
            nextFrameOrdinal: 1,
          ),
        ),
        throwsArgumentError,
      );
    });

    test(
      'rejects ticket/correlation mismatch and noncanonical ticket zero',
      () {
        final command = FlarkV3ParserHostPollCompleted(
          ticket: _ticket(FlarkV3ParserHostPollPhase.abort, pollTicket: 77),
          outcome: FlarkV3HostAbortComplete(_offer),
        );
        final encoded = FlarkV3ParserPublicationWireCodec.encodeCommand(
          command,
        );
        final crossedCorrelation = Uint8List.fromList(encoded);
        ByteData.sublistView(
          crossedCorrelation,
        ).setUint32(16, 78, Endian.little);
        expect(
          () => FlarkV3ParserPublicationWireCodec.decodeCommand(
            crossedCorrelation,
            expectedBinding: _binding,
          ),
          throwsA(
            _payloadFailure(
              FlarkV3ParserPublicationWireFailure.correlationMismatch,
            ),
          ),
        );

        final zeroTicket = Uint8List.fromList(encoded);
        ByteData.sublistView(
          zeroTicket,
        ).setUint32(FlarkV3WireProtocol.headerBytes + 28, 0, Endian.little);
        expect(
          () => FlarkV3ParserPublicationWireCodec.decodeCommand(
            zeroTicket,
            expectedBinding: _binding,
          ),
          throwsA(
            _payloadFailure(FlarkV3ParserPublicationWireFailure.invalidValue),
          ),
        );
      },
    );

    test(
      'decodes duplicate tickets explicitly for endpoint replay rejection',
      () {
        final ticket = _ticket(
          FlarkV3ParserHostPollPhase.packetCredit,
          pollTicket: 91,
        );
        final bytes = FlarkV3ParserPublicationWireCodec.encodeCommand(
          FlarkV3ParserHostPollCompleted(
            ticket: ticket,
            outcome: FlarkV3HostPacketCredit(
              offerId: ticket.offerId,
              nextFrameOrdinal: 1,
            ),
          ),
        );
        final first = FlarkV3ParserPublicationWireCodec.decodeCommand(
          bytes,
          expectedBinding: _binding,
        );
        final replay = FlarkV3ParserPublicationWireCodec.decodeCommand(
          bytes,
          expectedBinding: _binding,
        );

        expect(first.pollTicket, ticket);
        expect(replay.pollTicket, ticket);
        expect(replay.correlationId, first.correlationId);
        expect(
          () => FlarkV3ParserPublicationWireCodec.decodeCommand(
            bytes,
            expectedBinding: _binding.nextGeneration(4),
          ),
          throwsA(
            _payloadFailure(
              FlarkV3ParserPublicationWireFailure.identityMismatch,
            ),
          ),
        );
      },
    );
  });
}

FlarkV3ParserEvent _roundTrip(FlarkV3ParserEvent event) =>
    FlarkV3ParserPublicationWireCodec.decodeEvent(
      FlarkV3ParserPublicationWireCodec.encodeEvent(event),
      expectedBinding: _binding,
    );

FlarkV3DecodedParserPublicationCommand _roundTripCommand(
  FlarkV3ParserCommand command,
) => FlarkV3ParserPublicationWireCodec.decodeCommand(
  FlarkV3ParserPublicationWireCodec.encodeCommand(command),
  expectedBinding: _binding,
);

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _binding = FlarkV3ParserSessionBinding(
  documentSession: _documentSession,
  sourceSessionIdentity: 2,
  workerGeneration: 3,
);
final _publicationSession = FlarkV3PublicationSessionId(5, 6, 7, 8);
final _targetPublicationSession = FlarkV3PublicationSessionId(15, 16, 17, 18);
final _offer = FlarkV3OfferId(9, 10, 11, 12);
final _profile = FlarkV3SyntaxProfileId(1);
final _authority = FlarkV3StructuralAuthorityMask.complete;

FlarkV3ParserHostPollTicket _ticket(
  FlarkV3ParserHostPollPhase phase, {
  required int pollTicket,
}) => FlarkV3ParserHostPollTicket(
  binding: _binding,
  pollTicket: pollTicket,
  offerId: _offer,
  phase: phase,
);

FlarkV3ProtocolDigest128 _digest(int first) =>
    FlarkV3ProtocolDigest128(first, first + 1, first + 2, first + 3);

FlarkV3SourceVersion _source(int revision) => FlarkV3SourceVersion(
  documentSession: _documentSession,
  revision: revision,
  metric: FlarkV3SourceMetric(bytes: revision * 10, utf16: revision * 8),
  contentHash: FlarkV3ContentHash128(
    revision,
    revision + 1,
    revision + 2,
    revision + 3,
  ),
);

final _baseAck = FlarkV3StructuralAck(
  publicationSession: _publicationSession,
  hostRevision: FlarkV3HostRevisionId(1),
  sourceVersion: _source(1),
  sourceRoot: FlarkV3SourceRootId(0, 1),
  parseGeneration: 1,
  grammarRevision: 1,
  syntaxProfile: _profile,
  authorityMask: _authority,
  recordCount: 3,
  sequenceDigest: _digest(60),
  manifestDigest: _digest(70),
);

FlarkV3HostOfferLimits get _limits => FlarkV3HostOfferLimits(
  maximumFrameCount: 8,
  maximumEncodedFrameBytes: 4096,
  maximumPacketBytes: 2048,
  maximumFrameBytes: 1024,
  maximumProgramChildren: 32,
);

FlarkV3HostOfferBegin _fullBegin({
  FlarkV3HostOfferLimits? limits,
  int schema = FlarkV3HostOfferBegin.supportedManifestSchema,
}) => FlarkV3HostOfferBegin(
  schema: schema,
  offerId: _offer,
  publicationSession: _publicationSession,
  targetHostRevision: FlarkV3HostRevisionId(2),
  sourceVersion: _source(2),
  sourceRoot: FlarkV3SourceRootId(0, 2),
  parseGeneration: 2,
  grammarRevision: 1,
  syntaxProfile: _profile,
  authorityMask: _authority,
  mode: FlarkV3PublicationMode.fullSnapshot,
  baseAck: null,
  transferredRecordCount: 2,
  targetRecordCount: 2,
  limits: limits ?? _limits,
);

FlarkV3HostOfferBegin _deltaBegin() => FlarkV3HostOfferBegin(
  offerId: _offer,
  publicationSession: _targetPublicationSession,
  targetHostRevision: FlarkV3HostRevisionId(2),
  sourceVersion: _source(2),
  sourceRoot: FlarkV3SourceRootId(0, 2),
  parseGeneration: 2,
  grammarRevision: 1,
  syntaxProfile: _profile,
  authorityMask: _authority,
  mode: FlarkV3PublicationMode.exactBaseReferencesDelta,
  baseAck: _baseAck,
  transferredRecordCount: 2,
  targetRecordCount: 4,
  limits: _limits,
);

void _expectBegin(
  FlarkV3HostOfferBegin actual,
  FlarkV3HostOfferBegin expected,
) {
  expect(actual.schema, expected.schema);
  expect(actual.offerId, expected.offerId);
  expect(actual.publicationSession, expected.publicationSession);
  expect(actual.targetHostRevision, expected.targetHostRevision);
  expect(actual.sourceVersion, expected.sourceVersion);
  expect(actual.sourceRoot, expected.sourceRoot);
  expect(actual.parseGeneration, expected.parseGeneration);
  expect(actual.grammarRevision, expected.grammarRevision);
  expect(actual.syntaxProfile, expected.syntaxProfile);
  expect(actual.authorityMask, expected.authorityMask);
  expect(actual.mode, expected.mode);
  expect(actual.baseAck, expected.baseAck);
  expect(actual.transferredRecordCount, expected.transferredRecordCount);
  expect(actual.targetRecordCount, expected.targetRecordCount);
  expect(actual.limits.maximumFrameCount, expected.limits.maximumFrameCount);
  expect(
    actual.limits.maximumEncodedFrameBytes,
    expected.limits.maximumEncodedFrameBytes,
  );
  expect(actual.limits.maximumPacketBytes, expected.limits.maximumPacketBytes);
  expect(actual.limits.maximumFrameBytes, expected.limits.maximumFrameBytes);
  expect(
    actual.limits.maximumProgramChildren,
    expected.limits.maximumProgramChildren,
  );
}

Uint8List _twoFramePacketBytes({int trailingBytes = 0}) {
  const frameBytes = 5;
  final bodyOffset =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      2 * FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  final bytes = Uint8List(bodyOffset + frameBytes + trailingBytes);
  final data = ByteData.sublistView(bytes);

  void writeId(int offset, FlarkV3ProtocolId128 id) {
    data
      ..setUint32(offset, id.word0, Endian.little)
      ..setUint32(offset + 4, id.word1, Endian.little)
      ..setUint32(offset + 8, id.word2, Endian.little)
      ..setUint32(offset + 12, id.word3, Endian.little);
  }

  bytes.setRange(0, 4, const [0x46, 0x50, 0x4b, 0x33]);
  data
    ..setUint16(4, FlarkV3HostPublicationPacket.wireVersion, Endian.little)
    ..setUint16(6, FlarkV3HostPublicationPacket.wireFlags, Endian.little);
  writeId(8, _offer);
  data
    ..setUint32(24, 4, Endian.little)
    ..setUint32(28, 20, Endian.little)
    ..setUint32(32, 2, Endian.little)
    ..setUint32(36, 5, Endian.little)
    ..setUint32(40, frameBytes, Endian.little)
    ..setUint32(44, 2, Endian.little)
    ..setUint32(48, 2, Endian.little);
  writeId(52, _digest(30));
  data
    ..setUint32(68, 3, Endian.little)
    ..setUint32(72, 3, Endian.little);
  writeId(76, _digest(40));
  bytes.setRange(bodyOffset, bodyOffset + frameBytes, [
    0xaa,
    0xbb,
    0xcc,
    0xdd,
    0xee,
  ]);
  return bytes;
}

void _expectPayloadMutation(
  Uint8List original,
  int offset,
  int value,
  FlarkV3ParserPublicationWireFailure failure,
) {
  final bytes = Uint8List.fromList(original)..[offset] = value;
  expect(
    () => FlarkV3ParserPublicationWireCodec.decodeEvent(
      bytes,
      expectedBinding: _binding,
    ),
    throwsA(_payloadFailure(failure)),
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

Matcher _payloadFailure(FlarkV3ParserPublicationWireFailure failure) =>
    isA<FlarkV3ParserPublicationWireFormatException>().having(
      (error) => error.failure,
      'failure',
      failure,
    );

String _hex(Uint8List bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join(' ');

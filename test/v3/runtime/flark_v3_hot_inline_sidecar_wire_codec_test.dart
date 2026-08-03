import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_publication_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3HotInlineSidecarWireCodec contract', () {
    test('authoritative Begin has the Rust layout and lossless u64 lanes', () {
      final begin = _begin(_authoritativeDisposition);
      final encoded = FlarkV3HotInlineSidecarWireCodec.encodeEvent(
        FlarkV3ParserInlineSidecarBegin(
          eventId: 30,
          binding: _binding,
          begin: begin,
        ),
      );

      expect(
        encoded.length,
        FlarkV3WireProtocol.headerBytes +
            FlarkV3HotInlineSidecarWireCodec.payloadPrefixBytes +
            FlarkV3HotInlineSidecarWireCodec.beginBytes,
      );
      final data = ByteData.sublistView(encoded);
      expect(data.getUint16(8, Endian.little), 0x0110);
      expect(
        data.getUint16(FlarkV3WireProtocol.headerBytes + 2, Endian.little),
        0x0100,
      );
      final bodyOffset =
          FlarkV3WireProtocol.headerBytes +
          FlarkV3HotInlineSidecarWireCodec.payloadPrefixBytes;
      expect(data.getUint32(bodyOffset, Endian.little), 3);
      expect(data.getUint32(bodyOffset + 4, Endian.little), 1);

      final decoded =
          FlarkV3HotInlineSidecarWireCodec.decodeEvent(
                encoded,
                expectedBinding: _binding,
              )
              as FlarkV3ParserInlineSidecarBegin;
      expect(decoded.eventId, 30);
      expect(decoded.begin.baseAck, _baseAck);
      expect(decoded.begin.binding.refinementGeneration.lowWord, 7);
      expect(decoded.begin.binding.refinementGeneration.highWord, 2);
      expect(decoded.begin.binding.blockOrdinal.lowWord, 3);
      expect(decoded.begin.binding.blockOrdinal.highWord, 1);
      final disposition =
          decoded.begin.envelope.disposition
              as FlarkV3HotInlineSidecarAuthoritative;
      expect(disposition.logicalPageCount, _u64(1, high: 1));
      expect(disposition.factCount, _u64(2, high: 1));
      expect(disposition.storagePageCount, _u64(1));
      expect(disposition.linkValueEntryCount, 0);
      expect(disposition.linkValueStoragePageCount, FlarkV3ProtocolU64.zero);
      expect(disposition.linkValueEncodedBytes, 0);
      expect(disposition.orderedCommitment256, _digest256(0x81818181));
      expect(decoded.begin.hasExactBase(_baseAck), isTrue);
    });

    test('nonempty link values retain the Rust Begin field order', () {
      final encoded = FlarkV3HotInlineSidecarWireCodec.encodeEvent(
        FlarkV3ParserInlineSidecarBegin(
          eventId: 31,
          binding: _binding,
          begin: _begin(_authoritativeLinkValueDisposition),
        ),
      );
      final bodyOffset =
          FlarkV3WireProtocol.headerBytes +
          FlarkV3HotInlineSidecarWireCodec.payloadPrefixBytes;
      final data = ByteData.sublistView(encoded);

      expect(data.getUint32(bodyOffset + 264, Endian.little), 2);
      expect(data.getUint32(bodyOffset + 268, Endian.little), 0x1234);
      expect(data.getUint32(bodyOffset + 272, Endian.little), 0x55667788);
      expect(data.getUint32(bodyOffset + 276, Endian.little), 0x11223344);

      final decoded =
          FlarkV3HotInlineSidecarWireCodec.decodeEvent(
                encoded,
                expectedBinding: _binding,
              )
              as FlarkV3ParserInlineSidecarBegin;
      final disposition =
          decoded.begin.envelope.disposition
              as FlarkV3HotInlineSidecarAuthoritative;
      expect(disposition.linkValueEntryCount, 2);
      expect(disposition.linkValueEncodedBytes, 0x1234);
      expect(
        disposition.linkValueStoragePageCount,
        _u64(0x55667788, high: 0x11223344),
      );
    });

    test(
      'authoritative Begin accepts only exact inline and projection widths',
      () {
        for (final descriptorBytes in const [160, 168, 280, 328]) {
          final decoded =
              _roundTrip(
                    FlarkV3ParserInlineSidecarBegin(
                      eventId: 31 + descriptorBytes,
                      binding: _binding,
                      begin: _begin(
                        _authoritativeDisposition,
                        descriptorBytes: descriptorBytes,
                      ),
                    ),
                  )
                  as FlarkV3ParserInlineSidecarBegin;
          expect(decoded.begin.envelope.ipr2DescriptorBytes, descriptorBytes);
        }

        expect(
          () => _begin(_authoritativeDisposition, descriptorBytes: 164),
          throwsArgumentError,
        );

        final encoded = FlarkV3HotInlineSidecarWireCodec.encodeEvent(
          FlarkV3ParserInlineSidecarBegin(
            eventId: 32,
            binding: _binding,
            begin: _begin(_authoritativeDisposition),
          ),
        );
        final bodyOffset =
            FlarkV3WireProtocol.headerBytes +
            FlarkV3HotInlineSidecarWireCodec.payloadPrefixBytes;
        ByteData.sublistView(
          encoded,
        ).setUint32(bodyOffset + 224, 164, Endian.little);
        expect(
          () => FlarkV3HotInlineSidecarWireCodec.decodeEvent(
            encoded,
            expectedBinding: _binding,
          ),
          throwsA(
            _sidecarFailure(FlarkV3HotInlineSidecarWireFailure.invalidValue),
          ),
        );
      },
    );

    test(
      'round trips unsupported, packet, commit, abort, failure, and ACK',
      () {
        final unsupported =
            FlarkV3HotInlineSidecarWireCodec.decodeEvent(
                  FlarkV3HotInlineSidecarWireCodec.encodeEvent(
                    FlarkV3ParserInlineSidecarBegin(
                      eventId: 31,
                      binding: _binding,
                      begin: _begin(_unsupportedDisposition),
                    ),
                  ),
                  expectedBinding: _binding,
                )
                as FlarkV3ParserInlineSidecarBegin;
        expect(
          unsupported.begin.envelope.disposition,
          isA<FlarkV3HotInlineSidecarUnsupported>(),
        );
        expect(unsupported.begin.envelope.transferredNodeCount, 1);
        expect(unsupported.begin.envelope.ipr2DescriptorBytes, 0);

        final packetBytes = _twoFramePacketBytes();
        final packet =
            FlarkV3HotInlineSidecarWireCodec.decodeEvent(
                  FlarkV3HotInlineSidecarWireCodec.encodeEvent(
                    FlarkV3ParserInlineSidecarPacket(
                      eventId: 32,
                      binding: _binding,
                      packet: FlarkV3HostPublicationPacket.fromCopiedBytes(
                        packetBytes,
                      ),
                    ),
                  ),
                  expectedBinding: _binding,
                )
                as FlarkV3ParserInlineSidecarPacket;
        expect(packet.packet.rawBytes, orderedEquals(packetBytes));
        expect(packet.packet.frameCount, 2);

        final commit =
            _roundTrip(
                  FlarkV3ParserInlineSidecarCommitRequested(
                    eventId: 33,
                    binding: _binding,
                    request: FlarkV3HotInlineSidecarCommitRequest(
                      offerId: _offer,
                      actualFrameCount: 4,
                      actualEncodedFrameBytes: 900,
                      rollingTransportDigest: _digest128(100),
                      rootStreamDigest: _digest128(110),
                    ),
                  ),
                )
                as FlarkV3ParserInlineSidecarCommitRequested;
        expect(commit.request.actualFrameCount, 4);
        expect(commit.request.rootStreamDigest, _digest128(110));

        expect(
          _roundTrip(
            FlarkV3ParserInlineSidecarAbortRequested(
              eventId: 34,
              binding: _binding,
              offerId: _offer,
            ),
          ),
          isA<FlarkV3ParserInlineSidecarAbortRequested>(),
        );
        final failed =
            _roundTrip(
                  FlarkV3ParserInlineSidecarFailed(
                    eventId: 35,
                    binding: _binding,
                    offerId: _offer,
                    failureCode: 0x1020,
                  ),
                )
                as FlarkV3ParserInlineSidecarFailed;
        expect(failed.failureCode, 0x1020);

        final ack =
            _roundTrip(
                  FlarkV3ParserInlineSidecarDeliveryAcknowledged(
                    eventId: 36,
                    binding: _binding,
                    ack: _sidecarAck,
                  ),
                )
                as FlarkV3ParserInlineSidecarDeliveryAcknowledged;
        expect(ack.ack, _sidecarAck);
      },
    );

    test('round trips all terminal poll values and sidecar rejection', () {
      final creditTicket = _ticket(
        FlarkV3ParserInlineSidecarHostPollPhase.packetCredit,
        200,
      );
      final credit = _roundTripCommand(
        FlarkV3ParserInlineSidecarHostPollCompleted(
          ticket: creditTicket,
          outcome: FlarkV3InlineSidecarHostPacketCredit(
            offerId: _offer,
            nextFrameOrdinal: 2,
          ),
        ),
      );
      expect(credit.pollTicket, creditTicket);
      expect(
        (credit.command as FlarkV3ParserInlineSidecarHostPollCompleted).outcome,
        isA<FlarkV3InlineSidecarHostPacketCredit>(),
      );

      final commitTicket = _ticket(
        FlarkV3ParserInlineSidecarHostPollPhase.commit,
        201,
      );
      final committed = _roundTripCommand(
        FlarkV3ParserInlineSidecarHostPollCompleted(
          ticket: commitTicket,
          outcome: FlarkV3InlineSidecarHostCommitted(_sidecarAck),
        ),
      );
      final committedOutcome =
          (committed.command as FlarkV3ParserInlineSidecarHostPollCompleted)
                  .outcome
              as FlarkV3InlineSidecarHostCommitted;
      expect(committedOutcome.ack, _sidecarAck);

      final abortTicket = _ticket(
        FlarkV3ParserInlineSidecarHostPollPhase.abort,
        202,
      );
      final aborted = _roundTripCommand(
        FlarkV3ParserInlineSidecarHostPollCompleted(
          ticket: abortTicket,
          outcome: FlarkV3InlineSidecarHostAbortComplete(_offer),
        ),
      );
      expect(
        (aborted.command as FlarkV3ParserInlineSidecarHostPollCompleted)
            .outcome,
        isA<FlarkV3InlineSidecarHostAbortComplete>(),
      );

      final rejected = _roundTripCommand(
        FlarkV3ParserInlineSidecarHostPollRejected(
          ticket: commitTicket,
          reason: FlarkV3HostRejectReason.baseMismatch,
        ),
      );
      expect(
        (rejected.command as FlarkV3ParserInlineSidecarHostPollRejected).reason,
        FlarkV3HostRejectReason.baseMismatch,
      );
    });

    test('structural and sidecar events and polls cross-reject', () {
      final structuralEvent = FlarkV3ParserPublicationWireCodec.encodeEvent(
        FlarkV3ParserPublicationAbortRequested(
          eventId: 40,
          binding: _binding,
          offerId: _offer,
        ),
      );
      expect(
        () => FlarkV3HotInlineSidecarWireCodec.decodeEvent(
          structuralEvent,
          expectedBinding: _binding,
        ),
        throwsA(
          _sidecarFailure(FlarkV3HotInlineSidecarWireFailure.unknownVariant),
        ),
      );

      final sidecarEvent = FlarkV3HotInlineSidecarWireCodec.encodeEvent(
        FlarkV3ParserInlineSidecarAbortRequested(
          eventId: 41,
          binding: _binding,
          offerId: _offer,
        ),
      );
      expect(
        () => FlarkV3ParserPublicationWireCodec.decodeEvent(
          sidecarEvent,
          expectedBinding: _binding,
        ),
        throwsA(
          _structuralFailure(
            FlarkV3ParserPublicationWireFailure.unknownVariant,
          ),
        ),
      );

      final structuralTicket = FlarkV3ParserHostPollTicket(
        binding: _binding,
        pollTicket: 210,
        offerId: _offer,
        phase: FlarkV3ParserHostPollPhase.commit,
      );
      final structuralCommand = FlarkV3ParserPublicationWireCodec.encodeCommand(
        FlarkV3ParserHostPollRejected(
          ticket: structuralTicket,
          reason: FlarkV3HostRejectReason.baseMismatch,
        ),
      );
      expect(
        () => FlarkV3HotInlineSidecarWireCodec.decodeCommand(
          structuralCommand,
          expectedBinding: _binding,
        ),
        throwsA(
          _sidecarFailure(FlarkV3HotInlineSidecarWireFailure.invalidValue),
        ),
      );

      final sidecarCommand = FlarkV3HotInlineSidecarWireCodec.encodeCommand(
        FlarkV3ParserInlineSidecarHostPollRejected(
          ticket: _ticket(FlarkV3ParserInlineSidecarHostPollPhase.commit, 211),
          reason: FlarkV3HostRejectReason.baseMismatch,
        ),
      );
      expect(
        () => FlarkV3ParserPublicationWireCodec.decodeCommand(
          sidecarCommand,
          expectedBinding: _binding,
        ),
        throwsA(
          _structuralFailure(FlarkV3ParserPublicationWireFailure.invalidValue),
        ),
      );
    });
  });
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _binding = FlarkV3ParserSessionBinding(
  documentSession: _documentSession,
  sourceSessionIdentity: 2,
  workerGeneration: 3,
);
final _offer = FlarkV3OfferId(9, 10, 11, 12);
final _structuralPublication = FlarkV3PublicationSessionId(5, 6, 7, 8);
final _sidecarPublication = FlarkV3PublicationSessionId(15, 16, 17, 18);
final _profile = FlarkV3SyntaxProfileId(1);

FlarkV3ProtocolU64 _u64(int low, {int high = 0}) =>
    FlarkV3ProtocolU64(lowWord: low, highWord: high);

FlarkV3ProtocolDigest128 _digest128(int first) =>
    FlarkV3ProtocolDigest128(first, first + 1, first + 2, first + 3);

FlarkV3ProtocolDigest256 _digest256(int word) =>
    FlarkV3ProtocolDigest256(word, word, word, word, word, word, word, word);

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
  publicationSession: _structuralPublication,
  hostRevision: FlarkV3HostRevisionId(1),
  sourceVersion: _source(1),
  sourceRoot: FlarkV3SourceRootId(0, 1),
  parseGeneration: 1,
  grammarRevision: 1,
  syntaxProfile: _profile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 3,
  sequenceDigest: _digest128(60),
  manifestDigest: _digest128(70),
);

FlarkV3HostOfferLimits get _limits => FlarkV3HostOfferLimits(
  maximumFrameCount: 8,
  maximumEncodedFrameBytes: 4096,
  maximumPacketBytes: 2048,
  maximumFrameBytes: 1024,
  maximumProgramChildren: 32,
);

final _authoritativeDisposition = FlarkV3HotInlineSidecarAuthoritative(
  logicalPageCount: _u64(1, high: 1),
  factCount: _u64(2, high: 1),
  storagePageCount: _u64(1),
  linkValueEntryCount: 0,
  linkValueStoragePageCount: FlarkV3ProtocolU64.zero,
  linkValueEncodedBytes: 0,
  orderedCommitment256: _digest256(0x81818181),
);

final _authoritativeLinkValueDisposition = FlarkV3HotInlineSidecarAuthoritative(
  logicalPageCount: _u64(1),
  factCount: _u64(2),
  storagePageCount: _u64(1),
  linkValueEntryCount: 2,
  linkValueStoragePageCount: _u64(0x55667788, high: 0x11223344),
  linkValueEncodedBytes: 0x1234,
  orderedCommitment256: _digest256(0x83838383),
);

final _unsupportedDisposition = FlarkV3HotInlineSidecarUnsupported(
  reason: 0x20000001,
  metadataCommitment256: _digest256(0x82828282),
);

FlarkV3HotInlineSidecarOfferBegin _begin(
  FlarkV3HotInlineSidecarDisposition disposition, {
  int descriptorBytes =
      FlarkV3HotInlineSidecarEnvelopeMetrics.ipr2FixedDescriptorBytes,
}) => FlarkV3HotInlineSidecarOfferBegin(
  offerId: _offer,
  publicationSession: _sidecarPublication,
  baseAck: _baseAck,
  binding: FlarkV3HotInlineSidecarBinding(
    parserProfile: _profile,
    refinementGeneration: _u64(7, high: 2),
    blockOrdinal: _u64(3, high: 1),
    physicalStartUtf8: 0,
    physicalEndUtf8: 10,
    visibleStartUtf8: 1,
    visibleEndUtf8: 9,
    physicalStartUtf16: 0,
    physicalEndUtf16: 8,
    visibleStartUtf16: 1,
    visibleEndUtf16: 7,
  ),
  envelope: FlarkV3HotInlineSidecarEnvelopeMetrics(
    ipr2DescriptorBytes: disposition is FlarkV3HotInlineSidecarAuthoritative
        ? descriptorBytes
        : 0,
    transferredNodeCount: disposition is FlarkV3HotInlineSidecarAuthoritative
        ? 2
        : 1,
    hio1EnvelopeDigest256: _digest256(0x91919191),
    disposition: disposition,
  ),
  limits: _limits,
);

final _sidecarAck = FlarkV3InlineSidecarAck(
  publicationSession: _sidecarPublication,
  baseAck: _baseAck,
  refinementGeneration: _u64(7, high: 2),
  blockOrdinal: _u64(3, high: 1),
  transferredNodeCount: 2,
  disposition: FlarkV3InlineSidecarAckDisposition.authoritative,
  hio1EnvelopeDigest256: _digest256(0x91919191),
  rootStreamDigest: _digest128(90),
);

FlarkV3ParserInlineSidecarEvent _roundTrip(
  FlarkV3ParserInlineSidecarEvent event,
) => FlarkV3HotInlineSidecarWireCodec.decodeEvent(
  FlarkV3HotInlineSidecarWireCodec.encodeEvent(event),
  expectedBinding: _binding,
);

FlarkV3ParserInlineSidecarHostPollTicket _ticket(
  FlarkV3ParserInlineSidecarHostPollPhase phase,
  int pollTicket,
) => FlarkV3ParserInlineSidecarHostPollTicket(
  binding: _binding,
  pollTicket: pollTicket,
  offerId: _offer,
  phase: phase,
);

FlarkV3DecodedInlineSidecarHostPollCommand _roundTripCommand(
  FlarkV3ParserInlineSidecarHostPollCommand command,
) => FlarkV3HotInlineSidecarWireCodec.decodeCommand(
  FlarkV3HotInlineSidecarWireCodec.encodeCommand(command),
  expectedBinding: _binding,
);

Uint8List _twoFramePacketBytes() {
  const frameBytes = 5;
  final bodyOffset =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      2 * FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  final bytes = Uint8List(bodyOffset + frameBytes);
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
    ..setUint32(24, 0, Endian.little)
    ..setUint32(28, 0, Endian.little)
    ..setUint32(32, 2, Endian.little)
    ..setUint32(36, 2, Endian.little)
    ..setUint32(40, frameBytes, Endian.little)
    ..setUint32(44, 2, Endian.little)
    ..setUint32(48, 1, Endian.little);
  writeId(52, _digest128(30));
  data
    ..setUint32(68, 3, Endian.little)
    ..setUint32(72, 1, Endian.little);
  writeId(76, _digest128(40));
  bytes.setRange(bodyOffset, bodyOffset + frameBytes, const [
    0xe6,
    1,
    0xe1,
    2,
    0xe2,
  ]);
  return bytes;
}

Matcher _sidecarFailure(FlarkV3HotInlineSidecarWireFailure failure) =>
    isA<FlarkV3HotInlineSidecarWireFormatException>().having(
      (error) => error.failure,
      'failure',
      failure,
    );

Matcher _structuralFailure(FlarkV3ParserPublicationWireFailure failure) =>
    isA<FlarkV3ParserPublicationWireFormatException>().having(
      (error) => error.failure,
      'failure',
      failure,
    );

import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_viewport_presentation_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

import '../support/flark_v3_publication_packet_fixture.dart';

void main() {
  group('VPB1 publication wire', () {
    test('events use disjoint variants and exact Rust body sizes', () {
      final packet = testPublicationPacket(
        offerId: _offer,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: 1,
        digest: _digest128(90),
        frameBytes: Uint8List.fromList(const <int>[1, 2, 3]),
      );
      final commit = _commit();
      final ack = _ack();
      final events = <FlarkV3ParserViewportPresentationEvent>[
        FlarkV3ParserViewportPresentationBegin(
          eventId: 1,
          binding: _sessionBinding,
          begin: _begin(),
        ),
        FlarkV3ParserViewportPresentationPacket(
          eventId: 2,
          binding: _sessionBinding,
          packet: packet,
        ),
        FlarkV3ParserViewportPresentationCommitRequested(
          eventId: 3,
          binding: _sessionBinding,
          request: commit,
        ),
        FlarkV3ParserViewportPresentationAbortRequested(
          eventId: 4,
          binding: _sessionBinding,
          offerId: _offer,
        ),
        FlarkV3ParserViewportPresentationFailed(
          eventId: 5,
          binding: _sessionBinding,
          offerId: _offer,
          failureCode: 9,
        ),
        FlarkV3ParserViewportPresentationDeliveryAcknowledged(
          eventId: 6,
          binding: _sessionBinding,
          ack: ack,
        ),
      ];

      final expectedBodyBytes = <int>[
        FlarkV3ViewportPresentationWireCodec.beginBytes,
        packet.rawBytes.length,
        56,
        16,
        20,
        FlarkV3ViewportPresentationWireCodec.ackBytes,
      ];
      for (var index = 0; index < events.length; index += 1) {
        final encoded = FlarkV3ViewportPresentationWireCodec.encodeEvent(
          events[index],
        );
        expect(
          encoded.length,
          FlarkV3WireProtocol.headerBytes +
              FlarkV3ViewportPresentationWireCodec.payloadPrefixBytes +
              expectedBodyBytes[index],
        );
        final payload = ByteData.sublistView(
          encoded,
          FlarkV3WireProtocol.headerBytes,
        );
        expect(
          payload.getUint16(2, Endian.little),
          index == 4 ? 0x0201 : 0x0200,
        );
        final decoded = FlarkV3ViewportPresentationWireCodec.decodeEvent(
          encoded,
          expectedBinding: _sessionBinding,
        );
        expect(decoded.runtimeType, events[index].runtimeType);
        expect(decoded.eventId, events[index].eventId);
      }
    });

    test('host-poll phases and outcomes are disjoint and replayable', () {
      final cases =
          <
            (
              FlarkV3ParserViewportPresentationHostPollPhase,
              FlarkV3ViewportPresentationHostPollOutcome,
              int,
              int,
            )
          >[
            (
              FlarkV3ParserViewportPresentationHostPollPhase.packetCredit,
              FlarkV3ViewportPresentationHostPacketCredit(
                offerId: _offer,
                nextFrameOrdinal: 1,
              ),
              0x0200,
              0x0210,
            ),
            (
              FlarkV3ParserViewportPresentationHostPollPhase.commit,
              FlarkV3ViewportPresentationHostCommitted(_ack()),
              0x0201,
              0x0211,
            ),
            (
              FlarkV3ParserViewportPresentationHostPollPhase.abort,
              FlarkV3ViewportPresentationHostAbortComplete(_offer),
              0x0202,
              0x0212,
            ),
          ];

      for (var index = 0; index < cases.length; index += 1) {
        final (phase, outcome, phaseCode, variant) = cases[index];
        final ticket = FlarkV3ParserViewportPresentationHostPollTicket(
          binding: _sessionBinding,
          pollTicket: index + 20,
          offerId: _offer,
          phase: phase,
        );
        final encoded = FlarkV3ViewportPresentationWireCodec.encodeCommand(
          FlarkV3ParserViewportPresentationHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
        final payload = ByteData.sublistView(
          encoded,
          FlarkV3WireProtocol.headerBytes,
        );
        expect(payload.getUint16(2, Endian.little), variant);
        expect(
          payload.getUint32(
            FlarkV3ViewportPresentationWireCodec.payloadPrefixBytes + 20,
            Endian.little,
          ),
          phaseCode,
        );
        final first = FlarkV3ViewportPresentationWireCodec.decodeCommand(
          encoded,
          expectedBinding: _sessionBinding,
        );
        final replay = FlarkV3ViewportPresentationWireCodec.decodeCommand(
          encoded,
          expectedBinding: _sessionBinding,
        );
        expect(first.command.runtimeType, replay.command.runtimeType);
        expect(first.pollTicket, ticket);
      }

      final rejectedTicket = FlarkV3ParserViewportPresentationHostPollTicket(
        binding: _sessionBinding,
        pollTicket: 30,
        offerId: _offer,
        phase: FlarkV3ParserViewportPresentationHostPollPhase.commit,
      );
      final rejected = FlarkV3ViewportPresentationWireCodec.encodeCommand(
        FlarkV3ParserViewportPresentationHostPollRejected(
          ticket: rejectedTicket,
          reason: FlarkV3HostRejectReason.foregroundBoundExceeded,
        ),
      );
      final rejectedPayload = ByteData.sublistView(
        rejected,
        FlarkV3WireProtocol.headerBytes,
      );
      expect(rejectedPayload.getUint16(2, Endian.little), 0x0200);
      expect(
        FlarkV3ViewportPresentationWireCodec.decodeCommand(
          rejected,
          expectedBinding: _sessionBinding,
        ).command,
        isA<FlarkV3ParserViewportPresentationHostPollRejected>(),
      );
    });
  });
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _sessionBinding = FlarkV3ParserSessionBinding(
  documentSession: _documentSession,
  sourceSessionIdentity: 5,
  workerGeneration: 6,
);
final _offer = FlarkV3OfferId(19, 20, 21, 22);
final _basePublication = FlarkV3PublicationSessionId(5, 6, 7, 8);
final _viewportPublication = FlarkV3PublicationSessionId(23, 24, 25, 26);

FlarkV3ProtocolU64 _u64(int value) => FlarkV3ProtocolU64.fromU32(value);
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
  publicationSession: _basePublication,
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
final _binding = FlarkV3ViewportPresentationBinding(
  viewportGeneration: 9,
  requestedRange: FlarkV3ViewportPresentationMetricRange(
    startUtf8: 0,
    startUtf16: 0,
    endUtf8: 10,
    endUtf16: 8,
  ),
  coveredRange: FlarkV3ViewportPresentationMetricRange(
    startUtf8: 0,
    startUtf16: 0,
    endUtf8: 10,
    endUtf16: 8,
  ),
  start: FlarkV3ViewportPresentationVisitStart(
    blockOrdinal: _u64(7),
    utf8Offset: 0,
    utf16Offset: 0,
  ),
  next: FlarkV3ViewportPresentationVisitStart(
    blockOrdinal: _u64(10),
    utf8Offset: 10,
    utf16Offset: 8,
  ),
  complete: true,
);
final _envelope = FlarkV3ViewportPresentationEnvelopeMetrics(
  visitedStructuralEntries: 3,
  visitedStoragePages: 2,
  orderedLeafCount: 2,
  inlineSourceBytes: 8,
  factCount: 4,
  transferredNodeCount: 4,
  parserTransitions: 12,
  aggregateEnvelopeDigest256: _digest256(0xa1a1a1a1),
);
final _queryLimits = FlarkV3ViewportPresentationQueryLimits(
  maximumStructuralEntries: 8,
  maximumStoragePages: 8,
  maximumInlineLeaves: 8,
  maximumInlineLeafSourceBytes: 1024,
  maximumInlineSourceBytes: 4096,
  maximumFactRecords: 32,
  maximumEncodedFrameBytes: 4096,
  maximumParserTransitions: 1000,
);
final _offerLimits = FlarkV3ViewportPresentationOfferLimits(
  maximumFrameCount: 16,
  maximumEncodedFrameBytes: 4096,
  maximumPacketBytes: 2048,
  maximumFrameBytes: 1024,
  maximumProgramChildren: 32,
);

FlarkV3ViewportPresentationOfferBegin _begin() =>
    FlarkV3ViewportPresentationOfferBegin(
      offerId: _offer,
      publicationSession: _viewportPublication,
      baseAck: _baseAck,
      binding: _binding,
      envelope: _envelope,
      queryLimits: _queryLimits,
      limits: _offerLimits,
    );
FlarkV3ViewportPresentationCommitRequest _commit() =>
    FlarkV3ViewportPresentationCommitRequest(
      offerId: _offer,
      actualFrameCount: 11,
      actualEncodedFrameBytes: 900,
      rollingTransportDigest: _digest128(140),
      aggregateRootStreamDigest: _digest128(150),
    );
FlarkV3ViewportPresentationAck _ack() => FlarkV3ViewportPresentationAck(
  publicationSession: _viewportPublication,
  baseAck: _baseAck,
  binding: _binding,
  envelope: _envelope,
  actualFrameCount: 11,
  actualEncodedFrameBytes: 900,
  aggregateRootStreamDigest: _digest128(150),
);

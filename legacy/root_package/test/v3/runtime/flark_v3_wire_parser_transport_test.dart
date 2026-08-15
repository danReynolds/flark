import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';
import 'package:flark/src/v3/runtime/flark_v3_byte_endpoint.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_hot_inline_sidecar_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_publication_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_session_wire_codec.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_viewport_presentation_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_parser_transport.dart';
import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:flark/src/v3/source/source.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3WireParserTransport', () {
    test('routes a fresh session open and returns exact event credit', () {
      final harness = _Harness();
      harness.transport.send(
        FlarkV3ParserOpen(
          binding: _binding1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
      );

      final open = FlarkV3ParserSessionWireCodec.decodeCommand(
        harness.endpoint.sent.single,
      );
      expect(open.command, isA<FlarkV3ParserSessionOpenCommand>());
      expect(
        (open.command as FlarkV3ParserSessionOpenCommand).binding,
        _binding1,
      );

      harness.endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionOpenedEvent(
            binding: _binding1,
            eventId: 17,
            mode: FlarkV3ParserOpenMode.fresh,
          ),
          expectedBinding: _binding1,
        ),
      );
      expect(harness.events, hasLength(1));
      expect(harness.events.single, isA<FlarkV3ParserOpened>());

      harness.transport.send(
        FlarkV3ParserEventReceipt(
          eventId: 17,
          binding: _binding1,
          disposition: FlarkV3ParserEventDisposition.accepted,
        ),
      );
      final receipt = FlarkV3ParserSessionWireCodec.decodeCommand(
        harness.endpoint.sent.last,
        establishedBinding: _binding1,
      );
      expect(receipt.correlationId, 17);
      expect(
        receipt.command,
        isA<FlarkV3ParserSessionEventReceiptCommand>().having(
          (value) => value.binding,
          'binding',
          _binding1,
        ),
      );
      expect(harness.failures, isEmpty);
    });

    test('routes publication events onto the global schema-three credit', () {
      final harness = _Harness()..openAndAcknowledge();
      final offerId = FlarkV3OfferId(10, 11, 12, 13);
      harness.endpoint.emit(
        FlarkV3ParserPublicationWireCodec.encodeEvent(
          FlarkV3ParserPublicationAbortRequested(
            eventId: 23,
            binding: _binding1,
            offerId: offerId,
          ),
        ),
      );
      expect(
        harness.events.last,
        isA<FlarkV3ParserPublicationAbortRequested>().having(
          (event) => event.offerId,
          'offerId',
          offerId,
        ),
      );

      harness.transport.send(
        FlarkV3ParserEventReceipt(
          eventId: 23,
          binding: _binding1,
          disposition: FlarkV3ParserEventDisposition.accepted,
        ),
      );
      final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
        harness.endpoint.sent.last,
        establishedBinding: _binding1,
      );
      expect(decoded.correlationId, 23);
      expect(
        decoded.command,
        isA<FlarkV3ParserSessionEventReceiptCommand>().having(
          (receipt) => receipt.disposition,
          'disposition',
          FlarkV3ParserEventDisposition.accepted,
        ),
      );

      final ticket = FlarkV3ParserHostPollTicket(
        binding: _binding1,
        pollTicket: 24,
        offerId: offerId,
        phase: FlarkV3ParserHostPollPhase.abort,
      );
      harness.transport.send(
        FlarkV3ParserHostPollCompleted(
          ticket: ticket,
          outcome: FlarkV3HostAbortComplete(offerId),
        ),
      );
      expect(harness.endpoint.operations.last, 'sendHostPoll');
      expect(
        FlarkV3ParserPublicationWireCodec.decodeCommand(
          harness.endpoint.sent.last,
          expectedBinding: _binding1,
        ).pollTicket,
        ticket,
      );
      expect(harness.failures, isEmpty);
    });

    test(
      'routes refine-inline and sibling sidecar bytes without merging types',
      () {
        final harness = _Harness()..openAndAcknowledge();
        harness.transport.send(
          FlarkV3ParserRefineInline(
            binding: _binding1,
            refinementGeneration: 8,
            sourceVersion: _sourceVersion,
            baseAck: _baseAck,
            byteOffset: 9,
            utf16Offset: 8,
            affinity: FlarkV3InlinePointAffinity.after,
          ),
        );
        final refine = FlarkV3ParserSessionWireCodec.decodeCommand(
          harness.endpoint.sent.last,
          establishedBinding: _binding1,
        );
        expect(refine.correlationId, 8);
        expect(
          refine.command,
          isA<FlarkV3ParserSessionInlineRefinementCommand>(),
        );
        expect(harness.endpoint.operations.last, 'send');

        final offerId = FlarkV3OfferId(61, 62, 63, 64);
        harness.endpoint.emit(
          FlarkV3HotInlineSidecarWireCodec.encodeEvent(
            FlarkV3ParserInlineSidecarAbortRequested(
              eventId: 71,
              binding: _binding1,
              offerId: offerId,
            ),
          ),
        );
        expect(harness.sidecarEvents, hasLength(1));
        expect(
          harness.sidecarEvents.single,
          isA<FlarkV3ParserInlineSidecarAbortRequested>().having(
            (event) => event.offerId,
            'offerId',
            offerId,
          ),
        );

        harness.transport.send(
          FlarkV3ParserEventReceipt(
            eventId: 71,
            binding: _binding1,
            disposition: FlarkV3ParserEventDisposition.accepted,
          ),
        );
        final receipt = FlarkV3ParserSessionWireCodec.decodeCommand(
          harness.endpoint.sent.last,
          establishedBinding: _binding1,
        );
        expect(receipt.correlationId, 71);

        final ticket = FlarkV3ParserInlineSidecarHostPollTicket(
          binding: _binding1,
          pollTicket: 72,
          offerId: offerId,
          phase: FlarkV3ParserInlineSidecarHostPollPhase.abort,
        );
        harness.transport.sendInlineSidecarHostPoll(
          FlarkV3ParserInlineSidecarHostPollCompleted(
            ticket: ticket,
            outcome: FlarkV3InlineSidecarHostAbortComplete(offerId),
          ),
        );
        expect(harness.endpoint.operations.last, 'sendInlineSidecarHostPoll');
        expect(
          FlarkV3HotInlineSidecarWireCodec.decodeCommand(
            harness.endpoint.sent.last,
            expectedBinding: _binding1,
          ).pollTicket,
          ticket,
        );
        expect(harness.failures, isEmpty);
      },
    );

    test('session and sidecar callbacks share exactly one event credit', () {
      final harness = _Harness()..openAndAcknowledge();
      harness.endpoint.emit(
        FlarkV3HotInlineSidecarWireCodec.encodeEvent(
          FlarkV3ParserInlineSidecarAbortRequested(
            eventId: 81,
            binding: _binding1,
            offerId: FlarkV3OfferId(71, 72, 73, 74),
          ),
        ),
      );
      harness.endpoint.emit(
        FlarkV3ParserPublicationWireCodec.encodeEvent(
          FlarkV3ParserPublicationAbortRequested(
            eventId: 82,
            binding: _binding1,
            offerId: FlarkV3OfferId(75, 76, 77, 78),
          ),
        ),
      );

      expect(harness.sidecarEvents, hasLength(1));
      expect(
        harness.events.whereType<FlarkV3ParserPublicationEvent>(),
        isEmpty,
      );
      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
    });

    test(
      'viewport events and host polls use the shared credit and VP lane',
      () {
        final harness = _Harness()..openAndAcknowledge();
        final offerId = FlarkV3OfferId(81, 82, 83, 84);
        harness.endpoint.emit(
          FlarkV3ViewportPresentationWireCodec.encodeEvent(
            FlarkV3ParserViewportPresentationAbortRequested(
              eventId: 91,
              binding: _binding1,
              offerId: offerId,
            ),
          ),
        );
        expect(
          harness.viewportEvents.single,
          isA<FlarkV3ParserViewportPresentationAbortRequested>().having(
            (event) => event.offerId,
            'offerId',
            offerId,
          ),
        );

        harness.transport.send(
          FlarkV3ParserEventReceipt(
            eventId: 91,
            binding: _binding1,
            disposition: FlarkV3ParserEventDisposition.accepted,
          ),
        );
        final ticket = FlarkV3ParserViewportPresentationHostPollTicket(
          binding: _binding1,
          pollTicket: 92,
          offerId: offerId,
          phase: FlarkV3ParserViewportPresentationHostPollPhase.abort,
        );
        harness.transport.sendViewportPresentationHostPoll(
          FlarkV3ParserViewportPresentationHostPollCompleted(
            ticket: ticket,
            outcome: FlarkV3ViewportPresentationHostAbortComplete(offerId),
          ),
        );
        expect(
          harness.endpoint.operations.last,
          'sendViewportPresentationHostPoll',
        );
        expect(
          FlarkV3ViewportPresentationWireCodec.decodeCommand(
            harness.endpoint.sent.last,
            expectedBinding: _binding1,
          ).pollTicket,
          ticket,
        );
        expect(harness.failures, isEmpty);
      },
    );

    test('unknown publication variants fail closed before either callback', () {
      final harness = _Harness()..openAndAcknowledge();
      final frame = FlarkV3HotInlineSidecarWireCodec.encodeEvent(
        FlarkV3ParserInlineSidecarAbortRequested(
          eventId: 91,
          binding: _binding1,
          offerId: FlarkV3OfferId(81, 82, 83, 84),
        ),
      );
      ByteData.sublistView(
        frame,
      ).setUint16(FlarkV3WireProtocol.headerBytes + 2, 0x0200, Endian.little);
      harness.endpoint.emit(frame);

      expect(harness.sidecarEvents, isEmpty);
      expect(
        harness.events.whereType<FlarkV3ParserPublicationEvent>(),
        isEmpty,
      );
      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
    });

    test('validates and drops a delayed retired-generation frame', () {
      final harness = _Harness()..openAndAcknowledge();
      harness.transport.send(
        FlarkV3ParserOpen(
          binding: _binding2,
          mode: FlarkV3ParserOpenMode.recovery,
        ),
      );
      expect(harness.endpoint.recoveries, <FlarkV3ByteEndpointBinding>[
        FlarkV3ByteEndpointBinding(
          documentSessionWords: const <int>[1, 2, 3, 4],
          sourceSessionIdentity: 5,
          workerGeneration: 1,
        ),
      ]);
      expect(
        harness.endpoint.operations.sublist(
          harness.endpoint.operations.length - 2,
        ),
        <String>['recover:1', 'send'],
        reason: 'replacement construction must precede recovery-open bytes',
      );

      final priorEventCount = harness.events.length;
      final priorSendCount = harness.endpoint.sent.length;
      harness.endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionFailedEvent(
            binding: _binding1,
            eventId: 31,
            failureCode: 9,
          ),
          expectedBinding: _binding1,
        ),
      );
      expect(harness.events, hasLength(priorEventCount));
      expect(harness.endpoint.sent, hasLength(priorSendCount));

      final retiredDrainGrant = FlarkV3ParserSessionDrainGrant(
        binding: _binding1,
        drainId: 41,
        maximumTransitions: 1,
      );
      harness.endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionDrainProgressEvent(
            binding: _binding1,
            eventId: 42,
            drainId: retiredDrainGrant.drainId,
            releasedSourceLeases: 0,
            releasedSourceBytes: 0,
            arenaTransitions: 1,
            arenaNodesReclaimed: 0,
            complete: true,
          ),
          expectedBinding: _binding1,
          expectedDrainGrant: retiredDrainGrant,
        ),
      );
      expect(harness.events, hasLength(priorEventCount));
      expect(harness.endpoint.sent, hasLength(priorSendCount));
      expect(harness.transport.binding, _binding2);
      expect(harness.failures, isEmpty);
    });

    test('faults once when the endpoint exceeds its event credit', () {
      final harness = _Harness();
      harness.transport.send(
        FlarkV3ParserOpen(
          binding: _binding1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
      );
      final opened = FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionOpenedEvent(
          binding: _binding1,
          eventId: 41,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
        expectedBinding: _binding1,
      );
      harness.endpoint
        ..emit(opened)
        ..emit(
          FlarkV3ParserSessionWireCodec.encodeEvent(
            FlarkV3ParserSessionFailedEvent(
              binding: _binding1,
              eventId: 42,
              failureCode: 1,
            ),
            expectedBinding: _binding1,
          ),
        )
        ..emit(Uint8List(0));

      expect(harness.events, hasLength(1));
      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
      expect(harness.transport.isFaulted, isTrue);
    });

    test('faults without delivering an invalid or future-bound frame', () {
      final harness = _Harness()..openAndAcknowledge();
      harness.endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionFailedEvent(
            binding: _binding2,
            eventId: 51,
            failureCode: 3,
          ),
          expectedBinding: _binding2,
        ),
      );

      expect(harness.events, hasLength(1), reason: 'only Opened was valid');
      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
    });

    test('rejects a crossed event receipt before bytes leave Dart', () {
      final harness = _Harness();
      harness.transport.send(
        FlarkV3ParserOpen(
          binding: _binding1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
      );
      harness.endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionOpenedEvent(
            binding: _binding1,
            eventId: 61,
            mode: FlarkV3ParserOpenMode.fresh,
          ),
          expectedBinding: _binding1,
        ),
      );
      final sentBefore = harness.endpoint.sent.length;

      expect(
        () => harness.transport.send(
          FlarkV3ParserEventReceipt(
            eventId: 62,
            binding: _binding1,
            disposition: FlarkV3ParserEventDisposition.accepted,
          ),
        ),
        throwsStateError,
      );
      expect(harness.endpoint.sent, hasLength(sentBefore));
      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
    });

    test('malformed frames fail closed and close is idempotent', () {
      final harness = _Harness();
      harness.transport.send(
        FlarkV3ParserOpen(
          binding: _binding1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
      );
      harness.endpoint.emit(Uint8List.fromList(<int>[0x46, 0x4c, 0x4b, 0x33]));
      harness.transport.close();
      harness.transport.close();

      expect(harness.events, isEmpty);
      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
      expect(harness.transport.isFaulted, isTrue);
      expect(harness.transport.isClosed, isTrue);
    });

    test('asynchronous platform failure faults and closes exactly once', () {
      final harness = _Harness();
      harness.transport.send(
        FlarkV3ParserOpen(
          binding: _binding1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
      );

      harness.endpoint.fail(StateError('worker terminated'));
      harness.endpoint.fail(StateError('duplicate exit notification'));

      expect(harness.failures, hasLength(1));
      expect(harness.endpoint.closeCount, 1);
      expect(harness.transport.isFaulted, isTrue);
    });

    test('routes begin-close through the strict platform entrypoint', () {
      final harness = _Harness()..openAndAcknowledge();

      harness.transport.send(FlarkV3ParserBeginClose(1));

      expect(harness.endpoint.operations.last, 'sendClose');
      expect(
        FlarkV3ParserSessionWireCodec.decodeCommand(
          harness.endpoint.sent.last,
          establishedBinding: _binding1,
        ).command,
        isA<FlarkV3ParserSessionBeginCloseCommand>(),
      );
      expect(harness.failures, isEmpty);
    });
  });
}

final _binding1 = FlarkV3ParserSessionBinding(
  documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
  sourceSessionIdentity: 5,
  workerGeneration: 1,
);

final _binding2 = FlarkV3ParserSessionBinding(
  documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
  sourceSessionIdentity: 5,
  workerGeneration: 2,
);

final _sourceVersion = FlarkV3SourceVersion(
  documentSession: _binding1.documentSession,
  revision: 7,
  metric: FlarkV3SourceMetric(bytes: 10, utf16: 9),
  contentHash: FlarkV3ContentHash128(11, 12, 13, 14),
);

final _baseAck = FlarkV3StructuralAck(
  publicationSession: FlarkV3PublicationSessionId(21, 22, 23, 24),
  hostRevision: FlarkV3HostRevisionId(3),
  sourceVersion: _sourceVersion,
  sourceRoot: FlarkV3SourceRootId(31, 32),
  parseGeneration: 4,
  grammarRevision: 5,
  syntaxProfile: FlarkV3SyntaxProfileId(6),
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  recordCount: 17,
  sequenceDigest: FlarkV3ProtocolDigest128(41, 42, 43, 44),
  manifestDigest: FlarkV3ProtocolDigest128(51, 52, 53, 54),
);

final class _Harness {
  _Harness() {
    transport =
        FlarkV3WireParserTransport(
            endpoint: endpoint,
            onFailure: (error, stackTrace) => failures.add(error),
          )
          ..bindInlineSidecar(sidecarEvents.add)
          ..bindViewportPresentation(viewportEvents.add)
          ..bind(events.add);
  }

  final _FakeByteEndpoint endpoint = _FakeByteEndpoint();
  final List<FlarkV3ParserEvent> events = <FlarkV3ParserEvent>[];
  final List<FlarkV3ParserInlineSidecarEvent> sidecarEvents =
      <FlarkV3ParserInlineSidecarEvent>[];
  final List<FlarkV3ParserViewportPresentationEvent> viewportEvents =
      <FlarkV3ParserViewportPresentationEvent>[];
  final List<Object> failures = <Object>[];
  late final FlarkV3WireParserTransport transport;

  void openAndAcknowledge() {
    transport.send(
      FlarkV3ParserOpen(binding: _binding1, mode: FlarkV3ParserOpenMode.fresh),
    );
    endpoint.emit(
      FlarkV3ParserSessionWireCodec.encodeEvent(
        FlarkV3ParserSessionOpenedEvent(
          binding: _binding1,
          eventId: 1,
          mode: FlarkV3ParserOpenMode.fresh,
        ),
        expectedBinding: _binding1,
      ),
    );
    transport.send(
      FlarkV3ParserEventReceipt(
        eventId: 1,
        binding: _binding1,
        disposition: FlarkV3ParserEventDisposition.accepted,
      ),
    );
  }
}

final class _FakeByteEndpoint implements FlarkV3ByteEndpoint {
  FlarkV3ByteFrameCallback? _onFrame;
  FlarkV3ByteEndpointFailureCallback? _onFailure;
  final List<Uint8List> sent = <Uint8List>[];
  final List<FlarkV3ByteEndpointBinding> recoveries =
      <FlarkV3ByteEndpointBinding>[];
  final List<String> operations = <String>[];
  int closeCount = 0;

  @override
  void bind({
    required FlarkV3ByteFrameCallback onFrame,
    required FlarkV3ByteEndpointFailureCallback onFailure,
  }) {
    if (_onFrame != null) throw StateError('already bound');
    _onFrame = onFrame;
    _onFailure = onFailure;
  }

  @override
  void recover(FlarkV3ByteEndpointBinding previousBinding) {
    recoveries.add(previousBinding);
    operations.add('recover:${previousBinding.workerGeneration}');
  }

  @override
  void send(Uint8List frame) {
    sent.add(Uint8List.fromList(frame));
    operations.add('send');
  }

  @override
  void sendHostPoll(Uint8List frame) {
    sent.add(Uint8List.fromList(frame));
    operations.add('sendHostPoll');
  }

  @override
  void sendInlineSidecarHostPoll(Uint8List frame) {
    sent.add(Uint8List.fromList(frame));
    operations.add('sendInlineSidecarHostPoll');
  }

  @override
  void sendViewportPresentationHostPoll(Uint8List frame) {
    sent.add(Uint8List.fromList(frame));
    operations.add('sendViewportPresentationHostPoll');
  }

  @override
  void sendClose(Uint8List frame) {
    sent.add(Uint8List.fromList(frame));
    operations.add('sendClose');
  }

  void emit(Uint8List frame) => _onFrame!(frame);

  void fail(Object error) => _onFailure!(error, StackTrace.current);

  @override
  void close() {
    if (closeCount == 0) closeCount = 1;
  }
}

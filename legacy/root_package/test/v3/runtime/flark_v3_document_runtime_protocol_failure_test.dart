import 'dart:async';
import 'dart:typed_data';

import 'package:flark/flark_adapter.dart';
import 'package:flark/flark_v3.dart';
import 'package:flark/src/v3/runtime/flark_v3_byte_endpoint.dart';
import 'package:flark/src/v3/runtime/flark_v3_parser_session_wire_codec.dart';
import 'package:flark/src/v3/runtime/public/flark_v3_document_runtime.dart'
    show FlarkV3DocumentRuntimePlatformAttachment;
import 'package:flark/src/v3/runtime/public/flark_v3_platform_endpoint_handle.dart';
import 'package:test/test.dart';

void main() {
  test(
    'typed protocol failure settles managed initial readiness causally',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('not ready yet');
      final documentSession = FlarkV3DocumentSessionId(131, 132, 133, 134);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: _FailureHostStore(),
      );
      final binding = FlarkV3ParserSessionBinding(
        documentSession: documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      );
      final endpoint = _ProtocolFailureEndpoint(binding);
      final runtime = await FlarkV3DocumentRuntimePlatformAttachment.attach(
        document: document,
        parserBinding: binding,
        platformEndpoint: FlarkV3PlatformEndpointHandle(
          endpoint: endpoint,
          done: endpoint.done,
        ),
      );

      endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionOpenedEvent(
            binding: binding,
            eventId: 1,
            mode: FlarkV3ParserOpenMode.fresh,
          ),
          expectedBinding: binding,
        ),
      );
      await runtime.statuses
          .firstWhere(
            (status) => status.state == FlarkV3DocumentRuntimeState.open,
          )
          .timeout(const Duration(seconds: 1));
      expect(runtime.status.sourceCurrent, isFalse);

      const failureCode = 4;
      endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionFailedEvent(
            binding: binding,
            eventId: 2,
            failureCode: failureCode,
          ),
          expectedBinding: binding,
        ),
      );

      await expectLater(
        runtime.initialReady.timeout(const Duration(seconds: 1)),
        throwsA(
          isA<FlarkV3RuntimeParserFailure>().having(
            (failure) => failure.recoveryAvailable,
            'recoveryAvailable',
            isTrue,
          ),
        ),
      );
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.faulted);
      expect(runtime.status.recoveryAvailable, isTrue);

      await runtime.close().timeout(const Duration(seconds: 1));
      expect(endpoint.closed, isTrue);
    },
  );

  test(
    'terminal platform failure settles runtime close without a live worker',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('platform failure');
      final documentSession = FlarkV3DocumentSessionId(141, 142, 143, 144);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: _FailureHostStore(),
      );
      final binding = FlarkV3ParserSessionBinding(
        documentSession: documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      );
      final endpoint = _ProtocolFailureEndpoint(binding);
      final runtime = await FlarkV3DocumentRuntimePlatformAttachment.attach(
        document: document,
        parserBinding: binding,
        platformEndpoint: FlarkV3PlatformEndpointHandle(
          endpoint: endpoint,
          done: endpoint.done,
        ),
      );
      final readiness = expectLater(
        runtime.initialReady,
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            'platform endpoint failed',
          ),
        ),
      );

      endpoint.failPlatform(StateError('platform endpoint failed'));

      await readiness;
      await expectLater(
        runtime.close().timeout(const Duration(seconds: 1)),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            'platform endpoint failed',
          ),
        ),
      );
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
      expect(endpoint.closed, isTrue);
    },
  );

  test(
    'async status delivery preserves open fault closing and closed ordering',
    () async {
      final sourceSession = FlarkV3SourceSession.fromString('status ordering');
      final documentSession = FlarkV3DocumentSessionId(151, 152, 153, 154);
      final document = FlarkDocumentSession.attach(
        sourceSession: sourceSession,
        documentSession: documentSession,
        hostStore: _FailureHostStore(),
      );
      final binding = FlarkV3ParserSessionBinding(
        documentSession: documentSession,
        sourceSessionIdentity: sourceSession.sourceSessionIdentity,
        workerGeneration: sourceSession.workerGeneration,
      );
      final endpoint = _ProtocolFailureEndpoint(binding);
      final runtime = await FlarkV3DocumentRuntimePlatformAttachment.attach(
        document: document,
        parserBinding: binding,
        platformEndpoint: FlarkV3PlatformEndpointHandle(
          endpoint: endpoint,
          done: endpoint.done,
        ),
      );

      // Let the attachment notification retain its existing no-replay
      // broadcast behavior before observing the transitions under test.
      await Future<void>.delayed(Duration.zero);
      final observedFuture = runtime.statuses.toList();

      final opened = runtime.statuses.firstWhere(
        (status) => status.state == FlarkV3DocumentRuntimeState.open,
      );
      endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionOpenedEvent(
            binding: binding,
            eventId: 1,
            mode: FlarkV3ParserOpenMode.fresh,
          ),
          expectedBinding: binding,
        ),
      );
      await opened.timeout(const Duration(seconds: 1));

      endpoint.emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionFailedEvent(
            binding: binding,
            eventId: 2,
            failureCode: 4,
          ),
          expectedBinding: binding,
        ),
      );
      await expectLater(
        runtime.initialReady,
        throwsA(isA<FlarkV3RuntimeParserFailure>()),
      );
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.faulted);

      await runtime.close().timeout(const Duration(seconds: 1));
      final observed = await observedFuture.timeout(const Duration(seconds: 1));
      final states = observed.map((status) => status.state).toList();
      final openIndex = states.indexOf(FlarkV3DocumentRuntimeState.open);
      final faultedIndex = states.indexOf(FlarkV3DocumentRuntimeState.faulted);
      final closingIndex = states.indexOf(FlarkV3DocumentRuntimeState.closing);
      final closedIndex = states.indexOf(FlarkV3DocumentRuntimeState.closed);
      expect(openIndex, greaterThanOrEqualTo(0));
      expect(faultedIndex, greaterThan(openIndex));
      expect(closingIndex, greaterThan(faultedIndex));
      expect(closedIndex, greaterThan(closingIndex));
      expect(states.last, FlarkV3DocumentRuntimeState.closed);
      expect(endpoint.closed, isTrue);
    },
  );
}

final class _ProtocolFailureEndpoint implements FlarkV3ByteEndpoint {
  _ProtocolFailureEndpoint(this.binding);

  final FlarkV3ParserSessionBinding binding;
  final Completer<void> _done = Completer<void>();
  FlarkV3ByteFrameCallback? _onFrame;
  FlarkV3ByteEndpointFailureCallback? _onFailure;
  bool _openCommandSeen = false;
  int? _drainProgressEventId;
  bool closed = false;

  Future<void> get done => _done.future;

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
    throw UnsupportedError('recovery is outside this regression');
  }

  @override
  void send(Uint8List frame) {
    final decoded = FlarkV3ParserSessionWireCodec.decodeCommand(
      frame,
      establishedBinding: _openCommandSeen ? binding : null,
    );
    final command = decoded.command;
    if (command is FlarkV3ParserSessionOpenCommand) {
      _openCommandSeen = true;
      return;
    }
    if (command is FlarkV3ParserSessionDrainGrant) {
      const eventId = 3;
      _drainProgressEventId = eventId;
      emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionDrainProgressEvent(
            binding: binding,
            eventId: eventId,
            drainId: command.drainId,
            releasedSourceLeases: 0,
            releasedSourceBytes: 0,
            arenaTransitions: 1,
            arenaNodesReclaimed: 0,
            complete: true,
          ),
          expectedBinding: binding,
          expectedDrainGrant: command,
        ),
      );
      return;
    }
    if (command case FlarkV3ParserSessionEventReceiptCommand(
      :final eventId,
    ) when eventId == _drainProgressEventId) {
      emit(
        FlarkV3ParserSessionWireCodec.encodeEvent(
          FlarkV3ParserSessionClosedEvent(binding: binding, eventId: 4),
          expectedBinding: binding,
        ),
      );
    }
  }

  @override
  void sendHostPoll(Uint8List frame) {
    throw UnsupportedError('publication is outside this regression');
  }

  @override
  void sendInlineSidecarHostPoll(Uint8List frame) {
    throw UnsupportedError('inline sidecar is outside this regression');
  }

  @override
  void sendViewportPresentationHostPoll(Uint8List frame) {
    throw UnsupportedError('viewport presentation is outside this regression');
  }

  @override
  void sendClose(Uint8List frame) {}

  void emit(Uint8List frame) => _onFrame!(frame);

  void failPlatform(Object error) => _onFailure!(error, StackTrace.current);

  @override
  void close() {
    if (closed) return;
    closed = true;
    _done.complete();
  }
}

final class _FailureHostStore implements FlarkV3HostStore {
  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() =>
      const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => const FlarkV3HostAccepted(FlarkV3HostClosed());

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => throw UnsupportedError('No structural publication in this test.');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      throw UnsupportedError('No structural publication in this test.');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => throw UnsupportedError('No structural publication in this test.');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) => throw UnsupportedError('No structural publication in this test.');

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) => throw UnsupportedError('No structural query in this test.');

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => throw UnsupportedError('No structural publication in this test.');
}

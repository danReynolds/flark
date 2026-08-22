import 'dart:typed_data';

import '../host/host.dart';
import 'flark_v3_byte_endpoint.dart';
import 'flark_v3_hot_inline_sidecar_transport.dart';
import 'flark_v3_hot_inline_sidecar_wire_codec.dart';
import 'flark_v3_parser_publication_wire_codec.dart';
import 'flark_v3_parser_session_wire_codec.dart';
import 'flark_v3_parser_transport.dart';
import 'flark_v3_viewport_presentation_transport.dart';
import 'flark_v3_wire_protocol.dart';

typedef FlarkV3WireTransportFailureCallback =
    void Function(Object error, StackTrace stackTrace);

enum _WireLane { session, publication, inlineSidecar, viewportPresentation }

final class _CreditedWireEvent {
  const _CreditedWireEvent({required this.eventId, required this.binding});

  final int eventId;
  final FlarkV3ParserSessionBinding binding;
}

/// Typed parser transport over one platform-owned transferable-byte endpoint.
///
/// This class is deliberately platform neutral. It is the single owner of
/// session/publication wire routing, causal event credit, endpoint binding,
/// and control-frame correlation. Native isolate and Web Worker adapters do
/// not duplicate any of those protocol decisions.
final class FlarkV3WireParserTransport
    implements
        FlarkV3ParserTransport,
        FlarkV3ParserInlineSidecarTransport,
        FlarkV3ParserViewportPresentationTransport {
  FlarkV3WireParserTransport({
    required FlarkV3ByteEndpoint endpoint,
    required FlarkV3WireTransportFailureCallback onFailure,
  }) : _endpoint = endpoint,
       _onFailure = onFailure {
    _endpoint.bind(onFrame: _receiveFrame, onFailure: _receiveEndpointFailure);
  }

  final FlarkV3ByteEndpoint _endpoint;
  final FlarkV3WireTransportFailureCallback _onFailure;

  FlarkV3ParserEventCallback? _onEvent;
  FlarkV3ParserInlineSidecarEventCallback? _onInlineSidecarEvent;
  FlarkV3ParserViewportPresentationEventCallback? _onViewportPresentationEvent;
  FlarkV3ParserSessionBinding? _binding;
  FlarkV3ParserSessionDrainGrant? _expectedDrainGrant;
  _CreditedWireEvent? _creditedEvent;
  int _nextControlCorrelationId = 1;
  bool _closed = false;
  bool _faulted = false;

  bool get isClosed => _closed;
  bool get isFaulted => _faulted;
  FlarkV3ParserSessionBinding? get binding => _binding;

  @override
  void bind(FlarkV3ParserEventCallback onEvent) {
    if (_closed || _faulted) {
      throw StateError('Cannot bind a closed or faulted parser transport.');
    }
    if (_onEvent != null) {
      throw StateError('A parser event callback is already bound.');
    }
    _onEvent = onEvent;
  }

  @override
  void bindInlineSidecar(FlarkV3ParserInlineSidecarEventCallback onEvent) {
    if (_closed || _faulted) {
      throw StateError('Cannot bind a closed or faulted parser transport.');
    }
    if (_onInlineSidecarEvent != null) {
      throw StateError('An inline-sidecar event callback is already bound.');
    }
    _onInlineSidecarEvent = onEvent;
  }

  @override
  void bindViewportPresentation(
    FlarkV3ParserViewportPresentationEventCallback onEvent,
  ) {
    if (_closed || _faulted) {
      throw StateError('Cannot bind a closed or faulted parser transport.');
    }
    if (_onViewportPresentationEvent != null) {
      throw StateError(
        'A viewport-presentation event callback is already bound.',
      );
    }
    _onViewportPresentationEvent = onEvent;
  }

  @override
  void send(FlarkV3ParserCommand command) {
    if (_closed || _faulted) {
      throw StateError('Cannot send through a closed or faulted transport.');
    }
    if (_onEvent == null) {
      throw StateError('Bind the parser event callback before sending.');
    }

    try {
      final frame = _encodeCommand(command);
      if (command is FlarkV3ParserBeginClose) {
        _endpoint.sendClose(frame);
      } else if (command is FlarkV3ParserHostPollCompleted ||
          command is FlarkV3ParserHostPollRejected) {
        _endpoint.sendHostPoll(frame);
      } else {
        _endpoint.send(frame);
      }
    } catch (error, stackTrace) {
      _fail(error, stackTrace);
      rethrow;
    }
  }

  @override
  void sendInlineSidecarHostPoll(
    FlarkV3ParserInlineSidecarHostPollCommand command,
  ) {
    if (_closed || _faulted) {
      throw StateError('Cannot send through a closed or faulted transport.');
    }
    if (_onEvent == null || _onInlineSidecarEvent == null) {
      throw StateError(
        'Bind both parser and inline-sidecar callbacks before sending.',
      );
    }
    try {
      final current = _binding;
      if (current == null) {
        throw StateError('The parser endpoint has not been opened.');
      }
      final ticket = switch (command) {
        FlarkV3ParserInlineSidecarHostPollCompleted(:final ticket) => ticket,
        FlarkV3ParserInlineSidecarHostPollRejected(:final ticket) => ticket,
      };
      if (ticket.binding != current) {
        throw StateError(
          'Inline-sidecar host-poll result crossed its current endpoint.',
        );
      }
      _endpoint.sendInlineSidecarHostPoll(
        FlarkV3HotInlineSidecarWireCodec.encodeCommand(command),
      );
    } catch (error, stackTrace) {
      _fail(error, stackTrace);
      rethrow;
    }
  }

  @override
  void sendViewportPresentationHostPoll(
    FlarkV3ParserViewportPresentationHostPollCommand command,
  ) {
    if (_closed || _faulted) {
      throw StateError('Cannot send through a closed or faulted transport.');
    }
    if (_onEvent == null || _onViewportPresentationEvent == null) {
      throw StateError(
        'Bind both parser and viewport callbacks before sending.',
      );
    }
    try {
      final current = _binding;
      if (current == null) {
        throw StateError('The parser endpoint has not been opened.');
      }
      final ticket = switch (command) {
        FlarkV3ParserViewportPresentationHostPollCompleted(:final ticket) =>
          ticket,
        FlarkV3ParserViewportPresentationHostPollRejected(:final ticket) =>
          ticket,
      };
      if (ticket.binding != current) {
        throw StateError(
          'Viewport host-poll result crossed its current endpoint.',
        );
      }
      _endpoint.sendViewportPresentationHostPoll(
        FlarkV3ViewportPresentationWireCodec.encodeCommand(command),
      );
    } catch (error, stackTrace) {
      _fail(error, stackTrace);
      rethrow;
    }
  }

  Uint8List _encodeCommand(FlarkV3ParserCommand command) {
    if (command case final FlarkV3ParserOpen open) {
      final previous = _validateOpen(open);
      final frame = FlarkV3ParserSessionWireCodec.encodeParserCommand(
        command,
        binding: open.binding,
        correlationId: _takeControlCorrelationId(),
      );
      if (open.mode == FlarkV3ParserOpenMode.recovery) {
        _endpoint.recover(_byteBinding(previous!));
      }
      _binding = open.binding;
      _expectedDrainGrant = null;
      return frame;
    }

    final current = _binding;
    if (current == null) {
      throw StateError('The parser endpoint has not been opened.');
    }

    if (command case final FlarkV3ParserEventReceipt receipt) {
      return _encodeEventReceipt(receipt);
    }
    if (command is FlarkV3ParserHostPollCompleted ||
        command is FlarkV3ParserHostPollRejected) {
      final ticketBinding = switch (command) {
        FlarkV3ParserHostPollCompleted(:final binding) => binding,
        FlarkV3ParserHostPollRejected(:final binding) => binding,
        _ => throw StateError('Publication command routing diverged.'),
      };
      if (ticketBinding != current) {
        throw StateError('Host-poll result crossed its current endpoint.');
      }
      return FlarkV3ParserPublicationWireCodec.encodeCommand(command);
    }

    final correlationId = switch (command) {
      FlarkV3ParserSynchronizeSource(:final lease) => lease.leaseId,
      FlarkV3ParserDrainGrant(:final drainId) => drainId,
      FlarkV3ParserRefineInline(:final refinementGeneration) =>
        refinementGeneration,
      FlarkV3ParserPresentViewport(:final viewportGeneration) =>
        viewportGeneration,
      FlarkV3ParserRestart() ||
      FlarkV3ParserSupersede() ||
      FlarkV3ParserBeginClose() => _takeControlCorrelationId(),
      _ => throw ArgumentError.value(
        command,
        'command',
        'Unsupported parser command for the v3 session wire.',
      ),
    };
    final commandBinding = switch (command) {
      FlarkV3ParserRestart(:final workerGeneration) => current.nextGeneration(
        workerGeneration,
      ),
      _ => current,
    };
    if (command is FlarkV3ParserRestart) {
      if (_creditedEvent != null ||
          commandBinding.workerGeneration != current.workerGeneration + 1) {
        throw StateError(
          'Restart must advance one quiet parser endpoint generation.',
        );
      }
      _binding = commandBinding;
      _expectedDrainGrant = null;
    }
    if (command case final FlarkV3ParserDrainGrant grant) {
      _expectedDrainGrant = FlarkV3ParserSessionDrainGrant(
        binding: grant.binding,
        drainId: grant.drainId,
        maximumTransitions: grant.maximumTransitions,
      );
    }
    return FlarkV3ParserSessionWireCodec.encodeParserCommand(
      command,
      binding: commandBinding,
      correlationId: correlationId,
    );
  }

  Uint8List _encodeEventReceipt(FlarkV3ParserEventReceipt receipt) {
    final credited = _creditedEvent;
    if (credited == null) {
      throw StateError('No parser event credit is outstanding.');
    }
    final binding = receipt.binding;
    if (binding == null ||
        binding != credited.binding ||
        receipt.eventId != credited.eventId ||
        receipt.workerGeneration != credited.binding.workerGeneration) {
      throw StateError('Parser event receipt crossed its causal wire event.');
    }
    // Event credit is global to the parser endpoint. Publication events and
    // session events therefore share the one canonical session receipt lane.
    final frame = FlarkV3ParserSessionWireCodec.encodeParserCommand(
      receipt,
      binding: binding,
      correlationId: receipt.eventId,
    );
    _creditedEvent = null;
    return frame;
  }

  FlarkV3ParserSessionBinding? _validateOpen(FlarkV3ParserOpen open) {
    final current = _binding;
    switch (open.mode) {
      case FlarkV3ParserOpenMode.fresh:
        if (current != null) {
          throw StateError('A fresh parser binding is already established.');
        }
      case FlarkV3ParserOpenMode.recovery:
        if (current == null ||
            open.binding.documentSession != current.documentSession ||
            open.binding.sourceSessionIdentity !=
                current.sourceSessionIdentity ||
            open.binding.workerGeneration != current.workerGeneration + 1) {
          throw StateError(
            'Recovery must advance exactly one established worker generation.',
          );
        }
    }
    if (_creditedEvent != null) {
      throw StateError('Cannot open a generation with event credit in flight.');
    }
    return current;
  }

  static FlarkV3ByteEndpointBinding _byteBinding(
    FlarkV3ParserSessionBinding binding,
  ) => FlarkV3ByteEndpointBinding(
    documentSessionWords: <int>[
      binding.documentSession.word0,
      binding.documentSession.word1,
      binding.documentSession.word2,
      binding.documentSession.word3,
    ],
    sourceSessionIdentity: binding.sourceSessionIdentity,
    workerGeneration: binding.workerGeneration,
  );

  void _receiveFrame(Uint8List bytes) {
    if (_closed || _faulted) return;
    try {
      final current = _binding;
      final callback = _onEvent;
      if (current == null || callback == null) {
        throw StateError('Parser endpoint emitted before open and bind.');
      }
      if (_creditedEvent != null) {
        throw StateError('Parser endpoint exceeded its single event credit.');
      }

      final envelope = FlarkV3WireProtocol.decode(
        bytes,
        kind: FlarkV3WireFrameKind.request,
      );
      final eventBinding = _readEventBinding(envelope.payload);
      if (eventBinding.documentSession != current.documentSession ||
          eventBinding.sourceSessionIdentity != current.sourceSessionIdentity ||
          eventBinding.workerGeneration > current.workerGeneration) {
        throw StateError('Parser event crossed its established endpoint.');
      }

      final lane = _laneForEvent(envelope);
      final retiredGeneration =
          eventBinding.workerGeneration < current.workerGeneration;
      if (retiredGeneration) {
        switch (lane) {
          case _WireLane.publication:
            FlarkV3ParserPublicationWireCodec.decodeEvent(
              bytes,
              expectedBinding: eventBinding,
            );
          case _WireLane.inlineSidecar:
            FlarkV3HotInlineSidecarWireCodec.decodeEvent(
              bytes,
              expectedBinding: eventBinding,
            );
          case _WireLane.viewportPresentation:
            FlarkV3ViewportPresentationWireCodec.decodeEvent(
              bytes,
              expectedBinding: eventBinding,
            );
          case _WireLane.session:
            FlarkV3ParserSessionWireCodec.decodeEvent(
              bytes,
              expectedBinding: eventBinding,
              requireDrainGrant: false,
            );
        }
        // Recovery atomically replaces and revokes the prior native handle.
        // A frame already queued from that retired generation can therefore
        // be validated, but cannot truthfully receive causal credit.
        return;
      }
      switch (lane) {
        case _WireLane.inlineSidecar:
          final sidecarCallback = _onInlineSidecarEvent;
          if (sidecarCallback == null) {
            throw StateError(
              'Parser emitted a hot-inline sidecar event without a bound '
              'sidecar owner.',
            );
          }
          final event = FlarkV3HotInlineSidecarWireCodec.decodeEvent(
            bytes,
            expectedBinding: eventBinding,
          );
          // The same cell gates both callbacks. Whichever lane receives the
          // frame owns the sole endpoint credit until one exact receipt.
          _creditedEvent = _CreditedWireEvent(
            eventId: event.eventId,
            binding: eventBinding,
          );
          sidecarCallback(event);
        case _WireLane.viewportPresentation:
          final viewportCallback = _onViewportPresentationEvent;
          if (viewportCallback == null) {
            throw StateError(
              'Parser emitted a viewport event without a bound viewport owner.',
            );
          }
          final event = FlarkV3ViewportPresentationWireCodec.decodeEvent(
            bytes,
            expectedBinding: eventBinding,
          );
          _creditedEvent = _CreditedWireEvent(
            eventId: event.eventId,
            binding: eventBinding,
          );
          viewportCallback(event);
        case _WireLane.publication:
          final event = FlarkV3ParserPublicationWireCodec.decodeEvent(
            bytes,
            expectedBinding: eventBinding,
          );
          _creditedEvent = _CreditedWireEvent(
            eventId: event.eventId,
            binding: eventBinding,
          );
          callback(event);
        case _WireLane.session:
          final event = _decodeSessionEvent(bytes, binding: eventBinding);
          _creditedEvent = _CreditedWireEvent(
            eventId: event.eventId,
            binding: eventBinding,
          );
          callback(event);
      }
    } catch (error, stackTrace) {
      _fail(error, stackTrace);
    }
  }

  void _receiveEndpointFailure(Object error, StackTrace stackTrace) {
    _fail(error, stackTrace);
  }

  FlarkV3ParserEvent _decodeSessionEvent(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding binding,
  }) {
    final event = FlarkV3ParserSessionWireCodec.decodeEvent(
      bytes,
      expectedBinding: binding,
      expectedDrainGrant: _expectedDrainGrant,
    );
    if (event is FlarkV3ParserSessionDrainProgressEvent) {
      _expectedDrainGrant = null;
    }
    return switch (event) {
      FlarkV3ParserSessionOpenedEvent() => event.toParserEvent(),
      FlarkV3ParserSessionSourceSynchronizedEvent() => event.toParserEvent(),
      FlarkV3ParserSessionSourceFactsPageEvent() => event.toParserEvent(),
      FlarkV3ParserSessionSourceFactsCompletedEvent() => event.toParserEvent(),
      FlarkV3ParserSessionSourceFactsDeltaBeginEvent() => event.toParserEvent(),
      FlarkV3ParserSessionSourceFactsDeltaPageEvent() => event.toParserEvent(),
      FlarkV3ParserSessionSourceFactsDeltaCompletedEvent() =>
        event.toParserEvent(),
      FlarkV3ParserSessionInlineRefinementUnavailableEvent() =>
        event.toParserEvent(),
      FlarkV3ParserSessionViewportPresentationUnavailableEvent() =>
        event.toParserEvent(),
      FlarkV3ParserSessionDrainProgressEvent() => event.toParserEvent(),
      FlarkV3ParserSessionFailedEvent() => event.toParserEvent(),
      FlarkV3ParserSessionClosedEvent() => event.toParserEvent(),
    };
  }

  static _WireLane _laneForEvent(FlarkV3WireFrame envelope) =>
      switch (envelope.opcode) {
        FlarkV3WireOpcode.publishBegin ||
        FlarkV3WireOpcode.publishPacket ||
        FlarkV3WireOpcode.publishCommit ||
        FlarkV3WireOpcode.publishAbort ||
        FlarkV3WireOpcode.acknowledgeDelivery => _publicationLaneForPayload(
          envelope.payload,
        ),
        FlarkV3WireOpcode.parserOpen ||
        FlarkV3WireOpcode.snapshotPage ||
        FlarkV3WireOpcode.edit ||
        FlarkV3WireOpcode.parserPoll ||
        FlarkV3WireOpcode.drain ||
        FlarkV3WireOpcode.close => _WireLane.session,
        _ => throw StateError(
          'Unexpected parser event opcode: ${envelope.opcode}.',
        ),
      };

  static _WireLane _publicationLaneForPayload(Uint8List payload) {
    if (payload.length < 4) {
      throw const FormatException('Truncated parser publication event header.');
    }
    final header = ByteData.sublistView(payload, 0, 4);
    final schema = header.getUint16(0, Endian.little);
    final variant = header.getUint16(2, Endian.little);
    if (schema != FlarkV3ParserPublicationWireCodec.payloadSchema) {
      throw FormatException(
        'Unexpected parser publication payload schema $schema.',
      );
    }
    return switch (variant) {
      0 || 1 => _WireLane.publication,
      0x0100 || 0x0101 => _WireLane.inlineSidecar,
      0x0200 || 0x0201 => _WireLane.viewportPresentation,
      _ => throw FormatException(
        'Unknown parser publication event variant $variant.',
      ),
    };
  }

  static FlarkV3ParserSessionBinding _readEventBinding(Uint8List payload) {
    const bindingBytes = 28;
    if (payload.length < bindingBytes) {
      throw const FormatException('Truncated parser event binding.');
    }
    final data = ByteData.sublistView(payload, 0, bindingBytes);
    final generation = data.getUint32(4, Endian.little);
    final document = FlarkV3DocumentSessionId(
      data.getUint32(8, Endian.little),
      data.getUint32(12, Endian.little),
      data.getUint32(16, Endian.little),
      data.getUint32(20, Endian.little),
    );
    final sourceSessionIdentity = data.getUint32(24, Endian.little);
    return FlarkV3ParserSessionBinding(
      documentSession: document,
      sourceSessionIdentity: sourceSessionIdentity,
      workerGeneration: generation,
    );
  }

  int _takeControlCorrelationId() {
    final value = _nextControlCorrelationId;
    if (value > flarkV3TransportV1Maximum) {
      throw StateError('Parser control correlation lane exhausted.');
    }
    _nextControlCorrelationId += 1;
    return value;
  }

  void _fail(Object error, StackTrace stackTrace) {
    if (_faulted || _closed) return;
    _faulted = true;
    _endpoint.close();
    _onFailure(error, stackTrace);
  }

  @override
  void close() {
    if (_closed) return;
    _closed = true;
    _creditedEvent = null;
    _expectedDrainGrant = null;
    _endpoint.close();
  }
}

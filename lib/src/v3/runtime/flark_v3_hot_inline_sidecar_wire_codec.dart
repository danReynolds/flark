import 'dart:typed_data';

import '../host/host.dart';
import '../source/source.dart';
import 'flark_v3_hot_inline_sidecar_transport.dart';
import 'flark_v3_parser_transport.dart';
import 'flark_v3_viewport_presentation_transport.dart';
import 'flark_v3_wire_protocol.dart';

/// Binary codec for the sibling hot-inline sidecar publication protocol.
///
/// It shares FLK3 framing and the FPK3 packet body with structural
/// publication, but every event, ticket phase, outcome, rejection, and ACK is
/// tagged in a disjoint sidecar namespace.
final class FlarkV3HotInlineSidecarWireCodec {
  const FlarkV3HotInlineSidecarWireCodec._();

  static const int payloadSchema = 4;
  static const int payloadPrefixBytes = 28;
  static const int pollTicketBytes = 24;
  static const int structuralAckBytes = 124;
  static const int beginBytes = 364;
  static const int inlineSidecarAckBytes = 212;

  static const int _sidecarVariant = 0x0100;
  static const int _failedVariant = 0x0101;
  static const int _packetCreditVariant = 0x0110;
  static const int _committedVariant = 0x0111;
  static const int _abortCompleteVariant = 0x0112;

  static Uint8List encodeEvent(FlarkV3ParserInlineSidecarEvent event) {
    _requireCanonicalBinding(event.binding);
    switch (event) {
      case FlarkV3ParserInlineSidecarBegin(:final begin):
        if (begin.baseAck.sourceVersion.documentSession !=
            event.binding.documentSession) {
          throw ArgumentError(
            'Sidecar base document does not match its endpoint binding.',
          );
        }
      case FlarkV3ParserInlineSidecarDeliveryAcknowledged(:final ack):
        if (ack.baseAck.sourceVersion.documentSession !=
            event.binding.documentSession) {
          throw ArgumentError(
            'Sidecar ACK document does not match its endpoint binding.',
          );
        }
      default:
        break;
    }

    final opcode = switch (event) {
      FlarkV3ParserInlineSidecarBegin() => FlarkV3WireOpcode.publishBegin,
      FlarkV3ParserInlineSidecarPacket() => FlarkV3WireOpcode.publishPacket,
      FlarkV3ParserInlineSidecarCommitRequested() =>
        FlarkV3WireOpcode.publishCommit,
      FlarkV3ParserInlineSidecarAbortRequested() ||
      FlarkV3ParserInlineSidecarFailed() => FlarkV3WireOpcode.publishAbort,
      FlarkV3ParserInlineSidecarDeliveryAcknowledged() =>
        FlarkV3WireOpcode.acknowledgeDelivery,
    };
    final variant = event is FlarkV3ParserInlineSidecarFailed
        ? _failedVariant
        : _sidecarVariant;
    final bodyBytes = switch (event) {
      FlarkV3ParserInlineSidecarBegin() => beginBytes,
      FlarkV3ParserInlineSidecarPacket(:final packet) => packet.rawBytes.length,
      FlarkV3ParserInlineSidecarCommitRequested() => 56,
      FlarkV3ParserInlineSidecarAbortRequested() => 16,
      FlarkV3ParserInlineSidecarFailed() => 20,
      FlarkV3ParserInlineSidecarDeliveryAcknowledged() => inlineSidecarAckBytes,
    };
    final writer = _payloadWriter(bodyBytes);
    _writeHeader(writer, variant, event.binding);
    switch (event) {
      case FlarkV3ParserInlineSidecarBegin(:final begin):
        _writeBegin(writer, begin);
      case FlarkV3ParserInlineSidecarPacket(:final packet):
        writer.raw(packet.rawBytes);
      case FlarkV3ParserInlineSidecarCommitRequested(:final request):
        _writeCommit(writer, request);
      case FlarkV3ParserInlineSidecarAbortRequested(:final offerId):
        writer.id128(offerId);
      case FlarkV3ParserInlineSidecarFailed(:final offerId, :final failureCode):
        writer
          ..id128(offerId)
          ..u32(failureCode);
      case FlarkV3ParserInlineSidecarDeliveryAcknowledged(:final ack):
        _writeInlineSidecarAck(writer, ack);
    }
    writer.finish();
    return FlarkV3WireProtocol.encode(
      FlarkV3WireFrame.owned(
        kind: FlarkV3WireFrameKind.request,
        opcode: opcode,
        correlationId: event.eventId,
        payload: writer.bytes,
      ),
    );
  }

  static FlarkV3ParserInlineSidecarEvent decodeEvent(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding expectedBinding,
  }) {
    _requireCanonicalBinding(expectedBinding);
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.request,
    );
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    if (frame.correlationId == 0) {
      throw const FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: 16,
        expected: 1,
        actual: 0,
      );
    }
    _requireDecodedBinding(header.binding, expectedBinding, reader.offset);
    try {
      final event = switch ((frame.opcode, header.variant)) {
        (FlarkV3WireOpcode.publishBegin, _sidecarVariant) =>
          FlarkV3ParserInlineSidecarBegin(
            eventId: frame.correlationId,
            binding: header.binding,
            begin: _readBegin(reader),
          ),
        (FlarkV3WireOpcode.publishPacket, _sidecarVariant) =>
          FlarkV3ParserInlineSidecarPacket(
            eventId: frame.correlationId,
            binding: header.binding,
            packet: FlarkV3HostPublicationPacket.fromOwnedBytes(
              reader.remainder(),
            ),
          ),
        (FlarkV3WireOpcode.publishCommit, _sidecarVariant) =>
          FlarkV3ParserInlineSidecarCommitRequested(
            eventId: frame.correlationId,
            binding: header.binding,
            request: _readCommit(reader),
          ),
        (FlarkV3WireOpcode.publishAbort, _sidecarVariant) =>
          FlarkV3ParserInlineSidecarAbortRequested(
            eventId: frame.correlationId,
            binding: header.binding,
            offerId: reader.offerId(),
          ),
        (FlarkV3WireOpcode.publishAbort, _failedVariant) =>
          FlarkV3ParserInlineSidecarFailed(
            eventId: frame.correlationId,
            binding: header.binding,
            offerId: reader.offerId(),
            failureCode: reader.u32(),
          ),
        (FlarkV3WireOpcode.acknowledgeDelivery, _sidecarVariant) =>
          FlarkV3ParserInlineSidecarDeliveryAcknowledged(
            eventId: frame.correlationId,
            binding: header.binding,
            ack: _readInlineSidecarAck(reader),
          ),
        (
          FlarkV3WireOpcode.publishBegin ||
              FlarkV3WireOpcode.publishPacket ||
              FlarkV3WireOpcode.publishCommit ||
              FlarkV3WireOpcode.publishAbort ||
              FlarkV3WireOpcode.acknowledgeDelivery,
          _,
        ) =>
          throw FlarkV3HotInlineSidecarWireFormatException(
            FlarkV3HotInlineSidecarWireFailure.unknownVariant,
            byteOffset: 2,
            actual: header.variant,
          ),
        _ => throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.unexpectedOpcode,
          byteOffset: 8,
          actual: frame.opcode.code,
        ),
      };
      final documentSession = switch (event) {
        FlarkV3ParserInlineSidecarBegin(:final begin) =>
          begin.baseAck.sourceVersion.documentSession,
        FlarkV3ParserInlineSidecarDeliveryAcknowledged(:final ack) =>
          ack.baseAck.sourceVersion.documentSession,
        _ => null,
      };
      if (documentSession != null &&
          documentSession != event.binding.documentSession) {
        throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.identityMismatch,
          byteOffset: reader.offset,
        );
      }
      reader.finish();
      return event;
    } on FlarkV3HotInlineSidecarWireFormatException {
      rethrow;
    } on ArgumentError {
      throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }

  /// Encodes one terminal sidecar host-poll result.
  static Uint8List encodeCommand(
    FlarkV3ParserInlineSidecarHostPollCommand command,
  ) {
    final (ticket, outcome, rejection) = switch (command) {
      FlarkV3ParserInlineSidecarHostPollCompleted(
        :final ticket,
        :final outcome,
      ) =>
        (ticket, outcome, null),
      FlarkV3ParserInlineSidecarHostPollRejected(
        :final ticket,
        :final reason,
      ) =>
        (ticket, null, reason),
    };
    _requireCanonicalBinding(ticket.binding);
    if (outcome case FlarkV3InlineSidecarHostCommitted(:final ack)
        when ack.baseAck.sourceVersion.documentSession !=
            ticket.binding.documentSession) {
      throw ArgumentError(
        'Committed sidecar ACK document does not match its binding.',
      );
    }
    final status = rejection == null
        ? FlarkV3WireStatus.ok
        : _statusForReject(rejection);
    final variant = switch (outcome) {
      FlarkV3InlineSidecarHostPacketCredit() => _packetCreditVariant,
      FlarkV3InlineSidecarHostCommitted() => _committedVariant,
      FlarkV3InlineSidecarHostAbortComplete() => _abortCompleteVariant,
      FlarkV3InlineSidecarHostPollPending() ||
      FlarkV3InlineSidecarHostClosed() => throw StateError(
        'Only terminal sidecar publication outcomes cross the parser wire.',
      ),
      null => _sidecarVariant,
    };
    final outcomeBytes = switch (outcome) {
      FlarkV3InlineSidecarHostPacketCredit() => 20,
      FlarkV3InlineSidecarHostCommitted() => inlineSidecarAckBytes,
      FlarkV3InlineSidecarHostAbortComplete() => 16,
      FlarkV3InlineSidecarHostPollPending() ||
      FlarkV3InlineSidecarHostClosed() => 0,
      null => 0,
    };
    final writer = _payloadWriter(pollTicketBytes + outcomeBytes);
    _writeHeader(writer, variant, ticket.binding);
    _writePollTicket(writer, ticket);
    switch (outcome) {
      case FlarkV3InlineSidecarHostPacketCredit(
        :final offerId,
        :final nextFrameOrdinal,
      ):
        writer
          ..id128(offerId)
          ..u32(nextFrameOrdinal);
      case FlarkV3InlineSidecarHostCommitted(:final ack):
        _writeInlineSidecarAck(writer, ack);
      case FlarkV3InlineSidecarHostAbortComplete(:final offerId):
        writer.id128(offerId);
      case FlarkV3InlineSidecarHostPollPending() ||
          FlarkV3InlineSidecarHostClosed():
        throw StateError(
          'Nonterminal sidecar host outcome reached the wire writer.',
        );
      case null:
        break;
    }
    writer.finish();
    return FlarkV3WireProtocol.encode(
      FlarkV3WireFrame.owned(
        kind: FlarkV3WireFrameKind.response,
        opcode: FlarkV3WireOpcode.hostPoll,
        status: status,
        correlationId: ticket.pollTicket,
        payload: writer.bytes,
      ),
    );
  }

  static FlarkV3DecodedInlineSidecarHostPollCommand decodeCommand(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding expectedBinding,
  }) {
    _requireCanonicalBinding(expectedBinding);
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.response,
    );
    if (frame.correlationId == 0) {
      throw const FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: 16,
        expected: 1,
        actual: 0,
      );
    }
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    _requireDecodedBinding(header.binding, expectedBinding, reader.offset);
    try {
      if (frame.opcode != FlarkV3WireOpcode.hostPoll) {
        throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.unexpectedOpcode,
          byteOffset: 8,
          expected: FlarkV3WireOpcode.hostPoll.code,
          actual: frame.opcode.code,
        );
      }
      final ticket = _readPollTicket(reader, header.binding);
      if (ticket.pollTicket != frame.correlationId) {
        throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.correlationMismatch,
          byteOffset: payloadPrefixBytes,
          expected: ticket.pollTicket,
          actual: frame.correlationId,
        );
      }

      final FlarkV3ParserInlineSidecarHostPollCommand command;
      if (frame.status != FlarkV3WireStatus.ok) {
        if (header.variant != _sidecarVariant) {
          throw FlarkV3HotInlineSidecarWireFormatException(
            FlarkV3HotInlineSidecarWireFailure.unknownVariant,
            byteOffset: 2,
            actual: header.variant,
          );
        }
        command = FlarkV3ParserInlineSidecarHostPollRejected(
          ticket: ticket,
          reason: _rejectForStatus(frame.status),
        );
      } else {
        final outcome = switch (header.variant) {
          _packetCreditVariant => FlarkV3InlineSidecarHostPacketCredit(
            offerId: reader.offerId(),
            nextFrameOrdinal: reader.u32(),
          ),
          _committedVariant => FlarkV3InlineSidecarHostCommitted(
            _readInlineSidecarAck(reader),
          ),
          _abortCompleteVariant => FlarkV3InlineSidecarHostAbortComplete(
            reader.offerId(),
          ),
          _ => throw FlarkV3HotInlineSidecarWireFormatException(
            FlarkV3HotInlineSidecarWireFailure.unknownVariant,
            byteOffset: 2,
            actual: header.variant,
          ),
        };
        _validatePollOutcome(ticket, outcome, reader.offset);
        command = FlarkV3ParserInlineSidecarHostPollCompleted(
          ticket: ticket,
          outcome: outcome,
        );
      }
      reader.finish();
      return FlarkV3DecodedInlineSidecarHostPollCommand(
        correlationId: frame.correlationId,
        binding: header.binding,
        command: command,
      );
    } on FlarkV3HotInlineSidecarWireFormatException {
      rethrow;
    } on ArgumentError {
      throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }
}

/// Binary codec for aggregate VPB1 publication.
///
/// VPB1 shares only the FLK3 envelope and FPK3 packet body with structural and
/// HIO1 publication. Every event variant, poll phase, successful outcome, and
/// ACK shape is disjoint.
final class FlarkV3ViewportPresentationWireCodec {
  const FlarkV3ViewportPresentationWireCodec._();

  static const int payloadSchema =
      FlarkV3HotInlineSidecarWireCodec.payloadSchema;
  static const int payloadPrefixBytes =
      FlarkV3HotInlineSidecarWireCodec.payloadPrefixBytes;
  static const int pollTicketBytes =
      FlarkV3HotInlineSidecarWireCodec.pollTicketBytes;
  static const int beginBytes = 348;
  static const int ackBytes = 296;

  static const int _viewportVariant = 0x0200;
  static const int _failedVariant = 0x0201;
  static const int _packetCreditVariant = 0x0210;
  static const int _committedVariant = 0x0211;
  static const int _abortCompleteVariant = 0x0212;

  static Uint8List encodeEvent(FlarkV3ParserViewportPresentationEvent event) {
    _requireCanonicalBinding(event.binding);
    switch (event) {
      case FlarkV3ParserViewportPresentationBegin(:final begin):
        if (begin.baseAck.sourceVersion.documentSession !=
            event.binding.documentSession) {
          throw ArgumentError(
            'Viewport base document does not match its endpoint binding.',
          );
        }
      case FlarkV3ParserViewportPresentationDeliveryAcknowledged(:final ack):
        if (ack.baseAck.sourceVersion.documentSession !=
            event.binding.documentSession) {
          throw ArgumentError(
            'Viewport ACK document does not match its endpoint binding.',
          );
        }
      default:
        break;
    }

    final opcode = switch (event) {
      FlarkV3ParserViewportPresentationBegin() =>
        FlarkV3WireOpcode.publishBegin,
      FlarkV3ParserViewportPresentationPacket() =>
        FlarkV3WireOpcode.publishPacket,
      FlarkV3ParserViewportPresentationCommitRequested() =>
        FlarkV3WireOpcode.publishCommit,
      FlarkV3ParserViewportPresentationAbortRequested() ||
      FlarkV3ParserViewportPresentationFailed() =>
        FlarkV3WireOpcode.publishAbort,
      FlarkV3ParserViewportPresentationDeliveryAcknowledged() =>
        FlarkV3WireOpcode.acknowledgeDelivery,
    };
    final variant = event is FlarkV3ParserViewportPresentationFailed
        ? _failedVariant
        : _viewportVariant;
    final bodyBytes = switch (event) {
      FlarkV3ParserViewportPresentationBegin() => beginBytes,
      FlarkV3ParserViewportPresentationPacket(:final packet) =>
        packet.rawBytes.length,
      FlarkV3ParserViewportPresentationCommitRequested() => 56,
      FlarkV3ParserViewportPresentationAbortRequested() => 16,
      FlarkV3ParserViewportPresentationFailed() => 20,
      FlarkV3ParserViewportPresentationDeliveryAcknowledged() => ackBytes,
    };
    final writer = _payloadWriter(bodyBytes);
    _writeHeader(writer, variant, event.binding);
    switch (event) {
      case FlarkV3ParserViewportPresentationBegin(:final begin):
        _writeViewportBegin(writer, begin);
      case FlarkV3ParserViewportPresentationPacket(:final packet):
        writer.raw(packet.rawBytes);
      case FlarkV3ParserViewportPresentationCommitRequested(:final request):
        _writeViewportCommit(writer, request);
      case FlarkV3ParserViewportPresentationAbortRequested(:final offerId):
        writer.id128(offerId);
      case FlarkV3ParserViewportPresentationFailed(
        :final offerId,
        :final failureCode,
      ):
        writer
          ..id128(offerId)
          ..u32(failureCode);
      case FlarkV3ParserViewportPresentationDeliveryAcknowledged(:final ack):
        _writeViewportAck(writer, ack);
    }
    writer.finish();
    return FlarkV3WireProtocol.encode(
      FlarkV3WireFrame.owned(
        kind: FlarkV3WireFrameKind.request,
        opcode: opcode,
        correlationId: event.eventId,
        payload: writer.bytes,
      ),
    );
  }

  static FlarkV3ParserViewportPresentationEvent decodeEvent(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding expectedBinding,
  }) {
    _requireCanonicalBinding(expectedBinding);
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.request,
    );
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    if (frame.correlationId == 0) {
      throw const FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: 16,
        expected: 1,
        actual: 0,
      );
    }
    _requireDecodedBinding(header.binding, expectedBinding, reader.offset);
    try {
      final event = switch ((frame.opcode, header.variant)) {
        (FlarkV3WireOpcode.publishBegin, _viewportVariant) =>
          FlarkV3ParserViewportPresentationBegin(
            eventId: frame.correlationId,
            binding: header.binding,
            begin: _readViewportBegin(reader),
          ),
        (FlarkV3WireOpcode.publishPacket, _viewportVariant) =>
          FlarkV3ParserViewportPresentationPacket(
            eventId: frame.correlationId,
            binding: header.binding,
            packet: FlarkV3HostPublicationPacket.fromOwnedBytes(
              reader.remainder(),
            ),
          ),
        (FlarkV3WireOpcode.publishCommit, _viewportVariant) =>
          FlarkV3ParserViewportPresentationCommitRequested(
            eventId: frame.correlationId,
            binding: header.binding,
            request: _readViewportCommit(reader),
          ),
        (FlarkV3WireOpcode.publishAbort, _viewportVariant) =>
          FlarkV3ParserViewportPresentationAbortRequested(
            eventId: frame.correlationId,
            binding: header.binding,
            offerId: reader.offerId(),
          ),
        (FlarkV3WireOpcode.publishAbort, _failedVariant) =>
          FlarkV3ParserViewportPresentationFailed(
            eventId: frame.correlationId,
            binding: header.binding,
            offerId: reader.offerId(),
            failureCode: reader.u32(),
          ),
        (FlarkV3WireOpcode.acknowledgeDelivery, _viewportVariant) =>
          FlarkV3ParserViewportPresentationDeliveryAcknowledged(
            eventId: frame.correlationId,
            binding: header.binding,
            ack: _readViewportAck(reader),
          ),
        (
          FlarkV3WireOpcode.publishBegin ||
              FlarkV3WireOpcode.publishPacket ||
              FlarkV3WireOpcode.publishCommit ||
              FlarkV3WireOpcode.publishAbort ||
              FlarkV3WireOpcode.acknowledgeDelivery,
          _,
        ) =>
          throw FlarkV3HotInlineSidecarWireFormatException(
            FlarkV3HotInlineSidecarWireFailure.unknownVariant,
            byteOffset: 2,
            actual: header.variant,
          ),
        _ => throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.unexpectedOpcode,
          byteOffset: 8,
          actual: frame.opcode.code,
        ),
      };
      final documentSession = switch (event) {
        FlarkV3ParserViewportPresentationBegin(:final begin) =>
          begin.baseAck.sourceVersion.documentSession,
        FlarkV3ParserViewportPresentationDeliveryAcknowledged(:final ack) =>
          ack.baseAck.sourceVersion.documentSession,
        _ => null,
      };
      if (documentSession != null &&
          documentSession != event.binding.documentSession) {
        throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.identityMismatch,
          byteOffset: reader.offset,
        );
      }
      reader.finish();
      return event;
    } on FlarkV3HotInlineSidecarWireFormatException {
      rethrow;
    } on ArgumentError {
      throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }

  static Uint8List encodeCommand(
    FlarkV3ParserViewportPresentationHostPollCommand command,
  ) {
    final (ticket, outcome, rejection) = switch (command) {
      FlarkV3ParserViewportPresentationHostPollCompleted(
        :final ticket,
        :final outcome,
      ) =>
        (ticket, outcome, null),
      FlarkV3ParserViewportPresentationHostPollRejected(
        :final ticket,
        :final reason,
      ) =>
        (ticket, null, reason),
    };
    _requireCanonicalBinding(ticket.binding);
    if (outcome case FlarkV3ViewportPresentationHostCommitted(:final ack)
        when ack.baseAck.sourceVersion.documentSession !=
            ticket.binding.documentSession) {
      throw ArgumentError(
        'Committed viewport ACK document does not match its binding.',
      );
    }
    final status = rejection == null
        ? FlarkV3WireStatus.ok
        : _statusForReject(rejection);
    final variant = switch (outcome) {
      FlarkV3ViewportPresentationHostPacketCredit() => _packetCreditVariant,
      FlarkV3ViewportPresentationHostCommitted() => _committedVariant,
      FlarkV3ViewportPresentationHostAbortComplete() => _abortCompleteVariant,
      FlarkV3ViewportPresentationHostPollPending() ||
      FlarkV3ViewportPresentationHostClosed() => throw StateError(
        'Only terminal viewport outcomes cross the parser wire.',
      ),
      null => _viewportVariant,
    };
    final outcomeBytes = switch (outcome) {
      FlarkV3ViewportPresentationHostPacketCredit() => 20,
      FlarkV3ViewportPresentationHostCommitted() => ackBytes,
      FlarkV3ViewportPresentationHostAbortComplete() => 16,
      FlarkV3ViewportPresentationHostPollPending() ||
      FlarkV3ViewportPresentationHostClosed() => 0,
      null => 0,
    };
    final writer = _payloadWriter(pollTicketBytes + outcomeBytes);
    _writeHeader(writer, variant, ticket.binding);
    _writeViewportPollTicket(writer, ticket);
    switch (outcome) {
      case FlarkV3ViewportPresentationHostPacketCredit(
        :final offerId,
        :final nextFrameOrdinal,
      ):
        writer
          ..id128(offerId)
          ..u32(nextFrameOrdinal);
      case FlarkV3ViewportPresentationHostCommitted(:final ack):
        _writeViewportAck(writer, ack);
      case FlarkV3ViewportPresentationHostAbortComplete(:final offerId):
        writer.id128(offerId);
      case FlarkV3ViewportPresentationHostPollPending() ||
          FlarkV3ViewportPresentationHostClosed():
        throw StateError('Nonterminal viewport outcome reached the writer.');
      case null:
        break;
    }
    writer.finish();
    return FlarkV3WireProtocol.encode(
      FlarkV3WireFrame.owned(
        kind: FlarkV3WireFrameKind.response,
        opcode: FlarkV3WireOpcode.hostPoll,
        status: status,
        correlationId: ticket.pollTicket,
        payload: writer.bytes,
      ),
    );
  }

  static FlarkV3DecodedViewportPresentationHostPollCommand decodeCommand(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding expectedBinding,
  }) {
    _requireCanonicalBinding(expectedBinding);
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.response,
    );
    if (frame.correlationId == 0) {
      throw const FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: 16,
        expected: 1,
        actual: 0,
      );
    }
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    _requireDecodedBinding(header.binding, expectedBinding, reader.offset);
    try {
      if (frame.opcode != FlarkV3WireOpcode.hostPoll) {
        throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.unexpectedOpcode,
          byteOffset: 8,
          expected: FlarkV3WireOpcode.hostPoll.code,
          actual: frame.opcode.code,
        );
      }
      final ticket = _readViewportPollTicket(reader, header.binding);
      if (ticket.pollTicket != frame.correlationId) {
        throw FlarkV3HotInlineSidecarWireFormatException(
          FlarkV3HotInlineSidecarWireFailure.correlationMismatch,
          byteOffset: payloadPrefixBytes,
          expected: ticket.pollTicket,
          actual: frame.correlationId,
        );
      }

      final FlarkV3ParserViewportPresentationHostPollCommand command;
      if (frame.status != FlarkV3WireStatus.ok) {
        if (header.variant != _viewportVariant) {
          throw FlarkV3HotInlineSidecarWireFormatException(
            FlarkV3HotInlineSidecarWireFailure.unknownVariant,
            byteOffset: 2,
            actual: header.variant,
          );
        }
        command = FlarkV3ParserViewportPresentationHostPollRejected(
          ticket: ticket,
          reason: _rejectForStatus(frame.status),
        );
      } else {
        final outcome = switch (header.variant) {
          _packetCreditVariant => FlarkV3ViewportPresentationHostPacketCredit(
            offerId: reader.offerId(),
            nextFrameOrdinal: reader.u32(),
          ),
          _committedVariant => FlarkV3ViewportPresentationHostCommitted(
            _readViewportAck(reader),
          ),
          _abortCompleteVariant => FlarkV3ViewportPresentationHostAbortComplete(
            reader.offerId(),
          ),
          _ => throw FlarkV3HotInlineSidecarWireFormatException(
            FlarkV3HotInlineSidecarWireFailure.unknownVariant,
            byteOffset: 2,
            actual: header.variant,
          ),
        };
        _validateViewportPollOutcome(ticket, outcome, reader.offset);
        command = FlarkV3ParserViewportPresentationHostPollCompleted(
          ticket: ticket,
          outcome: outcome,
        );
      }
      reader.finish();
      return FlarkV3DecodedViewportPresentationHostPollCommand(
        correlationId: frame.correlationId,
        binding: header.binding,
        command: command,
      );
    } on FlarkV3HotInlineSidecarWireFormatException {
      rethrow;
    } on ArgumentError {
      throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }
}

final class FlarkV3DecodedInlineSidecarHostPollCommand {
  const FlarkV3DecodedInlineSidecarHostPollCommand({
    required this.correlationId,
    required this.binding,
    required this.command,
  });

  final int correlationId;
  final FlarkV3ParserSessionBinding binding;
  final FlarkV3ParserInlineSidecarHostPollCommand command;

  FlarkV3ParserInlineSidecarHostPollTicket get pollTicket => switch (command) {
    FlarkV3ParserInlineSidecarHostPollCompleted(:final ticket) ||
    FlarkV3ParserInlineSidecarHostPollRejected(:final ticket) => ticket,
  };
}

final class FlarkV3DecodedViewportPresentationHostPollCommand {
  const FlarkV3DecodedViewportPresentationHostPollCommand({
    required this.correlationId,
    required this.binding,
    required this.command,
  });

  final int correlationId;
  final FlarkV3ParserSessionBinding binding;
  final FlarkV3ParserViewportPresentationHostPollCommand command;

  FlarkV3ParserViewportPresentationHostPollTicket get pollTicket =>
      switch (command) {
        FlarkV3ParserViewportPresentationHostPollCompleted(:final ticket) ||
        FlarkV3ParserViewportPresentationHostPollRejected(
          :final ticket,
        ) => ticket,
      };
}

enum FlarkV3HotInlineSidecarWireFailure {
  unsupportedSchema,
  unexpectedOpcode,
  unknownVariant,
  truncatedPayload,
  trailingPayload,
  invalidValue,
  oversizedValue,
  identityMismatch,
  correlationMismatch,
  unmappedStatus,
}

final class FlarkV3HotInlineSidecarWireFormatException
    implements FormatException {
  const FlarkV3HotInlineSidecarWireFormatException(
    this.failure, {
    required this.byteOffset,
    this.expected,
    this.actual,
  });

  final FlarkV3HotInlineSidecarWireFailure failure;
  final int byteOffset;
  final int? expected;
  final int? actual;

  @override
  String get message =>
      'Invalid Flark v3 inline-sidecar payload: '
      '${failure.name}';

  @override
  int get offset => byteOffset;

  @override
  Object? get source => null;
}

final class _Header {
  const _Header(this.variant, this.binding);

  final int variant;
  final FlarkV3ParserSessionBinding binding;
}

void _writeHeader(
  _PayloadWriter writer,
  int variant,
  FlarkV3ParserSessionBinding binding,
) {
  writer
    ..u16(FlarkV3HotInlineSidecarWireCodec.payloadSchema)
    ..u16(variant)
    ..u32(binding.workerGeneration)
    ..u32(binding.documentSession.word0)
    ..u32(binding.documentSession.word1)
    ..u32(binding.documentSession.word2)
    ..u32(binding.documentSession.word3)
    ..u32(binding.sourceSessionIdentity);
}

_Header _readHeader(_PayloadReader reader) {
  final schema = reader.u16();
  if (schema != FlarkV3HotInlineSidecarWireCodec.payloadSchema) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.unsupportedSchema,
      byteOffset: 0,
      expected: FlarkV3HotInlineSidecarWireCodec.payloadSchema,
      actual: schema,
    );
  }
  final variant = reader.u16();
  final workerGeneration = reader.u32();
  final documentSession = FlarkV3DocumentSessionId(
    reader.u32(),
    reader.u32(),
    reader.u32(),
    reader.u32(),
  );
  final sourceSessionIdentity = reader.u32();
  final binding = FlarkV3ParserSessionBinding(
    documentSession: documentSession,
    sourceSessionIdentity: sourceSessionIdentity,
    workerGeneration: workerGeneration,
  );
  if (!_isCanonicalBinding(binding)) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: workerGeneration == 0
          ? 4
          : sourceSessionIdentity == 0
          ? 24
          : 8,
    );
  }
  return _Header(variant, binding);
}

_PayloadWriter _payloadWriter(int bodyBytes) {
  final length =
      FlarkV3HotInlineSidecarWireCodec.payloadPrefixBytes + bodyBytes;
  if (length > FlarkV3WireProtocol.maximumPayloadBytes) {
    throw RangeError.range(
      length,
      0,
      FlarkV3WireProtocol.maximumPayloadBytes,
      'payloadBytes',
    );
  }
  return _PayloadWriter(length);
}

void _writeBegin(
  _PayloadWriter writer,
  FlarkV3HotInlineSidecarOfferBegin begin,
) {
  writer
    ..u32(begin.schema)
    ..u32(1)
    ..id128(begin.offerId)
    ..id128(begin.publicationSession);
  _writeStructuralAck(writer, begin.baseAck);
  writer
    ..u64(FlarkV3ProtocolU64.fromU32(begin.binding.parserProfile.value))
    ..u64(begin.binding.refinementGeneration)
    ..u64(begin.binding.blockOrdinal)
    ..u32(begin.binding.physicalStartUtf8)
    ..u32(begin.binding.physicalEndUtf8)
    ..u32(begin.binding.visibleStartUtf8)
    ..u32(begin.binding.visibleEndUtf8)
    ..u32(begin.binding.physicalStartUtf16)
    ..u32(begin.binding.physicalEndUtf16)
    ..u32(begin.binding.visibleStartUtf16)
    ..u32(begin.binding.visibleEndUtf16)
    ..u32(begin.envelope.hio1EncodedBytes)
    ..u32(begin.envelope.ipr2DescriptorBytes)
    ..u32(begin.envelope.transferredNodeCount);
  switch (begin.envelope.disposition) {
    case FlarkV3HotInlineSidecarAuthoritative(
      :final logicalPageCount,
      :final factCount,
      :final storagePageCount,
      :final linkValueEntryCount,
      :final linkValueStoragePageCount,
      :final linkValueEncodedBytes,
      :final orderedCommitment256,
    ):
      writer
        ..u32(1)
        ..u32(0)
        ..u64(logicalPageCount)
        ..u64(factCount)
        ..u64(storagePageCount)
        ..u32(linkValueEntryCount)
        ..u32(linkValueEncodedBytes)
        ..u64(linkValueStoragePageCount)
        ..digest256(orderedCommitment256);
    case FlarkV3HotInlineSidecarUnsupported(
      :final reason,
      :final metadataCommitment256,
    ):
      writer
        ..u32(2)
        ..u32(reason)
        ..u64(FlarkV3ProtocolU64.zero)
        ..u64(FlarkV3ProtocolU64.zero)
        ..u64(FlarkV3ProtocolU64.zero)
        ..u32(0)
        ..u32(0)
        ..u64(FlarkV3ProtocolU64.zero)
        ..digest256(metadataCommitment256);
  }
  writer.digest256(begin.envelope.hio1EnvelopeDigest256);
  _writeLimits(writer, begin.limits);
}

FlarkV3HotInlineSidecarOfferBegin _readBegin(_PayloadReader reader) {
  final schema = reader.u32();
  if (schema != FlarkV3HotInlineSidecarOfferBegin.supportedSchema) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.unsupportedSchema,
      byteOffset: reader.offset - 4,
      expected: FlarkV3HotInlineSidecarOfferBegin.supportedSchema,
      actual: schema,
    );
  }
  final mode = reader.u32();
  if (mode != 1) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      expected: 1,
      actual: mode,
    );
  }
  final offerId = reader.offerId();
  final publicationSession = reader.publicationSessionId();
  final baseAck = _readStructuralAck(reader);
  final parserProfile = reader.u64();
  if (!parserProfile.fitsU32 || parserProfile.isZero) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  final binding = FlarkV3HotInlineSidecarBinding(
    parserProfile: FlarkV3SyntaxProfileId(parserProfile.lowWord),
    refinementGeneration: reader.u64(),
    blockOrdinal: reader.u64(),
    physicalStartUtf8: reader.u32(),
    physicalEndUtf8: reader.u32(),
    visibleStartUtf8: reader.u32(),
    visibleEndUtf8: reader.u32(),
    physicalStartUtf16: reader.u32(),
    physicalEndUtf16: reader.u32(),
    visibleStartUtf16: reader.u32(),
    visibleEndUtf16: reader.u32(),
  );
  final hio1EncodedBytes = reader.u32();
  final ipr2DescriptorBytes = reader.u32();
  final transferredNodeCount = reader.u32();
  final dispositionTag = reader.u32();
  final reason = reader.u32();
  final logicalPageCount = reader.u64();
  final factCount = reader.u64();
  final storagePageCount = reader.u64();
  final linkValueEntryCount = reader.u32();
  final linkValueEncodedBytes = reader.u32();
  final linkValueStoragePageCount = reader.u64();
  final dispositionCommitment = reader.digest256();
  final FlarkV3HotInlineSidecarDisposition disposition;
  if (dispositionTag == 1 && reason == 0) {
    disposition = FlarkV3HotInlineSidecarAuthoritative(
      logicalPageCount: logicalPageCount,
      factCount: factCount,
      storagePageCount: storagePageCount,
      linkValueEntryCount: linkValueEntryCount,
      linkValueStoragePageCount: linkValueStoragePageCount,
      linkValueEncodedBytes: linkValueEncodedBytes,
      orderedCommitment256: dispositionCommitment,
    );
  } else if (dispositionTag == 2 &&
      logicalPageCount.isZero &&
      factCount.isZero &&
      storagePageCount.isZero &&
      linkValueEntryCount == 0 &&
      linkValueStoragePageCount.isZero &&
      linkValueEncodedBytes == 0) {
    disposition = FlarkV3HotInlineSidecarUnsupported(
      reason: reason,
      metadataCommitment256: dispositionCommitment,
    );
  } else {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset,
    );
  }
  final envelope = FlarkV3HotInlineSidecarEnvelopeMetrics(
    hio1EncodedBytes: hio1EncodedBytes,
    ipr2DescriptorBytes: ipr2DescriptorBytes,
    transferredNodeCount: transferredNodeCount,
    hio1EnvelopeDigest256: reader.digest256(),
    disposition: disposition,
  );
  return FlarkV3HotInlineSidecarOfferBegin(
    schema: schema,
    offerId: offerId,
    publicationSession: publicationSession,
    baseAck: baseAck,
    binding: binding,
    envelope: envelope,
    limits: _readLimits(reader),
  );
}

void _writeCommit(
  _PayloadWriter writer,
  FlarkV3HotInlineSidecarCommitRequest request,
) {
  writer
    ..id128(request.offerId)
    ..u32(request.actualFrameCount)
    ..u32(request.actualEncodedFrameBytes)
    ..id128(request.rollingTransportDigest)
    ..id128(request.rootStreamDigest);
}

FlarkV3HotInlineSidecarCommitRequest _readCommit(_PayloadReader reader) =>
    FlarkV3HotInlineSidecarCommitRequest(
      offerId: reader.offerId(),
      actualFrameCount: reader.u32(),
      actualEncodedFrameBytes: reader.u32(),
      rollingTransportDigest: reader.digest128(),
      rootStreamDigest: reader.digest128(),
    );

void _writeInlineSidecarAck(
  _PayloadWriter writer,
  FlarkV3InlineSidecarAck ack,
) {
  writer.id128(ack.publicationSession);
  _writeStructuralAck(writer, ack.baseAck);
  writer
    ..u64(ack.refinementGeneration)
    ..u64(ack.blockOrdinal)
    ..u32(ack.transferredNodeCount)
    ..u32(ack.disposition.index + 1)
    ..digest256(ack.hio1EnvelopeDigest256)
    ..id128(ack.rootStreamDigest);
}

FlarkV3InlineSidecarAck _readInlineSidecarAck(_PayloadReader reader) {
  final publicationSession = reader.publicationSessionId();
  final baseAck = _readStructuralAck(reader);
  final refinementGeneration = reader.u64();
  final blockOrdinal = reader.u64();
  final transferredNodeCount = reader.u32();
  final dispositionCode = reader.u32();
  if (dispositionCode != 1 && dispositionCode != 2) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      expected: 2,
      actual: dispositionCode,
    );
  }
  return FlarkV3InlineSidecarAck(
    publicationSession: publicationSession,
    baseAck: baseAck,
    refinementGeneration: refinementGeneration,
    blockOrdinal: blockOrdinal,
    transferredNodeCount: transferredNodeCount,
    disposition: FlarkV3InlineSidecarAckDisposition.values[dispositionCode - 1],
    hio1EnvelopeDigest256: reader.digest256(),
    rootStreamDigest: reader.digest128(),
  );
}

void _writePollTicket(
  _PayloadWriter writer,
  FlarkV3ParserInlineSidecarHostPollTicket ticket,
) {
  writer
    ..u32(ticket.pollTicket)
    ..id128(ticket.offerId)
    ..u32(0x0100 + ticket.phase.index);
}

FlarkV3ParserInlineSidecarHostPollTicket _readPollTicket(
  _PayloadReader reader,
  FlarkV3ParserSessionBinding binding,
) {
  final pollTicket = reader.u32();
  final offerId = reader.offerId();
  final phaseCode = reader.u32();
  if (phaseCode < 0x0100 || phaseCode > 0x0102) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      expected: 0x0102,
      actual: phaseCode,
    );
  }
  return FlarkV3ParserInlineSidecarHostPollTicket(
    binding: binding,
    pollTicket: pollTicket,
    offerId: offerId,
    phase: FlarkV3ParserInlineSidecarHostPollPhase.values[phaseCode - 0x0100],
  );
}

void _validatePollOutcome(
  FlarkV3ParserInlineSidecarHostPollTicket ticket,
  FlarkV3InlineSidecarHostPollOutcome outcome,
  int offset,
) {
  final valid = switch ((ticket.phase, outcome)) {
    (
      FlarkV3ParserInlineSidecarHostPollPhase.packetCredit,
      FlarkV3InlineSidecarHostPacketCredit(
        :final offerId,
        :final nextFrameOrdinal,
      ),
    ) =>
      offerId == ticket.offerId && nextFrameOrdinal != 0,
    (
      FlarkV3ParserInlineSidecarHostPollPhase.commit,
      FlarkV3InlineSidecarHostCommitted(:final ack),
    ) =>
      ack.baseAck.sourceVersion.documentSession ==
          ticket.binding.documentSession,
    (
      FlarkV3ParserInlineSidecarHostPollPhase.abort,
      FlarkV3InlineSidecarHostAbortComplete(:final offerId),
    ) =>
      offerId == ticket.offerId,
    _ => false,
  };
  if (!valid) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: offset,
    );
  }
}

void _writeStructuralAck(_PayloadWriter writer, FlarkV3StructuralAck ack) {
  writer
    ..id128(ack.publicationSession)
    ..u32(ack.hostRevision.value);
  _writeSourceVersion(writer, ack.sourceVersion);
  writer
    ..u32(ack.sourceRoot.highWord)
    ..u32(ack.sourceRoot.lowWord)
    ..u32(ack.parseGeneration)
    ..u32(ack.grammarRevision)
    ..u32(ack.syntaxProfile.value)
    ..u32(ack.authorityMask.bits)
    ..u32(ack.recordCount)
    ..id128(ack.sequenceDigest)
    ..id128(ack.manifestDigest);
}

FlarkV3StructuralAck _readStructuralAck(_PayloadReader reader) =>
    FlarkV3StructuralAck(
      publicationSession: reader.publicationSessionId(),
      hostRevision: FlarkV3HostRevisionId(reader.u32()),
      sourceVersion: _readSourceVersion(reader),
      sourceRoot: FlarkV3SourceRootId(reader.u32(), reader.u32()),
      parseGeneration: reader.u32(),
      grammarRevision: reader.u32(),
      syntaxProfile: FlarkV3SyntaxProfileId(reader.u32()),
      authorityMask: FlarkV3StructuralAuthorityMask(reader.u32()),
      recordCount: reader.u32(),
      sequenceDigest: reader.digest128(),
      manifestDigest: reader.digest128(),
    );

void _writeSourceVersion(
  _PayloadWriter writer,
  FlarkV3SourceVersion sourceVersion,
) {
  writer
    ..id128(sourceVersion.documentSession)
    ..u32(sourceVersion.revision)
    ..u32(sourceVersion.metric.bytes)
    ..u32(sourceVersion.metric.utf16)
    ..u32(sourceVersion.contentHash.word0)
    ..u32(sourceVersion.contentHash.word1)
    ..u32(sourceVersion.contentHash.word2)
    ..u32(sourceVersion.contentHash.word3);
}

FlarkV3SourceVersion _readSourceVersion(_PayloadReader reader) =>
    FlarkV3SourceVersion(
      documentSession: reader.documentSessionId(),
      revision: reader.u32(),
      metric: FlarkV3SourceMetric(bytes: reader.u32(), utf16: reader.u32()),
      contentHash: FlarkV3ContentHash128(
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.u32(),
      ),
    );

void _writeLimits(_PayloadWriter writer, FlarkV3HostOfferLimits limits) {
  writer
    ..u32(limits.maximumFrameCount)
    ..u32(limits.maximumEncodedFrameBytes)
    ..u32(limits.maximumPacketBytes)
    ..u32(limits.maximumFrameBytes)
    ..u32(limits.maximumProgramChildren);
}

FlarkV3HostOfferLimits _readLimits(_PayloadReader reader) {
  final maximumFrameCount = reader.u32();
  final maximumEncodedFrameBytes = reader.u32();
  final maximumPacketBytes = reader.u32();
  final maximumFrameBytes = reader.u32();
  final maximumProgramChildren = reader.u32();
  if (maximumPacketBytes > FlarkV3HostOfferLimits.productMaximumPacketBytes ||
      maximumFrameBytes > FlarkV3HostOfferLimits.productMaximumFrameBytes ||
      maximumProgramChildren >
          FlarkV3HostOfferLimits.productMaximumProgramChildren) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.oversizedValue,
      byteOffset: reader.offset,
    );
  }
  return FlarkV3HostOfferLimits(
    maximumFrameCount: maximumFrameCount,
    maximumEncodedFrameBytes: maximumEncodedFrameBytes,
    maximumPacketBytes: maximumPacketBytes,
    maximumFrameBytes: maximumFrameBytes,
    maximumProgramChildren: maximumProgramChildren,
  );
}

void _writeViewportBegin(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationOfferBegin begin,
) {
  writer
    ..u32(begin.schema)
    ..u32(1)
    ..id128(begin.offerId)
    ..id128(begin.publicationSession);
  _writeStructuralAck(writer, begin.baseAck);
  _writeViewportBinding(writer, begin.binding);
  _writeViewportEnvelope(writer, begin.envelope);
  _writeViewportQueryLimits(writer, begin.queryLimits);
  _writeViewportOfferLimits(writer, begin.limits);
}

FlarkV3ViewportPresentationOfferBegin _readViewportBegin(
  _PayloadReader reader,
) {
  final schema = reader.u32();
  if (schema != FlarkV3ViewportPresentationOfferBegin.supportedSchema) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.unsupportedSchema,
      byteOffset: reader.offset - 4,
      expected: FlarkV3ViewportPresentationOfferBegin.supportedSchema,
      actual: schema,
    );
  }
  final mode = reader.u32();
  if (mode != 1) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      expected: 1,
      actual: mode,
    );
  }
  return FlarkV3ViewportPresentationOfferBegin(
    schema: schema,
    offerId: reader.offerId(),
    publicationSession: reader.publicationSessionId(),
    baseAck: _readStructuralAck(reader),
    binding: _readViewportBinding(reader),
    envelope: _readViewportEnvelope(reader),
    queryLimits: _readViewportQueryLimits(reader),
    limits: _readViewportOfferLimits(reader),
  );
}

void _writeViewportBinding(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationBinding binding,
) {
  writer.u32(binding.viewportGeneration);
  _writeViewportRange(writer, binding.requestedRange);
  _writeViewportRange(writer, binding.coveredRange);
  _writeViewportVisitStart(writer, binding.start);
  _writeViewportVisitStart(writer, binding.next);
  writer.u32(binding.complete ? 1 : 0);
}

FlarkV3ViewportPresentationBinding _readViewportBinding(_PayloadReader reader) {
  final viewportGeneration = reader.u32();
  final requestedRange = _readViewportRange(reader);
  final coveredRange = _readViewportRange(reader);
  final start = _readViewportVisitStart(reader);
  final next = _readViewportVisitStart(reader);
  final completeCode = reader.u32();
  if (completeCode > 1) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      expected: 1,
      actual: completeCode,
    );
  }
  return FlarkV3ViewportPresentationBinding(
    viewportGeneration: viewportGeneration,
    requestedRange: requestedRange,
    coveredRange: coveredRange,
    start: start,
    next: next,
    complete: completeCode == 1,
  );
}

void _writeViewportRange(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationMetricRange range,
) {
  writer
    ..u32(range.startUtf8)
    ..u32(range.startUtf16)
    ..u32(range.endUtf8)
    ..u32(range.endUtf16);
}

FlarkV3ViewportPresentationMetricRange _readViewportRange(
  _PayloadReader reader,
) => FlarkV3ViewportPresentationMetricRange(
  startUtf8: reader.u32(),
  startUtf16: reader.u32(),
  endUtf8: reader.u32(),
  endUtf16: reader.u32(),
);

void _writeViewportVisitStart(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationVisitStart start,
) {
  writer
    ..u64(start.blockOrdinal)
    ..u32(start.utf8Offset)
    ..u32(start.utf16Offset);
}

FlarkV3ViewportPresentationVisitStart _readViewportVisitStart(
  _PayloadReader reader,
) => FlarkV3ViewportPresentationVisitStart(
  blockOrdinal: reader.u64(),
  utf8Offset: reader.u32(),
  utf16Offset: reader.u32(),
);

void _writeViewportEnvelope(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationEnvelopeMetrics envelope,
) {
  writer
    ..u32(envelope.visitedStructuralEntries)
    ..u32(envelope.visitedStoragePages)
    ..u32(envelope.orderedLeafCount)
    ..u32(envelope.inlineSourceBytes)
    ..u32(envelope.factCount)
    ..u32(envelope.transferredNodeCount)
    ..u32(envelope.parserTransitions)
    ..digest256(envelope.aggregateEnvelopeDigest256);
}

FlarkV3ViewportPresentationEnvelopeMetrics _readViewportEnvelope(
  _PayloadReader reader,
) => FlarkV3ViewportPresentationEnvelopeMetrics(
  visitedStructuralEntries: reader.u32(),
  visitedStoragePages: reader.u32(),
  orderedLeafCount: reader.u32(),
  inlineSourceBytes: reader.u32(),
  factCount: reader.u32(),
  transferredNodeCount: reader.u32(),
  parserTransitions: reader.u32(),
  aggregateEnvelopeDigest256: reader.digest256(),
);

void _writeViewportQueryLimits(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationQueryLimits limits,
) {
  writer
    ..u32(limits.maximumStructuralEntries)
    ..u32(limits.maximumStoragePages)
    ..u32(limits.maximumInlineLeaves)
    ..u32(limits.maximumInlineLeafSourceBytes)
    ..u32(limits.maximumInlineSourceBytes)
    ..u32(limits.maximumFactRecords)
    ..u32(limits.maximumEncodedFrameBytes)
    ..u32(limits.maximumParserTransitions);
}

FlarkV3ViewportPresentationQueryLimits _readViewportQueryLimits(
  _PayloadReader reader,
) => FlarkV3ViewportPresentationQueryLimits(
  maximumStructuralEntries: reader.u32(),
  maximumStoragePages: reader.u32(),
  maximumInlineLeaves: reader.u32(),
  maximumInlineLeafSourceBytes: reader.u32(),
  maximumInlineSourceBytes: reader.u32(),
  maximumFactRecords: reader.u32(),
  maximumEncodedFrameBytes: reader.u32(),
  maximumParserTransitions: reader.u32(),
);

void _writeViewportOfferLimits(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationOfferLimits limits,
) {
  writer
    ..u32(limits.maximumFrameCount)
    ..u32(limits.maximumEncodedFrameBytes)
    ..u32(limits.maximumPacketBytes)
    ..u32(limits.maximumFrameBytes)
    ..u32(limits.maximumProgramChildren);
}

FlarkV3ViewportPresentationOfferLimits _readViewportOfferLimits(
  _PayloadReader reader,
) => FlarkV3ViewportPresentationOfferLimits(
  maximumFrameCount: reader.u32(),
  maximumEncodedFrameBytes: reader.u32(),
  maximumPacketBytes: reader.u32(),
  maximumFrameBytes: reader.u32(),
  maximumProgramChildren: reader.u32(),
);

void _writeViewportCommit(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationCommitRequest request,
) {
  writer
    ..id128(request.offerId)
    ..u32(request.actualFrameCount)
    ..u32(request.actualEncodedFrameBytes)
    ..id128(request.rollingTransportDigest)
    ..id128(request.aggregateRootStreamDigest);
}

FlarkV3ViewportPresentationCommitRequest _readViewportCommit(
  _PayloadReader reader,
) => FlarkV3ViewportPresentationCommitRequest(
  offerId: reader.offerId(),
  actualFrameCount: reader.u32(),
  actualEncodedFrameBytes: reader.u32(),
  rollingTransportDigest: reader.digest128(),
  aggregateRootStreamDigest: reader.digest128(),
);

void _writeViewportAck(
  _PayloadWriter writer,
  FlarkV3ViewportPresentationAck ack,
) {
  writer.id128(ack.publicationSession);
  _writeStructuralAck(writer, ack.baseAck);
  _writeViewportBinding(writer, ack.binding);
  _writeViewportEnvelope(writer, ack.envelope);
  writer
    ..u32(ack.actualFrameCount)
    ..u32(ack.actualEncodedFrameBytes)
    ..id128(ack.aggregateRootStreamDigest);
}

FlarkV3ViewportPresentationAck _readViewportAck(_PayloadReader reader) =>
    FlarkV3ViewportPresentationAck(
      publicationSession: reader.publicationSessionId(),
      baseAck: _readStructuralAck(reader),
      binding: _readViewportBinding(reader),
      envelope: _readViewportEnvelope(reader),
      actualFrameCount: reader.u32(),
      actualEncodedFrameBytes: reader.u32(),
      aggregateRootStreamDigest: reader.digest128(),
    );

void _writeViewportPollTicket(
  _PayloadWriter writer,
  FlarkV3ParserViewportPresentationHostPollTicket ticket,
) {
  writer
    ..u32(ticket.pollTicket)
    ..id128(ticket.offerId)
    ..u32(0x0200 + ticket.phase.index);
}

FlarkV3ParserViewportPresentationHostPollTicket _readViewportPollTicket(
  _PayloadReader reader,
  FlarkV3ParserSessionBinding binding,
) {
  final pollTicket = reader.u32();
  final offerId = reader.offerId();
  final phaseCode = reader.u32();
  if (phaseCode < 0x0200 || phaseCode > 0x0202) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      expected: 0x0202,
      actual: phaseCode,
    );
  }
  return FlarkV3ParserViewportPresentationHostPollTicket(
    binding: binding,
    pollTicket: pollTicket,
    offerId: offerId,
    phase: FlarkV3ParserViewportPresentationHostPollPhase
        .values[phaseCode - 0x0200],
  );
}

void _validateViewportPollOutcome(
  FlarkV3ParserViewportPresentationHostPollTicket ticket,
  FlarkV3ViewportPresentationHostPollOutcome outcome,
  int offset,
) {
  final valid = switch ((ticket.phase, outcome)) {
    (
      FlarkV3ParserViewportPresentationHostPollPhase.packetCredit,
      FlarkV3ViewportPresentationHostPacketCredit(
        :final offerId,
        :final nextFrameOrdinal,
      ),
    ) =>
      offerId == ticket.offerId && nextFrameOrdinal != 0,
    (
      FlarkV3ParserViewportPresentationHostPollPhase.commit,
      FlarkV3ViewportPresentationHostCommitted(:final ack),
    ) =>
      ack.baseAck.sourceVersion.documentSession ==
          ticket.binding.documentSession,
    (
      FlarkV3ParserViewportPresentationHostPollPhase.abort,
      FlarkV3ViewportPresentationHostAbortComplete(:final offerId),
    ) =>
      offerId == ticket.offerId,
    _ => false,
  };
  if (!valid) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.invalidValue,
      byteOffset: offset,
    );
  }
}

FlarkV3WireStatus _statusForReject(FlarkV3HostRejectReason reason) =>
    switch (reason) {
      FlarkV3HostRejectReason.invalid => FlarkV3WireStatus.invalid,
      FlarkV3HostRejectReason.backpressure => FlarkV3WireStatus.backpressure,
      FlarkV3HostRejectReason.staleSource => FlarkV3WireStatus.staleSource,
      FlarkV3HostRejectReason.exactSourceMismatch =>
        FlarkV3WireStatus.exactSourceMismatch,
      FlarkV3HostRejectReason.sessionSnapshotRequired =>
        FlarkV3WireStatus.sessionSnapshotRequired,
      FlarkV3HostRejectReason.baseMismatch => FlarkV3WireStatus.baseMismatch,
      FlarkV3HostRejectReason.wrongOffer => FlarkV3WireStatus.wrongOffer,
      FlarkV3HostRejectReason.corruptPublication =>
        FlarkV3WireStatus.corruptPayload,
      FlarkV3HostRejectReason.queryBoundExceeded =>
        FlarkV3WireStatus.queryBoundExceeded,
      FlarkV3HostRejectReason.foregroundBoundExceeded =>
        FlarkV3WireStatus.foregroundBoundExceeded,
      FlarkV3HostRejectReason.superseded => FlarkV3WireStatus.superseded,
      FlarkV3HostRejectReason.closed => FlarkV3WireStatus.closed,
    };

FlarkV3HostRejectReason _rejectForStatus(FlarkV3WireStatus status) =>
    switch (status) {
      FlarkV3WireStatus.invalid => FlarkV3HostRejectReason.invalid,
      FlarkV3WireStatus.backpressure => FlarkV3HostRejectReason.backpressure,
      FlarkV3WireStatus.staleSource => FlarkV3HostRejectReason.staleSource,
      FlarkV3WireStatus.exactSourceMismatch =>
        FlarkV3HostRejectReason.exactSourceMismatch,
      FlarkV3WireStatus.sessionSnapshotRequired =>
        FlarkV3HostRejectReason.sessionSnapshotRequired,
      FlarkV3WireStatus.baseMismatch => FlarkV3HostRejectReason.baseMismatch,
      FlarkV3WireStatus.wrongOffer => FlarkV3HostRejectReason.wrongOffer,
      FlarkV3WireStatus.corruptPayload =>
        FlarkV3HostRejectReason.corruptPublication,
      FlarkV3WireStatus.queryBoundExceeded =>
        FlarkV3HostRejectReason.queryBoundExceeded,
      FlarkV3WireStatus.foregroundBoundExceeded =>
        FlarkV3HostRejectReason.foregroundBoundExceeded,
      FlarkV3WireStatus.superseded => FlarkV3HostRejectReason.superseded,
      FlarkV3WireStatus.closed => FlarkV3HostRejectReason.closed,
      _ => throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.unmappedStatus,
        byteOffset: 10,
        actual: status.code,
      ),
    };

bool _isCanonicalBinding(FlarkV3ParserSessionBinding binding) =>
    binding.workerGeneration > 0 &&
    binding.workerGeneration <= flarkV3TransportV1Maximum &&
    binding.sourceSessionIdentity > 0 &&
    binding.sourceSessionIdentity <= flarkV3TransportV1Maximum &&
    (binding.documentSession.word0 != 0 ||
        binding.documentSession.word1 != 0 ||
        binding.documentSession.word2 != 0 ||
        binding.documentSession.word3 != 0);

void _requireCanonicalBinding(FlarkV3ParserSessionBinding binding) {
  if (!_isCanonicalBinding(binding)) {
    throw ArgumentError.value(
      binding,
      'binding',
      'Sidecar binding must be a known canonical endpoint identity.',
    );
  }
}

void _requireDecodedBinding(
  FlarkV3ParserSessionBinding actual,
  FlarkV3ParserSessionBinding expected,
  int offset,
) {
  if (actual != expected) {
    throw FlarkV3HotInlineSidecarWireFormatException(
      FlarkV3HotInlineSidecarWireFailure.identityMismatch,
      byteOffset: offset,
    );
  }
}

final class _PayloadWriter {
  _PayloadWriter(int length) : bytes = Uint8List(length) {
    _data = ByteData.sublistView(bytes);
  }

  final Uint8List bytes;
  late final ByteData _data;
  int _offset = 0;

  void u16(int value) {
    if (value < 0 || value > 0xffff) {
      throw RangeError.range(value, 0, 0xffff, 'u16');
    }
    _require(2);
    _data.setUint16(_offset, value, Endian.little);
    _offset += 2;
  }

  void u32(int value) {
    if (value < 0 || value > 0xffffffff) {
      throw RangeError.range(value, 0, 0xffffffff, 'u32');
    }
    _require(4);
    _data.setUint32(_offset, value, Endian.little);
    _offset += 4;
  }

  void u64(FlarkV3ProtocolU64 value) {
    u32(value.lowWord);
    u32(value.highWord);
  }

  void id128(FlarkV3ProtocolId128 value) {
    u32(value.word0);
    u32(value.word1);
    u32(value.word2);
    u32(value.word3);
  }

  void digest256(FlarkV3ProtocolDigest256 value) {
    u32(value.word0);
    u32(value.word1);
    u32(value.word2);
    u32(value.word3);
    u32(value.word4);
    u32(value.word5);
    u32(value.word6);
    u32(value.word7);
  }

  void raw(Uint8List value) {
    _require(value.length);
    bytes.setRange(_offset, _offset + value.length, value);
    _offset += value.length;
  }

  void finish() {
    if (_offset != bytes.length) {
      throw StateError('Inline-sidecar payload size calculation diverged.');
    }
  }

  void _require(int count) {
    if (_offset + count > bytes.length) {
      throw StateError('Inline-sidecar payload writer overflowed.');
    }
  }
}

final class _PayloadReader {
  _PayloadReader(this.bytes) : _data = ByteData.sublistView(bytes);

  final Uint8List bytes;
  final ByteData _data;
  int offset = 0;

  int u16() {
    _require(2);
    final value = _data.getUint16(offset, Endian.little);
    offset += 2;
    return value;
  }

  int u32() {
    _require(4);
    final value = _data.getUint32(offset, Endian.little);
    offset += 4;
    return value;
  }

  FlarkV3ProtocolU64 u64() =>
      FlarkV3ProtocolU64(lowWord: u32(), highWord: u32());

  FlarkV3OfferId offerId() => FlarkV3OfferId(u32(), u32(), u32(), u32());

  FlarkV3PublicationSessionId publicationSessionId() =>
      FlarkV3PublicationSessionId(u32(), u32(), u32(), u32());

  FlarkV3DocumentSessionId documentSessionId() =>
      FlarkV3DocumentSessionId(u32(), u32(), u32(), u32());

  FlarkV3ProtocolDigest128 digest128() =>
      FlarkV3ProtocolDigest128(u32(), u32(), u32(), u32());

  FlarkV3ProtocolDigest256 digest256() => FlarkV3ProtocolDigest256(
    u32(),
    u32(),
    u32(),
    u32(),
    u32(),
    u32(),
    u32(),
    u32(),
  );

  Uint8List remainder() {
    final result = Uint8List.sublistView(bytes, offset);
    offset = bytes.length;
    return result;
  }

  void finish() {
    if (offset != bytes.length) {
      throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.trailingPayload,
        byteOffset: offset,
        expected: offset,
        actual: bytes.length,
      );
    }
  }

  void _require(int count) {
    if (offset + count > bytes.length) {
      throw FlarkV3HotInlineSidecarWireFormatException(
        FlarkV3HotInlineSidecarWireFailure.truncatedPayload,
        byteOffset: offset,
        expected: offset + count,
        actual: bytes.length,
      );
    }
  }
}

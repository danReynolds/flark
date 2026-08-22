import 'dart:typed_data';

import '../host/host.dart';
import '../source/source.dart';
import 'flark_v3_parser_transport.dart';
import 'flark_v3_wire_protocol.dart';

/// Binary codec for the credited parser-publication handshake.
///
/// This is deliberately narrower than the complete parser transport. Source
/// replica synchronization, restart, and close have independent lifetimes and
/// are not smuggled through this schema. Both native-isolate and Web Worker
/// endpoints use these exact bytes; platform code only owns buffer transfer.
final class FlarkV3ParserPublicationWireCodec {
  const FlarkV3ParserPublicationWireCodec._();

  static const int payloadSchema = 4;
  static const int _prefixBytes = 28;
  static const int _pollTicketBytes = 24;
  static const int _ackBytes = 124;
  static const int _beginBytesWithoutBase = 144;

  static const int maximumPacketFrameCount =
      FlarkV3HostPublicationPacket.maximumFrameCount;
  static const int maximumPacketEncodedFrameBytes =
      FlarkV3HostPublicationPacket.maximumAggregateFrameBytes;
  static const int maximumPacketBodyBytes =
      FlarkV3HostPublicationPacket.maximumRawBytes;

  static Uint8List encodeEvent(FlarkV3ParserEvent event) {
    if (event is! FlarkV3ParserPublicationEvent) {
      throw ArgumentError.value(
        event,
        'event',
        'Not a parser-publication event.',
      );
    }
    _requireCanonicalBinding(event.binding);
    switch (event) {
      case FlarkV3ParserPublicationBegin(:final begin):
        if (begin.sourceVersion.documentSession !=
            event.binding.documentSession) {
          throw ArgumentError(
            'Publication source document does not match its binding.',
          );
        }
      case FlarkV3ParserPublicationDeliveryAcknowledged(:final ack):
        if (ack.sourceVersion.documentSession !=
            event.binding.documentSession) {
          throw ArgumentError(
            'Delivery acknowledgement document does not match its binding.',
          );
        }
      default:
        break;
    }
    final opcode = switch (event) {
      FlarkV3ParserPublicationBegin() => FlarkV3WireOpcode.publishBegin,
      FlarkV3ParserPublicationPacket() => FlarkV3WireOpcode.publishPacket,
      FlarkV3ParserPublicationCommitRequested() =>
        FlarkV3WireOpcode.publishCommit,
      FlarkV3ParserPublicationAbortRequested() ||
      FlarkV3ParserPublicationFailed() => FlarkV3WireOpcode.publishAbort,
      FlarkV3ParserPublicationDeliveryAcknowledged() =>
        FlarkV3WireOpcode.acknowledgeDelivery,
    };
    final variant = event is FlarkV3ParserPublicationFailed ? 1 : 0;
    final bodyBytes = switch (event) {
      FlarkV3ParserPublicationBegin(:final begin) =>
        _beginBytesWithoutBase + (begin.baseAck == null ? 0 : _ackBytes),
      FlarkV3ParserPublicationPacket(:final packet) => packet.rawBytes.length,
      FlarkV3ParserPublicationCommitRequested() => 56,
      FlarkV3ParserPublicationAbortRequested() => 16,
      FlarkV3ParserPublicationFailed() => 20,
      FlarkV3ParserPublicationDeliveryAcknowledged() => _ackBytes,
    };
    final writer = _payloadWriter(bodyBytes);
    _writeHeader(writer, variant, event.binding);
    switch (event) {
      case FlarkV3ParserPublicationBegin(:final begin):
        _writeBegin(writer, begin);
      case FlarkV3ParserPublicationPacket(:final packet):
        _writePacket(writer, packet);
      case FlarkV3ParserPublicationCommitRequested(:final request):
        _writeCommit(writer, request);
      case FlarkV3ParserPublicationAbortRequested(:final offerId):
        writer.id128(offerId);
      case FlarkV3ParserPublicationFailed(:final offerId, :final failureCode):
        writer
          ..id128(offerId)
          ..u32(failureCode);
      case FlarkV3ParserPublicationDeliveryAcknowledged(:final ack):
        _writeAck(writer, ack);
    }
    writer.finish();
    return FlarkV3WireProtocol.encode(
      FlarkV3WireFrame.owned(
        kind: FlarkV3WireFrameKind.request,
        opcode: opcode,
        correlationId: _positiveU32(event.eventId, 'event.eventId'),
        payload: writer.bytes,
      ),
    );
  }

  static FlarkV3ParserEvent decodeEvent(
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
      throw FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.invalidValue,
        byteOffset: 16,
      );
    }
    _requireDecodedBinding(header.binding, expectedBinding, reader);
    try {
      final event = switch (frame.opcode) {
        FlarkV3WireOpcode.publishBegin when header.variant == 0 =>
          FlarkV3ParserPublicationBegin(
            eventId: frame.correlationId,
            binding: header.binding,
            begin: _readBegin(reader),
          ),
        FlarkV3WireOpcode.publishPacket when header.variant == 0 =>
          FlarkV3ParserPublicationPacket(
            eventId: frame.correlationId,
            binding: header.binding,
            packet: _readPacket(reader),
          ),
        FlarkV3WireOpcode.publishCommit when header.variant == 0 =>
          FlarkV3ParserPublicationCommitRequested(
            eventId: frame.correlationId,
            binding: header.binding,
            request: _readCommit(reader),
          ),
        FlarkV3WireOpcode.publishAbort when header.variant == 0 =>
          FlarkV3ParserPublicationAbortRequested(
            eventId: frame.correlationId,
            binding: header.binding,
            offerId: reader.offerId(),
          ),
        FlarkV3WireOpcode.publishAbort when header.variant == 1 =>
          FlarkV3ParserPublicationFailed(
            eventId: frame.correlationId,
            binding: header.binding,
            offerId: reader.offerId(),
            failureCode: reader.u32(),
          ),
        FlarkV3WireOpcode.acknowledgeDelivery when header.variant == 0 =>
          FlarkV3ParserPublicationDeliveryAcknowledged(
            eventId: frame.correlationId,
            binding: header.binding,
            ack: _readAck(reader),
          ),
        _ => throw FlarkV3ParserPublicationWireFormatException(
          frame.opcode == FlarkV3WireOpcode.publishBegin ||
                  frame.opcode == FlarkV3WireOpcode.publishPacket ||
                  frame.opcode == FlarkV3WireOpcode.publishCommit ||
                  frame.opcode == FlarkV3WireOpcode.publishAbort ||
                  frame.opcode == FlarkV3WireOpcode.acknowledgeDelivery
              ? FlarkV3ParserPublicationWireFailure.unknownVariant
              : FlarkV3ParserPublicationWireFailure.unexpectedOpcode,
          byteOffset:
              frame.opcode == FlarkV3WireOpcode.publishBegin ||
                  frame.opcode == FlarkV3WireOpcode.publishPacket ||
                  frame.opcode == FlarkV3WireOpcode.publishCommit ||
                  frame.opcode == FlarkV3WireOpcode.publishAbort ||
                  frame.opcode == FlarkV3WireOpcode.acknowledgeDelivery
              ? 2
              : 8,
          actual:
              frame.opcode == FlarkV3WireOpcode.publishBegin ||
                  frame.opcode == FlarkV3WireOpcode.publishPacket ||
                  frame.opcode == FlarkV3WireOpcode.publishCommit ||
                  frame.opcode == FlarkV3WireOpcode.publishAbort ||
                  frame.opcode == FlarkV3WireOpcode.acknowledgeDelivery
              ? header.variant
              : frame.opcode.code,
        ),
      };
      _requireDecodedEventDocument(event, reader);
      reader.finish();
      return event;
    } on FlarkV3ParserPublicationWireFormatException {
      rethrow;
    } on ArgumentError catch (_) {
      throw FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }

  /// Encodes a terminal host-poll result for one exact publication endpoint.
  ///
  /// Event receipts belong to the session wire because event credit is global
  /// to the parser endpoint, not scoped to the publication subprotocol.
  /// Correlation here is therefore always owned by the causal [pollTicket].
  static Uint8List encodeCommand(FlarkV3ParserCommand command) {
    final (ticket, outcome, rejection) = switch (command) {
      FlarkV3ParserHostPollCompleted(:final ticket, :final outcome) => (
        ticket,
        outcome,
        null,
      ),
      FlarkV3ParserHostPollRejected(:final ticket, :final reason) => (
        ticket,
        null,
        reason,
      ),
      _ => throw ArgumentError.value(
        command,
        'command',
        'Not a parser-publication command.',
      ),
    };
    _requireCanonicalBinding(ticket.binding);
    if (outcome case FlarkV3HostCommitted(:final ack)
        when ack.sourceVersion.documentSession !=
            ticket.binding.documentSession) {
      throw ArgumentError(
        'Committed acknowledgement document does not match its binding.',
      );
    }
    final status = rejection == null
        ? FlarkV3WireStatus.ok
        : _statusForReject(rejection);
    final variant = switch (outcome) {
      null => 0,
      FlarkV3HostPacketCredit() => 1,
      FlarkV3HostCommitted() => 2,
      FlarkV3HostAbortComplete() => 3,
      FlarkV3HostPollPending() || FlarkV3HostClosed() => throw StateError(
        'Only terminal publication outcomes cross the parser wire.',
      ),
    };
    final outcomeBytes = switch (outcome) {
      FlarkV3HostPacketCredit() => 20,
      FlarkV3HostCommitted() => _ackBytes,
      FlarkV3HostAbortComplete() => 16,
      null => 0,
      FlarkV3HostPollPending() || FlarkV3HostClosed() => 0,
    };
    final writer = _payloadWriter(_pollTicketBytes + outcomeBytes);
    _writeHeader(writer, variant, ticket.binding);
    _writePollTicket(writer, ticket);
    switch (outcome) {
      case FlarkV3HostPacketCredit(:final offerId, :final nextFrameOrdinal):
        writer
          ..id128(offerId)
          ..u32(nextFrameOrdinal);
      case FlarkV3HostCommitted(:final ack):
        _writeAck(writer, ack);
      case FlarkV3HostAbortComplete(:final offerId):
        writer.id128(offerId);
      case null:
        break;
      case FlarkV3HostPollPending() || FlarkV3HostClosed():
        throw StateError('Nonterminal host outcome reached the wire writer.');
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

  static FlarkV3DecodedParserPublicationCommand decodeCommand(
    Uint8List bytes, {
    required FlarkV3ParserSessionBinding expectedBinding,
  }) {
    _requireCanonicalBinding(expectedBinding);
    final frame = FlarkV3WireProtocol.decode(
      bytes,
      kind: FlarkV3WireFrameKind.response,
    );
    if (frame.correlationId == 0) {
      throw const FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.invalidValue,
        byteOffset: 16,
      );
    }
    final reader = _PayloadReader(frame.payload);
    final header = _readHeader(reader);
    _requireDecodedBinding(header.binding, expectedBinding, reader);
    try {
      final FlarkV3ParserCommand command;
      if (frame.opcode == FlarkV3WireOpcode.hostPoll) {
        final ticket = _readPollTicket(reader, header.binding);
        if (ticket.pollTicket != frame.correlationId) {
          throw FlarkV3ParserPublicationWireFormatException(
            FlarkV3ParserPublicationWireFailure.correlationMismatch,
            byteOffset: _prefixBytes,
            expected: ticket.pollTicket,
            actual: frame.correlationId,
          );
        }
        if (frame.status != FlarkV3WireStatus.ok) {
          if (header.variant != 0) {
            throw FlarkV3ParserPublicationWireFormatException(
              FlarkV3ParserPublicationWireFailure.unknownVariant,
              byteOffset: 2,
              actual: header.variant,
            );
          }
          command = FlarkV3ParserHostPollRejected(
            ticket: ticket,
            reason: _rejectForStatus(frame.status),
          );
        } else {
          final outcome = switch (header.variant) {
            1 => FlarkV3HostPacketCredit(
              offerId: reader.offerId(),
              nextFrameOrdinal: reader.u32(),
            ),
            2 => FlarkV3HostCommitted(_readAck(reader)),
            3 => FlarkV3HostAbortComplete(reader.offerId()),
            _ => throw FlarkV3ParserPublicationWireFormatException(
              FlarkV3ParserPublicationWireFailure.unknownVariant,
              byteOffset: 2,
              actual: header.variant,
            ),
          };
          if (outcome case FlarkV3HostCommitted(:final ack)
              when ack.sourceVersion.documentSession !=
                  header.binding.documentSession) {
            throw FlarkV3ParserPublicationWireFormatException(
              FlarkV3ParserPublicationWireFailure.identityMismatch,
              byteOffset: reader.offset,
            );
          }
          command = FlarkV3ParserHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          );
        }
      } else {
        throw FlarkV3ParserPublicationWireFormatException(
          FlarkV3ParserPublicationWireFailure.unexpectedOpcode,
          byteOffset: 8,
          actual: frame.opcode.code,
        );
      }
      reader.finish();
      return FlarkV3DecodedParserPublicationCommand(
        correlationId: frame.correlationId,
        binding: header.binding,
        command: command,
      );
    } on FlarkV3ParserPublicationWireFormatException {
      rethrow;
    } on ArgumentError catch (_) {
      throw FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.invalidValue,
        byteOffset: reader.offset,
      );
    }
  }
}

final class FlarkV3DecodedParserPublicationCommand {
  const FlarkV3DecodedParserPublicationCommand({
    required this.correlationId,
    required this.binding,
    required this.command,
  });

  final int correlationId;
  final FlarkV3ParserSessionBinding binding;
  final FlarkV3ParserCommand command;

  int get workerGeneration => binding.workerGeneration;

  FlarkV3ParserHostPollTicket? get pollTicket => switch (command) {
    FlarkV3ParserHostPollCompleted(:final ticket) ||
    FlarkV3ParserHostPollRejected(:final ticket) => ticket,
    _ => null,
  };
}

enum FlarkV3ParserPublicationWireFailure {
  unsupportedSchema,
  unexpectedOpcode,
  unknownVariant,
  truncatedPayload,
  trailingPayload,
  invalidValue,
  identityMismatch,
  correlationMismatch,
  unmappedStatus,
}

final class FlarkV3ParserPublicationWireFormatException
    implements FormatException {
  const FlarkV3ParserPublicationWireFormatException(
    this.failure, {
    required this.byteOffset,
    this.expected,
    this.actual,
  });

  final FlarkV3ParserPublicationWireFailure failure;
  final int byteOffset;
  final int? expected;
  final int? actual;

  @override
  String get message => 'Invalid Flark v3 publication payload: ${failure.name}';

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
  _requireCanonicalBinding(binding);
  writer
    ..u16(FlarkV3ParserPublicationWireCodec.payloadSchema)
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
  if (schema != FlarkV3ParserPublicationWireCodec.payloadSchema) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.unsupportedSchema,
      byteOffset: 0,
      expected: FlarkV3ParserPublicationWireCodec.payloadSchema,
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
  final unknownDocument =
      documentSession.word0 == 0 &&
      documentSession.word1 == 0 &&
      documentSession.word2 == 0 &&
      documentSession.word3 == 0;
  if (workerGeneration == 0 || sourceSessionIdentity == 0 || unknownDocument) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.invalidValue,
      byteOffset: workerGeneration == 0
          ? 4
          : sourceSessionIdentity == 0
          ? 24
          : 8,
    );
  }
  final binding = FlarkV3ParserSessionBinding(
    documentSession: documentSession,
    sourceSessionIdentity: sourceSessionIdentity,
    workerGeneration: workerGeneration,
  );
  return _Header(variant, binding);
}

void _writePollTicket(
  _PayloadWriter writer,
  FlarkV3ParserHostPollTicket ticket,
) {
  writer
    ..u32(ticket.pollTicket)
    ..id128(ticket.offerId)
    ..u32(ticket.phase.index);
}

FlarkV3ParserHostPollTicket _readPollTicket(
  _PayloadReader reader,
  FlarkV3ParserSessionBinding binding,
) {
  final pollTicket = reader.u32();
  final offerId = reader.offerId();
  final phase = reader.u32();
  if (pollTicket == 0 || phase >= FlarkV3ParserHostPollPhase.values.length) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.invalidValue,
      byteOffset: phase >= FlarkV3ParserHostPollPhase.values.length
          ? reader.offset - 4
          : reader.offset - FlarkV3ParserPublicationWireCodec._pollTicketBytes,
      expected: phase >= FlarkV3ParserHostPollPhase.values.length
          ? FlarkV3ParserHostPollPhase.values.length - 1
          : 1,
      actual: phase >= FlarkV3ParserHostPollPhase.values.length
          ? phase
          : pollTicket,
    );
  }
  return FlarkV3ParserHostPollTicket(
    binding: binding,
    pollTicket: pollTicket,
    offerId: offerId,
    phase: FlarkV3ParserHostPollPhase.values[phase],
  );
}

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
      'Publication binding must be a known canonical endpoint identity.',
    );
  }
}

void _requireDecodedBinding(
  FlarkV3ParserSessionBinding actual,
  FlarkV3ParserSessionBinding expected,
  _PayloadReader reader,
) {
  if (actual != expected) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.identityMismatch,
      byteOffset: reader.offset,
    );
  }
}

void _requireDecodedEventDocument(
  FlarkV3ParserPublicationEvent event,
  _PayloadReader reader,
) {
  final document = switch (event) {
    FlarkV3ParserPublicationBegin(:final begin) =>
      begin.sourceVersion.documentSession,
    FlarkV3ParserPublicationDeliveryAcknowledged(:final ack) =>
      ack.sourceVersion.documentSession,
    _ => null,
  };
  if (document != null && document != event.binding.documentSession) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.identityMismatch,
      byteOffset: reader.offset,
    );
  }
}

_PayloadWriter _payloadWriter(int bodyBytes) {
  final length = FlarkV3ParserPublicationWireCodec._prefixBytes + bodyBytes;
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

void _writeBegin(_PayloadWriter writer, FlarkV3HostOfferBegin begin) {
  if (begin.limits.maximumPacketBytes >
      FlarkV3ParserPublicationWireCodec.maximumPacketBodyBytes) {
    throw ArgumentError.value(
      begin.limits.maximumPacketBytes,
      'begin.limits.maximumPacketBytes',
      'The advertised packet exceeds the publication wire envelope.',
    );
  }
  writer
    ..u32(begin.schema)
    ..id128(begin.offerId)
    ..id128(begin.publicationSession)
    ..u32(begin.targetHostRevision.value);
  _writeSourceVersion(writer, begin.sourceVersion);
  writer
    ..u32(begin.sourceRoot.highWord)
    ..u32(begin.sourceRoot.lowWord)
    ..u32(begin.parseGeneration)
    ..u32(begin.grammarRevision)
    ..u32(begin.syntaxProfile.value)
    ..u32(begin.authorityMask.bits)
    ..u32(begin.mode.index)
    ..u32(begin.baseAck == null ? 0 : 1);
  if (begin.baseAck case final base?) _writeAck(writer, base);
  writer
    ..u32(begin.transferredRecordCount)
    ..u32(begin.targetRecordCount)
    ..u32(begin.limits.maximumFrameCount)
    ..u32(begin.limits.maximumEncodedFrameBytes)
    ..u32(begin.limits.maximumPacketBytes)
    ..u32(begin.limits.maximumFrameBytes)
    ..u32(begin.limits.maximumProgramChildren);
}

FlarkV3HostOfferBegin _readBegin(_PayloadReader reader) {
  final schema = reader.u32();
  if (schema != FlarkV3HostOfferBegin.supportedManifestSchema) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.unsupportedSchema,
      byteOffset: reader.offset - 4,
      expected: FlarkV3HostOfferBegin.supportedManifestSchema,
      actual: schema,
    );
  }
  final offerId = reader.offerId();
  final publicationSession = reader.publicationSessionId();
  final hostRevision = FlarkV3HostRevisionId(reader.u32());
  final sourceVersion = _readSourceVersion(reader);
  final sourceRoot = FlarkV3SourceRootId(reader.u32(), reader.u32());
  final parseGeneration = reader.u32();
  final grammarRevision = reader.u32();
  final syntaxProfile = FlarkV3SyntaxProfileId(reader.u32());
  final authorityMask = FlarkV3StructuralAuthorityMask(reader.u32());
  final modeIndex = reader.u32();
  if (modeIndex >= FlarkV3PublicationMode.values.length) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      actual: modeIndex,
    );
  }
  final hasBase = reader.u32();
  if (hasBase > 1) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.invalidValue,
      byteOffset: reader.offset - 4,
      actual: hasBase,
    );
  }
  final baseAck = hasBase == 0 ? null : _readAck(reader);
  final transferredRecordCount = reader.u32();
  final targetRecordCount = reader.u32();
  final limits = FlarkV3HostOfferLimits(
    maximumFrameCount: reader.u32(),
    maximumEncodedFrameBytes: reader.u32(),
    maximumPacketBytes: reader.u32(),
    maximumFrameBytes: reader.u32(),
    maximumProgramChildren: reader.u32(),
  );
  if (limits.maximumPacketBytes >
      FlarkV3ParserPublicationWireCodec.maximumPacketBodyBytes) {
    throw FlarkV3ParserPublicationWireFormatException(
      FlarkV3ParserPublicationWireFailure.invalidValue,
      byteOffset: reader.offset - 12,
      expected: FlarkV3ParserPublicationWireCodec.maximumPacketBodyBytes,
      actual: limits.maximumPacketBytes,
    );
  }
  return FlarkV3HostOfferBegin(
    schema: schema,
    offerId: offerId,
    publicationSession: publicationSession,
    targetHostRevision: hostRevision,
    sourceVersion: sourceVersion,
    sourceRoot: sourceRoot,
    parseGeneration: parseGeneration,
    grammarRevision: grammarRevision,
    syntaxProfile: syntaxProfile,
    authorityMask: authorityMask,
    mode: FlarkV3PublicationMode.values[modeIndex],
    baseAck: baseAck,
    transferredRecordCount: transferredRecordCount,
    targetRecordCount: targetRecordCount,
    limits: limits,
  );
}

void _writePacket(_PayloadWriter writer, FlarkV3HostPublicationPacket packet) =>
    writer.raw(packet.rawBytes);

FlarkV3HostPublicationPacket _readPacket(_PayloadReader reader) =>
    FlarkV3HostPublicationPacket.fromOwnedBytes(reader.remainder());

void _writeCommit(_PayloadWriter writer, FlarkV3HostCommitRequest request) {
  writer
    ..id128(request.offerId)
    ..u32(request.actualFrameCount)
    ..u32(request.actualEncodedFrameBytes)
    ..id128(request.rollingTransportDigest)
    ..id128(request.canonicalStreamDigest);
}

FlarkV3HostCommitRequest _readCommit(_PayloadReader reader) =>
    FlarkV3HostCommitRequest(
      offerId: reader.offerId(),
      actualFrameCount: reader.u32(),
      actualEncodedFrameBytes: reader.u32(),
      rollingTransportDigest: reader.digest128(),
      canonicalStreamDigest: reader.digest128(),
    );

void _writeAck(_PayloadWriter writer, FlarkV3StructuralAck ack) {
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

FlarkV3StructuralAck _readAck(_PayloadReader reader) => FlarkV3StructuralAck(
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
      _ => throw FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.unmappedStatus,
        byteOffset: 10,
        actual: status.code,
      ),
    };

int _positiveU32(int value, String name) {
  if (value <= 0 || value > 0xffffffff) {
    throw RangeError.range(value, 1, 0xffffffff, name);
  }
  return value;
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

  void id128(FlarkV3ProtocolId128 id) {
    u32(id.word0);
    u32(id.word1);
    u32(id.word2);
    u32(id.word3);
  }

  void raw(Uint8List value) {
    _require(value.length);
    bytes.setRange(_offset, _offset + value.length, value);
    _offset += value.length;
  }

  void finish() {
    if (_offset != bytes.length) {
      throw StateError('Publication payload size calculation diverged.');
    }
  }

  void _require(int count) {
    if (_offset + count > bytes.length) {
      throw StateError('Publication payload writer overflowed.');
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

  FlarkV3OfferId offerId() => FlarkV3OfferId(u32(), u32(), u32(), u32());

  FlarkV3PublicationSessionId publicationSessionId() =>
      FlarkV3PublicationSessionId(u32(), u32(), u32(), u32());

  FlarkV3DocumentSessionId documentSessionId() =>
      FlarkV3DocumentSessionId(u32(), u32(), u32(), u32());

  FlarkV3ProtocolDigest128 digest128() =>
      FlarkV3ProtocolDigest128(u32(), u32(), u32(), u32());

  Uint8List remainder() {
    final result = Uint8List.sublistView(bytes, offset);
    offset = bytes.length;
    return result;
  }

  void finish() {
    if (offset != bytes.length) {
      throw FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.trailingPayload,
        byteOffset: offset,
        expected: offset,
        actual: bytes.length,
      );
    }
  }

  void _require(int count) {
    if (offset + count > bytes.length) {
      throw FlarkV3ParserPublicationWireFormatException(
        FlarkV3ParserPublicationWireFailure.truncatedPayload,
        byteOffset: offset,
        expected: offset + count,
        actual: bytes.length,
      );
    }
  }
}

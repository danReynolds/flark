import '../host/host.dart';
import 'flark_v3_parser_transport.dart';

/// Receives one credited event from the sibling hot-inline publication lane.
typedef FlarkV3ParserInlineSidecarEventCallback =
    void Function(FlarkV3ParserInlineSidecarEvent event);

/// Dedicated command seam for terminal hot-inline host-poll results.
///
/// Sidecar commands remain intentionally disjoint from structural parser
/// commands even though both protocols share one endpoint event-credit cell.
abstract interface class FlarkV3ParserInlineSidecarTransport {
  /// Installs the only owner of credited hot-inline sidecar events.
  void bindInlineSidecar(FlarkV3ParserInlineSidecarEventCallback onEvent);

  void sendInlineSidecarHostPoll(
    FlarkV3ParserInlineSidecarHostPollCommand command,
  );
}

/// Sidecar poll phases have disjoint wire codes from structural publication.
enum FlarkV3ParserInlineSidecarHostPollPhase { packetCredit, commit, abort }

/// Exact cause of one sidecar host-poll sequence.
final class FlarkV3ParserInlineSidecarHostPollTicket {
  FlarkV3ParserInlineSidecarHostPollTicket({
    required this.binding,
    required this.pollTicket,
    required this.offerId,
    required this.phase,
  }) {
    _requirePositiveU32(pollTicket, 'pollTicket');
    _requireNonZeroId(offerId, 'offerId');
  }

  final FlarkV3ParserSessionBinding binding;
  final int pollTicket;
  final FlarkV3OfferId offerId;
  final FlarkV3ParserInlineSidecarHostPollPhase phase;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ParserInlineSidecarHostPollTicket &&
      other.binding == binding &&
      other.pollTicket == pollTicket &&
      other.offerId == offerId &&
      other.phase == phase;

  @override
  int get hashCode => Object.hash(binding, pollTicket, offerId, phase);
}

sealed class FlarkV3ParserInlineSidecarHostPollCommand {
  const FlarkV3ParserInlineSidecarHostPollCommand();
}

final class FlarkV3ParserInlineSidecarHostPollCompleted
    extends FlarkV3ParserInlineSidecarHostPollCommand {
  FlarkV3ParserInlineSidecarHostPollCompleted({
    required this.ticket,
    required this.outcome,
  }) {
    final valid = switch ((ticket.phase, outcome)) {
      (
        FlarkV3ParserInlineSidecarHostPollPhase.packetCredit,
        FlarkV3InlineSidecarHostPacketCredit(:final offerId),
      ) =>
        offerId == ticket.offerId,
      (
        FlarkV3ParserInlineSidecarHostPollPhase.commit,
        FlarkV3InlineSidecarHostCommitted(),
      ) =>
        true,
      (
        FlarkV3ParserInlineSidecarHostPollPhase.abort,
        FlarkV3InlineSidecarHostAbortComplete(:final offerId),
      ) =>
        offerId == ticket.offerId,
      _ => false,
    };
    if (!valid) {
      throw ArgumentError(
        'Sidecar outcome does not match its causal poll ticket.',
      );
    }
  }

  final FlarkV3ParserInlineSidecarHostPollTicket ticket;
  final FlarkV3InlineSidecarHostPollOutcome outcome;
}

final class FlarkV3ParserInlineSidecarHostPollRejected
    extends FlarkV3ParserInlineSidecarHostPollCommand {
  const FlarkV3ParserInlineSidecarHostPollRejected({
    required this.ticket,
    required this.reason,
  });

  final FlarkV3ParserInlineSidecarHostPollTicket ticket;
  final FlarkV3HostRejectReason reason;
}

/// Parser-to-host events for the sibling hot-inline publication protocol.
///
/// This is deliberately not a [FlarkV3ParserPublicationEvent], preventing a
/// sidecar ACK or offer from entering structural publication state.
sealed class FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarEvent({
    required this.eventId,
    required this.binding,
  }) {
    _requirePositiveU32(eventId, 'eventId');
  }

  final int eventId;
  final FlarkV3ParserSessionBinding binding;
}

final class FlarkV3ParserInlineSidecarBegin
    extends FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarBegin({
    required super.eventId,
    required super.binding,
    required this.begin,
  });

  final FlarkV3HotInlineSidecarOfferBegin begin;
}

/// Transfers the unchanged, bounded FPK3 packet value.
final class FlarkV3ParserInlineSidecarPacket
    extends FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarPacket({
    required super.eventId,
    required super.binding,
    required this.packet,
  });

  final FlarkV3HostPublicationPacket packet;
}

final class FlarkV3ParserInlineSidecarCommitRequested
    extends FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarCommitRequested({
    required super.eventId,
    required super.binding,
    required this.request,
  });

  final FlarkV3HotInlineSidecarCommitRequest request;
}

final class FlarkV3ParserInlineSidecarDeliveryAcknowledged
    extends FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarDeliveryAcknowledged({
    required super.eventId,
    required super.binding,
    required this.ack,
  });

  final FlarkV3InlineSidecarAck ack;
}

final class FlarkV3ParserInlineSidecarAbortRequested
    extends FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarAbortRequested({
    required super.eventId,
    required super.binding,
    required this.offerId,
  }) {
    _requireNonZeroId(offerId, 'offerId');
  }

  final FlarkV3OfferId offerId;
}

final class FlarkV3ParserInlineSidecarFailed
    extends FlarkV3ParserInlineSidecarEvent {
  FlarkV3ParserInlineSidecarFailed({
    required super.eventId,
    required super.binding,
    required this.offerId,
    required this.failureCode,
  }) {
    _requireNonZeroId(offerId, 'offerId');
    if (failureCode < 0 || failureCode > flarkV3TransportV1Maximum) {
      throw RangeError.range(
        failureCode,
        0,
        flarkV3TransportV1Maximum,
        'failureCode',
      );
    }
  }

  final FlarkV3OfferId offerId;
  final int failureCode;
}

void _requirePositiveU32(int value, String name) {
  if (value <= 0 || value > flarkV3TransportV1Maximum) {
    throw RangeError.range(value, 1, flarkV3TransportV1Maximum, name);
  }
}

void _requireNonZeroId(FlarkV3ProtocolId128 id, String name) {
  if (id.word0 == 0 && id.word1 == 0 && id.word2 == 0 && id.word3 == 0) {
    throw ArgumentError.value(id, name, 'Protocol identity must be non-zero.');
  }
}

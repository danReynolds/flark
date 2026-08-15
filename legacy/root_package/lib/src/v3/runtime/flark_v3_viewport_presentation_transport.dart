import '../host/host.dart';
import 'flark_v3_parser_transport.dart';

typedef FlarkV3ParserViewportPresentationEventCallback =
    void Function(FlarkV3ParserViewportPresentationEvent event);

/// Dedicated command seam for terminal VPB1 host-poll results.
abstract interface class FlarkV3ParserViewportPresentationTransport {
  void bindViewportPresentation(
    FlarkV3ParserViewportPresentationEventCallback onEvent,
  );

  void sendViewportPresentationHostPoll(
    FlarkV3ParserViewportPresentationHostPollCommand command,
  );
}

enum FlarkV3ParserViewportPresentationHostPollPhase {
  packetCredit,
  commit,
  abort,
}

final class FlarkV3ParserViewportPresentationHostPollTicket {
  FlarkV3ParserViewportPresentationHostPollTicket({
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
  final FlarkV3ParserViewportPresentationHostPollPhase phase;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ParserViewportPresentationHostPollTicket &&
      other.binding == binding &&
      other.pollTicket == pollTicket &&
      other.offerId == offerId &&
      other.phase == phase;

  @override
  int get hashCode => Object.hash(binding, pollTicket, offerId, phase);
}

sealed class FlarkV3ParserViewportPresentationHostPollCommand {
  const FlarkV3ParserViewportPresentationHostPollCommand();
}

final class FlarkV3ParserViewportPresentationHostPollCompleted
    extends FlarkV3ParserViewportPresentationHostPollCommand {
  FlarkV3ParserViewportPresentationHostPollCompleted({
    required this.ticket,
    required this.outcome,
  }) {
    final valid = switch ((ticket.phase, outcome)) {
      (
        FlarkV3ParserViewportPresentationHostPollPhase.packetCredit,
        FlarkV3ViewportPresentationHostPacketCredit(:final offerId),
      ) =>
        offerId == ticket.offerId,
      (
        FlarkV3ParserViewportPresentationHostPollPhase.commit,
        FlarkV3ViewportPresentationHostCommitted(),
      ) =>
        true,
      (
        FlarkV3ParserViewportPresentationHostPollPhase.abort,
        FlarkV3ViewportPresentationHostAbortComplete(:final offerId),
      ) =>
        offerId == ticket.offerId,
      _ => false,
    };
    if (!valid) {
      throw ArgumentError(
        'Viewport outcome does not match its causal poll ticket.',
      );
    }
  }

  final FlarkV3ParserViewportPresentationHostPollTicket ticket;
  final FlarkV3ViewportPresentationHostPollOutcome outcome;
}

final class FlarkV3ParserViewportPresentationHostPollRejected
    extends FlarkV3ParserViewportPresentationHostPollCommand {
  const FlarkV3ParserViewportPresentationHostPollRejected({
    required this.ticket,
    required this.reason,
  });

  final FlarkV3ParserViewportPresentationHostPollTicket ticket;
  final FlarkV3HostRejectReason reason;
}

sealed class FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationEvent({
    required this.eventId,
    required this.binding,
  }) {
    _requirePositiveU32(eventId, 'eventId');
  }

  final int eventId;
  final FlarkV3ParserSessionBinding binding;
}

final class FlarkV3ParserViewportPresentationBegin
    extends FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationBegin({
    required super.eventId,
    required super.binding,
    required this.begin,
  });

  final FlarkV3ViewportPresentationOfferBegin begin;
}

final class FlarkV3ParserViewportPresentationPacket
    extends FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationPacket({
    required super.eventId,
    required super.binding,
    required this.packet,
  });

  final FlarkV3HostPublicationPacket packet;
}

final class FlarkV3ParserViewportPresentationCommitRequested
    extends FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationCommitRequested({
    required super.eventId,
    required super.binding,
    required this.request,
  });

  final FlarkV3ViewportPresentationCommitRequest request;
}

final class FlarkV3ParserViewportPresentationDeliveryAcknowledged
    extends FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationDeliveryAcknowledged({
    required super.eventId,
    required super.binding,
    required this.ack,
  });

  final FlarkV3ViewportPresentationAck ack;
}

final class FlarkV3ParserViewportPresentationAbortRequested
    extends FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationAbortRequested({
    required super.eventId,
    required super.binding,
    required this.offerId,
  }) {
    _requireNonZeroId(offerId, 'offerId');
  }

  final FlarkV3OfferId offerId;
}

final class FlarkV3ParserViewportPresentationFailed
    extends FlarkV3ParserViewportPresentationEvent {
  FlarkV3ParserViewportPresentationFailed({
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

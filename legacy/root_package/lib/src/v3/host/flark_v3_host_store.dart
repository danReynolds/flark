import 'flark_v3_host_protocol.dart';

/// ABI-neutral rejection classes returned by the shared Rust host store.
///
/// Expected races such as a stale offer or one-credit backpressure are values,
/// not exceptions on the UI isolate.
enum FlarkV3HostRejectReason {
  invalid,
  backpressure,
  staleSource,
  exactSourceMismatch,
  sessionSnapshotRequired,
  baseMismatch,
  wrongOffer,
  corruptPublication,
  queryBoundExceeded,
  foregroundBoundExceeded,
  superseded,
  closed,
}

final class FlarkV3HostRejection {
  const FlarkV3HostRejection(this.reason, this.message);

  final FlarkV3HostRejectReason reason;
  final String message;
}

sealed class FlarkV3HostCallResult<T> {
  const FlarkV3HostCallResult();
}

final class FlarkV3HostAccepted<T> extends FlarkV3HostCallResult<T> {
  const FlarkV3HostAccepted(this.value);

  final T value;
}

final class FlarkV3HostRejected<T> extends FlarkV3HostCallResult<T> {
  const FlarkV3HostRejected(this.rejection);

  final FlarkV3HostRejection rejection;
}

enum FlarkV3HostUnit { accepted }

sealed class FlarkV3HostPollOutcome {
  const FlarkV3HostPollOutcome();
}

final class FlarkV3HostPollPending extends FlarkV3HostPollOutcome {
  const FlarkV3HostPollPending();
}

/// The prior packet is fully adopted and the worker has one transfer credit.
final class FlarkV3HostPacketCredit extends FlarkV3HostPollOutcome {
  FlarkV3HostPacketCredit({
    required this.offerId,
    required this.nextFrameOrdinal,
  }) {
    if (nextFrameOrdinal <= 0 || nextFrameOrdinal > flarkV3TransportV1Maximum) {
      throw RangeError.range(
        nextFrameOrdinal,
        1,
        flarkV3TransportV1Maximum,
        'nextFrameOrdinal',
      );
    }
  }

  final FlarkV3OfferId offerId;
  final int nextFrameOrdinal;
}

/// One complete exact-current target became atomically queryable.
final class FlarkV3HostCommitted extends FlarkV3HostPollOutcome {
  const FlarkV3HostCommitted(this.ack);

  final FlarkV3StructuralAck ack;
}

final class FlarkV3HostAbortComplete extends FlarkV3HostPollOutcome {
  const FlarkV3HostAbortComplete(this.offerId);

  final FlarkV3OfferId offerId;
}

/// Confirms that all host roots and staged work have drained after
/// [FlarkV3HostStore.close].
///
/// `close()` only begins bounded retirement. Callers must continue granting
/// poll fuel until this terminal outcome before releasing the host adapter.
final class FlarkV3HostClosed extends FlarkV3HostPollOutcome {
  const FlarkV3HostClosed();
}

sealed class FlarkV3HostStoreQueryOutcome {
  const FlarkV3HostStoreQueryOutcome();
}

final class FlarkV3HostStoreStructuralQuery
    extends FlarkV3HostStoreQueryOutcome {
  const FlarkV3HostStoreStructuralQuery(this.viewport);

  final FlarkV3HostStructuralViewport viewport;
}

final class FlarkV3HostStoreSourceGapQuery
    extends FlarkV3HostStoreQueryOutcome {
  const FlarkV3HostStoreSourceGapQuery(this.gap);

  final FlarkV3HostLocalSourceGap gap;
}

sealed class FlarkV3HostStoreBlockRangeQueryOutcome {
  const FlarkV3HostStoreBlockRangeQueryOutcome();
}

final class FlarkV3HostStoreStructuralBlockRangeQuery
    extends FlarkV3HostStoreBlockRangeQueryOutcome {
  const FlarkV3HostStoreStructuralBlockRangeQuery(this.range);

  final FlarkV3HostStructuralBlockRange range;
}

final class FlarkV3HostStoreBlockRangeSourceGapQuery
    extends FlarkV3HostStoreBlockRangeQueryOutcome {
  const FlarkV3HostStoreBlockRangeSourceGapQuery(this.gap);

  final FlarkV3HostBlockRangeSourceGap gap;
}

/// Persistent host-store boundary implemented once in Rust.
///
/// Native uses an FFI handle owned by the UI isolate. Web uses a distinct
/// main-context WebAssembly instance. Parser-worker roots and the host-store
/// root never share addresses; publication crosses the boundary only as one
/// credited, bounded, record-aligned packet.
abstract interface class FlarkV3HostStore {
  /// Advances exact source authority immediately, superseding an in-flight
  /// staging offer inside the store without withdrawing the installed root.
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  );

  /// Consumes [FlarkV3HostPublicationPacket.rawBytes] synchronously. The
  /// adapter must not retain the caller's Dart view after this call returns.
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId);

  /// Advances validation, sequence construction, commit preparation, or
  /// fuelled retirement. No call may exceed the supplied independent grants.
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  );

  /// Confirms that the worker received the installed publication ACK.
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  );

  /// Copies only a bounded viewport closure out of the persistent Rust store.
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  );

  /// Begins bounded arena/root retirement. Implementations must not recursively
  /// destroy a document-sized object graph in this call.
  FlarkV3HostCallResult<FlarkV3HostUnit> close();
}

/// Additive structure-only range-query lane.
///
/// Keeping this separate mirrors the independently optional inline-sidecar
/// lane and lets non-range test stores remain deliberately narrow.
abstract interface class FlarkV3BlockRangeHostStore {
  FlarkV3HostCallResult<FlarkV3HostStoreBlockRangeQueryOutcome>
  queryStructuralRange(FlarkV3HostBlockRangeQuery query);
}

/// Additive ordinal-to-source locator lane.
///
/// The result contains only authenticated sequence counts and source cuts.
/// No structural records or document-sized payload cross this boundary.
abstract interface class FlarkV3StructuralOrdinalWindowHostStore {
  FlarkV3HostCallResult<FlarkV3HostStructuralOrdinalWindowOutcome>
  queryStructuralOrdinalWindow(FlarkV3HostStructuralOrdinalWindowQuery query);
}

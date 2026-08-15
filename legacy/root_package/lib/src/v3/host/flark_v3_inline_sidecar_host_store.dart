import 'flark_v3_host_protocol.dart';
import 'flark_v3_host_store.dart';
import 'flark_v3_hot_inline_sidecar_protocol.dart';

/// Native-capable host seam for the sibling hot-inline publication lifecycle.
///
/// Implementations must keep these operations separate from structural offer
/// state even though both protocols share one generation-checked host handle,
/// one FPK3 packet value, and the same bounded work-grant shape.
abstract interface class FlarkV3InlineSidecarHostStore {
  FlarkV3HostCallResult<FlarkV3HostUnit> beginInlineSidecarOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> admitInlineSidecarPacket(
    FlarkV3HostPublicationPacket packet,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> requestInlineSidecarCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> abortInlineSidecarOffer(
    FlarkV3OfferId offerId,
  );

  FlarkV3HostCallResult<FlarkV3InlineSidecarHostPollOutcome> pollInlineSidecar(
    FlarkV3HostWorkGrant grant,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeInlineSidecarDelivery(
    FlarkV3InlineSidecarAck ack,
  );

  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> queryInlineSidecar(
    FlarkV3InlineSidecarQuery query,
  );
}

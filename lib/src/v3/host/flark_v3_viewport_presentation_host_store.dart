import 'flark_v3_host_protocol.dart';
import 'flark_v3_host_store.dart';
import 'flark_v3_viewport_presentation_protocol.dart';

/// Additive host capability for authenticated aggregate viewport pages.
///
/// Implementations keep VPB1 state separate from structural offers and HIO1
/// sidecars. One query returns one owned aggregate page; Dart performs any
/// subsequent leaf splitting and caching without per-leaf host calls.
abstract interface class FlarkV3ViewportPresentationHostStore {
  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  );

  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  );

  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant);

  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack);

  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query);
}

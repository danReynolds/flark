import 'flark_v3_host_protocol.dart';
import 'flark_v3_host_store.dart';
import 'flark_v3_viewport_presentation_host_store.dart';
import 'flark_v3_viewport_presentation_protocol.dart';

/// Dart owner of the sibling VPB1 host lifecycle.
///
/// The controller validates exact structural authority and publication
/// causality. Aggregate-page query decoding remains a later consumer concern.
final class FlarkV3ViewportPresentationController {
  FlarkV3ViewportPresentationController.attach({
    required FlarkV3SourceVersion currentSource,
    required FlarkV3HostStore store,
  }) : _currentSource = currentSource,
       _store = store is FlarkV3ViewportPresentationHostStore
           ? store as FlarkV3ViewportPresentationHostStore
           : null;

  final FlarkV3ViewportPresentationHostStore? _store;
  FlarkV3SourceVersion _currentSource;
  FlarkV3StructuralAck? _structuralAck;
  FlarkV3ViewportPresentationOfferBegin? _activeOffer;
  FlarkV3ViewportPresentationCommitRequest? _activeCommit;
  FlarkV3ViewportPresentationAck? _pendingDeliveryAck;
  FlarkV3ViewportPresentationAck? _installedAck;
  bool _abortRequested = false;
  bool _closed = false;

  bool get supported => _store != null;
  FlarkV3ViewportPresentationAck? get pendingDeliveryAck => _pendingDeliveryAck;
  FlarkV3ViewportPresentationAck? get installedAck => _installedAck;

  void invalidateForUiSourceAdvance() {
    _suppressActiveOffer();
    _structuralAck = null;
    _pendingDeliveryAck = null;
    _installedAck = null;
  }

  void observeSourceAdvance(FlarkV3SourceVersion source) {
    if (source == _currentSource) return;
    _suppressActiveOffer();
    _currentSource = source;
    _structuralAck = null;
    _pendingDeliveryAck = null;
    _installedAck = null;
  }

  void suppressParserOffer() => _suppressActiveOffer();

  void adoptStructuralAck(FlarkV3StructuralAck ack) {
    if (ack.sourceVersion != _currentSource) {
      _structuralAck = null;
      _pendingDeliveryAck = null;
      _installedAck = null;
      _clearActive();
      return;
    }
    if (_structuralAck != ack) {
      _pendingDeliveryAck = null;
      _installedAck = null;
      _clearActive();
    }
    _structuralAck = ack;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    final base = _structuralAck;
    if (base == null ||
        base.sourceVersion != _currentSource ||
        begin.baseAck != base) {
      return _rejected(
        FlarkV3HostRejectReason.baseMismatch,
        'Viewport offer does not bind the exact current structural ACK.',
      );
    }
    if (_activeOffer != null ||
        _pendingDeliveryAck != null ||
        _abortRequested) {
      return _rejected(
        FlarkV3HostRejectReason.backpressure,
        'Another viewport offer or delivery owns host work.',
      );
    }
    final installed = _installedAck;
    if (installed != null &&
        begin.binding.viewportGeneration <=
            installed.binding.viewportGeneration) {
      return _rejected(
        FlarkV3HostRejectReason.baseMismatch,
        'Viewport generation must strictly advance.',
      );
    }
    final result = _store!.beginViewportPresentationOffer(begin);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _activeOffer = begin;
      _activeCommit = null;
      _abortRequested = false;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (!_activeBindsCurrentAuthority() ||
        _activeOffer?.offerId != packet.offerId ||
        _abortRequested) {
      return _rejected(
        FlarkV3HostRejectReason.wrongOffer,
        'Viewport packet does not belong to the active exact offer.',
      );
    }
    return _store!.admitViewportPresentationPacket(packet);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    final active = _activeOffer;
    if (!_activeBindsCurrentAuthority() ||
        active?.offerId != request.offerId ||
        _abortRequested) {
      return _rejected(
        FlarkV3HostRejectReason.wrongOffer,
        'Viewport commit does not name the active exact offer.',
      );
    }
    try {
      request.requireLimits(active!.limits);
    } on ArgumentError {
      return _rejected(
        FlarkV3HostRejectReason.corruptPublication,
        'Viewport commit exceeds its admitted transport limits.',
      );
    }
    final result = _store!.requestViewportPresentationCommit(request);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _activeCommit = request;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (_activeOffer?.offerId != offerId || _abortRequested) {
      return _rejected(
        FlarkV3HostRejectReason.wrongOffer,
        'Viewport abort does not name the active offer.',
      );
    }
    final result = _store!.abortViewportPresentationOffer(offerId);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _abortRequested = true;
      _activeCommit = null;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final unavailable =
        _availabilityRejection<FlarkV3ViewportPresentationHostPollOutcome>();
    if (unavailable != null) return unavailable;
    final result = _store!.pollViewportPresentation(grant);
    if (result
        case FlarkV3HostAccepted<FlarkV3ViewportPresentationHostPollOutcome>(
          value: final outcome,
        )) {
      switch (outcome) {
        case FlarkV3ViewportPresentationHostPacketCredit(:final offerId):
          if (!_activeBindsCurrentAuthority() ||
              _activeOffer?.offerId != offerId ||
              _abortRequested) {
            return _rejected(
              FlarkV3HostRejectReason.superseded,
              'Viewport packet credit belongs to stale parser work.',
            );
          }
        case FlarkV3ViewportPresentationHostCommitted(:final ack):
          final offer = _activeOffer;
          final commit = _activeCommit;
          if (offer == null ||
              commit == null ||
              !_activeBindsCurrentAuthority() ||
              !_ackMatchesOffer(ack, offer, commit)) {
            _clearActive();
            return _rejected(
              FlarkV3HostRejectReason.invalid,
              'Viewport ACK escaped its exact active offer.',
            );
          }
          _installedAck = ack;
          _pendingDeliveryAck = ack;
          _clearActive();
        case FlarkV3ViewportPresentationHostAbortComplete(:final offerId):
          final active = _activeOffer;
          if (active != null && active.offerId != offerId) {
            return _rejected(
              FlarkV3HostRejectReason.invalid,
              'Viewport abort completion names another active offer.',
            );
          }
          _clearActive();
        case FlarkV3ViewportPresentationHostClosed():
          closeLocally();
        case FlarkV3ViewportPresentationHostPollPending():
          break;
      }
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3ViewportPresentationAck ack,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (_pendingDeliveryAck != ack ||
        _structuralAck != ack.baseAck ||
        ack.baseAck.sourceVersion != _currentSource) {
      return _rejected(
        FlarkV3HostRejectReason.invalid,
        'Viewport ACK does not match the pending exact delivery.',
      );
    }
    final result = _store!.acknowledgeViewportPresentationDelivery(ack);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _pendingDeliveryAck = null;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome> query(
    FlarkV3ViewportPresentationQuery query,
  ) {
    final unavailable =
        _availabilityRejection<FlarkV3ViewportPresentationQueryOutcome>();
    if (unavailable != null) return unavailable;
    final installed = _installedAck;
    final structural = _structuralAck;
    if (installed == null ||
        query.ack != installed ||
        structural == null ||
        installed.baseAck != structural ||
        structural.sourceVersion != _currentSource) {
      return const FlarkV3HostAccepted(
        FlarkV3ViewportPresentationQueryUnavailable(),
      );
    }
    return _store!.queryViewportPresentation(query);
  }

  void closeLocally() {
    _closed = true;
    _structuralAck = null;
    _pendingDeliveryAck = null;
    _installedAck = null;
    _clearActive();
  }

  bool _activeBindsCurrentAuthority() {
    final active = _activeOffer;
    final base = _structuralAck;
    return active != null &&
        base != null &&
        active.baseAck == base &&
        base.sourceVersion == _currentSource;
  }

  void _suppressActiveOffer() {
    final active = _activeOffer;
    if (active == null) return;
    _clearActive();
    _store?.abortViewportPresentationOffer(active.offerId);
  }

  void _clearActive() {
    _activeOffer = null;
    _activeCommit = null;
    _abortRequested = false;
  }

  FlarkV3HostRejected<T>? _availabilityRejection<T>() {
    final store = _store;
    if (_closed || store == null) {
      return _rejected(
        FlarkV3HostRejectReason.closed,
        store == null
            ? 'Host store does not provide viewport capability.'
            : 'Viewport controller is closed.',
      );
    }
    return null;
  }
}

bool _ackMatchesOffer(
  FlarkV3ViewportPresentationAck ack,
  FlarkV3ViewportPresentationOfferBegin offer,
  FlarkV3ViewportPresentationCommitRequest commit,
) =>
    ack.publicationSession == offer.publicationSession &&
    ack.baseAck == offer.baseAck &&
    ack.binding == offer.binding &&
    ack.envelope == offer.envelope &&
    ack.actualFrameCount == commit.actualFrameCount &&
    ack.actualEncodedFrameBytes == commit.actualEncodedFrameBytes &&
    ack.aggregateRootStreamDigest == commit.aggregateRootStreamDigest;

FlarkV3HostRejected<T> _rejected<T>(
  FlarkV3HostRejectReason reason,
  String message,
) => FlarkV3HostRejected(FlarkV3HostRejection(reason, message));

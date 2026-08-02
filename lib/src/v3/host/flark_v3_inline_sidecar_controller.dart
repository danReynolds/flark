import 'flark_v3_host_protocol.dart';
import 'flark_v3_host_store.dart';
import 'flark_v3_hot_inline_sidecar_protocol.dart';
import 'flark_v3_inline_sidecar_host_store.dart';

/// Dart owner of the sibling hot-inline host lifecycle.
///
/// Structural presentation authority deliberately stays in
/// `FlarkV3HostController`. This controller remembers only the exact
/// structural base that may admit a sidecar, one active sidecar offer, and one
/// pending sidecar delivery ACK.
final class FlarkV3InlineSidecarController {
  FlarkV3InlineSidecarController.attach({
    required FlarkV3SourceVersion currentSource,
    required FlarkV3HostStore store,
  }) : _currentSource = currentSource,
       _store = store is FlarkV3InlineSidecarHostStore
           ? store as FlarkV3InlineSidecarHostStore
           : null;

  final FlarkV3InlineSidecarHostStore? _store;
  FlarkV3SourceVersion _currentSource;
  FlarkV3StructuralAck? _structuralAck;
  FlarkV3HotInlineSidecarOfferBegin? _activeOffer;
  FlarkV3HotInlineSidecarCommitRequest? _activeCommit;
  FlarkV3InlineSidecarAck? _pendingDeliveryAck;
  _InstalledInlineSidecar? _installed;
  bool _abortRequested = false;
  bool _closed = false;

  bool get supported => _store != null;
  FlarkV3InlineSidecarAck? get pendingDeliveryAck => _pendingDeliveryAck;
  FlarkV3InlineSidecarAck? get installedAck => _installed?.ack;
  FlarkV3HotInlineSidecarBinding? get installedBinding => _installed?.binding;
  FlarkV3OfferId? get activeOfferId => _activeOffer?.offerId;

  /// Suppresses stale parser work before a newer UI source becomes visible.
  ///
  /// The structural host may still retain its prior root for paint, but that
  /// root and every sibling sidecar cease to be exact-current authority. The
  /// next certified structural source observation atomically supersedes any
  /// retained host-side staging.
  void invalidateForUiSourceAdvance() {
    _suppressActiveOffer();
    _structuralAck = null;
    _pendingDeliveryAck = null;
    _installed = null;
  }

  /// Advances the certified source boundary before structural store adoption.
  void observeSourceAdvance(FlarkV3SourceVersion source) {
    if (source == _currentSource) return;
    _suppressActiveOffer();
    _currentSource = source;
    _structuralAck = null;
    _pendingDeliveryAck = null;
    _installed = null;
  }

  /// Invalidates work owned by a parser replica without withdrawing an
  /// already installed exact sidecar.
  void suppressParserOffer() {
    _suppressActiveOffer();
  }

  /// Adopts the structural ACK already validated by the structural controller.
  ///
  /// A new ACK invalidates the sibling generation locally. The native host
  /// performs the corresponding fuelled root retirement independently.
  void adoptStructuralAck(FlarkV3StructuralAck ack) {
    if (ack.sourceVersion != _currentSource) {
      _structuralAck = null;
      _pendingDeliveryAck = null;
      _installed = null;
      _clearActive();
      return;
    }
    if (_structuralAck != ack) {
      _pendingDeliveryAck = null;
      _installed = null;
      _clearActive();
    }
    _structuralAck = ack;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    final capability = _store!;
    final base = _structuralAck;
    if (base == null ||
        base.sourceVersion != _currentSource ||
        begin.baseAck != base) {
      return _rejected(
        FlarkV3HostRejectReason.baseMismatch,
        'Hot-inline offer does not bind the exact current structural ACK.',
      );
    }
    if (_activeOffer != null ||
        _pendingDeliveryAck != null ||
        _abortRequested) {
      return _rejected(
        FlarkV3HostRejectReason.backpressure,
        'Another sidecar offer or delivery still owns host work.',
      );
    }
    final installed = _installed;
    if (installed != null &&
        begin.binding.refinementGeneration.compareTo(
              installed.ack.refinementGeneration,
            ) <=
            0) {
      return _rejected(
        FlarkV3HostRejectReason.baseMismatch,
        'Hot-inline refinement generation must strictly advance.',
      );
    }
    final result = capability.beginInlineSidecarOffer(begin);
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
        'Sidecar packet does not belong to the active exact offer.',
      );
    }
    return _store!.admitInlineSidecarPacket(packet);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (!_activeBindsCurrentAuthority() ||
        _activeOffer?.offerId != request.offerId ||
        _abortRequested) {
      return _rejected(
        FlarkV3HostRejectReason.wrongOffer,
        'Sidecar commit does not name the active exact offer.',
      );
    }
    final result = _store!.requestInlineSidecarCommit(request);
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
        'Sidecar abort does not name the active offer.',
      );
    }
    final result = _store!.abortInlineSidecarOffer(offerId);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _abortRequested = true;
      _activeCommit = null;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3InlineSidecarHostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final unavailable =
        _availabilityRejection<FlarkV3InlineSidecarHostPollOutcome>();
    if (unavailable != null) return unavailable;
    final result = _store!.pollInlineSidecar(grant);
    if (result case FlarkV3HostAccepted<FlarkV3InlineSidecarHostPollOutcome>(
      value: final outcome,
    )) {
      switch (outcome) {
        case FlarkV3InlineSidecarHostPacketCredit(:final offerId):
          if (!_activeBindsCurrentAuthority() ||
              _activeOffer?.offerId != offerId ||
              _abortRequested) {
            return _rejected(
              FlarkV3HostRejectReason.superseded,
              'Sidecar packet credit belongs to stale parser work.',
            );
          }
        case FlarkV3InlineSidecarHostCommitted(:final ack):
          final offer = _activeOffer;
          final commit = _activeCommit;
          if (offer == null ||
              commit == null ||
              !_activeBindsCurrentAuthority() ||
              !_ackMatchesOffer(ack, offer, commit)) {
            _clearActive();
            return _rejected(
              FlarkV3HostRejectReason.invalid,
              'Sidecar ACK escaped its exact active offer.',
            );
          }
          _installed = _InstalledInlineSidecar(
            binding: offer.binding,
            ack: ack,
          );
          _pendingDeliveryAck = ack;
          _clearActive();
        case FlarkV3InlineSidecarHostAbortComplete(:final offerId):
          final active = _activeOffer;
          if (active != null && active.offerId != offerId) {
            return _rejected(
              FlarkV3HostRejectReason.invalid,
              'Sidecar abort completion names another active offer.',
            );
          }
          _clearActive();
        case FlarkV3InlineSidecarHostClosed():
          closeLocally();
        case FlarkV3InlineSidecarHostPollPending():
          break;
      }
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3InlineSidecarAck ack,
  ) {
    final unavailable = _availabilityRejection<FlarkV3HostUnit>();
    if (unavailable != null) return unavailable;
    if (_pendingDeliveryAck != ack ||
        _structuralAck != ack.baseAck ||
        ack.baseAck.sourceVersion != _currentSource) {
      return _rejected(
        FlarkV3HostRejectReason.invalid,
        'Sidecar ACK does not match the pending exact delivery.',
      );
    }
    final result = _store!.acknowledgeInlineSidecarDelivery(ack);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _pendingDeliveryAck = null;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> query(
    FlarkV3InlineSidecarQuery query,
  ) {
    final unavailable =
        _availabilityRejection<FlarkV3InlineSidecarQueryOutcome>();
    if (unavailable != null) return unavailable;
    final installed = _installed;
    if (installed == null ||
        installed.binding != query.binding ||
        installed.ack.baseAck != _structuralAck ||
        installed.ack.baseAck.sourceVersion != _currentSource) {
      return _rejected(
        FlarkV3HostRejectReason.baseMismatch,
        'Sidecar query does not bind the installed exact generation.',
      );
    }
    final result = _store!.queryInlineSidecar(query);
    if (result case FlarkV3HostAccepted<FlarkV3InlineSidecarQueryOutcome>(
      value: final outcome,
    )) {
      final encodedBytes = switch (outcome) {
        FlarkV3InlineSidecarQueryAuthoritative(:final encodedByteLength) =>
          encodedByteLength,
        FlarkV3InlineSidecarQueryUnsupported(:final metadata) =>
          metadata.length,
        FlarkV3InlineSidecarQueryUnavailable() => 0,
      };
      if (encodedBytes > query.maximumEncodedBytes) {
        return _rejected(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Sidecar query exceeded its caller-owned output bound.',
        );
      }
    }
    return result;
  }

  void closeLocally() {
    _closed = true;
    _structuralAck = null;
    _pendingDeliveryAck = null;
    _installed = null;
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
    _store?.abortInlineSidecarOffer(active.offerId);
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
            ? 'Host store does not provide hot-inline sidecar capability.'
            : 'Hot-inline sidecar controller is closed.',
      );
    }
    return null;
  }
}

final class _InstalledInlineSidecar {
  const _InstalledInlineSidecar({required this.binding, required this.ack});

  final FlarkV3HotInlineSidecarBinding binding;
  final FlarkV3InlineSidecarAck ack;
}

bool _ackMatchesOffer(
  FlarkV3InlineSidecarAck ack,
  FlarkV3HotInlineSidecarOfferBegin offer,
  FlarkV3HotInlineSidecarCommitRequest commit,
) {
  final expectedDisposition = switch (offer.envelope.disposition) {
    FlarkV3HotInlineSidecarAuthoritative() =>
      FlarkV3InlineSidecarAckDisposition.authoritative,
    FlarkV3HotInlineSidecarUnsupported() =>
      FlarkV3InlineSidecarAckDisposition.unsupported,
  };
  return ack.publicationSession == offer.publicationSession &&
      ack.baseAck == offer.baseAck &&
      ack.refinementGeneration == offer.binding.refinementGeneration &&
      ack.blockOrdinal == offer.binding.blockOrdinal &&
      ack.transferredNodeCount == offer.envelope.transferredNodeCount &&
      ack.disposition == expectedDisposition &&
      ack.hio1EnvelopeDigest256 == offer.envelope.hio1EnvelopeDigest256 &&
      ack.rootStreamDigest == commit.rootStreamDigest;
}

FlarkV3HostRejected<T> _rejected<T>(
  FlarkV3HostRejectReason reason,
  String message,
) => FlarkV3HostRejected(FlarkV3HostRejection(reason, message));

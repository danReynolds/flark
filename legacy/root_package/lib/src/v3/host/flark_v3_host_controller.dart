import 'flark_v3_host_protocol.dart';
import 'flark_v3_host_store.dart';

sealed class FlarkV3HostPresentationState {
  const FlarkV3HostPresentationState({
    required this.uiSource,
    required this.sourceVersion,
  });

  final FlarkV3UiSourceIdentity uiSource;

  /// Last certified metric/hash authority known to the host store.
  ///
  /// This may intentionally trail [uiSource] while exact UTF-16 editing stays
  /// live and background source certification is pending.
  final FlarkV3SourceVersion sourceVersion;

  bool get uiSourceCertified => uiSource.bindsCertified(sourceVersion);
}

final class FlarkV3ExactStructuralPresentation
    extends FlarkV3HostPresentationState {
  const FlarkV3ExactStructuralPresentation({
    required super.uiSource,
    required super.sourceVersion,
    required this.ack,
  });

  final FlarkV3StructuralAck ack;
}

enum FlarkV3StablePendingReason {
  initialSnapshot,
  sourceUncertified,
  sourceAdvanced,
  storeUnsynchronized,
  publicationPending,
}

/// Honest fallback authority while exact-current structure is unavailable.
///
/// A separate renderer cache may continue painting [stablePaintAck], but it is
/// explicitly non-authoritative. Exact source input, selection, caret, and IME
/// remain owned by the source/editor path.
final class FlarkV3StablePendingPresentation
    extends FlarkV3HostPresentationState {
  const FlarkV3StablePendingPresentation({
    required super.uiSource,
    required super.sourceVersion,
    required this.uiSourceGap,
    required this.sourceGap,
    required this.stablePaintAck,
    required this.reason,
  });

  /// Exact editable UI range. This remains truthful without byte/hash facts.
  final FlarkV3UiSourceGap uiSourceGap;

  /// Certified structural gap, absent while [uiSource] is ahead of the host.
  final FlarkV3SourceGap? sourceGap;
  final FlarkV3StructuralAck? stablePaintAck;
  final FlarkV3StablePendingReason reason;
}

final class FlarkV3UiSourceGap {
  FlarkV3UiSourceGap.whole(this.uiSource)
    : startUtf16 = 0,
      endUtf16 = uiSource.utf16Length;

  final FlarkV3UiSourceIdentity uiSource;
  final int startUtf16;
  final int endUtf16;

  bool get sourceEditable => true;
  bool get semanticActionsValid => false;
  bool get accessibilitySemanticsValid => false;
  bool get markdownHitTargetsValid => false;
  bool get semanticSelectionMapValid => false;
}

final class FlarkV3SourceGap {
  const FlarkV3SourceGap({
    required this.sourceVersion,
    required this.range,
    this.structuralReason,
    this.structuralReceipt,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3MetricRange range;
  final FlarkV3HostSourceGapReason? structuralReason;
  final FlarkV3HostViewportReceipt? structuralReceipt;

  bool get sourceEditable => true;
  bool get semanticActionsValid => false;
  bool get accessibilitySemanticsValid => false;
  bool get markdownHitTargetsValid => false;
  bool get semanticSelectionMapValid => false;
}

sealed class FlarkV3HostPresentationQuery {
  const FlarkV3HostPresentationQuery();
}

final class FlarkV3StructuralPresentationQuery
    extends FlarkV3HostPresentationQuery {
  const FlarkV3StructuralPresentationQuery(this.viewport);

  final FlarkV3HostStructuralViewport viewport;
}

final class FlarkV3SourceGapPresentationQuery
    extends FlarkV3HostPresentationQuery {
  const FlarkV3SourceGapPresentationQuery(this.gap);

  final FlarkV3SourceGap gap;
}

sealed class FlarkV3HostBlockRangePresentationQuery {
  const FlarkV3HostBlockRangePresentationQuery();
}

final class FlarkV3StructuralBlockRangePresentationQuery
    extends FlarkV3HostBlockRangePresentationQuery {
  const FlarkV3StructuralBlockRangePresentationQuery(this.range);

  final FlarkV3HostStructuralBlockRange range;
}

final class FlarkV3BlockRangeSourceGapPresentationQuery
    extends FlarkV3HostBlockRangePresentationQuery {
  const FlarkV3BlockRangeSourceGapPresentationQuery(this.gap);

  final FlarkV3HostBlockRangeSourceGap gap;
}

final class FlarkV3HostAttachment {
  const FlarkV3HostAttachment({
    required this.controller,
    required this.storeRejection,
  });

  final FlarkV3HostController controller;
  final FlarkV3HostRejection? storeRejection;

  bool get storeSynchronized => storeRejection == null;
}

/// Exact source adoption completes before the optional host-store sync.
final class FlarkV3SourceAdoptionReceipt {
  const FlarkV3SourceAdoptionReceipt({
    required this.sourceVersion,
    required this.storeRejection,
  });

  final FlarkV3SourceVersion sourceVersion;
  final FlarkV3HostRejection? storeRejection;

  bool get sourceAccepted => true;
  bool get storeSynchronized => storeRejection == null;
}

/// UI source advanced before a certified byte/hash version was available.
final class FlarkV3UiSourceAdvanceReceipt {
  const FlarkV3UiSourceAdvanceReceipt({
    required this.uiSource,
    required this.activeOfferAbort,
  });

  final FlarkV3UiSourceIdentity uiSource;
  final FlarkV3HostCallResult<FlarkV3HostUnit>? activeOfferAbort;

  bool get sourceCertified => false;
  bool get semanticsSuppressed => true;
}

/// UI-isolate authority controller for the shared Rust host store.
///
/// This class intentionally contains no green decoder, measured tree, splice
/// implementation, or Markdown grammar. It coordinates exact source identity,
/// stable-pending presentation, and the staged offer/ACK state machine only.
final class FlarkV3HostController {
  FlarkV3HostController._({
    required FlarkV3SourceVersion currentSource,
    required FlarkV3UiSourceIdentity currentUiSource,
    required FlarkV3HostStore store,
  }) : _currentSource = currentSource,
       _currentUiSource = currentUiSource,
       _store = store;

  static FlarkV3HostAttachment attach({
    required FlarkV3SourceVersion currentSource,
    FlarkV3UiSourceIdentity? currentUiSource,
    required FlarkV3HostStore store,
  }) {
    final uiSource =
        currentUiSource ?? FlarkV3UiSourceIdentity.fromCertified(currentSource);
    _validateUiAtOrAfterCertified(
      uiSource: uiSource,
      certifiedSource: currentSource,
    );
    final controller = FlarkV3HostController._(
      currentSource: currentSource,
      currentUiSource: uiSource,
      store: store,
    );
    final observed = store.observeSourceVersion(currentSource);
    final rejection = switch (observed) {
      FlarkV3HostAccepted<FlarkV3HostUnit>() => null,
      FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection) => rejection,
    };
    controller._storeSourceSynchronized = rejection == null;
    return FlarkV3HostAttachment(
      controller: controller,
      storeRejection: rejection,
    );
  }

  final FlarkV3HostStore _store;
  FlarkV3SourceVersion _currentSource;
  FlarkV3UiSourceIdentity _currentUiSource;
  bool _storeSourceSynchronized = false;
  FlarkV3StructuralAck? _installedAck;
  FlarkV3StructuralAck? _pendingDeliveryAck;
  FlarkV3HostOfferBegin? _activeOffer;

  FlarkV3SourceVersion get currentSource => _currentSource;
  FlarkV3UiSourceIdentity get currentUiSource => _currentUiSource;
  bool get currentUiSourceCertified =>
      _currentUiSource.bindsCertified(_currentSource);
  bool get storeSourceSynchronized => _storeSourceSynchronized;
  FlarkV3StructuralAck? get pendingDeliveryAck => _pendingDeliveryAck;

  FlarkV3HostPresentationState get presentationState {
    final installed = _installedAck;
    if (currentUiSourceCertified &&
        _storeSourceSynchronized &&
        installed != null &&
        installed.sourceVersion == _currentSource) {
      return FlarkV3ExactStructuralPresentation(
        uiSource: _currentUiSource,
        sourceVersion: _currentSource,
        ack: installed,
      );
    }
    return FlarkV3StablePendingPresentation(
      uiSource: _currentUiSource,
      sourceVersion: _currentSource,
      uiSourceGap: _wholeUiSourceGap(),
      sourceGap: currentUiSourceCertified ? _wholeSourceGap() : null,
      stablePaintAck: installed,
      reason: !currentUiSourceCertified
          ? FlarkV3StablePendingReason.sourceUncertified
          : !_storeSourceSynchronized
          ? FlarkV3StablePendingReason.storeUnsynchronized
          : installed == null
          ? FlarkV3StablePendingReason.initialSnapshot
          : _activeOffer == null
          ? FlarkV3StablePendingReason.sourceAdvanced
          : FlarkV3StablePendingReason.publicationPending,
    );
  }

  /// Advances exact source authority even if the structural store cannot.
  ///
  /// A store rejection leaves the controller in source-authoritative pending
  /// mode. The caller can replace the store or call [resynchronizeStore]; stale
  /// semantics never become authoritative as an error fallback.
  FlarkV3SourceAdoptionReceipt observeSourceEdit(FlarkV3SourceVersion target) {
    if (!currentUiSourceCertified ||
        target.documentSession != _currentUiSource.documentSession ||
        target.revision <= _currentUiSource.uiRevision) {
      throw ArgumentError.value(
        target,
        'target',
        'A certified fast-path edit must strictly extend certified UI source.',
      );
    }

    _currentUiSource = FlarkV3UiSourceIdentity.fromCertified(target);
    return _adoptCertifiedSource(target);
  }

  /// Advances exact UTF-16 UI source without manufacturing byte/hash facts.
  ///
  /// An in-flight offer is suppressed locally. Stable old structure may
  /// remain paint-only.
  ///
  /// The store retains staging until the corresponding certified source is
  /// observed. That single source-authority transition atomically supersedes
  /// staging and moves its storage into bounded background reclamation. An
  /// explicit abort here would instead enter an abort-completion protocol that
  /// must be polled before a replacement offer can begin.
  FlarkV3UiSourceAdvanceReceipt observeUncertifiedUiSource(
    FlarkV3UiSourceIdentity target,
  ) {
    if (target.documentSession != _currentUiSource.documentSession ||
        target.uiRevision <= _currentUiSource.uiRevision) {
      throw ArgumentError.value(
        target,
        'target',
        'UI source must strictly extend the current document session.',
      );
    }
    _currentUiSource = target;
    // Fail closed without crossing into an adapter: no contract-violating
    // store call can leave old structure authoritative for the newer UI
    // source, and certified source adoption remains the sole physical
    // supersession boundary.
    _activeOffer = null;
    return FlarkV3UiSourceAdvanceReceipt(
      uiSource: target,
      activeOfferAbort: null,
    );
  }

  /// Suppresses the active parser offer while leaving certified UI source and
  /// installed paint authority unchanged.
  ///
  /// Source-worker restart uses this because an offer produced by the old
  /// replica generation must never commit after that worker is invalidated.
  FlarkV3HostCallResult<FlarkV3HostUnit>? suppressActiveOffer() {
    final active = _activeOffer;
    if (active == null) return null;
    _activeOffer = null;
    return _store.abortOffer(active.offerId);
  }

  /// Promotes facts only when they bind the exact UI lineage still visible.
  FlarkV3SourceAdoptionReceipt observeCertifiedUiSource({
    required FlarkV3UiSourceIdentity uiSource,
    required FlarkV3SourceVersion certifiedSource,
  }) {
    if (uiSource != _currentUiSource ||
        !uiSource.bindsCertified(certifiedSource) ||
        certifiedSource.revision <= _currentSource.revision) {
      throw ArgumentError(
        'Certified source must bind the exact current UI lineage and advance '
        'the last certified host version.',
      );
    }
    return _adoptCertifiedSource(certifiedSource);
  }

  FlarkV3SourceAdoptionReceipt _adoptCertifiedSource(
    FlarkV3SourceVersion target,
  ) {
    _currentSource = target;
    _storeSourceSynchronized = false;
    _activeOffer = null;
    final observed = _store.observeSourceVersion(target);
    if (observed is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _storeSourceSynchronized = true;
    }
    return FlarkV3SourceAdoptionReceipt(
      sourceVersion: target,
      storeRejection: switch (observed) {
        FlarkV3HostAccepted<FlarkV3HostUnit>() => null,
        FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection) => rejection,
      },
    );
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> resynchronizeStore() {
    if (!currentUiSourceCertified) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'UI source is awaiting certified byte/hash facts.',
        ),
      );
    }
    final observed = _store.observeSourceVersion(_currentSource);
    if (observed is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _storeSourceSynchronized = true;
    }
    return observed;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    if (!currentUiSourceCertified || !_storeSourceSynchronized) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Host store is not synchronized with certified current UI source.',
        ),
      );
    }
    if (begin.sourceVersion.revision < _currentSource.revision) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.staleSource,
          'Publication targets an older source revision.',
        ),
      );
    }
    if (begin.sourceVersion != _currentSource) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Publication does not bind exact current source.',
        ),
      );
    }
    if (begin.mode != FlarkV3PublicationMode.fullSnapshot &&
        begin.baseAck != _installedAck) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.baseMismatch,
          'Exact-base publication does not bind the installed ACK.',
        ),
      );
    }
    final result = _store.beginOffer(begin);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _activeOffer = begin;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (!currentUiSourceCertified) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Structural admission is closed until UI source is certified.',
        ),
      );
    }
    final active = _activeOffer;
    if (active == null || active.offerId != packet.offerId) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.wrongOffer,
          'Packet does not belong to the active offer.',
        ),
      );
    }
    return _store.admitPacket(packet);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    if (!currentUiSourceCertified) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Structural commit is closed until UI source is certified.',
        ),
      );
    }
    if (_activeOffer?.offerId != request.offerId) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.wrongOffer,
          'Commit does not name the active offer.',
        ),
      );
    }
    return _store.requestCommit(request);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    if (_activeOffer?.offerId != offerId) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.wrongOffer,
          'Abort does not name the active offer.',
        ),
      );
    }
    return _store.abortOffer(offerId);
  }

  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    final result = _store.poll(grant);
    if (result case FlarkV3HostAccepted<FlarkV3HostPollOutcome>(
      value: final outcome,
    )) {
      switch (outcome) {
        case FlarkV3HostCommitted(:final ack):
          final active = _activeOffer;
          if (!currentUiSourceCertified ||
              !_storeSourceSynchronized ||
              active == null ||
              !_ackMatchesOffer(ack, active) ||
              ack.sourceVersion != _currentSource) {
            // Reassert the exact source boundary so even a faulty adapter
            // cannot leak a stale installed root through a later query.
            if (currentUiSourceCertified) {
              _storeSourceSynchronized = false;
              _store.observeSourceVersion(_currentSource);
            }
            return const FlarkV3HostRejected(
              FlarkV3HostRejection(
                FlarkV3HostRejectReason.invalid,
                'Host store committed an ACK outside the active exact offer.',
              ),
            );
          }
          _installedAck = ack;
          _pendingDeliveryAck = ack;
          _activeOffer = null;
        case FlarkV3HostAbortComplete(:final offerId):
          if (_activeOffer?.offerId == offerId) _activeOffer = null;
        case FlarkV3HostPacketCredit(:final offerId):
          if (!currentUiSourceCertified || _activeOffer?.offerId != offerId) {
            return const FlarkV3HostRejected(
              FlarkV3HostRejection(
                FlarkV3HostRejectReason.superseded,
                'Packet credit belongs to a suppressed source offer.',
              ),
            );
          }
          break;
        case FlarkV3HostPollPending():
          break;
        case FlarkV3HostClosed():
          break;
      }
    }
    return result;
  }

  /// Retires a previously delivered ACK; it never adopts presentation state.
  ///
  /// Therefore an ACK for the prior certified root may still drain while the
  /// UI source is uncertified without restoring semantic authority.
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) {
    if (_pendingDeliveryAck != ack) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.invalid,
          'ACK does not match the pending delivery.',
        ),
      );
    }
    final result = _store.acknowledgeDelivery(ack);
    if (result is FlarkV3HostAccepted<FlarkV3HostUnit>) {
      _pendingDeliveryAck = null;
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostPresentationQuery> query(
    FlarkV3HostPointQuery query,
  ) {
    if (!currentUiSourceCertified) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Structural query is closed until UI source is certified.',
        ),
      );
    }
    if (query.sourceVersion != _currentSource ||
        !_currentSource.metric.contains(query.position)) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Query does not bind a point in exact current source.',
        ),
      );
    }
    if (presentationState is FlarkV3StablePendingPresentation) {
      return FlarkV3HostAccepted(
        FlarkV3SourceGapPresentationQuery(_wholeSourceGap()),
      );
    }
    final result = _store.queryStructural(query);
    if (result case FlarkV3HostAccepted<FlarkV3HostStoreQueryOutcome>(
      value: final outcome,
    )) {
      switch (outcome) {
        case FlarkV3HostStoreStructuralQuery(:final viewport):
          final valid =
              viewport.sourceVersion == _currentSource &&
              _currentSource.metric.contains(viewport.range.end) &&
              viewport.receipt.encodedBytes == viewport.encoded.length &&
              viewport.receipt.encodedBytes <= query.budget.maxEncodedBytes &&
              viewport.receipt.openDepth <= query.budget.maxOpenDepth &&
              viewport.receipt.leafCount <= query.budget.maxLeafCount &&
              viewport.receipt.treeNodesVisited <=
                  query.budget.maxTreeNodesVisited;
          if (valid) {
            return FlarkV3HostAccepted(
              FlarkV3StructuralPresentationQuery(viewport),
            );
          }
          _storeSourceSynchronized = false;
          return FlarkV3HostAccepted(
            FlarkV3SourceGapPresentationQuery(_wholeSourceGap()),
          );
        case FlarkV3HostStoreSourceGapQuery(:final gap):
          final pointFollowsStart = query.position.contains(gap.range.start);
          final valid =
              gap.sourceVersion == _currentSource &&
              _currentSource.metric.contains(gap.range.end) &&
              pointFollowsStart &&
              gap.range.end.contains(query.position) &&
              gap.receipt.encodedBytes <= query.budget.maxEncodedBytes &&
              gap.receipt.openDepth <= query.budget.maxOpenDepth &&
              gap.receipt.leafCount <= query.budget.maxLeafCount &&
              gap.receipt.treeNodesVisited <= query.budget.maxTreeNodesVisited;
          if (valid) {
            return FlarkV3HostAccepted(
              FlarkV3SourceGapPresentationQuery(
                FlarkV3SourceGap(
                  sourceVersion: gap.sourceVersion,
                  range: gap.range,
                  structuralReason: gap.reason,
                  structuralReceipt: gap.receipt,
                ),
              ),
            );
          }
          _storeSourceSynchronized = false;
          return FlarkV3HostAccepted(
            FlarkV3SourceGapPresentationQuery(_wholeSourceGap()),
          );
      }
    }
    final rejection = (result as FlarkV3HostRejected).rejection;
    if (rejection.reason == FlarkV3HostRejectReason.queryBoundExceeded) {
      return FlarkV3HostAccepted(
        FlarkV3SourceGapPresentationQuery(_wholeSourceGap()),
      );
    }
    return FlarkV3HostRejected(rejection);
  }

  FlarkV3HostCallResult<FlarkV3HostBlockRangePresentationQuery> queryBlockRange(
    FlarkV3HostBlockRangeQuery query,
  ) {
    if (!currentUiSourceCertified) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Structural range query is closed until UI source is certified.',
        ),
      );
    }
    if (query.sourceVersion != _currentSource ||
        !_currentSource.metric.contains(query.requestedRange.end)) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Range query does not bind exact current source.',
        ),
      );
    }
    if (presentationState is FlarkV3StablePendingPresentation) {
      return FlarkV3HostAccepted(
        FlarkV3BlockRangeSourceGapPresentationQuery(
          _blockRangeGap(query, FlarkV3HostSourceGapReason.unavailableFacts),
        ),
      );
    }

    final store = _store;
    final FlarkV3BlockRangeHostStore? rangeStore =
        store is FlarkV3BlockRangeHostStore
        ? store as FlarkV3BlockRangeHostStore
        : null;
    if (rangeStore == null) {
      return FlarkV3HostAccepted(
        FlarkV3BlockRangeSourceGapPresentationQuery(
          _blockRangeGap(query, FlarkV3HostSourceGapReason.unavailableFacts),
        ),
      );
    }
    final result = rangeStore.queryStructuralRange(query);
    if (result case FlarkV3HostAccepted<FlarkV3HostStoreBlockRangeQueryOutcome>(
      value: final outcome,
    )) {
      switch (outcome) {
        case FlarkV3HostStoreStructuralBlockRangeQuery(:final range):
          final receipt = range.receipt;
          final valid =
              range.sourceVersion == _currentSource &&
              range.requestedRange == query.requestedRange &&
              _currentSource.metric.contains(range.coveredRange.end) &&
              receipt.encodedBytes == range.encoded.length &&
              receipt.encodedBytes <= query.budget.maxEncodedBytes &&
              receipt.blockCount <= query.budget.maxBlockCount &&
              receipt.storagePagesVisited <=
                  query.budget.maxStoragePagesVisited &&
              receipt.openDepth <= query.budget.maxOpenDepth &&
              receipt.treeNodesVisited <= query.budget.maxTreeNodesVisited &&
              receipt.complete == (range.continuation == null) &&
              (receipt.blockCount > 0 || receipt.complete);
          if (valid) {
            return FlarkV3HostAccepted(
              FlarkV3StructuralBlockRangePresentationQuery(range),
            );
          }
        case FlarkV3HostStoreBlockRangeSourceGapQuery(:final gap):
          final receipt = gap.receipt;
          final valid =
              gap.sourceVersion == _currentSource &&
              gap.requestedRange == query.requestedRange &&
              receipt.encodedBytes == 0 &&
              receipt.blockCount == 0 &&
              receipt.encodedBytes <= query.budget.maxEncodedBytes &&
              receipt.storagePagesVisited <=
                  query.budget.maxStoragePagesVisited &&
              receipt.openDepth <= query.budget.maxOpenDepth &&
              receipt.treeNodesVisited <= query.budget.maxTreeNodesVisited;
          if (valid) {
            return FlarkV3HostAccepted(
              FlarkV3BlockRangeSourceGapPresentationQuery(gap),
            );
          }
      }
      _storeSourceSynchronized = false;
      return FlarkV3HostAccepted(
        FlarkV3BlockRangeSourceGapPresentationQuery(
          _blockRangeGap(query, FlarkV3HostSourceGapReason.undecodableClosure),
        ),
      );
    }
    final rejection = (result as FlarkV3HostRejected).rejection;
    if (rejection.reason == FlarkV3HostRejectReason.queryBoundExceeded) {
      return FlarkV3HostAccepted(
        FlarkV3BlockRangeSourceGapPresentationQuery(
          _blockRangeGap(query, FlarkV3HostSourceGapReason.encodedByteLimit),
        ),
      );
    }
    return FlarkV3HostRejected(rejection);
  }

  FlarkV3HostCallResult<FlarkV3HostStructuralOrdinalWindowOutcome>
  queryStructuralOrdinalWindow(FlarkV3HostStructuralOrdinalWindowQuery query) {
    if (!currentUiSourceCertified) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Structural ordinal query is closed until UI source is certified.',
        ),
      );
    }
    if (query.sourceVersion != _currentSource) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.exactSourceMismatch,
          'Structural ordinal query does not bind exact current source.',
        ),
      );
    }
    if (presentationState is FlarkV3StablePendingPresentation) {
      return FlarkV3HostAccepted(_ordinalWindowUnavailable(query));
    }
    final store = _store;
    final FlarkV3StructuralOrdinalWindowHostStore? ordinalStore =
        store is FlarkV3StructuralOrdinalWindowHostStore
        ? store as FlarkV3StructuralOrdinalWindowHostStore
        : null;
    if (ordinalStore == null) {
      return FlarkV3HostAccepted(_ordinalWindowUnavailable(query));
    }
    final result = ordinalStore.queryStructuralOrdinalWindow(query);
    if (result
        case FlarkV3HostAccepted<FlarkV3HostStructuralOrdinalWindowOutcome>(
          :final value,
        )) {
      if (value.binds(query)) return result;
      _storeSourceSynchronized = false;
      return FlarkV3HostAccepted(
        FlarkV3HostStructuralOrdinalWindowFailure(
          sourceVersion: query.sourceVersion,
          totalBlockCount: FlarkV3ProtocolU64.zero,
          startBlockOrdinal: query.startBlockOrdinal,
          reason: FlarkV3HostStructuralOrdinalWindowFailureReason.undecodable,
          work: FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero,
        ),
      );
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> close() => _store.close();

  FlarkV3SourceGap _wholeSourceGap() => FlarkV3SourceGap(
    sourceVersion: _currentSource,
    range: FlarkV3MetricRange(
      start: FlarkV3SourceMetric.zero,
      end: _currentSource.metric,
    ),
  );

  FlarkV3HostBlockRangeSourceGap _blockRangeGap(
    FlarkV3HostBlockRangeQuery query,
    FlarkV3HostSourceGapReason reason,
  ) => FlarkV3HostBlockRangeSourceGap(
    sourceVersion: _currentSource,
    requestedRange: query.requestedRange,
    reason: reason,
    receipt: FlarkV3HostBlockRangeReceipt(
      encodedBytes: 0,
      blockCount: 0,
      storagePagesVisited: 0,
      openDepth: 0,
      treeNodesVisited: 0,
      packedEntriesInspected: 0,
      summaryNodesSkipped: 0,
      complete: false,
    ),
  );

  FlarkV3HostStructuralOrdinalWindowFailure _ordinalWindowUnavailable(
    FlarkV3HostStructuralOrdinalWindowQuery query,
  ) => FlarkV3HostStructuralOrdinalWindowFailure(
    sourceVersion: query.sourceVersion,
    totalBlockCount: FlarkV3ProtocolU64.zero,
    startBlockOrdinal: query.startBlockOrdinal,
    reason: FlarkV3HostStructuralOrdinalWindowFailureReason.unavailable,
    work: FlarkV3HostStructuralOrdinalWindowWorkReceipt.zero,
  );

  FlarkV3UiSourceGap _wholeUiSourceGap() =>
      FlarkV3UiSourceGap.whole(_currentUiSource);
}

void _validateUiAtOrAfterCertified({
  required FlarkV3UiSourceIdentity uiSource,
  required FlarkV3SourceVersion certifiedSource,
}) {
  if (uiSource.documentSession != certifiedSource.documentSession ||
      uiSource.uiRevision < certifiedSource.revision ||
      (uiSource.uiRevision == certifiedSource.revision &&
          !uiSource.bindsCertified(certifiedSource))) {
    throw ArgumentError(
      'UI source must be in the certified document lineage and cannot precede '
      'its last certified version.',
    );
  }
}

bool _ackMatchesOffer(FlarkV3StructuralAck ack, FlarkV3HostOfferBegin offer) =>
    ack.publicationSession == offer.publicationSession &&
    ack.hostRevision == offer.targetHostRevision &&
    ack.sourceVersion == offer.sourceVersion &&
    ack.sourceRoot == offer.sourceRoot &&
    ack.parseGeneration == offer.parseGeneration &&
    ack.grammarRevision == offer.grammarRevision &&
    ack.syntaxProfile == offer.syntaxProfile &&
    ack.authorityMask == offer.authorityMask &&
    ack.recordCount == offer.targetRecordCount;

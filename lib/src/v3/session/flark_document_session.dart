import '../host/host.dart';
import '../host/flark_v3_inline_sidecar_controller.dart';
import '../source/source.dart';
import 'flark_document_work_profile.dart';

/// Result of applying one exact source transaction to a document session.
final class FlarkDocumentEditReceipt {
  const FlarkDocumentEditReceipt({
    required this.sourceApply,
    required this.uiSource,
    required this.sourceCertified,
    required this.storeSynchronized,
    required this.certifiedAdoption,
    required this.uiAdvance,
  });

  final FlarkV3SourceSessionApplyReceipt sourceApply;
  final FlarkV3UiSourceIdentity uiSource;
  final bool sourceCertified;
  final bool storeSynchronized;
  final FlarkV3SourceAdoptionReceipt? certifiedAdoption;
  final FlarkV3UiSourceAdvanceReceipt? uiAdvance;

  bool get changed => sourceApply.changed;
  bool get provisional => !sourceCertified;
}

final class FlarkDocumentCertificationReceipt {
  const FlarkDocumentCertificationReceipt({
    required this.promotion,
    required this.uiSource,
    required this.hostAdoption,
  });

  final FlarkV3SourcePromotionReceipt promotion;
  final FlarkV3UiSourceIdentity uiSource;
  final FlarkV3SourceAdoptionReceipt? hostAdoption;

  bool get promoted =>
      promotion.disposition == FlarkV3SourcePromotionDisposition.promoted;
}

final class FlarkDocumentSourceWorkerRestartReceipt {
  const FlarkDocumentSourceWorkerRestartReceipt({
    required this.workerGeneration,
    required this.activeOfferAbort,
  });

  final int workerGeneration;
  final FlarkV3HostCallResult<FlarkV3HostUnit>? activeOfferAbort;
}

/// Pure-Dart owner of exact source, parser-replica and host-publication state.
///
/// Flutter and other presentation adapters translate their input model into
/// [FlarkV3SourceTransaction] values and observe/query this single authority.
final class FlarkDocumentSession {
  FlarkDocumentSession._({
    required FlarkV3SourceSession sourceSession,
    required FlarkV3HostController hostController,
    required FlarkV3InlineSidecarController inlineSidecarController,
    required FlarkV3ViewportPresentationController
    viewportPresentationController,
    required this.workProfile,
  }) : _sourceSession = sourceSession,
       _hostController = hostController,
       _inlineSidecarController = inlineSidecarController,
       _viewportPresentationController = viewportPresentationController;

  factory FlarkDocumentSession.attach({
    required FlarkV3SourceSession sourceSession,
    required FlarkV3DocumentSessionId documentSession,
    required FlarkV3HostStore hostStore,
    FlarkV3SourceVersion? certifiedSourceVersion,
    FlarkDocumentWorkProfile workProfile = FlarkDocumentWorkProfile.prototype,
  }) {
    final source = sourceSession.document;
    final FlarkV3SourceVersion sourceVersion;
    if (source.hasCertifiedFacts) {
      sourceVersion = FlarkV3SourceVersion.fromDocument(
        documentSession: documentSession,
        document: source,
      );
      if (certifiedSourceVersion != null &&
          certifiedSourceVersion != sourceVersion) {
        throw ArgumentError(
          'Explicit certified source must match the fully indexed source.',
        );
      }
    } else {
      sourceVersion =
          certifiedSourceVersion ??
          (throw ArgumentError(
            'A provisional source session requires its last certified host '
            'version.',
          ));
      if (sourceVersion.documentSession != documentSession ||
          !_sourceVersionMatchesFingerprint(
            sourceVersion,
            sourceSession.lastCertifiedFingerprint,
          )) {
        throw ArgumentError(
          'Certified source must match the session last-certified '
          'fingerprint exactly.',
        );
      }
    }
    final uiSource = FlarkV3UiSourceIdentity(
      documentSession: documentSession,
      uiRevision: sourceSession.uiRevision,
      utf16Length: source.utf16Length,
    );
    final attachment = FlarkV3HostController.attach(
      currentSource: sourceVersion,
      currentUiSource: uiSource,
      store: hostStore,
    );
    return FlarkDocumentSession._(
      sourceSession: sourceSession,
      hostController: attachment.controller,
      inlineSidecarController: FlarkV3InlineSidecarController.attach(
        currentSource: sourceVersion,
        store: hostStore,
      ),
      viewportPresentationController:
          FlarkV3ViewportPresentationController.attach(
            currentSource: sourceVersion,
            store: hostStore,
          ),
      workProfile: workProfile,
    );
  }

  final FlarkV3SourceSession _sourceSession;
  final FlarkV3HostController _hostController;
  final FlarkV3InlineSidecarController _inlineSidecarController;
  final FlarkV3ViewportPresentationController _viewportPresentationController;
  final FlarkDocumentWorkProfile workProfile;

  FlarkV3SourceDocument get source => _sourceSession.document;
  int get uiRevision => _sourceSession.uiRevision;
  FlarkV3SourceVersion get sourceVersion => _hostController.currentSource;
  FlarkV3UiSourceIdentity get uiSource => _hostController.currentUiSource;
  FlarkV3HostPresentationState get presentationState =>
      _hostController.presentationState;
  FlarkV3StructuralAck? get pendingDeliveryAck =>
      _hostController.pendingDeliveryAck;
  bool get supportsInlineSidecars => _inlineSidecarController.supported;
  FlarkV3InlineSidecarAck? get pendingInlineSidecarDeliveryAck =>
      _inlineSidecarController.pendingDeliveryAck;
  FlarkV3InlineSidecarAck? get installedInlineSidecarAck =>
      _inlineSidecarController.installedAck;
  FlarkV3HotInlineSidecarBinding? get installedInlineSidecarBinding =>
      _inlineSidecarController.installedBinding;
  bool get supportsViewportPresentation =>
      _viewportPresentationController.supported;
  FlarkV3ViewportPresentationAck? get pendingViewportPresentationDeliveryAck =>
      _viewportPresentationController.pendingDeliveryAck;
  FlarkV3ViewportPresentationAck? get installedViewportPresentationAck =>
      _viewportPresentationController.installedAck;
  bool get currentUiSourceCertified => _hostController.currentUiSourceCertified;
  bool get storeSourceSynchronized => _hostController.storeSourceSynchronized;
  bool get hasPendingSourceWorkerSync => _sourceSession.hasPendingWorkerSync;
  bool get sourceWorkerSynchronized =>
      _sourceSession.workerRevision == _sourceSession.uiRevision &&
      !_sourceSession.hasPendingWorkerSync;
  FlarkV3CanonicalSourceFactAuthority?
  get retainedCanonicalSourceFactDeltaBase =>
      _sourceSession.retainedCanonicalSourceFactDeltaBase;

  FlarkDocumentEditReceipt apply(FlarkV3SourceTransaction transaction) =>
      _adoptAppliedSourceEdit(_sourceSession.apply(transaction));

  FlarkDocumentEditReceipt? undo() {
    final applied = _sourceSession.undo();
    return applied == null ? null : _adoptAppliedSourceEdit(applied);
  }

  FlarkDocumentEditReceipt _adoptAppliedSourceEdit(
    FlarkV3SourceSessionApplyReceipt applied,
  ) {
    if (!applied.changed) {
      return FlarkDocumentEditReceipt(
        sourceApply: applied,
        uiSource: uiSource,
        sourceCertified: currentUiSourceCertified,
        storeSynchronized: currentUiSourceCertified && storeSourceSynchronized,
        certifiedAdoption: null,
        uiAdvance: null,
      );
    }

    final nextUiSource = _sourceIdentity();
    FlarkV3SourceAdoptionReceipt? certifiedAdoption;
    FlarkV3UiSourceAdvanceReceipt? uiAdvance;
    if (!source.hasCertifiedFacts) {
      _inlineSidecarController.invalidateForUiSourceAdvance();
      _viewportPresentationController.invalidateForUiSourceAdvance();
      uiAdvance = _hostController.observeUncertifiedUiSource(nextUiSource);
    } else {
      final target = FlarkV3SourceVersion.fromDocument(
        documentSession: sourceVersion.documentSession,
        document: source,
      );
      _inlineSidecarController.observeSourceAdvance(target);
      _viewportPresentationController.observeSourceAdvance(target);
      if (_hostController.currentUiSourceCertified) {
        certifiedAdoption = _hostController.observeSourceEdit(target);
      } else {
        uiAdvance = _hostController.observeUncertifiedUiSource(nextUiSource);
        certifiedAdoption = _hostController.observeCertifiedUiSource(
          uiSource: nextUiSource,
          certifiedSource: target,
        );
      }
    }
    return FlarkDocumentEditReceipt(
      sourceApply: applied,
      uiSource: nextUiSource,
      sourceCertified: currentUiSourceCertified,
      storeSynchronized: currentUiSourceCertified && storeSourceSynchronized,
      certifiedAdoption: certifiedAdoption,
      uiAdvance: uiAdvance,
    );
  }

  FlarkV3SourceWorkerSyncLease beginSourceWorkerSync({
    int maximumEntries = 64,
    int maximumOperations = 1024,
    int maximumPayloadUtf16 = 8192,
    int maximumSnapshotPageUtf16 = 8192,
  }) => _sourceSession.beginWorkerSync(
    maximumEntries: maximumEntries,
    maximumOperations: maximumOperations,
    maximumPayloadUtf16: maximumPayloadUtf16,
    maximumSnapshotPageUtf16: maximumSnapshotPageUtf16,
  );

  FlarkV3SourceWorkerSyncAckReceipt acknowledgeSourceWorkerSync(
    FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
  ) => _sourceSession.acknowledgeWorkerSync(acknowledgement);

  bool ownsSourceWorkerSyncLease(int leaseId) =>
      _sourceSession.ownsWorkerSyncLease(leaseId);

  bool releaseSourceWorkerSyncLease(int leaseId) =>
      _sourceSession.releaseWorkerSyncLease(leaseId);

  FlarkDocumentSourceWorkerRestartReceipt restartSourceWorker() {
    final workerGeneration = _sourceSession.restartWorker();
    _inlineSidecarController.suppressParserOffer();
    _viewportPresentationController.suppressParserOffer();
    final activeOfferAbort = _hostController.suppressActiveOffer();
    return FlarkDocumentSourceWorkerRestartReceipt(
      workerGeneration: workerGeneration,
      activeOfferAbort: activeOfferAbort,
    );
  }

  FlarkV3SourceCertificationRequest beginSourceCertification({
    int maximumPieceDescriptors = 64,
    int maximumDiscoveryNodes = 512,
  }) => _sourceSession.beginCertification(
    maximumPieceDescriptors: maximumPieceDescriptors,
    maximumDiscoveryNodes: maximumDiscoveryNodes,
  );

  FlarkV3SourcePendingPiecePage continueSourceCertificationPieces({
    required int requestId,
    required int cursorUtf16,
    int maximumPieceDescriptors = 64,
    int maximumDiscoveryNodes = 512,
  }) => _sourceSession.continueCertificationPieces(
    requestId: requestId,
    cursorUtf16: cursorUtf16,
    maximumPieceDescriptors: maximumPieceDescriptors,
    maximumDiscoveryNodes: maximumDiscoveryNodes,
  );

  /// Moves one bounded worker-produced checkpoint page into the hidden source
  /// certification candidate. No source or host authority changes here.
  FlarkV3SourceFactStageReceipt stageSourceCertificationCheckpointPage(
    FlarkV3SourceFactCheckpointPage page, {
    int maximumPathNodes = 512,
  }) => _sourceSession.stageCertificationCheckpointPage(
    page,
    maximumPathNodes: maximumPathNodes,
  );

  /// Atomically promotes a completely staged source-fact candidate, then
  /// advances host source authority through the same adoption gate as the
  /// bounded one-shot convenience.
  FlarkDocumentCertificationReceipt commitSourceFactCertification(
    FlarkV3SourceFactCompletion completion,
  ) => _adoptSourcePromotion(
    _sourceSession.commitSourceFactCertification(completion),
  );

  /// Moves one bounded canonical Rust SourceFacts page into the hidden global
  /// candidate. No source or host authority changes here.
  FlarkV3SourceFactStageReceipt stageCanonicalSourceFactCheckpointPage(
    FlarkV3CanonicalSourceFactCheckpointPage page,
  ) => _sourceSession.stageCanonicalSourceFactCheckpointPage(page);

  /// Atomically promotes the canonical global fact root, then advances host
  /// source authority through the ordinary exact adoption gate.
  FlarkDocumentCertificationReceipt commitCanonicalSourceFactCertification(
    FlarkV3CanonicalSourceFactCompletion completion,
  ) => _adoptSourcePromotion(
    _sourceSession.commitCanonicalSourceFactCertification(completion),
  );

  FlarkV3CanonicalSourceFactDeltaBeginReceipt beginCanonicalSourceFactDelta(
    FlarkV3CanonicalSourceFactDelta delta,
  ) => _sourceSession.beginCanonicalSourceFactDelta(delta);

  FlarkV3SourceFactStageReceipt stageCanonicalSourceFactDeltaCheckpointPage(
    FlarkV3CanonicalSourceFactDeltaCheckpointPage page,
  ) => _sourceSession.stageCanonicalSourceFactDeltaCheckpointPage(page);

  FlarkV3CanonicalSourceFactDeltaPromotionReceipt
  commitCanonicalSourceFactDeltaCertification(
    FlarkV3CanonicalSourceFactDeltaCompletion completion,
  ) {
    final promotion = _sourceSession
        .commitCanonicalSourceFactDeltaCertification(completion);
    if (promotion.disposition == FlarkV3SourcePromotionDisposition.promoted) {
      _adoptCurrentCertifiedSource();
    }
    return promotion;
  }

  /// Promotes the currently certified SourceFacts root into the reusable
  /// incremental base at the same authority boundary as structural commit.
  void commitCanonicalSourceFactStructuralBase(FlarkV3StructuralAck ack) {
    final installed = FlarkV3SourceVersion.fromDocument(
      documentSession: sourceVersion.documentSession,
      document: source,
    );
    if (pendingDeliveryAck != ack ||
        ack.sourceVersion != sourceVersion ||
        ack.sourceVersion != installed) {
      throw StateError(
        'Structural SourceFacts base does not bind the committed source.',
      );
    }
    _sourceSession.commitInstalledCanonicalSourceFactStructuralBase();
  }

  FlarkV3SourceFactCancellationReceipt cancelSourceFactCertification(
    int requestId,
  ) => _sourceSession.cancelSourceFactCertification(requestId);

  FlarkV3SourceFactCancellationReceipt rejectSourceFactCertification(
    FlarkV3SourceCertificationFailure failure,
  ) => _sourceSession.rejectSourceFactCertification(failure);

  FlarkDocumentCertificationReceipt applySourceCertification(
    FlarkV3SourceCertificationReceipt receipt,
  ) => _adoptSourcePromotion(_sourceSession.applyCertification(receipt));

  FlarkDocumentCertificationReceipt _adoptSourcePromotion(
    FlarkV3SourcePromotionReceipt promotion,
  ) {
    if (promotion.disposition != FlarkV3SourcePromotionDisposition.promoted) {
      return FlarkDocumentCertificationReceipt(
        promotion: promotion,
        uiSource: uiSource,
        hostAdoption: null,
      );
    }
    if (!source.hasCertifiedFacts) {
      throw StateError('Promoted source still lacks certified derived facts.');
    }
    final adoption = _adoptCurrentCertifiedSource();
    return FlarkDocumentCertificationReceipt(
      promotion: promotion,
      uiSource: _sourceIdentity(),
      hostAdoption: adoption,
    );
  }

  FlarkV3SourceAdoptionReceipt? _adoptCurrentCertifiedSource() {
    if (!source.hasCertifiedFacts) {
      throw StateError('Promoted source still lacks certified derived facts.');
    }
    final currentUiSource = _sourceIdentity();
    final certifiedSource = FlarkV3SourceVersion.fromDocument(
      documentSession: sourceVersion.documentSession,
      document: source,
    );
    final FlarkV3SourceAdoptionReceipt? adoption;
    if (_hostController.currentUiSource == currentUiSource &&
        _hostController.currentSource == certifiedSource) {
      adoption = null;
    } else {
      _inlineSidecarController.observeSourceAdvance(certifiedSource);
      _viewportPresentationController.observeSourceAdvance(certifiedSource);
      adoption = _hostController.observeCertifiedUiSource(
        uiSource: currentUiSource,
        certifiedSource: certifiedSource,
      );
    }
    return adoption;
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    if (!sourceWorkerSynchronized) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Structural publication requires an exact source-worker replica.',
        ),
      );
    }
    return _hostController.beginOffer(begin);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (packet.rawBytes.length > workProfile.maximumPublicationPacketBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Publication packet exceeds the document-session work profile.',
        ),
      );
    }
    return _hostController.admitPacket(packet);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => _hostController.requestCommit(request);

  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      _hostController.abortOffer(offerId);

  FlarkV3HostCallResult<FlarkV3HostPollOutcome> pollHost(
    FlarkV3HostWorkGrant grant,
  ) {
    if (!workProfile.admitsHostGrant(grant)) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Host work grant exceeds the document-session work profile.',
        ),
      );
    }
    final result = _hostController.poll(grant);
    if (result case FlarkV3HostAccepted<FlarkV3HostPollOutcome>(
      value: FlarkV3HostCommitted(:final ack),
    )) {
      commitCanonicalSourceFactStructuralBase(ack);
      _inlineSidecarController.adoptStructuralAck(ack);
      _viewportPresentationController.adoptStructuralAck(ack);
    }
    return result;
  }

  FlarkV3HostCallResult<FlarkV3HostPresentationQuery> query(
    FlarkV3HostPointQuery query,
  ) {
    workProfile.validateQueryBudget(query.budget);
    return _hostController.query(query);
  }

  FlarkV3HostCallResult<FlarkV3HostBlockRangePresentationQuery> queryBlockRange(
    FlarkV3HostBlockRangeQuery query,
  ) {
    final budget = query.budget;
    if (budget.maxEncodedBytes > workProfile.maximumQueryEncodedBytes ||
        budget.maxBlockCount > workProfile.maximumQueryLeafCount ||
        budget.maxStoragePagesVisited > workProfile.maximumHostTransitions ||
        budget.maxOpenDepth > workProfile.maximumQueryOpenDepth ||
        budget.maxTreeNodesVisited > workProfile.maximumQueryTreeNodesVisited) {
      throw ArgumentError.value(
        budget,
        'budget',
        'Structural range query exceeds the document-session work profile.',
      );
    }
    return _hostController.queryBlockRange(query);
  }

  FlarkV3HostCallResult<FlarkV3HostStructuralOrdinalWindowOutcome>
  queryStructuralOrdinalWindow(FlarkV3HostStructuralOrdinalWindowQuery query) {
    final budget = query.budget;
    if (budget.maximumStoragePagesVisited >
            workProfile.maximumHostTransitions ||
        budget.maximumTreeNodesVisited >
            workProfile.maximumQueryTreeNodesVisited ||
        budget.maximumPackedEntriesInspected >
            workProfile.maximumQueryTreeNodesVisited) {
      throw ArgumentError.value(
        budget,
        'budget',
        'Structural ordinal query exceeds the document-session work profile.',
      );
    }
    return _hostController.queryStructuralOrdinalWindow(query);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => _hostController.acknowledgeDelivery(ack);

  FlarkV3HostCallResult<FlarkV3HostUnit> beginInlineSidecarOffer(
    FlarkV3HotInlineSidecarOfferBegin begin,
  ) {
    if (!sourceWorkerSynchronized ||
        presentationState is! FlarkV3ExactStructuralPresentation) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Hot-inline publication requires exact current structural authority.',
        ),
      );
    }
    if (pendingDeliveryAck != null) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.backpressure,
          'Structural delivery must complete before a sidecar can begin.',
        ),
      );
    }
    return _inlineSidecarController.beginOffer(begin);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> admitInlineSidecarPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (packet.rawBytes.length > workProfile.maximumPublicationPacketBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Sidecar packet exceeds the document-session work profile.',
        ),
      );
    }
    return _inlineSidecarController.admitPacket(packet);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> requestInlineSidecarCommit(
    FlarkV3HotInlineSidecarCommitRequest request,
  ) => _inlineSidecarController.requestCommit(request);

  FlarkV3HostCallResult<FlarkV3HostUnit> abortInlineSidecarOffer(
    FlarkV3OfferId offerId,
  ) => _inlineSidecarController.abortOffer(offerId);

  FlarkV3HostCallResult<FlarkV3InlineSidecarHostPollOutcome> pollInlineSidecar(
    FlarkV3HostWorkGrant grant,
  ) {
    if (!workProfile.admitsHostGrant(grant)) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Sidecar host grant exceeds the document-session work profile.',
        ),
      );
    }
    return _inlineSidecarController.poll(grant);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeInlineSidecarDelivery(
    FlarkV3InlineSidecarAck ack,
  ) => _inlineSidecarController.acknowledgeDelivery(ack);

  FlarkV3HostCallResult<FlarkV3InlineSidecarQueryOutcome> queryInlineSidecar(
    FlarkV3InlineSidecarQuery query,
  ) {
    if (query.maximumEncodedBytes >
        workProfile.maximumInlineSidecarQueryEncodedBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Sidecar query exceeds the document-session work profile.',
        ),
      );
    }
    return _inlineSidecarController.query(query);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> beginViewportPresentationOffer(
    FlarkV3ViewportPresentationOfferBegin begin,
  ) {
    if (!sourceWorkerSynchronized ||
        presentationState is! FlarkV3ExactStructuralPresentation) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.closed,
          'Viewport publication requires exact structural authority.',
        ),
      );
    }
    if (pendingDeliveryAck != null || pendingInlineSidecarDeliveryAck != null) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.backpressure,
          'Existing publication delivery must complete first.',
        ),
      );
    }
    return _viewportPresentationController.beginOffer(begin);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> admitViewportPresentationPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (packet.rawBytes.length > workProfile.maximumPublicationPacketBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Viewport packet exceeds the document-session work profile.',
        ),
      );
    }
    return _viewportPresentationController.admitPacket(packet);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> requestViewportPresentationCommit(
    FlarkV3ViewportPresentationCommitRequest request,
  ) => _viewportPresentationController.requestCommit(request);

  FlarkV3HostCallResult<FlarkV3HostUnit> abortViewportPresentationOffer(
    FlarkV3OfferId offerId,
  ) => _viewportPresentationController.abortOffer(offerId);

  void suppressViewportPresentationOffer() =>
      _viewportPresentationController.suppressParserOffer();

  FlarkV3HostCallResult<FlarkV3ViewportPresentationHostPollOutcome>
  pollViewportPresentation(FlarkV3HostWorkGrant grant) {
    if (!workProfile.admitsHostGrant(grant)) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.foregroundBoundExceeded,
          'Viewport host grant exceeds the document-session work profile.',
        ),
      );
    }
    return _viewportPresentationController.poll(grant);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit>
  acknowledgeViewportPresentationDelivery(FlarkV3ViewportPresentationAck ack) =>
      _viewportPresentationController.acknowledgeDelivery(ack);

  FlarkV3HostCallResult<FlarkV3ViewportPresentationQueryOutcome>
  queryViewportPresentation(FlarkV3ViewportPresentationQuery query) {
    if (query.maximumEncodedBytes >
        workProfile.maximumViewportQueryEncodedBytes) {
      return const FlarkV3HostRejected(
        FlarkV3HostRejection(
          FlarkV3HostRejectReason.queryBoundExceeded,
          'Viewport query exceeds the document-session work profile.',
        ),
      );
    }
    return _viewportPresentationController.query(query);
  }

  FlarkV3HostCallResult<FlarkV3HostUnit> resynchronizeHost() =>
      _hostController.resynchronizeStore();

  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    _inlineSidecarController.closeLocally();
    _viewportPresentationController.closeLocally();
    return _hostController.close();
  }

  FlarkV3UiSourceIdentity _sourceIdentity() => FlarkV3UiSourceIdentity(
    documentSession: sourceVersion.documentSession,
    uiRevision: _sourceSession.uiRevision,
    utf16Length: source.utf16Length,
  );
}

bool _sourceVersionMatchesFingerprint(
  FlarkV3SourceVersion sourceVersion,
  FlarkV3SourceFingerprint fingerprint,
) =>
    sourceVersion.revision == fingerprint.revision &&
    sourceVersion.metric.utf16 == fingerprint.utf16Length &&
    sourceVersion.metric.bytes == fingerprint.utf8Length &&
    sourceVersion.contentHash == fingerprint.contentHash128;

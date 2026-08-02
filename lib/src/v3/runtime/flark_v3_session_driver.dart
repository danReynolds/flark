import '../host/host.dart';
import '../session/session.dart';
import '../source/source.dart';
import 'flark_v3_hot_inline_sidecar_transport.dart';
import 'flark_v3_parser_transport.dart';
import 'flark_v3_viewport_presentation_transport.dart';

enum FlarkV3SessionDriverState { opening, open, faulted, closing, closed }

enum FlarkV3SessionPumpAction {
  idle,
  parserEvent,
  closeLatch,
  parserDrain,
  hostPoll,
  inlineSidecarHostPoll,
  viewportPresentationHostPoll,
  inlineRefinementRequest,
  viewportPresentationRequest,
  sourceSync,
}

enum FlarkV3PublicationDriverState {
  idle,
  acceptingPackets,
  awaitingPacketCredit,
  awaitingCommit,
  awaitingDeliveryAck,
  aborting,
}

enum FlarkV3InlineSidecarPublicationDriverState {
  idle,
  acceptingPackets,
  awaitingPacketCredit,
  awaitingCommit,
  awaitingDeliveryAck,
  aborting,
}

enum FlarkV3ViewportPresentationPublicationDriverState {
  idle,
  acceptingPackets,
  awaitingPacketCredit,
  awaitingCommit,
  awaitingDeliveryAck,
  aborting,
}

/// Receipt proving that one pump performed at most one bounded action.
final class FlarkV3SessionPumpReceipt {
  const FlarkV3SessionPumpReceipt({
    required this.action,
    required this.needsMoreWork,
  });

  final FlarkV3SessionPumpAction action;
  final bool needsMoreWork;
}

/// Caller-isolate orchestration for one [FlarkDocumentSession].
///
/// Parser callbacks only occupy [_pendingEvent]. Protocol work happens when
/// the caller explicitly grants one bounded [pump]. Source edits are already
/// coalesced by the session's intent journal; this driver creates no per-edit
/// queue and never creates a per-edit Future.
///
/// Structural publication is likewise credited. Begin, one closed packet,
/// commit, host polling, and delivery acknowledgement are distinct actions.
/// Sending a committed ACK to the worker is not treated as delivery: only an
/// exact credited delivery-ack event releases host backpressure.
final class FlarkV3SessionDriver {
  static const int _defaultHostInspectBytes = 16 * 1024;
  static const int _defaultHostCopyBytes = 16 * 1024;
  static const int _defaultHostTransitions = 32;
  static const int _minimumHostInspectBytes =
      FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes +
      FlarkV3HostOfferLimits.productMaximumFrameBytes;
  static const int _minimumHostCopyBytes =
      FlarkV3HostOfferLimits.productMaximumFrameBytes;

  FlarkV3SessionDriver({
    required FlarkDocumentSession session,
    required FlarkV3ParserTransport transport,
    required FlarkV3ParserSessionBinding parserBinding,
    FlarkV3ParserPublicationAuthority? publicationAuthority,
    FlarkV3HostWorkGrant? hostPollGrant,
    int parserDrainTransitions = flarkV3ParserMaximumDrainTransitions,
  }) : _session = session,
       _transport = transport,
       _inlineSidecarTransport =
           transport is FlarkV3ParserInlineSidecarTransport
           ? transport as FlarkV3ParserInlineSidecarTransport
           : null,
       _viewportPresentationTransport =
           transport is FlarkV3ParserViewportPresentationTransport
           ? transport as FlarkV3ParserViewportPresentationTransport
           : null,
       _binding = parserBinding,
       _publicationAuthority = publicationAuthority,
       _parserDrainTransitions = parserDrainTransitions,
       _hostPollGrant =
           hostPollGrant ??
           FlarkV3HostWorkGrant(
             inspectBytes: _boundedDefault(
               session.workProfile.maximumHostInspectBytes,
               _defaultHostInspectBytes,
             ),
             copyBytes: _boundedDefault(
               session.workProfile.maximumHostCopyBytes,
               _defaultHostCopyBytes,
             ),
             transitions: _boundedDefault(
               session.workProfile.maximumHostTransitions,
               _defaultHostTransitions,
             ),
           ),
       _sourceDirty = session.hasPendingSourceWorkerSync,
       _workerGeneration = parserBinding.workerGeneration {
    if (parserBinding.documentSession !=
        session.sourceVersion.documentSession) {
      throw ArgumentError.value(
        parserBinding,
        'parserBinding',
        'Parser binding does not name the document session.',
      );
    }
    if (parserDrainTransitions <= 0 ||
        parserDrainTransitions > flarkV3ParserMaximumDrainTransitions) {
      throw RangeError.range(
        parserDrainTransitions,
        1,
        flarkV3ParserMaximumDrainTransitions,
        'parserDrainTransitions',
      );
    }
    if (!session.workProfile.admitsHostGrant(_hostPollGrant)) {
      throw ArgumentError.value(
        _hostPollGrant,
        'hostPollGrant',
        'Driver host-poll grant exceeds the document work profile.',
      );
    }
    if (_hostPollGrant.inspectBytes < _minimumHostInspectBytes ||
        _hostPollGrant.copyBytes < _minimumHostCopyBytes ||
        _hostPollGrant.transitions < 1) {
      throw ArgumentError.value(
        _hostPollGrant,
        'hostPollGrant',
        'Driver host-poll grant cannot advance one maximum product frame.',
      );
    }
    _transport.bind(_enqueueParserEvent);
    _inlineSidecarTransport?.bindInlineSidecar(_enqueueInlineSidecarEvent);
    _viewportPresentationTransport?.bindViewportPresentation(
      _enqueueViewportPresentationEvent,
    );
    _transport.send(
      FlarkV3ParserOpen(binding: _binding, mode: FlarkV3ParserOpenMode.fresh),
    );
  }

  final FlarkDocumentSession _session;
  final FlarkV3ParserTransport _transport;
  final FlarkV3ParserInlineSidecarTransport? _inlineSidecarTransport;
  final FlarkV3ParserViewportPresentationTransport?
  _viewportPresentationTransport;
  FlarkV3ParserSessionBinding _binding;
  final FlarkV3ParserPublicationAuthority? _publicationAuthority;
  final FlarkV3HostWorkGrant _hostPollGrant;
  final int _parserDrainTransitions;

  FlarkV3SessionDriverState _state = FlarkV3SessionDriverState.opening;
  FlarkV3PublicationDriverState _publicationState =
      FlarkV3PublicationDriverState.idle;
  FlarkV3InlineSidecarPublicationDriverState _inlineSidecarPublicationState =
      FlarkV3InlineSidecarPublicationDriverState.idle;
  FlarkV3ViewportPresentationPublicationDriverState
  _viewportPresentationPublicationState =
      FlarkV3ViewportPresentationPublicationDriverState.idle;
  Object? _pendingEvent;
  FlarkV3SourceWorkerSyncLease? _sourceLease;
  FlarkV3HostOfferBegin? _activeOffer;
  FlarkV3ParserHostPollTicket? _activeHostPollTicket;
  int? _expectedPacketCreditFrameOrdinal;
  FlarkV3StructuralAck? _committedAckAwaitingDelivery;
  FlarkV3StructuralAck? _undeliveredAckRecoveryBase;
  FlarkV3HotInlineSidecarOfferBegin? _activeInlineSidecarOffer;
  FlarkV3HotInlineSidecarCommitRequest? _activeInlineSidecarCommit;
  FlarkV3ParserInlineSidecarHostPollTicket? _activeInlineSidecarPollTicket;
  int? _expectedInlineSidecarPacketCreditFrameOrdinal;
  FlarkV3InlineSidecarAck? _committedInlineSidecarAckAwaitingDelivery;
  FlarkV3InlineSidecarAck? _undeliveredInlineSidecarRecoveryAck;
  FlarkV3ViewportPresentationOfferBegin? _activeViewportPresentationOffer;
  FlarkV3ViewportPresentationCommitRequest? _activeViewportPresentationCommit;
  FlarkV3ParserViewportPresentationHostPollTicket?
  _activeViewportPresentationPollTicket;
  int? _expectedViewportPresentationPacketCreditFrameOrdinal;
  FlarkV3ViewportPresentationAck?
  _committedViewportPresentationAckAwaitingDelivery;
  FlarkV3ViewportPresentationAck? _undeliveredViewportPresentationRecoveryAck;
  FlarkV3ParserRefineInline? _pendingInlineRefinement;
  FlarkV3ParserPresentViewport? _pendingViewportPresentation;
  FlarkV3ParserPresentViewport? _issuedViewportPresentationRequest;
  bool _sourceDirty;
  bool _closeRequested = false;
  bool _hostPollPending = false;
  bool _inlineSidecarHostPollPending = false;
  bool _viewportPresentationHostPollPending = false;
  bool _parserClosed = false;
  bool _parserDrainPending = false;
  bool _parserDrained = false;
  bool _hostDrained = false;
  int? _workerGeneration;
  int _nextDrainId = 1;
  FlarkV3ParserDrainGrant? _activeDrainGrant;
  FlarkV3ParserOpenMode _expectedOpenMode = FlarkV3ParserOpenMode.fresh;
  int _lastHandledEventId = 0;
  int _lastAcceptedParseGeneration = 0;
  int _nextInlineRefinementGeneration = 1;
  int _lastIssuedInlineRefinementGeneration = 0;
  int _inlinePresentationGeneration = 0;
  int _inlineAttemptOutcomeGeneration = 0;
  int _nextViewportPresentationGeneration = 1;
  int _lastIssuedViewportPresentationGeneration = 0;
  int _viewportPresentationAttemptOutcomeGeneration = 0;
  int? _lastViewportPresentationUnavailableGeneration;
  int? _lastViewportPresentationUnavailableReason;
  FlarkV3ParserFailed? _lastFailure;
  FlarkV3ParserPublicationFailed? _lastPublicationFailure;
  FlarkV3ParserInlineSidecarFailed? _lastInlineSidecarFailure;
  FlarkV3ParserViewportPresentationFailed? _lastViewportPresentationFailure;
  FlarkV3HostRejection? _lastHostRejection;

  FlarkV3SessionDriverState get state => _state;
  FlarkV3PublicationDriverState get publicationState => _publicationState;
  FlarkV3InlineSidecarPublicationDriverState
  get inlineSidecarPublicationState => _inlineSidecarPublicationState;
  FlarkV3ViewportPresentationPublicationDriverState
  get viewportPresentationPublicationState =>
      _viewportPresentationPublicationState;
  bool get hasPendingParserEvent => _pendingEvent != null;
  bool get hasSourceLeaseInFlight => _sourceLease != null;
  bool get hasPendingHostPoll => _hostPollPending;
  bool get hasPendingInlineSidecarHostPoll => _inlineSidecarHostPollPending;
  bool get hasPendingViewportPresentationHostPoll =>
      _viewportPresentationHostPollPending;
  FlarkV3ParserHostPollTicket? get activeHostPollTicket =>
      _activeHostPollTicket;
  FlarkV3ParserInlineSidecarHostPollTicket?
  get activeInlineSidecarHostPollTicket => _activeInlineSidecarPollTicket;
  FlarkV3ParserViewportPresentationHostPollTicket?
  get activeViewportPresentationHostPollTicket =>
      _activeViewportPresentationPollTicket;
  bool get parserClosed => _parserClosed;
  bool get parserDrained => _parserDrained;
  bool get hostDrained => _hostDrained;
  int? get workerGeneration => _workerGeneration;
  FlarkV3ParserSessionBinding get parserBinding => _binding;
  FlarkV3ParserFailed? get lastFailure => _lastFailure;
  FlarkV3ParserPublicationFailed? get lastPublicationFailure =>
      _lastPublicationFailure;
  FlarkV3ParserInlineSidecarFailed? get lastInlineSidecarFailure =>
      _lastInlineSidecarFailure;
  FlarkV3ParserViewportPresentationFailed?
  get lastViewportPresentationFailure => _lastViewportPresentationFailure;
  FlarkV3HostRejection? get lastHostRejection => _lastHostRejection;
  int get inlinePresentationGeneration => _inlinePresentationGeneration;

  /// Caller-side observation generation for terminal inline attempts.
  ///
  /// This advances once when an exact-source attempt commits or when its host
  /// abort completes. Packet progress and delivery acknowledgement do not
  /// advance it.
  int get inlineAttemptOutcomeGeneration => _inlineAttemptOutcomeGeneration;
  int get viewportPresentationAttemptOutcomeGeneration =>
      _viewportPresentationAttemptOutcomeGeneration;
  int? get lastViewportPresentationUnavailableGeneration =>
      _lastViewportPresentationUnavailableGeneration;
  int? get lastViewportPresentationUnavailableReason =>
      _lastViewportPresentationUnavailableReason;

  /// Coalesces one exact-base inline refinement demand into the next bounded
  /// driver action and returns its monotonic parser generation.
  int requestInlineRefinement({
    required int utf16Offset,
    FlarkV3InlinePointAffinity affinity = FlarkV3InlinePointAffinity.after,
    FlarkV3InlineRefinementTarget target =
        FlarkV3InlineRefinementTarget.automatic,
  }) {
    _requireWritable();
    if (_state != FlarkV3SessionDriverState.open ||
        _inlineSidecarTransport == null ||
        !_session.supportsInlineSidecars ||
        !_session.sourceWorkerSynchronized) {
      throw StateError(
        'Inline refinement requires one open, synchronized sidecar session.',
      );
    }
    final presentation = _session.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation) {
      throw StateError(
        'Inline refinement requires an exact-current structural base.',
      );
    }
    if (_undeliveredInlineSidecarRecoveryAck != null) {
      throw StateError(
        'Inline refinement waits for structural recovery after lost delivery.',
      );
    }
    if (_nextInlineRefinementGeneration > flarkV3TransportV1Maximum) {
      throw StateError('Inline refinement generation lane exhausted.');
    }
    final generation = _nextInlineRefinementGeneration;
    final request = FlarkV3ParserRefineInline(
      binding: _binding,
      refinementGeneration: generation,
      sourceVersion: _session.sourceVersion,
      baseAck: presentation.ack,
      byteOffset: _session.source.utf16ToUtf8(utf16Offset),
      utf16Offset: utf16Offset,
      affinity: affinity,
      target: target,
    );
    _nextInlineRefinementGeneration += 1;
    _pendingViewportPresentation = null;
    _pendingInlineRefinement = request;
    return generation;
  }

  /// Coalesces one exact structural page into the passive viewport lane.
  ///
  /// The focused inline lane remains higher priority and clears an unsent
  /// passive request. The producer authenticates [startBlockOrdinal] and the
  /// exact source cut before doing any inline work.
  int requestViewportPresentation({
    required int requestedStartUtf8,
    required int requestedStartUtf16,
    required int requestedEndUtf8,
    required int requestedEndUtf16,
    required FlarkV3ProtocolU64 startBlockOrdinal,
    required FlarkV3ParserViewportPresentationLimits limits,
  }) {
    _requireWritable();
    if (_state != FlarkV3SessionDriverState.open ||
        _viewportPresentationTransport == null ||
        !_session.supportsViewportPresentation ||
        !_session.sourceWorkerSynchronized) {
      throw StateError(
        'Viewport presentation requires one open, synchronized session.',
      );
    }
    final presentation = _session.presentationState;
    if (presentation is! FlarkV3ExactStructuralPresentation) {
      throw StateError(
        'Viewport presentation requires an exact-current structural base.',
      );
    }
    if (_nextViewportPresentationGeneration > flarkV3TransportV1Maximum) {
      throw StateError('Viewport presentation generation lane exhausted.');
    }
    if (_undeliveredViewportPresentationRecoveryAck != null) {
      throw StateError(
        'Viewport presentation waits for structural recovery after lost '
        'delivery.',
      );
    }
    final generation = _nextViewportPresentationGeneration;
    _nextViewportPresentationGeneration += 1;
    _lastViewportPresentationUnavailableGeneration = null;
    _lastViewportPresentationUnavailableReason = null;
    _pendingViewportPresentation = FlarkV3ParserPresentViewport(
      binding: _binding,
      viewportGeneration: generation,
      sourceVersion: _session.sourceVersion,
      baseAck: presentation.ack,
      requestedStartUtf8: requestedStartUtf8,
      requestedStartUtf16: requestedStartUtf16,
      requestedEndUtf8: requestedEndUtf8,
      requestedEndUtf16: requestedEndUtf16,
      startBlockOrdinal: startBlockOrdinal,
      startUtf8: requestedStartUtf8,
      startUtf16: requestedStartUtf16,
      limits: limits,
    );
    return generation;
  }

  /// Coalesces any number of caller-side edits into one future source lease.
  void markDirty() {
    _requireWritable();
    _sourceDirty = _session.hasPendingSourceWorkerSync;
    _pendingInlineRefinement = null;
    _pendingViewportPresentation = null;
    _lastIssuedViewportPresentationGeneration = 0;
    _issuedViewportPresentationRequest = null;
    _clearInlineSidecarDriverState();
    _clearViewportPresentationDriverState();
    _undeliveredViewportPresentationRecoveryAck = null;
    _undeliveredInlineSidecarRecoveryAck = null;
    if (_activeOffer != null &&
        _publicationState !=
            FlarkV3PublicationDriverState.awaitingDeliveryAck) {
      // Host source adoption synchronously supersedes staging by contract, so
      // there is no remaining offer to poll. The parser learns that its old
      // publication lost authority while the newest source sync coalesces.
      _activeOffer = null;
      _activeHostPollTicket = null;
      _expectedPacketCreditFrameOrdinal = null;
      _publicationState = FlarkV3PublicationDriverState.idle;
      _hostPollPending = false;
    }
  }

  /// Performs at most one parser event, control, host, or source dispatch.
  FlarkV3SessionPumpReceipt pump() {
    if (_state == FlarkV3SessionDriverState.closed) {
      return const FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.idle,
        needsMoreWork: false,
      );
    }

    final event = _pendingEvent;
    if (event != null) {
      _pendingEvent = null;
      switch (event) {
        case FlarkV3ParserEvent():
          _handleParserEvent(event);
        case FlarkV3ParserInlineSidecarEvent():
          _handleInlineSidecarEvent(event);
        case FlarkV3ParserViewportPresentationEvent():
          _handleViewportPresentationEvent(event);
        default:
          throw StateError('Driver pending-event lane held an unknown value.');
      }
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.parserEvent,
        needsMoreWork: _needsPump,
      );
    }

    if (_closeRequested) {
      _latchClose();
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.closeLatch,
        needsMoreWork: _needsPump,
      );
    }

    if (_parserDrainPending && _activeDrainGrant == null) {
      _grantParserDrain();
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.parserDrain,
        needsMoreWork: _needsPump,
      );
    }

    if (_hostPollPending) {
      _pollHostOnce();
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.hostPoll,
        needsMoreWork: _needsPump,
      );
    }

    if (_inlineSidecarHostPollPending) {
      _pollInlineSidecarHostOnce();
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.inlineSidecarHostPoll,
        needsMoreWork: _needsPump,
      );
    }

    if (_viewportPresentationHostPollPending &&
        _pendingInlineRefinement != null) {
      _preemptViewportPresentationHostPollForInline();
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.viewportPresentationHostPoll,
        needsMoreWork: _needsPump,
      );
    }

    if (_viewportPresentationHostPollPending) {
      _pollViewportPresentationHostOnce();
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.viewportPresentationHostPoll,
        needsMoreWork: _needsPump,
      );
    }

    if (_pendingInlineRefinement != null &&
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState.idle) {
      // Once Begin has been admitted, the viewport lane owns host state until
      // one explicit host-poll rejection or exact delivery terminates it. The
      // next parser event owns the wakeup edge. Dispatching focused work while
      // that event is in flight could replay it from the endpoint's deferred
      // command cell before Dart has returned the viewport's host outcome.
      return const FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.idle,
        needsMoreWork: false,
      );
    }

    final inlineRefinement = _pendingInlineRefinement;
    if (inlineRefinement != null) {
      _pendingInlineRefinement = null;
      _lastIssuedInlineRefinementGeneration =
          inlineRefinement.refinementGeneration;
      _transport.send(inlineRefinement);
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.inlineRefinementRequest,
        needsMoreWork: _needsPump,
      );
    }

    final viewportPresentation = _pendingViewportPresentation;
    if (viewportPresentation != null) {
      _pendingViewportPresentation = null;
      _lastIssuedViewportPresentationGeneration =
          viewportPresentation.viewportGeneration;
      _issuedViewportPresentationRequest = viewportPresentation;
      _transport.send(viewportPresentation);
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.viewportPresentationRequest,
        needsMoreWork: _needsPump,
      );
    }

    if (_state != FlarkV3SessionDriverState.open ||
        !_sourceDirty ||
        _sourceLease != null) {
      return FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.idle,
        needsMoreWork: _needsPump,
      );
    }

    if (!_session.hasPendingSourceWorkerSync) {
      _sourceDirty = false;
      return const FlarkV3SessionPumpReceipt(
        action: FlarkV3SessionPumpAction.idle,
        needsMoreWork: false,
      );
    }

    final lease = _session.beginSourceWorkerSync();
    if (lease.sourceSessionIdentity != _binding.sourceSessionIdentity ||
        lease.workerGeneration != _binding.workerGeneration) {
      _session.releaseSourceWorkerSyncLease(lease.leaseId);
      _state = FlarkV3SessionDriverState.faulted;
      throw StateError('Source lease crossed the established parser binding.');
    }
    _sourceLease = lease;
    // Installing newer source is itself the stronger authority transition.
    // On accepted installation the endpoint enters start_source_facts, whose
    // first transition is cancel_derived, before it scans the new source. Do
    // not also enqueue Supersede: an event may already be outstanding but
    // still in transit to Dart, and two speculative commands would overflow
    // the platform endpoint's single bounded deferred-command cell. Edits
    // made while this lease is live stay coalesced in the source journal; the
    // next source command is not admitted until this lease's event is receipted.
    _transport.send(FlarkV3ParserSynchronizeSource(lease));
    return FlarkV3SessionPumpReceipt(
      action: FlarkV3SessionPumpAction.sourceSync,
      needsMoreWork: _needsPump,
    );
  }

  /// Starts a clean source replica after a terminal worker failure.
  FlarkDocumentSourceWorkerRestartReceipt restart() {
    if (_state != FlarkV3SessionDriverState.faulted ||
        _lastFailure == null ||
        _pendingEvent != null ||
        _sourceLease != null ||
        _hostPollPending ||
        _inlineSidecarHostPollPending ||
        _viewportPresentationHostPollPending ||
        _activeHostPollTicket != null ||
        _activeInlineSidecarPollTicket != null ||
        _activeViewportPresentationPollTicket != null ||
        _activeDrainGrant != null ||
        _closeRequested) {
      throw StateError(
        'Only a quiet driver faulted by a terminal parser failure can restart.',
      );
    }
    if (_publicationState != FlarkV3PublicationDriverState.idle ||
        _committedAckAwaitingDelivery != null ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.idle ||
        _committedInlineSidecarAckAwaitingDelivery != null ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState.idle ||
        _committedViewportPresentationAckAwaitingDelivery != null) {
      throw StateError(
        'Publication lanes must abort or deliver exactly before restart.',
      );
    }
    final receipt = _session.restartSourceWorker();
    _pendingEvent = null;
    _sourceLease = null;
    _sourceDirty = _session.hasPendingSourceWorkerSync;
    _hostPollPending = false;
    _inlineSidecarHostPollPending = false;
    _viewportPresentationHostPollPending = false;
    _activeHostPollTicket = null;
    _activeInlineSidecarPollTicket = null;
    _activeViewportPresentationPollTicket = null;
    _expectedPacketCreditFrameOrdinal = null;
    _expectedInlineSidecarPacketCreditFrameOrdinal = null;
    _expectedViewportPresentationPacketCreditFrameOrdinal = null;
    _activeInlineSidecarOffer = null;
    _committedInlineSidecarAckAwaitingDelivery = null;
    _inlineSidecarPublicationState =
        FlarkV3InlineSidecarPublicationDriverState.idle;
    _activeViewportPresentationOffer = null;
    _activeViewportPresentationCommit = null;
    _committedViewportPresentationAckAwaitingDelivery = null;
    _viewportPresentationPublicationState =
        FlarkV3ViewportPresentationPublicationDriverState.idle;
    _pendingInlineRefinement = null;
    _pendingViewportPresentation = null;
    _lastIssuedViewportPresentationGeneration = 0;
    _issuedViewportPresentationRequest = null;
    _lastIssuedInlineRefinementGeneration = 0;
    _workerGeneration = receipt.workerGeneration;
    _binding = _binding.nextGeneration(receipt.workerGeneration);
    _expectedOpenMode = FlarkV3ParserOpenMode.recovery;
    _lastHandledEventId = 0;
    _lastAcceptedParseGeneration = 0;
    _lastFailure = null;
    _lastPublicationFailure = null;
    _lastInlineSidecarFailure = null;
    _lastViewportPresentationFailure = null;
    _lastHostRejection = null;
    _parserClosed = false;
    _parserDrained = false;
    _parserDrainPending = false;
    _activeDrainGrant = null;
    _state = FlarkV3SessionDriverState.opening;
    _transport.send(
      FlarkV3ParserOpen(
        binding: _binding,
        mode: FlarkV3ParserOpenMode.recovery,
      ),
    );
    return receipt;
  }

  /// Begins bounded shutdown and waits for both worker and host-store drain.
  void beginClose() {
    if (_state == FlarkV3SessionDriverState.closed ||
        _state == FlarkV3SessionDriverState.closing) {
      return;
    }
    if (_closeRequested) return;
    _closeRequested = true;
    if (_pendingEvent == null) _latchClose();
  }

  void _latchClose() {
    if (!_closeRequested) {
      throw StateError('Parser close was not requested.');
    }
    _closeRequested = false;
    final lease = _sourceLease;
    if (lease != null) {
      _session.releaseSourceWorkerSyncLease(lease.leaseId);
      _sourceLease = null;
    }
    _sourceDirty = false;
    _pendingInlineRefinement = null;
    _pendingViewportPresentation = null;
    _lastIssuedViewportPresentationGeneration = 0;
    _issuedViewportPresentationRequest = null;
    _clearInlineSidecarDriverState();
    _clearViewportPresentationDriverState();
    _activeHostPollTicket = null;
    final result = _session.close();
    _state = FlarkV3SessionDriverState.closing;
    _parserDrainPending = true;
    _parserDrained = false;
    _activeDrainGrant = null;
    _hostDrained = false;
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _hostPollPending = true;
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _hostPollPending = false;
    }
    _transport.send(FlarkV3ParserBeginClose(_workerGeneration));
  }

  /// Bounded fallback for an unavailable worker or host close acknowledgement.
  void forceClose() {
    if (_state == FlarkV3SessionDriverState.closed) return;
    try {
      if (_state != FlarkV3SessionDriverState.closing) {
        beginClose();
        if (_closeRequested) {
          _pendingEvent = null;
          _latchClose();
        }
      }
    } finally {
      // Emergency retirement must finish locally even when the platform
      // transport is already faulted and rejects the graceful close command.
      // Otherwise the executor becomes disposed while the driver remains in
      // `closing`, leaving every later close() completion permanently pending.
      _pendingEvent = null;
      _hostPollPending = false;
      _inlineSidecarHostPollPending = false;
      _viewportPresentationHostPollPending = false;
      _activeHostPollTicket = null;
      _activeInlineSidecarPollTicket = null;
      _activeViewportPresentationPollTicket = null;
      _pendingInlineRefinement = null;
      _pendingViewportPresentation = null;
      _lastIssuedViewportPresentationGeneration = 0;
      _issuedViewportPresentationRequest = null;
      _parserDrainPending = false;
      _activeDrainGrant = null;
      try {
        _transport.close();
      } finally {
        _state = FlarkV3SessionDriverState.closed;
      }
    }
  }

  bool get _needsPump =>
      _pendingEvent != null ||
      _closeRequested ||
      (_parserDrainPending && _activeDrainGrant == null) ||
      _hostPollPending ||
      _inlineSidecarHostPollPending ||
      _viewportPresentationHostPollPending ||
      _pendingInlineRefinement != null ||
      _pendingViewportPresentation != null ||
      (_state == FlarkV3SessionDriverState.open &&
          _sourceDirty &&
          _sourceLease == null &&
          _session.hasPendingSourceWorkerSync);

  void _enqueueParserEvent(FlarkV3ParserEvent event) =>
      _enqueueCreditedEvent(event);

  void _enqueueInlineSidecarEvent(FlarkV3ParserInlineSidecarEvent event) =>
      _enqueueCreditedEvent(event);

  void _enqueueViewportPresentationEvent(
    FlarkV3ParserViewportPresentationEvent event,
  ) => _enqueueCreditedEvent(event);

  void _enqueueCreditedEvent(Object event) {
    if (_state == FlarkV3SessionDriverState.closed) {
      throw StateError('A closed parser transport emitted an event.');
    }
    if (_pendingEvent != null) {
      throw StateError(
        'Parser transport exceeded its shared single event credit.',
      );
    }
    _pendingEvent = event;
  }

  void _handleParserEvent(FlarkV3ParserEvent event) {
    if (!_isFreshCurrentEvent(event)) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    _lastHandledEventId = event.eventId;

    switch (event) {
      case FlarkV3ParserOpened():
        _handleOpened(event);
      case FlarkV3ParserSourceSynchronized():
        _handleSourceAcknowledgement(event);
      case FlarkV3ParserSourceFactsPage():
        _handleSourceFactsPage(event);
      case FlarkV3ParserSourceFactsCompleted():
        _handleSourceFactsCompletion(event);
      case FlarkV3ParserSourceFactsDeltaBegin():
        _handleSourceFactsDeltaBegin(event);
      case FlarkV3ParserSourceFactsDeltaPage():
        _handleSourceFactsDeltaPage(event);
      case FlarkV3ParserSourceFactsDeltaCompleted():
        _handleSourceFactsDeltaCompletion(event);
      case FlarkV3ParserInlineRefinementUnavailable():
        _handleInlineRefinementUnavailable(event);
      case FlarkV3ParserViewportPresentationUnavailable():
        _handleViewportPresentationUnavailable(event);
      case FlarkV3ParserPublicationBegin():
        _handlePublicationBegin(event);
      case FlarkV3ParserPublicationPacket():
        _handlePublicationPacket(event);
      case FlarkV3ParserPublicationCommitRequested():
        _handlePublicationCommit(event);
      case FlarkV3ParserPublicationDeliveryAcknowledged():
        _handlePublicationDelivery(event);
      case FlarkV3ParserPublicationAbortRequested():
        _handlePublicationAbort(event);
      case FlarkV3ParserPublicationFailed():
        _handlePublicationFailure(event);
      case FlarkV3ParserFailed():
        _handleFailure(event);
      case FlarkV3ParserDrainProgress():
        _handleDrainProgress(event);
      case FlarkV3ParserClosed():
        _handleClosed(event);
    }
  }

  void _handleOpened(FlarkV3ParserOpened event) {
    if (_state != FlarkV3SessionDriverState.opening ||
        event.binding != _binding ||
        event.mode != _expectedOpenMode) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _state = FlarkV3SessionDriverState.open;
    _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
  }

  void _handleSourceAcknowledgement(FlarkV3ParserSourceSynchronized event) {
    if (_state != FlarkV3SessionDriverState.open) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final lease = _sourceLease;
    if (lease == null) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }

    final bindsLease = _acknowledgementBindsLease(event.acknowledgement, lease);
    final sourceReceipt = _session.acknowledgeSourceWorkerSync(
      event.acknowledgement,
    );
    if (bindsLease || !_session.ownsSourceWorkerSyncLease(lease.leaseId)) {
      // An exact ACK for an invalidated draining lease is intentionally stale,
      // but it still returns the only lease credit and permits latest rebase.
      // A malformed completion for this exact live lease may also poison and
      // release it inside the source session; never retain phantom credit.
      _sourceLease = null;
      _sourceDirty = _session.hasPendingSourceWorkerSync;
    }
    _returnEventCredit(
      event,
      sourceReceipt.disposition ==
              FlarkV3SourceWorkerSyncAckDisposition.acknowledged
          ? FlarkV3ParserEventDisposition.accepted
          : FlarkV3ParserEventDisposition.stale,
      sourceSync: sourceReceipt,
    );
  }

  void _handleSourceFactsPage(FlarkV3ParserSourceFactsPage event) {
    if (_state != FlarkV3SessionDriverState.open || event.binding != _binding) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final staged = _session.stageCanonicalSourceFactCheckpointPage(event.page);
    _returnEventCredit(event, switch (staged.disposition) {
      FlarkV3SourceFactStageDisposition.staged =>
        FlarkV3ParserEventDisposition.accepted,
      FlarkV3SourceFactStageDisposition.stale =>
        FlarkV3ParserEventDisposition.stale,
      FlarkV3SourceFactStageDisposition.rejected =>
        FlarkV3ParserEventDisposition.rejected,
    });
  }

  void _handleSourceFactsCompletion(FlarkV3ParserSourceFactsCompleted event) {
    if (_state != FlarkV3SessionDriverState.open || event.binding != _binding) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final committed = _session.commitCanonicalSourceFactCertification(
      event.completion,
    );
    switch (committed.promotion.disposition) {
      case FlarkV3SourcePromotionDisposition.promoted:
        final proof = committed.promotion.canonicalProof;
        if (proof == null) {
          _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
          return;
        }
        _returnEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
          sourceCertification: proof,
        );
      case FlarkV3SourcePromotionDisposition.stale:
        _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      case FlarkV3SourcePromotionDisposition.rejected:
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handleSourceFactsDeltaBegin(FlarkV3ParserSourceFactsDeltaBegin event) {
    if (_state != FlarkV3SessionDriverState.open || event.binding != _binding) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final base = _session.retainedCanonicalSourceFactDeltaBase;
    if (base == null) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final opened = _session.beginCanonicalSourceFactDelta(
      event.header.bindBase(base),
    );
    _returnEventCredit(event, switch (opened.disposition) {
      FlarkV3CanonicalSourceFactDeltaBeginDisposition.accepted =>
        FlarkV3ParserEventDisposition.accepted,
      FlarkV3CanonicalSourceFactDeltaBeginDisposition.stale =>
        FlarkV3ParserEventDisposition.stale,
      FlarkV3CanonicalSourceFactDeltaBeginDisposition.rejected =>
        FlarkV3ParserEventDisposition.rejected,
    });
  }

  void _handleSourceFactsDeltaPage(FlarkV3ParserSourceFactsDeltaPage event) {
    if (_state != FlarkV3SessionDriverState.open || event.binding != _binding) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final staged = _session.stageCanonicalSourceFactDeltaCheckpointPage(
      event.page,
    );
    _returnEventCredit(event, switch (staged.disposition) {
      FlarkV3SourceFactStageDisposition.staged =>
        FlarkV3ParserEventDisposition.accepted,
      FlarkV3SourceFactStageDisposition.stale =>
        FlarkV3ParserEventDisposition.stale,
      FlarkV3SourceFactStageDisposition.rejected =>
        FlarkV3ParserEventDisposition.rejected,
    });
  }

  void _handleSourceFactsDeltaCompletion(
    FlarkV3ParserSourceFactsDeltaCompleted event,
  ) {
    if (_state != FlarkV3SessionDriverState.open || event.binding != _binding) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      return;
    }
    final promoted = _session.commitCanonicalSourceFactDeltaCertification(
      event.completion,
    );
    switch (promoted.disposition) {
      case FlarkV3SourcePromotionDisposition.promoted:
        final proof = promoted.canonicalProof;
        if (proof == null) {
          _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
          return;
        }
        _returnEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
          sourceCertification: proof,
        );
      case FlarkV3SourcePromotionDisposition.stale:
        _returnEventCredit(event, FlarkV3ParserEventDisposition.stale);
      case FlarkV3SourcePromotionDisposition.rejected:
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handlePublicationBegin(FlarkV3ParserPublicationBegin event) {
    final begin = event.begin;
    final authority = _publicationAuthority;
    final recoveryBase = _undeliveredAckRecoveryBase;
    if (_state != FlarkV3SessionDriverState.open ||
        authority == null ||
        !authority.admits(begin) ||
        _publicationState != FlarkV3PublicationDriverState.idle ||
        begin.sourceVersion != _session.sourceVersion ||
        begin.parseGeneration <= _lastAcceptedParseGeneration ||
        (recoveryBase != null &&
            (begin.mode != FlarkV3PublicationMode.fullSnapshot ||
                begin.publicationSession == recoveryBase.publicationSession))) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }

    final result = _session.beginOffer(begin);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _activeOffer = begin;
        _publicationState = FlarkV3PublicationDriverState.acceptingPackets;
        _lastAcceptedParseGeneration = begin.parseGeneration;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handlePublicationPacket(FlarkV3ParserPublicationPacket event) {
    final active = _activeOffer;
    if (_state != FlarkV3SessionDriverState.open ||
        _publicationState != FlarkV3PublicationDriverState.acceptingPackets ||
        active == null ||
        event.packet.offerId != active.offerId) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    final result = _session.admitPacket(event.packet);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _publicationState = FlarkV3PublicationDriverState.awaitingPacketCredit;
        _expectedPacketCreditFrameOrdinal =
            event.packet.firstFrameOrdinal + event.packet.frameCount;
        _activeHostPollTicket = FlarkV3ParserHostPollTicket(
          binding: _binding,
          pollTicket: event.eventId,
          offerId: event.packet.offerId,
          phase: FlarkV3ParserHostPollPhase.packetCredit,
        );
        _hostPollPending = true;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handlePublicationCommit(FlarkV3ParserPublicationCommitRequested event) {
    final active = _activeOffer;
    if (_state != FlarkV3SessionDriverState.open ||
        _publicationState != FlarkV3PublicationDriverState.acceptingPackets ||
        active == null ||
        event.request.offerId != active.offerId) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    final result = _session.requestCommit(event.request);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _publicationState = FlarkV3PublicationDriverState.awaitingCommit;
        _activeHostPollTicket = FlarkV3ParserHostPollTicket(
          binding: _binding,
          pollTicket: event.eventId,
          offerId: event.request.offerId,
          phase: FlarkV3ParserHostPollPhase.commit,
        );
        _hostPollPending = true;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handlePublicationDelivery(
    FlarkV3ParserPublicationDeliveryAcknowledged event,
  ) {
    final pending = _committedAckAwaitingDelivery;
    if ((_state != FlarkV3SessionDriverState.open &&
            _state != FlarkV3SessionDriverState.closing) ||
        _publicationState !=
            FlarkV3PublicationDriverState.awaitingDeliveryAck ||
        pending == null ||
        event.ack != pending ||
        _session.pendingDeliveryAck != pending) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    final result = _session.acknowledgeDelivery(event.ack);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _activeOffer = null;
        _committedAckAwaitingDelivery = null;
        _undeliveredAckRecoveryBase = null;
        _publicationState = FlarkV3PublicationDriverState.idle;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handlePublicationAbort(FlarkV3ParserPublicationAbortRequested event) {
    if (!_publicationCanAbort(event.offerId)) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _beginPublicationAbort(event, event.offerId);
  }

  void _handlePublicationFailure(FlarkV3ParserPublicationFailed event) {
    _lastPublicationFailure = event;
    if (!_publicationCanAbort(event.offerId)) {
      // Once a commit ACK was sent, only exact delivery or session close can
      // clear host backpressure. Do not pretend a parser-local failure nacked
      // an already installed root.
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _beginPublicationAbort(event, event.offerId);
  }

  void _handleInlineSidecarEvent(FlarkV3ParserInlineSidecarEvent event) {
    if (!_isFreshCurrentInlineSidecarEvent(event)) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.stale,
      );
      return;
    }
    _lastHandledEventId = event.eventId;
    switch (event) {
      case FlarkV3ParserInlineSidecarBegin():
        _handleInlineSidecarBegin(event);
      case FlarkV3ParserInlineSidecarPacket():
        _handleInlineSidecarPacket(event);
      case FlarkV3ParserInlineSidecarCommitRequested():
        _handleInlineSidecarCommit(event);
      case FlarkV3ParserInlineSidecarDeliveryAcknowledged():
        _handleInlineSidecarDelivery(event);
      case FlarkV3ParserInlineSidecarAbortRequested():
        _handleInlineSidecarAbort(event);
      case FlarkV3ParserInlineSidecarFailed():
        _handleInlineSidecarFailure(event);
    }
  }

  void _handleInlineSidecarBegin(FlarkV3ParserInlineSidecarBegin event) {
    final begin = event.begin;
    final presentation = _session.presentationState;
    final authority = _publicationAuthority;
    final generation = begin.binding.refinementGeneration;
    if (_state != FlarkV3SessionDriverState.open ||
        _inlineSidecarTransport == null ||
        !_session.supportsInlineSidecars ||
        event.binding != _binding ||
        _publicationState != FlarkV3PublicationDriverState.idle ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.idle ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState.idle ||
        _undeliveredInlineSidecarRecoveryAck != null ||
        presentation is! FlarkV3ExactStructuralPresentation ||
        begin.baseAck != presentation.ack ||
        begin.baseAck.sourceVersion != _session.sourceVersion ||
        authority == null ||
        begin.baseAck.grammarRevision != authority.grammarRevision ||
        begin.baseAck.syntaxProfile != authority.syntaxProfile ||
        begin.baseAck.authorityMask != authority.authorityMask ||
        begin.binding.parserProfile != authority.syntaxProfile ||
        !generation.fitsU32 ||
        generation.lowWord != _lastIssuedInlineRefinementGeneration) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }

    final result = _session.beginInlineSidecarOffer(begin);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _activeInlineSidecarOffer = begin;
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.acceptingPackets;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleInlineSidecarPacket(FlarkV3ParserInlineSidecarPacket event) {
    final active = _activeInlineSidecarOffer;
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.acceptingPackets ||
        active == null ||
        event.packet.offerId != active.offerId) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    final result = _session.admitInlineSidecarPacket(event.packet);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.awaitingPacketCredit;
        _expectedInlineSidecarPacketCreditFrameOrdinal =
            event.packet.firstFrameOrdinal + event.packet.frameCount;
        _activeInlineSidecarPollTicket =
            FlarkV3ParserInlineSidecarHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: event.packet.offerId,
              phase: FlarkV3ParserInlineSidecarHostPollPhase.packetCredit,
            );
        _inlineSidecarHostPollPending = true;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleInlineSidecarCommit(
    FlarkV3ParserInlineSidecarCommitRequested event,
  ) {
    final active = _activeInlineSidecarOffer;
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.acceptingPackets ||
        active == null ||
        event.request.offerId != active.offerId) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    final result = _session.requestInlineSidecarCommit(event.request);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _activeInlineSidecarCommit = event.request;
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.awaitingCommit;
        _activeInlineSidecarPollTicket =
            FlarkV3ParserInlineSidecarHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: event.request.offerId,
              phase: FlarkV3ParserInlineSidecarHostPollPhase.commit,
            );
        _inlineSidecarHostPollPending = true;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleInlineSidecarDelivery(
    FlarkV3ParserInlineSidecarDeliveryAcknowledged event,
  ) {
    final pending = _committedInlineSidecarAckAwaitingDelivery;
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.awaitingDeliveryAck ||
        pending == null ||
        event.ack != pending ||
        _session.pendingInlineSidecarDeliveryAck != pending) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    final result = _session.acknowledgeInlineSidecarDelivery(event.ack);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _clearInlineSidecarDriverState();
        _undeliveredInlineSidecarRecoveryAck = null;
        _clearViewportPresentationDriverState();
        _undeliveredViewportPresentationRecoveryAck = null;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleInlineSidecarAbort(
    FlarkV3ParserInlineSidecarAbortRequested event,
  ) {
    if (!_inlineSidecarCanAbort(event.offerId)) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    _beginInlineSidecarAbort(event, event.offerId);
  }

  void _handleInlineSidecarFailure(FlarkV3ParserInlineSidecarFailed event) {
    _lastInlineSidecarFailure = event;
    if (!_inlineSidecarCanAbort(event.offerId)) {
      _returnInlineSidecarEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    _beginInlineSidecarAbort(event, event.offerId);
  }

  void _handleViewportPresentationEvent(
    FlarkV3ParserViewportPresentationEvent event,
  ) {
    if (!_isFreshCurrentViewportPresentationEvent(event)) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.stale,
      );
      return;
    }
    _lastHandledEventId = event.eventId;
    switch (event) {
      case FlarkV3ParserViewportPresentationBegin():
        _handleViewportPresentationBegin(event);
      case FlarkV3ParserViewportPresentationPacket():
        _handleViewportPresentationPacket(event);
      case FlarkV3ParserViewportPresentationCommitRequested():
        _handleViewportPresentationCommit(event);
      case FlarkV3ParserViewportPresentationDeliveryAcknowledged():
        _handleViewportPresentationDelivery(event);
      case FlarkV3ParserViewportPresentationAbortRequested():
        _handleViewportPresentationAbort(event);
      case FlarkV3ParserViewportPresentationFailed():
        _handleViewportPresentationFailure(event);
    }
  }

  void _handleViewportPresentationBegin(
    FlarkV3ParserViewportPresentationBegin event,
  ) {
    final begin = event.begin;
    final presentation = _session.presentationState;
    final authority = _publicationAuthority;
    final request = _issuedViewportPresentationRequest;
    if (_state != FlarkV3SessionDriverState.open ||
        _viewportPresentationTransport == null ||
        !_session.supportsViewportPresentation ||
        event.binding != _binding ||
        _publicationState != FlarkV3PublicationDriverState.idle ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.idle ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState.idle ||
        presentation is! FlarkV3ExactStructuralPresentation ||
        begin.baseAck != presentation.ack ||
        begin.baseAck.sourceVersion != _session.sourceVersion ||
        authority == null ||
        begin.baseAck.grammarRevision != authority.grammarRevision ||
        begin.baseAck.syntaxProfile != authority.syntaxProfile ||
        begin.baseAck.authorityMask != authority.authorityMask ||
        request == null ||
        !_viewportBeginMatchesRequest(begin, request)) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }

    final result = _session.beginViewportPresentationOffer(begin);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _activeViewportPresentationOffer = begin;
        _activeViewportPresentationCommit = null;
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState.acceptingPackets;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleViewportPresentationPacket(
    FlarkV3ParserViewportPresentationPacket event,
  ) {
    final active = _activeViewportPresentationOffer;
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState
                .acceptingPackets ||
        active == null ||
        event.packet.offerId != active.offerId) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    final result = _session.admitViewportPresentationPacket(event.packet);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState
                .awaitingPacketCredit;
        _expectedViewportPresentationPacketCreditFrameOrdinal =
            event.packet.firstFrameOrdinal + event.packet.frameCount;
        _activeViewportPresentationPollTicket =
            FlarkV3ParserViewportPresentationHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: event.packet.offerId,
              phase:
                  FlarkV3ParserViewportPresentationHostPollPhase.packetCredit,
            );
        _viewportPresentationHostPollPending = true;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleViewportPresentationCommit(
    FlarkV3ParserViewportPresentationCommitRequested event,
  ) {
    final active = _activeViewportPresentationOffer;
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState
                .acceptingPackets ||
        active == null ||
        event.request.offerId != active.offerId) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    final result = _session.requestViewportPresentationCommit(event.request);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _activeViewportPresentationCommit = event.request;
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState.awaitingCommit;
        _activeViewportPresentationPollTicket =
            FlarkV3ParserViewportPresentationHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: event.request.offerId,
              phase: FlarkV3ParserViewportPresentationHostPollPhase.commit,
            );
        _viewportPresentationHostPollPending = true;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleViewportPresentationDelivery(
    FlarkV3ParserViewportPresentationDeliveryAcknowledged event,
  ) {
    final pending = _committedViewportPresentationAckAwaitingDelivery;
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState
                .awaitingDeliveryAck ||
        pending == null ||
        event.ack != pending ||
        _session.pendingViewportPresentationDeliveryAck != pending) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    final result = _session.acknowledgeViewportPresentationDelivery(event.ack);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _clearViewportPresentationDriverState();
        _issuedViewportPresentationRequest = null;
        _lastIssuedViewportPresentationGeneration = 0;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  void _handleViewportPresentationAbort(
    FlarkV3ParserViewportPresentationAbortRequested event,
  ) {
    if (!_viewportPresentationCanAbort(event.offerId)) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    _beginViewportPresentationAbort(event, event.offerId);
  }

  void _handleViewportPresentationFailure(
    FlarkV3ParserViewportPresentationFailed event,
  ) {
    _lastViewportPresentationFailure = event;
    if (!_viewportPresentationCanAbort(event.offerId)) {
      _returnViewportPresentationEventCredit(
        event,
        FlarkV3ParserEventDisposition.rejected,
      );
      return;
    }
    _beginViewportPresentationAbort(event, event.offerId);
  }

  bool _viewportPresentationCanAbort(FlarkV3OfferId offerId) =>
      _state == FlarkV3SessionDriverState.open &&
      _activeViewportPresentationOffer?.offerId == offerId &&
      _viewportPresentationPublicationState !=
          FlarkV3ViewportPresentationPublicationDriverState.idle &&
      _viewportPresentationPublicationState !=
          FlarkV3ViewportPresentationPublicationDriverState
              .awaitingDeliveryAck &&
      _viewportPresentationPublicationState !=
          FlarkV3ViewportPresentationPublicationDriverState.aborting;

  void _beginViewportPresentationAbort(
    FlarkV3ParserViewportPresentationEvent event,
    FlarkV3OfferId offerId,
  ) {
    final result = _session.abortViewportPresentationOffer(offerId);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState.aborting;
        _activeViewportPresentationPollTicket =
            FlarkV3ParserViewportPresentationHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: offerId,
              phase: FlarkV3ParserViewportPresentationHostPollPhase.abort,
            );
        _viewportPresentationHostPollPending = true;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnViewportPresentationEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  bool _inlineSidecarCanAbort(FlarkV3OfferId offerId) =>
      _state == FlarkV3SessionDriverState.open &&
      _activeInlineSidecarOffer?.offerId == offerId &&
      _inlineSidecarPublicationState !=
          FlarkV3InlineSidecarPublicationDriverState.idle &&
      _inlineSidecarPublicationState !=
          FlarkV3InlineSidecarPublicationDriverState.awaitingDeliveryAck &&
      _inlineSidecarPublicationState !=
          FlarkV3InlineSidecarPublicationDriverState.aborting;

  void _beginInlineSidecarAbort(
    FlarkV3ParserInlineSidecarEvent event,
    FlarkV3OfferId offerId,
  ) {
    final result = _session.abortInlineSidecarOffer(offerId);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.aborting;
        _activeInlineSidecarPollTicket =
            FlarkV3ParserInlineSidecarHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: offerId,
              phase: FlarkV3ParserInlineSidecarHostPollPhase.abort,
            );
        _inlineSidecarHostPollPending = true;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.accepted,
        );
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnInlineSidecarEventCredit(
          event,
          FlarkV3ParserEventDisposition.rejected,
        );
    }
  }

  bool _publicationCanAbort(FlarkV3OfferId offerId) =>
      _state == FlarkV3SessionDriverState.open &&
      _activeOffer?.offerId == offerId &&
      _publicationState != FlarkV3PublicationDriverState.idle &&
      _publicationState != FlarkV3PublicationDriverState.awaitingDeliveryAck &&
      _publicationState != FlarkV3PublicationDriverState.aborting;

  void _beginPublicationAbort(
    FlarkV3ParserEvent event,
    FlarkV3OfferId offerId,
  ) {
    final result = _session.abortOffer(offerId);
    switch (result) {
      case FlarkV3HostAccepted<FlarkV3HostUnit>():
        _publicationState = FlarkV3PublicationDriverState.aborting;
        _activeHostPollTicket = FlarkV3ParserHostPollTicket(
          binding: _binding,
          pollTicket: event.eventId,
          offerId: offerId,
          phase: FlarkV3ParserHostPollPhase.abort,
        );
        _hostPollPending = true;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
      case FlarkV3HostRejected<FlarkV3HostUnit>(:final rejection):
        _lastHostRejection = rejection;
        _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
    }
  }

  void _handleFailure(FlarkV3ParserFailed event) {
    final lease = _sourceLease;
    if (lease != null) {
      _session.releaseSourceWorkerSyncLease(lease.leaseId);
      _sourceLease = null;
    }
    _sourceDirty = false;
    _lastFailure = event;
    if (_state == FlarkV3SessionDriverState.closing) {
      // This event is terminal for the current parser generation. Host-store
      // retirement remains independently fuelled, so keep closing and count
      // the failed parser endpoint as drained instead of abandoning host drain.
      _parserClosed = true;
      _parserDrained = true;
      _parserDrainPending = false;
      _activeDrainGrant = null;
      _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
      _finishCloseIfDrained();
      return;
    }
    final active = _activeOffer;
    final undelivered = _committedAckAwaitingDelivery;
    if (_publicationState ==
            FlarkV3PublicationDriverState.awaitingDeliveryAck &&
        undelivered != null) {
      // The installed root remains exact host authority, but receipt by the
      // failed worker can no longer be proven. Do not acknowledge it on the
      // worker's behalf. A restarted worker must establish a distinct
      // publication session and replace it with a clean full snapshot.
      _undeliveredAckRecoveryBase = undelivered;
      _activeOffer = null;
      _activeHostPollTicket = null;
      _committedAckAwaitingDelivery = null;
      _publicationState = FlarkV3PublicationDriverState.idle;
      _hostPollPending = false;
    } else if (active != null) {
      final abort = _session.abortOffer(active.offerId);
      if (abort is FlarkV3HostAccepted<FlarkV3HostUnit>) {
        _publicationState = FlarkV3PublicationDriverState.aborting;
        _activeHostPollTicket = FlarkV3ParserHostPollTicket(
          binding: _binding,
          pollTicket: event.eventId,
          offerId: active.offerId,
          phase: FlarkV3ParserHostPollPhase.abort,
        );
        _hostPollPending = true;
      } else {
        _lastHostRejection = (abort as FlarkV3HostRejected).rejection;
      }
    }

    final activeInlineSidecar = _activeInlineSidecarOffer;
    final undeliveredInlineSidecar = _committedInlineSidecarAckAwaitingDelivery;
    if (_inlineSidecarPublicationState ==
            FlarkV3InlineSidecarPublicationDriverState.awaitingDeliveryAck &&
        undeliveredInlineSidecar != null) {
      _undeliveredInlineSidecarRecoveryAck = undeliveredInlineSidecar;
      _clearInlineSidecarDriverState();
    } else if (activeInlineSidecar != null) {
      final abort = _session.abortInlineSidecarOffer(
        activeInlineSidecar.offerId,
      );
      if (abort is FlarkV3HostAccepted<FlarkV3HostUnit>) {
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.aborting;
        _activeInlineSidecarPollTicket =
            FlarkV3ParserInlineSidecarHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: activeInlineSidecar.offerId,
              phase: FlarkV3ParserInlineSidecarHostPollPhase.abort,
            );
        _inlineSidecarHostPollPending = true;
      } else {
        _lastHostRejection = (abort as FlarkV3HostRejected).rejection;
      }
    }

    final activeViewportPresentation = _activeViewportPresentationOffer;
    final undeliveredViewportPresentation =
        _committedViewportPresentationAckAwaitingDelivery;
    if (_viewportPresentationPublicationState ==
            FlarkV3ViewportPresentationPublicationDriverState
                .awaitingDeliveryAck &&
        undeliveredViewportPresentation != null) {
      _undeliveredViewportPresentationRecoveryAck =
          undeliveredViewportPresentation;
      _clearViewportPresentationDriverState();
    } else if (activeViewportPresentation != null) {
      final abort = _session.abortViewportPresentationOffer(
        activeViewportPresentation.offerId,
      );
      if (abort is FlarkV3HostAccepted<FlarkV3HostUnit>) {
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState.aborting;
        _activeViewportPresentationPollTicket =
            FlarkV3ParserViewportPresentationHostPollTicket(
              binding: _binding,
              pollTicket: event.eventId,
              offerId: activeViewportPresentation.offerId,
              phase: FlarkV3ParserViewportPresentationHostPollPhase.abort,
            );
        _viewportPresentationHostPollPending = true;
      } else {
        _lastHostRejection = (abort as FlarkV3HostRejected).rejection;
      }
    }
    _state = FlarkV3SessionDriverState.faulted;
    _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
  }

  void _handleInlineRefinementUnavailable(
    FlarkV3ParserInlineRefinementUnavailable event,
  ) {
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        event.refinementGeneration != _lastIssuedInlineRefinementGeneration ||
        _inlineSidecarPublicationState !=
            FlarkV3InlineSidecarPublicationDriverState.idle ||
        _inlineSidecarHostPollPending) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _lastIssuedInlineRefinementGeneration = 0;
    _clearInlineSidecarDriverState();
    _inlineAttemptOutcomeGeneration += 1;
    _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
  }

  void _handleViewportPresentationUnavailable(
    FlarkV3ParserViewportPresentationUnavailable event,
  ) {
    if (_state != FlarkV3SessionDriverState.open ||
        event.binding != _binding ||
        event.viewportGeneration != _lastIssuedViewportPresentationGeneration ||
        _viewportPresentationPublicationState !=
            FlarkV3ViewportPresentationPublicationDriverState.idle ||
        _viewportPresentationHostPollPending) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _lastIssuedViewportPresentationGeneration = 0;
    _issuedViewportPresentationRequest = null;
    _lastViewportPresentationUnavailableGeneration = event.viewportGeneration;
    _lastViewportPresentationUnavailableReason = event.reasonCode;
    _viewportPresentationAttemptOutcomeGeneration += 1;
    _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
  }

  void _handleDrainProgress(FlarkV3ParserDrainProgress event) {
    final grant = _activeDrainGrant;
    if (_state != FlarkV3SessionDriverState.closing ||
        grant == null ||
        !event.bindsGrant(grant)) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _activeDrainGrant = null;
    if (event.complete) {
      _parserDrained = true;
      _parserDrainPending = false;
    } else {
      _parserDrainPending = true;
    }
    _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
  }

  void _handleClosed(FlarkV3ParserClosed event) {
    if (_state != FlarkV3SessionDriverState.closing) {
      _state = FlarkV3SessionDriverState.faulted;
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    if (!_parserDrained) {
      _returnEventCredit(event, FlarkV3ParserEventDisposition.rejected);
      return;
    }
    _parserClosed = true;
    _returnEventCredit(event, FlarkV3ParserEventDisposition.accepted);
    _finishCloseIfDrained();
  }

  void _grantParserDrain() {
    final grant = FlarkV3ParserDrainGrant(
      binding: _binding,
      drainId: _nextDrainId,
      maximumTransitions: _parserDrainTransitions,
    );
    if (_nextDrainId == flarkV3TransportV1Maximum) {
      throw StateError('Parser drain identity lane exhausted.');
    }
    _nextDrainId += 1;
    _parserDrainPending = false;
    _activeDrainGrant = grant;
    _transport.send(grant);
  }

  void _pollHostOnce() {
    final result = _session.pollHost(_hostPollGrant);
    switch (result) {
      case FlarkV3HostRejected<FlarkV3HostPollOutcome>(:final rejection):
        _lastHostRejection = rejection;
        _hostPollPending = false;
        final ticket = _activeHostPollTicket;
        _activeHostPollTicket = null;
        if (_state == FlarkV3SessionDriverState.closing) {
          // Close polling has no publication cause and no corresponding worker
          // result frame. Retirement failure remains caller-side state.
          return;
        }
        if (ticket != null) {
          _transport.send(
            FlarkV3ParserHostPollRejected(
              ticket: ticket,
              reason: rejection.reason,
            ),
          );
        }
        _state = FlarkV3SessionDriverState.faulted;
      case FlarkV3HostAccepted<FlarkV3HostPollOutcome>(:final value):
        _handleHostPollOutcome(value);
    }
  }

  void _handleHostPollOutcome(FlarkV3HostPollOutcome outcome) {
    switch (outcome) {
      case FlarkV3HostPollPending():
        // The phase remains the authority for whether more poll fuel exists.
        _hostPollPending = true;
      case FlarkV3HostPacketCredit(:final offerId, :final nextFrameOrdinal):
        final active = _activeOffer;
        final ticket = _activeHostPollTicket;
        if (_publicationState !=
                FlarkV3PublicationDriverState.awaitingPacketCredit ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase != FlarkV3ParserHostPollPhase.packetCredit ||
            offerId != active.offerId ||
            nextFrameOrdinal != _expectedPacketCreditFrameOrdinal) {
          _rejectUnexpectedHostOutcome();
          return;
        }
        _expectedPacketCreditFrameOrdinal = null;
        _publicationState = FlarkV3PublicationDriverState.acceptingPackets;
        _hostPollPending = false;
        _activeHostPollTicket = null;
        _transport.send(
          FlarkV3ParserHostPollCompleted(ticket: ticket, outcome: outcome),
        );
      case FlarkV3HostCommitted(:final ack):
        final active = _activeOffer;
        final ticket = _activeHostPollTicket;
        if (_publicationState != FlarkV3PublicationDriverState.awaitingCommit ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase != FlarkV3ParserHostPollPhase.commit ||
            !_ackBindsOffer(ack, active) ||
            _session.pendingDeliveryAck != ack) {
          _rejectUnexpectedHostOutcome();
          return;
        }
        _committedAckAwaitingDelivery = ack;
        _publicationState = FlarkV3PublicationDriverState.awaitingDeliveryAck;
        _clearInlineSidecarDriverState();
        _undeliveredInlineSidecarRecoveryAck = null;
        _hostPollPending = false;
        _activeHostPollTicket = null;
        _transport.send(
          FlarkV3ParserHostPollCompleted(ticket: ticket, outcome: outcome),
        );
      case FlarkV3HostAbortComplete(:final offerId):
        final active = _activeOffer;
        final ticket = _activeHostPollTicket;
        if (_publicationState != FlarkV3PublicationDriverState.aborting ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase != FlarkV3ParserHostPollPhase.abort ||
            offerId != active.offerId) {
          _rejectUnexpectedHostOutcome();
          return;
        }
        _activeOffer = null;
        _expectedPacketCreditFrameOrdinal = null;
        _publicationState = FlarkV3PublicationDriverState.idle;
        _hostPollPending = false;
        _activeHostPollTicket = null;
        _transport.send(
          FlarkV3ParserHostPollCompleted(ticket: ticket, outcome: outcome),
        );
      case FlarkV3HostClosed():
        if (_state != FlarkV3SessionDriverState.closing) {
          _rejectUnexpectedHostOutcome();
          return;
        }
        _hostDrained = true;
        _hostPollPending = false;
        _activeHostPollTicket = null;
        _finishCloseIfDrained();
    }
  }

  void _rejectUnexpectedHostOutcome() {
    const rejection = FlarkV3HostRejection(
      FlarkV3HostRejectReason.invalid,
      'Host poll outcome does not match driver publication state.',
    );
    _lastHostRejection = rejection;
    _hostPollPending = false;
    final ticket = _activeHostPollTicket;
    _activeHostPollTicket = null;
    if (ticket != null) {
      _transport.send(
        FlarkV3ParserHostPollRejected(
          ticket: ticket,
          reason: FlarkV3HostRejectReason.invalid,
        ),
      );
    }
    if (_state != FlarkV3SessionDriverState.closing) {
      _state = FlarkV3SessionDriverState.faulted;
    }
  }

  void _pollInlineSidecarHostOnce() {
    final transport = _inlineSidecarTransport;
    if (transport == null) {
      _rejectUnexpectedInlineSidecarHostOutcome();
      return;
    }
    final result = _session.pollInlineSidecar(_hostPollGrant);
    switch (result) {
      case FlarkV3HostRejected<FlarkV3InlineSidecarHostPollOutcome>(
        :final rejection,
      ):
        _lastHostRejection = rejection;
        _inlineSidecarHostPollPending = false;
        final ticket = _activeInlineSidecarPollTicket;
        _activeInlineSidecarPollTicket = null;
        if (ticket != null) {
          transport.sendInlineSidecarHostPoll(
            FlarkV3ParserInlineSidecarHostPollRejected(
              ticket: ticket,
              reason: rejection.reason,
            ),
          );
        }
        _state = FlarkV3SessionDriverState.faulted;
      case FlarkV3HostAccepted<FlarkV3InlineSidecarHostPollOutcome>(
        :final value,
      ):
        _handleInlineSidecarHostPollOutcome(value);
    }
  }

  void _handleInlineSidecarHostPollOutcome(
    FlarkV3InlineSidecarHostPollOutcome outcome,
  ) {
    final transport = _inlineSidecarTransport;
    if (transport == null) {
      _rejectUnexpectedInlineSidecarHostOutcome();
      return;
    }
    switch (outcome) {
      case FlarkV3InlineSidecarHostPollPending():
        _inlineSidecarHostPollPending = true;
      case FlarkV3InlineSidecarHostPacketCredit(
        :final offerId,
        :final nextFrameOrdinal,
      ):
        final active = _activeInlineSidecarOffer;
        final ticket = _activeInlineSidecarPollTicket;
        if (_inlineSidecarPublicationState !=
                FlarkV3InlineSidecarPublicationDriverState
                    .awaitingPacketCredit ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase !=
                FlarkV3ParserInlineSidecarHostPollPhase.packetCredit ||
            offerId != active.offerId ||
            nextFrameOrdinal !=
                _expectedInlineSidecarPacketCreditFrameOrdinal) {
          _rejectUnexpectedInlineSidecarHostOutcome();
          return;
        }
        _expectedInlineSidecarPacketCreditFrameOrdinal = null;
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.acceptingPackets;
        _inlineSidecarHostPollPending = false;
        _activeInlineSidecarPollTicket = null;
        transport.sendInlineSidecarHostPoll(
          FlarkV3ParserInlineSidecarHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
      case FlarkV3InlineSidecarHostCommitted(:final ack):
        final active = _activeInlineSidecarOffer;
        final commit = _activeInlineSidecarCommit;
        final ticket = _activeInlineSidecarPollTicket;
        if (_inlineSidecarPublicationState !=
                FlarkV3InlineSidecarPublicationDriverState.awaitingCommit ||
            active == null ||
            commit == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase != FlarkV3ParserInlineSidecarHostPollPhase.commit ||
            !_inlineSidecarAckBindsOffer(ack, active, commit) ||
            _session.pendingInlineSidecarDeliveryAck != ack ||
            _session.installedInlineSidecarAck != ack) {
          _rejectUnexpectedInlineSidecarHostOutcome();
          return;
        }
        if (_inlinePresentationGeneration == flarkV3TransportV1Maximum) {
          _rejectUnexpectedInlineSidecarHostOutcome();
          return;
        }
        _committedInlineSidecarAckAwaitingDelivery = ack;
        _inlineSidecarPublicationState =
            FlarkV3InlineSidecarPublicationDriverState.awaitingDeliveryAck;
        _inlineSidecarHostPollPending = false;
        _activeInlineSidecarPollTicket = null;
        _inlinePresentationGeneration += 1;
        _inlineAttemptOutcomeGeneration += 1;
        transport.sendInlineSidecarHostPoll(
          FlarkV3ParserInlineSidecarHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
      case FlarkV3InlineSidecarHostAbortComplete(:final offerId):
        final active = _activeInlineSidecarOffer;
        final ticket = _activeInlineSidecarPollTicket;
        if (_inlineSidecarPublicationState !=
                FlarkV3InlineSidecarPublicationDriverState.aborting ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase != FlarkV3ParserInlineSidecarHostPollPhase.abort ||
            offerId != active.offerId) {
          _rejectUnexpectedInlineSidecarHostOutcome();
          return;
        }
        _inlineAttemptOutcomeGeneration += 1;
        _clearInlineSidecarDriverState();
        transport.sendInlineSidecarHostPoll(
          FlarkV3ParserInlineSidecarHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
      case FlarkV3InlineSidecarHostClosed():
        // Whole-host close is exclusively advanced by structural poll.
        _rejectUnexpectedInlineSidecarHostOutcome();
    }
  }

  void _rejectUnexpectedInlineSidecarHostOutcome() {
    const rejection = FlarkV3HostRejection(
      FlarkV3HostRejectReason.invalid,
      'Sidecar host poll outcome does not match driver publication state.',
    );
    _lastHostRejection = rejection;
    _inlineSidecarHostPollPending = false;
    final ticket = _activeInlineSidecarPollTicket;
    _activeInlineSidecarPollTicket = null;
    final transport = _inlineSidecarTransport;
    if (ticket != null && transport != null) {
      transport.sendInlineSidecarHostPoll(
        FlarkV3ParserInlineSidecarHostPollRejected(
          ticket: ticket,
          reason: rejection.reason,
        ),
      );
    }
    if (_state != FlarkV3SessionDriverState.closing) {
      _state = FlarkV3SessionDriverState.faulted;
    }
  }

  void _pollViewportPresentationHostOnce() {
    final transport = _viewportPresentationTransport;
    if (transport == null) {
      _rejectUnexpectedViewportPresentationHostOutcome();
      return;
    }
    final result = _session.pollViewportPresentation(_hostPollGrant);
    switch (result) {
      case FlarkV3HostRejected<FlarkV3ViewportPresentationHostPollOutcome>(
        :final rejection,
      ):
        _lastHostRejection = rejection;
        _viewportPresentationHostPollPending = false;
        final ticket = _activeViewportPresentationPollTicket;
        _activeViewportPresentationPollTicket = null;
        if (ticket != null) {
          transport.sendViewportPresentationHostPoll(
            FlarkV3ParserViewportPresentationHostPollRejected(
              ticket: ticket,
              reason: rejection.reason,
            ),
          );
        }
        _session.suppressViewportPresentationOffer();
        _clearViewportPresentationDriverState();
      case FlarkV3HostAccepted<FlarkV3ViewportPresentationHostPollOutcome>(
        :final value,
      ):
        _handleViewportPresentationHostPollOutcome(value);
    }
  }

  void _preemptViewportPresentationHostPollForInline() {
    final ticket = _activeViewportPresentationPollTicket;
    final transport = _viewportPresentationTransport;
    if (ticket == null || transport == null) {
      _rejectUnexpectedViewportPresentationHostOutcome();
      return;
    }
    _viewportPresentationHostPollPending = false;
    _activeViewportPresentationPollTicket = null;
    transport.sendViewportPresentationHostPoll(
      FlarkV3ParserViewportPresentationHostPollRejected(
        ticket: ticket,
        reason: FlarkV3HostRejectReason.superseded,
      ),
    );
    _session.suppressViewportPresentationOffer();
    _clearViewportPresentationDriverState();
  }

  void _handleViewportPresentationHostPollOutcome(
    FlarkV3ViewportPresentationHostPollOutcome outcome,
  ) {
    final transport = _viewportPresentationTransport;
    if (transport == null) {
      _rejectUnexpectedViewportPresentationHostOutcome();
      return;
    }
    switch (outcome) {
      case FlarkV3ViewportPresentationHostPollPending():
        _viewportPresentationHostPollPending = true;
      case FlarkV3ViewportPresentationHostPacketCredit(
        :final offerId,
        :final nextFrameOrdinal,
      ):
        final active = _activeViewportPresentationOffer;
        final ticket = _activeViewportPresentationPollTicket;
        if (_viewportPresentationPublicationState !=
                FlarkV3ViewportPresentationPublicationDriverState
                    .awaitingPacketCredit ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase !=
                FlarkV3ParserViewportPresentationHostPollPhase.packetCredit ||
            offerId != active.offerId ||
            nextFrameOrdinal !=
                _expectedViewportPresentationPacketCreditFrameOrdinal) {
          _rejectUnexpectedViewportPresentationHostOutcome();
          return;
        }
        _expectedViewportPresentationPacketCreditFrameOrdinal = null;
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState.acceptingPackets;
        _viewportPresentationHostPollPending = false;
        _activeViewportPresentationPollTicket = null;
        transport.sendViewportPresentationHostPoll(
          FlarkV3ParserViewportPresentationHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
      case FlarkV3ViewportPresentationHostCommitted(:final ack):
        final active = _activeViewportPresentationOffer;
        final commit = _activeViewportPresentationCommit;
        final ticket = _activeViewportPresentationPollTicket;
        if (_viewportPresentationPublicationState !=
                FlarkV3ViewportPresentationPublicationDriverState
                    .awaitingCommit ||
            active == null ||
            commit == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase !=
                FlarkV3ParserViewportPresentationHostPollPhase.commit ||
            !_viewportPresentationAckBindsOffer(ack, active, commit) ||
            _session.pendingViewportPresentationDeliveryAck != ack ||
            _session.installedViewportPresentationAck != ack) {
          _rejectUnexpectedViewportPresentationHostOutcome();
          return;
        }
        _committedViewportPresentationAckAwaitingDelivery = ack;
        _viewportPresentationPublicationState =
            FlarkV3ViewportPresentationPublicationDriverState
                .awaitingDeliveryAck;
        _viewportPresentationHostPollPending = false;
        _activeViewportPresentationPollTicket = null;
        _viewportPresentationAttemptOutcomeGeneration += 1;
        transport.sendViewportPresentationHostPoll(
          FlarkV3ParserViewportPresentationHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
      case FlarkV3ViewportPresentationHostAbortComplete(:final offerId):
        final active = _activeViewportPresentationOffer;
        final ticket = _activeViewportPresentationPollTicket;
        if (_viewportPresentationPublicationState !=
                FlarkV3ViewportPresentationPublicationDriverState.aborting ||
            active == null ||
            ticket == null ||
            ticket.binding != _binding ||
            ticket.offerId != active.offerId ||
            ticket.phase !=
                FlarkV3ParserViewportPresentationHostPollPhase.abort ||
            offerId != active.offerId) {
          _rejectUnexpectedViewportPresentationHostOutcome();
          return;
        }
        _viewportPresentationAttemptOutcomeGeneration += 1;
        _clearViewportPresentationDriverState();
        _issuedViewportPresentationRequest = null;
        _lastIssuedViewportPresentationGeneration = 0;
        transport.sendViewportPresentationHostPoll(
          FlarkV3ParserViewportPresentationHostPollCompleted(
            ticket: ticket,
            outcome: outcome,
          ),
        );
      case FlarkV3ViewportPresentationHostClosed():
        _rejectUnexpectedViewportPresentationHostOutcome();
    }
  }

  void _rejectUnexpectedViewportPresentationHostOutcome() {
    const rejection = FlarkV3HostRejection(
      FlarkV3HostRejectReason.invalid,
      'Viewport host poll outcome does not match driver publication state.',
    );
    _lastHostRejection = rejection;
    _viewportPresentationHostPollPending = false;
    final ticket = _activeViewportPresentationPollTicket;
    _activeViewportPresentationPollTicket = null;
    final transport = _viewportPresentationTransport;
    if (ticket != null && transport != null) {
      transport.sendViewportPresentationHostPoll(
        FlarkV3ParserViewportPresentationHostPollRejected(
          ticket: ticket,
          reason: rejection.reason,
        ),
      );
    }
    if (_state != FlarkV3SessionDriverState.closing) {
      _state = FlarkV3SessionDriverState.faulted;
    }
  }

  void _clearInlineSidecarDriverState() {
    _activeInlineSidecarOffer = null;
    _activeInlineSidecarCommit = null;
    _activeInlineSidecarPollTicket = null;
    _expectedInlineSidecarPacketCreditFrameOrdinal = null;
    _committedInlineSidecarAckAwaitingDelivery = null;
    _inlineSidecarHostPollPending = false;
    _inlineSidecarPublicationState =
        FlarkV3InlineSidecarPublicationDriverState.idle;
  }

  void _clearViewportPresentationDriverState() {
    _activeViewportPresentationOffer = null;
    _activeViewportPresentationCommit = null;
    _activeViewportPresentationPollTicket = null;
    _expectedViewportPresentationPacketCreditFrameOrdinal = null;
    _committedViewportPresentationAckAwaitingDelivery = null;
    _viewportPresentationHostPollPending = false;
    _viewportPresentationPublicationState =
        FlarkV3ViewportPresentationPublicationDriverState.idle;
  }

  void _finishCloseIfDrained() {
    if (_state == FlarkV3SessionDriverState.closing &&
        _parserClosed &&
        _hostDrained) {
      _transport.close();
      _state = FlarkV3SessionDriverState.closed;
    }
  }

  bool _isFreshCurrentEvent(FlarkV3ParserEvent event) {
    final generation = _workerGeneration;
    if (generation == null) {
      return event is FlarkV3ParserClosed &&
          _state == FlarkV3SessionDriverState.closing;
    }
    final exactBinding = switch (event) {
      FlarkV3ParserPublicationEvent(:final binding) => binding,
      FlarkV3ParserSourceFactsPage(:final binding) => binding,
      FlarkV3ParserSourceFactsCompleted(:final binding) => binding,
      FlarkV3ParserSourceFactsDeltaBegin(:final binding) => binding,
      FlarkV3ParserSourceFactsDeltaPage(:final binding) => binding,
      FlarkV3ParserSourceFactsDeltaCompleted(:final binding) => binding,
      FlarkV3ParserInlineRefinementUnavailable(:final binding) => binding,
      FlarkV3ParserViewportPresentationUnavailable(:final binding) => binding,
      _ => null,
    };
    return event.workerGeneration == generation &&
        (exactBinding == null || exactBinding == _binding) &&
        event.eventId > _lastHandledEventId;
  }

  bool _isFreshCurrentInlineSidecarEvent(
    FlarkV3ParserInlineSidecarEvent event,
  ) => event.binding == _binding && event.eventId > _lastHandledEventId;

  bool _isFreshCurrentViewportPresentationEvent(
    FlarkV3ParserViewportPresentationEvent event,
  ) => event.binding == _binding && event.eventId > _lastHandledEventId;

  void _returnEventCredit(
    FlarkV3ParserEvent event,
    FlarkV3ParserEventDisposition disposition, {
    FlarkV3SourceWorkerSyncAckReceipt? sourceSync,
    FlarkV3CanonicalSourcePromotionProof? sourceCertification,
  }) {
    final receiptBinding = switch (event) {
      FlarkV3ParserPublicationEvent(:final binding) => binding,
      FlarkV3ParserSourceFactsPage(:final binding) => binding,
      FlarkV3ParserSourceFactsCompleted(:final binding) => binding,
      FlarkV3ParserSourceFactsDeltaBegin(:final binding) => binding,
      FlarkV3ParserSourceFactsDeltaPage(:final binding) => binding,
      FlarkV3ParserSourceFactsDeltaCompleted(:final binding) => binding,
      FlarkV3ParserInlineRefinementUnavailable(:final binding) => binding,
      FlarkV3ParserViewportPresentationUnavailable(:final binding) => binding,
      FlarkV3ParserOpened(:final binding) => binding,
      FlarkV3ParserDrainProgress(:final binding) => binding,
      _ => FlarkV3ParserSessionBinding(
        documentSession: _binding.documentSession,
        sourceSessionIdentity: _binding.sourceSessionIdentity,
        workerGeneration: event.workerGeneration,
      ),
    };
    _transport.send(
      FlarkV3ParserEventReceipt(
        eventId: event.eventId,
        binding: receiptBinding,
        disposition: disposition,
        sourceSync: sourceSync,
        sourceCertification: sourceCertification,
      ),
    );
  }

  void _returnInlineSidecarEventCredit(
    FlarkV3ParserInlineSidecarEvent event,
    FlarkV3ParserEventDisposition disposition,
  ) {
    _transport.send(
      FlarkV3ParserEventReceipt(
        eventId: event.eventId,
        binding: event.binding,
        disposition: disposition,
      ),
    );
  }

  void _returnViewportPresentationEventCredit(
    FlarkV3ParserViewportPresentationEvent event,
    FlarkV3ParserEventDisposition disposition,
  ) {
    _transport.send(
      FlarkV3ParserEventReceipt(
        eventId: event.eventId,
        binding: event.binding,
        disposition: disposition,
      ),
    );
  }

  void _requireWritable() {
    if (_closeRequested ||
        (_state != FlarkV3SessionDriverState.opening &&
            _state != FlarkV3SessionDriverState.open)) {
      throw StateError('Parser driver is not writable.');
    }
  }
}

int _boundedDefault(int admittedMaximum, int preferred) =>
    admittedMaximum < preferred ? admittedMaximum : preferred;

bool _ackBindsOffer(FlarkV3StructuralAck ack, FlarkV3HostOfferBegin offer) =>
    ack.publicationSession == offer.publicationSession &&
    ack.hostRevision == offer.targetHostRevision &&
    ack.sourceVersion == offer.sourceVersion &&
    ack.sourceRoot == offer.sourceRoot &&
    ack.parseGeneration == offer.parseGeneration &&
    ack.grammarRevision == offer.grammarRevision &&
    ack.syntaxProfile == offer.syntaxProfile &&
    ack.authorityMask == offer.authorityMask &&
    ack.recordCount == offer.targetRecordCount;

bool _inlineSidecarAckBindsOffer(
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

bool _viewportBeginMatchesRequest(
  FlarkV3ViewportPresentationOfferBegin begin,
  FlarkV3ParserPresentViewport request,
) {
  final binding = begin.binding;
  final requested = binding.requestedRange;
  final limits = begin.queryLimits;
  final requestedLimits = request.limits;
  return binding.viewportGeneration == request.viewportGeneration &&
      requested.startUtf8 == request.requestedStartUtf8 &&
      requested.startUtf16 == request.requestedStartUtf16 &&
      requested.endUtf8 == request.requestedEndUtf8 &&
      requested.endUtf16 == request.requestedEndUtf16 &&
      binding.start.blockOrdinal == request.startBlockOrdinal &&
      binding.start.utf8Offset == request.startUtf8 &&
      binding.start.utf16Offset == request.startUtf16 &&
      limits.maximumStructuralEntries ==
          requestedLimits.maximumStructuralEntries &&
      limits.maximumStoragePages == requestedLimits.maximumStoragePages &&
      limits.maximumInlineLeaves == requestedLimits.maximumInlineLeaves &&
      limits.maximumInlineLeafSourceBytes ==
          requestedLimits.maximumInlineLeafSourceBytes &&
      limits.maximumInlineSourceBytes ==
          requestedLimits.maximumInlineSourceBytes &&
      limits.maximumFactRecords == requestedLimits.maximumFactRecords &&
      limits.maximumEncodedFrameBytes ==
          requestedLimits.maximumEncodedFrameBytes &&
      limits.maximumParserTransitions ==
          requestedLimits.maximumParserTransitions;
}

bool _viewportPresentationAckBindsOffer(
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

bool _acknowledgementBindsLease(
  FlarkV3SourceWorkerSyncAcknowledgement acknowledgement,
  FlarkV3SourceWorkerSyncLease lease,
) {
  if (acknowledgement.sourceSessionIdentity != lease.sourceSessionIdentity ||
      acknowledgement.leaseId != lease.leaseId ||
      acknowledgement.workerGeneration != lease.workerGeneration ||
      acknowledgement.kind != lease.kind) {
    return false;
  }
  return switch ((lease, acknowledgement)) {
    (
      FlarkV3SourceSnapshotSyncLease lease,
      FlarkV3SourceSnapshotSyncAcknowledgement acknowledgement,
    ) =>
      acknowledgement.baseUiRevision == lease.baseUiRevision &&
          acknowledgement.startUtf16 == lease.startUtf16 &&
          acknowledgement.endUtf16 == lease.endUtf16 &&
          acknowledgement.throughIntentSequence == lease.throughIntentSequence,
    (
      FlarkV3SourceIntentSyncLease lease,
      FlarkV3SourceIntentSyncAcknowledgement acknowledgement,
    ) =>
      acknowledgement.firstSequence == lease.firstSequence &&
          acknowledgement.lastSequence == lease.lastSequence &&
          acknowledgement.entryCount == lease.intents.length &&
          acknowledgement.payloadUtf16 == lease.payloadUtf16,
    _ => false,
  };
}

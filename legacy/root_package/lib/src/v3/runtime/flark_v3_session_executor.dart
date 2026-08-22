import 'dart:async';

import '../host/host.dart';
import '../session/session.dart';
import 'flark_v3_event_task_scheduler_stub.dart'
    if (dart.library.js_interop) 'web/flark_v3_web_event_task_scheduler.dart'
    as event_task;
import 'flark_v3_hot_inline_sidecar_transport.dart';
import 'flark_v3_parser_transport.dart';
import 'flark_v3_session_driver.dart';
import 'flark_v3_viewport_presentation_transport.dart';

typedef FlarkV3SessionExecutorCallback = void Function();
typedef FlarkV3SessionExecutorFailureCallback =
    void Function(Object error, StackTrace stackTrace);

/// Schedules one bounded Dart-side engine turn.
///
/// The default implementation yields through Dart's event queue. Tests and
/// embedding runtimes may inject a deterministic scheduler without importing
/// Flutter or exposing the driver's manual pump surface.
abstract interface class FlarkV3SessionTaskScheduler {
  void schedule(FlarkV3SessionExecutorCallback callback);
}

final class FlarkV3DartEventTaskScheduler
    implements FlarkV3SessionTaskScheduler {
  const FlarkV3DartEventTaskScheduler();

  @override
  void schedule(FlarkV3SessionExecutorCallback callback) {
    event_task.scheduleEventTask(callback);
  }
}

/// Event-queue owner for one bounded v3 session driver.
///
/// Parser callbacks only enqueue one credited event. This executor coalesces
/// wakeups, performs a small count-and-time-bounded turn, and yields before
/// continuing. Consequently neither a large document nor a fast background
/// worker can turn transport progress into an unbounded Dart/UI-isolate task.
/// The eventual public document facade owns this object; applications and
/// Flutter adapters do not call `pump`.
final class FlarkV3SessionExecutor {
  FlarkV3SessionExecutor._({
    required FlarkV3SessionDriver driver,
    required _WakeableParserTransport transport,
    required FlarkV3SessionTaskScheduler scheduler,
    required Zone schedulingZone,
    required this.maximumActionsPerTurn,
    required this.maximumTurnDuration,
    required FlarkV3SessionExecutorCallback? onProgress,
    required FlarkV3SessionExecutorFailureCallback? onFailure,
  }) : _driver = driver,
       _transport = transport,
       _scheduler = scheduler,
       _schedulingZone = schedulingZone,
       _onProgress = onProgress,
       _onFailure = onFailure;

  factory FlarkV3SessionExecutor.attach({
    required FlarkDocumentSession session,
    required FlarkV3ParserTransport transport,
    required FlarkV3ParserSessionBinding parserBinding,
    FlarkV3ParserPublicationAuthority? publicationAuthority,
    FlarkV3HostWorkGrant? hostPollGrant,
    int parserDrainTransitions = flarkV3ParserMaximumDrainTransitions,
    FlarkV3SessionTaskScheduler scheduler =
        const FlarkV3DartEventTaskScheduler(),
    int maximumActionsPerTurn = 8,
    Duration maximumTurnDuration = const Duration(milliseconds: 1),
    FlarkV3SessionExecutorCallback? onProgress,
    FlarkV3SessionExecutorFailureCallback? onFailure,
  }) {
    if (maximumActionsPerTurn <= 0) {
      throw RangeError.value(
        maximumActionsPerTurn,
        'maximumActionsPerTurn',
        'must be positive',
      );
    }
    if (maximumTurnDuration <= Duration.zero) {
      throw ArgumentError.value(
        maximumTurnDuration,
        'maximumTurnDuration',
        'must be positive',
      );
    }

    final _WakeableParserTransport wakeable;
    if (transport is FlarkV3ParserInlineSidecarTransport &&
        transport is FlarkV3ParserViewportPresentationTransport) {
      wakeable = _WakeableInlineViewportParserTransport(
        transport,
        transport as FlarkV3ParserInlineSidecarTransport,
        transport as FlarkV3ParserViewportPresentationTransport,
      );
    } else if (transport is FlarkV3ParserInlineSidecarTransport) {
      wakeable = _WakeableInlineParserTransport(
        transport,
        transport as FlarkV3ParserInlineSidecarTransport,
      );
    } else if (transport is FlarkV3ParserViewportPresentationTransport) {
      wakeable = _WakeableViewportParserTransport(
        transport,
        transport as FlarkV3ParserViewportPresentationTransport,
      );
    } else {
      wakeable = _WakeableParserTransport(transport);
    }
    final driver = FlarkV3SessionDriver(
      session: session,
      transport: wakeable,
      parserBinding: parserBinding,
      publicationAuthority: publicationAuthority,
      hostPollGrant: hostPollGrant,
      parserDrainTransitions: parserDrainTransitions,
    );
    final executor = FlarkV3SessionExecutor._(
      driver: driver,
      transport: wakeable,
      scheduler: scheduler,
      schedulingZone: Zone.current,
      maximumActionsPerTurn: maximumActionsPerTurn,
      maximumTurnDuration: maximumTurnDuration,
      onProgress: onProgress,
      onFailure: onFailure,
    );
    wakeable.arm(executor._requestTurn);
    return executor;
  }

  final FlarkV3SessionDriver _driver;
  final _WakeableParserTransport _transport;
  final FlarkV3SessionTaskScheduler _scheduler;
  final Zone _schedulingZone;
  final FlarkV3SessionExecutorCallback? _onProgress;
  final FlarkV3SessionExecutorFailureCallback? _onFailure;

  final int maximumActionsPerTurn;
  final Duration maximumTurnDuration;

  Completer<void>? _closeCompleter;
  bool _turnScheduled = false;
  bool _runningTurn = false;
  bool _disposed = false;

  FlarkV3SessionDriverState get state => _driver.state;
  FlarkV3PublicationDriverState get publicationState =>
      _driver.publicationState;
  FlarkV3InlineSidecarPublicationDriverState
  get inlineSidecarPublicationState => _driver.inlineSidecarPublicationState;
  FlarkV3ViewportPresentationPublicationDriverState
  get viewportPresentationPublicationState =>
      _driver.viewportPresentationPublicationState;
  FlarkV3ParserSessionBinding get parserBinding => _driver.parserBinding;
  FlarkV3ParserFailed? get lastFailure => _driver.lastFailure;
  FlarkV3ParserPublicationFailed? get lastPublicationFailure =>
      _driver.lastPublicationFailure;
  FlarkV3ParserInlineSidecarFailed? get lastInlineSidecarFailure =>
      _driver.lastInlineSidecarFailure;
  FlarkV3ParserViewportPresentationFailed?
  get lastViewportPresentationFailure =>
      _driver.lastViewportPresentationFailure;
  FlarkV3HostRejection? get lastHostRejection => _driver.lastHostRejection;
  int get inlinePresentationGeneration => _driver.inlinePresentationGeneration;
  int get inlineAttemptOutcomeGeneration =>
      _driver.inlineAttemptOutcomeGeneration;
  int get viewportPresentationAttemptOutcomeGeneration =>
      _driver.viewportPresentationAttemptOutcomeGeneration;
  int? get lastViewportPresentationUnavailableGeneration =>
      _driver.lastViewportPresentationUnavailableGeneration;
  int? get lastViewportPresentationUnavailableReason =>
      _driver.lastViewportPresentationUnavailableReason;

  /// Requests parser-certified inline facts at one exact-current source point.
  ///
  /// Repeated requests before the next bounded turn coalesce to the newest
  /// generation without creating per-request Futures.
  int requestInlineRefinement({
    required int utf16Offset,
    FlarkV3InlinePointAffinity affinity = FlarkV3InlinePointAffinity.after,
    FlarkV3InlineRefinementTarget target =
        FlarkV3InlineRefinementTarget.automatic,
  }) {
    _requireLive();
    final generation = _driver.requestInlineRefinement(
      utf16Offset: utf16Offset,
      affinity: affinity,
      target: target,
    );
    _requestTurn();
    return generation;
  }

  /// Requests one bounded parser-certified passive viewport page.
  ///
  /// Repeated requests before the next bounded turn coalesce to the newest
  /// generation. Focused inline refinement remains the higher-priority lane.
  int requestViewportPresentation({
    required int requestedStartUtf8,
    required int requestedStartUtf16,
    required int requestedEndUtf8,
    required int requestedEndUtf16,
    required FlarkV3ProtocolU64 startBlockOrdinal,
    FlarkV3ParserViewportPresentationLimits? limits,
  }) {
    _requireLive();
    final generation = _driver.requestViewportPresentation(
      requestedStartUtf8: requestedStartUtf8,
      requestedStartUtf16: requestedStartUtf16,
      requestedEndUtf8: requestedEndUtf8,
      requestedEndUtf16: requestedEndUtf16,
      startBlockOrdinal: startBlockOrdinal,
      limits: limits ?? FlarkV3ParserViewportPresentationLimits(),
    );
    _requestTurn();
    return generation;
  }

  /// Notifies the internal driver after the owning document facade commits one
  /// or more exact source transactions. Repeated calls before the next turn
  /// coalesce into one wakeup and the session's bounded intent journal.
  void sourceChanged() {
    _requireLive();
    _driver.markDirty();
    _requestTurn();
  }

  /// Starts a clean worker generation while preserving the same Dart session.
  FlarkDocumentSourceWorkerRestartReceipt restart() {
    if (_disposed || _driver.state != FlarkV3SessionDriverState.faulted) {
      throw StateError(
        'Only an executor faulted by a terminal parser failure can restart.',
      );
    }
    final receipt = _driver.restart();
    _requestTurn();
    return receipt;
  }

  /// Begins graceful, bounded parser and host retirement.
  ///
  /// Close may span many event-loop turns, but each turn retains the same
  /// foreground bound. Repeated calls share one completion.
  Future<void> close() {
    final existing = _closeCompleter;
    if (existing != null) return existing.future;
    final completer = Completer<void>();
    _closeCompleter = completer;
    if (_driver.state == FlarkV3SessionDriverState.closed) {
      completer.complete();
      return completer.future;
    }
    _driver.beginClose();
    _requestTurn();
    return completer.future;
  }

  /// Emergency local teardown for an unavailable platform endpoint.
  ///
  /// This is deliberately distinct from graceful close and proves no worker
  /// reclamation receipt. The public facade uses it only on terminal platform
  /// failure or caller-selected timeout.
  void emergencyDispose() {
    if (_disposed) return;
    _fail(
      StateError('Flark parser session required emergency disposal.'),
      StackTrace.current,
      notify: false,
    );
  }

  void _requestTurn() {
    if (_disposed || _turnScheduled) return;
    _turnScheduled = true;
    _schedulingZone.run(() => _scheduler.schedule(_runTurn));
  }

  void _runTurn() {
    if (_disposed) return;
    if (_runningTurn) {
      _requestTurn();
      return;
    }
    _turnScheduled = false;
    _runningTurn = true;
    final stopwatch = Stopwatch()..start();
    var actions = 0;
    var needsMoreWork = false;
    Object? failure;
    StackTrace? failureStack;
    try {
      while (actions < maximumActionsPerTurn &&
          (actions == 0 || stopwatch.elapsed < maximumTurnDuration)) {
        final receipt = _driver.pump();
        needsMoreWork = receipt.needsMoreWork;
        if (receipt.action == FlarkV3SessionPumpAction.idle) break;
        actions += 1;
      }
    } catch (error, stackTrace) {
      failure = error;
      failureStack = stackTrace;
    } finally {
      stopwatch.stop();
      _runningTurn = false;
    }

    if (failure != null) {
      _fail(failure, failureStack!, notify: true);
      return;
    }
    if (actions != 0) _onProgress?.call();
    if (_driver.state == FlarkV3SessionDriverState.closed) {
      _disposed = true;
      _transport.disarm();
      final completer = _closeCompleter;
      if (completer != null && !completer.isCompleted) completer.complete();
      return;
    }
    if (needsMoreWork) _requestTurn();
  }

  void _requireLive() {
    if (_disposed ||
        _driver.state == FlarkV3SessionDriverState.faulted ||
        _driver.state == FlarkV3SessionDriverState.closing ||
        _driver.state == FlarkV3SessionDriverState.closed) {
      throw StateError('The Flark session executor is not writable.');
    }
  }

  void _fail(Object error, StackTrace stackTrace, {required bool notify}) {
    if (_disposed) return;
    _disposed = true;
    _turnScheduled = false;
    try {
      _driver.forceClose();
    } on Object {
      // Preserve the causal failure. Emergency teardown is best effort and
      // does not manufacture a successful close receipt.
    } finally {
      _transport.disarm();
    }
    final completer = _closeCompleter;
    if (completer != null && !completer.isCompleted) {
      completer.completeError(error, stackTrace);
    }
    if (notify) _onFailure?.call(error, stackTrace);
  }
}

/// Decorates the typed transport with a wakeup edge but no extra queue.
base class _WakeableParserTransport implements FlarkV3ParserTransport {
  _WakeableParserTransport(this._delegate);

  final FlarkV3ParserTransport _delegate;
  FlarkV3SessionExecutorCallback? _wake;
  bool _wakePending = false;

  void arm(FlarkV3SessionExecutorCallback wake) {
    if (_wake != null) throw StateError('Executor wakeup is already armed.');
    _wake = wake;
    if (_wakePending) {
      _wakePending = false;
      wake();
    }
  }

  void disarm() {
    _wake = null;
    _wakePending = false;
  }

  @override
  void bind(FlarkV3ParserEventCallback onEvent) {
    _delegate.bind((event) {
      onEvent(event);
      _wakeAfterEvent();
    });
  }

  void _wakeAfterEvent() {
    final wake = _wake;
    if (wake == null) {
      _wakePending = true;
    } else {
      wake();
    }
  }

  @override
  void send(FlarkV3ParserCommand command) => _delegate.send(command);

  @override
  void close() => _delegate.close();
}

final class _WakeableInlineParserTransport extends _WakeableParserTransport
    implements FlarkV3ParserInlineSidecarTransport {
  _WakeableInlineParserTransport(super.delegate, this._inlineDelegate);

  final FlarkV3ParserInlineSidecarTransport _inlineDelegate;

  @override
  void bindInlineSidecar(FlarkV3ParserInlineSidecarEventCallback onEvent) {
    _inlineDelegate.bindInlineSidecar((event) {
      onEvent(event);
      _wakeAfterEvent();
    });
  }

  @override
  void sendInlineSidecarHostPoll(
    FlarkV3ParserInlineSidecarHostPollCommand command,
  ) => _inlineDelegate.sendInlineSidecarHostPoll(command);
}

base class _WakeableViewportParserTransport extends _WakeableParserTransport
    implements FlarkV3ParserViewportPresentationTransport {
  _WakeableViewportParserTransport(super.delegate, this._viewportDelegate);

  final FlarkV3ParserViewportPresentationTransport _viewportDelegate;

  @override
  void bindViewportPresentation(
    FlarkV3ParserViewportPresentationEventCallback onEvent,
  ) {
    _viewportDelegate.bindViewportPresentation((event) {
      onEvent(event);
      _wakeAfterEvent();
    });
  }

  @override
  void sendViewportPresentationHostPoll(
    FlarkV3ParserViewportPresentationHostPollCommand command,
  ) => _viewportDelegate.sendViewportPresentationHostPoll(command);
}

final class _WakeableInlineViewportParserTransport
    extends _WakeableViewportParserTransport
    implements FlarkV3ParserInlineSidecarTransport {
  _WakeableInlineViewportParserTransport(
    super.delegate,
    this._inlineDelegate,
    super.viewportDelegate,
  );

  final FlarkV3ParserInlineSidecarTransport _inlineDelegate;

  @override
  void bindInlineSidecar(FlarkV3ParserInlineSidecarEventCallback onEvent) {
    _inlineDelegate.bindInlineSidecar((event) {
      onEvent(event);
      _wakeAfterEvent();
    });
  }

  @override
  void sendInlineSidecarHostPoll(
    FlarkV3ParserInlineSidecarHostPollCommand command,
  ) => _inlineDelegate.sendInlineSidecarHostPoll(command);
}

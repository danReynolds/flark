import 'dart:async';

import '../markdown/markdown.dart';
import 'flark_flutter_controller.dart';

final class FlarkParseScheduler {
  FlarkParseScheduler({
    required FlarkFlutterController controller,
    required FlarkMarkdownParseBackend backend,
    FlarkMarkdownProfile profile = FlarkMarkdownProfile.commonMarkGfm,
    Duration debounce = const Duration(milliseconds: 80),
    void Function(Object error, StackTrace stackTrace)? onError,
  }) : _controller = controller,
       _backend = backend,
       _profile = profile,
       _debounce = debounce,
       _onError = onError;

  final FlarkFlutterController _controller;
  final FlarkMarkdownParseBackend _backend;
  final FlarkMarkdownProfile _profile;
  final Duration _debounce;
  final void Function(Object error, StackTrace stackTrace)? _onError;

  Timer? _timer;
  bool _started = false;
  bool _disposed = false;
  bool _inFlight = false;
  int? _scheduledRevision;
  int? _inFlightRevision;
  Future<void>? _activeParse;

  void start({bool immediate = true}) {
    if (_started) return;
    _started = true;
    _controller.addListener(_handleControllerChanged);
    _schedule(immediate: immediate);
  }

  /// Attempts a synchronous parse of the controller's current revision.
  ///
  /// Returns whether the render plan is authoritative afterwards. False when
  /// the backend cannot parse synchronously (async-only, or the document is
  /// large enough to belong on the worker isolate), when a parse is already
  /// in flight, or when the sync parse's result was rejected. Adopting the
  /// plan notifies the controller's listeners synchronously, so this is for
  /// callers that run before listeners attach (see
  /// `FlarkFlutterController.tryParseSync`) — deliberately NOT part of
  /// [start], where a shared controller may already have built widgets
  /// listening and a synchronous notify would mark them dirty mid-build.
  bool tryParseSync() {
    if (_disposed) return false;
    if (_controller.hasAuthoritativeRenderPlan) return true;
    if (_inFlight) return false;
    final backend = _backend;
    if (backend is! FlarkSyncCapableParseBackend) return false;
    try {
      final result = backend.parseSync(_requestForCurrentState());
      if (result == null) return false;
      // Inside the try: reconciliation errors route to onError like every
      // async parse's do, instead of throwing out of a caller's initState.
      if (!_controller.applyParseResult(result)) return false;
    } catch (error, stackTrace) {
      _onError?.call(error, stackTrace);
      return false;
    }
    // The plan is current — a parse already queued for this revision (a
    // pending debounce timer or scheduled microtask) would only repeat the
    // same work and re-notify every listener, so drop it. parseNow does the
    // same at entry.
    _timer?.cancel();
    _timer = null;
    _scheduledRevision = null;
    return _controller.hasAuthoritativeRenderPlan;
  }

  FlarkMarkdownParseRequest _requestForCurrentState() {
    final state = _controller.state;
    return FlarkMarkdownParseRequest(
      revision: state.revision,
      markdown: state.markdown,
      profile: _profile,
    );
  }

  /// Parses until the controller's current revision has an authoritative
  /// render plan, bypassing the debounce window.
  ///
  /// Resolves immediately when the plan is already authoritative. When a
  /// parse is in flight (now potentially milliseconds long on a worker
  /// isolate), this chains onto it instead of silently returning — callers
  /// like the live editor's structural-edit path rely on the returned future
  /// meaning "the plan is current", not "a parse may happen eventually".
  Future<void> parseNow() async {
    if (_disposed) return;
    _timer?.cancel();
    _timer = null;
    while (!_disposed && !_controller.hasAuthoritativeRenderPlan) {
      final active = _activeParse;
      if (active != null) {
        await active;
        continue;
      }
      final revisionBefore = _controller.state.revision;
      await _parseCurrentRevision();
      if (!_disposed &&
          !_controller.hasAuthoritativeRenderPlan &&
          _controller.state.revision == revisionBefore) {
        // The parse for this revision completed without producing an
        // authoritative plan (result rejected); bail rather than spin.
        break;
      }
    }
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _timer?.cancel();
    _controller.removeListener(_handleControllerChanged);
  }

  void _handleControllerChanged() {
    if (!_started || _disposed) return;
    if (_controller.hasAuthoritativeRenderPlan) return;
    _schedule(immediate: false);
  }

  void _schedule({required bool immediate}) {
    if (_disposed) return;
    final revision = _controller.state.revision;
    if (_inFlight && _inFlightRevision == revision) return;
    if (_scheduledRevision == revision) return;

    _timer?.cancel();
    _scheduledRevision = revision;
    // Both callbacks re-check _scheduledRevision: a queued parse may be
    // superseded before it runs — by tryParseSync adopting the revision
    // synchronously (which clears it), or by a newer revision re-scheduling —
    // and a microtask, unlike the timer, cannot be cancelled.
    if (immediate || _debounce == Duration.zero) {
      scheduleMicrotask(() {
        if (_disposed || _scheduledRevision != revision) return;
        _ignore(_parseCurrentRevision(), _onError);
      });
      return;
    }

    _timer = Timer(_debounce, () {
      _timer = null;
      if (_disposed || _scheduledRevision != revision) return;
      _ignore(_parseCurrentRevision(), _onError);
    });
  }

  Future<void> _parseCurrentRevision() {
    if (_inFlight || _disposed) return Future<void>.value();
    final future = _runParse();
    _activeParse = future;
    unawaited(
      future
          .whenComplete(() {
            if (identical(_activeParse, future)) _activeParse = null;
          })
          .catchError((Object _) {}),
    );
    return future;
  }

  Future<void> _runParse() async {
    if (_inFlight || _disposed) return;

    final state = _controller.state;
    _scheduledRevision = null;
    _inFlight = true;
    _inFlightRevision = state.revision;
    try {
      final result = await _backend.parse(_requestForCurrentState());
      if (_disposed) return;
      _controller.applyParseResult(result);
    } finally {
      _inFlight = false;
      final parsedRevision = _inFlightRevision;
      _inFlightRevision = null;
      if (!_disposed && _controller.state.revision != parsedRevision) {
        _scheduledRevision = null;
        _schedule(immediate: true);
      }
    }
  }
}

void _ignore(
  Future<void> future,
  void Function(Object error, StackTrace stackTrace)? onError,
) {
  future.catchError((Object error, StackTrace stackTrace) {
    onError?.call(error, stackTrace);
  });
}

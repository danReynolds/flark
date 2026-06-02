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

  void start({bool immediate = true}) {
    if (_started) return;
    _started = true;
    _controller.addListener(_handleControllerChanged);
    _schedule(immediate: immediate);
  }

  Future<void> parseNow() async {
    if (_disposed) return;
    _timer?.cancel();
    _timer = null;
    await _parseCurrentRevision();
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
    if (immediate || _debounce == Duration.zero) {
      scheduleMicrotask(() {
        if (!_disposed) _ignore(_parseCurrentRevision(), _onError);
      });
      return;
    }

    _timer = Timer(_debounce, () {
      _timer = null;
      if (!_disposed) _ignore(_parseCurrentRevision(), _onError);
    });
  }

  Future<void> _parseCurrentRevision() async {
    if (_inFlight || _disposed) return;

    final state = _controller.state;
    _scheduledRevision = null;
    _inFlight = true;
    _inFlightRevision = state.revision;
    try {
      final request = FlarkMarkdownParseRequest(
        revision: state.revision,
        markdown: state.markdown,
        profile: _profile,
      );
      final result = await _backend.parse(request);
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

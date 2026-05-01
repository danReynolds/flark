import 'dart:async';

import 'syntax_engine.dart';
import 'syntax_snapshot.dart';

typedef SyntaxParseRunner = Future<SyntaxSnapshot> Function(
    SyntaxParseRequest request);

typedef SyntaxParseErrorHandler = void Function(
  Object error,
  StackTrace stackTrace,
  SyntaxParseRequest request,
);

/// Single-flight scheduler for syntax parsing.
///
/// Invariant:
/// - Maximum one in-flight parse request.
/// - Maximum one pending request, always the latest request.
class SyntaxParseScheduler {
  final SyntaxParseRunner _runParse;
  final void Function(SyntaxSnapshot snapshot) _onSnapshot;
  final SyntaxParseErrorHandler? _onError;

  bool _disposed = false;
  bool _inFlight = false;
  SyntaxParseRequest? _inFlightRequest;
  SyntaxParseRequest? _pendingRequest;
  int _latestScheduledRevision = -1;

  int _pendingReplaceCount = 0;
  int _staleDropCount = 0;

  SyntaxParseScheduler({
    required SyntaxParseRunner runParse,
    required void Function(SyntaxSnapshot snapshot) onSnapshot,
    SyntaxParseErrorHandler? onError,
  })  : _runParse = runParse,
        _onSnapshot = onSnapshot,
        _onError = onError;

  bool get isDisposed => _disposed;
  bool get hasInFlight => _inFlight;
  int get inFlightCount => _inFlight ? 1 : 0;
  int get pendingCount => _pendingRequest == null ? 0 : 1;
  SyntaxParseRequest? get inFlightRequest => _inFlightRequest;
  SyntaxParseRequest? get pendingRequest => _pendingRequest;
  int get pendingReplaceCount => _pendingReplaceCount;
  int get staleDropCount => _staleDropCount;

  void resetCounters() {
    _pendingReplaceCount = 0;
    _staleDropCount = 0;
  }

  void schedule(SyntaxParseRequest request) {
    if (_disposed) return;

    if (request.revision > _latestScheduledRevision) {
      _latestScheduledRevision = request.revision;
    }

    if (!_inFlight) {
      unawaited(_dispatch(request));
      return;
    }

    if (_pendingRequest != null) {
      _pendingReplaceCount++;
    }
    _pendingRequest = request;
  }

  void dispose() {
    _disposed = true;
    _pendingRequest = null;
  }

  Future<void> _dispatch(SyntaxParseRequest request) async {
    if (_disposed) return;

    _inFlight = true;
    _inFlightRequest = request;

    try {
      final snapshot = await _runParse(request);
      if (_disposed) return;

      final isStale = snapshot.revision != request.revision ||
          snapshot.revision < _latestScheduledRevision;
      if (isStale) {
        _staleDropCount++;
      } else {
        _onSnapshot(snapshot);
      }
    } catch (error, stackTrace) {
      if (!_disposed) {
        _onError?.call(error, stackTrace, request);
      }
    } finally {
      _inFlight = false;
      _inFlightRequest = null;

      if (_disposed) {
        _pendingRequest = null;
      } else {
        final next = _pendingRequest;
        _pendingRequest = null;
        if (next != null && !_isEquivalentRequest(next, request)) {
          unawaited(_dispatch(next));
        }
      }
    }
  }

  static bool _isEquivalentRequest(SyntaxParseRequest a, SyntaxParseRequest b) {
    if (a.revision != b.revision) return false;
    if (a.text != b.text) return false;
    if (a.profile != b.profile) return false;
    if (a.priorityRanges.length != b.priorityRanges.length) return false;
    for (var i = 0; i < a.priorityRanges.length; i++) {
      final ar = a.priorityRanges[i];
      final br = b.priorityRanges[i];
      if (ar.start != br.start || ar.end != br.end) {
        return false;
      }
    }
    return true;
  }
}

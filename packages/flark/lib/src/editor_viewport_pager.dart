import 'dart:async';
import 'dart:math' as math;

import 'editor_coordinator.dart';
import 'models.dart';
import 'viewport_installation.dart';
import 'viewport_navigation.dart';
import 'viewport_source.dart';

/// One fully read bounded viewport, ready for synchronous host adoption.
final class FlarkViewportPageResult {
  FlarkViewportPageResult._({
    required FlarkEditorViewportPager owner,
    required this.viewport,
    required this.source,
    required int? requiredEditGeneration,
    required _FlarkViewportNavigationAdoption navigationAdoption,
  }) : _owner = owner,
       _requiredEditGeneration = requiredEditGeneration,
       _navigationAdoption = navigationAdoption;

  final FlarkViewport viewport;
  final String source;
  final FlarkEditorViewportPager _owner;
  final int? _requiredEditGeneration;
  final _FlarkViewportNavigationAdoption _navigationAdoption;
  bool _settled = false;
}

sealed class _FlarkViewportNavigationAdoption {
  const _FlarkViewportNavigationAdoption();
}

final class _FlarkViewportAdvance extends _FlarkViewportNavigationAdoption {
  const _FlarkViewportAdvance(this.anchor);

  final FlarkViewportPageAnchor anchor;
}

final class _FlarkViewportMoveBackward
    extends _FlarkViewportNavigationAdoption {
  const _FlarkViewportMoveBackward(this.anchor);

  final FlarkViewportPageAnchor anchor;
}

final class _FlarkViewportRefreshPath extends _FlarkViewportNavigationAdoption {
  _FlarkViewportRefreshPath(Iterable<FlarkViewportPageAnchor> anchors)
    : anchors = List.unmodifiable(anchors);

  final List<FlarkViewportPageAnchor> anchors;
}

/// Immutable host facts needed to choose a refresh origin.
final class FlarkViewportRefreshRequest {
  const FlarkViewportRefreshRequest({
    required this.previousViewport,
    required this.visibleUtf16Start,
    required this.visibleSource,
    required this.optimisticEditsStartAtOrAfterPreviousStart,
    required this.caretUtf16,
    required this.ensureCaretVisible,
    this.expectedEditGeneration,
  });

  final FlarkViewport? previousViewport;
  final int visibleUtf16Start;
  final String visibleSource;
  final bool optimisticEditsStartAtOrAfterPreviousStart;
  final int caretUtf16;
  final bool ensureCaretVisible;
  final int? expectedEditGeneration;
}

/// Owns native viewport queries, continuation cleanup, stale-result rejection,
/// and ordered page navigation for one editor.
///
/// It does not install render/input state or notify a host. A successful query
/// returns one typed result that the host can adopt synchronously; a stale or
/// non-advancing query returns null after releasing any owned continuation.
final class FlarkEditorViewportPager {
  FlarkEditorViewportPager({
    required FlarkViewportSource source,
    required FlarkEditorCoordinator coordinator,
    int maximumVisibleBytes = 16 * 1024,
    int rowsPerPage = 32,
    int maximumCaretPageHops = 513,
  }) : _source = source,
       _coordinator = coordinator,
       _maximumVisibleBytes = maximumVisibleBytes,
       _rowsPerPage = rowsPerPage,
       _maximumCaretPageHops = maximumCaretPageHops {
    if (maximumVisibleBytes <= 0 ||
        rowsPerPage <= 0 ||
        maximumCaretPageHops <= 0) {
      throw ArgumentError('Viewport pager bounds must be positive');
    }
  }

  final FlarkViewportSource _source;
  final FlarkEditorCoordinator _coordinator;
  final int _maximumVisibleBytes;
  final int _rowsPerPage;
  final int _maximumCaretPageHops;
  final FlarkViewportNavigationState _navigation =
      FlarkViewportNavigationState();

  int get pageIndex => _navigation.pageIndex;
  bool get canPageBackward => _navigation.canPageBackward;

  bool canPageForward({
    required bool semanticsCurrent,
    required FlarkViewport? viewport,
  }) =>
      semanticsCurrent &&
      viewport != null &&
      (viewport.continuation != 0 ||
          viewport.coveredBytes.end < _source.sourceByteLength);

  void retainRefreshAnchorForEdit({
    required int editStart,
    required bool deriveFromInput,
    required FlarkViewport? currentViewport,
    required int inputGlobalUtf16Start,
    required String inputText,
  }) => _navigation.retainRefreshAnchorForEdit(
    editStart: editStart,
    deriveFromInput: deriveFromInput,
    currentViewport: currentViewport,
    inputGlobalUtf16Start: inputGlobalUtf16Start,
    inputText: inputText,
  );

  void pinRefreshAnchor(FlarkViewportPageAnchor anchor) =>
      _navigation.pinRefreshAnchor(anchor);

  void resetPagePath() => _navigation.resetPagePath();

  /// Synchronously adopts a query receipt only while its edit generation is
  /// still current. Page history advances in this same call, so the host can
  /// install the paired viewport immediately without an asynchronous gap.
  bool adopt(FlarkViewportPageResult result) {
    _requireOwnedUnsettled(result);
    final requiredGeneration = result._requiredEditGeneration;
    if (requiredGeneration != null &&
        requiredGeneration != _coordinator.editGeneration) {
      return false;
    }
    switch (result._navigationAdoption) {
      case _FlarkViewportAdvance(:final anchor):
        _navigation.advanceTo(anchor);
      case _FlarkViewportMoveBackward(:final anchor):
        _navigation.moveBackwardTo(anchor);
      case _FlarkViewportRefreshPath(:final anchors):
        _navigation.installRefreshPath(anchors);
    }
    result._settled = true;
    return true;
  }

  /// Releases a queried receipt that the host did not adopt.
  Future<void>? discard(FlarkViewportPageResult result) {
    _requireOwnedUnsettled(result);
    result._settled = true;
    if (result.viewport.continuation == 0) return null;
    return _discard(result.viewport);
  }

  void _requireOwnedUnsettled(FlarkViewportPageResult result) {
    if (!identical(result._owner, this)) {
      throw StateError('Viewport result belongs to another pager');
    }
    if (result._settled) {
      throw StateError('Viewport result is already settled');
    }
  }

  /// Reconciles refresh-origin state with one synchronous viewport install.
  void observeInstallation({
    required FlarkViewport viewport,
    required FlarkViewportInstallationPlan installation,
    required int caretUtf16,
  }) {
    final ownsCaret =
        viewport.coveredUtf16.start <= caretUtf16 &&
        caretUtf16 <= viewport.coveredUtf16.end;
    if (installation.installsCertifiedSurface && ownsCaret) {
      _navigation.clearRefreshAnchor();
    } else if (!installation.retainsExistingSurface &&
        installation.sourceFitsViewport &&
        ownsCaret) {
      _navigation.pinRefreshAnchor(
        FlarkViewportPageAnchor(
          byte: viewport.coveredBytes.start,
          utf16: viewport.coveredUtf16.start,
        ),
      );
    }
  }

  Future<FlarkViewportPageResult?> nextPage(FlarkViewport current) =>
      _deliverInCompletionTurn(() => _nextPage(current));

  Future<FlarkViewportPageResult?> _nextPage(FlarkViewport current) async {
    if (!canPageForward(semanticsCurrent: true, viewport: current)) return null;
    final stamp = _coordinator.stamp;
    FlarkViewport? queriedViewport;
    try {
      late final FlarkViewport next;
      late final FlarkViewportPageAnchor nextAnchor;
      if (current.continuation != 0) {
        next = await _source.queryViewportNext(current, maxRows: _rowsPerPage);
        nextAnchor = FlarkViewportPageAnchor(
          byte: next.coveredBytes.start,
          utf16: next.coveredUtf16.start,
        );
      } else {
        final queried = await _queryAtAnchor(
          FlarkViewportPageAnchor(
            byte: current.coveredBytes.end,
            utf16: current.coveredUtf16.end,
          ),
        );
        next = queried.viewport;
        nextAnchor = queried.anchor;
        if (nextAnchor.byte <= current.coveredBytes.start ||
            next.coveredBytes.end <= current.coveredBytes.end) {
          if (next.continuation != 0) await _discard(next);
          return null;
        }
      }
      queriedViewport = next;
      if (!_coordinator.accepts(stamp, allowClosing: true)) {
        if (next.continuation != 0) await _discard(next);
        return null;
      }
      final source = await _readSource(next);
      if (!_coordinator.accepts(stamp, allowClosing: true)) {
        if (next.continuation != 0) await _discard(next);
        return null;
      }
      queriedViewport = null;
      return FlarkViewportPageResult._(
        owner: this,
        viewport: next,
        source: source,
        requiredEditGeneration: stamp.editGeneration,
        navigationAdoption: _FlarkViewportAdvance(nextAnchor),
      );
    } catch (_) {
      final abandoned = queriedViewport;
      if (abandoned != null && abandoned.continuation != 0) {
        await _discard(abandoned);
      }
      if (!_coordinator.accepts(stamp, allowClosing: true)) return null;
      rethrow;
    }
  }

  Future<FlarkViewportPageResult?> previousPage(FlarkViewport current) =>
      _deliverInCompletionTurn(() => _previousPage(current));

  Future<FlarkViewportPageResult?> _previousPage(FlarkViewport current) async {
    final previousAnchor = _navigation.previousAnchor;
    if (previousAnchor == null) return null;
    final stamp = _coordinator.stamp;
    FlarkViewport? queriedViewport;
    try {
      if (current.continuation != 0) await _discard(current);
      if (!_coordinator.accepts(stamp, allowClosing: true)) return null;
      final queried = await _queryAtAnchor(previousAnchor);
      final previous = queried.viewport;
      queriedViewport = previous;
      if (!_coordinator.accepts(stamp, allowClosing: true)) {
        if (previous.continuation != 0) await _discard(previous);
        return null;
      }
      final source = await _readSource(previous);
      if (!_coordinator.accepts(stamp, allowClosing: true)) {
        if (previous.continuation != 0) await _discard(previous);
        return null;
      }
      queriedViewport = null;
      return FlarkViewportPageResult._(
        owner: this,
        viewport: previous,
        source: source,
        requiredEditGeneration: stamp.editGeneration,
        navigationAdoption: _FlarkViewportMoveBackward(queried.anchor),
      );
    } catch (_) {
      final abandoned = queriedViewport;
      if (abandoned != null && abandoned.continuation != 0) {
        await _discard(abandoned);
      }
      if (!_coordinator.accepts(stamp, allowClosing: true)) return null;
      rethrow;
    }
  }

  Future<FlarkViewportPageResult?> refresh(
    FlarkViewportRefreshRequest request,
  ) => _deliverInCompletionTurn(() => _refresh(request));

  Future<FlarkViewportPageResult?> _refresh(
    FlarkViewportRefreshRequest request,
  ) async {
    if (!_acceptsExpected(request.expectedEditGeneration)) return null;
    FlarkViewport? pendingViewport;
    try {
      final previous = request.previousViewport;
      FlarkViewportPageAnchor? activeOrigin;
      if (request.ensureCaretVisible) {
        final retained = _navigation.refreshAnchorForCaret(request.caretUtf16);
        if (retained != null) {
          activeOrigin = retained;
        } else if (previous != null &&
            previous.coveredUtf16.start == request.visibleUtf16Start &&
            request.optimisticEditsStartAtOrAfterPreviousStart) {
          activeOrigin = FlarkViewportPageAnchor(
            byte: previous.coveredBytes.start,
            utf16: previous.coveredUtf16.start,
          );
        }
      }
      final activeByteWindow = activeOrigin == null
          ? null
          : _navigation.byteWindowForCaret(
              origin: activeOrigin,
              visibleUtf16Start: request.visibleUtf16Start,
              visibleSource: request.visibleSource,
              caret: request.caretUtf16,
              sourceByteLength: _source.sourceByteLength,
              maximumVisibleBytes: _maximumVisibleBytes,
            );
      var requestedAnchor = FlarkViewportPageAnchor.zero;
      var requestedPageAnchors = <FlarkViewportPageAnchor>[
        FlarkViewportPageAnchor.zero,
      ];
      if (request.ensureCaretVisible &&
          previous != null &&
          _navigation.canPageBackward &&
          request.caretUtf16 >= request.visibleUtf16Start &&
          previous.coveredUtf16.start == request.visibleUtf16Start &&
          _navigation.currentPageMatches(previous) &&
          request.optimisticEditsStartAtOrAfterPreviousStart &&
          previous.coveredBytes.start <= _source.sourceByteLength) {
        requestedAnchor = _navigation.currentAnchor;
        requestedPageAnchors = _navigation.pagePath.toList(growable: true);
      }
      if (previous != null && previous.continuation != 0) {
        await _discard(previous);
      }

      var queried = await _queryAtAnchor(requestedAnchor);
      var viewport = queried.viewport;
      pendingViewport = viewport;
      requestedAnchor = queried.anchor;
      requestedPageAnchors = _navigation.pathEndingAt(
        requestedAnchor,
        requestedPageAnchors,
      );
      if (!_acceptsExpected(request.expectedEditGeneration)) {
        if (viewport.continuation != 0) await _discard(viewport);
        return null;
      }
      var pageHops = 0;
      while (request.ensureCaretVisible &&
          request.caretUtf16 > viewport.coveredUtf16.end &&
          pageHops < _maximumCaretPageHops) {
        late final FlarkViewportPageAnchor nextAnchor;
        if (viewport.continuation != 0) {
          viewport = await _source.queryViewportNext(
            viewport,
            maxRows: _rowsPerPage,
          );
          nextAnchor = FlarkViewportPageAnchor(
            byte: viewport.coveredBytes.start,
            utf16: viewport.coveredUtf16.start,
          );
        } else if (viewport.coveredBytes.end < _source.sourceByteLength) {
          final prior = viewport;
          queried = await _queryAtAnchor(
            FlarkViewportPageAnchor(
              byte: prior.coveredBytes.end,
              utf16: prior.coveredUtf16.end,
            ),
          );
          viewport = queried.viewport;
          nextAnchor = queried.anchor;
          if (nextAnchor.byte <= prior.coveredBytes.start ||
              viewport.coveredBytes.end <= prior.coveredBytes.end) {
            if (viewport.continuation != 0) await _discard(viewport);
            viewport = prior;
            break;
          }
        } else {
          break;
        }
        pendingViewport = viewport;
        requestedAnchor = nextAnchor;
        requestedPageAnchors = _navigation.pathEndingAt(
          nextAnchor,
          requestedPageAnchors,
        );
        pageHops += 1;
        if (!_acceptsExpected(request.expectedEditGeneration)) {
          if (viewport.continuation != 0) await _discard(viewport);
          return null;
        }
      }
      if (request.ensureCaretVisible &&
          request.caretUtf16 > viewport.coveredUtf16.end &&
          viewport.continuation == 0 &&
          activeByteWindow != null &&
          activeByteWindow.startByte > requestedAnchor.byte) {
        final fallback = viewport;
        queried = await _queryAtAnchor(
          FlarkViewportPageAnchor(
            byte: activeByteWindow.startByte,
            utf16: activeByteWindow.startUtf16,
          ),
        );
        final direct = queried.viewport;
        pendingViewport = direct;
        if (!_acceptsExpected(request.expectedEditGeneration)) {
          if (direct.continuation != 0) await _discard(direct);
          return null;
        }
        final directOwnsCaret =
            direct.coveredUtf16.start <= request.caretUtf16 &&
            request.caretUtf16 <= direct.coveredUtf16.end;
        if (FlarkViewportInstallationPlan.rowsFitViewport(direct) &&
            directOwnsCaret &&
            activeByteWindow.caretByte - queried.anchor.byte <=
                _maximumVisibleBytes) {
          viewport = direct;
          requestedAnchor = queried.anchor;
          requestedPageAnchors = _navigation.pathEndingAt(
            requestedAnchor,
            requestedPageAnchors,
          );
        } else {
          if (direct.continuation != 0) await _discard(direct);
          viewport = fallback;
          pendingViewport = fallback;
        }
      }
      final source = await _readSource(viewport);
      if (!_acceptsExpected(request.expectedEditGeneration)) {
        if (viewport.continuation != 0) await _discard(viewport);
        return null;
      }
      pendingViewport = null;
      return FlarkViewportPageResult._(
        owner: this,
        viewport: viewport,
        source: source,
        requiredEditGeneration: request.expectedEditGeneration,
        navigationAdoption: _FlarkViewportRefreshPath(requestedPageAnchors),
      );
    } catch (_) {
      final abandoned = pendingViewport;
      if (abandoned != null && abandoned.continuation != 0) {
        await _discard(abandoned);
      }
      if (!_acceptsExpected(request.expectedEditGeneration)) return null;
      rethrow;
    }
  }

  bool _acceptsExpected(int? expectedEditGeneration) =>
      expectedEditGeneration == null ||
      expectedEditGeneration == _coordinator.editGeneration;

  /// Delivers an async query in the operation's completion turn. Flutter can
  /// start parsing from a fake-frame callback and later join it from a real
  /// async barrier; an ordinary asynchronous completer can strand the join
  /// until another frame is pumped.
  Future<T> _deliverInCompletionTurn<T>(Future<T> Function() operation) {
    final completion = Completer<T>.sync();
    Future<T>.sync(
      operation,
    ).then<void>(completion.complete, onError: completion.completeError);
    return completion.future;
  }

  Future<FlarkViewport> _queryPageAt(int startByte) => _source.queryViewport(
    startByte: startByte,
    endByte: math.min(
      _source.sourceByteLength,
      startByte + _maximumVisibleBytes,
    ),
    maxRows: _rowsPerPage,
  );

  Future<FlarkViewportQueryPage> _queryAtAnchor(
    FlarkViewportPageAnchor requested,
  ) async {
    var viewport = await _queryPageAt(requested.byte);
    var anchor = FlarkViewportPageAnchor(
      byte: viewport.coveredBytes.start,
      utf16: viewport.coveredUtf16.start,
    );
    FlarkViewportRow? enclosing;
    for (final row in viewport.rows) {
      if (row.sourceBytes.start >= anchor.byte) continue;
      if (enclosing == null ||
          row.sourceBytes.start < enclosing.sourceBytes.start) {
        enclosing = row;
      }
    }
    if (enclosing != null) {
      if (viewport.continuation != 0) await _discard(viewport);
      anchor = FlarkViewportPageAnchor(
        byte: enclosing.sourceBytes.start,
        utf16: enclosing.sourceUtf16.start,
      );
      viewport = await _queryPageAt(anchor.byte);
      anchor = FlarkViewportPageAnchor(
        byte: viewport.coveredBytes.start,
        utf16: viewport.coveredUtf16.start,
      );
    }
    return FlarkViewportQueryPage(viewport: viewport, anchor: anchor);
  }

  Future<String> _readSource(FlarkViewport viewport) =>
      viewport.neutralSource != null
      ? Future<String>.value(viewport.neutralSource)
      : _source.readSourceRange(
          viewport.coveredBytes.start,
          viewport.coveredBytes.end,
        );

  Future<void> _discard(FlarkViewport viewport) async {
    assert(viewport.continuation != 0);
    try {
      await _source.releaseViewportContinuation(viewport);
    } catch (_) {
      // Cleanup must not turn a superseded query into an editor fault.
    }
  }
}

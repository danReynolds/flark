import 'dart:convert';

import 'package:flark/flark.dart';

import 'editor_transactions.dart';

/// Owns the ordered page path and retained refresh origin for one viewport.
///
/// This state machine knows only byte/UTF-16 page origins. It does not query or
/// release native viewports, install rows, restore input, or publish UI state.
final class FlarkViewportNavigationState {
  List<FlarkViewportPageAnchor> _pagePath = const [
    FlarkViewportPageAnchor.zero,
  ];
  FlarkViewportPageAnchor? _refreshAnchor;

  int get pageIndex => _pagePath.length - 1;
  bool get canPageBackward => _pagePath.length > 1;
  FlarkViewportPageAnchor get currentAnchor => _pagePath.last;
  FlarkViewportPageAnchor? get previousAnchor =>
      canPageBackward ? _pagePath[_pagePath.length - 2] : null;
  List<FlarkViewportPageAnchor> get pagePath =>
      List<FlarkViewportPageAnchor>.unmodifiable(_pagePath);
  FlarkViewportPageAnchor? get refreshAnchor => _refreshAnchor;

  /// Advances along a newly queried page. Any abandoned forward path is
  /// removed before the new anchor becomes current.
  void advanceTo(FlarkViewportPageAnchor anchor) {
    final next = pathEndingAt(anchor, _pagePath);
    if (next.length <= _pagePath.length ||
        !_sameAnchor(next.last, anchor) ||
        _sameAnchor(currentAnchor, anchor)) {
      throw StateError('A forward page must advance the viewport origin');
    }
    _pagePath = next;
  }

  /// Adopts the freshly resolved origin of the preceding page. A query may
  /// rewind to an enclosing row, so the resulting path is normalized rather
  /// than assuming the old anchor remains exact.
  void moveBackwardTo(FlarkViewportPageAnchor anchor) {
    if (!canPageBackward) {
      throw StateError('The viewport has no preceding page');
    }
    _pagePath = pathEndingAt(anchor, _pagePath.take(_pagePath.length - 1));
  }

  /// Atomically adopts a refresh query's complete path. The current page is
  /// always the final anchor, so page index and history cannot disagree.
  void installRefreshPath(Iterable<FlarkViewportPageAnchor> anchors) {
    final path = anchors.toList(growable: false);
    _validatePath(path);
    _pagePath = path;
  }

  void resetPagePath() {
    _pagePath = const [FlarkViewportPageAnchor.zero];
  }

  bool currentPageMatches(FlarkViewport viewport) =>
      currentAnchor.byte == viewport.coveredBytes.start &&
      currentAnchor.utf16 == viewport.coveredUtf16.start;

  FlarkViewportPageAnchor knownAnchorFor(
    int editStart, {
    FlarkViewport? currentViewport,
  }) {
    var candidate = FlarkViewportPageAnchor.zero;
    for (final anchor in _pagePath) {
      if (anchor.utf16 <= editStart && anchor.utf16 >= candidate.utf16) {
        candidate = anchor;
      }
    }
    if (currentViewport != null &&
        currentViewport.coveredUtf16.start <= editStart &&
        currentViewport.coveredUtf16.start >= candidate.utf16) {
      candidate = FlarkViewportPageAnchor(
        byte: currentViewport.coveredBytes.start,
        utf16: currentViewport.coveredUtf16.start,
      );
    }
    return candidate;
  }

  /// Keeps the earliest still-relevant origin across optimistic edits. A later
  /// edit cannot move the next certification query past an earlier edit.
  void retainRefreshAnchorForEdit({
    required int editStart,
    required bool deriveFromInput,
    required FlarkViewport? currentViewport,
    required int inputGlobalUtf16Start,
    required String inputText,
  }) {
    final retained = _refreshAnchor;
    if (retained != null && retained.utf16 <= editStart) return;
    var candidate = knownAnchorFor(editStart, currentViewport: currentViewport);
    final inputEnd = inputGlobalUtf16Start + inputText.length;
    if (deriveFromInput &&
        currentViewport != null &&
        editStart < currentViewport.coveredUtf16.start &&
        inputGlobalUtf16Start <= editStart &&
        currentViewport.coveredUtf16.start <= inputEnd) {
      final localEdit = editStart - inputGlobalUtf16Start;
      final localOrigin =
          currentViewport.coveredUtf16.start - inputGlobalUtf16Start;
      var localAnchor = localEdit == 0
          ? 0
          : inputText.lastIndexOf('\n', localEdit - 1) + 1;
      if (localAnchor == localEdit && localAnchor > 0) {
        localAnchor = localAnchor == 1
            ? 0
            : inputText.lastIndexOf('\n', localAnchor - 2) + 1;
      }
      if (localAnchor <= localOrigin) {
        final byteDistance = utf8
            .encode(inputText.substring(localAnchor, localOrigin))
            .length;
        final derived = FlarkViewportPageAnchor(
          byte: currentViewport.coveredBytes.start - byteDistance,
          utf16: inputGlobalUtf16Start + localAnchor,
        );
        if (derived.byte >= 0 && derived.utf16 >= candidate.utf16) {
          candidate = derived;
        }
      }
    }
    _refreshAnchor = candidate;
  }

  /// Pins a parser/Core-authored origin that is stronger than a derived page
  /// history anchor.
  void pinRefreshAnchor(FlarkViewportPageAnchor anchor) {
    _validateAnchor(anchor);
    _refreshAnchor = anchor;
  }

  FlarkViewportPageAnchor? refreshAnchorForCaret(int caret) {
    final retained = _refreshAnchor;
    return retained != null && retained.utf16 <= caret ? retained : null;
  }

  void clearRefreshAnchor() {
    _refreshAnchor = null;
  }

  ({int startByte, int startUtf16, int caretByte})? byteWindowForCaret({
    required FlarkViewportPageAnchor origin,
    required int visibleUtf16Start,
    required String visibleSource,
    required int caret,
    required int sourceByteLength,
    required int maximumVisibleBytes,
  }) {
    final localOrigin = origin.utf16 - visibleUtf16Start;
    final localCaret = caret - visibleUtf16Start;
    if (localOrigin < 0 ||
        localOrigin > visibleSource.length ||
        localCaret < 0 ||
        localCaret > visibleSource.length) {
      return null;
    }

    var lineStart = localCaret == 0
        ? 0
        : visibleSource.lastIndexOf('\n', localCaret - 1) + 1;
    if (lineStart == localCaret && lineStart > 0) {
      lineStart = lineStart == 1
          ? 0
          : visibleSource.lastIndexOf('\n', lineStart - 2) + 1;
    }
    final startByte = lineStart >= localOrigin
        ? origin.byte +
              utf8
                  .encode(visibleSource.substring(localOrigin, lineStart))
                  .length
        : origin.byte -
              utf8
                  .encode(visibleSource.substring(lineStart, localOrigin))
                  .length;
    final caretByte =
        startByte +
        utf8.encode(visibleSource.substring(lineStart, localCaret)).length;
    if (startByte < 0 ||
        caretByte < startByte ||
        caretByte > sourceByteLength ||
        caretByte - startByte > maximumVisibleBytes) {
      return null;
    }
    return (
      startByte: startByte,
      startUtf16: visibleUtf16Start + lineStart,
      caretByte: caretByte,
    );
  }

  List<FlarkViewportPageAnchor> pathEndingAt(
    FlarkViewportPageAnchor anchor,
    Iterable<FlarkViewportPageAnchor> history,
  ) {
    _validateAnchor(anchor);
    if (_sameAnchor(anchor, FlarkViewportPageAnchor.zero)) {
      return const [FlarkViewportPageAnchor.zero];
    }
    final result = <FlarkViewportPageAnchor>[FlarkViewportPageAnchor.zero];
    for (final candidate in history) {
      if (candidate.byte <= 0 ||
          candidate.byte >= anchor.byte ||
          candidate.utf16 > anchor.utf16) {
        continue;
      }
      final last = result.last;
      if (candidate.byte > last.byte && candidate.utf16 >= last.utf16) {
        result.add(candidate);
      }
    }
    if (!_sameAnchor(result.last, anchor)) result.add(anchor);
    _validatePath(result);
    return result;
  }

  static bool _sameAnchor(
    FlarkViewportPageAnchor left,
    FlarkViewportPageAnchor right,
  ) => left.byte == right.byte && left.utf16 == right.utf16;

  static void _validateAnchor(FlarkViewportPageAnchor anchor) {
    if (anchor.byte < 0 || anchor.utf16 < 0) {
      throw ArgumentError.value(anchor, 'anchor', 'must be nonnegative');
    }
  }

  static void _validatePath(List<FlarkViewportPageAnchor> path) {
    if (path.isEmpty ||
        !_sameAnchor(path.first, FlarkViewportPageAnchor.zero)) {
      throw ArgumentError.value(path, 'anchors', 'must begin at zero');
    }
    for (var index = 1; index < path.length; index++) {
      final previous = path[index - 1];
      final current = path[index];
      _validateAnchor(current);
      if (current.byte <= previous.byte || current.utf16 < previous.utf16) {
        throw ArgumentError.value(
          path,
          'anchors',
          'must advance in byte order without rewinding UTF-16',
        );
      }
    }
  }
}

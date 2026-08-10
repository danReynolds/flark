import 'package:characters/characters.dart';

import 'document.dart';

/// Which splice edge a caret or selection endpoint follows when an edit lands
/// exactly on it.
enum FlarkCoreAffinity { upstream, downstream }

/// Extended-grapheme-cluster policy pinned to `package:characters` 1.4.1
/// (Unicode 16.0.0). Pure functions over a caller-supplied bounded context:
/// callers provide the text window, so no function here can scan a document.
abstract final class FlarkCoreGraphemePolicy {
  /// The cluster range that Backspace at [offset] removes, or null at the
  /// window start.
  static (int, int)? previousClusterRange(String text, int offset) {
    if (offset <= 0 || offset > text.length) return null;
    final cluster = CharacterRange.at(text, offset);
    if (cluster.isEmpty && !cluster.moveBack()) return null;
    return (cluster.stringBeforeLength, offset);
  }

  /// The cluster range that forward Delete at [offset] removes, or null at
  /// the window end.
  static (int, int)? nextClusterRange(String text, int offset) {
    if (offset < 0 || offset >= text.length) return null;
    final cluster = CharacterRange.at(text, offset);
    if (cluster.isEmpty && !cluster.moveNext()) return null;
    return (offset, text.length - cluster.stringAfterLength);
  }

  static bool isSingleCluster(String text) =>
      text.isNotEmpty && text.characters.length == 1;
}

/// A canonical selection observation: plain UTF-16 offsets valid at
/// [revision], identified by [generation]. [adapterState] carries an opaque
/// host payload (for example a platform selection object) through history
/// restoration without the core interpreting it.
final class FlarkCoreSelectionSnapshot {
  const FlarkCoreSelectionSnapshot({
    required this.base,
    required this.extent,
    this.affinity = FlarkCoreAffinity.downstream,
    this.generation = 0,
    this.revision = 0,
    this.adapterState,
  });

  final int base;
  final int extent;
  final FlarkCoreAffinity affinity;
  final int generation;
  final int revision;
  final Object? adapterState;

  bool get isCollapsed => base == extent;
}

/// One user-visible undo/redo unit: the ordered native history tokens plus
/// the selections to restore on either side of the unit.
final class _HistoryUnit {
  const _HistoryUnit({
    required this.tokens,
    required this.beforeSelection,
    required this.afterSelection,
    this.typingEnd,
    this.typingAtMicros,
    this.typingEpoch,
    this.compositionGroup,
  });

  final List<FlarkCoreHistoryToken> tokens;
  final FlarkCoreSelectionSnapshot beforeSelection;
  final FlarkCoreSelectionSnapshot afterSelection;
  final int? typingEnd;
  final int? typingAtMicros;
  final int? typingEpoch;
  final int? compositionGroup;
}

sealed class FlarkCoreHistoryOutcome {
  const FlarkCoreHistoryOutcome(this.restoreSelection);

  /// The canonical selection to restore: the unit's before-selection for
  /// undo, its after-selection for redo.
  final FlarkCoreSelectionSnapshot restoreSelection;
}

/// The unit was replayed atomically and the opposite direction now holds its
/// inverse.
final class FlarkCoreHistoryReplayed extends FlarkCoreHistoryOutcome {
  const FlarkCoreHistoryReplayed(super.restoreSelection, this.receipt);

  /// The receipt of the final replayed token: current revision and lengths.
  final FlarkCoreEditReceipt receipt;
}

/// Native retention evicted part of the requested unit, so the entire history
/// was released and both stacks are now empty. Source is unchanged or rolled
/// back exactly; the host should refresh derived state.
final class FlarkCoreHistoryDropped extends FlarkCoreHistoryOutcome {
  const FlarkCoreHistoryDropped(super.restoreSelection);
}

const _typingIdleMicros = 1000000;
const _historyTokenEvicted = 0x0305;
const _historyTokenStale = 0x0306;

/// Canonical editing policy over one [FlarkCoreDocument]: undo/redo ordering
/// and grouping of opaque native history tokens, typing coalescing,
/// composition grouping, and the anchor-backed canonical selection.
///
/// This class holds no Markdown knowledge and no document text beyond the
/// bounded arguments callers pass in. Callers serialize their own mutations:
/// operations must be awaited in order, never interleaved.
final class FlarkCoreEditorSession {
  FlarkCoreEditorSession(this.document, {int Function()? clockMicros})
    : _clockMicros = clockMicros ?? _stopwatchClock();

  static int Function() _stopwatchClock() {
    final stopwatch = Stopwatch()..start();
    return () => stopwatch.elapsedMicroseconds;
  }

  final FlarkCoreDocument document;
  final int Function() _clockMicros;

  final List<_HistoryUnit> _undoUnits = [];
  final List<_HistoryUnit> _redoUnits = [];
  int _typingEpoch = 0;
  int _activeCompositionGroup = 0;
  int _nextCompositionGroup = 0;

  FlarkCoreAnchor? _selectionStart;
  FlarkCoreAnchor? _selectionEnd;
  bool _selectionBaseIsStart = true;
  FlarkCoreAffinity _selectionAffinity = FlarkCoreAffinity.downstream;
  Object? _selectionAdapterState;
  int _selectionGeneration = 0;

  bool get canUndo => _undoUnits.isNotEmpty;
  bool get canRedo => _redoUnits.isNotEmpty;
  bool get compositionActive => _activeCompositionGroup != 0;
  int get selectionGeneration => _selectionGeneration;
  bool get hasCanonicalSelection => _selectionStart != null;

  /// Applies one revision-checked source edit and records it as history.
  ///
  /// With [coalesceTyping], a single-cluster insertion coalesces into the
  /// previous unit while it stays adjacent, inside the one-second idle
  /// window, and in the same typing epoch. A non-null [compositionGroup]
  /// (from [compositionGroupForMutation], claimed synchronously in callback
  /// order) instead groups the edit with that composition unit.
  Future<FlarkCoreEditReceipt> applyEditUtf16(
    int start,
    int end,
    String replacement, {
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    bool coalesceTyping = false,
    int? compositionGroup,
  }) async {
    final typing = coalesceTyping && compositionGroup == null
        ? _typingEvent(
            start: start,
            end: end,
            replacement: replacement,
            beforeSelection: beforeSelection,
            afterSelection: afterSelection,
          )
        : null;
    final receipt = await document.applyEditUtf16(start, end, replacement);
    await _recordForward(
      receipt,
      beforeSelection: beforeSelection,
      afterSelection: afterSelection,
      typing: typing,
      compositionGroup: compositionGroup,
    );
    return receipt;
  }

  /// Claims the history group for a mutation observed with the given
  /// composing state. Must be called synchronously in platform callback
  /// order: an active composition keeps its group, the mutation that ends a
  /// composition still joins it, and a mutation outside composition returns
  /// null.
  int? compositionGroupForMutation({required bool composingActive}) =>
      _compositionGroupForMutation(composingActive);

  /// Ends the current coalescible typing run; the next insertion starts a new
  /// undo unit.
  void breakTypingGroup() {
    _typingEpoch += 1;
  }

  /// Observes a platform composing-range update that carried no source
  /// mutation. Returns true when an active composition just ended.
  bool trackCompositionWithoutMutation({required bool composingActive}) {
    final wasActive = _activeCompositionGroup != 0;
    if (composingActive) {
      if (!wasActive) _activeCompositionGroup = ++_nextCompositionGroup;
      return false;
    }
    _activeCompositionGroup = 0;
    return wasActive;
  }

  void endCompositionGroup() {
    _activeCompositionGroup = 0;
  }

  Future<FlarkCoreHistoryOutcome?> undo() => _replayDirection(undo: true);

  Future<FlarkCoreHistoryOutcome?> redo() => _replayDirection(undo: false);

  /// Releases every retained unit in both directions.
  Future<void> clearHistory() async {
    final units = [..._undoUnits, ..._redoUnits];
    _undoUnits.clear();
    _redoUnits.clear();
    await _releaseUnits(units);
  }

  /// Replaces the canonical selection with anchor-backed authority.
  ///
  /// Endpoint policy: a collapsed caret follows text inserted at it; a range
  /// excludes text inserted exactly at either edge. The previous anchors are
  /// released and the selection generation increases.
  Future<int> setSelectionUtf16(
    int base,
    int extent, {
    FlarkCoreAffinity affinity = FlarkCoreAffinity.downstream,
    Object? adapterState,
  }) async {
    final start = base <= extent ? base : extent;
    final end = base <= extent ? extent : base;
    final collapsed = start == end;
    final nextStart = await document.createAnchorUtf16(
      start,
      downstream: true,
    );
    final nextEnd = collapsed
        ? nextStart
        : await document.createAnchorUtf16(end, downstream: false);
    await _releaseSelectionAnchors();
    _selectionStart = nextStart;
    _selectionEnd = nextEnd;
    _selectionBaseIsStart = base <= extent;
    _selectionAffinity = affinity;
    _selectionAdapterState = adapterState;
    return ++_selectionGeneration;
  }

  /// Resolves the canonical selection at the current revision, or null when
  /// no canonical selection is installed.
  Future<FlarkCoreSelectionSnapshot?> resolveSelection() async {
    final startAnchor = _selectionStart;
    final endAnchor = _selectionEnd;
    if (startAnchor == null || endAnchor == null) return null;
    final generation = _selectionGeneration;
    final start = await document.resolveAnchorUtf16(startAnchor);
    final end = identical(endAnchor, startAnchor)
        ? start
        : await document.resolveAnchorUtf16(endAnchor);
    if (generation != _selectionGeneration) return null;
    return FlarkCoreSelectionSnapshot(
      base: _selectionBaseIsStart ? start : end,
      extent: _selectionBaseIsStart ? end : start,
      affinity: _selectionAffinity,
      generation: generation,
      revision: document.revision,
      adapterState: _selectionAdapterState,
    );
  }

  Future<void> clearSelection() async {
    await _releaseSelectionAnchors();
    _selectionAdapterState = null;
    _selectionGeneration += 1;
  }

  /// Releases history and selection authority. The document itself is owned
  /// by the caller and stays open.
  Future<void> dispose() async {
    await clearHistory();
    await _releaseSelectionAnchors();
  }

  Future<void> _releaseSelectionAnchors() async {
    final start = _selectionStart;
    final end = _selectionEnd;
    _selectionStart = null;
    _selectionEnd = null;
    if (start != null) {
      try {
        await document.releaseAnchor(start);
      } on Object {
        // Close reclamation may already have drained the anchor.
      }
    }
    if (end != null && !identical(end, start)) {
      try {
        await document.releaseAnchor(end);
      } on Object {
        // Close reclamation may already have drained the anchor.
      }
    }
  }

  ({int end, int atMicros})? _typingEvent({
    required int start,
    required int end,
    required String replacement,
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
  }) {
    if (start != end ||
        replacement.contains('\n') ||
        replacement.contains('\r') ||
        !FlarkCoreGraphemePolicy.isSingleCluster(replacement) ||
        !beforeSelection.isCollapsed ||
        beforeSelection.extent != start ||
        !afterSelection.isCollapsed ||
        afterSelection.extent != start + replacement.length) {
      return null;
    }
    return (end: start + replacement.length, atMicros: _clockMicros());
  }

  int? _compositionGroupForMutation(bool composingActive) {
    final wasActive = _activeCompositionGroup != 0;
    if (!wasActive && !composingActive) return null;
    final group = wasActive
        ? _activeCompositionGroup
        : ++_nextCompositionGroup;
    _activeCompositionGroup = composingActive ? group : 0;
    return group;
  }

  Future<void> _recordForward(
    FlarkCoreEditReceipt receipt, {
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    required ({int end, int atMicros})? typing,
    required int? compositionGroup,
  }) async {
    final stale = _redoUnits.toList(growable: false);
    _redoUnits.clear();
    await _releaseUnits(stale);
    final token = receipt.historyToken;
    if (token == null) {
      // Without a retained inverse the timeline breaks: older units can no
      // longer replay against the new source state.
      final broken = _undoUnits.toList(growable: false);
      _undoUnits.clear();
      await _releaseUnits(broken);
      return;
    }
    final previous = _undoUnits.isEmpty ? null : _undoUnits.last;
    if (compositionGroup != null &&
        previous != null &&
        previous.compositionGroup == compositionGroup) {
      _undoUnits[_undoUnits.length - 1] = _HistoryUnit(
        tokens: List.unmodifiable([...previous.tokens, token]),
        beforeSelection: previous.beforeSelection,
        afterSelection: afterSelection,
        compositionGroup: compositionGroup,
      );
      return;
    }
    if (typing != null &&
        previous != null &&
        previous.typingEnd == beforeSelection.extent &&
        previous.typingAtMicros != null &&
        typing.atMicros - previous.typingAtMicros! <= _typingIdleMicros &&
        previous.typingEpoch == _typingEpoch) {
      _undoUnits[_undoUnits.length - 1] = _HistoryUnit(
        tokens: List.unmodifiable([...previous.tokens, token]),
        beforeSelection: previous.beforeSelection,
        afterSelection: afterSelection,
        typingEnd: typing.end,
        typingAtMicros: typing.atMicros,
        typingEpoch: _typingEpoch,
      );
      return;
    }
    _undoUnits.add(
      _HistoryUnit(
        tokens: [token],
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        typingEnd: typing?.end,
        typingAtMicros: typing?.atMicros,
        typingEpoch: typing == null ? null : _typingEpoch,
        compositionGroup: compositionGroup,
      ),
    );
  }

  Future<FlarkCoreHistoryOutcome?> _replayDirection({
    required bool undo,
  }) async {
    final source = undo ? _undoUnits : _redoUnits;
    final destination = undo ? _redoUnits : _undoUnits;
    if (source.isEmpty) return null;
    _breakActiveGroups();
    final unit = source.removeLast();
    final replayed = await _replayUnit(unit);
    if (replayed == null) {
      final stale = [...source, ...destination];
      source.clear();
      destination.clear();
      await _releaseUnits(stale);
      // Source is unchanged (or rolled back exactly), so the selection to
      // restore is the unit's current-state side, not its replayed side.
      return FlarkCoreHistoryDropped(
        undo ? unit.afterSelection : unit.beforeSelection,
      );
    }
    destination.add(replayed.unit);
    return FlarkCoreHistoryReplayed(
      undo ? unit.beforeSelection : unit.afterSelection,
      replayed.receipt,
    );
  }

  void _breakActiveGroups() {
    _typingEpoch += 1;
    _activeCompositionGroup = 0;
  }

  Future<({_HistoryUnit unit, FlarkCoreEditReceipt receipt})?> _replayUnit(
    _HistoryUnit unit,
  ) async {
    final reverseTokens = <FlarkCoreHistoryToken>[];
    late FlarkCoreEditReceipt receipt;
    for (final token in unit.tokens.reversed) {
      try {
        receipt = await document.replayHistory(token);
      } on FlarkCoreNativeException catch (error) {
        if (error.status == _historyTokenEvicted ||
            error.status == _historyTokenStale) {
          await _rollback(reverseTokens);
          return null;
        }
        rethrow;
      }
      final reverseToken = receipt.historyToken;
      if (reverseToken == null) {
        throw StateError(
          'Flark could not retain the inverse of a grouped history replay',
        );
      }
      reverseTokens.add(reverseToken);
    }
    return (
      unit: _HistoryUnit(
        tokens: List.unmodifiable(reverseTokens),
        beforeSelection: unit.beforeSelection,
        afterSelection: unit.afterSelection,
      ),
      receipt: receipt,
    );
  }

  Future<void> _rollback(List<FlarkCoreHistoryToken> reverseTokens) async {
    for (final token in reverseTokens.reversed) {
      final receipt = await document.replayHistory(token);
      final restoredToken = receipt.historyToken;
      if (restoredToken == null) {
        throw StateError('Flark could not roll back a grouped history replay');
      }
      try {
        await document.releaseHistory(restoredToken);
      } on Object {
        // The source is restored even if native retention raced eviction.
      }
    }
  }

  Future<void> _releaseUnits(List<_HistoryUnit> units) async {
    for (final unit in units) {
      for (final token in unit.tokens) {
        try {
          await document.releaseHistory(token);
        } on Object {
          // Retention is bounded and may already have evicted an old token.
        }
      }
    }
  }
}

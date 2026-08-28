import 'dart:convert';

import 'package:characters/characters.dart';

import 'document.dart';

/// Which splice edge a caret or selection endpoint follows when an edit lands
/// exactly on it.
enum FlarkCoreAffinity { upstream, downstream }

/// A parser-certified edit whose target is independent of the current text
/// selection. Frontends provide only a source position inside the target row;
/// Rust owns the Markdown mutation and Core preserves canonical selection.
enum FlarkCoreSemanticActionV1 { toggleTaskChecked }

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

  /// The largest extended-grapheme-cluster boundary at or before [offset].
  ///
  /// Hosts that must cut a text window — a render surface splitting a long
  /// row into bounded fragments, for example — use this so a cut never lands
  /// inside a cluster and render one cluster as two.
  static int clusterBoundaryAtOrBefore(String text, int offset) {
    if (offset <= 0) return 0;
    if (offset >= text.length) return text.length;
    // `CharacterRange.at` is empty exactly when the index already sits on a
    // cluster boundary, and otherwise expands to the containing cluster.
    final cluster = CharacterRange.at(text, offset);
    return cluster.isEmpty ? offset : cluster.stringBeforeLength;
  }

  /// The smallest extended-grapheme-cluster boundary at or after [offset].
  static int clusterBoundaryAtOrAfter(String text, int offset) {
    if (offset <= 0) return 0;
    if (offset >= text.length) return text.length;
    final cluster = CharacterRange.at(text, offset);
    return cluster.isEmpty ? offset : text.length - cluster.stringAfterLength;
  }
}

/// One source-mapped visible run supplied to Core before a semantic command.
/// The frontend maps its presentation model into this scalar-only context;
/// Core uses it only after Rust has committed and authenticated the exact
/// delete-to-empty owner closure.
final class FlarkCoreInlineContinuationRunV1 {
  const FlarkCoreInlineContinuationRunV1({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.semanticallyStyled,
  });

  final String text;
  final int sourceUtf16Start;
  final int sourceUtf16End;
  final bool semanticallyStyled;
}

/// Bounded pre-command evidence from the current parser-authored row.
///
/// This is not command authority: Rust still decides whether the semantic
/// deletion applies. It merely gives Core enough immutable context to derive
/// the next writable intent inside the same serialized command, before the
/// history unit is recorded.
final class FlarkCoreInlineContinuationContextV1 {
  FlarkCoreInlineContinuationContextV1({
    required this.revision,
    required this.sourceUtf16Start,
    required this.source,
    required List<FlarkCoreInlineContinuationRunV1> runs,
  }) : runs = List.unmodifiable(runs);

  final int revision;
  final int sourceUtf16Start;
  final String source;
  final List<FlarkCoreInlineContinuationRunV1> runs;

  FlarkCoreInlineContinuationV1? resolve(FlarkCoreEditIntentReceiptV1 receipt) {
    if (!receipt.hasCommit ||
        receipt.baseRevision != revision ||
        receipt.presentationTransition !=
            FlarkCoreEditPresentationTransitionV1.deleteInlineOwner ||
        receipt.replacement.isNotEmpty ||
        receipt.baseUtf16Start >= receipt.baseUtf16End) {
      return null;
    }
    final localStart = receipt.baseUtf16Start - sourceUtf16Start;
    final localEnd = receipt.baseUtf16End - sourceUtf16Start;
    if (localStart < 0 || localEnd > source.length) return null;
    final deletedSource = source.substring(localStart, localEnd);
    final deletedRuns = runs
        .where(
          (run) =>
              receipt.baseUtf16Start <= run.sourceUtf16Start &&
              run.sourceUtf16End <= receipt.baseUtf16End &&
              run.text.isNotEmpty,
        )
        .toList(growable: false);
    final visible = deletedRuns.map((run) => run.text).join();
    if (deletedRuns.isEmpty ||
        deletedRuns.any((run) => !run.semanticallyStyled) ||
        !FlarkCoreGraphemePolicy.isSingleCluster(visible)) {
      return null;
    }
    final prefixEnd =
        deletedRuns.first.sourceUtf16Start - receipt.baseUtf16Start;
    final suffixStart =
        deletedRuns.last.sourceUtf16End - receipt.baseUtf16Start;
    if (prefixEnd <= 0 ||
        prefixEnd > suffixStart ||
        suffixStart >= deletedSource.length) {
      return null;
    }
    return FlarkCoreInlineContinuationV1(
      revision: receipt.resultRevision,
      caretUtf16: receipt.resultSelectionUtf16,
      prefix: deletedSource.substring(0, prefixEnd),
      suffix: deletedSource.substring(suffixStart),
    );
  }
}

/// Typed next-write intent left by deleting the final visible grapheme from a
/// supported inline owner. It is recorded atomically with source, selection,
/// and history by [FlarkCoreEditorSession].
final class FlarkCoreInlineContinuationV1 {
  const FlarkCoreInlineContinuationV1({
    required this.revision,
    required this.caretUtf16,
    required this.prefix,
    required this.suffix,
  });

  final int revision;
  final int caretUtf16;
  final String prefix;
  final String suffix;

  /// The bounded D0 continuation vocabulary. Batches of ordinary prose are
  /// equivalent to sequential key delivery. Whitespace and Markdown-active
  /// ASCII punctuation leave the emptied owner rather than blindly forming a
  /// delimiter collision.
  bool acceptsReplacement(String value) {
    if (value.isEmpty) return false;
    for (final rune in value.runes) {
      final scalar = String.fromCharCode(rune);
      if (scalar.trim().isEmpty) return false;
      if (rune > 0x7f) continue;
      final alphanumeric =
          (0x30 <= rune && rune <= 0x39) ||
          (0x41 <= rune && rune <= 0x5a) ||
          (0x61 <= rune && rune <= 0x7a);
      const safeProse = <int>{
        0x21,
        0x22,
        0x27,
        0x28,
        0x29,
        0x2c,
        0x2d,
        0x2e,
        0x3a,
        0x3b,
        0x3f,
      };
      if (!alphanumeric && !safeProse.contains(rune)) return false;
    }
    return true;
  }

  FlarkCoreInlineContinuationV1 atRevision(int nextRevision, int caret) =>
      FlarkCoreInlineContinuationV1(
        revision: nextRevision,
        caretUtf16: caret,
        prefix: prefix,
        suffix: suffix,
      );
}

final class FlarkCoreEditIntentOutcomeV1 {
  const FlarkCoreEditIntentOutcomeV1({
    required this.receipt,
    this.inlineContinuation,
  });

  final FlarkCoreEditIntentReceiptV1 receipt;
  final FlarkCoreInlineContinuationV1? inlineContinuation;
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
    this.inlineContinuation,
  });

  final int base;
  final int extent;
  final FlarkCoreAffinity affinity;
  final int generation;
  final int revision;
  final Object? adapterState;
  final FlarkCoreInlineContinuationV1? inlineContinuation;

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
    this.nativeHistoryGroupId = 0,
    this.nativeHistoryStepCount = 1,
  });

  final List<FlarkCoreHistoryToken> tokens;
  final FlarkCoreSelectionSnapshot beforeSelection;
  final FlarkCoreSelectionSnapshot afterSelection;
  final int? typingEnd;
  final int? typingAtMicros;
  final int? typingEpoch;
  final int? compositionGroup;
  final int nativeHistoryGroupId;
  final int nativeHistoryStepCount;
}

/// Native anchors that preserve the selection on the precomposition side of
/// a composition history unit.
///
/// The endpoints use outward affinity, rather than the canonical selection's
/// inward affinity. A replacement may therefore collapse or cross these
/// anchors while composing, but replaying the native inverse returns both to
/// their exact original offsets without allocating after the source rewind.
final class _CompositionScope {
  const _CompositionScope({
    required this.group,
    required this.baseAnchor,
    required this.extentAnchor,
    required this.baseSelection,
  });

  final int group;
  final FlarkCoreAnchor baseAnchor;
  final FlarkCoreAnchor extentAnchor;
  final FlarkCoreSelectionSnapshot baseSelection;
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
const _sourceTransactionV1MaximumReplacementBytes = 64 * 1024;
// A valid UTF-8 source scalar uses at most three bytes per UTF-16 code unit.
// This conservative host-side cut guarantees native's 64 KiB inverse bound
// without a coordinate/source preflight on the common transaction lane.
const _sourceTransactionV1MaximumDeletedUtf16 = 16 * 1024;
// Keep one native composite both replay-atomic and strictly bounded. Once a
// user-visible group reaches the native cap, Core starts a new undo unit
// instead of silently falling back to a multi-token partial-replay path.
const _maximumNativeCompositeHistorySteps = 256;

/// Canonical editing policy over one [FlarkCoreDocument]: undo/redo ordering
/// and grouping of opaque native history tokens, typing coalescing,
/// composition grouping, and the anchor-backed canonical selection.
///
/// This class holds no document text beyond bounded caller arguments. Literal
/// policy remains host-neutral while the named `flark-edit-v1` semantic
/// commands are resolved by Rust. Every logical mutation is serialized by the
/// private command gate, including its history and selection adoption.
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
  _CompositionScope? _compositionScope;
  Future<void> _commandTail = Future<void>.value();
  int _pendingCommandCount = 0;
  int _nextLogicalEditId = 0;
  int _pendingTerminalLogicalEditId = 0;
  int _nextNativeTypingHistoryGroup = 0;
  bool _postCommitUnknown = false;

  FlarkCoreAnchor? _selectionStart;
  FlarkCoreAnchor? _selectionEnd;
  bool _selectionBaseIsStart = true;
  FlarkCoreAffinity _selectionAffinity = FlarkCoreAffinity.downstream;
  Object? _selectionAdapterState;
  FlarkCoreInlineContinuationV1? _selectionInlineContinuation;
  int _selectionGeneration = 0;
  int _selectionBaseUtf16 = 0;
  int _selectionExtentUtf16 = 0;

  bool get canUndo => _undoUnits.isNotEmpty;
  bool get canRedo => _redoUnits.isNotEmpty;
  bool get compositionActive => _activeCompositionGroup != 0;
  int get selectionGeneration => _selectionGeneration;
  bool get hasCanonicalSelection => _selectionStart != null;
  bool get postCommitUnknown => _postCommitUnknown;

  /// Cancels the active composition as one authoritative history rewind.
  ///
  /// Intermediate composition updates are already one native composite
  /// history token. Cancellation replays that token once, releases the
  /// otherwise-created redo inverse, and restores the precomposition
  /// selection. Earlier history remains replayable because native also
  /// restores the composition token's target history state.
  Future<FlarkCoreHistoryOutcome?> cancelComposition() =>
      _serializeCommand(_cancelComposition);

  Future<FlarkCoreHistoryOutcome?> _cancelComposition() async {
    _ensureAuthoritativeCommandsAvailable();
    final group = _activeCompositionGroup;
    _activeCompositionGroup = 0;
    if (group == 0) return null;
    if (_undoUnits.isEmpty || _undoUnits.last.compositionGroup != group) {
      await _releaseCompositionScope(group);
      return null;
    }

    final unit = _undoUnits.removeLast();
    final scope = _compositionScope;
    if (scope == null || scope.group != group) {
      _postCommitUnknown = true;
      throw StateError('Flark composition history omitted its base anchors');
    }
    final replayed = await _replayUnit(unit);
    if (replayed == null) {
      final stale = [..._undoUnits, ..._redoUnits];
      _undoUnits.clear();
      _redoUnits.clear();
      await _releaseUnits(stale);
      await _releaseCompositionScope(group);
      _adoptSelectionMetadata(unit.afterSelection);
      return FlarkCoreHistoryDropped(unit.afterSelection);
    }

    await _releaseUnits([replayed.unit]);
    await _adoptCompositionBase(scope);
    return FlarkCoreHistoryReplayed(scope.baseSelection, replayed.receipt);
  }

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
    bool compositionFinal = false,
  }) {
    final queuedAt = _clockMicros();
    final admittedTypingEpoch = _typingEpoch;
    return _serializeCommand(
      () => _applyEditUtf16(
        start,
        end,
        replacement,
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        coalesceTyping: coalesceTyping,
        compositionGroup: compositionGroup,
        compositionFinal: compositionFinal,
        typingAtMicros: queuedAt,
        typingEpoch: admittedTypingEpoch,
        coreQueueMicros: _clockMicros() - queuedAt,
      ),
    );
  }

  Future<FlarkCoreEditReceipt> _applyEditUtf16(
    int start,
    int end,
    String replacement, {
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    bool coalesceTyping = false,
    int? compositionGroup,
    bool compositionFinal = false,
    required int typingAtMicros,
    required int typingEpoch,
    required int coreQueueMicros,
  }) async {
    _ensureAuthoritativeCommandsAvailable();
    if (_selectionStart == null ||
        _selectionEnd == null ||
        _selectionBaseUtf16 != beforeSelection.base ||
        _selectionExtentUtf16 != beforeSelection.extent) {
      await _setSelectionUtf16(
        beforeSelection.base,
        beforeSelection.extent,
        affinity: beforeSelection.affinity,
        adapterState: beforeSelection.adapterState,
        inlineContinuation: beforeSelection.inlineContinuation,
      );
    } else {
      _selectionAffinity = beforeSelection.affinity;
      _selectionAdapterState = beforeSelection.adapterState;
      _selectionInlineContinuation = beforeSelection.inlineContinuation;
    }
    if (compositionFinal && compositionGroup == null) {
      throw ArgumentError(
        'A final composition mutation requires its composition group',
      );
    }
    if (compositionGroup != null) {
      await _pinCompositionBase(compositionGroup, beforeSelection);
    }
    if (end - start > _sourceTransactionV1MaximumDeletedUtf16 ||
        utf8.encode(replacement).length >
            _sourceTransactionV1MaximumReplacementBytes) {
      final stagedResultCaret = start + replacement.length;
      if (afterSelection.base == stagedResultCaret &&
          afterSelection.extent == stagedResultCaret) {
        return _applyStagedSourceTransactionUtf16(
          start,
          end,
          replacement,
          beforeSelection: beforeSelection,
          afterSelection: afterSelection,
          coalesceTyping: coalesceTyping,
          compositionGroup: compositionGroup,
          compositionFinal: compositionFinal,
          typingAtMicros: typingAtMicros,
          typingEpoch: typingEpoch,
          coreQueueMicros: coreQueueMicros,
        );
      }
      return _applyLegacyBulkEditUtf16(
        start,
        end,
        replacement,
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        coalesceTyping: coalesceTyping,
        compositionGroup: compositionGroup,
        compositionFinal: compositionFinal,
        typingAtMicros: typingAtMicros,
        typingEpoch: typingEpoch,
      );
    }
    final startAnchor = _selectionStart!;
    final endAnchor = _selectionEnd!;
    final baseAnchor = _selectionBaseIsStart ? startAnchor : endAnchor;
    final extentAnchor = _selectionBaseIsStart ? endAnchor : startAnchor;
    final baseRevision = document.revision;
    final baseGeneration = _selectionGeneration;
    final logicalEditId = ++_nextLogicalEditId;
    final typing = coalesceTyping && compositionGroup == null
        ? _typingEvent(
            start: start,
            end: end,
            replacement: replacement,
            beforeSelection: beforeSelection,
            afterSelection: afterSelection,
            atMicros: typingAtMicros,
            epoch: typingEpoch,
          )
        : null;
    final previous = _undoUnits.isEmpty ? null : _undoUnits.last;
    final joinsComposition =
        compositionGroup != null &&
        previous != null &&
        previous.compositionGroup == compositionGroup &&
        previous.nativeHistoryStepCount < _maximumNativeCompositeHistorySteps;
    final joinsTyping =
        typing != null &&
        previous != null &&
        _typingCoalesces(previous, typing, beforeSelection);
    final historyGroupId = switch ((compositionGroup, typing)) {
      (final int group, _) => (group << 1) | 1,
      (_, != null) when joinsTyping => previous.nativeHistoryGroupId,
      (_, != null) => (++_nextNativeTypingHistoryGroup) << 1,
      _ => 0,
    };
    final requestDigest = _sourceTransactionDigest(
      logicalEditId: logicalEditId,
      revision: baseRevision,
      selectionGeneration: baseGeneration,
      start: start,
      end: end,
      replacement: replacement,
      resultBase: afterSelection.base,
      resultExtent: afterSelection.extent,
      historyGroupId: historyGroupId,
    );
    late final FlarkCoreSourceTransactionReceiptV1 transaction;
    try {
      transaction = await document.applySourceTransactionV1(
        expectedRevision: baseRevision,
        selectionBaseAnchor: baseAnchor,
        selectionExtentAnchor: extentAnchor,
        logicalEditId: logicalEditId,
        requestDigest: requestDigest,
        acknowledgePreviousLogicalEditId: _pendingTerminalLogicalEditId,
        selectionGeneration: baseGeneration,
        historyGroupId: historyGroupId,
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
        resultSelectionBaseUtf16: afterSelection.base,
        resultSelectionExtentUtf16: afterSelection.extent,
        selectionAffinityDownstream:
            afterSelection.affinity == FlarkCoreAffinity.downstream,
        selectionDirectional: afterSelection.base > afterSelection.extent,
      );
    } on FlarkCoreWorkerException {
      _postCommitUnknown = true;
      rethrow;
    }
    final adoptionWatch = Stopwatch()..start();
    if (transaction.logicalEditId != logicalEditId ||
        transaction.requestDigest != requestDigest ||
        transaction.baseRevision != baseRevision ||
        transaction.baseUtf16Start != start ||
        transaction.baseUtf16End != end ||
        transaction.resultSelectionBaseUtf16 != afterSelection.base ||
        transaction.resultSelectionExtentUtf16 != afterSelection.extent) {
      _postCommitUnknown = true;
      throw StateError('Flark source transaction receipt correlation failed');
    }
    if (transaction.historyCompositeExtended !=
        (joinsComposition || joinsTyping)) {
      _postCommitUnknown = true;
      throw StateError('Flark native history grouping diverged from Core');
    }
    _pendingTerminalLogicalEditId = logicalEditId;
    _selectionBaseIsStart = afterSelection.base <= afterSelection.extent;
    _selectionStart = _selectionBaseIsStart ? baseAnchor : extentAnchor;
    _selectionEnd = _selectionBaseIsStart ? extentAnchor : baseAnchor;
    _selectionBaseUtf16 = afterSelection.base;
    _selectionExtentUtf16 = afterSelection.extent;
    _selectionAffinity = afterSelection.affinity;
    _selectionAdapterState = afterSelection.adapterState;
    _selectionInlineContinuation = afterSelection.inlineContinuation;
    _selectionGeneration += 1;
    final receipt = FlarkCoreEditReceipt(
      revision: transaction.resultRevision,
      sourceByteLength: transaction.resultSourceByteLength,
      sourceUtf16Length: transaction.resultSourceUtf16Length,
      historyToken: transaction.historyToken,
      historyDisposition: FlarkCoreHistoryDisposition.retained,
    );
    try {
      await _recordForward(
        receipt,
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        typing: typing,
        compositionGroup: compositionGroup,
        nativeHistoryGroupId: historyGroupId,
        historyCompositeExtended: transaction.historyCompositeExtended,
      );
      if (compositionFinal) {
        await _releaseCompositionScope(compositionGroup!);
      }
    } on Object {
      _postCommitUnknown = true;
      rethrow;
    }
    adoptionWatch.stop();
    return receipt.withTelemetry(
      transaction.telemetry.withCoreStages(
        coreQueueMicros: coreQueueMicros,
        coreAdoptionMicros: adoptionWatch.elapsedMicroseconds,
      ),
    );
  }

  Future<FlarkCoreEditReceipt> _applyStagedSourceTransactionUtf16(
    int start,
    int end,
    String replacement, {
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    required bool coalesceTyping,
    required int? compositionGroup,
    required bool compositionFinal,
    required int typingAtMicros,
    required int typingEpoch,
    required int coreQueueMicros,
  }) async {
    final startAnchor = _selectionStart!;
    final endAnchor = _selectionEnd!;
    final baseAnchor = _selectionBaseIsStart ? startAnchor : endAnchor;
    final extentAnchor = _selectionBaseIsStart ? endAnchor : startAnchor;
    final baseRevision = document.revision;
    final baseGeneration = _selectionGeneration;
    final logicalEditId = ++_nextLogicalEditId;
    final typing = coalesceTyping && compositionGroup == null
        ? _typingEvent(
            start: start,
            end: end,
            replacement: replacement,
            beforeSelection: beforeSelection,
            afterSelection: afterSelection,
            atMicros: typingAtMicros,
            epoch: typingEpoch,
          )
        : null;
    final requestDigest = _sourceTransactionDigest(
      logicalEditId: logicalEditId,
      revision: baseRevision,
      selectionGeneration: baseGeneration,
      start: start,
      end: end,
      replacement: replacement,
      resultBase: afterSelection.base,
      resultExtent: afterSelection.extent,
      historyGroupId: 0,
    );
    late final FlarkCoreSourceTransactionReceiptV1 transaction;
    try {
      transaction = await document.applyStagedSourceTransactionV1(
        expectedRevision: baseRevision,
        selectionBaseAnchor: baseAnchor,
        selectionExtentAnchor: extentAnchor,
        logicalEditId: logicalEditId,
        requestDigest: requestDigest,
        acknowledgePreviousLogicalEditId: _pendingTerminalLogicalEditId,
        selectionGeneration: baseGeneration,
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
        resultSelectionUtf16: afterSelection.extent,
      );
    } on FlarkCoreWorkerException {
      _postCommitUnknown = true;
      rethrow;
    }
    final adoptionWatch = Stopwatch()..start();
    if (transaction.logicalEditId != logicalEditId ||
        transaction.requestDigest != requestDigest ||
        transaction.baseRevision != baseRevision ||
        transaction.baseUtf16Start != start ||
        transaction.baseUtf16End != end ||
        transaction.resultSelectionBaseUtf16 != afterSelection.base ||
        transaction.resultSelectionExtentUtf16 != afterSelection.extent ||
        transaction.historyCompositeExtended) {
      _postCommitUnknown = true;
      throw StateError('Flark staged transaction receipt correlation failed');
    }
    _pendingTerminalLogicalEditId = logicalEditId;
    _selectionBaseIsStart = true;
    _selectionStart = baseAnchor;
    _selectionEnd = extentAnchor;
    _selectionBaseUtf16 = afterSelection.base;
    _selectionExtentUtf16 = afterSelection.extent;
    _selectionAffinity = afterSelection.affinity;
    _selectionAdapterState = afterSelection.adapterState;
    _selectionInlineContinuation = afterSelection.inlineContinuation;
    _selectionGeneration += 1;
    final receipt = FlarkCoreEditReceipt(
      revision: transaction.resultRevision,
      sourceByteLength: transaction.resultSourceByteLength,
      sourceUtf16Length: transaction.resultSourceUtf16Length,
      historyToken: transaction.historyToken,
      historyDisposition: FlarkCoreHistoryDisposition.retained,
    );
    try {
      await _recordForward(
        receipt,
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        typing: typing,
        compositionGroup: compositionGroup,
        nativeHistoryGroupId: 0,
      );
      if (compositionFinal) {
        await _releaseCompositionScope(compositionGroup!);
      }
    } on Object {
      _postCommitUnknown = true;
      rethrow;
    }
    adoptionWatch.stop();
    return receipt.withTelemetry(
      transaction.telemetry.withCoreStages(
        coreQueueMicros: coreQueueMicros,
        coreAdoptionMicros: adoptionWatch.elapsedMicroseconds,
      ),
    );
  }

  /// Compatibility fallback for a rare oversized operation whose requested
  /// result is not the staged v1 collapsed caret at the inserted range end.
  Future<FlarkCoreEditReceipt> _applyLegacyBulkEditUtf16(
    int start,
    int end,
    String replacement, {
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    required bool coalesceTyping,
    required int? compositionGroup,
    required bool compositionFinal,
    required int typingAtMicros,
    required int typingEpoch,
  }) async {
    final typing = coalesceTyping && compositionGroup == null
        ? _typingEvent(
            start: start,
            end: end,
            replacement: replacement,
            beforeSelection: beforeSelection,
            afterSelection: afterSelection,
            atMicros: typingAtMicros,
            epoch: typingEpoch,
          )
        : null;
    late final FlarkCoreEditReceipt receipt;
    try {
      receipt = await document.applyEditUtf16(start, end, replacement);
    } on FlarkCoreWorkerException {
      _postCommitUnknown = true;
      rethrow;
    }
    _pendingTerminalLogicalEditId = 0;
    if (receipt.historyToken == null ||
        receipt.historyDisposition != FlarkCoreHistoryDisposition.retained) {
      _postCommitUnknown = true;
      throw StateError('Flark staged edit committed without required history');
    }
    try {
      await _recordForward(
        receipt,
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        typing: typing,
        compositionGroup: compositionGroup,
        nativeHistoryGroupId: 0,
      );
      await _setSelectionUtf16(
        afterSelection.base,
        afterSelection.extent,
        affinity: afterSelection.affinity,
        adapterState: afterSelection.adapterState,
        inlineContinuation: afterSelection.inlineContinuation,
      );
      if (compositionFinal) {
        await _releaseCompositionScope(compositionGroup!);
      }
    } on Object {
      _postCommitUnknown = true;
      rethrow;
    }
    return receipt;
  }

  /// Applies one collapsed-caret `flark-edit-v1` command. Rust resolves and
  /// commits the exact Markdown splice from the canonical native anchors;
  /// Core adopts only the returned receipt and records one standalone undo
  /// unit. No source or coordinate preflight crosses the worker boundary.
  ///
  /// A collapsed platform selection may report either visual affinity. Core's
  /// single collapsed source anchor is deliberately downstream regardless;
  /// [_selectionAffinity] remains adapter metadata for restoring the visual
  /// caret and is not a semantic-edit admission rule.
  Future<FlarkCoreEditIntentReceiptV1> applyEditIntentV1(
    FlarkCoreEditIntentV1 intent, {
    required bool compositionActive,
  }) async {
    final outcome = await applyEditIntentOutcomeV1(
      intent,
      compositionActive: compositionActive,
    );
    return outcome.receipt;
  }

  /// Applies one semantic command and returns the typed selection intent that
  /// was recorded atomically with its history result.
  Future<FlarkCoreEditIntentOutcomeV1> applyEditIntentOutcomeV1(
    FlarkCoreEditIntentV1 intent, {
    required bool compositionActive,
    FlarkCoreInlineContinuationContextV1? inlineContinuationContext,
  }) {
    if (intent == FlarkCoreEditIntentV1.toggleTaskChecked) {
      throw ArgumentError(
        'Use applySemanticActionV1 for selection-independent actions',
      );
    }
    final queuedAt = _clockMicros();
    return _serializeCommand(
      () => _applyEditIntentV1(
        intent,
        compositionActive: compositionActive,
        coreQueueMicros: _clockMicros() - queuedAt,
        inlineContinuationContext: inlineContinuationContext,
      ),
    );
  }

  /// Applies one parser-owned action at [targetUtf16] without moving or
  /// replacing the canonical selection. The temporary target anchor and the
  /// complete logical command live inside the same serialized Core gate.
  Future<FlarkCoreEditIntentReceiptV1> applySemanticActionV1(
    FlarkCoreSemanticActionV1 action, {
    required int targetUtf16,
    bool compositionActive = false,
  }) {
    final queuedAt = _clockMicros();
    return _serializeCommand(() async {
      _ensureAuthoritativeCommandsAvailable();
      final target = await document.createAnchorUtf16(
        targetUtf16,
        downstream: true,
      );
      try {
        final outcome = await _applyEditIntentV1(
          switch (action) {
            FlarkCoreSemanticActionV1.toggleTaskChecked =>
              FlarkCoreEditIntentV1.toggleTaskChecked,
          },
          compositionActive: compositionActive,
          coreQueueMicros: _clockMicros() - queuedAt,
          targetAnchor: target,
          targetUtf16: targetUtf16,
          preserveSelection: true,
        );
        return outcome.receipt;
      } finally {
        await document.releaseAnchor(target);
      }
    });
  }

  Future<FlarkCoreEditIntentOutcomeV1> _applyEditIntentV1(
    FlarkCoreEditIntentV1 intent, {
    required bool compositionActive,
    required int coreQueueMicros,
    FlarkCoreInlineContinuationContextV1? inlineContinuationContext,
    FlarkCoreAnchor? targetAnchor,
    int? targetUtf16,
    bool preserveSelection = false,
  }) async {
    _ensureAuthoritativeCommandsAvailable();
    final start = _selectionStart;
    final end = _selectionEnd;
    if (start == null || end == null) {
      throw StateError('Flark semantic edit requires a canonical selection');
    }
    if (!preserveSelection && _selectionBaseUtf16 != _selectionExtentUtf16) {
      throw StateError('flark-edit-v1 currently requires one collapsed caret');
    }
    if (preserveSelection != (targetAnchor != null && targetUtf16 != null)) {
      throw StateError('Flark semantic action target contract is incomplete');
    }
    final baseRevision = document.revision;
    final baseGeneration = _selectionGeneration;
    final logicalEditId = ++_nextLogicalEditId;
    final requestDigest = _editIntentDigest(
      logicalEditId,
      baseRevision,
      baseGeneration,
      intent.index,
      compositionActive,
      targetUtf16 ?? -1,
    );
    final before = FlarkCoreSelectionSnapshot(
      base: _selectionBaseUtf16,
      extent: _selectionExtentUtf16,
      affinity: _selectionAffinity,
      generation: baseGeneration,
      revision: baseRevision,
      adapterState: _selectionAdapterState,
      inlineContinuation: _selectionInlineContinuation,
    );
    late final FlarkCoreEditIntentReceiptV1 receipt;
    try {
      receipt = await document.applyEditIntentV1(
        intent: intent,
        expectedRevision: baseRevision,
        selectionBaseAnchor: start,
        selectionExtentAnchor: end,
        targetAnchor: targetAnchor,
        logicalEditId: logicalEditId,
        requestDigest: requestDigest,
        acknowledgePreviousLogicalEditId: _pendingTerminalLogicalEditId,
        selectionGeneration: baseGeneration,
        compositionActive: compositionActive,
      );
    } on FlarkCoreWorkerException {
      _postCommitUnknown = true;
      rethrow;
    }
    final adoptionWatch = Stopwatch()..start();
    if (receipt.logicalEditId != logicalEditId ||
        receipt.requestDigest != requestDigest ||
        receipt.baseRevision != baseRevision ||
        (preserveSelection &&
            receipt.hasCommit &&
            receipt.resultSelectionUtf16 !=
                (before.base <= before.extent ? before.base : before.extent))) {
      _postCommitUnknown = true;
      throw StateError('Flark semantic receipt correlation failed');
    }
    _pendingTerminalLogicalEditId = logicalEditId;
    if (!receipt.hasCommit) {
      adoptionWatch.stop();
      return FlarkCoreEditIntentOutcomeV1(
        receipt: receipt.withCoreTelemetry(
          coreQueueMicros: coreQueueMicros,
          coreAdoptionMicros: adoptionWatch.elapsedMicroseconds,
        ),
      );
    }
    final token = receipt.historyToken;
    if (token == null) {
      _postCommitUnknown = true;
      throw StateError('Flark semantic commit omitted required history');
    }
    await _breakActiveGroups();
    if (!preserveSelection) {
      _selectionBaseUtf16 = receipt.resultSelectionUtf16;
      _selectionExtentUtf16 = receipt.resultSelectionUtf16;
    }
    final inlineContinuation = preserveSelection
        ? _selectionInlineContinuation
        : inlineContinuationContext?.resolve(receipt);
    _selectionInlineContinuation = inlineContinuation;
    final afterGeneration = ++_selectionGeneration;
    final after = FlarkCoreSelectionSnapshot(
      base: _selectionBaseUtf16,
      extent: _selectionExtentUtf16,
      affinity: _selectionAffinity,
      generation: afterGeneration,
      revision: receipt.resultRevision,
      adapterState: _selectionAdapterState,
      inlineContinuation: inlineContinuation,
    );
    try {
      await _recordForward(
        FlarkCoreEditReceipt(
          revision: receipt.resultRevision,
          sourceByteLength: receipt.resultSourceByteLength,
          sourceUtf16Length: receipt.resultSourceUtf16Length,
          historyToken: token,
          historyDisposition: FlarkCoreHistoryDisposition.retained,
        ),
        beforeSelection: before,
        afterSelection: after,
        typing: null,
        compositionGroup: null,
        nativeHistoryGroupId: 0,
      );
    } on Object {
      _postCommitUnknown = true;
      rethrow;
    }
    adoptionWatch.stop();
    return FlarkCoreEditIntentOutcomeV1(
      receipt: receipt.withCoreTelemetry(
        coreQueueMicros: coreQueueMicros,
        coreAdoptionMicros: adoptionWatch.elapsedMicroseconds,
      ),
      inlineContinuation: inlineContinuation,
    );
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

  /// Releases the cancellation-only base anchors after a composition was
  /// committed without a final source mutation. Callers must enqueue this
  /// behind every source mutation already observed for that composition.
  Future<void> finishComposition() =>
      _serializeCommand(_releaseCompositionScope);

  Future<FlarkCoreHistoryOutcome?> undo() {
    final queuedAt = _clockMicros();
    return _serializeCommand(
      () => _replayDirection(
        undo: true,
        coreQueueMicros: _clockMicros() - queuedAt,
      ),
    );
  }

  Future<FlarkCoreHistoryOutcome?> redo() {
    final queuedAt = _clockMicros();
    return _serializeCommand(
      () => _replayDirection(
        undo: false,
        coreQueueMicros: _clockMicros() - queuedAt,
      ),
    );
  }

  /// Releases every retained unit in both directions.
  Future<void> clearHistory() => _serializeCommand(_clearHistory);

  Future<void> _clearHistory() async {
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
    FlarkCoreInlineContinuationV1? inlineContinuation,
  }) => _serializeCommand(
    () => _setSelectionUtf16(
      base,
      extent,
      affinity: affinity,
      adapterState: adapterState,
      inlineContinuation: inlineContinuation,
    ),
  );

  Future<int> _setSelectionUtf16(
    int base,
    int extent, {
    FlarkCoreAffinity affinity = FlarkCoreAffinity.downstream,
    Object? adapterState,
    FlarkCoreInlineContinuationV1? inlineContinuation,
  }) async {
    _ensureAuthoritativeCommandsAvailable();
    final start = base <= extent ? base : extent;
    final end = base <= extent ? extent : base;
    final collapsed = start == end;
    final nextStart = await document.createAnchorUtf16(start, downstream: true);
    late final FlarkCoreAnchor nextEnd;
    try {
      nextEnd = await document.createAnchorUtf16(end, downstream: collapsed);
    } catch (_) {
      await document.releaseAnchor(nextStart);
      rethrow;
    }
    await _releaseSelectionAnchors();
    _selectionStart = nextStart;
    _selectionEnd = nextEnd;
    _selectionBaseIsStart = base <= extent;
    _selectionAffinity = affinity;
    _selectionAdapterState = adapterState;
    _selectionInlineContinuation = inlineContinuation;
    _selectionBaseUtf16 = base;
    _selectionExtentUtf16 = extent;
    return ++_selectionGeneration;
  }

  /// Resolves the canonical selection at the current revision, or null when
  /// no canonical selection is installed.
  Future<FlarkCoreSelectionSnapshot?> resolveSelection() =>
      _serializeCommand(_resolveSelection);

  Future<FlarkCoreSelectionSnapshot?> _resolveSelection() async {
    final startAnchor = _selectionStart;
    final endAnchor = _selectionEnd;
    if (startAnchor == null || endAnchor == null) return null;
    final generation = _selectionGeneration;
    final start = await document.resolveAnchorUtf16(startAnchor);
    final end = identical(endAnchor, startAnchor)
        ? start
        : await document.resolveAnchorUtf16(endAnchor);
    if (generation != _selectionGeneration) return null;
    _selectionBaseUtf16 = _selectionBaseIsStart ? start : end;
    _selectionExtentUtf16 = _selectionBaseIsStart ? end : start;
    return FlarkCoreSelectionSnapshot(
      base: _selectionBaseUtf16,
      extent: _selectionExtentUtf16,
      affinity: _selectionAffinity,
      generation: generation,
      revision: document.revision,
      adapterState: _selectionAdapterState,
      inlineContinuation: _selectionInlineContinuation,
    );
  }

  Future<void> clearSelection() => _serializeCommand(_clearSelection);

  Future<void> _clearSelection() async {
    await _releaseSelectionAnchors();
    _selectionAdapterState = null;
    _selectionInlineContinuation = null;
    _selectionGeneration += 1;
  }

  /// Releases history and selection authority. The document itself is owned
  /// by the caller and stays open.
  Future<void> dispose() => _serializeCommand(_dispose);

  Future<void> _dispose() async {
    await _clearHistory();
    await _releaseCompositionScope();
    await _releaseSelectionAnchors();
  }

  Future<void> _pinCompositionBase(
    int group,
    FlarkCoreSelectionSnapshot baseSelection,
  ) async {
    final existing = _compositionScope;
    if (existing != null) {
      if (existing.group != group) {
        throw StateError(
          'Flark composition $group overlapped composition ${existing.group}',
        );
      }
      return;
    }

    // Outward affinity preserves both original range edges through a
    // replacement followed by its native inverse. At a collapsed caret the
    // pair deliberately brackets the composing insertion and converges again
    // on cancellation.
    final baseAnchor = await document.createAnchorUtf16(
      baseSelection.base,
      downstream: baseSelection.base > baseSelection.extent,
    );
    late final FlarkCoreAnchor extentAnchor;
    try {
      extentAnchor = await document.createAnchorUtf16(
        baseSelection.extent,
        downstream: baseSelection.extent >= baseSelection.base,
      );
    } catch (_) {
      await _releaseAnchorPair(baseAnchor, null);
      rethrow;
    }
    _compositionScope = _CompositionScope(
      group: group,
      baseAnchor: baseAnchor,
      extentAnchor: extentAnchor,
      baseSelection: baseSelection,
    );
  }

  Future<void> _adoptCompositionBase(_CompositionScope scope) async {
    final base = await document.resolveAnchorUtf16(scope.baseAnchor);
    final extent = await document.resolveAnchorUtf16(scope.extentAnchor);
    if (base != scope.baseSelection.base ||
        extent != scope.baseSelection.extent) {
      _postCommitUnknown = true;
      throw StateError(
        'Flark composition base anchors did not round-trip through replay',
      );
    }

    final oldStart = _selectionStart;
    final oldEnd = _selectionEnd;
    final baseIsStart = base <= extent;
    _selectionStart = baseIsStart ? scope.baseAnchor : scope.extentAnchor;
    _selectionEnd = baseIsStart ? scope.extentAnchor : scope.baseAnchor;
    _selectionBaseIsStart = baseIsStart;
    _compositionScope = null;
    _adoptSelectionMetadata(scope.baseSelection);
    await _releaseAnchorPair(oldStart, oldEnd);
  }

  void _adoptSelectionMetadata(FlarkCoreSelectionSnapshot selection) {
    _selectionBaseIsStart = selection.base <= selection.extent;
    _selectionBaseUtf16 = selection.base;
    _selectionExtentUtf16 = selection.extent;
    _selectionAffinity = selection.affinity;
    _selectionAdapterState = selection.adapterState;
    _selectionInlineContinuation = selection.inlineContinuation;
    _selectionGeneration += 1;
  }

  Future<void> _releaseCompositionScope([int? expectedGroup]) async {
    final scope = _compositionScope;
    if (scope == null ||
        (expectedGroup != null && scope.group != expectedGroup)) {
      return;
    }
    _compositionScope = null;
    await _releaseAnchorPair(scope.baseAnchor, scope.extentAnchor);
  }

  Future<void> _releaseSelectionAnchors() async {
    final start = _selectionStart;
    final end = _selectionEnd;
    _selectionStart = null;
    _selectionEnd = null;
    await _releaseAnchorPair(start, end);
  }

  Future<void> _releaseAnchorPair(
    FlarkCoreAnchor? start,
    FlarkCoreAnchor? end,
  ) async {
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

  ({int end, int atMicros, int epoch})? _typingEvent({
    required int start,
    required int end,
    required String replacement,
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    required int atMicros,
    required int epoch,
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
    return (end: start + replacement.length, atMicros: atMicros, epoch: epoch);
  }

  int? _compositionGroupForMutation(bool composingActive) {
    final wasActive = _activeCompositionGroup != 0;
    if (!wasActive && !composingActive) return null;
    final group = wasActive ? _activeCompositionGroup : ++_nextCompositionGroup;
    _activeCompositionGroup = composingActive ? group : 0;
    return group;
  }

  Future<void> _recordForward(
    FlarkCoreEditReceipt receipt, {
    required FlarkCoreSelectionSnapshot beforeSelection,
    required FlarkCoreSelectionSnapshot afterSelection,
    required ({int end, int atMicros, int epoch})? typing,
    required int? compositionGroup,
    required int nativeHistoryGroupId,
    bool historyCompositeExtended = false,
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
        previous.compositionGroup == compositionGroup &&
        previous.nativeHistoryStepCount < _maximumNativeCompositeHistorySteps) {
      if (historyCompositeExtended &&
          previous.nativeHistoryGroupId != nativeHistoryGroupId) {
        throw StateError('Flark composite history group identity diverged');
      }
      _undoUnits[_undoUnits.length - 1] = _HistoryUnit(
        tokens: historyCompositeExtended
            ? previous.tokens
            : List.unmodifiable([...previous.tokens, token]),
        beforeSelection: previous.beforeSelection,
        afterSelection: afterSelection,
        compositionGroup: compositionGroup,
        nativeHistoryGroupId: nativeHistoryGroupId,
        nativeHistoryStepCount: previous.nativeHistoryStepCount + 1,
      );
      return;
    }
    if (typing != null &&
        previous != null &&
        _typingCoalesces(previous, typing, beforeSelection)) {
      if (historyCompositeExtended &&
          previous.nativeHistoryGroupId != nativeHistoryGroupId) {
        throw StateError('Flark composite typing identity diverged');
      }
      _undoUnits[_undoUnits.length - 1] = _HistoryUnit(
        tokens: historyCompositeExtended
            ? previous.tokens
            : List.unmodifiable([...previous.tokens, token]),
        beforeSelection: previous.beforeSelection,
        afterSelection: afterSelection,
        typingEnd: typing.end,
        typingAtMicros: typing.atMicros,
        typingEpoch: typing.epoch,
        nativeHistoryGroupId: nativeHistoryGroupId,
        nativeHistoryStepCount: previous.nativeHistoryStepCount + 1,
      );
      return;
    }
    if (historyCompositeExtended) {
      throw StateError('Flark extended history without a matching Core unit');
    }
    _undoUnits.add(
      _HistoryUnit(
        tokens: [token],
        beforeSelection: beforeSelection,
        afterSelection: afterSelection,
        typingEnd: typing?.end,
        typingAtMicros: typing?.atMicros,
        typingEpoch: typing?.epoch,
        compositionGroup: compositionGroup,
        nativeHistoryGroupId: nativeHistoryGroupId,
      ),
    );
  }

  bool _typingCoalesces(
    _HistoryUnit previous,
    ({int end, int atMicros, int epoch}) typing,
    FlarkCoreSelectionSnapshot beforeSelection,
  ) =>
      previous.typingEnd == beforeSelection.extent &&
      previous.nativeHistoryStepCount < _maximumNativeCompositeHistorySteps &&
      previous.typingAtMicros != null &&
      typing.atMicros - previous.typingAtMicros! <= _typingIdleMicros &&
      previous.typingEpoch == typing.epoch;

  Future<FlarkCoreHistoryOutcome?> _replayDirection({
    required bool undo,
    required int coreQueueMicros,
  }) async {
    _ensureAuthoritativeCommandsAvailable();
    final source = undo ? _undoUnits : _redoUnits;
    final destination = undo ? _redoUnits : _undoUnits;
    if (source.isEmpty) return null;
    await _breakActiveGroups();
    final unit = source.removeLast();
    final replayed = await _replayUnit(unit);
    if (replayed == null) {
      final stale = [...source, ...destination];
      source.clear();
      destination.clear();
      await _releaseUnits(stale);
      // Source is unchanged (or rolled back exactly), so the selection to
      // restore is the unit's current-state side, not its replayed side.
      final restore = _selectionAtCurrentRevision(
        undo ? unit.afterSelection : unit.beforeSelection,
      );
      await _setSelectionUtf16(
        restore.base,
        restore.extent,
        affinity: restore.affinity,
        adapterState: restore.adapterState,
        inlineContinuation: restore.inlineContinuation,
      );
      return FlarkCoreHistoryDropped(restore);
    }
    final adoptionWatch = Stopwatch()..start();
    destination.add(replayed.unit);
    final restore = _selectionAtCurrentRevision(
      undo ? unit.beforeSelection : unit.afterSelection,
    );
    await _setSelectionUtf16(
      restore.base,
      restore.extent,
      affinity: restore.affinity,
      adapterState: restore.adapterState,
      inlineContinuation: restore.inlineContinuation,
    );
    adoptionWatch.stop();
    final telemetry = replayed.receipt.telemetry;
    if (telemetry == null) {
      throw StateError('Flark history replay omitted performance telemetry');
    }
    return FlarkCoreHistoryReplayed(
      restore,
      replayed.receipt.withTelemetry(
        telemetry.withCoreStages(
          coreQueueMicros: coreQueueMicros,
          coreAdoptionMicros: adoptionWatch.elapsedMicroseconds,
        ),
      ),
    );
  }

  FlarkCoreSelectionSnapshot _selectionAtCurrentRevision(
    FlarkCoreSelectionSnapshot selection,
  ) {
    final continuation = selection.inlineContinuation;
    return FlarkCoreSelectionSnapshot(
      base: selection.base,
      extent: selection.extent,
      affinity: selection.affinity,
      generation: selection.generation,
      revision: document.revision,
      adapterState: selection.adapterState,
      inlineContinuation: continuation == null || !selection.isCollapsed
          ? null
          : continuation.atRevision(document.revision, selection.extent),
    );
  }

  Future<void> _breakActiveGroups() async {
    _typingEpoch += 1;
    _activeCompositionGroup = 0;
    await _releaseCompositionScope();
  }

  Future<({_HistoryUnit unit, FlarkCoreEditReceipt receipt})?> _replayUnit(
    _HistoryUnit unit,
  ) async {
    final reverseTokens = <FlarkCoreHistoryToken>[];
    late FlarkCoreEditReceipt receipt;
    var workerRoundTripMicros = 0;
    var workerQueueMicros = 0;
    var nativeFfiMicros = 0;
    for (final token in unit.tokens.reversed) {
      try {
        receipt = await document.replayHistory(token);
        final telemetry = receipt.telemetry;
        if (telemetry == null) {
          throw StateError('Flark history replay omitted worker telemetry');
        }
        workerRoundTripMicros += telemetry.workerRoundTripMicros;
        workerQueueMicros += telemetry.workerQueueMicros;
        nativeFfiMicros += telemetry.nativeFfiMicros;
        _pendingTerminalLogicalEditId = 0;
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
      receipt: receipt.withTelemetry(
        FlarkCoreEditIntentTelemetryV1(
          coreQueueMicros: 0,
          workerRoundTripMicros: workerRoundTripMicros,
          workerQueueMicros: workerQueueMicros,
          nativeFfiMicros: nativeFfiMicros,
          coreAdoptionMicros: 0,
        ),
      ),
    );
  }

  Future<void> _rollback(List<FlarkCoreHistoryToken> reverseTokens) async {
    for (final token in reverseTokens.reversed) {
      final receipt = await document.replayHistory(token);
      _pendingTerminalLogicalEditId = 0;
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

  Future<T> _serializeCommand<T>(Future<T> Function() command) {
    final hasPredecessor = _pendingCommandCount != 0;
    final prior = _commandTail;
    _pendingCommandCount += 1;
    Future<T> run() async {
      try {
        if (hasPredecessor) await prior;
        return await command();
      } finally {
        _pendingCommandCount -= 1;
      }
    }

    // Start the async gate immediately in the caller's active zone. Adding an
    // idle microtask here can strand a command queued from Flutter's fake
    // async callback zone when its completion is awaited from a real-async
    // barrier before another frame pump. A queued command still suspends on
    // its captured predecessor, preserving strict ordering.
    final result = run();
    _commandTail = result.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {},
    );
    return result;
  }

  void _ensureAuthoritativeCommandsAvailable() {
    if (_postCommitUnknown) {
      throw StateError(
        'Flark editor session is fail-stopped after an uncertain native commit',
      );
    }
  }

  static int _editIntentDigest(
    int logicalEditId,
    int revision,
    int selectionGeneration,
    int intent,
    bool compositionActive,
    int targetUtf16,
  ) {
    var hash = 0x6a09e667f3bcc909;
    for (final value in [
      logicalEditId,
      revision,
      selectionGeneration,
      intent,
      compositionActive ? 1 : 0,
      targetUtf16,
    ]) {
      hash = ((hash ^ value) * 0x100000001b3) & 0x7fffffffffffffff;
    }
    return hash == 0 ? 1 : hash;
  }

  static int _sourceTransactionDigest({
    required int logicalEditId,
    required int revision,
    required int selectionGeneration,
    required int start,
    required int end,
    required String replacement,
    required int resultBase,
    required int resultExtent,
    required int historyGroupId,
  }) {
    var hash = 0x510e527fade682d1;
    for (final value in [
      logicalEditId,
      revision,
      selectionGeneration,
      start,
      end,
      resultBase,
      resultExtent,
      historyGroupId,
      replacement.length,
      ...replacement.codeUnits,
    ]) {
      hash = ((hash ^ value) * 0x100000001b3) & 0x7fffffffffffffff;
    }
    return hash == 0 ? 1 : hash;
  }
}

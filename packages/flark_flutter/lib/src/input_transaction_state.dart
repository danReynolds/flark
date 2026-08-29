import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

import 'editor_transactions.dart';
import 'input_reconciliation.dart';

/// Sole owner of Flutter input work that can outlive one platform callback.
///
/// This state machine knows callback lineage, paired platform actions,
/// composition input, and provisional-to-committed reconciliation bookkeeping.
/// It deliberately does not decide Markdown meaning, edit eligibility, source
/// mutations, or rendered presentation; those remain controller/Core work.
final class FlarkInputTransactionState {
  bool _platformMutationActive = false;
  FlarkPendingPlatformLineage? _lineage;
  int? _activeCallbackStartedEpochMicros;
  FlarkPlatformInputTiming? _activeTiming;
  bool _newlineTextObservationAwaitingAction = false;
  bool _backspaceTextObservationAwaitingSelector = false;
  FlarkCompositionInputBase? _compositionInputBase;
  int _successorHighWatermark = 0;
  int _lastReconciliationMicros = 0;

  bool get platformMutationActive => _platformMutationActive;
  int? get activeCallbackStartedEpochMicros =>
      _activeCallbackStartedEpochMicros;
  FlarkPlatformInputTiming? get activeTiming => _activeTiming;
  FlarkCompositionInputBase? get compositionInputBase => _compositionInputBase;
  int get successorHighWatermark => _successorHighWatermark;
  int get lastReconciliationMicros => _lastReconciliationMicros;

  FlarkPendingSemanticInput? get pendingSemantic =>
      _lineage is FlarkPendingSemanticInput
      ? _lineage as FlarkPendingSemanticInput
      : null;

  set pendingSemantic(FlarkPendingSemanticInput? value) {
    if (value != null) {
      if (_lineage != null && !identical(_lineage, value)) {
        throw StateError('Input lineage must be cleared before replacement');
      }
      _lineage = value;
    } else if (_lineage is FlarkPendingSemanticInput) {
      _lineage = null;
    }
  }

  FlarkLateSemanticInput? get lateSemantic => _lineage is FlarkLateSemanticInput
      ? _lineage as FlarkLateSemanticInput
      : null;

  set lateSemantic(FlarkLateSemanticInput? value) {
    if (value != null) {
      if (_lineage != null && !identical(_lineage, value)) {
        throw StateError('Input lineage must be cleared before replacement');
      }
      _lineage = value;
    } else if (_lineage is FlarkLateSemanticInput) {
      _lineage = null;
    }
  }

  /// Opens one platform callback scope. Nested callbacks would make timing
  /// and provisional lineage ambiguous, so they fail immediately.
  FlarkPlatformInputTiming beginCallback() {
    if (_activeTiming != null) {
      throw StateError('Platform input callbacks cannot nest');
    }
    final timing = FlarkPlatformInputTiming();
    _activeCallbackStartedEpochMicros = timing.acceptedAtEpochMicros;
    _activeTiming = timing;
    return timing;
  }

  /// Closes exactly the scope returned by [beginCallback]. If that callback
  /// admitted a semantic transaction, records its synchronous callback cost
  /// on the same lineage before releasing the scope.
  void finishCallback(FlarkPlatformInputTiming timing) {
    if (!identical(_activeTiming, timing)) {
      throw StateError('Platform input callback scope mismatch');
    }
    timing.complete();
    final pending = pendingSemantic;
    if (pending?.initialCallbackStartedEpochMicros ==
        timing.acceptedAtEpochMicros) {
      pending!.initialCallbackMicros = timing.editorSyncMicros;
    }
    _activeCallbackStartedEpochMicros = null;
    _activeTiming = null;
  }

  void beginPlatformMutation() {
    if (_platformMutationActive) {
      throw StateError('Platform mutation scopes cannot nest');
    }
    _platformMutationActive = true;
  }

  void endPlatformMutation() {
    if (!_platformMutationActive) {
      throw StateError('No platform mutation scope is active');
    }
    _platformMutationActive = false;
  }

  void markNewlineTextObserved() {
    _newlineTextObservationAwaitingAction = true;
  }

  void clearNewlineTextObservation() {
    _newlineTextObservationAwaitingAction = false;
  }

  /// Consumes the action paired with a newline already reported as text.
  /// Returns true when the caller must not execute another newline command.
  bool consumeNewlineAction({required bool textObservationAlreadyApplied}) {
    final consumed =
        textObservationAlreadyApplied || _newlineTextObservationAwaitingAction;
    _newlineTextObservationAwaitingAction = false;
    return consumed;
  }

  void markBackspaceTextObserved() {
    _backspaceTextObservationAwaitingSelector = true;
  }

  void clearBackspaceTextObservation() {
    _backspaceTextObservationAwaitingSelector = false;
  }

  /// Consumes the selector paired with a Backspace already reported as text.
  /// Returns true when the caller must not execute another delete command.
  bool consumeBackspaceSelector({required bool textObservationAlreadyApplied}) {
    final consumed =
        textObservationAlreadyApplied ||
        _backspaceTextObservationAwaitingSelector;
    _backspaceTextObservationAwaitingSelector = false;
    return consumed;
  }

  void markObservedCommand(FlarkDeferredInputCommand? command) {
    switch (command) {
      case FlarkDeferredInputCommand.insertNewline:
        markNewlineTextObserved();
        return;
      case FlarkDeferredInputCommand.deleteBackward:
        markBackspaceTextObserved();
        return;
      case FlarkDeferredInputCommand.deleteForward:
      case null:
        return;
    }
  }

  void rememberCompositionInputBase({
    required int windowStart,
    required TextEditingValue value,
  }) {
    _compositionInputBase ??= FlarkCompositionInputBase(
      windowStart: windowStart,
      value: value.copyWith(composing: TextRange.empty),
    );
  }

  void clearCompositionInputBase() {
    _compositionInputBase = null;
  }

  void observeSuccessorCount(int count) {
    if (count > _successorHighWatermark) _successorHighWatermark = count;
  }

  /// Classifies one platform value change into a logical command or literal
  /// replacement that can be replayed after its semantic predecessor.
  /// Markdown meaning is deliberately absent: this is only Flutter text-value
  /// lineage and bounded grapheme geometry.
  FlarkDeferredInputSuccessor? classifySemanticSuccessor(
    TextEditingValue before,
    TextEditingValue after, {
    required FlarkTextMutation? mutation,
  }) {
    if (before.composing != TextRange.empty ||
        after.composing != TextRange.empty ||
        !before.selection.isValid ||
        !after.selection.isValid ||
        !after.selection.isCollapsed ||
        mutation == null) {
      return null;
    }
    if (mutation.start < 0 ||
        mutation.end < mutation.start ||
        mutation.end > before.text.length ||
        before.text.replaceRange(
              mutation.start,
              mutation.end,
              mutation.replacement,
            ) !=
            after.text) {
      return null;
    }
    final selectionStart = math.min(
      before.selection.baseOffset,
      before.selection.extentOffset,
    );
    final selectionEnd = math.max(
      before.selection.baseOffset,
      before.selection.extentOffset,
    );
    final resultCaret = mutation.start + mutation.replacement.length;
    if (after.selection.extentOffset != resultCaret) return null;

    if (mutation.start == selectionStart && mutation.end == selectionEnd) {
      if (mutation.replacement == '\n') {
        return FlarkDeferredInputSuccessor(
          FlarkDeferredInputCommand.insertNewline,
          platformTiming: _activeTiming,
        );
      }
      if ((mutation.replacement.isNotEmpty || !before.selection.isCollapsed) &&
          !mutation.replacement.contains('\n') &&
          !mutation.replacement.contains('\r')) {
        return FlarkDeferredInputSuccessor(
          null,
          replacement: mutation.replacement,
          typingInput:
              before.selection.isCollapsed &&
              mutation.start == mutation.end &&
              mutation.replacement.isNotEmpty,
          platformTiming: _activeTiming,
        );
      }
    }
    if (!before.selection.isCollapsed || mutation.replacement.isNotEmpty) {
      return null;
    }
    final caret = before.selection.extentOffset;
    final previous = FlarkCoreGraphemePolicy.previousClusterRange(
      before.text,
      caret,
    );
    if (previous != null &&
        mutation.start == previous.$1 &&
        mutation.end == previous.$2) {
      return FlarkDeferredInputSuccessor(
        FlarkDeferredInputCommand.deleteBackward,
        platformTiming: _activeTiming,
      );
    }
    final next = FlarkCoreGraphemePolicy.nextClusterRange(before.text, caret);
    if (next != null && mutation.start == next.$1 && mutation.end == next.$2) {
      return FlarkDeferredInputSuccessor(
        FlarkDeferredInputCommand.deleteForward,
        platformTiming: _activeTiming,
      );
    }
    return null;
  }

  FlarkDeferredInputSuccessor reclassifyAfterCertification(
    FlarkDeferredInputSuccessor successor,
  ) {
    if (successor.command == null) return successor;
    // A command observed against provisional geometry cannot safely target a
    // receipt-backed row: hidden prefixes or padding may move its canonical
    // caret. Preserve the logical command but require certified re-routing.
    return FlarkDeferredInputSuccessor(
      successor.command,
      replacement: successor.replacement,
      typingInput: successor.typingInput,
      semanticAlreadyAttempted: successor.semanticAlreadyAttempted,
      reclassifyAfterCertification: true,
      platformTiming: successor.platformTiming,
    );
  }

  /// Reserves one bounded successor slot. Overflow atomically retires the
  /// lineage and completes every waiter before the caller resynchronizes.
  bool reserveSemanticSuccessor(
    FlarkPendingSemanticInput pending, {
    required int maximum,
  }) {
    if (!identical(pendingSemantic, pending)) {
      throw StateError('Successor reservation requires the owned lineage');
    }
    if (pending.successors.length < maximum) return true;
    discardPendingSemantic();
    return false;
  }

  void observePendingSuccessors(FlarkPendingSemanticInput pending) {
    if (!identical(pendingSemantic, pending)) {
      throw StateError('Successor metrics require the owned lineage');
    }
    observeSuccessorCount(pending.successors.length);
  }

  FlarkPendingSemanticInput? takePendingSemantic() {
    final pending = pendingSemantic;
    pendingSemantic = null;
    return pending;
  }

  void completeDeferredHistorySuccessors(
    Iterable<FlarkSemanticInputSuccessor> successors,
    bool result,
  ) {
    for (final successor
        in successors.whereType<FlarkDeferredHistorySuccessor>()) {
      if (!successor.completion.isCompleted) {
        successor.completion.complete(result);
      }
    }
  }

  void discardPendingSemantic() {
    final pending = takePendingSemantic();
    if (pending == null) return;
    final promotion = pending.certificationPromotion;
    pending.certificationPromotion = null;
    if (promotion != null && !promotion.isCompleted) promotion.complete();
    completeDeferredHistorySuccessors(pending.successors, false);
  }

  void recordReconciliationMicros(int elapsedMicros) {
    if (elapsedMicros < 0) {
      throw ArgumentError.value(
        elapsedMicros,
        'elapsedMicros',
        'must not be negative',
      );
    }
    _lastReconciliationMicros = elapsedMicros;
  }
}

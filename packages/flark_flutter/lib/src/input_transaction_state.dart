import 'dart:async';
import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

import 'editor_transactions.dart';
import 'input_reconciliation.dart';
import 'input_window.dart';
import 'platform_input_bridge.dart';

/// Typed effect the Flutter facade must perform after the input transaction
/// state machine captures one platform observation.
sealed class FlarkInputCaptureOutcome {
  const FlarkInputCaptureOutcome();
}

final class FlarkInputCaptureShadow extends FlarkInputCaptureOutcome {
  const FlarkInputCaptureShadow({
    required this.value,
    required this.globalUtf16Start,
    this.latePromotion,
  });

  final TextEditingValue value;
  final int globalUtf16Start;
  final FlarkLateInputPromotion? latePromotion;
}

final class FlarkInputCaptureResync extends FlarkInputCaptureOutcome {
  const FlarkInputCaptureResync(this.reason);

  final FlarkInputResyncReason reason;
}

/// One already-captured late successor that must be promoted against the
/// committed semantic predecessor. The transaction state owns its lineage;
/// the controller owns only execution of these typed effects.
final class FlarkLateInputPromotion {
  const FlarkLateInputPromotion({
    required this.pending,
    required this.reconciliation,
  });

  final FlarkPendingSemanticInput pending;
  final FlarkInputReconciliationMap reconciliation;
}

sealed class FlarkInputDeferralOutcome {
  const FlarkInputDeferralOutcome();
}

final class FlarkInputDeferralIgnored extends FlarkInputDeferralOutcome {
  const FlarkInputDeferralIgnored();
}

final class FlarkInputDeferralStored extends FlarkInputDeferralOutcome {
  const FlarkInputDeferralStored();
}

final class FlarkInputDeferralResync extends FlarkInputDeferralOutcome {
  const FlarkInputDeferralResync(this.reason);

  final FlarkInputResyncReason reason;
}

final class FlarkInputDeferralPromote extends FlarkInputDeferralOutcome {
  const FlarkInputDeferralPromote({
    required this.command,
    required this.platformTiming,
  });

  final FlarkDeferredInputCommand command;
  final FlarkPlatformInputTiming? platformTiming;
}

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

  void discardLateSemantic() {
    lateSemantic = null;
  }

  FlarkPendingSemanticInput beginSemanticInput({
    required TextEditingValue base,
    required int inputGlobalUtf16Start,
    required TextEditingValue provisionalAfter,
    FlarkPlatformInputTiming? platformTiming,
    FlarkTextMutation? provisionalMutation,
  }) {
    lateSemantic = null;
    final timing = platformTiming ?? activeTiming;
    final pending = FlarkPendingSemanticInput(
      base: base,
      inputGlobalUtf16Start: inputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          timing?.acceptedAtEpochMicros ??
          activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: timing,
      provisionalMutation: provisionalMutation,
      provisionalAfter: provisionalAfter,
    );
    pendingSemantic = pending;
    return pending;
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

  /// Captures one observation behind the currently pending semantic command.
  ///
  /// Lineage mutation, command pairing, bounded successor admission, and
  /// provisional-tail advancement are atomic here. The caller only installs
  /// the returned platform shadow or performs the requested resynchronization.
  FlarkInputCaptureOutcome capturePendingObservation({
    required FlarkPlatformInputObservation observation,
    required bool observedValueValid,
    required FlarkTextMutation? fallbackMutation,
    required int maximumSuccessors,
  }) {
    final pending = pendingSemantic;
    if (pending == null) {
      throw StateError('Pending capture requires a semantic lineage');
    }
    if (!reserveSemanticSuccessor(pending, maximum: maximumSuccessors)) {
      return const FlarkInputCaptureResync(
        FlarkInputResyncReason.successorQueueOverflow,
      );
    }
    if (!observation.accepted || !observedValueValid) {
      discardPendingSemantic();
      return FlarkInputCaptureResync(
        observation.accepted
            ? FlarkInputResyncReason.unsupportedSuccessorObservation
            : observation.rejection,
      );
    }

    final logical = classifySemanticSuccessor(
      observation.before,
      observation.after,
      mutation: observation.observedMutation ?? fallbackMutation,
    );
    if (logical != null) {
      pending.successors.add(reclassifyAfterCertification(logical));
      markObservedCommand(logical.command);
    } else {
      if (pending.successors.isNotEmpty &&
          pending.successors.last is FlarkDeferredInputSuccessor) {
        discardPendingSemantic();
        return const FlarkInputCaptureResync(
          FlarkInputResyncReason.unsupportedSuccessorObservation,
        );
      }
      pending.successors.add(
        FlarkProvisionalInputBatch(
          before: observation.before,
          after: observation.after,
          typingInput: observation.typingInput,
          platformTiming: activeTiming,
        ),
      );
    }
    pending.provisionalTail = observation.after;
    observePendingSuccessors(pending);
    return FlarkInputCaptureShadow(
      value: observation.after,
      globalUtf16Start: pending.inputGlobalUtf16Start,
    );
  }

  /// Retires late lineage once the platform has adopted the committed input.
  bool retainLateLineage({
    required bool shadowMatchesCurrentInput,
    required TextEditingValue currentInput,
  }) {
    final late = lateSemantic;
    if (late == null) return false;
    final provisional = late.provisionalTail;
    if (shadowMatchesCurrentInput ||
        (provisional.text == currentInput.text &&
            provisional.selection == currentInput.selection &&
            provisional.composing == currentInput.composing)) {
      lateSemantic = null;
      return false;
    }
    return true;
  }

  /// Captures one callback that raced a semantic receipt publication.
  FlarkInputCaptureOutcome captureLateObservation({
    required FlarkPlatformInputObservation observation,
    required FlarkTextMutation? fallbackMutation,
    required int currentInputGlobalUtf16Start,
    required int shadowGlobalUtf16Start,
    required int maximumSuccessors,
  }) {
    final late = lateSemantic;
    if (late == null) {
      throw StateError('Late capture requires a retained semantic lineage');
    }
    if (late.successorCount >= maximumSuccessors) {
      lateSemantic = null;
      return const FlarkInputCaptureResync(
        FlarkInputResyncReason.successorQueueOverflow,
      );
    }

    final logical = classifySemanticSuccessor(
      observation.before,
      observation.after,
      mutation: observation.observedMutation ?? fallbackMutation,
    );
    final holder = FlarkPendingSemanticInput(
      base: observation.before,
      inputGlobalUtf16Start: currentInputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: activeTiming,
      provisionalAfter: observation.before,
    );
    if (logical != null) {
      holder.successors.add(logical);
      markObservedCommand(logical.command);
    } else {
      holder.successors.add(
        FlarkProvisionalInputBatch(
          before: observation.before,
          after: observation.after,
          typingInput: observation.typingInput,
          platformTiming: activeTiming,
        ),
      );
    }
    late.provisionalTail = observation.after;
    late.successorCount += 1;
    observeSuccessorCount(late.successorCount);
    return FlarkInputCaptureShadow(
      value: observation.after,
      globalUtf16Start: shadowGlobalUtf16Start,
      latePromotion: FlarkLateInputPromotion(
        pending: holder,
        reconciliation: late.reconciliation,
      ),
    );
  }

  /// Captures a complete logical command reported against a platform shadow
  /// while the current source publication is waiting for certification.
  FlarkInputCaptureOutcome captureCertificationDeferredObservation({
    required FlarkPlatformInputObservation observation,
    required bool observedValueValid,
    required FlarkTextMutation? fallbackMutation,
    required TextEditingValue currentInput,
    required int shadowGlobalUtf16Start,
  }) {
    if (!observation.accepted || !observedValueValid) {
      return FlarkInputCaptureResync(
        observation.accepted
            ? FlarkInputResyncReason.unsupportedSuccessorObservation
            : observation.rejection,
      );
    }
    final logical = classifySemanticSuccessor(
      observation.before,
      observation.after,
      mutation: observation.observedMutation ?? fallbackMutation,
    );
    if (logical == null) {
      return const FlarkInputCaptureResync(
        FlarkInputResyncReason.unsupportedSuccessorObservation,
      );
    }
    final timing = activeTiming;
    final pending = FlarkPendingSemanticInput(
      base: currentInput,
      inputGlobalUtf16Start: shadowGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          timing?.acceptedAtEpochMicros ??
          activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: timing,
      provisionalAfter: observation.after,
    );
    pending.successors.add(reclassifyAfterCertification(logical));
    pending.certificationPromotion = Completer<void>();
    pendingSemantic = pending;
    markObservedCommand(logical.command);
    observePendingSuccessors(pending);
    return FlarkInputCaptureShadow(
      value: observation.after,
      globalUtf16Start: shadowGlobalUtf16Start,
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

  bool appendPendingSuccessor(
    FlarkPendingSemanticInput pending,
    FlarkSemanticInputSuccessor successor, {
    required int maximum,
  }) {
    if (!reserveSemanticSuccessor(pending, maximum: maximum)) return false;
    pending.successors.add(successor);
    observePendingSuccessors(pending);
    return true;
  }

  FlarkInputDeferralOutcome deferSuccessor({
    FlarkDeferredInputCommand? command,
    String? replacement,
    FlarkPlatformInputTiming? platformTiming,
    required bool certificationDeferred,
    required bool shadowMatchesCurrentInput,
    required int maximumSuccessors,
  }) {
    final pending = pendingSemantic;
    final timing = platformTiming ?? activeTiming;
    if (pending != null) {
      final stored = appendPendingSuccessor(
        pending,
        FlarkDeferredInputSuccessor(
          command,
          replacement: replacement,
          reclassifyAfterCertification:
              certificationDeferred && command != null,
          platformTiming: timing,
        ),
        maximum: maximumSuccessors,
      );
      return stored
          ? const FlarkInputDeferralStored()
          : const FlarkInputDeferralResync(
              FlarkInputResyncReason.successorQueueOverflow,
            );
    }

    final late = lateSemantic;
    if (late == null || command == null || replacement != null) {
      return const FlarkInputDeferralIgnored();
    }
    if (shadowMatchesCurrentInput) {
      lateSemantic = null;
      return const FlarkInputDeferralIgnored();
    }
    if (late.successorCount >= maximumSuccessors) {
      lateSemantic = null;
      return const FlarkInputDeferralResync(
        FlarkInputResyncReason.successorQueueOverflow,
      );
    }
    late.successorCount += 1;
    observeSuccessorCount(late.successorCount);
    lateSemantic = null;
    return FlarkInputDeferralPromote(command: command, platformTiming: timing);
  }

  Completer<void> beginCertificationDeferredInput() {
    final pending = pendingSemantic;
    if (pending == null) {
      throw StateError('Certification-deferred input requires live lineage');
    }
    return pending.certificationPromotion ??= Completer<void>();
  }

  Completer<void>? takeCertificationPromotion() {
    final pending = pendingSemantic;
    final promotion = pending?.certificationPromotion;
    if (pending != null) pending.certificationPromotion = null;
    return promotion;
  }

  void setSemanticFallback(
    FlarkPendingSemanticInput pending,
    FlarkDeferredInputCommand? fallback,
  ) {
    if (!identical(pendingSemantic, pending)) {
      throw StateError('Semantic fallback requires the owned lineage');
    }
    pending.fallbackWhenNotApplied = fallback;
  }

  void prependSemanticFallback(FlarkPendingSemanticInput pending) {
    if (!identical(pendingSemantic, pending)) {
      throw StateError('Semantic fallback requires the owned lineage');
    }
    final fallback = pending.fallbackWhenNotApplied;
    if (fallback == null) return;
    pending.successors.insert(
      0,
      FlarkDeferredInputSuccessor(
        fallback,
        semanticAlreadyAttempted: true,
        platformTiming: pending.platformTiming,
      ),
    );
    // This fallback is synthesized after a Core not-applicable receipt; it is
    // not another platform-observed successor and must not inflate the public
    // successor high-water receipt.
  }

  void retainLateAfterCommit({
    required FlarkPendingSemanticInput pending,
    required FlarkInputReconciliationMap reconciliation,
  }) {
    if (_lineage != null) {
      throw StateError('Late lineage requires the pending lineage to be taken');
    }
    lateSemantic = FlarkLateSemanticInput(
      provisionalTail: pending.provisionalTail,
      reconciliation: reconciliation,
      successorCount: pending.successors.length,
    );
  }

  void carryPendingSuccessors({
    required FlarkPendingSemanticInput from,
    required int startIndex,
    required FlarkPendingSemanticInput to,
  }) {
    if (!identical(pendingSemantic, to)) {
      throw StateError('Successor carry requires the new owned lineage');
    }
    if (startIndex < 0 || startIndex > from.successors.length) {
      throw RangeError.range(
        startIndex,
        0,
        from.successors.length,
        'startIndex',
      );
    }
    to.successors.addAll(from.successors.skip(startIndex));
    to.provisionalTail = from.provisionalTail;
    observePendingSuccessors(to);
  }

  Future<bool>? deferHistoryReplay({
    required bool undoDirection,
    required int maximumSuccessors,
  }) {
    final pending = pendingSemantic;
    if (pending == null) return null;
    final completion = Completer<bool>();
    if (!appendPendingSuccessor(
      pending,
      FlarkDeferredHistorySuccessor(
        undoDirection: undoDirection,
        completion: completion,
      ),
      maximum: maximumSuccessors,
    )) {
      return Future<bool>.value(false);
    }
    return completion.future;
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

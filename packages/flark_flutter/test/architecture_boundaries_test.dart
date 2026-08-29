import 'dart:async';

import 'package:flark_flutter/src/editor_input_state.dart';
import 'package:flark_flutter/src/editor_transactions.dart';
import 'package:flark_flutter/src/editor_performance.dart';
import 'package:flark_flutter/src/input_reconciliation.dart';
import 'package:flark_flutter/src/input_transaction_state.dart';
import 'package:flark_flutter/src/input_window.dart';
import 'package:flark_flutter/src/platform_input_bridge.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('editor input state', () {
    test('activation publishes one scalar-aligned bounded window', () {
      final state = FlarkEditorInputState();
      const sourceStart = 100;
      const text = '01234😀567890abcdefghij';

      final represented = state.activateWindow(
        text: text,
        sourceStart: sourceStart,
        caret: sourceStart + 9,
        selectionExtent: sourceStart + 12,
        ordinal: 7,
        affinity: TextAffinity.upstream,
        maximumCodeUnits: 8,
      );

      expect(represented, isTrue);
      expect(state.value.text.length, lessThanOrEqualTo(8));
      expect(state.activeOrdinal, 7);
      expect(state.selectionBaseUtf16, sourceStart + 9);
      expect(state.selectionExtentUtf16, sourceStart + 12);
      expect(state.crossRowSelection, isTrue);
      expect(
        state.globalUtf16Start + state.value.selection.baseOffset,
        state.selectionBaseUtf16,
      );
      expect(
        state.globalUtf16Start + state.value.selection.extentOffset,
        state.selectionExtentUtf16,
      );
      expect(_startsWithLowSurrogate(state.value.text), isFalse);
      expect(_endsWithHighSurrogate(state.value.text), isFalse);
    });

    test('unrepresentable selection preserves exact canonical endpoints', () {
      final state = FlarkEditorInputState();

      final represented = state.activateWindow(
        text: '0123456789abcdefghij',
        sourceStart: 100,
        caret: 102,
        selectionExtent: 118,
        ordinal: 3,
        affinity: TextAffinity.downstream,
        maximumCodeUnits: 8,
      );

      expect(represented, isFalse);
      expect(state.value.selection.isCollapsed, isTrue);
      expect(state.selectionBaseUtf16, 102);
      expect(state.selectionExtentUtf16, 118);
      expect(state.value.text.length, lessThanOrEqualTo(8));

      state.markOversizedSelection(
        base: state.selectionBaseUtf16,
        extent: state.selectionExtentUtf16,
        activeOrdinal: state.activeOrdinal,
      );
      expect(state.oversizedSelection, isTrue);
      expect(state.crossRowSelection, isTrue);
    });

    test('collapsed restoration updates canonical selection atomically', () {
      final state = FlarkEditorInputState();

      state.activateCollapsedWindow(
        text: '01234😀567890abcdefghij',
        sourceStart: 40,
        caret: 51,
        ordinal: 4,
        maximumCodeUnits: 7,
      );

      expect(state.selectionBaseUtf16, 51);
      expect(state.selectionExtentUtf16, 51);
      expect(state.activeOrdinal, 4);
      expect(state.crossRowSelection, isFalse);
      expect(state.value.text.length, lessThanOrEqualTo(7));
      expect(_startsWithLowSurrogate(state.value.text), isFalse);
      expect(_endsWithHighSurrogate(state.value.text), isFalse);
    });

    test('window bounds reject nonpositive capacities', () {
      final state = FlarkEditorInputState();

      expect(
        () => state.activateWindow(
          text: 'a',
          sourceStart: 0,
          caret: 0,
          ordinal: 0,
          affinity: TextAffinity.downstream,
          maximumCodeUnits: 0,
        ),
        throwsArgumentError,
      );
      expect(
        () => state.activateCollapsedWindow(
          text: 'a',
          sourceStart: 0,
          caret: 0,
          ordinal: 0,
          maximumCodeUnits: -1,
        ),
        throwsArgumentError,
      );
    });
  });

  group('input transaction state', () {
    test('callback scope owns timing and rejects nested callbacks', () {
      final state = FlarkInputTransactionState();
      final timing = state.beginCallback();
      final pending = FlarkPendingSemanticInput(
        base: const TextEditingValue(text: 'a'),
        inputGlobalUtf16Start: 0,
        initialCallbackStartedEpochMicros: timing.acceptedAtEpochMicros,
        platformTiming: timing,
        provisionalAfter: const TextEditingValue(text: 'ab'),
      );
      state.pendingSemantic = pending;

      expect(state.beginCallback, throwsStateError);
      state.finishCallback(timing);

      expect(state.activeTiming, isNull);
      expect(state.activeCallbackStartedEpochMicros, isNull);
      expect(pending.initialCallbackMicros, greaterThanOrEqualTo(0));
      expect(() => state.finishCallback(timing), throwsStateError);
    });

    test('semantic and late lineages are mutually exclusive', () {
      final state = FlarkInputTransactionState();
      final pending = FlarkPendingSemanticInput(
        base: const TextEditingValue(text: 'a'),
        inputGlobalUtf16Start: 0,
        initialCallbackStartedEpochMicros: 1,
        provisionalAfter: const TextEditingValue(text: 'ab'),
      );
      final late = FlarkLateSemanticInput(
        provisionalTail: const TextEditingValue(text: 'ab'),
        reconciliation: const FlarkInputReconciliationMap(
          fromStart: 1,
          fromEnd: 1,
          toStart: 1,
          toEnd: 1,
        ),
        successorCount: 0,
      );

      state.pendingSemantic = pending;
      expect(state.pendingSemantic, same(pending));
      expect(() => state.lateSemantic = late, throwsStateError);
      state.pendingSemantic = null;
      state.lateSemantic = late;
      expect(state.pendingSemantic, isNull);
      expect(state.lateSemantic, same(late));

      state.pendingSemantic = null;
      expect(state.lateSemantic, same(late));
      state.lateSemantic = null;
      expect(state.lateSemantic, isNull);
    });

    test('paired platform commands are consumed exactly once', () {
      final state = FlarkInputTransactionState();

      state.markNewlineTextObserved();
      expect(
        state.consumeNewlineAction(textObservationAlreadyApplied: false),
        isTrue,
      );
      expect(
        state.consumeNewlineAction(textObservationAlreadyApplied: false),
        isFalse,
      );

      state.markBackspaceTextObserved();
      expect(
        state.consumeBackspaceSelector(textObservationAlreadyApplied: false),
        isTrue,
      );
      expect(
        state.consumeBackspaceSelector(textObservationAlreadyApplied: false),
        isFalse,
      );
      expect(
        state.consumeBackspaceSelector(textObservationAlreadyApplied: true),
        isTrue,
      );
    });

    test('mutation scope and metrics enforce monotone invariants', () {
      final state = FlarkInputTransactionState();

      state.beginPlatformMutation();
      expect(state.platformMutationActive, isTrue);
      expect(state.beginPlatformMutation, throwsStateError);
      state.endPlatformMutation();
      expect(state.platformMutationActive, isFalse);
      expect(state.endPlatformMutation, throwsStateError);

      state.observeSuccessorCount(3);
      state.observeSuccessorCount(1);
      expect(state.successorHighWatermark, 3);
      state.recordReconciliationMicros(7);
      expect(state.lastReconciliationMicros, 7);
      expect(() => state.recordReconciliationMicros(-1), throwsArgumentError);
    });

    test('composition input base is first-writer until cleared', () {
      final state = FlarkInputTransactionState();
      state.rememberCompositionInputBase(
        windowStart: 2,
        value: const TextEditingValue(
          text: 'a',
          composing: TextRange(start: 0, end: 1),
        ),
      );
      state.rememberCompositionInputBase(
        windowStart: 9,
        value: const TextEditingValue(text: 'b'),
      );

      expect(state.compositionInputBase!.windowStart, 2);
      expect(state.compositionInputBase!.value.text, 'a');
      expect(state.compositionInputBase!.value.composing, TextRange.empty);

      state.clearCompositionInputBase();
      expect(state.compositionInputBase, isNull);
    });

    test('semantic successor classification owns logical input lineage', () {
      final state = FlarkInputTransactionState();

      final newline = state.classifySemanticSuccessor(
        const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 1),
        ),
        const TextEditingValue(
          text: 'a\n',
          selection: TextSelection.collapsed(offset: 2),
        ),
        mutation: const FlarkTextMutation(1, 1, '\n'),
      );
      expect(newline?.command, FlarkDeferredInputCommand.insertNewline);

      final backspace = state.classifySemanticSuccessor(
        const TextEditingValue(
          text: 'a🌍',
          selection: TextSelection.collapsed(offset: 3),
        ),
        const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 1),
        ),
        mutation: const FlarkTextMutation(1, 3, ''),
      );
      expect(backspace?.command, FlarkDeferredInputCommand.deleteBackward);

      final replacement = state.classifySemanticSuccessor(
        const TextEditingValue(
          text: 'abc',
          selection: TextSelection(baseOffset: 1, extentOffset: 2),
        ),
        const TextEditingValue(
          text: 'axc',
          selection: TextSelection.collapsed(offset: 2),
        ),
        mutation: const FlarkTextMutation(1, 2, 'x'),
      );
      expect(replacement?.command, isNull);
      expect(replacement?.replacement, 'x');
    });

    test('successor overflow retires lineage and completes waiters', () async {
      final state = FlarkInputTransactionState();
      final history = Completer<bool>();
      final pending = FlarkPendingSemanticInput(
        base: const TextEditingValue(text: 'a'),
        inputGlobalUtf16Start: 0,
        initialCallbackStartedEpochMicros: 1,
        provisionalAfter: const TextEditingValue(text: 'a'),
      );
      pending.successors.add(
        FlarkDeferredHistorySuccessor(undoDirection: true, completion: history),
      );
      state.pendingSemantic = pending;

      expect(state.reserveSemanticSuccessor(pending, maximum: 1), isFalse);
      expect(state.pendingSemantic, isNull);
      expect(await history.future, isFalse);
    });
  });

  group('platform input bridge', () {
    test('platform updates advance the window without reconnecting', () {
      final bridge = FlarkPlatformInputBridge();
      bridge.install(
        text: 'abc',
        globalStart: 4,
        selection: const TextSelection.collapsed(offset: 3),
        platformOriginated: false,
        closed: false,
        faulted: false,
      );
      final connection = bridge.connectionEpoch;
      final window = bridge.windowEpoch;

      bridge.install(
        text: 'abcx',
        globalStart: 4,
        selection: const TextSelection.collapsed(offset: 4),
        platformOriginated: true,
        closed: false,
        faulted: false,
      );

      expect(bridge.connectionEpoch, connection);
      expect(bridge.windowEpoch, window + 1);
      expect(bridge.shadowText, 'abcx');
    });

    test('batch validation is atomic and rejects a broken delta chain', () {
      final bridge = FlarkPlatformInputBridge();
      const initial = TextEditingValue(
        text: 'abc',
        selection: TextSelection.collapsed(offset: 3),
      );
      bridge.install(
        text: initial.text,
        globalStart: 0,
        selection: initial.selection,
        platformOriginated: false,
        closed: false,
        faulted: false,
      );

      final result = bridge.validateDeltaBatch(const [
        TextEditingDeltaInsertion(
          oldText: 'abc',
          textInserted: 'x',
          insertionOffset: 3,
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange.empty,
        ),
        TextEditingDeltaDeletion(
          oldText: 'abc',
          deletedRange: TextRange(start: 0, end: 1),
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      ], fallbackValue: initial);

      expect(result, FlarkInputResyncReason.deltaChainMismatch);
      expect(bridge.shadowText, initial.text);
      expect(bridge.resyncCount, 0);
    });

    test('value differencing never splits a UTF-16 scalar', () {
      final bridge = FlarkPlatformInputBridge();
      final mutation = bridge.differenceMutation('x😀y', 'x😁y');

      expect(mutation, isNotNull);
      expect((mutation!.start, mutation.end), (1, 3));
      expect(mutation.replacement, '😁');
    });

    test('newline recognition is bound to the serialized selection', () {
      final bridge = FlarkPlatformInputBridge();
      bridge.install(
        text: 'ab',
        globalStart: 0,
        selection: const TextSelection(baseOffset: 0, extentOffset: 2),
        platformOriginated: false,
        closed: false,
        faulted: false,
      );
      const current = TextEditingValue(
        text: 'ab',
        selection: TextSelection(baseOffset: 0, extentOffset: 2),
      );
      const delta = TextEditingDeltaReplacement(
        oldText: 'ab',
        replacementText: '\n',
        replacedRange: TextRange(start: 0, end: 2),
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange.empty,
      );

      expect(
        bridge.isNewlineDeltaBatch(const [delta], currentValue: current),
        isTrue,
      );
    });

    test('full-value Backspace recognizes the grapheme before the caret', () {
      final bridge = FlarkPlatformInputBridge();
      const current = TextEditingValue(
        text: 'a😀',
        selection: TextSelection.collapsed(offset: 3),
      );
      bridge.install(
        text: current.text,
        globalStart: 0,
        selection: current.selection,
        platformOriginated: false,
        closed: false,
        faulted: false,
      );

      expect(
        bridge.isDeleteBackwardValue(
          currentValue: current,
          observedValue: const TextEditingValue(
            text: 'a',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        isTrue,
      );
    });

    test(
      'delta and full-value insertion normalize to the same transaction',
      () {
        final bridge = FlarkPlatformInputBridge();
        const current = TextEditingValue(
          text: 'ab',
          selection: TextSelection.collapsed(offset: 1),
        );
        bridge.install(
          text: current.text,
          globalStart: 0,
          selection: current.selection,
          platformOriginated: false,
          closed: false,
          faulted: false,
        );
        const after = TextEditingValue(
          text: 'axb',
          selection: TextSelection.collapsed(offset: 2),
        );
        final delta = bridge.observeDeltaBatch(const [
          TextEditingDeltaInsertion(
            oldText: 'ab',
            textInserted: 'x',
            insertionOffset: 1,
            selection: TextSelection.collapsed(offset: 2),
            composing: TextRange.empty,
          ),
        ], currentValue: current);
        final value = bridge.observeValue(after, currentValue: current);

        expect(delta.accepted, isTrue);
        expect(value.accepted, isTrue);
        expect(delta.after, value.after);
        expect(
          (delta.effectiveMutation!.start, delta.effectiveMutation!.end),
          (value.effectiveMutation!.start, value.effectiveMutation!.end),
        );
        expect(
          delta.effectiveMutation!.replacement,
          value.effectiveMutation!.replacement,
        );
        expect(delta.typingInput, isTrue);
        expect(value.typingInput, isTrue);
        expect(delta.newlineCommand, isFalse);
        expect(value.newlineCommand, isFalse);
      },
    );

    test('delta and full-value commands share newline and Backspace facts', () {
      final newlineBridge = FlarkPlatformInputBridge();
      const selected = TextEditingValue(
        text: 'ab',
        selection: TextSelection(baseOffset: 0, extentOffset: 2),
      );
      newlineBridge.install(
        text: selected.text,
        globalStart: 0,
        selection: selected.selection,
        platformOriginated: false,
        closed: false,
        faulted: false,
      );
      final newlineDelta = newlineBridge.observeDeltaBatch(const [
        TextEditingDeltaReplacement(
          oldText: 'ab',
          replacementText: '\n',
          replacedRange: TextRange(start: 0, end: 2),
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ], currentValue: selected);
      final newlineValue = newlineBridge.observeValue(
        const TextEditingValue(
          text: '\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
        currentValue: selected,
      );

      expect(newlineDelta.newlineCommand, isTrue);
      expect(newlineValue.newlineCommand, isTrue);
      expect(newlineDelta.selectedDeletion, isFalse);
      expect(newlineValue.selectedDeletion, isFalse);

      final backspaceBridge = FlarkPlatformInputBridge();
      const emoji = TextEditingValue(
        text: 'a😀',
        selection: TextSelection.collapsed(offset: 3),
      );
      backspaceBridge.install(
        text: emoji.text,
        globalStart: 0,
        selection: emoji.selection,
        platformOriginated: false,
        closed: false,
        faulted: false,
      );
      final backspaceDelta = backspaceBridge.observeDeltaBatch(const [
        TextEditingDeltaDeletion(
          oldText: 'a😀',
          deletedRange: TextRange(start: 1, end: 3),
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ], currentValue: emoji);
      final backspaceValue = backspaceBridge.observeValue(
        const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 1),
        ),
        currentValue: emoji,
      );

      expect(backspaceDelta.deleteBackwardCommand, isTrue);
      expect(backspaceValue.deleteBackwardCommand, isTrue);
      expect(backspaceDelta.effectiveMutation!.replacement, isEmpty);
      expect(backspaceValue.effectiveMutation!.replacement, isEmpty);
    });

    test('a rejected batch never produces a partial observation', () {
      final bridge = FlarkPlatformInputBridge();
      const current = TextEditingValue(
        text: 'abc',
        selection: TextSelection.collapsed(offset: 3),
      );
      bridge.install(
        text: current.text,
        globalStart: 0,
        selection: current.selection,
        platformOriginated: false,
        closed: false,
        faulted: false,
      );

      final observation = bridge.observeDeltaBatch(const [
        TextEditingDeltaInsertion(
          oldText: 'stale',
          textInserted: 'x',
          insertionOffset: 3,
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange.empty,
        ),
      ], currentValue: current);

      expect(observation.accepted, isFalse);
      expect(observation.rejection, FlarkInputResyncReason.oldTextMismatch);
      expect(observation.after, current);
      expect(observation.effectiveMutation, isNull);
      expect(observation.mutatingChanges, 0);
    });
  });

  test('performance log bounds diagnostic receipts independently', () {
    final log = FlarkEditorPerformanceLog(maximumReceipts: 2);
    for (var generation = 1; generation <= 3; generation += 1) {
      log.recordSemantic(
        FlarkSemanticEditPerformance(
          sourceGeneration: generation,
          platformCallbackMicros: 0,
          coreQueueMicros: 0,
          workerRoundTripMicros: 0,
          workerQueueMicros: 0,
          nativeFfiMicros: 0,
          coreAdoptionMicros: 0,
          flutterReceiptAdoptionMicros: 0,
          callbackToReceiptMicros: 0,
        ),
      );
      log.recordSource(
        FlarkSourceEditPerformance(
          kind: FlarkSourceEditPerformanceKind.source,
          sourceGeneration: generation,
          coreQueueMicros: 0,
          workerRoundTripMicros: 0,
          workerQueueMicros: 0,
          nativeFfiMicros: 0,
          coreAdoptionMicros: 0,
          flutterReceiptAdoptionMicros: 0,
          acceptanceToReceiptMicros: 0,
        ),
      );
    }

    expect(log.semantic.map((receipt) => receipt.sourceGeneration), [2, 3]);
    expect(log.source.map((receipt) => receipt.sourceGeneration), [2, 3]);
    expect(log.lastSemantic?.sourceGeneration, 3);
    expect(
      () => FlarkEditorPerformanceLog(maximumReceipts: 0),
      throwsArgumentError,
    );
  });
}

bool _startsWithLowSurrogate(String value) =>
    value.isNotEmpty &&
    value.codeUnitAt(0) >= 0xDC00 &&
    value.codeUnitAt(0) <= 0xDFFF;

bool _endsWithHighSurrogate(String value) =>
    value.isNotEmpty &&
    value.codeUnitAt(value.length - 1) >= 0xD800 &&
    value.codeUnitAt(value.length - 1) <= 0xDBFF;

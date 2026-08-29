import 'package:flark_flutter/src/editor_transactions.dart';
import 'package:flark_flutter/src/input_reconciliation.dart';
import 'package:flark_flutter/src/input_transaction_state.dart';
import 'package:flark_flutter/src/input_window.dart';
import 'package:flark_flutter/src/platform_input_bridge.dart';
import 'package:flark_flutter/src/surface_projector.dart';
import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
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
  });

  group('surface projector', () {
    FlarkViewportRow row() => FlarkViewportRow(
      ordinal: 0,
      kind: 5,
      sourceBytes: const FlarkSourceRange(0, 3),
      sourceUtf16: const FlarkSourceRange(0, 3),
      editableBytes: const FlarkSourceRange(0, 3),
      editableUtf16: const FlarkSourceRange(0, 3),
      editCapability: FlarkViewportRowEditCapability.contiguous,
      headingLevel: null,
      headingStyle: null,
      listItem: null,
      blockQuote: null,
      codeBlock: null,
      thematicBreak: false,
      pathDepth: 0,
      inlineFacts: const [],
    );

    test(
      'captures optimistic range state instead of sharing controller state',
      () {
        final optimisticRanges = FlarkOptimisticRangeMap();
        final projector = FlarkSurfaceProjector(
          pendingPresentation: const FlarkPendingPresentationSnapshot.empty(),
          visibleUtf16Start: 0,
          visibleSource: 'abc',
          inputGlobalUtf16Start: 0,
          inputValue: const TextEditingValue(
            text: 'abc',
            selection: TextSelection.collapsed(offset: 3),
          ),
          activeOrdinal: 0,
          selectionBaseUtf16: 3,
          selectionExtentUtf16: 3,
          crossRowSelection: false,
          semanticViewportCurrent: true,
          certificationRevisionCurrent: true,
          certificationRanges: const [],
          optimisticRanges: optimisticRanges,
        );

        optimisticRanges.add(
          const FlarkOptimisticViewportEdit(
            start: 0,
            end: 0,
            replacementLength: 2,
          ),
        );

        final sourceRange = projector.surfaceSourceRange(row());
        expect((sourceRange.start, sourceRange.end), (0, 3));
        expect(projector.surfaceRow(row()).text, 'abc');
      },
    );
  });
}

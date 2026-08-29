import 'dart:async';

import 'package:flark_flutter/src/editor_runtime.dart';
import 'package:flark_flutter/src/editor_snapshot.dart';
import 'package:flark_flutter/src/editor_transactions.dart';
import 'package:flark_flutter/src/input_reconciliation.dart';
import 'package:flark_flutter/src/input_transaction_state.dart';
import 'package:flark_flutter/src/input_window.dart';
import 'package:flark_flutter/src/optimistic_range_map.dart';
import 'package:flark_flutter/src/platform_input_bridge.dart';
import 'package:flark_flutter/src/surface_projector.dart';
import 'package:flark_flutter/src/viewport_installation.dart';
import 'package:flark_flutter/src/viewport_navigation_state.dart';
import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('editor runtime lineage', () {
    test('stale effects cannot cross edit or interaction generations', () {
      final runtime = FlarkEditorRuntimeState();
      final initial = runtime.stamp;

      runtime.recordInteraction();
      expect(runtime.accepts(initial), isTrue);
      expect(runtime.accepts(initial, requireInteraction: true), isFalse);

      runtime.admitEditingCommand();
      expect(runtime.accepts(initial), isFalse);
    });

    test('an older completion cannot clear a newer publication barrier', () {
      final runtime = FlarkEditorRuntimeState();
      final firstGeneration = runtime.admitEditingCommand();
      runtime.beginPublicationBarrier();

      final secondGeneration = runtime.admitEditingCommand();
      runtime.beginPublicationBarrier();

      expect(runtime.endPublicationBarrierForEdit(firstGeneration), isFalse);
      expect(runtime.publicationCertificationBarrierActive, isTrue);
      expect(runtime.endPublicationBarrierForEdit(secondGeneration), isTrue);
      expect(runtime.publicationCertificationBarrierActive, isFalse);
    });

    test('disposed is a terminal runtime status', () {
      final runtime = FlarkEditorRuntimeState()..markDisposed();

      expect(
        () => runtime.transitionStatus(FlarkEditorStatus.ready),
        throwsStateError,
      );
    });

    test('closing blocks new effects but can drain admitted page work', () {
      final runtime = FlarkEditorRuntimeState();
      final admitted = runtime.stamp;

      runtime.beginClosing();

      expect(runtime.accepts(admitted), isFalse);
      expect(runtime.accepts(admitted, allowClosing: true), isTrue);
    });

    test('edit operations have one serialized runtime tail', () async {
      final runtime = FlarkEditorRuntimeState();
      final firstStarted = Completer<void>();
      final releaseFirst = Completer<void>();
      final events = <String>[];

      final first = runtime.queueEdit(() async {
        events.add('first-start');
        firstStarted.complete();
        await releaseFirst.future;
        events.add('first-end');
      });
      final second = runtime.queueEdit(() async {
        events.add('second');
      });

      await firstStarted.future;
      expect(events, ['first-start']);
      releaseFirst.complete();
      await Future.wait([first, second]);
      expect(events, ['first-start', 'first-end', 'second']);
    });

    test('parser work is single-flight', () async {
      final runtime = FlarkEditorRuntimeState();
      final started = Completer<void>();
      final release = Completer<void>();
      var starts = 0;

      Future<void> parse() async {
        starts += 1;
        started.complete();
        await release.future;
      }

      final first = runtime.runParser(parse);
      await started.future;
      final joined = runtime.runParser(() async {
        starts += 100;
      });
      expect(identical(first, joined), isTrue);
      release.complete();
      await joined;
      expect(starts, 1);
      expect(runtime.parserTask, isNull);
    });

    test(
      'completed page work reopens admission without staying in flight',
      () async {
        final runtime = FlarkEditorRuntimeState();
        var callerSettled = false;

        final page = runtime.runPage(() async => true);
        final caller = page.whenComplete(() => callerSettled = true);
        await runtime.pageTask;
        await caller;

        expect(callerSettled, isTrue);
        expect(runtime.pageTask, isNull);
        expect(await runtime.runPage(() async => false), isFalse);
      },
    );

    test('pending edit accounting has one owner and cannot underflow', () {
      final runtime = FlarkEditorRuntimeState();

      runtime.beginPendingEdit();
      runtime.beginPendingEdit();
      expect(runtime.pendingEdits, 2);
      runtime.endPendingEdit();
      runtime.endPendingEdit();
      expect(runtime.pendingEdits, 0);
      expect(runtime.endPendingEdit, throwsStateError);
    });

    test('history replay is exclusive and runtime-owned', () {
      final runtime = FlarkEditorRuntimeState();

      runtime.beginHistoryReplay();
      expect(runtime.historyReplayPending, isTrue);
      expect(runtime.beginHistoryReplay, throwsStateError);
      runtime.endHistoryReplay();
      expect(runtime.historyReplayPending, isFalse);
      expect(runtime.endHistoryReplay, throwsStateError);

      runtime.beginClosing();
      expect(runtime.beginHistoryReplay, throwsStateError);
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

  group('viewport installation plan', () {
    FlarkViewport viewport({
      required FlarkCertification certification,
      required String source,
    }) => FlarkViewport(
      revision: 1,
      snapshot: 1,
      requestedBytes: FlarkSourceRange(0, source.length),
      coveredBytes: FlarkSourceRange(0, source.length),
      coveredUtf16: FlarkSourceRange(0, source.length),
      certification: certification,
      rows: const [],
      neutralSource: source,
      continuation: 0,
    );

    test('certified zero-row result is current without installing rows', () {
      final plan = FlarkViewportInstallationPlan.evaluate(
        viewport: viewport(
          certification: FlarkCertification.currentCertified,
          source: '',
        ),
        source: '',
        previousVisibleUtf16Start: 0,
        previousVisibleSource: 'old',
        mappedCachedRowRanges: const [],
      );

      expect(plan.installsFreshRows, isFalse);
      expect(plan.installsCertifiedSurface, isTrue);
      expect(plan.retainsExistingSurface, isFalse);
    });

    test('pending exact source can retain a matching certified shell', () {
      final plan = FlarkViewportInstallationPlan.evaluate(
        viewport: viewport(
          certification: FlarkCertification.pendingNeutral,
          source: 'abc',
        ),
        source: 'abc',
        previousVisibleUtf16Start: 0,
        previousVisibleSource: 'abc',
        mappedCachedRowRanges: [FlarkSourceRange(0, 3)],
      );

      expect(plan.retainsExistingSurface, isTrue);
      expect(plan.installsFreshRows, isFalse);
      expect(plan.installsCertifiedSurface, isFalse);
    });
  });

  group('viewport navigation state', () {
    const pageA = FlarkViewportPageAnchor(byte: 100, utf16: 80);
    const pageB = FlarkViewportPageAnchor(byte: 220, utf16: 170);
    const replacementB = FlarkViewportPageAnchor(byte: 210, utf16: 165);

    FlarkViewport viewportAt(FlarkViewportPageAnchor anchor) => FlarkViewport(
      revision: 1,
      snapshot: 1,
      requestedBytes: FlarkSourceRange(anchor.byte, anchor.byte + 10),
      coveredBytes: FlarkSourceRange(anchor.byte, anchor.byte + 10),
      coveredUtf16: FlarkSourceRange(anchor.utf16, anchor.utf16 + 10),
      certification: FlarkCertification.currentCertified,
      rows: const [],
      neutralSource: '0123456789',
      continuation: 0,
    );

    test('page path and index advance atomically', () {
      final navigation = FlarkViewportNavigationState();

      navigation.advanceTo(pageA);
      navigation.advanceTo(pageB);

      expect(navigation.pageIndex, 2);
      expect(navigation.previousAnchor, pageA);
      expect(navigation.currentPageMatches(viewportAt(pageB)), isTrue);
      expect(() => navigation.advanceTo(pageB), throwsStateError);
    });

    test('backward adoption normalizes a rewound enclosing-row anchor', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB);

      navigation.moveBackwardTo(
        const FlarkViewportPageAnchor(byte: 90, utf16: 70),
      );

      expect(navigation.pagePath, const [
        FlarkViewportPageAnchor.zero,
        FlarkViewportPageAnchor(byte: 90, utf16: 70),
      ]);
      expect(navigation.pageIndex, 1);
    });

    test('refresh path rejects a torn or nonmonotone history', () {
      final navigation = FlarkViewportNavigationState();

      expect(
        () => navigation.installRefreshPath(const [pageA]),
        throwsArgumentError,
      );
      expect(
        () => navigation.installRefreshPath(const [
          FlarkViewportPageAnchor.zero,
          pageB,
          replacementB,
        ]),
        throwsArgumentError,
      );
    });

    test('new forward navigation replaces abandoned history', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB)
        ..moveBackwardTo(pageA)
        ..advanceTo(replacementB);

      expect(navigation.pagePath, const [
        FlarkViewportPageAnchor.zero,
        pageA,
        replacementB,
      ]);
    });

    test('refresh origin retains the earliest affected edit', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB);

      navigation.retainRefreshAnchorForEdit(
        editStart: 160,
        deriveFromInput: false,
        currentViewport: null,
        inputGlobalUtf16Start: 0,
        inputText: '',
      );
      navigation.retainRefreshAnchorForEdit(
        editStart: 240,
        deriveFromInput: false,
        currentViewport: null,
        inputGlobalUtf16Start: 0,
        inputText: '',
      );
      expect(navigation.refreshAnchor, pageA);

      navigation.retainRefreshAnchorForEdit(
        editStart: 60,
        deriveFromInput: false,
        currentViewport: null,
        inputGlobalUtf16Start: 0,
        inputText: '',
      );
      expect(navigation.refreshAnchor, FlarkViewportPageAnchor.zero);
      expect(
        navigation.refreshAnchorForCaret(20),
        FlarkViewportPageAnchor.zero,
      );
      navigation.clearRefreshAnchor();
      expect(navigation.refreshAnchor, isNull);
    });

    test('refresh origin can rewind through the exact input snapshot', () {
      final navigation = FlarkViewportNavigationState();

      navigation.retainRefreshAnchorForEdit(
        editStart: 7,
        deriveFromInput: true,
        currentViewport: viewportAt(
          const FlarkViewportPageAnchor(byte: 10, utf16: 10),
        ),
        inputGlobalUtf16Start: 0,
        inputText: 'aaaa\nbbbb\ncccc',
      );

      final retained = navigation.refreshAnchor!;
      expect((retained.byte, retained.utf16), (5, 5));
    });

    test('caret byte window keeps the preceding newline boundary', () {
      final navigation = FlarkViewportNavigationState();

      final window = navigation.byteWindowForCaret(
        origin: FlarkViewportPageAnchor.zero,
        visibleUtf16Start: 0,
        visibleSource: 'abc\ndef',
        caret: 4,
        sourceByteLength: 7,
        maximumVisibleBytes: 16,
      );

      expect(window, isNotNull);
      expect(
        (window!.startByte, window.startUtf16, window.caretByte),
        (0, 0, 4),
      );
    });

    test('known edit anchor selects the nearest certified origin', () {
      final navigation = FlarkViewportNavigationState()
        ..advanceTo(pageA)
        ..advanceTo(pageB);

      expect(navigation.knownAnchorFor(160), pageA);
      final selected = navigation.knownAnchorFor(
        205,
        currentViewport: viewportAt(
          const FlarkViewportPageAnchor(byte: 240, utf16: 190),
        ),
      );
      expect((selected.byte, selected.utf16), (240, 190));
    });
  });

  group('optimistic range map', () {
    test('maps following ranges without preserving a touched range', () {
      final ranges = FlarkOptimisticRangeMap()
        ..add(
          const FlarkOptimisticViewportEdit(
            start: 2,
            end: 2,
            replacementLength: 3,
          ),
        );

      final mapped = ranges.mapRange(FlarkSourceRange(5, 8));
      expect((mapped.start, mapped.end), (8, 11));
      expect(ranges.leavesRangeUnchanged(FlarkSourceRange(5, 8)), isTrue);
      expect(ranges.leavesRangeUnchanged(FlarkSourceRange(0, 4)), isFalse);
    });

    test('container retention fails closed for structural receipts', () {
      final ranges = FlarkOptimisticRangeMap()
        ..add(
          const FlarkOptimisticViewportEdit(
            start: 3,
            end: 4,
            replacementLength: 0,
            preservesMappedRowFacts: false,
          ),
        );

      expect(ranges.staysWithin(FlarkSourceRange(0, 8)), isFalse);
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

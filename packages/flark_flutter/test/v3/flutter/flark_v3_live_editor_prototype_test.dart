import 'dart:convert';
import 'dart:typed_data';

import 'package:flark/flark_adapter.dart';
// ignore: implementation_imports
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
import 'package:flark_flutter/src/v3/flutter/flutter.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'authoritative nested inline facts hide markers and Unsupported restores source paint',
    (tester) async {
      const source = '***x***';
      final sourceSession = FlarkV3SourceSession.fromString(source);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 4),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final editableKey = GlobalKey<EditableTextState>();

      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      final editableState = editableKey.currentState;
      final editingController = controller.editingController;

      controller.adoptInlineIslandPresentation(
        _inlinePresentation(
          sourceDocument: controller.source,
          sourceVersion: controller.sourceVersion,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          records: [
            _inlineRecord(
              kind: 1,
              start: 0,
              length: 7,
              contentStart: 1,
              contentLength: 5,
            ),
            _inlineRecord(
              kind: 2,
              start: 1,
              length: 5,
              contentStart: 3,
              contentLength: 1,
            ),
          ],
        ),
      );
      await tester.pump();

      expect(controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(controller.editingController.text, 'x');
      expect(
        controller.editingController.selection,
        const TextSelection.collapsed(offset: 1),
      );
      expect(controller.globalEditingState.selection.extentOffset, 4);
      expect(controller.hasCertifiedInlinePresentation, isTrue);
      final authoritativeSpan =
          (controller.editingController as FlarkV3InlineTextEditingController)
              .buildTextSpan(
                context: editableKey.currentContext!,
                style: const TextStyle(),
                withComposing: false,
              );
      final nestedRun = authoritativeSpan.children!.single as TextSpan;
      expect(nestedRun.text, 'x');
      expect(nestedRun.style!.fontStyle, FontStyle.italic);
      expect(nestedRun.style!.fontWeight, FontWeight.w700);

      final unsupported = _inlinePresentation(
        sourceDocument: controller.source,
        sourceVersion: controller.sourceVersion,
        disposition: FlarkV3InlineFactsDisposition.unsupported,
      );
      expect(
        unsupported,
        isA<FlarkV3SourcePaintInlineIslandPresentation>().having(
          (decision) => decision.reason,
          'reason',
          FlarkV3InlineIslandSourcePaintReason.inlineFactsUnsupported,
        ),
      );
      controller.adoptInlineIslandPresentation(unsupported);
      await tester.pump();

      expect(controller.source.toString(), source);
      expect(controller.editingController.text, source);
      expect(
        controller.editingController.selection,
        const TextSelection.collapsed(offset: 4),
      );
      expect(controller.hasProjectedInlinePresentation, isFalse);
      expect(controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(
        controller.editingController
            .buildTextSpan(
              context: editableKey.currentContext!,
              style: const TextStyle(),
              withComposing: false,
            )
            .toPlainText(),
        source,
      );
    },
  );

  test(
    'projected backspace removes orphaned nested markers without source guessing',
    () {
      const source = '***x***';
      final sourceSession = FlarkV3SourceSession.fromString(source);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 4),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      controller.adoptInlineIslandPresentation(
        _inlinePresentation(
          sourceDocument: controller.source,
          sourceVersion: controller.sourceVersion,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          records: [
            _inlineRecord(
              kind: 1,
              start: 0,
              length: 7,
              contentStart: 1,
              contentLength: 5,
            ),
            _inlineRecord(
              kind: 2,
              start: 1,
              length: 5,
              contentStart: 3,
              contentLength: 1,
            ),
          ],
        ),
      );

      controller.applyTextEditingDelta(
        const TextEditingDeltaDeletion(
          oldText: 'x',
          deletedRange: TextRange(start: 0, end: 1),
          selection: TextSelection.collapsed(offset: 0),
          composing: TextRange.empty,
        ),
      );
      expect(controller.source.toString(), isEmpty);
      expect(controller.editingController.text, isEmpty);
      expect(
        controller.globalEditingState.selection,
        const TextSelection.collapsed(offset: 0),
      );
      expect(controller.hasProjectedInlinePresentation, isTrue);
      expect(controller.hasCertifiedInlinePresentation, isFalse);

      controller.applyTextEditingDelta(
        const TextEditingDeltaInsertion(
          oldText: '',
          textInserted: 'y',
          insertionOffset: 0,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      );
      expect(controller.source.toString(), 'y');
      expect(controller.editingController.text, 'y');
      expect(
        controller.globalEditingState.selection,
        const TextSelection.collapsed(offset: 1),
      );
      final lease =
          (controller.editingController as FlarkV3InlineTextEditingController)
              .projectedInputLease!;
      final styled = lease.buildTextSpan(
        baseStyle: const TextStyle(),
        composing: TextRange.empty,
      );
      final run = styled.children!.single as TextSpan;
      expect(run.text, 'y');
      expect(run.style!.fontStyle, isNull);
      expect(run.style!.fontWeight, isNull);
    },
  );

  testWidgets(
    'editable opts into the platform delta model and applies a batch exactly',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('a😀b');
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'a😀b',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final editableKey = GlobalKey<EditableTextState>();

      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );

      final editableState = editableKey.currentState!;
      expect(editableState, isA<DeltaTextInputClient>());
      expect(editableState.textInputConfiguration.enableDeltaModel, isTrue);
      final editingController = controller.editingController;

      expect(
        () => controller.applyTextEditingDelta(
          const TextEditingDeltaDeletion(
            oldText: 'a😀b',
            deletedRange: TextRange(start: 1, end: 2),
            selection: TextSelection.collapsed(offset: 1),
            composing: TextRange.empty,
          ),
        ),
        throwsRangeError,
      );
      expect(controller.source.toString(), 'a😀b');
      expect(sourceSession.uiRevision, 0);

      (editableState as DeltaTextInputClient)
          .updateEditingValueWithDeltas(const [
            TextEditingDeltaNonTextUpdate(
              oldText: 'a😀b',
              selection: TextSelection.collapsed(offset: 5),
              composing: TextRange.empty,
            ),
          ]);
      expect(controller.source.toString(), 'a😀b');
      expect(controller.editingController, same(editingController));

      (editableState as DeltaTextInputClient)
          .updateEditingValueWithDeltas(const [
            TextEditingDeltaInsertion(
              oldText: 'a😀b',
              textInserted: '*',
              insertionOffset: 1,
              selection: TextSelection.collapsed(offset: 2),
              composing: TextRange(start: 1, end: 2),
            ),
            TextEditingDeltaReplacement(
              oldText: 'a*😀b',
              replacementText: 'é',
              replacedRange: TextRange(start: 2, end: 4),
              selection: TextSelection.collapsed(offset: 3),
              composing: TextRange(start: 2, end: 3),
            ),
            TextEditingDeltaNonTextUpdate(
              oldText: 'a*éb',
              selection: TextSelection.collapsed(offset: 4),
              composing: TextRange.empty,
            ),
          ]);

      expect(controller.source.toString(), 'a*éb');
      expect(
        sourceSession.uiRevision,
        1,
        reason: 'one platform callback must commit one source revision',
      );
      expect(
        controller.editingController.value,
        const TextEditingValue(
          text: 'a*éb',
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange.empty,
        ),
      );
      expect(controller.globalEditingState.selection.extentOffset, 4);
      await tester.pump();
      expect(editableKey.currentState, same(editableState));
      expect(controller.editingController, same(editingController));

      final revisionBeforeRejectedBatch = sourceSession.uiRevision;
      expect(
        () => (editableState as DeltaTextInputClient)
            .updateEditingValueWithDeltas(const [
              TextEditingDeltaInsertion(
                oldText: 'a*éb',
                textInserted: '!',
                insertionOffset: 4,
                selection: TextSelection.collapsed(offset: 5),
                composing: TextRange.empty,
              ),
              TextEditingDeltaInsertion(
                oldText: 'stale',
                textInserted: '?',
                insertionOffset: 0,
                selection: TextSelection.collapsed(offset: 1),
                composing: TextRange.empty,
              ),
            ]),
        throwsStateError,
      );
      expect(sourceSession.uiRevision, revisionBeforeRejectedBatch);
      expect(controller.source.toString(), 'a*éb');
      expect(controller.editingController, same(editingController));
    },
  );

  testWidgets(
    'full-value platform fallback computes a scalar-safe bounded replacement',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('x😀y');
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'x😀y',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final editableKey = GlobalKey<EditableTextState>();

      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );

      final editableState = editableKey.currentState!;
      editableState.updateEditingValue(
        const TextEditingValue(
          text: 'x😁y',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 1, end: 3),
        ),
      );

      expect(controller.source.toString(), 'x😁y');
      expect(
        controller.editingController.value,
        const TextEditingValue(
          text: 'x😁y',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 1, end: 3),
        ),
      );
      expect(editableKey.currentState, same(editableState));
    },
  );

  testWidgets('focused editor receives the platform delta channel end to end', (
    tester,
  ) async {
    final sourceSession = FlarkV3SourceSession.fromString('**x');
    final controller = FlarkV3FlutterLiveController.attach(
      documentSession: _attachDocumentSession(
        sourceSession: sourceSession,
        hostStore: _FrameModelHostStore(),
      ),
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 64,
        value: const TextEditingValue(
          text: '**x',
          selection: TextSelection.collapsed(offset: 3),
        ),
      ),
      queryBudget: _queryBudget,
    );
    addTearDown(controller.dispose);
    final editableKey = GlobalKey<EditableTextState>();

    await tester.pumpWidget(
      _testApp(
        FlarkV3LiveEditorPrototype(
          controller: controller,
          editableKey: editableKey,
          paintLayerBuilder: (context, state) => const SizedBox.shrink(),
        ),
      ),
    );
    editableKey.currentState!.requestKeyboard();
    await tester.pump();

    expect(tester.testTextInput.setClientArgs!['enableDeltaModel'], isTrue);
    final setClient = tester.testTextInput.log.lastWhere(
      (call) => call.method == 'TextInput.setClient',
    );
    final clientId = (setClient.arguments as List<dynamic>).first as int;
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      SystemChannels.textInput.name,
      SystemChannels.textInput.codec.encodeMethodCall(
        MethodCall('TextInputClient.updateEditingStateWithDeltas', [
          clientId,
          {
            'deltas': [
              {
                'oldText': '**x',
                'deltaText': '*',
                'deltaStart': 3,
                'deltaEnd': 3,
                'selectionBase': 4,
                'selectionExtent': 4,
                'selectionAffinity': 'TextAffinity.downstream',
                'selectionIsDirectional': false,
                'composingBase': -1,
                'composingExtent': -1,
              },
            ],
          },
        ]),
      ),
      (_) {},
    );
    await tester.pump();

    expect(controller.source.toString(), '**x*');
    expect(controller.editingController.text, '**x*');
    expect(controller.editingController.selection.extentOffset, 4);
  });

  testWidgets(
    'external exact-base delta progress adopts exact paint on one Flutter frame',
    (tester) async {
      const baseSource = '**x**';
      const targetSource = '**y**';
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(
        baseSource,
      );
      final documentSession = _attachDocumentSession(
        sourceSession: sourceSession,
        hostStore: _FrameModelHostStore(),
        certifiedSourceVersion: FlarkV3SourceVersion.empty(_documentSession),
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: documentSession,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: baseSource,
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final executor = _ExactBaseDeltaExecutorFake(
        documentSession: documentSession,
        sourceSession: sourceSession,
        onProgress: controller.handleSessionExecutorProgress,
      );

      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            paintLayerBuilder: (context, state) => Text(
              '${state.mode.name}:${state.ack?.hostRevision.value ?? '-'}',
            ),
          ),
        ),
      );

      final base = executor.certifyAndPublishBase(baseSource);
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.sourceGap);
      await tester.pump();
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(find.text('exactStructural:1'), findsOneWidget);

      controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 2,
          localEndUtf16: 3,
          replacement: 'y',
          nextSelection: TextSelection.collapsed(offset: 3),
          nextComposing: TextRange.empty,
        ),
      );
      await tester.pump();
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.stablePaint);
      expect(controller.paintState.ack, base);
      expect(find.text('stablePaint:1'), findsOneWidget);

      final scheduledBeforePromotion = controller.scheduledFrameCallbacks;
      final target = executor.promoteAndPublishDelta(targetSource, base: base);
      expect(
        controller.scheduledFrameCallbacks,
        scheduledBeforePromotion + 1,
        reason: 'all executor progress before the frame must coalesce',
      );
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.stablePaint,
        reason: 'executor progress must not mutate Flutter paint mid-frame',
      );

      await tester.pump();
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(controller.paintState.ack, target);
      expect(controller.paintState.sourceVersion.revision, 2);
      expect(controller.paintState.uiSource.uiRevision, 2);
      expect(controller.semanticActionsValid, isTrue);
      expect(find.text('exactStructural:2'), findsOneWidget);
    },
  );

  testWidgets(
    'rapid Unicode/IME edits retain editable identity while paint authority advances atomically',
    (tester) async {
      const wholeSource = 'prefix\na🌍b\nsuffix';
      final sourceSession = FlarkV3SourceSession.fromString(wholeSource);
      final store = _FrameModelHostStore();
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: store,
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 7,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'a🌍b',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final editableKey = GlobalKey<EditableTextState>();
      var paintActions = 0;

      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            paintLayerBuilder: (context, state) => Semantics(
              label: 'Markdown action',
              button: true,
              child: GestureDetector(
                key: const ValueKey('paint-action'),
                onTap: () => paintActions += 1,
                child: Text(
                  '${state.mode.name}:${state.ack?.hostRevision.value ?? '-'}',
                  key: ValueKey('paint-${state.mode.name}'),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      final editableState = editableKey.currentState;
      final editingController = controller.editingController;
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.sourceGap);
      expect(controller.source.toString(), wholeSource);
      expect(controller.inputIslandGlobalStartUtf16, 7);
      expect(controller.sourceWorkerSynchronized, isFalse);
      expect(
        controller.beginOffer(
          _snapshot(controller.sourceVersion, hostRevision: 1),
        ),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      _acknowledgeAllSourceWorkerSync(controller);
      expect(controller.sourceWorkerSynchronized, isTrue);

      final base = _publishOnePacket(
        controller,
        _snapshot(controller.sourceVersion, hostRevision: 1),
      );
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.sourceGap);
      expect(controller.semanticActionsValid, isFalse);
      await tester.pump();
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(controller.semanticActionsValid, isTrue);
      expect(controller.acknowledgeDelivery(base), isA<FlarkV3HostAccepted>());
      expect(editableKey.currentState, same(editableState));
      expect(controller.editingController, same(editingController));

      final queriesBeforeSplitScalar = store.queryCount;
      controller.updateLocalEditingValue(
        const TextEditingValue(
          text: 'a🌍b',
          selection: TextSelection.collapsed(offset: 2),
        ),
      );
      await tester.pump();
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.sourceGap);
      expect(store.queryCount, queriesBeforeSplitScalar);
      expect(controller.semanticActionsValid, isFalse);
      expect(editableKey.currentState, same(editableState));

      controller.updateLocalEditingValue(
        const TextEditingValue(
          text: 'a🌍b',
          selection: TextSelection.collapsed(offset: 1),
        ),
      );
      await tester.pump();
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(store.queryCount, queriesBeforeSplitScalar + 1);

      final framesBeforeEdits = controller.scheduledFrameCallbacks;
      final first = controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 1,
          localEndUtf16: 1,
          replacement: 'é',
          nextSelection: TextSelection.collapsed(offset: 2),
          nextComposing: TextRange(start: 1, end: 2),
        ),
      );
      expect(first.storeSynchronized, isTrue);
      final second = controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 2,
          localEndUtf16: 4,
          replacement: '',
          nextSelection: TextSelection.collapsed(offset: 2),
          nextComposing: TextRange(start: 1, end: 2),
        ),
      );
      expect(second.storeSynchronized, isTrue);
      expect(controller.scheduledFrameCallbacks, framesBeforeEdits + 1);
      expect(controller.source.toString(), 'prefix\naéb\nsuffix');
      expect(
        controller.sourceVersion.contentHash,
        FlarkV3SourceDocument.fromString('prefix\naéb\nsuffix').contentHash128,
      );
      const exactEditingValue = TextEditingValue(
        text: 'aéb',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      );
      expect(controller.editingController.value, exactEditingValue);
      expect(controller.inputIslandGlobalEndUtf16, 10);
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
        reason: 'paint changes only at a frame boundary',
      );
      expect(
        controller.semanticActionsValid,
        isFalse,
        reason: 'source authority disables old semantics synchronously',
      );

      await tester.pump();
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.stablePaint);
      expect(controller.paintState.ack, base);
      expect(find.text('stablePaint:1'), findsOneWidget);
      expect(controller.paintState.semanticActionsValid, isFalse);
      await tester.tap(
        find.byKey(const ValueKey('paint-action')),
        warnIfMissed: false,
      );
      expect(paintActions, 0);
      final semantics = tester.ensureSemantics();
      expect(find.bySemanticsLabel('Markdown action'), findsNothing);
      semantics.dispose();
      expect(editableKey.currentState, same(editableState));
      expect(controller.editingController.value, exactEditingValue);
      expect(
        store.lastQuery!.position.utf16,
        8,
        reason: 'stable paint must not query stale structure after the edit',
      );

      expect(controller.sourceWorkerSynchronized, isFalse);
      _acknowledgeAllSourceWorkerSync(controller);
      expect(controller.sourceWorkerSynchronized, isTrue);
      final currentOffer = _snapshot(controller.sourceVersion, hostRevision: 2);
      expect(controller.beginOffer(currentOffer), isA<FlarkV3HostAccepted>());
      expect(
        controller.admitPacket(
          _publicationPacket(
            offerId: currentOffer.offerId,
            firstFrameOrdinal: 0,
            firstRecordOrdinal: 0,
            recordCount: 1,
            digest: _digest(40),
            frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
          ),
        ),
        isA<FlarkV3HostAccepted>(),
      );
      expect(
        (controller.pollHost(_grant)
                as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
            .value,
        isA<FlarkV3HostPacketCredit>(),
      );
      expect(
        controller.requestCommit(_onePacketCommit(currentOffer)),
        isA<FlarkV3HostAccepted>(),
      );
      store.forcedAck = base;
      final stale = controller.pollHost(_grant);
      expect(
        (stale as FlarkV3HostRejected<FlarkV3HostPollOutcome>).rejection.reason,
        FlarkV3HostRejectReason.invalid,
      );
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.stablePaint);
      expect(controller.editingController.value, exactEditingValue);

      expect(controller.resynchronizeHost(), isA<FlarkV3HostAccepted>());
      final exact = controller.pollHost(_grant);
      final currentAck =
          (exact as FlarkV3HostAccepted<FlarkV3HostPollOutcome>).value
              as FlarkV3HostCommitted;
      expect(currentAck.ack.sourceVersion, controller.sourceVersion);
      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.stablePaint,
        reason: 'ACK adoption does not mutate paint mid-frame',
      );
      expect(controller.editingController.value, exactEditingValue);
      await tester.pump();

      expect(
        controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(controller.paintState.ack, currentAck.ack);
      expect(controller.semanticActionsValid, isTrue);
      expect(find.text('exactStructural:2'), findsOneWidget);
      expect(store.lastQuery!.position.utf16, 9);
      expect(store.lastQuery!.position.bytes, controller.source.utf16ToUtf8(9));
      final exactSemantics = tester.ensureSemantics();
      expect(
        find.bySemanticsLabel(RegExp(r'\bMarkdown action\b')),
        findsOneWidget,
      );
      exactSemantics.dispose();
      await tester.tap(find.byKey(const ValueKey('paint-action')));
      expect(paintActions, 1);
      expect(editableKey.currentState, same(editableState));
      expect(controller.editingController, same(editingController));
      expect(controller.editingController.value, exactEditingValue);

      final revisionBeforeSelection = controller.source.revision;
      final framesBeforeSelection = controller.scheduledFrameCallbacks;
      controller.updateLocalEditingValue(
        const TextEditingValue(
          text: 'aéb',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      );
      expect(controller.source.revision, revisionBeforeSelection);
      expect(controller.scheduledFrameCallbacks, framesBeforeSelection + 1);
      await tester.pump();
      expect(store.lastQuery!.position.utf16, 8);
      expect(editableKey.currentState, same(editableState));
    },
  );

  testWidgets(
    'provisional initial attach exposes exact UTF-16 UI lineage without a fake host hash',
    (tester) async {
      const text = '**live 🌍**';
      final sourceSession = FlarkV3SourceSession.fromProvisionalString(text);
      final certifiedBase = FlarkV3SourceVersion.empty(_documentSession);
      final store = _FrameModelHostStore();
      final inputIsland = FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 64,
        value: const TextEditingValue(
          text: text,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 0, end: 2),
        ),
      );
      expect(
        () => FlarkDocumentSession.attach(
          sourceSession: sourceSession,
          documentSession: _documentSession,
          hostStore: _FrameModelHostStore(),
          certifiedSourceVersion: FlarkV3SourceVersion(
            documentSession: _documentSession,
            revision: 0,
            metric: FlarkV3SourceMetric.zero,
            contentHash: const FlarkV3ContentHash128(1, 2, 3, 4),
          ),
        ),
        throwsArgumentError,
        reason: 'worker revision alone cannot certify a provisional base',
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: store,
          certifiedSourceVersion: certifiedBase,
        ),
        inputIsland: inputIsland,
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      expect(controller.source, same(sourceSession.document));
      expect(controller.source.toString(), text);
      expect(controller.uiSource.uiRevision, 1);
      expect(controller.uiSource.utf16Length, text.length);
      expect(controller.sourceVersion, certifiedBase);
      expect(sourceSession.lastCertifiedFingerprint.revision, 0);
      expect(sourceSession.lastCertifiedFingerprint.utf16Length, 0);
      expect(
        sourceSession.lastCertifiedFingerprint.contentHash128,
        FlarkV3ContentHash128.zero,
      );
      expect(store.currentSource, certifiedBase);
      expect(
        controller.documentSession.presentationState,
        isA<FlarkV3StablePendingPresentation>().having(
          (state) => state.reason,
          'reason',
          FlarkV3StablePendingReason.sourceUncertified,
        ),
      );
      expect(controller.paintState.sourceGap, isNull);
      expect(controller.paintState.uiSourceGap!.endUtf16, text.length);
      expect(controller.semanticActionsValid, isFalse);
      expect(
        controller.beginOffer(_snapshot(certifiedBase, hostRevision: 1)),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      expect(store.queryCount, 0);
    },
  );

  testWidgets(
    'deleting the last uncertified slice publishes real facts without a fake certification job',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromProvisionalString('x');
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
          certifiedSourceVersion: FlarkV3SourceVersion.empty(_documentSession),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'x',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      final edit = controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 0,
          localEndUtf16: 1,
          replacement: '',
          nextSelection: TextSelection.collapsed(offset: 0),
          nextComposing: TextRange.empty,
        ),
      );
      expect(edit.sourceApply.provisional, isTrue);
      expect(edit.provisional, isFalse);
      expect(edit.uiAdvance, isNotNull);
      expect(edit.certifiedAdoption!.storeSynchronized, isTrue);
      expect(controller.source.isFullyIndexed, isTrue);
      expect(controller.source.toString(), isEmpty);
      expect(
        controller.uiSource.bindsCertified(controller.sourceVersion),
        isTrue,
      );
      expect(controller.sourceVersion.revision, 2);
      expect(controller.sourceVersion.metric, FlarkV3SourceMetric.zero);
      expect(
        () => controller.beginSourceCertification(),
        throwsStateError,
        reason: 'there are no missing derived facts to certify',
      );
      expect(controller.sourceWorkerSynchronized, isFalse);
      _acknowledgeAllSourceWorkerSync(controller);
      expect(controller.sourceWorkerSynchronized, isTrue);
      expect(sourceSession.workerRevision, 2);
    },
  );

  testWidgets('ordinary indexed edits keep the synchronous certified path', (
    tester,
  ) async {
    final sourceSession = FlarkV3SourceSession.fromString('live');
    final controller = FlarkV3FlutterLiveController.attach(
      documentSession: _attachDocumentSession(
        sourceSession: sourceSession,
        hostStore: _FrameModelHostStore(),
      ),
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 64,
        value: const TextEditingValue(
          text: 'live',
          selection: TextSelection.collapsed(offset: 4),
        ),
      ),
      queryBudget: _queryBudget,
    );
    addTearDown(controller.dispose);

    expect(controller.sourceWorkerSynchronized, isFalse);
    final released = controller.beginSourceWorkerSync();
    expect(controller.releaseSourceWorkerSyncLease(released.leaseId), isTrue);
    expect(controller.releaseSourceWorkerSyncLease(released.leaseId), isFalse);
    _acknowledgeAllSourceWorkerSync(controller);
    expect(controller.sourceWorkerSynchronized, isTrue);

    final edit = controller.applyExactEdit(
      const FlarkV3ExactFlutterEdit(
        localStartUtf16: 4,
        localEndUtf16: 4,
        replacement: '!',
        nextSelection: TextSelection.collapsed(offset: 5),
        nextComposing: TextRange.empty,
      ),
    );
    expect(edit.provisional, isFalse);
    expect(edit.certifiedAdoption!.storeSynchronized, isTrue);
    expect(edit.uiAdvance, isNull);
    expect(
      controller.uiSource.bindsCertified(controller.sourceVersion),
      isTrue,
    );
    expect(controller.source.toString(), 'live!');
    expect(controller.source.isFullyIndexed, isTrue);
    expect(controller.sourceWorkerSynchronized, isFalse);
    expect(
      () => controller.beginSourceCertification(),
      throwsStateError,
      reason: 'indexed facts do not create a certification job',
    );
    expect(
      controller.beginOffer(
        _snapshot(controller.sourceVersion, hostRevision: 1),
      ),
      isA<FlarkV3HostRejected>().having(
        (result) => result.rejection.reason,
        'reason',
        FlarkV3HostRejectReason.closed,
      ),
    );
    _acknowledgeAllSourceWorkerSync(controller);
    expect(controller.sourceWorkerSynchronized, isTrue);
    expect(sourceSession.workerRevision, sourceSession.uiRevision);
    expect(
      sourceSession.lastCertifiedFingerprint.contentHash128,
      controller.sourceVersion.contentHash,
    );

    final active = _snapshot(controller.sourceVersion, hostRevision: 1);
    expect(controller.beginOffer(active), isA<FlarkV3HostAccepted>());
    final previousGeneration = sourceSession.workerGeneration;
    final restart = controller.restartSourceWorker();
    expect(restart.workerGeneration, previousGeneration + 1);
    expect(restart.activeOfferAbort, isA<FlarkV3HostAccepted>());
    expect(controller.sourceWorkerSynchronized, isFalse);
    expect(
      controller.beginOffer(active),
      isA<FlarkV3HostRejected>().having(
        (result) => result.rejection.reason,
        'reason',
        FlarkV3HostRejectReason.closed,
      ),
    );
    expect(
      (controller.pollHost(_grant)
              as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
          .value,
      isA<FlarkV3HostAbortComplete>(),
    );
    _acknowledgeAllSourceWorkerSync(controller);
    expect(controller.sourceWorkerSynchronized, isTrue);
    expect(controller.source, same(sourceSession.document));
  });

  testWidgets(
    'provisional edits suppress old offers, reject stale facts, and promote only exact current lineage',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString(
        '**live**',
        ordinaryReplacementUtf16Limit: 1,
      );
      final store = _FrameModelHostStore();
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: store,
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: '**live**',
            selection: TextSelection.collapsed(offset: 2),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      _acknowledgeAllSourceWorkerSync(controller);
      expect(controller.sourceWorkerSynchronized, isTrue);

      final base = _publishOnePacket(
        controller,
        _snapshot(controller.sourceVersion, hostRevision: 1),
      );
      expect(controller.acknowledgeDelivery(base), isA<FlarkV3HostAccepted>());
      await tester.pump();
      expect(controller.semanticActionsValid, isTrue);

      final oldOffer = _snapshot(controller.sourceVersion, hostRevision: 2);
      expect(controller.beginOffer(oldOffer), isA<FlarkV3HostAccepted>());
      expect(
        controller.admitPacket(
          _publicationPacket(
            offerId: oldOffer.offerId,
            firstFrameOrdinal: 0,
            firstRecordOrdinal: 0,
            recordCount: 1,
            digest: _digest(40),
            frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
          ),
        ),
        isA<FlarkV3HostAccepted>(),
      );
      expect(
        (controller.pollHost(_grant)
                as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
            .value,
        isA<FlarkV3HostPacketCredit>(),
      );
      expect(
        controller.requestCommit(_onePacketCommit(oldOffer)),
        isA<FlarkV3HostAccepted>(),
      );

      final first = controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 2,
          localEndUtf16: 2,
          replacement: 'xx',
          nextSelection: TextSelection.collapsed(offset: 4),
          nextComposing: TextRange(start: 2, end: 4),
        ),
      );
      expect(first.provisional, isTrue);
      expect(first.storeSynchronized, isFalse);
      expect(
        first.uiAdvance!.activeOfferAbort,
        isNull,
        reason:
            'the certified source observation atomically supersedes staged host state',
      );
      expect(first.certifiedAdoption, isNull);
      expect(controller.source, same(sourceSession.document));
      expect(controller.source.toString(), '**xxlive**');
      expect(controller.sourceVersion.revision, 0);
      expect(store.currentSource!.revision, 0);
      expect(controller.semanticActionsValid, isFalse);
      expect(
        controller.beginOffer(oldOffer),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      expect(
        controller.admitPacket(
          _publicationPacket(
            offerId: oldOffer.offerId,
            firstFrameOrdinal: 1,
            firstRecordOrdinal: 1,
            recordCount: 1,
            digest: _digest(41),
            frameBytes: Uint8List.fromList([0xAA]),
          ),
        ),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      expect(
        controller.requestCommit(_onePacketCommit(oldOffer)),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      expect(
        controller.documentSession.query(
          FlarkV3HostPointQuery(
            sourceVersion: controller.sourceVersion,
            position: FlarkV3SourceMetric.zero,
            budget: _queryBudget,
          ),
        ),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      expect(
        controller.resynchronizeHost(),
        isA<FlarkV3HostRejected>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.closed,
        ),
      );
      final scheduledBeforeAbortDrain = controller.scheduledFrameCallbacks;
      expect(
        controller.pollHost(_grant),
        isA<FlarkV3HostRejected<FlarkV3HostPollOutcome>>().having(
          (result) => result.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.invalid,
        ),
      );
      expect(
        controller.scheduledFrameCallbacks,
        scheduledBeforeAbortDrain,
        reason: 'offer retirement cannot adopt a presentation frame',
      );

      store.forcedAck = FlarkV3StructuralAck(
        publicationSession: oldOffer.publicationSession,
        hostRevision: oldOffer.targetHostRevision,
        sourceVersion: oldOffer.sourceVersion,
        sourceRoot: oldOffer.sourceRoot,
        parseGeneration: oldOffer.parseGeneration,
        grammarRevision: oldOffer.grammarRevision,
        syntaxProfile: oldOffer.syntaxProfile,
        authorityMask: oldOffer.authorityMask,
        recordCount: oldOffer.targetRecordCount,
        sequenceDigest: _digest(101),
        manifestDigest: _digest(201),
      );
      final oldAck = controller.pollHost(_grant);
      expect(
        (oldAck as FlarkV3HostRejected<FlarkV3HostPollOutcome>)
            .rejection
            .reason,
        FlarkV3HostRejectReason.invalid,
      );
      expect(controller.documentSession.pendingDeliveryAck, isNull);

      expect(controller.sourceWorkerSynchronized, isFalse);
      _acknowledgeAllSourceWorkerSync(controller);
      expect(controller.sourceWorkerSynchronized, isTrue);
      expect(sourceSession.workerRevision, 1);
      expect(controller.source.isFullyIndexed, isFalse);
      expect(controller.sourceVersion.revision, 0);
      final staleRequest = controller.beginSourceCertification();
      final staleReceipt = FlarkV3SourceCertificationReceipt.scan(
        staleRequest,
        sourceReplica: controller.source,
      );
      final queriesBeforeSelection = store.queryCount;
      controller.updateLocalEditingValue(
        const TextEditingValue(
          text: '**xxlive**',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 2, end: 4),
        ),
      );
      await tester.pump();
      expect(sourceSession.uiRevision, 1);
      expect(store.queryCount, queriesBeforeSelection);
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.stablePaint);
      expect(controller.paintState.sourceGap, isNull);
      expect(controller.paintState.uiSourceGap!.endUtf16, 10);
      expect(controller.paintState.ack, base);

      final second = controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 4,
          localEndUtf16: 4,
          replacement: 'z',
          nextSelection: TextSelection.collapsed(offset: 5),
          nextComposing: TextRange(start: 2, end: 5),
        ),
      );
      expect(second.provisional, isTrue);
      expect(sourceSession.uiRevision, 2);
      expect(controller.source, same(sourceSession.document));
      final stale = controller.applySourceCertification(staleReceipt);
      expect(stale.promoted, isFalse);
      expect(controller.sourceVersion.revision, 0);
      expect(controller.uiSource.uiRevision, 2);
      expect(controller.semanticActionsValid, isFalse);

      expect(controller.sourceWorkerSynchronized, isFalse);
      _acknowledgeAllSourceWorkerSync(controller);
      expect(controller.sourceWorkerSynchronized, isTrue);
      final workerRevisionBeforeFactPromotion = sourceSession.workerRevision;
      final currentRequest = controller.beginSourceCertification();
      final currentReceipt = FlarkV3SourceCertificationReceipt.scan(
        currentRequest,
        sourceReplica: controller.source,
      );
      final current = controller.applySourceCertification(currentReceipt);
      expect(current.promoted, isTrue);
      expect(
        sourceSession.workerRevision,
        workerRevisionBeforeFactPromotion,
        reason: 'fact promotion never acknowledges source-replica credit',
      );
      expect(current.hostAdoption!.storeSynchronized, isTrue);
      expect(controller.source, same(sourceSession.document));
      expect(controller.source.isFullyIndexed, isTrue);
      expect(controller.source.toString(), '**xxzlive**');
      expect(
        controller.uiSource.bindsCertified(controller.sourceVersion),
        isTrue,
      );
      expect(controller.sourceVersion.revision, 2);
      expect(store.currentSource, controller.sourceVersion);
      expect(
        controller.sourceVersion.contentHash,
        FlarkV3SourceDocument.fromString('**xxzlive**').contentHash128,
      );
      expect(
        controller.beginOffer(
          _snapshot(controller.sourceVersion, hostRevision: 3),
        ),
        isA<FlarkV3HostAccepted>(),
      );
    },
  );

  testWidgets(
    'old delivery ACK drains as housekeeping during a provisional UI gap',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString(
        'live',
        ordinaryReplacementUtf16Limit: 1,
      );
      final store = _FrameModelHostStore();
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: store,
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'live',
            selection: TextSelection.collapsed(offset: 4),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      _acknowledgeAllSourceWorkerSync(controller);

      final oldAck = _publishOnePacket(
        controller,
        _snapshot(controller.sourceVersion, hostRevision: 1),
      );
      await tester.pump();
      expect(controller.semanticActionsValid, isTrue);
      expect(controller.documentSession.pendingDeliveryAck, oldAck);

      final edit = controller.applyExactEdit(
        const FlarkV3ExactFlutterEdit(
          localStartUtf16: 4,
          localEndUtf16: 4,
          replacement: '!!',
          nextSelection: TextSelection.collapsed(offset: 6),
          nextComposing: TextRange.empty,
        ),
      );
      expect(edit.provisional, isTrue);
      expect(edit.uiAdvance!.activeOfferAbort, isNull);
      expect(controller.semanticActionsValid, isFalse);
      final scheduledBeforeRetirement = controller.scheduledFrameCallbacks;
      expect(
        controller.acknowledgeDelivery(oldAck),
        isA<FlarkV3HostAccepted>(),
      );
      expect(controller.documentSession.pendingDeliveryAck, isNull);
      expect(controller.scheduledFrameCallbacks, scheduledBeforeRetirement);
      expect(controller.semanticActionsValid, isFalse);

      await tester.pump();
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.stablePaint);
      expect(controller.paintState.ack, oldAck);
      expect(controller.paintState.sourceGap, isNull);
      expect(controller.paintState.uiSourceGap!.endUtf16, 6);
      expect(controller.paintState.semanticActionsValid, isFalse);
    },
  );

  testWidgets('bounded host query SourceGap fails closed without remounting', (
    tester,
  ) async {
    final sourceSession = FlarkV3SourceSession.fromString('> deeply nested');
    final store = _FrameModelHostStore()..forceSourceGap = true;
    final controller = FlarkV3FlutterLiveController.attach(
      documentSession: _attachDocumentSession(
        sourceSession: sourceSession,
        hostStore: store,
      ),
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 64,
        value: const TextEditingValue(
          text: '> deeply nested',
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange(start: 2, end: 8),
        ),
      ),
      queryBudget: _queryBudget,
    );
    addTearDown(controller.dispose);
    final editableKey = GlobalKey<EditableTextState>();

    _acknowledgeAllSourceWorkerSync(controller);

    await tester.pumpWidget(
      _testApp(
        FlarkV3LiveEditorPrototype(
          controller: controller,
          editableKey: editableKey,
          paintLayerBuilder: (context, state) => Text(state.mode.name),
        ),
      ),
    );
    await tester.pump();
    final editableState = editableKey.currentState;
    final before = controller.editingController.value;

    _publishOnePacket(
      controller,
      _snapshot(controller.sourceVersion, hostRevision: 1),
    );
    await tester.pump();

    expect(controller.paintState.mode, FlarkV3FlutterPaintMode.sourceGap);
    expect(
      controller.paintState.sourceGap!.structuralReason,
      FlarkV3HostSourceGapReason.openDepthLimit,
    );
    expect(
      controller.paintState.sourceGap!.structuralReceipt!.openDepth,
      _queryBudget.maxOpenDepth,
    );
    expect(
      controller.paintState.sourceGap!.range.start,
      FlarkV3SourceMetric.zero,
    );
    expect(
      controller.paintState.sourceGap!.range.end,
      controller.sourceVersion.metric,
    );
    expect(controller.semanticActionsValid, isFalse);
    expect(controller.editingController.value, before);
    expect(editableKey.currentState, same(editableState));
  });

  testWidgets(
    'bulk paste advances exact provisional source then installs only a bounded island',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('ab');
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'ab',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final replacement = 'x' * 100000;
      final caret = 1 + replacement.length;

      final receipt = controller.applyBulkEditAndHandoff(
        FlarkV3BulkFlutterEdit(
          localStartUtf16: 1,
          localEndUtf16: 1,
          replacement: replacement,
          nextGlobalEditingState: FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: caret),
            composing: TextRange.empty,
          ),
        ),
      );

      expect(receipt.provisional, isTrue);
      expect(receipt.sourceApply.sourceWork.replacementUtf8BytesEncoded, 0);
      expect(receipt.sourceApply.sourceWork.replacementChunksEncoded, 0);
      expect(controller.source.utf16Length, 100002);
      expect(controller.source.readRange(0, 2), 'ax');
      expect(controller.source.readRange(99999, 100002), 'xxb');
      expect(controller.editingController.text.length, lessThanOrEqualTo(64));
      expect(
        controller.editingController.text,
        controller.source.readRange(
          controller.inputIslandGlobalStartUtf16,
          controller.inputIslandGlobalEndUtf16,
        ),
      );
      expect(controller.globalEditingState.selection.extentOffset, caret);
      expect(
        controller.editingController.selection.extentOffset,
        caret - controller.inputIslandGlobalStartUtf16,
      );
      expect(controller.semanticActionsValid, isFalse);
      await tester.pump();
      expect(controller.paintState.mode, FlarkV3FlutterPaintMode.sourceGap);
    },
  );

  testWidgets(
    'typed Flutter deltas route ordinary changes exactly and giant insertion to bulk handoff',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('ab');
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'ab',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      controller.applyTextEditingDelta(
        const TextEditingDeltaInsertion(
          oldText: 'ab',
          textInserted: 'x',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      );
      expect(controller.source.toString(), 'axb');
      controller.applyTextEditingDelta(
        const TextEditingDeltaReplacement(
          oldText: 'axb',
          replacementText: 'YZ',
          replacedRange: TextRange(start: 1, end: 2),
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      );
      expect(controller.source.toString(), 'aYZb');
      controller.applyTextEditingDelta(
        const TextEditingDeltaDeletion(
          oldText: 'aYZb',
          deletedRange: TextRange(start: 1, end: 3),
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      );
      expect(controller.source.toString(), 'ab');
      expect(
        controller.applyTextEditingDelta(
          const TextEditingDeltaNonTextUpdate(
            oldText: 'ab',
            selection: TextSelection.collapsed(offset: 2),
            composing: TextRange.empty,
          ),
        ),
        isNull,
      );
      expect(controller.globalEditingState.selection.extentOffset, 2);

      final giant = 'g' * 100000;
      final receipt = controller.applyTextEditingDelta(
        TextEditingDeltaInsertion(
          oldText: 'ab',
          textInserted: giant,
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 1 + giant.length),
          composing: TextRange.empty,
        ),
      );
      expect(receipt, isNotNull);
      expect(receipt!.provisional, isTrue);
      expect(receipt.sourceApply.sourceWork.replacementUtf8BytesEncoded, 0);
      expect(controller.editingController.text.length, lessThanOrEqualTo(64));

      final revision = sourceSession.uiRevision;
      expect(
        () => controller.applyTextEditingDelta(
          const TextEditingDeltaInsertion(
            oldText: 'stale',
            textInserted: '!',
            insertionOffset: 0,
            selection: TextSelection.collapsed(offset: 1),
            composing: TextRange.empty,
          ),
        ),
        throwsStateError,
      );
      expect(sourceSession.uiRevision, revision);
      await tester.pump();
    },
  );

  testWidgets(
    'cross-island selection remains global while EditableText gets an extent proxy',
    (tester) async {
      final sourceText = 'a' * 20000;
      final sourceSession = FlarkV3SourceSession.fromString(sourceText);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text:
                'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
            selection: TextSelection.collapsed(offset: 10),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final revision = sourceSession.uiRevision;
      const global = FlarkV3GlobalEditingState(
        selection: TextSelection(baseOffset: 0, extentOffset: 19000),
        composing: TextRange.empty,
      );

      final handoff = controller.handoffInputIsland(global);

      expect(handoff.selectionSpansOutsideIsland, isTrue);
      expect(sourceSession.uiRevision, revision);
      expect(controller.globalEditingState.selection, global.selection);
      expect(controller.inputIslandGlobalStartUtf16, greaterThan(0));
      expect(controller.inputIslandGlobalEndUtf16, greaterThanOrEqualTo(19000));
      expect(controller.editingController.selection.isCollapsed, isTrue);
      expect(
        controller.editingController.selection.extentOffset,
        19000 - controller.inputIslandGlobalStartUtf16,
      );
      expect(controller.editingController.text.length, 64);
      await tester.pump();
    },
  );

  testWidgets(
    'exact parser range handoff keeps the input client and projected extent proxy',
    (tester) async {
      const sourceText = 'left\n\n**x**';
      final sourceSession = FlarkV3SourceSession.fromString(sourceText);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'left',
            selection: TextSelection.collapsed(offset: 4),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final editableKey = GlobalKey<EditableTextState>();

      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      final editableState = editableKey.currentState!;
      final editingController = controller.editingController;
      editableState.requestKeyboard();
      await tester.pump();
      final setClientCallsBefore = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList();
      final clientId =
          (setClientCallsBefore.last.arguments as List<dynamic>).first as int;
      const global = FlarkV3GlobalEditingState(
        selection: TextSelection(baseOffset: 1, extentOffset: 9),
        composing: TextRange.empty,
      );
      final revision = sourceSession.uiRevision;

      final handoff = controller.handoffInputIslandToExactRange(
        startUtf16: 6,
        endUtf16: 11,
        nextGlobalEditingState: global,
      );
      await tester.pump();

      expect(handoff.currentStartUtf16, 6);
      expect(handoff.currentEndUtf16, 11);
      expect(handoff.selectionSpansOutsideIsland, isTrue);
      expect(controller.editingController.text, '**x**');
      expect(
        controller.editingController.selection,
        const TextSelection.collapsed(offset: 3),
      );
      expect(controller.globalEditingState.selection, global.selection);
      expect(sourceSession.uiRevision, revision);

      controller.adoptInlineIslandPresentation(
        _inlinePresentation(
          sourceDocument: controller.source,
          sourceVersion: controller.sourceVersion,
          disposition: FlarkV3InlineFactsDisposition.authoritative,
          leafStartUtf16: 6,
          leafEndUtf16: 11,
          records: [
            _inlineRecord(
              kind: 2,
              start: 0,
              length: 5,
              contentStart: 2,
              contentLength: 1,
            ),
          ],
        ),
      );
      await tester.pump();

      expect(controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(controller.editingController.text, 'x');
      expect(
        controller.editingController.selection,
        const TextSelection.collapsed(offset: 1),
      );
      expect(controller.globalEditingState.selection, global.selection);
      expect(controller.hasCertifiedInlinePresentation, isTrue);
      final setClientCallsAfter = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList();
      expect(setClientCallsAfter, hasLength(setClientCallsBefore.length));
      expect(
        (setClientCallsAfter.last.arguments as List<dynamic>).first,
        clientId,
      );
    },
  );

  test(
    'exact parser range handoff enforces sealed edges, extent, and composition',
    () {
      const sourceText = 'a🌍b\r\ncdefgh';
      final sourceSession = FlarkV3SourceSession.fromString(sourceText);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 5,
          value: const TextEditingValue(
            text: 'a🌍b',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final before = controller.editingController.value;

      expect(
        () => controller.handoffInputIslandToExactRange(
          startUtf16: 2,
          endUtf16: 4,
          nextGlobalEditingState: const FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: 3),
            composing: TextRange.empty,
          ),
        ),
        throwsRangeError,
        reason: 'the start splits the UTF-16 scalar pair',
      );
      expect(
        () => controller.handoffInputIslandToExactRange(
          startUtf16: 3,
          endUtf16: 5,
          nextGlobalEditingState: const FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: 4),
            composing: TextRange.empty,
          ),
        ),
        throwsRangeError,
        reason: 'the end splits CRLF',
      );
      expect(
        () => controller.handoffInputIslandToExactRange(
          startUtf16: 0,
          endUtf16: 6,
          nextGlobalEditingState: const FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: 4),
            composing: TextRange.empty,
          ),
        ),
        throwsRangeError,
        reason: 'the exact leaf exceeds the controller sealed bound',
      );
      expect(
        () => controller.handoffInputIslandToExactRange(
          startUtf16: 6,
          endUtf16: 10,
          nextGlobalEditingState: const FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: 5),
            composing: TextRange.empty,
          ),
        ),
        throwsStateError,
        reason: 'the active range must contain the global extent',
      );
      expect(
        () => controller.handoffInputIslandToExactRange(
          startUtf16: 6,
          endUtf16: 10,
          nextGlobalEditingState: const FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: 8),
            composing: TextRange(start: 4, end: 7),
          ),
        ),
        throwsStateError,
        reason: 'composition cannot straddle the exact active range',
      );
      expect(controller.editingController.value, before);
      expect(controller.inputIslandGlobalStartUtf16, 0);

      const composingSource = '0123456789';
      final composingSession = FlarkV3SourceSession.fromString(composingSource);
      final composingController = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: composingSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 2,
          maximumUtf16: 6,
          value: const TextEditingValue(
            text: '23456',
            selection: TextSelection.collapsed(offset: 2),
            composing: TextRange(start: 1, end: 3),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(composingController.dispose);

      composingController.handoffInputIslandToExactRange(
        startUtf16: 1,
        endUtf16: 6,
        nextGlobalEditingState: const FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange(start: 3, end: 5),
        ),
      );

      expect(composingController.editingController.text, '12345');
      expect(
        composingController.editingController.value.composing,
        const TextRange(start: 2, end: 4),
      );
      expect(composingController.editingController.text.substring(2, 4), '34');
    },
  );

  testWidgets(
    'island handoff preserves active IME text and global composing offsets',
    (tester) async {
      final sourceText = List.generate(200, (index) => '${index % 10}').join();
      final sourceSession = FlarkV3SourceSession.fromString(sourceText);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 50,
          maximumUtf16: 64,
          value: TextEditingValue(
            text: sourceText.substring(50, 114),
            selection: const TextSelection.collapsed(offset: 15),
            composing: const TextRange(start: 10, end: 15),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      const global = FlarkV3GlobalEditingState(
        selection: TextSelection.collapsed(offset: 62),
        composing: TextRange(start: 60, end: 65),
      );
      final composingText = sourceText.substring(60, 65);

      controller.handoffInputIsland(global);

      expect(controller.globalEditingState.composing, global.composing);
      expect(
        controller.editingController.text.substring(
          controller.editingController.value.composing.start,
          controller.editingController.value.composing.end,
        ),
        composingText,
      );
      final before = controller.editingController.value;
      final startBefore = controller.inputIslandGlobalStartUtf16;
      expect(
        () => controller.handoffInputIsland(
          const FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: 62),
            composing: TextRange(start: 61, end: 65),
          ),
        ),
        throwsStateError,
      );
      expect(controller.editingController.value, before);
      expect(controller.inputIslandGlobalStartUtf16, startBefore);
      await tester.pump();
    },
  );

  testWidgets(
    'island handoff never splits scalar or CRLF boundaries across every caret cut',
    (tester) async {
      final sourceText = List.filled(80, 'a🌍\r\n').join();
      final sourceSession = FlarkV3SourceSession.fromString(sourceText);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 17,
          value: TextEditingValue(
            text: sourceText.substring(0, 15),
            selection: const TextSelection.collapsed(offset: 0),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      for (var extent = 0; extent <= sourceText.length; extent += 1) {
        controller.handoffInputIsland(
          FlarkV3GlobalEditingState(
            selection: TextSelection(baseOffset: 0, extentOffset: extent),
            composing: TextRange.empty,
          ),
        );
        final start = controller.inputIslandGlobalStartUtf16;
        final end = controller.inputIslandGlobalEndUtf16;
        expect(_isSafeBoundary(sourceText, start), isTrue);
        expect(_isSafeBoundary(sourceText, end), isTrue);
        expect(controller.editingController.text.length, lessThanOrEqualTo(17));
        expect(
          controller.editingController.text,
          sourceText.substring(start, end),
        );
        expect(controller.globalEditingState.selection.extentOffset, extent);
      }
      await tester.pump();
    },
  );

  testWidgets(
    'impossible bulk composition is rejected before source or island mutation',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('ab');
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'ab',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      final revision = sourceSession.uiRevision;
      final before = controller.editingController.value;

      expect(
        () => controller.applyBulkEditAndHandoff(
          FlarkV3BulkFlutterEdit(
            localStartUtf16: 1,
            localEndUtf16: 1,
            replacement: 'x' * 1000,
            nextGlobalEditingState: const FlarkV3GlobalEditingState(
              selection: TextSelection.collapsed(offset: 100),
              composing: TextRange(start: 1, end: 100),
            ),
          ),
        ),
        throwsStateError,
      );
      expect(sourceSession.uiRevision, revision);
      expect(controller.source.toString(), 'ab');
      expect(controller.editingController.value, before);
    },
  );

  testWidgets('bulk handoff emits a warmed size-scaling mechanism receipt', (
    tester,
  ) async {
    final sourceSession = FlarkV3SourceSession.fromString('ab');
    final controller = FlarkV3FlutterLiveController.attach(
      documentSession: _attachDocumentSession(
        sourceSession: sourceSession,
        hostStore: _FrameModelHostStore(),
      ),
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 64,
        value: const TextEditingValue(
          text: 'ab',
          selection: TextSelection.collapsed(offset: 1),
        ),
      ),
      queryBudget: _queryBudget,
    );
    addTearDown(controller.dispose);
    for (final size in [10000, 100000, 1000000, 10000000, 10000000]) {
      final replacement = 'z' * size;
      // Force the backing before the measured controller call.
      expect(replacement.codeUnitAt(0), 0x7A);
      expect(replacement.codeUnitAt(replacement.length - 1), 0x7A);
      final localCaret = controller.editingController.selection.extentOffset;
      final globalCaret =
          controller.inputIslandGlobalStartUtf16 + localCaret + size;
      final stopwatch = Stopwatch()..start();
      final receipt = controller.applyBulkEditAndHandoff(
        FlarkV3BulkFlutterEdit(
          localStartUtf16: localCaret,
          localEndUtf16: localCaret,
          replacement: replacement,
          nextGlobalEditingState: FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: globalCaret),
            composing: TextRange.empty,
          ),
        ),
      );
      stopwatch.stop();
      expect(receipt.provisional, isTrue);
      expect(receipt.sourceApply.sourceWork.replacementUtf8BytesEncoded, 0);
      expect(controller.editingController.text.length, lessThanOrEqualTo(64));
      // ignore: avoid_print
      print(
        'flark_v3_bulk_scaling size=$size us=${stopwatch.elapsedMicroseconds}',
      );
    }
  });

  testWidgets(
    'random ordinary and bulk deltas remain exact across repeated island rebases',
    (tester) async {
      var oracle = List.filled(300, '0123456789').join();
      final sourceSession = FlarkV3SourceSession.fromString(oracle);
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: _FrameModelHostStore(),
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 500,
          maximumUtf16: 64,
          value: TextEditingValue(
            text: oracle.substring(500, 564),
            selection: const TextSelection.collapsed(offset: 32),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);
      var random = 0x5EED;
      int nextInt(int ceiling) {
        random = (random * 1103515245 + 12345) & 0x7FFFFFFF;
        return random % ceiling;
      }

      for (var iteration = 0; iteration < 160; iteration += 1) {
        final oldIsland = controller.editingController.text;
        final first = nextInt(oldIsland.length + 1);
        final second = nextInt(oldIsland.length + 1);
        final start = first < second ? first : second;
        final end = first < second ? second : first;
        final replacement = iteration % 29 == 0
            ? 'B' * 10000
            : ['x', '', 'yz', '\r\n', 'q' * 9][nextInt(5)];
        final globalStart = controller.inputIslandGlobalStartUtf16 + start;
        final globalEnd = controller.inputIslandGlobalStartUtf16 + end;
        final globalCaret = globalStart + replacement.length;
        oracle = oracle.replaceRange(globalStart, globalEnd, replacement);

        controller.applyTextEditingDelta(
          TextEditingDeltaReplacement(
            oldText: oldIsland,
            replacementText: replacement,
            replacedRange: TextRange(start: start, end: end),
            selection: TextSelection.collapsed(
              offset: start + replacement.length,
            ),
            composing: TextRange.empty,
          ),
        );

        expect(controller.source.utf16Length, oracle.length);
        expect(
          controller.globalEditingState.selection.extentOffset,
          globalCaret,
        );
        expect(controller.editingController.text.length, lessThanOrEqualTo(64));
        expect(
          controller.editingController.text,
          oracle.substring(
            controller.inputIslandGlobalStartUtf16,
            controller.inputIslandGlobalEndUtf16,
          ),
        );
        if (iteration % 31 == 0) {
          expect(controller.source.toString(), oracle);
        }
      }
      expect(controller.source.toString(), oracle);
      await tester.pump();
    },
  );

  testWidgets(
    'sealed foreground envelope rejects document-sized island and query configuration',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('x');
      expect(
        () => FlarkV3FlutterLiveController.attach(
          documentSession: _attachDocumentSession(
            sourceSession: sourceSession,
            hostStore: _FrameModelHostStore(),
          ),
          inputIsland: FlarkV3InputIslandSnapshot(
            globalStartUtf16: 0,
            maximumUtf16:
                FlarkV3FlutterForegroundProfile
                    .prototype
                    .maximumInputIslandUtf16 +
                1,
            value: const TextEditingValue(
              text: 'x',
              selection: TextSelection.collapsed(offset: 1),
            ),
          ),
          queryBudget: _queryBudget,
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3FlutterLiveController.attach(
          documentSession: _attachDocumentSession(
            sourceSession: sourceSession,
            hostStore: _FrameModelHostStore(),
          ),
          inputIsland: FlarkV3InputIslandSnapshot(
            globalStartUtf16: 0,
            maximumUtf16: 64,
            value: const TextEditingValue(
              text: 'x',
              selection: TextSelection.collapsed(offset: 1),
            ),
          ),
          queryBudget: FlarkV3HostQueryBudget(
            maxEncodedBytes:
                FlarkV3FlutterForegroundProfile
                    .prototype
                    .maximumQueryEncodedBytes +
                1,
            maxOpenDepth: 1,
            maxLeafCount: 1,
            maxTreeNodesVisited: 1,
          ),
        ),
        throwsArgumentError,
      );
    },
  );

  testWidgets('sealed platform batch work rejects before source mutation', (
    tester,
  ) async {
    final source = 'x' * (8 * 1024);
    final sourceSession = FlarkV3SourceSession.fromString(source);
    final controller = FlarkV3FlutterLiveController.attach(
      documentSession: _attachDocumentSession(
        sourceSession: sourceSession,
        hostStore: _FrameModelHostStore(),
      ),
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: source.length,
        value: TextEditingValue(
          text: source,
          selection: const TextSelection.collapsed(offset: 0),
        ),
      ),
      queryBudget: _queryBudget,
    );
    addTearDown(controller.dispose);
    final deltas = List<TextEditingDelta>.generate(
      9,
      (_) => TextEditingDeltaNonTextUpdate(
        oldText: source,
        selection: const TextSelection.collapsed(offset: 0),
        composing: TextRange.empty,
      ),
    );

    expect(() => controller.applyTextEditingDeltas(deltas), throwsStateError);
    expect(sourceSession.uiRevision, 0);
    expect(controller.source.toString(), source);
    expect(controller.editingController.text, source);
  });

  testWidgets(
    'ordinary edit and host poll reject oversized foreground work before mutation',
    (tester) async {
      final sourceSession = FlarkV3SourceSession.fromString('x');
      final store = _FrameModelHostStore();
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: _attachDocumentSession(
          sourceSession: sourceSession,
          hostStore: store,
        ),
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: 'x',
            selection: TextSelection.collapsed(offset: 1),
          ),
        ),
        queryBudget: _queryBudget,
      );
      addTearDown(controller.dispose);

      final revision = sourceSession.uiRevision;
      expect(
        () => controller.applyExactEdit(
          FlarkV3ExactFlutterEdit(
            localStartUtf16: 1,
            localEndUtf16: 1,
            replacement:
                'x' *
                (FlarkV3FlutterForegroundProfile
                        .prototype
                        .maximumOrdinaryReplacementUtf16 +
                    1),
            nextSelection: const TextSelection.collapsed(offset: 1),
            nextComposing: TextRange.empty,
          ),
        ),
        throwsStateError,
      );
      expect(sourceSession.uiRevision, revision);
      expect(controller.editingController.text, 'x');

      final result = controller.pollHost(
        FlarkV3HostWorkGrant(
          inspectBytes:
              FlarkV3FlutterForegroundProfile
                  .prototype
                  .maximumHostInspectBytes +
              1,
          copyBytes: 0,
          transitions: 0,
        ),
      );
      expect(
        result,
        isA<FlarkV3HostRejected<FlarkV3HostPollOutcome>>().having(
          (rejected) => rejected.rejection.reason,
          'reason',
          FlarkV3HostRejectReason.foregroundBoundExceeded,
        ),
      );
      expect(store.pollCount, 0);
    },
  );
}

Widget _testApp(Widget child) => Directionality(
  textDirection: TextDirection.ltr,
  child: Center(child: SizedBox(width: 400, child: child)),
);

final class _ExactBaseDeltaExecutorFake {
  const _ExactBaseDeltaExecutorFake({
    required this.documentSession,
    required this.sourceSession,
    required this.onProgress,
  });

  final FlarkDocumentSession documentSession;
  final FlarkV3SourceSession sourceSession;
  final VoidCallback onProgress;

  FlarkV3StructuralAck certifyAndPublishBase(String source) {
    _synchronizeSourceWorker();
    _installCanonicalFacts(source, requestId: 91);
    onProgress();
    final ack = _publishOnSession(
      documentSession,
      _snapshot(documentSession.sourceVersion, hostRevision: 1),
    );
    expect(
      documentSession.acknowledgeDelivery(ack),
      isA<FlarkV3HostAccepted>(),
    );
    onProgress();
    return ack;
  }

  FlarkV3StructuralAck promoteAndPublishDelta(
    String targetSource, {
    required FlarkV3StructuralAck base,
  }) {
    _synchronizeSourceWorker();
    final baseAuthority = documentSession.retainedCanonicalSourceFactDeltaBase!;
    final lineage = _lineage(requestId: 92);
    final targetFacts = _canonicalFacts(targetSource);
    final rootGuard = _portableFactsHash(targetFacts);

    expect(
      documentSession
          .beginCanonicalSourceFactDelta(
            FlarkV3CanonicalSourceFactDelta(
              lineage: lineage,
              baseAuthority: baseAuthority,
              baseFingerprint: baseAuthority.fingerprint,
              baseCheckpointRootGuard128: baseAuthority.checkpointHash128,
              baseCheckpointCount: baseAuthority.checkpointCount,
              basePageCount: baseAuthority.pageCount,
              baseCheckpointSpacingUtf16: baseAuthority.checkpointSpacingUtf16,
              basePageStart: 0,
              basePageEnd: 1,
              targetPageStart: 0,
              targetPageEnd: 1,
              targetCheckpointCount: targetFacts.length,
              targetPageCount: 1,
              targetCheckpointRootGuardAlgorithm:
                  flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
              targetCheckpointRootGuard128: rootGuard,
              replacementCheckpointCount: targetFacts.length,
            ),
          )
          .disposition,
      FlarkV3CanonicalSourceFactDeltaBeginDisposition.accepted,
    );
    onProgress();

    expect(
      documentSession
          .stageCanonicalSourceFactDeltaCheckpointPage(
            FlarkV3CanonicalSourceFactDeltaCheckpointPage(
              lineage: lineage,
              pageOrdinal: 0,
              checkpoints: targetFacts,
            ),
          )
          .disposition,
      FlarkV3SourceFactStageDisposition.staged,
    );
    onProgress();

    final oracle = FlarkV3SourceDocument.fromString(targetSource);
    expect(
      documentSession
          .commitCanonicalSourceFactDeltaCertification(
            FlarkV3CanonicalSourceFactDeltaCompletion(
              lineage: lineage,
              fingerprintAlgorithm: 1,
              fingerprint: FlarkV3SourceFingerprint(
                revision: sourceSession.uiRevision,
                utf16Length: targetSource.length,
                utf8Length: utf8.encode(targetSource).length,
                contentHash128: oracle.contentHash128,
              ),
              logicalLineBreaks: 0,
              checkpointSpacingUtf16: 4,
              checkpointCount: targetFacts.length,
              pageCount: 1,
              checkpointRootGuardAlgorithm:
                  flarkV3CanonicalSourceFactDeltaRootGuardAlgorithm,
              checkpointRootGuard128: rootGuard,
              replacementCheckpointHash128: rootGuard,
            ),
          )
          .disposition,
      FlarkV3SourcePromotionDisposition.promoted,
    );
    onProgress();

    final target = _publishOnSession(
      documentSession,
      _exactDeltaOffer(documentSession.sourceVersion, base: base),
    );
    onProgress();
    return target;
  }

  void _installCanonicalFacts(String source, {required int requestId}) {
    final lineage = _lineage(requestId: requestId);
    final facts = _canonicalFacts(source);
    expect(
      documentSession
          .stageCanonicalSourceFactCheckpointPage(
            FlarkV3CanonicalSourceFactCheckpointPage(
              lineage: lineage,
              pageOrdinal: 0,
              pageCount: 1,
              checkpointCount: facts.length,
              checkpointSpacingUtf16: 4,
              checkpoints: facts,
            ),
          )
          .disposition,
      FlarkV3SourceFactStageDisposition.staged,
    );
    final oracle = FlarkV3SourceDocument.fromString(source);
    expect(
      documentSession
          .commitCanonicalSourceFactCertification(
            FlarkV3CanonicalSourceFactCompletion(
              lineage: lineage,
              fingerprintAlgorithm: 1,
              fingerprint: FlarkV3SourceFingerprint(
                revision: sourceSession.uiRevision,
                utf16Length: source.length,
                utf8Length: utf8.encode(source).length,
                contentHash128: oracle.contentHash128,
              ),
              logicalLineBreaks: 0,
              checkpointSpacingUtf16: 4,
              checkpointCount: facts.length,
              pageCount: 1,
              checkpointHash128: _portableFactsHash(facts),
            ),
          )
          .promoted,
      isTrue,
    );
  }

  void _synchronizeSourceWorker() {
    while (sourceSession.hasPendingWorkerSync) {
      final lease = sourceSession.beginWorkerSync();
      final observed = _observedForSourceSession(sourceSession, lease);
      final receipt = sourceSession.acknowledgeWorkerSync(switch (lease) {
        FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
          observedReplica: lease.isLast ? observed : null,
        ),
        FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
          observedReplica: observed,
        ),
      });
      expect(
        receipt.disposition,
        FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
      );
    }
  }

  FlarkV3SourceCertificationLineage _lineage({required int requestId}) {
    final observed = sourceSession.observedWorkerReplica;
    return FlarkV3SourceCertificationLineage(
      sourceSessionIdentity: sourceSession.sourceSessionIdentity,
      requestId: requestId,
      workerGeneration: sourceSession.workerGeneration,
      workerReplicaRevision: observed.revision,
      uiRevision: sourceSession.uiRevision,
      utf16Length: sourceSession.document.utf16Length,
      intentHighWater: observed.intentHighWater,
    );
  }
}

FlarkV3InlineIslandPresentation _inlinePresentation({
  required FlarkV3SourceDocument sourceDocument,
  required FlarkV3SourceVersion sourceVersion,
  required FlarkV3InlineFactsDisposition disposition,
  List<Uint8List> records = const [],
  int leafStartUtf16 = 0,
  int? leafEndUtf16,
}) {
  final endUtf16 = leafEndUtf16 ?? sourceDocument.utf16Length;
  final leaf = FlarkV3SourceSpan(
    startUtf8: sourceDocument.utf16ToUtf8(leafStartUtf16),
    endUtf8: sourceDocument.utf16ToUtf8(endUtf16),
    startUtf16: leafStartUtf16,
    endUtf16: endUtf16,
  );
  final facts = FlarkV3InlineFactsDecoder.decode(
    sourceDocument: sourceDocument,
    expectedSource: sourceVersion,
    factSource: sourceVersion,
    expectedProfilePartition: 1,
    profilePartition: 1,
    expectedLeaf: leaf,
    factLeaf: leaf,
    disposition: disposition,
    factCount: records.length,
    encodedFacts: Uint8List.fromList([for (final record in records) ...record]),
  );
  return FlarkV3InlineIslandPresentation.resolve(
    sourceDocument: sourceDocument,
    expectedSource: sourceVersion,
    structuralQuery: FlarkV3DocumentStructuralQuery(
      sourceRevision: sourceVersion.revision,
      structureRevision: sourceVersion.revision,
      structure: FlarkV3DocumentStructure(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: leaf,
        visibleSource: leaf,
        referenceDefinitionCount: 0,
      ),
      projection: FlarkV3DocumentProjection(
        kind: FlarkV3DocumentStructureKind.paragraph,
        source: leaf,
        projectedSource: leaf,
        runCount: 1,
      ),
      inlineFacts: facts,
    ),
    activeIsland: leaf,
  );
}

Uint8List _inlineRecord({
  required int kind,
  required int start,
  required int length,
  required int contentStart,
  required int contentLength,
}) {
  final bytes = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  ByteData.sublistView(bytes)
    ..setUint8(0, kind)
    ..setUint8(1, 0)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, start, Endian.little)
    ..setUint32(8, length, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, contentLength, Endian.little);
  return bytes;
}

bool _isSafeBoundary(String source, int offset) {
  if (offset == 0 || offset == source.length) return true;
  final previous = source.codeUnitAt(offset - 1);
  final next = source.codeUnitAt(offset);
  final splitsScalar =
      previous >= 0xD800 &&
      previous <= 0xDBFF &&
      next >= 0xDC00 &&
      next <= 0xDFFF;
  return !splitsScalar && !(previous == 0x0D && next == 0x0A);
}

FlarkDocumentSession _attachDocumentSession({
  required FlarkV3SourceSession sourceSession,
  required FlarkV3HostStore hostStore,
  FlarkV3SourceVersion? certifiedSourceVersion,
}) {
  final session = FlarkDocumentSession.attach(
    sourceSession: sourceSession,
    documentSession: _documentSession,
    hostStore: hostStore,
    certifiedSourceVersion: certifiedSourceVersion,
    workProfile: FlarkV3FlutterForegroundProfile.prototype.documentWorkProfile,
  );
  addTearDown(session.close);
  return session;
}

final _documentSession = FlarkV3DocumentSessionId(1, 2, 3, 4);
final _publicationSession = FlarkV3PublicationSessionId(10, 11, 12, 13);
final _deltaPublicationSession = FlarkV3PublicationSessionId(20, 21, 22, 23);
final _syntaxProfile = FlarkV3SyntaxProfileId(1);
final _queryBudget = FlarkV3HostQueryBudget(
  maxEncodedBytes: 64 * 1024,
  maxOpenDepth: 64,
  maxLeafCount: 256,
  maxTreeNodesVisited: 1024,
);
final _limits = FlarkV3HostOfferLimits(
  maximumFrameCount: 4,
  maximumEncodedFrameBytes: 1024,
  maximumPacketBytes: 256,
  maximumFrameBytes: 128,
  maximumProgramChildren: 128,
);
final _grant = FlarkV3HostWorkGrant(
  inspectBytes: 4096,
  copyBytes: 4096,
  transitions: 64,
);

FlarkV3HostOfferBegin _snapshot(
  FlarkV3SourceVersion source, {
  required int hostRevision,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(hostRevision, source.revision, 1, 1),
  publicationSession: _publicationSession,
  targetHostRevision: FlarkV3HostRevisionId(hostRevision),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(0, source.revision + 1),
  parseGeneration: source.revision + 1,
  grammarRevision: 1,
  syntaxProfile: _syntaxProfile,
  authorityMask: FlarkV3StructuralAuthorityMask.complete,
  mode: FlarkV3PublicationMode.fullSnapshot,
  baseAck: null,
  transferredRecordCount: 1,
  targetRecordCount: 1,
  limits: _limits,
);

FlarkV3HostOfferBegin _exactDeltaOffer(
  FlarkV3SourceVersion source, {
  required FlarkV3StructuralAck base,
}) => FlarkV3HostOfferBegin(
  offerId: FlarkV3OfferId(2, source.revision, 2, 2),
  publicationSession: _deltaPublicationSession,
  targetHostRevision: FlarkV3HostRevisionId(2),
  sourceVersion: source,
  sourceRoot: FlarkV3SourceRootId(0, source.revision + 101),
  parseGeneration: base.parseGeneration + 1,
  grammarRevision: base.grammarRevision,
  syntaxProfile: base.syntaxProfile,
  authorityMask: base.authorityMask,
  mode: FlarkV3PublicationMode.exactBaseDelta,
  baseAck: base,
  transferredRecordCount: 1,
  targetRecordCount: 1,
  limits: _limits,
);

/// Small test-worker stand-in. Production must return these typed ACKs from
/// the native/Wasm replica after actually applying each bounded lease.
void _acknowledgeAllSourceWorkerSync(FlarkV3FlutterLiveController controller) {
  while (controller.documentSession.hasPendingSourceWorkerSync) {
    final lease = controller.beginSourceWorkerSync();
    final acknowledgement = switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.acknowledgement(
        observedReplica: lease.isLast ? _observedFor(controller, lease) : null,
      ),
      FlarkV3SourceIntentSyncLease() => lease.acknowledgement(
        observedReplica: _observedFor(controller, lease),
      ),
    };
    final receipt = controller.acknowledgeSourceWorkerSync(acknowledgement);
    expect(
      receipt.disposition,
      FlarkV3SourceWorkerSyncAckDisposition.acknowledged,
    );
  }
}

FlarkV3ObservedSourceReplicaVersion _observedFor(
  FlarkV3FlutterLiveController controller,
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: switch (target) {
      FlarkV3KnownSourceStamp() => target.utf8Length,
      FlarkV3ProvisionalSourceStamp() =>
        utf8.encode(controller.source.toString()).length,
    },
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

FlarkV3ObservedSourceReplicaVersion _observedForSourceSession(
  FlarkV3SourceSession session,
  FlarkV3SourceWorkerSyncLease lease,
) {
  final target = switch (lease) {
    FlarkV3SourceSnapshotSyncLease() => lease.targetStamp,
    FlarkV3SourceIntentSyncLease() => lease.targetStamp,
  };
  return FlarkV3ObservedSourceReplicaVersion(
    revision: target.revision,
    utf16Length: target.utf16Length,
    utf8Length: switch (target) {
      FlarkV3KnownSourceStamp() => target.utf8Length,
      FlarkV3ProvisionalSourceStamp() =>
        utf8.encode(session.document.toString()).length,
    },
    intentHighWater: switch (lease) {
      FlarkV3SourceSnapshotSyncLease() => lease.throughIntentSequence,
      FlarkV3SourceIntentSyncLease() => lease.lastSequence,
    },
  );
}

List<FlarkV3SourcePrefixFacts> _canonicalFacts(String source) => [
  FlarkV3SourcePrefixFacts(
    utf16Offset: 4,
    utf8Offset: 4,
    newlines: 0,
    hash: FlarkV3SourceDocument.fromString(
      source.substring(0, 4),
    ).contentHash128,
  ),
  FlarkV3SourcePrefixFacts(
    utf16Offset: source.length,
    utf8Offset: utf8.encode(source).length,
    newlines: 0,
    hash: FlarkV3SourceDocument.fromString(source).contentHash128,
  ),
];

FlarkV3ContentHash128 _portableFactsHash(List<FlarkV3SourcePrefixFacts> facts) {
  const mask32 = 0xFFFFFFFF;
  const bases = [0x00100193, 0x9E3779B1, 0x85EBCA77, 0xC2B2AE3D];
  var words = [0, 0, 0, 0];
  for (final fact in facts) {
    for (final value in [
      fact.utf16Offset,
      fact.utf8Offset,
      fact.newlines,
      fact.hash.word0,
      fact.hash.word1,
      fact.hash.word2,
      fact.hash.word3,
    ]) {
      for (var shift = 0; shift < 64; shift += 8) {
        final term = ((value >>> shift) & 0xFF) + 1;
        words = [
          for (var lane = 0; lane < 4; lane += 1)
            (words[lane] * bases[lane] + term) & mask32,
        ];
      }
    }
  }
  return FlarkV3ContentHash128(words[0], words[1], words[2], words[3]);
}

FlarkV3StructuralAck _publishOnePacket(
  FlarkV3FlutterLiveController controller,
  FlarkV3HostOfferBegin offer,
) {
  expect(controller.beginOffer(offer), isA<FlarkV3HostAccepted>());
  expect(
    controller.admitPacket(
      _publicationPacket(
        offerId: offer.offerId,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: 1,
        digest: _digest(40),
        frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
      ),
    ),
    isA<FlarkV3HostAccepted>(),
  );
  expect(
    (controller.pollHost(_grant) as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
        .value,
    isA<FlarkV3HostPacketCredit>(),
  );
  expect(
    controller.requestCommit(_onePacketCommit(offer)),
    isA<FlarkV3HostAccepted>(),
  );
  return ((controller.pollHost(_grant)
                  as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
              .value
          as FlarkV3HostCommitted)
      .ack;
}

FlarkV3StructuralAck _publishOnSession(
  FlarkDocumentSession session,
  FlarkV3HostOfferBegin offer,
) {
  expect(session.beginOffer(offer), isA<FlarkV3HostAccepted>());
  expect(
    session.admitPacket(
      _publicationPacket(
        offerId: offer.offerId,
        firstFrameOrdinal: 0,
        firstRecordOrdinal: 0,
        recordCount: 1,
        digest: _digest(40),
        frameBytes: Uint8List.fromList([0xC1, 0xAA, 0x55]),
      ),
    ),
    isA<FlarkV3HostAccepted>(),
  );
  expect(
    (session.pollHost(_grant) as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
        .value,
    isA<FlarkV3HostPacketCredit>(),
  );
  expect(
    session.requestCommit(_onePacketCommit(offer)),
    isA<FlarkV3HostAccepted>(),
  );
  return ((session.pollHost(_grant)
                  as FlarkV3HostAccepted<FlarkV3HostPollOutcome>)
              .value
          as FlarkV3HostCommitted)
      .ack;
}

FlarkV3HostPublicationPacket _publicationPacket({
  required FlarkV3OfferId offerId,
  required int firstFrameOrdinal,
  required int firstRecordOrdinal,
  required int recordCount,
  required FlarkV3ProtocolDigest128 digest,
  required Uint8List frameBytes,
}) {
  final bodyOffset =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  final rawBytes = Uint8List(bodyOffset + frameBytes.length);
  final data = ByteData.sublistView(rawBytes);

  void writeId(int offset, FlarkV3ProtocolId128 id) {
    data
      ..setUint32(offset, id.word0, Endian.little)
      ..setUint32(offset + 4, id.word1, Endian.little)
      ..setUint32(offset + 8, id.word2, Endian.little)
      ..setUint32(offset + 12, id.word3, Endian.little);
  }

  rawBytes.setRange(0, 4, const <int>[0x46, 0x50, 0x4b, 0x33]);
  data
    ..setUint16(4, FlarkV3HostPublicationPacket.wireVersion, Endian.little)
    ..setUint16(6, FlarkV3HostPublicationPacket.wireFlags, Endian.little);
  writeId(8, offerId);
  data
    ..setUint32(24, firstFrameOrdinal, Endian.little)
    ..setUint32(28, firstRecordOrdinal, Endian.little)
    ..setUint32(32, 1, Endian.little)
    ..setUint32(36, recordCount, Endian.little)
    ..setUint32(40, frameBytes.length, Endian.little)
    ..setUint32(
      FlarkV3HostPublicationPacket.wireHeaderBytes,
      frameBytes.length,
      Endian.little,
    )
    ..setUint32(
      FlarkV3HostPublicationPacket.wireHeaderBytes + 4,
      recordCount,
      Endian.little,
    );
  writeId(FlarkV3HostPublicationPacket.wireHeaderBytes + 8, digest);
  rawBytes.setRange(bodyOffset, rawBytes.length, frameBytes);
  return FlarkV3HostPublicationPacket.fromOwnedBytes(rawBytes);
}

Uint8List _singleFrameBody(FlarkV3HostPublicationPacket packet) {
  if (packet.frameCount != 1) {
    throw ArgumentError.value(packet.frameCount, 'packet.frameCount');
  }
  final bodyOffset =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  return Uint8List.sublistView(packet.rawBytes, bodyOffset);
}

FlarkV3HostCommitRequest _onePacketCommit(FlarkV3HostOfferBegin offer) =>
    FlarkV3HostCommitRequest(
      offerId: offer.offerId,
      actualFrameCount: 1,
      actualEncodedFrameBytes: 3,
      rollingTransportDigest: _digest(50),
      canonicalStreamDigest: _digest(60),
    );

FlarkV3ProtocolDigest128 _digest(int seed) =>
    FlarkV3ProtocolDigest128(seed, seed + 1, seed + 2, seed + 3);

final class _FrameModelHostStore implements FlarkV3HostStore {
  FlarkV3SourceVersion? currentSource;
  FlarkV3StructuralAck? installed;
  FlarkV3StructuralAck? pendingAck;
  FlarkV3HostOfferBegin? active;
  FlarkV3HostCommitRequest? commit;
  FlarkV3OfferId? pendingAbort;
  _OwnedPacket? pendingPacket;
  int acceptedFrames = 0;
  int acceptedBytes = 0;
  int acceptedRecords = 0;
  FlarkV3StructuralAck? forcedAck;
  bool forceSourceGap = false;
  bool closed = false;
  FlarkV3HostPointQuery? lastQuery;
  int queryCount = 0;
  int pollCount = 0;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) {
    if (closed) return _rejected(FlarkV3HostRejectReason.closed);
    final current = currentSource;
    if (current != null &&
        (current.documentSession != sourceVersion.documentSession ||
            sourceVersion.revision < current.revision)) {
      return _rejected(FlarkV3HostRejectReason.invalid);
    }
    currentSource = sourceVersion;
    if (active?.sourceVersion != sourceVersion) {
      active = null;
      commit = null;
      pendingPacket = null;
    }
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) {
    if (begin.sourceVersion != currentSource) {
      return _rejected(FlarkV3HostRejectReason.exactSourceMismatch);
    }
    if (active != null) {
      return _rejected(FlarkV3HostRejectReason.backpressure);
    }
    active = begin;
    commit = null;
    pendingPacket = null;
    acceptedFrames = 0;
    acceptedBytes = 0;
    acceptedRecords = 0;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) {
    if (active?.offerId != packet.offerId) {
      return _rejected(FlarkV3HostRejectReason.wrongOffer);
    }
    if (pendingPacket != null) {
      return _rejected(FlarkV3HostRejectReason.backpressure);
    }
    pendingPacket = _OwnedPacket(
      frameCount: packet.frameCount,
      recordCount: packet.aggregateRecordCount,
      frameBytes: Uint8List.fromList(_singleFrameBody(packet)),
    );
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) {
    if (active?.offerId != request.offerId) {
      return _rejected(FlarkV3HostRejectReason.wrongOffer);
    }
    if (pendingPacket != null ||
        acceptedFrames != request.actualFrameCount ||
        acceptedBytes != request.actualEncodedFrameBytes) {
      return _rejected(FlarkV3HostRejectReason.invalid);
    }
    commit = request;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) {
    if (active?.offerId != offerId) {
      return _rejected(FlarkV3HostRejectReason.wrongOffer);
    }
    pendingAbort = offerId;
    active = null;
    commit = null;
    pendingPacket = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) {
    pollCount += 1;
    final aborted = pendingAbort;
    if (aborted != null) {
      pendingAbort = null;
      return FlarkV3HostAccepted(FlarkV3HostAbortComplete(aborted));
    }
    final forced = forcedAck;
    if (forced != null) {
      forcedAck = null;
      return FlarkV3HostAccepted(FlarkV3HostCommitted(forced));
    }
    final packet = pendingPacket;
    if (packet != null) {
      if (grant.inspectBytes < packet.frameBytes.length ||
          grant.copyBytes < packet.frameBytes.length ||
          grant.transitions == 0) {
        return const FlarkV3HostAccepted(FlarkV3HostPollPending());
      }
      pendingPacket = null;
      acceptedFrames += packet.frameCount;
      acceptedBytes += packet.frameBytes.length;
      acceptedRecords += packet.recordCount;
      return FlarkV3HostAccepted(
        FlarkV3HostPacketCredit(
          offerId: active!.offerId,
          nextFrameOrdinal: acceptedFrames,
        ),
      );
    }
    final begin = active;
    if (begin == null || commit == null) {
      return const FlarkV3HostAccepted(FlarkV3HostPollPending());
    }
    if (begin.sourceVersion != currentSource ||
        acceptedRecords != begin.transferredRecordCount) {
      return _rejected(FlarkV3HostRejectReason.superseded);
    }
    final ack = FlarkV3StructuralAck(
      publicationSession: begin.publicationSession,
      hostRevision: begin.targetHostRevision,
      sourceVersion: begin.sourceVersion,
      sourceRoot: begin.sourceRoot,
      parseGeneration: begin.parseGeneration,
      grammarRevision: begin.grammarRevision,
      syntaxProfile: begin.syntaxProfile,
      authorityMask: begin.authorityMask,
      recordCount: begin.targetRecordCount,
      sequenceDigest: _digest(100 + begin.sourceVersion.revision),
      manifestDigest: _digest(200 + begin.sourceVersion.revision),
    );
    installed = ack;
    pendingAck = ack;
    active = null;
    commit = null;
    return FlarkV3HostAccepted(FlarkV3HostCommitted(ack));
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) {
    if (pendingAck != ack) {
      return _rejected(FlarkV3HostRejectReason.invalid);
    }
    pendingAck = null;
    return _accepted();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) {
    queryCount += 1;
    lastQuery = query;
    if (installed?.sourceVersion != currentSource ||
        query.sourceVersion != currentSource) {
      return _rejected(FlarkV3HostRejectReason.exactSourceMismatch);
    }
    final range = FlarkV3MetricRange(
      start: FlarkV3SourceMetric.zero,
      end: currentSource!.metric,
    );
    if (forceSourceGap) {
      return FlarkV3HostAccepted(
        FlarkV3HostStoreSourceGapQuery(
          FlarkV3HostLocalSourceGap(
            sourceVersion: currentSource!,
            range: range,
            reason: FlarkV3HostSourceGapReason.openDepthLimit,
            receipt: FlarkV3HostViewportReceipt(
              encodedBytes: 0,
              leafCount: 1,
              openDepth: query.budget.maxOpenDepth,
              treeNodesVisited: 2,
              summaryNodesSkipped: 1,
            ),
          ),
        ),
      );
    }
    return FlarkV3HostAccepted(
      FlarkV3HostStoreStructuralQuery(
        FlarkV3HostStructuralViewport.owned(
          sourceVersion: currentSource!,
          range: range,
          encoded: Uint8List.fromList([1, 2, 3]),
          receipt: FlarkV3HostViewportReceipt(
            encodedBytes: 3,
            leafCount: 1,
            openDepth: 1,
            treeNodesVisited: 2,
            summaryNodesSkipped: 1,
          ),
        ),
      ),
    );
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    closed = true;
    return _accepted();
  }
}

final class _OwnedPacket {
  const _OwnedPacket({
    required this.frameCount,
    required this.recordCount,
    required this.frameBytes,
  });

  final int frameCount;
  final int recordCount;
  final Uint8List frameBytes;
}

FlarkV3HostAccepted<FlarkV3HostUnit> _accepted() =>
    const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

FlarkV3HostRejected<T> _rejected<T>(FlarkV3HostRejectReason reason) =>
    FlarkV3HostRejected(FlarkV3HostRejection(reason, reason.name));

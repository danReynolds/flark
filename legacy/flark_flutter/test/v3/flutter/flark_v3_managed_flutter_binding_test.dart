import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../v2/support/flark_test_paths.dart';

void main() {
  testWidgets(
    'hidden projected input survives edits and IME without replacing input state',
    (tester) async {
      const source = '[ref]: /target\n**x**';
      final runtime = (await tester.runAsync(() async {
        return FlarkV3DocumentRuntime.open(
          source,
          nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
        );
      }))!;
      final initialInlineGeneration =
          runtime.status.inlinePresentationGeneration;
      final initialInlineStopwatch = Stopwatch()..start();

      final binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 15,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: '**x**',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 16 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
      addTearDown(binding.dispose);
      final editingController = binding.controller.editingController;
      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkV3LiveEditorPrototype(
            controller: binding.controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      final editableState = editableKey.currentState!;

      expect(
        binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.sourceGap,
      );
      await tester.runAsync(() async {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        await _waitForInlinePresentationAfter(runtime, initialInlineGeneration);
      });
      initialInlineStopwatch.stop();
      await tester.pump();
      expect(
        initialInlineStopwatch.elapsed,
        lessThan(const Duration(seconds: 2)),
        reason:
            'initial demanded inline presentation took '
            '${initialInlineStopwatch.elapsed.inMilliseconds}ms',
      );
      expect(
        binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(binding.controller.editingController.text, 'x');
      expect(editableKey.currentState, same(editableState));
      editableState.requestKeyboard();
      await tester.pump();
      final setClient = tester.testTextInput.log.lastWhere(
        (call) => call.method == 'TextInput.setClient',
      );
      final clientId = (setClient.arguments as List<dynamic>).first as int;
      final setClientCount = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .length;
      final deltaClient = editableState as DeltaTextInputClient;
      final setEditingStateCount = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setEditingState')
          .length;
      final sourceSeenByControllerListeners = <String>[];
      void observeSourceFirst() {
        sourceSeenByControllerListeners.add(runtime.exportMarkdown());
      }

      editingController.addListener(observeSourceFirst);
      addTearDown(() => editingController.removeListener(observeSourceFirst));

      Future<void> sendPlatformDeltas(List<Map<String, Object?>> deltas) async {
        await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          SystemChannels.textInput.name,
          SystemChannels.textInput.codec.encodeMethodCall(
            MethodCall('TextInputClient.updateEditingStateWithDeltas', [
              clientId,
              {'deltas': deltas},
            ]),
          ),
          (_) {},
        );
        await tester.pump();
      }

      final targetRevision = runtime.sourceRevision + 3;
      await sendPlatformDeltas([
        _delta(
          oldText: 'x',
          deltaText: 'y',
          deltaStart: 0,
          deltaEnd: 1,
          selection: 1,
        ),
      ]);
      expect(sourceSeenByControllerListeners.last, '[ref]: /target\n**y**');
      await sendPlatformDeltas([
        _delta(
          oldText: 'y',
          deltaText: 'に',
          deltaStart: 1,
          deltaEnd: 1,
          selection: 2,
          composingStart: 1,
          composingEnd: 2,
        ),
      ]);

      expect(runtime.exportMarkdown(), '[ref]: /target\n**yに**');
      expect(sourceSeenByControllerListeners.last, '[ref]: /target\n**yに**');
      expect(
        binding.controller.editingController.value,
        const TextEditingValue(
          text: 'yに',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 1, end: 2),
        ),
        reason: 'the platform composing value must be echoed byte-for-byte',
      );
      await sendPlatformDeltas([
        _delta(
          oldText: 'yに',
          deltaText: '日本🌍',
          deltaStart: 1,
          deltaEnd: 2,
          selection: 5,
          composingStart: 1,
          composingEnd: 5,
        ),
      ]);

      expect(runtime.exportMarkdown(), '[ref]: /target\n**y日本🌍**');
      expect(sourceSeenByControllerListeners.last, '[ref]: /target\n**y日本🌍**');
      expect(
        binding.controller.editingController.value,
        const TextEditingValue(
          text: 'y日本🌍',
          selection: TextSelection.collapsed(offset: 5),
          composing: TextRange(start: 1, end: 5),
        ),
      );
      await sendPlatformDeltas([
        _delta(
          oldText: 'y日本🌍',
          deltaText: '',
          deltaStart: -1,
          deltaEnd: -1,
          selection: 5,
        ),
      ]);

      expect(runtime.exportMarkdown(), '[ref]: /target\n**y日本🌍**');
      expect(runtime.status.sourceRevision, targetRevision);
      expect(
        runtime.status.sourceCurrent,
        isFalse,
        reason: 'the controller edit must wake the runtime-owned executor',
      );
      expect(binding.controller.hasCertifiedInlinePresentation, isFalse);
      expect(binding.controller.hasProjectedInlinePresentation, isTrue);
      expect(binding.controller.editingController.text, 'y日本🌍');
      expect(runtime.status.state, FlarkV3DocumentRuntimeState.open);
      for (
        var turn = 0;
        turn < 150 &&
            (!runtime.status.structureCurrent ||
                binding.controller.paintState.mode !=
                    FlarkV3FlutterPaintMode.exactStructural ||
                !binding.controller.hasCertifiedInlinePresentation);
        turn++
      ) {
        await tester.pump(const Duration(milliseconds: 1));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 1)),
        );
      }
      expect(runtime.status.sourceCurrent, isTrue);
      expect(runtime.status.structureCurrent, isTrue);
      expect(
        binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(
        binding.controller.paintState.sourceVersion.revision,
        targetRevision,
      );
      expect(binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(binding.controller.editingController.text, 'y日本🌍');
      expect(binding.controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(focusNode.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .length,
        setClientCount,
      );
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setEditingState')
            .length,
        setEditingStateCount,
        reason:
            'a remote platform delta must not be echoed back as local state',
      );
      expect(
        (tester.testTextInput.log
                    .lastWhere((call) => call.method == 'TextInput.setClient')
                    .arguments
                as List<dynamic>)
            .first,
        clientId,
      );

      final revisionBeforeSplitScalar = runtime.sourceRevision;
      expect(
        () => deltaClient.updateEditingValueWithDeltas(const [
          TextEditingDeltaDeletion(
            oldText: 'y日本🌍',
            deletedRange: TextRange(start: 3, end: 4),
            selection: TextSelection.collapsed(offset: 3),
            composing: TextRange.empty,
          ),
        ]),
        throwsRangeError,
      );
      expect(runtime.sourceRevision, revisionBeforeSplitScalar);
      expect(runtime.exportMarkdown(), '[ref]: /target\n**y日本🌍**');
      expect(binding.controller.editingController.text, 'y日本🌍');

      final revisionBeforeOrphanCleanup = runtime.sourceRevision;
      deltaClient.updateEditingValueWithDeltas(const [
        TextEditingDeltaDeletion(
          oldText: 'y日本🌍',
          deletedRange: TextRange(start: 0, end: 5),
          selection: TextSelection.collapsed(offset: 0),
          composing: TextRange.empty,
        ),
      ]);
      expect(runtime.sourceRevision, revisionBeforeOrphanCleanup + 1);
      expect(runtime.exportMarkdown(), '[ref]: /target\n');
      expect(binding.controller.editingController.text, isEmpty);
      expect(binding.controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .length,
        setClientCount,
      );
      for (
        var turn = 0;
        turn < 150 && !runtime.status.structureCurrent;
        turn++
      ) {
        await tester.pump(const Duration(milliseconds: 1));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 1)),
        );
      }
      expect(runtime.status.structureCurrent, isTrue);
      expect(binding.controller.editingController.text, isEmpty);
      expect(editableKey.currentState, same(editableState));

      binding.dispose();
      expect(binding.isDisposed, isTrue);
      expect(runtime.status.state, isNot(FlarkV3DocumentRuntimeState.closed));
      await tester.runAsync(
        () => runtime.close().timeout(const Duration(seconds: 5)),
      );
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );

  testWidgets(
    'certified markers disappear live on the same platform input client',
    (tester) async {
      const source = 'plain';
      final runtime = (await tester.runAsync(() async {
        return FlarkV3DocumentRuntime.open(
          source,
          nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
        );
      }))!;
      final initialInlineGeneration =
          runtime.status.inlinePresentationGeneration;
      final initialInlineStopwatch = Stopwatch()..start();
      final binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 5),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 16 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
      addTearDown(() async {
        binding.dispose();
        if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
          await runtime.close();
        }
      });
      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkV3LiveEditorPrototype(
            controller: binding.controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      await tester.runAsync(() async {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        await _waitForInlinePresentationAfter(runtime, initialInlineGeneration);
      });
      initialInlineStopwatch.stop();
      await tester.pump();
      expect(
        initialInlineStopwatch.elapsed,
        lessThan(const Duration(seconds: 2)),
        reason:
            'initial demanded inline presentation took '
            '${initialInlineStopwatch.elapsed.inMilliseconds}ms',
      );
      expect(binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(binding.controller.editingController.text, source);

      editableKey.currentState!.requestKeyboard();
      await tester.pump();
      final initialClientCall = tester.testTextInput.log.lastWhere(
        (call) => call.method == 'TextInput.setClient',
      );
      final clientId =
          (initialClientCall.arguments as List<dynamic>).first as int;
      final initialClientCount = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .length;
      final initialSetEditingStateCount = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setEditingState')
          .length;

      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        SystemChannels.textInput.name,
        SystemChannels.textInput.codec.encodeMethodCall(
          MethodCall('TextInputClient.updateEditingStateWithDeltas', [
            clientId,
            {
              'deltas': [
                _delta(
                  oldText: source,
                  deltaText: '**plain**',
                  deltaStart: 0,
                  deltaEnd: 5,
                  selection: 9,
                ),
              ],
            },
          ]),
        ),
        (_) {},
      );
      await tester.pump();
      expect(runtime.exportMarkdown(), '**plain**');
      final targetRevision = runtime.sourceRevision;

      for (
        var turn = 0;
        turn < 150 &&
            (!runtime.status.structureCurrent ||
                !binding.controller.hasCertifiedInlinePresentation ||
                binding.controller.editingController.text != source);
        turn++
      ) {
        await tester.pump(const Duration(milliseconds: 1));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 1)),
        );
      }

      expect(runtime.status.sourceRevision, targetRevision);
      expect(
        runtime.status.structureCurrent,
        isTrue,
        reason:
            'state=${runtime.status.state.name}, '
            'source=${runtime.status.sourceRevision}, '
            'certified=${runtime.status.certifiedSourceRevision}, '
            'sourceCurrent=${runtime.status.sourceCurrent}, '
            'structure=${runtime.status.structureRevision}, '
            'recovery=${runtime.status.recoveryAvailable}',
      );
      expect(binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(
        binding.controller.editingController.value,
        const TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(offset: 5),
        ),
      );
      expect(focusNode.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .length,
        initialClientCount,
        reason: 'live marker hiding must not restart the input connection',
      );
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setEditingState')
            .length,
        initialSetEditingStateCount + 1,
        reason:
            'Flutter must acknowledge the certified projection on the same '
            'platform client',
      );
      final finalEditingState =
          tester.testTextInput.log
                  .lastWhere(
                    (call) => call.method == 'TextInput.setEditingState',
                  )
                  .arguments
              as Map<String, dynamic>;
      expect(finalEditingState['text'], source);
      expect(
        (tester.testTextInput.log
                    .lastWhere((call) => call.method == 'TextInput.setClient')
                    .arguments
                as List<dynamic>)
            .first,
        clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );

  testWidgets(
    'projected delta batches preserve separate hidden marker chains atomically',
    (tester) async {
      const source = '**a** _b_';
      final runtime = (await tester.runAsync(() async {
        return FlarkV3DocumentRuntime.open(
          source,
          nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
        );
      }))!;
      final initialInlineGeneration =
          runtime.status.inlinePresentationGeneration;
      final initialInlineStopwatch = Stopwatch()..start();
      final binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 16 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
      addTearDown(() async {
        binding.dispose();
        if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
          await runtime.close();
        }
      });
      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkV3LiveEditorPrototype(
            controller: binding.controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      await tester.runAsync(() async {
        await runtime.initialReady.timeout(const Duration(seconds: 5));
        await _waitForInlinePresentationAfter(runtime, initialInlineGeneration);
      });
      initialInlineStopwatch.stop();
      await tester.pump();
      expect(
        initialInlineStopwatch.elapsed,
        lessThan(const Duration(seconds: 2)),
        reason:
            'initial demanded inline presentation took '
            '${initialInlineStopwatch.elapsed.inMilliseconds}ms',
      );
      expect(binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(binding.controller.editingController.text, 'a b');
      editableKey.currentState!.requestKeyboard();
      await tester.pump();

      final sourceSeenByControllerListeners = <String>[];
      void observeSourceFirst() {
        sourceSeenByControllerListeners.add(runtime.exportMarkdown());
      }

      binding.controller.editingController.addListener(observeSourceFirst);
      addTearDown(
        () => binding.controller.editingController.removeListener(
          observeSourceFirst,
        ),
      );
      final revisionBefore = runtime.sourceRevision;
      final deltaClient = editableKey.currentState! as DeltaTextInputClient;

      deltaClient.updateEditingValueWithDeltas(const [
        TextEditingDeltaInsertion(
          oldText: 'a b',
          textInserted: 'X',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
        TextEditingDeltaReplacement(
          oldText: 'aX b',
          replacementText: 'Y',
          replacedRange: TextRange(start: 3, end: 4),
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange.empty,
        ),
        TextEditingDeltaNonTextUpdate(
          oldText: 'aX Y',
          selection: TextSelection.collapsed(offset: 4),
          composing: TextRange.empty,
        ),
      ]);

      expect(runtime.exportMarkdown(), '**aX** _Y_');
      expect(runtime.sourceRevision, revisionBefore + 1);
      expect(sourceSeenByControllerListeners.last, '**aX** _Y_');
      expect(
        binding.controller.editingController.value,
        const TextEditingValue(
          text: 'aX Y',
          selection: TextSelection.collapsed(offset: 4),
        ),
      );
      expect(binding.controller.hasProjectedInlinePresentation, isTrue);
      expect(binding.controller.hasCertifiedInlinePresentation, isFalse);

      final revisionBeforeRejected = runtime.sourceRevision;
      expect(
        () => deltaClient.updateEditingValueWithDeltas(const [
          TextEditingDeltaInsertion(
            oldText: 'aX Y',
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
      expect(runtime.sourceRevision, revisionBeforeRejected);
      expect(runtime.exportMarkdown(), '**aX** _Y_');
      expect(binding.controller.editingController.text, 'aX Y');

      deltaClient.updateEditingValueWithDeltas(const [
        TextEditingDeltaDeletion(
          oldText: 'aX Y',
          deletedRange: TextRange(start: 0, end: 2),
          selection: TextSelection.collapsed(offset: 0),
          composing: TextRange.empty,
        ),
        TextEditingDeltaInsertion(
          oldText: ' Y',
          textInserted: 'に',
          insertionOffset: 0,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange(start: 0, end: 1),
        ),
      ]);
      expect(runtime.exportMarkdown(), '**に** _Y_');
      expect(runtime.sourceRevision, revisionBeforeRejected + 1);
      expect(
        binding.controller.editingController.value,
        const TextEditingValue(
          text: 'に Y',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange(start: 0, end: 1),
        ),
      );
      expect(
        binding.controller.globalEditingState.composing,
        const TextRange(start: 2, end: 3),
        reason:
            'the continuation anchor must preserve both inherited style and '
            'exact IME coordinates',
      );
      deltaClient.updateEditingValueWithDeltas(const [
        TextEditingDeltaNonTextUpdate(
          oldText: 'に Y',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);

      final revisionBeforeCrossingMarkers = runtime.sourceRevision;
      expect(
        () => deltaClient.updateEditingValueWithDeltas(const [
          TextEditingDeltaReplacement(
            oldText: 'に Y',
            replacementText: 'Z',
            replacedRange: TextRange(start: 0, end: 3),
            selection: TextSelection.collapsed(offset: 1),
            composing: TextRange.empty,
          ),
        ]),
        throwsStateError,
      );
      expect(runtime.sourceRevision, revisionBeforeCrossingMarkers);
      expect(runtime.exportMarkdown(), '**に** _Y_');
      expect(binding.controller.editingController.text, 'に Y');

      for (
        var turn = 0;
        turn < 150 && !runtime.status.structureCurrent;
        turn++
      ) {
        await tester.pump(const Duration(milliseconds: 1));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 1)),
        );
      }
      expect(runtime.status.structureCurrent, isTrue);

      final externalBaseRevision = runtime.sourceRevision;
      runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: externalBaseRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 7,
            endUtf16: 8,
            replacement: 'Z',
          ),
        ),
      );
      expect(runtime.exportMarkdown(), '**に** _Z_');
      expect(
        () => binding.controller.applyTextEditingDelta(
          const TextEditingDeltaInsertion(
            oldText: 'に Y',
            textInserted: '!',
            insertionOffset: 3,
            selection: TextSelection.collapsed(offset: 4),
            composing: TextRange.empty,
          ),
        ),
        throwsA(isA<FlarkV3RevisionMismatch>()),
        reason:
            'a platform island must submit its captured base revision rather '
            'than overwrite an external same-length edit',
      );
      expect(runtime.sourceRevision, externalBaseRevision + 1);
      expect(runtime.exportMarkdown(), '**に** _Z_');

      for (
        var turn = 0;
        turn < 150 && !runtime.status.structureCurrent;
        turn++
      ) {
        await tester.pump(const Duration(milliseconds: 1));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 1)),
        );
      }
      expect(runtime.status.structureCurrent, isTrue);
      binding.dispose();
      await tester.runAsync(
        () => runtime.close().timeout(const Duration(seconds: 5)),
      );
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );
}

Future<void> _waitForInlinePresentationAfter(
  FlarkV3DocumentRuntime runtime,
  int generation,
) async {
  if (runtime.status.inlinePresentationGeneration > generation) return;
  await runtime.statuses
      .firstWhere((status) => status.inlinePresentationGeneration > generation)
      .timeout(const Duration(seconds: 1));
}

Map<String, Object?> _delta({
  required String oldText,
  required String deltaText,
  required int deltaStart,
  required int deltaEnd,
  required int selection,
  int composingStart = -1,
  int composingEnd = -1,
}) => {
  'oldText': oldText,
  'deltaText': deltaText,
  'deltaStart': deltaStart,
  'deltaEnd': deltaEnd,
  'selectionBase': selection,
  'selectionExtent': selection,
  'selectionAffinity': 'TextAffinity.downstream',
  'selectionIsDirectional': false,
  'composingBase': composingStart,
  'composingExtent': composingEnd,
};

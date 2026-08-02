import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

const _initialSource = 'Before.\n\n**β😀** and _em_.\n\nAfter.';
const _composedSource = 'Before.\n\n**βに😀** and _em_.\n\nAfter.';
const _initialDisplay = 'β😀 and em.\n';
const _composedDisplay = 'βに😀 and em.\n';

void main() {
  testWidgets(
    'small marker-free virtualized editor preserves IME composition on one client',
    (tester) async {
      final targetStart = _initialSource.indexOf('**β');
      final initialCaret = _initialSource.indexOf('😀');
      final runtime = await runManagedRuntimeAsyncForTest(
        tester,
        () => openManagedRuntimeForTest(_initialSource),
      );
      final binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 8192,
          value: TextEditingValue(
            text: _initialSource,
            selection: TextSelection.collapsed(offset: initialCaret),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 16 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
      final presentationSource = binding
          .attachCompleteDocumentViewportPresentation();
      final surfaceController = FlarkV3VirtualizedLiveSurfaceController();
      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      var closed = false;

      Future<void> close() async {
        if (closed) return;
        closed = true;
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pump();
        binding.dispose();
        focusNode.dispose();
        if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
          await runManagedRuntimeAsyncForTest(
            tester,
            () => runtime.close().timeout(const Duration(seconds: 10)),
          );
        }
      }

      addTearDown(close);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Center(
            child: SizedBox(
              width: 640,
              height: 360,
              child: FlarkV3VirtualizedLiveSurface(
                liveController: binding.controller,
                visibleBlockCoordinator: binding.visibleBlocks,
                presentationSource: presentationSource,
                controller: surfaceController,
                editableKey: editableKey,
                focusNode: focusNode,
                paintLayerBuilder: (context, state) => const SizedBox.shrink(),
              ),
            ),
          ),
        ),
      );
      await runManagedRuntimeAsyncForTest(
        tester,
        () => runtime.initialReady.timeout(const Duration(seconds: 10)),
      );
      await _waitForExactMarkerFreeLeaf(
        tester,
        runtime: runtime,
        binding: binding,
        presentationSource: presentationSource,
        expectedRevision: runtime.sourceRevision,
        expectedDisplay: _initialDisplay,
      );

      final editableFinder = _editableText();
      expect(
        _editableText(skipOffstage: false),
        findsOneWidget,
        reason:
            'The production surface must retain its one long-lived editor '
            'even before the follower is painted.',
      );
      expect(
        editableKey.currentState,
        isNotNull,
        reason:
            'The production active-editor branch must own one EditableText.',
      );
      expect(editableFinder, findsOneWidget);
      expect(binding.controller.editingController.text, _initialDisplay);
      expect(
        binding.controller.editingController.text,
        isNot(anyOf(contains('**'), contains('_em_'))),
      );
      expect(
        surfaceController.mountedPresentationCount,
        lessThanOrEqualTo(flarkV3MaximumMountedViewportPresentations),
      );

      final editableState = editableKey.currentState!;
      editableState.requestKeyboard();
      await tester.pump();
      final setClientCallsBefore = _setClientCalls(tester);
      expect(setClientCallsBefore, hasLength(1));
      final clientId =
          (setClientCallsBefore.single.arguments as List<dynamic>).first as int;
      expect(tester.testTextInput.setClientArgs!['enableDeltaModel'], isTrue);

      final insertionOffset = _initialDisplay.indexOf('😀');
      final revisionBeforeComposition = runtime.sourceRevision;
      await _sendPlatformDelta(
        tester,
        clientId: clientId,
        delta: _delta(
          oldText: _initialDisplay,
          deltaText: 'に',
          deltaStart: insertionOffset,
          deltaEnd: insertionOffset,
          selection: insertionOffset + 1,
          composingStart: insertionOffset,
          composingEnd: insertionOffset + 1,
        ),
      );

      final composedSourceOffset = _composedSource.indexOf('に', targetStart);
      expect(runtime.sourceRevision, revisionBeforeComposition + 1);
      expect(runtime.exportMarkdown(), _composedSource);
      expect(
        binding.controller.editingController.value,
        TextEditingValue(
          text: _composedDisplay,
          selection: TextSelection.collapsed(offset: insertionOffset + 1),
          composing: TextRange(
            start: insertionOffset,
            end: insertionOffset + 1,
          ),
        ),
      );
      expect(
        binding.controller.globalEditingState.composing,
        TextRange(start: composedSourceOffset, end: composedSourceOffset + 1),
      );
      _expectSameInputClient(
        tester,
        editableKey: editableKey,
        editableState: editableState,
        clientId: clientId,
        setClientCount: setClientCallsBefore.length,
      );

      await _sendPlatformDelta(
        tester,
        clientId: clientId,
        delta: _delta(
          oldText: _composedDisplay,
          deltaText: '',
          deltaStart: -1,
          deltaEnd: -1,
          selection: insertionOffset + 1,
        ),
      );
      expect(
        binding.controller.editingController.value.composing,
        TextRange.empty,
      );
      expect(binding.controller.globalEditingState.composing, TextRange.empty);

      await _waitForExactMarkerFreeLeaf(
        tester,
        runtime: runtime,
        binding: binding,
        presentationSource: presentationSource,
        expectedRevision: revisionBeforeComposition + 1,
        expectedDisplay: _composedDisplay,
      );
      final query =
          binding.controller.paintState.documentQuery
              as FlarkV3DocumentStructuralQuery;
      expect(query.structure.kind, FlarkV3DocumentStructureKind.paragraph);
      expect(
        query.inlineFacts!.facts.map((fact) => fact.kind),
        containsAll([
          FlarkV3InlineFactKind.strong,
          FlarkV3InlineFactKind.emphasis,
        ]),
      );
      expect(runtime.exportMarkdown(), _composedSource);
      expect(binding.controller.editingController.text, _composedDisplay);
      expect(
        binding.controller.editingController.text,
        isNot(anyOf(contains('**'), contains('_em_'))),
      );
      expect(editableFinder, findsOneWidget);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      _expectSameInputClient(
        tester,
        editableKey: editableKey,
        editableState: editableState,
        clientId: clientId,
        setClientCount: setClientCallsBefore.length,
      );

      await close();
      expect(tester.testTextInput.hasAnyClients, isFalse);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

Future<void> _waitForExactMarkerFreeLeaf(
  WidgetTester tester, {
  required FlarkV3DocumentRuntime runtime,
  required FlarkV3ManagedFlutterBinding binding,
  required FlarkV3ManagedViewportPresentationSource presentationSource,
  required int expectedRevision,
  required String expectedDisplay,
}) async {
  final stopwatch = Stopwatch()..start();
  while (stopwatch.elapsed < const Duration(seconds: 10)) {
    await tester.pump(const Duration(milliseconds: 1));
    await runManagedRuntimeAsyncForTest(
      tester,
      () => Future<void>.delayed(const Duration(milliseconds: 1)),
    );
    final status = runtime.status;
    final query = binding.controller.paintState.documentQuery;
    if (status.state == FlarkV3DocumentRuntimeState.open &&
        status.sourceRevision == expectedRevision &&
        status.certifiedSourceRevision == expectedRevision &&
        status.sourceCurrent &&
        status.structureRevision == expectedRevision &&
        status.structureCurrent &&
        presentationSource.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
        binding.visibleBlocks.phase == FlarkV3FlutterVisibleBlockPhase.exact &&
        query is FlarkV3DocumentStructuralQuery &&
        query.structure.kind == FlarkV3DocumentStructureKind.paragraph &&
        query.inlineFacts?.disposition ==
            FlarkV3InlineFactsDisposition.authoritative &&
        binding.controller.hasCertifiedInlinePresentation &&
        binding.controller.editingController.text == expectedDisplay) {
      await tester.pump();
      return;
    }
  }
  throw TestFailure(
    'Timed out waiting for an exact marker-free inline leaf: '
    'revision=${runtime.status.sourceRevision}/$expectedRevision, '
    'certified=${runtime.status.certifiedSourceRevision}, '
    'structure=${runtime.status.structureRevision}, '
    'current=${runtime.status.structureCurrent}, '
    'visible=${binding.visibleBlocks.phase.name}, '
    'viewport=${presentationSource.snapshot.runtimeType}, '
    'paint=${binding.controller.paintState.mode.name}, '
    'query=${binding.controller.paintState.documentQuery.runtimeType}, '
    'display=${binding.controller.editingController.text}.',
  );
}

Future<void> _sendPlatformDelta(
  WidgetTester tester, {
  required int clientId,
  required Map<String, Object?> delta,
}) async {
  await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    SystemChannels.textInput.name,
    SystemChannels.textInput.codec.encodeMethodCall(
      MethodCall('TextInputClient.updateEditingStateWithDeltas', [
        clientId,
        {
          'deltas': [delta],
        },
      ]),
    ),
    (_) {},
  );
  await tester.pump();
}

Map<String, Object?> _delta({
  required String oldText,
  required String deltaText,
  required int deltaStart,
  required int deltaEnd,
  required int selection,
  int composingStart = -1,
  int composingEnd = -1,
}) => <String, Object?>{
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

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

Finder _editableText({bool skipOffstage = true}) => find.byWidgetPredicate(
  (widget) => widget is EditableText,
  description: 'the single EditableText-compatible v3 input host',
  skipOffstage: skipOffstage,
);

void _expectSameInputClient(
  WidgetTester tester, {
  required GlobalKey<EditableTextState> editableKey,
  required EditableTextState editableState,
  required int clientId,
  required int setClientCount,
}) {
  expect(editableKey.currentState, same(editableState));
  final calls = _setClientCalls(tester);
  expect(calls, hasLength(setClientCount));
  expect((calls.last.arguments as List<dynamic>).first, clientId);
}

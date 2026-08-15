import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

const _source =
    '- a\n'
    '  > **b** and _c_\n'
    '  ```\n'
    '  code\n'
    '  ```\n'
    '- **d**\n';
const _finalSource =
    '- a\n'
    '  > **by** and _c_\n'
    '  ```\n'
    '  code\n'
    '  ```\n'
    '- **d**\n';

void main() {
  testWidgets(
    'recursive Green Paragraph stays marker-free on one input client',
    (tester) async {
      final runtime = await runManagedRuntimeAsyncForTest(
        tester,
        () => openManagedRuntimeForTest(_source),
      );
      final initialRevision = runtime.sourceRevision;
      final caret = _source.indexOf('b');
      final binding = FlarkV3ManagedFlutterBinding.attach(
        runtime: runtime,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 256,
          value: TextEditingValue(
            text: _source,
            selection: TextSelection.collapsed(offset: caret),
          ),
        ),
        queryBudget: FlarkV3HostQueryBudget(
          maxEncodedBytes: 16 * 1024,
          maxOpenDepth: 64,
          maxLeafCount: 256,
          maxTreeNodesVisited: 1024,
        ),
      );
      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      var closed = false;
      addTearDown(() async {
        if (closed) return;
        binding.dispose();
        focusNode.dispose();
        await runManagedRuntimeAsyncForTest(tester, runtime.close);
      });

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
      await runManagedRuntimeAsyncForTest(
        tester,
        () => runtime.initialReady.timeout(const Duration(seconds: 60)),
      );
      final initialQuery = await _waitForRecursiveParagraph(
        tester,
        runtime: runtime,
        binding: binding,
        expectedRevision: initialRevision,
        expectedDisplay: 'b and c',
        expectedSource: _source,
      );
      expect(
        initialQuery.inlineFacts!.facts.map((fact) => fact.kind),
        containsAll([
          FlarkV3InlineFactKind.strong,
          FlarkV3InlineFactKind.emphasis,
        ]),
      );
      expect(
        runtime.readSourceRange(
          initialQuery.inlineSource!.startUtf16,
          initialQuery.inlineSource!.endUtf16,
        ),
        '**b** and _c_',
      );

      final editableState = editableKey.currentState!;
      editableState.requestKeyboard();
      await tester.pump();
      expect(focusNode.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      final editingController = binding.controller.editingController;
      final initialSetClients = _setClientCalls(tester);
      final clientId =
          (initialSetClients.last.arguments as List<dynamic>).first as int;
      final observedDisplays = <String>[editingController.text];
      void recordDisplay() => observedDisplays.add(editingController.text);

      editingController.addListener(recordDisplay);
      addTearDown(() => editingController.removeListener(recordDisplay));

      final deltaClient = editableState as DeltaTextInputClient;
      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'b and c',
          textInserted: 'x',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      ]);
      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'bx and c',
          textInserted: 'y',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);
      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaDeletion(
          oldText: 'bxy and c',
          deletedRange: TextRange(start: 1, end: 2),
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      ]);

      final finalRevision = initialRevision + 3;
      expect(runtime.sourceRevision, finalRevision);
      expect(runtime.exportMarkdown(), _finalSource);
      final finalQuery = await _waitForRecursiveParagraph(
        tester,
        runtime: runtime,
        binding: binding,
        expectedRevision: finalRevision,
        expectedDisplay: 'by and c',
        expectedSource: _finalSource,
      );
      expect(finalQuery.owner.kind, FlarkV3RecursiveGreenKind.paragraph);
      expect(observedDisplays, isNotEmpty);
      expect(
        observedDisplays,
        everyElement(
          allOf(
            isNotEmpty,
            isNot(contains('**')),
            isNot(contains('_')),
            isNot(contains('>')),
            isNot(contains('```')),
          ),
        ),
        reason:
            'after recursive Paragraph authority arrives, neither fast edits '
            'nor recertification may expose canonical markers or blank paint',
      );
      expect(editableKey.currentState, same(editableState));
      expect(binding.controller.editingController, same(editingController));
      expect(focusNode.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(_setClientCalls(tester), hasLength(initialSetClients.length));
      expect(
        (_setClientCalls(tester).last.arguments as List<dynamic>).first,
        clientId,
      );
      expect(runtime.exportMarkdown(), _finalSource);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
      binding.dispose();
      focusNode.dispose();
      await runManagedRuntimeAsyncForTest(
        tester,
        () => runtime.close().timeout(const Duration(seconds: 60)),
      );
      closed = true;
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}

Future<FlarkV3RecursiveGreenPointQuery> _waitForRecursiveParagraph(
  WidgetTester tester, {
  required FlarkV3DocumentRuntime runtime,
  required FlarkV3ManagedFlutterBinding binding,
  required int expectedRevision,
  required String expectedDisplay,
  required String expectedSource,
}) async {
  final stopwatch = Stopwatch()..start();
  while (stopwatch.elapsed < const Duration(seconds: 60)) {
    await tester.pump(const Duration(milliseconds: 1));
    await runManagedRuntimeAsyncForTest(
      tester,
      () => Future<void>.delayed(const Duration(milliseconds: 1)),
    );
    final status = runtime.status;
    if (status.state == FlarkV3DocumentRuntimeState.faulted) {
      throw TestFailure(
        'The managed runtime faulted at revision ${status.sourceRevision}.',
      );
    }
    final query = binding.controller.paintState.documentQuery;
    if (status.state == FlarkV3DocumentRuntimeState.open &&
        status.sourceRevision == expectedRevision &&
        status.certifiedSourceRevision == expectedRevision &&
        status.sourceCurrent &&
        status.structureRevision == expectedRevision &&
        status.structureCurrent &&
        query is FlarkV3RecursiveGreenPointQuery &&
        query.sourceRevision == expectedRevision &&
        query.structureRevision == expectedRevision &&
        query.owner.kind == FlarkV3RecursiveGreenKind.paragraph &&
        query.inlineFacts != null &&
        query.paragraphSource != null &&
        query.inlineSource != null &&
        binding.controller.hasCertifiedInlinePresentation &&
        binding.controller.editingController.text == expectedDisplay &&
        runtime.exportMarkdown() == expectedSource) {
      return query;
    }
  }
  final status = runtime.status;
  throw TestFailure(
    'Timed out waiting for recursive Paragraph: '
    'revision=${status.sourceRevision}/$expectedRevision, '
    'certified=${status.certifiedSourceRevision}, '
    'structure=${status.structureRevision}, '
    'inline=${status.inlinePresentationGeneration}/'
    '${status.inlineAttemptOutcomeGeneration}, '
    'query=${binding.controller.paintState.documentQuery.runtimeType}, '
    'display=${binding.controller.editingController.text}.',
  );
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'real indented code stays marker-free and Enter preserves canonical indentation',
    (tester) async {
      const initialCode = '    alpha\n      beta\n';
      const initialSource = 'before\n\n$initialCode\n*tail*';
      final codeStart = initialSource.indexOf(initialCode);
      final harness = await _ManagedIndentedCodeHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: codeStart + '    alpha'.length,
      );

      final initialQuery = await harness.waitForIndentedCode(
        tester,
        expectedDisplay: 'alpha\n  beta\n',
      );
      final initialFacts = initialQuery.structure.indentedCode!;
      expect(initialFacts.deindentColumns, 4);
      expect(initialFacts.lineCount, 2);
      expect(initialQuery.indentedCodeProjection, isNotNull);
      final initialRecords = initialQuery.indentedCodeProjection!.records;
      expect(initialRecords, hasLength(2));
      expect(harness.sourceText(initialRecords[0].hiddenPrefix), '    ');
      expect(harness.sourceText(initialRecords[0].content), 'alpha');
      expect(harness.sourceText(initialRecords[1].hiddenPrefix), '    ');
      expect(harness.sourceText(initialRecords[1].content), '  beta');
      expect(harness.sourceText(initialQuery.structure.source), initialCode);
      expect(
        harness.runtime.exportMarkdown(),
        initialSource,
        reason: 'marker-free display must not normalize the canonical Markdown',
      );
      expect(harness.binding.controller.inputIslandGlobalStartUtf16, codeStart);
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16,
        codeStart + initialCode.length,
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('    alpha')),
        reason: 'the parser-authored four-column prefixes stay out of display',
      );
      _expectIndentedCodeStyle(tester, harness);

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;
      final initialRevision = harness.runtime.sourceRevision;

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'alpha\n  beta\n',
          textInserted: '\n',
          insertionOffset: 5,
          selection: TextSelection.collapsed(offset: 6),
          composing: TextRange.empty,
        ),
      ]);

      const editedCode = '    alpha\n    \n      beta\n';
      const editedSource = 'before\n\n$editedCode\n*tail*';
      expect(harness.runtime.sourceRevision, initialRevision + 1);
      expect(
        harness.runtime.exportMarkdown(),
        editedSource,
        reason:
            'display Enter maps to one canonical line ending plus the '
            'parser-selected four-space continuation prefix',
      );
      expect(
        harness.binding.controller.editingController.text,
        'alpha\n\n  beta\n',
      );
      expect(
        harness.binding.controller.globalEditingState.selection,
        TextSelection.collapsed(offset: codeStart + 14),
        reason:
            'the display caret after Enter maps beyond the newly hidden '
            'continuation prefix',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      _expectIndentedCodeStyle(tester, harness);

      final recertified = await harness.waitForIndentedCode(
        tester,
        expectedDisplay: 'alpha\n\n  beta\n',
      );
      expect(recertified.structure.indentedCode!.lineCount, 3);
      final recertifiedRecords = recertified.indentedCodeProjection!.records;
      expect(recertifiedRecords, hasLength(3));
      expect(recertifiedRecords[1].isInternalBlank, isTrue);
      expect(harness.sourceText(recertifiedRecords[1].hiddenPrefix), '    ');
      expect(harness.sourceText(recertifiedRecords[1].content), isEmpty);
      expect(harness.sourceText(recertified.structure.source), editedCode);
      expect(recertified.sourceRevision, harness.runtime.sourceRevision);
      expect(recertified.structureRevision, harness.runtime.sourceRevision);
      expect(
        recertified.indentedCodeProjection!.sourceVersion.revision,
        harness.runtime.sourceRevision,
      );
      expect(harness.runtime.status.sourceCurrent, isTrue);
      expect(harness.runtime.status.structureCurrent, isTrue);
      expect(
        harness.runtime.status.certifiedSourceRevision,
        harness.runtime.sourceRevision,
      );
      expect(
        harness.runtime.status.structureRevision,
        harness.runtime.sourceRevision,
      );
      expect(harness.runtime.exportMarkdown(), editedSource);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      _expectIndentedCodeStyle(tester, harness);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );
}

final class _ManagedIndentedCodeHarness {
  _ManagedIndentedCodeHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_ManagedIndentedCodeHarness> mount(
    WidgetTester tester, {
    required String source,
    required int caretUtf16,
  }) async {
    final runtime = await runManagedRuntimeAsyncForTest(
      tester,
      () => openManagedRuntimeForTest(source),
    );
    final binding = FlarkV3ManagedFlutterBinding.attach(
      runtime: runtime,
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 128,
        value: TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(offset: caretUtf16),
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
    final harness = _ManagedIndentedCodeHarness._(
      runtime: runtime,
      binding: binding,
      editableKey: editableKey,
      focusNode: focusNode,
    );
    addTearDown(() async {
      binding.dispose();
      focusNode.dispose();
      if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
        await runManagedRuntimeAsyncForTest(
          tester,
          () => runtime.close().timeout(const Duration(seconds: 5)),
        );
      }
    });

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SingleChildScrollView(
          child: FlarkV3LiveEditorPrototype(
            controller: binding.controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      ),
    );
    await runManagedRuntimeAsyncForTest(
      tester,
      () => runtime.initialReady.timeout(const Duration(seconds: 5)),
    );
    return harness;
  }

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3ManagedFlutterBinding binding;
  final GlobalKey<EditableTextState> editableKey;
  final FocusNode focusNode;

  EditableTextState get editableState => editableKey.currentState!;

  String sourceText(FlarkV3SourceSpan span) =>
      runtime.readSourceRange(span.startUtf16, span.endUtf16);

  Future<FlarkV3DocumentStructuralQuery> waitForIndentedCode(
    WidgetTester tester, {
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
      if (status.sourceCurrent &&
          status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.sourceRevision == status.sourceRevision &&
          query.structureRevision == status.sourceRevision &&
          query.structure.kind == FlarkV3DocumentStructureKind.indentedCode &&
          query.structure.indentedCode != null &&
          query.indentedCodeProjection != null &&
          binding.controller.editingController.text == expectedDisplay &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.blockStyleLease?.kind ==
              FlarkV3FlutterBlockStyleKind.indentedCode) {
        return query;
      }
    }
    final status = runtime.status;
    final query = binding.controller.paintState.documentQuery;
    throw TestFailure(
      'Timed out waiting for managed indented-code projection: '
      'revision=${status.sourceRevision}, '
      'certified=${status.certifiedSourceRevision}, '
      'sourceCurrent=${status.sourceCurrent}, '
      'structure=${status.structureRevision}, '
      'structureCurrent=${status.structureCurrent}, '
      'leafProjection=${status.leafProjectionPresentationGeneration}/'
      '${status.leafProjectionAttemptOutcomeGeneration}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'style=${binding.controller.paintState.blockStyleLease?.kind.name}, '
      'query=${query.runtimeType}, '
      'queryKind=${query is FlarkV3DocumentStructuralQuery ? query.structure.kind.name : '-'}, '
      'payload=${query is FlarkV3DocumentStructuralQuery ? query.indentedCodeProjection != null : false}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'text=${binding.controller.editingController.text}.',
    );
  }
}

void _expectIndentedCodeStyle(
  WidgetTester tester,
  _ManagedIndentedCodeHarness harness,
) {
  expect(
    harness.binding.controller.paintState.blockStyleLease?.kind,
    FlarkV3FlutterBlockStyleKind.indentedCode,
    reason: 'the parser-authored variant-7 block selects code typography',
  );
  expect(
    tester
        .widget<EditableText>(find.byKey(harness.editableKey))
        .style
        .fontFamily,
    'monospace',
  );
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedIndentedCodeHarness harness, {
  required EditableTextState editableState,
  required int setClientCount,
  required int clientId,
}) {
  expect(harness.editableKey.currentState, same(editableState));
  expect(_setClientCalls(tester), hasLength(setClientCount));
  expect(
    (_setClientCalls(tester).last.arguments as List<dynamic>).first,
    clientId,
  );
}

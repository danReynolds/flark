import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'real block quote hides prefixes, preserves one client, and continues Enter',
    (tester) async {
      const initialQuote = '> alpha\n> beta\nlazy\n';
      const initialSource = 'before\n\n$initialQuote\n*tail*';
      final quoteStart = initialSource.indexOf(initialQuote);
      final harness = await _ManagedBlockQuoteHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: quoteStart + '> al'.length,
      );

      final initialQuery = await harness.waitForBlockQuote(
        tester,
        expectedDisplay: 'alpha\nbeta\nlazy\n',
      );
      final facts = initialQuery.structure.blockQuote!;
      final payload = initialQuery.blockQuoteProjection!;
      expect(facts.lineCount, 3);
      expect(facts.childFirstLine, 0);
      expect(facts.childLineCount, 3);
      expect(payload.records, hasLength(3));
      expect(payload.pointPath.nodes, hasLength(2));
      expect(
        payload.pointPath.blockQuoteAncestor.kind,
        FlarkV3DocumentPointPathNodeKind.blockQuote,
      );
      expect(
        payload.pointPath.selectedLeaf.kind,
        FlarkV3DocumentPointPathNodeKind.paragraph,
      );
      expect(harness.sourceText(payload.records[0].hiddenPrefix), '> ');
      expect(harness.sourceText(payload.records[1].hiddenPrefix), '> ');
      expect(payload.records[2].isLazyContinuation, isTrue);
      expect(harness.sourceText(payload.records[2].hiddenPrefix), isEmpty);
      expect(harness.sourceText(initialQuery.structure.source), initialQuote);
      expect(harness.runtime.exportMarkdown(), initialSource);
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        quoteStart,
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16,
        quoteStart + initialQuote.length,
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('> ')),
      );
      _expectBlockQuotePresentation(tester, harness);

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
          oldText: 'alpha\nbeta\nlazy\n',
          textInserted: '\n',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);

      const editedQuote = '> al\n> pha\n> beta\nlazy\n';
      const editedSource = 'before\n\n$editedQuote\n*tail*';
      expect(harness.runtime.sourceRevision, initialRevision + 1);
      expect(
        harness.runtime.exportMarkdown(),
        editedSource,
        reason:
            'display Enter must insert a canonical newline and quote prefix',
      );
      expect(
        harness.binding.controller.editingController.text,
        'al\npha\nbeta\nlazy\n',
      );
      expect(
        harness.binding.controller.globalEditingState.selection,
        TextSelection.collapsed(offset: quoteStart + 7),
        reason: 'the source caret lands after the newly hidden `> ` prefix',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      _expectBlockQuotePresentation(tester, harness);

      final recertified = await harness.waitForBlockQuote(
        tester,
        expectedDisplay: 'al\npha\nbeta\nlazy\n',
      );
      expect(recertified.structure.blockQuote!.lineCount, 4);
      expect(recertified.blockQuoteProjection!.records, hasLength(4));
      expect(
        recertified.blockQuoteProjection!.records.last.isLazyContinuation,
        isTrue,
      );
      expect(recertified.sourceRevision, harness.runtime.sourceRevision);
      expect(recertified.structureRevision, harness.runtime.sourceRevision);
      expect(
        recertified.blockQuoteProjection!.sourceVersion.revision,
        harness.runtime.sourceRevision,
      );
      expect(harness.runtime.status.sourceCurrent, isTrue);
      expect(harness.runtime.status.structureCurrent, isTrue);
      expect(harness.runtime.exportMarkdown(), editedSource);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      _expectBlockQuotePresentation(tester, harness);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );
}

final class _ManagedBlockQuoteHarness {
  _ManagedBlockQuoteHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_ManagedBlockQuoteHarness> mount(
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
    final harness = _ManagedBlockQuoteHarness._(
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

  Future<FlarkV3DocumentStructuralQuery> waitForBlockQuote(
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
          query.structure.kind == FlarkV3DocumentStructureKind.blockQuote &&
          query.structure.blockQuote != null &&
          query.pointPath != null &&
          query.blockQuoteProjection != null &&
          binding.controller.editingController.text == expectedDisplay &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.blockStyleLease?.kind ==
              FlarkV3FlutterBlockStyleKind.blockQuote) {
        return query;
      }
    }
    final status = runtime.status;
    final query = binding.controller.paintState.documentQuery;
    throw TestFailure(
      'Timed out waiting for managed block-quote projection: '
      'state=${status.state.name}, '
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
      'payload=${query is FlarkV3DocumentStructuralQuery ? query.blockQuoteProjection != null : false}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'text=${binding.controller.editingController.text}.',
    );
  }
}

void _expectBlockQuotePresentation(
  WidgetTester tester,
  _ManagedBlockQuoteHarness harness,
) {
  expect(
    harness.binding.controller.paintState.blockStyleLease?.kind,
    FlarkV3FlutterBlockStyleKind.blockQuote,
  );
  expect(
    tester.widget<EditableText>(find.byKey(harness.editableKey)).style.color,
    const Color(0xFF4B5563),
  );
  expect(
    find.byWidgetPredicate((widget) => widget is EditableText),
    findsOneWidget,
    reason: 'quote presentation must retain one EditableText',
  );
  final rail = tester.widget<Container>(
    find.byKey(const Key('flark-v3-block-quote-rail')),
  );
  final decoration = rail.decoration! as BoxDecoration;
  final border = decoration.border! as Border;
  expect(border.left.width, 3);
  expect(border.left.color, const Color(0xFFCBD5E1));
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedBlockQuoteHarness harness, {
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

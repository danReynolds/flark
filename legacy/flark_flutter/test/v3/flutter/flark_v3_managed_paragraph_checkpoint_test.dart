import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'real middle Paragraph renders marker-free and moves on one input client',
    (tester) async {
      const source =
          '*first*\n\n'
          '**middle** and _em_.\n\n'
          '`tail`';
      final middlePoint = source.indexOf('middle') + 2;
      final harness = await _ManagedParagraphHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: middlePoint,
        maximumIslandUtf16: 128,
      );

      final middle = await harness.waitForParagraph(
        tester,
        containingSource: '**middle**',
        expectedDisplay: 'middle and em.\n',
      );
      expect(middle.inlineFacts, isNotNull);
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('**')),
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('_')),
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;
      final initialSetClientCount = _setClientCalls(tester).length;

      const middleDisplay = 'middle and em.\n';
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: middleDisplay,
          textInserted: '!',
          insertionOffset: 6,
          selection: TextSelection.collapsed(offset: 7),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(
        harness.runtime.exportMarkdown(),
        '*first*\n\n**middle**! and _em_.\n\n`tail`',
      );
      await harness.waitForParagraph(
        tester,
        containingSource: '**middle**!',
        expectedDisplay: 'middle! and em.\n',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      final tailPoint = harness.runtime.exportMarkdown().indexOf('tail') + 2;
      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: tailPoint),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForParagraph(
        tester,
        containingSource: '`tail`',
        expectedDisplay: 'tail',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      final firstPoint = harness.runtime.exportMarkdown().indexOf('first') + 2;
      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: firstPoint),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForParagraph(
        tester,
        containingSource: '*first*',
        expectedDisplay: 'first\n',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      final generationAfterThreeLeaves =
          harness.runtime.status.inlinePresentationGeneration;

      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: middlePoint),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForParagraph(
        tester,
        containingSource: '**middle**!',
        expectedDisplay: 'middle! and em.\n',
      );
      expect(
        harness.runtime.status.inlinePresentationGeneration,
        generationAfterThreeLeaves,
        reason:
            'returning to a current-revision cached leaf must not rebuild or '
            'move the host sidecar',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      expect(
        harness.runtime.exportMarkdown(),
        '*first*\n\n**middle**! and _em_.\n\n`tail`',
        reason:
            'moving the selected presentation leaf never rewrites canonical '
            'Markdown',
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'oversized Paragraph stays bounded and source-visible without inline demand',
    (tester) async {
      final body = List<String>.generate(
        700,
        (index) => 'segment-$index **literal** ',
        growable: false,
      ).join();
      final source = 'prefix\n\n$body\n\ntail';
      final bodyStart = 'prefix\n\n'.length;
      final caret = bodyStart + body.length ~/ 2;
      const maximumIslandUtf16 = 8192;
      final islandStart = caret - maximumIslandUtf16 ~/ 2;
      final islandEnd = islandStart + maximumIslandUtf16;
      final harness = await _ManagedParagraphHarness.mount(
        tester,
        source: source,
        islandStartUtf16: islandStart,
        islandEndUtf16: islandEnd,
        caretUtf16: caret,
        maximumIslandUtf16: maximumIslandUtf16,
      );
      final initialInlineOutcome =
          harness.runtime.status.inlineAttemptOutcomeGeneration;

      final query = await harness.waitForParagraph(
        tester,
        containingSource: '**literal**',
        requireCertifiedInline: false,
      );
      expect(
        query.projection.projectedSource.endUtf16 -
            query.projection.projectedSource.startUtf16,
        greaterThan(maximumIslandUtf16),
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16 -
            harness.binding.controller.inputIslandGlobalStartUtf16,
        lessThanOrEqualTo(maximumIslandUtf16),
      );
      expect(
        harness.binding.controller.editingController.text,
        contains('**literal**'),
        reason:
            'an oversized whole Paragraph remains exact source paint until '
            'windowed parser facts exist',
      );
      expect(
        harness.binding.controller.hasProjectedInlinePresentation,
        isFalse,
      );
      expect(
        harness.binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );

      for (var turn = 0; turn < 10; turn += 1) {
        harness.binding.controller.handoffInputIsland(
          FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(offset: caret + turn),
            composing: TextRange.empty,
          ),
        );
        await tester.pump();
      }
      expect(
        harness.runtime.status.inlineAttemptOutcomeGeneration,
        initialInlineOutcome,
        reason:
            'selection churn in an ineligible leaf must not repeatedly build '
            'unconsumable whole-leaf sidecars',
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'indented hard-break continuation stays source-visible and fail-closed',
    (tester) async {
      const source = 'before\\\n after';
      final harness = await _ManagedParagraphHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: source.indexOf('after'),
        maximumIslandUtf16: 128,
      );

      final query = await harness.waitForParagraph(
        tester,
        containingSource: source,
        expectedDisplay: source,
        requireCertifiedInline: false,
      );
      expect(query.inlineFacts, isNull);
      expect(
        harness.binding.controller.editingController.text,
        source,
        reason:
            'a parser-rejected indented continuation must remain exact source '
            'instead of being guessed into a hard break',
      );
      expect(
        harness.binding.controller.hasProjectedInlinePresentation,
        isFalse,
      );
      expect(
        harness.binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(harness.runtime.exportMarkdown(), source);
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'escaped punctuation is marker-free and replaces its source atom',
    (tester) async {
      const source = r'before \* after';
      final harness = await _ManagedParagraphHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: source.indexOf('*'),
        maximumIslandUtf16: 128,
      );

      final initial = await harness.waitForParagraph(
        tester,
        containingSource: r'\*',
        expectedDisplay: 'before * after',
      );
      expect(initial.inlineFacts, isNotNull);
      expect(initial.inlineFacts!.facts.map((fact) => fact.kind), [
        FlarkV3InlineFactKind.escapedPunctuation,
      ]);
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains(r'\')),
      );

      final editableState = harness.editableState;
      editableState.requestKeyboard();
      await tester.pump();
      final setClientCallsBefore = _setClientCalls(tester);
      final clientId =
          (setClientCallsBefore.last.arguments as List<dynamic>).first as int;
      final star = editableState.widget.controller.text.indexOf('*');
      expect(star, greaterThanOrEqualTo(0));

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaReplacement(
          oldText: editableState.widget.controller.text,
          replacementText: 'x',
          replacedRange: TextRange(start: star, end: star + 1),
          selection: TextSelection.collapsed(offset: star + 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();

      expect(
        harness.runtime.exportMarkdown(),
        'before x after',
        reason:
            'replacing the visible escaped byte must consume the certified '
            r'backslash atom rather than leave \x',
      );
      final recertified = await harness.waitForParagraph(
        tester,
        containingSource: 'before x after',
        expectedDisplay: 'before x after',
      );
      expect(recertified.inlineFacts, isNotNull);
      expect(recertified.inlineFacts!.facts, isEmpty);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: setClientCallsBefore.length,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'soft-hard line break transitions stay live on one input client',
    (tester) async {
      const source = 'a\nb';
      final harness = await _ManagedParagraphHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: 1,
        maximumIslandUtf16: 64,
      );

      final initial = await harness.waitForParagraph(
        tester,
        containingSource: source,
        expectedDisplay: source,
      );
      expect(initial.inlineFacts, isNotNull);
      expect(initial.inlineFacts!.facts, isEmpty);

      final editableState = harness.editableState;
      editableState.requestKeyboard();
      await tester.pump();
      final setClientCallsBefore = _setClientCalls(tester);
      final clientId =
          (setClientCallsBefore.last.arguments as List<dynamic>).first as int;

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: source,
          textInserted: ' ',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      ]);
      final soft = await harness.waitForParagraph(
        tester,
        containingSource: 'a \nb',
        expectedDisplay: 'a \nb',
      );
      expect(soft.inlineFacts, isNotNull);
      expect(soft.inlineFacts!.facts, isEmpty);

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'a \nb',
          textInserted: ' ',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);
      final hard = await harness.waitForParagraph(
        tester,
        containingSource: 'a  \nb',
        expectedDisplay: source,
      );
      expect(
        hard.inlineFacts!.facts.map((fact) => fact.kind),
        contains(FlarkV3InlineFactKind.hardLineBreak),
      );

      var hardSource = true;
      for (var turn = 0; turn < 8; turn += 1) {
        final edit = hardSource
            ? const FlarkV3SourceEdit(
                startUtf16: 1,
                endUtf16: 2,
                replacement: '',
              )
            : const FlarkV3SourceEdit(
                startUtf16: 2,
                endUtf16: 2,
                replacement: ' ',
              );
        harness.runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: harness.runtime.sourceRevision,
            operation: edit,
          ),
        );
        hardSource = !hardSource;
      }
      expect(hardSource, isTrue);
      expect(harness.runtime.exportMarkdown(), 'a  \nb');

      final settled = await harness.waitForParagraph(
        tester,
        containingSource: 'a  \nb',
        expectedDisplay: source,
      );
      expect(
        settled.inlineFacts!.facts.map((fact) => fact.kind),
        contains(FlarkV3InlineFactKind.hardLineBreak),
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: setClientCallsBefore.length,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

final class _ManagedParagraphHarness {
  _ManagedParagraphHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_ManagedParagraphHarness> mount(
    WidgetTester tester, {
    required String source,
    required int islandStartUtf16,
    required int islandEndUtf16,
    required int caretUtf16,
    required int maximumIslandUtf16,
  }) async {
    final runtime = await runManagedRuntimeAsyncForTest(
      tester,
      () => openManagedRuntimeForTest(source),
    );
    final binding = FlarkV3ManagedFlutterBinding.attach(
      runtime: runtime,
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: islandStartUtf16,
        maximumUtf16: maximumIslandUtf16,
        value: TextEditingValue(
          text: source.substring(islandStartUtf16, islandEndUtf16),
          selection: TextSelection.collapsed(
            offset: caretUtf16 - islandStartUtf16,
          ),
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
    final harness = _ManagedParagraphHarness._(
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

  Future<FlarkV3DocumentStructuralQuery> waitForParagraph(
    WidgetTester tester, {
    required String containingSource,
    String? expectedDisplay,
    bool requireCertifiedInline = true,
  }) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 5)) {
      await tester.pump(const Duration(milliseconds: 1));
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final query = binding.controller.paintState.documentQuery;
      if (runtime.status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.structure.kind == FlarkV3DocumentStructureKind.paragraph) {
        final sourceText = runtime.readSourceRange(
          query.projection.projectedSource.startUtf16,
          query.projection.projectedSource.endUtf16,
        );
        if (sourceText.contains(containingSource) &&
            (!requireCertifiedInline ||
                (binding.controller.hasCertifiedInlinePresentation &&
                    query.inlineFacts != null)) &&
            (expectedDisplay == null ||
                binding.controller.editingController.text == expectedDisplay)) {
          return query;
        }
      }
    }
    final query = binding.controller.paintState.documentQuery;
    throw TestFailure(
      'Timed out waiting for managed Paragraph projection: '
      'revision=${runtime.sourceRevision}, '
      'sourceCurrent=${runtime.status.sourceCurrent}, '
      'structureCurrent=${runtime.status.structureCurrent}, '
      'inline=${runtime.status.inlinePresentationGeneration}/'
      '${runtime.status.inlineAttemptOutcomeGeneration}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${query.runtimeType}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'text=${binding.controller.editingController.text}.',
    );
  }
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedParagraphHarness harness, {
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

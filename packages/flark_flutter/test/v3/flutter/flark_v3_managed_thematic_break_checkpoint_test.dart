import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'real thematic break is marker-free, source-exact, and affinity-aware',
    (tester) async {
      final semantics = tester.ensureSemantics();

      const atom = '  * \t* *  \r\n';
      const source = 'before\n\n$atom\nafter';
      final atomStart = source.indexOf(atom);
      final atomEnd = atomStart + atom.length;
      final harness = await _ManagedThematicBreakHarness.mount(
        tester,
        source: source,
        caretUtf16: atomStart + 3,
        affinity: TextAffinity.upstream,
      );

      final query = await harness.waitForThematicBreak(
        tester,
        expectedBoundaryUtf16: atomStart,
        expectedBoundaryAffinity: TextAffinity.downstream,
      );
      final structure = query.structure;
      final facts = structure.thematicBreak!;
      expect(facts.marker, FlarkV3ThematicBreakMarker.asterisk);
      expect(facts.markerCount, 3);
      expect(facts.openingIndent, 2);
      expect(facts.hasBofBom, isFalse);
      expect(harness.sourceText(structure.source), atom);
      expect(harness.sourceText(facts.markerEnvelope), '* \t* *');
      expect(harness.sourceText(facts.lineEnding), '\r\n');
      expect(structure.visibleSource.startUtf16, atomStart);
      expect(structure.visibleSource.endUtf16, atomStart);
      expect(query.projection.projectedSource.startUtf16, atomStart);
      expect(query.projection.projectedSource.endUtf16, atomStart);
      expect(query.projection.runCount, 0);
      expect(harness.runtime.exportMarkdown(), source);
      expect(harness.binding.controller.editingController.text, isEmpty);
      expect(
        harness.binding.controller.paintState.atomicBlockLease,
        FlarkV3FlutterAtomicBlockLease.thematicBreak(source: structure.source),
      );
      expect(find.byKey(const Key('flark-v3-thematic-break')), findsOneWidget);
      expect(find.bySemanticsLabel('Thematic break'), findsOneWidget);
      final dividerSize = tester.getSize(
        find.byKey(const Key('flark-v3-thematic-break')),
      );
      expect(dividerSize.height, 1);
      expect(
        dividerSize.width,
        greaterThan(0),
        reason: 'the parser-certified atom must paint a visible divider',
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final setClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(
            offset: atomStart + 3,
            affinity: TextAffinity.downstream,
          ),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForThematicBreak(
        tester,
        expectedBoundaryUtf16: atomEnd,
        expectedBoundaryAffinity: TextAffinity.upstream,
      );
      expect(harness.runtime.exportMarkdown(), source);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: setClientCount,
        clientId: clientId,
      );
      semantics.dispose();
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'typing fails semantics closed and both delete keys remove the whole atom',
    (tester) async {
      final semantics = tester.ensureSemantics();

      const source = '---\nnext';
      final harness = await _ManagedThematicBreakHarness.mount(
        tester,
        source: source,
        caretUtf16: 1,
        affinity: TextAffinity.upstream,
      );
      await harness.waitForThematicBreak(
        tester,
        expectedBoundaryUtf16: 0,
        expectedBoundaryAffinity: TextAffinity.downstream,
      );
      expect(find.bySemanticsLabel('Thematic break'), findsOneWidget);

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final setClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: '',
          textInserted: 'x',
          insertionOffset: 0,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      expect(harness.runtime.exportMarkdown(), 'x---\nnext');
      expect(
        harness.binding.controller.semanticActionsValid,
        isFalse,
        reason:
            'source advance must synchronously revoke the old atomic authority',
      );
      harness.frameScheduler.flushAll();
      await tester.pump();
      expect(find.bySemanticsLabel('Thematic break'), findsNothing);
      expect(find.byKey(const Key('flark-v3-thematic-break')), findsNothing);

      await harness.waitForParagraph(tester, expectedSource: 'x---\nnext');
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: setClientCount,
        clientId: clientId,
      );
      semantics.dispose();

      expect(harness.runtime.undo(), isNotNull);
      await harness.waitForThematicBreak(tester);
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      expect(
        harness.runtime.exportMarkdown(),
        'next',
        reason: 'Backspace must delete the complete canonical marker line',
      );
      await harness.waitForParagraph(tester, expectedSource: 'next');
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: setClientCount,
        clientId: clientId,
      );

      expect(harness.runtime.undo(), isNotNull);
      await harness.waitForThematicBreak(tester);
      await tester.sendKeyEvent(LogicalKeyboardKey.delete);
      expect(
        harness.runtime.exportMarkdown(),
        'next',
        reason: 'Delete must delete the complete canonical marker line',
      );
      await harness.waitForParagraph(tester, expectedSource: 'next');
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: setClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

final class _ManagedThematicBreakHarness {
  _ManagedThematicBreakHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
    required this.frameScheduler,
  });

  static Future<_ManagedThematicBreakHarness> mount(
    WidgetTester tester, {
    required String source,
    required int caretUtf16,
    required TextAffinity affinity,
  }) async {
    final runtime = await runManagedRuntimeAsyncForTest(
      tester,
      () => openManagedRuntimeForTest(source),
    );
    final frameScheduler = _ManualFrameScheduler();
    final binding = FlarkV3ManagedFlutterBinding.attach(
      runtime: runtime,
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 256,
        value: TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(
            offset: caretUtf16,
            affinity: affinity,
          ),
        ),
      ),
      queryBudget: FlarkV3HostQueryBudget(
        maxEncodedBytes: 16 * 1024,
        maxOpenDepth: 64,
        maxLeafCount: 256,
        maxTreeNodesVisited: 1024,
      ),
      frameScheduler: frameScheduler,
    );
    final editableKey = GlobalKey<EditableTextState>();
    final focusNode = FocusNode();
    final harness = _ManagedThematicBreakHarness._(
      runtime: runtime,
      binding: binding,
      editableKey: editableKey,
      focusNode: focusNode,
      frameScheduler: frameScheduler,
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
        child: Center(
          child: SizedBox(
            width: 320,
            child: FlarkV3LiveEditorPrototype(
              controller: binding.controller,
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
    return harness;
  }

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3ManagedFlutterBinding binding;
  final GlobalKey<EditableTextState> editableKey;
  final FocusNode focusNode;
  final _ManualFrameScheduler frameScheduler;

  EditableTextState get editableState => editableKey.currentState!;

  String sourceText(FlarkV3SourceSpan span) =>
      runtime.readSourceRange(span.startUtf16, span.endUtf16);

  Future<FlarkV3DocumentStructuralQuery> waitForThematicBreak(
    WidgetTester tester, {
    int? expectedBoundaryUtf16,
    TextAffinity? expectedBoundaryAffinity,
  }) async {
    if ((expectedBoundaryUtf16 == null) != (expectedBoundaryAffinity == null)) {
      throw ArgumentError(
        'Boundary position and affinity must be asserted together.',
      );
    }
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 10)) {
      await _pumpRuntime(tester);
      final query = binding.controller.paintState.documentQuery;
      if (runtime.status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.structure.kind == FlarkV3DocumentStructureKind.thematicBreak &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.atomicBlockLease != null &&
          binding.controller.editingController.text.isEmpty &&
          (expectedBoundaryUtf16 == null ||
              (binding.controller.inputIslandGlobalStartUtf16 ==
                      expectedBoundaryUtf16 &&
                  binding.controller.inputIslandGlobalEndUtf16 ==
                      expectedBoundaryUtf16 &&
                  binding.controller.globalEditingState.selection ==
                      TextSelection.collapsed(
                        offset: expectedBoundaryUtf16,
                        affinity: expectedBoundaryAffinity!,
                      ) &&
                  binding.controller.editingController.selection ==
                      TextSelection.collapsed(
                        offset: 0,
                        affinity: expectedBoundaryAffinity,
                      )))) {
        return query;
      }
    }
    throw TestFailure(
      'Timed out waiting for managed thematic break: '
      'revision=${runtime.sourceRevision}, '
      'sourceCurrent=${runtime.status.sourceCurrent}, '
      'structureCurrent=${runtime.status.structureCurrent}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${binding.controller.paintState.documentQuery.runtimeType}, '
      'atom=${binding.controller.paintState.atomicBlockLease}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'text=${binding.controller.editingController.text}, '
      'source=${runtime.exportMarkdown()}.',
    );
  }

  Future<FlarkV3DocumentStructuralQuery> waitForParagraph(
    WidgetTester tester, {
    required String expectedSource,
  }) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 10)) {
      await _pumpRuntime(tester);
      final query = binding.controller.paintState.documentQuery;
      if (runtime.status.structureCurrent &&
          runtime.exportMarkdown() == expectedSource &&
          query is FlarkV3DocumentStructuralQuery &&
          query.structure.kind == FlarkV3DocumentStructureKind.paragraph &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.atomicBlockLease == null) {
        return query;
      }
    }
    throw TestFailure(
      'Timed out waiting for managed Paragraph after atomic edit: '
      'revision=${runtime.sourceRevision}, '
      'sourceCurrent=${runtime.status.sourceCurrent}, '
      'structureCurrent=${runtime.status.structureCurrent}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${binding.controller.paintState.documentQuery.runtimeType}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'text=${binding.controller.editingController.text}, '
      'source=${runtime.exportMarkdown()}.',
    );
  }

  Future<void> _pumpRuntime(WidgetTester tester) async {
    frameScheduler.flushAll();
    await tester.pump(const Duration(milliseconds: 1));
    await runManagedRuntimeAsyncForTest(
      tester,
      () => Future<void>.delayed(const Duration(milliseconds: 1)),
    );
    frameScheduler.flushAll();
    await tester.pump();
  }
}

final class _ManualFrameScheduler implements FlarkV3FrameScheduler {
  final List<VoidCallback> _callbacks = <VoidCallback>[];

  @override
  void schedule(VoidCallback callback) {
    _callbacks.add(callback);
  }

  void flushAll() {
    var turns = 0;
    while (_callbacks.isNotEmpty) {
      if (turns >= 100) {
        throw StateError('Managed Flutter frame callbacks did not converge.');
      }
      turns += 1;
      _callbacks.removeAt(0)();
    }
  }
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedThematicBreakHarness harness, {
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

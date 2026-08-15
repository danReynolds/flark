import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'real ATX heading stays marker-free, source-exact, and on one input client',
    (tester) async {
      const initialHeading = '# **β😀** ###\r\n';
      const initialSource = 'before\n\n$initialHeading\r\n*tail*';
      final openingMarkerOffset = initialSource.indexOf('#');
      final harness = await _ManagedHeadingHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: openingMarkerOffset,
      );

      final initialQuery = await harness.waitForHeading(
        tester,
        expectedDisplay: 'β😀',
      );
      final initialFacts =
          initialQuery.structure.heading! as FlarkV3AtxHeadingFacts;
      expect(initialFacts.level, 1);
      expect(
        harness.sourceText(initialFacts.openingMarker),
        '#',
        reason: 'Flutter consumes parser-authored marker geometry only',
      );
      expect(harness.sourceText(initialFacts.contentSource), '**β😀**');
      expect(harness.sourceText(initialFacts.closingMarker!), '###');
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        initialFacts.contentSource.startUtf16,
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16,
        initialFacts.contentSource.endUtf16,
      );
      expect(
        harness.binding.controller.globalEditingState.selection,
        TextSelection.collapsed(offset: initialFacts.contentSource.startUtf16),
        reason:
            'an initial source caret on the hidden ATX opener clamps to the '
            'parser-certified content boundary',
      );
      _expectHeadingStyle(tester, harness, fontSize: 28);

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaNonTextUpdate(
          oldText: 'β😀',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);
      expect(
        harness.binding.controller.globalEditingState.selection.extentOffset,
        initialFacts.contentSource.endUtf16 - 2,
        reason:
            'the visible end caret maps before the hidden strong closer '
            'without splitting the emoji scalar',
      );

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'β😀',
          textInserted: '!',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      ]);
      harness.frameScheduler.flushAll();
      expect(
        harness.binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.stablePaint,
      );
      expect(harness.binding.controller.semanticActionsValid, isFalse);
      await tester.pump();
      _expectHeadingStyle(tester, harness, fontSize: 28);
      expect(
        harness.runtime.exportMarkdown(),
        'before\n\n# **β!😀** ###\r\n\r\n*tail*',
      );
      expect(
        harness.binding.controller.globalEditingState.selection.extentOffset,
        harness.runtime.exportMarkdown().indexOf('!') + 1,
        reason: 'display insertion maps exactly through hidden delimiters',
      );
      await harness.waitForHeading(tester, expectedDisplay: 'β!😀');
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'β!😀',
          textInserted: 'に',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 2, end: 3),
        ),
      ]);
      await tester.pump();
      final composingStart = harness.runtime.exportMarkdown().indexOf('に');
      expect(
        harness.binding.controller.globalEditingState.composing,
        TextRange(start: composingStart, end: composingStart + 1),
      );
      expect(
        harness.binding.controller.editingController.value,
        const TextEditingValue(
          text: 'β!に😀',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 2, end: 3),
        ),
        reason:
            'parser progress must not rewrite an active platform composition',
      );
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaNonTextUpdate(
          oldText: 'β!に😀',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);
      await harness.waitForHeading(tester, expectedDisplay: 'β!に😀');

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaReplacement(
          oldText: 'β!に😀',
          replacementText: 'paste',
          replacedRange: TextRange(start: 1, end: 3),
          selection: TextSelection.collapsed(offset: 6),
          composing: TextRange.empty,
        ),
      ]);
      await harness.waitForHeading(tester, expectedDisplay: 'βpaste😀');
      expect(
        harness.runtime.exportMarkdown(),
        'before\n\n# **βpaste😀** ###\r\n\r\n*tail*',
        reason: 'a platform paste edits content without replacing ATX markers',
      );

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaNonTextUpdate(
          oldText: 'βpaste😀',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      expect(harness.runtime.undo(), isNotNull);
      await harness.waitForHeading(tester, expectedDisplay: 'β!に😀');
      expect(
        harness.runtime.exportMarkdown(),
        'before\n\n# **β!に😀** ###\r\n\r\n*tail*',
        reason: 'source history rehydrates the same parser-certified heading',
      );

      final tailPoint = harness.runtime.exportMarkdown().indexOf('tail') + 2;
      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: tailPoint),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForParagraph(tester, expectedDisplay: 'tail');
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      final paragraphStyle = tester
          .widget<EditableText>(find.byKey(harness.editableKey))
          .style;
      expect(paragraphStyle.fontSize, 14);
      expect(paragraphStyle.fontWeight, isNull);

      final headingPoint = harness.runtime.exportMarkdown().indexOf('β');
      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: headingPoint),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForHeading(tester, expectedDisplay: 'β!に😀');
      _expectHeadingStyle(tester, harness, fontSize: 28);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
      expect(
        tester.testTextInput.hasAnyClients,
        isFalse,
        reason: 'unmounting the stable editable closes its platform client',
      );
      expect(harness.binding.isDisposed, isFalse);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'real Setext heading hides its underline and edits on one input client',
    (tester) async {
      const initialSource = 'before\n\nSetext **β😀**\r\n---\r\n\r\n*tail*';
      final underlineOffset = initialSource.indexOf('---');
      final harness = await _ManagedHeadingHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: underlineOffset + 1,
      );

      final initialQuery = await harness.waitForHeading(
        tester,
        expectedDisplay: 'Setext β😀',
      );
      final initialFacts =
          initialQuery.structure.heading! as FlarkV3SetextHeadingFacts;
      expect(initialFacts.level, 2);
      expect(initialFacts.openingIndent, 0);
      expect(harness.sourceText(initialFacts.contentSource), 'Setext **β😀**');
      expect(harness.sourceText(initialFacts.contentLineEnding), '\r\n');
      expect(harness.sourceText(initialFacts.underlineMarker), '---');
      expect(harness.sourceText(initialFacts.underlineLineEnding), '\r\n');
      expect(
        harness.binding.controller.editingController.text,
        'Setext β😀',
        reason: 'the parser-certified underline never enters display space',
      );
      _expectHeadingStyle(tester, harness, fontSize: 21);

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'Setext β😀',
          textInserted: '!',
          insertionOffset: 8,
          selection: TextSelection.collapsed(offset: 9),
          composing: TextRange.empty,
        ),
      ]);
      final edited = await harness.waitForHeading(
        tester,
        expectedDisplay: 'Setext β!😀',
      );
      expect(
        harness.runtime.exportMarkdown(),
        'before\n\nSetext **β!😀**\r\n---\r\n\r\n*tail*',
        reason:
            'display editing changes content without touching the underline',
      );
      final editedFacts =
          edited.structure.heading! as FlarkV3SetextHeadingFacts;
      expect(harness.sourceText(editedFacts.underlineMarker), '---');
      expect(harness.sourceText(editedFacts.underlineLineEnding), '\r\n');
      _expectHeadingStyle(tester, harness, fontSize: 21);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
      expect(tester.testTextInput.hasAnyClients, isFalse);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );
}

final class _ManagedHeadingHarness {
  _ManagedHeadingHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
    required this.frameScheduler,
  });

  static Future<_ManagedHeadingHarness> mount(
    WidgetTester tester, {
    required String source,
    required int caretUtf16,
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
          selection: TextSelection.collapsed(offset: caretUtf16),
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
    final harness = _ManagedHeadingHarness._(
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

  Future<FlarkV3DocumentStructuralQuery> waitForHeading(
    WidgetTester tester, {
    required String expectedDisplay,
  }) => _waitForStructure(
    tester,
    kind: FlarkV3DocumentStructureKind.heading,
    expectedDisplay: expectedDisplay,
    requireCertifiedInline: expectedDisplay.isNotEmpty,
  );

  Future<FlarkV3DocumentStructuralQuery> waitForParagraph(
    WidgetTester tester, {
    required String expectedDisplay,
  }) => _waitForStructure(
    tester,
    kind: FlarkV3DocumentStructureKind.paragraph,
    expectedDisplay: expectedDisplay,
    requireCertifiedInline: expectedDisplay.isNotEmpty,
  );

  Future<FlarkV3DocumentStructuralQuery> _waitForStructure(
    WidgetTester tester, {
    required FlarkV3DocumentStructureKind kind,
    required String expectedDisplay,
    required bool requireCertifiedInline,
  }) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 10)) {
      frameScheduler.flushAll();
      await tester.pump(const Duration(milliseconds: 1));
      await runManagedRuntimeAsyncForTest(
        tester,
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      frameScheduler.flushAll();
      final query = binding.controller.paintState.documentQuery;
      if (runtime.status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.structure.kind == kind &&
          binding.controller.editingController.text == expectedDisplay &&
          (!requireCertifiedInline ||
              (binding.controller.hasCertifiedInlinePresentation &&
                  query.inlineFacts != null))) {
        return query;
      }
    }
    throw TestFailure(
      'Timed out waiting for ${kind.name}: '
      'status=${runtime.status.state.name}, '
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

void _expectHeadingStyle(
  WidgetTester tester,
  _ManagedHeadingHarness harness, {
  required double fontSize,
}) {
  final style = tester
      .widget<EditableText>(find.byKey(harness.editableKey))
      .style;
  expect(style.fontSize, fontSize);
  expect(style.fontWeight, FontWeight.w700);
}

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedHeadingHarness harness, {
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

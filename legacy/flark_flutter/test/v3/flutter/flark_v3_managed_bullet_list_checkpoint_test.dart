import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'selected list item composes certified inline styles on one input client',
    (tester) async {
      const initialSource =
          '- **bold** *em* `code`\r\n'
          '- plain\r\n';
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: initialSource.indexOf('bold') + 2,
      );

      final query = await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'bold em code\n',
        expectedOrdinal: 0,
        requireInlineFacts: true,
      );
      expect(query.inlineFacts, isNotNull);
      expect(
        query.inlineFacts!.facts.map((fact) => fact.kind),
        containsAll(<FlarkV3InlineFactKind>[
          FlarkV3InlineFactKind.strong,
          FlarkV3InlineFactKind.emphasis,
          FlarkV3InlineFactKind.code,
        ]),
      );
      expect(query.bulletListProjection, isNotNull);
      expect(
        harness.binding.controller.editingController.text,
        'bold em code\n',
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(anyOf(contains('*'), contains('`'), contains('- '))),
      );
      _expectBulletPresentation(tester, harness);

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
          oldText: 'bold em code\n',
          textInserted: '!',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);

      expect(harness.runtime.sourceRevision, initialRevision + 1);
      expect(
        harness.runtime.exportMarkdown(),
        '- **bo!ld** *em* `code`\r\n- plain\r\n',
      );
      await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'bo!ld em code\n',
        expectedOrdinal: 0,
        requireInlineFacts: true,
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'ordered item hides its exact marker and continues numbering on one client',
    (tester) async {
      const initialSource = '007) alpha\r\n9) beta\r\n';
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: initialSource.indexOf('alpha') + 2,
      );

      final first = await harness.waitForOrderedItem(
        tester,
        expectedDisplay: 'alpha\n',
        expectedOrdinal: 0,
        expectedMarker: '007)',
      );
      expect(first.structure.orderedList!.start, 7);
      expect(
        first.structure.orderedList!.delimiter,
        FlarkV3OrderedListDelimiter.parenthesis,
      );
      expect(
        first.orderedListProjection!.editingInputs.continuationSourcePrefix,
        '008) ',
      );
      expect(harness.binding.controller.editingController.text, 'alpha\n');
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('007)')),
      );
      _expectOrderedPresentation(tester, harness, marker: '007)');

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
          oldText: 'alpha\n',
          textInserted: '\n',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);

      expect(harness.runtime.sourceRevision, initialRevision + 1);
      expect(
        harness.runtime.exportMarkdown(),
        '007) al\r\n008) pha\r\n9) beta\r\n',
      );
      final continued = await harness.waitForOrderedItem(
        tester,
        expectedDisplay: 'pha\n',
        expectedOrdinal: 1,
        expectedMarker: '008)',
      );
      expect(
        continued.orderedListProjection!.editingInputs.canonicalLineEnding,
        '\r\n',
      );
      _expectOrderedPresentation(tester, harness, marker: '008)');
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'ordered item normalizes Web Enter CR through continuation policy',
    (tester) async {
      const initialSource = '007) alpha\r\n9) beta\r\n';
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: initialSource.indexOf('alpha') + 2,
      );
      await harness.waitForOrderedItem(
        tester,
        expectedDisplay: 'alpha\n',
        expectedOrdinal: 0,
        expectedMarker: '007)',
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      (harness.editableState as DeltaTextInputClient)
          .updateEditingValueWithDeltas([
            const TextEditingDeltaInsertion(
              oldText: 'alpha\n',
              textInserted: '\r',
              insertionOffset: 2,
              selection: TextSelection.collapsed(offset: 3),
              composing: TextRange.empty,
            ),
          ]);

      expect(
        harness.runtime.exportMarkdown(),
        '007) al\r\n008) pha\r\n9) beta\r\n',
      );
      await harness.waitForOrderedItem(
        tester,
        expectedDisplay: 'pha\n',
        expectedOrdinal: 1,
        expectedMarker: '008)',
      );
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'real tight list hands off items and preserves CRLF, Unicode, and one client',
    (tester) async {
      const initialList = '  - α😀\r\n  - β\r\n';
      const initialSource = 'before\n\n$initialList';
      final listStart = initialSource.indexOf(initialList);
      final secondItemStart = initialSource.indexOf('  - β');
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: listStart + '  - α'.length,
      );

      final first = await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'α😀\n',
        expectedOrdinal: 0,
      );
      expect(first.structure.bulletList!.itemCount, 2);
      expect(first.structure.bulletList!.tight, isTrue);
      expect(first.bulletListProjection!.coversWholeList, isFalse);
      expect(first.bulletListProjection!.records, hasLength(1));
      expect(
        harness.sourceText(
          first.bulletListProjection!.records.first.hiddenPrefix,
        ),
        '  - ',
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('-')),
      );
      _expectBulletPresentation(tester, harness);

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      // A point move within the same structural list must requery the selected
      // item; reusing the whole-list cache would incorrectly retain ordinal 0.
      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: secondItemStart + 4),
          composing: TextRange.empty,
        ),
      );
      final second = await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'β\n',
        expectedOrdinal: 1,
      );
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        second.bulletListProjection!.selectedItem.physicalSource.startUtf16,
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: listStart + 4),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'α😀\n',
        expectedOrdinal: 0,
      );

      final initialRevision = harness.runtime.sourceRevision;
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: 'α😀\n',
          textInserted: '\n',
          insertionOffset: 1,
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        ),
      ]);

      const continuedSource = 'before\n\n  - α\r\n  - 😀\r\n  - β\r\n';
      expect(harness.runtime.sourceRevision, initialRevision + 1);
      expect(harness.runtime.exportMarkdown(), continuedSource);
      expect(
        harness.binding.controller.editingController.text,
        'α\n😀\n',
        reason: 'the provisional selected-item projection stays LF-only',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      _expectBulletPresentation(tester, harness);

      final recertified = await harness.waitForBulletItem(
        tester,
        expectedDisplay: '😀\n',
        expectedOrdinal: 1,
      );
      expect(recertified.structure.bulletList!.itemCount, 3);
      expect(
        recertified.bulletListProjection!.editingInputs.canonicalLineEnding,
        '\r\n',
      );
      expect(
        harness.binding.controller.editingController.selection,
        const TextSelection.collapsed(offset: 0),
      );

      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      expect(
        harness.runtime.exportMarkdown(),
        'before\n\n  - α\r\n😀\r\n  - β\r\n',
        reason:
            'column-zero Backspace removes only the parser-certified prefix',
      );
      expect(
        find.byKey(const Key('flark-v3-bullet-list-item-gutter')),
        findsNothing,
        reason: 'the stale bullet disappears in the command frame',
      );
      await harness.waitForCurrentWithoutBullet(tester);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'compact list demand crosses five items without exhausting retries',
    (tester) async {
      const source =
          '- zero\n'
          '- one\n'
          '- two\n'
          '- three\n'
          '- four\n';
      const words = <String>['zero', 'one', 'two', 'three', 'four'];
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: source,
        caretUtf16: source.indexOf(words.first) + 1,
      );
      final first = await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'zero\n',
        expectedOrdinal: 0,
      );
      expect(first.bulletListProjection!.coversWholeList, isFalse);
      expect(first.bulletListProjection!.records, hasLength(1));

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      for (var ordinal = 1; ordinal < words.length; ordinal += 1) {
        final word = words[ordinal];
        harness.binding.controller.handoffInputIsland(
          FlarkV3GlobalEditingState(
            selection: TextSelection.collapsed(
              offset: source.indexOf(word) + 1,
            ),
            composing: TextRange.empty,
          ),
        );
        final query = await harness.waitForBulletItem(
          tester,
          expectedDisplay: '$word\n',
          expectedOrdinal: ordinal,
        );
        expect(query.bulletListProjection!.coversWholeList, isFalse);
        expect(query.bulletListProjection!.records, hasLength(1));
        _expectSameInputClient(
          tester,
          harness,
          editableState: editableState,
          setClientCount: initialSetClientCount,
          clientId: clientId,
        );
      }
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'terminal empty Enter exits the list with its canonical CRLF',
    (tester) async {
      const initialSource = 'before\n\n- α\r\n- ';
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: initialSource,
        caretUtf16: initialSource.length,
      );
      final query = await harness.waitForBulletItem(
        tester,
        expectedDisplay: '',
        expectedOrdinal: 1,
      );
      expect(query.structure.bulletList!.hasTerminalEmptyItem, isTrue);
      expect(query.bulletListProjection!.editingInputs.emptyEnterExits, isTrue);

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
          oldText: '',
          textInserted: '\n',
          insertionOffset: 0,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);

      expect(harness.runtime.sourceRevision, initialRevision + 1);
      expect(harness.runtime.exportMarkdown(), 'before\n\n- α\r\n\r\n');
      await harness.waitForCurrentWithoutBullet(tester);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'active composition withholds both the selected projection and its gutter',
    (tester) async {
      const source = '- にほん\r\n- β\r\n';
      final harness = await _ManagedBulletListHarness.mount(
        tester,
        source: source,
        caretUtf16: 3,
        composing: const TextRange(start: 2, end: 3),
      );

      await harness.waitForBulletPayloadWhileCompositionIsActive(tester);
      expect(
        harness.binding.controller.paintState.blockStyleLease?.kind,
        isNot(FlarkV3FlutterBlockStyleKind.tightBulletListItem),
      );
      expect(
        find.byKey(const Key('flark-v3-bullet-list-item-gutter')),
        findsNothing,
      );
      expect(harness.binding.controller.editingController.text, contains('- '));

      harness.binding.controller.updateLocalEditingValue(
        harness.binding.controller.editingController.value.copyWith(
          composing: TextRange.empty,
        ),
      );
      await harness.waitForBulletItem(
        tester,
        expectedDisplay: 'にほん\n',
        expectedOrdinal: 0,
      );
      _expectBulletPresentation(tester, harness);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );
}

final class _ManagedBulletListHarness {
  _ManagedBulletListHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_ManagedBulletListHarness> mount(
    WidgetTester tester, {
    required String source,
    required int caretUtf16,
    TextRange composing = TextRange.empty,
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
          composing: composing,
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
    final harness = _ManagedBulletListHarness._(
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

  Future<FlarkV3DocumentStructuralQuery> waitForBulletItem(
    WidgetTester tester, {
    required String expectedDisplay,
    required int expectedOrdinal,
    bool requireInlineFacts = false,
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
          query.structure.kind == FlarkV3DocumentStructureKind.bulletList &&
          query.bulletListProjection?.selectedItemOrdinal == expectedOrdinal &&
          (!requireInlineFacts || query.inlineFacts != null) &&
          binding.controller.editingController.text == expectedDisplay &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.blockStyleLease?.kind ==
              FlarkV3FlutterBlockStyleKind.tightBulletListItem) {
        return query;
      }
    }
    throw TestFailure(_diagnostic('managed bullet item'));
  }

  Future<FlarkV3DocumentStructuralQuery> waitForOrderedItem(
    WidgetTester tester, {
    required String expectedDisplay,
    required int expectedOrdinal,
    required String expectedMarker,
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
          query.structure.kind == FlarkV3DocumentStructureKind.orderedList &&
          query.orderedListProjection?.selectedItemOrdinal == expectedOrdinal &&
          query.orderedListProjection?.selectedMarkerText == expectedMarker &&
          binding.controller.editingController.text == expectedDisplay &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.blockStyleLease?.kind ==
              FlarkV3FlutterBlockStyleKind.tightListItem) {
        return query;
      }
    }
    throw TestFailure(_diagnostic('managed ordered item'));
  }

  Future<void> waitForCurrentWithoutBullet(WidgetTester tester) async {
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
          query?.sourceRevision == status.sourceRevision &&
          (query is! FlarkV3DocumentStructuralQuery ||
              query.structure.kind !=
                  FlarkV3DocumentStructureKind.bulletList) &&
          binding.controller.paintState.blockStyleLease?.kind !=
              FlarkV3FlutterBlockStyleKind.tightBulletListItem) {
        return;
      }
    }
    throw TestFailure(_diagnostic('current non-list presentation'));
  }

  Future<void> waitForBulletPayloadWhileCompositionIsActive(
    WidgetTester tester,
  ) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 10)) {
      await tester.pump(const Duration(milliseconds: 1));
      await runManagedRuntimeAsyncForTest(
        tester,
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final query = binding.controller.paintState.documentQuery;
      if (runtime.status.sourceCurrent &&
          runtime.status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.structure.kind == FlarkV3DocumentStructureKind.bulletList &&
          query.bulletListProjection != null &&
          binding.controller.globalEditingState.composing.isValid &&
          !binding.controller.globalEditingState.composing.isCollapsed) {
        return;
      }
    }
    throw TestFailure(_diagnostic('withheld composing list payload'));
  }

  String _diagnostic(String target) {
    final status = runtime.status;
    final query = binding.controller.paintState.documentQuery;
    return 'Timed out waiting for $target: '
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
        'payload=${query is FlarkV3DocumentStructuralQuery ? query.bulletListProjection != null || query.orderedListProjection != null : false}, '
        'inline=${query is FlarkV3DocumentStructuralQuery ? query.inlineFacts?.disposition.name : '-'}, '
        'ordinal=${query is FlarkV3DocumentStructuralQuery ? query.bulletListProjection?.selectedItemOrdinal ?? query.orderedListProjection?.selectedItemOrdinal : '-'}, '
        'island=${binding.controller.inputIslandGlobalStartUtf16}..'
        '${binding.controller.inputIslandGlobalEndUtf16}, '
        'text=${binding.controller.editingController.text}, '
        'source=${runtime.exportMarkdown()}.';
  }
}

void _expectBulletPresentation(
  WidgetTester tester,
  _ManagedBulletListHarness harness,
) {
  expect(
    harness.binding.controller.paintState.blockStyleLease?.kind,
    FlarkV3FlutterBlockStyleKind.tightBulletListItem,
  );
  expect(
    find.byKey(const Key('flark-v3-bullet-list-item-gutter')),
    findsOneWidget,
  );
  expect(
    find.byKey(const Key('flark-v3-bullet-list-item-marker')),
    findsOneWidget,
  );
  expect(
    find.byWidgetPredicate((widget) => widget is EditableText),
    findsOneWidget,
    reason: 'list presentation must retain one EditableText',
  );
}

void _expectOrderedPresentation(
  WidgetTester tester,
  _ManagedBulletListHarness harness, {
  required String marker,
}) {
  expect(
    harness.binding.controller.paintState.blockStyleLease?.kind,
    FlarkV3FlutterBlockStyleKind.tightListItem,
  );
  expect(
    find.byKey(const Key('flark-v3-ordered-list-item-gutter')),
    findsOneWidget,
  );
  expect(
    find.byKey(const Key('flark-v3-ordered-list-item-marker')),
    findsOneWidget,
  );
  expect(find.text(marker, findRichText: true), findsOneWidget);
  expect(
    find.byKey(const Key('flark-v3-bullet-list-item-marker')),
    findsNothing,
  );
  expect(
    find.byWidgetPredicate((widget) => widget is EditableText),
    findsOneWidget,
    reason: 'ordered-list presentation must retain one EditableText',
  );
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedBulletListHarness harness, {
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

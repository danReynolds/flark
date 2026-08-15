import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../v2/support/flark_test_paths.dart';

void main() {
  testWidgets(
    'closed fence projects only its body and edits it on the managed input client',
    (tester) async {
      const prefix = '```dart\n';
      const body = "print('x');\n";
      const closer = '```\n';
      const source = '$prefix$body$closer';
      final caret = prefix.length + body.indexOf('x') + 1;
      final harness = await _ManagedFenceHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: caret,
        maximumIslandUtf16: 64,
      );

      final initialFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + body.length,
        expectedIslandText: body,
      );
      initialFence.expectExposedFacts(
        legacyClosed: true,
        rawInfoStartUtf16: 3,
        rawInfoEndUtf16: 7,
      );
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        prefix.length,
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16,
        prefix.length + body.length,
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('```')),
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('dart')),
      );
      expect(
        tester
            .widget<EditableText>(find.byKey(harness.editableKey))
            .style
            .fontFamily,
        'monospace',
        reason:
            'the exact fenced-body query, not a Dart grammar guess, selects '
            'the block-level code style',
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCalls = _setClientCalls(tester);
      final clientId =
          (initialSetClientCalls.last.arguments as List<dynamic>).first as int;
      final editableState = harness.editableState;
      final insertionOffset = body.indexOf('x') + 1;
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaInsertion(
          oldText: body,
          textInserted: '!',
          insertionOffset: insertionOffset,
          selection: TextSelection.collapsed(offset: insertionOffset + 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();

      const editedBody = "print('x!');\n";
      expect(harness.runtime.exportMarkdown(), '$prefix$editedBody$closer');
      final editedFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + editedBody.length,
        expectedIslandText: editedBody,
      );
      editedFence.expectExposedFacts(
        legacyClosed: true,
        rawInfoStartUtf16: 3,
        rawInfoEndUtf16: 7,
      );
      expect(harness.editableKey.currentState, same(editableState));
      expect(_setClientCalls(tester), hasLength(initialSetClientCalls.length));
      expect(
        (_setClientCalls(tester).last.arguments as List<dynamic>).first,
        clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'unclosed fence is authoritative and Markdown-looking body remains literal',
    (tester) async {
      const prefix = '```markdown\n';
      const body = '**literal** _still literal_ `code`\n';
      const source = '$prefix$body';
      final harness = await _ManagedFenceHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: prefix.length + 4,
        maximumIslandUtf16: 64,
      );

      final fence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: source.length,
        expectedIslandText: body,
      );
      fence.expectExposedFacts(legacyClosed: false);
      expect(fence.editableSource.endUtf16, source.length);
      expect(
        harness.binding.controller.hasProjectedInlinePresentation,
        isFalse,
      );
      expect(
        harness.binding.controller.editingController.text,
        body,
        reason:
            'inline-looking source inside a fenced body must not enter the '
            'paragraph inline projection lane',
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'closer removal and restoration preserve EditableText and platform client',
    (tester) async {
      const prefix = '```text\n';
      const body = 'body\n';
      const closer = '```\n';
      const source = '$prefix$body$closer';
      final harness = await _ManagedFenceHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: source.length,
        caretUtf16: prefix.length + 2,
        maximumIslandUtf16: 64,
      );
      final initialFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + body.length,
        expectedIslandText: body,
      );
      initialFence.expectExposedFacts(legacyClosed: true);

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      final closingStart = prefix.length + body.length;
      harness.runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: harness.runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: closingStart,
            endUtf16: source.length,
            replacement: '',
          ),
        ),
      );
      expect(harness.runtime.exportMarkdown(), '$prefix$body');
      final unclosedFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + body.length,
        expectedIslandText: body,
      );
      unclosedFence.expectExposedFacts(legacyClosed: false);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      harness.runtime.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: harness.runtime.sourceRevision,
          operation: FlarkV3SourceEdit(
            startUtf16: harness.runtime.exportMarkdown().length,
            endUtf16: harness.runtime.exportMarkdown().length,
            replacement: closer,
          ),
        ),
      );
      expect(harness.runtime.exportMarkdown(), source);
      final restoredFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + body.length,
        expectedIslandText: body,
      );
      restoredFence.expectExposedFacts(legacyClosed: true);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'oversized fenced body keeps a bounded caret-local managed island',
    (tester) async {
      const prefix = '```text\n';
      const closer = '```\n';
      final body = List<String>.generate(
        2500,
        (index) => 'line-${index.toString().padLeft(4, '0')}\n',
        growable: false,
      ).join();
      final source = '$prefix$body$closer';
      final middleBodyOffset = body.indexOf('line-1250') + 'line-'.length;
      final caret = prefix.length + middleBodyOffset;
      const maximumIslandUtf16 = 8192;
      final islandStart = caret - maximumIslandUtf16 ~/ 2;
      final islandEnd = islandStart + maximumIslandUtf16;
      final harness = await _ManagedFenceHarness.mount(
        tester,
        source: source,
        islandStartUtf16: islandStart,
        islandEndUtf16: islandEnd,
        caretUtf16: caret,
        maximumIslandUtf16: maximumIslandUtf16,
      );

      final initialFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + body.length,
      );
      expect(initialFence.query, isA<FlarkV3RecursiveGreenPointQuery>());
      initialFence.expectExposedFacts(legacyClosed: true);
      expect(initialFence.editableSource.endUtf16, greaterThan(8192));
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16 -
            harness.binding.controller.inputIslandGlobalStartUtf16,
        lessThanOrEqualTo(maximumIslandUtf16),
      );
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        lessThanOrEqualTo(caret),
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16,
        greaterThanOrEqualTo(caret),
      );
      expect(
        harness.binding.controller.editingController.text.length,
        lessThanOrEqualTo(maximumIslandUtf16),
      );
      expect(harness.binding.controller.editingController.text, isNot(body));
      expect(
        harness.binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
        reason:
            'a bounded body shard must retain the fence structural authority '
            'rather than fall back after rejecting the whole body',
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;
      final localEditOffset =
          caret - harness.binding.controller.inputIslandGlobalStartUtf16;
      final oldIslandText = harness.binding.controller.editingController.text;
      expect(
        oldIslandText.substring(localEditOffset, localEditOffset + 1),
        '1',
      );
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaReplacement(
          oldText: oldIslandText,
          replacementText: 'X',
          replacedRange: TextRange(
            start: localEditOffset,
            end: localEditOffset + 1,
          ),
          selection: TextSelection.collapsed(offset: localEditOffset + 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();

      final editedBody = body.replaceRange(
        middleBodyOffset,
        middleBodyOffset + 1,
        'X',
      );
      expect(harness.runtime.exportMarkdown(), '$prefix$editedBody$closer');
      final editedFence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + editedBody.length,
      );
      expect(editedFence.query, isA<FlarkV3RecursiveGreenPointQuery>());
      editedFence.expectExposedFacts(legacyClosed: true);
      expect(editedFence.query.sourceRevision, harness.runtime.sourceRevision);
      expect(
        harness.binding.controller.paintState.mode,
        FlarkV3FlutterPaintMode.exactStructural,
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16 -
            harness.binding.controller.inputIslandGlobalStartUtf16,
        lessThanOrEqualTo(maximumIslandUtf16),
      );
      expect(
        harness.binding.controller.editingController.text,
        contains('line-X250'),
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('```')),
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );

  testWidgets(
    'oversized body handoff trims syntax from an initially crossing island',
    (tester) async {
      const prefix = '```text\n';
      const closer = '```\n';
      final body = List<String>.generate(
        1200,
        (index) => 'literal-$index **code**\n',
        growable: false,
      ).join();
      final source = '$prefix$body$closer';
      const maximumIslandUtf16 = 8192;
      final caret = prefix.length + 100;
      final harness = await _ManagedFenceHarness.mount(
        tester,
        source: source,
        islandStartUtf16: 0,
        islandEndUtf16: maximumIslandUtf16,
        caretUtf16: caret,
        maximumIslandUtf16: maximumIslandUtf16,
      );

      final fence = await harness.waitForExactFence(
        tester,
        expectedBodyStartUtf16: prefix.length,
        expectedBodyEndUtf16: prefix.length + body.length,
        expectedIslandText: source.substring(
          prefix.length,
          prefix.length + maximumIslandUtf16,
        ),
      );
      fence.expectExposedFacts(legacyClosed: true);
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        prefix.length,
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16,
        prefix.length + maximumIslandUtf16,
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(contains('```')),
      );
      expect(
        harness.binding.controller.editingController.text,
        contains('**code**'),
        reason: 'inline-looking code bytes remain literal inside the body',
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

final class _ManagedFenceHarness {
  _ManagedFenceHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_ManagedFenceHarness> mount(
    WidgetTester tester, {
    required String source,
    required int islandStartUtf16,
    required int islandEndUtf16,
    required int caretUtf16,
    required int maximumIslandUtf16,
  }) async {
    final runtime = (await tester.runAsync(() {
      return FlarkV3DocumentRuntime.open(
        source,
        nativeLibraryPath: flarkNativeBridgeLibraryPathForPlatform(),
      );
    }))!;
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
    final harness = _ManagedFenceHarness._(
      runtime: runtime,
      binding: binding,
      editableKey: editableKey,
      focusNode: focusNode,
    );
    addTearDown(() async {
      binding.dispose();
      focusNode.dispose();
      if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
        await runtime.close().timeout(const Duration(seconds: 5));
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
    await tester.runAsync(
      () => runtime.initialReady.timeout(const Duration(seconds: 5)),
    );
    return harness;
  }

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3ManagedFlutterBinding binding;
  final GlobalKey<EditableTextState> editableKey;
  final FocusNode focusNode;

  EditableTextState get editableState => editableKey.currentState!;

  Future<_FenceInspection> waitForExactFence(
    WidgetTester tester, {
    required int expectedBodyStartUtf16,
    required int expectedBodyEndUtf16,
    String? expectedIslandText,
  }) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 5)) {
      await tester.pump(const Duration(milliseconds: 1));
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final query = binding.controller.paintState.documentQuery;
      final islandStart = binding.controller.inputIslandGlobalStartUtf16;
      final islandEnd = binding.controller.inputIslandGlobalEndUtf16;
      if (runtime.status.structureCurrent &&
          binding.controller.paintState.mode ==
              FlarkV3FlutterPaintMode.exactStructural &&
          binding.controller.paintState.blockStyleLease?.kind ==
              FlarkV3FlutterBlockStyleKind.fencedCode &&
          query != null &&
          islandStart >= expectedBodyStartUtf16 &&
          islandEnd <= expectedBodyEndUtf16 &&
          runtime.readSourceRange(islandStart, islandEnd) ==
              binding.controller.editingController.text &&
          (expectedIslandText == null ||
              binding.controller.editingController.text ==
                  expectedIslandText)) {
        final inspection = _inspectFence(runtime, query);
        if (inspection != null &&
            _spanHasUtf16(
              inspection.physicalSource,
              0,
              runtime.exportMarkdown().length,
            ) &&
            _spanHasUtf16(
              inspection.editableSource,
              expectedBodyStartUtf16,
              expectedBodyEndUtf16,
            )) {
          return inspection;
        }
      }
    }
    final query = binding.controller.paintState.documentQuery;
    throw TestFailure(
      'Timed out waiting for managed fenced-code projection: '
      'revision=${runtime.sourceRevision}, '
      'sourceCurrent=${runtime.status.sourceCurrent}, '
      'structureCurrent=${runtime.status.structureCurrent}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${query.runtimeType}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'islandLength=${binding.controller.editingController.text.length}, '
      'expectedLength=${expectedIslandText?.length}.',
    );
  }
}

final class _FenceInspection {
  const _FenceInspection({
    required this.query,
    required this.physicalSource,
    required this.editableSource,
    required this.legacyFacts,
    required this.recursiveFacts,
  });

  final FlarkV3DocumentQueryResult query;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3SourceSpan editableSource;
  final FlarkV3FencedCodeFacts? legacyFacts;
  final FlarkV3RecursiveGreenCodePathFact? recursiveFacts;

  void expectExposedFacts({
    required bool legacyClosed,
    int? rawInfoStartUtf16,
    int? rawInfoEndUtf16,
  }) {
    final legacy = legacyFacts;
    if (legacy != null) {
      expect(legacy.closed, legacyClosed);
      if (rawInfoStartUtf16 != null) {
        expect(legacy.rawInfoSource.startUtf16, rawInfoStartUtf16);
      }
      if (rawInfoEndUtf16 != null) {
        expect(legacy.rawInfoSource.endUtf16, rawInfoEndUtf16);
      }
    }
    final recursive = recursiveFacts;
    if (recursive != null) {
      expect(recursive.marker, FlarkV3CodeFenceMarker.backtick);
      expect(recursive.fenceOffsetColumns, 0);
      expect(recursive.minimumClosingLength, BigInt.from(3));
    }
  }
}

_FenceInspection? _inspectFence(
  FlarkV3DocumentRuntime runtime,
  FlarkV3DocumentQueryResult query,
) {
  if (query.sourceRevision != runtime.sourceRevision) return null;
  switch (query) {
    case FlarkV3DocumentStructuralQuery(
      :final structureRevision,
      :final structure,
      :final projection,
    ):
      final facts = structure.fencedCode;
      if (structureRevision != runtime.sourceRevision ||
          structure.kind != FlarkV3DocumentStructureKind.fencedCode ||
          facts == null ||
          !_sameSourceSpan(projection.projectedSource, facts.bodySource)) {
        return null;
      }
      return _FenceInspection(
        query: query,
        physicalSource: structure.source,
        editableSource: facts.bodySource,
        legacyFacts: facts,
        recursiveFacts: null,
      );
    case final FlarkV3RecursiveGreenPointQuery recursiveQuery:
      if (recursiveQuery.structureRevision != runtime.sourceRevision ||
          recursiveQuery.owner.kind != FlarkV3RecursiveGreenKind.fencedCode ||
          !recursiveQuery.isIdentityEditableContent) {
        return null;
      }
      final range = runtime.queryBlockRange(
        recursiveQuery.source.startUtf16,
        recursiveQuery.source.endUtf16,
      );
      expect(range, isA<FlarkV3RecursiveGreenRowRange>());
      final greenRange = range as FlarkV3RecursiveGreenRowRange;
      expect(
        (greenRange.sourceRevision, greenRange.structureRevision),
        (recursiveQuery.sourceRevision, recursiveQuery.structureRevision),
      );
      expect(greenRange.selectedRow, isNotNull);
      final row = greenRange.selectedRow!;
      expect(row.kind, FlarkV3RecursiveGreenKind.fencedCode);
      expect(
        row.presentationKind,
        FlarkV3RecursiveGreenRowPresentationKind.fencedCode,
      );
      expect(row.literal, isTrue);
      expect(
        row.editCapability,
        FlarkV3RecursiveGreenRowEditCapability.contiguous,
      );
      expect(row.frameId, recursiveQuery.owner.frameId);
      expect(row.editableSource, isNotNull);
      final editableSource = row.editableSource!;
      final ownerFrame = row.path.singleWhere((frame) => frame.isRowOwner);
      final recursiveFacts = ownerFrame.fact;
      expect(ownerFrame.frameId, row.frameId);
      expect(recursiveFacts, isA<FlarkV3RecursiveGreenCodePathFact>());
      return _FenceInspection(
        query: recursiveQuery,
        physicalSource: row.physicalSource,
        editableSource: editableSource,
        legacyFacts: null,
        recursiveFacts: recursiveFacts! as FlarkV3RecursiveGreenCodePathFact,
      );
    case _:
      return null;
  }
}

bool _spanHasUtf16(FlarkV3SourceSpan span, int startUtf16, int endUtf16) =>
    span.startUtf16 == startUtf16 && span.endUtf16 == endUtf16;

bool _sameSourceSpan(FlarkV3SourceSpan left, FlarkV3SourceSpan right) =>
    left.startUtf8 == right.startUtf8 &&
    left.endUtf8 == right.endUtf8 &&
    left.startUtf16 == right.startUtf16 &&
    left.endUtf16 == right.endUtf16;

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _ManagedFenceHarness harness, {
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

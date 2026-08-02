import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

const int _paragraphCount = 4096;
const int _middleParagraphIndex = _paragraphCount ~/ 2;
const int _maximumIslandUtf16 = 128;

const String _initialTargetMarkdown = '**β😀** and _em_.';
const String _composedTargetMarkdown = '**βに😀** and _em_.';
const String _pastedTargetMarkdown = '**βに😀** and _paste🌍_.';

const String _initialDisplay = 'β😀 and em.\n';
const String _composedDisplay = 'βに😀 and em.\n';
const String _pastedDisplay = 'βに😀 and paste🌍.\n';

const int _mixedDocumentMinimumUtf16 = 512 * 1024;
const String _lateTargetMarkdown =
    'Late active **β😀bold** stays _fluid_ with `code`.';
const String _lateTargetDisplay =
    'Late active β😀bold stays fluid with code.\n';
const String _rapidInsertion = 'swift';

void main() {
  testWidgets(
    '4,096 Paragraphs retain one marker-free bounded middle editor',
    (tester) async {
      final paragraphs = List<String>.generate(
        _paragraphCount,
        (index) => index == _middleParagraphIndex
            ? _initialTargetMarkdown
            : 'Paragraph ${index.toString().padLeft(4, '0')} is canonical.',
        growable: false,
      );
      final initialSource = paragraphs.join('\n\n');
      final targetStart = initialSource.indexOf(_initialTargetMarkdown);
      final targetEnd = targetStart + _initialTargetMarkdown.length;
      final initialCaret =
          targetStart + _initialTargetMarkdown.indexOf('β') + 1;
      final composedSource = initialSource.replaceRange(
        targetStart,
        targetEnd,
        _composedTargetMarkdown,
      );
      final pastedSource = initialSource.replaceRange(
        targetStart,
        targetEnd,
        _pastedTargetMarkdown,
      );

      expect(paragraphs, hasLength(_paragraphCount));
      expect(targetStart, greaterThan(0));
      expect(targetEnd, lessThan(initialSource.length));

      final harness = await _LargeDocumentHarness.mount(
        tester,
        source: initialSource,
        islandStartUtf16: targetStart,
        islandEndUtf16: targetEnd + 1,
        caretUtf16: initialCaret,
      );
      final initialRevision = harness.runtime.sourceRevision;

      final initialQuery = await harness.waitForExactParagraph(
        tester,
        expectedTargetMarkdown: _initialTargetMarkdown,
        expectedDisplay: _initialDisplay,
        expectedRevision: initialRevision,
      );
      _expectStrongAndEmphasis(initialQuery);
      expect(harness.runtime.exportMarkdown(), initialSource);
      expect(
        harness.binding.controller.inputIslandGlobalStartUtf16,
        targetStart,
      );
      expect(
        harness.binding.controller.inputIslandGlobalEndUtf16 -
            harness.binding.controller.inputIslandGlobalStartUtf16,
        lessThanOrEqualTo(_maximumIslandUtf16),
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(anyOf(contains('**'), contains('_'))),
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      final strongInsertionOffset = _initialDisplay.indexOf('😀');
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaInsertion(
          oldText: _initialDisplay,
          textInserted: 'に',
          insertionOffset: strongInsertionOffset,
          selection: TextSelection.collapsed(offset: strongInsertionOffset + 1),
          composing: TextRange(
            start: strongInsertionOffset,
            end: strongInsertionOffset + 1,
          ),
        ),
      ]);
      await tester.pump();
      final composedSourceOffset = composedSource.indexOf('に', targetStart);
      expect(harness.runtime.exportMarkdown(), composedSource);
      expect(
        harness.binding.controller.editingController.value,
        TextEditingValue(
          text: _composedDisplay,
          selection: TextSelection.collapsed(offset: strongInsertionOffset + 1),
          composing: TextRange(
            start: strongInsertionOffset,
            end: strongInsertionOffset + 1,
          ),
        ),
      );
      expect(
        harness.binding.controller.globalEditingState.composing,
        TextRange(start: composedSourceOffset, end: composedSourceOffset + 1),
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaNonTextUpdate(
          oldText: _composedDisplay,
          selection: TextSelection.collapsed(offset: strongInsertionOffset + 1),
          composing: TextRange.empty,
        ),
      ]);
      final composedRevision = harness.runtime.sourceRevision;
      final composedQuery = await harness.waitForExactParagraph(
        tester,
        expectedTargetMarkdown: _composedTargetMarkdown,
        expectedDisplay: _composedDisplay,
        expectedRevision: composedRevision,
      );
      expect(composedRevision, initialRevision + 1);
      _expectStrongAndEmphasis(composedQuery);
      expect(harness.runtime.exportMarkdown(), composedSource);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      final emphasisStart = _composedDisplay.indexOf('em');
      const pastedText = 'paste🌍';
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaReplacement(
          oldText: _composedDisplay,
          replacementText: pastedText,
          replacedRange: TextRange(
            start: emphasisStart,
            end: emphasisStart + 2,
          ),
          selection: TextSelection.collapsed(
            offset: emphasisStart + pastedText.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      final pastedRevision = harness.runtime.sourceRevision;
      final pastedQuery = await harness.waitForExactParagraph(
        tester,
        expectedTargetMarkdown: _pastedTargetMarkdown,
        expectedDisplay: _pastedDisplay,
        expectedRevision: pastedRevision,
      );
      expect(pastedRevision, composedRevision + 1);
      _expectStrongAndEmphasis(pastedQuery);
      expect(harness.runtime.exportMarkdown(), pastedSource);
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaNonTextUpdate(
          oldText: _pastedDisplay,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      final undo = harness.runtime.undo();
      expect(undo, isNotNull);
      expect(undo!.changed, isTrue);
      final undoQuery = await harness.waitForExactParagraph(
        tester,
        expectedTargetMarkdown: _composedTargetMarkdown,
        expectedDisplay: _composedDisplay,
        expectedRevision: undo.sourceRevision,
      );
      _expectStrongAndEmphasis(undoQuery);
      expect(
        harness.runtime.exportMarkdown(),
        composedSource,
        reason:
            'undo restores only the edited middle Paragraph while all 4,095 '
            'other Paragraphs remain byte-for-byte canonical',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      await harness.close(tester);
      expect(harness.runtime.status.state, FlarkV3DocumentRuntimeState.closed);
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'large mixed late Paragraph stays live through a zero-cadence burst',
    (tester) async {
      final fixture = _mixedProductLivenessFixture();
      expect(fixture.source.length, greaterThan(_mixedDocumentMinimumUtf16));
      expect(fixture.targetStartUtf16, greaterThan(_mixedDocumentMinimumUtf16));

      final harness = await _LargeDocumentHarness.mount(
        tester,
        source: fixture.source,
        islandStartUtf16: fixture.targetStartUtf16,
        islandEndUtf16: fixture.targetEndUtf16 + 1,
        caretUtf16:
            fixture.targetStartUtf16 + _lateTargetMarkdown.indexOf('bold') + 2,
      );
      final initialRevision = harness.runtime.sourceRevision;
      final initialStructureGeneration =
          harness.runtime.status.structureGeneration;
      final initialQuery = await harness.waitForExactParagraph(
        tester,
        expectedTargetMarkdown: _lateTargetMarkdown,
        expectedDisplay: _lateTargetDisplay,
        expectedRevision: initialRevision,
        timeout: const Duration(seconds: 10),
        failFastOnTerminalInlineAttempt: true,
      );
      expect(
        initialQuery.inlineFacts!.facts.map((fact) => fact.kind),
        containsAll([
          FlarkV3InlineFactKind.strong,
          FlarkV3InlineFactKind.emphasis,
          FlarkV3InlineFactKind.code,
        ]),
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final deltaClient = editableState as DeltaTextInputClient;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      final observedDisplays = <String>[
        harness.binding.controller.editingController.text,
      ];
      void recordDisplay() => observedDisplays.add(
        harness.binding.controller.editingController.text,
      );

      harness.binding.controller.editingController.addListener(recordDisplay);
      addTearDown(
        () => harness.binding.controller.editingController.removeListener(
          recordDisplay,
        ),
      );

      final currentStructureRevisions = <int>[];
      final statusSubscription = harness.runtime.statuses.listen((status) {
        if (status.sourceRevision > initialRevision &&
            status.structureCurrent) {
          currentStructureRevisions.add(status.structureRevision!);
        }
      });
      addTearDown(statusSubscription.cancel);

      var currentDisplay = _lateTargetDisplay;
      var insertionOffset = currentDisplay.indexOf('bold') + 'bold'.length;
      var maximumCallback = Duration.zero;
      var totalCallbackMicroseconds = 0;
      final callbackMicroseconds = <int>[];
      for (var index = 0; index < _rapidInsertion.length; index += 1) {
        final character = _rapidInsertion[index];
        final callbackClock = Stopwatch()..start();
        deltaClient.updateEditingValueWithDeltas([
          TextEditingDeltaInsertion(
            oldText: currentDisplay,
            textInserted: character,
            insertionOffset: insertionOffset,
            selection: TextSelection.collapsed(offset: insertionOffset + 1),
            composing: TextRange.empty,
          ),
        ]);
        callbackClock.stop();
        callbackMicroseconds.add(callbackClock.elapsedMicroseconds);
        currentDisplay = currentDisplay.replaceRange(
          insertionOffset,
          insertionOffset,
          character,
        );
        insertionOffset += 1;
        totalCallbackMicroseconds += callbackClock.elapsedMicroseconds;
        if (callbackClock.elapsed > maximumCallback) {
          maximumCallback = callbackClock.elapsed;
        }
        expect(harness.runtime.sourceRevision, initialRevision + index + 1);
      }

      final finalRevision = initialRevision + _rapidInsertion.length;
      final finalTargetMarkdown = _lateTargetMarkdown.replaceFirst(
        '**β😀bold**',
        '**β😀bold$_rapidInsertion**',
      );
      expect(
        currentDisplay,
        _lateTargetDisplay.replaceFirst('bold', 'boldswift'),
      );
      expect(
        harness.runtime.readSourceRange(
          fixture.targetStartUtf16,
          fixture.targetStartUtf16 + finalTargetMarkdown.length + 1,
        ),
        '$finalTargetMarkdown\n',
      );
      expect(
        harness.runtime.readSourceRange(0, fixture.prefixSentinel.length),
        fixture.prefixSentinel,
        reason: 'the distant prefix must remain byte-for-byte exact',
      );
      final finalQuery = await harness.waitForExactParagraph(
        tester,
        expectedTargetMarkdown: finalTargetMarkdown,
        expectedDisplay: currentDisplay,
        expectedRevision: finalRevision,
        timeout: const Duration(seconds: 10),
        failFastOnTerminalInlineAttempt: true,
      );
      expect(
        harness.runtime.status.structureGeneration,
        initialStructureGeneration + 1,
      );
      expect(currentStructureRevisions, isNotEmpty);
      expect(currentStructureRevisions, everyElement(finalRevision));
      expect(
        finalQuery.inlineFacts!.facts.map((fact) => fact.kind),
        containsAll([
          FlarkV3InlineFactKind.strong,
          FlarkV3InlineFactKind.emphasis,
          FlarkV3InlineFactKind.code,
        ]),
      );
      expect(
        observedDisplays,
        everyElement(
          isNot(anyOf(contains('**'), contains('_fluid_'), contains('`code`'))),
        ),
        reason:
            'once exact marker-free paint exists, provisional recertification '
            'must never repaint canonical Markdown markers',
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );
      await harness.close(tester);
      // Widget tests run debug/JIT code, so the cold callback is a regression
      // sentinel rather than the production frame-time SLO. The product SLO
      // is measured in the profile/device lane; this gate proves the hot path
      // becomes sub-frame without document-sized foreground work.
      expect(
        Duration(microseconds: callbackMicroseconds.first),
        lessThan(const Duration(milliseconds: 25)),
      );
      expect(
        callbackMicroseconds.skip(1),
        everyElement(lessThan(2000)),
      );
      expect(maximumCallback, lessThan(const Duration(milliseconds: 25)));
      expect(
        Duration(microseconds: totalCallbackMicroseconds),
        lessThan(const Duration(milliseconds: 50)),
      );
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );

  testWidgets(
    '4,096 Paragraphs keep one marker-free ATX and fence input client',
    (tester) async {
      final paragraphs = List<String>.generate(
        _paragraphCount,
        (index) =>
            'Paragraph ${index.toString().padLeft(4, '0')} is canonical.',
        growable: false,
      );
      const heading = '## mixed **heading**\n';
      const fence = '```dart\nlet value = 1;\n```\n';
      final initialSource =
          '${paragraphs.take(_middleParagraphIndex).join('\n\n')}\n\n'
          '$heading\n'
          '$fence\n'
          '${paragraphs.skip(_middleParagraphIndex).join('\n\n')}';
      final headingStart = initialSource.indexOf(heading);
      final headingEnd = headingStart + heading.length;
      final headingCaret = initialSource.indexOf('heading', headingStart) + 1;

      final harness = await _LargeDocumentHarness.mount(
        tester,
        source: initialSource,
        islandStartUtf16: headingStart,
        islandEndUtf16: headingEnd,
        caretUtf16: headingCaret,
      );
      final initialRevision = harness.runtime.sourceRevision;
      final initialHeading = await harness.waitForExactHeading(
        tester,
        expectedDisplay: 'mixed heading',
        expectedProjectedMarkdown: 'mixed **heading**',
        expectedRevision: initialRevision,
      );
      expect(initialHeading.structure.heading!.level, 2);
      expect(
        harness.binding.controller.editingController.text,
        isNot(anyOf(contains('##'), contains('**'))),
      );

      harness.editableState.requestKeyboard();
      await tester.pump();
      final editableState = harness.editableState;
      final initialSetClientCount = _setClientCalls(tester).length;
      final clientId =
          (_setClientCalls(tester).last.arguments as List<dynamic>).first
              as int;

      const headingDisplay = 'mixed heading';
      const editedHeadingDisplay = 'mixed! heading';
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: headingDisplay,
          textInserted: '!',
          insertionOffset: 5,
          selection: TextSelection.collapsed(offset: 6),
          composing: TextRange.empty,
        ),
      ]);
      final headingRevision = harness.runtime.sourceRevision;
      expect(headingRevision, initialRevision + 1);
      expect(
        harness.runtime.exportMarkdown(),
        initialSource.replaceFirst(
          '## mixed **heading**',
          '## mixed! **heading**',
        ),
      );
      await harness.waitForExactHeading(
        tester,
        expectedDisplay: editedHeadingDisplay,
        expectedProjectedMarkdown: 'mixed! **heading**',
        expectedRevision: headingRevision,
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      final fencePoint =
          harness.runtime.exportMarkdown().indexOf('value = 1') + 2;
      harness.binding.controller.handoffInputIsland(
        FlarkV3GlobalEditingState(
          selection: TextSelection.collapsed(offset: fencePoint),
          composing: TextRange.empty,
        ),
      );
      await harness.waitForExactFence(
        tester,
        expectedDisplay: 'let value = 1;\n',
        expectedProjectedMarkdown: 'let value = 1;\n',
        expectedRevision: headingRevision,
      );
      expect(
        harness.binding.controller.editingController.text,
        isNot(anyOf(contains('```'), contains('dart'))),
      );
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      const fenceDisplay = 'let value = 1;\n';
      final valueOffset = fenceDisplay.indexOf('1');
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaReplacement(
          oldText: fenceDisplay,
          replacementText: '2',
          replacedRange: TextRange(start: valueOffset, end: valueOffset + 1),
          selection: TextSelection.collapsed(offset: valueOffset + 1),
          composing: TextRange.empty,
        ),
      ]);
      final fenceRevision = harness.runtime.sourceRevision;
      expect(fenceRevision, headingRevision + 1);
      await harness.waitForExactFence(
        tester,
        expectedDisplay: 'let value = 2;\n',
        expectedProjectedMarkdown: 'let value = 2;\n',
        expectedRevision: fenceRevision,
      );
      expect(harness.runtime.exportMarkdown(), contains('let value = 2;\n'));
      _expectSameInputClient(
        tester,
        harness,
        editableState: editableState,
        setClientCount: initialSetClientCount,
        clientId: clientId,
      );

      final queryPoints = [
        4,
        harness.runtime.exportMarkdown().indexOf('mixed!') + 1,
        harness.runtime.exportMarkdown().indexOf('value = 2') + 1,
        harness.runtime.exportMarkdown().lastIndexOf('Paragraph 4095') + 4,
      ];
      const expectedKinds = [
        FlarkV3DocumentStructureKind.paragraph,
        FlarkV3DocumentStructureKind.heading,
        FlarkV3DocumentStructureKind.fencedCode,
        FlarkV3DocumentStructureKind.paragraph,
      ];
      for (var index = 0; index < queryPoints.length; index += 1) {
        final query = harness.runtime.queryAtUtf16(queryPoints[index]);
        expect(query, isA<FlarkV3DocumentStructuralQuery>());
        final structuralQuery = query as FlarkV3DocumentStructuralQuery;
        expect(structuralQuery.structure.kind, expectedKinds[index]);
        expect(structuralQuery.sourceRevision, fenceRevision);
        expect(structuralQuery.structureRevision, fenceRevision);
      }

      await harness.close(tester);
      expect(harness.runtime.status.state, FlarkV3DocumentRuntimeState.closed);
    },
    timeout: const Timeout(Duration(minutes: 2)),
  );
}

({
  String source,
  String prefixSentinel,
  int targetStartUtf16,
  int targetEndUtf16,
})
_mixedProductLivenessFixture() {
  const prefixSentinel = 'Opening sentinel remains canonical.\n\n';
  final source = StringBuffer(prefixSentinel);
  var cycle = 0;
  while (source.length < _mixedDocumentMinimumUtf16) {
    source
      ..writeln('Paragraph $cycle has **strong** and _emphasis_ content.')
      ..writeln()
      ..writeln('## Heading $cycle')
      ..writeln()
      ..writeln('> Quote $cycle remains source-backed.')
      ..writeln('> Its continuation stays in the same container.')
      ..writeln()
      ..writeln('- list item $cycle')
      ..writeln('- second item $cycle')
      ..writeln()
      ..writeln('```dart')
      ..writeln('final value$cycle = $cycle;')
      ..writeln('```')
      ..writeln()
      ..writeln('---')
      ..writeln();
    cycle += 1;
  }
  final targetStartUtf16 = source.length;
  source.writeln(_lateTargetMarkdown);
  return (
    source: source.toString(),
    prefixSentinel: prefixSentinel,
    targetStartUtf16: targetStartUtf16,
    targetEndUtf16: targetStartUtf16 + _lateTargetMarkdown.length,
  );
}

final class _LargeDocumentHarness {
  _LargeDocumentHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_LargeDocumentHarness> mount(
    WidgetTester tester, {
    required String source,
    required int islandStartUtf16,
    required int islandEndUtf16,
    required int caretUtf16,
  }) async {
    final runtime = await runManagedRuntimeAsyncForTest(
      tester,
      () => openManagedRuntimeForTest(source),
    );
    final binding = FlarkV3ManagedFlutterBinding.attach(
      runtime: runtime,
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: islandStartUtf16,
        maximumUtf16: _maximumIslandUtf16,
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
    final harness = _LargeDocumentHarness._(
      runtime: runtime,
      binding: binding,
      editableKey: GlobalKey<EditableTextState>(),
      focusNode: FocusNode(),
    );
    addTearDown(() => harness._disposeAfterFailure(tester));

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkV3LiveEditorPrototype(
          controller: binding.controller,
          editableKey: harness.editableKey,
          focusNode: harness.focusNode,
          paintLayerBuilder: (context, state) => const SizedBox.shrink(),
        ),
      ),
    );
    await runManagedRuntimeAsyncForTest(
      tester,
      () => runtime.initialReady.timeout(const Duration(seconds: 60)),
    );
    return harness;
  }

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3ManagedFlutterBinding binding;
  final GlobalKey<EditableTextState> editableKey;
  final FocusNode focusNode;

  bool _uiDisposed = false;

  EditableTextState get editableState => editableKey.currentState!;

  Future<FlarkV3DocumentStructuralQuery> waitForExactParagraph(
    WidgetTester tester, {
    required String expectedTargetMarkdown,
    required String expectedDisplay,
    required int expectedRevision,
    Duration timeout = const Duration(seconds: 60),
    bool failFastOnTerminalInlineAttempt = false,
  }) => _waitForExactStructure(
    tester,
    kind: FlarkV3DocumentStructureKind.paragraph,
    expectedDisplay: expectedDisplay,
    expectedProjectedMarkdown: '$expectedTargetMarkdown\n',
    expectedRevision: expectedRevision,
    requireCertifiedInline: true,
    timeout: timeout,
    failFastOnTerminalInlineAttempt: failFastOnTerminalInlineAttempt,
  );

  Future<FlarkV3DocumentStructuralQuery> waitForExactHeading(
    WidgetTester tester, {
    required String expectedDisplay,
    required String expectedProjectedMarkdown,
    required int expectedRevision,
  }) => _waitForExactStructure(
    tester,
    kind: FlarkV3DocumentStructureKind.heading,
    expectedDisplay: expectedDisplay,
    expectedProjectedMarkdown: expectedProjectedMarkdown,
    expectedRevision: expectedRevision,
    requireCertifiedInline: true,
  );

  Future<FlarkV3DocumentStructuralQuery> waitForExactFence(
    WidgetTester tester, {
    required String expectedDisplay,
    required String expectedProjectedMarkdown,
    required int expectedRevision,
  }) => _waitForExactStructure(
    tester,
    kind: FlarkV3DocumentStructureKind.fencedCode,
    expectedDisplay: expectedDisplay,
    expectedProjectedMarkdown: expectedProjectedMarkdown,
    expectedRevision: expectedRevision,
    requireCertifiedInline: false,
  );

  Future<FlarkV3DocumentStructuralQuery> _waitForExactStructure(
    WidgetTester tester, {
    required FlarkV3DocumentStructureKind kind,
    required String expectedDisplay,
    required String expectedProjectedMarkdown,
    required int expectedRevision,
    required bool requireCertifiedInline,
    Duration timeout = const Duration(seconds: 60),
    bool failFastOnTerminalInlineAttempt = false,
  }) async {
    final startingInlinePresentation =
        runtime.status.inlinePresentationGeneration;
    final startingInlineOutcome = runtime.status.inlineAttemptOutcomeGeneration;
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < timeout) {
      await tester.pump(const Duration(milliseconds: 1));
      await runManagedRuntimeAsyncForTest(
        tester,
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final status = runtime.status;
      final query = binding.controller.paintState.documentQuery;
      if (status.state == FlarkV3DocumentRuntimeState.faulted) {
        throw TestFailure(
          'The managed runtime faulted at source revision '
          '${status.sourceRevision}.',
        );
      }
      final terminalInlineAttempt =
          failFastOnTerminalInlineAttempt &&
          requireCertifiedInline &&
          status.inlinePresentationGeneration == startingInlinePresentation &&
          ((startingInlinePresentation == 0 &&
                  status.inlineAttemptOutcomeGeneration >= 2) ||
              status.inlineAttemptOutcomeGeneration >=
                  startingInlineOutcome + 2);
      if (terminalInlineAttempt) {
        throw TestFailure(
          'Inline certification reached its bounded retry limit without a '
          'presentation: revision=${status.sourceRevision}, '
          'inline=${status.inlinePresentationGeneration}/'
          '${status.inlineAttemptOutcomeGeneration}, '
          'target=$expectedProjectedMarkdown.',
        );
      }
      if (status.state == FlarkV3DocumentRuntimeState.open &&
          status.sourceRevision == expectedRevision &&
          status.certifiedSourceRevision == expectedRevision &&
          status.sourceCurrent &&
          status.structureRevision == expectedRevision &&
          status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.sourceRevision == expectedRevision &&
          query.structureRevision == expectedRevision &&
          query.structure.kind == kind &&
          (!requireCertifiedInline ||
              (query.inlineFacts != null &&
                  binding.controller.hasCertifiedInlinePresentation)) &&
          binding.controller.editingController.text == expectedDisplay &&
          runtime.readSourceRange(
                query.projection.projectedSource.startUtf16,
                query.projection.projectedSource.endUtf16,
              ) ==
              expectedProjectedMarkdown) {
        expect(
          binding.controller.inputIslandGlobalEndUtf16 -
              binding.controller.inputIslandGlobalStartUtf16,
          lessThanOrEqualTo(_maximumIslandUtf16),
        );
        return query;
      }
    }
    final status = runtime.status;
    throw TestFailure(
      'Timed out waiting for exact-current ${kind.name}: '
      'expectedRevision=$expectedRevision, '
      'sourceRevision=${status.sourceRevision}, '
      'certifiedRevision=${status.certifiedSourceRevision}, '
      'structureRevision=${status.structureRevision}, '
      'structureCurrent=${status.structureCurrent}, '
      'inline=${status.inlinePresentationGeneration}/'
      '${status.inlineAttemptOutcomeGeneration}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${binding.controller.paintState.documentQuery.runtimeType}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'display=${binding.controller.editingController.text}.',
    );
  }

  Future<void> close(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    expect(tester.testTextInput.hasAnyClients, isFalse);
    expect(tester.takeException(), isNull);
    _disposeUi();
    await runManagedRuntimeAsyncForTest(
      tester,
      () => runtime.close().timeout(const Duration(seconds: 60)),
    );
  }

  Future<void> _disposeAfterFailure(WidgetTester tester) async {
    _disposeUi();
    if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
      await runManagedRuntimeAsyncForTest(
        tester,
        () => runtime.close().timeout(const Duration(seconds: 60)),
      );
    }
  }

  void _disposeUi() {
    if (_uiDisposed) return;
    _uiDisposed = true;
    binding.dispose();
    focusNode.dispose();
  }
}

void _expectStrongAndEmphasis(FlarkV3DocumentStructuralQuery query) {
  final inline = query.inlineFacts;
  expect(inline, isNotNull);
  expect(inline!.disposition, FlarkV3InlineFactsDisposition.authoritative);
  expect(
    inline.facts.map((fact) => fact.kind),
    containsAll([FlarkV3InlineFactKind.strong, FlarkV3InlineFactKind.emphasis]),
  );
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

void _expectSameInputClient(
  WidgetTester tester,
  _LargeDocumentHarness harness, {
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

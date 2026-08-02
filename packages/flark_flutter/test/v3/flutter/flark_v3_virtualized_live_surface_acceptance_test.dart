import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

/// Acceptance scaffold for the first real multi-block v3 Flutter surface.
///
/// The first two gates exercise the real native parser, managed runtime,
/// parser-authored viewport materializer, and production virtualized surface.
/// The distant large-document reveal remains skipped only until the host
/// source-point/ordinal locator is connected.
///
/// The harness never recognizes Markdown or manufactures marker-free passive
/// text. Every inspected block comes from a revision-bound production
/// presentation snapshot.
const bool _completeDocumentSurfaceAvailable = true;
const bool _distantOrdinalLocatorAvailable = true;

const String _mixedMarkdown = '''
Alpha **bold** and _emphasis_ with `code` and ~~gone~~.

## Heading **strong**

```dart
final value = 1;
```

Tail is ~~active~~.
''';

const String _recursiveGreenCm321 =
    '- a\n'
    '  > **b** and _c_\n'
    '  ```\n'
    '  code\n'
    '  ```\n'
    '- **d**\n';

const String _recursiveGreenCm321AfterBurst =
    '- a\n'
    '  > **by** and _c_\n'
    '  ```\n'
    '  code\n'
    '  ```\n'
    '- **d**\n';

void main() {
  group('v3 virtualized live surface acceptance', () {
    testWidgets(
      'nested Green row stays marker-free across passive activation and burst',
      (tester) async {
        final nestedPoint = _recursiveGreenCm321.indexOf('b**');
        final fencePoint = _recursiveGreenCm321.indexOf('code');
        final firstSiblingPoint = _recursiveGreenCm321.indexOf('a\n');
        final lastSiblingPoint = _recursiveGreenCm321.lastIndexOf('d**');
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: _recursiveGreenCm321,
          initialCaretUtf16: lastSiblingPoint,
          viewportSize: const Size(640, 640),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();

        final nestedPassive = harness.passiveBlockContainingSourcePoint(
          nestedPoint,
        );
        final fencePassive = harness.passiveBlockContainingSourcePoint(
          fencePoint,
        );
        final firstSibling = harness.passiveBlockContainingSourcePoint(
          firstSiblingPoint,
        );
        expect(nestedPassive.kind, FlarkV3DocumentStructureKind.paragraph);
        expect(nestedPassive.displayText, 'b and c');
        expect(fencePassive.kind, FlarkV3DocumentStructureKind.fencedCode);
        expect(fencePassive.displayText, 'code\n');
        expect(firstSibling.displayText, 'a');
        expect(
          find.byKey(const ValueKey<Object>(('flark-v3-green-quote', 1))),
          findsOneWidget,
        );
        expect(
          find.byKey(const ValueKey<Object>(('flark-v3-green-list', 0))),
          findsWidgets,
        );

        final editable = _editableText();
        final editableState = tester.state<EditableTextState>(editable);
        editableState.requestKeyboard();
        await tester.pump();
        final setClientCalls = _setClientCalls(tester);
        final clientId =
            (setClientCalls.last.arguments as List<dynamic>).first as int;
        final editingController = editableState.widget.controller;
        final observedDisplays = <String>[editingController.text];
        void recordDisplay() => observedDisplays.add(editingController.text);
        editingController.addListener(recordDisplay);
        addTearDown(() => editingController.removeListener(recordDisplay));

        await harness.revealAndActivateSourcePoint(nestedPoint);
        await harness.waitForExactViewport();
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(editingController.text, 'b and c');
        expect(
          find.byKey(const ValueKey<Object>(('flark-v3-green-quote', 1))),
          findsWidgets,
        );

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

        await harness.waitForExactViewport();

        expect(harness.exportMarkdown(), _recursiveGreenCm321AfterBurst);
        expect(harness.activeDisplayText, 'by and c');
        expect(harness.activeRecursiveGreenAuthorityCurrent, isTrue);
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
        );
        expect(
          harness.passiveBlockContainingSourcePoint(fencePoint).displayText,
          'code\n',
        );
        expect(
          harness
              .passiveBlockContainingSourcePoint(lastSiblingPoint + 1)
              .displayText,
          'd',
        );
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(_setClientCalls(tester), hasLength(setClientCalls.length));
        expect(
          (_setClientCalls(tester).last.arguments as List<dynamic>).first,
          clientId,
        );
      },
      timeout: const Timeout(Duration(minutes: 2)),
    );

    testWidgets(
      'inactive mixed blocks are parser-authored, styled, and marker-free',
      (tester) async {
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: _mixedMarkdown,
          initialCaretUtf16: _mixedMarkdown.indexOf('active') + 2,
          viewportSize: const Size(640, 640),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();

        expect(_editableText(), findsOneWidget);
        expect(harness.createdTextEditingControllerCount, 1);
        expect(harness.activeKind, FlarkV3DocumentStructureKind.paragraph);
        expect(harness.activeDisplayText, contains('Tail is active.'));
        expect(harness.activeDisplayText, isNot(contains('~')));

        final paragraph = harness.passiveBlockContainingSourcePoint(
          _mixedMarkdown.indexOf('Alpha'),
        );
        final heading = harness.passiveBlockContainingSourcePoint(
          _mixedMarkdown.indexOf('Heading'),
        );
        final fence = harness.passiveBlockContainingSourcePoint(
          _mixedMarkdown.indexOf('final value'),
        );

        expect(paragraph.kind, FlarkV3DocumentStructureKind.paragraph);
        expect(
          paragraph.displayText,
          contains('Alpha bold and emphasis with code and gone.'),
        );
        expect(
          paragraph.stylesForText('bold'),
          contains(FlarkV3InlineFactKind.strong),
        );
        expect(
          paragraph.stylesForText('emphasis'),
          contains(FlarkV3InlineFactKind.emphasis),
        );
        expect(
          paragraph.stylesForText('code'),
          contains(FlarkV3InlineFactKind.code),
        );
        expect(
          paragraph.stylesForText('gone'),
          contains(FlarkV3InlineFactKind.strikethrough),
        );

        expect(heading.kind, FlarkV3DocumentStructureKind.heading);
        expect(heading.headingLevel, 2);
        expect(heading.displayText, contains('Heading strong'));
        expect(
          heading.stylesForText('strong'),
          contains(FlarkV3InlineFactKind.strong),
        );

        expect(fence.kind, FlarkV3DocumentStructureKind.fencedCode);
        expect(fence.displayText, 'final value = 1;\n');

        final inactiveDisplay = harness.passiveBlocks
            .map((block) => block.displayText)
            .join();
        expect(inactiveDisplay, isNot(contains('**')));
        expect(inactiveDisplay, isNot(contains('_emphasis_')));
        expect(inactiveDisplay, isNot(contains('`code`')));
        expect(inactiveDisplay, isNot(contains('~~gone~~')));
        expect(inactiveDisplay, isNot(contains('```')));
        expect(inactiveDisplay, isNot(contains('dart\n')));

        final editableState = tester.state<EditableTextState>(_editableText());
        final oldValue = editableState.widget.controller.value;
        final insertionOffset = oldValue.selection.extentOffset;
        (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
          TextEditingDeltaInsertion(
            oldText: oldValue.text,
            textInserted: 'X',
            insertionOffset: insertionOffset,
            selection: TextSelection.collapsed(offset: insertionOffset + 1),
            composing: TextRange.empty,
          ),
        ]);
        await harness.waitForExactViewport();

        expect(harness.activeDisplayText, contains('acXtive'));
        expect(harness.activeDisplayText, isNot(contains('~')));
        final sourceCaret = _mixedMarkdown.indexOf('active') + 2;
        expect(
          harness.exportMarkdown(),
          _mixedMarkdown.replaceRange(sourceCaret, sourceCaret, 'X'),
          reason:
              'editing the marker-free strike content must preserve both '
              'canonical tilde pairs',
        );
      },
      skip: !_completeDocumentSurfaceAvailable,
    );

    testWidgets(
      'escaped punctuation stays marker-free across passive-to-active handoff',
      (tester) async {
        const source = 'Active.\n\n\\*';
        final escapedPoint = source.indexOf('*');
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: source.indexOf('Active') + 1,
          viewportSize: const Size(640, 480),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();

        final passive = harness.passiveBlockContainingSourcePoint(escapedPoint);
        expect(passive.kind, FlarkV3DocumentStructureKind.paragraph);
        expect(
          source.substring(
            passive.physicalSource.startUtf16,
            passive.physicalSource.endUtf16,
          ),
          r'\*',
        );
        expect(passive.displayText, '*');
        expect(passive.stylesForText('*'), isEmpty);
        expect(harness.exportMarkdown(), source);

        await harness.revealAndActivateSourcePoint(escapedPoint);
        await harness.waitForExactViewport();

        expect(harness.activeSourceContains(escapedPoint), isTrue);
        expect(harness.activeDisplayText, '*');
        expect(harness.exportMarkdown(), source);
      },
      skip: !_completeDocumentSurfaceAvailable,
    );

    testWidgets(
      'CRLF hard break stays marker-free across one passive-active input client',
      (tester) async {
        const source = 'Active.\n\nbefore  \r\nafter';
        final hardBreakPoint = source.indexOf('before');
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: source.indexOf('Active') + 1,
          viewportSize: const Size(640, 480),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();
        final passive = harness.passiveBlockContainingSourcePoint(
          hardBreakPoint,
        );
        expect(passive.kind, FlarkV3DocumentStructureKind.paragraph);
        expect(
          source.substring(
            passive.physicalSource.startUtf16,
            passive.physicalSource.endUtf16,
          ),
          'before  \r\nafter',
        );
        expect(passive.displayText, 'before\nafter');
        expect(passive.displayText, isNot(contains('  \r\n')));

        final editable = _editableText();
        final editableState = tester.state<EditableTextState>(editable);
        editableState.requestKeyboard();
        await tester.pump();
        final setClientCallsBefore = _setClientCalls(tester);
        final clientId =
            (setClientCallsBefore.last.arguments as List<dynamic>).first as int;

        await harness.revealAndActivateSourcePoint(hardBreakPoint);
        await harness.waitForExactViewport();

        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(harness.activeSourceContains(hardBreakPoint), isTrue);
        expect(harness.activeDisplayText, 'before\nafter');
        expect(_setClientCalls(tester), hasLength(setClientCallsBefore.length));
        expect(
          (_setClientCalls(tester).last.arguments as List<dynamic>).first,
          clientId,
        );
        expect(harness.exportMarkdown(), source);
      },
      skip: !_completeDocumentSurfaceAvailable,
    );

    testWidgets(
      'paragraph promotion keeps one active EditableTextState and input client',
      (tester) async {
        const source = 'Before.\n\nplain\n\nAfter.';
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: source.indexOf('plain') + 2,
          viewportSize: const Size(640, 480),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();
        final editable = _editableText();
        expect(editable, findsOneWidget);
        expect(harness.createdTextEditingControllerCount, 1);
        expect(harness.activeKind, FlarkV3DocumentStructureKind.paragraph);

        final stateBefore = tester.state<EditableTextState>(editable);
        stateBefore.requestKeyboard();
        await tester.pump();
        final setClientCallsBefore = _setClientCalls(tester);
        final clientId =
            (setClientCallsBefore.last.arguments as List<dynamic>).first as int;
        final unrelatedSourcePoint = source.indexOf('Before');
        final beforeBuildCount = harness.passiveBuildCountContainingSourcePoint(
          unrelatedSourcePoint,
        );

        final oldValue = stateBefore.widget.controller.value;
        (stateBefore as DeltaTextInputClient).updateEditingValueWithDeltas([
          TextEditingDeltaInsertion(
            oldText: oldValue.text,
            textInserted: '## ',
            insertionOffset: 0,
            selection: const TextSelection.collapsed(offset: 3),
            composing: TextRange.empty,
          ),
        ]);
        await harness.waitForActiveKind(FlarkV3DocumentStructureKind.heading);

        final stateAfter = tester.state<EditableTextState>(editable);
        expect(stateAfter, same(stateBefore));
        expect(_editableText(), findsOneWidget);
        expect(harness.createdTextEditingControllerCount, 1);
        expect(harness.activeKind, FlarkV3DocumentStructureKind.heading);
        expect(harness.activeHeadingLevel, 2);
        expect(harness.activeDisplayText, 'plain');
        expect(harness.exportMarkdown(), 'Before.\n\n## plain\n\nAfter.');
        expect(
          harness.passiveBuildCountContainingSourcePoint(unrelatedSourcePoint),
          beforeBuildCount,
          reason: 'an unrelated mounted passive block must not rebuild',
        );

        final setClientCallsAfter = _setClientCalls(tester);
        expect(setClientCallsAfter, hasLength(setClientCallsBefore.length));
        expect(
          (setClientCallsAfter.last.arguments as List<dynamic>).first,
          clientId,
        );
      },
      skip: !_completeDocumentSurfaceAvailable,
    );

    testWidgets(
      '4,096 blocks mount viewport work only and retain the active input host',
      (tester) async {
        const blockCount = 4096;
        final source = List<String>.generate(
          blockCount,
          (index) => switch (index % 4) {
            0 => 'Paragraph $index with **bold**.',
            1 => '## Heading $index',
            2 => 'Paragraph $index with _emphasis_.',
            _ => 'Paragraph $index with `code`.',
          },
          growable: false,
        ).join('\n\n');
        final middleText = 'Paragraph 2048 with **bold**.';
        final middleCaret = source.indexOf(middleText) + 3;
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: middleCaret,
          viewportSize: const Size(640, 600),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();
        expect(_editableText(), findsOneWidget);
        expect(harness.createdTextEditingControllerCount, 1);
        expect(
          harness.mountedPresentationCount,
          lessThanOrEqualTo(96),
          reason: 'mounted work is viewport plus bounded overscan, not 4,096',
        );
        final activeState = tester.state<EditableTextState>(_editableText());
        activeState.requestKeyboard();
        await tester.pump();
        final initialSetClientCalls = _setClientCalls(tester);
        final clientId =
            (initialSetClientCalls.last.arguments as List<dynamic>).first
                as int;

        final finalContent = 'Paragraph 4095 with `code`.';
        final finalCaret = source.lastIndexOf(finalContent) + 3;
        await harness.revealAndActivateSourcePoint(finalCaret);
        await harness.waitForExactViewport();

        expect(
          tester.state<EditableTextState>(_editableText()),
          same(activeState),
        );
        expect(_editableText(), findsOneWidget);
        expect(harness.createdTextEditingControllerCount, 1);
        expect(harness.mountedPresentationCount, lessThanOrEqualTo(96));
        expect(
          harness.totalCanonicalStructuralEntryCount,
          greaterThanOrEqualTo(blockCount),
          reason:
              'the host reports canonical structural entries; blank-boundary '
              'records must not be renumbered away by Flutter',
        );
        expect(
          harness.activeOrdinal,
          lessThan(harness.totalCanonicalStructuralEntryCount),
        );
        expect(harness.activeSourceContains(finalCaret), isTrue);
        expect(harness.activeDisplayText, contains('code'));
        expect(harness.activeDisplayText, isNot(contains('`')));
        expect(harness.exportMarkdown(), source);

        final finalSetClientCalls = _setClientCalls(tester);
        expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
        expect(
          (finalSetClientCalls.last.arguments as List<dynamic>).first,
          clientId,
        );
      },
      skip: !_distantOrdinalLocatorAvailable,
      timeout: const Timeout(Duration(minutes: 2)),
    );

    testWidgets(
      'large distant RecursiveGreen burst stays centered, bounded, and live',
      (tester) async {
        const cycleCount = 3200;
        const targetCycle = 2800;
        const targetMarkdown =
            'Distant target **β😀bold** stays _fluid_ with `code` while editing.';
        const rapidInsertion = 'swift';
        final sourceBuffer = StringBuffer();
        var targetStartUtf16 = -1;
        for (var cycle = 0; cycle < cycleCount; cycle += 1) {
          if (cycle == targetCycle) {
            targetStartUtf16 = sourceBuffer.length;
            sourceBuffer.writeln(targetMarkdown);
          } else {
            sourceBuffer.writeln(
              'Paragraph $cycle has **strong** and _emphasis_ content with '
              '`code` plus enough canonical padding for a realistic document.',
            );
          }
          sourceBuffer
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
        }
        final source = sourceBuffer.toString();
        final openingCaret = source.indexOf('Paragraph 0') + 3;
        final targetCaret =
            targetStartUtf16 + targetMarkdown.indexOf('bold') + 2;
        expect(source.length, greaterThan(512 * 1024));
        expect(targetStartUtf16, greaterThan(512 * 1024));

        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: openingCaret,
          viewportSize: const Size(640, 600),
        );
        addTearDown(harness.close);
        final production = harness as _ProductionVirtualizedSurfaceHarness;

        await harness.waitForExactViewport();
        final editable = _editableText();
        final editableState = tester.state<EditableTextState>(editable);
        editableState.requestKeyboard();
        await tester.pump();
        final editingController = editableState.widget.controller;
        final initialSetClientCalls = _setClientCalls(tester);
        final clientId =
            (initialSetClientCalls.last.arguments as List<dynamic>).first
                as int;
        final observedDisplays = <String>[editingController.text];
        void recordDisplay() => observedDisplays.add(editingController.text);
        editingController.addListener(recordDisplay);
        addTearDown(() => editingController.removeListener(recordDisplay));

        await harness.revealAndActivateSourcePoint(targetCaret);
        await harness.waitForExactViewport();

        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(harness.createdTextEditingControllerCount, 1);
        expect(harness.activeKind, FlarkV3DocumentStructureKind.paragraph);
        expect(harness.activeSourceContains(targetCaret), isTrue);
        expect(harness.activeDisplayText, contains('β😀bold'));
        expect(harness.activeDisplayText, contains('fluid'));
        expect(harness.activeDisplayText, contains('code'));
        expect(
          harness.activeDisplayText,
          isNot(anyOf(contains('**'), contains('_fluid_'), contains('`code`'))),
        );
        expect(harness.activeRecursiveGreenAuthorityCurrent, isTrue);
        expect(harness.totalCanonicalStructuralEntryCount, greaterThan(10000));
        expect(harness.activeOrdinal, greaterThan(10000));
        expect(harness.exactPageLength, lessThanOrEqualTo(96));
        expect(harness.mountedPresentationCount, lessThanOrEqualTo(96));

        final exactSnapshot = production._exactSnapshot;
        expect(
          exactSnapshot.blocks.every(
            (block) => block.recursiveGreenRow != null,
          ),
          isTrue,
          reason: 'the large viewport must be entirely RecursiveGreen-backed',
        );
        expect(
          exactSnapshot.blocks.first.ordinal,
          lessThan(harness.activeOrdinal),
        );
        expect(
          exactSnapshot.blocks.last.ordinal,
          greaterThan(harness.activeOrdinal),
        );

        final status = production.runtime.status;
        final ordinalWindowStart = harness.activeOrdinal > 48
            ? harness.activeOrdinal - 48
            : 0;
        final ordinalWindow = production.runtime.queryBlockOrdinalWindow(
          FlarkV3DocumentOrdinalWindowDemand(
            sourceRevision: status.sourceRevision,
            structureGeneration: status.structureGeneration,
            startBlockOrdinal: ordinalWindowStart,
          ),
          budget: const FlarkV3DocumentOrdinalWindowBudget(
            maximumEntries: 96,
            maximumStoragePagesVisited: 8,
            maximumTreeNodesVisited: 128,
            maximumPackedEntriesInspected: 1024,
          ),
        );
        expect(ordinalWindow, isA<FlarkV3ExactDocumentOrdinalWindow>());
        final exactOrdinalWindow =
            ordinalWindow as FlarkV3ExactDocumentOrdinalWindow;
        expect(exactOrdinalWindow.totalBlockCount, greaterThan(10000));
        expect(exactOrdinalWindow.startBlockOrdinal, ordinalWindowStart);
        expect(
          exactOrdinalWindow.nextBlockOrdinal -
              exactOrdinalWindow.startBlockOrdinal,
          lessThanOrEqualTo(96),
        );
        expect(exactOrdinalWindow.storagePagesVisited, lessThanOrEqualTo(8));
        expect(exactOrdinalWindow.treeNodesVisited, lessThanOrEqualTo(128));
        expect(
          exactOrdinalWindow.packedEntriesInspected,
          lessThanOrEqualTo(1024),
        );
        expect(exactOrdinalWindow.summaryNodesSkipped, greaterThan(0));
        expect(
          exactOrdinalWindow.coveredSource.startUtf16,
          lessThan(targetStartUtf16),
        );
        expect(
          exactOrdinalWindow.coveredSource.endUtf16,
          greaterThan(targetStartUtf16 + targetMarkdown.length),
        );

        Set<FlarkV3InlineFactKind> activeStylesFor(String text) {
          final displayStart = harness.activeDisplayText.indexOf(text);
          expect(displayStart, greaterThanOrEqualTo(0));
          final displayEnd = displayStart + text.length;
          return {
            for (final run in production._activeBlock.runs)
              if (run.startUtf16 <= displayStart && run.endUtf16 >= displayEnd)
                ...run.styles,
          };
        }

        expect(
          activeStylesFor('β😀bold'),
          contains(FlarkV3InlineFactKind.strong),
        );
        expect(
          activeStylesFor('fluid'),
          contains(FlarkV3InlineFactKind.emphasis),
        );
        expect(activeStylesFor('code'), contains(FlarkV3InlineFactKind.code));

        final burstBaseRevision = production.runtime.sourceRevision;
        final burstBaseStructureGeneration =
            production.runtime.status.structureGeneration;
        final currentStructureRevisions = <int>[];
        final statusSubscription = production.runtime.statuses.listen((status) {
          if (status.sourceRevision > burstBaseRevision &&
              status.structureCurrent) {
            currentStructureRevisions.add(status.sourceRevision);
          }
        });
        addTearDown(statusSubscription.cancel);

        final deltaClient = editableState as DeltaTextInputClient;
        var currentDisplay = editingController.text;
        var insertionOffset = currentDisplay.indexOf('bold') + 'bold'.length;
        final callbackMicroseconds = <int>[];
        for (var index = 0; index < rapidInsertion.length; index += 1) {
          final character = rapidInsertion[index];
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
          expect(
            production.runtime.sourceRevision,
            burstBaseRevision + index + 1,
          );
        }
        expect(
          production.runtime.status.structureGeneration,
          burstBaseStructureGeneration,
          reason: 'the zero-cadence callbacks must not synchronously reparse',
        );

        await harness.waitForExactViewport();

        final finalRevision = burstBaseRevision + rapidInsertion.length;
        final sourceInsertionUtf16 =
            targetStartUtf16 + targetMarkdown.indexOf('bold') + 'bold'.length;
        final expectedSource = source.replaceRange(
          sourceInsertionUtf16,
          sourceInsertionUtf16,
          rapidInsertion,
        );
        expect(production.runtime.sourceRevision, finalRevision);
        expect(
          production.runtime.status.structureGeneration,
          burstBaseStructureGeneration + 1,
        );
        expect(currentStructureRevisions, isNotEmpty);
        expect(currentStructureRevisions, everyElement(finalRevision));
        expect(production.runtime.exportMarkdown(), expectedSource);
        expect(harness.activeDisplayText, contains('β😀boldswift'));
        expect(
          activeStylesFor('β😀boldswift'),
          contains(FlarkV3InlineFactKind.strong),
        );
        expect(harness.activeRecursiveGreenAuthorityCurrent, isTrue);
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(harness.createdTextEditingControllerCount, 1);
        expect(harness.exactPageLength, lessThanOrEqualTo(96));
        expect(harness.mountedPresentationCount, lessThanOrEqualTo(96));
        expect(
          observedDisplays,
          everyElement(
            allOf(
              isNotEmpty,
              isNot(contains('**')),
              isNot(contains('_fluid_')),
              isNot(contains('`code`')),
            ),
          ),
          reason: 'the live host must never repaint canonical markers',
        );
        final finalSetClientCalls = _setClientCalls(tester);
        expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
        expect(
          (finalSetClientCalls.last.arguments as List<dynamic>).first,
          clientId,
        );
        expect(
          Duration(microseconds: callbackMicroseconds.first),
          lessThan(const Duration(milliseconds: 25)),
        );
        expect(callbackMicroseconds.skip(1), everyElement(lessThan(2000)));
        expect(
          Duration(microseconds: callbackMicroseconds.reduce((a, b) => a + b)),
          lessThan(const Duration(milliseconds: 50)),
        );
      },
      skip: !_distantOrdinalLocatorAvailable,
      timeout: const Timeout(Duration(minutes: 3)),
    );

    testWidgets(
      'large RecursiveGreen headings keep nested references on one bounded client',
      (tester) async {
        const minimumDocumentUtf16 = 512 * 1024;
        const targetAtxMarkdown = '#### ATX _fluid_ [**alpha**][guide]';
        const initialSetextMarkdown =
            'Setext _fluid_ [**seed old value**][guide]\n=======';
        const finalSetextMarkdown =
            'Setext _fluid_ [**seed new value**][guide]\n=======';
        final sourceBuffer = StringBuffer()
          ..writeln('[guide]: /handbook "Handbook title"')
          ..writeln();
        var fillerIndex = 0;
        while (sourceBuffer.length <= minimumDocumentUtf16 + 8192) {
          final serial = fillerIndex.toString().padLeft(5, '0');
          sourceBuffer
            ..writeln('### Product section $serial')
            ..writeln()
            ..writeln(
              'Release note $serial keeps **strong** product context, '
              '_emphasis_, and a [guide][guide] reference while the reader '
              'moves through a realistically long manual.',
            )
            ..writeln();
          fillerIndex += 1;
        }
        final targetAtxStartUtf16 = sourceBuffer.length;
        sourceBuffer
          ..writeln(targetAtxMarkdown)
          ..writeln();
        final targetSetextStartUtf16 = sourceBuffer.length;
        sourceBuffer
          ..writeln(initialSetextMarkdown)
          ..writeln();
        for (var index = 0; index < 128; index += 1) {
          sourceBuffer
            ..writeln(
              'Following product note ${index.toString().padLeft(3, '0')} '
              'keeps the target away from the document tail.',
            )
            ..writeln();
        }
        final source = sourceBuffer.toString();
        final openingCaret = source.indexOf('Product section 00000') + 3;
        final targetAtxPoint =
            targetAtxStartUtf16 + targetAtxMarkdown.indexOf('alpha') + 2;
        final targetSetextValueStart =
            targetSetextStartUtf16 + initialSetextMarkdown.indexOf('old');
        final targetSetextPoint = targetSetextValueStart + 1;
        final deletedSource = source.replaceRange(
          targetSetextValueStart,
          targetSetextValueStart + 'old'.length,
          '',
        );
        final expectedSource = deletedSource.replaceRange(
          targetSetextValueStart,
          targetSetextValueStart,
          'new',
        );

        expect(source.length, greaterThan(minimumDocumentUtf16));
        expect(targetAtxStartUtf16, greaterThan(minimumDocumentUtf16));
        expect(targetSetextStartUtf16, greaterThan(minimumDocumentUtf16));

        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: openingCaret,
          viewportSize: const Size(640, 600),
        );
        addTearDown(harness.close);
        final production = harness as _ProductionVirtualizedSurfaceHarness;

        void expectBoundedSurface() {
          expect(harness.exactPageLength, lessThanOrEqualTo(96));
          expect(harness.mountedPresentationCount, lessThanOrEqualTo(96));
        }

        void expectNestedHeading(
          FlarkV3ParserAuthoredBlockPresentation block, {
          required String display,
          required String emphasisValue,
          required String referenceValue,
          required int level,
          required FlarkV3RecursiveGreenHeadingStyle style,
        }) {
          expect(block.kind, FlarkV3DocumentStructureKind.heading);
          expect(block.displayText, display);
          expect(block.headingLevel, level);
          final row = block.recursiveGreenRow;
          expect(row, isNotNull);
          expect(row!.kind, FlarkV3RecursiveGreenKind.heading);
          final headingFacts = row.path
              .map((frame) => frame.fact)
              .whereType<FlarkV3RecursiveGreenHeadingPathFact>()
              .toList(growable: false);
          expect(headingFacts, hasLength(1));
          expect(headingFacts.single.level, level);
          expect(headingFacts.single.style, style);

          final emphasisStart = block.displayText.indexOf(emphasisValue);
          expect(emphasisStart, greaterThanOrEqualTo(0));
          final emphasisEnd = emphasisStart + emphasisValue.length;
          final emphasisRuns = block.runs
              .where(
                (run) =>
                    run.endUtf16 > emphasisStart &&
                    run.startUtf16 < emphasisEnd,
              )
              .toList(growable: false);
          expect(emphasisRuns, isNotEmpty);
          expect(
            <FlarkV3InlineFactKind>{
              for (final run in emphasisRuns) ...run.styles,
            },
            contains(FlarkV3InlineFactKind.emphasis),
          );

          final referenceStart = block.displayText.indexOf(referenceValue);
          expect(referenceStart, greaterThanOrEqualTo(0));
          final referenceEnd = referenceStart + referenceValue.length;
          final referenceRuns = block.runs
              .where(
                (run) =>
                    run.endUtf16 > referenceStart &&
                    run.startUtf16 < referenceEnd,
              )
              .toList(growable: false);
          expect(referenceRuns, isNotEmpty);
          expect(
            <FlarkV3InlineFactKind>{
              for (final run in referenceRuns) ...run.styles,
            },
            contains(FlarkV3InlineFactKind.strong),
          );
          final references = referenceRuns
              .map((run) => run.linkAnnotation)
              .whereType<FlarkV3InlineLinkAnnotation>()
              .toList(growable: false);
          expect(references, isNotEmpty);
          expect(
            references.map((annotation) => annotation.kind),
            everyElement(FlarkV3InlineLinkKind.reference),
          );
          expect(
            references.map((annotation) => annotation.destination),
            everyElement('/handbook'),
          );
          expect(
            references.map((annotation) => annotation.title),
            everyElement('Handbook title'),
          );
        }

        await harness.waitForExactViewport();
        final editable = _editableText();
        final editableState = tester.state<EditableTextState>(editable);
        editableState.requestKeyboard();
        await tester.pump();
        final editingController = editableState.widget.controller;
        final initialSetClientCalls = _setClientCalls(tester);
        final clientId =
            (initialSetClientCalls.last.arguments as List<dynamic>).first
                as int;
        final observedDisplays = <String>[editingController.text];
        void recordDisplay() => observedDisplays.add(editingController.text);
        editingController.addListener(recordDisplay);
        addTearDown(() => editingController.removeListener(recordDisplay));

        await harness.revealAndActivateSourcePoint(targetAtxPoint);
        await harness.waitForExactViewport();
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(harness.activeRecursiveGreenAuthorityCurrent, isTrue);
        expectNestedHeading(
          production._activeBlock,
          display: 'ATX fluid alpha',
          emphasisValue: 'fluid',
          referenceValue: 'alpha',
          level: 4,
          style: FlarkV3RecursiveGreenHeadingStyle.atx,
        );
        final passiveSetext = production._exactSnapshot.blocks.singleWhere(
          (block) => _spanContains(block.physicalSource, targetSetextPoint),
        );
        expectNestedHeading(
          passiveSetext,
          display: 'Setext fluid seed old value',
          emphasisValue: 'fluid',
          referenceValue: 'seed old value',
          level: 1,
          style: FlarkV3RecursiveGreenHeadingStyle.setext,
        );
        expectBoundedSurface();

        await harness.revealAndActivateSourcePoint(targetSetextPoint);
        await harness.waitForExactViewport();
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(harness.activeRecursiveGreenAuthorityCurrent, isTrue);
        expectNestedHeading(
          production._activeBlock,
          display: 'Setext fluid seed old value',
          emphasisValue: 'fluid',
          referenceValue: 'seed old value',
          level: 1,
          style: FlarkV3RecursiveGreenHeadingStyle.setext,
        );
        expectBoundedSurface();

        final deltaClient = editableState as DeltaTextInputClient;
        const initialDisplay = 'Setext fluid seed old value';
        final retypeOffset = initialDisplay.indexOf('old');
        deltaClient.updateEditingValueWithDeltas([
          TextEditingDeltaDeletion(
            oldText: initialDisplay,
            deletedRange: TextRange(
              start: retypeOffset,
              end: retypeOffset + 'old'.length,
            ),
            selection: TextSelection.collapsed(offset: retypeOffset),
            composing: TextRange.empty,
          ),
        ]);
        await harness.waitForExactViewport();

        const deletedDisplay = 'Setext fluid seed  value';
        expect(production.runtime.exportMarkdown(), deletedSource);
        expectNestedHeading(
          production._activeBlock,
          display: deletedDisplay,
          emphasisValue: 'fluid',
          referenceValue: 'seed  value',
          level: 1,
          style: FlarkV3RecursiveGreenHeadingStyle.setext,
        );
        deltaClient.updateEditingValueWithDeltas([
          TextEditingDeltaInsertion(
            oldText: deletedDisplay,
            textInserted: 'new',
            insertionOffset: retypeOffset,
            selection: TextSelection.collapsed(offset: retypeOffset + 3),
            composing: TextRange.empty,
          ),
        ]);
        await harness.waitForExactViewport();

        expect(production.runtime.exportMarkdown(), expectedSource);
        expect(
          expectedSource.substring(
            targetSetextStartUtf16,
            targetSetextStartUtf16 + finalSetextMarkdown.length,
          ),
          finalSetextMarkdown,
        );
        expectNestedHeading(
          production._activeBlock,
          display: 'Setext fluid seed new value',
          emphasisValue: 'fluid',
          referenceValue: 'seed new value',
          level: 1,
          style: FlarkV3RecursiveGreenHeadingStyle.setext,
        );
        expect(harness.activeRecursiveGreenAuthorityCurrent, isTrue);
        expectBoundedSurface();
        expect(harness.createdTextEditingControllerCount, 1);
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(
          observedDisplays,
          everyElement(
            allOf(
              isNot(contains('####')),
              isNot(contains('=======')),
              isNot(contains('**')),
              isNot(contains('_')),
              isNot(contains('[guide]')),
            ),
          ),
        );
        final finalSetClientCalls = _setClientCalls(tester);
        expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
        expect(
          (finalSetClientCalls.last.arguments as List<dynamic>).first,
          clientId,
        );
      },
      skip: !_distantOrdinalLocatorAvailable,
      timeout: const Timeout(Duration(minutes: 3)),
    );

    testWidgets(
      'distant winning definition edit atomically recertifies reference family',
      (tester) async {
        const referenceUse =
            'Full [label][id], collapsed [id][], shortcut [id], '
            'image ![hero][asset], undefined [missing].';
        const oldDefinitions =
            '[id]: /old-target "Old title"\n'
            '[ID]: /ignored-duplicate "Ignored title"\n'
            '[asset]: /old.png "Old asset"\n';
        const newDefinitions =
            '[id]: /new-target "New title"\n'
            '[ID]: /ignored-duplicate "Ignored title"\n'
            '[asset]: /new.png "New asset"\n';
        final sourceBuffer = StringBuffer()
          ..writeln(referenceUse)
          ..writeln();
        for (var index = 0; index < 9000; index += 1) {
          sourceBuffer
            ..writeln(
              'Filler paragraph ${index.toString().padLeft(5, '0')} remains '
              'independent, source-backed, and long enough to make the '
              'definition mutation genuinely distant.',
            )
            ..writeln();
        }
        final definitionsStartUtf16 = sourceBuffer.length;
        sourceBuffer.write(oldDefinitions);
        final source = sourceBuffer.toString();
        final useCaret = source.indexOf('label') + 2;
        expect(source.length, greaterThan(512 * 1024));
        expect(definitionsStartUtf16, greaterThan(512 * 1024));

        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: useCaret,
          viewportSize: const Size(640, 600),
        );
        addTearDown(harness.close);
        final production = harness as _ProductionVirtualizedSurfaceHarness;

        List<FlarkV3InlineLinkAnnotation> activeLinks() {
          final bySource = <int, FlarkV3InlineLinkAnnotation>{};
          for (final run in production._activeBlock.runs) {
            final annotation = run.linkAnnotation;
            if (annotation != null) {
              bySource[annotation.source.startUtf16] = annotation;
            }
          }
          return bySource.values.toList(growable: false);
        }

        List<String> activeReferenceFamily() {
          final values = <String>[
            for (final annotation in activeLinks()) annotation.destination,
            for (final image in production._activeBlock.images)
              image.annotation.destination,
          ]..sort();
          return values;
        }

        await harness.waitForExactViewport();
        expect(harness.activeDisplayText, contains('Full label'));
        expect(harness.activeDisplayText, contains('collapsed id'));
        expect(harness.activeDisplayText, contains('shortcut id'));
        expect(harness.activeDisplayText, contains('image hero'));
        expect(
          harness.activeDisplayText,
          contains('undefined [missing]'),
          reason: 'a definitive undefined reference remains literal',
        );
        expect(
          harness.activeDisplayText,
          isNot(anyOf(contains('[label][id]'), contains('[id][]'))),
        );
        expect(activeLinks(), hasLength(3));
        expect(
          activeLinks().map((annotation) => annotation.destination),
          everyElement('/old-target'),
        );
        expect(
          activeLinks().map((annotation) => annotation.title),
          everyElement('Old title'),
        );
        expect(production._activeBlock.images, hasLength(1));
        expect(
          production._activeBlock.images.single.annotation.destination,
          '/old.png',
        );
        expect(
          production._activeBlock.images.single.annotation.title,
          'Old asset',
        );
        expect(activeReferenceFamily(), const <String>[
          '/old-target',
          '/old-target',
          '/old-target',
          '/old.png',
        ]);

        final editable = _editableText();
        final editableState = tester.state<EditableTextState>(editable);
        editableState.requestKeyboard();
        await tester.pump();
        final initialSetClientCalls = _setClientCalls(tester);
        final clientId =
            (initialSetClientCalls.last.arguments as List<dynamic>).first
                as int;
        final observedReferenceFamilies = <List<String>>[];
        void recordReferenceFamily() {
          final snapshot = production.presentationSource.snapshot;
          if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot) return;
          final block = snapshot.blocks
              .where(
                (candidate) =>
                    candidate.physicalSource.startUtf16 <= useCaret &&
                    candidate.physicalSource.endUtf16 > useCaret,
              )
              .firstOrNull;
          if (block == null) return;
          final links = <int, FlarkV3InlineLinkAnnotation>{};
          for (final run in block.runs) {
            final annotation = run.linkAnnotation;
            if (annotation != null) {
              links[annotation.source.startUtf16] = annotation;
            }
          }
          final family = <String>[
            for (final annotation in links.values) annotation.destination,
            for (final image in block.images) image.annotation.destination,
          ];
          if (family.isEmpty) return;
          observedReferenceFamilies.add(family..sort());
        }

        production.presentationSource.addListener(recordReferenceFamily);
        addTearDown(
          () => production.presentationSource.removeListener(
            recordReferenceFamily,
          ),
        );
        recordReferenceFamily();

        final baseRevision = production.runtime.sourceRevision;
        final edit = production.runtime.apply(
          FlarkV3SourceTransaction.single(
            baseRevision: baseRevision,
            operation: FlarkV3SourceEdit(
              startUtf16: definitionsStartUtf16,
              endUtf16: definitionsStartUtf16 + oldDefinitions.length,
              replacement: newDefinitions,
            ),
          ),
        );
        expect(edit.changed, isTrue);
        expect(edit.sourceRevision, baseRevision + 1);
        final expectedSource = source.replaceRange(
          definitionsStartUtf16,
          definitionsStartUtf16 + oldDefinitions.length,
          newDefinitions,
        );
        expect(production.runtime.exportMarkdown(), expectedSource);

        await harness.waitForExactViewport();

        expect(activeLinks(), hasLength(3));
        expect(
          activeLinks().map((annotation) => annotation.destination),
          everyElement('/new-target'),
        );
        expect(
          activeLinks().map((annotation) => annotation.title),
          everyElement('New title'),
        );
        expect(
          production._activeBlock.images.single.annotation.destination,
          '/new.png',
        );
        expect(
          production._activeBlock.images.single.annotation.title,
          'New asset',
        );
        expect(activeReferenceFamily(), const <String>[
          '/new-target',
          '/new-target',
          '/new-target',
          '/new.png',
        ]);
        expect(
          observedReferenceFamilies,
          everyElement(
            anyOf(
              equals(const <String>[
                '/old-target',
                '/old-target',
                '/old-target',
                '/old.png',
              ]),
              equals(const <String>[
                '/new-target',
                '/new-target',
                '/new-target',
                '/new.png',
              ]),
            ),
          ),
          reason:
              'one exact viewport may show the prior or target generation, '
              'but never a mixed reference family',
        );
        expect(production.runtime.exportMarkdown(), expectedSource);
        expect(harness.activeDisplayText, contains('undefined [missing]'));
        expect(harness.exactPageLength, lessThanOrEqualTo(96));
        expect(harness.mountedPresentationCount, lessThanOrEqualTo(96));
        expect(tester.state<EditableTextState>(editable), same(editableState));
        final finalSetClientCalls = _setClientCalls(tester);
        expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
        expect(
          (finalSetClientCalls.last.arguments as List<dynamic>).first,
          clientId,
        );
      },
      skip: !_distantOrdinalLocatorAvailable,
      timeout: const Timeout(Duration(minutes: 3)),
    );

    testWidgets(
      'dense viewport shrinks around the active leaf and stays settled',
      (tester) async {
        final source = _maximumAdversarialViewportMarkdown();
        final activeLine = source.split('\n')[32];
        final caret = source.indexOf(activeLine) + 4;
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: caret,
          viewportSize: const Size(640, 600),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();

        expect(harness.exactPageLength, anyOf(32, 16, 8, 4, 2, 1));
        expect(
          harness.visibleMaximumBlocks,
          harness.exactPageLength,
          reason: 'structural and parser presentation cuts must stay exact',
        );
        expect(
          harness.viewportAttemptOutcomeGeneration,
          greaterThanOrEqualTo(3),
          reason: 'the original 64-entry parser attempt must have reduced',
        );
        expect(harness.activeSourceContains(caret), isTrue);
        expect(harness.activeDisplayText, contains('x'));
        expect(harness.activeDisplayText, isNot(contains('**')));
        expect(harness.exportMarkdown(), source);

        final editable = _editableText();
        final editableState = tester.state<EditableTextState>(editable);
        editableState.requestKeyboard();
        await tester.pump();
        final clientsBefore = _setClientCalls(tester);
        final clientId =
            (clientsBefore.last.arguments as List<dynamic>).first as int;
        final outcomeBefore = harness.viewportAttemptOutcomeGeneration;
        final pageLengthBefore = harness.exactPageLength;
        expect(pageLengthBefore, 16);

        harness.requestWindowAtActive(maximumBlocks: 64);
        await tester.pump(const Duration(milliseconds: 50));

        expect(harness.viewportAttemptOutcomeGeneration, outcomeBefore);
        expect(harness.exactPageLength, pageLengthBefore);
        expect(tester.state<EditableTextState>(editable), same(editableState));
        final clientsAfter = _setClientCalls(tester);
        expect(clientsAfter, hasLength(clientsBefore.length));
        expect((clientsAfter.last.arguments as List<dynamic>).first, clientId);

        final editOutcomeBefore = harness.viewportAttemptOutcomeGeneration;
        final oldValue = editableState.widget.controller.value;
        final insertionOffset = oldValue.selection.extentOffset;
        (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
          TextEditingDeltaInsertion(
            oldText: oldValue.text,
            textInserted: 'z',
            insertionOffset: insertionOffset,
            selection: TextSelection.collapsed(offset: insertionOffset + 1),
            composing: TextRange.empty,
          ),
        ]);
        await harness.waitForExactViewport();

        expect(harness.exactPageLength, lessThanOrEqualTo(pageLengthBefore));
        expect(
          harness.viewportAttemptOutcomeGeneration,
          inInclusiveRange(editOutcomeBefore + 1, editOutcomeBefore + 2),
          reason:
              'the admitted dense cap should survive a source revision '
              'instead of rediscovering 64 then 32 on every keystroke; one '
              'retryable focused-inline preemption is permitted',
        );
        expect(harness.activeDisplayText, contains('z'));
        expect(tester.state<EditableTextState>(editable), same(editableState));
        expect(_setClientCalls(tester), hasLength(clientsAfter.length));
      },
      timeout: const Timeout(Duration(minutes: 2)),
    );

    testWidgets(
      'default compact window admits 32 inline leaves without shrinking',
      (tester) async {
        final paragraphs = List<String>.generate(
          49,
          (index) => 'Paragraph $index with **bold**.',
          growable: false,
        );
        final source = paragraphs.join('\n\n');
        final caret = source.indexOf(paragraphs[24]) + 4;
        final harness = await _VirtualizedSurfaceHarness.mount(
          tester,
          source: source,
          initialCaretUtf16: caret,
          viewportSize: const Size(640, 600),
        );
        addTearDown(harness.close);

        await harness.waitForExactViewport();

        expect(harness.totalCanonicalStructuralEntryCount, 97);
        expect(
          harness.exactPageLength,
          FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
          reason:
              'the 64-entry structural window contains 32 inline-bearing '
              'paragraphs and must fit the matching parser leaf budget '
              'without adaptive fallback',
        );
        expect(
          harness.visibleMaximumBlocks,
          FlarkV3VisibleBlockDemand.defaultMaximumBlocks,
        );
        expect(harness.exactParagraphCount, 32);
        expect(harness.activeSourceContains(caret), isTrue);
        expect(harness.activeDisplayText, contains('bold'));
        expect(harness.activeDisplayText, isNot(contains('**')));
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );
  });
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);

Finder _editableText() => find.byWidgetPredicate(
  (widget) => widget is EditableText,
  description: 'the single EditableText input host',
);

/// Test-only adapter boundary for the future production surface.
///
/// Its eventual implementation should mount the real widget and read its
/// revision-bound debug snapshots. It must not parse [source], strip markers,
/// create passive [TextEditingController]s, or eagerly materialize all blocks.
abstract interface class _VirtualizedSurfaceHarness {
  static Future<_VirtualizedSurfaceHarness> mount(
    WidgetTester tester, {
    required String source,
    required int initialCaretUtf16,
    required Size viewportSize,
  }) => _ProductionVirtualizedSurfaceHarness.mount(
    tester,
    source: source,
    initialCaretUtf16: initialCaretUtf16,
    viewportSize: viewportSize,
  );

  List<_PassiveBlockSnapshot> get passiveBlocks;
  int get activeOrdinal;
  FlarkV3DocumentStructureKind get activeKind;
  int? get activeHeadingLevel;
  String get activeDisplayText;
  int get totalCanonicalStructuralEntryCount;
  int get mountedPresentationCount;
  int get createdTextEditingControllerCount;
  int get exactPageLength;
  int get exactParagraphCount;
  int get visibleMaximumBlocks;
  int get viewportAttemptOutcomeGeneration;
  bool get activeRecursiveGreenAuthorityCurrent;

  _PassiveBlockSnapshot passiveBlockContainingSourcePoint(int positionUtf16);
  int passiveBuildCountContainingSourcePoint(int positionUtf16);
  String exportMarkdown();
  void requestWindowAtActive({required int maximumBlocks});

  Future<void> waitForExactViewport();
  Future<void> waitForActiveKind(FlarkV3DocumentStructureKind kind);
  bool activeSourceContains(int positionUtf16);
  Future<void> revealAndActivateSourcePoint(int positionUtf16);
  Future<void> close();
}

final class _ProductionVirtualizedSurfaceHarness
    implements _VirtualizedSurfaceHarness {
  _ProductionVirtualizedSurfaceHarness._({
    required this.tester,
    required this.runtime,
    required this.binding,
    required this.presentationSource,
    required this.surfaceController,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_ProductionVirtualizedSurfaceHarness> mount(
    WidgetTester tester, {
    required String source,
    required int initialCaretUtf16,
    required Size viewportSize,
  }) async {
    final runtime = await runManagedRuntimeAsyncForTest(
      tester,
      () => openManagedRuntimeForTest(source),
    );
    await runManagedRuntimeAsyncForTest(
      tester,
      () => runtime.initialReady.timeout(const Duration(seconds: 60)),
    );
    final island = _initialInputIsland(
      runtime,
      initialCaretUtf16,
      maximumUtf16: 8192,
    );
    final binding = FlarkV3ManagedFlutterBinding.attach(
      runtime: runtime,
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: island.startUtf16,
        maximumUtf16: 8192,
        value: TextEditingValue(
          text: runtime.readSourceRange(island.startUtf16, island.endUtf16),
          selection: TextSelection.collapsed(
            offset: initialCaretUtf16 - island.startUtf16,
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
    final presentationSource = runtime.sourceLengthUtf16 <= 8192
        ? binding.attachCompleteDocumentViewportPresentation()
        : binding.attachViewportPresentationAroundSourcePoint(
            sourcePointUtf16: initialCaretUtf16,
          );
    final surfaceController = FlarkV3VirtualizedLiveSurfaceController();
    final editableKey = GlobalKey<EditableTextState>();
    final focusNode = FocusNode();
    final harness = _ProductionVirtualizedSurfaceHarness._(
      tester: tester,
      runtime: runtime,
      binding: binding,
      presentationSource: presentationSource,
      surfaceController: surfaceController,
      editableKey: editableKey,
      focusNode: focusNode,
    );
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: SizedBox(
            width: viewportSize.width,
            height: viewportSize.height,
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
    final mountException = tester.takeException();
    if (mountException != null) {
      throw StateError('Surface mount failed: $mountException');
    }
    return harness;
  }

  final WidgetTester tester;
  final FlarkV3DocumentRuntime runtime;
  final FlarkV3ManagedFlutterBinding binding;
  final FlarkV3ManagedViewportPresentationSource presentationSource;
  final FlarkV3VirtualizedLiveSurfaceController surfaceController;
  final GlobalKey<EditableTextState> editableKey;
  final FocusNode focusNode;
  bool _closed = false;

  FlarkV3ExactViewportSurfaceSnapshot get _exactSnapshot {
    final value = presentationSource.snapshot;
    if (value is! FlarkV3ExactViewportSurfaceSnapshot) {
      throw StateError(
        'The production viewport is ${value.runtimeType}'
        '${switch (value) {
          FlarkV3SourceGapViewportSurfaceSnapshot(:final reason) => ' ($reason)',
          _ => '',
        }}.',
      );
    }
    return value;
  }

  FlarkV3ParserAuthoredBlockPresentation get _activeBlock {
    final exact = _exactSnapshot;
    return exact.blocks.singleWhere(
      (block) => block.ordinal == exact.activeOrdinal,
    );
  }

  @override
  List<_PassiveBlockSnapshot> get passiveBlocks {
    final exact = _exactSnapshot;
    return [
      for (final block in exact.blocks)
        if (block.ordinal != exact.activeOrdinal) _passiveSnapshot(block),
    ];
  }

  @override
  int get activeOrdinal => _exactSnapshot.activeOrdinal;

  @override
  FlarkV3DocumentStructureKind get activeKind => _activeBlock.kind;

  @override
  int? get activeHeadingLevel => _activeBlock.headingLevel;

  @override
  String get activeDisplayText => binding.controller.editingController.text;

  @override
  int get totalCanonicalStructuralEntryCount => _exactSnapshot.totalBlockCount;

  @override
  int get mountedPresentationCount =>
      surfaceController.mountedPresentationCount;

  @override
  int get createdTextEditingControllerCount => 1;

  @override
  int get exactPageLength => _exactSnapshot.blocks.length;

  @override
  int get exactParagraphCount => _exactSnapshot.blocks
      .where((block) => block.kind == FlarkV3DocumentStructureKind.paragraph)
      .length;

  @override
  int get visibleMaximumBlocks => binding.visibleBlocks.demand!.maximumBlocks;

  @override
  int get viewportAttemptOutcomeGeneration =>
      runtime.status.viewportPresentationAttemptOutcomeGeneration;

  @override
  bool get activeRecursiveGreenAuthorityCurrent {
    final active = _activeBlock;
    return active.recursiveGreenRow != null &&
        binding.controller.isExactCurrentPresentationFor(
          targetSourceVersion: active.identity.sourceVersion,
          targetPhysicalSource: active.physicalSource,
          targetKind: active.kind,
          targetDisplayText: active.displayText,
          targetRecursiveGreenAck: active.recursiveGreenStructuralAck,
          targetRecursiveGreenRow: active.recursiveGreenRow,
        );
  }

  @override
  _PassiveBlockSnapshot passiveBlockContainingSourcePoint(int positionUtf16) {
    final block = _exactSnapshot.blocks.singleWhere(
      (candidate) =>
          candidate.ordinal != _exactSnapshot.activeOrdinal &&
          _spanContains(candidate.physicalSource, positionUtf16),
    );
    return _passiveSnapshot(block);
  }

  @override
  int passiveBuildCountContainingSourcePoint(int positionUtf16) {
    final block = _exactSnapshot.blocks.singleWhere(
      (candidate) =>
          candidate.ordinal != _exactSnapshot.activeOrdinal &&
          _spanContains(candidate.physicalSource, positionUtf16),
    );
    return surfaceController.passiveBuildCount(block.ordinal);
  }

  @override
  String exportMarkdown() => runtime.exportMarkdown();

  @override
  void requestWindowAtActive({required int maximumBlocks}) {
    presentationSource.requestWindow(
      FlarkV3ViewportWindowDemand(
        centerOrdinal: activeOrdinal,
        maximumBlocks: maximumBlocks,
      ),
    );
  }

  @override
  bool activeSourceContains(int positionUtf16) =>
      _spanContains(_activeBlock.physicalSource, positionUtf16);

  @override
  Future<void> waitForExactViewport() async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 10)) {
      await tester.pump(const Duration(milliseconds: 1));
      await runManagedRuntimeAsyncForTest(
        tester,
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final snapshot = presentationSource.snapshot;
      if (snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
          runtime.status.structureCurrent) {
        final active = snapshot.blocks
            .where((block) => block.ordinal == snapshot.activeOrdinal)
            .firstOrNull;
        final query = binding.controller.paintState.documentQuery;
        final selection =
            binding.controller.globalEditingState.selection.extentOffset;
        final inlineActive =
            active?.kind == FlarkV3DocumentStructureKind.paragraph ||
            active?.kind == FlarkV3DocumentStructureKind.heading;
        if (active?.recursiveGreenRow != null &&
            query is FlarkV3RecursiveGreenPointQuery &&
            _spanContains(active!.physicalSource, selection) &&
            (!inlineActive ||
                binding.controller.hasCertifiedInlinePresentation) &&
            activeRecursiveGreenAuthorityCurrent) {
          await tester.pump();
          return;
        }
        if (active != null &&
            binding.visibleBlocks.phase ==
                FlarkV3FlutterVisibleBlockPhase.exact &&
            query is FlarkV3DocumentStructuralQuery &&
            binding.controller.semanticActionsValid &&
            active.kind == query.structure.kind &&
            _spanContains(active.physicalSource, selection) &&
            _spanContains(query.structure.source, selection) &&
            (!inlineActive ||
                binding.controller.hasCertifiedInlinePresentation)) {
          await tester.pump();
          return;
        }
      }
    }
    throw TestFailure(
      'Timed out waiting for exact production viewport: '
      'status=${runtime.status.state.name}, '
      'sourceCurrent=${runtime.status.sourceCurrent}, '
      'structureCurrent=${runtime.status.structureCurrent}, '
      'visible=${binding.visibleBlocks.phase.name}, '
      'viewport=${runtime.status.viewportPresentationGeneration}/'
      '${runtime.status.viewportPresentationAttemptOutcomeGeneration}, '
      'unavailable=${runtime.status.viewportPresentationUnavailableReason}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${binding.controller.paintState.documentQuery.runtimeType}, '
      'inline=${binding.controller.hasCertifiedInlinePresentation}/'
      '${runtime.status.inlinePresentationGeneration}/'
      '${runtime.status.inlineAttemptOutcomeGeneration}, '
      'island=${binding.controller.inputIslandGlobalStartUtf16}..'
      '${binding.controller.inputIslandGlobalEndUtf16}, '
      'frames=${binding.controller.scheduledFrameCallbacks}/'
      '${binding.controller.appliedPresentationFrames}, '
      'snapshot=${presentationSource.snapshot.runtimeType}, '
      'gap=${switch (presentationSource.snapshot) {
        FlarkV3SourceGapViewportSurfaceSnapshot(:final reason) => reason,
        _ => null,
      }}.',
    );
  }

  @override
  Future<void> waitForActiveKind(FlarkV3DocumentStructureKind kind) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 10)) {
      await tester.pump(const Duration(milliseconds: 1));
      await runManagedRuntimeAsyncForTest(
        tester,
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final snapshot = presentationSource.snapshot;
      if (snapshot is FlarkV3ExactViewportSurfaceSnapshot) {
        final active = snapshot.blocks
            .where((block) => block.ordinal == snapshot.activeOrdinal)
            .firstOrNull;
        final query = binding.controller.paintState.documentQuery;
        if (active?.kind == kind &&
            query is FlarkV3DocumentStructuralQuery &&
            query.structure.kind == kind) {
          await tester.pump();
          return;
        }
      }
    }
    throw TestFailure(
      'Timed out waiting for active ${kind.name}: '
      'source=${runtime.exportMarkdown()}, '
      'paint=${binding.controller.paintState.mode.name}, '
      'query=${binding.controller.paintState.documentQuery.runtimeType}, '
      'snapshot=${presentationSource.snapshot.runtimeType}.',
    );
  }

  @override
  Future<void> revealAndActivateSourcePoint(int positionUtf16) {
    presentationSource.revealAndActivateSourcePoint(positionUtf16);
    return Future<void>.value();
  }

  @override
  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump();
    binding.dispose();
    focusNode.dispose();
    if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
      await runManagedRuntimeAsyncForTest(
        tester,
        () => runtime.close().timeout(const Duration(seconds: 5)),
      );
    }
  }
}

String _maximumAdversarialViewportMarkdown() {
  final facts = List<String>.filled(32, '**x**').join(' ');
  final padding = List<String>.filled(800, 'p').join();
  return List<String>.generate(
    64,
    (index) => '# $facts $padding $index',
    growable: false,
  ).join('\n');
}

_PassiveBlockSnapshot _passiveSnapshot(
  FlarkV3ParserAuthoredBlockPresentation block,
) => _PassiveBlockSnapshot(
  ordinal: block.ordinal,
  physicalSource: block.physicalSource,
  kind: block.kind,
  displayText: block.displayText,
  runs: [
    for (final run in block.runs)
      _PassiveInlineRunSnapshot(
        startUtf16: run.startUtf16,
        endUtf16: run.endUtf16,
        styles: run.styles,
      ),
  ],
  headingLevel: block.headingLevel,
);

bool _spanContains(FlarkV3SourceSpan span, int positionUtf16) =>
    positionUtf16 >= span.startUtf16 && positionUtf16 < span.endUtf16;

({int startUtf16, int endUtf16}) _initialInputIsland(
  FlarkV3DocumentRuntime runtime,
  int positionUtf16, {
  required int maximumUtf16,
}) {
  if (runtime.sourceLengthUtf16 <= maximumUtf16) {
    return (startUtf16: 0, endUtf16: runtime.sourceLengthUtf16);
  }
  final rangeStart = positionUtf16 == runtime.sourceLengthUtf16
      ? positionUtf16 - 1
      : positionUtf16;
  final result = runtime.queryBlockRange(
    rangeStart,
    rangeStart + 1,
    budget: const FlarkV3DocumentBlockRangeBudget(
      maximumEncodedBytes: 64 * 1024,
      maximumBlockCount: 8,
      maximumStoragePagesVisited: 9,
      maximumOpenDepth: 64,
      maximumTreeNodesVisited: 512,
    ),
  );
  if (result is FlarkV3RecursiveGreenRowRange) {
    for (final row in result.rows) {
      final span = row.editableSource;
      if (span == null) continue;
      if (positionUtf16 >= span.startUtf16 &&
          positionUtf16 <= span.endUtf16 &&
          span.endUtf16 - span.startUtf16 <= maximumUtf16) {
        return (startUtf16: span.startUtf16, endUtf16: span.endUtf16);
      }
    }
  }
  if (result is FlarkV3DocumentStructuralBlockRange) {
    for (final block in result.blocks) {
      final span = block.structure.source;
      if (positionUtf16 >= span.startUtf16 &&
          positionUtf16 <= span.endUtf16 &&
          span.endUtf16 - span.startUtf16 <= maximumUtf16) {
        return (startUtf16: span.startUtf16, endUtf16: span.endUtf16);
      }
    }
  }
  if (result is FlarkV3RecursiveGreenRowRange) {
    for (final row in result.rows) {
      final span = row.editableSource;
      if (span != null &&
          positionUtf16 >= span.startUtf16 &&
          positionUtf16 <= span.endUtf16 &&
          span.endUtf16 - span.startUtf16 <= maximumUtf16) {
        return (startUtf16: span.startUtf16, endUtf16: span.endUtf16);
      }
    }
  }
  throw TestFailure(
    'Parser range authority could not provide a bounded initial input island '
    'at source point $positionUtf16 (${result.runtimeType}).',
  );
}

/// Immutable inspection value supplied by the production surface's debug seam.
///
/// Text and styles must originate from one exact parser-authored presentation,
/// not from expectations or fixture-specific transformations in the harness.
final class _PassiveBlockSnapshot {
  const _PassiveBlockSnapshot({
    required this.ordinal,
    required this.physicalSource,
    required this.kind,
    required this.displayText,
    required this.runs,
    required this.headingLevel,
  });

  final int ordinal;
  final FlarkV3SourceSpan physicalSource;
  final FlarkV3DocumentStructureKind kind;
  final String displayText;
  final List<_PassiveInlineRunSnapshot> runs;
  final int? headingLevel;

  Set<FlarkV3InlineFactKind> stylesForText(String text) {
    final start = displayText.indexOf(text);
    if (start < 0) {
      throw StateError('Passive display does not contain "$text".');
    }
    final end = start + text.length;
    return {
      for (final run in runs)
        if (run.startUtf16 <= start && run.endUtf16 >= end) ...run.styles,
    };
  }
}

final class _PassiveInlineRunSnapshot {
  const _PassiveInlineRunSnapshot({
    required this.startUtf16,
    required this.endUtf16,
    required this.styles,
  });

  final int startUtf16;
  final int endUtf16;
  final Set<FlarkV3InlineFactKind> styles;
}

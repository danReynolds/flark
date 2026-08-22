@TestOn('browser')
library;

import 'package:example/v3_engine_lab.dart';
import 'package:example/v3_engine_lab_web_asset_version.dart';
import 'package:example/v3_live_editor_checkpoint.dart'
    show v3LiveCheckpointMarkdown;
import 'package:flark/flark_v3.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('mirrored Flutter Worker and Wasm open the public runtime', () async {
    final assets = v3EngineLabWebAssets(flutterTestPackageServer: true);
    expect(
      assets.workerUri.path,
      '/packages/flark_flutter/assets/worker/flark_v3_parser_worker.js',
    );
    expect(
      assets.wasmUri.path,
      '/packages/flark_flutter/assets/wasm/flark_comrak_bridge.wasm',
    );
    expect(
      assets.workerUri.queryParameters['flark-build'],
      v3EngineLabWebAssetVersion,
    );
    expect(
      assets.wasmUri.queryParameters['flark-build'],
      v3EngineLabWebAssetVersion,
    );

    final runtime = await FlarkV3DocumentRuntime.open(
      'small exact paragraph',
      webAssets: assets,
    );
    addTearDown(runtime.close);

    await runtime.initialReady.timeout(const Duration(seconds: 30));
    if (!runtime.status.structureCurrent) {
      await runtime.statuses
          .firstWhere((status) => status.structureCurrent)
          .timeout(const Duration(seconds: 30));
    }
    expect(runtime.status.sourceCurrent, isTrue);
    expect(runtime.status.structureCurrent, isTrue);

    final initial = runtime.queryAtUtf16(1);
    expect(initial, isA<FlarkV3DocumentStructuralQuery>());
    expect(
      (initial as FlarkV3DocumentStructuralQuery).structure.kind,
      FlarkV3DocumentStructureKind.paragraph,
    );

    final nextRevision = runtime.sourceRevision + 1;
    final exactCurrent = runtime.statuses.firstWhere(
      (status) =>
          status.sourceRevision == nextRevision && status.structureCurrent,
    );
    final receipt = runtime.apply(
      FlarkV3SourceTransaction.single(
        baseRevision: runtime.sourceRevision,
        operation: FlarkV3SourceEdit(
          startUtf16: runtime.sourceLengthUtf16,
          endUtf16: runtime.sourceLengthUtf16,
          replacement: ' edited',
        ),
      ),
    );
    expect(receipt.changed, isTrue);
    await exactCurrent.timeout(const Duration(seconds: 30));

    final edited = runtime.queryAtUtf16(runtime.sourceLengthUtf16);
    expect(edited, isA<FlarkV3DocumentStructuralQuery>());
    expect(edited.sourceRevision, nextRevision);

    await runtime.close().timeout(const Duration(seconds: 30));
    expect(runtime.status.state, FlarkV3DocumentRuntimeState.closed);
  });

  test(
    'mirrored Worker and Wasm certify the live checkpoint document',
    () async {
      final runtime = await FlarkV3DocumentRuntime.open(
        v3LiveCheckpointMarkdown,
        webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
      ).timeout(const Duration(seconds: 30));
      addTearDown(runtime.close);

      await runtime.initialReady.timeout(const Duration(seconds: 30));
      expect(runtime.status.sourceCurrent, isTrue);
      expect(runtime.status.structureCurrent, isTrue);
    },
  );

  testWidgets(
    'Checkpoint B runs through the packaged Worker and Wasm',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );

      final runProof = find.byKey(const Key('v3-engine-lab-checkpoint-b-run'));
      await tester.ensureVisible(runProof);
      await tester.pump();
      await tester.tap(runProof);
      await tester.pump();
      await _pumpUntil(
        tester,
        () => find
            .byKey(const Key('v3-engine-lab-checkpoint-b-pass'))
            .evaluate()
            .isNotEmpty,
        timeout: const Duration(seconds: 60),
        description: 'Checkpoint B production proof receipt',
      );

      expect(find.textContaining('prefix insertion'), findsOneWidget);
      expect(find.textContaining('middle Unicode replacement'), findsOneWidget);
      expect(find.textContaining('tail insertion'), findsOneWidget);
      expect(find.textContaining('split CRLF insertion'), findsOneWidget);
      expect(
        find.byKey(const Key('v3-engine-lab-checkpoint-b-parity')),
        findsOneWidget,
      );
      expect(find.textContaining('close reached zero'), findsOneWidget);

      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 75)),
  );

  testWidgets(
    'small seed shows parser-certified inline editing',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final openSmall = find.text('Open ${V3EngineLabSeed.small.label}');
      await tester.ensureVisible(openSmall);
      await tester.pump();
      await tester.tap(openSmall);
      await tester.pump();

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified hidden inline rendering active')
            .evaluate()
            .isNotEmpty,
        description: 'small-seed certified inline presentation',
      );

      expect(
        find.byKey(const Key('v3-engine-lab-live-editor')),
        findsOneWidget,
      );
      final editable = tester.widget<EditableText>(
        find.byKey(const Key('v3-engine-lab-editor')),
      );
      expect(editable.controller.text, v3EngineLabEditableTailDisplay);
      expect(editable.controller.text, isNot(contains('**Bold**')));
      expect(editable.controller.text, isNot(contains('_emphasis_')));
      expect(editable.controller.text, isNot(contains('`code`')));
      expect(editable.controller.text, isNot(contains('~~strike~~')));
      expect(editable.controller.text, isNot(contains('[flark]: /target')));
      expect(editable.controller.text, contains('©'));
      expect(editable.controller.text, contains('≧\u{338}'));
      expect(editable.controller.text, contains('https://e.test/?q=&'));
      expect(editable.controller.text, isNot(contains('&copy;')));
      expect(editable.controller.text, isNot(contains('&ngE;')));
      expect(editable.controller.text, isNot(contains('&amp;')));
      expect(editable.controller.text, contains('Escaped * punctuation'));
      expect(editable.controller.text, isNot(contains(r'\*')));
      expect(
        editable.controller.text,
        contains(
          'canonical source remains intact.\n'
          'A parser-certified hard break',
        ),
      );
      expect(
        editable.controller.text,
        isNot(contains('canonical source remains intact.  \n')),
      );
      final exactSource = tester.widget<SelectableText>(
        find.byKey(const Key('v3-engine-lab-exact-source')),
      );
      expect(exactSource.data, v3EngineLabEditableTailSource);
      expect(exactSource.data, contains('&copy;'));
      expect(exactSource.data, contains('&ngE;'));
      expect(exactSource.data, contains('<https://e.test/?q=&amp;>'));
      expect(exactSource.data, contains(r'\*'));
      expect(
        exactSource.data,
        contains('canonical source remains intact.  \n'),
      );

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );

  testWidgets(
    'small seed converges after sequential insert/delete platform key deltas',
    (tester) async {
      await _runSequentialKeyTrial(tester);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'small seed coalesces a rapid platform key burst and stays marker-free',
    (tester) async {
      await _runSequentialKeyTrial(tester, cadence: Duration.zero);
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'multi-block checkpoint moves marker-free Paragraphs on one client',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.multiBlockParagraph;
      final openMultiBlock = find.text('Open ${seed.label}');
      await tester.ensureVisible(openMultiBlock);
      await tester.pump();
      await tester.tap(openMultiBlock);
      await tester.pump();

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified hidden inline rendering active')
            .evaluate()
            .isNotEmpty,
        description: 'selected middle-Paragraph presentation',
      );
      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final editable = tester.widget<EditableText>(editorFinder);
      final editableState = tester.state<EditableTextState>(editorFinder);
      final deltaClient = editableState as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabMultiBlockMiddleDisplay);
      expect(editable.controller.text, isNot(contains('**')));
      expect(editable.controller.text, isNot(contains('_')));
      expect(editable.controller.text, isNot(contains('`')));
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-multi-block-source'),
              ),
            )
            .data,
        v3EngineLabMultiBlockSource,
      );

      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      final clientId =
          (initialSetClientCalls.last.arguments as List<dynamic>).first as int;
      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: v3EngineLabMultiBlockMiddleDisplay,
          textInserted: '!',
          insertionOffset: 6,
          selection: TextSelection.collapsed(offset: 7),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(
        editable.controller.text,
        v3EngineLabMultiBlockMiddleDisplay.replaceRange(6, 6, '!'),
      );
      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            find
                .text('Parser-certified hidden inline rendering active')
                .evaluate()
                .isNotEmpty,
        description: 'middle-Paragraph edit to become exact-current',
      );
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-multi-block-source'),
              ),
            )
            .data,
        contains('**Middle**!'),
      );

      final selectTail = find.byKey(
        const Key('v3-engine-lab-select-tail-paragraph'),
      );
      await tester.ensureVisible(selectTail);
      await tester.pump();
      await tester.tap(selectTail);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            editable.controller.text == 'Tail paragraph remains canonical.\n' &&
            find
                .text('Parser-certified hidden inline rendering active')
                .evaluate()
                .isNotEmpty,
        description: 'tail Paragraph to become marker-free',
      );

      final selectFirst = find.byKey(
        const Key('v3-engine-lab-select-first-paragraph'),
      );
      await tester.ensureVisible(selectFirst);
      await tester.pump();
      await tester.tap(selectFirst);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            editable.controller.text ==
                'First paragraph stays outside the active island.\n' &&
            find
                .text('Parser-certified hidden inline rendering active')
                .evaluate()
                .isNotEmpty,
        description: 'first Paragraph to become marker-free',
      );
      expect(
        tester.state<EditableTextState>(editorFinder),
        same(editableState),
      );
      final finalSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
      expect(
        (finalSetClientCalls.last.arguments as List<dynamic>).first,
        clientId,
      );

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'ATX checkpoint hides block and inline markers while editing canonical source',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.atxHeading;
      final openHeading = find.text('Open ${seed.label}');
      await tester.ensureVisible(openHeading);
      await tester.pump();
      await tester.tap(openHeading);
      await tester.pump();

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified marker-free ATX heading active')
            .evaluate()
            .isNotEmpty,
        description: 'ATX heading presentation',
      );

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final editable = tester.widget<EditableText>(editorFinder);
      final deltaClient =
          tester.state<EditableTextState>(editorFinder) as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabAtxHeadingDisplay);
      expect(editable.controller.text, isNot(contains('#')));
      expect(editable.controller.text, isNot(contains('**')));
      expect(editable.controller.text, isNot(contains('_')));
      expect(editable.style.fontSize, 24);
      expect(editable.style.fontWeight, FontWeight.w700);
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-atx-heading-source'),
              ),
            )
            .data,
        v3EngineLabAtxHeadingSource,
      );

      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      final insertionOffset = v3EngineLabAtxHeadingDisplay.indexOf('live');
      deltaClient.updateEditingValueWithDeltas([
        TextEditingDeltaInsertion(
          oldText: v3EngineLabAtxHeadingDisplay,
          textInserted: 'fluid ',
          insertionOffset: insertionOffset,
          selection: TextSelection.collapsed(
            offset: insertionOffset + 'fluid '.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(editable.controller.text, 'β😀 fluid live heading');
      expect(editable.controller.text, isNot(contains('#')));

      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            find
                .text('Parser-certified marker-free ATX heading active')
                .evaluate()
                .isNotEmpty,
        description: 'edited ATX heading to become exact-current',
      );
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-atx-heading-source'),
              ),
            )
            .data,
        '## **β😀** fluid live _heading_ ###\r\n',
      );
      _expectNoFrameworkException(tester, 'editing the ATX heading');

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );

  testWidgets(
    'Setext checkpoint preserves H2 and canonical underline on one client',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.setextHeading;
      final openHeading = find.text('Open ${seed.label}');
      await tester.ensureVisible(openHeading);
      await tester.pump();
      await tester.tap(openHeading);
      await tester.pump();

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified marker-free Setext heading active')
            .evaluate()
            .isNotEmpty,
        description: 'Setext H2 presentation',
      );
      _expectNoFrameworkException(tester, 'opening the Setext H2');

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final editable = tester.widget<EditableText>(editorFinder);
      final editableState = tester.state<EditableTextState>(editorFinder);
      expect(editable.controller.text, v3EngineLabSetextHeadingDisplay);
      expect(editable.controller.text, isNot(contains('---')));
      expect(editable.controller.text, isNot(contains('**')));
      expect(editable.controller.text, isNot(contains('_')));
      expect(editable.controller.text, isNot(contains('\r\n')));
      expect(editable.style.fontSize, 24);
      expect(editable.style.fontWeight, FontWeight.w700);
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-setext-heading-source'),
              ),
            )
            .data,
        v3EngineLabSetextHeadingSource,
      );

      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      final clientId =
          (initialSetClientCalls.last.arguments as List<dynamic>).first as int;
      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      final presentationBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-inline-presentation-generation'),
      );
      final insertionOffset = v3EngineLabSetextHeadingDisplay.indexOf('live');
      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaInsertion(
          oldText: v3EngineLabSetextHeadingDisplay,
          textInserted: 'fluid ',
          insertionOffset: insertionOffset,
          selection: TextSelection.collapsed(
            offset: insertionOffset + 'fluid '.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(editable.controller.text, 'β😀 fluid live heading');
      expect(editable.controller.text, isNot(contains('---')));
      expect(editable.controller.text, isNot(contains('**')));
      expect(editable.controller.text, isNot(contains('_')));

      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            int.parse(
                  _metricValue(
                    tester,
                    'v3-engine-lab-inline-presentation-generation',
                  ),
                ) >
                presentationBefore &&
            find
                .text('Parser-certified marker-free Setext heading active')
                .evaluate()
                .isNotEmpty,
        description: 'edited Setext H2 to recertify',
      );
      final canonicalSource = tester
          .widget<SelectableText>(
            find.byKey(
              const Key('v3-engine-lab-canonical-setext-heading-source'),
            ),
          )
          .data;
      expect(canonicalSource, '**β😀** fluid live _heading_\r\n---\r\n');
      expect(canonicalSource, endsWith('\r\n---\r\n'));
      expect(
        tester.state<EditableTextState>(editorFinder),
        same(editableState),
      );
      final finalSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
      expect(
        (finalSetClientCalls.last.arguments as List<dynamic>).first,
        clientId,
      );
      _expectNoFrameworkException(tester, 'editing the Setext H2');

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );

  testWidgets(
    'thematic-break checkpoint paints one atom and deletes it on one client',
    (tester) async {
      for (final deletionKey in const [
        LogicalKeyboardKey.backspace,
        LogicalKeyboardKey.delete,
      ]) {
        await tester.pumpWidget(
          V3EngineLabApp(
            openOnStart: false,
            webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
          ),
        );
        final seed = V3EngineLabSeed.thematicBreak;
        final openThematicBreak = find.text('Open ${seed.label}');
        await tester.ensureVisible(openThematicBreak);
        await tester.pump();
        await tester.tap(openThematicBreak);
        await tester.pump();

        await _pumpUntil(
          tester,
          () =>
              find
                  .text(
                    'Parser-certified atomic marker-free thematic break active',
                  )
                  .evaluate()
                  .isNotEmpty &&
              find
                  .byKey(const Key('flark-v3-thematic-break'))
                  .evaluate()
                  .isNotEmpty,
          description: 'atomic thematic-break presentation',
        );
        _expectNoFrameworkException(tester, 'opening the thematic break');

        final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
        final editable = tester.widget<EditableText>(editorFinder);
        final editableState = tester.state<EditableTextState>(editorFinder);
        expect(editable.controller.text, v3EngineLabThematicBreakDisplay);
        expect(editable.controller.text, isEmpty);
        expect(editable.controller.text, isNot(contains('*')));
        expect(
          find.byKey(const Key('flark-v3-thematic-break')),
          findsOneWidget,
        );
        expect(
          tester
              .widget<SelectableText>(
                find.byKey(
                  const Key('v3-engine-lab-canonical-thematic-break-source'),
                ),
              )
              .data,
          v3EngineLabThematicBreakSource,
        );

        editableState.requestKeyboard();
        await tester.pump();
        final initialSetClientCalls = tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .toList(growable: false);
        final clientId =
            (initialSetClientCalls.last.arguments as List<dynamic>).first
                as int;
        final revisionBefore = int.parse(
          _metricValue(tester, 'v3-engine-lab-source-revision'),
        );

        await tester.sendKeyEvent(deletionKey);
        await tester.pump();
        await _pumpUntil(
          tester,
          () =>
              _metricValue(tester, 'v3-engine-lab-source-revision') ==
                  '${revisionBefore + 1}' &&
              _metricValue(tester, 'v3-engine-lab-source-length') == '0 u16' &&
              _metricValue(tester, 'v3-engine-lab-structure-current') ==
                  'yes' &&
              find
                  .byKey(const Key('flark-v3-thematic-break'))
                  .evaluate()
                  .isEmpty,
          description:
              '${deletionKey.keyLabel} to delete the whole thematic-break atom',
        );
        expect(
          tester
              .widget<SelectableText>(
                find.byKey(
                  const Key('v3-engine-lab-canonical-thematic-break-source'),
                ),
              )
              .data,
          isEmpty,
        );
        expect(editable.controller.text, isEmpty);
        expect(
          tester.state<EditableTextState>(editorFinder),
          same(editableState),
        );
        final finalSetClientCalls = tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .toList(growable: false);
        expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
        expect(
          (finalSetClientCalls.last.arguments as List<dynamic>).first,
          clientId,
        );
        _expectNoFrameworkException(
          tester,
          '${deletionKey.keyLabel} deleting the thematic break',
        );

        await _closeLabRuntime(tester);
        await tester.pumpWidget(const SizedBox.shrink());
      }
    },
    timeout: const Timeout(Duration(seconds: 90)),
  );

  testWidgets(
    'fenced-code checkpoint hides fence syntax and edits literal code live',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.fencedCode;
      final openFence = find.text('Open ${seed.label}');
      await tester.ensureVisible(openFence);
      await tester.pump();
      await tester.tap(openFence);
      await tester.pump();

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified fenced code body active')
            .evaluate()
            .isNotEmpty,
        description: 'fenced-code body presentation',
      );

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final editable = tester.widget<EditableText>(editorFinder);
      final deltaClient =
          tester.state<EditableTextState>(editorFinder) as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabFencedCodeBody);
      expect(editable.controller.text, contains('**literal Markdown**'));
      expect(editable.controller.text, isNot(contains('```')));
      expect(editable.controller.text, isNot(startsWith('dart')));
      expect(editable.style.fontFamily, 'monospace');
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(const Key('v3-engine-lab-canonical-fence-source')),
            )
            .data,
        v3EngineLabFencedCodeSource,
        reason:
            'the instrumentation view proves hidden fence syntax remains '
            'canonical source outside the live editor',
      );

      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      final insertionOffset = v3EngineLabFencedCodeBody.indexOf('literal');
      deltaClient.updateEditingValueWithDeltas([
        TextEditingDeltaInsertion(
          oldText: v3EngineLabFencedCodeBody,
          textInserted: 'live ',
          insertionOffset: insertionOffset,
          selection: TextSelection.collapsed(
            offset: insertionOffset + 'live '.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(
        editable.controller.text,
        v3EngineLabFencedCodeBody.replaceRange(
          insertionOffset,
          insertionOffset,
          'live ',
        ),
      );
      expect(editable.controller.text, isNot(contains('```')));

      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            find
                .text('Parser-certified fenced code body active')
                .evaluate()
                .isNotEmpty,
        description: 'fenced-code edit to become exact-current',
      );
      _expectNoFrameworkException(
        tester,
        'certifying the marker-free fenced-code edit',
      );

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );

  testWidgets(
    'tight bullet checkpoint is marker-free, source-exact, and stays on one client',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.bulletList;
      final openList = find.text('Open ${seed.label}');
      await tester.ensureVisible(openList);
      await tester.pump();
      await tester.tap(openList);
      await tester.pump();

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      await _pumpUntil(
        tester,
        () =>
            find
                .text('Parser-certified marker-free bullet-list item active')
                .evaluate()
                .isNotEmpty &&
            editorFinder.evaluate().isNotEmpty &&
            tester.widget<EditableText>(editorFinder).controller.text ==
                v3EngineLabBulletListSecondDisplay,
        description: 'selected bullet-list item and inline presentation',
      );

      final editable = tester.widget<EditableText>(editorFinder);
      final editableState = tester.state<EditableTextState>(editorFinder);
      final deltaClient = editableState as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabBulletListSecondDisplay);
      expect(
        editable.controller.text,
        isNot(
          anyOf(contains('- '), contains('**'), contains('_'), contains('`')),
        ),
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
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-bullet-list-source'),
              ),
            )
            .data,
        v3EngineLabBulletListSource,
      );

      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      final clientId =
          (initialSetClientCalls.last.arguments as List<dynamic>).first as int;

      final selectFirst = find.byKey(
        const Key('v3-engine-lab-select-first-list-item'),
      );
      await tester.ensureVisible(selectFirst);
      await tester.pump();
      await tester.tap(selectFirst);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            editable.controller.text == v3EngineLabBulletListFirstDisplay &&
            find
                .text('Parser-certified marker-free bullet-list item active')
                .evaluate()
                .isNotEmpty,
        description: 'first bullet-list item handoff',
      );

      final selectSecond = find.byKey(
        const Key('v3-engine-lab-select-second-list-item'),
      );
      await tester.tap(selectSecond);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            editable.controller.text == v3EngineLabBulletListSecondDisplay &&
            find
                .text('Parser-certified marker-free bullet-list item active')
                .evaluate()
                .isNotEmpty,
        description: 'second bullet-list item handoff',
      );

      final revisionBeforeEdit = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      final presentationGenerationBeforeEdit = int.parse(
        _metricValue(tester, 'v3-engine-lab-inline-presentation-generation'),
      );
      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaReplacement(
          oldText: v3EngineLabBulletListSecondDisplay,
          replacementText: 'É',
          replacedRange: TextRange(start: 0, end: 1),
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      final editedDisplay = v3EngineLabBulletListSecondDisplay.replaceFirst(
        'E',
        'É',
      );
      expect(editable.controller.text, editedDisplay);
      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBeforeEdit + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            int.parse(
                  _metricValue(
                    tester,
                    'v3-engine-lab-inline-presentation-generation',
                  ),
                ) >
                presentationGenerationBeforeEdit &&
            find
                .text('Parser-certified marker-free bullet-list item active')
                .evaluate()
                .isNotEmpty,
        description: 'Unicode bullet-list edit to become exact-current',
      );
      final editedSecondSource = v3EngineLabBulletListSecondSource.replaceFirst(
        'Edit',
        'Édit',
      );
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-bullet-list-source'),
              ),
            )
            .data,
        '$v3EngineLabBulletListFirstSource'
        '$editedSecondSource'
        '$v3EngineLabBulletListTerminalSource',
      );

      final selectEmpty = find.byKey(
        const Key('v3-engine-lab-select-empty-list-item'),
      );
      await tester.ensureVisible(selectEmpty);
      await tester.pump();
      await tester.tap(selectEmpty);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            editable.controller.text == v3EngineLabBulletListTerminalDisplay &&
            find
                .text('Parser-certified marker-free bullet-list item active')
                .evaluate()
                .isNotEmpty,
        description: 'terminal empty bullet-list item handoff',
      );
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(const Key('v3-engine-lab-exact-source')),
            )
            .data,
        v3EngineLabBulletListTerminalSource,
      );
      final handoffEditableState = tester.state<EditableTextState>(
        editorFinder,
      );
      expect(handoffEditableState, same(editableState));
      final handoffSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      expect(handoffSetClientCalls, hasLength(initialSetClientCalls.length));
      expect(
        (handoffSetClientCalls.last.arguments as List<dynamic>).first,
        clientId,
      );

      final revisionBeforeExit = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: v3EngineLabBulletListTerminalDisplay,
          textInserted: '\n',
          insertionOffset: 0,
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBeforeExit + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            find
                .byKey(const Key('flark-v3-bullet-list-item-gutter'))
                .evaluate()
                .isEmpty,
        description: 'terminal empty item to exit the list',
      );
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-bullet-list-source'),
              ),
            )
            .data,
        '$v3EngineLabBulletListFirstSource$editedSecondSource\r\n  ',
      );
      expect(
        tester.state<EditableTextState>(editorFinder),
        same(editableState),
      );
      final finalSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
      expect(
        (finalSetClientCalls.last.arguments as List<dynamic>).first,
        clientId,
      );
      _expectNoFrameworkException(
        tester,
        'exiting the terminal empty list item',
      );

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    'ordered checkpoint paints exact marker and preserves CRLF on one client',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.orderedList;
      final openList = find.text('Open ${seed.label}');
      await tester.ensureVisible(openList);
      await tester.pump();
      await tester.tap(openList);
      await tester.pump();

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      await _pumpUntil(
        tester,
        () =>
            find
                .text('Parser-certified marker-free ordered-list item active')
                .evaluate()
                .isNotEmpty &&
            editorFinder.evaluate().any(
              (element) => element.widget is EditableText,
            ) &&
            tester.widget<EditableText>(editorFinder).controller.text ==
                v3EngineLabOrderedListDisplay,
        description: 'selected ordered-list item',
      );

      final editable = tester.widget<EditableText>(editorFinder);
      final editableState = tester.state<EditableTextState>(editorFinder);
      final deltaClient = editableState as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabOrderedListDisplay);
      expect(editable.controller.text, isNot(contains('007)')));
      expect(editorFinder, findsOneWidget);
      expect(
        find.byKey(const Key('flark-v3-ordered-list-item-gutter')),
        findsOneWidget,
      );
      final marker = tester.widget<RichText>(
        find.byKey(const Key('flark-v3-ordered-list-item-marker')),
      );
      expect(marker.text.toPlainText(), '007)');
      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-ordered-list-source'),
              ),
            )
            .data,
        v3EngineLabOrderedListSource,
      );
      expect(
        find.byKey(const Key('v3-engine-lab-ordered-list-scope')),
        findsOneWidget,
      );

      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      final clientId =
          (initialSetClientCalls.last.arguments as List<dynamic>).first as int;
      final revisionBeforeEnter = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );

      deltaClient.updateEditingValueWithDeltas([
        const TextEditingDeltaInsertion(
          oldText: v3EngineLabOrderedListDisplay,
          textInserted: '\r',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBeforeEnter + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            editable.controller.text == 'pha\n' &&
            find.text('008)', findRichText: true).evaluate().isNotEmpty,
        description: 'ordered-list Enter continuation to become exact-current',
      );

      expect(
        tester
            .widget<SelectableText>(
              find.byKey(
                const Key('v3-engine-lab-canonical-ordered-list-source'),
              ),
            )
            .data,
        '007) al\r\n008) pha\r\n9) beta\r\n',
      );
      expect(
        tester.state<EditableTextState>(editorFinder),
        same(editableState),
      );
      final finalSetClientCalls = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      expect(finalSetClientCalls, hasLength(initialSetClientCalls.length));
      expect(
        (finalSetClientCalls.last.arguments as List<dynamic>).first,
        clientId,
      );
      _expectNoFrameworkException(
        tester,
        'continuing the ordered list with CRLF',
      );

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    '4,096-reference seed reuses the marker-free tail editor and converges',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.references4096;
      final openReferenceTail = find.text('Open ${seed.label}');
      await tester.ensureVisible(openReferenceTail);
      await tester.pump();
      await tester.tap(openReferenceTail);
      await tester.pump();
      _expectNoFrameworkException(tester, 'opening 4,096-reference fixture');

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified hidden inline rendering active')
            .evaluate()
            .isNotEmpty,
        timeout: const Duration(seconds: 30),
        description: '4,096-reference certified tail presentation',
      );
      _expectNoFrameworkException(
        tester,
        'adopting 4,096-reference projected tail',
      );

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final editable = tester.widget<EditableText>(editorFinder);
      final deltaClient =
          tester.state<EditableTextState>(editorFinder) as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabEditableTailDisplay);
      expect(
        _metricValue(tester, 'v3-engine-lab-checkpoint-c-fixture'),
        seed.label,
      );
      expect(
        _metricValue(tester, 'v3-engine-lab-cold-open-current'),
        isNot('not observed'),
      );
      await _pumpUntil(
        tester,
        () => _metricValue(
          tester,
          'v3-engine-lab-checkpoint-c-visible-range',
        ).startsWith('exact · 1 block'),
        timeout: const Duration(seconds: 10),
        description: 'Dart-first visible range to become exact',
      );
      expect(
        _metricValue(tester, 'v3-engine-lab-checkpoint-c-visible-work'),
        contains('bounded quant'),
      );
      final visibleQuantaBefore = int.parse(
        _metricValue(
          tester,
          'v3-engine-lab-checkpoint-c-visible-work',
        ).split(' ').first,
      );
      final inlinePresentationBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-inline-presentation-generation'),
      );

      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );
      deltaClient.updateEditingValueWithDeltas([
        TextEditingDeltaInsertion(
          oldText: v3EngineLabEditableTailDisplay,
          textInserted: '!',
          insertionOffset: v3EngineLabEditableTailDisplay.length,
          selection: TextSelection.collapsed(
            offset: v3EngineLabEditableTailDisplay.length + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      _expectNoFrameworkException(tester, 'editing 4,096-reference tail');
      expect(editable.controller.text, '$v3EngineLabEditableTailDisplay!');

      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            _metricValue(tester, 'v3-engine-lab-latest-edit-current') !=
                'not observed' &&
            int.parse(
                  _metricValue(
                    tester,
                    'v3-engine-lab-inline-presentation-generation',
                  ),
                ) >
                inlinePresentationBefore &&
            find
                .text('Parser-certified hidden inline rendering active')
                .evaluate()
                .isNotEmpty &&
            _metricValue(
              tester,
              'v3-engine-lab-checkpoint-c-visible-range',
            ).startsWith('exact · 1 block') &&
            int.parse(
                  _metricValue(
                    tester,
                    'v3-engine-lab-checkpoint-c-visible-work',
                  ).split(' ').first,
                ) >
                visibleQuantaBefore,
        timeout: const Duration(seconds: 30),
        description: '4,096-reference tail edit to become exact-current',
      );
      _expectNoFrameworkException(
        tester,
        'certifying 4,096-reference tail edit',
      );
      expect(editable.controller.text, isNot(contains('**')));
      expect(editable.controller.text, isNot(contains('_')));
      expect(editable.controller.text, isNot(contains('`')));

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 60)),
  );

  testWidgets(
    '100,000-reference release Worker keeps the live tail bounded and exact',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final seed = V3EngineLabSeed.references100000;
      final openReferenceTail = find.text('Open ${seed.label}');
      await tester.ensureVisible(openReferenceTail);
      await tester.pump();
      await tester.tap(openReferenceTail);
      await tester.pump();
      _expectNoFrameworkException(tester, 'opening 100,000-reference fixture');

      await _pumpUntil(
        tester,
        () => find
            .text('Parser-certified hidden inline rendering active')
            .evaluate()
            .isNotEmpty,
        timeout: const Duration(seconds: 120),
        description: '100,000-reference certified tail presentation',
      );
      _expectNoFrameworkException(
        tester,
        'adopting 100,000-reference projected tail',
      );

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final editable = tester.widget<EditableText>(editorFinder);
      final editableState = tester.state<EditableTextState>(editorFinder);
      final deltaClient = editableState as DeltaTextInputClient;
      expect(editable.controller.text, v3EngineLabEditableTailDisplay);
      expect(editable.controller.text, isNot(contains('**')));
      expect(editable.controller.text, isNot(contains('_emphasis_')));
      expect(editable.controller.text, isNot(contains('`code`')));
      expect(editable.controller.text, isNot(contains('~~strike~~')));
      expect(
        _metricValue(tester, 'v3-engine-lab-checkpoint-c-fixture'),
        seed.label,
      );
      expect(
        _metricValue(tester, 'v3-engine-lab-cold-open-current'),
        isNot('not observed'),
      );
      await _pumpUntil(
        tester,
        () => _metricValue(
          tester,
          'v3-engine-lab-checkpoint-c-visible-range',
        ).startsWith('exact · 1 block'),
        timeout: const Duration(seconds: 30),
        description: '100,000-reference visible tail to become exact',
      );
      final visibleQuantaBefore = int.parse(
        _metricValue(
          tester,
          'v3-engine-lab-checkpoint-c-visible-work',
        ).split(' ').first,
      );

      editableState.requestKeyboard();
      await tester.pump();
      final clientsBefore = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      final clientId =
          (clientsBefore.last.arguments as List<dynamic>).first as int;
      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );

      const rapidInsertion = 'instant';
      var currentDisplay = v3EngineLabEditableTailDisplay;
      var maximumForegroundEdit = Duration.zero;
      var totalForegroundEditMicroseconds = 0;
      for (final character in rapidInsertion.split('')) {
        final foregroundClock = Stopwatch()..start();
        deltaClient.updateEditingValueWithDeltas([
          TextEditingDeltaInsertion(
            oldText: currentDisplay,
            textInserted: character,
            insertionOffset: currentDisplay.length,
            selection: TextSelection.collapsed(
              offset: currentDisplay.length + character.length,
            ),
            composing: TextRange.empty,
          ),
        ]);
        foregroundClock.stop();
        currentDisplay += character;
        totalForegroundEditMicroseconds += foregroundClock.elapsedMicroseconds;
        if (foregroundClock.elapsed > maximumForegroundEdit) {
          maximumForegroundEdit = foregroundClock.elapsed;
        }
      }
      await tester.pump();
      expect(
        editable.controller.text,
        '$v3EngineLabEditableTailDisplay$rapidInsertion',
      );
      _expectNoFrameworkException(tester, 'editing 100,000-reference tail');
      expect(
        maximumForegroundEdit,
        lessThan(const Duration(microseconds: 16667)),
        reason:
            'A projected local edit must return inside one 60 Hz frame on the '
            'Chrome regression host even with 100,000 leading definitions.',
      );
      expect(
        Duration(microseconds: totalForegroundEditMicroseconds),
        lessThan(const Duration(milliseconds: 50)),
        reason:
            'The complete zero-cadence burst must remain bounded on the '
            'Flutter caller isolate.',
      );

      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + rapidInsertion.length}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            _metricValue(tester, 'v3-engine-lab-latest-edit-current') !=
                'not observed' &&
            _metricValue(
              tester,
              'v3-engine-lab-checkpoint-c-visible-range',
            ).startsWith('exact · 1 block') &&
            int.parse(
                  _metricValue(
                    tester,
                    'v3-engine-lab-checkpoint-c-visible-work',
                  ).split(' ').first,
                ) >
                visibleQuantaBefore,
        timeout: const Duration(seconds: 90),
        description: '100,000-reference tail edit to become exact-current',
      );
      _expectNoFrameworkException(
        tester,
        'certifying 100,000-reference tail edit',
      );

      expect(
        tester
            .widget<SelectableText>(
              find.byKey(const Key('v3-engine-lab-exact-source')),
            )
            .data,
        '$v3EngineLabEditableTailSource$rapidInsertion',
      );
      expect(
        tester.state<EditableTextState>(editorFinder),
        same(editableState),
      );
      final clientsAfter = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList(growable: false);
      expect(clientsAfter, hasLength(clientsBefore.length));
      expect((clientsAfter.last.arguments as List<dynamic>).first, clientId);
      // ignore: avoid_print
      print(
        'flark_v3_web_100k_reference_rapid_tail '
        'edits=${rapidInsertion.length} '
        'apply_max_us=${maximumForegroundEdit.inMicroseconds} '
        'apply_total_us=$totalForegroundEditMicroseconds',
      );

      await _closeLabRuntime(tester);
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(minutes: 4)),
  );

  testWidgets(
    'large seed accepts a normal insertion and converges',
    (tester) async {
      await tester.pumpWidget(
        V3EngineLabApp(
          openOnStart: false,
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
        ),
      );
      final openOneMiB = find.text('Open ${V3EngineLabSeed.oneMebibyte.label}');
      await tester.ensureVisible(openOneMiB);
      await tester.pump();
      await tester.tap(openOneMiB);
      await tester.pump();

      await _pumpUntil(
        tester,
        () {
          final editor = tester.widget<TextField>(
            find.byKey(const Key('v3-engine-lab-editor')),
          );
          return editor.enabled == true &&
              _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes';
        },
        description: '1 MiB runtime to become editable and structure-current',
      );

      final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
      final controller = tester.widget<TextField>(editorFinder).controller!;
      expect(controller.text.length, v3EngineLabLoadedNeighborhoodUtf16);
      expect(controller.text.length, lessThan(v3EngineLabMaximumActiveUtf16));
      final sourceBefore = _metricValue(tester, 'v3-engine-lab-source-length');
      final revisionBefore = int.parse(
        _metricValue(tester, 'v3-engine-lab-source-revision'),
      );

      await tester.enterText(editorFinder, '${controller.text}x');
      await tester.pump();
      expect(controller.text.length, v3EngineLabLoadedNeighborhoodUtf16 + 1);

      await _pumpUntil(
        tester,
        () =>
            _metricValue(tester, 'v3-engine-lab-source-revision') ==
                '${revisionBefore + 1}' &&
            _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
            _metricValue(tester, 'v3-engine-lab-latest-edit-current') !=
                'not observed',
        description: 'inserted revision to become structure-current',
      );
      expect(
        _metricValue(tester, 'v3-engine-lab-source-length'),
        isNot(sourceBefore),
      );

      final closeButton = find.text('Close and await receipt');
      await tester.ensureVisible(closeButton);
      await tester.pump();
      await tester.tap(closeButton);
      await tester.pump();
      await _pumpUntil(
        tester,
        () => find
            .textContaining('Close completed with the endpoint slot released')
            .evaluate()
            .isNotEmpty,
        description: 'runtime close receipt',
      );
      await tester.pumpWidget(const SizedBox.shrink());
    },
    timeout: const Timeout(Duration(seconds: 45)),
  );
}

Future<void> _runSequentialKeyTrial(
  WidgetTester tester, {
  Duration cadence = const Duration(milliseconds: 80),
}) async {
  await tester.pumpWidget(
    V3EngineLabApp(
      openOnStart: false,
      webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
    ),
  );
  final openSmall = find.text('Open ${V3EngineLabSeed.small.label}');
  await tester.ensureVisible(openSmall);
  await tester.pump();
  await tester.tap(openSmall);
  await tester.pump();

  await _pumpUntil(
    tester,
    () => find
        .text('Parser-certified hidden inline rendering active')
        .evaluate()
        .isNotEmpty,
    description: 'small-seed certified inline presentation',
  );

  final editorFinder = find.byKey(const Key('v3-engine-lab-editor'));
  final editableState = tester.state<EditableTextState>(editorFinder);
  final deltaClient = editableState as DeltaTextInputClient;
  final controller = tester.widget<EditableText>(editorFinder).controller;
  editableState.requestKeyboard();
  await tester.pump();

  final revisionBefore = int.parse(
    _metricValue(tester, 'v3-engine-lab-source-revision'),
  );
  const keyEdits = <String?>[
    'a',
    null,
    ' ',
    '*',
    '*',
    'i',
    'n',
    's',
    't',
    'a',
    'n',
    't',
    '*',
    '*',
  ];
  for (var index = 0; index < keyEdits.length; index++) {
    final oldText = controller.text;
    final inserted = keyEdits[index];
    deltaClient.updateEditingValueWithDeltas([
      if (inserted == null)
        TextEditingDeltaDeletion(
          oldText: oldText,
          deletedRange: TextRange(
            start: oldText.length - 1,
            end: oldText.length,
          ),
          selection: TextSelection.collapsed(offset: oldText.length - 1),
          composing: TextRange.empty,
        )
      else
        TextEditingDeltaInsertion(
          oldText: oldText,
          textInserted: inserted,
          insertionOffset: oldText.length,
          selection: TextSelection.collapsed(offset: oldText.length + 1),
          composing: TextRange.empty,
        ),
    ]);
    if (cadence > Duration.zero) {
      await tester.runAsync(() => Future<void>.delayed(cadence));
      await tester.pump(cadence);
    }
    _expectNoFrameworkException(
      tester,
      'accepting sequential key delta ${index + 1}',
    );
  }
  await tester.pump();

  expect(keyEdits, hasLength(14));
  expect(
    _metricValue(tester, 'v3-engine-lab-source-revision'),
    '${revisionBefore + keyEdits.length}',
  );
  await _pumpUntil(
    tester,
    () =>
        _metricValue(tester, 'v3-engine-lab-source-revision') ==
            '${revisionBefore + keyEdits.length}' &&
        _metricValue(tester, 'v3-engine-lab-structure-current') == 'yes' &&
        find
            .text('Parser-certified hidden inline rendering active')
            .evaluate()
            .isNotEmpty &&
        controller.text.endsWith(' instant'),
    timeout: const Duration(seconds: 30),
    description:
        '14 sequential platform deltas to become exact-current and marker-free',
  );
  _expectNoFrameworkException(
    tester,
    'certifying sequential platform key deltas',
  );
  final exactSource = tester.widget<SelectableText>(
    find.byKey(const Key('v3-engine-lab-exact-source')),
  );
  expect(exactSource.data, endsWith(' **instant**'));
  expect(controller.text, isNot(contains('**')));
  expect(controller.text, isNot(contains('_')));
  expect(controller.text, isNot(contains('`')));

  await _closeLabRuntime(tester);
  await tester.pumpWidget(const SizedBox.shrink());
}

Future<void> _closeLabRuntime(WidgetTester tester) async {
  final closeButton = find.text('Close and await receipt');
  await tester.ensureVisible(closeButton);
  await tester.pump();
  await tester.tap(closeButton);
  await tester.pump();
  await _pumpUntil(
    tester,
    () => find
        .textContaining('Close completed with the endpoint slot released')
        .evaluate()
        .isNotEmpty,
    description: 'runtime close receipt',
  );
}

void _expectNoFrameworkException(WidgetTester tester, String operation) {
  final exception = tester.takeException();
  expect(exception, isNull, reason: operation);
}

String _metricValue(WidgetTester tester, String key) {
  final texts = tester
      .widgetList<Text>(
        find.descendant(of: find.byKey(Key(key)), matching: find.byType(Text)),
      )
      .toList(growable: false);
  expect(texts, hasLength(2));
  return texts.last.data!;
}

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 15),
  required String description,
}) async {
  final watch = Stopwatch()..start();
  while (!condition()) {
    if (watch.elapsed >= timeout) {
      final visibleText = tester
          .widgetList<Text>(find.byType(Text))
          .map((widget) => widget.data)
          .whereType<String>()
          .where((value) => value.isNotEmpty)
          .join(' | ');
      debugPrint(
        'V3_ENGINE_LAB_TIMEOUT: $description; visible text: $visibleText',
      );
      fail(
        'Timed out waiting for $description within $timeout. '
        'Visible text: $visibleText',
      );
    }
    await tester.runAsync(() async {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    });
    await tester.pump(const Duration(milliseconds: 20));
  }
}

import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

// The acceptance fixture intentionally imports the app's real default source
// so a copied test string cannot drift from the dogfood surface.
// ignore: avoid_relative_lib_imports
import '../example/lib/dogfood_documents.dart';
import 'support/live_editor_transition_probe.dart';

final _productTourSource = buildDogfoodDocument(
  DogfoodDocumentPreset.productTour,
);
const _productTourParagraph =
    '''This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.''';
final _productTourParagraphStart = _productTourSource.indexOf(
  _productTourParagraph,
);
final _productTourParagraphEnd =
    _productTourParagraphStart + _productTourParagraph.length;
const _dogfoodTerminalSuccessor = ' Testing is somewhat useful but like.';
const _guardedProsePunctuation = [
  (name: 'period', scalar: '.'),
  (name: 'comma', scalar: ','),
  (name: 'semicolon', scalar: ';'),
  (name: 'colon', scalar: ':'),
  (name: 'exclamation', scalar: '!'),
  (name: 'question mark', scalar: '?'),
  (name: 'apostrophe', scalar: "'"),
  (name: 'double quote', scalar: '"'),
  (name: 'open parenthesis', scalar: '('),
  (name: 'close parenthesis', scalar: ')'),
  (name: 'hyphen', scalar: '-'),
  (name: 'en dash', scalar: '–'),
  (name: 'em dash', scalar: '—'),
];
final _scenarios = <_TypingScenario>[
  _TypingScenario(
    name: 'product-tour paragraph prefix beside Strong',
    initial: _productTourSource.replaceRange(
      _productTourParagraphStart,
      _productTourParagraphStart,
      '¦',
    ),
    inserted: 'keep what',
    renderedBefore: '',
    renderedAfter: _productTourParagraph.replaceAll('**', ''),
    finalMarked: _productTourSource.replaceRange(
      _productTourParagraphStart,
      _productTourParagraphStart,
      'keep what¦',
    ),
    forbiddenMarkers: ['# ', '**'],
    staticStyledRuns: [
      (text: 'Rust → Dart → Flutter', style: FlarkSurfaceInlineStyle.strong),
    ],
    shell: _paragraphShell,
    unpumpedBurst: true,
  ),
  _TypingScenario(
    name: 'actual dogfood terminal typing after punctuation',
    initial: _productTourSource.replaceRange(
      _productTourParagraphEnd,
      _productTourParagraphEnd,
      '¦',
    ),
    inserted: _dogfoodTerminalSuccessor,
    renderedBefore: _productTourParagraph.replaceAll('**', ''),
    renderedAfter: '',
    finalMarked: _productTourSource.replaceRange(
      _productTourParagraphEnd,
      _productTourParagraphEnd,
      '$_dogfoodTerminalSuccessor¦',
    ),
    forbiddenMarkers: ['# ', '**'],
    staticStyledRuns: [
      (text: 'Rust → Dart → Flutter', style: FlarkSurfaceInlineStyle.strong),
    ],
    shell: _paragraphShell,
    unpumpedBurst: true,
  ),
  _TypingScenario(
    name: 'terminal typing at document EOF without a final newline',
    initial: 'Before **bold**\nplain terminal.¦',
    inserted: ' Testing.',
    renderedBefore: 'Before bold\nplain terminal.',
    renderedAfter: '',
    finalMarked: 'Before **bold**\nplain terminal. Testing.¦',
    forbiddenMarkers: ['**'],
    staticStyledRuns: [(text: 'bold', style: FlarkSurfaceInlineStyle.strong)],
    shell: _paragraphShell,
    unpumpedBurst: true,
  ),
  _TypingScenario(
    name: 'plain literal between independent Strong and Emphasis facts',
    initial: '**left** mi¦ddle _right_\n',
    inserted: 'ke',
    renderedBefore: 'left mi',
    renderedAfter: 'ddle right',
    finalMarked: '**left** mike¦ddle _right_\n',
    forbiddenMarkers: ['**', '_right_'],
    staticStyledRuns: [
      (text: 'left', style: FlarkSurfaceInlineStyle.strong),
      (text: 'right', style: FlarkSurfaceInlineStyle.emphasis),
    ],
    shell: _paragraphShell,
    unpumpedBurst: true,
  ),
  for (final punctuation in _guardedProsePunctuation)
    _TypingScenario(
      name: 'guarded prose ${punctuation.name} beside Strong',
      initial: 'Alpha¦Beta and **bold**.\n',
      inserted: punctuation.scalar,
      renderedBefore: 'Alpha',
      renderedAfter: 'Beta and bold.',
      finalMarked: 'Alpha${punctuation.scalar}¦Beta and **bold**.\n',
      forbiddenMarkers: ['**'],
      staticStyledRuns: [(text: 'bold', style: FlarkSurfaceInlineStyle.strong)],
      shell: _paragraphShell,
    ),
  _TypingScenario(
    name: 'syntax asterisk island beside Emphasis',
    initial: 'ab¦cd _right_\n',
    inserted: '*',
    renderedBefore: 'ab',
    renderedAfter: 'cd right',
    finalMarked: 'ab*¦cd _right_\n',
    forbiddenMarkers: ['_right_'],
    staticStyledRuns: [
      (text: 'right', style: FlarkSurfaceInlineStyle.emphasis),
    ],
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'syntax underscore island beside Strong',
    initial: 'ab¦cd **right**\n',
    inserted: '_',
    renderedBefore: 'ab',
    renderedAfter: 'cd right',
    finalMarked: 'ab_¦cd **right**\n',
    forbiddenMarkers: ['**'],
    staticStyledRuns: [(text: 'right', style: FlarkSurfaceInlineStyle.strong)],
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'syntax tilde island beside Strong',
    initial: 'ab¦cd **right**\n',
    inserted: '~',
    renderedBefore: 'ab',
    renderedAfter: 'cd right',
    finalMarked: 'ab~¦cd **right**\n',
    forbiddenMarkers: ['**'],
    staticStyledRuns: [(text: 'right', style: FlarkSurfaceInlineStyle.strong)],
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'syntax backtick island beside Emphasis',
    initial: 'ab¦cd _right_\n',
    inserted: '`',
    renderedBefore: 'ab',
    renderedAfter: 'cd right',
    finalMarked: 'ab`¦cd _right_\n',
    forbiddenMarkers: ['_right_'],
    staticStyledRuns: [
      (text: 'right', style: FlarkSurfaceInlineStyle.emphasis),
    ],
    shell: _paragraphShell,
  ),
  for (final bracket in const [
    (name: 'open bracket', scalar: '['),
    (name: 'close bracket', scalar: ']'),
  ])
    _TypingScenario(
      name: 'syntax ${bracket.name} island beside Emphasis',
      initial: 'ab¦cd _right_\n',
      inserted: bracket.scalar,
      renderedBefore: 'ab',
      renderedAfter: 'cd right',
      finalMarked: 'ab${bracket.scalar}¦cd _right_\n',
      forbiddenMarkers: ['_right_'],
      staticStyledRuns: [
        (text: 'right', style: FlarkSurfaceInlineStyle.emphasis),
      ],
      shell: _paragraphShell,
    ),
  _TypingScenario(
    name: 'typing inside a certified Strong word',
    initial: 'Before **bo¦ld** after.\n',
    inserted: 'ke',
    renderedBefore: 'Before bo',
    renderedAfter: 'ld after.',
    finalMarked: 'Before **boke¦ld** after.\n',
    forbiddenMarkers: ['**'],
    dynamicStyledBefore: 'bo',
    dynamicStyledAfter: 'ld',
    dynamicStyle: FlarkSurfaceInlineStyle.strong,
    insertedStyles: {FlarkSurfaceInlineStyle.strong},
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'typing inside the second word leaf of a Strong fact',
    initial: 'Before **bold te¦xt** after.\n',
    inserted: 'ke',
    renderedBefore: 'Before bold te',
    renderedAfter: 'xt after.',
    finalMarked: 'Before **bold teke¦xt** after.\n',
    forbiddenMarkers: ['**'],
    dynamicStyledBefore: 'bold te',
    dynamicStyledAfter: 'xt',
    dynamicStyle: FlarkSurfaceInlineStyle.strong,
    insertedStyles: {FlarkSurfaceInlineStyle.strong},
    shell: _paragraphShell,
    unpumpedBurst: true,
  ),
  _TypingScenario(
    name: 'typing inside a certified Emphasis word',
    initial: 'Before _ri¦ght_ after.\n',
    inserted: 'ke',
    renderedBefore: 'Before ri',
    renderedAfter: 'ght after.',
    finalMarked: 'Before _rike¦ght_ after.\n',
    forbiddenMarkers: ['_right_'],
    dynamicStyledBefore: 'ri',
    dynamicStyledAfter: 'ght',
    dynamicStyle: FlarkSurfaceInlineStyle.emphasis,
    insertedStyles: {FlarkSurfaceInlineStyle.emphasis},
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'typing inside a certified Strikethrough word',
    initial: 'Before ~~ri¦ght~~ after.\n',
    inserted: 'ke',
    renderedBefore: 'Before ri',
    renderedAfter: 'ght after.',
    finalMarked: 'Before ~~rike¦ght~~ after.\n',
    forbiddenMarkers: ['~~'],
    dynamicStyledBefore: 'ri',
    dynamicStyledAfter: 'ght',
    dynamicStyle: FlarkSurfaceInlineStyle.strikethrough,
    insertedStyles: {FlarkSurfaceInlineStyle.strikethrough},
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'typing inside a certified inline-code word',
    initial: 'Before `ri¦ght` after.\n',
    inserted: 'ke',
    renderedBefore: 'Before ri',
    renderedAfter: 'ght after.',
    finalMarked: 'Before `rike¦ght` after.\n',
    forbiddenMarkers: ['`'],
    dynamicStyledBefore: 'ri',
    dynamicStyledAfter: 'ght',
    dynamicStyle: FlarkSurfaceInlineStyle.code,
    insertedStyles: {FlarkSurfaceInlineStyle.code},
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'typing inside a certified direct-link label',
    initial: 'Before [ri¦ght](https://example.com) after.\n',
    inserted: 'ke',
    renderedBefore: 'Before ri',
    renderedAfter: 'ght after.',
    finalMarked: 'Before [rike¦ght](https://example.com) after.\n',
    forbiddenMarkers: ['[', '](', 'https://example.com'],
    dynamicStyledBefore: 'ri',
    dynamicStyledAfter: 'ght',
    dynamicStyle: FlarkSurfaceInlineStyle.link,
    insertedStyles: {FlarkSurfaceInlineStyle.link},
    shell: _paragraphShell,
  ),
  _TypingScenario(
    name: 'list item shell beside Strong',
    initial: '- fi¦rst **bold**\n',
    inserted: 'ke',
    renderedBefore: 'fi',
    renderedAfter: 'rst bold',
    finalMarked: '- fike¦rst **bold**\n',
    // The selected list presentation intentionally keeps the authored marker;
    // non-neutral block identity plus the Strong run distinguish it from raw
    // whole-row fallback.
    forbiddenMarkers: ['**'],
    staticStyledRuns: [(text: 'bold', style: FlarkSurfaceInlineStyle.strong)],
    shell: _listShell,
  ),
  _TypingScenario(
    name: 'block quote shell beside Strong',
    initial: '> fi¦rst **bold**\n',
    inserted: 'ke',
    renderedBefore: 'fi',
    renderedAfter: 'rst bold',
    finalMarked: '> fike¦rst **bold**\n',
    forbiddenMarkers: ['> ', '**'],
    staticStyledRuns: [(text: 'bold', style: FlarkSurfaceInlineStyle.strong)],
    shell: _quoteShell,
  ),
  _TypingScenario(
    name: 'plain table cell beside a Strong cell',
    initial: '| f¦oo | **bold** |\n| --- | --- |\n',
    inserted: 'x',
    renderedBefore: 'f',
    renderedAfter: 'oo │ bold',
    finalMarked: '| fx¦oo | **bold** |\n| --- | --- |\n',
    forbiddenMarkers: ['|', '---', '**'],
    staticStyledRuns: [(text: 'bold', style: FlarkSurfaceInlineStyle.strong)],
    shell: _tableShell,
  ),
  _TypingScenario(
    name: 'fenced Dart code body',
    initial: "```dart\nfinal value = 'a¦';\n```\n",
    inserted: 'x',
    renderedBefore: "final value = 'a",
    renderedAfter: "';\n",
    finalMarked: "```dart\nfinal value = 'ax¦';\n```\n",
    forbiddenMarkers: ['```'],
    shell: _fencedCodeShell,
  ),
];

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  for (final cadence in const [
    (name: 'zero-cadence', delay: Duration.zero),
    (name: 'human-cadence', delay: Duration(milliseconds: 80)),
  ]) {
    for (final scenario in _scenarios) {
      testWidgets(
        '${cadence.name} ${scenario.name} keeps the best rendered result',
        (tester) async {
          final probe = (await tester.runAsync(
            () => LiveEditorTransitionProbe.open(
              scenario.initial,
              libraryPath: libraryPath!,
            ),
          ))!;
          final mounted = await MountedTransitionRecorder.mount(tester, probe);

          try {
            var insertedSoFar = '';
            var expectedGeneration = probe.controller.sourceGeneration;
            var expectedCaret = MarkedSource.parse(scenario.initial).caret;
            for (final rune in scenario.inserted.runes) {
              final character = String.fromCharCode(rune);
              final paintStart = mounted.paints.length;
              insertedSoFar += character;
              final expectedText =
                  '${scenario.renderedBefore}$insertedSoFar${scenario.renderedAfter}';
              expectedGeneration += 1;
              expectedCaret += character.length;

              await mounted.typeText(character);
              await mounted.pumpImmediate();
              if (cadence.delay != Duration.zero) {
                var remaining = cadence.delay.inMilliseconds;
                while (remaining > 0) {
                  final slice = remaining < 8 ? remaining : 8;
                  await tester.runAsync(
                    () => Future<void>.delayed(Duration(milliseconds: slice)),
                  );
                  await mounted.pumpImmediate();
                  remaining -= slice;
                }
              }

              final editPaints = mounted.paints.skip(paintStart).toList();
              expect(
                editPaints,
                isNotEmpty,
                reason:
                    '${scenario.name}: every accepted rune must produce a paint',
              );
              _expectScenarioPaints(
                editPaints,
                scenario: scenario,
                insertedSoFar: insertedSoFar,
                expectedText: expectedText,
                expectedGeneration: expectedGeneration,
                expectedCaret: expectedCaret,
              );
            }

            final settleStart = mounted.paints.length;
            await mounted.pumpPresentationSettled();
            _expectScenarioPaints(
              mounted.paints.skip(settleStart).toList(),
              scenario: scenario,
              insertedSoFar: insertedSoFar,
              expectedText:
                  '${scenario.renderedBefore}$insertedSoFar${scenario.renderedAfter}',
              expectedGeneration: expectedGeneration,
              expectedCaret: expectedCaret,
              allowEmpty: true,
            );
            await tester.runAsync(
              () => probe.expectSourceAndCaret(scenario.finalMarked),
            );
            await tester.runAsync(probe.expectHealthy);
            await tester.runAsync(probe.expectConvergesWithCleanRebuild);
          } finally {
            await mounted.close();
            await tester.runAsync(probe.close);
          }
        },
        skip: libraryPath == null,
        timeout: const Timeout(Duration(minutes: 2)),
      );
    }

    if (cadence.delay == Duration.zero) {
      for (final scenario in _scenarios.where(
        (scenario) => scenario.unpumpedBurst,
      )) {
        testWidgets(
          'unpumped burst ${scenario.name} paints only the final rendered generation',
          (tester) async {
            final probe = (await tester.runAsync(
              () => LiveEditorTransitionProbe.open(
                scenario.initial,
                libraryPath: libraryPath!,
              ),
            ))!;
            final mounted = await MountedTransitionRecorder.mount(
              tester,
              probe,
            );
            try {
              final paintStart = mounted.paints.length;
              final initialGeneration = probe.controller.sourceGeneration;
              final initialCaret = MarkedSource.parse(scenario.initial).caret;
              await mounted.typeText(scenario.inserted);
              expect(
                mounted.paints.skip(paintStart),
                isEmpty,
                reason: 'no synthetic pump may split the burst into frames',
              );
              await mounted.pumpImmediate();
              final expectedGeneration =
                  initialGeneration + scenario.inserted.runes.length;
              final expectedCaret = initialCaret + scenario.inserted.length;
              final expectedText =
                  '${scenario.renderedBefore}${scenario.inserted}${scenario.renderedAfter}';
              _expectScenarioPaints(
                mounted.paints.skip(paintStart).toList(),
                scenario: scenario,
                insertedSoFar: scenario.inserted,
                expectedText: expectedText,
                expectedGeneration: expectedGeneration,
                expectedCaret: expectedCaret,
              );
              final settleStart = mounted.paints.length;
              await mounted.pumpPresentationSettled();
              _expectScenarioPaints(
                mounted.paints.skip(settleStart).toList(),
                scenario: scenario,
                insertedSoFar: scenario.inserted,
                expectedText: expectedText,
                expectedGeneration: expectedGeneration,
                expectedCaret: expectedCaret,
                allowEmpty: true,
              );
              await tester.runAsync(
                () => probe.expectSourceAndCaret(scenario.finalMarked),
              );
              await tester.runAsync(probe.expectHealthy);
            } finally {
              await mounted.close();
              await tester.runAsync(probe.close);
            }
          },
          skip: libraryPath == null,
          timeout: const Timeout(Duration(minutes: 2)),
        );
      }
    }

    testWidgets(
      '${cadence.name} Backspace in the dogfood literal keeps the best rendered result',
      (tester) async {
        const initial = '''# Flark dogfood

This¦ is the real **Rust → Dart → Flutter** editor path.
''';
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            initial,
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          final paintStart = mounted.paints.length;
          final expectedGeneration = probe.controller.sourceGeneration + 1;
          final expectedCaret = MarkedSource.parse(initial).caret - 1;
          await mounted.pressBackspace();
          await mounted.pumpImmediate();
          await _pumpCadence(tester, mounted, cadence.delay);
          _expectDogfoodPaints(
            mounted.paints.skip(paintStart).toList(),
            expectedText: 'Thi is the real Rust → Dart → Flutter editor path.',
            expectedVisibleSource: '''# Flark dogfood

Thi is the real **Rust → Dart → Flutter** editor path.
''',
            expectedGeneration: expectedGeneration,
            expectedCaret: expectedCaret,
            operation: '${cadence.name} Backspace',
          );
          final settleStart = mounted.paints.length;
          await mounted.pumpPresentationSettled();
          _expectDogfoodPaints(
            mounted.paints.skip(settleStart).toList(),
            expectedText: 'Thi is the real Rust → Dart → Flutter editor path.',
            expectedVisibleSource: '''# Flark dogfood

Thi is the real **Rust → Dart → Flutter** editor path.
''',
            expectedGeneration: expectedGeneration,
            expectedCaret: expectedCaret,
            operation: '${cadence.name} Backspace settle',
            allowEmpty: true,
          );
          await tester.runAsync(
            () => probe.expectSourceAndCaret('''# Flark dogfood

Thi¦ is the real **Rust → Dart → Flutter** editor path.
'''),
          );
          await tester.runAsync(probe.expectHealthy);
          await tester.runAsync(probe.expectConvergesWithCleanRebuild);
        } finally {
          await mounted.close();
          await tester.runAsync(probe.close);
        }
      },
      skip: libraryPath == null,
      timeout: const Timeout(Duration(minutes: 2)),
    );

    testWidgets(
      '${cadence.name} forward Delete in the dogfood literal keeps the best rendered result',
      (tester) async {
        const initial = '''# Flark dogfood

This ¦is the real **Rust → Dart → Flutter** editor path.
''';
        final initialCaret = MarkedSource.parse(initial).caret;
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            initial,
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          final paintStart = mounted.paints.length;
          final expectedGeneration = probe.controller.sourceGeneration + 1;
          await mounted.pressDelete();
          await mounted.pumpImmediate();
          await _pumpCadence(tester, mounted, cadence.delay);
          _expectDogfoodPaints(
            mounted.paints.skip(paintStart).toList(),
            expectedText: 'This s the real Rust → Dart → Flutter editor path.',
            expectedVisibleSource: '''# Flark dogfood

This s the real **Rust → Dart → Flutter** editor path.
''',
            expectedGeneration: expectedGeneration,
            expectedCaret: initialCaret,
            operation: '${cadence.name} forward Delete',
          );
          final settleStart = mounted.paints.length;
          await mounted.pumpPresentationSettled();
          _expectDogfoodPaints(
            mounted.paints.skip(settleStart).toList(),
            expectedText: 'This s the real Rust → Dart → Flutter editor path.',
            expectedVisibleSource: '''# Flark dogfood

This s the real **Rust → Dart → Flutter** editor path.
''',
            expectedGeneration: expectedGeneration,
            expectedCaret: initialCaret,
            operation: '${cadence.name} forward Delete settle',
            allowEmpty: true,
          );
          await tester.runAsync(
            () => probe.expectSourceAndCaret('''# Flark dogfood

This ¦s the real **Rust → Dart → Flutter** editor path.
'''),
          );
          await tester.runAsync(probe.expectHealthy);
          await tester.runAsync(probe.expectConvergesWithCleanRebuild);
        } finally {
          await mounted.close();
          await tester.runAsync(probe.close);
        }
      },
      skip: libraryPath == null,
      timeout: const Timeout(Duration(minutes: 2)),
    );

    testWidgets(
      '${cadence.name} selection replacement in the dogfood literal keeps the best rendered result',
      (tester) async {
        const initial = '''# Flark dogfood

This is¦ the real **Rust → Dart → Flutter** editor path.
''';
        final marked = MarkedSource.parse(initial);
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            initial,
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          final selectionStart = marked.caret - 2;
          await mounted.selectRange(selectionStart, marked.caret);
          var inserted = '';
          var expectedGeneration = probe.controller.sourceGeneration;
          for (final rune in 'was'.runes) {
            final character = String.fromCharCode(rune);
            final paintStart = mounted.paints.length;
            inserted += character;
            expectedGeneration += 1;
            await mounted.typeText(character);
            await mounted.pumpImmediate();
            await _pumpCadence(tester, mounted, cadence.delay);
            _expectDogfoodPaints(
              mounted.paints.skip(paintStart).toList(),
              expectedText:
                  'This $inserted the real Rust → Dart → Flutter editor path.',
              expectedVisibleSource:
                  '''# Flark dogfood

This $inserted the real **Rust → Dart → Flutter** editor path.
''',
              expectedGeneration: expectedGeneration,
              expectedCaret: selectionStart + inserted.length,
              operation: '${cadence.name} replacement $inserted',
            );
          }
          final settleStart = mounted.paints.length;
          await mounted.pumpPresentationSettled();
          _expectDogfoodPaints(
            mounted.paints.skip(settleStart).toList(),
            expectedText:
                'This was the real Rust → Dart → Flutter editor path.',
            expectedVisibleSource: '''# Flark dogfood

This was the real **Rust → Dart → Flutter** editor path.
''',
            expectedGeneration: expectedGeneration,
            expectedCaret: selectionStart + 3,
            operation: '${cadence.name} replacement settle',
            allowEmpty: true,
          );
          await tester.runAsync(
            () => probe.expectSourceAndCaret('''# Flark dogfood

This was¦ the real **Rust → Dart → Flutter** editor path.
'''),
          );
          await tester.runAsync(probe.expectHealthy);
          await tester.runAsync(probe.expectConvergesWithCleanRebuild);
        } finally {
          await mounted.close();
          await tester.runAsync(probe.close);
        }
      },
      skip: libraryPath == null,
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }

  testWidgets(
    'paste plus undo and redo preserve the Product Tour rendered row',
    (tester) async {
      const replaced = 'temporarily pending';
      const replacement = 'briefly pending';
      final selectionStart = _productTourSource.indexOf(replaced);
      final selectionEnd = selectionStart + replaced.length;
      final initial = _productTourSource.replaceRange(
        selectionEnd,
        selectionEnd,
        '¦',
      );
      final pastedSource = _productTourSource.replaceRange(
        selectionStart,
        selectionEnd,
        replacement,
      );
      final originalText = _productTourParagraph.replaceAll('**', '');
      final pastedText = originalText.replaceFirst(replaced, replacement);
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open(initial, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.selectRange(selectionStart, selectionEnd);
        final initialGeneration = probe.controller.sourceGeneration;
        final pastedCaret = selectionStart + replacement.length;

        final pastePaintStart = mounted.paints.length;
        await mounted.replaceSelection(replacement);
        await mounted.pumpImmediate();
        _expectDogfoodPaints(
          mounted.paints.skip(pastePaintStart).toList(),
          expectedText: pastedText,
          expectedVisibleSource: pastedSource,
          expectedGeneration: initialGeneration + 1,
          expectedCaret: pastedCaret,
          operation: 'single paste transaction',
        );
        final pasteSettleStart = mounted.paints.length;
        await mounted.pumpPresentationSettled();
        _expectDogfoodPaints(
          mounted.paints.skip(pasteSettleStart).toList(),
          expectedText: pastedText,
          expectedVisibleSource: pastedSource,
          expectedGeneration: initialGeneration + 1,
          expectedCaret: pastedCaret,
          operation: 'single paste transaction settle',
          allowEmpty: true,
        );

        final undoPaintStart = mounted.paints.length;
        await mounted.undo();
        await mounted.pumpImmediate();
        _expectDogfoodPaints(
          mounted.paints.skip(undoPaintStart).toList(),
          expectedText: originalText,
          expectedVisibleSource: _productTourSource,
          expectedGeneration: initialGeneration + 2,
          expectedCaret: selectionEnd,
          expectedBase: selectionStart,
          operation: 'paste undo',
        );

        final redoPaintStart = mounted.paints.length;
        await mounted.redo();
        await mounted.pumpImmediate();
        _expectDogfoodPaints(
          mounted.paints.skip(redoPaintStart).toList(),
          expectedText: pastedText,
          expectedVisibleSource: pastedSource,
          expectedGeneration: initialGeneration + 3,
          expectedCaret: pastedCaret,
          operation: 'paste redo',
        );
        final settleStart = mounted.paints.length;
        await mounted.pumpPresentationSettled();
        _expectDogfoodPaints(
          mounted.paints.skip(settleStart).toList(),
          expectedText: pastedText,
          expectedVisibleSource: pastedSource,
          expectedGeneration: initialGeneration + 3,
          expectedCaret: pastedCaret,
          operation: 'paste redo settle',
          allowEmpty: true,
        );
        await tester.runAsync(
          () => probe.expectSourceAndCaret(
            pastedSource.replaceRange(pastedCaret, pastedCaret, '¦'),
          ),
        );
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'standalone structural Return paints one projected successor caret',
    (tester) async {
      const initial = 'Before **bold**.¦\n';
      const expectedSource = 'Before **bold**.\n\n\n';
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open(initial, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        final paintStart = mounted.paints.length;
        final expectedGeneration = probe.controller.sourceGeneration + 1;
        await mounted.pressReturn();
        await _pumpUntilGeneration(
          tester,
          mounted,
          expectedGeneration,
          paintStart,
        );
        final paints = mounted.paints
            .skip(paintStart)
            .where((paint) => paint.sourceGeneration == expectedGeneration)
            .toList();
        _expectStructuralPaints(
          paints,
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedCaret: 'Before **bold**.\n\n'.length,
          expectedActiveText: '',
          expectedFullText: 'Before bold.\n',
          operation: 'standalone Return',
        );
        final settleStart = mounted.paints.length;
        await mounted.pumpPresentationSettled();
        _expectStructuralPaints(
          mounted.paints.skip(settleStart).toList(),
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedCaret: 'Before **bold**.\n\n'.length,
          expectedActiveText: '',
          expectedFullText: 'Before bold.\n\n',
          operation: 'standalone Return settle',
          allowEmpty: true,
        );
        await tester.runAsync(
          () => probe.expectSourceAndCaret('Before **bold**.\n\n¦\n'),
        );
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'structural Return with a rapid successor never exposes predecessor markers',
    (tester) async {
      const initial = 'Before **bold**.¦\n';
      const expectedSource = 'Before **bold**.\n\nx\n';
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open(initial, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        final paintStart = mounted.paints.length;
        final expectedGeneration = probe.controller.sourceGeneration + 2;
        await mounted.pressReturn();
        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await _pumpUntilGeneration(
          tester,
          mounted,
          expectedGeneration,
          paintStart,
        );
        final paints = mounted.paints
            .skip(paintStart)
            .where((paint) => paint.sourceGeneration == expectedGeneration)
            .toList();
        _expectStructuralPaints(
          paints,
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedCaret: 'Before **bold**.\n\nx'.length,
          expectedActiveText: 'x',
          expectedFullText: 'Before bold.\nx',
          operation: 'Return successor',
        );
        final settleStart = mounted.paints.length;
        await mounted.pumpPresentationSettled();
        _expectStructuralPaints(
          mounted.paints.skip(settleStart).toList(),
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedCaret: 'Before **bold**.\n\nx'.length,
          expectedActiveText: 'x',
          expectedFullText: 'Before bold.\nx',
          operation: 'Return successor settle',
          allowEmpty: true,
        );
        await tester.runAsync(
          () => probe.expectSourceAndCaret('Before **bold**.\n\nx¦\n'),
        );
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'structural Backspace merge never exposes predecessor markers',
    (tester) async {
      const initial = 'Before **bold**.\n\n¦After.\n';
      const expectedSource = 'Before **bold**.After.\n';
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open(initial, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        final paintStart = mounted.paints.length;
        final expectedGeneration = probe.controller.sourceGeneration + 1;
        await mounted.pressBackspace();
        await _pumpUntilGeneration(
          tester,
          mounted,
          expectedGeneration,
          paintStart,
        );
        final paints = mounted.paints
            .skip(paintStart)
            .where((paint) => paint.sourceGeneration == expectedGeneration)
            .toList();
        _expectStructuralPaints(
          paints,
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedCaret: 'Before **bold**.'.length,
          expectedActiveText: 'Before bold.After.',
          expectedFullText: 'Before bold.After.',
          operation: 'structural Backspace',
        );
        final settleStart = mounted.paints.length;
        await mounted.pumpPresentationSettled();
        _expectStructuralPaints(
          mounted.paints.skip(settleStart).toList(),
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedCaret: 'Before **bold**.'.length,
          expectedActiveText: 'Before bold.After.',
          expectedFullText: 'Before bold.After.',
          operation: 'structural Backspace settle',
          allowEmpty: true,
        );
        await tester.runAsync(
          () => probe.expectSourceAndCaret('Before **bold**.¦After.\n'),
        );
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}

Future<void> _pumpUntilGeneration(
  WidgetTester tester,
  MountedTransitionRecorder mounted,
  int generation,
  int paintStart,
) async {
  for (var turn = 0; turn < 40; turn += 1) {
    if (mounted.paints
        .skip(paintStart)
        .any((paint) => paint.sourceGeneration == generation)) {
      return;
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 8)),
    );
    await mounted.pumpImmediate();
  }
  fail('source generation $generation never painted');
}

void _expectStructuralPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
  required String expectedActiveText,
  required String expectedFullText,
  required String operation,
  bool allowEmpty = false,
}) {
  if (!allowEmpty) expect(paints, isNotEmpty, reason: operation);
  for (final paint in paints) {
    expect(paint.sourceGeneration, expectedGeneration, reason: operation);
    expect(paint.visibleSource, expectedSource, reason: operation);
    expect(paint.canonicalSelectionBaseUtf16, expectedCaret, reason: operation);
    expect(
      paint.canonicalSelectionExtentUtf16,
      expectedCaret,
      reason: operation,
    );
    expect(paint.caretRect, isNotNull, reason: operation);
    expect(paint.caretSourceUtf16, expectedCaret, reason: operation);
    expect(paint.presentation, expectedFullText, reason: operation);
    expect(paint.presentation, isNot(contains('**')), reason: operation);
    final activeRows = paint.rows.where((row) => row.active).toList();
    expect(activeRows, hasLength(1), reason: operation);
    expect(
      activeRows.single.kind,
      expectedActiveText.isEmpty ? anyOf(0, 5) : 5,
      reason:
          '$operation: an exact empty successor is visually identical to a certified empty paragraph',
    );
    expect(
      activeRows.single.text,
      expectedActiveText.isEmpty ? anyOf('', '\n') : expectedActiveText,
      reason:
          '$operation: an exact line ending and an empty paragraph paint the same blank successor',
    );
    expect(
      paint.rows.any(
        (row) => row.runs.any(
          (run) =>
              run.text.contains('bold') &&
              run.styles.contains(FlarkSurfaceInlineStyle.strong) &&
              _resolvedStyleMatches(run, FlarkSurfaceInlineStyle.strong),
        ),
      ),
      isTrue,
      reason: '$operation: predecessor Strong styling was lost',
    );
  }
}

Future<void> _pumpCadence(
  WidgetTester tester,
  MountedTransitionRecorder mounted,
  Duration delay,
) async {
  var remaining = delay.inMilliseconds;
  while (remaining > 0) {
    final slice = remaining < 8 ? remaining : 8;
    await tester.runAsync(
      () => Future<void>.delayed(Duration(milliseconds: slice)),
    );
    await mounted.pumpImmediate();
    remaining -= slice;
  }
}

void _expectDogfoodPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedText,
  required String expectedVisibleSource,
  required int expectedGeneration,
  required int expectedCaret,
  int? expectedBase,
  required String operation,
  bool allowEmpty = false,
}) {
  if (!allowEmpty) {
    expect(
      paints,
      isNotEmpty,
      reason: '$operation: the accepted edit must produce a paint',
    );
  }
  for (final paint in paints) {
    final activeRows = paint.rows.where((row) => row.active).toList();
    expect(activeRows, hasLength(1), reason: operation);
    final active = activeRows.single;
    expect(active.neutral, isFalse, reason: operation);
    expect(active.kind, 5, reason: operation);
    expect(active.headingLevel, isNull, reason: operation);
    expect(active.blockQuoteDepth, isNull, reason: operation);
    expect(active.listItem, isFalse, reason: operation);
    expect(active.table, isFalse, reason: operation);
    expect(active.text, expectedText, reason: operation);
    expect(paint.visibleSource, expectedVisibleSource, reason: operation);
    expect(paint.sourceGeneration, expectedGeneration, reason: operation);
    expect(
      paint.canonicalSelectionExtentUtf16,
      expectedCaret,
      reason: operation,
    );
    final expectedSelectionBase = expectedBase ?? expectedCaret;
    expect(
      paint.canonicalSelectionBaseUtf16,
      expectedSelectionBase,
      reason: operation,
    );
    if (expectedSelectionBase == expectedCaret) {
      expect(paint.caretRect, isNotNull, reason: operation);
      expect(paint.caretSourceUtf16, expectedCaret, reason: operation);
      expect(paint.caretDisplayUtf16, isNotNull, reason: operation);
      expect(paint.selectionRects, isEmpty, reason: operation);
    } else {
      expect(paint.caretRect, isNull, reason: operation);
      expect(paint.caretSourceUtf16, isNull, reason: operation);
      expect(paint.caretDisplayUtf16, isNull, reason: operation);
      expect(paint.selectionRects, isNotEmpty, reason: operation);
    }
    expect(paint.presentation, isNot(contains('# ')), reason: operation);
    expect(paint.presentation, isNot(contains('**')), reason: operation);
    _expectStyledRun(active, (
      text: 'Rust → Dart → Flutter',
      style: FlarkSurfaceInlineStyle.strong,
    ), operation);
    final expectedPlainPrefix = expectedText
        .split('Rust → Dart → Flutter')
        .first;
    final exactPlainText = active.runs
        .where(
          (run) =>
              run.sourceExact &&
              run.styles.isEmpty &&
              run.resolvedStyle == active.resolvedBlockStyle,
        )
        .map((run) => run.text)
        .join();
    expect(
      exactPlainText,
      contains(expectedPlainPrefix),
      reason: '$operation: edited prose inherited an inline style',
    );
  }
}

void _expectScenarioPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required _TypingScenario scenario,
  required String insertedSoFar,
  required String expectedText,
  required int expectedGeneration,
  required int expectedCaret,
  bool allowEmpty = false,
}) {
  if (!allowEmpty) {
    expect(paints, isNotEmpty, reason: scenario.name);
  }
  final expectedSource = scenario.sourceAfter(insertedSoFar);
  final operation =
      '${scenario.name} after ${insertedSoFar.length} UTF-16 units';
  for (final paint in paints) {
    final activeRows = paint.rows.where((row) => row.active).toList();
    expect(
      activeRows,
      isNotEmpty,
      reason:
          '$operation: ${activeRows.map((row) => '${row.ordinal}:${row.kind}:${row.text}').join(' | ')}',
    );
    expect(
      activeRows.map((row) => row.ordinal).toSet(),
      hasLength(1),
      reason: '$operation: all active fragments must belong to one source row',
    );
    for (final active in activeRows) {
      expect(active.neutral, isFalse, reason: operation);
      expect(active.kind, scenario.shell.kind, reason: operation);
      expect(
        active.headingLevel,
        scenario.shell.headingLevel,
        reason: operation,
      );
      expect(
        active.blockQuoteDepth,
        scenario.shell.blockQuoteDepth,
        reason: operation,
      );
      expect(active.listItem, scenario.shell.listItem, reason: operation);
      expect(active.table, scenario.shell.table, reason: operation);
      expect(active.text, expectedText, reason: operation);
    }
    expect(paint.sourceGeneration, expectedGeneration, reason: operation);
    expect(paint.visibleSource, expectedSource, reason: operation);
    expect(
      paint.canonicalSelectionBaseUtf16,
      expectedCaret,
      reason: scenario.name,
    );
    expect(
      paint.canonicalSelectionExtentUtf16,
      expectedCaret,
      reason: scenario.name,
    );
    expect(paint.caretRect, isNotNull, reason: scenario.name);
    expect(paint.caretSourceUtf16, expectedCaret, reason: scenario.name);
    final fullDisplayCaret =
        scenario.renderedBefore.length + insertedSoFar.length;
    final caretFragments = activeRows
        .where(
          (row) =>
              (fullDisplayCaret >= row.fragmentStart &&
                  fullDisplayCaret < row.fragmentEnd) ||
              (fullDisplayCaret == row.fragmentEnd &&
                  row.fragmentEnd == row.text.length),
        )
        .toList();
    expect(caretFragments, hasLength(1), reason: operation);
    final caretFragment = caretFragments.single;
    expect(
      paint.caretDisplayUtf16,
      caretFragment.leadingText.length +
          fullDisplayCaret -
          caretFragment.fragmentStart,
      reason: '${scenario.name}: painted caret display offset',
    );
    for (final marker in scenario.forbiddenMarkers) {
      expect(
        paint.presentation,
        isNot(contains(marker)),
        reason: '${scenario.name}: unrelated source marker $marker painted',
      );
    }
    for (final expected in scenario.staticStyledRuns) {
      _expectStyledRunAcrossRows(activeRows, expected, scenario.name);
    }
    if (scenario.dynamicStyledBefore != null) {
      _expectStyledRunAcrossRows(activeRows, (
        text:
            '${scenario.dynamicStyledBefore}$insertedSoFar${scenario.dynamicStyledAfter}',
        style: scenario.dynamicStyle!,
      ), scenario.name);
    }
    _expectInsertedSourceStyle(
      activeRows,
      sourceStart: scenario.initialCaret,
      sourceEnd: scenario.initialCaret + insertedSoFar.length,
      styles: scenario.insertedStyles,
      operation: operation,
    );
  }
}

void _expectInsertedSourceStyle(
  List<FlarkSurfacePaintRowObservation> rows, {
  required int sourceStart,
  required int sourceEnd,
  required Set<FlarkSurfaceInlineStyle> styles,
  required String operation,
}) {
  final covering = rows
      .expand((row) => row.runs.map((run) => (row: row, run: run)))
      .where(
        (entry) =>
            entry.run.sourceUtf16Start < sourceEnd &&
            sourceStart < entry.run.sourceUtf16End,
      )
      .toList();
  expect(covering, isNotEmpty, reason: '$operation: inserted source vanished');
  expect(
    covering
        .map((entry) => entry.run.sourceUtf16Start)
        .reduce((left, right) => left < right ? left : right),
    lessThanOrEqualTo(sourceStart),
    reason: operation,
  );
  expect(
    covering
        .map((entry) => entry.run.sourceUtf16End)
        .reduce((left, right) => left > right ? left : right),
    greaterThanOrEqualTo(sourceEnd),
    reason: operation,
  );
  for (final entry in covering) {
    expect(entry.run.styles, styles, reason: operation);
    if (styles.isEmpty) {
      expect(entry.run.sourceExact, isTrue, reason: operation);
      expect(
        entry.run.resolvedStyle,
        entry.row.resolvedBlockStyle,
        reason: '$operation: plain edit inherited an inline style',
      );
    }
  }
}

void _expectStyledRunAcrossRows(
  List<FlarkSurfacePaintRowObservation> rows,
  ({String text, FlarkSurfaceInlineStyle style}) expected,
  String scenario,
) {
  final ordered = [...rows]
    ..sort((left, right) => left.fragmentStart.compareTo(right.fragmentStart));
  final styledText = ordered
      .expand((row) => row.runs)
      .where(
        (run) =>
            run.styles.contains(expected.style) &&
            _resolvedStyleMatches(run, expected.style),
      )
      .map((run) => run.text)
      .join();
  expect(
    styledText,
    contains(expected.text),
    reason:
        '$scenario: ${expected.style.name} style missing for ${expected.text}',
  );
}

void _expectStyledRun(
  FlarkSurfacePaintRowObservation row,
  ({String text, FlarkSurfaceInlineStyle style}) expected,
  String scenario,
) {
  expect(
    _hasStyledRun(row, expected),
    isTrue,
    reason:
        '$scenario: ${expected.style.name} style missing for ${expected.text}',
  );
}

bool _hasStyledRun(
  FlarkSurfacePaintRowObservation row,
  ({String text, FlarkSurfaceInlineStyle style}) expected,
) => row.runs.any((run) {
  if (!run.text.contains(expected.text) ||
      !run.styles.contains(expected.style)) {
    return false;
  }
  return _resolvedStyleMatches(run, expected.style);
});

bool _resolvedStyleMatches(
  FlarkSurfacePaintRunObservation run,
  FlarkSurfaceInlineStyle style,
) => switch (style) {
  FlarkSurfaceInlineStyle.strong =>
    run.resolvedStyle.fontWeight == FontWeight.w700,
  FlarkSurfaceInlineStyle.emphasis =>
    run.resolvedStyle.fontStyle == FontStyle.italic,
  FlarkSurfaceInlineStyle.code => run.resolvedStyle.fontFamily == 'Menlo',
  FlarkSurfaceInlineStyle.strikethrough =>
    run.resolvedStyle.decoration?.contains(TextDecoration.lineThrough) == true,
  FlarkSurfaceInlineStyle.link =>
    run.resolvedStyle.decoration?.contains(TextDecoration.underline) == true,
};

final class _TypingScenario {
  const _TypingScenario({
    required this.name,
    required this.initial,
    required this.inserted,
    required this.renderedBefore,
    required this.renderedAfter,
    required this.finalMarked,
    required this.forbiddenMarkers,
    required this.shell,
    this.staticStyledRuns = const [],
    this.dynamicStyledBefore,
    this.dynamicStyledAfter,
    this.dynamicStyle,
    this.insertedStyles = const {},
    this.unpumpedBurst = false,
  }) : assert(
         (dynamicStyledBefore == null) == (dynamicStyledAfter == null) &&
             (dynamicStyledBefore == null) == (dynamicStyle == null),
       );

  final String name;
  final String initial;
  final String inserted;
  final String renderedBefore;
  final String renderedAfter;
  final String finalMarked;
  final List<String> forbiddenMarkers;
  final _ExpectedShell shell;
  final List<({String text, FlarkSurfaceInlineStyle style})> staticStyledRuns;
  final String? dynamicStyledBefore;
  final String? dynamicStyledAfter;
  final FlarkSurfaceInlineStyle? dynamicStyle;
  final Set<FlarkSurfaceInlineStyle> insertedStyles;
  final bool unpumpedBurst;

  int get initialCaret => MarkedSource.parse(initial).caret;

  String sourceAfter(String inserted) {
    final marked = MarkedSource.parse(initial);
    return marked.source.replaceRange(marked.caret, marked.caret, inserted);
  }
}

typedef _ExpectedShell = ({
  int kind,
  int? headingLevel,
  int? blockQuoteDepth,
  bool listItem,
  bool table,
});

const _paragraphShell = (
  kind: 5,
  headingLevel: null,
  blockQuoteDepth: null,
  listItem: false,
  table: false,
);
const _listShell = (
  kind: 5,
  headingLevel: null,
  blockQuoteDepth: null,
  listItem: true,
  table: false,
);
const _quoteShell = (
  kind: 5,
  headingLevel: null,
  blockQuoteDepth: 1,
  listItem: false,
  table: false,
);
const _tableShell = (
  kind: 5,
  headingLevel: null,
  blockQuoteDepth: null,
  listItem: false,
  table: true,
);
const _fencedCodeShell = (
  kind: 7,
  headingLevel: null,
  blockQuoteDepth: null,
  listItem: false,
  table: false,
);

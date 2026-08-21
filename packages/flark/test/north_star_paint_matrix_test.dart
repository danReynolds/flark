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
    unpumpedBurst: true,
  ),
  _TypingScenario(
    name: 'typing inside a certified Strong word',
    initial: 'Before **bo¦ld** after.\n',
    inserted: 'ke',
    renderedBefore: 'Before bo',
    renderedAfter: 'ld after.',
    finalMarked: 'Before **boke¦ld** after.\n',
    forbiddenMarkers: ['**'],
    dynamicStrongBefore: 'bo',
    dynamicStrongAfter: 'ld',
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
    expect(active.kind, isNot(0), reason: operation);
    expect(active.text, expectedText, reason: operation);
    expect(paint.visibleSource, expectedVisibleSource, reason: operation);
    expect(paint.sourceGeneration, expectedGeneration, reason: operation);
    expect(
      paint.canonicalSelectionExtentUtf16,
      expectedCaret,
      reason: operation,
    );
    expect(paint.caretRect, isNotNull, reason: operation);
    expect(paint.caretSourceUtf16, expectedCaret, reason: operation);
    expect(paint.canonicalSelectionBaseUtf16, expectedCaret, reason: operation);
    expect(paint.caretDisplayUtf16, isNotNull, reason: operation);
    expect(paint.presentation, isNot(contains('# ')), reason: operation);
    expect(paint.presentation, isNot(contains('**')), reason: operation);
    _expectStyledRun(active, (
      text: 'Rust → Dart → Flutter',
      style: FlarkSurfaceInlineStyle.strong,
    ), operation);
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
      expect(active.kind, isNot(0), reason: operation);
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
    if (scenario.dynamicStrongBefore != null) {
      _expectStyledRunAcrossRows(activeRows, (
        text:
            '${scenario.dynamicStrongBefore}$insertedSoFar${scenario.dynamicStrongAfter}',
        style: FlarkSurfaceInlineStyle.strong,
      ), scenario.name);
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
    this.staticStyledRuns = const [],
    this.dynamicStrongBefore,
    this.dynamicStrongAfter,
    this.unpumpedBurst = false,
  }) : assert((dynamicStrongBefore == null) == (dynamicStrongAfter == null));

  final String name;
  final String initial;
  final String inserted;
  final String renderedBefore;
  final String renderedAfter;
  final String finalMarked;
  final List<String> forbiddenMarkers;
  final List<({String text, FlarkSurfaceInlineStyle style})> staticStyledRuns;
  final String? dynamicStrongBefore;
  final String? dynamicStrongAfter;
  final bool unpumpedBurst;

  String sourceAfter(String inserted) {
    final marked = MarkedSource.parse(initial);
    return marked.source.replaceRange(marked.caret, marked.caret, inserted);
  }
}

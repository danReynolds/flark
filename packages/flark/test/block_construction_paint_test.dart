import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

typedef _BlockConstructionCase = ({
  String name,
  String prefix,
  int kind,
  int? headingLevel,
  int? quoteDepth,
  bool listItem,
});

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  for (final scenario in const <_BlockConstructionCase>[
    (
      name: 'ATX heading',
      prefix: '# ',
      kind: 12,
      headingLevel: 1,
      quoteDepth: null,
      listItem: false,
    ),
    (
      name: 'block quote',
      prefix: '> ',
      kind: 5,
      headingLevel: null,
      quoteDepth: 1,
      listItem: false,
    ),
    (
      name: 'bullet list',
      prefix: '- ',
      kind: 5,
      headingLevel: null,
      quoteDepth: null,
      listItem: true,
    ),
    (
      name: 'ordered list',
      prefix: '1. ',
      kind: 5,
      headingLevel: null,
      quoteDepth: null,
      listItem: true,
    ),
  ]) {
    testWidgets(
      '${scenario.name} construction and removal paint the clean block shell',
      (tester) async {
        const body = 'change this line\n\n**sentinel**\n';
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            '¦$body',
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          var source = body;
          var caret = 0;
          var currentPrefix = '';
          for (final rune in scenario.prefix.runes) {
            final character = String.fromCharCode(rune);
            final paintStart = mounted.paints.length;
            final generation = probe.controller.sourceGeneration + 1;
            await mounted.typeText(character);
            source = source.replaceRange(caret, caret, character);
            caret += character.length;
            currentPrefix += character;
            await mounted.pumpImmediate();
            await mounted.pumpPresentationSettled();
            final paints = mounted.paints
                .skip(paintStart)
                .toList(growable: false);
            _expectSourceAndSentinel(
              paints,
              expectedSource: source,
              expectedCaret: caret,
              expectedGeneration: generation,
              operation: '${scenario.name} insert $character',
            );
            _expectBlockState(
              paints,
              scenario,
              currentPrefix: currentPrefix,
              bodyLine: 'change this line',
              operation: '${scenario.name} insert $character',
            );
          }

          final paintStart = mounted.paints.length;
          final generation = probe.controller.sourceGeneration + 1;
          await mounted.pressBackspace();
          source = body;
          caret = 0;
          currentPrefix = '';
          await mounted.pumpImmediate();
          await mounted.pumpPresentationSettled();
          final paints = mounted.paints
              .skip(paintStart)
              .toList(growable: false);
          _expectSourceAndSentinel(
            paints,
            expectedSource: source,
            expectedCaret: caret,
            expectedGeneration: generation,
            operation: '${scenario.name} structural Backspace',
          );
          _expectBlockState(
            paints,
            scenario,
            currentPrefix: currentPrefix,
            bodyLine: 'change this line',
            operation: '${scenario.name} structural Backspace',
          );

          expect(source, body);
          await tester.runAsync(() => probe.expectSourceAndCaret('¦$body'));
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
      '${scenario.name} unpumped prefix burst paints only the final block shell',
      (tester) async {
        const body = 'change this line\n\n**sentinel**\n';
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            '¦$body',
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          final paintStart = mounted.paints.length;
          final initialGeneration = probe.controller.sourceGeneration;
          await mounted.typeText(scenario.prefix);
          expect(
            mounted.paints.skip(paintStart),
            isEmpty,
            reason: 'no synthetic pump may split the prefix burst',
          );

          await mounted.pumpImmediate();
          final source = '${scenario.prefix}$body';
          final caret = scenario.prefix.length;
          final generation = initialGeneration + scenario.prefix.runes.length;
          var paints = mounted.paints.skip(paintStart).toList(growable: false);
          _expectSourceAndSentinel(
            paints,
            expectedSource: source,
            expectedCaret: caret,
            expectedGeneration: generation,
            operation: '${scenario.name} unpumped prefix burst',
          );
          _expectBlockState(
            paints,
            scenario,
            currentPrefix: scenario.prefix,
            bodyLine: 'change this line',
            operation: '${scenario.name} unpumped prefix burst',
          );

          final settleStart = mounted.paints.length;
          await mounted.pumpPresentationSettled();
          paints = mounted.paints.skip(settleStart).toList(growable: false);
          if (paints.isNotEmpty) {
            _expectSourceAndSentinel(
              paints,
              expectedSource: source,
              expectedCaret: caret,
              expectedGeneration: generation,
              operation: '${scenario.name} unpumped prefix burst settle',
            );
            _expectBlockState(
              paints,
              scenario,
              currentPrefix: scenario.prefix,
              bodyLine: 'change this line',
              operation: '${scenario.name} unpumped prefix burst settle',
            );
          }

          await tester.runAsync(
            () => probe.expectSourceAndCaret('${scenario.prefix}¦$body'),
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

void _expectSourceAndSentinel(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedCaret,
  required int expectedGeneration,
  required String operation,
}) {
  expect(paints, isNotEmpty, reason: operation);
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
    expect(paint.presentation, isNot(contains('**')), reason: operation);
    final sentinelRuns = paint.rows
        .expand((row) => row.runs)
        .where((run) => run.text == 'sentinel')
        .toList(growable: false);
    expect(sentinelRuns, isNotEmpty, reason: operation);
    expect(
      sentinelRuns.every(
        (run) => run.styles.contains(FlarkSurfaceInlineStyle.strong),
      ),
      isTrue,
      reason: operation,
    );
  }
}

void _expectBlockState(
  List<FlarkSurfacePaintObservation> paints,
  _BlockConstructionCase scenario, {
  required String currentPrefix,
  required String bodyLine,
  required String operation,
}) {
  final structural = switch (scenario.name) {
    'block quote' => currentPrefix.startsWith('>'),
    _ => currentPrefix == scenario.prefix,
  };
  final expectedPresentation = structural
      ? switch (scenario.name) {
          'ATX heading' => bodyLine,
          'block quote' => '│ $bodyLine',
          _ => '$currentPrefix$bodyLine',
        }
      : '$currentPrefix$bodyLine';
  for (final paint in paints) {
    final active = paint.rows
        .where((row) => row.active)
        .toList(growable: false);
    final details = active
        .map(
          (row) => (
            neutral: row.neutral,
            kind: row.kind,
            heading: row.headingLevel,
            quote: row.blockQuoteDepth,
            list: row.listItem,
            leading: row.leadingText,
            text: row.text,
          ),
        )
        .toList(growable: false);
    expect(active, isNotEmpty, reason: '$operation $details');
    final actualPresentation = active
        .map((row) => '${row.leadingText}${row.text}')
        .join()
        .replaceFirst(RegExp(r'\r?\n$'), '');
    expect(
      actualPresentation,
      expectedPresentation,
      reason: '$operation $details',
    );
    expect(
      active.every((row) => !structural || !row.neutral),
      isTrue,
      reason: '$operation $details',
    );
    expect(
      active.every(
        (row) => structural
            ? row.kind == scenario.kind
            : row.kind == 0 || row.kind == 5,
      ),
      isTrue,
      reason: '$operation $details',
    );
    expect(
      active.every(
        (row) =>
            row.headingLevel == (structural ? scenario.headingLevel : null),
      ),
      isTrue,
      reason: '$operation $details',
    );
    expect(
      active.every(
        (row) =>
            row.blockQuoteDepth == (structural ? scenario.quoteDepth : null),
      ),
      isTrue,
      reason: '$operation $details',
    );
    expect(
      active.every((row) => row.listItem == (structural && scenario.listItem)),
      isTrue,
      reason: '$operation $details',
    );
  }
}

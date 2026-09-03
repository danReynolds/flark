import 'dart:io';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/widgets.dart';
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

typedef _CleanConstructionFrame = ({
  String source,
  _CleanConstructionPresentation presentation,
  int caret,
  int generation,
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

  testWidgets(
    'fenced code construction paints each clean parser result',
    (tester) async {
      const body = 'change this line\n\n**sentinel**\n';
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open('¦$body', libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      var source = body;
      var caret = 0;

      Future<void> insert(String text, {required String operation}) async {
        final precedingSource = source;
        final precedingCaret = caret;
        final precedingGeneration = probe.controller.sourceGeneration;
        final generation = precedingGeneration + 1;
        source = source.replaceRange(caret, caret, text);
        caret += text.length;
        final cleanPresentation = (await tester.runAsync(
          () => _cleanPresentation(source, libraryPath!),
        ))!;
        final paintStart = mounted.paints.length;
        if (text == '\n') {
          await mounted.pressReturn();
        } else {
          await mounted.typeText(text);
        }
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();
        _expectCleanConstructionPaints(
          mounted.paints.skip(paintStart).toList(growable: false),
          expectedSource: source,
          expectedPresentation: cleanPresentation,
          expectedCaret: caret,
          expectedGeneration: generation,
          precedingSource: precedingSource,
          precedingCaret: precedingCaret,
          precedingGeneration: precedingGeneration,
          operation: operation,
        );
      }

      try {
        for (final rune in '```dart'.runes) {
          final character = String.fromCharCode(rune);
          await insert(character, operation: 'opening fence $character');
        }
        expect(
          probe.controller.debugProjectionContinuityActive,
          isTrue,
          reason: 'the opening plan retains its parser-declared Return step',
        );
        await insert('\n', operation: 'opening fence Return');
        expect(
          probe.controller.debugProjectionContinuityActive,
          isFalse,
          reason: 'the completed opening plan retires after fresh authority',
        );

        caret = source.indexOf('change this line') + 'change this line'.length;
        await mounted.moveCaret(caret);
        await insert('\n', operation: 'closing fence Return');
        for (final rune in '```'.runes) {
          final character = String.fromCharCode(rune);
          await insert(character, operation: 'closing fence $character');
        }
        expect(
          probe.controller.debugProjectionContinuityActive,
          isFalse,
          reason: 'the completed closing plan retires after fresh authority',
        );

        final finalPaint = mounted.paints.last;
        expect(finalPaint.presentation, isNot(contains('**')));
        expect(
          finalPaint.rows.any(
            (row) => row.kind == 7 && row.text.contains('change this line'),
          ),
          isTrue,
        );
        final sentinelRuns = finalPaint.rows
            .expand((row) => row.runs)
            .where((run) => run.text == 'sentinel')
            .toList(growable: false);
        expect(sentinelRuns, isNotEmpty);
        expect(
          sentinelRuns.every(
            (run) => run.styles.contains(FlarkSurfaceInlineStyle.strong),
          ),
          isTrue,
        );
        await tester.runAsync(() => probe.expectHealthy());
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
    'fenced code construction true bursts paint the same clean results',
    (tester) async {
      const body = 'change this line\n\n**sentinel**\n';
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open('¦$body', libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      var source = body;

      try {
        var paintStart = mounted.paints.length;
        final initialGeneration = probe.controller.sourceGeneration;
        final openingGeneration = initialGeneration + 8;
        await mounted.typeText('```dart\n');
        source = '```dart\n$source';
        final openingPrefixSource = '```dart$body';
        final openingPrefixClean = (await tester.runAsync(
          () => _cleanPresentation(openingPrefixSource, libraryPath!),
        ))!;
        final openingClean = (await tester.runAsync(
          () => _cleanPresentation(source, libraryPath!),
        ))!;
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();
        _expectCleanConstructionPaints(
          mounted.paints.skip(paintStart).toList(growable: false),
          expectedSource: source,
          expectedPresentation: openingClean,
          expectedCaret: 8,
          expectedGeneration: openingGeneration,
          precedingSource: body,
          precedingCaret: 0,
          precedingGeneration: initialGeneration,
          intermediateFrames: [
            (
              source: openingPrefixSource,
              presentation: openingPrefixClean,
              caret: 7,
              generation: initialGeneration + 7,
            ),
          ],
          operation: 'true-burst opening fence',
        );

        final closingCaret =
            source.indexOf('change this line') + 'change this line'.length;
        await mounted.moveCaret(closingCaret);
        mounted.paints.clear();
        paintStart = mounted.paints.length;
        final closingPrecedingSource = source;
        final closingPrecedingGeneration = probe.controller.sourceGeneration;
        final closingGeneration = closingPrecedingGeneration + 4;
        await mounted.typeText('\n```');
        source = source.replaceRange(closingCaret, closingCaret, '\n```');
        final closingClean = (await tester.runAsync(
          () => _cleanPresentation(source, libraryPath!),
        ))!;
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();
        _expectCleanConstructionPaints(
          mounted.paints.skip(paintStart).toList(growable: false),
          expectedSource: source,
          expectedPresentation: closingClean,
          expectedCaret: closingCaret + 4,
          expectedGeneration: closingGeneration,
          precedingSource: closingPrecedingSource,
          precedingCaret: closingCaret,
          precedingGeneration: closingPrecedingGeneration,
          operation: 'true-burst closing fence',
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

Future<_CleanConstructionPresentation> _cleanPresentation(
  String source,
  String libraryPath,
) async {
  final controller = await FlarkEditorController.open(
    source,
    libraryPath: libraryPath,
  );
  try {
    await controller.continueParsing();
    final presentations = <String>[];
    final rows = <_CleanConstructionRow>[];
    var cursor = controller.visibleUtf16Start;
    for (final row in controller.rows) {
      final range = controller.surfaceSourceRange(row);
      if (range.start > cursor) {
        presentations.addAll(
          _cleanNeutralGap(
            controller,
            cursor,
            range.start,
            hasPrecedingRow: cursor > controller.visibleUtf16Start,
            hasFollowingRow: true,
          ),
        );
      }
      final surfaces = controller.surfaceRowsFor(
        row,
        includeEditingState: false,
      );
      presentations.addAll(
        surfaces.map((surface) => '${surface.leadingText}${surface.text}'),
      );
      rows.addAll(surfaces.map(_CleanConstructionRow.fromSurface));
      if (range.end > cursor) cursor = range.end;
    }
    final visibleEnd =
        controller.visibleUtf16Start + controller.visibleSource.length;
    if (cursor < visibleEnd) {
      presentations.addAll(
        _cleanNeutralGap(
          controller,
          cursor,
          visibleEnd,
          hasPrecedingRow: true,
          hasFollowingRow: false,
        ),
      );
    }
    return _CleanConstructionPresentation(
      presentation: presentations.join('\n'),
      rows: List.unmodifiable(rows),
    );
  } finally {
    await controller.close();
  }
}

Iterable<String> _cleanNeutralGap(
  FlarkEditorController controller,
  int globalStart,
  int globalEnd, {
  required bool hasPrecedingRow,
  required bool hasFollowingRow,
}) sync* {
  final localStart = globalStart - controller.visibleUtf16Start;
  final localEnd = globalEnd - controller.visibleUtf16Start;
  final lines = <String>[];
  var cursor = localStart;
  while (cursor < localEnd) {
    final newline = controller.visibleSource.indexOf('\n', cursor);
    final end = newline == -1 || newline >= localEnd ? localEnd : newline + 1;
    lines.add(controller.visibleSource.substring(cursor, end));
    cursor = end;
  }
  assert(!hasPrecedingRow || globalStart >= controller.visibleUtf16Start);
  final first = hasPrecedingRow && lines.length >= 3 ? 1 : 0;
  final end = lines.length - (hasFollowingRow ? 1 : 0);
  for (var index = first; index < end; index += 1) {
    yield lines[index];
  }
}

void _expectCleanConstructionPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required _CleanConstructionPresentation expectedPresentation,
  required int expectedCaret,
  required int expectedGeneration,
  required String precedingSource,
  required int precedingCaret,
  required int precedingGeneration,
  List<_CleanConstructionFrame> intermediateFrames = const [],
  required String operation,
}) {
  expect(paints, isNotEmpty, reason: operation);
  final resultStart = paints.indexWhere(
    (paint) => paint.sourceGeneration == expectedGeneration,
  );
  expect(resultStart, isNonNegative, reason: '$operation result publication');
  for (final paint in paints.take(resultStart)) {
    // A caret-only repaint already scheduled for the certified predecessor
    // may land while an asynchronous structural receipt is in flight. It is
    // valid only while the entire predecessor frame remains coherent.
    final retainsPredecessor =
        paint.sourceGeneration == precedingGeneration &&
        paint.visibleSource == precedingSource &&
        paint.canonicalSelectionBaseUtf16 == precedingCaret &&
        paint.canonicalSelectionExtentUtf16 == precedingCaret &&
        paint.caretSourceUtf16 == precedingCaret;
    if (retainsPredecessor) continue;
    final matching = intermediateFrames
        .where(
          (frame) =>
              frame.generation == paint.sourceGeneration &&
              frame.source == paint.visibleSource &&
              frame.caret == paint.canonicalSelectionBaseUtf16 &&
              frame.caret == paint.canonicalSelectionExtentUtf16 &&
              frame.caret == paint.caretSourceUtf16,
        )
        .toList(growable: false);
    expect(
      matching,
      hasLength(1),
      reason: '$operation undeclared intermediate publication',
    );
    _expectPaintMatchesCleanConstruction(
      paint,
      expectedSource: matching.single.source,
      expectedPresentation: matching.single.presentation,
      expectedCaret: matching.single.caret,
      expectedGeneration: matching.single.generation,
      operation: '$operation clean intermediate',
    );
  }
  for (final paint in paints.skip(resultStart)) {
    _expectPaintMatchesCleanConstruction(
      paint,
      expectedSource: expectedSource,
      expectedPresentation: expectedPresentation,
      expectedCaret: expectedCaret,
      expectedGeneration: expectedGeneration,
      operation: operation,
    );
  }
}

void _expectPaintMatchesCleanConstruction(
  FlarkSurfacePaintObservation paint, {
  required String expectedSource,
  required _CleanConstructionPresentation expectedPresentation,
  required int expectedCaret,
  required int expectedGeneration,
  required String operation,
}) {
  expect(paint.sourceGeneration, expectedGeneration, reason: operation);
  expect(paint.visibleSource, expectedSource, reason: operation);
  expect(paint.canonicalSelectionBaseUtf16, expectedCaret, reason: operation);
  expect(paint.canonicalSelectionExtentUtf16, expectedCaret, reason: operation);
  expect(paint.caretRect, isNotNull, reason: operation);
  expect(paint.caretSourceUtf16, expectedCaret, reason: operation);
  // Intermediate literal delimiters may remain exact in the active physical
  // line. Compare visible authored content here; the final assertions below
  // separately require the completed typed code shell and outside styles.
  expect(
    _nonblankConstructionLines(paint.presentation),
    _nonblankConstructionLines(expectedPresentation.presentation),
    reason: operation,
  );
  final actualRows = paint.rows
      .where((row) => !row.neutral)
      .toList(growable: false);
  expect(
    actualRows,
    hasLength(expectedPresentation.rows.length),
    reason: operation,
  );
  for (var rowIndex = 0; rowIndex < actualRows.length; rowIndex += 1) {
    final actual = actualRows[rowIndex];
    final expected = expectedPresentation.rows[rowIndex];
    expect(actual.kind, expected.kind, reason: '$operation row $rowIndex');
    expect(actual.text, expected.text, reason: '$operation row $rowIndex');
    expect(
      actual.sourceUtf16Start,
      expected.globalUtf16Start,
      reason: '$operation row $rowIndex',
    );
    expect(actual.runs, hasLength(expected.runs.length), reason: operation);
    for (var runIndex = 0; runIndex < actual.runs.length; runIndex += 1) {
      final actualRun = actual.runs[runIndex];
      final expectedRun = expected.runs[runIndex];
      expect(actualRun.text, expectedRun.text, reason: operation);
      expect(
        actualRun.sourceUtf16Start,
        expectedRun.sourceUtf16Start,
        reason: operation,
      );
      expect(
        actualRun.sourceUtf16End,
        expectedRun.sourceUtf16End,
        reason: operation,
      );
      expect(actualRun.sourceExact, expectedRun.sourceExact, reason: operation);
      expect(actualRun.styles, expectedRun.styles, reason: operation);
      for (final style in expectedRun.styles) {
        expect(
          _resolvedConstructionStyleMatches(actualRun, style),
          isTrue,
          reason: '$operation row $rowIndex run $runIndex $style',
        );
      }
    }
  }
}

bool _resolvedConstructionStyleMatches(
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

final class _CleanConstructionPresentation {
  const _CleanConstructionPresentation({
    required this.presentation,
    required this.rows,
  });

  final String presentation;
  final List<_CleanConstructionRow> rows;
}

final class _CleanConstructionRow {
  const _CleanConstructionRow({
    required this.kind,
    required this.text,
    required this.globalUtf16Start,
    required this.runs,
  });

  factory _CleanConstructionRow.fromSurface(FlarkSurfaceRow surface) =>
      _CleanConstructionRow(
        kind: surface.kind,
        text: surface.text,
        globalUtf16Start: surface.globalUtf16Start,
        runs: List.unmodifiable(
          surface.runs.map(
            (run) => _CleanConstructionRun(
              text: run.text,
              sourceUtf16Start: run.sourceUtf16Start,
              sourceUtf16End: run.sourceUtf16End,
              sourceExact: run.sourceExact,
              styles: run.styles,
            ),
          ),
        ),
      );

  final int kind;
  final String text;
  final int globalUtf16Start;
  final List<_CleanConstructionRun> runs;
}

final class _CleanConstructionRun {
  const _CleanConstructionRun({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.sourceExact,
    required this.styles,
  });

  final String text;
  final int sourceUtf16Start;
  final int sourceUtf16End;
  final bool sourceExact;
  final Set<FlarkSurfaceInlineStyle> styles;
}

List<String> _nonblankConstructionLines(String presentation) => presentation
    .split('\n')
    .where((line) => line.isNotEmpty)
    .toList(growable: false);

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

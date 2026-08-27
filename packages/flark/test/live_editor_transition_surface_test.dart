import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'syntax hazards never paint an unrelated certified block as source',
    (tester) async {
      const cases = <(String, String)>[
        ('pla¦in', '*'),
        ('pla¦in', '['),
        ('**bo¦ld**', '_'),
        ('**bo¦ld**', ']'),
        ('_e¦m_', '`'),
        ('_e¦m_', '~'),
        ('`co¦de`', '['),
        ('[la¦bel](https://example.test)', ']'),
        ('[la¦bel](https://example.test)', '>'),
        ('- it¦em', '~'),
        ('> qu¦ote', '>'),
        ('> qu¦ote', '*'),
        ('*op¦en', '_'),
        (r'escaped \¦* literal', 'a'),
        (r'escaped \¦* literal', '`'),
      ];
      for (final (subject, character) in cases) {
        final marked = '**sentinel**\n\n$subject\n';
        final initial = MarkedSource.parse(marked);
        final expectedSource = initial.source.replaceRange(
          initial.caret,
          initial.caret,
          character,
        );
        final probe = (await tester.runAsync(
          () =>
              LiveEditorTransitionProbe.open(marked, libraryPath: libraryPath!),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          await mounted.typeText(character);
          await mounted.pumpImmediate();

          // A parser-unproven syntax hazard may retain the prior immutable
          // publication until certification instead of emitting a speculative
          // frame. Receipt-backed literal edits have a separate immediate-
          // paint acceptance suite; this matrix requires that every frame it
          // does emit keeps unrelated certified content rendered.
          expect(
            mounted.paints,
            everyElement(
              isA<FlarkSurfacePaintObservation>().having(
                (paint) => paint.presentation,
                'presentation for $subject + $character',
                isNot(contains('**sentinel**')),
              ),
            ),
          );

          await mounted.pumpPresentationSettled();
          expect(mounted.paints, isNotEmpty, reason: '$subject + $character');
          expect(
            mounted.paints.last.rows.any(
              (row) => !row.neutral && row.text == 'sentinel',
            ),
            isTrue,
            reason: '$subject + $character demoted the sentinel',
          );

          expect(
            await tester.runAsync(probe.controller.readSource),
            expectedSource,
          );
          expect(
            probe.controller.globalCaretOffset,
            initial.caret + character.length,
          );
          await tester.runAsync(probe.expectHealthy);
          await tester.runAsync(probe.expectConvergesWithCleanRebuild);

          if (subject == 'pla¦in' && character == '*') {
            mounted.paints.clear();
            await mounted.pressBackspace();
            await mounted.pumpImmediate();
            await mounted.pumpPresentationSettled();
            expect(
              await tester.runAsync(probe.controller.readSource),
              initial.source,
            );
            expect(probe.controller.globalCaretOffset, initial.caret);
            expect(
              mounted.paints,
              everyElement(
                isA<FlarkSurfacePaintObservation>().having(
                  (paint) => paint.presentation,
                  'presentation after deleting the syntax hazard',
                  isNot(contains('**sentinel**')),
                ),
              ),
            );
          }
        } finally {
          await mounted.close();
          await tester.runAsync(probe.close);
        }
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'blank-line Return and Backspace have one-line geometry and exact source',
    (tester) async {
      const original = 'alpha\n\n## next\n';
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'alpha¦\n\n## next\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        final afterFirst = original.replaceRange(5, 5, '\n\n');
        expect(await tester.runAsync(probe.controller.readSource), afterFirst);
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        final afterSecond = afterFirst.replaceRange(7, 7, '\n');
        expect(await tester.runAsync(probe.controller.readSource), afterSecond);

        final neutralRows = mounted.paints.last.rows
            .where((row) => row.neutral)
            .toList(growable: false);
        expect(neutralRows, isNotEmpty);
        expect(
          neutralRows.map((row) => row.rect.height),
          everyElement(lessThanOrEqualTo(30)),
          reason: 'one source newline must occupy one visual line',
        );

        await mounted.pressBackspace();
        await mounted.pumpPresentationSettled();
        expect(await tester.runAsync(probe.controller.readSource), afterFirst);
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'typing into a settled blank block keeps row and sibling geometry stable',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Plain text.¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final finalPaint = mounted.paints.last;
        FlarkSurfacePaintRowObservation rowNamed(
          FlarkSurfacePaintObservation paint,
          String text,
        ) => paint.rows.singleWhere(
          (row) => row.text.replaceFirst(RegExp(r'\r?\n$'), '') == text,
        );

        final finalActive = rowNamed(finalPaint, 'x');
        final finalSentinel = rowNamed(finalPaint, 'sentinel');
        for (final paint in mounted.paints) {
          final active = rowNamed(paint, 'x');
          final sentinel = rowNamed(paint, 'sentinel');
          expect(active.rect, finalActive.rect, reason: paint.presentation);
          expect(
            active.resolvedBlockStyle,
            finalActive.resolvedBlockStyle,
            reason: paint.presentation,
          );
          expect(sentinel.rect, finalSentinel.rect, reason: paint.presentation);
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Return plus immediate typing never paints a redundant trailing blank',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Plain text.¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['Plain text.', 'x', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = mounted.paints.last.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => row.text),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'typing after a settled Return consumes only its block separator',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Plain text.¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['Plain text.', 'x', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = mounted.paints.last.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => row.text),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'deleting a one-character paragraph preserves every blank row per frame',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Plain text.¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.typeText('x');
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.pressBackspace();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['Plain text.', '', '', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = mounted.paints.last.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => row.text),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'list Return paints one marker with stable row geometry and style',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '- list item¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['- list item', '- ', 'sentinel'];
        final finalPaint = mounted.paints.last;
        final finalRects = finalPaint.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = finalPaint.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => '${row.leadingText}${row.text}'),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'ordinary list Backspace keeps projected marker geometry every frame',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '- list item¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressBackspace();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['- list ite', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = mounted.paints.last.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => '${row.leadingText}${row.text}'),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'empty list exit plus immediate typing never paints marker or blank rows',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '- list item¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.pressReturn();
        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['- list item', 'x', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => '${row.leadingText}${row.text}'),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'empty list Backspace preserves every editor-owned blank row',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '- list item¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpImmediate();
        mounted.paints.clear();

        await mounted.pressBackspace();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['- list item', '', '', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = mounted.paints.last.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => '${row.leadingText}${row.text}'),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'settled empty-list Backspace preserves every blank row per frame',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '- list item¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.pressBackspace();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        final expectedTexts = <String>['- list item', '', '', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        final finalStyles = mounted.paints.last.rows
            .map((row) => row.resolvedBlockStyle)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(
            paint.rows.map((row) => '${row.leadingText}${row.text}'),
            expectedTexts,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
          expect(
            paint.rows.map((row) => row.resolvedBlockStyle),
            finalStyles,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'typing after two Returns preserves the earlier blank block every frame',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Plain text.¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        List<String> texts(FlarkSurfacePaintObservation paint) => paint.rows
            .map((row) => row.text.replaceFirst(RegExp(r'\r?\n$'), ''))
            .toList(growable: false);
        final expectedTexts = <String>['Plain text.', '', 'x', 'sentinel'];
        final finalRects = mounted.paints.last.rows
            .map((row) => row.rect)
            .toList(growable: false);
        for (final paint in mounted.paints) {
          expect(texts(paint), expectedTexts, reason: paint.presentation);
          expect(
            paint.rows.map((row) => row.rect),
            finalRects,
            reason: paint.presentation,
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'structural Return with immediate typing paints one exact lineage',
    (tester) async {
      const cases = <(String, String)>[
        (
          '**sentinel**\n\nParagraph.¦\n1. item\n',
          '**sentinel**\n\nParagraph.\n\nNext¦\n1. item\n',
        ),
        ('**sentinel**\n\n## Heading¦', '**sentinel**\n\n## Heading\n\nNext¦'),
        ('**sentinel**\n\n- item¦', '**sentinel**\n\n- item\n- Next¦'),
        ('**sentinel**\n\n9) item¦', '**sentinel**\n\n9) item\n10) Next¦'),
        (
          '**sentinel**\n\n- [x] done¦',
          '**sentinel**\n\n- [x] done\n- [ ] Next¦',
        ),
        ('**sentinel**\n\n> quote¦', '**sentinel**\n\n> quote\n> Next¦'),
      ];
      for (final (initial, expected) in cases) {
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            initial,
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          await mounted.pressReturn();
          await mounted.typeText('Next');
          await mounted.pumpImmediate();
          await mounted.pumpPresentationSettled();

          await tester.runAsync(() => probe.expectSourceAndCaret(expected));
          expect(mounted.paints, isNotEmpty, reason: initial);
          expect(
            mounted.paints,
            everyElement(
              isA<FlarkSurfacePaintObservation>().having(
                (paint) => paint.presentation,
                'presentation for $initial',
                isNot(contains('**sentinel**')),
              ),
            ),
          );
          await tester.runAsync(probe.expectHealthy);
          await tester.runAsync(probe.expectConvergesWithCleanRebuild);
        } finally {
          await mounted.close();
          await tester.runAsync(probe.close);
        }
      }
    },
    skip: libraryPath == null,
  );

  test(
    'structural receipt and successor cannot retain stale inline styling',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        '> *¦foo*',
        libraryPath: libraryPath!,
      );
      addTearDown(probe.close);
      final publicationStart = probe.publications.length;

      probe.pressReturn();
      probe.typeText(' ');
      await probe.expectSourceAndCaret('> *\n>  ¦foo*');
      await probe.presentationSettled();

      final transitionSamples = probe.publications
          .skip(publicationStart)
          .where(
            (sample) =>
                sample.visibleSource == '> *\n> foo*' ||
                sample.visibleSource == '> *\n>  foo*',
          )
          .toList(growable: false);
      expect(transitionSamples, isNotEmpty);
      for (final sample in transitionSamples) {
        expect(
          sample.rows
              .expand((row) => row.runs)
              .any((run) => run.styles.contains('emphasis')),
          isFalse,
          reason:
              'a block-transition receipt carries no result-revision inline '
              'fact authority',
        );
        expect(
          sample.presentation,
          contains('*'),
          reason: 'invalidated delimiters must stay exact until recertified',
        );
      }
      await probe.expectHealthy();
      await probe.expectConvergesWithCleanRebuild();
    },
    skip: libraryPath == null,
  );

  test('paragraph split never retains stale inline styling', () async {
    final probe = await LiveEditorTransitionProbe.open(
      '*fo¦o*',
      libraryPath: libraryPath!,
    );
    addTearDown(probe.close);
    final publicationStart = probe.publications.length;

    probe.pressReturn();
    await probe.expectSourceAndCaret('*fo\n\n¦o*');
    await probe.presentationSettled();

    final transitionSamples = probe.publications
        .skip(publicationStart)
        .where((sample) => sample.visibleSource == '*fo\n\no*')
        .toList(growable: false);
    expect(transitionSamples, isNotEmpty);
    for (final sample in transitionSamples) {
      expect(
        sample.rows
            .expand((row) => row.runs)
            .any((run) => run.styles.contains('emphasis')),
        isFalse,
        reason:
            'a paragraph-gap receipt carries no result-revision inline '
            'fact authority',
      );
      expect(sample.presentation, contains('*'));
    }
    await probe.expectHealthy();
    await probe.expectConvergesWithCleanRebuild();
  }, skip: libraryPath == null);

  testWidgets(
    'inline split plus same-burst typing never paints a raw block separator',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Before **bold¦**.\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        mounted.paints.clear();
        await mounted.pressReturn();
        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        const expected = 'Before **bold\nx**.\nsentinel';
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>().having(
              (paint) => paint.presentation,
              'presentation',
              expected,
            ),
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
  );

  testWidgets('table line rejoin never flashes raw table source', (
    tester,
  ) async {
    final probe = (await tester.runAsync(
      () => LiveEditorTransitionProbe.open(
        '| a¦ | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n',
        libraryPath: libraryPath!,
      ),
    ))!;
    final mounted = await MountedTransitionRecorder.mount(tester, probe);
    try {
      await mounted.pressReturn();
      await mounted.pumpPresentationSettled();
      mounted.paints.clear();

      await mounted.pressBackspace();
      await mounted.pumpImmediate();
      await mounted.pumpPresentationSettled();

      expect(mounted.paints, isNotEmpty);
      const expected = 'a │ b\nc │ d\nsentinel';
      expect(
        mounted.paints,
        everyElement(
          isA<FlarkSurfacePaintObservation>().having(
            (paint) => paint.presentation,
            'presentation',
            expected,
          ),
        ),
      );
      await tester.runAsync(probe.expectHealthy);
      await tester.runAsync(probe.expectConvergesWithCleanRebuild);
    } finally {
      await mounted.close();
      await tester.runAsync(probe.close);
    }
  }, skip: libraryPath == null);

  testWidgets(
    'mixed-delivery rapid table Returns keep every paint at the rendered caret boundary',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '| a¦ | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        mounted.paints.clear();
        await tester.runAsync(() async {
          final before = probe.controller.inputValue;
          final caret = before.selection.extentOffset;
          probe.controller.updateEditingValue(
            before.copyWith(
              text: before.text.replaceRange(caret, caret, '\n'),
              selection: TextSelection.collapsed(offset: caret + 1),
              composing: TextRange.empty,
            ),
          );
          probe.controller.observePlatformNewlineAction(
            textObservationAlreadyApplied: true,
          );
          probe.controller.observePlatformNewlineAction();
        });
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(probe.controller.globalCaretOffset, 6);
        expect(probe.controller.inputWindowShadow.globalUtf16Start, 6);
        expect(mounted.paints, isNotEmpty);
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>().having(
              (paint) => paint.presentation,
              'presentation',
              '| a\n\n| b |\n| --- | --- |\n| c | d |\nsentinel',
            ),
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
  );

  testWidgets(
    'typing behind an uncertified Return paints only the parser-canonical result',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Café é 👩‍💻¦ text.\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        mounted.paints.clear();
        await tester.runAsync(() async {
          final before = probe.controller.inputValue;
          final caret = before.selection.extentOffset;
          final provisional = before.copyWith(
            text: before.text.replaceRange(caret, caret, '\n'),
            selection: TextSelection.collapsed(offset: caret + 1),
            composing: TextRange.empty,
          );
          probe.controller.updateEditingValue(provisional);
          probe.controller.observePlatformNewlineAction(
            textObservationAlreadyApplied: true,
          );
          probe.controller.applyDeltas([
            TextEditingDeltaInsertion(
              oldText: provisional.text,
              textInserted: 'x',
              insertionOffset: provisional.selection.extentOffset,
              selection: TextSelection.collapsed(
                offset: provisional.selection.extentOffset + 1,
              ),
              composing: TextRange.empty,
            ),
          ]);
        });
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(probe.controller.globalCaretOffset, 17);
        expect(
          await tester.runAsync(probe.controller.readSource),
          'Café é 👩‍💻\n\n xtext.\n\n**sentinel**\n',
        );
        expect(mounted.paints, isNotEmpty);
        final settled = mounted.paints.last.presentation;
        expect(settled, contains('xtext.'));
        expect(settled, isNot(contains('**sentinel**')));
        expect(
          mounted.paints.map((paint) => paint.presentation),
          everyElement(settled),
        );
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Undo during an in-flight heading edit never flashes heading markers',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '## Heading¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        mounted.paints.clear();
        await mounted.replaceSelection('é');
        await mounted.undo();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        for (final paint in mounted.paints) {
          expect(paint.presentation, isNot(contains('##')));
          expect(paint.presentation, isNot(contains('**sentinel**')));
          expect(
            paint.presentation,
            anyOf('Headingé\nsentinel', 'Heading\nsentinel'),
          );
        }
        expect(mounted.paints.last.presentation, 'Heading\nsentinel');
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Undo behind an in-flight Return publishes one settled semantic state',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'Plain text¦.\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        mounted.paints.clear();
        await tester.runAsync(() async {
          final before = probe.controller.inputValue;
          final provisional = before.copyWith(
            text: before.text.replaceRange(10, 10, '\n'),
            selection: const TextSelection.collapsed(offset: 11),
            composing: TextRange.empty,
          );
          probe.controller.updateEditingValue(provisional);
          probe.controller.observePlatformNewlineAction(
            textObservationAlreadyApplied: true,
          );
        });
        expect(await tester.runAsync(probe.controller.undo), isTrue);
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        expect(
          mounted.paints.map((paint) => paint.presentation),
          everyElement('Plain text.\nsentinel'),
        );
        expect(probe.controller.pendingEdits, 0);
        expect(
          probe.controller.debugPublicationCertificationBarrierActive,
          isFalse,
        );
        expect(probe.controller.debugSemanticEditV1Active, isTrue);
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Return behind an uncertified fence edit paints only canonical prefixes',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '```dart\nfinal value = 1;¦\n```\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        await mounted.pressDelete();
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await tester.runAsync(() async {
          final beforeEmoji = probe.controller.inputValue;
          probe.controller.applyDeltas([
            TextEditingDeltaInsertion(
              oldText: beforeEmoji.text,
              textInserted: '👩‍💻',
              insertionOffset: 25,
              selection: const TextSelection.collapsed(offset: 30),
              composing: TextRange.empty,
            ),
          ]);
          final beforeReturn = probe.controller.inputValue;
          probe.controller.updateEditingValue(
            beforeReturn.copyWith(
              text: beforeReturn.text.replaceRange(30, 30, '\n'),
              selection: const TextSelection.collapsed(offset: 31),
              composing: TextRange.empty,
            ),
          );
          probe.controller.observePlatformNewlineAction(
            textObservationAlreadyApplied: true,
          );
        });
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        expect(
          mounted.paints.map((paint) => paint.presentation),
          everyElement(
            anyOf(
              'final value = 1;\nsentinel',
              'final value = 1;\n👩‍💻\n\nsentinel',
            ),
          ),
        );
        expect(
          await tester.runAsync(probe.controller.readSource),
          '```dart\nfinal value = 1;\n👩‍💻\n```\n\n**sentinel**\n',
        );
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'table split then list Return never duplicates the table tail',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '| a¦ | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        await mounted.typeText('*');
        await mounted.pumpPresentationSettled();
        mounted.paints.clear();

        await mounted.pressReturn();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        const expected =
            '| a\n'
            '* \n'
            '* | b |\n'
            '| --- | --- |\n'
            '| c | d |\n'
            'sentinel';
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>().having(
              (paint) => paint.presentation,
              'presentation',
              expected,
            ),
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
  );

  testWidgets(
    'Delete after platform-delivered table Returns preserves hidden syntax',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '| a¦ | b |\n| --- | --- |\n| c | d |\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();
        final beforeDelete = await tester.runAsync(probe.controller.readSource);
        mounted.paints.clear();

        await mounted.pressDelete();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(
          await tester.runAsync(probe.controller.readSource),
          beforeDelete,
        );
        expect(probe.controller.globalCaretOffset, 6);
        for (final paint in mounted.paints) {
          expect(
            paint.presentation,
            '| a\n\n| b |\n| --- | --- |\n| c | d |\nsentinel',
          );
        }
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'active heading trailing space remains painted after certification',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '## Heading¦\n\n**sentinel**\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.typeText(' ');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>().having(
              (paint) => paint.presentation,
              'presentation',
              'Heading \nsentinel',
            ),
          ),
        );
        expect(probe.controller.globalCaretOffset, 11);
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'projected heading start retains its hidden-prefix source identity',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '# Flark dogfood\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final paints = <FlarkSurfacePaintObservation>[];
      try {
        await tester.runAsync(controller.continueParsing);
        await tester.binding.setSurfaceSize(const Size(360, 420));
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: FlarkEditor(
              controller: controller,
              autofocus: true,
              debugPaintObserver: paints.add,
            ),
          ),
        );
        await tester.pump();
        await tester.runAsync(controller.debugWaitForPresentationSettled);
        await tester.pump();

        expect(controller.globalSelectionExtent, 0);
        expect(paints, isNotEmpty);
        expect(paints.last.canonicalSelectionExtentUtf16, 0);
        expect(paints.last.caretSourceUtf16, 0);
        expect(controller.lastError, isNull);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.binding.setSurfaceSize(null);
        await tester.runAsync(controller.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'sustained editing keeps the painted caret at its source offset',
    (tester) async {
      const marked = '''# Flark dogfood

¦This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

# Start here
''';
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(marked, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(
        tester,
        probe,
        size: const Size(640, 600),
      );
      try {
        await mounted.typeTextAndPumpEachCharacter('keepwhat');
        await mounted.pressReturn();
        await mounted.pumpPresentationSettled();

        final beforeLaterEdit = await tester.runAsync(
          probe.controller.readSource,
        );
        final laterCaret =
            beforeLaterEdit!.indexOf('locally.') + 'locally.'.length;
        await mounted.moveCaret(laterCaret);
        const successor = ' Testing is somewhat useful but lik';
        await mounted.typeTextAndPumpEachCharacter(successor);
        await mounted.pumpPresentationSettled();

        final expectedCaret = laterCaret + successor.length;
        expect(
          await tester.runAsync(probe.controller.readSource),
          beforeLaterEdit.replaceRange(laterCaret, laterCaret, successor),
        );
        expect(probe.controller.globalCaretOffset, expectedCaret);
        expect(mounted.paints.last.caretSourceUtf16, expectedCaret);
        await tester.runAsync(probe.expectHealthy);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'mixed Markdown typing stays source-exact across wraps and recertification',
    (tester) async {
      const marked = '''**sentinel**

¦A plain paragraph that is already long enough to wrap on a narrow surface.
''';
      final initial = MarkedSource.parse(marked);
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(marked, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(
        tester,
        probe,
        size: const Size(360, 420),
      );
      try {
        const inserted =
            'Rapid **bold** and _emphasis_ with `code` and [label](https://a.test). ';
        await mounted.typeTextAndPumpEachCharacter(inserted);
        await mounted.pumpPresentationSettled();

        final expected = initial.source.replaceRange(
          initial.caret,
          initial.caret,
          inserted,
        );
        expect(await tester.runAsync(probe.controller.readSource), expected);
        expect(
          probe.controller.globalCaretOffset,
          initial.caret + inserted.length,
        );
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>().having(
              (paint) => paint.presentation,
              'presentation',
              isNot(contains('**sentinel**')),
            ),
          ),
        );

        final boldCaret = expected.indexOf('bold') + 2;
        await mounted.moveCaret(boldCaret);
        mounted.paints.clear();
        await mounted.typeTextAndPumpEachCharacter('Xy');
        await mounted.pumpPresentationSettled();
        expect(
          await tester.runAsync(probe.controller.readSource),
          expected.replaceRange(boldCaret, boldCaret, 'Xy'),
        );
        expect(probe.controller.globalCaretOffset, boldCaret + 2);
        expect(mounted.paints, isNotEmpty);
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
  );
}

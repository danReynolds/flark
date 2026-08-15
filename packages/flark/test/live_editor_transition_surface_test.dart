import 'dart:io';

import 'package:flark/flark.dart';
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

          expect(mounted.paints, isNotEmpty, reason: '$subject + $character');
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
          expect(
            mounted.paints.last.rows.any(
              (row) => !row.neutral && row.text == 'sentinel',
            ),
            isTrue,
            reason: '$subject + $character demoted the sentinel',
          );

          await mounted.pumpPresentationSettled();
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

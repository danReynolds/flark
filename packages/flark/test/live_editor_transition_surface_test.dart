import 'dart:io';
import 'dart:ui';

import 'package:flark/flark.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'syntax hazard never paints an unrelated certified block as source',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '**sentinel**\n\npla¦in\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.typeText('*');
        await mounted.pumpImmediate();

        expect(mounted.paints, isNotEmpty);
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
        expect(
          mounted.paints.last.rows.any(
            (row) => !row.neutral && row.text == 'sentinel',
          ),
          isTrue,
        );

        await mounted.pumpPresentationSettled();
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
}

import 'dart:io';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'final inline grapheme deletion matrix removes the whole owner',
    () async {
      const cases = <({String source, int caret, bool backward})>[
        (source: 'A *t* Z', caret: 4, backward: true),
        (source: 'A *t* Z', caret: 3, backward: false),
        (source: 'A **t** Z', caret: 5, backward: true),
        (source: 'A **t** Z', caret: 4, backward: false),
        (source: 'A ~~t~~ Z', caret: 5, backward: true),
        (source: 'A ~~t~~ Z', caret: 4, backward: false),
        (source: 'A `t` Z', caret: 4, backward: true),
        (source: 'A `t` Z', caret: 3, backward: false),
        (source: 'A ***t*** Z', caret: 6, backward: true),
        (source: 'A ***t*** Z', caret: 5, backward: false),
        (source: 'A *e\u{301}* Z', caret: 5, backward: true),
        (source: 'A *e\u{301}* Z', caret: 3, backward: false),
        (source: r'A \* Z', caret: 4, backward: true),
        (source: r'A \* Z', caret: 3, backward: false),
      ];
      for (final testCase in cases) {
        final controller = await FlarkEditorController.open(
          testCase.source,
          libraryPath: libraryPath!,
        );
        try {
          await controller.continueParsing();

          final row = controller.rows.single;
          controller.activateRow(row, testCase.caret);
          if (testCase.backward) {
            controller.deleteBackward();
          } else {
            controller.deleteForward();
          }
          await controller.debugWaitForPresentationSettled();

          expect(
            await controller.readSource(),
            'A  Z',
            reason: '${testCase.source} backward=${testCase.backward}',
          );
          expect(controller.globalCaretOffset, 2);
          expect(controller.surfaceRow(controller.rows.single).text, 'A  Z');
          expect(controller.lastError, isNull);
        } finally {
          await controller.close();
        }
      }
    },
    skip: libraryPath == null,
  );

  test(
    'delete-to-empty remains writable for character, batch, and whitespace',
    () async {
      const cases =
          <({String inserted, String source, String presentation, int caret})>[
            (inserted: 'x', source: 'A *x* Z', presentation: 'A x Z', caret: 4),
            (
              inserted: 'xy',
              source: 'A *xy* Z',
              presentation: 'A xy Z',
              caret: 5,
            ),
            (inserted: ' ', source: 'A   Z', presentation: 'A   Z', caret: 3),
          ];
      for (final testCase in cases) {
        final probe = await LiveEditorTransitionProbe.open(
          'A *t¦* Z',
          libraryPath: libraryPath!,
        );
        try {
          probe.pressBackspace();
          probe.typeText(testCase.inserted);
          await probe.presentationSettled();

          expect(await probe.controller.readSource(), testCase.source);
          expect(probe.controller.globalCaretOffset, testCase.caret);
          expect(
            captureControllerSurfaceRows(
              probe.controller,
            ).map((row) => row.text).join(),
            testCase.presentation,
          );
          await probe.expectHealthy();
          await probe.expectConvergesWithCleanRebuild();
        } finally {
          await probe.close();
        }
      }
    },
    skip: libraryPath == null,
  );

  test(
    'explicit selection adoption retires delete-to-empty continuation',
    () async {
      Future<String> run(List<int> selectedCarets) async {
        final probe = await LiveEditorTransitionProbe.open(
          'A *t¦* Z',
          libraryPath: libraryPath!,
        );
        try {
          probe.pressBackspace();
          await probe.presentationSettled();
          for (final caret in selectedCarets) {
            probe.moveCaret(caret);
          }
          probe.typeText('x');
          await probe.presentationSettled();
          await probe.expectConvergesWithCleanRebuild();
          return probe.controller.readSource();
        } finally {
          await probe.close();
        }
      }

      expect(
        await run(const [2]),
        'A x Z',
        reason:
            'clicking the same plain gap selects plain context instead of '
            'reusing the deleted emphasis owner',
      );
      expect(
        await run(const [1, 2]),
        'A x Z',
        reason: 'navigation away and back selects the target context anew',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'batched continuation matches sequential typing and syntax exits safely',
    () async {
      Future<({String source, int caret, String presentation})> run(
        List<String> inserts, {
        String initial = 'A *t¦* Z',
        bool settleEach = false,
      }) async {
        final probe = await LiveEditorTransitionProbe.open(
          initial,
          libraryPath: libraryPath!,
        );
        try {
          probe.pressBackspace();
          for (final insert in inserts) {
            probe.replaceSelection(insert);
            if (settleEach) await probe.presentationSettled();
          }
          await probe.presentationSettled();
          return (
            source: await probe.controller.readSource(),
            caret: probe.controller.globalCaretOffset,
            presentation: captureControllerSurfaceRows(
              probe.controller,
            ).map((row) => row.text).join(),
          );
        } finally {
          await probe.close();
        }
      }

      expect(await run(['xy']), await run(['x', 'y']));
      expect(await run(['xy z']), await run(['x', 'y', ' ', 'z']));
      expect(
        await run(['xy z']),
        await run(['x', 'y', ' ', 'z'], settleEach: true),
      );
      expect(await run(['xy*z']), await run(['x', 'y', '*', 'z']));
      expect(await run(['x\\z']), await run(['x', '\\', 'z']));
      expect(
        await run(['\\'], initial: 'A `t¦` Z'),
        (source: 'A `\\` Z', caret: 4, presentation: 'A \\ Z'),
        reason: 'backslash is literal inside an inline-code owner',
      );
      final nestedEscapeBatch = await run(['xy z'], initial: r'A *\*¦* Z');
      expect(
        nestedEscapeBatch,
        await run(['x', 'y', ' ', 'z'], initial: r'A *\*¦* Z'),
      );
      expect(
        nestedEscapeBatch,
        (source: 'A *xy* z Z', caret: 8, presentation: 'A xy z Z'),
        reason:
            'an atomic escape nested inside emphasis must be deleted without '
            'leaking its backslash into the continued persistent owner',
      );
      expect(
        await run(['*']),
        (source: 'A * Z', caret: 3, presentation: 'A * Z'),
        reason:
            'Markdown-active punctuation must leave the emptied owner rather '
            'than being blindly wrapped into a new delimiter pair',
      );
    },
    skip: libraryPath == null,
  );

  test(
    'intraword owners continue ordinary text and exit punctuation',
    () async {
      Future<({String source, int caret, String presentation})> run(
        List<String> inserts, {
        bool forward = false,
      }) async {
        final probe = await LiveEditorTransitionProbe.open(
          forward ? 'A*¦t*Z' : 'A*t¦*Z',
          libraryPath: libraryPath!,
        );
        try {
          if (forward) {
            probe.pressDelete();
          } else {
            probe.pressBackspace();
          }
          for (final insert in inserts) {
            probe.replaceSelection(insert);
          }
          await probe.presentationSettled();
          await probe.expectConvergesWithCleanRebuild();
          return (
            source: await probe.controller.readSource(),
            caret: probe.controller.globalCaretOffset,
            presentation: captureControllerSurfaceRows(
              probe.controller,
            ).map((row) => row.text).join(),
          );
        } finally {
          await probe.close();
        }
      }

      const continued = (source: 'A*x*Z', caret: 3, presentation: 'AxZ');
      expect(await run(['x']), continued);
      expect(await run(['x'], forward: true), continued);
      expect(await run(['!']), (source: 'A!Z', caret: 2, presentation: 'A!Z'));
      expect(await run(['x!y']), await run(['x', '!', 'y']));
      expect(await run(['x!y']), (
        source: 'A*x*!yZ',
        caret: 6,
        presentation: 'Ax!yZ',
      ));
    },
    skip: libraryPath == null,
  );

  test(
    'ordinary deletion preserves a nonempty inline owner',
    () async {
      final controller = await FlarkEditorController.open(
        'A *ab* Z',
        libraryPath: libraryPath!,
      );
      try {
        await controller.continueParsing();
        controller.activateRow(controller.rows.single, 5);
        controller.deleteBackward();
        await controller.debugWaitForPresentationSettled();

        expect(await controller.readSource(), 'A *a* Z');
        expect(controller.globalCaretOffset, 4);
        expect(controller.surfaceRow(controller.rows.single).text, 'A a Z');
        expect(controller.lastError, isNull);
      } finally {
        await controller.close();
      }
    },
    skip: libraryPath == null,
  );

  test(
    'delete-to-empty intent survives ordered undo and redo',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        'A *t¦* Z',
        libraryPath: libraryPath!,
      );
      try {
        probe.pressBackspace();
        probe.typeText('x');
        await probe.presentationSettled();
        await probe.expectSourceAndCaret('A *x¦* Z');

        await probe.undo();
        await probe.expectSourceAndCaret('A ¦ Z');
        await probe.undo();
        await probe.expectSourceAndCaret('A *t¦* Z');
        await probe.redo();
        await probe.expectSourceAndCaret('A ¦ Z');
        await probe.redo();
        await probe.expectSourceAndCaret('A *x¦* Z');
        await probe.expectHealthy();
        await probe.expectConvergesWithCleanRebuild();
      } finally {
        await probe.close();
      }
    },
    skip: libraryPath == null,
  );

  test(
    'undoing the continuation insertion restores the semantic typing intent',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        'A *t¦* Z',
        libraryPath: libraryPath!,
      );
      try {
        probe.pressBackspace();
        probe.typeText('x');
        await probe.presentationSettled();
        await probe.undo();
        await probe.expectSourceAndCaret('A ¦ Z');

        probe.typeText('y');
        await probe.presentationSettled();
        await probe.expectSourceAndCaret('A *y¦* Z');
      } finally {
        await probe.close();
      }
    },
    skip: libraryPath == null,
  );

  test(
    'rapid continuation typing remains one reversible history group',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        'A *t¦* Z',
        libraryPath: libraryPath!,
      );
      try {
        probe.pressBackspace();
        probe.typeText('xy');
        await probe.presentationSettled();
        await probe.expectSourceAndCaret('A *xy¦* Z');

        await probe.undo();
        await probe.expectSourceAndCaret('A ¦ Z');
        probe.typeText('z');
        await probe.presentationSettled();
        await probe.expectSourceAndCaret('A *z¦* Z');
      } finally {
        await probe.close();
      }
    },
    skip: libraryPath == null,
  );

  test(
    'unsupported single-grapheme inline owners use ordinary deletion',
    () async {
      final controller = await FlarkEditorController.open(
        'A [t](url) Z',
        libraryPath: libraryPath!,
      );
      try {
        await controller.continueParsing();
        controller.activateRow(controller.rows.single, 4);
        controller.deleteBackward();

        expect(controller.visibleSource, 'A [](url) Z');
        await controller.debugWaitForPresentationSettled();
        expect(await controller.readSource(), 'A [](url) Z');
        expect(controller.globalCaretOffset, 3);
        expect(controller.lastError, isNull);
      } finally {
        await controller.close();
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'final emphasized grapheme deletion stays rendered in both directions',
    (tester) async {
      for (final backward in [true, false]) {
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            backward ? 'A *t¦* Z' : 'A *¦t* Z',
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);
        try {
          if (backward) {
            await mounted.pressBackspace();
          } else {
            await mounted.pressDelete();
          }
          await mounted.pumpMutationSettled();
          expect(mounted.paints, isNotEmpty);
          expect(
            mounted.paints,
            everyElement(
              isA<FlarkSurfacePaintObservation>()
                  .having(
                    (paint) => paint.presentation,
                    'delete-only presentation',
                    'A  Z',
                  )
                  .having(
                    (paint) => paint.caretSourceUtf16,
                    'delete-only caret',
                    2,
                  ),
            ),
          );
          expect(
            mounted.paints.every((paint) => !paint.presentation.contains('*')),
            isTrue,
          );

          mounted.paints.clear();
          await mounted.typeText('x');
          await mounted.pumpImmediate();
          await mounted.pumpPresentationSettled();
          expect(mounted.paints, isNotEmpty);
          expect(
            mounted.paints,
            everyElement(
              isA<FlarkSurfacePaintObservation>().having(
                (paint) => paint.presentation,
                'presentation',
                anyOf('A  Z', 'A x Z'),
              ),
            ),
          );
          expect(
            mounted.paints.every((paint) => !paint.presentation.contains('*')),
            isTrue,
          );
          final finalPaint = mounted.paints.last;
          expect(finalPaint.presentation, 'A x Z');
          expect(finalPaint.caretSourceUtf16, 4);
          expect(
            finalPaint.rows
                .expand((row) => row.runs)
                .where((run) => run.text == 'x')
                .single
                .styles,
            contains(FlarkSurfaceInlineStyle.emphasis),
          );
          expect(await tester.runAsync(probe.controller.readSource), 'A *x* Z');
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
}

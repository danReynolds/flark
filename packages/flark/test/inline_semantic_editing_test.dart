import 'dart:io';

import 'package:flark/flark.dart';
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
        (source: 'A ~~t~~ Z', caret: 5, backward: true),
        (source: 'A `t` Z', caret: 4, backward: true),
        (source: 'A ***t*** Z', caret: 6, backward: true),
        (source: 'A *e\u{301}* Z', caret: 5, backward: true),
        (source: r'A \* Z', caret: 4, backward: true),
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
    'delete-to-empty remains writable for character and whitespace',
    () async {
      const cases = <({String inserted, String expected, int caret})>[
        (inserted: 'x', expected: 'A x Z', caret: 3),
        (inserted: ' ', expected: 'A   Z', caret: 3),
      ];
      for (final testCase in cases) {
        final probe = await LiveEditorTransitionProbe.open(
          'A *t¦* Z',
          libraryPath: libraryPath!,
        );
        try {
          probe.pressBackspace();
          await probe.presentationSettled();
          probe.typeText(testCase.inserted);
          await probe.presentationSettled();

          expect(await probe.controller.readSource(), testCase.expected);
          expect(probe.controller.globalCaretOffset, testCase.caret);
          expect(
            captureControllerSurfaceRows(
              probe.controller,
            ).map((row) => row.text).join(),
            testCase.expected,
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
    'final emphasized grapheme deletion never paints Markdown markers',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          'A *t¦* Z',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        await mounted.pressBackspace();
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();

        expect(mounted.paints, isNotEmpty);
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>()
                .having((paint) => paint.presentation, 'presentation', 'A  Z')
                .having((paint) => paint.caretSourceUtf16, 'caret', 2),
          ),
        );
        expect(await tester.runAsync(probe.controller.readSource), 'A  Z');
        await tester.runAsync(probe.expectHealthy);
        await tester.runAsync(probe.expectConvergesWithCleanRebuild);

        mounted.paints.clear();
        await mounted.typeText('x');
        await mounted.pumpImmediate();
        await mounted.pumpPresentationSettled();
        expect(
          mounted.paints,
          everyElement(
            isA<FlarkSurfacePaintObservation>()
                .having((paint) => paint.presentation, 'presentation', 'A x Z')
                .having((paint) => paint.caretSourceUtf16, 'caret', 3),
          ),
        );
        expect(await tester.runAsync(probe.controller.readSource), 'A x Z');
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

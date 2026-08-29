import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  for (final scenario in const [
    (name: 'zero-cadence', delay: Duration.zero),
    (name: 'human-cadence', delay: Duration(milliseconds: 80)),
  ]) {
    testWidgets(
      '${scenario.name} heading typing never drops the projected shell',
      (tester) async {
        const inserted = ' now';
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            '# Test is¦ here\n',
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);

        try {
          var expectedPresentation = 'Test is here';
          var projectedCaret = 'Test is'.length;
          var expectedSourceGeneration = probe.controller.sourceGeneration;
          for (final rune in inserted.runes) {
            final text = String.fromCharCode(rune);
            final paintStart = mounted.paints.length;
            expectedPresentation = expectedPresentation.replaceRange(
              projectedCaret,
              projectedCaret,
              text,
            );
            projectedCaret += text.length;
            expectedSourceGeneration += 1;

            await mounted.typeText(text);
            await mounted.pumpImmediate();
            if (scenario.delay != Duration.zero) {
              var remaining = scenario.delay.inMilliseconds;
              while (remaining > 0) {
                final slice = remaining < 8 ? remaining : 8;
                await tester.runAsync(
                  () => Future<void>.delayed(Duration(milliseconds: slice)),
                );
                await mounted.pumpImmediate();
                remaining -= slice;
              }
            }
            expect(
              mounted.paints.length,
              greaterThan(paintStart),
              reason: 'each input rune must produce an observed paint',
            );
            final stepPaints = mounted.paints.skip(paintStart).toList();
            for (final paint in stepPaints) {
              final activeRows = paint.rows
                  .where((row) => row.active)
                  .toList(growable: false);
              expect(
                activeRows,
                isNotEmpty,
                reason: 'each edit frame must paint the active heading row',
              );
              expect(
                activeRows.every((row) => !row.neutral),
                isTrue,
                reason: 'the active heading shell became a neutral source row',
              );
              expect(
                activeRows.every((row) => row.text == expectedPresentation),
                isTrue,
                reason:
                    'a stale or missing edit frame was painted; expected '
                    '$expectedPresentation, got '
                    '${activeRows.map((row) => row.text).toList()}',
              );
              expect(
                paint.sourceGeneration,
                expectedSourceGeneration,
                reason:
                    'the painted presentation must belong to the accepted '
                    'source generation for this rune',
              );
              expect(
                paint.presentation,
                isNot(contains('# ')),
                reason:
                    'a transient painted frame exposed the heading source '
                    'marker',
              );
              expect(
                paint.caretRect,
                isNotNull,
                reason: 'the visible active edit frame must paint a caret',
              );
              expect(
                paint.caretSourceUtf16,
                paint.canonicalSelectionExtentUtf16,
                reason:
                    'the painted caret must represent the canonical source '
                    'selection in the same frame',
              );
            }
          }

          await mounted.pumpPresentationSettled();
          await tester.runAsync(
            () => probe.expectSourceAndCaret('# Test is now¦ here\n'),
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

  testWidgets(
    'empty heading keeps its projected shell for the first Unicode insertion',
    (tester) async {
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open('# ¦\n', libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);

      try {
        final paintStart = mounted.paints.length;
        final expectedSourceGeneration = probe.controller.sourceGeneration + 1;
        await mounted.typeText('🌍');
        await mounted.pumpImmediate();

        final editPaints = mounted.paints.skip(paintStart).toList();
        expect(editPaints, isNotEmpty);
        for (final paint in editPaints) {
          final activeRows = paint.rows
              .where((row) => row.active)
              .toList(growable: false);
          expect(activeRows, isNotEmpty);
          expect(activeRows.every((row) => !row.neutral), isTrue);
          expect(activeRows.every((row) => row.text == '🌍'), isTrue);
          expect(paint.sourceGeneration, expectedSourceGeneration);
          expect(paint.presentation, isNot(contains('# ')));
          expect(paint.caretRect, isNotNull);
          expect(paint.caretSourceUtf16, paint.canonicalSelectionExtentUtf16);
        }

        await mounted.pumpPresentationSettled();
        await tester.runAsync(() => probe.expectSourceAndCaret('# 🌍¦\n'));
        await tester.runAsync(probe.expectHealthy);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'plain heading deletion keeps its projected shell in every paint',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '# Tes¦t\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);

      try {
        final paintStart = mounted.paints.length;
        final expectedSourceGeneration = probe.controller.sourceGeneration + 1;
        await mounted.pressBackspace();
        await mounted.pumpImmediate();

        final editPaints = mounted.paints.skip(paintStart).toList();
        expect(editPaints, isNotEmpty);
        for (final paint in editPaints) {
          final activeRows = paint.rows
              .where((row) => row.active)
              .toList(growable: false);
          expect(activeRows, isNotEmpty);
          expect(activeRows.every((row) => !row.neutral), isTrue);
          expect(activeRows.every((row) => row.text == 'Tet'), isTrue);
          expect(paint.sourceGeneration, expectedSourceGeneration);
          expect(paint.presentation, isNot(contains('# ')));
          expect(paint.caretRect, isNotNull);
          expect(paint.caretSourceUtf16, paint.canonicalSelectionExtentUtf16);
        }

        await mounted.pumpPresentationSettled();
        await tester.runAsync(() => probe.expectSourceAndCaret('# Te¦t\n'));
        await tester.runAsync(probe.expectHealthy);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'plain heading replacement keeps its projected shell in every paint',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '# T¦est\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);

      try {
        await mounted.selectRange(3, 5);
        final paintStart = mounted.paints.length;
        final expectedSourceGeneration = probe.controller.sourceGeneration + 1;
        await mounted.typeText('🌍');
        await mounted.pumpImmediate();

        final editPaints = mounted.paints.skip(paintStart).toList();
        expect(editPaints, isNotEmpty);
        for (final paint in editPaints) {
          final activeRows = paint.rows
              .where((row) => row.active)
              .toList(growable: false);
          expect(activeRows, isNotEmpty);
          expect(activeRows.every((row) => !row.neutral), isTrue);
          expect(activeRows.every((row) => row.text == 'T🌍t'), isTrue);
          expect(paint.sourceGeneration, expectedSourceGeneration);
          expect(paint.presentation, isNot(contains('# ')));
          expect(paint.caretRect, isNotNull);
          expect(paint.caretSourceUtf16, paint.canonicalSelectionExtentUtf16);
        }

        await mounted.pumpPresentationSettled();
        await tester.runAsync(() => probe.expectSourceAndCaret('# T🌍¦t\n'));
        await tester.runAsync(probe.expectHealthy);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'pending heading continuity never follows a cross-row selection',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          '# Head¦ing\n\nParagraph\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);

      try {
        await mounted.typeText('x');
        await mounted.pumpImmediate();

        final paragraph = probe.controller.rows.last;
        final paragraphOffset =
            probe.controller.visibleSource.indexOf('Paragraph') + 2;
        final paintStart = mounted.paints.length;
        probe.controller.extendSelectionTo(
          paragraphOffset,
          activeOrdinal: paragraph.ordinal,
        );
        await mounted.pumpImmediate();

        final destination = probe.controller.surfaceRow(paragraph);
        expect(destination.kind, 5);
        expect(destination.text, contains('Paragraph'));
        expect(destination.text, isNot(contains('Headxing')));

        final selectionPaints = mounted.paints.skip(paintStart).toList();
        expect(selectionPaints, isNotEmpty);
        for (final paint in selectionPaints) {
          final activeRows = paint.rows
              .where((row) => row.active)
              .toList(growable: false);
          expect(activeRows, isNotEmpty);
          expect(
            activeRows.any((row) => row.text.contains('Paragraph')),
            isTrue,
            reason: 'the destination paragraph must own the selected frame',
          );
          expect(
            activeRows.any((row) => row.text.contains('Headxing')),
            isFalse,
            reason:
                'the source heading continuity surface followed the caret '
                'into another row',
          );
        }
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

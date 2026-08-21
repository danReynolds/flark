import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'invalidating strong keeps the heading shell and unrelated emphasis projected',
    (tester) async {
      const initial = '# **¦left** middle _right_';
      const expected = '# ** ¦left** middle _right_';
      const pendingPresentation = '** left** middle right';
      final expectedCaret = MarkedSource.parse(expected).caret;
      final expectedSource = MarkedSource.parse(expected).source;

      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open(initial, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);

      try {
        final expectedSourceGeneration = probe.controller.sourceGeneration + 1;
        await mounted.typeText(' ');
        await mounted.pumpImmediate();

        expect(
          mounted.paints,
          isNotEmpty,
          reason: 'the construct-invalidating space must produce a paint',
        );
        final immediatePaintCount = mounted.paints.length;
        expect(probe.controller.debugProjectionContinuityActive, isTrue);
        for (final paint in mounted.paints) {
          expect(
            paint.sourceGeneration,
            expectedSourceGeneration,
            reason:
                'each paint must belong to the source generation that inserted '
                'the construct-invalidating space',
          );
          expect(paint.visibleUtf16Start, 0);
          expect(paint.visibleUtf16Length, expectedSource.length);
          expect(
            paint.canonicalSelectionExtentUtf16,
            expectedCaret,
            reason: 'each paint must carry the exact post-edit source caret',
          );
          expect(
            paint.caretSourceUtf16,
            expectedCaret,
            reason: 'each visible caret must own that same source position',
          );
          expect(
            paint.presentation,
            pendingPresentation,
            reason:
                'only the invalidated ** left** dependency island may become '
                'exact while the ATX shell and _right_ stay projected',
          );
        }

        // Do not call the debug presentation barrier here: it invokes
        // continueParsing itself and could conceal a regression in the
        // production edit-cell scheduler. Pump at a sub-vsync cadence while
        // native refresh and parser convergence notify the widget tree.
        var observedPaints = mounted.paints.length;
        var sawCommittedEdit = false;
        for (var tick = 0; tick < 250; tick += 1) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 8)),
          );
          await mounted.pumpImmediate();
          if (probe.controller.pendingEdits == 0) {
            sawCommittedEdit = true;
            expect(
              probe.controller.debugDelayedParseScheduled,
              isFalse,
              reason:
                  'edit-cell recertification must start immediately instead '
                  'of entering the ordinary 32 ms debounce',
            );
          }
          for (final paint in mounted.paints.skip(observedPaints)) {
            expect(paint.sourceGeneration, expectedSourceGeneration);
            expect(paint.visibleUtf16Start, 0);
            expect(paint.visibleUtf16Length, expectedSource.length);
            expect(paint.canonicalSelectionExtentUtf16, expectedCaret);
            expect(paint.caretSourceUtf16, expectedCaret);
            expect(paint.presentation, isNot(contains('# ')));
            expect(paint.presentation, isNot(contains('_right_')));
            expect(paint.presentation, endsWith(' middle right'));
          }
          observedPaints = mounted.paints.length;
          if (sawCommittedEdit && probe.controller.semanticsCurrent) break;
        }
        expect(sawCommittedEdit, isTrue);
        expect(
          probe.controller.semanticsCurrent,
          isTrue,
          reason:
              'production scheduling must recertify without a test-triggered '
              'continueParsing call',
        );
        expect(
          probe.controller.debugProjectionContinuityActive,
          isFalse,
          reason:
              'fresh complete inline facts must supersede the one-shot island',
        );
        await tester.runAsync(() => probe.expectSourceAndCaret(expected));

        for (final paint in mounted.paints) {
          expect(paint.sourceGeneration, expectedSourceGeneration);
          expect(paint.visibleUtf16Start, 0);
          expect(paint.visibleUtf16Length, expectedSource.length);
          expect(paint.canonicalSelectionExtentUtf16, expectedCaret);
          expect(paint.caretSourceUtf16, expectedCaret);
          expect(
            paint.presentation,
            isNot(contains('# ')),
            reason: 'the ATX source marker must never reappear',
          );
          expect(
            paint.presentation,
            isNot(contains('_right_')),
            reason: 'the unrelated emphasis projection must never drop',
          );
          expect(
            paint.presentation,
            endsWith(' middle right'),
            reason:
                'the exact fallback may cover only the invalidated left '
                'dependency island',
          );
        }
        expect(
          mounted.paints.length,
          greaterThanOrEqualTo(immediatePaintCount),
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

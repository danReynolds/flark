import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  for (final cadence in const [
    (name: 'zero-cadence', delay: Duration.zero),
    (name: 'human-cadence', delay: Duration(milliseconds: 80)),
  ]) {
    testWidgets(
      '${cadence.name} invalidating strong keeps only its island exact',
      (tester) async {
        const initial = '# **¦left** middle _right_';
        const expected = '# ** ¦left** middle _right_';
        const pendingPresentation = '** left** middle right';
        final expectedCaret = MarkedSource.parse(expected).caret;
        final expectedSource = MarkedSource.parse(expected).source;

        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            initial,
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await MountedTransitionRecorder.mount(tester, probe);

        try {
          final expectedSourceGeneration =
              probe.controller.sourceGeneration + 1;
          await mounted.typeText(' ');
          await mounted.pumpImmediate();
          expect(
            probe.controller.debugProjectionContinuityActive,
            isTrue,
            reason:
                'the immediate edit frame must use the parser-authored cell',
          );
          var remaining = cadence.delay.inMilliseconds;
          while (remaining > 0) {
            final slice = remaining < 8 ? remaining : 8;
            await tester.runAsync(
              () => Future<void>.delayed(Duration(milliseconds: slice)),
            );
            await mounted.pumpImmediate();
            remaining -= slice;
          }

          expect(
            mounted.paints,
            isNotEmpty,
            reason: 'the construct-invalidating space must produce a paint',
          );
          final immediatePaintCount = mounted.paints.length;
          for (final paint in mounted.paints) {
            _expectIslandPaint(
              paint,
              expectedSource: expectedSource,
              expectedSourceGeneration: expectedSourceGeneration,
              expectedCaret: expectedCaret,
              expectedPresentation: pendingPresentation,
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
              _expectIslandPaint(
                paint,
                expectedSource: expectedSource,
                expectedSourceGeneration: expectedSourceGeneration,
                expectedCaret: expectedCaret,
                expectedPresentation: pendingPresentation,
              );
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
            _expectIslandPaint(
              paint,
              expectedSource: expectedSource,
              expectedSourceGeneration: expectedSourceGeneration,
              expectedCaret: expectedCaret,
              expectedPresentation: pendingPresentation,
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

  testWidgets(
    'paint observer control captures an unsupported exact pending frame',
    (tester) async {
      const initial = 'Before **bo¦ld** after.\n';
      const expected = 'Before **bo*¦ld** after.\n';
      final marked = MarkedSource.parse(expected);
      final probe = (await tester.runAsync(
        () =>
            LiveEditorTransitionProbe.open(initial, libraryPath: libraryPath!),
      ))!;
      final mounted = await MountedTransitionRecorder.mount(tester, probe);
      try {
        final expectedGeneration = probe.controller.sourceGeneration + 1;
        final paintStart = mounted.paints.length;
        await mounted.typeText('*');
        await mounted.pumpImmediate();
        final pending = mounted.paints.skip(paintStart).where((paint) {
          final active = paint.rows.where((row) => row.active).toList();
          return paint.sourceGeneration == expectedGeneration &&
              paint.visibleSource == marked.source &&
              paint.canonicalSelectionExtentUtf16 == marked.caret &&
              paint.caretSourceUtf16 == marked.caret &&
              active.length == 1 &&
              active.single.neutral &&
              active.single.kind == 0 &&
              active.single.text == marked.source;
        });
        expect(
          pending,
          isNotEmpty,
          reason:
              'the evidence lane must observe a real unsupported pending '
              'paint instead of coalescing directly to the settled frame',
        );
        await mounted.pumpPresentationSettled();
        await tester.runAsync(() => probe.expectSourceAndCaret(expected));
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

void _expectIslandPaint(
  FlarkSurfacePaintObservation paint, {
  required String expectedSource,
  required int expectedSourceGeneration,
  required int expectedCaret,
  required String expectedPresentation,
}) {
  expect(paint.sourceGeneration, expectedSourceGeneration);
  expect(paint.visibleUtf16Start, 0);
  expect(paint.visibleUtf16Length, expectedSource.length);
  expect(paint.visibleSource, expectedSource);
  expect(paint.canonicalSelectionBaseUtf16, expectedCaret);
  expect(paint.canonicalSelectionExtentUtf16, expectedCaret);
  expect(paint.caretSourceUtf16, expectedCaret);
  expect(paint.caretDisplayUtf16, 3);
  expect(paint.presentation, expectedPresentation);
  final active = paint.rows.singleWhere((row) => row.active);
  expect(active.neutral, isFalse);
  expect(active.kind, 12);
  expect(active.headingLevel, 1);
  final exactIslandRuns = active.runs.where((run) {
    return run.sourceExact &&
        run.sourceUtf16Start < 11 &&
        run.sourceUtf16End > 2;
  }).toList();
  final exactIslandText = exactIslandRuns.map((run) {
    final overlapStart = run.sourceUtf16Start < 2 ? 2 : run.sourceUtf16Start;
    final overlapEnd = run.sourceUtf16End > 11 ? 11 : run.sourceUtf16End;
    return run.text.substring(
      overlapStart - run.sourceUtf16Start,
      overlapEnd - run.sourceUtf16Start,
    );
  }).join();
  expect(
    exactIslandText,
    '** left**',
    reason: 'the invalidated Strong closure must paint exact and unstyled',
  );
  expect(
    exactIslandRuns.every(
      (run) =>
          run.styles.isEmpty && run.resolvedStyle == active.resolvedBlockStyle,
    ),
    isTrue,
    reason:
        'the exact closure may keep the Heading block style but no stale '
        'inline style',
  );
  expect(
    active.runs.any(
      (run) =>
          run.text == 'right' &&
          run.styles.contains(FlarkSurfaceInlineStyle.emphasis) &&
          run.resolvedStyle.fontStyle == FontStyle.italic,
    ),
    isTrue,
    reason: 'the independent Emphasis fact must stay actually italic',
  );
}

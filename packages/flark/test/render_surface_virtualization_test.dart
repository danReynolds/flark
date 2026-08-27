import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'unchanged visible text layouts are reused across editor notifications',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          'First paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 640,
            height: 400,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      final paintedBefore = surface.debugPaintedFragmentCount;
      expect(paintedBefore, greaterThan(1));

      await tester.runAsync(() async {
        controller.activateRow(
          controller.rows[1],
          controller.rows[1].editableUtf16!.start,
        );
        await controller.resolveCanonicalSelection();
      });
      await tester.pump();
      expect(surface.debugPaintedFragmentCount, paintedBefore);
      expect(surface.debugReusedPainterCount, paintedBefore);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'a giant physical line lays out as bounded fragments',
    (tester) async {
      final giant = List.filled(8 * 1024, 'p').join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '$giant\n\nAfter paragraph.\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 640,
            height: 3000,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();

      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      expect(surface.debugFragmentBudget, 256);
      expect(
        surface.debugMaxFragmentUnits,
        lessThanOrEqualTo(surface.debugFragmentBudget),
      );

      // The active row is separately paint-capped by the controller; the
      // fragmentation property under test needs the giant row passive.
      await tester.runAsync(() async {
        controller.activateRow(
          controller.rows.last,
          controller.rows.last.sourceUtf16.start,
        );
        await controller.resolveCanonicalSelection();
      });
      await tester.pump();

      // Every painter stays inside the fragment budget, and the giant row is
      // fully accounted for: whatever is not laid out is explicitly skipped.
      expect(
        surface.debugMaxFragmentUnits,
        lessThanOrEqualTo(surface.debugFragmentBudget),
      );
      final rowUnits = controller.surfaceRow(controller.rows.first).text.length;
      expect(rowUnits, 8192);
      expect(
        surface.debugPaintedFragmentCount + surface.debugSkippedFragmentCount,
        greaterThanOrEqualTo((rowUnits / surface.debugFragmentBudget).ceil()),
      );

      // The layout budget applies within a row, not only between rows: a
      // giant line does not lay out its whole length for one visible frame.
      expect(surface.debugSkippedFragmentCount, greaterThan(0));
      final shortViewportFragments = surface.debugPaintedFragmentCount;

      // A taller viewport materializes more of the same row. The test surface
      // itself must grow: a SizedBox alone is clamped by the 800x600 default.
      await tester.binding.setSurfaceSize(const Size(640, 12000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pump();
      expect(
        surface.debugPaintedFragmentCount,
        greaterThan(shortViewportFragments),
      );
      expect(
        surface.debugMaxFragmentUnits,
        lessThanOrEqualTo(surface.debugFragmentBudget),
      );

      // Offsets stay monotonic and exact across fragment boundaries.
      final shallow = surface.positionForOffset(const Offset(10, 40));
      final deep = surface.positionForOffset(const Offset(10, 4000));
      expect(shallow, isNotNull);
      expect(deep, isNotNull);
      expect(deep!.globalUtf16Offset, greaterThan(shallow!.globalUtf16Offset));

      // Activating deep inside the giant line places the caret without fault.
      await tester.runAsync(() async {
        controller.activateRow(controller.rows.first, 5000);
        await controller.resolveCanonicalSelection();
      });
      await tester.pump();
      expect(controller.globalCaretOffset, 5000);
      expect(controller.lastError, isNull);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'an internal fragment boundary does not become a visible newline',
    (tester) async {
      final source = '${'a' * 256}h\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 640,
            height: 500,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();

      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      final beforeBoundary = surface.debugLocalPositionForSourceUtf16(255);
      final afterBoundary = surface.debugLocalPositionForSourceUtf16(256);
      expect(beforeBoundary, isNotNull);
      expect(afterBoundary, isNotNull);
      expect(afterBoundary!.dy, closeTo(beforeBoundary!.dy, 0.01));
      expect(afterBoundary.dx, greaterThan(beforeBoundary.dx));

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets('fragment cuts land on grapheme-cluster boundaries', (
    tester,
  ) async {
    // A family emoji is one cluster of 11 UTF-16 units. Padding is chosen so
    // clusters straddle the 2048-unit fragment budget rather than aligning
    // with it.
    const family = '\u{1F468}‍\u{1F469}‍\u{1F467}‍\u{1F466}';
    final buffer = StringBuffer();
    while (buffer.length < 9000) {
      buffer.write(family);
    }
    final line = buffer.toString();
    // A short first paragraph holds the caret so the emoji row stays
    // passive; the active row has its own separate transient cap.
    final controller = (await tester.runAsync(
      () => FlarkEditorController.open(
        'Start.\n\n$line\n',
        libraryPath: libraryPath!,
      ),
    ))!;
    await tester.runAsync(controller.continueParsing);

    await tester.binding.setSurfaceSize(const Size(640, 12000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkEditor(controller: controller),
      ),
    );
    await tester.pump();
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkRenderSurfaceWidget),
    );
    expect(surface.debugPaintedFragmentCount, greaterThan(1));
    // Every cut sits on a cluster boundary, so no fragment begins or ends
    // mid-family.
    for (final boundary in surface.debugFragmentBoundaries) {
      if (boundary == 0 || boundary >= line.length) continue;
      expect(
        boundary % family.length,
        0,
        reason: 'cut at $boundary splits a grapheme cluster',
      );
    }

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(controller.close);
  }, skip: libraryPath == null);

  testWidgets(
    'one oversized grapheme remains one bounded visible fragment',
    (tester) async {
      final cluster = 'a${List<String>.filled(3000, '\u0301').join()}';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          'Start.\n\n$cluster\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);
      await tester.binding.setSurfaceSize(const Size(640, 12000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditor(controller: controller),
        ),
      );
      await tester.pump();

      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      expect(surface.debugMaxFragmentUnits, cluster.length);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'mixed quote transition lays out one truthful pending or certified surface',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '> first\n> second\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 640,
            height: 320,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();

      final row = controller.rows.single;
      controller.activateRow(row, 10);
      await tester.pump();
      await tester.runAsync(() async {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      });
      controller.deleteBackward();
      await tester.pump();
      for (var turn = 0; turn < 8 && controller.pendingEdits != 0; turn += 1) {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 10)),
        );
        await tester.pump();
      }
      expect(controller.pendingEdits, 0);
      expect(controller.visibleSource, '> first\n\nsecond\n');
      await tester.pump();

      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      final presentations = surface.debugPaintedPlan
          .where((entry) => entry.ordinal == row.ordinal)
          .toList(growable: false);
      if (!controller.semanticsCurrent) {
        expect(controller.surfaceRowsFor(row), hasLength(1));
        final pending = controller.surfaceRowsFor(row).single;
        expect(pending.kind, 0);
        expect(pending.leadingText, '> ');
        expect(pending.text, 'first\n\nsecond\n');
        expect(pending.runs, hasLength(1));
        expect(pending.runs.single.sourceExact, isTrue);
        expect(presentations, hasLength(1));
        expect(presentations.single.text, 'first\n\nsecond\n');
        expect(presentations.single.active, isTrue);
      } else {
        // Fast parser convergence may supersede the transitional surface
        // before Flutter receives a frame. In that case the only valid paint
        // is the atomic certified partition; the test must not require the
        // renderer to expose an obsolete pending publication merely to make
        // its timing deterministic.
        expect(controller.rows, hasLength(2));
        expect(
          surface.debugPaintedPlan.map((entry) => entry.text),
          orderedEquals(const ['first', 'second']),
        );
        expect(
          surface.debugPaintedPlan.where((entry) => entry.active),
          hasLength(1),
        );
        expect(
          surface.debugPaintedPlan.singleWhere((entry) => entry.active).text,
          'second',
        );
      }

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'below-fold rows are estimated, not laid out, until scrolled',
    (tester) async {
      final source = List<String>.generate(
        600,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final paints = <FlarkSurfacePaintObservation>[];

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Center(
            child: SizedBox(
              width: 640,
              height: 240,
              child: FlarkEditor(
                controller: controller,
                debugPaintObserver: paints.add,
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      expect(
        surface.debugSkippedRowCount,
        greaterThan(0),
        reason:
            'laidOut=${surface.debugLaidOutRowCount} '
            'rows=${controller.rows.length} fragments='
            '${surface.debugPaintedFragmentCount} size='
            '${surface.debugSurfaceSize} content='
            '${surface.debugContentHeight}',
      );
      expect(
        surface.debugLaidOutRowCount + surface.debugSkippedRowCount,
        controller.rows.length,
      );
      final firstPaint = paints.last;
      expect(firstPaint.completeVisibleSurface, isTrue);
      expect(firstPaint.completeVisiblePlusOverscanSurface, isTrue);
      expect(firstPaint.rows.length, firstPaint.requiredVisibleFragmentCount);
      expect(
        firstPaint.laidOutVisiblePlusOverscanFragmentCount,
        greaterThan(firstPaint.requiredVisibleFragmentCount),
      );
      expect(
        firstPaint.visiblePlusOverscanUtf16End,
        greaterThan(firstPaint.paintedSourceUtf16End!),
      );
      expect(
        firstPaint.visiblePlusOverscanUtf16End,
        lessThan(firstPaint.visibleUtf16Start + firstPaint.visibleUtf16Length),
      );
      final laidOutBefore = surface.debugLaidOutRowCount;

      // Scrolling toward the estimated region materializes it.
      surface.scrollBy(100);
      await tester.pump();
      expect(surface.debugLaidOutRowCount, greaterThan(laidOutBefore));
      final scrolledPaint = paints.last;
      expect(scrolledPaint.completeVisibleSurface, isTrue);
      expect(scrolledPaint.completeVisiblePlusOverscanSurface, isTrue);
      expect(
        scrolledPaint.rows.length,
        scrolledPaint.requiredVisibleFragmentCount,
      );
      expect(
        scrolledPaint.visiblePlusOverscanUtf16End,
        lessThan(
          scrolledPaint.visibleUtf16Start + scrolledPaint.visibleUtf16Length,
        ),
      );

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );
}

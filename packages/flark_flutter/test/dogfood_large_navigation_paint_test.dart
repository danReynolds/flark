import 'dart:async';
import 'dart:io';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/render_surface.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

// The large navigation gates use the exact selectable candidate presets.
// ignore: avoid_relative_lib_imports
import '../example/lib/dogfood_documents.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'Prose 1 MiB pages edit return and undo with truthful paints',
    (tester) async {
      final source = buildDogfoodDocument(DogfoodDocumentPreset.prose1MiB);
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final paints = <FlarkSurfacePaintObservation>[];
      await _mount(tester, controller, paints);
      try {
        final initialGeneration = controller.sourceGeneration;
        final initialSelectionGeneration =
            controller.canonicalSelectionGeneration;
        for (var page = 1; page <= 2; page += 1) {
          final paintStart = paints.length;
          expect(await tester.runAsync(controller.nextViewportPage), isTrue);
          await tester.pump();
          expect(controller.viewportPageIndex, page);
          _expectLargePaints(
            paints.skip(paintStart).toList(growable: false),
            source,
            generation: initialGeneration,
            base: controller.globalSelectionBase,
            extent: controller.globalSelectionExtent,
            expectedPage: page,
          );
        }

        final targetRow = controller.rows.firstWhere(
          (row) =>
              row.editableUtf16 != null &&
              (row.inlineFacts?.any(
                    (fact) => fact.kind == FlarkInlineFactKind.strong,
                  ) ??
                  false),
        );
        final editable = targetRow.editableUtf16!;
        final targetStart = source.indexOf('ordinary', editable.start);
        expect(targetStart, inInclusiveRange(editable.start, editable.end));
        final targetEnd = targetStart + 'ordinary'.length;
        await tester.runAsync(() async {
          controller.activateRow(
            targetRow,
            targetStart,
            selectionExtent: targetEnd,
          );
          await controller.resolveCanonicalSelection();
        });
        await tester.pump();
        expect(controller.globalSelectionBase, targetStart);
        expect(controller.globalSelectionExtent, targetEnd);
        expect(tester.testTextInput.hasAnyClients, isTrue);

        const replacement = 'responsive';
        final expectedSource = source.replaceRange(
          targetStart,
          targetEnd,
          replacement,
        );
        final editGeneration = initialGeneration + 1;
        final editPaintStart = paints.length;
        final before = controller.inputValue;
        tester.testTextInput.updateEditingValue(
          TextEditingValue(
            text: before.text.replaceRange(
              before.selection.start,
              before.selection.end,
              replacement,
            ),
            selection: TextSelection.collapsed(
              offset: before.selection.start + replacement.length,
            ),
          ),
        );
        await _pumpUntil(
          tester,
          () =>
              controller.sourceGeneration == editGeneration &&
              controller.pendingEdits == 0,
        );
        unawaited(controller.continueParsing());
        await _pumpUntil(tester, () => controller.semanticsCurrent);
        await tester.pump();
        final editedCaret = targetStart + replacement.length;
        _expectLargePaints(
          paints
              .skip(editPaintStart)
              .where((paint) => paint.sourceGeneration == editGeneration)
              .toList(growable: false),
          expectedSource,
          generation: editGeneration,
          base: editedCaret,
          extent: editedCaret,
          expectedPage: 2,
          requireStrong: true,
        );

        for (var page = 1; page >= 0; page -= 1) {
          final paintStart = paints.length;
          expect(
            await tester.runAsync(controller.previousViewportPage),
            isTrue,
          );
          await tester.pump();
          expect(controller.viewportPageIndex, page);
          _expectLargePaints(
            paints.skip(paintStart).toList(growable: false),
            expectedSource,
            generation: editGeneration,
            base: editedCaret,
            extent: editedCaret,
            expectedPage: page,
          );
        }

        final undoGeneration = editGeneration + 1;
        final undoPaintStart = paints.length;
        expect(await tester.runAsync(controller.undo), isTrue);
        await tester.pump();
        unawaited(controller.continueParsing());
        await _pumpUntil(
          tester,
          () =>
              controller.sourceGeneration == undoGeneration &&
              controller.pendingEdits == 0 &&
              controller.semanticsCurrent,
        );
        await tester.pump();
        expect(controller.globalSelectionBase, targetStart);
        expect(controller.globalSelectionExtent, targetEnd);
        final undoPaints = paints
            .skip(undoPaintStart)
            .where((paint) => paint.sourceGeneration == undoGeneration)
            .toList(growable: false);
        _expectLargePaints(
          undoPaints,
          source,
          generation: undoGeneration,
          base: targetStart,
          extent: targetEnd,
        );
        expect(undoPaints.last.viewportPageIndex, controller.viewportPageIndex);
        expect(await tester.runAsync(controller.readSource), source);
        expect(
          controller.canonicalSelectionGeneration,
          greaterThan(initialSelectionGeneration),
        );
        expect(controller.lastError, isNull);
        expect(controller.resyncCount, 0);
      } finally {
        await _close(tester, controller);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  testWidgets(
    'Prose 5 MiB scrolls two pages away and back without input drift',
    (tester) async {
      final source = buildDogfoodDocument(DogfoodDocumentPreset.prose5MiB);
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final paints = <FlarkSurfacePaintObservation>[];
      await _mount(tester, controller, paints);
      try {
        final sourceGeneration = controller.sourceGeneration;
        final selectionGeneration = controller.canonicalSelectionGeneration;
        final base = controller.globalSelectionBase;
        final extent = controller.globalSelectionExtent;
        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkRenderSurfaceWidget),
        );

        for (var page = 1; page <= 2; page += 1) {
          final paintStart = paints.length;
          surface.scrollBy(1000000);
          await _pumpUntil(tester, () => controller.viewportPageIndex == page);
          await tester.pump();
          final transitionPaints = paints
              .skip(paintStart)
              .toList(growable: false);
          _expectLargePaints(
            transitionPaints,
            source,
            generation: sourceGeneration,
            base: base,
            extent: extent,
          );
          expect(transitionPaints.last.viewportPageIndex, page);
        }

        for (var page = 1; page >= 0; page -= 1) {
          final paintStart = paints.length;
          surface.scrollBy(-1000000);
          await _pumpUntil(tester, () => controller.viewportPageIndex == page);
          await tester.pump();
          final transitionPaints = paints
              .skip(paintStart)
              .toList(growable: false);
          _expectLargePaints(
            transitionPaints,
            source,
            generation: sourceGeneration,
            base: base,
            extent: extent,
          );
          expect(transitionPaints.last.viewportPageIndex, page);
        }

        expect(controller.sourceGeneration, sourceGeneration);
        expect(controller.canonicalSelectionGeneration, selectionGeneration);
        expect(controller.globalSelectionBase, base);
        expect(controller.globalSelectionExtent, extent);
        expect(await tester.runAsync(controller.readSource), source);
        expect(controller.lastError, isNull);
        expect(controller.resyncCount, 0);
      } finally {
        await _close(tester, controller);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  testWidgets(
    'Dense blocks 1 MiB edits undoes pages and closes without raw headings',
    (tester) async {
      final source = buildDogfoodDocument(
        DogfoodDocumentPreset.denseBlocks1MiB,
      );
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final paints = <FlarkSurfacePaintObservation>[];
      await _mount(tester, controller, paints);
      try {
        final initialGeneration = controller.sourceGeneration;
        final anchor = source.indexOf('Short bounded paragraph 000001.');
        expect(anchor, greaterThanOrEqualTo(0));
        final insertion = anchor + 'Short bounded'.length;
        final row = controller.rows.firstWhere(
          (candidate) =>
              candidate.editableUtf16 != null &&
              candidate.editableUtf16!.start <= insertion &&
              insertion <= candidate.editableUtf16!.end,
        );
        await tester.runAsync(() async {
          controller.activateRow(row, insertion);
          await controller.resolveCanonicalSelection();
        });
        await tester.pump();

        final edited = source.replaceRange(insertion, insertion, 'x');
        final editPaintStart = paints.length;
        final before = controller.inputValue;
        tester.testTextInput.updateEditingValue(
          before.copyWith(
            text: before.text.replaceRange(
              before.selection.start,
              before.selection.end,
              'x',
            ),
            selection: TextSelection.collapsed(
              offset: before.selection.start + 1,
            ),
          ),
        );
        await _pumpUntil(
          tester,
          () =>
              controller.sourceGeneration == initialGeneration + 1 &&
              controller.pendingEdits == 0,
        );
        unawaited(controller.continueParsing());
        await _pumpUntil(tester, () => controller.semanticsCurrent);
        await tester.pump();
        _expectLargePaints(
          paints
              .skip(editPaintStart)
              .where((paint) => paint.sourceGeneration == initialGeneration + 1)
              .toList(growable: false),
          edited,
          generation: initialGeneration + 1,
          base: insertion + 1,
          extent: insertion + 1,
          forbiddenMarkers: const ['### '],
        );

        final undoPaintStart = paints.length;
        expect(await tester.runAsync(controller.undo), isTrue);
        await tester.pump();
        await _pumpUntil(
          tester,
          () =>
              controller.sourceGeneration == initialGeneration + 2 &&
              controller.pendingEdits == 0 &&
              controller.semanticsCurrent,
        );
        _expectLargePaints(
          paints
              .skip(undoPaintStart)
              .where((paint) => paint.sourceGeneration == initialGeneration + 2)
              .toList(growable: false),
          source,
          generation: initialGeneration + 2,
          base: insertion,
          extent: insertion,
          forbiddenMarkers: const ['### '],
        );

        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkRenderSurfaceWidget),
        );
        for (var page = 1; page <= 2; page += 1) {
          final paintStart = paints.length;
          surface.scrollBy(1000000);
          await _pumpUntil(tester, () => controller.viewportPageIndex == page);
          await tester.pump();
          final transition = paints.skip(paintStart).toList(growable: false);
          _expectLargePaints(
            transition,
            source,
            generation: initialGeneration + 2,
            base: insertion,
            extent: insertion,
            forbiddenMarkers: const ['### '],
          );
          expect(transition.last.viewportPageIndex, page);
        }
        for (var page = 1; page >= 0; page -= 1) {
          final paintStart = paints.length;
          surface.scrollBy(-1000000);
          await _pumpUntil(tester, () => controller.viewportPageIndex == page);
          await tester.pump();
          final transition = paints.skip(paintStart).toList(growable: false);
          _expectLargePaints(
            transition,
            source,
            generation: initialGeneration + 2,
            base: insertion,
            extent: insertion,
            forbiddenMarkers: const ['### '],
          );
          expect(transition.last.viewportPageIndex, page);
        }
        expect(await tester.runAsync(controller.readSource), source);
        expect(controller.lastError, isNull);
        expect(controller.resyncCount, 0);
      } finally {
        await _close(tester, controller);
      }
      expect(
        FlarkNativeDocument.inspectGlobalLiveState(
          libraryPath: libraryPath,
        ).isEmpty,
        isTrue,
      );
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}

Future<void> _mount(
  WidgetTester tester,
  FlarkEditorController controller,
  List<FlarkSurfacePaintObservation> paints,
) async {
  await tester.binding.setSurfaceSize(const Size(720, 520));
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: SizedBox.expand(
        child: FlarkEditor(
          controller: controller,
          autofocus: true,
          padding: EdgeInsets.zero,
          debugPaintObserver: paints.add,
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.pump();
  expect(paints, isNotEmpty);
}

Future<void> _close(
  WidgetTester tester,
  FlarkEditorController controller,
) async {
  await tester.pumpWidget(const SizedBox.shrink());
  await tester.binding.setSurfaceSize(null);
  await tester.runAsync(controller.close);
}

void _expectLargePaints(
  List<FlarkSurfacePaintObservation> paints,
  String source, {
  required int generation,
  required int base,
  required int extent,
  int? expectedPage,
  bool requireStrong = false,
  List<String> forbiddenMarkers = const ['**'],
}) {
  expect(paints, isNotEmpty);
  var sawStrong = false;
  for (final paint in paints) {
    expect(paint.sourceGeneration, generation);
    if (expectedPage != null) {
      expect(paint.viewportPageIndex, expectedPage);
    }
    expect(paint.canonicalSelectionBaseUtf16, base);
    expect(paint.canonicalSelectionExtentUtf16, extent);
    final visibleEnd = paint.visibleUtf16Start + paint.visibleUtf16Length;
    expect(paint.visibleUtf16Start, inInclusiveRange(0, source.length));
    expect(
      visibleEnd,
      inInclusiveRange(paint.visibleUtf16Start, source.length),
    );
    expect(
      paint.visibleSource,
      source.substring(paint.visibleUtf16Start, visibleEnd),
    );
    for (final marker in forbiddenMarkers) {
      expect(paint.presentation, isNot(contains(marker)));
    }
    for (final row in paint.rows) {
      expect(
        row.sourceUtf16Start,
        greaterThanOrEqualTo(paint.visibleUtf16Start),
      );
      for (final run in row.runs) {
        expect(
          run.sourceUtf16Start,
          greaterThanOrEqualTo(paint.visibleUtf16Start),
        );
        expect(run.sourceUtf16End, lessThanOrEqualTo(visibleEnd));
        if (run.text == 'Flark' &&
            run.styles.contains(FlarkSurfaceInlineStyle.strong)) {
          sawStrong = true;
        }
      }
    }
    if (paint.caretRect != null) {
      expect(paint.caretSourceUtf16, extent);
      expect(paint.caretDisplayUtf16, isNotNull);
    }
  }
  if (requireStrong) expect(sawStrong, isTrue);
}

Future<void> _pumpUntil(WidgetTester tester, bool Function() predicate) async {
  final deadline = DateTime.now().add(const Duration(seconds: 10));
  while (!predicate() && DateTime.now().isBefore(deadline)) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 2)),
    );
    await tester.pump(const Duration(milliseconds: 2));
  }
  expect(predicate(), isTrue);
}

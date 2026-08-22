import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

// The acceptance fixture imports the app's real default source so the moving
// surface cannot drift from the candidate Dan will dogfood.
// ignore: avoid_relative_lib_imports
import '../example/lib/dogfood_documents.dart';
import 'support/live_editor_transition_probe.dart';

final _productTourSource = buildDogfoodDocument(
  DogfoodDocumentPreset.productTour,
);
const _productTourParagraph =
    '''This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.''';
final _paragraphStart = _productTourSource.indexOf(_productTourParagraph);
final _paragraphEnd = _paragraphStart + _productTourParagraph.length;

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'Product Tour keyboard navigation paints current caret geometry',
    (tester) async {
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, _paragraphStart),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
      try {
        final dynamic state = tester.state(find.byType(FlarkEditor));
        final generation = probe.controller.sourceGeneration;

        await mounted.performSelector(
          state,
          'moveRight:',
          expectedBase: _paragraphStart + 1,
          expectedExtent: _paragraphStart + 1,
        );
        await mounted.performSelector(
          state,
          'moveLeft:',
          expectedBase: _paragraphStart,
          expectedExtent: _paragraphStart,
        );
        await mounted.performSelector(
          state,
          'moveWordRight:',
          expectedBase: _paragraphStart + 4,
          expectedExtent: _paragraphStart + 4,
        );
        await mounted.performSelector(
          state,
          'moveWordLeft:',
          expectedBase: _paragraphStart,
          expectedExtent: _paragraphStart,
        );

        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkRenderSurfaceWidget),
        );
        final preferredX = surface.localXForSourceUtf16(_paragraphStart);
        final down = surface.verticalHit(
          _paragraphStart,
          forward: true,
          preferredX: preferredX,
        );
        expect(down, isNotNull);
        await mounted.performSelector(
          state,
          'moveDown:',
          expectedBase: down!.globalUtf16Offset,
          expectedExtent: down.globalUtf16Offset,
        );
        final up = surface.verticalHit(
          down.globalUtf16Offset,
          forward: false,
          preferredX: preferredX,
        );
        expect(up, isNotNull);
        await mounted.performSelector(
          state,
          'moveUp:',
          expectedBase: up!.globalUtf16Offset,
          expectedExtent: up.globalUtf16Offset,
        );

        expect(probe.controller.sourceGeneration, generation);
        expect(probe.controller.visibleSource, _productTourSource);
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
    'Product Tour Shift navigation paints an exact cross-row selection',
    (tester) async {
      final start = _paragraphEnd - 2;
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, start),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
      try {
        final dynamic state = tester.state(find.byType(FlarkEditor));
        await mounted.performSelector(
          state,
          'moveRightAndModifySelection:',
          expectedBase: start,
          expectedExtent: start + 1,
        );

        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkRenderSurfaceWidget),
        );
        final preferredX = surface.localXForSourceUtf16(start + 1);
        final down = surface.verticalHit(
          start + 1,
          forward: true,
          preferredX: preferredX,
        );
        expect(down, isNotNull);
        expect(down!.globalUtf16Offset, greaterThanOrEqualTo(_paragraphEnd));
        await mounted.performSelector(
          state,
          'moveDownAndModifySelection:',
          expectedBase: start,
          expectedExtent: down.globalUtf16Offset,
        );
        await mounted.performSelector(
          state,
          'moveRight:',
          expectedBase: down.globalUtf16Offset,
          expectedExtent: down.globalUtf16Offset,
        );

        expect(probe.controller.visibleSource, _productTourSource);
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
    'Product Tour resize keeps wrapping and painted caret source current',
    (tester) async {
      final caret = _paragraphEnd - 1;
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, caret),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(
        tester,
        probe,
        size: const Size(1569, 906),
      );
      try {
        final initialCaret = mounted.paints.last.caretRect;
        mounted.paints.clear();
        await tester.binding.setSurfaceSize(const Size(1000, 700));
        await tester.pump();
        mounted.expectPaints(
          mounted.paints,
          expectedBase: caret,
          expectedExtent: caret,
        );
        final compactCaret = mounted.paints.last.caretRect;
        expect(compactCaret, isNot(initialCaret));

        mounted.paints.clear();
        await tester.binding.setSurfaceSize(const Size(1569, 906));
        await tester.pump();
        mounted.expectPaints(
          mounted.paints,
          expectedBase: caret,
          expectedExtent: caret,
        );
        expect(mounted.paints.last.caretRect, initialCaret);
        await tester.runAsync(probe.expectHealthy);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  for (final sequence in const ['👩‍💻', '🧑🏽‍🚀', '👨‍👩‍👧‍👦']) {
    testWidgets(
      'Product Tour Unicode insertion $sequence paints one grapheme-safe result',
      (tester) async {
        const anchorText = 'Try emoji and joined sequences: ';
        final anchor =
            _productTourSource.indexOf(anchorText) + anchorText.length;
        final probe = (await tester.runAsync(
          () => LiveEditorTransitionProbe.open(
            _marked(_productTourSource, anchor),
            libraryPath: libraryPath!,
          ),
        ))!;
        final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
        try {
          final paintStart = mounted.paints.length;
          final generation = probe.controller.sourceGeneration + 1;
          await mounted.replaceSelection(sequence);
          await tester.pump();
          await tester.runAsync(probe.presentationSettled);
          await tester.pump();
          final expectedSource = _productTourSource.replaceRange(
            anchor,
            anchor,
            sequence,
          );
          final expectedCaret = anchor + sequence.length;
          final paints = mounted.paints
              .skip(paintStart)
              .toList(growable: false);
          mounted.expectPaints(
            paints,
            expectedSource: expectedSource,
            expectedGeneration: generation,
            expectedBase: expectedCaret,
            expectedExtent: expectedCaret,
          );
          expect(
            paints.every(
              (paint) => paint.rows.any(
                (row) => row.active && row.text.contains(sequence),
              ),
            ),
            isTrue,
          );
          await tester.runAsync(
            () => probe.expectSourceAndCaret(
              _marked(expectedSource, expectedCaret),
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
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
}

String _marked(String source, int offset) =>
    source.replaceRange(offset, offset, '¦');

final class _MountedEditorPaintRecorder {
  _MountedEditorPaintRecorder._(this.tester, this.probe);

  static Future<_MountedEditorPaintRecorder> mount(
    WidgetTester tester,
    LiveEditorTransitionProbe probe, {
    Size size = const Size(520, 420),
  }) async {
    await tester.binding.setSurfaceSize(size);
    final recorder = _MountedEditorPaintRecorder._(tester, probe);
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox.expand(
          child: FlarkEditor(
            controller: probe.controller,
            autofocus: true,
            padding: EdgeInsets.zero,
            debugPaintObserver: recorder.paints.add,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkRenderSurfaceWidget),
    );
    for (
      var step = 0;
      recorder.paints.last.caretRect == null && step < 24;
      step += 1
    ) {
      surface.scrollBy(size.height * 0.6);
      await tester.pump();
    }
    recorder.expectPaints(
      [recorder.paints.last],
      expectedBase: probe.controller.globalSelectionBase,
      expectedExtent: probe.controller.globalSelectionExtent,
    );
    return recorder;
  }

  final WidgetTester tester;
  final LiveEditorTransitionProbe probe;
  final List<FlarkSurfacePaintObservation> paints = [];

  Future<void> performSelector(
    dynamic state,
    String selector, {
    required int expectedBase,
    required int expectedExtent,
  }) async {
    final paintStart = paints.length;
    final selectionGeneration = probe.controller.canonicalSelectionGeneration;
    state.performSelector(selector);
    await _pumpUntil(
      tester,
      () => probe.controller.canonicalSelectionGeneration > selectionGeneration,
    );
    expectPaints(
      paints.skip(paintStart).toList(growable: false),
      expectedBase: expectedBase,
      expectedExtent: expectedExtent,
    );
  }

  Future<void> replaceSelection(String replacement) async {
    await tester.runAsync(() async => probe.replaceSelection(replacement));
  }

  void expectPaints(
    List<FlarkSurfacePaintObservation> observed, {
    String? expectedSource,
    int? expectedGeneration,
    required int expectedBase,
    required int expectedExtent,
  }) {
    final source = expectedSource ?? _productTourSource;
    final generation = expectedGeneration ?? probe.controller.sourceGeneration;
    expect(observed, isNotEmpty);
    for (final paint in observed) {
      expect(paint.sourceGeneration, generation);
      expect(paint.visibleSource, source);
      expect(paint.canonicalSelectionBaseUtf16, expectedBase);
      expect(paint.canonicalSelectionExtentUtf16, expectedExtent);
      expect(paint.presentation, isNot(contains('**')));
      if (expectedBase == expectedExtent) {
        expect(paint.selectionRects, isEmpty);
        expect(paint.caretRect, isNotNull);
        expect(paint.caretSourceUtf16, expectedExtent);
        expect(paint.caretDisplayUtf16, isNotNull);
      } else {
        expect(paint.selectionRects, isNotEmpty);
      }
      final strong = paint.rows
          .expand((row) => row.runs)
          .where((run) => run.text == 'Rust → Dart → Flutter')
          .toList(growable: false);
      if (strong.isNotEmpty) {
        expect(
          strong.every(
            (run) => run.styles.contains(FlarkSurfaceInlineStyle.strong),
          ),
          isTrue,
        );
      }
    }
  }

  Future<void> close() async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.binding.setSurfaceSize(null);
  }
}

Future<void> _pumpUntil(WidgetTester tester, bool Function() predicate) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (!predicate() && DateTime.now().isBefore(deadline)) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 1)),
    );
    await tester.pump(const Duration(milliseconds: 1));
  }
  expect(predicate(), isTrue);
}

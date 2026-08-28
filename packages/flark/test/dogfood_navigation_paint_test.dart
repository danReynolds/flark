import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
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

  testWidgets(
    'Product Tour pointer selection cut and undo preserve rendered lineage',
    (tester) async {
      String? clipboardText;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          switch (call.method) {
            case 'Clipboard.setData':
              clipboardText =
                  (call.arguments as Map<Object?, Object?>)['text'] as String?;
              return null;
            case 'Clipboard.getData':
              return <String, Object?>{'text': clipboardText};
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );

      final strongStart = _productTourSource.indexOf('Rust');
      final strongEnd = strongStart + 'Rust'.length;
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, strongStart),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
      try {
        final dynamic state = tester.state(find.byType(FlarkEditor));
        final target = mounted.debugHandle.geometryForSourceUtf16(
          strongStart + 1,
        );
        expect(target, isNotNull);
        final selectionGeneration =
            probe.controller.canonicalSelectionGeneration;
        final paintStart = mounted.paints.length;
        await tester.tapAt(
          target!.globalPosition,
          kind: PointerDeviceKind.mouse,
        );
        await tester.pump(kDoubleTapMinTime + const Duration(milliseconds: 25));
        await tester.tapAt(
          target.globalPosition,
          kind: PointerDeviceKind.mouse,
        );
        await _pumpUntil(
          tester,
          () =>
              probe.controller.canonicalSelectionGeneration >=
                  selectionGeneration + 2 &&
              probe.controller.globalSelectionBase == strongStart &&
              probe.controller.globalSelectionExtent == strongEnd,
        );
        final doubleTapPaints = mounted.paints
            .skip(paintStart)
            .toList(growable: false);
        mounted.expectDynamicSelectionPaints(doubleTapPaints);
        expect(
          doubleTapPaints.where(
            (paint) =>
                paint.canonicalSelectionBaseUtf16 == strongStart &&
                paint.canonicalSelectionExtentUtf16 == strongEnd,
          ),
          isNotEmpty,
        );
        expect(
          probe.controller.visibleSource.substring(strongStart, strongEnd),
          'Rust',
        );

        await tester.pump(kDoubleTapTimeout);
        final dragStart = mounted.debugHandle.geometryForSourceUtf16(
          _paragraphStart,
        );
        expect(dragStart, isNotNull);
        final dragEnd = mounted.debugHandle.geometryForSourceUtf16(
          _paragraphEnd - 1,
        );
        expect(dragEnd, isNotNull);
        final dragGeneration = probe.controller.canonicalSelectionGeneration;
        final dragPaintStart = mounted.paints.length;
        final drag = await tester.startGesture(
          dragStart!.globalPosition,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveTo(dragEnd!.globalPosition);
        await drag.up();
        await _pumpUntil(
          tester,
          () =>
              probe.controller.canonicalSelectionGeneration > dragGeneration &&
              probe.controller.globalSelectionBase !=
                  probe.controller.globalSelectionExtent,
        );
        final dragBase = probe.controller.globalSelectionBase;
        final dragExtent = probe.controller.globalSelectionExtent;
        final selectionStart = math.min(dragBase, dragExtent);
        final selectionEnd = math.max(dragBase, dragExtent);
        expect(
          selectionStart,
          inInclusiveRange(_paragraphStart, _paragraphStart + 1),
        );
        expect(selectionEnd, greaterThan(_paragraphEnd - 8));
        expect(selectionEnd, lessThanOrEqualTo(_paragraphEnd));
        mounted.expectDynamicSelectionPaints(
          mounted.paints.skip(dragPaintStart).toList(growable: false),
        );
        final selectedText = _productTourSource.substring(
          selectionStart,
          selectionEnd,
        );
        expect(
          probe.controller.visibleSource.substring(
            selectionStart,
            selectionEnd,
          ),
          selectedText,
        );

        state.performSelector('copy:');
        await _pumpUntil(tester, () => clipboardText == selectedText);

        final cutGeneration = probe.controller.sourceGeneration + 1;
        final cutPaintStart = mounted.paints.length;
        state.performSelector('cut:');
        final cutSource = _productTourSource.replaceRange(
          selectionStart,
          selectionEnd,
          '',
        );
        await _pumpUntil(
          tester,
          () => probe.controller.sourceGeneration == cutGeneration,
        );
        await _waitForMutationCommitted(tester, probe.controller);
        await tester.pump();
        mounted.expectPaints(
          mounted.paints
              .skip(cutPaintStart)
              .where((paint) => paint.sourceGeneration == cutGeneration)
              .toList(growable: false),
          expectedSource: cutSource,
          expectedGeneration: cutGeneration,
          expectedBase: selectionStart,
          expectedExtent: selectionStart,
        );
        expect(clipboardText, selectedText);

        final undoGeneration = cutGeneration + 1;
        final undoPaintStart = mounted.paints.length;
        await tester.runAsync(probe.undo);
        await tester.pump();
        await tester.runAsync(probe.presentationSettled);
        await tester.pump();
        final undoPaints = mounted.paints
            .skip(undoPaintStart)
            .where((paint) => paint.sourceGeneration == undoGeneration)
            .toList(growable: false);
        mounted.expectPaints(
          undoPaints,
          expectedGeneration: undoGeneration,
          expectedBase: dragBase,
          expectedExtent: dragExtent,
        );
        expect(
          undoPaints.every(
            (paint) => paint.rows
                .expand((row) => row.runs)
                .any(
                  (run) =>
                      run.text == 'Rust → Dart → Flutter' &&
                      run.styles.contains(FlarkSurfaceInlineStyle.strong),
                ),
          ),
          isTrue,
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

  testWidgets(
    'tap-down from pre-command layout cannot retarget a newer edit',
    (tester) async {
      final caret = _paragraphStart;
      final targetOffset = _paragraphStart + 18;
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, caret),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
      try {
        final target = mounted.debugHandle.geometryForSourceUtf16(targetOffset);
        expect(target, isNotNull);
        final gesture = await tester.startGesture(
          target!.globalPosition,
          kind: PointerDeviceKind.mouse,
        );

        await mounted.replaceSelection('x');
        final expectedSource = _productTourSource.replaceRange(
          caret,
          caret,
          'x',
        );
        expect(probe.controller.visibleSource, expectedSource);
        expect(probe.controller.globalCaretOffset, caret + 1);

        await gesture.up();
        await _waitForMutationCommitted(tester, probe.controller);
        await tester.pump();

        expect(probe.controller.globalCaretOffset, caret + 1);
        expect(probe.controller.visibleSource, expectedSource);
        expect(probe.controller.resyncCount, 0);
        expect(probe.controller.lastError, isNull);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'Product Tour focus reconnect accepts one current rendered edit',
    (tester) async {
      final caret = _productTourSource.indexOf('editor path') + 3;
      final expectedSource = _productTourSource.replaceRange(caret, caret, 'x');
      final expectedManifest = (await tester.runAsync(() async {
        final oracle = await LiveEditorTransitionProbe.open(
          _marked(expectedSource, caret + 1),
          libraryPath: libraryPath!,
        );
        try {
          return oracle.semanticManifest;
        } finally {
          await oracle.close();
        }
      }))!;
      final editorFocus = FocusNode();
      final otherFocus = FocusNode();
      final inputEvents = <String>[];
      addTearDown(() {
        editorFocus.dispose();
        otherFocus.dispose();
      });
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, caret),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(
        tester,
        probe,
        focusNode: editorFocus,
        otherFocusNode: otherFocus,
        debugInputEventObserver: inputEvents.add,
      );
      try {
        expect(editorFocus.hasFocus, isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        final originalGeneration = probe.controller.sourceGeneration;
        final originalSelectionGeneration =
            probe.controller.canonicalSelectionGeneration;

        otherFocus.requestFocus();
        await tester.pump();
        expect(editorFocus.hasFocus, isFalse);
        expect(tester.testTextInput.hasAnyClients, isFalse);

        editorFocus.requestFocus();
        await tester.pump();
        expect(editorFocus.hasFocus, isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(
          tester.testTextInput.editingState?['text'],
          probe.controller.inputValue.text,
        );

        tester.testTextInput.closeConnection();
        await tester.pump();
        expect(editorFocus.hasFocus, isFalse);
        expect(
          inputEvents.where((event) => event == 'connection-closed'),
          hasLength(1),
        );

        editorFocus.requestFocus();
        await tester.pump();
        expect(editorFocus.hasFocus, isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        final before = probe.controller.inputValue;
        expect(before.selection.isCollapsed, isTrue);
        final localCaret = before.selection.extentOffset;
        expect(
          probe.controller.globalSelectionExtent,
          caret,
          reason: 'reconnect must not rehome the canonical caret',
        );
        expect(tester.testTextInput.editingState?['text'], before.text);

        final expectedGeneration = originalGeneration + 1;
        final paintStart = mounted.paints.length;
        tester.testTextInput.updateEditingValue(
          TextEditingValue(
            text: before.text.replaceRange(localCaret, localCaret, 'x'),
            selection: TextSelection.collapsed(offset: localCaret + 1),
          ),
        );
        await _pumpUntil(
          tester,
          () =>
              probe.controller.sourceGeneration == expectedGeneration &&
              probe.controller.visibleSource == expectedSource &&
              probe.controller.globalSelectionExtent == caret + 1,
        );
        await _waitForMutationCommitted(tester, probe.controller);
        await tester.pump();

        expect(
          probe.controller.canonicalSelectionGeneration,
          greaterThan(originalSelectionGeneration),
        );
        expect(
          inputEvents.where((event) => event == 'connection-closed'),
          hasLength(1),
        );
        expect(probe.controller.resyncCount, 0);
        unawaited(probe.controller.continueParsing());
        await _pumpUntil(tester, () => probe.controller.semanticsCurrent);
        await tester.pump();
        mounted.expectPaints(
          mounted.paints
              .skip(paintStart)
              .where((paint) => paint.sourceGeneration == expectedGeneration)
              .toList(growable: false),
          expectedSource: expectedSource,
          expectedGeneration: expectedGeneration,
          expectedBase: caret + 1,
          expectedExtent: caret + 1,
        );
        await tester.runAsync(probe.expectHealthy);
        expect(probe.semanticManifest, expectedManifest);
      } finally {
        await mounted.close();
        await tester.runAsync(probe.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  testWidgets(
    'Product Tour canonical Unicode replacement undo and redo stay current',
    (tester) async {
      const decomposed = 'cafe\u0301';
      const precomposed = 'café';
      final start = _productTourSource.indexOf(decomposed);
      final end = start + decomposed.length;
      final replacedSource = _productTourSource.replaceRange(
        start,
        end,
        precomposed,
      );
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, start),
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(() => probe.selectRange(start, end));
      final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
      try {
        final replacementGeneration = probe.controller.sourceGeneration + 1;
        final replacementPaintStart = mounted.paints.length;
        await mounted.replaceSelection(precomposed);
        await tester.pump();
        await tester.runAsync(probe.presentationSettled);
        await tester.pump();
        final replacedCaret = start + precomposed.length;
        mounted.expectPaints(
          mounted.paints
              .skip(replacementPaintStart)
              .where((paint) => paint.sourceGeneration == replacementGeneration)
              .toList(growable: false),
          expectedSource: replacedSource,
          expectedGeneration: replacementGeneration,
          expectedBase: replacedCaret,
          expectedExtent: replacedCaret,
        );
        expect(
          probe.controller.visibleSource.substring(
            start,
            start + precomposed.length,
          ),
          precomposed,
        );

        final undoGeneration = replacementGeneration + 1;
        final undoPaintStart = mounted.paints.length;
        await tester.runAsync(probe.undo);
        await tester.pump();
        await tester.runAsync(probe.presentationSettled);
        await tester.pump();
        mounted.expectPaints(
          mounted.paints
              .skip(undoPaintStart)
              .where((paint) => paint.sourceGeneration == undoGeneration)
              .toList(growable: false),
          expectedGeneration: undoGeneration,
          expectedBase: start,
          expectedExtent: end,
        );
        expect(
          probe.controller.visibleSource.substring(start, end),
          decomposed,
        );

        final redoGeneration = undoGeneration + 1;
        final redoPaintStart = mounted.paints.length;
        await tester.runAsync(probe.redo);
        await tester.pump();
        await tester.runAsync(probe.presentationSettled);
        await tester.pump();
        mounted.expectPaints(
          mounted.paints
              .skip(redoPaintStart)
              .where((paint) => paint.sourceGeneration == redoGeneration)
              .toList(growable: false),
          expectedSource: replacedSource,
          expectedGeneration: redoGeneration,
          expectedBase: replacedCaret,
          expectedExtent: replacedCaret,
        );
        await tester.runAsync(
          () => probe.expectSourceAndCaret(
            _marked(replacedSource, replacedCaret),
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

  testWidgets(
    'Product Tour bidi movement and extension keep source geometry current',
    (tester) async {
      const bidi = 'English العربية עברית English';
      final lineStart = _productTourSource.indexOf(bidi);
      final lineEnd = lineStart + bidi.length;
      final arabicStart = _productTourSource.indexOf('العربية', lineStart);
      final arabicEnd = arabicStart + 'العربية'.length;
      final hebrewStart = _productTourSource.indexOf('עברית', arabicEnd);
      final hebrewEnd = hebrewStart + 'עברית'.length;
      final probe = (await tester.runAsync(
        () => LiveEditorTransitionProbe.open(
          _marked(_productTourSource, lineStart),
          libraryPath: libraryPath!,
        ),
      ))!;
      final mounted = await _MountedEditorPaintRecorder.mount(tester, probe);
      try {
        final dynamic state = tester.state(find.byType(FlarkEditor));
        final sourceGeneration = probe.controller.sourceGeneration;
        var previous = lineStart;
        var sawArabic = false;
        var sawHebrew = false;
        for (var step = 0; step < 48 && !(sawArabic && sawHebrew); step += 1) {
          final paintStart = mounted.paints.length;
          final selectionGeneration =
              probe.controller.canonicalSelectionGeneration;
          state.performSelector('moveRight:');
          await _pumpUntil(
            tester,
            () =>
                probe.controller.canonicalSelectionGeneration >
                selectionGeneration,
          );
          final current = probe.controller.globalSelectionExtent;
          expect(current, isNot(previous));
          expect(current, inInclusiveRange(lineStart, lineEnd));
          mounted.expectPaints(
            mounted.paints.skip(paintStart).toList(growable: false),
            expectedBase: current,
            expectedExtent: current,
          );
          sawArabic =
              sawArabic || (arabicStart <= current && current <= arabicEnd);
          sawHebrew =
              sawHebrew || (hebrewStart <= current && current <= hebrewEnd);
          previous = current;
        }
        expect(sawArabic, isTrue);
        expect(sawHebrew, isTrue);

        final resetGeneration = probe.controller.canonicalSelectionGeneration;
        await tester.runAsync(() async => probe.moveCaret(lineStart));
        await _pumpUntil(
          tester,
          () =>
              probe.controller.canonicalSelectionGeneration > resetGeneration &&
              probe.controller.globalSelectionBase == lineStart &&
              probe.controller.globalSelectionExtent == lineStart,
        );

        var selectedBoth = false;
        previous = lineStart;
        for (var step = 0; step < 48 && !selectedBoth; step += 1) {
          final paintStart = mounted.paints.length;
          final selectionGeneration =
              probe.controller.canonicalSelectionGeneration;
          state.performSelector('moveRightAndModifySelection:');
          await _pumpUntil(
            tester,
            () =>
                probe.controller.canonicalSelectionGeneration >
                selectionGeneration,
          );
          final base = probe.controller.globalSelectionBase;
          final extent = probe.controller.globalSelectionExtent;
          expect(base, lineStart);
          expect(extent, isNot(previous));
          expect(extent, inInclusiveRange(lineStart, lineEnd));
          mounted.expectPaints(
            mounted.paints.skip(paintStart).toList(growable: false),
            expectedBase: base,
            expectedExtent: extent,
          );
          final selectionStart = math.min(base, extent);
          final selectionEnd = math.max(base, extent);
          final selected = _productTourSource.substring(
            selectionStart,
            selectionEnd,
          );
          selectedBoth =
              selected.contains('العربية') && selected.contains('עברית');
          previous = extent;
        }
        expect(selectedBoth, isTrue);
        expect(probe.controller.sourceGeneration, sourceGeneration);
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
    FocusNode? focusNode,
    FocusNode? otherFocusNode,
    ValueChanged<String>? debugInputEventObserver,
  }) async {
    await tester.binding.setSurfaceSize(size);
    final recorder = _MountedEditorPaintRecorder._(tester, probe);
    final editor = FlarkEditor(
      controller: probe.controller,
      autofocus: true,
      focusNode: focusNode,
      padding: EdgeInsets.zero,
      debugHandle: recorder.debugHandle,
      debugPaintObserver: recorder.paints.add,
      debugInputEventObserver: debugInputEventObserver,
    );
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox.expand(
          child: otherFocusNode == null
              ? editor
              : Column(
                  children: [
                    Expanded(child: editor),
                    Focus(
                      focusNode: otherFocusNode,
                      child: const SizedBox(height: 1),
                    ),
                  ],
                ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkRenderSurfaceWidget),
    );
    bool selectionIsVisible() {
      final selection =
          probe.controller.globalSelectionBase ==
          probe.controller.globalSelectionExtent;
      return selection
          ? recorder.paints.last.caretRect != null
          : recorder.paints.last.selectionRects.isNotEmpty;
    }

    for (var step = 0; !selectionIsVisible() && step < 24; step += 1) {
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
  final FlarkEditorDebugHandle debugHandle = FlarkEditorDebugHandle();
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
      expect(
        paint.presentation.replaceAll('**unfinished', ''),
        isNot(contains('**')),
      );
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

  void expectDynamicSelectionPaints(
    List<FlarkSurfacePaintObservation> observed,
  ) {
    expect(observed, isNotEmpty);
    for (final paint in observed) {
      expect(paint.sourceGeneration, probe.controller.sourceGeneration);
      expect(paint.visibleSource, _productTourSource);
      expect(
        paint.presentation.replaceAll('**unfinished', ''),
        isNot(contains('**')),
      );
      expect(
        paint.canonicalSelectionBaseUtf16,
        inInclusiveRange(_paragraphStart, _paragraphEnd),
      );
      expect(
        paint.canonicalSelectionExtentUtf16,
        inInclusiveRange(_paragraphStart, _paragraphEnd),
      );
      if (paint.canonicalSelectionBaseUtf16 ==
          paint.canonicalSelectionExtentUtf16) {
        expect(paint.caretRect, isNotNull);
        expect(paint.selectionRects, isEmpty);
      } else {
        expect(paint.caretRect, isNull);
        expect(paint.selectionRects, isNotEmpty);
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

Future<void> _waitForMutationCommitted(
  WidgetTester tester,
  FlarkEditorController controller,
) async {
  await _pumpUntil(tester, () => controller.pendingEdits == 0);
  expect(controller.lastError, isNull);
}

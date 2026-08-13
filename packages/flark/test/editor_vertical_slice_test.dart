import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'rendered task checkbox toggles without moving the editor selection',
    (tester) async {
      const initial = '- [ ] todo\n\nSelection stays here.\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(initial, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final task = controller.rows.first;
      final paragraph = controller.rows.last;
      await tester.runAsync(() async {
        controller.activateRow(paragraph, paragraph.editableUtf16!.end);
        await controller.resolveCanonicalSelection();
      });
      final selectionBefore = controller.inputValue.selection;
      final caretBefore = controller.globalCaretOffset;
      final debugHandle = FlarkEditorDebugHandle();

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugHandle: debugHandle,
            ),
          ),
        ),
      );
      await tester.pump();
      final checkbox = debugHandle.geometryForTaskCheckboxOrdinal(task.ordinal);
      expect(checkbox, isNotNull);
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.addPointer(location: checkbox!.globalPosition);
      await gesture.down(checkbox.globalPosition);
      await gesture.up();
      await gesture.removePointer();
      await _pumpUntilTransactions(tester, controller);

      expect(controller.visibleSource, '- [x] todo\n\nSelection stays here.\n');
      expect(controller.surfaceRow(controller.rows.first).leadingText, '☑ ');
      expect(controller.globalCaretOffset, caretBefore);
      expect(controller.inputValue.selection, selectionBefore);
      expect(controller.lastError, isNull);

      expect(await tester.runAsync(controller.undo), isTrue);
      await tester.pump();
      expect(controller.visibleSource, initial);
      expect(controller.globalCaretOffset, caretBefore);

      expect(await tester.runAsync(controller.redo), isTrue);
      await tester.pump();
      expect(controller.visibleSource, '- [x] todo\n\nSelection stays here.\n');
      expect(controller.globalCaretOffset, caretBefore);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'visible headings and tasks expose bounded interactive semantics',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '# Heading\n\n- [ ] todo\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      await tester.pump();
      final headingFinder = find.semantics.byLabel('Heading');
      expect(headingFinder, findsOne);
      expect(
        headingFinder.evaluate().single,
        isSemantics(label: 'Heading', isHeader: true),
      );
      final taskFinder = find.semantics.byLabel('todo');
      expect(taskFinder, findsOne);
      final task = taskFinder.evaluate().single;
      expect(
        task,
        isSemantics(
          label: 'todo',
          hasCheckedState: true,
          isChecked: false,
          hasTapAction: true,
        ),
      );

      tester.semantics.tap(taskFinder);
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, '# Heading\n\n- [x] todo\n');
      expect(
        taskFinder.evaluate().single,
        isSemantics(
          label: 'todo',
          hasCheckedState: true,
          isChecked: true,
          hasTapAction: true,
        ),
      );

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
      semantics.dispose();
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'touch task target has a 48 logical pixel interaction extent',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '- [ ] todo\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final debugHandle = FlarkEditorDebugHandle();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugHandle: debugHandle,
            ),
          ),
        ),
      );
      await tester.pump();
      final center = debugHandle.geometryForTaskCheckboxOrdinal(
        controller.rows.single.ordinal,
      );
      expect(center, isNotNull);
      final nearEdge = center!.globalPosition + const Offset(20, 0);
      final gesture = await tester.startGesture(
        nearEdge,
        kind: PointerDeviceKind.touch,
      );
      await gesture.up();
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, '- [x] todo\n');

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'touch taps activate text while touch drags scroll without selecting',
    (tester) async {
      final source = List.generate(
        12,
        (index) => 'Paragraph $index has enough text to paint.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final first = controller.rows.first;
      await tester.runAsync(() async {
        controller.activateRow(first, first.editableUtf16!.start);
        await controller.resolveCanonicalSelection();
      });
      final selectionBeforeScroll = await tester.runAsync(
        controller.resolveCanonicalSelection,
      );
      final debugHandle = FlarkEditorDebugHandle();

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Center(
            child: SizedBox(
              width: 420,
              height: 240,
              child: FlarkEditor(
                controller: controller,
                padding: EdgeInsets.zero,
                debugHandle: debugHandle,
              ),
            ),
          ),
        ),
      );
      await tester.pump();
      final firstPoint = debugHandle.geometryForSourceUtf16(
        first.editableUtf16!.start,
      );
      expect(firstPoint, isNotNull);

      final scroll = await tester.startGesture(
        const Offset(400, 300),
        kind: PointerDeviceKind.touch,
      );
      for (var step = 0; step < 3; step += 1) {
        await scroll.moveBy(const Offset(0, -30));
      }
      await scroll.up();
      await tester.pump();

      final selectionAfterScroll = await tester.runAsync(
        controller.resolveCanonicalSelection,
      );
      expect(selectionAfterScroll?.base, selectionBeforeScroll?.base);
      expect(selectionAfterScroll?.extent, selectionBeforeScroll?.extent);
      final firstPointAfterScroll = debugHandle.geometryForSourceUtf16(
        first.editableUtf16!.start,
      );
      expect(
        firstPointAfterScroll == null ||
            firstPointAfterScroll.globalPosition.dy <
                firstPoint!.globalPosition.dy - 40,
        isTrue,
      );
      expect(await tester.runAsync(controller.readSource), source);

      final visibleRow = controller.rows.firstWhere((row) {
        final editable = row.editableUtf16;
        return editable != null &&
            debugHandle.geometryForSourceUtf16(editable.start) != null;
      });
      final target = visibleRow.editableUtf16!.start + 3;
      final targetPoint = debugHandle.geometryForSourceUtf16(target);
      expect(targetPoint, isNotNull);
      final selectionGeneration = controller.canonicalSelectionGeneration;
      final tap = await tester.startGesture(
        targetPoint!.globalPosition,
        kind: PointerDeviceKind.touch,
      );
      await tap.up();
      await tester.pump();
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > selectionGeneration,
      );
      expect(controller.globalCaretOffset, target);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'keyboard selectors navigate rendered graphemes and preserve source selection',
    (tester) async {
      const source = 'Before **A🌍B** after\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      final start = source.indexOf('A');
      await tester.runAsync(() async {
        controller.activateRow(row, start);
        await controller.resolveCanonicalSelection();
      });

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(controller: controller, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));

      await _performSelectorAndWait(tester, controller, state, 'moveRight:');
      expect(controller.globalCaretOffset, start + 1);
      await _pressKeyAndWait(tester, controller, LogicalKeyboardKey.arrowRight);
      expect(
        controller.globalCaretOffset,
        start + 3,
        reason: 'the emoji is one rendered grapheme but two UTF-16 units',
      );
      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveRightAndModifySelection:',
      );
      expect(controller.globalSelectionBase, start + 3);
      expect(
        controller.globalSelectionExtent,
        start + 6,
        reason: 'the rendered stop after B skips both closing strong markers',
      );
      await _performSelectorAndWait(tester, controller, state, 'moveLeft:');
      expect(controller.globalSelectionBase, start + 3);
      expect(controller.globalSelectionExtent, start + 3);
      await _performSelectorAndWait(tester, controller, state, 'moveLeft:');
      expect(controller.globalCaretOffset, start + 1);
      expect(await tester.runAsync(controller.readSource), source);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'vertical keyboard selectors use painted line geometry',
    (tester) async {
      const source = 'abc\nuvwxyz\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, 2);
        await controller.resolveCanonicalSelection();
      });

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 300,
            child: FlarkEditor(
              controller: controller,
              padding: EdgeInsets.zero,
            ),
          ),
        ),
      );
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));
      await _performSelectorAndWait(tester, controller, state, 'moveDown:');
      expect(controller.globalCaretOffset, 6);
      await _performSelectorAndWait(tester, controller, state, 'moveUp:');
      expect(controller.globalCaretOffset, 2);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'custom surface paints bounded rows and applies input optimistically',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '# Flark\n\nA quick paragraph.\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      await tester.pump();

      expect(find.byType(EditableText), findsNothing);
      expect(find.byType(FlarkEditor), findsOneWidget);

      final before = controller.inputValue;
      final optimisticPaintBudget = Stopwatch()..start();
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'Live ',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 'Live '.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      optimisticPaintBudget.stop();

      expect(controller.inputValue.text, startsWith('Live '));
      expect(
        optimisticPaintBudget.elapsedMicroseconds,
        lessThan(16000),
        reason: 'local input state must be available inside one 16 ms frame',
      );
      expect(controller.pendingEdits, 1);
      await _pumpUntilTransactions(tester, controller);
      expect(controller.revision, 2);
      expect(controller.pendingEdits, 0);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'custom surface advances through bounded native viewport pages',
    (tester) async {
      final source = List<String>.generate(
        600,
        (index) => 'Paragraph $index.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 640,
            height: 240,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();

      expect(controller.canPageForward, isTrue);
      final firstOrdinal = controller.rows.first.ordinal;
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      surface.scrollBy(1000000);
      await tester.runAsync(() async {
        final deadline = DateTime.now().add(const Duration(seconds: 5));
        while (controller.viewportPageIndex == 0 &&
            DateTime.now().isBefore(deadline)) {
          await Future<void>.delayed(const Duration(milliseconds: 5));
        }
      });
      await tester.pump();

      expect(controller.viewportPageIndex, 1);
      expect(controller.rows.first.ordinal, greaterThan(firstOrdinal));
      expect(controller.visibleSource, contains('Paragraph'));

      surface.scrollBy(-1000000);
      await tester.runAsync(() async {
        final deadline = DateTime.now().add(const Duration(seconds: 5));
        while (controller.viewportPageIndex != 0 &&
            DateTime.now().isBefore(deadline)) {
          await Future<void>.delayed(const Duration(milliseconds: 5));
        }
      });
      await tester.pump();
      expect(controller.viewportPageIndex, 0);
      expect(controller.rows.first.ordinal, firstOrdinal);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Mac selectors copy, cut, and paste the exact bounded selection',
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

      const source = 'alpha beta\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      addTearDown(controller.close);
      await tester.runAsync(controller.continueParsing);
      controller.activateRow(controller.rows.first, 0);
      controller.extendSelectionTo(
        5,
        activeOrdinal: controller.rows.first.ordinal,
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      final dynamic state = tester.state(find.byType(FlarkEditor));

      state.performSelector('copy:');
      await _pumpUntil(tester, () => clipboardText == 'alpha');
      expect(clipboardText, 'alpha');

      state.performSelector('cut:');
      await _pumpUntil(tester, () => controller.visibleSource == ' beta\n');
      await _pumpUntilTransactions(tester, controller);
      expect(clipboardText, 'alpha');
      expect(controller.visibleSource, ' beta\n');

      clipboardText = 'pasted';
      state.performSelector('paste:');
      await _pumpUntil(
        tester,
        () => controller.visibleSource == 'pasted beta\n',
      );
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, 'pasted beta\n');
      expect(controller.lastError, isNull);
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'macOS newline delta plus action commits exactly once',
    (tester) async {
      final controller = (await tester.runAsync(
        () =>
            FlarkEditorController.open('9) alpha\n', libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      final dynamic state = tester.state(find.byType(FlarkEditor));
      final revision = controller.revision;
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, row.editableUtf16!.end);
        await controller.resolveCanonicalSelection();
        final before = controller.inputValue;
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: '\n',
            insertionOffset: before.selection.extentOffset,
            selection: TextSelection.collapsed(
              offset: before.selection.extentOffset + 1,
            ),
            composing: TextRange.empty,
          ),
        ]);
        state.performAction(TextInputAction.newline);
        final deadline = DateTime.now().add(const Duration(seconds: 5));
        while (controller.pendingEdits != 0 &&
            DateTime.now().isBefore(deadline)) {
          await Future<void>.delayed(const Duration(milliseconds: 2));
        }
      });
      await tester.pump();
      expect(
        controller.pendingEdits,
        0,
        reason: 'status=${controller.status}; error=${controller.lastError}',
      );
      expect(controller.revision, revision + 1);
      expect(controller.visibleSource, '9) alpha\n10) \n');
      expect(controller.lastError, isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );
}

Future<void> _pumpUntilTransactions(
  WidgetTester tester,
  FlarkEditorController controller,
) async {
  await _pumpUntil(tester, () => controller.pendingEdits == 0);
  expect(
    controller.pendingEdits,
    0,
    reason: 'status=${controller.status}; error=${controller.lastError}',
  );
}

Future<void> _performSelectorAndWait(
  WidgetTester tester,
  FlarkEditorController controller,
  dynamic state,
  String selector,
) async {
  final generation = controller.canonicalSelectionGeneration;
  state.performSelector(selector);
  await _pumpUntil(
    tester,
    () => controller.canonicalSelectionGeneration > generation,
  );
}

Future<void> _pressKeyAndWait(
  WidgetTester tester,
  FlarkEditorController controller,
  LogicalKeyboardKey key,
) async {
  final generation = controller.canonicalSelectionGeneration;
  await tester.sendKeyEvent(key);
  await _pumpUntil(
    tester,
    () => controller.canonicalSelectionGeneration > generation,
  );
}

Future<void> _pumpUntil(WidgetTester tester, bool Function() predicate) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (!predicate() && DateTime.now().isBefore(deadline)) {
    // Native/isolate replies arrive in real time; controller continuations
    // and widget work run on the test binding's fake frame clock. Advance
    // both just as the live event loop would between frames.
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 1)),
    );
    await tester.pump(const Duration(milliseconds: 1));
  }
  expect(predicate(), isTrue);
}

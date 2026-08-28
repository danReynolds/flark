import 'dart:async';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart' as material;
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'rendered task checkbox toggles without moving the editor selection',
    (tester) async {
      const initial = 'Selection stays here.\n\n- [ ] todo\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(initial, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final paragraph = controller.rows.first;
      final task = controller.rows.last;
      await tester.runAsync(() async {
        controller.activateRow(paragraph, paragraph.editableUtf16!.end);
        await controller.resolveCanonicalSelection();
      });
      final selectionBefore = controller.inputValue.selection;
      final caretBefore = controller.globalCaretOffset;
      final debugHandle = FlarkEditorDebugHandle();
      final paints = <FlarkSurfacePaintObservation>[];

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugHandle: debugHandle,
              debugPaintObserver: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      paints.clear();
      final checkbox = debugHandle.geometryForTaskCheckboxOrdinal(task.ordinal);
      expect(checkbox, isNotNull);
      final toggleGeneration = controller.sourceGeneration + 1;
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.addPointer(location: checkbox!.globalPosition);
      await gesture.down(checkbox.globalPosition);
      await gesture.up();
      await gesture.removePointer();
      await _pumpUntilTransactions(tester, controller);

      expect(controller.visibleSource, 'Selection stays here.\n\n- [x] todo\n');
      expect(controller.surfaceRow(controller.rows.last).leadingText, '☑ ');
      expect(controller.globalCaretOffset, caretBefore);
      expect(controller.inputValue.selection, selectionBefore);
      expect(controller.lastError, isNull);
      _expectTaskTogglePaints(
        paints,
        expectedSource: 'Selection stays here.\n\n- [x] todo\n',
        expectedGeneration: toggleGeneration,
        expectedCaret: caretBefore,
      );

      expect(await tester.runAsync(controller.undo), isTrue);
      await tester.pump();
      expect(controller.visibleSource, initial);
      expect(controller.globalCaretOffset, caretBefore);

      expect(await tester.runAsync(controller.redo), isTrue);
      await tester.pump();
      expect(controller.visibleSource, 'Selection stays here.\n\n- [x] todo\n');
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
      final headingFinder = find.semantics.byValue('Heading');
      expect(headingFinder, findsOne);
      expect(
        headingFinder.evaluate().single,
        isSemantics(value: 'Heading', isHeader: true),
      );
      final taskFinder = find.semantics.byValue('todo');
      expect(taskFinder, findsOne);
      final task = taskFinder.evaluate().single;
      expect(
        task,
        isSemantics(
          value: 'todo',
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
          value: 'todo',
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
    'task acknowledgement cannot regress a newer typed source generation',
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
      final sourceCaret = controller.globalCaretOffset;
      final initialGeneration = controller.sourceGeneration;
      final observedGenerations = <int>[initialGeneration];
      void captureGeneration() {
        observedGenerations.add(controller.sourceGeneration);
      }

      controller.addListener(captureGeneration);
      try {
        final toggle = controller.toggleTaskChecked(task);
        final before = controller.inputValue;
        final localCaret = before.selection.extentOffset;
        controller.applyDeltas([
          TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: 'x',
            insertionOffset: localCaret,
            selection: TextSelection.collapsed(offset: localCaret + 1),
            composing: TextRange.empty,
          ),
        ]);
        await _pumpUntilTransactions(tester, controller);
        expect(await tester.runAsync(() => toggle), isTrue);
        await tester.runAsync(controller.continueParsing);

        final taskMarker = initial.indexOf('[ ]') + 1;
        final expectedAfterTyping = initial.replaceRange(
          sourceCaret,
          sourceCaret,
          'x',
        );
        final shiftedTaskMarker =
            taskMarker + (sourceCaret <= taskMarker ? 1 : 0);
        final expected = expectedAfterTyping.replaceRange(
          shiftedTaskMarker,
          shiftedTaskMarker + 1,
          'x',
        );
        expect(controller.visibleSource, expected);
        expect(controller.sourceGeneration, initialGeneration + 2);
        for (var index = 1; index < observedGenerations.length; index += 1) {
          expect(
            observedGenerations[index],
            greaterThanOrEqualTo(observedGenerations[index - 1]),
            reason: 'published source generations must be monotonic',
          );
        }
        expect(controller.lastError, isNull);
      } finally {
        controller.removeListener(captureGeneration);
        await tester.runAsync(controller.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'pending edit cells retain visual shells without stale task actions',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final headingController = (await tester.runAsync(
        () => FlarkEditorController.open(
          '# Heading\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(headingController.continueParsing);
      final heading = headingController.rows.single;
      await tester.runAsync(() async {
        headingController.activateRow(heading, heading.sourceUtf16.start + 2);
        await headingController.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(controller: headingController),
          ),
        ),
      );
      await tester.pump();

      final headingBefore = headingController.inputValue;
      final headingCaret = headingBefore.selection.extentOffset;
      headingController.updateEditingValue(
        TextEditingValue(
          text: headingBefore.text.replaceRange(
            headingCaret,
            headingCaret,
            'x',
          ),
          selection: TextSelection.collapsed(offset: headingCaret + 1),
          composing: TextRange.empty,
        ),
      );
      await _pumpUntilTransactions(tester, headingController);
      final pendingHeading = headingController.surfaceRow(
        headingController.rows.single,
      );
      expect(pendingHeading.kind, 12);
      expect(pendingHeading.headingLevel, 1);
      final pendingHeadingFinder = find.semantics.byValue(pendingHeading.text);
      expect(pendingHeadingFinder, findsOne);
      expect(
        pendingHeadingFinder.evaluate().single,
        isSemantics(value: pendingHeading.text, isHeader: true),
      );

      headingController.commitActiveComposition();
      await _pumpUntilTransactions(tester, headingController);
      await tester.runAsync(headingController.continueParsing);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(headingController.close);

      final taskController = (await tester.runAsync(
        () => FlarkEditorController.open(
          '- [ ] todo\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(taskController.continueParsing);
      final task = taskController.rows.single;
      await tester.runAsync(() async {
        taskController.activateRow(task, task.sourceUtf16.start + 2);
        await taskController.resolveCanonicalSelection();
      });
      final debugHandle = FlarkEditorDebugHandle();
      final taskPaints = <FlarkSurfacePaintObservation>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: taskController,
              debugHandle: debugHandle,
              debugPaintObserver: taskPaints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      expect(
        debugHandle.geometryForTaskCheckboxOrdinal(task.ordinal),
        isNotNull,
      );
      taskPaints.clear();

      final taskBefore = taskController.inputValue;
      final taskCaret = taskBefore.selection.extentOffset;
      final taskGlobalCaret = taskController.globalCaretOffset;
      final taskGeneration = taskController.sourceGeneration + 1;
      final pendingTaskInput = taskBefore.text.replaceRange(
        taskCaret,
        taskCaret,
        'x',
      );
      final expectedPendingTaskSource = taskController.visibleSource
          .replaceRange(taskGlobalCaret, taskGlobalCaret, 'x');
      taskController.updateEditingValue(
        TextEditingValue(
          text: pendingTaskInput,
          selection: TextSelection.collapsed(offset: taskCaret + 1),
          composing: TextRange.empty,
        ),
      );
      await _pumpUntilTransactions(tester, taskController);
      final pendingTask = taskController.surfaceRow(taskController.rows.single);
      expect(pendingTask.kind, isNot(0));
      expect(debugHandle.geometryForTaskCheckboxOrdinal(task.ordinal), isNull);
      expect(taskController.canToggleTaskChecked(task), isFalse);
      expect(
        await tester.runAsync(() => taskController.toggleTaskChecked(task)),
        isFalse,
      );
      expect(taskController.visibleSource, expectedPendingTaskSource);
      final pendingTaskFinder = find.semantics.byValue(pendingTask.text);
      expect(pendingTaskFinder, findsOne);
      expect(
        pendingTaskFinder.evaluate().single,
        isSemantics(value: pendingTask.text),
      );
      _expectPendingTaskPaints(
        taskPaints,
        expectedSource: expectedPendingTaskSource,
        expectedGeneration: taskGeneration,
        expectedCaret: taskGlobalCaret + 1,
      );

      taskController.commitActiveComposition();
      await _pumpUntilTransactions(tester, taskController);
      await tester.runAsync(taskController.continueParsing);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(taskController.close);
      semantics.dispose();
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'editable row semantics map selections and cursor moves through projection',
    (tester) async {
      const source = '**alpha beta**\n';
      final semantics = tester.ensureSemantics();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, row.editableUtf16!.start);
        await controller.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      await tester.pump();
      final editable = find.semantics.byValue('alpha beta');
      expect(editable, findsOne);
      final data = editable.evaluate().single.getSemanticsData();
      expect(data.flagsCollection.isTextField, isTrue);
      expect(data.hasAction(SemanticsAction.setSelection), isTrue);
      expect(
        data.hasAction(SemanticsAction.moveCursorForwardByCharacter),
        isTrue,
      );
      expect(data.hasAction(SemanticsAction.copy), isFalse);

      var generation = controller.canonicalSelectionGeneration;
      tester.semantics.performAction(
        editable,
        SemanticsAction.setSelection,
        args: <String, int>{'base': 0, 'extent': 5},
      );
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
      );
      expect(controller.globalSelectionBase, 2);
      expect(controller.globalSelectionExtent, 7);

      generation = controller.canonicalSelectionGeneration;
      tester.semantics.performAction(
        editable,
        SemanticsAction.setSelection,
        args: <String, int>{'base': 0, 'extent': 0},
      );
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
      );
      generation = controller.canonicalSelectionGeneration;
      tester.semantics.performAction(
        editable,
        SemanticsAction.moveCursorForwardByCharacter,
        args: false,
      );
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
      );
      expect(controller.globalCaretOffset, 3);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
      semantics.dispose();
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'stale semantics geometry cannot retarget a newer edit',
    (tester) async {
      const source = '**alpha beta**\n';
      final semantics = tester.ensureSemantics();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, row.editableUtf16!.start);
        await controller.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      await tester.pump();
      final staleEditable = find.semantics.byValue('alpha beta');
      expect(staleEditable, findsOne);

      final insertionOffset = controller.globalCaretOffset;
      controller.replaceSelection('x');
      final admittedSelection = controller.inputValue.selection;

      tester.semantics.performAction(
        staleEditable,
        SemanticsAction.setSelection,
        args: <String, int>{'base': 0, 'extent': 5},
      );
      expect(controller.inputValue.selection, admittedSelection);
      await _pumpUntilTransactions(tester, controller);

      expect(controller.globalCaretOffset, insertionOffset + 1);
      expect(
        controller.visibleSource,
        source.replaceRange(insertionOffset, insertionOffset, 'x'),
      );

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
      semantics.dispose();
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Tab and Shift-Tab route list indentation through authoritative receipts',
    (tester) async {
      const initial = '- parent\n- child\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(initial, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final child = controller.rows.last;
      await tester.runAsync(() async {
        controller.activateRow(child, child.editableUtf16!.end);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      final paints = <FlarkSurfacePaintObservation>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              autofocus: true,
              debugInputEventObserver: events.add,
              debugPaintObserver: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      paints.clear();

      final indentGeneration = controller.sourceGeneration + 1;
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, '- parent\n  - child\n');
      expect(controller.globalCaretOffset, 18);
      expect(events, contains('shortcut:indent-list'));
      expect(controller.lastError, isNull);
      _expectListActionPaints(
        paints,
        expectedSource: '- parent\n  - child\n',
        expectedGeneration: indentGeneration,
        expectedCaret: 18,
        operation: 'indent list',
      );

      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      paints.clear();
      final outdentGeneration = controller.sourceGeneration + 1;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, initial);
      expect(controller.globalCaretOffset, 16);
      expect(events, contains('shortcut:outdent-list'));
      expect(controller.lastError, isNull);
      _expectListActionPaints(
        paints,
        expectedSource: initial,
        expectedGeneration: outdentGeneration,
        expectedCaret: 16,
        operation: 'outdent list',
      );

      expect(await tester.runAsync(controller.undo), isTrue);
      await tester.pump();
      expect(controller.visibleSource, '- parent\n  - child\n');
      expect(controller.globalCaretOffset, 18);
      expect(await tester.runAsync(controller.redo), isTrue);
      await tester.pump();
      expect(controller.visibleSource, initial);
      expect(controller.globalCaretOffset, 16);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'pending list continuity owns Tab without granting structural edit authority',
    (tester) async {
      const initial = '- parent\n- child\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(initial, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final child = controller.rows.last;
      await tester.runAsync(() async {
        controller.activateRow(child, child.editableUtf16!.end - 1);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      final paints = <FlarkSurfacePaintObservation>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              autofocus: true,
              debugInputEventObserver: events.add,
              debugPaintObserver: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      paints.clear();

      final before = controller.inputValue;
      final caret = before.selection.extentOffset;
      final pendingGeneration = controller.sourceGeneration + 1;
      controller.updateEditingValue(
        TextEditingValue(
          text: before.text.replaceRange(caret, caret, 'x'),
          selection: TextSelection.collapsed(offset: caret + 1),
          composing: TextRange.empty,
        ),
      );
      await _pumpUntilTransactions(tester, controller);

      expect(controller.surfaceRow(controller.rows.last).kind, isNot(0));
      final dynamic state = tester.state(find.byType(FlarkEditor));
      final selectionBeforeTab = controller.globalCaretOffset;
      state.performSelector('insertTab:');
      await tester.pump();
      expect(events, contains('shortcut:indent-list'));
      expect(controller.globalCaretOffset, selectionBeforeTab);
      expect(controller.visibleSource, '- parent\n- chilxd\n');
      _expectPendingListPaints(
        paints,
        expectedSource: '- parent\n- chilxd\n',
        expectedGeneration: pendingGeneration,
        expectedCaret: selectionBeforeTab,
      );

      controller.commitActiveComposition();
      await _pumpUntilTransactions(tester, controller);
      await tester.runAsync(controller.continueParsing);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Tab and Shift-Tab traverse parser-authored table cells',
    (tester) async {
      const source = '| a | b |\n| --- | --- |\n| c | d |\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.singleWhere((row) => row.table != null);
      final dataCells = row.table!.rows.last;
      await tester.runAsync(() async {
        controller.activateRow(row, dataCells.first.contentUtf16.start);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      final paints = <FlarkSurfacePaintObservation>[];

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              autofocus: true,
              debugInputEventObserver: events.add,
              debugPaintObserver: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      expect(
        surface.isTableCellPosition(dataCells.first.contentUtf16.start),
        isTrue,
      );
      expect(
        surface
            .adjacentTableCellHit(
              dataCells.first.contentUtf16.start,
              forward: true,
            )
            ?.globalUtf16Offset,
        dataCells.last.contentUtf16.start,
      );
      final dynamic state = tester.state(find.byType(FlarkEditor));

      paints.clear();
      await _performSelectorAndWait(tester, controller, state, 'insertTab:');
      expect(controller.globalCaretOffset, dataCells.last.contentUtf16.start);
      expect(events, contains('shortcut:next-table-cell'));
      _expectTableNavigationPaints(
        paints,
        expectedSource: source,
        expectedGeneration: controller.sourceGeneration,
        expectedCaret: dataCells.last.contentUtf16.start,
        operation: 'next table cell',
      );

      paints.clear();
      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'insertBacktab:',
      );
      expect(controller.globalCaretOffset, dataCells.first.contentUtf16.start);
      expect(events, contains('shortcut:previous-table-cell'));
      expect(controller.visibleSource, source);
      expect(controller.lastError, isNull);
      _expectTableNavigationPaints(
        paints,
        expectedSource: source,
        expectedGeneration: controller.sourceGeneration,
        expectedCaret: dataCells.first.contentUtf16.start,
        operation: 'previous table cell',
      );

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'pending table cells suppress stale navigation until recertified',
    (tester) async {
      const source = '| a | b |\n| --- | --- |\n| c | d |\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.singleWhere((row) => row.table != null);
      final firstCell = row.table!.rows.last.first;
      await tester.runAsync(() async {
        controller.activateRow(row, firstCell.contentUtf16.start);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              autofocus: true,
              debugInputEventObserver: events.add,
            ),
          ),
        ),
      );
      await tester.pump();

      final before = controller.inputValue;
      final caret = before.selection.extentOffset;
      controller.updateEditingValue(
        TextEditingValue(
          text: before.text.replaceRange(caret, caret, 'xyz'),
          selection: TextSelection.collapsed(offset: caret + 3),
          composing: TextRange.empty,
        ),
      );
      await tester.pump();

      expect(controller.surfaceRow(controller.rows.single).kind, isNot(0));
      expect(controller.debugProjectionContinuityActive, isTrue);
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      final pendingCaret = controller.globalCaretOffset;
      expect(surface.isTableCellPosition(pendingCaret), isFalse);
      expect(surface.adjacentTableCellHit(pendingCaret, forward: true), isNull);

      final dynamic state = tester.state(find.byType(FlarkEditor));
      await _performSelectorAndWait(tester, controller, state, 'insertTab:');
      expect(controller.globalCaretOffset, pendingCaret);
      expect(events, contains('shortcut:pending-table-cell'));

      await _pumpUntilTransactions(tester, controller);
      await tester.runAsync(controller.continueParsing);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'paragraph selectors cross a rendered block without entering markers',
    (tester) async {
      const source = '## alpha beta gamma\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      final editable = row.editableUtf16!;
      await tester.runAsync(() async {
        controller.activateRow(row, source.indexOf('beta') + 2);
        await controller.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(child: FlarkEditor(controller: controller)),
        ),
      );
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));

      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveToBeginningOfParagraph:',
      );
      expect(controller.globalCaretOffset, editable.start);

      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveToEndOfParagraphAndModifySelection:',
      );
      expect(controller.globalSelectionBase, editable.start);
      expect(controller.globalSelectionExtent, editable.end);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
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
        material.MaterialApp(
          home: SizedBox.expand(
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
    'touch long press selects and replaces one rendered styled word',
    (tester) async {
      const source = 'Tap **this** word\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final debugHandle = FlarkEditorDebugHandle();
      await tester.pumpWidget(
        material.MaterialApp(
          home: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugHandle: debugHandle,
            ),
          ),
        ),
      );
      await tester.pump();
      final target = debugHandle.geometryForSourceUtf16(
        source.indexOf('this') + 1,
      );
      expect(target, isNotNull);
      final generation = controller.canonicalSelectionGeneration;

      await tester.longPressAt(target!.globalPosition);
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
      );

      expect(controller.canonicalSelectionGeneration, generation + 1);
      expect(controller.globalSelectionBase, source.indexOf('this'));
      expect(controller.globalSelectionExtent, source.indexOf('this') + 4);
      expect(await tester.runAsync(controller.readSelectedText), 'this');
      expect(await tester.runAsync(controller.readSource), source);
      expect(
        find.byType(material.AdaptiveTextSelectionToolbar),
        findsOneWidget,
      );

      ContextMenuController.removeAny();
      controller.replaceSelection('that');
      await _pumpUntilTransactions(tester, controller);
      expect(
        await tester.runAsync(controller.readSource),
        'Tap **that** word\n',
      );
      expect(controller.lastError, isNull);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'source geometry round trips inside a tall soft-break paragraph',
    (tester) async {
      final source = List.generate(
        40,
        (index) => 'Line ${index.toString().padLeft(2, '0')}\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final debugHandle = FlarkEditorDebugHandle();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              padding: EdgeInsets.zero,
              debugHandle: debugHandle,
            ),
          ),
        ),
      );
      await tester.pump();
      const targetOffset = 4;
      final target = debugHandle.geometryForSourceUtf16(targetOffset);
      expect(target, isNotNull);
      final generation = controller.canonicalSelectionGeneration;

      await tester.tapAt(target!.globalPosition, kind: PointerDeviceKind.mouse);
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
      );

      final selection = await tester.runAsync(
        controller.resolveCanonicalSelection,
      );
      expect(selection?.base, targetOffset);
      expect(selection?.extent, targetOffset);
      expect(await tester.runAsync(controller.readSource), source);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets('mouse double tap selects one rendered styled word', (
    tester,
  ) async {
    const source = 'Tap **this** word\n';
    final controller = (await tester.runAsync(
      () => FlarkEditorController.open(source, libraryPath: libraryPath!),
    ))!;
    await tester.runAsync(controller.continueParsing);
    final debugHandle = FlarkEditorDebugHandle();
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox.expand(
          child: FlarkEditor(controller: controller, debugHandle: debugHandle),
        ),
      ),
    );
    await tester.pump();
    final target = debugHandle.geometryForSourceUtf16(
      source.indexOf('this') + 1,
    );
    expect(target, isNotNull);
    final generation = controller.canonicalSelectionGeneration;

    await tester.tapAt(target!.globalPosition, kind: PointerDeviceKind.mouse);
    await tester.pump(kDoubleTapMinTime + const Duration(milliseconds: 25));
    await tester.tapAt(target.globalPosition, kind: PointerDeviceKind.mouse);
    await _pumpUntil(
      tester,
      () =>
          controller.canonicalSelectionGeneration >= generation + 2 &&
          controller.globalSelectionBase == source.indexOf('this') &&
          controller.globalSelectionExtent == source.indexOf('this') + 4,
    );
    final selection = await tester.runAsync(
      controller.resolveCanonicalSelection,
    );
    expect(selection?.base, source.indexOf('this'));
    expect(selection?.extent, source.indexOf('this') + 4);
    expect(await tester.runAsync(controller.readSelectedText), 'this');
    expect(await tester.runAsync(controller.readSource), source);

    await tester.pump(kDoubleTapTimeout);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(controller.close);
  }, skip: libraryPath == null);

  testWidgets(
    'stale double-tap geometry cannot select a newer source',
    (tester) async {
      const source = 'Tap **this** word\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
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
      final target = debugHandle.geometryForSourceUtf16(
        source.indexOf('this') + 1,
      )!;

      final firstTap = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
      );
      await firstTap.down(target.globalPosition);
      await firstTap.up();
      await tester.pump(kDoubleTapMinTime + const Duration(milliseconds: 25));
      final insertionOffset = source.indexOf('this') + 1;
      expect(controller.globalCaretOffset, insertionOffset);

      controller.replaceSelection('x');
      final admittedSelection = controller.inputValue.selection;

      final secondTap = await tester.createGesture(
        kind: PointerDeviceKind.mouse,
      );
      await secondTap.down(target.globalPosition);
      await secondTap.up();
      expect(controller.inputValue.selection, admittedSelection);
      await _pumpUntilTransactions(tester, controller);

      expect(controller.globalCaretOffset, insertionOffset + 1);
      expect(
        controller.visibleSource,
        source.replaceRange(insertionOffset, insertionOffset, 'x'),
      );

      await tester.pump(kDoubleTapTimeout);
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
    'vertical navigation continues across bounded viewport pages',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(640, 1600));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final source = List<String>.generate(
        96,
        (index) => 'Paragraph ${index.toString().padLeft(3, '0')} text.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final lastRow = controller.rows.last;
      final sourceColumn = lastRow.editableUtf16!.start + 4;
      await tester.runAsync(() async {
        controller.activateRow(lastRow, sourceColumn);
        await controller.resolveCanonicalSelection();
      });

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              padding: EdgeInsets.zero,
            ),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      expect(surface.debugLocalPositionForSourceUtf16(sourceColumn), isNotNull);
      final dynamic state = tester.state(find.byType(FlarkEditor));
      final observedPages = <int>[];
      void recordPage() => observedPages.add(controller.viewportPageIndex);
      controller.addListener(recordPage);
      state.performSelector('moveDown:');
      final firstRow = controller.rows.first;
      final interruptedCaret = firstRow.editableUtf16!.start + 4;
      controller.activateRow(firstRow, interruptedCaret);
      await _pumpUntil(
        tester,
        () => observedPages.contains(1) && controller.viewportPageIndex == 0,
      );
      controller.removeListener(recordPage);
      expect(controller.globalCaretOffset, interruptedCaret);
      expect(controller.rows.first.ordinal, firstRow.ordinal);

      await tester.runAsync(() async {
        controller.activateRow(lastRow, sourceColumn);
        await controller.resolveCanonicalSelection();
      });
      await tester.pump();
      final selectionGeneration = controller.canonicalSelectionGeneration;

      state.performSelector('moveDown:');
      state.performSelector('moveDown:');
      state.performSelector('moveDown:');
      await _pumpUntil(tester, () => controller.viewportPageIndex == 1);
      final pageOneCaret = controller.rows[2].editableUtf16!.start + 4;
      await _pumpUntil(
        tester,
        () =>
            controller.globalCaretOffset == pageOneCaret &&
            controller.canonicalSelectionGeneration >= selectionGeneration + 3,
      );
      expect(controller.globalCaretOffset, pageOneCaret);
      expect(controller.globalSelectionBase, controller.globalSelectionExtent);

      await _performSelectorAndWait(tester, controller, state, 'moveUp:');
      await _performSelectorAndWait(tester, controller, state, 'moveUp:');
      final pageOneFirstCaret = controller.rows.first.editableUtf16!.start + 4;
      expect(controller.globalCaretOffset, pageOneFirstCaret);

      final reverseGeneration = controller.canonicalSelectionGeneration;
      state.performSelector('moveUpAndModifySelection:');
      await _pumpUntil(tester, () => controller.viewportPageIndex == 0);
      final previousLastRow = controller.rows.last;
      final previousLastCaret = previousLastRow.editableUtf16!.start + 4;
      await _pumpUntil(
        tester,
        () =>
            controller.globalSelectionExtent == previousLastCaret &&
            controller.canonicalSelectionGeneration > reverseGeneration,
      );

      expect(controller.globalSelectionBase, pageOneFirstCaret);
      expect(controller.globalSelectionExtent, previousLastCaret);
      expect(await tester.runAsync(controller.readSource), source);
      expect(controller.lastError, isNull);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'vertical navigation keeps each adopted caret visible',
    (tester) async {
      final source = List<String>.generate(
        48,
        (index) => 'Paragraph ${index.toString().padLeft(3, '0')} text.\n\n',
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
            height: 180,
            child: FlarkEditor(
              controller: controller,
              padding: EdgeInsets.zero,
            ),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      final visibleRows = controller.rows
          .where((row) {
            final range = row.editableUtf16;
            return range != null &&
                surface.debugLocalPositionForSourceUtf16(range.start + 4) !=
                    null;
          })
          .toList(growable: false);
      expect(visibleRows.length, greaterThan(1));
      expect(visibleRows.length, lessThan(controller.rows.length));
      final row = visibleRows.last;
      final start = row.editableUtf16!.start + 4;
      await tester.runAsync(() async {
        controller.activateRow(row, start);
        await controller.resolveCanonicalSelection();
      });
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));
      final initialScroll = surface.scrollOffset;

      await _performSelectorAndWait(tester, controller, state, 'moveDown:');
      await tester.pump();
      final firstCaret = controller.globalCaretOffset;
      expect(firstCaret, greaterThan(start));
      expect(surface.scrollOffset, greaterThan(initialScroll));
      expect(surface.debugLocalPositionForSourceUtf16(firstCaret), isNotNull);

      await _performSelectorAndWait(tester, controller, state, 'moveDown:');
      await tester.pump();
      expect(controller.globalCaretOffset, greaterThan(firstCaret));
      expect(
        surface.debugLocalPositionForSourceUtf16(controller.globalCaretOffset),
        isNotNull,
      );
      expect(await tester.runAsync(controller.readSource), source);
      expect(controller.lastError, isNull);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'select-all shortcut installs one exact document-wide selection',
    (tester) async {
      final source = List.generate(
        24,
        (index) => 'Paragraph $index with **rendered text**.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.first;
      await tester.runAsync(() async {
        controller.activateRow(row, row.editableUtf16!.start);
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

      final generation = controller.canonicalSelectionGeneration;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyA);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
      );
      final selection = await tester.runAsync(
        controller.resolveCanonicalSelection,
      );
      expect(selection?.base, 0);
      expect(selection?.extent, source.length);
      expect(await tester.runAsync(controller.readSelectedText), source);
      expect(controller.hasOversizedSelection, isTrue);
      expect(await tester.runAsync(controller.readSource), source);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'line-boundary selectors use rendered lines and source mappings',
    (tester) async {
      const source = 'abc **bold** tail\nsecond line\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      final start = source.indexOf('bold') + 2;
      await tester.runAsync(() async {
        controller.activateRow(row, start);
        await controller.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 500,
            child: FlarkEditor(
              controller: controller,
              padding: EdgeInsets.zero,
            ),
          ),
        ),
      );
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));

      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveToRightEndOfLine:',
      );
      expect(controller.globalCaretOffset, source.indexOf('\n'));
      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveToLeftEndOfLine:',
      );
      expect(controller.globalCaretOffset, 0);
      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveToRightEndOfLineAndModifySelection:',
      );
      expect(controller.globalSelectionBase, 0);
      expect(controller.globalSelectionExtent, source.indexOf('\n'));

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'word selectors use Unicode layout boundaries and skip hidden markers',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      addTearDown(() => debugDefaultTargetPlatformOverride = null);
      const source = 'one **two** three\n';
      final inputEvents = <String>[];
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, 0);
        await controller.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              autofocus: true,
              debugInputEventObserver: inputEvents.add,
            ),
          ),
        ),
      );
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));

      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveWordRight:',
      );
      expect(controller.globalCaretOffset, 3);
      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveWordRight:',
      );
      expect(
        controller.globalCaretOffset,
        11,
        reason: 'the next rendered word stop skips opening and closing **',
      );
      await _performSelectorAndWait(tester, controller, state, 'moveWordLeft:');
      expect(
        controller.globalCaretOffset,
        4,
        reason: 'the rendered stop before two lands before opening **',
      );
      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveWordRightAndModifySelection:',
      );
      expect(controller.globalSelectionBase, 4);
      expect(controller.globalSelectionExtent, 11);

      await tester.runAsync(() async {
        controller.activateRow(row, 0);
        await controller.resolveCanonicalSelection();
      });
      final generation = controller.canonicalSelectionGeneration;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await _pumpUntil(
        tester,
        () => controller.canonicalSelectionGeneration > generation,
        reason: 'inputEvents=$inputEvents',
      );
      expect(controller.globalCaretOffset, 3);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
      debugDefaultTargetPlatformOverride = null;
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'word navigation does not stop at internal paint fragments',
    (tester) async {
      final source = '${'a' * 600}\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, 10);
        await controller.resolveCanonicalSelection();
      });
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 320,
            height: 400,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      expect(surface.debugPaintedFragmentCount, greaterThan(1));
      final dynamic state = tester.state(find.byType(FlarkEditor));

      await _performSelectorAndWait(
        tester,
        controller,
        state,
        'moveWordRight:',
      );
      expect(controller.globalCaretOffset, 600);

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
      final events = <String>[];

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugInputEventObserver: events.add,
            ),
          ),
        ),
      );
      final dynamic state = tester.state(find.byType(FlarkEditor));

      state.performSelector('copy:');
      await _pumpUntil(tester, () => clipboardText == 'alpha');
      expect(clipboardText, 'alpha');

      state.performSelector('cut:');
      await _pumpUntil(
        tester,
        () => events.any((event) => event.startsWith('completed-cut:')),
      );
      await _pumpUntil(tester, () => controller.visibleSource == ' beta\n');
      await _pumpUntilTransactions(tester, controller);
      expect(clipboardText, 'alpha');
      expect(controller.visibleSource, ' beta\n');

      clipboardText = 'pasted';
      state.performSelector('paste:');
      await _pumpUntil(
        tester,
        () => events.any((event) => event.startsWith('completed-paste:')),
      );
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
    'text drops use the mounted caret and ordinary transaction lane',
    (tester) async {
      const source = 'alpha beta\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      final debugHandle = FlarkEditorDebugHandle();
      await tester.runAsync(() async {
        controller.activateRow(row, 0);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugHandle: debugHandle,
              debugInputEventObserver: events.add,
            ),
          ),
        ),
      );
      await tester.pump();
      final dropOffset = source.indexOf('beta') + 4;
      final geometry = debugHandle.geometryForSourceUtf16(dropOffset)!;
      final target = tester.widget<DragTarget<String>>(
        find.byType(DragTarget<String>),
      );

      target.onAcceptWithDetails!(
        DragTargetDetails<String>(data: '!', offset: geometry.globalPosition),
      );
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, 'alpha beta!\n');
      expect(controller.globalCaretOffset, dropOffset + 1);
      expect(events, contains('drop:text'));
      expect(controller.lastError, isNull);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'text drops abort when their mounted geometry is stale',
    (tester) async {
      const source = 'alpha beta\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      final debugHandle = FlarkEditorDebugHandle();
      await tester.runAsync(() async {
        controller.activateRow(row, 0);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(
              controller: controller,
              debugHandle: debugHandle,
              debugInputEventObserver: events.add,
            ),
          ),
        ),
      );
      await tester.pump();
      final geometry = debugHandle.geometryForSourceUtf16(
        source.indexOf('beta') + 4,
      )!;
      final staleTarget = tester.widget<DragTarget<String>>(
        find.byType(DragTarget<String>),
      );

      controller.replaceSelection('x');
      final admittedSelection = controller.inputValue.selection;

      staleTarget.onAcceptWithDetails!(
        DragTargetDetails<String>(data: '!', offset: geometry.globalPosition),
      );
      expect(controller.inputValue.selection, admittedSelection);
      await _pumpUntilTransactions(tester, controller);

      expect(controller.globalCaretOffset, 1);
      expect(controller.visibleSource, 'xalpha beta\n');
      expect(events, isNot(contains('drop:text')));

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'rich input and private commands cross only configured host callbacks',
    (tester) async {
      final inserted = <KeyboardInsertedContent>[];
      final commands = <(String, Map<String, dynamic>)>[];
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open('alpha\n', libraryPath: libraryPath!),
      ))!;
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditor(
            controller: controller,
            contentInsertionConfiguration: ContentInsertionConfiguration(
              allowedMimeTypes: const ['image/png'],
              onContentInserted: inserted.add,
            ),
            onAppPrivateCommand: (action, data) {
              commands.add((action, data));
            },
          ),
        ),
      );
      final dynamic state = tester.state(find.byType(FlarkEditor));
      const accepted = KeyboardInsertedContent(
        mimeType: 'image/png',
        uri: 'content://accepted',
      );
      const rejected = KeyboardInsertedContent(
        mimeType: 'text/html',
        uri: 'content://rejected',
      );

      state.insertContent(accepted);
      state.insertContent(rejected);
      state.performPrivateCommand('flark.test', <String, dynamic>{'value': 7});
      expect(inserted, [accepted]);
      expect(commands, hasLength(1));
      expect(commands.single.$1, 'flark.test');
      expect(commands.single.$2, {'value': 7});
      expect(controller.visibleSource, 'alpha\n');

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'platform toolbar request shows adaptive bounded selection actions',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          'alpha beta\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.single;
      await tester.runAsync(() async {
        controller.activateRow(row, 0, selectionExtent: 5);
        await controller.resolveCanonicalSelection();
      });
      final events = <String>[];
      await tester.pumpWidget(
        material.MaterialApp(
          home: FlarkEditor(
            controller: controller,
            debugInputEventObserver: events.add,
          ),
        ),
      );
      await tester.pump();
      final dynamic state = tester.state(find.byType(FlarkEditor));

      state.showToolbar();
      await tester.pump();
      expect(
        find.byType(material.AdaptiveTextSelectionToolbar),
        findsOneWidget,
      );
      expect(events, contains('context-menu:show'));

      ContextMenuController.removeAny();
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Mac cancel selector rewinds the active composition scope',
    (tester) async {
      const source = 'base\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      addTearDown(controller.close);
      await tester.runAsync(controller.continueParsing);

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

      state.updateEditingValue(
        const TextEditingValue(
          text: 'kbase\n',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange(start: 0, end: 1),
        ),
      );
      state.updateEditingValue(
        const TextEditingValue(
          text: 'kabase\n',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 0, end: 2),
        ),
      );
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, 'ka$source');

      state.performSelector('cancelOperation:');
      await _pumpUntil(
        tester,
        () =>
            controller.pendingEdits == 0 && controller.visibleSource == source,
      );
      expect(controller.inputValue.composing, TextRange.empty);
      expect(controller.canUndo, isFalse);
      expect(controller.lastError, isNull);
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'input connection survives focus cycling and platform closure',
    (tester) async {
      const source = 'base\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      final editorFocus = FocusNode();
      final otherFocus = FocusNode();
      final inputEvents = <String>[];
      var controllerClosed = false;
      addTearDown(() async {
        if (!controllerClosed) await tester.runAsync(controller.close);
        editorFocus.dispose();
        otherFocus.dispose();
      });
      await tester.runAsync(controller.continueParsing);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Column(
            children: [
              Expanded(
                child: FlarkEditor(
                  controller: controller,
                  autofocus: true,
                  focusNode: editorFocus,
                  debugInputEventObserver: inputEvents.add,
                ),
              ),
              Focus(focusNode: otherFocus, child: const SizedBox(height: 1)),
            ],
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(editorFocus.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);

      otherFocus.requestFocus();
      await tester.pump();
      expect(editorFocus.hasFocus, isFalse);
      expect(tester.testTextInput.hasAnyClients, isFalse);

      editorFocus.requestFocus();
      await tester.pump();
      expect(editorFocus.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(tester.testTextInput.editingState?['text'], source);

      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'xbase\n',
          selection: TextSelection.collapsed(offset: 1),
        ),
      );
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, 'xbase\n');
      expect(controller.lastError, isNull);

      tester.testTextInput.closeConnection();
      await tester.pump();
      expect(
        inputEvents.where((event) => event == 'connection-closed'),
        hasLength(1),
      );
      expect(editorFocus.hasFocus, isFalse);

      editorFocus.requestFocus();
      await tester.pump();
      expect(editorFocus.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(tester.testTextInput.editingState?['text'], 'xbase\n');

      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'xybase\n',
          selection: TextSelection.collapsed(offset: 2),
        ),
      );
      await _pumpUntilTransactions(tester, controller);
      expect(controller.visibleSource, 'xybase\n');
      expect(controller.revision, 3);
      expect(controller.resyncCount, 0);
      expect(controller.lastError, isNull);

      await tester.pump(const Duration(milliseconds: 40));
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
      controllerClosed = true;
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'tapping an attached editor reshows a dismissed platform keyboard',
    (tester) async {
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          'Tap here to keep editing.\n',
          libraryPath: libraryPath!,
        ),
      ))!;

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox.expand(
            child: FlarkEditor(controller: controller, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(tester.testTextInput.isVisible, isTrue);

      tester.testTextInput.hide();
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(tester.testTextInput.isVisible, isFalse);

      final surface = find.byType(FlarkRenderSurfaceWidget);
      final gesture = await tester.startGesture(
        tester.getTopLeft(surface) + const Offset(80, 24),
      );
      await tester.pump(const Duration(milliseconds: 150));

      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(tester.testTextInput.isVisible, isTrue);
      await gesture.cancel();
      await tester.pump();

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'selected-text Backspace delta plus selector deletes exactly once',
    (tester) async {
      const source = 'alpha beta\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      final paints = <FlarkSurfacePaintObservation>[];
      try {
        await tester.runAsync(controller.continueParsing);
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: SizedBox.expand(
              child: FlarkEditor(
                controller: controller,
                autofocus: true,
                debugPaintObserver: paints.add,
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump();
        final dynamic state = tester.state(find.byType(FlarkEditor));
        await tester.runAsync(() async {
          controller.activateRow(controller.rows.single, 10);
          await controller.resolveCanonicalSelection();
        });
        await tester.pump();

        final selected = controller.inputValue.copyWith(
          selection: const TextSelection(
            baseOffset: 6,
            extentOffset: 10,
            isDirectional: true,
          ),
        );
        state.updateEditingValue(selected);
        await tester.pump();
        final revision = controller.revision;
        paints.clear();
        final delta = TextEditingDeltaDeletion(
          oldText: selected.text,
          deletedRange: const TextRange(start: 6, end: 10),
          selection: const TextSelection.collapsed(offset: 6),
          composing: TextRange.empty,
        );
        state.updateEditingValueWithDeltas([delta]);
        state.performSelector('deleteBackward:');
        await _pumpUntilTransactions(tester, controller);
        await tester.runAsync(controller.continueParsing);
        await tester.pump();

        expect(await tester.runAsync(controller.readSource), 'alpha \n');
        expect(controller.revision, revision + 1);
        expect(controller.resyncCount, 0);
        expect(paints, isNotEmpty);
        expect(
          paints.map((paint) => paint.presentation),
          everyElement(anyOf('alpha beta', 'alpha ')),
        );
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(controller.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'projection-collapsed selection types at one rendered caret every frame',
    (tester) async {
      const source = '## Heading\n\n**sentinel**\n';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      final paints = <FlarkSurfacePaintObservation>[];
      try {
        await tester.runAsync(controller.continueParsing);
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: SizedBox.expand(
              child: FlarkEditor(
                controller: controller,
                autofocus: true,
                debugPaintObserver: paints.add,
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump();
        final dynamic state = tester.state(find.byType(FlarkEditor));
        await tester.runAsync(() async {
          controller.activateRow(controller.rows.first, 10);
          await controller.resolveCanonicalSelection();
        });
        await tester.pump();

        final selected = controller.inputValue.copyWith(
          selection: const TextSelection(
            baseOffset: 10,
            extentOffset: 11,
            isDirectional: true,
          ),
        );
        state.updateEditingValue(selected);
        await tester.pump();
        expect(
          controller.inputValue.selection,
          const TextSelection.collapsed(offset: 10),
        );
        paints.clear();
        final before = controller.inputValue;
        final delta = TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '*',
          insertionOffset: 10,
          selection: const TextSelection.collapsed(offset: 11),
          composing: TextRange.empty,
        );
        state.updateEditingValueWithDeltas([delta]);
        await _pumpUntilTransactions(tester, controller);
        await tester.runAsync(controller.continueParsing);
        await tester.pump();

        expect(
          await tester.runAsync(controller.readSource),
          '## Heading*\n\n**sentinel**\n',
        );
        expect(controller.resyncCount, 0);
        expect(controller.lastError, isNull);
        expect(paints, isNotEmpty);
        expect(
          paints.map((paint) => paint.presentation),
          everyElement(anyOf('Heading\nsentinel', 'Heading*\nsentinel')),
        );
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(controller.close);
      }
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'repeated macOS Return callbacks resynchronize an identical blank input window',
    (tester) => _expectRepeatedReturnPlatformLiveness(
      tester,
      libraryPath!,
      useDeltas: true,
    ),
    skip: libraryPath == null,
  );

  testWidgets(
    'repeated full-value Return callbacks resynchronize an identical blank input window',
    (tester) => _expectRepeatedReturnPlatformLiveness(
      tester,
      libraryPath!,
      useDeltas: false,
    ),
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

Future<void> _expectRepeatedReturnPlatformLiveness(
  WidgetTester tester,
  String libraryPath, {
  required bool useDeltas,
}) async {
  final controller = (await tester.runAsync(
    () => FlarkEditorController.open('fff', libraryPath: libraryPath),
  ))!;
  await tester.runAsync(controller.continueParsing);

  final inputEvents = <String>[];

  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: SizedBox.expand(
        child: FlarkEditor(
          controller: controller,
          autofocus: true,
          debugInputEventObserver: inputEvents.add,
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.pump();
  final row = controller.rows.single;
  await tester.runAsync(() async {
    controller.activateRow(row, row.editableUtf16!.end);
    await controller.resolveCanonicalSelection();
  });
  await tester.pump();
  expect(tester.testTextInput.hasAnyClients, isTrue);
  final dynamic state = tester.state(find.byType(FlarkEditor));
  var platformValue = TextEditingValue.fromJSON(
    tester.testTextInput.editingState!,
  );
  const expectedSources = <String>['fff\n\n', 'fff\n\n\n', 'fff\n\n\n\n'];
  const expectedCarets = <int>[5, 6, 7];

  for (var index = 0; index < 3; index += 1) {
    final before = platformValue;
    inputEvents.clear();
    final offset = before.selection.extentOffset;
    final delta = TextEditingDeltaInsertion(
      oldText: before.text,
      textInserted: '\n',
      insertionOffset: offset,
      selection: TextSelection.collapsed(offset: offset + 1),
      composing: TextRange.empty,
    );
    platformValue = delta.apply(before);
    tester.testTextInput.log.clear();
    if (useDeltas) {
      state.updateEditingValueWithDeltas([delta]);
    } else {
      state.updateEditingValue(platformValue);
    }
    state.performAction(TextInputAction.newline);
    await _pumpUntilTransactions(tester, controller);
    await tester.pump();

    final synchronizations = tester.testTextInput.log
        .where((call) => call.method == 'TextInput.setEditingState')
        .toList(growable: false);
    if (controller.inputValue != platformValue) {
      expect(
        synchronizations,
        isNotEmpty,
        reason:
            'Return ${index + 1} left the platform provisional newline '
            'installed even though the controller had adopted a new '
            'authoritative input window',
      );
    }
    if (synchronizations.isNotEmpty) {
      platformValue = TextEditingValue.fromJSON(
        synchronizations.last.arguments as Map<String, dynamic>,
      );
    }
    expect(platformValue, controller.inputValue);
    await tester.runAsync(controller.continueParsing);
    await tester.pump();
    final settledSynchronizations = tester.testTextInput.log
        .where((call) => call.method == 'TextInput.setEditingState')
        .toList(growable: false);
    if (settledSynchronizations.isNotEmpty) {
      platformValue = TextEditingValue.fromJSON(
        settledSynchronizations.last.arguments as Map<String, dynamic>,
      );
    }
    expect(platformValue, controller.inputValue);
    expect(
      await tester.runAsync(controller.readSource),
      expectedSources[index],
      reason:
          'Return ${index + 1} must extend the active paragraph gap once; '
          'before=$before; controller=${controller.inputValue}; '
          'events=$inputEvents',
    );
    final canonicalSelection = await tester.runAsync(
      controller.resolveCanonicalSelection,
    );
    expect(canonicalSelection!.base, expectedCarets[index]);
    expect(canonicalSelection.extent, expectedCarets[index]);
  }

  final sourceBeforeTyping = await tester.runAsync(controller.readSource);
  final caretBeforeTyping = controller.globalSelectionExtent;
  final localCaret = platformValue.selection.extentOffset;
  final insertion = TextEditingDeltaInsertion(
    oldText: platformValue.text,
    textInserted: 'x',
    insertionOffset: localCaret,
    selection: TextSelection.collapsed(offset: localCaret + 1),
    composing: TextRange.empty,
  );
  platformValue = insertion.apply(platformValue);
  if (useDeltas) {
    state.updateEditingValueWithDeltas([insertion]);
  } else {
    state.updateEditingValue(platformValue);
  }
  await _pumpUntilTransactions(tester, controller);
  await tester.pump();

  expect(
    await tester.runAsync(controller.readSource),
    sourceBeforeTyping!.replaceRange(caretBeforeTyping, caretBeforeTyping, 'x'),
  );
  expect(controller.globalSelectionExtent, caretBeforeTyping + 1);
  expect(controller.lastError, isNull);

  await tester.pumpWidget(const SizedBox.shrink());
  await tester.runAsync(controller.close);
}

void _expectListActionPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
  required String operation,
}) {
  _expectCurrentPaints(
    paints,
    expectedSource: expectedSource,
    expectedGeneration: expectedGeneration,
    expectedCaret: expectedCaret,
    operation: operation,
    expectRows: (rows) {
      final active = rows.where((row) => row.active).toList(growable: false);
      expect(active, isNotEmpty, reason: operation);
      expect(active.map((row) => row.ordinal).toSet(), hasLength(1));
      for (final row in active) {
        expect(row.neutral, isFalse, reason: operation);
        expect(row.kind, 5, reason: operation);
        expect(row.listItem, isTrue, reason: operation);
        expect(row.text, contains('child'), reason: operation);
      }
    },
  );
}

void _expectPendingListPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
}) {
  _expectCurrentPaints(
    paints,
    expectedSource: expectedSource,
    expectedGeneration: expectedGeneration,
    expectedCaret: expectedCaret,
    operation: 'pending list action suppression',
    expectRows: (rows) {
      final active = rows.where((row) => row.active).toList(growable: false);
      expect(active, isNotEmpty);
      expect(active.every((row) => !row.neutral), isTrue);
      expect(active.every((row) => row.kind == 5 && row.listItem), isTrue);
    },
  );
}

void _expectTaskTogglePaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
}) {
  _expectCurrentPaints(
    paints,
    expectedSource: expectedSource,
    expectedGeneration: expectedGeneration,
    expectedCaret: expectedCaret,
    operation: 'task toggle',
    expectRows: (rows) {
      final task = rows.where((row) => row.listItem).toList(growable: false);
      expect(task, isNotEmpty);
      expect(task.every((row) => !row.neutral && row.kind == 5), isTrue);
      expect(task.map((row) => row.leadingText).join(), contains('☑'));
    },
  );
}

void _expectPendingTaskPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
}) {
  _expectCurrentPaints(
    paints,
    expectedSource: expectedSource,
    expectedGeneration: expectedGeneration,
    expectedCaret: expectedCaret,
    operation: 'pending task action suppression',
    expectRows: (rows) {
      final active = rows.where((row) => row.active).toList(growable: false);
      expect(active, isNotEmpty);
      expect(active.every((row) => !row.neutral), isTrue);
      expect(active.every((row) => row.kind == 5 && row.listItem), isTrue);
    },
  );
}

void _expectTableNavigationPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
  required String operation,
}) {
  _expectCurrentPaints(
    paints,
    expectedSource: expectedSource,
    expectedGeneration: expectedGeneration,
    expectedCaret: expectedCaret,
    operation: operation,
    expectRows: (rows) {
      final table = rows.where((row) => row.table).toList(growable: false);
      expect(table, isNotEmpty);
      expect(table.every((row) => !row.neutral), isTrue);
    },
  );
}

void _expectCurrentPaints(
  List<FlarkSurfacePaintObservation> paints, {
  required String expectedSource,
  required int expectedGeneration,
  required int expectedCaret,
  required String operation,
  required void Function(List<FlarkSurfacePaintRowObservation>) expectRows,
}) {
  final current = paints
      .where((paint) => paint.sourceGeneration == expectedGeneration)
      .toList(growable: false);
  expect(
    current,
    isNotEmpty,
    reason: '$operation must produce a current paint',
  );
  for (final paint in current) {
    expect(paint.visibleSource, expectedSource, reason: operation);
    expect(paint.canonicalSelectionBaseUtf16, expectedCaret, reason: operation);
    expect(
      paint.canonicalSelectionExtentUtf16,
      expectedCaret,
      reason: operation,
    );
    expect(paint.caretRect, isNotNull, reason: operation);
    expect(paint.caretSourceUtf16, expectedCaret, reason: operation);
    expectRows(paint.rows);
  }
}

Future<void> _pumpUntilTransactions(
  WidgetTester tester,
  FlarkEditorController controller,
) async {
  var settled = false;
  Object? settlementError;
  StackTrace? settlementStackTrace;
  unawaited(
    controller.debugWaitForMutationSettled().then<void>(
      (_) => settled = true,
      onError: (Object error, StackTrace stackTrace) {
        settlementError = error;
        settlementStackTrace = stackTrace;
        settled = true;
      },
    ),
  );
  await _pumpUntil(tester, () => controller.pendingEdits == 0 && settled);
  if (settlementError case final error?) {
    Error.throwWithStackTrace(error, settlementStackTrace!);
  }
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

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() predicate, {
  String? reason,
}) async {
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
  expect(predicate(), isTrue, reason: reason);
}

import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'focus preserves inline projection and hidden-boundary topology',
    () async {
      const source = 'Anchor.\n\nbefore **bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final row = controller.rows.last;
      final passive = controller.surfaceRow(row);
      final boldStart = source.indexOf('bold');
      final boldEnd = boldStart + 'bold'.length;
      final boldDisplayStart = passive.text.indexOf('bold');
      final boldDisplayEnd = boldDisplayStart + 'bold'.length;

      controller.activateRow(row, boldStart + 2);
      final active = controller.surfaceRow(row);
      expect(active.text, passive.text);
      expect(
        active.runs.map((run) => run.text),
        passive.runs.map((run) => run.text),
      );
      expect(
        active.runs.any(
          (run) => run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayStart,
          affinity: TextAffinity.downstream,
        ),
        boldStart,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayStart,
          affinity: TextAffinity.upstream,
        ),
        boldStart - 2,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayEnd,
          affinity: TextAffinity.upstream,
        ),
        boldEnd,
      );
      expect(
        active.sourceOffsetForTextOffset(
          boldDisplayEnd,
          affinity: TextAffinity.downstream,
        ),
        boldEnd + 2,
      );
    },
    skip: libraryPath == null,
  );

  test(
    'Backspace edits visible graphemes and never hidden delimiters',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      final boldStart = source.indexOf('bold');

      controller.activateRow(row, boldStart);
      controller.deleteBackward();
      expect(controller.visibleSource, source);

      controller.activateRow(row, boldStart + 1);
      controller.deleteBackward();
      expect(controller.visibleSource, '**old** after\n');
      await _settle(controller);
      expect(controller.surfaceRow(controller.rows.first).text, 'old after');
    },
    skip: libraryPath == null,
  );

  test(
    'parser-authorized ordinary edits retain projection while pending',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('bold') + 2);
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'x',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.semanticsCurrent, isFalse);
      final pending = controller.surfaceRow(row);
      expect(pending.text, 'boxld after');
      expect(pending.text, isNot(contains('**')));
      expect(
        pending.runs.any(
          (run) =>
              run.text == 'boxld' &&
              run.styles.contains(FlarkSurfaceInlineStyle.strong),
        ),
        isTrue,
      );
      await _settle(controller);
      expect(controller.visibleSource, '**boxld** after\n');
      final settled = controller.surfaceRow(controller.rows.first);
      expect(settled.active, isTrue);
      expect(settled.text, 'boxld after');
    },
    skip: libraryPath == null,
  );

  test(
    'plain-text edits at inline content edges retain projection',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('bold') + 'bold'.length);
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'x',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.surfaceRow(row).text, 'boldx after');
      expect(controller.surfaceRow(row).text, isNot(contains('**')));
    },
    skip: libraryPath == null,
  );

  test(
    'plain heading typing never demotes the visible page between receipts',
    () async {
      const source = '# Heading\n\nPlain paragraph.\n\n## Sibling\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final heading = controller.rows.first;
      final caret = source.indexOf('Heading') + 'Heading'.length;
      controller.activateRow(heading, caret);
      final observed = <({int firstKind, int lastKind, String firstText})>[];
      void capture() {
        final rows = controller.rows;
        if (rows.length < 3) return;
        final first = controller.surfaceRow(rows.first);
        final last = controller.surfaceRow(rows.last);
        observed.add((
          firstKind: first.kind,
          lastKind: last.kind,
          firstText: first.text,
        ));
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      final before = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: 'x',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      capture();
      await _settle(controller);
      expect(observed, isNotEmpty);
      expect(
        observed,
        everyElement(
          isA<({int firstKind, int lastKind, String firstText})>()
              .having((state) => state.firstKind, 'active heading kind', 12)
              .having((state) => state.lastKind, 'sibling heading kind', 12)
              .having(
                (state) => state.firstText,
                'projected active text',
                isNot(contains('#')),
              ),
        ),
      );
      expect(controller.visibleSource, startsWith('# Headingx'));
    },
    skip: libraryPath == null,
  );

  test(
    'plain heading backspace never demotes the visible page between receipts',
    () async {
      const source = '# Heading\n\nPlain paragraph.\n\n## Sibling\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final heading = controller.rows.first;
      controller.activateRow(heading, heading.editableUtf16!.end);
      final observed = <({int firstKind, int lastKind, String firstText})>[];
      void capture() {
        final rows = controller.rows;
        if (rows.length < 3) return;
        observed.add((
          firstKind: controller.surfaceRow(rows.first).kind,
          lastKind: controller.surfaceRow(rows.last).kind,
          firstText: controller.surfaceRow(rows.first).text,
        ));
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.deleteBackward();
      capture();
      await _settle(controller);

      expect(observed, isNotEmpty);
      expect(
        observed,
        everyElement(
          isA<({int firstKind, int lastKind, String firstText})>()
              .having((state) => state.firstKind, 'active heading kind', 12)
              .having((state) => state.lastKind, 'sibling heading kind', 12)
              .having(
                (state) => state.firstText,
                'projected active text',
                isNot(contains('#')),
              ),
        ),
      );
      expect(controller.visibleSource, startsWith('# Headin\n'));
    },
    skip: libraryPath == null,
  );

  test(
    'plain paragraph backspace retains block and sibling presentation',
    () async {
      const source = 'Plain paragraph.\n\n## Sibling\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();

      final paragraph = controller.rows.first;
      controller.activateRow(paragraph, paragraph.editableUtf16!.end);
      final observed = <({int paragraphKind, int siblingKind})>[];
      void capture() {
        final rows = controller.rows;
        if (rows.length < 2) return;
        observed.add((
          paragraphKind: controller.surfaceRow(rows.first).kind,
          siblingKind: controller.surfaceRow(rows.last).kind,
        ));
      }

      controller.addListener(capture);
      addTearDown(() => controller.removeListener(capture));
      controller.deleteBackward();
      capture();
      await _settle(controller);

      expect(observed, isNotEmpty);
      expect(
        observed,
        everyElement(
          isA<({int paragraphKind, int siblingKind})>()
              .having((state) => state.paragraphKind, 'paragraph kind', 5)
              .having((state) => state.siblingKind, 'sibling heading kind', 12),
        ),
      );
      expect(controller.visibleSource, startsWith('Plain paragraph\n'));
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'Return at the dogfood paragraph boundary owns a visible empty block',
    (tester) async {
      const source =
          '''This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

## Start here

1. Click here.
''';
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      addTearDown(controller.close);
      await tester.runAsync(controller.continueParsing);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 900,
            height: 600,
            child: FlarkEditor(controller: controller),
          ),
        ),
      );

      final boundary = source.indexOf('\n\n##');
      final paragraph = controller.rows.firstWhere(
        (row) => row.kind == 5 && row.editableUtf16!.end == boundary,
      );
      controller.activateRow(paragraph, boundary);
      await tester.pump();
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump();
      controller.insertNewline();
      expect(controller.visibleSource, source);
      expect(controller.pendingEdits, 1);
      await tester.pump();
      for (var turn = 0; turn < 4 && controller.pendingEdits != 0; turn += 1) {
        await tester.pump();
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 10)),
        );
        await tester.pump();
      }
      expect(controller.pendingEdits, 0);
      expect(controller.lastError, isNull);
      await tester.pump();

      RenderFlarkSurface surface() =>
          tester.renderObject(find.byType(FlarkRenderSurfaceWidget));
      expect(controller.rows.map((row) => controller.surfaceRow(row).kind), [
        5,
        12,
        5,
      ]);
      var emptyBlock = surface().debugPaintedPlan.singleWhere(
        (entry) =>
            entry.neutral && entry.sourceStart == controller.globalCaretOffset,
      );
      expect(emptyBlock.text, '\n');
      expect(emptyBlock.active, isTrue);

      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      emptyBlock = surface().debugPaintedPlan.singleWhere(
        (entry) =>
            entry.neutral && entry.sourceStart == controller.globalCaretOffset,
      );
      expect(emptyBlock.text, '\n');
      expect(emptyBlock.active, isTrue);
      expect(controller.inputValue.text, '\n');
      expect(controller.inputValue.selection.extentOffset, 0);

      final beforeTyping = controller.inputValue;
      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: beforeTyping.text,
          textInserted: 'x',
          insertionOffset: beforeTyping.selection.extentOffset,
          selection: const TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      expect(controller.rows.map((row) => controller.surfaceRow(row).kind), [
        5,
        12,
        5,
      ]);
      final pendingTextBlock = surface().debugPaintedPlan.singleWhere(
        (entry) => entry.neutral && entry.active,
      );
      expect(pendingTextBlock.text, 'x\n');

      for (var turn = 0; turn < 4 && controller.pendingEdits != 0; turn += 1) {
        await tester.pump();
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 10)),
        );
        await tester.pump();
      }
      expect(controller.pendingEdits, 0);
      await tester.runAsync(controller.continueParsing);
      await tester.pump();
      expect(
        controller.rows.any(
          (row) =>
              controller.surfaceRow(row).active &&
              controller.surfaceRow(row).text == 'x',
        ),
        isTrue,
      );
      expect(
        controller.surfaceRow(controller.rows.first).text,
        isNot(contains('**')),
      );
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  test(
    'syntax-shaped edits fall back to exact local source',
    () async {
      const source = '**bold** after\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('bold') + 2);
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '*',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.surfaceRow(row).text, contains('**'));
      expect(controller.surfaceRow(row).text, contains('bo*ld'));
    },
    skip: libraryPath == null,
  );

  test(
    'completed inline syntax projects after parser certification',
    () async {
      const source = '*bold\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      final row = controller.rows.first;
      controller.activateRow(row, source.indexOf('\n'));
      final before = controller.inputValue;

      controller.applyDeltas([
        TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '*',
          insertionOffset: before.selection.extentOffset,
          selection: TextSelection.collapsed(
            offset: before.selection.extentOffset + 1,
          ),
          composing: TextRange.empty,
        ),
      ]);

      expect(controller.surfaceRow(row).text, contains('*bold*'));
      await _settle(controller);
      expect(controller.surfaceRow(controller.rows.first).text, 'bold');
    },
    skip: libraryPath == null,
  );

  test(
    'platform selections inside hidden markers normalize to legal stops',
    () async {
      const source = '**bold**\n';
      final controller = await FlarkEditorController.open(
        source,
        libraryPath: libraryPath!,
      );
      addTearDown(controller.close);
      await controller.continueParsing();
      controller.activateRow(controller.rows.first, 2);

      controller.updateEditingValue(
        controller.inputValue.copyWith(
          selection: const TextSelection.collapsed(offset: 1),
        ),
      );
      expect(controller.inputValue.selection.extentOffset, 2);
      expect(controller.globalCaretOffset, 2);
    },
    skip: libraryPath == null,
  );

  testWidgets('editor and read-only view share one render plan', (
    tester,
  ) async {
    const source = '# Heading\n\nbefore **bold** after\n';
    final controller = (await tester.runAsync(
      () => FlarkEditorController.open(source, libraryPath: libraryPath!),
    ))!;
    addTearDown(controller.close);
    await tester.runAsync(controller.continueParsing);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Row(
          children: [
            Expanded(child: FlarkEditor(controller: controller)),
            Expanded(child: FlarkMarkdownView(controller: controller)),
          ],
        ),
      ),
    );
    await tester.pump();

    final surfaces = tester
        .renderObjectList<RenderFlarkSurface>(
          find.byType(FlarkRenderSurfaceWidget),
        )
        .toList();
    expect(surfaces, hasLength(2));
    expect(surfaces[0].debugRenderPlanHash, surfaces[1].debugRenderPlanHash);
    expect(find.byType(EditableText), findsNothing);
  }, skip: libraryPath == null);

  testWidgets(
    'trackpad scrolling never changes the canonical selection',
    (tester) async {
      final source = List<String>.generate(
        100,
        (index) => 'Paragraph $index with enough text.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      addTearDown(controller.close);
      await tester.runAsync(controller.continueParsing);
      controller.activateRow(controller.rows.first, 4);
      final selectionBefore = controller.inputValue.selection;
      final globalCaretBefore = controller.globalCaretOffset;

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
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );

      await tester.trackpadFling(
        find.byType(FlarkEditor),
        const Offset(0, -300),
        1200,
      );
      await tester.pump();

      expect(surface.scrollOffset, greaterThan(0));
      expect(controller.inputValue.selection, selectionBefore);
      expect(controller.globalCaretOffset, globalCaretBefore);
    },
    skip: libraryPath == null,
  );
}

Future<void> _settle(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 2));
  }
  await controller.continueParsing();
}

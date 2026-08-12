import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

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
